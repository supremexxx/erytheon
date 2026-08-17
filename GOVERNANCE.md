# Governance

FireSift is currently initiated and maintained by a single primary
maintainer, with contributions accepted via pull requests. This document
describes how decisions get made — it is intentionally light. FireSift
does not need a foundation, a steering committee, or a formal RFC process
at its current size; it needs a clear, honest rule for the one thing that
actually matters: **what happens to models and the claims FireSift makes
about them.**

## Roles

- **Maintainer(s)**: merge PRs, cut releases, and make the calls described
  below. Currently one primary maintainer.
- **Contributors**: anyone opening issues or pull requests. No special
  process to become one — just open a PR that follows
  [`CONTRIBUTING.md`](CONTRIBUTING.md).

This can grow into something more formal (multiple maintainers, a
documented promotion path from contributor to maintainer) as the project
and community grow. That is not needed today, and this document will be
updated when it is.

## Principles

- **v1 stays active until a separate, explicit promotion decision says
  otherwise.** No PR, migration, or documentation change activates or
  deactivates a model as a side effect.
- **The candidate model's `inactive` status cannot be changed by a
  documentation or code PR alone.** Promotion requires the process in
  [`docs/research/reports/MODEL_PROMOTION_PLAN.md`](docs/research/reports/MODEL_PROMOTION_PLAN.md)
  and a completed shadow-scoring phase
  ([`docs/research/reports/SHADOW_SCORING_DESIGN.md`](docs/research/reports/SHADOW_SCORING_DESIGN.md)),
  not just a good historical benchmark number.
- **No benchmark chasing.** A pull request that reports a better ROC-AUC,
  AP, or calibration score on a historical split is not, by itself,
  grounds to change what's served. See
  [`docs/scientific-limitations.md`](docs/scientific-limitations.md) for
  why historical validation is not live validation.
- **Every model/data/scoring change is reversible, measured, and
  independent of the serving path** until a maintainer explicitly wires it
  in. Interface and documentation changes must never trigger scoring,
  import, or migration in production as a side effect.
- **Scientific decisions important enough to change what FireSift claims**
  (a new model family, a change to what the score represents, a change to
  labeling methodology) should be discussed in an issue before
  implementation, using the scientific-proposal issue template.
- **Published Git tags are never moved**, and applied migrations are never
  retroactively edited — see [`CONTRIBUTING.md`](CONTRIBUTING.md).

## Decision-making

- Routine changes (bug fixes, documentation, refactors, new tests) are
  merged by the maintainer via normal PR review.
- Changes touching models, labels, datasets, features, scoring, sampling,
  or calibration go through the extra review described in
  [`CONTRIBUTING.md`](CONTRIBUTING.md#changes-to-models-labels-datasets-features-scoring-sampling-or-calibration).
- Disagreements are resolved by the maintainer, informed by the discussion
  on the relevant issue or PR. As the contributor base grows, this section
  will be revisited.

## What this document is not

This is not a promise of response time, not a commitment to any roadmap
item in [`ROADMAP.md`](ROADMAP.md), and not a legal document. It exists so
contributors know how decisions get made, especially the one decision this
project is most careful about: never quietly changing what a "risk score"
means or which model produces it.
