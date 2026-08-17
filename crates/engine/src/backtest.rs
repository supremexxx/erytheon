use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    path::Path,
    time::Instant,
};

use anyhow::Context as _;
use chrono::{DateTime, Datelike as _, Days, NaiveDate, Utc};
use chrono_tz::Europe::Paris;
use fwi::{FwiState, Weather, calculate_daily};
use grid::{CellIndex, H3Grid, LatLng};
use ingest::{
    fire_history::FireHistoryPayload,
    meteo_archive::{ARCHIVE_STATIONS, MeteoArchive},
    meteo_france::WeatherStationObservation,
};
use risk::{CellFeatures, IgnitionModel};
use serde::Deserialize;
use store::{HistoricalIgnition, Store};

const REPORT_PATH: &str = "out/backtest_report.md";
const HISTORY_KERNEL_DISTANCE: u32 = 2;
const AUC_BINS: usize = 1_000;
const FALSE_NEGATIVE_LIMIT: usize = 10;
const EXACT_STATION_DISTANCE_KM: f64 = 1.0e-9;

/// Immutable settings for one retrospective replay.
pub struct BacktestOptions<'a> {
    pub grid: H3Grid,
    pub cells: &'a [CellIndex],
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub weather_path: &'a Path,
    pub weather_idw_power: f64,
    pub warmup_days: u16,
}

/// Headline measurements written to the Markdown report.
#[derive(Clone, Debug)]
pub struct BacktestSummary {
    pub days: usize,
    pub warmup_days: usize,
    pub cells_per_day: usize,
    pub ignitions: usize,
    pub approximate_auc: Option<f64>,
    pub top_five_hits: usize,
    pub top_ten_hits: usize,
    pub elapsed_seconds: f64,
}

/// Replays historical weather without persisting FWI or risk-score rows.
#[allow(clippy::too_many_lines)]
pub async fn run(
    store: &Store,
    model: &impl IgnitionModel,
    options: BacktestOptions<'_>,
) -> anyhow::Result<BacktestSummary> {
    anyhow::ensure!(options.from <= options.to, "--from must not follow --to");
    anyhow::ensure!(
        !options.cells.is_empty(),
        "backtest AOI contains no H3 cells"
    );
    let started = Instant::now();
    let replay_from = options
        .from
        .checked_sub_days(Days::new(u64::from(options.warmup_days)))
        .context("backtest warm-up date overflow")?;
    let archive = MeteoArchive::new(options.weather_path);
    let weather = archive
        .load(replay_from, options.to)
        .context("failed to load historical SYNOP weather")?;
    let static_rows = store
        .cell_static_rows(options.cells)
        .await
        .context("failed to load backtest static features")?;
    anyhow::ensure!(
        static_rows.len() == options.cells.len(),
        "cell_static is incomplete: expected {} rows, found {}; run `pyrorisk load-static`",
        options.cells.len(),
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
        .calendar_days_between(options.from, options.to)
        .await
        .context("failed to load calendar flags")?
        .into_iter()
        .map(|day| (day.date, (day.school_holiday, day.public_holiday)))
        .collect::<HashMap<_, _>>();
    let historical = store
        .historical_ignitions_until(options.to)
        .await
        .context("failed to load ignition history")?;
    let aoi = options.cells.iter().copied().collect::<HashSet<_>>();
    let mut ignitions = historical
        .into_iter()
        .filter(|ignition| aoi.contains(&ignition.cell))
        .map(TaggedIgnition::try_from)
        .collect::<anyhow::Result<Vec<_>>>()?;
    ignitions.sort_by_key(|ignition| ignition.occurred_at);
    let daily_ignitions = evaluation_ignitions(&ignitions, options.from, options.to);
    let evaluation_count = daily_ignitions.values().map(Vec::len).sum::<usize>();
    anyhow::ensure!(
        evaluation_count > 0,
        "ignition_history contains no ignition in the requested interval; run `pyrorisk load-static`"
    );

    let distances = station_distances(options.grid, options.cells)?;
    let mut states = vec![FwiState::default(); options.cells.len()];
    let mut history_raw = HashMap::<CellIndex, f32>::new();
    let mut history_cursor = 0;
    let mut auc = ApproximateAuc::default();
    let mut top_five_hits = 0;
    let mut top_ten_hits = 0;
    let mut false_negatives = Vec::new();
    let mut date = replay_from;
    loop {
        while ignitions
            .get(history_cursor)
            .is_some_and(|ignition| ignition.local_date < date)
        {
            add_history_kernel(
                options.grid,
                &aoi,
                ignitions[history_cursor].cell,
                &mut history_raw,
            );
            history_cursor += 1;
        }
        let history_maximum = history_raw.values().copied().fold(0.0_f32, f32::max);
        let stations = weather
            .get(&date)
            .context("validated archive date disappeared")?;
        let station_slots = station_slots(stations);
        let month = u8::try_from(date.month()).context("month does not fit in u8")?;
        let scored_day = date >= options.from;
        let calendar_flags = calendar.get(&date).copied().unwrap_or_default();
        let computed_at = DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_hms_opt(12, 0, 0)
                .context("invalid noon timestamp")?,
            Utc,
        );
        let positives = daily_ignitions
            .get(&date)
            .map(|values| {
                values
                    .iter()
                    .map(|value| value.cell)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let mut scores = Vec::with_capacity(options.cells.len());

        for (index, cell) in options.cells.iter().copied().enumerate() {
            let daily_weather = interpolate_weather(
                &distances[index],
                &station_slots,
                options.weather_idw_power,
                month,
            )?;
            let output = calculate_daily(daily_weather, states[index])
                .context("daily FWI calculation failed")?;
            states[index] = output.state();
            if !scored_day {
                continue;
            }
            let static_features = static_by_cell
                .get(&cell)
                .context("validated static cell disappeared")?;
            let hist = if history_maximum > 0.0 {
                history_raw.get(&cell).copied().unwrap_or(0.0) / history_maximum
            } else {
                0.0
            };
            #[allow(clippy::cast_possible_truncation)]
            let prediction = model.score(
                cell,
                &CellFeatures {
                    fwi: output.fwi as f32,
                    hist,
                    wui: static_features.wui,
                    road: static_features.road,
                    agri: static_features.agri,
                    population: static_features.population,
                    poi: static_features.poi,
                    power_line: static_features.power_line,
                    combustible: static_features.combustible,
                    date,
                    school_holiday: calendar_flags.0,
                    public_holiday: calendar_flags.1,
                },
                computed_at,
            );
            auc.observe(prediction.score, positives.contains(&cell));
            scores.push((cell, prediction.score));
        }

        if let Some(actual) = daily_ignitions.get(&date) {
            scores.sort_unstable_by(|left, right| {
                right
                    .1
                    .total_cmp(&left.1)
                    .then_with(|| left.0.cmp(&right.0))
            });
            let positive_cells = actual
                .iter()
                .map(|ignition| ignition.cell)
                .collect::<HashSet<_>>();
            let ranks = scores
                .iter()
                .enumerate()
                .filter(|(_, (cell, _))| positive_cells.contains(cell))
                .map(|(index, (cell, score))| (*cell, (index + 1, *score)))
                .collect::<HashMap<_, _>>();
            let top_five_limit = percentage_limit(scores.len(), 5);
            let top_ten_limit = percentage_limit(scores.len(), 10);
            for ignition in actual {
                let (rank, risk_value) = ranks[&ignition.cell];
                top_five_hits += usize::from(rank <= top_five_limit);
                top_ten_hits += usize::from(rank <= top_ten_limit);
                if rank > top_ten_limit {
                    false_negatives.push(FalseNegative {
                        date,
                        cell: ignition.cell,
                        municipality: ignition.municipality.clone(),
                        source: ignition.source.clone(),
                        score: risk_value,
                        rank,
                        cell_count: scores.len(),
                    });
                }
            }
        }
        tracing::debug!(%date, "backtest day complete");
        if date == options.to {
            break;
        }
        date = date.succ_opt().context("backtest date overflow")?;
    }

    false_negatives.sort_by(|left, right| {
        right
            .rank
            .cmp(&left.rank)
            .then_with(|| left.score.total_cmp(&right.score))
    });
    false_negatives.truncate(FALSE_NEGATIVE_LIMIT);
    let days = usize::try_from((options.to - options.from).num_days() + 1)
        .context("backtest duration does not fit usize")?;
    let summary = BacktestSummary {
        days,
        warmup_days: usize::from(options.warmup_days),
        cells_per_day: options.cells.len(),
        ignitions: evaluation_count,
        approximate_auc: auc.value(),
        top_five_hits,
        top_ten_hits,
        elapsed_seconds: started.elapsed().as_secs_f64(),
    };
    write_report(
        Path::new(REPORT_PATH),
        &summary,
        options.from,
        options.to,
        replay_from,
        options.weather_path,
        calendar.len(),
        &false_negatives,
    )
    .await?;
    Ok(summary)
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct StaticFeatures {
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

#[derive(Clone, Debug)]
struct TaggedIgnition {
    cell: CellIndex,
    occurred_at: DateTime<Utc>,
    local_date: NaiveDate,
    source: String,
    municipality: String,
}

impl TryFrom<HistoricalIgnition> for TaggedIgnition {
    type Error = anyhow::Error;

    fn try_from(value: HistoricalIgnition) -> Result<Self, Self::Error> {
        let payload = serde_json::from_value::<FireHistoryPayload>(value.payload)
            .context("invalid ignition_history payload")?;
        Ok(Self {
            cell: value.cell,
            occurred_at: value.occurred_at,
            local_date: value.occurred_at.with_timezone(&Paris).date_naive(),
            source: value.source,
            municipality: payload.municipality,
        })
    }
}

fn evaluation_ignitions(
    ignitions: &[TaggedIgnition],
    from: NaiveDate,
    to: NaiveDate,
) -> HashMap<NaiveDate, Vec<&TaggedIgnition>> {
    let mut grouped = HashMap::<NaiveDate, Vec<&TaggedIgnition>>::new();
    for ignition in ignitions {
        if (from..=to).contains(&ignition.local_date) {
            grouped
                .entry(ignition.local_date)
                .or_default()
                .push(ignition);
        }
    }
    grouped
}

fn station_distances(grid: H3Grid, cells: &[CellIndex]) -> anyhow::Result<Vec<[f64; 4]>> {
    let locations = ARCHIVE_STATIONS
        .map(|station| LatLng::new(station.latitude, station.longitude))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .context("invalid archive station coordinate")?;
    Ok(cells
        .iter()
        .map(|cell| {
            let target = grid.cell_center(*cell);
            std::array::from_fn(|index| target.distance_km(locations[index]))
        })
        .collect())
}

fn station_slots(
    stations: &[WeatherStationObservation],
) -> [Option<&WeatherStationObservation>; 4] {
    std::array::from_fn(|index| {
        stations
            .iter()
            .find(|station| station.station_id == ARCHIVE_STATIONS[index].id)
    })
}

fn interpolate_weather(
    distances: &[f64; 4],
    stations: &[Option<&WeatherStationObservation>; 4],
    power: f64,
    month: u8,
) -> anyhow::Result<Weather> {
    anyhow::ensure!(
        power.is_finite() && power > 0.0,
        "IDW power must be positive"
    );
    if let Some((_, station)) = distances
        .iter()
        .zip(stations)
        .find(|(distance, station)| **distance <= EXACT_STATION_DISTANCE_KM && station.is_some())
    {
        return Ok(weather_from_station(
            station.context("station disappeared")?,
            month,
        ));
    }
    let mut weight_sum = 0.0;
    let mut temperature = 0.0;
    let mut humidity = 0.0;
    let mut wind = 0.0;
    let mut precipitation = 0.0;
    for (distance, station) in distances.iter().zip(stations) {
        let Some(station) = station else {
            continue;
        };
        let weight = distance.powf(-power);
        weight_sum += weight;
        temperature += weight * station.temperature_c;
        humidity += weight * station.relative_humidity_pct;
        wind += weight * station.wind_speed_kmh;
        precipitation += weight * station.precipitation_24h_mm;
    }
    anyhow::ensure!(weight_sum > 0.0, "no complete archive station is available");
    Ok(Weather {
        temperature_c: temperature / weight_sum,
        relative_humidity_pct: humidity / weight_sum,
        wind_speed_kmh: wind / weight_sum,
        precipitation_mm: precipitation / weight_sum,
        month,
    })
}

fn weather_from_station(station: &WeatherStationObservation, month: u8) -> Weather {
    Weather {
        temperature_c: station.temperature_c,
        relative_humidity_pct: station.relative_humidity_pct,
        wind_speed_kmh: station.wind_speed_kmh,
        precipitation_mm: station.precipitation_24h_mm,
        month,
    }
}

fn add_history_kernel(
    grid: H3Grid,
    aoi: &HashSet<CellIndex>,
    ignition: CellIndex,
    values: &mut HashMap<CellIndex, f32>,
) {
    for (cell, distance) in grid.neighbors_with_distance(ignition, HISTORY_KERNEL_DISTANCE) {
        if !aoi.contains(&cell) {
            continue;
        }
        let weight = match distance {
            0 => 1.0,
            1 => 0.5,
            _ => 0.25,
        };
        *values.entry(cell).or_default() += weight;
    }
}

const fn percentage_limit(cell_count: usize, percentage: usize) -> usize {
    cell_count.saturating_mul(percentage).div_ceil(100)
}

#[derive(Clone, Copy, Debug, Default)]
struct AucBin {
    positives: f64,
    negatives: f64,
}

#[derive(Clone, Debug)]
struct ApproximateAuc {
    bins: Vec<AucBin>,
    positives: f64,
    negatives: f64,
}

impl Default for ApproximateAuc {
    fn default() -> Self {
        Self {
            bins: vec![AucBin::default(); AUC_BINS + 1],
            positives: 0.0,
            negatives: 0.0,
        }
    }
}

impl ApproximateAuc {
    fn observe(&mut self, score: f32, positive: bool) {
        let index = score_bin(score);
        if positive {
            self.bins[index].positives += 1.0;
            self.positives += 1.0;
        } else {
            self.bins[index].negatives += 1.0;
            self.negatives += 1.0;
        }
    }

    fn value(&self) -> Option<f64> {
        if self.positives == 0.0 || self.negatives == 0.0 {
            return None;
        }
        let mut lower_negatives = 0.0;
        let mut concordance = 0.0;
        for bin in &self.bins {
            concordance += bin.positives * (lower_negatives + 0.5 * bin.negatives);
            lower_negatives += bin.negatives;
        }
        Some(concordance / (self.positives * self.negatives))
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn score_bin(score: f32) -> usize {
    (score.clamp(0.0, 1.0) * 1_000.0).round() as usize
}

#[derive(Clone, Debug)]
struct FalseNegative {
    date: NaiveDate,
    cell: CellIndex,
    municipality: String,
    source: String,
    score: f32,
    rank: usize,
    cell_count: usize,
}

#[allow(clippy::too_many_arguments)]
async fn write_report(
    path: &Path,
    summary: &BacktestSummary,
    from: NaiveDate,
    to: NaiveDate,
    replay_from: NaiveDate,
    weather_path: &Path,
    calendar_days: usize,
    false_negatives: &[FalseNegative],
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("failed to create backtest output directory")?;
    }
    let auc = summary
        .approximate_auc
        .map_or_else(|| "N/A".to_owned(), |value| format!("{value:.4}"));
    let top_five = percentage(summary.top_five_hits, summary.ignitions);
    let top_ten = percentage(summary.top_ten_hits, summary.ignitions);
    let mut report = format!(
        "# FireSift Backtest Report\n\n\
         Generated at: {}\n\n\
         ## Scope\n\n\
         - Period: `{from}` through `{to}` (inclusive)\n\
         - FWI warm-up: `{replay_from}` through the day before `{from}` ({} days)\n\
         - Replayed days: {}\n\
         - H3 cells per day: {}\n\
         - Evaluated day-cells: {}\n\
         - Observed ignitions in AOI: {}\n\
         - Weather archive: `{}`\n\
         - Calendar days explicitly available: {calendar_days}/{}\n\
         - Runtime: {:.2} seconds\n\n\
         ## Metrics\n\n\
         | Metric | Result |\n\
         | --- | ---: |\n\
         | Approximate ROC AUC (1,000 score bins) | {auc} |\n\
         | Ignitions inside top 5% of cells | {}/{} ({top_five:.1}%) |\n\
         | Ignitions inside top 10% of cells | {}/{} ({top_ten:.1}%) |\n\n\
         ## Worst False Negatives\n\n",
        Utc::now().to_rfc3339(),
        summary.warmup_days,
        summary.days,
        summary.cells_per_day,
        summary.days.saturating_mul(summary.cells_per_day),
        summary.ignitions,
        weather_path.display(),
        summary.days,
        summary.elapsed_seconds,
        summary.top_five_hits,
        summary.ignitions,
        summary.top_ten_hits,
        summary.ignitions,
    );
    if false_negatives.is_empty() {
        report.push_str("No ignition ranked below the top 10%.\n");
    } else {
        report.push_str(
            "| Date | Municipality | Source | H3 | Score | Rank percentile |\n\
             | --- | --- | --- | --- | ---: | ---: |\n",
        );
        for item in false_negatives {
            let percentile = percentage(item.rank, item.cell_count);
            let _ = writeln!(
                report,
                "| {} | {} | {} | `{}` | {:.4} | {:.2}% |",
                item.date,
                markdown_cell(&item.municipality),
                markdown_cell(&item.source),
                item.cell,
                item.score,
                percentile
            );
        }
    }
    report.push_str(
        "\n## Method and Limitations\n\n\
         - The production `HeuristicV1` model and configured coefficients are used unchanged.\n\
         - Daily FWI state starts from the standard initialization on the first warm-up day.\n\
         - Historical ignition density is rebuilt using only alerts strictly before each scored day, preventing target leakage.\n\
         - Missing calendar rows default to non-holiday flags.\n\
         - Public BDIFF coordinates may represent municipality centres rather than ignition points.\n\
         - Equal scores are ordered by H3 index for deterministic top-percentile metrics.\n\n\
         ## Proposed Follow-up (No Model Change in Phase 6)\n\n\
         - Repeat the report over additional summer seasons and spatial holdouts.\n\
         - Replace sparse fixture static layers with complete local OSM, CORINE, and INSEE extracts before interpreting model quality.\n\
         - Replace the fixed warm-up duration with persisted off-season moisture-code state when available.\n",
    );
    tokio::fs::write(path, report)
        .await
        .context("failed to write backtest report")
}

#[allow(clippy::cast_precision_loss)]
fn percentage(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        100.0 * numerator as f64 / denominator as f64
    }
}

fn markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::{ApproximateAuc, percentage_limit};

    #[test]
    fn auc_handles_perfect_reversed_and_tied_scores() {
        let mut perfect = ApproximateAuc::default();
        perfect.observe(0.9, true);
        perfect.observe(0.1, false);
        assert!((perfect.value().expect("AUC") - 1.0).abs() < f64::EPSILON);

        let mut reversed = ApproximateAuc::default();
        reversed.observe(0.1, true);
        reversed.observe(0.9, false);
        assert!(reversed.value().expect("AUC").abs() < f64::EPSILON);

        let mut tied = ApproximateAuc::default();
        tied.observe(0.5, true);
        tied.observe(0.5, false);
        assert!((tied.value().expect("AUC") - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn percentile_limits_round_up() {
        assert_eq!(percentage_limit(101, 5), 6);
        assert_eq!(percentage_limit(101, 10), 11);
    }
}
