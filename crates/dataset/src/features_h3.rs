//! Resolution-9 -> resolution-8 feature aggregation for phase 3B.5.
//!
//! `public.cell_static` is stored at H3 resolution 9 (920,016 rows); the
//! dataset's canonical observation unit is H3 resolution 8 (matching
//! `fire.ignition_events`). This module defines, once, the explicit,
//! tested rule for turning several resolution-9 feature rows into one
//! resolution-8 row — no other code path may compare or merge H3 indexes
//! of different resolutions directly (see `dataset::negative_design`'s
//! own resolution-normalization note for the companion spatial-window
//! side of this same underlying mismatch).

use std::collections::HashMap;

use grid::CellIndex;
use serde::{Deserialize, Serialize};

/// One resolution-9 `cell_static` row's numeric features, as stored.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Res9Features {
    pub poi: Option<f64>,
    pub wui: Option<f64>,
    pub agri: Option<f64>,
    pub hist: Option<f64>,
    pub road: Option<f64>,
    pub population: Option<f64>,
    pub power_line: Option<f64>,
    pub combustible: Option<bool>,
}

/// One resolution-8 cell's aggregated features, plus the provenance needed
/// to judge whether the aggregation is trustworthy.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Res8AggregatedFeatures {
    pub poi: Option<f64>,
    pub wui: Option<f64>,
    pub agri: Option<f64>,
    pub hist: Option<f64>,
    pub road: Option<f64>,
    pub population: Option<f64>,
    pub power_line: Option<f64>,
    /// True if **any** resolution-9 child is combustible. Deliberately
    /// permissive rather than requiring all children combustible: a
    /// resolution-8 cell area is capable of hosting a fire if any part of
    /// it is, and requiring unanimity would improperly shrink the
    /// eligible negative-candidate population for a reason unrelated to
    /// real risk. Documented here, not left implicit.
    pub combustible: bool,
    /// How many resolution-9 rows were found under this resolution-8
    /// parent. Zero means the parent has no `cell_static` coverage at
    /// all (should not happen for the studied territory; if it does, the
    /// cell must be treated as `missing_features`, never silently
    /// defaulted).
    pub children_present: u32,
}

impl Res8AggregatedFeatures {
    /// Whether this aggregated cell has enough information to be used at
    /// all: at least one resolution-9 child was found. A cell with zero
    /// children is `missing_features`, never combustible-by-default.
    #[must_use]
    pub const fn has_features(&self) -> bool {
        self.children_present > 0
    }
}

fn mean(values: &[Option<f64>]) -> Option<f64> {
    let present: Vec<f64> = values.iter().filter_map(|value| *value).collect();
    if present.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let count = present.len() as f64;
    Some(present.iter().sum::<f64>() / count)
}

/// Aggregates every resolution-9 child under one resolution-8 parent into
/// one resolution-8 feature row. `combustible` is `any(...)`; every
/// numeric feature is the mean of the children that actually carried a
/// value for it (missing values in some children do not poison the mean
/// for children that do have a value) — see the module doc for why `any`
/// was chosen for `combustible` specifically.
#[must_use]
pub fn aggregate_res9_children(children: &[Res9Features]) -> Res8AggregatedFeatures {
    Res8AggregatedFeatures {
        poi: mean(&children.iter().map(|c| c.poi).collect::<Vec<_>>()),
        wui: mean(&children.iter().map(|c| c.wui).collect::<Vec<_>>()),
        agri: mean(&children.iter().map(|c| c.agri).collect::<Vec<_>>()),
        hist: mean(&children.iter().map(|c| c.hist).collect::<Vec<_>>()),
        road: mean(&children.iter().map(|c| c.road).collect::<Vec<_>>()),
        population: mean(&children.iter().map(|c| c.population).collect::<Vec<_>>()),
        power_line: mean(&children.iter().map(|c| c.power_line).collect::<Vec<_>>()),
        combustible: children.iter().any(|child| child.combustible == Some(true)),
        children_present: u32::try_from(children.len()).unwrap_or(u32::MAX),
    }
}

/// Groups resolution-9 `(h3, features)` pairs by their resolution-8 parent
/// and aggregates each group. Rows whose resolution-9 cell has no valid
/// resolution-8 parent (should never happen for real H3 indexes) are
/// dropped, not silently merged into an arbitrary bucket.
#[must_use]
pub fn aggregate_all_to_res8(
    rows: &[(CellIndex, Res9Features)],
    res8: grid::Resolution,
) -> HashMap<CellIndex, Res8AggregatedFeatures> {
    let mut grouped: HashMap<CellIndex, Vec<Res9Features>> = HashMap::new();
    for (cell, features) in rows {
        if let Some(parent) = cell.parent(res8) {
            grouped.entry(parent).or_default().push(*features);
        }
    }
    grouped
        .into_iter()
        .map(|(parent, children)| (parent, aggregate_res9_children(&children)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use grid::{LatLng, Resolution};

    fn res9_at(lat: f64, lng: f64) -> CellIndex {
        LatLng::new(lat, lng)
            .unwrap()
            .to_cell(Resolution::try_from(9).unwrap())
    }

    #[test]
    fn combustible_is_true_if_any_child_is_combustible() {
        let children = vec![
            Res9Features {
                combustible: Some(false),
                ..Default::default()
            },
            Res9Features {
                combustible: Some(true),
                ..Default::default()
            },
        ];
        assert!(aggregate_res9_children(&children).combustible);
    }

    #[test]
    fn combustible_is_false_when_no_child_is_combustible() {
        let children = vec![
            Res9Features {
                combustible: Some(false),
                ..Default::default()
            },
            Res9Features {
                combustible: None,
                ..Default::default()
            },
        ];
        assert!(!aggregate_res9_children(&children).combustible);
    }

    #[test]
    fn numeric_mean_ignores_missing_children_but_not_present_ones() {
        let children = vec![
            Res9Features {
                wui: Some(2.0),
                ..Default::default()
            },
            Res9Features {
                wui: None,
                ..Default::default()
            },
            Res9Features {
                wui: Some(4.0),
                ..Default::default()
            },
        ];
        assert_eq!(aggregate_res9_children(&children).wui, Some(3.0));
    }

    #[test]
    fn numeric_feature_is_none_when_every_child_is_missing_it() {
        let children = vec![Res9Features::default(), Res9Features::default()];
        assert_eq!(aggregate_res9_children(&children).poi, None);
    }

    #[test]
    fn zero_children_means_missing_features_not_default_combustible() {
        let aggregated = aggregate_res9_children(&[]);
        assert_eq!(aggregated.children_present, 0);
        assert!(!aggregated.has_features());
        assert!(
            !aggregated.combustible,
            "an h3 cell with zero cell_static coverage must never default to combustible"
        );
    }

    #[test]
    fn grouping_assigns_each_res9_child_to_its_real_res8_parent() {
        let res8 = Resolution::try_from(8).unwrap();
        let a = res9_at(45.0, 5.0);
        let b = res9_at(45.0001, 5.0001); // extremely close: likely same res8 parent
        let parent_a = a.parent(res8).unwrap();
        let rows = vec![
            (
                a,
                Res9Features {
                    wui: Some(1.0),
                    combustible: Some(true),
                    ..Default::default()
                },
            ),
            (
                b,
                Res9Features {
                    wui: Some(3.0),
                    combustible: Some(false),
                    ..Default::default()
                },
            ),
        ];
        let aggregated = aggregate_all_to_res8(&rows, res8);
        // Whether a and b share a parent depends on real H3 geometry;
        // either way every row must be attributed to *some* real parent,
        // and the total number of aggregated children across all buckets
        // must equal the input row count exactly (no row dropped or
        // double-counted).
        let total_children: u32 = aggregated.values().map(|v| v.children_present).sum();
        assert_eq!(total_children, 2);
        assert!(aggregated.contains_key(&parent_a));
    }
}
