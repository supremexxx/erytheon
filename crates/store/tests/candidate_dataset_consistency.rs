//! Phase 3B.5 consistency checks for the four candidate dataset versions,
//! executed as SQL assertions against whichever isolated DB `DATABASE_URL`
//! points at. Skips (does not fail) if the candidate datasets have not
//! been built there yet — this is an audit of an existing build, not a
//! builder itself.

use sqlx::PgPool;

const LOGICAL_ID_PATTERN: &str = "erytheon_human_ignition_cell_day_v1_candidate%";

// One flat sequence of independent SQL assertions by design; splitting it
// would only scatter one coherent audit across several near-duplicate
// helper functions.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn candidate_datasets_pass_all_consistency_checks() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let pool = PgPool::connect(&database_url).await.expect("pool");

    let version_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM ml.dataset_versions WHERE logical_id LIKE $1")
            .bind(LOGICAL_ID_PATTERN)
            .fetch_one(&pool)
            .await
            .expect("version count");
    if version_count == 0 {
        eprintln!("skipping: no phase 3B.5 candidate dataset versions found in this DB yet");
        return;
    }
    assert_eq!(
        version_count, 4,
        "expected exactly the four phase 3B.5 candidate variants (strict/inclusive x N2/N3)"
    );

    // count(*) = count(distinct deterministic_key), per dataset version.
    let mismatches: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT dv.logical_id, count(*)::bigint, count(DISTINCT dr.deterministic_key)::bigint
         FROM ml.dataset_rows dr JOIN ml.dataset_versions dv ON dv.id = dr.dataset_version_id
         WHERE dv.logical_id LIKE $1
         GROUP BY dv.logical_id
         HAVING count(*) <> count(DISTINCT dr.deterministic_key)",
    )
    .bind(LOGICAL_ID_PATTERN)
    .fetch_all(&pool)
    .await
    .expect("row/key count query");
    assert!(
        mismatches.is_empty(),
        "row_count must equal distinct deterministic_key count for every candidate dataset: {mismatches:?}"
    );

    // No cell-date positive also present as a negative in the same version.
    let overlaps: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ml.dataset_rows p
         JOIN ml.dataset_rows n ON n.dataset_version_id = p.dataset_version_id
             AND n.h3 = p.h3 AND n.local_date = p.local_date AND n.label = 0
         JOIN ml.dataset_versions dv ON dv.id = p.dataset_version_id
         WHERE p.label = 1 AND dv.logical_id LIKE $1",
    )
    .bind(LOGICAL_ID_PATTERN)
    .fetch_one(&pool)
    .await
    .expect("overlap query");
    assert_eq!(
        overlaps, 0,
        "no cell-date may be both a positive and a negative row in the same dataset version"
    );

    // Every row is H3 resolution 8, the dataset's canonical unit.
    let wrong_resolution: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ml.dataset_rows dr JOIN ml.dataset_versions dv ON dv.id = dr.dataset_version_id
         WHERE dv.logical_id LIKE $1 AND dr.h3_resolution <> 8",
    )
    .bind(LOGICAL_ID_PATTERN)
    .fetch_one(&pool)
    .await
    .expect("resolution query");
    assert_eq!(
        wrong_resolution, 0,
        "every candidate row must be H3 resolution 8"
    );

    // No row outside the studied period.
    let out_of_period: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ml.dataset_rows dr JOIN ml.dataset_versions dv ON dv.id = dr.dataset_version_id
         WHERE dv.logical_id LIKE $1 AND (dr.local_date < '2020-01-01' OR dr.local_date > '2026-12-31')",
    )
    .bind(LOGICAL_ID_PATTERN)
    .fetch_one(&pool)
    .await
    .expect("period query");
    assert_eq!(
        out_of_period, 0,
        "no row may fall outside 2020-01-01..2026-12-31"
    );

    // No cell-date assigned to more than one split.
    let split_conflicts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (
            SELECT dv.logical_id, dr.h3, dr.local_date
            FROM ml.dataset_rows dr JOIN ml.dataset_versions dv ON dv.id = dr.dataset_version_id
            WHERE dv.logical_id LIKE $1
            GROUP BY dv.logical_id, dr.h3, dr.local_date
            HAVING count(DISTINCT dr.split) > 1
         ) AS conflicts",
    )
    .bind(LOGICAL_ID_PATTERN)
    .fetch_one(&pool)
    .await
    .expect("split conflict query");
    assert_eq!(
        split_conflicts, 0,
        "no (h3, local_date) may be assigned to more than one split"
    );

    // Status must stay draft: this phase never finalizes anything.
    let non_draft: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ml.dataset_versions WHERE logical_id LIKE $1 AND status <> 'draft'",
    )
    .bind(LOGICAL_ID_PATTERN)
    .fetch_one(&pool)
    .await
    .expect("status query");
    assert_eq!(
        non_draft, 0,
        "every phase 3B.5 candidate dataset version must remain in draft status"
    );

    // Idempotence: at least one replay has happened (>= 2 builds per version).
    let under_replayed: Vec<(String, i64)> = sqlx::query_as(
        "SELECT dv.logical_id, count(*)::bigint
         FROM ml.dataset_builds db JOIN ml.dataset_versions dv ON dv.id = db.dataset_version_id
         WHERE dv.logical_id LIKE $1
         GROUP BY dv.logical_id
         HAVING count(*) < 2",
    )
    .bind(LOGICAL_ID_PATTERN)
    .fetch_all(&pool)
    .await
    .expect("build count query");
    assert!(
        under_replayed.is_empty(),
        "each candidate dataset version should have been rebuilt at least once to prove idempotence: {under_replayed:?}"
    );
}
