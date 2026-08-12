BEGIN;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM blue.forecast_bulletins) THEN
        RAISE EXCEPTION 'refusing rollback 0025: BLUE forecast bulletins exist';
    END IF;
    IF EXISTS (SELECT 1 FROM reference.commune_h3_cells) THEN
        RAISE EXCEPTION 'refusing rollback 0025: commune H3 mappings exist';
    END IF;
END;
$$;

DROP TRIGGER blue_alerts_published_immutable ON blue.forecast_alerts;
DROP TRIGGER blue_index_archives_published_immutable ON blue.forecast_index_archives;
DROP TRIGGER blue_bulletins_published_immutable ON blue.forecast_bulletins;
DROP FUNCTION blue.forbid_published_forecast_change();
DROP TABLE blue.forecast_evaluations;
DROP TABLE blue.forecast_alerts;
DROP TABLE blue.forecast_index_archives;
DROP TABLE blue.forecast_bulletins;
DROP SCHEMA blue;
DROP TABLE reference.commune_h3_cells;
ALTER TABLE reference.commune_boundaries
    DROP COLUMN source_checksum,
    DROP COLUMN source_version,
    DROP COLUMN region_code,
    DROP COLUMN department_code;
DELETE FROM _sqlx_migrations WHERE version=25;

COMMIT;
