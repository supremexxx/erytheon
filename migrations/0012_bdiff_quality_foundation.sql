CREATE SCHEMA IF NOT EXISTS validation;

CREATE TABLE validation.rule_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    logical_id TEXT NOT NULL,
    rule_type TEXT NOT NULL,
    description TEXT NOT NULL,
    parameters JSONB NOT NULL,
    code_version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    checksum TEXT NOT NULL,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    activated_at TIMESTAMPTZ,
    CONSTRAINT rule_versions_logical_id_not_blank CHECK (BTRIM(logical_id) <> ''),
    CONSTRAINT rule_versions_type_check CHECK (rule_type IN (
        'taxonomy', 'label_quality', 'geographic_quality',
        'duplicate_detection', 'combustibility'
    )),
    CONSTRAINT rule_versions_parameters_object CHECK (JSONB_TYPEOF(parameters) = 'object'),
    CONSTRAINT rule_versions_code_not_blank CHECK (BTRIM(code_version) <> ''),
    CONSTRAINT rule_versions_status_check CHECK (status IN ('draft', 'validated', 'active', 'retired')),
    CONSTRAINT rule_versions_checksum_not_blank CHECK (BTRIM(checksum) <> ''),
    CONSTRAINT rule_versions_activation_check CHECK (
        (status = 'active' AND activated_at IS NOT NULL)
        OR (status <> 'active')
    ),
    UNIQUE (logical_id),
    UNIQUE (rule_type, checksum)
);

CREATE UNIQUE INDEX rule_versions_one_active_per_type
    ON validation.rule_versions (rule_type)
    WHERE status = 'active';

CREATE TABLE validation.coordinate_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    rule_version_id UUID NOT NULL REFERENCES validation.rule_versions(id) ON DELETE RESTRICT,
    latitude DOUBLE PRECISION NOT NULL,
    longitude DOUBLE PRECISION NOT NULL,
    event_count BIGINT NOT NULL,
    municipality_count BIGINT NOT NULL,
    year_count BIGINT NOT NULL,
    decimal_precision SMALLINT NOT NULL,
    repeated_coordinate BOOLEAN NOT NULL,
    rounded_coordinate_probable BOOLEAN NOT NULL,
    centroid_status TEXT NOT NULL,
    signals JSONB NOT NULL,
    logical_checksum TEXT NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT coordinate_groups_latitude_check CHECK (latitude BETWEEN -90 AND 90),
    CONSTRAINT coordinate_groups_longitude_check CHECK (longitude BETWEEN -180 AND 180),
    CONSTRAINT coordinate_groups_counts_check CHECK (
        event_count > 0 AND municipality_count > 0 AND year_count > 0
    ),
    CONSTRAINT coordinate_groups_precision_check CHECK (decimal_precision BETWEEN 0 AND 15),
    CONSTRAINT coordinate_groups_centroid_check CHECK (centroid_status IN (
        'confirmed_centroid', 'probable_centroid', 'possible_centroid',
        'not_centroid_like', 'undetermined'
    )),
    CONSTRAINT coordinate_groups_signals_object CHECK (JSONB_TYPEOF(signals) = 'object'),
    UNIQUE (rule_version_id, latitude, longitude),
    UNIQUE (rule_version_id, logical_checksum)
);

CREATE TABLE validation.event_label_quality (
    ignition_event_id UUID NOT NULL REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    rule_version_id UUID NOT NULL REFERENCES validation.rule_versions(id) ON DELETE RESTRICT,
    taxonomy_version TEXT NOT NULL,
    cause_category TEXT NOT NULL,
    cause_subcategory TEXT NOT NULL,
    confidence TEXT NOT NULL,
    proposed_eligibility TEXT NOT NULL,
    requires_accidental_sensitivity_analysis BOOLEAN NOT NULL DEFAULT FALSE,
    reasons JSONB NOT NULL,
    logical_checksum TEXT NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (ignition_event_id, rule_version_id),
    CONSTRAINT event_label_quality_confidence_check CHECK (
        confidence IN ('high', 'medium', 'low', 'unknown')
    ),
    CONSTRAINT event_label_quality_eligibility_check CHECK (proposed_eligibility IN (
        'eligible_human_positive', 'eligible_natural_cohort',
        'unknown_cause_cohort', 'indeterminate_cause_cohort',
        'invalid_or_unusable', 'requires_review'
    )),
    CONSTRAINT event_label_quality_reasons_array CHECK (JSONB_TYPEOF(reasons) = 'array'),
    CONSTRAINT event_label_quality_checksum_not_blank CHECK (BTRIM(logical_checksum) <> '')
);

CREATE TABLE validation.event_geographic_quality (
    ignition_event_id UUID NOT NULL REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    rule_version_id UUID NOT NULL REFERENCES validation.rule_versions(id) ON DELETE RESTRICT,
    coordinate_group_id UUID NOT NULL REFERENCES validation.coordinate_groups(id) ON DELETE RESTRICT,
    latitude_original DOUBLE PRECISION NOT NULL,
    longitude_original DOUBLE PRECISION NOT NULL,
    geom_original geometry(Point, 4326) NOT NULL,
    h3_original BIGINT NOT NULL,
    h3_resolution SMALLINT NOT NULL,
    municipality_source TEXT NOT NULL,
    derived_insee_code TEXT,
    derived_department_code TEXT,
    derived_region_code TEXT,
    derivation_method TEXT,
    reference_name TEXT,
    reference_version TEXT,
    municipality_centroid_distance_m DOUBLE PRECISION,
    shared_coordinate_event_count BIGINT NOT NULL,
    shared_coordinate_municipality_count BIGINT NOT NULL,
    decimal_precision SMALLINT NOT NULL,
    rounded_coordinate_probable BOOLEAN NOT NULL,
    centroid_status TEXT NOT NULL,
    geographic_category TEXT NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    reasons JSONB NOT NULL,
    logical_checksum TEXT NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (ignition_event_id, rule_version_id),
    CONSTRAINT event_geographic_quality_geometry_check CHECK (
        ST_SRID(geom_original) = 4326 AND GeometryType(geom_original) = 'POINT'
    ),
    CONSTRAINT event_geographic_quality_resolution_check CHECK (h3_resolution BETWEEN 0 AND 15),
    CONSTRAINT event_geographic_quality_distance_check CHECK (
        municipality_centroid_distance_m IS NULL OR municipality_centroid_distance_m >= 0
    ),
    CONSTRAINT event_geographic_quality_counts_check CHECK (
        shared_coordinate_event_count > 0 AND shared_coordinate_municipality_count > 0
    ),
    CONSTRAINT event_geographic_quality_precision_check CHECK (decimal_precision BETWEEN 0 AND 15),
    CONSTRAINT event_geographic_quality_centroid_check CHECK (centroid_status IN (
        'confirmed_centroid', 'probable_centroid', 'possible_centroid',
        'not_centroid_like', 'undetermined'
    )),
    CONSTRAINT event_geographic_quality_category_check CHECK (geographic_category IN (
        'precise_reported', 'estimated_reported', 'municipality_centroid_confirmed',
        'municipality_centroid_probable', 'rounded_coordinate_probable',
        'precision_undocumented', 'unknown_location', 'invalid_location'
    )),
    CONSTRAINT event_geographic_quality_confidence_check CHECK (confidence BETWEEN 0 AND 1),
    CONSTRAINT event_geographic_quality_reasons_array CHECK (JSONB_TYPEOF(reasons) = 'array')
);

CREATE TABLE validation.event_combustibility_assessments (
    ignition_event_id UUID NOT NULL REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    rule_version_id UUID NOT NULL REFERENCES validation.rule_versions(id) ON DELETE RESTRICT,
    h3_original BIGINT NOT NULL,
    h3_resolution SMALLINT NOT NULL,
    cell_features_present BOOLEAN NOT NULL,
    original_cell_combustible BOOLEAN,
    feature_snapshot_at TIMESTAMPTZ,
    nearest_combustible_h3 BIGINT,
    nearest_combustible_ring SMALLINT,
    nearest_combustible_distance_m DOUBLE PRECISION,
    combustible_ring1_count INTEGER NOT NULL DEFAULT 0,
    combustible_ring2_count INTEGER NOT NULL DEFAULT 0,
    status_flags JSONB NOT NULL,
    territorial_signals JSONB NOT NULL,
    reasons JSONB NOT NULL,
    logical_checksum TEXT NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (ignition_event_id, rule_version_id),
    CONSTRAINT event_combustibility_resolution_check CHECK (h3_resolution BETWEEN 0 AND 15),
    CONSTRAINT event_combustibility_ring_check CHECK (
        nearest_combustible_ring IS NULL OR nearest_combustible_ring BETWEEN 0 AND 2
    ),
    CONSTRAINT event_combustibility_distance_check CHECK (
        nearest_combustible_distance_m IS NULL OR nearest_combustible_distance_m >= 0
    ),
    CONSTRAINT event_combustibility_ring_counts_check CHECK (
        combustible_ring1_count >= 0 AND combustible_ring2_count >= combustible_ring1_count
    ),
    CONSTRAINT event_combustibility_status_array CHECK (JSONB_TYPEOF(status_flags) = 'array'),
    CONSTRAINT event_combustibility_signals_object CHECK (JSONB_TYPEOF(territorial_signals) = 'object'),
    CONSTRAINT event_combustibility_reasons_array CHECK (JSONB_TYPEOF(reasons) = 'array')
);

CREATE TABLE validation.combustible_cell_candidates (
    ignition_event_id UUID NOT NULL,
    rule_version_id UUID NOT NULL,
    candidate_h3 BIGINT NOT NULL,
    h3_ring SMALLINT NOT NULL,
    rank SMALLINT NOT NULL,
    center_distance_m DOUBLE PRECISION NOT NULL,
    features JSONB NOT NULL,
    score DOUBLE PRECISION NOT NULL,
    justification JSONB NOT NULL,
    PRIMARY KEY (ignition_event_id, rule_version_id, candidate_h3),
    FOREIGN KEY (ignition_event_id, rule_version_id)
        REFERENCES validation.event_combustibility_assessments
        (ignition_event_id, rule_version_id) ON DELETE RESTRICT,
    CONSTRAINT combustible_cell_candidates_ring_check CHECK (h3_ring BETWEEN 1 AND 2),
    CONSTRAINT combustible_cell_candidates_rank_check CHECK (rank > 0),
    CONSTRAINT combustible_cell_candidates_distance_check CHECK (center_distance_m >= 0),
    CONSTRAINT combustible_cell_candidates_score_check CHECK (score BETWEEN 0 AND 1),
    CONSTRAINT combustible_cell_candidates_features_object CHECK (JSONB_TYPEOF(features) = 'object'),
    CONSTRAINT combustible_cell_candidates_justification_object CHECK (JSONB_TYPEOF(justification) = 'object')
);

CREATE TABLE validation.duplicate_candidate_pairs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    rule_version_id UUID NOT NULL REFERENCES validation.rule_versions(id) ON DELETE RESTRICT,
    left_event_id UUID NOT NULL REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    right_event_id UUID NOT NULL REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    score DOUBLE PRECISION NOT NULL,
    classification TEXT NOT NULL,
    raw_signals JSONB NOT NULL,
    contributions JSONB NOT NULL,
    justification TEXT NOT NULL,
    logical_checksum TEXT NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT duplicate_candidate_pairs_order_check CHECK (left_event_id < right_event_id),
    CONSTRAINT duplicate_candidate_pairs_score_check CHECK (score BETWEEN 0 AND 1),
    CONSTRAINT duplicate_candidate_pairs_classification_check CHECK (classification IN (
        'certain_duplicate', 'probable_duplicate', 'possible_duplicate',
        'indeterminate', 'probably_distinct'
    )),
    CONSTRAINT duplicate_candidate_pairs_signals_object CHECK (JSONB_TYPEOF(raw_signals) = 'object'),
    CONSTRAINT duplicate_candidate_pairs_contributions_object CHECK (JSONB_TYPEOF(contributions) = 'object'),
    UNIQUE (rule_version_id, left_event_id, right_event_id),
    UNIQUE (rule_version_id, logical_checksum)
);

CREATE TABLE validation.duplicate_candidate_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    rule_version_id UUID NOT NULL REFERENCES validation.rule_versions(id) ON DELETE RESTRICT,
    stable_key TEXT NOT NULL,
    score DOUBLE PRECISION NOT NULL,
    classification TEXT NOT NULL,
    member_count INTEGER NOT NULL,
    principal_signals JSONB NOT NULL,
    proposed_decision TEXT NOT NULL,
    human_decision TEXT,
    review_status TEXT NOT NULL DEFAULT 'unreviewed',
    justification TEXT NOT NULL,
    logical_checksum TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewed_at TIMESTAMPTZ,
    CONSTRAINT duplicate_candidate_groups_score_check CHECK (score BETWEEN 0 AND 1),
    CONSTRAINT duplicate_candidate_groups_classification_check CHECK (classification IN (
        'certain_duplicate', 'probable_duplicate', 'possible_duplicate', 'indeterminate'
    )),
    CONSTRAINT duplicate_candidate_groups_members_check CHECK (member_count >= 2),
    CONSTRAINT duplicate_candidate_groups_signals_object CHECK (JSONB_TYPEOF(principal_signals) = 'object'),
    CONSTRAINT duplicate_candidate_groups_decision_check CHECK (
        proposed_decision IN ('retain_all', 'review', 'possible_single_representative')
    ),
    CONSTRAINT duplicate_candidate_groups_review_check CHECK (
        review_status IN ('unreviewed', 'in_review', 'confirmed', 'rejected')
    ),
    UNIQUE (rule_version_id, stable_key),
    UNIQUE (rule_version_id, logical_checksum)
);

CREATE TABLE validation.duplicate_candidate_members (
    group_id UUID NOT NULL REFERENCES validation.duplicate_candidate_groups(id) ON DELETE RESTRICT,
    ignition_event_id UUID NOT NULL REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    role TEXT NOT NULL DEFAULT 'candidate',
    individual_score DOUBLE PRECISION NOT NULL,
    pair_ids JSONB NOT NULL,
    justification TEXT NOT NULL,
    PRIMARY KEY (group_id, ignition_event_id),
    CONSTRAINT duplicate_candidate_members_role_check CHECK (
        role IN ('anchor', 'candidate', 'ambiguous_bridge')
    ),
    CONSTRAINT duplicate_candidate_members_score_check CHECK (individual_score BETWEEN 0 AND 1),
    CONSTRAINT duplicate_candidate_members_pairs_array CHECK (JSONB_TYPEOF(pair_ids) = 'array')
);

CREATE INDEX coordinate_groups_repeated_idx
    ON validation.coordinate_groups (rule_version_id, event_count DESC)
    WHERE repeated_coordinate;
CREATE INDEX event_label_quality_eligibility_idx
    ON validation.event_label_quality (rule_version_id, proposed_eligibility);
CREATE INDEX event_geographic_quality_category_idx
    ON validation.event_geographic_quality (rule_version_id, geographic_category);
CREATE INDEX event_geographic_quality_centroid_idx
    ON validation.event_geographic_quality (rule_version_id, centroid_status);
CREATE INDEX event_combustibility_status_gin
    ON validation.event_combustibility_assessments USING GIN (status_flags);
CREATE INDEX duplicate_candidate_pairs_classification_idx
    ON validation.duplicate_candidate_pairs (rule_version_id, classification, score DESC);
CREATE INDEX duplicate_candidate_groups_classification_idx
    ON validation.duplicate_candidate_groups (rule_version_id, classification, score DESC);
CREATE INDEX duplicate_candidate_members_event_idx
    ON validation.duplicate_candidate_members (ignition_event_id);

COMMENT ON SCHEMA validation IS
    'Versioned, additive and non-destructive scientific assessments over immutable fire events.';
COMMENT ON TABLE validation.event_geographic_quality IS
    'Original geometry and H3 are copied for audit only; proposed corrections never replace fire.ignition_events.';
COMMENT ON TABLE validation.combustible_cell_candidates IS
    'Candidate combustible neighbours are proposals and never overwrite the original event H3.';
COMMENT ON TABLE validation.duplicate_candidate_groups IS
    'Potential duplicates remain separate source events; no automatic merge or deletion is performed.';
