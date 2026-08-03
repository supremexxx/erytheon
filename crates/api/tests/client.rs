//! Integration tests for the read-only client-facing commune console
//! API (`/api/client/*`). These tests exercise the real database
//! (skipped, not failed, when `DATABASE_URL` is unset) and focus on:
//! the deployment gate, generic INSEE-code lookup, honest handling of
//! an unregistered commune, and read-only behavior.

use api::AppState;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use store::Store;
use tokio::sync::broadcast;
use tower::ServiceExt;

const TEST_INSEE_CODE: &str = "00000";
const SQUARE_GEOMETRY: &str = r#"{"type":"Polygon","coordinates":[[[1.35,43.75],[1.40,43.75],[1.40,43.80],[1.35,43.80],[1.35,43.75]]]}"#;

async fn connect() -> Option<Store> {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping client console integration test: DATABASE_URL is not configured");
        return None;
    };
    Some(
        Store::connect(&database_url)
            .await
            .expect("database should accept connections and migrations"),
    )
}

async fn seed_test_commune(store: &Store) {
    let geometry: Value = serde_json::from_str(SQUARE_GEOMETRY).expect("valid geometry fixture");
    store
        .upsert_commune_boundary(
            TEST_INSEE_CODE,
            "Testville",
            &["00000".to_owned()],
            &geometry,
        )
        .await
        .expect("seed test commune boundary");
}

fn build_app(store: Store) -> axum::Router {
    let grid = grid::H3Grid::new(9).expect("valid grid");
    let (updates, _) = broadcast::channel(1);
    api::router(AppState::new(store, grid, updates).with_client_console_enabled(true))
}

async fn get_json(app: &axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("request should be valid"),
        )
        .await
        .expect("request should complete");
    let status = response.status();
    let body = to_bytes(response.into_body(), 8_000_000)
        .await
        .expect("body should be readable");
    let json = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).expect("body should be valid JSON")
    };
    (status, json)
}

/// When the console is disabled (the production default), none of the
/// `/api/client/*` or `/client*` routes should exist at all -- proves
/// the deployment gate actually gates something.
#[tokio::test]
async fn client_routes_absent_when_console_disabled() {
    let Some(store) = connect().await else {
        return;
    };
    let grid = grid::H3Grid::new(9).expect("valid grid");
    let (updates, _) = broadcast::channel(1);
    let app = api::router(AppState::new(store, grid, updates));

    for uri in [
        "/client",
        "/client/00000",
        "/api/client/communes/00000",
        "/api/client/communes/00000/risk",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request should be valid"),
            )
            .await
            .expect("request should complete");
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{uri} should not be routable when the console is disabled"
        );
    }
}

#[tokio::test]
async fn unknown_commune_is_a_honest_404() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, body) = get_json(&app, "/api/client/communes/97999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], json!("commune_not_found"));
}

#[tokio::test]
async fn malformed_insee_code_is_rejected() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, body) = get_json(&app, "/api/client/communes/not-a-code").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], json!("invalid_insee_code"));
}

#[tokio::test]
async fn registered_commune_returns_its_boundary() {
    let Some(store) = connect().await else {
        return;
    };
    seed_test_commune(&store).await;
    let app = build_app(store);

    let (status, body) = get_json(&app, &format!("/api/client/communes/{TEST_INSEE_CODE}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["insee_code"], json!(TEST_INSEE_CODE));
    assert_eq!(body["name"], json!("Testville"));
    assert_eq!(body["postal_codes"], json!(["00000"]));
    assert_eq!(body["boundary"]["type"], json!("Polygon"));
    let bbox = body["bbox"].as_array().expect("bbox array");
    assert_eq!(bbox.len(), 4);
}

#[tokio::test]
async fn commune_risk_cells_stay_inside_the_commune_bbox() {
    let Some(store) = connect().await else {
        return;
    };
    seed_test_commune(&store).await;
    let app = build_app(store);

    let (status, body) = get_json(
        &app,
        &format!("/api/client/communes/{TEST_INSEE_CODE}/risk"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["type"], json!("FeatureCollection"));
    assert_eq!(body["insee_code"], json!(TEST_INSEE_CODE));

    // The test fixture database may have no scored cells at all for the
    // commune's square; an empty, well-formed FeatureCollection is the
    // honest result and must not be presented as an error.
    let features = body["features"].as_array().expect("features array");
    for feature in features {
        let coordinates = &feature["geometry"]["coordinates"][0];
        for point in coordinates.as_array().expect("ring") {
            let lng = point[0].as_f64().expect("lng");
            let lat = point[1].as_f64().expect("lat");
            assert!(
                (1.30..=1.45).contains(&lng) && (43.70..=43.85).contains(&lat),
                "cell at ({lng}, {lat}) falls well outside the seeded commune square"
            );
        }
    }
}
