//! Loads one commune (municipality) boundary `GeoJSON` fixture into
//! `reference.commune_boundaries`, generically by INSEE code.

use std::path::Path;

use anyhow::Context as _;
use geojson::GeoJson;

/// A commune boundary parsed from a local `GeoJSON` file, ready to persist.
#[derive(Clone, Debug)]
pub struct CommuneBoundaryFixture {
    pub insee_code: String,
    pub name: String,
    pub postal_codes: Vec<String>,
    pub geometry: serde_json::Value,
}

impl CommuneBoundaryFixture {
    pub fn load(
        path: &Path,
        insee_code: &str,
        name: &str,
        postal_codes: &[String],
    ) -> anyhow::Result<Self> {
        let document = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read commune boundary file {}", path.display()))?;
        Self::from_geojson(&document, insee_code, name, postal_codes)
            .with_context(|| format!("failed to parse commune boundary file {}", path.display()))
    }

    fn from_geojson(
        document: &str,
        insee_code: &str,
        name: &str,
        postal_codes: &[String],
    ) -> anyhow::Result<Self> {
        let parsed = document.parse::<GeoJson>()?;
        let geometry = match parsed {
            GeoJson::Geometry(geometry) => geometry,
            GeoJson::Feature(feature) => feature
                .geometry
                .context("commune boundary feature has no geometry")?,
            GeoJson::FeatureCollection(collection) => {
                anyhow::ensure!(
                    collection.features.len() == 1,
                    "commune boundary FeatureCollection must contain exactly one feature, found {}",
                    collection.features.len()
                );
                collection.features[0]
                    .geometry
                    .clone()
                    .context("commune boundary feature has no geometry")?
            }
        };
        let geo_geometry: geo::Geometry<f64> = geometry
            .clone()
            .try_into()
            .context("commune boundary geometry is invalid")?;
        anyhow::ensure!(
            matches!(
                geo_geometry,
                geo::Geometry::Polygon(_) | geo::Geometry::MultiPolygon(_)
            ),
            "commune {insee_code} boundary is not polygonal"
        );
        Ok(Self {
            insee_code: insee_code.to_owned(),
            name: name.to_owned(),
            postal_codes: postal_codes.to_vec(),
            geometry: serde_json::to_value(geometry)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CommuneBoundaryFixture;

    const SQUARE: &str = r#"{
      "type":"Feature",
      "properties":{"nom":"Saint-Jory","code":"31490"},
      "geometry":{"type":"Polygon","coordinates":[[[1.35,43.75],[1.40,43.75],[1.40,43.80],[1.35,43.80],[1.35,43.75]]]}
    }"#;

    #[test]
    fn parses_a_single_feature_boundary() {
        let fixture = CommuneBoundaryFixture::from_geojson(
            SQUARE,
            "31490",
            "Saint-Jory",
            &["31790".to_owned()],
        )
        .expect("fixture");

        assert_eq!(fixture.insee_code, "31490");
        assert_eq!(fixture.name, "Saint-Jory");
        assert_eq!(fixture.postal_codes, vec!["31790".to_owned()]);
        assert_eq!(fixture.geometry["type"], "Polygon");
    }

    #[test]
    fn rejects_non_polygonal_geometry() {
        let point = r#"{"type":"Point","coordinates":[1.37,43.77]}"#;
        let error =
            CommuneBoundaryFixture::from_geojson(point, "31490", "Saint-Jory", &[]).unwrap_err();
        assert!(error.to_string().contains("not polygonal"));
    }
}
