ALTER TABLE ignition_history
    ADD COLUMN dedupe_key TEXT;

CREATE UNIQUE INDEX ignition_history_source_dedupe_key_idx
    ON ignition_history (source, dedupe_key)
    WHERE dedupe_key IS NOT NULL;

CREATE TABLE calendar_days (
    date DATE PRIMARY KEY,
    school_holiday BOOLEAN NOT NULL,
    public_holiday BOOLEAN NOT NULL,
    label TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
