# ERYTHEON — Scientific console operational checklist

Use this checklist after an application deployment, access-control change or reported
console incident. Commands and credential locations are intentionally omitted.

## Before a change

- [ ] Record UTC time, current Git revision, image tag/digest and container ID.
- [ ] Confirm application and PostgreSQL health, Caddy state and restart counts.
- [ ] Record CPU, memory and filesystem use.
- [ ] Confirm exactly one active v1 model.
- [ ] Confirm every candidate is inactive.
- [ ] Confirm there is no candidate scoring or shadow-scoring process/table.
- [ ] Record migrations applied/failed and verify that the proposed change has no migration.
- [ ] Capture sensitive read-only counts: models, candidates, datasets, builds,
      rows, exclusions, snapshots, imports, pipeline runs and source observations.
- [ ] Verify the previous application image/configuration is locally available as
      the rollback target.
- [ ] Ensure secrets will not be passed on command lines or printed in logs/tool output.

## Access control

- [ ] Anonymous `/science`, a deep UI route and an API route each return 401.
- [ ] A valid credential returns 200 for the same protected routes.
- [ ] A revoked/previous credential returns 401 after any rotation.
- [ ] Encoded-path and static-asset variants remain protected.
- [ ] Unsupported write methods return 405.
- [ ] Security headers include no-referrer, nosniff and frame denial.
- [ ] Do not record credentials, hashes, IPs or request identities in reports.

## Functional smoke test

- [ ] Open overview, progress, sources, data quality, features, datasets, models and system.
- [ ] Direct-load at least one deep route.
- [ ] Confirm no browser console, page or failed-request errors.
- [ ] Confirm tables scroll locally without widening the page.
- [ ] Confirm keyboard focus reveals scientific-definition tooltips.
- [ ] Confirm empty registries are described as empty, not draft/validated.
- [ ] Confirm loading, 404 and API-error states are intelligible and recoverable.
- [ ] Test desktop, tablet and mobile widths.

## Scientific and database invariants

- [ ] Compare active model ID/status across UI, API and SQL.
- [ ] Compare candidate ID/status across UI, API and SQL.
- [ ] Compare BDIFF total and cause-category counts.
- [ ] Compare FIRMS, static cell, snapshot, dataset/build and migration counts.
- [ ] For populated datasets, compare rows, labels, splits, exclusions, seed and checksums.
- [ ] Compare candidate metric/checksum/tree/calibration metadata without loading it
      into a scoring path.
- [ ] Confirm no console API exposes a write operation.
- [ ] Repeat the sensitive count snapshot after navigation and account separately for
      expected scheduler writes.

## Sources, logs and freshness

- [ ] Review application ERROR/WARN/panic/timeout and science-route errors.
- [ ] Review Open-Meteo 429s, attempts, delay, abandon messages and last complete forecast.
- [ ] Confirm retries are bounded, FIRMS continues and the application does not crash.
- [ ] Classify freshness as normal, transient, degraded or an operational bug.
- [ ] Review Caddy/application route status and latency aggregates when available,
      without exposing request identities.
- [ ] Open a separate operational issue when freshness exceeds the agreed SLO.

## Performance

- [ ] Use only light sequential probes, normally 20 requests per endpoint.
- [ ] Record p50, p95, maximum, average size and errors.
- [ ] Check overview/progress/datasets/models against 500 ms p95.
- [ ] Check sources/data-quality/features/system against 1 s p95.
- [ ] Inspect SQL call count and rule out N+1 or repeated scans before adding cache.
- [ ] Note unusually large payloads even when latency is acceptable.

## Controlled application deployment

- [ ] Local format, strict Clippy, full tests and targeted science tests are green.
- [ ] GitHub CI is green for the exact commit.
- [ ] The diff contains no migration, scoring, scheduler, FIRMS, FWI or `/risk` change.
- [ ] Build/tag the exact reviewed SHA for the production architecture.
- [ ] Preserve the current Basic credential and science feature flag.
- [ ] Recreate only the application service.
- [ ] Wait for application health; do not restart PostgreSQL or Caddy.
- [ ] If health or invariants fail, restore the previous image/configuration and recreate
      only the application service.

## After deployment

- [ ] Record new image tag/digest, container ID, start time and restart count.
- [ ] Confirm application and PostgreSQL healthy and Caddy running/config valid.
- [ ] Repeat anonymous/authenticated route and deep-link checks.
- [ ] Repeat browser checks for every corrected defect.
- [ ] Confirm map and public operational behavior are unchanged.
- [ ] Confirm migrations unchanged, v1 active, candidate inactive, no candidate scoring
      and no shadow scoring.
- [ ] Compare pre/post sensitive counts, source freshness, CPU, memory and logs.
- [ ] Update the stabilization report with exact evidence and the rollback target.

## Escalation

- [ ] P0: stop, contain, rotate exposed credentials if applicable and preserve evidence.
- [ ] P1: classify impact, create a focused issue and avoid unrelated redesign.
- [ ] P2: fix only with a reproducible observation and regression validation.
- [ ] P3: add to the observed backlog; do not implement during stabilization.
