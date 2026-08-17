# Phase 3B.7 — Calibration Report

Scope: how raw model scores from `MODEL_EXPERIMENTS.md`'s M1
(logistic) and M2 (GBM) were calibrated, and whether the resulting
probabilities are trustworthy as *relative* risk within this
experimental exercise — **not** a claim that any of these are, or are
about to become, the operational v1 score.

## Method

Strict order, enforced by the code path in
`run_one_dataset_experiment` (`crates/engine/src/model_experiments.rs`):

1. M1/M2 architecture and hyperparameters are frozen using only
   internal `train 2020-2023` progressive validation (`MODEL_EXPERIMENTS.md`
   §"Hyperparameter selection").
2. Both models are retrained on the full `train 2020-2023`.
3. Both are scored on `calibration 2024` — this is the *only* data any
   calibrator sees.
4. Three variants are compared per model: **raw** (no calibration),
   **Platt scaling** (a 1-feature logistic regression of label on raw
   score, fit via the same `fit_logistic` used for M1, `l2=0`
   implicitly since it's a fresh 1-D fit), and — for GBM only —
   **isotonic regression** via the Pool-Adjacent-Violators (PAV)
   algorithm.
5. Isotonic is only attempted when the calibration split has ≥200 rows
   (all three datasets clear this: 2652/2224/2652 calibration rows).
   Below that threshold isotonic would overfit to noise in the
   calibration data — this experiment never had to exercise that guard,
   but it exists and is unit-tested
   (`model_experiments::tests::isotonic_calibration_is_non_decreasing`).
6. Model + calibrator are frozen (no further fitting) and scored exactly
   once on `test 2025`.
7. **The calibration method was never chosen using test 2025** — the
   choice of "isotonic is the primary GBM calibrator" was made by
   design before any test result was read, not by comparing calibrated
   test metrics after the fact.

## Why isotonic over Platt for GBM

Raw GBM scores are poorly calibrated out of the box (ECE up to 0.14 on
`principal` test) — expected, since gradient-boosted log-odds are not
inherently calibrated probabilities even when ranking is excellent
(ROC-AUC 0.98). Platt scaling barely helps (ECE actually *worsens*
slightly on `principal`, 0.136 → 0.152) because Platt assumes a
sigmoid-shaped miscalibration, which does not match this model's
error pattern. Isotonic, being non-parametric, corrects it much more
effectively:

| Dataset | GBM raw ECE | GBM Platt ECE | GBM isotonic ECE |
|---|---|---|---|
| principal | 0.1360 | 0.1515 | **0.0096** |
| sensitivity_quality | 0.0957 | 0.1527 | **0.0172** |
| sensitivity_negative_window | 0.1384 | 0.1501 | **0.0236** |

Isotonic used 1,774 monotone blocks on `principal` (out of 2,652
calibration rows) — plenty of resolution for a 10-bin ECE, and not a
sign of instability (a degenerate 1-2 block isotonic fit would be the
red flag for insufficient calibration volume; nothing close to that
was observed on any of the three datasets).

Logistic (M1) calibration is comparatively minor: raw scores already
have modest ECE (0.041-0.049 across datasets), and Platt scaling
*worsens* it in every case (e.g. `principal` 0.047 → 0.093) — logistic
regression's own sigmoid output is already close to its best simple
recalibration, so an additional Platt layer adds noise rather than
correcting bias. **Only GBM+isotonic is treated as the calibrated
candidate going forward**; GBM+Platt and logistic+Platt are reported
for completeness but not recommended.

## Full calibration comparison, test 2025

| Dataset | Model+calibration | ROC-AUC | AP | Brier | ECE | log loss |
|---|---|---|---|---|---|---|
| principal | logistic raw | 0.9744 | 0.9283 | 0.0596 | 0.0472 | 0.2063 |
| principal | logistic Platt | 0.9744 | 0.9283 | 0.0674 | 0.0931 | 0.2537 |
| principal | GBM raw | 0.9819 | 0.9345 | 0.0668 | 0.1360 | 0.2631 |
| principal | GBM Platt | 0.9819 | 0.9345 | 0.0720 | 0.1515 | 0.2753 |
| principal | **GBM isotonic** | 0.9764 | 0.9308 | **0.0460** | **0.0096** | 0.2207 |
| sensitivity_quality | logistic raw | 0.9586 | 0.7376 | 0.0548 | 0.0411 | 0.1904 |
| sensitivity_quality | GBM raw | 0.9718 | 0.7721 | 0.0570 | 0.0957 | 0.2086 |
| sensitivity_quality | **GBM isotonic** | 0.9691 | 0.7570 | **0.0459** | **0.0172** | 0.1509 |
| sensitivity_negative_window | logistic raw | 0.9740 | 0.9266 | 0.0597 | 0.0493 | 0.2086 |
| sensitivity_negative_window | GBM raw | 0.9813 | 0.9377 | 0.0675 | 0.1384 | 0.2646 |
| sensitivity_negative_window | **GBM isotonic** | 0.9738 | 0.9307 | **0.0488** | **0.0236** | 0.2523 |

(AUC/AP are rank-based and identical between raw and Platt for the same
model, since Platt scaling is a monotone transform of the raw score.)

## Interpretation

- Ranking quality (ROC-AUC, AP) is essentially unaffected by
  calibration choice, as expected.
- Calibration quality (Brier, ECE) is materially improved by isotonic
  on GBM, at a small cost to AP (0.9345 → 0.9308 on `principal`, i.e.
  isotonic collapses some fine-grained rank order into shared blocks).
  This is the expected ranking/calibration trade-off, not a defect.
- **These are relative propensity scores, not demonstrated absolute
  probabilities of ignition for a given cell-day** in the sense of
  being validated against an external, independent probability
  reference. The isotonic-calibrated GBM score is calibrated *against
  the 2024 sampled dataset's own label distribution*, which itself
  inherits the negative-sampling design's known properties (documented
  in `NEGATIVE_SAMPLING_DESIGN.md` / phase 3B.4/3B.6). Any future use of
  these scores as an operational probability would require the same
  scrutiny already applied to v1's own probability semantics — this
  phase does not extend that scrutiny beyond the isotonic fit's
  internal consistency (ECE on held-out 2025).
