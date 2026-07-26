//! CORINE Land Cover cell-sample loader.

use std::{collections::HashMap, path::PathBuf, process::Command};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use proj4rs::proj::Proj;
use serde::{Deserialize, Serialize};

use crate::{Cadence, FetchCtx, Observation, ObservationKind, Source, SourceError};

const EPSG_3035: &str = concat!(
    "+proj=laea +lat_0=52 +lon_0=10 ",
    "+x_0=4321000 +y_0=3210000 +ellps=GRS80 +units=m +no_defs"
);
const WGS84: &str = "+proj=longlat +datum=WGS84 +no_defs";

/// One-shot normalized CORINE sample source.
#[derive(Clone, Debug)]
pub struct CorineSource {
    path: PathBuf,
}

impl CorineSource {
    /// Creates a CORINE loader for a sampled CSV.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl Source for CorineSource {
    fn id(&self) -> &'static str {
        "corine"
    }

    fn cadence(&self) -> Cadence {
        Cadence::OneShot
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<Vec<Observation>, SourceError> {
        if matches!(
            self.path.extension().and_then(|value| value.to_str()),
            Some("tif" | "tiff")
        ) {
            return read_geotiff_aggregates(&self.path, ctx);
        }
        read_csv(&self.path)
            .await?
            .into_iter()
            .filter_map(|record| match record {
                record if ctx.aoi.contains(record.latitude, record.longitude) => {
                    Some(normalize(record, ctx))
                }
                _ => None,
            })
            .collect()
    }
}

async fn read_csv(path: &PathBuf) -> Result<Vec<CorineRecord>, SourceError> {
    let document =
        tokio::fs::read_to_string(path)
            .await
            .map_err(|source| SourceError::FixtureRead {
                path: path.clone(),
                source,
            })?;
    let mut reader = csv::Reader::from_reader(document.as_bytes());
    reader
        .deserialize()
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn read_geotiff_aggregates(
    path: &PathBuf,
    ctx: &FetchCtx,
) -> Result<Vec<Observation>, SourceError> {
    let from = Proj::from_proj_string(WGS84)
        .map_err(|error| SourceError::Projection(error.to_string()))?;
    let to = Proj::from_proj_string(EPSG_3035)
        .map_err(|error| SourceError::Projection(error.to_string()))?;
    let mut upper_left = (ctx.aoi.west.to_radians(), ctx.aoi.north.to_radians(), 0.0);
    let mut lower_right = (ctx.aoi.east.to_radians(), ctx.aoi.south.to_radians(), 0.0);
    proj4rs::transform::transform(&from, &to, &mut upper_left)
        .map_err(|error| SourceError::Projection(error.to_string()))?;
    proj4rs::transform::transform(&from, &to, &mut lower_right)
        .map_err(|error| SourceError::Projection(error.to_string()))?;
    let output = Command::new("gdal_translate")
        .args([
            "-q",
            "-projwin",
            &upper_left.0.to_string(),
            &upper_left.1.to_string(),
            &lower_right.0.to_string(),
            &lower_right.1.to_string(),
            "-of",
            "XYZ",
        ])
        .arg(path)
        .arg("/vsistdout/")
        .output()
        .map_err(|error| SourceError::ExternalCommand {
            program: "gdal_translate",
            message: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(SourceError::ExternalCommand {
            program: "gdal_translate",
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let reverse = Proj::from_proj_string(EPSG_3035)
        .map_err(|error| SourceError::Projection(error.to_string()))?;
    let wgs84 = Proj::from_proj_string(WGS84)
        .map_err(|error| SourceError::Projection(error.to_string()))?;
    let observed_at = "2018-01-01T00:00:00Z"
        .parse::<DateTime<Utc>>()
        .expect("valid CORINE release timestamp");
    let mut aggregates = HashMap::<grid::CellIndex, CorineCellAggregatePayload>::new();
    for line in output.stdout.split(|value| *value == b'\n') {
        if line.is_empty() {
            continue;
        }
        let line = std::str::from_utf8(line)
            .map_err(|error| SourceError::InvalidStaticRecord(error.to_string()))?;
        let mut fields = line.split_whitespace();
        let easting = parse_field(fields.next(), line)?;
        let northing = parse_field(fields.next(), line)?;
        let class_code = parse_field::<u16>(fields.next(), line)?;
        if class_code == 0 {
            continue;
        }
        let mut coordinate = (easting, northing, 0.0);
        proj4rs::transform::transform(&reverse, &wgs84, &mut coordinate)
            .map_err(|error| SourceError::Projection(error.to_string()))?;
        let latitude = coordinate.1.to_degrees();
        let longitude = coordinate.0.to_degrees();
        if !ctx.aoi.contains(latitude, longitude) {
            continue;
        }
        let cell = ctx.grid.cell_for_point(latitude, longitude)?;
        let (combustible, agricultural) = classify(class_code);
        let aggregate = aggregates.entry(cell).or_default();
        aggregate.combustible |= combustible;
        aggregate.agricultural |= agricultural;
    }
    let mut aggregates = aggregates.into_iter().collect::<Vec<_>>();
    aggregates.sort_unstable_by_key(|(cell, _)| *cell);
    aggregates
        .into_iter()
        .map(|(cell, payload)| {
            Ok(Observation {
                source: "corine".to_owned(),
                kind: ObservationKind::StaticFeature,
                cell,
                observed_at,
                payload: serde_json::to_value(payload)?,
                dedupe_key: format!("aggregate/{cell}"),
            })
        })
        .collect()
}

fn parse_field<T>(field: Option<&str>, line: &str) -> Result<T, SourceError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    field
        .ok_or_else(|| SourceError::InvalidStaticRecord(line.to_owned()))?
        .parse()
        .map_err(|error: T::Err| SourceError::InvalidStaticRecord(error.to_string()))
}

fn normalize(record: CorineRecord, ctx: &FetchCtx) -> Result<Observation, SourceError> {
    let (combustible, agricultural) = classify(record.class_code);
    let payload = CorinePayload {
        sample_id: record.sample_id.clone(),
        latitude: record.latitude,
        longitude: record.longitude,
        class_code: record.class_code,
        combustible,
        agricultural,
    };
    Ok(Observation {
        source: "corine".to_owned(),
        kind: ObservationKind::StaticFeature,
        cell: ctx.grid.cell_for_point(record.latitude, record.longitude)?,
        observed_at: record.observed_at,
        payload: serde_json::to_value(payload)?,
        dedupe_key: record.sample_id,
    })
}

fn classify(class_code: u16) -> (bool, bool) {
    let agricultural = matches!(class_code, 211..=244);
    let combustible = agricultural || matches!(class_code, 311..=313 | 321..=324 | 333 | 334);
    (combustible, agricultural)
}

#[derive(Debug, Deserialize)]
struct CorineRecord {
    sample_id: String,
    latitude: f64,
    longitude: f64,
    class_code: u16,
    observed_at: DateTime<Utc>,
}

/// Normalized CORINE class sampled at one WGS84 point.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CorinePayload {
    /// Stable sample identifier.
    pub sample_id: String,
    /// WGS84 latitude.
    pub latitude: f64,
    /// WGS84 longitude.
    pub longitude: f64,
    /// Three-digit CORINE class code.
    pub class_code: u16,
    /// Whether the class is treated as burnable vegetation.
    pub combustible: bool,
    /// Whether the class is agricultural land.
    pub agricultural: bool,
}

/// Per-H3 land-cover flags emitted by `GeoTIFF` ingestion.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorineCellAggregatePayload {
    /// Whether at least one sampled land-cover class is combustible.
    pub combustible: bool,
    /// Whether at least one sampled land-cover class is agricultural.
    pub agricultural: bool,
}

#[cfg(test)]
mod tests {
    use super::classify;

    #[test]
    fn classifies_forest_and_agriculture() {
        assert_eq!(classify(311), (true, false));
        assert_eq!(classify(211), (true, true));
        assert_eq!(classify(112), (false, false));
    }
}
