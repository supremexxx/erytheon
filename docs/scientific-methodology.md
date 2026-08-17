# Scientific methodology

This page explains what Erytheon models, at what spatial and temporal
resolution, from which signals, and how it evaluates candidate models
before any promotion decision. It is written for a developer or data
scientist who wants to understand the system well enough to critique it,
reproduce a result, or propose a change — not as a marketing summary.

For a per-decision derivation with more detail, see the
[research archive](research/README.md), in particular `PHASE3A_HUMAN_DATASET_SPECIFICATION.md`,
`NEGATIVE_SAMPLING_DESIGN.md`, and `MODEL_TRAINING_PROTOCOL.md` under
`docs/research/`.

## What Erytheon models

Erytheon estimates a **relative wildfire ignition risk score** — not an
absolute probability, not a guarantee, and not an official alert. The
target concept is: *given weather, satellite, territorial, and historical
signals for a place and time, how does its ignition risk compare to other
places and times?* See [`docs/scientific-limitations.md`](scientific-limitations.md)
for what this framing deliberately does not claim.

## Spatio-temporal unit

- **Space**: H3 hexagonal cells (resolution 8 or 9, configurable via
  `H3_RESOLUTION`). H3 was chosen for uniform cell area, hierarchical
  aggregation, and straightforward neighbor queries — see `crates/grid`.
- **Time**: one civil day (Europe/Paris) per cell, for the human-ignition
  dataset; forecast horizons served by the API are `nowcast`, `+6h`,
  `+24h`, and `+48h`.
- The core observation unit for the learned model datasets is **one H3
  cell on one calendar date** — a "cell-day".

## Signals

| Signal | Source | Role |
|---|---|---|
| Fire Weather Index (FFMC, DMC, DC, ISI, BUI, FWI) | Météo-France / ECMWF / Open-Meteo, via `crates/fwi` | Physical fire-danger component |
| Satellite thermal anomalies | NASA FIRMS | Recent-activity signal, not a label by itself |
| Historical ignition events | BDIFF, Prométhée | Ground-truth labels for the learned component |
| Road / POI / power-line density | OpenStreetMap | Human-activity proxy features |
| Land cover / combustibility | CORINE Land Cover | Vegetation fuel proxy |
| Population density | INSEE (Filosofi 200m) | Human-presence proxy |
| Wildland-urban interface (WUI) | Derived from OSM + CORINE | Structural risk proxy |
| Calendar (weekday, school/public holiday, season) | Territorial calendars | Human-behavior proxy |

Full source-by-source licensing and redistribution terms are in
[`docs/data-sources.md`](data-sources.md).

## Labels

A cell-day is labeled `human_ignition = 1` if and only if at least one
admissible `human_known`-cause fire event is recorded in that cell on that
date, per BDIFF/Prométhée. Events with `natural_known` or `unknown` cause
are **never** used as positive labels for the human-ignition target, and —
importantly — a cell-day carrying such an event is also excluded from the
negative candidate pool (it is neither a confirmed positive nor a confirmed
negative for the human-caused target). See
`docs/research/reports/NEGATIVE_SAMPLING_DESIGN.md` for the full exclusion
logic and `docs/research/phases/PHASE3A_HUMAN_DATASET_SPECIFICATION.md` for
the label specification.

A `label = 0` row means *no ignition was recorded* in that cell-day — it
does not mean risk was actually zero. See
[Negative sampling](#negative-sampling-and-class-balance) below and
[`docs/scientific-limitations.md`](scientific-limitations.md).

## Negative sampling and class balance

Because true ignitions are rare relative to the number of cell-days in a
territory, the training dataset uses **negative sampling**: negatives are
drawn from an eligible population (combustible cells, sufficient feature
coverage, no co-located human/natural/unknown event, correct time window)
rather than including every non-event cell-day. Several sampling window
strategies were compared (see `docs/research/reports/NEGATIVE_SAMPLING_DESIGN.md`)
before selecting one for the v1 candidate dataset.

**This means the positive/negative ratio in Erytheon's datasets is a
sampling design choice, not the real-world prevalence of fire ignition.**
A dataset "balance" of, say, 1 positive to 4 negatives (the default
`HUMAN_MODEL_NEGATIVES_PER_POSITIVE=4`) says nothing about how rare fires
actually are in the territory; it is chosen to make the learning problem
tractable. Metrics computed on such a dataset (ROC-AUC, average precision)
should always be read in that context, not treated as operational
detection rates.

## Train / calibration / test splits

Splits are constructed to respect time — later periods are held out from
training so that a model is never evaluated on data from before it "knew"
about — with training, calibration, and test periods documented per
dataset in `docs/research/reports/MODEL_TRAINING_PROTOCOL.md` and
`docs/research/reports/MODEL_CANDIDATE_ARTIFACT.md`. Same-day rows across
different cells are correlated (they share weather and calendar features),
which is accounted for when interpreting confidence intervals — see
`docs/research/phases/PHASE3B7_MODEL_CANDIDATE_REPORT.md` §"paired
comparison" for the specific correction used.

## Models

Erytheon separates a **physical** component (FWI, deterministic, not
learned) from a **human** component (learned from historical labels). The
two are fused into the operational score. See
[`docs/models.md`](models.md) for the full v1 (active) and candidate v2
(inactive) architecture, features, and status.

## Calibration

The active v1 human component is a logistic regression whose raw output is
documented as a *relative propensity*, with no separate probability
calibration layer. The candidate v2 model adds an isotonic-regression
calibration step on top of a gradient-boosted-tree ranker. See
`docs/research/reports/MODEL_CALIBRATION_REPORT.md` for the calibration
methodology and its own stated limits (in particular, that calibration
quality measured on a historical, sampled dataset does not guarantee
calibration on live, real-world-prevalence data).

## Evaluation

Standard binary-classification metrics (ROC-AUC, average precision,
calibration error) are computed on held-out historical splits. These are
**historical benchmark numbers**, not live operational performance — see
[`docs/models.md`](models.md#historical-benchmark-performance-does-not-imply-live-operational-performance)
for why that distinction matters and is enforced in how results are
reported.

## Shadow scoring and promotion

No candidate model is ever served to users, and no candidate is promoted to
active status, without a separate, explicit, controlled shadow-scoring
phase against live (not historical) data — see
[`docs/research/reports/SHADOW_SCORING_DESIGN.md`](research/reports/SHADOW_SCORING_DESIGN.md)
for the design (not yet implemented) and
[`docs/research/reports/MODEL_PROMOTION_PLAN.md`](research/reports/MODEL_PROMOTION_PLAN.md)
for the promotion criteria. A model is never promoted purely because a pull
request reports better historical metrics — see
[`GOVERNANCE.md`](../GOVERNANCE.md).
