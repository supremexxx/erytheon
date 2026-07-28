-- Phase 3B.10 P1 audit addendum: migration 0016 (already applied to
-- the isolated deploy database as a side effect of phase 3B.9's own
-- `Store::connect` calls -- every connection runs pending migrations,
-- which the phase 3B.9 report did not account for; corrected here
-- rather than silently glossed over) had no uniqueness constraint,
-- which idempotent registration (mission section 12) requires at the
-- database level, not only in application logic.
--
-- A candidate's logical identity is (model_family, model_name,
-- dataset_logical_id, seed) -- the same trained lineage should never
-- be registered twice under a different artifact_checksum (that would
-- mean the artifact silently changed underneath an unchanged identity,
-- which registration must refuse, not overwrite).
--
-- Migration 0016 itself is left untouched (never edit an already-
-- applied migration) -- this is an additive ALTER instead.

ALTER TABLE ml.model_candidate_registry
    ADD COLUMN seed BIGINT NOT NULL DEFAULT 0;

ALTER TABLE ml.model_candidate_registry
    ALTER COLUMN seed DROP DEFAULT;

ALTER TABLE ml.model_candidate_registry
    ADD CONSTRAINT model_candidate_registry_logical_identity_unique
        UNIQUE (model_family, model_name, dataset_logical_id, seed);
