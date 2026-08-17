# Phase 3B.7 — Model Candidate Report

**Entraînement expérimental, calibration et comparaison des modèles
candidats.** This report covers the full protocol from git audit
through the final promotion decision. Details on the training/baseline
pipeline are in `MODEL_EXPERIMENTS.md`; calibration methodology and
comparison in `MODEL_CALIBRATION_REPORT.md`.

## 1. Git and environment audit

Starting state: `2d9cf3b` (phase 3B.6 review), continuing directly
from it — no phases repeated. `git status --short` was clean before
this phase's changes; no secrets, dumps, or `.env` files were present
or introduced. Nothing under `crates/api`, FIRMS, FWI, or the
scheduler was touched. `git diff origin/main..HEAD --stat` shows the
expected multi-phase divergence from `origin/main` (43 files, work
never pushed, consistent with every prior phase in this project).

This phase's code was committed at `ff84fbd` (`feat: train reproducible
model candidates`), touching only:
`crates/engine/src/main.rs`, `crates/engine/src/model_experiments.rs`
(new), `crates/engine/build.rs` (new), `crates/store/src/dataset.rs`,
`crates/store/src/lib.rs`. No production code path (serving, API,
scheduler, FIRMS, FWI, active model) was modified.

Environment: isolated build container `erytheon-3b7-build` (2 CPU
budget via `CARGO_BUILD_JOBS=2`, 4 GiB RAM), connected over
`erytheon-3b7-net` to the long-lived isolated Postgres deploy
container `erytheon-3b3-deploy-20260727T203310Z` (preserved unchanged
across phases 3B.3-3B.7, no new dump created, no new volume). Source
was transferred via an explicit include-list `tar`
(`Cargo.toml Cargo.lock crates migrations testdata`, `.git` excluded
by design), SHA-256 verified identical on both ends before extraction.

**A real environment gap and its fix**: the build container's transfer
excludes `.git`, so the manifest's `git_commit` field cannot be
resolved by running `git rev-parse` inside the container. Fixed by
adding `crates/engine/build.rs`, which accepts an
`ERYTHEON_GIT_COMMIT` override env var (falling back to a local `git`
lookup, then `"unknown"`, for ordinary local builds where `.git` is
present). The official run below was built with
`ERYTHEON_GIT_COMMIT=ff84fbd`, matching the exact commit this report
describes.

## 2. Datasets

Three of the four phase 3B.5 candidate datasets were exercised end to
end (`principal`=inclusive N3, `sensitivity_quality`=strict N3,
`sensitivity_negative_window`=inclusive N2 k-ring2/day3). `strict_n2`
was explicitly not run this phase — see `MODEL_EXPERIMENTS.md`'s
"Datasets exercised" section for the scoping rationale.

## 3. Pre-registered promotion criteria (mission §19)

Printed to stdout as `3b7_promotion_criteria_registered` **before**
any dataset row was read this run:

```json
{
  "min_roc_auc": 0.60,
  "max_brier_score": 0.20,
  "max_ece": 0.10,
  "min_lift_at_10pct": 1.5,
  "min_average_precision_gain_over_v1": 0.0
}
```

## 4. Baselines

See `MODEL_EXPERIMENTS.md` §"Baselines". Constant/seasonal/spatial/
spatio-seasonal all implemented and evaluated on all three datasets.
v1 was **not** evaluated under this protocol — feature-vector
incompatibility documented honestly in `MODEL_EXPERIMENTS.md`, and
treated as an unmet promotion criterion below, not silently skipped.

## 5. Models and hyperparameter search

M1 (logistic) and M2 (GBM), both from-scratch. Hyperparameters chosen
using only internal progressive validation on `train 2020-2023`;
`2024`/`2025` never read during this step (enforced by construction —
see `MODEL_EXPERIMENTS.md`). Chosen per dataset:

| Dataset | Logistic l2 | GBM (n_trees, depth, lr) |
|---|---|---|
| principal | 0.001 | (50, 3, 0.1) |
| sensitivity_quality | 0.001 | (50, 3, 0.1) |
| sensitivity_negative_window | 0.001 | (100, 3, 0.05) |

## 6. Calibration

See `MODEL_CALIBRATION_REPORT.md` in full. Summary: GBM+isotonic is
the best-calibrated candidate on every dataset (ECE 0.010-0.024 vs.
0.096-0.152 for raw/Platt), at a small AP cost relative to raw GBM.

## 7. Test 2025 results (single evaluation)

See `MODEL_EXPERIMENTS.md` §"Results" for the full table. Headline: GBM
(raw ranking) reaches ROC-AUC 0.972-0.982 and AP 0.74-0.94 across all
three datasets; GBM+isotonic trades a small amount of AP for
dramatically better calibration (ECE ≤0.024 everywhere).

## 8. Uncertainty (bootstrap)

200-round block bootstrap by unique test date (respecting that
same-day rows are correlated), `principal`/GBM+isotonic: ROC-AUC 95%
CI [0.972, 0.980], AP [0.913, 0.945], Brier [0.042, 0.050]. Full detail
in `MODEL_EXPERIMENTS.md`.

## 9. Spatial validation

5-fold, grouped by H3 resolution-5 parent, within train (logistic,
cost-bounded): mean AUC 0.9741, variance 7.14e-06, no failing or
over-represented zone. Complementary to, and does not replace, the
2025 temporal test. Full detail in `MODEL_EXPERIMENTS.md`.

## 10. Dataset comparison

| Dataset | Best model | Test AP | Test ROC-AUC | Calibrated ECE |
|---|---|---|---|---|
| principal (inclusive N3) | GBM isotonic | 0.9308 | 0.9764 | 0.0096 |
| sensitivity_quality (strict N3) | GBM isotonic | 0.7570 | 0.9691 | 0.0172 |
| sensitivity_negative_window (inclusive N2) | GBM (raw ranking) | 0.9377 | 0.9813 | — |

Strict N3's much lower AP (0.76 vs. 0.93) is expected and was already
predicted in phase 3B.6: it has a genuinely lower positive rate
(≈11.3% vs. 25% on the same-seed test split) because strict and
inclusive variants share the same negative pool while strict discards
label-uncertain positives, changing the class balance, not the ranking
difficulty (ROC-AUC is comparable, 0.969 vs. 0.976). **No dataset is
picked purely because it has the best raw AUC/AP** — `principal` is
retained as the primary candidate dataset because its rate/AP
combination is the most representative of the deployed use case
(inclusive labeling matches what phase 3B.5 designed for production
continuity), not because its metrics happen to be highest.

## 11. Combustibility (H3 9→8) sensitivity

Reused phase 3B.6's already-measured `any(child)` vs. majority vs.
proportion-threshold cell classification counts (761,556 vs. 745,445-
745,598 eligible parents). **Did not** re-derive positives-retained/
negatives-available per rule or retrain any model under alternate
rules this phase — an explicit, documented scope decision (see
`MODEL_EXPERIMENTS.md` §"Combustibility sensitivity"), not a silent
omission. The `any(child)` rule was not modified.

## 12. Weighting

Class weighting (positive weight ×3.0) vs. unweighted, `principal`,
calibration split only: negligible ranking change (AUC 0.9696 →
0.9698), calibration worsens (ECE 0.059 → 0.082). Not recommended.
Full detail in `MODEL_EXPERIMENTS.md`.

## 13. Feature importance

`hist` dominates both models; `combustible` and `road` behave
inconsistently between the two architectures — flagged as unstable,
not treated causally. Full detail and discussion in
`MODEL_EXPERIMENTS.md`.

## 14. Reproducibility

Per-dataset artifacts (`{role}_report.json`, `/tmp/erytheon-experiments-3b7/`
inside the ephemeral container): manifest (experiment ID, git commit
`ff84fbd`, dataset logical ID, pre-training row fingerprint, 12
features, normalization methods, seed `2026071`, code version, UTC
start, hardware, scientific objective, promotion criteria), chosen
hyperparameters, full test metrics for all 5 model/calibration
variants, baseline metrics, row counts, and (principal only)
supplementary analyses (bootstrap CI, spatial CV, weighting
comparison, feature importance).

| Artifact | Size (bytes) |
|---|---|
| `principal_report.json` | 13,427 |
| `sensitivity_quality_report.json` | 9,275 |
| `sensitivity_negative_window_report.json` | 9,087 |

Dataset fingerprint (`Store::dataset_rows_fingerprint`) was checked
identical before and after training for all three datasets — no
in-place modification occurred. No model binaries or raw predictions
are committed to Git (no existing project convention allows it, per
mission §24); only the JSON reports' *contents* are summarized in this
document and the two companions.

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--all-features --locked -- -D warnings`, and `cargo test --workspace
--locked` all pass clean in the isolated build container (52 store
unit tests, 44 engine unit tests including 20 new tests in
`model_experiments::tests` covering leakage-check pass/fail cases,
promotion-criteria freezing, manifest JSON round-trip, metric
correctness on known small examples, baseline fallback behavior, and
calibrator monotonicity).

## 15. Risks and limitations (honestly carried forward)

- v1 comparison not performed (feature-vector incompatibility,
  documented in `MODEL_EXPERIMENTS.md`) — `min_average_precision_
  gain_over_v1` is treated as **not met**, not assumed satisfied.
- `strict_n2` untested this phase (explicit scoping decision).
- Combustibility-rule sensitivity limited to phase 3B.6's existing
  cell-classification counts; no per-rule retraining.
- All non-calendar features inherit `current_snapshot_applied_
  historically` (phase 3B.5/3B.6's known, audited limitation).
- Isotonic calibration trades some AP for ECE — expected, not a defect,
  but means the "best AP" and "best calibrated" models are not
  identical (GBM raw vs. GBM isotonic).
- `road` and `combustible` show inconsistent importance across
  architectures — flagged, not resolved, this phase.

## 16. Final decision

Checked against the pre-registered criteria:

| Criterion | Threshold | Best candidate (principal, GBM isotonic) | Met? |
|---|---|---|---|
| `min_roc_auc` | ≥0.60 | 0.9764 | Yes |
| `max_brier_score` | ≤0.20 | 0.0460 | Yes |
| `max_ece` | ≤0.10 | 0.0096 | Yes |
| `min_lift_at_10pct` | ≥1.5 | 3.91 | Yes |
| `min_average_precision_gain_over_v1` | ≥0.0 | not evaluable (v1 incompatible) | **No** |

Four of five criteria are met comfortably. The fifth cannot be
evaluated honestly this phase — not "assumed passed," but genuinely
undemonstrated, because no faithful v1 comparison exists yet. The
mission is explicit that a model "must not be recommended solely
because it improves one metric," and symmetrically, a genuinely
unmeasured criterion cannot be waved through as met. Promotion
requires all pre-registered criteria to be demonstrably satisfied.

```
NO MODEL CANDIDATE MEETS PROMOTION CRITERIA
```

The blocking gap is narrow and specific: build a faithful v1-comparable
evaluation (either a documented feature-vector bridge, or a
side-by-side operational-lift comparison on the same 2025 cell-days)
before any future promotion review. Everything else — ranking,
calibration, spatial stability, dataset robustness, reproducibility —
already clears its bar for the `principal` dataset with GBM+isotonic.
