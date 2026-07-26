DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM fire.ignition_events) THEN
        RAISE EXCEPTION
            'rollback refused: BDIFF ignition events exist';
    END IF;

    IF EXISTS (SELECT 1 FROM staging.bdiff_events_normalized) THEN
        RAISE EXCEPTION
            'rollback refused: normalized BDIFF events exist';
    END IF;

    IF EXISTS (SELECT 1 FROM raw.bdiff_records) THEN
        RAISE EXCEPTION
            'rollback refused: raw BDIFF records exist';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM ops.pipeline_runs
        WHERE import_batch_id IN (
            SELECT id
            FROM ops.import_batches
            WHERE source_id = '00000000-0000-4000-8000-000000000011'::UUID
        )
    ) THEN
        RAISE EXCEPTION
            'rollback refused: BDIFF pipeline runs exist';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM ops.import_batches
        WHERE source_id = '00000000-0000-4000-8000-000000000011'::UUID
    ) THEN
        RAISE EXCEPTION
            'rollback refused: BDIFF import batches exist';
    END IF;
END
$$;

DROP TABLE fire.ignition_events;
DROP TABLE staging.bdiff_events_normalized;
DROP TABLE raw.bdiff_records;

DELETE FROM reference.data_sources
WHERE id = '00000000-0000-4000-8000-000000000011'::UUID
  AND code = 'bdiff';
