//! Rule-defined dataset splits. Splits are a property of the calendar year,
//! never adjustable row by row without a new, justified rule version.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    Train,
    Calibration,
    Test,
    Prospective,
}

impl Split {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Train => "train",
            Self::Calibration => "calibration",
            Self::Test => "test",
            Self::Prospective => "prospective",
        }
    }

    /// The default temporal split rule: train 2020-2023, calibration 2024,
    /// test 2025, prospective 2026. Returns `None` outside this range.
    #[must_use]
    pub const fn for_year(year: i32) -> Option<Self> {
        match year {
            2020..=2023 => Some(Self::Train),
            2024 => Some(Self::Calibration),
            2025 => Some(Self::Test),
            2026 => Some(Self::Prospective),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_match_the_specified_years() {
        assert_eq!(Split::for_year(2020), Some(Split::Train));
        assert_eq!(Split::for_year(2023), Some(Split::Train));
        assert_eq!(Split::for_year(2024), Some(Split::Calibration));
        assert_eq!(Split::for_year(2025), Some(Split::Test));
        assert_eq!(Split::for_year(2026), Some(Split::Prospective));
        assert_eq!(Split::for_year(2019), None);
        assert_eq!(Split::for_year(2027), None);
    }
}
