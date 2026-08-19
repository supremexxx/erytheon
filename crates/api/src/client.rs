//! Read-only client-facing commune console API. Scoped to a single
//! commune's real boundary, resolved generically by INSEE code -- no
//! commune is special-cased. Every handler here is `GET`-only and
//! never writes anything.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use geojson::{FeatureCollection, Geometry, Value};
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};

use crate::{ApiError, AppState, database_error, parse_horizon, parse_unit_interval, risk_feature};

/// Builds the `/api/client/*` sub-router. The caller decides whether to
/// nest this at all (only when `AppState::client_console_enabled` is
/// true).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/communes/{insee_code}", get(commune))
        .route("/communes/{insee_code}/risk", get(commune_risk))
}

#[derive(Debug, Serialize)]
struct CommuneResponse {
    insee_code: String,
    name: String,
    postal_codes: Vec<String>,
    bbox: [f64; 4],
    boundary: Geometry,
}

async fn commune(
    State(state): State<AppState>,
    Path(insee_code): Path<String>,
) -> Result<Json<CommuneResponse>, ApiError> {
    let insee_code = validate_insee_code(&insee_code)?;
    let boundary = state
        .store()
        .commune_boundary(&insee_code)
        .await
        .map_err(database_error)?
        .ok_or_else(|| {
            ApiError::not_found("commune_not_found", "no commune boundary is registered")
        })?;
    Ok(Json(CommuneResponse {
        insee_code: boundary.insee_code,
        name: boundary.name,
        postal_codes: boundary.postal_codes,
        bbox: [
            boundary.bbox.west,
            boundary.bbox.south,
            boundary.bbox.east,
            boundary.bbox.north,
        ],
        boundary: Geometry::new(Value::from(&boundary.geometry)),
    }))
}

#[derive(Debug, Deserialize)]
struct CommuneRiskQuery {
    min_score: Option<String>,
    horizon: Option<String>,
}

async fn commune_risk(
    State(state): State<AppState>,
    Path(insee_code): Path<String>,
    Query(query): Query<CommuneRiskQuery>,
) -> Result<Json<FeatureCollection>, ApiError> {
    let insee_code = validate_insee_code(&insee_code)?;
    let min_score = parse_unit_interval(query.min_score.as_deref(), 0.0, "min_score")?;
    let horizon = parse_horizon(query.horizon.as_deref())?;
    let boundary = state
        .store()
        .commune_boundary(&insee_code)
        .await
        .map_err(database_error)?
        .ok_or_else(|| {
            ApiError::not_found("commune_not_found", "no commune boundary is registered")
        })?;
    let cells = state
        .grid()
        .cells_for_geometry(&boundary.geometry)
        .map_err(|error| {
            tracing::error!(%error, insee_code, "failed to resolve commune H3 cells");
            ApiError::service_unavailable(
                "commune_geometry_unresolvable",
                "commune boundary could not be resolved to H3 cells",
            )
        })?;
    if cells.is_empty() {
        // A commune whose polygon contains no H3 cell centroid at the
        // configured resolution (small or narrow shapes) is a distinct
        // failure from "valid commune, no scores computed yet" -- the
        // latter still returns an empty FeatureCollection below, but
        // this case must not be silently indistinguishable from it.
        tracing::error!(insee_code, "commune boundary resolved to zero H3 cells");
        return Err(ApiError::service_unavailable(
            "commune_geometry_unresolvable",
            "commune boundary could not be resolved to H3 cells",
        ));
    }
    let scores = state
        .store()
        .latest_risk_scores(&cells, min_score, horizon)
        .await
        .map_err(database_error)?;
    let mut metadata = Map::new();
    metadata.insert("insee_code".to_owned(), json!(insee_code));
    Ok(Json(FeatureCollection {
        bbox: None,
        features: scores
            .iter()
            .map(|score| risk_feature(score, false))
            .collect(),
        foreign_members: Some(metadata),
    }))
}

/// Five-character French INSEE municipality code: either five digits,
/// or `2A`/`2B` (Corsica) followed by three digits.
pub(crate) fn validate_insee_code(raw: &str) -> Result<String, ApiError> {
    let code = raw.to_uppercase();
    let valid = code.len() == 5
        && if let Some(rest) = code.strip_prefix("2A").or_else(|| code.strip_prefix("2B")) {
            rest.bytes().all(|byte| byte.is_ascii_digit())
        } else {
            code.bytes().all(|byte| byte.is_ascii_digit())
        };
    if !valid {
        return Err(ApiError::bad_request(
            "invalid_insee_code",
            "insee_code must be five digits, or 2A/2B followed by three digits",
        ));
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::validate_insee_code;

    #[test]
    fn accepts_metropolitan_codes() {
        assert_eq!(validate_insee_code("31490").unwrap(), "31490");
    }

    #[test]
    fn accepts_corsican_codes() {
        assert_eq!(validate_insee_code("2a004").unwrap(), "2A004");
    }

    #[test]
    fn rejects_malformed_codes() {
        assert!(validate_insee_code("3144").is_err());
        assert!(validate_insee_code("abcde").is_err());
        assert!(validate_insee_code("314906").is_err());
    }
}
