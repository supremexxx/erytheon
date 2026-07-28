//! `experimental_negative_sampling`: phase 3B.4 exclusion-window comparison,
//! v2 (post-audit).
//!
//! This is a measurement, not a dataset build. It never writes to
//! `ml.dataset_*`, never calls `build_human_dataset`, and produces no
//! dataset rows. See `NEGATIVE_SAMPLING_DESIGN.md` and
//! `PHASE3B4_NEGATIVE_SAMPLING_REPORT.md` for the numbers this test
//! produced, the audit of the v1 experiment that made this v2 necessary,
//! and the recommendation drawn from them.
//!
//! The v1 experiment (300 combustible cells x one date/year, 2,100
//! candidates) measured near-zero exclusions (N0=0, N1=0, N2=1, N3=3). The
//! phase 3B.4 audit found the real cause: `public.cell_static` (the source
//! of negative candidates) is H3 resolution 9, while `fire.ignition_events`
//! (the exclusion event set) is resolution 8 — comparing `CellIndex`
//! values of different resolutions is either always unequal or always an
//! `h3o` error, so v1's candidates could essentially never register as
//! "at" an event regardless of true proximity. That bug is now fixed in
//! `dataset::negative_design::is_within_window` (resolution normalization).
//! This v2 experiment also replaces the arbitrary, uniformly sparse v1
//! population with a deliberately stratified one that guarantees both
//! near-event and far-from-event candidates, per the phase 3B.4 mission.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use chrono::{Datelike, NaiveDate};
use dataset::negative_design::{ExclusionStrategy, is_within_window};
use dataset::splits::Split;
use grid::{H3Grid, cell_from_db};
use sqlx::PgPool;
use store::{AnyCauseEventForNegativeDesign, Store};

fn url_for_database(base_url: &str, db_name: &str) -> String {
    let (prefix, _) = base_url
        .rsplit_once('/')
        .expect("DATABASE_URL must contain a database name");
    format!("{prefix}/{db_name}")
}

async fn create_and_seed_temp_database(admin_url: &str, db_name: &str) -> String {
    let admin_pool = PgPool::connect(admin_url)
        .await
        .expect("connect to admin database");
    sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("drop stale negative-sampling test database");
    sqlx::query(&format!("CREATE DATABASE {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("create negative-sampling test database");
    admin_pool.close().await;

    let temp_url = url_for_database(admin_url, db_name);
    Store::connect(&temp_url)
        .await
        .expect("migrate negative-sampling test database");
    let pool = PgPool::connect(&temp_url).await.expect("fixture pool");

    let grid8 = H3Grid::new(8).expect("resolution 8 grid");
    let event_h3 = grid::cell_to_db(
        grid8
            .cell_for_point(43.0, 2.0)
            .expect("valid event coordinate"),
    );

    seed_ignition_events(&pool, event_h3).await;
    seed_geographic_quality(&pool, event_h3).await;
    seed_combustible_cells(&pool).await;

    pool.close().await;
    temp_url
}

async fn seed_ignition_events(pool: &PgPool, event_h3: i64) {
    sqlx::query(
        "INSERT INTO ops.import_batches (
             id, source_id, batch_type, status, started_at, finished_at, parameters
         ) VALUES (
             '10000000-0000-4000-8000-000000000001',
             '00000000-0000-4000-8000-000000000011',
             'test_fixture', 'succeeded', NOW(), NOW(), '{\"fixture\":true}'
         )",
    )
    .execute(pool)
    .await
    .expect("fixture import batch");

    for (index, cause) in ["human_known", "natural_known", "unknown"]
        .into_iter()
        .enumerate()
    {
        let suffix = index + 1;
        let raw_id = format!("20000000-0000-4000-8000-{suffix:012}");
        let staging_id = format!("30000000-0000-4000-8000-{suffix:012}");
        let event_id = format!("40000000-0000-4000-8000-{suffix:012}");
        let source_record_id = format!("negative-fixture-{cause}");

        sqlx::query(
            "INSERT INTO raw.bdiff_records (
                 id, import_batch_id, source_record_id, source_line_number,
                 payload, retrieved_at, parsing_status
             ) VALUES (
                 $1::uuid, '10000000-0000-4000-8000-000000000001',
                 $2, $3, '{\"fixture\":true}', NOW(), 'parsed'
             )",
        )
        .bind(&raw_id)
        .bind(&source_record_id)
        .bind(i64::try_from(suffix).unwrap())
        .execute(pool)
        .await
        .expect("fixture raw event");

        sqlx::query(
            "INSERT INTO staging.bdiff_events_normalized (
                 id, raw_record_id, source_record_id, occurred_at,
                 municipality_source, latitude, longitude, geom_original,
                 surface_ha, cause_source, cause_category, cause_subcategory,
                 taxonomy_version, normalizer_version, validation_status,
                 validation_errors
             ) VALUES (
                 $1::uuid, $2::uuid, $3, '2022-06-15T12:00:00Z',
                 'Fixture', 43.0, 2.0, ST_SetSRID(ST_MakePoint(2.0, 43.0), 4326),
                 1.0, $4, $4, 'fixture', 'test-v1', 'test-v1', 'valid', '[]'
             )",
        )
        .bind(&staging_id)
        .bind(&raw_id)
        .bind(&source_record_id)
        .bind(cause)
        .execute(pool)
        .await
        .expect("fixture normalized event");

        sqlx::query(
            "INSERT INTO fire.ignition_events (
                 id, source_id, source_record_id, staging_event_id, occurred_at,
                 occurred_on_local, municipality_source, latitude_original,
                 longitude_original, geom_original, h3, h3_resolution, surface_ha,
                 cause_source, cause_category, cause_subcategory, taxonomy_version
             ) VALUES (
                 $1::uuid, '00000000-0000-4000-8000-000000000011', $2,
                 $3::uuid, '2022-06-15T12:00:00Z', '2022-06-15', 'Fixture',
                 43.0, 2.0, ST_SetSRID(ST_MakePoint(2.0, 43.0), 4326),
                 $4, 8, 1.0, $5, $5, 'fixture', 'test-v1'
             )",
        )
        .bind(&event_id)
        .bind(&source_record_id)
        .bind(&staging_id)
        .bind(event_h3)
        .bind(cause)
        .execute(pool)
        .await
        .expect("fixture ignition event");
    }
}

async fn seed_geographic_quality(pool: &PgPool, event_h3: i64) {
    sqlx::query(
        "INSERT INTO validation.rule_versions (
             id, logical_id, rule_type, description, parameters, code_version,
             status, checksum
         ) VALUES (
             '50000000-0000-4000-8000-000000000001',
             'negative_sampling_fixture_geography', 'geographic_quality',
             'Deterministic integration-test fixture', '{\"fixture\":true}',
             'test-v1', 'draft', 'negative-sampling-fixture-geography'
         )",
    )
    .execute(pool)
    .await
    .expect("fixture geography rule");
    sqlx::query(
        "INSERT INTO validation.coordinate_groups (
             id, rule_version_id, latitude, longitude, event_count,
             municipality_count, year_count, decimal_precision,
             repeated_coordinate, rounded_coordinate_probable, centroid_status,
             signals, logical_checksum
         ) VALUES (
             '60000000-0000-4000-8000-000000000001',
             '50000000-0000-4000-8000-000000000001',
             43.0, 2.0, 3, 1, 1, 1, TRUE, FALSE, 'not_centroid_like',
             '{\"fixture\":true}', 'negative-sampling-fixture-coordinate'
         )",
    )
    .execute(pool)
    .await
    .expect("fixture coordinate group");

    for suffix in 1..=3 {
        let event_id = format!("40000000-0000-4000-8000-{suffix:012}");
        sqlx::query(
            "INSERT INTO validation.event_geographic_quality (
                 ignition_event_id, rule_version_id, coordinate_group_id,
                 latitude_original, longitude_original, geom_original,
                 h3_original, h3_resolution, municipality_source,
                 shared_coordinate_event_count, shared_coordinate_municipality_count,
                 decimal_precision, rounded_coordinate_probable, centroid_status,
                 geographic_category, confidence, reasons, logical_checksum
             ) VALUES (
                 $1::uuid, '50000000-0000-4000-8000-000000000001',
                 '60000000-0000-4000-8000-000000000001',
                 43.0, 2.0, ST_SetSRID(ST_MakePoint(2.0, 43.0), 4326),
                 $2, 8, 'Fixture', 3, 1, 1, FALSE, 'not_centroid_like',
                 'precision_undocumented', 1.0, '[]', $3
             )",
        )
        .bind(&event_id)
        .bind(event_h3)
        .bind(format!("negative-sampling-fixture-geo-{suffix}"))
        .execute(pool)
        .await
        .expect("fixture geographic assessment");
    }
}

async fn seed_combustible_cells(pool: &PgPool) {
    let grid9 = H3Grid::new(9).expect("resolution 9 grid");
    let background_cells = [(43.0, 2.0), (44.0, 3.0), (45.0, 4.0)].map(|(lat, lon)| {
        grid::cell_to_db(
            grid9
                .cell_for_point(lat, lon)
                .expect("valid background coordinate"),
        )
    });
    for h3 in background_cells {
        sqlx::query(
            "INSERT INTO cell_static (h3, features)
             VALUES ($1, '{\"combustible\":true,\"fixture\":true}')",
        )
        .bind(h3)
        .execute(pool)
        .await
        .expect("fixture combustible cell");
    }
}

async fn drop_temp_database(admin_url: &str, db_name: &str) {
    let admin_pool = PgPool::connect(admin_url)
        .await
        .expect("connect to admin database for cleanup");
    sqlx::query(
        "SELECT pg_terminate_backend(pid)
         FROM pg_stat_activity
         WHERE datname = $1 AND pid <> pg_backend_pid()",
    )
    .bind(db_name)
    .execute(&admin_pool)
    .await
    .expect("terminate negative-sampling test connections");
    sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
        .execute(&admin_pool)
        .await
        .expect("drop negative-sampling test database");
    admin_pool.close().await;
}

/// Reads the process's current resident-set size from `/proc/self/status`,
/// in kibibytes. Linux-only (the isolated build/test container this runs
/// in is always Linux); returns `None` off Linux or if the file is
/// unavailable, in which case the caller reports memory as "not measured"
/// rather than fabricating a number.
fn resident_memory_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|value| value.parse().ok())
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Origin {
    /// A deterministic candidate placed at a known H3 grid-distance and
    /// day-offset from a real event, to verify the strategies' boundary
    /// behavior directly rather than hoping a random sample lands nearby.
    Probe,
    /// A stratified sample of combustible cells x representative dates,
    /// mostly far from any event, giving a realistic background
    /// exclusion-rate estimate.
    Background,
}

#[derive(Clone, Debug)]
struct Candidate {
    h3: i64,
    date: NaiveDate,
    origin: Origin,
}

// One long, linear measurement-and-report function by design: it produces
// a single coherent, ordered console report across all four strategies,
// which splitting into helpers would only obscure without reducing what
// it actually does.
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn experimental_negative_sampling_window_comparison() {
    dotenvy::dotenv().ok();
    let Ok(admin_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let db_name = format!("erytheon_negative_sampling_{}", std::process::id());
    let database_url = create_and_seed_temp_database(&admin_url, &db_name).await;
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
    println!(
        "experimental_negative_sampling total_events={}",
        events.len()
    );
    for cause in ["human_known", "natural_known", "unknown"] {
        let count = events.iter().filter(|e| e.cause_category == cause).count();
        println!("experimental_negative_sampling events_cause={cause} count={count}");
    }

    let grid8 = H3Grid::new(8).expect("resolution 8 grid");

    // --- Probe population: deterministic, guarantees near-event coverage ---
    // One event every 400th (by stable id order) across the full period,
    // spanning years/causes/geo-quality categories without needing all
    // ~16k events. For each probed event, place a candidate at H3 grid
    // distance {0,1,2,3,5} and day offset {0,1,2,3,5} from it (25 per
    // event) so every strategy's boundary is exercised by construction,
    // not by chance.
    let mut sorted_events = events.clone();
    sorted_events.sort_by(|a, b| {
        a.occurred_on_local
            .cmp(&b.occurred_on_local)
            .then(a.h3.cmp(&b.h3))
    });
    let probed_events: Vec<&AnyCauseEventForNegativeDesign> =
        sorted_events.iter().step_by(400).collect();
    println!(
        "experimental_negative_sampling probed_events={}",
        probed_events.len()
    );

    let mut candidates: Vec<Candidate> = Vec::new();
    for event in &probed_events {
        let Ok(event_cell) = cell_from_db(event.h3) else {
            continue;
        };
        let disk5 = grid8.neighbors_with_distance(event_cell, 5);
        for target_distance in [0u32, 1, 2, 3, 5] {
            let Some((cell, _)) = disk5.iter().find(|(_, d)| *d == target_distance) else {
                continue;
            };
            for day_offset in [0i64, 1, 2, 3, 5] {
                let Some(date) = event
                    .occurred_on_local
                    .checked_add_signed(chrono::Duration::days(day_offset))
                else {
                    continue;
                };
                candidates.push(Candidate {
                    h3: grid::cell_to_db(*cell),
                    date,
                    origin: Origin::Probe,
                });
            }
        }
    }
    let probe_count = candidates.len();

    // --- Background population: stratified, mostly far from events ---
    // 800 combustible cells (sample_combustible_cells, fixed seed) x one
    // date per quarter x every covered year = a much larger, still
    // deliberately stratified-by-year/quarter population than the v1
    // experiment's single date/year.
    let combustible_pool = store
        .sample_combustible_cells(800, 2_026_071)
        .await
        .expect("combustible pool");
    for cell in &combustible_pool {
        let h3 = grid::cell_to_db(cell.cell);
        for year in 2020..=2026 {
            if Split::for_year(year).is_none() {
                continue;
            }
            for (month, day) in [(2, 15), (5, 15), (8, 15), (11, 15)] {
                if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                    candidates.push(Candidate {
                        h3,
                        date,
                        origin: Origin::Background,
                    });
                }
            }
        }
    }
    let background_count = candidates.len() - probe_count;

    println!(
        "experimental_negative_sampling candidate_population total={} probe={} background={}",
        candidates.len(),
        probe_count,
        background_count
    );

    // Distribution of the population itself (not exclusions yet), per
    // mission section D: by year, split, month.
    let mut by_year: HashMap<i32, usize> = HashMap::new();
    let mut by_split: HashMap<&str, usize> = HashMap::new();
    let mut by_month: HashMap<u32, usize> = HashMap::new();
    for candidate in &candidates {
        *by_year.entry(candidate.date.year()).or_default() += 1;
        if let Some(split) = Split::for_year(candidate.date.year()) {
            *by_split.entry(split.as_str()).or_default() += 1;
        }
        *by_month.entry(candidate.date.month()).or_default() += 1;
    }
    let mut years: Vec<_> = by_year.keys().copied().collect();
    years.sort_unstable();
    for year in years {
        println!(
            "experimental_negative_sampling population_by_year year={} count={}",
            year, by_year[&year]
        );
    }
    let mut splits: Vec<_> = by_split.keys().copied().collect();
    splits.sort_unstable();
    for split in splits {
        println!(
            "experimental_negative_sampling population_by_split split={} count={}",
            split, by_split[split]
        );
    }

    for strategy in [
        ExclusionStrategy::N0,
        ExclusionStrategy::N1,
        ExclusionStrategy::N2,
        ExclusionStrategy::N3,
    ] {
        let mem_before = resident_memory_kb();
        let started = Instant::now();

        let mut excluded = 0usize;
        let mut excluded_by_human = 0usize;
        let mut excluded_by_natural = 0usize;
        let mut excluded_by_unknown = 0usize;
        let mut excluded_spatial_only = 0usize;
        let mut excluded_temporal_only = 0usize;
        let mut excluded_combined = 0usize;
        let mut excluded_by_year: HashMap<i32, usize> = HashMap::new();
        let mut excluded_by_split: HashMap<&str, usize> = HashMap::new();
        let mut excluded_by_month: HashMap<u32, usize> = HashMap::new();
        let mut excluded_probe = 0usize;
        let mut excluded_background = 0usize;

        for candidate in &candidates {
            let Ok(candidate_cell) = cell_from_db(candidate.h3) else {
                continue;
            };
            let mut hit = false;
            let mut hit_human = false;
            let mut hit_natural = false;
            let mut hit_unknown = false;
            let mut hit_spatial_only = false;
            let mut hit_temporal_only = false;
            let mut hit_combined = false;
            for event in &events {
                let day_gap = (event.occurred_on_local - candidate.date).num_days().abs();
                if day_gap > 3 {
                    continue;
                }
                let Ok(event_cell) = cell_from_db(event.h3) else {
                    continue;
                };
                let window = strategy.window(Some(event.geographic_category.as_str()));
                // Fail closed: an H3 distance/normalization that cannot be
                // computed is treated as "cannot rule out overlap", i.e.
                // excluded, never silently included.
                let within = is_within_window(
                    candidate_cell,
                    candidate.date,
                    event_cell,
                    event.occurred_on_local,
                    window,
                )
                .unwrap_or(true);
                if !within {
                    continue;
                }
                hit = true;
                match event.cause_category.as_str() {
                    "human_known" => hit_human = true,
                    "natural_known" => hit_natural = true,
                    "unknown" => hit_unknown = true,
                    _ => {}
                }
                let same_date = day_gap == 0;
                let same_cell = candidate_cell == event_cell;
                if same_cell && !same_date {
                    hit_temporal_only = true;
                } else if same_date && !same_cell {
                    hit_spatial_only = true;
                } else {
                    hit_combined = true;
                }
            }
            if hit {
                excluded += 1;
                if hit_human {
                    excluded_by_human += 1;
                }
                if hit_natural {
                    excluded_by_natural += 1;
                }
                if hit_unknown {
                    excluded_by_unknown += 1;
                }
                if hit_spatial_only {
                    excluded_spatial_only += 1;
                }
                if hit_temporal_only {
                    excluded_temporal_only += 1;
                }
                if hit_combined {
                    excluded_combined += 1;
                }
                *excluded_by_year.entry(candidate.date.year()).or_default() += 1;
                if let Some(split) = Split::for_year(candidate.date.year()) {
                    *excluded_by_split.entry(split.as_str()).or_default() += 1;
                }
                *excluded_by_month.entry(candidate.date.month()).or_default() += 1;
                match candidate.origin {
                    Origin::Probe => excluded_probe += 1,
                    Origin::Background => excluded_background += 1,
                }
            }
        }

        let elapsed = started.elapsed();
        let mem_after = resident_memory_kb();
        let remaining = candidates.len() - excluded;

        println!(
            "experimental_negative_sampling strategy={} candidates={} excluded={} remaining={} exclusion_rate={:.4} elapsed_ms={} mem_before_kb={:?} mem_after_kb={:?}",
            strategy.id(),
            candidates.len(),
            excluded,
            remaining,
            f64::from(u32::try_from(excluded).unwrap())
                / f64::from(u32::try_from(candidates.len()).unwrap()),
            elapsed.as_millis(),
            mem_before,
            mem_after
        );
        println!(
            "experimental_negative_sampling strategy={} excluded_by_cause human={} natural={} unknown={} (not mutually exclusive: one candidate can be excluded by more than one cause)",
            strategy.id(),
            excluded_by_human,
            excluded_by_natural,
            excluded_by_unknown
        );
        println!(
            "experimental_negative_sampling strategy={} excluded_by_locus spatial_only={} temporal_only={} combined={} (not mutually exclusive across events, but each is >=1 event of that kind)",
            strategy.id(),
            excluded_spatial_only,
            excluded_temporal_only,
            excluded_combined
        );
        println!(
            "experimental_negative_sampling strategy={} excluded_by_origin probe={}/{} background={}/{}",
            strategy.id(),
            excluded_probe,
            probe_count,
            excluded_background,
            background_count
        );
        let mut years: Vec<_> = excluded_by_year.keys().copied().collect();
        years.sort_unstable();
        for year in years {
            println!(
                "experimental_negative_sampling strategy={} excluded_by_year year={} count={}",
                strategy.id(),
                year,
                excluded_by_year[&year]
            );
        }
        let mut splits: Vec<_> = excluded_by_split.keys().copied().collect();
        splits.sort_unstable();
        for split in splits {
            println!(
                "experimental_negative_sampling strategy={} excluded_by_split split={} count={}",
                strategy.id(),
                split,
                excluded_by_split[split]
            );
        }
        let mut months: Vec<_> = excluded_by_month.keys().copied().collect();
        months.sort_unstable();
        for month in months {
            println!(
                "experimental_negative_sampling strategy={} excluded_by_month month={} count={}",
                strategy.id(),
                month,
                excluded_by_month[&month]
            );
        }
    }

    // --- FIRMS coincidence analysis (mission section F) ---
    // Never a label; only a contamination check against the background
    // population. Reported honestly against the real coverage window.
    let firms_points = store
        .firms_points_for_negative_design_check()
        .await
        .expect("firms points");
    println!(
        "experimental_negative_sampling firms_observations_available={}",
        firms_points.len()
    );
    if let (Some(min_date), Some(max_date)) = (
        firms_points.iter().map(|p| p.1).min(),
        firms_points.iter().map(|p| p.1).max(),
    ) {
        println!(
            "experimental_negative_sampling firms_coverage_min={min_date} firms_coverage_max={max_date}"
        );
        let covered_years: HashSet<i32> = firms_points.iter().map(|p| p.1.year()).collect();
        println!(
            "experimental_negative_sampling firms_covers_full_2020_2026_period={}",
            (2020..=2026).all(|year| covered_years.contains(&year))
        );
        let firms_cells: HashSet<i64> = firms_points
            .iter()
            .filter_map(|(cell, _)| grid8.cell_for_point(cell.0, cell.1).ok())
            .map(grid::cell_to_db)
            .collect();
        let background_candidates: Vec<&Candidate> = candidates
            .iter()
            .filter(|c| c.origin == Origin::Background)
            .collect();
        let coincident_cell_only = background_candidates
            .iter()
            .filter(|c| firms_cells.contains(&c.h3))
            .count();
        let coincident_cell_and_date = background_candidates
            .iter()
            .filter(|c| {
                firms_cells.contains(&c.h3) && firms_points.iter().any(|(_, date)| *date == c.date)
            })
            .count();
        println!(
            "experimental_negative_sampling firms_contamination_check background_candidates={} same_cell_ever={} same_cell_and_exact_date={}",
            background_candidates.len(),
            coincident_cell_only,
            coincident_cell_and_date
        );
    }

    drop(store);
    drop_temp_database(&admin_url, &db_name).await;
}
