//! Pure, deterministic logic for the phase 3B.3 dataset foundation:
//! historical calendar computation, temporal-validity classification,
//! splits, pilot-only negative selection, and dataset-row primitives.
//! No I/O, no database access — see `store::dataset` for persistence and
//! `engine::dataset_pipeline` for orchestration.

pub mod calendar;
pub mod checksums;
pub mod exclusions;
pub mod features_h3;
pub mod negative_design;
pub mod negatives;
pub mod normalization;
pub mod rows;
pub mod snapshots;
pub mod splits;
pub mod temporal;
