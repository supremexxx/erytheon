# Scientific limitations

Erytheon is an experimental research platform, not a validated operational
forecasting product. This page lists its known limitations without
minimizing them. A credible scientific project is expected to state its
weaknesses at least as clearly as its results — this document is that
statement, and it should be kept current as new limitations are found.

If you find a limitation that isn't listed here, please open an issue (see
[`CONTRIBUTING.md`](../CONTRIBUTING.md)) — surfacing gaps is a contribution,
not a criticism of the project.

## The score is a relative risk, not an absolute probability

Erytheon's output is a **relative ignition-risk score**. It is not:

- an absolute probability that a fire will occur;
- a guarantee that a fire will or will not start;
- a civil-security alert or an official warning;
- a substitute for information issued by competent authorities.

The active v1 human-ignition component is explicitly documented in its own
training code as a "relative human ignition propensity, not absolute
probability" (see `docs/research/phases/PHASE3B8_PROMOTION_GAP_REPORT.md`
and `crates/risk`). There is no calibration step in v1 that would let its
raw sigmoid output be read as a probability.

## Historical validation is not the same as live operational validation

All model metrics currently reported (ROC-AUC, average precision,
calibration error) are computed on **held-out historical data splits**, not
on live forecasts matched against subsequently observed events. Strong
historical numbers do not by themselves establish:

- stability of the model once deployed;
- calibration quality on real-time, real-prevalence data (as opposed to a
  sampled dataset — see [Negative sampling](#negative-sampling-changes-the-class-balance-you-see) below);
  
- robustness to unusual conditions absent from the historical record
  (extreme weather, new territories, data-source outages);
- generalization to territories or periods not represented in training.

This is the specific reason the candidate `gbm_isotonic_v2` model remains
`inactive` and has not been promoted, despite reporting materially better
historical metrics than v1 (see [`docs/models.md`](models.md)). See
[Prospective validation is partially implemented, not complete](#prospective-validation-is-partially-implemented-not-complete)
below for what would close this gap.

## Static territorial features have temporal drift

Features like WUI, road density, POI density, power-line density,
agricultural land use, population density, and combustibility are computed
from a single snapshot of territorial data (OSM, CORINE, INSEE), but are
applied across a multi-year historical training window (2020–2026 in
current datasets). A road built in 2024 is treated as present for a 2020
cell-day; a population figure from one census year is applied to every
year in range. This is a real, acknowledged approximation, not an oversight
— no per-year historical snapshot of these layers currently exists to
correct it. See `docs/research/reports/DATASET_NORMALIZATION_AND_IMPUTATION.md`
and `docs/research/phases/PHASE3B3_DATASET_FOUNDATION_REPORT.md` for the
detailed accounting.

## Negative sampling changes the class balance you see

Because true ignitions are rare, training datasets use negative sampling
(see [`docs/scientific-methodology.md`](scientific-methodology.md#negative-sampling-and-class-balance)).
**The ratio of positive to negative rows in any Erytheon dataset is a
sampling design choice, not the real-world prevalence of fire ignition
events.** Metrics computed on such a dataset characterize model
discrimination on that sampled distribution — they are not detection rates
you should expect against the true, highly imbalanced, real-world
distribution of cell-days.

## FIRMS observes, it does not predict

NASA FIRMS reports satellite-detected thermal anomalies — it is an
after-the-fact observation of heat, not a forecast. Erytheon uses FIRMS as
one input signal among several; it is never treated, by itself, as a
prediction of a future ignition.

## Coverage and freshness depend on upstream sources

Erytheon's risk surfaces are only as fresh and complete as the weather,
satellite, and territorial sources feeding them. A degraded or stale
upstream source (a missed Météo-France poll, an ECMWF outage, a FIRMS gap)
degrades the resulting score without necessarily being visually obvious —
see [`docs/reproducibility.md`](reproducibility.md) and the operational
dashboard's source-status view for how staleness is currently surfaced.

## Prospective validation is partially implemented, not complete

BLUE (see `docs/research/reports/BLUE_FORECAST_EVIDENCE_CONTRACT.md` and
`docs/research/reports/BLUE_AI_EVIDENCE_WORKFLOW.md`) implements a first,
partial foundation for this: forecasts are locked immutably at publication
time, and a daily selection of communes is checked against real-world
evidence at the `+24h` and `+48h` horizons, with every run, response, and
cited source archived append-only.

This is **not yet a complete prospective validation system**. In
particular:

- Only a bounded daily selection (up to twenty communes) is checked, not
  the full forecast archive.
- There is no reverse pass from all observed fire events back to the
  forecast archive, so **false negatives are not currently measured** —
  the workflow can show it found evidence where expected, but not
  systematically show what it missed.
- Because of that, Erytheon **cannot yet compute or publish recall,
  specificity, or a global precision figure** from this system. What
  exists today is case-level evidence-gathering, not an aggregate
  performance metric.
- Calibration drift and hit-rate tracking over time are not implemented.

A design for the complete system — including the reverse matching needed
for recall/specificity and a published aggregate track record — is
sketched in [`docs/public-platform.md`](public-platform.md) (roadmap
Phase D). No current claim in this repository should be read as if
full prospective validation, or any aggregate accuracy figure derived
from it, already existed.

## Shadow scoring has not been run

The candidate v2 model has never received live scoring against real,
current data — only historical backtests. Shadow scoring (running the
candidate silently, in parallel with v1, without serving its output) is
designed (`docs/research/reports/SHADOW_SCORING_DESIGN.md`) but not yet
implemented. Until it runs for a meaningful observation window, the
candidate's real-world behavior is genuinely unknown, regardless of how
strong its historical metrics look.

## Known leakage risks and controls

Because same-day rows across cells share weather and calendar features,
and neighboring cell-days can be correlated with a single event, several
specific leakage risks were evaluated during dataset construction — for
example whether a train-split row could depend on information from its
paired calibration/test period, or whether neighboring-cell exclusion
windows around a known event were tight enough. See
`docs/research/reports/NEGATIVE_SAMPLING_DESIGN.md` and
`docs/research/phases/PHASE3B7_MODEL_CANDIDATE_REPORT.md` for the specific
checks performed and their results. This is not a claim that all possible
leakage has been ruled out — new contributions that touch labels, features,
or sampling should re-examine this question explicitly (see
[`CONTRIBUTING.md`](../CONTRIBUTING.md)).

## No cherry-picking, including for future validation

As a matter of project philosophy, once prospective validation exists (see
above), Erytheon commits to recording and publishing failures — false
positives, false negatives, data outages, periods of low confidence —
alongside successes. A track record with only favorable examples would not
be credible, and is not the goal.
