use chrono::Utc;
use grid::H3Grid;
use ingest::{Observation, ObservationKind};
use store::Store;

#[tokio::test]
async fn observation_insertion_is_idempotent() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url)
        .await
        .expect("database should accept migrations");
    let observed_at = Utc::now();
    let dedupe_key = format!(
        "integration-test:{}:{}",
        std::process::id(),
        observed_at
            .timestamp_nanos_opt()
            .expect("representable time")
    );
    let observation = Observation {
        source: "test".to_owned(),
        kind: ObservationKind::ActiveFire,
        cell: H3Grid::new(9)
            .expect("valid grid")
            .cell_for_point(43.2122, 2.3537)
            .expect("valid coordinate"),
        observed_at,
        payload: serde_json::json!({"fixture": true}),
        dedupe_key,
    };

    assert_eq!(
        store
            .insert_observations(std::slice::from_ref(&observation))
            .await
            .expect("first insert should succeed"),
        1
    );
    assert_eq!(
        store
            .insert_observations(&[observation])
            .await
            .expect("duplicate insert should succeed"),
        0
    );
}
