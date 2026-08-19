-- Preserve community terrain reports without presenting them as official
-- confirmations. False alerts are archived separately and can never create a
-- positive ground-truth match.

ALTER TABLE blue.ground_truth_confirmations
    DROP CONSTRAINT blue_ground_truth_confirmation_level_check,
    ADD CONSTRAINT blue_ground_truth_confirmation_level_check CHECK (
        evidence_level IN (
            'community_reported','press_confirmed','authority_confirmed'
        )
    );

CREATE TABLE blue.ground_truth_rejections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bulletin_id UUID NOT NULL
        REFERENCES blue.forecast_bulletins(id) ON DELETE RESTRICT,
    insee_code TEXT NOT NULL
        REFERENCES reference.commune_boundaries(insee_code) ON DELETE RESTRICT,
    event_date DATE NOT NULL,
    rejection_reason TEXT NOT NULL,
    source_url TEXT NOT NULL,
    source_title TEXT NOT NULL,
    source_published_on DATE NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    verified_by TEXT NOT NULL,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(bulletin_id,insee_code,source_url),
    CONSTRAINT blue_ground_truth_rejection_reason_check CHECK (
        rejection_reason IN ('false_alert','wrong_location','outside_window')
    ),
    CONSTRAINT blue_ground_truth_rejection_url_check CHECK (
        source_url ~ '^https?://'
    ),
    CONSTRAINT blue_ground_truth_rejection_text_check CHECK (
        BTRIM(source_title)<>'' AND BTRIM(verified_by)<>''
    )
);

CREATE INDEX blue_ground_truth_rejections_event_idx
    ON blue.ground_truth_rejections(event_date DESC,insee_code);

CREATE FUNCTION blue.forbid_ground_truth_rejection_change()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'BLUE Ground Truth rejections are append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER blue_ground_truth_rejections_immutable
BEFORE UPDATE OR DELETE ON blue.ground_truth_rejections
FOR EACH ROW EXECUTE FUNCTION blue.forbid_ground_truth_rejection_change();

COMMENT ON COLUMN blue.ground_truth_confirmations.evidence_level IS
    'Community reports support a probable terrain signal only; press and authority confirmations retain their stronger provenance.';
COMMENT ON TABLE blue.ground_truth_rejections IS
    'Append-only reviewed reports that must not be interpreted as observed fires.';
