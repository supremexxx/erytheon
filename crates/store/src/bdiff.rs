use chrono::{DateTime, Utc};
use ingest::bdiff::{BdiffRow, NORMALIZER_VERSION, TAXONOMY_VERSION};
use serde_json::Value;

use super::{Store, StoreError};

const BDIFF_SOURCE_CODE: &str = "bdiff";

#[derive(Clone, Debug)]
pub struct BdiffImportStart {
    pub batch_type: String,
    pub trigger_type: String,
    pub parameters: Value,
    pub pipeline_version: String,
    pub code_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BdiffImportIds {
    pub batch_id: String,
    pub pipeline_run_id: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BdiffPersistenceResult {
    pub received: usize,
    pub raw_inserted: usize,
    pub staging_valid: usize,
    pub staging_rejected: usize,
    pub fire_created: usize,
    pub fire_already_present: usize,
    pub technical_duplicates: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BdiffTerminalState {
    Succeeded,
    PartiallySucceeded,
    Failed,
}

impl BdiffTerminalState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::PartiallySucceeded => "partially_succeeded",
            Self::Failed => "failed",
        }
    }
}

impl Store {
    /// Creates one pending BDIFF batch and run, then marks both running.
    ///
    /// # Errors
    ///
    /// Returns an error when the BDIFF source is absent or `PostgreSQL` rejects the transaction.
    pub async fn begin_bdiff_import(
        &self,
        start: &BdiffImportStart,
    ) -> Result<BdiffImportIds, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let source_id = sqlx::query_scalar::<_, String>(
            "SELECT id::text FROM reference.data_sources WHERE code = $1 AND is_active",
        )
        .bind(BDIFF_SOURCE_CODE)
        .fetch_one(&mut *transaction)
        .await?;
        let batch_id = sqlx::query_scalar::<_, String>(
            "INSERT INTO ops.import_batches (
                id, source_id, batch_type, status, started_at, pipeline_version, parameters
             )
             VALUES (gen_random_uuid(), $1::uuid, $2, 'pending', NOW(), $3, $4)
             RETURNING id::text",
        )
        .bind(source_id)
        .bind(&start.batch_type)
        .bind(&start.pipeline_version)
        .bind(&start.parameters)
        .fetch_one(&mut *transaction)
        .await?;
        let pipeline_run_id = sqlx::query_scalar::<_, String>(
            "INSERT INTO ops.pipeline_runs (
                id, pipeline_name, pipeline_version, status, started_at,
                trigger_type, import_batch_id, parameters, code_version
             )
             VALUES (
                gen_random_uuid(), 'bdiff_ingestion', $1, 'pending', NOW(),
                $2, $3::uuid, $4, $5
             )
             RETURNING id::text",
        )
        .bind(&start.pipeline_version)
        .bind(&start.trigger_type)
        .bind(&batch_id)
        .bind(&start.parameters)
        .bind(&start.code_version)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query("UPDATE ops.import_batches SET status = 'running' WHERE id = $1::uuid")
            .bind(&batch_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE ops.pipeline_runs SET status = 'running' WHERE id = $1::uuid")
            .bind(&pipeline_run_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(BdiffImportIds {
            batch_id,
            pipeline_run_id,
        })
    }

    /// Persists one decoded BDIFF file atomically across raw, staging, and fire.
    ///
    /// # Errors
    ///
    /// Returns an error and rolls back all three layers when any write fails.
    #[allow(clippy::too_many_lines)]
    pub async fn persist_bdiff_import(
        &self,
        ids: &BdiffImportIds,
        rows: &[BdiffRow],
        retrieved_at: DateTime<Utc>,
        h3_resolution: u8,
    ) -> Result<BdiffPersistenceResult, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let source_id = sqlx::query_scalar::<_, String>(
            "SELECT source_id::text FROM ops.import_batches WHERE id = $1::uuid",
        )
        .bind(&ids.batch_id)
        .fetch_one(&mut *transaction)
        .await?;
        let mut result = BdiffPersistenceResult {
            received: rows.len(),
            ..BdiffPersistenceResult::default()
        };

        for row in rows {
            let valid = row.normalized.is_valid();
            let raw_id = sqlx::query_scalar::<_, String>(
                "INSERT INTO raw.bdiff_records (
                    id, import_batch_id, source_record_id, source_line_number,
                    payload, payload_format, retrieved_at, parsing_status, parsing_error
                 )
                 VALUES (
                    gen_random_uuid(), $1::uuid, $2, $3, $4, 'csv', $5, $6, $7
                 )
                 ON CONFLICT DO NOTHING
                 RETURNING id::text",
            )
            .bind(&ids.batch_id)
            .bind(&row.source_record_id)
            .bind(source_line_number(row.source_line_number)?)
            .bind(&row.raw_payload)
            .bind(retrieved_at)
            .bind(if valid { "parsed" } else { "rejected" })
            .bind(row.normalized.parsing_error())
            .fetch_optional(&mut *transaction)
            .await?;
            let Some(raw_id) = raw_id else {
                result.technical_duplicates += 1;
                continue;
            };
            result.raw_inserted += 1;

            let validation_errors = serde_json::json!(row.normalized.validation_errors);
            let staging_id = sqlx::query_scalar::<_, String>(
                "INSERT INTO staging.bdiff_events_normalized (
                    id, raw_record_id, source_record_id, occurred_at,
                    municipality_source, latitude, longitude, geom_original,
                    surface_ha, cause_source, cause_category, cause_subcategory,
                    taxonomy_version, normalizer_version, validation_status,
                    validation_errors
                 )
                 VALUES (
                    gen_random_uuid(), $1::uuid, $2, $3, $4, $5, $6,
                    CASE
                        WHEN $5::double precision IS NULL
                          OR $6::double precision IS NULL
                          OR $5 NOT BETWEEN -90 AND 90
                          OR $6 NOT BETWEEN -180 AND 180
                        THEN NULL
                        ELSE ST_SetSRID(ST_MakePoint($6, $5), 4326)
                    END,
                    $7, $8, $9, $10, $11, $12, $13, $14
                 )
                 RETURNING id::text",
            )
            .bind(&raw_id)
            .bind(&row.source_record_id)
            .bind(row.normalized.occurred_at)
            .bind(&row.normalized.municipality_source)
            .bind(row.normalized.latitude)
            .bind(row.normalized.longitude)
            .bind(row.normalized.surface_ha)
            .bind(&row.normalized.cause_source)
            .bind(row.normalized.cause_category)
            .bind(row.normalized.cause_subcategory)
            .bind(TAXONOMY_VERSION)
            .bind(NORMALIZER_VERSION)
            .bind(if valid { "valid" } else { "rejected" })
            .bind(validation_errors)
            .fetch_one(&mut *transaction)
            .await?;

            if !valid {
                result.staging_rejected += 1;
                continue;
            }
            result.staging_valid += 1;
            let cell = row
                .normalized
                .cell
                .ok_or(StoreError::InvalidBdiffNormalizedRow)?;
            let inserted = sqlx::query(
                "INSERT INTO fire.ignition_events (
                    id, source_id, source_record_id, staging_event_id,
                    occurred_at, occurred_on_local, municipality_source,
                    latitude_original, longitude_original, geom_original,
                    h3, h3_resolution, surface_ha, cause_source,
                    cause_category, cause_subcategory, taxonomy_version
                 )
                 VALUES (
                    gen_random_uuid(), $1::uuid, $2, $3::uuid, $4,
                    ($4 AT TIME ZONE 'Europe/Paris')::date, $5, $6, $7,
                    ST_SetSRID(ST_MakePoint($7, $6), 4326),
                    $8, $9, $10, $11, $12, $13, $14
                 )
                 ON CONFLICT (source_id, source_record_id) DO NOTHING",
            )
            .bind(&source_id)
            .bind(row.source_record_id.as_deref())
            .bind(&staging_id)
            .bind(row.normalized.occurred_at)
            .bind(&row.normalized.municipality_source)
            .bind(row.normalized.latitude)
            .bind(row.normalized.longitude)
            .bind(grid::cell_to_db(cell))
            .bind(i16::from(h3_resolution))
            .bind(row.normalized.surface_ha)
            .bind(&row.normalized.cause_source)
            .bind(row.normalized.cause_category)
            .bind(row.normalized.cause_subcategory)
            .bind(TAXONOMY_VERSION)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            if inserted == 1 {
                result.fire_created += 1;
            } else {
                result.fire_already_present += 1;
            }
        }
        transaction.commit().await?;
        Ok(result)
    }

    /// Finalizes one BDIFF import batch and pipeline run atomically.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` rejects the final state.
    pub async fn finish_bdiff_import(
        &self,
        ids: &BdiffImportIds,
        state: BdiffTerminalState,
        result: BdiffPersistenceResult,
        error_message: Option<&str>,
    ) -> Result<(), StoreError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "UPDATE ops.import_batches
             SET status = $2, finished_at = NOW(), records_received = $3,
                 records_inserted = $4, records_ignored = $5,
                 records_rejected = $6, error_message = $7
             WHERE id = $1::uuid",
        )
        .bind(&ids.batch_id)
        .bind(state.as_str())
        .bind(count(result.received)?)
        .bind(count(result.fire_created)?)
        .bind(count(
            result.technical_duplicates + result.fire_already_present,
        )?)
        .bind(count(result.staging_rejected)?)
        .bind(error_message)
        .execute(&mut *transaction)
        .await?;
        let metrics = serde_json::json!({
            "received": result.received,
            "raw_inserted": result.raw_inserted,
            "staging_valid": result.staging_valid,
            "staging_rejected": result.staging_rejected,
            "fire_created": result.fire_created,
            "fire_already_present": result.fire_already_present,
            "technical_duplicates": result.technical_duplicates,
        });
        sqlx::query(
            "UPDATE ops.pipeline_runs
             SET status = $2, finished_at = NOW(), metrics = $3, error_message = $4
             WHERE id = $1::uuid",
        )
        .bind(&ids.pipeline_run_id)
        .bind(state.as_str())
        .bind(metrics)
        .bind(error_message)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}

fn count(value: usize) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::CountOverflow(value))
}

fn source_line_number(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::CountOverflow(usize::MAX))
}
