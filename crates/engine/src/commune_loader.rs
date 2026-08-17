//! Loads one commune (municipality) boundary `GeoJSON` fixture into
//! `reference.commune_boundaries`, generically by INSEE code.

use std::{collections::BTreeMap, path::Path};

use anyhow::Context as _;
use geojson::GeoJson;
use grid::{H3Grid, cell_to_db};
use sha2::{Digest as _, Sha256};
use store::CommuneCatalogEntry;

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
        let insee_code = insee_code.to_uppercase();
        let parsed = document.parse::<GeoJson>()?;
        let (geometry, properties) = match parsed {
            GeoJson::Geometry(geometry) => (geometry, None),
            GeoJson::Feature(feature) => (
                feature
                    .geometry
                    .context("commune boundary feature has no geometry")?,
                feature.properties,
            ),
            GeoJson::FeatureCollection(collection) => {
                anyhow::ensure!(
                    collection.features.len() == 1,
                    "commune boundary FeatureCollection must contain exactly one feature, found {}",
                    collection.features.len()
                );
                let feature = collection.features.into_iter().next().expect("len == 1");
                (
                    feature
                        .geometry
                        .context("commune boundary feature has no geometry")?,
                    feature.properties,
                )
            }
        };
        if let Some(properties) = &properties {
            check_property_matches(properties, "code", &insee_code)?;
            check_property_matches(properties, "nom", name)?;
        }
        let geo_geometry: geo::Geometry<f64> = geometry
            .clone()
            .try_into()
            .context("commune boundary geometry is invalid")?;
        anyhow::ensure!(
            grid::is_polygonal(&geo_geometry),
            "commune {insee_code} boundary is not polygonal"
        );
        Ok(Self {
            insee_code,
            name: name.to_owned(),
            postal_codes: postal_codes.to_vec(),
            geometry: serde_json::to_value(geometry)?,
        })
    }
}

/// Parses a complete official commune `FeatureCollection` and builds the
/// deterministic H3-centroid ownership used by BLUE bulletins.
pub fn load_catalog(
    path: &Path,
    grid: H3Grid,
) -> anyhow::Result<(Vec<CommuneCatalogEntry>, String)> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read commune catalog {}", path.display()))?;
    let checksum = format!("{:x}", Sha256::digest(&bytes));
    let parsed = std::str::from_utf8(&bytes)?.parse::<GeoJson>()?;
    let GeoJson::FeatureCollection(collection) = parsed else {
        anyhow::bail!("commune catalog must be a GeoJSON FeatureCollection");
    };
    let mut ownership = BTreeMap::new();
    let mut entries = Vec::with_capacity(collection.features.len());
    for feature in collection.features {
        let properties = feature
            .properties
            .context("commune feature has no properties")?;
        // Paris, Lyon and Marseille are present twice in the official
        // catalog: once as the legal commune and once through subordinate
        // municipal arrondissements. BLUE reports at commune/city level, so
        // retain the parent commune and avoid overlapping H3 ownership.
        if properties.contains_key("commune") {
            continue;
        }
        let code = required_property(&properties, "code")?.to_uppercase();
        let name = required_property(&properties, "nom")?.to_owned();
        anyhow::ensure!(
            valid_insee_code(&code),
            "invalid commune code {code} in catalog"
        );
        let geometry = feature
            .geometry
            .context("commune feature has no geometry")?;
        let geo_geometry: geo::Geometry<f64> = geometry.clone().try_into()?;
        anyhow::ensure!(
            grid::is_polygonal(&geo_geometry),
            "commune {code} boundary is not polygonal"
        );
        let cells = grid.cells_for_geometry(&geo_geometry)?;
        let mut h3_cells = Vec::with_capacity(cells.len());
        for cell in cells {
            let h3 = cell_to_db(cell);
            if let Some(previous) = ownership.insert(h3, code.clone()) {
                anyhow::ensure!(
                    previous == code,
                    "H3 cell {h3} belongs to both {previous} and {code}"
                );
            }
            h3_cells.push(h3);
        }
        h3_cells.sort_unstable();
        entries.push(CommuneCatalogEntry {
            insee_code: code,
            name,
            department_code: optional_property(&properties, "departement"),
            region_code: optional_property(&properties, "region"),
            boundary: serde_json::to_value(geometry)?,
            h3_cells,
        });
    }
    entries.sort_by(|left, right| left.insee_code.cmp(&right.insee_code));
    Ok((entries, checksum))
}

fn required_property<'a>(
    properties: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> anyhow::Result<&'a str> {
    properties
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("commune property {key} is missing"))
}

fn optional_property(
    properties: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    properties
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn valid_insee_code(code: &str) -> bool {
    code.len() == 5
        && if let Some(rest) = code.strip_prefix("2A").or_else(|| code.strip_prefix("2B")) {
            rest.bytes().all(|byte| byte.is_ascii_digit())
        } else {
            code.bytes().all(|byte| byte.is_ascii_digit())
        }
}

/// Fails loudly when a `GeoJSON` feature's own `properties[key]` string
/// disagrees with the value the operator supplied on the command line
/// -- catching commune/geometry mismatches (wrong file, transposed
/// code) instead of silently trusting whichever value is discarded.
/// Case-insensitive and trimmed, since accents and capitalization
/// conventions vary across boundary sources; only checked when the
/// property is present as a string.
fn check_property_matches(
    properties: &serde_json::Map<String, serde_json::Value>,
    key: &'static str,
    expected: &str,
) -> anyhow::Result<()> {
    let Some(actual) = properties.get(key).and_then(serde_json::Value::as_str) else {
        return Ok(());
    };
    anyhow::ensure!(
        actual.trim().eq_ignore_ascii_case(expected.trim()),
        "commune boundary file property `{key}` is \"{actual}\", \
         which does not match the supplied value \"{expected}\""
    );
    Ok(())
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

    #[test]
    fn normalizes_a_lowercase_corsican_code() {
        // Bare geometry, deliberately without a `properties` block, so
        // this test isolates case normalization from the separate
        // properties cross-check covered below.
        let bare_geometry = r#"{"type":"Polygon","coordinates":[[[1.35,43.75],[1.40,43.75],[1.40,43.80],[1.35,43.80],[1.35,43.75]]]}"#;
        let fixture = CommuneBoundaryFixture::from_geojson(bare_geometry, "2a004", "Ajaccio", &[])
            .expect("fixture");
        assert_eq!(fixture.insee_code, "2A004");
    }

    #[test]
    fn rejects_a_code_mismatched_with_the_file_properties() {
        let error =
            CommuneBoundaryFixture::from_geojson(SQUARE, "75056", "Saint-Jory", &[]).unwrap_err();
        assert!(error.to_string().contains("property `code`"));
    }

    #[test]
    fn rejects_a_name_mismatched_with_the_file_properties() {
        let error =
            CommuneBoundaryFixture::from_geojson(SQUARE, "31490", "Ramonville", &[]).unwrap_err();
        assert!(error.to_string().contains("property `nom`"));
    }

    #[test]
    fn accepts_missing_properties_without_a_cross_check() {
        let bare_geometry = r#"{"type":"Polygon","coordinates":[[[1.35,43.75],[1.40,43.75],[1.40,43.80],[1.35,43.80],[1.35,43.75]]]}"#;
        let fixture = CommuneBoundaryFixture::from_geojson(bare_geometry, "31490", "Anything", &[])
            .expect("fixture");
        assert_eq!(fixture.name, "Anything");
    }
}
