-- Phase 4A.5: deferred BDIFF label association. Purely additive.
--
-- BDIFF causes often arrive after a scientific snapshot has already been
-- published. This table links a later-known ignition event to the
-- snapshot(s) that existed at the relevant time and place, without ever
-- mutating the published snapshot itself. It prepares future dataset
-- construction (Phase 5) but does not build or activate anything here.
--
-- Absolute rules enforced by the CHECK below, matching
-- fire.ignition_events.cause_category (migration 0011) plus one
-- observability-specific 'no_event' value for a confirmed absence: an
-- unknown or indeterminate cause is never a negative, a known-natural
-- cause is never treated as "no fire", and FIRMS detections are never
-- used as a human label (this table only links to fire.ignition_events,
-- never to raw.firms_observations).

CREATE TABLE ml.snapshot_label_links (
    id BIGSERIAL PRIMARY KEY,
    snapshot_id UUID NOT NULL REFERENCES observability.scientific_snapshots(id) ON DELETE RESTRICT,
    ignition_event_id UUID REFERENCES fire.ignition_events(id) ON DELETE RESTRICT,
    h3 BIGINT NOT NULL,
    event_date DATE,
    label_class TEXT NOT NULL,
    label_confidence REAL,
    cause_version TEXT,
    matched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    matching_rule_version TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT snapshot_label_links_label_class_check CHECK (
        label_class IN ('human_known', 'natural_known', 'unknown', 'indeterminate', 'no_event')
    ),
    CONSTRAINT snapshot_label_links_confidence_check CHECK (
        label_confidence IS NULL OR label_confidence BETWEEN 0 AND 1
    ),
    CONSTRAINT snapshot_label_links_matching_rule_version_not_blank
        CHECK (BTRIM(matching_rule_version) <> ''),
    CONSTRAINT snapshot_label_links_metadata_object CHECK (JSONB_TYPEOF(metadata) = 'object'),
    CONSTRAINT snapshot_label_links_event_consistency_check CHECK (
        (label_class = 'no_event' AND ignition_event_id IS NULL)
        OR (label_class <> 'no_event' AND ignition_event_id IS NOT NULL)
    ),
    UNIQUE (snapshot_id, ignition_event_id)
);

CREATE INDEX snapshot_label_links_snapshot_idx
    ON ml.snapshot_label_links (snapshot_id);
CREATE INDEX snapshot_label_links_event_idx
    ON ml.snapshot_label_links (ignition_event_id)
    WHERE ignition_event_id IS NOT NULL;
CREATE INDEX snapshot_label_links_h3_date_idx
    ON ml.snapshot_label_links (h3, event_date);

COMMENT ON TABLE ml.snapshot_label_links IS
    'Deferred, versioned, replayable association between a later-known ignition event cause '
    'and the scientific snapshot(s) valid at that time/place. Prepares Phase 5 dataset '
    'construction; builds nothing automatically.';
COMMENT ON COLUMN ml.snapshot_label_links.label_class IS
    'Mirrors fire.ignition_events.cause_category plus no_event; unknown/indeterminate is never '
    'treated as a negative and is never derived from raw.firms_observations.';
COMMENT ON COLUMN ml.snapshot_label_links.matching_rule_version IS
    'Version of the spatial/temporal matching rule that produced this link, for reproducibility.';
