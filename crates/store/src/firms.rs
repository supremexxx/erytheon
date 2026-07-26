use chrono::{DateTime, Utc};
use ingest::firms::FirmsRow;
use serde_json::Value;

use super::{Store, StoreError, insert_observation};

const FIRMS_SOURCE_CODE: &str = "nasa_firms";

/// Metadata required before one FIRMS network request starts.
#[derive(Clone, Debug)]
pub struct FirmsImportStart {
    pub batch_type: String,
    pub trigger_type: String,
    pub requested_from: DateTime<Utc>,
    pub requested_to: DateTime<Utc>,
    pub parameters: Value,
    pub pipeline_version: String,
    pub code_version: Option<String>,
}

/// Stable technical identifiers for one FIRMS import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmsImportIds {
    pub batch_id: String,
    pub pipeline_run_id: String,
}

/// Counters produced by the atomic raw and public persistence transaction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FirmsPersistenceResult {
    pub received: usize,
    pub raw_inserted: usize,
    pub public_inserted: usize,
    pub duplicates_ignored: usize,
    pub rejected: usize,
}

/// Final operational state written to both the batch and pipeline run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirmsTerminalState {
    Succeeded,
    PartiallySucceeded,
    Failed,
    Cancelled,
}

impl FirmsTerminalState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::PartiallySucceeded => "partially_succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl Store {
    /// Creates one pending FIRMS batch and run, then marks both running.
    ///
    /// # Errors
    ///
    /// Returns an error when the seeded source is absent or `PostgreSQL` rejects the transaction.
    pub async fn begin_firms_import(
        &self,
        start: &FirmsImportStart,
    ) -> Result<FirmsImportIds, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let source_id = sqlx::query_scalar::<_, String>(
            "SELECT id::text
             FROM reference.data_sources
             WHERE code = $1 AND is_active",
        )
        .bind(FIRMS_SOURCE_CODE)
        .fetch_one(&mut *transaction)
        .await?;
        let batch_id = sqlx::query_scalar::<_, String>(
            "INSERT INTO ops.import_batches (
                id, source_id, batch_type, status, started_at,
                requested_from, requested_to, pipeline_version, parameters
             )
             VALUES (
                gen_random_uuid(), $1::uuid, $2, 'pending', NOW(),
                $3, $4, $5, $6
             )
             RETURNING id::text",
        )
        .bind(source_id)
        .bind(&start.batch_type)
        .bind(start.requested_from)
        .bind(start.requested_to)
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
                gen_random_uuid(), 'firms_ingestion', $1, 'pending', NOW(),
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
        Ok(FirmsImportIds {
            batch_id,
            pipeline_run_id,
        })
    }

    /// Persists raw FIRMS rows and their normalized V1 observations atomically.
    ///
    /// # Errors
    ///
    /// Returns an error and rolls back both targets when either write fails.
    pub async fn persist_firms_import(
        &self,
        ids: &FirmsImportIds,
        rows: &[FirmsRow],
        retrieved_at: DateTime<Utc>,
    ) -> Result<FirmsPersistenceResult, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let mut result = FirmsPersistenceResult {
            received: rows.len(),
            ..FirmsPersistenceResult::default()
        };
        for row in rows {
            let parsing_status = if row.parsing_error.is_some() {
                "rejected"
            } else {
                "parsed"
            };
            let inserted = sqlx::query(
                "INSERT INTO raw.firms_observations (
                    id, import_batch_id, source_record_id, retrieved_at,
                    observed_at, payload, source_version, parsing_status, parsing_error
                 )
                 VALUES (
                    gen_random_uuid(), $1::uuid, $2, $3, $4, $5, $6, $7, $8
                 )
                 ON CONFLICT (import_batch_id, source_record_id)
                    WHERE source_record_id IS NOT NULL
                 DO NOTHING",
            )
            .bind(&ids.batch_id)
            .bind(&row.source_record_id)
            .bind(retrieved_at)
            .bind(row.observed_at)
            .bind(&row.raw_payload)
            .bind(&row.source_version)
            .bind(parsing_status)
            .bind(&row.parsing_error)
            .execute(&mut *transaction)
            .await?
            .rows_affected();
            if inserted == 0 {
                result.duplicates_ignored += 1;
                continue;
            }
            result.raw_inserted += 1;
            if let Some(observation) = &row.observation {
                if insert_observation(&mut transaction, observation).await? == 1 {
                    result.public_inserted += 1;
                } else {
                    result.duplicates_ignored += 1;
                }
            } else {
                result.rejected += 1;
            }
        }
        transaction.commit().await?;
        Ok(result)
    }

    /// Finalizes a FIRMS batch, pipeline run, and V1 source status atomically.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` rejects the final state.
    pub async fn finish_firms_import(
        &self,
        ids: &FirmsImportIds,
        state: FirmsTerminalState,
        result: FirmsPersistenceResult,
        error_message: Option<&str>,
    ) -> Result<(), StoreError> {
        let mut transaction = self.pool.begin().await?;
        let received = count(result.received)?;
        let inserted = count(result.public_inserted)?;
        let ignored = count(result.duplicates_ignored)?;
        let rejected = count(result.rejected)?;
        sqlx::query(
            "UPDATE ops.import_batches
             SET status = $2,
                 finished_at = NOW(),
                 records_received = $3,
                 records_inserted = $4,
                 records_ignored = $5,
                 records_rejected = $6,
                 error_message = $7
             WHERE id = $1::uuid",
        )
        .bind(&ids.batch_id)
        .bind(state.as_str())
        .bind(received)
        .bind(inserted)
        .bind(ignored)
        .bind(rejected)
        .bind(error_message)
        .execute(&mut *transaction)
        .await?;
        let metrics = serde_json::json!({
            "received": result.received,
            "raw_inserted": result.raw_inserted,
            "public_inserted": result.public_inserted,
            "duplicates_ignored": result.duplicates_ignored,
            "rejected": result.rejected,
        });
        sqlx::query(
            "UPDATE ops.pipeline_runs
             SET status = $2,
                 finished_at = NOW(),
                 metrics = $3,
                 error_message = $4
             WHERE id = $1::uuid",
        )
        .bind(&ids.pipeline_run_id)
        .bind(state.as_str())
        .bind(metrics)
        .bind(error_message)
        .execute(&mut *transaction)
        .await?;
        if matches!(
            state,
            FirmsTerminalState::Succeeded | FirmsTerminalState::PartiallySucceeded
        ) {
            sqlx::query(
                "INSERT INTO public.source_status
                    (id, last_run, last_success, observation_count, recent_error)
                 VALUES ('firms', NOW(), NOW(), $1, NULL)
                 ON CONFLICT (id) DO UPDATE SET
                    last_run = NOW(),
                    last_success = NOW(),
                    observation_count = EXCLUDED.observation_count,
                    recent_error = NULL",
            )
            .bind(received - rejected)
            .execute(&mut *transaction)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO public.source_status
                    (id, last_run, last_success, observation_count, recent_error)
                 VALUES ('firms', NOW(), NULL, 0, $1)
                 ON CONFLICT (id) DO UPDATE SET
                    last_run = NOW(),
                    recent_error = EXCLUDED.recent_error",
            )
            .bind(error_message.unwrap_or("FIRMS import failed"))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }
}

fn count(value: usize) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::CountOverflow(value))
}
