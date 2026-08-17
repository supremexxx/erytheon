---
name: Scientific / model proposal
about: Propose a change to labels, features, sampling, scoring, calibration, or a new/updated model
title: "[science] "
labels: science
---

Changes in this category get extra scrutiny — see
[`CONTRIBUTING.md`](../../CONTRIBUTING.md#changes-to-models-labels-datasets-features-scoring-sampling-or-calibration)
and [`GOVERNANCE.md`](../../GOVERNANCE.md). A strong historical metric is
never sufficient by itself to activate or promote anything.

## Hypothesis

<!-- What do you believe, and why? -->

## Motivation

<!-- Why does this matter for FireSift's stated goal (modelling relative
     wildfire ignition risk)? -->

## Data

- Which dataset(s) does this use or change?
  (see [`docs/data-sources.md`](../../docs/data-sources.md) and
  [`docs/research/`](../../docs/research/) for what exists today)
- Does it introduce a new data source, feature, or label definition?

## Metrics

- What would you measure, and against which held-out split?
- What's the baseline you're comparing against?

## Leakage risk

- Does this touch negative sampling, feature timing, or train/calibration/
  test splits?
- What did you check to rule out leakage? (See
  [`docs/scientific-limitations.md`](../../docs/scientific-limitations.md#known-leakage-risks-and-controls)
  for the existing checks.)

## Reproducibility

- Seed(s) used:
- Can this be reproduced from the instructions in
  [`docs/reproducibility.md`](../../docs/reproducibility.md)? If not, what's
  missing?

## Production impact

- Does this change what v1 serves? (It should not, without a separate,
  explicit promotion decision — see `GOVERNANCE.md`.)
- Does this affect the candidate model's status? (It should not become
  active as a side effect of this proposal.)

## Proposed validation

- Historical backtest only, or does this warrant a shadow-scoring
  discussion? (See
  [`docs/research/reports/SHADOW_SCORING_DESIGN.md`](../../docs/research/reports/SHADOW_SCORING_DESIGN.md).)
