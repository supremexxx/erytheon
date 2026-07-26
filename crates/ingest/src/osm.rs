//! OpenStreetMap roads, buildings, activity POIs, and power-line loader.

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use grid::{CellIndex, H3Grid, LatLng, cell_to_db};
use osmpbf::{Element, ElementReader};
use serde::{Deserialize, Serialize};

use crate::{Cadence, FetchCtx, Observation, ObservationKind, Source, SourceError};

/// One-shot OpenStreetMap source supporting fixtures and Geofabrik PBF files.
#[derive(Clone, Debug)]
pub struct OsmSource {
    path: PathBuf,
}

impl OsmSource {
    /// Creates an OSM loader.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl Source for OsmSource {
    fn id(&self) -> &'static str {
        "osm"
    }

    fn cadence(&self) -> Cadence {
        Cadence::OneShot
    }

    async fn fetch(&self, ctx: &FetchCtx) -> Result<Vec<Observation>, SourceError> {
        if self.path.extension().and_then(|value| value.to_str()) == Some("jsonl") {
            return read_aggregate_cache(&self.path, ctx);
        }
        if self.path.is_dir()
            || self.path.extension().and_then(|value| value.to_str()) == Some("pbf")
        {
            return read_pbf_aggregates(&self.path, ctx);
        }
        read_csv(&self.path)
            .await?
            .into_iter()
            .filter(|feature| ctx.aoi.contains(feature.latitude, feature.longitude))
            .map(|feature| normalize(feature, ctx))
            .collect()
    }
}

/// Writes reusable per-H3 OSM aggregate observations as newline-delimited JSON.
///
/// # Errors
///
/// Returns an error when an observation has an invalid aggregate payload or the
/// destination cannot be created atomically.
pub fn write_aggregate_cache(path: &Path, observations: &[Observation]) -> Result<(), SourceError> {
    if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
        return Err(SourceError::InvalidStaticRecord(format!(
            "OSM aggregate cache must use a .jsonl extension: {}",
            path.display()
        )));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|source| SourceError::StaticIo {
            path: parent.to_owned(),
            source,
        })?;
    }
    let temporary_path = path.with_extension("jsonl.tmp");
    let file = File::create(&temporary_path).map_err(|source| SourceError::StaticIo {
        path: temporary_path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    for observation in observations {
        let record = OsmAggregateRecord {
            h3: cell_to_db(observation.cell),
            payload: normalize_aggregate_payload(serde_json::from_value(
                observation.payload.clone(),
            )?),
        };
        serde_json::to_writer(&mut writer, &record)?;
        writer
            .write_all(b"\n")
            .map_err(|source| SourceError::StaticIo {
                path: temporary_path.clone(),
                source,
            })?;
    }
    writer.flush().map_err(|source| SourceError::StaticIo {
        path: temporary_path.clone(),
        source,
    })?;
    fs::rename(&temporary_path, path).map_err(|source| SourceError::StaticIo {
        path: path.to_owned(),
        source,
    })?;
    Ok(())
}

fn read_aggregate_cache(path: &Path, ctx: &FetchCtx) -> Result<Vec<Observation>, SourceError> {
    let file = File::open(path).map_err(|source| SourceError::StaticIo {
        path: path.to_owned(),
        source,
    })?;
    let mut observations = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|source| SourceError::StaticIo {
            path: path.to_owned(),
            source,
        })?;
        let record = serde_json::from_str::<OsmAggregateRecord>(&line).map_err(|error| {
            SourceError::InvalidStaticRecord(format!(
                "{} line {}: {error}",
                path.display(),
                index + 1
            ))
        })?;
        let cell = grid::cell_from_db(record.h3)?;
        if cell.resolution() != ctx.grid.resolution() {
            return Err(SourceError::InvalidStaticRecord(format!(
                "{} line {} uses H3 resolution {}, expected {}",
                path.display(),
                index + 1,
                u8::from(cell.resolution()),
                u8::from(ctx.grid.resolution())
            )));
        }
        observations.push(aggregate_observation(
            cell,
            normalize_aggregate_payload(record.payload),
        )?);
    }
    Ok(observations)
}

async fn read_csv(path: &PathBuf) -> Result<Vec<RawOsmFeature>, SourceError> {
    let document =
        tokio::fs::read_to_string(path)
            .await
            .map_err(|source| SourceError::FixtureRead {
                path: path.clone(),
                source,
            })?;
    let mut reader = csv::Reader::from_reader(document.as_bytes());
    reader
        .deserialize()
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn read_pbf_aggregates(path: &Path, ctx: &FetchCtx) -> Result<Vec<Observation>, SourceError> {
    let paths = pbf_paths(path)?;
    let building_grid = H3Grid::new(u8::from(ctx.grid.resolution()).max(10))?;
    let mut aggregates = HashMap::<CellIndex, OsmCellAggregate>::new();
    for path in paths {
        tracing::info!(path = %path.display(), "OSM regional aggregation started");
        aggregate_pbf(&path, ctx, building_grid, &mut aggregates)?;
        tracing::info!(
            path = %path.display(),
            cells = aggregates.len(),
            "OSM regional aggregation complete"
        );
    }
    let mut aggregates = aggregates.into_iter().collect::<Vec<_>>();
    aggregates.sort_unstable_by_key(|(cell, _)| *cell);
    aggregates
        .into_iter()
        .map(|(cell, mut aggregate)| {
            aggregate.building_cells.sort_unstable();
            aggregate.building_cells.dedup();
            let payload = normalize_aggregate_payload(OsmCellAggregatePayload {
                road_length_m: aggregate.road_length_m,
                power_line_length_m: aggregate.power_line_length_m,
                poi_count: aggregate.poi_count,
                building_cells: aggregate.building_cells,
            });
            aggregate_observation(cell, payload)
        })
        .collect()
}

fn aggregate_observation(
    cell: CellIndex,
    payload: OsmCellAggregatePayload,
) -> Result<Observation, SourceError> {
    Ok(Observation {
        source: "osm".to_owned(),
        kind: ObservationKind::StaticFeature,
        cell,
        observed_at: DateTime::<Utc>::UNIX_EPOCH,
        payload: serde_json::to_value(payload)?,
        dedupe_key: format!("aggregate/{cell}"),
    })
}

fn normalize_aggregate_payload(mut payload: OsmCellAggregatePayload) -> OsmCellAggregatePayload {
    payload.road_length_m = (payload.road_length_m * 1_000.0).round() / 1_000.0;
    payload.power_line_length_m = (payload.power_line_length_m * 1_000.0).round() / 1_000.0;
    payload
}

fn pbf_paths(path: &Path) -> Result<Vec<PathBuf>, SourceError> {
    if path.is_file() {
        return Ok(vec![path.to_owned()]);
    }
    let entries = std::fs::read_dir(path).map_err(|source| SourceError::FixtureRead {
        path: path.to_owned(),
        source,
    })?;
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| {
            entry.is_file()
                && entry
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".osm.pbf"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    if paths.is_empty() {
        return Err(SourceError::InvalidStaticRecord(format!(
            "OSM directory contains no .osm.pbf files: {}",
            path.display()
        )));
    }
    Ok(paths)
}

fn aggregate_pbf(
    path: &Path,
    ctx: &FetchCtx,
    building_grid: H3Grid,
    aggregates: &mut HashMap<CellIndex, OsmCellAggregate>,
) -> Result<(), SourceError> {
    let mut nodes = HashMap::<i64, Option<(f32, f32)>>::new();
    ElementReader::from_path(path)?.for_each(|element| {
        let Element::Way(way) = element else {
            return;
        };
        let tags = way.tags().collect::<Vec<_>>();
        if classify_tags(&tags).is_some() {
            nodes.extend(way.refs().map(|node_id| (node_id, None)));
        }
    })?;

    ElementReader::from_path(path)?.for_each(|element| match element {
        Element::Node(node) => collect_aggregate_node(
            node.id(),
            node.lat(),
            node.lon(),
            node.tags(),
            ctx,
            building_grid,
            &mut nodes,
            aggregates,
        ),
        Element::DenseNode(node) => collect_aggregate_node(
            node.id(),
            node.lat(),
            node.lon(),
            node.tags(),
            ctx,
            building_grid,
            &mut nodes,
            aggregates,
        ),
        Element::Way(_) | Element::Relation(_) => {}
    })?;

    ElementReader::from_path(path)?.for_each(|element| {
        let Element::Way(way) = element else {
            return;
        };
        let tags = way.tags().collect::<Vec<_>>();
        let Some(feature_type) = classify_tags(&tags) else {
            return;
        };
        let coordinates = way
            .refs()
            .filter_map(|node_id| nodes.get(&node_id).copied().flatten())
            .map(|(latitude, longitude)| (f64::from(latitude), f64::from(longitude)))
            .collect::<Vec<_>>();
        if feature_type.is_line() {
            for pair in coordinates.windows(2) {
                aggregate_line(feature_type, pair[0], pair[1], ctx, aggregates);
            }
        } else if let Some((latitude, longitude)) = centroid(&coordinates) {
            aggregate_point(
                feature_type,
                latitude,
                longitude,
                ctx,
                building_grid,
                aggregates,
            );
        }
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn collect_aggregate_node<'a>(
    id: i64,
    latitude: f64,
    longitude: f64,
    tags: impl Iterator<Item = (&'a str, &'a str)>,
    ctx: &FetchCtx,
    building_grid: H3Grid,
    nodes: &mut HashMap<i64, Option<(f32, f32)>>,
    aggregates: &mut HashMap<CellIndex, OsmCellAggregate>,
) {
    if !ctx.aoi.contains(latitude, longitude) {
        return;
    }
    if let Some(coordinates) = nodes.get_mut(&id) {
        #[allow(clippy::cast_possible_truncation)]
        {
            *coordinates = Some((latitude as f32, longitude as f32));
        }
    }
    let tags = tags.collect::<Vec<_>>();
    let Some(feature_type) = classify_tags(&tags) else {
        return;
    };
    if !feature_type.is_line() {
        aggregate_point(
            feature_type,
            latitude,
            longitude,
            ctx,
            building_grid,
            aggregates,
        );
    }
}

fn aggregate_line(
    feature_type: OsmFeatureType,
    start: (f64, f64),
    end: (f64, f64),
    ctx: &FetchCtx,
    aggregates: &mut HashMap<CellIndex, OsmCellAggregate>,
) {
    let latitude = start.0.midpoint(end.0);
    let longitude = start.1.midpoint(end.1);
    if !ctx.aoi.contains(latitude, longitude) {
        return;
    }
    let (Ok(cell), Ok(start), Ok(end)) = (
        ctx.grid.cell_for_point(latitude, longitude),
        LatLng::new(start.0, start.1),
        LatLng::new(end.0, end.1),
    ) else {
        return;
    };
    let length = start.distance_m(end);
    let aggregate = aggregates.entry(cell).or_default();
    match feature_type {
        OsmFeatureType::Road => aggregate.road_length_m += length,
        OsmFeatureType::PowerLine => aggregate.power_line_length_m += length,
        OsmFeatureType::Building | OsmFeatureType::Poi => {}
    }
}

fn aggregate_point(
    feature_type: OsmFeatureType,
    latitude: f64,
    longitude: f64,
    ctx: &FetchCtx,
    building_grid: H3Grid,
    aggregates: &mut HashMap<CellIndex, OsmCellAggregate>,
) {
    if !ctx.aoi.contains(latitude, longitude) {
        return;
    }
    let Ok(cell) = ctx.grid.cell_for_point(latitude, longitude) else {
        return;
    };
    let aggregate = aggregates.entry(cell).or_default();
    match feature_type {
        OsmFeatureType::Building => {
            if let Ok(building_cell) = building_grid.cell_for_point(latitude, longitude) {
                aggregate.building_cells.push(cell_to_db(building_cell));
            }
        }
        OsmFeatureType::Poi => aggregate.poi_count += 1,
        OsmFeatureType::Road | OsmFeatureType::PowerLine => {}
    }
}

fn centroid(coordinates: &[(f64, f64)]) -> Option<(f64, f64)> {
    let count = u32::try_from(coordinates.len()).ok()?;
    if count == 0 {
        return None;
    }
    let count = f64::from(count);
    Some((
        coordinates.iter().map(|point| point.0).sum::<f64>() / count,
        coordinates.iter().map(|point| point.1).sum::<f64>() / count,
    ))
}

fn classify_tags(tags: &[(&str, &str)]) -> Option<OsmFeatureType> {
    let value = |wanted| {
        tags.iter()
            .find_map(|(key, value)| (*key == wanted).then_some(*value))
    };
    if value("highway").is_some() {
        Some(OsmFeatureType::Road)
    } else if value("power") == Some("line") {
        Some(OsmFeatureType::PowerLine)
    } else if value("building").is_some() {
        Some(OsmFeatureType::Building)
    } else if value("amenity") == Some("parking") || value("tourism") == Some("camp_site") {
        Some(OsmFeatureType::Poi)
    } else {
        None
    }
}

fn normalize(feature: RawOsmFeature, ctx: &FetchCtx) -> Result<Observation, SourceError> {
    let latitude = feature
        .end_latitude
        .map_or(feature.latitude, |end| feature.latitude.midpoint(end));
    let longitude = feature
        .end_longitude
        .map_or(feature.longitude, |end| feature.longitude.midpoint(end));
    let payload = OsmFeaturePayload {
        osm_id: feature.osm_id.clone(),
        feature_type: feature.feature_type,
        latitude: feature.latitude,
        longitude: feature.longitude,
        end_latitude: feature.end_latitude,
        end_longitude: feature.end_longitude,
        name: feature.name,
    };
    Ok(Observation {
        source: "osm".to_owned(),
        kind: ObservationKind::StaticFeature,
        cell: ctx.grid.cell_for_point(latitude, longitude)?,
        observed_at: DateTime::<Utc>::UNIX_EPOCH,
        payload: serde_json::to_value(payload)?,
        dedupe_key: feature.osm_id,
    })
}

#[derive(Clone, Debug, Deserialize)]
struct RawOsmFeature {
    osm_id: String,
    feature_type: OsmFeatureType,
    latitude: f64,
    longitude: f64,
    end_latitude: Option<f64>,
    end_longitude: Option<f64>,
    name: String,
}

/// OSM categories retained by the ignition model.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OsmFeatureType {
    /// Any OSM highway, including tracks and paths.
    Road,
    /// Building centroid.
    Building,
    /// Parking or campsite activity point.
    Poi,
    /// Electric power line segment.
    PowerLine,
}

impl OsmFeatureType {
    const fn is_line(self) -> bool {
        matches!(self, Self::Road | Self::PowerLine)
    }
}

/// Normalized OSM point or line-segment payload used by CSV fixtures.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OsmFeaturePayload {
    pub osm_id: String,
    pub feature_type: OsmFeatureType,
    pub latitude: f64,
    pub longitude: f64,
    pub end_latitude: Option<f64>,
    pub end_longitude: Option<f64>,
    pub name: String,
}

/// Per-H3 OSM aggregate emitted by PBF ingestion.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OsmCellAggregatePayload {
    pub road_length_m: f64,
    pub power_line_length_m: f64,
    pub poi_count: u64,
    pub building_cells: Vec<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OsmAggregateRecord {
    h3: i64,
    payload: OsmCellAggregatePayload,
}

#[derive(Clone, Debug, Default)]
struct OsmCellAggregate {
    road_length_m: f64,
    power_line_length_m: f64,
    poi_count: u64,
    building_cells: Vec<i64>,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::Utc;
    use grid::{BoundingBox, H3Grid};

    use crate::{FetchCtx, Source};

    use super::{
        OsmCellAggregatePayload, OsmFeatureType, OsmSource, aggregate_observation, centroid,
        classify_tags, pbf_paths, write_aggregate_cache,
    };

    #[test]
    fn classifies_supported_tags() {
        assert_eq!(
            classify_tags(&[("highway", "track")]),
            Some(OsmFeatureType::Road)
        );
        assert_eq!(
            classify_tags(&[("power", "line")]),
            Some(OsmFeatureType::PowerLine)
        );
        assert_eq!(
            classify_tags(&[("building", "yes")]),
            Some(OsmFeatureType::Building)
        );
        assert_eq!(
            classify_tags(&[("tourism", "camp_site")]),
            Some(OsmFeatureType::Poi)
        );
    }

    #[test]
    fn computes_way_centroid() {
        assert_eq!(centroid(&[(42.0, 2.0), (44.0, 4.0)]), Some((43.0, 3.0)));
        assert_eq!(centroid(&[]), None);
    }

    #[test]
    fn rejects_a_directory_without_pbf_files() {
        let error = pbf_paths(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect_err("crate directory has no regional PBF");
        assert!(error.to_string().contains("contains no .osm.pbf files"));
    }

    #[tokio::test]
    async fn round_trips_an_aggregate_cache() {
        let grid = H3Grid::new(8).expect("grid");
        let cell = grid.cell_for_point(42.0, 9.0).expect("cell");
        let path = std::env::temp_dir().join(format!("pyrorisk-osm-{cell}.jsonl"));
        let observation = aggregate_observation(
            cell,
            OsmCellAggregatePayload {
                road_length_m: 123.0,
                power_line_length_m: 45.0,
                poi_count: 2,
                building_cells: Vec::new(),
            },
        )
        .expect("observation");
        write_aggregate_cache(&path, &[observation]).expect("write cache");
        let context = FetchCtx {
            client: reqwest::Client::new(),
            aoi: BoundingBox::new(41.0, 8.0, 43.0, 10.0).expect("bbox"),
            grid,
            days: 1,
            end_date: Utc::now().date_naive(),
            firms_map_key: None,
            meteofrance_api_key: None,
        };
        let loaded = OsmSource::new(&path)
            .fetch(&context)
            .await
            .expect("read cache");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].cell, cell);
        std::fs::remove_file(path).expect("remove cache");
    }
}
