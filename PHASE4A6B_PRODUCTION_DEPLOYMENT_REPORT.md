# ERYTHEON — Phase 4A.6b production deployment report

## 1. Exact reviewed HEAD and CI

- Pull request: `#10`, `Phase 4A.6 — Harden scientific snapshots and temporal observability`.
- Reviewed branch HEAD: `d693b9b996a8da82630af8d7905f3c86406d372f`.
- The difference from `14b18ef` was one validation-report correction at `5425ac7`, followed by the production provenance wiring at `d693b9b`.
- Both push and pull-request CI checks passed on the exact reviewed HEAD.
- The final `main` CI passed on `abcd46c918d5e9f461e5fa8903237abdab0d8382` (GitHub Actions run `30708636211`).

## 2. Merge

PR #10 was moved from draft to ready and merged on 2026-08-01 at 16:39:49 UTC with a normal merge commit. No squash, rebase, force-push, or branch-history rewrite was used. There were no review comments, requested changes, or blocking threads.

## 3. Final application revision

The deployed application revision is:

```text
abcd46c918d5e9f461e5fa8903237abdab0d8382
```

## 4. Image and digest

The VPS image was built from a Git archive of the exact final `main` revision:

```text
image:    erytheon:phase4a6-abcd46c
digest:   sha256:a5bdebf5b0632a502285f2cccc15348c17546eaccb52bb72abd07ef19467022d
platform: linux/amd64
OCI revision: abcd46c918d5e9f461e5fa8903237abdab0d8382
```

The running container exposes the same revision, image reference, and digest through the non-secret `ERYTHEON_*` provenance variables.

## 5. Backup

The pre-migration PostgreSQL backup is a custom-format dump:

```text
path:     /opt/pyrorisk/backups/erytheon-pre-phase4a6-20260801-161515.dump
size:     2,032,157,777 bytes
sha256:   12beab8a647bdfd59cd4cf8b7ed545a749955e8f0165973368fa83aac5fa3ba1
created:  2026-08-01 16:21:06 UTC
```

`sha256sum -c` and `pg_restore --list` both passed. The dump was restored into a separate PostGIS 16 / PostGIS 3.4 container, separate volume, internal-only Docker network, and database created from `template0`. No port was published.

## 6. Legacy scientific snapshot preservation

The historical snapshot remained unchanged before rehearsal, after rehearsal, after production migration, after application deployment, and after v2 publication:

```text
id:               277efb46-6c03-4dbb-a512-cc4624d4c336
logical id:       scientific-weekly-nowcast-2026-07-30
status:           published
stored values:    920,016 (920,016 unique H3)
observed cells:   792,998
missing cells:    127,018
checksum:         ad4ed3e46c007a37fd116d67307061a826d9f876407e5df1912ef7a238f21d14
static snapshot:  NULL
app revision:     NULL
contract:         v1 / legacy_incomplete
```

The 4A.6 verifier reports `valid=true`, `checksum_valid=true`, and `unique_h3=true` in legacy mode, with the expected incomplete-provenance warning only.

## 7. Migration 0022 rehearsal and production application

The production dump restored with its two required ACL roles recreated without production credentials. On that representative copy:

1. SQLx applied migration `0022`, moving from 21 to 22 successful migrations.
2. The legacy snapshot ID, row count, and checksum remained identical.
3. The guarded rollback succeeded before any 4A.6 durable data existed.
4. Migration `0022` was reapplied.
5. A capture attempt was inserted in the isolated rehearsal database.
6. The rollback then refused with `refusing rollback 0022: Phase 4A.6 durable data exists`.

Production now reports `22` successful migrations, `0` failed migrations, and maximum migration version `22`.

## 8. Application deployment

Only the `app` service was recreated. The running state after deployment was:

```text
app image:       erytheon:phase4a6-abcd46c
app health:      healthy
restart count:   0
PostgreSQL:      same container ID, healthy
Caddy:           same container ID, running
```

The previous image and exact configuration were saved under:

```text
/opt/pyrorisk/phase4a6-rollback/20260801T164716Z
```

## 9. Immutable static bundle

```text
id:                   7ad95b1f-6e3c-4819-bb6c-efb4dec4c24c
family:               cell_static_bundle
status:               active
H3 resolution:        8
cell count:           920,016
logical checksum:     d0b856b40efc0f2eb204bc43e31c6456d2ab6be180056dbf3e8220d5f22d6816
temporal class:       current_snapshot_applied_historically
```

Replaying `snapshot-static-bundle` returned the same ID and did not duplicate the bundle.

## 10. Versioned coverage mask

```text
id:                   22ce9c09-94a9-4633-9cc4-9ee7f9169409
family:               operational_aoi
status:               published
H3 resolution:        8
modelable cells:      792,998
unique mask cells:    792,998
source checksum:      17d95bdbdd29774d2a16f4974f9a69bf76b7319fe1143699cb45234abfae78f8
```

The difference between the static grid and the operational denominator is exactly `127,018` cells. Replaying `snapshot-coverage-mask` returned the same ID.

## 11. True hourly history

The application scheduler created a 4A.6 hourly snapshot automatically immediately after deployment:

```text
logical snapshot id:  62
window start:         2026-08-01 16:00:00 UTC
window end:           2026-08-01 17:00:00 UTC
provenance status:    captured
checksum:             af63edc4a9e3c6c9df7924a2fa0e3ee30150e569a98c96c8124175c65d34ee87
```

Without any restart, manual backfill, or clock manipulation, the next scheduler tick created a second consecutive window:

```text
logical snapshot id:  65
window start:         2026-08-01 17:00:00 UTC
window end:           2026-08-01 18:00:00 UTC
captured at:          2026-08-01 17:48:08 UTC
provenance status:    captured
checksum:             f571292c949fdd9680b570e97364c42b3273c219593ba43d7645381d6db459ab
```

The hourly summary then reported `2` present slots, `2` expected slots, `0` missing slots, and `0` failed attempts.

Older hourly rows remain classified `legacy_last_state_only` and are not presented as complete hourly history.

## 12. Execution attempts and replay

The 16:00 UTC logical window retained one scheduler attempt and two explicit replay attempts. All three reference logical snapshot `62`, carry the exact application revision/image/digest, and share the deterministic checksum. No logical snapshot was overwritten or duplicated.

## 13. Scientific snapshot v2

```text
id:                       07638d41-e832-41a8-8664-25499f2b26e9
logical id:               scientific-weekly-nowcast-2026-08-01
status:                   published
contract:                 v2
traceability:             complete
completeness:             complete
static bundle:            7ad95b1f-6e3c-4819-bb6c-efb4dec4c24c
coverage mask:            22ce9c09-94a9-4633-9cc4-9ee7f9169409
pipeline run:             5c00710d-0248-42d9-a4af-4bbd7afe9f11
forecast batch:           2026-08-01T01:51:25.408307Z
forecast valid at:        2026-08-01T01:00:00Z
horizon:                  nowcast
checksum:                 f280f556f05f276fab0147f385d59f016909ff90581b98a2f00af25ef48d2e68
```

Strict verification passed with no errors or warnings: checksum valid, H3 unique, provenance complete, and coverage consistent.

## 14. Scientific coverage

```text
static cells:                 920,016
structural exclusions:       127,018 outside_operational_aoi
modelable denominator:       792,998
observed modelable cells:    792,998
unexpected missing cells:    0
```

Completeness refers to the modelable denominator and does not claim forecasts on all static cells.

## 15. Deferred BDIFF dry-run

`snapshot-link-labels` was executed without `--apply` and reported:

```text
dry_run:                         true
eligible events:                 0
human known:                     0
natural known:                   0
unknown/indeterminate:           0
H3 found / absent:               0 / 0
proposed links:                  0
inserted links:                  0
superseded links:                0
```

The snapshot is from the current day, so no mature event was eligible. `ml.snapshot_label_links` remained empty. No FIRMS observation was used as a cause label and no absence was converted into a negative.

## 16. Hourly scheduler proof

The first automatic run is proven by application logs and capture attempt `1`, with `trigger_kind=scheduler`. The second automatic run is proven by snapshot `65` and capture attempt `4` (`attempt_number=1`, `trigger_kind=scheduler`, `status=succeeded`, one row processed). It started at `17:48:08.489364 UTC` and finished at `17:48:08.898529 UTC` with the same deterministic checksum as snapshot `65`.

Both snapshots carry the exact deployed provenance:

```text
application revision:     abcd46c918d5e9f461e5fa8903237abdab0d8382
application image:        erytheon:phase4a6-abcd46c
application image digest: sha256:a5bdebf5b0632a502285f2cccc15348c17546eaccb52bb72abd07ef19467022d
```

## 17. Weekly scheduler

The weekly job is registered once by the single running application process. It targets Monday at 03:00 UTC, uses a unique logical snapshot identity to reject concurrent ownership, and the restart schedule tests passed in CI. The next production deadline is:

```text
2026-08-03T03:00:00Z
```

That deadline was not reached during this deployment window. The first real weekly trigger is therefore prepared but **not declared proven**.

## 18. API and console validation

The external science route returned `401` without credentials, confirming that the existing Caddy Basic Auth protection remains active. Public `/health` returned `200`. The unchanged application container was also tested through an SSH tunnel to its private Docker address.

All science pages loaded with real API responses at `1440×900`, `1280×800`, `1024×768`, and `375×812`:

- overview;
- sources;
- data quality;
- features;
- datasets;
- models;
- system;
- observability;
- progress.

There were no browser-console errors and no document-level horizontal overflow at any tested size. Snapshot list, attempts, hourly summary, strict-v2 verification, legacy verification, and deferred-label summary endpoints all returned HTTP 200.

## 19. Non-regression

Read-only checks passed for `/config`, `/alerts`, `/health`, `/sources`, `/risk` with an explicit bbox, `/risk/cell/{h3}`, and WebSocket `/stream`. The sampled v1 cell returned a normal nowcast score. The model registry still has exactly one active v1 and one inactive candidate. No candidate scoring, shadow scoring, training, Caddy change, or authentication change occurred.

## 20. Performance and post-deployment observation

Initial steady-state measurements:

```text
app memory:        ~95 MiB
PostgreSQL memory: ~1.4 GiB
Caddy memory:      ~24 MiB
database size:     9,192 MB
app restarts:      0
disk after bundle/mask/v2: 33 GiB free (66% used)
```

Bundle insertion took about 12 seconds, coverage-mask insertion about 9 seconds, and scientific snapshot value insertion about 27 seconds. Strict verification took a few seconds. No blocking PostgreSQL lock chain was observed.

Open-Meteo returned its known rate limit during the immediate post-restart forecast refresh and again on the next hourly cycle. On both cycles the scheduler abandoned partition `01` after three controlled retries. The last completed forecast remained available, serving and snapshots stayed healthy, and this operational source condition is retained as a residual monitoring item rather than hidden. The second cycle also completed its FIRMS import successfully (`219` accepted observations, all `219` ignored as already-public duplicates).

After the second real hourly tick, the application and PostgreSQL containers were still healthy, the application restart count remained `0`, and an explicit `pg_blocking_pids` check found `0` blocked sessions.

## 21. Rollback

Before 4A.6 durable data, the migration rollback was proven on the isolated production copy. After attempts, bundle, mask, and v2 data now exist, the authorized rollback is application-only:

1. restore `/opt/pyrorisk/phase4a6-rollback/20260801T164716Z/.env` and `compose.yml`;
2. select `erytheon:phase4a5-observability-11b0015`;
3. recreate only `app`;
4. preserve migration `0022` and every 4A.6 row;
5. verify health and legacy checksum.

The destructive `0022` down migration must not be used in production now.

## 22. Tags and release

The immutable application tag is `v0.4.5-app` on `abcd46c918d5e9f461e5fa8903237abdab0d8382`. The documentation tag is `v0.4.5` on the final report commit. Existing tags are not moved.

The GitHub release title is:

```text
ERYTHEON v0.4.5 — Hardened scientific snapshots and true hourly history
```

## 23. Residual limits

- The first scheduled weekly production trigger remains to be observed on 2026-08-03 at 03:00 UTC.
- Deferred BDIFF linking produced no eligible mature event for the current-day snapshot; a later dry-run is required before any separately authorized write.
- Open-Meteo rate limiting remains an operational source risk and must continue to be monitored.
- The static bundle is correctly classified as a current snapshot applied historically; it is not an exact historical reconstruction.
- No training, candidate scoring, shadow scoring, or model promotion was performed.

## Final status

```text
PHASE 4A.6B PRODUCTION DEPLOYMENT COMPLETED
TRUE HOURLY HISTORY ACTIVE
EXECUTION ATTEMPTS PRESERVED
LEGACY HOURLY STATES PRESERVED AND CLASSIFIED
IMMUTABLE STATIC BUNDLE ACTIVE
VERSIONED COVERAGE MASK ACTIVE
127018 CELLS CLASSIFIED OUTSIDE OPERATIONAL AOI
792998 OF 792998 MODELABLE CELLS OBSERVED
ZERO UNEXPECTED MISSING CELLS
SCIENTIFIC SNAPSHOT V2 PUBLISHED WITH COMPLETE PROVENANCE
LEGACY SCIENTIFIC SNAPSHOT UNCHANGED
LEGACY CHECKSUM PRESERVED
DEFERRED BDIFF LABEL LINKING VALIDATED IN DRY-RUN
APPLICATION HEALTHY
POSTGRESQL HEALTHY AND NOT RECREATED
CADDY RUNNING AND NOT RECREATED
22 MIGRATIONS SUCCESSFUL
V1 REMAINS ACTIVE
CANDIDATE REMAINS INACTIVE
NO TRAINING
NO CANDIDATE SCORING
NO SHADOW SCORING
```
