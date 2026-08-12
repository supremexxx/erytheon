//! Phase 4A.5: automated operational and scientific observability
//! snapshots. This module never scores a candidate, never activates a
//! model, never touches the risk engine or FWI, and never deletes a
//! published snapshot. See `PHASE4A5_SNAPSHOT_ARCHITECTURE_DECISION.md`
//! for the initial volumetry reasoning. The compact daily archive added
//! later preserves the same `nowcast`-only scope without full-row overhead.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{Store, StoreError};

/// Externally supplied context a snapshot cannot derive from SQL alone
/// (deployment metadata, VPS-side proxy state).
#[derive(Clone, Debug, Default)]
pub struct SystemSnapshotContext {
    pub application_revision: Option<String>,
    pub application_image: Option<String>,
    pub application_image_digest: Option<String>,
    pub application_restart_count: Option<i64>,
    /// `None` maps to `"non_exposed"`: `PostgreSQL` cannot observe Caddy
    /// by itself, and this module never invents that value.
    pub caddy_state: Option<String>,
    /// Invocation provenance. Defaults to `unknown` and never changes
    /// the logical identity of the captured window.
    pub trigger_kind: Option<String>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct SystemSnapshotRow {
    pub id: i64,
    pub captured_at: DateTime<Utc>,
    pub capture_date: NaiveDate,
    pub capture_window_start: DateTime<Utc>,
    pub capture_window_end: DateTime<Utc>,
    pub provenance_status: String,
    pub environment: String,
    pub cadence: String,
    pub application_revision: Option<String>,
    pub application_image: Option<String>,
    pub application_image_digest: Option<String>,
    pub application_healthy: Option<bool>,
    pub database_healthy: Option<bool>,
    pub caddy_state: String,
    pub application_restart_count: Option<i64>,
    pub migrations_applied: Option<i32>,
    pub migrations_failed: Option<i32>,
    pub active_model_id: Option<i64>,
    pub active_model_name: Option<String>,
    pub active_model_count: Option<i32>,
    pub candidate_id: Option<i64>,
    pub candidate_name: Option<String>,
    pub candidate_status: Option<String>,
    pub shadow_scoring_enabled: bool,
    pub firms_observation_count: Option<i64>,
    pub firms_last_success_at: Option<DateTime<Utc>>,
    pub firms_age_seconds: Option<i64>,
    pub forecast_last_complete_at: Option<DateTime<Utc>>,
    pub forecast_age_seconds: Option<i64>,
    pub forecast_horizon_count: Option<i32>,
    pub import_batches_total: Option<i64>,
    pub import_batches_success_24h: Option<i64>,
    pub import_batches_failed_24h: Option<i64>,
    pub pipeline_runs_total: Option<i64>,
    pub pipeline_runs_success_24h: Option<i64>,
    pub pipeline_runs_failed_24h: Option<i64>,
    pub warning_count_24h: Option<i64>,
    pub error_count_24h: Option<i64>,
    pub static_cell_count: Option<i64>,
    pub feature_snapshot_count: Option<i32>,
    pub dataset_version_count: Option<i32>,
    pub metadata: Value,
    pub checksum: String,
    pub created_at: DateTime<Utc>,
}

/// One field of a J-1/J-7 style comparison. `relative_delta` stays
/// `None` whenever the previous value is zero, absent, or the metric is
/// not numeric -- never a divide-by-zero percentage.
#[derive(Clone, Debug, Serialize)]
pub struct ComparisonEntry {
    pub metric: String,
    pub current_value: Option<i64>,
    pub previous_value: Option<i64>,
    pub absolute_delta: Option<i64>,
    pub relative_delta: Option<f64>,
    pub status: &'static str,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct ScientificSnapshotRow {
    pub id: String,
    pub logical_id: String,
    pub family: String,
    pub snapshot_type: String,
    pub resolution_h3: i16,
    pub valid_at: DateTime<Utc>,
    pub captured_at: DateTime<Utc>,
    pub source_period_start: Option<DateTime<Utc>>,
    pub source_period_end: Option<DateTime<Utc>>,
    pub application_revision: Option<String>,
    pub feature_schema_version: String,
    pub transform_version: String,
    pub static_snapshot_id: Option<String>,
    pub pipeline_run_id: Option<String>,
    pub cell_count_expected: i64,
    pub cell_count_present: i64,
    pub complete: bool,
    pub missing_count: i64,
    pub checksum: Option<String>,
    pub storage_kind: String,
    pub storage_location: String,
    pub status: String,
    pub temporal_classification: String,
    pub published_at: Option<DateTime<Utc>>,
    pub contract_version: i16,
    pub traceability_status: String,
    pub environment: Option<String>,
    pub application_image: Option<String>,
    pub application_image_digest: Option<String>,
    pub forecast_batch_computed_at: Option<DateTime<Utc>>,
    pub forecast_valid_at: Option<DateTime<Utc>>,
    pub forecast_horizon: Option<String>,
    pub coverage_mask_id: Option<String>,
    pub modelable_cell_count: Option<i64>,
    pub structural_exclusion_count: i64,
    pub unexpected_missing_count: i64,
    pub completeness_status: String,
}

#[derive(Clone, Debug)]
pub struct ScientificSnapshotContext {
    pub environment: String,
    pub application_revision: String,
    pub application_image: String,
    pub application_image_digest: String,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct SnapshotAlertRow {
    pub id: i64,
    pub detected_at: DateTime<Utc>,
    pub severity: String,
    pub rule_id: String,
    pub rule_version: String,
    pub observed_value: Option<String>,
    pub threshold: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeferredLabelLinkReport {
    pub snapshot_id: String,
    pub dry_run: bool,
    pub eligible_events: i64,
    pub human_known: i64,
    pub natural_known: i64,
    pub unknown_or_indeterminate: i64,
    pub h3_found: i64,
    pub h3_absent: i64,
    pub proposed_links: i64,
    pub excluded_from_supervised_labels: i64,
    pub inserted_links: i64,
    pub superseded_links: i64,
    pub rule_version: &'static str,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct SnapshotCaptureAttemptRow {
    pub id: i64,
    pub environment: String,
    pub cadence: String,
    pub capture_window_start: DateTime<Utc>,
    pub attempt_number: i32,
    pub trigger_kind: String,
    pub status: String,
    pub system_snapshot_id: Option<i64>,
    pub error_message: Option<String>,
    pub rows_processed: Option<i64>,
    pub pipeline_run_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize)]
pub struct ScientificSnapshotVerification {
    pub snapshot_id: String,
    pub mode: &'static str,
    pub valid: bool,
    pub checksum_valid: bool,
    pub unique_h3: bool,
    pub provenance_complete: bool,
    pub coverage_consistent: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

type MatureLabelCandidate = (String, i64, NaiveDate, String, String, DateTime<Utc>, bool);

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct HourlySnapshotSummary {
    pub present_slots: i64,
    pub expected_slots: i64,
    pub missing_slots: i64,
    pub last_window_start: Option<DateTime<Utc>>,
    pub failed_attempts: i64,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct DeferredLabelSummary {
    pub total: i64,
    pub human_known: i64,
    pub natural_known: i64,
    pub unknown_or_indeterminate: i64,
    pub mature: i64,
    pub provisional: i64,
}

/// Configurable freshness thresholds (spec §13). Defaults match the
/// proposal in the phase commande but must remain overridable, not
/// hardcoded in the frontend.
#[derive(Clone, Copy, Debug)]
pub struct FreshnessThresholds {
    pub forecast_transient_secs: i64,
    pub forecast_degraded_secs: i64,
    pub forecast_stale_secs: i64,
    pub firms_transient_secs: i64,
    pub firms_degraded_secs: i64,
    pub firms_stale_secs: i64,
}

impl Default for FreshnessThresholds {
    fn default() -> Self {
        Self {
            forecast_transient_secs: 3 * 3600,
            forecast_degraded_secs: 6 * 3600,
            forecast_stale_secs: 12 * 3600,
            firms_transient_secs: 6 * 3600,
            firms_degraded_secs: 24 * 3600,
            firms_stale_secs: 72 * 3600,
        }
    }
}

fn freshness_band(
    age_seconds: Option<i64>,
    transient: i64,
    degraded: i64,
    stale: i64,
) -> &'static str {
    match age_seconds {
        None => "unavailable",
        Some(age) if age < transient => "normal",
        Some(age) if age < degraded => "transient",
        Some(age) if age < stale => "degraded",
        Some(_) => "stale",
    }
}

fn checksum_of(fields: &BTreeMap<&'static str, Value>) -> String {
    let canonical = serde_json::to_vec(fields).unwrap_or_default();
    let digest = Sha256::digest(&canonical);
    hex_encode(&digest)
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

struct LineageMetrics {
    migrations_applied: i64,
    migrations_failed: i64,
    active_model_id: Option<i64>,
    active_model_name: Option<String>,
    active_model_count: i32,
    candidate_id: Option<i64>,
    candidate_name: Option<String>,
    candidate_status: Option<String>,
    firms_observation_count: i64,
    firms_last_success_at: Option<DateTime<Utc>>,
    firms_age_seconds: Option<i64>,
    forecast_last_complete_at: Option<DateTime<Utc>>,
    forecast_age_seconds: Option<i64>,
    forecast_horizon_count: i64,
}

struct VolumeCounts {
    import_batches_total: i64,
    import_batches_success_24h: i64,
    import_batches_failed_24h: i64,
    pipeline_runs_total: i64,
    pipeline_runs_success_24h: i64,
    pipeline_runs_failed_24h: i64,
    warning_count_24h: i64,
    error_count_24h: i64,
    static_cell_count: i64,
    feature_snapshot_count: i64,
    dataset_version_count: i64,
}

fn snapshot_window(cadence: &str, captured_at: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    match cadence {
        "hourly" => {
            let start =
                DateTime::from_timestamp(captured_at.timestamp().div_euclid(3600) * 3600, 0)
                    .unwrap_or(captured_at);
            (start, start + ChronoDuration::hours(1))
        }
        "daily" => {
            let start = captured_at
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map_or(captured_at, |value| value.and_utc());
            (start, start + ChronoDuration::days(1))
        }
        _ => (captured_at, captured_at + ChronoDuration::microseconds(1)),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_system_checksum(
    environment: &str,
    cadence: &str,
    capture_window_start: DateTime<Utc>,
    ctx: &SystemSnapshotContext,
    application_healthy: bool,
    database_healthy: bool,
    caddy_state: &str,
    lineage: &LineageMetrics,
    volume: &VolumeCounts,
    shadow_scoring_enabled: bool,
) -> String {
    let mut fields: BTreeMap<&'static str, Value> = BTreeMap::new();
    fields.insert("environment", json!(environment));
    fields.insert("capture_window_start", json!(capture_window_start));
    fields.insert("cadence", json!(cadence));
    fields.insert("application_revision", json!(ctx.application_revision));
    fields.insert("application_image", json!(ctx.application_image));
    fields.insert(
        "application_image_digest",
        json!(ctx.application_image_digest),
    );
    fields.insert(
        "application_restart_count",
        json!(ctx.application_restart_count),
    );
    fields.insert("application_healthy", json!(application_healthy));
    fields.insert("database_healthy", json!(database_healthy));
    fields.insert("caddy_state", json!(caddy_state));
    fields.insert("migrations_applied", json!(lineage.migrations_applied));
    fields.insert("migrations_failed", json!(lineage.migrations_failed));
    fields.insert("active_model_count", json!(lineage.active_model_count));
    fields.insert("candidate_status", json!(lineage.candidate_status));
    fields.insert("shadow_scoring_enabled", json!(shadow_scoring_enabled));
    fields.insert(
        "firms_observation_count",
        json!(lineage.firms_observation_count),
    );
    fields.insert(
        "firms_last_success_at",
        json!(lineage.firms_last_success_at),
    );
    fields.insert(
        "forecast_last_complete_at",
        json!(lineage.forecast_last_complete_at),
    );
    fields.insert(
        "forecast_horizon_count",
        json!(lineage.forecast_horizon_count),
    );
    fields.insert("import_batches_total", json!(volume.import_batches_total));
    fields.insert(
        "import_batches_success_24h",
        json!(volume.import_batches_success_24h),
    );
    fields.insert(
        "import_batches_failed_24h",
        json!(volume.import_batches_failed_24h),
    );
    fields.insert("pipeline_runs_total", json!(volume.pipeline_runs_total));
    fields.insert(
        "pipeline_runs_success_24h",
        json!(volume.pipeline_runs_success_24h),
    );
    fields.insert(
        "pipeline_runs_failed_24h",
        json!(volume.pipeline_runs_failed_24h),
    );
    fields.insert("warning_count_24h", json!(volume.warning_count_24h));
    fields.insert("error_count_24h", json!(volume.error_count_24h));
    fields.insert("static_cell_count", json!(volume.static_cell_count));
    fields.insert(
        "feature_snapshot_count",
        json!(volume.feature_snapshot_count),
    );
    fields.insert("dataset_version_count", json!(volume.dataset_version_count));
    checksum_of(&fields)
}

type AlertCandidate = (&'static str, &'static str, String, String, String);

/// Pure, versioned rule evaluation (spec §23): produces alert
/// candidates from one system snapshot without touching the database,
/// so the rules stay independently testable from persistence.
fn build_alert_candidates(
    snapshot: &SystemSnapshotRow,
    thresholds: FreshnessThresholds,
) -> Vec<AlertCandidate> {
    let mut candidates: Vec<AlertCandidate> = Vec::new();

    let forecast_band = freshness_band(
        snapshot.forecast_age_seconds,
        thresholds.forecast_transient_secs,
        thresholds.forecast_degraded_secs,
        thresholds.forecast_stale_secs,
    );
    if matches!(forecast_band, "degraded" | "stale" | "unavailable") {
        let severity = if matches!(forecast_band, "stale" | "unavailable") {
            "critical"
        } else {
            "warning"
        };
        candidates.push((
            "forecast_freshness",
            severity,
            snapshot
                .forecast_age_seconds
                .map_or_else(|| "unavailable".to_owned(), |v| v.to_string()),
            thresholds.forecast_degraded_secs.to_string(),
            format!("forecast freshness band is {forecast_band}"),
        ));
    }

    let firms_band = freshness_band(
        snapshot.firms_age_seconds,
        thresholds.firms_transient_secs,
        thresholds.firms_degraded_secs,
        thresholds.firms_stale_secs,
    );
    if matches!(firms_band, "degraded" | "stale" | "unavailable") {
        let severity = if matches!(firms_band, "stale" | "unavailable") {
            "critical"
        } else {
            "warning"
        };
        candidates.push((
            "firms_freshness",
            severity,
            snapshot
                .firms_age_seconds
                .map_or_else(|| "unavailable".to_owned(), |v| v.to_string()),
            thresholds.firms_degraded_secs.to_string(),
            format!("FIRMS freshness band is {firms_band}"),
        ));
    }

    match snapshot.active_model_count {
        Some(1) => {}
        other => candidates.push((
            "active_model_count",
            "critical",
            other.map_or_else(|| "none".to_owned(), |v| v.to_string()),
            "1".to_owned(),
            "expected exactly one active model".to_owned(),
        )),
    }

    if snapshot.candidate_status.as_deref() == Some("active") {
        candidates.push((
            "candidate_unexpectedly_active",
            "critical",
            "active".to_owned(),
            "candidate|inactive".to_owned(),
            "candidate registry reports an active status, which the schema should forbid"
                .to_owned(),
        ));
    }

    if snapshot.shadow_scoring_enabled {
        candidates.push((
            "shadow_scoring_unexpected",
            "critical",
            "true".to_owned(),
            "false".to_owned(),
            "shadow scoring flag is enabled outside phase P3".to_owned(),
        ));
    }

    if snapshot.migrations_failed.unwrap_or(0) > 0 {
        candidates.push((
            "migration_failed",
            "critical",
            snapshot.migrations_failed.unwrap_or(0).to_string(),
            "0".to_owned(),
            "one or more migrations failed".to_owned(),
        ));
    }

    candidates
}

impl Store {
    /// Captures one operational observability snapshot. Idempotent per
    /// `(environment, capture_date, cadence)`: a same-day re-run
    /// recomputes deterministically and upserts in place rather than
    /// creating a second row (`observability.system_snapshots` carries
    /// the `UNIQUE` constraint that makes this physically impossible to
    /// violate).
    ///
    /// # Errors
    ///
    /// Returns an error when any underlying query fails.
    pub async fn capture_system_snapshot(
        &self,
        environment: &str,
        cadence: &str,
        captured_at: DateTime<Utc>,
        ctx: &SystemSnapshotContext,
    ) -> Result<SystemSnapshotRow, StoreError> {
        let (capture_window_start, capture_window_end) = snapshot_window(cadence, captured_at);
        let attempt_id = self
            .start_snapshot_attempt(
                environment,
                cadence,
                capture_window_start,
                capture_window_end,
                ctx,
            )
            .await?;
        let result = self
            .capture_system_snapshot_inner(
                environment,
                cadence,
                captured_at,
                capture_window_start,
                capture_window_end,
                ctx,
            )
            .await;
        match &result {
            Ok(row) => {
                sqlx::query(
                    "UPDATE observability.snapshot_capture_attempts
                     SET status = 'succeeded', system_snapshot_id = $2,
                         checksum = $3, rows_processed = 1, finished_at = NOW()
                     WHERE id = $1",
                )
                .bind(attempt_id)
                .bind(row.id)
                .bind(&row.checksum)
                .execute(&self.pool)
                .await?;
            }
            Err(error) => {
                let _ = sqlx::query(
                    "UPDATE observability.snapshot_capture_attempts
                     SET status = 'failed', error_message = $2, finished_at = NOW()
                     WHERE id = $1",
                )
                .bind(attempt_id)
                .bind(error.to_string())
                .execute(&self.pool)
                .await;
            }
        }
        result
    }

    async fn start_snapshot_attempt(
        &self,
        environment: &str,
        cadence: &str,
        capture_window_start: DateTime<Utc>,
        capture_window_end: DateTime<Utc>,
        ctx: &SystemSnapshotContext,
    ) -> Result<i64, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let lock_key = format!("snapshot:{environment}:{cadence}:{capture_window_start}");
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_key)
            .execute(&mut *transaction)
            .await?;
        let id = sqlx::query_scalar(
            "INSERT INTO observability.snapshot_capture_attempts (
                environment, cadence, capture_window_start, capture_window_end,
                attempt_number, trigger_kind, application_revision, application_image,
                application_image_digest
             ) VALUES ($1,$2,$3,$4,
                COALESCE((SELECT MAX(attempt_number) + 1
                          FROM observability.snapshot_capture_attempts
                          WHERE environment=$1 AND cadence=$2 AND capture_window_start=$3), 1),
                $5,$6,$7,$8)
             RETURNING id",
        )
        .bind(environment)
        .bind(cadence)
        .bind(capture_window_start)
        .bind(capture_window_end)
        .bind(ctx.trigger_kind.as_deref().unwrap_or("unknown"))
        .bind(&ctx.application_revision)
        .bind(&ctx.application_image)
        .bind(&ctx.application_image_digest)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    async fn capture_system_snapshot_inner(
        &self,
        environment: &str,
        cadence: &str,
        captured_at: DateTime<Utc>,
        capture_window_start: DateTime<Utc>,
        capture_window_end: DateTime<Utc>,
        ctx: &SystemSnapshotContext,
    ) -> Result<SystemSnapshotRow, StoreError> {
        let capture_date = captured_at.date_naive();
        let application_healthy = self.health_check().await.is_ok();
        let database_healthy = application_healthy;

        let lineage = self.gather_lineage_metrics(captured_at).await?;
        let volume = self.gather_volume_counts(captured_at).await?;

        // Never implemented anywhere in this codebase; kept explicit and
        // sourced from code, not guessed, so a future P3 wiring is a
        // one-line change here rather than a silent always-false default
        // hidden in SQL.
        let shadow_scoring_enabled = false;
        let caddy_state = ctx
            .caddy_state
            .clone()
            .unwrap_or_else(|| "non_exposed".to_owned());

        let checksum = build_system_checksum(
            environment,
            cadence,
            capture_window_start,
            ctx,
            application_healthy,
            database_healthy,
            &caddy_state,
            &lineage,
            &volume,
            shadow_scoring_enabled,
        );

        self.upsert_system_snapshot(
            environment,
            cadence,
            captured_at,
            capture_date,
            capture_window_start,
            capture_window_end,
            ctx,
            application_healthy,
            database_healthy,
            &caddy_state,
            &lineage,
            &volume,
            shadow_scoring_enabled,
            &checksum,
        )
        .await
    }

    async fn gather_lineage_metrics(
        &self,
        captured_at: DateTime<Utc>,
    ) -> Result<LineageMetrics, StoreError> {
        let migrations: (i64, i64) = sqlx::query_as(
            "SELECT
                (SELECT COUNT(*) FROM _sqlx_migrations WHERE success),
                (SELECT COUNT(*) FROM _sqlx_migrations WHERE NOT success)",
        )
        .fetch_one(&self.pool)
        .await?;

        let active_models: Vec<(i64, DateTime<Utc>)> =
            sqlx::query_as("SELECT id, trained_at FROM human_model_versions WHERE active")
                .fetch_all(&self.pool)
                .await?;
        let active_model_count = i32::try_from(active_models.len()).unwrap_or(i32::MAX);
        let active_model_id = active_models.first().map(|(id, _)| *id);
        let active_model_name = active_model_id.map(|id| format!("human_model_versions#{id}"));

        let candidate: Option<(i64, String, String)> = sqlx::query_as(
            "SELECT id, status, model_name FROM ml.model_candidate_registry
             ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        let firms_observation_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM raw.firms_observations")
                .fetch_one(&self.pool)
                .await?;
        let firms_last_success_at: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT last_success FROM public.source_status WHERE id = 'firms'")
                .fetch_optional(&self.pool)
                .await?
                .flatten();
        let firms_age_seconds =
            firms_last_success_at.map(|t| (captured_at - t).num_seconds().max(0));

        let forecast_last_complete_at: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT MAX(completed_at) FROM forecast_batches WHERE completed_at IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        let forecast_age_seconds =
            forecast_last_complete_at.map(|t| (captured_at - t).num_seconds().max(0));
        let forecast_horizon_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT horizon) FROM forecast_fwi
             WHERE computed_at = (
                 SELECT computed_at FROM forecast_batches
                 WHERE completed_at IS NOT NULL
                 ORDER BY completed_at DESC LIMIT 1
             )",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(LineageMetrics {
            migrations_applied: migrations.0,
            migrations_failed: migrations.1,
            active_model_id,
            active_model_name,
            active_model_count,
            candidate_id: candidate.as_ref().map(|(id, ..)| *id),
            candidate_name: candidate.as_ref().map(|(_, _, name)| name.clone()),
            candidate_status: candidate.map(|(_, status, _)| status),
            firms_observation_count,
            firms_last_success_at,
            firms_age_seconds,
            forecast_last_complete_at,
            forecast_age_seconds,
            forecast_horizon_count,
        })
    }

    async fn gather_volume_counts(
        &self,
        captured_at: DateTime<Utc>,
    ) -> Result<VolumeCounts, StoreError> {
        let import_batches_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ops.import_batches")
                .fetch_one(&self.pool)
                .await?;
        let import_batches_success_24h: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ops.import_batches
             WHERE status IN ('succeeded', 'partially_succeeded')
               AND started_at >= $1 - INTERVAL '24 hours'",
        )
        .bind(captured_at)
        .fetch_one(&self.pool)
        .await?;
        let import_batches_failed_24h: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ops.import_batches
             WHERE status = 'failed' AND started_at >= $1 - INTERVAL '24 hours'",
        )
        .bind(captured_at)
        .fetch_one(&self.pool)
        .await?;

        let pipeline_runs_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ops.pipeline_runs")
            .fetch_one(&self.pool)
            .await?;
        let pipeline_runs_success_24h: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ops.pipeline_runs
             WHERE status IN ('succeeded', 'partially_succeeded')
               AND started_at >= $1 - INTERVAL '24 hours'",
        )
        .bind(captured_at)
        .fetch_one(&self.pool)
        .await?;
        let pipeline_runs_failed_24h: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ops.pipeline_runs
             WHERE status = 'failed' AND started_at >= $1 - INTERVAL '24 hours'",
        )
        .bind(captured_at)
        .fetch_one(&self.pool)
        .await?;
        let warning_count_24h: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ops.import_batches
             WHERE status = 'partially_succeeded' AND started_at >= $1 - INTERVAL '24 hours'",
        )
        .bind(captured_at)
        .fetch_one(&self.pool)
        .await?;
        let error_count_24h = import_batches_failed_24h + pipeline_runs_failed_24h;

        let static_cell_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cell_static")
            .fetch_one(&self.pool)
            .await?;
        let feature_snapshot_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM features.feature_snapshots")
                .fetch_one(&self.pool)
                .await?;
        let dataset_version_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ml.dataset_versions")
                .fetch_one(&self.pool)
                .await?;

        Ok(VolumeCounts {
            import_batches_total,
            import_batches_success_24h,
            import_batches_failed_24h,
            pipeline_runs_total,
            pipeline_runs_success_24h,
            pipeline_runs_failed_24h,
            warning_count_24h,
            error_count_24h,
            static_cell_count,
            feature_snapshot_count,
            dataset_version_count,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn upsert_system_snapshot(
        &self,
        environment: &str,
        cadence: &str,
        captured_at: DateTime<Utc>,
        capture_date: NaiveDate,
        capture_window_start: DateTime<Utc>,
        capture_window_end: DateTime<Utc>,
        ctx: &SystemSnapshotContext,
        application_healthy: bool,
        database_healthy: bool,
        caddy_state: &str,
        lineage: &LineageMetrics,
        volume: &VolumeCounts,
        shadow_scoring_enabled: bool,
        checksum: &str,
    ) -> Result<SystemSnapshotRow, StoreError> {
        let row: SystemSnapshotRow = sqlx::query_as(
            "INSERT INTO observability.system_snapshots (
                captured_at, capture_date, capture_window_start, capture_window_end,
                provenance_status, environment, cadence,
                application_revision, application_image, application_image_digest,
                application_healthy, database_healthy,
                caddy_state, application_restart_count, migrations_applied, migrations_failed,
                active_model_id, active_model_name, active_model_count,
                candidate_id, candidate_name, candidate_status, shadow_scoring_enabled,
                firms_observation_count, firms_last_success_at, firms_age_seconds,
                forecast_last_complete_at, forecast_age_seconds, forecast_horizon_count,
                import_batches_total, import_batches_success_24h, import_batches_failed_24h,
                pipeline_runs_total, pipeline_runs_success_24h, pipeline_runs_failed_24h,
                warning_count_24h, error_count_24h, static_cell_count,
                feature_snapshot_count, dataset_version_count, metadata, checksum
             ) VALUES (
                $1, $2, $3, $4, 'captured', $5, $6, $7, $8, $9, $10, $11, $12, $13, $14::int, $15::int,
                $16, $17, $18::int, $19, $20, $21, $22,
                $23, $24, $25, $26, $27, $28::int,
                $29, $30, $31, $32, $33, $34, $35, $36, $37, $38::int, $39::int, '{}'::jsonb, $40
             )
             ON CONFLICT (environment, cadence, capture_window_start) DO UPDATE SET
                capture_window_start = observability.system_snapshots.capture_window_start
             RETURNING *",
        )
        .bind(captured_at)
        .bind(capture_date)
        .bind(capture_window_start)
        .bind(capture_window_end)
        .bind(environment)
        .bind(cadence)
        .bind(&ctx.application_revision)
        .bind(&ctx.application_image)
        .bind(&ctx.application_image_digest)
        .bind(application_healthy)
        .bind(database_healthy)
        .bind(caddy_state)
        .bind(ctx.application_restart_count)
        .bind(i32::try_from(lineage.migrations_applied).unwrap_or(i32::MAX))
        .bind(i32::try_from(lineage.migrations_failed).unwrap_or(i32::MAX))
        .bind(lineage.active_model_id)
        .bind(&lineage.active_model_name)
        .bind(lineage.active_model_count)
        .bind(lineage.candidate_id)
        .bind(&lineage.candidate_name)
        .bind(&lineage.candidate_status)
        .bind(shadow_scoring_enabled)
        .bind(lineage.firms_observation_count)
        .bind(lineage.firms_last_success_at)
        .bind(lineage.firms_age_seconds)
        .bind(lineage.forecast_last_complete_at)
        .bind(lineage.forecast_age_seconds)
        .bind(i32::try_from(lineage.forecast_horizon_count).unwrap_or(i32::MAX))
        .bind(volume.import_batches_total)
        .bind(volume.import_batches_success_24h)
        .bind(volume.import_batches_failed_24h)
        .bind(volume.pipeline_runs_total)
        .bind(volume.pipeline_runs_success_24h)
        .bind(volume.pipeline_runs_failed_24h)
        .bind(volume.warning_count_24h)
        .bind(volume.error_count_24h)
        .bind(volume.static_cell_count)
        .bind(i32::try_from(volume.feature_snapshot_count).unwrap_or(i32::MAX))
        .bind(i32::try_from(volume.dataset_version_count).unwrap_or(i32::MAX))
        .bind(checksum)
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn latest_system_snapshot(
        &self,
        environment: &str,
        cadence: &str,
    ) -> Result<Option<SystemSnapshotRow>, StoreError> {
        let row = sqlx::query_as(
            "SELECT * FROM observability.system_snapshots
             WHERE environment = $1 AND cadence = $2
             ORDER BY capture_window_start DESC LIMIT 1",
        )
        .bind(environment)
        .bind(cadence)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn system_snapshot_history(
        &self,
        environment: &str,
        cadence: &str,
        days: i64,
    ) -> Result<Vec<SystemSnapshotRow>, StoreError> {
        let rows = sqlx::query_as(
            "SELECT * FROM observability.system_snapshots
             WHERE environment = $1 AND cadence = $2
               AND capture_window_start >= (CURRENT_DATE - $3::int)
             ORDER BY capture_window_start DESC",
        )
        .bind(environment)
        .bind(cadence)
        .bind(i32::try_from(days).unwrap_or(i32::MAX))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Compares the latest daily snapshot to the one `days_ago` days
    /// earlier. Returns `None` when either side is missing (no data
    /// before the first snapshot is ever fabricated).
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn compare_system_snapshots(
        &self,
        environment: &str,
        days_ago: i64,
    ) -> Result<Option<Vec<ComparisonEntry>>, StoreError> {
        let Some(latest) = self.latest_system_snapshot(environment, "daily").await? else {
            return Ok(None);
        };
        let target_date = latest.capture_date - ChronoDuration::days(days_ago);
        let previous: Option<SystemSnapshotRow> = sqlx::query_as(
            "SELECT * FROM observability.system_snapshots
             WHERE environment = $1 AND cadence = 'daily' AND capture_date = $2",
        )
        .bind(environment)
        .bind(target_date)
        .fetch_optional(&self.pool)
        .await?;
        let Some(previous) = previous else {
            return Ok(None);
        };

        Ok(Some(compare_fields(&latest, &previous)))
    }

    /// Evaluates the versioned degradation rules against one system
    /// snapshot and records any new alert. Never sends email/SMS/
    /// webhook and never triggers remediation; recording only.
    ///
    /// # Errors
    ///
    /// Returns an error when a query fails.
    pub async fn evaluate_and_record_alerts(
        &self,
        snapshot: &SystemSnapshotRow,
        thresholds: FreshnessThresholds,
    ) -> Result<Vec<SnapshotAlertRow>, StoreError> {
        const RULE_VERSION: &str = "v1";
        let candidates = build_alert_candidates(snapshot, thresholds);

        let mut recorded = Vec::new();
        for (rule_id, severity, observed_value, threshold, message) in candidates {
            let existing: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM observability.snapshot_alerts
                 WHERE rule_id = $1 AND system_snapshot_id = $2",
            )
            .bind(rule_id)
            .bind(snapshot.id)
            .fetch_optional(&self.pool)
            .await?;
            if existing.is_some() {
                continue;
            }
            let alert: SnapshotAlertRow = sqlx::query_as(
                "INSERT INTO observability.snapshot_alerts
                    (severity, rule_id, rule_version, observed_value, threshold, message, system_snapshot_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING id, detected_at, severity, rule_id, rule_version, observed_value, threshold, message",
            )
            .bind(severity)
            .bind(rule_id)
            .bind(RULE_VERSION)
            .bind(&observed_value)
            .bind(&threshold)
            .bind(&message)
            .bind(snapshot.id)
            .fetch_one(&self.pool)
            .await?;
            recorded.push(alert);
        }
        Ok(recorded)
    }

    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn list_snapshot_alerts(
        &self,
        severity: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SnapshotAlertRow>, StoreError> {
        let rows = sqlx::query_as(
            "SELECT id, detected_at, severity, rule_id, rule_version, observed_value, threshold, message
             FROM observability.snapshot_alerts
             WHERE ($1::text IS NULL OR severity = $1)
             ORDER BY detected_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(severity)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// # Errors
    ///
    /// Returns an error when the history query fails.
    pub async fn list_snapshot_capture_attempts(
        &self,
        environment: &str,
        limit: i64,
    ) -> Result<Vec<SnapshotCaptureAttemptRow>, StoreError> {
        Ok(sqlx::query_as(
            "SELECT id,environment,cadence,capture_window_start,attempt_number,
                    trigger_kind,status,system_snapshot_id,error_message,rows_processed,
                    pipeline_run_id::text,started_at,finished_at
             FROM observability.snapshot_capture_attempts WHERE environment=$1
             ORDER BY started_at DESC LIMIT $2",
        )
        .bind(environment)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    /// # Errors
    ///
    /// Returns an error when hourly slot aggregation fails.
    pub async fn hourly_snapshot_summary(
        &self,
        environment: &str,
    ) -> Result<HourlySnapshotSummary, StoreError> {
        Ok(sqlx::query_as(
            "WITH slots AS (
                SELECT capture_window_start FROM observability.system_snapshots
                WHERE environment=$1 AND cadence='hourly' AND provenance_status='captured'
             ), bounds AS (SELECT MIN(capture_window_start) lo,MAX(capture_window_start) hi FROM slots)
             SELECT (SELECT COUNT(*) FROM slots)::bigint AS present_slots,
                    COALESCE((EXTRACT(EPOCH FROM (bounds.hi-bounds.lo))/3600)::bigint+1,0) AS expected_slots,
                    COALESCE((EXTRACT(EPOCH FROM (bounds.hi-bounds.lo))/3600)::bigint+1,0)
                      - (SELECT COUNT(*) FROM slots)::bigint AS missing_slots,
                    bounds.hi AS last_window_start,
                    (SELECT COUNT(*) FROM observability.snapshot_capture_attempts
                     WHERE environment=$1 AND cadence='hourly' AND status='failed')::bigint AS failed_attempts
             FROM bounds",
        )
        .bind(environment)
        .fetch_one(&self.pool)
        .await?)
    }

    /// # Errors
    ///
    /// Returns an error when deferred-label aggregation fails.
    pub async fn deferred_label_summary(&self) -> Result<DeferredLabelSummary, StoreError> {
        Ok(sqlx::query_as(
            "SELECT COUNT(*) FILTER(WHERE is_current)::bigint AS total,
                    COUNT(*) FILTER(WHERE is_current AND label_class='human_known')::bigint AS human_known,
                    COUNT(*) FILTER(WHERE is_current AND label_class='natural_known')::bigint AS natural_known,
                    COUNT(*) FILTER(WHERE is_current AND label_class IN ('unknown','indeterminate'))::bigint AS unknown_or_indeterminate,
                    COUNT(*) FILTER(WHERE is_current AND maturity_status='mature')::bigint AS mature,
                    COUNT(*) FILTER(WHERE is_current AND maturity_status='provisional')::bigint AS provisional
             FROM ml.snapshot_label_links",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    /// Publishes a deterministic operational-AOI denominator. Replaying
    /// identical cells returns the same mask; a changed checksum supersedes
    /// the previous published mask for the family/resolution.
    ///
    /// # Errors
    ///
    /// Returns an error for an empty denominator or rejected invariant.
    pub async fn publish_coverage_mask(
        &self,
        family: &str,
        h3_resolution: i16,
        cells: &[i64],
        source_kind: &str,
    ) -> Result<String, StoreError> {
        if cells.is_empty() {
            return Err(StoreError::InvalidPersistedCount(0));
        }
        let mut ordered = cells.to_vec();
        ordered.sort_unstable();
        ordered.dedup();
        let mut digest = Sha256::new();
        for cell in &ordered {
            digest.update(cell.to_string().as_bytes());
            digest.update(b"\n");
        }
        let checksum = hex_encode(&digest.finalize());
        let logical_id = format!("{family}-h3r{h3_resolution}-{}", &checksum[..16]);
        let mut transaction = self.pool.begin().await?;
        if let Some(id) = sqlx::query_scalar::<_, String>(
            "SELECT id::text FROM observability.coverage_masks WHERE logical_id=$1",
        )
        .bind(&logical_id)
        .fetch_optional(&mut *transaction)
        .await?
        {
            transaction.commit().await?;
            return Ok(id);
        }
        let id: String = sqlx::query_scalar(
            "INSERT INTO observability.coverage_masks
                (logical_id,family,h3_resolution,source_kind,source_checksum,
                 expected_cell_count,status,metadata)
             VALUES ($1,$2,$3,$4,$5,$6,'building',
                     jsonb_build_object('ordering','h3_asc','hash','sha256'))
             RETURNING id::text",
        )
        .bind(&logical_id)
        .bind(family)
        .bind(h3_resolution)
        .bind(source_kind)
        .bind(&checksum)
        .bind(i64::try_from(ordered.len()).unwrap_or(i64::MAX))
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO observability.coverage_mask_cells(mask_id,h3)
             SELECT $1::uuid, unnest($2::bigint[])",
        )
        .bind(&id)
        .bind(&ordered)
        .execute(&mut *transaction)
        .await?;
        let persisted: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM observability.coverage_mask_cells WHERE mask_id=$1::uuid",
        )
        .bind(&id)
        .fetch_one(&mut *transaction)
        .await?;
        if persisted != i64::try_from(ordered.len()).unwrap_or(i64::MAX) {
            return Err(StoreError::InvalidPersistedCount(persisted));
        }
        sqlx::query(
            "UPDATE observability.coverage_masks SET status='superseded'
             WHERE family=$1 AND h3_resolution=$2 AND status='published' AND id<>$3::uuid",
        )
        .bind(family)
        .bind(h3_resolution)
        .bind(&id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE observability.coverage_masks
             SET status='published',published_at=NOW() WHERE id=$1::uuid",
        )
        .bind(&id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(id)
    }

    /// Captures one compact, immutable daily archive after a successful
    /// operational forecast. Six FWI components are packed as network-order
    /// float32 arrays in immutable coverage-mask H3 order. The first complete
    /// forecast for a UTC validity date wins; later cycles are idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error if provenance is incomplete, the exact forecast batch
    /// is unavailable, coverage is incomplete, or publication fails.
    pub async fn capture_daily_dense_scientific_snapshot(
        &self,
        forecast_batch_computed_at: DateTime<Utc>,
        forecast_valid_at: DateTime<Utc>,
        forecast_source_id: &str,
    ) -> Result<ScientificSnapshotRow, StoreError> {
        let context = ScientificSnapshotContext {
            environment: std::env::var("ERYTHEON_ENVIRONMENT")
                .unwrap_or_else(|_| "default".to_owned()),
            application_revision: std::env::var("ERYTHEON_GIT_REVISION")
                .or_else(|_| std::env::var("ERYTHEON_APPLICATION_REVISION"))
                .unwrap_or_default(),
            application_image: std::env::var("ERYTHEON_IMAGE_REFERENCE")
                .or_else(|_| std::env::var("ERYTHEON_APPLICATION_IMAGE"))
                .unwrap_or_default(),
            application_image_digest: std::env::var("ERYTHEON_IMAGE_DIGEST").unwrap_or_default(),
        };
        self.capture_daily_dense_scientific_snapshot_v2(
            forecast_batch_computed_at,
            forecast_valid_at,
            forecast_source_id,
            &context,
        )
        .await
    }

    /// Context-explicit daily archive capture used by controlled tooling and
    /// integration tests. Production should normally call the environment-
    /// backed wrapper above.
    ///
    /// # Errors
    ///
    /// Returns an error when provenance, freshness, coverage, or persistence
    /// does not satisfy the strict publication contract.
    #[allow(clippy::too_many_lines)]
    pub async fn capture_daily_dense_scientific_snapshot_v2(
        &self,
        forecast_batch_computed_at: DateTime<Utc>,
        forecast_valid_at: DateTime<Utc>,
        forecast_source_id: &str,
        context: &ScientificSnapshotContext,
    ) -> Result<ScientificSnapshotRow, StoreError> {
        if context.environment.trim().is_empty()
            || context.application_revision.trim().is_empty()
            || context.application_image.trim().is_empty()
            || context.application_image_digest.trim().is_empty()
        {
            return Err(StoreError::SnapshotContract(
                "environment, revision, image tag, and image digest are mandatory".to_owned(),
            ));
        }
        if forecast_source_id.trim().is_empty() {
            return Err(StoreError::SnapshotContract(
                "forecast source id is mandatory".to_owned(),
            ));
        }
        let forecast_age = forecast_batch_computed_at - forecast_valid_at;
        if forecast_age < ChronoDuration::zero() || forecast_age > ChronoDuration::hours(6) {
            return Err(StoreError::SnapshotContract(format!(
                "forecast validity is stale or in the future (age {forecast_age})"
            )));
        }

        let logical_id = format!(
            "scientific-daily-dense-nowcast-{}",
            forecast_valid_at.format("%Y-%m-%d")
        );
        if let Some(existing) = self.scientific_snapshot_by_logical_id(&logical_id).await? {
            if existing.status == "published" {
                return Ok(existing);
            }
            return Err(StoreError::SnapshotContract(format!(
                "daily dense snapshot {logical_id} already exists with status {}",
                existing.status
            )));
        }

        let static_bundle: Option<(String, i16, i64)> = sqlx::query_as(
            "SELECT id::text,h3_resolution,cell_count FROM features.feature_snapshots
             WHERE family='cell_static_bundle' AND status='active'
             ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        let (static_snapshot_id, resolution_h3, cell_count_expected) =
            static_bundle.ok_or_else(|| {
                StoreError::SnapshotContract("no active immutable cell_static_bundle".to_owned())
            })?;
        let mask: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT id::text,expected_cell_count,source_checksum
             FROM observability.coverage_masks
             WHERE family='operational_aoi' AND h3_resolution=$1 AND status='published'",
        )
        .bind(resolution_h3)
        .fetch_optional(&self.pool)
        .await?;
        let (coverage_mask_id, modelable_cell_count, h3_order_checksum) =
            mask.ok_or_else(|| {
                StoreError::SnapshotContract(
                    "no published operational_aoi coverage mask".to_owned(),
                )
            })?;
        let exact_batch: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1 FROM forecast_batches b JOIN forecast_fwi f
                  ON f.computed_at=b.computed_at AND f.horizon='nowcast'
                WHERE b.computed_at=$1 AND b.completed_at IS NOT NULL AND f.valid_at=$2
             )",
        )
        .bind(forecast_batch_computed_at)
        .bind(forecast_valid_at)
        .fetch_one(&self.pool)
        .await?;
        if !exact_batch {
            return Err(StoreError::SnapshotContract(
                "exact complete nowcast forecast batch is unavailable".to_owned(),
            ));
        }

        let pipeline_run_id: String = sqlx::query_scalar(
            "INSERT INTO ops.pipeline_runs(
                id,pipeline_name,pipeline_version,status,started_at,trigger_type,
                parameters,code_version)
             VALUES(gen_random_uuid(),'scientific_daily_dense_archive','v1','running',NOW(),
                    'snapshot',jsonb_build_object('logical_id',$1,'horizon','nowcast'),$2)
             RETURNING id::text",
        )
        .bind(&logical_id)
        .bind(&context.application_revision)
        .fetch_one(&self.pool)
        .await?;
        let manifest_id: Option<String> = sqlx::query_scalar(
            "INSERT INTO observability.scientific_snapshots (
                logical_id,family,snapshot_type,resolution_h3,valid_at,captured_at,
                source_period_start,source_period_end,application_revision,
                feature_schema_version,transform_version,source_versions,static_snapshot_id,
                pipeline_run_id,cell_count_expected,storage_kind,storage_location,status,
                temporal_classification,contract_version,traceability_status,environment,
                application_image,application_image_digest,forecast_batch_computed_at,
                forecast_valid_at,forecast_horizon,coverage_mask_id,modelable_cell_count,
                structural_exclusion_count,completeness_status
             ) VALUES (
                $1,'dynamic_weather_fwi_nowcast','daily_dense',$3,$2,NOW(),$2,$2,$4,
                'v2','dense-float32-v1',jsonb_build_object(
                    'forecast_batch_computed_at',$7::text,'forecast_status','complete',
                    'forecast_horizon','nowcast','forecast_source',$14,
                    'encoding','float32_be_h3_asc_v1'),
                $5::uuid,$13::uuid,$6,'postgres_table',
                'observability.scientific_dense_archives','building',
                'derived_past_only',2,'complete',$8,$9,$10,$7,$2,'nowcast',$11::uuid,
                $12,($6-$12),'building'
             ) ON CONFLICT(logical_id) DO NOTHING RETURNING id::text",
        )
        .bind(&logical_id)
        .bind(forecast_valid_at)
        .bind(resolution_h3)
        .bind(&context.application_revision)
        .bind(&static_snapshot_id)
        .bind(cell_count_expected)
        .bind(forecast_batch_computed_at)
        .bind(&context.environment)
        .bind(&context.application_image)
        .bind(&context.application_image_digest)
        .bind(&coverage_mask_id)
        .bind(modelable_cell_count)
        .bind(&pipeline_run_id)
        .bind(forecast_source_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(manifest_id) = manifest_id else {
            let concurrent = self
                .scientific_snapshot_by_logical_id(&logical_id)
                .await?
                .ok_or_else(|| StoreError::InvalidPersistedCount(0))?;
            let (status, error_message) = if concurrent.status == "published" {
                ("succeeded", None)
            } else {
                (
                    "failed",
                    Some(format!(
                        "concurrent snapshot is not yet published (status {})",
                        concurrent.status
                    )),
                )
            };
            sqlx::query(
                "UPDATE ops.pipeline_runs SET status=$2,finished_at=NOW(),error_message=$3,
                 metrics=jsonb_build_object('result','concurrent_replay') WHERE id=$1::uuid",
            )
            .bind(&pipeline_run_id)
            .bind(status)
            .bind(error_message.as_deref())
            .execute(&self.pool)
            .await?;
            if let Some(error_message) = error_message {
                return Err(StoreError::SnapshotContract(error_message));
            }
            return Ok(concurrent);
        };

        let result = self
            .fill_and_publish_daily_dense_archive(
                &manifest_id,
                &coverage_mask_id,
                &h3_order_checksum,
                forecast_batch_computed_at,
                modelable_cell_count,
            )
            .await;
        match result {
            Ok(snapshot) => {
                sqlx::query(
                    "UPDATE ops.pipeline_runs SET status='succeeded',finished_at=NOW(),
                     metrics=jsonb_build_object('cell_count_present',$2,'packed_bytes',$3)
                     WHERE id=$1::uuid",
                )
                .bind(&pipeline_run_id)
                .bind(snapshot.cell_count_present)
                .bind(snapshot.cell_count_present * 4 * 6)
                .execute(&self.pool)
                .await?;
                Ok(snapshot)
            }
            Err(error) => {
                let _ = sqlx::query(
                    "UPDATE observability.scientific_snapshots
                     SET status='failed',completeness_status='failed_validation',
                         metadata=metadata || jsonb_build_object('failure',$2)
                     WHERE id=$1::uuid AND status<>'published'",
                )
                .bind(&manifest_id)
                .bind(error.to_string())
                .execute(&self.pool)
                .await;
                let _ = sqlx::query(
                    "UPDATE ops.pipeline_runs SET status='failed',finished_at=NOW(),error_message=$2
                     WHERE id=$1::uuid",
                )
                .bind(&pipeline_run_id)
                .bind(error.to_string())
                .execute(&self.pool)
                .await;
                Err(error)
            }
        }
    }

    async fn scientific_snapshot_by_logical_id(
        &self,
        logical_id: &str,
    ) -> Result<Option<ScientificSnapshotRow>, StoreError> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT id::text FROM observability.scientific_snapshots WHERE logical_id=$1",
        )
        .bind(logical_id)
        .fetch_optional(&self.pool)
        .await?;
        match id {
            Some(id) => self.get_scientific_snapshot(&id).await,
            None => Ok(None),
        }
    }

    async fn fill_and_publish_daily_dense_archive(
        &self,
        manifest_id: &str,
        coverage_mask_id: &str,
        h3_order_checksum: &str,
        forecast_batch_computed_at: DateTime<Utc>,
        expected_count: i64,
    ) -> Result<ScientificSnapshotRow, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let packed_count: i64 = sqlx::query_scalar(
            "WITH packed AS (
                SELECT COUNT(*)::bigint AS h3_count,
                    string_agg(float4send(f.ffmc::real),''::bytea ORDER BY c.h3) ffmc_values,
                    string_agg(float4send(f.dmc::real),''::bytea ORDER BY c.h3) dmc_values,
                    string_agg(float4send(f.dc::real),''::bytea ORDER BY c.h3) dc_values,
                    string_agg(float4send(f.isi::real),''::bytea ORDER BY c.h3) isi_values,
                    string_agg(float4send(f.bui::real),''::bytea ORDER BY c.h3) bui_values,
                    string_agg(float4send(f.fwi::real),''::bytea ORDER BY c.h3) fwi_values
                FROM observability.coverage_mask_cells c
                JOIN forecast_fwi f ON f.h3=c.h3 AND f.computed_at=$4
                    AND f.horizon='nowcast'
                WHERE c.mask_id=$2::uuid AND c.eligibility='modelable'
             )
             INSERT INTO observability.scientific_dense_archives(
                snapshot_id,coverage_mask_id,h3_count,h3_order_checksum,
                ffmc_values,dmc_values,dc_values,isi_values,bui_values,fwi_values)
             SELECT $1::uuid,$2::uuid,h3_count,$3,ffmc_values,dmc_values,dc_values,
                    isi_values,bui_values,fwi_values FROM packed
             RETURNING h3_count",
        )
        .bind(manifest_id)
        .bind(coverage_mask_id)
        .bind(h3_order_checksum)
        .bind(forecast_batch_computed_at)
        .fetch_one(&mut *transaction)
        .await?;
        if packed_count != expected_count {
            return Err(StoreError::SnapshotContract(format!(
                "dense archive contains {packed_count} cells; expected {expected_count}"
            )));
        }
        let checksum: String = sqlx::query_scalar(
            "SELECT encode(digest(
                ffmc_values||dmc_values||dc_values||isi_values||bui_values||fwi_values,
                'sha256'),'hex')
             FROM observability.scientific_dense_archives WHERE snapshot_id=$1::uuid",
        )
        .bind(manifest_id)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE observability.scientific_snapshots SET
                cell_count_present=$2,missing_count=structural_exclusion_count,
                unexpected_missing_count=0,complete=true,checksum=$3,
                completeness_status='complete',status='published',published_at=NOW(),
                metadata=metadata || jsonb_build_object(
                    'dense_encoding','float32_be_h3_asc_v1',
                    'component_count',6,'bytes_per_component',4)
             WHERE id=$1::uuid AND status='building'",
        )
        .bind(manifest_id)
        .bind(packed_count)
        .bind(&checksum)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO observability.scientific_snapshot_missing_reasons
                (snapshot_id,reason,cell_count,classification_version)
             SELECT id,'outside_operational_aoi',structural_exclusion_count,'coverage-v1'
             FROM observability.scientific_snapshots
             WHERE id=$1::uuid AND structural_exclusion_count>0",
        )
        .bind(manifest_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.get_scientific_snapshot(manifest_id)
            .await?
            .ok_or_else(|| StoreError::InvalidPersistedCount(0))
    }

    /// Captures (or resumes/no-ops on) the weekly `nowcast`-only
    /// scientific snapshot pilot for `valid_at`'s calendar day. Raw
    /// interpolated weather (temperature/humidity/wind/precipitation)
    /// is not persisted anywhere in the current schema -- only derived
    /// FWI components are (`forecast_fwi`) -- so those fields are
    /// recorded as `data_status = 'missing'` rather than fabricated.
    /// Capturing raw weather would require instrumenting the forecast
    /// engine itself, which is out of scope for this phase.
    ///
    /// # Errors
    ///
    /// Returns an error when a query fails or the publication
    /// constraints cannot be satisfied. On error, the manifest is best-
    /// effort marked `failed` rather than left presenting a partial
    /// snapshot as valid.
    pub async fn capture_weekly_scientific_snapshot(
        &self,
        valid_at: DateTime<Utc>,
    ) -> Result<ScientificSnapshotRow, StoreError> {
        let context = ScientificSnapshotContext {
            environment: std::env::var("ERYTHEON_ENVIRONMENT")
                .unwrap_or_else(|_| "default".to_owned()),
            application_revision: std::env::var("ERYTHEON_GIT_REVISION")
                .or_else(|_| std::env::var("ERYTHEON_APPLICATION_REVISION"))
                .unwrap_or_default(),
            application_image: std::env::var("ERYTHEON_IMAGE_REFERENCE")
                .or_else(|_| std::env::var("ERYTHEON_APPLICATION_IMAGE"))
                .unwrap_or_default(),
            application_image_digest: std::env::var("ERYTHEON_IMAGE_DIGEST").unwrap_or_default(),
        };
        self.capture_weekly_scientific_snapshot_v2(valid_at, &context)
            .await
    }

    /// Captures a contract-v2 weekly snapshot. Publication is fail-closed:
    /// deployment lineage, active static bundle, published coverage mask,
    /// and exact source forecast batch must all be present.
    ///
    /// # Errors
    ///
    /// Returns an error when mandatory provenance is absent, no active
    /// dependency exists, or persistence/validation fails.
    #[allow(clippy::too_many_lines)]
    pub async fn capture_weekly_scientific_snapshot_v2(
        &self,
        valid_at: DateTime<Utc>,
        context: &ScientificSnapshotContext,
    ) -> Result<ScientificSnapshotRow, StoreError> {
        if context.environment.trim().is_empty()
            || context.application_revision.trim().is_empty()
            || context.application_image.trim().is_empty()
            || context.application_image_digest.trim().is_empty()
        {
            return Err(StoreError::SnapshotContract(
                "environment, revision, image tag, and image digest are mandatory".to_owned(),
            ));
        }
        let logical_id = format!("scientific-weekly-nowcast-{}", valid_at.format("%Y-%m-%d"));

        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT id::text, status FROM observability.scientific_snapshots WHERE logical_id = $1",
        )
        .bind(&logical_id)
        .fetch_optional(&self.pool)
        .await?;

        let (manifest_id, owns_capture) = match existing {
            Some((id, status)) if status == "published" => {
                return self
                    .get_scientific_snapshot(&id)
                    .await?
                    .ok_or_else(|| StoreError::InvalidPersistedCount(0));
            }
            Some((id, status)) => (id, status != "building"),
            None => {
                let static_bundle: Option<(String, i16, i64)> = sqlx::query_as(
                    "SELECT id::text, h3_resolution, cell_count FROM features.feature_snapshots
                     WHERE family = 'cell_static_bundle' AND status = 'active'
                     ORDER BY created_at DESC LIMIT 1",
                )
                .fetch_optional(&self.pool)
                .await?;
                let (static_snapshot_id, resolution_h3, cell_count_expected) = static_bundle
                    .ok_or_else(|| {
                        StoreError::SnapshotContract(
                            "no active immutable cell_static_bundle".to_owned(),
                        )
                    })?;
                let mask: Option<(String, i64)> = sqlx::query_as(
                    "SELECT id::text, expected_cell_count FROM observability.coverage_masks
                     WHERE family='operational_aoi' AND h3_resolution=$1 AND status='published'",
                )
                .bind(resolution_h3)
                .fetch_optional(&self.pool)
                .await?;
                let (coverage_mask_id, modelable_cell_count) = mask.ok_or_else(|| {
                    StoreError::SnapshotContract(
                        "no published operational_aoi coverage mask".to_owned(),
                    )
                })?;
                let forecast: Option<(DateTime<Utc>, DateTime<Utc>)> = sqlx::query_as(
                    "SELECT b.computed_at, MAX(f.valid_at)
                     FROM forecast_batches b
                     JOIN forecast_fwi f ON f.computed_at=b.computed_at AND f.horizon='nowcast'
                     WHERE b.completed_at IS NOT NULL
                     GROUP BY b.computed_at,b.completed_at
                     ORDER BY b.completed_at DESC LIMIT 1",
                )
                .fetch_optional(&self.pool)
                .await?;
                let (forecast_batch_computed_at, forecast_valid_at) =
                    forecast.ok_or_else(|| {
                        StoreError::SnapshotContract(
                            "no complete nowcast forecast batch".to_owned(),
                        )
                    })?;
                let pipeline_run_id: String = sqlx::query_scalar(
                    "INSERT INTO ops.pipeline_runs(
                        id,pipeline_name,pipeline_version,status,started_at,trigger_type,
                        parameters,code_version)
                     VALUES(gen_random_uuid(),'scientific_snapshot','v2','running',NOW(),
                            'snapshot',jsonb_build_object('logical_id',$1,'horizon','nowcast'),$2)
                     RETURNING id::text",
                )
                .bind(&logical_id)
                .bind(&context.application_revision)
                .fetch_one(&self.pool)
                .await?;
                let inserted: Option<String> = sqlx::query_scalar(
                    "INSERT INTO observability.scientific_snapshots (
                        logical_id, snapshot_type, resolution_h3, valid_at, captured_at,
                        source_period_start,source_period_end,application_revision,
                        feature_schema_version, transform_version, source_versions,static_snapshot_id,
                        pipeline_run_id,
                        cell_count_expected, storage_kind, storage_location, status,
                        temporal_classification,contract_version,traceability_status,
                        environment,application_image,application_image_digest,
                        forecast_batch_computed_at,forecast_valid_at,forecast_horizon,
                        coverage_mask_id,modelable_cell_count,structural_exclusion_count,
                        completeness_status
                     ) VALUES (
                        $1, 'weekly_full', $3, $2, NOW(),$8,$8,$4,
                        'v2', 'v2',jsonb_build_object(
                          'forecast_batch_computed_at',$7::text,'forecast_status','complete',
                          'forecast_horizon','nowcast'),
                        $5::uuid,$14::uuid, $6, 'postgres_table',
                        'observability.scientific_snapshot_values', 'building',
                        'current_snapshot_applied_historically',2,'complete',$9,$10,$11,
                        $7,$8,'nowcast',$12::uuid,$13,($6-$13),'building'
                     ) ON CONFLICT(logical_id) DO NOTHING RETURNING id::text",
                )
                .bind(&logical_id)
                .bind(valid_at)
                .bind(resolution_h3)
                .bind(&context.application_revision)
                .bind(static_snapshot_id)
                .bind(cell_count_expected)
                .bind(forecast_batch_computed_at)
                .bind(forecast_valid_at)
                .bind(&context.environment)
                .bind(&context.application_image)
                .bind(&context.application_image_digest)
                .bind(coverage_mask_id)
                .bind(modelable_cell_count)
                .bind(&pipeline_run_id)
                .fetch_optional(&self.pool)
                .await?;
                if let Some(id) = inserted {
                    (id, true)
                } else {
                    sqlx::query(
                        "UPDATE ops.pipeline_runs SET status='succeeded',finished_at=NOW(),
                         metrics=jsonb_build_object('result','concurrent_replay') WHERE id=$1::uuid",
                    )
                    .bind(&pipeline_run_id)
                    .execute(&self.pool)
                    .await?;
                    let existing_id = sqlx::query_scalar(
                        "SELECT id::text FROM observability.scientific_snapshots WHERE logical_id=$1",
                    )
                    .bind(&logical_id)
                    .fetch_one(&self.pool)
                    .await?;
                    (existing_id, false)
                }
            }
        };

        if !owns_capture {
            return Err(StoreError::SnapshotContract(
                "a concurrent capture already owns this logical weekly snapshot".to_owned(),
            ));
        }

        match self
            .fill_and_publish_scientific_snapshot_v2(&manifest_id, valid_at)
            .await
        {
            Ok(row) => {
                sqlx::query(
                    "UPDATE ops.pipeline_runs SET status='succeeded',finished_at=NOW(),
                         metrics=jsonb_build_object('cell_count_present',$2,'missing_count',$3)
                     WHERE id=(SELECT pipeline_run_id FROM observability.scientific_snapshots WHERE id=$1::uuid)",
                )
                .bind(&manifest_id)
                .bind(row.cell_count_present)
                .bind(row.missing_count)
                .execute(&self.pool)
                .await?;
                Ok(row)
            }
            Err(err) => {
                let _ = sqlx::query(
                    "UPDATE observability.scientific_snapshots
                     SET status = 'failed',
                         metadata = metadata || jsonb_build_object('failure', $2)
                     WHERE id = $1::uuid AND status <> 'published'",
                )
                .bind(&manifest_id)
                .bind(err.to_string())
                .execute(&self.pool)
                .await;
                let _ = sqlx::query(
                    "UPDATE ops.pipeline_runs SET status='failed',finished_at=NOW(),error_message=$2
                     WHERE id=(SELECT pipeline_run_id FROM observability.scientific_snapshots WHERE id=$1::uuid)",
                )
                .bind(&manifest_id)
                .bind(err.to_string())
                .execute(&self.pool)
                .await;
                Err(err)
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn fill_and_publish_scientific_snapshot_v2(
        &self,
        manifest_id: &str,
        valid_at: DateTime<Utc>,
    ) -> Result<ScientificSnapshotRow, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let latest_computed_at: DateTime<Utc> = sqlx::query_scalar(
            "SELECT forecast_batch_computed_at FROM observability.scientific_snapshots
             WHERE id=$1::uuid",
        )
        .bind(manifest_id)
        .fetch_one(&mut *transaction)
        .await?;

        sqlx::query(
            "DELETE FROM observability.scientific_snapshot_values WHERE snapshot_id = $1::uuid",
        )
        .bind(manifest_id)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            "INSERT INTO observability.scientific_snapshot_values
                (snapshot_id, h3, valid_at, ffmc, dmc, dc, isi, bui, fwi, data_status)
             SELECT $1::uuid, c.h3, $2, f.ffmc, f.dmc, f.dc, f.isi, f.bui, f.fwi,
                    CASE WHEN f.h3 IS NULL THEN 'missing' ELSE 'observed' END
             FROM features.feature_snapshot_values c
             JOIN observability.scientific_snapshots s
               ON s.id=$1::uuid AND s.static_snapshot_id=c.snapshot_id
             LEFT JOIN forecast_fwi f
               ON f.h3 = c.h3 AND f.horizon = 'nowcast' AND f.computed_at = $3
             ON CONFLICT (snapshot_id, h3) DO NOTHING",
        )
        .bind(manifest_id)
        .bind(valid_at)
        .bind(latest_computed_at)
        .execute(&mut *transaction)
        .await?;

        let (cell_count_present, missing_count, unexpected_missing_count, checksum): (
            i64,
            i64,
            i64,
            String,
        ) = sqlx::query_as(
            "SELECT
                COUNT(*) FILTER (WHERE data_status = 'observed'),
                COUNT(*) FILTER (WHERE data_status <> 'observed'),
                COUNT(*) FILTER (WHERE data_status <> 'observed' AND EXISTS (
                    SELECT 1 FROM observability.coverage_mask_cells cm
                    JOIN observability.scientific_snapshots s ON s.coverage_mask_id=cm.mask_id
                    WHERE s.id=$1::uuid AND cm.h3=v.h3 AND cm.eligibility='modelable'
                )),
                COALESCE(encode(digest(string_agg(
                    h3::text || ':' || coalesce(fwi::text, 'null') || ':' || data_status,
                    ',' ORDER BY h3
                ), 'sha256'), 'hex'), '')
             FROM observability.scientific_snapshot_values v
             WHERE snapshot_id = $1::uuid",
        )
        .bind(manifest_id)
        .fetch_one(&mut *transaction)
        .await?;

        sqlx::query(
            "UPDATE observability.scientific_snapshots
             SET cell_count_present = $2, missing_count = $3,
                 unexpected_missing_count=$4, complete = ($4 = 0), checksum = $5,
                 completeness_status=CASE WHEN $4=0 THEN 'complete' ELSE 'failed_validation' END,
                 metadata=metadata || jsonb_build_object(
                    'coverage_semantics','complete means no unexpected missing cells; structural exclusions remain explicit'),
                 status = CASE WHEN $4=0 THEN 'validated' ELSE 'failed' END
             WHERE id = $1::uuid AND status <> 'published'",
        )
        .bind(manifest_id)
        .bind(cell_count_present)
        .bind(missing_count)
        .bind(unexpected_missing_count)
        .bind(&checksum)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            "INSERT INTO observability.scientific_snapshot_missing_reasons
                (snapshot_id,reason,cell_count,classification_version)
             SELECT id,'outside_operational_aoi',structural_exclusion_count,'coverage-v1'
             FROM observability.scientific_snapshots WHERE id=$1::uuid AND structural_exclusion_count>0
             ON CONFLICT(snapshot_id,reason) DO NOTHING",
        )
        .bind(manifest_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO observability.scientific_snapshot_missing_reasons
                (snapshot_id,reason,cell_count,classification_version)
             VALUES($1::uuid,'unexpected_missing',$2,'coverage-v1')
             ON CONFLICT(snapshot_id,reason) DO NOTHING",
        )
        .bind(manifest_id)
        .bind(unexpected_missing_count)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            "UPDATE observability.scientific_snapshots
             SET status = 'published', published_at = NOW()
             WHERE id = $1::uuid AND status = 'validated' AND checksum IS NOT NULL",
        )
        .bind(manifest_id)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;

        if unexpected_missing_count > 0 {
            return Err(StoreError::SnapshotContract(format!(
                "{unexpected_missing_count} unexpected modelable cells are missing"
            )));
        }

        self.get_scientific_snapshot(manifest_id)
            .await?
            .ok_or_else(|| StoreError::InvalidPersistedCount(0))
    }

    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn get_scientific_snapshot(
        &self,
        id: &str,
    ) -> Result<Option<ScientificSnapshotRow>, StoreError> {
        let row = sqlx::query_as(
            "SELECT id::text, logical_id, family, snapshot_type, resolution_h3, valid_at,
                    captured_at,source_period_start,source_period_end,application_revision,
                    feature_schema_version,transform_version,static_snapshot_id::text,
                    pipeline_run_id::text,
                    cell_count_expected, cell_count_present, complete, missing_count,
                    checksum, storage_kind, storage_location, status, temporal_classification,
                    published_at,contract_version,traceability_status,environment,
                    application_image,application_image_digest,forecast_batch_computed_at,
                    forecast_valid_at,forecast_horizon,coverage_mask_id::text,
                    modelable_cell_count,structural_exclusion_count,unexpected_missing_count,
                    completeness_status
             FROM observability.scientific_snapshots WHERE id = $1::uuid",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn list_scientific_snapshots(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ScientificSnapshotRow>, StoreError> {
        let rows = sqlx::query_as(
            "SELECT id::text, logical_id, family, snapshot_type, resolution_h3, valid_at,
                    captured_at,source_period_start,source_period_end,application_revision,
                    feature_schema_version,transform_version,static_snapshot_id::text,
                    pipeline_run_id::text,
                    cell_count_expected, cell_count_present, complete, missing_count,
                    checksum, storage_kind, storage_location, status, temporal_classification,
                    published_at,contract_version,traceability_status,environment,
                    application_image,application_image_digest,forecast_batch_computed_at,
                    forecast_valid_at,forecast_horizon,coverage_mask_id::text,
                    modelable_cell_count,structural_exclusion_count,unexpected_missing_count,
                    completeness_status
             FROM observability.scientific_snapshots
             ORDER BY captured_at DESC
             LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Recomputes checksum, uniqueness, provenance, and v2 coverage. Legacy
    /// manifests remain verifiable with explicit warnings rather than being
    /// falsely upgraded to the new contract.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest or its values cannot be read.
    #[allow(clippy::too_many_lines)]
    pub async fn verify_scientific_snapshot(
        &self,
        id: &str,
    ) -> Result<ScientificSnapshotVerification, StoreError> {
        let snapshot = self
            .get_scientific_snapshot(id)
            .await?
            .ok_or_else(|| StoreError::SnapshotContract(format!("snapshot {id} not found")))?;
        let (rows, distinct_h3, recomputed, dense_order_valid): (i64, i64, Option<String>, bool) =
            if snapshot.snapshot_type == "daily_dense" {
                let dense: Option<(i64, String, bool)> = sqlx::query_as(
                    "SELECT d.h3_count,encode(digest(
                        ffmc_values||dmc_values||dc_values||isi_values||bui_values||fwi_values,
                        'sha256'),'hex'),d.h3_order_checksum=m.source_checksum
                     FROM observability.scientific_dense_archives d
                     JOIN observability.coverage_masks m ON m.id=d.coverage_mask_id
                     WHERE d.snapshot_id=$1::uuid",
                )
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
                dense.map_or((0, 0, None, false), |(count, checksum, order_valid)| {
                    (count, count, Some(checksum), order_valid)
                })
            } else {
                let (rows, distinct_h3, checksum) = sqlx::query_as(
                    "SELECT COUNT(*),COUNT(DISTINCT h3),encode(digest(string_agg(
                        h3::text||':'||coalesce(fwi::text,'null')||':'||data_status,
                        ',' ORDER BY h3),'sha256'),'hex')
                     FROM observability.scientific_snapshot_values WHERE snapshot_id=$1::uuid",
                )
                .bind(id)
                .fetch_one(&self.pool)
                .await?;
                (rows, distinct_h3, checksum, true)
            };
        let checksum_valid = snapshot.checksum.as_deref() == recomputed.as_deref();
        let unique_h3 = rows == distinct_h3;
        let provenance_complete = snapshot.contract_version == 1
            || (snapshot.traceability_status == "complete"
                && snapshot.static_snapshot_id.is_some()
                && snapshot.application_revision.is_some()
                && snapshot.application_image.is_some()
                && snapshot.application_image_digest.is_some()
                && snapshot.forecast_batch_computed_at.is_some()
                && snapshot.coverage_mask_id.is_some()
                && snapshot.pipeline_run_id.is_some());
        let observed_modelable: i64 = if snapshot.snapshot_type == "daily_dense" {
            rows
        } else if snapshot.contract_version == 2 {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM observability.scientific_snapshot_values v
                 JOIN observability.scientific_snapshots s ON s.id=v.snapshot_id
                 JOIN observability.coverage_mask_cells c
                   ON c.mask_id=s.coverage_mask_id AND c.h3=v.h3 AND c.eligibility='modelable'
                 WHERE v.snapshot_id=$1::uuid AND v.data_status='observed'",
            )
            .bind(id)
            .fetch_one(&self.pool)
            .await?
        } else {
            snapshot.cell_count_present
        };
        let coverage_consistent = snapshot.contract_version == 1
            || snapshot.modelable_cell_count.is_some_and(|expected| {
                expected == observed_modelable + snapshot.unexpected_missing_count
                    && snapshot.cell_count_expected
                        == expected + snapshot.structural_exclusion_count
            });
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        if snapshot.status != "published" {
            errors.push(format!("status is {}, expected published", snapshot.status));
        }
        if !checksum_valid {
            errors.push("checksum mismatch".to_owned());
        }
        if !unique_h3 {
            errors.push("duplicate H3 values".to_owned());
        }
        if snapshot.contract_version == 1 {
            warnings.push(
                "legacy snapshot: provenance and coverage contract are incomplete".to_owned(),
            );
        } else {
            if !provenance_complete {
                errors.push("mandatory v2 provenance is incomplete".to_owned());
            }
            if !coverage_consistent {
                errors.push("coverage denominator is inconsistent".to_owned());
            }
            if !dense_order_valid {
                errors.push("dense archive H3 order does not match its coverage mask".to_owned());
            }
            if snapshot.unexpected_missing_count > 0 {
                errors.push(format!(
                    "{} unexpected cells are missing",
                    snapshot.unexpected_missing_count
                ));
            }
            if snapshot.completeness_status != "complete" {
                errors.push(format!(
                    "completeness status is {}",
                    snapshot.completeness_status
                ));
            }
        }
        Ok(ScientificSnapshotVerification {
            snapshot_id: id.to_owned(),
            mode: if snapshot.contract_version == 1 {
                "legacy"
            } else {
                "strict-v2"
            },
            valid: errors.is_empty(),
            checksum_valid,
            unique_h3,
            provenance_complete,
            coverage_consistent,
            errors,
            warnings,
        })
    }

    /// Links a later-known BDIFF cause to a published scientific
    /// snapshot. Never mutates the snapshot itself. `label_class` must
    /// be one of `fire.ignition_events.cause_category`'s values or
    /// `no_event`; an unknown/indeterminate cause is never treated as a
    /// negative by this call or by any downstream reader.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    #[allow(clippy::too_many_arguments)]
    pub async fn link_snapshot_label(
        &self,
        snapshot_id: &str,
        ignition_event_id: Option<&str>,
        h3: i64,
        event_date: Option<NaiveDate>,
        label_class: &str,
        label_confidence: Option<f32>,
        matching_rule_version: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO ml.snapshot_label_links
                (snapshot_id, ignition_event_id, h3, event_date, label_class,
                 label_confidence, matching_rule_version)
             VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7)
             ON CONFLICT (snapshot_id, ignition_event_id)
             WHERE is_current AND ignition_event_id IS NOT NULL DO NOTHING",
        )
        .bind(snapshot_id)
        .bind(ignition_event_id)
        .bind(h3)
        .bind(event_date)
        .bind(label_class)
        .bind(label_confidence)
        .bind(matching_rule_version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Links only mature BDIFF events in the snapshot's outcome window
    /// (one day for dense daily archives, seven days for weekly snapshots)
    /// and exact H3 cell. FIRMS and synthetic negatives are never read.
    /// A dry run performs no write; changed causes supersede rather than erase
    /// prior link history.
    ///
    /// # Errors
    ///
    /// Returns an error when candidate selection or a versioned write fails.
    #[allow(clippy::too_many_lines)]
    pub async fn link_mature_snapshot_labels(
        &self,
        snapshot_id: &str,
        mature_before: DateTime<Utc>,
        dry_run: bool,
        limit: i64,
    ) -> Result<DeferredLabelLinkReport, StoreError> {
        const WEEKLY_RULE: &str = "bdiff-exact-h3-week-v1";
        const DAILY_RULE: &str = "bdiff-exact-h3-day-v1";
        let snapshot = self
            .get_scientific_snapshot(snapshot_id)
            .await?
            .ok_or_else(|| {
                StoreError::SnapshotContract(format!("snapshot {snapshot_id} not found"))
            })?;
        let rule = if snapshot.snapshot_type == "daily_dense" {
            DAILY_RULE
        } else {
            WEEKLY_RULE
        };
        let examined: Vec<MatureLabelCandidate> = sqlx::query_as(
            "SELECT e.id::text,e.h3,e.occurred_on_local,e.cause_category,
                        e.taxonomy_version,e.updated_at,
                        (EXISTS(SELECT 1 FROM observability.scientific_snapshot_values v
                                WHERE v.snapshot_id=s.id AND v.h3=e.h3)
                         OR (s.snapshot_type='daily_dense' AND EXISTS(
                                SELECT 1 FROM observability.coverage_mask_cells c
                                WHERE c.mask_id=s.coverage_mask_id AND c.h3=e.h3
                                  AND c.eligibility='modelable'))) AS h3_found
                 FROM fire.ignition_events e
                 JOIN observability.scientific_snapshots s ON s.id=$1::uuid
                 WHERE s.status='published' AND e.is_active
                   AND e.occurred_at >= s.valid_at
                   AND e.occurred_at < s.valid_at + CASE
                        WHEN s.snapshot_type='daily_dense' THEN interval '1 day'
                        ELSE interval '7 days' END
                   AND e.updated_at <= $2
                 ORDER BY e.id LIMIT $3",
        )
        .bind(snapshot_id)
        .bind(mature_before)
        .bind(limit.clamp(1, 10_000))
        .fetch_all(&self.pool)
        .await?;
        let eligible_events = i64::try_from(examined.len()).unwrap_or(i64::MAX);
        let count = |predicate: &dyn Fn(&MatureLabelCandidate) -> bool| {
            i64::try_from(examined.iter().filter(|row| predicate(row)).count()).unwrap_or(i64::MAX)
        };
        let human_known = count(&|row| row.3 == "human_known");
        let natural_known = count(&|row| row.3 == "natural_known");
        let unknown_or_indeterminate =
            count(&|row| matches!(row.3.as_str(), "unknown" | "indeterminate"));
        let h3_found = count(&|row| row.6);
        let h3_absent = eligible_events - h3_found;
        let proposed_links = h3_found;
        if dry_run {
            return Ok(DeferredLabelLinkReport {
                snapshot_id: snapshot_id.to_owned(),
                dry_run,
                eligible_events,
                human_known,
                natural_known,
                unknown_or_indeterminate,
                h3_found,
                h3_absent,
                proposed_links,
                excluded_from_supervised_labels: unknown_or_indeterminate + h3_absent,
                inserted_links: 0,
                superseded_links: 0,
                rule_version: rule,
            });
        }

        let mut transaction = self.pool.begin().await?;
        let mut inserted_links = 0_i64;
        let mut superseded_links = 0_i64;
        for (event_id, h3, event_date, label_class, cause_version, observed_at, h3_exists) in
            examined
        {
            if !h3_exists {
                continue;
            }
            let current: Option<(i64, String, Option<String>)> = sqlx::query_as(
                "SELECT id,label_class,cause_version FROM ml.snapshot_label_links
                 WHERE snapshot_id=$1::uuid AND ignition_event_id=$2::uuid AND is_current
                 FOR UPDATE",
            )
            .bind(snapshot_id)
            .bind(&event_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if current.as_ref().is_some_and(|(_, old_label, old_version)| {
                old_label == &label_class && old_version.as_deref() == Some(&cause_version)
            }) {
                continue;
            }
            let supersedes = current.as_ref().map(|(id, _, _)| *id);
            if let Some(old_id) = supersedes {
                sqlx::query(
                    "UPDATE ml.snapshot_label_links
                     SET is_current=false,maturity_status='superseded' WHERE id=$1",
                )
                .bind(old_id)
                .execute(&mut *transaction)
                .await?;
                superseded_links += 1;
            }
            sqlx::query(
                "INSERT INTO ml.snapshot_label_links (
                    snapshot_id,ignition_event_id,h3,event_date,label_class,
                    cause_version,cause_observed_at,matched_at,matching_rule_version,
                    maturity_status,is_current,supersedes_link_id,metadata)
                 VALUES ($1::uuid,$2::uuid,$3,$4,$5,$6,$7,NOW(),$8,
                         'mature',true,$9,jsonb_build_object('source','BDIFF'))",
            )
            .bind(snapshot_id)
            .bind(&event_id)
            .bind(h3)
            .bind(event_date)
            .bind(label_class)
            .bind(cause_version)
            .bind(observed_at)
            .bind(rule)
            .bind(supersedes)
            .execute(&mut *transaction)
            .await?;
            inserted_links += 1;
        }
        transaction.commit().await?;
        Ok(DeferredLabelLinkReport {
            snapshot_id: snapshot_id.to_owned(),
            dry_run,
            eligible_events,
            human_known,
            natural_known,
            unknown_or_indeterminate,
            h3_found,
            h3_absent,
            proposed_links,
            excluded_from_supervised_labels: unknown_or_indeterminate + h3_absent,
            inserted_links,
            superseded_links,
            rule_version: rule,
        })
    }
}

fn compare_fields(
    latest: &SystemSnapshotRow,
    previous: &SystemSnapshotRow,
) -> Vec<ComparisonEntry> {
    let mut entries = Vec::new();
    let mut push = |metric: &str, current: Option<i64>, prior: Option<i64>| {
        let absolute_delta = match (current, prior) {
            (Some(c), Some(p)) => Some(c - p),
            _ => None,
        };
        let relative_delta = match (absolute_delta, prior) {
            // Metric magnitudes stay well within f64's 52-bit mantissa
            // (operational counts, not raw 64-bit identifiers), so the
            // conversion is exact in practice.
            #[allow(clippy::cast_precision_loss)]
            (Some(delta), Some(p)) if p != 0 => Some((delta as f64) / (p as f64) * 100.0),
            _ => None,
        };
        let status = match absolute_delta {
            None => "not_comparable",
            Some(0) => "unchanged",
            Some(d) if d > 0 => "up",
            Some(_) => "down",
        };
        entries.push(ComparisonEntry {
            metric: metric.to_owned(),
            current_value: current,
            previous_value: prior,
            absolute_delta,
            relative_delta,
            status,
        });
    };

    push(
        "forecast_age_seconds",
        latest.forecast_age_seconds,
        previous.forecast_age_seconds,
    );
    push(
        "firms_age_seconds",
        latest.firms_age_seconds,
        previous.firms_age_seconds,
    );
    push(
        "firms_observation_count",
        latest.firms_observation_count,
        previous.firms_observation_count,
    );
    push(
        "import_batches_success_24h",
        latest.import_batches_success_24h,
        previous.import_batches_success_24h,
    );
    push(
        "import_batches_failed_24h",
        latest.import_batches_failed_24h,
        previous.import_batches_failed_24h,
    );
    push(
        "pipeline_runs_failed_24h",
        latest.pipeline_runs_failed_24h,
        previous.pipeline_runs_failed_24h,
    );
    push(
        "error_count_24h",
        latest.error_count_24h,
        previous.error_count_24h,
    );
    push(
        "warning_count_24h",
        latest.warning_count_24h,
        previous.warning_count_24h,
    );
    push(
        "static_cell_count",
        latest.static_cell_count,
        previous.static_cell_count,
    );
    push(
        "feature_snapshot_count",
        latest.feature_snapshot_count.map(i64::from),
        previous.feature_snapshot_count.map(i64::from),
    );
    push(
        "dataset_version_count",
        latest.dataset_version_count.map(i64::from),
        previous.dataset_version_count.map(i64::from),
    );
    push(
        "application_restart_count",
        latest.application_restart_count,
        previous.application_restart_count,
    );
    entries
}
