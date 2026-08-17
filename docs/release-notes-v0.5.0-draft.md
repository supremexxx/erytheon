# Erytheon v0.5.0 — First Open Research Release (draft)

**Status: DRAFT.** No `v0.5.0` tag or GitHub release exists yet. This is
release-notes content prepared ahead of that decision, for the maintainer
to review, edit, and publish when they choose to cut the release — not an
announcement that it has happened. See
[`OPEN_SOURCE_READINESS_REPORT.md`](../OPEN_SOURCE_READINESS_REPORT.md)
for the readiness assessment behind this recommendation, including why
`v0.5.0` (not `v1.0.0`) is the right version number here.

---

## Erytheon is now open source

Erytheon — an experimental open-source platform for modelling and mapping
**wildfire ignition risk** from weather, satellite observations,
territorial features, and historical fire records — is now published as
open source, dual-licensed MIT OR Apache-2.0.

This release does not claim Erytheon is a validated operational forecasting
product. It claims the opposite, honestly: Erytheon is a research
codebase, with a documented scientific foundation, real limitations, and
an explicit non-goal of overclaiming what it can currently do. See
[What Erytheon is / is not](../README.md#what-erytheon-is) in the README.

## What's in this release

- **Architecture**: a 9-crate Rust workspace (`engine`, `api`, `store`,
  `ingest`, `dataset`, `quality`, `risk`, `fwi`, `grid`) over PostgreSQL/
  PostGIS. See [`docs/architecture.md`](architecture.md).
- **Scientific status**: v1 (logistic regression + FWI fusion) is the sole
  active, served model. The `gbm_isotonic_v2` candidate is registered
  `inactive` — trained, calibrated, and compared against v1 on historical
  data, but never served and never shadow-scored against live data. See
  [`docs/models.md`](models.md).
- **Documented limitations**: relative (not absolute) risk scoring,
  historical-vs-live validation gap, static-feature temporal drift,
  negative-sampling class-balance caveats, and more — see
  [`docs/scientific-limitations.md`](scientific-limitations.md).
- **Reproducible local demo**: `docker compose up -d && cargo run -p
  engine -- run`, running entirely on small versioned fixtures — no API
  keys or real datasets required. See the [Quick start](../README.md#quick-start).
- **Data source licensing**: every third-party data source Erytheon reads
  (NASA FIRMS, Météo-France, ECMWF, Open-Meteo, BDIFF, Prométhée,
  OpenStreetMap, CORINE Land Cover, INSEE) is documented with its actual
  license and redistribution terms, verified against each provider's
  current published terms — see [`docs/data-sources.md`](data-sources.md).
- **Contribution infrastructure**: [`CONTRIBUTING.md`](../CONTRIBUTING.md),
  [`CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md), [`SECURITY.md`](../SECURITY.md),
  [`GOVERNANCE.md`](../GOVERNANCE.md), issue/PR templates, and a
  weekly-cadence Dependabot configuration.

## What this release is not

- Not a security-hardened public-facing production release — see the
  [Final Release Gate](../OPEN_SOURCE_READINESS_REPORT.md#final-release-gate)
  table in the readiness report for exactly what has and hasn't been
  verified.
- Not a promise about the future of the `gbm_isotonic_v2` candidate. Its
  status is unchanged by this release, and stays unchanged until a
  separate, explicit shadow-scoring and promotion process runs — see
  [`GOVERNANCE.md`](../GOVERNANCE.md).
- Not a claim that Erytheon is ready for institutional, commercial, or
  civil-protection use. See [What Erytheon is not](../README.md#what-erytheon-is-not).

## Versioning note

Software version `v0.5.0` reflects a pre-1.0, still-evolving API and
methodology — not a statement that the science is finished. Model
identity (`human-v1`, `gbm-isotonic-v2`) and dataset identity are versioned
independently of the software release; see [`docs/models.md`](models.md#versioning).

## Upgrading

There is no prior open-source release to upgrade from. Deployments that
were already running a private pre-release revision should read
[`docs/deployment.md`](deployment.md) and confirm their configuration
against the current `.env.production.example` before adopting this tag.

## Thanks

This release is the result of the phase-by-phase scientific and
engineering work archived in [`docs/research/`](research/) — kept, not
discarded, because the reasoning behind each decision has real value for
anyone auditing or extending the project.
