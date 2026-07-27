use chrono::{TimeZone, Utc};
use serde_json::json;
use sqlx::PgPool;
use store::{
    CalendarRuleVersion, DatasetBuildCounts, DatasetVersionSpec, FeatureSnapshotSpec,
    HistoricalCalendarDayRecord, Store,
};

#[tokio::test]
async fn feature_snapshot_registration_is_idempotent_and_supports_activation() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");
    let family = "test_snapshot_foundation_family";

    let spec_a = snapshot_spec(family, "checksum_a");
    let id_a1 = store
        .register_feature_snapshot(&spec_a)
        .await
        .expect("register a first time");
    let id_a2 = store
        .register_feature_snapshot(&spec_a)
        .await
        .expect("register a again");
    assert_eq!(
        id_a1, id_a2,
        "same (family, checksum) must return the same id"
    );

    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM features.feature_snapshots WHERE family = $1 AND logical_checksum = $2",
    )
    .bind(family)
    .bind("checksum_a")
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(rows, 1, "re-registering must never duplicate a row");

    store
        .activate_feature_snapshot(&id_a1)
        .await
        .expect("activate a");
    let status_a: String =
        sqlx::query_scalar("SELECT status FROM features.feature_snapshots WHERE id = $1::uuid")
            .bind(&id_a1)
            .fetch_one(&pool)
            .await
            .expect("status a");
    assert_eq!(status_a, "active");

    let spec_b = snapshot_spec(family, "checksum_b");
    let id_b = store
        .register_feature_snapshot(&spec_b)
        .await
        .expect("register b");
    store
        .activate_feature_snapshot(&id_b)
        .await
        .expect("activate b");

    let status_a_after: String =
        sqlx::query_scalar("SELECT status FROM features.feature_snapshots WHERE id = $1::uuid")
            .bind(&id_a1)
            .fetch_one(&pool)
            .await
            .expect("status a after");
    assert_eq!(
        status_a_after, "superseded",
        "activating b must supersede a"
    );
    let status_b: String =
        sqlx::query_scalar("SELECT status FROM features.feature_snapshots WHERE id = $1::uuid")
            .bind(&id_b)
            .fetch_one(&pool)
            .await
            .expect("status b");
    assert_eq!(status_b, "active");

    sqlx::query("DELETE FROM features.feature_snapshots WHERE family = $1")
        .bind(family)
        .execute(&pool)
        .await
        .expect("cleanup snapshots");
}

fn snapshot_spec(family: &str, checksum: &str) -> FeatureSnapshotSpec {
    FeatureSnapshotSpec {
        family: family.to_owned(),
        source: "test source".to_owned(),
        provider: None,
        vintage: None,
        valid_from: None,
        valid_until: None,
        available_from: Utc::now(),
        available_until: None,
        retrieved_at: None,
        code_version: "test".to_owned(),
        normalizer_version: "test".to_owned(),
        parameters: json!({"test": true}),
        source_checksum: None,
        logical_checksum: checksum.to_owned(),
        reference_table: "public.cell_static".to_owned(),
        cell_count: 1,
        h3_resolution: 8,
        geographic_coverage: None,
        temporal_classification: "current_snapshot_applied_historically".to_owned(),
        limitations: json!([]),
        license_attribution: None,
        notes: None,
    }
}

#[tokio::test]
async fn calendar_rule_version_rejects_silent_change() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");
    let logical_id = "test_calendar_rule_foundation";

    let rule_v1 = CalendarRuleVersion {
        logical_id: logical_id.to_owned(),
        rule_type: "public_holiday".to_owned(),
        description: "test".to_owned(),
        parameters: json!({"v": 1}),
        code_version: "test".to_owned(),
        status: "draft".to_owned(),
        checksum: "checksum_v1".to_owned(),
        notes: None,
    };
    let id1 = store
        .ensure_calendar_rule_version(&rule_v1)
        .await
        .expect("first ensure");
    let id2 = store
        .ensure_calendar_rule_version(&rule_v1)
        .await
        .expect("second ensure, same checksum");
    assert_eq!(id1, id2);

    let mut rule_changed = rule_v1.clone();
    rule_changed.checksum = "checksum_v2_different".to_owned();
    let result = store.ensure_calendar_rule_version(&rule_changed).await;
    assert!(
        result.is_err(),
        "changing checksum under the same logical_id must be rejected"
    );

    sqlx::query("DELETE FROM features.calendar_rule_versions WHERE logical_id = $1")
        .bind(logical_id)
        .execute(&pool)
        .await
        .expect("cleanup rule");
}

#[tokio::test]
async fn historical_calendar_days_persist_is_idempotent() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");
    let logical_id = "test_calendar_days_foundation";

    let rule = CalendarRuleVersion {
        logical_id: logical_id.to_owned(),
        rule_type: "public_holiday".to_owned(),
        description: "test".to_owned(),
        parameters: json!({"test": true}),
        code_version: "test".to_owned(),
        status: "draft".to_owned(),
        checksum: "checksum_calendar_days_test".to_owned(),
        notes: None,
    };
    let rule_version_id = store
        .ensure_calendar_rule_version(&rule)
        .await
        .expect("rule");

    let day = HistoricalCalendarDayRecord {
        date: chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        school_zone: "unspecified".to_owned(),
        year: 2020,
        month: 1,
        day_of_week: 2,
        is_weekend: false,
        public_holiday: true,
        public_holiday_label: Some("Jour de l'An".to_owned()),
        school_holiday: None,
        school_holiday_label: None,
        is_day_before_public_holiday: false,
        is_day_after_public_holiday: false,
        season: 0,
        season_sine: 0.0,
        season_cosine: 1.0,
        available_from: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
        source: "test".to_owned(),
        // school_holiday is None above (no verified source), so the row
        // must be classified unavailable_historically, matching the
        // database CHECK constraint (school_holiday_classification_check).
        temporal_classification: "unavailable_historically".to_owned(),
        logical_checksum: "day_checksum_test".to_owned(),
    };
    store
        .persist_historical_calendar_days(&rule_version_id, std::slice::from_ref(&day))
        .await
        .expect("persist first time");
    store
        .persist_historical_calendar_days(&rule_version_id, std::slice::from_ref(&day))
        .await
        .expect("persist replay");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM features.historical_calendar_days WHERE rule_version_id = $1::uuid",
    )
    .bind(&rule_version_id)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(count, 1, "replaying the same day must not duplicate it");

    sqlx::query("DELETE FROM features.historical_calendar_days WHERE rule_version_id = $1::uuid")
        .bind(&rule_version_id)
        .execute(&pool)
        .await
        .expect("cleanup days");
    sqlx::query("DELETE FROM features.calendar_rule_versions WHERE logical_id = $1")
        .bind(logical_id)
        .execute(&pool)
        .await
        .expect("cleanup rule");
}

#[tokio::test]
async fn dataset_version_finalization_is_immutable() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");
    let logical_id = "test_dataset_version_foundation";

    let spec = DatasetVersionSpec {
        logical_id: logical_id.to_owned(),
        name: "test dataset".to_owned(),
        description: "test".to_owned(),
        observation_unit: "h3_cell_x_civil_date".to_owned(),
        h3_resolution: 8,
        timezone: "Europe/Paris".to_owned(),
        period_start: chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        period_end: chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
        variant: "strict".to_owned(),
        code_version: "test".to_owned(),
        migrations: json!([13, 14, 15]),
        quality_rule_versions: json!([]),
        feature_snapshot_ids: json!([]),
        calendar_rule_version_id: None,
        inclusion_rules: json!({}),
        exclusion_rules: json!({}),
        negative_strategy: "test".to_owned(),
        negative_parameters: json!({}),
        seed: 1,
        splits: json!({}),
        author_or_pipeline: "test".to_owned(),
        notes: None,
    };
    let dataset_version_id = store.create_dataset_version(&spec).await.expect("create");
    store
        .set_dataset_version_status(&dataset_version_id, "building")
        .await
        .expect("set building");
    let counts = DatasetBuildCounts {
        row_count: 0,
        positive_count: 0,
        negative_count: 0,
        exclusion_count: 0,
    };
    store
        .finalize_dataset_version(&dataset_version_id, "final_checksum_test", &counts)
        .await
        .expect("finalize");

    let status: String =
        sqlx::query_scalar("SELECT status FROM ml.dataset_versions WHERE id = $1::uuid")
            .bind(&dataset_version_id)
            .fetch_one(&pool)
            .await
            .expect("status");
    assert_eq!(status, "finalized");

    let result = store
        .set_dataset_version_status(&dataset_version_id, "draft")
        .await;
    assert!(
        result.is_err(),
        "modifying a finalized dataset version must be rejected"
    );

    sqlx::query("DELETE FROM ml.dataset_versions WHERE logical_id = $1")
        .bind(logical_id)
        .execute(&pool)
        .await
        .expect("cleanup dataset version");
}
