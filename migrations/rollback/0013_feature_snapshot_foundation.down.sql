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
    IF EXISTS (SELECT 1 FROM features.feature_snapshots) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: feature snapshot data exists';
    END IF;
END $$;

DROP TABLE features.feature_snapshots;

COMMIT;
