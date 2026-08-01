use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use store::Store;

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
async fn static_bundle_is_deterministic_active_and_immutable() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    sqlx::query(
        "INSERT INTO public.cell_static(h3,features) VALUES ($1,$2)
         ON CONFLICT(h3) DO UPDATE SET features=EXCLUDED.features",
    )
    .bind(6_171_000_000_000_001_i64)
    .bind(json!({
        "hist":0.0,"wui":0.0,"road":0.0,"agri":0.0,"combustible":false,
        "population":0.0,"poi":0.0,"power_line":0.0,"school_zone":"C"
    }))
    .execute(&pool)
    .await
    .expect("static fixture");
    let first = store
        .build_cell_static_bundle(8, "test-revision")
        .await
        .expect("bundle");
    let replay = store
        .build_cell_static_bundle(8, "test-revision")
        .await
        .expect("replay");
    assert_eq!(first, replay);
    let manifest: (String, String, i64) = sqlx::query_as(
        "SELECT status,logical_checksum,cell_count FROM features.feature_snapshots WHERE id=$1::uuid",
    )
    .bind(&first)
    .fetch_one(&pool)
    .await
    .expect("manifest");
    assert_eq!(manifest.0, "active");
    assert_eq!(manifest.1.len(), 64);
    assert!(manifest.2 >= 1);
    let mutation = sqlx::query(
        "UPDATE features.feature_snapshot_values SET features=features||'{\"hist\":1}'::jsonb
         WHERE snapshot_id=$1::uuid",
    )
    .bind(&first)
    .execute(&pool)
    .await;
    assert!(mutation.is_err(), "active bundle values must be immutable");
}

#[tokio::test]
async fn coverage_mask_is_deterministic_and_frozen_after_publication() {
    let Some((store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let cells = [6_171_000_000_000_011_i64, 6_171_000_000_000_012_i64];
    let first = store
        .publish_coverage_mask("test_operational_aoi", 8, &cells, "test")
        .await
        .expect("mask");
    let replay = store
        .publish_coverage_mask("test_operational_aoi", 8, &cells, "test")
        .await
        .expect("mask replay");
    assert_eq!(first, replay);
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM observability.coverage_mask_cells WHERE mask_id=$1::uuid",
    )
    .bind(&first)
    .fetch_one(&pool)
    .await
    .expect("mask count");
    assert_eq!(count, 2);
    let mutation =
        sqlx::query("DELETE FROM observability.coverage_mask_cells WHERE mask_id=$1::uuid")
            .bind(&first)
            .execute(&pool)
            .await;
    assert!(mutation.is_err(), "published mask cells must be immutable");
}

#[tokio::test]
async fn legacy_scientific_manifest_keeps_v1_traceability_classification() {
    let Some((_store, pool)) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let logical_id = format!(
        "test-legacy-snapshot-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let row: (i16, String) = sqlx::query_as(
        "INSERT INTO observability.scientific_snapshots(
            logical_id,snapshot_type,resolution_h3,valid_at,captured_at,
            feature_schema_version,transform_version,cell_count_expected,
            storage_kind,storage_location,status,temporal_classification)
         VALUES($1,'metadata_only',8,NOW(),NOW(),'v1','v1',0,
                'metadata_only','legacy-test','building','current_snapshot_applied_historically')
         RETURNING contract_version,traceability_status",
    )
    .bind(logical_id)
    .fetch_one(&pool)
    .await
    .expect("legacy manifest");
    assert_eq!(row, (1, "legacy_incomplete".to_owned()));
}
