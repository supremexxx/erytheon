use std::path::Path;

use anyhow::Context;
use geojson::{Feature, FeatureCollection, GeoJson, Geometry, Value};
use ingest::Observation;
use serde_json::{Map, json};

pub async fn write_firms_geojson(
    observations: &[Observation],
    output_path: &Path,
) -> anyhow::Result<()> {
    let features = observations
        .iter()
        .map(observation_feature)
        .collect::<Vec<_>>();
    let collection = FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    };
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    tokio::fs::write(output_path, GeoJson::from(collection).to_string())
        .await
        .with_context(|| format!("failed to write {}", output_path.display()))
}

fn observation_feature(observation: &Observation) -> Feature {
    let boundary = observation.cell.boundary();
    let mut ring = boundary
        .iter()
        .map(|coordinate| vec![coordinate.lng(), coordinate.lat()])
        .collect::<Vec<_>>();
    if let Some(first) = ring.first().cloned() {
        ring.push(first);
    }

    let mut properties = Map::new();
    properties.insert("h3".to_owned(), json!(observation.cell.to_string()));
    properties.insert("source".to_owned(), json!(observation.source));
    properties.insert("kind".to_owned(), json!(observation.kind.as_str()));
    properties.insert("observed_at".to_owned(), json!(observation.observed_at));
    properties.insert("payload".to_owned(), observation.payload.clone());

    Feature {
        bbox: None,
        geometry: Some(Geometry::new(Value::Polygon(vec![ring]))),
        id: None,
        properties: Some(properties),
        foreign_members: None,
    }
}
