# ERYTHEON BDIFF pipeline

## Scope

Phase 3B.1 adds a parallel, traced BDIFF pipeline:

```text
normalized BDIFF CSV
→ ops.import_batches / ops.pipeline_runs
→ raw.bdiff_records
→ staging.bdiff_events_normalized
→ fire.ignition_events
```

The current `load-fire-history` command, `public.ignition_history`, risk model, API, and interface remain unchanged. The current model does not read `fire.ignition_events`.

## Layers

- `raw.bdiff_records` retains every inserted CSV row as a JSON object, including unknown columns and rejected values.
- `staging.bdiff_events_normalized` records deterministic parsing, grouped cause taxonomy, validation state, and machine-readable rejection reasons.
- `fire.ignition_events` stores stable valid ignition events and their original WGS84 geometry, H3 cell, and H3 resolution.
- `ops.import_batches` and `ops.pipeline_runs` retain lifecycle, parameters, safe error summaries, counters, code version, and pipeline version.

Coordinates can be municipality centroids supplied by the upstream normalization process. Phase 3B.1 never moves a coordinate, snaps an event to a combustible cell, or substitutes another geometry.

## Cause taxonomy

| Source label | Category | Subcategory |
| --- | --- | --- |
| `Malveillance` | `human_known` | `malicious` |
| `Involontaire (particulier)` | `human_known` | `private_activity_negligence` |
| `Involontaire (travaux)` | `human_known` | `work_activity` |
| `Accidentelle` | `human_known` | `accident_unspecified` |
| `Naturelle` | `natural_known` | `natural_unspecified` |
| `Inconnue` | `unknown` | `unknown_unspecified` |
| Any other non-empty label | `indeterminate` | `unmapped` |

Unknown causes remain separate. They are not human labels, negatives, natural causes, or absence of fire. NASA FIRMS supplies active-fire detections and never supplies the human-cause label.

## Validation

A staging row is rejected when the source identifier, timestamp, municipality, or cause is missing or invalid; when latitude/longitude are outside WGS84 bounds; or when surface is missing, non-numeric, non-finite, or negative.

Rejected values remain in raw and staging. They do not receive a geometry, H3 cell, or `fire.ignition_events` row.

## Idempotence and replay

- Within one batch, `(import_batch_id, source_record_id)` prevents duplicate source identities.
- `(import_batch_id, source_line_number)` also protects rows without a source identity during same-batch replay.
- A later batch may retain the same source record again in raw and staging.
- `(source_id, source_record_id)` prevents duplicate business events across batches.
- Similar events are never merged by date, coordinate, H3, cause, or surface.
- The business event retains the first valid staging row that created it. Later source observations remain traceable through their own batch, raw row, and staging row.

## Transaction strategy

One decoded normalized file is persisted in a single PostgreSQL transaction covering raw, staging, and fire. Any persistence error rolls back all three layers. The batch and run are created first and finalized in a separate atomic operation so a read or persistence failure remains visible as `failed`.

The current national file is small enough for this strategy. If future exports become materially larger, chunking must preserve a global run state and resume cursor; no chunked implementation exists in Phase 3B.1.

## CLI

Parse the deterministic fixture without writing to PostgreSQL:

```bash
cargo run -p engine -- import-bdiff \
  --path testdata/bdiff_pipeline_fixture.csv \
  --dry-run
```

Import a normalized file:

```bash
cargo run -p engine -- import-bdiff \
  --path /path/to/normalized-bdiff.csv
```

The summary contains batch/run identifiers, raw and staging counters, business inserts, already-present events, technical duplicates, duration, and status. Logs retain only the file name, never its full path, credentials, database URL, or full payload.

## Comparison queries

No reader switches to the new table. A future controlled comparison can use:

```sql
SELECT source, COUNT(*) FROM public.ignition_history GROUP BY source;
SELECT COUNT(*) FROM fire.ignition_events;

SELECT EXTRACT(YEAR FROM occurred_at)::INTEGER AS year, COUNT(*)
FROM public.ignition_history
WHERE source = 'bdiff'
GROUP BY year
ORDER BY year;

SELECT EXTRACT(YEAR FROM occurred_at)::INTEGER AS year, COUNT(*)
FROM fire.ignition_events
GROUP BY year
ORDER BY year;

SELECT cause_source, cause_category, cause_subcategory, COUNT(*)
FROM fire.ignition_events
GROUP BY cause_source, cause_category, cause_subcategory
ORDER BY cause_source;
```

Timestamp, municipality, coordinates, surface, H3, and source identity should be compared by extracting the legacy JSON payload and joining on `dedupe_key = source_record_id`. No production comparison import is authorized in Phase 3B.1.

Detailed row comparison:

```sql
WITH legacy AS (
    SELECT
        dedupe_key AS source_record_id,
        occurred_at,
        h3,
        payload->>'municipality' AS municipality,
        (payload->>'latitude')::DOUBLE PRECISION AS latitude,
        (payload->>'longitude')::DOUBLE PRECISION AS longitude,
        (payload->>'surface_ha')::DOUBLE PRECISION AS surface_ha,
        payload->>'cause' AS cause
    FROM public.ignition_history
    WHERE source = 'bdiff'
),
current AS (
    SELECT
        source_record_id,
        occurred_at,
        h3,
        municipality_source AS municipality,
        latitude_original AS latitude,
        longitude_original AS longitude,
        surface_ha,
        cause_source AS cause
    FROM fire.ignition_events
)
SELECT
    COALESCE(legacy.source_record_id, current.source_record_id) AS source_record_id,
    CASE
        WHEN legacy.source_record_id IS NULL THEN 'new_only'
        WHEN current.source_record_id IS NULL THEN 'legacy_only'
        WHEN legacy.occurred_at IS DISTINCT FROM current.occurred_at THEN 'timestamp'
        WHEN legacy.municipality IS DISTINCT FROM current.municipality THEN 'municipality'
        WHEN legacy.latitude IS DISTINCT FROM current.latitude THEN 'latitude'
        WHEN legacy.longitude IS DISTINCT FROM current.longitude THEN 'longitude'
        WHEN legacy.surface_ha IS DISTINCT FROM current.surface_ha THEN 'surface'
        WHEN legacy.h3 IS DISTINCT FROM current.h3 THEN 'h3'
        WHEN legacy.cause IS DISTINCT FROM current.cause THEN 'cause'
        ELSE 'identical'
    END AS comparison_class
FROM legacy
FULL JOIN current USING (source_record_id);
```

Rejected rows and cause audit:

```sql
SELECT validation_status, validation_errors, COUNT(*)
FROM staging.bdiff_events_normalized
GROUP BY validation_status, validation_errors
ORDER BY validation_status, validation_errors;

SELECT
    COUNT(*) FILTER (WHERE NULLIF(BTRIM(payload->>'cause'), '') IS NULL)
        AS empty_source_causes,
    COUNT(*) FILTER (WHERE parsing_status = 'rejected')
        AS rejected_raw_rows
FROM raw.bdiff_records;

SELECT cause_category, cause_subcategory, COUNT(*)
FROM fire.ignition_events
GROUP BY cause_category, cause_subcategory
ORDER BY cause_category, cause_subcategory;

SELECT source_record_id, COUNT(*)
FROM raw.bdiff_records
GROUP BY source_record_id
HAVING COUNT(*) > 1
ORDER BY COUNT(*) DESC, source_record_id;
```

Every non-identical result must be assigned one of: expected divergence, explicit improvement, source error, legacy-normalizer error, new-pipeline error, or documented limitation.

## Rollback

Before any BDIFF trace exists, apply:

```bash
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 \
  -f migrations/rollback/0011_bdiff_foundation.down.sql
```

The rollback refuses to proceed if raw rows, staging rows, business events, BDIFF batches, or BDIFF runs exist.

After real data exists:

1. Stop the new manual pipeline.
2. Restore the previous application binary if required.
3. Keep all additive tables and data.
4. Apply a later corrective migration.

## Future controlled deployment

Do not execute this procedure without explicit authorization:

1. Create and verify a complete PostgreSQL backup.
2. Verify the approved Git commit and a clean worktree.
3. Build with `Cargo.lock` and an immutable commit identifier.
4. Run unit, SQLx, migration, rollback, and compatibility tests outside production.
5. Apply migration `0011`.
6. Run the synthetic fixture in an isolated or explicitly authorized validation database.
7. Import a limited real normalized file.
8. Compare legacy and new counts and fields without switching readers.
9. Monitor batch/run status, rejects, duration, database growth, and query latency.
10. If needed, stop the new importer and use the non-destructive operational rollback above.

## Known limits

- The pipeline consumes the existing normalized seven-column CSV, not the full original BDIFF ZIP schema.
- Upstream normalization can substitute municipality centroids when precise source coordinates are unavailable.
- Duplicate detection beyond exact source identity belongs to Phase 3B.2.
- Geographic quality remains `precision_undocumented`.
- No ML dataset, negatives, chronological split, label generation, or model change is included.
