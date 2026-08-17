# ERYTHEON — Phase 4A.4 premium dashboard report

## Conclusion

Phase 4A.4 reproduces the validated dashboard composition as a dense, premium
scientific command surface while preserving ERYTHEON's real data, scientific
limits and read-only behaviour.

```text
PHASE 4A.4 PREMIUM DASHBOARD REDESIGN COMPLETED
REFERENCE VISUAL STRUCTURE REPRODUCED
REAL ERYTHEON DATA ONLY
FRONT-END PRESENTATION ONLY
NO API CHANGE
NO DATABASE CHANGE
NO MODEL CHANGE
NO SCORING CHANGE
NO MIGRATION
READ-ONLY BEHAVIOR PRESERVED
ACCESS CONTROL PRESERVED
V1 REMAINS ACTIVE
CANDIDATE REMAINS INACTIVE
NO SHADOW SCORING
```

No production deployment is part of this report.

## Git integration prerequisite

The Phase 4A.3 PR was inspected before the new dashboard branch was realigned:

- scope: seven declared stabilization and documentation files;
- secrets/paths: no credential value or local user path tracked;
- executable changes: responsive table containment, keyboard tooltips, honest
  empty registry state and regression coverage;
- CI: two successful runs;
- state before merge: clean and mergeable.

PR #3 was moved from draft to ready and merged normally into `main`:

```text
main integration commit: 45014c5e3cfcc4309283012393873e459622638b
```

The dashboard branch was then rebased on that exact integrated state:

```text
agent/phase4a4-premium-scientific-dashboard
```

No tag was moved and no deployment was triggered by the integration.

## Scope

Application files:

```text
crates/api/static/science/index.html
crates/api/static/science/science.css
crates/api/static/science/science.js
crates/api/static/science/phases.json
```

Documentation:

```text
PHASE4A4_PREMIUM_DASHBOARD_REPORT.md
SCIENTIFIC_DASHBOARD_DESIGN_SYSTEM.md
SCIENTIFIC_DASHBOARD_COMPONENT_MAP.md
```

No file changed under:

```text
crates/api/src
crates/store/src
crates/engine/src
crates/risk
crates/fwi
crates/ingest
migrations
deploy
```

## Reference-to-data audit

The implementation contract is recorded in
`SCIENTIFIC_DASHBOARD_COMPONENT_MAP.md`.

Key decisions:

| Reference element | ERYTHEON implementation |
|---|---|
| topbar | real app/DB/model/source context |
| six KPI cards | weather timestamp, FIRMS, BDIFF, cells, active v1, inactive candidate |
| main map | explicit unavailable state; no fictitious territory or FWI |
| physical risk drivers | explicit API limitation; no synthetic weather values or trends |
| high-risk locations | replaced by recent real ignition events |
| system journal | real import batches and pipeline runs |
| territory summary | H3 vocabulary plus real cell/event/snapshot counts |
| risk donut | real BDIFF cause distribution |
| quality gauge | real bars; no invented global score |
| model comparison | real version-controlled Phase 3B.8 values |
| calibration curve | explicit unavailable state because points are not exposed |

Deferred to Phase 4B:

- geographic map and H3 exploration;
- FWI and physical-driver observations;
- ROC, precision-recall and calibration curves;
- exposed population, losses, alerts and risk-ranked locations;
- any component requiring a new endpoint, query or model execution.

## Observed validation data

The final browser validation used the current production read-only APIs through
an SSH tunnel while serving the local static assets. Production code,
configuration and data were not modified.

Representative observations at validation time:

```text
application / PostgreSQL    ok / ok
BDIFF active events        15 956
human known                 7 094
natural known                 791
unknown                     8 071
FIRMS observations         36 265
static cells              920 016
migrations applied             17
active model                    1 (v1)
candidate                       1 (inactive)
dataset versions                0
feature snapshots               0
```

These values are not embedded in the frontend. They came from the existing API
responses and can change with the environment.

## Visual result

### Application shell

- 176 px fixed scientific navigation;
- 62 px segmented technical topbar;
- ERYTHEON monochrome mark;
- compact implemented shortcuts only;
- factual API/read-only footer;
- mobile drawer with named 40 px control.

### Overview

- six compact live metrics;
- 9/3 primary/context split at 1,440 px;
- large spatial state with real territorial counts;
- interpretation limits table;
- recent ignition event table;
- chronological import/pipeline journal;
- real model comparison and comparable population;
- territory, cause, quality and system context panels.

### Secondary routes

- Sources: freshness strip, complete source table, import and pipeline histories.
- Data Quality: cause donut, geographic and combustibility bars, event audit table.
- Features: snapshot catalogue and honest historical calendar states.
- Datasets: registry metrics and explicit empty production registry.
- Models: institutional active/inactive status, comparison, artifacts and limits.
- System: technical metrics, component integrity and source facts.
- Progression: compact versioned journal including completed 4A.3 and current 4A.4.

## Security and functional invariants

- every scientific network request observed in Chromium used `GET`;
- response caching is in-memory and keyed by the existing URL;
- API data is escaped before HTML insertion;
- tooltip text uses `textContent`;
- a hostile 503 message containing an HTML image/event-handler string was shown
  literally and did not create a DOM node or execute script;
- no credential, authorization header or remote asset is present in the source or
  captures;
- no write control exists in the UI.

The following remain unchanged:

```text
13 API routes
existing JSON contracts
existing SQL
17 database migrations
v1 serving
inactive candidate registry
scheduler and source ingestion
HTTP Basic protection
read-only console behavior
```

## Browser validation

Real Chromium matrix:

```text
8 routes × 4 viewports = 32 checks
1440 × 900
1280 × 800
1024 × 768
375 × 812
```

Results:

- 32/32 shell responses returned 200;
- 32/32 expected page headings rendered;
- zero console errors and zero page errors;
- zero UI error/loading residue;
- zero document-level horizontal overflow;
- exactly one active navigation item;
- every table remained in a keyboard-focusable local scroll region;
- every header cell exposed `scope="col"`;
- only GET requests reached `/api/science/*`.

Additional checks:

- missing dataset detail produced an honest escaped error;
- simulated 503 produced an escaped error and recovered after route restoration;
- hostile HTML remained text and did not execute;
- scientific tooltip opened on keyboard focus, remained linked by
  `aria-describedby` and stayed inside the mobile viewport;
- mobile drawer opened, updated its accessible name, navigated client-side and
  closed after route selection;
- the mobile control meets the 40 px touch-target requirement.

## Required captures

The following ignored local artifacts were produced from real API responses:

```text
output/playwright/phase4a4-premium/overview-final-1440x900.png
output/playwright/phase4a4-premium/overview-final-1280x800.png
output/playwright/phase4a4-premium/overview-final-1024x768.png
output/playwright/phase4a4-premium/overview-final-375x812.png
output/playwright/phase4a4-premium/sources-final-1440x900.png
output/playwright/phase4a4-premium/data-quality-final-1440x900.png
output/playwright/phase4a4-premium/datasets-final-1440x900.png
output/playwright/phase4a4-premium/models-final-1440x900.png
output/playwright/phase4a4-premium/system-final-1440x900.png
```

The screenshots contain no credential and are intentionally not committed.

## Quality gate

Executed successfully:

```text
node --check crates/api/static/science/science.js
jq empty crates/api/static/science/phases.json
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
```

The workspace tests used a disposable local PostGIS instance. The scientific
integration suite executed 10/10 tests with zero ignored.

## Performance

- no framework or chart library;
- no external font, CDN or image request;
- no remote visual asset;
- SVG/CSS-only local graphics;
- existing JSON endpoints only;
- one in-memory request per unique endpoint URL during a navigation session.

## Deployment and rollback

Deployment status:

```text
not deployed
production remains on Phase 4A.3 application code
database unchanged
Caddy unchanged
```

The review branch can be rolled back by reverting only the static asset and
documentation commits. No database rollback, migration rollback, model action or
container data operation is required.

If deployed later, the existing controlled procedure must recreate only the
application container after CI and image verification. PostgreSQL, Caddy and
volumes must remain untouched.

## Preserved local files

The unrelated pre-existing untracked files below were left untouched:

```text
PR1_INTEGRATION_REVIEW_REPORT.md
rust-toolchain 2.toml
```
