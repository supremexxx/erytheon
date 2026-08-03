DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM reference.commune_boundaries) THEN
        RAISE EXCEPTION 'refusing rollback 0023: commune_boundaries data exists';
    END IF;
END $$;

DROP TABLE IF EXISTS reference.commune_boundaries;
