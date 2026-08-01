-- Phase 4A.6: harden temporal identity, immutable static inputs,
-- coverage denominators, scientific provenance, and deferred labels.
-- Additive except for replacing the incorrect day-level system snapshot key.

ALTER TABLE observability.system_snapshots
    ADD COLUMN capture_window_start TIMESTAMPTZ,
    ADD COLUMN capture_window_end TIMESTAMPTZ,
    ADD COLUMN application_image_digest TEXT,
    ADD COLUMN provenance_status TEXT NOT NULL DEFAULT 'captured';

UPDATE observability.system_snapshots
SET capture_window_start = CASE cadence
        WHEN 'hourly' THEN date_trunc('hour', captured_at)
        WHEN 'daily' THEN capture_date::timestamptz
        ELSE captured_at
    END,
    capture_window_end = CASE cadence
        WHEN 'hourly' THEN date_trunc('hour', captured_at) + interval '1 hour'
        WHEN 'daily' THEN capture_date::timestamptz + interval '1 day'
        ELSE captured_at + interval '1 microsecond'
    END,
    provenance_status = CASE cadence
        WHEN 'hourly' THEN 'legacy_last_state_only'
        ELSE 'legacy_day_identity'
    END;

ALTER TABLE observability.system_snapshots
    ALTER COLUMN capture_window_start SET NOT NULL,
    ALTER COLUMN capture_window_end SET NOT NULL,
    ADD CONSTRAINT system_snapshots_window_order_check
        CHECK (capture_window_end > capture_window_start),
    ADD CONSTRAINT system_snapshots_provenance_status_check
        CHECK (provenance_status IN ('captured', 'legacy_last_state_only', 'legacy_day_identity'));

ALTER TABLE observability.system_snapshots
    DROP CONSTRAINT system_snapshots_identity_unique;
ALTER TABLE observability.system_snapshots
    ADD CONSTRAINT system_snapshots_window_identity_unique
        UNIQUE (environment, cadence, capture_window_start);

CREATE INDEX system_snapshots_window_history_idx
    ON observability.system_snapshots (environment, cadence, capture_window_start DESC);

CREATE TABLE observability.snapshot_capture_attempts (
    id BIGSERIAL PRIMARY KEY,
    environment TEXT NOT NULL,
    cadence TEXT NOT NULL,
    capture_window_start TIMESTAMPTZ NOT NULL,
    capture_window_end TIMESTAMPTZ NOT NULL,
    attempt_number INTEGER NOT NULL,
    trigger_kind TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'started',
    system_snapshot_id BIGINT REFERENCES observability.system_snapshots(id) ON DELETE RESTRICT,
    application_revision TEXT,
    application_image TEXT,
    application_image_digest TEXT,
    rows_processed BIGINT,
    pipeline_run_id UUID REFERENCES ops.pipeline_runs(id) ON DELETE RESTRICT,
    checksum TEXT,
    error_message TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT snapshot_capture_attempts_cadence_check
        CHECK (cadence IN ('daily', 'hourly', 'event')),
    CONSTRAINT snapshot_capture_attempts_window_check
        CHECK (capture_window_end > capture_window_start),
    CONSTRAINT snapshot_capture_attempts_status_check
        CHECK (status IN ('started', 'succeeded', 'failed')),
    CONSTRAINT snapshot_capture_attempts_trigger_check
        CHECK (trigger_kind IN ('scheduler', 'manual', 'replay', 'unknown')),
    CONSTRAINT snapshot_capture_attempts_number_check CHECK (attempt_number > 0),
    CONSTRAINT snapshot_capture_attempts_metadata_object CHECK (jsonb_typeof(metadata) = 'object'),
    UNIQUE (environment, cadence, capture_window_start, attempt_number)
);

CREATE INDEX snapshot_capture_attempts_window_idx
    ON observability.snapshot_capture_attempts
       (environment, cadence, capture_window_start DESC, attempt_number DESC);

-- Materialized copy of the exact model-ready static inputs. Rows become
-- immutable as soon as their manifest is active or superseded.
CREATE TABLE features.feature_snapshot_values (
    snapshot_id UUID NOT NULL REFERENCES features.feature_snapshots(id) ON DELETE RESTRICT,
    h3 BIGINT NOT NULL,
    features JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (snapshot_id, h3),
    CONSTRAINT feature_snapshot_values_object CHECK (jsonb_typeof(features) = 'object'),
    CONSTRAINT feature_snapshot_values_contract_keys CHECK (
        features ?& ARRAY['hist','wui','road','agri','combustible','population','poi','power_line','school_zone']
    )
);

CREATE OR REPLACE FUNCTION features.forbid_frozen_feature_snapshot_change()
RETURNS TRIGGER AS $$
DECLARE manifest_status TEXT;
BEGIN
    SELECT status INTO manifest_status
    FROM features.feature_snapshots
    WHERE id = COALESCE(OLD.snapshot_id, NEW.snapshot_id);
    IF manifest_status IN ('active', 'superseded') THEN
        RAISE EXCEPTION 'refusing modification: feature snapshot % is frozen',
            COALESCE(OLD.snapshot_id, NEW.snapshot_id);
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER feature_snapshot_values_frozen
    BEFORE UPDATE OR DELETE ON features.feature_snapshot_values
    FOR EACH ROW EXECUTE FUNCTION features.forbid_frozen_feature_snapshot_change();

CREATE OR REPLACE FUNCTION features.forbid_frozen_feature_snapshot_manifest_change()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.family = 'cell_static_bundle' AND OLD.status IN ('active','superseded') THEN
        IF TG_OP = 'DELETE' THEN
            RAISE EXCEPTION 'refusing deletion: feature snapshot % is frozen', OLD.id;
        END IF;
        IF NOT (OLD.status='active' AND NEW.status='superseded'
                AND (to_jsonb(OLD)-'status') = (to_jsonb(NEW)-'status')) THEN
            RAISE EXCEPTION 'refusing modification: feature snapshot % is frozen', OLD.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER feature_snapshot_manifest_frozen
    BEFORE UPDATE OR DELETE ON features.feature_snapshots
    FOR EACH ROW EXECUTE FUNCTION features.forbid_frozen_feature_snapshot_manifest_change();

CREATE TABLE features.feature_snapshot_activations (
    id BIGSERIAL PRIMARY KEY,
    family TEXT NOT NULL,
    h3_resolution SMALLINT NOT NULL,
    snapshot_id UUID NOT NULL REFERENCES features.feature_snapshots(id) ON DELETE RESTRICT,
    activated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deactivated_at TIMESTAMPTZ,
    activation_revision TEXT,
    CONSTRAINT feature_snapshot_activations_period_check
        CHECK (deactivated_at IS NULL OR deactivated_at >= activated_at)
);
CREATE UNIQUE INDEX feature_snapshot_activations_one_current
    ON features.feature_snapshot_activations (family, h3_resolution)
    WHERE deactivated_at IS NULL;

CREATE TABLE observability.coverage_masks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    logical_id TEXT NOT NULL UNIQUE,
    family TEXT NOT NULL,
    h3_resolution SMALLINT NOT NULL,
    source_kind TEXT NOT NULL,
    source_checksum TEXT NOT NULL,
    expected_cell_count BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'building',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT coverage_masks_resolution_check CHECK (h3_resolution BETWEEN 0 AND 15),
    CONSTRAINT coverage_masks_count_check CHECK (expected_cell_count >= 0),
    CONSTRAINT coverage_masks_status_check CHECK (status IN ('building','validated','published','superseded','failed')),
    CONSTRAINT coverage_masks_publication_check CHECK (
        (status IN ('published','superseded') AND published_at IS NOT NULL) OR status NOT IN ('published','superseded')
    ),
    CONSTRAINT coverage_masks_metadata_object CHECK (jsonb_typeof(metadata) = 'object')
);

CREATE TABLE observability.coverage_mask_cells (
    mask_id UUID NOT NULL REFERENCES observability.coverage_masks(id) ON DELETE RESTRICT,
    h3 BIGINT NOT NULL,
    eligibility TEXT NOT NULL DEFAULT 'modelable',
    reason TEXT NOT NULL DEFAULT 'inside_operational_aoi',
    PRIMARY KEY (mask_id, h3),
    CONSTRAINT coverage_mask_cells_eligibility_check CHECK (eligibility IN ('modelable','excluded')),
    CONSTRAINT coverage_mask_cells_reason_not_blank CHECK (btrim(reason) <> '')
);

CREATE OR REPLACE FUNCTION observability.forbid_frozen_coverage_mask_cell_change()
RETURNS TRIGGER AS $$
DECLARE mask_status TEXT;
DECLARE target_mask_id UUID;
BEGIN
    target_mask_id := CASE WHEN TG_OP='INSERT' THEN NEW.mask_id ELSE OLD.mask_id END;
    SELECT status INTO mask_status FROM observability.coverage_masks
    WHERE id=target_mask_id;
    IF mask_status IN ('published','superseded') THEN
        RAISE EXCEPTION 'refusing modification: coverage mask % is frozen',
            target_mask_id;
    END IF;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER coverage_mask_cells_frozen
    BEFORE INSERT OR UPDATE OR DELETE ON observability.coverage_mask_cells
    FOR EACH ROW EXECUTE FUNCTION observability.forbid_frozen_coverage_mask_cell_change();

CREATE UNIQUE INDEX coverage_masks_one_published_family_resolution
    ON observability.coverage_masks (family, h3_resolution)
    WHERE status = 'published';

CREATE OR REPLACE FUNCTION observability.forbid_published_coverage_mask_change()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.status IN ('published','superseded') THEN
        IF TG_OP='DELETE' THEN
            RAISE EXCEPTION 'refusing deletion: coverage mask % is published', OLD.id;
        END IF;
        IF NOT (OLD.status='published' AND NEW.status='superseded'
                AND (to_jsonb(OLD)-'status') = (to_jsonb(NEW)-'status')) THEN
            RAISE EXCEPTION 'refusing modification: coverage mask % is published', OLD.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER coverage_masks_published_immutable
    BEFORE UPDATE OR DELETE ON observability.coverage_masks
    FOR EACH ROW EXECUTE FUNCTION observability.forbid_published_coverage_mask_change();

ALTER TABLE observability.scientific_snapshots
    ADD COLUMN contract_version SMALLINT NOT NULL DEFAULT 1,
    ADD COLUMN traceability_status TEXT NOT NULL DEFAULT 'legacy_incomplete',
    ADD COLUMN environment TEXT,
    ADD COLUMN application_image TEXT,
    ADD COLUMN application_image_digest TEXT,
    ADD COLUMN forecast_batch_computed_at TIMESTAMPTZ REFERENCES public.forecast_batches(computed_at) ON DELETE RESTRICT,
    ADD COLUMN forecast_valid_at TIMESTAMPTZ,
    ADD COLUMN forecast_horizon TEXT,
    ADD COLUMN coverage_mask_id UUID REFERENCES observability.coverage_masks(id) ON DELETE RESTRICT,
    ADD COLUMN modelable_cell_count BIGINT,
    ADD COLUMN structural_exclusion_count BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN unexpected_missing_count BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN completeness_status TEXT NOT NULL DEFAULT 'legacy_incomplete',
    ADD CONSTRAINT scientific_snapshots_contract_version_check CHECK (contract_version IN (1,2)),
    ADD CONSTRAINT scientific_snapshots_traceability_status_check
        CHECK (traceability_status IN ('legacy_incomplete','complete')),
    ADD CONSTRAINT scientific_snapshots_completeness_status_check CHECK (
        completeness_status IN ('legacy_incomplete','building','complete',
          'published_partial_expected','published_partial_degraded','failed_validation')
    ),
    ADD CONSTRAINT scientific_snapshots_v2_counts_check CHECK (
        modelable_cell_count IS NULL OR
        (modelable_cell_count >= 0 AND structural_exclusion_count >= 0 AND unexpected_missing_count >= 0)
    ),
    ADD CONSTRAINT scientific_snapshots_v2_provenance_check CHECK (
        contract_version = 1 OR status NOT IN ('validated','published') OR (
            static_snapshot_id IS NOT NULL AND application_revision IS NOT NULL AND
            environment IS NOT NULL AND application_image IS NOT NULL AND
            application_image_digest IS NOT NULL AND
            forecast_batch_computed_at IS NOT NULL AND forecast_valid_at IS NOT NULL AND
            forecast_horizon = 'nowcast' AND coverage_mask_id IS NOT NULL AND
            modelable_cell_count IS NOT NULL AND pipeline_run_id IS NOT NULL AND
            traceability_status = 'complete'
        )
    );

CREATE TABLE observability.scientific_snapshot_missing_reasons (
    snapshot_id UUID NOT NULL REFERENCES observability.scientific_snapshots(id) ON DELETE RESTRICT,
    reason TEXT NOT NULL,
    cell_count BIGINT NOT NULL,
    classification_version TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(snapshot_id,reason),
    CONSTRAINT scientific_snapshot_missing_reason_check CHECK (reason IN (
        'expected_structural_exclusion','non_combustible','outside_operational_aoi',
        'water_or_invalid_land','forecast_source_uncovered','partition_failed',
        'h3_mapping_failure','pipeline_filter_exclusion','unexpected_missing',
        'unknown_missing_reason'
    )),
    CONSTRAINT scientific_snapshot_missing_reason_count_check CHECK (cell_count >= 0),
    CONSTRAINT scientific_snapshot_missing_reason_version_not_blank CHECK (btrim(classification_version)<>'')
);

ALTER TABLE ml.snapshot_label_links
    ADD COLUMN maturity_status TEXT NOT NULL DEFAULT 'provisional',
    ADD COLUMN cause_observed_at TIMESTAMPTZ,
    ADD COLUMN is_current BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN supersedes_link_id BIGINT REFERENCES ml.snapshot_label_links(id) ON DELETE RESTRICT,
    ADD COLUMN linker_run_id UUID REFERENCES ops.pipeline_runs(id) ON DELETE RESTRICT,
    ADD CONSTRAINT snapshot_label_links_maturity_check
        CHECK (maturity_status IN ('provisional','mature','superseded')),
    ADD CONSTRAINT snapshot_label_links_supersedes_not_self
        CHECK (supersedes_link_id IS NULL OR supersedes_link_id <> id);

ALTER TABLE ml.snapshot_label_links
    DROP CONSTRAINT snapshot_label_links_snapshot_id_ignition_event_id_key;

DROP INDEX IF EXISTS ml.snapshot_label_links_snapshot_event_current_unique;
CREATE UNIQUE INDEX snapshot_label_links_snapshot_event_current_unique
    ON ml.snapshot_label_links (snapshot_id, ignition_event_id)
    WHERE is_current AND ignition_event_id IS NOT NULL;

COMMENT ON COLUMN observability.system_snapshots.capture_window_start IS
    'Canonical UTC bucket identity. Hourly captures use date_trunc(hour, captured_at).';
COMMENT ON COLUMN observability.scientific_snapshots.contract_version IS
    'Version 1 is legacy 4A.5. Version 2 requires static bundle, source batch, deployment lineage, and coverage mask before publication.';
