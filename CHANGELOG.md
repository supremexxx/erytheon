# Changelog

All notable changes to PyroRisk are documented in this file.

## [Unreleased]

### Added

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
