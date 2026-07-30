BEGIN;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'observability' AND table_name = 'snapshot_alerts'
    ) OR EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'ml' AND table_name = 'snapshot_label_links'
    ) THEN
        RAISE EXCEPTION
            'refusing out-of-order rollback: migration 0020+ must be rolled back before 0019';
    END IF;

    IF EXISTS (SELECT 1 FROM observability.scientific_snapshot_values) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: scientific snapshot values exist';
    END IF;

    IF EXISTS (SELECT 1 FROM observability.scientific_snapshots) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: scientific snapshot manifests exist';
    END IF;
END $$;

DROP TRIGGER IF EXISTS scientific_snapshots_published_immutable ON observability.scientific_snapshots;
DROP FUNCTION IF EXISTS observability.forbid_published_scientific_snapshot_update();

DROP INDEX IF EXISTS observability.scientific_snapshot_values_h3_idx;
DROP INDEX IF EXISTS observability.scientific_snapshot_values_snapshot_idx;
DROP TABLE IF EXISTS observability.scientific_snapshot_values;

DROP INDEX IF EXISTS observability.scientific_snapshots_captured_at_idx;
DROP INDEX IF EXISTS observability.scientific_snapshots_status_idx;
DROP INDEX IF EXISTS observability.scientific_snapshots_family_valid_at_idx;
DROP TABLE IF EXISTS observability.scientific_snapshots;

COMMIT;
