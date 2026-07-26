INSERT INTO reference.data_sources (
    id,
    code,
    name,
    category,
    provider,
    description,
    base_url,
    is_active
)
VALUES (
    '00000000-0000-4000-8000-000000000011'::UUID,
    'bdiff',
    'Base de données sur les incendies de forêt en France',
    'fire_history',
    'Ministère de l''Agriculture et de la Souveraineté alimentaire',
    'Historical French vegetation-fire records imported from periodic normalized CSV exports.',
    'https://bdiff.agriculture.gouv.fr/incendies',
    TRUE
)
ON CONFLICT (code) DO NOTHING;

CREATE TABLE raw.bdiff_records (
    id UUID PRIMARY KEY,
    import_batch_id UUID NOT NULL,
    source_record_id TEXT,
    source_line_number BIGINT NOT NULL,
    payload JSONB NOT NULL,
    payload_format TEXT NOT NULL DEFAULT 'csv',
    retrieved_at TIMESTAMPTZ NOT NULL,
    checksum TEXT,
    parsing_status TEXT NOT NULL DEFAULT 'pending',
    parsing_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT bdiff_records_import_batch_fk
        FOREIGN KEY (import_batch_id)
        REFERENCES ops.import_batches (id)
        ON DELETE RESTRICT,
    CONSTRAINT bdiff_records_source_line_positive_check
        CHECK (source_line_number > 0),
    CONSTRAINT bdiff_records_payload_object_check
        CHECK (JSONB_TYPEOF(payload) = 'object'),
    CONSTRAINT bdiff_records_payload_format_not_blank_check
        CHECK (BTRIM(payload_format) <> ''),
    CONSTRAINT bdiff_records_parsing_status_check
        CHECK (parsing_status IN ('pending', 'parsed', 'rejected')),
    CONSTRAINT bdiff_records_batch_line_unique
        UNIQUE (import_batch_id, source_line_number)
);

CREATE UNIQUE INDEX bdiff_records_batch_source_record_unique
    ON raw.bdiff_records (import_batch_id, source_record_id)
    WHERE source_record_id IS NOT NULL;
CREATE INDEX bdiff_records_import_batch_idx
    ON raw.bdiff_records (import_batch_id);
CREATE INDEX bdiff_records_source_record_idx
    ON raw.bdiff_records (source_record_id)
    WHERE source_record_id IS NOT NULL;
CREATE INDEX bdiff_records_parsing_status_idx
    ON raw.bdiff_records (parsing_status);

COMMENT ON TABLE raw.bdiff_records IS
    'Append-only BDIFF source rows retained before deterministic normalization.';
COMMENT ON COLUMN raw.bdiff_records.source_line_number IS
    'One-based source line including the CSV header; used as a replay guard when source identity is absent.';
COMMENT ON COLUMN raw.bdiff_records.payload IS
    'All columns received on the normalized CSV row, including columns unknown to the current normalizer.';
COMMENT ON INDEX raw.bdiff_records_batch_source_record_unique IS
    'Prevents duplicate source identities inside one batch while allowing the same event in later batches.';

CREATE TABLE staging.bdiff_events_normalized (
    id UUID PRIMARY KEY,
    raw_record_id UUID NOT NULL,
    source_record_id TEXT,
    occurred_at TIMESTAMPTZ,
    municipality_source TEXT,
    latitude DOUBLE PRECISION,
    longitude DOUBLE PRECISION,
    geom_original geometry(Point, 4326),
    surface_ha DOUBLE PRECISION,
    cause_source TEXT,
    cause_category TEXT,
    cause_subcategory TEXT,
    taxonomy_version TEXT NOT NULL,
    normalizer_version TEXT NOT NULL,
    validation_status TEXT NOT NULL,
    validation_errors JSONB NOT NULL DEFAULT '[]'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT bdiff_events_normalized_raw_record_fk
        FOREIGN KEY (raw_record_id)
        REFERENCES raw.bdiff_records (id)
        ON DELETE RESTRICT,
    CONSTRAINT bdiff_events_normalized_raw_record_unique
        UNIQUE (raw_record_id),
    CONSTRAINT bdiff_events_normalized_latitude_check
        CHECK (
            validation_status = 'rejected'
            OR latitude IS NULL
            OR latitude BETWEEN -90 AND 90
        ),
    CONSTRAINT bdiff_events_normalized_longitude_check
        CHECK (
            validation_status = 'rejected'
            OR longitude IS NULL
            OR longitude BETWEEN -180 AND 180
        ),
    CONSTRAINT bdiff_events_normalized_surface_check
        CHECK (
            validation_status = 'rejected'
            OR surface_ha IS NULL
            OR surface_ha >= 0
        ),
    CONSTRAINT bdiff_events_normalized_geometry_check
        CHECK (
            geom_original IS NULL
            OR (
                ST_SRID(geom_original) = 4326
                AND GeometryType(geom_original) = 'POINT'
            )
        ),
    CONSTRAINT bdiff_events_normalized_cause_category_check
        CHECK (
            cause_category IS NULL
            OR cause_category IN (
                'human_known',
                'natural_known',
                'unknown',
                'indeterminate'
            )
        ),
    CONSTRAINT bdiff_events_normalized_validation_status_check
        CHECK (validation_status IN ('valid', 'rejected')),
    CONSTRAINT bdiff_events_normalized_validation_errors_array_check
        CHECK (JSONB_TYPEOF(validation_errors) = 'array'),
    CONSTRAINT bdiff_events_normalized_valid_row_check
        CHECK (
            validation_status = 'rejected'
            OR (
                source_record_id IS NOT NULL
                AND occurred_at IS NOT NULL
                AND BTRIM(municipality_source) <> ''
                AND latitude IS NOT NULL
                AND longitude IS NOT NULL
                AND geom_original IS NOT NULL
                AND surface_ha IS NOT NULL
                AND BTRIM(cause_source) <> ''
                AND cause_category IS NOT NULL
                AND cause_subcategory IS NOT NULL
                AND JSONB_ARRAY_LENGTH(validation_errors) = 0
            )
        )
);

CREATE INDEX bdiff_events_normalized_source_record_idx
    ON staging.bdiff_events_normalized (source_record_id)
    WHERE source_record_id IS NOT NULL;
CREATE INDEX bdiff_events_normalized_occurred_at_idx
    ON staging.bdiff_events_normalized (occurred_at DESC)
    WHERE occurred_at IS NOT NULL;
CREATE INDEX bdiff_events_normalized_validation_status_idx
    ON staging.bdiff_events_normalized (validation_status);
CREATE INDEX bdiff_events_normalized_geom_original_gix
    ON staging.bdiff_events_normalized
    USING GIST (geom_original);

COMMENT ON TABLE staging.bdiff_events_normalized IS
    'Deterministic BDIFF normalization; both valid and rejected raw rows remain traceable.';
COMMENT ON COLUMN staging.bdiff_events_normalized.geom_original IS
    'Unmodified WGS84 source coordinate, which may be a municipality centroid.';
COMMENT ON COLUMN staging.bdiff_events_normalized.validation_errors IS
    'Stable machine-readable validation error codes; no source row is silently discarded.';

CREATE TABLE fire.ignition_events (
    id UUID PRIMARY KEY,
    source_id UUID NOT NULL,
    source_record_id TEXT NOT NULL,
    staging_event_id UUID NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    occurred_on_local DATE NOT NULL,
    municipality_source TEXT NOT NULL,
    latitude_original DOUBLE PRECISION NOT NULL,
    longitude_original DOUBLE PRECISION NOT NULL,
    geom_original geometry(Point, 4326) NOT NULL,
    h3 BIGINT NOT NULL,
    h3_resolution SMALLINT NOT NULL,
    surface_ha DOUBLE PRECISION NOT NULL,
    cause_source TEXT NOT NULL,
    cause_category TEXT NOT NULL,
    cause_subcategory TEXT NOT NULL,
    taxonomy_version TEXT NOT NULL,
    geographic_quality TEXT NOT NULL DEFAULT 'precision_undocumented',
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT ignition_events_source_fk
        FOREIGN KEY (source_id)
        REFERENCES reference.data_sources (id)
        ON DELETE RESTRICT,
    CONSTRAINT ignition_events_staging_event_fk
        FOREIGN KEY (staging_event_id)
        REFERENCES staging.bdiff_events_normalized (id)
        ON DELETE RESTRICT,
    CONSTRAINT ignition_events_source_identity_unique
        UNIQUE (source_id, source_record_id),
    CONSTRAINT ignition_events_staging_event_unique
        UNIQUE (staging_event_id),
    CONSTRAINT ignition_events_source_record_not_blank_check
        CHECK (BTRIM(source_record_id) <> ''),
    CONSTRAINT ignition_events_municipality_not_blank_check
        CHECK (BTRIM(municipality_source) <> ''),
    CONSTRAINT ignition_events_latitude_check
        CHECK (latitude_original BETWEEN -90 AND 90),
    CONSTRAINT ignition_events_longitude_check
        CHECK (longitude_original BETWEEN -180 AND 180),
    CONSTRAINT ignition_events_geometry_check
        CHECK (
            ST_SRID(geom_original) = 4326
            AND GeometryType(geom_original) = 'POINT'
        ),
    CONSTRAINT ignition_events_h3_resolution_check
        CHECK (h3_resolution BETWEEN 0 AND 15),
    CONSTRAINT ignition_events_surface_check
        CHECK (surface_ha >= 0),
    CONSTRAINT ignition_events_cause_category_check
        CHECK (cause_category IN (
            'human_known',
            'natural_known',
            'unknown',
            'indeterminate'
        )),
    CONSTRAINT ignition_events_geographic_quality_check
        CHECK (geographic_quality IN ('precision_undocumented')),
    CONSTRAINT ignition_events_updated_at_check
        CHECK (updated_at >= created_at)
);

CREATE INDEX ignition_events_occurred_at_idx
    ON fire.ignition_events (occurred_at DESC);
CREATE INDEX ignition_events_occurred_on_local_idx
    ON fire.ignition_events (occurred_on_local DESC);
CREATE INDEX ignition_events_h3_occurred_at_idx
    ON fire.ignition_events (h3, occurred_at DESC);
CREATE INDEX ignition_events_cause_category_idx
    ON fire.ignition_events (cause_category);
CREATE INDEX ignition_events_geom_original_gix
    ON fire.ignition_events
    USING GIST (geom_original);

COMMENT ON TABLE fire.ignition_events IS
    'Stable BDIFF ignition events created only from valid staging rows; not read by the current model.';
COMMENT ON COLUMN fire.ignition_events.staging_event_id IS
    'First valid staging observation that created the stable business event.';
COMMENT ON COLUMN fire.ignition_events.geom_original IS
    'Unmodified source geometry; no combustible-cell snapping or centroid correction occurs here.';
COMMENT ON COLUMN fire.ignition_events.h3 IS
    'H3 cell calculated directly from the original WGS84 coordinate at h3_resolution.';
