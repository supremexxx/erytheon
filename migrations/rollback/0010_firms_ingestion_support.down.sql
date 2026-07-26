DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM raw.firms_observations) THEN
        RAISE EXCEPTION
            'rollback refused: FIRMS raw observations exist';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM ops.pipeline_runs
        WHERE import_batch_id IN (
            SELECT id
            FROM ops.import_batches
            WHERE source_id = '00000000-0000-4000-8000-000000000010'::UUID
        )
    ) THEN
        RAISE EXCEPTION
            'rollback refused: NASA FIRMS pipeline runs exist';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM ops.import_batches
        WHERE source_id = '00000000-0000-4000-8000-000000000010'::UUID
    ) THEN
        RAISE EXCEPTION
            'rollback refused: NASA FIRMS import batches exist';
    END IF;
END
$$;

DROP INDEX IF EXISTS raw.firms_observations_batch_source_record_unique;

DELETE FROM reference.data_sources
WHERE id = '00000000-0000-4000-8000-000000000010'::UUID
  AND code = 'nasa_firms';
