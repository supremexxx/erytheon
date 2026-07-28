//! Phase 3B.6 scientific review: H3 parent-block distribution for the
//! negative population, and a sensitivity analysis of the resolution-9-
//! to-8 `combustible` aggregation rule (`any` vs. majority vs. proportion
//! thresholds). Read-only; produces a printed report, not assertions
//! about a "correct" answer — this is measurement, not a pass/fail gate.
//! Skips if the candidate datasets or `cell_static` are not present.

use std::collections::HashMap;

use grid::{Resolution, cell_from_db};
use store::Store;

// One flat measurement-and-report function by design, matching the
// phase 3B.4 census experiment's own convention.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn h3_aggregation_and_negative_parent_distribution_report() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");

    // --- Section 6: resolution-9 -> resolution-8 aggregation sensitivity ---
    let cell_static_rows = store.all_cell_static_rows().await.expect("cell_static");
    let res8 = Resolution::try_from(8).expect("resolution 8");
    let mut children_by_parent: HashMap<u64, Vec<bool>> = HashMap::new();
    for row in &cell_static_rows {
        let combustible = row
            .features
            .get("combustible")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if let Some(parent) = row.cell.parent(res8) {
            children_by_parent
                .entry(u64::from(parent))
                .or_default()
                .push(combustible);
        }
    }
    let mut child_counts: Vec<usize> = children_by_parent.values().map(Vec::len).collect();
    child_counts.sort_unstable();
    let total_parents = child_counts.len();
    let min_children = child_counts.first().copied().unwrap_or(0);
    let max_children = child_counts.last().copied().unwrap_or(0);
    let median_children = child_counts.get(total_parents / 2).copied().unwrap_or(0);
    let single_child_parents = child_counts.iter().filter(|&&c| c == 1).count();
    let partial_coverage_parents = child_counts.iter().filter(|&&c| c > 1).count();

    println!(
        "3b6_h3_children_distribution total_parents={total_parents} min={min_children} median={median_children} max={max_children} single_child={single_child_parents} multi_child={partial_coverage_parents}"
    );

    let mut any_true = 0usize;
    let mut majority_true = 0usize;
    let mut prop25_true = 0usize;
    let mut prop50_true = 0usize;
    let mut prop75_true = 0usize;
    let mut differs_any_vs_majority = 0usize;
    let mut differs_any_vs_prop50 = 0usize;
    for children in children_by_parent.values() {
        let n = children.len();
        let true_count = children.iter().filter(|&&c| c).count();
        #[allow(clippy::cast_precision_loss)]
        let proportion = true_count as f64 / n as f64;
        let any = true_count > 0;
        let majority = proportion > 0.5;
        let p25 = proportion >= 0.25;
        let p50 = proportion >= 0.50;
        let p75 = proportion >= 0.75;
        if any {
            any_true += 1;
        }
        if majority {
            majority_true += 1;
        }
        if p25 {
            prop25_true += 1;
        }
        if p50 {
            prop50_true += 1;
        }
        if p75 {
            prop75_true += 1;
        }
        if any != majority {
            differs_any_vs_majority += 1;
        }
        if any != p50 {
            differs_any_vs_prop50 += 1;
        }
    }
    println!(
        "3b6_combustible_rule_sensitivity total_parents={total_parents} any_true={any_true} majority_true={majority_true} prop25_true={prop25_true} prop50_true={prop50_true} prop75_true={prop75_true}"
    );
    println!(
        "3b6_combustible_rule_disagreement any_vs_majority_differs={differs_any_vs_majority} any_vs_prop50_differs={differs_any_vs_prop50}"
    );

    // --- Section 5: negative population H3-parent-block (resolution 5) distribution ---
    let res5 = Resolution::try_from(5).expect("resolution 5");

    for logical_id in [
        "erytheon_human_ignition_cell_day_v1_candidate_inclusive_n2_kring2_day3",
        "erytheon_human_ignition_cell_day_v1_candidate_inclusive_n3_adaptive_geographic_quality",
    ] {
        let Ok(negative_h3s) = store.negative_h3_values_for_logical_id(logical_id).await else {
            eprintln!("skipping: could not read negatives for {logical_id}");
            continue;
        };
        if negative_h3s.is_empty() {
            eprintln!("skipping: no negative rows found for {logical_id}");
            continue;
        }
        let mut by_parent5: HashMap<u64, usize> = HashMap::new();
        for h3 in &negative_h3s {
            if let Ok(cell) = cell_from_db(*h3)
                && let Some(parent) = cell.parent(res5)
            {
                *by_parent5.entry(u64::from(parent)).or_default() += 1;
            }
        }
        let mut counts: Vec<usize> = by_parent5.values().copied().collect();
        counts.sort_unstable();
        let blocks = counts.len();
        let min = counts.first().copied().unwrap_or(0);
        let max = counts.last().copied().unwrap_or(0);
        let median = counts.get(blocks / 2).copied().unwrap_or(0);
        println!(
            "3b6_negative_h3_parent_distribution logical_id={logical_id} distinct_res5_blocks={blocks} min={min} median={median} max={max}"
        );
    }
}
