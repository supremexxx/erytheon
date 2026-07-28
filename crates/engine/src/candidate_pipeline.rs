//! Phase 3B.5 candidate dataset construction: four reproducible dataset
//! variants (strict/inclusive x N2/N3), built for scientific review, not
//! for training. Never registers a scheduler entry, never trains or
//! calibrates a model, never touches `crates/api`, FIRMS, or FWI.
//!
//! Real `cell_static` features (previously hardcoded to `0.0` placeholders
//! in the phase 3B.3 pilot) are wired in here via a resolution-9-to-8
//! aggregation (`dataset::features_h3`), and the real, already-built
//! historical calendar is used for weekend/public-holiday/season features
//! instead of placeholder zeros. `school_holiday` remains `None`: no
//! verified source exists, unchanged from phase 3B.3.

use std::collections::{HashMap, HashSet};

use anyhow::Context;
use chrono::{Datelike, NaiveDate, Utc};
use dataset::{
    checksums::logical_checksum,
    exclusions::ExclusionReason,
    features_h3::{self, Res8AggregatedFeatures, Res9Features},
    negative_design::{ExclusionStrategy, is_within_window},
    normalization::{self, FeatureStatistics, ImputationRule},
    rows::{RowCategory, RowFeatures, deterministic_row_key, row_checksum},
    splits::Split,
};
use grid::{CellIndex, Resolution, cell_from_db, cell_to_db};
use serde_json::json;
use store::{
    AnyCauseEventForNegativeDesign, DatasetBuildCounts, DatasetEventLinkRecord,
    DatasetExclusionRecord, DatasetRowRecord, DatasetVersionSpec, HumanDatasetCandidateEvent,
    Store,
};

use crate::config::Config;

const CODE_VERSION: &str = env!("CARGO_PKG_VERSION");
const CANDIDATE_LOGICAL_PREFIX: &str = "erytheon_human_ignition_cell_day_v1_candidate";
const CALENDAR_RULE_LOGICAL_ID: &str = "erytheon_calendar_generation_v1";
const LOW_CONFIDENCE_GEOGRAPHIC_CATEGORIES: &[&str] = &[
    "municipality_centroid_probable",
    "rounded_coordinate_probable",
];
const PERIOD_START: (i32, u32, u32) = (2020, 1, 1);
const PERIOD_END: (i32, u32, u32) = (2026, 12, 31);
const NUMERIC_FEATURE_NAMES: [&str; 7] = [
    "wui",
    "road",
    "agri",
    "population",
    "poi",
    "power_line",
    "hist",
];

#[derive(Clone, Copy, Debug)]
pub struct CandidateBuildOptions {
    pub dry_run: bool,
    pub seed: i64,
    pub ratio: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    Strict,
    Inclusive,
}

impl Variant {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Inclusive => "inclusive",
        }
    }
}

fn split_bounds(split: Split) -> (NaiveDate, NaiveDate) {
    let d = |y: i32, m: u32, day: u32| NaiveDate::from_ymd_opt(y, m, day).unwrap();
    match split {
        Split::Train => (d(2020, 1, 1), d(2023, 12, 31)),
        Split::Calibration => (d(2024, 1, 1), d(2024, 12, 31)),
        Split::Test => (d(2025, 1, 1), d(2025, 12, 31)),
        Split::Prospective => (d(2026, 1, 1), d(2026, 12, 31)),
    }
}

const fn mix64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    x
}

/// Builds the four phase 3B.5 candidate datasets on the isolated DB. See
/// `PHASE3B5_CANDIDATE_DATASET_REPORT.md` for the design and measured
/// results. Never trains, calibrates, or finalizes any dataset version.
// One long orchestration function by design: it runs the phase 3B.5
// pipeline's stages in a single readable sequence (aggregate features,
// load events, build positives, sample negatives, build all four
// variants); splitting it further would scatter tightly-coupled shared
// state (res8_map, events, calendar) across more function signatures
// without reducing what the pipeline actually does.
#[allow(clippy::too_many_lines)]
pub async fn build_candidate_datasets(
    config: Config,
    options: CandidateBuildOptions,
) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize dataset foundation database")?;
    let period_start = NaiveDate::from_ymd_opt(PERIOD_START.0, PERIOD_START.1, PERIOD_START.2)
        .context("invalid period start")?;
    let period_end = NaiveDate::from_ymd_opt(PERIOD_END.0, PERIOD_END.1, PERIOD_END.2)
        .context("invalid period end")?;

    // --- 1. Resolution-9 -> resolution-8 feature aggregation (once). ---
    let cell_static_rows = store
        .all_cell_static_rows()
        .await
        .context("failed to load cell_static for res9->res8 aggregation")?;
    let res9_pairs: Vec<(CellIndex, Res9Features)> = cell_static_rows
        .iter()
        .map(|row| (row.cell, parse_res9_features(&row.features)))
        .collect();
    let res8_resolution = Resolution::try_from(8).context("invalid resolution 8")?;
    let res8_map = features_h3::aggregate_all_to_res8(&res9_pairs, res8_resolution);
    let res8_feature_checksum = logical_checksum(&(
        res8_map.len(),
        cell_static_rows.len(),
        "res9_to_res8_mean_any_combustible_v1",
    ));
    // Sorted explicitly: `res8_map` is a HashMap, whose iteration order is
    // randomized per-process in Rust (DoS protection), not a deterministic
    // property of its contents. sample_negatives_for_split indexes into
    // this Vec by a seeded hash, so an unsorted, iteration-order-dependent
    // Vec would silently break reproducibility across runs — the same
    // seed would select a different actual cell each time the process
    // restarted, even though the *set* of eligible cells is identical.
    let mut eligible_negative_cells: Vec<i64> = res8_map
        .iter()
        .filter(|(_, aggregated)| aggregated.combustible && aggregated.has_features())
        .map(|(cell, _)| cell_to_db(*cell))
        .collect();
    eligible_negative_cells.sort_unstable();
    anyhow::ensure!(
        !eligible_negative_cells.is_empty(),
        "no eligible combustible resolution-8 cells found after aggregation"
    );

    // --- 2. Calendar lookup (real, not placeholder). ---
    let calendar_rule_id = store
        .calendar_rule_version_id(CALENDAR_RULE_LOGICAL_ID)
        .await
        .context("failed to look up calendar rule version")?
        .context("historical calendar not built yet; run build-historical-calendar first")?;
    let calendar_days = store
        .calendar_days_in_range(&calendar_rule_id, period_start, period_end)
        .await
        .context("failed to load historical calendar")?;
    let calendar_by_date: HashMap<NaiveDate, store::CalendarDayLookup> = calendar_days
        .into_iter()
        .map(|day| (day.date, day))
        .collect();

    // --- 3. Events (all causes, for negative exclusion windows). ---
    let events = store
        .all_events_with_geographic_quality(period_start, period_end)
        .await
        .context("failed to load events")?;
    let known_event_celldays: HashSet<(i64, NaiveDate)> = store
        .known_event_cell_dates(period_start, period_end)
        .await
        .context("failed to load known event cell-dates")?
        .into_iter()
        .collect();

    // --- 4. Positive candidate events (human_known). ---
    let candidate_events = store
        .human_dataset_candidate_events(period_start, period_end)
        .await
        .context("failed to load human dataset candidate events")?;

    let (strict_positive_rows, inclusive_positive_rows, positive_exclusions) =
        build_positive_rows(&candidate_events, &res8_map, &calendar_by_date);

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "phase": "3b5_setup",
            "res8_cells_aggregated": res8_map.len(),
            "res8_feature_checksum": res8_feature_checksum,
            "eligible_negative_cells": eligible_negative_cells.len(),
            "strict_positive_rows": strict_positive_rows.len(),
            "inclusive_positive_rows": inclusive_positive_rows.len(),
            "positive_exclusions": positive_exclusions.len(),
        }))?
    );

    if options.dry_run {
        return Ok(());
    }

    // --- 5. Negative sampling, once per strategy (shared across variants). ---
    // A type alias would only rename this once; the nesting itself
    // (strategy -> split -> sampled cell-days) is the real shape of the
    // data and is clearer written out than hidden behind an alias used
    // in exactly one place.
    #[allow(clippy::type_complexity)]
    let mut negatives_by_strategy: HashMap<&str, HashMap<Split, Vec<(i64, NaiveDate)>>> =
        HashMap::new();
    for strategy in [ExclusionStrategy::N2, ExclusionStrategy::N3] {
        let mut by_split = HashMap::new();
        for split in [
            Split::Train,
            Split::Calibration,
            Split::Test,
            Split::Prospective,
        ] {
            let needed = inclusive_positive_count_for_split(&inclusive_positive_rows, split)
                * usize::try_from(options.ratio).unwrap_or(1);
            let selected = sample_negatives_for_split(
                &eligible_negative_cells,
                &events,
                &known_event_celldays,
                split,
                strategy,
                needed,
                options.seed,
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "phase": "3b5_negative_sampling",
                    "strategy": strategy.id(),
                    "split": split.as_str(),
                    "needed": needed,
                    "sampled": selected.len(),
                }))?
            );
            by_split.insert(split, selected);
        }
        negatives_by_strategy.insert(strategy.id(), by_split);
    }

    // --- 6. Build the four dataset versions. ---
    for strategy in [ExclusionStrategy::N2, ExclusionStrategy::N3] {
        let negatives = &negatives_by_strategy[strategy.id()];
        for variant in [Variant::Strict, Variant::Inclusive] {
            let positive_rows = match variant {
                Variant::Strict => &strict_positive_rows,
                Variant::Inclusive => &inclusive_positive_rows,
            };
            build_one_variant(
                &store,
                variant,
                strategy,
                positive_rows,
                negatives,
                &positive_exclusions,
                &res8_map,
                &calendar_by_date,
                period_start,
                period_end,
                options,
                &res8_feature_checksum,
            )
            .await?;
        }
    }

    Ok(())
}

fn parse_res9_features(features: &serde_json::Value) -> Res9Features {
    Res9Features {
        poi: features.get("poi").and_then(serde_json::Value::as_f64),
        wui: features.get("wui").and_then(serde_json::Value::as_f64),
        agri: features.get("agri").and_then(serde_json::Value::as_f64),
        hist: features.get("hist").and_then(serde_json::Value::as_f64),
        road: features.get("road").and_then(serde_json::Value::as_f64),
        population: features
            .get("population")
            .and_then(serde_json::Value::as_f64),
        power_line: features
            .get("power_line")
            .and_then(serde_json::Value::as_f64),
        combustible: features
            .get("combustible")
            .and_then(serde_json::Value::as_bool),
    }
}

fn row_features_from_res8(
    aggregated: Option<&Res8AggregatedFeatures>,
    combustible_override: Option<bool>,
    date: NaiveDate,
    calendar_by_date: &HashMap<NaiveDate, store::CalendarDayLookup>,
) -> RowFeatures {
    let day = calendar_by_date.get(&date);
    RowFeatures {
        wui: aggregated.and_then(|a| a.wui).unwrap_or(0.0),
        road: aggregated.and_then(|a| a.road).unwrap_or(0.0),
        agri: aggregated.and_then(|a| a.agri).unwrap_or(0.0),
        population: aggregated.and_then(|a| a.population).unwrap_or(0.0),
        poi: aggregated.and_then(|a| a.poi).unwrap_or(0.0),
        power_line: aggregated.and_then(|a| a.power_line).unwrap_or(0.0),
        hist: aggregated.and_then(|a| a.hist).unwrap_or(0.0),
        combustible: combustible_override
            .unwrap_or_else(|| aggregated.is_some_and(|a| a.combustible)),
        weekend: day.is_some_and(|d| d.is_weekend),
        school_holiday: None,
        public_holiday: day.is_some_and(|d| d.public_holiday),
        season_sine: day.map_or(0.0, |d| d.season_sine),
        season_cosine: day.map_or(0.0, |d| d.season_cosine),
    }
}

#[allow(clippy::too_many_lines)]
fn build_positive_rows(
    candidate_events: &[HumanDatasetCandidateEvent],
    res8_map: &HashMap<CellIndex, Res8AggregatedFeatures>,
    calendar_by_date: &HashMap<NaiveDate, store::CalendarDayLookup>,
) -> (
    Vec<DatasetRowRecord>,
    Vec<DatasetRowRecord>,
    Vec<DatasetExclusionRecord>,
) {
    let mut grouped: std::collections::BTreeMap<
        (i64, NaiveDate),
        Vec<&HumanDatasetCandidateEvent>,
    > = std::collections::BTreeMap::new();
    for event in candidate_events {
        grouped
            .entry((event.h3, event.occurred_on_local))
            .or_default()
            .push(event);
    }

    let mut strict_rows = Vec::new();
    let mut inclusive_rows = Vec::new();
    let mut exclusions = Vec::new();

    for ((h3, date), events_here) in &grouped {
        let Some(split) = Split::for_year(date.year()) else {
            for event in events_here {
                exclusions.push(exclusion_record(
                    Some(event.id.clone()),
                    Some(*h3),
                    Some(*date),
                    ExclusionReason::OutOfPeriod,
                    "period_2020_2026",
                ));
            }
            continue;
        };
        let (eligible, duplicates) = events_here
            .iter()
            .partition::<Vec<&&HumanDatasetCandidateEvent>, _>(|event| {
                !event.certain_duplicate_non_anchor
            });
        for event in &duplicates {
            exclusions.push(exclusion_record(
                Some(event.id.clone()),
                Some(*h3),
                Some(*date),
                ExclusionReason::CertainDuplicate,
                "erytheon_duplicate_rules_v1",
            ));
        }
        if eligible.is_empty() {
            continue;
        }
        let representative = eligible[0];
        let combustible = representative.original_cell_combustible;
        let features_present = representative.cell_features_present;
        let low_geo_confidence = eligible.iter().any(|event| {
            LOW_CONFIDENCE_GEOGRAPHIC_CATEGORIES.contains(&event.geographic_category.as_str())
        });
        let accidental = eligible
            .iter()
            .any(|event| event.requires_accidental_sensitivity_analysis);
        let mut quality_flags = Vec::new();
        if accidental {
            quality_flags.push("requires_accidental_sensitivity_analysis".to_owned());
        }
        if low_geo_confidence {
            quality_flags.push("low_geographic_confidence".to_owned());
        }
        if combustible != Some(true) {
            quality_flags.push("non_combustible_or_unknown_cell".to_owned());
        }
        if !features_present {
            quality_flags.push("missing_features".to_owned());
        }

        let strict_ok = combustible == Some(true) && features_present && !low_geo_confidence;
        let Ok(cell) = cell_from_db(*h3) else {
            continue;
        };
        let aggregated = res8_map.get(&cell);
        let features = row_features_from_res8(aggregated, combustible, *date, calendar_by_date);
        let row = build_row(
            *h3,
            *date,
            split,
            1,
            RowCategory::Positive,
            "human_known_bdiff_v1",
            &features,
            &quality_flags,
            eligible
                .iter()
                .map(|event| DatasetEventLinkRecord {
                    ignition_event_id: event.id.clone(),
                    role: "primary".to_owned(),
                    label_quality_confidence: Some(event.label_confidence.clone()),
                    geographic_quality_category: Some(event.geographic_category.clone()),
                    duplicate_group_id: None,
                    inclusion_rule: "human_known_v1".to_owned(),
                    justification: "human_known cause, admissible after quality assessment"
                        .to_owned(),
                })
                .collect(),
        );
        inclusive_rows.push(row.clone());
        if strict_ok {
            strict_rows.push(row);
        } else {
            let reason = if !features_present {
                ExclusionReason::MissingFeatures
            } else if combustible != Some(true) {
                ExclusionReason::NonCombustibleCell
            } else {
                ExclusionReason::InsufficientGeographicQuality
            };
            exclusions.push(exclusion_record(
                None,
                Some(*h3),
                Some(*date),
                reason,
                "strict_v1_candidate",
            ));
        }
    }
    (strict_rows, inclusive_rows, exclusions)
}

#[allow(clippy::too_many_arguments)]
fn build_row(
    h3: i64,
    date: NaiveDate,
    split: Split,
    label: i16,
    category: RowCategory,
    selection_method: &str,
    features: &RowFeatures,
    quality_flags: &[String],
    event_links: Vec<DatasetEventLinkRecord>,
) -> DatasetRowRecord {
    let deterministic_key = deterministic_row_key(CANDIDATE_LOGICAL_PREFIX, h3, date, category);
    let checksum = row_checksum(
        &deterministic_key,
        split,
        u8::try_from(label).unwrap_or(0),
        features,
        quality_flags,
        &[],
    );
    DatasetRowRecord {
        h3,
        h3_resolution: 8,
        local_date: date,
        reference_timestamp: date
            .and_hms_opt(12, 0, 0)
            .and_then(|naive| naive.and_local_timezone(Utc).single())
            .unwrap_or_else(Utc::now),
        split: split.as_str().to_owned(),
        label,
        row_category: category.as_str().to_owned(),
        selection_method: selection_method.to_owned(),
        weight: None,
        quality_flags: json!(quality_flags),
        features: serde_json::to_value(features).unwrap_or_else(|_| json!({})),
        temporal_availability: json!({
            "features": "current_snapshot_applied_historically",
            "calendar": "historical_exact_or_unavailable_historically"
        }),
        deterministic_key,
        row_checksum: checksum,
        justification: "phase 3B.5 candidate build; not yet finalized".to_owned(),
        snapshot_ids: Vec::new(),
        event_links,
    }
}

fn exclusion_record(
    ignition_event_id: Option<String>,
    h3: Option<i64>,
    local_date: Option<NaiveDate>,
    reason: ExclusionReason,
    rule: &str,
) -> DatasetExclusionRecord {
    DatasetExclusionRecord {
        ignition_event_id,
        h3,
        local_date,
        reason_category: reason.as_str().to_owned(),
        rule: rule.to_owned(),
        rule_version: Some("v1_candidate".to_owned()),
        details: json!({}),
        reintegration_possible: true,
    }
}

fn inclusive_positive_count_for_split(rows: &[DatasetRowRecord], split: Split) -> usize {
    rows.iter()
        .filter(|row| row.split == split.as_str())
        .count()
}

/// Deterministically samples up to `needed` negative candidates for one
/// split under one strategy, from the full (eligible cell x split day
/// range) space without materializing it: candidates are drawn by hashing
/// `(seed, strategy, split, pool index)` directly into a cell index and a
/// day offset, oversampling and filtering by the exclusion window and
/// known-event overlap until either `needed` is reached or the oversample
/// factor is exhausted (reported honestly as a shortfall, never padded).
fn sample_negatives_for_split(
    eligible_cells: &[i64],
    events: &[AnyCauseEventForNegativeDesign],
    known_event_celldays: &HashSet<(i64, NaiveDate)>,
    split: Split,
    strategy: ExclusionStrategy,
    needed: usize,
    seed: i64,
) -> Vec<(i64, NaiveDate)> {
    if needed == 0 {
        return Vec::new();
    }
    let (start, end) = split_bounds(split);
    let day_count = u64::try_from((end - start).num_days() + 1).unwrap_or(1);
    let split_tag = mix64(split as u64 ^ 0xA5A5_A5A5_A5A5_A5A5);
    let strategy_tag = mix64(match strategy {
        ExclusionStrategy::N0 => 0,
        ExclusionStrategy::N1 => 1,
        ExclusionStrategy::N2 => 2,
        ExclusionStrategy::N3 => 3,
    });
    let base_seed = seed.unsigned_abs() ^ split_tag ^ strategy_tag;

    let mut oversample: u64 = 5;
    let max_oversample: u64 = 40;
    let mut selected: Vec<(i64, NaiveDate)> = Vec::new();
    loop {
        selected.clear();
        let mut visited: HashSet<(i64, NaiveDate)> = HashSet::new();
        let pool_size = needed as u64 * oversample;
        for i in 0..pool_size {
            if selected.len() >= needed {
                break;
            }
            let h = mix64(base_seed ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let cell_index = usize::try_from(h % eligible_cells.len() as u64).unwrap_or(0);
            let h3 = eligible_cells[cell_index];
            let h2 = mix64(h ^ 0xD1B5_4A32_D192_ED03);
            let offset = i64::try_from(h2 % day_count).unwrap_or(0);
            let Some(date) = start.checked_add_signed(chrono::Duration::days(offset)) else {
                continue;
            };
            if !visited.insert((h3, date)) {
                continue;
            }
            if known_event_celldays.contains(&(h3, date)) {
                continue;
            }
            let Ok(candidate_cell) = cell_from_db(h3) else {
                continue;
            };
            let excluded = events.iter().any(|event| {
                let day_gap = (event.occurred_on_local - date).num_days().abs();
                if day_gap > 5 {
                    return false;
                }
                let Ok(event_cell) = cell_from_db(event.h3) else {
                    return false;
                };
                let window = strategy.window(Some(event.geographic_category.as_str()));
                is_within_window(
                    candidate_cell,
                    date,
                    event_cell,
                    event.occurred_on_local,
                    window,
                )
                .unwrap_or(true)
            });
            if excluded {
                continue;
            }
            selected.push((h3, date));
        }
        if selected.len() >= needed || oversample >= max_oversample {
            break;
        }
        oversample *= 2;
    }
    selected
}

#[allow(clippy::too_many_arguments)]
// One long function by design: persists one dataset version end to
// end (rows, exclusions, train-only stats, spec, build bookkeeping) as
// a single auditable sequence.
#[allow(clippy::too_many_lines)]
async fn build_one_variant(
    store: &Store,
    variant: Variant,
    strategy: ExclusionStrategy,
    positive_rows: &[DatasetRowRecord],
    negatives_by_split: &HashMap<Split, Vec<(i64, NaiveDate)>>,
    positive_exclusions: &[DatasetExclusionRecord],
    res8_map: &HashMap<CellIndex, Res8AggregatedFeatures>,
    calendar_by_date: &HashMap<NaiveDate, store::CalendarDayLookup>,
    period_start: NaiveDate,
    period_end: NaiveDate,
    options: CandidateBuildOptions,
    res8_feature_checksum: &str,
) -> anyhow::Result<()> {
    let logical_id = format!(
        "{CANDIDATE_LOGICAL_PREFIX}_{}_{}",
        variant.as_str(),
        strategy.id()
    );

    let mut negative_rows = Vec::new();
    for (&split, candidates) in negatives_by_split {
        for &(h3, date) in candidates {
            let Ok(cell) = cell_from_db(h3) else {
                continue;
            };
            let aggregated = res8_map.get(&cell);
            let features = row_features_from_res8(aggregated, Some(true), date, calendar_by_date);
            negative_rows.push(build_row(
                h3,
                date,
                split,
                0,
                RowCategory::NegativePilot,
                strategy.id(),
                &features,
                &["candidate_negative".to_owned()],
                Vec::new(),
            ));
        }
    }

    let mut rows = positive_rows.to_vec();
    rows.extend(negative_rows);

    let positive_count = rows.iter().filter(|row| row.label == 1).count();
    let negative_count = rows.len() - positive_count;

    // --- train-only normalization / imputation, computed from this build's own train rows ---
    let train_rows: Vec<&DatasetRowRecord> = rows.iter().filter(|r| r.split == "train").collect();
    let mut feature_stats: Vec<FeatureStatistics> = Vec::new();
    let mut imputation_rules: Vec<ImputationRule> = Vec::new();
    for feature_name in NUMERIC_FEATURE_NAMES {
        let values: Vec<Option<f64>> = train_rows
            .iter()
            .map(|row| {
                row.features
                    .get(feature_name)
                    .and_then(serde_json::Value::as_f64)
            })
            .collect();
        let stats = normalization::train_only_statistics(feature_name, &values);
        let rule = normalization::fit_imputation_rule(&stats);
        feature_stats.push(stats);
        imputation_rules.push(rule);
    }
    let normalization_report = json!({
        "computed_from": "train split rows of this build only",
        "train_row_count": train_rows.len(),
        "statistics": feature_stats,
        "imputation": imputation_rules,
    });

    let spec = DatasetVersionSpec {
        logical_id: logical_id.clone(),
        name: format!(
            "ERYTHEON human ignition cell-day candidate ({}, {})",
            variant.as_str(),
            strategy.id()
        ),
        description: "Phase 3B.5 candidate dataset for scientific review; not finalized, \
                      not used for training."
            .to_owned(),
        observation_unit: "h3_cell_x_civil_date".to_owned(),
        h3_resolution: 8,
        timezone: "Europe/Paris".to_owned(),
        period_start,
        period_end,
        variant: variant.as_str().to_owned(),
        code_version: CODE_VERSION.to_owned(),
        migrations: json!([13, 14, 15]),
        quality_rule_versions: json!([
            "erytheon_taxonomy_v1",
            "erytheon_label_quality_v1",
            "erytheon_geographic_quality_v1",
            "erytheon_combustibility_assessment_v1",
            "erytheon_duplicate_rules_v1"
        ]),
        feature_snapshot_ids: json!([]),
        calendar_rule_version_id: None,
        inclusion_rules: json!({"cause": "human_known", "variant": variant.as_str()}),
        exclusion_rules: json!({
            "certain_duplicate_non_anchor": true,
            "strict_requires_combustible_and_features_and_geo_quality": variant == Variant::Strict,
            "negative_strategy": strategy.id(),
            "res8_feature_checksum": res8_feature_checksum,
        }),
        negative_strategy: strategy.id().to_owned(),
        negative_parameters: json!({"seed": options.seed, "ratio": options.ratio}),
        seed: options.seed,
        splits: json!({
            "train": "2020-2023", "calibration": "2024",
            "test": "2025", "prospective": "2026"
        }),
        author_or_pipeline: "engine::candidate_pipeline".to_owned(),
        notes: Some(normalization_report.to_string()),
    };

    let (dataset_version_id, reused_existing_version) = store
        .get_or_create_dataset_version(&spec)
        .await
        .context("failed to get or create candidate dataset version")?;
    store
        .set_dataset_version_status(&dataset_version_id, "building")
        .await?;
    let build_id = store
        .start_dataset_build(
            &dataset_version_id,
            CODE_VERSION,
            "candidate_pipeline_v1",
            &json!({"seed": options.seed, "ratio": options.ratio}),
            options.seed,
        )
        .await
        .context("failed to start dataset build")?;

    let persist_result = store
        .persist_dataset_rows(&dataset_version_id, &build_id, &rows)
        .await;
    if let Err(error) = persist_result {
        store
            .finish_dataset_build(&build_id, false, None, None, Some(&error.to_string()))
            .await?;
        store
            .set_dataset_version_status(&dataset_version_id, "failed")
            .await?;
        return Err(error).context("failed to persist candidate dataset rows");
    }

    let mut all_exclusions = positive_exclusions.to_vec();
    // Negative-side non-eligibility is implicit (candidates that failed
    // the window check were simply never sampled, not persisted as
    // exclusions) — only positive-path exclusions are recorded here,
    // consistent with phase 3B.3's schema (event/cell-day exclusions are
    // about admissibility of a *known* subject, not about candidates that
    // were never drawn).
    let deduped_exclusions = dedup_exclusions(&mut all_exclusions);
    store
        .persist_dataset_exclusions(&dataset_version_id, deduped_exclusions)
        .await?;

    let dataset_checksum =
        logical_checksum(&(&logical_id, rows.len(), options.seed, strategy.id()));
    let counts = DatasetBuildCounts {
        row_count: i64::try_from(rows.len())?,
        positive_count: i64::try_from(positive_count)?,
        negative_count: i64::try_from(negative_count)?,
        exclusion_count: i64::try_from(all_exclusions.len())?,
    };
    store
        .finish_dataset_build(
            &build_id,
            true,
            Some(&counts),
            Some(&dataset_checksum),
            None,
        )
        .await?;
    // Status stays "draft" per mission: no build in this phase is
    // authorized to finalize or even mark "validated".
    store
        .set_dataset_version_status(&dataset_version_id, "draft")
        .await?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "phase": "3b5_build",
            "variant": variant.as_str(),
            "strategy": strategy.id(),
            "logical_id": logical_id,
            "dataset_version_id": dataset_version_id,
            "reused_existing_version": reused_existing_version,
            "build_id": build_id,
            "status": "draft",
            "row_count": counts.row_count,
            "positive_count": counts.positive_count,
            "negative_count": counts.negative_count,
            "exclusion_count": counts.exclusion_count,
            "checksum": dataset_checksum,
        }))?
    );
    Ok(())
}

fn dedup_exclusions(exclusions: &mut Vec<DatasetExclusionRecord>) -> &[DatasetExclusionRecord] {
    exclusions.sort_by(|a, b| {
        (&a.ignition_event_id, a.h3, a.local_date, &a.reason_category).cmp(&(
            &b.ignition_event_id,
            b.h3,
            b.local_date,
            &b.reason_category,
        ))
    });
    exclusions.dedup_by(|a, b| {
        a.ignition_event_id == b.ignition_event_id
            && a.h3 == b.h3
            && a.local_date == b.local_date
            && a.reason_category == b.reason_category
    });
    exclusions
}
