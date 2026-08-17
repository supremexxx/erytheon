# Reproducibility

This page is for someone who wants to reproduce FireSift's local
development environment, its scientific experiments, or its reported
metrics — not just run the service.

## Software environment

- **Rust toolchain**: pinned in [`rust-toolchain.toml`](../rust-toolchain.toml)
  (currently `1.97.1`, `rustfmt` + `clippy` components). `rustup` picks
  this up automatically inside the repository.
- **Dependencies**: pinned in [`Cargo.lock`](../Cargo.lock); CI runs with
  `--locked` so a build never silently resolves to different dependency
  versions than what's committed.
- **Database**: PostgreSQL 16 + PostGIS 3.4 (`postgis/postgis:16-3.4`,
  pinned in [`docker-compose.yml`](../docker-compose.yml) and
  `.github/workflows/ci.yml`). Migrations under `migrations/` are applied
  additively at engine startup — they are never rewritten after being
  applied (see [`CONTRIBUTING.md`](../CONTRIBUTING.md)).

## Reproducing the local service

```sh
git clone <this-repository>
cd erytheon
cp .env.example .env
docker compose up -d
cargo run -p engine -- run
```

This runs against the versioned fixtures under `testdata/` (`DATA_PROFILE=fixture`
by default) — no external API keys or real datasets are required. See the
[Quick start](../README.md#quick-start) in the root README for the full
walkthrough, including the endpoints exposed.

## Reproducing experiments and reported metrics

Scientific experiments (dataset construction, negative-sampling comparison,
model training, v1/candidate comparison) are implemented as `engine`
subcommands and are documented, run-by-run, in
[`docs/research/`](research/). Each report that states a metric also states
the exact invocation and the seed used, for example:

```sh
pyrorisk run-v1-comparison --seed 2026071
```

(see `docs/research/reports/V1_CANDIDATE_COMPARISON.md`, official run
`seed=2026071`, code commit `2eec181`).

When reproducing a reported number:

- **Use the documented seed.** Several experiments are randomized
  (negative sampling, train/calibration/test partitioning); the seed is
  part of what makes a specific reported number reproducible.
- **Check the code commit.** A report names the commit it was run against;
  the codebase has since evolved, and re-running the same command on a
  later commit is not guaranteed to reproduce the same figures — that's
  expected, not a bug, and any real discrepancy should be reported as
  described in [`CONTRIBUTING.md`](../CONTRIBUTING.md).
- **Never touches production.** Every experiment subcommand in `engine` is
  documented as read-only with respect to the serving path — it writes to
  an ephemeral or explicitly separate location (e.g.
  `/tmp/erytheon-experiments-*`), never to the tables the operational API
  reads from.
- **Real training data is not committed.** Datasets are built from the
  real data sources in [`docs/data-sources.md`](data-sources.md), which are
  not bundled in this repository (see `.gitignore`); reproducing a training
  run requires independently acquiring that data under its own license.

## Checksums and manifests

Where a report states a dataset row fingerprint, artifact size, or file
checksum (e.g. `docs/research/reports/MODEL_CANDIDATE_ARTIFACT.md`'s
`dataset_row_fingerprint bee1bfaa5401144c5cbffe1f42bf45f7`), that value is
meant to let you confirm you are looking at the same dataset the report
describes, not a re-derivation that happens to look similar.

## Known reproducibility gaps

- Some historical phase reports predate a documented seed/commit
  convention; where a report doesn't state one, treat its numbers as
  illustrative of the finding rather than bit-for-bit reproducible.
- Territorial static-feature snapshots (OSM, CORINE, INSEE) are dated
  extracts; reproducing a historical training run exactly requires the
  same snapshot date, not just the same processing code — see
  [`docs/scientific-limitations.md`](scientific-limitations.md#static-territorial-features-have-temporal-drift).
