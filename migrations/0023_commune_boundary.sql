CREATE TABLE IF NOT EXISTS reference.commune_boundaries (
    insee_code TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    postal_codes TEXT[] NOT NULL DEFAULT '{}',
    geom geometry(MultiPolygon, 4326) NOT NULL,
    boundary JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT commune_boundaries_insee_code_format_check
        CHECK (insee_code ~ '^([0-9]{5}|2[AB][0-9]{3})$'),
    CONSTRAINT commune_boundaries_name_not_blank_check
        CHECK (BTRIM(name) <> ''),
    CONSTRAINT commune_boundaries_geom_check
        CHECK (
            ST_SRID(geom) = 4326
            AND GeometryType(geom) = 'MULTIPOLYGON'
        ),
    CONSTRAINT commune_boundaries_boundary_object_check
        CHECK (JSONB_TYPEOF(boundary) = 'object'),
    CONSTRAINT commune_boundaries_updated_at_check
        CHECK (updated_at >= created_at)
);

CREATE INDEX commune_boundaries_geom_gix
    ON reference.commune_boundaries
    USING GIST (geom);

COMMENT ON TABLE reference.commune_boundaries IS
    'Real commune (municipality) boundary polygons keyed by INSEE code, used to clip risk cells for the client-facing commune view. Read-only reference data; never written by the scoring or scheduler paths.';
COMMENT ON COLUMN reference.commune_boundaries.insee_code IS
    'Five-character French INSEE municipality code, e.g. 31446 for Saint-Jory.';
COMMENT ON COLUMN reference.commune_boundaries.postal_codes IS
    'Postal codes served by this commune; informational only, not used for geometry resolution.';
COMMENT ON COLUMN reference.commune_boundaries.geom IS
    'PostGIS geometry retained for spatial indexing and future ST_* queries; not read by the current API, which parses the boundary column instead.';
COMMENT ON COLUMN reference.commune_boundaries.boundary IS
    'Canonical GeoJSON Polygon or MultiPolygon geometry, parsed directly by the Rust application via the geojson crate to resolve H3 cell coverage.';
