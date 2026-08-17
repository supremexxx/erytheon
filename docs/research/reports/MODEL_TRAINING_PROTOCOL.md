# ERYTHEON — Model training protocol (prepared, not executed)

Training protocol for the next phase, prepared from the phase 3B.6
scientific review. **This document authorizes nothing by itself — no
training, calibration, or scoring happens until a separate, explicit
authorization is given for the next phase.**

## 1. Datasets retained

| Role | Dataset |
|---|---|
| **Principal** | `erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality` |
| **Sensitivity (stricter labels)** | `erytheon_human_ignition_cell_day_v1_candidate_strict_n3_adaptive_geographic_quality` |
| **Sensitivity (stricter negatives)** | `erytheon_human_ignition_cell_day_v1_candidate_inclusive_n2_kring2_day3` |
| **Exploratory (both stricter)** | `erytheon_human_ignition_cell_day_v1_candidate_strict_n2_kring2_day3` |

Inclusive is the principal variant: it retains the full admissible
positive population (6,849 cell-days vs. strict's 2,566), giving more
training signal, at the cost of the label-quality noise strict removes
(`PHASE3B6_SCIENTIFIC_DATASET_REVIEW.md` §10) — an explicit, argued
tradeoff, not a default choice. N3 is principal over N2 per §11 of that
review: it is the only strategy whose exclusion window adapts to real,
per-event geographic uncertainty. Strict-N3 and inclusive-N2 each isolate
one axis of the design (label strictness vs. negative-window strictness)
for sensitivity comparison against the principal run.

**Negative strategy**: N3 (adaptive), ratio 3:1 (per-split exact, §8 of
the review — 25.00% positive rate everywhere). **Weighting**: none for
the first baseline run (§8 of the review).

**Features to keep**: `wui`, `road`, `agri`, `population`, `poi`,
`power_line`, `hist`, `combustible`, `weekend`, `public_holiday`,
`season_sine`, `season_cosine` — all real, all with a documented temporal
classification. **Features to exclude**: `school_holiday` (100% missing,
never fabricated — must not be included as a feature until a verified
source exists). **Features to monitor, not exclude**: `poi`/`population`
(moderate correlation, r=0.570); all seven numeric features generally,
given the single largest caveat is `current_snapshot_applied_
historically` (today's static state applied across 2020–2026) — this
affects every one of them equally and is not specific to any single
feature.

**Combustible treatment**: `any(child)` aggregation is used as-is (not
changed in phase 3B.6); its measured 16,111-cell over-declaration
(2.1% of the eligible population) should be tracked as a known model
input imprecision, not silently ignored — see §7 (sensitivity analysis)
below.

**Hard/difficult cases**: `requires_accidental_sensitivity_analysis`-
flagged rows (1,301 inclusive / 542 strict) remain included as positives
per the existing validated label-quality rule; recommend a stratified
metric breakdown (with vs. without these rows) rather than exclusion.

## 2. Baselines (to run before any learned model)

- **Global frequency**: predict the dataset's overall positive rate
  (0.25) for every row — the naive floor any learned model must beat.
- **Frequency by month**: predict each month's empirical positive rate.
- **Frequency by H3-parent**: predict each resolution-5 block's empirical
  positive rate (only where enough samples exist per block — most blocks
  have a median of ~9 cell-days, too few for a stable per-block rate
  alone; use as a diagnostic, not a serious baseline).
- **Regularized logistic regression**: L2-penalized, on the 7 numeric +
  4 calendar features, standardized/scaled per §6 of the scientific
  review.
- **Current operational v1 model**: kept as the operational benchmark it
  already is — a relative-propensity ranking tool, not a calibrated
  probability source. Its scores are not comparable 1:1 to a model trained
  on this new label/feature construction without separate reconciliation.

## 3. Candidate models (short list, not trained)

- Regularized logistic regression (baseline and candidate both — a
  well-calibrated linear model is a legitimate final candidate, not only
  a floor).
- Gradient boosting (tabular, e.g. a standard boosted-tree implementation)
  — likely candidate given the feature set's non-linear correlations
  (§9 of the review) and mixed scales.
- Random forest — only if gradient boosting's variance/overfitting
  profile on this dataset's size (23–27k rows) turns out to need a more
  bagging-oriented alternative; not adopted by default.
- Other tabular model (e.g. a shallow neural net) — only if the above
  three underperform materially and a specific justification is written
  at that time; not a default candidate.

No training run for any of these happens in this phase.

## 4. Validation

- **Splits**: `train 2020-2023`, `calibration 2024`, `test 2025` — fixed,
  unchanged. `2026` (`prospective`) is reserved and must **not** be used
  for any fitting, tuning, or reporting of held-out performance until it
  has its own positives (currently zero, `PHASE3B6_SCIENTIFIC_DATASET_
  REVIEW.md` §4) — using it earlier would silently redefine "prospective"
  as another test set.
- **2025 (test) must not be used to adjust any transformation** —
  normalization/imputation parameters are fixed at train-time only
  (already enforced structurally, §12 of the review); this rule extends
  to model hyperparameters and feature engineering decisions during the
  training phase, not just to the dataset-construction code.
- **Spatial validation**: no versioned department/region reference exists
  (phase 3B.2/3B.4 finding, unchanged); use resolution-5 H3-parent blocks
  as the practical substitute for a spatial holdout (e.g.,
  leave-some-blocks-out), given §4 of the review already computed their
  distribution (2,416–2,417 distinct blocks, median 9 cell-days each).
- **Seed repetition**: repeat training with at least 2–3 different seeds
  for any model whose selection will be defended on a performance margin
  narrower than what a single seed's variance could plausibly produce;
  not required for a first exploratory pass.

## 5. Metrics

- ROC-AUC, average precision (PR-AUC) — primary discrimination metrics
  given the fixed, non-organic 25% positive rate.
- Log loss, Brier score — calibration-sensitive, not just ranking.
- Calibration curve (reliability diagram) and Expected Calibration Error
  (ECE) — required before any probability is presented as absolute, per
  the mission's own repeated caution and this project's standing rule
  that the active model is a relative propensity tool, not a calibrated
  probability, until proven otherwise.
- Recall in the highest-risk cells (e.g., top decile by predicted score).
- Precision@k for a few operationally meaningful k values.
- Lift over the frequency baselines (§2).
- Spatial stability: metric variance across resolution-5 blocks.
- Temporal stability: metric variance across train/calibration/test and,
  separately, across repeated seeds.

**No absolute probability may be claimed from this protocol's output
without calibration curve + ECE evidence presented alongside it** — this
is a hard requirement carried from the existing project standard, not a
new one invented for this document.

## 6. Stopping rules and promotion criteria (proposed, for the next
   phase to apply — not applied here)

- A candidate model may proceed to calibration only if it beats the
  best frequency baseline (§2) on both ROC-AUC and average precision on
  the **calibration** split (never test).
- A candidate may proceed to test-split evaluation only once, after
  calibration is fixed — no iterating against test results.
- A model is not "promotable" without: (a) a calibration curve/ECE
  computed on calibration, not train; (b) at least one spatial-stability
  check (resolution-5 block variance); (c) explicit reporting of the
  `current_snapshot_applied_historically` and `any(child)`-combustible
  caveats alongside any result, not only in a separate document.
- No model is promoted to operational status from this protocol alone —
  promotion requires a separate, explicit authorization referencing this
  document's criteria having been met, plus whatever additional review
  the next phase's own mission specifies.

---

```
PHASE 3B.6 SCIENTIFIC DATASET REVIEW PASSED
MODEL TRAINING PROTOCOL READY FOR REVIEW
NO MODEL TRAINING
NO PRODUCTION DEPLOYMENT
NO PUSH
```
