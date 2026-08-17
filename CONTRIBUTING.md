# Contributing to FireSift

Thanks for considering a contribution. FireSift is a research platform
before it is a product — the bar for a change is not just "does it work"
but "is it scientifically justified and honestly described."

## Local setup

1. Install the pinned Rust toolchain (handled automatically by `rustup` via
   [`rust-toolchain.toml`](rust-toolchain.toml), currently `1.97.1` with
   `rustfmt` and `clippy`).
2. Install Docker with Compose.
3. Install `gdal` and `eccodes` (`sudo apt-get install gdal-bin
   libeccodes-tools` on Debian/Ubuntu, `brew install gdal eccodes` on
   macOS) — only needed to exercise the ECMWF direct-GRIB2 weather path
   when running `engine` on the host rather than in the container (the
   `Dockerfile` already installs both for you). Without them the service
   still starts and most endpoints work, but weather ingestion silently
   fails over to empty results, which is confusing to debug the first
   time you hit it.
4. `cp .env.example .env`
5. `docker compose up -d` (PostgreSQL 16 / PostGIS 3.4)
6. `cargo run -p engine -- run`

See the root [`README.md`](README.md#quick-start) for the full quick start
and endpoint list.

## Before opening a PR

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
```

All three must pass; this mirrors CI (`.github/workflows/ci.yml`) exactly,
so a green local run means a green CI run. Integration tests require the
PostgreSQL/PostGIS service from `docker-compose.yml`.

## Project structure

Nine crates under `crates/`: `engine`, `api`, `store`, `ingest`, `dataset`,
`quality`, `risk`, `fwi`, `grid`. See [`docs/architecture.md`](docs/architecture.md)
for responsibilities and how they connect. Read the crate(s) you're
touching before writing code that spans more than one — most changes
should stay within a single crate's boundary.

## Commit and PR conventions

- Commit messages: short, imperative, prefixed by type where it helps
  (`fix:`, `feat:`, `docs:`, `chore:`, `test:`) — see `git log` for the
  existing convention.
- Keep PRs scoped to one logical change. A bug fix doesn't need a
  drive-by refactor bundled in.
- Fill in the PR template, including the science/dataset/model/production
  impact checkboxes — leaving them blank is not equivalent to "no impact."

## Changes to models, labels, datasets, features, scoring, sampling, or calibration

This is the one category of change with an extra bar, because it's exactly
where a well-intentioned PR can quietly make the project less honest:

- **State the scientific justification**, not just the metric delta. Why
  is this change correct, not just why does it score better?
- **Include tests.** A dataset or scoring change without a test that would
  have caught a regression is not reviewable.
- **Call out leakage risk explicitly.** If the change touches negative
  sampling, feature timing, or train/calibration/test splits, say what you
  checked for leakage and how. See
  [`docs/scientific-methodology.md`](docs/scientific-methodology.md) and
  [`docs/scientific-limitations.md`](docs/scientific-limitations.md#known-leakage-risks-and-controls)
  for the existing checks to build on.
- **A better historical metric is never sufficient by itself to activate
  or promote a model.** See [`GOVERNANCE.md`](GOVERNANCE.md). If your PR
  reports a strong ROC-AUC or AP improvement, that's a reason to propose a
  shadow-scoring evaluation, not a reason to flip a model to active.
- **Never modify an already-applied migration.** New schema changes are
  new migration files under `migrations/`; see the note in
  [`docs/deployment.md`](docs/deployment.md#database).

## Documentation changes

If you move or rename a document, use `git mv` so the file's history is
preserved, and update any Markdown links that pointed to it (`grep -rn
"](OLD_NAME" .` is a fast way to find them).

## Reporting issues

- **Bug report / feature request / data source proposal / scientific
  proposal**: use the templates under `.github/ISSUE_TEMPLATE/`.
- **Security issue**: do not open a public issue — see
  [`SECURITY.md`](SECURITY.md).

## Code of conduct

Participation in this project is covered by [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).
