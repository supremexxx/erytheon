//! Exclusion reasons. An exclusion is always explicit, categorized, and
//! traceable — never silent.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    UnknownCause,
    NaturalCause,
    Indeterminate,
    CertainDuplicate,
    InsufficientGeographicQuality,
    NonCombustibleCell,
    MissingFeatures,
    InvalidSnapshot,
    FutureData,
    OutOfPeriod,
    OutOfTerritory,
    WeatherUnavailable,
    TechnicalError,
}

impl ExclusionReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownCause => "unknown_cause",
            Self::NaturalCause => "natural_cause",
            Self::Indeterminate => "indeterminate",
            Self::CertainDuplicate => "certain_duplicate",
            Self::InsufficientGeographicQuality => "insufficient_geographic_quality",
            Self::NonCombustibleCell => "non_combustible_cell",
            Self::MissingFeatures => "missing_features",
            Self::InvalidSnapshot => "invalid_snapshot",
            Self::FutureData => "future_data",
            Self::OutOfPeriod => "out_of_period",
            Self::OutOfTerritory => "out_of_territory",
            Self::WeatherUnavailable => "weather_unavailable",
            Self::TechnicalError => "technical_error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_has_a_non_empty_label() {
        for reason in [
            ExclusionReason::UnknownCause,
            ExclusionReason::NaturalCause,
            ExclusionReason::Indeterminate,
            ExclusionReason::CertainDuplicate,
            ExclusionReason::InsufficientGeographicQuality,
            ExclusionReason::NonCombustibleCell,
            ExclusionReason::MissingFeatures,
            ExclusionReason::InvalidSnapshot,
            ExclusionReason::FutureData,
            ExclusionReason::OutOfPeriod,
            ExclusionReason::OutOfTerritory,
            ExclusionReason::WeatherUnavailable,
            ExclusionReason::TechnicalError,
        ] {
            assert!(!reason.as_str().is_empty());
        }
    }
}
