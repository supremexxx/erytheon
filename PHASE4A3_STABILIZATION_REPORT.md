# ERYTHEON — Phase 4A.3 stabilization report

Audit performed on 2026-07-28 UTC from branch
`agent/phase4a3-science-console-stabilization`.

## Executive conclusion

The private scientific console is read-only, access-controlled, consistent with the
production database and comfortably within its latency targets. The audit found no
candidate activation, candidate scoring, shadow scoring, v1 change, migration or
database write attributable to the console.

Four observed P2 presentation defects were corrected:

- wide tables could overflow the page instead of scrolling locally;
- scientific definition tooltips were not usable from the keyboard;
- an empty dataset registry was described as `draft / validated`;
- the progress catalogue omitted phases 4A.1–4A.3 and retained one resolved risk.

The production scheduler is alive and FIRMS continues to run. Open-Meteo forecasting
is, however, in a state of **DEGRADED DATA FRESHNESS** after repeated rate limiting.
This is not caused by the console and requires a separate operational investigation;
the scheduler was deliberately not changed here.

## 1. Initial state and Git

| Item | Observed value |
|---|---|
| `main` at branch creation | `795f0729d901bc0e8056ade12e701796019f5145` |
| Application release tag | `v0.4.2-app` → `849039385a14f95df0a95cca69e5987d3b311478` |
| Documentation release tag | `v0.4.2` → `d4730fda09571db491d0611e3308f96d1ee03ebb` |
| Production application revision | `849039385a14f95df0a95cca69e5987d3b311478` |
| Initial production image | `erytheon:phase4a2-science-84903938` |
| Initial image ID | `sha256:08f813aff108…` |
| Working tree at start | two pre-existing untracked user files, left untouched |

No history was rewritten and neither existing release tag was moved.

## 2. Production baseline

Baseline captured at `2026-07-28T22:42:15Z` before any application deployment or
service restart.

| Service | Container ID | State | Restarts | Started |
|---|---|---:|---:|---|
| Application | `784af51534…` | running, healthy | 0 | `2026-07-28T19:35:05.421194257Z` |
| PostgreSQL | `fbecc890…` | running, healthy | 0 | unchanged during audit |
| Caddy | `b68874…` initially | running; config valid | 0 | unchanged at baseline |

Approximate resource use was 31 MiB for the application, 202 MiB for PostgreSQL and
13 MiB for Caddy. The 96 GiB filesystem was 54% used. No resource pressure was
observed.

During the authenticated browser audit, an audit command printed the encoded
Authorization header in local tool output. This was treated as a P0 credential
exposure even though it did not enter Git or an application log. The affected
credential was immediately rotated, the old credential was verified rejected, the
new credential was verified accepted and the anonymous response remained 401.
Only Caddy was recreated (`a6eeb463f730…`, started
`2026-07-28T22:48:16Z`); the application and database were not restarted. No secret,
hash or identifying request data is reproduced in this report.

## 3. Code and route audit

The router exposes thirteen GET-only API routes. The store implementation contains
only `SELECT` statements/read aggregates. It has no model mutation, candidate
artifact loading into a scoring path, scoring call or write endpoint. Unsupported
HTTP methods return 405.

Limits supplied by clients are clamped to 1–200. Paginated queries use deterministic
descending date order plus `LIMIT`/`OFFSET`. No N+1 query pattern was found.

| Route | SQL source / tables read | Filters, order and limit | SQL calls | Measured p95 | Risk |
|---|---|---|---:|---:|---|
| `/api/science/overview` | migrations, active v1, candidate registry, ignition events, FIRMS, cells, snapshots, datasets/builds | latest candidate; active events | 12 incl. health | 238.5 ms | low; live aggregates |
| `/api/science/progress` | versioned `phases.json` | file read | 0 | 25.0 ms | low; catalogue can become stale |
| `/api/science/sources` | `source_status` + `reference.data_sources` | source ID ascending | 1 | 28.1 ms | low |
| `/api/science/imports` | `ops.import_batches` + source reference | optional source/status; newest first; 1–200 | 1 | 28.7 ms | low |
| `/api/science/pipelines` | `ops.pipeline_runs` | optional pipeline/status; newest first; 1–200 | 1 | 28.7 ms | low |
| `/api/science/data-quality` | active events and validation quality tables | grouped aggregates | 7 | 61.2 ms | low |
| `/api/science/data-quality/events` | `fire.ignition_events` | active; optional cause; newest first; 1–200 | 1 | 27.1 ms | low |
| `/api/science/features` | snapshots + calendar tables | snapshot family/date order | 6 | 39.1 ms | low |
| `/api/science/calendar` | calendar rule/day tables | active rule and aggregates | 5 | 26.0 ms | low |
| `/api/science/datasets` | `ml.dataset_versions` | newest first | 1 | 23.0 ms | low |
| `/api/science/datasets/{logical_id}` | version, rows, exclusions, builds | exact logical ID; grouped detail | 4 when found | not material; 404 tested | low |
| `/api/science/models` | active v1 + latest candidate registry | active/latest | 2 | 28.9 ms | low; 88,950-byte artifact payload |
| `/api/science/system` | migrations, models, cells, events, datasets, source status | latest source successes | 9 | 110.4 ms | low |

The models endpoint serializes the complete candidate artifact and is therefore much
larger than the other responses. It remained fast and is not a correctness problem;
payload slimming is a residual P3 optimization, not a Phase 4A.3 fix.

## 4. HTTP inventory and access control

Unauthenticated requests to `/science`, deep science pages and all tested
`/api/science/*` routes returned 401. Authenticated requests to all nine UI routes
returned 200 `text/html`; all API routes returned 200 JSON, except an unknown dataset
which correctly returned 404 JSON.

Encoded-path and static-asset variants remained protected. `POST`, `PUT`, `PATCH`,
`DELETE` and `OPTIONS` against the overview endpoint returned 405. Security headers
included:

- `Referrer-Policy: no-referrer`;
- `X-Content-Type-Options: nosniff`;
- `X-Frame-Options: DENY`;
- frontend assets with `Cache-Control: public, max-age=300`.

The credential rotation described above was revalidated with three independent
checks: anonymous 401, revoked credential 401 and replacement credential 200.

## 5. UI / API / SQL consistency

The following production facts were independently read from SQL, obtained from the
API and checked in the UI. Unless noted, the result was `MATCH`.

| Fact | UI / API / SQL value | Status |
|---|---:|---|
| Applied / failed migrations | 17 / 0 | MATCH |
| Active v1 | ID 1, exactly one active | MATCH |
| Candidate | ID 1, `gbm_isotonic_v2`, inactive | MATCH |
| BDIFF total | 15,956 | MATCH |
| Human known | 7,094 | MATCH |
| Natural known | 791 | MATCH |
| Unknown | 8,071 | MATCH |
| Indeterminate | 0 | MATCH |
| FIRMS raw observations | 34,139 | MATCH |
| Static cells | 920,016 | MATCH |
| Feature snapshots | 0 | MATCH |
| Dataset versions / builds | 0 / 0 | MATCH |
| Historical calendar days | 0 | MATCH |
| Candidate trees | 50 | MATCH |
| Candidate isotonic breakpoints / values | 1,774 / 1,774 | MATCH |
| Candidate scoring / shadow scoring | absent | MATCH |

Because the production registry currently contains no dataset versions, strict N2,
strict N3, inclusive N2 and inclusive N3 have no rows to compare. The UI now reports
that state honestly instead of claiming a draft or validated dataset. This is
`UNAVAILABLE`, not a failed comparison.

## 6. Logs, scheduler and Open-Meteo

The available application window covered roughly three hours from the initial
application start: 59 log lines, including four ERROR and twenty WARN lines. Every
ERROR/WARN concerned Open-Meteo rate limiting or the resulting forecast failure.
There were no panics, timeouts, science route errors, migration failures, candidate
scoring messages or shadow scoring messages.

Four consecutive hourly forecast cycles failed after three attempts each. Retries
were separated by a fixed 65-second delay and ended with the expected
`scheduled forecast failed; continuing` message. The process did not crash and the
30-minute FIRMS cycle continued successfully. The latest recorded Open-Meteo success
was `2026-07-28T06:40Z`; the last complete forecast batch finished around
`2026-07-28T06:39Z`, approximately sixteen hours before the audit.

Classification: **DEGRADED DATA FRESHNESS**.

The retry loop is bounded and non-aggressive, so Phase 4A.3 does not change it. A
separate operational issue should determine provider quota/usage, forecast freshness
expectations and an appropriate alert. Any scheduler redesign remains out of scope.

Caddy access logging was not enabled, so counts by status, route and latency were not
available for the historical window. This is an observability gap; no IP address or
request identity was collected.

## 7. Performance

Twenty sequential authenticated requests were sent to each list/summary endpoint
(240 requests total). There were no errors.

| Endpoint | p50 | p95 | maximum | response size |
|---|---:|---:|---:|---:|
| overview | 105.5 ms | 238.5 ms | 242.0 ms | 574 B |
| progress | 21.4 ms | 25.0 ms | 26.8 ms | 4,668 B |
| sources | 25.3 ms | 28.1 ms | 28.5 ms | 1,789 B |
| imports | 25.8 ms | 28.7 ms | 31.0 ms | 14,887 B |
| pipelines | 24.2 ms | 28.7 ms | 28.7 ms | 13,301 B |
| data quality | 49.1 ms | 61.2 ms | 63.3 ms | 792 B |
| events | 23.1 ms | 27.1 ms | 27.4 ms | 11,788 B |
| features | 26.7 ms | 39.1 ms | 78.1 ms | 190 B |
| calendar | 23.9 ms | 26.0 ms | 29.2 ms | 162 B |
| datasets | 20.9 ms | 23.0 ms | 28.8 ms | 2 B |
| models | 26.8 ms | 28.9 ms | 29.4 ms | 88,950 B |
| system | 88.4 ms | 110.4 ms | 154.9 ms | 285 B |

Every endpoint was below its objective. CPU and memory stayed stable under this light
load. No cache was added.

## 8. Browser validation

Real Chromium was exercised at 1440×900, 1280×800, 1024×768 and 375×812 on all eight
functional pages. The initial matrix exposed the four P2 issues listed below.

After correction, 32 page/viewport checks returned 200 with:

- no console, page or failed-request errors;
- no document-level horizontal overflow;
- local keyboard-focusable scrolling for wide tables;
- visible keyboard tooltips with an accessible description relationship;
- correct empty dataset wording;
- current progress entries through Phase 4A.3;
- correct rendering of long model JSON at mobile width;
- successful simulated 503 error display and subsequent recovery.

Screenshots were captured locally for the observed defects and the final states.
They contain production console content and are intentionally ignored by Git.

## 9. Read-only proof

Sensitive counts and state were captured before the controlled navigation session
and again afterwards.

| State | Before | After |
|---|---:|---:|
| human models / active | 1 / 1 | 1 / 1 |
| candidates / inactive | 1 / 1 | 1 / 1 |
| snapshots | 0 | 0 |
| calendar days | 0 | 0 |
| dataset versions / builds / rows / exclusions | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 |
| pipeline runs / import batches | 131 / 131 | 131 / 131 |
| FIRMS raw observations | 34,139 | 34,139 |

The snapshots were taken at approximately 22:45Z and 22:51Z. All UI pages and API
routes were used between them. No scheduler cycle changed the selected values in
that interval, making the before/after comparison unambiguous. Static analysis also
confirms that the console store contains only reads.

## 10. Anomalies and corrections

| Priority | Observation and proof | Impact | Disposition |
|---|---|---|---|
| P0 | Encoded audit Authorization header appeared in local tool output | credential confidentiality | credential rotated immediately; old value rejected; no Git/log inclusion |
| P1 operational | four forecast cycles exhausted bounded Open-Meteo retries; last complete data ≈16 h old | forecast freshness | classified and documented; separate operational issue, no scheduler change |
| P2 | table content caused document-level overflow at desktop/mobile widths | localized usability failure | added a focusable local scroll region and long-token wrapping |
| P2 | tooltip terms had `tabindex` but no focus handler | keyboard users could not read definitions | added focus in/out behavior and accessible description link |
| P2 | zero dataset rows displayed `draft / validated` | misleading production state | empty registry now says no version/build is registered |
| P2 | progress omitted 4A.1–4A.3 and retained a completed comparison risk | stale project state | catalogue corrected using existing historical commits |
| P3 | full candidate artifact makes `/models` 88,950 B | avoidable payload | backlog only; latency is already compliant |
| P3 | no Caddy access log metrics for science routes | limited operations evidence | backlog/checklist only; no config change in this phase |

Each application correction is limited to the existing static console assets and a
regression test. No API contract, SQL query, migration, model or scoring code changed.

## 11. Quality checks

All required local checks passed:

- `cargo fmt --all -- --check`;
- strict workspace/all-target/all-feature Clippy with `-D warnings`;
- `cargo test --workspace --locked --no-fail-fast`, including doc tests;
- science integration suite: 10 passed, 0 failed, 0 ignored;
- `node --check` for the console JavaScript;
- JSON parsing for `phases.json`;
- Chromium post-fix matrix: 32 checks, no browser errors.

No test was ignored or weakened. The known arm64 Cargo cache conflict was not changed
and remains reserved for a separate technical PR. Both GitHub CI runs for commit
`36027bfea23cef997a1f6a1fecd019c96966b734` passed before deployment.

## 12. Controlled deployment

The reviewed commit was built locally for `linux/amd64` with exact revision labels and
loaded on the VPS:

| Item | Deployed value |
|---|---|
| Commit / OCI revision | `36027bfea23cef997a1f6a1fecd019c96966b734` |
| Image | `erytheon:phase4a3-science-36027bf` |
| Image ID | `sha256:974536c0b7557ffcfc20d953a19cecf5df9547e6008f2a1e3504bf1d50042f1a` |
| Application container | `adce65acd90c…` |
| Started | `2026-07-28T23:10:26.695507866Z` |
| Health / restart count | healthy / 0 |
| Rollback configuration | `/opt/pyrorisk/phase4a3-rollback/20260728T231015Z` |
| Rollback image | `erytheon:phase4a2-science-84903938` |

The latest production backup
`pyrorisk-20260728T190714Z.dump` was independently verified before deployment:
its SHA-256 check and `pg_restore --list` catalogue validation both passed.

Only the application was recreated. PostgreSQL retained container ID
`fbecc890d704…` and Caddy retained `a6eeb463f730…`; both remained healthy/running
with zero restarts. No migration ran: the database remained at 17 successful and
zero failed migrations. The science flag and rotated Basic credential were
unchanged by the application deployment.

Post-deployment validation confirmed:

- anonymous science UI/API requests return 401 and authenticated requests return 200;
- production serves the corrected table, tooltip and progress assets;
- real Chromium at 375×812 has document width 375/375, zero console messages and
  an accessible tooltip visible on keyboard focus;
- the empty dataset registry is accurately labelled and Phase 4A.3 is visible;
- v1 remains the sole active model; the sole candidate remains inactive;
- dataset and snapshot counts remain zero; no scoring or shadow scoring appeared;
- five sequential requests to every API endpoint produced zero errors, with maxima
  from 21.8 ms to 186.4 ms (models 28.5 ms, system 90.9 ms);
- the public health endpoint remains 200.

The application restart normally triggered FIRMS and forecast scheduler work.
Between the original audit snapshot and final validation, FIRMS raw rows increased
from 34,139 to 34,709 and import/pipeline run counts from 131 to 133. Logs attribute
these writes to `trigger_type=scheduler`; the console's earlier controlled
before/after navigation window remained unchanged. Open-Meteo again followed its
bounded 65-second retry behavior, reinforcing the existing degraded-freshness
classification without introducing a new failure mode.

## 13. Residual risks and recommendation

- Resolve Open-Meteo forecast freshness in a separate operational task without
  folding a scheduler redesign into this PR.
- Add privacy-conscious route/status/latency observability at the reverse proxy or
  application boundary.
- Keep the full candidate artifact payload under observation; optimize only if
  real usage or transfer size justifies it.
- Continue using the operational checklist for access-control and read-only checks.
- The observed Phase 4B backlog is ready for prioritization only after this
  stabilization PR is merged. P3 remains later and separate.

Phase 4B, P3, shadow scoring, candidate activation/training and the arm64 Cargo cache
fix were not started.

```text
PHASE 4A.3 STABILIZATION COMPLETED
OBSERVED CONSOLE DEFECTS CORRECTED
PRODUCTION VALIDATED AFTER CONTROLLED DEPLOYMENT
UI API SQL CONSISTENCY CONFIRMED
READ-ONLY BEHAVIOR CONFIRMED
ACCESS CONTROL CONFIRMED
V1 REMAINS ACTIVE
CANDIDATE REMAINS INACTIVE
NO CANDIDATE SCORING
NO SHADOW SCORING
PHASE 4B BACKLOG READY
```
