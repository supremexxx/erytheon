//! Phase 3B.10 P1: registering a non-v1 model candidate in
//! `ml.model_candidate_registry` (migration 0016) as `candidate` or
//! `inactive` only -- this module never writes to `human_model_versions`
//! and never marks anything `active`.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{Store, StoreError};

/// Everything one registration attempt needs, all explicit -- no field
/// here is ever inferred from "the latest" anything (mission section
/// 10). The caller is responsible for having already verified these
/// values against an audited P0 artifact before calling.
#[derive(Clone, Debug)]
pub struct ModelCandidateRegistration {
    pub model_family: String,
    pub model_name: String,
    pub artifact_version: i32,
    pub status: ModelCandidateStatus,
    pub git_commit: String,
    pub dataset_logical_id: String,
    pub dataset_row_fingerprint: String,
    pub seed: i64,
    pub artifact: Value,
    pub artifact_checksum: String,
    pub metrics: Value,
    pub scientific_interpretation: String,
    pub known_limitations: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelCandidateStatus {
    Candidate,
    Inactive,
}

impl ModelCandidateStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Inactive => "inactive",
        }
    }
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct ModelCandidateRow {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub model_family: String,
    pub model_name: String,
    pub artifact_version: i32,
    pub status: String,
    pub git_commit: String,
    pub dataset_logical_id: String,
    pub dataset_row_fingerprint: String,
    pub seed: i64,
    pub artifact: Value,
    pub artifact_checksum: String,
    pub metrics: Value,
    pub scientific_interpretation: String,
    pub known_limitations: Value,
}

#[derive(Clone, Debug)]
pub enum ModelCandidateRegistrationOutcome {
    /// A new row was inserted.
    Registered(ModelCandidateRow),
    /// A row with the same logical identity (`model_family`,
    /// `model_name`, `dataset_logical_id`, seed) and the same
    /// `artifact_checksum` already existed -- no write occurred
    /// (mission section 12: idempotent replay).
    AlreadyRegistered(ModelCandidateRow),
}

impl Store {
    /// Registers one model candidate as `candidate` or `inactive`,
    /// never `active` (the column's `CHECK` constraint makes `active`
    /// impossible to insert at all). Idempotent on an exact replay of
    /// the same logical identity + checksum; refuses (returns
    /// `StoreError::ModelCandidateChecksumConflict`) if the same
    /// logical identity already exists under a *different*
    /// `artifact_checksum` -- that would mean the artifact silently
    /// changed underneath an unchanged identity, which must be a hard
    /// error, not an overwrite.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails, or when a row with the
    /// same logical identity already exists with a different checksum.
    pub async fn register_model_candidate(
        &self,
        registration: ModelCandidateRegistration,
    ) -> Result<ModelCandidateRegistrationOutcome, StoreError> {
        if let Some(existing) = self
            .model_candidate_by_identity(
                &registration.model_family,
                &registration.model_name,
                &registration.dataset_logical_id,
                registration.seed,
            )
            .await?
        {
            return if existing.artifact_checksum == registration.artifact_checksum {
                Ok(ModelCandidateRegistrationOutcome::AlreadyRegistered(
                    existing,
                ))
            } else {
                Err(StoreError::ModelCandidateChecksumConflict(format!(
                    "candidate ({}, {}, {}, seed {}) already registered as id {} with checksum {}, refusing to register a different checksum {}",
                    registration.model_family,
                    registration.model_name,
                    registration.dataset_logical_id,
                    registration.seed,
                    existing.id,
                    existing.artifact_checksum,
                    registration.artifact_checksum,
                )))
            };
        }

        let known_limitations = serde_json::to_value(&registration.known_limitations)?;
        let row = sqlx::query_as::<_, ModelCandidateRow>(
            "INSERT INTO ml.model_candidate_registry (
                 model_family, model_name, artifact_version, status, git_commit,
                 dataset_logical_id, dataset_row_fingerprint, seed, artifact,
                 artifact_checksum, metrics, scientific_interpretation, known_limitations
             )
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             RETURNING id, created_at, model_family, model_name, artifact_version, status,
                       git_commit, dataset_logical_id, dataset_row_fingerprint, seed, artifact,
                       artifact_checksum, metrics, scientific_interpretation, known_limitations",
        )
        .bind(&registration.model_family)
        .bind(&registration.model_name)
        .bind(registration.artifact_version)
        .bind(registration.status.as_str())
        .bind(&registration.git_commit)
        .bind(&registration.dataset_logical_id)
        .bind(&registration.dataset_row_fingerprint)
        .bind(registration.seed)
        .bind(&registration.artifact)
        .bind(&registration.artifact_checksum)
        .bind(&registration.metrics)
        .bind(&registration.scientific_interpretation)
        .bind(&known_limitations)
        .fetch_one(&self.pool)
        .await?;

        Ok(ModelCandidateRegistrationOutcome::Registered(row))
    }

    /// Reads back one candidate row by its logical identity, for
    /// idempotence checks and post-registration validation. Read-only.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn model_candidate_by_identity(
        &self,
        model_family: &str,
        model_name: &str,
        dataset_logical_id: &str,
        seed: i64,
    ) -> Result<Option<ModelCandidateRow>, StoreError> {
        let row = sqlx::query_as::<_, ModelCandidateRow>(
            "SELECT id, created_at, model_family, model_name, artifact_version, status,
                    git_commit, dataset_logical_id, dataset_row_fingerprint, seed, artifact,
                    artifact_checksum, metrics, scientific_interpretation, known_limitations
             FROM ml.model_candidate_registry
             WHERE model_family = $1 AND model_name = $2
               AND dataset_logical_id = $3 AND seed = $4",
        )
        .bind(model_family)
        .bind(model_name)
        .bind(dataset_logical_id)
        .bind(seed)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Reads back one candidate row strictly by its primary key, inside
    /// a real `PostgreSQL` read-only transaction (`SET TRANSACTION READ
    /// ONLY`) -- not merely "a query that happens not to write
    /// anything", but a transaction in which the server itself refuses
    /// any write statement. Used by phase 3B.11 P2's load-only
    /// verification, which must prove zero database writes occurred.
    ///
    /// # Errors
    ///
    /// Returns an error when the query or transaction fails.
    pub async fn model_candidate_by_id_read_only(
        &self,
        id: i64,
    ) -> Result<Option<ModelCandidateRow>, StoreError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *transaction)
            .await?;
        let row = sqlx::query_as::<_, ModelCandidateRow>(
            "SELECT id, created_at, model_family, model_name, artifact_version, status,
                    git_commit, dataset_logical_id, dataset_row_fingerprint, seed, artifact,
                    artifact_checksum, metrics, scientific_interpretation, known_limitations
             FROM ml.model_candidate_registry
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.rollback().await?;
        Ok(row)
    }

    /// Total row count in `ml.model_candidate_registry`, for
    /// before/after volume reporting (mission section 12).
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn model_candidate_registry_count(&self) -> Result<i64, StoreError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM ml.model_candidate_registry")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }
}
