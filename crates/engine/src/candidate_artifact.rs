//! Phase 3B.9: candidate model artifact packaging, canonical checksums,
//! and a training-independent inference path. Never activates the
//! candidate, never writes to `human_model_versions`, never touches
//! serving or the API. All computation here is local + read-only
//! database access (dataset rows, `cell_static`, `calendar_days`).

use std::collections::BTreeMap;

use anyhow::Context;
use chrono::NaiveDate;
use dataset::normalization::{
    FeatureStatistics, ImputationRule, NormalizationMethod, apply_normalization,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use store::{Store, TrainingRow};

use crate::model_experiments::{
    self, GbmModel, apply_isotonic, build_raw_row, fit_gbm, fit_isotonic, fit_train_only_transform,
    to_samples,
};

pub const ARTIFACT_VERSION: u32 = 1;
pub const MODEL_FAMILY: &str = "gbm_isotonic_v2";
/// Deliberately not `probability_of_fire` (mission section 8): the
/// score is a relative propensity calibrated against the sampled 2024
/// calibration distribution, not a demonstrated absolute probability.
pub const MODEL_NAME: &str = "human_ignition_propensity_v2";
const FROZEN_GBM: (usize, usize, f64) = (50, 3, 0.1);
const PRINCIPAL_LOGICAL_ID: &str =
    "erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality";

/// The 12 candidate features in their real, checksummed training order
/// (confirmed against `model_experiments::FEATURE_NAMES`, not the
/// mission brief's own indicative list, which uses a different order).
pub fn feature_order() -> Vec<String> {
    model_experiments::FEATURE_NAMES
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

fn feature_type(name: &str) -> &'static str {
    match name {
        "combustible" | "weekend" | "public_holiday" => "bool",
        _ => "f64",
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeatureNormalization {
    pub method: NormalizationMethod,
    pub statistics: FeatureStatistics,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct GbmHyperparameters {
    pub n_trees: usize,
    pub max_depth: usize,
    pub learning_rate: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateArtifact {
    pub artifact_version: u32,
    pub model_family: String,
    pub model_name: String,
    pub git_commit: String,
    pub dataset_logical_id: String,
    pub dataset_row_fingerprint: String,
    pub feature_names: Vec<String>,
    pub feature_types: BTreeMap<String, String>,
    pub normalization_parameters: BTreeMap<String, FeatureNormalization>,
    pub imputation_parameters: BTreeMap<String, ImputationRule>,
    pub gbm_hyperparameters: GbmHyperparameters,
    pub gbm: GbmModel,
    pub isotonic_breakpoints: Vec<f64>,
    pub isotonic_values: Vec<f64>,
    pub training_period: (String, String),
    pub calibration_period: (String, String),
    pub test_period: (String, String),
    pub seed: i64,
    pub metrics: serde_json::Value,
    pub created_at: String,
    pub scientific_interpretation: String,
    pub known_limitations: Vec<String>,
}

impl CandidateArtifact {
    /// Validates internal consistency: correct feature count/order/
    /// types, every feature has both normalization and imputation
    /// parameters, every numeric parameter is finite, and the artifact
    /// version is one this code understands. Fails loudly rather than
    /// silently scoring with a partially-broken artifact.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.artifact_version == ARTIFACT_VERSION,
            "unsupported artifact_version {} (expected {ARTIFACT_VERSION})",
            self.artifact_version
        );
        anyhow::ensure!(
            self.feature_names == feature_order(),
            "artifact feature order does not match the checksummed training order"
        );
        for name in &self.feature_names {
            anyhow::ensure!(
                self.normalization_parameters.contains_key(name),
                "missing normalization parameters for feature {name}"
            );
            anyhow::ensure!(
                self.imputation_parameters.contains_key(name),
                "missing imputation rule for feature {name}"
            );
            anyhow::ensure!(
                self.feature_types.contains_key(name),
                "missing feature type for feature {name}"
            );
        }
        anyhow::ensure!(
            self.isotonic_breakpoints.len() == self.isotonic_values.len(),
            "isotonic breakpoints/values length mismatch"
        );
        anyhow::ensure!(
            self.isotonic_breakpoints
                .iter()
                .chain(self.isotonic_values.iter())
                .all(|v| v.is_finite()),
            "isotonic calibrator contains a non-finite value"
        );
        for norm in self.normalization_parameters.values() {
            anyhow::ensure!(
                norm.statistics.mean.is_finite()
                    && norm.statistics.std_dev.is_finite()
                    && norm.statistics.median.is_finite(),
                "normalization statistics contain a non-finite value"
            );
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct ChecksumView<'a> {
    artifact_version: u32,
    model_family: &'a str,
    model_name: &'a str,
    git_commit: &'a str,
    dataset_logical_id: &'a str,
    dataset_row_fingerprint: &'a str,
    feature_names: &'a [String],
    feature_types: &'a BTreeMap<String, String>,
    normalization_parameters: &'a BTreeMap<String, FeatureNormalization>,
    imputation_parameters: &'a BTreeMap<String, ImputationRule>,
    gbm_hyperparameters: &'a GbmHyperparameters,
    gbm: &'a GbmModel,
    isotonic_breakpoints: &'a [f64],
    isotonic_values: &'a [f64],
    training_period: &'a (String, String),
    calibration_period: &'a (String, String),
    test_period: &'a (String, String),
    seed: i64,
}

/// Rounds a finite f64 to 13 significant decimal digits (via scientific
/// notation, scale-invariant for both very small and very large
/// values). Empirically, serializing an artifact and reloading it can
/// introduce noise as small as one part in 10^16 in a rare value
/// (observed once in a real trained `poi` standard deviation,
/// 0.047431670861761484 vs. 0.04743167086176149 after a JSON round
/// trip) -- most likely floating-point summation order sensitivity
/// inherited from `dataset::normalization`'s statistics computation,
/// not a data-corrupting bug (f64 has ~15-17 significant decimal
/// digits of precision to begin with). Checksumming raw bit patterns
/// would make the checksum spuriously non-reproducible across
/// serialize/deserialize round trips; quantizing first makes the
/// checksum robust to this while remaining far more precise than any
/// statistic here is scientifically meaningful to that many digits.
fn quantize(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    format!("{value:.13e}").parse::<f64>().unwrap_or(value)
}

fn canonicalize(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Number(n) => {
            n.as_f64()
                .map_or(serde_json::Value::Number(n.clone()), |f| {
                    serde_json::Number::from_f64(quantize(f))
                        .map_or(serde_json::Value::Number(n), serde_json::Value::Number)
                })
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonicalize).collect())
        }
        serde_json::Value::Object(map) => {
            serde_json::Value::Object(map.into_iter().map(|(k, v)| (k, canonicalize(v))).collect())
        }
        other => other,
    }
}

/// Checksums `value` after quantizing every floating-point number to
/// 13 significant digits (see [`quantize`]), so the checksum survives
/// a serialize/deserialize round trip and is independent of `HashMap`
/// iteration order (every name-keyed field in [`ChecksumView`] is a
/// `BTreeMap`) and of struct field declaration order (`serde_json`
/// already preserves that deterministically).
fn canonical_checksum<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
    let canonical = canonicalize(json);
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    format!("{:x}", sha2::Sha256::digest(bytes))
}

/// The full-artifact checksum. Deliberately excludes `created_at`
/// (a non-scientific timestamp, mission section 6) and `metrics`/
/// `scientific_interpretation`/`known_limitations` (descriptive, not
/// load-bearing for reproducing the model).
#[must_use]
pub fn artifact_checksum(artifact: &CandidateArtifact) -> String {
    canonical_checksum(&ChecksumView {
        artifact_version: artifact.artifact_version,
        model_family: &artifact.model_family,
        model_name: &artifact.model_name,
        git_commit: &artifact.git_commit,
        dataset_logical_id: &artifact.dataset_logical_id,
        dataset_row_fingerprint: &artifact.dataset_row_fingerprint,
        feature_names: &artifact.feature_names,
        feature_types: &artifact.feature_types,
        normalization_parameters: &artifact.normalization_parameters,
        imputation_parameters: &artifact.imputation_parameters,
        gbm_hyperparameters: &artifact.gbm_hyperparameters,
        gbm: &artifact.gbm,
        isotonic_breakpoints: &artifact.isotonic_breakpoints,
        isotonic_values: &artifact.isotonic_values,
        training_period: &artifact.training_period,
        calibration_period: &artifact.calibration_period,
        test_period: &artifact.test_period,
        seed: artifact.seed,
    })
}

#[must_use]
pub fn gbm_checksum(artifact: &CandidateArtifact) -> String {
    canonical_checksum(&artifact.gbm)
}

#[must_use]
pub fn calibrator_checksum(artifact: &CandidateArtifact) -> String {
    canonical_checksum(&(&artifact.isotonic_breakpoints, &artifact.isotonic_values))
}

#[must_use]
pub fn transforms_checksum(artifact: &CandidateArtifact) -> String {
    canonical_checksum(&(
        &artifact.normalization_parameters,
        &artifact.imputation_parameters,
    ))
}

#[must_use]
pub fn feature_list_checksum(artifact: &CandidateArtifact) -> String {
    canonical_checksum(&artifact.feature_names)
}

/// Deserializes an artifact and verifies it against an externally
/// supplied expected checksum (e.g. one previously recorded in a
/// manifest). Fails the load rather than returning a partially-trusted
/// artifact on mismatch.
pub fn load_and_verify_artifact(
    bytes: &[u8],
    expected_checksum: &str,
) -> anyhow::Result<CandidateArtifact> {
    let artifact: CandidateArtifact = serde_json::from_slice(bytes)
        .context("candidate artifact JSON is malformed or incompatible")?;
    artifact
        .validate()
        .context("candidate artifact failed internal validation")?;
    let actual = artifact_checksum(&artifact);
    anyhow::ensure!(
        actual == expected_checksum,
        "candidate artifact checksum mismatch: expected {expected_checksum}, computed {actual}"
    );
    Ok(artifact)
}

fn json_feature_value(value: &serde_json::Value) -> Option<f64> {
    if let Some(n) = value.as_f64() {
        Some(n)
    } else {
        value.as_bool().map(f64::from)
    }
}

/// The training-independent inference path: only reads serialized
/// artifact parameters (`GbmModel::predict`, `apply_normalization`,
/// `apply_isotonic`) -- never calls `fit_gbm`, `fit_train_only_
/// transform`, or `fit_isotonic`, which are training-only. `raw`
/// values may be given in any order (a `BTreeMap`, not a positional
/// list) and may contain extra, unrelated keys; only `artifact.
/// feature_names` in the artifact's own recorded order is ever read.
pub fn score_with_artifact(
    artifact: &CandidateArtifact,
    raw: &BTreeMap<String, serde_json::Value>,
) -> anyhow::Result<f64> {
    artifact.validate()?;
    let mut x = [0.0f64; 12];
    for (i, name) in artifact.feature_names.iter().enumerate() {
        let norm = artifact
            .normalization_parameters
            .get(name)
            .with_context(|| format!("missing normalization parameters for feature {name}"))?;
        let imputation = artifact
            .imputation_parameters
            .get(name)
            .with_context(|| format!("missing imputation rule for feature {name}"))?;
        let value = match raw.get(name).and_then(json_feature_value) {
            Some(v) => v,
            None => imputation
                .imputed_value
                .with_context(|| format!("feature {name} is missing and has no imputation rule"))?,
        };
        anyhow::ensure!(
            value.is_finite(),
            "feature {name} has a non-finite raw value"
        );
        let transformed = if matches!(norm.method, NormalizationMethod::Log1pThenStandardize) {
            value.max(0.0).ln_1p()
        } else {
            value
        };
        let scaled = apply_normalization(transformed, norm.method, &norm.statistics);
        anyhow::ensure!(
            scaled.is_finite(),
            "feature {name} produced a non-finite normalized value"
        );
        x[i] = scaled;
    }
    let raw_score = artifact.gbm.predict(&x);
    anyhow::ensure!(raw_score.is_finite(), "GBM produced a non-finite raw score");
    let blocks: Vec<(f64, f64)> = artifact
        .isotonic_breakpoints
        .iter()
        .zip(artifact.isotonic_values.iter())
        .map(|(&b, &v)| (b, v))
        .collect();
    let calibrated = apply_isotonic(raw_score, &blocks);
    anyhow::ensure!(
        (0.0..=1.0).contains(&calibrated) && calibrated.is_finite(),
        "calibrated score {calibrated} is out of the expected [0,1] range"
    );
    Ok(calibrated)
}

/// Builds the full candidate artifact by retraining the frozen phase
/// 3B.7 GBM+isotonic candidate (no new hyperparameter search) on the
/// principal dataset, then packaging every parameter the independent
/// inference path needs. This is the *only* place `fit_gbm`/`fit_
/// train_only_transform`/`fit_isotonic` are called in this module --
/// `score_with_artifact` never calls them.
pub async fn build_candidate_artifact(
    store: &Store,
    git_commit: &str,
    seed: i64,
) -> anyhow::Result<CandidateArtifact> {
    let fingerprint = store.dataset_rows_fingerprint(PRINCIPAL_LOGICAL_ID).await?;
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

    let train_raw: Vec<[Option<f64>; 12]> = train_rows.iter().map(build_raw_row).collect();
    let stats = fit_train_only_transform(&train_raw);
    let rules: Vec<ImputationRule> = stats
        .iter()
        .map(dataset::normalization::fit_imputation_rule)
        .collect();

    let train_samples = to_samples(&train_rows, &stats, &rules);
    let calib_samples = to_samples(&calib_rows, &stats, &rules);
    let test_samples = to_samples(&test_rows, &stats, &rules);

    let gbm = fit_gbm(&train_samples, FROZEN_GBM.0, FROZEN_GBM.1, FROZEN_GBM.2);
    let calib_scores: Vec<(f64, f64)> = calib_samples
        .iter()
        .map(|s| (gbm.predict(&s.x), s.y))
        .collect();
    let isotonic_blocks = fit_isotonic(&calib_scores);
    let test_scores: Vec<(f64, f64)> = test_samples
        .iter()
        .map(|s| (apply_isotonic(gbm.predict(&s.x), &isotonic_blocks), s.y))
        .collect();
    let metrics = model_experiments::compute_split_metrics("test", &test_scores);

    let names = feature_order();
    let mut feature_types = BTreeMap::new();
    let mut normalization_parameters = BTreeMap::new();
    let mut imputation_parameters = BTreeMap::new();
    for (i, name) in names.iter().enumerate() {
        feature_types.insert(name.clone(), feature_type(name).to_owned());
        normalization_parameters.insert(
            name.clone(),
            FeatureNormalization {
                method: normalization_method(i),
                statistics: stats[i].clone(),
            },
        );
        imputation_parameters.insert(name.clone(), rules[i].clone());
    }

    let train_dates: Vec<NaiveDate> = train_rows.iter().map(|r| r.local_date).collect();
    let calib_dates: Vec<NaiveDate> = calib_rows.iter().map(|r| r.local_date).collect();
    let test_dates: Vec<NaiveDate> = test_rows.iter().map(|r| r.local_date).collect();
    let period = |dates: &[NaiveDate]| -> (String, String) {
        (
            dates
                .iter()
                .min()
                .map_or_else(String::new, ToString::to_string),
            dates
                .iter()
                .max()
                .map_or_else(String::new, ToString::to_string),
        )
    };

    Ok(CandidateArtifact {
        artifact_version: ARTIFACT_VERSION,
        model_family: MODEL_FAMILY.to_owned(),
        model_name: MODEL_NAME.to_owned(),
        git_commit: git_commit.to_owned(),
        dataset_logical_id: PRINCIPAL_LOGICAL_ID.to_owned(),
        dataset_row_fingerprint: fingerprint,
        feature_names: names,
        feature_types,
        normalization_parameters,
        imputation_parameters,
        gbm_hyperparameters: GbmHyperparameters { n_trees: FROZEN_GBM.0, max_depth: FROZEN_GBM.1, learning_rate: FROZEN_GBM.2 },
        gbm,
        isotonic_breakpoints: isotonic_blocks.iter().map(|&(b, _)| b).collect(),
        isotonic_values: isotonic_blocks.iter().map(|&(_, v)| v).collect(),
        training_period: period(&train_dates),
        calibration_period: period(&calib_dates),
        test_period: period(&test_dates),
        seed,
        metrics: serde_json::to_value(&metrics)?,
        created_at: chrono::Utc::now().to_rfc3339(),
        scientific_interpretation: "relative human ignition propensity, calibrated against the sampled 2024 calibration distribution, not a demonstrated absolute real-world probability".to_owned(),
        known_limitations: vec![
            "current_snapshot_applied_historically: all non-calendar features use the current cell_static snapshot applied uniformly across 2020-2025 training dates (phase 3B.5/3B.6)".to_owned(),
            "combustible eligibility uses the any(child) H3 9->8 aggregation rule; 339 of 4,708 comparable 2025 rows (7.2%) would be excluded under a majority/proportion>=50% rule, disproportionately positives (268 of 1,177 positives vs. 71 of 3,531 negatives) -- phase 3B.8 finding, not yet independently resolved".to_owned(),
            "no faithful comparison exists against v1's full FWI-fused RiskScore, only against v1's learned human component alone".to_owned(),
            "negative sampling design differs structurally from v1's uniform combustible-cell sampling".to_owned(),
        ],
    })
}

fn normalization_method(index: usize) -> NormalizationMethod {
    match index {
        0 | 2 => NormalizationMethod::RobustScale,
        1 => NormalizationMethod::Standardize,
        3..=6 => NormalizationMethod::Log1pThenStandardize,
        _ => NormalizationMethod::None,
    }
}

/// Training/inference parity check (mission section 7): scores a fixed
/// sample of reference rows through the training path (`GbmModel::
/// predict` + `apply_isotonic` called directly, as `build_candidate_
/// artifact` does) and through the independent `score_with_artifact`
/// path, and reports the maximum absolute difference. The two paths
/// share `GbmModel::predict`/`apply_isotonic` themselves (there is no
/// second GBM implementation to keep in sync), so this mainly proves
/// the artifact's serialized parameters round-trip losslessly through
/// `score_with_artifact`'s normalization/imputation replay.
fn training_inference_parity(
    artifact: &CandidateArtifact,
    sample_rows: &[TrainingRow],
    training_scores: &[f64],
) -> serde_json::Value {
    const TOLERANCE: f64 = 1e-9;
    let mut max_diff: f64 = 0.0;
    let mut mismatches = 0usize;
    for (row, &training_score) in sample_rows.iter().zip(training_scores.iter()) {
        let raw: BTreeMap<String, serde_json::Value> = row
            .features
            .as_object()
            .into_iter()
            .flat_map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())))
            .collect();
        match score_with_artifact(artifact, &raw) {
            Ok(inference_score) => {
                let diff = (inference_score - training_score).abs();
                max_diff = max_diff.max(diff);
                if diff > TOLERANCE {
                    mismatches += 1;
                }
            }
            Err(_) => mismatches += 1,
        }
    }
    serde_json::json!({
        "rows_checked": sample_rows.len(),
        "tolerance": TOLERANCE,
        "max_absolute_difference": max_diff,
        "mismatches": mismatches,
        "parity_confirmed": mismatches == 0,
    })
}

/// Performance benchmark (mission section 16) for the independent
/// inference path alone: artifact load (deserialize + validate) once,
/// then per-row `score_with_artifact` timing over the full sample.
/// Isolated-container timings only -- not a substitute for a real
/// production benchmark, but sufficient to catch a gross regression
/// before any promotion review.
fn inference_performance_benchmark(
    bytes: &[u8],
    sample_rows: &[TrainingRow],
) -> anyhow::Result<serde_json::Value> {
    let load_start = std::time::Instant::now();
    let artifact: CandidateArtifact = serde_json::from_slice(bytes)?;
    artifact.validate()?;
    let load_duration = load_start.elapsed();

    let raws: Vec<BTreeMap<String, serde_json::Value>> = sample_rows
        .iter()
        .map(|row| {
            row.features
                .as_object()
                .into_iter()
                .flat_map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())))
                .collect()
        })
        .collect();

    let mut durations_ns: Vec<u128> = Vec::with_capacity(raws.len());
    for raw in &raws {
        let start = std::time::Instant::now();
        let _ = score_with_artifact(&artifact, raw);
        durations_ns.push(start.elapsed().as_nanos());
    }
    durations_ns.sort_unstable();
    let percentile = |p: f64| -> f64 {
        if durations_ns.is_empty() {
            return 0.0;
        }
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let idx = ((durations_ns.len() as f64 - 1.0) * p) as usize;
        #[allow(clippy::cast_precision_loss)]
        {
            durations_ns[idx] as f64 / 1000.0
        }
    };

    let batch_start = std::time::Instant::now();
    for raw in &raws {
        let _ = score_with_artifact(&artifact, raw);
    }
    let batch_duration = batch_start.elapsed();

    Ok(serde_json::json!({
        "artifact_bytes": bytes.len(),
        "artifact_load_and_validate_micros": load_duration.as_micros(),
        "rows_scored": raws.len(),
        "unit_score_p50_micros": percentile(0.50),
        "unit_score_p95_micros": percentile(0.95),
        "unit_score_p99_micros": percentile(0.99),
        "batch_score_total_micros": batch_duration.as_micros(),
        "environment_note": "measured in the isolated phase 3B.9 build container, not production; a gross-regression check only",
    }))
}

/// Offline/online feature parity (mission sections 12-13): for a
/// sample of the candidate's 2025 test rows, reconstructs each numeric/
/// boolean feature from the same source a live serving path would use
/// today (`cell_static.features` directly, and `calendar_days` -- the
/// production calendar table, *not* the `historical_calendar_days`
/// table the candidate dataset was built from) and compares against
/// the value stored in the dataset row. `wui/road/agri/population/poi/
/// power_line/hist/combustible` come from the same `cell_static` table
/// either way, so they are expected to match unless that table has
/// changed since the dataset was built. `weekend/season_sine/season_
/// cosine` are pure functions of the date and are expected to match
/// exactly. `public_holiday` is the one field genuinely sourced from a
/// *different* table online (`calendar_days`) than offline
/// (`historical_calendar_days`), and is measured, not assumed.
async fn offline_online_parity(
    store: &Store,
    sample_rows: &[TrainingRow],
) -> anyhow::Result<serde_json::Value> {
    let cell_static_rows = store
        .all_cell_static_rows()
        .await
        .context("load cell_static for parity check")?;
    let mut cell_static_by_h3: BTreeMap<i64, serde_json::Value> = BTreeMap::new();
    for row in cell_static_rows {
        cell_static_by_h3.insert(grid::cell_to_db(row.cell), row.features);
    }

    let min_date = sample_rows.iter().map(|r| r.local_date).min();
    let max_date = sample_rows.iter().map(|r| r.local_date).max();
    let mut public_holiday_by_date: BTreeMap<NaiveDate, bool> = BTreeMap::new();
    if let (Some(min_date), Some(max_date)) = (min_date, max_date) {
        for day in store
            .calendar_days_between(min_date, max_date)
            .await
            .context("load calendar_days for parity check")?
        {
            public_holiday_by_date.insert(day.date, day.public_holiday);
        }
    }

    let numeric_fields = [
        "wui",
        "road",
        "agri",
        "population",
        "poi",
        "power_line",
        "hist",
    ];
    let mut numeric_exact_matches = BTreeMap::new();
    let mut numeric_total = BTreeMap::new();
    for field in numeric_fields {
        numeric_exact_matches.insert(field, 0usize);
        numeric_total.insert(field, 0usize);
    }
    let mut combustible_matches = 0usize;
    let mut public_holiday_matches = 0usize;
    let mut public_holiday_total = 0usize;

    for row in sample_rows {
        if let Some(online_features) = cell_static_by_h3.get(&row.h3) {
            for field in numeric_fields {
                let offline = get_f64(&row.features, field);
                let online = get_f64(online_features, field);
                *numeric_total.get_mut(field).unwrap() += 1;
                if let (Some(a), Some(b)) = (offline, online)
                    && (a - b).abs() < 1e-9
                {
                    *numeric_exact_matches.get_mut(field).unwrap() += 1;
                }
            }
            let offline_combustible = row
                .features
                .get("combustible")
                .and_then(serde_json::Value::as_bool);
            let online_combustible = online_features
                .get("combustible")
                .and_then(serde_json::Value::as_bool);
            if offline_combustible.is_some() && offline_combustible == online_combustible {
                combustible_matches += 1;
            }
        }
        if let Some(&online_public_holiday) = public_holiday_by_date.get(&row.local_date) {
            public_holiday_total += 1;
            let offline_public_holiday = row
                .features
                .get("public_holiday")
                .and_then(serde_json::Value::as_bool);
            if offline_public_holiday == Some(online_public_holiday) {
                public_holiday_matches += 1;
            }
        }
    }

    let rate = |matches: usize, total: usize| {
        if total == 0 {
            0.0
        } else {
            f64_from_ratio(matches, total)
        }
    };
    let mut numeric_rates = serde_json::Map::new();
    for field in numeric_fields {
        numeric_rates.insert(
            field.to_owned(),
            serde_json::json!(rate(numeric_exact_matches[field], numeric_total[field])),
        );
    }

    Ok(serde_json::json!({
        "rows_checked": sample_rows.len(),
        "cell_static_numeric_and_combustible_exact_match_rate": numeric_rates,
        "combustible_exact_match_rate": rate(combustible_matches, sample_rows.len()),
        "public_holiday_exact_match_rate": rate(public_holiday_matches, public_holiday_total),
        "public_holiday_source_note": "offline uses historical_calendar_days (phase 3B.3+); online (serving) uses calendar_days, a distinct production table -- measured above, not assumed identical",
        "weekend_and_season_note": "weekend/season_sine/season_cosine are pure functions of the date in both paths and are not separately measured here; they cannot diverge without a code bug already covered by unit tests",
    }))
}

fn get_f64(v: &serde_json::Value, name: &str) -> Option<f64> {
    v.get(name).and_then(serde_json::Value::as_f64)
}

#[allow(clippy::cast_precision_loss)]
fn f64_from_ratio(matches: usize, total: usize) -> f64 {
    matches as f64 / total as f64
}

#[derive(Clone, Copy, Debug)]
pub struct PackagingOptions {
    pub seed: i64,
}

/// Top-level phase 3B.9 entry point: builds the candidate artifact,
/// validates it, computes every required checksum, runs the training/
/// inference parity check and the offline/online feature parity check,
/// and writes the artifact plus a packaging report to an isolated,
/// disposable directory. Never writes to `human_model_versions`, never
/// activates anything, never touches serving/API.
#[allow(clippy::too_many_lines)]
pub async fn run_packaging(
    config: crate::config::Config,
    options: PackagingOptions,
) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("connect to isolated database")?;
    let artifact_dir = std::path::PathBuf::from("/tmp/erytheon-experiments-3b9");
    std::fs::create_dir_all(&artifact_dir).context("create artifact dir")?;

    let git_commit = std::env::var("FIRESIFT_GIT_COMMIT_OVERRIDE")
        .unwrap_or_else(|_| env!("FIRESIFT_GIT_COMMIT").to_owned());
    let artifact = build_candidate_artifact(&store, &git_commit, options.seed).await?;
    artifact
        .validate()
        .context("freshly built artifact failed validation")?;

    let checksums = serde_json::json!({
        "artifact_checksum": artifact_checksum(&artifact),
        "gbm_checksum": gbm_checksum(&artifact),
        "calibrator_checksum": calibrator_checksum(&artifact),
        "transforms_checksum": transforms_checksum(&artifact),
        "feature_list_checksum": feature_list_checksum(&artifact),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"phase": "3b9_checksums", "result": &checksums})
        )?
    );

    // --- Round-trip check: serialize, then load-and-verify with the checksum just computed ---
    let bytes = serde_json::to_vec(&artifact)?;
    let expected = checksums["artifact_checksum"].as_str().unwrap_or_default();
    let reloaded = load_and_verify_artifact(&bytes, expected)
        .context("artifact failed its own round-trip verification")?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"phase": "3b9_round_trip", "verified": true, "artifact_bytes": bytes.len()})
        )?
    );

    // --- Training/inference parity, on the frozen test split ---
    let rows = store
        .dataset_rows_for_training(PRINCIPAL_LOGICAL_ID)
        .await?;
    let test_rows: Vec<TrainingRow> = rows.iter().filter(|r| r.split == "test").cloned().collect();
    let train_raw: Vec<[Option<f64>; 12]> = rows
        .iter()
        .filter(|r| r.split == "train")
        .map(build_raw_row)
        .collect();
    let stats = fit_train_only_transform(&train_raw);
    let rules: Vec<ImputationRule> = stats
        .iter()
        .map(dataset::normalization::fit_imputation_rule)
        .collect();
    let test_samples = to_samples(&test_rows, &stats, &rules);
    let training_scores: Vec<f64> = test_samples
        .iter()
        .map(|s| {
            let raw = reloaded.gbm.predict(&s.x);
            let blocks: Vec<(f64, f64)> = reloaded
                .isotonic_breakpoints
                .iter()
                .zip(reloaded.isotonic_values.iter())
                .map(|(&b, &v)| (b, v))
                .collect();
            apply_isotonic(raw, &blocks)
        })
        .collect();
    let parity = training_inference_parity(&reloaded, &test_rows, &training_scores);
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"phase": "3b9_training_inference_parity", "result": &parity})
        )?
    );

    // --- Offline/online feature parity ---
    let offline_online = offline_online_parity(&store, &test_rows).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"phase": "3b9_offline_online_parity", "result": &offline_online})
        )?
    );

    // --- Performance benchmark ---
    let performance = inference_performance_benchmark(&bytes, &test_rows)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"phase": "3b9_performance", "result": &performance})
        )?
    );

    let artifact_path = artifact_dir.join("candidate_artifact_v2.json");
    std::fs::write(&artifact_path, &bytes)?;
    let report = serde_json::json!({
        "checksums": checksums,
        "training_inference_parity": parity,
        "offline_online_parity": offline_online,
        "performance": performance,
    });
    let report_path = artifact_dir.join("packaging_report.json");
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "phase": "3b9_result",
            "artifact_path": artifact_path.display().to_string(),
            "artifact_size_bytes": bytes.len(),
            "report_path": report_path.display().to_string(),
        }))?
    );

    Ok(())
}

/// Every field required to register a model candidate in phase 3B.10
/// P1. Every value is explicit -- none is inferred from "the latest"
/// dataset/artifact/commit (mission section 10). `artifact_path` must
/// point at the exact P0 artifact file (`package-candidate-artifact`'s
/// output) -- this command never rebuilds the artifact from a live
/// dataset query, since the candidate dataset only ever exists in the
/// isolated training database, never in production (a real design
/// error caught the hard way: the first production attempt tried to
/// rebuild via `build_candidate_artifact`, which reads `ml.dataset_
/// rows`, empty on production, and failed with a decode error before
/// writing anything -- no partial row was created). The five expected
/// checksums must all match the loaded artifact's own checksums, and
/// `git_commit`/`dataset_logical_id`/`seed` must all match the values
/// already embedded in the artifact file, or registration is refused
/// before any database write.
#[derive(Clone, Debug)]
pub struct RegisterCandidateOptions {
    pub model_family: String,
    pub model_name: String,
    pub artifact_version: i32,
    pub artifact_path: std::path::PathBuf,
    pub git_commit: String,
    pub dataset_logical_id: String,
    pub seed: i64,
    pub status: store::ModelCandidateStatus,
    pub expected_artifact_checksum: String,
    pub expected_gbm_checksum: String,
    pub expected_calibrator_checksum: String,
    pub expected_transforms_checksum: String,
    pub expected_feature_list_checksum: String,
}

/// Phase 3B.10 P1: registers exactly one, explicitly-verified candidate
/// as `candidate`/`inactive` in `ml.model_candidate_registry`. Never
/// writes to `human_model_versions`, never loads the candidate into a
/// scoring path, never touches serving/API. Loads the already-built P0
/// artifact from `options.artifact_path` (never rebuilds it against a
/// live dataset -- see the doc comment on `RegisterCandidateOptions`),
/// then refuses to register unless *all five* checksums the caller
/// supplied match the loaded artifact's own, and unless `git_commit`/
/// `dataset_logical_id`/`seed` match what's embedded in the artifact --
/// this is the "no implicit latest" guard mission section 10 requires.
#[allow(clippy::too_many_lines)]
pub async fn run_register_model_candidate(
    config: crate::config::Config,
    options: RegisterCandidateOptions,
) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("connect to database")?;

    let artifact_bytes = std::fs::read(&options.artifact_path)
        .with_context(|| format!("read artifact file {}", options.artifact_path.display()))?;
    let artifact: CandidateArtifact = serde_json::from_slice(&artifact_bytes)
        .context("artifact file is malformed or incompatible JSON")?;
    artifact
        .validate()
        .context("loaded artifact failed validation")?;
    anyhow::ensure!(
        artifact.dataset_logical_id == options.dataset_logical_id,
        "dataset_logical_id mismatch: artifact file has {}, caller expected {}",
        artifact.dataset_logical_id,
        options.dataset_logical_id
    );
    anyhow::ensure!(
        artifact.git_commit == options.git_commit,
        "git_commit mismatch: artifact file has {}, caller expected {}",
        artifact.git_commit,
        options.git_commit
    );
    anyhow::ensure!(
        artifact.seed == options.seed,
        "seed mismatch: artifact file has {}, caller expected {}",
        artifact.seed,
        options.seed
    );

    let computed = serde_json::json!({
        "artifact_checksum": artifact_checksum(&artifact),
        "gbm_checksum": gbm_checksum(&artifact),
        "calibrator_checksum": calibrator_checksum(&artifact),
        "transforms_checksum": transforms_checksum(&artifact),
        "feature_list_checksum": feature_list_checksum(&artifact),
    });
    let mismatches: Vec<&str> = [
        (
            "artifact_checksum",
            &options.expected_artifact_checksum,
            computed["artifact_checksum"].as_str().unwrap_or_default(),
        ),
        (
            "gbm_checksum",
            &options.expected_gbm_checksum,
            computed["gbm_checksum"].as_str().unwrap_or_default(),
        ),
        (
            "calibrator_checksum",
            &options.expected_calibrator_checksum,
            computed["calibrator_checksum"].as_str().unwrap_or_default(),
        ),
        (
            "transforms_checksum",
            &options.expected_transforms_checksum,
            computed["transforms_checksum"].as_str().unwrap_or_default(),
        ),
        (
            "feature_list_checksum",
            &options.expected_feature_list_checksum,
            computed["feature_list_checksum"]
                .as_str()
                .unwrap_or_default(),
        ),
    ]
    .into_iter()
    .filter_map(|(name, expected, actual)| (expected != actual).then_some(name))
    .collect();
    anyhow::ensure!(
        mismatches.is_empty(),
        "refusing to register: checksum mismatch on {mismatches:?} (rebuilt artifact does not match the caller-supplied expected checksums -- registration must never proceed on an unverified artifact)"
    );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"phase": "3b10_checksum_verification", "verified": true, "computed": computed})
        )?
    );

    let registration = store::ModelCandidateRegistration {
        model_family: options.model_family,
        model_name: options.model_name,
        artifact_version: options.artifact_version,
        status: options.status,
        git_commit: options.git_commit,
        dataset_logical_id: options.dataset_logical_id,
        dataset_row_fingerprint: artifact.dataset_row_fingerprint.clone(),
        seed: options.seed,
        artifact: serde_json::to_value(&artifact)?,
        artifact_checksum: options.expected_artifact_checksum,
        metrics: artifact.metrics.clone(),
        scientific_interpretation: artifact.scientific_interpretation.clone(),
        known_limitations: artifact.known_limitations.clone(),
    };

    let count_before = store.model_candidate_registry_count().await?;
    let outcome = store.register_model_candidate(registration).await?;
    let count_after = store.model_candidate_registry_count().await?;

    let (outcome_name, row) = match outcome {
        store::ModelCandidateRegistrationOutcome::Registered(row) => ("registered", row),
        store::ModelCandidateRegistrationOutcome::AlreadyRegistered(row) => {
            ("already_registered", row)
        }
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "phase": "3b10_registration_result",
            "outcome": outcome_name,
            "row_id": row.id,
            "status": row.status,
            "artifact_checksum": row.artifact_checksum,
            "created_at": row.created_at.to_rfc3339(),
            "registry_row_count_before": count_before,
            "registry_row_count_after": count_after,
        }))?
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ARTIFACT_VERSION, CandidateArtifact, FeatureNormalization, GbmHyperparameters,
        artifact_checksum, feature_order, load_and_verify_artifact, score_with_artifact,
    };
    use dataset::normalization::{FeatureStatistics, ImputationRule, NormalizationMethod};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn sample_stats(feature: &str, mean: f64, std_dev: f64) -> FeatureStatistics {
        FeatureStatistics {
            feature: feature.to_owned(),
            count_present: 100,
            count_missing: 0,
            mean,
            std_dev,
            median: mean,
            p05: mean - std_dev,
            p95: mean + std_dev,
            min: mean - 3.0 * std_dev,
            max: mean + 3.0 * std_dev,
        }
    }

    fn minimal_artifact() -> CandidateArtifact {
        let names = feature_order();
        let mut feature_types = BTreeMap::new();
        let mut normalization_parameters = BTreeMap::new();
        let mut imputation_parameters = BTreeMap::new();
        for name in &names {
            feature_types.insert(name.clone(), "f64".to_owned());
            normalization_parameters.insert(
                name.clone(),
                FeatureNormalization {
                    method: NormalizationMethod::Standardize,
                    statistics: sample_stats(name, 0.0, 1.0),
                },
            );
            imputation_parameters.insert(
                name.clone(),
                ImputationRule {
                    feature: name.clone(),
                    imputed_value: Some(0.0),
                    missing_ratio_in_train: 0.0,
                },
            );
        }
        CandidateArtifact {
            artifact_version: ARTIFACT_VERSION,
            model_family: super::MODEL_FAMILY.to_owned(),
            model_name: super::MODEL_NAME.to_owned(),
            git_commit: "test".to_owned(),
            dataset_logical_id: "test_dataset".to_owned(),
            dataset_row_fingerprint: "test_fingerprint".to_owned(),
            feature_names: names,
            feature_types,
            normalization_parameters,
            imputation_parameters,
            gbm_hyperparameters: GbmHyperparameters {
                n_trees: 1,
                max_depth: 1,
                learning_rate: 0.1,
            },
            gbm: model_experiments_gbm_stub(),
            isotonic_breakpoints: vec![0.0, 1.0],
            isotonic_values: vec![0.1, 0.9],
            training_period: ("2020-01-01".to_owned(), "2023-12-31".to_owned()),
            calibration_period: ("2024-01-01".to_owned(), "2024-12-31".to_owned()),
            test_period: ("2025-01-01".to_owned(), "2025-12-31".to_owned()),
            seed: 42,
            metrics: json!({}),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            scientific_interpretation: "test".to_owned(),
            known_limitations: vec![],
        }
    }

    fn model_experiments_gbm_stub() -> crate::model_experiments::GbmModel {
        crate::model_experiments::GbmModel {
            trees: vec![crate::model_experiments::Tree::Leaf(0.0)],
            learning_rate: 0.1,
            base_score: 0.0,
            max_depth: 1,
            n_trees: 1,
        }
    }

    fn full_raw_features() -> BTreeMap<String, serde_json::Value> {
        feature_order()
            .into_iter()
            .map(|name| (name, json!(0.5)))
            .collect()
    }

    #[test]
    fn artifact_checksum_is_independent_of_created_at_and_metrics() {
        let mut a = minimal_artifact();
        let mut b = minimal_artifact();
        a.created_at = "2020-01-01T00:00:00Z".to_owned();
        b.created_at = "2099-01-01T00:00:00Z".to_owned();
        a.metrics = json!({"roc_auc": 0.5});
        b.metrics = json!({"roc_auc": 0.9});
        assert_eq!(artifact_checksum(&a), artifact_checksum(&b));
    }

    #[test]
    fn artifact_checksum_changes_when_a_weight_changes() {
        let mut a = minimal_artifact();
        let b = minimal_artifact();
        a.gbm.trees[0] = crate::model_experiments::Tree::Leaf(1.0);
        assert_ne!(artifact_checksum(&a), artifact_checksum(&b));
    }

    // Independent of BTreeMap insertion order (mission section 6): the
    // map is rebuilt in reverse here and must checksum identically.
    #[test]
    fn artifact_checksum_is_independent_of_map_insertion_order() {
        let a = minimal_artifact();
        let mut b = minimal_artifact();
        let reversed: BTreeMap<String, FeatureNormalization> =
            b.normalization_parameters.into_iter().rev().collect();
        b.normalization_parameters = reversed;
        assert_eq!(artifact_checksum(&a), artifact_checksum(&b));
    }

    // Regression test: a real trained artifact once produced a
    // std_dev of 0.047431670861761484 before a JSON round trip and
    // 0.04743167086176149 after -- a difference at the 16th
    // significant digit, most likely floating-point summation order
    // sensitivity, not data corruption. The checksum must survive
    // this via quantization (see `quantize`'s doc comment).
    #[test]
    fn artifact_checksum_survives_a_json_round_trip_despite_ulp_level_float_noise() {
        let mut artifact = minimal_artifact();
        artifact
            .normalization_parameters
            .get_mut("poi")
            .unwrap()
            .statistics
            .std_dev = 0.047_431_670_861_761_484;
        let before = artifact_checksum(&artifact);
        let bytes = serde_json::to_vec(&artifact).unwrap();
        let reloaded: CandidateArtifact = serde_json::from_slice(&bytes).unwrap();
        let after = artifact_checksum(&reloaded);
        assert_eq!(before, after);
    }

    #[test]
    fn score_with_artifact_succeeds_on_a_well_formed_artifact() {
        let artifact = minimal_artifact();
        let score = score_with_artifact(&artifact, &full_raw_features()).expect("should score");
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn score_with_artifact_uses_imputation_when_a_feature_is_missing() {
        let artifact = minimal_artifact();
        let mut raw = full_raw_features();
        raw.remove("hist");
        assert!(score_with_artifact(&artifact, &raw).is_ok());
    }

    #[test]
    fn score_with_artifact_ignores_unknown_extra_features() {
        let artifact = minimal_artifact();
        let mut raw = full_raw_features();
        raw.insert("totally_unknown_feature".to_owned(), json!(999.0));
        assert!(score_with_artifact(&artifact, &raw).is_ok());
    }

    #[test]
    fn score_with_artifact_fails_when_no_imputation_rule_exists_for_a_missing_feature() {
        let mut artifact = minimal_artifact();
        artifact
            .imputation_parameters
            .get_mut("hist")
            .unwrap()
            .imputed_value = None;
        let mut raw = full_raw_features();
        raw.remove("hist");
        assert!(score_with_artifact(&artifact, &raw).is_err());
    }

    #[test]
    fn score_with_artifact_rejects_non_finite_input() {
        // JSON structurally cannot carry NaN or Infinity: `json!(f64::
        // NAN)` becomes `null` (serde_json's `Number::from_f64` returns
        // `None` for non-finite values), and parsing an overflowing
        // literal like `1e400` is itself a parse error ("number out of
        // range"), not a silently-produced infinity. So a non-finite
        // *raw feature value* can never reach `score_with_artifact`
        // through its public JSON-based API -- the realistic failure
        // mode is a corrupted *artifact* (e.g. a non-finite statistic),
        // which `validate()` (called at the top of `score_with_
        // artifact`) must catch instead.
        let mut artifact = minimal_artifact();
        artifact
            .normalization_parameters
            .get_mut("hist")
            .unwrap()
            .statistics
            .mean = f64::INFINITY;
        assert!(score_with_artifact(&artifact, &full_raw_features()).is_err());
    }

    #[test]
    fn artifact_validate_rejects_wrong_version() {
        let mut artifact = minimal_artifact();
        artifact.artifact_version = 999;
        assert!(artifact.validate().is_err());
    }

    #[test]
    fn artifact_validate_rejects_shuffled_feature_order() {
        let mut artifact = minimal_artifact();
        artifact.feature_names.swap(0, 1);
        assert!(artifact.validate().is_err());
    }

    // Mission section 17: the candidate must never bring down the
    // serving path -- a missing/corrupted artifact must fail the load
    // (an Err a caller can catch and fall back to v1 on), never panic
    // and never silently produce a score.
    #[test]
    fn load_and_verify_artifact_rejects_an_empty_byte_slice() {
        assert!(load_and_verify_artifact(b"", "irrelevant").is_err());
    }

    #[test]
    fn load_and_verify_artifact_rejects_a_missing_required_field() {
        let mut value = serde_json::to_value(minimal_artifact()).unwrap();
        value.as_object_mut().unwrap().remove("gbm");
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(load_and_verify_artifact(&bytes, "irrelevant").is_err());
    }

    #[test]
    fn load_and_verify_artifact_rejects_an_incompatible_artifact_version() {
        let mut artifact = minimal_artifact();
        artifact.artifact_version = ARTIFACT_VERSION + 1;
        let checksum = artifact_checksum(&artifact);
        let bytes = serde_json::to_vec(&artifact).unwrap();
        // Even with a matching checksum, an incompatible version must
        // still be rejected by `validate()`.
        assert!(load_and_verify_artifact(&bytes, &checksum).is_err());
    }

    #[test]
    fn score_with_artifact_fails_rather_than_panics_on_a_missing_normalization_entry() {
        let mut artifact = minimal_artifact();
        artifact.normalization_parameters.remove("hist");
        // validate() would already catch this, but score_with_artifact
        // must not panic even if called on an unvalidated artifact.
        let result =
            std::panic::catch_unwind(|| score_with_artifact(&artifact, &full_raw_features()));
        assert!(result.is_ok(), "score_with_artifact must not panic");
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn load_and_verify_artifact_rejects_checksum_mismatch() {
        let artifact = minimal_artifact();
        let bytes = serde_json::to_vec(&artifact).unwrap();
        let result = load_and_verify_artifact(&bytes, "not_the_real_checksum");
        assert!(result.is_err());
    }

    #[test]
    fn load_and_verify_artifact_accepts_the_correct_checksum() {
        let artifact = minimal_artifact();
        let checksum = artifact_checksum(&artifact);
        let bytes = serde_json::to_vec(&artifact).unwrap();
        assert!(load_and_verify_artifact(&bytes, &checksum).is_ok());
    }

    #[test]
    fn load_and_verify_artifact_rejects_corrupted_json() {
        let result = load_and_verify_artifact(b"{ not valid json", "irrelevant");
        assert!(result.is_err());
    }

    #[test]
    fn feature_order_has_twelve_entries_and_no_duplicates() {
        let names = feature_order();
        assert_eq!(names.len(), 12);
        let unique: std::collections::HashSet<&String> = names.iter().collect();
        assert_eq!(unique.len(), 12);
    }
}
