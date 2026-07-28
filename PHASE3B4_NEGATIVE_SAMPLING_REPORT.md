# ERYTHEON — Phase 3B.4 negative-sampling report

Design comparison and measurement report for the negative-sampling
strategy. Design rationale is in `NEGATIVE_SAMPLING_DESIGN.md`; this report
covers what was actually measured, its limits, and the resulting v1
recommendation. **No dataset was built or rebuilt in this phase; no model
was trained.**

## 1. What was measured

`crates/store/tests/negative_sampling_experiment.rs`
(`experimental_negative_sampling_window_comparison`) ran against the
isolated pilot DB (`erytheon-3b3-deploy-20260727T203310Z`):

- Event set: all 15,956 ignition events 2020–2026, any cause, each with its
  real geographic-quality category (`store.all_events_with_geographic_
  quality`) — confirmed by assertion, before any comparison ran, to include
  both `natural_known` and `unknown` rows, not `human_known` only.
- Candidate population: 300 combustible cells (`sample_combustible_cells`,
  seed `2026071`, same mechanism the pilot itself uses) × one representative
  date per covered year (June 15, 2020–2026) = **2,100 candidates**.
- For each of `ExclusionStrategy::{N0,N1,N2,N3}`, every candidate was tested
  against every event within 3 days of it (the widest day radius any
  strategy defines) using `dataset::negative_design::is_within_window`,
  which computes real H3 grid distance (`h3o::CellIndex::grid_distance`),
  not an approximation.

## 2. Results

| Strategy | Candidates | Excluded | Remaining | Exclusion rate |
|---|---|---|---|---|
| N0 (exact cell-date) | 2,100 | 0 | 2,100 | 0.0000 |
| N1 (k-ring 1, ±1 day) | 2,100 | 0 | 2,100 | 0.0000 |
| N2 (k-ring 2, ±3 days) | 2,100 | 1 | 2,099 | 0.0005 |
| N3 (adaptive by geo quality) | 2,100 | 3 | 2,097 | 0.0014 |

These are the real, measured numbers from the run above — not adjusted or
rounded to match an expectation.

## 3. Why the exclusion rates are this low, honestly

This is not evidence that windowing "doesn't matter." It is a direct
consequence of the experiment's own candidate population being sparse:

- 300 cells is a small fraction of the combustible cells in
  `public.cell_static` (920,016 total rows). At this sampling density, the
  odds that a randomly drawn cell falls within even a k-ring-5 neighborhood
  of one of the ~2,278 events in a given year are low.
- One candidate date per year (June 15) means each candidate has only a
  handful of days of exposure to any given nearby event, rather than the
  365 (or 366) days of exposure a full-coverage candidate pool would have.
  A full-coverage pool would let *every event's own date* automatically
  coincide with some candidate in that cell — this experiment's fixed date
  almost never lands on an actual event date by chance.

**This means the measured exclusion rates in the table above should be read
as a lower bound produced by an intentionally coarse, fast experiment, not
as an estimate of how many negatives a full-coverage candidate pool would
lose to windowing.** A full build with daily candidate coverage would show
substantially higher exclusion rates, especially for N2/N3 — this is stated
explicitly so it is not later mistaken for the real figure.

## 4. What this measurement still tells us honestly

- All four strategies are computationally cheap at this scale (the full run
  — 2,100 candidates × up to ~15,956 events each, with an H3 grid-distance
  check only for events within 3 days — completed in about 11 seconds in an
  unoptimized debug build).
- N2 and N3 are strictly more exclusionary than N0/N1 at this sample size
  (1 and 3 exclusions respectively, versus 0), confirming the strategies
  behave in the intended relative order (wider window → more exclusions)
  even though the absolute numbers are small here.
- The mechanism itself (cause-agnostic exclusion, geographic-quality-adaptive
  widening) is exercised end-to-end against real production-derived data, not
  just unit-tested in isolation.

## 5. Ratio comparison

Not measured numerically in this pass — see `NEGATIVE_SAMPLING_DESIGN.md` §8.
Measuring ratio tradeoffs meaningfully requires the full-coverage candidate
population noted above; doing that now would mean building a much larger
experimental population than this fast comparison pass was scoped for, and
risks blurring into an actual dataset build, which this phase does not
authorize.

## 6. Biases and limits

- **Sparse-candidate bias** (§3): the dominant limitation of this specific
  measurement; does not reflect a flaw in the strategies themselves.
- **No department/region stratification measured**: no versioned
  administrative reference exists for arbitrary cells (documented in
  `NEGATIVE_SAMPLING_DESIGN.md` §9); H3-parent-block stratification is
  implemented (`spatial_seasonal_stratum`) but not run against real data in
  this pass.
- **FIRMS not evaluated as a filter**: blocked by the same missing
  normalized-FIRMS-table dependency documented in the design doc.
- **Matched case-control and hard-negative sampling**: not implemented or
  measured, per the mission's own caution about amplifying label noise.

## 7. Recommendation: negative-sampling strategy v1

Based on the design comparison in `NEGATIVE_SAMPLING_DESIGN.md` and this
measurement:

- **Population**: combustible cells with complete features, drawn from the
  same source as today's pilot (`sample_combustible_cells`), extended to
  **full daily coverage** (not one date/year) before any ratio/ratio-
  dependent decision is finalized.
- **Exclusion window**: **N3 (adaptive by geographic-quality category)** —
  it is the only strategy that avoids both under-excluding around
  imprecisely-located events and over-excluding around precisely-located
  ones, and it degrades gracefully to a cautious default when the category
  is unknown/undetermined (verified: `n3_treats_unknown_category_at_least_
  as_wide_as_any_named_category`).
- **Cause handling**: exclude around events of **any** cause
  (`human_known`, `natural_known`, `unknown`) using the same N3 window per
  event's own geographic-quality category; never treat `natural_known` or
  `unknown` as a negative label.
- **FIRMS**: not used as a label; not used as an exclusion filter in v1
  (missing normalized dependency); revisit once a normalized FIRMS
  table exists.
- **Stratification**: spatial (H3-parent-block) + seasonal (month), per
  `spatial_seasonal_stratum` — the only stratification dimension available
  without the missing administrative reference.
- **Ratio**: **not fixed in v1** — recommend measuring 1:1, 3:1, and 5:1
  against the full-coverage candidate population (§5) before choosing one;
  10:1 is not recommended without a compensating class-weight plan given the
  computational and class-imbalance costs already flagged in the design doc.
- **Splits**: negatives sampled independently within each of `train
  2020-2023` / `calibration 2024` / `test 2025` / `prospective 2026`, never
  pooled before sampling, per `stratified_select`'s own signature.
- **Seed**: derived from `(dataset_version_logical_id, strategy_id, ratio,
  split)`, reproducible by construction.
- **Weights**: not recommended in v1; revisit once ratio is fixed and class
  balance is actually measured on a full-coverage population.

**This is a recommendation for the next authorized phase, not an
implementation.** No code path wires this into `build_human_dataset`.

## 8. Dependencies missing before v1 can be implemented as final

1. A full-coverage (daily, not one date/year) candidate population.
2. A ratio measurement against that full-coverage population.
3. Either a versioned department/region reference, or an explicit decision
   to accept H3-parent-block as the permanent spatial-stratification proxy.
4. A normalized, queryable FIRMS table, if FIRMS-as-filter is ever revisited.

## 9. Risks carried into any future implementation

- Under-reporting/detection gaps in BDIFF mean any negative population,
  however carefully windowed, still risks including real but unrecorded
  ignitions — the exclusion windows reduce but do not eliminate this risk.
- The strict-variant `insufficient_geographic_quality` exclusion already
  removes about half of admissible positives (phase 3B.3); a negative
  population windowed by the same geographic-quality categories should be
  designed consistently with that existing asymmetry, not independently.

## 10. Suite status

Full workspace test suite (`cargo test --workspace`, isolated VPS build
container, rust 1.94): `dataset` 35/35, `store` unit 0/0 + all integration
suites including `dataset_foundation` 4/4 and the new
`negative_sampling_experiment` 1/1, `engine` 26/26 — all green, including
the two geographic-category regression tests from the phase 3B.3 hardening
pass and the eleven new `negative_design` tests.

---

```
PHASE 3B.3 REVIEW PASSED
PHASE 3B.4 NEGATIVE SAMPLING DESIGN READY FOR REVIEW
NO PRODUCTION DEPLOYMENT
NO MODEL TRAINING
NO PUSH
```
