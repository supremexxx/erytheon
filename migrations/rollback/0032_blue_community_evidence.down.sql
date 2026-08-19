DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM blue.ground_truth_confirmations
        WHERE evidence_level='community_reported'
    ) OR EXISTS (SELECT 1 FROM blue.ground_truth_rejections) THEN
        RAISE EXCEPTION
            'rollback blocked: export and review community confirmations and rejections first';
    END IF;
END;
$$;

DROP TRIGGER blue_ground_truth_rejections_immutable
    ON blue.ground_truth_rejections;
DROP FUNCTION blue.forbid_ground_truth_rejection_change();
DROP TABLE blue.ground_truth_rejections;

ALTER TABLE blue.ground_truth_confirmations
    DROP CONSTRAINT blue_ground_truth_confirmation_level_check,
    ADD CONSTRAINT blue_ground_truth_confirmation_level_check CHECK (
        evidence_level IN ('press_confirmed','authority_confirmed')
    );
