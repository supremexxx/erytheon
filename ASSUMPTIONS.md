# Assumptions

## Phase 0

- Rust edition 2024 is used with Rust 1.94 or newer, required by the current H3 dependency's constant floating-point operations.
- Pending migrations run automatically when the engine starts so `docker compose up -d && cargo run -p engine -- run` requires no separate SQLx CLI step.
- Source health is an empty array until the first connector is introduced in Phase 2.
- Debug builds use human-readable logs and release builds use JSON logs, avoiding an additional environment variable not present in the required configuration contract.
- H3 indexes are persisted as PostgreSQL `BIGINT`, following the schema required by the project brief; conversion details will be implemented with the H3 grid pipeline in Phase 2.

## Phase 1

- The brief defines month, but not latitude, as an FWI input. DMC and DC therefore use the original standard northern monthly day-length factors. These factors apply to the default Aude AOI and reproduce the standard 40°N `cffdrs` fixture.
- The implementation targets the FWI 1987 equations requested by the brief. It does not adopt the separate 2025 revision of the Canadian system.
- Calculations use `f64` internally to preserve reference precision. Phase 3 persists these values as PostgreSQL `DOUBLE PRECISION`.
- Relative humidity of exactly 100% is accepted and evaluated as 99.9999%, matching the protective constraint in `cffdrs`; values above 100% are rejected.

## Phase 2

- The current NASA FIRMS Area API accepts only one to five days per request, while the required CLI example requests seven. Real fetches are split into consecutive dated windows of at most five days.
- FIRMS acquisition dates are interpreted as UTC because NASA documents its product availability dates as GMT. Acquisition times are zero-padded to four digits before parsing.
- Without `FIRMS_MAP_KEY`, the connector uses five unmodified records from NASA's official educational sample. That sample has no Aude detection for its date, so the selected southern-France cluster is near Fos-sur-Mer and outside the default AOI.
- PostgreSQL `BIGINT` is signed while H3 indexes are unsigned 64-bit values. The exact H3 bit pattern is preserved through a byte-for-byte signed reinterpretation and is covered by a round-trip test.
- The static GeoJSON export contains one H3 polygon per FIRMS detection rather than merging detections that share a cell. Persistence still deduplicates repeated source records through `dedupe_key`.

## Phase 3

- Météo-France's current targeted-observation API uses an OAuth2 bearer token even though the required environment variable retains the brief's `METEOFRANCE_API_KEY` name. Without a token, the connector reads an official-format SYNOP fixture.
- SYNOP provides tri-hourly observations. For the default Aude longitudes, 12:00 UTC is used as the available observation nearest local solar noon; the longitude-induced solar offset is smaller than the three-hour source interval.
- Cell weather uses inverse-distance weighting with power 2 by default over the four nearest stations that contain temperature, humidity, wind, and 24-hour precipitation. Negative precipitation traces are clamped to zero; incomplete stations are skipped.
- `ContainmentMode::Covers` includes H3 cells intersecting the bounding-box boundary. This slightly overshoots the AOI but guarantees that no part of it remains uncovered.
- Moisture codes read state from the exact previous calendar day. If that row is absent, standard start-up values are used rather than carrying stale state across an unknown gap.
- Daily FFMC, DMC, DC, and BUI use the noon observations. Persisted ISI and FWI use the latest available wind and are replaced idempotently on a later `recompute` for the same date. Automatic source-triggered scheduling remains Phase 5 work.
- AROME/ARPEGE GRIB ingestion is deferred because station CSV observations meet the phase requirement and the model path introduces endpoint, file-size, and decoder complexity that is not necessary for the Phase 3 DoD.
- The six `fwi_state` numeric columns are promoted from PostgreSQL `REAL` to `DOUBLE PRECISION` so persisted values retain the `fwi` crate's validated `f64` precision without lossy casts.

## Phase 4

- Public BDIFF pages expose some ignitions only at municipality precision. The fixture preserves that public precision and projects the municipality centre rather than claiming a more precise ignition coordinate. The Prométhée legacy fixture is handled the same way.
- OSM ways are decomposed into node-to-node segments. Each segment length is assigned to its midpoint H3 cell, then summed over the target cell and ring 1. Buildings use the centroid of available in-AOI way nodes; parking and campsites are activity POIs.
- Road, power-line, POI, population, and historical densities are independently divided by their maximum value over the configured AOI. An all-zero layer remains zero instead of producing non-finite values.
- The history kernel contributes 1 in the ignition cell, 0.5 in ring 1, and 0.25 in ring 2. Kernel contributions outside the configured AOI are discarded, including at AOI borders.
- WUI is binary in Phase 4: a building qualifies when an available combustible CORINE sample is within 50 metres. CORINE classes 311–313, 321–324, 333, and 334 are combustible natural cover; classes 211–244 are both agricultural and combustible. Treating crops as combustible is required for the agricultural term to survive the final non-combustible mask.
- The committed CORINE fixture is intentionally sparse; unsampled fixture cells default to non-combustible and non-agricultural. A real CORINE GeoTIFF supplies dense 100-metre samples through `gdal_translate` clipped to the AOI.
- The host used for development has no GDAL installation, so the GeoTIFF branch is implemented but the default reproducible test uses CSV samples. GDAL is a runtime prerequisite only when `CORINE_PATH` points to `.tif` or `.tiff`.
- INSEE Filosofi imputed 200-metre cells are retained and explicitly carry their `i_est_200` indicator. Population is a structural proxy and no individual or nominative data is processed.
- The default Aude AOI uses school Zone C. The calendar is global rather than duplicated in every cell; `cell_static` records the zone and `calendar_days` stores the date flags.
- In fixture mode, static source failures are isolated and logged; remaining sources still produce complete `cell_static` rows with missing layers represented by zeros. Production mode introduced in Phase 7 fails the load instead.

## Phase 5

- Risk batches are explicitly scoped to the configured AOI cell list. Existing rows from a previous AOI or resolution cannot leak into a new calculation.
- The latest available FWI date is used by the periodic scheduler. `risk_scores.input_date` records that date separately from `computed_at`, which is always the current UTC calculation time.
- Calendar multipliers interpret a public holiday like a weekend day. A weekend/public holiday during a summer school holiday uses 1.4, another weekend/public holiday uses 1.2, a December–February weekday uses 0.6, and other weekdays use 1.0. A summer-vacation weekday remains 1.0 because the brief specifies only the summer-vacation weekend case.
- Agricultural activity contributes only from June through August. `top_factors` ranks the positive weighted terms by contribution; a non-combustible masked cell has no contributing factors.
- `GET /risk` covers the requested bbox with the configured H3 resolution and reads the globally latest atomic score batch. `GET /alerts` reads the same batch over the whole configured AOI.
- A WebSocket connection receives the full update batch until it sends a valid bbox subscription. Filtering uses cell centres and applies to subsequent batches.
- Polling triggers a new score batch only when a connector inserts new observations. The independent 15-minute tick still recalculates from the latest persisted state.
- In fixture mode, the scheduler's current-date Météo-France poll can report staleness because the committed weather sample is dated 2025-07-16. Explicit `recompute --date 2025-07-16` remains the reproducible fixture path.

## Phase 6

- Backtest dates are local civil dates in `Europe/Paris`; BDIFF alert timestamps remain stored in UTC and are converted back to the local date for daily labels.
- The official monthly SYNOP archive has no station coordinates, so the loader uses the published fixed coordinates for Millau, Saint-Girons, Toulouse-Blagnac, and Perpignan. Only complete 12:00 UTC rows are retained.
- Backtest scoring remains limited to the requested interval. Phase 7 may replay preceding weather to initialize moisture codes, but those warm-up days never contribute labels or metrics.
- Historical ignition density is reconstructed incrementally from records strictly before the scored date. The `hist` value precomputed by `load-static` is intentionally ignored during backtesting to prevent future-label leakage.
- Calendar dates absent from `calendar_days` default to non-holiday flags and their count is reported. This keeps the replay deterministic while making sparse fixture coverage visible.
- BDIFF exposes the public Aude records at municipality-centre precision. The 2025 summer fixture contains the 89 department records returned for June 1 through August 31, preserving that public precision and the original UTC offset.
- Full monthly weather archives live under ignored `data/`; only a two-day official-schema parser fixture is committed. The generated Markdown report lives under ignored `out/`.
- Approximate AUC uses 1,000 score bins over day-cell labels. Top-percentile ties are resolved by ascending H3 index for deterministic, fixed-size rankings.

## Operator dashboard

- The post-Phase-6 request for an interface explicitly supersedes the original headless/no-GUI rule. The exception is limited to a read-only operational dashboard; the risk engine and API contracts remain unchanged.
- Static HTML, CSS, and JavaScript are embedded with `include_str!` and served by Axum. No Node.js runtime, frontend build pipeline, authentication, or additional service is introduced.
- The initial map threshold is 0.10 to avoid transferring all 127,018 AOI polygons to the browser. Operators can adjust it from 0.01 to 0.80.
- The map clips viewport queries to the configured default Aude AOI. Changing `AOI_BBOX` does not currently alter the dashboard's initial viewport and requires a matching dashboard configuration change.
- Leaflet and CARTO raster tiles are public CDN dependencies used only for visualization. REST and WebSocket endpoints remain available if these visual dependencies are offline.

## Phase 7

- `DATA_PROFILE=fixture` remains the default so a fresh clone is reproducible. `DATA_PROFILE=production` is an explicit operator assertion and therefore rejects missing files, paths containing a `testdata` component, and parser failures from any static source.
- `data-status` reports non-zero feature presence rather than geographic source completeness. Zero can be a legitimate value, especially for population, WUI, agriculture, and non-combustible land cover, so the audit is a diagnostic and not a certification.
- The default historical FWI warm-up is 31 days. Moisture codes start from standard initialization on the first warm-up day, then advance normally; no warm-up prediction is scored or persisted.
- May 2025 SYNOP is downloaded locally alongside June–August for the reproducible summer replay, but monthly archives remain ignored because they are upstream data rather than source fixtures.
- Large OSM, CORINE, BDIFF, Prométhée, and INSEE source files are not fetched automatically. The development volume has limited free space, and operators must choose and version the extracts supplied through `.env.production.example`.

## Phase 8

- Operational forecasts use ECMWF IFS 0.25-degree open data through direct GRIB byte-range
  downloads. The runtime derives relative humidity from temperature/dewpoint, wind speed from its
  vector components, and rolling 24-hour rain from accumulated precipitation. Open-Meteo's
  Météo-France and ECMWF endpoints remain bounded fallbacks, not the primary transport.
- Fifty-four anchors on a 0.20-degree grid cover the default Aude AOI. Each H3 cell uses inverse-distance weighting over its four nearest anchors; this is an engineering interpolation choice, not a claim of native H3 model resolution.
- Forecast moisture codes advance only at forecast noon. Each requested horizon then recomputes ISI and FWI using the target-hour wind and the latest preceding noon moisture state, avoiding four moisture-code advances for one day.
- If no prior persisted daily FWI state is available, the forecast starts from standard FFMC 85, DMC 6, and DC 15. Subsequent batches reuse the latest prior noon state when its date is appropriate.
- Only the newest complete `forecast_fwi` and multi-horizon `risk_scores` batch is retained. Historical backtesting remains in the dedicated replay path rather than the operational forecast tables.
- FIRMS detections are evidence of recent thermal anomalies and active-fire confirmation, not a predictor of future ignitions. Future horizons are driven by forecast weather fused with static human and land-cover proxies.
- The default committed static fixtures are intentionally sparse. Live timestamps and weather do not make those demonstrator human-risk layers production-complete; operators must load real regional inputs before relying on the map operationally.

## Phase 9A

- The first free cloud deployment preserves the Aude pilot. Expanding the current rectangular AOI directly to France would calculate sea and neighbouring countries; a France boundary and department partitioning are required before the national rollout.
- Oracle Cloud Always Free is treated as an opportunistic prototype host, not a service with an availability guarantee. The Compose files remain provider-independent so they can move to another ARM64 or AMD64 Docker host.
- PostgreSQL is never published on a host port. Caddy is the only public ingress and transparently proxies both HTTP and WebSocket traffic.
- Cloudflare R2 stores one rolling custom-format PostgreSQL dump. Refusing backups larger than 9 GiB preserves room under the documented 10 GB free storage allowance; long-term forecast history is intentionally excluded.
- The application image includes GDAL so the same non-root container can audit and load a production CORINE GeoTIFF. Source files remain external read-only mounts and are never baked into the image.
- GHCR is the container registry because GitHub Actions can publish ARM64 and AMD64 from the existing repository. Package visibility and external cloud accounts still require explicit user setup.

## Phase 9B

- Metropolitan coverage means the 96 departments with codes `01`–`95`, `2A`, and `2B`; overseas departments are intentionally excluded from this rollout.
- Department polygons come from Etalab's official administrative-contour publication. The 1,000-metre generalized GeoJSON is used because national H3 resolution 8 is coarser than the pilot and startup cost matters on a two-vCPU VPS.
- H3 cells use centroid containment. This gives each adjacent department unique border-cell ownership and excludes most sea cells; it intentionally does not promise exact coastline coverage at sub-cell scale.
- National static features are normalized independently inside each department partition. This bounds the calculated feature maps and produces a hyperlocal relative proxy. Cross-department score magnitudes must still be calibrated before formal national alert thresholds are claimed.
- All partitions in one weather run share one `computed_at`. A running batch is excluded from API reads and only becomes visible after every requested department succeeds; an interrupted batch leaves the previous complete surface online.
- `AOI_BBOX` remains the acquisition envelope for FIRMS and the dashboard initial extent. Territory polygons, not that rectangle, define the national calculation cells.
- H3 resolution 8 is the only supported full-France baseline on the current VPS. Resolution 9 remains appropriate for selected departments through `TERRITORY_CODES`, not for an unconditional national run.

## Phase 9C

- Metropolitan OSM ingestion uses Geofabrik's 22 historical-region extracts instead of the monolithic France PBF. Extracts are processed sequentially, and each regional node map is released before the next file.
- Only node coordinates referenced by supported roads, power lines, buildings, parking areas, and campsites are retained. Relations are not interpreted in this prototype.
- Building occupancy is quantized to H3 resolution 10 in the aggregate cache. The 50-metre WUI test therefore uses those subcell centres and remains an approximate risk proxy rather than cadastral geometry.
- A way included by two adjacent regional extracts may contribute twice near their shared boundary. Avoid using the aggregate as an authoritative infrastructure inventory; national calibration must quantify this small edge effect.
- Regional PBF downloads are explicit because they require several gigabytes and upstream licensing/version awareness. The checksummed script never runs during a build or normal application startup.
- National CORINE GeoTIFF ingestion aggregates sampled land-cover flags by H3 cell. When only aggregate flags are available, WUI uses building and combustible presence in the same resolution-8 cell rather than claiming a precise 50-metre cadastral distance.
- National INSEE Filosofi 200-metre rows are summed by H3 cell during ingestion to bound memory before department feature calculation.

## Human ignition model and national interface

- The operational `human` component is currently an exposure heuristic built from historical ignition density, wildland-urban interface, road density, agricultural land, and calendar multipliers. It does not identify the cause of a future ignition.
- The national BDIFF and Prométhée input files are currently header-only because no stable bulk export was available during deployment. Historical ignition density is therefore zero for the active France profile, and the interface marks these sources as not loaded.
- A causal human-ignition model requires labelled BDIFF records with origin categories such as accidental, malicious, work-related involuntary, private-activity involuntary, and natural. Unknown causes must remain a separate label rather than being imputed as human.
- OSM roads, buildings, activity POIs, and power lines; CORINE agriculture and combustible cover; INSEE population; and school/public-holiday calendars remain proxy features. They can improve ranking after calibration but cannot substitute for labelled ignition causes.
- The national overview intentionally renders a limited set of priority points. Detailed H3 polygons are requested only after zooming, and stale viewport requests are cancelled.
- The dashboard uses a flat, light cartographic visual system. Rounded glass panels, decorative gradients, glows, and duplicated polygon-plus-marker rendering are intentionally avoided.
- National BDIFF synchronization is server-only. The raw portal export exists only in a temporary VPS directory, one normalized current CSV is retained, and PostgreSQL upserts by source identity before refreshing only the historical-density JSON feature.
- The BDIFF synchronizer reproduces a public metropolitan search in a temporary HTTP session before requesting the official ZIP endpoint. The default 2020-to-last-closed-campaign interval favors nationally comparable records and bounds memory use. A monthly timer is sufficient because public records are released after annual validation, not as a real-time feed.
