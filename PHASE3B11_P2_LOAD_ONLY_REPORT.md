# Phase 3B.11 — P2 Load-Only Verification Report

Two controlled operations: Part A (preventive rollback-guard fix, see
[ROLLBACK_GUARD_CORRECTION_REPORT.md](ROLLBACK_GUARD_CORRECTION_REPORT.md))
and Part B (P2 load-only verification), documented here.

## 1. Git audit

Continued from phase 3B.10's local commits (`9a831d6`, `df7285c`,
`060e827` — the P1 documentation commit). This phase added three local
commits, in order:

| Commit | Summary |
|---|---|
| `18d8c1e` | `fix: make historical rollback guards transaction-safe` |
| `48b6b9a` | `feat: add candidate load-only verification` |
| (this report's commit) | `docs: report P2 load-only verification` |

Working tree clean, no secrets, no dumps. No file under `crates/api`,
`crates/risk`, `human_model.rs`, FIRMS, FWI, or the scheduler was
touched. `git diff origin/main..HEAD --stat` shows only the expected
cumulative divergence from a repository that has never pushed. Not
pushed.

## 2. Part A summary

See `ROLLBACK_GUARD_CORRECTION_REPORT.md` for full detail: `0013`-`0015`
rollback scripts wrapped in explicit `BEGIN`/`COMMIT`, verified end to
end against a disposable temporary database inside the existing
isolated PostgreSQL server, and a real rollback-ordering constraint
(0015's foreign keys block 0013/0014) discovered and documented. No
rollback was ever run against production or the real isolated
database.

## 3. Part B: the load-only command

`crates/engine/src/candidate_load_verification.rs`, run via `pyrorisk
verify-model-candidate-load`. Required, explicit arguments (no default
to "latest"/"first"/"active"/"newest"): `--candidate-id`,
`--expected-status`, and all five expected checksums.

Steps performed, in order: connect; read `ml.model_candidate_registry_
count()` (before); open a **real PostgreSQL read-only transaction**
(`Store::model_candidate_by_id_read_only`, issuing `SET TRANSACTION
READ ONLY` before the `SELECT`) and read exactly the row matching
`--candidate-id`; confirm `status` matches `--expected-status` and is
never `"active"`; deserialize the `JSONB` artifact; recompute and
compare all five checksums; run `CandidateArtifact::validate()`;
confirm 12 features in the checksummed order, 50 trees, 1,774 isotonic
breakpoints, dataset fingerprint, seed, and train/calibration/test
periods; read the registry count again (after); report timings and
resident memory; let the artifact drop out of scope.

## 4. Zero-scoring proof (mission section 10)

Not merely asserted — checked. `candidate_load_verification.rs`
imports only `CandidateArtifact`, `ARTIFACT_VERSION`, and the five
checksum functions from `crate::candidate_artifact`. It never imports
`score_with_artifact`, `apply_isotonic`, `apply_normalization`, or any
`predict` method. A dedicated unit test
(`this_module_never_references_any_scoring_function`) scans this
file's own source text via `include_str!`, excludes the test module
itself (to avoid the banned-identifier list trivially matching its own
string literals — a bug caught and fixed while writing this test: the
first version failed on itself), and asserts none of
`score_with_artifact`, `apply_isotonic(`, `apply_normalization(`, or
`.predict(` appear in the production code. This test passed in the
full workspace run (70/70 `engine` unit tests green) before the
production execution.

## 5. Build

| Metadata | Value |
|---|---|
| Commit | `48b6b9a` |
| Rust version | 1.94.1 |
| SQLx version | 0.8.6 |
| Source archive checksum | `cb8fbc34321cd687ea10cbc616eeffdbe2ac63395fd81f62bc016091667feac6` |
| Binary checksum | `18c16e38a77cd897ffe0f58c4c172cff58695ec9284c62b1345d00b677919b61` |
| Binary size | 24,092,072 bytes |
| Build UTC | 2026-07-28T14:15:38Z |
| Build command | `cargo build --release -p engine --bin pyrorisk` |

No new application image was built; `pyrorisk-app-1` was never
restarted. The one-off binary ran connected to `pyrorisk_backend`
(production's internal network) from the same ephemeral build
container already used for the isolated rollback tests.

## 6. Pre-execution production controls

| Check | Result |
|---|---|
| PostgreSQL healthy | Yes |
| Application healthy | Yes (`Up 31 hours (healthy)`) |
| Caddy running | Yes |
| `/health` | `status: ok, db: ok` |
| Candidate row | `id=1`, `status=inactive` |
| v1 | `id=1`, `active=true`, exactly one active |
| Shadow scoring | Not present (confirmed: `pg_shadow` in the "shadow" name search is PostgreSQL's own system catalog, not a real shadow-scoring table) |
| Candidate/shadow log mentions | None |
| Long-running locks | None |
| Disk space | 49 GiB available (50% used) |

No new backup was created (strictly read-only operation, per the
mission's own guidance).

## 7. P2 execution (single run)

```
candidate_id = 1
status = inactive
artifact_load = success
artifact_validation = success
checksums_exact = true
trees = 50
isotonic_points = 1774
dataset_row_fingerprint = bee1bfaa5401144c5cbffe1f42bf45f7
seed = 2026071
feature_names = [wui, road, agri, population, poi, power_line, hist,
                 combustible, weekend, public_holiday, season_sine, season_cosine]
training_period = [2020-01-01, 2023-12-31]
calibration_period = [2024-01-01, 2024-12-31]
test_period = [2025-01-01, 2025-12-31]
scores_computed = 0
database_writes = 0
registry_row_count_before = 1
registry_row_count_after = 1
```

Timings (microseconds): connect 30,374; read SQL 14,133; deserialize
2,051; checksums 5,243; validate 7; total 74,957 (≈75 ms end to end).
Resident memory: 8,924 KB before → 14,272 KB after (+5,348 KB, i.e.
≈5.2 MiB, for parsing the 85,513-byte JSON into the full `Candidate
Artifact` structure — GBM trees, isotonic points, normalization/
imputation parameters — plus ordinary process/runtime overhead).

## 8. Write proof

- `ml.model_candidate_registry` row count: 1 before, 1 after.
- Row's `created_at` (`2026-07-28 13:11:53.209065+00`) and
  `artifact_checksum` unchanged, confirmed by a direct query
  immediately after the run.
- The read itself ran inside a genuine `SET TRANSACTION READ ONLY`
  session — not merely code that happens not to issue a write, but a
  transaction in which the PostgreSQL server itself would reject any
  write statement.
- No new row in `human_model_versions`; `id=1`/`active=true`/
  `trained_at` unchanged.
- No shadow-scoring table exists in the schema at all (confirmed via
  `information_schema.tables`; the one match for `%shadow%` is
  PostgreSQL's built-in `pg_shadow` system view).

## 9. Post-execution application controls

| Check | Result |
|---|---|
| App restart | None (`pyrorisk-app-1` uptime unchanged, 31 hours) |
| `/health` | `status: ok, db: ok` |
| `/sources` | HTTP 200 |
| `/alerts` | HTTP 200 |
| `/risk` (no bbox) | HTTP 400, `missing_bbox` — same baseline behavior as before P2 |
| App logs (last 5 min) | No `candidate`/`shadow` mentions |
| v1 | Unchanged |

## 10. Load-only measurements (mission section 16)

Total load time ≈75 ms, dominated by connection setup (30 ms) and the
JSONB read (14 ms) — the actual deserialize/checksum/validate work
inside the process took under 8 ms combined. Resident memory grew by
≈5.2 MiB for the fully-parsed artifact (50 trees + 1,774 isotonic
points + per-feature statistics). The artifact was dropped at the end
of the function via ordinary Rust scope exit — no explicit "unload"
step exists or is needed; ownership already guarantees deallocation.
No fragmentation or repeat-load behavior was measured, since only one
execution is authorized in production; a repeated-load timing/memory
profile would need to run in the isolated environment instead, not
attempted this phase (out of scope).

## 11. Risks and open items

- The `0013`-`0015` rollback fix is preventive; the rollback-ordering
  constraint (§ above, "0015 before 0013/0014") is now documented but
  has never been exercised against real production data, since no real
  rollback of any of these migrations has occurred or is planned.
- Load timing (75 ms) includes cold connection setup specific to a
  one-off CLI process; a persistent service holding an open connection
  pool would see materially lower per-load latency — not measured here,
  since this phase never loads the candidate into `pyrorisk-app-1`.
- The offline/online snapshot-drift risk from phase 3B.9/3B.10 (`hist`
  ~0.033% drift) is unchanged by this phase; P2 does not re-measure it,
  since P2 never scores anything and drift only matters for scoring
  fidelity.

## 12. Next milestone

```
P3 — manual, limited shadow scoring
```

Not started. Requires separate, explicit authorization, and (per
`SHADOW_SCORING_DESIGN.md`) an actual code change wiring an optional,
default-disabled shadow-scoring path into `HeuristicV1::score` — none
of which exists yet.

## 13. Conclusion

```
PHASE 3B.11 P2 COMPLETED
INACTIVE CANDIDATE LOAD VALIDATED
ARTIFACT NOT SCORED
DATABASE UNCHANGED
V1 REMAINS ACTIVE
NO SHADOW SCORING
NO PUSH
```
