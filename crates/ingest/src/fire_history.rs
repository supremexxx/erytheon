//! BDIFF and Prométhée historical ignition loaders.

use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Cadence, FetchCtx, Observation, ObservationKind, Source, SourceError};

/// Supported official fire-history exports.
#[derive(Clone, Copy, Debug)]
pub enum FireHistoryKind {
    /// National BDIFF database.
    Bdiff,
    /// Mediterranean Prométhée database.
    Promethee,
}

impl FireHistoryKind {
    const fn source_id(self) -> &'static str {
        match self {
            Self::Bdiff => "bdiff",
            Self::Promethee => "promethee",
        }
    }
}

/// One-shot CSV fire-history source.
#[derive(Clone, Debug)]
pub struct FireHistorySource {
    kind: FireHistoryKind,
    path: PathBuf,
}

impl FireHistorySource {
    /// Creates a BDIFF loader.
    #[must_use]
    pub fn bdiff(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: FireHistoryKind::Bdiff,
            path: path.into(),
        }
    }

    /// Creates a Prométhée loader.
    #[must_use]
    pub fn promethee(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: FireHistoryKind::Promethee,
            path: path.into(),
        }
    }
}

#[async_trait]
impl Source for FireHistorySource {
    fn id(&self) -> &'static str {
        self.kind.source_id()
    }

    fn cadence(&self) -> Cadence {
        Cadence::OneShot
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<Vec<Observation>, SourceError> {
        let document = tokio::fs::read_to_string(&self.path)
            .await
            .map_err(|source| SourceError::FixtureRead {
                path: self.path.clone(),
                source,
            })?;
        let mut reader = csv::Reader::from_reader(document.as_bytes());
        let mut observations = Vec::new();
        for record in reader.deserialize::<FireHistoryRecord>() {
            let record = record?;
            if !ctx.aoi.contains(record.latitude, record.longitude) {
                continue;
            }
            let payload = FireHistoryPayload {
                external_id: record.external_id.clone(),
                municipality: record.municipality,
                latitude: record.latitude,
                longitude: record.longitude,
                surface_ha: record.surface_ha,
                cause: record.cause,
            };
            observations.push(Observation {
                source: self.id().to_owned(),
                kind: ObservationKind::HistoricalIgnition,
                cell: ctx
                    .grid
                    .cell_for_point(payload.latitude, payload.longitude)?,
                observed_at: record.occurred_at,
                payload: serde_json::to_value(payload)?,
                dedupe_key: record.external_id,
            });
        }
        Ok(observations)
    }
}

#[derive(Debug, Deserialize)]
struct FireHistoryRecord {
    external_id: String,
    occurred_at: DateTime<Utc>,
    municipality: String,
    latitude: f64,
    longitude: f64,
    surface_ha: f64,
    cause: String,
}

/// Normalized historical ignition payload.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FireHistoryPayload {
    /// Stable identifier in the source export.
    pub external_id: String,
    /// Municipality reported by the source.
    pub municipality: String,
    /// WGS84 latitude or municipality-centre latitude.
    pub latitude: f64,
    /// WGS84 longitude or municipality-centre longitude.
    pub longitude: f64,
    /// Burned surface in hectares.
    pub surface_ha: f64,
    /// Public grouped cause label.
    pub cause: String,
}
