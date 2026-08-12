use chrono::{Duration, TimeZone, Utc};
use serde_json::json;
use sqlx::PgPool;
use store::{ScientificSnapshotContext, Store};

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
#[allow(clippy::too_many_lines)]
async fn daily_dense_archive_is_complete_idempotent_and_immutable() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let cells = [6_171_000_000_000_101_i64, 6_171_000_000_000_102_i64];
    for h3 in cells {
        sqlx::query(
            "INSERT INTO public.cell_static(h3,features) VALUES ($1,$2)
             ON CONFLICT(h3) DO UPDATE SET features=EXCLUDED.features",
        )
        .bind(h3)
        .bind(json!({
            "hist":0.1,"wui":0.2,"road":0.3,"agri":0.4,"combustible":true,
            "population":1.0,"poi":2.0,"power_line":3.0,"school_zone":"A"
        }))
        .execute(&pool)
        .await
        .expect("static fixture");
    }
    store
        .build_cell_static_bundle(8, "dense-test-revision")
        .await
        .expect("static bundle");
    store
        .publish_coverage_mask("operational_aoi", 8, &cells, "dense-test")
        .await
        .expect("coverage mask");

    let valid_at = Utc
        .with_ymd_and_hms(2031, 2, 3, 12, 0, 0)
        .single()
        .expect("valid timestamp");
    let computed_at = valid_at + Duration::minutes(30);
    sqlx::query("INSERT INTO forecast_batches(computed_at,completed_at) VALUES($1,$1)")
        .bind(computed_at)
        .execute(&pool)
        .await
        .expect("forecast batch");
    for (h3, base) in cells.into_iter().zip([1.0_f64, 2.0_f64]) {
        sqlx::query(
            "INSERT INTO forecast_fwi(
                h3,computed_at,valid_at,horizon,ffmc,dmc,dc,isi,bui,fwi)
             VALUES($1,$2,$3,'nowcast',$4,$5,$6,$7,$8,$9)",
        )
        .bind(h3)
        .bind(computed_at)
        .bind(valid_at)
        .bind(base)
        .bind(base + 1.0)
        .bind(base + 2.0)
        .bind(base + 3.0)
        .bind(base + 4.0)
        .bind(base + 5.0)
        .execute(&pool)
        .await
        .expect("forecast fixture");
    }
    let context = ScientificSnapshotContext {
        environment: "test".to_owned(),
        application_revision: "dense-test-revision".to_owned(),
        application_image: "erytheon:dense-test".to_owned(),
        application_image_digest: format!("sha256:{}", "a".repeat(64)),
    };
    let blank_source = store
        .capture_daily_dense_scientific_snapshot_v2(computed_at, valid_at, "", &context)
        .await;
    assert!(blank_source.is_err(), "source provenance is mandatory");
    let stale = store
        .capture_daily_dense_scientific_snapshot_v2(
            valid_at + Duration::hours(7),
            valid_at,
            "test-weather",
            &context,
        )
        .await;
    assert!(stale.is_err(), "a stale nowcast must not be archived");
    let first = store
        .capture_daily_dense_scientific_snapshot_v2(computed_at, valid_at, "test-weather", &context)
        .await
        .expect("daily archive");
    let replay = store
        .capture_daily_dense_scientific_snapshot_v2(computed_at, valid_at, "test-weather", &context)
        .await
        .expect("idempotent replay");
    assert_eq!(first.id, replay.id);
    assert_eq!(first.status, "published");
    assert_eq!(first.cell_count_present, 2);
    assert!(first.complete);

    let bytes: (i32, i32) = sqlx::query_as(
        "SELECT octet_length(ffmc_values),octet_length(fwi_values)
         FROM observability.scientific_dense_archives WHERE snapshot_id=$1::uuid",
    )
    .bind(&first.id)
    .fetch_one(&pool)
    .await
    .expect("dense payload");
    assert_eq!(bytes, (8, 8));
    let verification = store
        .verify_scientific_snapshot(&first.id)
        .await
        .expect("verification");
    assert!(verification.valid, "{:?}", verification.errors);

    let mutation = sqlx::query(
        "UPDATE observability.scientific_dense_archives
         SET fwi_values=ffmc_values WHERE snapshot_id=$1::uuid",
    )
    .bind(&first.id)
    .execute(&pool)
    .await;
    assert!(
        mutation.is_err(),
        "published dense archive must be immutable"
    );
}
