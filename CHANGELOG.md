# Changelog

All notable changes to Erytheon are documented in this file.

## [Unreleased]

## [0.5.0] - 2026-08-17 - First Open Research Release

### Added

- Copyright holder set in `LICENSE-MIT`/`LICENSE-APACHE`:
  `Copyright (c) 2026 William Ducamp`. Dual license (MIT OR Apache-2.0)
  unchanged.
- `NOTICE.md`, a quick-reference attribution list for all data sources
  (full detail stays in `docs/data-sources.md`).
- `docs/release-notes-v0.5.0.md` — release notes published as this
  `v0.5.0` GitHub Release.
- `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE` matching the `MIT OR Apache-2.0`
  license already declared in `Cargo.toml`.
- `docs/data-sources.md` documenting per-source data licensing, attribution,
  and redistribution status (NASA FIRMS, Météo-France, ECMWF, Open-Meteo,
  BDIFF, Prométhée, OpenStreetMap, CORINE Land Cover, INSEE).
- `docs/architecture.md`, `docs/api.md`, `docs/deployment.md`,
  `docs/models.md`, `docs/reproducibility.md`,
  `docs/scientific-methodology.md`, `docs/scientific-limitations.md`,
  `docs/public-platform.md` (vision document, not implemented).
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `GOVERNANCE.md`.
- GitHub issue templates (bug report, feature request, data source
  proposal, scientific/model proposal) and a pull request template under
  `.github/`.
- `.github/dependabot.yml` for Cargo, GitHub Actions, and Docker, on a
  weekly cadence with grouped minor/patch updates.
- `OPEN_SOURCE_READINESS_REPORT.md` — full audit ahead of public release.
- An "Open-source track" section in `ROADMAP.md` (Phases A–F), separate
  from the existing scientific/product roadmap.
- English-language root `README.md` rewritten for an open-source research
  audience, replacing the previous French-language version (content
  preserved and expanded, not just translated — see the readiness report
  for what changed).

### Changed

- Reorganized ~70 root-level phase/report Markdown documents into
  `docs/research/phases/` and `docs/research/reports/` (via `git mv`, to
  preserve file history), with a `docs/research/README.md` index. Internal
  cross-document links were updated to match.
- `docs/data-sources.md` license statuses re-verified against each
  provider's current live terms (not from memory): most sources moved
  from an unverified/uncertain status to `CLEAR`, with BDIFF and
  Open-Meteo given precise, conditional wording instead of a blanket
  status. See `OPEN_SOURCE_READINESS_REPORT.md`.
- README Quick Start and `CONTRIBUTING.md` now document a `gdal`/`eccodes`
  host prerequisite for the ECMWF direct-weather path, and a note on
  resolving a port-5432 conflict — both found by actually running the
  Quick Start end-to-end rather than only reading it.
- `.github/workflows/ci.yml` now declares an explicit minimal
  `permissions: contents: read` (defense-in-depth; no functional change).
- Replaced `testdata/promethee_aude.csv`'s single row with clearly
  synthetic data (fictional municipality, `SYNTH-`-prefixed ID), closing
  an earlier "unconfirmed provenance" question rather than leaving it
  open. No test asserted the row's specific values, so this is
  behavior-neutral; `crates/engine`'s static-layer tests were re-run to
  confirm.
- Reclassified administrative boundaries and territorial calendars in
  `docs/data-sources.md` from a blanket `REQUIRES LEGAL / LICENSE REVIEW`
  to precise statuses (`NOT BUNDLED / USER PROVIDED` for boundaries;
  `CLEAR` for the bundled calendar fixture, `NOT BUNDLED` for a real
  production calendar) after confirming what actually ships.

### Security

- Redacted a real production VPS public IP address and system hostname
  that were committed in three deployment/runbook documents, replacing
  them with placeholders. See `OPEN_SOURCE_READINESS_REPORT.md` for detail
  and for the full security audit (no secrets or credentials were found in
  the current tree or Git history via `gitleaks`). The same IP/hostname
  remain in Git history across most tags and branches — this is now a
  recorded, explicit accept-risk decision by the repository owner
  (William Ducamp), not an open question; see the readiness
  report's "Git history security" section for the exact scope and the
  commands to remove them if the maintainer chooses to.

- Direct, credential-free ECMWF IFS open-data weather acquisition with local decoded-grid caching
  and controlled Open-Meteo fallback.
- Phase 0 Cargo workspace with the seven required crates.
- PostgreSQL 16/PostGIS Docker Compose service and initial SQLx migration.
- Typed, validated environment configuration with documented defaults.
- Axum `GET /health` endpoint backed by a live database check.
- Structured tracing, graceful shutdown, CI checks, and setup documentation.
- Phase 1 pure Rust implementation of FFMC, DMC, DC, ISI, BUI, and FWI.
- Typed daily weather, persistent moisture state, outputs, and input errors.
- Standard 48-day `cffdrs` reference fixture with precision-aware validation.
- Phase 2 H3 point projection and lossless PostgreSQL `BIGINT` conversion.
- NASA FIRMS VIIRS S-NPP connector with real API windowing and fixture fallback.
- Idempotent observation persistence through source-specific deduplication keys.
- `engine backfill --source firms --days N` and static H3 GeoJSON export.
- Phase 3 Météo-France SYNOP connector with OAuth2 real access and official fixture fallback.
- Complete H3 AOI coverage and four-nearest-station inverse-distance interpolation.
- Daily moisture-code carry-over plus latest-wind intraday ISI/FWI recomputation.
- Set-based, idempotent PostgreSQL persistence for one FWI row per AOI cell and date.
- Phase 4 one-shot OSM, BDIFF, Prométhée, CORINE, INSEE, and calendar loaders.
- Direct Geofabrik PBF support, EPSG:3035 reprojection, and AOI-clipped GDAL GeoTIFF support.
- Ring-based road/POI/power densities, historical ignition kernel, and 50-metre WUI calculation.
- Idempotent `engine load-static`, `cell_static`, `ignition_history`, and `calendar_days` persistence.
- Phase 5 configurable `HeuristicV1` physical/human fusion with combustible masking and ranked factors.
- Atomic, AOI-scoped `risk_scores` batches retaining both input date and calculation time.
- Complete `/risk`, `/risk/cell/{h3}`, `/alerts`, `/sources`, and enriched `/health` REST API.
- Bbox-filterable `/stream` WebSocket risk updates and resilient FIRMS/Météo-France scheduler loops.
- Source execution status persistence, new-data triggers, and a configurable 15-minute risk safety tick.
- End-to-end fixture coverage from static/weather ingestion through valid GeoJSON and explainable scores.
- Phase 6 monthly Météo-France SYNOP archive loader with Gzip and plain-CSV support.
- Leakage-safe daily FWI and historical-density replay through `engine backtest --from --to`.
- Markdown evaluation report with approximate AUC, top-5%/top-10% ignition capture, and worst false negatives.
- Official 2025 Aude summer BDIFF evaluation fixture covering 89 forest-fire alerts.
- Embedded read-only operator dashboard served at `/` with an interactive H3 risk map.
- Threshold filtering, prioritized alerts, source health, cell explanations, FWI detail, and live WebSocket refreshes.
- Phase 7 fixture/production data profiles with strict real-file and connector validation.
- `data-status` CLI audit for configured static files, GDAL readiness, and PostgreSQL feature coverage.
- Configurable historical FWI warm-up, defaulting to 31 days before scored backtest dates.
- Phase 8 live Open-Meteo transport for Météo-France AROME/ARPEGE forecasts.
- Four atomic risk horizons at nowcast, +6 hours, +24 hours, and +48 hours with forecast-valid timestamps.
- Forecast-noon moisture-code progression, target-hour wind recomputation, and bounded forecast-batch retention.
- Horizon-aware risk, alert, cell-detail, GeoJSON, and WebSocket API contracts.
- Dashboard horizon controls with forecast-valid times and horizon-specific FWI explanations.
- Phase 9A non-root multi-architecture production image for AMD64 and Oracle ARM64 hosts.
- Isolated Oracle Compose stack with Caddy ingress, private PostGIS networking, health checks, and persistent volumes.
- Rolling Cloudflare R2 backup and guarded restore scripts with a daily systemd timer.
- GHCR multi-platform publishing workflow and zero-budget deployment runbook.
- Phase 9B official metropolitan-France department boundary download and `territory-plan` workload audit.
- Unique centroid-owned H3 resolution-8 department partitions with optional `TERRITORY_CODES` rollout filters.
- Sequential national cell-feature, FIRMS-triggered, and AROME/ARPEGE forecast processing with bounded per-partition calculation batches.
- Forecast batch lifecycle that hides partial national runs and atomically publishes only completed surfaces.
- Runtime dashboard configuration for the active territory, bbox, and H3 resolution.
- Phase 9C sequential regional Geofabrik PBF ingestion with relevant-node filtering and bounded regional working sets.
- Reusable per-H3 OSM JSONL cache plus checksummed metropolitan-France regional download workflow.
