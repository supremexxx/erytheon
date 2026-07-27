-- Phase 3B.3: dataset versioning foundation for
-- erytheon_human_ignition_cell_day_v1. Purely additive. Does not touch
-- `public`, `fire`, or `validation`. Does not change the active model,
-- API, interface, FWI, or FIRMS. Does not switch any reader to
-- ml.dataset_rows.

CREATE TABLE ml.dataset_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    logical_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    observation_unit TEXT NOT NULL,
    h3_resolution SMALLINT NOT NULL,
    timezone TEXT NOT NULL,
    period_start DATE NOT NULL,
    period_end DATE NOT NULL,
    variant TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    code_version TEXT NOT NULL,
    migrations JSONB NOT NULL,
    quality_rule_versions JSONB NOT NULL,
    feature_snapshot_ids JSONB NOT NULL,
    calendar_rule_version_id UUID REFERENCES features.calendar_rule_versions(id) ON DELETE RESTRICT,
    inclusion_rules JSONB NOT NULL,
    exclusion_rules JSONB NOT NULL,
    negative_strategy TEXT NOT NULL,
    negative_parameters JSONB NOT NULL,
    seed BIGINT NOT NULL,
    splits JSONB NOT NULL,
    checksum TEXT,
    row_count BIGINT,
    positive_count BIGINT,
    negative_count BIGINT,
    exclusion_count BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finalized_at TIMESTAMPTZ,
    author_or_pipeline TEXT NOT NULL,
    notes TEXT,
    CONSTRAINT dataset_versions_logical_id_not_blank CHECK (BTRIM(logical_id) <> ''),
    CONSTRAINT dataset_versions_unit_not_blank CHECK (BTRIM(observation_unit) <> ''),
    CONSTRAINT dataset_versions_timezone_not_blank CHECK (BTRIM(timezone) <> ''),
    CONSTRAINT dataset_versions_resolution_check CHECK (h3_resolution BETWEEN 0 AND 15),
    CONSTRAINT dataset_versions_period_check CHECK (period_end >= period_start),
    CONSTRAINT dataset_versions_variant_check CHECK (variant IN ('strict', 'inclusive')),
    CONSTRAINT dataset_versions_status_check CHECK (
        status IN ('draft', 'building', 'validated', 'finalized', 'failed', 'superseded')
    ),
    CONSTRAINT dataset_versions_migrations_array CHECK (JSONB_TYPEOF(migrations) = 'array'),
    CONSTRAINT dataset_versions_quality_rules_array
        CHECK (JSONB_TYPEOF(quality_rule_versions) = 'array'),
    CONSTRAINT dataset_versions_snapshot_ids_array
        CHECK (JSONB_TYPEOF(feature_snapshot_ids) = 'array'),
    CONSTRAINT dataset_versions_inclusion_rules_object
        CHECK (JSONB_TYPEOF(inclusion_rules) = 'object'),
    CONSTRAINT dataset_versions_exclusion_rules_object
        CHECK (JSONB_TYPEOF(exclusion_rules) = 'object'),
    CONSTRAINT dataset_versions_negative_parameters_object
        CHECK (JSONB_TYPEOF(negative_parameters) = 'object'),
    CONSTRAINT dataset_versions_splits_object CHECK (JSONB_TYPEOF(splits) = 'object'),
    CONSTRAINT dataset_versions_finalization_check CHECK (
        (status = 'finalized' AND finalized_at IS NOT NULL AND checksum IS NOT NULL)
        OR (status <> 'finalized')
    ),
    UNIQUE (logical_id)
);

CREATE OR REPLACE FUNCTION ml.forbid_finalized_dataset_version_update()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.status = 'finalized' THEN
        RAISE EXCEPTION
            'refusing modification: dataset version % is finalized and immutable',
            OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER dataset_versions_finalized_immutable
    BEFORE UPDATE ON ml.dataset_versions
    FOR EACH ROW
    EXECUTE FUNCTION ml.forbid_finalized_dataset_version_update();

CREATE TABLE ml.dataset_builds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_version_id UUID NOT NULL REFERENCES ml.dataset_versions(id) ON DELETE RESTRICT,
    import_batch_id UUID REFERENCES ops.import_batches(id) ON DELETE RESTRICT,
    pipeline_run_id UUID REFERENCES ops.pipeline_runs(id) ON DELETE RESTRICT,
    code_version TEXT NOT NULL,
    builder_version TEXT NOT NULL,
    parameters JSONB NOT NULL,
    seed BIGINT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'running',
    row_count BIGINT,
    positive_count BIGINT,
    negative_count BIGINT,
    exclusion_count BIGINT,
    error_message TEXT,
    checksum TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT dataset_builds_code_version_not_blank CHECK (BTRIM(code_version) <> ''),
    CONSTRAINT dataset_builds_builder_version_not_blank CHECK (BTRIM(builder_version) <> ''),
    CONSTRAINT dataset_builds_parameters_object CHECK (JSONB_TYPEOF(parameters) = 'object'),
    CONSTRAINT dataset_builds_status_check CHECK (
        status IN ('running', 'succeeded', 'failed')
    ),
    CONSTRAINT dataset_builds_finish_check CHECK (
        finished_at IS NULL OR finished_at >= started_at
    ),
    CONSTRAINT dataset_builds_completion_check CHECK (
        (status = 'succeeded' AND finished_at IS NOT NULL AND checksum IS NOT NULL)
        OR (status = 'failed' AND finished_at IS NOT NULL)
        OR (status = 'running')
    )
);

CREATE INDEX dataset_builds_version_idx ON ml.dataset_builds (dataset_version_id, started_at DESC);

CREATE TABLE ml.dataset_rows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_version_id UUID NOT NULL REFERENCES ml.dataset_versions(id) ON DELETE RESTRICT,
    build_id UUID NOT NULL REFERENCES ml.dataset_builds(id) ON DELETE RESTRICT,
    deterministic_key TEXT NOT NULL,
    h3 BIGINT NOT NULL,
    h3_resolution SMALLINT NOT NULL,
    local_date DATE NOT NULL,
    reference_timestamp TIMESTAMPTZ NOT NULL,
    split TEXT NOT NULL,
    label SMALLINT NOT NULL,
    row_category TEXT NOT NULL,
    selection_method TEXT NOT NULL,
    weight DOUBLE PRECISION,
    quality_flags JSONB NOT NULL DEFAULT '[]'::JSONB,
    features JSONB NOT NULL,
    temporal_availability JSONB NOT NULL,
    row_checksum TEXT NOT NULL,
    justification TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT dataset_rows_resolution_check CHECK (h3_resolution BETWEEN 0 AND 15),
    CONSTRAINT dataset_rows_split_check CHECK (
        split IN ('train', 'calibration', 'test', 'prospective')
    ),
    CONSTRAINT dataset_rows_label_check CHECK (label IN (0, 1)),
    CONSTRAINT dataset_rows_category_check CHECK (
        row_category IN ('positive', 'negative_pilot')
    ),
    CONSTRAINT dataset_rows_quality_flags_array CHECK (JSONB_TYPEOF(quality_flags) = 'array'),
    CONSTRAINT dataset_rows_features_object CHECK (JSONB_TYPEOF(features) = 'object'),
    CONSTRAINT dataset_rows_availability_object
        CHECK (JSONB_TYPEOF(temporal_availability) = 'object'),
    CONSTRAINT dataset_rows_checksum_not_blank CHECK (BTRIM(row_checksum) <> ''),
    UNIQUE (dataset_version_id, deterministic_key)
);

CREATE INDEX dataset_rows_version_split_idx
    ON ml.dataset_rows (dataset_version_id, split, label);
CREATE INDEX dataset_rows_h3_date_idx ON ml.dataset_rows (h3, local_date);

CREATE TABLE ml.dataset_row_snapshots (
    dataset_row_id UUID NOT NULL REFERENCES ml.dataset_rows(id) ON DELETE RESTRICT,
    feature_snapshot_id UUID NOT NULL REFERENCES features.feature_snapshots(id) ON DELETE RESTRICT,
    PRIMARY KEY (dataset_row_id, feature_snapshot_id)
);

COMMENT ON TABLE ml.dataset_row_snapshots IS
    'Structured provenance: which feature snapshot(s) each dataset row drew from.';

CREATE TABLE ml.dataset_event_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_row_id UUID NOT NULL REFERENCES ml.dataset_rows(id) ON DELETE RESTRICT,
    ignition_event_id UUID NOT NULL REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    role TEXT NOT NULL DEFAULT 'primary',
    label_quality_confidence TEXT,
    geographic_quality_category TEXT,
    duplicate_group_id UUID REFERENCES validation.duplicate_candidate_groups(id) ON DELETE RESTRICT,
    inclusion_rule TEXT NOT NULL,
    justification TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT dataset_event_links_role_check CHECK (role IN ('primary', 'co_occurring')),
    UNIQUE (dataset_row_id, ignition_event_id)
);

CREATE INDEX dataset_event_links_event_idx ON ml.dataset_event_links (ignition_event_id);

CREATE TABLE ml.dataset_exclusions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dataset_version_id UUID NOT NULL REFERENCES ml.dataset_versions(id) ON DELETE RESTRICT,
    ignition_event_id UUID REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    h3 BIGINT,
    local_date DATE,
    reason_category TEXT NOT NULL,
    rule TEXT NOT NULL,
    rule_version TEXT,
    details JSONB NOT NULL DEFAULT '{}'::JSONB,
    excluded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reintegration_possible BOOLEAN NOT NULL DEFAULT FALSE,
    CONSTRAINT dataset_exclusions_reason_check CHECK (
        reason_category IN (
            'unknown_cause', 'natural_cause', 'indeterminate', 'certain_duplicate',
            'insufficient_geographic_quality', 'non_combustible_cell',
            'missing_features', 'invalid_snapshot', 'future_data', 'out_of_period',
            'out_of_territory', 'weather_unavailable', 'technical_error'
        )
    ),
    CONSTRAINT dataset_exclusions_rule_not_blank CHECK (BTRIM(rule) <> ''),
    CONSTRAINT dataset_exclusions_details_object CHECK (JSONB_TYPEOF(details) = 'object'),
    CONSTRAINT dataset_exclusions_subject_check CHECK (
        ignition_event_id IS NOT NULL OR (h3 IS NOT NULL AND local_date IS NOT NULL)
    )
);

CREATE INDEX dataset_exclusions_version_idx
    ON ml.dataset_exclusions (dataset_version_id, reason_category);

COMMENT ON TABLE ml.dataset_versions IS
    'Immutable-once-finalized dataset version for erytheon_human_ignition_cell_day_v1. '
    'No reader is switched to this table by this migration.';
COMMENT ON TABLE ml.dataset_builds IS
    'One construction attempt for a dataset version. A failed build stays auditable.';
COMMENT ON TABLE ml.dataset_rows IS
    'Structured columns plus versioned JSON features; provenance kept in '
    'ml.dataset_row_snapshots rather than opaque JSON alone.';
COMMENT ON TABLE ml.dataset_event_links IS
    'A positive cell-day row may link to more than one admissible ignition event. '
    'No event is ever deleted or merged here.';
COMMENT ON TABLE ml.dataset_exclusions IS
    'Every exclusion is explicit and reasoned; never silent.';
