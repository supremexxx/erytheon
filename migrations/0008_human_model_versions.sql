CREATE TABLE human_model_versions (
    id BIGSERIAL PRIMARY KEY,
    trained_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    train_from DATE NOT NULL,
    train_to DATE NOT NULL,
    validation_from DATE NOT NULL,
    validation_to DATE NOT NULL,
    train_positive_count INTEGER NOT NULL CHECK (train_positive_count > 0),
    train_negative_count INTEGER NOT NULL CHECK (train_negative_count > 0),
    validation_positive_count INTEGER NOT NULL CHECK (validation_positive_count > 0),
    validation_negative_count INTEGER NOT NULL CHECK (validation_negative_count > 0),
    artifact JSONB NOT NULL,
    metrics JSONB NOT NULL,
    active BOOLEAN NOT NULL DEFAULT FALSE,
    CHECK (train_from <= train_to),
    CHECK (validation_from <= validation_to),
    CHECK (train_to < validation_from)
);

CREATE UNIQUE INDEX human_model_versions_one_active_idx
    ON human_model_versions (active)
    WHERE active;
