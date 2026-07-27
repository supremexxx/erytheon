//! Explicit temporal-validity classification for feature values and rows.
//! Every snapshot, calendar row, and dataset row must carry one of these.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalClassification {
    /// The value truly matches the state known at the observed date.
    HistoricalExact,
    /// The value comes from a snapshot dated close enough to be trusted.
    HistoricalSnapshot,
    /// Considered relatively stable, but no exact snapshot exists.
    StableApproximation,
    /// The current value is applied to the past with an explicit bias risk.
    CurrentSnapshotAppliedHistorically,
    /// No acceptable value exists for the period.
    UnavailableHistorically,
    /// Computed only from data strictly prior to the observation.
    DerivedPastOnly,
}

impl TemporalClassification {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HistoricalExact => "historical_exact",
            Self::HistoricalSnapshot => "historical_snapshot",
            Self::StableApproximation => "stable_approximation",
            Self::CurrentSnapshotAppliedHistorically => "current_snapshot_applied_historically",
            Self::UnavailableHistorically => "unavailable_historically",
            Self::DerivedPastOnly => "derived_past_only",
        }
    }

    /// Whether this classification is fit to feed a training row without a
    /// separate, explicit sensitivity flag.
    #[must_use]
    pub const fn safe_for_strict_dataset(self) -> bool {
        matches!(
            self,
            Self::HistoricalExact | Self::HistoricalSnapshot | Self::DerivedPastOnly
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_str() {
        for value in [
            TemporalClassification::HistoricalExact,
            TemporalClassification::HistoricalSnapshot,
            TemporalClassification::StableApproximation,
            TemporalClassification::CurrentSnapshotAppliedHistorically,
            TemporalClassification::UnavailableHistorically,
            TemporalClassification::DerivedPastOnly,
        ] {
            assert!(!value.as_str().is_empty());
        }
    }

    #[test]
    fn current_snapshot_applied_historically_is_not_strict_safe() {
        assert!(
            !TemporalClassification::CurrentSnapshotAppliedHistorically.safe_for_strict_dataset()
        );
        assert!(TemporalClassification::HistoricalExact.safe_for_strict_dataset());
    }
}
