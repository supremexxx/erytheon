//! INSEE Filosofi 200-metre population-grid loader.

use std::{collections::HashMap, path::PathBuf};

use async_trait::async_trait;
use chrono::NaiveDate;
use proj4rs::proj::Proj;
use serde::{Deserialize, Serialize};

use crate::{Cadence, FetchCtx, Observation, ObservationKind, Source, SourceError};

const HALF_GRID_SIZE_METRES: f64 = 100.0;
const EPSG_3035: &str = concat!(
    "+proj=laea +lat_0=52 +lon_0=10 ",
    "+x_0=4321000 +y_0=3210000 +ellps=GRS80 +units=m +no_defs"
);
const WGS84: &str = "+proj=longlat +datum=WGS84 +no_defs";

/// One-shot INSEE Filosofi CSV source.
#[derive(Clone, Debug)]
pub struct InseeSource {
    path: PathBuf,
}

impl InseeSource {
    /// Creates an INSEE 200-metre-grid loader.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl Source for InseeSource {
    fn id(&self) -> &'static str {
        "insee_filosofi"
    }

    fn cadence(&self) -> Cadence {
        Cadence::OneShot
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<Vec<Observation>, SourceError> {
        let from = Proj::from_proj_string(EPSG_3035)
            .map_err(|error| SourceError::Projection(error.to_string()))?;
        let to = Proj::from_proj_string(WGS84)
            .map_err(|error| SourceError::Projection(error.to_string()))?;
        let observed_at = NaiveDate::from_ymd_opt(2019, 1, 1)
            .expect("valid fixture date")
            .and_hms_opt(0, 0, 0)
            .expect("valid fixture time")
            .and_utc();
        let mut reader = csv::Reader::from_path(&self.path)?;
        let mut aggregates = HashMap::<grid::CellIndex, (f64, u8)>::new();

        for record in reader.deserialize::<InseeRecord>() {
            let record = record?;
            let (easting, northing) = parse_grid_id(&record.grid_id)?;
            let mut coordinate = (
                easting + HALF_GRID_SIZE_METRES,
                northing + HALF_GRID_SIZE_METRES,
                0.0,
            );
            proj4rs::transform::transform(&from, &to, &mut coordinate)
                .map_err(|error| SourceError::Projection(error.to_string()))?;
            let longitude = coordinate.0.to_degrees();
            let latitude = coordinate.1.to_degrees();
            if !ctx.aoi.contains(latitude, longitude) {
                continue;
            }
            let cell = ctx.grid.cell_for_point(latitude, longitude)?;
            let aggregate = aggregates.entry(cell).or_default();
            aggregate.0 += record.individuals.max(0.0);
            aggregate.1 = aggregate.1.max(record.imputed);
        }
        let mut aggregates = aggregates.into_iter().collect::<Vec<_>>();
        aggregates.sort_unstable_by_key(|(cell, _)| *cell);
        aggregates
            .into_iter()
            .map(|(cell, (individuals, imputed))| {
                let center = ctx.grid.cell_center(cell);
                let payload = InseePopulationPayload {
                    grid_id: format!("aggregate/{cell}"),
                    municipality_code: String::new(),
                    latitude: center.lat(),
                    longitude: center.lng(),
                    individuals,
                    imputed,
                };
                Ok(Observation {
                    source: self.id().to_owned(),
                    kind: ObservationKind::StaticFeature,
                    cell,
                    observed_at,
                    payload: serde_json::to_value(payload)?,
                    dedupe_key: format!("aggregate/{cell}"),
                })
            })
            .collect()
    }
}

fn parse_grid_id(grid_id: &str) -> Result<(f64, f64), SourceError> {
    let northing_start = grid_id
        .find('N')
        .ok_or_else(|| SourceError::InvalidStaticRecord(grid_id.to_owned()))?
        + 1;
    let easting_marker = grid_id[northing_start..]
        .find('E')
        .map(|index| index + northing_start)
        .ok_or_else(|| SourceError::InvalidStaticRecord(grid_id.to_owned()))?;
    let northing = grid_id[northing_start..easting_marker]
        .parse::<f64>()
        .map_err(|error| SourceError::InvalidStaticRecord(error.to_string()))?;
    let easting = grid_id[easting_marker + 1..]
        .parse::<f64>()
        .map_err(|error| SourceError::InvalidStaticRecord(error.to_string()))?;
    Ok((easting, northing))
}

#[derive(Debug, Deserialize)]
struct InseeRecord {
    #[serde(rename = "idcar_200m")]
    grid_id: String,
    #[serde(rename = "i_est_200")]
    imputed: u8,
    #[serde(rename = "ind")]
    individuals: f64,
}

/// Normalized population-grid payload.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InseePopulationPayload {
    /// Official INSPIRE grid identifier.
    pub grid_id: String,
    /// Dominant municipality code.
    pub municipality_code: String,
    /// WGS84 centre latitude.
    pub latitude: f64,
    /// WGS84 centre longitude.
    pub longitude: f64,
    /// Number of individuals in the 200-metre square.
    pub individuals: f64,
    /// INSEE imputation indicator.
    pub imputed: u8,
}

#[cfg(test)]
mod tests {
    use super::parse_grid_id;

    #[test]
    fn parses_an_official_grid_identifier() {
        assert_eq!(
            parse_grid_id("CRS3035RES200mN2260600E3703800").expect("valid grid id"),
            (3_703_800.0, 2_260_600.0)
        );
    }
}
