//! Cluster LOD stage of the converter (ADR-0013): classifier, builder, payload writers, tables
//! and the stage report, behind `--lod-mode auto|cluster|baked`.
//!
//! Coordinate conventions of the payloads: mesh space is the converted GLB's, with every node
//! transform and the scene root's Creation-to-runtime rotation baked in, so it is in runtime
//! (Y-up) axes. An instance transform is the placed reference's translation, rotation and scale in
//! the same axes (Creation units). Block bounds, in the `.mci` header and the database, are in
//! Creation axes (x east, y north, z up) like `lod_chunks`, so one R-tree query serves both.

pub mod authority;
pub mod builder;
pub mod classify;
pub mod glb_input;
pub mod identity;
pub mod model;
pub mod model_facts;
pub mod overrides;
pub mod peak_memory;
pub mod plain_lod;
pub mod probe;
pub mod publish;
pub mod report;
pub mod resolve;
pub mod scope;
pub mod stage;
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod test_support;

pub use report::StageReport;

use crate::{
    cache::ConversionManifest,
    config::PipelineConfig,
    derive_extensions::{self, DeriveOptions, DeriveReport},
    pipeline::PipelineReport,
};
use color_eyre::{Result, eyre::ensure};
use serde::{Deserialize, Serialize};
use std::{fmt, path::Path, str::FromStr};

/// Default smallest primitive that gets clusters (ADR-0013 section 5, a proposal until the
/// plain-chain comparison fixes it).
pub const DEFAULT_TRIANGLE_THRESHOLD: u32 = 1024;

/// How the converter prepares distant opaque static objects (`--lod-mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LodMode {
    /// Probe this PC's adapter. Standalone stages select a supported path; full conversions
    /// record the capability and derive an offline overlay after publishing the base.
    Auto,
    /// Build cluster payloads and tables.
    Cluster,
    /// Today's behaviour: nothing is added.
    #[default]
    Baked,
}

impl fmt::Display for LodMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::Cluster => "cluster",
            Self::Baked => "baked",
        })
    }
}

impl FromStr for LodMode {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "auto" => Ok(Self::Auto),
            "cluster" => Ok(Self::Cluster),
            "baked" => Ok(Self::Baked),
            other => Err(format!("unknown LOD mode {other}")),
        }
    }
}

/// Whether a conversion asks for a cluster overlay at all: `cluster` or `auto` with LOD on.
/// `baked` and `--no-lod` do no cluster work (no probe, no derive).
pub(crate) fn derive_requested(config: &PipelineConfig) -> bool {
    !config.no_lod && config.lod_mode != LodMode::Baked
}

/// Refuses cluster requests a conversion cannot honour, before any work starts.
pub(crate) fn validate_request(config: &PipelineConfig) -> Result<()> {
    ensure!(
        !config.cluster_bench_baked,
        "--cluster-bench-baked is not supported by conversion: cluster data is derived as an overlay \
         after the base is published, and the baked stand-in is a measurement build, not an overlay. \
         Run the standalone `cluster-stage --bench-baked` against the converted output instead"
    );
    Ok(())
}

/// The capability probe of `--lod-mode auto` (and the adapter record of a forced `cluster`),
/// asked once before any LOD work starts, as ADR-0013 section 6 says, and handed to the overlay
/// derivation so the adapter is never asked twice. `None` when no probe is needed.
pub(crate) fn probe_for_pipeline(config: &PipelineConfig) -> Option<(probe::ProbeResult, f64)> {
    if !derive_requested(config) {
        return None;
    }
    let started = std::time::Instant::now();
    let result = probe::probe_capabilities();
    Some((result, started.elapsed().as_secs_f64() * 1000.0))
}

type ProbeReceipt = Option<(probe::ProbeResult, f64)>;
type DeriveRunner = fn(&DeriveOptions, ProbeReceipt) -> Result<DeriveReport>;

/// The two environment-dependent steps of the post-publish overlay derivation. Production uses
/// [`PostPublish::REAL`]; tests substitute a synthetic capability for the GPU probe and call the
/// real derivation through a CPU-only entry point, so the base is really published and the
/// overlay is really derived.
#[derive(Clone, Copy)]
pub(crate) struct PostPublish {
    pub(crate) probe: fn(&PipelineConfig) -> Option<(probe::ProbeResult, f64)>,
    pub(crate) derive: DeriveRunner,
}

impl PostPublish {
    pub(crate) const REAL: Self = Self {
        probe: probe_for_pipeline,
        derive: derive_extensions::run_with_probe,
    };

    /// Synthetic capability, real derivation (no GPU, no disk-floor dependence).
    #[cfg(test)]
    pub(crate) const FIXTURE: Self = Self {
        probe: fixture_probe,
        derive: derive_extensions::run_fixture_with_probe,
    };
}

#[cfg(test)]
pub(crate) fn fixture_probe(config: &PipelineConfig) -> Option<(probe::ProbeResult, f64)> {
    derive_requested(config).then(|| {
        (
            probe::ProbeResult {
                passed: true,
                missing: Vec::new(),
                backend: "synthetic-no-device".into(),
                adapter_name: "CPU fixture: no GPU requested".into(),
                json: serde_json::json!({"test": true}),
            },
            0.0,
        )
    })
}

/// What the post-publish step did.
pub(crate) enum Overlay {
    /// `baked` or `--no-lod`: no cluster work was asked for.
    NotRequested,
    Derived(Box<DeriveReport>),
}

impl Overlay {
    pub(crate) fn record(self, report: &mut PipelineReport) {
        match self {
            Self::NotRequested => {}
            Self::Derived(derived) => {
                report.notices.push(format!(
                    "cluster overlay derived at {}",
                    derived.output.display()
                ));
                report.notices.extend(derived.notices);
                report.cluster = Some(derived.stage);
            }
        }
    }
}

/// The exact command that derives the overlay again over the published base with this
/// conversion's cluster settings.
fn retry_command(config: &PipelineConfig, base: &Path) -> String {
    format!(
        "converter derive-clusters {} --triangle-threshold {} --jobs {} --label default --replace",
        shared::cluster_overlay::quote_argument(&base.display().to_string()),
        config.cluster_triangle_threshold,
        config.cpu_jobs
    )
}

/// Context for an overlay failure after the base has been published.
pub(crate) fn overlay_failure_context(config: &PipelineConfig, base: &Path) -> String {
    format!(
        "the base conversion at {} is published and valid; the cluster overlay was not derived. \
         This is not a resumable conversion. Derive only the overlay with: {}",
        base.display(),
        retry_command(config, base)
    )
}

/// Derives the cluster overlay over the just-published base, for `cluster` and `auto` alike: the
/// derivation always builds in cluster mode, offline, and records the adapter probe (even a
/// failed one) as the capability fact of this run. Does nothing in `baked` mode or with
/// `--no-lod`. The caller has released its exclusive lock on
/// `base`; the derivation takes its own shared lock on it and an exclusive lock on the overlay
/// path, reads the base read-only and removes its own staging. `early_probe` is the one adapter
/// probe of this run. On failure the published base is untouched and the error carries the
/// original cause plus the exact retry command.
pub(crate) fn derive_after_publish(
    config: &PipelineConfig,
    base: &Path,
    early_probe: Option<(probe::ProbeResult, f64)>,
    hooks: &PostPublish,
) -> Result<Overlay> {
    if !derive_requested(config) {
        return Ok(Overlay::NotRequested);
    }
    let options = DeriveOptions {
        source: base.to_path_buf(),
        label: "default".into(),
        replace: false,
        keep_failed_staging: false,
        jobs: config.cpu_jobs,
        triangle_threshold: config.cluster_triangle_threshold,
        include_trees: false,
    };
    (hooks.derive)(&options, early_probe)
        .map(|derived| Overlay::Derived(Box::new(derived)))
        .map_err(|error| error.wrap_err(overlay_failure_context(config, base)))
}

/// Finalizes the certified object extension against the current core authority.
pub fn finalize_certified_extensions(
    settings: &stage::StageSettings,
    context: &publish::PublishContext<'_>,
    manifest: &ConversionManifest,
    builder_identity: &str,
) -> Result<(Vec<shared::extensions::ExtensionEntry>, Vec<String>)> {
    let authority = authority::CoreAuthority::open(
        context.open_core_read_only()?,
        context.core_root(),
        manifest,
        settings,
        builder_identity.to_owned(),
    )?;
    let base = publish::finalize_base(context, Some(&authority))?;
    Ok((vec![base.extension], Vec::new()))
}

