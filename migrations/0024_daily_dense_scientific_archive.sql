-- Phase 4A.7: compact daily scientific archive.
--
-- One row stores six float32 FWI components for every modelable H3 cell,
-- ordered by the immutable coverage mask (H3 ascending). This avoids the
-- per-row PostgreSQL overhead of scientific_snapshot_values: production
-- measures ~18 MiB/day uncompressed instead of ~200 MiB/capture.

ALTER TABLE observability.scientific_snapshots
    DROP CONSTRAINT scientific_snapshots_type_check,
    ADD CONSTRAINT scientific_snapshots_type_check CHECK (
        snapshot_type IN ('weekly_full', 'daily_dense', 'metadata_only')
    );

CREATE TABLE observability.scientific_dense_archives (
    snapshot_id UUID PRIMARY KEY
        REFERENCES observability.scientific_snapshots(id) ON DELETE RESTRICT,
    coverage_mask_id UUID NOT NULL
        REFERENCES observability.coverage_masks(id) ON DELETE RESTRICT,
    h3_count BIGINT NOT NULL,
    encoding TEXT NOT NULL DEFAULT 'float32_be_h3_asc_v1',
    h3_order_checksum TEXT NOT NULL,
    ffmc_values BYTEA NOT NULL,
    dmc_values BYTEA NOT NULL,
    dc_values BYTEA NOT NULL,
    isi_values BYTEA NOT NULL,
    bui_values BYTEA NOT NULL,
    fwi_values BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT scientific_dense_archives_count_check CHECK (h3_count > 0),
    CONSTRAINT scientific_dense_archives_encoding_check
        CHECK (encoding = 'float32_be_h3_asc_v1'),
    CONSTRAINT scientific_dense_archives_order_checksum_not_blank
        CHECK (BTRIM(h3_order_checksum) <> ''),
    CONSTRAINT scientific_dense_archives_blob_lengths_check CHECK (
        OCTET_LENGTH(ffmc_values) = h3_count * 4 AND
        OCTET_LENGTH(dmc_values) = h3_count * 4 AND
        OCTET_LENGTH(dc_values) = h3_count * 4 AND
        OCTET_LENGTH(isi_values) = h3_count * 4 AND
        OCTET_LENGTH(bui_values) = h3_count * 4 AND
        OCTET_LENGTH(fwi_values) = h3_count * 4
    )
);

CREATE OR REPLACE FUNCTION observability.forbid_published_dense_archive_change()
RETURNS TRIGGER AS $$
DECLARE snapshot_status TEXT;
DECLARE target_snapshot_id UUID;
BEGIN
    target_snapshot_id := CASE WHEN TG_OP = 'INSERT' THEN NEW.snapshot_id ELSE OLD.snapshot_id END;
    SELECT status INTO snapshot_status
    FROM observability.scientific_snapshots
    WHERE id = target_snapshot_id;
    IF snapshot_status = 'published' THEN
        RAISE EXCEPTION 'refusing modification: dense archive snapshot % is published',
            target_snapshot_id;
    END IF;
    IF TG_OP = 'DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER scientific_dense_archives_published_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON observability.scientific_dense_archives
    FOR EACH ROW EXECUTE FUNCTION observability.forbid_published_dense_archive_change();

COMMENT ON TABLE observability.scientific_dense_archives IS
    'Compact daily FWI archive. Each BYTEA contains one network-order float32 per modelable '
    'coverage-mask cell, ordered by H3 ascending. The parent scientific snapshot carries '
    'provenance, completeness and the combined SHA-256 checksum.';
COMMENT ON COLUMN observability.scientific_dense_archives.h3_order_checksum IS
    'Checksum of the immutable coverage mask defining the positional H3 mapping.';
