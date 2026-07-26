//! Lossless parsing and deterministic normalization of normalized BDIFF CSV exports.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use grid::{CellIndex, H3Grid};
use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;

/// Version of the deterministic BDIFF row normalizer.
pub const NORMALIZER_VERSION: &str = "bdiff_normalizer_v1";
/// Version of the minimal grouped-cause taxonomy.
pub const TAXONOMY_VERSION: &str = "bdiff_cause_v1";

/// One source row with both its lossless payload and normalized representation.
#[derive(Clone, Debug)]
pub struct BdiffRow {
    /// One-based CSV source line, including the header.
    pub source_line_number: u64,
    /// Stable source identifier, absent when validation rejects it.
    pub source_record_id: Option<String>,
    /// Every source column retained as string-valued JSON.
    pub raw_payload: Value,
    /// Deterministically normalized values, including rejection details.
    pub normalized: BdiffNormalizedEvent,
}

/// Deterministic normalized values derived from one BDIFF source row.
#[derive(Clone, Debug)]
pub struct BdiffNormalizedEvent {
    pub occurred_at: Option<DateTime<Utc>>,
    pub municipality_source: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub surface_ha: Option<f64>,
    pub cause_source: Option<String>,
    pub cause_category: Option<&'static str>,
    pub cause_subcategory: Option<&'static str>,
    pub cell: Option<CellIndex>,
    pub validation_errors: Vec<&'static str>,
}

impl BdiffNormalizedEvent {
    /// Returns whether the row satisfies all Phase 3B.1 validation rules.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.validation_errors.is_empty()
    }

    /// Stable aggregate safe to store without exposing a full source payload.
    #[must_use]
    pub fn parsing_error(&self) -> Option<String> {
        (!self.validation_errors.is_empty()).then(|| self.validation_errors.join(","))
    }
}

/// Complete decoded CSV file.
#[derive(Clone, Debug)]
pub struct BdiffDocument {
    pub rows: Vec<BdiffRow>,
}

/// Reads a normalized BDIFF CSV while retaining rejected source rows.
///
/// # Errors
///
/// Returns an error when the file cannot be read or its CSV structure cannot be decoded.
pub async fn read_file(path: &Path, grid: H3Grid) -> Result<BdiffDocument, BdiffReadError> {
    let document = tokio::fs::read(path)
        .await
        .map_err(|source| BdiffReadError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    parse_csv(&document, grid)
}

/// Decodes one normalized BDIFF CSV document.
///
/// # Errors
///
/// Returns an error for invalid UTF-8, missing headers, or structurally malformed CSV.
pub fn parse_csv(document: &[u8], grid: H3Grid) -> Result<BdiffDocument, BdiffReadError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(document);
    let headers = reader.headers()?.clone();
    if headers.is_empty() {
        return Err(BdiffReadError::MissingHeader);
    }

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        let source_line_number = record
            .position()
            .map_or_else(|| rows.len() as u64 + 2, csv::Position::line);
        rows.push(normalize_record(
            &headers,
            &record,
            source_line_number,
            grid,
        ));
    }
    Ok(BdiffDocument { rows })
}

fn normalize_record(
    headers: &csv::StringRecord,
    record: &csv::StringRecord,
    source_line_number: u64,
    grid: H3Grid,
) -> BdiffRow {
    let payload = headers
        .iter()
        .enumerate()
        .map(|(index, header)| {
            (
                header.to_owned(),
                Value::String(record.get(index).unwrap_or_default().to_owned()),
            )
        })
        .collect::<Map<_, _>>();
    let value = |name: &str| {
        headers
            .iter()
            .position(|header| header == name)
            .and_then(|index| record.get(index))
            .unwrap_or_default()
            .trim()
    };

    let source_record_id = non_blank(value("external_id"));
    let occurred_at = parse_timestamp(value("occurred_at"));
    let municipality_source = non_blank(value("municipality"));
    let latitude = parse_number(value("latitude"));
    let longitude = parse_number(value("longitude"));
    let surface_ha = parse_number(value("surface_ha"));
    let cause_source = non_blank(value("cause"));
    let mut validation_errors = Vec::new();

    if source_record_id.is_none() {
        validation_errors.push("missing_source_record_id");
    }
    if occurred_at.is_none() {
        validation_errors.push("invalid_timestamp");
    }
    match latitude {
        Some(value) if (-90.0..=90.0).contains(&value) => {}
        _ => validation_errors.push("invalid_latitude"),
    }
    match longitude {
        Some(value) if (-180.0..=180.0).contains(&value) => {}
        _ => validation_errors.push("invalid_longitude"),
    }
    match surface_ha {
        Some(value) if value >= 0.0 => {}
        _ => validation_errors.push("invalid_surface_ha"),
    }
    if municipality_source.is_none() {
        validation_errors.push("missing_municipality");
    }
    if cause_source.is_none() {
        validation_errors.push("missing_cause");
    }

    let (cause_category, cause_subcategory) = cause_source
        .as_deref()
        .map(map_cause)
        .map_or((None, None), |(category, subcategory)| {
            (Some(category), Some(subcategory))
        });
    let cell = match (latitude, longitude) {
        (Some(latitude), Some(longitude))
            if (-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude) =>
        {
            if let Ok(cell) = grid.cell_for_point(latitude, longitude) {
                Some(cell)
            } else {
                if !validation_errors.contains(&"invalid_coordinate") {
                    validation_errors.push("invalid_coordinate");
                }
                None
            }
        }
        _ => None,
    };

    BdiffRow {
        source_line_number,
        source_record_id,
        raw_payload: Value::Object(payload),
        normalized: BdiffNormalizedEvent {
            occurred_at,
            municipality_source,
            latitude,
            longitude,
            surface_ha,
            cause_source,
            cause_category,
            cause_subcategory,
            cell,
            validation_errors,
        },
    }
}

fn non_blank(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc))
}

fn parse_number(value: &str) -> Option<f64> {
    value
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
}

fn normalized_label(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn map_cause(value: &str) -> (&'static str, &'static str) {
    match normalized_label(value).as_str() {
        "malveillance" => ("human_known", "malicious"),
        "involontaire (particulier)" => ("human_known", "private_activity_negligence"),
        "involontaire (travaux)" => ("human_known", "work_activity"),
        "accidentelle" => ("human_known", "accident_unspecified"),
        "naturelle" => ("natural_known", "natural_unspecified"),
        "inconnue" => ("unknown", "unknown_unspecified"),
        _ => ("indeterminate", "unmapped"),
    }
}

/// BDIFF file decoding failures.
#[derive(Debug, thiserror::Error)]
pub enum BdiffReadError {
    #[error("failed to read BDIFF file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("BDIFF CSV header is missing")]
    MissingHeader,
    #[error("invalid BDIFF CSV: {0}")]
    Csv(#[from] csv::Error),
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use grid::{BoundingBox, H3Grid};

    use crate::{FetchCtx, Source, fire_history::FireHistorySource};

    use super::{TAXONOMY_VERSION, map_cause, parse_csv, read_file};

    #[test]
    fn parses_payload_maps_causes_and_rejects_invalid_values() {
        let csv =
            b"external_id,occurred_at,municipality,latitude,longitude,surface_ha,cause,extra\n\
event-1,2025-07-01T12:30:00+02:00,Nimes,43.8367,4.3601,1.5,Malveillance,retained\n\
,bad,,91,181,-1,,also retained\n";
        let document = parse_csv(csv, H3Grid::new(8).expect("grid")).expect("CSV should parse");
        assert_eq!(document.rows.len(), 2);
        let valid = &document.rows[0];
        assert_eq!(valid.source_line_number, 2);
        assert_eq!(valid.raw_payload["extra"], "retained");
        assert!(valid.normalized.is_valid());
        assert_eq!(valid.normalized.cause_category, Some("human_known"));
        assert_eq!(valid.normalized.cause_subcategory, Some("malicious"));
        assert!(valid.normalized.cell.is_some());

        let rejected = &document.rows[1];
        assert!(!rejected.normalized.is_valid());
        assert_eq!(
            rejected.normalized.parsing_error().as_deref(),
            Some(
                "missing_source_record_id,invalid_timestamp,invalid_latitude,invalid_longitude,\
invalid_surface_ha,missing_municipality,missing_cause"
            )
        );
        assert_eq!(rejected.raw_payload["extra"], "also retained");
        assert_eq!(TAXONOMY_VERSION, "bdiff_cause_v1");
    }

    #[test]
    fn normalizes_unicode_and_keeps_unmapped_causes_indeterminate() {
        assert_eq!(
            map_cause("  Involontaire\u{00a0}(travaux)  "),
            ("human_known", "work_activity")
        );
        assert_eq!(map_cause("Cause nouvelle"), ("indeterminate", "unmapped"));
    }

    #[test]
    fn maps_the_six_documented_causes_without_merging_unknowns() {
        assert_eq!(map_cause("Malveillance"), ("human_known", "malicious"));
        assert_eq!(
            map_cause("Involontaire (particulier)"),
            ("human_known", "private_activity_negligence")
        );
        assert_eq!(
            map_cause("Involontaire (travaux)"),
            ("human_known", "work_activity")
        );
        assert_eq!(
            map_cause("Accidentelle"),
            ("human_known", "accident_unspecified")
        );
        assert_eq!(
            map_cause("Naturelle"),
            ("natural_known", "natural_unspecified")
        );
        assert_eq!(map_cause("Inconnue"), ("unknown", "unknown_unspecified"));
    }

    #[tokio::test]
    async fn matches_the_legacy_normalizer_on_the_existing_valid_fixture() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bdiff_aude.csv");
        let grid = H3Grid::new(8).expect("grid");
        let document = read_file(&path, grid).await.expect("new parser");
        let legacy = FireHistorySource::bdiff(&path)
            .fetch(&FetchCtx {
                client: reqwest::Client::new(),
                aoi: BoundingBox::new(-6.0, 41.0, 10.0, 52.0).expect("France bbox"),
                grid,
                days: 1,
                end_date: NaiveDate::from_ymd_opt(2025, 12, 31).expect("date"),
                firms_map_key: None,
                meteofrance_api_key: None,
            })
            .await
            .expect("legacy parser");
        assert_eq!(document.rows.len(), legacy.len());
        let mut missing_cause_divergences = 0;
        for (row, observation) in document.rows.iter().zip(&legacy) {
            let normalized = &row.normalized;
            if normalized.validation_errors == ["missing_cause"] {
                missing_cause_divergences += 1;
            } else {
                assert!(
                    normalized.is_valid(),
                    "legacy fixture row {} rejected: {:?}",
                    row.source_line_number,
                    normalized.validation_errors
                );
            }
            assert_eq!(
                row.source_record_id.as_deref(),
                Some(observation.dedupe_key.as_str())
            );
            assert_eq!(normalized.occurred_at, Some(observation.observed_at));
            assert_eq!(normalized.cell, Some(observation.cell));
            assert_eq!(
                normalized.municipality_source.as_deref(),
                observation.payload["municipality"].as_str()
            );
            assert_eq!(
                normalized.latitude,
                observation.payload["latitude"].as_f64()
            );
            assert_eq!(
                normalized.longitude,
                observation.payload["longitude"].as_f64()
            );
            assert_eq!(
                normalized.surface_ha,
                observation.payload["surface_ha"].as_f64()
            );
            if normalized.cause_source.is_some() {
                assert_eq!(
                    normalized.cause_source.as_deref(),
                    observation.payload["cause"].as_str()
                );
            } else {
                assert_eq!(observation.payload["cause"].as_str(), Some(""));
            }
        }
        assert_eq!(missing_cause_divergences, 7);
    }
}
