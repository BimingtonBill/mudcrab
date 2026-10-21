//! Distant object LOD from the package's Skyrim inventory: the `.bto` blocks the converter
//! publishes as GLBs, drawn the way Skyrim draws them (the baked side of the cluster comparison,
//! ADR-0013; the `baked` conversion mode).
//!
//! The terrain LOD ([`super::lod`]) streams chunks keyed `(world, tier, anchor)` from
//! `lod_chunks`. Object blocks come from `lod_block` and follow Skyrim's rules instead: blocks of
//! 4, 8, 16 (and 32) cells on the worldspace's LOD grid, a quadtree in which a coarser block
//! splits into finer ones as the camera gets close, with Skyrim's block distances as the reach of
//! each level (`fBlockLevel0Distance`, `fBlockLevel1Distance`, `fBlockMaximumDistance`, no
//! multiplier - the terrain's multiplier is the object distance times `fSplitDistanceMult`).
//!
//! What is reused from the terrain LOD, not rewritten: the commit budget and its LOD priority,
//! the failure classes and the 1/2/4 s bounded retries ([`LodChunkFailureClass`]), the SHA-256
//! check of the payload against the manifest, the render layers of the LOD ring (the water
//! reflection pass sees it), the origin rebase (`LodChunkGridOrigin`) and the paced unload that
//! [`super::despawn_cells`] shares with cells and terrain chunks.
//!
//! **Handoff.** Main-world state controls residency only. All validated candidates remain
//! available to `native_lod`, which selects coarse/fine blocks and near-cell suppression
//! independently for every render view using current payload/material/pass proof.
//!
//! **Not drawn here:** trees ([`super::lod_trees`]), the terrain of a `.btr` (terrain chunks come
//! from `lod_chunks`), interiors (distant LOD follows the exterior the player is in, like terrain
//! LOD), and a block the manifest does not vouch for ([`crate::world::object_lod`]).

#[cfg(test)]
use super::lod::LodStreaming;
use super::{
    RenderOrigin, StreamingCommitBudget, StreamingMetrics,
    lod::{
        LodChunkFailure, LodChunkGridOrigin, LodDemand, LodDemandViews,
        classify_asset_load_failure, lod_retry_deadline, lod_ring_layers, verify_payload_hash,
    },
};
use crate::{
    config::EngineConfig,
    graphics_settings::GraphicsSettings,
    profiling::ProfilingState,
    world::{
        components::CELL_SIZE,
        object_lod::{IndexState, ObjectLodBlock, ObjectLodSource, ObjectLodWorld},
    },
};
use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    camera::primitives::MeshAabb,
    gltf::{GltfAssetLabel, GltfExtras},
    light::NotShadowCaster,
    prelude::*,
    tasks::{IoTaskPool, Task, block_on},
    world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

/// The margin, in cells, a resident block keeps past the reach it was selected at before it
/// retires: the same two cells terrain chunks keep, so the camera standing on a level boundary
/// does not load and unload a block every frame.
const OBJECT_UNLOAD_MARGIN_CELLS: i32 = 2;

/// How far a scene's measured bounds may differ from the bounds the converter recorded, per axis
/// in world units. The converter measures the vertices it wrote; the engine measures the meshes
/// Bevy built, through the node transforms, in `f32`. Measured on all 1,078 blocks of a full
/// conversion the two agree within 0.02.
const OBJECT_BOUNDS_TOLERANCE: f32 = 1.0;

/// A block's place in the quadtree: its level (cells per side) and south-west cell.
pub(super) type BlockKey = (u8, IVec2);

// ---------------------------------------------------------------------------------------------
// The plan: which blocks must be resident, which draw. Pure, so it is tested without an `App`.
// ---------------------------------------------------------------------------------------------

/// The result of one walk of the block quadtree.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ObjectPlan {
    /// The blocks that must be resident: the ones the camera wants at its distance, and the
    /// coarser block a split falls back to until its finer blocks are ready.
    pub(super) needed: HashSet<BlockKey>,
    /// The ready blocks that draw. Never two of them over the same cell.
    pub(super) shown: HashSet<BlockKey>,
}

/// The Chebyshev distance, in cells, from `center` to the nearest cell of the block.
pub(super) fn rect_distance(center: IVec2, block: IVec2, level: u8) -> i32 {
    let far = block + IVec2::splat(i32::from(level) - 1);
    let dx = (block.x - center.x).max(center.x - far.x).max(0);
    let dy = (block.y - center.y).max(center.y - far.y).max(0);
    dx.max(dy)
}

/// The cells of a block.
fn block_cells(level: u8, block: IVec2) -> impl Iterator<Item = IVec2> {
    let side = i32::from(level);
    (0..side).flat_map(move |dx| (0..side).map(move |dy| block + IVec2::new(dx, dy)))
}

/// Whether a shown block of `max_level` or finer draws the cell.
fn shown_covers(cell: IVec2, max_level: u8, shown: &HashSet<BlockKey>) -> bool {
    shown.iter().any(|&(level, block)| {
        level <= max_level
            && cell.x >= block.x
            && cell.y >= block.y
            && cell.x < block.x + i32::from(level)
            && cell.y < block.y + i32::from(level)
    })
}

/// How the blocks that stopped drawing between two plans were accounted for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Departures {
    pub left: u64,
    pub coarsened: u64,
    pub holes: u64,
}

/// Structural pop-in between two shown sets. A block replaced by the same or a finer level is a
/// refinement; replaced by a coarser one is a planned step back; replaced by nothing while still
/// inside its own reach, over cells the package has finer data for and no near cell draws, is a
/// hole. The loader and the per-view render filter both count with this one rule.
pub(super) fn audit_departures(
    world: &ObjectLodWorld,
    previous: &HashSet<BlockKey>,
    shown: &HashSet<BlockKey>,
    covered: &dyn Fn(IVec2) -> bool,
    center: IVec2,
    reach: &dyn Fn(u8) -> i32,
) -> Departures {
    let mut out = Departures::default();
    for &(level, block) in previous.difference(shown) {
        out.left += 1;
        let drawn = |max_level: u8| {
            block_cells(level, block)
                .all(|cell| covered(cell) || shown_covers(cell, max_level, shown))
        };
        if drawn(level) {
            continue;
        }
        // A cell no finer block exists for has nothing more to draw: the package has no
        // level-8 block under an area of a level-16 block that holds nothing finer.
        let finer_data = |cell: IVec2| {
            world.levels.iter().any(|&finer| {
                let side = i32::from(finer);
                let aligned =
                    |cell: i32, origin: i32| origin + (cell - origin).div_euclid(side) * side;
                let corner = IVec2::new(
                    aligned(cell.x, world.origin.x),
                    aligned(cell.y, world.origin.y),
                );
                finer < level && world.block(finer, corner).is_some()
            })
        };
        let accounted = block_cells(level, block)
            .all(|cell| covered(cell) || shown_covers(cell, u8::MAX, shown) || !finer_data(cell));
        if drawn(u8::MAX) {
            out.coarsened += 1;
        } else if !accounted && rect_distance(center, block, level) <= reach(level) {
            out.holes += 1;
            debug!(
                level,
                ?block,
                ?center,
                "distant object LOD hole: a block left with nothing drawn over it"
            );
        }
    }
    out
}

/// The Chebyshev distance, in cells, from `center` to the farthest cell of the block.
fn rect_far_distance(center: IVec2, block: IVec2, level: u8) -> i32 {
    let far = block + IVec2::splat(i32::from(level) - 1);
    [
        (block.x - center.x).abs(),
        (far.x - center.x).abs(),
        (block.y - center.y).abs(),
        (far.y - center.y).abs(),
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

/// What a walked node leaves on screen over its area.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cover {
    /// Drawn at the level the camera wants, or nothing is there to draw.
    Best,
    /// Fully drawn, but by a coarser ready block standing in for blocks still loading.
    Fallback,
    /// Not fully drawn: some part waits for a block that is not ready.
    Partial,
}

struct Selector<'a> {
    world: &'a ObjectLodWorld,
    center: IVec2,
    reach: &'a dyn Fn(u8) -> i32,
    margin: i32,
    ready: &'a HashSet<BlockKey>,
    /// The blocks that drew on the last walk.
    previous: &'a HashSet<BlockKey>,
    /// Every block that has a block of `previous` somewhere inside it: where finer detail drew.
    finer_before: HashSet<BlockKey>,
    /// Blocks whose every cell passes `covered` are skipped, but only when the farthest cell of
    /// the block is within `covered_radius` of the camera: a cheap test first, the cell walk only
    /// for blocks near enough to be covered at all.
    covered_radius: i32,
    covered: &'a dyn Fn(IVec2) -> bool,
    needed: HashSet<BlockKey>,
}

impl Selector<'_> {
    fn is_covered(&self, level: u8, block: IVec2) -> bool {
        if rect_far_distance(self.center, block, level) > self.covered_radius {
            return false;
        }
        let side = i32::from(level);
        (0..side).all(|dy| (0..side).all(|dx| (self.covered)(block + IVec2::new(dx, dy))))
    }

    /// Walks one node and reports how its area is covered. `shown` collects what draws there.
    ///
    /// `inside_split` is set for the children of a block that split: their area is already inside
    /// the reach of the block above, so a child past its own level's reach still draws rather than
    /// leaving a hole in that block's area.
    fn node(
        &mut self,
        level: u8,
        block: IVec2,
        inside_split: bool,
        shown: &mut Vec<BlockKey>,
    ) -> Cover {
        let distance = rect_distance(self.center, block, level);
        let key = (level, block);
        let own = self.world.block(level, block).is_some();
        if !inside_split && distance > (self.reach)(level) + self.margin {
            return Cover::Best;
        }
        if self.is_covered(level, block) {
            // Every cell under it draws its own models, so the block draws nothing, but it stays
            // resident: the moment those cells leave, it is what draws, with no load in between.
            if own {
                self.needed.insert(key);
            }
            return Cover::Best;
        }
        let finer = self.world.finer_level(level);
        let split = finer.is_some_and(|finer| distance <= (self.reach)(finer) + self.margin);
        if split {
            let finer = finer.expect("a split has a finer level");
            let step = i32::from(finer);
            let mut child_shown = Vec::new();
            let mut worst = Cover::Best;
            let mut partial = false;
            for dy in 0..i32::from(level) / step {
                for dx in 0..i32::from(level) / step {
                    let child = block + IVec2::new(dx * step, dy * step);
                    match self.node(finer, child, true, &mut child_shown) {
                        Cover::Best => {}
                        Cover::Fallback => worst = Cover::Fallback,
                        Cover::Partial => partial = true,
                    }
                }
            }
            if !partial {
                // Every child area is drawn, some perhaps by their own fallback.
                shown.extend(child_shown);
                return worst;
            }
            // Part of a child area is a hole: this block stays resident and draws instead, so
            // the area is never a hole and never both. Without a ready block of its own the
            // ready finer blocks draw alone.
            //
            // The exception is detail that was drawing a moment ago: swapping it for this
            // coarser block because a neighbour has not arrived would make a nearby structure
            // vanish for the load's duration, so the ready finer blocks keep drawing and the
            // late block's area waits, as any newly entering area does.
            if own {
                self.needed.insert(key);
                if self.ready.contains(&key) {
                    if self.previous.contains(&key) || !self.finer_before.contains(&key) {
                        shown.push(key);
                    } else {
                        shown.extend(child_shown);
                    }
                    return Cover::Fallback;
                }
            }
            shown.extend(child_shown);
            return Cover::Partial;
        }
        if !own {
            return Cover::Best;
        }
        self.needed.insert(key);
        if self.ready.contains(&key) {
            shown.push(key);
            return Cover::Best;
        }
        // Merging back: the finer blocks the camera has just left keep drawing until this one is
        // ready, and stay resident meanwhile.
        self.ready_descendants(level, block, shown);
        Cover::Partial
    }

    fn ready_descendants(&mut self, level: u8, block: IVec2, shown: &mut Vec<BlockKey>) {
        let Some(finer) = self.world.finer_level(level) else {
            return;
        };
        let step = i32::from(finer);
        for dy in 0..i32::from(level) / step {
            for dx in 0..i32::from(level) / step {
                let child = block + IVec2::new(dx * step, dy * step);
                let key = (finer, child);
                if self.world.block(finer, child).is_some() && self.ready.contains(&key) {
                    self.needed.insert(key);
                    shown.push(key);
                } else {
                    self.ready_descendants(finer, child, shown);
                }
            }
        }
    }
}

/// Walks the block quadtree of `world` for a camera in cell `center`.
///
/// `reach(level)` is how many cells out a block of that level draws; a block within
/// `reach(finer level)` of the camera splits into the finer level instead. `margin` widens every
/// distance test, so the walk with a margin is the set to *keep* and the walk without one the set
/// to *show*. `ready` holds the blocks that have been validated and can draw; `previous` the
/// blocks that drew on the last walk, so detail that was on screen is not swapped for a coarser
/// block while a neighbour loads.
#[allow(clippy::too_many_arguments)]
pub(super) fn select_blocks(
    world: &ObjectLodWorld,
    center: IVec2,
    reach: &dyn Fn(u8) -> i32,
    margin: i32,
    ready: &HashSet<BlockKey>,
    previous: &HashSet<BlockKey>,
    covered_radius: i32,
    covered: &dyn Fn(IVec2) -> bool,
) -> ObjectPlan {
    let Some(root_level) = world.coarsest_level() else {
        return ObjectPlan::default();
    };
    let side = i32::from(root_level);
    let extent = reach(root_level) + margin;
    let first = |center: i32, origin: i32| {
        origin + (center - extent - side - origin).div_euclid(side) * side
    };
    let mut finer_before = HashSet::new();
    for &(level, block) in previous {
        for &coarser in world.levels.iter().filter(|coarser| **coarser > level) {
            let side = i32::from(coarser);
            let aligned = |cell: i32, origin: i32| origin + (cell - origin).div_euclid(side) * side;
            finer_before.insert((
                coarser,
                IVec2::new(
                    aligned(block.x, world.origin.x),
                    aligned(block.y, world.origin.y),
                ),
            ));
        }
    }
    let mut selector = Selector {
        world,
        center,
        reach,
        margin,
        ready,
        previous,
        finer_before,
        covered_radius,
        covered,
        needed: HashSet::new(),
    };
    let mut shown = Vec::new();
    let mut y = first(center.y, world.origin.y);
    while y <= center.y + extent {
        let mut x = first(center.x, world.origin.x);
        while x <= center.x + extent {
            selector.node(root_level, IVec2::new(x, y), false, &mut shown);
            x += side;
        }
        y += side;
    }
    ObjectPlan {
        needed: selector.needed,
        shown: shown.into_iter().collect(),
    }
}

/// The absolute cell that owns `lodSegment` index `segment` (0..15) of the level-4 block whose
/// south-west cell is `block`: `segment = 4 * dx + dy`, x the major axis, as Skyrim's `.bto`
/// layout and the converter's contract have it (`docs/specs/converters/nif-to-gltf.md`,
/// "Distant object LOD segments").
pub(super) fn segment_owner_cell(block: IVec2, segment: u8) -> IVec2 {
    block + IVec2::new(i32::from(segment / 4), i32::from(segment % 4))
}

/// Reads `extras.openSkyrim.lodSegment` from a primitive's glTF extras. `Err` for a value that is
/// present but not 0..15, `Ok(None)` for a primitive that belongs to the whole block.
pub(super) fn lod_segment_extra(extras: &GltfExtras) -> Result<Option<u8>, String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&extras.value) else {
        return Ok(None);
    };
    match value.pointer("/openSkyrim/lodSegment") {
        None => Ok(None),
        Some(segment) => segment
            .as_u64()
            .and_then(|segment| u8::try_from(segment).ok())
            .filter(|segment| *segment < 16)
            .map(Some)
            .ok_or_else(|| format!("lodSegment {segment} is not an integer from 0 to 15")),
    }
}

// ---------------------------------------------------------------------------------------------
// Streaming state
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
enum BlockStatus {
    /// The root is spawned and its payload is loading or being checked.
    Loading { root: Entity },
    /// Validated; draws when the plan shows it.
    Ready { root: Entity },
    /// No longer wanted: hidden, waiting for the unload pacer. A block wanted again before its
    /// turn is revived as it is.
    Retiring { root: Entity, ready: bool },
    /// The load or validation failed. `retry_at` is the next attempt of a transient failure;
    /// `None` is terminal: the block stays out of the ready set and its coarser parent draws.
    Failed {
        retry_count: u8,
        retry_at: Option<Instant>,
    },
}

#[derive(Resource, Default)]
pub(super) struct ObjectLodStreaming {
    index: Option<Arc<crate::world::object_lod::ObjectLodIndex>>,
    /// The index failed to read; logged once.
    index_failed: bool,
    inventory_expected: bool,
    worldspace_id: Option<u32>,
    center: Option<IVec2>,
    blocks: HashMap<BlockKey, BlockStatus>,
    /// The blocks the last walk wants resident, nearest first, that have no root yet.
    spawn_queue: Vec<BlockKey>,
    needed: HashSet<BlockKey>,
    demand: Option<LodDemand>,
    plan_dirty: bool,
    stopped: bool,
    /// The exterior whose blocks `--lod-keep-hidden-in-interiors` is holding hidden and resident
    /// while the camera is in an interior. A return to the same worldspace revives them; any
    /// other plan retires them.
    held_worldspace: Option<u32>,
    shown: HashSet<BlockKey>,
    /// Blocks of the worldspace the player left: never revived, whatever the new walk wants, since
    /// a block key means a different place in another worldspace. Cleared as the pacer despawns.
    stale: HashSet<BlockKey>,
}

#[derive(Clone)]
pub(super) struct ObjectRenderState {
    pub demand: LodDemand,
    pub index: Arc<crate::world::object_lod::ObjectLodIndex>,
    pub roots: HashMap<Entity, BlockKey>,
}

impl ObjectLodStreaming {
    /// The roots waiting for the unload pacer, in key order so a replay unloads the same blocks
    /// on the same frames (which frame depends on machine speed, as for cells).
    pub(super) fn retiring_blocks(&self) -> Vec<(BlockKey, Entity)> {
        let mut retiring: Vec<_> = self
            .blocks
            .iter()
            .filter_map(|(key, status)| match status {
                BlockStatus::Retiring { root, .. } => Some((*key, *root)),
                _ => None,
            })
            .collect();
        retiring.sort_by_key(|(key, _)| (key.0, key.1.x, key.1.y));
        retiring
    }

    /// Forgets the block whose root the unload pacer has just despawned.
    pub(super) fn block_despawned(&mut self, key: BlockKey) {
        self.blocks.remove(&key);
        self.stale.remove(&key);
        self.plan_dirty = true;
    }

    pub(super) fn render_state(&self) -> Option<ObjectRenderState> {
        Some(ObjectRenderState {
            demand: self.demand?,
            index: self.index.as_ref()?.clone(),
            roots: self
                .blocks
                .iter()
                .filter_map(|(key, status)| match status {
                    BlockStatus::Ready { root } => Some((*root, *key)),
                    _ => None,
                })
                .collect(),
        })
    }

    fn ready_keys(&self) -> HashSet<BlockKey> {
        self.blocks
            .iter()
            .filter_map(|(key, status)| matches!(status, BlockStatus::Ready { .. }).then_some(*key))
            .collect()
    }
}

#[derive(Component)]
pub(super) struct ObjectLodRoot {
    world: u32,
    key: BlockKey,
    retry_count: u8,
}

#[derive(Component)]
struct PendingObjectBlock {
    block: ObjectLodBlock,
    asset: Handle<WorldAsset>,
    scene_spawned: bool,
    hash_task: Option<Task<Result<(), LodChunkFailure>>>,
    hash_verified: bool,
    started: Instant,
}

fn mark_object_instance_ready(
    ready: On<WorldInstanceReady>,
    mut pending: Query<&mut PendingObjectBlock>,
) {
    if let Ok(mut pending) = pending.get_mut(ready.entity) {
        pending.scene_spawned = true;
    }
}

pub struct ObjectLodPlugin;

impl Plugin for ObjectLodPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ObjectLodStreaming>()
            .add_observer(mark_object_instance_ready)
            .add_systems(Startup, open_object_lod_source)
            .add_systems(
                Update,
                poll_object_lod_source.before(super::lod::plan_lod_chunks),
            )
            .add_systems(
                Update,
                (
                    plan_object_lod
                        .after(super::lod::plan_lod_chunks)
                        .before(super::collect_cells),
                    collect_object_lod
                        .after(super::lod::collect_lod_chunks)
                        .before(super::arm_pending_models),
                    track_object_lod
                        .after(super::lod::track_lod_readiness)
                        .before(super::update_render_origin),
                ),
            );
    }
}

/// Starts the worker that reads the package's object LOD inventory, when the package has a world
/// database. A package without the tables answers with an empty index and nothing is drawn.
#[derive(Resource)]
struct OpeningObjectSource(Task<Result<ObjectLodSource, String>>);

fn open_object_lod_source(
    mut commands: Commands,
    config: Res<EngineConfig>,
    extensions: Option<Res<crate::extensions::DataSetExtensions>>,
    mut metrics: ResMut<StreamingMetrics>,
    mut state: ResMut<ObjectLodStreaming>,
) {
    if !config.assets_dir.join("skyrim_world.db").is_file() {
        return;
    }
    let Some(data) = extensions.as_ref().map(|extensions| extensions.data()) else {
        return;
    };
    let status = data.status(shared::extensions::LOD_INVENTORY, 1..=1);
    if !status.is_present() {
        warn!(%status,"native object/tree LOD inventory unavailable");
        return;
    }
    let entry = data
        .get(shared::extensions::LOD_INVENTORY)
        .expect("present entry")
        .clone();
    let root = config.assets_dir.clone();
    state.inventory_expected = true;
    metrics.pending_lod_object_blocks = 1;
    commands.insert_resource(OpeningObjectSource(IoTaskPool::get().spawn(async move {
        let checked = (|| -> color_eyre::Result<ObjectLodSource> {
            use color_eyre::eyre::ensure;
            let expected_tables = std::collections::BTreeSet::from([
                "lod_block",
                "lod_grid",
                "lod_tree_instance",
                "lod_tree_type",
            ]);
            ensure!(
                entry
                    .owns
                    .tables
                    .iter()
                    .map(String::as_str)
                    .collect::<std::collections::BTreeSet<_>>()
                    == expected_tables,
                "native inventory table ownership differs from v1"
            );
            ensure!(
                entry.owns.gltf_extensions.is_empty(),
                "native inventory unexpectedly owns glTF extensions"
            );
            let connection = rusqlite::Connection::open_with_flags(
                root.join("skyrim_world.db"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            // The conversion manifest records the digest of every `lod_block` payload. The
            // inventory hash takes those digests as recorded instead of reading 10,000+ files at
            // startup; a record that differs from what the converter hashed fails this check.
            // The bytes are checked where they are read: an object block against its digest
            // before it draws (`verify_payload_hash`). Terrain-kind blocks are never read by
            // the engine, and a loader added for them must check them the same way. Owned files
            // with no record or not named by `lod_block` (the tree billboards) are hashed now.
            let hashes = crate::world::object_lod::read_manifest_hashes(
                &root.join("conversion-manifest.json"),
            )?;
            let mut recorded = std::collections::HashMap::new();
            let mut paths = connection.prepare("SELECT DISTINCT mesh_path FROM lod_block")?;
            for path in paths.query_map([], |row| row.get::<_, String>(0))? {
                let path = path?;
                if let Some(digest) = hashes.get(&path) {
                    recorded.insert(path, digest.clone());
                }
            }
            drop(paths);
            let actual = crate::cluster::envelope::recompute_recorded(
                &connection,
                &root,
                &entry,
                &recorded,
            )?;
            ensure!(actual == entry.sha256, "native inventory hash mismatch");
            ObjectLodSource::open_with_hashes(&root, hashes)
        })();
        checked.map_err(|error| format!("{error:#}"))
    })));
}

fn poll_object_lod_source(
    mut commands: Commands,
    mut opening: Option<ResMut<OpeningObjectSource>>,
    mut state: ResMut<ObjectLodStreaming>,
    mut metrics: ResMut<StreamingMetrics>,
) {
    let Some(opening) = opening.as_mut() else {
        return;
    };
    if !opening.0.is_finished() {
        return;
    }
    match block_on(&mut opening.0) {
        Ok(source) => {
            commands.insert_resource(source);
            metrics.pending_lod_object_blocks = 0;
        }
        Err(reason) => {
            error!(%reason,"native LOD inventory validation failed");
            metrics.pending_lod_object_blocks = 0;
            state.index_failed = true;
            metrics.unrecovered_lod_object_blocks += 1;
            commands.write_message(AppExit::error());
        }
    }
    commands.remove_resource::<OpeningObjectSource>();
}

/// The world of the index the plan reads, if the package has one for `worldspace_id`: the setting is on and the index has
/// something for the active worldspace.
fn index_world(streaming: &ObjectLodStreaming, worldspace_id: u32) -> Option<&ObjectLodWorld> {
    streaming.index.as_ref()?.worlds.get(&worldspace_id)
}

#[allow(clippy::too_many_arguments)]
fn plan_object_lod(
    mut commands: Commands,
    config: Res<EngineConfig>,
    views: LodDemandViews,
    origin: Res<RenderOrigin>,

    settings: Option<Res<GraphicsSettings>>,
    source: Option<ResMut<ObjectLodSource>>,
    mut streaming: ResMut<ObjectLodStreaming>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    let started = Instant::now();
    let Some(mut source) = source else {
        return;
    };
    if streaming.index.is_none() {
        match source.poll_index() {
            IndexState::Loading => {
                metrics.pending_lod_object_blocks = 1;
                return;
            }
            IndexState::Failed(reason) => {
                if !streaming.index_failed {
                    streaming.index_failed = true;
                    error!(%reason, "distant object LOD is off: the package's LOD inventory could not be read");
                }
                metrics.unrecovered_lod_object_blocks = 1;
                return;
            }
            IndexState::Ready(index) => {
                let index = Arc::clone(index);
                if index.refused_rows > 0 {
                    warn!(
                        refused = index.refused_rows,
                        first = ?index.refused.first(),
                        "distant object LOD rows were refused; the blocks they describe are not drawn"
                    );
                    metrics.lod_object_rows_refused = index.refused_rows;
                }
                info!(
                    worlds = index.worlds.len(),
                    blocks = index
                        .worlds
                        .values()
                        .map(ObjectLodWorld::block_count)
                        .sum::<usize>(),
                    "distant object LOD inventory read"
                );
                streaming.index = Some(index);
            }
        }
    }
    let actual = views.current(&config, origin.0);
    let demand = actual;
    let wanted_world = settings
        .as_ref()
        .is_none_or(|s| s.lod_objects)
        .then_some(demand)
        .flatten()
        .map(|d| d.world)
        .filter(|world| index_world(&streaming, *world).is_some_and(|w| w.block_count() > 0));
    if streaming.demand != demand {
        streaming.plan_dirty = true;
    }
    streaming.demand = demand;
    let Some(worldspace_id) = wanted_world else {
        if !streaming.stopped {
            streaming.stopped = true;
            info!("distant object LOD stopped: nothing to draw in the active space");
        }
        let in_interior = actual.is_none();
        let holding = config.lod_keep_hidden_in_interiors
            && in_interior
            && settings
                .as_ref()
                .is_none_or(|settings| settings.lod_objects)
            && (streaming.worldspace_id.is_some() || streaming.held_worldspace.is_some());
        // Hiding happens once, on the frame the visit starts: blocks that finish loading later
        // are spawned hidden, so a held visit costs nothing per frame.
        let hold_starts = holding && streaming.held_worldspace.is_none();
        if holding {
            streaming.held_worldspace = streaming.worldspace_id.or(streaming.held_worldspace);
        } else {
            streaming.held_worldspace = None;
        }
        streaming.worldspace_id = None;
        streaming.center = None;
        streaming.spawn_queue.clear();
        streaming.needed.clear();
        streaming.shown.clear();
        if holding {
            if hold_starts {
                hold_all(&mut commands, &streaming, &mut profiler);
            }
        } else {
            retire_all(&mut commands, &mut streaming, &mut metrics, &mut profiler);
        }
        update_counts(&streaming, &mut metrics, &mut profiler);
        profiler.record_elapsed("lod/objects_plan", started);
        return;
    };
    if streaming.stopped {
        streaming.stopped = false;
        info!(
            worldspace_id,
            "distant object LOD resumed in the active exterior"
        );
    }

    let center = demand.expect("wanted world has a demand").center;
    let held = streaming.held_worldspace.take();
    if streaming.worldspace_id != Some(worldspace_id) && held != Some(worldspace_id) {
        // Another exterior: everything of the old one retires, nothing carries over.
        streaming.stale = streaming.blocks.keys().copied().collect();
        streaming.shown.clear();
        retire_all(&mut commands, &mut streaming, &mut metrics, &mut profiler);
        streaming.plan_dirty = true;
    }
    if streaming.center != Some(center) || streaming.worldspace_id != Some(worldspace_id) {
        streaming.plan_dirty = true;
    }
    streaming.worldspace_id = Some(worldspace_id);
    streaming.center = Some(center);

    if streaming.plan_dirty {
        streaming.plan_dirty = false;
        let previous = streaming.shown.clone();
        let index = Arc::clone(streaming.index.as_ref().expect("read above"));
        let world = index.worlds.get(&worldspace_id).expect("checked above");
        let distances = config.terrain_lod;
        let reach = move |level: u8| distances.object_reach_cells(level);
        let ready = streaming.ready_keys();
        let covered = |_: IVec2| false;
        let covered_radius = config.stream_radius.max(0);
        let shown_plan = select_blocks(
            world,
            center,
            &reach,
            0,
            &ready,
            &previous,
            covered_radius,
            &covered,
        );
        let keep_plan = select_blocks(
            world,
            center,
            &reach,
            OBJECT_UNLOAD_MARGIN_CELLS,
            &ready,
            &previous,
            0,
            &|_| false,
        );
        let mut keep = keep_plan.needed;
        // Render pipelines can lag scene readiness. Keep ancestor stand-ins available
        // to the render-view selector until the area itself leaves the residency set.
        for block in world.blocks() {
            if rect_distance(center, block.block, block.level)
                <= reach(block.level) + OBJECT_UNLOAD_MARGIN_CELLS
            {
                keep.insert((block.level, block.block));
            }
        }
        keep.extend(shown_plan.needed.iter().copied());
        let shown = shown_plan.shown;
        let needed = shown_plan.needed;

        // Retire what left the keep set, revive what came back, remember what to spawn.
        let mut retired = 0u64;
        let mut revived = 0u64;
        let mut dropped = Vec::new();
        let stale = std::mem::take(&mut streaming.stale);
        for (key, status) in &mut streaming.blocks {
            match status {
                BlockStatus::Loading { root } | BlockStatus::Ready { root }
                    if !keep.contains(key) =>
                {
                    let (root, ready) = (*root, matches!(status, BlockStatus::Ready { .. }));
                    commands.entity(root).try_insert(Visibility::Hidden);
                    *status = BlockStatus::Retiring { root, ready };
                    retired += 1;
                    profiler.event(format!("object {key:?}"), "retiring", None);
                }
                BlockStatus::Retiring { root, ready }
                    if keep.contains(key) && !stale.contains(key) =>
                {
                    let (root, ready) = (*root, *ready);
                    *status = if ready {
                        BlockStatus::Ready { root }
                    } else {
                        BlockStatus::Loading { root }
                    };
                    revived += 1;
                    profiler.event(format!("object {key:?}"), "revived", None);
                }
                BlockStatus::Failed { .. } if !keep.contains(key) => dropped.push(*key),
                _ => {}
            }
        }
        streaming.stale = stale;
        for key in dropped {
            streaming.blocks.remove(&key);
        }
        if retired > 0 {
            profiler.increment("lod/object_blocks_retiring", retired);
        }
        if revived > 0 {
            profiler.increment("lod/object_blocks_revived", revived);
        }

        // Visibility of every ready block: it draws exactly when the walk shows it.
        for status in streaming.blocks.values() {
            if let BlockStatus::Ready { root } = status {
                let visibility = if demand.is_some() {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                commands.entity(*root).try_insert(visibility);
            }
        }

        // Spawn order: what the camera needs now, then what the keep margin will need next, each
        // nearest first, then the coarser level. Loading the margin ahead of the camera means a
        // block is already ready when a split or a merge needs it, so the walk never has to fall
        // back to a coarser block that lacks the detail the camera is standing next to.
        let mut queue: Vec<BlockKey> = keep
            .iter()
            .filter(|key| !streaming.blocks.contains_key(*key))
            .copied()
            .collect();
        queue.sort_by_key(|&(level, block)| {
            (
                !needed.contains(&(level, block)),
                rect_distance(center, block, level),
                std::cmp::Reverse(level),
                block.x,
                block.y,
            )
        });
        streaming.spawn_queue = queue;
        let by_level = |keys: &HashSet<BlockKey>| {
            let mut counts = BTreeMap::<u8, usize>::new();
            for &(level, _) in keys {
                *counts.entry(level).or_default() += 1;
            }
            counts
        };
        debug!(
            ?center,
            needed = ?by_level(&needed),
            shown = ?by_level(&shown),
            queued = streaming.spawn_queue.len(),
            "distant object LOD plan (blocks per level)"
        );
        if previous != shown {
            // Structural pop-in: every block that starts or stops drawing is a visible change.
            // A block replaced by the same or a finer level is a refinement; replaced by a coarser
            // one is a planned step back; replaced by nothing while still inside its own reach
            // is a hole, the defect to drive to zero. A block that left because the camera moved
            // past its reach is not a hole: the walk asked for something coarser, and where the
            // package has none the far edge of the world is simply there. Cells that draw their
            // own models count as drawn.
            let entered = shown.difference(&previous).count() as u64;
            let Departures {
                left,
                coarsened,
                holes,
            } = audit_departures(world, &previous, &shown, &covered, center, &reach);
            profiler.increment("lod/object_blocks_entered", entered);
            profiler.increment("lod/object_blocks_left", left);
            if coarsened > 0 {
                profiler.increment("lod/object_blocks_coarsened", coarsened);
            }
            if holes > 0 {
                profiler.increment("lod/object_blocks_holes", holes);
            }
        }
        streaming.needed = needed;
        streaming.shown = shown;
        if retired > 0 || revived > 0 {}
    }

    // Transient failures whose backoff passed come back through the queue.
    let now = Instant::now();
    let due: Vec<BlockKey> = streaming
        .blocks
        .iter()
        .filter_map(|(key, status)| match status {
            BlockStatus::Failed {
                retry_at: Some(retry_at),
                ..
            } if *retry_at <= now && streaming.needed.contains(key) => Some(*key),
            _ => None,
        })
        .collect();
    for key in due {
        if !streaming.spawn_queue.contains(&key) {
            streaming.spawn_queue.push(key);
        }
    }

    update_counts(&streaming, &mut metrics, &mut profiler);
    profiler.record_elapsed("lod/objects_plan", started);
}

/// Every block stays resident but hidden, for the return to the exterior
/// (`--lod-keep-hidden-in-interiors`): memory the flag holds, priced by its measurement.
fn hold_all(
    commands: &mut Commands,
    streaming: &ObjectLodStreaming,
    profiler: &mut ProfilingState,
) {
    let mut held = 0u64;
    for status in streaming.blocks.values() {
        if let BlockStatus::Loading { root } | BlockStatus::Ready { root } = status {
            commands.entity(*root).try_insert(Visibility::Hidden);
            held += 1;
        }
    }
    if held > 0 {
        profiler.increment("lod/object_blocks_held", held);
    }
}

/// Every block leaves: hidden at once, unloaded by the pacer.
fn retire_all(
    commands: &mut Commands,
    streaming: &mut ObjectLodStreaming,
    metrics: &mut StreamingMetrics,
    profiler: &mut ProfilingState,
) {
    let mut retired = 0u64;
    streaming.blocks.retain(|key, status| match status {
        BlockStatus::Loading { root } | BlockStatus::Ready { root } => {
            let root = *root;
            let ready = matches!(status, BlockStatus::Ready { .. });
            commands.entity(root).try_insert(Visibility::Hidden);
            *status = BlockStatus::Retiring { root, ready };
            retired += 1;
            profiler.event(format!("object {key:?}"), "retiring", None);
            true
        }
        BlockStatus::Retiring { .. } => true,
        BlockStatus::Failed { .. } => false,
    });
    if retired > 0 {
        profiler.increment("lod/object_blocks_retiring", retired);
    }
    let _ = metrics;
}

#[allow(clippy::too_many_arguments)]
fn collect_object_lod(
    mut commands: Commands,
    config: Res<EngineConfig>,
    asset_server: Res<AssetServer>,
    origin: Res<RenderOrigin>,
    mut streaming: ResMut<ObjectLodStreaming>,
    mut budget: ResMut<StreamingCommitBudget>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    let started = Instant::now();
    let Some(worldspace_id) = streaming.worldspace_id else {
        return;
    };
    while budget.lod_slot_open(config.max_commit_micros_per_frame) {
        let Some(key) = streaming.spawn_queue.first().copied() else {
            break;
        };
        streaming.spawn_queue.remove(0);
        let retry_count = match streaming.blocks.get(&key) {
            None => 0,
            Some(BlockStatus::Failed {
                retry_count,
                retry_at: Some(retry_at),
            }) if Instant::now() >= *retry_at => retry_count.saturating_add(1),
            Some(_) => continue,
        };

        let Some(block) = index_world(&streaming, worldspace_id)
            .and_then(|world| world.block(key.0, key.1))
            .cloned()
        else {
            continue;
        };
        let asset = asset_server.load(GltfAssetLabel::Scene(0).from_asset(block.mesh_path.clone()));
        let hash_path = config.assets_dir.join(&block.mesh_path);
        let expected_hash = block.content_hash.clone();
        let hash_task =
            IoTaskPool::get().spawn(async move { verify_payload_hash(hash_path, expected_hash) });
        let (ring_layers, ring_propagation) = lod_ring_layers();
        // Block nodes carry absolute Creation coordinates (the converter wrote the block's own
        // offset into the node), so the root is the render-origin shift alone.
        let root = commands
            .spawn((
                Name::new(format!(
                    "LOD objects {} block {},{}",
                    key.0, key.1.x, key.1.y
                )),
                ObjectLodRoot {
                    world: worldspace_id,
                    key,
                    retry_count,
                },
                LodChunkGridOrigin::world_origin(),
                WorldAssetRoot(asset.clone()),
                Transform::from_xyz(
                    -(origin.0.x as f32) * CELL_SIZE,
                    0.0,
                    origin.0.y as f32 * CELL_SIZE,
                ),
                Visibility::Hidden,
                ring_layers,
                ring_propagation,
                PendingObjectBlock {
                    block,
                    asset,
                    scene_spawned: false,
                    hash_task: Some(hash_task),
                    hash_verified: false,
                    started: Instant::now(),
                },
            ))
            .id();
        streaming.blocks.insert(key, BlockStatus::Loading { root });
        budget.spend_lod_slot();
        metrics.lod_object_blocks_requested = metrics.lod_object_blocks_requested.saturating_add(1);
        profiler.increment("lod/object_blocks_requested", 1);
        profiler.event(format!("object {key:?}"), "requested", None);
    }
    update_counts(&streaming, &mut metrics, &mut profiler);
    profiler.record_elapsed("lod/objects_collect", started);
}

type ObjectPrimitive<'a> = (&'a Mesh3d, Option<&'a GltfExtras>);

#[allow(clippy::too_many_arguments)]
fn track_object_lod(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    world_assets: Res<Assets<WorldAsset>>,
    meshes: Res<Assets<Mesh>>,
    children: Query<&Children>,
    transforms: Query<&Transform>,
    primitives: Query<ObjectPrimitive>,
    mut pending_blocks: Query<(Entity, &ObjectLodRoot, &mut PendingObjectBlock)>,
    mut streaming: ResMut<ObjectLodStreaming>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    let started = Instant::now();
    for (entity, root, mut pending) in &mut pending_blocks {
        let load_failure = asset_server.get_load_states(pending.asset.id()).and_then(
            |(load, _, recursive)| match (load, recursive) {
                (LoadState::Failed(error), _) => Some(classify_asset_load_failure(&error)),
                (_, RecursiveDependencyLoadState::Failed(error)) => {
                    Some(classify_asset_load_failure(&error))
                }
                _ => None,
            },
        );
        if let Some(failure) = load_failure {
            fail_block(
                entity,
                root,
                failure,
                &mut commands,
                &mut streaming,
                &mut metrics,
                &mut profiler,
            );
            continue;
        }
        if !pending.hash_verified {
            let Some(task) = pending.hash_task.as_ref() else {
                continue;
            };
            if !task.is_finished() {
                continue;
            }
            let task = pending.hash_task.take().expect("finished hash task exists");
            match block_on(task) {
                Ok(()) => pending.hash_verified = true,
                Err(failure) => {
                    fail_block(
                        entity,
                        root,
                        failure,
                        &mut commands,
                        &mut streaming,
                        &mut metrics,
                        &mut profiler,
                    );
                    continue;
                }
            }
        }
        if !pending.scene_spawned
            || !asset_server.is_loaded_with_dependencies(pending.asset.id())
            || world_assets.get(&pending.asset).is_none()
        {
            continue;
        }
        let marks = match validate_object_scene(
            entity,
            &pending.block,
            &children,
            &transforms,
            &primitives,
            &meshes,
        ) {
            Ok(marks) => marks,
            Err(reason) => {
                fail_block(
                    entity,
                    root,
                    LodChunkFailure::terminal(reason),
                    &mut commands,
                    &mut streaming,
                    &mut metrics,
                    &mut profiler,
                );
                continue;
            }
        };
        let mut segments = 0u64;
        for mark in &marks {
            // Shadow cascades cover the full-detail grid only; a distant block casting would
            // double every shadow in the overlap and cost a cascade pass for nothing.
            let mut entity_commands = commands.entity(mark.entity);
            entity_commands.try_insert(NotShadowCaster);
            if mark.excluded {
                entity_commands.try_insert(Visibility::Hidden);
                metrics.lod_object_excluded_primitives =
                    metrics.lod_object_excluded_primitives.saturating_add(1);
            } else if let Some(segment) = mark.segment {
                entity_commands.try_insert((super::native_lod::NativeLodCell {
                    world: root.world,
                    cell: segment_owner_cell(pending.block.block, segment),
                },));
                segments += 1;
            }
        }
        commands.entity(entity).remove::<PendingObjectBlock>();
        let key = root.key;
        match streaming.blocks.get(&key).cloned() {
            Some(BlockStatus::Loading { root: expected }) if expected == entity => {
                streaming
                    .blocks
                    .insert(key, BlockStatus::Ready { root: entity });
            }
            // Left the keep set while loading: stays hidden, and becomes ready when revived.
            Some(BlockStatus::Retiring { root: expected, .. }) if expected == entity => {
                streaming.blocks.insert(
                    key,
                    BlockStatus::Retiring {
                        root: entity,
                        ready: true,
                    },
                );
            }
            _ => {}
        }
        streaming.plan_dirty = true;
        metrics.lod_object_blocks_ready = metrics.lod_object_blocks_ready.saturating_add(1);
        metrics.lod_object_segments = metrics.lod_object_segments.saturating_add(segments);
        profiler.increment("lod/object_blocks_ready", 1);
        profiler.record_elapsed("lod/object_block_ready", pending.started);
        profiler.event(format!("object {key:?}"), "ready", None);
    }
    update_counts(&streaming, &mut metrics, &mut profiler);
    profiler.record_elapsed("lod/objects_readiness", started);
}

fn fail_block(
    entity: Entity,
    root: &ObjectLodRoot,
    failure: LodChunkFailure,
    commands: &mut Commands,
    streaming: &mut ObjectLodStreaming,
    metrics: &mut StreamingMetrics,
    profiler: &mut ProfilingState,
) {
    let retry_at = lod_retry_deadline(failure.class, root.retry_count, Instant::now());
    warn!(
        key = ?root.key,
        class = ?failure.class,
        reason = %failure.reason,
        retrying = retry_at.is_some(),
        "distant object LOD block failed validation or loading"
    );
    metrics.lod_object_blocks_failed = metrics.lod_object_blocks_failed.saturating_add(1);
    profiler.increment("lod/object_blocks_failed", 1);
    profiler.event(format!("object {:?}", root.key), "failed", None);
    match streaming.blocks.get(&root.key).cloned() {
        Some(BlockStatus::Loading { root: expected }) if expected == entity => {
            streaming.blocks.insert(
                root.key,
                BlockStatus::Failed {
                    retry_count: root.retry_count,
                    retry_at,
                },
            );
            commands.entity(entity).try_despawn();
        }
        // Already leaving: the root is the unload pacer's.
        Some(BlockStatus::Retiring { .. }) => {}
        _ => commands.entity(entity).try_despawn(),
    }
    streaming.plan_dirty = true;
}

/// One mesh-bearing entity of a block scene, with what the converter said about it.
struct PrimitiveMark {
    entity: Entity,
    segment: Option<u8>,
    excluded: bool,
}

/// Checks a spawned block against its `lod_block` row and finds its per-cell segments.
///
/// The scene must hold at least one mesh, and the bounds of everything it holds, through the node
/// transforms and the Creation-to-glTF basis, must match the bounds the converter recorded: a
/// block that does not is a different file than the one the index describes. A segment index that
/// is not 0..15, or any segment on a block that is not level 4, is refused.
fn validate_object_scene(
    root: Entity,
    block: &ObjectLodBlock,
    children: &Query<&Children>,
    transforms: &Query<&Transform>,
    primitives: &Query<ObjectPrimitive>,
    meshes: &Assets<Mesh>,
) -> Result<Vec<PrimitiveMark>, String> {
    let name = || format!("object block {} ({})", block.mesh_path, block.level);
    let mut marks = Vec::new();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    // Entities carry their local transform; the walk accumulates it from the root, whose own
    // placement is the render origin and not part of the block's bounds.
    let mut stack = vec![(root, Mat4::IDENTITY)];
    while let Some((entity, parent)) = stack.pop() {
        let local = if entity == root {
            Mat4::IDENTITY
        } else {
            transforms
                .get(entity)
                .map_or(Mat4::IDENTITY, |transform| transform.to_matrix())
        };
        let matrix = parent * local;
        if let Ok((mesh_handle, extras)) = primitives.get(entity) {
            let mesh = meshes
                .get(&mesh_handle.0)
                .ok_or_else(|| format!("{} has a mesh that is not loaded", name()))?;
            let aabb = mesh
                .compute_aabb()
                .ok_or_else(|| format!("{} has a mesh with no positions", name()))?;
            let (low, high): (Vec3, Vec3) = (aabb.min().into(), aabb.max().into());
            for corner in 0..8 {
                let point = Vec3::new(
                    if corner & 1 == 0 { low.x } else { high.x },
                    if corner & 2 == 0 { low.y } else { high.y },
                    if corner & 4 == 0 { low.z } else { high.z },
                );
                let point = matrix.transform_point3(point);
                min = min.min(point);
                max = max.max(point);
            }
            let segment = match extras.map(lod_segment_extra) {
                Some(Err(reason)) => return Err(format!("{}: {reason}", name())),
                Some(Ok(segment)) => segment,
                None => None,
            };
            if segment.is_some() && block.level != 4 {
                return Err(format!(
                    "{} has a per-cell segment but is not a level-4 block",
                    name()
                ));
            }
            let excluded = extras.is_some_and(super::has_explicit_material_exclusion);
            marks.push(PrimitiveMark {
                entity,
                segment,
                excluded,
            });
        }
        if let Ok(direct) = children.get(entity) {
            for child in direct.iter() {
                stack.push((child, matrix));
            }
        }
    }
    if marks.is_empty() {
        return Err(format!("{} spawned no mesh", name()));
    }
    // The index stores the bounds in runtime space already (the converter measured the
    // vertices after the Creation-to-glTF basis); all 1,078 blocks of the Fiji package match the
    // scene's own bounds to 0.02 units.
    let (expected_min, expected_max) = (Vec3::from(block.bounds_min), Vec3::from(block.bounds_max));
    if (min - expected_min).abs().max_element() > OBJECT_BOUNDS_TOLERANCE
        || (max - expected_max).abs().max_element() > OBJECT_BOUNDS_TOLERANCE
    {
        return Err(format!(
            "{} spans {min:?}..{max:?}, the index says {expected_min:?}..{expected_max:?}",
            name()
        ));
    }
    Ok(marks)
}

fn update_counts(
    streaming: &ObjectLodStreaming,
    metrics: &mut StreamingMetrics,
    profiler: &mut ProfilingState,
) {
    let mut resident = 0usize;
    let mut loading = 0usize;
    let mut retiring = 0usize;
    let mut unrecovered = 0usize;
    for status in streaming.blocks.values() {
        match status {
            BlockStatus::Ready { .. } => resident += 1,
            BlockStatus::Loading { .. } => loading += 1,
            BlockStatus::Retiring { .. } => retiring += 1,
            BlockStatus::Failed { retry_at: None, .. } => unrecovered += 1,
            BlockStatus::Failed { .. } => loading += 1,
        }
    }
    metrics.resident_lod_object_blocks = resident;
    metrics.pending_lod_object_blocks = loading
        + streaming.spawn_queue.len()
        + usize::from(
            streaming.inventory_expected && streaming.index.is_none() && !streaming.index_failed,
        );
    metrics.retiring_lod_object_blocks = retiring;
    metrics.unrecovered_lod_object_blocks = unrecovered + usize::from(streaming.index_failed);
    profiler.set_gauge("lod/object_blocks_resident", resident as f64);
    profiler.set_gauge(
        "lod/object_blocks_pending",
        metrics.pending_lod_object_blocks as f64,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reach(level: u8) -> i32 {
        // Skyrim's defaults without the multiplier: 35,000, 70,000 and 250,000 units.
        match level {
            4 => 8,
            8 => 17,
            _ => 61,
        }
    }

    fn grid(origin: IVec2) -> ObjectLodWorld {
        // One level-16 block with the four level-8 and sixteen level-4 blocks under its
        // south-west quarter.
        let mut blocks = vec![(16, origin)];
        for dy in 0..2 {
            for dx in 0..2 {
                blocks.push((8, origin + IVec2::new(dx * 8, dy * 8)));
            }
        }
        for dy in 0..4 {
            for dx in 0..4 {
                blocks.push((4, origin + IVec2::new(dx * 4, dy * 4)));
            }
        }
        ObjectLodWorld::for_test(origin, &[4, 8, 16], &blocks)
    }

    fn select(
        world: &ObjectLodWorld,
        center: IVec2,
        margin: i32,
        ready: &HashSet<BlockKey>,
    ) -> ObjectPlan {
        select_blocks(
            world,
            center,
            &reach,
            margin,
            ready,
            &HashSet::new(),
            0,
            &|_| false,
        )
    }

    fn all_blocks(world: &ObjectLodWorld) -> HashSet<BlockKey> {
        world
            .blocks()
            .map(|block| (block.level, block.block))
            .collect()
    }

    /// The cells a set of block keys covers, counting each cell once per block.
    fn cell_coverage(keys: &HashSet<BlockKey>) -> HashMap<IVec2, u32> {
        let mut cells = HashMap::new();
        for &(level, block) in keys {
            for dy in 0..i32::from(level) {
                for dx in 0..i32::from(level) {
                    *cells.entry(block + IVec2::new(dx, dy)).or_insert(0) += 1;
                }
            }
        }
        cells
    }

    #[test]
    fn departures_count_holes_only_where_finer_data_exists_and_nothing_draws() {
        let world = grid(IVec2::ZERO);
        let set = |keys: &[BlockKey]| keys.iter().copied().collect::<HashSet<_>>();
        let none = |_: IVec2| false;
        let audit = |previous: &[BlockKey], shown: &[BlockKey], covered: &dyn Fn(IVec2) -> bool| {
            audit_departures(
                &world,
                &set(previous),
                &set(shown),
                covered,
                IVec2::ZERO,
                &reach,
            )
        };
        let children: Vec<BlockKey> = (0..2)
            .flat_map(|dy| (0..2).map(move |dx| (8, IVec2::new(dx * 8, dy * 8))))
            .collect();
        // Replaced by its children: a refinement, not a hole.
        assert_eq!(audit(&[(16, IVec2::ZERO)], &children, &none).holes, 0);
        // Replaced by its parent: a planned step back.
        let step_back = audit(&[(8, IVec2::ZERO)], &[(16, IVec2::ZERO)], &none);
        assert_eq!((step_back.coarsened, step_back.holes), (1, 0));
        // Replaced by nothing inside its reach: a hole.
        assert_eq!(audit(&[(16, IVec2::ZERO)], &[], &none).holes, 1);
        // Near cells drawing their own models cover it.
        assert_eq!(audit(&[(16, IVec2::ZERO)], &[], &|_| true).holes, 0);
        // Only coarser levels than 4 under it still count (the render audit once required
        // level-4 data).
        let sparse = ObjectLodWorld::for_test(
            IVec2::ZERO,
            &[8, 16],
            &[(16, IVec2::ZERO), (8, IVec2::ZERO)],
        );
        let departures = audit_departures(
            &sparse,
            &set(&[(16, IVec2::ZERO)]),
            &HashSet::new(),
            &none,
            IVec2::ZERO,
            &reach,
        );
        assert_eq!(departures.holes, 1);
        // Out of its own reach: the far edge, not a hole.
        let far = audit_departures(
            &world,
            &set(&[(16, IVec2::ZERO)]),
            &HashSet::new(),
            &none,
            IVec2::new(500, 0),
            &reach,
        );
        assert_eq!((far.left, far.holes), (1, 0));
    }

    #[test]
    fn block_distance_is_chebyshev_to_the_nearest_cell() {
        let block = IVec2::new(8, 8);
        assert_eq!(rect_distance(IVec2::new(9, 9), block, 4), 0);
        assert_eq!(rect_distance(IVec2::new(7, 8), block, 4), 1);
        assert_eq!(rect_distance(IVec2::new(20, 9), block, 4), 9);
        assert_eq!(rect_distance(IVec2::new(0, 0), block, 4), 8);
        assert_eq!(rect_far_distance(IVec2::new(9, 9), block, 4), 2);
    }

    #[test]
    fn segment_cells_follow_the_converters_x_major_order() {
        let block = IVec2::new(-12, -16);
        assert_eq!(segment_owner_cell(block, 0), IVec2::new(-12, -16));
        assert_eq!(segment_owner_cell(block, 3), IVec2::new(-12, -13));
        assert_eq!(segment_owner_cell(block, 4), IVec2::new(-11, -16));
        assert_eq!(segment_owner_cell(block, 15), IVec2::new(-9, -13));
    }

    #[test]
    fn segment_extras_must_be_a_cell_index() {
        let extras = |text: &str| GltfExtras {
            value: text.to_owned(),
        };
        assert_eq!(
            lod_segment_extra(&extras(r#"{"openSkyrim":{"lodSegment":5}}"#)),
            Ok(Some(5))
        );
        assert_eq!(
            lod_segment_extra(&extras(r#"{"openSkyrim":{"shapeBlock":2}}"#)),
            Ok(None)
        );
        assert_eq!(lod_segment_extra(&extras("not json")), Ok(None));
        assert!(lod_segment_extra(&extras(r#"{"openSkyrim":{"lodSegment":16}}"#)).is_err());
        assert!(lod_segment_extra(&extras(r#"{"openSkyrim":{"lodSegment":"3"}}"#)).is_err());
    }

    #[test]
    fn the_camera_on_a_block_splits_down_to_level_four_and_draws_each_cell_once() {
        let world = grid(IVec2::ZERO);
        let ready = all_blocks(&world);
        let plan = select(&world, IVec2::new(2, 2), 0, &ready);
        let coverage = cell_coverage(&plan.shown);
        assert!(
            coverage.values().all(|count| *count == 1),
            "a cell draws twice"
        );
        // The level-4 blocks within 8 cells draw; past them, level 8 out to 17; the rest, 16.
        assert!(plan.shown.contains(&(4, IVec2::ZERO)));
        assert!(plan.shown.contains(&(4, IVec2::new(4, 4))));
        assert!(
            plan.shown.contains(&(8, IVec2::new(8, 8)))
                || plan.shown.contains(&(4, IVec2::new(8, 8)))
        );
        assert!(!plan.shown.contains(&(16, IVec2::ZERO)));
        // Every cell of the 16x16 block is drawn by something.
        assert_eq!(coverage.len(), 16 * 16);
    }

    #[test]
    fn a_distant_camera_draws_the_coarse_block_alone() {
        let world = grid(IVec2::ZERO);
        let ready = all_blocks(&world);
        let plan = select(&world, IVec2::new(0, 60), 0, &ready);
        assert_eq!(plan.shown, HashSet::from([(16, IVec2::ZERO)]));
        assert_eq!(plan.needed, HashSet::from([(16, IVec2::ZERO)]));
    }

    #[test]
    fn nothing_draws_past_the_farthest_reach() {
        let world = grid(IVec2::ZERO);
        let ready = all_blocks(&world);
        let plan = select(&world, IVec2::new(0, 200), 0, &ready);
        assert!(plan.shown.is_empty() && plan.needed.is_empty());
    }

    /// A package with one level-4 object block at cell 0,0 of `worldspace_id`.
    fn one_block_package(dir: &std::path::Path, worldspace_id: u32) {
        let path = "meshes/terrain/test/objects/test.4.0.0.glb";
        rusqlite::Connection::open(dir.join("skyrim_world.db"))
            .unwrap()
            .execute_batch(&format!(
                "CREATE TABLE lod_grid (worldspace_id INTEGER PRIMARY KEY, origin_x INTEGER NOT NULL, origin_y INTEGER NOT NULL, levels TEXT NOT NULL);
                 CREATE TABLE lod_block (worldspace_id INTEGER NOT NULL, kind TEXT NOT NULL, level INTEGER NOT NULL, block_x INTEGER NOT NULL, block_y INTEGER NOT NULL, mesh_path TEXT NOT NULL, bounds_min_x REAL, bounds_min_y REAL, bounds_min_z REAL, bounds_max_x REAL, bounds_max_y REAL, bounds_max_z REAL);
                 INSERT INTO lod_grid VALUES ({worldspace_id}, 0, 0, '4');
                 INSERT INTO lod_block VALUES ({worldspace_id}, 'objects', 4, 0, 0, '{path}', 0, 0, 0, 1, 1, 1);"
            ))
            .unwrap();
        std::fs::write(
            dir.join("conversion-manifest.json"),
            serde_json::json!({ "entries": { "a.bto": { "output": path, "output_hash": "a".repeat(64) } } })
                .to_string(),
        )
        .unwrap();
    }

    /// An app that runs only the object plan, with the one block already loaded and ready.
    fn held_app(dir: &std::path::Path, keep_hidden: bool) -> (App, Entity) {
        let config = EngineConfig {
            assets_dir: dir.to_path_buf(),
            lod_keep_hidden_in_interiors: keep_hidden,
            ..EngineConfig::default()
        };
        one_block_package(dir, config.worldspace_id);
        let source = ObjectLodSource::open(dir).unwrap();
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(source)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<super::super::ActiveSpace>()
            .init_resource::<LodStreaming>()
            .init_resource::<ObjectLodStreaming>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, plan_object_lod);
        app.world_mut().spawn((
            Transform::default(),
            crate::world::components::StreamingCamera,
        ));
        for _ in 0..400 {
            app.update();
            if app.world().resource::<ObjectLodStreaming>().index.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            app.world().resource::<ObjectLodStreaming>().index.is_some(),
            "the index never loaded"
        );
        let root = app.world_mut().spawn(Visibility::Inherited).id();
        app.world_mut()
            .resource_mut::<ObjectLodStreaming>()
            .blocks
            .insert((4, IVec2::ZERO), BlockStatus::Ready { root });
        app.update();
        (app, root)
    }

    #[test]
    fn the_held_lod_flag_keeps_object_blocks_through_an_interior_visit() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, root) = held_app(directory.path(), true);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Inherited),
            "the block draws in the exterior"
        );

        app.world_mut()
            .resource_mut::<super::super::ActiveSpace>()
            .interior = Some(99);
        app.update();
        let streaming = app.world().resource::<ObjectLodStreaming>();
        assert!(
            matches!(
                streaming.blocks.get(&(4, IVec2::ZERO)),
                Some(BlockStatus::Ready { .. })
            ),
            "kept for the return, not the pacer's"
        );
        assert!(streaming.retiring_blocks().is_empty());
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden),
            "hidden for as long as the visit lasts"
        );

        // Back out the same door: the same root draws again.
        app.world_mut()
            .resource_mut::<super::super::ActiveSpace>()
            .interior = None;
        app.update();
        let streaming = app.world().resource::<ObjectLodStreaming>();
        assert!(streaming.retiring_blocks().is_empty());
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Inherited),
            "the held block is shown again with nothing streamed in"
        );
    }

    #[test]
    fn without_the_held_lod_flag_an_interior_visit_retires_the_object_blocks() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, root) = held_app(directory.path(), false);
        app.world_mut()
            .resource_mut::<super::super::ActiveSpace>()
            .interior = Some(99);
        app.update();
        let streaming = app.world().resource::<ObjectLodStreaming>();
        assert_eq!(streaming.retiring_blocks(), vec![((4, IVec2::ZERO), root)]);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn levels_that_are_not_powers_of_two_walk_like_any_other() {
        // 6, 12 and 36 cells: a level-36 block holds nine level-12 blocks, each four level-6 ones.
        let origin = IVec2::new(-12, 24);
        let mut blocks = vec![(36, origin)];
        for dy in 0..3 {
            for dx in 0..3 {
                blocks.push((12, origin + IVec2::new(dx * 12, dy * 12)));
            }
        }
        for dy in 0..6 {
            for dx in 0..6 {
                blocks.push((6, origin + IVec2::new(dx * 6, dy * 6)));
            }
        }
        let world = ObjectLodWorld::for_test(origin, &[6, 12, 36], &blocks);
        let ready = all_blocks(&world);
        // Close to one corner: fine blocks there, coarser ones elsewhere, every cell drawn once.
        let reach = |level: u8| match level {
            6 => 5,
            12 => 13,
            _ => 61,
        };
        let plan = select_blocks(
            &world,
            origin + IVec2::new(1, 1),
            &reach,
            0,
            &ready,
            &HashSet::new(),
            0,
            &|_| false,
        );
        let coverage = cell_coverage(&plan.shown);
        assert!(
            coverage.values().all(|count| *count == 1),
            "a cell draws twice"
        );
        assert_eq!(coverage.len(), 36 * 36, "a hole opened");
        assert!(plan.shown.contains(&(6, origin)));
        assert!(plan.shown.iter().any(|&(level, _)| level == 12));
    }

    #[test]
    fn a_parent_keeps_drawing_until_every_finer_block_is_ready() {
        let world = grid(IVec2::ZERO);
        let mut ready = all_blocks(&world);
        // The camera moves in close; one level-4 block has not arrived.
        ready.remove(&(4, IVec2::new(4, 4)));
        let plan = select(&world, IVec2::new(2, 2), 0, &ready);
        let coverage = cell_coverage(&plan.shown);
        assert!(
            coverage.values().all(|count| *count == 1),
            "a cell draws twice"
        );
        assert_eq!(
            coverage.len(),
            16 * 16,
            "a hole opened while a block loaded"
        );
        // The level-8 block over the missing one draws as the fallback, and is kept resident.
        assert!(plan.shown.contains(&(8, IVec2::ZERO)));
        assert!(plan.needed.contains(&(8, IVec2::ZERO)));
        assert!(plan.needed.contains(&(4, IVec2::new(4, 4))));
    }

    #[test]
    fn moving_away_keeps_the_finer_blocks_until_the_coarser_one_is_ready() {
        let world = grid(IVec2::ZERO);
        // Near: everything level 4 is ready. Far: the camera now wants the level-16 block, which
        // is not ready yet.
        let mut ready: HashSet<BlockKey> = world
            .blocks()
            .filter(|block| block.level == 4)
            .map(|block| (4, block.block))
            .collect();
        let plan = select(&world, IVec2::new(0, 40), 0, &ready);
        assert!(plan.needed.contains(&(16, IVec2::ZERO)));
        assert_eq!(
            plan.shown.len(),
            16,
            "the level-4 blocks keep drawing meanwhile"
        );
        let coverage = cell_coverage(&plan.shown);
        assert!(coverage.values().all(|count| *count == 1));
        ready.insert((16, IVec2::ZERO));
        let settled = select(&world, IVec2::new(0, 40), 0, &ready);
        assert_eq!(settled.shown, HashSet::from([(16, IVec2::ZERO)]));
    }

    #[test]
    fn a_failed_block_leaves_its_coarser_parent_drawing() {
        let world = grid(IVec2::ZERO);
        let mut ready = all_blocks(&world);
        ready.remove(&(4, IVec2::ZERO));
        // The block never becomes ready, however many walks are made.
        for _ in 0..3 {
            let plan = select(&world, IVec2::new(2, 2), 0, &ready);
            assert!(plan.shown.contains(&(8, IVec2::ZERO)));
            assert!(
                !plan.shown.contains(&(4, IVec2::new(4, 0))),
                "a sibling drew beside its parent"
            );
        }
    }

    #[test]
    fn the_keep_set_is_wider_than_the_show_set_by_the_margin() {
        let world = grid(IVec2::ZERO);
        let ready = all_blocks(&world);
        // Standing 9 cells from the level-4 blocks: the level-8 block draws, and the margin keeps
        // the level-4 blocks 2 cells farther in resident.
        let center = IVec2::new(2, 17);
        let shown = select(&world, center, 0, &ready);
        let kept = select(&world, center, OBJECT_UNLOAD_MARGIN_CELLS, &ready);
        // The plan system keeps the union, so a block drawn now is never retired by the margin.
        let keep: HashSet<BlockKey> = kept.needed.union(&shown.needed).copied().collect();
        assert!(keep.is_superset(&shown.needed));
        assert!(
            kept.needed
                .iter()
                .any(|key| key.0 == 4 && !shown.needed.contains(key)),
            "the margin kept no finer block beyond the drawn set"
        );
    }

    #[test]
    fn blocks_inside_ready_cells_stay_resident_without_drawing_and_partial_ones_draw() {
        let world = grid(IVec2::ZERO);
        let ready = all_blocks(&world);
        let center = IVec2::new(1, 1);
        // Cells 0..=2 on both axes are ready (a stream radius of 1 around cell (1, 1)).
        let covered = |cell: IVec2| (0..=2).contains(&cell.x) && (0..=2).contains(&cell.y);
        let plan = select_blocks(
            &world,
            center,
            &reach,
            0,
            &ready,
            &HashSet::new(),
            1,
            &covered,
        );
        // The level-4 block over cells 0..3 is not wholly covered (cell 3 is not), so it stays.
        assert!(plan.shown.contains(&(4, IVec2::ZERO)));
        // A fully covered block draws nothing but stays resident, so it can draw the moment its
        // cells leave the ready set.
        let all = |_: IVec2| true;
        let covered_all = select_blocks(
            &world,
            center,
            &reach,
            0,
            &ready,
            &HashSet::new(),
            100,
            &all,
        );
        assert!(covered_all.shown.is_empty());
        assert!(covered_all.needed.contains(&(16, IVec2::ZERO)));
    }

    #[test]
    fn detail_that_was_drawing_is_not_swapped_for_a_coarser_block_while_a_neighbour_loads() {
        let world = grid(IVec2::ZERO);
        let all = all_blocks(&world);
        let center = IVec2::new(2, 2);
        let settled = select(&world, center, 0, &all);
        // One level-4 block drops out of the ready set (reloading), the rest keep drawing.
        let mut ready = all.clone();
        ready.remove(&(4, IVec2::new(4, 4)));
        let plan = select_blocks(
            &world,
            center,
            &reach,
            0,
            &ready,
            &settled.shown,
            0,
            &|_| false,
        );
        assert!(
            !plan.shown.contains(&(16, IVec2::ZERO)),
            "the coarse block replaced detail that was drawing"
        );
        assert!(plan.shown.contains(&(4, IVec2::ZERO)));
        // A first walk with nothing drawn yet still falls back to the coarse block, so a fresh
        // area shows something while its detail loads.
        let fresh = select(&world, center, 0, &ready);
        assert!(fresh.shown.contains(&(8, IVec2::ZERO)));
    }

    #[test]
    fn a_world_with_no_blocks_plans_nothing() {
        let world = ObjectLodWorld::for_test(IVec2::ZERO, &[4, 8, 16], &[]);
        let plan = select(&world, IVec2::ZERO, 0, &HashSet::new());
        assert_eq!(plan, ObjectPlan::default());
    }

    #[test]
    fn origins_that_are_not_zero_align_the_roots() {
        // Tamriel's origin is (-96, -96); a level-16 block sits at -96 + 16 k.
        let origin = IVec2::new(-23, -9);
        let world = grid(origin);
        let ready = all_blocks(&world);
        let plan = select(&world, origin + IVec2::new(1, 1), 0, &ready);
        assert!(plan.shown.contains(&(4, origin)));
        let coverage = cell_coverage(&plan.shown);
        assert!(coverage.values().all(|count| *count == 1));
    }

    #[test]
    fn a_block_replaced_by_its_children_is_a_refinement_not_a_step_back() {
        let children: HashSet<BlockKey> = [
            (8, IVec2::new(0, 0)),
            (8, IVec2::new(8, 0)),
            (8, IVec2::new(0, 8)),
            (8, IVec2::new(8, 8)),
        ]
        .into_iter()
        .collect();
        let drawn = |level: u8, shown: &HashSet<BlockKey>, max_level: u8| {
            block_cells(level, IVec2::ZERO).all(|cell| shown_covers(cell, max_level, shown))
        };
        assert!(drawn(16, &children, 16));
        // Three of four children: the fourth quarter shows nothing.
        let mut partial = children.clone();
        partial.remove(&(8, IVec2::new(8, 8)));
        assert!(!drawn(16, &partial, u8::MAX));
        // A coarser block over the same cells is a step back, not a refinement.
        let coarse: HashSet<BlockKey> = [(32, IVec2::ZERO)].into_iter().collect();
        assert!(!drawn(16, &coarse, 16));
        assert!(drawn(16, &coarse, u8::MAX));
        assert!(!drawn(16, &HashSet::new(), u8::MAX));
    }
}

#[cfg(test)]
mod retirement_regression {
    use super::*;
    #[test]
    fn inventory_opening_survives_every_counts_publication_until_the_index_arrives() {
        let mut state = ObjectLodStreaming {
            inventory_expected: true,
            ..default()
        };
        let mut metrics = StreamingMetrics::default();
        let mut profiler = ProfilingState::default();
        for _ in 0..3 {
            update_counts(&state, &mut metrics, &mut profiler);
            assert_eq!(metrics.pending_lod_object_blocks, 1);
        }
        state.index = Some(Arc::new(crate::world::object_lod::ObjectLodIndex::default()));
        update_counts(&state, &mut metrics, &mut profiler);
        assert_eq!(metrics.pending_lod_object_blocks, 0);
        state.index_failed = true;
        update_counts(&state, &mut metrics, &mut profiler);
        assert_eq!(metrics.unrecovered_lod_object_blocks, 1);
    }

    #[test]
    fn paced_despawn_wakes_same_pose_replanning() {
        let mut state = ObjectLodStreaming::default();
        let key = (4, IVec2::ZERO);
        state.blocks.insert(
            key,
            BlockStatus::Retiring {
                root: Entity::PLACEHOLDER,
                ready: true,
            },
        );
        state.plan_dirty = false;
        state.block_despawned(key);
        assert!(state.plan_dirty);
        assert!(!state.blocks.contains_key(&key));
    }
}
