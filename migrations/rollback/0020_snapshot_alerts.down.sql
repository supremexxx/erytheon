BEGIN;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'ml' AND table_name = 'snapshot_label_links'
    ) THEN
        RAISE EXCEPTION
            'refusing out-of-order rollback: migration 0021 must be rolled back before 0020';
    END IF;

    IF EXISTS (SELECT 1 FROM observability.snapshot_alerts) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: recorded alerts exist';
    END IF;
END $$;

DROP INDEX IF EXISTS observability.snapshot_alerts_rule_id_idx;
DROP INDEX IF EXISTS observability.snapshot_alerts_severity_idx;
DROP INDEX IF EXISTS observability.snapshot_alerts_detected_at_idx;
DROP TABLE IF EXISTS observability.snapshot_alerts;

COMMIT;
