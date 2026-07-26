use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use grid::H3Grid;
use ingest::{Cadence, FetchCtx, Source, firms::FirmsSource, open_meteo::OpenMeteoForecastSource};
use risk::HeuristicV1;
use store::Store;
use tokio::{
    sync::broadcast,
    time::{MissedTickBehavior, interval},
};

use crate::{config::Config, firms_pipeline, forecast, territory::Territory};

const FORECAST_POLL_INTERVAL: Duration = Duration::from_hours(1);

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
        config, store, grid, model, territory, updates,
    ));
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
    let forecast_is_fresh = store.source_statuses().await.is_ok_and(|statuses| {
        statuses.into_iter().any(|status| {
            status.id == OpenMeteoForecastSource::ID
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
                record_success(&store, OpenMeteoForecastSource::ID, summary.anchors).await;
                tracing::info!(
                    computed_at = %summary.computed_at,
                    base_valid_at = %summary.base_valid_at,
                    anchors = summary.anchors,
                    cells = summary.cells,
                    elapsed_seconds = summary.elapsed_seconds,
                    "scheduled AROME forecast complete"
                );
            }
            Err(error) => {
                tracing::error!(source = OpenMeteoForecastSource::ID, %error, "scheduled forecast failed; continuing");
                record_error(&store, OpenMeteoForecastSource::ID, &error.to_string()).await;
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
