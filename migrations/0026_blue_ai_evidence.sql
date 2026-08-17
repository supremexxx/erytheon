-- BLUE daily showcase selection and independently auditable AI evidence research.
--
-- The complete forecast archive remains unchanged. At most twenty unique
-- communes per bulletin are selected for the readable evidence workflow.

CREATE TABLE blue.evidence_cases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bulletin_id UUID NOT NULL REFERENCES blue.forecast_bulletins(id) ON DELETE RESTRICT,
    insee_code TEXT NOT NULL REFERENCES reference.commune_boundaries(insee_code) ON DELETE RESTRICT,
    commune_name TEXT NOT NULL,
    department_code TEXT,
    daily_rank SMALLINT NOT NULL,
    selection_score REAL NOT NULL,
    alert_24h_id UUID REFERENCES blue.forecast_alerts(id) ON DELETE RESTRICT,
    alert_48h_id UUID REFERENCES blue.forecast_alerts(id) ON DELETE RESTRICT,
    research_after TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    verdict TEXT NOT NULL DEFAULT 'pending',
    confidence REAL,
    summary TEXT,
    observed_event_at TIMESTAMPTZ,
    observed_location TEXT,
    response_id TEXT,
    model TEXT,
    attempt_count SMALLINT NOT NULL DEFAULT 0,
    last_attempt_at TIMESTAMPTZ,
    next_attempt_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (bulletin_id, insee_code),
    UNIQUE (bulletin_id, daily_rank),
    CONSTRAINT blue_evidence_cases_rank_check CHECK (daily_rank BETWEEN 1 AND 20),
    CONSTRAINT blue_evidence_cases_score_check CHECK (selection_score BETWEEN 0 AND 1),
    CONSTRAINT blue_evidence_cases_alert_check CHECK (alert_24h_id IS NOT NULL OR alert_48h_id IS NOT NULL),
    CONSTRAINT blue_evidence_cases_status_check CHECK (
        status IN ('pending','researching','retry_due','reviewed','failed')
    ),
    CONSTRAINT blue_evidence_cases_verdict_check CHECK (
        verdict IN ('pending','signal_observed','probable','confirmed','no_evidence_found','inconclusive')
    ),
    CONSTRAINT blue_evidence_cases_confidence_check CHECK (confidence IS NULL OR confidence BETWEEN 0 AND 1),
    CONSTRAINT blue_evidence_cases_attempt_check CHECK (attempt_count BETWEEN 0 AND 2)
);
CREATE INDEX blue_evidence_cases_due_idx
    ON blue.evidence_cases(status, COALESCE(next_attempt_at,research_after), daily_rank);

CREATE TABLE blue.evidence_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES blue.evidence_cases(id) ON DELETE RESTRICT,
    attempt_no SMALLINT NOT NULL,
    request_checksum TEXT NOT NULL,
    model TEXT NOT NULL,
    response_id TEXT,
    status TEXT NOT NULL DEFAULT 'started',
    raw_response JSONB,
    input_tokens BIGINT,
    output_tokens BIGINT,
    web_search_count BIGINT NOT NULL DEFAULT 0,
    error TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    UNIQUE(case_id,attempt_no),
    CONSTRAINT blue_evidence_runs_attempt_check CHECK (attempt_no BETWEEN 1 AND 2),
    CONSTRAINT blue_evidence_runs_status_check CHECK (status IN ('started','completed','failed')),
    CONSTRAINT blue_evidence_runs_token_check CHECK (
        (input_tokens IS NULL OR input_tokens >= 0) AND (output_tokens IS NULL OR output_tokens >= 0)
        AND web_search_count >= 0
    )
);

CREATE TABLE blue.evidence_sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES blue.evidence_runs(id) ON DELETE RESTRICT,
    url TEXT NOT NULL,
    title TEXT NOT NULL,
    published_at TIMESTAMPTZ,
    excerpt TEXT,
    domain TEXT NOT NULL,
    relation_strength TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(run_id,url),
    CONSTRAINT blue_evidence_sources_url_check CHECK (url ~ '^https?://'),
    CONSTRAINT blue_evidence_sources_relation_check CHECK (
        relation_strength IN ('direct','corroborating','weak')
    )
);

COMMENT ON TABLE blue.evidence_cases IS
    'Top twenty unique communes selected deterministically from an immutable daily BLUE bulletin.';
COMMENT ON TABLE blue.evidence_runs IS
    'Append-only audit trail for automatic post-horizon web evidence reviews.';
COMMENT ON TABLE blue.evidence_sources IS
    'Cited sources retained for human verification; no-source results never prove absence of fire.';
