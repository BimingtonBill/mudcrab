//! Offline conversion of Skyrim assets into runtime-ready Mudcrab assets.

pub mod archive;
pub mod asset_path;
pub mod cache;
pub mod check;
pub mod cluster;
pub mod collision;
pub mod config;
#[cfg(test)]
mod derive_database;
pub mod derive_extensions;
mod derive_space;
mod derive_sqlite;
pub mod esm;
pub mod extensions;
pub mod integration;
pub mod lip;
pub mod lod;
pub mod material;
pub mod mesh;
mod metadata;
pub mod nif_animation;
pub mod pipeline;
/// The portal's distant-LOD inventory (`lod_grid`, `lod_block`, the tree
/// billboards). #105's terrain-chunk LOD lives in [`lod`]; the two run side by
/// side, so the portal's module keeps its own name.
pub mod portal_lod;
pub mod progress;
pub mod records;
pub mod repair;
pub mod script;
pub mod streaming_costs;
pub mod terrain_arrays;
pub mod terrain_chunks;
pub mod terrain_pipeline;
/// A synthetic complete base conversion for other crates' tests (feature `test-support`).
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod texture;
pub mod texture_gpu;
mod texture_ktx2;

#[cfg(test)]
mod test_strategies;

pub use check::{
    CheckCancelled, CheckMode, CheckProblem, CheckReport, check_output, check_output_with_cancel,
};
pub use config::{
    OutputDirError, PipelineConfig, TextureEncoder, check_output_dir, check_outside_data,
    find_resumable_staging,
};
pub use esm::EsmParser;
pub use integration::IntegrationReport;
pub use pipeline::{AssetPipeline, Cancellation, PipelineFailure, PipelineReport};
pub use progress::{AssetOutcome, ProgressEstimate, ProgressEvent, ProgressStage};
pub use shared;
