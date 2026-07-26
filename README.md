# PyroRisk

PyroRisk is a Rust service for hyperlocal wildfire ignition-risk nowcasting and forecasting. It combines physical fire-weather conditions and open-data proxies for human activity on an H3 grid. Phases 0–9C provide ingestion, explainable fusion, persistence, scheduling, APIs, retrospective evaluation, an operator dashboard, live multi-horizon forecasts, cloud deployment, metropolitan-France partitioning, and bounded-memory OSM aggregation.

## Architecture

The Cargo workspace is split into focused crates:

- `fwi`: pure Canadian Fire Weather Index calculations, validated against the standard `cffdrs` sequence.
- `grid`: typed bounding boxes, complete H3 AOI coverage, point projection, and PostgreSQL index conversion.
- `ingest`: source contract plus resilient pollable and one-shot connectors.
- `risk`: pure, configurable, explainable `HeuristicV1` ignition model.
- `store`: PostgreSQL repositories and migrations.
- `api`: read-only Axum HTTP API.
- `engine`: configuration, scheduling, wiring, and executable entry point.

PostgreSQL 16 with PostGIS is the only external service. The engine applies pending SQLx migrations when it starts.

## Quick start

Prerequisites: stable Rust, Docker with Compose, and `curl`.

```sh
cp .env.example .env
docker compose up -d
cargo run -p engine -- run
```

In another terminal:

```sh
curl http://localhost:8080/health
```

The scheduler immediately fetches NASA FIRMS detections and Météo-France AROME/ARPEGE forecasts, then refreshes them on their configured cadence. The response reports database and connector health:

```json
{"status":"ok","db":"ok","sources":[{"id":"firms","last_success":"...","staleness_s":42}]}
```

Stop the service with `Ctrl-C` and PostgreSQL with `docker compose down`. The database is retained in the `pyrorisk-postgres` volume.

## Démarrage rapide

Copiez `.env.example` vers `.env`, lancez PostgreSQL avec `docker compose up -d`, puis démarrez le service avec `cargo run -p engine -- run`. L'endpoint de santé est disponible sur `http://localhost:8080/health`.

## Free cloud deployment

Phase 9A packages PyroRisk as a non-root multi-architecture container for a zero-budget Oracle Cloud ARM deployment. The production Compose stack keeps PostGIS on an internal Docker network, exposes the API through Caddy, and can retain one rolling database backup in Cloudflare R2. GitHub Actions publishes both `linux/amd64` and `linux/arm64` images to GHCR.

Follow the complete procedure in [`deploy/oracle/README.md`](deploy/oracle/README.md). The deployed service deliberately preserves the validated Aude pilot until real national static layers are installed. Phase 9B now supplies the metropolitan-France boundary workflow, 96 department partitions, H3 resolution 8 processing, sequential database batches, and atomic publication.

## Metropolitan France rollout

Download the official Etalab department contours and inspect the exact H3 workload before importing any large source:

```sh
./deploy/oracle/fetch-france-boundaries.sh "$PWD/data/boundaries/departements-1000m.geojson"
TERRITORY_GEOJSON_PATH="$PWD/data/boundaries/departements-1000m.geojson" \
H3_RESOLUTION=8 cargo run --release -p engine -- territory-plan
```

`TERRITORY_CODES=11,34` limits a run to selected departments for progressive imports. With `TERRITORY_GEOJSON_PATH` configured, cell-feature writes, FIRMS-triggered scoring, `forecast`, the scheduler, and `data-status` process unique land cells department by department. One forecast timestamp is hidden from API reads until every requested partition succeeds; the prior complete surface remains visible after an interrupted run.

The broad `AOI_BBOX=-5.15,41.31,9.57,51.09` remains useful for one national FIRMS Area request. It is not used to generate the national H3 surface, so sea and neighbouring countries are excluded. The dashboard reads `/config` and automatically adopts the active territory, bbox, and H3 resolution.

Do not set `DATA_PROFILE=production` for France until the six real national files declared in `.env.production.example` exist. Phase 9C reads the 22 regional Geofabrik extracts sequentially, retains coordinates only for relevant OSM ways, and emits a reusable H3-resolution-8 JSONL cache. Aude fixtures are never representative of France.

## Configuration

Every supported variable is listed in `.env.example`. Defaults are used when variables are absent. The engine validates the AOI coordinate order, H3 resolution (8–10), positive intervals and exponents, and risk weights in the `[0, 1]` range before connecting to PostgreSQL. `DATA_PROFILE=fixture` keeps the reproducible development behavior; `DATA_PROFILE=production` rejects paths under `testdata/`, missing static files, and any static connector failure.

Secrets such as `FIRMS_MAP_KEY` and `METEOFRANCE_API_KEY` are optional until their respective ingestion phases. Never commit `.env` or files under `data/`.

Debug builds use pretty logs. Release builds emit JSON logs. `RUST_LOG` controls filtering in both modes.

## Fire Weather Index library

The `fwi` crate implements the 1987 Canadian Fire Weather Index System as pure `f64` functions. It exposes the individual FFMC, DMC, DC, ISI, BUI, and FWI equations as well as `calculate_daily`, which advances the three persistent moisture codes from one noon observation.

Inputs use degrees Celsius, relative humidity in percent, ten-metre open wind speed in kilometres per hour, 24-hour precipitation in millimetres, and a calendar month. `FwiState::default()` supplies the standard start-up values FFMC 85, DMC 6, and DC 15. Invalid and non-finite values return typed errors.

The 48-day reference sequence is committed in `testdata/fwi_reference.csv`; provenance and the precision-derived comparison tolerance are documented in `testdata/README.md`.

## FIRMS backfill

Run the Phase 2 pipeline after PostgreSQL is healthy:

```sh
cargo run -p engine -- backfill --source firms --days 7
```

When `FIRMS_MAP_KEY` is empty, the connector reads the official sample records in `testdata/firms_viirs_snpp.csv`. When a key is configured, it calls the NASA FIRMS Area CSV API for the configured `AOI_BBOX`. NASA currently limits an Area API request to five days, so longer intervals are split into consecutive windows automatically.

The scheduler and backfill use the same traced import pipeline. Each retrieval creates an `ops.import_batches` row and linked `ops.pipeline_runs` row, retains source fields append-only in `raw.firms_observations`, then writes the unchanged V1 representation to `public.observations` in the same short transaction. A deterministic source key preserves historical public idempotence, while uniqueness by batch and source key prevents accidental duplicates without erasing observations received again in later batches. `public.source_status` remains the V1 operational summary.

Every valid record is projected to the configured H3 resolution. An invalid individual row is retained in `raw` with its parsing error and makes the batch partially successful; an empty response is a successful zero-row batch. The backfill command still writes `out/firms.geojson`, containing one H3 polygon per normalized detection with its source payload under the GeoJSON feature properties.

Inspect the result with:

```sh
jq '.features | length' out/firms.geojson
```

Open `out/firms.geojson` directly in QGIS for visual inspection. The committed sample cluster is near Fos-sur-Mer rather than inside the default Aude AOI; this is documented in `testdata/README.md`.

The persisted FIRMS payload schema is:

```json
{
  "latitude": 43.43767,
  "longitude": 4.89077,
  "bright_ti4": 331.33,
  "scan": 0.39,
  "track": 0.36,
  "satellite": "N",
  "instrument": "VIIRS",
  "confidence": "n",
  "version": "2.0NRT",
  "bright_ti5": 292.82,
  "frp": 4.07,
  "daynight": "N"
}
```

## Weather to FWI

Run the Phase 3 pipeline for the date committed in the SYNOP fixture:

```sh
cargo run -p engine -- recompute --date 2025-07-16
```

Without `METEOFRANCE_API_KEY`, the connector reads `testdata/meteo_france_synop.csv`. With a token, it calls the official Météo-France targeted-observation SYNOP CSV endpoint using OAuth2 bearer authentication. The current endpoint exposes tri-hourly observations over a short rolling window, so real recomputes should target an available current date.

The engine covers the complete configured bounding box with H3 cells, including boundary cells. For each cell it applies inverse-distance weighting with configurable exponent `WEATHER_IDW_POWER` over the four nearest complete stations. The station record nearest 12:00 UTC advances daily FFMC, DMC, DC, and BUI from the exact previous calendar day; absent previous state uses the standard FWI start-up values. ISI and FWI then use the latest available wind observation, so rerunning `recompute` as new observations arrive updates the intraday values without advancing moisture codes twice.

The default Aude bounding box at H3 resolution 9 currently covers 127,018 cells. Inspect one fixture run with:

```sh
docker compose exec postgres psql -U pyrorisk -d pyrorisk -c \
  "SELECT count(*), min(fwi), avg(fwi), max(fwi) FROM fwi_state WHERE date = '2025-07-16';"
```

The persisted weather payload uses Celsius, percent, kilometres per hour, and millimetres:

```json
{
  "station_id": "07747",
  "station_name": "PERPIGNAN",
  "latitude": 42.737167,
  "longitude": 2.872833,
  "temperature_c": 32.0,
  "relative_humidity_pct": 28.0,
  "wind_speed_kmh": 21.6,
  "precipitation_24h_mm": 0.0
}
```

The historical `recompute` path remains available for fixture validation and retrospective work. Operational forecasting uses the separate live AROME/ARPEGE pipeline described below.

## Live forecast horizons

Phase 8 fetches the current Météo-France AROME/ARPEGE forecast through the Open-Meteo Météo-France endpoint. It samples 54 weather anchors over the default Aude AOI, interpolates the four nearest anchors onto all 127,018 H3 resolution-9 cells, and produces four atomic risk surfaces: `nowcast`, `hours_6`, `hours_24`, and `hours_48`.

Run one live forecast manually:

```sh
cargo run -p engine -- forecast
```

The command needs internet access but no additional weather API key. It uses forecast noon states to advance FFMC, DMC, and DC once per day, then recomputes ISI and FWI with the wind at each requested valid time. The previous persisted noon state is reused when available; otherwise the standard FWI initialization is used. Only the newest complete forecast batch is retained to keep operational storage bounded.

NASA FIRMS and weather forecasts have different roles. FIRMS reports recent satellite detections and helps confirm fires that may already be active; it is not itself a future-fire predictor. AROME/ARPEGE supplies the forecast temperature, humidity, precipitation, and wind that drive the future physical risk at `+6 h`, `+24 h`, and `+48 h`. Static human and land-cover factors are then fused with that physical risk.

The live weather source is updated hourly by `engine run`; FIRMS keeps its own polling cadence. Forecast output is current, but its operational quality still depends on replacing sparse demonstrator static fixtures with complete production OSM, CORINE, INSEE, calendar, and ignition-history inputs.

## Human static layers

Run the Phase 4 one-shot pipeline with the committed official-source fixtures:

```sh
cargo run -p engine -- load-static
```

The command independently loads OSM, BDIFF, Prométhée, CORINE Land Cover, INSEE Filosofi 200 m, and the Zone C/public-holiday calendar. In fixture mode, one unavailable source is logged and replaced by zero-valued features without aborting the other loaders. Production mode fails instead of silently publishing incomplete static data. Historical fires are upserted into `ignition_history`, calendar flags into `calendar_days`, and every H3 AOI cell receives one idempotent `cell_static` row.

Each `features` document contains:

```json
{
  "hist": 0.5,
  "wui": 1.0,
  "road": 0.72,
  "agri": 0.0,
  "combustible": true,
  "population": 0.36,
  "poi": 0.25,
  "power_line": 0.0,
  "school_zone": "C"
}
```

Numeric densities are normalized to `[0, 1]` over the AOI. Roads, power lines, and activity POIs include H3 ring 1. Historical ignitions use a ring kernel with weights 1, 0.5, and 0.25 at distances 0, 1, and 2. WUI is set when a building lies within 50 metres of a combustible CORINE sample. Empty and boundary cells remain valid complete documents with zero values.

For real local data, override the Phase 4 paths:

- `OSM_PATH` accepts the normalized fixture CSV, one Geofabrik `.osm.pbf`, a directory of regional PBF extracts, or a generated H3 aggregate `.jsonl` cache.
- `BDIFF_PATH` and `PROMETHEE_PATH` accept normalized public exports with the schema committed in their fixtures.
- `CORINE_PATH` accepts a sampled CSV or a CORINE GeoTIFF. GeoTIFF loading invokes `gdal_translate`, clips to the AOI, and therefore requires GDAL on the host.
- `INSEE_PATH` accepts the official Filosofi 200 m CSV in EPSG:3035 and reprojects grid centres with pure Rust `proj4rs`.
- `CALENDAR_PATH` accepts daily Zone C school/public-holiday flags.

For one regional development run, a PBF can still be loaded directly:

```sh
curl -L https://download.geofabrik.de/europe/france/languedoc-roussillon-latest.osm.pbf \
  -o data/languedoc-roussillon-latest.osm.pbf
OSM_PATH=data/languedoc-roussillon-latest.osm.pbf cargo run -p engine -- load-static
```

For metropolitan France, download the regional extracts explicitly and build the reusable cache before `load-static`:

```sh
./deploy/oracle/fetch-france-osm-regions.sh "$PWD/data/osm/regions"
OSM_PATH="$PWD/data/osm/regions" \
AOI_BBOX=-5.15,41.31,9.57,51.09 \
H3_RESOLUTION=8 \
cargo run --release -p engine -- osm-aggregate \
  --output "$PWD/data/osm/france-h3-r8.jsonl"
```

`OSM_REGIONS=corse` restricts the download script for a small validation run. The cache stores road and power-line lengths, activity-POI counts, and building occupancy at H3 resolution 10 inside each resolution-8 cell.

Inspect the result with:

```sh
docker compose exec postgres psql -U pyrorisk -d pyrorisk -c \
  "SELECT count(*), count(*) FILTER (WHERE (features->>'hist')::float > 0) FROM cell_static;"
```

## Production data readiness

Phase 7 separates demonstrator fixtures from operational static inputs. Start from the production template, place source files under ignored `data/`, then audit both files and PostgreSQL coverage before loading:

```sh
cp .env.production.example .env
cargo run -p engine -- data-status
cargo run -p engine -- load-static
cargo run -p engine -- data-status
```

`data-status` prints each configured source path, whether it is missing, fixture, or ready, and its size. It also reports the AOI or configured territory share with static rows and non-zero road, combustible, population, history, WUI, and agricultural features. These percentages describe feature presence, not accuracy; for example, a valid urban CORINE cell may intentionally be non-combustible.

When `CORINE_PATH` is a GeoTIFF, the audit also checks that `gdal_translate` is installed. The repository does not automatically download national or regional OSM, CORINE, BDIFF, Prométhée, or INSEE archives because their versions, licences, and sizes require an explicit operator choice.

## Risk fusion and API

Phase 5 implements the configurable `HeuristicV1` model. `physical` is `FWI / FWI_MAX` clamped to `[0, 1]`. `human` combines historical ignitions, WUI, road density, and in-season agricultural activity with the configured weights and calendar multiplier. The final score is `physical^RISK_ALPHA * human^RISK_BETA`; a non-combustible cell is always masked to zero. The three strongest weighted inputs are persisted as `top_factors`.

The weather command now triggers risk fusion after updating FWI. With the committed fixtures:

```sh
cargo run -p engine -- load-static
cargo run -p engine -- recompute --date 2025-07-16
cargo run -p engine -- run
```

The running service exposes:

- `GET /health`: database and compact connector freshness.
- `GET /risk?bbox=w,s,e,n&min_score=0&at=latest&horizon=nowcast`: latest H3 polygons as GeoJSON for one horizon.
- `GET /risk/cell/{h3}?horizon=hours_24`: decomposition, forecast-valid FWI, static features, and 24-hour score history.
- `GET /config`: active territory label, dashboard bbox, and H3 resolution.
- `GET /alerts?threshold=0.8&horizon=hours_48`: descending high-risk cells with centroids for one horizon.
- `GET /sources`: latest connector run, success, count, and recent error.
- `WS /stream`: `{ "type": "risk_update", "cells": [...] }` batches. Send `{ "type": "subscribe", "bbox": [w,s,e,n] }` to filter a connection.

Example requests:

```sh
curl 'http://localhost:8080/risk?bbox=2.34,43.20,2.36,43.22&min_score=0&at=latest'
curl 'http://localhost:8080/risk?bbox=2.34,43.20,2.36,43.22&min_score=0&horizon=hours_24'
curl 'http://localhost:8080/alerts?threshold=0.8&horizon=hours_48'
curl 'http://localhost:8080/sources'
```

Accepted horizon values are `nowcast`, `hours_6`, `hours_24`, and `hours_48`; omission defaults to `nowcast`. Every score carries both its computation timestamp and weather-valid timestamp.

The engine polls FIRMS every 30 minutes and refreshes the complete AROME/ARPEGE multi-horizon batch every hour. Both loops run immediately at startup. Source failures update operational status but never terminate the scheduler. A WebSocket nowcast notification tells the dashboard to reload all persisted horizons atomically.

All API errors use `{"error":{"code":"...","message":"..."}}`. `/risk` remains directly consumable by QGIS or another GeoJSON client.

## Operator dashboard

The original project brief required a headless service. After Phase 6, the product owner explicitly requested an interface; that later decision is implemented as a static operational dashboard embedded in the existing Axum binary. The browser contains no scoring logic and only consumes the documented REST and WebSocket contracts.

Start the service and open <http://localhost:8080/>:

```sh
docker compose up -d
cargo run -p engine -- run
```

The dashboard provides:

- an interactive Aude risk map with a configurable minimum-score threshold;
- current maximum risk, visible-cell count, and active-alert count;
- explicit `Maintenant`, `+6 h`, `+24 h`, and `+48 h` forecast controls with valid times;
- prioritized alert navigation and complete cell details;
- physical/human decomposition, FWI components, top factors, and 24-hour history;
- source-health visibility and live WebSocket refreshes;
- a responsive layout for desktop and mobile operations.

The dashboard assets are compiled into the Rust binary. Leaflet 1.9.4 and the CARTO dark basemap are loaded from their public CDNs, so the analytical API continues to work without them but the visual map requires internet access.

## Historical backtest

Phase 6 replays archived Météo-France SYNOP observations day by day, advances an in-memory FWI state for every AOI cell, applies the unchanged production `HeuristicV1`, and compares the resulting ranking with `ignition_history`. It writes `out/backtest_report.md` without persisting retrospective FWI or risk-score rows.

Download the official monthly SYNOP archives for the 2025 summer season plus May for the default 31-day FWI warm-up:

```sh
mkdir -p data/synop
for month in 202505 202506 202507 202508; do
  curl -fL "https://donneespubliques.meteofrance.fr/donnees_libres/Txt/Synop/Archive/synop.${month}.csv.gz" \
    -o "data/synop/synop.${month}.csv.gz"
done
```

The committed BDIFF fixture includes 89 Aude forest-fire alerts from June through August 2025. Load them and run the complete season:

```sh
cargo run -p engine -- load-static
cargo run -p engine -- backtest --from 2025-06-01 --to 2025-08-31 --warmup-days 31
```

## Learned human ignition model

The operational model can replace the original human heuristic with a versioned
regularized logistic model. Training uses only BDIFF records with a known human
cause, excludes unknown and natural causes, never uses the historical-density
feature as an input, and keeps the final calendar year untouched for chronological
validation. Deterministic combustible cell-days provide the non-event controls.

```bash
cargo run -p engine -- train-human-model \
  --train-from 2020-01-01 \
  --train-to 2024-12-31 \
  --validation-from 2025-01-01 \
  --validation-to 2025-12-31
```

The activated output is a relative human-ignition propensity, not an absolute
probability that a person will start a fire. The physical FWI component remains
separate and is fused only for the final operational score. If no valid learned
version exists, the service automatically retains `HeuristicV1`. The monthly VPS
BDIFF synchronization retrains on all closed years and uses the latest closed year
as the untouched holdout.

`BACKTEST_WEATHER_PATH` can point either to the monthly archive directory or to one plain/Gzip official-format CSV. The warm-up advances FFMC, DMC, and DC without scoring those preceding days; set `--warmup-days 0` only for comparison with the original Phase 6 behavior. The report contains an approximate 1,000-bin ROC AUC, the share of actual ignitions captured by the top 5% and top 10% of cells, and the ten worst false negatives. Historical density is rebuilt using only fires strictly before each scored day, so the evaluation period cannot leak into that input.

The reproducible sparse-fixture run evaluates 92 days and 127,018 cells per day after the warm-up. Its near-random AUC and low top-percentile capture are not suitable for model selection because the committed CORINE, OSM, INSEE, and calendar fixtures intentionally sample only a tiny fraction of the AOI. The generated report proposes data and validation follow-ups; the warm-up changes state initialization but not model coefficients or fusion logic.

## Development

Run the project quality checks with:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Database integration checks require the Compose service. Static source files used by later phases belong under `data/`; committed fixtures belong under `testdata/`.
