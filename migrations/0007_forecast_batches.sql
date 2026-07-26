CREATE TABLE forecast_batches (
    computed_at TIMESTAMPTZ PRIMARY KEY,
    completed_at TIMESTAMPTZ
);

CREATE INDEX forecast_batches_running_idx
    ON forecast_batches (computed_at)
    WHERE completed_at IS NULL;
