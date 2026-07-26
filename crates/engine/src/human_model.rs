use std::collections::{HashMap, HashSet};

use anyhow::Context as _;
use chrono::{Days, NaiveDate};
use risk::{
    CellFeatures, HUMAN_FEATURE_COUNT, HUMAN_FEATURE_NAMES, LearnedHumanModel, human_feature_vector,
};
use serde::{Deserialize, Serialize};
use store::{CellStaticRow, HumanIgnitionSample, Store};

const TRAINING_ITERATIONS: usize = 500;
const LEARNING_RATE: f64 = 0.12;
const L2_REGULARIZATION: f64 = 0.01;
const MIN_VALIDATION_AUC: f64 = 0.52;
const TRAIN_CONTROL_SEED: i64 = 2_024_051;
const VALIDATION_CONTROL_SEED: i64 = 2_025_097;

#[derive(Clone, Copy, Debug)]
pub struct TrainingOptions {
    pub train_from: NaiveDate,
    pub train_to: NaiveDate,
    pub validation_from: NaiveDate,
    pub validation_to: NaiveDate,
    pub negatives_per_positive: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct TrainingSummary {
    pub version: i64,
    pub train_positive_count: usize,
    pub train_negative_count: usize,
    pub validation_positive_count: usize,
    pub validation_negative_count: usize,
    pub validation_roc_auc: f64,
    pub validation_average_precision: f64,
    pub validation_brier_score: f64,
    pub validation_log_loss: f64,
}

#[derive(Clone, Debug, Serialize)]
struct ModelMetrics {
    method: &'static str,
    interpretation: &'static str,
    feature_names: [&'static str; HUMAN_FEATURE_COUNT],
    negative_sampling: &'static str,
    negatives_per_positive: usize,
    training_iterations: usize,
    l2_regularization: f64,
    validation_roc_auc: f64,
    validation_average_precision: f64,
    validation_brier_score: f64,
    validation_log_loss: f64,
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    values: [f64; HUMAN_FEATURE_COUNT],
    label: f64,
}

#[derive(Clone, Copy, Debug)]
struct Evaluation {
    roc_auc: f64,
    average_precision: f64,
    brier_score: f64,
    log_loss: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct StaticFeatures {
    #[serde(default)]
    hist: f32,
    wui: f32,
    road: f32,
    agri: f32,
    #[serde(default)]
    population: f32,
    #[serde(default)]
    poi: f32,
    #[serde(default)]
    power_line: f32,
    combustible: bool,
}

#[allow(clippy::too_many_lines)]
pub async fn train_and_activate(
    store: &Store,
    options: TrainingOptions,
) -> anyhow::Result<TrainingSummary> {
    validate_options(options)?;
    let train_positive_rows = store
        .human_ignition_samples_between(options.train_from, options.train_to)
        .await
        .context("failed to load training ignitions")?;
    let validation_positive_rows = store
        .human_ignition_samples_between(options.validation_from, options.validation_to)
        .await
        .context("failed to load validation ignitions")?;
    anyhow::ensure!(
        !train_positive_rows.is_empty(),
        "no known human BDIFF ignition is available in the training interval"
    );
    anyhow::ensure!(
        !validation_positive_rows.is_empty(),
        "no known human BDIFF ignition is available in the validation interval"
    );

    let train_negative_count = train_positive_rows
        .len()
        .checked_mul(options.negatives_per_positive)
        .context("training control count overflow")?;
    let validation_negative_count = validation_positive_rows
        .len()
        .checked_mul(options.negatives_per_positive)
        .context("validation control count overflow")?;
    let train_control_rows = store
        .sample_combustible_cells(train_negative_count, TRAIN_CONTROL_SEED)
        .await
        .context("failed to sample training control cells")?;
    let validation_control_rows = store
        .sample_combustible_cells(validation_negative_count, VALIDATION_CONTROL_SEED)
        .await
        .context("failed to sample validation control cells")?;
    anyhow::ensure!(
        train_control_rows.len() == train_negative_count,
        "not enough combustible cells for the requested training controls"
    );
    anyhow::ensure!(
        validation_control_rows.len() == validation_negative_count,
        "not enough combustible cells for the requested validation controls"
    );

    let calendar = store
        .calendar_days_between(options.train_from, options.validation_to)
        .await
        .context("failed to load model calendar flags")?
        .into_iter()
        .map(|day| (day.date, (day.school_holiday, day.public_holiday)))
        .collect::<HashMap<_, _>>();
    let train_positives = positive_samples(&train_positive_rows, &calendar)?;
    let validation_positives = positive_samples(&validation_positive_rows, &calendar)?;
    let train_positive_keys = event_keys(&train_positive_rows);
    let validation_positive_keys = event_keys(&validation_positive_rows);
    let train_controls = control_samples(
        &train_control_rows,
        options.train_from,
        options.train_to,
        TRAIN_CONTROL_SEED,
        &calendar,
        &train_positive_keys,
    )?;
    let validation_controls = control_samples(
        &validation_control_rows,
        options.validation_from,
        options.validation_to,
        VALIDATION_CONTROL_SEED,
        &calendar,
        &validation_positive_keys,
    )?;

    let mut training = train_positives;
    training.extend(train_controls);
    let mut validation = validation_positives;
    validation.extend(validation_controls);
    let model = fit_logistic(&training)?;
    model.validate().context("trained model is invalid")?;
    let evaluation = evaluate(&model, &validation);
    anyhow::ensure!(
        evaluation.roc_auc >= MIN_VALIDATION_AUC,
        "validation ROC AUC {:.3} is below activation threshold {:.2}",
        evaluation.roc_auc,
        MIN_VALIDATION_AUC
    );

    let metrics = ModelMetrics {
        method: "regularized_logistic_case_control_v1",
        interpretation: "relative_human_ignition_propensity_not_absolute_probability",
        feature_names: HUMAN_FEATURE_NAMES,
        negative_sampling: "deterministic_uniform_combustible_cell_days",
        negatives_per_positive: options.negatives_per_positive,
        training_iterations: TRAINING_ITERATIONS,
        l2_regularization: L2_REGULARIZATION,
        validation_roc_auc: evaluation.roc_auc,
        validation_average_precision: evaluation.average_precision,
        validation_brier_score: evaluation.brier_score,
        validation_log_loss: evaluation.log_loss,
    };
    let artifact = serde_json::to_value(&model).context("failed to serialize trained model")?;
    let metrics_json =
        serde_json::to_value(&metrics).context("failed to serialize model metrics")?;
    let version = store
        .activate_human_model(
            options.train_from,
            options.train_to,
            options.validation_from,
            options.validation_to,
            train_positive_rows.len(),
            train_negative_count,
            validation_positive_rows.len(),
            validation_negative_count,
            &artifact,
            &metrics_json,
        )
        .await
        .context("failed to activate trained human model")?;

    Ok(TrainingSummary {
        version,
        train_positive_count: train_positive_rows.len(),
        train_negative_count,
        validation_positive_count: validation_positive_rows.len(),
        validation_negative_count,
        validation_roc_auc: evaluation.roc_auc,
        validation_average_precision: evaluation.average_precision,
        validation_brier_score: evaluation.brier_score,
        validation_log_loss: evaluation.log_loss,
    })
}

fn validate_options(options: TrainingOptions) -> anyhow::Result<()> {
    anyhow::ensure!(
        options.train_from <= options.train_to,
        "training start must not follow training end"
    );
    anyhow::ensure!(
        options.validation_from <= options.validation_to,
        "validation start must not follow validation end"
    );
    anyhow::ensure!(
        options.train_to < options.validation_from,
        "validation must start strictly after the training interval"
    );
    anyhow::ensure!(
        (1..=20).contains(&options.negatives_per_positive),
        "negatives per positive must be between 1 and 20"
    );
    Ok(())
}

fn positive_samples(
    rows: &[HumanIgnitionSample],
    calendar: &HashMap<NaiveDate, (bool, bool)>,
) -> anyhow::Result<Vec<Sample>> {
    rows.iter()
        .map(|row| {
            sample(
                &row.features,
                row.date,
                calendar.get(&row.date).copied().unwrap_or_default(),
                1.0,
            )
        })
        .collect()
}

fn control_samples(
    rows: &[CellStaticRow],
    from: NaiveDate,
    to: NaiveDate,
    seed: i64,
    calendar: &HashMap<NaiveDate, (bool, bool)>,
    positives: &HashSet<(grid::CellIndex, NaiveDate)>,
) -> anyhow::Result<Vec<Sample>> {
    let day_count = to.signed_duration_since(from).num_days() + 1;
    let day_count = u64::try_from(day_count).context("invalid control date interval")?;
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let hash = mix64(
                u64::from_ne_bytes(grid::cell_to_db(row.cell).to_ne_bytes())
                    ^ u64::from_ne_bytes(seed.to_ne_bytes())
                    ^ u64::try_from(index).unwrap_or(u64::MAX),
            );
            let mut offset = hash % day_count;
            let mut date = from
                .checked_add_days(Days::new(offset))
                .context("control date overflow")?;
            while positives.contains(&(row.cell, date)) {
                offset = (offset + 1) % day_count;
                date = from
                    .checked_add_days(Days::new(offset))
                    .context("control date overflow")?;
            }
            sample(
                &row.features,
                date,
                calendar.get(&date).copied().unwrap_or_default(),
                0.0,
            )
        })
        .collect()
}

fn sample(
    value: &serde_json::Value,
    date: NaiveDate,
    calendar: (bool, bool),
    label: f64,
) -> anyhow::Result<Sample> {
    let static_features = serde_json::from_value::<StaticFeatures>(value.clone())
        .context("invalid cell_static feature document in training data")?;
    anyhow::ensure!(
        static_features.combustible,
        "human-model sample unexpectedly references non-combustible land"
    );
    let features = CellFeatures {
        fwi: 0.0,
        hist: static_features.hist,
        wui: static_features.wui,
        road: static_features.road,
        agri: static_features.agri,
        population: static_features.population,
        poi: static_features.poi,
        power_line: static_features.power_line,
        combustible: true,
        date,
        school_holiday: calendar.0,
        public_holiday: calendar.1,
    };
    Ok(Sample {
        values: human_feature_vector(&features).map(f64::from),
        label,
    })
}

fn event_keys(rows: &[HumanIgnitionSample]) -> HashSet<(grid::CellIndex, NaiveDate)> {
    rows.iter().map(|row| (row.cell, row.date)).collect()
}

fn fit_logistic(samples: &[Sample]) -> anyhow::Result<LearnedHumanModel> {
    let sample_count = samples.len();
    anyhow::ensure!(sample_count > 0, "model training sample is empty");
    #[allow(clippy::cast_precision_loss)]
    let denominator = sample_count as f64;
    let mut means = [0.0; HUMAN_FEATURE_COUNT];
    for sample in samples {
        for (mean, value) in means.iter_mut().zip(sample.values) {
            *mean += value / denominator;
        }
    }
    let mut scales = [0.0; HUMAN_FEATURE_COUNT];
    for sample in samples {
        for ((scale, value), mean) in scales.iter_mut().zip(sample.values).zip(means) {
            *scale += (value - mean).powi(2) / denominator;
        }
    }
    for scale in &mut scales {
        *scale = scale.sqrt().max(1.0e-6);
    }

    let positive_count = samples.iter().filter(|sample| sample.label > 0.5).count();
    let negative_count = sample_count - positive_count;
    anyhow::ensure!(
        positive_count > 0 && negative_count > 0,
        "training requires positive and negative samples"
    );
    #[allow(clippy::cast_precision_loss)]
    let mut intercept = (positive_count as f64 / negative_count as f64).ln();
    let mut weights = [0.0; HUMAN_FEATURE_COUNT];
    for iteration in 0..TRAINING_ITERATIONS {
        let mut intercept_gradient = 0.0;
        let mut gradients = [0.0; HUMAN_FEATURE_COUNT];
        for sample in samples {
            let standardized = standardize(sample.values, means, scales);
            let prediction = sigmoid(
                weights
                    .iter()
                    .zip(standardized)
                    .fold(intercept, |value, (weight, feature)| {
                        value + weight * feature
                    }),
            );
            let error = prediction - sample.label;
            intercept_gradient += error;
            for (gradient, feature) in gradients.iter_mut().zip(standardized) {
                *gradient += error * feature;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let learning_rate = LEARNING_RATE / (1.0 + iteration as f64 / 100.0).sqrt();
        intercept -= learning_rate * intercept_gradient / denominator;
        for (weight, gradient) in weights.iter_mut().zip(gradients) {
            *weight -= learning_rate * (gradient / denominator + L2_REGULARIZATION * *weight);
        }
    }

    Ok(LearnedHumanModel {
        intercept: to_f32(intercept)?,
        weights: convert_array(weights)?,
        means: convert_array(means)?,
        scales: convert_array(scales)?,
    })
}

fn evaluate(model: &LearnedHumanModel, samples: &[Sample]) -> Evaluation {
    let mut scored = samples
        .iter()
        .map(|sample| {
            let prediction = sigmoid(
                model
                    .weights
                    .iter()
                    .map(|value| f64::from(*value))
                    .zip(standardize(
                        sample.values,
                        model.means.map(f64::from),
                        model.scales.map(f64::from),
                    ))
                    .fold(f64::from(model.intercept), |value, (weight, feature)| {
                        value + weight * feature
                    }),
            );
            (prediction, sample.label > 0.5)
        })
        .collect::<Vec<_>>();
    let roc_auc = roc_auc(&mut scored);
    scored.sort_by(|left, right| right.0.total_cmp(&left.0));
    let positive_count = scored.iter().filter(|(_, label)| *label).count();
    let average_precision = if positive_count == 0 {
        0.0
    } else {
        let mut positives_seen = 0_usize;
        let precision_sum = scored
            .iter()
            .enumerate()
            .filter_map(|(index, (_, label))| {
                if *label {
                    positives_seen += 1;
                    #[allow(clippy::cast_precision_loss)]
                    Some(positives_seen as f64 / (index + 1) as f64)
                } else {
                    None
                }
            })
            .sum::<f64>();
        #[allow(clippy::cast_precision_loss)]
        {
            precision_sum / positive_count as f64
        }
    };
    #[allow(clippy::cast_precision_loss)]
    let denominator = scored.len() as f64;
    let (brier_score, log_loss) = scored.iter().fold((0.0, 0.0), |totals, item| {
        let label = if item.1 { 1.0 } else { 0.0 };
        let probability = item.0.clamp(1.0e-12, 1.0 - 1.0e-12);
        (
            totals.0 + (probability - label).powi(2),
            totals.1 - label * probability.ln() - (1.0 - label) * (1.0 - probability).ln(),
        )
    });
    Evaluation {
        roc_auc,
        average_precision,
        brier_score: brier_score / denominator,
        log_loss: log_loss / denominator,
    }
}

fn roc_auc(scored: &mut [(f64, bool)]) -> f64 {
    scored.sort_by(|left, right| left.0.total_cmp(&right.0));
    let positive_count = scored.iter().filter(|(_, label)| *label).count();
    let negative_count = scored.len() - positive_count;
    if positive_count == 0 || negative_count == 0 {
        return 0.5;
    }
    let mut rank_sum = 0.0;
    let mut start = 0;
    while start < scored.len() {
        let mut end = start + 1;
        while end < scored.len() && scored[end].0.total_cmp(&scored[start].0).is_eq() {
            end += 1;
        }
        #[allow(clippy::cast_precision_loss)]
        let average_rank = (start + 1 + end) as f64 / 2.0;
        let positives = scored[start..end]
            .iter()
            .filter(|(_, label)| *label)
            .count();
        #[allow(clippy::cast_precision_loss)]
        {
            rank_sum += average_rank * positives as f64;
        }
        start = end;
    }
    #[allow(clippy::cast_precision_loss)]
    {
        (rank_sum - (positive_count * (positive_count + 1)) as f64 / 2.0)
            / (positive_count * negative_count) as f64
    }
}

fn standardize(
    values: [f64; HUMAN_FEATURE_COUNT],
    means: [f64; HUMAN_FEATURE_COUNT],
    scales: [f64; HUMAN_FEATURE_COUNT],
) -> [f64; HUMAN_FEATURE_COUNT] {
    std::array::from_fn(|index| (values[index] - means[index]) / scales[index])
}

fn convert_array(values: [f64; HUMAN_FEATURE_COUNT]) -> anyhow::Result<[f32; HUMAN_FEATURE_COUNT]> {
    let mut converted = [0.0; HUMAN_FEATURE_COUNT];
    for (target, value) in converted.iter_mut().zip(values) {
        *target = to_f32(value)?;
    }
    Ok(converted)
}

fn to_f32(value: f64) -> anyhow::Result<f32> {
    anyhow::ensure!(value.is_finite(), "model parameter is not finite");
    #[allow(clippy::cast_possible_truncation)]
    Ok(value as f32)
}

fn sigmoid(value: f64) -> f64 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{Sample, evaluate, fit_logistic};

    #[test]
    fn logistic_training_separates_simple_samples() {
        let mut samples = Vec::new();
        for index in 0..100 {
            let positive = index >= 50;
            let mut values = [0.0; risk::HUMAN_FEATURE_COUNT];
            values[0] = if positive { 1.0 } else { 0.0 };
            values[9] = f64::from(index) / 100.0;
            samples.push(Sample {
                values,
                label: if positive { 1.0 } else { 0.0 },
            });
        }
        let model = fit_logistic(&samples).expect("model should train");
        let evaluation = evaluate(&model, &samples);

        assert!(evaluation.roc_auc > 0.99);
        assert!(evaluation.brier_score < 0.1);
    }

    #[test]
    fn options_require_chronological_holdout() {
        let date = NaiveDate::from_ymd_opt(2025, 1, 1).expect("valid date");
        let options = super::TrainingOptions {
            train_from: date,
            train_to: date,
            validation_from: date,
            validation_to: date,
            negatives_per_positive: 4,
        };
        assert!(super::validate_options(options).is_err());
    }
}
