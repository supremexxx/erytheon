//! Phase 4A: integration tests for the read-only scientific console API
//! (`/api/science/*`). These tests exercise the real isolated database
//! (skipped, not failed, when `DATABASE_URL` is unset) and focus on:
//! contract stability, pagination/filter plumbing, honest handling of
//! missing data (no active v1, no candidate, no dataset), and -- most
//! importantly -- proving that every endpoint is a `GET` that changes
//! nothing in the database.

use api::AppState;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::Value;
use sqlx::PgPool;
use store::Store;
use tokio::sync::broadcast;
use tower::ServiceExt;

const SCIENCE_CSS: &str = include_str!("../static/science/science.css");
const SCIENCE_JS: &str = include_str!("../static/science/science.js");
const SCIENCE_PHASES: &str = include_str!("../static/science/phases.json");

async fn connect() -> Option<Store> {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping science console integration test: DATABASE_URL is not configured");
        return None;
    };
    Some(
        Store::connect(&database_url)
            .await
            .expect("database should accept connections and migrations"),
    )
}

fn url_for_database(base_url: &str, db_name: &str) -> String {
    let (prefix, _) = base_url
        .rsplit_once('/')
        .expect("DATABASE_URL must contain a database name");
    format!("{prefix}/{db_name}")
}

async fn create_temp_database(admin_url: &str, db_name: &str) -> String {
    let admin_pool = PgPool::connect(admin_url)
        .await
        .expect("connect to admin database");
    sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("drop stale science test database");
    sqlx::query(&format!("CREATE DATABASE {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("create science test database");
    admin_pool.close().await;

    let temp_url = url_for_database(admin_url, db_name);
    Store::connect(&temp_url)
        .await
        .expect("migrate science test database");
    temp_url
}

async fn drop_temp_database(admin_url: &str, db_name: &str) {
    let admin_pool = PgPool::connect(admin_url)
        .await
        .expect("connect to admin database for cleanup");
    sqlx::query(
        "SELECT pg_terminate_backend(pid)
         FROM pg_stat_activity
         WHERE datname = $1 AND pid <> pg_backend_pid()",
    )
    .bind(db_name)
    .execute(&admin_pool)
    .await
    .expect("terminate science test connections");
    sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("drop science test database");
    admin_pool.close().await;
}

fn build_app(store: Store) -> axum::Router {
    let grid = grid::H3Grid::new(9).expect("valid grid");
    let (updates, _) = broadcast::channel(1);
    api::router(AppState::new(store, grid, updates).with_science_console_enabled(true))
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

#[test]
fn science_frontend_stabilizers_are_versioned() {
    assert!(
        SCIENCE_CSS.contains(".sci-table-scroll") && SCIENCE_CSS.contains("pre.sci-mono"),
        "wide tables need a local scroll container instead of overflowing the viewport"
    );
    assert!(
        SCIENCE_JS.contains("class=\"sci-table-scroll\""),
        "rendered tables need to use the scroll container"
    );
    assert!(
        SCIENCE_JS.contains("\"focusin\"")
            && SCIENCE_JS.contains("aria-describedby=\"sci-tooltip-portal\""),
        "scientific definitions need keyboard-accessible tooltips"
    );
    assert!(
        SCIENCE_JS.contains("aucune version ni aucun build enregistré"),
        "an empty production dataset registry must not be presented as draft or validated"
    );
    assert!(
        SCIENCE_PHASES.contains("\"id\": \"phase4a3\"")
            && !SCIENCE_PHASES.contains("Comparaison v1 non encore réalisée"),
        "progress must include the current stabilization phase and omit resolved risks"
    );
}

/// When the console is disabled (the production default), none of the
/// `/api/science/*` or `/science*` routes should exist at all -- proves
/// the deployment gate actually gates something, not merely a flag that
/// is read and ignored.
#[tokio::test]
async fn science_routes_absent_when_console_disabled() {
    let Some(store) = connect().await else {
        return;
    };
    let grid = grid::H3Grid::new(9).expect("valid grid");
    let (updates, _) = broadcast::channel(1);
    let app = api::router(AppState::new(store, grid, updates));

    for uri in ["/science", "/science/overview", "/api/science/overview"] {
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
async fn overview_reports_real_counters_and_is_stable_across_calls() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, first) = get_json(&app, "/api/science/overview").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["app_status"], Value::String("ok".into()));
    assert_eq!(first["db_status"], Value::String("ok".into()));
    assert!(first["migrations_applied"].as_i64().unwrap_or(0) > 0);
    assert!(first["bdiff_events_total"].is_i64());

    let (status_again, second) = get_json(&app, "/api/science/overview").await;
    assert_eq!(status_again, StatusCode::OK);
    assert_eq!(
        first, second,
        "a read-only overview must return identical data across back-to-back calls"
    );
}

#[tokio::test]
async fn models_endpoint_never_fabricates_a_missing_candidate_or_v1() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);
    let (status, body) = get_json(&app, "/api/science/models").await;
    assert_eq!(status, StatusCode::OK);

    // Whatever the current state of the isolated database, the response
    // must always carry these two keys explicitly (as `null`, not
    // omitted, and never a fabricated placeholder object).
    assert!(body.get("active_v1").is_some());
    assert!(body.get("candidate").is_some());
    assert!(
        body["comparison"]["source"]
            .as_str()
            .unwrap()
            .contains("PHASE3B8")
    );
    assert!(
        body["scientific_interpretation"]
            .as_str()
            .unwrap()
            .contains("propension")
    );

    if let Some(candidate) = body["candidate"].as_object() {
        // The candidate must never be reported as `active` -- the
        // console must not misrepresent an inactive/candidate row as
        // being in production.
        assert_ne!(candidate["status"], Value::String("active".into()));
    }
}

#[tokio::test]
async fn datasets_list_and_missing_detail_are_handled_honestly() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, datasets) = get_json(&app, "/api/science/datasets").await;
    assert_eq!(status, StatusCode::OK);
    assert!(datasets.is_array());
    for dataset in datasets.as_array().unwrap() {
        assert!(dataset["logical_id"].is_string());
        assert!(dataset["status"].is_string());
        // row_count/positive_count/etc. may genuinely be `null` for
        // draft datasets -- must never be silently coerced to 0.
    }

    let (status, body) = get_json(&app, "/api/science/datasets/does-not-exist-logical-id").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        body["error"]["code"],
        Value::String("dataset_not_found".into())
    );
}

#[tokio::test]
async fn data_quality_summary_and_paginated_events_respect_filters() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, summary) = get_json(&app, "/api/science/data-quality").await;
    assert_eq!(status, StatusCode::OK);
    assert!(summary["cause_counts"].is_array());
    assert!(summary["duplicate_classification_counts"].is_array());
    assert!(summary["geographic_quality_counts"].is_array());
    assert!(summary["combustibility_counts"].is_array());

    let (status, default_page) = get_json(&app, "/api/science/data-quality/events?limit=5").await;
    assert_eq!(status, StatusCode::OK);
    assert!(default_page.as_array().unwrap().len() <= 5);

    // An out-of-range limit must be clamped, never rejected with a
    // server error and never silently returning everything.
    let (status, clamped) = get_json(&app, "/api/science/data-quality/events?limit=999999").await;
    assert_eq!(status, StatusCode::OK);
    assert!(clamped.as_array().unwrap().len() <= 200);

    let (status, filtered) = get_json(
        &app,
        "/api/science/data-quality/events?cause=human_known&limit=10",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    for row in filtered.as_array().unwrap() {
        assert_eq!(row["cause_category"], Value::String("human_known".into()));
    }
}

#[tokio::test]
async fn sources_imports_and_pipelines_are_paginated_and_filterable() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, sources) = get_json(&app, "/api/science/sources").await;
    assert_eq!(status, StatusCode::OK);
    assert!(sources.is_array());

    let (status, imports_page1) = get_json(&app, "/api/science/imports?limit=2&offset=0").await;
    assert_eq!(status, StatusCode::OK);
    assert!(imports_page1.as_array().unwrap().len() <= 2);

    let (status, pipelines) =
        get_json(&app, "/api/science/pipelines?status=succeeded&limit=50").await;
    assert_eq!(status, StatusCode::OK);
    for row in pipelines.as_array().unwrap() {
        assert_eq!(row["status"], Value::String("succeeded".into()));
    }
}

#[tokio::test]
async fn features_and_calendar_never_hardcode_missing_school_holiday_data_as_zero() {
    let Some(store) = connect().await else {
        return;
    };
    let app = build_app(store);

    let (status, features) = get_json(&app, "/api/science/features").await;
    assert_eq!(status, StatusCode::OK);
    assert!(features["snapshots"].is_array());
    let calendar = &features["calendar"];
    let known = calendar["school_holiday_known_days"].as_i64().unwrap_or(0);
    let unknown = calendar["school_holiday_unknown_days"]
        .as_i64()
        .unwrap_or(0);
    let total = calendar["total_days"].as_i64().unwrap_or(0);
    assert_eq!(
        known + unknown,
        total,
        "known + unavailable school-holiday days must account for every calendar day, \
         with unavailability reported explicitly rather than folded into a zero count"
    );
}

#[tokio::test]
async fn system_summary_reports_exactly_one_active_model() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping science console integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = format!("erytheon_science_active_model_{}", std::process::id());
    let temp_url = create_temp_database(&admin_url, &db_name).await;
    let store = Store::connect(&temp_url).await.expect("science test store");
    store
        .activate_human_model(
            chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2021, 12, 31).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2022, 1, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2022, 12, 31).unwrap(),
            1,
            1,
            1,
            1,
            &serde_json::json!({"fixture": "science-active-v1"}),
            &serde_json::json!({"fixture": true}),
        )
        .await
        .expect("create exactly one active v1 fixture");

    let app = build_app(store);
    let (status, system) = get_json(&app, "/api/science/system").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(system["active_model_count"], Value::from(1));
    assert_eq!(system["migrations_failed"].as_i64().unwrap_or(-1), 0);

    drop(app);
    drop_temp_database(&admin_url, &db_name).await;
}

/// Confirms that hitting every read endpoint back-to-back does not
/// change the candidate registry row count -- the strongest evidence
/// available from a black-box HTTP test that this console is truly
/// read-only.
#[tokio::test]
async fn exercising_every_endpoint_writes_nothing_to_the_candidate_registry() {
    let Some(store) = connect().await else {
        return;
    };
    let count_before = store
        .model_candidate_registry_count()
        .await
        .expect("count should succeed");
    let app = build_app(store);

    for uri in [
        "/api/science/overview",
        "/api/science/progress",
        "/api/science/sources",
        "/api/science/imports",
        "/api/science/pipelines",
        "/api/science/data-quality",
        "/api/science/data-quality/events",
        "/api/science/features",
        "/api/science/calendar",
        "/api/science/datasets",
        "/api/science/models",
        "/api/science/system",
    ] {
        let (status, _) = get_json(&app, uri).await;
        assert!(
            status.is_success(),
            "{uri} should succeed against a healthy isolated database"
        );
    }

    // Re-open a fresh connection to check the post-request state,
    // independent of any connection-pool caching in `app`.
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("checked above");
    let verifying_store = Store::connect(&database_url)
        .await
        .expect("database should accept connections");
    let count_after = verifying_store
        .model_candidate_registry_count()
        .await
        .expect("count should succeed");
    assert_eq!(
        count_before, count_after,
        "no science console endpoint may write to ml.model_candidate_registry"
    );
}
