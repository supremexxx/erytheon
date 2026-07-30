-- Phase 4A.5: scientific snapshot registry (manifest) and a bounded pilot
-- of per-cell scientific values. Purely additive.
--
-- Architecture decision (see PHASE4A5_SNAPSHOT_ARCHITECTURE_DECISION.md):
-- a naive daily x multi-horizon per-cell capture was rejected as
-- disproportionate (~70-280 GB/year on a shared VPS). This migration
-- implements the scoped pilot instead: an always-on metadata manifest
-- (observability.scientific_snapshots, negligible size) plus a per-cell
-- VALUE table restricted in application code to a weekly cadence and the
-- nowcast horizon only (~10 GB/year). Static/slowly-changing features
-- (hist, wui, road, agri, combustible, population, poi, power_line,
-- calendar) are never duplicated here: they are already versioned by
-- features.feature_snapshots (migration 0013) and referenced by
-- static_snapshot_id below.
--
-- temporal_classification reuses the exact vocabulary already defined by
-- features.feature_snapshots (migration 0013) rather than inventing a
-- second, slightly different enum.

CREATE TABLE observability.scientific_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    logical_id TEXT NOT NULL,
    family TEXT NOT NULL DEFAULT 'dynamic_weather_fwi_nowcast',
    snapshot_type TEXT NOT NULL,
    resolution_h3 SMALLINT NOT NULL,
    valid_at TIMESTAMPTZ NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    source_period_start TIMESTAMPTZ,
    source_period_end TIMESTAMPTZ,
    application_revision TEXT,
    feature_schema_version TEXT NOT NULL,
    transform_version TEXT NOT NULL,
    source_versions JSONB NOT NULL DEFAULT '{}'::JSONB,
    static_snapshot_id UUID REFERENCES features.feature_snapshots(id) ON DELETE RESTRICT,
    cell_count_expected BIGINT NOT NULL,
    cell_count_present BIGINT NOT NULL DEFAULT 0,
    complete BOOLEAN NOT NULL DEFAULT FALSE,
    missing_count BIGINT NOT NULL DEFAULT 0,
    checksum TEXT,
    storage_kind TEXT NOT NULL,
    storage_location TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'building',
    temporal_classification TEXT NOT NULL,
    supersedes_id UUID REFERENCES observability.scientific_snapshots(id) ON DELETE RESTRICT,
    import_batch_id UUID REFERENCES ops.import_batches(id) ON DELETE RESTRICT,
    pipeline_run_id UUID REFERENCES ops.pipeline_runs(id) ON DELETE RESTRICT,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    CONSTRAINT scientific_snapshots_logical_id_not_blank CHECK (BTRIM(logical_id) <> ''),
    CONSTRAINT scientific_snapshots_type_check CHECK (
        snapshot_type IN ('weekly_full', 'metadata_only')
    ),
    CONSTRAINT scientific_snapshots_resolution_check CHECK (resolution_h3 BETWEEN 0 AND 15),
    CONSTRAINT scientific_snapshots_expected_check CHECK (cell_count_expected >= 0),
    CONSTRAINT scientific_snapshots_present_check CHECK (cell_count_present >= 0),
    CONSTRAINT scientific_snapshots_missing_check CHECK (missing_count >= 0),
    CONSTRAINT scientific_snapshots_storage_kind_check CHECK (
        storage_kind IN ('postgres_table', 'metadata_only')
    ),
    CONSTRAINT scientific_snapshots_storage_location_not_blank
        CHECK (BTRIM(storage_location) <> ''),
    CONSTRAINT scientific_snapshots_status_check CHECK (
        status IN ('building', 'validated', 'published', 'failed', 'superseded')
    ),
    CONSTRAINT scientific_snapshots_temporal_classification_check CHECK (
        temporal_classification IN (
            'historical_exact', 'historical_snapshot', 'stable_approximation',
            'current_snapshot_applied_historically', 'unavailable_historically',
            'derived_past_only'
        )
    ),
    CONSTRAINT scientific_snapshots_source_versions_object
        CHECK (JSONB_TYPEOF(source_versions) = 'object'),
    CONSTRAINT scientific_snapshots_metadata_object CHECK (JSONB_TYPEOF(metadata) = 'object'),
    CONSTRAINT scientific_snapshots_publication_check CHECK (
        (status = 'published' AND published_at IS NOT NULL AND checksum IS NOT NULL)
        OR (status <> 'published')
    ),
    CONSTRAINT scientific_snapshots_supersedes_not_self CHECK (supersedes_id IS NULL OR supersedes_id <> id),
    UNIQUE (logical_id)
);

CREATE INDEX scientific_snapshots_family_valid_at_idx
    ON observability.scientific_snapshots (family, valid_at DESC);
CREATE INDEX scientific_snapshots_status_idx
    ON observability.scientific_snapshots (status);
CREATE INDEX scientific_snapshots_captured_at_idx
    ON observability.scientific_snapshots (captured_at DESC);

-- Published is a terminal, immutable state: mirrors
-- ml.forbid_finalized_dataset_version_update (migration 0015). The trigger
-- blocks UPDATE; it deliberately does not and cannot block DELETE, so the
-- application layer (store::observability) must never expose a delete path
-- for a published row, and the retention policy documented in
-- PHASE4A5_RETENTION_POLICY.md never activates automatic deletion of
-- published rows in this phase.
CREATE OR REPLACE FUNCTION observability.forbid_published_scientific_snapshot_update()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.status = 'published' THEN
        RAISE EXCEPTION
            'refusing modification: scientific snapshot % is published and immutable',
            OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER scientific_snapshots_published_immutable
    BEFORE UPDATE ON observability.scientific_snapshots
    FOR EACH ROW
    EXECUTE FUNCTION observability.forbid_published_scientific_snapshot_update();

-- Pilot per-cell value storage: weekly cadence, nowcast horizon only,
-- enforced in application code (store::observability), not by a CHECK here,
-- since the horizon/cadence policy is expected to evolve without a schema
-- change once real volume is measured (see architecture decision §5).
CREATE TABLE observability.scientific_snapshot_values (
    id BIGSERIAL PRIMARY KEY,
    snapshot_id UUID NOT NULL REFERENCES observability.scientific_snapshots(id) ON DELETE RESTRICT,
    h3 BIGINT NOT NULL,
    valid_at TIMESTAMPTZ NOT NULL,
    temperature REAL,
    humidity REAL,
    wind_speed REAL,
    wind_direction REAL,
    precipitation REAL,
    ffmc REAL,
    dmc REAL,
    dc REAL,
    isi REAL,
    bui REAL,
    fwi REAL,
    data_status TEXT NOT NULL DEFAULT 'observed',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT scientific_snapshot_values_data_status_check CHECK (
        data_status IN ('observed', 'imputed', 'missing')
    ),
    UNIQUE (snapshot_id, h3)
);

CREATE INDEX scientific_snapshot_values_snapshot_idx
    ON observability.scientific_snapshot_values (snapshot_id);
CREATE INDEX scientific_snapshot_values_h3_idx
    ON observability.scientific_snapshot_values (h3);

COMMENT ON TABLE observability.scientific_snapshots IS
    'Immutable-once-published manifest of a scientific capture: identity, source versions, '
    'checksum, coverage. Never stores per-cell values directly.';
COMMENT ON COLUMN observability.scientific_snapshots.static_snapshot_id IS
    'Reference to the static feature bundle in effect at capture time (features.feature_snapshots). '
    'Static/slowly-changing features are never duplicated into scientific_snapshot_values.';
COMMENT ON TABLE observability.scientific_snapshot_values IS
    'Pilot per-cell dynamic weather/FWI values for one published snapshot. '
    'Restricted by application code to weekly cadence, nowcast horizon only (see '
    'PHASE4A5_SNAPSHOT_ARCHITECTURE_DECISION.md); not a general-purpose per-hour store.';
COMMENT ON COLUMN observability.scientific_snapshot_values.data_status IS
    'Distinguishes a real zero from an imputed value or a genuinely missing source reading.';
