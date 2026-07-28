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

FIRMS is stored today only as raw, unstaged `payload JSONB` in `raw.firms_
observations` (migration `0009`) — there is no normalized table with
computed `h3`/local date, so "cell-day has a FIRMS detection" is not
currently a queryable join. Decisions, independent of that gap:

- **FIRMS as a label**: forbidden, unconditionally. FIRMS is a detection
  signal with its own false-positive/false-negative profile, not ground
  truth for human ignition.
- **FIRMS as a cautionary exclusion filter** (candidate has a FIRMS
  detection without a corresponding BDIFF record → possibly contaminated,
  drop from the negative pool): acceptable in principle, but **not adopted
  for v1** because the normalized join it needs does not exist yet. This is
  a missing dependency (§9), not a rejected idea.
- **Absence of FIRMS as proof of absence of fire**: forbidden,
  unconditionally — FIRMS has known detection gaps (cloud cover, satellite
  revisit time, fire size threshold); its absence proves nothing.

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

Compared conceptually (1:1, 3:1, 5:1, 10:1 negative:positive), not yet
measured against a real full-coverage candidate population (the
experimental population here uses one date/year per cell, not daily
coverage — see limitation in §11). At 1:1 the dataset stays small and
balanced but may underrepresent the diversity of "no ignition" conditions;
at 10:1 the dataset grows an order of magnitude while the positive signal
becomes a small minority, increasing computational cost and risking the
model defaulting toward the majority class without compensating class
weights. No ratio is adopted as final in this phase; `stratified_select`
takes `ratio` as a parameter precisely so this remains an experiment
variable, not a hardcoded constant.

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

- The experimental candidate population (§ report) uses one representative
  date per year per sampled cell (mirroring the pilot's own simplification),
  not full daily coverage — window-exclusion rates measured against it are
  directionally informative, not a precise estimate of what a full-coverage
  candidate pool would yield.
- Ratio and ratio-dependent tradeoffs (§8) are described, not measured,
  pending a full-coverage candidate population.
- Matched case-control and hard-negative sampling are documented but not
  implemented or measured in this pass, per the mission's own caution about
  amplifying label noise.
