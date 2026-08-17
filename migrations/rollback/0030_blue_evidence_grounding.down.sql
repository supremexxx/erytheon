DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM blue.evidence_invalidations LIMIT 1) THEN
        RAISE EXCEPTION 'refusing rollback 0030: evidence invalidation audit exists';
    END IF;
END $$;

DROP TRIGGER blue_evidence_invalidations_immutable ON blue.evidence_invalidations;
DROP FUNCTION blue.forbid_evidence_invalidation_change();
DROP TABLE blue.evidence_invalidations;
