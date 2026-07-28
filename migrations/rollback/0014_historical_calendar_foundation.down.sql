-- See migrations/rollback/0016_model_candidate_registry.down.sql for
-- why this must run as a single transaction (phase 3B.10 incident,
-- fixed preventively here in phase 3B.11 before this script was ever
-- run against real data).
BEGIN;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM features.historical_calendar_days)
       OR EXISTS (SELECT 1 FROM features.calendar_rule_versions) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: historical calendar data exists';
    END IF;
END $$;

DROP TABLE features.historical_calendar_days;
DROP TABLE features.calendar_rule_versions;

COMMIT;
