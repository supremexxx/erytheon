# Phase 3B.10 — P1: Inactive Production Registration Report

**Result: candidate registered in production as `inactive`. v1 remains
the sole active model. No candidate scoring, no shadow scoring, no
serving/API change.**

## 1. Git audit

Continued from phase 3B.9's local commits (`ec5460d`, `e5026c7`,
`e9d08cf`, `cf17ce2` — the documentation commit). This phase added
three local commits, in order:

| Commit | Summary |
|---|---|
| `9a831d6` | `feat: add P1 model candidate registration and fix rollback guard bug` |
| `df7285c` | `fix: load the P0 artifact from a file instead of rebuilding it live` |
| (this report's commit) | `docs: report P1 inactive registration` |

Working tree clean, no secrets, no dumps committed to Git (the real
production `.dump` backup lives only on the VPS filesystem, outside
the repository). No file under `crates/api`, `crates/risk`,
`human_model.rs`, FIRMS, FWI, or the scheduler was touched. Not pushed.

## 2. Migration 0016 audit — and what it actually revealed

Re-auditing 0016 against the mission's checklist found a real gap:
**no uniqueness constraint**, which idempotent registration requires
at the database level. Fixing this also surfaced a bigger issue: `git
status`/history showed migration 0016 already had rows in `_sqlx_
migrations` on the **isolated** deploy database — a side effect of
every phase 3B.9 `Store::connect` call automatically running pending
migrations, which the phase 3B.9 report's claim of "not applied to any
database" did not account for. **Correction, stated plainly**: 0016
*was* applied to the isolated database during phase 3B.9; it was never
applied to production until this phase. Rather than edit an
already-applied migration, the fix is `migrations/0017_model_
candidate_registry_identity.sql`, an additive `ALTER` adding a `seed`
column and `UNIQUE (model_family, model_name, dataset_logical_id,
seed)`.

**A second, more serious bug was found and fixed while testing the
rollback guards on the isolated database**, before production was
touched (see §6). The final, audited state of 0016/0017:

- Additive only: creates `ml.model_candidate_registry` (0016) and adds
  one column + one constraint (0017). Neither touches `human_model_
  versions`.
- `status` is `CHECK`-constrained to `'candidate'`/`'inactive'` only —
  `'active'` is not a representable value (verified with a real failed
  `INSERT ... status = 'active'` in the isolated test suite).
- Logical identity `(model_family, model_name, dataset_logical_id,
  seed)` is now `UNIQUE`, enforced at the database level.
- Both `down.sql` files are additive-safe and refuse to run once real
  data exists (see §6 for how this guard was actually made to work).

## 3. Isolated revalidation (mission section 6)

Performed in full against the existing isolated deploy database
(`erytheon-3b3-deploy-20260727T203310Z`, unchanged across phases
3B.3-3B.10) **before any production step**:

- Migrations 0016+0017 applied cleanly.
- The exact P0 candidate registered: one row, `status='inactive'`,
  all fields and checksums exact.
- Replayed identically: `already_registered`, same row id, row count
  unchanged (1) — idempotence confirmed.
- A conflicting replay (same logical identity, different checksum) was
  refused with a hard error, confirmed via an automated integration
  test (`crates/store/tests/model_candidate_registry.rs`, 5 tests, all
  passing against the real isolated database).
- `human_model_versions` row count unchanged, confirmed by test.

## 4. A real incident during rollback testing — found, fixed, reverified

Testing the rollback-refusal guard the first time (`psql -f
0017.down.sql`, no `ON_ERROR_STOP`, no explicit transaction) **actually
destroyed** the isolated database's `ml.model_candidate_registry`
table and its one registered row. The guard's `RAISE EXCEPTION`
printed correctly, but a bare `DO $$ ... $$;` block only aborts its
*own* implicit autocommit transaction under plain `psql -f` — the
subsequent `DROP TABLE`/`DROP INDEX` statements ran in fresh
transactions and executed anyway. **This affected only the isolated
test database — production had not yet been touched at this point.**

Root cause and fix, documented in the migration files themselves
(`migrations/rollback/0016...down.sql`, `0017...down.sql`): wrap the
entire script in an explicit `BEGIN; ... COMMIT;` so PostgreSQL refuses
every statement after the guard's exception, not just the one inside
the `DO` block. Verified twice: on the isolated database (guard now
exits non-zero, table and row survive) and again on production (§7),
using `psql -v ON_ERROR_STOP=1 -1` in addition to the fixed scripts.

**The same latent bug exists in three pre-existing migrations'
rollback scripts** (`0013`, `0014`, `0015` — feature snapshots,
historical calendar, dataset versioning), which use the identical bare
`DO` pattern. They were not touched this phase (out of scope, no real
data has ever triggered them), but this is a real, concrete follow-up
recommendation: fix all four the same way before any of them are ever
run against a database with real rows.

Recovery: the isolated database's stale `_sqlx_migrations` rows for
16/17 were deleted, the table was recreated cleanly by the corrected
migrations on the next connect, and the P0 candidate was re-registered
there before repeating the full revalidation.

## 5. Design flaw found and fixed: registration must not rebuild the artifact

The first `register-model-candidate` implementation rebuilt the
artifact via `build_candidate_artifact`, which reads `ml.dataset_rows`
— empty on production by design (dataset construction has only ever
happened in the isolated training database, phases 3B.3-3B.9). The
first real production attempt failed with a decode error **before any
database write** (confirmed: registry row count was `0` before and
after). Fixed (`df7285c`) by having the command load the exact P0
artifact JSON file instead, verifying `git_commit`/`dataset_logical_
id`/`seed` against the values embedded in the file plus all five
checksums, before ever touching the database. Re-verified on the
isolated database before touching production again.

## 6. Release build

Built from commit `df7285c` in a fresh ephemeral container
(`rust:1.94-bookworm`, 2 CPU / 4 GiB), connected first to the isolated
network for revalidation, then additionally to `pyrorisk_backend` (the
production database's own internal, no-internet network) for the
production step — reusing the same container's already-cached
dependencies rather than building a second, separate container (which
failed: `pyrorisk_backend` has no internet access by design, so a
fresh container there cannot download crates or Rust components).

| Metadata | Value |
|---|---|
| Commit | `df7285c` |
| Rust version | 1.94.1 |
| SQLx version | 0.8.6 |
| Source archive checksum | `0b313a1c3a624e072c78842fc7a0452f984fc836904142c59aea43b224b06a12` |
| Binary checksum | `af4e922f2051a442195006c26ddefd9bd4d3d135b705c629421ed8009b038432` |
| Binary size | 23,836,344 bytes |
| Build UTC | 2026-07-28T13:02Z (approx.) |

**No new application (`pyrorisk-app-1`) image was built or deployed,
and the running app container was never restarted.** No runtime/
serving code changed this phase — only a migration and a one-off CLI
command were needed, both run manually against production via the
traceable release binary above. Rebuilding/redeploying the actual
serving image would have been unnecessary risk for zero behavioral
benefit.

## 7. Production backup

| Item | Value |
|---|---|
| File | `/opt/pyrorisk/backups/pyrorisk-20260728T125459Z.dump` |
| Size | 1,810,563,287 bytes |
| SHA-256 | `90ab47309674be6aae684aabdcf6adf279bcc3ba34a1f75f0af8841b2f33f380` |
| Verified | `sha256sum -c` → OK |
| Catalog | `pg_restore --list` → 410 TOC entries, valid custom-format archive, `schemas: environment, features, ...` confirmed present |
| UTC timestamp | 2026-07-28T12:54:59Z |

No existing backup was deleted; 3 prior backups (2026-07-26, -27 x2,
-28T02:31) remain untouched.

## 8. Pre-migration checks

| Check | Result |
|---|---|
| PostgreSQL healthy | Yes (`Up 9 days (healthy)`) |
| Application healthy | Yes (`Up 30 hours (healthy)`, `/health` → `status: ok, db: ok`) |
| Caddy running | Yes (`Up 9 days`) |
| Disk space | 51 GiB available / 96 GiB (48% used) |
| Long-running locks | None (`pg_stat_activity` query returned 0 rows) |
| Migrations present before | max version 12 (13-17 all pending — production had never received any phase 3B.3-3B.9 migration) |
| v1 model count / active | 1 row, `id=1`, `active=true` |
| `ml.model_candidate_registry` | Absent (confirmed before migration) |

## 9. Migration application

Applied via a read-only CLI command (`data-status`) that connects
through `Store::connect` — the same mechanism that runs pending
migrations on every connection, applied here deliberately and only for
this purpose. Migrations 13, 14, 15 (feature snapshot foundation,
historical calendar foundation, dataset versioning foundation —
all pure additive `CREATE TABLE` migrations from phases 3B.3-3B.5, no
`ALTER`/`DROP`/`TRUNCATE` against any existing table, confirmed by
grep), then 16 and 17, all applied successfully.

| Check | Before | After |
|---|---|---|
| `_sqlx_migrations` max version | 12 | 17, all `success=true` |
| `ml.model_candidate_registry` | absent | present, 0 rows, correct constraints/indexes |
| `human_model_versions` (id=1, active) | unchanged | unchanged |
| Schema hash `public` | `f59e763d...` | `f59e763d...` (identical) |
| Schema hash `fire` | `fc578223...` | `fc578223...` (identical) |
| Schema hash `validation` | `e536dc17...` | `e536dc17...` (identical) |

No scheduler was started; no candidate service was launched.

## 10. Snapshot drift measurement (mission section 16)

Pre-declared threshold (stated before measuring, not adjusted after):
**block if any critical feature falls below 90% exact parity, or if
`combustible` falls below 99%.** Since production never received the
training dataset, drift was measured between production's live
`cell_static` and the isolated database's `cell_static` (the one
phase 3B.9's own offline/online parity check used):

| Metric | Production | Isolated | Match |
|---|---|---|---|
| Total rows | 920,016 | 920,016 | 100% |
| `combustible` cells | 761,560 | 761,560 | **100%** |
| `hist`-positive cells | 108,163 | 108,163 | 100% |
| Sum `wui`/`road`/`agri`/`population`/`poi`/`power_line` | identical | identical | 100% |
| Sum `hist` | 9,355.267679 | 9,358.378790 | 99.967% |
| Spot sample (5 rows, all fields) | byte-identical | byte-identical | 100% |

Only `hist` shows any measurable drift (~0.033% relative), consistent
with it being the one periodically-refreshed feature
(`refresh_history_features`, run independently on each database) —
exactly the risk already characterized in phase 3B.9, now confirmed
negligible in production specifically. **Well above both pre-declared
thresholds — not blocking.**

## 11. Candidate registration

```
model_family: gbm_isotonic_v2
model_name: human_ignition_propensity_v2
artifact_version: 1
status: inactive
git_commit: e9d08cf
dataset_logical_id: erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality
dataset_row_fingerprint: bee1bfaa5401144c5cbffe1f42bf45f7
seed: 2026071
artifact_checksum: 868333c5afc0898ff4dc0cb3a4c922eae851fd28ecca1834e666bc40833fcd74
row id: 1
created_at: 2026-07-28T13:11:53.209065+00:00
```

All five checksums (artifact, GBM, calibrator, transforms, feature
list) verified to match the loaded artifact file exactly before the
write occurred.

## 12. Read-back validation

The artifact was read back from PostgreSQL's `JSONB` column, written
to a file, and fed through the *same* `register-model-candidate`
command a second time (with the same expected checksums). Outcome:
`already_registered`, same row id, same `created_at`, checksums
matched exactly — proving the stored artifact reproduces identical
checksums after a real database round trip, and confirming idempotence
in the same step. `CandidateArtifact::validate()` passed against the
read-back copy. No score was computed at any point.

## 13. Idempotence

| Attempt | Outcome | Row count before | Row count after |
|---|---|---|---|
| 1st registration | `registered`, row id 1 | 0 | 1 |
| 2nd (read-back file, same identity+checksum) | `already_registered`, row id 1 | 1 | 1 |

No second row was ever created. `human_model_versions` was queried
before and after: unchanged (1 row, `id=1`, `active=true`,
`trained_at` unchanged).

## 14. v1 controls

```
human_model_versions.id = 1
active = true
trained_at = 2026-07-24 20:55:48.823149+00 (unchanged)
train_positive_count = 5334, train_negative_count = 21336 (unchanged)
```

Exactly one active row. The candidate exists only in
`ml.model_candidate_registry`, never in `human_model_versions`.

## 15. Application controls

No restart performed (uptime unchanged: `pyrorisk-app-1` up 30 hours
throughout).

| Check | Result |
|---|---|
| `/health` | `{"status":"ok","db":"ok",...}` |
| `/sources` | HTTP 200 |
| `/risk` (no bbox) | HTTP 400, `missing_bbox` — expected baseline behavior, not a regression |
| `/alerts` | HTTP 200 |
| App logs (last 10 min) | No `candidate`/`shadow` mentions |
| FIRMS | Normal (`last_success` recent, `staleness_s` small) |
| FWI | Unchanged (existing slow-query warnings only, pre-existing pattern, unrelated to this phase) |

## 16. Rollback-refusal test

Run against production with the corrected, transaction-wrapped
`down.sql` scripts (`psql -v ON_ERROR_STOP=1 -1`):

```
BEGIN
WARNING: there is already a transaction in progress
ERROR: refusing destructive rollback: model candidate registry data exists
(exit code 3)
```

Table and row confirmed intact immediately after (`SELECT * FROM ml.
model_candidate_registry` → 1 row, unchanged). No destructive statement
executed; no bypass attempted.

## 17. Cleanup

Ephemeral build container and its temporary network removed; the
isolated deploy database was disconnected from that network and left
running unchanged on only its own network, as in every prior phase.
No stray files left on the VPS (`/tmp` checked clean of this phase's
artifacts). No existing backup deleted.

## 18. Risks and follow-ups

- **The 0013/0014/0015 rollback-guard bug is not yet fixed** — same
  latent flaw as 0016/0017 had. No real data has ever triggered those
  guards, so this is a recommendation, not an active incident.
- **`hist` snapshot drift** (~0.033%, production vs. isolated) is
  negligible today but will grow between refreshes; a documented
  refresh cadence (not created this phase) would keep this bounded.
- **The unified single-active-model-across-families registry** (P1's
  `ml.model_candidate_registry` and v1's `human_model_versions` remain
  two separate tables) is deferred to whichever future phase actually
  authorizes activation — flagged, not designed, here.

## 19. Next milestone

```
P2 — load-only verification
```

Requires separate, explicit authorization before any work begins.

## 20. Conclusion

```
PHASE 3B.10 P1 COMPLETED
CANDIDATE REGISTERED IN PRODUCTION AS INACTIVE
V1 REMAINS ACTIVE
NO CANDIDATE SCORING
NO SHADOW SCORING
NO PUSH
```
