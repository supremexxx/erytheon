-- See migrations/rollback/0016_model_candidate_registry.down.sql for
-- why this must run as a single transaction (phase 3B.10 incident,
-- fixed preventively here in phase 3B.11 before this script was ever
-- run against real data).
BEGIN;

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
END $$;

DROP TABLE ml.dataset_exclusions;
DROP TABLE ml.dataset_event_links;
DROP TABLE ml.dataset_row_snapshots;
DROP TABLE ml.dataset_rows;
DROP TABLE ml.dataset_builds;
DROP TRIGGER dataset_versions_finalized_immutable ON ml.dataset_versions;
DROP FUNCTION ml.forbid_finalized_dataset_version_update();
DROP TABLE ml.dataset_versions;

COMMIT;
