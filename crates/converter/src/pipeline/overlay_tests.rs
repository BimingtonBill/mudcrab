//! Post-publish cluster overlay derivation (DS1): a conversion never writes cluster data into the
//! base; `cluster` and `auto` both derive one overlay over the published base afterwards.
//!
//! Every test converts a real fixture and derives for real. Only the environment is substituted
//! (`PostPublish::FIXTURE`: a synthetic adapter capability and the CPU-only derive entry with an
//! injected free-space reader); nothing here touches a GPU or the disk-floor probe.

use super::*;
use crate::{
    check::{CheckMode, CheckProblem, check_output},
    cluster::{DEFAULT_TRIANGLE_THRESHOLD, LodMode, PostPublish, probe::ProbeResult},
    derive_extensions::{self, DeriveOptions, DeriveReport},
};
use dummy_content::{Entry, bsa, layout};
use std::sync::atomic::AtomicUsize;

// Skyrim layout: two i16 origins, then i32 stride/minimum/maximum levels.
const PACKED_SETTINGS: [u8; 16] = [0xfc, 0xff, 0xfc, 0xff, 32, 0, 0, 0, 4, 0, 0, 0, 32, 0, 0, 0];

struct World {
    _directory: tempfile::TempDir,
    data: PathBuf,
    output: PathBuf,
}

/// Generated game data with a worldspace whose LOD settings give the base a core identity.
fn world(generate: bool) -> World {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("Data");
    let output = directory.path().join("assets");
    if generate {
        layout::prepare_directory(&data, false).unwrap();
        layout::generate(
            &data,
            layout::DEFAULT_SEED,
            layout::Formats::parse("dds,pex,nif,esm").unwrap(),
        )
        .unwrap();
        fs::write(
            data.join("Skyrim - Misc.bsa"),
            bsa::v105(
                &[Entry {
                    name: "LODSettings/GeneratedWorld.LOD",
                    data: &PACKED_SETTINGS,
                }],
                bsa::Compression::None,
            )
            .unwrap(),
        )
        .unwrap();
    } else {
        fs::create_dir_all(&data).unwrap();
    }
    World {
        _directory: directory,
        data,
        output,
    }
}

fn config(world: &World, mode: LodMode) -> PipelineConfig {
    let mut config = PipelineConfig::new(&world.data, &world.output);
    config.lod_mode = mode;
    config.cpu_jobs = 2;
    config
}

async fn convert(
    config: PipelineConfig,
    hooks: PostPublish,
) -> Result<PipelineReport, PipelineFailure> {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
    let drain = tokio::spawn(async move { while receiver.recv().await.is_some() {} });
    let result =
        AssetPipeline::run_with_post_publish(config, sender, Cancellation::new(), hooks).await;
    drain.await.unwrap();
    result
}

/// The published base carries no cluster table, payload, manifest or extension entry.
fn assert_clusterless_base(output: &Path) {
    for absent in [
        "clusters",
        "cluster-manifest.json",
        "cluster-terrain-manifest.json",
    ] {
        assert!(
            !output.join(absent).exists(),
            "the base holds {absent}; cluster data belongs to the overlay"
        );
    }
    let database = Connection::open_with_flags(
        output.join("skyrim_world.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let tables: i64 = database
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name LIKE 'cluster_%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0, "the base database holds cluster tables");
    let manifest = ConversionManifest::load(&output.join("conversion-manifest.json")).unwrap();
    assert!(
        manifest
            .extensions
            .iter()
            .all(|entry| entry.id != shared::extensions::CLUSTERS
                && entry.id != shared::extensions::CLUSTER_TERRAIN),
        "the base manifest declares a cluster extension"
    );
}

/// The only overlay under `derived/`; a leftover derive staging folder would be a second one.
fn sole_overlay(output: &Path) -> PathBuf {
    let mut overlays: Vec<_> = fs::read_dir(output.join("derived"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    assert_eq!(overlays.len(), 1, "{overlays:?}");
    overlays.remove(0)
}

/// The engine-side binder accepts the overlay for this base.
fn assert_accepted_overlay(output: &Path) {
    let overlay = fs::canonicalize(sole_overlay(output)).unwrap();
    let metadata =
        shared::cluster_overlay::checked_overlay(&fs::canonicalize(output).unwrap(), &overlay)
            .unwrap();
    assert_eq!(metadata.label, "default");
    assert!(
        metadata
            .extensions
            .iter()
            .any(|entry| entry.id == shared::extensions::CLUSTERS)
    );
    assert!(overlay.join("clusters.db").is_file());
    assert!(
        !overlay.join("skyrim_world.db").exists(),
        "the overlay copied the core database"
    );
}

fn no_staging_beside(output: &Path) {
    for entry in fs::read_dir(output.parent().unwrap()).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(!name.contains(".staging-"), "left behind: {name}");
    }
}

/// The text between `marker` and the next double quote.
fn quoted_after(text: &str, marker: &str) -> PathBuf {
    let rest = &text[text
        .find(marker)
        .unwrap_or_else(|| panic!("{marker} in {text}"))
        + marker.len()..];
    PathBuf::from(&rest[..rest.find(marker.chars().last().unwrap()).unwrap()])
}

fn failing_derive(
    _: &DeriveOptions,
    _: Option<(ProbeResult, f64)>,
) -> color_eyre::Result<DeriveReport> {
    Err(color_eyre::eyre::eyre!("injected derive failure"))
}

const FAILING: PostPublish = PostPublish {
    probe: crate::cluster::fixture_probe,
    derive: failing_derive,
};

#[tokio::test]
async fn cluster_and_auto_conversions_publish_a_clusterless_base_and_one_accepted_overlay() {
    for mode in [LodMode::Cluster, LodMode::Auto] {
        let world = world(true);
        let report = convert(config(&world, mode), PostPublish::FIXTURE)
            .await
            .unwrap();
        assert!(
            report.cluster.is_some(),
            "{mode}: preserve the overlay stage counters in the pipeline report"
        );
        assert!(
            report
                .notices
                .iter()
                .any(|notice| notice.contains("cluster overlay derived at")),
            "{mode}: {:?}",
            report.notices
        );
        assert_clusterless_base(&world.output);
        assert_accepted_overlay(&world.output);
        no_staging_beside(&world.output);
    }
}

static PROBES: AtomicUsize = AtomicUsize::new(0);
static DERIVE_CALLS: AtomicUsize = AtomicUsize::new(0);
static DERIVE_SAW_FAILED_PROBE: AtomicBool = AtomicBool::new(false);

fn counting_failed_probe(config: &PipelineConfig) -> Option<(ProbeResult, f64)> {
    PROBES.fetch_add(1, Ordering::SeqCst);
    crate::cluster::derive_requested(config).then(|| {
        (
            ProbeResult {
                passed: false,
                missing: vec!["synthetic missing feature".into()],
                backend: "synthetic-no-device".into(),
                adapter_name: "CPU fixture: probe failed".into(),
                json: serde_json::json!({"test": true}),
            },
            0.0,
        )
    })
}

fn checking_derive(
    options: &DeriveOptions,
    probe: Option<(ProbeResult, f64)>,
) -> color_eyre::Result<DeriveReport> {
    DERIVE_CALLS.fetch_add(1, Ordering::SeqCst);
    DERIVE_SAW_FAILED_PROBE.store(
        probe.as_ref().is_some_and(|(result, _)| !result.passed),
        Ordering::SeqCst,
    );
    derive_extensions::run_fixture_with_probe(options, probe)
}

#[tokio::test]
async fn auto_derives_even_after_a_failed_probe_and_asks_the_adapter_once() {
    let world = world(true);
    let hooks = PostPublish {
        probe: counting_failed_probe,
        derive: checking_derive,
    };
    let report = convert(config(&world, LodMode::Auto), hooks).await.unwrap();
    let stage = report.cluster.as_ref().unwrap();
    assert_eq!(stage.mode, "cluster");
    assert_eq!(stage.probe.as_ref().unwrap()["forced"], true);
    assert_eq!(
        PROBES.load(Ordering::SeqCst),
        1,
        "one adapter probe per run"
    );
    assert_eq!(DERIVE_CALLS.load(Ordering::SeqCst), 1);
    assert!(
        DERIVE_SAW_FAILED_PROBE.load(Ordering::SeqCst),
        "the derivation receives the run's own probe instead of probing again"
    );
    assert_clusterless_base(&world.output);
    assert_accepted_overlay(&world.output);
}

#[tokio::test]
async fn baked_and_no_lod_do_no_cluster_work() {
    let hooks = PostPublish {
        probe: crate::cluster::fixture_probe,
        derive: |_, _| panic!("a baked or --no-lod conversion must not derive"),
    };
    for (mode, no_lod) in [(LodMode::Baked, false), (LodMode::Cluster, true)] {
        let world = world(true);
        let mut config = config(&world, mode);
        config.no_lod = no_lod;
        assert!(crate::cluster::fixture_probe(&config).is_none());
        let report = convert(config, hooks).await.unwrap();
        assert!(!report.notices.iter().any(|n| n.contains("cluster overlay")));
        assert_clusterless_base(&world.output);
        assert!(!world.output.join("derived").exists());
    }
}

#[tokio::test]
async fn derive_failure_keeps_the_published_base_and_names_the_exact_retry() {
    let world = world(true);
    let config = config(&world, LodMode::Cluster);
    let jobs = config.cpu_jobs;
    let failure = convert(config, FAILING).await.unwrap_err();
    let text = format!("{:#}", failure.error);
    assert!(text.contains("injected derive failure"), "{text}");
    assert!(text.contains("published and valid"), "{text}");
    assert!(text.contains("not a resumable conversion"), "{text}");
    assert!(
        text.contains(&format!(
            "' --triangle-threshold {DEFAULT_TRIANGLE_THRESHOLD} --jobs {jobs}"
        )),
        "{text}"
    );
    assert!(!text.contains("--terrain"), "{text}");
    let retry_base = quoted_after(&text, "converter derive-clusters '");
    assert_eq!(
        fs::canonicalize(retry_base).unwrap(),
        fs::canonicalize(&world.output).unwrap()
    );
    assert!(failure.staging.is_none(), "the conversion is not resumable");
    assert!(!failure.cancelled);
    // No staging is kept when the retry does not need it; the base is intact and has no overlay.
    no_staging_beside(&world.output);
    assert_clusterless_base(&world.output);
    assert!(!world.output.join("derived").exists());
    let check = check_output(&world.output, CheckMode::Full, |_, _| {}).unwrap();
    assert!(
        check
            .problems
            .iter()
            .all(|problem| matches!(problem, CheckProblem::Incomplete)),
        "{:?}",
        check.problems
    );
    // The retry the message names works on that base.
    derive_extensions::run_fixture(&DeriveOptions {
        source: world.output.clone(),
        label: "default".into(),
        replace: false,
        keep_failed_staging: false,

        jobs,
        triangle_threshold: DEFAULT_TRIANGLE_THRESHOLD,
        include_trees: false,
    })
    .unwrap();
    assert_accepted_overlay(&world.output);
}

/// Folders beside the output that hold detached raw terrain textures.
fn terrain_source_folders_beside(output: &Path) -> Vec<PathBuf> {
    fs::read_dir(output.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".terrain-sources-")
        })
        .collect()
}


#[tokio::test]
async fn a_non_terrain_derive_failure_leaves_no_raw_sources_folder() {
    let world = world(true);
    convert(config(&world, LodMode::Cluster), FAILING)
        .await
        .unwrap_err();
    assert!(terrain_source_folders_beside(&world.output).is_empty());
    no_staging_beside(&world.output);
}

#[tokio::test]
async fn unsupported_cluster_requests_fail_before_any_work() {
    let world = world(false);
    let mut bench = config(&world, LodMode::Cluster);
    bench.cluster_bench_baked = true;
    let failure = convert(bench, PostPublish::FIXTURE).await.unwrap_err();
    assert!(
        failure.to_string().contains("cluster-stage --bench-baked"),
        "{failure}"
    );

}

#[tokio::test]
async fn metadata_rebuild_publishes_a_clusterless_base_then_derives_the_overlay() {
    let world = world(true);
    convert(config(&world, LodMode::Baked), PostPublish::FIXTURE)
        .await
        .unwrap();
    let rebuild = |name: &str, hooks: PostPublish| {
        let mut config = PipelineConfig::new(&world.data, world.output.with_file_name(name));
        config.lod_mode = LodMode::Cluster;
        config.cpu_jobs = 2;
        let source = world.output.clone();
        async move {
            let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
            let drain = tokio::spawn(async move { while receiver.recv().await.is_some() {} });
            let result =
                AssetPipeline::rebuild_metadata_with_post_publish(config, &source, sender, hooks)
                    .await;
            drain.await.unwrap();
            result
        }
    };
    let rebuilt = world.output.with_file_name("rebuilt");
    let report = rebuild("rebuilt", PostPublish::FIXTURE).await.unwrap();
    assert!(report.cluster.is_some());
    assert_clusterless_base(&rebuilt);
    assert_accepted_overlay(&rebuilt);
    assert!(
        !world.output.join("derived").exists(),
        "the source is untouched"
    );

    let error = rebuild("rebuilt-failed", FAILING).await.unwrap_err();
    let text = format!("{error:#}");
    assert!(text.contains("injected derive failure"), "{text}");
    assert!(text.contains("converter derive-clusters '"), "{text}");
    let failed = world.output.with_file_name("rebuilt-failed");
    assert_clusterless_base(&failed);
    assert!(!failed.join("derived").exists());
}
