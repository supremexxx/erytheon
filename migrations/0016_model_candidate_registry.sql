-- Phase 3B.9: an additive registry for non-v1 model families
-- ("gbm_isotonic_v2" and any future family), deliberately separate from
-- `human_model_versions` (migration 0008), which is v1-specific: its
-- `train_from`/`validation_from` columns and `CHECK (train_to <
-- validation_from)` constraint encode v1's chronological train/
-- validation holdout, not the train/calibration/test split the
-- candidate pipeline uses. Reusing that table would either misuse
-- those columns or require relaxing v1's own constraints -- both
-- rejected in favor of a new table.
--
-- This table intentionally has no path to "active": `status` only
-- allows 'candidate' or 'inactive'. Activation remains solely a
-- `human_model_versions` concept until a future, separately authorized
-- migration unifies the two into one cross-family "active model"
-- registry (out of scope for phase 3B.9 -- see MODEL_PROMOTION_PLAN.md
-- P1 and later). This migration is proposed and reviewed, not applied,
-- in phase 3B.9.

CREATE TABLE ml.model_candidate_registry (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    model_family TEXT NOT NULL,
    model_name TEXT NOT NULL,
    artifact_version INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('candidate', 'inactive')),
    git_commit TEXT NOT NULL,
    dataset_logical_id TEXT NOT NULL,
    dataset_row_fingerprint TEXT NOT NULL,
    artifact JSONB NOT NULL,
    artifact_checksum TEXT NOT NULL,
    metrics JSONB NOT NULL,
    scientific_interpretation TEXT NOT NULL,
    known_limitations JSONB NOT NULL
);

CREATE INDEX model_candidate_registry_family_idx
    ON ml.model_candidate_registry (model_family, created_at DESC);
