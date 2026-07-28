-- IMPORTANT: this script must always be executed as a single
-- transaction (psql -v ON_ERROR_STOP=1 -1 -f, or an explicit BEGIN/
-- COMMIT as below). A bare `DO $$ ... RAISE EXCEPTION ... $$;` block
-- only aborts its OWN implicit autocommit transaction under plain
-- `psql -f` -- later statements in the same script still run in fresh
-- transactions and would execute anyway. This was discovered the hard
-- way in phase 3B.10: running this file with plain `psql -f` (no
-- ON_ERROR_STOP, no explicit transaction) let the DROP TABLE execute
-- immediately after the guard's RAISE EXCEPTION printed, destroying
-- the isolated test database's registry table and its one row. The
-- explicit BEGIN/COMMIT here closes that gap: once the DO block
-- aborts, PostgreSQL refuses every subsequent statement in the same
-- transaction, so the DROP statements below can never run once real
-- data exists, regardless of how the script is invoked.
BEGIN;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM ml.model_candidate_registry) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: model candidate registry data exists';
    END IF;
END $$;

DROP INDEX IF EXISTS ml.model_candidate_registry_family_idx;
DROP TABLE IF EXISTS ml.model_candidate_registry;

COMMIT;
