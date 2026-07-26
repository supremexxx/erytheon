CREATE SCHEMA IF NOT EXISTS raw;
CREATE SCHEMA IF NOT EXISTS staging;
CREATE SCHEMA IF NOT EXISTS reference;
CREATE SCHEMA IF NOT EXISTS environment;
CREATE SCHEMA IF NOT EXISTS human;
CREATE SCHEMA IF NOT EXISTS fire;
CREATE SCHEMA IF NOT EXISTS features;
CREATE SCHEMA IF NOT EXISTS risk;
CREATE SCHEMA IF NOT EXISTS validation;
CREATE SCHEMA IF NOT EXISTS ml;
CREATE SCHEMA IF NOT EXISTS serving;
CREATE SCHEMA IF NOT EXISTS ops;

COMMENT ON SCHEMA raw IS
    'Append-only external data retained as received before normalization.';
COMMENT ON SCHEMA staging IS
    'Rebuildable preparation area for parsing, validation, normalization, and rejected rows.';
COMMENT ON SCHEMA reference IS
    'Stable geographic, administrative, source, unit, and classification reference data.';
COMMENT ON SCHEMA environment IS
    'Normalized environmental observations, forecasts, and derived environmental measures.';
COMMENT ON SCHEMA human IS
    'Normalized anthropogenic, infrastructure, population, and activity data.';
COMMENT ON SCHEMA fire IS
    'Normalized observed fire incidents, detections, causes, perimeters, and source links.';
COMMENT ON SCHEMA features IS
    'Documented and versioned model-ready variables forming the stable algorithm input layer.';
COMMENT ON SCHEMA risk IS
    'Versioned ERYTHEON risk runs, append-only snapshots, components, alerts, and explanations.';
COMMENT ON SCHEMA validation IS
    'Reproducible comparison of predictions with observed incidents and validation metrics.';
COMMENT ON SCHEMA ml IS
    'Model, dataset, experiment, artifact, training-run, and metric metadata.';
COMMENT ON SCHEMA serving IS
    'Read-optimized projections and caches for APIs, maps, exports, and dashboards.';
COMMENT ON SCHEMA ops IS
    'Operational lineage for imports, pipelines, data-quality checks, errors, and jobs.';

CREATE TABLE IF NOT EXISTS reference.data_sources (
    id UUID PRIMARY KEY,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    provider TEXT NOT NULL,
    description TEXT,
    base_url TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT data_sources_code_unique UNIQUE (code),
    CONSTRAINT data_sources_code_format_check
        CHECK (code ~ '^[a-z][a-z0-9_]*$'),
    CONSTRAINT data_sources_name_not_blank_check
        CHECK (BTRIM(name) <> ''),
    CONSTRAINT data_sources_provider_not_blank_check
        CHECK (BTRIM(provider) <> ''),
    CONSTRAINT data_sources_category_check
        CHECK (category IN (
            'satellite',
            'weather',
            'fire_history',
            'geography',
            'land_cover',
            'population',
            'calendar',
            'infrastructure'
        )),
    CONSTRAINT data_sources_updated_at_check
        CHECK (updated_at >= created_at)
);

COMMENT ON TABLE reference.data_sources IS
    'Registry of external data sources known to ERYTHEON; credentials and tokens are never stored here.';
COMMENT ON COLUMN reference.data_sources.id IS
    'Application-generated stable UUID for references from operational and lineage tables.';
COMMENT ON COLUMN reference.data_sources.code IS
    'Stable lowercase application identifier such as firms or open_meteo_arome.';
COMMENT ON COLUMN reference.data_sources.category IS
    'Broad source family constrained to the currently supported platform categories.';
COMMENT ON COLUMN reference.data_sources.base_url IS
    'Optional public provider or API base URL; never contains credentials or tokens.';
COMMENT ON COLUMN reference.data_sources.updated_at IS
    'UTC instant of the latest metadata update; the application updates this value explicitly.';

CREATE TABLE IF NOT EXISTS ops.import_batches (
    id UUID PRIMARY KEY,
    source_id UUID NOT NULL,
    batch_type TEXT NOT NULL,
    status TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    requested_from TIMESTAMPTZ,
    requested_to TIMESTAMPTZ,
    records_received BIGINT NOT NULL DEFAULT 0,
    records_inserted BIGINT NOT NULL DEFAULT 0,
    records_updated BIGINT NOT NULL DEFAULT 0,
    records_ignored BIGINT NOT NULL DEFAULT 0,
    records_rejected BIGINT NOT NULL DEFAULT 0,
    checksum TEXT,
    source_version TEXT,
    pipeline_version TEXT,
    parameters JSONB NOT NULL DEFAULT '{}'::JSONB,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT import_batches_source_fk
        FOREIGN KEY (source_id)
        REFERENCES reference.data_sources (id)
        ON DELETE RESTRICT,
    CONSTRAINT import_batches_batch_type_not_blank_check
        CHECK (BTRIM(batch_type) <> ''),
    CONSTRAINT import_batches_status_check
        CHECK (status IN (
            'pending',
            'running',
            'succeeded',
            'partially_succeeded',
            'failed',
            'cancelled'
        )),
    CONSTRAINT import_batches_finished_at_check
        CHECK (finished_at IS NULL OR finished_at >= started_at),
    CONSTRAINT import_batches_requested_interval_check
        CHECK (requested_to IS NULL OR requested_from IS NULL OR requested_to >= requested_from),
    CONSTRAINT import_batches_records_received_check CHECK (records_received >= 0),
    CONSTRAINT import_batches_records_inserted_check CHECK (records_inserted >= 0),
    CONSTRAINT import_batches_records_updated_check CHECK (records_updated >= 0),
    CONSTRAINT import_batches_records_ignored_check CHECK (records_ignored >= 0),
    CONSTRAINT import_batches_records_rejected_check CHECK (records_rejected >= 0),
    CONSTRAINT import_batches_parameters_object_check
        CHECK (JSONB_TYPEOF(parameters) = 'object')
);

CREATE INDEX IF NOT EXISTS import_batches_source_started_at_idx
    ON ops.import_batches (source_id, started_at DESC);
CREATE INDEX IF NOT EXISTS import_batches_status_idx
    ON ops.import_batches (status);
CREATE INDEX IF NOT EXISTS import_batches_started_at_idx
    ON ops.import_batches (started_at DESC);

COMMENT ON TABLE ops.import_batches IS
    'One externally requested or file-based import attempt, including lineage, counts, status, and errors.';
COMMENT ON COLUMN ops.import_batches.id IS
    'Application-generated UUID that can be propagated across future pipeline components.';
COMMENT ON COLUMN ops.import_batches.status IS
    'Lifecycle state: pending, running, succeeded, partially_succeeded, failed, or cancelled.';
COMMENT ON COLUMN ops.import_batches.started_at IS
    'UTC instant when the import attempt began.';
COMMENT ON COLUMN ops.import_batches.finished_at IS
    'UTC instant when the attempt stopped; nullable while running or after an abrupt interruption.';
COMMENT ON COLUMN ops.import_batches.requested_from IS
    'Optional beginning of the source time interval requested by the import.';
COMMENT ON COLUMN ops.import_batches.requested_to IS
    'Optional end of the source time interval requested by the import.';
COMMENT ON COLUMN ops.import_batches.checksum IS
    'Optional checksum of the complete source response, file, or logical batch.';
COMMENT ON COLUMN ops.import_batches.parameters IS
    'Variable non-secret request and pipeline parameters represented as one JSON object.';

CREATE TABLE IF NOT EXISTS ops.pipeline_runs (
    id UUID PRIMARY KEY,
    pipeline_name TEXT NOT NULL,
    pipeline_version TEXT,
    status TEXT NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    trigger_type TEXT NOT NULL,
    import_batch_id UUID,
    parent_run_id UUID,
    parameters JSONB NOT NULL DEFAULT '{}'::JSONB,
    metrics JSONB NOT NULL DEFAULT '{}'::JSONB,
    error_message TEXT,
    code_version TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT pipeline_runs_import_batch_fk
        FOREIGN KEY (import_batch_id)
        REFERENCES ops.import_batches (id)
        ON DELETE RESTRICT,
    CONSTRAINT pipeline_runs_parent_fk
        FOREIGN KEY (parent_run_id)
        REFERENCES ops.pipeline_runs (id)
        ON DELETE RESTRICT,
    CONSTRAINT pipeline_runs_name_not_blank_check
        CHECK (BTRIM(pipeline_name) <> ''),
    CONSTRAINT pipeline_runs_trigger_not_blank_check
        CHECK (BTRIM(trigger_type) <> ''),
    CONSTRAINT pipeline_runs_status_check
        CHECK (status IN (
            'pending',
            'running',
            'succeeded',
            'partially_succeeded',
            'failed',
            'cancelled'
        )),
    CONSTRAINT pipeline_runs_finished_at_check
        CHECK (finished_at IS NULL OR finished_at >= started_at),
    CONSTRAINT pipeline_runs_parent_not_self_check
        CHECK (parent_run_id IS NULL OR parent_run_id <> id),
    CONSTRAINT pipeline_runs_parameters_object_check
        CHECK (JSONB_TYPEOF(parameters) = 'object'),
    CONSTRAINT pipeline_runs_metrics_object_check
        CHECK (JSONB_TYPEOF(metrics) = 'object')
);

CREATE INDEX IF NOT EXISTS pipeline_runs_name_started_at_idx
    ON ops.pipeline_runs (pipeline_name, started_at DESC);
CREATE INDEX IF NOT EXISTS pipeline_runs_status_idx
    ON ops.pipeline_runs (status);
CREATE INDEX IF NOT EXISTS pipeline_runs_started_at_idx
    ON ops.pipeline_runs (started_at DESC);
CREATE INDEX IF NOT EXISTS pipeline_runs_import_batch_idx
    ON ops.pipeline_runs (import_batch_id)
    WHERE import_batch_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS pipeline_runs_parent_idx
    ON ops.pipeline_runs (parent_run_id)
    WHERE parent_run_id IS NOT NULL;

COMMENT ON TABLE ops.pipeline_runs IS
    'One internal pipeline execution, optionally linked to an import batch and a parent run.';
COMMENT ON COLUMN ops.pipeline_runs.id IS
    'Application-generated UUID for one pipeline execution.';
COMMENT ON COLUMN ops.pipeline_runs.pipeline_version IS
    'Version of the pipeline contract or transformation, distinct from the source version.';
COMMENT ON COLUMN ops.pipeline_runs.status IS
    'Lifecycle state: pending, running, succeeded, partially_succeeded, failed, or cancelled.';
COMMENT ON COLUMN ops.pipeline_runs.trigger_type IS
    'Non-empty operator-defined trigger such as scheduled, manual, backfill, retry, or system.';
COMMENT ON COLUMN ops.pipeline_runs.import_batch_id IS
    'Optional external import batch that initiated or supplied this run.';
COMMENT ON COLUMN ops.pipeline_runs.parent_run_id IS
    'Optional parent execution for a retry or simple nested operation; self-reference is forbidden.';
COMMENT ON COLUMN ops.pipeline_runs.parameters IS
    'Variable non-secret execution parameters represented as one JSON object.';
COMMENT ON COLUMN ops.pipeline_runs.metrics IS
    'Variable execution counters or measurements represented as one JSON object.';
COMMENT ON COLUMN ops.pipeline_runs.code_version IS
    'Optional source-control commit or immutable build identifier.';

CREATE TABLE IF NOT EXISTS raw.firms_observations (
    id UUID PRIMARY KEY,
    import_batch_id UUID NOT NULL,
    source_record_id TEXT,
    retrieved_at TIMESTAMPTZ NOT NULL,
    observed_at TIMESTAMPTZ,
    payload JSONB NOT NULL,
    checksum TEXT,
    source_version TEXT,
    parsing_status TEXT NOT NULL DEFAULT 'pending',
    parsing_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT firms_observations_import_batch_fk
        FOREIGN KEY (import_batch_id)
        REFERENCES ops.import_batches (id)
        ON DELETE RESTRICT,
    CONSTRAINT firms_observations_payload_object_check
        CHECK (JSONB_TYPEOF(payload) = 'object'),
    CONSTRAINT firms_observations_parsing_status_check
        CHECK (parsing_status IN ('pending', 'parsed', 'rejected'))
);

CREATE INDEX IF NOT EXISTS firms_observations_import_batch_idx
    ON raw.firms_observations (import_batch_id);
CREATE INDEX IF NOT EXISTS firms_observations_observed_at_idx
    ON raw.firms_observations (observed_at DESC)
    WHERE observed_at IS NOT NULL;

COMMENT ON TABLE raw.firms_observations IS
    'Append-only NASA FIRMS source records retained before staging and normalization; phase 1 creates structure only.';
COMMENT ON COLUMN raw.firms_observations.id IS
    'Application-generated UUID for one received raw record version.';
COMMENT ON COLUMN raw.firms_observations.import_batch_id IS
    'Import batch that received this exact raw record.';
COMMENT ON COLUMN raw.firms_observations.source_record_id IS
    'Optional original identifier supplied by NASA FIRMS or derived from the source row.';
COMMENT ON COLUMN raw.firms_observations.retrieved_at IS
    'UTC instant when ERYTHEON received the source response containing this record.';
COMMENT ON COLUMN raw.firms_observations.observed_at IS
    'Optional UTC acquisition instant represented by the FIRMS record.';
COMMENT ON COLUMN raw.firms_observations.payload IS
    'Raw source fields represented as a JSON object without destructive correction.';
COMMENT ON COLUMN raw.firms_observations.checksum IS
    'Optional checksum of this exact record representation; duplicates remain permitted in raw.';
COMMENT ON COLUMN raw.firms_observations.parsing_status IS
    'Parsing state: pending, parsed, or rejected. Raw rows remain append-only in every state.';
COMMENT ON COLUMN raw.firms_observations.parsing_error IS
    'Optional parser error retained without deleting or overwriting the received record.';
