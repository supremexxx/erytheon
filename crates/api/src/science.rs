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
