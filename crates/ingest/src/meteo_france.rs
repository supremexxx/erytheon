//! Météo-France SYNOP weather-station connector.

use std::{path::PathBuf, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Days, Utc};
use serde::{Deserialize, Serialize};

use crate::{Cadence, FetchCtx, Observation, ObservationKind, Source, SourceError};

const API_URL: &str = "https://public-api.meteofrance.fr/public/DPObs/v1/synop?format=csv";
const POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);
const KELVIN_OFFSET: f64 = 273.15;
const METRES_PER_SECOND_TO_KILOMETRES_PER_HOUR: f64 = 3.6;

/// Météo-France SYNOP source with a local official-format fixture fallback.
#[derive(Clone, Debug)]
pub struct MeteoFranceSource {
    fixture_path: PathBuf,
}

impl MeteoFranceSource {
    /// Creates a connector with a local fallback fixture.
    #[must_use]
    pub fn new(fixture_path: impl Into<PathBuf>) -> Self {
        Self {
            fixture_path: fixture_path.into(),
        }
    }

    async fn fetch_csv(&self, ctx: &FetchCtx) -> Result<String, SourceError> {
        let Some(token) = ctx.meteofrance_api_key.as_deref() else {
            tracing::info!(
                path = %self.fixture_path.display(),
                "Météo-France token absent; using fixture"
            );
            return tokio::fs::read_to_string(&self.fixture_path)
                .await
                .map_err(|source| SourceError::FixtureRead {
                    path: self.fixture_path.clone(),
                    source,
                });
        };

        Ok(ctx
            .client
            .get(API_URL)
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }
}

#[async_trait]
impl Source for MeteoFranceSource {
    fn id(&self) -> &'static str {
        "meteofrance_synop"
    }

    fn cadence(&self) -> Cadence {
        Cadence::Poll(POLL_INTERVAL)
    }

    #[tracing::instrument(skip_all, fields(source = self.id(), days = ctx.days))]
    async fn fetch(&self, ctx: &FetchCtx) -> Result<Vec<Observation>, SourceError> {
        if ctx.days == 0 {
            return Err(SourceError::InvalidDayCount);
        }
        let document = self.fetch_csv(ctx).await?;
        let first_date = ctx
            .end_date
            .checked_sub_days(Days::new(u64::from(ctx.days - 1)))
            .ok_or_else(|| SourceError::InvalidTimestamp {
                value: ctx.end_date.to_string(),
            })?;
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b';')
            .from_reader(document.as_bytes());
        let mut observations = Vec::new();
        let mut incomplete = 0_u64;

        for row in reader.deserialize::<SynopRecord>() {
            let record = row?;
            let date = record.validity_time.date_naive();
            if date < first_date || date > ctx.end_date {
                continue;
            }
            let Some(observation) = normalize(record, ctx)? else {
                incomplete += 1;
                continue;
            };
            observations.push(observation);
        }

        tracing::info!(
            observations = observations.len(),
            incomplete,
            "Météo-France SYNOP fetch complete"
        );
        Ok(observations)
    }
}

fn normalize(record: SynopRecord, ctx: &FetchCtx) -> Result<Option<Observation>, SourceError> {
    let (
        Some(temperature_kelvin),
        Some(relative_humidity_pct),
        Some(wind_speed_metres_second),
        Some(precipitation_24h),
    ) = (
        record.temperature,
        record.relative_humidity,
        record.wind_speed,
        record.precipitation_24h,
    )
    else {
        return Ok(None);
    };
    let payload = WeatherStationObservation {
        station_id: record.station_id,
        station_name: record.station_name,
        latitude: record.latitude,
        longitude: record.longitude,
        temperature_c: temperature_kelvin - KELVIN_OFFSET,
        relative_humidity_pct,
        wind_speed_kmh: wind_speed_metres_second * METRES_PER_SECOND_TO_KILOMETRES_PER_HOUR,
        precipitation_24h_mm: precipitation_24h.max(0.0),
    };
    let cell = ctx
        .grid
        .cell_for_point(payload.latitude, payload.longitude)?;
    let dedupe_key = format!("{}:{}", payload.station_id, record.validity_time);

    Ok(Some(Observation {
        source: "meteofrance_synop".to_owned(),
        kind: ObservationKind::WeatherObs,
        cell,
        observed_at: record.validity_time,
        payload: serde_json::to_value(payload)?,
        dedupe_key,
    }))
}

#[derive(Debug, Deserialize)]
struct SynopRecord {
    #[serde(rename = "lat")]
    latitude: f64,
    #[serde(rename = "lon")]
    longitude: f64,
    #[serde(rename = "geo_id_wmo")]
    station_id: String,
    #[serde(rename = "name")]
    station_name: String,
    #[serde(rename = "validity_time")]
    validity_time: DateTime<Utc>,
    #[serde(rename = "ff")]
    wind_speed: Option<f64>,
    #[serde(rename = "t")]
    temperature: Option<f64>,
    #[serde(rename = "u")]
    relative_humidity: Option<f64>,
    #[serde(rename = "rr24")]
    precipitation_24h: Option<f64>,
}

/// Weather fields retained from a normalized SYNOP station observation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct WeatherStationObservation {
    /// WMO station identifier.
    pub station_id: String,
    /// Human-readable station name.
    pub station_name: String,
    /// WGS84 latitude in degrees.
    pub latitude: f64,
    /// WGS84 longitude in degrees.
    pub longitude: f64,
    /// Screen-level temperature in degrees Celsius.
    pub temperature_c: f64,
    /// Relative humidity in percent.
    pub relative_humidity_pct: f64,
    /// Wind speed in kilometres per hour.
    pub wind_speed_kmh: f64,
    /// Accumulated precipitation over 24 hours in millimetres.
    pub precipitation_24h_mm: f64,
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use grid::{BoundingBox, H3Grid};

    use super::{MeteoFranceSource, WeatherStationObservation};
    use crate::{FetchCtx, Source};

    #[tokio::test]
    async fn loads_and_converts_the_official_fixture() {
        let source = MeteoFranceSource::new("../../testdata/meteo_france_synop.csv");
        let context = FetchCtx {
            client: reqwest::Client::new(),
            aoi: BoundingBox::new(1.68, 42.57, 3.26, 43.46).expect("valid AOI"),
            grid: H3Grid::new(9).expect("valid grid"),
            days: 1,
            end_date: NaiveDate::from_ymd_opt(2025, 7, 16).expect("valid date"),
            firms_map_key: None,
            meteofrance_api_key: None,
        };

        let observations = source.fetch(&context).await.expect("valid fixture");
        assert_eq!(observations.len(), 4);
        let weather: WeatherStationObservation =
            serde_json::from_value(observations[0].payload.clone()).expect("typed payload");
        assert!((weather.temperature_c - 32.0).abs() < 0.001);
        assert!((weather.wind_speed_kmh - 21.6).abs() < 0.001);
    }
}
