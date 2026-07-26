use std::path::Path;

use chrono::Utc;
use grid::H3Grid;
use ingest::bdiff::read_file;
use sqlx::PgPool;
use store::{BdiffImportIds, BdiffImportStart, BdiffPersistenceResult, BdiffTerminalState, Store};

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn bdiff_ingestion_preserves_lineage_rejections_and_business_idempotence() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url)
        .await
        .expect("database should accept BDIFF foundation migration");
    Store::connect(&database_url)
        .await
        .expect("BDIFF source initialization should be idempotent");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("verification connection");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM reference.data_sources WHERE code = 'bdiff'",
        )
        .fetch_one(&pool)
        .await
        .expect("source count"),
        1
    );

    let grid = H3Grid::new(8).expect("grid");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bdiff_pipeline_fixture.csv");
    let document = read_file(&fixture, grid)
        .await
        .expect("fixture should parse");
    assert_eq!(document.rows.len(), 15);

    let first = store
        .begin_bdiff_import(&start("bdiff_integration_first"))
        .await
        .expect("first batch should start");
    let first_result = store
        .persist_bdiff_import(&first, &document.rows, Utc::now(), 8)
        .await
        .expect("first batch should persist");
    assert_eq!(
        first_result,
        BdiffPersistenceResult {
            received: 15,
            raw_inserted: 14,
            staging_valid: 9,
            staging_rejected: 5,
            fire_created: 9,
            fire_already_present: 0,
            technical_duplicates: 1,
        }
    );
    store
        .finish_bdiff_import(
            &first,
            BdiffTerminalState::PartiallySucceeded,
            first_result,
            Some("One or more BDIFF rows were rejected"),
        )
        .await
        .expect("first batch should finish");
    assert_rejected_rows(&pool, &first.batch_id).await;
    assert_distinct_same_place_events(&pool).await;

    let same_batch_result = store
        .persist_bdiff_import(&first, &document.rows, Utc::now(), 8)
        .await
        .expect("same batch replay should be idempotent");
    assert_eq!(same_batch_result.raw_inserted, 0);
    assert_eq!(same_batch_result.technical_duplicates, 15);
    assert_eq!(same_batch_result.fire_created, 0);

    let second = store
        .begin_bdiff_import(&start("bdiff_integration_second"))
        .await
        .expect("second batch should start");
    let second_result = store
        .persist_bdiff_import(&second, &document.rows, Utc::now(), 8)
        .await
        .expect("new batch should preserve raw history");
    assert_eq!(second_result.raw_inserted, 14);
    assert_eq!(second_result.staging_valid, 9);
    assert_eq!(second_result.staging_rejected, 5);
    assert_eq!(second_result.fire_created, 0);
    assert_eq!(second_result.fire_already_present, 9);
    assert_eq!(second_result.technical_duplicates, 1);
    store
        .finish_bdiff_import(
            &second,
            BdiffTerminalState::PartiallySucceeded,
            second_result,
            Some("One or more BDIFF rows were rejected"),
        )
        .await
        .expect("second batch should finish");

    let empty = store
        .begin_bdiff_import(&start("bdiff_integration_empty"))
        .await
        .expect("empty batch should start");
    let empty_result = store
        .persist_bdiff_import(&empty, &[], Utc::now(), 8)
        .await
        .expect("empty batch should succeed");
    assert_eq!(empty_result, BdiffPersistenceResult::default());
    store
        .finish_bdiff_import(&empty, BdiffTerminalState::Succeeded, empty_result, None)
        .await
        .expect("empty batch should finish");

    let failed = store
        .begin_bdiff_import(&start("bdiff_integration_failed"))
        .await
        .expect("failure batch should start");
    assert_atomic_failure(&store, &pool, &document.rows[0]).await;
    store
        .finish_bdiff_import(
            &failed,
            BdiffTerminalState::Failed,
            BdiffPersistenceResult::default(),
            Some("deterministic test failure"),
        )
        .await
        .expect("failed batch should finish");

    assert_tracking(&pool, &first.batch_id, "partially_succeeded", first_result).await;
    assert_tracking(&pool, &empty.batch_id, "succeeded", empty_result).await;
    assert_tracking(
        &pool,
        &failed.batch_id,
        "failed",
        BdiffPersistenceResult::default(),
    )
    .await;
    cleanup(&pool, &[first, second, empty, failed]).await;
}

fn start(batch_type: &str) -> BdiffImportStart {
    BdiffImportStart {
        batch_type: batch_type.to_owned(),
        trigger_type: "test".to_owned(),
        parameters: serde_json::json!({
            "file_name": "bdiff_pipeline_fixture.csv",
            "h3_resolution": 8,
        }),
        pipeline_version: "test".to_owned(),
        code_version: Some("test".to_owned()),
    }
}

async fn assert_rejected_rows(pool: &PgPool, batch_id: &str) {
    let latitude = sqlx::query_as::<_, (String, f64, bool, serde_json::Value)>(
        "SELECT raw.parsing_status, staging.latitude,
                staging.geom_original IS NULL, staging.validation_errors
         FROM raw.bdiff_records AS raw
         JOIN staging.bdiff_events_normalized AS staging
           ON staging.raw_record_id = raw.id
         WHERE raw.import_batch_id = $1::uuid
           AND raw.source_record_id = 'fixture-bad-latitude'",
    )
    .bind(batch_id)
    .fetch_one(pool)
    .await
    .expect("rejected latitude");
    assert_eq!(latitude.0, "rejected");
    assert!((latitude.1 - 120.0).abs() < f64::EPSILON);
    assert!(latitude.2);
    assert!(
        latitude
            .3
            .as_array()
            .expect("errors")
            .iter()
            .any(|error| error.as_str() == Some("invalid_latitude"))
    );

    let values =
        sqlx::query_as::<_, (Option<String>, Option<f64>, Option<f64>, Option<f64>, bool)>(
            "SELECT
            MAX(raw.payload->>'occurred_at')
                FILTER (WHERE raw.source_record_id = 'fixture-bad-timestamp'),
            MAX(staging.longitude)
                FILTER (WHERE raw.source_record_id = 'fixture-bad-longitude'),
            MAX(staging.surface_ha)
                FILTER (WHERE raw.source_record_id = 'fixture-bad-surface'),
            MAX(staging.cause_source)
                FILTER (WHERE raw.source_record_id IS NULL),
            BOOL_OR(
                raw.source_record_id IS NULL
                AND staging.validation_errors ? 'missing_cause'
            )
         FROM raw.bdiff_records AS raw
         JOIN staging.bdiff_events_normalized AS staging
           ON staging.raw_record_id = raw.id
         WHERE raw.import_batch_id = $1::uuid",
        )
        .bind(batch_id)
        .fetch_one(pool)
        .await
        .expect("rejected values");
    assert_eq!(values.0.as_deref(), Some("not-a-date"));
    assert_eq!(values.1, Some(250.0));
    assert_eq!(values.2, Some(-1.0));
    assert_eq!(values.3, None);
    assert!(values.4);
}

async fn assert_distinct_same_place_events(pool: &PgPool) {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM fire.ignition_events
         WHERE source_record_id IN ('fixture-same-place-a', 'fixture-same-place-b')",
    )
    .fetch_one(pool)
    .await
    .expect("same-place count");
    assert_eq!(count, 2);
}

async fn assert_atomic_failure(store: &Store, pool: &PgPool, row: &ingest::bdiff::BdiffRow) {
    let before = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM raw.bdiff_records")
        .fetch_one(pool)
        .await
        .expect("raw count");
    let invalid = BdiffImportIds {
        batch_id: "00000000-0000-4000-8000-000000000bad".to_owned(),
        pipeline_run_id: "00000000-0000-4000-8000-000000000bad".to_owned(),
    };
    assert!(
        store
            .persist_bdiff_import(&invalid, std::slice::from_ref(row), Utc::now(), 8)
            .await
            .is_err()
    );
    let after = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM raw.bdiff_records")
        .fetch_one(pool)
        .await
        .expect("raw count");
    assert_eq!(before, after);
}

async fn assert_tracking(
    pool: &PgPool,
    batch_id: &str,
    expected_status: &str,
    expected: BdiffPersistenceResult,
) {
    let batch = sqlx::query_as::<_, (String, i64, i64, i64, i64, bool)>(
        "SELECT status, records_received, records_inserted,
                records_ignored, records_rejected, finished_at IS NOT NULL
         FROM ops.import_batches
         WHERE id = $1::uuid",
    )
    .bind(batch_id)
    .fetch_one(pool)
    .await
    .expect("batch");
    assert_eq!(batch.0, expected_status);
    assert_eq!(batch.1, i64::try_from(expected.received).expect("count"));
    assert_eq!(
        batch.2,
        i64::try_from(expected.fire_created).expect("count")
    );
    assert_eq!(
        batch.3,
        i64::try_from(expected.technical_duplicates + expected.fire_already_present)
            .expect("count")
    );
    assert_eq!(
        batch.4,
        i64::try_from(expected.staging_rejected).expect("count")
    );
    assert!(batch.5);

    let run = sqlx::query_as::<_, (String, serde_json::Value, bool)>(
        "SELECT status, metrics, finished_at IS NOT NULL
         FROM ops.pipeline_runs WHERE import_batch_id = $1::uuid",
    )
    .bind(batch_id)
    .fetch_one(pool)
    .await
    .expect("run");
    assert_eq!(run.0, expected_status);
    assert_eq!(run.1["raw_inserted"], expected.raw_inserted);
    assert!(run.2);
}

async fn cleanup(pool: &PgPool, imports: &[BdiffImportIds]) {
    let batch_ids = imports
        .iter()
        .map(|ids| ids.batch_id.clone())
        .collect::<Vec<_>>();
    sqlx::query(
        "DELETE FROM fire.ignition_events
         WHERE staging_event_id IN (
            SELECT staging.id
            FROM staging.bdiff_events_normalized AS staging
            JOIN raw.bdiff_records AS raw ON raw.id = staging.raw_record_id
            WHERE raw.import_batch_id::text = ANY($1)
         )",
    )
    .bind(&batch_ids)
    .execute(pool)
    .await
    .expect("fire cleanup");
    sqlx::query(
        "DELETE FROM staging.bdiff_events_normalized
         WHERE raw_record_id IN (
            SELECT id FROM raw.bdiff_records WHERE import_batch_id::text = ANY($1)
         )",
    )
    .bind(&batch_ids)
    .execute(pool)
    .await
    .expect("staging cleanup");
    sqlx::query("DELETE FROM raw.bdiff_records WHERE import_batch_id::text = ANY($1)")
        .bind(&batch_ids)
        .execute(pool)
        .await
        .expect("raw cleanup");
    sqlx::query("DELETE FROM ops.pipeline_runs WHERE import_batch_id::text = ANY($1)")
        .bind(&batch_ids)
        .execute(pool)
        .await
        .expect("run cleanup");
    sqlx::query("DELETE FROM ops.import_batches WHERE id::text = ANY($1)")
        .bind(&batch_ids)
        .execute(pool)
        .await
        .expect("batch cleanup");
}
