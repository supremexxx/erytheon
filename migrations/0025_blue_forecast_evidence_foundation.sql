-- BLUE Forecast & Evidence Center: immutable daily +24 h / +48 h bulletins.
--
-- Forecasts and later evidence are deliberately separated. A published
-- forecast can never be rewritten after an observed event becomes known.

CREATE SCHEMA IF NOT EXISTS blue;

ALTER TABLE reference.commune_boundaries
    ADD COLUMN department_code TEXT,
    ADD COLUMN region_code TEXT,
    ADD COLUMN source_version TEXT,
    ADD COLUMN source_checksum TEXT;

CREATE TABLE reference.commune_h3_cells (
    insee_code TEXT NOT NULL REFERENCES reference.commune_boundaries(insee_code) ON DELETE RESTRICT,
    h3 BIGINT NOT NULL,
    h3_resolution SMALLINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (insee_code, h3_resolution, h3),
    CONSTRAINT commune_h3_cells_resolution_check CHECK (h3_resolution BETWEEN 0 AND 15)
);
CREATE UNIQUE INDEX commune_h3_cells_one_commune_per_cell
    ON reference.commune_h3_cells(h3_resolution, h3);
CREATE INDEX commune_h3_cells_h3_idx ON reference.commune_h3_cells(h3);

CREATE TABLE blue.forecast_bulletins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    logical_id TEXT NOT NULL UNIQUE,
    bulletin_date DATE NOT NULL UNIQUE,
    scheduled_for TIMESTAMPTZ NOT NULL,
    issued_at TIMESTAMPTZ NOT NULL,
    forecast_batch_computed_at TIMESTAMPTZ NOT NULL
        REFERENCES public.forecast_batches(computed_at) ON DELETE RESTRICT,
    forecast_source TEXT NOT NULL,
    model_version_id BIGINT NOT NULL REFERENCES public.human_model_versions(id) ON DELETE RESTRICT,
    application_revision TEXT NOT NULL,
    application_image TEXT NOT NULL,
    application_image_digest TEXT NOT NULL,
    environment TEXT NOT NULL,
    coverage_mask_id UUID NOT NULL REFERENCES observability.coverage_masks(id) ON DELETE RESTRICT,
    forecast_cell_count BIGINT NOT NULL,
    mapped_cell_count BIGINT NOT NULL DEFAULT 0,
    unmapped_cell_count BIGINT NOT NULL DEFAULT 0,
    commune_count BIGINT NOT NULL DEFAULT 0,
    alerts_24h BIGINT NOT NULL DEFAULT 0,
    alerts_48h BIGINT NOT NULL DEFAULT 0,
    aggregation_contract JSONB NOT NULL,
    checksum TEXT,
    status TEXT NOT NULL DEFAULT 'building',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ,
    CONSTRAINT blue_bulletins_source_not_blank CHECK (BTRIM(forecast_source) <> ''),
    CONSTRAINT blue_bulletins_revision_not_blank CHECK (BTRIM(application_revision) <> ''),
    CONSTRAINT blue_bulletins_image_not_blank CHECK (BTRIM(application_image) <> ''),
    CONSTRAINT blue_bulletins_digest_not_blank CHECK (BTRIM(application_image_digest) <> ''),
    CONSTRAINT blue_bulletins_environment_not_blank CHECK (BTRIM(environment) <> ''),
    CONSTRAINT blue_bulletins_counts_check CHECK (
        forecast_cell_count > 0 AND mapped_cell_count >= 0 AND unmapped_cell_count >= 0
        AND forecast_cell_count = mapped_cell_count + unmapped_cell_count
        AND commune_count >= 0 AND alerts_24h >= 0 AND alerts_48h >= 0
    ),
    CONSTRAINT blue_bulletins_contract_object CHECK (JSONB_TYPEOF(aggregation_contract) = 'object'),
    CONSTRAINT blue_bulletins_status_check CHECK (status IN ('building','published','failed')),
    CONSTRAINT blue_bulletins_publication_check CHECK (
        (status='published' AND published_at IS NOT NULL AND checksum IS NOT NULL
         AND commune_count > 0 AND mapped_cell_count > 0)
        OR status<>'published'
    )
);

CREATE TABLE blue.forecast_index_archives (
    bulletin_id UUID PRIMARY KEY REFERENCES blue.forecast_bulletins(id) ON DELETE RESTRICT,
    commune_codes TEXT[] NOT NULL,
    commune_count BIGINT NOT NULL,
    code_order_checksum TEXT NOT NULL,
    p95_24h BYTEA NOT NULL,
    max_24h BYTEA NOT NULL,
    p95_48h BYTEA NOT NULL,
    max_48h BYTEA NOT NULL,
    encoding TEXT NOT NULL DEFAULT 'float32_be_insee_asc_v1',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT blue_index_archive_count_check CHECK (
        commune_count > 0 AND CARDINALITY(commune_codes) = commune_count
    ),
    CONSTRAINT blue_index_archive_encoding_check CHECK (encoding='float32_be_insee_asc_v1'),
    CONSTRAINT blue_index_archive_lengths_check CHECK (
        OCTET_LENGTH(p95_24h)=commune_count*4 AND OCTET_LENGTH(max_24h)=commune_count*4
        AND OCTET_LENGTH(p95_48h)=commune_count*4 AND OCTET_LENGTH(max_48h)=commune_count*4
    )
);

CREATE TABLE blue.forecast_alerts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bulletin_id UUID NOT NULL REFERENCES blue.forecast_bulletins(id) ON DELETE RESTRICT,
    insee_code TEXT NOT NULL REFERENCES reference.commune_boundaries(insee_code) ON DELETE RESTRICT,
    commune_name TEXT NOT NULL,
    department_code TEXT,
    horizon TEXT NOT NULL,
    valid_at TIMESTAMPTZ NOT NULL,
    alert_index REAL NOT NULL,
    max_score REAL NOT NULL,
    mean_score REAL NOT NULL,
    physical_at_peak REAL NOT NULL,
    human_at_peak REAL NOT NULL,
    evaluated_cell_count BIGINT NOT NULL,
    elevated_cell_count BIGINT NOT NULL,
    critical_cell_count BIGINT NOT NULL,
    risk_level TEXT NOT NULL,
    top_factors JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(bulletin_id,insee_code,horizon),
    CONSTRAINT blue_alerts_horizon_check CHECK (horizon IN ('hours_24','hours_48')),
    CONSTRAINT blue_alerts_scores_check CHECK (
        alert_index BETWEEN 0 AND 1 AND max_score BETWEEN 0 AND 1
        AND mean_score BETWEEN 0 AND 1 AND physical_at_peak BETWEEN 0 AND 1
        AND human_at_peak BETWEEN 0 AND 1
    ),
    CONSTRAINT blue_alerts_counts_check CHECK (
        evaluated_cell_count > 0 AND elevated_cell_count >= 0 AND critical_cell_count >= 0
        AND elevated_cell_count <= evaluated_cell_count
        AND critical_cell_count <= elevated_cell_count
    ),
    CONSTRAINT blue_alerts_level_check CHECK (risk_level IN ('elevated','critical')),
    CONSTRAINT blue_alerts_factors_array CHECK (JSONB_TYPEOF(top_factors)='array')
);
CREATE INDEX blue_forecast_alerts_latest_idx
    ON blue.forecast_alerts(bulletin_id,horizon,alert_index DESC);
CREATE INDEX blue_forecast_alerts_commune_idx
    ON blue.forecast_alerts(insee_code,created_at DESC);

CREATE TABLE blue.forecast_evaluations (
    alert_id UUID PRIMARY KEY REFERENCES blue.forecast_alerts(id) ON DELETE RESTRICT,
    status TEXT NOT NULL DEFAULT 'pending',
    maturity_at TIMESTAMPTZ,
    observed_event_at TIMESTAMPTZ,
    observed_h3 BIGINT,
    distance_km DOUBLE PRECISION,
    evidence_count BIGINT NOT NULL DEFAULT 0,
    reviewer_note TEXT,
    reviewed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT blue_evaluations_status_check CHECK (
        status IN ('pending','researching','signal_observed','probable','confirmed','no_event_confirmed','inconclusive')
    ),
    CONSTRAINT blue_evaluations_distance_check CHECK (distance_km IS NULL OR distance_km >= 0),
    CONSTRAINT blue_evaluations_evidence_count_check CHECK (evidence_count >= 0)
);

CREATE OR REPLACE FUNCTION blue.forbid_published_forecast_change()
RETURNS TRIGGER AS $$
DECLARE target_bulletin_id UUID;
DECLARE bulletin_status TEXT;
BEGIN
    IF TG_TABLE_NAME='forecast_bulletins' THEN
        IF TG_OP='DELETE' THEN target_bulletin_id := OLD.id;
        ELSE target_bulletin_id := NEW.id;
        END IF;
    ELSE
        IF TG_OP='DELETE' THEN target_bulletin_id := OLD.bulletin_id;
        ELSE target_bulletin_id := NEW.bulletin_id;
        END IF;
    END IF;
    SELECT status INTO bulletin_status FROM blue.forecast_bulletins WHERE id=target_bulletin_id;
    IF bulletin_status='published' THEN
        RAISE EXCEPTION 'refusing modification: BLUE bulletin % is published', target_bulletin_id;
    END IF;
    IF TG_OP='DELETE' THEN RETURN OLD; END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER blue_bulletins_published_immutable
    BEFORE UPDATE OR DELETE ON blue.forecast_bulletins
    FOR EACH ROW EXECUTE FUNCTION blue.forbid_published_forecast_change();
CREATE TRIGGER blue_index_archives_published_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON blue.forecast_index_archives
    FOR EACH ROW EXECUTE FUNCTION blue.forbid_published_forecast_change();
CREATE TRIGGER blue_alerts_published_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON blue.forecast_alerts
    FOR EACH ROW EXECUTE FUNCTION blue.forbid_published_forecast_change();

COMMENT ON SCHEMA blue IS
    'Immutable BLUE forecast bulletins and separately mutable post-horizon evidence reviews.';
COMMENT ON TABLE blue.forecast_index_archives IS
    'Complete compact commune-level forecast index, including communes below the alert threshold.';
COMMENT ON TABLE blue.forecast_alerts IS
    'Commercially readable subset above the provisional alert threshold; not a calibrated fire probability.';
