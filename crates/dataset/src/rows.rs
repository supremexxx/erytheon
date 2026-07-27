//! Structured dataset-row assembly primitives: deterministic identifiers,
//! row checksums, and the structured feature bundle. Provenance stays
//! separate (see `ml.dataset_row_snapshots`); this is not an opaque blob.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{checksums::logical_checksum, splits::Split};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowCategory {
    Positive,
    NegativePilot,
}

impl RowCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Positive => "positive",
            Self::NegativePilot => "negative_pilot",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowFeatures {
    pub wui: f64,
    pub road: f64,
    pub agri: f64,
    pub population: f64,
    pub poi: f64,
    pub power_line: f64,
    pub combustible: bool,
    pub weekend: bool,
    /// `None` when no verified school-holiday source exists for this
    /// date/zone; never coerced to `false`.
    pub school_holiday: Option<bool>,
    pub public_holiday: bool,
    pub season_sine: f64,
    pub season_cosine: f64,
}

/// A stable identifier that does not depend on database row order and is
/// identical across rebuilds of the same dataset version.
#[must_use]
pub fn deterministic_row_key(
    dataset_logical_id: &str,
    h3: i64,
    date: NaiveDate,
    category: RowCategory,
) -> String {
    logical_checksum(&(dataset_logical_id, h3, date.to_string(), category.as_str()))
}

#[must_use]
pub fn row_checksum(
    deterministic_key: &str,
    split: Split,
    label: u8,
    features: &RowFeatures,
    quality_flags: &[String],
    snapshot_ids: &[String],
) -> String {
    let mut sorted_flags = quality_flags.to_vec();
    sorted_flags.sort();
    let mut sorted_snapshots = snapshot_ids.to_vec();
    sorted_snapshots.sort();
    logical_checksum(&(
        deterministic_key,
        split.as_str(),
        label,
        features,
        sorted_flags,
        sorted_snapshots,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_features() -> RowFeatures {
        RowFeatures {
            wui: 0.1,
            road: 0.2,
            agri: 0.0,
            population: 0.3,
            poi: 0.4,
            power_line: 0.0,
            combustible: true,
            weekend: false,
            school_holiday: Some(false),
            public_holiday: false,
            season_sine: 0.5,
            season_cosine: 0.6,
        }
    }

    #[test]
    fn deterministic_key_is_stable_and_distinguishes_category() {
        let date = NaiveDate::from_ymd_opt(2023, 6, 1).unwrap();
        let positive = deterministic_row_key("ds_v1", 42, date, RowCategory::Positive);
        let negative = deterministic_row_key("ds_v1", 42, date, RowCategory::NegativePilot);
        assert_ne!(positive, negative);
        assert_eq!(
            positive,
            deterministic_row_key("ds_v1", 42, date, RowCategory::Positive)
        );
    }

    #[test]
    fn row_checksum_ignores_input_order_of_flags_and_snapshots() {
        let key = "abc";
        let features = sample_features();
        let a = row_checksum(
            key,
            Split::Train,
            1,
            &features,
            &["b".into(), "a".into()],
            &["y".into(), "x".into()],
        );
        let b = row_checksum(
            key,
            Split::Train,
            1,
            &features,
            &["a".into(), "b".into()],
            &["x".into(), "y".into()],
        );
        assert_eq!(a, b);
    }

    #[test]
    fn row_checksum_changes_when_label_changes() {
        let key = "abc";
        let features = sample_features();
        let positive = row_checksum(key, Split::Train, 1, &features, &[], &[]);
        let negative = row_checksum(key, Split::Train, 0, &features, &[], &[]);
        assert_ne!(positive, negative);
    }
}
