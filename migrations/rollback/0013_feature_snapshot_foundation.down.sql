-- See migrations/rollback/0016_model_candidate_registry.down.sql for
-- why this must run as a single transaction (phase 3B.10 incident: a
-- bare DO-block guard does not stop later statements under plain
-- `psql -f` without ON_ERROR_STOP/an explicit transaction -- the guard
-- prints its error, but subsequent DROP statements run in fresh
-- autocommit transactions and execute anyway). Fixed preventively here
-- in phase 3B.11 before this script was ever run against real data.
BEGIN;

DO $$
BEGIN
    -- Phase 4A.5 (migration 0019) added
    -- observability.scientific_snapshots.static_snapshot_id, a foreign
    -- key onto this table. Roll back 0019 (and its own dependents,
    -- 0020-0021) before 0013, the same way 0015's dependents already
    -- had to be rolled back first.
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'observability' AND table_name = 'scientific_snapshots'
    ) THEN
        RAISE EXCEPTION
            'refusing out-of-order rollback: migration 0019 must be rolled back before 0013';
    END IF;

    IF EXISTS (SELECT 1 FROM features.feature_snapshots) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: feature snapshot data exists';
    END IF;
END $$;

DROP TABLE features.feature_snapshots;

COMMIT;
