use std::sync::Arc;
use std::time::Duration;

use chrono::{Datelike, Duration as ChronoDuration, Utc, Weekday};
use grid::H3Grid;
use ingest::{
    Cadence, FetchCtx, Source,
    firms::FirmsSource,
    open_meteo::{ForecastModel, OpenMeteoForecastSource},
};
use risk::HeuristicV1;
use store::{FreshnessThresholds, Store, SystemSnapshotContext};
use tokio::{
    sync::broadcast,
    time::{MissedTickBehavior, interval},
};

use crate::{config::Config, firms_pipeline, forecast, territory::Territory};

const FORECAST_POLL_INTERVAL: Duration = Duration::from_hours(1);
const DAY: Duration = Duration::from_hours(24);
const WEEK: Duration = Duration::from_hours(24 * 7);
/// 02:15 UTC: chosen to fall well outside `poll_forecast`'s on-the-hour
/// runs, the FIRMS poll window, and the documented VPS backup timers
/// (`deploy/oracle/systemd/pyrorisk-*.timer`), per
/// `PHASE4A5_SNAPSHOT_ARCHITECTURE_DECISION.md` §12 of the phase spec.
const DAILY_SNAPSHOT_HOUR_UTC: u32 = 2;
const DAILY_SNAPSHOT_MINUTE_UTC: u32 = 15;
/// Monday, immediately after the daily snapshot slot: the weekly
/// scientific pilot piggybacks on a day already known to be quiet.
const WEEKLY_SNAPSHOT_WEEKDAY: Weekday = Weekday::Mon;
const WEEKLY_SNAPSHOT_HOUR_UTC: u32 = 3;
/// Matches `snapshot_pipeline::ENVIRONMENT`: this pilot runs a single
/// environment bucket. A multi-environment deployment would need this
/// sourced from configuration instead of duplicated as a constant.
const SNAPSHOT_ENVIRONMENT: &str = "default";

pub fn spawn(
    config: Config,
    store: Store,
    grid: H3Grid,
    model: HeuristicV1,
    territory: Option<Arc<Territory>>,
    updates: broadcast::Sender<Arc<api::RiskUpdate>>,
) {
    tokio::spawn(poll_firms(config.clone(), store.clone(), grid));
    tokio::spawn(poll_forecast(
        config,
        store.clone(),
        grid,
        model,
        territory,
        updates,
    ));
    tokio::spawn(snapshot_operational_hourly(store.clone()));
    tokio::spawn(snapshot_operational_daily(store.clone()));
    tokio::spawn(snapshot_scientific_weekly(store));
}

async fn poll_firms(config: Config, store: Store, grid: H3Grid) {
    let source = FirmsSource::new(config.firms_fixture_path.clone());
    let mut ticker = source_ticker(&source);
    loop {
        ticker.tick().await;
        let context = fetch_context(&config, grid, Utc::now().date_naive());
        match firms_pipeline::run(
            &store,
            &source,
            &context,
            firms_pipeline::FirmsTrigger::Scheduler,
        )
        .await
        {
            Ok(result) => {
                tracing::info!(
                    import_batch_id = %result.ids.batch_id,
                    pipeline_run_id = %result.ids.pipeline_run_id,
                    fetched = result.persistence.received,
                    inserted = result.persistence.public_inserted,
                    "scheduled FIRMS poll complete"
                );
            }
            Err(error) => {
                tracing::error!(source = source.id(), %error, "scheduled source poll failed; continuing");
            }
        }
    }
}

async fn poll_forecast(
    config: Config,
    store: Store,
    grid: H3Grid,
    model: HeuristicV1,
    territory: Option<Arc<Territory>>,
    updates: broadcast::Sender<Arc<api::RiskUpdate>>,
) {
    let mut ticker = interval(FORECAST_POLL_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let forecast_sources = [ForecastModel::MeteoFrance, ForecastModel::Ecmwf]
        .map(|model| OpenMeteoForecastSource::new(model).id());
    let forecast_is_fresh = store.source_statuses().await.is_ok_and(|statuses| {
        statuses.into_iter().any(|status| {
            forecast_sources.contains(&status.id.as_str())
                && status.last_success.is_some_and(|last_success| {
                    let age = Utc::now().signed_duration_since(last_success);
                    age >= ChronoDuration::zero() && age < ChronoDuration::minutes(55)
                })
        })
    });
    if forecast_is_fresh {
        ticker.tick().await;
        tracing::info!("recent AROME forecast retained after restart");
    }
    loop {
        ticker.tick().await;
        let result = if let Some(territory) = &territory {
            let regions = territory
                .partitions
                .iter()
                .map(|partition| forecast::ForecastRegion {
                    code: &partition.code,
                    bbox: partition.bbox,
                    cells: &partition.cells,
                })
                .collect::<Vec<_>>();
            forecast::recompute_forecast_regions(
                &store,
                &model,
                grid,
                &regions,
                config.weather_idw_power,
                Some(&updates),
            )
            .await
        } else {
            forecast::recompute_forecast(
                &store,
                &model,
                grid,
                config.aoi_bbox,
                config.weather_idw_power,
                Some(&updates),
            )
            .await
        };
        match result {
            Ok(summary) => {
                if let Some(primary_error) = &summary.primary_error {
                    record_error(&store, forecast_sources[0], primary_error).await;
                }
                record_success(&store, summary.source_id, summary.anchors).await;
                tracing::info!(
                    source = summary.source_id,
                    computed_at = %summary.computed_at,
                    base_valid_at = %summary.base_valid_at,
                    anchors = summary.anchors,
                    cells = summary.cells,
                    elapsed_seconds = summary.elapsed_seconds,
                    "scheduled AROME forecast complete"
                );
            }
            Err(error) => {
                tracing::error!(%error, "scheduled weather forecast failed; continuing");
                record_error(&store, forecast_sources[0], &error.to_string()).await;
                record_error(&store, forecast_sources[1], &error.to_string()).await;
            }
        }
    }
}

fn source_ticker(source: &impl Source) -> tokio::time::Interval {
    let Cadence::Poll(cadence) = source.cadence() else {
        unreachable!("scheduler only accepts pollable sources");
    };
    let mut ticker = interval(cadence);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticker
}

fn fetch_context(config: &Config, grid: H3Grid, end_date: chrono::NaiveDate) -> FetchCtx {
    FetchCtx {
        client: reqwest::Client::new(),
        aoi: config.aoi_bbox,
        grid,
        days: 1,
        end_date,
        firms_map_key: config.firms_map_key.clone(),
        meteofrance_api_key: config.meteofrance_api_key.clone(),
    }
}

async fn record_success(store: &Store, source: &str, count: usize) {
    if let Err(error) = store.record_source_success(source, count).await {
        tracing::error!(source, %error, "failed to record source success");
    }
}

async fn record_error(store: &Store, source: &str, message: &str) {
    if let Err(error) = store.record_source_error(source, message).await {
        tracing::error!(source, %error, "failed to record source error");
    }
}

/// Duration until the next occurrence of `hour:minute` UTC (today if
/// still ahead, otherwise tomorrow). A snapshot outage never blocks the
/// main service: any failure here is logged and the loop simply waits
/// for its next tick.
fn duration_until_next_utc_time(hour: u32, minute: u32) -> Duration {
    let now = Utc::now();
    let today_target = now
        .date_naive()
        .and_hms_opt(hour, minute, 0)
        .expect("valid hour/minute constants");
    let target = if now.naive_utc() < today_target {
        today_target
    } else {
        today_target + ChronoDuration::days(1)
    };
    (target - now.naive_utc())
        .to_std()
        .unwrap_or(Duration::from_secs(0))
}

/// Duration until the next occurrence of `weekday` at `hour:00` UTC.
fn duration_until_next_utc_weekday(weekday: Weekday, hour: u32) -> Duration {
    duration_until_next_utc_weekday_from(Utc::now(), weekday, hour)
}

fn duration_until_next_utc_weekday_from(
    now: chrono::DateTime<Utc>,
    weekday: Weekday,
    hour: u32,
) -> Duration {
    let mut candidate = now
        .date_naive()
        .and_hms_opt(hour, 0, 0)
        .expect("valid hour constant");
    while candidate.weekday() != weekday || candidate <= now.naive_utc() {
        candidate += ChronoDuration::days(1);
    }
    (candidate - now.naive_utc())
        .to_std()
        .unwrap_or(Duration::from_secs(0))
}

async fn capture_operational_snapshot(store: &Store, cadence: &str) {
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
        trigger_kind: Some("scheduler".to_owned()),
    };
    match store
        .capture_system_snapshot(SNAPSHOT_ENVIRONMENT, cadence, Utc::now(), &ctx)
        .await
    {
        Ok(snapshot) => {
            tracing::info!(
                cadence,
                snapshot_id = snapshot.id,
                checksum = %snapshot.checksum,
                "operational snapshot captured"
            );
            match store
                .evaluate_and_record_alerts(&snapshot, FreshnessThresholds::default())
                .await
            {
                Ok(alerts) if !alerts.is_empty() => {
                    tracing::warn!(count = alerts.len(), "new observability alerts recorded");
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(%error, "failed to evaluate observability alert rules");
                }
            }
        }
        Err(error) => {
            tracing::error!(cadence, %error, "operational snapshot capture failed; continuing");
        }
    }
}

async fn snapshot_operational_hourly(store: Store) {
    let mut ticker = interval(Duration::from_hours(1));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        capture_operational_snapshot(&store, "hourly").await;
    }
}

async fn snapshot_operational_daily(store: Store) {
    tokio::time::sleep(duration_until_next_utc_time(
        DAILY_SNAPSHOT_HOUR_UTC,
        DAILY_SNAPSHOT_MINUTE_UTC,
    ))
    .await;
    let mut ticker = interval(DAY);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        capture_operational_snapshot(&store, "daily").await;
        ticker.tick().await;
    }
}

async fn snapshot_scientific_weekly(store: Store) {
    tokio::time::sleep(duration_until_next_utc_weekday(
        WEEKLY_SNAPSHOT_WEEKDAY,
        WEEKLY_SNAPSHOT_HOUR_UTC,
    ))
    .await;
    let mut ticker = interval(WEEK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        match store.capture_weekly_scientific_snapshot(Utc::now()).await {
            Ok(snapshot) => {
                tracing::info!(
                    snapshot_id = %snapshot.id,
                    status = %snapshot.status,
                    cell_count_present = snapshot.cell_count_present,
                    missing_count = snapshot.missing_count,
                    "scientific snapshot pilot captured"
                );
            }
            Err(error) => {
                tracing::error!(%error, "scientific snapshot capture failed; continuing");
            }
        }
        ticker.tick().await;
    }
}

#[cfg(test)]
mod snapshot_schedule_tests {
    use chrono::{TimeZone as _, Utc, Weekday};

    use super::duration_until_next_utc_weekday_from;

    #[test]
    fn weekly_snapshot_targets_monday_0300_utc() {
        let friday = Utc.with_ymd_and_hms(2026, 7, 31, 13, 42, 0).unwrap();
        let delay = duration_until_next_utc_weekday_from(friday, Weekday::Mon, 3);
        assert_eq!(delay.as_secs(), 61 * 3600 + 18 * 60);
    }

    #[test]
    fn restart_after_monday_slot_targets_next_week_without_double_fire() {
        let after_slot = Utc.with_ymd_and_hms(2026, 8, 3, 3, 0, 1).unwrap();
        let delay = duration_until_next_utc_weekday_from(after_slot, Weekday::Mon, 3);
        assert_eq!(delay.as_secs(), 7 * 24 * 3600 - 1);
    }
}
