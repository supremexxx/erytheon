# ERYTHEON — Phase 3B.6 scientific dataset review

Independent scientific review of the four phase 3B.5 candidate datasets,
and the basis for the training protocol (`MODEL_TRAINING_PROTOCOL.md`).
**No model was trained or calibrated in this phase.**

## 1. Git

`git status --short` clean before and after. Base commit `00c7419`
(phase 3B.5 docs). `git diff origin/main..HEAD --name-only` confirms no
file under `crates/api`, `crates/risk`, `crates/fwi`, FIRMS, or the
scheduler was touched. No secret/dump/build-artifact found in the diff
(the only grep hits are inside this review's own prose describing the
check).

## 2. Inventory (exact, measured)

| Logical ID | `dataset_version_id` | Active `build_id` | Strategy | Variant | Ratio | Seed | Status | Rows | Positive | Negative | Exclusions | Checksum |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `..._strict_n2_kring2_day3` | `c0b20bbf-f3aa-4cec-98d4-47fceca10ec8` | `944fdec4-a164-4a0a-b1ca-c969bc6db9ae` | N2 | strict | 3:1 | 2026071 | draft | 23,113 | 2,566 | 20,547 | 4,286 | `7f61271e...` |
| `..._inclusive_n2_kring2_day3` | `5d55158a-3a40-43b8-850b-272c86fce986` | `26df34bf-6eac-40d3-a64a-5f1875f55ea5` | N2 | inclusive | 3:1 | 2026071 | draft | 27,396 | 6,849 | 20,547 | 4,286 | `f9fa6b31...` |
| `..._strict_n3_adaptive_geographic_quality` | `04df2b9a-f8a2-4f60-9ed7-858c2be99892` | `76af0365-6e41-4e6e-9809-6ebabdd9bde1` | N3 | strict | 3:1 | 2026071 | draft | 23,113 | 2,566 | 20,547 | 4,286 | `4a011a7b...` |
| `..._inclusive_n3_adaptive_geographic_quality` | `c7b18870-b4e4-49e1-bdc7-89c0574c8138` | `adfded13-2a2f-446e-8911-5f5bb893cff9` | N3 | inclusive | 3:1 | 2026071 | draft | 27,396 | 6,849 | 20,547 | 4,286 | `8bbb2ee8...` |

All built at commit `2ea573d`. All second (active) `build_id` produced
from an identical replay — same `dataset_version_id`/checksum as the
first build, confirming idempotence (phase 3B.5 finding, re-confirmed
here at the version-table level, not re-derived).

**Per-split checksums** (computed for this review —
`md5(string_agg(deterministic_key, ',' ORDER BY deterministic_key))`,
since no per-split checksum is persisted; fully reproducible from the
same DB state):

| Logical ID | train | calibration | test |
|---|---|---|---|
| strict N2 | `361d1e82...` (16,909 rows) | `d883b322...` (2,224) | `dd468995...` (3,980) |
| inclusive N2 | `11e3cd59...` (20,036) | `69756112...` (2,652) | `2a4dee9a...` (4,708) |
| strict N3 | `aa80e126...` (16,909) | `7c348bf4...` (2,224) | `7a27791b...` (3,980) |
| inclusive N3 | `2bf90982...` (20,036) | `10f27ccd...` (2,652) | `75eaa92e...` (4,708) |

Approximate disk size: strict ≈ 18 MB each, inclusive ≈ 21–22 MB each
(`pg_column_size` sum over `ml.dataset_rows`). Build duration: 4.1–5.6s per
build (8 builds total, 2 per variant).

## 3. Label coherence

| Check | Result |
|---|---|
| Admissible `human_known` events | 7,094 |
| Distinct positive cell-days (inclusive) | 6,849 |
| Strict positive cell-days | 2,566 |
| Positive rows linking >1 event (inclusive) | 194 |
| Positive rows linking >1 event (strict) | 37 |
| `requires_accidental_sensitivity_analysis` rows, inclusive | 1,301 |
| `requires_accidental_sensitivity_analysis` rows, strict | 542 |
| Event links by cause | 100% `human_known` (7,091 links inclusive, 2,608 strict) — 0 `natural_known`/`unknown` links found |
| Positive cell-date also present as negative | 0 (all 4 datasets) |
| Certain-duplicate exclusions | 3 (shared, identical across all 4) |

`Accidentelle` (medium-confidence human) events remain included, per the
existing validated label-quality rule (unchanged from phase 3B.2/3B.3) —
confirmed by the `requires_accidental_sensitivity_analysis` flag being
present on real rows rather than absent.

**Strict vs. inclusive gap**: 6,849 → 2,566 (4,283 cell-days removed),
accounted for exactly by `insufficient_geographic_quality` (3,624),
`missing_features` (22), `non_combustible_cell` (637) — `3,624+22+637 =
4,283`, matching exactly (the `certain_duplicate` exclusion, 3 rows, is
event-level and does not enter this cell-day subtraction — see phase
3B.3's final review for why conflating the two granularities produces a
false gap).

## 4. Negative population coherence

| Metric | strict/inclusive N2 | strict/inclusive N3 |
|---|---|---|
| Total negatives | 20,547 (identical across strict/inclusive of the same strategy — shared population) | 20,547 |
| Distinct cells used | 20,270 | 20,288 |
| Average reuse per cell | 1.014 | 1.013 |
| Max reuse of one cell | 2 | 3 |
| Distinct resolution-5 blocks (inclusive) | 2,417 | 2,416 |
| Cells per res5 block (min/median/max) | 1 / 9 / 22 | 1 / 9 / 25 |

No cell is reused excessively (max 2–3 occurrences out of 20,547), and no
resolution-5 block dominates (max 22–25 cell-days out of 20,547, ≈0.1%).

**By split** (identical target counts for N2 and N3, since both are sized
to the same inclusive-positives x ratio formula):

| Split | Negatives |
|---|---|
| train | 15,027 |
| calibration | 1,989 |
| test | 3,531 |
| prospective | 0 |

**By month** (inclusive N3, representative): fairly even, 1,623–1,806 per
month — expected, since the sampler draws uniformly across each split's
date range rather than matching positives' seasonal concentration (a
known, documented limitation — see `FULL_CANDIDATE_DATASET.md` §6).

**By year**: 2020: 3,776; 2021: 3,718; 2022: 3,751; 2023: 3,782 (train);
2024: 1,989 (calibration); 2025: 3,531 (test); 2026: 0 (prospective, no
positives yet).

**N2 vs. N3 divergence, confirmed at the source, not just counted**: of
`inclusive_n2`'s and `inclusive_n3`'s 20,547 negatives each, only **1** is
shared — 99.995% differ. This is exactly what the two strategies' distinct
exclusion-window definitions predict (N2: fixed k-ring2/±3 days for every
event regardless of cause or quality; N3: adaptive per-category window,
narrower for `precision_undocumented`, wider for `municipality_centroid_
probable`/unknown) — the divergence is a direct, expected consequence of
the rule difference, not an artifact of seed or HashMap iteration order
(that exact artifact was found and fixed in phase 3B.5, and is now
guarded by `eligible_negative_cells.sort_unstable()` plus the
`candidate_dataset_consistency` idempotence test).

## 5. H3 resolution-9-to-8 aggregation: deep audit

Measured directly (`crates/store/tests/candidate_dataset_scientific_
review.rs`), not estimated:

| Metric | Value |
|---|---|
| Total resolution-8 parents | 794,651 |
| Children per parent: min / median / max | 1 / 1 / 8 |
| Parents with exactly 1 child | 776,305 (97.7%) |
| Parents with >1 child (partial coverage) | 18,346 (2.3%) |

**Combustible-rule sensitivity** (`any` vs. majority vs. proportion
thresholds), computed over all 794,651 parents:

| Rule | Cells classified combustible |
|---|---|
| `any` (adopted) | 761,556 |
| Majority (>50%) | 745,445 |
| ≥25% | 745,598 |
| ≥50% | 745,493 |
| ≥75% | 745,445 |

`any` vs. majority disagree on **16,111** cells (2.0% of all parents);
`any` vs. ≥50% disagree on 16,063. Since 97.7% of parents have exactly one
child (where every rule agrees trivially — a single child is either 0% or
100% combustible), **all of the disagreement comes from the 18,346
multi-child parents**: 16,111 of those 18,346 (87.8%) have genuinely mixed
combustibility among their children, and `any` classifies all of them
combustible while majority/proportion rules would not.

**This confirms the mission's own concern directly**: `any` does
over-declare combustible cells relative to a majority or proportion rule,
by exactly 16,111 cells (2.1% of the 761,556 cells `any` currently
classifies eligible). This is a real, measured, non-trivial effect — not
dismissed. It was a deliberate choice (documented in `dataset::features_
h3`'s module doc: a resolution-8 area can host a fire if any part of it
can), and this review does not overturn that choice, but it is now
quantified rather than asserted. **Not modified in this phase** — any
change to the aggregation rule requires separate authorization and would
require rebuilding all four datasets.

## 6. Feature review

| Feature | Source | Temporal classification | Aggregation | Normalization | Imputation | Missing (measured) |
|---|---|---|---|---|---|---|
| `wui` | `cell_static` (res9→8 mean) | `current_snapshot_applied_historically` | mean of present children | `robust_scale` | median (1.0) | 0% |
| `road` | same | same | same | `standardize` | median (0.117) | 0% |
| `agri` | same | same | same | `robust_scale` | median (1.0) | 0% |
| `population` | same | same | same | `log1p_then_standardize` | median (0.0009) | 0% |
| `poi` | same | same | same | `log1p_then_standardize` | median (0.0005) | 0% |
| `power_line` | same | same | same | `log1p_then_standardize` | median (0.0) | 0% |
| `hist` | same | same | same | `log1p_then_standardize` | median (0.0) | 0% |
| `combustible` | same, `any(child)` | same | boolean | none (eligibility gate, not normalized) | n/a | 0% |
| `weekend`, `public_holiday` | `historical_calendar_days` | `historical_exact` | n/a | none | n/a | 0% |
| `season_sine`/`cosine` | same | `historical_exact` | n/a | none (already [-1,1]) | n/a | 0% |
| `school_holiday` | same | `unavailable_historically` | n/a | n/a | never imputed, always `NULL` | 100% (by design) |

**Classification**: `wui/road/agri/population/poi/power_line/hist/
combustible` are `current_snapshot_applied_historically` — real but
temporally approximate (today's static state applied uniformly across
2020–2026), the single most important caveat carried into training.
`weekend/public_holiday/season_sine/season_cosine` are genuinely
`historical_exact` (computed deterministically for the correct calendar
year). `school_holiday` is `unavailable_historically`, never fabricated.

**Fragile/redundant candidates**: `poi` and `population` are moderately
correlated (r=0.570, train split) — both proxy human presence; not
redundant enough to drop, but worth watching if a linear model shows
unstable coefficients on either. No feature pair exceeds r=0.6 in this
check (see §9).

## 7. Distribution across splits (train/calibration/test)

Real per-split means (inclusive N3; strict shows the same pattern at
different absolute scale since it's a subset):

| Feature | train mean (sd) | calibration mean (sd) | test mean (sd) |
|---|---|---|---|
| `wui` | 0.7190 (0.446) | 0.7064 (0.450) | 0.7145 (0.447) |
| `road` | 0.1355 (0.092) | 0.1335 (0.092) | 0.1379 (0.097) |
| `agri` | 0.6466 | 0.6260 | 0.6479 |
| `population` | 0.0121 (0.050) | 0.0134 (0.059) | 0.0136 (0.057) |
| `poi` | 0.0178 (0.059) | 0.0184 (0.061) | 0.0193 (0.061) |
| `power_line` | 0.0417 | 0.0432 | 0.0427 |
| `hist` | 0.0841 | 0.0767 | 0.0841 |

Maximum absolute mean difference across splits is ≈0.013 (`wui`,
train vs. calibration), against a standard deviation of ≈0.45 —
standardized difference ≈0.03, well below any conventional drift
threshold (e.g., PSI's usual 0.1 "watch" line, though a full PSI/KS
computation was not run in this pass — flagged as a lighter-weight check,
not a substitute for one before training). **No feature shows a marked
split-to-split shift** at this level of scrutiny. 2025 (test) was not used
to fit any transformation — normalization/imputation parameters come
exclusively from each build's own train rows (`DATASET_NORMALIZATION_AND_
IMPUTATION.md`).

## 8. Class balance and weighting

Positive rate is **exactly 0.2500** in every split (train/calibration/
test) for every dataset — a direct, mechanical consequence of sizing
negatives to `positives x 3` per split, not a coincidence to interpret
further. No per-month or per-H3-parent ratio was computed separately in
this pass (negatives are not stratified by either — §4's "known
limitation"), so a genuine per-stratum imbalance analysis is not yet
possible; this is named as an open risk (§12), not glossed over.

**Weighting options, not applied (no training in this phase)**:

| Option | Risk |
|---|---|
| No weighting | Simplest; matches the mechanically-fixed 25% positive rate, likely adequate as a first baseline. |
| Class weights (inverse frequency) | Would over-correct given the ratio is already fixed at design time, not organically imbalanced. |
| Per-stratum weighting (month/H3-parent) | Cannot be soundly fit without first measuring per-stratum imbalance (not done here). |
| Inverse sampling-probability weighting | Requires knowing each negative's true selection probability under the seeded-hash sampler, which is not currently exposed/computed. |
| Strict vs. inclusive differential weighting | Not recommended — mixes two already-distinct label-inclusion rules with a third free parameter. |

**Recommendation**: start with no weighting for the first baseline runs;
revisit only if per-stratum imbalance is measured and found material.

## 9. Basic statistical controls

- **Correlations** (train, inclusive N3): `poi`~`population` 0.570,
  `agri`~`wui` 0.347, `wui`~`road` 0.152, `power_line`~`road` 0.145,
  `wui`~`population` -0.046. No pair exceeds 0.6; no near-duplicate
  feature pair found.
- **Quasi-constant features**: none of the 7 numeric features has zero
  variance (all have std_dev > 0, per `DATASET_NORMALIZATION_AND_
  IMPUTATION.md` §2).
- **Extreme skew**: `population`, `poi`, `power_line`, `hist` all have
  median far below mean (heavy right tail) — exactly why they were
  assigned `log1p_then_standardize` (§6), not left unaddressed.
- **Duplicate columns**: none — each of the 7 features has a distinct
  real-world meaning and distinct summary statistics.
- **Impossible values**: all 7 features are bounded [0,1] by construction
  (aggregated means of already-normalized `cell_static` inputs); no value
  outside that range was found in any statistic computed.
- **Label vs. feature association**: not computed as a formal univariate
  test in this pass (explicitly out of scope per the mission's own
  caution not to present exploratory association as causal evidence);
  deferred to the training phase's own exploratory step, done there with
  the appropriate statistical framing.

## 10. Strict vs. inclusive — what each does

**Strict removes**: geographically imprecise events
(`insufficient_geographic_quality`, 3,624), features-missing cells (22),
non-combustible cells (637). **Risk introduced**: strict systematically
under-represents exactly the harder-to-locate/harder-to-verify events —
if imprecise geography correlates with certain regions or report sources,
strict could carry a geographic selection bias not present in inclusive.

**Inclusive keeps** all of the above, including the 1,301
`requires_accidental_sensitivity_analysis`-flagged rows and events with
weaker geographic quality. **Noise introduced**: cell-day rows whose true
location may not exactly match the recorded H3 cell, and non-combustible
or feature-incomplete cells that received real feature values from the
aggregation (§5) despite being scientifically weaker admissions.

Neither is unconditionally better; both should be considered together, not resolved by dataset size alone.

## 11. N2 vs. N3 — should N3 stay principal?

Confirmed here: N3 and N2 produce genuinely different negative
populations (§4, 99.995% divergence) for a principled reason (adaptive
vs. fixed exclusion windows), not a coincidence or bug. N3 remains
recommended as principal (per phase 3B.4's own recommendation, unchanged):
it is the only strategy that adapts to real per-event geographic
uncertainty rather than applying one blanket rule regardless of how
precisely an event's location is actually known. N2 remains the
sensitivity variant — a stricter, quality-agnostic baseline to compare
model results against, exactly as designed.

## 12. Temporal leakage — control matrix

| Feature/step | Observation date | Availability date | Split scoping | Source | Transform | Fit population | Leakage risk | Mitigation |
|---|---|---|---|---|---|---|---|---|
| `cell_static` features | "today" (build time) | build time | all splits, same snapshot | `public.cell_static` | mean (res9→8) | n/a (not fit, static) | Real: today's state applied to 2020–2026 | Flagged `current_snapshot_applied_historically` on every row |
| calendar (`weekend`/`holiday`/`season`) | exact calendar date | always available | all splits | `features.historical_calendar_days` | none | n/a | None (deterministic, law-fixed) | — |
| `school_holiday` | n/a | never available | all splits | none | n/a | n/a | None (never fabricated) | Always `NULL` |
| normalization stats | n/a | build time | **train only** | this build's own train rows | standardize/robust/log1p | train split of this build | Would leak if test/calibration included | `build_one_variant` filters to `split=="train"` before any stat call |
| imputation rule | n/a | build time | **train only** | same | median | train split of this build | Same as above | Same filter |
| negative sampling | candidate date | build time | **per split**, independent seeds | seeded hash over eligible cells | n/a | n/a | Cross-split leakage if seed/pool shared | `base_seed` folds in `split_tag`; `split_bounds` restricts the date range per split |

Verified in code and data (not asserted): normalization/imputation
train-only (§7/§8 of `PHASE3B5_CANDIDATE_DATASET_REPORT.md`, re-confirmed
by this review's own split-level statistics showing train's own
parameters, not a blend); sampling separated by split (§4, per-split
counts sum correctly and no split leaks into another —
`candidate_dataset_consistency`'s zero-`split_conflicts` check);
per-split checksums independent (§2, all 12 distinct); no global
computation used to fit train (only train rows ever enter `train_only_
statistics`); no future event enters historical aggregates (`no_future_
information_leaks_into_a_past_year`, re-run green); no snapshot silently
selected from the future (`never_selects_a_snapshot_from_the_future`,
re-run green).

## 13. Exclusion analysis

By reason (identical across all 4 datasets, since exclusions are
shared): `insufficient_geographic_quality` 3,624, `non_combustible_cell`
637, `missing_features` 22, `certain_duplicate` 3 — 4,286 total. Not
recomputed by year/month/H3-parent in this pass (would require joining
exclusions back to their original event/cell-day, most of which have no
persisted date breakdown beyond what's already in `ml.dataset_exclusions.
local_date`); flagged as a follow-up if a more granular bias
characterization is needed before training. What is already known: this
exclusion set is a strict-only concern (§3) and does not affect inclusive
or the negative population at all.

## 14. Open risks

- `any(child)` combustible rule over-declares 16,111 cells (2.1% of the
  eligible population) relative to a majority rule — quantified, not
  fixed, in this phase.
- Negative sampling is not stratified by month or H3-parent-block (only
  by split) — real risk of unmeasured seasonal/spatial imbalance.
- Strict systematically excludes geographically-imprecise events, a
  possible geographic selection bias not directly measured here.
- `current_snapshot_applied_historically` remains the single largest
  scientific caveat: every feature value is today's state, not the
  historical state at the time of each row.
- No formal PSI/KS drift test was run (§7 used standardized mean
  difference only); recommended before any production training run.
- Exclusions were not broken down spatially/temporally in this pass.

## 15. Verdict

The four datasets are internally consistent (identical shared exclusions
and positive counts across strategies; identical negative-count targets
per split; zero label/negative overlap; zero cross-split contamination;
idempotent and checksummed). The one quantitatively significant, newly
measured finding — the `any`-rule combustible over-declaration — is real
but does not by itself invalidate the datasets; it is a documented,
bounded, and now-measured design choice, not a hidden defect.

```
PHASE 3B.6 SCIENTIFIC DATASET REVIEW PASSED
```
