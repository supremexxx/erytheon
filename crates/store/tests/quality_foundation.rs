use std::collections::HashMap;

use chrono::Utc;
use grid::H3Grid;
use ingest::bdiff::read_file;
use quality::{COMBUSTIBILITY_RULE_ID, DUPLICATE_RULE_ID, GEOGRAPHIC_RULE_ID, LABEL_RULE_ID};
use serde_json::json;
use sqlx::PgPool;
use store::{
    BdiffImportStart, CombustibilityAssessmentRecord, CoordinateGroupRecord,
    GeographicAssessmentRecord, LabelAssessmentRecord, QualityPersistenceBundle,
    QualityRuleVersion, Store,
};

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn quality_foundation_is_versioned_idempotent_and_non_destructive() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url).await.expect("migrations");
    let pool = PgPool::connect(&database_url).await.expect("pool");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/bdiff_pipeline_fixture.csv");
    let document = read_file(&fixture, H3Grid::new(8).expect("grid"))
        .await
        .expect("fixture");
    let import = store
        .begin_bdiff_import(&BdiffImportStart {
            batch_type: "quality_foundation_test".to_owned(),
            trigger_type: "test".to_owned(),
            parameters: json!({"fixture": "bdiff_pipeline_fixture.csv"}),
            pipeline_version: "test".to_owned(),
            code_version: Some("test".to_owned()),
        })
        .await
        .expect("import start");
    store
        .persist_bdiff_import(&import, &document.rows, Utc::now(), 8)
        .await
        .expect("fixture import");
    let before_fire = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fire.ignition_events")
        .fetch_one(&pool)
        .await
        .expect("before fire");
    let before_public =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM public.ignition_history")
            .fetch_one(&pool)
            .await
            .expect("before public");
    let source = store
        .quality_source_events(None, Some("fixture-malicious"))
        .await
        .expect("source event");
    let event = source.first().expect("fixture event");
    let mut rule_ids = HashMap::new();
    for (logical_id, rule_type) in [
        (LABEL_RULE_ID, "label_quality"),
        (GEOGRAPHIC_RULE_ID, "geographic_quality"),
        (COMBUSTIBILITY_RULE_ID, "combustibility"),
        (DUPLICATE_RULE_ID, "duplicate_detection"),
    ] {
        let rule = QualityRuleVersion {
            logical_id: format!("{logical_id}_integration"),
            rule_type: rule_type.to_owned(),
            description: "integration test".to_owned(),
            parameters: json!({"fixture": true}),
            code_version: "test".to_owned(),
            status: "draft".to_owned(),
            checksum: format!("{logical_id}_integration_checksum"),
            notes: None,
        };
        let first = store.ensure_quality_rule(&rule).await.expect("first rule");
        let second = store.ensure_quality_rule(&rule).await.expect("second rule");
        assert_eq!(first, second);
        rule_ids.insert(logical_id.to_owned(), first);
    }
    let coordinate_checksum = "coordinate_fixture_checksum".to_owned();
    let bundle = QualityPersistenceBundle {
        coordinates: vec![CoordinateGroupRecord {
            latitude: event.latitude,
            longitude: event.longitude,
            event_count: event.coordinate_event_count,
            municipality_count: event.coordinate_municipality_count,
            year_count: event.coordinate_year_count,
            decimal_precision: 5,
            repeated_coordinate: event.coordinate_event_count > 1,
            rounded_coordinate_probable: false,
            centroid_status: "undetermined".to_owned(),
            signals: json!({"fixture": true}),
            logical_checksum: coordinate_checksum.clone(),
        }],
        labels: vec![LabelAssessmentRecord {
            event_id: event.id.clone(),
            taxonomy_version: event.taxonomy_version.clone(),
            cause_category: event.cause_category.clone(),
            cause_subcategory: event.cause_subcategory.clone(),
            confidence: "high".to_owned(),
            proposed_eligibility: "eligible_human_positive".to_owned(),
            requires_accidental_sensitivity_analysis: false,
            reasons: json!(["fixture"]),
            logical_checksum: "label_fixture_checksum".to_owned(),
        }],
        geography: vec![GeographicAssessmentRecord {
            event_id: event.id.clone(),
            coordinate_group_checksum: coordinate_checksum,
            latitude: event.latitude,
            longitude: event.longitude,
            h3: event.h3,
            h3_resolution: event.h3_resolution,
            municipality: event.municipality.clone(),
            shared_event_count: event.coordinate_event_count,
            shared_municipality_count: event.coordinate_municipality_count,
            decimal_precision: 5,
            rounded_coordinate_probable: false,
            centroid_status: "undetermined".to_owned(),
            category: "precision_undocumented".to_owned(),
            confidence: 0.5,
            reasons: json!(["fixture"]),
            logical_checksum: "geography_fixture_checksum".to_owned(),
        }],
        combustibility: vec![CombustibilityAssessmentRecord {
            event_id: event.id.clone(),
            h3: event.h3,
            h3_resolution: event.h3_resolution,
            cell_features_present: false,
            original_cell_combustible: None,
            feature_snapshot_at: None,
            nearest_combustible_h3: None,
            nearest_combustible_ring: None,
            nearest_combustible_distance_m: None,
            combustible_ring1_count: 0,
            combustible_ring2_count: 0,
            status_flags: json!(["missing_cell_features", "requires_review"]),
            territorial_signals: json!({}),
            reasons: json!(["fixture"]),
            logical_checksum: "combustibility_fixture_checksum".to_owned(),
            candidates: Vec::new(),
        }],
        ..QualityPersistenceBundle::default()
    };
    store
        .persist_quality_bundle(&rule_ids, &bundle)
        .await
        .expect("first persistence");
    store
        .persist_quality_bundle(&rule_ids, &bundle)
        .await
        .expect("idempotent persistence");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM validation.event_label_quality
             WHERE ignition_event_id = $1::uuid",
        )
        .bind(&event.id)
        .fetch_one(&pool)
        .await
        .expect("label count"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fire.ignition_events")
            .fetch_one(&pool)
            .await
            .expect("after fire"),
        before_fire
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM public.ignition_history")
            .fetch_one(&pool)
            .await
            .expect("after public"),
        before_public
    );
    cleanup(&pool, &event.id, &import.batch_id).await;
}

async fn cleanup(pool: &PgPool, event_id: &str, batch_id: &str) {
    sqlx::query(
        "DELETE FROM validation.event_combustibility_assessments WHERE ignition_event_id=$1::uuid",
    )
    .bind(event_id)
    .execute(pool)
    .await
    .expect("combustibility cleanup");
    sqlx::query("DELETE FROM validation.event_geographic_quality WHERE ignition_event_id=$1::uuid")
        .bind(event_id)
        .execute(pool)
        .await
        .expect("geography cleanup");
    sqlx::query("DELETE FROM validation.event_label_quality WHERE ignition_event_id=$1::uuid")
        .bind(event_id)
        .execute(pool)
        .await
        .expect("label cleanup");
    sqlx::query(
        "DELETE FROM validation.coordinate_groups WHERE rule_version_id IN (
        SELECT id FROM validation.rule_versions WHERE logical_id LIKE '%_integration')",
    )
    .execute(pool)
    .await
    .expect("coordinate cleanup");
    sqlx::query("DELETE FROM validation.rule_versions WHERE logical_id LIKE '%_integration'")
        .execute(pool)
        .await
        .expect("rule cleanup");
    sqlx::query(
        "DELETE FROM fire.ignition_events WHERE staging_event_id IN (
        SELECT staging.id FROM staging.bdiff_events_normalized staging
        JOIN raw.bdiff_records raw ON raw.id=staging.raw_record_id
        WHERE raw.import_batch_id=$1::uuid)",
    )
    .bind(batch_id)
    .execute(pool)
    .await
    .expect("fire cleanup");
    sqlx::query(
        "DELETE FROM staging.bdiff_events_normalized WHERE raw_record_id IN (
        SELECT id FROM raw.bdiff_records WHERE import_batch_id=$1::uuid)",
    )
    .bind(batch_id)
    .execute(pool)
    .await
    .expect("staging cleanup");
    sqlx::query("DELETE FROM raw.bdiff_records WHERE import_batch_id=$1::uuid")
        .bind(batch_id)
        .execute(pool)
        .await
        .expect("raw cleanup");
    sqlx::query("DELETE FROM ops.pipeline_runs WHERE import_batch_id=$1::uuid")
        .bind(batch_id)
        .execute(pool)
        .await
        .expect("run cleanup");
    sqlx::query("DELETE FROM ops.import_batches WHERE id=$1::uuid")
        .bind(batch_id)
        .execute(pool)
        .await
        .expect("batch cleanup");
}
