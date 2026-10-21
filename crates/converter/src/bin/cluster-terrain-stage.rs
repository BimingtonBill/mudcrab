//! Adds terrain blocks to an uncertified benchmark package that `cluster-stage` already wrote.
//! Certified datasets use the pipeline and its staged whole-package publication instead.
//!
//! ```text
//! cluster-terrain-stage --converted DIR --package DIR
//!                       [--area WORLDSPACE_ID MIN_X MIN_Y MAX_X MAX_Y]
//! ```
//!
//! `--converted` is the finished source conversion (its world database, `cell_cache.rkyv` and
//! `vfs/textures` are read, nothing is written there). `--package` is the work folder of
//! `cluster-stage`: its `skyrim_world.db` gets the terrain tables and its `clusters/terrain` folder
//! the payloads. A package that already has terrain rows is refused: run the stage on a fresh copy
//! of the package. `--area` limits the stage to one worldspace and to the 16x16-cell blocks that
//! have a land cell inside the rectangle (corners inclusive); each such block keeps all its cells.

use color_eyre::{
    Result,
    eyre::{bail, ensure, eyre},
};
use converter::cluster::{classify::read_sources, publish, stage::CellArea, terrain_stage};
use shared::asset_lock::{AssetLock, resolve_asset_path};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<()> {
    color_eyre::install()?;
    let mut converted = None;
    let mut package = None;
    let mut area = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| eyre!("{name} requires a value"));
        match argument.as_str() {
            "--converted" => converted = Some(PathBuf::from(value("--converted")?)),
            "--package" => package = Some(PathBuf::from(value("--package")?)),
            "--area" => {
                let mut number = |name: &str| -> Result<i64> { Ok(value(name)?.parse::<i64>()?) };
                let worldspace_id = u32::try_from(number("--area WORLDSPACE_ID")?)?;
                let min = (
                    i32::try_from(number("--area MIN_X")?)?,
                    i32::try_from(number("--area MIN_Y")?)?,
                );
                let max = (
                    i32::try_from(number("--area MAX_X")?)?,
                    i32::try_from(number("--area MAX_Y")?)?,
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
    let (Some(converted), Some(package)) = (converted, package) else {
        bail!("--converted and --package are required");
    };
    let lock = AssetLock::acquire_exclusive(&package)?;
    let _source_lock = if resolve_asset_path(&converted)? != resolve_asset_path(&package)? {
        Some(AssetLock::acquire_shared(&converted)?)
    } else {
        None
    };
    let previous: serde_json::Value =
        serde_json::from_slice(&std::fs::read(package.join(publish::CLUSTER_MANIFEST))?)?;
    ensure!(
        previous
            .get("uncertified")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        "standalone terrain runs require an uncertified benchmark package; use the pipeline for datasets"
    );
    let context = publish::PublishContext::new(&lock, &package, &package)?;
    let started = Instant::now();
    let report = terrain_stage::run_benchmark_with_report(&converted, &context, area)?;
    let source = rusqlite::Connection::open_with_flags(
        converted.join("skyrim_world.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let origins = read_sources(&source, &[])?
        .worldspaces
        .into_iter()
        .map(|world| {
            (
                world.worldspace_id,
                (world.origin.grid_x, world.origin.grid_y),
            )
        })
        .collect();
    publish::finalize_benchmark_with_origins(&context, &origins)?;
    eprintln!(
        "terrain stage: {} blocks, {} cells, {} height rows in {:.1} s",
        report.blocks,
        report.cells,
        report.height_rows,
        started.elapsed().as_secs_f64()
    );
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
