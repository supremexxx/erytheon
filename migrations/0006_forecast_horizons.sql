ALTER TABLE risk_scores
    ADD COLUMN valid_at TIMESTAMPTZ;

UPDATE risk_scores
SET valid_at = computed_at
WHERE valid_at IS NULL;

ALTER TABLE risk_scores
    ALTER COLUMN valid_at SET NOT NULL;

ALTER TABLE risk_scores
    DROP CONSTRAINT risk_scores_pkey;

ALTER TABLE risk_scores
    ADD PRIMARY KEY (h3, computed_at, horizon);

CREATE INDEX risk_scores_horizon_computed_at_score_idx
    ON risk_scores (horizon, computed_at DESC, score DESC);

CREATE TABLE forecast_fwi (
    h3 BIGINT NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL,
    valid_at TIMESTAMPTZ NOT NULL,
    horizon TEXT NOT NULL,
    ffmc DOUBLE PRECISION NOT NULL,
    dmc DOUBLE PRECISION NOT NULL,
    dc DOUBLE PRECISION NOT NULL,
    isi DOUBLE PRECISION NOT NULL,
    bui DOUBLE PRECISION NOT NULL,
    fwi DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (h3, computed_at, horizon)
);

CREATE INDEX forecast_fwi_horizon_computed_at_idx
    ON forecast_fwi (horizon, computed_at DESC);
