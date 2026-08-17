DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM blue.ground_truth_confirmations LIMIT 1)
       OR EXISTS (
           SELECT 1 FROM blue.evidence_cases
           WHERE selection_reason<>'legacy_national_top'
              OR trigger_observation_id IS NOT NULL
       )
       OR EXISTS (
           SELECT 1 FROM blue.evidence_runs
           WHERE attempt_no>4 OR stage_attempt_no>2
       ) THEN
        RAISE EXCEPTION 'refusing rollback 0029: hardened BLUE evidence data exists';
    END IF;
END $$;

DROP TRIGGER blue_ground_truth_confirmations_immutable ON blue.ground_truth_confirmations;
DROP FUNCTION blue.forbid_ground_truth_confirmation_change();
DROP TABLE blue.ground_truth_confirmations;

ALTER TABLE blue.evidence_runs
    DROP CONSTRAINT blue_evidence_runs_stage_attempt_check,
    DROP CONSTRAINT blue_evidence_runs_attempt_check,
    ADD CONSTRAINT blue_evidence_runs_attempt_check CHECK (attempt_no BETWEEN 1 AND 4),
    ADD CONSTRAINT blue_evidence_runs_stage_attempt_check CHECK (stage_attempt_no BETWEEN 1 AND 2);
ALTER TABLE blue.evidence_cases
    DROP CONSTRAINT blue_evidence_cases_reactive_trigger_check,
    DROP CONSTRAINT blue_evidence_cases_selection_reason_check,
    DROP CONSTRAINT blue_evidence_cases_stage_attempt_check,
    DROP CONSTRAINT blue_evidence_cases_attempt_check,
    DROP COLUMN trigger_observation_id,
    DROP COLUMN selection_reason,
    ADD CONSTRAINT blue_evidence_cases_attempt_check CHECK (attempt_count BETWEEN 0 AND 4),
    ADD CONSTRAINT blue_evidence_cases_stage_attempt_check CHECK (stage_attempt_count BETWEEN 0 AND 2);
