//! Phase 4A: read-only queries backing the scientific console. Every
//! query here is a plain `SELECT` (or a read-only aggregate) -- this
//! module never inserts, updates, or deletes anything, never triggers
//! a migration, never loads or scores the model candidate, and never
//! touches `human_model_versions`/`ml.model_candidate_registry` beyond
//! reading them.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::{ModelCandidateRow, Store, StoreError};

#[derive(Clone, Debug, Serialize)]
pub struct ScienceOverview {
    pub app_status: &'static str,
    pub db_status: &'static str,
    pub migrations_applied: i64,
    pub active_model_id: Option<i64>,
    pub active_model_trained_at: Option<DateTime<Utc>>,
    pub candidate_id: Option<i64>,
    pub candidate_status: Option<String>,
    pub candidate_model_family: Option<String>,
    pub candidate_model_name: Option<String>,
    pub bdiff_events_total: i64,
    pub bdiff_human_known: i64,
    pub bdiff_natural_known: i64,
    pub bdiff_unknown: i64,
    pub bdiff_indeterminate: i64,
    pub firms_observations_total: i64,
    pub cell_static_total: i64,
    pub feature_snapshots_total: i64,
    pub dataset_versions_total: i64,
    pub dataset_builds_total: i64,
    pub human_model_versions_total: i64,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct SourceOverviewRow {
    pub id: String,
    pub last_run: DateTime<Utc>,
    pub last_success: Option<DateTime<Utc>>,
    pub observation_count: i64,
    pub recent_error: Option<String>,
    pub category: Option<String>,
    pub provider: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct ImportBatchRow {
    pub id: String,
    pub source_code: Option<String>,
    pub batch_type: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub records_received: i64,
    pub records_inserted: i64,
    pub records_rejected: i64,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct PipelineRunRow {
    pub id: String,
    pub pipeline_name: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub import_batch_id: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CategoryCount {
    pub category: String,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DataQualitySummary {
    pub bdiff_events_total: i64,
    pub cause_counts: Vec<CategoryCount>,
    pub duplicate_classification_counts: Vec<CategoryCount>,
    pub geographic_quality_counts: Vec<CategoryCount>,
    pub combustibility_counts: Vec<CategoryCount>,
    pub coordinate_groups_total: i64,
    pub duplicate_candidate_pairs_total: i64,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct IgnitionEventExplorationRow {
    pub id: String,
    pub occurred_on_local: NaiveDate,
    pub h3: i64,
    pub cause_category: String,
    pub cause_subcategory: String,
    pub geographic_quality: String,
    pub is_active: bool,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct FeatureSnapshotRow {
    pub id: String,
    pub family: String,
    pub source: String,
    pub status: String,
    pub temporal_classification: String,
    pub vintage: Option<String>,
    pub available_from: DateTime<Utc>,
    pub cell_count: i64,
    pub h3_resolution: i16,
    pub logical_checksum: String,
    pub limitations: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct CalendarSummary {
    pub total_days: i64,
    pub min_date: Option<NaiveDate>,
    pub max_date: Option<NaiveDate>,
    pub public_holiday_days: i64,
    pub school_holiday_known_days: i64,
    pub school_holiday_unknown_days: i64,
    pub active_rule_checksum: Option<String>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct DatasetVersionSummaryRow {
    pub id: String,
    pub logical_id: String,
    pub name: String,
    pub variant: String,
    pub status: String,
    pub seed: i64,
    pub checksum: Option<String>,
    pub row_count: Option<i64>,
    pub positive_count: Option<i64>,
    pub negative_count: Option<i64>,
    pub exclusion_count: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub finalized_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DatasetSplitCount {
    pub split: String,
    pub label: i16,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DatasetExclusionCount {
    pub reason_category: String,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DatasetDetail {
    pub summary: DatasetVersionSummaryRow,
    pub splits: Vec<DatasetSplitCount>,
    pub exclusions: Vec<DatasetExclusionCount>,
    pub build_count: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SystemSummary {
    pub migrations_applied: i64,
    pub migrations_failed: i64,
    pub active_model_count: i64,
    pub candidate_registry_count: i64,
    pub cell_static_total: i64,
    pub ignition_events_total: i64,
    pub dataset_versions_total: i64,
    pub last_firms_success: Option<DateTime<Utc>>,
    pub last_bdiff_success: Option<DateTime<Utc>>,
}

impl Store {
    /// Phase 4A overview counts. Every value is a live read; nothing
    /// here is hardcoded.
    ///
    /// # Errors
    ///
    /// Returns an error when any underlying query fails.
    pub async fn science_overview(&self) -> Result<ScienceOverview, StoreError> {
        self.health_check().await?;

        let migrations_applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success")
                .fetch_one(&self.pool)
                .await?;

        let active_model: Option<(i64, DateTime<Utc>)> =
            sqlx::query_as("SELECT id, trained_at FROM human_model_versions WHERE active LIMIT 1")
                .fetch_optional(&self.pool)
                .await?;

        let candidate: Option<(i64, String, String, String)> = sqlx::query_as(
            "SELECT id, status, model_family, model_name
             FROM ml.model_candidate_registry
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        let cause_counts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT cause_category, COUNT(*) FROM fire.ignition_events WHERE is_active GROUP BY cause_category",
        )
        .fetch_all(&self.pool)
        .await?;
        let cause = |name: &str| {
            cause_counts
                .iter()
                .find(|(c, _)| c == name)
                .map_or(0, |(_, n)| *n)
        };

        let bdiff_events_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM fire.ignition_events WHERE is_active")
                .fetch_one(&self.pool)
                .await?;
        let firms_observations_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM raw.firms_observations")
                .fetch_one(&self.pool)
                .await?;
        let cell_static_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cell_static")
            .fetch_one(&self.pool)
            .await?;
        let feature_snapshots_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM features.feature_snapshots")
                .fetch_one(&self.pool)
                .await?;
        let dataset_versions_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ml.dataset_versions")
                .fetch_one(&self.pool)
                .await?;
        let dataset_builds_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ml.dataset_builds")
                .fetch_one(&self.pool)
                .await?;
        let human_model_versions_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM human_model_versions")
                .fetch_one(&self.pool)
                .await?;

        Ok(ScienceOverview {
            app_status: "ok",
            db_status: "ok",
            migrations_applied,
            active_model_id: active_model.as_ref().map(|(id, _)| *id),
            active_model_trained_at: active_model.map(|(_, trained_at)| trained_at),
            candidate_id: candidate.as_ref().map(|(id, ..)| *id),
            candidate_status: candidate.as_ref().map(|(_, status, ..)| status.clone()),
            candidate_model_family: candidate.as_ref().map(|(_, _, family, _)| family.clone()),
            candidate_model_name: candidate.map(|(_, _, _, name)| name),
            bdiff_events_total,
            bdiff_human_known: cause("human_known"),
            bdiff_natural_known: cause("natural_known"),
            bdiff_unknown: cause("unknown"),
            bdiff_indeterminate: cause("indeterminate"),
            firms_observations_total,
            cell_static_total,
            feature_snapshots_total,
            dataset_versions_total,
            dataset_builds_total,
            human_model_versions_total,
        })
    }

    /// Live source health joined with static source metadata. Reads
    /// `public.source_status` (the table `record_source_success`/
    /// `record_source_error` actually write) left-joined to
    /// `reference.data_sources` on `code = id` for category/provider/
    /// description; sources with no metadata row still appear.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn science_sources(&self) -> Result<Vec<SourceOverviewRow>, StoreError> {
        let rows = sqlx::query_as::<_, SourceOverviewRow>(
            "SELECT s.id, s.last_run, s.last_success, s.observation_count, s.recent_error,
                    d.category, d.provider, d.description
             FROM public.source_status s
             LEFT JOIN reference.data_sources d ON d.code = s.id
             ORDER BY s.id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Paginated import batches, most recent first, optionally
    /// filtered by source code and/or status.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn science_import_batches(
        &self,
        source_code: Option<&str>,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ImportBatchRow>, StoreError> {
        let rows = sqlx::query_as::<_, ImportBatchRow>(
            "SELECT b.id::text, d.code AS source_code, b.batch_type, b.status, b.started_at, b.finished_at,
                    b.records_received, b.records_inserted, b.records_rejected, b.error_message
             FROM ops.import_batches b
             LEFT JOIN reference.data_sources d ON d.id = b.source_id
             WHERE ($1::text IS NULL OR d.code = $1)
               AND ($2::text IS NULL OR b.status = $2)
             ORDER BY b.started_at DESC
             LIMIT $3 OFFSET $4",
        )
        .bind(source_code)
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Paginated pipeline runs, most recent first, optionally filtered
    /// by pipeline name and/or status.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn science_pipeline_runs(
        &self,
        pipeline_name: Option<&str>,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PipelineRunRow>, StoreError> {
        let rows = sqlx::query_as::<_, PipelineRunRow>(
            "SELECT id::text, pipeline_name, status, started_at, finished_at, import_batch_id::text, error_message
             FROM ops.pipeline_runs
             WHERE ($1::text IS NULL OR pipeline_name = $1)
               AND ($2::text IS NULL OR status = $2)
             ORDER BY started_at DESC
             LIMIT $3 OFFSET $4",
        )
        .bind(pipeline_name)
        .bind(status)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Data-quality summary counts, all live.
    ///
    /// # Errors
    ///
    /// Returns an error when any underlying query fails.
    pub async fn science_data_quality_summary(&self) -> Result<DataQualitySummary, StoreError> {
        let bdiff_events_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM fire.ignition_events WHERE is_active")
                .fetch_one(&self.pool)
                .await?;

        let cause_counts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT cause_category, COUNT(*) FROM fire.ignition_events WHERE is_active GROUP BY cause_category ORDER BY 1",
        )
        .fetch_all(&self.pool)
        .await?;

        let duplicate_counts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT classification, COUNT(*) FROM validation.duplicate_candidate_groups GROUP BY classification ORDER BY 1",
        )
        .fetch_all(&self.pool)
        .await?;

        let geo_counts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT geographic_category, COUNT(*) FROM validation.event_geographic_quality GROUP BY geographic_category ORDER BY 1",
        )
        .fetch_all(&self.pool)
        .await?;

        let combustibility_counts: Vec<(Option<bool>, i64)> = sqlx::query_as(
            "SELECT original_cell_combustible, COUNT(*) FROM validation.event_combustibility_assessments GROUP BY original_cell_combustible",
        )
        .fetch_all(&self.pool)
        .await?;
        let combustibility_counts = combustibility_counts
            .into_iter()
            .map(|(value, count)| CategoryCount {
                category: match value {
                    Some(true) => "combustible".to_owned(),
                    Some(false) => "non_combustible".to_owned(),
                    None => "features_missing".to_owned(),
                },
                count,
            })
            .collect();

        let coordinate_groups_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM validation.coordinate_groups")
                .fetch_one(&self.pool)
                .await?;
        let duplicate_candidate_pairs_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM validation.duplicate_candidate_pairs")
                .fetch_one(&self.pool)
                .await?;

        Ok(DataQualitySummary {
            bdiff_events_total,
            cause_counts: cause_counts
                .into_iter()
                .map(|(category, count)| CategoryCount { category, count })
                .collect(),
            duplicate_classification_counts: duplicate_counts
                .into_iter()
                .map(|(category, count)| CategoryCount { category, count })
                .collect(),
            geographic_quality_counts: geo_counts
                .into_iter()
                .map(|(category, count)| CategoryCount { category, count })
                .collect(),
            combustibility_counts,
            coordinate_groups_total,
            duplicate_candidate_pairs_total,
        })
    }

    /// Paginated, filterable ignition-event exploration table.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn science_ignition_events(
        &self,
        cause_category: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<IgnitionEventExplorationRow>, StoreError> {
        let rows = sqlx::query_as::<_, IgnitionEventExplorationRow>(
            "SELECT id::text, occurred_on_local, h3, cause_category, cause_subcategory, geographic_quality, is_active
             FROM fire.ignition_events
             WHERE is_active AND ($1::text IS NULL OR cause_category = $1)
             ORDER BY occurred_on_local DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(cause_category)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// All registered feature snapshots.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn science_feature_snapshots(&self) -> Result<Vec<FeatureSnapshotRow>, StoreError> {
        let rows = sqlx::query_as::<_, FeatureSnapshotRow>(
            "SELECT id::text, family, source, status, temporal_classification, vintage, available_from,
                    cell_count, h3_resolution, logical_checksum, limitations
             FROM features.feature_snapshots
             ORDER BY family, available_from DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Historical calendar summary for the active rule version, if any.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn science_calendar_summary(&self) -> Result<CalendarSummary, StoreError> {
        let active_checksum: Option<String> = sqlx::query_scalar(
            "SELECT checksum FROM features.calendar_rule_versions WHERE status = 'active' LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        let (total_days, min_date, max_date): (i64, Option<NaiveDate>, Option<NaiveDate>) =
            sqlx::query_as(
                "SELECT COUNT(*), MIN(date), MAX(date) FROM features.historical_calendar_days",
            )
            .fetch_one(&self.pool)
            .await?;
        let public_holiday_days: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM features.historical_calendar_days WHERE public_holiday",
        )
        .fetch_one(&self.pool)
        .await?;
        let school_holiday_known_days: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM features.historical_calendar_days WHERE school_holiday IS NOT NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        let school_holiday_unknown_days: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM features.historical_calendar_days WHERE school_holiday IS NULL",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(CalendarSummary {
            total_days,
            min_date,
            max_date,
            public_holiday_days,
            school_holiday_known_days,
            school_holiday_unknown_days,
            active_rule_checksum: active_checksum,
        })
    }

    /// Summary rows for every dataset version, most recent first.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn science_dataset_versions(
        &self,
    ) -> Result<Vec<DatasetVersionSummaryRow>, StoreError> {
        let rows = sqlx::query_as::<_, DatasetVersionSummaryRow>(
            "SELECT id::text, logical_id, name, variant, status, seed, checksum,
                    row_count, positive_count, negative_count, exclusion_count,
                    created_at, finalized_at
             FROM ml.dataset_versions
             ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Full detail for one dataset version by its `logical_id`.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails, or `Ok(None)` if no such
    /// dataset exists.
    pub async fn science_dataset_detail(
        &self,
        logical_id: &str,
    ) -> Result<Option<DatasetDetail>, StoreError> {
        let Some(summary) = sqlx::query_as::<_, DatasetVersionSummaryRow>(
            "SELECT id::text, logical_id, name, variant, status, seed, checksum,
                    row_count, positive_count, negative_count, exclusion_count,
                    created_at, finalized_at
             FROM ml.dataset_versions
             WHERE logical_id = $1",
        )
        .bind(logical_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let split_rows: Vec<(String, i16, i64)> = sqlx::query_as(
            "SELECT split, label, COUNT(*) FROM ml.dataset_rows
             WHERE dataset_version_id = $1::uuid
             GROUP BY split, label
             ORDER BY split, label",
        )
        .bind(&summary.id)
        .fetch_all(&self.pool)
        .await?;
        let splits = split_rows
            .into_iter()
            .map(|(split, label, count)| DatasetSplitCount {
                split,
                label,
                count,
            })
            .collect();

        let exclusion_rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT reason_category, COUNT(*) FROM ml.dataset_exclusions
             WHERE dataset_version_id = $1::uuid
             GROUP BY reason_category
             ORDER BY reason_category",
        )
        .bind(&summary.id)
        .fetch_all(&self.pool)
        .await?;
        let exclusions = exclusion_rows
            .into_iter()
            .map(|(reason_category, count)| DatasetExclusionCount {
                reason_category,
                count,
            })
            .collect();

        let build_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ml.dataset_builds WHERE dataset_version_id = $1::uuid",
        )
        .bind(&summary.id)
        .fetch_one(&self.pool)
        .await?;

        Ok(Some(DatasetDetail {
            summary,
            splits,
            exclusions,
            build_count,
        }))
    }

    /// System/integrity summary for the `/science/system` page.
    ///
    /// # Errors
    ///
    /// Returns an error when any underlying query fails.
    pub async fn science_system_summary(&self) -> Result<SystemSummary, StoreError> {
        let migrations_applied: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success")
                .fetch_one(&self.pool)
                .await?;
        let migrations_failed: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE NOT success")
                .fetch_one(&self.pool)
                .await?;
        let active_model_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM human_model_versions WHERE active")
                .fetch_one(&self.pool)
                .await?;
        let candidate_registry_count = self.model_candidate_registry_count().await?;
        let cell_static_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cell_static")
            .fetch_one(&self.pool)
            .await?;
        let ignition_events_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM fire.ignition_events WHERE is_active")
                .fetch_one(&self.pool)
                .await?;
        let dataset_versions_total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ml.dataset_versions")
                .fetch_one(&self.pool)
                .await?;
        let last_firms_success: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT last_success FROM public.source_status WHERE id LIKE '%firms%' ORDER BY last_success DESC NULLS LAST LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .flatten();
        let last_bdiff_success: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT last_success FROM public.source_status WHERE id LIKE '%bdiff%' ORDER BY last_success DESC NULLS LAST LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .flatten();

        Ok(SystemSummary {
            migrations_applied,
            migrations_failed,
            active_model_count,
            candidate_registry_count,
            cell_static_total,
            ignition_events_total,
            dataset_versions_total,
            last_firms_success,
            last_bdiff_success,
        })
    }

    /// The most recently registered model candidate, if any. Read-only;
    /// never loads it into a scoring path.
    ///
    /// # Errors
    ///
    /// Returns an error when the query fails.
    pub async fn science_latest_model_candidate(
        &self,
    ) -> Result<Option<ModelCandidateRow>, StoreError> {
        let row = sqlx::query_as::<_, ModelCandidateRow>(
            "SELECT id, created_at, model_family, model_name, artifact_version, status,
                    git_commit, dataset_logical_id, dataset_row_fingerprint, seed, artifact,
                    artifact_checksum, metrics, scientific_interpretation, known_limitations
             FROM ml.model_candidate_registry
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }
}
