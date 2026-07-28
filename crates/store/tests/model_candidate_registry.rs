use serde_json::json;
use sqlx::PgPool;
use store::{
    ModelCandidateRegistration, ModelCandidateRegistrationOutcome, ModelCandidateStatus, Store,
};

fn registration(seed: i64, checksum: &str) -> ModelCandidateRegistration {
    ModelCandidateRegistration {
        model_family: "test_gbm_isotonic_v2".to_owned(),
        model_name: "test_human_ignition_propensity_v2".to_owned(),
        artifact_version: 1,
        status: ModelCandidateStatus::Inactive,
        git_commit: "0000000".to_owned(),
        dataset_logical_id: "test_dataset_logical_id".to_owned(),
        dataset_row_fingerprint: "test_fingerprint".to_owned(),
        seed,
        artifact: json!({"stub": true}),
        artifact_checksum: checksum.to_owned(),
        metrics: json!({"roc_auc": 0.9}),
        scientific_interpretation: "test interpretation".to_owned(),
        known_limitations: vec!["test limitation".to_owned()],
    }
}

#[tokio::test]
async fn registering_a_candidate_creates_exactly_one_row() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");
    let seed = 900_001;

    sqlx::query("DELETE FROM ml.model_candidate_registry WHERE model_family = $1")
        .bind("test_gbm_isotonic_v2")
        .execute(&pool)
        .await
        .expect("cleanup");

    let outcome = store
        .register_model_candidate(registration(seed, "checksum_a"))
        .await
        .expect("register");
    let ModelCandidateRegistrationOutcome::Registered(row) = outcome else {
        panic!("expected a fresh Registered outcome");
    };
    assert_eq!(row.status, "inactive");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ml.model_candidate_registry WHERE model_family = $1 AND seed = $2",
    )
    .bind("test_gbm_isotonic_v2")
    .bind(seed)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(count, 1);

    sqlx::query("DELETE FROM ml.model_candidate_registry WHERE model_family = $1")
        .bind("test_gbm_isotonic_v2")
        .execute(&pool)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn replaying_the_same_identity_and_checksum_is_idempotent() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");
    let seed = 900_002;

    sqlx::query("DELETE FROM ml.model_candidate_registry WHERE model_family = $1")
        .bind("test_gbm_isotonic_v2")
        .execute(&pool)
        .await
        .expect("cleanup");

    let first = store
        .register_model_candidate(registration(seed, "checksum_b"))
        .await
        .expect("register first time");
    let ModelCandidateRegistrationOutcome::Registered(first_row) = first else {
        panic!("expected Registered on first call");
    };

    let second = store
        .register_model_candidate(registration(seed, "checksum_b"))
        .await
        .expect("register second time");
    let ModelCandidateRegistrationOutcome::AlreadyRegistered(second_row) = second else {
        panic!("expected AlreadyRegistered on identical replay");
    };
    assert_eq!(
        first_row.id, second_row.id,
        "must be the same row, not a duplicate"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ml.model_candidate_registry WHERE model_family = $1 AND seed = $2",
    )
    .bind("test_gbm_isotonic_v2")
    .bind(seed)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(
        count, 1,
        "an identical replay must never create a second row"
    );

    sqlx::query("DELETE FROM ml.model_candidate_registry WHERE model_family = $1")
        .bind("test_gbm_isotonic_v2")
        .execute(&pool)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn the_same_logical_identity_with_a_different_checksum_is_refused() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");
    let seed = 900_003;

    sqlx::query("DELETE FROM ml.model_candidate_registry WHERE model_family = $1")
        .bind("test_gbm_isotonic_v2")
        .execute(&pool)
        .await
        .expect("cleanup");

    store
        .register_model_candidate(registration(seed, "checksum_c"))
        .await
        .expect("register first time");

    let conflict = store
        .register_model_candidate(registration(seed, "a_totally_different_checksum"))
        .await;
    assert!(
        conflict.is_err(),
        "the same logical identity must never silently accept a different checksum"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ml.model_candidate_registry WHERE model_family = $1 AND seed = $2",
    )
    .bind("test_gbm_isotonic_v2")
    .bind(seed)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(
        count, 1,
        "the conflicting attempt must not have written anything"
    );

    sqlx::query("DELETE FROM ml.model_candidate_registry WHERE model_family = $1")
        .bind("test_gbm_isotonic_v2")
        .execute(&pool)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn the_registry_can_never_represent_an_active_status() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let _store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");

    let result = sqlx::query(
        "INSERT INTO ml.model_candidate_registry (
             model_family, model_name, artifact_version, status, git_commit,
             dataset_logical_id, dataset_row_fingerprint, seed, artifact,
             artifact_checksum, metrics, scientific_interpretation, known_limitations
         ) VALUES ('x', 'y', 1, 'active', 'z', 'd', 'f', 1, '{}', 'c', '{}', 'i', '[]')",
    )
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "the CHECK constraint must reject any attempt to store status = 'active'"
    );
}

#[tokio::test]
async fn v1_human_model_versions_is_never_touched_by_candidate_registration() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");
    let seed = 900_004;

    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM human_model_versions")
        .fetch_one(&pool)
        .await
        .expect("count before");

    sqlx::query("DELETE FROM ml.model_candidate_registry WHERE model_family = $1")
        .bind("test_gbm_isotonic_v2")
        .execute(&pool)
        .await
        .expect("cleanup");
    store
        .register_model_candidate(registration(seed, "checksum_d"))
        .await
        .expect("register");

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM human_model_versions")
        .fetch_one(&pool)
        .await
        .expect("count after");
    assert_eq!(
        before, after,
        "human_model_versions row count must be unchanged"
    );

    sqlx::query("DELETE FROM ml.model_candidate_registry WHERE model_family = $1")
        .bind("test_gbm_isotonic_v2")
        .execute(&pool)
        .await
        .expect("cleanup");
}
