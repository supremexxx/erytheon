use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use anyhow::Context as _;
use grid::{CellIndex, H3Grid, LatLng, cell_from_db};
use ingest::{
    FetchCtx, Observation, Source,
    calendar::CalendarSource,
    corine::{CorineCellAggregatePayload, CorinePayload, CorineSource},
    fire_history::FireHistorySource,
    insee::{InseePopulationPayload, InseeSource},
    osm::{OsmCellAggregatePayload, OsmFeaturePayload, OsmFeatureType, OsmSource},
};
use serde::Serialize;
use store::{CellStaticRow, Store};

const WUI_DISTANCE_KM: f64 = 0.05;
const NEIGHBORHOOD_DISTANCE: u32 = 1;
const HISTORY_KERNEL_DISTANCE: u32 = 2;
const SCHOOL_ZONE: &str = "C";

#[derive(Clone, Debug)]
pub struct StaticPaths {
    pub osm: PathBuf,
    pub bdiff: PathBuf,
    pub promethee: PathBuf,
    pub corine: PathBuf,
    pub insee: PathBuf,
    pub calendar: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticLoadSummary {
    pub cells: usize,
    pub osm_records: usize,
    pub corine_samples: usize,
    pub population_samples: usize,
    pub historical_ignitions: usize,
    pub calendar_days: usize,
    pub cell_rows_upserted: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryRefreshSummary {
    pub ignitions: usize,
    pub cells: usize,
    pub rows_updated: u64,
}

pub async fn load_static(
    store: &Store,
    context: &FetchCtx,
    paths: StaticPaths,
    strict: bool,
) -> anyhow::Result<StaticLoadSummary> {
    let cells = context
        .grid
        .cells_for_bbox(context.aoi)
        .context("failed to cover static AOI")?;
    load_static_partitions(store, context, paths, strict, &[&cells]).await
}

pub async fn load_static_partitions(
    store: &Store,
    context: &FetchCtx,
    paths: StaticPaths,
    strict: bool,
    partitions: &[&[CellIndex]],
) -> anyhow::Result<StaticLoadSummary> {
    anyhow::ensure!(!partitions.is_empty(), "static partition list is empty");
    let osm = fetch_source(store, &OsmSource::new(paths.osm), context, strict).await?;
    let corine = fetch_source(store, &CorineSource::new(paths.corine), context, strict).await?;
    let population = fetch_source(store, &InseeSource::new(paths.insee), context, strict).await?;
    let mut history = fetch_source(
        store,
        &FireHistorySource::bdiff(paths.bdiff),
        context,
        strict,
    )
    .await?;
    history.extend(
        fetch_source(
            store,
            &FireHistorySource::promethee(paths.promethee),
            context,
            strict,
        )
        .await?,
    );
    let calendar_source = CalendarSource::new(paths.calendar);
    let calendar = match calendar_source.load().await {
        Ok(days) => {
            record_success(store, "calendar", days.len()).await;
            days
        }
        Err(error) => {
            if strict {
                tracing::error!(source = "calendar", %error, "static source failed");
            } else {
                tracing::error!(source = "calendar", %error, "static source failed; continuing");
            }
            record_error(store, "calendar", &error.to_string()).await;
            if strict {
                return Err(error).context("calendar source failed in production mode");
            }
            Vec::new()
        }
    };

    store
        .upsert_ignition_history(&history)
        .await
        .context("failed to persist ignition history")?;
    store
        .upsert_calendar_days(&calendar)
        .await
        .context("failed to persist calendar")?;
    let mut cell_rows_upserted = 0;
    let mut cells = 0;
    for partition in partitions {
        let rows = compute_features(
            context.grid,
            partition,
            &osm,
            &corine,
            &population,
            &history,
        )?;
        cell_rows_upserted += store
            .upsert_cell_static(&rows)
            .await
            .context("failed to persist static cell features")?;
        cells += partition.len();
    }

    Ok(StaticLoadSummary {
        cells,
        osm_records: osm.len(),
        corine_samples: corine.len(),
        population_samples: population.len(),
        historical_ignitions: history.len(),
        calendar_days: calendar.len(),
        cell_rows_upserted,
    })
}

async fn fetch_source(
    store: &Store,
    source: &impl Source,
    context: &FetchCtx,
    strict: bool,
) -> anyhow::Result<Vec<Observation>> {
    match source.fetch(context).await {
        Ok(observations) => {
            record_success(store, source.id(), observations.len()).await;
            Ok(observations)
        }
        Err(error) => {
            if strict {
                tracing::error!(source = source.id(), %error, "static source failed");
            } else {
                tracing::error!(source = source.id(), %error, "static source failed; continuing");
            }
            record_error(store, source.id(), &error.to_string()).await;
            if strict {
                Err(error)
                    .with_context(|| format!("{} source failed in production mode", source.id()))
            } else {
                Ok(Vec::new())
            }
        }
    }
}

async fn record_success(store: &Store, source: &str, count: usize) {
    if let Err(error) = store.record_source_success(source, count).await {
        tracing::error!(source, %error, "failed to record source status");
    }
}

async fn record_error(store: &Store, source: &str, message: &str) {
    if let Err(error) = store.record_source_error(source, message).await {
        tracing::error!(source, %error, "failed to record source status");
    }
}

pub async fn refresh_history_features(
    store: &Store,
    grid: H3Grid,
    partitions: &[&[CellIndex]],
    history_cells: &[CellIndex],
) -> anyhow::Result<HistoryRefreshSummary> {
    anyhow::ensure!(!partitions.is_empty(), "history partition list is empty");
    let mut rows_updated = 0;
    let mut cells = 0;
    for partition in partitions {
        let rows = history_rows(grid, partition, history_cells);
        rows_updated += store
            .update_cell_history(&rows)
            .await
            .context("failed to refresh historical ignition features")?;
        cells += partition.len();
    }
    Ok(HistoryRefreshSummary {
        ignitions: history_cells.len(),
        cells,
        rows_updated,
    })
}

fn history_rows(
    grid: H3Grid,
    cells: &[CellIndex],
    history_cells: &[CellIndex],
) -> Vec<(CellIndex, f64)> {
    let history_neighborhood = cells
        .iter()
        .flat_map(|cell| grid.neighbors(*cell, HISTORY_KERNEL_DISTANCE))
        .collect::<HashSet<_>>();
    let relevant_history = history_cells
        .iter()
        .filter(|cell| history_neighborhood.contains(cell))
        .copied()
        .collect::<Vec<_>>();
    let values = normalize(history_kernel(grid, cells, &relevant_history));
    cells
        .iter()
        .copied()
        .map(|cell| (cell, value(&values, cell)))
        .collect()
}

fn compute_features(
    grid: H3Grid,
    cells: &[CellIndex],
    osm: &[Observation],
    corine: &[Observation],
    population: &[Observation],
    history: &[Observation],
) -> anyhow::Result<Vec<CellStaticRow>> {
    let target_cells = cells.iter().copied().collect::<HashSet<_>>();
    let neighborhood_cells = cells
        .iter()
        .flat_map(|cell| grid.neighbors(*cell, NEIGHBORHOOD_DISTANCE))
        .collect::<HashSet<_>>();
    let history_neighborhood = cells
        .iter()
        .flat_map(|cell| grid.neighbors(*cell, HISTORY_KERNEL_DISTANCE))
        .collect::<HashSet<_>>();
    let osm_metrics = collect_osm_metrics(osm, &target_cells, &neighborhood_cells)?;

    let mut combustible = HashMap::<CellIndex, bool>::new();
    let mut agricultural = HashMap::<CellIndex, bool>::new();
    let mut combustible_points = HashMap::<CellIndex, Vec<LatLng>>::new();
    for observation in corine {
        if !neighborhood_cells.contains(&observation.cell) {
            continue;
        }
        let payload = serde_json::from_value::<CorineInput>(observation.payload.clone())
            .context("invalid normalized CORINE payload")?;
        let (combustible_value, agricultural_value, point) = match payload {
            CorineInput::Aggregate(payload) => (payload.combustible, payload.agricultural, None),
            CorineInput::Sample(payload) => (
                payload.combustible,
                payload.agricultural,
                Some(
                    LatLng::new(payload.latitude, payload.longitude)
                        .context("invalid CORINE coordinate")?,
                ),
            ),
        };
        combustible
            .entry(observation.cell)
            .and_modify(|value| *value |= combustible_value)
            .or_insert(combustible_value);
        agricultural
            .entry(observation.cell)
            .and_modify(|value| *value |= agricultural_value)
            .or_insert(agricultural_value);
        if combustible_value && let Some(point) = point {
            combustible_points
                .entry(observation.cell)
                .or_default()
                .push(point);
        }
    }

    let mut population_counts = HashMap::<CellIndex, f64>::new();
    for observation in population {
        if !target_cells.contains(&observation.cell) {
            continue;
        }
        let payload = serde_json::from_value::<InseePopulationPayload>(observation.payload.clone())
            .context("invalid normalized INSEE payload")?;
        *population_counts.entry(observation.cell).or_default() += payload.individuals;
    }

    let road = normalize(neighborhood_sum(grid, cells, &osm_metrics.road_lengths));
    let power_line = normalize(neighborhood_sum(grid, cells, &osm_metrics.power_lengths));
    let poi = normalize(neighborhood_sum(grid, cells, &osm_metrics.poi_counts));
    let population = normalize(values_for_cells(cells, &population_counts));
    let history_cells = history
        .iter()
        .filter(|observation| history_neighborhood.contains(&observation.cell))
        .map(|observation| observation.cell)
        .collect::<Vec<_>>();
    let hist = normalize(history_kernel(grid, cells, &history_cells));
    let mut wui = wui_cells(grid, &osm_metrics.buildings, &combustible_points);
    wui.extend(
        osm_metrics
            .building_parent_cells
            .iter()
            .filter(|cell| combustible.get(cell).copied().unwrap_or(false))
            .copied(),
    );

    cells
        .iter()
        .copied()
        .map(|cell| {
            let features = CellStaticFeatures {
                hist: value(&hist, cell),
                wui: if wui.contains(&cell) { 1.0 } else { 0.0 },
                road: value(&road, cell),
                agri: if agricultural.get(&cell).copied().unwrap_or(false) {
                    1.0
                } else {
                    0.0
                },
                combustible: combustible.get(&cell).copied().unwrap_or(false),
                population: value(&population, cell),
                poi: value(&poi, cell),
                power_line: value(&power_line, cell),
                school_zone: SCHOOL_ZONE,
            };
            Ok(CellStaticRow {
                cell,
                features: serde_json::to_value(features)?,
            })
        })
        .collect()
}

#[derive(Default)]
struct OsmMetrics {
    road_lengths: HashMap<CellIndex, f64>,
    power_lengths: HashMap<CellIndex, f64>,
    poi_counts: HashMap<CellIndex, f64>,
    buildings: Vec<(CellIndex, LatLng)>,
    building_parent_cells: HashSet<CellIndex>,
}

fn collect_osm_metrics(
    observations: &[Observation],
    target_cells: &HashSet<CellIndex>,
    neighborhood_cells: &HashSet<CellIndex>,
) -> anyhow::Result<OsmMetrics> {
    let mut metrics = OsmMetrics::default();
    for observation in observations {
        if !neighborhood_cells.contains(&observation.cell) {
            continue;
        }
        let payload = serde_json::from_value::<OsmPayload>(observation.payload.clone())
            .context("invalid normalized OSM payload")?;
        match payload {
            OsmPayload::Aggregate(payload) => {
                *metrics.road_lengths.entry(observation.cell).or_default() += payload.road_length_m;
                *metrics.power_lengths.entry(observation.cell).or_default() +=
                    payload.power_line_length_m;
                #[allow(clippy::cast_precision_loss)]
                {
                    *metrics.poi_counts.entry(observation.cell).or_default() +=
                        payload.poi_count as f64;
                }
                if target_cells.contains(&observation.cell) {
                    if !payload.building_cells.is_empty() {
                        metrics.building_parent_cells.insert(observation.cell);
                    }
                    for building_cell in payload.building_cells {
                        let building_cell = cell_from_db(building_cell)
                            .context("invalid aggregated OSM building cell")?;
                        metrics
                            .buildings
                            .push((observation.cell, LatLng::from(building_cell)));
                    }
                }
            }
            OsmPayload::Feature(payload) => match payload.feature_type {
                OsmFeatureType::Road => {
                    *metrics.road_lengths.entry(observation.cell).or_default() +=
                        segment_length(&payload)?;
                }
                OsmFeatureType::PowerLine => {
                    *metrics.power_lengths.entry(observation.cell).or_default() +=
                        segment_length(&payload)?;
                }
                OsmFeatureType::Poi => {
                    *metrics.poi_counts.entry(observation.cell).or_default() += 1.0;
                }
                OsmFeatureType::Building => {
                    if target_cells.contains(&observation.cell) {
                        metrics.building_parent_cells.insert(observation.cell);
                        metrics.buildings.push((
                            observation.cell,
                            LatLng::new(payload.latitude, payload.longitude)
                                .context("invalid OSM building coordinate")?,
                        ));
                    }
                }
            },
        }
    }
    Ok(metrics)
}

fn segment_length(payload: &OsmFeaturePayload) -> anyhow::Result<f64> {
    let end_latitude = payload
        .end_latitude
        .context("OSM line has no end latitude")?;
    let end_longitude = payload
        .end_longitude
        .context("OSM line has no end longitude")?;
    let start =
        LatLng::new(payload.latitude, payload.longitude).context("invalid OSM line start")?;
    let end = LatLng::new(end_latitude, end_longitude).context("invalid OSM line end")?;
    Ok(start.distance_m(end))
}

fn neighborhood_sum(
    grid: H3Grid,
    cells: &[CellIndex],
    raw: &HashMap<CellIndex, f64>,
) -> HashMap<CellIndex, f64> {
    cells
        .iter()
        .copied()
        .map(|cell| {
            let sum = grid
                .neighbors(cell, NEIGHBORHOOD_DISTANCE)
                .into_iter()
                .map(|neighbor| raw.get(&neighbor).copied().unwrap_or(0.0))
                .sum();
            (cell, sum)
        })
        .collect()
}

fn history_kernel(
    grid: H3Grid,
    cells: &[CellIndex],
    ignition_cells: &[CellIndex],
) -> HashMap<CellIndex, f64> {
    let aoi = cells.iter().copied().collect::<HashSet<_>>();
    let mut values = values_for_cells(cells, &HashMap::new());
    for ignition in ignition_cells {
        for (cell, distance) in grid.neighbors_with_distance(*ignition, HISTORY_KERNEL_DISTANCE) {
            if aoi.contains(&cell) {
                let weight = match distance {
                    0 => 1.0,
                    1 => 0.5,
                    _ => 0.25,
                };
                *values.entry(cell).or_default() += weight;
            }
        }
    }
    values
}

fn wui_cells(
    grid: H3Grid,
    buildings: &[(CellIndex, LatLng)],
    combustible_points: &HashMap<CellIndex, Vec<LatLng>>,
) -> HashSet<CellIndex> {
    buildings
        .iter()
        .filter_map(|(cell, building)| {
            grid.neighbors(*cell, NEIGHBORHOOD_DISTANCE)
                .into_iter()
                .filter_map(|neighbor| combustible_points.get(&neighbor))
                .flatten()
                .any(|vegetation| building.distance_km(*vegetation) <= WUI_DISTANCE_KM)
                .then_some(*cell)
        })
        .collect()
}

fn values_for_cells(cells: &[CellIndex], raw: &HashMap<CellIndex, f64>) -> HashMap<CellIndex, f64> {
    cells
        .iter()
        .copied()
        .map(|cell| (cell, raw.get(&cell).copied().unwrap_or(0.0)))
        .collect()
}

fn normalize(mut values: HashMap<CellIndex, f64>) -> HashMap<CellIndex, f64> {
    let maximum = values.values().copied().fold(0.0, f64::max);
    if maximum > 0.0 {
        for value in values.values_mut() {
            *value /= maximum;
        }
    }
    values
}

fn value(values: &HashMap<CellIndex, f64>, cell: CellIndex) -> f64 {
    values.get(&cell).copied().unwrap_or(0.0)
}

#[derive(Serialize)]
struct CellStaticFeatures<'a> {
    hist: f64,
    wui: f64,
    road: f64,
    agri: f64,
    combustible: bool,
    population: f64,
    poi: f64,
    power_line: f64,
    school_zone: &'a str,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum OsmPayload {
    Aggregate(OsmCellAggregatePayload),
    Feature(OsmFeaturePayload),
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum CorineInput {
    Aggregate(CorineCellAggregatePayload),
    Sample(CorinePayload),
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::Path};

    use chrono::Utc;
    use grid::{BoundingBox, H3Grid};
    use ingest::FetchCtx;
    use store::Store;

    use super::{
        CorineInput, StaticPaths, compute_features, history_kernel, history_rows, load_static,
        wui_cells,
    };

    #[test]
    fn distinguishes_corine_samples_from_cell_aggregates() {
        let aggregate = serde_json::json!({"combustible": true, "agricultural": false});
        let sample = serde_json::json!({
            "sample_id": "sample",
            "latitude": 43.0,
            "longitude": 2.0,
            "class_code": 311,
            "combustible": true,
            "agricultural": false
        });

        assert!(matches!(
            serde_json::from_value::<CorineInput>(aggregate).expect("aggregate"),
            CorineInput::Aggregate(_)
        ));
        assert!(matches!(
            serde_json::from_value::<CorineInput>(sample).expect("sample"),
            CorineInput::Sample(_)
        ));
    }

    #[test]
    fn empty_cells_receive_complete_zero_features() {
        let grid = H3Grid::new(9).expect("valid grid");
        let cells = grid
            .cells_for_bbox(BoundingBox::new(2.34, 43.20, 2.345, 43.205).expect("valid bbox"))
            .expect("valid coverage");
        let rows = compute_features(grid, &cells, &[], &[], &[], &[]).expect("features");

        assert_eq!(rows.len(), cells.len());
        assert!(rows.iter().all(|row| row.features["road"] == 0.0));
        assert!(rows.iter().all(|row| row.features["school_zone"] == "C"));
    }

    #[test]
    fn history_kernel_handles_an_aoi_border() {
        let grid = H3Grid::new(9).expect("valid grid");
        let cell = grid.cell_for_point(43.2122, 2.3537).expect("valid cell");
        let values = history_kernel(grid, &[cell], &[cell]);

        assert!((values[&cell] - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn history_refresh_normalizes_only_requested_cells() {
        let grid = H3Grid::new(9).expect("valid grid");
        let ignition = grid.cell_for_point(43.2122, 2.3537).expect("valid cell");
        let cells = grid.neighbors(ignition, 1);
        let rows = history_rows(grid, &cells, &[ignition]);

        assert_eq!(rows.len(), cells.len());
        assert!(
            rows.iter()
                .any(|(cell, value)| *cell == ignition && (*value - 1.0).abs() < f64::EPSILON)
        );
        assert!(rows.iter().all(|(_, value)| (0.0..=1.0).contains(value)));
    }

    #[test]
    fn wui_respects_the_fifty_metre_threshold() {
        let grid = H3Grid::new(9).expect("valid grid");
        let building = grid::LatLng::new(43.2122, 2.3537).expect("valid building");
        let cell = building.to_cell(grid.resolution());
        let near = grid::LatLng::new(43.2124, 2.3537).expect("valid vegetation");
        let far = grid::LatLng::new(43.2132, 2.3537).expect("valid vegetation");
        let mut vegetation = HashMap::from([(near.to_cell(grid.resolution()), vec![near])]);

        assert!(wui_cells(grid, &[(cell, building)], &vegetation).contains(&cell));
        vegetation.clear();
        vegetation.insert(far.to_cell(grid.resolution()), vec![far]);
        assert!(!wui_cells(grid, &[(cell, building)], &vegetation).contains(&cell));
    }

    #[tokio::test]
    async fn fixture_load_is_complete_and_idempotent() {
        dotenvy::dotenv().ok();
        let Ok(database_url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipping database integration test: DATABASE_URL is not configured");
            return;
        };
        let store = Store::connect(&database_url)
            .await
            .expect("database should accept migrations");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let paths = || StaticPaths {
            osm: root.join("testdata/osm_features.csv"),
            bdiff: root.join("testdata/bdiff_aude.csv"),
            promethee: root.join("testdata/promethee_aude.csv"),
            corine: root.join("testdata/corine_aude.csv"),
            insee: root.join("testdata/insee_filosofi_200m.csv"),
            calendar: root.join("testdata/calendar_zone_c.csv"),
        };
        let grid = H3Grid::new(9).expect("valid grid");
        let aoi = BoundingBox::new(2.34, 43.20, 2.36, 43.22).expect("valid AOI");
        let context = FetchCtx {
            client: reqwest::Client::new(),
            aoi,
            grid,
            days: 1,
            end_date: Utc::now().date_naive(),
            firms_map_key: None,
            meteofrance_api_key: None,
        };

        let first = load_static(&store, &context, paths(), false)
            .await
            .expect("first static load");
        let second = load_static(&store, &context, paths(), false)
            .await
            .expect("second static load");
        let cells = grid.cells_for_bbox(aoi).expect("valid coverage");

        assert_eq!(first.cells, cells.len());
        assert_eq!(first.osm_records, 8);
        assert_eq!(second.cells, first.cells);
        assert_eq!(
            store.count_cell_static(&cells).await.expect("static rows"),
            i64::try_from(cells.len()).expect("cell count fits i64")
        );
    }
}
