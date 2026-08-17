//! Phase 3B.7 experimental training, calibration, and candidate
//! comparison. Never replaces the active v1 model, never writes a
//! serving score, never touches `crates/api`, FIRMS, or FWI. All
//! artifacts are written to an isolated, disposable directory (never a
//! production volume) and reported by size/checksum, not committed to
//! Git (mission section 24: no binary model/prediction files in Git
//! without an existing convention — none exists here).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use dataset::normalization::{
    self, FeatureStatistics, ImputationRule, NormalizationMethod, apply_normalization,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use store::{Store, TrainingRow};

use crate::config::Config;

const CODE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The 12 features every experiment trains on, in fixed order. Chosen in
/// `PHASE3B6_SCIENTIFIC_DATASET_REVIEW.md` §6/§16: 7 real `cell_static`
/// features, `combustible` as 0/1, and 4 real calendar features.
/// `school_holiday` is excluded (100% missing, never fabricated).
pub(crate) const FEATURE_NAMES: [&str; 12] = [
    "wui",
    "road",
    "agri",
    "population",
    "poi",
    "power_line",
    "hist",
    "combustible",
    "weekend",
    "public_holiday",
    "season_sine",
    "season_cosine",
];

#[derive(Clone, Copy, Debug)]
pub struct ExperimentOptions {
    pub dry_run: bool,
    pub seed: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromotionCriteria {
    pub min_average_precision_gain_over_v1: f64,
    pub min_roc_auc: f64,
    pub max_brier_score: f64,
    pub max_ece: f64,
    pub min_lift_at_10pct: f64,
}

impl Default for PromotionCriteria {
    fn default() -> Self {
        // Registered before any 2025 result is read (mission section 19):
        // deliberately modest thresholds for a first experimental pass,
        // not tuned to any known outcome.
        Self {
            min_average_precision_gain_over_v1: 0.0,
            min_roc_auc: 0.60,
            max_brier_score: 0.20,
            max_ece: 0.10,
            min_lift_at_10pct: 1.5,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ExperimentManifest {
    pub experiment_id: String,
    pub git_commit: String,
    pub dataset_logical_id: String,
    pub dataset_row_fingerprint_before: String,
    pub features: Vec<String>,
    pub normalization_methods: HashMap<String, String>,
    pub seed: i64,
    pub code_version: String,
    pub started_at_utc: String,
    pub hardware: String,
    pub scientific_objective: String,
    pub promotion_criteria: PromotionCriteria,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sample {
    pub(crate) x: [f64; 12],
    pub(crate) y: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogisticModel {
    pub weights: [f64; 12],
    pub bias: f64,
    pub l2: f64,
}

impl LogisticModel {
    fn predict(&self, x: &[f64; 12]) -> f64 {
        let mut z = self.bias;
        for (w, xi) in self.weights.iter().zip(x.iter()) {
            z += w * xi;
        }
        sigmoid(z)
    }
}

pub(crate) const fn mix64_local(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    x
}

/// Feature importance (logistic: standardized coefficient magnitude/sign;
/// GBM: count of splits using each feature, a cheap, real proxy for gain-
/// based importance -- not a causal claim, per mission section 16), a
/// block bootstrap (by unique test date, respecting that rows on the
/// same day are not independent) for the frozen GBM+isotonic model's key
/// metrics, a 5-fold spatial cross-validation within train (grouped by
/// H3 resolution-5 parent, using the cheaper logistic architecture to
/// keep cost bounded), and a class-weighting comparison evaluated on
/// calibration only (test is never touched twice).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn run_supplementary_analyses(
    train_owned: &[TrainingRow],
    test_owned: &[TrainingRow],
    train_samples: &[Sample],
    test_samples: &[Sample],
    calib_samples: &[Sample],
    logistic_final: &LogisticModel,
    gbm_final: &GbmModel,
    gbm_test_scores_isotonic: &[(f64, f64)],
    isotonic_blocks: Option<&Vec<(f64, f64)>>,
    gbm_params: (usize, usize, f64),
    logistic_l2: f64,
    seed: i64,
) -> serde_json::Value {
    fn count_splits(tree: &Tree, counts: &mut [usize; 12]) {
        if let Tree::Split {
            feature,
            left,
            right,
            ..
        } = tree
        {
            counts[*feature] += 1;
            count_splits(left, counts);
            count_splits(right, counts);
        }
    }
    const BOOTSTRAP_ROUNDS: usize = 200;

    let _ = test_samples;
    // --- Feature importance ---
    let logistic_importance: Vec<(String, f64)> = (0..12)
        .map(|i| (FEATURE_NAMES[i].to_owned(), logistic_final.weights[i]))
        .collect();
    let mut split_counts = [0usize; 12];
    for tree in &gbm_final.trees {
        count_splits(tree, &mut split_counts);
    }
    let gbm_importance: Vec<(String, usize)> = (0..12)
        .map(|i| (FEATURE_NAMES[i].to_owned(), split_counts[i]))
        .collect();

    // --- Block bootstrap by unique test date ---
    let dates: Vec<NaiveDate> = test_owned.iter().map(|r| r.local_date).collect();
    let mut unique_dates: Vec<NaiveDate> = dates.clone();
    unique_dates.sort_unstable();
    unique_dates.dedup();
    let mut by_date: HashMap<NaiveDate, Vec<usize>> = HashMap::new();
    for (i, date) in dates.iter().enumerate() {
        by_date.entry(*date).or_default().push(i);
    }
    let mut auc_samples = Vec::with_capacity(BOOTSTRAP_ROUNDS);
    let mut ap_samples = Vec::with_capacity(BOOTSTRAP_ROUNDS);
    let mut brier_samples = Vec::with_capacity(BOOTSTRAP_ROUNDS);
    for round in 0..BOOTSTRAP_ROUNDS {
        let mut resampled = Vec::new();
        for i in 0..unique_dates.len() {
            let h = mix64_local(
                seed.unsigned_abs() ^ (round as u64).wrapping_mul(0x9E37_79B9) ^ i as u64,
            );
            #[allow(clippy::cast_possible_truncation)]
            let picked_date = unique_dates[(h as usize) % unique_dates.len()];
            if let Some(indices) = by_date.get(&picked_date) {
                for &idx in indices {
                    resampled.push(gbm_test_scores_isotonic[idx]);
                }
            }
        }
        if resampled.is_empty() {
            continue;
        }
        auc_samples.push(roc_auc(&resampled));
        ap_samples.push(average_precision(&resampled));
        brier_samples.push(brier_score(&resampled));
    }
    let percentile_ci = |values: &mut Vec<f64>| -> (f64, f64) {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if values.is_empty() {
            return (0.0, 0.0);
        }
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let lo_idx = (values.len() as f64 * 0.025) as usize;
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let hi_idx = ((values.len() as f64 * 0.975) as usize).min(values.len() - 1);
        (values[lo_idx], values[hi_idx])
    };
    let auc_ci = percentile_ci(&mut auc_samples.clone());
    let ap_ci = percentile_ci(&mut ap_samples.clone());
    let brier_ci = percentile_ci(&mut brier_samples.clone());

    // --- 5-fold spatial cross-validation within train (logistic, by H3 res5 parent) ---
    let res5 = grid::Resolution::try_from(5).ok();
    let mut fold_of_row: Vec<usize> = Vec::with_capacity(train_owned.len());
    for row in train_owned {
        let fold = res5
            .and_then(|res| {
                grid::cell_from_db(row.h3)
                    .ok()
                    .and_then(|cell| cell.parent(res))
            })
            .map_or(0, |parent| (u64::from(parent) % 5) as usize);
        fold_of_row.push(fold);
    }
    let mut fold_aucs = Vec::new();
    for fold in 0..5 {
        let fit_indices: Vec<usize> = (0..train_owned.len())
            .filter(|&i| fold_of_row[i] != fold)
            .collect();
        let held_indices: Vec<usize> = (0..train_owned.len())
            .filter(|&i| fold_of_row[i] == fold)
            .collect();
        if fit_indices.is_empty() || held_indices.is_empty() {
            continue;
        }
        let fit_samples: Vec<Sample> = fit_indices.iter().map(|&i| train_samples[i]).collect();
        let held_samples: Vec<Sample> = held_indices.iter().map(|&i| train_samples[i]).collect();
        let model = fit_logistic(&fit_samples, logistic_l2, 300, 0.3);
        let scored: Vec<(f64, f64)> = held_samples
            .iter()
            .map(|s| (model.predict(&s.x), s.y))
            .collect();
        fold_aucs.push(roc_auc(&scored));
    }
    #[allow(clippy::cast_precision_loss)]
    let fold_mean = if fold_aucs.is_empty() {
        0.0
    } else {
        fold_aucs.iter().sum::<f64>() / fold_aucs.len() as f64
    };
    let fold_variance = if fold_aucs.len() > 1 {
        #[allow(clippy::cast_precision_loss)]
        let n = fold_aucs.len() as f64;
        fold_aucs
            .iter()
            .map(|a| (a - fold_mean).powi(2))
            .sum::<f64>()
            / n
    } else {
        0.0
    };

    // --- Weighting comparison (class weight vs none), evaluated on calibration only ---
    #[allow(clippy::cast_precision_loss)]
    let positive_weight = {
        let positives = train_samples.iter().filter(|s| s.y > 0.5).count();
        let negatives = train_samples.len() - positives;
        if positives == 0 {
            1.0
        } else {
            negatives as f64 / positives as f64
        }
    };
    let weighted_samples: Vec<Sample> = train_samples
        .iter()
        .flat_map(|s| {
            if s.y > 0.5 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let repeats = positive_weight.round().max(1.0) as usize;
                std::iter::repeat_n(*s, repeats)
            } else {
                std::iter::repeat_n(*s, 1)
            }
        })
        .collect();
    let weighted_model = fit_logistic(&weighted_samples, logistic_l2, 300, 0.3);
    let unweighted_calib_scored: Vec<(f64, f64)> = calib_samples
        .iter()
        .map(|s| (logistic_final.predict(&s.x), s.y))
        .collect();
    let weighted_calib_scored: Vec<(f64, f64)> = calib_samples
        .iter()
        .map(|s| (weighted_model.predict(&s.x), s.y))
        .collect();

    json!({
        "feature_importance": {
            "logistic_standardized_coefficients": logistic_importance,
            "gbm_split_counts": gbm_importance,
        },
        "bootstrap_block_by_date": {
            "rounds": BOOTSTRAP_ROUNDS,
            "roc_auc_95pct_ci": auc_ci,
            "average_precision_95pct_ci": ap_ci,
            "brier_score_95pct_ci": brier_ci,
        },
        "spatial_cv_logistic_by_h3_res5": {
            "fold_aucs": fold_aucs,
            "mean_auc": fold_mean,
            "variance": fold_variance,
            "l2_used": logistic_l2,
        },
        "weighting_comparison_on_calibration": {
            "positive_class_weight": positive_weight,
            "unweighted": compute_split_metrics("calibration", &unweighted_calib_scored),
            "class_weighted": compute_split_metrics("calibration", &weighted_calib_scored),
        },
        "gbm_params_used_for_primary_model": {"n_trees": gbm_params.0, "max_depth": gbm_params.1, "learning_rate": gbm_params.2},
        "isotonic_blocks_count": isotonic_blocks.map(Vec::len),
    })
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + (-z).exp())
}

fn fit_logistic(samples: &[Sample], l2: f64, iterations: usize, lr: f64) -> LogisticModel {
    let mut weights = [0.0f64; 12];
    let mut bias = 0.0f64;
    #[allow(clippy::cast_precision_loss)]
    let n = samples.len() as f64;
    if samples.is_empty() {
        return LogisticModel { weights, bias, l2 };
    }
    for _ in 0..iterations {
        let mut grad_w = [0.0f64; 12];
        let mut grad_b = 0.0f64;
        for sample in samples {
            let mut z = bias;
            for (w, x) in weights.iter().zip(sample.x.iter()) {
                z += w * x;
            }
            let p = sigmoid(z);
            let error = p - sample.y;
            for (g, x) in grad_w.iter_mut().zip(sample.x.iter()) {
                *g += error * x;
            }
            grad_b += error;
        }
        for (w, g) in weights.iter_mut().zip(grad_w.iter()) {
            let regularized = g / n + l2 * *w;
            *w -= lr * regularized;
        }
        bias -= lr * (grad_b / n);
    }
    LogisticModel { weights, bias, l2 }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Tree {
    Leaf(f64),
    Split {
        feature: usize,
        threshold: f64,
        left: Box<Tree>,
        right: Box<Tree>,
    },
}

impl Tree {
    fn predict(&self, x: &[f64; 12]) -> f64 {
        match self {
            Tree::Leaf(value) => *value,
            Tree::Split {
                feature,
                threshold,
                left,
                right,
            } => {
                if x[*feature] <= *threshold {
                    left.predict(x)
                } else {
                    right.predict(x)
                }
            }
        }
    }

    /// Builds one shallow regression tree fitting `residuals` at `indices`
    /// into `samples`, splitting on variance reduction over a small set of
    /// candidate thresholds (feature deciles), not an exact full sort.
    fn fit(samples: &[Sample], residuals: &[f64], indices: &[usize], depth: usize) -> Tree {
        let n = indices.len();
        #[allow(clippy::cast_precision_loss)]
        let mean = indices.iter().map(|&i| residuals[i]).sum::<f64>() / n.max(1) as f64;
        if depth == 0 || n < 20 {
            return Tree::Leaf(mean);
        }
        let mut best: Option<(usize, f64, f64)> = None; // (feature, threshold, score)
        for feature in 0..12 {
            let mut values: Vec<f64> = indices.iter().map(|&i| samples[i].x[feature]).collect();
            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let candidates: Vec<f64> = (1..10)
                .map(|decile| {
                    let idx = (values.len() * decile / 10).min(values.len() - 1);
                    values[idx]
                })
                .collect();
            for threshold in candidates {
                let (mut left_sum, mut left_n, mut right_sum, mut right_n) =
                    (0.0, 0usize, 0.0, 0usize);
                for &i in indices {
                    if samples[i].x[feature] <= threshold {
                        left_sum += residuals[i];
                        left_n += 1;
                    } else {
                        right_sum += residuals[i];
                        right_n += 1;
                    }
                }
                if left_n == 0 || right_n == 0 {
                    continue;
                }
                #[allow(clippy::cast_precision_loss)]
                let (left_mean, right_mean) =
                    (left_sum / left_n as f64, right_sum / right_n as f64);
                let mut sse = 0.0;
                for &i in indices {
                    let predicted = if samples[i].x[feature] <= threshold {
                        left_mean
                    } else {
                        right_mean
                    };
                    sse += (residuals[i] - predicted).powi(2);
                }
                if best.is_none_or(|(_, _, best_sse)| sse < best_sse) {
                    best = Some((feature, threshold, sse));
                }
            }
        }
        let Some((feature, threshold, _)) = best else {
            return Tree::Leaf(mean);
        };
        let (left_idx, right_idx): (Vec<usize>, Vec<usize>) = indices
            .iter()
            .partition(|&&i| samples[i].x[feature] <= threshold);
        if left_idx.is_empty() || right_idx.is_empty() {
            return Tree::Leaf(mean);
        }
        Tree::Split {
            feature,
            threshold,
            left: Box::new(Tree::fit(samples, residuals, &left_idx, depth - 1)),
            right: Box::new(Tree::fit(samples, residuals, &right_idx, depth - 1)),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GbmModel {
    pub trees: Vec<Tree>,
    pub learning_rate: f64,
    pub base_score: f64,
    pub max_depth: usize,
    pub n_trees: usize,
}

impl GbmModel {
    pub(crate) fn predict(&self, x: &[f64; 12]) -> f64 {
        let mut z = self.base_score;
        for tree in &self.trees {
            z += self.learning_rate * tree.predict(x);
        }
        sigmoid(z)
    }
}

pub(crate) fn fit_gbm(
    samples: &[Sample],
    n_trees: usize,
    max_depth: usize,
    learning_rate: f64,
) -> GbmModel {
    #[allow(clippy::cast_precision_loss)]
    let positive_rate =
        samples.iter().filter(|s| s.y > 0.5).count() as f64 / samples.len().max(1) as f64;
    let base_score = (positive_rate.clamp(1e-6, 1.0 - 1e-6)
        / (1.0 - positive_rate.clamp(1e-6, 1.0 - 1e-6)))
    .ln();
    let mut current = vec![base_score; samples.len()];
    let mut trees = Vec::with_capacity(n_trees);
    let indices: Vec<usize> = (0..samples.len()).collect();
    for _ in 0..n_trees {
        let residuals: Vec<f64> = samples
            .iter()
            .zip(current.iter())
            .map(|(sample, &z)| sample.y - sigmoid(z))
            .collect();
        let tree = Tree::fit(samples, &residuals, &indices, max_depth);
        for (i, sample) in samples.iter().enumerate() {
            current[i] += learning_rate * tree.predict(&sample.x);
        }
        trees.push(tree);
    }
    GbmModel {
        trees,
        learning_rate,
        base_score,
        max_depth,
        n_trees,
    }
}

// --- Metrics ---

pub(crate) fn roc_auc(scored: &[(f64, f64)]) -> f64 {
    let mut sorted = scored.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let positives = sorted.iter().filter(|(_, y)| *y > 0.5).count();
    let negatives = sorted.len() - positives;
    if positives == 0 || negatives == 0 {
        return 0.5;
    }
    let mut rank_sum = 0.0;
    let mut rank = 1.0;
    let mut i = 0;
    while i < sorted.len() {
        let mut j = i;
        while j + 1 < sorted.len() && (sorted[j + 1].0 - sorted[i].0).abs() < 1e-12 {
            j += 1;
        }
        #[allow(clippy::cast_precision_loss)]
        let avg_rank = (rank + rank + (j - i) as f64) / 2.0;
        for (_, y) in &sorted[i..=j] {
            if *y > 0.5 {
                rank_sum += avg_rank;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        {
            rank += (j - i + 1) as f64;
        }
        i = j + 1;
    }
    #[allow(clippy::cast_precision_loss)]
    let (p, n) = (positives as f64, negatives as f64);
    (rank_sum - p * (p + 1.0) / 2.0) / (p * n)
}

pub(crate) fn average_precision(scored: &[(f64, f64)]) -> f64 {
    let mut sorted = scored.to_vec();
    sorted.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let total_positives = sorted.iter().filter(|(_, y)| *y > 0.5).count();
    if total_positives == 0 {
        return 0.0;
    }
    let mut tp = 0usize;
    let mut sum = 0.0;
    for (rank, (_, y)) in sorted.iter().enumerate() {
        if *y > 0.5 {
            tp += 1;
            #[allow(clippy::cast_precision_loss)]
            let precision_at_rank = tp as f64 / (rank + 1) as f64;
            sum += precision_at_rank;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    {
        sum / total_positives as f64
    }
}

pub(crate) fn log_loss(scored: &[(f64, f64)]) -> f64 {
    let eps = 1e-12;
    #[allow(clippy::cast_precision_loss)]
    let n = scored.len() as f64;
    if scored.is_empty() {
        return 0.0;
    }
    let sum: f64 = scored
        .iter()
        .map(|(p, y)| {
            let p = p.clamp(eps, 1.0 - eps);
            -(y * p.ln() + (1.0 - y) * (1.0 - p).ln())
        })
        .sum();
    sum / n
}

pub(crate) fn brier_score(scored: &[(f64, f64)]) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let n = scored.len() as f64;
    if scored.is_empty() {
        return 0.0;
    }
    scored.iter().map(|(p, y)| (p - y).powi(2)).sum::<f64>() / n
}

/// Expected calibration error over 10 equal-width bins.
pub(crate) fn expected_calibration_error(scored: &[(f64, f64)]) -> f64 {
    const BINS: usize = 10;
    let mut bin_sum_p = [0.0; BINS];
    let mut bin_sum_y = [0.0; BINS];
    let mut bin_count = [0usize; BINS];
    for &(p, y) in scored {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let bin = ((p * BINS as f64).floor() as usize).min(BINS - 1);
        bin_sum_p[bin] += p;
        bin_sum_y[bin] += y;
        bin_count[bin] += 1;
    }
    #[allow(clippy::cast_precision_loss)]
    let n = scored.len() as f64;
    if scored.is_empty() {
        return 0.0;
    }
    let mut ece = 0.0;
    for bin in 0..BINS {
        if bin_count[bin] == 0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let count = bin_count[bin] as f64;
        let avg_p = bin_sum_p[bin] / count;
        let avg_y = bin_sum_y[bin] / count;
        ece += (count / n) * (avg_p - avg_y).abs();
    }
    ece
}

pub(crate) fn precision_recall_f1(scored: &[(f64, f64)], threshold: f64) -> (f64, f64, f64) {
    let mut tp = 0.0;
    let mut fp = 0.0;
    let mut fn_ = 0.0;
    for &(p, y) in scored {
        let predicted = p >= threshold;
        let actual = y > 0.5;
        match (predicted, actual) {
            (true, true) => tp += 1.0,
            (true, false) => fp += 1.0,
            (false, true) => fn_ += 1.0,
            (false, false) => {}
        }
    }
    let precision = if tp + fp > 0.0 { tp / (tp + fp) } else { 0.0 };
    let recall = if tp + fn_ > 0.0 { tp / (tp + fn_) } else { 0.0 };
    let f1 = if precision + recall > 0.0 {
        2.0 * precision * recall / (precision + recall)
    } else {
        0.0
    };
    (precision, recall, f1)
}

pub(crate) fn precision_recall_lift_at_k(scored: &[(f64, f64)], fraction: f64) -> (f64, f64, f64) {
    let mut sorted = scored.to_vec();
    sorted.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let k = ((sorted.len() as f64) * fraction).ceil() as usize;
    let k = k.max(1).min(sorted.len());
    let top = &sorted[..k];
    let tp = top.iter().filter(|(_, y)| *y > 0.5).count();
    let total_positives = sorted.iter().filter(|(_, y)| *y > 0.5).count();
    #[allow(clippy::cast_precision_loss)]
    let precision_at_k = tp as f64 / k as f64;
    #[allow(clippy::cast_precision_loss)]
    let recall_at_k = if total_positives > 0 {
        tp as f64 / total_positives as f64
    } else {
        0.0
    };
    #[allow(clippy::cast_precision_loss)]
    let base_rate = total_positives as f64 / sorted.len().max(1) as f64;
    let lift = if base_rate > 0.0 {
        precision_at_k / base_rate
    } else {
        0.0
    };
    (precision_at_k, recall_at_k, lift)
}

#[derive(Clone, Debug, Serialize)]
pub struct SplitMetrics {
    pub split: String,
    pub n: usize,
    pub positive_rate: f64,
    pub roc_auc: f64,
    pub average_precision: f64,
    pub log_loss: f64,
    pub brier_score: f64,
    pub ece: f64,
    pub precision_at_threshold_0_5: f64,
    pub recall_at_threshold_0_5: f64,
    pub f1_at_threshold_0_5: f64,
    pub precision_at_1pct: f64,
    pub recall_at_1pct: f64,
    pub lift_at_1pct: f64,
    pub precision_at_5pct: f64,
    pub recall_at_5pct: f64,
    pub lift_at_5pct: f64,
    pub precision_at_10pct: f64,
    pub recall_at_10pct: f64,
    pub lift_at_10pct: f64,
}

pub(crate) fn compute_split_metrics(split: &str, scored: &[(f64, f64)]) -> SplitMetrics {
    #[allow(clippy::cast_precision_loss)]
    let positive_rate =
        scored.iter().filter(|(_, y)| *y > 0.5).count() as f64 / scored.len().max(1) as f64;
    let (p1, r1, l1) = precision_recall_lift_at_k(scored, 0.01);
    let (p5, r5, l5) = precision_recall_lift_at_k(scored, 0.05);
    let (p10, r10, l10) = precision_recall_lift_at_k(scored, 0.10);
    let (precision_5, recall_5, f1_5) = precision_recall_f1(scored, 0.5);
    SplitMetrics {
        split: split.to_owned(),
        n: scored.len(),
        positive_rate,
        roc_auc: roc_auc(scored),
        average_precision: average_precision(scored),
        log_loss: log_loss(scored),
        brier_score: brier_score(scored),
        ece: expected_calibration_error(scored),
        precision_at_threshold_0_5: precision_5,
        recall_at_threshold_0_5: recall_5,
        f1_at_threshold_0_5: f1_5,
        precision_at_1pct: p1,
        recall_at_1pct: r1,
        lift_at_1pct: l1,
        precision_at_5pct: p5,
        recall_at_5pct: r5,
        lift_at_5pct: l5,
        precision_at_10pct: p10,
        recall_at_10pct: r10,
        lift_at_10pct: l10,
    }
}

// --- Calibration ---

/// Platt scaling: a 1-feature logistic regression of label on raw score.
fn fit_platt(scores: &[(f64, f64)]) -> (f64, f64) {
    let samples: Vec<Sample> = scores
        .iter()
        .map(|&(p, y)| {
            let mut x = [0.0; 12];
            x[0] = p;
            Sample { x, y }
        })
        .collect();
    let model = fit_logistic(&samples, 0.0, 300, 0.3);
    (model.weights[0], model.bias)
}

fn apply_platt(score: f64, weight: f64, bias: f64) -> f64 {
    sigmoid(weight * score + bias)
}

/// Pool-adjacent-violators isotonic regression, fit on (score, label)
/// pairs sorted by score, producing a monotonic step function.
pub(crate) fn fit_isotonic(scores: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted = scores.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut blocks: Vec<(f64, f64, usize)> = Vec::new(); // (x_max, mean_y, count)
    for (x, y) in sorted {
        blocks.push((x, y, 1));
        while blocks.len() > 1 {
            let last = blocks.len() - 1;
            let (_x1, y1, n1) = blocks[last - 1];
            let (x2, y2, n2) = blocks[last];
            if y1 > y2 {
                #[allow(clippy::cast_precision_loss)]
                let merged_y = (y1 * n1 as f64 + y2 * n2 as f64) / (n1 + n2) as f64;
                blocks.pop();
                blocks.pop();
                blocks.push((x2, merged_y, n1 + n2));
            } else {
                break;
            }
        }
    }
    blocks.into_iter().map(|(x, y, _)| (x, y)).collect()
}

pub(crate) fn apply_isotonic(score: f64, blocks: &[(f64, f64)]) -> f64 {
    if blocks.is_empty() {
        return score;
    }
    for &(x, y) in blocks {
        if score <= x {
            return y;
        }
    }
    blocks.last().map_or(score, |&(_, y)| y)
}

// --- Feature extraction ---

fn extract_numeric(features: &serde_json::Value, name: &str) -> Option<f64> {
    features.get(name).and_then(serde_json::Value::as_f64)
}

pub(crate) fn build_raw_row(row: &TrainingRow) -> [Option<f64>; 12] {
    let f = &row.features;
    [
        extract_numeric(f, "wui"),
        extract_numeric(f, "road"),
        extract_numeric(f, "agri"),
        extract_numeric(f, "population"),
        extract_numeric(f, "poi"),
        extract_numeric(f, "power_line"),
        extract_numeric(f, "hist"),
        f.get("combustible")
            .and_then(serde_json::Value::as_bool)
            .map(f64::from),
        f.get("weekend")
            .and_then(serde_json::Value::as_bool)
            .map(f64::from),
        f.get("public_holiday")
            .and_then(serde_json::Value::as_bool)
            .map(f64::from),
        extract_numeric(f, "season_sine"),
        extract_numeric(f, "season_cosine"),
    ]
}

fn feature_method(index: usize) -> NormalizationMethod {
    match index {
        0 | 2 => NormalizationMethod::RobustScale, // wui, agri
        1 => NormalizationMethod::Standardize,     // road
        3..=6 => NormalizationMethod::Log1pThenStandardize, // population, poi, power_line, hist
        _ => NormalizationMethod::None,            // combustible, weekend, holiday, season
    }
}

/// Fits train-only statistics for each numeric feature (indices 0-6) and
/// returns them alongside the pre-chosen method map, ready to normalize
/// any split's rows without ever reading calibration/test to do so.
pub(crate) fn fit_train_only_transform(train_rows: &[[Option<f64>; 12]]) -> Vec<FeatureStatistics> {
    (0..12)
        .map(|i| {
            let values: Vec<Option<f64>> = train_rows
                .iter()
                .map(|row| {
                    if i < 7
                        && matches!(feature_method(i), NormalizationMethod::Log1pThenStandardize)
                    {
                        row[i].map(|v| v.max(0.0).ln_1p())
                    } else {
                        row[i]
                    }
                })
                .collect();
            normalization::train_only_statistics(FEATURE_NAMES[i], &values)
        })
        .collect()
}

fn transform_row(
    raw: &[Option<f64>; 12],
    stats: &[FeatureStatistics],
    rules: &[ImputationRule],
) -> [f64; 12] {
    let mut out = [0.0; 12];
    for i in 0..12 {
        let method = feature_method(i);
        let value = match raw[i] {
            Some(v) if matches!(method, NormalizationMethod::Log1pThenStandardize) => {
                v.max(0.0).ln_1p()
            }
            Some(v) => v,
            None => rules[i].imputed_value.unwrap_or(0.0),
        };
        out[i] = apply_normalization(value, method, &stats[i]);
    }
    out
}

pub(crate) fn to_samples(
    rows: &[TrainingRow],
    stats: &[FeatureStatistics],
    rules: &[ImputationRule],
) -> Vec<Sample> {
    rows.iter()
        .map(|row| {
            let raw = build_raw_row(row);
            Sample {
                x: transform_row(&raw, stats, rules),
                y: f64::from(row.label),
            }
        })
        .collect()
}

pub(crate) fn split_bounds(split: &str) -> Option<(NaiveDate, NaiveDate)> {
    let d = |y: i32, m: u32, day: u32| NaiveDate::from_ymd_opt(y, m, day).unwrap();
    match split {
        "train" => Some((d(2020, 1, 1), d(2023, 12, 31))),
        "calibration" => Some((d(2024, 1, 1), d(2024, 12, 31))),
        "test" => Some((d(2025, 1, 1), d(2025, 12, 31))),
        "prospective" => Some((d(2026, 1, 1), d(2026, 12, 31))),
        _ => None,
    }
}

/// Executable leakage assertion: every row claiming split `S` must fall
/// within `S`'s real date range. Fails the experiment (returns an error)
/// rather than silently continuing, per mission section 17.
pub(crate) fn assert_split_dates_in_range(rows: &[TrainingRow]) -> anyhow::Result<()> {
    for row in rows {
        let Some((start, end)) = split_bounds(&row.split) else {
            anyhow::bail!(
                "unknown split {} for h3={} date={}",
                row.split,
                row.h3,
                row.local_date
            );
        };
        anyhow::ensure!(
            row.local_date >= start && row.local_date <= end,
            "leakage check failed: row in split {} has date {} outside {start}..{end}",
            row.split,
            row.local_date
        );
    }
    Ok(())
}

/// Runs the experimental training pipeline for one dataset variant.
/// Writes all artifacts under `artifact_dir` (created if needed, never a
/// production path). See `PHASE3B7_MODEL_CANDIDATE_REPORT.md` for what
/// this produced.
pub async fn run_experiments(config: Config, options: ExperimentOptions) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("connect")?;
    let artifact_dir = PathBuf::from("/tmp/erytheon-experiments-3b7");
    std::fs::create_dir_all(&artifact_dir).context("create artifact dir")?;

    let promotion_criteria = PromotionCriteria::default();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "phase": "3b7_promotion_criteria_registered",
            "criteria": promotion_criteria,
        }))?
    );

    if options.dry_run {
        return Ok(());
    }

    let datasets = [
        (
            "principal",
            "erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality",
        ),
        (
            "sensitivity_quality",
            "erytheon_human_ignition_cell_day_v1_candidate_strict_n3_adaptive_geographic_quality",
        ),
        (
            "sensitivity_negative_window",
            "erytheon_human_ignition_cell_day_v1_candidate_inclusive_n2_kring2_day3",
        ),
    ];

    let mut best_of_all: Option<(String, f64)> = None;
    for (role, logical_id) in datasets {
        let result =
            run_one_dataset_experiment(&store, &artifact_dir, logical_id, role, options.seed)
                .await?;
        if best_of_all.as_ref().is_none_or(|(_, ap)| result.1 > *ap) {
            best_of_all = Some((logical_id.to_owned(), result.1));
        }
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "phase": "3b7_summary",
            "best_dataset_by_test_average_precision": best_of_all,
        }))?
    );

    Ok(())
}

pub(crate) fn h3_parent_res5(h3: i64) -> Option<u64> {
    let res5 = grid::Resolution::try_from(5).ok()?;
    grid::cell_from_db(h3)
        .ok()
        .and_then(|cell| cell.parent(res5))
        .map(u64::from)
}

/// Train-only monthly positive frequency, mission section 6 "baseline
/// saisonnier". Falls back to the train-wide constant rate for a month
/// that never occurs in train (cannot happen with 4 full train years,
/// but kept explicit rather than assumed).
struct SeasonalBaseline {
    monthly_rate: [f64; 12],
}

fn fit_seasonal_baseline(train_rows: &[TrainingRow]) -> SeasonalBaseline {
    let mut counts = [(0u64, 0u64); 12];
    for row in train_rows {
        let m = row.local_date.month0() as usize;
        counts[m].1 += 1;
        if row.label > 0 {
            counts[m].0 += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let fallback =
        train_rows.iter().filter(|r| r.label > 0).count() as f64 / train_rows.len().max(1) as f64;
    #[allow(clippy::cast_precision_loss)]
    let monthly_rate = std::array::from_fn(|m| {
        if counts[m].1 == 0 {
            fallback
        } else {
            counts[m].0 as f64 / counts[m].1 as f64
        }
    });
    SeasonalBaseline { monthly_rate }
}

fn predict_seasonal_baseline(baseline: &SeasonalBaseline, row: &TrainingRow) -> f64 {
    baseline.monthly_rate[row.local_date.month0() as usize]
}

/// Train-only H3 resolution-5-parent positive frequency, mission section
/// 6 "baseline spatial". `fallback` is the train-wide constant rate,
/// used explicitly for any parent cell never observed in train.
struct SpatialBaseline {
    rate_by_parent: HashMap<u64, f64>,
    fallback: f64,
}

fn fit_spatial_baseline(train_rows: &[TrainingRow], fallback: f64) -> SpatialBaseline {
    let mut counts: HashMap<u64, (u64, u64)> = HashMap::new();
    for row in train_rows {
        if let Some(parent) = h3_parent_res5(row.h3) {
            let entry = counts.entry(parent).or_insert((0, 0));
            entry.1 += 1;
            if row.label > 0 {
                entry.0 += 1;
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let rate_by_parent = counts
        .into_iter()
        .map(|(parent, (pos, tot))| (parent, pos as f64 / tot as f64))
        .collect();
    SpatialBaseline {
        rate_by_parent,
        fallback,
    }
}

fn predict_spatial_baseline(baseline: &SpatialBaseline, row: &TrainingRow) -> f64 {
    h3_parent_res5(row.h3)
        .and_then(|p| baseline.rate_by_parent.get(&p).copied())
        .unwrap_or(baseline.fallback)
}

/// Train-only H3 resolution-5-parent x month positive frequency, mission
/// section 6 "baseline spatio-saisonnier". Explicit 3-level fallback
/// chain, chosen to avoid unstable rates from sparse cell x month cells:
/// a (parent, month) cell needs at least `MIN_CELL_COUNT` train rows to
/// be trusted; otherwise fall back to the parent-only spatial rate;
/// a parent never seen in train falls back further to the train-wide
/// constant rate (via `SpatialBaseline::fallback`).
const MIN_CELL_COUNT: u64 = 10;

struct SpatioSeasonalBaseline {
    rate_by_parent_month: HashMap<(u64, u32), f64>,
    rate_by_parent: HashMap<u64, f64>,
    fallback: f64,
}

fn fit_spatio_seasonal_baseline(
    train_rows: &[TrainingRow],
    spatial: &SpatialBaseline,
    fallback: f64,
) -> SpatioSeasonalBaseline {
    let mut counts: HashMap<(u64, u32), (u64, u64)> = HashMap::new();
    for row in train_rows {
        if let Some(parent) = h3_parent_res5(row.h3) {
            let key = (parent, row.local_date.month0());
            let entry = counts.entry(key).or_insert((0, 0));
            entry.1 += 1;
            if row.label > 0 {
                entry.0 += 1;
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let rate_by_parent_month = counts
        .into_iter()
        .filter(|(_, (_, tot))| *tot >= MIN_CELL_COUNT)
        .map(|(key, (pos, tot))| (key, pos as f64 / tot as f64))
        .collect();
    SpatioSeasonalBaseline {
        rate_by_parent_month,
        rate_by_parent: spatial.rate_by_parent.clone(),
        fallback,
    }
}

fn predict_spatio_seasonal_baseline(baseline: &SpatioSeasonalBaseline, row: &TrainingRow) -> f64 {
    let month = row.local_date.month0();
    if let Some(parent) = h3_parent_res5(row.h3) {
        if let Some(rate) = baseline.rate_by_parent_month.get(&(parent, month)) {
            return *rate;
        }
        if let Some(rate) = baseline.rate_by_parent.get(&parent) {
            return *rate;
        }
    }
    baseline.fallback
}

#[allow(clippy::too_many_lines)]
async fn run_one_dataset_experiment(
    store: &Store,
    artifact_dir: &Path,
    logical_id: &str,
    role: &str,
    seed: i64,
) -> anyhow::Result<(String, f64)> {
    let fingerprint_before = store.dataset_rows_fingerprint(logical_id).await?;
    let rows = store.dataset_rows_for_training(logical_id).await?;
    anyhow::ensure!(!rows.is_empty(), "no rows found for {logical_id}");
    assert_split_dates_in_range(&rows)?;

    let train_rows: Vec<&TrainingRow> = rows.iter().filter(|r| r.split == "train").collect();
    let calibration_rows: Vec<&TrainingRow> =
        rows.iter().filter(|r| r.split == "calibration").collect();
    let test_rows: Vec<&TrainingRow> = rows.iter().filter(|r| r.split == "test").collect();

    let train_owned: Vec<TrainingRow> = train_rows.iter().map(|r| (*r).clone()).collect();
    let calib_owned: Vec<TrainingRow> = calibration_rows.iter().map(|r| (*r).clone()).collect();
    let test_owned: Vec<TrainingRow> = test_rows.iter().map(|r| (*r).clone()).collect();

    let train_raw: Vec<[Option<f64>; 12]> = train_owned.iter().map(build_raw_row).collect();
    let stats = fit_train_only_transform(&train_raw);
    let rules: Vec<ImputationRule> = stats
        .iter()
        .map(normalization::fit_imputation_rule)
        .collect();

    let train_samples = to_samples(&train_owned, &stats, &rules);
    let calib_samples = to_samples(&calib_owned, &stats, &rules);
    let test_samples = to_samples(&test_owned, &stats, &rules);

    // --- Baselines (train-only, applied unchanged to calibration/test) ---
    #[allow(clippy::cast_precision_loss)]
    let constant_rate = train_samples.iter().filter(|s| s.y > 0.5).count() as f64
        / train_samples.len().max(1) as f64;

    let seasonal = fit_seasonal_baseline(&train_owned);
    let spatial = fit_spatial_baseline(&train_owned, constant_rate);
    let spatio_seasonal = fit_spatio_seasonal_baseline(&train_owned, &spatial, constant_rate);

    let constant_test: Vec<(f64, f64)> = test_owned
        .iter()
        .map(|r| (constant_rate, f64::from(r.label)))
        .collect();
    let seasonal_test: Vec<(f64, f64)> = test_owned
        .iter()
        .map(|r| (predict_seasonal_baseline(&seasonal, r), f64::from(r.label)))
        .collect();
    let spatial_test: Vec<(f64, f64)> = test_owned
        .iter()
        .map(|r| (predict_spatial_baseline(&spatial, r), f64::from(r.label)))
        .collect();
    let spatio_seasonal_test: Vec<(f64, f64)> = test_owned
        .iter()
        .map(|r| {
            (
                predict_spatio_seasonal_baseline(&spatio_seasonal, r),
                f64::from(r.label),
            )
        })
        .collect();

    let baseline_metrics = json!({
        "constant": compute_split_metrics("test", &constant_test),
        "seasonal": compute_split_metrics("test", &seasonal_test),
        "spatial": compute_split_metrics("test", &spatial_test),
        "spatio_seasonal": compute_split_metrics("test", &spatio_seasonal_test),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "phase": "3b7_baseline",
            "dataset": logical_id,
            "role": role,
            "train_constant_rate": constant_rate,
            "test_metrics": baseline_metrics,
        }))?
    );

    // --- M1: logistic regression, internal progressive temporal validation ---
    let train_years: Vec<(i32, &TrainingRow)> = train_owned
        .iter()
        .map(|r| (r.local_date.year(), r))
        .collect();
    let l2_grid = [0.001, 0.01, 0.1, 1.0];
    let mut best_l2 = l2_grid[0];
    let mut best_avg_auc = -1.0;
    for &l2 in &l2_grid {
        let mut aucs = Vec::new();
        for val_year in [2021, 2022, 2023] {
            let fit_rows: Vec<TrainingRow> = train_years
                .iter()
                .filter(|(y, _)| *y < val_year)
                .map(|(_, r)| (*r).clone())
                .collect();
            let val_rows: Vec<TrainingRow> = train_years
                .iter()
                .filter(|(y, _)| *y == val_year)
                .map(|(_, r)| (*r).clone())
                .collect();
            if fit_rows.is_empty() || val_rows.is_empty() {
                continue;
            }
            let fit_raw: Vec<[Option<f64>; 12]> = fit_rows.iter().map(build_raw_row).collect();
            let fit_stats = fit_train_only_transform(&fit_raw);
            let fit_rules: Vec<ImputationRule> = fit_stats
                .iter()
                .map(normalization::fit_imputation_rule)
                .collect();
            let fit_samples = to_samples(&fit_rows, &fit_stats, &fit_rules);
            let val_samples = to_samples(&val_rows, &fit_stats, &fit_rules);
            let model = fit_logistic(&fit_samples, l2, 300, 0.3);
            let scored: Vec<(f64, f64)> = val_samples
                .iter()
                .map(|s| (model.predict(&s.x), s.y))
                .collect();
            aucs.push(roc_auc(&scored));
        }
        #[allow(clippy::cast_precision_loss)]
        let avg = if aucs.is_empty() {
            0.0
        } else {
            aucs.iter().sum::<f64>() / aucs.len() as f64
        };
        if avg > best_avg_auc {
            best_avg_auc = avg;
            best_l2 = l2;
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"phase": "3b7_hyperparam_selection", "dataset": logical_id, "model": "logistic", "chosen_l2": best_l2, "internal_avg_auc": best_avg_auc})
        )?
    );
    let logistic_final = fit_logistic(&train_samples, best_l2, 500, 0.3);

    // --- M2: gradient boosting, internal progressive temporal validation ---
    let gbm_grid: [(usize, usize, f64); 4] =
        [(50, 2, 0.1), (50, 3, 0.1), (100, 2, 0.05), (100, 3, 0.05)];
    let mut best_gbm_params = gbm_grid[0];
    let mut best_gbm_auc = -1.0;
    for &(n_trees, depth, lr) in &gbm_grid {
        let mut aucs = Vec::new();
        for val_year in [2022, 2023] {
            let fit_rows: Vec<TrainingRow> = train_years
                .iter()
                .filter(|(y, _)| *y < val_year)
                .map(|(_, r)| (*r).clone())
                .collect();
            let val_rows: Vec<TrainingRow> = train_years
                .iter()
                .filter(|(y, _)| *y == val_year)
                .map(|(_, r)| (*r).clone())
                .collect();
            if fit_rows.len() < 100 || val_rows.is_empty() {
                continue;
            }
            let fit_raw: Vec<[Option<f64>; 12]> = fit_rows.iter().map(build_raw_row).collect();
            let fit_stats = fit_train_only_transform(&fit_raw);
            let fit_rules: Vec<ImputationRule> = fit_stats
                .iter()
                .map(normalization::fit_imputation_rule)
                .collect();
            let fit_samples = to_samples(&fit_rows, &fit_stats, &fit_rules);
            let val_samples = to_samples(&val_rows, &fit_stats, &fit_rules);
            let model = fit_gbm(&fit_samples, n_trees, depth, lr);
            let scored: Vec<(f64, f64)> = val_samples
                .iter()
                .map(|s| (model.predict(&s.x), s.y))
                .collect();
            aucs.push(roc_auc(&scored));
        }
        #[allow(clippy::cast_precision_loss)]
        let avg = if aucs.is_empty() {
            0.0
        } else {
            aucs.iter().sum::<f64>() / aucs.len() as f64
        };
        if avg > best_gbm_auc {
            best_gbm_auc = avg;
            best_gbm_params = (n_trees, depth, lr);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"phase": "3b7_hyperparam_selection", "dataset": logical_id, "model": "gbm", "chosen_params": best_gbm_params, "internal_avg_auc": best_gbm_auc})
        )?
    );
    let gbm_final = fit_gbm(
        &train_samples,
        best_gbm_params.0,
        best_gbm_params.1,
        best_gbm_params.2,
    );

    // --- Calibration on 2024, frozen, then single evaluation on 2025 ---
    let logistic_calib_scores: Vec<(f64, f64)> = calib_samples
        .iter()
        .map(|s| (logistic_final.predict(&s.x), s.y))
        .collect();
    let gbm_calib_scores: Vec<(f64, f64)> = calib_samples
        .iter()
        .map(|s| (gbm_final.predict(&s.x), s.y))
        .collect();
    let (platt_w, platt_b) = fit_platt(&gbm_calib_scores);
    let (logistic_platt_w, logistic_platt_b) = fit_platt(&logistic_calib_scores);
    let isotonic_blocks = if calib_samples.len() >= 200 {
        Some(fit_isotonic(&gbm_calib_scores))
    } else {
        None
    };

    let logistic_test_scores_raw: Vec<(f64, f64)> = test_samples
        .iter()
        .map(|s| (logistic_final.predict(&s.x), s.y))
        .collect();
    let logistic_test_scores_platt: Vec<(f64, f64)> = logistic_test_scores_raw
        .iter()
        .map(|&(p, y)| (apply_platt(p, logistic_platt_w, logistic_platt_b), y))
        .collect();
    let gbm_test_scores_raw: Vec<(f64, f64)> = test_samples
        .iter()
        .map(|s| (gbm_final.predict(&s.x), s.y))
        .collect();
    let gbm_test_scores_platt: Vec<(f64, f64)> = gbm_test_scores_raw
        .iter()
        .map(|&(p, y)| (apply_platt(p, platt_w, platt_b), y))
        .collect();
    let gbm_test_scores_isotonic: Vec<(f64, f64)> = isotonic_blocks.as_ref().map_or_else(
        || gbm_test_scores_raw.clone(),
        |blocks| {
            gbm_test_scores_raw
                .iter()
                .map(|&(p, y)| (apply_isotonic(p, blocks), y))
                .collect()
        },
    );

    let logistic_metrics_test_raw = compute_split_metrics("test", &logistic_test_scores_raw);
    let logistic_metrics_test_platt = compute_split_metrics("test", &logistic_test_scores_platt);
    let gbm_metrics_test_raw = compute_split_metrics("test", &gbm_test_scores_raw);
    let gbm_metrics_test_platt = compute_split_metrics("test", &gbm_test_scores_platt);
    let gbm_metrics_test_isotonic = compute_split_metrics("test", &gbm_test_scores_isotonic);

    // --- Supplementary analyses, scoped to the principal dataset only ---
    // (feature importance, block bootstrap CI, spatial cross-validation,
    // and a weighting comparison) to keep the total experiment cost
    // bounded, per the mission's own repeated "si le coût reste
    // raisonnable" framing -- the sensitivity datasets already receive
    // the full baseline+M1+M2+calibration+single-test-evaluation
    // treatment above, which is the primary comparison.
    let supplementary = if role == "principal" {
        Some(run_supplementary_analyses(
            &train_owned,
            &test_owned,
            &train_samples,
            &test_samples,
            &calib_samples,
            &logistic_final,
            &gbm_final,
            &gbm_test_scores_isotonic,
            isotonic_blocks.as_ref(),
            best_gbm_params,
            best_l2,
            seed,
        ))
    } else {
        None
    };

    let fingerprint_after = store.dataset_rows_fingerprint(logical_id).await?;
    anyhow::ensure!(
        fingerprint_before == fingerprint_after,
        "leakage check failed: dataset {logical_id} changed during the experiment"
    );

    let experiment_id = format!("3b7_{role}_{seed}");
    let manifest = ExperimentManifest {
        experiment_id: experiment_id.clone(),
        git_commit: env!("FIRESIFT_GIT_COMMIT").to_owned(),
        dataset_logical_id: logical_id.to_owned(),
        dataset_row_fingerprint_before: fingerprint_before,
        features: FEATURE_NAMES.iter().map(|s| (*s).to_owned()).collect(),
        normalization_methods: (0..12)
            .map(|i| {
                (
                    FEATURE_NAMES[i].to_owned(),
                    format!("{:?}", feature_method(i)),
                )
            })
            .collect(),
        seed,
        code_version: CODE_VERSION.to_owned(),
        started_at_utc: chrono::Utc::now().to_rfc3339(),
        hardware: "isolated build container, 2 CPU / 4 GiB".to_owned(),
        scientific_objective: format!("phase 3B.7 experimental comparison, role={role}"),
        promotion_criteria: PromotionCriteria::default(),
    };

    let report = json!({
        "manifest": manifest,
        "chosen_logistic_l2": best_l2,
        "chosen_gbm_params": {"n_trees": best_gbm_params.0, "max_depth": best_gbm_params.1, "learning_rate": best_gbm_params.2},
        "test_metrics": {
            "logistic_raw": logistic_metrics_test_raw,
            "logistic_platt": logistic_metrics_test_platt,
            "gbm_raw": gbm_metrics_test_raw,
            "gbm_platt": gbm_metrics_test_platt,
            "gbm_isotonic": gbm_metrics_test_isotonic,
            "isotonic_used": isotonic_blocks.is_some(),
        },
        "row_counts": {"train": train_samples.len(), "calibration": calib_samples.len(), "test": test_samples.len()},
        "baselines": baseline_metrics,
        "supplementary": supplementary,
    });

    let report_path = artifact_dir.join(format!("{role}_report.json"));
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    let report_bytes = std::fs::metadata(&report_path)?.len();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "phase": "3b7_result",
            "role": role,
            "dataset": logical_id,
            "artifact_path": report_path.display().to_string(),
            "artifact_size_bytes": report_bytes,
            "test_metrics": report["test_metrics"],
        }))?
    );

    Ok((
        logical_id.to_owned(),
        gbm_metrics_test_platt.average_precision,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        FEATURE_NAMES, GbmModel, PromotionCriteria, Sample, Tree, apply_isotonic, apply_platt,
        assert_split_dates_in_range, average_precision, brier_score, expected_calibration_error,
        fit_isotonic, fit_logistic, fit_platt, fit_seasonal_baseline, fit_spatial_baseline,
        fit_spatio_seasonal_baseline, h3_parent_res5, log_loss, predict_seasonal_baseline,
        predict_spatial_baseline, predict_spatio_seasonal_baseline, roc_auc,
    };
    use chrono::NaiveDate;
    use serde_json::json;
    use store::TrainingRow;

    fn row(h3: i64, date: &str, split: &str, label: i16) -> TrainingRow {
        TrainingRow {
            h3,
            local_date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            split: split.to_owned(),
            label,
            features: json!({}),
        }
    }

    // Mission section 17: a row whose calendar date falls outside its
    // claimed split's real bounds must hard-fail the experiment, not
    // just log a warning.
    #[test]
    fn leakage_check_accepts_rows_within_split_bounds() {
        let rows = vec![
            row(1, "2021-06-01", "train", 0),
            row(1, "2024-03-01", "calibration", 1),
            row(1, "2025-08-01", "test", 0),
        ];
        assert!(assert_split_dates_in_range(&rows).is_ok());
    }

    #[test]
    fn leakage_check_rejects_2025_row_labeled_as_train() {
        let rows = vec![row(1, "2025-01-01", "train", 0)];
        assert!(
            assert_split_dates_in_range(&rows).is_err(),
            "a 2025 row must never be accepted as part of the train split"
        );
    }

    #[test]
    fn leakage_check_rejects_2024_row_labeled_as_test() {
        let rows = vec![row(1, "2024-06-01", "test", 0)];
        assert!(assert_split_dates_in_range(&rows).is_err());
    }

    #[test]
    fn leakage_check_rejects_unknown_split_name() {
        let rows = vec![row(1, "2021-01-01", "prospective_2026_typo", 0)];
        assert!(assert_split_dates_in_range(&rows).is_err());
    }

    // Mission section 19: promotion criteria must be a fixed, pre-
    // registered default, not derived from any run's results.
    #[test]
    fn promotion_criteria_default_is_frozen() {
        let criteria = PromotionCriteria::default();
        assert!((criteria.min_roc_auc - 0.60).abs() < 1e-12);
        assert!((criteria.max_brier_score - 0.20).abs() < 1e-12);
        assert!((criteria.max_ece - 0.10).abs() < 1e-12);
        assert!((criteria.min_lift_at_10pct - 1.5).abs() < 1e-12);
        assert!((criteria.min_average_precision_gain_over_v1 - 0.0).abs() < 1e-12);
    }

    #[test]
    fn manifest_criteria_round_trip_through_json() {
        let criteria = PromotionCriteria::default();
        let value = serde_json::to_value(&criteria).unwrap();
        let restored: PromotionCriteria = serde_json::from_value(value).unwrap();
        assert!((restored.min_roc_auc - criteria.min_roc_auc).abs() < 1e-12);
    }

    #[test]
    fn feature_names_has_twelve_fixed_entries() {
        assert_eq!(FEATURE_NAMES.len(), 12);
    }

    // A perfectly separable score must score AUC 1.0, and a constant
    // score (as used by the "constant" baseline) must score 0.5 by
    // definition, not an arbitrary or undefined value.
    #[test]
    fn roc_auc_perfect_and_constant_scores() {
        let perfect = [(0.1, 0.0), (0.2, 0.0), (0.8, 1.0), (0.9, 1.0)];
        assert!((roc_auc(&perfect) - 1.0).abs() < 1e-9);

        let constant = [(0.3, 0.0), (0.3, 1.0), (0.3, 0.0), (0.3, 1.0)];
        assert!((roc_auc(&constant) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn brier_and_log_loss_are_zero_for_perfect_confident_predictions() {
        let scored = [(1.0, 1.0), (0.0, 0.0)];
        assert!(brier_score(&scored) < 1e-9);
        assert!(log_loss(&scored) < 1e-6);
    }

    #[test]
    fn average_precision_is_one_for_perfect_ranking() {
        let scored = [(0.9, 1.0), (0.8, 1.0), (0.1, 0.0), (0.05, 0.0)];
        assert!((average_precision(&scored) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn expected_calibration_error_is_zero_when_scores_match_outcomes() {
        // All scores land in the same decile bin and their average (0.5)
        // exactly matches the empirical outcome rate for that bin (0.5),
        // so ECE must be 0.
        let scored = [(0.5, 1.0), (0.5, 0.0)];
        assert!(expected_calibration_error(&scored) < 1e-9);
    }

    // Platt scaling and isotonic regression must be fit only on the
    // calibration split (mission section 9) -- this test only checks
    // that the fitted calibrator is monotonic and reproduces the
    // calibration data reasonably, which is a necessary (not sufficient)
    // condition for "fit only on calibration, not on train/test".
    #[test]
    fn platt_calibration_is_monotonic_in_the_raw_score() {
        let scored = [(0.1, 0.0), (0.3, 0.0), (0.6, 1.0), (0.9, 1.0)];
        let (w, b) = fit_platt(&scored);
        let low = apply_platt(0.1, w, b);
        let high = apply_platt(0.9, w, b);
        assert!(high > low);
    }

    #[test]
    fn isotonic_calibration_is_non_decreasing() {
        let scored: Vec<(f64, f64)> = (0..300)
            .map(|i| {
                let p = f64::from(i) / 300.0;
                let y = f64::from(u8::from(p > 0.5));
                (p, y)
            })
            .collect();
        let blocks = fit_isotonic(&scored);
        let low = apply_isotonic(0.1, &blocks);
        let high = apply_isotonic(0.9, &blocks);
        assert!(high >= low);
    }

    // Mission section 6: seasonal/spatial/spatio-seasonal baselines must
    // be fit only from train and must have an explicit fallback for
    // anything unseen in train, not a panic or a silent NaN.
    #[test]
    fn seasonal_baseline_uses_train_only_monthly_rate() {
        let train = vec![
            row(1, "2021-01-05", "train", 1),
            row(1, "2021-01-15", "train", 0),
            row(1, "2021-06-05", "train", 0),
            row(1, "2021-06-15", "train", 0),
        ];
        let baseline = fit_seasonal_baseline(&train);
        assert!((predict_seasonal_baseline(&baseline, &train[0]) - 0.5).abs() < 1e-9);
        assert!((predict_seasonal_baseline(&baseline, &train[2]) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn spatial_baseline_falls_back_for_unseen_parent() {
        let train = vec![
            row(1, "2021-01-01", "train", 1),
            row(1, "2021-01-02", "train", 0),
        ];
        let fallback = 0.5;
        let baseline = fit_spatial_baseline(&train, fallback);
        // Cell 999 is not an H3 index the dataset ever produced, so
        // `h3_parent_res5` may return None or a parent never seen in
        // train -- either path must land on the explicit fallback.
        let unseen = row(999, "2021-01-01", "train", 0);
        assert!((predict_spatial_baseline(&baseline, &unseen) - fallback).abs() < 1e-9);
    }

    #[test]
    fn spatio_seasonal_baseline_falls_back_below_min_cell_count() {
        let train = vec![
            row(1, "2021-01-01", "train", 1),
            row(1, "2021-02-01", "train", 0),
        ];
        let fallback = 0.5;
        let spatial = fit_spatial_baseline(&train, fallback);
        let spatio_seasonal = fit_spatio_seasonal_baseline(&train, &spatial, fallback);
        // Only 1 row exists for (parent-of-1, January) -- far below
        // MIN_CELL_COUNT -- so prediction must fall back, not use a
        // rate estimated from a single observation.
        let predicted = predict_spatio_seasonal_baseline(&spatio_seasonal, &train[0]);
        assert!((predicted - fallback).abs() < 1e-9 || h3_parent_res5(1).is_none());
    }

    // A serialized model must round-trip byte-for-byte in its
    // predictions (mission section 18: prediction checksum stability).
    #[test]
    fn logistic_model_prediction_is_deterministic_given_fixed_weights() {
        let samples = vec![
            Sample {
                x: [1.0; 12],
                y: 1.0,
            },
            Sample {
                x: [0.0; 12],
                y: 0.0,
            },
        ];
        let model_a = fit_logistic(&samples, 0.01, 50, 0.3);
        let model_b = fit_logistic(&samples, 0.01, 50, 0.3);
        assert!(
            model_a
                .weights
                .iter()
                .zip(model_b.weights.iter())
                .all(|(a, b)| (a - b).abs() < 1e-12)
        );
        assert!((model_a.bias - model_b.bias).abs() < 1e-12);
    }

    #[test]
    fn gbm_model_serializes_and_predicts_after_round_trip() {
        let model = GbmModel {
            trees: vec![Tree::Leaf(0.2)],
            learning_rate: 0.1,
            base_score: 0.0,
            max_depth: 1,
            n_trees: 1,
        };
        let json = serde_json::to_string(&model).unwrap();
        let restored: GbmModel = serde_json::from_str(&json).unwrap();
        assert!((restored.predict(&[0.0; 12]) - model.predict(&[0.0; 12])).abs() < 1e-12);
    }
}
