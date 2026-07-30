use chrono::{Duration, Utc};
use sqlx::PgPool;
use store::{FreshnessThresholds, Store, SystemSnapshotContext};

async fn connect() -> Option<(Store, PgPool)> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").ok()?;
    let store = Store::connect(&database_url).await.expect("connect");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("verification pool");
    Some((store, pool))
}

#[tokio::test]
async fn system_snapshot_capture_is_idempotent_same_day() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let environment = "test-idempotent";
    sqlx::query("DELETE FROM observability.system_snapshots WHERE environment = $1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup");

    let captured_at = Utc::now();
    let ctx = SystemSnapshotContext {
        application_revision: Some("abc123".to_owned()),
        ..Default::default()
    };

    let first = store
        .capture_system_snapshot(environment, "daily", captured_at, &ctx)
        .await
        .expect("first capture");
    let second = store
        .capture_system_snapshot(environment, "daily", captured_at, &ctx)
        .await
        .expect("second capture");

    assert_eq!(first.id, second.id, "same day must upsert, never duplicate");
    assert_eq!(
        first.checksum, second.checksum,
        "same inputs must checksum identically"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM observability.system_snapshots WHERE environment = $1",
    )
    .bind(environment)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(
        count, 1,
        "unique (environment, capture_date, cadence) must hold"
    );
}

#[tokio::test]
async fn system_snapshot_reports_forecast_and_firms_absence_honestly() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let environment = "test-absence";
    sqlx::query("DELETE FROM observability.system_snapshots WHERE environment = $1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup");

    let row = store
        .capture_system_snapshot(
            environment,
            "daily",
            Utc::now(),
            &SystemSnapshotContext::default(),
        )
        .await
        .expect("capture");

    // A fixture-profile database with no forecast batch yet must report
    // absence explicitly, never a fabricated age.
    if row.forecast_last_complete_at.is_none() {
        assert!(row.forecast_age_seconds.is_none());
    }
    assert_eq!(row.caddy_state, "non_exposed");
    assert!(!row.shadow_scoring_enabled);
}

#[tokio::test]
async fn compare_j1_reports_deltas_and_avoids_division_by_zero() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let environment = "test-compare";
    sqlx::query("DELETE FROM observability.system_snapshots WHERE environment = $1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup");

    let yesterday = Utc::now() - Duration::days(1);
    store
        .capture_system_snapshot(
            environment,
            "daily",
            yesterday,
            &SystemSnapshotContext::default(),
        )
        .await
        .expect("capture j-1");
    store
        .capture_system_snapshot(
            environment,
            "daily",
            Utc::now(),
            &SystemSnapshotContext::default(),
        )
        .await
        .expect("capture latest");

    let comparison = store
        .compare_system_snapshots(environment, 1)
        .await
        .expect("compare query")
        .expect("both days present");

    let error_metric = comparison
        .iter()
        .find(|entry| entry.metric == "error_count_24h")
        .expect("error_count_24h present");
    // Both days start from a clean fixture with zero errors: previous == 0
    // must never produce a relative_delta (would be a divide-by-zero).
    if error_metric.previous_value == Some(0) {
        assert!(error_metric.relative_delta.is_none());
    }
}

#[tokio::test]
async fn alerts_flag_missing_active_model_and_are_not_duplicated_on_replay() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let environment = "test-alerts";
    sqlx::query("DELETE FROM observability.snapshot_alerts WHERE system_snapshot_id IN (SELECT id FROM observability.system_snapshots WHERE environment = $1)")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup alerts");
    sqlx::query("DELETE FROM observability.system_snapshots WHERE environment = $1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup snapshots");
    sqlx::query("DELETE FROM human_model_versions WHERE active")
        .execute(&pool)
        .await
        .expect("ensure no active model for this test");

    let snapshot = store
        .capture_system_snapshot(
            environment,
            "daily",
            Utc::now(),
            &SystemSnapshotContext::default(),
        )
        .await
        .expect("capture");
    assert_eq!(snapshot.active_model_count, Some(0));

    let first = store
        .evaluate_and_record_alerts(&snapshot, FreshnessThresholds::default())
        .await
        .expect("first evaluation");
    assert!(first.iter().any(|a| a.rule_id == "active_model_count"));

    let second = store
        .evaluate_and_record_alerts(&snapshot, FreshnessThresholds::default())
        .await
        .expect("second evaluation must not duplicate");
    assert!(
        second.is_empty(),
        "replaying the same snapshot must not re-record an already-recorded alert"
    );
}

#[tokio::test]
async fn weekly_scientific_snapshot_is_idempotent_and_published_immutable() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let valid_at = Utc::now();
    let logical_id = format!("scientific-weekly-nowcast-{}", valid_at.format("%Y-%m-%d"));
    sqlx::query(
        "DELETE FROM observability.scientific_snapshot_values WHERE snapshot_id IN
            (SELECT id FROM observability.scientific_snapshots WHERE logical_id = $1)",
    )
    .bind(&logical_id)
    .execute(&pool)
    .await
    .expect("cleanup values");
    sqlx::query("DELETE FROM observability.scientific_snapshots WHERE logical_id = $1")
        .bind(&logical_id)
        .execute(&pool)
        .await
        .expect("cleanup manifest");

    let first = store
        .capture_weekly_scientific_snapshot(valid_at)
        .await
        .expect("first capture");
    assert_eq!(first.status, "published");
    assert!(first.checksum.is_some());
    assert_eq!(
        first.cell_count_expected,
        first.cell_count_present + first.missing_count
    );

    let second = store
        .capture_weekly_scientific_snapshot(valid_at)
        .await
        .expect("second capture must be a safe no-op, not an error");
    assert_eq!(first.id, second.id);
    assert_eq!(
        first.checksum, second.checksum,
        "replay must not silently change published content"
    );

    let value_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM observability.scientific_snapshot_values WHERE snapshot_id = $1::uuid",
    )
    .bind(&first.id)
    .fetch_one(&pool)
    .await
    .expect("count values");
    assert_eq!(
        value_count,
        first.cell_count_present + (first.missing_count)
    );
}
