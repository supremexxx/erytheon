//! Commune (municipality) boundary reference data.
//!
//! Boundaries are read-only reference data used to clip risk cells for the
//! client-facing commune view; nothing in the scoring or scheduler paths
//! writes to this table.

use geo::{BoundingRect as _, Geometry};
use grid::BoundingBox;
use sqlx::Row;

use crate::{Store, StoreError};

/// A commune boundary resolved from `reference.commune_boundaries`.
#[derive(Clone, Debug)]
pub struct CommuneBoundary {
    /// Five-character INSEE municipality code.
    pub insee_code: String,
    /// Commune name.
    pub name: String,
    /// Postal codes served by the commune.
    pub postal_codes: Vec<String>,
    /// Polygonal geometry in WGS84.
    pub geometry: Geometry<f64>,
    /// Bounding box of `geometry`.
    pub bbox: BoundingBox,
}

impl Store {
    /// Inserts or replaces a commune boundary.
    ///
    /// `boundary` must be a `GeoJSON` `Polygon` or `MultiPolygon` geometry
    /// object (not a `Feature` or `FeatureCollection`).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database operation fails.
    pub async fn upsert_commune_boundary(
        &self,
        insee_code: &str,
        name: &str,
        postal_codes: &[String],
        boundary: &serde_json::Value,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO reference.commune_boundaries
                (insee_code, name, postal_codes, boundary, updated_at)
             VALUES ($1, $2, $3, $4, NOW())
             ON CONFLICT (insee_code) DO UPDATE SET
                name = EXCLUDED.name,
                postal_codes = EXCLUDED.postal_codes,
                boundary = EXCLUDED.boundary,
                updated_at = NOW()",
        )
        .bind(insee_code)
        .bind(name)
        .bind(postal_codes)
        .bind(boundary)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Looks up a commune boundary by INSEE code.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the database operation fails or the
    /// persisted boundary is not a valid, non-empty polygonal geometry.
    pub async fn commune_boundary(
        &self,
        insee_code: &str,
    ) -> Result<Option<CommuneBoundary>, StoreError> {
        let Some(row) = sqlx::query(
            "SELECT name, postal_codes, boundary
               FROM reference.commune_boundaries
              WHERE insee_code = $1",
        )
        .bind(insee_code)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        let name: String = row.try_get("name")?;
        let postal_codes: Vec<String> = row.try_get("postal_codes")?;
        let boundary: serde_json::Value = row.try_get("boundary")?;
        let geometry: Geometry<f64> = serde_json::from_value::<geojson::Geometry>(boundary)
            .map_err(|error| StoreError::InvalidCommuneBoundary(error.to_string()))?
            .try_into()
            .map_err(|error: geojson::Error| {
                StoreError::InvalidCommuneBoundary(error.to_string())
            })?;
        if !grid::is_polygonal(&geometry) {
            return Err(StoreError::InvalidCommuneBoundary(format!(
                "commune {insee_code} boundary is not polygonal"
            )));
        }
        let rectangle = geometry.bounding_rect().ok_or_else(|| {
            StoreError::InvalidCommuneBoundary(format!("commune {insee_code} boundary is empty"))
        })?;
        let bbox = BoundingBox::new(
            rectangle.min().x,
            rectangle.min().y,
            rectangle.max().x,
            rectangle.max().y,
        )?;
        Ok(Some(CommuneBoundary {
            insee_code: insee_code.to_owned(),
            name,
            postal_codes,
            geometry,
            bbox,
        }))
    }
}
