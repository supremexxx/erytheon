DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM features.feature_snapshots) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: feature snapshot data exists';
    END IF;
END
$$;

DROP TABLE features.feature_snapshots;
