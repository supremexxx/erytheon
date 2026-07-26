use std::collections::HashMap;

use chrono::{DateTime, Utc};
use quality::QualityEvent;
use serde_json::Value;
use sqlx::FromRow;

use crate::{Store, StoreError};

#[derive(Clone, Debug, FromRow)]
pub struct QualitySourceEvent {
    pub id: String,
    pub source_record_id: String,
    pub occurred_at: DateTime<Utc>,
    pub municipality: String,
    pub latitude: f64,
    pub longitude: f64,
    pub h3: i64,
    pub h3_resolution: i16,
    pub surface_ha: f64,
    pub cause_source: String,
    pub cause_category: String,
    pub cause_subcategory: String,
    pub taxonomy_version: String,
    pub coordinate_event_count: i64,
    pub coordinate_municipality_count: i64,
    pub coordinate_year_count: i64,
}

impl TryFrom<&QualitySourceEvent> for QualityEvent {
    type Error = StoreError;

    fn try_from(value: &QualitySourceEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.clone(),
            source_record_id: value.source_record_id.clone(),
            occurred_at: value.occurred_at,
            municipality: value.municipality.clone(),
            latitude: value.latitude,
            longitude: value.longitude,
            h3: value.h3,
            h3_resolution: u8::try_from(value.h3_resolution)
                .map_err(|_| StoreError::InvalidH3Resolution(value.h3_resolution))?,
            surface_ha: value.surface_ha,
            cause_source: value.cause_source.clone(),
            cause_category: value.cause_category.clone(),
            cause_subcategory: value.cause_subcategory.clone(),
            taxonomy_version: value.taxonomy_version.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct QualityRuleVersion {
    pub logical_id: String,
    pub rule_type: String,
    pub description: String,
    pub parameters: Value,
    pub code_version: String,
    pub status: String,
    pub checksum: String,
    pub notes: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CoordinateGroupRecord {
    pub latitude: f64,
    pub longitude: f64,
    pub event_count: i64,
    pub municipality_count: i64,
    pub year_count: i64,
    pub decimal_precision: i16,
    pub repeated_coordinate: bool,
    pub rounded_coordinate_probable: bool,
    pub centroid_status: String,
    pub signals: Value,
    pub logical_checksum: String,
}

#[derive(Clone, Debug)]
pub struct LabelAssessmentRecord {
    pub event_id: String,
    pub taxonomy_version: String,
    pub cause_category: String,
    pub cause_subcategory: String,
    pub confidence: String,
    pub proposed_eligibility: String,
    pub requires_accidental_sensitivity_analysis: bool,
    pub reasons: Value,
    pub logical_checksum: String,
}

#[derive(Clone, Debug)]
pub struct GeographicAssessmentRecord {
    pub event_id: String,
    pub coordinate_group_checksum: String,
    pub latitude: f64,
    pub longitude: f64,
    pub h3: i64,
    pub h3_resolution: i16,
    pub municipality: String,
    pub shared_event_count: i64,
    pub shared_municipality_count: i64,
    pub decimal_precision: i16,
    pub rounded_coordinate_probable: bool,
    pub centroid_status: String,
    pub category: String,
    pub confidence: f64,
    pub reasons: Value,
    pub logical_checksum: String,
}

#[derive(Clone, Debug)]
pub struct CombustibilityAssessmentRecord {
    pub event_id: String,
    pub h3: i64,
    pub h3_resolution: i16,
    pub cell_features_present: bool,
    pub original_cell_combustible: Option<bool>,
    pub feature_snapshot_at: Option<DateTime<Utc>>,
    pub nearest_combustible_h3: Option<i64>,
    pub nearest_combustible_ring: Option<i16>,
    pub nearest_combustible_distance_m: Option<f64>,
    pub combustible_ring1_count: i32,
    pub combustible_ring2_count: i32,
    pub status_flags: Value,
    pub territorial_signals: Value,
    pub reasons: Value,
    pub logical_checksum: String,
    pub candidates: Vec<CombustibleCandidateRecord>,
}

#[derive(Clone, Debug)]
pub struct CombustibleCandidateRecord {
    pub h3: i64,
    pub ring: i16,
    pub rank: i16,
    pub center_distance_m: f64,
    pub features: Value,
    pub score: f64,
    pub justification: Value,
}

#[derive(Clone, Debug)]
pub struct DuplicatePairRecord {
    pub left_event_id: String,
    pub right_event_id: String,
    pub score: f64,
    pub classification: String,
    pub raw_signals: Value,
    pub contributions: Value,
    pub justification: String,
    pub logical_checksum: String,
}

#[derive(Clone, Debug)]
pub struct DuplicateGroupRecord {
    pub stable_key: String,
    pub score: f64,
    pub classification: String,
    pub principal_signals: Value,
    pub proposed_decision: String,
    pub justification: String,
    pub logical_checksum: String,
    pub members: Vec<DuplicateMemberRecord>,
}

#[derive(Clone, Debug)]
pub struct DuplicateMemberRecord {
    pub event_id: String,
    pub role: String,
    pub individual_score: f64,
    pub pair_checksums: Value,
    pub justification: String,
}

#[derive(Clone, Debug, Default)]
pub struct QualityPersistenceBundle {
    pub coordinates: Vec<CoordinateGroupRecord>,
    pub labels: Vec<LabelAssessmentRecord>,
    pub geography: Vec<GeographicAssessmentRecord>,
    pub combustibility: Vec<CombustibilityAssessmentRecord>,
    pub duplicate_pairs: Vec<DuplicatePairRecord>,
    pub duplicate_groups: Vec<DuplicateGroupRecord>,
}

impl Store {
    /// Loads immutable BDIFF events with deterministic coordinate aggregates.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` rejects the read.
    pub async fn quality_source_events(
        &self,
        year: Option<i32>,
        source_record_id: Option<&str>,
    ) -> Result<Vec<QualitySourceEvent>, StoreError> {
        sqlx::query_as::<_, QualitySourceEvent>(
            "WITH coordinate_stats AS (
                SELECT latitude_original, longitude_original,
                       COUNT(*) AS event_count,
                       COUNT(DISTINCT municipality_source) AS municipality_count,
                       COUNT(DISTINCT EXTRACT(YEAR FROM occurred_on_local)) AS year_count
                FROM fire.ignition_events
                GROUP BY latitude_original, longitude_original
             )
             SELECT event.id::text AS id, event.source_record_id,
                    event.occurred_at, event.municipality_source AS municipality,
                    event.latitude_original AS latitude,
                    event.longitude_original AS longitude,
                    event.h3, event.h3_resolution, event.surface_ha,
                    event.cause_source, event.cause_category,
                    event.cause_subcategory, event.taxonomy_version,
                    stats.event_count AS coordinate_event_count,
                    stats.municipality_count AS coordinate_municipality_count,
                    stats.year_count AS coordinate_year_count
             FROM fire.ignition_events AS event
             JOIN coordinate_stats AS stats
               USING (latitude_original, longitude_original)
             WHERE ($1::integer IS NULL OR EXTRACT(YEAR FROM event.occurred_on_local) = $1)
               AND ($2::text IS NULL OR event.source_record_id = $2)
             ORDER BY event.occurred_at, event.id",
        )
        .bind(year)
        .bind(source_record_id)
        .fetch_all(&self.pool)
        .await
        .map_err(StoreError::from)
    }

    /// Loads static feature documents for the requested H3 cells.
    ///
    /// # Errors
    ///
    /// Returns an error when `PostgreSQL` rejects the read.
    pub async fn quality_static_features(
        &self,
        cells: &[i64],
    ) -> Result<HashMap<i64, (Value, DateTime<Utc>)>, StoreError> {
        let rows = sqlx::query_as::<_, (i64, Value, DateTime<Utc>)>(
            "SELECT h3, features, updated_at FROM public.cell_static WHERE h3 = ANY($1)",
        )
        .bind(cells)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(h3, features, updated_at)| (h3, (features, updated_at)))
            .collect())
    }

    /// Creates an immutable logical rule or returns its existing identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the existing checksum differs or persistence fails.
    pub async fn ensure_quality_rule(
        &self,
        rule: &QualityRuleVersion,
    ) -> Result<String, StoreError> {
        let mut transaction = self.pool.begin().await?;
        let existing = sqlx::query_as::<_, (String, String)>(
            "SELECT id::text, checksum FROM validation.rule_versions WHERE logical_id = $1",
        )
        .bind(&rule.logical_id)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some((id, checksum)) = existing {
            if checksum != rule.checksum {
                return Err(StoreError::QualityRuleChanged(rule.logical_id.clone()));
            }
            transaction.commit().await?;
            return Ok(id);
        }
        let id = sqlx::query_scalar::<_, String>(
            "INSERT INTO validation.rule_versions (
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

    /// Persists one complete quality calculation atomically.
    ///
    /// # Errors
    ///
    /// Returns an error when a rule is absent or `PostgreSQL` rejects the transaction.
    pub async fn persist_quality_bundle(
        &self,
        rule_ids: &HashMap<String, String>,
        bundle: &QualityPersistenceBundle,
    ) -> Result<(), StoreError> {
        let label_rule = required_rule(rule_ids, quality::LABEL_RULE_ID)?;
        let geographic_rule = required_rule(rule_ids, quality::GEOGRAPHIC_RULE_ID)?;
        let combustibility_rule = required_rule(rule_ids, quality::COMBUSTIBILITY_RULE_ID)?;
        let duplicate_rule = required_rule(rule_ids, quality::DUPLICATE_RULE_ID)?;
        let mut transaction = self.pool.begin().await?;
        let coordinate_payload = bundle
            .coordinates
            .iter()
            .map(|row| {
                serde_json::json!({
                    "latitude": row.latitude, "longitude": row.longitude,
                    "event_count": row.event_count, "municipality_count": row.municipality_count,
                    "year_count": row.year_count, "decimal_precision": row.decimal_precision,
                    "repeated_coordinate": row.repeated_coordinate,
                    "rounded_coordinate_probable": row.rounded_coordinate_probable,
                    "centroid_status": row.centroid_status, "signals": row.signals,
                    "logical_checksum": row.logical_checksum
                })
            })
            .collect::<Vec<_>>();
        let coordinate_rows = sqlx::query_as::<_, (String, String)>(
            "WITH input AS (
                SELECT * FROM jsonb_to_recordset($2::jsonb) AS value(
                    latitude double precision, longitude double precision,
                    event_count bigint, municipality_count bigint, year_count bigint,
                    decimal_precision smallint, repeated_coordinate boolean,
                    rounded_coordinate_probable boolean, centroid_status text,
                    signals jsonb, logical_checksum text
                )
             )
             INSERT INTO validation.coordinate_groups (
                    rule_version_id, latitude, longitude, event_count,
                    municipality_count, year_count, decimal_precision,
                    repeated_coordinate, rounded_coordinate_probable,
                    centroid_status, signals, logical_checksum
             )
             SELECT $1::uuid, latitude, longitude, event_count, municipality_count,
                    year_count, decimal_precision, repeated_coordinate,
                    rounded_coordinate_probable, centroid_status, signals, logical_checksum
             FROM input
             ON CONFLICT (rule_version_id, latitude, longitude)
             DO UPDATE SET logical_checksum = validation.coordinate_groups.logical_checksum
             RETURNING logical_checksum, id::text",
        )
        .bind(geographic_rule)
        .bind(serde_json::json!(coordinate_payload))
        .fetch_all(&mut *transaction)
        .await?;
        let coordinate_ids = coordinate_rows.into_iter().collect::<HashMap<_, _>>();
        persist_labels(&mut transaction, label_rule, &bundle.labels).await?;
        persist_geography(
            &mut transaction,
            geographic_rule,
            &coordinate_ids,
            &bundle.geography,
        )
        .await?;
        persist_combustibility(
            &mut transaction,
            combustibility_rule,
            &bundle.combustibility,
        )
        .await?;
        let pair_ids =
            persist_duplicate_pairs(&mut transaction, duplicate_rule, &bundle.duplicate_pairs)
                .await?;
        persist_duplicate_groups(
            &mut transaction,
            duplicate_rule,
            &pair_ids,
            &bundle.duplicate_groups,
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}

async fn persist_labels(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rule: &str,
    rows: &[LabelAssessmentRecord],
) -> Result<(), StoreError> {
    let payload = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "event_id": row.event_id, "taxonomy_version": row.taxonomy_version,
                "cause_category": row.cause_category, "cause_subcategory": row.cause_subcategory,
                "confidence": row.confidence, "proposed_eligibility": row.proposed_eligibility,
                "requires_accidental": row.requires_accidental_sensitivity_analysis,
                "reasons": row.reasons, "logical_checksum": row.logical_checksum
            })
        })
        .collect::<Vec<_>>();
    sqlx::query(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($2::jsonb) AS value(
                event_id text, taxonomy_version text, cause_category text,
                cause_subcategory text, confidence text, proposed_eligibility text,
                requires_accidental boolean, reasons jsonb, logical_checksum text
            )
         )
         INSERT INTO validation.event_label_quality (
                ignition_event_id, rule_version_id, taxonomy_version,
                cause_category, cause_subcategory, confidence,
                proposed_eligibility, requires_accidental_sensitivity_analysis,
                reasons, logical_checksum
         )
         SELECT event_id::uuid, $1::uuid, taxonomy_version, cause_category,
                cause_subcategory, confidence, proposed_eligibility,
                requires_accidental, reasons, logical_checksum
         FROM input ON CONFLICT DO NOTHING",
    )
    .bind(rule)
    .bind(serde_json::json!(payload))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_geography(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rule: &str,
    coordinate_ids: &HashMap<String, String>,
    rows: &[GeographicAssessmentRecord],
) -> Result<(), StoreError> {
    let payload = rows
        .iter()
        .map(|row| {
            let coordinate_id = coordinate_ids
                .get(&row.coordinate_group_checksum)
                .ok_or(StoreError::MissingCoordinateGroup)?;
            Ok(serde_json::json!({
                "event_id": row.event_id, "coordinate_id": coordinate_id,
                "latitude": row.latitude, "longitude": row.longitude, "h3": row.h3,
                "h3_resolution": row.h3_resolution, "municipality": row.municipality,
                "shared_event_count": row.shared_event_count,
                "shared_municipality_count": row.shared_municipality_count,
                "decimal_precision": row.decimal_precision,
                "rounded": row.rounded_coordinate_probable,
                "centroid_status": row.centroid_status, "category": row.category,
                "confidence": row.confidence, "reasons": row.reasons,
                "logical_checksum": row.logical_checksum
            }))
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    sqlx::query(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($2::jsonb) AS value(
                event_id text, coordinate_id text, latitude double precision,
                longitude double precision, h3 bigint, h3_resolution smallint,
                municipality text, shared_event_count bigint,
                shared_municipality_count bigint, decimal_precision smallint,
                rounded boolean, centroid_status text, category text,
                confidence double precision, reasons jsonb, logical_checksum text
            )
         )
         INSERT INTO validation.event_geographic_quality (
                ignition_event_id, rule_version_id, coordinate_group_id,
                latitude_original, longitude_original, geom_original,
                h3_original, h3_resolution, municipality_source,
                shared_coordinate_event_count, shared_coordinate_municipality_count,
                decimal_precision, rounded_coordinate_probable, centroid_status,
                geographic_category, confidence, reasons, logical_checksum
         )
         SELECT event_id::uuid, $1::uuid, coordinate_id::uuid, latitude, longitude,
                ST_SetSRID(ST_MakePoint(longitude,latitude),4326), h3, h3_resolution,
                municipality, shared_event_count, shared_municipality_count,
                decimal_precision, rounded, centroid_status, category, confidence,
                reasons, logical_checksum
         FROM input ON CONFLICT DO NOTHING",
    )
    .bind(rule)
    .bind(serde_json::json!(payload))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_combustibility(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rule: &str,
    rows: &[CombustibilityAssessmentRecord],
) -> Result<(), StoreError> {
    let payload = rows.iter().map(|row| serde_json::json!({
        "event_id": row.event_id, "h3": row.h3, "h3_resolution": row.h3_resolution,
        "cell_features_present": row.cell_features_present,
        "original_cell_combustible": row.original_cell_combustible,
        "feature_snapshot_at": row.feature_snapshot_at,
        "nearest_h3": row.nearest_combustible_h3, "nearest_ring": row.nearest_combustible_ring,
        "nearest_distance": row.nearest_combustible_distance_m,
        "ring1_count": row.combustible_ring1_count, "ring2_count": row.combustible_ring2_count,
        "status_flags": row.status_flags, "territorial_signals": row.territorial_signals,
        "reasons": row.reasons, "logical_checksum": row.logical_checksum
    })).collect::<Vec<_>>();
    sqlx::query(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($2::jsonb) AS value(
                event_id text, h3 bigint, h3_resolution smallint,
                cell_features_present boolean, original_cell_combustible boolean,
                feature_snapshot_at timestamptz, nearest_h3 bigint, nearest_ring smallint,
                nearest_distance double precision, ring1_count integer, ring2_count integer,
                status_flags jsonb, territorial_signals jsonb, reasons jsonb,
                logical_checksum text
            )
         )
         INSERT INTO validation.event_combustibility_assessments (
                ignition_event_id, rule_version_id, h3_original, h3_resolution,
                cell_features_present, original_cell_combustible, feature_snapshot_at,
                nearest_combustible_h3, nearest_combustible_ring,
                nearest_combustible_distance_m, combustible_ring1_count,
                combustible_ring2_count, status_flags, territorial_signals,
                reasons, logical_checksum
         )
         SELECT event_id::uuid,$1::uuid,h3,h3_resolution,cell_features_present,
                original_cell_combustible,feature_snapshot_at,nearest_h3,nearest_ring,
                nearest_distance,ring1_count,ring2_count,status_flags,territorial_signals,
                reasons,logical_checksum
         FROM input ON CONFLICT DO NOTHING",
    )
    .bind(rule)
    .bind(serde_json::json!(payload))
    .execute(&mut **transaction)
    .await?;
    let candidates = rows
        .iter()
        .flat_map(|row| {
            row.candidates.iter().map(|candidate| {
                serde_json::json!({
                    "event_id": row.event_id, "h3": candidate.h3, "ring": candidate.ring,
                    "rank": candidate.rank, "distance": candidate.center_distance_m,
                    "features": candidate.features, "score": candidate.score,
                    "justification": candidate.justification
                })
            })
        })
        .collect::<Vec<_>>();
    sqlx::query(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($2::jsonb) AS value(
                event_id text,h3 bigint,ring smallint,rank smallint,
                distance double precision,features jsonb,score double precision,
                justification jsonb
            )
         )
         INSERT INTO validation.combustible_cell_candidates (
                    ignition_event_id, rule_version_id, candidate_h3, h3_ring,
                    rank, center_distance_m, features, score, justification
         )
         SELECT event_id::uuid,$1::uuid,h3,ring,rank,distance,features,score,justification
         FROM input ON CONFLICT DO NOTHING",
    )
    .bind(rule)
    .bind(serde_json::json!(candidates))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn persist_duplicate_pairs(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rule: &str,
    rows: &[DuplicatePairRecord],
) -> Result<HashMap<String, String>, StoreError> {
    let payload = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "left_id": row.left_event_id, "right_id": row.right_event_id,
                "score": row.score, "classification": row.classification,
                "raw_signals": row.raw_signals, "contributions": row.contributions,
                "justification": row.justification, "logical_checksum": row.logical_checksum
            })
        })
        .collect::<Vec<_>>();
    let ids = sqlx::query_as::<_, (String, String)>(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($2::jsonb) AS value(
                left_id text,right_id text,score double precision,classification text,
                raw_signals jsonb,contributions jsonb,justification text,logical_checksum text
            )
         )
         INSERT INTO validation.duplicate_candidate_pairs (
                rule_version_id, left_event_id, right_event_id, score,
                classification, raw_signals, contributions, justification,
                logical_checksum
         )
         SELECT $1::uuid,left_id::uuid,right_id::uuid,score,classification,
                raw_signals,contributions,justification,logical_checksum
         FROM input
         ON CONFLICT (rule_version_id,left_event_id,right_event_id)
         DO UPDATE SET logical_checksum=validation.duplicate_candidate_pairs.logical_checksum
         RETURNING logical_checksum,id::text",
    )
    .bind(rule)
    .bind(serde_json::json!(payload))
    .fetch_all(&mut **transaction)
    .await?;
    Ok(ids.into_iter().collect())
}

async fn persist_duplicate_groups(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rule: &str,
    pair_ids: &HashMap<String, String>,
    rows: &[DuplicateGroupRecord],
) -> Result<(), StoreError> {
    let payload = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "stable_key": row.stable_key, "score": row.score,
                "classification": row.classification, "member_count": row.members.len(),
                "principal_signals": row.principal_signals,
                "proposed_decision": row.proposed_decision,
                "justification": row.justification, "logical_checksum": row.logical_checksum
            })
        })
        .collect::<Vec<_>>();
    let group_rows = sqlx::query_as::<_, (String, String)>(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($2::jsonb) AS value(
                stable_key text,score double precision,classification text,
                member_count integer,principal_signals jsonb,proposed_decision text,
                justification text,logical_checksum text
            )
         )
         INSERT INTO validation.duplicate_candidate_groups (
                rule_version_id, stable_key, score, classification,
                member_count, principal_signals, proposed_decision,
                justification, logical_checksum
         )
         SELECT $1::uuid,stable_key,score,classification,member_count,
                principal_signals,proposed_decision,justification,logical_checksum
         FROM input
         ON CONFLICT (rule_version_id,stable_key)
         DO UPDATE SET logical_checksum=validation.duplicate_candidate_groups.logical_checksum
         RETURNING stable_key,id::text",
    )
    .bind(rule)
    .bind(serde_json::json!(payload))
    .fetch_all(&mut **transaction)
    .await?;
    let group_ids = group_rows.into_iter().collect::<HashMap<_, _>>();
    let members = rows
        .iter()
        .flat_map(|row| {
            row.members.iter().map(|member| {
                let resolved = member
                    .pair_checksums
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter_map(|checksum| pair_ids.get(checksum))
                    .cloned()
                    .collect::<Vec<_>>();
                serde_json::json!({
                    "group_id": group_ids[&row.stable_key], "event_id": member.event_id,
                    "role": member.role, "score": member.individual_score,
                    "pair_ids": resolved, "justification": member.justification
                })
            })
        })
        .collect::<Vec<_>>();
    sqlx::query(
        "WITH input AS (
            SELECT * FROM jsonb_to_recordset($1::jsonb) AS value(
                group_id text,event_id text,role text,score double precision,
                pair_ids jsonb,justification text
            )
         )
         INSERT INTO validation.duplicate_candidate_members (
                    group_id, ignition_event_id, role, individual_score,
                    pair_ids, justification
         )
         SELECT group_id::uuid,event_id::uuid,role,score,pair_ids,justification
         FROM input ON CONFLICT DO NOTHING",
    )
    .bind(serde_json::json!(members))
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn required_rule<'a>(
    ids: &'a HashMap<String, String>,
    logical_id: &str,
) -> Result<&'a str, StoreError> {
    ids.get(logical_id)
        .map(String::as_str)
        .ok_or_else(|| StoreError::MissingQualityRule(logical_id.to_owned()))
}
