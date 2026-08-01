DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM observability.scientific_snapshots WHERE contract_version = 2)
       OR EXISTS (SELECT 1 FROM features.feature_snapshot_values)
       OR EXISTS (SELECT 1 FROM observability.coverage_masks)
       OR EXISTS (SELECT 1 FROM observability.snapshot_capture_attempts)
       OR EXISTS (SELECT 1 FROM ml.snapshot_label_links
                  WHERE maturity_status <> 'provisional' OR supersedes_link_id IS NOT NULL) THEN
        RAISE EXCEPTION 'refusing rollback 0022: Phase 4A.6 durable data exists';
    END IF;
END $$;

DROP INDEX IF EXISTS ml.snapshot_label_links_snapshot_event_current_unique;
ALTER TABLE ml.snapshot_label_links
    DROP CONSTRAINT IF EXISTS snapshot_label_links_supersedes_not_self,
    DROP CONSTRAINT IF EXISTS snapshot_label_links_maturity_check,
    DROP COLUMN IF EXISTS linker_run_id,
    DROP COLUMN IF EXISTS supersedes_link_id,
    DROP COLUMN IF EXISTS is_current,
    DROP COLUMN IF EXISTS cause_observed_at,
    DROP COLUMN IF EXISTS maturity_status;
ALTER TABLE ml.snapshot_label_links
    ADD CONSTRAINT snapshot_label_links_snapshot_id_ignition_event_id_key
        UNIQUE (snapshot_id, ignition_event_id);

ALTER TABLE observability.scientific_snapshots
    DROP CONSTRAINT IF EXISTS scientific_snapshots_v2_provenance_check,
    DROP CONSTRAINT IF EXISTS scientific_snapshots_v2_counts_check,
    DROP CONSTRAINT IF EXISTS scientific_snapshots_traceability_status_check,
    DROP CONSTRAINT IF EXISTS scientific_snapshots_contract_version_check,
    DROP COLUMN IF EXISTS unexpected_missing_count,
    DROP COLUMN IF EXISTS structural_exclusion_count,
    DROP COLUMN IF EXISTS modelable_cell_count,
    DROP COLUMN IF EXISTS coverage_mask_id,
    DROP COLUMN IF EXISTS forecast_horizon,
    DROP COLUMN IF EXISTS forecast_valid_at,
    DROP COLUMN IF EXISTS forecast_batch_computed_at,
    DROP COLUMN IF EXISTS application_image_digest,
    DROP COLUMN IF EXISTS application_image,
    DROP COLUMN IF EXISTS environment,
    DROP COLUMN IF EXISTS traceability_status,
    DROP COLUMN IF EXISTS contract_version;

DROP TRIGGER IF EXISTS coverage_masks_published_immutable ON observability.coverage_masks;
DROP FUNCTION IF EXISTS observability.forbid_published_coverage_mask_change();
DROP TRIGGER IF EXISTS coverage_mask_cells_frozen ON observability.coverage_mask_cells;
DROP FUNCTION IF EXISTS observability.forbid_frozen_coverage_mask_cell_change();
DROP TABLE IF EXISTS observability.coverage_mask_cells;
DROP TABLE IF EXISTS observability.coverage_masks;
DROP TABLE IF EXISTS features.feature_snapshot_activations;
DROP TRIGGER IF EXISTS feature_snapshot_manifest_frozen ON features.feature_snapshots;
DROP FUNCTION IF EXISTS features.forbid_frozen_feature_snapshot_manifest_change();
DROP TRIGGER IF EXISTS feature_snapshot_values_frozen ON features.feature_snapshot_values;
DROP FUNCTION IF EXISTS features.forbid_frozen_feature_snapshot_change();
DROP TABLE IF EXISTS features.feature_snapshot_values;
DROP TABLE IF EXISTS observability.snapshot_capture_attempts;

DROP INDEX IF EXISTS observability.system_snapshots_window_history_idx;
ALTER TABLE observability.system_snapshots
    DROP CONSTRAINT IF EXISTS system_snapshots_window_identity_unique,
    DROP CONSTRAINT IF EXISTS system_snapshots_provenance_status_check,
    DROP CONSTRAINT IF EXISTS system_snapshots_window_order_check,
    DROP COLUMN IF EXISTS provenance_status,
    DROP COLUMN IF EXISTS capture_window_end,
    DROP COLUMN IF EXISTS capture_window_start,
    ADD CONSTRAINT system_snapshots_identity_unique UNIQUE (environment, capture_date, cadence);
