//! Immutable BLUE daily forecast bulletins and read-only evidence views.

use chrono::{DateTime, NaiveDate, TimeZone as _, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::{Store, StoreError};

const BULLETIN_HOUR_UTC: u32 = 6;
const ALERT_THRESHOLD: f32 = 0.65;
const CRITICAL_THRESHOLD: f32 = 0.75;

#[derive(Clone, Debug)]
pub struct BlueForecastContext {
    pub environment: String,
    pub application_revision: String,
    pub application_image: String,
    pub application_image_digest: String,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct BlueForecastBulletinRow {
    pub id: String,
    pub logical_id: String,
    pub bulletin_date: NaiveDate,
    pub scheduled_for: DateTime<Utc>,
    pub issued_at: DateTime<Utc>,
    pub forecast_batch_computed_at: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub forecast_source: String,
    pub model_version_id: i64,
    pub application_revision: String,
    pub forecast_cell_count: i64,
    pub mapped_cell_count: i64,
    pub unmapped_cell_count: i64,
    pub commune_count: i64,
    pub alerts_24h: i64,
    pub alerts_48h: i64,
    pub checksum: Option<String>,
    pub status: String,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct BlueForecastAlertRow {
    pub id: String,
    pub bulletin_id: String,
    pub bulletin_date: NaiveDate,
    pub issued_at: DateTime<Utc>,
    pub insee_code: String,
    pub commune_name: String,
    pub department_code: Option<String>,
    pub horizon: String,
    pub valid_at: DateTime<Utc>,
    pub alert_index: f32,
    pub max_score: f32,
    pub mean_score: f32,
    pub physical_at_peak: f32,
    pub human_at_peak: f32,
    pub evaluated_cell_count: i64,
    pub elevated_cell_count: i64,
    pub critical_cell_count: i64,
    pub risk_level: String,
    pub top_factors: Value,
    pub evaluation_status: String,
    pub evidence_count: i64,
}

const BULLETIN_COLUMNS: &str = "id::text,logical_id,bulletin_date,scheduled_for,issued_at,
     forecast_batch_computed_at,forecast_source,model_version_id,application_revision,
     forecast_cell_count,mapped_cell_count,unmapped_cell_count,commune_count,
     alerts_24h,alerts_48h,checksum,status,published_at";

impl Store {
    /// Captures the first complete batch issued after 06:00 UTC as the day's
    /// immutable BLUE bulletin. Before 06:00 this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns an error when provenance, horizons, commune coverage or
    /// publication integrity is incomplete.
    #[allow(clippy::too_many_lines)]
    pub async fn capture_blue_daily_bulletin(
        &self,
        computed_at: DateTime<Utc>,
        forecast_source: &str,
        context: &BlueForecastContext,
    ) -> Result<Option<BlueForecastBulletinRow>, StoreError> {
        if forecast_source.trim().is_empty()
            || context.environment.trim().is_empty()
            || context.application_revision.trim().is_empty()
            || context.application_image.trim().is_empty()
            || context.application_image_digest.trim().is_empty()
        {
            return Err(StoreError::SnapshotContract(
                "BLUE bulletin provenance is incomplete".to_owned(),
            ));
        }
        let bulletin_date = computed_at.date_naive();
        let scheduled_for = Utc.from_utc_datetime(&bulletin_date.and_time(
            chrono::NaiveTime::from_hms_opt(BULLETIN_HOUR_UTC, 0, 0).ok_or_else(|| {
                StoreError::SnapshotContract("invalid BLUE issue hour".to_owned())
            })?,
        ));
        if computed_at < scheduled_for {
            return Ok(None);
        }
        let logical_id = format!("blue-daily-{}", bulletin_date.format("%Y-%m-%d"));
        if let Some(row) = self.blue_bulletin_by_logical_id(&logical_id).await? {
            return if row.status == "published" {
                Ok(Some(row))
            } else {
                Err(StoreError::SnapshotContract(format!(
                    "BLUE bulletin {logical_id} has status {}",
                    row.status
                )))
            };
        }
        let model_version_id: i64 = sqlx::query_scalar(
            "SELECT id FROM human_model_versions WHERE active ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StoreError::SnapshotContract("no active BLUE model".to_owned()))?;
        let coverage_mask_id: String = sqlx::query_scalar(
            "SELECT id::text FROM observability.coverage_masks
             WHERE family='operational_aoi' AND status='published'
             ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StoreError::SnapshotContract("no published coverage mask".to_owned()))?;
        let (cells_24h, cells_48h): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(DISTINCT h3) FILTER (WHERE horizon='hours_24'),
                    COUNT(DISTINCT h3) FILTER (WHERE horizon='hours_48')
             FROM risk_scores WHERE computed_at=$1",
        )
        .bind(computed_at)
        .fetch_one(&self.pool)
        .await?;
        if cells_24h == 0 || cells_24h != cells_48h {
            return Err(StoreError::SnapshotContract(format!(
                "incomplete BLUE horizons: +24 h={cells_24h}, +48 h={cells_48h}"
            )));
        }
        let id: Option<String> = sqlx::query_scalar(
            "INSERT INTO blue.forecast_bulletins(
                logical_id,bulletin_date,scheduled_for,issued_at,forecast_batch_computed_at,
                forecast_source,model_version_id,application_revision,application_image,
                application_image_digest,environment,coverage_mask_id,forecast_cell_count,
                unmapped_cell_count,aggregation_contract)
             VALUES($1,$2,$3,$4,$4,$5,$6,$7,$8,$9,$10,$11::uuid,$12,$12,
                jsonb_build_object('version','commune-p95-v1','alert_threshold',$13,
                    'critical_threshold',$14,'interpretation',
                    'relative_vigilance_index_not_calibrated_fire_probability'))
             ON CONFLICT(logical_id) DO NOTHING RETURNING id::text",
        )
        .bind(&logical_id)
        .bind(bulletin_date)
        .bind(scheduled_for)
        .bind(computed_at)
        .bind(forecast_source)
        .bind(model_version_id)
        .bind(&context.application_revision)
        .bind(&context.application_image)
        .bind(&context.application_image_digest)
        .bind(&context.environment)
        .bind(&coverage_mask_id)
        .bind(cells_24h)
        .bind(ALERT_THRESHOLD)
        .bind(CRITICAL_THRESHOLD)
        .fetch_optional(&self.pool)
        .await?;
        let Some(id) = id else {
            return self
                .blue_bulletin_by_logical_id(&logical_id)
                .await
                .map(|row| row.filter(|item| item.status == "published"));
        };
        match self
            .fill_and_publish_blue_bulletin(&id, computed_at, cells_24h)
            .await
        {
            Ok(row) => Ok(Some(row)),
            Err(error) => {
                let _ = sqlx::query(
                    "UPDATE blue.forecast_bulletins SET status='failed'
                     WHERE id=$1::uuid AND status='building'",
                )
                .bind(&id)
                .execute(&self.pool)
                .await;
                Err(error)
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn fill_and_publish_blue_bulletin(
        &self,
        id: &str,
        computed_at: DateTime<Utc>,
        forecast_cell_count: i64,
    ) -> Result<BlueForecastBulletinRow, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "CREATE TEMP TABLE blue_forecast_aggregate ON COMMIT DROP AS
             SELECT b.insee_code,b.name commune_name,b.department_code,
                COUNT(DISTINCT r.h3)::bigint evaluated_cell_count,
                MAX(r.valid_at) FILTER (WHERE r.horizon='hours_24') valid_at_24,
                percentile_cont(0.95) WITHIN GROUP (ORDER BY r.score)
                    FILTER (WHERE r.horizon='hours_24')::real p95_24,
                MAX(r.score) FILTER (WHERE r.horizon='hours_24')::real max_24,
                AVG(r.score) FILTER (WHERE r.horizon='hours_24')::real mean_24,
                (array_agg(r.physical ORDER BY r.score DESC)
                    FILTER (WHERE r.horizon='hours_24'))[1]::real physical_24,
                (array_agg(r.human ORDER BY r.score DESC)
                    FILTER (WHERE r.horizon='hours_24'))[1]::real human_24,
                (array_agg(r.factors ORDER BY r.score DESC)
                    FILTER (WHERE r.horizon='hours_24'))[1] factors_24,
                COUNT(*) FILTER (WHERE r.horizon='hours_24' AND r.score>=0.65)::bigint elevated_24,
                COUNT(*) FILTER (WHERE r.horizon='hours_24' AND r.score>=0.75)::bigint critical_24,
                MAX(r.valid_at) FILTER (WHERE r.horizon='hours_48') valid_at_48,
                percentile_cont(0.95) WITHIN GROUP (ORDER BY r.score)
                    FILTER (WHERE r.horizon='hours_48')::real p95_48,
                MAX(r.score) FILTER (WHERE r.horizon='hours_48')::real max_48,
                AVG(r.score) FILTER (WHERE r.horizon='hours_48')::real mean_48,
                (array_agg(r.physical ORDER BY r.score DESC)
                    FILTER (WHERE r.horizon='hours_48'))[1]::real physical_48,
                (array_agg(r.human ORDER BY r.score DESC)
                    FILTER (WHERE r.horizon='hours_48'))[1]::real human_48,
                (array_agg(r.factors ORDER BY r.score DESC)
                    FILTER (WHERE r.horizon='hours_48'))[1] factors_48,
                COUNT(*) FILTER (WHERE r.horizon='hours_48' AND r.score>=0.65)::bigint elevated_48,
                COUNT(*) FILTER (WHERE r.horizon='hours_48' AND r.score>=0.75)::bigint critical_48
             FROM reference.commune_boundaries b
             JOIN reference.commune_h3_cells c ON c.insee_code=b.insee_code
             JOIN risk_scores r ON r.h3=c.h3 AND r.computed_at=$1
                AND r.horizon IN ('hours_24','hours_48')
             GROUP BY b.insee_code,b.name,b.department_code
             HAVING COUNT(*) FILTER (WHERE r.horizon='hours_24')>0
                AND COUNT(*) FILTER (WHERE r.horizon='hours_48')>0",
        )
        .bind(computed_at)
        .execute(&mut *tx)
        .await?;
        let (commune_count, mapped_cells): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*),COALESCE(SUM(evaluated_cell_count),0)::bigint
             FROM blue_forecast_aggregate",
        )
        .fetch_one(&mut *tx)
        .await?;
        if commune_count == 0 || mapped_cells * 100 < forecast_cell_count * 99 {
            return Err(StoreError::SnapshotContract(format!(
                "commune coverage is {mapped_cells}/{forecast_cell_count} cells"
            )));
        }
        sqlx::query(
            "INSERT INTO blue.forecast_index_archives(
                bulletin_id,commune_codes,commune_count,code_order_checksum,
                p95_24h,max_24h,p95_48h,max_48h)
             SELECT $1::uuid,array_agg(insee_code ORDER BY insee_code),COUNT(*),
                encode(digest(string_agg(insee_code,',' ORDER BY insee_code),'sha256'),'hex'),
                string_agg(float4send(p95_24),''::bytea ORDER BY insee_code),
                string_agg(float4send(max_24),''::bytea ORDER BY insee_code),
                string_agg(float4send(p95_48),''::bytea ORDER BY insee_code),
                string_agg(float4send(max_48),''::bytea ORDER BY insee_code)
             FROM blue_forecast_aggregate",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO blue.forecast_alerts(
                bulletin_id,insee_code,commune_name,department_code,horizon,valid_at,
                alert_index,max_score,mean_score,physical_at_peak,human_at_peak,
                evaluated_cell_count,elevated_cell_count,critical_cell_count,risk_level,top_factors)
             SELECT $1::uuid,a.insee_code,a.commune_name,a.department_code,v.horizon,v.valid_at,
                v.alert_index,v.max_score,v.mean_score,v.physical,v.human,a.evaluated_cell_count,
                v.elevated,v.critical,CASE WHEN v.alert_index>=0.75 THEN 'critical'
                ELSE 'elevated' END,v.factors
             FROM blue_forecast_aggregate a CROSS JOIN LATERAL (VALUES
                ('hours_24',valid_at_24,p95_24,max_24,mean_24,physical_24,human_24,elevated_24,critical_24,factors_24),
                ('hours_48',valid_at_48,p95_48,max_48,mean_48,physical_48,human_48,elevated_48,critical_48,factors_48)
             ) v(horizon,valid_at,alert_index,max_score,mean_score,physical,human,elevated,critical,factors)
             WHERE v.alert_index>=0.65",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO blue.forecast_evaluations(alert_id,status)
             SELECT id,'pending' FROM blue.forecast_alerts WHERE bulletin_id=$1::uuid",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        let checksum: String = sqlx::query_scalar(
            "SELECT encode(digest(p95_24h||max_24h||p95_48h||max_48h,'sha256'),'hex')
             FROM blue.forecast_index_archives WHERE bulletin_id=$1::uuid",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE blue.forecast_bulletins SET mapped_cell_count=$2,
                unmapped_cell_count=forecast_cell_count-$2,commune_count=$3,
                alerts_24h=(SELECT COUNT(*) FROM blue.forecast_alerts WHERE bulletin_id=$1::uuid AND horizon='hours_24'),
                alerts_48h=(SELECT COUNT(*) FROM blue.forecast_alerts WHERE bulletin_id=$1::uuid AND horizon='hours_48'),
                checksum=$4,status='published',published_at=NOW()
             WHERE id=$1::uuid AND status='building'",
        )
        .bind(id)
        .bind(mapped_cells)
        .bind(commune_count)
        .bind(checksum)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.blue_bulletin(id)
            .await?
            .ok_or_else(|| StoreError::InvalidPersistedCount(0))
    }

    async fn blue_bulletin_by_logical_id(
        &self,
        logical_id: &str,
    ) -> Result<Option<BlueForecastBulletinRow>, StoreError> {
        let query =
            format!("SELECT {BULLETIN_COLUMNS} FROM blue.forecast_bulletins WHERE logical_id=$1");
        sqlx::query_as(&query)
            .bind(logical_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::from)
    }

    /// Reads one immutable BLUE bulletin.
    ///
    /// # Errors
    ///
    /// Returns a database error when the read cannot be completed.
    pub async fn blue_bulletin(
        &self,
        id: &str,
    ) -> Result<Option<BlueForecastBulletinRow>, StoreError> {
        let query =
            format!("SELECT {BULLETIN_COLUMNS} FROM blue.forecast_bulletins WHERE id=$1::uuid");
        sqlx::query_as(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::from)
    }

    /// Reads the latest published BLUE bulletin.
    ///
    /// # Errors
    ///
    /// Returns a database error when the read cannot be completed.
    pub async fn latest_blue_bulletin(
        &self,
    ) -> Result<Option<BlueForecastBulletinRow>, StoreError> {
        let query = format!(
            "SELECT {BULLETIN_COLUMNS} FROM blue.forecast_bulletins
             WHERE status='published' ORDER BY bulletin_date DESC LIMIT 1"
        );
        sqlx::query_as(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(StoreError::from)
    }

    /// Lists recent published bulletins.
    ///
    /// # Errors
    ///
    /// Returns a database error when the read cannot be completed.
    pub async fn list_blue_bulletins(
        &self,
        limit: i64,
    ) -> Result<Vec<BlueForecastBulletinRow>, StoreError> {
        let query = format!(
            "SELECT {BULLETIN_COLUMNS} FROM blue.forecast_bulletins
             WHERE status='published' ORDER BY bulletin_date DESC LIMIT $1"
        );
        sqlx::query_as(&query)
            .bind(limit.clamp(1, 366))
            .fetch_all(&self.pool)
            .await
            .map_err(StoreError::from)
    }

    /// Lists readable alerts for one bulletin.
    ///
    /// # Errors
    ///
    /// Returns a database error when the read cannot be completed.
    pub async fn list_blue_alerts(
        &self,
        bulletin_id: &str,
        horizon: Option<&str>,
        limit: i64,
    ) -> Result<Vec<BlueForecastAlertRow>, StoreError> {
        sqlx::query_as(
            "SELECT a.id::text,a.bulletin_id::text,b.bulletin_date,b.issued_at,
                a.insee_code,a.commune_name,a.department_code,a.horizon,a.valid_at,
                a.alert_index,a.max_score,a.mean_score,a.physical_at_peak,a.human_at_peak,
                a.evaluated_cell_count,a.elevated_cell_count,a.critical_cell_count,
                a.risk_level,a.top_factors,e.status evaluation_status,e.evidence_count
             FROM blue.forecast_alerts a JOIN blue.forecast_bulletins b ON b.id=a.bulletin_id
             JOIN blue.forecast_evaluations e ON e.alert_id=a.id
             WHERE a.bulletin_id=$1::uuid AND ($2::text IS NULL OR a.horizon=$2)
             ORDER BY a.alert_index DESC,a.commune_name LIMIT $3",
        )
        .bind(bulletin_id)
        .bind(horizon)
        .bind(limit.clamp(1, 10_000))
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// Reads one alert and its current evidence status.
    ///
    /// # Errors
    ///
    /// Returns a database error when the read cannot be completed.
    pub async fn blue_alert(&self, id: &str) -> Result<Option<BlueForecastAlertRow>, StoreError> {
        sqlx::query_as(
            "SELECT a.id::text,a.bulletin_id::text,b.bulletin_date,b.issued_at,
                a.insee_code,a.commune_name,a.department_code,a.horizon,a.valid_at,
                a.alert_index,a.max_score,a.mean_score,a.physical_at_peak,a.human_at_peak,
                a.evaluated_cell_count,a.elevated_cell_count,a.critical_cell_count,
                a.risk_level,a.top_factors,e.status evaluation_status,e.evidence_count
             FROM blue.forecast_alerts a JOIN blue.forecast_bulletins b ON b.id=a.bulletin_id
             JOIN blue.forecast_evaluations e ON e.alert_id=a.id WHERE a.id=$1::uuid",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(StoreError::from)
    }
}
