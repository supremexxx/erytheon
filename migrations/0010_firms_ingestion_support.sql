INSERT INTO reference.data_sources (
    id,
    code,
    name,
    category,
    provider,
    description,
    base_url,
    is_active
)
VALUES (
    '00000000-0000-4000-8000-000000000010'::UUID,
    'nasa_firms',
    'NASA FIRMS',
    'satellite',
    'NASA',
    'Near-real-time satellite active-fire detections from the FIRMS service.',
    'https://firms.modaps.eosdis.nasa.gov/api/area/csv',
    TRUE
)
ON CONFLICT (code) DO NOTHING;

CREATE UNIQUE INDEX IF NOT EXISTS firms_observations_batch_source_record_unique
    ON raw.firms_observations (import_batch_id, source_record_id)
    WHERE source_record_id IS NOT NULL;

COMMENT ON INDEX raw.firms_observations_batch_source_record_unique IS
    'Prevents accidental duplicate source rows inside one import batch while preserving the same detection across later batches.';
