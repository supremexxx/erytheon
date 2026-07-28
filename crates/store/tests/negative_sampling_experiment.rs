//! `experimental_negative_sampling`: phase 3B.4 exclusion-window comparison.
//!
//! This is a measurement, not a dataset build. It never writes to
//! `ml.dataset_*`, never calls `build_human_dataset`, and produces no
//! dataset rows. It only counts, for each of the four candidate exclusion
//! strategies (N0-N3), how many of a small experimental candidate
//! population would be excluded. See `NEGATIVE_SAMPLING_DESIGN.md` and
//! `PHASE3B4_NEGATIVE_SAMPLING_REPORT.md` for the numbers this test
//! produced and the recommendation drawn from them.

use chrono::NaiveDate;
use dataset::negative_design::{ExclusionStrategy, is_within_window};
use dataset::splits::Split;
use grid::cell_from_db;
use store::Store;

#[tokio::test]
async fn experimental_negative_sampling_window_comparison() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");

    let period_start = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
    let period_end = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();

    let events = store
        .all_events_with_geographic_quality(period_start, period_end)
        .await
        .expect("events");
    assert!(
        events
            .iter()
            .any(|event| event.cause_category == "natural_known"),
        "the exclusion event set must include natural-cause events, not human_known only"
    );
    assert!(
        events.iter().any(|event| event.cause_category == "unknown"),
        "the exclusion event set must include unknown-cause events, not human_known only"
    );

    // Same combustible-cell sampling already used by the pilot
    // (sample_combustible_cells), same seed, so this experiment is
    // reproducible against the same population the pilot itself drew from.
    let combustible_pool = store
        .sample_combustible_cells(300, 2_026_071)
        .await
        .expect("combustible pool");

    // One representative candidate date per year per sampled cell, mirroring
    // engine::dataset_pipeline::build_human_dataset's own pilot-negative
    // candidate generation (mid-year date, not full daily coverage). This
    // is an explicit, documented limitation of the experiment, not of the
    // strategies themselves (see NEGATIVE_SAMPLING_DESIGN.md open risks).
    let candidates: Vec<(i64, NaiveDate)> = combustible_pool
        .iter()
        .flat_map(|cell| {
            let h3 = grid::cell_to_db(cell.cell);
            (2020..=2026).filter_map(move |year| {
                Split::for_year(year).map(|_| (h3, NaiveDate::from_ymd_opt(year, 6, 15).unwrap()))
            })
        })
        .collect();

    println!(
        "experimental_negative_sampling candidate_population={}",
        candidates.len()
    );

    for strategy in [
        ExclusionStrategy::N0,
        ExclusionStrategy::N1,
        ExclusionStrategy::N2,
        ExclusionStrategy::N3,
    ] {
        let mut excluded = 0usize;
        for &(candidate_h3, candidate_date) in &candidates {
            let Ok(candidate_cell) = cell_from_db(candidate_h3) else {
                continue;
            };
            let hit = events.iter().any(|event| {
                // Cheap pre-filter before any H3 distance computation: no
                // strategy's day_radius exceeds 3, and chrono's date
                // subtraction is correct across a year boundary on its own
                // (no special-casing needed), so this can never skip an
                // event that is actually within window.
                let day_gap = (event.occurred_on_local - candidate_date).num_days().abs();
                if day_gap > 3 {
                    return false;
                }
                let Ok(event_cell) = cell_from_db(event.h3) else {
                    return false;
                };
                let window = strategy.window(Some(event.geographic_category.as_str()));
                // Fail closed: an H3 distance that cannot be computed
                // (pentagon/base-cell edge case) is treated as "cannot rule
                // out overlap", i.e. excluded, never silently included.
                is_within_window(
                    candidate_cell,
                    candidate_date,
                    event_cell,
                    event.occurred_on_local,
                    window,
                )
                .unwrap_or(true)
            });
            if hit {
                excluded += 1;
            }
        }
        let remaining = candidates.len() - excluded;
        println!(
            "experimental_negative_sampling strategy={} candidates={} excluded={} remaining={} exclusion_rate={:.4}",
            strategy.id(),
            candidates.len(),
            excluded,
            remaining,
            f64::from(u32::try_from(excluded).unwrap())
                / f64::from(u32::try_from(candidates.len()).unwrap())
        );
    }
}
