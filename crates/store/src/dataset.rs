use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::{Store, StoreError};

/// One `human_known` BDIFF event with its quality assessments joined in,
/// candidate for the pilot dataset. Read-only: never mutates `fire`,
/// `validation`, or any source table.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct HumanDatasetCandidateEvent {
    pub id: String,
    pub h3: i64,
    pub h3_resolution: i16,
    pub occurred_on_local: NaiveDate,
    pub cause_category: String,
    pub cause_subcategory: String,
    pub label_confidence: String,
    pub requires_accidental_sensitivity_analysis: bool,
    pub geographic_category: String,
    pub original_cell_combustible: Option<bool>,
    pub cell_features_present: bool,
    pub certain_duplicate_non_anchor: bool,
}

/// One ignition event of any cause, for the phase 3B.4 negative-sampling
/// exclusion-window experiment only.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct AnyCauseEventForNegativeDesign {
    pub h3: i64,
    pub occurred_on_local: NaiveDate,
    pub cause_category: String,
    pub geographic_category: String,
}

#[derive(Clone, Debug)]
pub struct FeatureSnapshotSpec {
    pub family: String,
    pub source: String,
    pub provider: Option<String>,
    pub vintage: Option<String>,
    pub valid_from: Option<NaiveDate>,
    pub valid_until: Option<NaiveDate>,
    pub available_from: DateTime<Utc>,
    pub available_until: Option<DateTime<Utc>>,
    pub retrieved_at: Option<DateTime<Utc>>,
    pub code_version: String,
    pub normalizer_version: String,
    pub parameters: Value,
    pub source_checksum: Option<String>,
    pub logical_checksum: String,
    pub reference_table: String,
    pub cell_count: i64,
    pub h3_resolution: i16,
    pub geographic_coverage: Option<Value>,
    pub temporal_classification: String,
    pub limitations: Value,
    pub license_attribution: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CalendarRuleVersion {
    pub logical_id: String,
    pub rule_type: String,
    pub description: String,
    pub parameters: Value,
    pub code_version: String,
    pub status: String,
    pub checksum: String,
    pub notes: Option<String>,
}

// Each flag is an independent, well-named calendar fact, not a state
// machine over mutually exclusive states.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug)]
pub struct HistoricalCalendarDayRecord {
    pub date: NaiveDate,
    pub school_zone: String,
    pub year: i16,
    pub month: i16,
    pub day_of_week: i16,
    pub is_weekend: bool,
    pub public_holiday: bool,
    pub public_holiday_label: Option<String>,
    pub school_holiday: Option<bool>,
    pub school_holiday_label: Option<String>,
    pub is_day_before_public_holiday: bool,
    pub is_day_after_public_holiday: bool,
    pub season: i16,
    pub season_sine: f64,
    pub season_cosine: f64,
    pub available_from: DateTime<Utc>,
    pub source: String,
    pub temporal_classification: String,
    pub logical_checksum: String,
}

#[derive(Clone, Debug)]
pub struct DatasetVersionSpec {
    pub logical_id: String,
    pub name: String,
    pub description: String,
    pub observation_unit: String,
    pub h3_resolution: i16,
    pub timezone: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub variant: String,
    pub code_version: String,
    pub migrations: Value,
    pub quality_rule_versions: Value,
    pub feature_snapshot_ids: Value,
    pub calendar_rule_version_id: Option<String>,
    pub inclusion_rules: Value,
    pub exclusion_rules: Value,
    pub negative_strategy: String,
    pub negative_parameters: Value,
    pub seed: i64,
    pub splits: Value,
    pub author_or_pipeline: String,
    pub notes: Option<String>,
}

/// The subset of an existing `ml.dataset_versions` row that defines the
/// dataset (excludes name/description/notes/code_version/author, which may
/// legitimately vary across a rebuild without it being a different dataset).
#[derive(Clone, Debug, sqlx::FromRow)]
struct ExistingDatasetVersionDefinition {
    id: String,
    status: String,
    variant: String,
    h3_resolution: i16,
    timezone: String,
    period_start: NaiveDate,
    period_end: NaiveDate,
    seed: i64,
    negative_strategy: String,
    migrations: Value,
    quality_rule_versions: Value,
    feature_snapshot_ids: Value,
    calendar_rule_version_id: Option<String>,
    inclusion_rules: Value,
    exclusion_rules: Value,
    negative_parameters: Value,
    splits: Value,
}

impl ExistingDatasetVersionDefinition {
    fn matches(&self, spec: &DatasetVersionSpec) -> bool {
        self.variant == spec.variant
            && self.h3_resolution == spec.h3_resolution
            && self.timezone == spec.timezone
            && self.period_start == spec.period_start
            && self.period_end == spec.period_end
            && self.seed == spec.seed
            && self.negative_strategy == spec.negative_strategy
            && self.migrations == spec.migrations
            && self.quality_rule_versions == spec.quality_rule_versions
            && self.feature_snapshot_ids == spec.feature_snapshot_ids
            && self.calendar_rule_version_id == spec.calendar_rule_version_id
            && self.inclusion_rules == spec.inclusion_rules
            && self.exclusion_rules == spec.exclusion_rules
            && self.negative_parameters == spec.negative_parameters
            && self.splits == spec.splits
    }
}

#[derive(Clone, Debug)]
pub struct DatasetRowRecord {
    pub h3: i64,
    pub h3_resolution: i16,
    pub local_date: NaiveDate,
    pub reference_timestamp: DateTime<Utc>,
    pub split: String,
    pub label: i16,
    pub row_category: String,
    pub selection_method: String,
    pub weight: Option<f64>,
    pub quality_flags: Value,
    pub features: Value,
    pub temporal_availability: Value,
    pub deterministic_key: String,
    pub row_checksum: String,
    pub justification: String,
    pub snapshot_ids: Vec<String>,
    pub event_links: Vec<DatasetEventLinkRecord>,
}

#[derive(Clone, Debug)]
pub struct DatasetEventLinkRecord {
    pub ignition_event_id: String,
    pub role: String,
    pub label_quality_confidence: Option<String>,
    pub geographic_quality_category: Option<String>,
    pub duplicate_group_id: Option<String>,
    pub inclusion_rule: String,
    pub justification: String,
}

#[derive(Clone, Debug)]
pub struct DatasetExclusionRecord {
    pub ignition_event_id: Option<String>,
    pub h3: Option<i64>,
    pub local_date: Option<NaiveDate>,
    pub reason_category: String,
    pub rule: String,
    pub rule_version: Option<String>,
    pub details: Value,
    pub reintegration_possible: bool,
}

#[derive(Clone, Debug)]
pub struct DatasetBuildCounts {
    pub row_count: i64,
    pub positive_count: i64,
    pub negative_count: i64,
    pub exclusion_count: i64,
}

impl Store {
    /// Loads `human_known` BDIFF events strictly within `[period_start,
    /// period_end]` (local date) with their quality assessments joined in.
    /// Read-only; never mutates `fire` or `validation`.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the read.
    pub async fn human_dataset_candidate_events(
        &self,
        period_start: NaiveDate,
        period_end: NaiveDate,
    ) -> Result<Vec<HumanDatasetCandidateEvent>, StoreError> {
        sqlx::query_as::<_, HumanDatasetCandidateEvent>(
            "SELECT
                event.id::text AS id, event.h3, event.h3_resolution,
                event.occurred_on_local, event.cause_category, event.cause_subcategory,
                label.confidence AS label_confidence,
                label.requires_accidental_sensitivity_analysis,
                geo.geographic_category,
                combustibility.original_cell_combustible,
                combustibility.cell_features_present,
                EXISTS (
                    SELECT 1 FROM validation.duplicate_candidate_members AS member
                    JOIN validation.duplicate_candidate_groups AS grp ON grp.id = member.group_id
                    WHERE member.ignition_event_id = event.id
                      AND member.role = 'candidate'
                      AND grp.classification = 'certain_duplicate'
                ) AS certain_duplicate_non_anchor
             FROM fire.ignition_events AS event
             JOIN validation.event_label_quality AS label ON label.ignition_event_id = event.id
             JOIN validation.event_geographic_quality AS geo ON geo.ignition_event_id = event.id
             LEFT JOIN validation.event_combustibility_assessments AS combustibility
                ON combustibility.ignition_event_id = event.id
             WHERE event.cause_category = 'human_known'
               AND event.occurred_on_local BETWEEN $1 AND $2
             ORDER BY event.occurred_on_local, event.id",
        )
        .bind(period_start)
        .bind(period_end)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// All `(h3, local_date)` pairs carrying any known fire event (any
    /// cause) in the period, used to keep pilot negatives away from a
    /// known event of any kind, not only `human_known`.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the read.
    pub async fn known_event_cell_dates(
        &self,
        period_start: NaiveDate,
        period_end: NaiveDate,
    ) -> Result<Vec<(i64, NaiveDate)>, StoreError> {
        sqlx::query_as::<_, (i64, NaiveDate)>(
            "SELECT DISTINCT h3, occurred_on_local FROM fire.ignition_events
             WHERE occurred_on_local BETWEEN $1 AND $2",
        )
        .bind(period_start)
        .bind(period_end)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// Every ignition event of any cause with its geographic-quality
    /// category, for the phase 3B.4 negative-sampling exclusion-window
    /// experiment only (see `dataset::negative_design` and
    /// `NEGATIVE_SAMPLING_DESIGN.md`). Read-only; not used by
    /// `build_human_dataset`.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the read.
    pub async fn all_events_with_geographic_quality(
        &self,
        period_start: NaiveDate,
        period_end: NaiveDate,
    ) -> Result<Vec<AnyCauseEventForNegativeDesign>, StoreError> {
        sqlx::query_as::<_, AnyCauseEventForNegativeDesign>(
            "SELECT event.h3, event.occurred_on_local, event.cause_category,
                    geo.geographic_category
             FROM fire.ignition_events AS event
             JOIN validation.event_geographic_quality AS geo
                ON geo.ignition_event_id = event.id
             WHERE event.occurred_on_local BETWEEN $1 AND $2
             ORDER BY event.occurred_on_local, event.h3",
        )
        .bind(period_start)
        .bind(period_end)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// Registers a feature-snapshot metadata record, or returns the
    /// existing id if an identical `(family, logical_checksum)` was
    /// already registered.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the insert.
    pub async fn register_feature_snapshot(
        &self,
        spec: &FeatureSnapshotSpec,
    ) -> Result<String, StoreError> {
        let existing = sqlx::query_scalar::<_, String>(
            "SELECT id::text FROM features.feature_snapshots
             WHERE family = $1 AND logical_checksum = $2",
        )
        .bind(&spec.family)
        .bind(&spec.logical_checksum)
        .fetch_optional(&self.pool)
        .await?;
        if let Some(id) = existing {
            return Ok(id);
        }
        sqlx::query_scalar::<_, String>(
            "INSERT INTO features.feature_snapshots (
                family, source, provider, vintage, valid_from, valid_until,
                available_from, available_until, retrieved_at, code_version,
                normalizer_version, parameters, source_checksum, logical_checksum,
                reference_table, cell_count, h3_resolution, geographic_coverage,
                status, temporal_classification, limitations, license_attribution, notes
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,
                       'draft',$19,$20,$21,$22)
             RETURNING id::text",
        )
        .bind(&spec.family)
        .bind(&spec.source)
        .bind(&spec.provider)
        .bind(&spec.vintage)
        .bind(spec.valid_from)
        .bind(spec.valid_until)
        .bind(spec.available_from)
        .bind(spec.available_until)
        .bind(spec.retrieved_at)
        .bind(&spec.code_version)
        .bind(&spec.normalizer_version)
        .bind(&spec.parameters)
        .bind(&spec.source_checksum)
        .bind(&spec.logical_checksum)
        .bind(&spec.reference_table)
        .bind(spec.cell_count)
        .bind(spec.h3_resolution)
        .bind(&spec.geographic_coverage)
        .bind(&spec.temporal_classification)
        .bind(&spec.limitations)
        .bind(&spec.license_attribution)
        .bind(&spec.notes)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// Marks a snapshot `validated`/`active`, superseding any prior active
    /// snapshot in the same family.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the transaction.
    pub async fn activate_feature_snapshot(&self, snapshot_id: &str) -> Result<(), StoreError> {
        let mut transaction = self.pool.begin().await?;
        let family = sqlx::query_scalar::<_, String>(
            "SELECT family FROM features.feature_snapshots WHERE id = $1::uuid",
        )
        .bind(snapshot_id)
        .fetch_one(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE features.feature_snapshots SET status = 'superseded'
             WHERE family = $1 AND status = 'active' AND id <> $2::uuid",
        )
        .bind(&family)
        .bind(snapshot_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE features.feature_snapshots
             SET status = 'active', validated_at = COALESCE(validated_at, NOW())
             WHERE id = $1::uuid",
        )
        .bind(snapshot_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Computes a deterministic checksum, row count, and explicit-order
    /// summary of `public.cell_static` without pulling all rows into
    /// application memory (920,016+ rows at current volume).
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the read.
    pub async fn cell_static_snapshot_summary(&self) -> Result<(String, i64), StoreError> {
        let row = sqlx::query_as::<_, (Option<String>, i64)>(
            "SELECT md5(string_agg(h3::text || ':' || features::text, ',' ORDER BY h3)), COUNT(*)
             FROM public.cell_static",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok((row.0.unwrap_or_default(), row.1))
    }

    /// Creates or returns an immutable calendar rule version.
    ///
    /// # Errors
    ///
    /// Returns an error when the existing checksum differs or persistence fails.
    pub async fn ensure_calendar_rule_version(
        &self,
        rule: &CalendarRuleVersion,
    ) -> Result<String, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let existing = sqlx::query_as::<_, (String, String)>(
            "SELECT id::text, checksum FROM features.calendar_rule_versions WHERE logical_id = $1",
        )
        .bind(&rule.logical_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some((id, checksum)) = existing {
            if checksum != rule.checksum {
                return Err(StoreError::CalendarRuleChanged(rule.logical_id.clone()));
            }
            transaction.commit().await?;
            return Ok(id);
        }
        let id = sqlx::query_scalar::<_, String>(
            "INSERT INTO features.calendar_rule_versions (
                logical_id, rule_type, description, parameters, code_version,
                status, checksum, notes, activated_at
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,
                CASE WHEN $6 = 'active' THEN NOW() ELSE NULL END)
             RETURNING id::text",
        )
        .bind(&rule.logical_id)
        .bind(&rule.rule_type)
        .bind(&rule.description)
        .bind(&rule.parameters)
        .bind(&rule.code_version)
        .bind(&rule.status)
        .bind(&rule.checksum)
        .bind(&rule.notes)
        .fetch_one(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(id)
    }

    /// Persists historical calendar days for one rule version. Idempotent:
    /// replaying the same rule version and rows inserts nothing new.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the transaction.
    pub async fn persist_historical_calendar_days(
        &self,
        rule_version_id: &str,
        rows: &[HistoricalCalendarDayRecord],
    ) -> Result<(), StoreError> {
        let payload = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "date": row.date, "school_zone": row.school_zone, "year": row.year,
                    "month": row.month, "day_of_week": row.day_of_week,
                    "is_weekend": row.is_weekend, "public_holiday": row.public_holiday,
                    "public_holiday_label": row.public_holiday_label,
                    "school_holiday": row.school_holiday,
                    "school_holiday_label": row.school_holiday_label,
                    "is_day_before_public_holiday": row.is_day_before_public_holiday,
                    "is_day_after_public_holiday": row.is_day_after_public_holiday,
                    "season": row.season, "season_sine": row.season_sine,
                    "season_cosine": row.season_cosine, "available_from": row.available_from,
                    "source": row.source, "temporal_classification": row.temporal_classification,
                    "logical_checksum": row.logical_checksum
                })
            })
            .collect::<Vec<_>>();
        sqlx::query(
            "WITH input AS (
                SELECT * FROM jsonb_to_recordset($2::jsonb) AS value(
                    date date, school_zone text, year smallint, month smallint,
                    day_of_week smallint, is_weekend boolean, public_holiday boolean,
                    public_holiday_label text, school_holiday boolean,
                    school_holiday_label text, is_day_before_public_holiday boolean,
                    is_day_after_public_holiday boolean, season smallint,
                    season_sine double precision, season_cosine double precision,
                    available_from timestamptz, source text,
                    temporal_classification text, logical_checksum text
                )
             )
             INSERT INTO features.historical_calendar_days (
                    rule_version_id, date, school_zone, year, month, day_of_week,
                    is_weekend, public_holiday, public_holiday_label, school_holiday,
                    school_holiday_label, is_day_before_public_holiday,
                    is_day_after_public_holiday, season, season_sine, season_cosine,
                    available_from, source, temporal_classification, logical_checksum
             )
             SELECT $1::uuid, date, school_zone, year, month, day_of_week, is_weekend,
                    public_holiday, public_holiday_label, school_holiday,
                    school_holiday_label, is_day_before_public_holiday,
                    is_day_after_public_holiday, season, season_sine, season_cosine,
                    available_from, source, temporal_classification, logical_checksum
             FROM input
             ON CONFLICT (rule_version_id, date, school_zone) DO NOTHING",
        )
        .bind(rule_version_id)
        .bind(serde_json::json!(payload))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Creates a new draft dataset version.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the insert.
    pub async fn create_dataset_version(
        &self,
        spec: &DatasetVersionSpec,
    ) -> Result<String, StoreError> {
        sqlx::query_scalar::<_, String>(
            "INSERT INTO ml.dataset_versions (
                logical_id, name, description, observation_unit, h3_resolution,
                timezone, period_start, period_end, variant, code_version,
                migrations, quality_rule_versions, feature_snapshot_ids,
                calendar_rule_version_id, inclusion_rules, exclusion_rules,
                negative_strategy, negative_parameters, seed, splits,
                author_or_pipeline, notes
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14::uuid,$15,$16,$17,$18,$19,$20,$21,$22)
             RETURNING id::text",
        )
        .bind(&spec.logical_id)
        .bind(&spec.name)
        .bind(&spec.description)
        .bind(&spec.observation_unit)
        .bind(spec.h3_resolution)
        .bind(&spec.timezone)
        .bind(spec.period_start)
        .bind(spec.period_end)
        .bind(&spec.variant)
        .bind(&spec.code_version)
        .bind(&spec.migrations)
        .bind(&spec.quality_rule_versions)
        .bind(&spec.feature_snapshot_ids)
        .bind(&spec.calendar_rule_version_id)
        .bind(&spec.inclusion_rules)
        .bind(&spec.exclusion_rules)
        .bind(&spec.negative_strategy)
        .bind(&spec.negative_parameters)
        .bind(spec.seed)
        .bind(&spec.splits)
        .bind(&spec.author_or_pipeline)
        .bind(&spec.notes)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// Returns the existing non-finalized dataset version for `spec.logical_id`
    /// if its defining parameters are unchanged, or creates a new one.
    /// A rebuild with the same `logical_id` is therefore idempotent at the
    /// version level: it never duplicates a version, and callers can add a
    /// new [`ml.dataset_builds`] row and replay row/exclusion persistence
    /// against the same version. A `finalized` version, or one whose
    /// defining parameters differ, is rejected explicitly rather than
    /// silently reused or duplicated.
    ///
    /// Returns `(id, reused)`, where `reused` is `true` when an existing
    /// draft/validated version was returned instead of a new one.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::DatasetVersionFinalized`] if the existing
    /// version under this `logical_id` is finalized,
    /// [`StoreError::DatasetVersionParametersChanged`] if it exists with
    /// different defining parameters, or a database error otherwise.
    pub async fn get_or_create_dataset_version(
        &self,
        spec: &DatasetVersionSpec,
    ) -> Result<(String, bool), StoreError> {
        let existing = sqlx::query_as::<_, ExistingDatasetVersionDefinition>(
            "SELECT id::text, status, variant, h3_resolution, timezone,
                    period_start, period_end, seed, negative_strategy,
                    migrations, quality_rule_versions, feature_snapshot_ids,
                    calendar_rule_version_id::text, inclusion_rules,
                    exclusion_rules, negative_parameters, splits
             FROM ml.dataset_versions WHERE logical_id = $1",
        )
        .bind(&spec.logical_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(existing) = existing else {
            let id = self.create_dataset_version(spec).await?;
            return Ok((id, false));
        };
        if existing.status == "finalized" {
            return Err(StoreError::DatasetVersionFinalized(spec.logical_id.clone()));
        }
        if !existing.matches(spec) {
            return Err(StoreError::DatasetVersionParametersChanged(
                spec.logical_id.clone(),
            ));
        }
        Ok((existing.id, true))
    }

    /// Updates a non-finalized dataset version's status.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the update (including the
    /// finalized-immutability trigger).
    pub async fn set_dataset_version_status(
        &self,
        dataset_version_id: &str,
        status: &str,
    ) -> Result<(), StoreError> {
        sqlx::query("UPDATE ml.dataset_versions SET status = $2 WHERE id = $1::uuid")
            .bind(dataset_version_id)
            .bind(status)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Starts a new build attempt for a dataset version.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the insert.
    pub async fn start_dataset_build(
        &self,
        dataset_version_id: &str,
        code_version: &str,
        builder_version: &str,
        parameters: &Value,
        seed: i64,
    ) -> Result<String, StoreError> {
        sqlx::query_scalar::<_, String>(
            "INSERT INTO ml.dataset_builds (
                dataset_version_id, code_version, builder_version, parameters,
                seed, started_at
             ) VALUES ($1::uuid,$2,$3,$4,$5,NOW())
             RETURNING id::text",
        )
        .bind(dataset_version_id)
        .bind(code_version)
        .bind(builder_version)
        .bind(parameters)
        .bind(seed)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// Marks a build finished, successfully or not. A failed build stays auditable.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the update.
    pub async fn finish_dataset_build(
        &self,
        build_id: &str,
        succeeded: bool,
        counts: Option<&DatasetBuildCounts>,
        checksum: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE ml.dataset_builds SET
                status = $2, finished_at = NOW(), row_count = $3, positive_count = $4,
                negative_count = $5, exclusion_count = $6, checksum = $7, error_message = $8
             WHERE id = $1::uuid",
        )
        .bind(build_id)
        .bind(if succeeded { "succeeded" } else { "failed" })
        .bind(counts.map(|value| value.row_count))
        .bind(counts.map(|value| value.positive_count))
        .bind(counts.map(|value| value.negative_count))
        .bind(counts.map(|value| value.exclusion_count))
        .bind(checksum)
        .bind(error_message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Persists dataset rows, their snapshot provenance, and their event
    /// links atomically. Idempotent on `(dataset_version_id, deterministic_key)`.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the transaction.
    pub async fn persist_dataset_rows(
        &self,
        dataset_version_id: &str,
        build_id: &str,
        rows: &[DatasetRowRecord],
    ) -> Result<(), StoreError> {
        let mut transaction = self.pool.begin().await?;
        let row_ids =
            insert_dataset_rows(&mut transaction, dataset_version_id, build_id, rows).await?;
        insert_dataset_row_snapshots(&mut transaction, rows, &row_ids).await?;
        insert_dataset_event_links(&mut transaction, rows, &row_ids).await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Replaces the exclusion set for one dataset version with the
    /// current build's recomputed exclusions. Idempotent: rebuilding the
    /// same version from the same inputs first clears its prior exclusion
    /// rows (these are ERYTHEON-derived audit rows scoped to this dataset
    /// version, never `fire.ignition_events` itself, which is never
    /// touched) then inserts the freshly computed set, so replaying a
    /// build never accumulates duplicates.
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the transaction.
    pub async fn persist_dataset_exclusions(
        &self,
        dataset_version_id: &str,
        exclusions: &[DatasetExclusionRecord],
    ) -> Result<(), StoreError> {
        let payload = exclusions
            .iter()
            .map(|exclusion| {
                serde_json::json!({
                    "ignition_event_id": exclusion.ignition_event_id, "h3": exclusion.h3,
                    "local_date": exclusion.local_date, "reason_category": exclusion.reason_category,
                    "rule": exclusion.rule, "rule_version": exclusion.rule_version,
                    "details": exclusion.details, "reintegration_possible": exclusion.reintegration_possible
                })
            })
            .collect::<Vec<_>>();
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM ml.dataset_exclusions WHERE dataset_version_id = $1::uuid")
            .bind(dataset_version_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "WITH input AS (
                SELECT * FROM jsonb_to_recordset($2::jsonb) AS value(
                    ignition_event_id text, h3 bigint, local_date date,
                    reason_category text, rule text, rule_version text,
                    details jsonb, reintegration_possible boolean
                )
             )
             INSERT INTO ml.dataset_exclusions (
                    dataset_version_id, ignition_event_id, h3, local_date,
                    reason_category, rule, rule_version, details, reintegration_possible
             )
             SELECT $1::uuid, ignition_event_id::uuid, h3, local_date, reason_category,
                    rule, rule_version, details, reintegration_possible
             FROM input",
        )
        .bind(dataset_version_id)
        .bind(serde_json::json!(payload))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    /// Finalizes a dataset version, freezing it permanently. Fails if the
    /// version is already finalized (enforced by a database trigger).
    ///
    /// # Errors
    ///
    /// Returns an error if `PostgreSQL` rejects the update.
    pub async fn finalize_dataset_version(
        &self,
        dataset_version_id: &str,
        checksum: &str,
        counts: &DatasetBuildCounts,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE ml.dataset_versions SET
                status = 'finalized', finalized_at = NOW(), checksum = $2,
                row_count = $3, positive_count = $4, negative_count = $5,
                exclusion_count = $6
             WHERE id = $1::uuid",
        )
        .bind(dataset_version_id)
        .bind(checksum)
        .bind(counts.row_count)
        .bind(counts.positive_count)
        .bind(counts.negative_count)
        .bind(counts.exclusion_count)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

async fn insert_dataset_rows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    dataset_version_id: &str,
    build_id: &str,
    rows: &[DatasetRowRecord],
) -> Result<HashMap<String, String>, StoreError> {
    let payload = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "h3": row.h3, "h3_resolution": row.h3_resolution,
                "local_date": row.local_date, "reference_timestamp": row.reference_timestamp,
                "split": row.split, "label": row.label, "row_category": row.row_category,
                "selection_method": row.selection_method, "weight": row.weight,
                "quality_flags": row.quality_flags, "features": row.features,
                "temporal_availability": row.temporal_availability,
                "deterministic_key": row.deterministic_key, "row_checksum": row.row_checksum,
                "justification": row.justification
            })
        })
        .collect::<Vec<_>>();
    let inserted = sqlx::query_as::<_, (String, String)>(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($3::jsonb) AS value(
                h3 bigint, h3_resolution smallint, local_date date,
                reference_timestamp timestamptz, split text, label smallint,
                row_category text, selection_method text, weight double precision,
                quality_flags jsonb, features jsonb, temporal_availability jsonb,
                deterministic_key text, row_checksum text, justification text
            )
         )
         INSERT INTO ml.dataset_rows (
                dataset_version_id, build_id, deterministic_key, h3, h3_resolution,
                local_date, reference_timestamp, split, label, row_category,
                selection_method, weight, quality_flags, features,
                temporal_availability, row_checksum, justification
         )
         SELECT $1::uuid, $2::uuid, deterministic_key, h3, h3_resolution, local_date,
                reference_timestamp, split, label, row_category, selection_method,
                weight, quality_flags, features, temporal_availability, row_checksum,
                justification
         FROM input
         ON CONFLICT (dataset_version_id, deterministic_key)
         DO UPDATE SET deterministic_key = ml.dataset_rows.deterministic_key
         RETURNING deterministic_key, id::text",
    )
    .bind(dataset_version_id)
    .bind(build_id)
    .bind(serde_json::json!(payload))
    .fetch_all(&mut **transaction)
    .await?;
    Ok(inserted.into_iter().collect())
}

async fn insert_dataset_row_snapshots(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rows: &[DatasetRowRecord],
    row_ids: &HashMap<String, String>,
) -> Result<(), StoreError> {
    let snapshot_payload = rows
        .iter()
        .flat_map(|row| {
            let row_id = row_ids.get(&row.deterministic_key).cloned().unwrap_or_default();
            row.snapshot_ids
                .iter()
                .map(move |snapshot_id| serde_json::json!({"row_id": row_id, "snapshot_id": snapshot_id}))
        })
        .collect::<Vec<_>>();
    if snapshot_payload.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($1::jsonb)
                AS value(row_id text, snapshot_id text)
        )
         INSERT INTO ml.dataset_row_snapshots (dataset_row_id, feature_snapshot_id)
         SELECT row_id::uuid, snapshot_id::uuid FROM input
         ON CONFLICT DO NOTHING",
    )
    .bind(serde_json::json!(snapshot_payload))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn insert_dataset_event_links(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rows: &[DatasetRowRecord],
    row_ids: &HashMap<String, String>,
) -> Result<(), StoreError> {
    let link_payload = rows
        .iter()
        .flat_map(|row| {
            let row_id = row_ids
                .get(&row.deterministic_key)
                .cloned()
                .unwrap_or_default();
            row.event_links.iter().map(move |link| {
                serde_json::json!({
                    "row_id": row_id, "event_id": link.ignition_event_id, "role": link.role,
                    "label_quality_confidence": link.label_quality_confidence,
                    "geographic_quality_category": link.geographic_quality_category,
                    "duplicate_group_id": link.duplicate_group_id,
                    "inclusion_rule": link.inclusion_rule, "justification": link.justification
                })
            })
        })
        .collect::<Vec<_>>();
    if link_payload.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($1::jsonb) AS value(
                row_id text, event_id text, role text,
                label_quality_confidence text, geographic_quality_category text,
                duplicate_group_id text, inclusion_rule text, justification text
            )
         )
         INSERT INTO ml.dataset_event_links (
                dataset_row_id, ignition_event_id, role, label_quality_confidence,
                geographic_quality_category, duplicate_group_id, inclusion_rule,
                justification
         )
         SELECT row_id::uuid, event_id::uuid, role, label_quality_confidence,
                geographic_quality_category, duplicate_group_id::uuid, inclusion_rule,
                justification
         FROM input
         ON CONFLICT (dataset_row_id, ignition_event_id) DO NOTHING",
    )
    .bind(serde_json::json!(link_payload))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

/// Read-only helper used by tests and CLI inspection; not part of the hot path.
///
/// # Errors
///
/// Returns an error if `PostgreSQL` rejects the read.
pub async fn dataset_row_count(pool: &PgPool, dataset_version_id: &str) -> Result<i64, StoreError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM ml.dataset_rows WHERE dataset_version_id = $1::uuid",
    )
    .bind(dataset_version_id)
    .fetch_one(pool)
    .await
    .map_err(StoreError::from)
}

/// Read-only summary of one dataset version for manual inspection.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct DatasetVersionSummary {
    pub logical_id: String,
    pub variant: String,
    pub status: String,
    pub row_count: Option<i64>,
    pub positive_count: Option<i64>,
    pub negative_count: Option<i64>,
    pub exclusion_count: Option<i64>,
    pub checksum: Option<String>,
    pub created_at: DateTime<Utc>,
    pub finalized_at: Option<DateTime<Utc>>,
}

impl Store {
    /// Loads a read-only summary of a dataset version.
    ///
    /// # Errors
    ///
    /// Returns an error if the version does not exist or the read fails.
    pub async fn dataset_version_summary(
        &self,
        dataset_version_id: &str,
    ) -> Result<DatasetVersionSummary, StoreError> {
        sqlx::query_as::<_, DatasetVersionSummary>(
            "SELECT logical_id, variant, status, row_count, positive_count,
                    negative_count, exclusion_count, checksum, created_at, finalized_at
             FROM ml.dataset_versions WHERE id = $1::uuid",
        )
        .bind(dataset_version_id)
        .fetch_one(&self.pool)
        .await
        .map_err(StoreError::from)
    }
}
