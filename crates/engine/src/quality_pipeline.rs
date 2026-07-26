use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::Instant,
};

use anyhow::Context;
use grid::{cell_from_db, cell_to_db};
use quality::{
    COMBUSTIBILITY_RULE_ID, DUPLICATE_RULE_ID, GEOGRAPHIC_RULE_ID, GeographicAssessment,
    LABEL_RULE_ID, QualityEvent, TAXONOMY_RULE_ID, assess_duplicate, assess_geography,
    assess_label, cell_center_distance_m, coordinate_signals, duplicate_candidate,
    logical_checksum,
};
use serde::Serialize;
use serde_json::json;
use store::{
    CombustibilityAssessmentRecord, CombustibleCandidateRecord, CoordinateGroupRecord,
    DuplicateGroupRecord, DuplicateMemberRecord, DuplicatePairRecord, GeographicAssessmentRecord,
    LabelAssessmentRecord, QualityPersistenceBundle, QualityRuleVersion, QualitySourceEvent, Store,
};

use crate::config::Config;

const CODE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct QualityOptions {
    pub dry_run: bool,
    pub rules_version: String,
    pub year: Option<i32>,
    pub source_record_id: Option<String>,
    pub recalculate: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct QualitySummary {
    pub events_inspected: usize,
    pub labels_assessed: usize,
    pub geographic_assessments: usize,
    pub repeated_coordinate_events: usize,
    pub probable_centroids: usize,
    pub non_combustible_events: usize,
    pub missing_feature_events: usize,
    pub near_combustible_events: usize,
    pub human_non_combustible_events: usize,
    pub human_missing_feature_events: usize,
    pub human_difficult_events: usize,
    pub duplicate_candidate_pairs: usize,
    pub duplicate_groups: usize,
    pub errors: usize,
    pub elapsed_ms: u128,
    pub rules_version: String,
    pub checksum: String,
    pub dry_run: bool,
}

pub async fn audit_bdiff_quality(config: Config, options: QualityOptions) -> anyhow::Result<()> {
    anyhow::ensure!(
        options.rules_version == "v1",
        "unsupported rules version {}; only v1 is defined",
        options.rules_version
    );
    let started = Instant::now();
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize quality database")?;
    let source = store
        .quality_source_events(options.year, options.source_record_id.as_deref())
        .await
        .context("failed to load immutable BDIFF events")?;
    let (bundle, metrics) = compute_bundle(&store, &source).await?;
    let rules = rule_versions();
    if !options.dry_run {
        let mut rule_ids = HashMap::new();
        for rule in &rules {
            let id = store.ensure_quality_rule(rule).await?;
            rule_ids.insert(rule.logical_id.clone(), id);
        }
        store
            .persist_quality_bundle(&rule_ids, &bundle)
            .await
            .context("failed to persist quality audit atomically")?;
    }
    let summary_seed = json!({
        "events": source.iter().map(|event| &event.id).collect::<Vec<_>>(),
        "rules": rules.iter().map(|rule| (&rule.logical_id, &rule.checksum)).collect::<Vec<_>>(),
        "metrics": metrics,
        "year": options.year,
        "source_record_id": options.source_record_id,
        "recalculate": options.recalculate
    });
    let summary = QualitySummary {
        events_inspected: source.len(),
        labels_assessed: bundle.labels.len(),
        geographic_assessments: bundle.geography.len(),
        repeated_coordinate_events: metrics.repeated_coordinate_events,
        probable_centroids: metrics.probable_centroids,
        non_combustible_events: metrics.non_combustible_events,
        missing_feature_events: metrics.missing_feature_events,
        near_combustible_events: metrics.near_combustible_events,
        human_non_combustible_events: metrics.human_non_combustible_events,
        human_missing_feature_events: metrics.human_missing_feature_events,
        human_difficult_events: metrics.human_non_combustible_events
            + metrics.human_missing_feature_events,
        duplicate_candidate_pairs: bundle.duplicate_pairs.len(),
        duplicate_groups: bundle.duplicate_groups.len(),
        errors: 0,
        elapsed_ms: started.elapsed().as_millis(),
        rules_version: options.rules_version,
        checksum: logical_checksum(&summary_seed),
        dry_run: options.dry_run,
    };
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct ComputationMetrics {
    repeated_coordinate_events: usize,
    probable_centroids: usize,
    non_combustible_events: usize,
    missing_feature_events: usize,
    near_combustible_events: usize,
    human_non_combustible_events: usize,
    human_missing_feature_events: usize,
}

async fn compute_bundle(
    store: &Store,
    source: &[QualitySourceEvent],
) -> anyhow::Result<(QualityPersistenceBundle, ComputationMetrics)> {
    let events = source
        .iter()
        .map(QualityEvent::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    let coordinate_data = coordinate_records(source)?;
    let mut bundle = QualityPersistenceBundle {
        coordinates: coordinate_data.records,
        ..QualityPersistenceBundle::default()
    };
    let mut geography_by_event = HashMap::new();
    let mut metrics = ComputationMetrics::default();
    for (row, event) in source.iter().zip(&events) {
        let label = assess_label(event);
        bundle.labels.push(LabelAssessmentRecord {
            event_id: event.id.clone(),
            taxonomy_version: event.taxonomy_version.clone(),
            cause_category: event.cause_category.clone(),
            cause_subcategory: event.cause_subcategory.clone(),
            confidence: label.confidence.clone(),
            proposed_eligibility: label.proposed_eligibility.clone(),
            requires_accidental_sensitivity_analysis: label
                .requires_accidental_sensitivity_analysis,
            reasons: json!(label.reasons),
            logical_checksum: logical_checksum(&(event.id.as_str(), &label)),
        });
        let signals = coordinate_signals(
            usize::try_from(row.coordinate_event_count)?,
            usize::try_from(row.coordinate_municipality_count)?,
            usize::try_from(row.coordinate_year_count)?,
            event.latitude,
            event.longitude,
        );
        let geography = assess_geography(&signals, false);
        if signals.repeated_coordinate {
            metrics.repeated_coordinate_events += 1;
        }
        if signals.centroid_status == "probable_centroid" {
            metrics.probable_centroids += 1;
        }
        let coordinate_checksum = coordinate_data
            .checksums
            .get(&(event.latitude.to_bits(), event.longitude.to_bits()))
            .context("missing coordinate checksum")?
            .clone();
        bundle.geography.push(GeographicAssessmentRecord {
            event_id: event.id.clone(),
            coordinate_group_checksum: coordinate_checksum,
            latitude: event.latitude,
            longitude: event.longitude,
            h3: event.h3,
            h3_resolution: i16::from(event.h3_resolution),
            municipality: event.municipality.clone(),
            shared_event_count: row.coordinate_event_count,
            shared_municipality_count: row.coordinate_municipality_count,
            decimal_precision: i16::from(signals.decimal_precision),
            rounded_coordinate_probable: signals.rounded_coordinate_probable,
            centroid_status: signals.centroid_status.clone(),
            category: geography.category.clone(),
            confidence: geography.confidence,
            reasons: json!(geography.reasons),
            logical_checksum: logical_checksum(&(event.id.as_str(), &signals, &geography)),
        });
        geography_by_event.insert(event.id.clone(), geography);
    }
    let (combustibility, combustion_metrics) =
        combustibility_records(store, &events, &geography_by_event).await?;
    bundle.combustibility = combustibility;
    metrics.non_combustible_events = combustion_metrics.non_combustible_events;
    metrics.missing_feature_events = combustion_metrics.missing_feature_events;
    metrics.near_combustible_events = combustion_metrics.near_combustible_events;
    metrics.human_non_combustible_events = combustion_metrics.human_non_combustible_events;
    metrics.human_missing_feature_events = combustion_metrics.human_missing_feature_events;
    let (pairs, groups) = duplicate_records(&events, &geography_by_event);
    bundle.duplicate_pairs = pairs;
    bundle.duplicate_groups = groups;
    Ok((bundle, metrics))
}

struct CoordinateData {
    records: Vec<CoordinateGroupRecord>,
    checksums: HashMap<(u64, u64), String>,
}

fn coordinate_records(source: &[QualitySourceEvent]) -> anyhow::Result<CoordinateData> {
    let mut unique = BTreeMap::<(u64, u64), &QualitySourceEvent>::new();
    for event in source {
        unique
            .entry((event.latitude.to_bits(), event.longitude.to_bits()))
            .or_insert(event);
    }
    let mut records = Vec::with_capacity(unique.len());
    let mut checksums = HashMap::new();
    for (key, event) in unique {
        let signals = coordinate_signals(
            usize::try_from(event.coordinate_event_count)?,
            usize::try_from(event.coordinate_municipality_count)?,
            usize::try_from(event.coordinate_year_count)?,
            event.latitude,
            event.longitude,
        );
        let checksum = logical_checksum(&(
            event.latitude.to_bits(),
            event.longitude.to_bits(),
            &signals,
        ));
        records.push(CoordinateGroupRecord {
            latitude: event.latitude,
            longitude: event.longitude,
            event_count: event.coordinate_event_count,
            municipality_count: event.coordinate_municipality_count,
            year_count: event.coordinate_year_count,
            decimal_precision: i16::from(signals.decimal_precision),
            repeated_coordinate: signals.repeated_coordinate,
            rounded_coordinate_probable: signals.rounded_coordinate_probable,
            centroid_status: signals.centroid_status.clone(),
            signals: json!(signals),
            logical_checksum: checksum.clone(),
        });
        checksums.insert(key, checksum);
    }
    Ok(CoordinateData { records, checksums })
}

#[allow(clippy::too_many_lines)]
async fn combustibility_records(
    store: &Store,
    events: &[QualityEvent],
    geography: &HashMap<String, GeographicAssessment>,
) -> anyhow::Result<(Vec<CombustibilityAssessmentRecord>, ComputationMetrics)> {
    let mut requested = BTreeSet::new();
    for event in events {
        let cell = cell_from_db(event.h3)?;
        let neighbours: Vec<_> = cell.grid_disk(2);
        requested.extend(neighbours.into_iter().map(cell_to_db));
    }
    let features = store
        .quality_static_features(&requested.into_iter().collect::<Vec<_>>())
        .await?;
    let mut records = Vec::with_capacity(events.len());
    let mut metrics = ComputationMetrics::default();
    for event in events {
        let original = features.get(&event.h3);
        let combustible = original.and_then(|(value, _)| value["combustible"].as_bool());
        let cell = cell_from_db(event.h3)?;
        let neighbours: Vec<(grid::CellIndex, u32)> = cell.grid_disk_distances(2);
        let mut candidates = neighbours
            .into_iter()
            .filter(|(_, ring)| *ring > 0)
            .filter_map(|(candidate, ring)| {
                let h3 = cell_to_db(candidate);
                let (candidate_features, _) = features.get(&h3)?;
                candidate_features["combustible"]
                    .as_bool()
                    .filter(|value| *value)?;
                Some((
                    h3,
                    i16::try_from(ring).ok()?,
                    cell_center_distance_m(cell, candidate),
                    candidate_features.clone(),
                ))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| left.2.total_cmp(&right.2))
                .then_with(|| left.0.cmp(&right.0))
        });
        let ring1_count = candidates
            .iter()
            .filter(|candidate| candidate.1 <= 1)
            .count();
        let ring2_count = candidates.len();
        let candidate_rows = if combustible == Some(true) {
            Vec::new()
        } else {
            candidates
                .iter()
                .take(5)
                .enumerate()
                .map(
                    |(index, (h3, ring, distance, values))| CombustibleCandidateRecord {
                        h3: *h3,
                        ring: *ring,
                        rank: i16::try_from(index + 1).unwrap_or(i16::MAX),
                        center_distance_m: *distance,
                        features: values.clone(),
                        score: if *ring == 1 { 1.0 } else { 0.7 },
                        justification: json!({
                            "distance_method": "h3_center_geodesic",
                            "ring": ring,
                            "original_h3_preserved": true
                        }),
                    },
                )
                .collect::<Vec<_>>()
        };
        let nearest = (combustible != Some(true))
            .then(|| candidates.first())
            .flatten();
        let mut statuses = Vec::new();
        match combustible {
            Some(true) => statuses.push("combustible_original_cell"),
            Some(false) => {
                statuses.push("non_combustible_original_cell");
                metrics.non_combustible_events += 1;
                if event.cause_category == "human_known" {
                    metrics.human_non_combustible_events += 1;
                }
            }
            None => {
                statuses.push("missing_cell_features");
                metrics.missing_feature_events += 1;
                if event.cause_category == "human_known" {
                    metrics.human_missing_feature_events += 1;
                }
            }
        }
        if combustible != Some(true) && nearest.is_some() {
            statuses.push("near_combustible_cell");
            metrics.near_combustible_events += 1;
        }
        if geography
            .get(&event.id)
            .is_some_and(|value| value.category == "municipality_centroid_probable")
        {
            statuses.push("urban_centroid_suspected");
        }
        if combustible != Some(true) {
            statuses.push("requires_review");
        }
        let territorial = original.map_or_else(
            || json!({}),
            |(value, _)| {
                json!({
                    "road": value.get("road"),
                    "population": value.get("population"),
                    "poi": value.get("poi"),
                    "wui": value.get("wui"),
                    "agri": value.get("agri"),
                    "power_line": value.get("power_line")
                })
            },
        );
        let checksum_seed = json!({
            "event": event.id,
            "combustible": combustible,
            "nearest": nearest.map(|value| (value.0, value.1)),
            "statuses": statuses,
            "territorial": territorial
        });
        records.push(CombustibilityAssessmentRecord {
            event_id: event.id.clone(),
            h3: event.h3,
            h3_resolution: i16::from(event.h3_resolution),
            cell_features_present: original.is_some(),
            original_cell_combustible: combustible,
            feature_snapshot_at: original.map(|(_, updated_at)| *updated_at),
            nearest_combustible_h3: nearest.map(|value| value.0),
            nearest_combustible_ring: nearest.map(|value| value.1),
            nearest_combustible_distance_m: nearest.map(|value| value.2),
            combustible_ring1_count: i32::try_from(ring1_count)?,
            combustible_ring2_count: i32::try_from(ring2_count)?,
            status_flags: json!(statuses),
            territorial_signals: territorial,
            reasons: json!(["original_h3_preserved", "neighbours_are_proposals_only"]),
            logical_checksum: logical_checksum(&checksum_seed),
            candidates: candidate_rows,
        });
    }
    Ok((records, metrics))
}

fn duplicate_records(
    events: &[QualityEvent],
    geography: &HashMap<String, GeographicAssessment>,
) -> (Vec<DuplicatePairRecord>, Vec<DuplicateGroupRecord>) {
    let mut by_day = BTreeMap::new();
    for (index, event) in events.iter().enumerate() {
        by_day
            .entry(event.occurred_at.date_naive())
            .or_insert_with(Vec::new)
            .push(index);
    }
    let mut candidate_keys = BTreeSet::new();
    for indices in by_day.values() {
        for (offset, left) in indices.iter().enumerate() {
            for right in &indices[offset + 1..] {
                if duplicate_candidate(&events[*left], &events[*right]) {
                    candidate_keys.insert((*left, *right));
                }
            }
        }
    }
    let mut pairs = Vec::new();
    let mut groups = Vec::new();
    for (left_index, right_index) in candidate_keys {
        let left = &events[left_index];
        let right = &events[right_index];
        let assessment = assess_duplicate(left, right, &geography[&left.id], &geography[&right.id]);
        let (left_id, right_id) = if left.id < right.id {
            (&left.id, &right.id)
        } else {
            (&right.id, &left.id)
        };
        let pair_checksum = logical_checksum(&(left_id, right_id, &assessment));
        pairs.push(DuplicatePairRecord {
            left_event_id: left_id.clone(),
            right_event_id: right_id.clone(),
            score: assessment.score,
            classification: assessment.classification.clone(),
            raw_signals: assessment.raw_signals.clone(),
            contributions: json!(assessment.contributions),
            justification: assessment.justification.clone(),
            logical_checksum: pair_checksum.clone(),
        });
        if assessment.classification != "probably_distinct" {
            let stable_key = format!("{left_id}:{right_id}");
            let proposed_decision = if assessment.classification == "certain_duplicate" {
                "possible_single_representative"
            } else {
                "review"
            };
            let members = vec![
                DuplicateMemberRecord {
                    event_id: left_id.clone(),
                    role: "anchor".to_owned(),
                    individual_score: assessment.score,
                    pair_checksums: json!([pair_checksum]),
                    justification: "pairwise anchor; no transitive expansion".to_owned(),
                },
                DuplicateMemberRecord {
                    event_id: right_id.clone(),
                    role: "candidate".to_owned(),
                    individual_score: assessment.score,
                    pair_checksums: json!([pair_checksum]),
                    justification: "direct pairwise evidence only".to_owned(),
                },
            ];
            let group_checksum =
                logical_checksum(&(&stable_key, &assessment.classification, assessment.score));
            groups.push(DuplicateGroupRecord {
                stable_key,
                score: assessment.score,
                classification: assessment.classification,
                principal_signals: assessment.raw_signals,
                proposed_decision: proposed_decision.to_owned(),
                justification: "two-member direct-evidence group; weak A-B-C chains are not merged"
                    .to_owned(),
                logical_checksum: group_checksum,
                members,
            });
        }
    }
    (pairs, groups)
}

fn rule_versions() -> Vec<QualityRuleVersion> {
    [
        (
            TAXONOMY_RULE_ID,
            "taxonomy",
            "BDIFF source taxonomy preserved from phase 3B.1.",
            json!({"unknown_is_negative": false, "natural_is_absence": false}),
        ),
        (
            LABEL_RULE_ID,
            "label_quality",
            "Eligibility and confidence rules without silent relabelling.",
            json!({"accidental_sensitivity": true}),
        ),
        (
            GEOGRAPHIC_RULE_ID,
            "geographic_quality",
            "Repeated-coordinate and centroid-likelihood rules without coordinate correction.",
            json!({"probable_centroid_min_events": 5, "probable_centroid_min_years": 2}),
        ),
        (
            COMBUSTIBILITY_RULE_ID,
            "combustibility",
            "Original-cell assessment and proposed H3 neighbours through ring 2.",
            json!({"max_ring": 2, "distance_method": "h3_center_geodesic"}),
        ),
        (
            DUPLICATE_RULE_ID,
            "duplicate_detection",
            "Deterministic explainable pair scoring without automatic merging.",
            json!({
                "candidate_window_hours": 24,
                "certain": 0.92,
                "probable": 0.75,
                "possible": 0.55,
                "certain_requires_full_evidence_convergence": true,
                "certain_convergence_signals": [
                    "same_municipality", "same_h3", "same_cause",
                    "distance_m<=25", "time_minutes<=30", "surface_relative_difference<=0.05"
                ]
            }),
        ),
    ]
    .into_iter()
    .map(
        |(logical_id, rule_type, description, parameters)| QualityRuleVersion {
            logical_id: logical_id.to_owned(),
            rule_type: rule_type.to_owned(),
            description: description.to_owned(),
            checksum: logical_checksum(&(logical_id, rule_type, description, &parameters)),
            parameters,
            code_version: CODE_VERSION.to_owned(),
            status: "active".to_owned(),
            notes: Some("Phase 3B.2 deterministic foundation".to_owned()),
        },
    )
    .collect()
}
