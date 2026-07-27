-- Phase 3B.3: versioned historical calendar (2020-2026), additive, distinct
-- from the operational public.calendar_days table already read by the
-- active model. public.calendar_days is not touched by this migration.
--
-- Public holidays in France are fixed by law and are fully and correctly
-- computable retroactively for any past year (fixed dates plus an
-- Easter-based algorithm), so they can be classified historical_exact.
-- School holidays are zone-specific administrative decisions; no verified
-- source for 2020-2024 exists in this environment today, so rows for
-- those years must carry school_holiday = NULL and
-- temporal_classification = 'unavailable_historically' rather than a
-- fabricated value.

CREATE TABLE features.calendar_rule_versions (
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
    CONSTRAINT calendar_rule_versions_logical_id_not_blank CHECK (BTRIM(logical_id) <> ''),
    CONSTRAINT calendar_rule_versions_type_check CHECK (
        rule_type IN ('public_holiday', 'school_holiday', 'season')
    ),
    CONSTRAINT calendar_rule_versions_parameters_object CHECK (JSONB_TYPEOF(parameters) = 'object'),
    CONSTRAINT calendar_rule_versions_code_not_blank CHECK (BTRIM(code_version) <> ''),
    CONSTRAINT calendar_rule_versions_status_check CHECK (
        status IN ('draft', 'validated', 'active', 'retired')
    ),
    CONSTRAINT calendar_rule_versions_checksum_not_blank CHECK (BTRIM(checksum) <> ''),
    CONSTRAINT calendar_rule_versions_activation_check CHECK (
        (status = 'active' AND activated_at IS NOT NULL) OR (status <> 'active')
    ),
    UNIQUE (logical_id),
    UNIQUE (rule_type, checksum)
);

CREATE UNIQUE INDEX calendar_rule_versions_one_active_per_type
    ON features.calendar_rule_versions (rule_type)
    WHERE status = 'active';

CREATE TABLE features.historical_calendar_days (
    rule_version_id UUID NOT NULL REFERENCES features.calendar_rule_versions(id) ON DELETE RESTRICT,
    date DATE NOT NULL,
    school_zone TEXT NOT NULL DEFAULT 'unspecified',
    year SMALLINT NOT NULL,
    month SMALLINT NOT NULL,
    day_of_week SMALLINT NOT NULL,
    is_weekend BOOLEAN NOT NULL,
    public_holiday BOOLEAN NOT NULL,
    public_holiday_label TEXT,
    school_holiday BOOLEAN,
    school_holiday_label TEXT,
    is_day_before_public_holiday BOOLEAN NOT NULL,
    is_day_after_public_holiday BOOLEAN NOT NULL,
    season SMALLINT NOT NULL,
    season_sine DOUBLE PRECISION NOT NULL,
    season_cosine DOUBLE PRECISION NOT NULL,
    available_from TIMESTAMPTZ NOT NULL,
    source TEXT NOT NULL,
    temporal_classification TEXT NOT NULL,
    logical_checksum TEXT NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (rule_version_id, date, school_zone),
    CONSTRAINT historical_calendar_days_month_check CHECK (month BETWEEN 1 AND 12),
    CONSTRAINT historical_calendar_days_dow_check CHECK (day_of_week BETWEEN 0 AND 6),
    CONSTRAINT historical_calendar_days_season_check CHECK (season BETWEEN 0 AND 3),
    CONSTRAINT historical_calendar_days_zone_check CHECK (
        school_zone IN ('A', 'B', 'C', 'unspecified')
    ),
    CONSTRAINT historical_calendar_days_source_not_blank CHECK (BTRIM(source) <> ''),
    CONSTRAINT historical_calendar_days_checksum_not_blank CHECK (BTRIM(logical_checksum) <> ''),
    CONSTRAINT historical_calendar_days_classification_check CHECK (
        temporal_classification IN (
            'historical_exact', 'historical_snapshot', 'stable_approximation',
            'current_snapshot_applied_historically', 'unavailable_historically',
            'derived_past_only'
        )
    ),
    CONSTRAINT historical_calendar_days_school_holiday_classification_check CHECK (
        school_holiday IS NOT NULL OR temporal_classification = 'unavailable_historically'
    )
);

CREATE INDEX historical_calendar_days_year_idx
    ON features.historical_calendar_days (rule_version_id, year);

COMMENT ON TABLE features.calendar_rule_versions IS
    'Immutable versioned calendar rules (public holiday algorithm, school '
    'holiday source, season definition). A validated or active version must '
    'never change checksum silently.';
COMMENT ON TABLE features.historical_calendar_days IS
    'Versioned historical calendar, distinct from public.calendar_days '
    '(still read by the active operational model, untouched by this '
    'migration). school_holiday is NULL, not false, when no verified '
    'source exists for that year/zone.';
COMMENT ON COLUMN features.historical_calendar_days.school_holiday IS
    'NULL means no verified source for this date/zone, not "not on '
    'holiday". Never treat NULL as false.';
