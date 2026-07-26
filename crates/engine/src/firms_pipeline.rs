use std::time::Instant;

use chrono::{DateTime, Days, Utc};
use ingest::{FetchCtx, SourceError, firms::FirmsSource};
use store::{
    FirmsImportIds, FirmsImportStart, FirmsPersistenceResult, FirmsTerminalState, Store, StoreError,
};

const PIPELINE_VERSION: &str = "v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirmsTrigger {
    Scheduler,
    Backfill,
}

impl FirmsTrigger {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Scheduler => "scheduler",
            Self::Backfill => "backfill",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FirmsImportResult {
    pub ids: FirmsImportIds,
    pub persistence: FirmsPersistenceResult,
    pub status: FirmsTerminalState,
    pub elapsed_seconds: f64,
    pub observations: Vec<ingest::Observation>,
}

/// Executes one traced FIRMS retrieval and atomic raw/public persistence.
///
/// # Errors
///
/// Returns an error after attempting to finalize known failures.
pub async fn run(
    store: &Store,
    source: &FirmsSource,
    context: &FetchCtx,
    trigger: FirmsTrigger,
) -> Result<FirmsImportResult, FirmsPipelineError> {
    let started = Instant::now();
    let requested_to = utc_midnight(context.end_date);
    let requested_from = requested_to
        .checked_sub_days(Days::new(u64::from(context.days.saturating_sub(1))))
        .unwrap_or(requested_to);
    let parameters = batch_parameters(context);
    let ids = store
        .begin_firms_import(&FirmsImportStart {
            batch_type: format!("firms_{}", trigger.as_str()),
            trigger_type: trigger.as_str().to_owned(),
            requested_from,
            requested_to,
            parameters,
            pipeline_version: PIPELINE_VERSION.to_owned(),
            code_version: option_env!("GIT_COMMIT_SHA").map(str::to_owned),
        })
        .await
        .map_err(FirmsPipelineError::Begin)?;
    tracing::info!(
        source_code = "nasa_firms",
        import_batch_id = %ids.batch_id,
        pipeline_run_id = %ids.pipeline_run_id,
        trigger_type = trigger.as_str(),
        days = context.days,
        end_date = %context.end_date,
        "FIRMS import started"
    );

    let fetch = match source.fetch_batch(context).await {
        Ok(fetch) => fetch,
        Err(error) => {
            let safe_error = safe_source_error(&error);
            finalize_failure(store, &ids, safe_error).await?;
            tracing::error!(
                source_code = "nasa_firms",
                import_batch_id = %ids.batch_id,
                pipeline_run_id = %ids.pipeline_run_id,
                trigger_type = trigger.as_str(),
                status = "failed",
                error = safe_error,
                "FIRMS import failed"
            );
            return Err(FirmsPipelineError::Fetch(error));
        }
    };
    let retrieved_at = Utc::now();
    let persistence = match store
        .persist_firms_import(&ids, &fetch.rows, retrieved_at)
        .await
    {
        Ok(result) => result,
        Err(error) => {
            let safe_error = "FIRMS PostgreSQL persistence failed";
            finalize_failure(store, &ids, safe_error).await?;
            tracing::error!(
                source_code = "nasa_firms",
                import_batch_id = %ids.batch_id,
                pipeline_run_id = %ids.pipeline_run_id,
                trigger_type = trigger.as_str(),
                status = "failed",
                error = safe_error,
                "FIRMS import failed"
            );
            return Err(FirmsPipelineError::Persist(error));
        }
    };
    let status = terminal_state(persistence);
    let error_message =
        (persistence.rejected > 0).then_some("One or more FIRMS rows could not be normalized");
    store
        .finish_firms_import(&ids, status, persistence, error_message)
        .await
        .map_err(FirmsPipelineError::Finalize)?;
    let elapsed_seconds = started.elapsed().as_secs_f64();
    tracing::info!(
        source_code = "nasa_firms",
        import_batch_id = %ids.batch_id,
        pipeline_run_id = %ids.pipeline_run_id,
        trigger_type = trigger.as_str(),
        received = persistence.received,
        raw_inserted = persistence.raw_inserted,
        public_inserted = persistence.public_inserted,
        duplicates_ignored = persistence.duplicates_ignored,
        rejected = persistence.rejected,
        status = status.as_str(),
        elapsed_seconds,
        "FIRMS import complete"
    );
    Ok(FirmsImportResult {
        ids,
        persistence,
        status,
        elapsed_seconds,
        observations: fetch.observations(),
    })
}

fn batch_parameters(context: &FetchCtx) -> serde_json::Value {
    serde_json::json!({
        "aoi_bbox": {
            "west": context.aoi.west,
            "south": context.aoi.south,
            "east": context.aoi.east,
            "north": context.aoi.north,
        },
        "days": context.days,
        "end_date": context.end_date,
        "product": "VIIRS_SNPP_NRT",
    })
}

async fn finalize_failure(
    store: &Store,
    ids: &FirmsImportIds,
    safe_error: &str,
) -> Result<(), FirmsPipelineError> {
    store
        .finish_firms_import(
            ids,
            FirmsTerminalState::Failed,
            FirmsPersistenceResult::default(),
            Some(safe_error),
        )
        .await
        .map_err(FirmsPipelineError::Finalize)
}

fn utc_midnight(date: chrono::NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_utc()
}

fn safe_source_error(error: &SourceError) -> &'static str {
    match error {
        SourceError::Http(_) => "NASA FIRMS HTTP request failed",
        SourceError::FixtureRead { .. } => "NASA FIRMS fixture could not be read",
        SourceError::Csv(_) => "NASA FIRMS CSV could not be decoded",
        SourceError::InvalidTimestamp { .. } => "NASA FIRMS timestamp was invalid",
        SourceError::Grid(_) => "NASA FIRMS coordinate could not be projected",
        SourceError::Json(_) => "NASA FIRMS payload could not be serialized",
        SourceError::InvalidDayCount => "NASA FIRMS day count was invalid",
        SourceError::InvalidFirmsRows(_) => "NASA FIRMS rows could not be normalized",
        _ => "NASA FIRMS import failed",
    }
}

const fn terminal_state(persistence: FirmsPersistenceResult) -> FirmsTerminalState {
    if persistence.rejected > 0 {
        FirmsTerminalState::PartiallySucceeded
    } else {
        FirmsTerminalState::Succeeded
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FirmsPipelineError {
    #[error("failed to create FIRMS import tracking: {0}")]
    Begin(StoreError),
    #[error("FIRMS retrieval failed: {0}")]
    Fetch(SourceError),
    #[error("FIRMS persistence failed: {0}")]
    Persist(StoreError),
    #[error("failed to finalize FIRMS import tracking: {0}")]
    Finalize(StoreError),
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use grid::{BoundingBox, H3Grid};
    use ingest::FetchCtx;
    use ingest::SourceError;

    use store::{FirmsPersistenceResult, FirmsTerminalState};

    use super::{batch_parameters, safe_source_error, terminal_state};

    #[test]
    fn persisted_parameters_and_errors_never_include_keys() {
        let context = FetchCtx {
            client: reqwest::Client::new(),
            aoi: BoundingBox::new(4.8, 43.3, 5.0, 43.6).expect("valid bbox"),
            grid: H3Grid::new(9).expect("valid grid"),
            days: 1,
            end_date: NaiveDate::from_ymd_opt(2023, 7, 12).expect("valid date"),
            firms_map_key: Some("secret-map-key".to_owned()),
            meteofrance_api_key: None,
        };
        let parameters = batch_parameters(&context).to_string();
        assert!(!parameters.contains("secret-map-key"));
        assert!(!parameters.contains("map_key"));
        assert_eq!(
            safe_source_error(&SourceError::InvalidDayCount),
            "NASA FIRMS day count was invalid"
        );
        assert!(!safe_source_error(&SourceError::InvalidDayCount).contains("secret"));
    }

    #[test]
    fn terminal_state_accepts_empty_batches_and_reports_partial_rows() {
        assert_eq!(
            terminal_state(FirmsPersistenceResult::default()),
            FirmsTerminalState::Succeeded
        );
        assert_eq!(
            terminal_state(FirmsPersistenceResult {
                received: 2,
                rejected: 1,
                ..FirmsPersistenceResult::default()
            }),
            FirmsTerminalState::PartiallySucceeded
        );
    }
}
