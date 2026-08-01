//! Phase 4A.5: CLI entry points for automated observability snapshots.
//! Every command here only captures, verifies, compares, or dry-run
//! reports on snapshots; none of them trains, scores a candidate, or
//! activates a model.

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use store::{FreshnessThresholds, Store, SystemSnapshotContext};

use crate::{config::Config, territory::Territory};

const CODE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Matches `scheduler::SNAPSHOT_ENVIRONMENT`: this pilot runs a single
/// environment bucket. A multi-environment deployment would need this
/// sourced from configuration instead of duplicated as a constant.
const ENVIRONMENT: &str = "default";

pub async fn run_static_bundle(config: Config) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize snapshot database")?;
    let id = store
        .build_cell_static_bundle(i16::from(config.h3_resolution), CODE_VERSION)
        .await
        .context("failed to build immutable cell_static bundle")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "static_snapshot_id": id,
            "status": "active",
            "h3_resolution": config.h3_resolution,
        }))?
    );
    Ok(())
}

pub async fn run_coverage_mask(config: Config) -> anyhow::Result<()> {
    let path = config
        .territory_geojson_path
        .as_deref()
        .context("TERRITORY_GEOJSON_PATH is required")?;
    let grid = grid::H3Grid::new(config.h3_resolution).context("invalid H3 resolution")?;
    let territory = Territory::load(path, &config.territory_codes, grid)?;
    let cells = territory
        .partitions
        .iter()
        .flat_map(|partition| partition.cells.iter().copied())
        .map(grid::cell_to_db)
        .collect::<Vec<_>>();
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize snapshot database")?;
    let id = store
        .publish_coverage_mask(
            "operational_aoi",
            i16::from(config.h3_resolution),
            &cells,
            "configured_department_geojson",
        )
        .await
        .context("failed to publish coverage mask")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "coverage_mask_id": id,
            "status": "published",
            "modelable_cells": cells.len(),
            "h3_resolution": config.h3_resolution,
        }))?
    );
    Ok(())
}

pub async fn run_label_linking(
    config: Config,
    snapshot_id: String,
    mature_before: Option<DateTime<Utc>>,
    apply: bool,
    limit: i64,
) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize snapshot database")?;
    let report = store
        .link_mature_snapshot_labels(
            &snapshot_id,
            mature_before.unwrap_or_else(Utc::now),
            !apply,
            limit,
        )
        .await
        .context("failed to evaluate deferred BDIFF links")?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[derive(Clone, Debug)]
pub struct OperationalSnapshotOptions {
    pub at: Option<DateTime<Utc>>,
    pub cadence: String,
}

pub async fn run_operational_snapshot(
    config: Config,
    options: OperationalSnapshotOptions,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        matches!(options.cadence.as_str(), "daily" | "hourly" | "event"),
        "unsupported --cadence {:?}: must be daily, hourly, or event",
        options.cadence
    );
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize observability database")?;
    let captured_at = options.at.unwrap_or_else(Utc::now);
    let ctx = SystemSnapshotContext {
        application_revision: std::env::var("ERYTHEON_GIT_REVISION")
            .or_else(|_| std::env::var("ERYTHEON_APPLICATION_REVISION"))
            .ok(),
        application_image: std::env::var("ERYTHEON_IMAGE_REFERENCE")
            .or_else(|_| std::env::var("ERYTHEON_APPLICATION_IMAGE"))
            .ok(),
        application_image_digest: std::env::var("ERYTHEON_IMAGE_DIGEST").ok(),
        application_restart_count: None,
        caddy_state: std::env::var("ERYTHEON_CADDY_STATE").ok(),
        trigger_kind: Some(
            if options.at.is_some() {
                "replay"
            } else {
                "manual"
            }
            .to_owned(),
        ),
    };
    let snapshot = store
        .capture_system_snapshot(ENVIRONMENT, &options.cadence, captured_at, &ctx)
        .await
        .context("failed to capture operational snapshot")?;

    let alerts = store
        .evaluate_and_record_alerts(&snapshot, FreshnessThresholds::default())
        .await
        .context("failed to evaluate freshness/degradation rules")?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "id": snapshot.id,
            "environment": snapshot.environment,
            "cadence": snapshot.cadence,
            "capture_date": snapshot.capture_date,
            "checksum": snapshot.checksum,
            "forecast_age_seconds": snapshot.forecast_age_seconds,
            "firms_age_seconds": snapshot.firms_age_seconds,
            "active_model_count": snapshot.active_model_count,
            "candidate_status": snapshot.candidate_status,
            "new_alerts": alerts.len(),
            "code_version": CODE_VERSION,
        }))?
    );
    Ok(())
}

#[derive(Clone, Debug)]
pub struct ScientificSnapshotOptions {
    pub valid_at: DateTime<Utc>,
}

pub async fn run_scientific_snapshot(
    config: Config,
    options: ScientificSnapshotOptions,
) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize observability database")?;
    let snapshot = store
        .capture_weekly_scientific_snapshot(options.valid_at)
        .await
        .context("failed to capture scientific snapshot")?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

#[derive(Clone, Debug)]
pub struct VerifySnapshotOptions {
    pub id: String,
}

pub async fn run_verify_snapshot(
    config: Config,
    options: VerifySnapshotOptions,
) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize observability database")?;
    let report = store
        .verify_scientific_snapshot(&options.id)
        .await
        .context("failed to verify scientific snapshot")?;
    anyhow::ensure!(
        report.valid,
        "snapshot {} failed {} verification: {}",
        options.id,
        report.mode,
        report.errors.join("; ")
    );
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[derive(Clone, Debug)]
pub struct CompareSnapshotOptions {
    pub days: Vec<i64>,
}

#[derive(Serialize)]
struct CompareReport {
    days_ago: i64,
    available: bool,
    entries: Vec<store::ComparisonEntry>,
}

pub async fn run_compare_snapshots(
    config: Config,
    options: CompareSnapshotOptions,
) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize observability database")?;
    let mut reports = Vec::new();
    for days_ago in options.days {
        let comparison = store
            .compare_system_snapshots(ENVIRONMENT, days_ago)
            .await
            .with_context(|| format!("failed to compare J-{days_ago}"))?;
        reports.push(CompareReport {
            days_ago,
            available: comparison.is_some(),
            entries: comparison.unwrap_or_default(),
        });
    }
    println!("{}", serde_json::to_string_pretty(&reports)?);
    Ok(())
}

/// Reports what a future retention pass would remove, without deleting
/// anything. No deletion path is implemented in this phase (see
/// `PHASE4A5_RETENTION_POLICY.md`): this command exists so the dry-run
/// output format is already stable and reviewable before any deletion
/// logic is proposed.
pub async fn run_retention_dry_run(config: Config) -> anyhow::Result<()> {
    let store = Store::connect(&config.database_url)
        .await
        .context("failed to initialize observability database")?;
    let hourly_count = store
        .system_snapshot_history(ENVIRONMENT, "hourly", 3650)
        .await
        .context("failed to read hourly snapshot history")?
        .len();
    let daily_count = store
        .system_snapshot_history(ENVIRONMENT, "daily", 3650)
        .await
        .context("failed to read daily snapshot history")?
        .len();
    let scientific_count = store
        .list_scientific_snapshots(1000, 0)
        .await
        .context("failed to read scientific snapshot manifests")?
        .len();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "dry_run": true,
            "would_delete": 0,
            "policy": {
                "operational_hourly_retention_days": 90,
                "operational_daily_retention": "indefinite",
                "scientific_manifests_retention": "indefinite",
                "scientific_values_retention": "indefinite in this pilot; see PHASE4A5_RETENTION_POLICY.md",
            },
            "current_counts": {
                "operational_hourly": hourly_count,
                "operational_daily": daily_count,
                "scientific_manifests": scientific_count,
            },
            "note": "no automatic deletion is implemented or enabled in phase 4A.5",
        }))?
    );
    Ok(())
}
