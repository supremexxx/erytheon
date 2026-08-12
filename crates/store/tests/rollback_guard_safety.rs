//! Phase 3B.11: end-to-end verification that the `0013`-`0015` rollback
//! guards actually stop destructive statements when the real `.sql`
//! file is executed with `psql`, not just that their `DO` block's
//! logic is sound in isolation (mission section 8: "ne considère pas
//! un test du seul bloc DO comme suffisant"). Runs entirely against a
//! disposable temporary database created inside the existing isolated
//! `PostgreSQL` server -- never a new container, never the real isolated
//! `pyrorisk` database (which now holds real, useful historical
//! calendar/dataset data from earlier phases that must not be touched).

use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use std::path::PathBuf;
use std::process::Command;
use store::{
    CalendarRuleVersion, DatasetVersionSpec, FeatureSnapshotSpec, HistoricalCalendarDayRecord,
    Store,
};

fn migrations_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations/rollback")
}

/// Replaces the trailing `/pyrorisk` database name in a connection
/// string with a disposable temporary database name.
fn url_for_database(base_url: &str, db_name: &str) -> String {
    let (prefix, _) = base_url
        .rsplit_once('/')
        .expect("DATABASE_URL must contain a database name");
    format!("{prefix}/{db_name}")
}

fn run_psql(database_url: &str, sql_path: &std::path::Path) -> std::process::Output {
    Command::new("psql")
        .arg(database_url)
        .arg("-v")
        .arg("ON_ERROR_STOP=1")
        .arg("-1")
        .arg("-f")
        .arg(sql_path)
        .output()
        .expect(
            "psql must be installed in the test environment (apt-get install postgresql-client)",
        )
}

/// Creates a fresh, disposable database on the same server as
/// `admin_url` points to, runs every pending migration against it via
/// `Store::connect`, and returns its connection URL. The caller is
/// responsible for dropping it afterward.
async fn create_and_migrate_temp_database(admin_url: &str, db_name: &str) -> String {
    let admin_pool = PgPool::connect(admin_url)
        .await
        .expect("connect to admin database");
    sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("drop any stale temp database");
    sqlx::query(&format!("CREATE DATABASE {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("create temp database");
    admin_pool.close().await;

    let temp_url = url_for_database(admin_url, db_name);
    Store::connect(&temp_url)
        .await
        .expect("run all migrations against the temp database");
    temp_url
}

async fn drop_temp_database(admin_url: &str, db_name: &str) {
    let admin_pool = PgPool::connect(admin_url)
        .await
        .expect("connect to admin database for cleanup");
    // Terminate any lingering connections so DROP DATABASE doesn't fail.
    sqlx::query(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = $1 AND pid <> pg_backend_pid()",
    )
    .bind(db_name)
    .execute(&admin_pool)
    .await
    .ok();
    sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("drop temp database");
}

/// Removes a migration's `_sqlx_migrations` tracking row so the next
/// `Store::connect` re-applies its forward `.sql` file, restoring the
/// table that the empty-state rollback test just dropped. Mission
/// section 7: "restaurer proprement la migration via `SQLx`" -- never
/// leave `_sqlx_migrations` desynchronized from the real schema.
async fn mark_migration_pending_again(url: &str, version: i64) {
    let pool = PgPool::connect(url)
        .await
        .expect("pool for migration reset");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
        .bind(version)
        .execute(&pool)
        .await
        .expect("clear stale migration tracking row");
    pool.close().await;
}

async fn table_is_empty(pool: &PgPool, table: &str) -> bool {
    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap_or(-1);
    count == 0
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn rollback_0013_refuses_destructively_once_a_snapshot_exists() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = "erytheon_rollback_test_0013";
    let temp_url = create_and_migrate_temp_database(&admin_url, db_name).await;
    let down_sql = migrations_root().join("0013_feature_snapshot_foundation.down.sql");

    // Migration 0015 adds a foreign key from ml.dataset_row_snapshots to
    // features.feature_snapshots, and migration 0019 (phase 4A.5) adds
    // another from observability.scientific_snapshots.static_snapshot_id;
    // 0013 cannot roll back while either FK exists, even with an empty
    // feature_snapshots table -- rollbacks must run in reverse migration
    // order. Every dependent's own tables are empty here, so each
    // rollback below is authorized and safe.
    for down in [
        "0026_blue_ai_evidence.down.sql",
        "0025_blue_forecast_evidence_foundation.down.sql",
        "0024_daily_dense_scientific_archive.down.sql",
        "0022_scientific_snapshot_hardening.down.sql",
        "0021_snapshot_label_links.down.sql",
        "0020_snapshot_alerts.down.sql",
        "0019_scientific_snapshot_registry.down.sql",
        "0015_dataset_versioning_foundation.down.sql",
    ] {
        let output = run_psql(&temp_url, &migrations_root().join(down));
        assert!(
            output.status.success(),
            "rolling back dependent migration {down} first must succeed while its tables are empty: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // --- Empty-state rollback must succeed ---
    let pool = PgPool::connect(&temp_url).await.expect("pool");
    assert!(table_is_empty(&pool, "features.feature_snapshots").await);
    let output = run_psql(&temp_url, &down_sql);
    assert!(
        output.status.success(),
        "empty-state rollback must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let table_gone: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'features' AND table_name = 'feature_snapshots')",
    )
    .fetch_one(&pool)
    .await
    .expect("check table dropped");
    assert!(
        table_gone,
        "table must actually be dropped after an authorized empty rollback"
    );
    pool.close().await;

    // --- Restore via SQLx, keeping _sqlx_migrations consistent ---
    // 13, 15, 19, 20, 21, and 22 all had their objects dropped above; every
    // tracking row must be cleared so SQLx re-applies all five forward
    // migrations, in order, on the next connect.
    for version in [13, 15, 19, 20, 21, 22] {
        mark_migration_pending_again(&temp_url, version).await;
    }
    Store::connect(&temp_url)
        .await
        .expect("reapply migrations 13, 15, 19, 20, and 21");

    // --- Insert a minimal fixture, then the guard must refuse ---
    let store = Store::connect(&temp_url).await.expect("store");
    let pool = PgPool::connect(&temp_url).await.expect("pool");
    let spec = FeatureSnapshotSpec {
        family: "rollback_test_family".to_owned(),
        source: "test".to_owned(),
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
        logical_checksum: "rollback_test_checksum".to_owned(),
        reference_table: "public.cell_static".to_owned(),
        cell_count: 1,
        h3_resolution: 8,
        geographic_coverage: None,
        temporal_classification: "current_snapshot_applied_historically".to_owned(),
        limitations: json!([]),
        license_attribution: None,
        notes: None,
    };
    store
        .register_feature_snapshot(&spec)
        .await
        .expect("register fixture snapshot");
    assert!(!table_is_empty(&pool, "features.feature_snapshots").await);

    // Reapplying migrations above brought 0019-0021 back too; clear them
    // out of the way again so the assertion below exercises 0013's own
    // "data exists" guard, not the out-of-order guard.
    for down in [
        "0026_blue_ai_evidence.down.sql",
        "0025_blue_forecast_evidence_foundation.down.sql",
        "0024_daily_dense_scientific_archive.down.sql",
        "0022_scientific_snapshot_hardening.down.sql",
        "0021_snapshot_label_links.down.sql",
        "0020_snapshot_alerts.down.sql",
        "0019_scientific_snapshot_registry.down.sql",
    ] {
        let output = run_psql(&temp_url, &migrations_root().join(down));
        assert!(
            output.status.success(),
            "rolling back dependent migration {down} again must succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = run_psql(&temp_url, &down_sql);
    assert!(
        !output.status.success(),
        "rollback must refuse (non-zero exit) once real data exists"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing destructive rollback"),
        "guard error message must be present: {stderr}"
    );
    assert!(
        !table_is_empty(&pool, "features.feature_snapshots").await,
        "fixture row must survive the refused rollback"
    );
    let table_still_there: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'features' AND table_name = 'feature_snapshots')",
    )
    .fetch_one(&pool)
    .await
    .expect("check table survives");
    assert!(
        table_still_there,
        "table must not be dropped when the guard refuses"
    );
    pool.close().await;

    drop_temp_database(&admin_url, db_name).await;
}

#[tokio::test]
async fn rollback_0014_refuses_destructively_once_calendar_data_exists() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = "erytheon_rollback_test_0014";
    let temp_url = create_and_migrate_temp_database(&admin_url, db_name).await;
    let down_sql = migrations_root().join("0014_historical_calendar_foundation.down.sql");

    // Migration 0015 adds a foreign key from ml.dataset_versions to
    // features.calendar_rule_versions; 0014 cannot roll back while that
    // FK exists, even with empty tables -- rollbacks must run in
    // reverse migration order. 0015's own tables are empty here, so its
    // rollback is authorized and safe.
    let output_0015 = run_psql(
        &temp_url,
        &migrations_root().join("0015_dataset_versioning_foundation.down.sql"),
    );
    assert!(
        output_0015.status.success(),
        "rolling back the dependent migration 0015 first must succeed while its tables are empty: {}",
        String::from_utf8_lossy(&output_0015.stderr)
    );

    let pool = PgPool::connect(&temp_url).await.expect("pool");
    assert!(table_is_empty(&pool, "features.historical_calendar_days").await);
    assert!(table_is_empty(&pool, "features.calendar_rule_versions").await);
    let output = run_psql(&temp_url, &down_sql);
    assert!(
        output.status.success(),
        "empty-state rollback must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    pool.close().await;

    mark_migration_pending_again(&temp_url, 14).await;
    mark_migration_pending_again(&temp_url, 15).await;
    let store = Store::connect(&temp_url)
        .await
        .expect("reapply migrations 14 and 15");
    let pool = PgPool::connect(&temp_url).await.expect("pool");

    let rule = CalendarRuleVersion {
        logical_id: "rollback_test_calendar_rule".to_owned(),
        rule_type: "public_holiday".to_owned(),
        description: "test".to_owned(),
        parameters: json!({"test": true}),
        code_version: "test".to_owned(),
        status: "draft".to_owned(),
        checksum: "rollback_test_calendar_checksum".to_owned(),
        notes: None,
    };
    let rule_version_id = store
        .ensure_calendar_rule_version(&rule)
        .await
        .expect("fixture rule");
    let day = HistoricalCalendarDayRecord {
        date: chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        school_zone: "unspecified".to_owned(),
        year: 2020,
        month: 1,
        day_of_week: 2,
        is_weekend: false,
        public_holiday: true,
        public_holiday_label: Some("test holiday".to_owned()),
        school_holiday: None,
        school_holiday_label: None,
        is_day_before_public_holiday: false,
        is_day_after_public_holiday: false,
        season: 0,
        season_sine: 0.0,
        season_cosine: 1.0,
        available_from: Utc::now(),
        source: "test".to_owned(),
        temporal_classification: "unavailable_historically".to_owned(),
        logical_checksum: "rollback_test_day_checksum".to_owned(),
    };
    store
        .persist_historical_calendar_days(&rule_version_id, std::slice::from_ref(&day))
        .await
        .expect("fixture day");
    assert!(!table_is_empty(&pool, "features.historical_calendar_days").await);

    let output = run_psql(&temp_url, &down_sql);
    assert!(
        !output.status.success(),
        "rollback must refuse once real data exists"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing destructive rollback"),
        "guard error message must be present: {stderr}"
    );
    assert!(
        !table_is_empty(&pool, "features.historical_calendar_days").await,
        "fixture day must survive"
    );
    assert!(
        !table_is_empty(&pool, "features.calendar_rule_versions").await,
        "fixture rule must survive"
    );
    pool.close().await;

    drop_temp_database(&admin_url, db_name).await;
}

#[tokio::test]
async fn rollback_0015_refuses_destructively_once_a_dataset_version_exists() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = "erytheon_rollback_test_0015";
    let temp_url = create_and_migrate_temp_database(&admin_url, db_name).await;
    let down_sql = migrations_root().join("0015_dataset_versioning_foundation.down.sql");

    let pool = PgPool::connect(&temp_url).await.expect("pool");
    for table in [
        "ml.dataset_versions",
        "ml.dataset_builds",
        "ml.dataset_rows",
    ] {
        assert!(
            table_is_empty(&pool, table).await,
            "{table} must start empty"
        );
    }
    let output = run_psql(&temp_url, &down_sql);
    assert!(
        output.status.success(),
        "empty-state rollback must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    pool.close().await;

    mark_migration_pending_again(&temp_url, 15).await;
    let store = Store::connect(&temp_url)
        .await
        .expect("reapply migration 15");
    let pool = PgPool::connect(&temp_url).await.expect("pool");

    let spec = DatasetVersionSpec {
        logical_id: "rollback_test_dataset_version".to_owned(),
        name: "rollback test dataset".to_owned(),
        description: "test".to_owned(),
        observation_unit: "h3_cell_x_civil_date".to_owned(),
        h3_resolution: 8,
        timezone: "Europe/Paris".to_owned(),
        period_start: chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
        period_end: chrono::NaiveDate::from_ymd_opt(2020, 12, 31).unwrap(),
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
    store
        .create_dataset_version(&spec)
        .await
        .expect("fixture dataset version");
    assert!(!table_is_empty(&pool, "ml.dataset_versions").await);

    let output = run_psql(&temp_url, &down_sql);
    assert!(
        !output.status.success(),
        "rollback must refuse once real data exists"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing destructive rollback"),
        "guard error message must be present: {stderr}"
    );
    assert!(
        !table_is_empty(&pool, "ml.dataset_versions").await,
        "fixture dataset version must survive"
    );
    pool.close().await;

    drop_temp_database(&admin_url, db_name).await;
}

async fn table_exists(pool: &PgPool, schema: &str, table: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM information_schema.tables
             WHERE table_schema = $1 AND table_name = $2
         )",
    )
    .bind(schema)
    .bind(table)
    .fetch_one(pool)
    .await
    .expect("check table existence")
}

async fn column_exists(pool: &PgPool, schema: &str, table: &str, column: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM information_schema.columns
             WHERE table_schema = $1 AND table_name = $2 AND column_name = $3
         )",
    )
    .bind(schema)
    .bind(table)
    .bind(column)
    .fetch_one(pool)
    .await
    .expect("check column existence")
}

#[tokio::test]
async fn rollback_0017_then_0016_succeeds_only_in_reverse_order_when_empty() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = "erytheon_rollback_test_0016_0017_empty";
    let temp_url = create_and_migrate_temp_database(&admin_url, db_name).await;
    let pool = PgPool::connect(&temp_url).await.expect("pool");

    let out_of_order = run_psql(
        &temp_url,
        &migrations_root().join("0016_model_candidate_registry.down.sql"),
    );
    assert!(
        !out_of_order.status.success(),
        "0016 must refuse while migration 0017 objects still exist"
    );
    assert!(
        String::from_utf8_lossy(&out_of_order.stderr).contains("0017 must be rolled back"),
        "out-of-order failure must explain the required order: {}",
        String::from_utf8_lossy(&out_of_order.stderr)
    );
    assert!(
        table_exists(&pool, "ml", "model_candidate_registry").await,
        "out-of-order rollback must preserve the registry"
    );
    assert!(
        column_exists(&pool, "ml", "model_candidate_registry", "seed").await,
        "out-of-order rollback must preserve 0017 objects"
    );

    let rollback_0017 = run_psql(
        &temp_url,
        &migrations_root().join("0017_model_candidate_registry_identity.down.sql"),
    );
    assert!(
        rollback_0017.status.success(),
        "0017 rollback must succeed on an empty registry: {}",
        String::from_utf8_lossy(&rollback_0017.stderr)
    );
    assert!(
        table_exists(&pool, "ml", "model_candidate_registry").await,
        "0016 registry must remain after rolling back 0017"
    );
    assert!(
        !column_exists(&pool, "ml", "model_candidate_registry", "seed").await,
        "0017 seed column must be removed"
    );

    let rollback_0016 = run_psql(
        &temp_url,
        &migrations_root().join("0016_model_candidate_registry.down.sql"),
    );
    assert!(
        rollback_0016.status.success(),
        "0016 rollback must succeed after 0017 on an empty registry: {}",
        String::from_utf8_lossy(&rollback_0016.stderr)
    );
    assert!(
        !table_exists(&pool, "ml", "model_candidate_registry").await,
        "0016 registry must be removed"
    );
    let ml_schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM information_schema.schemata WHERE schema_name = 'ml'
         )",
    )
    .fetch_one(&pool)
    .await
    .expect("check ml schema");
    assert!(ml_schema_exists, "unrelated ml schema must remain intact");

    pool.close().await;
    drop_temp_database(&admin_url, db_name).await;
}

#[tokio::test]
async fn rollback_0017_refuses_and_preserves_a_registered_candidate() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = "erytheon_rollback_test_0017_populated";
    let temp_url = create_and_migrate_temp_database(&admin_url, db_name).await;
    let pool = PgPool::connect(&temp_url).await.expect("pool");

    sqlx::query(
        "INSERT INTO ml.model_candidate_registry (
             model_family, model_name, artifact_version, status, git_commit,
             dataset_logical_id, dataset_row_fingerprint, seed, artifact,
             artifact_checksum, metrics, scientific_interpretation, known_limitations
         ) VALUES (
             'rollback_fixture', 'rollback_fixture', 1, 'inactive', 'fixture',
             'rollback_fixture', 'fixture', 17, '{}', 'fixture', '{}',
             'fixture', '[]'
         )",
    )
    .execute(&pool)
    .await
    .expect("insert 0017 fixture");

    let output = run_psql(
        &temp_url,
        &migrations_root().join("0017_model_candidate_registry_identity.down.sql"),
    );
    assert!(
        !output.status.success(),
        "0017 rollback must refuse when a candidate exists"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("refusing destructive rollback"),
        "0017 guard must return a useful error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ml.model_candidate_registry")
            .fetch_one(&pool)
            .await
            .expect("count preserved candidate"),
        1
    );
    assert!(
        column_exists(&pool, "ml", "model_candidate_registry", "seed").await,
        "failed rollback must preserve the seed column"
    );
    let identity_constraint_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM pg_constraint
             WHERE conname = 'model_candidate_registry_logical_identity_unique'
         )",
    )
    .fetch_one(&pool)
    .await
    .expect("check identity constraint");
    assert!(
        identity_constraint_exists,
        "failed rollback must preserve the identity constraint"
    );

    pool.close().await;
    drop_temp_database(&admin_url, db_name).await;
}

#[tokio::test]
async fn rollback_0016_refuses_and_preserves_a_registered_candidate() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = "erytheon_rollback_test_0016_populated";
    let temp_url = create_and_migrate_temp_database(&admin_url, db_name).await;
    let pool = PgPool::connect(&temp_url).await.expect("pool");

    let rollback_0017 = run_psql(
        &temp_url,
        &migrations_root().join("0017_model_candidate_registry_identity.down.sql"),
    );
    assert!(
        rollback_0017.status.success(),
        "0017 must be removed before testing the populated 0016 guard: {}",
        String::from_utf8_lossy(&rollback_0017.stderr)
    );
    sqlx::query(
        "INSERT INTO ml.model_candidate_registry (
             model_family, model_name, artifact_version, status, git_commit,
             dataset_logical_id, dataset_row_fingerprint, artifact,
             artifact_checksum, metrics, scientific_interpretation, known_limitations
         ) VALUES (
             'rollback_fixture', 'rollback_fixture', 1, 'inactive', 'fixture',
             'rollback_fixture', 'fixture', '{}', 'fixture', '{}',
             'fixture', '[]'
         )",
    )
    .execute(&pool)
    .await
    .expect("insert 0016 fixture");

    let output = run_psql(
        &temp_url,
        &migrations_root().join("0016_model_candidate_registry.down.sql"),
    );
    assert!(
        !output.status.success(),
        "0016 rollback must refuse when a candidate exists"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("refusing destructive rollback"),
        "0016 guard must return a useful error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ml.model_candidate_registry")
            .fetch_one(&pool)
            .await
            .expect("count preserved candidate"),
        1
    );
    assert!(
        table_exists(&pool, "ml", "model_candidate_registry").await,
        "failed rollback must preserve the registry table"
    );
    let family_index_exists: bool = sqlx::query_scalar(
        "SELECT to_regclass('ml.model_candidate_registry_family_idx') IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("check family index");
    assert!(
        family_index_exists,
        "failed rollback must preserve the registry index"
    );

    pool.close().await;
    drop_temp_database(&admin_url, db_name).await;
}

/// `0026`-`0018` must roll back only in strict reverse order
/// on an empty database, and each rollback must refuse while a later
/// migration's objects still exist.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn rollback_0026_to_0018_succeeds_only_in_reverse_order_when_empty() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = "erytheon_rollback_test_0018_0022_empty";
    let temp_url = create_and_migrate_temp_database(&admin_url, db_name).await;
    let pool = PgPool::connect(&temp_url).await.expect("pool");

    let out_of_order = run_psql(
        &temp_url,
        &migrations_root().join("0018_observability_foundation.down.sql"),
    );
    assert!(
        !out_of_order.status.success(),
        "0018 must refuse while migration 0019+ objects still exist"
    );
    assert!(
        table_exists(&pool, "observability", "scientific_snapshots").await,
        "out-of-order rollback must preserve 0019's manifest table"
    );

    let rollback_0026 = run_psql(
        &temp_url,
        &migrations_root().join("0026_blue_ai_evidence.down.sql"),
    );
    assert!(
        rollback_0026.status.success(),
        "0026 rollback must succeed before 0025: {}",
        String::from_utf8_lossy(&rollback_0026.stderr)
    );

    let rollback_0025 = run_psql(
        &temp_url,
        &migrations_root().join("0025_blue_forecast_evidence_foundation.down.sql"),
    );
    assert!(
        rollback_0025.status.success(),
        "0025 rollback must succeed before 0024: {}",
        String::from_utf8_lossy(&rollback_0025.stderr)
    );

    let rollback_0024 = run_psql(
        &temp_url,
        &migrations_root().join("0024_daily_dense_scientific_archive.down.sql"),
    );
    assert!(
        rollback_0024.status.success(),
        "0024 rollback must succeed before 0022: {}",
        String::from_utf8_lossy(&rollback_0024.stderr)
    );

    let rollback_0022 = run_psql(
        &temp_url,
        &migrations_root().join("0022_scientific_snapshot_hardening.down.sql"),
    );
    assert!(
        rollback_0022.status.success(),
        "0022 rollback must succeed before 0021: {}",
        String::from_utf8_lossy(&rollback_0022.stderr)
    );

    let rollback_0021 = run_psql(
        &temp_url,
        &migrations_root().join("0021_snapshot_label_links.down.sql"),
    );
    assert!(
        rollback_0021.status.success(),
        "0021 rollback must succeed on an empty table: {}",
        String::from_utf8_lossy(&rollback_0021.stderr)
    );
    assert!(!table_exists(&pool, "ml", "snapshot_label_links").await);

    let rollback_0020 = run_psql(
        &temp_url,
        &migrations_root().join("0020_snapshot_alerts.down.sql"),
    );
    assert!(
        rollback_0020.status.success(),
        "0020 rollback must succeed on an empty table: {}",
        String::from_utf8_lossy(&rollback_0020.stderr)
    );
    assert!(!table_exists(&pool, "observability", "snapshot_alerts").await);

    let rollback_0019 = run_psql(
        &temp_url,
        &migrations_root().join("0019_scientific_snapshot_registry.down.sql"),
    );
    assert!(
        rollback_0019.status.success(),
        "0019 rollback must succeed on empty tables: {}",
        String::from_utf8_lossy(&rollback_0019.stderr)
    );
    assert!(!table_exists(&pool, "observability", "scientific_snapshots").await);
    assert!(!table_exists(&pool, "observability", "scientific_snapshot_values").await);

    let rollback_0018 = run_psql(
        &temp_url,
        &migrations_root().join("0018_observability_foundation.down.sql"),
    );
    assert!(
        rollback_0018.status.success(),
        "0018 rollback must succeed on an empty table: {}",
        String::from_utf8_lossy(&rollback_0018.stderr)
    );
    assert!(!table_exists(&pool, "observability", "system_snapshots").await);
    let observability_schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM information_schema.schemata WHERE schema_name = 'observability'
         )",
    )
    .fetch_one(&pool)
    .await
    .expect("check observability schema");
    assert!(
        !observability_schema_exists,
        "0018 rollback must remove the observability schema it created"
    );

    pool.close().await;
    drop_temp_database(&admin_url, db_name).await;
}

/// Phase 4A.5: `0018` must refuse destructively once an operational
/// snapshot exists, regardless of migration ordering.
#[tokio::test]
async fn rollback_0018_refuses_and_preserves_populated_system_snapshots() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = "erytheon_rollback_test_0018_populated";
    let temp_url = create_and_migrate_temp_database(&admin_url, db_name).await;
    let pool = PgPool::connect(&temp_url).await.expect("pool");

    for down in [
        "0026_blue_ai_evidence.down.sql",
        "0025_blue_forecast_evidence_foundation.down.sql",
        "0024_daily_dense_scientific_archive.down.sql",
        "0022_scientific_snapshot_hardening.down.sql",
        "0021_snapshot_label_links.down.sql",
        "0020_snapshot_alerts.down.sql",
        "0019_scientific_snapshot_registry.down.sql",
    ] {
        let out = run_psql(&temp_url, &migrations_root().join(down));
        assert!(
            out.status.success(),
            "prerequisite rollback {down} must succeed"
        );
    }

    sqlx::query(
        "INSERT INTO observability.system_snapshots
            (captured_at, capture_date, environment, cadence, checksum)
         VALUES (NOW(), CURRENT_DATE, 'rollback_fixture', 'daily', 'fixture')",
    )
    .execute(&pool)
    .await
    .expect("insert system snapshot fixture");

    let output = run_psql(
        &temp_url,
        &migrations_root().join("0018_observability_foundation.down.sql"),
    );
    assert!(
        !output.status.success(),
        "0018 rollback must refuse when operational snapshot history exists"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("refusing destructive rollback"),
        "0018 guard must return a useful error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM observability.system_snapshots")
            .fetch_one(&pool)
            .await
            .expect("count preserved snapshot"),
        1
    );
    assert!(table_exists(&pool, "observability", "system_snapshots").await);

    pool.close().await;
    drop_temp_database(&admin_url, db_name).await;
}
