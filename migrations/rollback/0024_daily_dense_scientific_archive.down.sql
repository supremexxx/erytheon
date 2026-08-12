BEGIN;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM observability.scientific_dense_archives) THEN
        RAISE EXCEPTION
            'refusing rollback 0024: compact daily scientific archives exist';
    END IF;
END;
$$;

DROP TRIGGER IF EXISTS scientific_dense_archives_published_immutable
    ON observability.scientific_dense_archives;
DROP FUNCTION IF EXISTS observability.forbid_published_dense_archive_change();
DROP TABLE observability.scientific_dense_archives;

ALTER TABLE observability.scientific_snapshots
    DROP CONSTRAINT scientific_snapshots_type_check,
    ADD CONSTRAINT scientific_snapshots_type_check CHECK (
        snapshot_type IN ('weekly_full', 'metadata_only')
    );

DELETE FROM _sqlx_migrations WHERE version = 24;

COMMIT;
