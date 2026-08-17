-- Two-stage BLUE evidence review: an early +24 h result followed by the
-- authoritative +48 h review. Published forecasts remain immutable.

ALTER TABLE blue.evidence_cases
    ADD COLUMN review_stage TEXT NOT NULL DEFAULT 'hours_48',
    ADD COLUMN stage_attempt_count SMALLINT NOT NULL DEFAULT 0,
    ADD COLUMN provisional_verdict TEXT NOT NULL DEFAULT 'pending',
    ADD COLUMN provisional_confidence REAL,
    ADD COLUMN provisional_summary TEXT,
    ADD COLUMN provisional_observed_event_at TIMESTAMPTZ,
    ADD COLUMN provisional_observed_location TEXT,
    ADD COLUMN provisional_completed_at TIMESTAMPTZ;

UPDATE blue.evidence_cases c
SET stage_attempt_count=CASE
        WHEN c.status IN ('reviewed','failed') THEN LEAST(c.attempt_count,2)
        WHEN c.attempt_count=0 AND NOT EXISTS (
            SELECT 1 FROM blue.evidence_runs r WHERE r.case_id=c.id
        ) THEN 0
        ELSE LEAST(c.attempt_count,2) END,
    review_stage=CASE
        WHEN c.status IN ('reviewed','failed') THEN 'completed'
        WHEN c.attempt_count=0 AND c.alert_24h_id IS NOT NULL AND NOT EXISTS (
            SELECT 1 FROM blue.evidence_runs r WHERE r.case_id=c.id
        ) THEN 'hours_24'
        ELSE 'hours_48' END,
    research_after=CASE
        WHEN c.attempt_count=0 AND c.alert_24h_id IS NOT NULL AND NOT EXISTS (
            SELECT 1 FROM blue.evidence_runs r WHERE r.case_id=c.id
        ) THEN (SELECT a.valid_at+INTERVAL '3 hours'
            FROM blue.forecast_alerts a WHERE a.id=c.alert_24h_id)
        ELSE c.research_after END;

ALTER TABLE blue.evidence_cases
    DROP CONSTRAINT blue_evidence_cases_attempt_check,
    ADD CONSTRAINT blue_evidence_cases_attempt_check CHECK (attempt_count BETWEEN 0 AND 4),
    ADD CONSTRAINT blue_evidence_cases_stage_attempt_check CHECK (stage_attempt_count BETWEEN 0 AND 2),
    ADD CONSTRAINT blue_evidence_cases_stage_check CHECK (
        review_stage IN ('hours_24','hours_48','completed')
    ),
    ADD CONSTRAINT blue_evidence_cases_provisional_verdict_check CHECK (
        provisional_verdict IN (
            'pending','signal_observed','probable','confirmed','no_evidence_found','inconclusive'
        )
    ),
    ADD CONSTRAINT blue_evidence_cases_provisional_confidence_check CHECK (
        provisional_confidence IS NULL OR provisional_confidence BETWEEN 0 AND 1
    );

ALTER TABLE blue.evidence_runs
    ADD COLUMN review_horizon TEXT NOT NULL DEFAULT 'hours_48',
    ADD COLUMN stage_attempt_no SMALLINT,
    ADD COLUMN verdict TEXT,
    ADD COLUMN confidence REAL,
    ADD COLUMN summary TEXT,
    ADD COLUMN observed_event_at TIMESTAMPTZ,
    ADD COLUMN observed_location TEXT;

UPDATE blue.evidence_runs SET stage_attempt_no=attempt_no;

ALTER TABLE blue.evidence_runs
    ALTER COLUMN stage_attempt_no SET NOT NULL,
    DROP CONSTRAINT evidence_runs_case_id_attempt_no_key,
    DROP CONSTRAINT blue_evidence_runs_attempt_check,
    ADD CONSTRAINT blue_evidence_runs_attempt_check CHECK (attempt_no BETWEEN 1 AND 4),
    ADD CONSTRAINT blue_evidence_runs_stage_attempt_check CHECK (stage_attempt_no BETWEEN 1 AND 2),
    ADD CONSTRAINT blue_evidence_runs_horizon_check CHECK (
        review_horizon IN ('hours_24','hours_48')
    ),
    ADD CONSTRAINT blue_evidence_runs_verdict_check CHECK (
        verdict IS NULL OR verdict IN (
            'signal_observed','probable','confirmed','no_evidence_found','inconclusive'
        )
    ),
    ADD CONSTRAINT blue_evidence_runs_confidence_check CHECK (
        confidence IS NULL OR confidence BETWEEN 0 AND 1
    ),
    ADD CONSTRAINT blue_evidence_runs_case_stage_attempt_key
        UNIQUE(case_id,review_horizon,stage_attempt_no);

COMMENT ON COLUMN blue.evidence_cases.review_stage IS
    'Next review horizon. Unreviewed pre-0027 cases enter +24 h; cases already attempted resume at +48 h.';
COMMENT ON COLUMN blue.evidence_cases.provisional_verdict IS
    'Early +24 h result; never replaces the authoritative +48 h verdict.';
COMMENT ON COLUMN blue.evidence_runs.review_horizon IS
    'Forecast horizon whose elapsed observation window was searched by this run.';
