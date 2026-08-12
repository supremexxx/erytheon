DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM blue.evidence_cases LIMIT 1) THEN
        RAISE EXCEPTION 'refusing rollback: BLUE evidence cases exist';
    END IF;
END $$;

DROP TABLE blue.evidence_sources;
DROP TABLE blue.evidence_runs;
DROP TABLE blue.evidence_cases;
