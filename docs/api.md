# API

All endpoints are unauthenticated `GET`/`WS` reads unless noted. There is
no write, import, training, migration, or model-activation endpoint
anywhere in the HTTP API — every route in `crates/api/src` is registered
with `get(...)`. Stability tiers below reflect actual project maturity;
do not treat `experimental` routes as stable contracts.

## Operational (stable-ish, pre-1.0)

| Endpoint | Description |
|---|---|
| `GET /health` | Service and data-source health, backed by a live database check |
| `GET /` | Operational dashboard (HTML) |
| `GET /config` | Public runtime configuration |
| `GET /risk` | Risk surfaces as GeoJSON, over the configured AOI, for a given horizon |
| `GET /risk/cell/{h3}` | Explained risk for a single H3 cell (factor breakdown) |
| `GET /alerts` | Cells exceeding a configured risk threshold |
| `GET /sources` | Per-source ingestion/freshness status |
| `WS /stream` | Live risk-update push |

`nowcast`, `+6h`, `+24h`, and `+48h` horizons are supported where a
`horizon` parameter is accepted; see `crates/risk` for the `Horizon` enum.

## Scientific console — `experimental`, disabled by default

Mounted only when `SCIENCE_CONSOLE_ENABLED=true`; all routes under
`/api/science/*` are read-only (`overview`, `progress`, `sources`,
`imports`, `pipelines`, `data-quality`, `data-quality/events`, `features`,
`calendar`, `datasets`, `datasets/{logical_id}`, `models`, `system`,
`observability/latest`, `observability/history`, `observability/compare`,
`observability/attempts`, `snapshots`, `snapshots/{id}`,
`snapshots/{id}/verification`, `snapshot-labels/summary`,
`snapshot-alerts`). See
[`docs/research/reports/SCIENTIFIC_CONSOLE_DATA_CONTRACTS.md`](research/reports/SCIENTIFIC_CONSOLE_DATA_CONTRACTS.md)
for response shapes. `SCIENCE_CONSOLE_ENABLED` is a deployment lock, not an
authentication mechanism — a public deployment must put its own
authentication (e.g. a reverse-proxy basic-auth layer) in front of it if it
is enabled and not meant to be fully public.

## Territorial console — `experimental`, disabled by default

`/client`, `/client/{*path}`, and `/api/client/*` (including
`/api/client/communes/{insee_code}` and
`/api/client/communes/{insee_code}/risk`) provide a read-only view scoped
to a single commune, resolved generically by INSEE code. See
[`docs/architecture.md`](architecture.md#api-surfaces) for framing notes.

## Internal / not part of the public contract

Anything under `crates/api/src/static` (dashboard/science/client HTML, CSS,
JS assets) is served as-is and is an implementation detail of the bundled
UI, not a documented API.

## Stability

This project is pre-1.0 (`v0.4.x`). No endpoint listed above is guaranteed
stable across releases yet; breaking changes will be called out in
[`CHANGELOG.md`](../CHANGELOG.md), not silently shipped.
