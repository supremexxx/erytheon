# Rollback Guard Correction Report (Phase 3B.11, Part A)

Preventive correction of the same rollback-guard flaw phase 3B.10
found and fixed in `0016`/`0017`, applied here to the three older
migrations (`0013`, `0014`, `0015`) — before any of them was ever run
against real data.

## The flaw

```sql
DO $$
BEGIN
    IF ... THEN
        RAISE EXCEPTION 'refusing destructive rollback: ... data exists';
    END IF;
END $$;

DROP ...;
```

Under plain `psql -f <file>` (no `-v ON_ERROR_STOP=1`, no `-1`, no
explicit transaction), PostgreSQL's default per-statement autocommit
means the `DO` block's `RAISE EXCEPTION` only aborts its *own* implicit
transaction. The following `DROP` statements each start a fresh
autocommit transaction and execute normally — the guard's error prints
correctly, but the destructive statements run anyway. Phase 3B.10
discovered this the hard way on `0016`/`0017`, destroying the isolated
test database's registry table and its one row.

## The fix

```sql
BEGIN;

DO $$
BEGIN
    IF EXISTS (...) THEN
        RAISE EXCEPTION 'refusing destructive rollback: ... data exists';
    END IF;
END $$;

DROP ...;

COMMIT;
```

Wrapping the entire script in one explicit transaction means the `DO`
block's exception puts *that* transaction into an aborted state;
PostgreSQL then refuses every subsequent statement ("current
transaction is aborted, commands ignored until end of transaction
block") regardless of how the script was invoked — `psql -f`, `psql -1`,
or any other client. Applied to:

- `migrations/rollback/0013_feature_snapshot_foundation.down.sql`
- `migrations/rollback/0014_historical_calendar_foundation.down.sql`
- `migrations/rollback/0015_dataset_versioning_foundation.down.sql`

(`0016`/`0017` were already fixed in phase 3B.10.)

## Verification

Official test invocation still uses `psql -v ON_ERROR_STOP=1 -1`, per
the mission — but the script's own safety no longer depends on that
flag being present; it is a defense-in-depth belt, not the buckle.

Automated end-to-end tests
(`crates/store/tests/rollback_guard_safety.rs`) run the *real* `.sql`
files with `psql` — not just the `DO` block's logic in isolation —
against a disposable temporary database created inside the existing
isolated PostgreSQL server (never a new container, never the real
isolated `pyrorisk` database, which now holds real historical
calendar/dataset data from earlier phases that must not be touched).
For each of `0013`/`0014`/`0015`:

1. Confirm an empty-state rollback succeeds (exit 0) and the table is
   actually dropped.
2. Restore via `SQLx` (clear the affected `_sqlx_migrations` tracking
   row(s), reconnect, forward migration reapplies) — never leaving
   tracking desynchronized from the real schema.
3. Insert one minimal fixture row via the existing `Store` methods
   (`register_feature_snapshot`, `ensure_calendar_rule_version` +
   `persist_historical_calendar_days`, `create_dataset_version`).
4. Confirm the rollback is now refused: non-zero exit, `"refusing
   destructive rollback"` in stderr, table and fixture both intact.

All three pass:

```
test rollback_0013_refuses_destructively_once_a_snapshot_exists ... ok
test rollback_0014_refuses_destructively_once_calendar_data_exists ... ok
test rollback_0015_refuses_destructively_once_a_dataset_version_exists ... ok
```

## A second real finding: rollback order matters

Testing `0013`'s and `0014`'s empty-state case surfaced a genuine,
previously-undocumented constraint: migration `0015` adds foreign keys
from `ml.dataset_row_snapshots` to `features.feature_snapshots` and
from `ml.dataset_versions` to `features.calendar_rule_versions`.
Neither `0013` nor `0014` can roll back while `0015`'s tables still
exist — **regardless of whether `0013`/`0014`'s own tables are
empty** — Postgres refuses the `DROP` with a foreign-key dependency
error. This is correct, expected Postgres behavior, not a bug in the
fix: rollbacks must run in reverse migration order (`0015` before
`0014`/`0013`). Both tests roll back `0015` first (safe, since its
tables are empty in the test) before exercising `0013`/`0014`.

**This is not merely a test artifact — it is a real operational
constraint for any future manual rollback of `0013`-`0015` in
production**, and is now documented here rather than left implicit.

## Production impact

**None.** No rollback script was ever run against production or
against the real isolated `pyrorisk` database in this phase — only
against disposable temporary databases created and dropped inside the
same isolated PostgreSQL server. Migrations `0013`-`0015` remain
applied in production exactly as before, with empty tables (per phase
3B.10's audit), and this phase does not touch that state.

## Historical incident, restated for completeness

Documented in full in `PHASE3B10_P1_INACTIVE_REGISTRATION_REPORT.md`
§4: the transaction-safety bug was originally discovered on `0016`/
`0017` when a rollback test destroyed the isolated database's
`ml.model_candidate_registry` table and its one row. Recovery (delete
stale `_sqlx_migrations` tracking, reconnect to re-apply, re-register)
is documented there. This phase's fix to `0013`-`0015` is preventive —
none of those three has ever been run against data that mattered.
