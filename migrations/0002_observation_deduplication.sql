ALTER TABLE observations
    ADD COLUMN dedupe_key TEXT;

CREATE UNIQUE INDEX observations_source_dedupe_key_idx
    ON observations (source, dedupe_key)
    WHERE dedupe_key IS NOT NULL;

