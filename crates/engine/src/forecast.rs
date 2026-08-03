use std::{collections::HashMap, path::Path, sync::Arc, time::Instant};

use anyhow::Context as _;
use chrono::{DateTime, Datelike as _, Days, Duration, Utc};
use fwi::{
    FwiOutput, FwiState, Weather, calculate_daily, fire_weather_index, initial_spread_index,
};
use grid::{BoundingBox, CellIndex, H3Grid, LatLng};
use ingest::ecmwf_open::EcmwfOpenDataForecastSource;
use ingest::open_meteo::{
    ForecastLocation, ForecastModel, ForecastSample, OpenMeteoError, OpenMeteoForecastSource,
};
use risk::{CellFeatures, Horizon, IgnitionModel, RiskScore};
use serde::Deserialize;
use store::{ForecastFwiRow, FwiStateRow, Store};
use tokio::sync::broadcast;

const ANCHOR_STEP_DEGREES: f64 = 0.20;
const DIRECT_WEATHER_GRID_PADDING: f64 = 0.25;
const NEAREST_ANCHOR_COUNT: usize = 4;
const EXACT_ANCHOR_DISTANCE_KM: f64 = 1.0e-9;

#[derive(Clone, Debug)]
pub struct ForecastSummary {
    pub computed_at: DateTime<Utc>,
    pub base_valid_at: DateTime<Utc>,
    pub anchors: usize,
    pub cells: usize,
    pub scores_upserted: u64,
    pub elapsed_seconds: f64,
    pub source_id: &'static str,
    pub source_errors: Vec<ForecastSourceError>,
}

#[derive(Clone, Debug)]
pub struct ForecastSourceError {
    pub source_id: &'static str,
    pub message: String,
}

#[derive(Clone, Copy, Debug)]
pub struct ForecastRegion<'a> {
    pub code: &'a str,
    pub bbox: BoundingBox,
    pub cells: &'a [CellIndex],
}

/// Fetches operational weather and publishes four operational risk horizons.
#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
pub async fn recompute_forecast(
    store: &Store,
    model: &impl IgnitionModel,
    grid: H3Grid,
    aoi: BoundingBox,
    idw_power: f64,
    weather_cache_dir: &Path,
    updates: Option<&broadcast::Sender<Arc<api::RiskUpdate>>>,
) -> anyhow::Result<ForecastSummary> {
    let cells = grid
        .cells_for_bbox(aoi)
        .context("failed to cover forecast AOI")?;
    recompute_forecast_regions(
        store,
        model,
        grid,
        &[ForecastRegion {
            code: "aoi",
            bbox: aoi,
            cells: &cells,
        }],
        idw_power,
        weather_cache_dir,
        updates,
    )
    .await
}

/// Recomputes one atomic forecast batch from sequential geographic partitions.
#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
pub async fn recompute_forecast_regions(
    store: &Store,
    model: &impl IgnitionModel,
    grid: H3Grid,
    regions: &[ForecastRegion<'_>],
    idw_power: f64,
    weather_cache_dir: &Path,
    updates: Option<&broadcast::Sender<Arc<api::RiskUpdate>>>,
) -> anyhow::Result<ForecastSummary> {
    let primary = EcmwfOpenDataForecastSource::new(weather_cache_dir, combined_bbox(regions)?);
    let anchor_groups = regions
        .iter()
        .map(|region| anchor_grid(region.bbox))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let anchors = anchor_groups.iter().flatten().copied().collect::<Vec<_>>();
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_mins(2))
        .build()
        .context("failed to configure direct weather HTTP client")?;
    match primary.fetch(&client, &anchors).await {
        Ok(forecasts) => {
            let forecasts = split_forecasts(forecasts, &anchor_groups)?;
            recompute_forecast_regions_with_data(
                store,
                model,
                grid,
                regions,
                &forecasts,
                idw_power,
                updates,
                EcmwfOpenDataForecastSource::ID,
            )
            .await
        }
        Err(primary_error) => {
            tracing::warn!(
                primary = EcmwfOpenDataForecastSource::ID,
                error = %primary_error,
                "direct ECMWF acquisition failed; using normalized weather fallback"
            );
            let mut summary =
                recompute_open_meteo_with_failover(store, model, grid, regions, idw_power, updates)
                    .await?;
            summary.source_errors.insert(
                0,
                ForecastSourceError {
                    source_id: EcmwfOpenDataForecastSource::ID,
                    message: primary_error.to_string(),
                },
            );
            Ok(summary)
        }
    }
}

fn combined_bbox(regions: &[ForecastRegion<'_>]) -> anyhow::Result<BoundingBox> {
    let first = regions.first().context("forecast region list is empty")?;
    let (mut west, mut south, mut east, mut north) = (
        first.bbox.west,
        first.bbox.south,
        first.bbox.east,
        first.bbox.north,
    );
    for region in &regions[1..] {
        west = west.min(region.bbox.west);
        south = south.min(region.bbox.south);
        east = east.max(region.bbox.east);
        north = north.max(region.bbox.north);
    }
    BoundingBox::new(
        west - DIRECT_WEATHER_GRID_PADDING,
        south - DIRECT_WEATHER_GRID_PADDING,
        east + DIRECT_WEATHER_GRID_PADDING,
        north + DIRECT_WEATHER_GRID_PADDING,
    )
    .context("invalid combined forecast bounding box")
}

fn split_forecasts(
    forecasts: Vec<ForecastLocation>,
    anchor_groups: &[Vec<LatLng>],
) -> anyhow::Result<Vec<Vec<ForecastLocation>>> {
    let expected = anchor_groups.iter().map(Vec::len).sum::<usize>();
    anyhow::ensure!(
        forecasts.len() == expected,
        "direct weather source returned {} locations; expected {expected}",
        forecasts.len()
    );
    let mut forecasts = forecasts.into_iter();
    Ok(anchor_groups
        .iter()
        .map(|anchors| forecasts.by_ref().take(anchors.len()).collect())
        .collect())
}

async fn recompute_open_meteo_with_failover(
    store: &Store,
    model: &impl IgnitionModel,
    grid: H3Grid,
    regions: &[ForecastRegion<'_>],
    idw_power: f64,
    updates: Option<&broadcast::Sender<Arc<api::RiskUpdate>>>,
) -> anyhow::Result<ForecastSummary> {
    let primary = OpenMeteoForecastSource::new(ForecastModel::MeteoFrance);
    match recompute_forecast_regions_from_source(
        store, model, grid, regions, idw_power, updates, primary,
    )
    .await
    {
        Ok(summary) => Ok(summary),
        Err(primary_error) if is_weather_source_error(&primary_error) => {
            let fallback = OpenMeteoForecastSource::new(ForecastModel::Ecmwf);
            tracing::warn!(
                primary = primary.id(),
                fallback = fallback.id(),
                error = %primary_error,
                "primary weather model failed; retrying the atomic batch with fallback"
            );
            let mut summary = recompute_forecast_regions_from_source(
                store, model, grid, regions, idw_power, updates, fallback,
            )
            .await
            .context("ECMWF fallback forecast failed")?;
            summary.source_errors.push(ForecastSourceError {
                source_id: primary.id(),
                message: primary_error.to_string(),
            });
            Ok(summary)
        }
        Err(error) => Err(error),
    }
}

fn is_weather_source_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<OpenMeteoError>().is_some())
}

async fn recompute_forecast_regions_from_source(
    store: &Store,
    model: &impl IgnitionModel,
    grid: H3Grid,
    regions: &[ForecastRegion<'_>],
    idw_power: f64,
    updates: Option<&broadcast::Sender<Arc<api::RiskUpdate>>>,
    source: OpenMeteoForecastSource,
) -> anyhow::Result<ForecastSummary> {
    anyhow::ensure!(!regions.is_empty(), "forecast region list is empty");
    let client = reqwest::Client::new();
    let mut forecasts = Vec::with_capacity(regions.len());
    for region in regions {
        let anchors = anchor_grid(region.bbox)?;
        forecasts.push(
            source.fetch(&client, &anchors).await.with_context(|| {
                format!("failed to fetch weather forecast from {}", source.id())
            })?,
        );
    }
    recompute_forecast_regions_with_data(
        store,
        model,
        grid,
        regions,
        &forecasts,
        idw_power,
        updates,
        source.id(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn recompute_forecast_regions_with_data(
    store: &Store,
    model: &impl IgnitionModel,
    grid: H3Grid,
    regions: &[ForecastRegion<'_>],
    forecasts_by_region: &[Vec<ForecastLocation>],
    idw_power: f64,
    updates: Option<&broadcast::Sender<Arc<api::RiskUpdate>>>,
    source_id: &'static str,
) -> anyhow::Result<ForecastSummary> {
    anyhow::ensure!(!regions.is_empty(), "forecast region list is empty");
    anyhow::ensure!(
        forecasts_by_region.len() == regions.len(),
        "forecast data does not match region count"
    );
    let started = Instant::now();
    let computed_at = Utc::now();
    store
        .begin_forecast_batch(computed_at)
        .await
        .context("failed to register forecast batch")?;
    let mut base_valid_at = None;
    let mut anchors = 0;
    let mut cells = 0;
    let mut scores_upserted = 0;
    for (region, forecasts) in regions.iter().zip(forecasts_by_region) {
        tracing::info!(
            territory = region.code,
            cells = region.cells.len(),
            "forecast partition started"
        );
        let summary = recompute_forecast_partition(
            store,
            model,
            grid,
            *region,
            idw_power,
            computed_at,
            base_valid_at,
            updates,
            forecasts,
        )
        .await;
        let summary = match summary {
            Ok(summary) => summary,
            Err(error) => {
                if let Err(cleanup_error) = store.abort_forecast_batch(computed_at).await {
                    tracing::error!(
                        %computed_at,
                        %cleanup_error,
                        "failed forecast batch cleanup failed"
                    );
                }
                return Err(error)
                    .with_context(|| format!("forecast partition {} failed", region.code));
            }
        };
        base_valid_at = Some(summary.base_valid_at);
        anchors += summary.anchors;
        cells += region.cells.len();
        scores_upserted += summary.scores_upserted;
        tracing::info!(
            territory = region.code,
            anchors = summary.anchors,
            cells = region.cells.len(),
            "forecast partition complete"
        );
    }
    store
        .retain_forecast_batch(computed_at)
        .await
        .context("failed to publish forecast batch")?;
    Ok(ForecastSummary {
        computed_at,
        base_valid_at: base_valid_at.context("forecast produced no validity time")?,
        anchors,
        cells,
        scores_upserted,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        source_id,
        source_errors: Vec::new(),
    })
}

#[derive(Clone, Copy, Debug)]
struct PartitionSummary {
    base_valid_at: DateTime<Utc>,
    anchors: usize,
    scores_upserted: u64,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
async fn recompute_forecast_partition(
    store: &Store,
    model: &impl IgnitionModel,
    grid: H3Grid,
    region: ForecastRegion<'_>,
    idw_power: f64,
    computed_at: DateTime<Utc>,
    expected_base_valid_at: Option<DateTime<Utc>>,
    updates: Option<&broadcast::Sender<Arc<api::RiskUpdate>>>,
    forecasts: &[ForecastLocation],
) -> anyhow::Result<PartitionSummary> {
    let base_valid_at =
        expected_base_valid_at.map_or_else(|| latest_available_hour(forecasts, Utc::now()), Ok)?;
    let valid_times = Horizon::ALL.map(|horizon| base_valid_at + Duration::hours(horizon.hours()));
    let noon_times = daily_noons(valid_times)?;
    let target_samples = forecast_samples(forecasts, &valid_times)?;
    let noon_samples = forecast_samples(forecasts, &noon_times)?;
    let cells = region.cells;
    let interpolation = interpolation_weights(grid, cells, forecasts, idw_power)?;
    let previous_date = noon_times[0]
        .date_naive()
        .checked_sub_days(Days::new(1))
        .context("forecast date has no previous day")?;
    let previous_states = store
        .fwi_states(previous_date, cells)
        .await
        .context("failed to load previous FWI states")?;
    let static_rows = store
        .cell_static_rows(cells)
        .await
        .context("failed to load static forecast features")?;
    anyhow::ensure!(
        static_rows.len() == cells.len(),
        "cell_static is incomplete: expected {} rows, found {}; run `pyrorisk load-static`",
        cells.len(),
        static_rows.len()
    );
    let static_by_cell = static_rows
        .into_iter()
        .map(|row| {
            let features = serde_json::from_value::<StaticFeatures>(row.features)
                .context("invalid cell_static feature document")?;
            Ok((row.cell, features))
        })
        .collect::<anyhow::Result<HashMap<_, _>>>()?;
    let calendar = store
        .calendar_days_between(valid_times[0].date_naive(), valid_times[3].date_naive())
        .await
        .context("failed to load forecast calendar flags")?
        .into_iter()
        .map(|day| (day.date, (day.school_holiday, day.public_holiday)))
        .collect::<HashMap<_, _>>();

    let mut outputs = Vec::<[FwiOutput; 4]>::with_capacity(cells.len());
    for (index, cell) in cells.iter().copied().enumerate() {
        let target_weather = (0..4)
            .map(|horizon_index| {
                interpolate_forecast(
                    &interpolation[index],
                    &target_samples,
                    horizon_index,
                    valid_times[horizon_index],
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let noon_weather = noon_times
            .iter()
            .enumerate()
            .map(|(noon_index, valid_at)| {
                interpolate_forecast(&interpolation[index], &noon_samples, noon_index, *valid_at)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut state = previous_states
            .get(&cell)
            .copied()
            .unwrap_or_else(FwiState::default);
        let mut daily_outputs = Vec::with_capacity(noon_weather.len());
        for weather in noon_weather {
            let output = calculate_daily(weather, state).context("forecast noon FWI failed")?;
            state = output.state();
            daily_outputs.push(output);
        }
        let horizon_outputs = valid_times
            .iter()
            .zip(target_weather)
            .map(|(valid_at, weather)| {
                let daily_index = noon_times
                    .iter()
                    .rposition(|noon| noon <= valid_at)
                    .context("forecast horizon has no preceding noon state")?;
                intraday_output(daily_outputs[daily_index], weather.wind_speed_kmh)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        outputs.push(
            horizon_outputs
                .try_into()
                .map_err(|_| anyhow::anyhow!("forecast horizon output count changed"))?,
        );
    }

    let current_state_date = latest_noon(valid_times[0])?.date_naive();
    let current_states = cells
        .iter()
        .copied()
        .zip(&outputs)
        .map(|(cell, output)| FwiStateRow {
            cell,
            date: current_state_date,
            ffmc: output[0].ffmc,
            dmc: output[0].dmc,
            dc: output[0].dc,
            isi: output[0].isi,
            bui: output[0].bui,
            fwi: output[0].fwi,
        })
        .collect::<Vec<_>>();
    store
        .upsert_fwi_states(&current_states)
        .await
        .context("failed to persist current forecast FWI")?;

    let mut scores_upserted = 0;
    let mut nowcast_scores = Vec::new();
    for (horizon_index, horizon) in Horizon::ALL.into_iter().enumerate() {
        let valid_at = valid_times[horizon_index];
        let calendar_flags = calendar
            .get(&valid_at.date_naive())
            .copied()
            .unwrap_or_default();
        let fwi_rows = cells
            .iter()
            .copied()
            .zip(&outputs)
            .map(|(cell, output)| {
                forecast_fwi_row(cell, computed_at, valid_at, horizon, output[horizon_index])
            })
            .collect::<Vec<_>>();
        store
            .upsert_forecast_fwi(&fwi_rows)
            .await
            .context("failed to persist forecast FWI")?;
        let scores = cells
            .iter()
            .copied()
            .zip(&outputs)
            .map(|(cell, output)| {
                let features = static_by_cell
                    .get(&cell)
                    .context("validated static forecast cell disappeared")?;
                let mut prediction = model.score(
                    cell,
                    &CellFeatures {
                        fwi: output[horizon_index].fwi as f32,
                        hist: features.hist,
                        wui: features.wui,
                        road: features.road,
                        agri: features.agri,
                        population: features.population,
                        poi: features.poi,
                        power_line: features.power_line,
                        combustible: features.combustible,
                        date: valid_at.date_naive(),
                        school_holiday: calendar_flags.0,
                        public_holiday: calendar_flags.1,
                    },
                    computed_at,
                );
                prediction.horizon = horizon;
                prediction.valid_at = valid_at;
                Ok(prediction)
            })
            .collect::<anyhow::Result<Vec<RiskScore>>>()?;
        scores_upserted += store
            .upsert_risk_scores(valid_at.date_naive(), &scores)
            .await
            .context("failed to persist forecast risk scores")?;
        if horizon == Horizon::Nowcast {
            nowcast_scores = scores;
        }
    }
    if let Some(updates) = updates {
        let _receivers = updates.send(Arc::new(api::RiskUpdate::from_scores(&nowcast_scores)));
    }
    Ok(PartitionSummary {
        base_valid_at,
        anchors: forecasts.len(),
        scores_upserted,
    })
}

fn forecast_fwi_row(
    cell: CellIndex,
    computed_at: DateTime<Utc>,
    valid_at: DateTime<Utc>,
    horizon: Horizon,
    output: FwiOutput,
) -> ForecastFwiRow {
    ForecastFwiRow {
        cell,
        computed_at,
        valid_at,
        horizon,
        ffmc: output.ffmc,
        dmc: output.dmc,
        dc: output.dc,
        isi: output.isi,
        bui: output.bui,
        fwi: output.fwi,
    }
}

fn anchor_grid(aoi: BoundingBox) -> anyhow::Result<Vec<LatLng>> {
    let mut anchors = Vec::new();
    let mut latitude = aoi.south;
    loop {
        let mut longitude = aoi.west;
        loop {
            anchors.push(LatLng::new(latitude, longitude).context("invalid forecast anchor")?);
            if longitude >= aoi.east {
                break;
            }
            longitude = (longitude + ANCHOR_STEP_DEGREES).min(aoi.east);
        }
        if latitude >= aoi.north {
            break;
        }
        latitude = (latitude + ANCHOR_STEP_DEGREES).min(aoi.north);
    }
    Ok(anchors)
}

fn latest_available_hour(
    forecasts: &[ForecastLocation],
    target: DateTime<Utc>,
) -> anyhow::Result<DateTime<Utc>> {
    forecasts
        .first()
        .context("Open-Meteo returned no forecast location")?
        .samples
        .iter()
        .filter(|sample| sample.valid_at <= target)
        .max_by_key(|sample| sample.valid_at)
        .map(|sample| sample.valid_at)
        .context("Open-Meteo returned no forecast hour at or before now")
}

fn forecast_samples(
    forecasts: &[ForecastLocation],
    valid_times: &[DateTime<Utc>],
) -> anyhow::Result<Vec<Vec<ForecastSample>>> {
    forecasts
        .iter()
        .map(|forecast| {
            valid_times
                .iter()
                .map(|valid_at| {
                    forecast
                        .sample_at(*valid_at)
                        .with_context(|| format!("forecast hour {valid_at} is missing"))
                })
                .collect()
        })
        .collect()
}

fn daily_noons(valid_times: [DateTime<Utc>; 4]) -> anyhow::Result<Vec<DateTime<Utc>>> {
    let first = latest_noon(valid_times[0])?;
    let last = latest_noon(valid_times[3])?;
    let mut noons = Vec::new();
    let mut noon = first;
    loop {
        noons.push(noon);
        if noon == last {
            break;
        }
        noon = noon
            .checked_add_days(Days::new(1))
            .context("forecast noon date overflow")?;
    }
    Ok(noons)
}

fn latest_noon(valid_at: DateTime<Utc>) -> anyhow::Result<DateTime<Utc>> {
    let date = valid_at.date_naive();
    let noon = date
        .and_hms_opt(12, 0, 0)
        .context("invalid forecast noon")?
        .and_utc();
    if noon <= valid_at {
        Ok(noon)
    } else {
        Ok(date
            .checked_sub_days(Days::new(1))
            .context("forecast noon date underflow")?
            .and_hms_opt(12, 0, 0)
            .context("invalid previous forecast noon")?
            .and_utc())
    }
}

#[derive(Clone, Copy, Debug)]
struct AnchorWeight {
    index: usize,
    weight: f64,
}

fn interpolation_weights(
    grid: H3Grid,
    cells: &[CellIndex],
    forecasts: &[ForecastLocation],
    power: f64,
) -> anyhow::Result<Vec<Vec<AnchorWeight>>> {
    anyhow::ensure!(
        power.is_finite() && power > 0.0,
        "IDW power must be positive"
    );
    cells
        .iter()
        .map(|cell| {
            let target = grid.cell_center(*cell);
            let mut distances = forecasts
                .iter()
                .enumerate()
                .map(|(index, forecast)| (index, target.distance_km(forecast.location)))
                .collect::<Vec<_>>();
            distances.sort_by(|left, right| left.1.total_cmp(&right.1));
            if distances[0].1 <= EXACT_ANCHOR_DISTANCE_KM {
                return Ok(vec![AnchorWeight {
                    index: distances[0].0,
                    weight: 1.0,
                }]);
            }
            let weights = distances
                .into_iter()
                .take(NEAREST_ANCHOR_COUNT)
                .map(|(index, distance)| AnchorWeight {
                    index,
                    weight: distance.powf(-power),
                })
                .collect();
            Ok(weights)
        })
        .collect()
}

fn interpolate_forecast(
    weights: &[AnchorWeight],
    samples: &[Vec<ForecastSample>],
    horizon_index: usize,
    valid_at: DateTime<Utc>,
) -> anyhow::Result<Weather> {
    let mut weight_sum = 0.0;
    let mut temperature = 0.0;
    let mut humidity = 0.0;
    let mut wind = 0.0;
    let mut precipitation = 0.0;
    for anchor in weights {
        let sample = samples[anchor.index][horizon_index];
        weight_sum += anchor.weight;
        temperature += anchor.weight * sample.temperature_c;
        humidity += anchor.weight * sample.relative_humidity_pct;
        wind += anchor.weight * sample.wind_speed_kmh;
        precipitation += anchor.weight * sample.precipitation_24h_mm;
    }
    anyhow::ensure!(weight_sum > 0.0, "forecast interpolation has no weight");
    Ok(Weather {
        temperature_c: temperature / weight_sum,
        relative_humidity_pct: humidity / weight_sum,
        wind_speed_kmh: wind / weight_sum,
        precipitation_mm: precipitation / weight_sum,
        month: u8::try_from(valid_at.month()).context("forecast month does not fit u8")?,
    })
}

fn intraday_output(daily: FwiOutput, wind_speed_kmh: f64) -> anyhow::Result<FwiOutput> {
    let isi = initial_spread_index(daily.ffmc, wind_speed_kmh)
        .context("forecast intraday ISI calculation failed")?;
    let fwi =
        fire_weather_index(isi, daily.bui).context("forecast intraday FWI calculation failed")?;
    Ok(FwiOutput { isi, fwi, ..daily })
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct StaticFeatures {
    hist: f32,
    wui: f32,
    road: f32,
    agri: f32,
    #[serde(default)]
    population: f32,
    #[serde(default)]
    poi: f32,
    #[serde(default)]
    power_line: f32,
    combustible: bool,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};
    use grid::{BoundingBox, LatLng};
    use ingest::open_meteo::{ForecastLocation, ForecastSample};

    use super::{anchor_grid, latest_available_hour};

    const COORDINATE_TOLERANCE: f64 = 1.0e-9;

    #[test]
    fn anchor_grid_covers_all_aoi_edges() {
        let aoi = BoundingBox::new(1.68, 42.57, 3.26, 43.46).expect("valid AOI");
        let anchors = anchor_grid(aoi).expect("valid anchors");
        assert!(anchors.len() >= 40);
        assert!(
            anchors
                .iter()
                .any(|value| (value.lat() - aoi.south).abs() < COORDINATE_TOLERANCE)
        );
        assert!(
            anchors
                .iter()
                .any(|value| (value.lat() - aoi.north).abs() < COORDINATE_TOLERANCE)
        );
        assert!(
            anchors
                .iter()
                .any(|value| (value.lng() - aoi.west).abs() < COORDINATE_TOLERANCE)
        );
        assert!(
            anchors
                .iter()
                .any(|value| (value.lng() - aoi.east).abs() < COORDINATE_TOLERANCE)
        );
    }

    #[test]
    fn nowcast_uses_the_latest_completed_forecast_hour() {
        let location = ForecastLocation {
            location: LatLng::new(43.2, 2.3).expect("valid location"),
            samples: [21, 22]
                .map(|hour| ForecastSample {
                    valid_at: Utc.with_ymd_and_hms(2026, 7, 18, hour, 0, 0).unwrap(),
                    temperature_c: 30.0,
                    relative_humidity_pct: 25.0,
                    wind_speed_kmh: 18.0,
                    precipitation_24h_mm: 0.0,
                })
                .to_vec(),
        };
        let now = Utc.with_ymd_and_hms(2026, 7, 18, 21, 34, 0).unwrap();

        let selected = latest_available_hour(&[location], now).expect("available hour");

        assert_eq!(
            selected,
            Utc.with_ymd_and_hms(2026, 7, 18, 21, 0, 0).unwrap()
        );
    }
}
