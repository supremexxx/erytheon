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
/// `candidate_h3` and `event_h3` are normalized to a common resolution
/// before any spatial comparison (via [`CellIndex::parent`], coarsening the
/// finer of the two down to the coarser one's resolution). This matters in
/// this codebase specifically: `public.cell_static` (the source of negative
/// candidates, via `sample_combustible_cells`) is stored at H3 resolution 9
/// today, while `fire.ignition_events` (the exclusion event set) is at
/// resolution 8 — a real, pre-existing mismatch discovered during the
/// phase 3B.4 audit (see `PHASE3B4_NEGATIVE_SAMPLING_REPORT.md`). Comparing
/// two `CellIndex` values of different resolutions directly — by equality
/// or via `grid_distance` — is either always false or always an error, so
/// skipping normalization here would silently make every strategy appear
/// to exclude almost nothing, which is exactly what the first (unfixed)
/// version of this experiment measured.
///
/// # Errors
///
/// Returns an error (the underlying H3 library's message) if the grid
/// distance or the resolution normalization cannot be computed (cells on
/// incompatible base cells, straddling a pentagon, or the finer resolution
/// somehow not a descendant of the coarser one); callers should treat that
/// as "cannot rule out overlap" rather than "not excluded".
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
    let (candidate_h3, event_h3) = normalize_to_common_resolution(candidate_h3, event_h3)?;
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

/// Coarsens whichever of `a`/`b` has the finer H3 resolution down to the
/// other's resolution, so the pair can be compared by equality or
/// `grid_distance`. A no-op when both are already the same resolution.
fn normalize_to_common_resolution(
    a: CellIndex,
    b: CellIndex,
) -> Result<(CellIndex, CellIndex), String> {
    match a.resolution().cmp(&b.resolution()) {
        std::cmp::Ordering::Equal => Ok((a, b)),
        std::cmp::Ordering::Greater => {
            let coarsened = a
                .parent(b.resolution())
                .ok_or("cannot coarsen candidate cell to the event's resolution")?;
            Ok((coarsened, b))
        }
        std::cmp::Ordering::Less => {
            let coarsened = b
                .parent(a.resolution())
                .ok_or("cannot coarsen event cell to the candidate's resolution")?;
            Ok((a, coarsened))
        }
    }
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
// This is a design/experimental helper called with the standard hasher
// from a small number of known call sites (not a hasher-agnostic public
// API), so generalizing over BuildHasher would add generic-parameter
// noise without a real caller that needs it.
#[allow(clippy::implicit_hasher)]
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
        // quota is a rounded share of a non-negative target_total, so it is
        // always >= 0 and fits usize; the f64 arithmetic only loses
        // precision far beyond any realistic candidate count.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation
        )]
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

    /// Regression test for the phase 3B.4 audit finding: `cell_static`
    /// (candidates) is resolution 9, `fire.ignition_events` (events) is
    /// resolution 8. A candidate that is genuinely the resolution-9 child
    /// of an event's resolution-8 cell must still be recognized as the
    /// exact same location under N0, not silently treated as unrelated.
    #[test]
    fn cross_resolution_candidate_on_the_same_location_as_the_event_is_recognized() {
        let event_res8 = cell(45.0, 5.0, 8);
        let candidate_res9 = cell(45.0, 5.0, 9);
        assert_eq!(
            candidate_res9.parent(Resolution::try_from(8).unwrap()),
            Some(event_res8),
            "test setup: the resolution-9 point must actually be a child of the resolution-8 cell"
        );
        let window = ExclusionStrategy::N0.window(None);
        assert!(
            is_within_window(
                candidate_res9,
                d(2023, 6, 1),
                event_res8,
                d(2023, 6, 1),
                window
            )
            .unwrap(),
            "a resolution-9 candidate over the same location as a resolution-8 event must be excluded under N0"
        );
    }

    #[test]
    fn cross_resolution_candidate_outside_the_window_is_still_not_excluded() {
        let event_res8 = cell(45.0, 5.0, 8);
        let far_candidate_res9 = cell(46.0, 6.0, 9);
        let window = ExclusionStrategy::N2.window(None);
        assert!(
            !is_within_window(
                far_candidate_res9,
                d(2023, 6, 1),
                event_res8,
                d(2023, 6, 1),
                window
            )
            .unwrap(),
            "normalizing resolutions must not turn an unrelated, distant cell into a false exclusion"
        );
    }

    #[test]
    fn n2_excludes_a_kring2_neighbor_that_kring1_would_miss() {
        let event = cell(45.0, 5.0, 8);
        let grid = grid::H3Grid::new(8).unwrap();
        let kring1: std::collections::HashSet<_> = grid.neighbors(event, 1).into_iter().collect();
        let kring2_only = grid
            .neighbors(event, 2)
            .into_iter()
            .find(|candidate| !kring1.contains(candidate))
            .expect("a k-ring-2-only neighbor must exist");
        let n1_window = ExclusionStrategy::N1.window(None);
        let n2_window = ExclusionStrategy::N2.window(None);
        assert!(
            !is_within_window(kring2_only, d(2023, 6, 1), event, d(2023, 6, 1), n1_window).unwrap(),
            "N1 (k-ring 1) must not exclude a cell that is only within k-ring 2"
        );
        assert!(
            is_within_window(kring2_only, d(2023, 6, 1), event, d(2023, 6, 1), n2_window).unwrap(),
            "N2 (k-ring 2) must exclude a cell that is within k-ring 2"
        );
    }

    #[test]
    fn window_respects_the_day_radius_boundary_across_a_month_and_year_change() {
        let event = cell(45.0, 5.0, 8);
        let window = ExclusionStrategy::N2.window(None); // +/- 3 days
        // 2023-12-30 is 2 days before 2024-01-01: within the +/-3 day radius,
        // crossing both a month and a year boundary.
        assert!(is_within_window(event, d(2023, 12, 30), event, d(2024, 1, 1), window).unwrap());
        // 2023-12-27 is 5 days before 2024-01-01: outside the +/-3 day radius.
        assert!(!is_within_window(event, d(2023, 12, 27), event, d(2024, 1, 1), window).unwrap());
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
