//! Deployment-gate and privacy checks for the read-only BLUE center.

use api::AppState;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use store::Store;
use tokio::sync::broadcast;
use tower::ServiceExt as _;

async fn connect() -> Option<Store> {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping BLUE integration test: DATABASE_URL is not configured");
        return None;
    };
    Some(
        Store::connect(&database_url)
            .await
            .expect("connect and migrate"),
    )
}

fn app(store: Store, enabled: bool) -> axum::Router {
    let grid = grid::H3Grid::new(8).expect("valid grid");
    let (updates, _) = broadcast::channel(1);
    api::router(AppState::new(store, grid, updates).with_blue_center_enabled(enabled))
}

#[tokio::test]
async fn blue_routes_are_absent_by_default() {
    let Some(store) = connect().await else {
        return;
    };
    let app = app(store, false);
    for uri in ["/blue", "/blue/forecast", "/api/blue/overview"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
}

#[tokio::test]
async fn enabled_blue_center_is_read_only_and_hides_upstream_provenance() {
    let Some(store) = connect().await else {
        return;
    };
    let app = app(store, true);
    let shell = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/blue")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(shell.status(), StatusCode::OK);
    assert_eq!(
        shell.headers().get(header::CONTENT_TYPE),
        Some(&"text/html; charset=utf-8".parse().expect("content type"))
    );

    let overview = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/blue/overview")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(overview.status(), StatusCode::OK);
    let body = to_bytes(overview.into_body(), 8_000_000)
        .await
        .expect("body");
    let text = String::from_utf8(body.to_vec()).expect("utf8 JSON");
    assert!(!text.contains("forecast_source"));

    let performance = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/blue/performance")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(performance.status(), StatusCode::OK);

    let invalid_period = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/blue/performance?from=2026-08-14&to=2026-08-01")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(invalid_period.status(), StatusCode::BAD_REQUEST);

    let mutation = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/blue/alerts")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(mutation.status(), StatusCode::METHOD_NOT_ALLOWED);
}
