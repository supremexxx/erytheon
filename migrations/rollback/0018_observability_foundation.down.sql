-- See migrations/rollback/0016_model_candidate_registry.down.sql for why this
-- must run as a single transaction (BEGIN/COMMIT, not a bare psql -f).
BEGIN;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'observability' AND table_name = 'scientific_snapshots'
    ) THEN
        RAISE EXCEPTION
            'refusing out-of-order rollback: migration 0019+ must be rolled back before 0018';
    END IF;

    IF EXISTS (SELECT 1 FROM observability.system_snapshots) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: operational snapshot history exists';
    END IF;
END $$;

DROP INDEX IF EXISTS observability.system_snapshots_environment_cadence_idx;
DROP INDEX IF EXISTS observability.system_snapshots_captured_at_idx;
DROP TABLE IF EXISTS observability.system_snapshots;
DROP SCHEMA IF EXISTS observability;
-- Only this migration introduced pgcrypto; safe to drop alongside it.
DROP EXTENSION IF EXISTS pgcrypto;

COMMIT;
