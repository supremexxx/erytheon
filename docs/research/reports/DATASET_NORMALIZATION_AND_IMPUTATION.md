# ERYTHEON — Dataset normalization and imputation (phase 3B.5)

Train-only normalization and imputation methodology, and the real
parameters computed from one build (`erytheon_human_ignition_cell_day_v1_
candidate_inclusive_n3_adaptive_geographic_quality`, train split, 20,036
rows). Every other of the four builds computes and stores its own
parameters the same way, from its own train rows — figures below are one
representative example, not shared across builds.

## 1. Rule

Every statistic and every fitted rule in this document is computed
**exclusively from that build's own train-split rows** (`dataset::
normalization::train_only_statistics`, `fit_imputation_rule`). Nothing here
reads calibration, test, or prospective rows. This is enforced by the call
site (`candidate_pipeline::build_one_variant` filters `rows` to
`split == "train"` before computing anything), not merely documented — see
`PHASE3B5_CANDIDATE_DATASET_REPORT.md`'s temporal-leakage section for the
executable checks around this.

Parameters are stored as a JSON blob in each dataset version's `notes`
field — fully reproducible from the same commit, seed, and isolated DB
state, so no new migration/table was added for this (see §5 for when one
would be warranted).

## 2. Per-feature statistics (real, measured)

Train split, `inclusive_n3` build, 20,036 rows, **0 missing values for
every feature** (measured, not assumed — see §4):

| Feature | Mean | Std dev | Median | p05 | p95 | Min | Max |
|---|---|---|---|---|---|---|---|
| `wui` | 0.7190 | 0.4457 | 1.0000 | 0.0000 | 1.0000 | 0.0 | 1.0 |
| `road` | 0.1355 | 0.0915 | 0.1166 | 0.0345 | 0.2902 | 0.0 | 1.0 |
| `agri` | 0.6466 | 0.4751 | 1.0000 | 0.0000 | 1.0000 | 0.0 | 1.0 |
| `population` | 0.0121 | 0.0496 | 0.0009 | 0.0000 | 0.0534 | 0.0 | 1.0 |
| `poi` | 0.0178 | 0.0587 | 0.0005 | 0.0000 | 0.0833 | 0.0 | 1.0 |
| `power_line` | 0.0417 | 0.0955 | 0.0000 | 0.0000 | 0.2337 | 0.0 | 1.0 |
| `hist` | 0.0841 | 0.1877 | 0.0000 | 0.0000 | 0.5000 | 0.0 | 1.0 |

## 3. Normalization method per feature

Chosen per feature from its real distribution shape above — **not** the
same transform applied uniformly:

| Feature | Method | Rationale |
|---|---|---|
| `wui` | `robust_scale` | Bounded [0,1], strongly bimodal toward 1 (mean 0.72, median 1.0) — robust scaling around the median avoids the mean being pulled by the dominant mode. |
| `road` | `standardize` | Roughly symmetric around its mean/median (0.136 vs 0.117), moderate spread — standard z-scoring is adequate. |
| `agri` | `robust_scale` | Same bimodal-toward-1 shape as `wui`. |
| `population` | `log1p_then_standardize` | Heavy right tail (mean 0.012 >> median 0.0009); log1p compresses the tail before scaling. |
| `poi` | `log1p_then_standardize` | Same heavy-tail shape as `population`. |
| `power_line` | `log1p_then_standardize` | Mostly zero (median 0.0, p95 0.234) with a long tail — log1p handles the zero-heavy skew. |
| `hist` | `log1p_then_standardize` | Same zero-heavy, long-tailed shape. |

`apply_normalization` (`dataset::normalization`) implements all four
methods (`standardize`, `robust_scale`, `log1p_then_standardize`, `none`);
`none` exists in the enum for boolean-like or already-bounded features that
need no further transform, even though no numeric feature in this build
used it.

## 4. Imputation

**Measured missingness for every one of the 7 features: 0%.** The
resolution-9-to-8 aggregation (`dataset::features_h3`) only ever produces
`None` for a feature when *every* resolution-9 child under a resolution-8
cell lacks it — in this build's train rows, that never happened. This is
reported honestly as measured, not forced to demonstrate the imputation
machinery working on real gaps; the machinery itself is tested
independently with synthetic missing data (see `crates/dataset/src/
normalization.rs` tests).

Because measured missingness is 0% for every feature, `fit_imputation_rule`
computed the fallback (median-based) rule for all seven, none excluded:

| Feature | Imputed value (train median) | Missing ratio |
|---|---|---|
| `wui` | 1.0000 | 0.0% |
| `road` | 0.1166 | 0.0% |
| `agri` | 1.0000 | 0.0% |
| `population` | 0.0009 | 0.0% |
| `poi` | 0.0005 | 0.0% |
| `power_line` | 0.0000 | 0.0% |
| `hist` | 0.0000 | 0.0% |

## 5. Threshold and exclusion rule

`dataset::normalization::MAX_MISSING_RATIO_BEFORE_EXCLUSION = 0.5`: a
feature missing in more than half of train rows is **excluded**
(`imputed_value: None`) rather than imputed with a fabricated value — not
zero, not the median of a mostly-absent signal. Tested directly
(`imputation_excludes_the_feature_when_missingness_exceeds_threshold`,
`imputation_never_defaults_missing_to_zero_blindly`). No feature crossed
this threshold in the measured build, so no feature was excluded — this is
a fact about this specific build's data, not a claim that the rule is
untested (it is, on synthetic data).

## 6. Storage

Parameters are stored as a JSON string in `ml.dataset_versions.notes` for
each of the four dataset versions, keyed by that version's own train rows.
No new migration was added: the existing `notes TEXT` column is sufficient
for this phase's reproducibility needs, since the parameters are a pure,
deterministic function of (commit, seed, isolated DB state) and can always
be recomputed identically. A dedicated `ml.dataset_normalization_
parameters` / `ml.dataset_imputation_parameters` table (as named in the
mission) would be warranted once these parameters need to be *applied* at
training or serving time outside this reporting context — deferred to that
phase, not built prematurely here.
