//! H3 grid generation and spatial projection.

use std::str::FromStr;

use geo::{Geometry, LineString, Polygon};
use h3o::geom::{ContainmentMode, TilerBuilder};
pub use h3o::{CellIndex, LatLng, Resolution};

/// Geographic bounding box in longitude/latitude coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingBox {
    /// Western longitude.
    pub west: f64,
    /// Southern latitude.
    pub south: f64,
    /// Eastern longitude.
    pub east: f64,
    /// Northern latitude.
    pub north: f64,
}

impl BoundingBox {
    /// Creates and validates a bounding box.
    ///
    /// # Errors
    ///
    /// Returns [`GridError`] when coordinates are non-finite, outside WGS84
    /// limits, or not ordered west-to-east and south-to-north.
    pub fn new(west: f64, south: f64, east: f64, north: f64) -> Result<Self, GridError> {
        if [west, south, east, north]
            .into_iter()
            .any(|coordinate| !coordinate.is_finite())
        {
            return Err(GridError::InvalidBoundingBox(
                "coordinates must be finite".to_owned(),
            ));
        }
        if !(-180.0..=180.0).contains(&west)
            || !(-180.0..=180.0).contains(&east)
            || !(-90.0..=90.0).contains(&south)
            || !(-90.0..=90.0).contains(&north)
            || west >= east
            || south >= north
        {
            return Err(GridError::InvalidBoundingBox(
                "coordinates are outside WGS84 bounds or are not ordered".to_owned(),
            ));
        }
        Ok(Self {
            west,
            south,
            east,
            north,
        })
    }

    /// Returns the FIRMS API coordinate representation.
    #[must_use]
    pub fn api_coordinates(self) -> String {
        format!("{},{},{},{}", self.west, self.south, self.east, self.north)
    }

    /// Reports whether a WGS84 point lies inside the bounding box.
    #[must_use]
    pub fn contains(self, latitude: f64, longitude: f64) -> bool {
        (self.south..=self.north).contains(&latitude)
            && (self.west..=self.east).contains(&longitude)
    }
}

impl FromStr for BoundingBox {
    type Err = GridError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let coordinates = value
            .split(',')
            .map(str::trim)
            .map(str::parse::<f64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| GridError::InvalidBoundingBox(error.to_string()))?;
        let [west, south, east, north] = coordinates.as_slice() else {
            return Err(GridError::InvalidBoundingBox(
                "expected west,south,east,north".to_owned(),
            ));
        };
        Self::new(*west, *south, *east, *north)
    }
}

/// H3 projection configured at one resolution.
#[derive(Clone, Copy, Debug)]
pub struct H3Grid {
    resolution: Resolution,
}

impl H3Grid {
    /// Creates a grid at an H3 resolution from 0 through 15.
    ///
    /// # Errors
    ///
    /// Returns [`GridError`] for an invalid resolution.
    pub fn new(resolution: u8) -> Result<Self, GridError> {
        let resolution = Resolution::try_from(resolution)
            .map_err(|_| GridError::InvalidResolution { resolution })?;
        Ok(Self { resolution })
    }

    /// Returns the configured resolution.
    #[must_use]
    pub const fn resolution(self) -> Resolution {
        self.resolution
    }

    /// Projects a WGS84 point to its containing H3 cell.
    ///
    /// # Errors
    ///
    /// Returns [`GridError`] for invalid latitude or longitude.
    pub fn cell_for_point(self, latitude: f64, longitude: f64) -> Result<CellIndex, GridError> {
        let point = LatLng::new(latitude, longitude)
            .map_err(|error| GridError::InvalidCoordinate(error.to_string()))?;
        Ok(point.to_cell(self.resolution))
    }

    /// Returns every H3 cell required to cover a bounding box.
    ///
    /// Boundary-intersecting cells are included so the AOI has no gaps.
    ///
    /// # Errors
    ///
    /// Returns [`GridError`] if H3 rejects the bounding-box polygon.
    pub fn cells_for_bbox(self, bbox: BoundingBox) -> Result<Vec<CellIndex>, GridError> {
        let exterior = LineString::from(vec![
            (bbox.west, bbox.south),
            (bbox.east, bbox.south),
            (bbox.east, bbox.north),
            (bbox.west, bbox.north),
            (bbox.west, bbox.south),
        ]);
        let mut tiler = TilerBuilder::new(self.resolution)
            .containment_mode(ContainmentMode::Covers)
            .build();
        tiler
            .add(Polygon::new(exterior, Vec::new()))
            .map_err(|error| GridError::InvalidGeometry(error.to_string()))?;
        let mut cells = tiler.into_coverage().collect::<Vec<_>>();
        cells.sort_unstable();
        cells.dedup();
        Ok(cells)
    }

    /// Returns H3 cells whose centroids belong to a polygonal geometry.
    ///
    /// Centroid containment gives adjacent administrative partitions unique
    /// ownership of border cells and avoids covering the surrounding sea.
    ///
    /// # Errors
    ///
    /// Returns [`GridError`] for unsupported or invalid geometry.
    pub fn cells_for_geometry(self, geometry: &Geometry<f64>) -> Result<Vec<CellIndex>, GridError> {
        let mut tiler = TilerBuilder::new(self.resolution)
            .containment_mode(ContainmentMode::ContainsCentroid)
            .build();
        match geometry {
            Geometry::Polygon(polygon) => tiler.add(polygon.clone()),
            Geometry::MultiPolygon(polygons) => tiler.add_batch(polygons.0.clone()),
            _ => {
                return Err(GridError::InvalidGeometry(
                    "expected a Polygon or MultiPolygon".to_owned(),
                ));
            }
        }
        .map_err(|error| GridError::InvalidGeometry(error.to_string()))?;
        let mut cells = tiler.into_coverage().collect::<Vec<_>>();
        cells.sort_unstable();
        cells.dedup();
        Ok(cells)
    }

    /// Returns the WGS84 centre of a cell.
    #[must_use]
    pub fn cell_center(self, cell: CellIndex) -> LatLng {
        LatLng::from(cell)
    }

    /// Returns all cells up to the requested H3 grid distance.
    #[must_use]
    pub fn neighbors(self, cell: CellIndex, distance: u32) -> Vec<CellIndex> {
        cell.grid_disk(distance)
    }

    /// Returns cells paired with their H3 grid distance from the origin.
    #[must_use]
    pub fn neighbors_with_distance(self, cell: CellIndex, distance: u32) -> Vec<(CellIndex, u32)> {
        cell.grid_disk_distances(distance)
    }
}

/// Converts an H3 cell's unsigned bits for storage in `PostgreSQL` `BIGINT`.
#[must_use]
pub fn cell_to_db(cell: CellIndex) -> i64 {
    i64::from_be_bytes(u64::from(cell).to_be_bytes())
}

/// Reconstructs an H3 cell from its `PostgreSQL` `BIGINT` representation.
///
/// # Errors
///
/// Returns [`GridError`] if the stored bits do not encode a valid H3 cell.
pub fn cell_from_db(value: i64) -> Result<CellIndex, GridError> {
    let bits = u64::from_be_bytes(value.to_be_bytes());
    CellIndex::try_from(bits).map_err(|error| GridError::InvalidCell(error.to_string()))
}

/// Spatial projection failures.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GridError {
    /// Bounding box validation failed.
    #[error("invalid bounding box: {0}")]
    InvalidBoundingBox(String),
    /// H3 resolution validation failed.
    #[error("invalid H3 resolution {resolution}")]
    InvalidResolution {
        /// Invalid resolution number.
        resolution: u8,
    },
    /// Point coordinate validation failed.
    #[error("invalid coordinate: {0}")]
    InvalidCoordinate(String),
    /// AOI polygon conversion failed.
    #[error("invalid AOI geometry: {0}")]
    InvalidGeometry(String),
    /// Stored H3 bits are invalid.
    #[error("invalid H3 cell: {0}")]
    InvalidCell(String),
}

#[cfg(test)]
mod tests {
    use geo::{Geometry, polygon};

    use super::{BoundingBox, H3Grid, cell_from_db, cell_to_db};

    #[test]
    fn projects_and_round_trips_a_cell() {
        let grid = H3Grid::new(9).expect("valid resolution");
        let cell = grid
            .cell_for_point(43.2122, 2.3537)
            .expect("valid Carcassonne coordinate");

        assert_eq!(cell.resolution(), grid.resolution());
        assert_eq!(cell_from_db(cell_to_db(cell)).expect("valid cell"), cell);
    }

    #[test]
    fn parses_the_default_aoi() {
        let bbox: BoundingBox = "1.68,42.57,3.26,43.46".parse().expect("valid bbox");
        assert!((bbox.west - 1.68).abs() < f64::EPSILON);
        assert!((bbox.north - 43.46).abs() < f64::EPSILON);
    }

    #[test]
    fn covers_a_small_bbox_without_duplicate_cells() {
        let grid = H3Grid::new(9).expect("valid resolution");
        let bbox = BoundingBox::new(2.34, 43.20, 2.36, 43.22).expect("valid bbox");
        let cells = grid.cells_for_bbox(bbox).expect("valid coverage");

        assert!(!cells.is_empty());
        assert!(cells.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn adjacent_polygons_have_unique_centroid_cells() {
        let grid = H3Grid::new(8).expect("valid resolution");
        let west = Geometry::Polygon(polygon![
            (x: 2.0, y: 43.0), (x: 2.1, y: 43.0),
            (x: 2.1, y: 43.1), (x: 2.0, y: 43.1),
            (x: 2.0, y: 43.0),
        ]);
        let east = Geometry::Polygon(polygon![
            (x: 2.1, y: 43.0), (x: 2.2, y: 43.0),
            (x: 2.2, y: 43.1), (x: 2.1, y: 43.1),
            (x: 2.1, y: 43.0),
        ]);
        let west_cells = grid.cells_for_geometry(&west).expect("west coverage");
        let east_cells = grid.cells_for_geometry(&east).expect("east coverage");

        assert!(!west_cells.is_empty());
        assert!(!east_cells.is_empty());
        assert!(west_cells.iter().all(|cell| !east_cells.contains(cell)));
    }
}
