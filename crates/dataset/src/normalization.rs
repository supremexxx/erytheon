//! Train-only normalization and imputation statistics for phase 3B.5.
//!
//! Every parameter here must be computed from **train-split rows only**;
//! nothing in this module reads calibration/test/prospective data, and
//! callers must never pass it any. See `DATASET_NORMALIZATION_AND_
//! IMPUTATION.md` for how these are used and reported.

use serde::{Deserialize, Serialize};

/// Summary statistics for one numeric feature, computed over train-split
/// values only. `present` values exclude missing (`None`) observations;
/// `missing_count` records how many rows lacked this feature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureStatistics {
    pub feature: String,
    pub count_present: usize,
    pub count_missing: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub median: f64,
    pub p05: f64,
    pub p95: f64,
    pub min: f64,
    pub max: f64,
}

/// A normalization method chosen per feature, not applied uniformly to
/// all features (mission section 16: do not pick the same transform for
/// everything). `None` deliberately means "in the enum, not absent" — a
/// visible, auditable decision rather than a missing table row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationMethod {
    /// (x - mean) / `std_dev`. Suitable for roughly symmetric features.
    Standardize,
    /// (x - median) / (p95 - p05). Suitable for skewed/outlier-prone
    /// features, since it does not let a few extreme values dominate.
    RobustScale,
    /// log1p applied before standardization. Suitable for strictly
    /// non-negative, heavy-tailed count-like features.
    Log1pThenStandardize,
    /// No transformation. Suitable for features already in a bounded,
    /// comparable range, or boolean-like features.
    None,
}

/// Computes train-only summary statistics for one feature's values.
/// `values` must already be filtered to train-split rows by the caller —
/// this function has no way to enforce that itself, which is exactly why
/// every call site must be covered by a leakage-check test (see
/// `DATASET_NORMALIZATION_AND_IMPUTATION.md`).
#[must_use]
pub fn train_only_statistics(feature: &str, values: &[Option<f64>]) -> FeatureStatistics {
    let mut present: Vec<f64> = values.iter().filter_map(|value| *value).collect();
    present.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count_missing = values.len() - present.len();
    if present.is_empty() {
        return FeatureStatistics {
            feature: feature.to_owned(),
            count_present: 0,
            count_missing,
            mean: 0.0,
            std_dev: 0.0,
            median: 0.0,
            p05: 0.0,
            p95: 0.0,
            min: 0.0,
            max: 0.0,
        };
    }
    #[allow(clippy::cast_precision_loss)]
    let count = present.len() as f64;
    let mean = present.iter().sum::<f64>() / count;
    let variance = present.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count;
    FeatureStatistics {
        feature: feature.to_owned(),
        count_present: present.len(),
        count_missing,
        mean,
        std_dev: variance.sqrt(),
        median: percentile(&present, 0.50),
        p05: percentile(&present, 0.05),
        p95: percentile(&present, 0.95),
        min: present[0],
        max: present[present.len() - 1],
    }
}

/// Nearest-rank percentile over an already-sorted, non-empty slice.
fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    debug_assert!(!sorted.is_empty());
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// Applies a chosen normalization to one value using train-only
/// statistics. `None` values are never transformed here — imputation
/// happens first, as a separate, explicit step (see
/// [`ImputationRule`]), so a value reaching this function is either a
/// real observation or an already-imputed one.
#[must_use]
pub fn apply_normalization(
    value: f64,
    method: NormalizationMethod,
    stats: &FeatureStatistics,
) -> f64 {
    match method {
        NormalizationMethod::Standardize => {
            if stats.std_dev.abs() < f64::EPSILON {
                0.0
            } else {
                (value - stats.mean) / stats.std_dev
            }
        }
        NormalizationMethod::RobustScale => {
            let spread = stats.p95 - stats.p05;
            if spread.abs() < f64::EPSILON {
                0.0
            } else {
                (value - stats.median) / spread
            }
        }
        NormalizationMethod::Log1pThenStandardize => {
            let transformed = value.max(0.0).ln_1p();
            let mean_log = stats.mean; // caller must have computed stats over already-log1p'd train values
            if stats.std_dev.abs() < f64::EPSILON {
                0.0
            } else {
                (transformed - mean_log) / stats.std_dev
            }
        }
        NormalizationMethod::None => value,
    }
}

/// A per-feature imputation rule, fit on train-split values only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImputationRule {
    pub feature: String,
    /// The value substituted for a missing observation. `None` means the
    /// feature is excluded entirely (see [`missing_ratio_exceeds_threshold`]),
    /// not silently imputed with an arbitrary number.
    pub imputed_value: Option<f64>,
    pub missing_ratio_in_train: f64,
}

/// The maximum tolerable fraction of missing train-split values before a
/// feature must be excluded rather than imputed. Not zero and not one:
/// zero would exclude nearly every real-world feature, one would silently
/// accept a feature that is mostly fabricated values.
pub const MAX_MISSING_RATIO_BEFORE_EXCLUSION: f64 = 0.5;

/// Whether a feature's train-split missingness is too high to impute
/// responsibly, per [`MAX_MISSING_RATIO_BEFORE_EXCLUSION`].
#[must_use]
pub fn missing_ratio_exceeds_threshold(stats: &FeatureStatistics) -> bool {
    let total = stats.count_present + stats.count_missing;
    if total == 0 {
        return true;
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = stats.count_missing as f64 / total as f64;
    ratio > MAX_MISSING_RATIO_BEFORE_EXCLUSION
}

/// Fits an imputation rule for one feature from its train-only statistics:
/// median imputation (robust to outliers, unlike mean) when missingness is
/// within tolerance, or exclusion (`imputed_value: None`) otherwise.
#[must_use]
pub fn fit_imputation_rule(stats: &FeatureStatistics) -> ImputationRule {
    let total = stats.count_present + stats.count_missing;
    #[allow(clippy::cast_precision_loss)]
    let missing_ratio = if total == 0 {
        1.0
    } else {
        stats.count_missing as f64 / total as f64
    };
    let imputed_value = if missing_ratio_exceeds_threshold(stats) {
        None
    } else {
        Some(stats.median)
    };
    ImputationRule {
        feature: stats.feature.clone(),
        imputed_value,
        missing_ratio_in_train: missing_ratio,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn statistics_ignore_missing_values_but_count_them() {
        let stats = train_only_statistics("wui", &[Some(1.0), None, Some(3.0), Some(5.0)]);
        assert_eq!(stats.count_present, 3);
        assert_eq!(stats.count_missing, 1);
        assert!((stats.mean - 3.0).abs() < 1e-9);
        assert!((stats.min - 1.0).abs() < 1e-9);
        assert!((stats.max - 5.0).abs() < 1e-9);
    }

    #[test]
    fn statistics_on_all_missing_returns_zeroed_stats_not_a_panic() {
        let stats = train_only_statistics("wui", &[None, None]);
        assert_eq!(stats.count_present, 0);
        assert_eq!(stats.count_missing, 2);
        assert!(stats.mean.abs() < 1e-9);
    }

    #[test]
    fn standardize_of_the_mean_is_zero() {
        let stats = train_only_statistics("x", &[Some(1.0), Some(2.0), Some(3.0)]);
        let normalized = apply_normalization(stats.mean, NormalizationMethod::Standardize, &stats);
        assert!(normalized.abs() < 1e-9);
    }

    #[test]
    fn robust_scale_of_the_median_is_zero() {
        let stats = train_only_statistics("x", &[Some(1.0), Some(2.0), Some(3.0), Some(100.0)]);
        let normalized =
            apply_normalization(stats.median, NormalizationMethod::RobustScale, &stats);
        assert!(normalized.abs() < 1e-9);
    }

    #[test]
    fn imputation_uses_median_when_missingness_is_within_threshold() {
        let stats = train_only_statistics("x", &[Some(1.0), Some(2.0), Some(3.0), None]);
        let rule = fit_imputation_rule(&stats);
        assert_eq!(rule.imputed_value, Some(stats.median));
    }

    #[test]
    fn imputation_excludes_the_feature_when_missingness_exceeds_threshold() {
        let stats = train_only_statistics("x", &[Some(1.0), None, None, None]);
        assert!(missing_ratio_exceeds_threshold(&stats));
        let rule = fit_imputation_rule(&stats);
        assert_eq!(
            rule.imputed_value, None,
            "a feature missing in more than half of train rows must be excluded, not imputed with a fabricated value"
        );
    }

    #[test]
    fn imputation_never_defaults_missing_to_zero_blindly() {
        // A feature whose real values are all far from zero (e.g. around
        // 100) must not be imputed with 0 just because that's a common
        // default; it must use the actual train median.
        let stats = train_only_statistics("x", &[Some(98.0), Some(100.0), Some(102.0), None]);
        let rule = fit_imputation_rule(&stats);
        assert_ne!(rule.imputed_value, Some(0.0));
        assert_eq!(rule.imputed_value, Some(100.0));
    }
}
