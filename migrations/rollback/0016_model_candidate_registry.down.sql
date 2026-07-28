DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM ml.model_candidate_registry) THEN
        RAISE EXCEPTION
            'refusing destructive rollback: model candidate registry data exists';
    END IF;
END $$;

DROP INDEX IF EXISTS ml.model_candidate_registry_family_idx;
DROP TABLE IF EXISTS ml.model_candidate_registry;
