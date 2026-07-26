//! Deterministic and explainable quality rules for BDIFF ignition events.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use grid::{CellIndex, cell_from_db};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const LABEL_RULE_ID: &str = "erytheon_label_quality_v1";
pub const GEOGRAPHIC_RULE_ID: &str = "erytheon_geographic_quality_v1";
pub const DUPLICATE_RULE_ID: &str = "erytheon_duplicate_rules_v1";
pub const COMBUSTIBILITY_RULE_ID: &str = "erytheon_combustibility_assessment_v1";
pub const TAXONOMY_RULE_ID: &str = "erytheon_taxonomy_v1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QualityEvent {
    pub id: String,
    pub source_record_id: String,
    pub occurred_at: DateTime<Utc>,
    pub municipality: String,
    pub latitude: f64,
    pub longitude: f64,
    pub h3: i64,
    pub h3_resolution: u8,
    pub surface_ha: f64,
    pub cause_source: String,
    pub cause_category: String,
    pub cause_subcategory: String,
    pub taxonomy_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelAssessment {
    pub confidence: String,
    pub proposed_eligibility: String,
    pub requires_accidental_sensitivity_analysis: bool,
    pub reasons: Vec<String>,
}

#[must_use]
pub fn assess_label(event: &QualityEvent) -> LabelAssessment {
    match event.cause_category.as_str() {
        "human_known" => {
            let accidental = event.cause_subcategory == "accident_unspecified";
            LabelAssessment {
                confidence: if accidental { "medium" } else { "high" }.to_owned(),
                proposed_eligibility: "eligible_human_positive".to_owned(),
                requires_accidental_sensitivity_analysis: accidental,
                reasons: if accidental {
                    vec![
                        "known_human_taxonomy".to_owned(),
                        "accidental_category_requires_sensitivity_analysis".to_owned(),
                    ]
                } else {
                    vec!["known_human_taxonomy".to_owned()]
                },
            }
        }
        "natural_known" => LabelAssessment {
            confidence: "high".to_owned(),
            proposed_eligibility: "eligible_natural_cohort".to_owned(),
            requires_accidental_sensitivity_analysis: false,
            reasons: vec!["known_natural_taxonomy_not_absence_of_fire".to_owned()],
        },
        "unknown" => LabelAssessment {
            confidence: "unknown".to_owned(),
            proposed_eligibility: "unknown_cause_cohort".to_owned(),
            requires_accidental_sensitivity_analysis: false,
            reasons: vec!["unknown_cause_must_not_be_used_as_negative".to_owned()],
        },
        "indeterminate" => LabelAssessment {
            confidence: "low".to_owned(),
            proposed_eligibility: "indeterminate_cause_cohort".to_owned(),
            requires_accidental_sensitivity_analysis: false,
            reasons: vec!["unmapped_non_empty_source_label".to_owned()],
        },
        _ => LabelAssessment {
            confidence: "low".to_owned(),
            proposed_eligibility: "invalid_or_unusable".to_owned(),
            requires_accidental_sensitivity_analysis: false,
            reasons: vec!["unsupported_cause_category".to_owned()],
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinateSignals {
    pub event_count: usize,
    pub municipality_count: usize,
    pub year_count: usize,
    pub decimal_precision: u8,
    pub repeated_coordinate: bool,
    pub rounded_coordinate_probable: bool,
    pub centroid_status: String,
}

#[must_use]
pub fn decimal_precision(value: f64) -> u8 {
    let rendered = format!("{value:.15}");
    rendered
        .trim_end_matches('0')
        .split_once('.')
        .map_or(0, |(_, fraction)| {
            u8::try_from(fraction.len()).unwrap_or(15)
        })
}

#[must_use]
pub fn coordinate_signals(
    event_count: usize,
    municipality_count: usize,
    year_count: usize,
    latitude: f64,
    longitude: f64,
) -> CoordinateSignals {
    let precision = decimal_precision(latitude).min(decimal_precision(longitude));
    let rounded = precision <= 3;
    let centroid_status = if municipality_count == 1 && event_count >= 5 && year_count >= 2 {
        "probable_centroid"
    } else if municipality_count == 1 && event_count >= 3 {
        "possible_centroid"
    } else if event_count == 1 && !rounded {
        "not_centroid_like"
    } else {
        "undetermined"
    };
    CoordinateSignals {
        event_count,
        municipality_count,
        year_count,
        decimal_precision: precision,
        repeated_coordinate: event_count > 1,
        rounded_coordinate_probable: rounded,
        centroid_status: centroid_status.to_owned(),
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeographicAssessment {
    pub category: String,
    pub confidence: f64,
    pub reasons: Vec<String>,
}

#[must_use]
pub fn assess_geography(
    signals: &CoordinateSignals,
    official_centroid_match: bool,
) -> GeographicAssessment {
    if official_centroid_match {
        return GeographicAssessment {
            category: "municipality_centroid_confirmed".to_owned(),
            confidence: 1.0,
            reasons: vec!["official_versioned_centroid_match".to_owned()],
        };
    }
    if signals.centroid_status == "probable_centroid" {
        return GeographicAssessment {
            category: "municipality_centroid_probable".to_owned(),
            confidence: 0.75,
            reasons: vec![
                "same_coordinate_repeated_within_one_municipality".to_owned(),
                "multiple_years_without_spatial_variation".to_owned(),
                "no_official_centroid_reference_available".to_owned(),
            ],
        };
    }
    if signals.rounded_coordinate_probable {
        return GeographicAssessment {
            category: "rounded_coordinate_probable".to_owned(),
            confidence: 0.65,
            reasons: vec!["limited_decimal_precision".to_owned()],
        };
    }
    GeographicAssessment {
        category: "precision_undocumented".to_owned(),
        confidence: 0.5,
        reasons: vec!["source_precision_metadata_unavailable".to_owned()],
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DuplicateAssessment {
    pub score: f64,
    pub classification: String,
    pub raw_signals: Value,
    pub contributions: BTreeMap<String, f64>,
    pub justification: String,
}

#[must_use]
pub fn assess_duplicate(
    left: &QualityEvent,
    right: &QualityEvent,
    left_geography: &GeographicAssessment,
    right_geography: &GeographicAssessment,
) -> DuplicateAssessment {
    let time_minutes = (right.occurred_at - left.occurred_at)
        .num_minutes()
        .unsigned_abs();
    let distance_m = haversine_m(
        left.latitude,
        left.longitude,
        right.latitude,
        right.longitude,
    );
    let same_municipality = normalized(&left.municipality) == normalized(&right.municipality);
    let same_cause = left.cause_category == right.cause_category
        && left.cause_subcategory == right.cause_subcategory;
    let surface_relative_difference = relative_difference(left.surface_ha, right.surface_ha);
    let same_h3 = left.h3 == right.h3;
    let centroid_ambiguous = left_geography.category.contains("centroid")
        || right_geography.category.contains("centroid");

    let mut contributions = BTreeMap::new();
    contributions.insert(
        "same_municipality".to_owned(),
        if same_municipality { 0.15 } else { -0.15 },
    );
    contributions.insert("same_h3".to_owned(), if same_h3 { 0.10 } else { -0.10 });
    contributions.insert(
        "spatial_distance".to_owned(),
        if distance_m <= 25.0 {
            0.25
        } else if distance_m <= 150.0 {
            0.12
        } else {
            -0.15
        },
    );
    contributions.insert(
        "time_difference".to_owned(),
        if time_minutes <= 30 {
            0.25
        } else if time_minutes <= 180 {
            0.12
        } else if time_minutes >= 720 {
            -0.20
        } else {
            0.0
        },
    );
    contributions.insert(
        "same_cause".to_owned(),
        if same_cause { 0.15 } else { -0.08 },
    );
    contributions.insert(
        "surface_similarity".to_owned(),
        if surface_relative_difference <= 0.05 {
            0.10
        } else if surface_relative_difference >= 0.75 {
            -0.10
        } else {
            0.0
        },
    );
    contributions.insert(
        "centroid_ambiguity".to_owned(),
        if centroid_ambiguous { -0.10 } else { 0.0 },
    );
    let score = (0.20 + contributions.values().sum::<f64>()).clamp(0.0, 1.0);
    let classification = if score >= 0.92 && !centroid_ambiguous {
        "certain_duplicate"
    } else if score >= 0.75 && !centroid_ambiguous {
        "probable_duplicate"
    } else if score >= 0.55 {
        "possible_duplicate"
    } else if same_h3 && left.occurred_at.date_naive() == right.occurred_at.date_naive() {
        "indeterminate"
    } else {
        "probably_distinct"
    };
    DuplicateAssessment {
        score,
        classification: classification.to_owned(),
        raw_signals: json!({
            "time_difference_minutes": time_minutes,
            "distance_m": distance_m,
            "same_municipality": same_municipality,
            "same_h3": same_h3,
            "same_cause": same_cause,
            "surface_relative_difference": surface_relative_difference,
            "centroid_ambiguity": centroid_ambiguous
        }),
        contributions,
        justification: format!(
            "{classification}: deterministic weighted evidence; day/H3 alone is insufficient"
        ),
    }
}

#[must_use]
pub fn duplicate_candidate(left: &QualityEvent, right: &QualityEvent) -> bool {
    let same_day = left.occurred_at.date_naive() == right.occurred_at.date_naive();
    let nearby_time = (right.occurred_at - left.occurred_at)
        .num_hours()
        .unsigned_abs()
        <= 24;
    let same_municipality = normalized(&left.municipality) == normalized(&right.municipality);
    let nearby_h3 = h3_distance(left.h3, right.h3).is_some_and(|distance| distance <= 1);
    (same_day && nearby_h3) || (nearby_time && same_municipality)
}

#[must_use]
pub fn h3_distance(left: i64, right: i64) -> Option<u32> {
    let left = cell_from_db(left).ok()?;
    let right = cell_from_db(right).ok()?;
    left.grid_distance(right)
        .ok()
        .and_then(|distance| u32::try_from(distance).ok())
}

#[must_use]
pub fn cell_center_distance_m(left: CellIndex, right: CellIndex) -> f64 {
    let left = grid::LatLng::from(left);
    let right = grid::LatLng::from(right);
    haversine_m(left.lat(), left.lng(), right.lat(), right.lng())
}

#[must_use]
pub fn logical_checksum<T: Serialize>(value: &T) -> String {
    let serialized =
        serde_json::to_vec(value).unwrap_or_else(|error| error.to_string().into_bytes());
    format!("{:x}", Sha256::digest(serialized))
}

fn normalized(value: &str) -> String {
    value.trim().to_lowercase()
}

fn relative_difference(left: f64, right: f64) -> f64 {
    let denominator = left.abs().max(right.abs());
    if denominator <= f64::EPSILON {
        0.0
    } else {
        (left - right).abs() / denominator
    }
}

fn haversine_m(left_lat: f64, left_lon: f64, right_lat: f64, right_lon: f64) -> f64 {
    let radius_m = 6_371_008.8;
    let delta_lat = (right_lat - left_lat).to_radians();
    let delta_lon = (right_lon - left_lon).to_radians();
    let left_lat = left_lat.to_radians();
    let right_lat = right_lat.to_radians();
    let haversine = (delta_lat / 2.0).sin().powi(2)
        + left_lat.cos() * right_lat.cos() * (delta_lon / 2.0).sin().powi(2);
    2.0 * radius_m * haversine.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn event(id: &str, hour: u32, latitude: f64, surface: f64) -> QualityEvent {
        QualityEvent {
            id: id.to_owned(),
            source_record_id: id.to_owned(),
            occurred_at: Utc.with_ymd_and_hms(2025, 7, 1, hour, 0, 0).unwrap(),
            municipality: "Example".to_owned(),
            latitude,
            longitude: 2.0,
            h3: 0,
            h3_resolution: 8,
            surface_ha: surface,
            cause_source: "Malveillance".to_owned(),
            cause_category: "human_known".to_owned(),
            cause_subcategory: "malicious".to_owned(),
            taxonomy_version: "bdiff_taxonomy_v1".to_owned(),
        }
    }

    #[test]
    fn unknown_is_never_a_negative() {
        let mut value = event("unknown", 10, 43.0, 1.0);
        value.cause_category = "unknown".to_owned();
        value.cause_subcategory = "unknown_unspecified".to_owned();
        let assessment = assess_label(&value);
        assert_eq!(assessment.proposed_eligibility, "unknown_cause_cohort");
        assert_eq!(assessment.confidence, "unknown");
    }

    #[test]
    fn accidental_requires_sensitivity_analysis() {
        let mut value = event("accident", 10, 43.0, 1.0);
        value.cause_subcategory = "accident_unspecified".to_owned();
        let assessment = assess_label(&value);
        assert!(assessment.requires_accidental_sensitivity_analysis);
        assert_eq!(assessment.confidence, "medium");
    }

    #[test]
    fn natural_remains_a_fire_cohort() {
        let mut value = event("natural", 10, 43.0, 1.0);
        value.cause_category = "natural_known".to_owned();
        value.cause_subcategory = "natural_unspecified".to_owned();
        let assessment = assess_label(&value);
        assert_eq!(assessment.proposed_eligibility, "eligible_natural_cohort");
        assert!(
            assessment
                .reasons
                .contains(&"known_natural_taxonomy_not_absence_of_fire".to_owned())
        );
    }

    #[test]
    fn indeterminate_is_kept_for_review_cohort() {
        let mut value = event("indeterminate", 10, 43.0, 1.0);
        value.cause_category = "indeterminate".to_owned();
        value.cause_subcategory = "unmapped".to_owned();
        let assessment = assess_label(&value);
        assert_eq!(
            assessment.proposed_eligibility,
            "indeterminate_cause_cohort"
        );
    }

    #[test]
    fn centroid_is_never_confirmed_without_reference() {
        let signals = coordinate_signals(12, 1, 4, 43.12345, 2.12345);
        assert_eq!(signals.centroid_status, "probable_centroid");
        let assessment = assess_geography(&signals, false);
        assert_eq!(assessment.category, "municipality_centroid_probable");
    }

    #[test]
    fn rounded_coordinates_are_detected() {
        let signals = coordinate_signals(1, 1, 1, 43.12, 2.1);
        assert!(signals.rounded_coordinate_probable);
    }

    #[test]
    fn day_and_h3_alone_do_not_prove_duplicate() {
        let left = event("a", 1, 43.0, 1.0);
        let right = event("b", 20, 43.1, 20.0);
        let geography = GeographicAssessment {
            category: "precision_undocumented".to_owned(),
            confidence: 0.5,
            reasons: Vec::new(),
        };
        let result = assess_duplicate(&left, &right, &geography, &geography);
        assert_ne!(result.classification, "certain_duplicate");
        assert_ne!(result.classification, "probable_duplicate");
    }

    #[test]
    fn strong_matching_signals_produce_probable_duplicate() {
        let left = event("a", 10, 43.0, 1.0);
        let right = event("b", 10, 43.00001, 1.0);
        let geography = GeographicAssessment {
            category: "precision_undocumented".to_owned(),
            confidence: 0.5,
            reasons: Vec::new(),
        };
        let result = assess_duplicate(&left, &right, &geography, &geography);
        assert!(matches!(
            result.classification.as_str(),
            "probable_duplicate" | "certain_duplicate"
        ));
        assert!(result.contributions.contains_key("spatial_distance"));
    }

    #[test]
    fn checksum_is_deterministic() {
        let value = assess_label(&event("a", 10, 43.0, 1.0));
        assert_eq!(logical_checksum(&value), logical_checksum(&value));
    }

    #[test]
    fn h3_ring_distance_is_deterministic() {
        let grid = grid::H3Grid::new(8).expect("grid");
        let origin = grid.cell_for_point(43.0, 2.0).expect("origin");
        let neighbour = grid
            .neighbors_with_distance(origin, 1)
            .into_iter()
            .find_map(|(cell, distance)| (distance == 1).then_some(cell))
            .expect("neighbour");
        let origin = grid::cell_to_db(origin);
        let neighbour = grid::cell_to_db(neighbour);
        assert_eq!(h3_distance(origin, neighbour), Some(1));
        assert_eq!(
            h3_distance(origin, neighbour),
            h3_distance(origin, neighbour)
        );
    }

    #[test]
    fn ambiguous_chain_does_not_imply_endpoint_duplicate() {
        let geography = GeographicAssessment {
            category: "precision_undocumented".to_owned(),
            confidence: 0.5,
            reasons: Vec::new(),
        };
        let first = event("a", 10, 43.0, 1.0);
        let middle = event("b", 10, 43.00001, 1.0);
        let last = event("c", 22, 43.1, 20.0);
        let first_middle = assess_duplicate(&first, &middle, &geography, &geography);
        let first_last = assess_duplicate(&first, &last, &geography, &geography);
        assert!(first_middle.score > first_last.score);
        assert!(matches!(
            first_last.classification.as_str(),
            "indeterminate" | "probably_distinct"
        ));
    }
}
