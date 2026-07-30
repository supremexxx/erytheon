-- Phase 4A.5: recorded degradation alerts. Purely additive.
--
-- Alerts are produced by a versioned rule engine (store::observability
-- rules, see PHASE4A5_OPERATIONAL_OBSERVABILITY_REPORT.md) evaluated
-- against operational and scientific snapshots. This phase only records
-- and displays alerts (console + GET API); it never sends email, SMS, or
-- webhooks, and it never triggers an automated rollback or model change.

CREATE TABLE observability.snapshot_alerts (
    id BIGSERIAL PRIMARY KEY,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    severity TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    rule_version TEXT NOT NULL,
    observed_value TEXT,
    threshold TEXT,
    message TEXT NOT NULL,
    system_snapshot_id BIGINT REFERENCES observability.system_snapshots(id) ON DELETE RESTRICT,
    scientific_snapshot_id UUID REFERENCES observability.scientific_snapshots(id) ON DELETE RESTRICT,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT snapshot_alerts_severity_check CHECK (severity IN ('info', 'warning', 'critical')),
    CONSTRAINT snapshot_alerts_rule_id_not_blank CHECK (BTRIM(rule_id) <> ''),
    CONSTRAINT snapshot_alerts_rule_version_not_blank CHECK (BTRIM(rule_version) <> ''),
    CONSTRAINT snapshot_alerts_message_not_blank CHECK (BTRIM(message) <> ''),
    CONSTRAINT snapshot_alerts_metadata_object CHECK (JSONB_TYPEOF(metadata) = 'object')
);

CREATE INDEX snapshot_alerts_detected_at_idx
    ON observability.snapshot_alerts (detected_at DESC);
CREATE INDEX snapshot_alerts_severity_idx
    ON observability.snapshot_alerts (severity, detected_at DESC);
CREATE INDEX snapshot_alerts_rule_id_idx
    ON observability.snapshot_alerts (rule_id);

COMMENT ON TABLE observability.snapshot_alerts IS
    'Recorded, versioned degradation alerts. Display and GET-API only in this phase; '
    'no email/SMS/webhook, no automated remediation.';
COMMENT ON COLUMN observability.snapshot_alerts.rule_version IS
    'Version of the rule definition that produced this alert, so historical alerts remain '
    'interpretable after a rule threshold changes.';
