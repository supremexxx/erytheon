# ERYTHEON — Production map integration contract

## Scope

Phase 4A.4b embeds the existing operational risk map in
`/science/overview`. The integration is presentation-only: it must not add an
endpoint, alter a payload, recalculate a score, change serving, write to the
database or activate the candidate model.

This contract was established by reading the production dashboard source and
observing the deployed dashboard in a real browser on 29 July 2026.

## Existing implementation

| Concern | Production contract |
| --- | --- |
| Library | Leaflet 1.9.4, loaded from `unpkg.com` with the existing SRI hashes |
| Base map | CARTO `light_all` tiles with OpenStreetMap and CARTO attribution |
| Area configuration | `GET /config` returns `bbox`, `territory` and `h3_resolution` |
| Risk surface | `GET /risk` returns a GeoJSON `FeatureCollection` |
| Cell details | `GET /risk/cell/{h3}?horizon=…` returns current score, FWI, factors and history |
| Alerts | `GET /alerts?threshold=…&horizon=…&limit=100` |
| Source health | `GET /health` and `GET /sources` |
| Live update | WebSocket `/stream`, subscription payload `{type:"subscribe", bbox:[west,south,east,north]}` |
| Horizons | `nowcast`, `hours_6`, `hours_24`, `hours_48` |
| Default threshold | `0.10`, adjustable from `0.01` to `0.80` by `0.01` |
| Risk request | `bbox`, `min_score`, `at=latest`, `horizon`, `limit`, `geometry` |
| Overview geometry policy | Zoom ≤ 7: up to 2,000 points; zoom > 7: up to 5,000 polygons |
| Colour thresholds | low `<0.25`, moderate `<0.50`, high `<0.75`, critical `≥0.75` |
| Initial bounds | Fallback operational AOI, then the `/config` bounding box |
| Map zoom | 5–15, zoom controls at bottom-right, canvas preferred |

## Behaviour that must be preserved

1. The visible map bounds are clipped to the configured operational area before
   querying `/risk`.
2. A pending risk request is aborted when a newer map, threshold or horizon
   request supersedes it.
3. At low zoom, prioritised point markers replace polygons to bound rendering
   cost.
4. Moving or zooming the map schedules a debounced risk refresh and renews the
   WebSocket subscription.
5. Hovering a risk feature increases its visual emphasis without changing its
   score.
6. Selecting a risk feature or an alert fetches the existing cell detail and
   opens the existing read-only drawer.
7. The map reports loading, synchronized, lightened/truncated, empty and error
   states.
8. The map remains usable if scientific overview requests and operational map
   requests complete in a different order.
9. Destroying or replacing the overview must abort fetches, clear timers,
   detach listeners, close the WebSocket and remove the Leaflet instance.
10. Returning to Overview must create one fresh map instance, invalidate its
    size after layout, and must not duplicate controls, listeners or sockets.

## Integration design

The production map logic is exposed as a shared browser component by
`/dashboard.js`:

```text
window.ErytheonOperationalMap.mount(options) → controller
controller.refresh()
controller.resize()
controller.destroy()
```

The production dashboard keeps its current automatic boot behaviour when
`#map` and the complete operational controls are present. The science console
loads the same asset and mounts it with its own scoped element references.
There is one risk-fetching implementation and one map lifecycle.

The science console may adapt labels and the surrounding visual composition,
but it must not fork the request builder, scoring colours, feature rendering,
cell selection logic or WebSocket protocol.

## Science overview element contract

The overview provides scoped equivalents for:

- map host;
- map status and cell count;
- loading and empty states;
- four horizon buttons;
- selected horizon validity;
- threshold input and output;
- visible-cell and maximum-score summaries;
- detail drawer, FWI grid, factors and history;
- explicit refresh and close controls.

Operational-only sidebar features such as the full alerts and source lists are
optional in the scientific overview. If absent, the shared component must skip
them without error.

## Lifecycle contract for the single-page console

`science.js` owns the route lifecycle:

1. before rendering another page, call the current map controller’s
   `destroy()`;
2. render the new page;
3. when the new route is Overview, mount the shared map after the DOM is
   connected;
4. call `resize()` after the panel has received its final dimensions;
5. on `pagehide`, destroy the controller.

No map object may survive outside the Overview route.

## Security and data rules

- All map APIs remain same-origin.
- No authorization header, credential, server path or secret is added to the
  static assets.
- The science console remains read-only.
- The Basic Auth and deployment gate remain outside the frontend component.
- User-derived and API-derived text inserted into HTML is escaped by the
  existing escape function.
- No operational response is persisted in local or session storage.

## Failure modes and required UI

| Failure | Required result |
| --- | --- |
| Leaflet CDN unavailable | Stable map panel with an explicit library-unavailable message |
| `/config` unavailable | Keep fallback bounds, flag configuration failure, still attempt operational data |
| `/risk` unavailable | Clear stale risk layers; show the API error in the map empty state |
| Empty GeoJSON | Show honest “no cell at this threshold” guidance |
| Detail unavailable | Keep drawer open and report that the detail is unavailable |
| WebSocket unavailable | Keep HTTP map functional and report reconnecting/degraded live state |
| Route leaves Overview | Abort requests and destroy Leaflet, listeners, timers and socket |
| Container resizes | Invalidate Leaflet size without remounting or duplicating controls |

## Browser evidence captured before implementation

- Production dashboard loaded with no console error.
- `/config`, `/alerts`, `/health`, `/sources` and `/risk` returned HTTP 200.
- The low-zoom risk request used `limit=2000&geometry=point`.
- Selecting `+24 h` changed the active horizon to `hours_24`.
- Selecting the first alert opened the detail drawer and loaded a matching
  `+24 h` cell detail.
- The initial and loaded production-map screenshots are stored under the
  ignored `output/playwright/` validation directory.
