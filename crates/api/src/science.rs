//! Phase 4A: read-only scientific console API. Every handler here is
//! `GET`-only, uses parameterized queries (via `store::Store`), and
//! never activates the candidate, never triggers a migration, never
//! computes a score, and never writes anything.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{ApiError, AppState};

const MAX_PAGE_SIZE: i64 = 200;
const DEFAULT_PAGE_SIZE: i64 = 50;

/// Builds the `/api/science/*` sub-router. The caller decides whether
/// to nest this at all (phase 4A: only when `AppState::science_
/// console_enabled` is true).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/overview", get(overview))
        .route("/progress", get(progress))
        .route("/sources", get(sources))
        .route("/imports", get(imports))
        .route("/pipelines", get(pipelines))
        .route("/data-quality", get(data_quality))
        .route("/data-quality/events", get(data_quality_events))
        .route("/features", get(features))
        .route("/calendar", get(calendar))
        .route("/datasets", get(datasets))
        .route("/datasets/{logical_id}", get(dataset_detail))
        .route("/models", get(models))
        .route("/system", get(system))
        .route("/observability/latest", get(observability_latest))
        .route("/observability/history", get(observability_history))
        .route("/observability/compare", get(observability_compare))
        .route("/observability/attempts", get(observability_attempts))
        .route(
            "/observability/hourly-summary",
            get(observability_hourly_summary),
        )
        .route("/snapshots", get(snapshots))
        .route("/snapshots/{id}", get(snapshot_detail))
        .route("/snapshots/{id}/verification", get(snapshot_verification))
        .route("/snapshot-labels/summary", get(snapshot_label_summary))
        .route("/snapshot-alerts", get(snapshot_alerts))
}

const DEFAULT_ENVIRONMENT: &str = "default";
const MAX_HISTORY_DAYS: i64 = 366;
const DEFAULT_HISTORY_DAYS: i64 = 30;

#[derive(Debug, Deserialize)]
struct ObservabilityQuery {
    environment: Option<String>,
    cadence: Option<String>,
}

async fn observability_latest(
    State(state): State<AppState>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<store::SystemSnapshotRow>, ApiError> {
    let environment = query.environment.as_deref().unwrap_or(DEFAULT_ENVIRONMENT);
    let cadence = query.cadence.as_deref().unwrap_or("daily");
    state
        .store()
        .latest_system_snapshot(environment, cadence)
        .await
        .map_err(database_error)?
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found(
                "no_operational_snapshot",
                "no operational snapshot has been captured yet",
            )
        })
}

#[derive(Debug, Deserialize)]
struct ObservabilityHistoryQuery {
    environment: Option<String>,
    cadence: Option<String>,
    days: Option<i64>,
}

async fn observability_history(
    State(state): State<AppState>,
    Query(query): Query<ObservabilityHistoryQuery>,
) -> Result<Json<Vec<store::SystemSnapshotRow>>, ApiError> {
    let environment = query.environment.as_deref().unwrap_or(DEFAULT_ENVIRONMENT);
    let cadence = query.cadence.as_deref().unwrap_or("daily");
    let days = query
        .days
        .unwrap_or(DEFAULT_HISTORY_DAYS)
        .clamp(1, MAX_HISTORY_DAYS);
    Ok(Json(
        state
            .store()
            .system_snapshot_history(environment, cadence, days)
            .await
            .map_err(database_error)?,
    ))
}

async fn observability_attempts(
    State(state): State<AppState>,
    Query(query): Query<ObservabilityHistoryQuery>,
) -> Result<Json<Vec<store::SnapshotCaptureAttemptRow>>, ApiError> {
    let environment = query.environment.as_deref().unwrap_or(DEFAULT_ENVIRONMENT);
    let limit = query.days.unwrap_or(50).clamp(1, 200);
    Ok(Json(
        state
            .store()
            .list_snapshot_capture_attempts(environment, limit)
            .await
            .map_err(database_error)?,
    ))
}

async fn observability_hourly_summary(
    State(state): State<AppState>,
    Query(query): Query<ObservabilityQuery>,
) -> Result<Json<store::HourlySnapshotSummary>, ApiError> {
    let environment = query.environment.as_deref().unwrap_or(DEFAULT_ENVIRONMENT);
    Ok(Json(
        state
            .store()
            .hourly_snapshot_summary(environment)
            .await
            .map_err(database_error)?,
    ))
}

#[derive(Debug, Deserialize)]
struct ObservabilityCompareQuery {
    environment: Option<String>,
    /// Comma-separated list of J-N offsets, e.g. `1,7`.
    days: Option<String>,
}

#[derive(Debug, Serialize)]
struct ComparisonReport {
    days_ago: i64,
    available: bool,
    entries: Vec<store::ComparisonEntry>,
}

async fn observability_compare(
    State(state): State<AppState>,
    Query(query): Query<ObservabilityCompareQuery>,
) -> Result<Json<Vec<ComparisonReport>>, ApiError> {
    let environment = query.environment.as_deref().unwrap_or(DEFAULT_ENVIRONMENT);
    let raw_days = query.days.as_deref().unwrap_or("1,7");
    let mut offsets = Vec::new();
    for part in raw_days.split(',') {
        let offset: i64 = part.trim().parse().map_err(|_| {
            ApiError::bad_request(
                "invalid_days",
                "days must be a comma-separated list of positive integers, e.g. 1,7",
            )
        })?;
        if !(1..=MAX_HISTORY_DAYS).contains(&offset) {
            return Err(ApiError::bad_request(
                "invalid_days",
                "each day offset must be between 1 and 366",
            ));
        }
        offsets.push(offset);
    }
    let mut reports = Vec::with_capacity(offsets.len());
    for days_ago in offsets {
        let comparison = state
            .store()
            .compare_system_snapshots(environment, days_ago)
            .await
            .map_err(database_error)?;
        reports.push(ComparisonReport {
            days_ago,
            available: comparison.is_some(),
            entries: comparison.unwrap_or_default(),
        });
    }
    Ok(Json(reports))
}

async fn snapshots(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Vec<store::ScientificSnapshotRow>>, ApiError> {
    let (limit, offset) = page_params(&query);
    Ok(Json(
        state
            .store()
            .list_scientific_snapshots(limit, offset)
            .await
            .map_err(database_error)?,
    ))
}

async fn snapshot_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<store::ScientificSnapshotRow>, ApiError> {
    state
        .store()
        .get_scientific_snapshot(&id)
        .await
        .map_err(database_error)?
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found("snapshot_not_found", "no scientific snapshot with this id")
        })
}

async fn snapshot_verification(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<store::ScientificSnapshotVerification>, ApiError> {
    Ok(Json(
        state
            .store()
            .verify_scientific_snapshot(&id)
            .await
            .map_err(database_error)?,
    ))
}

async fn snapshot_label_summary(
    State(state): State<AppState>,
) -> Result<Json<store::DeferredLabelSummary>, ApiError> {
    Ok(Json(
        state
            .store()
            .deferred_label_summary()
            .await
            .map_err(database_error)?,
    ))
}

#[derive(Debug, Deserialize)]
struct AlertsQuery {
    severity: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn snapshot_alerts(
    State(state): State<AppState>,
    Query(query): Query<AlertsQuery>,
) -> Result<Json<Vec<store::SnapshotAlertRow>>, ApiError> {
    let (limit, offset) = page_params(&PageQuery {
        limit: query.limit,
        offset: query.offset,
    });
    if let Some(severity) = query.severity.as_deref()
        && !matches!(severity, "info" | "warning" | "critical")
    {
        return Err(ApiError::bad_request(
            "invalid_severity",
            "severity must be info, warning, or critical",
        ));
    }
    Ok(Json(
        state
            .store()
            .list_snapshot_alerts(query.severity.as_deref(), limit, offset)
            .await
            .map_err(database_error)?,
    ))
}

fn database_error(error: store::StoreError) -> ApiError {
    tracing::error!(%error, "scientific console database operation failed");
    drop(error);
    ApiError::service_unavailable("database_unavailable", "database operation failed")
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

fn page_params(query: &PageQuery) -> (i64, i64) {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = query.offset.unwrap_or(0).max(0);
    (limit, offset)
}

async fn overview(State(state): State<AppState>) -> Result<Json<store::ScienceOverview>, ApiError> {
    Ok(Json(
        state
            .store()
            .science_overview()
            .await
            .map_err(database_error)?,
    ))
}

/// Static, version-controlled phase history (mission section 10: no
/// dedicated database table exists for this yet, and back-filling
/// fictitious timestamps for already-completed phases was rejected).
async fn progress() -> Json<serde_json::Value> {
    const PHASES_JSON: &str = include_str!("../static/science/phases.json");
    Json(serde_json::from_str(PHASES_JSON).unwrap_or_else(|_| json!([])))
}

async fn sources(
    State(state): State<AppState>,
) -> Result<Json<Vec<store::SourceOverviewRow>>, ApiError> {
    Ok(Json(
        state
            .store()
            .science_sources()
            .await
            .map_err(database_error)?,
    ))
}

#[derive(Debug, Deserialize)]
struct ImportsQuery {
    source: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn imports(
    State(state): State<AppState>,
    Query(query): Query<ImportsQuery>,
) -> Result<Json<Vec<store::ImportBatchRow>>, ApiError> {
    let (limit, offset) = page_params(&PageQuery {
        limit: query.limit,
        offset: query.offset,
    });
    Ok(Json(
        state
            .store()
            .science_import_batches(
                query.source.as_deref(),
                query.status.as_deref(),
                limit,
                offset,
            )
            .await
            .map_err(database_error)?,
    ))
}

#[derive(Debug, Deserialize)]
struct PipelinesQuery {
    pipeline: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn pipelines(
    State(state): State<AppState>,
    Query(query): Query<PipelinesQuery>,
) -> Result<Json<Vec<store::PipelineRunRow>>, ApiError> {
    let (limit, offset) = page_params(&PageQuery {
        limit: query.limit,
        offset: query.offset,
    });
    Ok(Json(
        state
            .store()
            .science_pipeline_runs(
                query.pipeline.as_deref(),
                query.status.as_deref(),
                limit,
                offset,
            )
            .await
            .map_err(database_error)?,
    ))
}

async fn data_quality(
    State(state): State<AppState>,
) -> Result<Json<store::DataQualitySummary>, ApiError> {
    Ok(Json(
        state
            .store()
            .science_data_quality_summary()
            .await
            .map_err(database_error)?,
    ))
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    cause: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn data_quality_events(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Vec<store::IgnitionEventExplorationRow>>, ApiError> {
    let (limit, offset) = page_params(&PageQuery {
        limit: query.limit,
        offset: query.offset,
    });
    Ok(Json(
        state
            .store()
            .science_ignition_events(query.cause.as_deref(), limit, offset)
            .await
            .map_err(database_error)?,
    ))
}

#[derive(Debug, Serialize)]
struct FeaturesResponse {
    snapshots: Vec<store::FeatureSnapshotRow>,
    calendar: store::CalendarSummary,
}

async fn features(State(state): State<AppState>) -> Result<Json<FeaturesResponse>, ApiError> {
    let snapshots = state
        .store()
        .science_feature_snapshots()
        .await
        .map_err(database_error)?;
    let calendar = state
        .store()
        .science_calendar_summary()
        .await
        .map_err(database_error)?;
    Ok(Json(FeaturesResponse {
        snapshots,
        calendar,
    }))
}

async fn calendar(State(state): State<AppState>) -> Result<Json<store::CalendarSummary>, ApiError> {
    Ok(Json(
        state
            .store()
            .science_calendar_summary()
            .await
            .map_err(database_error)?,
    ))
}

async fn datasets(
    State(state): State<AppState>,
) -> Result<Json<Vec<store::DatasetVersionSummaryRow>>, ApiError> {
    Ok(Json(
        state
            .store()
            .science_dataset_versions()
            .await
            .map_err(database_error)?,
    ))
}

async fn dataset_detail(
    State(state): State<AppState>,
    Path(logical_id): Path<String>,
) -> Result<Json<store::DatasetDetail>, ApiError> {
    state
        .store()
        .science_dataset_detail(&logical_id)
        .await
        .map_err(database_error)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("dataset_not_found", "no dataset with this logical_id"))
}

/// The v1-vs-candidate paired comparison (AP/ROC-AUC/lift, with a
/// bootstrap confidence interval) was computed once in phase 3B.8 and
/// reported in `PHASE3B8_PROMOTION_GAP_REPORT.md` -- no database table
/// stores paired-comparison results. Exposed here as a small, explicit,
/// version-controlled constant rather than silently re-deriving or
/// inventing it. See `SCIENTIFIC_CONSOLE_DATA_CONTRACTS.md`.
fn phase_3b8_comparison() -> serde_json::Value {
    json!({
        "source": "PHASE3B8_PROMOTION_GAP_REPORT.md (phase 3B.8, not a live database query)",
        "population": {"comparable_rows": 4708, "total_rows": 4708, "comparable_fraction": 1.0},
        "v1": {"roc_auc": 0.7836, "average_precision": 0.5840, "lift_at_10pct": 2.86},
        "candidate": {"roc_auc": 0.9764, "average_precision": 0.9308, "lift_at_10pct": 3.91},
        "ap_diff_candidate_minus_v1": 0.3473,
        "ap_diff_95pct_ci": [0.3157, 0.3852],
        "promotion_stages": {"p0": true, "p1": true, "p2": true, "p3": false},
    })
}

#[derive(Debug, Serialize)]
struct ModelsResponse {
    active_v1: Option<serde_json::Value>,
    candidate: Option<store::ModelCandidateRow>,
    comparison: serde_json::Value,
    scientific_interpretation: &'static str,
}

async fn models(State(state): State<AppState>) -> Result<Json<ModelsResponse>, ApiError> {
    let active_v1 = state
        .store()
        .active_human_model()
        .await
        .map_err(database_error)?
        .map(|version| {
            json!({
                "id": version.id,
                "trained_at": version.trained_at,
                "metrics": version.metrics,
            })
        });
    let candidate = state
        .store()
        .science_latest_model_candidate()
        .await
        .map_err(database_error)?;
    Ok(Json(ModelsResponse {
        active_v1,
        candidate,
        comparison: phase_3b8_comparison(),
        scientific_interpretation: "Le score candidat est une propension relative calibrée sur la distribution échantillonnée de 2024. Ce n'est pas une probabilité absolue d'incendie.",
    }))
}

async fn system(State(state): State<AppState>) -> Result<Json<store::SystemSummary>, ApiError> {
    Ok(Json(
        state
            .store()
            .science_system_summary()
            .await
            .map_err(database_error)?,
    ))
}
