# Phase 3B.7 — Model Experiments

Reproducible experimental training, calibration, and comparison of
candidate ignition-risk models. Run via:

```
pyrorisk run-model-experiments --seed <i64>
```

Never touches the active v1 model, the serving table, `crates/api`,
FIRMS, or FWI. All artifacts are written to `/tmp/erytheon-experiments-3b7/`
inside the ephemeral build container, never a production volume.

Official run reported here: `seed=2026071`, `git_commit=ff84fbd`.

## Datasets exercised

| Role | Logical ID |
|---|---|
| `principal` | `erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality` |
| `sensitivity_quality` | `erytheon_human_ignition_cell_day_v1_candidate_strict_n3_adaptive_geographic_quality` |
| `sensitivity_negative_window` | `erytheon_human_ignition_cell_day_v1_candidate_inclusive_n2_kring2_day3` |

**`strict_n2` (the "exploratoire" fourth variant) was deliberately not
run.** The mission (§13) explicitly permits limiting it to "the best
architecture found," but at the time this pipeline was written the best
architecture (GBM, `n_trees=50, max_depth=3, lr=0.1`) had not yet been
identified — running it would have required the exact same
grid-search cost as the other three. This is an explicit, documented
scoping decision, not a silent omission: `strict_n2` remains untested
by this phase and should be the first addition in any follow-up pass,
using the already-identified best hyperparameters directly rather than
repeating the search.

## Protocol (mission §20, followed in this order)

1. Baselines evaluated on train-internal data.
2. Architectures/hyperparameters chosen using only `train 2020-2023`
   internal progressive-temporal validation (never 2024/2025).
3. Hyperparameters frozen (printed to stdout as `3b7_hyperparam_selection`
   before the final model is fit).
4. Final model trained on the full `train 2020-2023`.
5. Calibrated on `calibration 2024` only.
6. Model + calibrator frozen.
7. Evaluated exactly once on `test 2025`.
8. Compared against the pre-registered `PromotionCriteria` (printed
   to stdout as `3b7_promotion_criteria_registered` *before* any
   dataset is touched).
9. Recommendation produced (see `PHASE3B7_MODEL_CANDIDATE_REPORT.md`).

## Baselines (mission §6)

All four are fit **only on train** and applied unchanged to
calibration/test, each with an explicit fallback for anything unseen
in train:

- **`constant`**: train-wide positive rate. ROC-AUC is 0.5 by
  definition (a constant score carries no ranking information).
- **`seasonal`**: train-only monthly positive frequency. Falls back to
  the constant rate for a month absent from train (cannot happen with
  4 full train years, kept explicit anyway).
- **`spatial`**: train-only H3 resolution-5-parent positive frequency.
  Falls back to the constant rate for a parent cell never observed in
  train.
- **`spatio_seasonal`**: train-only (H3 resolution-5-parent, month)
  positive frequency, but only trusted when the cell has at least
  `MIN_CELL_COUNT = 10` train rows (a deliberate smoothing threshold to
  avoid single-observation rates); otherwise falls back to the
  parent-only spatial rate, and further to the constant rate if the
  parent itself is unseen.

The **active v1 model was not evaluated under this protocol.** v1 uses
a materially different feature vector (`risk::human_feature_vector`,
via `crates/engine/src/human_model.rs`) and its own sampling/training
pipeline, built before the candidate-dataset work of phases 3B.3–3B.6.
Running v1 "as-is" against this pipeline's 12-feature vector would not
be a faithful comparison — it would silently substitute a different
model. Faithfully retraining v1 under the *same* feature definition
would itself be a retraining of v1, which this phase is expressly
forbidden from doing. This gap is carried into
`PHASE3B7_MODEL_CANDIDATE_REPORT.md`'s promotion decision as an
honestly-documented limitation: `min_average_precision_gain_over_v1`
cannot be evaluated this phase and is treated as unmet rather than
assumed satisfied.

## Candidate models (mission §7)

- **M1 — logistic regression**: from-scratch batch gradient descent
  with L2 regularization, grid `{0.001, 0.01, 0.1, 1.0}`.
- **M2 — gradient boosting**: from-scratch shallow regression trees
  (decile-threshold candidate splits) fit to logistic-loss
  pseudo-residuals, grid over `(n_trees, max_depth, learning_rate)` ∈
  `{(50,2,0.1), (50,3,0.1), (100,2,0.05), (100,3,0.05)}`.
- No random forest (not justified in phase 3B.6) and no neural network
  (out of scope per mission §7).

Both are custom, dependency-free implementations — no new ML crate was
added, consistent with the ephemeral container's 2 CPU / 4 GiB budget.

## Hyperparameter selection (mission §8)

- **Logistic**: progressive validation, 3 folds — fit on years
  `< 2021/2022/2023`, validate on `2021/2022/2023` respectively.
  Average internal AUC across folds picks `l2`.
- **GBM** (more expensive): 2 folds only — fit on `< 2022/2023`,
  validate on `2022/2023`. A fold is skipped if its fit set has fewer
  than 100 rows.
- **2024 and 2025 are never read during this step.** Enforced by
  construction (the grids only ever see `train_years`, itself already
  filtered to the `train` split before this loop runs).

## Results (test 2025, seed 2026071)

| Dataset | Model | ROC-AUC | AP | Brier | ECE | log loss | lift@10% |
|---|---|---|---|---|---|---|---|
| principal | logistic (raw) | 0.9744 | 0.9283 | 0.0596 | 0.0472 | 0.2063 | 3.92 |
| principal | GBM (raw) | 0.9819 | 0.9345 | 0.0668 | 0.1360 | 0.2631 | 3.91 |
| principal | GBM (isotonic) | 0.9764 | 0.9308 | **0.0460** | **0.0096** | 0.2207 | 3.91 |
| sensitivity_quality | logistic (raw) | 0.9586 | 0.7376 | 0.0548 | 0.0411 | 0.1904 | 6.37 |
| sensitivity_quality | GBM (isotonic) | 0.9691 | 0.7570 | 0.0459 | 0.0172 | 0.1509 | 6.44 |
| sensitivity_negative_window | logistic (raw) | 0.9740 | 0.9266 | 0.0597 | 0.0493 | 0.2086 | 3.92 |
| sensitivity_negative_window | GBM (isotonic) | 0.9738 | 0.9307 | 0.0488 | 0.0236 | 0.2523 | 3.92 |

Full metrics (all 5 model/calibration variants × 3 datasets, plus
per-dataset baselines) are in the `{role}_report.json` artifacts;
see `PHASE3B7_MODEL_CANDIDATE_REPORT.md` for sizes/checksums and
`MODEL_CALIBRATION_REPORT.md` for the calibration-specific detail.

Baseline results on the `principal` test split, for context:

| Baseline | ROC-AUC | AP |
|---|---|---|
| constant | 0.500 | 0.249 |
| seasonal | 0.684 | 0.398 |
| spatial | 0.852 | 0.696 |
| spatio_seasonal | 0.852 | 0.695 |

All candidate models clear every baseline by a wide margin on ranking
quality; the spatio-seasonal baseline does not meaningfully improve on
the pure-spatial one, suggesting most of the spatial baseline's signal
already captures the calendar effect indirectly (fire-prone parents
tend to also be the parents most active in the fire season).

## Uncertainty (mission §11) — `principal`, GBM+isotonic, 200-round
block bootstrap by unique test date (not naive per-row resampling,
since same-day rows are spatially/temporally correlated):

| Metric | 95% CI |
|---|---|
| ROC-AUC | [0.972, 0.980] |
| Average precision | [0.913, 0.945] |
| Brier score | [0.042, 0.050] |

## Spatial validation (mission §12) — 5-fold, grouped by H3
resolution-5 parent, within train, logistic architecture (chosen to
bound cost):

| Fold | AUC |
|---|---|
| 0 | 0.9745 |
| 1 | 0.9714 |
| 2 | 0.9760 |
| 3 | 0.9779 |
| 4 | 0.9708 |

Mean 0.9741, variance 7.14e-06 — stable across spatial folds, no
failing or over-represented zone detected at this granularity.

## Weighting comparison (mission §15) — `principal`, calibration split
only (test never touched twice):

| | ROC-AUC | AP | Brier | ECE |
|---|---|---|---|---|
| unweighted | 0.9696 | 0.9111 | 0.0682 | 0.0591 |
| class-weighted (×3.0) | 0.9698 | 0.9109 | 0.0674 | 0.0821 |

Class weighting has negligible effect on ranking (AUC/AP essentially
unchanged) but visibly worsens calibration (ECE 0.059 → 0.082) —
consistent with the mission's own warning (§15) not to mix class
weights and calibration without documenting the interaction. No
weighting variant is recommended for promotion.

## Feature importance (mission §16) — `principal`

| Feature | Logistic coefficient | GBM split count |
|---|---|---|
| hist | +3.781 | 187 |
| combustible | −0.994 | 0 |
| agri | −1.141 | 17 |
| season_cosine | −0.577 | 23 |
| poi | +0.375 | 6 |
| road | −0.231 | 105 |
| population | +0.199 | 1 |
| wui | −0.182 | 10 |
| season_sine | +0.075 | 1 |
| weekend | +0.035 | 0 |
| public_holiday | −0.031 | 0 |
| power_line | +0.020 | 0 |

`hist` (historical fire density) dominates both models. `road` has a
large GBM split count (105) but a much smaller, negative logistic
coefficient — an **unstable** feature whose ranking-relevant signal
(GBM) does not translate into a simple linear direction (logistic),
plausibly because it interacts non-linearly with `hist`/`agri`. The
logistic `combustible` coefficient is **negative** (−0.99) despite
`combustible` being an eligibility gate expected to raise risk — most
likely explained by near-total prevalence of `combustible=1` among
eligible cells (per phase 3B.6, `any(child)` classifies 761,556/794,651
= 96% of parents combustible), leaving little variance for the
coefficient to capture cleanly; GBM assigns it 0 splits for the same
reason. Neither model's importance should be read causally (mission
§16) — both are candidate-selection tools, not an explanation of fire
ignition mechanics. All 12 features carry the
`current_snapshot_applied_historically` caveat already documented in
`PHASE3B6_SCIENTIFIC_DATASET_REVIEW.md` except the 4 calendar features.

## Combustibility (H3 9→8) sensitivity (mission §14)

The eligible-cell distributional analysis (`any(child)` vs. majority
vs. proportion thresholds ≥25/50/75%) was already computed in phase
3B.6 (`PHASE3B6_SCIENTIFIC_DATASET_REVIEW.md` §5): `any` classifies
761,556 of 794,651 parents combustible, over-declaring 16,111 cells
(2.1%) relative to majority/≥50%, entirely from the 18,346 multi-child
parents. **This phase does not re-derive per-rule positives-retained/
negatives-available counts or re-run model training under each rule**
— doing so would require rebuilding the training feature vector under
each of the 5 rules, a materially larger data-engineering task than fit
this phase's ephemeral-container budget. This is an explicit,
documented gap, not a silent one: any future sensitivity pass should
build on phase 3B.6's cell-level counts by joining them against
`ml.dataset_rows` to get positives/negatives-per-rule, then optionally
retrain only the best architecture (GBM) under each rule.

## Leakage controls (mission §17)

`assert_split_dates_in_range` hard-fails (returns `Err`, not a log
line) if any row's calendar date falls outside its claimed split's
real bounds — covers "any 2024-2026 row used while fitting", "any 2025
row used while selecting hyperparameters/calibrator" by construction,
since the grids and calibration step only ever read from
pre-filtered, date-bounds-checked `train`/`calibration` slices.
`Store::dataset_rows_fingerprint` is computed before and after each
per-dataset run and asserted equal, catching any modification of the
dataset during the experiment. See `crates/engine/src/model_experiments.rs`
`#[cfg(test)] mod tests` for the unit-level version of these checks
(pure functions, no DB required).
