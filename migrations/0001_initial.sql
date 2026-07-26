CREATE EXTENSION IF NOT EXISTS postgis;

CREATE TABLE observations (
    id BIGSERIAL PRIMARY KEY,
    source TEXT NOT NULL,
    kind TEXT NOT NULL,
    h3 BIGINT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL
);

CREATE INDEX observations_h3_observed_at_idx
    ON observations (h3, observed_at DESC);
CREATE INDEX observations_source_observed_at_idx
    ON observations (source, observed_at DESC);

CREATE TABLE cell_static (
    h3 BIGINT PRIMARY KEY,
    features JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE fwi_state (
    h3 BIGINT NOT NULL,
    date DATE NOT NULL,
    ffmc REAL NOT NULL,
    dmc REAL NOT NULL,
    dc REAL NOT NULL,
    isi REAL NOT NULL,
    bui REAL NOT NULL,
    fwi REAL NOT NULL,
    PRIMARY KEY (h3, date)
);

CREATE TABLE risk_scores (
    h3 BIGINT NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL,
    horizon TEXT NOT NULL,
    score REAL NOT NULL CHECK (score >= 0 AND score <= 1),
    physical REAL NOT NULL CHECK (physical >= 0 AND physical <= 1),
    human REAL NOT NULL CHECK (human >= 0 AND human <= 1),
    factors JSONB NOT NULL,
    PRIMARY KEY (h3, computed_at)
);

CREATE INDEX risk_scores_computed_at_score_idx
    ON risk_scores (computed_at DESC, score DESC);

CREATE TABLE ignition_history (
    id BIGSERIAL PRIMARY KEY,
    occurred_at TIMESTAMPTZ NOT NULL,
    h3 BIGINT NOT NULL,
    source TEXT NOT NULL,
    payload JSONB NOT NULL
);

CREATE INDEX ignition_history_h3_occurred_at_idx
    ON ignition_history (h3, occurred_at DESC);

