-- Deterministic evidence grounding. Invalidated AI runs remain append-only,
-- while their mutable case projection is cleared and safely re-queued.

CREATE TABLE blue.evidence_invalidations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL UNIQUE REFERENCES blue.evidence_runs(id) ON DELETE RESTRICT,
    case_id UUID NOT NULL REFERENCES blue.evidence_cases(id) ON DELETE RESTRICT,
    review_horizon TEXT NOT NULL,
    reason TEXT NOT NULL,
    invalidated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    invalidated_by TEXT NOT NULL,
    CONSTRAINT blue_evidence_invalidations_horizon_check CHECK (
        review_horizon IN ('hours_24','hours_48')
    ),
    CONSTRAINT blue_evidence_invalidations_text_check CHECK (
        BTRIM(reason)<>'' AND BTRIM(invalidated_by)<>''
    )
);
CREATE INDEX blue_evidence_invalidations_case_idx
    ON blue.evidence_invalidations(case_id,invalidated_at DESC);

CREATE FUNCTION blue.forbid_evidence_invalidation_change()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'BLUE evidence invalidations are append-only';
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER blue_evidence_invalidations_immutable
BEFORE UPDATE OR DELETE ON blue.evidence_invalidations
FOR EACH ROW EXECUTE FUNCTION blue.forbid_evidence_invalidation_change();

COMMENT ON TABLE blue.evidence_invalidations IS
    'Append-only audit of AI evidence rejected by deterministic date, location, provenance, or source-grounding rules.';

-- Invalidate every existing positive run that is already provably unsafe.
-- This catches the 2025 Ribaute evidence attributed to Narbonne/Carcassonne
-- and the Ucel response that invented a source without any web-search call.
INSERT INTO blue.evidence_invalidations(
    run_id,case_id,review_horizon,reason,invalidated_by)
SELECT r.id,r.case_id,r.review_horizon,
    CASE
        WHEN r.web_search_count=0 THEN 'positive verdict without web search'
        WHEN r.observed_event_at IS NULL THEN 'positive verdict without a valid event date'
        WHEN r.observed_event_at<b.issued_at OR r.observed_event_at>a.valid_at
            THEN 'event date outside the forecast window'
        ELSE 'positive evidence failed deterministic grounding'
    END,
    'migration_0030'
FROM blue.evidence_runs r
JOIN blue.evidence_cases c ON c.id=r.case_id
JOIN blue.forecast_bulletins b ON b.id=c.bulletin_id
JOIN blue.forecast_alerts a ON a.id=CASE
    WHEN r.review_horizon='hours_24' THEN c.alert_24h_id ELSE c.alert_48h_id END
WHERE r.status='completed'
  AND r.verdict IN ('signal_observed','probable','confirmed')
  AND (
      r.web_search_count=0
      OR r.observed_event_at IS NULL
      OR r.observed_event_at<b.issued_at
      OR r.observed_event_at>a.valid_at
  )
ON CONFLICT(run_id) DO NOTHING;

UPDATE blue.evidence_cases c
SET provisional_verdict='pending',provisional_confidence=NULL,
    provisional_summary=NULL,provisional_observed_event_at=NULL,
    provisional_observed_location=NULL,provisional_completed_at=NULL,
    verdict='pending',confidence=NULL,summary=NULL,observed_event_at=NULL,
    observed_location=NULL,response_id=NULL,status='retry_due',review_stage='hours_24',
    stage_attempt_count=0,next_attempt_at=NOW(),research_after=LEAST(c.research_after,NOW()),
    completed_at=NULL,updated_at=NOW()
FROM blue.evidence_invalidations i
WHERE i.case_id=c.id AND i.review_horizon='hours_24';

UPDATE blue.forecast_evaluations e
SET status='pending',observed_event_at=NULL,evidence_count=0,
    reviewer_note='Previous AI evidence invalidated by deterministic grounding',
    reviewed_at=NULL,updated_at=NOW()
FROM blue.evidence_invalidations i
JOIN blue.evidence_cases c ON c.id=i.case_id
WHERE i.review_horizon='hours_24' AND e.alert_id=c.alert_24h_id;
