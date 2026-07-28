//! Phase 3B.3 dataset foundation orchestration: feature-snapshot
//! registration, historical calendar construction, and pilot dataset
//! builds. Manual commands only; never registered with the scheduler.
//! Does not replace the active model, train anything, or switch any
//! reader to `ml.dataset_rows`.

use std::collections::{BTreeMap, HashSet};

use anyhow::Context;
use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use dataset::{
    calendar,
    negatives::{self, EligibleCellDay},
    rows::{RowCategory, RowFeatures, deterministic_row_key, row_checksum},
    splits::Split,
    temporal::TemporalClassification,
};
use serde_json::json;
use store::{
    CalendarRuleVersion, DatasetBuildCounts, DatasetEventLinkRecord, DatasetExclusionRecord,
    DatasetRowRecord, DatasetVersionSpec, FeatureSnapshotSpec, HistoricalCalendarDayRecord, Store,
};

use crate::config::Config;

const CODE_VERSION: &str = env!("CARGO_PKG_VERSION");
const CELL_STATIC_FAMILY: &str = "cell_static_bundle";
const CALENDAR_RULE_LOGICAL_ID: &str = "erytheon_calendar_generation_v1";
const DATASET_LOGICAL_ID: &str = "erytheon_human_ignition_cell_day_v1_pilot";

#[derive(Clone, Debug)]
pub struct SnapshotOptions {
    pub dry_run: bool,
}

/// Registers the current `public.cell_static` bundle as a feature
/// snapshot. Never duplicates the 920,000+ rows: only metadata and a
/// server-computed checksum are stored. Classified
/// `current_snapshot_applied_historically` because no historical vintage
/// exists for OSM/CORINE/INSEE in this environment (see audit).
pub async fn register_feature_snapshot(
    config: Config,
    options: SnapshotOptions,
) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize dataset foundation database")?;
    let (checksum, cell_count) = store
        .cell_static_snapshot_summary()
        .await
        .context("failed to summarize cell_static")?;
    anyhow::ensure!(cell_count > 0, "cell_static is empty; nothing to register");
    let now = Utc::now();
    let spec = FeatureSnapshotSpec {
        family: CELL_STATIC_FAMILY.to_owned(),
        source: "production cell_static (OSM + CORINE + INSEE, bundled load)".to_owned(),
        provider: Some("internal static-layer pipeline".to_owned()),
        vintage: None,
        valid_from: None,
        valid_until: None,
        available_from: now,
        available_until: None,
        retrieved_at: None,
        code_version: CODE_VERSION.to_owned(),
        normalizer_version: "static_layers_v1".to_owned(),
        parameters: json!({
            "h3_resolution": config.h3_resolution,
            "neighborhood_distance": 1,
            "history_kernel_distance": 2,
            "wui_distance_km": 0.05
        }),
        source_checksum: None,
        logical_checksum: checksum,
        reference_table: "public.cell_static".to_owned(),
        cell_count,
        h3_resolution: i16::from(config.h3_resolution),
        geographic_coverage: None,
        temporal_classification: TemporalClassification::CurrentSnapshotAppliedHistorically
            .as_str()
            .to_owned(),
        limitations: json!([
            "no documented per-source vintage for OSM/CORINE/INSEE",
            "current snapshot applied retroactively to 2020-2025 events; bias risk",
            "school_zone is a hardcoded placeholder, not real data"
        ]),
        license_attribution: None,
        notes: Some(
            "Registered by the phase 3B.3 dataset foundation; not the final \
                     historical vintage."
                .to_owned(),
        ),
    };
    let summary = json!({
        "cell_count": spec.cell_count,
        "logical_checksum": spec.logical_checksum,
        "temporal_classification": spec.temporal_classification,
        "dry_run": options.dry_run
    });
    if options.dry_run {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    let snapshot_id = store
        .register_feature_snapshot(&spec)
        .await
        .context("failed to register feature snapshot")?;
    store
        .activate_feature_snapshot(&snapshot_id)
        .await
        .context("failed to activate feature snapshot")?;
    let mut output = summary_as_object(&summary);
    output.insert("snapshot_id".to_owned(), json!(snapshot_id));
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn summary_as_object(value: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    value.as_object().cloned().unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct CalendarOptions {
    pub from_year: i32,
    pub to_year: i32,
    pub dry_run: bool,
}

/// Builds the versioned historical calendar for `[from_year, to_year]`.
/// Public holidays are computed deterministically for every year
/// (fixed by law). `school_holiday` stays `NULL` unless a verified source
/// is later wired in; it is never fabricated or defaulted to `false`.
pub async fn build_historical_calendar(
    config: Config,
    options: CalendarOptions,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        options.from_year <= options.to_year,
        "from_year must be <= to_year"
    );
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize dataset foundation database")?;
    let parameters = json!({
        "easter_algorithm": "meeus_jones_butcher",
        "season_definition": "meteorological",
        "school_holiday_source": "none (unavailable_historically for all years today)"
    });
    let checksum = dataset::checksums::logical_checksum(&(CALENDAR_RULE_LOGICAL_ID, &parameters));
    let rule = CalendarRuleVersion {
        logical_id: CALENDAR_RULE_LOGICAL_ID.to_owned(),
        rule_type: "public_holiday".to_owned(),
        description: "Deterministic French public holidays, meteorological seasons, \
                      and seasonal sine/cosine encoding for 2020-2026."
            .to_owned(),
        parameters,
        code_version: CODE_VERSION.to_owned(),
        status: "active".to_owned(),
        checksum,
        notes: Some(
            "school_holiday intentionally left NULL: no verified source for \
                     2020-2024 in this environment."
                .to_owned(),
        ),
    };
    let mut days = Vec::new();
    for year in options.from_year..=options.to_year {
        for day in calendar::build_year(year) {
            let available_from = Utc
                .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
                .single()
                .context("invalid available_from")?;
            let logical_checksum = dataset::checksums::logical_checksum(&(
                day.date,
                "unspecified",
                day.public_holiday,
                day.season,
            ));
            days.push(HistoricalCalendarDayRecord {
                date: day.date,
                school_zone: "unspecified".to_owned(),
                year: i16::try_from(day.year)?,
                month: i16::try_from(day.month)?,
                day_of_week: i16::try_from(day.day_of_week)?,
                is_weekend: day.is_weekend,
                public_holiday: day.public_holiday,
                public_holiday_label: day.public_holiday_label.clone(),
                school_holiday: None,
                school_holiday_label: None,
                is_day_before_public_holiday: day.is_day_before_public_holiday,
                is_day_after_public_holiday: day.is_day_after_public_holiday,
                season: i16::from(day.season),
                season_sine: day.season_sine,
                season_cosine: day.season_cosine,
                available_from,
                source: "computed:erytheon_calendar_generation_v1".to_owned(),
                // Public holidays, weekday, and season are exactly and
                // deterministically computable for any past date, but
                // school_holiday is NULL above (no verified source), so the
                // row as a whole cannot honestly be classified
                // historical_exact yet; this must track school_holiday's
                // presence, matching the database CHECK constraint exactly.
                temporal_classification: TemporalClassification::UnavailableHistorically
                    .as_str()
                    .to_owned(),
                logical_checksum,
            });
        }
    }
    let summary = json!({
        "from_year": options.from_year,
        "to_year": options.to_year,
        "days_computed": days.len(),
        "rule_checksum": rule.checksum,
        "school_holiday_available": false,
        "dry_run": options.dry_run
    });
    if options.dry_run {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    let rule_version_id = store
        .ensure_calendar_rule_version(&rule)
        .await
        .context("failed to register calendar rule version")?;
    store
        .persist_historical_calendar_days(&rule_version_id, &days)
        .await
        .context("failed to persist historical calendar")?;
    let mut output = summary_as_object(&summary);
    output.insert("rule_version_id".to_owned(), json!(rule_version_id));
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[derive(Clone, Debug)]
pub struct DatasetBuildOptions {
    pub dry_run: bool,
    pub seed: i64,
    pub negatives_per_split_year: usize,
}

/// Geographic categories flagged as low-confidence for the strict variant.
/// `quality::assess_geography` (see `crates/quality/src/lib.rs`) never
/// actually emits `precise_reported`/`estimated_reported` in this dataset —
/// those are aspirational categories from the phase 3A specification with
/// no assignment path today. `precision_undocumented` is the ceiling of
/// quality currently achievable for real reported coordinates (see
/// `BDIFF_QUALITY.md`) and must not be treated as low confidence; only the
/// two genuinely suspect categories are excluded from strict.
const LOW_CONFIDENCE_GEOGRAPHIC_CATEGORIES: &[&str] = &[
    "municipality_centroid_probable",
    "rounded_coordinate_probable",
];

/// Whether `category` should exclude a positive from the strict variant.
/// Pulled out of `build_human_dataset` so the real, currently-assigned
/// categories (`municipality_centroid_probable`, `precision_undocumented`,
/// `rounded_coordinate_probable`) can be regression-tested directly.
fn is_low_confidence_geographic_category(category: &str) -> bool {
    LOW_CONFIDENCE_GEOGRAPHIC_CATEGORIES.contains(&category)
}

/// Builds a pilot `erytheon_human_ignition_cell_day_v1` dataset (strict
/// and inclusive variants) over 2020-2026. Positives come from
/// `human_known` BDIFF events; a small deterministic pilot negative
/// population is added purely to exercise the architecture (mission
/// section 21) — this is NOT the final scientific negative strategy.
#[allow(clippy::too_many_lines)]
pub async fn build_human_dataset(
    config: Config,
    options: DatasetBuildOptions,
) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize dataset foundation database")?;
    let period_start = NaiveDate::from_ymd_opt(2020, 1, 1).context("invalid period start")?;
    let period_end = NaiveDate::from_ymd_opt(2026, 12, 31).context("invalid period end")?;

    let candidates = store
        .human_dataset_candidate_events(period_start, period_end)
        .await
        .context("failed to load human dataset candidate events")?;
    let known_event_days: HashSet<(i64, NaiveDate)> = store
        .known_event_cell_dates(period_start, period_end)
        .await
        .context("failed to load known event cell-days")?
        .into_iter()
        .collect();

    let mut grouped: BTreeMap<(i64, NaiveDate), Vec<store::HumanDatasetCandidateEvent>> =
        BTreeMap::new();
    for event in candidates {
        grouped
            .entry((event.h3, event.occurred_on_local))
            .or_default()
            .push(event);
    }

    let mut strict_rows = Vec::new();
    let mut inclusive_rows = Vec::new();
    let mut exclusions = Vec::new();

    for ((h3, date), events) in &grouped {
        let Some(split) = Split::for_year(date.year()) else {
            for event in events {
                exclusions.push(pending_exclusion(
                    Some(event.id.clone()),
                    Some(*h3),
                    Some(*date),
                    dataset::exclusions::ExclusionReason::OutOfPeriod,
                    "period_2020_2026",
                ));
            }
            continue;
        };
        let (eligible, duplicates): (Vec<_>, Vec<_>) = events
            .iter()
            .partition(|event| !event.certain_duplicate_non_anchor);
        for event in &duplicates {
            exclusions.push(pending_exclusion(
                Some(event.id.clone()),
                Some(*h3),
                Some(*date),
                dataset::exclusions::ExclusionReason::CertainDuplicate,
                "erytheon_duplicate_rules_v1",
            ));
        }
        if eligible.is_empty() {
            continue;
        }
        let representative = eligible[0];
        let combustible = representative.original_cell_combustible;
        let features_present = representative.cell_features_present;
        let low_geo_confidence = eligible
            .iter()
            .any(|event| is_low_confidence_geographic_category(&event.geographic_category));
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
        let features = RowFeatures {
            wui: 0.0,
            road: 0.0,
            agri: 0.0,
            population: 0.0,
            poi: 0.0,
            power_line: 0.0,
            hist: 0.0,
            combustible: combustible.unwrap_or(false),
            weekend: false,
            school_holiday: None,
            public_holiday: false,
            season_sine: 0.0,
            season_cosine: 0.0,
        };
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
            // Check missing_features first: `combustible == None` also
            // indicates missing features, so this must be checked before
            // the non-combustible branch, or a cell with no cell_static
            // row at all would be misclassified as non_combustible_cell.
            let reason = if !features_present {
                dataset::exclusions::ExclusionReason::MissingFeatures
            } else if combustible != Some(true) {
                dataset::exclusions::ExclusionReason::NonCombustibleCell
            } else {
                dataset::exclusions::ExclusionReason::InsufficientGeographicQuality
            };
            exclusions.push(pending_exclusion(
                None,
                Some(*h3),
                Some(*date),
                reason,
                "strict_v1_pilot",
            ));
        }
    }

    let combustible_pool = store
        .sample_combustible_cells(2000, options.seed)
        .await
        .context("failed to sample combustible cells for pilot negatives")?;
    let eligible_negative_candidates: Vec<EligibleCellDay> = combustible_pool
        .iter()
        .flat_map(|cell| {
            let h3 = grid::cell_to_db(cell.cell);
            (2020..=2026).filter_map(move |year| {
                Split::for_year(year).map(|_| EligibleCellDay {
                    h3,
                    date: NaiveDate::from_ymd_opt(year, 6, 15).unwrap_or(period_start),
                })
            })
        })
        .filter(|candidate| !known_event_days.contains(&(candidate.h3, candidate.date)))
        .collect();
    let total_negatives = options.negatives_per_split_year * 4;
    let selected_negatives = negatives::select_pilot_negatives(
        &eligible_negative_candidates,
        options.seed.unsigned_abs(),
        total_negatives,
    );
    for candidate in &selected_negatives {
        let Some(split) = Split::for_year(candidate.date.year()) else {
            continue;
        };
        let features = RowFeatures {
            wui: 0.0,
            road: 0.0,
            agri: 0.0,
            population: 0.0,
            poi: 0.0,
            power_line: 0.0,
            hist: 0.0,
            combustible: true,
            weekend: false,
            school_holiday: None,
            public_holiday: false,
            season_sine: 0.0,
            season_cosine: 0.0,
        };
        let row = build_row(
            candidate.h3,
            candidate.date,
            split,
            0,
            RowCategory::NegativePilot,
            negatives::PILOT_STRATEGY_ID,
            &features,
            &["pilot_only".to_owned()],
            Vec::new(),
        );
        strict_rows.push(row.clone());
        inclusive_rows.push(row);
    }

    let summary = json!({
        "positives_grouped_cell_days": grouped.len(),
        "strict_rows": strict_rows.len(),
        "inclusive_rows": inclusive_rows.len(),
        "exclusions": exclusions.len(),
        "pilot_negatives": selected_negatives.len(),
        "seed": options.seed,
        "dry_run": options.dry_run
    });
    if options.dry_run {
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    for (variant, rows) in [("strict", &strict_rows), ("inclusive", &inclusive_rows)] {
        let logical_id = format!("{DATASET_LOGICAL_ID}_{variant}");
        let spec = DatasetVersionSpec {
            logical_id: logical_id.clone(),
            name: format!("ERYTHEON human ignition cell-day pilot ({variant})"),
            description: "Phase 3B.3 pilot dataset to exercise the versioning \
                          architecture; not the final scientific dataset."
                .to_owned(),
            observation_unit: "h3_cell_x_civil_date".to_owned(),
            h3_resolution: i16::from(config.h3_resolution),
            timezone: "Europe/Paris".to_owned(),
            period_start,
            period_end,
            variant: variant.to_owned(),
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
            inclusion_rules: json!({"cause": "human_known", "variant": variant}),
            exclusion_rules: json!({
                "certain_duplicate_non_anchor": true,
                "strict_requires_combustible_and_features_and_geo_quality": variant == "strict"
            }),
            negative_strategy: negatives::PILOT_STRATEGY_ID.to_owned(),
            negative_parameters: json!({"seed": options.seed, "count": total_negatives}),
            seed: options.seed,
            splits: json!({
                "train": "2020-2023", "calibration": "2024",
                "test": "2025", "prospective": "2026"
            }),
            author_or_pipeline: "engine::dataset_pipeline".to_owned(),
            notes: Some("pilot_only".to_owned()),
        };
        let (dataset_version_id, reused_existing_version) = store
            .get_or_create_dataset_version(&spec)
            .await
            .context("failed to get or create dataset version")?;
        store
            .set_dataset_version_status(&dataset_version_id, "building")
            .await?;
        let build_id = store
            .start_dataset_build(
                &dataset_version_id,
                CODE_VERSION,
                "dataset_pipeline_v1",
                &json!({"seed": options.seed}),
                options.seed,
            )
            .await
            .context("failed to start dataset build")?;
        let persist_result = store
            .persist_dataset_rows(&dataset_version_id, &build_id, rows)
            .await;
        let variant_exclusions: Vec<_> = exclusions.clone();
        if let Err(error) = persist_result {
            store
                .finish_dataset_build(&build_id, false, None, None, Some(&error.to_string()))
                .await?;
            store
                .set_dataset_version_status(&dataset_version_id, "failed")
                .await?;
            return Err(error).context("failed to persist dataset rows");
        }
        store
            .persist_dataset_exclusions(&dataset_version_id, &variant_exclusions)
            .await?;
        let positive_count = rows.iter().filter(|row| row.label == 1).count();
        let negative_count = rows.len() - positive_count;
        let counts = DatasetBuildCounts {
            row_count: i64::try_from(rows.len())?,
            positive_count: i64::try_from(positive_count)?,
            negative_count: i64::try_from(negative_count)?,
            exclusion_count: i64::try_from(variant_exclusions.len())?,
        };
        let dataset_checksum =
            dataset::checksums::logical_checksum(&(&logical_id, rows.len(), options.seed));
        store
            .finish_dataset_build(
                &build_id,
                true,
                Some(&counts),
                Some(&dataset_checksum),
                None,
            )
            .await?;
        store
            .set_dataset_version_status(&dataset_version_id, "validated")
            .await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "variant": variant,
                "dataset_version_id": dataset_version_id,
                "reused_existing_version": reused_existing_version,
                "build_id": build_id,
                "row_count": counts.row_count,
                "positive_count": counts.positive_count,
                "negative_count": counts.negative_count,
                "exclusion_count": counts.exclusion_count,
                "checksum": dataset_checksum
            }))?
        );
    }
    Ok(())
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
    let deterministic_key = deterministic_row_key(DATASET_LOGICAL_ID, h3, date, category);
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
            "features": TemporalClassification::CurrentSnapshotAppliedHistorically.as_str(),
            "calendar": TemporalClassification::UnavailableHistorically.as_str()
        }),
        deterministic_key,
        row_checksum: checksum,
        justification: "pilot build; see dataset version notes".to_owned(),
        snapshot_ids: Vec::new(),
        event_links,
    }
}

fn pending_exclusion(
    ignition_event_id: Option<String>,
    h3: Option<i64>,
    local_date: Option<NaiveDate>,
    reason: dataset::exclusions::ExclusionReason,
    rule: &str,
) -> DatasetExclusionRecord {
    DatasetExclusionRecord {
        ignition_event_id,
        h3,
        local_date,
        reason_category: reason.as_str().to_owned(),
        rule: rule.to_owned(),
        rule_version: Some("v1_pilot".to_owned()),
        details: json!({}),
        reintegration_possible: true,
    }
}

#[cfg(test)]
mod tests {
    use super::is_low_confidence_geographic_category;

    /// Regression test for the geographic-category bug found during the
    /// pilot build: the strict-mode filter must key off the categories
    /// `quality::assess_geography` actually assigns today
    /// (`municipality_centroid_probable`, `precision_undocumented`,
    /// `rounded_coordinate_probable`), not the aspirational, never-emitted
    /// `precise_reported`/`estimated_reported` categories from the phase
    /// 3A specification.
    #[test]
    fn strict_mode_rejects_only_the_genuinely_low_confidence_real_categories() {
        assert!(
            !is_low_confidence_geographic_category("precision_undocumented"),
            "precision_undocumented is the achievable ceiling of quality today \
             and must not be auto-rejected from strict mode"
        );
        assert!(is_low_confidence_geographic_category(
            "municipality_centroid_probable"
        ));
        assert!(is_low_confidence_geographic_category(
            "rounded_coordinate_probable"
        ));
    }

    #[test]
    fn strict_mode_selection_is_not_driven_by_fictional_categories() {
        assert!(!is_low_confidence_geographic_category("precise_reported"));
        assert!(!is_low_confidence_geographic_category("estimated_reported"));
    }
}
