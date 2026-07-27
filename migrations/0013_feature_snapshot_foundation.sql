-- Phase 3B.3: versioned feature-snapshot metadata.
--
-- This migration is purely additive and does not touch `public`, `fire`,
-- or `validation`. It does not duplicate `public.cell_static` rows: with a
-- single real static-feature load in production today (no historical
-- vintages available), a snapshot record captures metadata and a checksum
-- of the referenced data rather than materializing a copy. When a real
-- historical vintage becomes available, it should be registered as its
-- own snapshot referencing its own frozen storage; this table does not
-- prevent that evolution.

CREATE TABLE features.feature_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    family TEXT NOT NULL,
    source TEXT NOT NULL,
    provider TEXT,
    vintage TEXT,
    valid_from DATE,
    valid_until DATE,
    available_from TIMESTAMPTZ NOT NULL,
    available_until TIMESTAMPTZ,
    retrieved_at TIMESTAMPTZ,
    import_batch_id UUID REFERENCES ops.import_batches(id) ON DELETE RESTRICT,
    pipeline_run_id UUID REFERENCES ops.pipeline_runs(id) ON DELETE RESTRICT,
    code_version TEXT NOT NULL,
    normalizer_version TEXT NOT NULL,
    parameters JSONB NOT NULL,
    source_checksum TEXT,
    logical_checksum TEXT NOT NULL,
    reference_table TEXT NOT NULL,
    cell_count BIGINT NOT NULL,
    h3_resolution SMALLINT NOT NULL,
    geographic_coverage JSONB,
    status TEXT NOT NULL DEFAULT 'draft',
    temporal_classification TEXT NOT NULL,
    limitations JSONB NOT NULL DEFAULT '[]'::JSONB,
    license_attribution TEXT,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    validated_at TIMESTAMPTZ,
    CONSTRAINT feature_snapshots_family_not_blank CHECK (BTRIM(family) <> ''),
    CONSTRAINT feature_snapshots_source_not_blank CHECK (BTRIM(source) <> ''),
    CONSTRAINT feature_snapshots_code_version_not_blank CHECK (BTRIM(code_version) <> ''),
    CONSTRAINT feature_snapshots_normalizer_version_not_blank
        CHECK (BTRIM(normalizer_version) <> ''),
    CONSTRAINT feature_snapshots_logical_checksum_not_blank
        CHECK (BTRIM(logical_checksum) <> ''),
    CONSTRAINT feature_snapshots_reference_table_not_blank
        CHECK (BTRIM(reference_table) <> ''),
    CONSTRAINT feature_snapshots_parameters_object CHECK (JSONB_TYPEOF(parameters) = 'object'),
    CONSTRAINT feature_snapshots_limitations_array CHECK (JSONB_TYPEOF(limitations) = 'array'),
    CONSTRAINT feature_snapshots_coverage_object CHECK (
        geographic_coverage IS NULL OR JSONB_TYPEOF(geographic_coverage) = 'object'
    ),
    CONSTRAINT feature_snapshots_cell_count_check CHECK (cell_count >= 0),
    CONSTRAINT feature_snapshots_resolution_check CHECK (h3_resolution BETWEEN 0 AND 15),
    CONSTRAINT feature_snapshots_validity_check CHECK (
        valid_until IS NULL OR valid_from IS NULL OR valid_until >= valid_from
    ),
    CONSTRAINT feature_snapshots_availability_check CHECK (
        available_until IS NULL OR available_until >= available_from
    ),
    CONSTRAINT feature_snapshots_status_check CHECK (
        status IN ('draft', 'validated', 'active', 'superseded', 'failed')
    ),
    CONSTRAINT feature_snapshots_activation_check CHECK (
        (status IN ('validated', 'active', 'superseded') AND validated_at IS NOT NULL)
        OR (status IN ('draft', 'failed'))
    ),
    CONSTRAINT feature_snapshots_temporal_classification_check CHECK (
        temporal_classification IN (
            'historical_exact', 'historical_snapshot', 'stable_approximation',
            'current_snapshot_applied_historically', 'unavailable_historically',
            'derived_past_only'
        )
    ),
    UNIQUE (family, logical_checksum)
);

-- Exactly one active snapshot per feature family, mirroring
-- validation.rule_versions_one_active_per_type.
CREATE UNIQUE INDEX feature_snapshots_one_active_per_family
    ON features.feature_snapshots (family)
    WHERE status = 'active';

CREATE INDEX feature_snapshots_family_status_idx
    ON features.feature_snapshots (family, status);

COMMENT ON TABLE features.feature_snapshots IS
    'Versioned metadata for a coherent batch of model-ready feature values. '
    'Does not materialize a copy of the referenced data; see reference_table. '
    'A validated or active snapshot must never be modified silently.';
COMMENT ON COLUMN features.feature_snapshots.reference_table IS
    'Fully qualified table the snapshot metadata describes, e.g. public.cell_static. '
    'The snapshot does not own or duplicate this data.';
COMMENT ON COLUMN features.feature_snapshots.logical_checksum IS
    'Deterministic SHA-256 over the referenced data as of available_from, '
    'used to detect silent drift if the referenced table is later mutated in place.';
COMMENT ON COLUMN features.feature_snapshots.temporal_classification IS
    'One of: historical_exact, historical_snapshot, stable_approximation, '
    'current_snapshot_applied_historically, unavailable_historically, derived_past_only.';
