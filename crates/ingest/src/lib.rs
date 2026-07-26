//! External data source contracts and connectors.

use std::{path::PathBuf, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use grid::{BoundingBox, CellIndex, H3Grid};
use serde::{Deserialize, Serialize};

pub mod calendar;
pub mod corine;
pub mod fire_history;
pub mod firms;
pub mod insee;
pub mod meteo_archive;
pub mod meteo_france;
pub mod open_meteo;
pub mod osm;

/// Polling schedule declared by a source connector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cadence {
    /// Repeated polling at the specified interval.
    Poll(Duration),
    /// One-time static loading.
    OneShot,
}

/// Context shared with a source during one fetch.
#[derive(Clone, Debug)]
pub struct FetchCtx {
    /// Reusable HTTP client.
    pub client: reqwest::Client,
    /// Configured area of interest.
    pub aoi: BoundingBox,
    /// Configured H3 projector.
    pub grid: H3Grid,
    /// Inclusive number of days requested by the backfill.
    pub days: u16,
    /// Final UTC date of the requested interval.
    pub end_date: NaiveDate,
    /// Optional FIRMS map key. Absence selects the fixture.
    pub firms_map_key: Option<String>,
    /// Optional Météo-France `OAuth2` access token. Absence selects the fixture.
    pub meteofrance_api_key: Option<String>,
}

/// Normalized observation produced by any source.
#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    /// Stable source identifier.
    pub source: String,
    /// Observation category.
    pub kind: ObservationKind,
    /// H3 cell containing the observation.
    #[serde(skip)]
    pub cell: CellIndex,
    /// UTC acquisition timestamp.
    pub observed_at: DateTime<Utc>,
    /// Source-specific typed payload serialized as JSON.
    pub payload: serde_json::Value,
    /// Source-specific deterministic identity used for idempotent persistence.
    #[serde(skip)]
    pub dedupe_key: String,
}

/// Supported observation categories.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    /// Satellite active-fire detection.
    ActiveFire,
    /// Weather station or model observation.
    WeatherObs,
    /// Static open-data feature.
    StaticFeature,
    /// Historical wildfire ignition.
    HistoricalIgnition,
}

impl ObservationKind {
    /// Stable database representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ActiveFire => "active_fire",
            Self::WeatherObs => "weather_obs",
            Self::StaticFeature => "static_feature",
            Self::HistoricalIgnition => "historical_ignition",
        }
    }
}

/// Asynchronous external-data connector.
#[async_trait]
pub trait Source: Send + Sync {
    /// Stable connector identifier.
    fn id(&self) -> &'static str;
    /// Polling schedule.
    fn cadence(&self) -> Cadence;
    /// Fetches and normalizes observations.
    async fn fetch(&self, ctx: &FetchCtx) -> Result<Vec<Observation>, SourceError>;
}

/// Source connector failures.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    /// Local fixture could not be read.
    #[error("failed to read fixture {path}: {source}")]
    FixtureRead {
        /// Fixture path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// A generated static-data file could not be read or written.
    #[error("failed to access static data file {path}: {source}")]
    StaticIo {
        /// Static-data path.
        path: PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Source HTTP request failed.
    #[error("source HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    /// CSV payload was malformed.
    #[error("invalid source CSV: {0}")]
    Csv(#[from] csv::Error),
    /// A date or acquisition time was invalid.
    #[error("invalid source timestamp `{value}`")]
    InvalidTimestamp {
        /// Invalid source value.
        value: String,
    },
    /// A source coordinate could not be projected.
    #[error("failed to project source coordinate: {0}")]
    Grid(#[from] grid::GridError),
    /// A typed payload could not be serialized.
    #[error("failed to serialize source payload: {0}")]
    Json(#[from] serde_json::Error),
    /// An OpenStreetMap PBF could not be decoded.
    #[error("failed to decode OpenStreetMap PBF: {0}")]
    Osm(#[from] osmpbf::Error),
    /// A coordinate reference system transformation failed.
    #[error("coordinate transformation failed: {0}")]
    Projection(String),
    /// A source record contains an unsupported value.
    #[error("invalid static source record: {0}")]
    InvalidStaticRecord(String),
    /// An external geospatial decoder was unavailable or failed.
    #[error("external command `{program}` failed: {message}")]
    ExternalCommand {
        /// Executable name.
        program: &'static str,
        /// Process or output error.
        message: String,
    },
    /// A zero-day request was supplied.
    #[error("backfill day count must be greater than zero")]
    InvalidDayCount,
    /// One or more NASA FIRMS rows could not be normalized.
    #[error("{0} NASA FIRMS rows could not be normalized")]
    InvalidFirmsRows(usize),
}
