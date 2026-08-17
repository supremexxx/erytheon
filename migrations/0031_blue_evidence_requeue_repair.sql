-- Repair the retry cursor for evidence cases invalidated by migration 0030.
-- Historical runs are append-only, so a re-analysis must continue with the
-- next unused stage attempt instead of starting again at attempt one.

WITH pending_reanalysis AS (
    SELECT DISTINCT i.case_id,i.review_horizon
    FROM blue.evidence_invalidations i
    WHERE NOT EXISTS (
        SELECT 1
        FROM blue.evidence_runs newer
        WHERE newer.case_id=i.case_id
          AND newer.review_horizon=i.review_horizon
          AND newer.started_at>i.invalidated_at
    )
), retry_cursor AS (
    SELECT p.case_id,p.review_horizon,
        COALESCE(MAX(r.stage_attempt_no),0)::SMALLINT AS used_stage_attempts
    FROM pending_reanalysis p
    LEFT JOIN blue.evidence_runs r
      ON r.case_id=p.case_id AND r.review_horizon=p.review_horizon
    GROUP BY p.case_id,p.review_horizon
)
UPDATE blue.evidence_cases c
SET status='retry_due',
    review_stage=retry_state.review_horizon,
    stage_attempt_count=retry_state.used_stage_attempts,
    next_attempt_at=NOW(),
    updated_at=NOW()
FROM retry_cursor retry_state
WHERE c.id=retry_state.case_id
  AND retry_state.used_stage_attempts<3;
