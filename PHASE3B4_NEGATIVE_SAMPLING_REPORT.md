# ERYTHEON — Phase 3B.4 negative-sampling report (v2, post-audit)

Design comparison and measurement report for the negative-sampling
strategy. Design rationale is in `NEGATIVE_SAMPLING_DESIGN.md`; this report
covers what was actually measured, the audit that made a v2 measurement
necessary, its limits, and the resulting v1 recommendation. **No dataset
was built or rebuilt in this phase; no model was trained.**

## 1. Why this is v2

The first report (v1) measured N0=0, N1=0, N2=1, N3=3 exclusions on a
2,100-candidate population and attributed this to sparse sampling. A
follow-up audit (mission section C) found the real cause instead: **`public
.cell_static` (candidates) is H3 resolution 9; `fire.ignition_events`
(the exclusion event set) is resolution 8.** Two `CellIndex` values of
different resolutions compare as always-unequal or always-erroring, so v1's
N0 check (exact equality) could never fire regardless of true proximity,
and v1's N1–N3 checks depended on how the resulting `grid_distance` error
was handled. This was not a sampling-density problem; it was a structural
bug. See `NEGATIVE_SAMPLING_DESIGN.md` §3bis for the full audit and the fix
(`normalize_to_common_resolution` in `crates/dataset/src/negative_design.
rs`, resolution-aware from now on).

Fixing it also required auditing and fixing several things the original
v1 population didn't exercise, per the mission's explicit follow-up:

1. Resolution normalization (above).
2. A properly stratified, much larger candidate population that
   deliberately includes both near-event and far-from-event candidates
   (v1's uniform random sample could — and did — miss meaningful proximity
   almost entirely by chance).
3. A full breakdown per strategy (by cause, by spatial/temporal locus, by
   year/split/month, timing, memory).
4. A real (not assumed-blocked) FIRMS contamination check.
5. Ratio feasibility against real per-split positive counts.

## 2. Git and quality gates

- `git status --short`: clean before and after this pass.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D
  warnings`: **green**, 0 errors, run to completion in the isolated VPS
  build container (`rust:1.94-bookworm`, 2 CPU / 2 GiB, `CARGO_BUILD_JOBS=2`).
  This required regenerating `Cargo.lock` (`cargo generate-lockfile`) to
  register the new `dataset` dev-dependency of `store`; the regeneration
  also picked up unrelated semver-compatible patch/minor bumps for many
  transitive dependencies (no major version changes) — a side effect of
  `--locked` requiring a consistent lockfile, not a deliberate dependency
  upgrade decision.
- `cargo fmt --check`: clean.
- `cargo test --workspace`: all suites green (§8).
- No new dump, no new isolated `PostgreSQL`, no production container
  touched — reused the existing `erytheon-3b3-deploy-20260727T203310Z`
  isolated DB and a fresh, disposable build container removed after use.

## 3. Census methodology (v2)

Built in `crates/store/tests/negative_sampling_experiment.rs`, entirely via
streamed queries and in-memory computation over query results — no dataset
rows materialized, no `ml.dataset_*` writes.

**Probe population (1,000 candidates)**: 40 admissible events selected
deterministically (every 400th, by stable `(date, h3)` order, spanning
2020–2026). For each probed event, one candidate at each of H3 grid
distance `{0, 1, 2, 3, 5}` (via `H3Grid::neighbors_with_distance`) × day
offset `{0, 1, 2, 3, 5}` (25 per event). This *guarantees* every strategy's
boundary is exercised by construction — not hoped for by chance, which is
what made v1 uninformative.

**Background population (22,400 candidates)**: 800 combustible cells
(`sample_combustible_cells`, seed `2,026,071`, same mechanism the pilot
itself uses) × 4 representative dates per year (mid-February/May/August/
November) × the 7 covered years. Deliberately larger and more
seasonally-stratified than v1's single date/year, while staying tractable
for an in-process comparison (no full daily materialization, per the
mission's own instruction not to build a final dataset).

**Total: 23,400 candidates**, distributed:

| Year | Population |
|---|---|
| 2020 | 3,375 |
| 2021 | 3,350 |
| 2022 | 3,475 |
| 2023 | 3,375 |
| 2024 | 3,275 |
| 2025 | 3,350 |
| 2026 | 3,200 |

| Split | Population |
|---|---|
| train | 13,575 |
| calibration | 3,275 |
| test | 3,350 |
| prospective | 3,200 |

Event set: 15,956 events, `human_known` 7,094 / `natural_known` 791 /
`unknown` 8,071 — confirmed non-`human_known`-only before any comparison
ran (`assert!` in the test itself, not just documentation).

## 4. Results: N0–N3, full breakdown

| Strategy | Candidates | Excluded | Remaining | Exclusion rate | Elapsed |
|---|---|---|---|---|---|
| N0 (exact cell-date) | 23,400 | 40 | 23,360 | 0.17% | 25.3s |
| N1 (k-ring1, ±1 day) | 23,400 | 170 | 23,230 | 0.73% | 27.3s |
| N2 (k-ring2, ±3 days) | 23,400 | 516 | 22,884 | 2.21% | 29.6s |
| N3 (adaptive) | 23,400 | 466 | 22,934 | 1.99% | 27.7s |

Memory: resident-set size (`/proc/self/status` `VmRSS`) stayed at ~15.4 MB
before and after every strategy pass — the candidate/event data held in
memory is small (23,400 candidates + 15,956 events, both flat structures);
no strategy accumulates additional memory during its pass.

**Note on N3 vs. N2**: N3 (466) excludes *fewer* candidates than N2 (516)
here — not a bug. N3's per-category windows are narrower than N2's fixed
window for two of the three real categories (`precision_undocumented`:
k-ring1/±1day; `rounded_coordinate_probable`: k-ring3/±2days) and only
wider for `municipality_centroid_probable`/unknown (k-ring5/±3days). Since
`precision_undocumented` dominates the real category distribution, N3 is on
average less exclusionary than N2's blanket k-ring2/±3days — a genuine,
non-obvious finding worth stating plainly rather than assuming "adaptive
always excludes more."

**By cause** (a candidate can be excluded by more than one event/cause; not
mutually exclusive):

| Strategy | Excluded by human | by natural | by unknown |
|---|---|---|---|
| N0 | 19 | 1 | 20 |
| N1 | 78 | 5 | 89 |
| N2 | 248 | 16 | 280 |
| N3 | 233 | 12 | 242 |

Natural and unknown causes contribute meaningfully to exclusions at every
strategy — confirming they are not a negligible edge case in this
mechanism, consistent with never treating them as negative labels
themselves.

**By locus** (spatial-only: same date, different cell within k-ring;
temporal-only: same cell, different date within radius; combined: both
differ — again not mutually exclusive across the multiple events a
candidate can be near):

| Strategy | Spatial-only | Temporal-only | Combined |
|---|---|---|---|
| N0 | 0 | 0 | 40 |
| N1 | 40 | 45 | 87 |
| N2 | 86 | 124 | 310 |
| N3 | 118 | 68 | 289 |

**By origin** (probe vs. background — confirms the probe population is
doing its job of guaranteeing measurable exclusions, while background gives
a realistic sparse-population estimate):

| Strategy | Probe excluded / 1,000 | Background excluded / 22,400 |
|---|---|---|
| N0 | 40 | 0 |
| N1 | 168 | 2 |
| N2 | 487 | 29 |
| N3 | 432 | 34 |

**By year / split / month**: recorded in full in the test's console output
(`experimental_negative_sampling excluded_by_year/split/month`); omitted
here for length, available by re-running
`cargo test -p store --test negative_sampling_experiment -- --nocapture`.
No split or month shows a qualitatively different pattern from the others
at this sample size.

## 5. FIRMS

See `NEGATIVE_SAMPLING_DESIGN.md` §6 for the decision; measured here:
26,527 parsed FIRMS observations available, but coverage is **2026-07-26 to
2026-07-27 only** — confirmed directly, not assumed
(`firms_covers_full_2020_2026_period = false`). Of the 22,400 background
candidates, 28 share a cell with some FIRMS point (any date), 0 share both
cell and exact date. No year-by-year FIRMS comparison is attempted, per the
mission's own instruction, since the coverage is not homogeneous across
2020–2025 — it is nearly absent.

## 6. Ratios and stratification feasibility

See `NEGATIVE_SAMPLING_DESIGN.md` §8 for the full table. Summary: real
per-split positive counts (measured, not estimated) are train=5,009/1,882
(inclusive/strict), calibration=663/235, test=1,177/449,
**prospective=0/0**. Against this v2 experiment's own 22,400-candidate
background pool, ratios above 1:1 for train/test exceed what this
*sample* holds — an artifact of deliberately keeping the experiment
tractable (800 cells), not of the true ~761,560-combustible-cell
population, which comfortably supports any of the four ratios. The real
constraint for an eventual implementation is avoiding excessive reuse of a
small, fixed cell panel across many dates, not raw scarcity.

## 7. Temporal leakage checks (mission section H)

- **Train (2020-2023) independence from 2024-2026**: `stratified_select`'s
  signature takes one split's own `candidates_by_stratum` and
  `positive_counts_by_stratum` — there is no parameter through which a
  later split's data could reach an earlier split's selection. Verified by
  construction, not by convention.
- **Sampling parameters not tuned against 2025 test performance**: no model
  has been trained in this phase (mission interdiction), so no such tuning
  loop exists to audit — moot today, but the seed-derivation design
  (below) keeps it structurally impossible later too.
- **Seeds derived separately per split**: `deterministic_negative_key` and
  `stratified_select` fold `split` into the hash alongside
  `dataset_version_logical_id`/`strategy_id`/`ratio` — changing the split
  changes every derived key.
- **Historical aggregates use only the past**: unaffected by this phase;
  inherited from phase 3B.3's calendar/snapshot foundation (`no_future_
  information_leaks_into_a_past_year`, `never_selects_a_snapshot_from_the_
  future`), re-verified green in this pass's test run.
- **Snapshot availability controlled**: same inherited mechanism
  (`select_snapshot_for_date`), unchanged, re-verified green.
- **`current_snapshot_applied_historically` limitation**: honestly
  restated — every negative candidate's features, like every positive
  row's, would carry today's `cell_static` snapshot applied uniformly
  across 2020–2026; no negative-specific mitigation exists for this,
  because none exists for positives either (phase 3B.3 finding, unchanged).

## 8. Test suite status

Full workspace, isolated VPS build container:

- `dataset`: **39/39** (23 pre-existing from phase 3B.3 + hardening, plus
  16 in the new `negative_design` module: 12 from the first
  negative-sampling pass, plus 4 new for the cross-resolution fix in this
  pass — `cross_resolution_candidate_on_the_same_location_as_the_event_is_
  recognized`, `cross_resolution_candidate_outside_the_window_is_still_
  not_excluded`, `n2_excludes_a_kring2_neighbor_that_kring1_would_miss`,
  `window_respects_the_day_radius_boundary_across_a_month_and_year_
  change`).
- `engine`: **26/26**, including the two geographic-category regression
  tests.
- `store`: all integration suites green — `dataset_foundation` 4/4,
  `bdiff_ingestion` 1/1, `firms_ingestion` 1/1, `observations` 1/1,
  `platform_foundation` 1/1, `quality_foundation` 1/1, and
  `negative_sampling_experiment` 1/1 (the census itself, ~111s, real DB).
- `cargo clippy --workspace --all-targets --all-features --locked -- -D
  warnings`: 0 errors.
- `cargo fmt --check`: clean.

Mission-requested test coverage (section J), confirmed present:

| Requirement | Test |
|---|---|
| Candidate exactly on an event | `n0_excludes_only_the_exact_cell_date` |
| H3-neighbor candidate | same test (neighbor case) |
| k-ring2-but-not-k-ring1 candidate | `n2_excludes_a_kring2_neighbor_that_kring1_would_miss` |
| ±1 and ±3 day windows | `n1_excludes_immediate_neighbors_within_one_day`, N2 window in the k-ring2 test |
| Month/year crossing | `window_respects_the_day_radius_boundary_across_a_month_and_year_change` |
| Natural-cause event | Verified live in the census (`excluded_by_cause natural=...` at every strategy) and by the up-front `assert!` requiring `natural_known` in the event set |
| Unknown-cause event | Same mechanism, `unknown` |
| `municipality_centroid_probable` / `rounded_coordinate_probable` quality | `n3_uses_the_widest_window_for_municipality_centroid`, N3 window definition tests |
| Determinism / seed | `stratified_select_is_deterministic_for_a_given_seed`, `deterministic_negative_key_distinguishes_strategy_and_ratio` |
| Split separation | `deterministic_negative_key_distinguishes_strategy_and_ratio` (split case), `stratified_select`'s signature (§7 above) |
| 2026 without positives | `stratified_select_returns_empty_when_no_positives_in_split`, confirmed live: `prospective` positives = 0 in both variants |
| Insufficient candidates | `stratified_select_never_exceeds_available_candidates_in_a_stratum` |
| No duplicate negatives | `stratified_select_never_selects_the_same_candidate_twice` |
| Cross-resolution correctness | the two new tests in §"why v2" above |

Not covered by a dedicated unit test (acknowledged gap): a direct assertion
that aggregate statistics match a real selected sample one-to-one — the
census test reports aggregates and the `stratified_select` tests report
selections, but nothing cross-checks the two against the same input in one
test. Flagged as a minor follow-up, not blocking.

## 9. Recommendation

### Main strategy (v1 candidate)

- **Population**: combustible cells with complete features, from
  `sample_combustible_cells`'s mechanism, extended to full daily coverage
  (not the experiment's 4-dates/year) and drawn from a much larger cell
  sample (or the full 761,560-cell pool) before finalizing, to avoid the
  reuse risk noted in §6.
- **Exclusion window**: **N3 (adaptive by geographic-quality category)** —
  measured to behave correctly at every tested boundary and to degrade
  cautiously (never narrower than any named category) when the category is
  unknown. Its lower raw exclusion count than N2 here is a feature of
  matching window width to actual location certainty, not a weakness.
- **Human/natural/unknown treatment**: exclude the negative candidate pool
  around events of **any** cause, using N3's per-event geographic-quality
  category; never use `natural_known` or `unknown` as a negative label.
- **FIRMS**: not a label; not a filter in v1 (2-day coverage is not
  representative — §5).
- **Stratification**: spatial (H3-parent-block) + seasonal (month).
- **Ratio**: **not fixed** — measure 1:1/3:1/5:1 against a full-coverage,
  larger-cell-sample population before choosing (§6); 10:1 not recommended
  without a compensating class-weight plan.
- **Splits**: independent per-split sampling, as implemented.
- **Seed**: `(dataset_version_logical_id, strategy_id, ratio, split)`.
- **Weights**: not recommended in v1; revisit once ratio is fixed.
- **Limits**: resolution-9/8 mismatch fixed here only for new design code,
  not for the existing pilot rows (§ "why v2"); `current_snapshot_applied_
  historically` features apply identically to negatives as to positives;
  `prospective` (2026) has no positives and an unresolved policy for
  whether to sample negatives there at all.

### Sensitivity strategy (stricter)

**N2** (fixed k-ring2/±3days) as a deliberately more conservative
alternative to N3: wider than N3 for the dominant `precision_undocumented`
category, giving a stricter, less category-dependent baseline to compare
model results against. Use for a sensitivity analysis alongside the N3 main
run, not as a replacement.

### Experimental strategy (hard negatives)

Explicitly **not recommended for adoption**, only as a separately-flagged
experiment if pursued later: sampling combustible, high-operational-risk
cell-days with no recorded ignition as negatives. Given the documented
under-reporting/detection gaps in BDIFF (phase 3B.1/3B.2), this risks
amplifying exactly the label noise the exclusion windows are designed to
reduce. If ever run, it must be reported and evaluated separately from the
main/sensitivity strategies, never blended into them silently.

### Sensitivity analyses still needed before any final choice

1. Ratio measurement against a full-coverage, larger-sample population.
2. Comparison of N3 vs. N2 model-quality impact once training is
   authorized (not in this phase).
3. Resolution-consistency fix for `cell_static` vs. `fire.ignition_events`,
   or an explicit, documented decision to keep normalizing at read time
   indefinitely.

---

```
PHASE 3B.3 REVIEW PASSED
PHASE 3B.4 NEGATIVE SAMPLING DESIGN VALIDATED
NO PRODUCTION DEPLOYMENT
NO MODEL TRAINING
NO PUSH
```
