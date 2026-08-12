DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM blue.evidence_cases
        WHERE review_stage='hours_24' OR provisional_verdict<>'pending'
    ) OR EXISTS (
        SELECT 1 FROM blue.evidence_runs
        WHERE review_horizon='hours_24' OR attempt_no>2
    ) THEN
        RAISE EXCEPTION 'refusing rollback: staged BLUE evidence exists';
    END IF;
END $$;

ALTER TABLE blue.evidence_runs
    DROP CONSTRAINT blue_evidence_runs_case_stage_attempt_key,
    DROP CONSTRAINT blue_evidence_runs_confidence_check,
    DROP CONSTRAINT blue_evidence_runs_verdict_check,
    DROP CONSTRAINT blue_evidence_runs_horizon_check,
    DROP CONSTRAINT blue_evidence_runs_stage_attempt_check,
    DROP CONSTRAINT blue_evidence_runs_attempt_check,
    ADD CONSTRAINT blue_evidence_runs_attempt_check CHECK (attempt_no BETWEEN 1 AND 2),
    ADD CONSTRAINT evidence_runs_case_id_attempt_no_key UNIQUE(case_id,attempt_no),
    DROP COLUMN observed_location,
    DROP COLUMN observed_event_at,
    DROP COLUMN summary,
    DROP COLUMN confidence,
    DROP COLUMN verdict,
    DROP COLUMN stage_attempt_no,
    DROP COLUMN review_horizon;

ALTER TABLE blue.evidence_cases
    DROP CONSTRAINT blue_evidence_cases_provisional_confidence_check,
    DROP CONSTRAINT blue_evidence_cases_provisional_verdict_check,
    DROP CONSTRAINT blue_evidence_cases_stage_check,
    DROP CONSTRAINT blue_evidence_cases_stage_attempt_check,
    DROP CONSTRAINT blue_evidence_cases_attempt_check,
    ADD CONSTRAINT blue_evidence_cases_attempt_check CHECK (attempt_count BETWEEN 0 AND 2),
    DROP COLUMN provisional_completed_at,
    DROP COLUMN provisional_observed_location,
    DROP COLUMN provisional_observed_event_at,
    DROP COLUMN provisional_summary,
    DROP COLUMN provisional_confidence,
    DROP COLUMN provisional_verdict,
    DROP COLUMN stage_attempt_count,
    DROP COLUMN review_stage;
