use chrono::{Duration, TimeZone as _, Utc};
use serde_json::json;
use sqlx::PgPool;
use store::{BlueForecastContext, Store};

fn url_for_database(base_url: &str, database: &str) -> String {
    let (prefix, _) = base_url
        .rsplit_once('/')
        .expect("DATABASE_URL must contain a database name");
    format!("{prefix}/{database}")
}

async fn create_database(admin_url: &str, name: &str) -> String {
    let admin = PgPool::connect(admin_url).await.expect("admin pool");
    sqlx::query(&format!("DROP DATABASE IF EXISTS {name}"))
        .execute(&admin)
        .await
        .expect("drop stale test database");
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .expect("create test database");
    admin.close().await;
    let url = url_for_database(admin_url, name);
    Store::connect(&url).await.expect("apply migrations");
    url
}

async fn drop_database(admin_url: &str, name: &str) {
    let admin = PgPool::connect(admin_url).await.expect("admin pool");
    sqlx::query(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
         WHERE datname=$1 AND pid<>pg_backend_pid()",
    )
    .bind(name)
    .execute(&admin)
    .await
    .expect("terminate test connections");
    sqlx::query(&format!("DROP DATABASE IF EXISTS {name}"))
        .execute(&admin)
        .await
        .expect("drop test database");
    admin.close().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn daily_blue_bulletin_is_complete_idempotent_private_and_immutable() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let database = "erytheon_blue_forecast_test";
    let url = create_database(&admin_url, database).await;
    let store = Store::connect(&url).await.expect("store");
    let pool = PgPool::connect(&url).await.expect("fixture pool");

    let model_id: i64 = sqlx::query_scalar(
        "INSERT INTO human_model_versions(
            train_from,train_to,validation_from,validation_to,
            train_positive_count,train_negative_count,
            validation_positive_count,validation_negative_count,
            artifact,metrics,active)
         VALUES('2029-01-01','2029-06-30','2029-07-01','2029-12-31',
            1,1,1,1,'{}','{}',TRUE) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("active model");
    let cells = [6_171_000_000_000_201_i64, 6_171_000_000_000_202_i64];
    store
        .publish_coverage_mask("operational_aoi", 8, &cells, "blue-test")
        .await
        .expect("coverage mask");
    sqlx::query(
        "INSERT INTO reference.commune_boundaries(
            insee_code,name,boundary,department_code,region_code,source_version,source_checksum)
         VALUES('31490','Saint-Jory',
            '{\"type\":\"Polygon\",\"coordinates\":[[[1.35,43.75],[1.40,43.75],[1.40,43.80],[1.35,43.80],[1.35,43.75]]]}',
            '31','76','test-v1','catalog-checksum')",
    )
    .execute(&pool)
    .await
    .expect("commune");
    for cell in cells {
        sqlx::query(
            "INSERT INTO reference.commune_h3_cells(insee_code,h3,h3_resolution)
             VALUES('31490',$1,8)",
        )
        .bind(cell)
        .execute(&pool)
        .await
        .expect("commune mapping");
    }

    let context = BlueForecastContext {
        environment: "test".to_owned(),
        application_revision: "blue-test-revision".to_owned(),
        application_image: "erytheon:blue-test".to_owned(),
        application_image_digest: format!("sha256:{}", "a".repeat(64)),
    };
    let before_slot = Utc
        .with_ymd_and_hms(2032, 8, 12, 5, 59, 0)
        .single()
        .expect("timestamp");
    assert!(
        store
            .capture_blue_daily_bulletin(before_slot, "internal-weather-source", &context)
            .await
            .expect("pre-slot no-op")
            .is_none()
    );

    let computed_at = Utc
        .with_ymd_and_hms(2032, 8, 12, 6, 15, 0)
        .single()
        .expect("timestamp");
    sqlx::query("INSERT INTO forecast_batches(computed_at,completed_at) VALUES($1,$1)")
        .bind(computed_at)
        .execute(&pool)
        .await
        .expect("forecast batch");
    for (horizon, hours, scores) in [
        ("hours_24", 24_i64, [0.70_f32, 0.80_f32]),
        ("hours_48", 48_i64, [0.66_f32, 0.72_f32]),
    ] {
        for (cell, score) in cells.into_iter().zip(scores) {
            sqlx::query(
                "INSERT INTO risk_scores(
                    h3,computed_at,horizon,score,physical,human,factors,input_date,valid_at)
                 VALUES($1,$2,$3,$4,0.6,0.4,$5,$2::date,$6)",
            )
            .bind(cell)
            .bind(computed_at)
            .bind(horizon)
            .bind(score)
            .bind(json!(["fwi", "combustibility"]))
            .bind(computed_at + Duration::hours(hours))
            .execute(&pool)
            .await
            .expect("risk score");
        }
    }

    let first = store
        .capture_blue_daily_bulletin(computed_at, "internal-weather-source", &context)
        .await
        .expect("capture")
        .expect("published bulletin");
    let replay = store
        .capture_blue_daily_bulletin(computed_at, "internal-weather-source", &context)
        .await
        .expect("replay")
        .expect("same bulletin");
    assert_eq!(first.id, replay.id);
    assert_eq!(first.status, "published");
    assert_eq!(first.model_version_id, model_id);
    assert_eq!(first.forecast_cell_count, 2);
    assert_eq!(first.mapped_cell_count, 2);
    assert_eq!(first.commune_count, 1);
    assert_eq!((first.alerts_24h, first.alerts_48h), (1, 1));
    assert_eq!(first.checksum.as_deref().map(str::len), Some(64));

    let alerts = store
        .list_blue_alerts(&first.id, None, 10)
        .await
        .expect("alerts");
    assert_eq!(alerts.len(), 2);
    assert!(
        alerts
            .iter()
            .all(|alert| alert.evaluation_status == "pending")
    );
    assert!(
        alerts
            .iter()
            .all(|alert| alert.commune_name == "Saint-Jory")
    );
    assert_eq!(
        store
            .ensure_blue_evidence_cases(&first.id, 20)
            .await
            .expect("create top selection"),
        1
    );
    assert_eq!(
        store
            .ensure_blue_evidence_cases(&first.id, 20)
            .await
            .expect("selection is idempotent"),
        0
    );
    let cases = store
        .list_blue_evidence_cases(&first.id)
        .await
        .expect("list evidence cases");
    assert_eq!(cases.len(), 1, "+24 h and +48 h share one commune case");
    assert_eq!(cases[0].daily_rank, 1);
    assert!(cases[0].alert_24h_id.is_some());
    assert!(cases[0].alert_48h_id.is_some());
    assert_eq!(
        cases[0].research_after,
        computed_at + Duration::hours(27),
        "the provisional review starts three hours after the +24 h horizon"
    );
    assert_eq!(cases[0].review_stage, "hours_24");
    assert_eq!(cases[0].stage_attempt_count, 0);
    assert!(cases[0].next_attempt_at.is_none());
    assert_eq!(cases[0].provisional_verdict, "pending");
    let serialized = serde_json::to_value(&first).expect("serialize bulletin");
    assert!(
        serialized.get("forecast_source").is_none(),
        "upstream provenance must stay internal"
    );

    let archive_bytes: (i32, i32, i32, i32) = sqlx::query_as(
        "SELECT octet_length(p95_24h),octet_length(max_24h),
                octet_length(p95_48h),octet_length(max_48h)
         FROM blue.forecast_index_archives WHERE bulletin_id=$1::uuid",
    )
    .bind(&first.id)
    .fetch_one(&pool)
    .await
    .expect("compact archive");
    assert_eq!(archive_bytes, (4, 4, 4, 4));
    let mutation =
        sqlx::query("UPDATE blue.forecast_alerts SET alert_index=0.1 WHERE bulletin_id=$1::uuid")
            .bind(&first.id)
            .execute(&pool)
            .await;
    assert!(mutation.is_err(), "published forecasts must be immutable");

    pool.close().await;
    drop(store);
    drop_database(&admin_url, database).await;
}
