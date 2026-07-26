use std::collections::HashMap;

use anyhow::Context as _;
use chrono::{Datelike as _, Days, NaiveDate, NaiveTime};
use fwi::{FwiState, Weather, calculate_daily, fire_weather_index, initial_spread_index};
use grid::LatLng;
use ingest::{FetchCtx, Observation, Source, meteo_france::WeatherStationObservation};
use store::{FwiStateRow, Store};

const SOLAR_NOON_UTC: NaiveTime = NaiveTime::from_hms_opt(12, 0, 0).expect("valid constant time");
const EXACT_STATION_DISTANCE_KM: f64 = 1.0e-9;
const IDW_NEAREST_STATION_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecomputeSummary {
    pub date: NaiveDate,
    pub station_count: usize,
    pub cell_count: usize,
    pub observations_inserted: u64,
    pub states_upserted: u64,
}

pub async fn recompute_weather(
    store: &Store,
    source: &impl Source,
    context: &FetchCtx,
    idw_power: f64,
) -> anyhow::Result<RecomputeSummary> {
    let observations = source
        .fetch(context)
        .await
        .context("Météo-France fetch failed")?;
    let observations_inserted = store
        .insert_observations(&observations)
        .await
        .context("failed to persist weather observations")?;
    let stations = select_station_observations(&observations)?;
    anyhow::ensure!(
        !stations.noon.is_empty(),
        "no complete weather station observation is available for {}",
        context.end_date
    );

    let cells = context
        .grid
        .cells_for_bbox(context.aoi)
        .context("failed to cover AOI with H3 cells")?;
    let previous_date = context
        .end_date
        .checked_sub_days(Days::new(1))
        .context("recompute date has no representable previous day")?;
    let previous_states = store
        .fwi_states(previous_date, &cells)
        .await
        .context("failed to load previous FWI state")?;
    let month = u8::try_from(context.end_date.month()).context("month does not fit in u8")?;
    let noon_weather = stations
        .noon
        .iter()
        .map(|station| station.weather.clone())
        .collect::<Vec<_>>();
    let latest_weather = stations
        .latest
        .iter()
        .map(|station| station.weather.clone())
        .collect::<Vec<_>>();
    let mut states = Vec::with_capacity(cells.len());

    for cell in cells.iter().copied() {
        let target = context.grid.cell_center(cell);
        let weather = interpolate_station_weather(target, &noon_weather, idw_power, month)?;
        let latest_weather =
            interpolate_station_weather(target, &latest_weather, idw_power, month)?;
        let previous = previous_states
            .get(&cell)
            .copied()
            .unwrap_or_else(FwiState::default);
        let output = calculate_daily(weather, previous).context("daily FWI calculation failed")?;
        let isi = initial_spread_index(output.ffmc, latest_weather.wind_speed_kmh)
            .context("intraday ISI calculation failed")?;
        let intraday_fwi =
            fire_weather_index(isi, output.bui).context("intraday FWI calculation failed")?;
        states.push(FwiStateRow {
            cell,
            date: context.end_date,
            ffmc: output.ffmc,
            dmc: output.dmc,
            dc: output.dc,
            isi,
            bui: output.bui,
            fwi: intraday_fwi,
        });
    }

    let states_upserted = store
        .upsert_fwi_states(&states)
        .await
        .context("failed to persist FWI state")?;
    Ok(RecomputeSummary {
        date: context.end_date,
        station_count: stations.noon.len(),
        cell_count: cells.len(),
        observations_inserted,
        states_upserted,
    })
}

#[derive(Clone, Debug)]
struct TimedStation {
    observed_at: chrono::DateTime<chrono::Utc>,
    weather: WeatherStationObservation,
}

struct StationSelections {
    noon: Vec<TimedStation>,
    latest: Vec<TimedStation>,
}

fn select_station_observations(observations: &[Observation]) -> anyhow::Result<StationSelections> {
    let mut noon = HashMap::<String, TimedStation>::new();
    let mut latest = HashMap::<String, TimedStation>::new();
    for observation in observations {
        let weather =
            serde_json::from_value::<WeatherStationObservation>(observation.payload.clone())
                .context("invalid normalized weather payload")?;
        let candidate = TimedStation {
            observed_at: observation.observed_at,
            weather,
        };
        noon.entry(candidate.weather.station_id.clone())
            .and_modify(|current| {
                if distance_from_noon(candidate.observed_at)
                    < distance_from_noon(current.observed_at)
                {
                    *current = candidate.clone();
                }
            })
            .or_insert_with(|| candidate.clone());
        latest
            .entry(candidate.weather.station_id.clone())
            .and_modify(|current| {
                if candidate.observed_at > current.observed_at {
                    *current = candidate.clone();
                }
            })
            .or_insert(candidate);
    }
    let mut noon = noon.into_values().collect::<Vec<_>>();
    let mut latest = latest.into_values().collect::<Vec<_>>();
    noon.sort_by(|left, right| left.weather.station_id.cmp(&right.weather.station_id));
    latest.sort_by(|left, right| left.weather.station_id.cmp(&right.weather.station_id));
    Ok(StationSelections { noon, latest })
}

fn distance_from_noon(observed_at: chrono::DateTime<chrono::Utc>) -> u64 {
    observed_at
        .time()
        .signed_duration_since(SOLAR_NOON_UTC)
        .num_seconds()
        .unsigned_abs()
}

pub(crate) fn interpolate_station_weather(
    target: LatLng,
    stations: &[WeatherStationObservation],
    power: f64,
    month: u8,
) -> anyhow::Result<Weather> {
    anyhow::ensure!(
        !stations.is_empty(),
        "at least one weather station is required"
    );
    anyhow::ensure!(
        power.is_finite() && power > 0.0,
        "IDW power must be positive"
    );

    let mut stations_by_distance = stations
        .iter()
        .map(|station| {
            let location = LatLng::new(station.latitude, station.longitude)
                .context("invalid station coordinate")?;
            Ok((target.distance_km(location), station))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    stations_by_distance.sort_by(|left, right| left.0.total_cmp(&right.0));
    if stations_by_distance[0].0 <= EXACT_STATION_DISTANCE_KM {
        return Ok(weather_from_station(stations_by_distance[0].1, month));
    }

    let mut weight_sum = 0.0;
    let mut temperature = 0.0;
    let mut humidity = 0.0;
    let mut wind = 0.0;
    let mut precipitation = 0.0;
    for (distance, station) in stations_by_distance
        .into_iter()
        .take(IDW_NEAREST_STATION_COUNT)
    {
        let weight = distance.powf(-power);
        weight_sum += weight;
        temperature += weight * station.temperature_c;
        humidity += weight * station.relative_humidity_pct;
        wind += weight * station.wind_speed_kmh;
        precipitation += weight * station.precipitation_24h_mm;
    }

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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::NaiveDate;
    use grid::{BoundingBox, H3Grid};
    use ingest::{FetchCtx, meteo_france::MeteoFranceSource};
    use store::Store;

    use super::{interpolate_station_weather, recompute_weather};
    use ingest::meteo_france::WeatherStationObservation;

    #[test]
    fn interpolation_preserves_an_exact_station_value() {
        let station = WeatherStationObservation {
            station_id: "07747".to_owned(),
            station_name: "PERPIGNAN".to_owned(),
            latitude: 42.737_167,
            longitude: 2.872_833,
            temperature_c: 32.0,
            relative_humidity_pct: 28.0,
            wind_speed_kmh: 21.6,
            precipitation_24h_mm: 0.0,
        };
        let target = grid::LatLng::new(42.737_167, 2.872_833).expect("valid location");
        let weather =
            interpolate_station_weather(target, &[station], 2.0, 7).expect("interpolation");

        assert!((weather.temperature_c - 32.0).abs() < f64::EPSILON);
        assert!((weather.wind_speed_kmh - 21.6).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn fixture_recompute_persists_every_aoi_cell() {
        dotenvy::dotenv().ok();
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping database integration test: DATABASE_URL is not configured");
            return;
        };
        let store = Store::connect(&database_url)
            .await
            .expect("database should accept migrations");
        let date = NaiveDate::from_ymd_opt(2025, 7, 16).expect("valid date");
        let grid = H3Grid::new(9).expect("valid grid");
        let aoi = BoundingBox::new(2.34, 43.20, 2.36, 43.22).expect("valid AOI");
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/meteo_france_synop.csv");
        let source = MeteoFranceSource::new(fixture);
        let context = FetchCtx {
            client: reqwest::Client::new(),
            aoi,
            grid,
            days: 1,
            end_date: date,
            firms_map_key: None,
            meteofrance_api_key: None,
        };

        let summary = recompute_weather(&store, &source, &context, 2.0)
            .await
            .expect("fixture recompute should succeed");
        let cells = grid.cells_for_bbox(aoi).expect("valid AOI coverage");
        let persisted = store
            .fwi_states(date, &cells)
            .await
            .expect("persisted FWI states");

        assert_eq!(summary.station_count, 4);
        assert_eq!(summary.cell_count, cells.len());
        assert_eq!(persisted.len(), cells.len());
    }
}
