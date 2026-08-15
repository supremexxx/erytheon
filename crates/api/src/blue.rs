//! Read-only BLUE Forecast & Evidence Center API.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use store::{BlueEvidenceCaseRow, BlueForecastAlertRow, BlueForecastBulletinRow};

use crate::{ApiError, AppState, database_error};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/overview", get(overview))
        .route("/bulletins", get(bulletins))
        .route("/performance", get(performance))
        .route("/ground-truth", get(ground_truth))
        .route("/cases", get(cases))
        .route("/alerts", get(alerts))
        .route("/alerts/{id}", get(alert))
}

async fn ground_truth(
    State(state): State<AppState>,
) -> Result<Json<store::BlueGroundTruthSummary>, ApiError> {
    state
        .store()
        .blue_ground_truth_summary()
        .await
        .map(Json)
        .map_err(database_error)
}

#[derive(Serialize)]
struct BlueOverview {
    bulletin: Option<BlueForecastBulletinRow>,
    top_cases: Vec<BlueEvidenceCaseRow>,
    interpretation: &'static str,
}

async fn overview(State(state): State<AppState>) -> Result<Json<BlueOverview>, ApiError> {
    let bulletin = state
        .store()
        .latest_blue_bulletin()
        .await
        .map_err(database_error)?;
    let top_cases = if let Some(item) = &bulletin {
        state
            .store()
            .list_blue_evidence_cases(&item.id)
            .await
            .map_err(database_error)?
    } else {
        Vec::new()
    };
    Ok(Json(BlueOverview {
        bulletin,
        top_cases,
        interpretation: "Indice relatif de vigilance BLUE, pas une probabilité calibrée d'incendie.",
    }))
}

#[derive(Deserialize)]
struct CasesQuery {
    bulletin_id: Option<String>,
}

async fn cases(
    State(state): State<AppState>,
    Query(query): Query<CasesQuery>,
) -> Result<Json<Vec<BlueEvidenceCaseRow>>, ApiError> {
    let bulletin_id = if let Some(id) = query.bulletin_id {
        id
    } else {
        state
            .store()
            .latest_blue_bulletin()
            .await
            .map_err(database_error)?
            .ok_or_else(|| ApiError::not_found("no_bulletin", "no BLUE bulletin is published"))?
            .id
    };
    state
        .store()
        .list_blue_evidence_cases(&bulletin_id)
        .await
        .map(Json)
        .map_err(database_error)
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
}

async fn bulletins(
    State(state): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<BlueForecastBulletinRow>>, ApiError> {
    state
        .store()
        .list_blue_bulletins(query.limit.unwrap_or(30))
        .await
        .map(Json)
        .map_err(database_error)
}

#[derive(Deserialize)]
struct PerformanceQuery {
    from: Option<chrono::NaiveDate>,
    to: Option<chrono::NaiveDate>,
}

async fn performance(
    State(state): State<AppState>,
    Query(query): Query<PerformanceQuery>,
) -> Result<Json<store::BluePerformanceSummary>, ApiError> {
    if query.from.zip(query.to).is_some_and(|(from, to)| from > to) {
        return Err(ApiError::bad_request(
            "invalid_period",
            "from must be earlier than or equal to to",
        ));
    }
    state
        .store()
        .blue_performance_summary(query.from, query.to)
        .await
        .map(Json)
        .map_err(database_error)
}

#[derive(Deserialize)]
struct AlertsQuery {
    bulletin_id: Option<String>,
    horizon: Option<String>,
    limit: Option<i64>,
}

async fn alerts(
    State(state): State<AppState>,
    Query(query): Query<AlertsQuery>,
) -> Result<Json<Vec<BlueForecastAlertRow>>, ApiError> {
    if query
        .horizon
        .as_deref()
        .is_some_and(|value| !matches!(value, "hours_24" | "hours_48"))
    {
        return Err(ApiError::bad_request(
            "invalid_horizon",
            "horizon must be hours_24 or hours_48",
        ));
    }
    let bulletin_id = if let Some(id) = query.bulletin_id {
        id
    } else {
        state
            .store()
            .latest_blue_bulletin()
            .await
            .map_err(database_error)?
            .ok_or_else(|| ApiError::not_found("no_bulletin", "no BLUE bulletin is published"))?
            .id
    };
    state
        .store()
        .list_blue_alerts(
            &bulletin_id,
            query.horizon.as_deref(),
            query.limit.unwrap_or(500),
        )
        .await
        .map(Json)
        .map_err(database_error)
}

async fn alert(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BlueForecastAlertRow>, ApiError> {
    state
        .store()
        .blue_alert(&id)
        .await
        .map_err(database_error)?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("alert_not_found", "BLUE alert not found"))
}
