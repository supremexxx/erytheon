//! A fallback-chain source that a poll cycle did not need to try (because
//! a higher-priority source already succeeded) must not keep showing a
//! stale error from a past failure forever. `clear_stale_source_error`
//! removes that message without fabricating a success the source never
//! had.

use store::Store;

async fn connect() -> Option<Store> {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").ok()?;
    Some(
        Store::connect(&database_url)
            .await
            .expect("database should accept connections and migrations"),
    )
}

#[tokio::test]
async fn clearing_a_stale_error_preserves_last_success_but_removes_the_message() {
    let Some(store) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    let id = "test_fallback_source_clear";

    store
        .record_source_error(id, "ECMWF fallback forecast failed")
        .await
        .expect("record error");
    let before = store
        .source_statuses()
        .await
        .expect("read statuses")
        .into_iter()
        .find(|status| status.id == id)
        .expect("row exists after recording an error");
    assert_eq!(
        before.recent_error.as_deref(),
        Some("ECMWF fallback forecast failed")
    );
    assert!(before.last_success.is_none());

    store
        .clear_stale_source_error(id)
        .await
        .expect("clear stale error");
    let after = store
        .source_statuses()
        .await
        .expect("read statuses")
        .into_iter()
        .find(|status| status.id == id)
        .expect("row still exists");
    assert_eq!(after.recent_error, None, "the stale message must be gone");
    assert!(
        after.last_success.is_none(),
        "clearing an error must never fabricate a success this source never had"
    );
    assert_eq!(
        after.last_run, before.last_run,
        "clearing an error must not touch last_run either"
    );
}

#[tokio::test]
async fn clearing_a_source_with_no_recorded_error_is_a_harmless_no_op() {
    let Some(store) = connect().await else {
        eprintln!("skipping: DATABASE_URL not configured");
        return;
    };
    // A source id that has never been recorded at all: clearing it must
    // not create a row out of nothing.
    store
        .clear_stale_source_error("test_never_recorded_source")
        .await
        .expect("clear on an unknown id must not error");
    let found = store
        .source_statuses()
        .await
        .expect("read statuses")
        .into_iter()
        .any(|status| status.id == "test_never_recorded_source");
    assert!(!found, "clearing must never create a phantom source row");
}
