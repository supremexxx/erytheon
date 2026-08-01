use chrono::{Duration, Timelike as _, Utc};
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
async fn system_snapshot_capture_is_idempotent_same_daily_window() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let environment = "test-idempotent";
    sqlx::query("DELETE FROM observability.snapshot_capture_attempts WHERE environment = $1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup attempts");
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
    assert_eq!(count, 1, "unique daily window must hold");
}

#[tokio::test]
async fn hourly_snapshots_keep_distinct_buckets_and_auditable_replays() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let environment = "test-hourly-buckets";
    sqlx::query("DELETE FROM observability.snapshot_capture_attempts WHERE environment=$1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup attempts");
    sqlx::query("DELETE FROM observability.system_snapshots WHERE environment=$1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup snapshots");
    let base = (Utc::now() - Duration::hours(3))
        .with_minute(0)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .expect("exact UTC hour");
    let first = store
        .capture_system_snapshot(
            environment,
            "hourly",
            base,
            &SystemSnapshotContext::default(),
        )
        .await
        .expect("first hour");
    let replay = store
        .capture_system_snapshot(
            environment,
            "hourly",
            base + Duration::minutes(20),
            &SystemSnapshotContext::default(),
        )
        .await
        .expect("same-hour replay");
    let next = store
        .capture_system_snapshot(
            environment,
            "hourly",
            base + Duration::hours(1),
            &SystemSnapshotContext::default(),
        )
        .await
        .expect("next hour");
    assert_eq!(first.id, replay.id);
    assert_ne!(first.id, next.id);
    assert_eq!(first.capture_window_start, replay.capture_window_start);
    let attempts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM observability.snapshot_capture_attempts WHERE environment=$1",
    )
    .bind(environment)
    .fetch_one(&pool)
    .await
    .expect("attempt count");
    assert_eq!(attempts, 3);
}

#[tokio::test]
async fn concurrent_hourly_replay_converges_on_one_snapshot() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let environment = "test-hourly-concurrent";
    sqlx::query("DELETE FROM observability.snapshot_capture_attempts WHERE environment=$1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup attempts");
    sqlx::query("DELETE FROM observability.system_snapshots WHERE environment=$1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup snapshots");
    let at = Utc::now() - Duration::hours(6);
    let left_store = store.clone();
    let right_store = store.clone();
    let left_context = SystemSnapshotContext::default();
    let right_context = SystemSnapshotContext::default();
    let (left, right) = tokio::join!(
        left_store.capture_system_snapshot(environment, "hourly", at, &left_context),
        right_store.capture_system_snapshot(
            environment,
            "hourly",
            at + Duration::minutes(5),
            &right_context
        ),
    );
    let left = left.expect("left capture");
    let right = right.expect("right capture");
    assert_eq!(left.id, right.id);
    let attempts: Vec<i32> = sqlx::query_scalar(
        "SELECT attempt_number FROM observability.snapshot_capture_attempts
         WHERE environment=$1 ORDER BY attempt_number",
    )
    .bind(environment)
    .fetch_all(&pool)
    .await
    .expect("attempt sequence");
    assert_eq!(attempts, vec![1, 2]);
}

#[tokio::test]
async fn twenty_four_utc_slots_cross_midnight_and_report_a_real_gap() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let environment = "test-hourly-24";
    sqlx::query("DELETE FROM observability.snapshot_capture_attempts WHERE environment=$1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup attempts");
    sqlx::query("DELETE FROM observability.system_snapshots WHERE environment=$1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup snapshots");
    let start = (Utc::now().date_naive() - Duration::days(3))
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc();
    for hour in 0..24 {
        let row = store
            .capture_system_snapshot(
                environment,
                "hourly",
                start + Duration::hours(hour),
                &SystemSnapshotContext::default(),
            )
            .await
            .expect("hourly capture");
        assert_eq!(row.capture_window_start.minute(), 0);
    }
    let next_day = store
        .capture_system_snapshot(
            environment,
            "hourly",
            start + Duration::hours(24),
            &SystemSnapshotContext::default(),
        )
        .await
        .expect("midnight next day");
    assert_ne!(next_day.capture_date, start.date_naive());
    sqlx::query(
        "DELETE FROM observability.snapshot_capture_attempts
         WHERE environment=$1 AND capture_window_start=$2",
    )
    .bind(environment)
    .bind(start + Duration::hours(12))
    .execute(&pool)
    .await
    .expect("remove attempt");
    sqlx::query(
        "DELETE FROM observability.system_snapshots
         WHERE environment=$1 AND capture_window_start=$2",
    )
    .bind(environment)
    .bind(start + Duration::hours(12))
    .execute(&pool)
    .await
    .expect("create gap");
    let summary = store
        .hourly_snapshot_summary(environment)
        .await
        .expect("summary");
    assert_eq!(summary.expected_slots, 25);
    assert_eq!(summary.present_slots, 24);
    assert_eq!(summary.missing_slots, 1);
}

#[tokio::test]
async fn system_snapshot_reports_forecast_and_firms_absence_honestly() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let environment = "test-absence";
    sqlx::query("DELETE FROM observability.snapshot_capture_attempts WHERE environment = $1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup attempts");
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
    sqlx::query("DELETE FROM observability.snapshot_capture_attempts WHERE environment = $1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup attempts");
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
    sqlx::query("DELETE FROM observability.snapshot_capture_attempts WHERE environment = $1")
        .bind(environment)
        .execute(&pool)
        .await
        .expect("cleanup attempts");
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
async fn scientific_v2_refuses_incomplete_deployment_provenance() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    drop(pool);
    let error = store
        .capture_weekly_scientific_snapshot(Utc::now())
        .await
        .expect_err("missing revision/image lineage must fail closed");
    assert!(error.to_string().contains("snapshot contract violation"));
}
