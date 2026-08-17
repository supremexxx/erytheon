# Models

Erytheon separates a **physical** fire-danger component (the Canadian Fire
Weather Index, deterministic, not learned) from a **human-ignition**
component (learned from historical labels). This page documents both
generations of the learned component and their current status. Nothing on
this page changes by editing this file — model status is a runtime/registry
fact (`human_model_versions` table), and this page is descriptive of that
fact, not authoritative over it.

> **Historical benchmark performance does not imply live operational
> performance.** Every metric below was computed on held-out historical
> data splits. See [`docs/scientific-limitations.md`](scientific-limitations.md).

## Status summary

| Model | Status | Served to users |
|---|---|---|
| v1 (`LearnedHumanModel`) | **Active** | Yes — the only model served |
| `gbm_isotonic_v2` (candidate) | **Inactive** | No |
| Shadow scoring | Not implemented | N/A |

## v1 — operational model

- **Architecture**: standardized logistic regression (`crates/risk`,
  `crates/engine/src/human_model.rs`) — an intercept plus 11 weights, fit
  by plain batch gradient descent (500 iterations, learning rate
  `0.12/√(1+t/100)`, L2 regularization `0.01`).
- **Features** (fixed order, `risk::HUMAN_FEATURE_NAMES`): `wui`, `road`,
  `agri`, `population`, `poi`, `power_line`, `weekend`, `school_holiday`,
  `public_holiday`, `season_sine`, `season_cosine`. Note: 11 features —
  no `hist` (historical ignition kernel) and no `combustible` as direct
  model inputs. `combustible` gates training/serving eligibility (only
  combustible cells are ever scored) rather than being a learned feature.
- **Output**: a sigmoid, explicitly documented in the training code as a
  **relative human ignition propensity, not an absolute probability**. No
  separate calibration layer (no Platt/isotonic step).
- **Fusion**: the full operational score multiplies this human component
  by the physical (FWI-derived) component and zeroes the result for
  non-combustible cells (`HeuristicV1::score`).
- **Negative sampling**: uniform random combustible cell-days — a simpler
  design than the spatio-temporally stratified sampling used for the v2
  candidate's datasets (see [`docs/scientific-methodology.md`](scientific-methodology.md)).
- **Role**: the sole model whose output is exposed by the operational API
  (`/risk`, `/risk/cell/{h3}`, `/alerts`). It has not been modified,
  retrained, or replaced by any of the scientific-foundation work described
  in [`docs/research/`](research/).

## Candidate v2 — `gbm_isotonic_v2`

- **Status**: `inactive` in the model candidate registry. Registered,
  never served, never activated.
- **Architecture**: gradient-boosted trees (50 trees, max depth 3,
  learning rate 0.1) followed by isotonic-regression calibration
  (1,774 calibration breakpoints).
- **Features** (12, real training order — confirmed from the trained
  artifact): `wui`, `road`, `agri`, `population`, `poi`, `power_line`,
  `hist`, `combustible`, `weekend`, `public_holiday`, `season_sine`,
  `season_cosine`. Unlike v1, this candidate includes `hist` (a historical
  ignition kernel) and `combustible` as direct learned features rather than
  only as an eligibility gate.
- **Dataset**: `erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality`
  (see [`docs/data-sources.md`](data-sources.md) and
  `docs/research/reports/NEGATIVE_SAMPLING_DESIGN.md` for how it was
  constructed).
- **Artifact format**: JSON, ~85 KB, containing fitted parameters only (no
  raw training data) — feature names/types, normalization and imputation
  parameters, GBM hyperparameters and tree structure, isotonic
  breakpoints/values, and the frozen test-period metrics. See
  `docs/research/phases/PHASE3B9_PROMOTION_REVIEW.md` and
  `docs/research/reports/MODEL_CANDIDATE_ARTIFACT.md` for the full format
  and its validation rules.

### Historical benchmark metrics (candidate vs. v1's learned human component)

Measured on a paired, same-2025-rows comparison
(`docs/research/reports/V1_CANDIDATE_COMPARISON.md`; official run
`seed=2026071`):

| Metric | v1 (human component) | Candidate `gbm_isotonic_v2` |
|---|---|---|
| ROC-AUC | 0.7836 | 0.9764 |
| Average precision | 0.5840 | 0.9308 |

These figures are real, reproducible, and taken directly from the project's
own comparison report — they are **not** grounds to treat the candidate as
"97.6% accurate at predicting wildfires." They describe the candidate's
ranking/discrimination ability on a specific held-out historical split of a
sampled dataset (see [Negative sampling](scientific-methodology.md#negative-sampling-and-class-balance)).
ROC-AUC and average precision are not detection rates, and a strong
historical score does not establish live-deployment behavior — see
[`docs/scientific-limitations.md`](scientific-limitations.md).

### Why the candidate is not promoted

Promotion requires a separate, explicit shadow-scoring phase against live
data and an explicit promotion decision — not a historical metric
threshold alone. See `docs/research/reports/MODEL_PROMOTION_PLAN.md` for
the promotion criteria and `docs/research/reports/SHADOW_SCORING_DESIGN.md`
for the (not yet implemented) shadow-scoring design. Per
[`GOVERNANCE.md`](../GOVERNANCE.md), no pull request or automated process
can activate a candidate model — activation is always a separate, explicit,
documented decision.

## Versioning

- **Software**: SemVer-ish (currently pre-1.0, e.g. `v0.4.x`), tracked by
  Git tags. Existing tags are never moved.
- **Model**: named independently of software version — `human-v1`,
  `gbm-isotonic-v2` — since a model can be retrained without a software
  release and vice versa.
- **Dataset**: identified by a logical ID plus a manifest/fingerprint (see
  `dataset_row_fingerprint` in the candidate artifact format above), not by
  software version.
