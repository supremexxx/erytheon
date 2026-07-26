//! NASA FIRMS VIIRS S-NPP connector.

use std::{path::PathBuf, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Days, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Cadence, FetchCtx, Observation, ObservationKind, Source, SourceError};

const API_BASE_URL: &str = "https://firms.modaps.eosdis.nasa.gov/api/area/csv";
const PRODUCT: &str = "VIIRS_SNPP_NRT";
const MAX_API_DAY_RANGE: u16 = 5;
const POLL_INTERVAL: Duration = Duration::from_secs(30 * 60);

/// One raw FIRMS row and its optional normalized V1 observation.
#[derive(Clone, Debug)]
pub struct FirmsRow {
    /// Stable identity inside one retrieval, equal to the historical dedupe key when valid.
    pub source_record_id: String,
    /// Full source row represented with original CSV field names and string values.
    pub raw_payload: Value,
    /// Parsed UTC acquisition timestamp when normalization succeeded.
    pub observed_at: Option<DateTime<Utc>>,
    /// Source product version when available.
    pub source_version: Option<String>,
    /// Historical V1 observation, absent when the row cannot be normalized.
    pub observation: Option<Observation>,
    /// Row-level parsing or normalization error.
    pub parsing_error: Option<String>,
}

/// Complete result of one logical FIRMS retrieval.
#[derive(Clone, Debug, Default)]
pub struct FirmsFetch {
    /// Every syntactically readable source row, including rejected rows.
    pub rows: Vec<FirmsRow>,
    /// Number of response documents fetched from the API or fixture.
    pub documents: usize,
}

impl FirmsFetch {
    /// Number of source rows received.
    #[must_use]
    pub fn received(&self) -> usize {
        self.rows.len()
    }

    /// Number of rows normalized for the historical public contract.
    #[must_use]
    pub fn accepted(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.observation.is_some())
            .count()
    }

    /// Number of rows retained with a parsing error.
    #[must_use]
    pub fn rejected(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| row.parsing_error.is_some())
            .count()
    }

    /// Clones the normalized observations for V1 persistence or export.
    #[must_use]
    pub fn observations(&self) -> Vec<Observation> {
        self.rows
            .iter()
            .filter_map(|row| row.observation.clone())
            .collect()
    }
}

/// NASA FIRMS active-fire source.
#[derive(Clone, Debug)]
pub struct FirmsSource {
    fixture_path: PathBuf,
}

impl FirmsSource {
    /// Creates a connector with a local fallback fixture.
    #[must_use]
    pub fn new(fixture_path: impl Into<PathBuf>) -> Self {
        Self {
            fixture_path: fixture_path.into(),
        }
    }

    /// Fetches FIRMS while retaining each original CSV row for raw persistence.
    ///
    /// # Errors
    ///
    /// Returns an error when the request, fixture, or CSV header cannot be read.
    pub async fn fetch_batch(&self, ctx: &FetchCtx) -> Result<FirmsFetch, SourceError> {
        if ctx.days == 0 {
            return Err(SourceError::InvalidDayCount);
        }
        let documents = self.fetch_csv_documents(ctx).await?;
        let mut fetch = FirmsFetch {
            rows: Vec::new(),
            documents: documents.len(),
        };
        for document in documents {
            fetch.rows.extend(parse_csv_rows(&document, ctx)?);
        }
        tracing::info!(
            received = fetch.received(),
            accepted = fetch.accepted(),
            rejected = fetch.rejected(),
            "FIRMS fetch complete"
        );
        Ok(fetch)
    }

    async fn fetch_csv_documents(&self, ctx: &FetchCtx) -> Result<Vec<String>, SourceError> {
        let Some(map_key) = ctx.firms_map_key.as_deref() else {
            tracing::info!(path = %self.fixture_path.display(), "FIRMS map key absent; using fixture");
            let content = tokio::fs::read_to_string(&self.fixture_path)
                .await
                .map_err(|source| SourceError::FixtureRead {
                    path: self.fixture_path.clone(),
                    source,
                })?;
            return Ok(vec![content]);
        };

        let windows = request_windows(ctx.days, ctx.end_date)?;
        let mut documents = Vec::with_capacity(windows.len());
        for window in windows {
            let url = format!(
                "{API_BASE_URL}/{map_key}/{PRODUCT}/{}/{}/{}",
                ctx.aoi.api_coordinates(),
                window.days,
                window.start
            );
            let response = ctx.client.get(url).send().await?.error_for_status()?;
            documents.push(response.text().await?);
        }
        Ok(documents)
    }
}

#[async_trait]
impl Source for FirmsSource {
    fn id(&self) -> &'static str {
        "firms"
    }

    fn cadence(&self) -> Cadence {
        Cadence::Poll(POLL_INTERVAL)
    }

    #[tracing::instrument(skip_all, fields(source = self.id(), days = ctx.days))]
    async fn fetch(&self, ctx: &FetchCtx) -> Result<Vec<Observation>, SourceError> {
        let fetch = self.fetch_batch(ctx).await?;
        if fetch.rejected() > 0 {
            return Err(SourceError::InvalidFirmsRows(fetch.rejected()));
        }
        Ok(fetch.observations())
    }
}

fn parse_csv_rows(document: &str, ctx: &FetchCtx) -> Result<Vec<FirmsRow>, SourceError> {
    if document.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut reader = csv::Reader::from_reader(document.as_bytes());
    let headers = reader.headers()?.clone();
    let mut rows = Vec::new();
    for (index, record) in reader.records().enumerate() {
        let row_number = index + 2;
        let record = record?;
        let raw_payload = raw_payload(&headers, &record);
        match record.deserialize::<FirmsRecord>(Some(&headers)) {
            Ok(parsed) => {
                let source_version = Some(parsed.version.clone());
                match normalize(parsed, ctx) {
                    Ok(observation) => rows.push(FirmsRow {
                        source_record_id: observation.dedupe_key.clone(),
                        raw_payload,
                        observed_at: Some(observation.observed_at),
                        source_version,
                        observation: Some(observation),
                        parsing_error: None,
                    }),
                    Err(error) => rows.push(rejected_row(
                        row_number,
                        &record,
                        raw_payload,
                        error.to_string(),
                    )),
                }
            }
            Err(error) => rows.push(rejected_row(
                row_number,
                &record,
                raw_payload,
                error.to_string(),
            )),
        }
    }
    Ok(rows)
}

fn raw_payload(headers: &csv::StringRecord, record: &csv::StringRecord) -> Value {
    let fields = headers
        .iter()
        .zip(record.iter())
        .map(|(header, value)| (header.to_owned(), Value::String(value.to_owned())))
        .collect::<Map<_, _>>();
    Value::Object(fields)
}

fn rejected_row(
    row_number: usize,
    _record: &csv::StringRecord,
    raw_payload: Value,
    error: String,
) -> FirmsRow {
    FirmsRow {
        source_record_id: format!("rejected:{row_number}"),
        raw_payload,
        observed_at: None,
        source_version: None,
        observation: None,
        parsing_error: Some(error),
    }
}

fn normalize(record: FirmsRecord, ctx: &FetchCtx) -> Result<Observation, SourceError> {
    let acquisition_time = format!("{:04}", record.acq_time);
    let time = NaiveTime::parse_from_str(&acquisition_time, "%H%M").map_err(|_| {
        SourceError::InvalidTimestamp {
            value: format!("{} {}", record.acq_date, record.acq_time),
        }
    })?;
    let observed_at =
        DateTime::<Utc>::from_naive_utc_and_offset(NaiveDateTime::new(record.acq_date, time), Utc);
    let cell = ctx.grid.cell_for_point(record.latitude, record.longitude)?;
    let dedupe_key = format!(
        "{}:{}:{}:{:.5}:{:.5}:{}",
        record.satellite,
        record.acq_date,
        acquisition_time,
        record.latitude,
        record.longitude,
        record.version
    );
    let payload = serde_json::to_value(FirmsPayload::from(record))?;

    Ok(Observation {
        source: "firms".to_owned(),
        kind: ObservationKind::ActiveFire,
        cell,
        observed_at,
        payload,
        dedupe_key,
    })
}

#[derive(Clone, Debug, Deserialize)]
struct FirmsRecord {
    latitude: f64,
    longitude: f64,
    bright_ti4: f64,
    scan: f64,
    track: f64,
    acq_date: NaiveDate,
    acq_time: u16,
    satellite: String,
    instrument: String,
    confidence: String,
    version: String,
    bright_ti5: f64,
    frp: f64,
    daynight: String,
}

#[derive(Debug, Serialize)]
struct FirmsPayload {
    latitude: f64,
    longitude: f64,
    bright_ti4: f64,
    scan: f64,
    track: f64,
    satellite: String,
    instrument: String,
    confidence: String,
    version: String,
    bright_ti5: f64,
    frp: f64,
    daynight: String,
}

impl From<FirmsRecord> for FirmsPayload {
    fn from(record: FirmsRecord) -> Self {
        Self {
            latitude: record.latitude,
            longitude: record.longitude,
            bright_ti4: record.bright_ti4,
            scan: record.scan,
            track: record.track,
            satellite: record.satellite,
            instrument: record.instrument,
            confidence: record.confidence,
            version: record.version,
            bright_ti5: record.bright_ti5,
            frp: record.frp,
            daynight: record.daynight,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RequestWindow {
    start: NaiveDate,
    days: u16,
}

fn request_windows(days: u16, end_date: NaiveDate) -> Result<Vec<RequestWindow>, SourceError> {
    if days == 0 {
        return Err(SourceError::InvalidDayCount);
    }
    let start_date = end_date
        .checked_sub_days(Days::new(u64::from(days - 1)))
        .ok_or_else(|| SourceError::InvalidTimestamp {
            value: end_date.to_string(),
        })?;
    let mut remaining = days;
    let mut start = start_date;
    let mut windows = Vec::new();
    while remaining > 0 {
        let window_days = remaining.min(MAX_API_DAY_RANGE);
        windows.push(RequestWindow {
            start,
            days: window_days,
        });
        start = start
            .checked_add_days(Days::new(u64::from(window_days)))
            .ok_or_else(|| SourceError::InvalidTimestamp {
                value: start.to_string(),
            })?;
        remaining -= window_days;
    }
    Ok(windows)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use grid::{BoundingBox, H3Grid};

    use crate::FetchCtx;

    use super::{RequestWindow, parse_csv_rows, request_windows};

    const HEADER: &str = "latitude,longitude,bright_ti4,scan,track,acq_date,acq_time,satellite,instrument,confidence,version,bright_ti5,frp,daynight";
    const VALID_ROW: &str =
        "43.43767,4.89077,331.33,0.39,0.36,2023-07-12,134,N,VIIRS,n,2.0NRT,292.82,4.07,N";

    #[test]
    fn splits_seven_days_into_supported_windows() {
        let end = NaiveDate::from_ymd_opt(2026, 7, 16).expect("valid date");
        let windows = request_windows(7, end).expect("valid day count");

        assert_eq!(
            windows,
            vec![
                RequestWindow {
                    start: NaiveDate::from_ymd_opt(2026, 7, 10).expect("valid date"),
                    days: 5,
                },
                RequestWindow {
                    start: NaiveDate::from_ymd_opt(2026, 7, 15).expect("valid date"),
                    days: 2,
                },
            ]
        );
    }

    #[test]
    fn preserves_raw_fields_and_historical_normalization() {
        let rows = parse_csv_rows(&format!("{HEADER}\n{VALID_ROW}\n"), &context())
            .expect("valid FIRMS row");
        let row = &rows[0];
        let observation = row.observation.as_ref().expect("normalized observation");

        assert_eq!(row.raw_payload["acq_time"], "134");
        assert_eq!(row.raw_payload["acq_date"], "2023-07-12");
        assert_eq!(row.raw_payload["satellite"], "N");
        assert_eq!(row.source_version.as_deref(), Some("2.0NRT"));
        assert_eq!(
            row.source_record_id,
            "N:2023-07-12:0134:43.43767:4.89077:2.0NRT"
        );
        assert_eq!(observation.source, "firms");
        assert_eq!(observation.kind.as_str(), "active_fire");
        assert_eq!(
            observation.observed_at.to_rfc3339(),
            "2023-07-12T01:34:00+00:00"
        );
        assert_eq!(observation.payload["latitude"], 43.43767);
        assert!(row.raw_payload.get("map_key").is_none());
    }

    #[test]
    fn retains_an_invalid_row_without_normalizing_it() {
        let invalid = VALID_ROW.replacen("43.43767", "invalid", 1);
        let rows = parse_csv_rows(&format!("{HEADER}\n{invalid}\n"), &context())
            .expect("readable CSV row");
        let row = &rows[0];

        assert_eq!(row.raw_payload["latitude"], "invalid");
        assert!(row.observation.is_none());
        assert!(row.parsing_error.is_some());
        assert_eq!(row.source_record_id, "rejected:2");
    }

    #[test]
    fn accepts_an_empty_response() {
        assert!(
            parse_csv_rows("", &context())
                .expect("empty response is valid")
                .is_empty()
        );
    }

    fn context() -> FetchCtx {
        FetchCtx {
            client: reqwest::Client::new(),
            aoi: BoundingBox::new(4.8, 43.3, 5.0, 43.6).expect("valid bbox"),
            grid: H3Grid::new(9).expect("valid grid"),
            days: 1,
            end_date: NaiveDate::from_ymd_opt(2023, 7, 12).expect("valid date"),
            firms_map_key: None,
            meteofrance_api_key: None,
        }
    }
}
