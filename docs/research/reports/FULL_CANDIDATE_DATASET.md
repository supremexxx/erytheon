# ERYTHEON — Full candidate dataset (phase 3B.5)

Architecture and construction of the four phase 3B.5 candidate dataset
variants, built on the isolated DB for scientific review. **Not finalized,
not used for training.**

## 1. Scope and status

Four dataset versions, all `status = draft`:

| Logical ID | Variant | Negative strategy |
|---|---|---|
| `erytheon_human_ignition_cell_day_v1_candidate_strict_n2_kring2_day3` | strict | N2 |
| `erytheon_human_ignition_cell_day_v1_candidate_inclusive_n2_kring2_day3` | inclusive | N2 |
| `erytheon_human_ignition_cell_day_v1_candidate_strict_n3_adaptive_geographic_quality` | strict | N3 |
| `erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality` | inclusive | N3 |

None of these logical IDs are reused from the phase 3B.3 pilot
(`erytheon_human_ignition_cell_day_v1_pilot_*`); the pilot's rows,
versions, and builds are untouched by this phase.

Built by `engine::candidate_pipeline::build_candidate_datasets`, invoked
via `pyrorisk build-candidate-dataset --seed 2026071 --ratio 3`.

## 2. Observation unit and the resolution-9-to-8 aggregation

The dataset's unit is **one H3 resolution-8 cell x one Europe/Paris civil
date**, matching `fire.ignition_events`. `public.cell_static` — the source
of every feature value and of every negative candidate — is stored at H3
resolution 9. No code path compares or merges `CellIndex` values of
different resolutions directly (this exact mistake caused the phase 3B.4
audit's v1 measurement bug, see `PHASE3B4_NEGATIVE_SAMPLING_REPORT.md`).

**Aggregation rule** (`dataset::features_h3::aggregate_res9_children`,
tested):

- Group every resolution-9 `cell_static` row by its resolution-8 parent
  (`CellIndex::parent`).
- `combustible`: **any** resolution-9 child combustible = `true`. Chosen
  over requiring unanimity: a resolution-8 area can host a fire if any
  part of it can, and requiring all children combustible would shrink the
  eligible population for a reason unrelated to real risk.
- Each numeric feature (`wui`, `road`, `agri`, `population`, `poi`,
  `power_line`, `hist`): the **mean** of the children that actually carried
  a value for it. A child missing one feature does not poison the mean for
  children that have it.
- A resolution-8 cell with **zero** resolution-9 children is
  `missing_features` (`has_features() == false`) — never defaulted to
  combustible or to zero-valued features.

**Measured, not assumed**: `public.cell_static` turned out to hold 920,016
resolution-9 rows that aggregate to **794,651** distinct resolution-8
cells — not the ~131,000 a dense resolution-9 tiling of France would
imply. This means `cell_static` is a *sparse* resolution-9 sampling (on
average ~1.16 children per resolution-8 parent, not the ~7 an aperture-7
dense tiling would give), consistent with metropolitan France's area
(~551,695 km²) divided by a resolution-8 cell's average area (~0.737 km²)
≈ 748,568 — very close to the measured 794,651. This was verified directly
(decoding a real `cell_static` H3 index's resolution bits confirms
resolution 9), not assumed from either the recorded snapshot metadata or
the aggregation code alone.

Of the 794,651 aggregated resolution-8 cells, **761,556** are eligible
negative candidates (combustible and with features present).

## 3. Positives

Built once (shared logic, independent of negative strategy) from
`human_known` BDIFF events, grouped by `(h3, local_date)`, mirroring the
phase 3B.3 pilot's admissibility rules exactly (unchanged, already
reviewed): `certain_duplicate` candidates excluded per
`erytheon_duplicate_rules_v1`; strict additionally requires combustible +
features present + adequate geographic quality.

Measured (identical to the phase 3B.3 pilot's validated numbers — a
continuity check, not a coincidence):

| Quantity | Count |
|---|---|
| Admissible `human_known` events | 7,094 |
| Distinct positive cell-days (inclusive) | 6,849 |
| Strict positive cell-days | 2,566 |
| Shared positive-path exclusions | 4,286 |

Real `cell_static` feature values and the real historical calendar
(weekend/public-holiday/season) are wired in for every positive row —
previously hardcoded to `0.0`/`false` placeholders in the pilot.
`school_holiday` remains `None`: no verified source exists, unchanged.

## 4. Negatives

Sampled once per strategy (N2, N3), **shared across strict and inclusive**
of that strategy — negatives depend only on the exclusion window, not on
the positive-inclusion rule. Sized to the **inclusive** positive count per
split x ratio, so strict's realized negative:positive ratio is higher than
nominal (documented here, not hidden): strict has fewer positives but the
same negative count as inclusive.

**Sampling method** (`sample_negatives_for_split`): draws directly from the
full (eligible cell x split day-range) space without materializing it —
for pool index `i`, a cell and a day-offset are derived from
`mix64(seed, split, strategy, i)`, oversampling (starting at 5x `needed`,
doubling up to 40x if short) and filtering out any candidate that overlaps
a known event cell-date or falls within the strategy's exclusion window
(checked via `dataset::negative_design::is_within_window`, day-gap
pre-filtered to ≤5 days before any H3 distance computation). All four
splits reached their exact `needed` count at 5x oversampling — no split
required escalation.

| Split | Needed = inclusive positives x 3 | Sampled |
|---|---|---|
| train | 15,027 | 15,027 |
| calibration | 1,989 | 1,989 |
| test | 3,531 | 3,531 |
| prospective | 0 | 0 |

`prospective` (2026) has **zero** positives (measured directly), so zero
negatives are sampled for it — not a bug, an open policy question (see
§9).

## 5. Ratio: feasibility and choice

Feasibility was checked against the *measured* 761,556-cell eligible
population before committing to a ratio (mission-required step 1 before
step 2): at that scale, ratios 1:1 through 10:1 are all trivially
satisfiable for every split (train's 10:1 need, 50,090, is a negligible
fraction of 761,556 cells x up to 1,461 train days). **3:1 was chosen** —
the mission's own suggested default, not forced, and confirmed reasonable
by this feasibility check. Full per-ratio, per-split arithmetic is in
`PHASE3B4_NEGATIVE_SAMPLING_REPORT.md` §6 (computed there against the
phase 3B.4 experimental population; re-confirmed here against the real,
much larger 761,556-cell population).

## 6. Stratification

Positives are naturally stratified by their real event dates (year, month,
split). Negatives are stratified by construction: the sampler draws
uniformly across each split's own date range and across the full eligible
cell population (761,556 cells, sorted deterministically before indexing —
see §8), which spans every combustible area regardless of department.
Explicit month x H3-parent-block stratified quotas
(`dataset::negative_design::stratified_select`) were designed and tested in
phase 3B.4 but are **not** wired into this build's sampler; the simpler
uniform-within-split-and-population draw was used instead for this pass,
given the already-large eligible population makes gross imbalance unlikely
but not something this build directly measures per H3-parent-block.
Flagged as a limitation (§9), not silently assumed adequate.

No department/region variable is used: no versioned administrative
reference exists for arbitrary cells (phase 3B.2 finding, still true).

## 7. Splits

`train 2020-2023`, `calibration 2024`, `test 2025`, `prospective 2026`,
unchanged. Negatives sampled independently per split (§4); no split's
sampling reads another split's data (see `PHASE3B5_CANDIDATE_DATASET_REPORT.md`
§ temporal leakage for the executable checks).

## 8. Determinism and idempotence

The negative sampler indexes into `eligible_negative_cells`, a `Vec<i64>`
derived by filtering and mapping over a `HashMap` (`res8_map`). Rust's
`HashMap` iteration order is **randomized per process** (DoS protection),
not a deterministic property of its contents — so the derived `Vec`'s
*order* is not reproducible across runs unless sorted explicitly. This was
found as a real bug during this phase's own idempotence verification: an
unfixed first build, replayed, inserted ~2x the rows instead of reusing
identical ones. Fixed by sorting `eligible_negative_cells` before use
(`sort_unstable()`), and re-verified:

- Cleaned the corrupted state, rebuilt fresh, replayed once.
- `reused_existing_version: true` for all four variants; same
  `dataset_version_id`s and checksums both times; only a new `build_id`
  per replay.
- `SELECT count(*), count(DISTINCT deterministic_key) ...` equal for all
  four (23,113/23,113 strict, 27,396/27,396 inclusive) — confirmed via
  direct SQL and via `crates/store/tests/candidate_dataset_consistency.rs`.
- Two `ml.dataset_builds` rows per version (the fresh build plus the
  replay), zero row duplication.

## 9. Known limitations

- Negative stratification is uniform-within-split, not the tested
  month/H3-parent-block quota sampler (§6).
- `prospective` (2026) negatives exist with no corresponding positives yet
  — sampled anyway per the mission's instruction that 2026 "peut contenir
  uniquement des négatifs à ce stade", but whether this is useful before
  any 2026 positive exists is an open question.
- Strict/inclusive of the same strategy share an identical negative
  population sized to inclusive's count, so strict's realized ratio is
  higher than 3:1 (§4).
- The already-committed phase 3B.3 pilot rows retain their own
  `h3_resolution: 8` metadata paired with real resolution-9-sourced `h3`
  values for pilot negatives — a pre-existing inconsistency, not modified
  by this phase (see `NEGATIVE_SAMPLING_DESIGN.md` §3bis).
- Real feature values now flow through for both positives and negatives,
  but the underlying `cell_static` snapshot is still
  `current_snapshot_applied_historically` — today's static-feature state
  applied uniformly across 2020-2026, unchanged from phase 3B.3.

See `PHASE3B5_CANDIDATE_DATASET_REPORT.md` for full statistics, temporal
leakage checks, and the training-phase recommendation.
