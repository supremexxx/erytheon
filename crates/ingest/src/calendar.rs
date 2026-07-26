//! Official school-holiday and public-holiday calendar loader.

use std::path::PathBuf;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{Cadence, SourceError};

/// One-shot normalized calendar source.
#[derive(Clone, Debug)]
pub struct CalendarSource {
    path: PathBuf,
}

impl CalendarSource {
    /// Creates a calendar CSV loader.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Declares this loader as one-shot static data.
    #[must_use]
    pub const fn cadence(&self) -> Cadence {
        Cadence::OneShot
    }

    /// Reads normalized daily calendar flags.
    ///
    /// # Errors
    ///
    /// Returns an error when the fixture cannot be read or decoded.
    pub async fn load(&self) -> Result<Vec<CalendarDay>, SourceError> {
        let document = tokio::fs::read_to_string(&self.path)
            .await
            .map_err(|source| SourceError::FixtureRead {
                path: self.path.clone(),
                source,
            })?;
        let mut reader = csv::Reader::from_reader(document.as_bytes());
        reader
            .deserialize()
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

/// Calendar flags used by the later dynamic risk multiplier.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CalendarDay {
    /// Calendar date.
    pub date: NaiveDate,
    /// Whether Zone C pupils are on school holiday.
    pub school_holiday: bool,
    /// Whether the date is a metropolitan public holiday.
    pub public_holiday: bool,
    /// Human-readable source labels joined with a semicolon.
    pub label: String,
}
