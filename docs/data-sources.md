# Data sources

FireSift's **code** is dual-licensed MIT OR Apache-2.0 (see
[`COPYRIGHT`](../COPYRIGHT)). That license does not extend to the third-party
data FireSift reads, computes over, or displays. Each source below has its
own license and its own attribution and redistribution rules, and they are
not all the same. This document exists so that a contributor, a downstream
user, or a future maintainer can answer "am I allowed to do X with this
data?" without having to reverse-engineer it from ingestion code.

None of these entries should be treated as legal advice. Where a license is
marked `REQUIRES LEGAL / LICENSE REVIEW`, do not assume redistribution is
safe — check with the current terms of the provider, or with counsel, before
publishing derived data.

FireSift does not commit real production datasets to this repository (see
[Fixtures vs. real data](#fixtures-vs-real-data) below). Discussion of
"redistribution" below concerns what a deployer downloads and stores
locally, not what ships in Git.

## Status summary

Verified 2026-08-17 against each provider's current published terms (see
per-source sections below for sources checked and exact wording).

| Source | Status | Notes |
|---|---|---|
| NASA FIRMS | CLEAR | Free use, attribution requested |
| Météo-France | CLEAR | Etalab Licence Ouverte 2.0 |
| ECMWF IFS Open Data | CLEAR | CC-BY-4.0, fully open since 2025-10-01 |
| Open-Meteo | OPTIONAL PROVIDER | Data is CC-BY-4.0; the *free API* is non-commercial-use only — see [Open-Meteo](#open-meteo) |
| BDIFF | CLEAR for non-commercial reuse | Commercial/advertising reuse requires prior request to the Ministry — see [BDIFF](#bdiff-base-de-données-incendies-de-forêt-en-france) |
| Prométhée | NOT BUNDLED | Resale explicitly prohibited; usage must be declared; merged into BDIFF in 2023 |
| OpenStreetMap | CLEAR, with conditions | ODbL 1.0 — share-alike risk for derived databases |
| CORINE Land Cover | CLEAR | Copernicus full/open/free access policy, attribution expected |
| INSEE (Filosofi) | CLEAR | Etalab Licence Ouverte 2.0 |
| Administrative boundaries | NOT BUNDLED / USER PROVIDED | No boundary file ships in this repository; deployer supplies and licenses their own — see [Administrative boundaries](#administrative-boundaries) |
| Territorial calendars | CLEAR (bundled fixture) / NOT BUNDLED (production) | Fixture is 3 rows of public factual calendar data; a real production calendar is user-provided — see [Territorial calendars](#territorial-calendars) |

## Satellite fire detection

### NASA FIRMS (Fire Information for Resource Management System)

- **Provider**: NASA / University of Maryland, via the LANCE/FIRMS API.
- **Usage in FireSift**: near-real-time VIIRS (S-NPP) thermal anomaly
  detections, ingested via `crates/ingest/src/firms.rs`
  (`https://firms.modaps.eosdis.nasa.gov/api/area/csv`).
- **Access**: requires a free `FIRMS_MAP_KEY` (`.env.example`), obtained by
  registering at <https://firms.modaps.eosdis.nasa.gov/api/>.
- **License**: `CLEAR`. NASA FIRMS data is provided for free use, with
  attribution requested to NASA FIRMS/LANCE. See
  <https://www.earthdata.nasa.gov/data/tools/firms>. NASA/US-government-produced
  data is not subject to US copyright, but attribution to FIRMS/LANCE is
  the expected practice and is what FireSift follows.
- **Redistribution**: not committed to this repository (a small anonymized
  fixture is used for local development — see `testdata/`).
- **Important limitation**: FIRMS reports observed thermal anomalies, not
  predictions. It tells FireSift a fire (or another heat source) was
  detected recently; it says nothing by itself about future ignitions.

## Weather

### Météo-France

- **Provider**: Météo-France, the French national meteorological service.
- **Usage**: SYNOP station observations (`crates/ingest/src/meteo_france.rs`,
  `https://public-api.meteofrance.fr/public/DPObs/v1/synop`).
- **Access**: requires a `METEOFRANCE_API_KEY` obtained via
  <https://portail-api.meteofrance.fr/>.
- **License**: `CLEAR`. Météo-France open data is published under
  **Licence Ouverte / Open Licence 2.0 (Etalab)**, which grants a
  non-exclusive, free, worldwide, unlimited-duration right to reuse the
  data — including commercial reuse — with attribution. See
  <https://www.data.gouv.fr/pages/legal/licences/etalab-2.0> for the full
  license text and <https://meteofrance.com/mentions-legales> for the
  Météo-France-specific statement.
- **Redistribution**: not committed; a fixture (`testdata/meteo_france_synop.csv`)
  is used for local development.

### ECMWF IFS Open Data

- **Provider**: European Centre for Medium-Range Weather Forecasts.
- **Usage**: direct, credential-free acquisition of open forecast GRIB2
  data (`crates/ingest/src/ecmwf_open.rs`,
  `https://data.ecmwf.int/forecasts`).
- **License**: `CLEAR`. ECMWF's entire Real-time Catalogue (including the
  IFS forecasts FireSift reads) has been published under **CC-BY-4.0**
  since ECMWF completed its transition to fully open data on
  **2025-10-01** — full native resolution, no data cost. See
  <https://www.ecmwf.int/en/forecasts/datasets/open-data> and
  <https://www.ecmwf.int/en/about/media-centre/news/2025/ecmwf-makes-its-entire-real-time-catalogue-open-all>.
  Attribution example from ECMWF's own guidance: *"Adapted from 'ECMWF IFS
  Forecast Data' by ECMWF, licensed under CC BY 4.0, available at
  <https://data.ecmwf.int/forecasts/>."* This is FireSift's **primary**
  weather-forecast source (FireSift reads it credential-free).
- **Redistribution**: raw GRIB2 caches are not committed (`.gitignore`
  excludes `data/`); attribution to ECMWF should be preserved wherever
  derived forecast values are shown.

### Open-Meteo

- **Provider**: Open-Meteo, used as a **controlled, optional fallback**
  when ECMWF direct access or Météo-France observations are unavailable
  (`crates/ingest/src/open_meteo.rs`,
  `https://api.open-meteo.com/v1/forecast` and
  `https://api.open-meteo.com/v1/meteofrance`).
- **License**: `OPTIONAL PROVIDER` — two separate things are true at once,
  and they should not be conflated:
  1. The **data itself** is CC-BY-4.0 (share and adapt, including
     commercially, with attribution) — see
     <https://open-meteo.com/en/licence>.
  2. The **free API service** that serves that data is contractually
     restricted to non-commercial use — private/non-profit sites with no
     subscriptions or ads, personal automation, research, education — with
     rate limits (600 calls/min, 5,000/hour, 10,000/day, 300,000/month).
     Commercial deployment through the free tier is explicitly prohibited
     by Open-Meteo's terms; it requires a paid plan (Standard/Professional/
     Enterprise). See <https://open-meteo.com/en/terms>.
  FireSift calling the free API is only within terms for non-commercial
  use of the API service — which matches FireSift's current
  non-commercial research positioning (see root README), but **is not
  automatically true for every deployer**. Anyone standing up a
  commercial or ad-supported FireSift deployment must either stop routing
  through the free Open-Meteo fallback or obtain a paid Open-Meteo plan —
  FireSift's own MIT/Apache-2.0 code license does not make that decision
  for you. The API service's server code itself is separately licensed
  AGPLv3 by Open-Meteo (irrelevant to FireSift, which only calls the
  hosted API — no Open-Meteo source is vendored here).
- **Recommendation**: keep `OPEN_METEO_*`/fallback usage explicit and
  configurable (already the case — it only activates when ECMWF/Météo-France
  are unavailable), and do not present it in documentation as an
  unconditional free production backend. See
  [Open-Meteo framing](../README.md#data-sources) in the root README for
  the corresponding user-facing wording.

## Historical fire events

### BDIFF (Base de Données Incendies de Forêt en France)

- **Provider**: French Ministry of Agriculture
  (`bdiff.agriculture.gouv.fr`).
- **Usage**: historical wildfire event records used as positive labels for
  the human-ignition dataset (`crates/ingest/src/bdiff.rs`; see
  [`research/reports/BDIFF_PIPELINE.md`](research/reports/BDIFF_PIPELINE.md)
  and [`research/reports/BDIFF_QUALITY.md`](research/reports/BDIFF_QUALITY.md)
  for the full ingestion and quality pipeline).
- **License**: `CLEAR for non-commercial reuse`. BDIFF is produced by the
  French Ministry of Agriculture (data itself compiled by IGN). Its own
  mentions légales (<https://bdiff.agriculture.gouv.fr/mentions-legales>,
  checked 2026-08-17) state: *"Toute utilisation, reproduction ou
  réutilisation des données du site à des fins commerciales ou
  publicitaires doit faire l'objet d'une demande préalable et pourra
  donner lieu à l'établissement d'une convention ou à l'octroi d'une
  licence"* — i.e. **commercial or advertising reuse requires a prior
  request** and may require a specific convention/license from the
  Ministry; it is not a blanket Etalab Licence Ouverte grant the way
  Météo-France's or INSEE's data is. Non-commercial, research use — which
  is FireSift's current positioning (see root README) — is not subject to
  that restriction. **If FireSift or a fork of it is ever positioned
  commercially again, BDIFF reuse terms must be re-checked and a prior
  request made before continuing to use BDIFF data.**
- **Redistribution**: not committed; a small anonymized fixture
  (`testdata/bdiff_aude.csv`) covers one pilot department (Aude) for local
  development and tests only.

### Prométhée

- **Provider**: Prométhée wildfire database (Mediterranean France, regional
  forest-fire prevention entities). **Merged into BDIFF in early 2023** —
  Prométhée is now a legacy/historical source rather than an actively
  maintained separate one; BDIFF should be treated as the primary,
  forward-looking historical fire-event source for new work.
- **Usage**: supplementary historical fire records
  (`PROMETHEE_PATH` in `.env.example`).
- **License**: `NOT BUNDLED`. Confirmed (2026-08-17): **resale of
  Prométhée data is explicitly prohibited**, and use must be declared via
  the Prométhée portal's contact form before reuse. No dataset from this
  source is bundled in this repository beyond the small pilot fixture
  described below, and no derived dataset built from real Prométhée data
  should be redistributed without first declaring that use as required.
- **Redistribution**: not committed. `testdata/promethee_aude.csv` is a
  **synthetic** single-row fixture — fictional municipality
  ("Testville-sur-Aude"), a `SYNTH-`-prefixed `external_id`, and
  round-number coordinates chosen to be obviously not a real record. It
  replaces an earlier version of this fixture whose provenance (hand-built
  vs. derived from a real historical export) could not be confirmed; that
  uncertainty is now moot since the file contains no real data at all. No
  declaration or resale restriction applies to it.

## Territorial / static features

### OpenStreetMap (OSM)

- **Provider**: OpenStreetMap contributors, via Geofabrik PBF extracts.
- **Usage**: road, POI, and power-line density features
  (`crates/ingest/src/osm.rs`).
- **License**: `CLEAR, with conditions` — **Open Database License (ODbL)
  1.0**. Attribution to
  "© OpenStreetMap contributors" is required; derived datasets that are
  themselves databases may trigger ODbL's share-alike clause — review
  before distributing a derived geospatial dataset built from OSM.
- **Redistribution**: only a small extracted CSV fixture
  (`testdata/osm_features.csv`) is committed for local development.

### CORINE Land Cover

- **Provider**: European Environment Agency (EEA) / Copernicus Land
  Monitoring Service.
- **Usage**: land-cover / vegetation combustibility classification
  (`crates/ingest/src/corine.rs`).
- **License**: `CLEAR`. Copernicus data policy is "full, open and free
  access" — confirmed 2026-08-17 against
  <https://land.copernicus.eu/en/clc35> and
  <https://www.copernicus.eu/en/use-cases/corine-land-cover>. Attribution
  (e.g. "Contains modified Copernicus Land Monitoring Service information
  [year]") is expected practice under the Copernicus license; see
  <https://land.copernicus.eu/en/data-policy> for the exact wording.
- **Redistribution**: not committed; `testdata/corine_aude.csv` is a small
  pilot-area fixture only.

### INSEE (Institut National de la Statistique et des Études Économiques)

- **Provider**: INSEE, French national statistics institute.
- **Usage**: population density (Filosofi 200m grid) used as a static
  feature (`crates/ingest/src/insee.rs`).
- **License**: `CLEAR`. Confirmed 2026-08-17: INSEE Filosofi carroyée data
  published on data.gouv.fr under **Etalab Licence Ouverte 2.0** — free
  reuse, including commercial, with attribution (not legally mandatory
  under LO 2.0, but expected practice). See
  <https://www.data.gouv.fr/pages/legal/licences/etalab-2.0> and
  <https://www.insee.fr/fr/information/2410994> for the INSEE-specific
  statement.
- **Redistribution**: not committed; `testdata/insee_filosofi_200m.csv` is
  a pilot fixture only.

### Administrative boundaries

- **Provider**: not fixed by FireSift — typically IGN administrative
  boundaries (e.g. ADMIN EXPRESS) in a French deployment, but the code
  does not pin a specific provider, file, or version.
- **Usage**: `TERRITORY_GEOJSON_PATH`.
- **License**: `NOT BUNDLED / USER PROVIDED`. Confirmed (2026-08-17): no
  boundary file ships in this repository or in any `.env*.example` —
  `TERRITORY_GEOJSON_PATH` defaults to empty in `.env.example` and
  `deploy/oracle/.env.example`, and `.env.production.example` merely shows
  an *illustrative* path (`data/boundaries/departements-1000m.geojson`)
  under the gitignored `data/` directory, not a real committed file. There
  is therefore no license to review here — FireSift ships no boundary
  data at all. **Whoever configures `TERRITORY_GEOJSON_PATH` for a real
  deployment supplies their own boundary file and is responsible for
  confirming its license** (IGN's Etalab Licence Ouverte 2.0 is the
  typical case, matching the pattern confirmed for Météo-France and INSEE
  above, but must be checked against whichever specific file is actually
  used).

### Territorial calendars

- **Provider**: French school-zone and public-holiday calendars — purely
  factual information (a date, a holiday name), not a copyrightable
  dataset in the way satellite imagery or a curated fire-event database
  is.
- **Usage**: `CALENDAR_PATH`.
- **License**: `CLEAR` for the bundled fixture. **Redistribution**:
  `testdata/calendar_zone_c.csv` is committed — 3 rows, each just a date,
  a boolean school/public-holiday flag, and a plain-language holiday
  label ("Vacances d'Été", "14 juillet"). This is public factual
  information, not a licensed third-party dataset, so no attribution or
  redistribution restriction applies to it. A real deployment's full
  calendar (`data/calendar/france.csv` in `.env.production.example`) is
  `NOT BUNDLED / USER PROVIDED` — not committed, and whoever builds a
  full production calendar file is responsible for whatever source they
  compile it from.

## Fixtures vs. real data

Everything under `testdata/` is a small, hand-trimmed sample meant only for
local development, CI, and demos — never a claim about real-world fire risk.
See the `.gitignore` entries for `data/`, `out/`, `*.dump`, and similar:
real production datasets, database dumps, and trained-model caches are
never intended to be committed, regardless of source license. A contributor
who needs real data should acquire it directly from the provider using the
credentials and paths documented in `.env.production.example`.
