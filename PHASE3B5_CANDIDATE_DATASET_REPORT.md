# ERYTHEON — Phase 3B.5 candidate dataset report

Final report for the phase 3B.5 candidate dataset construction. **No
model was trained or calibrated; no dataset was finalized; nothing was
pushed or deployed.**

## 1. Git

```
git status --short   → clean before and after this pass
Base commit           → cc94f40 (feat: validate negative sampling strategies)
```

`git diff origin/main..HEAD --name-only` confirms no file under
`crates/api`, `crates/risk`, `crates/fwi`, FIRMS, or the scheduler was
touched by this phase. No secret, no dump, no build artifact in the diff.

New/changed files this phase: `crates/dataset/src/features_h3.rs` (new),
`crates/dataset/src/normalization.rs` (new), `crates/dataset/src/rows.rs`
(added `hist` feature field), `crates/dataset/src/splits.rs` (added `Hash`
derive, needed for split-keyed maps), `crates/store/src/dataset.rs` (new
read methods: `all_cell_static_rows`, `calendar_rule_version_id`,
`calendar_days_in_range`, `firms_points_for_negative_design_check` already
existed from 3B.4), `crates/store/src/lib.rs` (re-exports),
`crates/engine/src/candidate_pipeline.rs` (new, the builder),
`crates/engine/src/main.rs` (new `build-candidate-dataset` CLI command),
`crates/store/tests/candidate_dataset_consistency.rs` (new).

## 2. Architecture

See `FULL_CANDIDATE_DATASET.md` for the complete design: the resolution-9-
to-8 feature aggregation, positive/negative construction, ratio choice,
and idempotence fix. No new migration was applied — the existing `ml.*`
schema from migration `0015` (phase 3B.3) is reused as-is; normalization/
imputation parameters are stored as JSON in the existing `notes` column
(see `DATASET_NORMALIZATION_AND_IMPUTATION.md` §6 for why a dedicated
table was not warranted yet).

## 3. Features used

| Feature | Type | Source | Temporal classification | Missing handling |
|---|---|---|---|---|
| `wui`, `road`, `agri`, `population`, `poi`, `power_line`, `hist` | numeric [0,1] | `public.cell_static`, aggregated res9→res8 (mean) | `current_snapshot_applied_historically` | mean-of-present-children per cell; whole-cell `missing_features` if zero children |
| `combustible` | boolean | same aggregation, `any(child combustible)` | `current_snapshot_applied_historically` | n/a (drives eligibility, not a normalized feature) |
| `weekend`, `public_holiday` | boolean | `features.historical_calendar_days` (phase 3B.3) | `historical_exact` | n/a |
| `season_sine`, `season_cosine` | numeric | same calendar | `historical_exact` | n/a |
| `school_holiday` | boolean, nullable | same calendar | `unavailable_historically` | always `NULL`; never fabricated |

No feature is included silently: every one above is either wired from a
real source (this phase's fix) or explicitly `None`/`unavailable_
historically` (`school_holiday`). No placeholder zero remains for any
feature that has a real source.

## 4. Ratio: chosen and feasibility

**3:1** chosen (mission's suggested default, confirmed not forced — see
`FULL_CANDIDATE_DATASET.md` §5). Feasibility against the measured
761,556-cell eligible population: all of 1:1/3:1/5:1/10:1 trivially
satisfiable for every split; the phase 3B.4 report's smaller-sample
feasibility table (§6 there) is superseded by this larger, real
population for production-scale reasoning, though its ratio-tradeoff
discussion (dataset size, class imbalance, compute cost) still applies
conceptually.

## 5. Stratification

Positives: real event dates (year/month/split), by construction. Negatives:
uniform draw across each split's date range and the full eligible cell
population — see `FULL_CANDIDATE_DATASET.md` §6 for why the tested
month/H3-parent-block quota sampler (`stratified_select`) was not wired
into this particular build, flagged as a limitation, not hidden.

## 6. Datasets built

| Logical ID | `dataset_version_id` | Rows | Positive | Negative | Exclusions | Checksum |
|---|---|---|---|---|---|---|
| `..._candidate_strict_n2_kring2_day3` | `c0b20bbf-f3aa-4cec-98d4-47fceca10ec8` | 23,113 | 2,566 | 20,547 | 4,286 | `7f61271e...` |
| `..._candidate_inclusive_n2_kring2_day3` | `5d55158a-3a40-43b8-850b-272c86fce986` | 27,396 | 6,849 | 20,547 | 4,286 | `f9fa6b31...` |
| `..._candidate_strict_n3_adaptive_geographic_quality` | `04df2b9a-f8a2-4f60-9ed7-858c2be99892` | 23,113 | 2,566 | 20,547 | 4,286 | `4a011a7b...` |
| `..._candidate_inclusive_n3_adaptive_geographic_quality` | `c7b18870-b4e4-49e1-bdc7-89c0574c8138` | 27,396 | 6,849 | 20,547 | 4,286 | `8bbb2ee8...` |

All `status = draft`, all `seed = 2026071`, all `code_version` = current
commit's package version. Each built once, then replayed once
(`reused_existing_version: true`, identical `dataset_version_id` and
checksum, new `build_id`) — see §8.

**Positives by split** (identical across N2/N3 of the same variant, since
positives don't depend on negative strategy):

| Split | Strict | Inclusive |
|---|---|---|
| train | 1,882 | 5,009 |
| calibration | 235 | 663 |
| test | 449 | 1,177 |
| prospective | 0 | 0 |

## 7. Idempotence (bug found and fixed)

The negative sampler's candidate-cell pool (`eligible_negative_cells`) was
originally built by collecting a `HashMap`'s iteration order into a `Vec`
— but Rust randomizes `HashMap` iteration order per process, so the same
seed selected a *different* actual cell each run. The first replay
attempt, before this was caught, inserted ~2x the expected rows instead of
reusing identical ones (47,942 vs 27,396 for `inclusive_n2`). Fixed by
sorting `eligible_negative_cells` (`sort_unstable()`) before any seeded
indexing. Corrupted state was deleted and both variants rebuilt fresh.

Re-verified after the fix:

- Fresh build, then one identical replay (`--seed 2026071 --ratio 3`).
- All four variants: `reused_existing_version: true`, same
  `dataset_version_id`, same checksum, only a new `build_id`.
- `SELECT count(*), count(DISTINCT deterministic_key) ...`: equal for all
  four (23,113/23,113, 27,396/27,396, 23,113/23,113, 27,396/27,396) — no
  duplication.
- Two `ml.dataset_builds` rows per version (original + replay).
- Automated in `crates/store/tests/candidate_dataset_consistency.rs`
  (`candidate_datasets_pass_all_consistency_checks`), which additionally
  asserts `status = 'draft'` for all four and `>= 2` builds per version —
  green.

## 8. Consistency checks

All measured directly against the built data (SQL and the Rust test
above), all clean:

| Check | Result |
|---|---|
| `count(*) = count(DISTINCT deterministic_key)` per version | equal for all 4 |
| Positive cell-date also present as negative | 0 |
| Row with `h3_resolution <> 8` | 0 |
| Row outside `2020-01-01..2026-12-31` | 0 |
| `(h3, local_date)` assigned to more than one split | 0 |
| `status <> 'draft'` | 0 |
| Version with fewer than 2 builds (no replay) | 0 |

## 9. Temporal leakage

| Check | How verified |
|---|---|
| Train reads no 2024-2026 event | `split_bounds(Train) = 2020-01-01..2023-12-31`; positives/negatives for train are drawn only from events/candidates within that range, by construction of `Split::for_year` and the per-split sampler call. |
| Calibration/test don't modify train parameters | No model was trained (mission interdiction); `sample_negatives_for_split` and `train_only_statistics` each take exactly one split's own data, no cross-split parameter passing exists in either signature. |
| Seeds independent per split | `base_seed = seed ^ split_tag ^ strategy_tag` — changing `split` changes every derived seed. |
| Historical aggregates strictly before the row's date | Inherited from phase 3B.3's calendar/snapshot foundation (`no_future_information_leaks_into_a_past_year`, `never_selects_a_snapshot_from_the_future`), re-verified green in this pass's `cargo test --workspace`. |
| Normalization/imputation fit on train only | `build_one_variant` filters `rows` to `split == "train"` before calling `train_only_statistics`/`fit_imputation_rule` — see `DATASET_NORMALIZATION_AND_IMPUTATION.md`. |
| Current snapshot flagged as historical approximation | `current_snapshot_applied_historically` recorded in every row's `temporal_availability` and in each dataset version's `exclusion_rules.res8_feature_checksum` provenance — unchanged limitation from phase 3B.3, not hidden. |
| Test statistics don't drive train sampling | The sampler takes no test-split input when sampling train; parameters (`seed`, `ratio`, `strategy`) are fixed CLI arguments, not derived from any split's realized data. |

## 10. Normalization and imputation

See `DATASET_NORMALIZATION_AND_IMPUTATION.md` for full detail. Summary:
7 numeric features, 4 distinct normalization methods assigned by real
distribution shape (not uniform), 0% measured missingness in the
`inclusive_n3` build's 20,036 train rows (imputation machinery is real and
tested on synthetic gaps, simply not exercised by real gaps in this
particular build).

## 11. Performance

| Stage | Measured |
|---|---|
| `cell_static` full read (920,016 rows) | 3.0–6.8s (varied by run; logged as a `sqlx` slow-query warning both times) |
| Full pipeline (aggregation + positives + negative sampling for both strategies + all 4 builds) | ~100s wall clock, first build |
| Replay (all 4 variants, idempotent) | ~100s wall clock |
| Largest single row-insert batch | 27,396 rows in 1.6–9.1s (varied by run) |
| Resident memory during aggregation | ~1.6–1.8 GB peak (within the 4 GiB container budget) |

**On not materializing 761,556 x 2,557 cell-days**: the negative sampler
never computes this product. It draws directly from `(cell index, day
offset)` via a seeded hash and checks each drawn candidate individually
against the exclusion window — the only per-candidate cost, not a
combinatorial one. Total candidates ever materialized across all sampling:
on the order of the oversample pool sizes (needed x 5, no split required
escalation beyond that), not billions.

## 12. Comparison: strict vs. inclusive

Per strategy, strict is a strict subset of inclusive's positive population
(same construction as phase 3B.3): 2,566 vs. 6,849 positive rows — the
`insufficient_geographic_quality` (3,624), `missing_features` (22), and
`non_combustible_cell` (637) exclusions account for the gap, per split
identical to phase 3B.3's already-reported per-variant totals. Negative
rows are **identical** between strict and inclusive of the same strategy
(shared population, §6) — the datasets differ only in which positives are
included, not in their negative side.

## 13. Comparison: N2 vs. N3

Both strategies were sized to the same target count per split (inclusive
positives x ratio), so **total negative counts are identical** between N2
and N3 (20,547 each) — this alone says nothing about whether the
strategies select the same candidates. Measured directly: of
`inclusive_n2`'s and `inclusive_n3`'s 20,547 negatives each, only **1** is
shared between the two. **99.995% of negative candidates differ** between
N2 and N3 despite matching counts — the two strategies genuinely diverge
in which cell-days they consider safe to use as negatives, exactly as
their different exclusion-window definitions would predict (N3's
per-category adaptive windows vs. N2's fixed k-ring2/±3days), not a
cosmetic difference in name only.

## 14. Open risks

- Negative stratification is uniform, not the tested quota-based sampler
  (real risk of some imbalance not directly measured in this build).
- Strict/inclusive negative-count sharing means strict's realized ratio
  exceeds the nominal 3:1.
- `prospective` negatives exist with no positives to pair them against yet.
- The pilot's pre-existing resolution metadata inconsistency is
  untouched (out of this phase's scope).
- `current_snapshot_applied_historically` and `school_holiday`
  unavailability are unchanged, inherited limitations.
- Negative sampling still assumes no per-day BDIFF completeness signal and
  no FIRMS filter (phase 3B.4 findings, unchanged).

## 15. Recommendation for the training phase

Do not train on these datasets as-is without first: (a) deciding the
`prospective`-negatives-without-positives policy; (b) either wiring the
tested month/H3-parent-block stratified sampler into the negative draw or
explicitly accepting uniform sampling with a measured imbalance check; (c)
choosing which of N2/N3 (or both, as main + sensitivity, per phase 3B.4's
recommendation) to actually train against, informed by this report's §13
finding that they are substantively different candidate pools, not
interchangeable relabelings of the same rows.

---

```
PHASE 3B.5 CANDIDATE DATASETS BUILT
READY FOR SCIENTIFIC REVIEW
NO PRODUCTION DEPLOYMENT
NO MODEL TRAINING
NO PUSH
```
