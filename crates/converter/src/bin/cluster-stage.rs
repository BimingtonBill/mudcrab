//! Runs the cluster LOD stage alone against a finished conversion, for timing (ADR-0013
//! "Prototype proof"). The finished output is read, never written: its world database is opened
//! read-only and the payloads and a small tables database are written to the work folder.
//!
//! ```text
//! cluster-stage --converted DIR --work DIR [--previous DIR] [--lod-mode auto|cluster|baked]
//!               [--cluster-triangle-threshold N] [--bench-baked] [--jobs N]
//!               [--simulate-plugin-change PLUGIN] [--simulate-mesh-change GLB]
//!               [--area WORLDSPACE_ID MIN_X MIN_Y MAX_X MAX_Y] [--include-worldspace ID]...
//!               [--include-trees] [--run-artifacts DIR --run-owner TEXT]
//! ```
//!
//! `--previous` is an earlier work folder: its payloads are reused when its builder identity
//! matches, which is what a warm conversion does with the published output. The simulate flags
//! change the inputs of this run only: a plugin change moves every reference the plugin owns by
//! one unit in the copied database; a mesh change gives one GLB a different content hash.
//! `--area` keeps only the references of one worldspace inside a rectangle of exterior cells
//! (corners inclusive), for a small demo output of one place. `--include-worldspace` (repeatable)
//! adds a child worldspace whose coordinates are its parent's, such as WhiterunWorld (107119):
//! its references join the parent's blocks and are marked with their source worldspace.

use color_eyre::{
    Result,
    eyre::{WrapErr, bail},
};
use converter::{
    cache::ConversionManifest,
    cluster::{
        DEFAULT_TRIANGLE_THRESHOLD, LodMode,
        stage::{self, CellArea},
    },
};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut converted = None;
    let mut work = None;
    let mut previous = None;
    let mut mode = LodMode::Cluster;
    let mut threshold = DEFAULT_TRIANGLE_THRESHOLD;
    let mut bench_baked = false;
    let mut jobs = std::thread::available_parallelism().map_or(1, usize::from);
    let mut plugin_change = None;
    let mut mesh_change = None;
    let mut area = None;
    let mut child_worldspaces = Vec::new();
    let mut include_trees = false;
    let mut run_artifacts = None;
    let mut run_owner = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| color_eyre::eyre::eyre!("{name} requires a value"))
        };
        match argument.as_str() {
            "--converted" => converted = Some(PathBuf::from(value("--converted")?)),
            "--work" => work = Some(PathBuf::from(value("--work")?)),
            "--previous" => previous = Some(PathBuf::from(value("--previous")?)),
            "--lod-mode" => {
                mode = value("--lod-mode")?
                    .parse()
                    .map_err(|error| color_eyre::eyre::eyre!("{error}"))?
            }
            "--cluster-triangle-threshold" => {
                threshold = value("--cluster-triangle-threshold")?.parse()?
            }
            "--bench-baked" => bench_baked = true,
            "--jobs" => jobs = value("--jobs")?.parse()?,
            "--simulate-plugin-change" => plugin_change = Some(value("--simulate-plugin-change")?),
            "--simulate-mesh-change" => mesh_change = Some(value("--simulate-mesh-change")?),
            "--include-trees" => include_trees = true,
            "--run-artifacts" => {
                run_artifacts = Some(std::path::PathBuf::from(value("--run-artifacts")?))
            }
            "--run-owner" => run_owner = Some(value("--run-owner")?),
            "--include-worldspace" => {
                child_worldspaces.push(value("--include-worldspace")?.parse::<u32>()?)
            }
            "--area" => {
                let mut number = |name: &str| -> Result<i64> { Ok(value(name)?.parse::<i64>()?) };
                let worldspace_id = number("--area WORLDSPACE_ID")? as u32;
                let min = (
                    number("--area MIN_X")? as i32,
                    number("--area MIN_Y")? as i32,
                );
                let max = (
                    number("--area MAX_X")? as i32,
                    number("--area MAX_Y")? as i32,
                );
                if min.0 > max.0 || min.1 > max.1 {
                    bail!("--area needs MIN_X <= MAX_X and MIN_Y <= MAX_Y");
                }
                area = Some(CellArea {
                    worldspace_id,
                    min,
                    max,
                });
            }
            other => bail!("unknown option {other}; see the module documentation"),
        }
    }
    let (Some(converted), Some(work)) = (converted, work) else {
        bail!("--converted and --work are required");
    };
    std::fs::create_dir_all(&work)?;
    // The exclusive writer lock of this package, taken before anything is read or written and
    // kept until the manifest is final.
    let work_lock = shared::asset_lock::AssetLock::acquire_exclusive(&work)?;
    let tables = work.join("skyrim_world.db");
    if tables.exists() {
        bail!("{} exists: use a new work folder per run", tables.display());
    }
    // The finished output's database is read in place; only a simulated plugin change needs a
    // private copy, because it edits references.
    let mut read_database = converted.join("skyrim_world.db");
    if let Some(plugin) = &plugin_change {
        read_database = work.join("edited-input.db");
        std::fs::copy(converted.join("skyrim_world.db"), &read_database)
            .wrap_err("cannot copy the world database")?;
        let connection = rusqlite::Connection::open(&read_database)?;
        let changed = connection.execute(
            "UPDATE \"references\" SET pos_x = pos_x + 1.0
             WHERE id IN (SELECT form_id FROM formid_map WHERE plugin_name = ?1 COLLATE NOCASE)",
            [plugin],
        )?;
        eprintln!("simulated change of {plugin}: {changed} references moved by one unit");
    }
    // Read the manifest as written, whatever converter schema produced it: `ConversionManifest::load`
    // drops the mesh entries of an output from another schema, and this tool only reads GLB paths and hashes.
    let manifest_path = converted.join("conversion-manifest.json");
    let mut manifest: ConversionManifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .wrap_err_with(|| format!("cannot read {}", manifest_path.display()))?,
    )
    .wrap_err("invalid conversion manifest")?;
    if let Some(glb) = &mesh_change {
        let entry = manifest
            .entries
            .values_mut()
            .find(|entry| entry.output == *glb)
            .ok_or_else(|| color_eyre::eyre::eyre!("{glb} is not in the manifest"))?;
        entry.output_hash = entry.output_hash.chars().rev().collect();
        eprintln!("simulated change of {glb}: new content hash");
    }

    let settings = stage::StageSettings {
        mode,
        triangle_threshold: threshold,
        bench_baked,
        jobs,
        assets_root: Some(converted.clone()),
        keep_bench_artifacts: true,
        read_database: Some(read_database.clone()),
        area,
        child_worldspaces,
        include_trees,
        map_references: false,
        run_artifacts: match (run_artifacts, run_owner) {
            (Some(directory), Some(owner)) => Some(stage::RunArtifacts { directory, owner }),
            (None, None) => None,
            _ => bail!("--run-artifacts and --run-owner go together: the folder gets an OWNER.txt"),
        },
    };
    let none = work.join("no-previous-output");
    let started = Instant::now();
    let outcome = stage::run_stage(
        &settings,
        None,
        &work,
        previous.as_deref().unwrap_or(&none),
        &manifest,
    );
    if plugin_change.is_some() {
        let _ = std::fs::remove_file(&read_database);
    }
    let (report, _artifacts) = outcome?;
    if report.mode == "cluster" {
        let context = converter::cluster::publish::PublishContext::new(&work_lock, &work, &work)?;
        // The work folder holds no core data set: this is a benchmark store, marked uncertified
        // and without a wrapper entry, never a data set publication.
        converter::cluster::publish::finalize_benchmark(&context)?;
    }
    eprintln!("stage wall time {:.1} s", started.elapsed().as_secs_f64());
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
