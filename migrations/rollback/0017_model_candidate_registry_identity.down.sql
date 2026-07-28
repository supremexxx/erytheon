-- See migrations/rollback/0016_model_candidate_registry.down.sql for
-- why this must run as a single transaction (phase 3B.10 incident:
-- a bare DO-block guard does not stop later statements under plain
-- `psql -f` without ON_ERROR_STOP/an explicit transaction).
BEGIN;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM ml.model_candidate_registry) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: model candidate registry data exists';
    END IF;
END $$;

ALTER TABLE ml.model_candidate_registry
    DROP CONSTRAINT IF EXISTS model_candidate_registry_logical_identity_unique;
ALTER TABLE ml.model_candidate_registry
    DROP COLUMN IF EXISTS seed;

COMMIT;
