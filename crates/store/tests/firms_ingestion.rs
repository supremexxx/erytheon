use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use grid::{BoundingBox, H3Grid};
use ingest::{FetchCtx, firms::FirmsSource};
use sqlx::PgPool;
use store::{FirmsImportStart, FirmsPersistenceResult, FirmsTerminalState, Store};

#[tokio::test]
async fn firms_ingestion_tracks_batches_and_preserves_v1_deduplication() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url)
        .await
        .expect("database should accept FIRMS support migration");
    Store::connect(&database_url)
        .await
        .expect("source initialization should be idempotent");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("verification connection");
    let source_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM reference.data_sources WHERE code = 'nasa_firms'",
    )
    .fetch_one(&pool)
    .await
    .expect("source should be queryable");
    assert_eq!(source_count, 1);
    let source_status_before =
        sqlx::query_as::<_, (DateTime<Utc>, Option<DateTime<Utc>>, i64, Option<String>)>(
            "SELECT last_run, last_success, observation_count, recent_error
         FROM public.source_status WHERE id = 'firms'",
        )
        .fetch_optional(&pool)
        .await
        .expect("source status should be queryable");

    let fetch = fixture_fetch().await;
    let keys = fetch
        .rows
        .iter()
        .map(|row| row.source_record_id.clone())
        .collect::<Vec<_>>();
    let public_ids_before = public_ids(&pool, &keys).await;

    let first = store
        .begin_firms_import(&start("integration_first"))
        .await
        .expect("first batch should start");
    let mut rows_with_duplicate = fetch.rows.clone();
    rows_with_duplicate.push(fetch.rows[0].clone());
    let first_result = store
        .persist_firms_import(&first, &rows_with_duplicate, Utc::now())
        .await
        .expect("first batch should persist atomically");
    assert_eq!(first_result.received, 6);
    assert_eq!(first_result.raw_inserted, 5);
    assert_eq!(first_result.duplicates_ignored, 1);
    store
        .finish_firms_import(&first, FirmsTerminalState::Succeeded, first_result, None)
        .await
        .expect("first batch should finish");

    let public_after_first = public_ids(&pool, &keys).await;
    let second = store
        .begin_firms_import(&start("integration_second"))
        .await
        .expect("second batch should start");
    let second_result = store
        .persist_firms_import(&second, &fetch.rows, Utc::now())
        .await
        .expect("second batch should preserve raw history");
    assert_eq!(second_result.raw_inserted, 5);
    assert_eq!(second_result.public_inserted, 0);
    assert_eq!(public_ids(&pool, &keys).await, public_after_first);
    store
        .finish_firms_import(&second, FirmsTerminalState::Succeeded, second_result, None)
        .await
        .expect("second batch should finish");

    let partial = store
        .begin_firms_import(&start("integration_partial"))
        .await
        .expect("partial batch should start");
    let mut rejected = fetch.rows[0].clone();
    rejected.source_record_id = "rejected:integration".to_owned();
    rejected.observation = None;
    rejected.observed_at = None;
    rejected.parsing_error = Some("deterministic test rejection".to_owned());
    let partial_result = store
        .persist_firms_import(&partial, &[fetch.rows[0].clone(), rejected], Utc::now())
        .await
        .expect("partial batch should retain accepted and rejected rows");
    assert_eq!(partial_result.rejected, 1);
    store
        .finish_firms_import(
            &partial,
            FirmsTerminalState::PartiallySucceeded,
            partial_result,
            Some("One or more FIRMS rows could not be normalized"),
        )
        .await
        .expect("partial batch should finish");

    let failed = store
        .begin_firms_import(&start("integration_failed"))
        .await
        .expect("failed batch should start");
    store
        .finish_firms_import(
            &failed,
            FirmsTerminalState::Failed,
            FirmsPersistenceResult::default(),
            Some("deterministic test failure"),
        )
        .await
        .expect("failed batch should finish");

    assert_tracking(&pool, &first.batch_id, "succeeded", first_result).await;
    assert_tracking(
        &pool,
        &partial.batch_id,
        "partially_succeeded",
        partial_result,
    )
    .await;
    assert_tracking(
        &pool,
        &failed.batch_id,
        "failed",
        FirmsPersistenceResult::default(),
    )
    .await;
    assert_atomic_failure(&store, &pool, &fetch.rows[0]).await;

    cleanup(
        &pool,
        &[first, second, partial, failed],
        &public_ids_before,
        &public_after_first,
        source_status_before,
    )
    .await;
}

async fn fixture_fetch() -> ingest::firms::FirmsFetch {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/firms_viirs_snpp.csv");
    FirmsSource::new(fixture)
        .fetch_batch(&FetchCtx {
            client: reqwest::Client::new(),
            aoi: BoundingBox::new(4.8, 43.3, 5.0, 43.6).expect("valid bbox"),
            grid: H3Grid::new(9).expect("valid grid"),
            days: 1,
            end_date: NaiveDate::from_ymd_opt(2023, 7, 12).expect("valid date"),
            firms_map_key: None,
            meteofrance_api_key: None,
        })
        .await
        .expect("fixture should load")
}

fn start(batch_type: &str) -> FirmsImportStart {
    let requested = NaiveDate::from_ymd_opt(2023, 7, 12)
        .expect("valid date")
        .and_hms_opt(0, 0, 0)
        .expect("valid time")
        .and_utc();
    FirmsImportStart {
        batch_type: batch_type.to_owned(),
        trigger_type: "test".to_owned(),
        requested_from: requested,
        requested_to: requested,
        parameters: serde_json::json!({
            "days": 1,
            "aoi_bbox": [4.8, 43.3, 5.0, 43.6],
        }),
        pipeline_version: "test".to_owned(),
        code_version: Some("test".to_owned()),
    }
}

async fn public_ids(pool: &PgPool, keys: &[String]) -> Vec<i64> {
    sqlx::query_scalar(
        "SELECT id FROM public.observations
         WHERE source = 'firms' AND dedupe_key = ANY($1)
         ORDER BY id",
    )
    .bind(keys)
    .fetch_all(pool)
    .await
    .expect("public FIRMS rows should be queryable")
}

async fn assert_tracking(
    pool: &PgPool,
    batch_id: &str,
    expected_status: &str,
    expected: FirmsPersistenceResult,
) {
    let row = sqlx::query_as::<_, (String, i64, i64, i64, i64, bool)>(
        "SELECT status, records_received, records_inserted,
                records_ignored, records_rejected, finished_at IS NOT NULL
         FROM ops.import_batches
         WHERE id = $1::uuid",
    )
    .bind(batch_id)
    .fetch_one(pool)
    .await
    .expect("batch should be queryable");
    assert_eq!(row.0, expected_status);
    assert_eq!(row.1, i64::try_from(expected.received).expect("count"));
    assert_eq!(
        row.2,
        i64::try_from(expected.public_inserted).expect("count")
    );
    assert_eq!(
        row.3,
        i64::try_from(expected.duplicates_ignored).expect("count")
    );
    assert_eq!(row.4, i64::try_from(expected.rejected).expect("count"));
    assert!(row.5);

    let run = sqlx::query_as::<_, (String, bool, bool)>(
        "SELECT status, import_batch_id::text = $1, finished_at IS NOT NULL
         FROM ops.pipeline_runs
         WHERE import_batch_id = $1::uuid",
    )
    .bind(batch_id)
    .fetch_one(pool)
    .await
    .expect("linked pipeline run should be queryable");
    assert_eq!(run.0, expected_status);
    assert!(run.1);
    assert!(run.2);
}

async fn assert_atomic_failure(store: &Store, pool: &PgPool, row: &ingest::firms::FirmsRow) {
    let raw_before = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM raw.firms_observations")
        .fetch_one(pool)
        .await
        .expect("raw count");
    let invalid_ids = store::FirmsImportIds {
        batch_id: "00000000-0000-4000-8000-000000000bad".to_owned(),
        pipeline_run_id: "00000000-0000-4000-8000-000000000bad".to_owned(),
    };
    assert!(
        store
            .persist_firms_import(&invalid_ids, std::slice::from_ref(row), Utc::now())
            .await
            .is_err()
    );
    let raw_after = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM raw.firms_observations")
        .fetch_one(pool)
        .await
        .expect("raw count");
    assert_eq!(raw_before, raw_after);
}

async fn cleanup(
    pool: &PgPool,
    imports: &[store::FirmsImportIds],
    public_before: &[i64],
    public_after_first: &[i64],
    source_status_before: Option<(DateTime<Utc>, Option<DateTime<Utc>>, i64, Option<String>)>,
) {
    let batch_ids = imports
        .iter()
        .map(|ids| ids.batch_id.clone())
        .collect::<Vec<_>>();
    sqlx::query("DELETE FROM raw.firms_observations WHERE import_batch_id::text = ANY($1)")
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
    let inserted_ids = public_after_first
        .iter()
        .filter(|id| !public_before.contains(id))
        .copied()
        .collect::<Vec<_>>();
    sqlx::query("DELETE FROM public.observations WHERE id = ANY($1)")
        .bind(inserted_ids)
        .execute(pool)
        .await
        .expect("public cleanup");
    if let Some((last_run, last_success, observation_count, recent_error)) = source_status_before {
        sqlx::query(
            "INSERT INTO public.source_status
                (id, last_run, last_success, observation_count, recent_error)
             VALUES ('firms', $1, $2, $3, $4)
             ON CONFLICT (id) DO UPDATE SET
                last_run = EXCLUDED.last_run,
                last_success = EXCLUDED.last_success,
                observation_count = EXCLUDED.observation_count,
                recent_error = EXCLUDED.recent_error",
        )
        .bind(last_run)
        .bind(last_success)
        .bind(observation_count)
        .bind(recent_error)
        .execute(pool)
        .await
        .expect("source status restore");
    } else {
        sqlx::query("DELETE FROM public.source_status WHERE id = 'firms'")
            .execute(pool)
            .await
            .expect("source status cleanup");
    }
}
