# ERYTHEON — Phase 3B.1 report

## A. Initial audit

- Branch: `main`.
- Reference commit: `361d46800815d2be8ad49c75932ff42ced64d7a6`.
- Pre-existing untracked files: `PHASE2_PRODUCTION_DEPLOYMENT_REPORT.md` and `PHASE3A_HUMAN_DATASET_SPECIFICATION.md`.
- Existing migrations: `0001` through `0010`.
- Legacy flow: seven-column normalized CSV → `crates/ingest/src/fire_history.rs` → `Observation` → `public.ignition_history`.
- Legacy business identity: `source` plus `dedupe_key`; BDIFF uses `external_id`.
- Legacy schema: `public.ignition_history(id, occurred_at, h3, source, payload, dedupe_key)`.
- Legacy normalization: upstream Python aliases fields, decodes UTF-8/Windows-1252, normalizes timestamps to UTC, parses French numbers, optionally converts m² to hectares, and supplies municipality-centre coordinates when needed.
- Existing lineage pattern: FIRMS uses `reference.data_sources`, `ops.import_batches`, and `ops.pipeline_runs`, with explicit lifecycle and transactions.
- Rust/SQLx conventions: workspace dependencies, modules split across ingest/engine/store, dynamic SQLx queries, explicit PostgreSQL transactions, Clap commands, `thiserror`, and structured `tracing`.
- Existing assets: BDIFF sample, production normalizer and synchronization script, legacy `load-fire-history` command, FIRMS SQLx tests, and H3 helpers.

## B. Architecture

Migration `0011_bdiff_foundation.sql` seeds the idempotent `bdiff` source and creates:

- `raw.bdiff_records`: append-only payload, source line, identity, parsing state, and error.
- `staging.bdiff_events_normalized`: one staging row per inserted raw row, including rejected rows and deterministic error codes.
- `fire.ignition_events`: valid stable business events with original geometry and H3.

Key rules:

- Intra-batch uniqueness: source identity and source line.
- Cross-batch raw history: allowed.
- Business uniqueness: `(source_id, source_record_id)`.
- H3: direct WGS84 projection at configured resolution, tested at resolution 8.
- Geometry: original point only; no spatial correction.
- Cause: six mappings, unknown kept separate, unmapped labels become `indeterminate`.
- Transaction: one atomic raw/staging/fire transaction per decoded file.
- Rejected rows: raw and staging retained, no fire event.
- Indexes: batch, source identity, parsing/validation status, occurred time/date, H3/time, cause category, and PostGIS GiST geometry.

## C. Modifications

- Added migration `0011_bdiff_foundation.sql`.
- Added guarded rollback `rollback/0011_bdiff_foundation.down.sql`.
- Added lossless parser and normalizer `ingest::bdiff`.
- Added transactional persistence `store::bdiff`.
- Added centralized orchestrator `engine::bdiff_pipeline`.
- Added CLI command `import-bdiff` with explicit path and `--dry-run`.
- Added deterministic fixture `testdata/bdiff_pipeline_fixture.csv`.
- Added unit, compatibility, and SQLx integration tests.
- Added `BDIFF_PIPELINE.md`.
- Added direct `unicode-normalization` dependency; `Cargo.lock` updated.

No legacy table, legacy importer, API, UI, FWI code, risk code, model code, or scheduler behavior was changed.

## D. Tests

Executed locally:

- Targeted compilation: passed.
- BDIFF ingest unit tests: 4 passed.
- BDIFF engine test: 1 passed.
- Rust formatting: applied.

Executed through an SSH tunnel against the distinct non-production PostGIS container `erytheon-phase1-test-20260725t223712z`, exposed only on VPS loopback port `55433`:

- Migration `0011`: passed.
- SQLx BDIFF integration test: passed.
- Initial import, intra-batch replay, new-batch replay, rejected row, empty file, transaction failure, batch/run metrics, and source idempotence: passed.
- Rollback with BDIFF batch present: correctly refused.
- Rollback after removing isolated test traces: passed.
- Migration reapplication after rollback: passed.
- Integration test after reapplication: passed.

One initial isolated test exposed that staging coordinate constraints rejected invalid source values. No partial raw/staging/fire rows were committed. The migration was corrected so rejected source values remain auditable while invalid geometry stays null, then the full isolated cycle passed.

## E. Fixture statistics

First import:

- Received: 15.
- Raw inserted: 14.
- Staging valid: 9.
- Staging rejected: 5.
- Business events created: 9.
- Technical duplicates: 1.
- Business events already present: 0.

Same-batch replay:

- Raw inserted: 0.
- Technical duplicates: 15.
- Business events created: 0.

Identical new batch:

- Raw inserted: 14.
- Staging valid: 9.
- Staging rejected: 5.
- Business events created: 0.
- Business events already present: 9.
- Technical duplicates: 1.

Two distinct events on the same date and coordinate remain two business rows.

## F. Compatibility

The existing 94-row Aude fixture was parsed by both the legacy and new paths.

Matching fields:

- source identifier;
- timestamp;
- municipality;
- latitude;
- longitude;
- surface;
- non-empty source cause;
- H3 cell at resolution 8.

Documented divergence:

- Seven legacy rows have an empty cause. The legacy typed parser accepted an empty string. Phase 3B.1 explicitly requires a non-empty cause, so the new pipeline retains those rows in raw/staging and rejects them from fire with `missing_cause`.

No real BDIFF file was imported into the new production tables, so no production count comparison between `public.ignition_history` and `fire.ignition_events` was performed.

## G. Risks

- Technical: a future materially larger export may need chunked transactions and resumable progress.
- Migration: PostGIS must remain installed; rollback is intentionally non-destructive after any BDIFF trace.
- Idempotence: exact source identifiers are trusted; unstable provider identifiers would create separate business events.
- Performance: indexes support expected audit queries, but national real-volume measurements remain pending.
- Scientific: current coordinates may be municipality centroids and geographic precision is undocumented.
- Scientific: exact duplicates, spatial plausibility, label construction, negatives, splits, and model evaluation remain outside Phase 3B.1.

## H. Future deployment

The unexecuted controlled procedure is documented in `BDIFF_PIPELINE.md`. It requires backup verification, approved commit verification, locked build, complete tests outside production, migration, synthetic fixture validation, limited real import, legacy/new comparison, monitoring, and non-destructive operational rollback.

## I. Decision

All requested local and isolated technical foundations are implemented and validated. Production was not accessed for writes, migrated, restarted, or deployed. This technical report was prepared before the separate final Git commit review; no remote push was performed.

PHASE 3B.1 READY FOR REVIEW
