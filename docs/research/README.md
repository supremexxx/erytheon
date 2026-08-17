# Research archive

This directory is the historical record of how Erytheon's scientific
foundation was built: phase-by-phase engineering reports, dataset and model
design documents, and console/UI build reports. They were written as
internal working documents during development, not as public-facing
documentation — expect implementation detail, French-language sections, and
references to internal phase numbering.

They are kept, not deleted, because the reasoning behind design decisions
(why a negative-sampling strategy was chosen, why a feature was dropped,
what a calibration report actually measured) has real value for anyone
auditing or extending the project. If you are new to Erytheon, start with
the root [`README.md`](../../README.md) and [`docs/`](../) instead — this
archive is for people who want the full derivation.

- **`phases/`** — chronological phase reports (data platform foundation,
  FIRMS ingestion, dataset construction, model candidate training,
  scientific console build-out, production deployment notes).
- **`reports/`** — standalone design and audit documents referenced by one
  or more phases (dataset specifications, model calibration, console
  architecture, UI style guides, negative-sampling design, etc.).

Some documents describe infrastructure or deployment details for a specific
private VPS; where those details were not relevant to reproducing the
system elsewhere, identifying values (IP addresses, hostnames) have been
replaced with placeholders. See
[`../../OPEN_SOURCE_READINESS_REPORT.md`](../../OPEN_SOURCE_READINESS_REPORT.md)
for the full audit.
