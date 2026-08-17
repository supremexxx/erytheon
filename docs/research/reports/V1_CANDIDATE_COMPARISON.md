# Phase 3B.8 — V1 vs. Candidate Comparison

A faithful, paired comparison between the active v1 model's learned
human-ignition component and the frozen phase 3B.7 GBM+isotonic
candidate, on exactly the same 2025 rows. Run via:

```
pyrorisk run-v1-comparison --seed <i64>
```

Official run: `seed=2026071`, code commit `2eec181`, v1 artifact
`human_model_versions.id=1` (trained `2026-07-24T20:55:48Z`, read-only
— never retrained, never modified). Never touches serving, API, or
production data; artifact written to
`/tmp/erytheon-experiments-3b8/v1_candidate_comparison_report.json`
inside the ephemeral container (6,016 bytes).

## 1. v1 audit

`crates/engine/src/human_model.rs` + `crates/risk/src/lib.rs`:

- **Architecture**: `risk::LearnedHumanModel` — a standardized logistic
  regression (intercept + 11 weights + per-feature means/scales),
  fit by `human_model.rs::fit_logistic` via plain batch gradient
  descent (500 iterations, learning rate 0.12/√(1+t/100), L2=0.01).
- **Features** (`risk::HUMAN_FEATURE_NAMES`, fixed order): `wui`,
  `road`, `agri`, `population`, `poi`, `power_line`, `weekend`,
  `school_holiday`, `public_holiday`, `season_sine`, `season_cosine`.
  **11 features — notably no `hist` and no `combustible`** as direct
  model inputs (unlike the phase 3B.7 candidate's 12, which include
  both). `combustible` gates *training/serving eligibility* in v1
  (only combustible cells are ever sampled), it is not a feature the
  logistic regression itself sees.
- **Transformation**: per-feature standardization `(raw - mean) /
  scale`, with `mean`/`scale` computed once at training time from the
  training sample itself and frozen into the artifact — no separate
  imputation rule (v1's training never encounters missing values,
  since `sample()` in `human_model.rs` requires `combustible == true`
  and reads directly from `cell_static`/calendar tables).
- **Output**: `LearnedHumanModel::predict` returns a **sigmoid** —
  explicitly documented in v1's own training code
  (`ModelMetrics.interpretation = "relative_human_ignition_propensity_
  not_absolute_probability"`) as a *relative* propensity, not a
  validated absolute probability. No separate calibration step exists
  for v1 (no Platt/isotonic layer).
- **Negative sampling**: uniform random combustible cell-days
  (`Store::sample_combustible_cells`), not the phase 3B.4/3B.5
  spatio-temporal-stratified negative design the candidate datasets
  use — a real, structural difference in what "negative" means between
  the two models, separate from the feature-vector difference.
- **Serving**: `Store::active_human_model()` reads the single active
  row from `human_model_versions` (id, trained_at, artifact JSON,
  metrics JSON) — this phase reads it exactly once, read-only, and
  never calls `activate_human_model` or any write path.
- **Full fused score** (`HeuristicV1::score`) multiplies `human` (the
  learned component above) with `physical` (FWI-derived) and zeroes
  the result for non-combustible cells. **This phase deliberately
  compares the learned human component alone** — the candidate model
  has no FWI feature at all, so comparing the fused score would
  silently confound a weather signal the candidate was never asked to
  predict. `predict()` itself does not depend on `combustible` or FWI,
  so scoring it directly on candidate rows (regardless of their
  combustible flag) is the correct, faithful, like-for-like object.

## 2. Feature compatibility

| v1 feature | Candidate dataset source | Fidelity |
|---|---|---|
| `wui`, `road`, `agri`, `population`, `poi`, `power_line` | `ml.dataset_rows.features` (same underlying `cell_static` snapshot) | exact_match |
| `weekend`, `public_holiday`, `season_sine`, `season_cosine` | same, `historical_exact` | exact_match |
| `school_holiday` | absent (`unavailable_historically`, phase 3B.6 §6, 100% of rows) | approximate_bridge |

**`school_holiday` bridge**: bridged as `false` for every row — not a
new assumption invented for this comparison, but the *exact same*
`COALESCE(school_holiday, FALSE)` convention v1's own live serving
query (`Store::risk_inputs`) already applies whenever the calendar
table has no verified value. 10 of 11 features are exact; 1 of 11 is
approximated using v1's own established fallback. Approach A (score
the real v1 artifact) was fully usable — Approach B (a separate
feature bridge document) was not needed.

## 3. Common population (mission §5)

Measured, not assumed, over the principal dataset's `test 2025` split
(4,708 rows):

| Category | Rows | Positives | Negatives |
|---|---|---|---|
| `v1_comparable` | 4,708 (100%) | 1,177 | 3,531 |
| `v1_missing_features` | 0 | 0 | 0 |

Every row in the candidate test split is directly scoreable by v1 —
the population intersection is the full principal test set, no
comparability bias to control for.

## 4. Candidate reproduction (mission §8)

The frozen phase 3B.7 candidate (GBM `n_trees=50, max_depth=3,
learning_rate=0.1`, isotonic calibrated on 2024) was retrained from the
manifest with **no new hyperparameter search**:

| Metric | Phase 3B.7 report | Replayed here |
|---|---|---|
| ROC-AUC | 0.9764 | 0.976385 |
| Average precision | 0.9308 | 0.930843 |
| Brier | 0.0460 | 0.045997 |
| ECE | 0.0096 | 0.009558 |

GBM training and isotonic fitting are deterministic given fixed data
(no RNG involved), so this replay matches the original run to full
precision beyond the 4-decimal rounding used in
`PHASE3B7_MODEL_CANDIDATE_REPORT.md`'s tables — confirmed reproducible.

## 5. Paired metrics on the shared population

| Metric | v1 (learned human component) | Candidate (GBM+isotonic) |
|---|---|---|
| ROC-AUC | 0.7836 | 0.9764 |
| Average precision | 0.5840 | 0.9308 |
| Brier | 0.1493 | 0.0460 |
| log loss | 0.4614 | 0.2207 |
| ECE | 0.0364 | 0.0096 |
| lift@1% | 3.75 | 3.92 |
| lift@5% | 3.27 | 3.90 |
| lift@10% | 2.86 | 3.91 |

v1's Brier/log loss/ECE are reported descriptively only — v1's own
training code documents its output as a relative propensity, not a
validated probability, so these are not claimed as validated
calibration error, per mission §9.

## 6. Paired difference and bootstrap (mission §10–11)

Average-precision difference (candidate − v1): **+0.3473**, 200-round
block bootstrap by unique test date (same resampled dates used for
both models each round — a true paired bootstrap, not two independent
ones): **95% CI [+0.3157, +0.3852]**. ROC-AUC difference: +0.2038, 95%
CI [+0.1787, +0.2091]. Both intervals sit entirely above zero — the
candidate's advantage is not a bootstrap artifact.

## 7. Operational top-k comparison (mission §11)

| Level | v1 positives captured | Candidate positives captured | Captured by both | Only v1 | Only candidate |
|---|---|---|---|---|---|
| top 1% | 45 | 47 | 2 | 43 | 45 |
| top 5% | 193 | 230 | 59 | 134 | 171 |
| top 10% | 337 | 460 | 178 | 159 | 282 |

The candidate captures more true positives at every operational
threshold, and the two models substantially disagree on *which* rows
rank highest (low both-captured overlap relative to each model's own
top-k) — consistent with them using materially different feature sets
and sampling designs, not just different noise realizations of the
same signal.

## 8. Disagreement analysis (mission §12)

| | v1-high / candidate-low (n=388) | v1-low / candidate-high (n=299) |
|---|---|---|
| positive rate | 1.0% | 34.4% |
| mean `wui` | 0.853 | 0.421 |
| mean `hist` | 0.0009 | 0.120 |
| mean `agri` | 0.466 | 0.703 |
| mean `road` | 0.179 | 0.074 |

The v1-high/candidate-low bucket is almost entirely negative (1.0%
positive rate) and dominated by high `wui` with near-zero historical
ignition density — v1's learned component appears to weight WUI/road
proximity heavily on its own (consistent with its coefficient
structure) without `hist` available to temper it (v1 does not use
`hist` as a feature at all). The v1-low/candidate-high bucket has a
34% positive rate and much higher `hist` — the candidate, having
`hist` as a direct feature, correctly ranks these cells higher; v1,
lacking that feature, systematically underranks them. This is a
plausible, feature-availability-driven explanation, not a causal claim
about ignition mechanics.

## 9. Combustibility sensitivity (mission §16)

Analytic join against `cell_static` (no dataset rebuild), applied to
the same 4,708-row comparable population:

| Rule | Rows retained | Rows excluded | Positives retained | Negatives retained |
|---|---|---|---|---|
| `any(child)` (current) | 4,708 | 0 | 1,177 | 3,531 |
| majority (>50%) | 4,369 | 339 | 909 | 3,460 |
| proportion ≥50% | 4,369 | 339 | 909 | 3,460 |
| proportion ≥75% | 4,369 | 339 | 909 | 3,460 |

Majority/≥50%/≥75% produce identical counts here (no test-population
cell happens to sit at an exact 50% child-combustibility tie). Under
the stricter rules, 339 rows (7.2%) would be excluded, disproportionately
positives (268 of 1,177 positives, 22.8%, vs. 71 of 3,531 negatives,
2.0%) — **a real, measured signal that combustible-uncertain cells are
more likely to be true positives**, worth a dedicated follow-up, but
not large enough to change this phase's population-fidelity conclusion
(92.8% of rows are unaffected by the rule choice). No model was
retrained under alternate rules — eligibility only, per the mission's
own cost-bounding allowance.

## 10. Strict N2 (mission §17)

Frozen hyperparameters only, no new search, on
`erytheon_human_ignition_cell_day_v1_candidate_strict_n2_kring2_day3`:

| Metric | Value |
|---|---|
| ROC-AUC | 0.9479 |
| Average precision | 0.7497 |
| Brier | 0.0462 |
| ECE | 0.0200 |
| lift@10% | 6.48 |

Consistent with the principal/strict-N3 pattern: strong ranking
(ROC-AUC comparable to the other datasets), lower AP reflecting a
lower positive rate (11.3%, matching strict N3's known rate), well
calibrated. **The general conclusion does not depend on this
dataset's omission** — it was the one gap flagged in phase 3B.7 and
is now closed.

## 11. Reproducibility manifest

```json
{
  "seed": 2026071,
  "v1_artifact_id": 1,
  "v1_trained_at": "2026-07-24T20:55:48.823149+00:00",
  "candidate_gbm_hyperparameters": {"n_trees": 50, "max_depth": 3, "learning_rate": 0.1},
  "principal_dataset_row_fingerprint_before": "<matches _after, checked equal>",
  "ap_diff_candidate_minus_v1": 0.34725860285809995,
  "ap_diff_95pct_ci": [0.31568356454924384, 0.38519512021580904]
}
```

Dataset fingerprint checked identical before and after the comparison
(no in-place modification). A replay confirmed identical hyperparameters
and metrics within the documented 4-decimal-rounding tolerance (§4).
