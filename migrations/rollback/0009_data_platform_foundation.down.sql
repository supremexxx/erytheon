DO $$
BEGIN
    IF TO_REGCLASS('raw.firms_observations') IS NOT NULL
       AND EXISTS (SELECT 1 FROM raw.firms_observations LIMIT 1) THEN
        RAISE EXCEPTION
            'rollback refused: raw.firms_observations contains data';
    END IF;
    IF TO_REGCLASS('ops.pipeline_runs') IS NOT NULL
       AND EXISTS (SELECT 1 FROM ops.pipeline_runs LIMIT 1) THEN
        RAISE EXCEPTION
            'rollback refused: ops.pipeline_runs contains data';
    END IF;
    IF TO_REGCLASS('ops.import_batches') IS NOT NULL
       AND EXISTS (SELECT 1 FROM ops.import_batches LIMIT 1) THEN
        RAISE EXCEPTION
            'rollback refused: ops.import_batches contains data';
    END IF;
    IF TO_REGCLASS('reference.data_sources') IS NOT NULL
       AND EXISTS (SELECT 1 FROM reference.data_sources LIMIT 1) THEN
        RAISE EXCEPTION
            'rollback refused: reference.data_sources contains data';
    END IF;
END
$$;

DROP TABLE IF EXISTS raw.firms_observations;
DROP TABLE IF EXISTS ops.pipeline_runs;
DROP TABLE IF EXISTS ops.import_batches;
DROP TABLE IF EXISTS reference.data_sources;

DROP SCHEMA IF EXISTS raw RESTRICT;
DROP SCHEMA IF EXISTS staging RESTRICT;
DROP SCHEMA IF EXISTS reference RESTRICT;
DROP SCHEMA IF EXISTS environment RESTRICT;
DROP SCHEMA IF EXISTS human RESTRICT;
DROP SCHEMA IF EXISTS fire RESTRICT;
DROP SCHEMA IF EXISTS features RESTRICT;
DROP SCHEMA IF EXISTS risk RESTRICT;
DROP SCHEMA IF EXISTS validation RESTRICT;
DROP SCHEMA IF EXISTS ml RESTRICT;
DROP SCHEMA IF EXISTS serving RESTRICT;
DROP SCHEMA IF EXISTS ops RESTRICT;
