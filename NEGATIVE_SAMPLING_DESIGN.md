# ERYTHEON — Negative sampling design (phase 3B.4)

Design document for the scientific negative-sampling strategy for
`erytheon_human_ignition_cell_day_v1`. This is a **design and isolated
experiment**, not an implementation of a final strategy and not a model
input. See `PHASE3B4_NEGATIVE_SAMPLING_REPORT.md` for the measured
comparison and the v1 recommendation drawn from this design.

## 1. The scientific problem

The dataset's observation unit is one H3 resolution-8 cell on one civil
(Europe/Paris) date. The label is:

```
human_ignition = 1  iff at least one admissible human_known event
                     exists in that cell-date
```

A row with `label = 0` means *no ignition was recorded* in that cell-date —
not that risk was actually absent. Under-reporting, detection gaps, and the
`insufficient_geographic_quality`/`missing_features` exclusions already
documented in phase 3B.3 mean a naive negative (any non-positive cell-day)
can silently include cell-days that were actually at real risk, or can be
trivially easy (e.g., a cell that is never combustible) in a way that
teaches the model nothing. The design goal is a negative population that is
neither artificially easy nor contaminated by near-misses of an actual
event.

## 2. Eligible negative population

A candidate cell-day is eligible only if **all** of the following hold:

1. Its H3 cell falls in the studied territory (implicitly satisfied today:
   candidates are drawn from `public.cell_static`, which only covers the
   studied territory).
2. It has the features required by the dataset (`cell_features_present`
   equivalent — same check as the positive path's `missing_features`
   exclusion).
3. It is combustible (`combustible = true`, not `false` or unknown).
4. Its date falls in a covered period (2020–2026, same as positives).
5. It is covered by an authorized feature snapshot (`cell_static_bundle`,
   `active`) and by the historical calendar (2020–2026 range).
6. It carries no human, natural, or unknown-cause ignition event on that
   exact cell-date.
7. It uses no future information relative to its own split (a train-split
   candidate's eligibility must not depend on 2025/2026 data).

**Additional exclusions evaluated, decision per criterion:**

- **Neighboring cells of an event / neighboring days of an event**: yes —
  this is exactly what the exclusion-window strategies (§4) formalize; a
  fixed, unconditional "exclude neighbors" rule is too blunt on its own,
  which is why four window strategies are compared rather than one.
- **Cells with FIRMS but no BDIFF**: evaluated in §6 (FIRMS). Not adopted as
  a hard exclusion in v1 — see rationale there.
- **Days with incomplete BDIFF data**: no per-day BDIFF completeness signal
  exists in the schema today; not evaluated further (documented as a missing
  dependency, §9).
- **Zones without homogeneous coverage**: no versioned administrative
  reference exists to define "zone" precisely (see phase 3B.2's data-gap
  finding: no official commune/department mapping for arbitrary cells).
  Deferred; see §7 spatial stratification.
- **Cells with insufficient territorial quality**: `cell_static`'s own
  `combustible`/feature-presence checks are the only quality signal
  available for arbitrary (non-event) cells; no separate territorial-
  quality score exists for cells that never had an event, so nothing further
  can be checked today.

## 3. Unknown and natural causes are never negatives

`unknown` (8,071 events) and `natural_known` (791 events) causes are never
treated as ground truth for the absence of human ignition, and never appear
as `label = 0` rows. A cell-day carrying an `unknown`-cause or `natural_
known` event must be excluded from the negative candidate pool exactly like
a `human_known` one — the exclusion mechanism (`is_within_window`) is
cause-agnostic by construction (it only takes `h3`/`date`), and the
verifying experiment in `crates/store/tests/negative_sampling_experiment.rs`
asserts the event set it draws from actually contains both `natural_known`
and `unknown` rows before running any comparison, so this isn't asserted by
convention alone.

`unknown` causes carry no geographic-quality difference in kind from
`human_known` — 8,071 of the 8,071 unknown events have a geographic-quality
assessment, same as human and natural causes (verified directly:
`event_geographic_quality` covers all 15,956 events, 0 without an
assessment). Their impact on the negative population is therefore governed
by the same exclusion-window strategy as human events, using their own
geographic-quality category — no special-casing needed or added.

## 3bis. Audit finding: the resolution mismatch bug (v1 → v2)

The first measurement pass (v1: 300 combustible cells × one date/year,
2,100 candidates) reported near-zero exclusions (N0=0, N1=0, N2=1, N3=3)
and was initially — wrongly — explained away as "the candidate population
is just sparse." A dedicated audit of that experiment (mission section C)
found the real, structural cause instead:

**`public.cell_static` (the source of every negative candidate, via
`sample_combustible_cells`) is stored at H3 resolution 9. `fire.
ignition_events` (the exclusion event set) is at resolution 8.** Verified
directly:

```sql
SELECT h3_resolution, count(*) FROM fire.ignition_events GROUP BY 1;
-- 8 | 15956
SELECT logical_checksum, h3_resolution, cell_count FROM features.feature_snapshots;
-- ... | 9 | 920016
```

Comparing two `h3o::CellIndex` values of different resolutions is either
always unequal (`==`) or always an error (`grid_distance`, which requires
matching resolutions and returns `LocalIjError::ResolutionMismatch`
otherwise). `is_within_window`'s original implementation checked equality
directly and treated a `grid_distance` error as "fail closed" (excluded) —
but for `N0` (`k_ring == 0`), the function never even reaches
`grid_distance`, so a resolution-9 candidate could **never** register as
"at" a resolution-8 event no matter how close the true locations were.
This is exactly why v1 measured N0 = 0.

This is a **real, pre-existing data inconsistency** (not introduced by this
phase): `config.rs`'s `DEFAULT_H3_RESOLUTION` is `"9"` today, and
`cell_static` was generated under that config, while the BDIFF events in
this isolated DB were ingested under an earlier resolution-8 configuration.
It also affects the existing, already-committed phase 3B.3 pilot: `build_
row` in `engine::dataset_pipeline` hardcodes `h3_resolution: 8` on every
row, including pilot-negative rows whose `h3` value actually comes from
`cell_static` at resolution 9 — the stored metadata and the stored value
disagree. This is flagged here as an open risk (§9) for a future, separately
authorized fix; it is **not** modified in this phase (no dataset rebuild is
authorized here).

**Fix applied in this phase**, scoped to the new design code only:
`is_within_window` now normalizes both cells to a common resolution first
(coarsening the finer one via `CellIndex::parent`) before any equality or
`grid_distance` check — see `normalize_to_common_resolution` in `crates/
dataset/src/negative_design.rs`, covered by two new regression tests
(`cross_resolution_candidate_on_the_same_location_as_the_event_is_
recognized`, `cross_resolution_candidate_outside_the_window_is_still_not_
excluded`).

## 4. Exclusion window strategies (N0–N3)

| Strategy | Spatial (H3 k-ring) | Temporal (days) | Rationale |
|---|---|---|---|
| **N0** | 0 (exact cell only) | 0 (exact date only) | Baseline; no spatial/temporal buffer at all — closest to today's pilot mechanism. |
| **N1** | 1 | ±1 | Small, fixed buffer against immediate neighbors. |
| **N2** | 2 | ±3 | Wider fixed buffer; more conservative against under-reporting/detection lag. |
| **N3** | Adaptive: 1 (precision_undocumented) / 3 (rounded_coordinate_probable) / 5 (municipality_centroid_probable) / 5 (unknown/undetermined, cautious default) | Adaptive: 1 / 2 / 2 / 3 respectively | A precisely reported event needs only a small buffer; a municipality-centroid event could really be anywhere in that commune, so its true location's neighborhood must be treated as wide. |

Implemented as pure functions in `crates/dataset/src/negative_design.rs`
(`ExclusionStrategy::window`, `is_within_window`), unit-tested for: exact-
cell-date exclusion (N0), immediate-neighbor exclusion within the temporal
radius (N1), and that `N3`'s window widens correctly by category and that
an unknown/undetermined category never gets a narrower window than any
named category (never accidentally under-excludes just because the
location is uncertain).

Measured candidate counts, coverage, and exclusion rate for all four
strategies against a real (not fabricated) experimental candidate
population are in `PHASE3B4_NEGATIVE_SAMPLING_REPORT.md`.

## 5. Sampling approach

| Approach | Description | Verdict for v1 |
|---|---|---|
| Uniform global | Uniform draw from all eligible candidates. | Rejected as sole method — risks concentrating negatives in low-exposure regions/seasons, teaching the model "this region is just safe" rather than "this cell-day had no observed ignition." |
| Stratified by month | Match positives' monthly distribution. | Partial adoption — folded into the spatial+seasonal stratum below. |
| Stratified by department/region | Avoid pure-geography learning. | Not implementable as specified — no versioned department/region reference exists for arbitrary cells (only for BDIFF event coordinates, via `derived_department_code` in `validation.event_geographic_quality`). H3 parent-cell (coarser resolution) is used as the practical proxy instead (see `spatial_seasonal_stratum` in `negative_design.rs`). |
| **Stratified spatial + seasonal** | Match positives' (H3-parent-block, month) joint distribution. | **Adopted for v1** — implemented (`stratified_select`), deterministic, testable, and the only approach that doesn't require a missing administrative reference. |
| Matched case-control | Per-positive nearby-but-not-too-near negatives. | Not adopted for v1 — this is a variant of N1/N2/N3 windowing applied per-positive rather than pool-wide; worth comparing once a full-coverage candidate population (not one date/year) exists, flagged as future work. |
| Hard negatives | High-operational-risk cell-days with no recorded ignition. | Explicitly rejected for v1 — the mission's own caution applies directly: with real under-reporting/detection gaps in this data (documented since phase 3B.1/3B.2), a "hard negative" selection risks amplifying exactly the label noise the strategy is trying to avoid. Revisit only after negative-label confidence is separately studied. |

## 6. FIRMS

FIRMS is stored today as raw `payload JSONB` in `raw.firms_observations`
(migration `0009`), with no dedicated `h3`/date columns — but `latitude`,
`longitude`, and `acq_date` are present as strings *inside* the JSONB
payload, so a coincidence check is directly queryable (`(payload->>
'latitude')::double precision`, etc.), contrary to what was assumed before
this audit. `store::firms_points_for_negative_design_check` extracts them
read-only, for this check only.

**Measured** (mission section F), against the isolated pilot DB's 26,527
parsed FIRMS observations:

- Coverage: **2026-07-26 to 2026-07-27 only** (min/max `acq_date`). FIRMS
  ingestion here is near-real-time recent data, not a historical archive —
  it does **not** cover the 2020–2026 period the dataset spans
  (`firms_covers_full_2020_2026_period = false`, checked directly, not
  assumed). Per the mission's own caution, no year-by-year FIRMS comparison
  is attempted, since only a 2-day window exists to compare against.
- Contamination check against the 22,400-candidate background population:
  28 candidates share a cell with *some* FIRMS point (any date), 0 share
  both the same cell and the same exact date — expected, since essentially
  none of the background candidates' dates fall in the 2-day FIRMS window.

**Decisions, unchanged by this measurement:**

- **FIRMS as a label**: forbidden, unconditionally. FIRMS is a detection
  signal with its own false-positive/false-negative profile, not ground
  truth for human ignition.
- **FIRMS as a cautionary exclusion filter**: the join is now technically
  possible (unlike what was assumed in the first draft of this document),
  but **still not adopted for v1** — the 2-day coverage window makes any
  filter built on it apply to a negligible, non-representative fraction of
  the 2020–2026 candidate population. Revisit once/if a longer FIRMS
  ingestion history is accumulated.
- **Absence of FIRMS as proof of absence of fire**: forbidden,
  unconditionally — FIRMS has known detection gaps (cloud cover, satellite
  revisit time, fire size threshold), and today's 2-day coverage makes this
  doubly true: absence of FIRMS for 2020–2025 reflects absence of ingested
  data, not absence of fire.

## 7. Splits and seeding

Negatives are sampled **separately per split** (`train 2020-2023`,
`calibration 2024`, `test 2025`, `prospective 2026`), never pooled across
splits before sampling — `stratified_select` takes `split` as an explicit
parameter and folds it into both the per-candidate hash and the
deterministic key, so the same candidate sampled for two different splits
(impossible in practice, since a cell-date belongs to exactly one split by
year, but defended anyway) would still get distinct, non-colliding keys.

No positive from a later split (e.g., 2025 `test`) is read when deciding the
train split's sampling parameters — `stratified_select` only ever receives
one split's own `positive_counts_by_stratum` and one split's own
`candidates_by_stratum`; there is no cross-split argument in its signature
by construction, not by discipline alone.

Seed derivation: `deterministic_negative_key` and `stratified_select` take
`dataset_version_logical_id` (or an equivalent stable seed), `strategy_id`,
`ratio`, and `split` as explicit, separate hash inputs — changing any one of
the four changes every derived key, and holding all four fixed reproduces
the identical selection, verified by
`stratified_select_is_deterministic_for_a_given_seed`.

## 8. Ratios

At 1:1 the dataset stays small and balanced but may underrepresent the
diversity of "no ignition" conditions; at 10:1 the dataset grows an order
of magnitude while the positive signal becomes a small minority, increasing
computational cost and risking the model defaulting toward the majority
class without compensating class weights. No ratio is adopted as final in
this phase; `stratified_select` takes `ratio` as a parameter precisely so
this remains an experiment variable, not a hardcoded constant.

**Feasibility, measured against the real per-split positive counts and the
v2 background candidate population** (800 combustible cells × 4 dates/year
× 7 years = 22,400 candidates, ≤2.3% excluded by any strategy):

| Split | Positives (inclusive) | Positives (strict) | Background candidates available |
|---|---|---|---|
| train (2020-2023) | 5,009 | 1,882 | 13,575 |
| calibration (2024) | 663 | 235 | 3,275 |
| test (2025) | 1,177 | 449 | 3,350 |
| prospective (2026) | **0** | **0** | 3,200 |

| Ratio | train (inclusive, needs) | calibration (needs) | test (needs) |
|---|---|---|---|
| 1:1 | 5,009 (37% of pool) | 663 (20%) | 1,177 (35%) |
| 3:1 | 15,027 (**exceeds pool**) | 1,989 (61%) | 3,531 (**exceeds pool**) |
| 5:1 | 25,045 (**exceeds pool**) | 3,315 (**exceeds pool**) | 5,885 (**exceeds pool**) |
| 10:1 | 50,090 (**exceeds pool**) | 6,630 (**exceeds pool**) | 11,770 (**exceeds pool**) |

**This "exceeds pool" is a limitation of this experiment's deliberately
small sample (800 cells), not of the real candidate universe.** The true
population is `public.cell_static`'s 761,560 combustible cells × the full
day-count of each split (up to ~1,461 days for train alone) — vastly larger
than any of these ratios require. The real constraint for an eventual
implementation is **not** raw availability but **avoiding excessive reuse
of the same small set of cells across many dates** (mission section G):
this v2 experiment itself reuses each of its 800 sampled cells 4 times a
year, which is a reasonable stratification density for a fast comparison
but would need to be drawn independently per date (or from a much larger
cell sample) in a final implementation to avoid teaching the model
"these specific 800 cells are the negative ones."

**`prospective` (2026) has zero positives, measured directly, not assumed**
(`ml.dataset_rows` currently holds 0 label=1 rows with `split = prospective`
in both variants). Sampling negatives for a split with no positives is
mechanically well-defined (`stratified_select` returns an empty selection
when `total_positives == 0`, tested by
`stratified_select_returns_empty_when_no_positives_in_split`) but is a
genuine open design question, not just an edge case to handle gracefully:
should 2026 carry any negatives at all before any 2026 positive exists? This
is flagged as unresolved, not decided here.

## 9. Missing dependencies (recorded, not invented around)

- No versioned department/region reference for arbitrary H3 cells (only for
  BDIFF event coordinates) — blocks true department-based stratification
  and leave-one-department-out validation; H3-parent-cell blocks are used
  as the practical substitute.
- No normalized, queryable FIRMS table (h3/date derived from `payload`) —
  blocks FIRMS-as-cautionary-filter even though the underlying raw data
  exists.
- No per-day BDIFF-completeness signal — blocks a "days with incomplete
  BDIFF data" exclusion rule as specified in the mission.
- No historical (non-"current-snapshot") `cell_static` vintage — every
  negative candidate's features would carry the same
  `current_snapshot_applied_historically` classification and limitation
  already documented for positives in phase 3B.3.

## 10. Spatial validation (prepared, not implemented)

Future spatial-holdout options, to be revisited once a real administrative
reference exists or a leave-one-block-out scheme is explicitly authorized:
H3-parent block, department, region, generic spatial blocks, leave-one-
department-out, and a buffered gap between spatially-adjacent train/test
blocks (to prevent spatial leakage through nearby, correlated cells). No
geographic holdout is implemented in this phase; only the statistics needed
to eventually support one (candidates per H3-parent block) are produced.

## 11. Known limitations of this design pass

- The v2 background population (§ report) uses 4 representative dates per
  year per sampled cell, not full daily coverage — window-exclusion rates
  measured against it are a real, honest measurement of *this* population,
  but still not a precise estimate of what a full-coverage candidate pool
  would yield. The v2 **probe** population is deterministic and exhaustive
  at the k-ring/day-offset boundaries it targets, so the *shape* of each
  strategy's behavior (N2 excludes more than N1, etc.) is directly verified,
  not merely sampled.
- Ratio feasibility (§8) is measured against the v2 sample size, and the
  "exceeds pool" results there are a sample-size artifact, not a true
  scarcity — flagged explicitly, not left to be misread.
- Matched case-control and hard-negative sampling are documented but not
  implemented or measured in this pass, per the mission's own caution about
  amplifying label noise.
- The resolution mismatch (§3bis) is fixed in the new design code only; the
  already-committed phase 3B.3 pilot rows still carry the same underlying
  inconsistency (`h3_resolution: 8` metadata on rows whose real `h3` value
  is resolution 9) and are not modified by this phase.
