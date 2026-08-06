CREATE TABLE IF NOT EXISTS reference.commune_boundaries (
    insee_code TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    postal_codes TEXT[] NOT NULL DEFAULT '{}',
    boundary JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT commune_boundaries_insee_code_format_check
        CHECK (insee_code ~ '^([0-9]{5}|2[AB][0-9]{3})$'),
    CONSTRAINT commune_boundaries_name_not_blank_check
        CHECK (BTRIM(name) <> ''),
    CONSTRAINT commune_boundaries_boundary_object_check
        CHECK (JSONB_TYPEOF(boundary) = 'object'),
    CONSTRAINT commune_boundaries_updated_at_check
        CHECK (updated_at >= created_at)
);

COMMENT ON TABLE reference.commune_boundaries IS
    'Real commune (municipality) boundary polygons keyed by INSEE code, used to clip risk cells for the client-facing commune view. Read-only reference data; never written by the scoring or scheduler paths.';
COMMENT ON COLUMN reference.commune_boundaries.insee_code IS
    'Five-character French INSEE municipality code, e.g. 31490 for Saint-Jory.';
COMMENT ON COLUMN reference.commune_boundaries.postal_codes IS
    'Postal codes served by this commune; informational only, not used for geometry resolution.';
COMMENT ON COLUMN reference.commune_boundaries.boundary IS
    'Canonical GeoJSON Polygon or MultiPolygon geometry, parsed directly by the Rust application via the geojson crate to resolve H3 cell coverage. No PostGIS geometry column exists yet; add one alongside real ST_* query needs rather than speculatively.';
