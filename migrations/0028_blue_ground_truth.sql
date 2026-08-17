-- BLUE Ground Truth: versioned operational observations and immutable
-- comparisons against the complete commune-level forecast archive.
--
-- Satellite detections remain signals, never confirmed fires. Delayed
-- ignition records are stored separately as confirmed observations. No
-- absence is inferred from missing observations.

CREATE TABLE blue.ground_truth_observations (
    id BIGSERIAL PRIMARY KEY,
    observation_key TEXT NOT NULL UNIQUE,
    evidence_class TEXT NOT NULL,
    insee_code TEXT NOT NULL REFERENCES reference.commune_boundaries(insee_code) ON DELETE RESTRICT,
    commune_name TEXT NOT NULL,
    department_code TEXT,
    occurred_at TIMESTAMPTZ NOT NULL,
    observed_until TIMESTAMPTZ NOT NULL,
    signal_count BIGINT NOT NULL,
    max_frp REAL,
    official_event_id UUID REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT blue_ground_truth_class_check CHECK (
        evidence_class IN ('satellite_signal','confirmed_ignition')
    ),
    CONSTRAINT blue_ground_truth_name_not_blank CHECK (BTRIM(commune_name) <> ''),
    CONSTRAINT blue_ground_truth_time_check CHECK (observed_until >= occurred_at),
    CONSTRAINT blue_ground_truth_signal_count_check CHECK (signal_count > 0),
    CONSTRAINT blue_ground_truth_frp_check CHECK (max_frp IS NULL OR max_frp >= 0),
    CONSTRAINT blue_ground_truth_official_consistency CHECK (
        (evidence_class='confirmed_ignition' AND official_event_id IS NOT NULL)
        OR (evidence_class='satellite_signal' AND official_event_id IS NULL)
    ),
    CONSTRAINT blue_ground_truth_metadata_object CHECK (JSONB_TYPEOF(metadata)='object')
);
CREATE INDEX blue_ground_truth_observed_idx
    ON blue.ground_truth_observations(occurred_at DESC);
CREATE INDEX blue_ground_truth_commune_idx
    ON blue.ground_truth_observations(insee_code,occurred_at DESC);
CREATE INDEX blue_ground_truth_class_idx
    ON blue.ground_truth_observations(evidence_class,occurred_at DESC);

CREATE TABLE blue.ground_truth_matches (
    observation_id BIGINT NOT NULL REFERENCES blue.ground_truth_observations(id) ON DELETE RESTRICT,
    bulletin_id UUID NOT NULL REFERENCES blue.forecast_bulletins(id) ON DELETE RESTRICT,
    horizon TEXT NOT NULL,
    forecast_score REAL NOT NULL,
    forecast_max_score REAL NOT NULL,
    alert_threshold REAL NOT NULL,
    classification TEXT NOT NULL,
    lead_time_hours DOUBLE PRECISION NOT NULL,
    matching_rule_version TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY(observation_id,bulletin_id,horizon),
    CONSTRAINT blue_ground_truth_match_horizon CHECK (horizon IN ('hours_24','hours_48')),
    CONSTRAINT blue_ground_truth_match_scores CHECK (
        forecast_score BETWEEN 0 AND 1 AND forecast_max_score BETWEEN 0 AND 1
        AND alert_threshold BETWEEN 0 AND 1
    ),
    CONSTRAINT blue_ground_truth_match_classification CHECK (
        classification IN ('signal_covered','signal_below_threshold','confirmed_hit','confirmed_miss')
    ),
    CONSTRAINT blue_ground_truth_match_lead_check CHECK (lead_time_hours >= 0),
    CONSTRAINT blue_ground_truth_match_rule_not_blank CHECK (BTRIM(matching_rule_version) <> '')
);
CREATE INDEX blue_ground_truth_matches_bulletin_idx
    ON blue.ground_truth_matches(bulletin_id,horizon);
CREATE INDEX blue_ground_truth_matches_class_idx
    ON blue.ground_truth_matches(classification,created_at DESC);

CREATE TABLE blue.ground_truth_refreshes (
    id BIGSERIAL PRIMARY KEY,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ NOT NULL,
    satellite_windows_upserted BIGINT NOT NULL,
    confirmed_ignitions_upserted BIGINT NOT NULL,
    comparisons_inserted BIGINT NOT NULL,
    rule_version TEXT NOT NULL,
    CONSTRAINT blue_ground_truth_refresh_time_check CHECK (completed_at >= started_at),
    CONSTRAINT blue_ground_truth_refresh_counts_check CHECK (
        satellite_windows_upserted >= 0 AND confirmed_ignitions_upserted >= 0
        AND comparisons_inserted >= 0
    )
);

COMMENT ON TABLE blue.ground_truth_observations IS
    'Observed operational signals used to evaluate BLUE forecasts. Satellite signals are not confirmed fires.';
COMMENT ON TABLE blue.ground_truth_matches IS
    'Reproducible comparison of an observation with the complete immutable commune forecast archive.';
COMMENT ON TABLE blue.ground_truth_refreshes IS
    'Append-only audit history for automatic BLUE Ground Truth refreshes.';
