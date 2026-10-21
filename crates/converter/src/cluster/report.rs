//! The cluster stage's report: what ran, what it cost and what it produced (ADR-0013 "Prototype
//! proof"). Written to `cluster-report.json` and carried in the pipeline report so a benchmark
//! driver can read it back.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Wall-clock and CPU time of the stage, split the way the ADR asks for them.
///
/// The model pass runs loading, classifying, cluster building and baked chain building in one
/// parallel pass, so each part is measured as summed CPU time across workers. The attributed
/// wall time apportions the pass's wall time by those shares; it is an estimate, and the report
/// says so.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageTimings {
    pub probe_ms: f64,
    pub classify_ms: f64,
    pub models_pass_wall_ms: f64,
    pub blocks_pass_wall_ms: f64,
    pub tables_ms: f64,
    pub total_ms: f64,
    pub cpu_model_load_ms: f64,
    pub cpu_cluster_build_ms: f64,
    pub cpu_cluster_write_ms: f64,
    pub cpu_baked_chain_ms: f64,
    pub cpu_instance_block_ms: f64,
    pub cpu_baked_block_ms: f64,
    /// Attributed wall time of the baked object chunk build (chains plus blocks).
    pub attributed_baked_ms: f64,
    /// Attributed wall time of the cluster build (payloads, instance blocks, tables).
    pub attributed_cluster_ms: f64,
    /// Attributed wall time of loading and classifying models, which both modes share.
    pub attributed_shared_ms: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageCounts {
    pub references_examined: u64,
    pub references_eligible: u64,
    pub models: u64,
    pub models_loaded: u64,
    pub models_known_from_previous: u64,
    pub models_missing_from_manifest: u64,
    pub models_failed_to_load: u64,
    pub primitives: u64,
    pub primitives_clustered: u64,
    pub meshes_built: u64,
    pub meshes_reused: u64,
    pub clusters: u64,
    pub groups: u64,
    pub source_triangles_clustered: u64,
    pub monotonic_fixups: u64,
    pub instances: u64,
    pub blocks: u64,
    pub baked_primitives: u64,
    pub baked_blocks: u64,
    pub baked_blocks_reused: u64,
    pub baked_instances: u64,
    pub baked_input_triangles: u64,
    pub baked_output_triangles: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageBytes {
    pub mesh_payloads: u64,
    pub instance_payloads: u64,
    pub baked_chunks: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StageReport {
    pub requested_mode: String,
    /// The mode the stage ran in after the probe: `cluster` or `baked`.
    pub mode: String,
    pub probe: Option<serde_json::Value>,
    pub builder_identity: String,
    pub triangle_threshold: u32,
    /// Whether the stand-in baked object chunks were built (measurement only).
    pub bench_baked: bool,
    pub jobs: usize,
    pub timings: StageTimings,
    pub counts: StageCounts,
    pub bytes: StageBytes,
    pub peak_working_set_bytes: Option<u64>,
    /// First failing reference rule, counted.
    pub reference_rejects: BTreeMap<String, u64>,
    /// Primitive reject reasons, counted (what `cluster_rejects` holds).
    pub primitive_rejects: BTreeMap<String, u64>,
    pub notices: Vec<String>,
}
