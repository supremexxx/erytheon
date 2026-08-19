//! Integration tests for the public wildfire-risk map's small API
//! surface (`/api/watch/*` -- everything else Watch consumes is the
//! existing unconditional `/risk`, `/risk/cell/{h3}`, `/sources` and
//! `/config` routes, already covered elsewhere). Focused on: the
//! deployment gate, commune name-search, and the commune bbox lookup
//! that backs "select a commune, pan the map".

use api::AppState;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use store::Store;
use tokio::sync::broadcast;
use tower::ServiceExt;

const TEST_INSEE_CODE: &str = "00001";
const TEST_COMMUNE_NAME: &str = "Testwatchville";
const SQUARE_GEOMETRY: &str = r#"{"type":"Polygon","coordinates":[[[1.35,43.75],[1.40,43.75],[1.40,43.80],[1.35,43.80],[1.35,43.75]]]}"#;

async fn connect() -> Option<Store> {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping watch console integration test: DATABASE_URL is not configured");
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
            TEST_COMMUNE_NAME,
            &["00001".to_owned()],
            &geometry,
        )
        .await
        .expect("seed test commune boundary");
}

fn build_app(store: Store) -> axum::Router {
    let grid = grid::H3Grid::new(9).expect("valid grid");
    let (updates, _) = broadcast::channel(1);
    api::router(AppState::new(store, grid, updates).with_watch_console_enabled(true))
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
/// `/watch*` or `/api/watch/*` routes should exist at all -- proves the
/// deployment gate actually gates something.
#[tokio::test]
async fn watch_routes_absent_when_console_disabled() {
    let Some(store) = connect().await else {
        return;
    };
    let grid = grid::H3Grid::new(9).expect("valid grid");
    let (updates, _) = broadcast::channel(1);
    let app = api::router(AppState::new(store, grid, updates));

    for uri in [
        "/watch",
        "/watch.css",
        "/watch.js",
        "/api/watch/communes?q=te",
        "/api/watch/communes/00001",
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
async fn commune_search_finds_seeded_commune_by_prefix() {
    let Some(store) = connect().await else {
        return;
    };
    seed_test_commune(&store).await;
    let app = build_app(store);

    let (status, body) = get_json(&app, "/api/watch/communes?q=Testwatch").await;
    assert_eq!(status, StatusCode::OK);
    let results = body.as_array().expect("results array");
    assert!(
        results
            .iter()
            .any(|entry| entry["insee_code"] == json!(TEST_INSEE_CODE)
                && entry["name"] == json!(TEST_COMMUNE_NAME)),
        "expected {TEST_COMMUNE_NAME} in search results: {body}"
    );
}

#[tokio::test]
async fn commune_search_rejects_short_query() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, body) = get_json(&app, "/api/watch/communes?q=a").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], json!("query_too_short"));
}

#[tokio::test]
async fn commune_lookup_returns_bbox_for_registered_commune() {
    let Some(store) = connect().await else {
        return;
    };
    seed_test_commune(&store).await;
    let app = build_app(store);

    let (status, body) = get_json(&app, &format!("/api/watch/communes/{TEST_INSEE_CODE}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["insee_code"], json!(TEST_INSEE_CODE));
    assert_eq!(body["name"], json!(TEST_COMMUNE_NAME));
    let bbox = body["bbox"].as_array().expect("bbox array");
    assert_eq!(bbox.len(), 4);
}

#[tokio::test]
async fn commune_lookup_unknown_commune_is_an_honest_404() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, body) = get_json(&app, "/api/watch/communes/97999").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], json!("commune_not_found"));
}

#[tokio::test]
async fn commune_lookup_malformed_insee_code_is_rejected() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, body) = get_json(&app, "/api/watch/communes/not-a-code").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], json!("invalid_insee_code"));
}
