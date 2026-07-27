DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM ml.dataset_versions)
       OR EXISTS (SELECT 1 FROM ml.dataset_builds)
       OR EXISTS (SELECT 1 FROM ml.dataset_rows)
       OR EXISTS (SELECT 1 FROM ml.dataset_row_snapshots)
       OR EXISTS (SELECT 1 FROM ml.dataset_event_links)
       OR EXISTS (SELECT 1 FROM ml.dataset_exclusions) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: dataset versioning data exists';
    END IF;
END
$$;

DROP TABLE ml.dataset_exclusions;
DROP TABLE ml.dataset_event_links;
DROP TABLE ml.dataset_row_snapshots;
DROP TABLE ml.dataset_rows;
DROP TABLE ml.dataset_builds;
DROP TRIGGER dataset_versions_finalized_immutable ON ml.dataset_versions;
DROP FUNCTION ml.forbid_finalized_dataset_version_update();
DROP TABLE ml.dataset_versions;
