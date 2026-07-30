BEGIN;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM ml.snapshot_label_links) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: snapshot label links exist';
    END IF;
END $$;

DROP INDEX IF EXISTS ml.snapshot_label_links_h3_date_idx;
DROP INDEX IF EXISTS ml.snapshot_label_links_event_idx;
DROP INDEX IF EXISTS ml.snapshot_label_links_snapshot_idx;
DROP TABLE IF EXISTS ml.snapshot_label_links;

COMMIT;
