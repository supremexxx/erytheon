use api::AppState;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use serde_json::json;
use store::Store;
use tokio::sync::broadcast;
use tower::ServiceExt;

#[tokio::test]
async fn dashboard_and_health_are_served() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };

    let store = Store::connect(&database_url)
        .await
        .expect("database should accept connections and migrations");
    let grid = grid::H3Grid::new(9).expect("valid grid");
    let (updates, _) = broadcast::channel(1);
    let aoi = grid::BoundingBox::new(-5.15, 41.31, 9.57, 51.09).expect("valid France bbox");
    let app = api::router(
        AppState::new(store, grid, updates).with_operational_area(aoi, "France métropolitaine"),
    );
    let dashboard = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("request should be valid"),
        )
        .await
        .expect("dashboard request should complete");
    assert_eq!(dashboard.status(), StatusCode::OK);
    assert_eq!(
        dashboard.headers().get(header::CONTENT_TYPE),
        Some(&"text/html; charset=utf-8".parse().expect("valid header"))
    );
    let dashboard_body = to_bytes(dashboard.into_body(), 256_000)
        .await
        .expect("dashboard body should be readable");
    assert!(String::from_utf8_lossy(&dashboard_body).contains("PyroRisk — Centre opérationnel"));

    let config_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/config")
                .body(Body::empty())
                .expect("request should be valid"),
        )
        .await
        .expect("config request should complete");
    assert_eq!(config_response.status(), StatusCode::OK);
    let config_body = to_bytes(config_response.into_body(), 1024)
        .await
        .expect("config body should be readable");
    let config: serde_json::Value =
        serde_json::from_slice(&config_body).expect("config body should be JSON");
    assert_eq!(config["territory"], json!("France métropolitaine"));
    assert_eq!(config["h3_resolution"], json!(9));
    assert_eq!(config["bbox"], json!([-5.15, 41.31, 9.57, 51.09]));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .expect("request should be valid"),
        )
        .await
        .expect("health request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024)
        .await
        .expect("response body should be readable");
    let payload: serde_json::Value =
        serde_json::from_slice(&body).expect("response body should be JSON");
    assert_eq!(payload["status"], json!("ok"));
    assert_eq!(payload["db"], json!("ok"));
    assert!(payload["sources"].is_array());
}
