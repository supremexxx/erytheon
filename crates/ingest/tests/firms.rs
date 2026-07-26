use std::{path::Path, time::Duration};

use chrono::NaiveDate;
use grid::{BoundingBox, H3Grid, Resolution};
use ingest::{Cadence, FetchCtx, ObservationKind, Source, firms::FirmsSource};

#[tokio::test]
async fn loads_and_projects_the_official_fixture() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/firms_viirs_snpp.csv");
    let source = FirmsSource::new(fixture);
    let context = FetchCtx {
        client: reqwest::Client::new(),
        aoi: BoundingBox::new(4.8, 43.3, 5.0, 43.6).expect("valid fixture bbox"),
        grid: H3Grid::new(9).expect("valid resolution"),
        days: 7,
        end_date: NaiveDate::from_ymd_opt(2023, 7, 12).expect("valid date"),
        firms_map_key: None,
        meteofrance_api_key: None,
    };

    let observations = source.fetch(&context).await.expect("fixture should load");
    let batch = source
        .fetch_batch(&context)
        .await
        .expect("batch fixture should load");

    assert_eq!(source.id(), "firms");
    assert_eq!(source.cadence(), Cadence::Poll(Duration::from_mins(30)));
    assert_eq!(observations.len(), 5);
    assert_eq!(batch.received(), 5);
    assert_eq!(batch.accepted(), 5);
    assert_eq!(batch.rejected(), 0);
    let batch_observations = batch.observations();
    assert_eq!(observations.len(), batch_observations.len());
    for (legacy, traced) in observations.iter().zip(&batch_observations) {
        assert_eq!(legacy.source, traced.source);
        assert_eq!(legacy.kind, traced.kind);
        assert_eq!(legacy.cell, traced.cell);
        assert_eq!(legacy.observed_at, traced.observed_at);
        assert_eq!(legacy.payload, traced.payload);
        assert_eq!(legacy.dedupe_key, traced.dedupe_key);
    }
    assert!(
        observations
            .iter()
            .all(|observation| observation.kind == ObservationKind::ActiveFire)
    );
    assert!(
        observations
            .iter()
            .all(|observation| observation.cell.resolution() == Resolution::Nine)
    );
    assert_eq!(
        observations[0].observed_at.to_rfc3339(),
        "2023-07-12T01:34:00+00:00"
    );
    assert_eq!(observations[0].payload["satellite"], "N");
}
