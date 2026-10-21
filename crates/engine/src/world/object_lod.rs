//! The package's Skyrim object LOD inventory, read once and kept in memory: the `lod_grid`,
//! `lod_block` (object blocks, `.bto`) and `lod_tree_*` (billboards, `.btt`/`.lst`) tables the
//! converter fills from vanilla or mod LOD files (`crates/converter/src/portal_lod.rs`).
//!
//! The index is small (one row per block and per tree type, about 1,100 blocks and 120 types for
//! all of Skyrim), so planning reads it from memory and the worker answers only the per-block
//! tree instance reads, which scale with content (tens of thousands of rows in Tamriel).
//!
//! Every row is input the engine did not write, so it is validated on the way in and a bad row is
//! counted and left out, never repaired: a path that could leave the assets root, a level the
//! grid does not list, a block that is not on the grid, bounds that are not finite. An object
//! block's GLB must also match the `output_hash` the conversion manifest recorded for it; a
//! block the manifest does not know is refused, because a file the manifest does not vouch for
//! is not part of the package. This reader carries that expected hash; the payload consumer
//! verifies the GLB bytes against it before drawing.
//!
//! A database without these tables, or a world with no rows, is not an error: the answer is an
//! empty index and nothing is drawn.

use super::is_safe_relative_asset_path;
use bevy::{math::IVec2, prelude::Resource};
use color_eyre::eyre::{Result, WrapErr};
use crossbeam_channel::{Receiver, Sender, bounded};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    thread,
};

/// The level object LOD tree billboards are stored at: one block of the tree table is 4x4 cells.
pub const TREE_BLOCK_LEVEL: u8 = 4;

/// One converted object LOD block, a `.bto` as a GLB.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectLodBlock {
    /// Cells per side, a level of the worldspace's LOD grid (Skyrim's are 4, 8, 16 and 32).
    pub level: u8,
    /// The block's south-west cell, in absolute grid coordinates.
    pub block: IVec2,
    /// Canonical path relative to the converted assets root.
    pub mesh_path: String,
    /// The conversion manifest's lowercase SHA-256 of the published GLB.
    pub content_hash: String,
    /// The AABB the converter measured over the block's vertices, in the runtime space the
    /// scene is spawned in (Y up), before the render-origin shift.
    pub bounds_min: [f32; 3],
    pub bounds_max: [f32; 3],
}

/// One billboard type of a worldspace: a crossed-quad GLB cut from the worldspace's tree atlas.
#[derive(Debug, Clone, PartialEq)]
pub struct TreeLodType {
    pub index: u32,
    pub mesh_path: String,
}

/// What the index holds for one worldspace.
#[derive(Debug, Default)]
pub struct ObjectLodWorld {
    /// The grid origin the block coordinates are aligned to, from `lod_grid`.
    pub origin: IVec2,
    /// The levels `lod_grid` lists, ascending.
    pub levels: Vec<u8>,
    blocks: HashMap<(u8, IVec2), ObjectLodBlock>,
    /// The billboard types, by their index in the worldspace's `.lst`.
    pub tree_types: BTreeMap<u32, TreeLodType>,
    /// The level-4 blocks that hold at least one tree instance.
    pub tree_blocks: HashSet<IVec2>,
}

impl ObjectLodWorld {
    /// The block of `level` whose south-west cell is `block`, if the package has it.
    pub fn block(&self, level: u8, block: IVec2) -> Option<&ObjectLodBlock> {
        self.blocks.get(&(level, block))
    }

    /// How many object blocks the world has.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Every object block, in no particular order.
    pub fn blocks(&self) -> impl Iterator<Item = &ObjectLodBlock> {
        self.blocks.values()
    }

    /// The coarsest level the grid lists that has at least one block.
    pub fn coarsest_level(&self) -> Option<u8> {
        self.levels.iter().rev().copied().find(|level| {
            self.blocks
                .keys()
                .any(|(block_level, _)| block_level == level)
        })
    }

    /// The next finer level of `level` that has blocks, if any.
    pub fn finer_level(&self, level: u8) -> Option<u8> {
        self.levels
            .iter()
            .rev()
            .copied()
            .filter(|candidate| *candidate < level)
            .find(|candidate| {
                self.blocks
                    .keys()
                    .any(|(block_level, _)| block_level == candidate)
            })
    }
    #[cfg(test)]
    pub(crate) fn for_test(origin: IVec2, levels: &[u8], blocks: &[(u8, IVec2)]) -> Self {
        let mut world = Self {
            origin,
            levels: levels.to_vec(),
            ..Self::default()
        };
        for &(level, block) in blocks {
            world.blocks.insert(
                (level, block),
                ObjectLodBlock {
                    level,
                    block,
                    mesh_path: format!(
                        "meshes/terrain/test/objects/test.{level}.{}.{}.glb",
                        block.x, block.y
                    ),
                    content_hash: String::new(),
                    bounds_min: [0.0; 3],
                    bounds_max: [1.0; 3],
                },
            );
        }
        world
    }
}

/// The whole inventory, per worldspace, plus what was refused on the way in.
#[derive(Debug, Default)]
pub struct ObjectLodIndex {
    pub worlds: HashMap<u32, ObjectLodWorld>,
    /// Rows left out, with why, for the log and the metrics. Capped at [`MAX_REFUSALS`].
    pub refused: Vec<String>,
    /// How many rows were refused, including those past the cap.
    pub refused_rows: u64,
}

/// How many refusal reasons the index keeps; the count keeps going.
pub const MAX_REFUSALS: usize = 32;

impl ObjectLodIndex {
    fn refuse(&mut self, reason: String) {
        self.refused_rows = self.refused_rows.saturating_add(1);
        if self.refused.len() < MAX_REFUSALS {
            self.refused.push(reason);
        }
    }

    /// Whether the package holds anything to draw.
    pub fn is_empty(&self) -> bool {
        self.worlds
            .values()
            .all(|world| world.blocks.is_empty() && world.tree_blocks.is_empty())
    }
}

/// One tree instance as the converter stored it, in Creation space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreeInstance {
    pub tree_index: u32,
    pub position: [f32; 3],
    /// Yaw about Creation's Z axis, in radians.
    pub rotation: f32,
    pub scale: f32,
}

/// The answer to a [`ObjectLodSource::request_tree_block`].
#[derive(Debug)]
pub struct TreeBlockResponse {
    pub worldspace_id: u32,
    pub block: IVec2,
    pub result: std::result::Result<Vec<TreeInstance>, ObjectLodFailure>,
}

/// Why a read failed, and whether asking again can help: a busy or locked database can, a row the
/// engine rejects cannot.
#[derive(Debug, Clone)]
pub struct ObjectLodFailure {
    pub transient: bool,
    pub reason: String,
}

impl ObjectLodFailure {
    fn from_error(error: &color_eyre::Report) -> Self {
        let transient = error.downcast_ref::<rusqlite::Error>().is_some_and(|error| {
            matches!(error, rusqlite::Error::SqliteFailure(code, _) if matches!(code.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked | rusqlite::ErrorCode::SystemIoFailure))
        });
        Self {
            transient,
            reason: format!("{error:#}"),
        }
    }
}

impl std::fmt::Display for ObjectLodFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.reason)
    }
}

/// Where the one-time read of the index stands.
#[derive(Debug, Clone)]
pub enum IndexState {
    /// The worker is still reading it.
    Loading,
    /// Read and validated; empty when the package has no object LOD.
    Ready(Arc<ObjectLodIndex>),
    /// The read failed. Object LOD is off for the run and the reason is logged once.
    Failed(String),
}

enum Request {
    TreeBlock { worldspace_id: u32, block: IVec2 },
    Shutdown,
}

/// The object LOD inventory and the worker that reads tree instances from the world database.
#[derive(Resource)]
pub struct ObjectLodSource {
    requests: Sender<Request>,
    index: Receiver<std::result::Result<Arc<ObjectLodIndex>, String>>,
    trees: Receiver<TreeBlockResponse>,
    state: IndexState,
    worker: Option<thread::JoinHandle<()>>,
}

impl ObjectLodSource {
    /// Starts the worker, which reads the index of the package at `assets_dir` and then answers
    /// tree instance reads until the resource is dropped.
    ///
    /// The caller must first validate the package's `mudcrab.lod_inventory` capability and
    /// content hash, and retain the dataset's shared asset lock for this resource's lifetime.
    /// This reader validates row shapes; it does not certify the extension envelope or tree
    /// replacement identities. A cluster-aware billboard consumer also needs the validated
    /// tree relation and same-frame per-view/pass draw proof before retiring a billboard.
    pub fn open(assets_dir: &Path) -> Result<Self> {
        Self::start(
            assets_dir,
            Hashes::Manifest(assets_dir.join("conversion-manifest.json")),
        )
    }

    /// [`Self::open`] with the manifest's payload hashes already read by the caller.
    pub fn open_with_hashes(assets_dir: &Path, hashes: HashMap<String, String>) -> Result<Self> {
        Self::start(assets_dir, Hashes::Read(hashes))
    }

    fn start(assets_dir: &Path, manifest: Hashes) -> Result<Self> {
        let database = assets_dir.join("skyrim_world.db");
        let (request_tx, request_rx) = bounded(256);
        let (index_tx, index_rx) = bounded(1);
        let (tree_tx, tree_rx) = bounded(256);
        let worker = thread::Builder::new()
            .name("mudcrab-object-lod-db".into())
            .spawn(move || worker(&database, manifest, request_rx, index_tx, tree_tx))
            .wrap_err("failed to start the object LOD worker")?;
        Ok(Self {
            requests: request_tx,
            index: index_rx,
            trees: tree_rx,
            state: IndexState::Loading,
            worker: Some(worker),
        })
    }

    /// The index once the worker has read it. Polled every frame by the plan; cheap once settled.
    pub fn poll_index(&mut self) -> &IndexState {
        if matches!(self.state, IndexState::Loading)
            && let Ok(result) = self.index.try_recv()
        {
            self.state = match result {
                Ok(index) => IndexState::Ready(index),
                Err(reason) => IndexState::Failed(reason),
            };
        }
        &self.state
    }

    /// Where the index stands without polling.
    pub fn state(&self) -> &IndexState {
        &self.state
    }

    /// Asks for the tree instances of one level-4 block. Errors when the queue is full; the
    /// caller asks again on a later frame.
    pub fn request_tree_block(&self, worldspace_id: u32, block: IVec2) -> Result<()> {
        self.requests
            .try_send(Request::TreeBlock {
                worldspace_id,
                block,
            })
            .map_err(|error| color_eyre::eyre::eyre!("object LOD request queue: {error}"))
    }

    /// The next finished tree block read, if any.
    pub fn try_tree_response(&self) -> Option<TreeBlockResponse> {
        self.trees.try_recv().ok()
    }
}

impl Drop for ObjectLodSource {
    fn drop(&mut self) {
        let _ = self.requests.try_send(Request::Shutdown);
        // The worker owns a read-only connection and exits on `Shutdown` or when the channel
        // closes; it is not joined, so dropping the resource never blocks a frame. Dropping
        // `trees` then disconnects the response channel and releases a blocked worker send.
        drop(self.worker.take());
    }
}

/// Where the worker gets the payload hashes the index binds to each block.
enum Hashes {
    Manifest(PathBuf),
    Read(HashMap<String, String>),
}

fn worker(
    database: &Path,
    manifest: Hashes,
    requests: Receiver<Request>,
    index: Sender<std::result::Result<Arc<ObjectLodIndex>, String>>,
    trees: Sender<TreeBlockResponse>,
) {
    let connection = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(color_eyre::eyre::Report::from);
    let result = match &connection {
        Ok(connection) => match manifest {
            Hashes::Manifest(path) => read_index(connection, &path),
            Hashes::Read(hashes) => read_index_with(connection, || Ok(hashes)),
        }
        .map(Arc::new)
        .map_err(|error| format!("{error:#}")),
        Err(error) => Err(format!(
            "world database {} is unusable: {error:#}",
            database.display()
        )),
    };
    if index.send(result.clone()).is_err() {
        return;
    }
    while let Ok(request) = requests.recv() {
        match request {
            Request::Shutdown => break,
            Request::TreeBlock {
                worldspace_id,
                block,
            } => {
                let result = match (&connection, &result) {
                    (Ok(connection), Ok(index)) => {
                        read_tree_block(connection, index, worldspace_id, block)
                            .map_err(|error| ObjectLodFailure::from_error(&error))
                    }
                    (_, Err(reason)) => Err(ObjectLodFailure {
                        transient: false,
                        reason: reason.clone(),
                    }),
                    (Err(error), _) => Err(ObjectLodFailure {
                        transient: false,
                        reason: format!("{error:#}"),
                    }),
                };
                if trees
                    .send(TreeBlockResponse {
                        worldspace_id,
                        block,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}

fn has_table(connection: &Connection, name: &str) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |row| row.get(0),
    )
}

/// The conversion manifest entries this module needs: a published path and the hash the manifest
/// recorded for it.
#[derive(Deserialize)]
struct ManifestFile {
    #[serde(default)]
    entries: BTreeMap<String, ManifestEntry>,
}

#[derive(Deserialize)]
struct ManifestEntry {
    output: Option<String>,
    output_hash: Option<String>,
}

/// The hash of every distant-terrain GLB the manifest vouches for, by its published path.
pub(crate) fn read_manifest_hashes(path: &Path) -> Result<HashMap<String, String>> {
    let bytes = std::fs::read(path)
        .wrap_err_with(|| format!("cannot read the conversion manifest {}", path.display()))?;
    let manifest: ManifestFile = serde_json::from_slice(&bytes)
        .wrap_err_with(|| format!("cannot parse the conversion manifest {}", path.display()))?;
    let mut hashes = HashMap::new();
    for entry in manifest.entries.into_values() {
        let (Some(output), Some(hash)) = (entry.output, entry.output_hash) else {
            continue;
        };
        if !output.starts_with("meshes/terrain/") {
            continue;
        }
        color_eyre::eyre::ensure!(
            hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "conversion manifest output {output} has an invalid SHA-256 digest"
        );
        let hash = hash.to_ascii_lowercase();
        if let Some(previous) = hashes.insert(output.clone(), hash.clone()) {
            color_eyre::eyre::ensure!(
                previous == hash,
                "conversion manifest output {output} has conflicting SHA-256 digests"
            );
        }
    }
    Ok(hashes)
}

/// Why a level list cannot be walked as a quadtree, if it cannot: every level must be a whole
/// multiple of the next finer one, or a block would straddle two blocks of the finer level.
fn level_nesting_error(levels: &[u8]) -> Option<String> {
    levels
        .windows(2)
        .find(|pair| pair[1] % pair[0] != 0)
        .map(|pair| {
            format!(
                "level {} is not a whole multiple of the next finer level {}",
                pair[1], pair[0]
            )
        })
}

/// Parses `lod_grid.levels`, a comma list such as `4,8,16,32`, into ascending levels. Any level
/// this reader can represent is accepted, not only Skyrim's own four. The reused planner API
/// represents levels as `u8`; wider or non-nested grids receive an explicit unsupported diagnostic.
fn parse_levels(raw: &str) -> Option<Vec<u8>> {
    let mut levels = Vec::new();
    for part in raw.split(',') {
        let level: u8 = part.trim().parse().ok().filter(|level| *level > 0)?;
        if levels.contains(&level) {
            return None;
        }
        levels.push(level);
    }
    levels.sort_unstable();
    (!levels.is_empty()).then_some(levels)
}

/// Whether `path` is a published LOD payload path: `meshes/terrain/...glb`, plain segments only.
fn is_lod_mesh_path(path: &str) -> bool {
    path.starts_with("meshes/terrain/")
        && path.ends_with(".glb")
        && !path.contains('\\')
        && !path.contains(':')
        && is_safe_relative_asset_path(path)
}

fn read_index(connection: &Connection, manifest: &Path) -> Result<ObjectLodIndex> {
    read_index_with(connection, || read_manifest_hashes(manifest))
}

fn read_index_with(
    connection: &Connection,
    hashes: impl FnOnce() -> Result<HashMap<String, String>>,
) -> Result<ObjectLodIndex> {
    let mut index = ObjectLodIndex::default();
    if !has_table(connection, "lod_grid")? || !has_table(connection, "lod_block")? {
        return Ok(index);
    }

    let mut grids =
        connection.prepare("SELECT worldspace_id, origin_x, origin_y, levels FROM lod_grid")?;
    let rows = grids.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for row in rows {
        let (worldspace_id, origin_x, origin_y, levels) = row?;
        let (Ok(worldspace_id), Ok(origin_x), Ok(origin_y)) = (
            u32::try_from(worldspace_id),
            i32::try_from(origin_x),
            i32::try_from(origin_y),
        ) else {
            index.refuse(format!(
                "lod_grid row for worldspace {worldspace_id} is out of range"
            ));
            continue;
        };
        let Some(levels) = parse_levels(&levels) else {
            index.refuse(format!(
                "lod_grid levels {levels:?} of worldspace {worldspace_id:#X} are not a list of distinct block sizes"
            ));
            continue;
        };
        if let Some(reason) = level_nesting_error(&levels) {
            index.refuse(format!(
                "lod_grid levels {levels:?} of worldspace {worldspace_id:#X} cannot nest: {reason}; its object LOD is not drawn"
            ));
            continue;
        }
        index.worlds.insert(
            worldspace_id,
            ObjectLodWorld {
                origin: IVec2::new(origin_x, origin_y),
                levels,
                ..ObjectLodWorld::default()
            },
        );
    }

    let hashes = hashes()?;

    let mut blocks = connection.prepare(
        "SELECT worldspace_id, level, block_x, block_y, mesh_path, \
         bounds_min_x, bounds_min_y, bounds_min_z, bounds_max_x, bounds_max_y, bounds_max_z \
         FROM lod_block WHERE kind = 'objects'",
    )?;
    let rows = blocks.query_map([], |row| {
        Ok(RawBlock {
            worldspace_id: row.get(0)?,
            level: row.get(1)?,
            block_x: row.get(2)?,
            block_y: row.get(3)?,
            mesh_path: row.get(4)?,
            min: [row.get(5)?, row.get(6)?, row.get(7)?],
            max: [row.get(8)?, row.get(9)?, row.get(10)?],
        })
    })?;
    for row in rows {
        let raw = row?;
        match validate_block(&index, &hashes, &raw) {
            Ok((worldspace_id, block)) => {
                if let Some(world) = index.worlds.get_mut(&worldspace_id) {
                    world.blocks.insert((block.level, block.block), block);
                }
            }
            Err(reason) => index.refuse(reason),
        }
    }

    if has_table(connection, "lod_tree_type")? && has_table(connection, "lod_tree_instance")? {
        read_tree_tables(connection, &mut index)?;
    }
    Ok(index)
}

struct RawBlock {
    worldspace_id: i64,
    level: i64,
    block_x: i64,
    block_y: i64,
    mesh_path: String,
    min: [Option<f64>; 3],
    max: [Option<f64>; 3],
}

fn validate_block(
    index: &ObjectLodIndex,
    hashes: &HashMap<String, String>,
    raw: &RawBlock,
) -> std::result::Result<(u32, ObjectLodBlock), String> {
    let name = || {
        format!(
            "object block {} (worldspace {}, level {}, {},{})",
            raw.mesh_path, raw.worldspace_id, raw.level, raw.block_x, raw.block_y
        )
    };
    let worldspace_id = u32::try_from(raw.worldspace_id)
        .map_err(|_| format!("{} has no valid worldspace", name()))?;
    let world = index.worlds.get(&worldspace_id).ok_or_else(|| {
        format!(
            "{} belongs to a worldspace with no usable lod_grid row",
            name()
        )
    })?;
    let level = u8::try_from(raw.level)
        .ok()
        .filter(|level| world.levels.contains(level))
        .ok_or_else(|| format!("{} is at a level the grid does not list", name()))?;
    let (Ok(block_x), Ok(block_y)) = (i32::try_from(raw.block_x), i32::try_from(raw.block_y))
    else {
        return Err(format!("{} has coordinates out of range", name()));
    };
    let side = i64::from(level);
    if (i64::from(block_x) - i64::from(world.origin.x)).rem_euclid(side) != 0
        || (i64::from(block_y) - i64::from(world.origin.y)).rem_euclid(side) != 0
    {
        return Err(format!(
            "{} is not aligned to the grid origin {:?}",
            name(),
            world.origin
        ));
    }
    if !is_lod_mesh_path(&raw.mesh_path) {
        return Err(format!("{} has a path outside meshes/terrain", name()));
    }
    let bounds = |values: &[Option<f64>; 3]| -> Option<[f32; 3]> {
        let mut out = [0.0f32; 3];
        for (slot, value) in out.iter_mut().zip(values) {
            let value = (*value)?;
            let value = value as f32;
            if !value.is_finite() {
                return None;
            }
            *slot = value;
        }
        Some(out)
    };
    let (Some(bounds_min), Some(bounds_max)) = (bounds(&raw.min), bounds(&raw.max)) else {
        return Err(format!("{} has no finite bounds", name()));
    };
    if bounds_min
        .iter()
        .zip(&bounds_max)
        .any(|(low, high)| low > high)
    {
        return Err(format!("{} has inverted bounds", name()));
    }
    let content_hash = hashes
        .get(&raw.mesh_path)
        .ok_or_else(|| format!("{} is not in the conversion manifest", name()))?
        .to_ascii_lowercase();
    Ok((
        worldspace_id,
        ObjectLodBlock {
            level,
            block: IVec2::new(block_x, block_y),
            mesh_path: raw.mesh_path.clone(),
            content_hash,
            bounds_min,
            bounds_max,
        },
    ))
}

fn read_tree_tables(connection: &Connection, index: &mut ObjectLodIndex) -> Result<()> {
    let mut types =
        connection.prepare("SELECT worldspace_id, tree_index, mesh_path FROM lod_tree_type")?;
    let rows = types.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (worldspace_id, tree_index, mesh_path) = row?;
        let (Ok(worldspace_id), Ok(tree_index)) =
            (u32::try_from(worldspace_id), u32::try_from(tree_index))
        else {
            index.refuse(format!("tree type {mesh_path} has an out-of-range id"));
            continue;
        };
        if !is_lod_mesh_path(&mesh_path) {
            index.refuse(format!(
                "tree type {mesh_path} has a path outside meshes/terrain"
            ));
            continue;
        }
        let Some(world) = index.worlds.get_mut(&worldspace_id) else {
            index.refuse(format!(
                "tree type {mesh_path} belongs to a worldspace with no usable lod_grid row"
            ));
            continue;
        };
        world.tree_types.insert(
            tree_index,
            TreeLodType {
                index: tree_index,
                mesh_path,
            },
        );
    }

    let mut blocks = connection
        .prepare("SELECT DISTINCT worldspace_id, block_x, block_y FROM lod_tree_instance")?;
    let rows = blocks.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (worldspace_id, block_x, block_y) = row?;
        let (Ok(worldspace_id), Ok(block_x), Ok(block_y)) = (
            u32::try_from(worldspace_id),
            i32::try_from(block_x),
            i32::try_from(block_y),
        ) else {
            index.refuse("a tree block row has coordinates out of range".to_owned());
            continue;
        };
        let Some(world) = index.worlds.get_mut(&worldspace_id) else {
            index.refuse(format!(
                "tree block {block_x},{block_y} belongs to a worldspace with no usable lod_grid row"
            ));
            continue;
        };
        let side = i64::from(TREE_BLOCK_LEVEL);
        if (i64::from(block_x) - i64::from(world.origin.x)).rem_euclid(side) != 0
            || (i64::from(block_y) - i64::from(world.origin.y)).rem_euclid(side) != 0
        {
            index.refuse(format!(
                "tree block {block_x},{block_y} is not aligned to the grid origin"
            ));
            continue;
        }
        world.tree_blocks.insert(IVec2::new(block_x, block_y));
    }
    Ok(())
}

/// Reads one tree block's instances. A row the engine cannot place - a non-finite number, a
/// scale of zero, a type the worldspace does not list - fails the whole block: a half-drawn
/// forest would look like a finished one.
fn read_tree_block(
    connection: &Connection,
    index: &ObjectLodIndex,
    worldspace_id: u32,
    block: IVec2,
) -> Result<Vec<TreeInstance>> {
    let mut statement = connection.prepare(
        "SELECT tree_index, pos_x, pos_y, pos_z, rotation, scale FROM lod_tree_instance \
         WHERE worldspace_id = ?1 AND block_x = ?2 AND block_y = ?3 \
         ORDER BY tree_index, pos_x, pos_y",
    )?;
    let rows = statement.query_map(
        rusqlite::params![
            i64::from(worldspace_id),
            i64::from(block.x),
            i64::from(block.y)
        ],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, f64>(5)?,
            ))
        },
    )?;
    let mut instances = Vec::new();
    for row in rows {
        let (tree_index, x, y, z, rotation, scale) = row?;
        let tree_index = u32::try_from(tree_index).map_err(|_| {
            color_eyre::eyre::eyre!("tree instance with type {tree_index} is out of range")
        })?;
        if !index
            .worlds
            .get(&worldspace_id)
            .is_some_and(|world| world.tree_types.contains_key(&tree_index))
        {
            color_eyre::eyre::bail!(
                "tree instance type {tree_index} is not listed in the validated worldspace {worldspace_id:#X}"
            );
        }
        let [x, y, z, rotation, scale] = [x, y, z, rotation, scale].map(|value| value as f32);
        if ![x, y, z, rotation, scale]
            .iter()
            .all(|value| value.is_finite())
            || scale <= 0.0
        {
            color_eyre::eyre::bail!(
                "tree instance of type {tree_index} has a non-finite or non-positive value"
            );
        }
        instances.push(TreeInstance {
            tree_index,
            position: [x, y, z],
            rotation,
            scale,
        });
    }
    if instances.is_empty() {
        color_eyre::eyre::bail!(
            "tree block {},{} of worldspace {worldspace_id:#X} has no instances: the table changed since the index was read",
            block.x,
            block.y
        );
    }
    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("OWNER.txt"),
            "object_lod unit test fixture; temporary, removed with TempDir\n",
        )
        .unwrap();
        dir
    }

    fn manifest_with_hashes(dir: &Path, hashes: &[&str]) -> PathBuf {
        let output = "meshes/terrain/test/objects/test.4.0.0.glb";
        let entries = hashes
            .iter()
            .enumerate()
            .map(|(ordinal, hash)| {
                let entry = serde_json::json!({ "output": output, "output_hash": hash });
                format!("\"source_{ordinal}\":{entry}")
            })
            .collect::<Vec<_>>()
            .join(",");
        let path = dir.join("conversion-manifest.json");
        std::fs::write(&path, format!("{{\"entries\":{{{entries}}}}}")).unwrap();
        path
    }

    fn fixture(dir: &Path, with_hashes: bool) -> (PathBuf, PathBuf) {
        let database = dir.join("skyrim_world.db");
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE lod_grid (worldspace_id INTEGER PRIMARY KEY, origin_x INTEGER NOT NULL, origin_y INTEGER NOT NULL, levels TEXT NOT NULL);
                 CREATE TABLE lod_block (worldspace_id INTEGER NOT NULL, kind TEXT NOT NULL, level INTEGER NOT NULL, block_x INTEGER NOT NULL, block_y INTEGER NOT NULL, mesh_path TEXT NOT NULL, bounds_min_x REAL, bounds_min_y REAL, bounds_min_z REAL, bounds_max_x REAL, bounds_max_y REAL, bounds_max_z REAL);
                 CREATE TABLE lod_tree_type (worldspace_id INTEGER NOT NULL, tree_index INTEGER NOT NULL, mesh_path TEXT NOT NULL, size_x REAL, size_y REAL, u0 REAL, v0 REAL, u1 REAL, v1 REAL);
                 CREATE TABLE lod_tree_instance (worldspace_id INTEGER NOT NULL, block_x INTEGER NOT NULL, block_y INTEGER NOT NULL, tree_index INTEGER NOT NULL, pos_x REAL, pos_y REAL, pos_z REAL, rotation REAL, scale REAL);
                 INSERT INTO lod_grid VALUES (60, -96, -96, '4,8,16');
                 INSERT INTO lod_block VALUES (60, 'objects', 4, -8, -8, 'meshes/terrain/tamriel/objects/tamriel.4.-8.-8.glb', -32768.0, -32768.0, -10.0, -16000.0, -16000.0, 900.0);
                 INSERT INTO lod_block VALUES (60, 'objects', 4, -7, -8, 'meshes/terrain/tamriel/objects/tamriel.4.-7.-8.glb', 0, 0, 0, 1, 1, 1);
                 INSERT INTO lod_block VALUES (60, 'objects', 4, -8, -4, '../escape.glb', 0, 0, 0, 1, 1, 1);
                 INSERT INTO lod_block VALUES (60, 'objects', 32, -96, -96, 'meshes/terrain/tamriel/objects/tamriel.32.-96.-96.glb', 0, 0, 0, 1, 1, 1);
                 INSERT INTO lod_block VALUES (60, 'terrain', 4, -8, -8, 'meshes/terrain/tamriel/tamriel.4.-8.-8.btr.glb', 0, 0, 0, 1, 1, 1);
                 INSERT INTO lod_tree_type VALUES (60, 0, 'meshes/terrain/tamriel/trees/tamriel.tree.0.glb', 100, 200, 0, 0, 1, 1);
                 INSERT INTO lod_tree_instance VALUES (60, -8, -8, 0, -30000.0, -30000.0, 120.0, 0.5, 1.25);
                 INSERT INTO lod_tree_instance VALUES (60, -8, -8, 0, -29000.0, -30000.0, 80.0, 0.0, 1.0);",
            )
            .unwrap();
        let manifest = dir.join("conversion-manifest.json");
        let entries = if with_hashes {
            serde_json::json!({
                "a.bto": { "output": "meshes/terrain/tamriel/objects/tamriel.4.-8.-8.glb", "output_hash": "A".repeat(64) },
                "b.nif": { "output": "meshes/other/thing.glb", "output_hash": "0".repeat(64) },
            })
        } else {
            serde_json::json!({})
        };
        std::fs::write(
            &manifest,
            serde_json::json!({ "entries": entries }).to_string(),
        )
        .unwrap();
        (database, manifest)
    }

    #[test]
    fn any_distinct_levels_parse_and_only_nested_ones_walk() {
        assert_eq!(parse_levels("4, 8,16,32"), Some(vec![4, 8, 16, 32]));
        assert_eq!(parse_levels("12,6,36"), Some(vec![6, 12, 36]));
        assert_eq!(parse_levels("4,4"), None);
        assert_eq!(parse_levels("0,4"), None);
        assert_eq!(parse_levels("4,x"), None);
        assert_eq!(parse_levels("4,300"), None);
        assert!(level_nesting_error(&[6, 12, 36]).is_none());
        assert!(level_nesting_error(&[4]).is_none());
        let reason = level_nesting_error(&[4, 8, 16, 24]).expect("24 is not a multiple of 16");
        assert!(reason.contains("24") && reason.contains("16"), "{reason}");
    }

    #[test]
    fn the_index_keeps_valid_blocks_and_counts_every_refusal() {
        let dir = test_dir();
        let (database, manifest) = fixture(dir.path(), true);
        let connection = Connection::open(&database).unwrap();
        let index = read_index(&connection, &manifest).unwrap();
        let world = &index.worlds[&60];
        assert_eq!(world.origin, IVec2::new(-96, -96));
        assert_eq!(world.levels, vec![4, 8, 16]);
        // Only the block the manifest vouches for survives: -7,-8 is off the grid (and not in the
        // manifest), ../escape.glb leaves the assets root, level 32 is not in the grid's list.
        assert_eq!(world.block_count(), 1);
        let block = world.block(4, IVec2::new(-8, -8)).unwrap();
        assert_eq!(block.content_hash, "a".repeat(64));
        assert_eq!(block.bounds_max, [-16000.0, -16000.0, 900.0]);
        assert_eq!(index.refused_rows, 3, "{:?}", index.refused);
        assert!(world.tree_types.contains_key(&0));
        assert!(world.tree_blocks.contains(&IVec2::new(-8, -8)));
        assert_eq!(world.coarsest_level(), Some(4));
        assert_eq!(world.finer_level(8), Some(4));
    }

    #[test]
    fn a_block_the_manifest_does_not_know_is_refused() {
        let dir = test_dir();
        let (database, manifest) = fixture(dir.path(), false);
        let connection = Connection::open(&database).unwrap();
        let index = read_index(&connection, &manifest).unwrap();
        assert_eq!(index.worlds[&60].block_count(), 0);
        assert!(
            index
                .refused
                .iter()
                .any(|reason| reason.contains("not in the conversion manifest"))
        );
    }

    #[test]
    fn conflicting_manifest_outputs_fail_in_either_source_order() {
        let dir = test_dir();
        let first = "A".repeat(64);
        let second = "b".repeat(64);
        let mut reasons = Vec::new();
        for hashes in [
            [first.as_str(), second.as_str()],
            [second.as_str(), first.as_str()],
        ] {
            let manifest = manifest_with_hashes(dir.path(), &hashes);
            let error = read_manifest_hashes(&manifest).unwrap_err();
            let reason = error.to_string();
            assert!(reason.contains("conflicting SHA-256"), "{reason}");
            reasons.push(reason);
        }
        assert_eq!(reasons[0], reasons[1]);
    }

    #[test]
    fn identical_manifest_outputs_allow_case_variants_in_either_source_order() {
        let dir = test_dir();
        let upper = "A".repeat(64);
        let lower = upper.to_ascii_lowercase();
        for hashes in [
            [upper.as_str(), lower.as_str()],
            [lower.as_str(), upper.as_str()],
        ] {
            let manifest = manifest_with_hashes(dir.path(), &hashes);
            let hashes = read_manifest_hashes(&manifest).unwrap();
            assert_eq!(hashes.len(), 1);
            assert_eq!(hashes["meshes/terrain/test/objects/test.4.0.0.glb"], lower);
        }
    }

    #[test]
    fn malformed_manifest_sha256_digests_are_rejected() {
        let dir = test_dir();
        for digest in [
            String::new(),
            "abcd".to_owned(),
            "a".repeat(63),
            "a".repeat(65),
            "g".repeat(64),
        ] {
            let manifest = manifest_with_hashes(dir.path(), &[&digest]);
            let error = read_manifest_hashes(&manifest).unwrap_err();
            assert!(error.to_string().contains("invalid SHA-256"), "{error}");
        }
    }

    #[test]
    fn a_database_without_the_tables_is_an_empty_index_not_an_error() {
        let dir = test_dir();
        let database = dir.path().join("skyrim_world.db");
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch("CREATE TABLE schema_info (version INTEGER);")
            .unwrap();
        let index = read_index(&connection, &dir.path().join("missing.json")).unwrap();
        assert!(index.is_empty());
        assert_eq!(index.refused_rows, 0);
    }

    #[test]
    fn tree_instances_read_in_a_stable_order_and_a_bad_row_fails_the_block() {
        let dir = test_dir();
        let (database, manifest) = fixture(dir.path(), true);
        let connection = Connection::open(&database).unwrap();
        let index = read_index(&connection, &manifest).unwrap();
        let instances = read_tree_block(&connection, &index, 60, IVec2::new(-8, -8)).unwrap();
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].position, [-30000.0, -30000.0, 120.0]);
        assert_eq!(instances[0].scale, 1.25);
        assert!(read_tree_block(&connection, &index, 60, IVec2::new(-4, -4)).is_err());
        connection
            .execute(
                "INSERT INTO lod_tree_instance VALUES (60, -4, -4, 0, 1, 1, 1, 0, 0)",
                [],
            )
            .unwrap();
        let error = read_tree_block(&connection, &index, 60, IVec2::new(-4, -4)).unwrap_err();
        assert!(format!("{error}").contains("non-positive"), "{error}");
    }

    #[test]
    fn finite_database_bounds_that_overflow_runtime_floats_are_refused() {
        let dir = test_dir();
        let (database, manifest) = fixture(dir.path(), true);
        let connection = Connection::open(&database).unwrap();
        connection
            .execute(
                "UPDATE lod_block SET bounds_min_x = ?1, bounds_max_x = ?2 \
                 WHERE kind = 'objects' AND block_x = -8 AND block_y = -8",
                rusqlite::params![-f64::MAX, f64::MAX],
            )
            .unwrap();
        let index = read_index(&connection, &manifest).unwrap();
        assert_eq!(index.worlds[&60].block_count(), 0);
        assert!(
            index
                .refused
                .iter()
                .any(|reason| reason.contains("no finite bounds"))
        );
    }

    #[test]
    fn grid_alignment_handles_the_full_database_coordinate_range() {
        let dir = test_dir();
        let (database, manifest) = fixture(dir.path(), true);
        let connection = Connection::open(&database).unwrap();
        let block = IVec2::splat(i32::MAX - 3);
        connection
            .execute(
                "UPDATE lod_grid SET origin_x = ?1, origin_y = ?1",
                [i32::MIN],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE lod_block SET block_x = ?1, block_y = ?1 \
                 WHERE kind = 'objects' AND block_x = -8 AND block_y = -8",
                [block.x],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE lod_tree_instance SET block_x = ?1, block_y = ?1",
                [block.x],
            )
            .unwrap();
        let index = read_index(&connection, &manifest).unwrap();
        let world = &index.worlds[&60];
        assert!(world.block(4, block).is_some());
        assert!(world.tree_blocks.contains(&block));
    }

    #[test]
    fn a_tree_type_from_another_world_does_not_validate_an_instance() {
        let dir = test_dir();
        let (database, manifest) = fixture(dir.path(), true);
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(
                "INSERT INTO lod_tree_type VALUES (61, 7, 'meshes/terrain/other/trees/other.tree.7.glb', 1, 1, 0, 0, 1, 1);
                 INSERT INTO lod_tree_instance VALUES (60, -8, -8, 7, 1, 1, 1, 0, 1);",
            )
            .unwrap();
        let index = read_index(&connection, &manifest).unwrap();
        let error = read_tree_block(&connection, &index, 60, IVec2::new(-8, -8)).unwrap_err();
        assert!(format!("{error}").contains("not listed"), "{error}");
    }

    #[test]
    fn an_unsafe_tree_type_cannot_use_another_worlds_accepted_type() {
        let dir = test_dir();
        let (database, manifest) = fixture(dir.path(), true);
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(
                "UPDATE lod_tree_type SET mesh_path = '../escape.glb' WHERE worldspace_id = 60;
                 INSERT INTO lod_grid VALUES (61, -96, -96, '4,8,16');
                 INSERT INTO lod_tree_type VALUES (61, 0, 'meshes/terrain/other/trees/other.tree.0.glb', 1, 1, 0, 0, 1, 1);",
            )
            .unwrap();
        let index = read_index(&connection, &manifest).unwrap();
        assert!(!index.worlds[&60].tree_types.contains_key(&0));
        assert!(index.worlds[&61].tree_types.contains_key(&0));
        let error = read_tree_block(&connection, &index, 60, IVec2::new(-8, -8)).unwrap_err();
        assert!(
            error.to_string().contains("validated worldspace"),
            "{error}"
        );
    }

    #[test]
    fn tree_runtime_float_overflow_and_scale_underflow_fail_the_whole_block() {
        let dir = test_dir();
        let (database, manifest) = fixture(dir.path(), true);
        let connection = Connection::open(&database).unwrap();
        let index = read_index(&connection, &manifest).unwrap();
        let valid = [1.0, 2.0, 3.0, 0.5, 1.0];
        let mut cases = Vec::new();
        for field in 0..valid.len() {
            let mut values = valid;
            values[field] = f64::MAX;
            cases.push(values);
        }
        let mut underflow = valid;
        underflow[4] = f64::MIN_POSITIVE;
        cases.push(underflow);
        for values in cases {
            connection
                .execute("DELETE FROM lod_tree_instance", [])
                .unwrap();
            connection
                .execute(
                    "INSERT INTO lod_tree_instance VALUES (60, -8, -8, 0, ?1, ?2, ?3, ?4, ?5)",
                    values,
                )
                .unwrap();
            let error = read_tree_block(&connection, &index, 60, IVec2::new(-8, -8)).unwrap_err();
            assert!(format!("{error}").contains("non-positive"), "{error}");
        }
    }

    #[test]
    fn lod_paths_must_stay_under_meshes_terrain() {
        assert!(is_lod_mesh_path(
            "meshes/terrain/tamriel/objects/tamriel.4.-8.-8.glb"
        ));
        for bad in [
            "meshes/terrain/../../x.glb",
            "/meshes/terrain/x.glb",
            "meshes\\terrain\\x.glb",
            "meshes/terrain/x.nif",
            "textures/terrain/x.glb",
            "meshes/terrain/c:/x.glb",
        ] {
            assert!(!is_lod_mesh_path(bad), "{bad}");
        }
    }
}
