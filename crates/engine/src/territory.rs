use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use anyhow::Context as _;
use geo::{BoundingRect as _, Geometry};
use geojson::GeoJson;
use grid::{BoundingBox, CellIndex, H3Grid};

#[derive(Clone, Debug)]
pub struct TerritoryPartition {
    pub code: String,
    pub name: String,
    pub bbox: BoundingBox,
    pub cells: Vec<CellIndex>,
}

#[derive(Clone, Debug)]
pub struct Territory {
    pub partitions: Vec<TerritoryPartition>,
    pub duplicate_cells_removed: usize,
}

impl Territory {
    pub fn load(path: &Path, selected_codes: &[String], grid: H3Grid) -> anyhow::Result<Self> {
        let document = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read territory file {}", path.display()))?;
        Self::from_geojson(&document, selected_codes, grid)
            .with_context(|| format!("failed to parse territory file {}", path.display()))
    }

    fn from_geojson(
        document: &str,
        selected_codes: &[String],
        grid: H3Grid,
    ) -> anyhow::Result<Self> {
        let GeoJson::FeatureCollection(collection) = document.parse::<GeoJson>()? else {
            anyhow::bail!("territory GeoJSON must be a FeatureCollection");
        };
        let selected = selected_codes
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let mut geometries = HashMap::<String, (String, Geometry<f64>)>::new();
        for feature in collection.features {
            let properties = feature
                .properties
                .as_ref()
                .context("territory feature has no properties")?;
            let code = string_property(properties, "code")?;
            if !is_metropolitan_department(&code)
                || (!selected.is_empty() && !selected.contains(code.as_str()))
            {
                continue;
            }
            let name = string_property(properties, "nom")?;
            let geometry = feature
                .geometry
                .context("territory feature has no geometry")?
                .try_into()
                .context("territory feature has invalid geometry")?;
            anyhow::ensure!(
                matches!(geometry, Geometry::Polygon(_) | Geometry::MultiPolygon(_)),
                "department {code} is not polygonal"
            );
            anyhow::ensure!(
                geometries.insert(code.clone(), (name, geometry)).is_none(),
                "department code {code} appears more than once"
            );
        }
        if selected.is_empty() {
            anyhow::ensure!(
                geometries.len() == 96,
                "expected 96 metropolitan departments, found {}",
                geometries.len()
            );
        } else {
            let missing = selected
                .iter()
                .filter(|code| !geometries.contains_key(**code))
                .copied()
                .collect::<Vec<_>>();
            anyhow::ensure!(
                missing.is_empty(),
                "unknown department codes: {}",
                missing.join(",")
            );
        }

        let mut codes = geometries.keys().cloned().collect::<Vec<_>>();
        codes.sort();
        let mut owned_cells = HashSet::new();
        let mut duplicate_cells_removed = 0;
        let mut partitions = Vec::with_capacity(codes.len());
        for code in codes {
            let (name, geometry) = geometries
                .remove(&code)
                .expect("code was collected from map");
            let rectangle = geometry
                .bounding_rect()
                .with_context(|| format!("department {code} has an empty geometry"))?;
            let bbox = BoundingBox::new(
                rectangle.min().x,
                rectangle.min().y,
                rectangle.max().x,
                rectangle.max().y,
            )?;
            let mut cells = grid.cells_for_geometry(&geometry)?;
            cells.retain(|cell| {
                if owned_cells.insert(*cell) {
                    true
                } else {
                    duplicate_cells_removed += 1;
                    false
                }
            });
            anyhow::ensure!(!cells.is_empty(), "department {code} has no H3 cells");
            partitions.push(TerritoryPartition {
                code,
                name,
                bbox,
                cells,
            });
        }
        anyhow::ensure!(!partitions.is_empty(), "territory selection is empty");
        Ok(Self {
            partitions,
            duplicate_cells_removed,
        })
    }

    pub fn cell_count(&self) -> usize {
        self.partitions
            .iter()
            .map(|partition| partition.cells.len())
            .sum()
    }
}

fn string_property(
    properties: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
) -> anyhow::Result<String> {
    properties
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("territory feature has no string `{key}` property"))
}

fn is_metropolitan_department(code: &str) -> bool {
    matches!(code, "2A" | "2B")
        || code
            .parse::<u8>()
            .is_ok_and(|number| (1..=95).contains(&number) && number != 20)
}

#[cfg(test)]
mod tests {
    use grid::H3Grid;

    use super::Territory;

    const DEPARTMENTS: &str = r#"{
      "type":"FeatureCollection",
      "features":[
        {"type":"Feature","properties":{"code":"11","nom":"Aude"},"geometry":{"type":"Polygon","coordinates":[[[2.0,43.0],[2.1,43.0],[2.1,43.1],[2.0,43.1],[2.0,43.0]]]}},
        {"type":"Feature","properties":{"code":"34","nom":"Hérault"},"geometry":{"type":"Polygon","coordinates":[[[2.1,43.0],[2.2,43.0],[2.2,43.1],[2.1,43.1],[2.1,43.0]]]}},
        {"type":"Feature","properties":{"code":"971","nom":"Guadeloupe"},"geometry":{"type":"Polygon","coordinates":[[[-61.8,15.8],[-61.7,15.8],[-61.7,15.9],[-61.8,15.9],[-61.8,15.8]]]}}
      ]
    }"#;

    #[test]
    fn filters_selected_metropolitan_departments() {
        let territory = Territory::from_geojson(
            DEPARTMENTS,
            &["11".to_owned(), "34".to_owned()],
            H3Grid::new(8).expect("grid"),
        )
        .expect("territory");

        assert_eq!(territory.partitions.len(), 2);
        assert_eq!(territory.partitions[0].code, "11");
        assert_eq!(territory.partitions[1].name, "Hérault");
        assert!(territory.cell_count() > 0);
    }

    #[test]
    fn rejects_unknown_selected_codes() {
        let error = Territory::from_geojson(
            DEPARTMENTS,
            &["09".to_owned()],
            H3Grid::new(8).expect("grid"),
        )
        .expect_err("unknown code");

        assert!(error.to_string().contains("unknown department codes: 09"));
    }
}
