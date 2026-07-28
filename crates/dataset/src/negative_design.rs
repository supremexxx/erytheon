//! Negative-sampling **design** primitives for phase 3B.4. Pure, testable
//! building blocks only — not wired into `build_human_dataset` and not a
//! final scientific strategy. See `NEGATIVE_SAMPLING_DESIGN.md` and
//! `PHASE3B4_NEGATIVE_SAMPLING_REPORT.md` for the comparison and
//! recommendation these functions were measured against.

use chrono::NaiveDate;
use grid::CellIndex;

use crate::checksums::logical_checksum;

/// A spatial/temporal exclusion window applied around one ignition event
/// when deciding whether a nearby cell-day may become a negative candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExclusionWindow {
    /// H3 grid (k-ring) distance, inclusive: 0 means only the exact cell.
    pub k_ring: u32,
    /// Day radius, inclusive: 0 means only the exact date.
    pub day_radius: i64,
}

/// The four exclusion-window strategies compared for phase 3B.4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExclusionStrategy {
    /// Exact cell-date only.
    N0,
    /// H3 k-ring 1, +/- 1 day.
    N1,
    /// H3 k-ring 2, +/- 3 days.
    N2,
    /// Window adapted to the causing event's geographic quality category.
    N3,
}

impl ExclusionStrategy {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::N0 => "n0_exact_cell_date",
            Self::N1 => "n1_kring1_day1",
            Self::N2 => "n2_kring2_day3",
            Self::N3 => "n3_adaptive_geographic_quality",
        }
    }

    /// The exclusion window for this strategy. `N3` requires the causing
    /// event's geographic quality category (`None` when unknown/undetermined,
    /// which is treated as the most cautious, widest window this strategy
    /// defines — an undetermined location must not produce a falsely narrow
    /// exclusion).
    #[must_use]
    pub fn window(self, geographic_quality_category: Option<&str>) -> ExclusionWindow {
        match self {
            Self::N0 => ExclusionWindow {
                k_ring: 0,
                day_radius: 0,
            },
            Self::N1 => ExclusionWindow {
                k_ring: 1,
                day_radius: 1,
            },
            Self::N2 => ExclusionWindow {
                k_ring: 2,
                day_radius: 3,
            },
            Self::N3 => match geographic_quality_category {
                // precision_undocumented is the achievable ceiling of
                // reported-coordinate quality today (see BDIFF_QUALITY.md);
                // it gets the narrowest adaptive window.
                Some("precision_undocumented") => ExclusionWindow {
                    k_ring: 1,
                    day_radius: 1,
                },
                Some("rounded_coordinate_probable") => ExclusionWindow {
                    k_ring: 3,
                    day_radius: 2,
                },
                // A municipality centroid can stand in for any cell in that
                // commune; widest spatial window of the three known
                // categories, since the true cell is unknown.
                Some("municipality_centroid_probable") => ExclusionWindow {
                    k_ring: 5,
                    day_radius: 2,
                },
                // Unknown/undetermined category: cautious default, at least
                // as wide as the widest named category above.
                _ => ExclusionWindow {
                    k_ring: 5,
                    day_radius: 3,
                },
            },
        }
    }
}

/// Whether `candidate` (h3, date) falls inside the exclusion window drawn
/// around one `event` (h3, date). Pure H3 grid-distance and calendar-day
/// arithmetic; never touches the database.
///
/// # Errors
///
/// Returns an error (the underlying H3 library's message) if the grid
/// distance cannot be computed (cells on incompatible base cells or
/// straddling a pentagon); callers should treat that as "cannot rule out
/// overlap" rather than "not excluded".
pub fn is_within_window(
    candidate_h3: CellIndex,
    candidate_date: NaiveDate,
    event_h3: CellIndex,
    event_date: NaiveDate,
    window: ExclusionWindow,
) -> Result<bool, String> {
    let day_gap = (candidate_date - event_date).num_days().abs();
    if day_gap > window.day_radius {
        return Ok(false);
    }
    if candidate_h3 == event_h3 {
        return Ok(true);
    }
    if window.k_ring == 0 {
        return Ok(false);
    }
    let distance = candidate_h3
        .grid_distance(event_h3)
        .map_err(|error| error.to_string())?;
    Ok(i64::from(distance) <= i64::from(window.k_ring))
}

/// The stratification key for one candidate: month and a coarser H3 "parent"
/// cell used as a spatial-block proxy in the absence of a versioned
/// administrative (department/region) reference (see phase 3B.2 audit: no
/// official commune/department mapping exists for arbitrary cells, only for
/// BDIFF event coordinates). `block_resolution` must be coarser
/// (numerically smaller) than the candidate's own resolution.
///
/// # Errors
///
/// Returns an error if `block_resolution` is not a valid coarser ancestor of
/// `cell` (see `h3o::CellIndex::parent`).
pub fn spatial_seasonal_stratum(
    cell: CellIndex,
    date: NaiveDate,
    block_resolution: grid::Resolution,
) -> Result<(u64, u32), &'static str> {
    let parent = cell
        .parent(block_resolution)
        .ok_or("block_resolution must be coarser than the cell's own resolution")?;
    Ok((u64::from(parent), date_month_key(date)))
}

fn date_month_key(date: NaiveDate) -> u32 {
    use chrono::Datelike;
    #[allow(clippy::cast_sign_loss)]
    let year = date.year() as u32;
    year * 12 + (date.month() - 1)
}

/// A stable identifier for one experimental negative candidate, distinct
/// across `dataset_version_logical_id` / `strategy` / `ratio` / `split`, so
/// comparing strategies never collides two candidates that only differ in
/// which strategy or ratio proposed them (mission section 16).
#[must_use]
pub fn deterministic_negative_key(
    dataset_version_logical_id: &str,
    strategy_id: &str,
    ratio: u32,
    split: &str,
    h3: i64,
    date: NaiveDate,
) -> String {
    logical_checksum(&(
        dataset_version_logical_id,
        strategy_id,
        ratio,
        split,
        h3,
        date.to_string(),
    ))
}

/// `splitmix64`-style deterministic mixing, identical construction to
/// `crate::negatives`'s pilot hash so the two stay auditable against each
/// other.
const fn mix64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    x
}

/// One stratified deterministic sampling round: distributes `target_total`
/// selections across strata in proportion to each stratum's share of
/// `positive_counts_by_stratum`, then fills each stratum's quota
/// deterministically from `candidates_by_stratum` using
/// `(seed, strategy_id, ratio, split)`. Strata with eligible candidates but
/// no matching positive stratum get no quota (undersampling silently is
/// preferred here to inventing a target); a stratum with positives but zero
/// eligible candidates simply contributes nothing — this is reported, not
/// hidden (mission section 17/19: "comportement 2026 sans positif").
///
/// # Panics
///
/// Never panics; a stratum key present only in one of the two maps is
/// treated as having zero of the missing side.
#[must_use]
pub fn stratified_select<K: Ord + Clone + std::hash::Hash + Eq>(
    candidates_by_stratum: &std::collections::HashMap<K, Vec<(i64, NaiveDate)>>,
    positive_counts_by_stratum: &std::collections::HashMap<K, usize>,
    seed: u64,
    strategy_id: &str,
    ratio: u32,
    split: &str,
) -> Vec<(K, i64, NaiveDate)> {
    let total_positives: usize = positive_counts_by_stratum.values().sum();
    if total_positives == 0 {
        return Vec::new();
    }
    let target_total = total_positives * ratio as usize;
    let mut selected = Vec::new();
    for (stratum, positive_count) in positive_counts_by_stratum {
        if *positive_count == 0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let quota = ((*positive_count as f64 / total_positives as f64) * target_total as f64)
            .round() as usize;
        let Some(candidates) = candidates_by_stratum.get(stratum) else {
            continue;
        };
        let mut scored: Vec<(u64, i64, NaiveDate)> = candidates
            .iter()
            .map(|(h3, date)| {
                let salt = logical_checksum(&(strategy_id, ratio, split, h3, date.to_string()));
                let hash = mix64(seed ^ u64::from_str_radix(&salt[..16], 16).unwrap_or(0));
                (hash, *h3, *date)
            })
            .collect();
        scored.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        });
        for (_, h3, date) in scored.into_iter().take(quota) {
            selected.push((stratum.clone(), h3, date));
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use grid::{LatLng, Resolution};
    use std::collections::HashMap;

    fn cell(lat: f64, lng: f64, resolution: u8) -> CellIndex {
        let resolution = Resolution::try_from(resolution).unwrap();
        LatLng::new(lat, lng).unwrap().to_cell(resolution)
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn n0_excludes_only_the_exact_cell_date() {
        let event = cell(45.0, 5.0, 8);
        let window = ExclusionStrategy::N0.window(None);
        assert!(is_within_window(event, d(2023, 6, 1), event, d(2023, 6, 1), window).unwrap());
        assert!(!is_within_window(event, d(2023, 6, 2), event, d(2023, 6, 1), window).unwrap());
        let neighbor = grid::H3Grid::new(8)
            .unwrap()
            .neighbors(event, 1)
            .into_iter()
            .find(|candidate| *candidate != event)
            .unwrap();
        assert!(!is_within_window(neighbor, d(2023, 6, 1), event, d(2023, 6, 1), window).unwrap());
    }

    #[test]
    fn n1_excludes_immediate_neighbors_within_one_day() {
        let event = cell(45.0, 5.0, 8);
        let neighbor = grid::H3Grid::new(8)
            .unwrap()
            .neighbors(event, 1)
            .into_iter()
            .find(|candidate| *candidate != event)
            .unwrap();
        let window = ExclusionStrategy::N1.window(None);
        assert!(is_within_window(neighbor, d(2023, 6, 2), event, d(2023, 6, 1), window).unwrap());
        assert!(!is_within_window(neighbor, d(2023, 6, 5), event, d(2023, 6, 1), window).unwrap());
    }

    #[test]
    fn n3_uses_the_widest_window_for_municipality_centroid() {
        let precise = ExclusionStrategy::N3.window(Some("precision_undocumented"));
        let centroid = ExclusionStrategy::N3.window(Some("municipality_centroid_probable"));
        assert!(centroid.k_ring > precise.k_ring);
    }

    #[test]
    fn n3_treats_unknown_category_at_least_as_wide_as_any_named_category() {
        let unknown = ExclusionStrategy::N3.window(None);
        for category in [
            "precision_undocumented",
            "rounded_coordinate_probable",
            "municipality_centroid_probable",
        ] {
            let named = ExclusionStrategy::N3.window(Some(category));
            assert!(unknown.k_ring >= named.k_ring);
            assert!(unknown.day_radius >= named.day_radius);
        }
    }

    #[test]
    fn deterministic_negative_key_distinguishes_strategy_and_ratio() {
        let base = deterministic_negative_key(
            "ds_v1",
            "n0_exact_cell_date",
            3,
            "train",
            42,
            d(2023, 6, 1),
        );
        let other_strategy =
            deterministic_negative_key("ds_v1", "n1_kring1_day1", 3, "train", 42, d(2023, 6, 1));
        let other_ratio = deterministic_negative_key(
            "ds_v1",
            "n0_exact_cell_date",
            5,
            "train",
            42,
            d(2023, 6, 1),
        );
        let other_split =
            deterministic_negative_key("ds_v1", "n0_exact_cell_date", 3, "test", 42, d(2023, 6, 1));
        assert_ne!(base, other_strategy);
        assert_ne!(base, other_ratio);
        assert_ne!(base, other_split);
        assert_eq!(
            base,
            deterministic_negative_key(
                "ds_v1",
                "n0_exact_cell_date",
                3,
                "train",
                42,
                d(2023, 6, 1)
            )
        );
    }

    #[test]
    fn stratified_select_is_deterministic_for_a_given_seed() {
        let mut candidates = HashMap::new();
        candidates.insert(
            1u32,
            vec![
                (10, d(2023, 6, 1)),
                (11, d(2023, 6, 2)),
                (12, d(2023, 6, 3)),
            ],
        );
        let mut positives = HashMap::new();
        positives.insert(1u32, 2usize);
        let a = stratified_select(&candidates, &positives, 7, "n0_exact_cell_date", 1, "train");
        let b = stratified_select(&candidates, &positives, 7, "n0_exact_cell_date", 1, "train");
        assert_eq!(a, b);
    }

    #[test]
    fn stratified_select_respects_the_requested_ratio() {
        let mut candidates = HashMap::new();
        candidates.insert(
            1u32,
            (0..20)
                .map(|i| {
                    (
                        i64::from(i),
                        d(2023, 6, 1) + chrono::Duration::days(i64::from(i)),
                    )
                })
                .collect(),
        );
        let mut positives = HashMap::new();
        positives.insert(1u32, 4usize);
        let selected =
            stratified_select(&candidates, &positives, 3, "n0_exact_cell_date", 3, "train");
        assert_eq!(selected.len(), 12, "4 positives * ratio 3 = 12 negatives");
    }

    #[test]
    fn stratified_select_returns_empty_when_no_positives_in_split() {
        // Mission section 17: "comportement 2026 sans positif" — a
        // prospective split with zero positives must yield zero negatives,
        // not a divide-by-zero panic or a fabricated quota.
        let mut candidates = HashMap::new();
        candidates.insert(1u32, vec![(10, d(2026, 1, 1))]);
        let positives: HashMap<u32, usize> = HashMap::new();
        let selected = stratified_select(
            &candidates,
            &positives,
            1,
            "n0_exact_cell_date",
            3,
            "prospective",
        );
        assert!(selected.is_empty());
    }

    #[test]
    fn stratified_select_never_exceeds_available_candidates_in_a_stratum() {
        let mut candidates = HashMap::new();
        candidates.insert(1u32, vec![(10, d(2023, 6, 1))]);
        let mut positives = HashMap::new();
        positives.insert(1u32, 10usize);
        let selected =
            stratified_select(&candidates, &positives, 1, "n0_exact_cell_date", 5, "train");
        assert_eq!(
            selected.len(),
            1,
            "cannot select more negatives than eligible candidates exist, even if the quota asks for more"
        );
    }

    #[test]
    fn stratified_select_never_selects_the_same_candidate_twice() {
        let mut candidates = HashMap::new();
        candidates.insert(
            1u32,
            (0..10)
                .map(|i| {
                    (
                        i64::from(i),
                        d(2023, 6, 1) + chrono::Duration::days(i64::from(i)),
                    )
                })
                .collect(),
        );
        let mut positives = HashMap::new();
        positives.insert(1u32, 3usize);
        let selected =
            stratified_select(&candidates, &positives, 5, "n0_exact_cell_date", 2, "train");
        let mut unique = selected.clone();
        unique.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
        unique.dedup_by(|a, b| a.1 == b.1 && a.2 == b.2);
        assert_eq!(
            unique.len(),
            selected.len(),
            "no (h3, date) candidate may be selected more than once within a split/strategy/ratio"
        );
    }

    #[test]
    fn spatial_seasonal_stratum_groups_by_coarser_parent_and_month() {
        let cell_a = cell(45.0, 5.0, 8);
        let block_res = Resolution::try_from(5).unwrap();
        let (parent_a, month_a) =
            spatial_seasonal_stratum(cell_a, d(2023, 6, 15), block_res).unwrap();
        let (parent_a2, month_a2) =
            spatial_seasonal_stratum(cell_a, d(2023, 6, 1), block_res).unwrap();
        assert_eq!(
            parent_a, parent_a2,
            "same cell must map to the same spatial block"
        );
        assert_eq!(month_a, month_a2, "same month must map to the same key");
        let (_, month_b) = spatial_seasonal_stratum(cell_a, d(2023, 7, 1), block_res).unwrap();
        assert_ne!(month_a, month_b);
    }

    #[test]
    fn deterministic_negative_key_never_uses_information_from_a_later_date_than_itself() {
        // The key is a pure function of its own inputs; changing an
        // unrelated later date must not change an earlier candidate's key.
        let earlier =
            deterministic_negative_key("ds_v1", "n0_exact_cell_date", 1, "train", 1, d(2020, 1, 1));
        let earlier_again =
            deterministic_negative_key("ds_v1", "n0_exact_cell_date", 1, "train", 1, d(2020, 1, 1));
        assert_eq!(earlier, earlier_again);
    }
}
