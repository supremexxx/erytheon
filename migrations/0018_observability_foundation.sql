-- Phase 4A.5: operational observability foundation. Purely additive.
-- Does not touch `public`, `fire`, `features`, `ml`, or the risk engine.
-- Stores periodic synthetic snapshots of system health (aggregates and
-- statuses only -- never per-cell values, see observability.scientific_*
-- in migration 0019 for that). One row per (environment, capture_date,
-- cadence): a daily official capture and an optional lighter hourly
-- capture share the table but never collide, and an explicit "event"
-- cadence covers deployments/incidents without polluting the routine
-- cadences.

CREATE SCHEMA IF NOT EXISTS observability;

-- Needed for observability.scientific_snapshots' deterministic checksum
-- (digest(...), migration 0019); gen_random_uuid() used elsewhere in this
-- schema is already built into core PostgreSQL and does not need it.
CREATE EXTENSION IF NOT EXISTS pgcrypto;

COMMENT ON SCHEMA observability IS
    'Automated, read-mostly operational and scientific observability history. '
    'Never a source of truth for scoring, training, or activation decisions.';

CREATE TABLE observability.system_snapshots (
    id BIGSERIAL PRIMARY KEY,
    captured_at TIMESTAMPTZ NOT NULL,
    capture_date DATE NOT NULL,
    environment TEXT NOT NULL,
    cadence TEXT NOT NULL,
    application_revision TEXT,
    application_image TEXT,
    application_healthy BOOLEAN,
    database_healthy BOOLEAN,
    caddy_state TEXT NOT NULL DEFAULT 'non_exposed',
    application_restart_count BIGINT,
    migrations_applied INTEGER,
    migrations_failed INTEGER,
    active_model_id BIGINT,
    active_model_name TEXT,
    active_model_count INTEGER,
    candidate_id BIGINT,
    candidate_name TEXT,
    candidate_status TEXT,
    shadow_scoring_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    firms_observation_count BIGINT,
    firms_last_success_at TIMESTAMPTZ,
    firms_age_seconds BIGINT,
    forecast_last_complete_at TIMESTAMPTZ,
    forecast_age_seconds BIGINT,
    forecast_horizon_count INTEGER,
    import_batches_total BIGINT,
    import_batches_success_24h BIGINT,
    import_batches_failed_24h BIGINT,
    pipeline_runs_total BIGINT,
    pipeline_runs_success_24h BIGINT,
    pipeline_runs_failed_24h BIGINT,
    warning_count_24h BIGINT,
    error_count_24h BIGINT,
    static_cell_count BIGINT,
    feature_snapshot_count INTEGER,
    dataset_version_count INTEGER,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    checksum TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT system_snapshots_environment_not_blank CHECK (BTRIM(environment) <> ''),
    CONSTRAINT system_snapshots_cadence_check CHECK (
        cadence IN ('daily', 'hourly', 'event')
    ),
    CONSTRAINT system_snapshots_caddy_state_check CHECK (
        caddy_state IN ('running', 'degraded', 'down', 'unknown', 'non_exposed')
    ),
    CONSTRAINT system_snapshots_candidate_status_check CHECK (
        candidate_status IS NULL OR candidate_status IN ('candidate', 'inactive')
    ),
    CONSTRAINT system_snapshots_active_model_count_check CHECK (
        active_model_count IS NULL OR active_model_count >= 0
    ),
    CONSTRAINT system_snapshots_metadata_object CHECK (JSONB_TYPEOF(metadata) = 'object'),
    CONSTRAINT system_snapshots_checksum_not_blank CHECK (BTRIM(checksum) <> ''),
    -- One official snapshot per environment/day/cadence: a re-run of the same
    -- window recomputes deterministically and must confirm the same checksum
    -- rather than silently multiply rows (see store::observability for the
    -- idempotent-upsert policy this constraint enforces).
    CONSTRAINT system_snapshots_identity_unique UNIQUE (environment, capture_date, cadence)
);

CREATE INDEX system_snapshots_captured_at_idx
    ON observability.system_snapshots (captured_at DESC);
CREATE INDEX system_snapshots_environment_cadence_idx
    ON observability.system_snapshots (environment, cadence, capture_date DESC);

COMMENT ON TABLE observability.system_snapshots IS
    'Periodic synthetic health snapshot: aggregates and statuses only, never per-cell data.';
COMMENT ON COLUMN observability.system_snapshots.caddy_state IS
    'Reported only when explicitly captured by a separate VPS-side component; '
    'defaults to non_exposed rather than inventing a value PostgreSQL cannot observe.';
COMMENT ON COLUMN observability.system_snapshots.checksum IS
    'Deterministic checksum of the normalized field set, used to confirm idempotent recomputation.';
