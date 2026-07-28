//! Phase 3B.8: a faithful, paired comparison between the active v1
//! model's learned human-ignition component and the frozen phase 3B.7
//! GBM+isotonic candidate, on exactly the same 2025 rows. Never
//! retrains v1, never touches serving/API, never writes a new
//! operational score. All artifacts are written to an isolated,
//! disposable directory (never a production volume).

use std::collections::HashMap;

use anyhow::Context;
use dataset::normalization;
use risk::{CellFeatures, LearnedHumanModel};
use serde::Serialize;
use serde_json::json;
use store::{Store, TrainingRow};

use crate::config::Config;
use crate::model_experiments::{
    self, average_precision, brier_score, build_raw_row, compute_split_metrics,
    expected_calibration_error, fit_isotonic, fit_train_only_transform, log_loss, mix64_local,
    precision_recall_lift_at_k, roc_auc, to_samples,
};

const PRINCIPAL_LOGICAL_ID: &str =
    "erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality";
const STRICT_N2_LOGICAL_ID: &str =
    "erytheon_human_ignition_cell_day_v1_candidate_strict_n2_kring2_day3";
const FROZEN_GBM: (usize, usize, f64) = (50, 3, 0.1);
const EXPECTED_PRINCIPAL_TEST: (f64, f64, f64, f64) = (0.9764, 0.9308, 0.0460, 0.0096);

#[derive(Clone, Copy, Debug)]
pub struct ComparisonOptions {
    pub seed: i64,
}

fn get_f64(v: &serde_json::Value, name: &str) -> Option<f64> {
    v.get(name).and_then(serde_json::Value::as_f64)
}

fn get_bool(v: &serde_json::Value, name: &str) -> Option<bool> {
    v.get(name).and_then(serde_json::Value::as_bool)
}

/// Whether all fields the v1 learned human model needs are present in a
/// candidate row's raw feature JSON. `school_holiday` is deliberately
/// excluded from this check: it is universally absent (`unavailable_
/// historically`, phase 3B.6 §6), and is bridged via the exact same
/// `COALESCE(school_holiday, FALSE)` convention v1's own production
/// serving query already uses (`Store::risk_inputs`) -- not a new
/// assumption introduced by this phase.
fn v1_reconstructable(row: &TrainingRow) -> bool {
    let f = &row.features;
    get_f64(f, "wui").is_some()
        && get_f64(f, "road").is_some()
        && get_f64(f, "agri").is_some()
        && get_f64(f, "population").is_some()
        && get_f64(f, "poi").is_some()
        && get_f64(f, "power_line").is_some()
        && get_bool(f, "combustible").is_some()
        && get_bool(f, "public_holiday").is_some()
}

/// Reconstructs the exact `risk::CellFeatures` v1's learned human
/// component needs from one candidate dataset row. `fwi` is set to 0.0:
/// `LearnedHumanModel::predict` never reads it (only the full
/// `HeuristicV1::score` fusion does, and this phase deliberately scores
/// the learned human component alone -- see `V1_CANDIDATE_COMPARISON.md`
/// for why comparing the fused score would confound FWI into a
/// candidate that has no FWI feature at all).
#[allow(clippy::cast_possible_truncation)]
fn v1_cell_features(row: &TrainingRow) -> CellFeatures {
    let f = &row.features;
    CellFeatures {
        fwi: 0.0,
        hist: get_f64(f, "hist").unwrap_or(0.0) as f32,
        wui: get_f64(f, "wui").unwrap_or(0.0) as f32,
        road: get_f64(f, "road").unwrap_or(0.0) as f32,
        agri: get_f64(f, "agri").unwrap_or(0.0) as f32,
        population: get_f64(f, "population").unwrap_or(0.0) as f32,
        poi: get_f64(f, "poi").unwrap_or(0.0) as f32,
        power_line: get_f64(f, "power_line").unwrap_or(0.0) as f32,
        combustible: get_bool(f, "combustible").unwrap_or(false),
        date: row.local_date,
        school_holiday: false,
        public_holiday: get_bool(f, "public_holiday").unwrap_or(false),
    }
}

#[derive(Debug, Serialize)]
struct PopulationReport {
    total_rows: usize,
    v1_comparable: usize,
    v1_missing_features: usize,
    comparable_positive_count: usize,
    comparable_negative_count: usize,
    missing_positive_count: usize,
    missing_negative_count: usize,
}

fn build_population_report(rows: &[TrainingRow]) -> (PopulationReport, Vec<usize>) {
    let mut comparable_idx = Vec::new();
    let mut missing_idx = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if v1_reconstructable(row) {
            comparable_idx.push(i);
        } else {
            missing_idx.push(i);
        }
    }
    let count_positive = |idx: &[usize]| idx.iter().filter(|&&i| rows[i].label > 0).count();
    let report = PopulationReport {
        total_rows: rows.len(),
        v1_comparable: comparable_idx.len(),
        v1_missing_features: missing_idx.len(),
        comparable_positive_count: count_positive(&comparable_idx),
        comparable_negative_count: comparable_idx.len() - count_positive(&comparable_idx),
        missing_positive_count: count_positive(&missing_idx),
        missing_negative_count: missing_idx.len() - count_positive(&missing_idx),
    };
    (report, comparable_idx)
}

/// Retrains the frozen phase 3B.7 GBM+isotonic candidate exactly (no new
/// hyperparameter search, no new seed dependence -- GBM/isotonic fitting
/// here is fully deterministic given fixed data) on one dataset's train/
/// calibration/test split, returning per-row test scores in the same
/// order as the input `test_rows`, plus the reproduced test-split
/// metrics for a reproducibility check.
fn replay_frozen_gbm_candidate(
    train_rows: &[TrainingRow],
    calib_rows: &[TrainingRow],
    test_rows: &[TrainingRow],
) -> (Vec<f64>, model_experiments::SplitMetrics) {
    let train_raw: Vec<[Option<f64>; 12]> = train_rows.iter().map(build_raw_row).collect();
    let stats = fit_train_only_transform(&train_raw);
    let rules: Vec<_> = stats
        .iter()
        .map(normalization::fit_imputation_rule)
        .collect();

    let train_samples = to_samples(train_rows, &stats, &rules);
    let calib_samples = to_samples(calib_rows, &stats, &rules);
    let test_samples = to_samples(test_rows, &stats, &rules);

    let gbm = model_experiments::fit_gbm(&train_samples, FROZEN_GBM.0, FROZEN_GBM.1, FROZEN_GBM.2);
    let calib_scores: Vec<(f64, f64)> = calib_samples
        .iter()
        .map(|s| (gbm.predict(&s.x), s.y))
        .collect();
    let isotonic_blocks = fit_isotonic(&calib_scores);
    let test_scores: Vec<(f64, f64)> = test_samples
        .iter()
        .map(|s| {
            (
                model_experiments::apply_isotonic(gbm.predict(&s.x), &isotonic_blocks),
                s.y,
            )
        })
        .collect();
    let metrics = compute_split_metrics("test", &test_scores);
    let raw_scores: Vec<f64> = test_scores.iter().map(|&(p, _)| p).collect();
    (raw_scores, metrics)
}

#[derive(Debug, Serialize)]
struct PairedBootstrap {
    rounds: usize,
    ap_diff_95pct_ci: (f64, f64),
    roc_auc_diff_95pct_ci: (f64, f64),
    ap_diff_mean: f64,
}

fn paired_block_bootstrap(
    dates: &[chrono::NaiveDate],
    v1_scores: &[(f64, f64)],
    candidate_scores: &[(f64, f64)],
    seed: i64,
) -> PairedBootstrap {
    const ROUNDS: usize = 200;
    let mut unique_dates: Vec<chrono::NaiveDate> = dates.to_vec();
    unique_dates.sort_unstable();
    unique_dates.dedup();
    let mut by_date: HashMap<chrono::NaiveDate, Vec<usize>> = HashMap::new();
    for (i, date) in dates.iter().enumerate() {
        by_date.entry(*date).or_default().push(i);
    }
    let mut ap_diffs = Vec::with_capacity(ROUNDS);
    let mut auc_diffs = Vec::with_capacity(ROUNDS);
    for round in 0..ROUNDS {
        let mut idx = Vec::new();
        for i in 0..unique_dates.len() {
            let h = mix64_local(
                seed.unsigned_abs() ^ (round as u64).wrapping_mul(0x9E37_79B9) ^ i as u64,
            );
            #[allow(clippy::cast_possible_truncation)]
            let picked = unique_dates[(h as usize) % unique_dates.len()];
            if let Some(indices) = by_date.get(&picked) {
                idx.extend(indices.iter().copied());
            }
        }
        if idx.is_empty() {
            continue;
        }
        let v1_resampled: Vec<(f64, f64)> = idx.iter().map(|&i| v1_scores[i]).collect();
        let cand_resampled: Vec<(f64, f64)> = idx.iter().map(|&i| candidate_scores[i]).collect();
        ap_diffs.push(average_precision(&cand_resampled) - average_precision(&v1_resampled));
        auc_diffs.push(roc_auc(&cand_resampled) - roc_auc(&v1_resampled));
    }
    let ci = |values: &mut Vec<f64>| -> (f64, f64) {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if values.is_empty() {
            return (0.0, 0.0);
        }
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let lo = (values.len() as f64 * 0.025) as usize;
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let hi = ((values.len() as f64 * 0.975) as usize).min(values.len() - 1);
        (values[lo], values[hi])
    };
    #[allow(clippy::cast_precision_loss)]
    let ap_diff_mean = if ap_diffs.is_empty() {
        0.0
    } else {
        ap_diffs.iter().sum::<f64>() / ap_diffs.len() as f64
    };
    let ap_ci = ci(&mut ap_diffs.clone());
    let auc_ci = ci(&mut auc_diffs.clone());
    PairedBootstrap {
        rounds: ROUNDS,
        ap_diff_95pct_ci: ap_ci,
        roc_auc_diff_95pct_ci: auc_ci,
        ap_diff_mean,
    }
}

#[derive(Debug, Serialize)]
struct TopKOverlap {
    fraction: f64,
    v1_positives_captured: usize,
    candidate_positives_captured: usize,
    positives_captured_by_both: usize,
    positives_only_by_v1: usize,
    positives_only_by_candidate: usize,
}

fn top_k_overlap(
    v1_scores: &[(f64, f64)],
    candidate_scores: &[(f64, f64)],
    fraction: f64,
) -> TopKOverlap {
    let n = v1_scores.len();
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let k = ((n as f64) * fraction).ceil() as usize;
    let top = |scores: &[(f64, f64)]| -> std::collections::HashSet<usize> {
        let mut idx: Vec<usize> = (0..scores.len()).collect();
        idx.sort_by(|&a, &b| {
            scores[b]
                .0
                .partial_cmp(&scores[a].0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        idx.into_iter().take(k).collect()
    };
    let v1_top = top(v1_scores);
    let cand_top = top(candidate_scores);
    let positive_in = |set: &std::collections::HashSet<usize>| {
        set.iter().filter(|&&i| v1_scores[i].1 > 0.5).count()
    };
    let both: std::collections::HashSet<usize> = v1_top.intersection(&cand_top).copied().collect();
    TopKOverlap {
        fraction,
        v1_positives_captured: positive_in(&v1_top),
        candidate_positives_captured: positive_in(&cand_top),
        positives_captured_by_both: positive_in(&both),
        positives_only_by_v1: v1_top
            .difference(&cand_top)
            .filter(|&&i| v1_scores[i].1 > 0.5)
            .count(),
        positives_only_by_candidate: cand_top
            .difference(&v1_top)
            .filter(|&&i| v1_scores[i].1 > 0.5)
            .count(),
    }
}

/// Feature-level characterization of the two "large disagreement"
/// buckets (mission §12): v1-high/candidate-low and v1-low/candidate-
/// high, using each score's own rank percentile (not raw magnitude,
/// since v1 and the candidate are different models with different
/// score distributions).
fn disagreement_summary(
    rows: &[TrainingRow],
    v1_scores: &[f64],
    candidate_scores: &[f64],
) -> serde_json::Value {
    let n = rows.len();
    let rank_percentile = |scores: &[f64]| -> Vec<f64> {
        let mut idx: Vec<usize> = (0..n).collect();
        idx.sort_by(|&a, &b| {
            scores[a]
                .partial_cmp(&scores[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut pct = vec![0.0; n];
        for (rank, &i) in idx.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            {
                pct[i] = rank as f64 / n.max(1) as f64;
            }
        }
        pct
    };
    let v1_pct = rank_percentile(v1_scores);
    let cand_pct = rank_percentile(candidate_scores);

    let mut v1_high_cand_low = Vec::new();
    let mut v1_low_cand_high = Vec::new();
    for i in 0..n {
        let diff = v1_pct[i] - cand_pct[i];
        if diff > 0.5 {
            v1_high_cand_low.push(i);
        } else if diff < -0.5 {
            v1_low_cand_high.push(i);
        }
    }

    let summarize = |idx: &[usize]| -> serde_json::Value {
        if idx.is_empty() {
            return json!({"n": 0});
        }
        let mean = |f: fn(&serde_json::Value) -> Option<f64>| -> f64 {
            let values: Vec<f64> = idx.iter().filter_map(|&i| f(&rows[i].features)).collect();
            #[allow(clippy::cast_precision_loss)]
            if values.is_empty() {
                0.0
            } else {
                values.iter().sum::<f64>() / values.len() as f64
            }
        };
        #[allow(clippy::cast_precision_loss)]
        let positive_rate =
            idx.iter().filter(|&&i| rows[i].label > 0).count() as f64 / idx.len() as f64;
        #[allow(clippy::cast_precision_loss)]
        let combustible_rate = idx
            .iter()
            .filter(|&&i| get_bool(&rows[i].features, "combustible").unwrap_or(false))
            .count() as f64
            / idx.len() as f64;
        json!({
            "n": idx.len(),
            "positive_rate": positive_rate,
            "mean_hist": mean(|f| get_f64(f, "hist")),
            "mean_road": mean(|f| get_f64(f, "road")),
            "mean_agri": mean(|f| get_f64(f, "agri")),
            "mean_population": mean(|f| get_f64(f, "population")),
            "mean_wui": mean(|f| get_f64(f, "wui")),
            "combustible_rate": combustible_rate,
        })
    };
    json!({
        "v1_high_candidate_low": summarize(&v1_high_cand_low),
        "v1_low_candidate_high": summarize(&v1_low_cand_high),
    })
}

/// Combustibility-rule sensitivity (mission §16): an analytic join
/// against `cell_static`'s already-loaded resolution-9 children, no
/// dataset rebuild. For each rule, measures how many of the *same*
/// comparable test rows would remain eligible, without retraining or
/// rescoring anything -- eligibility only.
async fn combustibility_sensitivity(
    store: &Store,
    rows: &[TrainingRow],
) -> anyhow::Result<serde_json::Value> {
    let res9_rows = store
        .all_cell_static_rows()
        .await
        .context("load cell_static for combustibility sensitivity")?;
    let res8 =
        grid::Resolution::try_from(8).map_err(|_| anyhow::anyhow!("resolution 8 unavailable"))?;
    let mut children_by_parent: HashMap<i64, Vec<bool>> = HashMap::new();
    for row in &res9_rows {
        if let Some(parent) = row.cell.parent(res8) {
            let combustible = row
                .features
                .get("combustible")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            children_by_parent
                .entry(grid::cell_to_db(parent))
                .or_default()
                .push(combustible);
        }
    }

    let rule_eligible = |h3: i64, threshold: f64, require_majority: bool| -> bool {
        children_by_parent.get(&h3).is_some_and(|children| {
            if children.is_empty() {
                return false;
            }
            #[allow(clippy::cast_precision_loss)]
            let proportion = children.iter().filter(|&&c| c).count() as f64 / children.len() as f64;
            if require_majority {
                proportion > 0.5
            } else {
                proportion >= threshold
            }
        })
    };

    let mut out = serde_json::Map::new();
    for (name, threshold, majority) in [
        ("majority", 0.5, true),
        ("proportion_ge_50pct", 0.5, false),
        ("proportion_ge_75pct", 0.75, false),
    ] {
        let eligible: Vec<&TrainingRow> = rows
            .iter()
            .filter(|r| rule_eligible(r.h3, threshold, majority))
            .collect();
        let positives = eligible.iter().filter(|r| r.label > 0).count();
        out.insert(
            name.to_owned(),
            json!({
                "rows_retained": eligible.len(),
                "rows_excluded": rows.len() - eligible.len(),
                "positives_retained": positives,
                "negatives_retained": eligible.len() - positives,
            }),
        );
    }
    out.insert(
        "any_child_current".to_owned(),
        json!({"rows_retained": rows.len(), "rows_excluded": 0, "positives_retained": rows.iter().filter(|r| r.label > 0).count(), "negatives_retained": rows.iter().filter(|r| r.label <= 0).count()}),
    );
    Ok(serde_json::Value::Object(out))
}

#[allow(clippy::too_many_lines)]
pub async fn run_v1_comparison(config: Config, options: ComparisonOptions) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("connect to isolated database")?;
    let artifact_dir = std::path::PathBuf::from("/tmp/erytheon-experiments-3b8");
    std::fs::create_dir_all(&artifact_dir).context("create artifact dir")?;

    let Some(active) = store
        .active_human_model()
        .await
        .context("load active human model")?
    else {
        let verdict = json!({
            "phase": "3b8_verdict",
            "verdict": "V1_COMPARISON_NOT_SCIENTIFICALLY_VALID",
            "reason": "no active v1 human model artifact found in human_model_versions",
        });
        println!("{}", serde_json::to_string_pretty(&verdict)?);
        return Ok(());
    };
    let v1_model: LearnedHumanModel = serde_json::from_value(active.artifact.clone())
        .context("deserialize active v1 artifact")?;
    v1_model
        .validate()
        .context("active v1 artifact failed validation")?;

    let fingerprint_before = store.dataset_rows_fingerprint(PRINCIPAL_LOGICAL_ID).await?;
    let rows = store
        .dataset_rows_for_training(PRINCIPAL_LOGICAL_ID)
        .await?;
    anyhow::ensure!(!rows.is_empty(), "no rows found for {PRINCIPAL_LOGICAL_ID}");
    model_experiments::assert_split_dates_in_range(&rows)?;

    let train_rows: Vec<TrainingRow> = rows
        .iter()
        .filter(|r| r.split == "train")
        .cloned()
        .collect();
    let calib_rows: Vec<TrainingRow> = rows
        .iter()
        .filter(|r| r.split == "calibration")
        .cloned()
        .collect();
    let test_rows: Vec<TrainingRow> = rows.iter().filter(|r| r.split == "test").cloned().collect();

    // --- Reproduce the frozen candidate exactly, no new hyperparameter search ---
    let (candidate_scores, reproduced_metrics) =
        replay_frozen_gbm_candidate(&train_rows, &calib_rows, &test_rows);
    let reproduction_check = json!({
        "expected": {"roc_auc": EXPECTED_PRINCIPAL_TEST.0, "average_precision": EXPECTED_PRINCIPAL_TEST.1, "brier_score": EXPECTED_PRINCIPAL_TEST.2, "ece": EXPECTED_PRINCIPAL_TEST.3},
        "reproduced": {"roc_auc": reproduced_metrics.roc_auc, "average_precision": reproduced_metrics.average_precision, "brier_score": reproduced_metrics.brier_score, "ece": reproduced_metrics.ece},
        // The phase 3B.7 report rounded metrics to 4 decimal places for
        // human-readable tables; this replay computes full precision, so
        // the documented tolerance is against that rounding, not a claim
        // of bit-identical floating point reproduction.
        "tolerance_note": "expected values are 4-decimal-place-rounded from PHASE3B7_MODEL_CANDIDATE_REPORT.md; reproduced values are full precision",
        "roc_auc_matches": (reproduced_metrics.roc_auc - EXPECTED_PRINCIPAL_TEST.0).abs() < 5e-4,
        "average_precision_matches": (reproduced_metrics.average_precision - EXPECTED_PRINCIPAL_TEST.1).abs() < 5e-4,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"phase": "3b8_reproducibility_check", "result": reproduction_check})
        )?
    );

    // --- Population definition (mission §5) ---
    let (population_report, comparable_idx) = build_population_report(&test_rows);
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"phase": "3b8_population", "report": &population_report})
        )?
    );

    let comparable_rows: Vec<&TrainingRow> =
        comparable_idx.iter().map(|&i| &test_rows[i]).collect();
    let comparable_candidate_scores: Vec<(f64, f64)> = comparable_idx
        .iter()
        .map(|&i| (candidate_scores[i], f64::from(test_rows[i].label)))
        .collect();
    let v1_raw_scores: Vec<f64> = comparable_rows
        .iter()
        .map(|row| f64::from(v1_model.predict(&v1_cell_features(row))))
        .collect();
    let v1_scores_with_labels: Vec<(f64, f64)> = v1_raw_scores
        .iter()
        .zip(comparable_rows.iter())
        .map(|(&s, r)| (s, f64::from(r.label)))
        .collect();

    // --- Per-model metrics on the shared population ---
    let v1_metrics = json!({
        "roc_auc": roc_auc(&v1_scores_with_labels),
        "average_precision": average_precision(&v1_scores_with_labels),
        "brier_score": brier_score(&v1_scores_with_labels),
        "log_loss": log_loss(&v1_scores_with_labels),
        "ece": expected_calibration_error(&v1_scores_with_labels),
        "lift_at_1pct": precision_recall_lift_at_k(&v1_scores_with_labels, 0.01).2,
        "lift_at_5pct": precision_recall_lift_at_k(&v1_scores_with_labels, 0.05).2,
        "lift_at_10pct": precision_recall_lift_at_k(&v1_scores_with_labels, 0.10).2,
        "calibration_metrics_caveat": "v1's LearnedHumanModel output is documented by its own training code (human_model.rs ModelMetrics.interpretation) as a relative propensity, not an absolute probability; brier/log_loss/ece are reported here descriptively, not as validated calibration error against a demonstrated true probability.",
    });
    let candidate_metrics = json!({
        "roc_auc": roc_auc(&comparable_candidate_scores),
        "average_precision": average_precision(&comparable_candidate_scores),
        "brier_score": brier_score(&comparable_candidate_scores),
        "log_loss": log_loss(&comparable_candidate_scores),
        "ece": expected_calibration_error(&comparable_candidate_scores),
        "lift_at_1pct": precision_recall_lift_at_k(&comparable_candidate_scores, 0.01).2,
        "lift_at_5pct": precision_recall_lift_at_k(&comparable_candidate_scores, 0.05).2,
        "lift_at_10pct": precision_recall_lift_at_k(&comparable_candidate_scores, 0.10).2,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"phase": "3b8_paired_metrics", "v1": v1_metrics, "candidate": candidate_metrics})
        )?
    );

    // --- Paired bootstrap (same resampled rows for both models) ---
    let dates: Vec<chrono::NaiveDate> = comparable_rows.iter().map(|r| r.local_date).collect();
    let bootstrap = paired_block_bootstrap(
        &dates,
        &v1_scores_with_labels,
        &comparable_candidate_scores,
        options.seed,
    );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"phase": "3b8_paired_bootstrap", "result": &bootstrap})
        )?
    );

    // --- Top-k operational comparison ---
    let top_k: Vec<TopKOverlap> = [0.01, 0.05, 0.10]
        .into_iter()
        .map(|f| top_k_overlap(&v1_scores_with_labels, &comparable_candidate_scores, f))
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"phase": "3b8_top_k", "result": &top_k}))?
    );

    // --- Disagreement analysis ---
    let comparable_rows_owned: Vec<TrainingRow> =
        comparable_rows.iter().map(|&r| r.clone()).collect();
    let disagreement = disagreement_summary(
        &comparable_rows_owned,
        &v1_raw_scores,
        &comparable_candidate_scores
            .iter()
            .map(|&(p, _)| p)
            .collect::<Vec<_>>(),
    );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"phase": "3b8_disagreement", "result": &disagreement})
        )?
    );

    // --- Combustibility sensitivity (analytic join, no dataset rebuild) ---
    let combustibility = combustibility_sensitivity(&store, &comparable_rows_owned).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"phase": "3b8_combustibility_sensitivity", "result": &combustibility})
        )?
    );

    // --- Strict N2, frozen hyperparameters only ---
    let strict_n2_result = match store.dataset_rows_for_training(STRICT_N2_LOGICAL_ID).await {
        Ok(n2_rows) if !n2_rows.is_empty() => {
            model_experiments::assert_split_dates_in_range(&n2_rows)?;
            let n2_train: Vec<TrainingRow> = n2_rows
                .iter()
                .filter(|r| r.split == "train")
                .cloned()
                .collect();
            let n2_calib: Vec<TrainingRow> = n2_rows
                .iter()
                .filter(|r| r.split == "calibration")
                .cloned()
                .collect();
            let n2_test: Vec<TrainingRow> = n2_rows
                .iter()
                .filter(|r| r.split == "test")
                .cloned()
                .collect();
            if n2_train.is_empty() || n2_calib.is_empty() || n2_test.is_empty() {
                json!({"available": false, "reason": "strict_n2 dataset has an empty split"})
            } else {
                let (_, metrics) = replay_frozen_gbm_candidate(&n2_train, &n2_calib, &n2_test);
                json!({"available": true, "test_metrics": metrics})
            }
        }
        _ => json!({"available": false, "reason": "strict_n2 dataset not found in this database"}),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"phase": "3b8_strict_n2", "result": &strict_n2_result})
        )?
    );

    let fingerprint_after = store.dataset_rows_fingerprint(PRINCIPAL_LOGICAL_ID).await?;
    anyhow::ensure!(
        fingerprint_before == fingerprint_after,
        "leakage check failed: principal dataset changed during comparison"
    );

    // --- Decision ---
    let ap_diff = bootstrap.ap_diff_mean;
    let ap_ci_excludes_zero_positive = bootstrap.ap_diff_95pct_ci.0 > 0.0;
    #[allow(clippy::cast_precision_loss)]
    let comparable_fraction =
        population_report.v1_comparable as f64 / population_report.total_rows.max(1) as f64;
    let population_fidelity_ok =
        population_report.v1_missing_features == 0 || comparable_fraction > 0.95;

    // A gain that is not statistically distinguishable from zero, or a
    // negative gain, both fail the promotion criterion the same way:
    // "not demonstrated to be superior" (mission section 14/15).
    let verdict = if !population_fidelity_ok {
        "V1_COMPARISON_NOT_SCIENTIFICALLY_VALID"
    } else if ap_diff > 0.0 && ap_ci_excludes_zero_positive {
        "MODEL_CANDIDATE_READY_FOR_PROMOTION_REVIEW"
    } else {
        "MODEL_CANDIDATE_NOT_SUPERIOR_TO_V1"
    };

    let manifest = json!({
        "phase": "3b8_manifest",
        "seed": options.seed,
        "principal_dataset_row_fingerprint_before": fingerprint_before,
        "principal_dataset_row_fingerprint_after": fingerprint_after,
        "v1_artifact_id": active.id,
        "v1_trained_at": active.trained_at.to_rfc3339(),
        "candidate_gbm_hyperparameters": {"n_trees": FROZEN_GBM.0, "max_depth": FROZEN_GBM.1, "learning_rate": FROZEN_GBM.2},
        "population_report": &population_report,
        "ap_diff_candidate_minus_v1": ap_diff,
        "ap_diff_95pct_ci": bootstrap.ap_diff_95pct_ci,
        "started_at_utc": chrono::Utc::now().to_rfc3339(),
    });

    let report = json!({
        "manifest": manifest,
        "reproduction_check": reproduction_check,
        "population_report": population_report,
        "v1_metrics": v1_metrics,
        "candidate_metrics": candidate_metrics,
        "paired_bootstrap": bootstrap,
        "top_k": top_k,
        "disagreement": disagreement,
        "combustibility_sensitivity": combustibility,
        "strict_n2": strict_n2_result,
        "verdict": verdict,
    });
    let report_path = artifact_dir.join("v1_candidate_comparison_report.json");
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;
    let report_bytes = std::fs::metadata(&report_path)?.len();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "phase": "3b8_verdict",
            "verdict": verdict,
            "ap_diff_candidate_minus_v1": ap_diff,
            "ap_diff_95pct_ci": bootstrap.ap_diff_95pct_ci,
            "artifact_path": report_path.display().to_string(),
            "artifact_size_bytes": report_bytes,
        }))?
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_population_report, paired_block_bootstrap, top_k_overlap, v1_cell_features,
        v1_reconstructable,
    };
    use chrono::NaiveDate;
    use serde_json::json;
    use store::TrainingRow;

    fn row_with_features(
        h3: i64,
        date: &str,
        split: &str,
        label: i16,
        features: serde_json::Value,
    ) -> TrainingRow {
        TrainingRow {
            h3,
            local_date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            split: split.to_owned(),
            label,
            features,
        }
    }

    fn full_features() -> serde_json::Value {
        json!({
            "wui": 0.4, "road": 0.2, "agri": 0.1, "population": 0.3, "poi": 0.05,
            "power_line": 0.0, "hist": 0.6, "combustible": true, "weekend": false,
            "public_holiday": false, "season_sine": 0.1, "season_cosine": 0.9,
        })
    }

    // Mission section 18: exact reconstruction of the vector v1 needs,
    // and the school_holiday bridge is exactly the COALESCE-to-false
    // convention v1's own production serving already uses.
    #[test]
    fn v1_cell_features_reconstructs_exact_values_and_bridges_school_holiday() {
        let row = row_with_features(1, "2025-06-01", "test", 1, full_features());
        let features = v1_cell_features(&row);
        assert!((f64::from(features.wui) - 0.4).abs() < 1e-6);
        assert!((f64::from(features.road) - 0.2).abs() < 1e-6);
        assert!((f64::from(features.hist) - 0.6).abs() < 1e-6);
        assert!(features.combustible);
        assert!(
            !features.school_holiday,
            "must bridge missing school_holiday to false, matching v1's own Store::risk_inputs COALESCE convention"
        );
        assert_eq!(features.date, row.local_date);
    }

    #[test]
    fn v1_reconstructable_is_true_when_all_needed_fields_present() {
        let row = row_with_features(1, "2025-06-01", "test", 1, full_features());
        assert!(v1_reconstructable(&row));
    }

    #[test]
    fn v1_reconstructable_is_false_when_a_needed_field_is_missing() {
        let mut features = full_features();
        features.as_object_mut().unwrap().remove("wui");
        let row = row_with_features(1, "2025-06-01", "test", 1, features);
        assert!(!v1_reconstructable(&row));
    }

    // Mission section 5: measure the common population, not assume it.
    #[test]
    fn population_report_counts_comparable_and_missing_rows_and_their_labels() {
        let mut missing = full_features();
        missing.as_object_mut().unwrap().remove("agri");
        let rows = vec![
            row_with_features(1, "2025-01-01", "test", 1, full_features()),
            row_with_features(2, "2025-01-02", "test", 0, full_features()),
            row_with_features(3, "2025-01-03", "test", 1, missing),
        ];
        let (report, comparable_idx) = build_population_report(&rows);
        assert_eq!(report.total_rows, 3);
        assert_eq!(report.v1_comparable, 2);
        assert_eq!(report.v1_missing_features, 1);
        assert_eq!(report.comparable_positive_count, 1);
        assert_eq!(report.comparable_negative_count, 1);
        assert_eq!(report.missing_positive_count, 1);
        assert_eq!(comparable_idx, vec![0, 1]);
    }

    // Mission section 18: bootstrap must be paired -- the same resampled
    // rows must be used for both models each round. Determinism given a
    // fixed seed is a necessary precondition for that.
    #[test]
    fn paired_block_bootstrap_is_deterministic_for_a_fixed_seed() {
        let dates: Vec<NaiveDate> = (1..=20)
            .map(|d| NaiveDate::from_ymd_opt(2025, 1, d).unwrap())
            .collect();
        let v1_scores: Vec<(f64, f64)> = dates
            .iter()
            .enumerate()
            .map(|(i, _)| {
                #[allow(clippy::cast_precision_loss)]
                (i as f64 / 20.0, f64::from(u8::from(i >= 10)))
            })
            .collect();
        let candidate_scores: Vec<(f64, f64)> = v1_scores
            .iter()
            .map(|&(p, y)| ((p + 0.1).min(1.0), y))
            .collect();
        let a = paired_block_bootstrap(&dates, &v1_scores, &candidate_scores, 42);
        let b = paired_block_bootstrap(&dates, &v1_scores, &candidate_scores, 42);
        assert!((a.ap_diff_mean - b.ap_diff_mean).abs() < 1e-12);
        assert_eq!(a.ap_diff_95pct_ci, b.ap_diff_95pct_ci);
    }

    // Mission section 18: a simple, known case -- candidate ranks
    // perfectly, v1 ranks randomly-ish -- must show the candidate
    // capturing at least as many positives in its top-k as v1 in a
    // constructed scenario where they agree on the top row.
    #[test]
    fn top_k_overlap_counts_shared_and_exclusive_positives() {
        // 10 rows, row 9 is the only positive. Both models rank it top-1.
        let v1_scores: Vec<(f64, f64)> = (0..10)
            .map(|i| {
                (
                    if i == 9 { 1.0 } else { f64::from(i) / 100.0 },
                    f64::from(u8::from(i == 9)),
                )
            })
            .collect();
        let candidate_scores = v1_scores.clone();
        let overlap = top_k_overlap(&v1_scores, &candidate_scores, 0.1);
        assert_eq!(overlap.v1_positives_captured, 1);
        assert_eq!(overlap.candidate_positives_captured, 1);
        assert_eq!(overlap.positives_captured_by_both, 1);
        assert_eq!(overlap.positives_only_by_v1, 0);
        assert_eq!(overlap.positives_only_by_candidate, 0);
    }
}
