CREATE INDEX risk_scores_h3_computed_at_idx
    ON risk_scores (h3, computed_at DESC);

ALTER TABLE risk_scores
    ADD COLUMN input_date DATE;

UPDATE risk_scores
SET input_date = computed_at::date
WHERE input_date IS NULL;

ALTER TABLE risk_scores
    ALTER COLUMN input_date SET NOT NULL;

CREATE TABLE source_status (
    id TEXT PRIMARY KEY,
    last_run TIMESTAMPTZ NOT NULL,
    last_success TIMESTAMPTZ,
    observation_count BIGINT NOT NULL DEFAULT 0 CHECK (observation_count >= 0),
    recent_error TEXT
);
