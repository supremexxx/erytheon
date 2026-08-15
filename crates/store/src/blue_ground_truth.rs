//! Operational Ground Truth registry for immutable BLUE forecasts.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;

use crate::{Store, StoreError};

const MATCHING_RULE_VERSION: &str = "blue-ground-truth-v1";
const ALERT_THRESHOLD: f32 = 0.65;

#[derive(Clone, Debug, Serialize)]
pub struct BlueGroundTruthSummary {
    pub generated_at: DateTime<Utc>,
    pub last_refresh_at: Option<DateTime<Utc>>,
    pub observation_count: i64,
    pub satellite_signal_windows: i64,
    pub confirmed_ignitions: i64,
    pub forecast_comparisons: i64,
    pub signal_covered: i64,
    pub signal_below_threshold: i64,
    pub confirmed_hits: i64,
    pub confirmed_misses: i64,
    pub signal_coverage_rate: Option<f64>,
    pub confirmed_recall: Option<f64>,
    pub recent_matches: Vec<BlueGroundTruthMatchRow>,
    pub interpretation: &'static str,
    pub limitations: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct BlueGroundTruthMatchRow {
    pub bulletin_id: String,
    pub bulletin_date: NaiveDate,
    pub insee_code: String,
    pub commune_name: String,
    pub department_code: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub observed_until: DateTime<Utc>,
    pub evidence_class: String,
    pub signal_count: i64,
    pub max_frp: Option<f32>,
    pub horizon: String,
    pub forecast_score: f32,
    pub forecast_max_score: f32,
    pub classification: String,
    pub lead_time_hours: f64,
}

#[derive(Clone, Debug)]
pub struct BlueGroundTruthRefresh {
    pub satellite_windows_upserted: u64,
    pub confirmed_ignitions_upserted: u64,
    pub comparisons_inserted: u64,
}

#[derive(sqlx::FromRow)]
struct GroundTruthObservation {
    id: i64,
    evidence_class: String,
    insee_code: String,
    occurred_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ForecastArchive {
    id: String,
    issued_at: DateTime<Utc>,
    scheduled_for: DateTime<Utc>,
    commune_codes: Vec<String>,
    p95_24h: Vec<u8>,
    max_24h: Vec<u8>,
    p95_48h: Vec<u8>,
    max_48h: Vec<u8>,
}

#[derive(sqlx::FromRow)]
struct GroundTruthCounts {
    observation_count: i64,
    satellite_signal_windows: i64,
    confirmed_ignitions: i64,
    forecast_comparisons: i64,
    signal_covered: i64,
    signal_below_threshold: i64,
    confirmed_hits: i64,
    confirmed_misses: i64,
    signal_coverage_rate: Option<f64>,
    confirmed_recall: Option<f64>,
}

impl Store {
    /// Refreshes observed signal windows and compares them with immutable
    /// commune-level forecasts. It never mutates a published bulletin.
    ///
    /// # Errors
    ///
    /// Returns an error when observation normalization, archive decoding, or
    /// persistence fails.
    #[allow(clippy::too_many_lines)]
    pub async fn refresh_blue_ground_truth(&self) -> Result<BlueGroundTruthRefresh, StoreError> {
        let started_at = Utc::now();
        let satellite_windows_upserted = sqlx::query(
            "WITH clustered AS (
                SELECT m.insee_code,b.name commune_name,b.department_code,
                    date_bin(INTERVAL '6 hours',o.observed_at,TIMESTAMPTZ '2000-01-01') bucket,
                    MIN(o.observed_at) occurred_at,MAX(o.observed_at) observed_until,
                    COUNT(*)::bigint signal_count,
                    MAX(CASE WHEN jsonb_typeof(o.payload->'frp')='number'
                        THEN (o.payload->>'frp')::real
                        WHEN o.payload->>'frp' ~ '^[0-9]+([.][0-9]+)?$'
                        THEN (o.payload->>'frp')::real END) max_frp,
                    COUNT(*) FILTER (WHERE o.payload->>'confidence' IN ('h','high')) high_confidence,
                    COUNT(*) FILTER (WHERE o.payload->>'confidence' IN ('n','nominal')) nominal_confidence
                FROM observations o
                JOIN reference.commune_h3_cells m ON m.h3=o.h3 AND m.h3_resolution=8
                JOIN reference.commune_boundaries b ON b.insee_code=m.insee_code
                WHERE o.source='firms' AND o.observed_at IS NOT NULL
                GROUP BY m.insee_code,b.name,b.department_code,bucket
             )
             INSERT INTO blue.ground_truth_observations(
                observation_key,evidence_class,insee_code,commune_name,department_code,
                occurred_at,observed_until,signal_count,max_frp,metadata)
             SELECT 'satellite:'||insee_code||':'||to_char(bucket AT TIME ZONE 'UTC','YYYYMMDDHH24MI'),
                'satellite_signal',insee_code,commune_name,department_code,occurred_at,
                observed_until,signal_count,max_frp,jsonb_build_object(
                    'window_hours',6,'high_confidence_count',high_confidence,
                    'nominal_confidence_count',nominal_confidence)
             FROM clustered
             ON CONFLICT(observation_key) DO UPDATE SET
                observed_until=EXCLUDED.observed_until,signal_count=EXCLUDED.signal_count,
                max_frp=EXCLUDED.max_frp,metadata=EXCLUDED.metadata,updated_at=NOW()
             WHERE blue.ground_truth_observations.observed_until<>EXCLUDED.observed_until
                OR blue.ground_truth_observations.signal_count<>EXCLUDED.signal_count
                OR blue.ground_truth_observations.max_frp IS DISTINCT FROM EXCLUDED.max_frp
                OR blue.ground_truth_observations.metadata<>EXCLUDED.metadata",
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        let confirmed_ignitions_upserted = sqlx::query(
            "INSERT INTO blue.ground_truth_observations(
                observation_key,evidence_class,insee_code,commune_name,department_code,
                occurred_at,observed_until,signal_count,official_event_id,metadata)
             SELECT 'ignition:'||e.id::text,'confirmed_ignition',m.insee_code,b.name,
                b.department_code,e.occurred_at,e.occurred_at,1,e.id,
                jsonb_build_object('surface_ha',e.surface_ha,'cause_category',e.cause_category,
                    'geographic_quality',e.geographic_quality)
             FROM fire.ignition_events e
             JOIN reference.commune_h3_cells m ON m.h3=e.h3 AND m.h3_resolution=e.h3_resolution
             JOIN reference.commune_boundaries b ON b.insee_code=m.insee_code
             WHERE e.is_active
             ON CONFLICT(observation_key) DO NOTHING",
        )
        .execute(&self.pool)
        .await?
        .rows_affected();

        let archives: Vec<ForecastArchive> = sqlx::query_as(
            "SELECT b.id::text,b.issued_at,b.scheduled_for,a.commune_codes,
                a.p95_24h,a.max_24h,a.p95_48h,a.max_48h
             FROM blue.forecast_bulletins b
             JOIN blue.forecast_index_archives a ON a.bulletin_id=b.id
             WHERE b.status='published' ORDER BY b.scheduled_for",
        )
        .fetch_all(&self.pool)
        .await?;
        let minimum_issue = archives.first().map(|archive| archive.issued_at);
        let observations: Vec<GroundTruthObservation> = if let Some(minimum_issue) = minimum_issue {
            sqlx::query_as(
                "SELECT id,evidence_class,insee_code,occurred_at
                 FROM blue.ground_truth_observations
                 WHERE occurred_at >= $1 ORDER BY occurred_at",
            )
            .bind(minimum_issue)
            .fetch_all(&self.pool)
            .await?
        } else {
            Vec::new()
        };

        let mut comparisons_inserted = 0_u64;
        let mut tx = self.pool.begin().await?;
        for archive in &archives {
            validate_archive(archive)?;
            for observation in observations.iter().filter(|observation| {
                observation.occurred_at >= archive.issued_at
                    && observation.occurred_at <= archive.scheduled_for + Duration::hours(48)
            }) {
                let Ok(position) = archive.commune_codes.binary_search(&observation.insee_code)
                else {
                    continue;
                };
                for (horizon, hours, scores, maxima) in [
                    ("hours_24", 24_i64, &archive.p95_24h, &archive.max_24h),
                    ("hours_48", 48_i64, &archive.p95_48h, &archive.max_48h),
                ] {
                    if observation.occurred_at > archive.scheduled_for + Duration::hours(hours) {
                        continue;
                    }
                    let forecast_score = decode_float(scores, position)?;
                    let forecast_max_score = decode_float(maxima, position)?;
                    let covered = forecast_score >= ALERT_THRESHOLD;
                    let classification = match (observation.evidence_class.as_str(), covered) {
                        ("confirmed_ignition", true) => "confirmed_hit",
                        ("confirmed_ignition", false) => "confirmed_miss",
                        (_, true) => "signal_covered",
                        _ => "signal_below_threshold",
                    };
                    let lead_time_hours = observation
                        .occurred_at
                        .signed_duration_since(archive.issued_at)
                        .to_std()
                        .map_err(|error| {
                            StoreError::SnapshotContract(format!(
                                "BLUE Ground Truth lead time is invalid: {error}"
                            ))
                        })?
                        .as_secs_f64()
                        / 3_600.0;
                    comparisons_inserted += sqlx::query(
                        "INSERT INTO blue.ground_truth_matches(
                            observation_id,bulletin_id,horizon,forecast_score,
                            forecast_max_score,alert_threshold,classification,
                            lead_time_hours,matching_rule_version)
                         VALUES($1,$2::uuid,$3,$4,$5,$6,$7,$8,$9)
                         ON CONFLICT(observation_id,bulletin_id,horizon) DO NOTHING",
                    )
                    .bind(observation.id)
                    .bind(&archive.id)
                    .bind(horizon)
                    .bind(forecast_score)
                    .bind(forecast_max_score)
                    .bind(ALERT_THRESHOLD)
                    .bind(classification)
                    .bind(lead_time_hours)
                    .bind(MATCHING_RULE_VERSION)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
                }
            }
        }
        sqlx::query(
            "INSERT INTO blue.ground_truth_refreshes(
                started_at,completed_at,satellite_windows_upserted,
                confirmed_ignitions_upserted,comparisons_inserted,rule_version)
             VALUES($1,NOW(),$2,$3,$4,$5)",
        )
        .bind(started_at)
        .bind(i64::try_from(satellite_windows_upserted).unwrap_or(i64::MAX))
        .bind(i64::try_from(confirmed_ignitions_upserted).unwrap_or(i64::MAX))
        .bind(i64::try_from(comparisons_inserted).unwrap_or(i64::MAX))
        .bind(MATCHING_RULE_VERSION)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(BlueGroundTruthRefresh {
            satellite_windows_upserted,
            confirmed_ignitions_upserted,
            comparisons_inserted,
        })
    }

    /// Returns an honest read-only summary of forecast/observation matches.
    ///
    /// # Errors
    ///
    /// Returns an error when summary queries fail.
    pub async fn blue_ground_truth_summary(&self) -> Result<BlueGroundTruthSummary, StoreError> {
        let counts: GroundTruthCounts = sqlx::query_as(
            "SELECT
                (SELECT COUNT(*) FROM blue.ground_truth_observations) observation_count,
                (SELECT COUNT(*) FROM blue.ground_truth_observations WHERE evidence_class='satellite_signal') satellite_signal_windows,
                (SELECT COUNT(*) FROM blue.ground_truth_observations WHERE evidence_class='confirmed_ignition') confirmed_ignitions,
                COUNT(*) forecast_comparisons,
                COUNT(*) FILTER (WHERE classification='signal_covered') signal_covered,
                COUNT(*) FILTER (WHERE classification='signal_below_threshold') signal_below_threshold,
                COUNT(*) FILTER (WHERE classification='confirmed_hit') confirmed_hits,
                COUNT(*) FILTER (WHERE classification='confirmed_miss') confirmed_misses,
                COUNT(*) FILTER (WHERE classification='signal_covered')::double precision
                    / NULLIF(COUNT(*) FILTER (WHERE classification IN
                        ('signal_covered','signal_below_threshold')),0)::double precision signal_coverage_rate,
                COUNT(*) FILTER (WHERE classification='confirmed_hit')::double precision
                    / NULLIF(COUNT(*) FILTER (WHERE classification IN
                        ('confirmed_hit','confirmed_miss')),0)::double precision confirmed_recall
             FROM blue.ground_truth_matches",
        )
        .fetch_one(&self.pool)
        .await?;
        let recent_matches = sqlx::query_as(
            "SELECT m.bulletin_id::text,b.bulletin_date,o.insee_code,o.commune_name,
                o.department_code,o.occurred_at,o.observed_until,o.evidence_class,
                o.signal_count,o.max_frp,m.horizon,m.forecast_score,m.forecast_max_score,
                m.classification,m.lead_time_hours
             FROM blue.ground_truth_matches m
             JOIN blue.ground_truth_observations o ON o.id=m.observation_id
             JOIN blue.forecast_bulletins b ON b.id=m.bulletin_id
             ORDER BY o.occurred_at DESC,m.horizon LIMIT 200",
        )
        .fetch_all(&self.pool)
        .await?;
        let last_refresh_at: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT MAX(completed_at) FROM blue.ground_truth_refreshes")
                .fetch_one(&self.pool)
                .await?;
        Ok(BlueGroundTruthSummary {
            generated_at: Utc::now(),
            last_refresh_at,
            observation_count: counts.observation_count,
            satellite_signal_windows: counts.satellite_signal_windows,
            confirmed_ignitions: counts.confirmed_ignitions,
            forecast_comparisons: counts.forecast_comparisons,
            signal_covered: counts.signal_covered,
            signal_below_threshold: counts.signal_below_threshold,
            confirmed_hits: counts.confirmed_hits,
            confirmed_misses: counts.confirmed_misses,
            signal_coverage_rate: counts.signal_coverage_rate,
            confirmed_recall: counts.confirmed_recall,
            recent_matches,
            interpretation: "Les signaux satellitaires indiquent une chaleur détectée, pas un incendie confirmé. Seuls les événements confirmés peuvent mesurer le rappel scientifique.",
            limitations: vec![
                "Une absence de signal ne prouve jamais une absence d'incendie.",
                "Les signaux satellitaires peuvent inclure des sources de chaleur industrielles ou agricoles.",
                "La précision, la spécificité et les faux positifs exigent une vérité territoriale exhaustive encore indisponible.",
            ],
        })
    }
}

fn validate_archive(archive: &ForecastArchive) -> Result<(), StoreError> {
    let expected = archive.commune_codes.len().saturating_mul(4);
    if archive.p95_24h.len() != expected
        || archive.max_24h.len() != expected
        || archive.p95_48h.len() != expected
        || archive.max_48h.len() != expected
    {
        return Err(StoreError::SnapshotContract(format!(
            "BLUE forecast archive {} has inconsistent byte lengths",
            archive.id
        )));
    }
    Ok(())
}

fn decode_float(bytes: &[u8], position: usize) -> Result<f32, StoreError> {
    let start = position.saturating_mul(4);
    let value = bytes.get(start..start + 4).ok_or_else(|| {
        StoreError::SnapshotContract("BLUE forecast archive offset is invalid".to_owned())
    })?;
    let array: [u8; 4] = value.try_into().map_err(|_| {
        StoreError::SnapshotContract("BLUE forecast archive value is truncated".to_owned())
    })?;
    let decoded = f32::from_be_bytes(array);
    if !decoded.is_finite() || !(0.0..=1.0).contains(&decoded) {
        return Err(StoreError::SnapshotContract(
            "BLUE forecast archive contains an invalid score".to_owned(),
        ));
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::decode_float;

    #[test]
    fn decodes_network_order_forecast_scores() {
        let bytes = [0.25_f32.to_be_bytes(), 0.75_f32.to_be_bytes()].concat();
        assert!((decode_float(&bytes, 0).expect("first score") - 0.25).abs() < f32::EPSILON);
        assert!((decode_float(&bytes, 1).expect("second score") - 0.75).abs() < f32::EPSILON);
        assert!(decode_float(&bytes, 2).is_err());
    }
}
