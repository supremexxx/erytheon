use geo::Geometry;
use store::Store;

#[tokio::test]
async fn commune_boundary_round_trips_through_geojson() {
    dotenvy::dotenv().ok();
    let Ok(database_url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping database integration test: DATABASE_URL is not configured");
        return;
    };
    let store = Store::connect(&database_url)
        .await
        .expect("database should accept migrations");

    let insee_code = "00001";
    let geometry: serde_json::Value = serde_json::from_str(
        r#"{"type":"Polygon","coordinates":[[[1.35,43.75],[1.40,43.75],[1.40,43.80],[1.35,43.80],[1.35,43.75]]]}"#,
    )
    .expect("valid geometry fixture");

    store
        .upsert_commune_boundary(insee_code, "Testville", &["00000".to_owned()], &geometry)
        .await
        .expect("insert commune boundary");

    let boundary = store
        .commune_boundary(insee_code)
        .await
        .expect("lookup should succeed")
        .expect("boundary should exist after insert");
    assert_eq!(boundary.insee_code, insee_code);
    assert_eq!(boundary.name, "Testville");
    assert_eq!(boundary.postal_codes, vec!["00000".to_owned()]);
    assert!(matches!(boundary.geometry, Geometry::Polygon(_)));
    assert!((boundary.bbox.west - 1.35).abs() < 1e-9);
    assert!((boundary.bbox.east - 1.40).abs() < 1e-9);

    // Upsert must replace, not duplicate.
    store
        .upsert_commune_boundary(insee_code, "Testville renamed", &[], &geometry)
        .await
        .expect("update commune boundary");
    let updated = store
        .commune_boundary(insee_code)
        .await
        .expect("lookup should succeed")
        .expect("boundary should still exist");
    assert_eq!(updated.name, "Testville renamed");
    assert!(updated.postal_codes.is_empty());

    let missing = store
        .commune_boundary("99999")
        .await
        .expect("lookup should succeed");
    assert!(missing.is_none());
}
