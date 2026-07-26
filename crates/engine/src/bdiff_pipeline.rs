use std::{path::Path, time::Instant};

use chrono::Utc;
use grid::H3Grid;
use store::{
    BdiffImportIds, BdiffImportStart, BdiffPersistenceResult, BdiffTerminalState, Store, StoreError,
};

const PIPELINE_VERSION: &str = "v1";

#[derive(Clone, Debug)]
pub struct BdiffImportResult {
    pub ids: BdiffImportIds,
    pub persistence: BdiffPersistenceResult,
    pub status: BdiffTerminalState,
    pub elapsed_seconds: f64,
}

/// Executes one traced BDIFF file import.
///
/// # Errors
///
/// Returns an error after attempting to finalize known failures.
pub async fn run(
    store: &Store,
    path: &Path,
    grid: H3Grid,
) -> Result<BdiffImportResult, BdiffPipelineError> {
    let started = Instant::now();
    let parameters = batch_parameters(path, u8::from(grid.resolution()));
    let ids = store
        .begin_bdiff_import(&BdiffImportStart {
            batch_type: "bdiff_file_import".to_owned(),
            trigger_type: "manual".to_owned(),
            parameters,
            pipeline_version: PIPELINE_VERSION.to_owned(),
            code_version: option_env!("GIT_COMMIT_SHA").map(str::to_owned),
        })
        .await
        .map_err(BdiffPipelineError::Begin)?;
    tracing::info!(
        source_code = "bdiff",
        import_batch_id = %ids.batch_id,
        pipeline_run_id = %ids.pipeline_run_id,
        file = %safe_file_name(path),
        "BDIFF import started"
    );

    let document = match ingest::bdiff::read_file(path, grid).await {
        Ok(document) => document,
        Err(error) => {
            finalize_failure(store, &ids, "BDIFF file could not be decoded").await?;
            return Err(BdiffPipelineError::Read(error));
        }
    };
    let persistence = match store
        .persist_bdiff_import(
            &ids,
            &document.rows,
            Utc::now(),
            u8::from(grid.resolution()),
        )
        .await
    {
        Ok(result) => result,
        Err(error) => {
            finalize_failure(store, &ids, "BDIFF PostgreSQL persistence failed").await?;
            return Err(BdiffPipelineError::Persist(error));
        }
    };
    let status = if persistence.staging_rejected > 0 {
        BdiffTerminalState::PartiallySucceeded
    } else {
        BdiffTerminalState::Succeeded
    };
    let error_message =
        (persistence.staging_rejected > 0).then_some("One or more BDIFF rows were rejected");
    store
        .finish_bdiff_import(&ids, status, persistence, error_message)
        .await
        .map_err(BdiffPipelineError::Finalize)?;
    let elapsed_seconds = started.elapsed().as_secs_f64();
    tracing::info!(
        source_code = "bdiff",
        import_batch_id = %ids.batch_id,
        pipeline_run_id = %ids.pipeline_run_id,
        received = persistence.received,
        raw_inserted = persistence.raw_inserted,
        staging_valid = persistence.staging_valid,
        staging_rejected = persistence.staging_rejected,
        fire_created = persistence.fire_created,
        fire_already_present = persistence.fire_already_present,
        technical_duplicates = persistence.technical_duplicates,
        status = status.as_str(),
        elapsed_seconds,
        "BDIFF import complete"
    );
    Ok(BdiffImportResult {
        ids,
        persistence,
        status,
        elapsed_seconds,
    })
}

fn batch_parameters(path: &Path, h3_resolution: u8) -> serde_json::Value {
    serde_json::json!({
        "file_name": safe_file_name(path),
        "format": "normalized_csv",
        "h3_resolution": h3_resolution,
        "normalizer_version": ingest::bdiff::NORMALIZER_VERSION,
        "taxonomy_version": ingest::bdiff::TAXONOMY_VERSION,
    })
}

fn safe_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("bdiff.csv")
        .to_owned()
}

async fn finalize_failure(
    store: &Store,
    ids: &BdiffImportIds,
    safe_error: &str,
) -> Result<(), BdiffPipelineError> {
    store
        .finish_bdiff_import(
            ids,
            BdiffTerminalState::Failed,
            BdiffPersistenceResult::default(),
            Some(safe_error),
        )
        .await
        .map_err(BdiffPipelineError::Finalize)
}

#[derive(Debug, thiserror::Error)]
pub enum BdiffPipelineError {
    #[error("failed to create BDIFF import tracking: {0}")]
    Begin(StoreError),
    #[error("BDIFF file retrieval failed: {0}")]
    Read(ingest::bdiff::BdiffReadError),
    #[error("BDIFF persistence failed: {0}")]
    Persist(StoreError),
    #[error("failed to finalize BDIFF import tracking: {0}")]
    Finalize(StoreError),
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{batch_parameters, safe_file_name};

    #[test]
    fn parameters_only_retain_safe_non_secret_metadata() {
        let path = Path::new("/private/operator/export.csv");
        let parameters = batch_parameters(path, 8).to_string();
        assert_eq!(safe_file_name(path), "export.csv");
        assert!(parameters.contains("export.csv"));
        assert!(!parameters.contains("/private/operator"));
    }
}
