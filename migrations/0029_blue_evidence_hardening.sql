-- BLUE evidence hardening: diversified proactive sampling, bounded reactive
-- reviews, tolerant structured-output repair, and an immutable human-reviewed
-- confirmation register.

ALTER TABLE blue.evidence_cases
    ADD COLUMN selection_reason TEXT NOT NULL DEFAULT 'legacy_national_top',
    ADD COLUMN trigger_observation_id BIGINT
        REFERENCES blue.ground_truth_observations(id) ON DELETE RESTRICT,
    ADD CONSTRAINT blue_evidence_cases_selection_reason_check CHECK (
        selection_reason IN (
            'legacy_national_top','national_top','territorial_top',
            'risk_acceleration','reactive_signal'
        )
    ),
    ADD CONSTRAINT blue_evidence_cases_reactive_trigger_check CHECK (
        (selection_reason='reactive_signal' AND trigger_observation_id IS NOT NULL)
        OR (selection_reason<>'reactive_signal' AND trigger_observation_id IS NULL)
    );
CREATE INDEX blue_evidence_cases_trigger_idx
    ON blue.evidence_cases(trigger_observation_id)
    WHERE trigger_observation_id IS NOT NULL;

-- A third attempt is reserved for structured-output validation failures. The
-- normal HTTP/API retry policy remains bounded to two attempts in application
-- logic.
ALTER TABLE blue.evidence_cases
    DROP CONSTRAINT blue_evidence_cases_attempt_check,
    DROP CONSTRAINT blue_evidence_cases_stage_attempt_check,
    ADD CONSTRAINT blue_evidence_cases_attempt_check CHECK (attempt_count BETWEEN 0 AND 6),
    ADD CONSTRAINT blue_evidence_cases_stage_attempt_check CHECK (stage_attempt_count BETWEEN 0 AND 3);
ALTER TABLE blue.evidence_runs
    DROP CONSTRAINT blue_evidence_runs_attempt_check,
    DROP CONSTRAINT blue_evidence_runs_stage_attempt_check,
    ADD CONSTRAINT blue_evidence_runs_attempt_check CHECK (attempt_no BETWEEN 1 AND 6),
    ADD CONSTRAINT blue_evidence_runs_stage_attempt_check CHECK (stage_attempt_no BETWEEN 1 AND 3);

CREATE TABLE blue.ground_truth_confirmations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bulletin_id UUID NOT NULL REFERENCES blue.forecast_bulletins(id) ON DELETE RESTRICT,
    insee_code TEXT NOT NULL REFERENCES reference.commune_boundaries(insee_code) ON DELETE RESTRICT,
    event_date DATE NOT NULL,
    event_started_at TIMESTAMPTZ,
    evidence_level TEXT NOT NULL,
    source_url TEXT NOT NULL,
    source_title TEXT NOT NULL,
    source_published_on DATE NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    verified_by TEXT NOT NULL,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(bulletin_id,insee_code,source_url),
    CONSTRAINT blue_ground_truth_confirmation_level_check CHECK (
        evidence_level IN ('press_confirmed','authority_confirmed')
    ),
    CONSTRAINT blue_ground_truth_confirmation_url_check CHECK (source_url ~ '^https?://'),
    CONSTRAINT blue_ground_truth_confirmation_text_check CHECK (
        BTRIM(source_title)<>'' AND BTRIM(verified_by)<>''
    )
);
CREATE INDEX blue_ground_truth_confirmations_event_idx
    ON blue.ground_truth_confirmations(event_date DESC,insee_code);

CREATE FUNCTION blue.forbid_ground_truth_confirmation_change()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'BLUE Ground Truth confirmations are append-only';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER blue_ground_truth_confirmations_immutable
BEFORE UPDATE OR DELETE ON blue.ground_truth_confirmations
FOR EACH ROW EXECUTE FUNCTION blue.forbid_ground_truth_confirmation_change();

COMMENT ON TABLE blue.ground_truth_confirmations IS
    'Append-only human-reviewed confirmations kept separate from thermal signals and delayed official registries.';
COMMENT ON COLUMN blue.evidence_cases.selection_reason IS
    'Deterministic reason for proactive or signal-triggered evidence selection.';
COMMENT ON COLUMN blue.evidence_cases.trigger_observation_id IS
    'Thermal-signal window that triggered a bounded reactive evidence review.';

-- First documented BLUE success. The article confirms a Monthermé fire during
-- the night following the immutable 2026-08-15 bulletin. No artificial event
-- start time is invented: only the independently known date is stored.
INSERT INTO blue.ground_truth_confirmations(
    bulletin_id,insee_code,event_date,evidence_level,source_url,source_title,
    source_published_on,verified_by,notes)
SELECT b.id,'08302',DATE '2026-08-16','press_confirmed',
    'https://www.lardennais.fr/id822709/article/2026-08-16/feu-de-montherme-le-point-sur-ce-quil-sest-passe-cette-nuit',
    'Feu de Monthermé : le point sur ce qu''il s''est passé cette nuit',
    DATE '2026-08-16','manual_review',
    'Commune-level confirmation; exact ignition time remains unavailable.'
FROM blue.forecast_bulletins b
WHERE b.bulletin_date=DATE '2026-08-15' AND b.status='published'
  AND EXISTS (
      SELECT 1 FROM blue.forecast_alerts a
      WHERE a.bulletin_id=b.id AND a.insee_code='08302'
  )
ON CONFLICT(bulletin_id,insee_code,source_url) DO NOTHING;

-- The four +24 h cases that exhausted both attempts solely because the model
-- returned a non-RFC3339 date receive exactly one repaired attempt after the
-- tolerant parser is deployed. Failed audit runs remain append-only.
UPDATE blue.evidence_cases c
SET status='retry_due',review_stage='hours_24',stage_attempt_count=2,
    next_attempt_at=NOW(),research_after=LEAST(research_after,NOW()),updated_at=NOW()
FROM blue.forecast_bulletins b
WHERE b.id=c.bulletin_id AND b.bulletin_date=DATE '2026-08-15'
  AND c.provisional_completed_at IS NULL
  AND c.provisional_verdict='pending'
  AND (
      SELECT COUNT(*) FROM blue.evidence_runs r
      WHERE r.case_id=c.id AND r.review_horizon='hours_24'
        AND r.status='failed'
        AND r.error='invalid evidence output: invalid observed_event_at'
  )=2;
