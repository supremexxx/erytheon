use sqlx::{PgPool, Row};
use store::Store;

const SOURCE_ID: &str = "00000000-0000-4000-8000-000000000009";
const ROLLBACK_SOURCE_ID: &str = "00000000-0000-4000-8000-000000000099";
const BATCH_ID: &str = "00000000-0000-4000-8000-000000000109";
const RUN_ID: &str = "00000000-0000-4000-8000-000000000209";

#[tokio::test]
async fn data_platform_foundation_is_additive_and_enforced() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };

    let store = Store::connect(&database_url)
        .await
        .expect("database should accept foundation migration");
    store
        .health_check()
        .await
        .expect("application health check should read the migrated database");
    store
        .source_statuses()
        .await
        .expect("application should still read existing public operational data");
    let pool = PgPool::connect(&database_url)
        .await
        .expect("database should accept verification connection");

    assert_schemas(&pool).await;
    assert_tables_and_columns(&pool).await;
    assert_keys_and_constraints(&pool).await;
    assert_new_objects_are_outside_public(&pool).await;
    assert_inserts_and_transactionality(&pool).await;
}

async fn assert_schemas(pool: &PgPool) {
    let expected = [
        "environment",
        "features",
        "fire",
        "human",
        "ml",
        "ops",
        "raw",
        "reference",
        "risk",
        "serving",
        "staging",
        "validation",
    ];
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT schema_name::text
         FROM information_schema.schemata
         WHERE schema_name = ANY($1)
         ORDER BY schema_name",
    )
    .bind(expected)
    .fetch_all(pool)
    .await
    .expect("foundation schemas should be queryable");
    assert_eq!(rows, expected);
}

async fn assert_tables_and_columns(pool: &PgPool) {
    let tables = sqlx::query(
        "SELECT table_schema, table_name
         FROM information_schema.tables
         WHERE (table_schema, table_name) IN (
            ('reference', 'data_sources'),
            ('ops', 'import_batches'),
            ('ops', 'pipeline_runs'),
            ('raw', 'firms_observations')
         )
         ORDER BY table_schema, table_name",
    )
    .fetch_all(pool)
    .await
    .expect("foundation tables should be queryable")
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("table_schema"),
            row.get::<String, _>("table_name"),
        )
    })
    .collect::<Vec<_>>();
    assert_eq!(
        tables,
        vec![
            ("ops".to_owned(), "import_batches".to_owned()),
            ("ops".to_owned(), "pipeline_runs".to_owned()),
            ("raw".to_owned(), "firms_observations".to_owned()),
            ("reference".to_owned(), "data_sources".to_owned()),
        ]
    );

    let expected = [
        ("ops", "import_batches", "id", "uuid"),
        ("ops", "import_batches", "records_received", "bigint"),
        (
            "ops",
            "import_batches",
            "started_at",
            "timestamp with time zone",
        ),
        ("ops", "pipeline_runs", "id", "uuid"),
        ("ops", "pipeline_runs", "parameters", "jsonb"),
        ("raw", "firms_observations", "id", "uuid"),
        ("raw", "firms_observations", "payload", "jsonb"),
        ("reference", "data_sources", "id", "uuid"),
        (
            "reference",
            "data_sources",
            "created_at",
            "timestamp with time zone",
        ),
    ];
    for (schema, table, column, data_type) in expected {
        let actual = sqlx::query_scalar::<_, String>(
            "SELECT data_type
             FROM information_schema.columns
             WHERE table_schema = $1 AND table_name = $2 AND column_name = $3",
        )
        .bind(schema)
        .bind(table)
        .bind(column)
        .fetch_one(pool)
        .await
        .expect("expected foundation column should exist");
        assert_eq!(actual, data_type, "{schema}.{table}.{column}");
    }
}

async fn assert_keys_and_constraints(pool: &PgPool) {
    let primary_keys = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM information_schema.table_constraints
         WHERE constraint_type = 'PRIMARY KEY'
           AND (table_schema, table_name) IN (
              ('reference', 'data_sources'),
              ('ops', 'import_batches'),
              ('ops', 'pipeline_runs'),
              ('raw', 'firms_observations')
           )",
    )
    .fetch_one(pool)
    .await
    .expect("primary keys should be queryable");
    assert_eq!(primary_keys, 4);

    let foreign_keys = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM information_schema.table_constraints
         WHERE constraint_type = 'FOREIGN KEY'
           AND table_schema IN ('ops', 'raw')
           AND table_name IN ('import_batches', 'pipeline_runs', 'firms_observations')",
    )
    .fetch_one(pool)
    .await
    .expect("foreign keys should be queryable");
    assert_eq!(foreign_keys, 4);

    let code_unique = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM information_schema.table_constraints
         WHERE table_schema = 'reference'
           AND table_name = 'data_sources'
           AND constraint_name = 'data_sources_code_unique'
           AND constraint_type = 'UNIQUE'",
    )
    .fetch_one(pool)
    .await
    .expect("source code uniqueness should be queryable");
    assert_eq!(code_unique, 1);
}

async fn assert_new_objects_are_outside_public(pool: &PgPool) {
    let misplaced = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_name IN (
              'data_sources',
              'import_batches',
              'pipeline_runs',
              'firms_observations'
           )",
    )
    .fetch_one(pool)
    .await
    .expect("public schema should be queryable");
    assert_eq!(misplaced, 0);
}

async fn assert_inserts_and_transactionality(pool: &PgPool) {
    let mut transaction = pool.begin().await.expect("transaction should start");
    insert_valid_foundation_graph(&mut transaction).await;
    assert_foundation_constraints(&mut transaction).await;
    transaction
        .rollback()
        .await
        .expect("test transaction should roll back");

    let persisted = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM reference.data_sources WHERE id = $1::uuid",
    )
    .bind(SOURCE_ID)
    .fetch_one(pool)
    .await
    .expect("rolled back source should be queryable");
    assert_eq!(persisted, 0);

    assert_error_transaction_rolls_back(pool).await;
}

async fn insert_valid_foundation_graph(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>) {
    sqlx::query(
        "INSERT INTO reference.data_sources
            (id, code, name, category, provider, description, base_url)
         VALUES ($1::uuid, 'firms_test', 'NASA FIRMS test', 'satellite', 'NASA', 'test', NULL)",
    )
    .bind(SOURCE_ID)
    .execute(&mut **transaction)
    .await
    .expect("source insertion should succeed");

    for (index, status) in [
        "pending",
        "running",
        "succeeded",
        "partially_succeeded",
        "failed",
        "cancelled",
    ]
    .into_iter()
    .enumerate()
    {
        let batch_id = format!("00000000-0000-4000-8000-{:012}", 300 + index);
        sqlx::query(
            "INSERT INTO ops.import_batches
                (id, source_id, batch_type, status, started_at, finished_at)
             VALUES (
                $1::uuid, $2::uuid, 'integration_test', $3,
                '2026-07-26 00:00:00+00',
                CASE WHEN $3 IN ('pending', 'running') THEN NULL
                     ELSE '2026-07-26 00:01:00+00'::timestamptz END
             )",
        )
        .bind(batch_id)
        .bind(SOURCE_ID)
        .bind(status)
        .execute(&mut **transaction)
        .await
        .expect("every supported batch status should succeed");
    }

    sqlx::query(
        "INSERT INTO ops.import_batches
            (id, source_id, batch_type, status, started_at, records_received)
         VALUES (
            $1::uuid, $2::uuid, 'integration_test', 'running',
            '2026-07-26 00:00:00+00', 1
         )",
    )
    .bind(BATCH_ID)
    .bind(SOURCE_ID)
    .execute(&mut **transaction)
    .await
    .expect("batch insertion should succeed");
    sqlx::query(
        "INSERT INTO ops.pipeline_runs
            (id, pipeline_name, pipeline_version, status, started_at, trigger_type,
             import_batch_id, parameters, metrics, code_version)
         VALUES (
            $1::uuid, 'firms_test', 'v1', 'running',
            '2026-07-26 00:00:00+00', 'manual',
            $2::uuid, '{\"days\": 1}', '{\"received\": 1}', 'test'
         )",
    )
    .bind(RUN_ID)
    .bind(BATCH_ID)
    .execute(&mut **transaction)
    .await
    .expect("pipeline run linked to a batch should succeed");
    sqlx::query(
        "INSERT INTO raw.firms_observations
            (id, import_batch_id, retrieved_at, observed_at, payload, parsing_status)
         VALUES (
            '00000000-0000-4000-8000-000000000409'::uuid,
            $1::uuid,
            '2026-07-26 00:00:00+00',
            '2026-07-25 23:59:00+00',
            '{\"latitude\": 43.0, \"longitude\": 2.0}',
            'pending'
         )",
    )
    .bind(BATCH_ID)
    .execute(&mut **transaction)
    .await
    .expect("raw FIRMS insertion should succeed");
}

async fn assert_foundation_constraints(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>) {
    assert_rejected_statement(
        transaction,
        "INSERT INTO ops.import_batches
            (id, source_id, batch_type, status, started_at, records_received)
         VALUES (
            '00000000-0000-4000-8000-000000000509'::uuid,
            '00000000-0000-4000-8000-000000000009'::uuid,
            'integration_test', 'running', NOW(), -1
         )",
    )
    .await;
    assert_rejected_statement(
        transaction,
        "INSERT INTO ops.import_batches
            (id, source_id, batch_type, status, started_at, finished_at)
         VALUES (
            '00000000-0000-4000-8000-000000000609'::uuid,
            '00000000-0000-4000-8000-000000000009'::uuid,
            'integration_test', 'succeeded',
            '2026-07-26 01:00:00+00', '2026-07-26 00:00:00+00'
         )",
    )
    .await;
    assert_rejected_statement(
        transaction,
        "INSERT INTO ops.pipeline_runs
            (id, pipeline_name, status, started_at, trigger_type)
         VALUES (
            '00000000-0000-4000-8000-000000000709'::uuid,
            'firms_test', 'unknown', NOW(), 'manual'
         )",
    )
    .await;
}

async fn assert_error_transaction_rolls_back(pool: &PgPool) {
    let mut rollback_transaction = pool.begin().await.expect("transaction should start");
    sqlx::query(
        "INSERT INTO reference.data_sources
            (id, code, name, category, provider)
         VALUES ($1::uuid, 'rollback_test', 'Rollback test', 'satellite', 'test')",
    )
    .bind(ROLLBACK_SOURCE_ID)
    .execute(&mut *rollback_transaction)
    .await
    .expect("rollback source insertion should succeed");
    assert_rejected_statement(
        &mut rollback_transaction,
        "INSERT INTO ops.import_batches
            (id, source_id, batch_type, status, started_at, records_inserted)
         VALUES (
            '00000000-0000-4000-8000-000000000809'::uuid,
            '00000000-0000-4000-8000-000000000099'::uuid,
            'rollback_test', 'running', NOW(), -1
         )",
    )
    .await;
    rollback_transaction
        .rollback()
        .await
        .expect("failed operation transaction should roll back");

    let rollback_persisted = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM reference.data_sources WHERE id = $1::uuid",
    )
    .bind(ROLLBACK_SOURCE_ID)
    .fetch_one(pool)
    .await
    .expect("rollback source should be queryable");
    assert_eq!(rollback_persisted, 0);
}

async fn assert_rejected_statement(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    statement: &str,
) {
    sqlx::query("SAVEPOINT expected_error")
        .execute(&mut **transaction)
        .await
        .expect("savepoint should be created");
    assert!(
        sqlx::query(statement)
            .execute(&mut **transaction)
            .await
            .is_err(),
        "constraint should reject statement: {statement}"
    );
    sqlx::query("ROLLBACK TO SAVEPOINT expected_error")
        .execute(&mut **transaction)
        .await
        .expect("transaction should recover after expected error");
}
