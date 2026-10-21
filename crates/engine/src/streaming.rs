use crate::physics::{DebugTankard, PlayerBody, PlayerControlsSystems};
use crate::{
    config::EngineConfig,
    nif_depth::NifDepthMaterialSource,
    profiling::ProfilingState,
    render::{
        PLACED_OBJECT_RENDER_LAYERS, QUADRANT_WEIGHT_SAMPLES, TerrainExtension, TerrainMaterial,
        WaterExtension, WaterMaterial, WaterReflectionTexture,
    },
    swap_bridge::{SwapDrawn, SwapInCameraView, SwapStandIn, SwapSurface, SwapWaiting},
    terrain_merge::{
        CellLookup, TerrainMerge, classes::TerrainClasses, material::MergedTerrainMaterial,
    },
    world::{
        cache::{CellCache, TerrainLayerSnapshot, TerrainSnapshot},
        components::{
            CELL_SIZE, CellRef, DistantTerrainRoot, ExpectedModelBounds, ExteriorCellGrid, FormId,
            InstanceBounds, MeshHandle, StreamedCellKey, StreamedCellRoot, StreamingCamera,
            TerrainPatch, WaterSurface, WorldPosition, WorldTransform,
        },
        database::{
            AssetCatalog, CellDetail, CellKey, CellPayload, DatabaseRequest, DatabaseResponse,
            ReferenceRow, WorldDatabase,
        },
        is_safe_relative_asset_path,
    },
};
use bevy::{
    app::{Propagate, SceneSpawnerSystems},
    asset::{LoadState, RecursiveDependencyLoadState, RenderAssetUsages},
    camera::{primitives::MeshAabb, visibility::RenderLayers},
    ecs::system::SystemParam,
    gltf::{GltfExtras, GltfMaterialName, GltfSceneExtras},
    image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler},
    math::Affine3A,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
    world_serialization::{WorldInstance, WorldInstanceReady},
};
use bevy_rapier3d::prelude::{
    Collider, ColliderDisabled, MassProperties, RigidBody, WriteRapierContext,
};
use serde::{Deserialize, Serialize};
use shared::collision::{
    BodyKind, COLLISION_ASSET_VERSION, CollisionAsset, CollisionBody, CollisionShape,
};
use shared::lod::LodTier;
use std::collections::{HashMap, HashSet, VecDeque};
use std::error::Error as StdError;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub(crate) mod far_stock;
mod reference_spawn;
pub(crate) use reference_spawn::{ReferenceSpawnContext, reference_placement, spawn_reference};
pub(crate) mod identity_changes;
pub(crate) mod lod;
mod lod_objects;
mod lod_trees;
pub(crate) mod native_lod;
pub(crate) mod terrain_audit;
mod terrain_identity;
use shared::terrain_mesh::TerrainCellMesh;
use std::sync::Arc;
pub(crate) use terrain_identity::{TerrainIdentityAccess, TerrainPrimitiveIdentity};

pub(crate) mod admission;
pub(crate) mod control;
#[cfg(test)]
mod merged_tests;
mod priority;
pub(crate) mod requests;
mod reservations;
pub(crate) mod runtime;
mod surface_costs;

// Wall-clock spans can include a short OS scheduler preemption. Keep the raw maximum in metrics,
// but require a material overrun before classifying the frame as a commit-budget violation.
const COMMIT_BUDGET_SCHEDULER_TOLERANCE_MICROS: u64 = 1_000;

/// How many terrain-only cells one frame may commit, on top of the full cells
/// [`EngineConfig::max_cell_commits_per_frame`] allows.
///
/// A terrain-only commit builds four small landscape meshes and their materials and spawns no
/// references, so it is a fraction of a full commit's cost. The default ring is 264 such cells: at
/// one commit per frame they would take 264 frames to appear, four seconds at sixty frames a
/// second, and a `--shots` pose that waits for streaming to settle would wait that out for every
/// pose - measured at 1.9 s for the ring to settle with this budget against 1.3 s without a ring
/// at all (a `--shots` run of the Pale view, release build). The frame's own
/// [`EngineConfig::max_commit_micros_per_frame`] budget still applies, and a response that does
/// not fit is held for the next frame rather than dropped.
const MAX_TERRAIN_CELL_COMMITS_PER_FRAME: usize = 16;

/// Total load attempts a failing cell gets before the streamer gives up on it for this visit of
/// the camera: the first try plus two retries. The failures that retry at all are transient (a
/// database or IO error; a load-order race can heal), and two retries spread over a 1 s then 2 s
/// backoff outlast any such race, while a cell that fails for a real reason is not re-queried -
/// or re-logged - forever. A cell that leaves the unload radius is forgotten, so its next visit
/// starts this budget fresh.
///
/// The same budget covers every cell, exteriors and interiors alike: the interior the camera
/// stands in is retried ([`retry_failed_interior`]) until it has spent these attempts, and
/// [`record_failed_cell`] gives up on any cell at its last one.
const MAX_CELL_LOAD_ATTEMPTS: u32 = 3;

/// Retries a failing cell gets after its first load: the rest of [`MAX_CELL_LOAD_ATTEMPTS`].
const MAX_CELL_LOAD_RETRIES: u32 = MAX_CELL_LOAD_ATTEMPTS - 1;

/// How long the streamer waits before the first retry of a failed cell. Every further failure
/// doubles the wait, so the budget of [`MAX_CELL_LOAD_ATTEMPTS`] waits 1 s and then 2 s. The wait
/// is wall-clock time (the clock the request queue and commit budget already measure with), never
/// a frame count: a retry every frame would re-query a busy database once per frame.
const CELL_RETRY_BACKOFF_BASE: Duration = Duration::from_secs(1);

fn commit_budget_exceeded(elapsed_micros: u64, budget_micros: u64) -> bool {
    elapsed_micros > budget_micros.saturating_add(COMMIT_BUDGET_SCHEDULER_TOLERANCE_MICROS)
}

/// What [`collect_cells`] has committed so far in the frame, and how long the frame's commit work
/// has taken: what [`commit_waits_for_next_frame`] decides the next commit on.
#[derive(Debug, Clone, Copy)]
struct CommitsThisFrame {
    /// Answers whose commit started this frame, whatever came of them: a cell that failed its
    /// validation, or a ring grid with nothing in it, took its share of the frame too.
    started: usize,
    /// Full cells committed this frame.
    full: usize,
    /// Terrain-only cells committed this frame.
    terrain: usize,
    /// Microseconds since the frame's commit work started, answers taken from the worker included.
    elapsed_micros: u64,
}

/// Whether the next answer, a cell at `detail`, waits for the next frame instead of being
/// committed in this one.
///
/// Two limits apply, and a commit that would break either waits:
///
/// - **A count per tier.** A full cell commits under [`EngineConfig::max_cell_commits_per_frame`]
///   (one by default): its references, their assets and their lights are what that cap protects.
///   A terrain-only cell is a fraction of that work, so it commits under its own, larger count,
///   [`MAX_TERRAIN_CELL_COMMITS_PER_FRAME`].
/// - **The frame's time budget,** [`EngineConfig::max_commit_micros_per_frame`], for both tiers:
///   no commit *starts* once the frame's commit work has used the budget up. A full commit that
///   finds the budget spent by the terrain-only commits before it waits a frame like they would.
///
/// The budget is checked before a commit, not during one, so it bounds when commits start rather
/// than when they finish: the last commit that starts under budget can still take the frame past
/// it, and [`collect_cells`] counts that frame as a budget violation afterwards
/// ([`commit_budget_exceeded`]). The first commit of a frame is never held by the time budget - only
/// by a count of zero - so however small the budget, or however long taking the answers from the
/// worker took, every frame commits at least one cell and a held answer, which is the first in
/// line the next frame, cannot wait forever.
fn commit_waits_for_next_frame(
    detail: CellDetail,
    done: CommitsThisFrame,
    config: &EngineConfig,
) -> bool {
    let over_count = match detail {
        CellDetail::Full => done.full >= config.max_cell_commits_per_frame,
        CellDetail::Terrain => done.terrain >= MAX_TERRAIN_CELL_COMMITS_PER_FRAME,
    };
    over_count || (done.started > 0 && done.elapsed_micros >= config.max_commit_micros_per_frame)
}

/// Grid cell and quadrant covered by a terrain surface at one LOD tier.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerrainCoverage {
    grid: IVec2,
    quadrant: u8,
    tier: Option<LodTier>,
}

#[derive(Component)]
pub(crate) struct TerrainWorldspace(u32);

#[derive(Component)]
struct TerrainSurfaceReady;

/// The time since `started` in whole microseconds, saturating at `u64::MAX`.
fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

/// The full-detail terrain of one exterior cell drawn as a single merged mesh (#215). It covers
/// all four quadrants of `grid`, so the LOD hand-off treats each of them as ready when this
/// entity is.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MergedTerrainCoverage {
    pub(crate) grid: IVec2,
}

/// A merged terrain mesh that has been spawned and waits for the readiness pass.
#[derive(Component)]
struct PendingMergedTerrain;

/// Marks the merged terrain mesh entity of a cell, for the scene gauges.
#[derive(Component)]
pub(crate) struct MergedTerrainCell;

/// The resources `collect_cells` fills a cell's entities from. Grouped because the system would
/// otherwise pass Bevy's limit of sixteen parameters.
#[derive(SystemParam)]
struct CellAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    terrain_materials: ResMut<'w, Assets<TerrainMaterial>>,
    water_materials: ResMut<'w, Assets<WaterMaterial>>,
    /// Present when the run installed the merged terrain plugin.
    merge: Option<ResMut<'w, TerrainMerge>>,
}

/// The merged data a cell is drawn from, once it has passed every check and been welded.
struct MergedSpawn {
    cell: Box<TerrainCellMesh>,
    classes: Arc<TerrainClasses>,
    material: Handle<MergedTerrainMaterial>,
    /// Every image `material` binds ([`TerrainMerge::material_images`]): the cell's declaration to
    /// the swap bridge.
    images: Arc<[AssetId<Image>]>,
}

/// A fixed placement's collision provenance.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticColliderSource {
    NifAuthored,
    RenderProxy,
}

pub struct StreamingPlugin;

impl Plugin for StreamingPlugin {
    fn build(&self, app: &mut App) {
        identity_changes::install(app.world_mut());
        app.add_systems(Last, terrain_identity::validate_relationships);
        if app
            .world()
            .get_resource::<EngineConfig>()
            .is_some_and(|config| config.cluster_own_statics)
        {
            crate::cluster::ownership::install(app);
        }
        app.init_resource::<StreamingWorld>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<lod::LodStreaming>()
            .init_resource::<DiagnosticFallbackAssets>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<SceneSpawnBatch>()
            .init_resource::<ActiveSpace>()
            .init_resource::<LodHorizon>()
            .init_resource::<StaticCollisionCache>()
            .init_resource::<admission::SceneAdmission>()
            .init_resource::<requests::SceneSchedulingState>()
            .init_resource::<runtime::StreamingRuntime>()
            .add_plugins(crate::streaming_preparation::StreamingPreparationPlugin)
            .init_resource::<crate::pacing::PacingTracker>()
            .add_observer(mark_world_instance_ready)
            .add_observer(lod::mark_lod_world_instance_ready)
            .add_systems(
                Update,
                (
                    (
                        // First, so the LOD planner two systems down reads this frame's space.
                        runtime::update_streaming_control,
                        plan_cells,
                        despawn_cells,
                        lod::plan_lod_chunks,
                        collect_cells,
                        lod::collect_lod_chunks,
                        // Include this frame's deferred LOD roots before the next
                        // player update can encounter their visible mesh geometry.
                        requests::dispatch_scene_requests,
                        arm_pending_models,
                        track_asset_readiness,
                    )
                        .chain(),
                    (
                        track_surface_readiness,
                        track_merged_surface_readiness,
                        lod::track_lod_readiness,
                        lod::batch_ready_lod_chunks,
                        lod::update_terrain_lod_visibility,
                        finish_streaming_commit_budget,
                        update_render_origin,
                        validate_streaming_lifecycle,
                        runtime::observe_streaming_ownership,
                    )
                        .chain(),
                )
                    .chain()
                    .after(PlayerControlsSystems),
            )
            // Bevy instantiates every ready converted model in one unbudgeted pass inside
            // `SceneSpawnerSystems::WorldInstanceSpawn`; these two systems bracket that pass so the
            // profile can tell a frame that spawned a batch from one that spawned nothing.
            .add_systems(
                SpawnScene,
                (
                    begin_scene_spawn_batch.before(SceneSpawnerSystems::WorldInstanceSpawn),
                    end_scene_spawn_batch.after(SceneSpawnerSystems::WorldInstanceSpawn),
                ),
            );
        // Distant object LOD and tree billboards from the package's Skyrim LOD inventory. Inert
        // for a package without it, and for a run that turns both off.
        app.add_plugins((
            lod_objects::ObjectLodPlugin,
            lod_trees::TreeLodPlugin,
            native_lod::NativeLodPlugin,
        ));
        // On a profiled run, the despawns the immediate unload queues through `Commands` are
        // applied at the schedule's sync point right after `plan_cells` - exactly where Bevy
        // would insert one on its own (the exclusive `despawn_cells` forces it). These systems
        // span that apply as `streaming/space_change_unload_apply`: the sync point applies every
        // command buffer pending at that distance, which on a crossing frame carries the
        // space-change despawns. A run that is not profiling installs none of this, and the
        // explicit `ApplyDeferred` is then the same sync point Bevy's own pass inserts.
        //
        // The second bracket spans the frame's **cell commit** apply, the sync point on the
        // `collect_cells` -> `lod::collect_lod_chunks` edge. A commit queues a cell's root, its
        // references and their scene spawns as deferred commands, so the frame's spawning cost is
        // paid at that sync point rather than inside the system - the same shape as the unload
        // apply, and the other deferred half of a crossing frame's `Update` (analysis-1103 Â§4
        // leaves the 140 ms `Update` unattributed; analysis-1074 Â§2's open item is which part of
        // it is the apply). Both brackets sit exactly where Bevy's own pass inserts its sync
        // point, so a profiled run's schedule is the unmeasured one's; only the measurement is
        // added.
        if app
            .world()
            .get_resource::<EngineConfig>()
            .is_some_and(|config| config.profile_output_dir.is_some())
        {
            app.init_resource::<SpaceUnloadFlushClock>()
                .init_resource::<CellCommitFlushClock>()
                .add_systems(
                    Update,
                    (
                        stamp_space_unload_flush.after_ignore_deferred(plan_cells),
                        bevy::ecs::schedule::ApplyDeferred,
                        record_space_unload_flush.before(despawn_cells),
                    )
                        .chain(),
                )
                .add_systems(
                    Update,
                    (
                        stamp_cell_commit_flush.after_ignore_deferred(collect_cells),
                        bevy::ecs::schedule::ApplyDeferred,
                        record_cell_commit_flush.before(lod::collect_lod_chunks),
                    )
                        .chain(),
                );
        }
        // The world-ready scan walks the whole stream window every frame and only feeds the pacing
        // report, so an ordinary play session installs none of it.
        if app
            .world()
            .get_resource::<crate::config::EngineConfig>()
            .is_some_and(crate::config::EngineConfig::measures_pacing)
        {
            app.add_systems(
                Update,
                crate::pacing::track_world_ready.after(validate_streaming_lifecycle),
            );
        }
        // The hole-risk gauge walks every terrain patch, so it exists only for a run that profiles:
        // it is measurement data for the bundle, like the frame series.
        if app
            .world()
            .get_resource::<crate::config::EngineConfig>()
            .is_some_and(|config| config.profile_output_dir.is_some())
        {
            app.add_systems(
                Update,
                record_terrain_swap_gauge.after(track_surface_readiness),
            );
        }
        // The retire-visibility probe (`--probe-retire-visibility`): a measurement-only run that
        // checks a draining retiring cell never shows in a view. It installs neither the resource
        // nor the system unless the flag names a directory.
        install_retire_visibility_probe(
            app,
            app.world()
                .get_resource::<EngineConfig>()
                .and_then(|config| config.probe_retire_visibility.clone()),
        );
    }
}

/// One exterior cell's failed full request, kept for as long as the cell stays planned
/// ([`StreamingWorld::full_failed`]).
///
/// `attempts` is how many full loads of the cell have failed since it was last wanted from
/// nothing, `reason` is why the last one failed, and `retry_at` is the earliest time the plan may
/// ask for the cell in full again. `retry_at` is `None` once the cell is given up on - its data is
/// invalid, or the [`MAX_CELL_LOAD_ATTEMPTS`] budget is spent - and a given-up cell is downgraded
/// to terrain-only until it leaves the plan.
#[derive(Debug, Clone)]
struct FullFailure {
    attempts: u32,
    reason: String,
    retry_at: Option<Instant>,
}

impl FullFailure {
    /// Whether the plan may ask for the cell in full again now: its backoff has passed and its
    /// attempt budget is not spent. A `retry_at` in the future is the wait between two attempts;
    /// `None` is a cell given up on, which is never asked again while the entry lives.
    fn retryable(&self, now: Instant) -> bool {
        matches!(self.retry_at, Some(retry_at) if now >= retry_at)
            && self.attempts < MAX_CELL_LOAD_ATTEMPTS
    }
}

#[derive(Resource, Default)]
pub struct StreamingWorld {
    generation: u64,
    /// Spawn order for the models [`arm_pending_models`] has yet to arm, so a backlog drains
    /// oldest first whatever order the queries iterate in.
    next_model_sequence: u64,
    cells: HashMap<CellKey, CellStatus>,
    /// Exterior cells whose full request failed, while they stay planned, with the failure record
    /// the retry decision reads. The plan asks for these cells as terrain-only instead, wherever
    /// they are: a cell the camera stands in whose references failed still has a landscape to
    /// draw, and asking for it in full again would fail again (the data does not change) and drop
    /// that landscape every time. An entry whose backoff has passed and whose attempt budget
    /// ([`MAX_CELL_LOAD_ATTEMPTS`]) is not spent is the one exception: the plan asks for the cell
    /// in full again ([`plan_cells`]), and the terrain root it holds keeps drawing it meanwhile.
    /// Forgotten when the cell leaves the plan, so a cell the camera comes back to is asked for in
    /// full once more, with a fresh record.
    full_failed: HashMap<CellKey, FullFailure>,
    /// How many of its [`MAX_CELL_LOAD_RETRIES`] each failed interior has spent. A budget belongs to one
    /// stay in the plan: it is dropped with the cell, so a cell the camera comes back to is asked
    /// for in full again with its budget whole.
    failed_retries: HashMap<CellKey, u32>,
    /// The interior the camera stood in when [`plan_cells`] last ran: [`ActiveCell::interior`] as
    /// the plan saw it. It is what lets [`StreamingWorld::has_failed`] tell a failure the plan is
    /// about to retry ([`retry_failed_interior`]) from one it has given up on, without every
    /// caller having to pass the active cell in. `None` while the camera is outdoors, and before
    /// the plan has run at all.
    active_interior: Option<u32>,
    /// Cells loaded by id through [`StreamingWorld::request_cell`] rather than by the plan. The
    /// plan keeps them whatever the camera does, the way main's streamer keeps an interior until
    /// something unloads it. A pin lasts the run: the streaming fixture, the only caller, loads its
    /// interior this way and checks it stays. A caller that needs to let a cell go again adds an
    /// unpin here rather than leaving the cell pinned.
    pinned: HashSet<CellKey>,
    /// The retire backlog [`despawn_cells`] saw on its last run: the backlog at the start of that
    /// frame's drain, cleared when the drain emptied it. The next frame compares its own backlog
    /// against this to tell sustained growth (flight retiring cells faster than the budget drains
    /// them) from a one-frame burst, which a fast flight or a teleport retires all at once.
    last_retire_backlog: usize,
    outstanding_cells: HashMap<CellKey, u64>,
    held_responses: VecDeque<DatabaseResponse>,
}

impl StreamingWorld {
    /// Whether `key` is streamed in as a whole cell and its root entity spawned.
    ///
    /// A terrain-only cell is not resident by this test: it holds no references, so it cannot be
    /// crossed into (a load door's destination), looked through (the portal's) or shot (a `--shots`
    /// pose). Every caller wants a cell it can stand in, and a destination a door pre-streamed is
    /// always full, so a terrain-only cell is never the answer they are waiting for.
    /// A cell waiting for its turn in the unload budget is still resident by this test while it
    /// holds its full root: it is still drawn, and the plan revives it the same frame if it comes
    /// back. A door gate waiting on its destination ([`crate::transition`]) would otherwise see a
    /// destination the pacer has just retired as gone and fall back to holding the camera, for a
    /// cell that is still there and is about to be revived by the plan's next pass.
    pub fn is_resident(&self, key: &CellKey) -> bool {
        matches!(
            self.cells.get(key),
            Some(
                CellStatus::Resident {
                    detail: CellDetail::Full,
                    ..
                } | CellStatus::Retiring {
                    detail: CellDetail::Full,
                    ..
                }
            )
        )
    }

    /// Whether the request for `key` in full failed and will not be repeated for now: the cell is
    /// never going to become resident while this answers `true`, so a caller waiting for it (a
    /// `--shots` pose) stops waiting.
    ///
    /// An exterior cell whose full request failed is drawn as terrain-only afterwards, so it is
    /// failed by this test whatever its terrain-only status is. The plan may still ask for the
    /// cell in full again once its backoff passes ([`plan_cells`]), but a caller that settled on
    /// the failure has already stopped waiting, and the terrain the cell is drawn by does not
    /// depend on the answer.
    ///
    /// The interior the camera stands in is the one cell whose failed full request *is* repeated:
    /// the plan asks for it again up to [`MAX_CELL_LOAD_RETRIES`] times, the shared attempt budget
    /// ([`retry_failed_interior`]). Between a failed answer and the retry that follows it the cell
    /// reads as failed in the cell table, but a retry is still owed, so it is not failed by this
    /// test until the last of its retries has failed too. A caller that settled on the first
    /// failure - a `--shots` pose, a door's destination - would otherwise stop waiting for a cell
    /// the plan is about to ask for again. An interior that is not the one the camera stands in is
    /// never retried, so its first failure is final, exactly as an exterior's is.
    pub fn has_failed(&self, key: &CellKey) -> bool {
        self.full_failed.contains_key(key)
            || (matches!(
                self.cells.get(key),
                Some(CellStatus::Failed {
                    detail: CellDetail::Full,
                    ..
                })
            ) && !retry_failed_interior(key, self.active_interior, self))
    }

    /// The number of cells the planner wants for `space` (the stream window of `radius` around
    /// `center` outside, the one active cell inside an interior), how many of them are resident (at
    /// full detail: a retiring cell inside the window is revived by the planner before this is
    /// read, so it counts as resident), and how many are failed or given up on. A failed cell (map
    /// edge, missing data) never becomes resident - a transient failure is retried on the plan's
    /// own clock, with its terrain rescue drawn meanwhile - so the world-ready predicate counts it
    /// as settled rather than waiting for it forever.
    pub(crate) fn window_residency(
        &self,
        space: &ActiveSpace,
        configured_worldspace: u32,
        center: IVec2,
        radius: i32,
    ) -> (usize, usize, usize) {
        let wanted = wanted_in_space(space, configured_worldspace, center, radius);
        let mut resident = 0;
        let mut failed = 0;
        for key in &wanted {
            if self.is_resident(key) {
                resident += 1;
            } else if self.has_failed(key) {
                failed += 1;
            }
        }
        (wanted.len(), resident, failed)
    }
    /// Residency of the current interior or the active exterior worldspace window.
    pub(crate) fn active_residency(
        &self,
        space: &ActiveSpace,
        configured_worldspace: u32,
        center: IVec2,
        radius: i32,
    ) -> (usize, usize, usize) {
        self.window_residency(space, configured_worldspace, center, radius)
    }
}

fn wanted_in_space(
    space: &ActiveSpace,
    configured_worldspace: u32,
    center: IVec2,
    radius: i32,
) -> HashSet<CellKey> {
    if let Some(cell_id) = space.interior {
        return HashSet::from([CellKey::Interior(cell_id)]);
    }
    let worldspace_id = space.exterior_worldspace(configured_worldspace);
    let mut wanted = HashSet::new();
    for y in -radius..=radius {
        for x in -radius..=radius {
            wanted.insert(CellKey::Exterior {
                worldspace_id,
                grid_x: center.x + x,
                grid_y: center.y + y,
            });
        }
    }
    wanted
}

/// Whether this run builds world colliders (main's `interactive_world_physics`, narrowed to a run
/// that walks).
///
/// Colliders exist for the player's ray probe to hit ([`crate::player`]), so a run that never walks
/// (a `--shots` pose, a benchmark, an auto-flight tour) builds none and is the run it was before:
/// no collider work in `spawn_cell`, no static collision extraction in [`track_asset_readiness`],
/// nothing for the rebase to shift. The one gate, so the streaming side and the player side cannot
/// disagree about whether the world collides.
pub(crate) fn builds_world_colliders(config: &EngineConfig) -> bool {
    config.interactive_world_physics()
}

/// Marks a terrain quadrant that carries a rapier collider. Built only for a full-detail cell of a
/// run that walks ([`builds_world_colliders`]); a ring cell ([`CellDetail::Terrain`]) is landscape
/// seen from a distance and never walked on, so it gets none.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainCollider;

impl StreamingWorld {
    /// Submits one full-detail load for `key` unless it is already loading, resident or failed,
    /// and records the request on the streaming metrics. The streaming fixture loads its interior
    /// through this, by id, because nothing in the planner switches the active space on its own.
    ///
    /// A cell the unload pacer has retired but not yet despawned is *revived*: pinning it takes it
    /// out of the retire backlog with the root it kept, so a cell pinned between [`plan_cells`] and
    /// [`despawn_cells`] is never despawned and requested again.
    pub(crate) fn request_cell(
        &mut self,
        database: &WorldDatabase,
        key: CellKey,
        metrics: &mut StreamingMetrics,
        profiler: &mut ProfilingState,
    ) -> bool {
        if let Some(status) = self.cells.get_mut(&key) {
            if let CellStatus::Retiring { root, detail } = *status {
                *status = CellStatus::Resident { root, detail };
                metrics.revived_cells = metrics.revived_cells.saturating_add(1);
                profiler.increment("streaming/revived_cells", 1);
                profiler.event(format!("{key:?}"), "revived_by_pin", None);
            }
            self.pinned.insert(key);
            return false;
        }
        if self.outstanding_cells.contains_key(&key) {
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        if database
            .try_request(DatabaseRequest::Load {
                generation,
                key,
                detail: CellDetail::Full,
                queued_at: Instant::now(),
            })
            .is_ok()
        {
            metrics.requests_submitted += 1;
            profiler.increment("streaming/requests", 1);
            profiler.event(format!("{key:?}"), "requested", None);
            self.cells.insert(
                key,
                CellStatus::Loading {
                    generation,
                    detail: CellDetail::Full,
                    replaced: None,
                },
            );
            // Pinned only once the queue took the request: a refused one leaves nothing behind,
            // and the next call asks again.
            self.pinned.insert(key);
            self.outstanding_cells.insert(key, generation);
            return true;
        }
        false
    }

    fn consume_cell_response(&mut self, key: CellKey, generation: u64, detail: CellDetail) {
        // A mismatched detail cannot consume the outstanding slot of the current attempt.
        if matches!(self.cells.get(&key),
            Some(CellStatus::Loading { generation: current, detail: wanted, .. })
            if *current == generation && *wanted != detail)
        {
            return;
        }
        if self.outstanding_cells.get(&key) == Some(&generation) {
            self.outstanding_cells.remove(&key);
        }
    }

    /// Whether a response matches the generation and detail still wanted. A cell that left
    /// the active space or unload radius while its response was in flight is not.
    fn awaits(&self, key: CellKey, generation: u64, detail: CellDetail) -> bool {
        matches!(
            self.cells.get(&key),
            Some(CellStatus::Loading { generation: current, detail: wanted, .. })
                if *current == generation && *wanted == detail
        )
    }
}

#[derive(Resource)]
struct StreamingCommitBudget {
    frame_started: Instant,
    /// Full-cell commits left this frame (far stock commits from these too).
    remaining: usize,
    /// Distant LOD requests left this frame ([`EngineConfig::max_lod_requests_per_frame`]).
    lod_remaining: usize,
    commits: usize,
    lod_priority: bool,
    reserved_for_lod: bool,
    prefer_deferred_response: bool,
}

impl Default for StreamingCommitBudget {
    fn default() -> Self {
        Self {
            frame_started: Instant::now(),
            remaining: 0,
            lod_remaining: usize::MAX,
            commits: 0,
            lod_priority: false,
            reserved_for_lod: false,
            prefer_deferred_response: true,
        }
    }
}

impl StreamingCommitBudget {
    fn remaining_for_cells(&self) -> usize {
        self.remaining
            .saturating_sub(usize::from(self.reserved_for_lod))
    }

    fn reserve_for_lod(&mut self, waiting: bool, frame_limit: usize) {
        self.reserved_for_lod = waiting && (self.lod_priority || frame_limit > 1);
    }

    /// Whether the frame can start another distant LOD request: under the LOD count and, once
    /// anything has committed this frame, under the frame's time budget. Like the cells' rule,
    /// the first commit of a frame is never held by time, so the queue always moves.
    fn lod_slot_open(&self, max_commit_micros: u64) -> bool {
        self.lod_remaining > 0
            && !(self.commits > 0
                && max_commit_micros > 0
                && self.frame_started.elapsed().as_micros() >= u128::from(max_commit_micros))
    }

    /// Counts one distant LOD request started this frame.
    fn spend_lod_slot(&mut self) {
        self.lod_remaining = self.lod_remaining.saturating_sub(1);
        self.commits = self.commits.saturating_add(1);
    }

    fn elapsed_limit_reached(&self, runtime: Option<&runtime::StreamingRuntime>) -> bool {
        self.commits != 0
            && runtime
                .and_then(|runtime| runtime.decision)
                .is_some_and(|decision| {
                    self.frame_started.elapsed().as_micros()
                        >= u128::from(decision.budgets.max_commit_micros)
                })
    }
}

#[derive(Resource, Reflect, Debug, Clone, Default, Serialize)]
#[reflect(Resource)]
pub struct StreamingMetrics {
    pub requests_submitted: u64,
    pub responses_received: u64,
    pub stale_responses: u64,
    pub failed_cells: u64,
    /// Failed cells the planner has asked for again once their backoff passed, so a retried cell
    /// that fails again is visible as one failure and one retry rather than as a new cell.
    pub failed_cell_retries: u64,
    /// Cells given up on - invalid data, or a spent [`MAX_CELL_LOAD_ATTEMPTS`] budget - which will
    /// not be requested again while they stay inside the unload radius.
    pub failed_cells_given_up: u64,
    pub unloaded_cells: u64,
    pub resident_cells: usize,
    pub loading_cells: usize,
    pub peak_resident_cells: usize,
    pub peak_loading_cells: usize,
    /// Resident cells that hold their landscape only, the distant ring ([`CellDetail::Terrain`]).
    pub terrain_cells: usize,
    pub peak_terrain_cells: usize,
    /// Cells reloaded at the other detail as the camera moved: a terrain-only cell that entered the
    /// full-detail grid, and a full cell that left it for the ring.
    pub cells_upgraded: u64,
    pub cells_downgraded: u64,
    /// Detail changes whose stand-in outlived
    /// [`MAX_SWAP_HOLD_FRAMES`](crate::swap_bridge::MAX_SWAP_HOLD_FRAMES): the root the cell was
    /// drawn by was despawned anyway, so what it covered is unguarded from that frame on. A failure
    /// count, not a wait - every one of them is warned about as well.
    pub swap_hold_timeouts: u64,
    /// Ring grids that held nothing: the ones a worldspace does not reach, and cells whose
    /// landscape the cell cache does not carry. Deliberately not part of [`Self::failed_cells`]:
    /// see the commit in [`collect_cells`](fn@collect_cells).
    pub ring_cells_empty: u64,
    /// Full-detail exterior cells whose answer carried no terrain at all: committed as resident,
    /// with no landscape quadrants and no water, and marked [`TerrainLess`]. The ring's twin is
    /// [`Self::ring_cells_empty`]; unlike a ring cell this one is held, so it is counted on its own
    /// and is not a streaming invariant failure ([`validate_streaming_lifecycle`]). Whether such a
    /// cell should draw nothing or fail for a retry is a data decision this counter feeds.
    pub full_cells_without_terrain: u64,
    pub total_query_micros: u64,
    pub max_query_micros: u64,
    pub max_commit_micros: u64,
    pub total_frame_commit_micros: u64,
    pub max_frame_commit_micros: u64,
    pub commit_frames: u64,
    pub commit_budget_micros: u64,
    pub commit_budget_violations: u64,
    pub total_queue_wait_micros: u64,
    pub max_queue_wait_micros: u64,
    pub total_request_micros: u64,
    pub max_request_micros: u64,
    pub total_rows_loaded: u64,
    pub assets_ready: u64,
    pub asset_load_failures: u64,
    pub max_asset_ready_micros: u64,
    pub pending_asset_instances: usize,
    pub pending_surface_instances: usize,
    pub meshes_validated: u64,
    pub materials_validated: u64,
    pub images_validated: u64,
    pub material_validation_failures: u64,
    pub diagnostic_fallbacks: u64,
    pub canonical_fixture_validated: bool,
    pub terrain_patches_validated: u64,
    /// Exterior cells drawn as one merged mesh with the shared terrain material (#215).
    pub terrain_cells_merged: u64,
    /// Terrain cells drawn by the four-quadrant path: every cell when the merged path is absent
    /// or refused, and the cells the merged data could not serve otherwise.
    pub terrain_cells_fallback: u64,
    /// The subset of [`Self::terrain_cells_fallback`] where merged data was expected: the chunk
    /// file or the cell's entry is missing, or the entry failed validation.
    pub terrain_cells_fallback_expected_merged: u64,
    /// The subset of those whose merged data exists but failed validation, which is the one worth
    /// a look.
    pub terrain_cells_merged_invalid: u64,
    /// Where the merged terrain path stands: absent, awaiting device, loading arrays, ready or
    /// refused with its reason.
    pub terrain_merge_state: String,
    pub terrain_seams_validated: u64,
    /// Edge points of an arriving cell whose height had to move onto a resident neighbour's by more
    /// than the edge tolerance, so the two meshes meet exactly instead of the cell being rejected
    /// (see [`validate_and_register_terrain_edges`]).
    pub terrain_seam_points_welded: u64,
    /// Shared edges that differ past [`MAX_WELDABLE_EDGE_DELTA`] and are drawn as authored: a
    /// city's sculpted landscape meeting an unsculpted copy of the land around it, which Skyrim
    /// itself never stitches.
    pub terrain_edges_left_as_authored: u64,
    pub terrain_validation_failures: u64,
    /// Fixed placements represented by NIF-authored Havok shapes.
    pub static_colliders_authored: u64,
    /// Authored colliders that also contain shape families this extractor could not decode.
    pub static_colliders_partial: u64,
    /// Fixed placements represented by a legacy render-triangle proxy.
    pub static_colliders_proxy: u64,
    /// Fixed-placement collider entities resident at the last lifecycle check.
    pub resident_static_colliders: usize,
    /// NIF models that explicitly have no physical collision.
    pub static_colliders_authored_absent: u64,
    /// Eligible fixed placements for which no supported collision could be made.
    pub static_colliders_skipped: u64,
    /// Placed clutter references simulated as dynamic Rapier bodies from authored physics data.
    pub dynamic_clutter_bodies: u64,
    /// Dynamic clutter bodies whose non-convex shapes were replaced by one convex hull.
    pub dynamic_clutter_convexified: u64,
    /// Dynamic clutter placements that could not be built (no hull, invalid shape); the reference
    /// keeps no body and never falls back to a fixed collider.
    pub dynamic_clutter_skipped: u64,
    /// Dynamic clutter placements refused because [`crate::physics::MAX_LIVE_DYNAMIC_CLUTTER`]
    /// bodies were already live; they keep no collider, like movable types before this slice.
    pub dynamic_clutter_overflow: u64,
    /// Dynamic bodies beyond a model's first, which are not simulated (one body per reference).
    pub dynamic_clutter_extra_bodies_ignored: u64,
    /// Dynamic clutter bodies whose authored mass or inertia tensor was unusable, so collider
    /// density supplied the mass instead.
    pub dynamic_clutter_mass_fallbacks: u64,
    /// Dynamic clutter bodies clamped to their authored maximum speed, summed over frames (a body
    /// clamped on five frames counts five).
    pub dynamic_clutter_clamped: u64,
    pub water_surfaces_validated: u64,
    pub water_validation_failures: u64,
    pub terrain_water_fixture_validated: bool,
    pub transform_instances_validated: u64,
    pub transform_nodes_validated: u64,
    pub bounds_validated: u64,
    /// References whose converted model is an empty glTF scene: an editor marker the converter
    /// dropped every shape of, so the model holds no render primitive and there is nothing to
    /// place, draw or bound. Counted here rather than in
    /// [`Self::transform_bounds_validation_failures`], which is a hard gate and must count only
    /// real conversion defects.
    pub empty_model_references: u64,
    pub transform_bounds_validation_failures: u64,
    pub transform_bounds_fixture_validated: bool,
    pub active_requests: usize,
    pub peak_active_requests: usize,
    /// Distant object LOD blocks (`.bto`) the plan has asked for since the run began.
    pub lod_object_blocks_requested: u64,
    /// Object blocks that passed their hash and bounds checks.
    pub lod_object_blocks_ready: u64,
    /// Object blocks that failed to load or validate, retries counted again.
    pub lod_object_blocks_failed: u64,
    /// Object blocks resident and ready.
    pub resident_lod_object_blocks: usize,
    /// Object blocks asked for and not ready yet, queued or loading.
    pub pending_lod_object_blocks: usize,
    pub pending_native_lod_draws: usize,
    pub visible_native_lod_draws: usize,
    pub native_lod_drawn_twice: usize,
    pub native_lod_drawn_by_nothing: usize,
    /// Object blocks hidden and waiting for the unload pacer.
    pub retiring_lod_object_blocks: usize,
    /// Object blocks that failed for good and are not retried.
    pub unrecovered_lod_object_blocks: usize,
    /// Object blocks the last plan draws.
    pub lod_object_blocks_visible: usize,
    /// Per-cell segments the ready blocks carry, and how many the ready cells hide now.
    pub lod_object_segments: u64,
    pub lod_object_segments_hidden: usize,
    /// Object primitives the converter marked as non-rendering, kept hidden.
    pub lod_object_excluded_primitives: u64,
    /// Index rows left out because they failed validation (`world::object_lod`).
    pub lod_object_rows_refused: u64,
    /// Object blocks the unload pacer has despawned.
    pub unloaded_lod_object_blocks: u64,
    /// Distant tree billboard instance blocks: reads asked for, merged and spawned, failed.
    pub lod_tree_blocks_requested: u64,
    pub lod_tree_blocks_ready: u64,
    pub lod_tree_blocks_failed: u64,
    /// Reads whose block was no longer wanted when the answer came.
    pub lod_tree_stale_responses: u64,
    /// Tree instances merged into drawn meshes since the run began.
    pub lod_tree_instances: u64,
    pub resident_lod_tree_blocks: usize,
    pub pending_lod_tree_blocks: usize,
    pub unrecovered_lod_tree_blocks: usize,
    /// Tree cells drawing now (cells whose full-detail model is not ready yet).
    pub lod_tree_cells_visible: usize,
    pub lod_queries_submitted: u64,
    pub lod_query_responses: u64,
    pub lod_query_submission_failures: u64,
    pub stale_lod_query_responses: u64,
    pub failed_lod_queries: u64,
    pub unrecovered_lod_queries: usize,
    pub pending_lod_queries: usize,
    pub lod_chunks_requested: u64,
    pub lod_chunks_ready: u64,
    pub failed_lod_chunks: u64,
    pub unrecovered_lod_chunks: usize,
    pub resident_lod_chunks: usize,
    pub pending_lod_chunks: usize,
    /// Cluster batches whose material is wanted by a view but not yet loaded, so a view with
    /// one is not settled (`--clusters`).
    pub pending_cluster_batches: usize,
    /// Demanded cluster source/material loads that failed permanently; never readiness.
    pub failed_cluster_batches: usize,
    /// Current exact distant-stock cell requests not yet submitted to the database.
    pub queued_far_stock_queries: usize,
    /// Submitted exact distant-stock cells, including answers awaiting the commit budget.
    pub pending_far_stock_queries: usize,
    /// Current distant stock still awaiting fresh scoped render qualification feedback.
    pub pending_far_stock_qualification: usize,
    /// Current-demand exact source query, ownership or asset failures; never readiness.
    pub failed_far_stock_sources: usize,
    /// Diagnostic count of current-world terrain batches without the legacy cross-view proof.
    /// This includes blocks outside the requested view, so it must not gate settling:
    /// exact per-view readiness accepts drawable stock terrain or its cluster replacement.
    pub pending_cluster_terrain_blocks: usize,
    pub ready_lod_terrain_patches: u64,
    pub visible_lod_terrain_patches: usize,
    pub total_lod_query_micros: u64,
    pub max_lod_query_micros: u64,
    pub resident_roots: usize,
    pub duplicate_cell_roots: u64,
    pub orphaned_cell_roots: u64,
    pub missing_cell_roots: u64,
    pub out_of_range_cell_roots: u64,
    pub streaming_invariant_failures: u64,
    pub origin_rebases: u64,
    pub streaming_fixture_validated: bool,
    pub streaming_fixture_failures: u64,
    pub asset_failures: Vec<AssetFailure>,
    // Main's paced-unload, paced-model-spawn and collision counters. The paced-unload and arming
    // counters are portal's own now; the collision ones are driven only by a walked run, where the
    // world's colliders are built ([`builds_world_colliders`]), and stay zero everywhere else.
    pub despawns_this_frame: u64,
    pub max_despawns_per_frame: u64,
    pub despawned_entities: u64,
    pub instances_spawned_this_frame: u64,
    pub max_instances_spawned_per_frame: u64,
    pub arming_queue_depth: usize,
    pub peak_arming_queue_depth: usize,
    pub instances_armed_this_frame: u64,
    pub max_instances_armed_per_frame: u64,
    pub instances_completed_this_scan: u64,
    pub max_instances_completed_per_scan: u64,
    pub retiring_cells: usize,
    pub peak_retiring_cells: usize,
    pub revived_cells: u64,
    /// LOD chunks waiting for their turn in the same unload budget ([`despawn_cells`]) and the
    /// ones it has despawned ([`Self::unloaded_lod_chunks`]). A whole exterior's distant terrain
    /// retires at once on a move into an interior, so this is the counter a door crossing moves.
    pub retiring_lod_chunks: usize,
    pub peak_retiring_lod_chunks: usize,
    pub unloaded_lod_chunks: u64,
    pub revived_lod_chunks: u64,
    /// Chunks kept hidden for the return to their exterior instead of retiring
    /// (`--lod-keep-hidden-in-interiors`): memory the flag holds, priced by its measurement.
    pub held_lod_chunks: usize,
    /// Entities the LOD chunk unloads removed, kept apart from [`Self::despawned_entities`] so
    /// the cell numbers stay comparable with the runs before the chunks were paced.
    pub despawned_lod_entities: u64,
    /// LOD chunks the unload pacer despawned this frame, beside [`Self::despawns_this_frame`]'s
    /// cells: the two together are the roots the frame's budget spent itself on.
    pub lod_despawns_this_frame: usize,
    pub max_lod_despawns_per_frame: usize,
    pub retire_backlog_overflows: u64,
    /// Frames the retire backlog sat above its bound but stayed paced: a flight's backlog
    /// draining at the budget, one drain frame at a time, instead of un-bounding. Kept apart from
    /// [`Self::retire_backlog_overflows`], which counts only escape trips, so a pricing window
    /// can tell a healthy drain from a pacer that lost.
    pub retire_backlog_drains: u64,
    pub physics_fixture_validated: bool,
    pub physics_fixture_failures: u64,
}

#[derive(Debug, Clone, Reflect, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetFailure {
    pub model_path: String,
    pub reference_form_id: u32,
    pub base_form_id: u32,
    pub cell_id: u32,
    pub dependency_chain: Vec<String>,
}

#[derive(Resource, Default)]
struct DiagnosticFallbackAssets {
    mesh: Option<Handle<Mesh>>,
    material: Option<Handle<StandardMaterial>>,
}

#[derive(Resource, Default)]
struct StaticCollisionCache(HashMap<(Option<String>, String), StaticCollisionDecision>);

#[derive(Clone)]
enum StaticCollisionDecision {
    Authored(Vec<AuthoredColliderPart>, Vec<String>),
    RenderProxy(Collider),
    AuthoredAbsent,
    LegacyExcluded,
    Skipped(String),
    /// A movable-type model with an authored dynamic body (#104 phase a).
    Clutter(Box<ClutterSpec>),
    /// A movable-type model with no dynamic body: it keeps no collision, as before.
    ClutterNone,
    /// A movable-type model whose dynamic body could not be built. It gets no collider at all.
    ClutterSkipped(String),
}

/// Colliders and dynamics for one placed dynamic clutter model.
#[derive(Clone)]
struct ClutterSpec {
    parts: Vec<AuthoredColliderPart>,
    body: CollisionBody,
    /// The body's non-convex shapes were replaced by one convex hull.
    convexified: bool,
    /// Further dynamic bodies in the same model, which phase (a) does not simulate.
    ignored_bodies: usize,
    /// `None` when the authored mass or tensor is unusable; density supplies the mass instead.
    mass: Option<MassProperties>,
}

#[derive(Clone)]
struct AuthoredColliderPart {
    translation: Vec3,
    collider: Collider,
}

#[derive(Resource, Default)]
pub(crate) struct TerrainContinuity {
    edges: HashMap<CellKey, TerrainEdges>,
}

#[derive(Clone)]
struct TerrainEdges {
    west: Vec<f32>,
    east: Vec<f32>,
    south: Vec<f32>,
    north: Vec<f32>,
}

impl TerrainEdges {
    /// The four edges of the heights `terrain` currently holds. Registering them after welding
    /// records what was actually drawn, so the next cell welds onto the same surface.
    fn of(terrain: &TerrainSnapshot) -> Self {
        let samples = |side: TerrainEdgeSide| {
            side.points(terrain)
                .into_iter()
                .map(|index| terrain.heights[index])
                .collect()
        };
        Self {
            west: samples(TerrainEdgeSide::West),
            east: samples(TerrainEdgeSide::East),
            south: samples(TerrainEdgeSide::South),
            north: samples(TerrainEdgeSide::North),
        }
    }

    fn side(&self, side: TerrainEdgeSide) -> &[f32] {
        match side {
            TerrainEdgeSide::West => &self.west,
            TerrainEdgeSide::East => &self.east,
            TerrainEdgeSide::South => &self.south,
            TerrainEdgeSide::North => &self.north,
        }
    }
}

/// Which side of an exterior cell an edge belongs to, and which neighbour shares it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerrainEdgeSide {
    West,
    East,
    South,
    North,
}

impl TerrainEdgeSide {
    /// Every side. This is also the order [`validate_and_register_terrain_edges`] welds them in,
    /// and for a corner the order in which its two neighbours are preferred.
    const ALL: [Self; 4] = [Self::West, Self::East, Self::South, Self::North];

    /// The side of the neighbour sharing this edge that lines up position for position with it.
    const fn opposite(self) -> Self {
        match self {
            Self::West => Self::East,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::North => Self::South,
        }
    }

    /// The neighbour sharing this side in the same worldspace.
    fn neighbor_key(self, worldspace_id: u32, grid: IVec2) -> CellKey {
        let (grid_x, grid_y) = match self {
            Self::West => (grid.x - 1, grid.y),
            Self::East => (grid.x + 1, grid.y),
            Self::South => (grid.x, grid.y - 1),
            Self::North => (grid.x, grid.y + 1),
        };
        CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        }
    }

    /// How many height-field points lie along this side.
    fn len(self, terrain: &TerrainSnapshot) -> usize {
        match self {
            Self::West | Self::East => usize::from(terrain.height),
            Self::South | Self::North => usize::from(terrain.width),
        }
    }

    /// The height-field point at `position` along this side, numbered from the same end on both
    /// cells of a shared edge, so a cell's side lines up position for position with the
    /// neighbour's opposite side.
    fn index(self, terrain: &TerrainSnapshot, position: usize) -> usize {
        let width = usize::from(terrain.width);
        let height = usize::from(terrain.height);
        match self {
            Self::West => position * width,
            Self::East => position * width + width - 1,
            Self::South => position,
            Self::North => (height - 1) * width + position,
        }
    }

    fn points(self, terrain: &TerrainSnapshot) -> Vec<usize> {
        let width = usize::from(terrain.width);
        let height = usize::from(terrain.height);
        match self {
            Self::West => (0..height).map(|row| row * width).collect(),
            Self::East => (0..height).map(|row| row * width + width - 1).collect(),
            Self::South => (0..width).collect(),
            Self::North => ((height - 1) * width..height * width).collect(),
        }
    }
}

#[derive(Debug)]
enum CellStatus {
    /// A request is outstanding. The generation is what makes a late answer from a superseded
    /// request detectable, and it changes when a cell has to be loaded again at another detail.
    ///
    /// `replaced` is the root the cell is drawn by *now*, while it is loading at another detail:
    /// a cell that changes tier keeps drawing what it had until its replacement is committed, so a
    /// tier change is never a hole in the world. The old root is despawned in the same frame the
    /// new one is spawned, so the two are never drawn together and a cell still never has two
    /// terrains.
    Loading {
        generation: u64,
        detail: CellDetail,
        replaced: Option<Entity>,
    },
    Resident {
        root: Entity,
        detail: CellDetail,
    },
    /// The cell has fallen out of the keep-set by distance and is waiting for its turn in the
    /// unload budget ([`despawn_cells`]). The map entry, the tier it holds and the root stay, so a
    /// cell that comes back into range before its turn is **revived** as it is instead of being
    /// despawned and requested again - the same root entity keeps drawing the cell throughout, so
    /// there is no flicker and nothing is spawned twice.
    ///
    /// A cell that retires is hidden the frame it does so ([`plan_cells`]): the pacer spreads the
    /// *unload* over several frames, never the drawing, so the drain looks exactly like an unpaced
    /// one that left in a single frame - the cell is off every camera the frame it leaves the
    /// keep-set, then despawned when the budget reaches it. The visibility it had is recorded in
    /// [`RetireHidden`] and given back on revive.
    ///
    /// Only a cell that falls out by *distance* retires. A space change - an interior that is
    /// neither active nor pre-streamed, or the exterior of a worldspace the camera just left on a
    /// door crossing - unloads in the frame it falls out, exactly as before the pacer existed:
    /// such a cell is not visible from where the camera now is, so holding it drawn is not an
    /// option, and its cell root must not outlive the space it belonged to ([`plan_cells`]).
    Retiring {
        root: Entity,
        detail: CellDetail,
    },
    /// The request failed. The detail it failed at is kept: a cell that held nothing as
    /// terrain-only is asked for again in full when the camera reaches it, because the full
    /// request asks a different question (see [`plan_cells`]).
    ///
    /// `attempts` is how many loads of this cell have failed since it was last wanted from
    /// nothing, `reason` is why the last one failed, and `retry_at` is the earliest time the plan
    /// may ask for the cell again at this detail. `retry_at` is `None` once the cell is given up
    /// on - its data is invalid, or the [`MAX_CELL_LOAD_ATTEMPTS`] budget is spent - and a
    /// given-up cell is not requested again until it leaves the unload radius and is wanted from
    /// nothing again.
    ///
    /// `replaced` is the root the cell is still drawn by while it is failed: the terrain root a
    /// full request was upgrading from, kept drawn so a failed full load never opens a hole in the
    /// ground. An interior's failed load has no landscape of its own and carries `None`. The plan's
    /// unload paths treat it exactly like a [`CellStatus::Loading`] `replaced`: a space change
    /// despawns it, and a fall out by distance retires the cell onto it.
    Failed {
        detail: CellDetail,
        attempts: u32,
        reason: String,
        retry_at: Option<Instant>,
        replaced: Option<Entity>,
    },
}

/// A root whose visibility the plan hid when it stopped wanting it - a cell retiring
/// ([`CellStatus::Retiring`]) or a LOD chunk that retired or was held
/// ([`lod::LodChunkStatus::Retiring`], [`lod::LodChunkStatus::Held`]) - carrying the visibility to
/// give back when the root is wanted again.
///
/// It exists so the hide is undone exactly - and only that hide. A root something else already hid,
/// from [`crate::portal::isolate_cells`]'s role of a cell outside the active space to a cell an
/// unpaced unload would have taken the same frame, carries no such marker and is left alone, its own
/// restore untouched. Set in [`plan_cells`] and in the LOD plan
/// ([`lod::plan_lod_chunks`] through [`hide_retiring_root`]), read and removed on the revive in
/// each.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct RetireHidden(Visibility);

/// Read-only proof of streaming-owned distance retirement. An arbitrary hidden
/// cell/reference never produces this fact; the marker remembers an actual hide.
#[derive(SystemParam)]
pub(crate) struct RetiredCellAccess<'w, 's> {
    cells: Query<
        'w,
        's,
        (&'static Visibility, &'static RetireHidden),
        With<crate::world::components::StreamedCellKey>,
    >,
}

impl RetiredCellAccess<'_, '_> {
    pub(crate) fn contains(&self, cell: Entity) -> bool {
        self.cells.get(cell).is_ok_and(|(visibility, original)| {
            *visibility == Visibility::Hidden && original.0 != Visibility::Hidden
        })
    }
}

#[cfg(test)]
pub(crate) fn set_distance_retired_for_cluster_test(
    world: &mut World,
    cell: Entity,
    retired: bool,
) {
    if retired {
        let mut state = bevy::ecs::system::SystemState::<(
            Commands,
            Query<(&Visibility, Option<&RetireHidden>)>,
        )>::new(world);
        {
            let (mut commands, roots) = state
                .get_mut(world)
                .expect("retirement test query resources");
            hide_retiring_root(&mut commands, &roots, cell);
        }
        state.apply(world);
    } else {
        let original = world
            .get::<RetireHidden>(cell)
            .expect("actual retirement marker")
            .0;
        world
            .entity_mut(cell)
            .insert(original)
            .remove::<RetireHidden>();
    }
}

/// The space the player is in. The planner streams only this space: the active interior when
/// there is one, otherwise the exterior cells around the camera in the active worldspace. A load
/// door crossing (`crate::door_crossing`) changes it; nothing else does, so the default (no
/// interior, the configured worldspace) is the behaviour before doors existed.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActiveSpace {
    /// `None` means the worldspace the engine was configured with.
    pub worldspace_id: Option<u32>,
    /// `Some(cell_id)` while the player is inside that interior cell.
    pub interior: Option<u32>,
}

impl ActiveSpace {
    pub fn exterior_worldspace(&self, configured: u32) -> u32 {
        self.worldspace_id.unwrap_or(configured)
    }
}

/// The cells the planner wants resident: the active interior alone, or the exterior window of
/// `radius` cells around `center` in the active worldspace.

#[derive(Resource, Reflect, Debug, Clone, Copy)]
#[reflect(Resource)]
pub struct RenderOrigin(pub IVec2);

impl StreamingWorld {
    /// The exterior cells that are resident at full detail, with their roots: the cells the
    /// distant object LOD can hand over to ([`lod_objects`]).
    fn full_resident_cells(&self) -> impl Iterator<Item = (CellKey, Entity)> + '_ {
        self.cells.iter().filter_map(|(key, status)| match status {
            CellStatus::Resident {
                root,
                detail: CellDetail::Full,
            } => Some((*key, *root)),
            _ => None,
        })
    }

    /// The root entity of `key` while it is resident.
    pub(crate) fn resident_root(&self, key: CellKey) -> Option<Entity> {
        match self.cells.get(&key) {
            Some(CellStatus::Resident { root, .. }) => Some(*root),
            _ => None,
        }
    }

    /// Whether the load of `key` failed.
    pub(crate) fn is_failed(&self, key: CellKey) -> bool {
        matches!(self.cells.get(&key), Some(CellStatus::Failed { .. }))
    }

    #[cfg(test)]
    pub(crate) fn set_resident_for_test(&mut self, key: CellKey, root: Entity) {
        self.cells.insert(
            key,
            CellStatus::Resident {
                root,
                detail: CellDetail::Full,
            },
        );
    }

    #[cfg(test)]
    pub(crate) fn set_failed_for_test(&mut self, key: CellKey) {
        self.cells.insert(
            key,
            CellStatus::Failed {
                detail: CellDetail::Full,
                attempts: 1,
                reason: "failed for test".to_owned(),
                retry_at: None,
                replaced: None,
            },
        );
    }

    #[cfg(test)]
    pub(crate) fn cell_count_for_test(&self) -> usize {
        self.cells.len()
    }
}

/// How far distant terrain reaches, once a worldspace is known to have any: the horizon the
/// exterior fog has to reach for the LOD beyond the full-detail ring to be seen, and not fade to
/// sky colour at the ring's edge.
///
/// Set by [`lod::collect_lod_chunks`] on the first current answer that holds chunks, so a world
/// converted without LOD data never sets it and its fog stays the ring's. It is per worldspace and
/// is kept through an interior: the player coming back out of a house finds the same horizon the
/// moment the exterior returns, with no frame of ring fog while the first query is answered again.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct LodHorizon {
    worldspace_id: Option<u32>,
    reach: f32,
}

impl LodHorizon {
    /// The horizon of `worldspace_id`, in Creation units, or `None` when it has no LOD data.
    pub fn reach_in(&self, worldspace_id: u32) -> Option<f32> {
        (self.worldspace_id == Some(worldspace_id) && self.reach > 0.0).then_some(self.reach)
    }

    /// Records that `worldspace_id` has LOD data reaching `reach` units. A no-op (and no change
    /// for change detection) when it is already known.
    pub(crate) fn set(&mut self, worldspace_id: u32, reach: f32) {
        let wanted = Self {
            worldspace_id: Some(worldspace_id),
            reach,
        };
        if *self != wanted {
            *self = wanted;
        }
    }
}

/// Unloads every cell at once, unpaced: the space the player left must not be drawn or collide
/// for another frame. Entries still loading are dropped, so their responses turn stale.
pub(crate) fn unload_all_cells_now(world: &mut World) -> usize {
    let started = Instant::now();
    let cells: Vec<(CellKey, CellStatus)> = world
        .resource_mut::<StreamingWorld>()
        .cells
        .drain()
        .collect();
    let mut entities = 0usize;
    let mut unloaded = 0u64;
    for (key, status) in &cells {
        if let CellStatus::Resident { root, .. } | CellStatus::Retiring { root, .. } = status {
            entities = entities.saturating_add(despawn_subtree(world, *root));
            unloaded += 1;
        }
        // A failed cell that kept its terrain rescue root loses it here: the space it belonged to
        // is gone, and nothing else references the root once the cell table is drained.
        if let CellStatus::Failed {
            replaced: Some(root),
            ..
        } = status
        {
            entities = entities.saturating_add(despawn_subtree(world, *root));
        }
        world.resource_mut::<TerrainContinuity>().edges.remove(key);
    }
    {
        let mut metrics = world.resource_mut::<StreamingMetrics>();
        metrics.unloaded_cells = metrics.unloaded_cells.saturating_add(unloaded);
        metrics.despawned_entities = metrics.despawned_entities.saturating_add(entities as u64);
        metrics.resident_cells = 0;
        metrics.loading_cells = 0;
        metrics.retiring_cells = 0;
    }
    let mut profiler = world.resource_mut::<ProfilingState>();
    profiler.record_elapsed("streaming/space_switch_unload", started);
    profiler.increment("streaming/despawned_cells", unloaded);
    cells.len()
}

/// The active demand and camera grid used to plan full, terrain and prestream cells.
/// Grouped to stay within Bevy's sixteen-parameter system function limit.
#[derive(SystemParam)]
struct CellPlanningContext<'w, 's> {
    space: Res<'w, ActiveSpace>,
    origin: Res<'w, RenderOrigin>,
    camera: Query<'w, 's, &'static Transform, With<StreamingCamera>>,
}

#[allow(clippy::too_many_arguments)]
fn plan_cells(
    mut commands: Commands,
    config: Res<EngineConfig>,
    database: Res<WorldDatabase>,
    context: CellPlanningContext,
    children: Query<&Children>,
    // The visibility a retiring root is hidden at, and the marker of a hide this plan itself made
    // ([`RetireHidden`]). Read, never written here directly: the writes go through `commands`.
    roots: Query<(&Visibility, Option<&RetireHidden>)>,
    mut streaming: ResMut<StreamingWorld>,
    mut commit_budget: ResMut<StreamingCommitBudget>,
    mut continuity: ResMut<TerrainContinuity>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
    // Present only on a profiled run, where the plugin installs the apply bracket after this
    // system: the frame's unloaded-root count tells it whether the sync point carries despawns.
    unload_flush: Option<ResMut<SpaceUnloadFlushClock>>,
    mut runtime: Option<ResMut<runtime::StreamingRuntime>>,
) {
    let CellPlanningContext {
        space,
        origin,
        camera,
    } = context;
    let plan_started = Instant::now();
    // The immediate-unload span and the subtree counts cost time, so a run that is not
    // profiling pays none of them (the frame the crossing pays for stays unmeasured there).
    let measure_space_unload = config.profile_output_dir.is_some();
    let mut space_unload_ms = 0.0f64;
    let mut space_unload_roots = 0u64;
    let mut space_unload_entities = 0u64;
    // Recorded before anything else, so `has_failed` reads the interior the camera is in even on
    // a frame the plan stops early.
    streaming.active_interior = space.interior;
    commit_budget.remaining = runtime
        .as_ref()
        .and_then(|runtime| runtime.decision)
        .map_or(config.max_cell_commits_per_frame, |decision| {
            decision.budgets.max_cell_commits
        });
    commit_budget.lod_remaining = match config.max_lod_requests_per_frame {
        0 => usize::MAX,
        count => count,
    };
    commit_budget.commits = 0;
    commit_budget.lod_priority = !commit_budget.lod_priority;
    commit_budget.reserved_for_lod = false;
    let Ok(camera) = camera.single() else {
        return;
    };
    let center = window_center(&config, camera.translation, origin.0);
    // The ring of the worldspace the camera is in, resolved this frame: a run that did not name one
    // draws none where the world has its own terrain LOD, and keeps it where it has none
    // ([`EngineConfig::effective_terrain_radius`]). Resolved here rather than read from a resource
    // so the plan cannot run on a ring from the space the camera was in last frame.
    let terrain_radius = config.effective_terrain_radius(
        database.has_lod_for(space.exterior_worldspace(config.worldspace_id)),
    );
    let mut wanted = wanted_cells(
        &space,
        config.worldspace_id,
        config.stream_radius,
        terrain_radius,
        center,
    );
    let controlled = config.streaming_controls_enabled();
    if config.prioritize_streaming || controlled {
        // Keep door prestreams and full-detail cells ahead of the terrain ring,
        // then preserve main's nearest-first admission within each class.
        wanted.sort_by_key(|(key, detail)| {
            let distance = match *key {
                CellKey::Exterior { grid_x, grid_y, .. } => {
                    let dx = i128::from(grid_x) - i128::from(center.x);
                    let dy = i128::from(grid_y) - i128::from(center.y);
                    dx * dx + dy * dy
                }
                CellKey::Interior(_) => 0,
            };
            (
                *detail == CellDetail::Terrain,
                distance,
                cell_order_key(*key),
            )
        });
    }
    let request_limit = if controlled {
        commit_budget.remaining
    } else {
        usize::MAX
    };
    let outstanding_limit = if controlled {
        match config.streaming_backlog_limit() {
            0 => 256,
            limit => limit,
        }
    } else {
        usize::MAX
    };
    let mut submitted = 0;
    for (key, detail) in wanted {
        // Whether this frame re-asks a failed cell for the full load that failed: the failure
        // record's backoff has passed and its attempt budget is not spent. The re-ask goes through
        // the ordinary full path below, so a cell that can succeed on a retry comes back with the
        // terrain root it holds riding along as `replaced` - the ground never disappears.
        let full_retry = detail == CellDetail::Full
            && streaming
                .full_failed
                .get(&key)
                .is_some_and(|failure| failure.retryable(Instant::now()));
        // A cell whose full request failed is not asked for in full again while it stays planned:
        // its landscape is asked for instead, so a cell the camera stands in is never a hole in the
        // ground because its references failed (see `fail_cell`) - unless this frame re-asks it,
        // which is the bounded retry above.
        let detail = if detail == CellDetail::Full
            && streaming.full_failed.contains_key(&key)
            && !full_retry
        {
            CellDetail::Terrain
        } else {
            detail
        };
        // Whether this is a retry of a failure at exactly the tier the plan is asking for again:
        // only the interior the camera stands in is ever asked for again at the tier it failed at
        // ([`retry_failed_interior`]), and only while its retry budget lasts.
        let retrying = retry_failed_interior(&key, space.interior, &streaming);
        // What the cell is streamed as now, and the root that draws it.
        let (held, root) = match streaming.cells.get(&key) {
            None => (None, None),
            // A failed cell is asked for again only at a *different* tier than the one it failed
            // at, so a grid the worldspace does not cover - hundreds of them, in the ring - stays
            // failed instead of being re-requested every frame. A tier change asks a different
            // question: a cell that held nothing as terrain-only may still hold the references,
            // lights and doors the full request asks for, and a cell the camera is standing in must
            // be complete; a full request that failed may have failed on its references, and the
            // ring can still draw that cell's landscape. Each tier is tried once - except the
            // interior the camera stands in, which is the one cell whose failure leaves it nowhere
            // to be, and which is retried while its budget lasts, and the exterior full failure
            // whose backoff has passed, which this frame re-asks in full (`full_retry`). A failed
            // cell that still draws its terrain rescue hands that root to the replacement below,
            // so the ground keeps being drawn while it is asked for again.
            Some(CellStatus::Failed {
                detail: failed,
                replaced,
                ..
            }) => {
                if *failed == detail && !retrying && !full_retry {
                    continue;
                }
                (None, *replaced)
            }
            Some(CellStatus::Loading {
                detail, replaced, ..
            }) => (Some(*detail), *replaced),
            Some(CellStatus::Resident { root, detail }) => (Some(*detail), Some(*root)),
            // A retiring cell holds its tier and its root the way a resident one does, so a plan
            // that wants it again at its own tier revives it below, and one that wants the other
            // tier submits the replacement through the same `replaced` path a resident takes.
            Some(CellStatus::Retiring { root, detail }) => (Some(*detail), Some(*root)),
        };
        if held == Some(detail) {
            continue;
        }
        if submitted >= request_limit || streaming.outstanding_cells.len() >= outstanding_limit {
            break;
        }
        if streaming.outstanding_cells.contains_key(&key) {
            continue;
        }
        let generation = streaming.generation.wrapping_add(1);
        if let Some(runtime) = runtime.as_mut()
            && (!runtime.cell_requests_allowed()
                || !runtime.reserve_cell_attempt(
                    key,
                    generation,
                    detail,
                    builds_world_colliders(&config) && detail == CellDetail::Full,
                ))
        {
            continue;
        }
        // The request goes first: a cell only gives up the root that draws it once its replacement
        // is on the worker's queue. A full queue (or a stopped worker) leaves the cell exactly as
        // it was - still drawn, still resident - rather than despawned with a status that says it
        // is there, which the lifecycle check reads as a missing root.
        streaming.generation = generation;
        if database
            .try_request(DatabaseRequest::Load {
                generation,
                key,
                detail,
                queued_at: Instant::now(),
            })
            .is_err()
        {
            if let Some(runtime) = runtime.as_mut() {
                runtime.retire_cell_attempt(key, generation, detail);
            }
            // No room for this request this frame. The wanted list is ordered nearest first, so
            // stopping keeps the cells closest to the camera at the head of the queue; the rest are
            // asked for again next frame.
            break;
        }
        submitted += 1;
        streaming.outstanding_cells.insert(key, generation);
        if retrying {
            // A retry is spent once the queue takes it: a request the full queue refused costs
            // nothing, and the cell is asked for again next frame on the same budget.
            *streaming.failed_retries.entry(key).or_default() += 1;
        }
        if full_retry {
            // Say what is being asked for again, and why it failed last time: the record the
            // attempt budget is spent against.
            let last = match streaming.cells.get(&key) {
                Some(CellStatus::Failed {
                    attempts, reason, ..
                }) => Some((*attempts, reason.clone())),
                // The rescue committed while the record waited: the failure record still says
                // what the last full load died of.
                _ => streaming
                    .full_failed
                    .get(&key)
                    .map(|failure| (failure.attempts, failure.reason.clone())),
            };
            let (attempts, reason) = last.unwrap_or((0, String::new()));
            debug!(?key, attempts, %reason, "asking for the failed cell in full again");
            // The same for an exterior's full retry: counted once the queue takes the re-ask.
            metrics.failed_cell_retries = metrics.failed_cell_retries.saturating_add(1);
            profiler.event(format!("{key:?}"), "full_retry_requested", None);
        }
        match (held, detail) {
            (Some(CellDetail::Terrain), CellDetail::Full) => {
                metrics.cells_upgraded = metrics.cells_upgraded.saturating_add(1);
                profiler.event(format!("{key:?}"), "upgraded_to_full", None);
            }
            (Some(CellDetail::Full), CellDetail::Terrain) => {
                metrics.cells_downgraded = metrics.cells_downgraded.saturating_add(1);
                profiler.event(format!("{key:?}"), "downgraded_to_terrain", None);
            }
            _ => {}
        }
        metrics.requests_submitted += 1;
        profiler.increment("streaming/requests", 1);
        profiler.event(format!("{key:?}"), "requested", None);
        // A cell being loaded at another detail keeps drawing its old root until the commit swaps
        // it; a fresh cell has none.
        streaming.cells.insert(
            key,
            CellStatus::Loading {
                generation,
                detail,
                replaced: root,
            },
        );
    }
    let pinned = std::mem::take(&mut streaming.pinned);
    let mut revived = 0u64;
    streaming.cells.retain(|key, status| {
        let keep = pinned.contains(key)
            || cell_within_unload_radius(
                *key,
                &space,
                config.worldspace_id,
                center,
                config.unload_radius,
                terrain_radius,
            );
        if keep {
            // A cell that comes back before the unload budget reaches it is revived as it is: the
            // root it kept starts drawing it again - the same entity, so nothing is spawned a
            // second time - and it is never requested again.
            if let CellStatus::Retiring { root, detail } = *status {
                *status = CellStatus::Resident { root, detail };
                // Give back the visibility the retire took, and only that: a root the portal
                // isolation hid carries no marker of ours, and the isolation restores its own.
                if let Ok((_, Some(hidden))) = roots.get(root) {
                    let original = hidden.0;
                    commands.entity(root).try_insert(original);
                    commands.entity(root).try_remove::<RetireHidden>();
                }
                if config.clusters.is_some() && detail == CellDetail::Full {
                    far_stock::queue_revive(&mut commands, root, *key);
                }
                revived += 1;
            }
            return true;
        }
        // The cell has left the keep-set. One that falls out by distance retires: the entry and
        // the root stay until the budget in `despawn_cells` reaches it. Everything else is a space
        // change - an interior left behind, a worldspace just left on a door crossing - and
        // unloads in this frame, exactly as it did before the pacer existed.
        if paces_its_unload(*key, &space, config.worldspace_id) {
            match status {
                CellStatus::Resident { root, detail } => {
                    if config.clusters.is_some() {
                        far_stock::queue_release(&mut commands, *root);
                    }
                    hide_retiring_root(&mut commands, &roots, *root);
                    *status = CellStatus::Retiring {
                        root: *root,
                        detail: *detail,
                    };
                    return true;
                }
                // Already waiting for its turn in the budget.
                CellStatus::Retiring { .. } => return true,
                // A cell loading a replacement - or failed while it still drew one - keeps the
                // root it is drawn by, at the tier that root was committed at
                // ([`other_detail`]).
                CellStatus::Loading {
                    replaced: Some(root),
                    detail,
                    ..
                }
                | CellStatus::Failed {
                    replaced: Some(root),
                    detail,
                    ..
                } => {
                    if config.clusters.is_some() {
                        far_stock::queue_release(&mut commands, *root);
                    }
                    hide_retiring_root(&mut commands, &roots, *root);
                    *status = CellStatus::Retiring {
                        root: *root,
                        detail: other_detail(*detail),
                    };
                    return true;
                }
                // Nothing was spawned, so there is no subtree to pace: drop the entry now and let
                // the in-flight response turn into a stale one. A failed or given-up cell is
                // dropped here too, so coming back into range starts its attempt budget from
                // nothing again.
                CellStatus::Loading { replaced: None, .. }
                | CellStatus::Failed { replaced: None, .. } => {
                    metrics.unloaded_cells += 1;
                    continuity.edges.remove(key);
                    profiler.event(format!("{key:?}"), "unload_dropped", None);
                    return false;
                }
            }
        }
        metrics.unloaded_cells += 1;
        continuity.edges.remove(key);
        profiler.event(format!("{key:?}"), "unloaded", None);
        // A loading cell that still draws the root it is replacing loses that root too: it is
        // the only root the cell has. A failed cell that kept its terrain rescue root loses it
        // the same way - nothing else references it once the entry is gone. Every arm despawns
        // at most one root, held in one place so the span and the subtree count below see it.
        let unload_root: Option<Entity> = match status {
            CellStatus::Resident { root, .. } | CellStatus::Retiring { root, .. } => Some(*root),
            CellStatus::Loading { replaced, .. } | CellStatus::Failed { replaced, .. } => *replaced,
        };
        let unload_started = measure_space_unload.then(Instant::now);
        if let Some(root) = unload_root {
            if config.clusters.is_some() {
                far_stock::queue_despawn(&mut commands, root);
            } else {
                commands.entity(root).try_despawn();
            }
        }
        // The entities the queued despawn will remove. Only counted when profiling, because
        // walking a subtree costs time on the very frame a crossing pays for.
        if measure_space_unload && let Some(root) = unload_root {
            space_unload_entities += 1;
            if children.contains(root) {
                space_unload_entities += children.iter_descendants(root).count() as u64;
            }
        }
        if let Some(started) = unload_started {
            space_unload_ms += started.elapsed().as_secs_f64() * 1000.0;
        }
        space_unload_roots += u64::from(unload_root.is_some());
        false
    });
    streaming.pinned = pinned;
    if revived > 0 {
        metrics.revived_cells = metrics.revived_cells.saturating_add(revived);
        profiler.increment("streaming/revived_cells", revived);
    }
    let StreamingWorld {
        cells,
        full_failed,
        failed_retries,
        ..
    } = &mut *streaming;
    full_failed.retain(|key, _| cells.contains_key(key));
    // A retry budget is spent on one stay in the plan: a cell that leaves it - which is how the
    // camera leaves a failed interior, and comes back to it - starts with the whole budget again.
    failed_retries.retain(|key, _| cells.contains_key(key));
    metrics.resident_cells = streaming
        .cells
        .values()
        .filter(|status| matches!(status, CellStatus::Resident { .. }))
        .count();
    metrics.loading_cells = streaming
        .cells
        .values()
        .filter(|status| matches!(status, CellStatus::Loading { .. }))
        .count();
    metrics.retiring_cells = streaming
        .cells
        .values()
        .filter(|status| matches!(status, CellStatus::Retiring { .. }))
        .count();
    metrics.terrain_cells = streaming
        .cells
        .values()
        .filter(|status| {
            matches!(
                status,
                CellStatus::Resident {
                    detail: CellDetail::Terrain,
                    ..
                }
            )
        })
        .count();
    metrics.peak_resident_cells = metrics.peak_resident_cells.max(metrics.resident_cells);
    metrics.peak_loading_cells = metrics.peak_loading_cells.max(metrics.loading_cells);
    metrics.peak_retiring_cells = metrics.peak_retiring_cells.max(metrics.retiring_cells);
    metrics.peak_terrain_cells = metrics.peak_terrain_cells.max(metrics.terrain_cells);
    profiler.set_gauge("streaming/resident_cells", metrics.resident_cells as f64);
    profiler.set_gauge("streaming/loading_cells", metrics.loading_cells as f64);
    profiler.set_gauge("streaming/retiring_cells", metrics.retiring_cells as f64);
    profiler.set_gauge("streaming/terrain_cells", metrics.terrain_cells as f64);
    // The frame's immediate unload of space-change cells. The span covers only the plan's own
    // bookkeeping for that branch - the despawns ride `Commands` and are applied (and spanned)
    // at the sync point after this system, as `streaming/space_change_unload_apply`.
    //
    // The two counts are the run's **maximum**, not its last: this frame unloads a space change
    // and is zero on every quiet frame after it, so the last value the report would keep is always
    // zero and a crossing could not be sized from it (see `ProfilingState::set_max_gauge`).
    if measure_space_unload {
        profiler.set_max_gauge(
            "streaming/space_change_unload_roots",
            space_unload_roots as f64,
        );
        profiler.set_max_gauge(
            "streaming/space_change_unload_entities",
            space_unload_entities as f64,
        );
        if space_unload_roots > 0 {
            profiler.record_ms("streaming/space_change_unload", space_unload_ms);
        }
    }
    if let Some(mut unload_flush) = unload_flush {
        unload_flush.pending_roots = space_unload_roots;
    }
    profiler.record_elapsed("streaming/plan_cells", plan_started);
}

/// The apply bracket around the sync point after [`plan_cells`], and whether this frame's
/// space-change unload queued despawns into it. Held by the bracket systems that span
/// `streaming/space_change_unload_apply`, which only exist on a profiled run; [`plan_cells`] fills
/// `pending_roots` so the span measures only the application that carries despawns, not the empty
/// sync point of a quiet frame.
#[derive(Resource, Default)]
struct SpaceUnloadFlushClock {
    started: Option<Instant>,
    pending_roots: u64,
}

/// Opens the bracket around the schedule's sync point after [`plan_cells`], before its deferred
/// commands (the space-change despawns among them) are applied.
fn stamp_space_unload_flush(mut clock: ResMut<SpaceUnloadFlushClock>) {
    clock.started = Some(Instant::now());
}

/// Closes the bracket and records `streaming/space_change_unload_apply`: the time the executor
/// spent applying the pending command buffers at that sync point. A frame whose plan unloaded
/// nothing records nothing - its sync point is empty.
fn record_space_unload_flush(
    mut clock: ResMut<SpaceUnloadFlushClock>,
    mut profiler: ResMut<ProfilingState>,
) {
    let started = clock.started.take();
    let carried_despawns = clock.pending_roots > 0;
    clock.pending_roots = 0;
    if let Some(started) = started
        && carried_despawns
    {
        profiler.record_elapsed("streaming/space_change_unload_apply", started);
    }
}

/// The apply bracket around the sync point after [`collect_cells`], and whether this frame's plan
/// committed any cell into it. Held by the bracket systems that span `streaming/commit_apply`,
/// which only exist on a profiled run; the stamping system reads the frame's commit count out of
/// [`StreamingCommitBudget`] so the span measures only a sync point that carries a commit, not the
/// empty one of a quiet frame.
///
/// A commit is queued as deferred commands by [`collect_cells`] (cell roots, references, the scene
/// instances they request), so the sync point is where the frame's spawning is actually paid - the
/// mirror image of [`SpaceUnloadFlushClock`], which spans the frame's despawns.
#[derive(Resource, Default)]
struct CellCommitFlushClock {
    started: Option<Instant>,
    pending_commits: usize,
}

/// Opens the bracket around the schedule's sync point after [`collect_cells`], before the deferred
/// commands a commit queued (cell roots, references, scene spawns) are applied.
fn stamp_cell_commit_flush(
    budget: Res<StreamingCommitBudget>,
    mut clock: ResMut<CellCommitFlushClock>,
) {
    clock.started = Some(Instant::now());
    clock.pending_commits = budget.commits;
}

/// Closes the bracket and records `streaming/commit_apply`: the time the executor spent applying
/// the command buffers pending at that sync point. A frame whose plan committed nothing records
/// nothing - its sync point is empty.
fn record_cell_commit_flush(
    mut clock: ResMut<CellCommitFlushClock>,
    mut profiler: ResMut<ProfilingState>,
) {
    let started = clock.started.take();
    let carried_commits = clock.pending_commits > 0;
    clock.pending_commits = 0;
    if let Some(started) = started
        && carried_commits
    {
        profiler.record_elapsed("streaming/commit_apply", started);
    }
}

/// Whether a cell that has left the keep-set leaves it by *distance* - the case the unload pacer
/// covers - rather than because the camera changed space.
///
/// Only a cell of the worldspace the camera is standing in, while it is outdoors, can have been
/// left by flying away from it: that is a cell still drawn beside the ones the camera wants, and
/// its unload is what the budget spreads. An interior is never paced (an interior that is neither
/// active nor pre-streamed must stop being drawn the frame it falls out), and neither is an
/// exterior of a worldspace the camera just left, on this frame or on the door crossing before it:
/// those are the cells of a space the camera is no longer in, and holding them drawn is not an
/// option. Both unload in [`plan_cells`] itself, as they did before the pacer existed.
fn paces_its_unload(key: CellKey, active: &ActiveSpace, configured_worldspace: u32) -> bool {
    match key {
        CellKey::Exterior { worldspace_id, .. } => {
            active.interior.is_none()
                && worldspace_id == active.exterior_worldspace(configured_worldspace)
        }
        CellKey::Interior(_) => false,
    }
}

/// Hides the root of a cell - or a LOD chunk, which retires and is held the same way - that has
/// just fallen out of the keep-set, so it stops being drawn the frame it leaves instead of every
/// frame until the unload budget reaches it ([`CellStatus::Retiring`],
/// [`lod::LodChunkStatus::Retiring`]). The visibility the root had is kept in [`RetireHidden`] for
/// the revive to give back.
///
/// A root that is already hidden is left alone, marker and all: the portal isolation hides a cell
/// outside the active space the same frame a distance unload retires it, and that hide is the
/// isolation's to undo, not this plan's. Skipping it is what keeps the two from fighting - a marker
/// written over an isolation's hide would restore `Hidden` onto a cell the isolation had just made
/// active again.
fn hide_retiring_root(
    commands: &mut Commands,
    roots: &Query<'_, '_, (&Visibility, Option<&RetireHidden>)>,
    root: Entity,
) {
    let Ok((visibility, hidden)) = roots.get(root) else {
        return;
    };
    if hidden.is_some() || *visibility == Visibility::Hidden {
        return;
    }
    let original = *visibility;
    commands.queue(move |world: &mut World| {
        if !far_stock::removal_blocked(world, root)
            && let Ok(mut entity) = world.get_entity_mut(root)
        {
            entity.insert((RetireHidden(original), Visibility::Hidden));
        }
    });
}

/// The tier a cell holds while it is being replaced: a cell loading a full replacement still draws
/// the terrain-only root it had, and one loading a terrain-only replacement its full root.
///
/// Exact because the plan only submits a request when the cell does not already hold the tier it
/// is asking for ([`plan_cells`]), so the two tiers of a `Loading` cell's root and its request are
/// always different.
fn other_detail(detail: CellDetail) -> CellDetail {
    match detail {
        CellDetail::Full => CellDetail::Terrain,
        CellDetail::Terrain => CellDetail::Full,
    }
}

/// How many cells may wait for their turn before the unload pacer's escape arms.
///
/// One whole window at the unload radius, `(2 * r + 1)^2` cells: what a single teleport or one
/// frame of fast flight was assumed to retire at once. That assumption does not hold - a long
/// jump away retires the whole ring behind the camera, several windows' worth - so the bound is
/// not a cap by itself. [`despawn_cells`] un-bounds the drain only when a backlog above it also
/// failed to shrink against the last frame's: sustained flight retiring cells faster than the
/// budget drains them, the case where holding on to the backlog costs more than one larger frame.
fn retire_backlog_bound(unload_radius: i32) -> usize {
    let side = 2 * unload_radius.max(0) as usize + 1;
    side * side
}

/// Stable order for the unload budget, so a replayed crossing unloads the same cells on the same
/// frames whatever order the status map iterates in.
fn cell_order_key(key: CellKey) -> (u32, i32, i32, u32) {
    match key {
        CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        } => (0, grid_y, grid_x, worldspace_id),
        CellKey::Interior(cell_id) => (1, 0, 0, cell_id),
    }
}

/// Unloads retiring cell roots and LOD chunks under the frame's one unload budget.
///
/// The budget is wall-clock time (`EngineConfig::max_unload_ms_per_frame`): roots go in stable
/// order until the clock is spent, always at least one of them, so a backlog bigger than the
/// budget drains over several frames instead of landing on one - fast flight away from the camera
/// retires a whole ring of cells at once, and a move into an interior retires a whole exterior's
/// distant terrain at once, both far past what a per-frame count can pace. Cells go first, in
/// [`cell_order_key`] order, then LOD chunks in [`lod::LodStreaming::retiring_chunks`]'s order
/// ([`ChunkKey`]'s own), all off the same clock: one budget, not two.
///
/// Scope: this paces the *distance* retires of the space the camera stands in, and the LOD chunk
/// unload - including the whole exterior ring when the camera walks through a door into an
/// interior. A door crossing's unload of the *cells* of the space the camera left happens in
/// [`plan_cells`] in the frame it falls out (`paces_its_unload` is false for it) and is not paced
/// here.
///
/// Replay stability: the order the roots unload in is stable across runs; which frame each one
/// unloads on depends on machine speed, because the budget is wall-clock.
///
/// `0` turns the clock off and paces by the old count (`max_cell_unloads_per_frame`, now counting
/// cells and chunk roots together); only that count at `0` as well is exactly the old unbudgeted
/// path, since the escape below - which trips on drain failure, not on size - came with the
/// budget.
///
/// Whatever paces it, the drain keeps one escape: a *cell* backlog above
/// [`retire_backlog_bound`] that also failed to shrink against the last frame's says the pacer is
/// losing - flight retiring cells faster than it drains - and un-bounds for that frame. A burst
/// that shrinks every drain frame never trips it. The escape counts cells only: a chunk backlog is
/// what the chunk grid holds and shrinks by at least one root a frame, so sustained flight is not
/// what grows it.
///
/// This is an exclusive system on purpose: a command-buffered `despawn` only queues the removal and
/// the real work happens later at a schedule sync point, where no span can see it - and the whole
/// point of the budget is to keep that work off one frame.
fn despawn_cells(world: &mut World) {
    let (count_budget, spent_by, backlog_bound) = {
        let config = world.resource::<EngineConfig>();
        (
            // The count path's `0` is unbudgeted: every retiring root unloads at once, as before
            // the budget. It paces the drain only while the time budget is off.
            match config.max_cell_unloads_per_frame {
                0 => usize::MAX,
                budget => budget,
            },
            (config.max_unload_ms_per_frame > 0.0)
                .then(|| Duration::from_secs_f64(config.max_unload_ms_per_frame / 1000.0)),
            retire_backlog_bound(config.unload_radius),
        )
    };
    let mut retiring: Vec<(CellKey, Entity)> = world
        .resource::<StreamingWorld>()
        .cells
        .iter()
        .filter_map(|(key, status)| match status {
            CellStatus::Retiring { root, .. } => Some((*key, *root)),
            _ => None,
        })
        .collect();
    let retiring_chunks = world.resource::<lod::LodStreaming>().retiring_chunks();
    let retiring_blocks = world
        .get_resource::<lod_objects::ObjectLodStreaming>()
        .map(lod_objects::ObjectLodStreaming::retiring_blocks)
        .unwrap_or_default();
    if retiring.is_empty() && retiring_chunks.is_empty() && retiring_blocks.is_empty() {
        world.resource_mut::<StreamingWorld>().last_retire_backlog = 0;
        let mut metrics = world.resource_mut::<StreamingMetrics>();
        metrics.despawns_this_frame = 0;
        metrics.lod_despawns_this_frame = 0;
        let mut profiler = world.resource_mut::<ProfilingState>();
        profiler.set_gauge("streaming/despawns_this_frame", 0.0);
        profiler.set_gauge("streaming/lod_despawns_this_frame", 0.0);
        return;
    }
    retiring.sort_by_key(|(key, _)| cell_order_key(*key));
    let cell_backlog = retiring.len();
    let chunk_backlog = retiring_chunks.len();
    let block_backlog = retiring_blocks.len();
    // The escape trips on drain failure, not on size: a flight's burst sits above the bound the
    // frame it lands and shrinks from then on, while a pacer losing the race leaves the backlog no
    // smaller than the last frame left it. A first-ever backlog has no last frame to fail against
    // (`last_retire_backlog` is zero), so it drains paced too.
    let last_backlog = world.resource::<StreamingWorld>().last_retire_backlog;
    let overflow = cell_backlog > backlog_bound && last_backlog > 0 && cell_backlog >= last_backlog;
    let started = Instant::now();
    let mut cell_entities = 0usize;
    let mut chunk_entities = 0usize;
    let mut cells = 0usize;
    let mut chunks = 0usize;
    let mut blocks = 0usize;
    let mut block_entities = 0usize;
    match spent_by {
        // The time budget: walk the stable order and read the clock after each subtree. A subtree
        // that has started always finishes, and at least one root always goes, so the backlog
        // always drains. Cells and chunks share the one clock - the chunks only see what the
        // cells left of it.
        Some(spent) if !overflow => {
            for (_, root) in retiring.iter() {
                cell_entities = cell_entities.saturating_add(despawn_subtree(world, *root));
                cells += 1;
                if started.elapsed() >= spent {
                    break;
                }
            }
            if cells == cell_backlog {
                for (_, root) in retiring_chunks.iter() {
                    chunk_entities = chunk_entities.saturating_add(despawn_subtree(world, *root));
                    chunks += 1;
                    if started.elapsed() >= spent {
                        break;
                    }
                }
            }
            // The object LOD blocks go last, off the same clock.
            if cells == cell_backlog && chunks == chunk_backlog {
                for (_, root) in retiring_blocks.iter() {
                    block_entities = block_entities.saturating_add(despawn_subtree(world, *root));
                    blocks += 1;
                    if started.elapsed() >= spent {
                        break;
                    }
                }
            }
        }
        // The count budget, or the escape: a fixed prefix of cells-then-chunks, the escape taking
        // the whole cell backlog.
        _ => {
            let budget = if overflow {
                cell_backlog
            } else {
                count_budget.min(cell_backlog + chunk_backlog)
            };
            cells = budget.min(cell_backlog);
            chunks = (budget - cells).min(chunk_backlog);
            blocks = budget - cells - chunks;
            for (_, root) in retiring.iter().take(cells) {
                cell_entities = cell_entities.saturating_add(despawn_subtree(world, *root));
            }
            for (_, root) in retiring_chunks.iter().take(chunks) {
                chunk_entities = chunk_entities.saturating_add(despawn_subtree(world, *root));
            }
            for (_, root) in retiring_blocks.iter().take(blocks) {
                block_entities = block_entities.saturating_add(despawn_subtree(world, *root));
            }
        }
    }
    let remaining = cell_backlog - cells;
    let remaining_chunks = chunk_backlog - chunks;
    if cells > 0 {
        world.resource_mut::<lod::LodStreaming>().visibility_dirty = true;
    }
    {
        let mut streaming = world.resource_mut::<StreamingWorld>();
        // The next frame compares against this frame's backlog - not the remainder, or a drain of
        // one cell a frame would read as "no shrink" - cleared when the drain emptied it, so a
        // fresh burst after a finished drain never reads as sustained growth. Cells only: the
        // escape it feeds is the cells' ([`retire_backlog_bound`]).
        streaming.last_retire_backlog = if remaining == 0 { 0 } else { cell_backlog };
        for (key, _) in retiring.iter().take(cells) {
            streaming.cells.remove(key);
        }
    }
    {
        let mut lod = world.resource_mut::<lod::LodStreaming>();
        for (key, _) in retiring_chunks.iter().take(chunks) {
            lod.chunk_despawned(*key);
        }
    }
    if blocks > 0 {
        let mut objects = world.resource_mut::<lod_objects::ObjectLodStreaming>();
        for (key, _) in retiring_blocks.iter().take(blocks) {
            objects.block_despawned(*key);
        }
        let mut metrics = world.resource_mut::<StreamingMetrics>();
        metrics.unloaded_lod_object_blocks = metrics
            .unloaded_lod_object_blocks
            .saturating_add(blocks as u64);
        metrics.despawned_lod_entities = metrics
            .despawned_lod_entities
            .saturating_add(block_entities as u64);
    }
    for (key, _) in retiring.iter().take(cells) {
        world.resource_mut::<TerrainContinuity>().edges.remove(key);
        world
            .resource_mut::<ProfilingState>()
            .event(format!("{key:?}"), "unloaded", None);
    }
    for (key, _) in retiring_chunks.iter().take(chunks) {
        world
            .resource_mut::<ProfilingState>()
            .event(format!("{key:?}"), "unloaded", None);
    }
    let max_despawns_per_frame = {
        let mut metrics = world.resource_mut::<StreamingMetrics>();
        metrics.retiring_cells = remaining;
        metrics.peak_retiring_cells = metrics.peak_retiring_cells.max(cell_backlog);
        metrics.unloaded_cells = metrics.unloaded_cells.saturating_add(cells as u64);
        metrics.despawns_this_frame = cells as u64;
        metrics.despawned_entities = metrics
            .despawned_entities
            .saturating_add(cell_entities as u64);
        metrics.max_despawns_per_frame = metrics.max_despawns_per_frame.max(cells as u64);
        metrics.retiring_lod_chunks = remaining_chunks;
        metrics.peak_retiring_lod_chunks = metrics.peak_retiring_lod_chunks.max(chunk_backlog);
        metrics.unloaded_lod_chunks = metrics.unloaded_lod_chunks.saturating_add(chunks as u64);
        metrics.lod_despawns_this_frame = chunks;
        metrics.despawned_lod_entities = metrics
            .despawned_lod_entities
            .saturating_add(chunk_entities as u64);
        metrics.max_lod_despawns_per_frame = metrics.max_lod_despawns_per_frame.max(chunks);
        if overflow {
            metrics.retire_backlog_overflows = metrics.retire_backlog_overflows.saturating_add(1);
        } else if cell_backlog > backlog_bound {
            // A drain frame: the backlog sat above the bound and the pacer kept pacing it, a
            // flight's backlog going out one budgeted frame at a time.
            metrics.retire_backlog_drains = metrics.retire_backlog_drains.saturating_add(1);
        }
        (
            metrics.max_despawns_per_frame,
            metrics.max_lod_despawns_per_frame,
        )
    };
    let mut profiler = world.resource_mut::<ProfilingState>();
    profiler.record_elapsed("streaming/cell_despawn", started);
    profiler.increment("streaming/despawned_cells", cells as u64);
    profiler.increment("streaming/despawned_entities", cell_entities as u64);
    profiler.increment("streaming/despawned_lod_chunks", chunks as u64);
    profiler.increment("streaming/despawned_lod_entities", chunk_entities as u64);
    profiler.set_gauge("streaming/despawns_this_frame", cells as f64);
    profiler.set_gauge("streaming/lod_despawns_this_frame", chunks as f64);
    profiler.set_gauge(
        "streaming/max_despawns_per_frame",
        max_despawns_per_frame.0 as f64,
    );
    profiler.set_gauge(
        "streaming/max_lod_despawns_per_frame",
        max_despawns_per_frame.1 as f64,
    );
    profiler.set_gauge("streaming/retiring_cells", remaining as f64);
    profiler.set_gauge("streaming/retiring_lod_chunks", remaining_chunks as f64);
    if overflow {
        profiler.increment("streaming/retire_backlog_overflows", 1);
        profiler.event(
            "streaming",
            "retire_backlog_overflow",
            Some(cell_backlog as f64),
        );
        warn!(
            backlog = cell_backlog,
            bound = backlog_bound,
            "retire backlog outgrew the drain; unloading every retiring cell at once"
        );
    } else if cell_backlog > backlog_bound {
        profiler.increment("streaming/retire_backlog_drains", 1);
        profiler.event(
            "streaming",
            "retire_backlog_drain",
            Some(cell_backlog as f64),
        );
    }
}

/// Despawns a cell root and everything under it, returning the number of entities that removes.
///
/// The count walks `Children` first, so the batch span covers the accounting as well as the
/// recursive removal. The walk is only paid on frames that unload cells.
fn despawn_subtree(world: &mut World, root: Entity) -> usize {
    if world.contains_resource::<far_stock::FarStock>() {
        far_stock::release(world, root);
    }
    if far_stock::removal_blocked(world, root) {
        return 0;
    }
    let mut entities = 0usize;
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        entities += 1;
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    if let Ok(entity) = world.get_entity_mut(root) {
        entity.despawn();
    }
    entities
}

// -----------------------------------------------------------------------------------------
// The retire-visibility probe (`--probe-retire-visibility`): a measurement-only run that proves
// a cell held in the retire backlog - hidden the same frame it retires, drained over several
// frames under `max_unload_ms_per_frame` - never shows in a view while it drains. Off unless the
// flag names a directory.
// -----------------------------------------------------------------------------------------

/// Backlog frames the probe photographs first, whatever the visible count: enough frames of one
/// drain to see the retiring cell is gone from the drawn image while the table still holds it.
const PROBE_FIRST_BACKLOG_CAPTURES: usize = 5;
/// The most PNGs the probe writes overall, so a run that does find hits cannot fill the disk.
const PROBE_MAX_CAPTURES: usize = 20;
/// Quiet frames after the backlog empties before the probe writes its JSON and exits, so the
/// screenshots requested on the last backlog frames have reached the disk.
const PROBE_QUIET_FRAMES: u32 = 45;
/// The longest the probe waits for a backlog and its drain before it gives up and reports.
const PROBE_TIMEOUT_SECONDS: f32 = 120.0;

/// The retire-visibility probe's state: one run's count of whether a retiring cell ever showed in
/// a view. See [`probe_retire_visibility`].
#[derive(Resource)]
struct RetireVisibilityProbe {
    dir: PathBuf,
    elapsed: f32,
    frame: u64,
    frames_probed: u64,
    max_backlog: usize,
    visible_hits: u64,
    hit_frames: Vec<u64>,
    captures: Vec<(u64, String)>,
    backlog_seen: bool,
    quiet_frames: u32,
    finished: bool,
}

impl RetireVisibilityProbe {
    fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            elapsed: 0.0,
            frame: 0,
            frames_probed: 0,
            max_backlog: 0,
            visible_hits: 0,
            hit_frames: Vec::new(),
            captures: Vec::new(),
            backlog_seen: false,
            quiet_frames: 0,
            finished: false,
        }
    }

    fn png_name(frame: u64) -> String {
        format!("retire-frame-{frame:06}.png")
    }

    /// The run's record, hand-written (no JSON dependency) and small: the numbers the proof
    /// rests on, and the names of the PNGs beside it.
    fn json(&self) -> String {
        let hit_frames = self
            .hit_frames
            .iter()
            .map(|frame| frame.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let pngs = self
            .captures
            .iter()
            .map(|(_, name)| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{{\n  \"frames_probed\": {},\n  \"max_backlog\": {},\n  \
             \"visible_retiring_hits\": {},\n  \"hit_frames\": [{hit_frames}],\n  \
             \"pngs\": [{pngs}]\n}}\n",
            self.frames_probed, self.max_backlog, self.visible_hits,
        )
    }
}

/// Installs the retire-visibility probe when `--probe-retire-visibility` names a directory:
/// nothing at all - no resource, no system - otherwise, so an ordinary run is exactly as before.
fn install_retire_visibility_probe(app: &mut App, dir: Option<PathBuf>) {
    let Some(dir) = dir else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&dir) {
        warn!(
            target: "retire_probe",
            "could not create {}: {error}",
            dir.display()
        );
    }
    app.insert_resource(RetireVisibilityProbe::new(dir))
        // PostUpdate, after every pass that sets `ViewVisibility` (the camera's and the light
        // passes'): read in Update it is the previous frame's value, so a cell retired and hidden
        // this frame still read as shown on its first backlog frame (15 false hits, u1-check).
        .add_systems(
            PostUpdate,
            probe_retire_visibility
                .after(bevy::camera::visibility::VisibilitySystems::CheckVisibility)
                .after(bevy::light::SimulationLightSystems::CheckLightVisibility),
        );
}

/// `--probe-retire-visibility`: on each frame after the drain, counts the roots still retiring
/// (cells and LOD chunks - hidden, waiting for the budget) that show in a view, photographs the
/// first backlog frames, and once the backlog has drained writes `retire-visibility.json` and
/// exits - non-zero on any hit, or when no backlog ever appeared, since a run that retired
/// nothing proved nothing.
///
/// "Shows" is [`ViewVisibility::get`] on a retiring root or on a descendant that carries a
/// [`Mesh3d`], the same signal the renderer culls on: a cell retired by distance is
/// [`crate::portal`]'s [`CellRole::Hidden`] the frame it retires, and a chunk root is given
/// [`Visibility::Hidden`] by its own retire, so a root that kept a layer or its visibility for
/// the drain would read true here, which is the failure this probe exists to catch. Measurement
/// only: it rewrites no root that is retiring.
///
/// [`CellRole::Hidden`]: crate::portal::CellRole
#[allow(clippy::too_many_arguments)]
fn probe_retire_visibility(
    mut commands: Commands,
    streaming: Res<StreamingWorld>,
    lod: Option<Res<lod::LodStreaming>>,
    metrics: Res<StreamingMetrics>,
    mut probe: ResMut<RetireVisibilityProbe>,
    views: Query<&ViewVisibility>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
) {
    if probe.finished {
        return;
    }
    probe.elapsed += time.delta_secs();
    probe.frame += 1;
    let frame = probe.frame;

    let mut roots: Vec<Entity> = streaming
        .cells
        .values()
        .filter_map(|status| match status {
            CellStatus::Retiring { root, .. } => Some(*root),
            _ => None,
        })
        .collect();
    // The retiring LOD chunk roots are probed the same way: they are hidden the frame they retire
    // and drained by the same budget, so the same question - does one show while it drains - has
    // the same answer to check. `Option` because the probe's own test apps carry no LOD plan.
    roots.extend(
        lod.as_deref()
            .into_iter()
            .flat_map(|lod| lod.retiring_chunks())
            .map(|(_, root)| root),
    );
    // The backlog the drain held this frame: the surviving retirees plus the ones this frame's
    // budget unloaded, which is the count `despawn_cells` paced against.
    let backlog =
        roots.len() + metrics.despawns_this_frame as usize + metrics.lod_despawns_this_frame;

    let mut visible = 0u64;
    for root in &roots {
        let mut stack = vec![*root];
        while let Some(entity) = stack.pop() {
            // The root answers for itself; below it only the entities that carry a mesh are
            // something a view draws, so only those are counted.
            if (entity == *root || meshes.contains(entity))
                && views.get(entity).is_ok_and(|view| view.get())
            {
                visible += 1;
            }
            if let Ok(children) = children.get(entity) {
                stack.extend(children.iter());
            }
        }
    }

    if backlog > 0 {
        probe.backlog_seen = true;
        probe.quiet_frames = 0;
        probe.frames_probed += 1;
        probe.max_backlog = probe.max_backlog.max(backlog);

        // Photograph the first few backlog frames, and every frame a hit shows on (capped), so the
        // frames behind the JSON are on disk beside it.
        if probe.captures.len() < PROBE_MAX_CAPTURES
            && (probe.frames_probed as usize <= PROBE_FIRST_BACKLOG_CAPTURES || visible > 0)
        {
            let name = RetireVisibilityProbe::png_name(frame);
            crate::shots::capture_primary_window(&mut commands, probe.dir.join(&name));
            probe.captures.push((frame, name));
        }

        if visible > 0 {
            probe.visible_hits += visible;
            probe.hit_frames.push(frame);
            warn!(
                target: "retire_probe",
                frame,
                backlog,
                visible,
                "a retiring cell shows in a view while it drains"
            );
        }
    } else if probe.backlog_seen {
        probe.quiet_frames += 1;
    }

    let captures_ready = probe
        .captures
        .iter()
        .all(|(_, name)| probe.dir.join(name).is_file());
    let settled = probe.backlog_seen && probe.quiet_frames >= PROBE_QUIET_FRAMES && captures_ready;
    if !(settled || probe.elapsed >= PROBE_TIMEOUT_SECONDS) {
        return;
    }

    probe.finished = true;
    let path = probe.dir.join("retire-visibility.json");
    let write_failed = match std::fs::write(&path, probe.json()) {
        Ok(()) => {
            info!(
                target: "retire_probe",
                "retire-visibility probe wrote {}: {} backlog frames, max backlog {}, \
                 {} visible retiring hits",
                path.display(),
                probe.frames_probed,
                probe.max_backlog,
                probe.visible_hits
            );
            false
        }
        Err(error) => {
            error!(
                target: "retire_probe",
                "could not write {}: {error}",
                path.display()
            );
            true
        }
    };
    // Fail on a hit, on a JSON that would not write, and on a run that saw no backlog at all -
    // the last has proven nothing about the drain, so it must not read as a pass.
    let failed = write_failed || probe.visible_hits > 0 || !probe.backlog_seen;
    exit.write(if failed {
        AppExit::error()
    } else {
        AppExit::Success
    });
}

///
/// They travel as one - together with the frame's shared commit budget, which main's commit
/// pacing spends from - because a system function takes at most sixteen parameters, and
/// [`collect_cells`] - the commit path - is at that limit.
#[derive(SystemParam)]
struct CommitCatalogs<'w, 's> {
    assets: Res<'w, AssetCatalog>,
    budget: ResMut<'w, StreamingCommitBudget>,
    far: Option<ResMut<'w, far_stock::FarStock>>,
    reference_entities: Query<'w, 's, (), With<FormId>>,
    cluster_terrain: Option<Res<'w, crate::cluster::terrain::ClusterTerrain>>,
}

/// Hands the root a detail change replaces over to the stand-in hold (design-1091 Â§3): the cell
/// changed detail on the same grid, and until the root that replaced it can be drawn this one keeps
/// covering the ground, so no frame shows neither.
///
/// The root stops being a streamed cell root here: the markers every root scan keys on go with it,
/// so [`validate_streaming_lifecycle`]'s duplicate/orphaned/missing counts, the portal's isolation,
/// room culling and the shots' settle see the cell's one root - the new one - and never count this
/// one as a second. The cell's `CellRef` stays for the log, and its `ExteriorCellGrid` so the
/// render origin still places the stand-in ([`reposition_cell_roots`]).
///
/// The new root's terrain patches name it back ([`SwapSurface::stand_in`], through [`spawn_cell`]),
/// which is what `crate::swap_bridge`'s detail-change hand-over reads to decide when the hold ends.
fn hold_replaced_root(commands: &mut Commands, replaced: Entity) {
    commands.queue(move |world: &mut World| {
        if !far_stock::removal_blocked(world, replaced)
            && let Ok(mut entity) = world.get_entity_mut(replaced)
        {
            entity.remove::<(StreamedCellRoot, DistantTerrainRoot)>();
            entity.insert(SwapStandIn::default());
        }
    });
}

/// Collect database responses, reject stale generations and commit cells within the shared budget.
#[allow(clippy::too_many_arguments)]
fn collect_cells(
    mut commands: Commands,
    config: Res<EngineConfig>,
    origin: Res<RenderOrigin>,
    database: Res<WorldDatabase>,
    cache: Res<CellCache>,
    asset_server: Res<AssetServer>,
    mut catalogs: CommitCatalogs,
    reflection: Res<WaterReflectionTexture>,
    mut assets: CellAssets,
    mut streaming: ResMut<StreamingWorld>,
    mut continuity: ResMut<TerrainContinuity>,
    (mut metrics, mut runtime): (
        ResMut<StreamingMetrics>,
        Option<ResMut<runtime::StreamingRuntime>>,
    ),
    mut profiler: ResMut<ProfilingState>,
) {
    // Merged terrain decides cell by cell between its own mesh and the quadrant path, so an
    // exterior cell with terrain is not committed until the arrays it depends on are ready or
    // refused (the setup is bounded by `terrain_merge::SETUP_TIMEOUT`). Interiors and cells with
    // no LAND never wait.
    let holding = assets
        .merge
        .as_deref()
        .is_some_and(TerrainMerge::holds_cells);
    let config = &*config;
    let catalog = &catalogs.assets;
    let world_physics = builds_world_colliders(config);
    let frame_commit_started = Instant::now();
    let mut commits_started = 0;
    let mut full_commits = 0;
    let mut terrain_commits = 0;
    catalogs.budget.frame_started = frame_commit_started;
    release_stale_held_responses(
        &mut streaming,
        &mut metrics,
        &mut profiler,
        runtime.as_deref_mut(),
    );
    let response_scan_limit = config
        .max_cell_commits_per_frame
        .saturating_mul(8)
        .saturating_add(MAX_TERRAIN_CELL_COMMITS_PER_FRAME)
        .max(8);
    // Retry each previously deferred response once. Responses refused during
    // this scan wait until the next frame, leaving room for other arrivals.
    let mut deferred_remaining = runtime
        .as_ref()
        .map_or(0, |runtime| runtime.deferred_responses.len());
    for _ in 0..response_scan_limit {
        if catalogs.budget.elapsed_limit_reached(runtime.as_deref()) {
            break;
        }
        let mut take_deferred = || {
            if deferred_remaining == 0 {
                return None;
            }
            deferred_remaining -= 1;
            runtime
                .as_mut()
                .and_then(|runtime| runtime.deferred_responses.pop_front())
                .map(|response| (response, true))
        };
        // Held responses were counted when they first arrived, and replay ahead of new
        // responses once the merged path is no longer holding cells.
        let replayed = if holding {
            None
        } else {
            streaming.held_responses.pop_front()
        };
        let response = if let Some(response) = replayed {
            Some((response, true))
        } else if catalogs.budget.prefer_deferred_response {
            take_deferred().or_else(|| database.try_response().map(|response| (response, false)))
        } else {
            database
                .try_response()
                .map(|response| (response, false))
                .or_else(take_deferred)
        };
        let Some((response, was_deferred)) = response else {
            break;
        };
        // Persist the opposite source preference even when this frame can
        // commit only one cell. Empty sources fall back to the other queue.
        catalogs.budget.prefer_deferred_response = !was_deferred;
        if !was_deferred {
            metrics.responses_received += 1;
            metrics.total_query_micros = metrics
                .total_query_micros
                .saturating_add(response.query_micros);
            metrics.max_query_micros = metrics.max_query_micros.max(response.query_micros);
            metrics.total_queue_wait_micros = metrics
                .total_queue_wait_micros
                .saturating_add(response.queue_wait_micros);
            metrics.max_queue_wait_micros = metrics
                .max_queue_wait_micros
                .max(response.queue_wait_micros);
            metrics.total_request_micros = metrics
                .total_request_micros
                .saturating_add(response.total_request_micros);
            metrics.max_request_micros = metrics
                .max_request_micros
                .max(response.total_request_micros);
            metrics.total_rows_loaded = metrics
                .total_rows_loaded
                .saturating_add(response.row_count as u64);
            profiler.record_completed_latency_micros(
                "streaming/db_queue_wait",
                response.queue_wait_micros,
            );
            profiler.record_completed_latency_micros("streaming/db_query", response.query_micros);
            profiler.record_completed_latency_micros(
                "streaming/db_request_total",
                response.total_request_micros,
            );
            // A response for a load no longer wanted is stale; it must not wait for setup.
            if !streaming.awaits(response.key, response.generation, response.detail) {
                streaming.consume_cell_response(response.key, response.generation, response.detail);
                if let Some(runtime) = runtime.as_mut() {
                    runtime.retire_cell_attempt(response.key, response.generation, response.detail);
                }
                metrics.stale_responses += 1;
                profiler.event(format!("{:?}", response.key), "stale_discarded", None);
                continue;
            }
            if holding
                && let Some(merge) = assets.merge.as_deref_mut()
                && should_hold_response(merge, &response.result, |id| cache.has_terrain(id))
            {
                profiler.increment("terrain/cells_held", 1);
                streaming.held_responses.push_back(response);
                continue;
            }
        }
        let Some(CellStatus::Loading {
            generation,
            detail,
            replaced,
        }) = streaming.cells.get(&response.key)
        else {
            streaming.consume_cell_response(response.key, response.generation, response.detail);
            if let Some(runtime) = runtime.as_mut() {
                runtime.retire_cell_attempt(response.key, response.generation, response.detail);
            }
            metrics.stale_responses += 1;
            profiler.event(format!("{:?}", response.key), "stale_discarded", None);
            continue;
        };
        if *generation != response.generation || *detail != response.detail {
            streaming.consume_cell_response(response.key, response.generation, response.detail);
            if let Some(runtime) = runtime.as_mut() {
                runtime.retire_cell_attempt(response.key, response.generation, response.detail);
            }
            metrics.stale_responses += 1;
            profiler.event(format!("{:?}", response.key), "stale_generation", None);
            continue;
        }
        let (detail, replaced) = (*detail, *replaced);
        let no_full_slot = detail == CellDetail::Full && catalogs.budget.remaining_for_cells() == 0;
        if no_full_slot
            || commit_waits_for_next_frame(
                detail,
                CommitsThisFrame {
                    started: commits_started,
                    full: full_commits,
                    terrain: terrain_commits,
                    elapsed_micros: elapsed_micros(frame_commit_started),
                },
                config,
            )
        {
            // Rotate deferred full responses so terrain and unrelated reservations can progress.
            if let Some(runtime) = runtime.as_mut() {
                runtime.deferred_responses.push_back(response);
            } else {
                streaming.held_responses.push_back(response);
            }
            continue;
        }
        if let Some(runtime) = runtime.as_mut()
            && let Ok(payload) = &response.result
        {
            let terrain = cache.terrain(payload.cell_id);
            let paths = surface_costs::surface_resource_paths(terrain.as_ref(), true, catalog);
            if !runtime.reserve_cell_attempt_with_textures(
                response.key,
                response.generation,
                detail,
                world_physics,
                paths,
            ) {
                runtime.deferred_responses.push_back(response);
                continue;
            }
        }
        streaming.consume_cell_response(response.key, response.generation, response.detail);
        if replaced.is_some() {
            continuity.edges.remove(&response.key);
        }
        commits_started += 1;
        // A full cell takes one of the frame's full-cell slots, which far stock also commits
        // from; distant LOD requests have their own ([`EngineConfig::max_lod_requests_per_frame`]). A terrain-only
        // ring cell is a fraction of a full commit's work and
        // keeps its own, larger count ([`MAX_TERRAIN_CELL_COMMITS_PER_FRAME`]) and the frame's time
        // budget instead, so the ring still fills at the pace it was tuned to. Every commit is
        // counted in the frame's report ([`finish_streaming_commit_budget`]).
        if detail == CellDetail::Full {
            catalogs.budget.remaining -= 1;
        }
        catalogs.budget.commits = catalogs.budget.commits.saturating_add(1);
        let commit_started = std::time::Instant::now();
        match response.result {
            Ok(mut payload) => {
                let mut terrain = cache.terrain(payload.cell_id);
                if let (Some(terrain), Some(cluster)) =
                    (terrain.as_mut(), catalogs.cluster_terrain.as_deref())
                    && let CellKey::Exterior {
                        worldspace_id,
                        grid_x,
                        grid_y,
                    } = payload.key
                    && let Some(heights) =
                        cluster.canonical_heights(worldspace_id, IVec2::new(grid_x, grid_y))
                {
                    // The cluster adapter and near mesh/collider must use the same canonical edge heights.
                    let changed: Vec<_> = terrain
                        .heights
                        .iter()
                        .zip(heights)
                        .enumerate()
                        .filter_map(|(index, (old, new))| {
                            ((old - new).abs() > EDGE_MATCH_TOLERANCE).then_some(index)
                        })
                        .collect();
                    terrain.heights = heights.to_vec();
                    recompute_packed_normals(terrain, &changed);
                }
                let mut merged = terrain.as_ref().and_then(|_| {
                    choose_merged_terrain(
                        assets.merge.as_deref_mut(),
                        payload.key,
                        payload.cell_id,
                        &mut metrics,
                        &mut profiler,
                    )
                });
                if let Some(terrain) = terrain.as_mut() {
                    let canonical =
                        catalogs
                            .cluster_terrain
                            .as_deref()
                            .and_then(|cluster| match payload.key {
                                CellKey::Exterior {
                                    worldspace_id,
                                    grid_x,
                                    grid_y,
                                } => cluster
                                    .canonical_heights(worldspace_id, IVec2::new(grid_x, grid_y)),
                                CellKey::Interior(_) => None,
                            });
                    let validation = settle_cell_terrain_canonical(
                        payload.key,
                        payload.cell_id,
                        merged.take(),
                        terrain,
                        &catalogs.assets,
                        &mut continuity,
                        &mut metrics,
                        &mut profiler,
                        canonical,
                    )
                    .map(|settled| merged = settled);
                    if let Err(reason) = validation {
                        if detail == CellDetail::Terrain {
                            // A ring cell whose landscape does not validate either is empty space
                            // for the ring, and counted like one: a few hundred ring cells are
                            // requested at a time, and a bad one must not read as a cell failure
                            // the camera can stand in. If the data really is broken, the full
                            // request that follows the camera into the cell validates the same
                            // landscape again and reports it then.
                            debug!(
                                cell = format_args!("{:08X}", payload.cell_id),
                                %reason,
                                "terrain-only cell's landscape failed validation"
                            );
                            metrics.ring_cells_empty = metrics.ring_cells_empty.saturating_add(1);
                            profiler.increment("streaming/ring_cells_invalid_terrain", 1);
                            if config.clusters.is_some()
                                && let Some(root) = replaced
                            {
                                far_stock::queue_release(&mut commands, root);
                            }
                            fail_cell(
                                &mut commands,
                                &mut streaming,
                                response.key,
                                detail,
                                reason,
                                replaced,
                            );
                            if let Some(runtime) = runtime.as_mut() {
                                runtime.retire_cell_attempt(
                                    response.key,
                                    response.generation,
                                    detail,
                                );
                            }
                            continue;
                        }
                        metrics.terrain_validation_failures =
                            metrics.terrain_validation_failures.saturating_add(1);
                        metrics.asset_failures.push(AssetFailure {
                            model_path: format!("terrain/{:08X}", payload.cell_id),
                            reference_form_id: 0,
                            base_form_id: 0,
                            cell_id: payload.cell_id,
                            dependency_chain: vec![reason.clone()],
                        });
                        profiler.increment("terrain/validation_failures", 1);
                        // The data itself is invalid: a retry re-reads the same rows from the same
                        // read-only database (and the same fixed neighbour grids, for an edge
                        // rejection), so it can only fail the same way. Not retried.
                        let attempt = next_full_attempt(&streaming, &response.key);
                        record_failed_cell(
                            &mut streaming,
                            response.key,
                            Some(payload.cell_id),
                            detail,
                            attempt,
                            reason,
                            true,
                            replaced,
                            &mut metrics,
                        );
                        if let Some(runtime) = runtime.as_mut() {
                            runtime.retire_cell_attempt(response.key, response.generation, detail);
                        }
                        continue;
                    }
                }
                if detail == CellDetail::Terrain && terrain.is_none() {
                    // A terrain-only cell with no landscape: the grid has a cell but the cell cache
                    // holds no terrain for it (a persistent cell, or terrain outside the cache).
                    // There is nothing to draw in it and nothing to enter, so it is not held: an
                    // empty root would count against the ring's budget and draw nothing. This is
                    // not a terrain validation failure - the cell has no terrain to be wrong.
                    debug!(
                        cell = format_args!("{:08X}", payload.cell_id),
                        "terrain-only cell has no landscape in the cell cache"
                    );
                    metrics.ring_cells_empty = metrics.ring_cells_empty.saturating_add(1);
                    profiler.increment("streaming/ring_cells_without_terrain", 1);
                    if config.clusters.is_some()
                        && let Some(root) = replaced
                    {
                        far_stock::queue_release(&mut commands, root);
                    }
                    fail_cell(
                        &mut commands,
                        &mut streaming,
                        response.key,
                        detail,
                        "terrain-only cell has no landscape in the cell cache".to_owned(),
                        replaced,
                    );
                    if let Some(runtime) = runtime.as_mut() {
                        runtime.retire_cell_attempt(response.key, response.generation, detail);
                    }
                    continue;
                }
                // The replacement is committed: the root the cell kept drawing does not go now. It
                // becomes the new root's stand-in (at the end of this arm) and stays drawn until
                // the new root's terrain is confirmed drawn, so no frame shows neither. See
                // [`hold_replaced_root`].
                // A full exterior cell answered without terrain is committed as before - no
                // quadrants, no water - but no longer silently: counted, logged, and marked so the
                // lifecycle check can tell it from a cell that lost its landscape.
                let terrain_less = is_terrain_less_full_exterior(payload.key, detail, &terrain);
                if terrain_less {
                    debug!(
                        cell = format_args!("{:08X}", payload.cell_id),
                        key = ?payload.key,
                        "full exterior cell has no terrain in its answer"
                    );
                    metrics.full_cells_without_terrain =
                        metrics.full_cells_without_terrain.saturating_add(1);
                    profiler.increment("streaming/full_cells_without_terrain", 1);
                }
                let adoptions = if detail == CellDetail::Full {
                    catalogs.far.as_deref_mut().map(|far| {
                        far.take_adoptions(payload.key, &mut payload.references, |entity| {
                            catalogs.reference_entities.contains(entity)
                        })
                    })
                } else {
                    None
                };
                let committed_key = payload.key;
                let cell_id = payload.cell_id;
                let drawn_merged = merged.is_some();
                let root = spawn_cell_with_options(
                    &mut commands,
                    &asset_server,
                    &catalogs.assets,
                    &reflection,
                    &mut assets.meshes,
                    &mut assets.terrain_materials,
                    &mut assets.water_materials,
                    origin.0,
                    config.lights,
                    requests::enabled(config),
                    payload,
                    terrain,
                    merged,
                    detail,
                    world_physics,
                    None,
                    replaced,
                    &mut streaming.next_model_sequence,
                    &mut profiler,
                );
                let root = match root {
                    Ok(root) => root,
                    Err(reason) => {
                        error!(cell = format_args!("{cell_id:08X}"), %reason, "terrain collider build failed");
                        metrics.terrain_validation_failures =
                            metrics.terrain_validation_failures.saturating_add(1);
                        let attempt = next_full_attempt(&streaming, &response.key);
                        record_failed_cell(
                            &mut streaming,
                            response.key,
                            Some(cell_id),
                            detail,
                            attempt,
                            reason,
                            true,
                            replaced,
                            &mut metrics,
                        );
                        if let Some(runtime) = runtime.as_mut() {
                            runtime.retire_cell_attempt(response.key, response.generation, detail);
                        }
                        continue;
                    }
                };
                if drawn_merged {
                    metrics.terrain_cells_merged = metrics.terrain_cells_merged.saturating_add(1);
                    profiler.increment("terrain/cells_merged", 1);
                }
                if let Some(runtime) = runtime.as_mut() {
                    runtime.bind_cell_attempt(response.key, response.generation, detail, root);
                }
                if terrain_less {
                    commands.entity(root).insert(TerrainLess);
                }
                if let Some(adoptions) = adoptions {
                    far_stock::queue_adopt(&mut commands, root, committed_key, adoptions);
                }
                streaming
                    .cells
                    .insert(response.key, CellStatus::Resident { root, detail });
                // The root this cell was drawn by until now keeps covering it, and the new root's
                // patches name it back (`spawn_cell`), so it is despawned the frame the new terrain
                // is confirmed drawn - in the same frame the two are drawn together, never in
                // neither - or, loudly and counted, when the hold bound passes.
                if let Some(replaced) = replaced {
                    if config.clusters.is_some() {
                        far_stock::queue_release(&mut commands, replaced);
                    }
                    hold_replaced_root(&mut commands, replaced);
                }
                // A full commit of a previously failed cell ends its failure record: the rescue is
                // over, the plan asks for the cell in full again the way it asks for any resident,
                // and a later failure starts its attempt budget from this success.
                if detail == CellDetail::Full {
                    streaming.full_failed.remove(&response.key);
                    streaming.failed_retries.remove(&response.key);
                }
            }
            Err(error) => {
                // A terrain-only request that fails is empty space, not a failure: the ring covers
                // hundreds of grids around the camera, and the corners of a worldspace beyond its
                // cells, or a grid whose cell the cache has no landscape for, are what most of the
                // answers are (the transition layer's own fixture has 624 of them). Counting them
                // as cell failures would drown the number that matters - a full cell the camera
                // stands in failing to load - so they are counted on their own.
                if detail == CellDetail::Terrain {
                    debug!(?response.key, %error, "the ring holds nothing at this grid");
                    metrics.ring_cells_empty = metrics.ring_cells_empty.saturating_add(1);
                    profiler.increment("streaming/ring_cells_without_a_cell", 1);
                    if config.clusters.is_some()
                        && let Some(root) = replaced
                    {
                        far_stock::queue_release(&mut commands, root);
                    }
                    fail_cell(
                        &mut commands,
                        &mut streaming,
                        response.key,
                        detail,
                        error,
                        replaced,
                    );
                } else {
                    // A database or IO error: the read failed, not the data, so the cell retries
                    // once its backoff passes ([`record_failed_cell`]).
                    profiler.increment("streaming/failed_cells", 1);
                    let attempt = next_full_attempt(&streaming, &response.key);
                    record_failed_cell(
                        &mut streaming,
                        response.key,
                        None,
                        detail,
                        attempt,
                        error,
                        false,
                        replaced,
                        &mut metrics,
                    );
                }
            }
        }
        if let Some(runtime) = runtime.as_mut() {
            runtime.retire_cell_attempt(response.key, response.generation, detail);
        }
        let commit_micros = commit_started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        metrics.max_commit_micros = metrics.max_commit_micros.max(commit_micros);
        match detail {
            CellDetail::Full => full_commits += 1,
            CellDetail::Terrain => terrain_commits += 1,
        }
        profiler.record_micros("streaming/cell_commit", commit_micros);
        profiler.event(
            format!("{:?}", response.key),
            "committed",
            Some(commit_micros as f64 / 1000.0),
        );
    }
}

fn finish_streaming_commit_budget(
    config: Res<EngineConfig>,
    budget: Res<StreamingCommitBudget>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
    mut runtime: Option<ResMut<runtime::StreamingRuntime>>,
) {
    if let Some(runtime) = runtime.as_mut() {
        runtime.previous_commit_ms = budget.frame_started.elapsed().as_secs_f64() * 1000.0;
    }
    if budget.commits == 0 {
        return;
    }
    let frame_micros = budget
        .frame_started
        .elapsed()
        .as_micros()
        .min(u128::from(u64::MAX)) as u64;
    metrics.commit_frames = metrics.commit_frames.saturating_add(1);
    metrics.total_frame_commit_micros = metrics
        .total_frame_commit_micros
        .saturating_add(frame_micros);
    metrics.max_frame_commit_micros = metrics.max_frame_commit_micros.max(frame_micros);
    metrics.commit_budget_micros = config.max_commit_micros_per_frame;
    if commit_budget_exceeded(frame_micros, metrics.commit_budget_micros) {
        metrics.commit_budget_violations = metrics.commit_budget_violations.saturating_add(1);
        profiler.event(
            "streaming",
            "commit_budget_exceeded",
            Some(frame_micros as f64 / 1_000.0),
        );
    }
    profiler.set_gauge("streaming/commits_this_frame", budget.commits as f64);
    profiler.record_micros("streaming/frame_commit", frame_micros);
}

/// Marks a terrain-only request failed, dropping the root it kept drawn while it loaded.
///
/// Terrain-detail failures are the ring's empty space - a grid the worldspace does not reach, a
/// cell the cache has no landscape for, a landscape that does not validate - and are counted where
/// the caller counts them, never here. A failed replacement tears the cell's old root down with
/// it: what is drawn would no longer be what the cell claims to be. The failure itself is not
/// retried and carries no retry bookkeeping: the full request that follows the camera into the
/// cell asks a different question, and that is where a real failure is reported
/// ([`record_failed_cell`]).
fn fail_cell(
    commands: &mut Commands,
    streaming: &mut StreamingWorld,
    key: CellKey,
    detail: CellDetail,
    reason: String,
    replaced: Option<Entity>,
) {
    debug_assert!(
        detail == CellDetail::Terrain,
        "full-detail failures go through record_failed_cell"
    );
    if let Some(replaced) = replaced {
        far_stock::queue_despawn(commands, replaced);
    }
    streaming.cells.insert(
        key,
        CellStatus::Failed {
            detail,
            attempts: 1,
            reason,
            retry_at: None,
            replaced: None,
        },
    );
}

/// Which attempt the next full-detail failure of `key` would be: one more than the failures the
/// failure records already hold. An exterior counts the attempts its [`StreamingWorld::full_failed`]
/// entry carries; an interior counts the retries its `failed_retries` budget has submitted, which
/// is the same count one off ([`MAX_CELL_LOAD_RETRIES`]).
fn next_full_attempt(streaming: &StreamingWorld, key: &CellKey) -> u32 {
    match key {
        CellKey::Exterior { .. } => streaming
            .full_failed
            .get(key)
            .map_or(1, |failure| failure.attempts + 1),
        CellKey::Interior(_) => streaming.failed_retries.get(key).copied().unwrap_or(0) + 1,
    }
}

/// Records one failed full-detail load of a wanted cell, the state a rejected response leaves it
/// in.
///
/// Every failure is logged once, with the cell key, the cell id (when the response carried one),
/// the attempt number and the reason; nothing is swallowed and no failure is counted as a
/// success. A failure whose cause is the data itself (`deterministic`) cannot succeed on a retry,
/// and neither can one that has spent [`MAX_CELL_LOAD_ATTEMPTS`] loads, so both go straight to
/// the given-up state with one `error!` and are not requested again while the camera stays near.
/// Anything else gets a doubling backoff, after which the plan pass asks for the cell again.
///
/// The `replaced` root - the terrain root a full request was upgrading from - is kept drawn: a
/// failed full load must not open a hole in the ground, and the plan's downgrade-to-terrain or
/// the next full retry takes the root over the way any tier change does.
#[allow(clippy::too_many_arguments)]
fn record_failed_cell(
    streaming: &mut StreamingWorld,
    key: CellKey,
    cell_id: Option<u32>,
    detail: CellDetail,
    attempt: u32,
    reason: String,
    deterministic: bool,
    replaced: Option<Entity>,
    metrics: &mut StreamingMetrics,
) {
    metrics.failed_cells = metrics.failed_cells.saturating_add(1);
    match cell_id {
        Some(cell_id) => warn!(
            cell = format_args!("{cell_id:08X}"),
            ?key,
            attempt,
            %reason,
            "cell load failed"
        ),
        None => warn!(?key, attempt, %reason, "cell load failed"),
    }
    let retry_at = if deterministic || attempt >= MAX_CELL_LOAD_ATTEMPTS {
        match cell_id {
            Some(cell_id) => error!(
                cell = format_args!("{cell_id:08X}"),
                ?key,
                attempts = attempt,
                %reason,
                "cell load given up; not requested again while the camera stays near"
            ),
            None => error!(
                ?key,
                attempts = attempt,
                %reason,
                "cell load given up; not requested again while the camera stays near"
            ),
        }
        metrics.failed_cells_given_up = metrics.failed_cells_given_up.saturating_add(1);
        None
    } else {
        // The wait doubles per failure: 1 s after the first, 2 s after the second.
        Some(Instant::now() + CELL_RETRY_BACKOFF_BASE * (1u32 << (attempt - 1)))
    };
    // An exterior's failed full request is remembered so the plan draws the cell's landscape on
    // its own, and so it knows when - and whether - to ask for the cell in full again. An
    // interior has no landscape to fall back to and is retried through its own budget
    // ([`retry_failed_interior`]).
    if detail == CellDetail::Full && matches!(key, CellKey::Exterior { .. }) {
        streaming.full_failed.insert(
            key,
            FullFailure {
                attempts: attempt,
                reason: reason.clone(),
                retry_at,
            },
        );
    }
    streaming.cells.insert(
        key,
        CellStatus::Failed {
            detail,
            attempts: attempt,
            reason,
            retry_at,
            replaced,
        },
    );
}

/// Whether the plan asks again for `key`, which failed at exactly the tier it is asking for.
///
/// A failed cell is otherwise never asked for again at the same tier - the data does not change,
/// and a ring grid that holds nothing would be re-requested every frame (see [`plan_cells`]) - and
/// an exterior that failed in full is covered instead by [`StreamingWorld::full_failed`], which
/// asks for its landscape on its own. An **interior** has neither: it is one tier, with no
/// landscape to fall back to, and the cell the camera stands in is the whole of what it can see.
/// Left as failed it would stay empty for as long as the camera stands in it, with no retry at
/// all, and a crossing into it is what turns it into the active interior, a door whose destination
/// counts as settled once it has failed (`crate::transition::destination_is_loaded`): the player
/// can walk into that emptiness. So the interior the camera stands in is asked for again, up to
/// [`MAX_CELL_LOAD_RETRIES`] times, the same [`MAX_CELL_LOAD_ATTEMPTS`] budget every cell gets: a
/// bounded number, so a cell the database really does not have costs its budget and is then left
/// alone rather than re-requested every frame.
///
/// Only the interior the camera is *in*: an interior merely pre-streamed behind a nearby door is
/// not waited on forever either way, and the moment the camera crosses into it, it is the active
/// interior and this answers `true` for it.
///
/// A retry also waits for the failed answer's backoff to pass, the same wall-clock wait an
/// exterior's full retry pays: a failure just recorded is not re-asked the frame after it. A cell
/// given up on (`retry_at` of `None`) is not asked for again at all, whatever its budget says.
///
/// `active_interior` is [`ActiveCell::interior`]: the plan passes the one it is planning for, and
/// [`StreamingWorld::has_failed`] the one the plan last saw, so the two answer the same question
/// the same way.
fn retry_failed_interior(
    key: &CellKey,
    active_interior: Option<u32>,
    streaming: &StreamingWorld,
) -> bool {
    let CellKey::Interior(cell_id) = *key else {
        return false;
    };
    if active_interior != Some(cell_id) {
        return false;
    }
    let Some(CellStatus::Failed {
        detail: CellDetail::Full,
        retry_at: Some(retry_at),
        ..
    }) = streaming.cells.get(key)
    else {
        return false;
    };
    if Instant::now() < *retry_at {
        return false;
    }
    streaming.failed_retries.get(key).copied().unwrap_or(0) < MAX_CELL_LOAD_RETRIES
}

/// The cells the active cell wants resident, and what it wants of each: the interior the camera is
/// in, or the exterior grid of `stream_radius` around it in full and the terrain-only ring out to
/// `terrain_radius` beyond â€” plus everything the transition layer pre-streamed for a nearby load
/// door, which is what makes a crossing seamless.
///
/// Ordered for the worker's queue, which takes them in the order they are asked for:
///
/// 1. the cells a nearby door pre-streamed, wherever they are - the space behind a door the camera
///    may cross into at any moment, which is worth more than any view of the landscape;
/// 2. the interior the camera is inside, if it is in one - the cell it stands in;
/// 3. the exterior grid, nearest first and full before terrain-only, so the world appears from the
///    camera outwards.
fn wanted_cells(
    active: &ActiveSpace,
    configured_worldspace: u32,
    stream_radius: i32,
    terrain_radius: i32,
    center: IVec2,
) -> Vec<(CellKey, CellDetail)> {
    let mut wanted = Vec::new();
    if let Some(cell_id) = active.interior {
        // An interior is the whole plan: it is one cell, and no exterior is streamed while the
        // camera is inside one.
        wanted.push((CellKey::Interior(cell_id), CellDetail::Full));
    } else {
        let radius = terrain_radius.max(stream_radius);
        let mut grid = Vec::new();
        for y in -radius..=radius {
            for x in -radius..=radius {
                let key = CellKey::Exterior {
                    worldspace_id: active.exterior_worldspace(configured_worldspace),
                    grid_x: center.x + x,
                    grid_y: center.y + y,
                };
                if let Some(detail) = wanted_detail(
                    active,
                    configured_worldspace,
                    stream_radius,
                    terrain_radius,
                    center,
                    key,
                ) {
                    grid.push((x.abs().max(y.abs()), key, detail));
                }
            }
        }
        grid.sort_by_key(|(distance, _, detail)| {
            (matches!(detail, CellDetail::Terrain), *distance)
        });
        wanted.extend(grid.into_iter().map(|(_, key, detail)| (key, detail)));
    }
    wanted
}

/// What the plan wants done with one cell: nothing, its terrain, or the whole cell.
///
/// A cell is full inside the inner grid, terrain-only in the ring around it, and not wanted beyond
/// the ring â€” except that a pre-streamed cell is always full, however far away it is: it is the
/// space behind a door the camera is about to cross into, and an interior is never terrain-only.
fn wanted_detail(
    active: &ActiveSpace,
    configured_worldspace: u32,
    stream_radius: i32,
    terrain_radius: i32,
    center: IVec2,
    key: CellKey,
) -> Option<CellDetail> {
    match (key, active.interior) {
        (CellKey::Interior(cell_id), Some(interior)) if cell_id == interior => {
            Some(CellDetail::Full)
        }
        (CellKey::Interior(_), _) => None,
        (CellKey::Exterior { .. }, Some(_)) => None,
        (
            CellKey::Exterior {
                worldspace_id,
                grid_x,
                grid_y,
            },
            None,
        ) => {
            if worldspace_id != active.exterior_worldspace(configured_worldspace) {
                return None;
            }
            let distance = (grid_x - center.x).abs().max((grid_y - center.y).abs());
            if distance <= stream_radius {
                Some(CellDetail::Full)
            } else if distance <= terrain_radius {
                Some(CellDetail::Terrain)
            } else {
                None
            }
        }
    }
}

/// Whether a streamed cell survives this frame: a cell the transition layer pre-streamed, the
/// active interior, or an exterior of the active worldspace within the outer of the two radii.
///
/// `radius` is the full-detail band's, which is what it was before the ring existed;
/// `terrain_radius + 1` is the ring's, one cell further out than the ring itself so that its edge
/// does not pop in and out as the camera crosses a cell boundary. The wider of the two decides:
/// beyond `radius` inside the ring only a terrain-only cell can exist, because the plan demotes a
/// full cell the moment it leaves the inner grid, so the extra cell this keeps is a ring cell.
///
/// So an interior unloads as soon as it is neither active nor pre-streamed, no exterior survives
/// while an interior is active, and a crossing into another worldspace drops the one just left
/// instead of holding its cells at whatever grid the destination happens to share.
fn cell_within_unload_radius(
    key: CellKey,
    active: &ActiveSpace,
    configured_worldspace: u32,
    center: IVec2,
    radius: i32,
    terrain_radius: i32,
) -> bool {
    match (key, active.interior) {
        (CellKey::Interior(cell_id), Some(active_interior)) => cell_id == active_interior,
        (CellKey::Interior(_), None) => false,
        (CellKey::Exterior { .. }, Some(_)) => false,
        (
            CellKey::Exterior {
                worldspace_id,
                grid_x,
                grid_y,
            },
            None,
        ) => {
            let limit = radius.max(terrain_radius.saturating_add(1));
            worldspace_id == active.exterior_worldspace(configured_worldspace)
                && (grid_x - center.x).abs() <= limit
                && (grid_y - center.y).abs() <= limit
        }
    }
}

/// Whether a water plane at `water_height` would cover any of this cell's ground, which is what
/// decides whether the cell gets one. A cell whose own height samples all stay at or above the
/// water level is dry: the plane would sit exactly on top of the terrain where they are equal and
/// within a few units of it where they are close, which is inside the depth precision of a
/// distant camera, so the two surfaces z-fight and the plane's cell-aligned edges read as a grid.
/// A cell with one sample below the level still gets its plane - clipping it to the shoreline is
/// a separate change.
fn terrain_reaches_water(terrain: &TerrainSnapshot, water_height: f32) -> bool {
    terrain.heights.iter().any(|height| *height < water_height)
}

/// Marks the root of a full-detail exterior cell that was committed with no terrain in its answer
/// (counted in [`StreamingMetrics::full_cells_without_terrain`]). The lifecycle check expects every
/// other full exterior root to hold a [`TerrainPatch`], so this is how a counted, known case is told
/// apart from a cell that lost its landscape after it was spawned.
#[derive(Component, Debug)]
pub(crate) struct TerrainLess;

/// Whether this commit is a full-detail exterior cell with no terrain to draw. An interior has no
/// landscape by nature and a terrain-only cell without one is refused before it gets here, so
/// neither is this case.
fn is_terrain_less_full_exterior(
    key: CellKey,
    detail: CellDetail,
    terrain: &Option<TerrainSnapshot>,
) -> bool {
    detail == CellDetail::Full && matches!(key, CellKey::Exterior { .. }) && terrain.is_none()
}

/// Build a cell hierarchy and request its model scenes, recording their IDs for optional tracing.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn spawn_cell(
    commands: &mut Commands,
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    reflection: &WaterReflectionTexture,
    meshes: &mut Assets<Mesh>,
    terrain_materials: &mut Assets<TerrainMaterial>,
    water_materials: &mut Assets<WaterMaterial>,
    origin: IVec2,
    lights: bool,
    terrain_physics: bool,
    schedule_scenes: bool,
    payload: CellPayload,
    terrain: Option<TerrainSnapshot>,
    merged: Option<MergedSpawn>,
    model_sequence: &mut u64,
    profiler: &mut ProfilingState,
) -> Result<Entity, String> {
    spawn_cell_with_options(
        commands,
        asset_server,
        catalog,
        reflection,
        meshes,
        terrain_materials,
        water_materials,
        origin,
        lights,
        schedule_scenes,
        payload,
        terrain,
        merged,
        CellDetail::Full,
        terrain_physics,
        None,
        None,
        model_sequence,
        profiler,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn spawn_demo_cell(
    commands: &mut Commands,
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    reflection: &WaterReflectionTexture,
    meshes: &mut Assets<Mesh>,
    terrain_materials: &mut Assets<TerrainMaterial>,
    water_materials: &mut Assets<WaterMaterial>,
    origin: IVec2,
    payload: CellPayload,
    terrain: Option<TerrainSnapshot>,
    detail: CellDetail,
    // Whether this run builds world colliders ([`builds_world_colliders`]). Only a full cell gets
    // terrain colliders; a ring cell is never walked on.
    world_physics: bool,
    // The layers the cell is born on, from `crate::portal`'s resolver ([`StreamedCellLayerStamps`]):
    // the root carries its role's `Propagate` source and every plain node its `Inherited` and
    // concrete `RenderLayers` in the bundle it is spawned with, so the propagation's first pass
    // over the tree is a comparison and the crossing's later role flips are value writes, never
    // the per-node archetype moves that re-stamping a bare subtree costs (impl-1058). `None`
    // spawns the cell exactly as a run without the portal always did. A node with layers of its
    // own (the water plane) is left to the adoption that makes it a source.
    stamp: Option<RenderLayers>,
    // The root this cell was drawn by until this frame, when the commit is a detail change: every
    // terrain patch is born naming it as its stand-in ([`SwapSurface::stand_in`]), which is what
    // keeps it drawn and what the hand-over reads to know when to despawn it.
    stand_in: Option<Entity>,
    model_sequence: &mut u64,
    profiler: &mut ProfilingState,
) -> Entity {
    spawn_cell_with_options(
        commands,
        asset_server,
        catalog,
        reflection,
        meshes,
        terrain_materials,
        water_materials,
        origin,
        true,
        false,
        payload,
        terrain,
        None,
        detail,
        world_physics,
        stamp,
        stand_in,
        model_sequence,
        profiler,
    )
    .expect("test cell must build")
}

#[allow(clippy::too_many_arguments)]
fn spawn_cell_with_options(
    commands: &mut Commands,
    asset_server: &AssetServer,
    catalog: &AssetCatalog,
    reflection: &WaterReflectionTexture,
    meshes: &mut Assets<Mesh>,
    terrain_materials: &mut Assets<TerrainMaterial>,
    water_materials: &mut Assets<WaterMaterial>,
    origin: IVec2,
    lights: bool,
    schedule_scenes: bool,
    payload: CellPayload,
    terrain: Option<TerrainSnapshot>,
    merged: Option<MergedSpawn>,
    detail: CellDetail,
    // Whether this run builds world colliders ([`builds_world_colliders`]). Only a full cell gets
    // terrain colliders; a ring cell is never walked on.
    world_physics: bool,
    // The layers the cell is born on, from `crate::portal`'s resolver ([`StreamedCellLayerStamps`]):
    // the root carries its role's `Propagate` source and every plain node its `Inherited` and
    // concrete `RenderLayers` in the bundle it is spawned with, so the propagation's first pass
    // over the tree is a comparison and the crossing's later role flips are value writes, never
    // the per-node archetype moves that re-stamping a bare subtree costs (impl-1058). `None`
    // spawns the cell exactly as a run without the portal always did. A node with layers of its
    // own (the water plane) is left to the adoption that makes it a source.
    stamp: Option<RenderLayers>,
    // The root this cell was drawn by until this frame, when the commit is a detail change: every
    // terrain patch is born naming it as its stand-in ([`SwapSurface::stand_in`]), which is what
    // keeps it drawn and what the hand-over reads to know when to despawn it.
    stand_in: Option<Entity>,
    model_sequence: &mut u64,
    profiler: &mut ProfilingState,
) -> Result<Entity, String> {
    let spawn_started = Instant::now();
    let reference_count = payload
        .references
        .iter()
        .filter(|row| row.initially_enabled)
        .count();
    let suppressed_count = payload.references.len() - reference_count;
    let root_translation = cell_translation(payload.key, origin);
    let terrain_physics = world_physics && detail == CellDetail::Full;
    let terrain_started = Instant::now();
    let terrain_grid = match payload.key {
        CellKey::Exterior { grid_x, grid_y, .. } => Some(IVec2::new(grid_x, grid_y)),
        CellKey::Interior(_) => None,
    };
    // The merged cell's one mesh and its one collider, built from the same buffers.
    let merged_cell = merged
        .map(|spawn| {
            let mesh = crate::terrain_merge::mesh::build_merged_mesh(
                &spawn.cell,
                &spawn.classes.ids_by_form_id,
            )?;
            let collider = terrain_physics
                .then(|| terrain_collider_from_mesh(&mesh))
                .transpose()?;
            Ok::<_, String>((mesh, collider, spawn.material, spawn.images))
        })
        .transpose()?;
    let terrain_quadrants = if let (Some(terrain), None) = (&terrain, &merged_cell) {
        (0..4)
            .map(|quadrant| {
                let mesh = build_terrain_quadrant_mesh(terrain, quadrant)?;
                let collider = terrain_physics
                    .then(|| terrain_collider_from_mesh(&mesh))
                    .transpose()?;
                Ok((quadrant, mesh, collider))
            })
            .collect::<Result<Vec<_>, String>>()?
    } else {
        Vec::new()
    };
    // The four quadrant meshes and their colliders, the cell's heaviest CPU work after the
    // references; a cell without terrain records nothing.
    if !terrain_quadrants.is_empty() {
        profiler.record_elapsed("streaming/terrain_mesh", terrain_started);
    }
    let mut root_commands = commands.spawn((
        Name::new(format!("Cell {:08X}", payload.cell_id)),
        CellRef(payload.cell_id),
        StreamedCellKey(payload.key),
        Transform::from_translation(root_translation),
        Visibility::default(),
    ));
    match detail {
        CellDetail::Full => {
            root_commands.insert(StreamedCellRoot);
        }
        CellDetail::Terrain => {
            root_commands.insert(DistantTerrainRoot);
        }
    }
    if let CellKey::Exterior { grid_x, grid_y, .. } = payload.key {
        root_commands.insert(ExteriorCellGrid(IVec2::new(grid_x, grid_y)));
    }
    let root = root_commands.id();
    commands.entity(root).with_children(|parent| {
        if let Some(terrain) = terrain {
            if let Some((mesh, collider, material, images)) = merged_cell {
                // Waiting for the render world from birth, like a quadrant: the grid's LOD tiles
                // stay drawn until the bridge confirms the merged mesh drawn
                // (`lod::update_terrain_lod_visibility`). The cell declares the shared material and
                // every land texture array it binds.
                let surface = SwapSurface::new(material.id().untyped(), images.iter().copied());
                let surface = match stand_in {
                    Some(root) => surface.with_stand_in(root),
                    None => surface,
                };
                let mut patch = parent.spawn((
                    Name::new("Terrain cell"),
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material),
                    Transform::default(),
                    TerrainPatch,
                    crate::render::reflectable_world_layers(),
                    MergedTerrainCell,
                    Visibility::Hidden,
                    PendingMergedTerrain,
                    SwapWaiting,
                    surface,
                ));
                if let Some(grid) = terrain_grid {
                    patch.insert(MergedTerrainCoverage { grid });
                    if let CellKey::Exterior { worldspace_id, .. } = payload.key {
                        patch.insert(TerrainWorldspace(worldspace_id));
                    }
                }
                if let Some(collider) = collider {
                    patch.insert((
                        TerrainCollider,
                        RigidBody::Fixed,
                        collider,
                        crate::physics::world_collision_groups(),
                    ));
                }
            }
            for (quadrant, mesh, collider) in terrain_quadrants {
                let (extension, images) =
                    TerrainExtension::from_quadrant(&terrain, quadrant, catalog, asset_server)
                        .expect("validated terrain material must build");
                let material = terrain_materials.add(TerrainMaterial {
                    base: StandardMaterial {
                        base_color: Color::WHITE,
                        perceptual_roughness: 0.92,
                        cull_mode: None,
                        double_sided: true,
                        ..default()
                    },
                    extension,
                });
                // The quadrant declares what its draw needs, for the renderâ†’main swap bridge
                // (`crate::swap_bridge`): the material it draws with and every image that material
                // binds. The images are the ones the readiness pass validates below, and the
                // declaration includes every authored diffuse and normal dependency.
                let surface = SwapSurface::new(
                    material.id().untyped(),
                    images
                        .color
                        .iter()
                        .chain(images.normal.iter())
                        .map(|handle| handle.id()),
                );
                // A detail change's new quadrants name the root that is covering the cell until
                // they are drawn; the commit keeps that root alive for exactly as long.
                let surface = match stand_in {
                    Some(stand_in) => surface.with_stand_in(stand_in),
                    None => surface,
                };
                let mut patch = parent.spawn((
                    Name::new(format!("Terrain quadrant {quadrant}")),
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(material),
                    Transform::default(),
                    TerrainPatch,
                    crate::render::reflectable_world_layers(),
                    Visibility::Hidden,
                    PendingTerrainProfile {
                        cell_id: terrain.cell_id,
                        quadrant,
                        images: images.color,
                        normals: images.normal,
                    },
                    // Waiting for the render world from birth: the quadrant stays hidden (and its
                    // grid's LOD tile stays drawn) until the bridge confirms it is drawable.
                    SwapWaiting,
                    surface,
                ));
                if let Some(grid) = terrain_grid {
                    patch.insert(TerrainCoverage {
                        grid,
                        quadrant,
                        tier: None,
                    });
                    if let CellKey::Exterior { worldspace_id, .. } = payload.key {
                        patch.insert(TerrainWorldspace(worldspace_id));
                    }
                }
                if let Some(collider) = collider {
                    patch.insert((
                        TerrainCollider,
                        RigidBody::Fixed,
                        collider,
                        crate::physics::world_collision_groups(),
                    ));
                }
                // Born on the cell's layers, like every other plain node of the tree. A run without
                // the portal's stamps puts the patch on the reflectable world layers instead, so the
                // water reflection draws the terrain on either shadow route (`crate::shadow_route`).
                // Not both: layers of the patch's own would make it a layer source of its own under
                // the portal's cell isolation (`crate::portal`'s `adopt_own_layer_node`).
                patch.insert(crate::render::reflectable_world_layers());
                if detail == CellDetail::Terrain {
                    // A ring cell casts no shadow: its own shadows fall on the ring, tens of
                    // thousands of units away and past the last shadow cascade, so nobody can see
                    // them - and every caster is another draw per frame. Keeping the ring out of
                    // the shadow pass measured 30 -> 59 frames a second at a twelve-cell ring with
                    // the whole Pale in view, which is what the default's cell count is chosen
                    // against (`crate::config::DEFAULT_TERRAIN_RADIUS`). Receiving light is
                    // untouched: the ring is shaded by the sun like everything else.
                    patch.insert(bevy::light::NotShadowCaster);
                }
            }

            if let Some(height) = terrain
                .water_height
                .filter(|height| height.is_finite() && height.abs() < 1.0e7)
                // A cell whose own samples stay at or above the water level has no ground for the
                // plane to cover: drawing the full 4096x4096 quad there puts a second surface on
                // top of the terrain, whose cell-aligned edges then show through it as grid lines.
                .filter(|height| terrain_reaches_water(&terrain, *height))
            {
                let water_mesh = meshes.add(Plane3d::default().mesh().size(CELL_SIZE, CELL_SIZE));
                let water_colors = terrain
                    .water_type_form_id
                    .and_then(|form_id| catalog.water_colors(form_id));
                let flow_normal = terrain
                    .water_type_form_id
                    .and_then(|form_id| catalog.water_flow(form_id))
                    .map(|path| {
                        asset_server
                            .load_builder()
                            .with_settings(|settings: &mut ImageLoaderSettings| {
                                settings.is_srgb = false;
                            })
                            .load(path.to_owned())
                    });
                // Skyrim's DefaultWater deep colour after Update.esm, used when this water has no
                // decoded colours yet (a database converted before the WATR colour export). Skyrim
                // thins water to show the bed where it is shallow; without depth fog a 60% cover
                // keeps river beds visible.
                let base_color = water_colors.map_or(Color::srgba_u8(5, 14, 18, 153), |colors| {
                    let [r, g, b] = colors.deep;
                    Color::srgba_u8(r, g, b, 153)
                });
                let (fresnel, reflectivity) = water_colors.map_or(
                    (
                        crate::render::DEFAULT_WATER_FRESNEL,
                        crate::render::DEFAULT_WATER_REFLECTIVITY,
                    ),
                    |colors| (colors.fresnel, colors.reflectivity),
                );
                let water_material = water_materials.add(WaterMaterial {
                    base: StandardMaterial {
                        base_color,
                        metallic: 0.15,
                        perceptual_roughness: 0.06,
                        reflectance: 0.9,
                        alpha_mode: AlphaMode::Blend,
                        ..default()
                    },
                    extension: WaterExtension::with_reflection_and_factors(
                        reflection.0.clone(),
                        flow_normal.clone(),
                        fresnel,
                        reflectivity,
                    )
                    .with_reflection_color(
                        water_colors
                            .map_or(crate::render::DEFAULT_WATER_REFLECTION_COLOR, |colors| {
                                colors.reflection
                            }),
                    ),
                });
                parent.spawn((
                    Name::new("Water"),
                    Mesh3d(water_mesh),
                    MeshMaterial3d(water_material),
                    Transform::from_translation(Vec3::new(
                        CELL_SIZE * 0.5,
                        height,
                        -CELL_SIZE * 0.5,
                    )),
                    WaterSurface,
                    Visibility::Hidden,
                    PendingWaterProfile {
                        cell_id: terrain.cell_id,
                        flow_normal,
                    },
                    bevy::camera::visibility::RenderLayers::layer(1),
                ));
            }
        }
        // A terrain-only cell is landscape seen from a distance and never entered. Its references
        // are not spawned - the database did not even read them - so nothing in it can be drawn,
        // lit, walked through or opened. Its terrain and water plane above are built by the same
        // code the full path uses, so the two tiers meet with the same mesh and the same materials.
        if detail == CellDetail::Full {
            let references_started = Instant::now();
            let mut model_loads = 0u64;
            let mut context = ReferenceSpawnContext {
                key: payload.key,
                model_sequence,
                asset_server,
                profiler,
                world_physics: terrain_physics,
                lights,
                schedule_scenes,
            };
            for reference in payload.references {
                if let Some((_, requested)) = spawn_reference(parent, reference, &mut context) {
                    model_loads += u64::from(requested);
                }
            }
            profiler.record_elapsed("streaming/spawn_references", references_started);
            // Per committed cell: what it carried and what it asked the loaders for; the cell's
            // whole commit time follows in the "committed" event.
            profiler.event(
                format!("{:?}", payload.key),
                format!("spawned references={reference_count} model_loads={model_loads}"),
                Some(references_started.elapsed().as_secs_f64() * 1000.0),
            );
        }
    });
    profiler.increment("streaming/references_spawned", reference_count as u64);
    profiler.increment("streaming/references_suppressed", suppressed_count as u64);
    profiler.record_elapsed("streaming/spawn_cell", spawn_started);
    Ok(root)
}

/// A converted model [`spawn_cell`] found for a reference but has not handed to Bevy yet.
///
/// Bevy instantiates every model it is handed in one unbudgeted pass, so handing it a whole cell's
/// models at once is what makes an arrival frame expensive. [`arm_pending_models`] replaces this
/// component with [`WorldAssetRoot`] once the converted scene is loaded and the budget allows it.
/// A model whose cell is unloaded before its turn needs no cleanup: this is a component on that
/// cell's subtree, not an entry in a queue of its own, so it disappears with the subtree.
#[derive(Component)]
pub(crate) struct PendingModel {
    handle: Handle<WorldAsset>,
    /// Where the reference was in spawn order, so a backlog is armed oldest first.
    sequence: u64,
}

#[cfg(test)]
impl PendingModel {
    pub(crate) fn for_test() -> Self {
        Self {
            handle: Handle::default(),
            sequence: 0,
        }
    }
}

/// Stays on a model, terrain or water entity whose load or validation failed for good. The pending
/// markers are removed when the failure is recorded and the scene is hidden, so without this a
/// resident cell holding a failed model looks finished and fully drawn; readiness reads it to report
/// the destination failed instead of ready.
#[derive(Component)]
pub(crate) struct AssetFailed;

#[derive(Component)]
struct PendingAssetProfile {
    started: Instant,
    scene_spawned: bool,
    path: String,
    form_id: u32,
    base_form_id: u32,
    /// The base record's type (`STAT`, `TREE`, `FURN`, ...), which decides whether this placement
    /// may carry a collider at all.
    base_record_type: Option<String>,
    /// Whether this run builds world colliders ([`builds_world_colliders`]). Carried on the
    /// placement rather than read from the config in the readiness scan, so the scan and the spawn
    /// that queued it cannot disagree.
    static_physics: bool,
    cell_id: u32,
}

#[derive(Component)]
struct PendingTerrainProfile {
    cell_id: u32,
    quadrant: u8,
    images: Vec<Handle<Image>>,
    /// Authored normal maps are required dependencies when declared. Their failure refuses
    /// the surface through the same recorded AssetFailed path as diffuse-map failure.
    normals: Vec<Handle<Image>>,
}

#[derive(Component)]
struct PendingWaterProfile {
    cell_id: u32,
    flow_normal: Option<Handle<Image>>,
}

/// Counts the streamer's unfinished-work markers under one cell root: a model not yet armed or
/// instantiated, terrain or water whose textures are still loading, and a surface the render world
/// has not confirmed drawn. Unlike the global [`StreamingMetrics`] counts these cover only that
/// cell, which is what a load door waits for.
#[derive(SystemParam)]
#[allow(clippy::type_complexity)]
pub(crate) struct PendingUnder<'w, 's> {
    children: Query<'w, 's, &'static Children>,
    pending: Query<
        'w,
        's,
        (),
        Or<(
            With<PendingModel>,
            With<PendingAssetProfile>,
            With<PendingTerrainProfile>,
            With<PendingWaterProfile>,
        )>,
    >,
    /// The surfaces still waiting on the render world ([`SwapWaiting`]). Counted only while a view
    /// can see them, the same rule the settle uses ([`crate::swap_bridge::pending_drawn`]): a
    /// landing interior's own surfaces are in view the moment the camera arrives, so a crossing does
    /// not finish on textures loaded with nothing yet drawn, and it does not wait on a pre-streamed
    /// cell's surfaces, which no view can see and which the bridge can never confirm.
    swapping: Query<'w, 's, Has<SwapInCameraView>, With<SwapWaiting>>,
    failed: Query<'w, 's, (), With<AssetFailed>>,
}

impl PendingUnder<'_, '_> {
    /// How many entities at or under `root` failed for good ([`AssetFailed`]).
    pub(crate) fn failed(&self, root: Entity) -> usize {
        std::iter::once(root)
            .chain(self.children.iter_descendants(root))
            .filter(|entity| self.failed.contains(*entity))
            .count()
    }

    /// How many entities at or under `root` still carry an unfinished-work marker.
    pub(crate) fn count(&self, root: Entity) -> usize {
        std::iter::once(root)
            .chain(self.children.iter_descendants(root))
            .filter(|entity| {
                self.pending.contains(*entity)
                    || self.swapping.get(*entity).is_ok_and(|in_view| in_view)
            })
            .count()
    }
}

/// A pending-work marker for tests in other modules: terrain whose textures are still loading.
#[cfg(test)]
pub(crate) fn pending_marker_for_test() -> impl Bundle {
    PendingTerrainProfile {
        cell_id: 0,
        quadrant: 0,
        images: Vec::new(),
        normals: Vec::new(),
    }
}

type RenderPrimitiveQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static Mesh3d,
        Option<&'static MeshMaterial3d<StandardMaterial>>,
        Option<&'static GltfMaterialName>,
        Option<&'static GltfExtras>,
        Option<&'static NifDepthMaterialSource>,
    ),
>;

/// A spawned model's hierarchy nodes as the strict bounds check reads them. `Billboard` marks the
/// camera-facing nodes, whose authored rotation stands in for their camera-dependent one.
type SpawnedNodeQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static Transform,
        &'static GlobalTransform,
        Option<&'static crate::billboard::Billboard>,
    ),
>;

/// Every streamed cell root, in either tier: a full cell ([`StreamedCellRoot`]) or one of the
/// ring's terrain-only cells ([`DistantTerrainRoot`]). Both carry the cell they draw, so the
/// lifecycle check counts them together against the plan.
type CellRootsQuery<'world, 'state> = Query<
    'world,
    'state,
    (Entity, &'static CellRef, Option<&'static ExteriorCellGrid>),
    Or<(With<StreamedCellRoot>, With<DistantTerrainRoot>)>,
>;

/// The mesh and material stores, the camera and the pacing tracker the readiness scan also uses.
type ReadinessExtras<'world, 'state> = (
    ResMut<'world, Assets<Mesh>>,
    ResMut<'world, Assets<StandardMaterial>>,
    Query<'world, 'state, &'static Transform, With<StreamingCamera>>,
    ResMut<'world, crate::pacing::PacingTracker>,
    Query<'world, 'state, (), With<crate::physics::DynamicClutter>>,
    Option<Res<'world, Assets<WorldAsset>>>,
);

type PendingAssetQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static WorldAssetRoot,
        &'static PendingAssetProfile,
        &'static Transform,
        &'static GlobalTransform,
        &'static WorldTransform,
        Option<&'static ExpectedModelBounds>,
    ),
>;

/// Wall-clock timing for the pass Bevy spends instantiating converted models.
#[derive(Resource, Default)]
struct SceneSpawnBatch {
    started: Option<Instant>,
}

fn begin_scene_spawn_batch(mut batch: ResMut<SceneSpawnBatch>) {
    batch.started = Some(Instant::now());
}

/// Records the cost of Bevy's world-instance spawn pass and counts what the pass did.
///
/// The counts read the world rather than Bevy's instance bookkeeping: a reference gains a child the
/// moment its converted scene is written below it, so `Changed<Children>` on a reference that owns
/// an instance is exactly "this model was instantiated this frame". `WorldInstance` itself is
/// inserted when the reference joins the spawner's queue
/// (`bevy_world_serialization::world_asset_spawner`, `world_instance_spawner`) - which happens
/// whether or not the asset has landed - so `Added<WorldInstance>` counts the models *handed to*
/// the spawner this frame, the same thing as spawned only once arming waits for the asset.
fn end_scene_spawn_batch(
    mut batch: ResMut<SceneSpawnBatch>,
    spawned: Query<(), (With<WorldInstance>, Changed<Children>)>,
    armed: Query<(), Added<WorldInstance>>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
    mut runtime: Option<ResMut<runtime::StreamingRuntime>>,
) {
    if let Some(started) = batch.started.take() {
        if let Some(runtime) = runtime.as_mut() {
            runtime.previous_spawn_ms = started.elapsed().as_secs_f64() * 1000.0;
        }
        profiler.record_elapsed("scene/spawn_batch", started);
    }
    let spawned = spawned.iter().count() as u64;
    let armed = armed.iter().count() as u64;
    metrics.instances_spawned_this_frame = spawned;
    metrics.max_instances_spawned_per_frame = metrics.max_instances_spawned_per_frame.max(spawned);
    metrics.instances_armed_this_frame = armed;
    metrics.max_instances_armed_per_frame = metrics.max_instances_armed_per_frame.max(armed);
    profiler.set_gauge("scene/instances_spawned_this_frame", spawned as f64);
    profiler.set_gauge("scene/instances_armed_this_frame", armed as f64);
}

#[allow(clippy::too_many_arguments)]
fn arm_pending_models(
    config: Res<EngineConfig>,
    asset_server: Res<AssetServer>,
    world_assets: Option<Res<Assets<WorldAsset>>>,
    pending: Query<(Entity, &PendingModel, Option<&requests::SceneRequest>)>,
    cameras: Query<&Transform, With<StreamingCamera>>,
    origin: Option<Res<RenderOrigin>>,
    mut scheduling: Option<ResMut<requests::SceneSchedulingState>>,
    streaming: Option<Res<StreamingWorld>>,
    mut commands: Commands,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
    mut runtime: Option<ResMut<runtime::StreamingRuntime>>,
    mut ownership: crate::cluster::ownership::OwnershipGate,
) {
    use bevy::math::DVec3;
    let started = Instant::now();
    let backlog = pending.iter().count();
    let origin = origin.map_or(IVec2::ZERO, |origin| origin.0);
    let view = cameras
        .single()
        .ok()
        .map(|camera| priority::PriorityView {
            position: camera.translation.as_dvec3()
                + DVec3::new(
                    f64::from(origin.x) * f64::from(CELL_SIZE),
                    0.0,
                    -f64::from(origin.y) * f64::from(CELL_SIZE),
                ),
            forward: (camera.rotation * Vec3::NEG_Z).as_dvec3(),
        })
        .unwrap_or(priority::PriorityView {
            position: DVec3::NAN,
            forward: DVec3::NAN,
        });
    ownership.begin_frame();
    let mut ready: Vec<_> = pending
        .iter()
        .filter(|(entity, model, request)| {
            request.is_none_or(|request| {
                streaming
                    .as_ref()
                    .is_none_or(|streaming| requests::relevant(request, streaming))
            }) && model_can_spawn(world_assets.as_deref(), &asset_server, &model.handle)
                && ownership.can_arm(
                    *entity,
                    world_assets
                        .as_deref()
                        .and_then(|assets| assets.get(&model.handle)),
                    &mut commands,
                )
        })
        .map(|(entity, model, request)| {
            let rank = request
                .map(|request| {
                    priority::DemandPriority::new(
                        view,
                        request.center,
                        request.radius,
                        request.collision_candidate,
                        model.sequence,
                    )
                    .with_coarse_terrain(request.coarse_terrain)
                })
                .unwrap_or_else(|| {
                    priority::DemandPriority::new(view, DVec3::NAN, None, false, model.sequence)
                });
            (model.sequence, entity, model.handle.clone(), rank)
        })
        .collect();
    ready.sort_by_key(|(sequence, entity, _, _)| (*sequence, entity.to_bits()));
    let decision = runtime.as_ref().and_then(|runtime| runtime.decision);
    let budget = decision.map_or(config.max_model_spawns_per_frame, |decision| {
        decision.budgets.max_model_activations
    });
    let armed = if budget == 0 && decision.is_none() {
        ready.len()
    } else {
        budget.min(ready.len())
    };
    let mut choices = scheduling
        .as_ref()
        .map_or(0, |state| state.activation_choices);
    let selected: Vec<_> = if config.prioritize_streaming {
        let indices = priority::ordered_choices(
            &ready
                .iter()
                .map(|(_, _, _, rank)| *rank)
                .collect::<Vec<_>>(),
            choices,
            armed,
        );
        indices
            .into_iter()
            .map(|index| ready[index].clone())
            .collect()
    } else {
        // Preserve the linear FIFO drain when priority scheduling is disabled.
        ready.drain(..armed).collect()
    };
    let mut actually_armed = 0usize;
    let activation_started = Instant::now();
    for (_, entity, handle, _) in selected {
        if actually_armed != 0
            && decision.is_some_and(|decision| {
                activation_started.elapsed().as_micros()
                    >= u128::from(decision.budgets.max_activation_micros)
            })
        {
            break;
        }
        if let Some(runtime) = runtime.as_mut() {
            let request = pending.get(entity).ok().and_then(|(_, _, request)| request);
            if !runtime.reserve_placement(
                entity,
                request,
                config.interactive_world_physics()
                    && request.is_some_and(|request| request.collision_candidate),
            ) {
                continue;
            }
        }
        commands
            .entity(entity)
            .insert(WorldAssetRoot(handle))
            .remove::<PendingModel>();
        actually_armed += 1;
        choices = choices.saturating_add(1);
    }
    if let Some(state) = scheduling.as_mut() {
        state.activation_choices = choices;
    }
    let armed = actually_armed;
    let depth = backlog.saturating_sub(armed);
    metrics.arming_queue_depth = depth;
    metrics.peak_arming_queue_depth = metrics.peak_arming_queue_depth.max(backlog);
    profiler.increment("streaming/models_armed", armed as u64);
    profiler.set_gauge("streaming/arming_queue_depth", depth as f64);
    profiler.set_gauge(
        "streaming/peak_arming_queue_depth",
        metrics.peak_arming_queue_depth as f64,
    );
    profiler.record_elapsed("streaming/arm_models", started);
}

/// How many ready models to arm this frame.
///
/// `budget` is `--max-model-spawns-per-frame` (0 = every ready model). A burst larger than the budget
/// can drain in about [`ARMING_DRAIN_FRAMES`]: on arrival in a new place the whole window becomes
/// ready at once (2,000+ models at Riverwood), and at 4 a frame they would pop in over several
/// seconds. The budget grows with the backlog, up to [`ARMING_BURST_FACTOR`] times itself, so a
/// burst still never instantiates more than that in one frame.
#[cfg(test)]
fn models_to_arm(budget: usize, ready: usize) -> usize {
    if budget == 0 {
        return ready;
    }
    let scaled = ready
        .div_ceil(ARMING_DRAIN_FRAMES)
        .clamp(budget, budget * ARMING_BURST_FACTOR);
    scaled.min(ready)
}

/// The frames a backlog of ready models is spread over before the budget grows past its floor.
#[cfg(test)]
const ARMING_DRAIN_FRAMES: usize = 60;

/// The most the arming budget may grow to under a backlog, as a multiple of its configured floor.
#[cfg(test)]
const ARMING_BURST_FACTOR: usize = 8;

/// Whether Bevy can instantiate this model now.
///
/// The converted scene being in `Assets<WorldAsset>` is exactly the condition the spawner retries
/// on, so arming on it hands a model over on the first frame it can actually spawn. A model whose
/// load failed will never arrive: it is armed too, so the readiness scan reports the failure
/// exactly as it did when every model was armed at commit, instead of leaving the reference
/// pending for as long as its cell lives.
///
/// `world_assets` is `None` in an app that never registered the converted-scene asset collection:
/// the streaming, portal and transition tests add [`StreamingPlugin`] without the renderer's
/// world-serialization plugin, and such an app has no spawner either. There the asset server's own
/// answer decides, which is what keeps a reference from being pending forever in a world that
/// cannot load the model at all.
fn mark_world_instance_ready(
    ready: On<WorldInstanceReady>,
    mut pending: Query<&mut PendingAssetProfile>,
) {
    if let Ok(mut pending) = pending.get_mut(ready.entity) {
        pending.scene_spawned = true;
    }
}

fn model_can_spawn(
    world_assets: Option<&Assets<WorldAsset>>,
    asset_server: &AssetServer,
    handle: &Handle<WorldAsset>,
) -> bool {
    world_assets.map_or_else(
        || asset_server.is_loaded_with_dependencies(handle.id()),
        |assets| assets.contains(handle.id()),
    ) || matches!(
        asset_server
            .get_load_states(handle.id())
            .map(|(load, _, _)| load),
        Some(LoadState::Failed(_))
    )
}

/// Validate spawned model instances, account for failures and record completed readiness latency.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn track_asset_readiness(
    mut commands: Commands,
    config: Res<EngineConfig>,
    asset_server: Res<AssetServer>,
    pending: PendingAssetQuery,
    unarmed: Query<(), (With<PendingAssetProfile>, Without<WorldAssetRoot>)>,
    children: Query<&Children>,
    primitives: RenderPrimitiveQuery,
    transforms: SpawnedNodeQuery,
    scene_extras: Query<&GltfSceneExtras>,
    images: Res<Assets<Image>>,
    mut fallback_assets: ResMut<DiagnosticFallbackAssets>,
    mut static_cache: ResMut<StaticCollisionCache>,
    (mut meshes, mut materials, camera, mut pacing, clutter, world_assets): ReadinessExtras,
    (mut metrics, mut runtime): (
        ResMut<StreamingMetrics>,
        Option<ResMut<runtime::StreamingRuntime>>,
    ),
    mut profiler: ResMut<ProfilingState>,
) {
    let started = Instant::now();
    // While flying, how far from the camera each model is when it finishes loading: the number
    // that says how far behind loading falls at speed. The camera is a root entity, so its
    // `Transform` is its world position; `GlobalTransform` only updates in PostUpdate, after
    // `fly_camera` moved the camera in Update, and would lag a frame.
    let fly_camera = config
        .records_fly_lag()
        .then(|| camera.single().ok().map(|camera| camera.translation))
        .flatten();
    // A model still waiting in the arming queue has no scene yet, so the scan below cannot see it;
    // count it here so the readiness gates keep waiting for every queued model.
    metrics.pending_asset_instances = pending.iter().count() + unarmed.iter().count();
    let mut completed_this_scan = 0usize;
    // Live dynamic clutter bodies, including the ones this scan spawns (their commands are
    // deferred, so the query cannot see them yet). The cap bounds what a cell arrival can create.
    let mut live_clutter = clutter.iter().count();
    for (entity, root, pending, local, global, world_transform, expected_bounds) in &pending {
        if completed_this_scan != 0
            && runtime
                .as_ref()
                .and_then(|runtime| runtime.decision)
                .is_some_and(|decision| {
                    started.elapsed().as_micros()
                        >= u128::from(decision.budgets.max_collision_micros)
                })
        {
            break;
        }
        let static_candidate = pending.static_physics
            && fixed_collision_record_eligible(pending.base_record_type.as_deref());
        let load_failure =
            asset_server
                .get_load_states(root.0.id())
                .and_then(|(load, _, recursive)| match (load, recursive) {
                    (LoadState::Failed(error), _) => Some(error),
                    (_, RecursiveDependencyLoadState::Failed(error)) => Some(error),
                    _ => None,
                });
        if let Some(error) = load_failure {
            let chain = error_chain(error.as_ref());
            record_asset_failure(&mut metrics, &mut profiler, pending, chain, false);
            if static_candidate {
                note_static_proxy_skip(&mut metrics, &mut profiler);
            }
            hide_partial_scene(&mut commands, entity, &children);
            commands.entity(entity).try_insert(AssetFailed);
            if config.diagnostic_asset_fallbacks {
                spawn_diagnostic_fallback(
                    &mut commands,
                    entity,
                    &mut fallback_assets,
                    &mut meshes,
                    &mut materials,
                );
                metrics.diagnostic_fallbacks = metrics.diagnostic_fallbacks.saturating_add(1);
            }
            commands.entity(entity).try_remove::<PendingAssetProfile>();
            completed_this_scan += 1;
        } else if pending.scene_spawned && asset_server.is_loaded_with_dependencies(root.0.id()) {
            let transform_validation = validate_spawned_transforms_and_bounds(
                entity,
                local,
                global,
                world_transform,
                expected_bounds,
                world_assets
                    .as_deref()
                    .and_then(|assets| assets.get(&root.0)),
                &children,
                &transforms,
                &primitives,
                &meshes,
            );
            let transform_summary = match transform_validation {
                Ok(summary) => summary,
                Err(reason) => {
                    metrics.transform_bounds_validation_failures = metrics
                        .transform_bounds_validation_failures
                        .saturating_add(1);
                    profiler.increment("transforms/validation_failures", 1);
                    record_asset_failure(&mut metrics, &mut profiler, pending, vec![reason], false);
                    if static_candidate {
                        note_static_proxy_skip(&mut metrics, &mut profiler);
                    }
                    hide_partial_scene(&mut commands, entity, &children);
                    commands.entity(entity).try_insert(AssetFailed);
                    if config.diagnostic_asset_fallbacks {
                        spawn_diagnostic_fallback(
                            &mut commands,
                            entity,
                            &mut fallback_assets,
                            &mut meshes,
                            &mut materials,
                        );
                        metrics.diagnostic_fallbacks =
                            metrics.diagnostic_fallbacks.saturating_add(1);
                    }
                    commands.entity(entity).try_remove::<PendingAssetProfile>();
                    completed_this_scan += 1;
                    continue;
                }
            };
            // An empty model - the converter's scene for an editor marker - spawns no render
            // primitive at all, so there is nothing to place, draw or validate: skip it and count
            // it separately instead of failing the run's bounds gate. Everything else about the
            // reference stays: it keeps its transform, its load door and its light, and its scene
            // is left alone (it is empty; there is nothing in it to hide).
            if transform_summary.empty_model {
                if static_candidate {
                    note_static_proxy_skip(&mut metrics, &mut profiler);
                }
                metrics.empty_model_references = metrics.empty_model_references.saturating_add(1);
                profiler.increment("assets/empty_model_references", 1);
                profiler.event(&pending.path, "empty_model", None);
                commands.entity(entity).try_remove::<PendingAssetProfile>();
                completed_this_scan += 1;
                continue;
            }
            let validation = validate_spawned_asset(
                entity,
                &children,
                &primitives,
                &meshes,
                &materials,
                &images,
            );
            let summary = match validation {
                Ok(summary) => summary,
                Err(reason) => {
                    record_asset_failure(&mut metrics, &mut profiler, pending, vec![reason], true);
                    if static_candidate {
                        note_static_proxy_skip(&mut metrics, &mut profiler);
                    }
                    hide_partial_scene(&mut commands, entity, &children);
                    commands.entity(entity).try_insert(AssetFailed);
                    if config.diagnostic_asset_fallbacks {
                        spawn_diagnostic_fallback(
                            &mut commands,
                            entity,
                            &mut fallback_assets,
                            &mut meshes,
                            &mut materials,
                        );
                        metrics.diagnostic_fallbacks =
                            metrics.diagnostic_fallbacks.saturating_add(1);
                    }
                    commands.entity(entity).try_remove::<PendingAssetProfile>();
                    completed_this_scan += 1;
                    continue;
                }
            };
            // Movable references get exactly one of the two paths: a dynamic clutter body here,
            // or (for an ineligible type) nothing at all, which is what they had before this slice.
            if pending.static_physics
                && dynamic_clutter_record_eligible(pending.base_record_type.as_deref())
            {
                let key = (pending.base_record_type.clone(), pending.path.clone());
                let first_placement = !static_cache.0.contains_key(&key);
                let decision = static_cache.0.entry(key).or_insert_with(|| {
                    clutter_collision_from_hierarchy(entity, &children, &scene_extras)
                });
                match decision {
                    StaticCollisionDecision::Clutter(spec) => {
                        if live_clutter >= crate::physics::MAX_LIVE_DYNAMIC_CLUTTER {
                            metrics.dynamic_clutter_overflow =
                                metrics.dynamic_clutter_overflow.saturating_add(1);
                            profiler.increment("physics/dynamic_clutter_overflow", 1);
                            if first_placement {
                                warn!(
                                    model = %pending.path,
                                    limit = crate::physics::MAX_LIVE_DYNAMIC_CLUTTER,
                                    "dynamic clutter over the live cap; reference keeps no collider"
                                );
                            }
                        } else {
                            crate::physics::spawn_dynamic_clutter(
                                &mut commands,
                                entity,
                                &spec.body,
                                spec.parts
                                    .iter()
                                    .map(|part| (part.translation, &part.collider)),
                                spec.mass,
                                local.scale.x,
                            );
                            live_clutter += 1;
                            metrics.dynamic_clutter_bodies =
                                metrics.dynamic_clutter_bodies.saturating_add(1);
                            profiler.increment("physics/dynamic_clutter_bodies", 1);
                            if spec.convexified {
                                metrics.dynamic_clutter_convexified =
                                    metrics.dynamic_clutter_convexified.saturating_add(1);
                            }
                            if spec.ignored_bodies > 0 {
                                if first_placement {
                                    warn!(
                                        model = %pending.path,
                                        ignored = spec.ignored_bodies,
                                        "model has more than one dynamic body; only the first is simulated"
                                    );
                                }
                                metrics.dynamic_clutter_extra_bodies_ignored = metrics
                                    .dynamic_clutter_extra_bodies_ignored
                                    .saturating_add(spec.ignored_bodies as u64);
                            }
                            if spec.mass.is_none() {
                                metrics.dynamic_clutter_mass_fallbacks =
                                    metrics.dynamic_clutter_mass_fallbacks.saturating_add(1);
                            }
                        }
                    }
                    StaticCollisionDecision::ClutterSkipped(reason) => {
                        if first_placement {
                            warn!(model = %pending.path, %reason, "dynamic clutter skipped");
                        }
                        metrics.dynamic_clutter_skipped =
                            metrics.dynamic_clutter_skipped.saturating_add(1);
                        profiler.increment("physics/dynamic_clutter_skipped", 1);
                    }
                    _ => {}
                }
            }
            if static_candidate {
                // One decision per (record type, model): the same converted model is the same
                // collision problem wherever it is placed, and the cache is what keeps a village of
                // repeated statics from re-walking the same hierarchy. A placement that fails here
                // logs once and is skipped; it never fails the cell.
                let key = (pending.base_record_type.clone(), pending.path.clone());
                let first_placement = !static_cache.0.contains_key(&key);
                let decision = static_cache.0.entry(key).or_insert_with(|| {
                    static_collision_from_hierarchy(
                        pending.base_record_type.as_deref(),
                        &pending.path,
                        entity,
                        &children,
                        &scene_extras,
                        &transforms,
                        &primitives,
                        &meshes,
                        &materials,
                    )
                });
                match decision {
                    StaticCollisionDecision::Authored(parts, skipped) => {
                        attach_authored_collision(&mut commands, entity, parts);
                        metrics.static_colliders_authored =
                            metrics.static_colliders_authored.saturating_add(1);
                        profiler.increment("physics/static_authored_placements", 1);
                        if !skipped.is_empty() {
                            metrics.static_colliders_partial =
                                metrics.static_colliders_partial.saturating_add(1);
                            profiler.increment("physics/static_authored_partial", 1);
                            if first_placement {
                                warn!(model = %pending.path, reason = %skipped.join("; "), "authored collision is partial");
                            }
                        }
                    }
                    StaticCollisionDecision::RenderProxy(collider) => {
                        commands.entity(entity).insert((
                            StaticColliderSource::RenderProxy,
                            RigidBody::Fixed,
                            collider.clone(),
                            crate::physics::world_collision_groups(),
                        ));
                        metrics.static_colliders_proxy =
                            metrics.static_colliders_proxy.saturating_add(1);
                        profiler.increment("physics/static_proxy_placements", 1);
                    }
                    StaticCollisionDecision::AuthoredAbsent => {
                        metrics.static_colliders_authored_absent =
                            metrics.static_colliders_authored_absent.saturating_add(1);
                        profiler.increment("physics/static_authored_absent", 1);
                    }
                    // A fixed record type never asks for the clutter decision, so these arms cannot
                    // be reached on this path; the movable path above owns them.
                    StaticCollisionDecision::LegacyExcluded
                    | StaticCollisionDecision::Clutter(_)
                    | StaticCollisionDecision::ClutterNone
                    | StaticCollisionDecision::ClutterSkipped(_) => {}
                    StaticCollisionDecision::Skipped(reason) => {
                        if first_placement {
                            warn!(model = %pending.path, %reason, "static collision skipped");
                        }
                        note_static_proxy_skip(&mut metrics, &mut profiler);
                    }
                }
            }
            let micros = pending
                .started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64;
            metrics.assets_ready = metrics.assets_ready.saturating_add(1);
            if let Some(camera) = fly_camera {
                pacing.record_model_ready(crate::pacing::horizontal_distance(
                    global.translation(),
                    camera,
                ));
            }
            metrics.meshes_validated = metrics
                .meshes_validated
                .saturating_add(summary.meshes as u64);
            metrics.materials_validated = metrics
                .materials_validated
                .saturating_add(summary.materials as u64);
            metrics.images_validated = metrics
                .images_validated
                .saturating_add(summary.images as u64);
            metrics.transform_instances_validated =
                metrics.transform_instances_validated.saturating_add(1);
            metrics.transform_nodes_validated = metrics
                .transform_nodes_validated
                .saturating_add(transform_summary.nodes as u64);
            metrics.bounds_validated = metrics.bounds_validated.saturating_add(1);
            metrics.max_asset_ready_micros = metrics.max_asset_ready_micros.max(micros);
            profiler.record_completed_latency_micros("assets/model_ready", micros);
            profiler.event(&pending.path, "asset_ready", Some(micros as f64 / 1000.0));
            commands.entity(entity).try_remove::<PendingAssetProfile>();
            completed_this_scan += 1;
        }
    }
    metrics.pending_asset_instances = metrics
        .pending_asset_instances
        .saturating_sub(completed_this_scan);
    metrics.instances_completed_this_scan = completed_this_scan as u64;
    metrics.max_instances_completed_per_scan = metrics
        .max_instances_completed_per_scan
        .max(metrics.instances_completed_this_scan);
    profiler.set_gauge(
        "assets/pending_instances",
        metrics.pending_asset_instances as f64,
    );
    profiler.set_gauge(
        "assets/instances_completed_this_scan",
        metrics.instances_completed_this_scan as f64,
    );
    profiler.record_elapsed("assets/readiness_scan", started);
    if let Some(runtime) = runtime.as_mut() {
        runtime.previous_validation_ms = started.elapsed().as_secs_f64() * 1000.0;
    }
}

fn note_static_proxy_skip(metrics: &mut StreamingMetrics, profiler: &mut ProfilingState) {
    metrics.static_colliders_skipped = metrics.static_colliders_skipped.saturating_add(1);
    profiler.increment("physics/static_proxy_skipped", 1);
}

/// Opens the merged data of an exterior cell for drawing, or counts why the cell takes the
/// quadrant path. Every cell with terrain ends in exactly one of the two counters.
fn choose_merged_terrain(
    merge: Option<&mut TerrainMerge>,
    key: CellKey,
    cell_id: u32,
    metrics: &mut StreamingMetrics,
    profiler: &mut ProfilingState,
) -> Option<MergedSpawn> {
    let lookup = match merge {
        Some(merge) => match merge.lookup(key, cell_id) {
            CellLookup::Found(cell) => match merged_geometry_check(&cell) {
                Ok(()) => match (
                    merge.classes().cloned(),
                    merge.material().cloned(),
                    merge.material_images().cloned(),
                ) {
                    (Some(classes), Some(material), Some(images)) => {
                        return Some(MergedSpawn {
                            cell,
                            classes,
                            material,
                            images,
                        });
                    }
                    _ => CellLookup::Unavailable,
                },
                Err(reason) => CellLookup::Invalid(reason),
            },
            other => other,
        },
        None => CellLookup::Unavailable,
    };
    count_merged_fallback(metrics, profiler, cell_id, lookup);
    None
}

/// Counts one terrain cell drawn by the quadrant path and says why, once.
fn count_merged_fallback(
    metrics: &mut StreamingMetrics,
    profiler: &mut ProfilingState,
    cell_id: u32,
    lookup: CellLookup,
) {
    metrics.terrain_cells_fallback = metrics.terrain_cells_fallback.saturating_add(1);
    profiler.increment("terrain/cells_fallback", 1);
    match lookup {
        CellLookup::NoData => {
            metrics.terrain_cells_fallback_expected_merged = metrics
                .terrain_cells_fallback_expected_merged
                .saturating_add(1);
            // One line per cell would flood the log for a worldspace the converter never merged;
            // the counter above is what reports it.
            debug!(
                cell = format_args!("{cell_id:08X}"),
                "no merged terrain data for this cell; drawn by quadrant"
            );
        }
        CellLookup::Invalid(reason) => {
            metrics.terrain_cells_fallback_expected_merged = metrics
                .terrain_cells_fallback_expected_merged
                .saturating_add(1);
            metrics.terrain_cells_merged_invalid =
                metrics.terrain_cells_merged_invalid.saturating_add(1);
            warn!(
                cell = format_args!("{cell_id:08X}"),
                %reason,
                "merged terrain data failed validation; drawn by quadrant"
            );
        }
        CellLookup::Unavailable | CellLookup::Found(_) => {
            debug!(
                cell = format_args!("{cell_id:08X}"),
                "merged terrain is not in use; drawn by quadrant"
            );
        }
    }
}

/// What the merged path needs beyond [`TerrainCellMesh::validate`]: seam duplicates that agree,
/// so the cell has one height field, and no zero normal, which today's validation refuses.
fn merged_geometry_check(cell: &TerrainCellMesh) -> Result<(), String> {
    crate::terrain_merge::mesh::cell_to_grid(cell)?;
    if crate::terrain_merge::mesh::has_zero_normal(cell) {
        return Err("terrain contains a zero-length packed normal".to_owned());
    }
    Ok(())
}

/// Drops the held responses whose load is no longer wanted, releasing each one's outstanding slot.
/// Setup can hold cells for up to [`terrain_merge::SETUP_TIMEOUT`]; a held response that went stale
/// meanwhile must not keep its slot until then. Current held responses keep their place in order.
fn release_stale_held_responses(
    streaming: &mut StreamingWorld,
    metrics: &mut StreamingMetrics,
    profiler: &mut ProfilingState,
    mut runtime: Option<&mut runtime::StreamingRuntime>,
) {
    let held = std::mem::take(&mut streaming.held_responses);
    for response in held {
        if streaming.awaits(response.key, response.generation, response.detail) {
            streaming.held_responses.push_back(response);
        } else {
            streaming.consume_cell_response(response.key, response.generation, response.detail);
            if let Some(runtime) = runtime.as_mut() {
                runtime.retire_cell_attempt(response.key, response.generation, response.detail);
            }
            metrics.stale_responses += 1;
            profiler.event(format!("{:?}", response.key), "stale_discarded", None);
        }
    }
}

/// Whether a database response waits for the merged terrain setup: only a loaded exterior cell
/// that has terrain, and only while the setup is in progress. Logs the first hold.
fn should_hold_response(
    merge: &mut TerrainMerge,
    result: &Result<CellPayload, String>,
    has_terrain: impl FnOnce(u32) -> bool,
) -> bool {
    let Ok(payload) = result else {
        return false;
    };
    if !merge.holds_cell(payload.key, has_terrain(payload.cell_id)) {
        return false;
    }
    if merge.note_hold() {
        info!(
            state = %merge.state.label(),
            "holding exterior terrain cells until the land texture arrays are ready"
        );
    }
    true
}

/// Welds a cell's terrain onto its resident neighbours and validates it, choosing its path.
///
/// A merged cell is welded with [`weld_merged_cell`]. If that fails, the cell is not a failed
/// cell: the quadrant path gets the same chance to draw it, and the cell is counted as a fallback
/// with the reason. A cell with no merged data validates and welds as the quadrant path always has.
/// The settled merged data, if the cell keeps it, is returned.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn settle_cell_terrain(
    key: CellKey,
    cell_id: u32,
    merged: Option<MergedSpawn>,
    terrain: &mut TerrainSnapshot,
    catalog: &AssetCatalog,
    continuity: &mut TerrainContinuity,
    metrics: &mut StreamingMetrics,
    profiler: &mut ProfilingState,
) -> Result<Option<MergedSpawn>, String> {
    settle_cell_terrain_canonical(
        key, cell_id, merged, terrain, catalog, continuity, metrics, profiler, None,
    )
}

#[allow(clippy::too_many_arguments)]
fn settle_cell_terrain_canonical(
    key: CellKey,
    cell_id: u32,
    merged: Option<MergedSpawn>,
    terrain: &mut TerrainSnapshot,
    catalog: &AssetCatalog,
    continuity: &mut TerrainContinuity,
    metrics: &mut StreamingMetrics,
    profiler: &mut ProfilingState,
    canonical: Option<&[f32]>,
) -> Result<Option<MergedSpawn>, String> {
    let mut merged = merged;
    if let Some(spawn) = &mut merged {
        let weld_started = Instant::now();
        let welded = if let Some(heights) = canonical {
            // Both near representations consume the cluster's finalized whole-world seam choice.
            let (_, normals) = crate::terrain_merge::mesh::cell_to_grid(&spawn.cell)?;
            if heights.len().saturating_mul(3) != normals.len() {
                return Err("canonical terrain grid dimensions differ".to_owned());
            }
            let grid = TerrainSnapshot {
                cell_id,
                width: shared::LAND_SIDE,
                height: shared::LAND_SIDE,
                heights: heights.to_vec(),
                normals: terrain.normals.clone(),
                vertex_colors: Vec::new(),
                layers: Vec::new(),
                water_height: None,
                water_type_form_id: None,
            };
            crate::terrain_merge::mesh::write_grid(
                &mut spawn.cell,
                &grid.heights,
                &terrain.normals,
            );
            continuity.edges.insert(key, TerrainEdges::of(&grid));
            Ok(())
        } else {
            weld_merged_cell(key, &mut spawn.cell, continuity, metrics)
        };
        profiler.record_elapsed("streaming/terrain_seam_weld", weld_started);
        if let Err(reason) = welded {
            merged = None;
            count_merged_fallback(
                metrics,
                profiler,
                cell_id,
                CellLookup::Invalid(format!("weld failed: {reason}")),
            );
        }
    }
    if merged.is_none() {
        let validation_started = Instant::now();
        let validation = validate_terrain_snapshot(terrain, catalog);
        profiler.record_elapsed("streaming/terrain_validation", validation_started);
        validation?;
        let weld_started = Instant::now();
        let welded = if canonical.is_some() {
            continuity.edges.insert(key, TerrainEdges::of(terrain));
            Ok(())
        } else {
            validate_and_register_terrain_edges(key, terrain, continuity, metrics)
        };
        profiler.record_elapsed("streaming/terrain_seam_weld", weld_started);
        welded?;
    }
    Ok(merged)
}

/// Welds a merged cell's edges onto its resident neighbours with the same code the quadrant path
/// uses. The cell's 33 x 33 height and normal grid goes through
/// [`validate_and_register_terrain_edges`] unchanged, and the welded grid is written back to every
/// vertex, seam duplicates included, so the two paths agree sample for sample and either can be
/// the neighbour of the other.
fn weld_merged_cell(
    key: CellKey,
    cell: &mut TerrainCellMesh,
    continuity: &mut TerrainContinuity,
    metrics: &mut StreamingMetrics,
) -> Result<(), String> {
    let (heights, normals) = crate::terrain_merge::mesh::cell_to_grid(cell)?;
    let mut grid = TerrainSnapshot {
        cell_id: cell.cell_id,
        width: shared::LAND_SIDE,
        height: shared::LAND_SIDE,
        heights,
        normals,
        vertex_colors: Vec::new(),
        layers: Vec::new(),
        water_height: None,
        water_type_form_id: None,
    };
    validate_and_register_terrain_edges(key, &mut grid, continuity, metrics)?;
    crate::terrain_merge::mesh::write_grid(cell, &grid.heights, &grid.normals);
    Ok(())
}

/// Makes a spawned merged terrain mesh visible and its collider live. Its arrays were validated
/// before any cell was committed, so there is nothing left to wait for.
fn track_merged_surface_readiness(
    mut commands: Commands,
    config: Res<EngineConfig>,
    pending: Query<Entity, With<PendingMergedTerrain>>,
    mut lod_streaming: ResMut<lod::LodStreaming>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    for entity in &pending {
        metrics.terrain_patches_validated = metrics.terrain_patches_validated.saturating_add(1);
        profiler.increment("terrain/patches_validated", 1);
        commands
            .entity(entity)
            .insert((Visibility::Inherited, TerrainSurfaceReady))
            .remove::<PendingMergedTerrain>();
        lod_streaming.visibility_dirty = true;
    }
}

#[allow(clippy::too_many_arguments)]
fn track_surface_readiness(
    mut commands: Commands,
    config: Res<EngineConfig>,
    asset_server: Res<AssetServer>,
    images: Res<Assets<Image>>,
    terrain: Query<(Entity, &PendingTerrainProfile)>,
    water: Query<(Entity, &PendingWaterProfile)>,
    mut lod_streaming: ResMut<lod::LodStreaming>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    metrics.pending_surface_instances = terrain.iter().count() + water.iter().count();
    let mut completed = 0usize;
    for (entity, pending) in &terrain {
        let state = terrain_dependency_state(
            validate_surface_dependencies(
                &asset_server,
                &images,
                &pending.images,
                SurfaceImageKind::Terrain,
            ),
            || {
                validate_surface_dependencies(
                    &asset_server,
                    &images,
                    &pending.normals,
                    SurfaceImageKind::TerrainNormal,
                )
            },
        );
        match state {
            SurfaceDependencyState::Pending => {}
            SurfaceDependencyState::Ready => {
                let normals_validated = pending.normals.len();
                metrics.terrain_patches_validated =
                    metrics.terrain_patches_validated.saturating_add(1);
                metrics.materials_validated = metrics.materials_validated.saturating_add(1);
                metrics.images_validated = metrics
                    .images_validated
                    .saturating_add((pending.images.len() + normals_validated) as u64);
                profiler.increment("terrain/patches_validated", 1);
                commands
                    .entity(entity)
                    .try_insert((Visibility::Inherited, TerrainSurfaceReady));
                lod_streaming.visibility_dirty = true;
                commands
                    .entity(entity)
                    .try_remove::<PendingTerrainProfile>();
                completed += 1;
            }
            SurfaceDependencyState::Failed(reason) => {
                metrics.asset_load_failures = metrics.asset_load_failures.saturating_add(1);
                metrics.terrain_validation_failures =
                    metrics.terrain_validation_failures.saturating_add(1);
                metrics.asset_failures.push(AssetFailure {
                    model_path: format!(
                        "terrain/{:08X}/quadrant-{}",
                        pending.cell_id, pending.quadrant
                    ),
                    reference_form_id: 0,
                    base_form_id: 0,
                    cell_id: pending.cell_id,
                    dependency_chain: vec![reason],
                });
                profiler.increment("terrain/validation_failures", 1);
                commands
                    .entity(entity)
                    .try_insert((Visibility::Hidden, AssetFailed));
                commands
                    .entity(entity)
                    .try_remove::<PendingTerrainProfile>();
                completed += 1;
            }
        }
    }
    for (entity, pending) in &water {
        let handles: Vec<_> = pending.flow_normal.iter().cloned().collect();
        match validate_surface_dependencies(
            &asset_server,
            &images,
            &handles,
            SurfaceImageKind::Water,
        ) {
            SurfaceDependencyState::Pending => {}
            SurfaceDependencyState::Ready => {
                metrics.water_surfaces_validated =
                    metrics.water_surfaces_validated.saturating_add(1);
                metrics.materials_validated = metrics.materials_validated.saturating_add(1);
                metrics.images_validated = metrics
                    .images_validated
                    .saturating_add(handles.len() as u64);
                profiler.increment("water/surfaces_validated", 1);
                commands.entity(entity).try_insert(Visibility::Inherited);
                commands.entity(entity).try_remove::<PendingWaterProfile>();
                completed += 1;
            }
            SurfaceDependencyState::Failed(reason) => {
                metrics.asset_load_failures = metrics.asset_load_failures.saturating_add(1);
                metrics.water_validation_failures =
                    metrics.water_validation_failures.saturating_add(1);
                metrics.asset_failures.push(AssetFailure {
                    model_path: format!("water/{:08X}", pending.cell_id),
                    reference_form_id: 0,
                    base_form_id: 0,
                    cell_id: pending.cell_id,
                    dependency_chain: vec![reason],
                });
                profiler.increment("water/validation_failures", 1);
                commands
                    .entity(entity)
                    .try_insert((Visibility::Hidden, AssetFailed));
                commands.entity(entity).try_remove::<PendingWaterProfile>();
                completed += 1;
            }
        }
    }
    metrics.pending_surface_instances = metrics.pending_surface_instances.saturating_sub(completed);
    profiler.set_gauge(
        "assets/pending_surface_instances",
        metrics.pending_surface_instances as f64,
    );
}

/// Terrain patches whose surface is ready on the CPU side but not yet drawn.
type ReadyUndrawnPatches = (
    With<TerrainPatch>,
    With<TerrainSurfaceReady>,
    Without<SwapDrawn>,
);

/// The hole-risk gauge (design-1091 C5): how many terrain quadrants this world has validated
/// (`TerrainSurfaceReady`) but the render world has not confirmed drawn (`SwapDrawn`).
///
/// This is the number the LOD-hide rule runs ahead of the GPU by. `lod::update_terrain_lod_visibility`
/// hides a grid's tile the moment its quadrants are surface-ready, and that flag says the textures
/// are in `Assets<Image>` - nothing about the mesh, the material's bind group or the pipeline.
///
/// Read it as an upper bound, not as a leak: a quadrant no view can see is never confirmed drawn,
/// so an ordinary frame counts every patch behind the camera as well. What it measures is how the
/// gap moves - when the camera turns, when a cell arrives, when a chunk lands - which is what the
/// per-frame series column is for.
///
/// It walks every terrain patch, so it is installed only for a run that profiles.
fn record_terrain_swap_gauge(
    ready: Query<(), ReadyUndrawnPatches>,
    mut profiler: ResMut<ProfilingState>,
) {
    profiler.set_gauge("swap/terrain_ready_not_drawn", ready.iter().count() as f64);
}

enum SurfaceDependencyState {
    Pending,
    Ready,
    Failed(String),
}

/// Undeclared normals require no assets; declared normals must validate without substitution.
fn terrain_dependency_state(
    diffuse: SurfaceDependencyState,
    normals: impl FnOnce() -> SurfaceDependencyState,
) -> SurfaceDependencyState {
    match diffuse {
        SurfaceDependencyState::Ready => normals(),
        other => other,
    }
}

/// Which surface a set of images belongs to, and so which colour space and sampler each must have
/// loaded with: a landscape's diffuse layers are sRGB, its normal maps are linear, and water's flow
/// normals are linear.
#[derive(Clone, Copy)]
enum SurfaceImageKind {
    Terrain,
    TerrainNormal,
    Water,
}

fn terrain_sampler_repeats(sampler: &ImageSampler) -> bool {
    matches!(
        sampler,
        ImageSampler::Descriptor(descriptor)
            if descriptor.address_mode_u == ImageAddressMode::Repeat
                && descriptor.address_mode_v == ImageAddressMode::Repeat
    )
}

fn validate_surface_dependencies(
    asset_server: &AssetServer,
    images: &Assets<Image>,
    handles: &[Handle<Image>],
    kind: SurfaceImageKind,
) -> SurfaceDependencyState {
    for handle in handles {
        if let Some((load, _, recursive)) = asset_server.get_load_states(handle.id()) {
            let failed = match (load, recursive) {
                (LoadState::Failed(error), _) => Some(error),
                (_, RecursiveDependencyLoadState::Failed(error)) => Some(error),
                _ => None,
            };
            if let Some(error) = failed {
                return SurfaceDependencyState::Failed(error_chain(error.as_ref()).join(" -> "));
            }
        }
        if !asset_server.is_loaded_with_dependencies(handle.id()) {
            return SurfaceDependencyState::Pending;
        }
        let Some(image) = images.get(handle) else {
            return SurfaceDependencyState::Pending;
        };
        if image.texture_descriptor.format.is_srgb() != matches!(kind, SurfaceImageKind::Terrain) {
            return SurfaceDependencyState::Failed(format!(
                "image {:?} has wrong color space {:?}",
                handle.id(),
                image.texture_descriptor.format
            ));
        }
        if let Err(reason) = validate_image_sampler("surface", &image.sampler) {
            return SurfaceDependencyState::Failed(reason);
        }
        if matches!(
            kind,
            SurfaceImageKind::Terrain | SurfaceImageKind::TerrainNormal
        ) && !terrain_sampler_repeats(&image.sampler)
        {
            return SurfaceDependencyState::Failed(format!(
                "terrain image {:?} did not load with a repeating sampler",
                handle.id()
            ));
        }
    }
    SurfaceDependencyState::Ready
}

#[derive(Debug, Default, PartialEq, Eq)]
struct AssetValidationSummary {
    meshes: usize,
    materials: usize,
    images: usize,
    excluded_materials: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct TransformValidationSummary {
    nodes: usize,
    /// The model is an empty glTF scene - an editor marker the converter dropped every shape of -
    /// so it carries no converted bounds and the spawned hierarchy holds no render primitive.
    /// Nothing was drawn, so the reference is counted in
    /// [`StreamingMetrics::empty_model_references`] rather than as a validated instance.
    empty_model: bool,
}

#[allow(clippy::too_many_arguments)]
fn validate_spawned_transforms_and_bounds(
    root: Entity,
    root_local: &Transform,
    root_global: &GlobalTransform,
    world_transform: &WorldTransform,
    expected: Option<&ExpectedModelBounds>,
    converted_model: Option<&WorldAsset>,
    children: &Query<&Children>,
    transforms: &SpawnedNodeQuery,
    primitives: &RenderPrimitiveQuery,
    meshes: &Assets<Mesh>,
) -> Result<TransformValidationSummary, String> {
    validate_transform("reference", root_local, root_global)?;
    let local_matrix = root_local.to_matrix();
    if matrix_max_difference(local_matrix, world_transform.0) > 1.0e-4 {
        return Err("WorldTransform differs from the spawned reference Transform".to_owned());
    }
    let Some(expected) = expected else {
        // A model the converter wrote no aggregate bounds for is an empty scene: a model with no
        // renderable geometry, so there is nothing to place, draw or bound. What makes it empty is
        // read from the converted model itself - the scene the converter wrote, which declares no
        // node and no mesh - rather than from the spawned instance, so a hierarchy that has not
        // spawned yet cannot pass as an empty model. A scene that does declare a node or a mesh
        // and still arrived without aggregate bounds is a conversion defect, and stays as fatal as
        // any other.
        let scene = converted_model.ok_or_else(|| {
            "the converted model is not loaded while validating its bounds; reconvert the asset"
                .to_owned()
        })?;
        let contents = converted_scene_contents(scene);
        return if contents.is_empty() {
            Ok(TransformValidationSummary {
                nodes: 0,
                empty_model: true,
            })
        } else {
            Err(format!(
                "converted model has no validated aggregate bounds; reconvert the asset (its converted scene is not empty: {} mesh primitives, {} nodes)",
                contents.meshes, contents.nodes
            ))
        };
    };
    ExpectedModelBounds::new(expected.min, expected.max)
        .ok_or_else(|| "converted model bounds are non-finite, empty, or inverted".to_owned())?;

    let spawned = spawned_relative_bounds(root, children, transforms, primitives, meshes)?;
    let extent = (expected.max - expected.min).abs().max_element().max(1.0);
    let tolerance = (extent * 1.0e-4).max(1.0e-3);
    let error = (spawned.min - expected.min)
        .abs()
        .max((spawned.max - expected.max).abs())
        .max_element();
    if !error.is_finite() || error > tolerance {
        return Err(format!(
            "spawned hierarchy bounds diverge from conversion: expected {:?}..{:?}, actual {:?}..{:?}, tolerance {tolerance}",
            expected.min, expected.max, spawned.min, spawned.max
        ));
    }
    Ok(TransformValidationSummary {
        nodes: spawned.nodes,
        empty_model: false,
    })
}

/// Aggregate bounds of the spawned hierarchy's meshes, measured in model space: every
/// node's transform relative to `root` is composed from the local `Transform`s along its
/// parent chain, so the result does not depend on where the reference sits in the world.
///
/// `root_global.affine().inverse() * global.affine()` is the same matrix in exact
/// arithmetic, but evaluating it in world space subtracts two translations the size of the
/// reference's distance from the render origin in f32. That cancels catastrophically - the
/// error grows with the distance and can exceed the caller's tolerance for a model the
/// converter produced correctly - whereas the composed local product stays at model-space
/// magnitudes.
#[derive(Debug, Default, PartialEq, Eq)]
struct ConvertedSceneContents {
    /// One per glTF primitive the converter exported: an entity carrying a [`Mesh3d`].
    meshes: usize,
    /// Every entity the loader attached below another one, which in a glTF scene is every node;
    /// the scene's own root is the only entity without a parent.
    nodes: usize,
}

impl ConvertedSceneContents {
    /// The converter's empty scene, which is what a model with no renderable geometry converts
    /// to: no node and no mesh to place, draw or bound.
    fn is_empty(&self) -> bool {
        self.meshes == 0 && self.nodes == 0
    }
}

/// Counts what a converted model holds. Taken from the loaded asset rather than from its spawned
/// instance, so what is read is the whole converted file: a scene whose entities have not spawned
/// yet, or one whose geometry an exporter dropped, cannot pass as an empty model.
fn converted_scene_contents(scene: &WorldAsset) -> ConvertedSceneContents {
    let mut contents = ConvertedSceneContents::default();
    for entity in scene.world.iter_entities() {
        contents.meshes += usize::from(entity.contains::<Mesh3d>());
        contents.nodes += usize::from(entity.contains::<ChildOf>());
    }
    contents
}

fn spawned_relative_bounds(
    root: Entity,
    children: &Query<&Children>,
    transforms: &SpawnedNodeQuery,
    primitives: &RenderPrimitiveQuery,
    meshes: &Assets<Mesh>,
) -> Result<SpawnedBounds, String> {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let mut nodes = 0;
    let mut bounded_meshes = 0;
    if let Ok(children_of_root) = children.get(root) {
        for child in children_of_root.iter() {
            accumulate_relative_bounds(
                child,
                Affine3A::IDENTITY,
                children,
                transforms,
                primitives,
                meshes,
                &mut nodes,
                &mut bounded_meshes,
                &mut min,
                &mut max,
            )?;
        }
    }
    if bounded_meshes == 0 {
        return Err("spawned hierarchy contains no bounded mesh".to_owned());
    }
    Ok(SpawnedBounds { min, max, nodes })
}

#[allow(clippy::too_many_arguments)]
fn accumulate_relative_bounds(
    entity: Entity,
    relative_to_root: Affine3A,
    children: &Query<&Children>,
    transforms: &SpawnedNodeQuery,
    primitives: &RenderPrimitiveQuery,
    meshes: &Assets<Mesh>,
    nodes: &mut usize,
    bounded_meshes: &mut usize,
    actual_min: &mut Vec3,
    actual_max: &mut Vec3,
) -> Result<(), String> {
    // An explicit stack, not recursion: a deeply nested model must not overflow the thread's
    // stack. Children are pushed in reverse so they are visited in order, as before.
    let mut stack = vec![(entity, relative_to_root)];
    while let Some((entity, parent_to_root)) = stack.pop() {
        let (local, global, billboard) = transforms
            .get(entity)
            .map_err(|_| format!("hierarchy node {entity:?} has no local/global transform"))?;
        validate_transform(&format!("hierarchy node {entity:?}"), local, global)?;
        *nodes += 1;
        let local = crate::billboard::authored_transform(local, billboard);
        let relative_to_root = parent_to_root * local.compute_affine();
        if let Ok((mesh_handle, _, _, _, _)) = primitives.get(entity) {
            let mesh = meshes.get(mesh_handle).ok_or_else(|| {
                format!(
                    "mesh {:?} is absent while validating bounds",
                    mesh_handle.id()
                )
            })?;
            let aabb = mesh.compute_aabb().ok_or_else(|| {
                format!("mesh {:?} has no finite POSITION bounds", mesh_handle.id())
            })?;
            let center = Vec3::from(aabb.center);
            let half_extents = Vec3::from(aabb.half_extents);
            let transformed = InstanceBounds::transformed(
                center - half_extents,
                center + half_extents,
                Mat4::from(relative_to_root),
            );
            *actual_min = actual_min.min(transformed.min);
            *actual_max = actual_max.max(transformed.max);
            *bounded_meshes += 1;
        }
        if let Ok(kids) = children.get(entity) {
            stack.extend(kids.iter().rev().map(|child| (child, relative_to_root)));
        }
    }
    Ok(())
}

/// How many render primitives (`Mesh3d` entities) the spawned hierarchy under `root` holds, at any
/// depth. Zero is an empty scene - what the converter writes for an editor marker - and is read
/// from the asset, never from the model's name or path.

#[derive(Debug, Clone, Copy)]
struct SpawnedBounds {
    min: Vec3,
    max: Vec3,
    /// Hierarchy nodes visited, which the validation summary reports to the metrics.
    nodes: usize,
}

fn validate_transform(
    label: &str,
    local: &Transform,
    global: &GlobalTransform,
) -> Result<(), String> {
    let local_matrix = local.to_matrix();
    let global_matrix = global.to_matrix();
    if !local_matrix.is_finite() || !global_matrix.is_finite() {
        return Err(format!("{label} contains a non-finite transform"));
    }
    if local.scale.abs().min_element() <= 1.0e-6
        || local_matrix.determinant().abs() <= 1.0e-8
        || global_matrix.determinant().abs() <= 1.0e-8
    {
        return Err(format!(
            "{label} contains a singular scale or hierarchy transform"
        ));
    }
    let rotation_length = local.rotation.length();
    if !rotation_length.is_finite() || (rotation_length - 1.0).abs() > 1.0e-3 {
        return Err(format!("{label} contains a non-normalized rotation"));
    }
    Ok(())
}

fn matrix_max_difference(left: Mat4, right: Mat4) -> f32 {
    left.to_cols_array()
        .into_iter()
        .zip(right.to_cols_array())
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, f32::max)
}

fn validate_spawned_asset(
    root: Entity,
    children: &Query<&Children>,
    primitives: &RenderPrimitiveQuery,
    meshes: &Assets<Mesh>,
    materials: &Assets<StandardMaterial>,
    images: &Assets<Image>,
) -> Result<AssetValidationSummary, String> {
    let mut summary = AssetValidationSummary::default();
    for descendant in children.iter_descendants(root) {
        let Ok((mesh, material_handle, _, extras, native_source)) = primitives.get(descendant)
        else {
            continue;
        };
        if meshes.get(mesh).is_none() {
            return Err(format!(
                "mesh {:?} is absent after scene readiness",
                mesh.id()
            ));
        }
        summary.meshes += 1;
        let Some(material_handle) = material_handle
            .map(|handle| &handle.0)
            .or_else(|| native_source.map(|source| &source.0))
        else {
            if extras.is_some_and(has_explicit_material_exclusion) {
                summary.excluded_materials += 1;
                continue;
            }
            return Err(format!(
                "mesh entity {descendant:?} has no loaded material or explicit exclusion"
            ));
        };
        let material = materials.get(material_handle).ok_or_else(|| {
            format!(
                "material {:?} is absent after scene readiness",
                material_handle.id()
            )
        })?;
        summary.images += validate_standard_material(material, images)?;
        summary.materials += 1;
    }
    Ok(summary)
}

fn has_explicit_material_exclusion(extras: &GltfExtras) -> bool {
    serde_json::from_str::<serde_json::Value>(&extras.value)
        .ok()
        .and_then(|value| {
            value
                .pointer("/openSkyrim/materialExclusion")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .is_some()
}

pub(crate) fn validate_standard_material(
    material: &StandardMaterial,
    images: &Assets<Image>,
) -> Result<usize, String> {
    match material.alpha_mode {
        AlphaMode::Mask(cutoff) if cutoff.is_finite() && (0.0..=1.0).contains(&cutoff) => {}
        AlphaMode::Mask(cutoff) => return Err(format!("invalid alpha cutoff {cutoff}")),
        mode => return Err(format!("unsupported Skyrim material alpha mode {mode:?}")),
    }
    if material.double_sided != material.cull_mode.is_none() {
        return Err(format!(
            "inconsistent culling: double_sided={} cull_mode={:?}",
            material.double_sided, material.cull_mode
        ));
    }
    let emissive = material.emissive;
    if ![emissive.red, emissive.green, emissive.blue, emissive.alpha]
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.0)
    {
        return Err("emissive contains a non-finite or negative channel".to_owned());
    }

    let slots = [
        ("base_color", material.base_color_texture.as_ref(), true),
        ("emissive", material.emissive_texture.as_ref(), true),
        (
            "metallic_roughness",
            material.metallic_roughness_texture.as_ref(),
            false,
        ),
        ("normal", material.normal_map_texture.as_ref(), false),
        ("occlusion", material.occlusion_texture.as_ref(), false),
        ("specular", material.specular_texture.as_ref(), false),
        (
            "specular_tint",
            material.specular_tint_texture.as_ref(),
            true,
        ),
    ];
    let mut validated = 0usize;
    for (slot, handle, expects_srgb) in slots {
        let Some(handle) = handle else {
            continue;
        };
        let image = images
            .get(handle)
            .ok_or_else(|| format!("{slot} image {:?} is not loaded", handle.id()))?;
        let descriptor = &image.texture_descriptor;
        if descriptor.size.width == 0
            || descriptor.size.height == 0
            || descriptor.mip_level_count == 0
        {
            return Err(format!("{slot} image has invalid dimensions or mip levels"));
        }
        if descriptor.format.is_srgb() != expects_srgb {
            return Err(format!(
                "{slot} image color space mismatch: {:?}",
                descriptor.format
            ));
        }
        validate_image_sampler(slot, &image.sampler)?;
        validated += 1;
    }
    Ok(validated)
}

fn validate_image_sampler(slot: &str, sampler: &ImageSampler) -> Result<(), String> {
    let ImageSampler::Descriptor(descriptor) = sampler else {
        return Ok(());
    };
    if descriptor.anisotropy_clamp == 0
        || !descriptor.lod_min_clamp.is_finite()
        || !descriptor.lod_max_clamp.is_finite()
        || descriptor.lod_min_clamp > descriptor.lod_max_clamp
    {
        return Err(format!("{slot} image has an invalid sampler descriptor"));
    }
    if descriptor.anisotropy_clamp > 1
        && (descriptor.mag_filter != ImageFilterMode::Linear
            || descriptor.min_filter != ImageFilterMode::Linear
            || descriptor.mipmap_filter != ImageFilterMode::Linear)
    {
        return Err(format!(
            "{slot} image requests anisotropy without linear filtering"
        ));
    }
    Ok(())
}

fn error_chain(error: &(dyn StdError + 'static)) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = Some(error);
    while let Some(error) = current {
        chain.push(error.to_string());
        current = error.source();
    }
    chain
}

fn record_asset_failure(
    metrics: &mut StreamingMetrics,
    profiler: &mut ProfilingState,
    pending: &PendingAssetProfile,
    dependency_chain: Vec<String>,
    material_validation: bool,
) {
    metrics.asset_load_failures = metrics.asset_load_failures.saturating_add(1);
    if material_validation {
        metrics.material_validation_failures =
            metrics.material_validation_failures.saturating_add(1);
    }
    profiler.increment("assets/load_failures", 1);
    profiler.event(&pending.path, "asset_failed", None);
    let mut full_chain = vec![
        format!("REFR {:08X}", pending.form_id),
        format!("base record {:08X}", pending.base_form_id),
        pending.path.clone(),
    ];
    full_chain.extend(dependency_chain);
    error!(
        reference = format_args!("{:08X}", pending.form_id),
        base = format_args!("{:08X}", pending.base_form_id),
        cell = format_args!("{:08X}", pending.cell_id),
        path = %pending.path,
        chain = ?full_chain,
        "model, material, or image dependency failed strict validation"
    );
    metrics.asset_failures.push(AssetFailure {
        model_path: pending.path.clone(),
        reference_form_id: pending.form_id,
        base_form_id: pending.base_form_id,
        cell_id: pending.cell_id,
        dependency_chain: full_chain,
    });
}

fn hide_partial_scene(commands: &mut Commands, root: Entity, children: &Query<&Children>) {
    for descendant in children.iter_descendants(root) {
        commands.entity(descendant).try_insert(Visibility::Hidden);
    }
}

fn spawn_diagnostic_fallback(
    commands: &mut Commands,
    root: Entity,
    fallback: &mut DiagnosticFallbackAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh = fallback
        .mesh
        .get_or_insert_with(|| meshes.add(Cuboid::new(96.0, 96.0, 96.0)))
        .clone();
    let material = fallback
        .material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.0, 0.8),
                emissive: LinearRgba::new(8.0, 0.0, 5.0, 1.0),
                unlit: true,
                ..default()
            })
        })
        .clone();
    commands.entity(root).with_child((
        Name::new("DIAGNOSTIC ASSET FAILURE"),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
    ));
}

/// Where the south-west corner of exterior grid `grid` sits in render coordinates while `origin` is
/// the render origin: the one place the grid-to-render offset is written.
///
/// Creation's cell grid runs `x` east and `y` north, and the Bevy frame has `x` east and `-z`
/// north, so a grid step north is a step of `-CELL_SIZE` along `z`. The difference is taken in
/// whole cells before it is scaled, which keeps the answer exact however far from the origin the
/// grid is. Every other conversion is this one read a different way: a cell root's translation
/// ([`cell_translation`], [`reposition_cell_roots`]), a camera moved by whole cells (a rebase in
/// [`update_render_origin`]), and - with `grid` as the render origin and `origin` as zero - the
/// shift between the rebased frame and the absolute one that [`render_position`] takes off and
/// [`absolute_render_position`] puts back.
pub(crate) fn grid_translation(grid: IVec2, origin: IVec2) -> Vec3 {
    let offset = grid - origin;
    Vec3::new(
        offset.x as f32 * CELL_SIZE,
        0.0,
        -offset.y as f32 * CELL_SIZE,
    )
}

fn cell_translation(key: CellKey, origin: IVec2) -> Vec3 {
    match key {
        CellKey::Exterior { grid_x, grid_y, .. } => {
            grid_translation(IVec2::new(grid_x, grid_y), origin)
        }
        CellKey::Interior(_) => Vec3::ZERO,
    }
}

pub(crate) fn creation_to_bevy(position: Vec3) -> Vec3 {
    Vec3::from_array(shared::coordinates::creation_to_runtime_vector(
        position.to_array(),
    ))
}

/// Where a Creation-space point lands in render coordinates while `origin` is the render origin:
/// the position [`spawn_cell`] gives an exterior reference of that point, and the local position
/// a camera has to take to stand on it.
///
/// An interior cell root carries no grid offset, so an interior reference at the same point
/// renders at [`creation_to_bevy`] of it instead.
pub(crate) fn render_position(creation_position: Vec3, origin: IVec2) -> Vec3 {
    creation_to_bevy(creation_position) - grid_translation(origin, IVec2::ZERO)
}

/// The inverse of [`render_position`]'s rebase: a render-space point of an exterior, rendered
/// against `origin`, back in the absolute Bevy frame - the one [`creation_to_bevy`] maps Creation
/// into, where `x` is Creation's `x` and `-z` is Creation's `y`.
///
/// This is how a camera's render position becomes a Creation position again
/// (`crate::pose_capture::creation_position`) and a grid ([`camera_grid`]). An interior carries no
/// grid offset, so for an interior pass the zero origin, which leaves the point as it is.
pub(crate) fn absolute_render_position(render_position: Vec3, origin: IVec2) -> Vec3 {
    render_position + grid_translation(origin, IVec2::ZERO)
}

/// The centre of the stream window: the exterior grid the camera is over, which is what
/// [`plan_cells`] streams around. The pacing report's world-ready scan reads the same square.
pub(crate) fn window_center(config: &EngineConfig, translation: Vec3, origin: IVec2) -> IVec2 {
    if config.acceptance_screenshot.is_some() {
        origin
    } else {
        streaming_center(translation, origin)
    }
}

/// The exterior grid a render-space point stands in while `origin` is the render origin: the cell
/// [`plan_cells`] streams around a camera there.
pub(crate) fn streaming_center(translation: Vec3, origin: IVec2) -> IVec2 {
    camera_grid(translation, origin)
}

pub(crate) fn camera_grid(render_position: Vec3, origin: IVec2) -> IVec2 {
    let absolute = absolute_render_position(render_position, origin);
    IVec2::new(
        (absolute.x / CELL_SIZE).floor() as i32,
        (-absolute.z / CELL_SIZE).floor() as i32,
    )
}

pub(crate) fn creation_rotation_to_bevy(rotation: [f32; 3]) -> Quat {
    Quat::from_array(shared::coordinates::creation_euler_to_runtime_quaternion(
        rotation,
    ))
}

/// The converted `.glb` a database model path names, or `None` for a marker, sky or effects model
/// and for a path that could leave `meshes/`. Lowercased throughout like main's resolver and like
/// the converter's published names (`crates/converter/src/pipeline.rs`); world-inspect calls this
/// too, so its model counts match the engine's.
pub fn converted_model_path(path: String) -> Option<String> {
    let lowercase = path.replace('\\', "/").to_ascii_lowercase();
    let filename = lowercase.rsplit('/').next().unwrap_or_default();
    if lowercase.starts_with("meshes/sky/")
        || lowercase.starts_with("sky/")
        || lowercase.starts_with("meshes/markers/")
        || lowercase.starts_with("markers/")
        || lowercase.starts_with("meshes/effects/")
        || lowercase.starts_with("effects/")
        || filename.contains("marker")
    {
        return None;
    }
    let without_prefix = lowercase.strip_prefix("meshes/").unwrap_or(&lowercase);
    if without_prefix.is_empty() || !is_safe_relative_asset_path(without_prefix) {
        return None;
    }
    let mut converted = std::path::PathBuf::from("meshes").join(without_prefix);
    converted.set_extension("glb");
    Some(converted.to_string_lossy().replace('\\', "/"))
}

fn terrain_collider_from_mesh(mesh: &Mesh) -> Result<Collider, String> {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return Err("terrain mesh lacks Float32x3 positions".to_owned());
    };
    let Some(Indices::U32(indices)) = mesh.indices() else {
        return Err("terrain mesh lacks U32 triangle indices".to_owned());
    };
    if indices.len() % 3 != 0 {
        return Err("terrain mesh triangle indices are incomplete".to_owned());
    }
    let vertices = positions.iter().copied().map(Vec3::from_array).collect();
    let triangles = indices.as_chunks::<3>().0.to_vec();
    Collider::trimesh(vertices, triangles)
        .map_err(|error| format!("invalid terrain trimesh: {error}"))
}

fn static_proxy_eligible(record_type: Option<&str>, path: &str) -> bool {
    let pine = path.starts_with("meshes/landscape/trees/treepineforest");
    match record_type {
        Some("TREE") => pine,
        Some("STAT") => {
            path.starts_with("meshes/landscape/rocks/")
                || path.starts_with("meshes/architecture/")
                || pine
                || path.starts_with("meshes/clutter/firewood/firewoodpile")
                || (path.starts_with("meshes/landscape/roads/road") && path.contains("ramp"))
        }
        _ => false,
    }
}

fn fixed_collision_record_eligible(record_type: Option<&str>) -> bool {
    matches!(record_type, Some("STAT" | "TREE" | "FURN"))
}

fn dynamic_clutter_record_eligible(record_type: Option<&str>) -> bool {
    matches!(
        record_type,
        Some(
            "MISC" | "WEAP" | "ARMO" | "BOOK" | "AMMO" | "ALCH" | "INGR" | "SLGM" | "KEYM" | "SCRL"
        )
    )
}

fn static_proxy_material_allowed(
    path: &str,
    material_name: Option<&str>,
    material: &StandardMaterial,
) -> bool {
    matches!(material.alpha_mode, AlphaMode::Opaque)
        // Riverwood RockCliff GLBs put the large rock faces in BLEND primitives. Their masked
        // detail primitives stay excluded, as do blended materials on unrelated models.
        || (path.starts_with("meshes/landscape/rocks/rockcliff")
            && matches!(material.alpha_mode, AlphaMode::Blend))
        // The lumbermill's walkable ramp/boards are woodwalkway01 with MASK alpha. Rope and
        // roof cutouts remain excluded; the material name comes from the converted GLB.
        || (path == "meshes/architecture/farmhouse/lumbermill01.glb"
            && material_name.is_some_and(|name| name.starts_with("LumbermillMesh:19"))
            && matches!(material.alpha_mode, AlphaMode::Mask(_)))
}

fn static_proxy_from_hierarchy(
    path: &str,
    root: Entity,
    children: &Query<&Children>,
    transforms: &SpawnedNodeQuery,
    primitives: &RenderPrimitiveQuery,
    meshes: &Assets<Mesh>,
    materials: &Assets<StandardMaterial>,
) -> Result<Option<Collider>, String> {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut stack = Vec::new();
    if let Ok(kids) = children.get(root) {
        stack.extend(kids.iter().map(|child| (child, Affine3A::IDENTITY)));
    }
    while let Some((entity, parent_to_root)) = stack.pop() {
        let (local, _, _) = transforms
            .get(entity)
            .map_err(|_| format!("static proxy node {entity:?} has no transform"))?;
        let node_to_root = parent_to_root * local.compute_affine();
        if let Ok((mesh_handle, material_handle, material_name, extras, native_source)) =
            primitives.get(entity)
            && !extras.is_some_and(has_explicit_material_exclusion)
        {
            let material = material_handle
                .map(|handle| &handle.0)
                .or_else(|| native_source.map(|source| &source.0))
                .and_then(|handle| materials.get(handle));
            if material.is_some_and(|material| {
                static_proxy_material_allowed(
                    path,
                    material_name.map(|name| name.0.as_str()),
                    material,
                )
            }) {
                let mesh = meshes.get(mesh_handle).ok_or_else(|| {
                    format!("static proxy mesh {:?} is missing", mesh_handle.id())
                })?;
                if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
                    return Err("static proxy mesh is not a triangle list".to_owned());
                }
                let Some(VertexAttributeValues::Float32x3(positions)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    return Err("static proxy mesh lacks Float32x3 positions".to_owned());
                };
                let indices: Vec<u32> = match mesh.indices() {
                    Some(Indices::U16(indices)) => {
                        indices.iter().map(|index| u32::from(*index)).collect()
                    }
                    Some(Indices::U32(indices)) => indices.clone(),
                    None => return Err("static proxy mesh lacks triangle indices".to_owned()),
                };
                if !indices.len().is_multiple_of(3) {
                    return Err("static proxy mesh has incomplete triangles".to_owned());
                }
                let offset = u32::try_from(vertices.len())
                    .map_err(|_| "static proxy vertex count exceeds u32".to_owned())?;
                for position in positions {
                    let position = node_to_root.transform_point3(Vec3::from_array(*position));
                    if !position.is_finite() {
                        return Err("static proxy has a non-finite vertex".to_owned());
                    }
                    vertices.push(position);
                }
                for index in indices.as_chunks::<3>().0 {
                    if index.iter().any(|index| *index as usize >= positions.len()) {
                        return Err("static proxy triangle index is out of range".to_owned());
                    }
                    triangles.push([offset + index[0], offset + index[1], offset + index[2]]);
                }
            }
        }
        if let Ok(kids) = children.get(entity) {
            stack.extend(kids.iter().map(|child| (child, node_to_root)));
        }
    }
    if triangles.is_empty() {
        return Ok(None);
    }
    Collider::trimesh(vertices, triangles)
        .map(Some)
        .map_err(|error| format!("invalid static proxy trimesh: {error}"))
}

fn drop_malformed_bodies(collision: &mut serde_json::Value) {
    let Some(object) = collision.as_object_mut() else {
        return;
    };
    // Only an array is filtered; anything else is left for the parse to report as it did before.
    if !object
        .get("bodies")
        .is_some_and(serde_json::Value::is_array)
    {
        return;
    }
    let Some(serde_json::Value::Array(entries)) = object.remove("bodies") else {
        return;
    };
    let mut kept = Vec::with_capacity(entries.len());
    let mut notes = Vec::new();
    for (index, entry) in entries.into_iter().enumerate() {
        match serde_json::from_value::<CollisionBody>(entry.clone()) {
            Ok(_) => kept.push(entry),
            Err(error) => notes.push(serde_json::Value::String(format!(
                "rigid body {index} is malformed: {error}"
            ))),
        }
    }
    object.insert("bodies".to_owned(), serde_json::Value::Array(kept));
    if !notes.is_empty() {
        let skipped = object
            .entry("skipped")
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        if let Some(skipped) = skipped.as_array_mut() {
            skipped.extend(notes);
        }
    }
}

fn authored_collision_from_hierarchy(
    root: Entity,
    children: &Query<&Children>,
    extras: &Query<&GltfSceneExtras>,
) -> Result<Option<CollisionAsset>, String> {
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Ok(scene_extras) = extras.get(entity) {
            if scene_extras.value.len() > 64 * 1024 * 1024 {
                return Err("GLB collision extras exceed 64 MiB".to_owned());
            }
            let value: serde_json::Value = serde_json::from_str(&scene_extras.value)
                .map_err(|error| format!("invalid GLB scene extras: {error}"))?;
            let collision = value
                .get("mudcrabCollision")
                .or_else(|| value.get("openSkyrimCollision"))
                .cloned();
            if let Some(mut collision) = collision {
                drop_malformed_bodies(&mut collision);
                let asset: CollisionAsset = serde_json::from_value(collision)
                    .map_err(|error| format!("invalid GLB collision data: {error}"))?;
                if asset.version == 0 || asset.version > COLLISION_ASSET_VERSION || !asset.authored
                {
                    return Err("unsupported GLB collision contract".to_owned());
                }
                return Ok(Some(asset));
            }
        }
        if let Ok(kids) = children.get(entity) {
            stack.extend(kids.iter());
        }
    }
    Ok(None)
}

fn collider_parts_from_authored(
    asset: &CollisionAsset,
) -> Result<Vec<AuthoredColliderPart>, String> {
    asset.shapes.iter().map(collider_part_from_shape).collect()
}

fn collider_part_from_shape(shape: &CollisionShape) -> Result<AuthoredColliderPart, String> {
    let (translation, collider) = {
        match shape {
            CollisionShape::Mesh {
                vertices,
                triangles,
            } => {
                if vertices.len() > 1_000_000 || triangles.len() > 2_000_000 {
                    return Err("authored mesh exceeds safety limit".to_owned());
                }
                if vertices.iter().flatten().any(|value| !value.is_finite())
                    || triangles
                        .iter()
                        .flatten()
                        .any(|index| *index as usize >= vertices.len())
                {
                    return Err("authored mesh contains invalid vertices or indices".to_owned());
                }
                let collider = Collider::trimesh(
                    vertices.iter().copied().map(Vec3::from_array).collect(),
                    triangles.clone(),
                )
                .map_err(|error| format!("invalid authored trimesh: {error}"))?;
                (Vec3::ZERO, collider)
            }
            CollisionShape::Capsule { a, b, radius } => {
                if !a.iter().chain(b).all(|value| value.is_finite())
                    || !radius.is_finite()
                    || *radius <= 0.0
                {
                    return Err("invalid authored capsule".to_owned());
                }
                (
                    Vec3::ZERO,
                    Collider::capsule(Vec3::from_array(*a), Vec3::from_array(*b), *radius),
                )
            }
            CollisionShape::Box {
                center,
                half_extents,
            } => {
                if !center
                    .iter()
                    .chain(half_extents)
                    .all(|value| value.is_finite())
                    || half_extents.iter().any(|value| *value <= 0.0)
                {
                    return Err("invalid authored box".to_owned());
                }
                (
                    Vec3::from_array(*center),
                    Collider::cuboid(half_extents[0], half_extents[1], half_extents[2]),
                )
            }
            CollisionShape::Hull { points } => {
                if points.len() > 1_000_000
                    || points.iter().flatten().any(|value| !value.is_finite())
                {
                    return Err("invalid authored hull".to_owned());
                }
                let collider = Collider::convex_hull(
                    &points
                        .iter()
                        .copied()
                        .map(Vec3::from_array)
                        .collect::<Vec<_>>(),
                )
                .ok_or_else(|| "invalid authored convex hull".to_owned())?;
                (Vec3::ZERO, collider)
            }
        }
    };
    Ok(AuthoredColliderPart {
        translation,
        collider,
    })
}

fn attach_authored_collision(
    commands: &mut Commands,
    entity: Entity,
    parts: &[AuthoredColliderPart],
) {
    if let [
        AuthoredColliderPart {
            translation,
            collider,
        },
    ] = parts
        && *translation == Vec3::ZERO
    {
        commands.entity(entity).insert((
            StaticColliderSource::NifAuthored,
            RigidBody::Fixed,
            collider.clone(),
            crate::physics::world_collision_groups(),
        ));
        return;
    }
    commands
        .entity(entity)
        .insert((StaticColliderSource::NifAuthored, RigidBody::Fixed));
    for part in parts {
        commands.spawn((
            part.collider.clone(),
            Transform::from_translation(part.translation),
            crate::physics::world_collision_groups(),
            ChildOf(entity),
        ));
    }
}

fn for_each_shape_hull_point(shape: &CollisionShape, mut emit: impl FnMut(Vec3)) {
    match shape {
        CollisionShape::Mesh { vertices, .. } => {
            for vertex in vertices {
                emit(Vec3::from_array(*vertex));
            }
        }
        CollisionShape::Hull { points: hull } => {
            for point in hull {
                emit(Vec3::from_array(*point));
            }
        }
        CollisionShape::Box {
            center,
            half_extents,
        } => {
            let center = Vec3::from_array(*center);
            let half = Vec3::from_array(*half_extents);
            for corner in 0..8u8 {
                let sign = Vec3::new(
                    if corner & 1 == 0 { -1.0 } else { 1.0 },
                    if corner & 2 == 0 { -1.0 } else { 1.0 },
                    if corner & 4 == 0 { -1.0 } else { 1.0 },
                );
                emit(center + sign * half);
            }
        }
        CollisionShape::Capsule { a, b, radius } => {
            // The 13 directions to a cube's neighbours (3 axes, 6 face diagonals, 4 corner
            // diagonals), each used as + and -: 26 points per end. The 6 axis points alone make an
            // octahedron, whose surface sits up to ~42% of the radius inside the capsule.
            const FACE: f32 = std::f32::consts::FRAC_1_SQRT_2;
            const CORNER: f32 = 0.577_350_26;
            const DIRECTIONS: [Vec3; 13] = [
                Vec3::X,
                Vec3::Y,
                Vec3::Z,
                Vec3::new(FACE, FACE, 0.0),
                Vec3::new(FACE, -FACE, 0.0),
                Vec3::new(FACE, 0.0, FACE),
                Vec3::new(FACE, 0.0, -FACE),
                Vec3::new(0.0, FACE, FACE),
                Vec3::new(0.0, FACE, -FACE),
                Vec3::new(CORNER, CORNER, CORNER),
                Vec3::new(CORNER, CORNER, -CORNER),
                Vec3::new(CORNER, -CORNER, CORNER),
                Vec3::new(-CORNER, CORNER, CORNER),
            ];
            for end in [Vec3::from_array(*a), Vec3::from_array(*b)] {
                for direction in DIRECTIONS {
                    emit(end + direction * *radius);
                    emit(end - direction * *radius);
                }
            }
        }
    }
}

/// Most points one dynamic body may contribute after duplicate shape indices are removed.
const MAX_CLUTTER_HULL_POINTS: usize = 1_000_000;
/// Most collider parts one dynamic body may spawn. The point budget alone would permit
/// 125,000 distinct boxes; this cap bounds collider entities independently of geometry size.
const MAX_CLUTTER_COLLIDER_PARTS: usize = 256;

fn shape_hull_points(shape: &CollisionShape, points: &mut Vec<Vec3>) {
    for_each_shape_hull_point(shape, |point| points.push(point));
}

fn shape_hull_point_count(shape: &CollisionShape) -> usize {
    let mut count = 0usize;
    for_each_shape_hull_point(shape, |_| count += 1);
    count
}

fn clutter_hull_point_total(shapes: &[&CollisionShape], budget: usize) -> Option<usize> {
    let mut total = 0usize;
    for shape in shapes {
        total = total.checked_add(shape_hull_point_count(shape))?;
        if total > budget {
            return None;
        }
    }
    Some(total)
}

fn clutter_decision_from_asset(asset: &CollisionAsset) -> StaticCollisionDecision {
    clutter_decision_from_asset_within(asset, MAX_CLUTTER_HULL_POINTS)
}

fn clutter_decision_from_asset_within(
    asset: &CollisionAsset,
    point_budget: usize,
) -> StaticCollisionDecision {
    let mut dynamic = asset
        .bodies
        .iter()
        .filter(|body| body.kind == BodyKind::Dynamic);
    let Some(body) = dynamic.next() else {
        return StaticCollisionDecision::ClutterNone;
    };
    let ignored_bodies = dynamic.count();
    let mut shapes = Vec::new();
    let mut seen = HashSet::new();
    for index in &body.shapes {
        // Repeated indices add no geometry, but duplicate colliders would multiply both
        // entities and density-derived mass. Keep the first occurrence's order.
        if !seen.insert(*index) {
            continue;
        }
        match asset.shapes.get(*index as usize) {
            Some(shape) => shapes.push(shape),
            None => {
                return StaticCollisionDecision::ClutterSkipped(
                    "dynamic body lists a shape that does not exist".to_owned(),
                );
            }
        }
    }
    if shapes.is_empty() {
        return StaticCollisionDecision::ClutterSkipped("dynamic body has no shapes".to_owned());
    }
    let convexify = !body.convex
        || shapes
            .iter()
            .any(|shape| matches!(shape, CollisionShape::Mesh { .. }));
    if !convexify && shapes.len() > MAX_CLUTTER_COLLIDER_PARTS {
        return StaticCollisionDecision::ClutterSkipped(
            "dynamic body has too many collider parts".to_owned(),
        );
    }
    // Check the total distinct geometry before copying points or building any collider. Both
    // branches keep the point budget; the convex-primitive branch also has a separate part cap.
    let Some(point_count) = clutter_hull_point_total(&shapes, point_budget) else {
        return StaticCollisionDecision::ClutterSkipped(
            "dynamic body has too many hull points".to_owned(),
        );
    };
    let parts = if convexify {
        let mut points = Vec::with_capacity(point_count);
        for shape in &shapes {
            shape_hull_points(shape, &mut points);
        }
        if points.iter().any(|point| !point.is_finite()) {
            return StaticCollisionDecision::ClutterSkipped(
                "dynamic body has invalid hull points".to_owned(),
            );
        }
        match Collider::convex_hull(&points) {
            Some(collider) => vec![AuthoredColliderPart {
                translation: Vec3::ZERO,
                collider,
            }],
            None => {
                return StaticCollisionDecision::ClutterSkipped(
                    "no convex hull for the dynamic body".to_owned(),
                );
            }
        }
    } else {
        match shapes
            .iter()
            .map(|shape| collider_part_from_shape(shape))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(parts) => parts,
            Err(reason) => return StaticCollisionDecision::ClutterSkipped(reason),
        }
    };
    StaticCollisionDecision::Clutter(Box::new(ClutterSpec {
        parts,
        mass: crate::physics::mass_properties_from_body(body),
        body: body.clone(),
        convexified: convexify,
        ignored_bodies,
    }))
}

fn clutter_collision_from_hierarchy(
    root: Entity,
    children: &Query<&Children>,
    scene_extras: &Query<&GltfSceneExtras>,
) -> StaticCollisionDecision {
    match authored_collision_from_hierarchy(root, children, scene_extras) {
        Ok(Some(asset)) => clutter_decision_from_asset(&asset),
        Ok(None) => StaticCollisionDecision::ClutterNone,
        Err(reason) => StaticCollisionDecision::ClutterSkipped(reason),
    }
}

fn static_collision_from_hierarchy(
    record_type: Option<&str>,
    path: &str,
    root: Entity,
    children: &Query<&Children>,
    scene_extras: &Query<&GltfSceneExtras>,
    transforms: &SpawnedNodeQuery,
    primitives: &RenderPrimitiveQuery,
    meshes: &Assets<Mesh>,
    materials: &Assets<StandardMaterial>,
) -> StaticCollisionDecision {
    match authored_collision_from_hierarchy(root, children, scene_extras) {
        Ok(Some(asset)) => match collider_parts_from_authored(&asset) {
            Ok(parts) if !parts.is_empty() => {
                StaticCollisionDecision::Authored(parts, asset.skipped)
            }
            Ok(_) if asset.skipped.is_empty() => StaticCollisionDecision::AuthoredAbsent,
            Ok(_) => StaticCollisionDecision::Skipped(asset.skipped.join("; ")),
            Err(reason) => StaticCollisionDecision::Skipped(reason),
        },
        Ok(None) if static_proxy_eligible(record_type, path) => {
            match static_proxy_from_hierarchy(
                path, root, children, transforms, primitives, meshes, materials,
            ) {
                Ok(Some(collider)) => StaticCollisionDecision::RenderProxy(collider),
                Ok(None) => StaticCollisionDecision::Skipped(
                    "legacy render proxy has no eligible triangles".to_owned(),
                ),
                Err(reason) => StaticCollisionDecision::Skipped(reason),
            }
        }
        Ok(None) => StaticCollisionDecision::LegacyExcluded,
        Err(reason) => StaticCollisionDecision::Skipped(reason),
    }
}

pub(crate) fn quadrant_layers(
    terrain: &TerrainSnapshot,
    quadrant: u8,
) -> Result<Vec<&TerrainLayerSnapshot>, String> {
    let mut layers: Vec<_> = terrain
        .layers
        .iter()
        .filter(|layer| layer.quadrant == quadrant)
        .collect();
    layers.sort_by_key(|layer| (!layer.is_base, layer.layer, layer.texture_form_id));
    if layers.is_empty() {
        return Ok(layers);
    }
    let base_count = layers.iter().filter(|layer| layer.is_base).count();
    if base_count != 1 {
        return Err(format!(
            "LAND {:08X} quadrant {quadrant} has {base_count} base layers; expected one",
            terrain.cell_id
        ));
    }
    if layers.len() > 6 {
        return Err(format!(
            "LAND {:08X} quadrant {quadrant} has {} layers; runtime supports six",
            terrain.cell_id,
            layers.len()
        ));
    }
    let mut layer_ids = HashSet::new();
    for layer in layers.iter().filter(|layer| !layer.is_base) {
        if !layer_ids.insert(layer.layer) {
            return Err(format!(
                "LAND {:08X} quadrant {quadrant} repeats ATXT layer {}",
                terrain.cell_id, layer.layer
            ));
        }
        let mut vertices = HashSet::new();
        for &(vertex, opacity) in &layer.weights {
            if usize::from(vertex) >= 17 * 17
                || !opacity.is_finite()
                || !(0.0..=1.0).contains(&opacity)
                || !vertices.insert(vertex)
            {
                return Err(format!(
                    "LAND {:08X} quadrant {quadrant} has invalid or duplicate VTXT data",
                    terrain.cell_id
                ));
            }
        }
    }
    Ok(layers)
}

/// The dense weight field of one quadrant: one `17x17` grid per overlay layer, indexed by the raw
/// `VTXT` vertex value, in the same order as [`quadrant_layers`] (base first, so slot 0 is the
/// first overlay). Unlisted grid points are opacity 0. Both the mesh's packed vertex weights and
/// the material's weight images are built from this, so they cannot drift apart.
pub(crate) fn quadrant_overlay_weights(
    terrain: &TerrainSnapshot,
    quadrant: u8,
) -> Result<Vec<Vec<f32>>, String> {
    let layers = quadrant_layers(terrain, quadrant)?;
    let samples = QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES;
    let mut overlay_weights = vec![vec![0.0f32; samples]; layers.len().saturating_sub(1)];
    for (slot, layer) in layers.iter().skip(1).enumerate() {
        for &(vertex, opacity) in &layer.weights {
            overlay_weights[slot][usize::from(vertex)] = opacity;
        }
    }
    Ok(overlay_weights)
}

/// Validates terrain geometry and nonzero image dependencies. Normalized zero-ID layers retain
/// their slots and weights without requiring a converted texture.
fn validate_terrain_snapshot(
    terrain: &TerrainSnapshot,
    catalog: &AssetCatalog,
) -> Result<(), String> {
    let width = usize::from(terrain.width);
    let height = usize::from(terrain.height);
    if width != 33 || height != 33 || terrain.heights.len() != width * height {
        return Err(format!(
            "terrain dimensions/data mismatch: {width}x{height} with {} heights",
            terrain.heights.len()
        ));
    }
    if terrain.heights.iter().any(|height| !height.is_finite()) {
        return Err("terrain contains a non-finite height".to_owned());
    }
    if terrain.normals.len() != width * height * 3 {
        return Err(format!(
            "terrain has {} packed normal bytes",
            terrain.normals.len()
        ));
    }
    if terrain
        .normals
        .as_chunks::<3>()
        .0
        .iter()
        .any(|normal| normal == &[0, 0, 0])
    {
        return Err("terrain contains a zero-length packed normal".to_owned());
    }
    if !terrain.vertex_colors.is_empty() && terrain.vertex_colors.len() != width * height * 3 {
        return Err(format!(
            "terrain has {} packed vertex-color bytes",
            terrain.vertex_colors.len()
        ));
    }
    for quadrant in 0..4 {
        for layer in quadrant_layers(terrain, quadrant)? {
            if layer.texture_form_id == 0 {
                continue;
            }
            if catalog.landscape_diffuse(layer.texture_form_id).is_none() {
                return Err(format!(
                    "quadrant {quadrant} texture {:08X} has no converted diffuse image",
                    layer.texture_form_id
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn build_terrain_quadrant_mesh(
    terrain: &TerrainSnapshot,
    quadrant: u8,
) -> Result<Mesh, String> {
    let overlay_weights = quadrant_overlay_weights(terrain, quadrant)?;
    let width = usize::from(terrain.width);
    let height = usize::from(terrain.height);
    if width != 33 || height != 33 || terrain.heights.len() != width * height {
        return Err("terrain must contain a complete 33x33 height field".to_owned());
    }
    let step_x = CELL_SIZE / (width - 1) as f32;
    let step_z = CELL_SIZE / (height - 1) as f32;
    let origin_x = usize::from(quadrant % 2) * 16;
    let origin_y = usize::from(quadrant / 2) * 16;
    let mut positions = Vec::with_capacity(17 * 17);
    let mut normals = Vec::with_capacity(17 * 17);
    let mut uvs = Vec::with_capacity(17 * 17);
    let mut extra_weights = Vec::with_capacity(17 * 17);
    let mut packed_weights = Vec::with_capacity(17 * 17);
    let mut colors = Vec::with_capacity(17 * 17);
    for local_y in 0..17 {
        for local_x in 0..17 {
            let x = origin_x + local_x;
            let y = origin_y + local_y;
            let index = y * width + x;
            let local = local_y * 17 + local_x;
            positions.push([
                x as f32 * step_x,
                terrain.heights[index],
                -(y as f32 * step_z),
            ]);
            normals.push(
                Vec3::new(
                    terrain.normals[index * 3] as f32,
                    terrain.normals[index * 3 + 2] as f32,
                    -(terrain.normals[index * 3 + 1] as f32),
                )
                .normalize_or(Vec3::Y)
                .to_array(),
            );
            uvs.push([
                x as f32 / (width - 1) as f32,
                y as f32 / (height - 1) as f32,
            ]);
            let weight = |slot: usize| {
                overlay_weights
                    .get(slot)
                    .map_or(0.0, |values| values[local])
            };
            // The packed vertex weights are the fallback for materials with no weight field (the
            // synthetic fixtures, which are built without a LAND snapshot): weights 1-3 as a unit
            // direction plus its magnitude in `w`, weights 4-5 in the second UV set. Bevy
            // re-normalizes `world_tangent.xyz` in the vertex shader, so this carrier sharpens
            // every transition (`0.25` where the true interpolated weight is `0.5`); the streamed
            // path reads `TerrainExtension`'s weight field instead.
            let first = Vec3::new(weight(0), weight(1), weight(2));
            let length = first.length();
            packed_weights.push(if length > 0.0 {
                let normalized = first / length;
                [normalized.x, normalized.y, normalized.z, length]
            } else {
                [0.0; 4]
            });
            extra_weights.push([weight(3), weight(4)]);
            colors.push(if terrain.vertex_colors.is_empty() {
                [1.0; 4]
            } else {
                [
                    terrain.vertex_colors[index * 3] as f32 / 255.0,
                    terrain.vertex_colors[index * 3 + 1] as f32 / 255.0,
                    terrain.vertex_colors[index * 3 + 2] as f32 / 255.0,
                    1.0,
                ]
            });
        }
    }
    let mut indices = Vec::with_capacity(16 * 16 * 6);
    for y in 0..16 {
        for x in 0..16 {
            let a = (y * 17 + x) as u32;
            let b = a + 1;
            let c = a + 17;
            let d = c + 1;
            indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, extra_weights);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, packed_weights);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    Ok(mesh)
}

/// The largest difference between an arriving cell's edge and a resident neighbour's that is
/// still treated as a seam to weld.
///
/// Real seams are small: the worst measured on `Skyrim.esm` is 24 units on one of 33 points of
/// Tamriel's (18,18) north edge against (18,19), and 16 units on one of (18,20)'s east edge -
/// across a sample step of 128 units. A larger difference is not a crack but two landscapes that
/// were authored apart - the city worldspaces' sculpted cells against the copies of Tamriel's land
/// beside them (Markarth 520-2520 units, Solitude 560-2352; Phase 2's research-552) - which Skyrim
/// draws as authored, so that edge is left alone.
const MAX_WELDABLE_EDGE_DELTA: f32 = 512.0;

/// Edge heights closer than this are already the same point: the tolerance the strict comparison
/// used before edges were welded.
const EDGE_MATCH_TOLERANCE: f32 = 0.01;

/// Registers a cell's edge heights, welding the arriving cell onto the neighbours already drawn.
///
/// The resident neighbour is authoritative: its mesh is in the world, so where the two disagree
/// the arriving cell moves. This replaces the strict comparison that rejected the whole cell -
/// and with it the terrain and its references - over a single point of one edge, which is what
/// leaves a hole in the ground on real `Skyrim.esm` data. An edge that differs by more than
/// [`MAX_WELDABLE_EDGE_DELTA`] is not welded and not rejected either: it is drawn as authored and
/// counted, as Skyrim draws every `LAND` as its own mesh without comparing neighbours. What is
/// still rejected is data that cannot be drawn: an edge of a different length and a non-finite
/// height on either side.
///
/// The welded heights - not the loaded ones - are what gets registered, so a cell arriving later
/// welds onto the surface that is actually drawn and the block stays watertight.
struct WeldableEdge {
    /// Which of the arriving cell's four sides this is.
    side: TerrainEdgeSide,
    /// The neighbour sharing `side`.
    neighbor: CellKey,
    /// The neighbour's registered heights along the shared edge, position for position with `side`.
    heights: Vec<f32>,
    /// The largest difference the weld has to close along the edge.
    max_delta: f32,
}

fn points_and_neighbours(terrain: &TerrainSnapshot, points: &[usize]) -> Vec<usize> {
    let width = usize::from(terrain.width);
    let height = usize::from(terrain.height);
    let mut all = Vec::with_capacity(points.len() * 5);
    for &index in points {
        let (x, y) = (index % width, index / width);
        all.push(index);
        if x > 0 {
            all.push(index - 1);
        }
        if x + 1 < width {
            all.push(index + 1);
        }
        if y > 0 {
            all.push(index - width);
        }
        if y + 1 < height {
            all.push(index + width);
        }
    }
    all.sort_unstable();
    all.dedup();
    all
}

fn validate_and_register_terrain_edges(
    key: CellKey,
    terrain: &mut TerrainSnapshot,
    continuity: &mut TerrainContinuity,
    metrics: &mut StreamingMetrics,
) -> Result<(), String> {
    let CellKey::Exterior {
        worldspace_id,
        grid_x,
        grid_y,
    } = key
    else {
        return Ok(());
    };
    let width = usize::from(terrain.width);
    let height = usize::from(terrain.height);
    // `validate_terrain_snapshot` checks these on the loading path, but the point indices below are
    // computed here, and a non-finite height would be welded into the neighbours' shared edges while
    // `f32::max` kept quiet about it, so the field this function relies on is checked here too.
    if width < 2 || height < 2 || terrain.heights.len() != width * height {
        return Err(format!(
            "terrain must hold a height field of at least 2x2: {width}x{height} with {} heights",
            terrain.heights.len()
        ));
    }
    if terrain.heights.iter().any(|height| !height.is_finite()) {
        return Err("terrain contains a non-finite height".to_owned());
    }
    let grid = IVec2::new(grid_x, grid_y);
    // Every shared edge is read before any height moves, so a rejected cell is left exactly as it
    // was loaded even when an earlier edge turned out to be weldable.
    let mut weldable = Vec::new();
    for side in TerrainEdgeSide::ALL {
        let neighbor_key = side.neighbor_key(worldspace_id, grid);
        let Some(neighbor) = continuity.edges.get(&neighbor_key) else {
            continue;
        };
        let other = neighbor.side(side.opposite());
        if side.len(terrain) != other.len() {
            return Err(format!(
                "terrain edge {side:?} has {} points; neighbor {neighbor_key:?} has {}",
                side.len(terrain),
                other.len()
            ));
        }
        if other.iter().any(|height| !height.is_finite()) {
            return Err(format!(
                "terrain edge {side:?} of neighbor {neighbor_key:?} is not finite"
            ));
        }
        let max_delta = (0..side.len(terrain))
            .map(|position| {
                (terrain.heights[side.index(terrain, position)] - other[position]).abs()
            })
            .fold(0.0_f32, f32::max);
        if max_delta > MAX_WELDABLE_EDGE_DELTA {
            // Two landscapes authored apart, not a crack: rejecting the cell dropped its terrain
            // and every reference on it, the holes in Markarth's and Solitude's ground.
            warn!(
                ?key,
                neighbor = ?neighbor_key,
                side = ?side,
                max_delta,
                "terrain edge differs from its neighbour past the weld bound; drawn as authored"
            );
            metrics.terrain_edges_left_as_authored =
                metrics.terrain_edges_left_as_authored.saturating_add(1);
            continue;
        }
        weldable.push(WeldableEdge {
            side,
            neighbor: neighbor_key,
            heights: other.to_vec(),
            max_delta,
        });
    }
    let mut welded_points = Vec::new();
    for edge in &weldable {
        // The points at the two ends of a side are its corners, welded once each below.
        let mut moved = 0u64;
        for position in 1..edge.side.len(terrain) - 1 {
            let index = edge.side.index(terrain, position);
            let height = edge.heights[position];
            if (terrain.heights[index] - height).abs() > EDGE_MATCH_TOLERANCE {
                welded_points.push(index);
                moved += 1;
            }
            terrain.heights[index] = height;
        }
        if moved > 0 {
            debug!(
                cell = format_args!("{:08X}", terrain.cell_id),
                neighbor = ?edge.neighbor,
                max_delta = edge.max_delta,
                moved,
                "LAND edge welded onto the resident neighbor"
            );
        }
        metrics.terrain_seams_validated = metrics.terrain_seams_validated.saturating_add(1);
    }
    // A corner point is the end of two sides, so welding it with both would write it twice - the
    // later side winning - and count it twice. The two neighbours meeting there can disagree as
    // well, since they share only that point and never an edge, and then the arriving corner cannot
    // agree with both of them. Each corner is therefore welded last and once, to the first of its
    // two sides that has a resident neighbour: the corner follows a single neighbour, and what is
    // left where the two disagree is the difference they already had between them, which no
    // arriving cell can close.
    let corners = [
        (0, TerrainEdgeSide::West, TerrainEdgeSide::South),
        (width - 1, TerrainEdgeSide::East, TerrainEdgeSide::South),
        (
            (height - 1) * width,
            TerrainEdgeSide::West,
            TerrainEdgeSide::North,
        ),
        (
            height * width - 1,
            TerrainEdgeSide::East,
            TerrainEdgeSide::North,
        ),
    ];
    for (index, first, second) in corners {
        // The corner's row along a vertical side and its column along a horizontal one: how the
        // point sits on each of the two sides.
        let corner = [first, second].into_iter().find_map(|side| {
            let edge = weldable.iter().find(|edge| edge.side == side)?;
            let position = match side {
                TerrainEdgeSide::West | TerrainEdgeSide::East => index / width,
                TerrainEdgeSide::South | TerrainEdgeSide::North => index % width,
            };
            Some((side, edge.heights[position]))
        });
        let Some((side, height)) = corner else {
            continue;
        };
        if (terrain.heights[index] - height).abs() > EDGE_MATCH_TOLERANCE {
            welded_points.push(index);
            terrain.heights[index] = height;
            debug!(
                cell = format_args!("{:08X}", terrain.cell_id),
                corner = index,
                side = ?side,
                height,
                "LAND corner welded onto the resident neighbor"
            );
        }
    }
    if !welded_points.is_empty() {
        // A moved point no longer lies where its stored normal was computed, and the drawn
        // triangle is what gets shaded, so recompute it from the welded field. Real seams have
        // the two sides' normals already matching, so keeping the loaded ones would shade the
        // boundary exactly like the neighbour at the cost of a normal that disagrees with our
        // own geometry. The normals of the points beside a moved one are computed from its
        // height too, so they are recomputed as well.
        recompute_packed_normals(terrain, &points_and_neighbours(terrain, &welded_points));
        metrics.terrain_seam_points_welded = metrics
            .terrain_seam_points_welded
            .saturating_add(welded_points.len() as u64);
    }
    continuity.edges.insert(key, TerrainEdges::of(terrain));
    Ok(())
}

/// Recomputes packed `VNML` bytes from the welded height field, normalized and scaled to `i8`,
/// with the converter's stencil (`crates/converter/src/esm/cell_cache.rs`, `decode_normals`):
/// each axis's height difference is divided by the distance it actually spans, two intervals
/// inside the field and one at an edge or corner, so a planar slope keeps the same normal
/// everywhere. `points` index a complete `width * height` field.
fn recompute_packed_normals(terrain: &mut TerrainSnapshot, points: &[usize]) {
    let width = usize::from(terrain.width);
    let height = usize::from(terrain.height);
    if width < 2 || height < 2 || terrain.normals.len() != width * height * 3 {
        return;
    }
    let step = CELL_SIZE / (width - 1) as f32;
    for &index in points {
        let (x, y) = (index % width, index / width);
        let (west, east) = (x.saturating_sub(1), (x + 1).min(width - 1));
        let (south, north) = (y.saturating_sub(1), (y + 1).min(height - 1));
        let left = terrain.heights[y * width + west];
        let right = terrain.heights[y * width + east];
        let down = terrain.heights[south * width + x];
        let up = terrain.heights[north * width + x];
        // `width` and `height` are at least 2, so every span covers at least one interval.
        let span_x = (east - west) as f32 * step;
        let span_y = (north - south) as f32 * step;
        let normal =
            Vec3::new((left - right) / span_x, (down - up) / span_y, 1.0).normalize_or(Vec3::Z);
        // A height field's own normal always has a positive up component, so the packed bytes can
        // never come out all zero - which is what the validation rejects.
        let byte = |component: f32| (component * 127.0).round().clamp(-127.0, 127.0) as i8;
        terrain.normals[index * 3..index * 3 + 3].copy_from_slice(&[
            byte(normal.x),
            byte(normal.y),
            byte(normal.z),
        ]);
    }
}

/// Every spawned root the render origin places: an exterior cell root ([`ExteriorCellGrid`]) or a
/// terrain LOD chunk root ([`lod::LodChunkGridOrigin`]). Interiors carry neither and sit at the
/// origin already, so they are not touched.
pub(crate) type CellRootTransforms<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static ExteriorCellGrid>,
        Option<&'static lod::LodChunkGridOrigin>,
        &'static mut Transform,
    ),
    (
        Or<(With<ExteriorCellGrid>, With<lod::LodChunkGridOrigin>)>,
        Without<StreamingCamera>,
        Without<crate::physics::PlayerBody>,
        Without<crate::physics::DebugTankard>,
    ),
>;

/// Places every spawned exterior cell root, and every terrain LOD chunk root, where `origin` says
/// it belongs ([`CellRootTransforms`]).
pub(crate) fn reposition_cell_roots(origin: IVec2, roots: &mut CellRootTransforms) {
    for (cell_grid, lod_grid, mut transform) in roots.iter_mut() {
        let Some(grid) = cell_grid
            .map(|grid| (i64::from(grid.0.x), i64::from(grid.0.y)))
            .or_else(|| lod_grid.map(|grid| (grid.grid_x, grid.grid_y)))
        else {
            continue;
        };
        transform.translation = Vec3::new(
            (grid.0 - i64::from(origin.x)) as f32 * CELL_SIZE,
            0.0,
            -((grid.1 - i64::from(origin.y)) as f32) * CELL_SIZE,
        );
    }
}

type OriginParticipantFilter = (
    Or<(
        With<crate::physics::PlayerBody>,
        With<crate::physics::DebugTankard>,
    )>,
    Without<StreamingCamera>,
);

#[allow(clippy::too_many_arguments)] // The collider queries belong to this pass, not the caller.
fn update_render_origin(
    config: Res<EngineConfig>,
    space: Res<ActiveSpace>,
    mut origin: ResMut<RenderOrigin>,
    mut camera: Query<&mut Transform, With<StreamingCamera>>,
    mut roots: Query<
        (
            Option<&ExteriorCellGrid>,
            Option<&lod::LodChunkGridOrigin>,
            &mut Transform,
        ),
        (
            Or<(With<ExteriorCellGrid>, With<lod::LodChunkGridOrigin>)>,
            Without<StreamingCamera>,
            Without<PlayerBody>,
            Without<DebugTankard>,
        ),
    >,
    mut participants: Query<
        (Entity, &mut Transform),
        (
            Or<(With<PlayerBody>, With<DebugTankard>)>,
            Without<StreamingCamera>,
        ),
    >,
    terrain: Query<Entity, With<TerrainCollider>>,
    static_colliders: Query<Entity, With<StaticColliderSource>>,
    clutter: Query<Entity, With<crate::physics::DynamicClutter>>,
    mut physics: WriteRapierContext,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    // Screenshot runs keep the start-cell origin fixed: rebasing toward an
    // artistically offset camera would drag the streamed window (and the
    // screenshot target's cells) away from the framed view.
    if config.acceptance_screenshot.is_some() {
        return;
    }
    // An interior is placed in absolute coordinates around the origin it was entered with, so the
    // origin stays put while the player is inside one; leaving it sets the origin afresh.
    if space.interior.is_some() {
        return;
    }
    let started = Instant::now();
    let Ok(mut camera) = camera.single_mut() else {
        return;
    };
    let shift = IVec2::new(
        (camera.translation.x / CELL_SIZE).trunc() as i32,
        (-camera.translation.z / CELL_SIZE).trunc() as i32,
    );
    if shift == IVec2::ZERO {
        return;
    }
    origin.0 += shift;
    let displacement = Vec3::new(
        shift.x as f32 * CELL_SIZE,
        0.0,
        -(shift.y as f32) * CELL_SIZE,
    );
    camera.translation -= displacement;
    let mut bodies = Vec::new();
    for (entity, mut transform) in &mut participants {
        transform.translation -= displacement;
        bodies.push(entity);
    }
    bodies.extend(terrain.iter());
    bodies.extend(static_colliders.iter());
    // Dynamic clutter is a cell-root descendant, so the root's move already shifts its Transform;
    // only its Rapier body is teleported here. Adding it to `participants` instead would shift the
    // Transform a second time and double the displacement. The teleport keeps its velocity.
    bodies.extend(clutter.iter());
    if let Ok(mut context) = physics.single_mut() {
        for entity in bodies {
            if let Some(handle) = context.entity2body().get(&entity).copied()
                && let Some(body) = context.rigidbody_set.bodies.get_mut(handle)
            {
                body.set_translation(body.translation() - displacement, false);
            }
        }
        context.propagate_modified_body_positions_to_colliders();
    }
    for (cell_grid, lod_grid, mut transform) in &mut roots {
        let Some(grid) = cell_grid
            .map(|grid| (i64::from(grid.0.x), i64::from(grid.0.y)))
            .or_else(|| lod_grid.map(|grid| (grid.grid_x, grid.grid_y)))
        else {
            continue;
        };
        transform.translation = Vec3::new(
            (grid.0 - i64::from(origin.0.x)) as f32 * CELL_SIZE,
            0.0,
            -((grid.1 - i64::from(origin.0.y)) as f32) * CELL_SIZE,
        );
    }
    profiler.increment("streaming/origin_rebases", 1);
    metrics.origin_rebases = metrics.origin_rebases.saturating_add(1);
    profiler.event(
        format!("{},{}", origin.0.x, origin.0.y),
        "origin_rebased",
        None,
    );
    profiler.record_elapsed("streaming/render_origin_rebase", started);
}

/// Frames a visible full-detail terrain quadrant may wait on the render world before the run-level
/// invariant counts it as a hole that came back (design-1091 Â§4).
///
/// Derived from what a legitimate wait is made of: the bridge's own latency is two render frames (a
/// surface is prepared in the extract after the frame that established its assets, and the hand-over
/// runs in the frame after that), and the upload budget
/// (`EngineConfig::max_upload_mib_per_frame`, 16 MiB a frame in play, none for a capture) defers a
/// quadrant's mesh and its LAND layers' images to later frames under one shared counter a cell's own
/// models also draw on. One full-detail stream window is `(2 * stream_radius + 1)^2` cells of four
/// quadrants - 100 quadrants at the default `stream_radius` of 2 - whose images are on the order of
/// the budget's frame, so a whole window turning over is on the order of a hundred frames. 512 is
/// several times that: generous enough that no healthy run, slow machine or heavy mod cell trips it,
/// and short enough to catch a bridge that has stopped confirming a visible quadrant within seconds
/// at any frame rate the engine runs at.
///
/// The bound is on the *permanent* condition (never drawn while visible), not the transient one (not
/// yet drawn): the invariant gates shots and acceptance, so a false positive would cost a run, and
/// the stall it exists to catch is not one that clears by waiting.
const MAX_SWAP_WAIT_FRAMES: u64 = 512;

/// Terrain quadrants still waiting on the render world.
type WaitingTerrainPatches = (With<TerrainPatch>, With<SwapWaiting>);

/// The terrain-patch and static-collider queries `validate_streaming_lifecycle` reads, grouped to
/// stay within Bevy's 16 system parameters.
#[derive(SystemParam)]
struct LifecycleQueries<'w, 's> {
    terrain_patches: Query<'w, 's, &'static ChildOf, With<TerrainPatch>>,
    static_colliders: Query<'w, 's, (), With<StaticColliderSource>>,
}

#[allow(clippy::too_many_arguments)]
fn validate_streaming_lifecycle(
    config: Res<EngineConfig>,
    database: Option<Res<WorldDatabase>>,
    active: Res<ActiveSpace>,
    origin: Res<RenderOrigin>,
    streaming: Res<StreamingWorld>,
    camera: Query<&Transform, With<StreamingCamera>>,
    roots: CellRootsQuery,
    full_exteriors: Query<Option<&TerrainLess>, (With<StreamedCellRoot>, With<ExteriorCellGrid>)>,
    // One grouped parameter: a Bevy system takes at most 16.
    lifecycle_queries: LifecycleQueries,
    // A visible terrain quadrant still waiting on the render world, and how long each has waited:
    // the run-level check below (design-1091 C4). The map holds only the patches waiting now, so a
    // drawn or despawned patch's entity id cannot be inherited by a later one.
    parents: Query<&ChildOf>,
    patches: Query<(Entity, Has<SwapInCameraView>), WaitingTerrainPatches>,
    mut wait_frame: Local<u64>,
    mut waiting_since: Local<HashMap<Entity, u64>>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    let LifecycleQueries {
        terrain_patches,
        static_colliders,
    } = lifecycle_queries;
    let active_requests = streaming
        .cells
        .values()
        .filter(|status| matches!(status, CellStatus::Loading { .. }))
        .count();
    // A retiring cell still owns its root until the unload budget reaches it, so its root is
    // accounted for here rather than reading as orphaned.
    let resident_entities: HashSet<_> = streaming
        .cells
        .values()
        .filter_map(|status| match status {
            CellStatus::Resident { root, .. } => Some(*root),
            CellStatus::Retiring { root, .. } => Some(*root),
            // A cell loading at another detail still draws the root it is replacing, and a failed
            // cell keeps the terrain root it failed upgrading from: roots the plan accounts for,
            // not orphans.
            CellStatus::Loading { replaced, .. } | CellStatus::Failed { replaced, .. } => *replaced,
        })
        .collect();
    let root_entries: Vec<_> = roots.iter().collect();
    let root_entities: HashSet<_> = root_entries.iter().map(|(entity, _, _)| *entity).collect();
    let mut roots_by_cell = HashMap::<u32, usize>::new();
    for (_, cell, _) in &root_entries {
        *roots_by_cell.entry(cell.0).or_default() += 1;
    }
    let duplicate_roots = roots_by_cell.values().filter(|count| **count > 1).count() as u64;
    let orphaned_roots = root_entities.difference(&resident_entities).count() as u64;
    let missing_roots = resident_entities.difference(&root_entities).count() as u64;
    // Out of range means "resident although the plan does not want it any more": an exterior of
    // the active worldspace the camera has left behind. Cells of the worldspace the camera came
    // from, and the destination pre-streamed behind a door, are legitimately outside that radius
    // -- while an interior is active there is no exterior plan at all.
    //
    // The radius is the wider of the two the plan keeps cells by, for a cell of either tier: a full
    // cell outside the inner grid is on its way to being terrain-only, and that is a *request* the
    // worker's queue may not have room for in the frame the camera moved (`plan_cells` asks again
    // next frame), so a full cell a few cells out is a cell the plan wants and has asked for, not
    // one it has stopped wanting. What this catches is a cell of another worldspace, a cell beyond
    // every band, and one the plan's own retain should have dropped.
    let out_of_range_roots = if active.interior.is_some() {
        0
    } else {
        // The same ring the plan keeps cells by this frame, resolved the same way: a check that
        // read the configured ring where the plan drew none would accept a cell the plan has
        // stopped wanting (`EngineConfig::effective_terrain_radius`).
        let has_lod = database.as_ref().is_some_and(|database| {
            database.has_lod_for(active.exterior_worldspace(config.worldspace_id))
        });
        let terrain_radius = config.effective_terrain_radius(has_lod);
        camera.single().map_or(0, |camera| {
            let center = window_center(&config, camera.translation, origin.0);
            let limit = config.unload_radius.max(terrain_radius.saturating_add(1));
            streaming
                .cells
                .iter()
                .filter(|(key, status)| {
                    let key = **key;
                    matches!(status, CellStatus::Resident { .. })
                        && matches!(key, CellKey::Exterior { worldspace_id, grid_x, grid_y }
                            if worldspace_id == active.exterior_worldspace(config.worldspace_id)
                                && ((grid_x - center.x).abs() > limit
                                    || (grid_y - center.y).abs() > limit))
                })
                .count() as u64
        })
    };
    // A resident full-detail exterior cell draws its landscape: its root holds a `TerrainPatch`
    // child unless the commit counted it as terrain-less ([`TerrainLess`]). One without either lost
    // its landscape after it was spawned, which nothing counted. Cost: one pass over the cell map
    // (as above) with a root lookup per resident full exterior cell, then - only while such a cell
    // exists - one pass over the `TerrainPatch` entities (about four per resident terrain cell) that
    // ticks their parents off, stopping as soon as every cell is accounted for. No pass over the
    // cells' children, which hold every reference of the cell.
    let mut unpatched: HashSet<Entity> = streaming
        .cells
        .iter()
        .filter_map(|(key, status)| match (key, status) {
            (
                CellKey::Exterior { .. },
                CellStatus::Resident {
                    root,
                    detail: CellDetail::Full,
                },
            ) => Some(*root),
            _ => None,
        })
        .filter(|root| matches!(full_exteriors.get(*root), Ok(None)))
        .collect();
    if !unpatched.is_empty() {
        for parent in &terrain_patches {
            unpatched.remove(&parent.parent());
            if unpatched.is_empty() {
                break;
            }
        }
    }
    let terrain_lost_roots = unpatched.len() as u64;
    // A visible full-detail terrain quadrant the render world has not confirmed drawn past
    // [`MAX_SWAP_WAIT_FRAMES`] is a hole that came back: the bridge has had the bridge's latency and
    // the upload budget's worst case and still cannot draw it. Counted only for the resident
    // full-detail exterior roots inside the stream window - the ground a camera settling on the
    // active worldspace can see - so a cell on its way out, another worldspace's cells, and the
    // terrain ring (which the plan does not keep in full detail) are not read as failures. (A
    // counted terrain-less cell ([`TerrainLess`]) has no `TerrainPatch` at all, so it is never
    // seen here and needs no exemption: the terrain-lost check above covers it.)
    *wait_frame = wait_frame.saturating_add(1);
    let swap_wait_timeouts = if active.interior.is_some() {
        // No exterior geometry is drawn inside an interior; nothing here can be a hole.
        waiting_since.clear();
        0
    } else {
        let full_window_roots: HashSet<Entity> = camera.single().map_or_else(
            |_| HashSet::new(),
            |camera| {
                let center = window_center(&config, camera.translation, origin.0);
                streaming
                    .cells
                    .iter()
                    .filter_map(|(key, status)| match (key, status) {
                        (
                            CellKey::Exterior {
                                worldspace_id,
                                grid_x,
                                grid_y,
                            },
                            CellStatus::Resident {
                                root,
                                detail: CellDetail::Full,
                            },
                        ) if *worldspace_id == active.exterior_worldspace(config.worldspace_id)
                            && (grid_x - center.x).abs() <= config.stream_radius
                            && (grid_y - center.y).abs() <= config.stream_radius =>
                        {
                            Some(*root)
                        }
                        _ => None,
                    })
                    .collect()
            },
        );
        let mut stuck = 0u64;
        for (entity, in_view) in patches.iter() {
            let visible_under_a_full_window_root = in_view
                && parents
                    .iter_ancestors(entity)
                    .any(|ancestor| full_window_roots.contains(&ancestor));
            if !visible_under_a_full_window_root {
                waiting_since.remove(&entity);
                continue;
            }
            let since = *waiting_since.entry(entity).or_insert(*wait_frame);
            if *wait_frame - since > MAX_SWAP_WAIT_FRAMES {
                stuck = stuck.saturating_add(1);
            }
        }
        // Forget the patches that drew (their marker is gone) or despawned, so the map covers the
        // waiting set and nothing else.
        waiting_since.retain(|entity, _| patches.contains(*entity));
        stuck
    };

    let violations = duplicate_roots
        + orphaned_roots
        + missing_roots
        + out_of_range_roots
        + terrain_lost_roots
        + swap_wait_timeouts;

    metrics.active_requests = active_requests;
    metrics.peak_active_requests = metrics.peak_active_requests.max(active_requests);
    metrics.resident_roots = root_entries.len();
    metrics.resident_static_colliders = static_colliders.iter().count();
    metrics.duplicate_cell_roots = metrics.duplicate_cell_roots.max(duplicate_roots);
    metrics.orphaned_cell_roots = metrics.orphaned_cell_roots.max(orphaned_roots);
    metrics.missing_cell_roots = metrics.missing_cell_roots.max(missing_roots);
    metrics.out_of_range_cell_roots = metrics.out_of_range_cell_roots.max(out_of_range_roots);
    if violations > metrics.streaming_invariant_failures {
        error!(
            duplicate_roots,
            orphaned_roots,
            missing_roots,
            out_of_range_roots,
            terrain_lost_roots,
            swap_wait_timeouts,
            "streaming lifecycle invariant failed"
        );
        profiler.event("streaming", "invariant_failed", None);
    }
    metrics.streaming_invariant_failures = metrics.streaming_invariant_failures.max(violations);
    profiler.set_gauge("streaming/active_requests", active_requests as f64);
    profiler.set_gauge("streaming/resident_roots", root_entries.len() as f64);
    profiler.set_gauge("streaming/swap_wait_timeouts", swap_wait_timeouts as f64);
}

impl TerrainCoverage {
    pub(crate) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({
            "cell": self.grid.to_array(),
            "quadrant": self.quadrant,
            "tier": format!("{:?}", self.tier)
        })
    }
}

#[cfg(test)]
pub(crate) fn test_plant_retiring_cell(world: &mut World, key: CellKey, root: Entity) {
    world.resource_mut::<StreamingWorld>().cells.insert(
        key,
        CellStatus::Retiring {
            root,
            detail: CellDetail::Full,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consume_mock_cell_answer(app: &mut App, key: CellKey) {
        let mut streaming = app.world_mut().resource_mut::<StreamingWorld>();
        if let Some(CellStatus::Loading {
            generation, detail, ..
        }) = streaming.cells.get(&key)
        {
            let (generation, detail) = (*generation, *detail);
            streaming.consume_cell_response(key, generation, detail);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_spawned_transforms_and_bounds(
        root: Entity,
        root_local: &Transform,
        root_global: &GlobalTransform,
        world_transform: &WorldTransform,
        expected: Option<&ExpectedModelBounds>,
        children: &Query<&Children>,
        transforms: &SpawnedNodeQuery,
        primitives: &RenderPrimitiveQuery,
        meshes: &Assets<Mesh>,
    ) -> Result<TransformValidationSummary, String> {
        super::validate_spawned_transforms_and_bounds(
            root,
            root_local,
            root_global,
            world_transform,
            expected,
            None,
            children,
            transforms,
            primitives,
            meshes,
        )
    }

    use crate::world::components::FormId;
    use crate::world::database::LightRow;
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::ecs::system::SystemState;

    /// Exercise the real readiness system. AssetServer::add queues a procedural image
    /// synchronously, so no filesystem, loader sleep or private fixture is involved.
    #[test]
    fn invalid_authored_normal_records_failure_instead_of_revealing_the_surface() {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<TerrainMaterial>()
            .init_resource::<EngineConfig>()
            .init_resource::<lod::LodStreaming>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, track_surface_readiness);
        // sRGB is invalid for a declared linear terrain normal, even when fully loaded.
        let image = Image::new_fill(
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[128, 128, 255, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let normal = app.world().resource::<AssetServer>().add(image);
        app.update();
        assert!(
            app.world()
                .resource::<AssetServer>()
                .is_loaded_with_dependencies(normal.id())
        );
        let entity = app
            .world_mut()
            .spawn((
                PendingTerrainProfile {
                    cell_id: 7,
                    quadrant: 2,
                    images: vec![],
                    normals: vec![normal.clone()],
                },
                Visibility::Hidden,
                SwapSurface::new(
                    Handle::<TerrainMaterial>::default().id().untyped(),
                    [normal.id()],
                ),
                SwapWaiting,
            ))
            .id();
        app.update();
        assert!(app.world().get::<AssetFailed>(entity).is_some());
        assert_eq!(
            app.world().get::<Visibility>(entity),
            Some(&Visibility::Hidden)
        );
        assert!(app.world().get::<TerrainSurfaceReady>(entity).is_none());
        assert!(app.world().get::<SwapWaiting>(entity).is_some());
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.terrain_validation_failures, 1);
        assert_eq!(metrics.terrain_patches_validated, 0);
        assert!(
            metrics.asset_failures[0]
                .dependency_chain
                .iter()
                .any(|reason| reason.contains("wrong color space"))
        );
        app.update();
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .terrain_validation_failures,
            1
        );
    }

    #[test]
    fn declared_terrain_normal_failure_refuses_readiness_with_its_reason() {
        let state = terrain_dependency_state(SurfaceDependencyState::Ready, || {
            SurfaceDependencyState::Failed("normal image has wrong color space".into())
        });
        assert!(matches!(state, SurfaceDependencyState::Failed(reason)
            if reason == "normal image has wrong color space"));
        assert!(matches!(
            terrain_dependency_state(SurfaceDependencyState::Ready, || {
                SurfaceDependencyState::Pending
            }),
            SurfaceDependencyState::Pending
        ));
        assert!(matches!(
            terrain_dependency_state(SurfaceDependencyState::Ready, || {
                SurfaceDependencyState::Ready
            }),
            SurfaceDependencyState::Ready
        ));
    }

    #[test]
    fn window_residency_counts_only_the_active_interior_inside() {
        let mut streaming = StreamingWorld::default();
        let inside = ActiveSpace {
            worldspace_id: None,
            interior: Some(77),
        };
        // Entering an interior unloads every exterior cell; the window is the one interior cell.
        assert_eq!(
            streaming.window_residency(&inside, 0x3c, IVec2::ZERO, 2),
            (1, 0, 0)
        );
        // An exterior cell left in the map does not count towards the interior's window.
        streaming.set_resident_for_test(
            CellKey::Exterior {
                worldspace_id: 0x3c,
                grid_x: 0,
                grid_y: 0,
            },
            Entity::PLACEHOLDER,
        );
        assert_eq!(
            streaming.window_residency(&inside, 0x3c, IVec2::ZERO, 2),
            (1, 0, 0)
        );
        streaming.set_resident_for_test(CellKey::Interior(77), Entity::PLACEHOLDER);
        assert_eq!(
            streaming.window_residency(&inside, 0x3c, IVec2::ZERO, 2),
            (1, 1, 0)
        );
        streaming.set_failed_for_test(CellKey::Interior(77));
        assert_eq!(
            streaming.window_residency(&inside, 0x3c, IVec2::ZERO, 2),
            (1, 0, 1)
        );
    }

    #[test]
    fn window_residency_reads_the_active_exterior_worldspace() {
        let mut streaming = StreamingWorld::default();
        let configured = 0x3c;
        let other = ActiveSpace {
            worldspace_id: Some(0x1a26f),
            interior: None,
        };
        for (worldspace_id, grid_x) in [(configured, 0), (0x1a26f, 0), (0x1a26f, 1)] {
            streaming.set_resident_for_test(
                CellKey::Exterior {
                    worldspace_id,
                    grid_x,
                    grid_y: 0,
                },
                Entity::PLACEHOLDER,
            );
        }
        // The default space is the configured worldspace: one of its 9 window cells is resident.
        assert_eq!(
            streaming.window_residency(&ActiveSpace::default(), configured, IVec2::ZERO, 1),
            (9, 1, 0)
        );
        // After a crossing into another worldspace, its own cells count, not the configured one's.
        assert_eq!(
            streaming.window_residency(&other, configured, IVec2::ZERO, 1),
            (9, 2, 0)
        );
    }

    #[test]
    fn terrain_sampler_check_rejects_default_and_clamped_images() {
        assert!(!terrain_sampler_repeats(&ImageSampler::Default));
        assert!(!terrain_sampler_repeats(&ImageSampler::linear()));
        assert!(terrain_sampler_repeats(&ImageSampler::Descriptor(
            bevy::image::ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..bevy::image::ImageSamplerDescriptor::linear()
            }
        )));
    }

    #[test]
    fn commit_budget_ignores_only_the_documented_scheduler_tolerance() {
        assert!(!commit_budget_exceeded(16_670, 16_670));
        assert!(!commit_budget_exceeded(17_670, 16_670));
        assert!(commit_budget_exceeded(17_671, 16_670));
    }

    /// A full commit that finds the frame's time budget already spent waits for the next frame, the
    /// way a terrain-only one does: the budget stops a commit from *starting*, whatever its tier.
    #[test]
    fn a_full_commit_waits_when_the_frame_budget_is_spent_before_it_starts() {
        let config = EngineConfig::default();
        let budget = config.max_commit_micros_per_frame;
        // Sixteen terrain-only commits have used the frame up; the full cell behind them waits.
        let after_the_ring = CommitsThisFrame {
            started: 16,
            full: 0,
            terrain: 16,
            elapsed_micros: budget,
        };
        assert!(commit_waits_for_next_frame(
            CellDetail::Full,
            after_the_ring,
            &config
        ));
        // With budget left it goes ahead: only the count and the time hold a full commit.
        let with_time_left = CommitsThisFrame {
            elapsed_micros: budget - 1,
            ..after_the_ring
        };
        assert!(!commit_waits_for_next_frame(
            CellDetail::Full,
            with_time_left,
            &config
        ));
        // And its count still holds it with time to spare: one full cell a frame by default.
        let after_a_full_commit = CommitsThisFrame {
            started: 1,
            full: 1,
            terrain: 0,
            elapsed_micros: 0,
        };
        assert!(commit_waits_for_next_frame(
            CellDetail::Full,
            after_a_full_commit,
            &config
        ));
        assert!(!commit_waits_for_next_frame(
            CellDetail::Terrain,
            after_a_full_commit,
            &config
        ));
        // A failed commit took its share of the frame too: it counts as started, not committed.
        let after_a_failed_commit = CommitsThisFrame {
            started: 1,
            full: 0,
            terrain: 0,
            elapsed_micros: budget,
        };
        assert!(commit_waits_for_next_frame(
            CellDetail::Terrain,
            after_a_failed_commit,
            &config
        ));
    }

    /// The first commit of a frame is never held by the time budget, however small the budget or
    /// however long taking the worker's answers took: a held answer is first in line the next
    /// frame, so it cannot be held forever. Only a count of zero holds it.
    #[test]
    fn the_first_commit_of_a_frame_is_never_held_by_the_time_budget() {
        let config = EngineConfig {
            max_commit_micros_per_frame: 0,
            ..EngineConfig::default()
        };
        let nothing_yet = CommitsThisFrame {
            started: 0,
            full: 0,
            terrain: 0,
            elapsed_micros: 50_000,
        };
        for detail in [CellDetail::Full, CellDetail::Terrain] {
            assert!(
                !commit_waits_for_next_frame(detail, nothing_yet, &config),
                "{detail:?}: the frame's first commit goes ahead"
            );
        }
        let no_full_cells = EngineConfig {
            max_cell_commits_per_frame: 0,
            ..config
        };
        assert!(commit_waits_for_next_frame(
            CellDetail::Full,
            nothing_yet,
            &no_full_cells
        ));
    }

    /// The grid-to-render offset and its inverse agree at a render origin away from zero: a
    /// Creation point placed by [`render_position`] comes back from [`absolute_render_position`]
    /// where [`creation_to_bevy`] puts it, sits in the grid [`camera_grid`] names, and keeps the
    /// same place inside its cell root ([`cell_translation`]) whatever the origin is.
    #[test]
    fn the_grid_offset_and_its_inverse_agree_at_a_non_zero_origin() {
        let origin = IVec2::new(5, -3);
        let creation = Vec3::new(7.0 * CELL_SIZE + 1234.5, -2.0 * CELL_SIZE + 301.25, 812.0);
        let grid = IVec2::new(7, -2);
        let key = exterior_cell(grid.x, grid.y);

        let render = render_position(creation, origin);
        assert_eq!(
            absolute_render_position(render, origin),
            creation_to_bevy(creation)
        );
        assert_eq!(camera_grid(render, origin), grid);
        assert_eq!(
            render - cell_translation(key, origin),
            render_position(creation, IVec2::ZERO) - cell_translation(key, IVec2::ZERO),
            "a point keeps its place inside its cell root whatever the origin"
        );
        // Two cells east and one north of the origin, in render space.
        assert_eq!(
            grid_translation(origin + IVec2::new(2, 1), origin),
            Vec3::new(2.0 * CELL_SIZE, 0.0, -CELL_SIZE)
        );
        // The origin's own cell sits at the render-space zero.
        assert_eq!(
            cell_translation(exterior_cell(origin.x, origin.y), origin),
            Vec3::ZERO
        );
    }

    fn terrain_fixture(cell_id: u32, height: f32) -> TerrainSnapshot {
        TerrainSnapshot {
            cell_id,
            width: 33,
            height: 33,
            heights: vec![height; 33 * 33],
            normals: (0..33 * 33).flat_map(|_| [0, 0, 127]).collect(),
            vertex_colors: vec![255; 33 * 33 * 3],
            layers: (0..4)
                .map(|quadrant| TerrainLayerSnapshot {
                    texture_form_id: u32::from(quadrant) + 1,
                    quadrant,
                    layer: 0,
                    is_base: true,
                    weights: Vec::new(),
                })
                .collect(),
            water_height: None,
            water_type_form_id: None,
        }
    }

    /// The same field without layers: `spawn_cell` looks every layer's diffuse image up in the
    /// asset catalogue, and the catalogue of a test app has none in it.
    fn untextured_fixture(cell_id: u32, height: f32) -> TerrainSnapshot {
        let mut terrain = terrain_fixture(cell_id, height);
        terrain.layers.clear();
        terrain
    }

    fn exterior_cell(grid_x: i32, grid_y: i32) -> CellKey {
        CellKey::Exterior {
            worldspace_id: 60,
            grid_x,
            grid_y,
        }
    }

    /// The tables `AssetCatalog::open` reads, empty: the cells spawned by these tests carry no
    /// layers, so nothing is ever looked up in it. The catalogue is read eagerly, so the file may
    /// go away with its temporary directory.
    fn write_empty_catalogue(path: &std::path::Path) {
        rusqlite::Connection::open(path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,texture_set_id INTEGER);
                 CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,diffuse_path TEXT);
                 CREATE TABLE waters(id INTEGER PRIMARY KEY,flow_normal_path TEXT);",
            )
            .unwrap();
    }

    /// Zero overlays retain ordered weights and geometry validation while missing nonzero
    /// diffuse images still fail locally and a subsequent placeholder snapshot remains usable.
    #[test]
    fn terrain_zero_texture_overlay_preserves_weights_and_nonzero_image_errors() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("placeholder-catalogue.db");
        write_empty_catalogue(&path);
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("INSERT INTO landscape_textures VALUES(2116,NULL);")
            .unwrap();
        drop(connection);
        let catalog = AssetCatalog::open(&path).unwrap();
        let mut terrain = terrain_fixture(91, 12.5);
        for layer in &mut terrain.layers {
            layer.texture_form_id = 0;
        }
        terrain.layers.extend([
            TerrainLayerSnapshot {
                texture_form_id: 0,
                quadrant: 0,
                layer: 4,
                is_base: false,
                weights: vec![(3, 0.625)],
            },
            TerrainLayerSnapshot {
                texture_form_id: 0,
                quadrant: 0,
                layer: 1,
                is_base: false,
                weights: vec![(0, 0.25), (18, 0.75)],
            },
        ]);
        validate_terrain_snapshot(&terrain, &catalog)
            .expect("normalized zero overlays retain their weights without image lookup");
        let layers = quadrant_layers(&terrain, 0).unwrap();
        assert_eq!(
            layers.iter().map(|layer| layer.layer).collect::<Vec<_>>(),
            vec![0, 1, 4]
        );
        let grids = quadrant_overlay_weights(&terrain, 0).unwrap();
        assert_eq!(grids.len(), 2);
        assert_eq!(grids[0][0].to_bits(), 0.25f32.to_bits());
        assert_eq!(grids[0][18].to_bits(), 0.75f32.to_bits());
        assert_eq!(grids[1][3].to_bits(), 0.625f32.to_bits());
        assert_eq!(grids[1][0], 0.0);
        assert!(build_terrain_quadrant_mesh(&terrain, 0).is_ok());

        terrain.layers[4].texture_form_id = 0x844;
        let error = validate_terrain_snapshot(&terrain, &catalog).unwrap_err();
        assert!(error.contains("00000844") && error.contains("no converted diffuse image"));
        terrain.layers[4].texture_form_id = 0;
        assert!(validate_terrain_snapshot(&terrain, &catalog).is_ok());

        terrain.layers[4].weights.push((3, 0.5));
        assert!(
            validate_terrain_snapshot(&terrain, &catalog)
                .unwrap_err()
                .contains("duplicate VTXT")
        );
        terrain.layers[4].weights.pop();
        terrain.heights[0] = f32::NAN;
        assert!(
            validate_terrain_snapshot(&terrain, &catalog)
                .unwrap_err()
                .contains("non-finite height")
        );
    }

    #[derive(Resource, Default)]
    struct QueuedCells(Vec<(CellKey, TerrainSnapshot)>);

    /// The walk gate's answer for [`spawn_queued_cells`]. These apps add no [`StreamingPlugin`], so
    /// the gate [`collect_cells`] reads never runs; handing the answer in is what a test does
    /// instead of committing a cell through the plan.
    #[derive(Resource, Default)]
    struct QueuedCellsPhysics(bool);

    /// Spawns each queued cell through the real [`spawn_cell`] and records it as resident, so the
    /// lifecycle validator next in the chain sees entities and bookkeeping that agree.
    #[allow(clippy::too_many_arguments)]
    fn spawn_queued_cells(
        mut commands: Commands,
        queued: Res<QueuedCells>,
        physics: Res<QueuedCellsPhysics>,
        asset_server: Res<AssetServer>,
        catalog: Res<AssetCatalog>,
        reflection: Res<WaterReflectionTexture>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
        mut water_materials: ResMut<Assets<WaterMaterial>>,
        mut streaming: ResMut<StreamingWorld>,
        mut profiler: ResMut<ProfilingState>,
    ) {
        for (key, terrain) in &queued.0 {
            let payload = CellPayload {
                generation: 1,
                key: *key,
                cell_id: terrain.cell_id,
                references: Vec::new(),
            };
            let root = spawn_demo_cell(
                &mut commands,
                &asset_server,
                &catalog,
                &reflection,
                &mut meshes,
                &mut terrain_materials,
                &mut water_materials,
                IVec2::ZERO,
                payload,
                Some(terrain.clone()),
                CellDetail::Full,
                physics.0,
                None,
                None,
                &mut 0,
                &mut profiler,
            );
            streaming.cells.insert(
                *key,
                CellStatus::Resident {
                    root,
                    detail: CellDetail::Full,
                },
            );
        }
    }

    /// An app that streams the given cells the way a commit does - spawn first, then the lifecycle
    /// validator, in one frame - with the assets `spawn_cell` writes into.
    fn spawn_cells_app(cells: Vec<(CellKey, TerrainSnapshot)>, metrics: StreamingMetrics) -> App {
        spawn_cells_app_with_physics(cells, metrics, false)
    }

    /// The same, with the walk gate's answer: `true` builds the colliders a walked run builds.
    fn spawn_cells_app_with_physics(
        cells: Vec<(CellKey, TerrainSnapshot)>,
        metrics: StreamingMetrics,
        world_physics: bool,
    ) -> App {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalogue.db");
        write_empty_catalogue(&path);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .init_asset::<TerrainMaterial>()
            .init_asset::<WaterMaterial>()
            .insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(AssetCatalog::open(&path).unwrap())
            .insert_resource(WaterReflectionTexture(Handle::default()))
            .insert_resource(QueuedCells(cells))
            .insert_resource(QueuedCellsPhysics(world_physics))
            .insert_resource(metrics)
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<ProfilingState>()
            .add_systems(
                Update,
                (spawn_queued_cells, validate_streaming_lifecycle).chain(),
            );
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        app.update();
        app
    }

    /// A committed cell times its real work: the four terrain meshes and their colliders are one
    /// `streaming/terrain_mesh` sample per cell (it used to be recorded against an `Instant` made
    /// on the line before, so it always read zero), the reference loop has a span of its own, and
    /// the cell reports what it carried and asked the loaders for.
    #[test]
    fn spawn_cell_times_the_terrain_build_and_the_reference_loop() {
        let app = spawn_cells_app(
            vec![(exterior_cell(2, -3), untextured_fixture(1, 100.0))],
            StreamingMetrics::default(),
        );

        let profiler = app.world().resource::<ProfilingState>();
        let terrain = profiler.span_samples("streaming/terrain_mesh");
        assert_eq!(terrain.len(), 1, "one terrain build for the one cell");
        assert!(terrain[0] > 0.0, "the terrain build took measurable time");
        assert_eq!(profiler.span_samples("streaming/spawn_references").len(), 1);
        assert_eq!(profiler.span_samples("streaming/spawn_cell").len(), 1);
        assert!(
            profiler.has_event_stage("spawned references=0 model_loads=0"),
            "the cell reports its reference and model-load counts"
        );
    }

    #[test]
    fn maps_nif_paths_to_converted_glb_paths() {
        assert_eq!(
            converted_model_path("meshes\\architecture\\wall.nif".into()).as_deref(),
            Some("meshes/architecture/wall.glb")
        );
        assert_eq!(
            converted_model_path("meshes/Sky/CloudShape01.nif".into()),
            None
        );
        assert_eq!(
            converted_model_path("meshes/Architecture/Wall.nif".into()).as_deref(),
            Some("meshes/architecture/wall.glb")
        );
        assert_eq!(converted_model_path("meshes/Marker_Map.nif".into()), None);
        assert_eq!(
            converted_model_path("Markers/CivilWarMarkers/CWAttSpawn02.nif".into()),
            None
        );
        assert_eq!(converted_model_path("Effects/FXRapids.nif".into()), None);
        assert_eq!(
            converted_model_path("meshes/Furniture/SitLedgeMarker.nif".into()),
            None
        );
    }

    #[test]
    fn rejects_traversal_and_rooted_model_paths() {
        assert_eq!(
            converted_model_path("meshes/../../secrets.nif".into()),
            None
        );
        assert_eq!(converted_model_path("meshes//etc/passwd".into()), None);
        assert_eq!(
            converted_model_path(r"meshes\..\..\secrets.nif".into()),
            None
        );
        // Windows treats a drive-prefixed path as absolute (and `PathBuf::join`
        // would let it replace the base path entirely); Rust's path parsing is
        // OS-native, so this case only bites on the Windows target this engine
        // ships for.
        #[cfg(windows)]
        assert_eq!(
            converted_model_path("meshes/C:/Windows/evil.nif".into()),
            None
        );
        // A plain relative path is unaffected.
        assert_eq!(
            converted_model_path("meshes/architecture/wall.nif".into()).as_deref(),
            Some("meshes/architecture/wall.glb")
        );
    }

    #[test]
    fn unload_radius_drops_distant_exteriors_and_inactive_interiors() {
        let center = IVec2::new(4, -2);
        let outside = ActiveSpace {
            worldspace_id: Some(60),
            interior: None,
        };
        let inside = ActiveSpace {
            worldspace_id: Some(60),
            interior: Some(99),
        };
        let exterior = |worldspace_id, grid_x, grid_y| CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        };

        assert!(cell_within_unload_radius(
            exterior(60, 7, -5),
            &outside,
            60,
            center,
            3,
            0
        ));
        assert!(!cell_within_unload_radius(
            exterior(60, 8, -2),
            &outside,
            60,
            center,
            3,
            0
        ));
        assert!(
            !cell_within_unload_radius(exterior(614, 4, -2), &outside, 60, center, 3, 0),
            "the same grid in another worldspace belongs to the worldspace just left"
        );
        assert!(
            !cell_within_unload_radius(exterior(60, 4, -2), &inside, 60, center, 3, 0),
            "no exterior is streamed while an interior is active"
        );
        assert!(cell_within_unload_radius(
            CellKey::Interior(99),
            &inside,
            60,
            center,
            0,
            0
        ));
        assert!(
            !cell_within_unload_radius(CellKey::Interior(98), &inside, 60, center, 0, 0),
            "an interior that is neither active nor pre-streamed unloads"
        );
        assert!(!cell_within_unload_radius(
            CellKey::Interior(99),
            &outside,
            60,
            center,
            0,
            0
        ));

        // The ring's own band: a cell one past the ring's edge survives, the next one does not, so
        // the edge of the ring does not flicker as the camera crosses a cell boundary.
        let terrain_radius = 12;
        assert!(cell_within_unload_radius(
            exterior(60, 4 + terrain_radius + 1, -2),
            &outside,
            60,
            center,
            3,
            terrain_radius
        ));
        assert!(!cell_within_unload_radius(
            exterior(60, 4 + terrain_radius + 2, -2),
            &outside,
            60,
            center,
            3,
            terrain_radius
        ));
        assert!(
            cell_within_unload_radius(
                exterior(60, 4 + 7, -2),
                &outside,
                60,
                center,
                3,
                terrain_radius
            ),
            "a ring cell seven cells out is nobody's unload radius but the ring's"
        );
    }

    #[test]
    fn wanted_cells_are_the_active_interior_or_the_active_worldspace_grid() {
        let center = IVec2::new(4, -2);
        let inside = ActiveSpace {
            worldspace_id: Some(60),
            interior: Some(99),
        };
        assert_eq!(
            wanted_cells(&inside, 60, 1, 1, center),
            vec![(CellKey::Interior(99), CellDetail::Full)],
            "an interior is the whole plan, and there is no exterior grid while it is active"
        );

        let blackreach = ActiveSpace {
            worldspace_id: Some(614),
            interior: None,
        };
        // With no ring, the plan is exactly the grid `stream_radius` always asked for.
        let wanted = wanted_cells(&blackreach, 60, 1, 1, center);
        assert_eq!(wanted.len(), 9);
        assert!(wanted.iter().all(|(key, detail)| matches!(
            key,
            CellKey::Exterior {
                worldspace_id: 614,
                ..
            }
        ) && *detail == CellDetail::Full));
        assert!(wanted.contains(&(
            CellKey::Exterior {
                worldspace_id: 614,
                grid_x: 5,
                grid_y: -3,
            },
            CellDetail::Full
        )));
    }

    /// The ring classification, as a pure function of the camera cell and the two radii: the inner
    /// grid in full, the ring around it terrain-only, and nothing beyond. Its own cell is the
    /// nearest full one, and the cell diagonally inside the grid corner counts as inside it.
    #[test]
    fn classifies_cells_into_the_inner_grid_the_ring_and_nothing() {
        let center = IVec2::new(4, -2);
        let outside = ActiveSpace {
            worldspace_id: Some(60),
            interior: None,
        };
        let exterior = |grid_x, grid_y| CellKey::Exterior {
            worldspace_id: 60,
            grid_x,
            grid_y,
        };
        let detail = |key| wanted_detail(&outside, 60, 2, 5, center, key);

        assert_eq!(detail(exterior(4, -2)), Some(CellDetail::Full));
        assert_eq!(detail(exterior(2, -4)), Some(CellDetail::Full));
        assert_eq!(
            detail(exterior(6, 0)),
            Some(CellDetail::Full),
            "the corner of the 5x5 grid is inside it"
        );
        assert_eq!(detail(exterior(7, -2)), Some(CellDetail::Terrain));
        assert_eq!(detail(exterior(4, 3)), Some(CellDetail::Terrain));
        assert_eq!(
            detail(exterior(9, -7)),
            Some(CellDetail::Terrain),
            "the ring's own corner is in the ring"
        );
        assert_eq!(detail(exterior(10, -2)), None);
        assert_eq!(detail(exterior(4, 4)), None);
        assert_eq!(detail(exterior(4, 8)), None);
        assert_eq!(
            detail(CellKey::Interior(99)),
            None,
            "an interior is not streamed while an exterior is active"
        );
        assert_eq!(
            wanted_detail(
                &outside,
                60,
                2,
                5,
                center,
                CellKey::Exterior {
                    worldspace_id: 614,
                    grid_x: 4,
                    grid_y: -2,
                }
            ),
            None,
            "an exterior of the worldspace the camera came from is not wanted at any detail"
        );

        // A ring at or inside the inner grid is no ring at all: the same cells, one tier.
        assert_eq!(
            wanted_detail(&outside, 60, 3, 3, center, exterior(7, -2)),
            Some(CellDetail::Full)
        );
        assert_eq!(
            wanted_detail(&outside, 60, 3, 3, center, exterior(8, -2)),
            None
        );
        assert_eq!(
            wanted_detail(&outside, 60, 0, 0, center, exterior(1, -2)),
            None,
            "the plan is the one cell the camera is in"
        );
    }

    /// The order the plan asks for its cells in is the order the worker's queue gets them in, and
    /// it decides what appears on screen first: the space behind a nearby door, then the cell the
    /// destination worldspace: this test only checks which keys the planner asks for.
    fn write_plan_database(path: &std::path::Path) {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .execute_batch(&format!(
                r#"CREATE TABLE schema_info(version INTEGER NOT NULL);
                INSERT INTO schema_info VALUES({version});
                CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER,interior_name TEXT,flags INTEGER NOT NULL);
                CREATE TABLE land(cell_id INTEGER PRIMARY KEY,heightmap BLOB NOT NULL);
                CREATE TABLE statics(id INTEGER PRIMARY KEY,editor_id TEXT,model_path TEXT,flags INTEGER NOT NULL,
                    bounds_min_x REAL NOT NULL DEFAULT -64,bounds_min_y REAL NOT NULL DEFAULT -64,bounds_min_z REAL NOT NULL DEFAULT -64,
                    bounds_max_x REAL NOT NULL DEFAULT 64,bounds_max_y REAL NOT NULL DEFAULT 64,bounds_max_z REAL NOT NULL DEFAULT 64,
                    bounds_valid INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE "references"(id INTEGER PRIMARY KEY,cell_id INTEGER NOT NULL,worldspace_id INTEGER,base_form_id INTEGER NOT NULL,
                    is_exterior INTEGER NOT NULL,pos_x REAL NOT NULL,pos_y REAL NOT NULL,pos_z REAL NOT NULL,local_x REAL,local_y REAL,
                    rot_x REAL NOT NULL,rot_y REAL NOT NULL,rot_z REAL NOT NULL,scale REAL NOT NULL DEFAULT 1.0);
                CREATE VIRTUAL TABLE exterior_spatial USING rtree(id,minX,maxX,minY,maxY,minZ,maxZ,+cell_id,+worldspace_id);
                INSERT INTO cells VALUES(99,NULL,NULL,NULL,'Alftand01',0);"#,
                version = shared::WORLD_DATABASE_SCHEMA_VERSION
            ))
            .unwrap();
    }

    /// [`write_plan_database`] plus the `lod_chunks` table, holding one chunk of `worldspace_id` and
    /// none of any other worldspace: the ring is off in the first and on in the rest
    /// ([`WorldDatabase::has_lod_for`], the resolver's second rule). The table's shape is the
    /// converter's own (`crates/converter/src/lod/terrain.rs`), though the planner only ever reads
    /// `worldspace_id` out of it.
    fn write_plan_database_with_lod_for(path: &std::path::Path, worldspace_id: u32) {
        write_plan_database(path);
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .execute_batch(&format!(
                r#"CREATE TABLE lod_chunks(
                    worldspace_id INTEGER NOT NULL,tier INTEGER NOT NULL,anchor_x INTEGER NOT NULL,anchor_y INTEGER NOT NULL,
                    payload_path TEXT NOT NULL,content_hash TEXT NOT NULL,
                    bounds_min_x REAL NOT NULL,bounds_min_y REAL NOT NULL,bounds_min_z REAL NOT NULL,
                    bounds_max_x REAL NOT NULL,bounds_max_y REAL NOT NULL,bounds_max_z REAL NOT NULL,
                    source_cells TEXT NOT NULL,PRIMARY KEY(worldspace_id,tier,anchor_x,anchor_y));
                INSERT INTO lod_chunks VALUES({worldspace_id},4,0,0,'lod/0/4/cell_0_0.glb','{hash}',0.0,0.0,0.0,0.0,0.0,0.0,'0,0');"#,
                hash = "a".repeat(64)
            ))
            .unwrap();
    }

    /// A database in which worldspace 60's grid (2,-3) is cell 1 and that cell has landscape. It
    /// carries the reference tables too - empty - so a full request for the same cell resolves to a
    /// cell with no references rather than failing on a missing table.
    fn write_terrain_database(path: &std::path::Path) {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .execute_batch(&format!(
                r#"CREATE TABLE schema_info(version INTEGER NOT NULL);
                INSERT INTO schema_info VALUES({version});
                CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER);
                CREATE TABLE land(cell_id INTEGER PRIMARY KEY);
                CREATE TABLE statics(id INTEGER PRIMARY KEY,model_path TEXT,
                    bounds_min_x REAL NOT NULL DEFAULT -64,bounds_min_y REAL NOT NULL DEFAULT -64,bounds_min_z REAL NOT NULL DEFAULT -64,
                    bounds_max_x REAL NOT NULL DEFAULT 64,bounds_max_y REAL NOT NULL DEFAULT 64,bounds_max_z REAL NOT NULL DEFAULT 64,
                    bounds_valid INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE "references"(id INTEGER PRIMARY KEY,cell_id INTEGER NOT NULL,worldspace_id INTEGER,base_form_id INTEGER NOT NULL,
                    is_exterior INTEGER NOT NULL,pos_x REAL NOT NULL,pos_y REAL NOT NULL,pos_z REAL NOT NULL,local_x REAL,local_y REAL,
                    rot_x REAL NOT NULL,rot_y REAL NOT NULL,rot_z REAL NOT NULL,scale REAL NOT NULL DEFAULT 1.0);
                CREATE VIRTUAL TABLE exterior_spatial USING rtree(id,minX,maxX,minY,maxY,minZ,maxZ,+cell_id,+worldspace_id);
                INSERT INTO cells VALUES(1,60,2,-3);
                INSERT INTO land VALUES(1);
                -- The cell one grid west, with no landscape in the cache: a full request for it
                -- still resolves to a cell, so a run that starts there has no failed cell in it.
                INSERT INTO cells VALUES(2,60,1,-3);"#,
                version = shared::WORLD_DATABASE_SCHEMA_VERSION
            ))
            .unwrap();
    }

    /// A cell cache holding one flat landscape without layers, encoded as the converter writes it.
    /// Without layers no layer texture is ever looked up, which is what lets the test use an empty
    /// asset catalogue.
    fn write_terrain_cache(path: &std::path::Path, cell_id: u32) {
        let cache = shared::CellCache {
            version: shared::CELL_CACHE_VERSION,
            cells: vec![shared::CachedLand {
                cell_id,
                width: 33,
                height: 33,
                heights: vec![0.0; 33 * 33],
                normals: (0..33 * 33).flat_map(|_| [0, 0, 127]).collect(),
                vertex_colors: Vec::new(),
                layers: Vec::new(),
                water_height: None,
                water_type_form_id: None,
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cache).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// The same cache with a landscape that cannot pass validation: every packed normal is zero,
    /// which `validate_terrain_snapshot` rejects. A ring cell that gets this must not read as a
    /// cell failure.
    fn write_invalid_terrain_cache(path: &std::path::Path, cell_id: u32) {
        let cache = shared::CellCache {
            version: shared::CELL_CACHE_VERSION,
            cells: vec![shared::CachedLand {
                cell_id,
                width: 33,
                height: 33,
                heights: vec![0.0; 33 * 33],
                normals: vec![0; 33 * 33 * 3],
                vertex_colors: Vec::new(),
                layers: Vec::new(),
                water_height: None,
                water_type_form_id: None,
            }],
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cache).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// The terrain colliders a cell holds: how many, and how many of those are disabled.
    fn terrain_collider_state(app: &mut App) -> (usize, usize) {
        let mut query = app
            .world_mut()
            .query_filtered::<(Entity, Option<&ColliderDisabled>), With<TerrainCollider>>();
        let mut total = 0;
        let mut disabled = 0;
        for (_, flag) in query.iter(app.world()) {
            total += 1;
            disabled += usize::from(flag.is_some());
        }
        (total, disabled)
    }

    /// The ring is landscape seen from a distance: the plan holds the camera inside the full
    /// window, so no collider is ever walked on out there, and a walked run builds none for it.
    #[test]
    fn ring_cells_build_no_terrain_colliders() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_terrain_cache(&cache_path, 1);
        let mut app = terrain_streaming_app_with(&database_path, &cache_path, true);
        let key = exterior_cell(2, -3);
        // The camera stands one cell east of the cell, so the cell is the ring.
        app.world_mut()
            .spawn((Transform::from_xyz(CELL_SIZE, 0.0, 0.0), StreamingCamera));
        run_until(&mut app, "the ring's terrain-only cell", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Resident {
                    detail: CellDetail::Terrain,
                    ..
                })
            )
        });

        assert_eq!(
            terrain_collider_state(&mut app),
            (0, 0),
            "a ring cell is never walked on, so it carries no collider at all"
        );
    }

    /// A full cell of a walked run carries one enabled terrain collider per quadrant.
    /// A run that does not walk builds none. Private role gating extends this behavior later.
    #[test]
    fn a_full_cell_builds_terrain_colliders_only_in_a_walked_run() {
        for &(walk, expected) in &[(true, (4, 0)), (false, (0, 0))] {
            let directory = tempfile::tempdir().unwrap();
            let database_path = directory.path().join("world.db");
            write_terrain_database(&database_path);
            let cache_path = directory.path().join("cell_cache.rkyv");
            write_terrain_cache(&cache_path, 1);
            let mut app = terrain_streaming_app_with(&database_path, &cache_path, walk);
            let key = exterior_cell(2, -3);
            // The camera stands in the cell, which is the full window's centre.
            app.world_mut()
                .spawn((Transform::default(), StreamingCamera));
            run_until(&mut app, "the camera's own full cell", |app| {
                matches!(
                    app.world().resource::<StreamingWorld>().cells.get(&key),
                    Some(CellStatus::Resident {
                        detail: CellDetail::Full,
                        ..
                    })
                )
            });

            assert_eq!(
                terrain_collider_state(&mut app),
                expected,
                "a walked run ({walk}) builds one enabled collider per quadrant"
            );
        }
    }

    /// A quadrant whose collider cannot be built is drawn without one and the cell still commits.
    ///
    /// The extraction refuses a mesh that is not a U32 triangle list, and that is the only error
    /// its caller can return; `build_terrain_quadrant_mesh` always emits one, so the branch is
    /// defensive in the real path. What is reachable is asserted here from both ends: the
    /// extraction's refusal, and a cell of a walked run that builds no collider (the ring) still
    /// becoming resident and drawing its quadrants - see
    /// [`ring_cells_build_no_terrain_colliders`], which drives the same "no collider" outcome.
    #[test]
    fn a_quadrant_without_a_usable_collider_does_not_fail_the_cell() {
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![Vec3::ZERO, Vec3::X, Vec3::Y]);
        assert!(
            terrain_collider_from_mesh(&mesh).is_err(),
            "a mesh with no U32 triangle indices cannot become a terrain collider"
        );

        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_terrain_cache(&cache_path, 1);
        let mut app = terrain_streaming_app_with(&database_path, &cache_path, true);
        let key = exterior_cell(2, -3);
        app.world_mut()
            .spawn((Transform::from_xyz(CELL_SIZE, 0.0, 0.0), StreamingCamera));
        run_until(&mut app, "the cell drawn without a collider", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Resident { .. })
            )
        });
        assert_eq!(
            terrain_collider_state(&mut app),
            (0, 0),
            "the cell commits and draws its quadrants with no collider built for them"
        );
        assert!(
            !app.world().resource::<StreamingWorld>().has_failed(&key),
            "and its commit is not a cell failure"
        );
        let roots = app
            .world_mut()
            .query_filtered::<Entity, With<TerrainPatch>>()
            .iter(app.world())
            .count();
        assert_eq!(roots, 4, "four quadrants are drawn for it");
    }

    /// A collider belongs to its cell's subtree, so the paced despawn that removes a retired cell's
    /// root takes its colliders with it - nothing is left colliding in a space that is gone.
    #[test]
    fn a_retiring_cells_colliders_go_with_its_subtree() {
        let (mut app, _directory) = streaming_test_app(crossing_config(1));
        spawn_camera(&mut app, IVec2::ZERO);
        let root = spawn_mock_interior(&mut app, 98);
        let collider = app
            .world_mut()
            .spawn((
                TerrainCollider,
                Collider::cuboid(1.0, 1.0, 1.0),
                RigidBody::Fixed,
                ChildOf(root),
            ))
            .id();
        // What the pacer leaves behind: a root nothing wants any more, waiting for the unload
        // budget. An interior is despawned the frame it falls out.
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                CellKey::Interior(98),
                CellStatus::Retiring {
                    root,
                    detail: CellDetail::Full,
                },
            );

        app.update();

        assert!(
            app.world().get_entity(collider).is_err(),
            "the collider was despawned with the subtree its cell root owns"
        );
        assert!(app.world().get_entity(root).is_err(), "the root went too");
    }

    /// The rebase moves the camera and every cell root with it, and shifts the rapier bodies of the
    /// colliders they own by the same displacement: a fixed body does not follow its Bevy
    /// transform through a rebase teleport.
    ///
    /// This app has no rapier plugin - which is every run today, and every run that does not walk -
    /// so the context query finds nothing, the collider stays where its root put it and the pass is
    /// a no-op rather than a panic. The body shift itself needs the plugin's PreStartup context
    /// entity and its FixedUpdate ECS-to-rapier sync (see the handoff note).
    #[test]
    fn a_rebase_moves_the_camera_and_the_roots_with_colliders_present() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<ActiveSpace>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, update_render_origin);
        let camera = app
            .world_mut()
            .spawn((
                Transform::from_xyz(CELL_SIZE * 2.0, 0.0, 0.0),
                StreamingCamera,
            ))
            .id();
        let root = app
            .world_mut()
            .spawn((ExteriorCellGrid(IVec2::new(3, 0)), Transform::default()))
            .id();
        let collider = app
            .world_mut()
            .spawn((
                TerrainCollider,
                Collider::cuboid(1.0, 1.0, 1.0),
                RigidBody::Fixed,
                Transform::default(),
                GlobalTransform::default(),
                ChildOf(root),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().resource::<RenderOrigin>().0,
            IVec2::new(2, 0),
            "the origin advances by the cells the camera moved"
        );
        assert_eq!(
            app.world()
                .entity(camera)
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::ZERO,
            "and the camera is put back at the space it stands in"
        );
        assert_eq!(
            app.world()
                .entity(root)
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::new(CELL_SIZE, 0.0, 0.0),
            "the cell root moves with the origin, so it keeps its place in the world"
        );
        assert!(
            app.world().entity(collider).contains::<Collider>(),
            "the collider is still there: without a rapier context there is no body to shift"
        );
        assert_eq!(app.world().resource::<StreamingMetrics>().origin_rebases, 1);
    }

    /// With rapier running, a rebase moves a fixed collider body by the same displacement as the
    /// cell root it sits under, so the player keeps standing on what is drawn (review-279).
    #[test]
    fn a_rebase_moves_live_rapier_bodies_with_their_roots() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy_rapier3d::prelude::{RapierRigidBodyHandle, ReadRapierContext};

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .add_plugins(crate::physics::PhysicsCorePlugin)
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(1.0 / 60.0),
            ))
            .insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<ActiveSpace>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, update_render_origin);
        app.finish();
        let camera = app
            .world_mut()
            .spawn((Transform::default(), StreamingCamera))
            .id();
        let root = app
            .world_mut()
            .spawn((
                ExteriorCellGrid(IVec2::new(3, 0)),
                Transform::from_xyz(CELL_SIZE * 3.0, 0.0, 0.0),
            ))
            .id();
        let body = app
            .world_mut()
            .spawn((
                TerrainCollider,
                Collider::cuboid(10.0, 10.0, 10.0),
                RigidBody::Fixed,
                Transform::from_xyz(5.0, 0.0, 0.0),
                ChildOf(root),
            ))
            .id();
        let body_x = |app: &mut App| {
            let handle = app
                .world()
                .entity(body)
                .get::<RapierRigidBodyHandle>()
                .expect("rapier has the body")
                .0;
            app.world_mut()
                .run_system_once(move |context: ReadRapierContext| {
                    context.single().unwrap().rigidbody_set.bodies[handle]
                        .translation()
                        .x
                })
                .unwrap()
        };
        for _ in 0..5 {
            app.update();
        }
        assert!((body_x(&mut app) - (CELL_SIZE * 3.0 + 5.0)).abs() < 1.0e-2);

        // Two cells east: the origin moves by two cells and everything drawn shifts back by them.
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x = CELL_SIZE * 2.0;
        app.update();
        let root_x = app
            .world()
            .entity(root)
            .get::<Transform>()
            .unwrap()
            .translation
            .x;
        assert!(
            (root_x - CELL_SIZE).abs() < 1.0e-2,
            "the root moved: {root_x}"
        );
        let moved = body_x(&mut app);
        assert!(
            (moved - (root_x + 5.0)).abs() < 1.0e-2,
            "the body sits where its root now is: body {moved}, root {root_x}"
        );
    }

    /// A pre-streamed interior's root keeps its absolute place through an exterior rebase, so its
    fn terrain_streaming_app(database_path: &std::path::Path, cache_path: &std::path::Path) -> App {
        terrain_streaming_app_with(database_path, cache_path, false)
    }

    /// The same app as a walked run, or as any other: `walk` is the whole of what
    /// [`builds_world_colliders`] reads, so it is what decides whether the cells it streams carry
    /// colliders.
    fn terrain_streaming_app_with(
        database_path: &std::path::Path,
        cache_path: &std::path::Path,
        walk: bool,
    ) -> App {
        let catalogue_path = database_path.with_extension("catalogue");
        write_empty_catalogue(&catalogue_path);
        let mut app = App::new();
        let mut config = EngineConfig {
            // The camera's own cell is the inner grid; one cell out from it is the ring, so the
            // same cell can be streamed either way by moving the camera one cell.
            stream_radius: 0,
            unload_radius: 1,
            terrain_radius: 2,
            ..EngineConfig::default()
        };
        config.headless = !walk;
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .init_asset::<TerrainMaterial>()
            .init_asset::<WaterMaterial>()
            .insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::new(2, -3)))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(CellCache::open(cache_path).unwrap())
            .insert_resource(AssetCatalog::open(&catalogue_path).unwrap())
            .insert_resource(WaterReflectionTexture(Handle::default()))
            .insert_resource(WorldDatabase::open(database_path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, (plan_cells, collect_cells).chain());
        app
    }

    /// Runs frames until `done`, or panics: the worker answers on its own thread, so a fixed number
    /// of updates would be a race.
    fn run_until<F: Fn(&App) -> bool>(app: &mut App, what: &str, done: F) {
        for _ in 0..600 {
            app.update();
            if done(app) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out waiting for {what}");
    }

    /// A retry test's app: the terrain fixture's app over a database whose `references` table is
    /// missing, which makes the cell's full request fail - a database error, the transient case -
    /// while its terrain-only request never reads the table and commits.
    fn failed_full_app(database_path: &std::path::Path, cache_path: &std::path::Path) -> App {
        rusqlite::Connection::open(database_path)
            .unwrap()
            .execute_batch(r#"DROP TABLE "references";"#)
            .unwrap();
        terrain_streaming_app(database_path, cache_path)
    }

    /// Undoes [`failed_full_app`]'s drop: the transient cause heals, and the next full request the
    /// plan asks for succeeds.
    fn restore_references_table(database_path: &std::path::Path) {
        rusqlite::Connection::open(database_path)
            .unwrap()
            .execute_batch(
                r#"CREATE TABLE "references"(id INTEGER PRIMARY KEY,cell_id INTEGER NOT NULL,worldspace_id INTEGER,base_form_id INTEGER NOT NULL,
                    is_exterior INTEGER NOT NULL,pos_x REAL NOT NULL,pos_y REAL NOT NULL,pos_z REAL NOT NULL,local_x REAL,local_y REAL,
                    rot_x REAL NOT NULL,rot_y REAL NOT NULL,rot_z REAL NOT NULL,scale REAL NOT NULL DEFAULT 1.0);"#,
            )
            .unwrap();
    }

    /// A cell cache holding no landscape at all: the fixture's cell resolves, so its terrain-only
    /// request commits nothing and leaves the cell failed, and no rescue root is ever drawn.
    fn write_empty_cell_cache(path: &std::path::Path) {
        let cache = shared::CellCache {
            version: shared::CELL_CACHE_VERSION,
            cells: Vec::new(),
        };
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&cache).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// Pretends every backoff the failed cell's records carry has elapsed - the wait the real clock
    /// pays between plan passes while the camera stays near - so the next plan pass may ask again.
    fn elapse_backoff(app: &mut App, key: CellKey) {
        let past = Instant::now() - Duration::from_millis(1);
        let mut streaming = app.world_mut().resource_mut::<StreamingWorld>();
        if let Some(CellStatus::Failed {
            retry_at: Some(retry_at),
            ..
        }) = streaming.cells.get_mut(&key)
        {
            *retry_at = past;
        }
        if let Some(failure) = streaming
            .full_failed
            .get_mut(&key)
            .filter(|failure| failure.retry_at.is_some())
        {
            failure.retry_at = Some(past);
        }
    }

    /// The cell roots that draw one cell, counted by the cell id they carry. References carry a
    /// `CellRef` too, so only roots are counted.
    fn roots_of(app: &mut App, cell_id: u32) -> usize {
        let mut roots = app
            .world_mut()
            .query_filtered::<&CellRef, Or<(With<StreamedCellRoot>, With<DistantTerrainRoot>)>>();
        roots
            .iter(app.world())
            .filter(|cell| cell.0 == cell_id)
            .count()
    }

    fn planned_keys(app: &App) -> HashSet<CellKey> {
        app.world()
            .resource::<StreamingWorld>()
            .cells
            .keys()
            .copied()
            .collect()
    }

    #[test]
    fn the_lod_horizon_is_known_per_worldspace() {
        let mut horizon = LodHorizon::default();
        assert_eq!(horizon.reach_in(60), None, "nothing known before an answer");
        horizon.set(60, 372_000.0);
        assert_eq!(horizon.reach_in(60), Some(372_000.0));
        assert_eq!(
            horizon.reach_in(61),
            None,
            "another worldspace has not answered"
        );
    }

    #[test]
    fn the_held_lod_flag_keeps_chunks_through_an_interior_visit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database(&path);
        let config = EngineConfig {
            lod_keep_hidden_in_interiors: true,
            ..EngineConfig::default()
        };
        let worldspace = config.worldspace_id;
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(worldspace),
                interior: None,
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<ActiveSpace>()
            .init_resource::<lod::LodStreaming>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, lod::plan_lod_chunks);
        app.world_mut().spawn((
            Transform::from_translation(Vec3::new(100.0, 40.0, -200.0)),
            StreamingCamera,
        ));
        let chunk_root = lod::LodStreaming::spawn_ready_chunk_for_test(
            app.world_mut(),
            worldspace,
            shared::lod::ChunkAnchor::new(0, 0),
            shared::lod::LodOrigin::new(0, 0),
        );
        app.update();

        app.world_mut().resource_mut::<ActiveSpace>().interior = Some(99);
        app.update();
        let lod = app.world().resource::<lod::LodStreaming>();
        assert_eq!(lod.held_chunks_for_test(), 1, "kept for the return");
        assert_eq!(lod.retiring_chunks_for_test(), 0, "and not the pacer's");
        assert_eq!(
            app.world().get::<Visibility>(chunk_root),
            Some(&Visibility::Hidden),
            "hidden for as long as the visit lasts"
        );
        assert!(
            app.world().get::<RetireHidden>(chunk_root).is_some(),
            "the hide is marked, so the return gives back exactly it"
        );
        assert_eq!(
            app.world().resource::<StreamingMetrics>().held_lod_chunks,
            1
        );

        // The unload pacer has nothing to take: a held chunk is the flag's to show again, not a
        // retiree waiting for the budget.
        despawn_cells(app.world_mut());
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .unloaded_lod_chunks,
            0
        );
        assert!(app.world().get_entity(chunk_root).is_ok());

        // Back out the same door: the chunk is shown in the frame the exterior is entered.
        app.world_mut().resource_mut::<ActiveSpace>().interior = None;
        app.update();
        let lod = app.world().resource::<lod::LodStreaming>();
        assert_eq!(lod.held_chunks_for_test(), 0);
        assert_eq!(lod.resident_chunks_for_test(), 1);
        assert!(
            app.world().get_entity(chunk_root).is_ok(),
            "the same root draws it again: nothing was spawned a second time"
        );
        assert_ne!(
            app.world().get::<Visibility>(chunk_root),
            Some(&Visibility::Hidden),
            "shown at once, with no stream-in"
        );
        assert!(
            app.world().get::<RetireHidden>(chunk_root).is_none(),
            "and the marker goes with the show"
        );
        assert_eq!(
            app.world().resource::<StreamingMetrics>().held_lod_chunks,
            0
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .revived_lod_chunks,
            1,
            "the return is a revive of a held chunk"
        );

        // Held again, then a door into another worldspace: not its exterior any more, so the
        // chunk starts retiring and the budget takes it.
        app.world_mut().resource_mut::<ActiveSpace>().interior = Some(99);
        app.update();
        assert_eq!(
            app.world()
                .resource::<lod::LodStreaming>()
                .held_chunks_for_test(),
            1
        );
        app.world_mut().resource_mut::<ActiveSpace>().worldspace_id = Some(61);
        app.world_mut().resource_mut::<ActiveSpace>().interior = None;
        app.update();
        assert_eq!(
            app.world()
                .resource::<lod::LodStreaming>()
                .retiring_chunks_for_test(),
            1,
            "a held chunk the plan stops wanting starts retiring"
        );
        despawn_cells(app.world_mut());
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .unloaded_lod_chunks,
            1
        );
        assert!(app.world().get_entity(chunk_root).is_err());
    }

    /// A camera that reverses before the unload budget has reached the chunks revives them: the
    /// same roots draw again - nothing is spawned or streamed a second time - and the marker the
    /// retire wrote is given back with it.
    #[test]
    fn a_reversal_on_an_interior_crossing_revives_retiring_lod_chunks() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database(&path);
        let config = EngineConfig::default();
        let worldspace = config.worldspace_id;
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(worldspace),
                interior: None,
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<ActiveSpace>()
            .init_resource::<lod::LodStreaming>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, lod::plan_lod_chunks);
        app.world_mut().spawn((
            Transform::from_translation(Vec3::new(100.0, 40.0, -200.0)),
            StreamingCamera,
        ));
        let chunk_root = lod::LodStreaming::spawn_ready_chunk_for_test(
            app.world_mut(),
            worldspace,
            shared::lod::ChunkAnchor::new(0, 0),
            shared::lod::LodOrigin::new(0, 0),
        );
        app.update();

        app.world_mut().resource_mut::<ActiveSpace>().interior = Some(99);
        app.update();
        assert_eq!(
            app.world()
                .resource::<lod::LodStreaming>()
                .retiring_chunks_for_test(),
            1
        );

        // Turn around before the budget has reached the chunk.
        app.world_mut().resource_mut::<ActiveSpace>().interior = None;
        app.update();
        let lod = app.world().resource::<lod::LodStreaming>();
        assert_eq!(lod.retiring_chunks_for_test(), 0);
        assert_eq!(
            lod.resident_chunks_for_test(),
            1,
            "the entry is the one it was"
        );
        assert!(
            app.world().get_entity(chunk_root).is_ok(),
            "a revived chunk keeps the root it had: nothing is spawned a second time"
        );
        assert_ne!(
            app.world().get::<Visibility>(chunk_root),
            Some(&Visibility::Hidden),
            "and the visibility the retire took is given back"
        );
        assert!(
            app.world().get::<RetireHidden>(chunk_root).is_none(),
            "the marker goes with it, so a later retire hides afresh instead of trusting it"
        );
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.revived_lod_chunks, 1);
        assert_eq!(
            metrics.unloaded_lod_chunks, 0,
            "the budget never reached it, so nothing was unloaded"
        );
        assert_eq!(metrics.resident_lod_chunks, 1);
    }

    #[test]
    fn the_terrain_ring_follows_the_active_worldspaces_lod_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database_with_lod_for(&path, 614);
        let config = EngineConfig {
            stream_radius: 1,
            terrain_radius: 2,
            ..EngineConfig::default()
        };
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, plan_cells);
        let translation = Vec3::new(100.0, 40.0, -200.0);
        let center = camera_grid(translation, IVec2::ZERO);
        app.world_mut()
            .spawn((Transform::from_translation(translation), StreamingCamera));
        app.update();

        // Worldspace 60 has no `lod_chunks` row, so the ring is the configured one: two cells deep,
        // around the full-detail grid of `stream_radius`.
        let ring = planned_keys(&app);
        assert_eq!(
            ring.len(),
            25,
            "a worldspace without LOD data plans the 2-cell ring"
        );
        assert!(ring.iter().all(|key| matches!(
            key,
            CellKey::Exterior {
                worldspace_id: 60,
                ..
            }
        )));
        let ring_cell = CellKey::Exterior {
            worldspace_id: 60,
            grid_x: center.x + 2,
            grid_y: center.y,
        };
        assert!(
            ring.contains(&ring_cell),
            "the ring's own cells are planned, and {ring_cell:?} is not among the {} planned",
            ring.len()
        );

        // Worldspace 614 has one, so it draws its own distant terrain and the plan wants nothing
        // beyond the full-detail grid - the same crossing the review found the far plane did not
        // follow, planned from the other side.
        *app.world_mut().resource_mut::<ActiveSpace>() = ActiveSpace {
            worldspace_id: Some(614),
            interior: None,
        };
        app.update();

        let lod = planned_keys(&app);
        assert_eq!(
            lod.len(),
            9,
            "a worldspace with LOD data plans its full-detail grid and leaves the ring to the LOD"
        );
        assert!(lod.iter().all(|key| matches!(
            key,
            CellKey::Exterior {
                worldspace_id: 614,
                ..
            }
        )));
        assert!(
            !lod.contains(&CellKey::Exterior {
                worldspace_id: 614,
                grid_x: center.x + 2,
                grid_y: center.y,
            }),
            "the cell the LOD draws is not planned as a ring cell"
        );
        assert!(
            !lod.contains(&ring_cell),
            "the worldspace just left is unloaded, ring and all"
        );
    }

    /// A worldspace change that unloads resident cells records the immediate unload once, with the
    /// number of roots and the entities in their subtrees, and the queued despawns are applied at
    /// the explicit sync point the apply bracket spans. The test installs the same bracket the
    /// plugin installs for a profiled run.
    #[test]
    fn a_crossing_frame_records_the_space_change_unload_when_profiling() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database(&path);
        let config = EngineConfig {
            stream_radius: 1,
            terrain_radius: 1,
            profile_output_dir: Some(directory.path().join("profiles")),
            ..EngineConfig::default()
        };
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<lod::LodStreaming>()
            .init_resource::<ActiveSpace>()
            .init_resource::<ProfilingState>()
            .init_resource::<SpaceUnloadFlushClock>()
            .add_systems(
                Update,
                (
                    plan_cells,
                    stamp_space_unload_flush.after_ignore_deferred(plan_cells),
                    bevy::ecs::schedule::ApplyDeferred,
                    record_space_unload_flush.before(despawn_cells),
                    despawn_cells,
                )
                    .chain(),
            );
        spawn_camera(&mut app, IVec2::ZERO);
        let roots: Vec<Entity> = [(0, 0), (1, 0), (0, 1)]
            .iter()
            .map(|(grid_x, grid_y)| spawn_mock_cell(&mut app, *grid_x, *grid_y, true))
            .collect();
        app.update();

        // The three mock cells are resident inside the camera's keep-set, so the plan leaves them
        // alone and nothing is unloaded yet.
        assert!(
            roots
                .iter()
                .all(|root| app.world().get_entity(*root).is_ok())
        );
        assert!(
            app.world()
                .resource::<ProfilingState>()
                .span_samples("streaming/space_change_unload")
                .is_empty()
        );

        *app.world_mut().resource_mut::<ActiveSpace>() = ActiveSpace {
            worldspace_id: Some(614),
            interior: None,
        };
        app.update();

        let profiler = app.world().resource::<ProfilingState>();
        assert_eq!(
            profiler.span_samples("streaming/space_change_unload").len(),
            1,
            "the crossing frame records the immediate unload once"
        );
        assert_eq!(
            profiler.gauge("streaming/space_change_unload_roots"),
            Some(3.0),
            "the gauge counts the roots the frame unloads"
        );
        assert_eq!(
            profiler.gauge("streaming/space_change_unload_entities"),
            Some(9.0),
            "each mock root carries a child and a grandchild"
        );
        assert_eq!(
            profiler
                .span_samples("streaming/space_change_unload_apply")
                .len(),
            1,
            "the deferred despawns are applied at the spanned sync point"
        );
        assert!(
            roots
                .iter()
                .all(|root| app.world().get_entity(*root).is_err()),
            "the despawns the span brackets really removed the roots"
        );

        // The run report keeps a gauge's value at the end of the run, and a crossing's counts are
        // zero on every frame after it: a plain last-value gauge would read 0 for the whole run
        // (what the A1 window saw, analysis-1103 Â§0). Quiet frames after the crossing must not
        // erase it.
        for _ in 0..3 {
            app.update();
        }
        let profiler = app.world().resource::<ProfilingState>();
        assert_eq!(
            profiler.gauge("streaming/space_change_unload_roots"),
            Some(3.0),
            "the gauge keeps the crossing's real root count, not the quiet frame's zero"
        );
        assert_eq!(
            profiler.gauge("streaming/space_change_unload_entities"),
            Some(9.0),
            "the gauge keeps the crossing's real entity count, not the quiet frame's zero"
        );
    }

    /// Without profiling, a crossing that unloads cells records no span and no gauge: the counting
    /// walk and the apply bracket do not exist, and the despawns still happen at Bevy's own sync
    /// point before the exclusive `despawn_cells`.
    #[test]
    fn a_crossing_frame_records_nothing_without_profiling() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database(&path);
        let config = EngineConfig {
            stream_radius: 1,
            terrain_radius: 1,
            ..EngineConfig::default()
        };
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<lod::LodStreaming>()
            .init_resource::<ActiveSpace>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, (plan_cells, despawn_cells).chain());
        spawn_camera(&mut app, IVec2::ZERO);
        let roots: Vec<Entity> = [(0, 0), (1, 0), (0, 1)]
            .iter()
            .map(|(grid_x, grid_y)| spawn_mock_cell(&mut app, *grid_x, *grid_y, true))
            .collect();
        app.update();

        *app.world_mut().resource_mut::<ActiveSpace>() = ActiveSpace {
            worldspace_id: Some(614),
            interior: None,
        };
        app.update();

        let profiler = app.world().resource::<ProfilingState>();
        assert!(
            profiler
                .span_samples("streaming/space_change_unload")
                .is_empty()
        );
        assert!(
            profiler
                .span_samples("streaming/space_change_unload_apply")
                .is_empty()
        );
        assert!(
            profiler
                .gauge("streaming/space_change_unload_roots")
                .is_none()
        );
        assert!(
            roots
                .iter()
                .all(|root| app.world().get_entity(*root).is_err()),
            "the unload still happens, only unmeasured"
        );
    }

    /// A stand-in for the cell plan's commit: it queues the deferred spawn work a real one would,
    /// so the bracket after it has something to apply. The frame's commit count is set by the test
    /// beforehand, the way `plan_cells` and the commit loop set it in a real frame.
    fn queue_mock_commits(mut commands: Commands) {
        commands.queue(|_: &mut World| {});
        commands.queue(|_: &mut World| {});
    }

    /// The other deferred half of a frame's work: a cell commit queues the root, its references
    /// and their scene spawns as commands, so the frame pays for them at the sync point after
    /// `collect_cells`, not inside the system. The bracket spans that apply, and only on a frame
    /// that committed anything.
    #[test]
    fn a_commit_frame_records_the_apply_at_the_spanned_sync_point() {
        for (commits, expected) in [(2_usize, 1_usize), (0, 0)] {
            let mut app = App::new();
            app.init_resource::<StreamingCommitBudget>()
                .init_resource::<CellCommitFlushClock>()
                .init_resource::<ProfilingState>()
                .add_systems(
                    Update,
                    (
                        queue_mock_commits,
                        stamp_cell_commit_flush.after_ignore_deferred(queue_mock_commits),
                        bevy::ecs::schedule::ApplyDeferred,
                        record_cell_commit_flush,
                    )
                        .chain(),
                );
            app.world_mut()
                .resource_mut::<StreamingCommitBudget>()
                .commits = commits;
            app.update();
            assert_eq!(
                app.world()
                    .resource::<ProfilingState>()
                    .span_samples("streaming/commit_apply")
                    .len(),
                expected,
                "the apply is spanned exactly on a frame that committed cells"
            );
        }
    }

    /// The apply brackets are a profiled run's only, and an ordinary run installs none of them:
    /// the clock resources the bracket systems share exist only behind the plugin's gate.
    #[test]
    fn the_flush_brackets_are_installed_only_when_profiling() {
        let directory = tempfile::tempdir().unwrap();
        for (profiled, expected) in [(false, false), (true, true)] {
            let mut app = App::new();
            app.insert_resource(EngineConfig {
                profile_output_dir: profiled.then(|| directory.path().join("profiles")),
                ..EngineConfig::default()
            })
            .add_plugins(StreamingPlugin);
            assert_eq!(
                app.world().contains_resource::<SpaceUnloadFlushClock>(),
                expected,
                "the unload apply bracket follows the profile gate"
            );
            assert_eq!(
                app.world().contains_resource::<CellCommitFlushClock>(),
                expected,
                "the commit apply bracket follows the profile gate"
            );
        }
    }

    // --- The paced unload budget (upstream main's `despawn_cells`) ------------------------------

    /// An app with the cell plan, the unload pacer and the lifecycle validator chained as they run
    /// in `StreamingPlugin`. `collect_cells` is left out, so a requested cell stays `Loading` and
    /// the test drives the resident cells itself through [`spawn_mock_cell`].
    fn streaming_test_app(config: EngineConfig) -> (App, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database(&path);
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<lod::LodStreaming>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<ActiveSpace>()
            .init_resource::<ProfilingState>()
            .add_systems(
                Update,
                (plan_cells, despawn_cells, validate_streaming_lifecycle).chain(),
            );
        (app, directory)
    }

    fn spawn_camera(app: &mut App, center: IVec2) {
        app.world_mut().spawn((
            Transform::from_xyz(
                center.x as f32 * CELL_SIZE,
                0.0,
                -(center.y as f32) * CELL_SIZE,
            ),
            StreamingCamera,
        ));
    }

    fn move_camera(app: &mut App, shift: IVec2) {
        let mut camera = app
            .world_mut()
            .query_filtered::<&mut Transform, With<StreamingCamera>>();
        let world = app.world_mut();
        let mut transforms = camera.query_mut(world);
        let mut transform = transforms.single_mut().unwrap();
        transform.translation.x += shift.x as f32 * CELL_SIZE;
        transform.translation.z -= shift.y as f32 * CELL_SIZE;
    }

    fn cell_id_of(grid_x: i32, grid_y: i32) -> u32 {
        ((grid_x + 64) as u32) * 256 + (grid_y + 64) as u32
    }

    /// Spawns a cell root with a two-entity subtree under it and records the cell as resident or
    /// retiring, the state a committed cell is in.
    fn spawn_mock_cell(app: &mut App, grid_x: i32, grid_y: i32, resident: bool) -> Entity {
        let root = app
            .world_mut()
            .spawn((
                CellRef(cell_id_of(grid_x, grid_y)),
                StreamedCellRoot,
                ExteriorCellGrid(IVec2::new(grid_x, grid_y)),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        // The first child is the cell's landscape, as in a real cell: the lifecycle check expects
        // every resident full exterior root to hold a terrain patch.
        let child = app
            .world_mut()
            .spawn((Transform::default(), TerrainPatch, ChildOf(root)))
            .id();
        app.world_mut()
            .spawn((Transform::default(), ChildOf(child)));
        let detail = CellDetail::Full;
        let status = if resident {
            CellStatus::Resident { root, detail }
        } else {
            CellStatus::Retiring { root, detail }
        };
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(exterior_cell(grid_x, grid_y), status);
        root
    }

    /// The same for the interior the camera stands in, which carries no [`ExteriorCellGrid`].
    fn spawn_mock_interior(app: &mut App, cell_id: u32) -> Entity {
        let root = app
            .world_mut()
            .spawn((
                CellRef(cell_id),
                StreamedCellRoot,
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                CellKey::Interior(cell_id),
                CellStatus::Resident {
                    root,
                    detail: CellDetail::Full,
                },
            );
        root
    }

    /// A steady window around `center`: the 7x7 cells the camera wants plus the trailing column a
    /// crossing drops, which is the shape the resident set settles into while flying.
    fn spawn_steady_window(app: &mut App, center: IVec2) {
        for grid_y in (center.y - 3)..=(center.y + 3) {
            for grid_x in (center.x - 4)..=(center.x + 3) {
                spawn_mock_cell(app, grid_x, grid_y, true);
            }
        }
    }

    fn resident_root_count(app: &mut App) -> usize {
        let mut query = app
            .world_mut()
            .query_filtered::<Entity, With<StreamedCellRoot>>();
        let world = app.world();
        query.iter(world).count()
    }

    fn streaming_metrics(app: &App) -> StreamingMetrics {
        app.world().resource::<StreamingMetrics>().clone()
    }

    /// A crossing config with the ring empty (`terrain_radius == stream_radius`), so the wanted
    /// grid is exactly the stream grid and the numbers are main's.
    fn crossing_config(unload_budget: usize) -> EngineConfig {
        EngineConfig {
            stream_radius: 3,
            terrain_radius: 3,
            unload_radius: 4,
            max_cell_unloads_per_frame: unload_budget,
            // These tests pace by the cell count, the path the time budget replaces; the wall-clock
            // budget's own tests set `max_unload_ms_per_frame` instead.
            max_unload_ms_per_frame: 0.0,
            ..EngineConfig::default()
        }
    }

    #[test]
    fn a_zero_unload_budget_unloads_the_whole_crossing_in_one_frame() {
        let (mut app, _directory) = streaming_test_app(crossing_config(0));
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_steady_window(&mut app, IVec2::ZERO);
        let roots = resident_root_count(&mut app);
        app.update();

        move_camera(&mut app, IVec2::X);
        app.update();
        let metrics = streaming_metrics(&app);
        assert_eq!(
            metrics.despawns_this_frame, 7,
            "0 means unbudgeted, as before"
        );
        assert_eq!(metrics.retiring_cells, 0);
        assert_eq!(metrics.retire_backlog_overflows, 0);
        assert_eq!(resident_root_count(&mut app), roots - 7);
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    #[test]
    fn a_crossing_unloads_cells_within_the_budget_and_finishes_within_ceil_frames() {
        let (mut app, _directory) = streaming_test_app(crossing_config(2));
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_steady_window(&mut app, IVec2::ZERO);
        let roots = resident_root_count(&mut app);
        app.update();
        let settled = streaming_metrics(&app);
        assert_eq!(
            settled.unloaded_cells, 0,
            "the window fits inside the radius"
        );
        assert_eq!(settled.retiring_cells, 0);
        assert_eq!(settled.streaming_invariant_failures, 0);

        // Cross into the next cell: the whole trailing column leaves the unload radius at once.
        move_camera(&mut app, IVec2::X);
        let mut unloads_per_frame = Vec::new();
        let mut roots_left = Vec::new();
        for frame in 1..=4 {
            app.update();
            let metrics = streaming_metrics(&app);
            unloads_per_frame.push(metrics.despawns_this_frame);
            assert!(
                metrics.despawns_this_frame <= 2,
                "frame {frame} unloaded {} cells against a budget of 2",
                metrics.despawns_this_frame
            );
            assert_eq!(metrics.streaming_invariant_failures, 0, "frame {frame}");
            roots_left.push(resident_root_count(&mut app));
        }
        assert_eq!(
            unloads_per_frame,
            vec![2, 2, 2, 1],
            "the column's 7 cells must spread over ceil(7 / 2) frames"
        );
        assert_eq!(roots_left, vec![roots - 2, roots - 4, roots - 6, roots - 7]);
        let metrics = streaming_metrics(&app);
        assert_eq!(metrics.unloaded_cells, 7);
        assert_eq!(metrics.retiring_cells, 0);
        assert_eq!(metrics.max_despawns_per_frame, 2);
        assert_eq!(metrics.peak_retiring_cells, 7);
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    #[test]
    fn a_reversal_revives_retiring_cells_without_another_request() {
        let (mut app, _directory) = streaming_test_app(crossing_config(2));
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_steady_window(&mut app, IVec2::ZERO);
        app.update();
        move_camera(&mut app, IVec2::X);
        app.update();
        let after_crossing = streaming_metrics(&app);
        assert_eq!(
            after_crossing.retiring_cells, 5,
            "the crossing retires a column of 7 and the budget takes 2 in the same frame"
        );
        assert_eq!(after_crossing.unloaded_cells, 2);
        let roots_after_crossing = resident_root_count(&mut app);

        // Turn around before the budget has worked through the column.
        move_camera(&mut app, IVec2::NEG_X);
        app.update();
        let after_reversal = streaming_metrics(&app);
        assert_eq!(after_reversal.revived_cells, 5);
        assert_eq!(after_reversal.retiring_cells, 0);
        assert_eq!(after_reversal.despawns_this_frame, 0);
        assert_eq!(after_reversal.unloaded_cells, 2);
        assert_eq!(
            after_reversal.requests_submitted, after_crossing.requests_submitted,
            "a revived cell keeps its root, so it must not be requested again"
        );
        assert_eq!(resident_root_count(&mut app), roots_after_crossing);
        assert_eq!(after_reversal.streaming_invariant_failures, 0);
    }

    /// A LOD chunk backlog leaves under the *same* budget the cells drain by, cells first: the
    /// crossing that retires a whole exterior's distant terrain unloads a couple of chunks a frame
    /// instead of the one flush that put a whole world's LOD on the crossing frame.
    #[test]
    fn cells_and_lod_chunks_drain_under_the_one_unload_budget() {
        let (mut app, _directory) = streaming_test_app(crossing_config(2));
        spawn_camera(&mut app, IVec2::ZERO);
        app.update();
        // Two cells far behind the camera, retiring but not reached by the budget's first frame:
        // what a flight leaves. They are planted out of the keep radius, so `plan_cells` revives
        // neither.
        spawn_mock_cell(&mut app, 20, 20, false);
        spawn_mock_cell(&mut app, 21, 20, false);
        // Three chunks of an exterior the camera has just walked inside.
        let chunks: Vec<Entity> = [(0, 0), (1, 0), (2, 0)]
            .into_iter()
            .map(|(x, y)| {
                lod::LodStreaming::spawn_retiring_chunk_for_test(
                    app.world_mut(),
                    60,
                    shared::lod::ChunkAnchor::new(x, y),
                    shared::lod::LodOrigin::new(0, 0),
                )
            })
            .collect();

        app.update();
        let mut per_frame = Vec::new();
        for frame in 1..=3 {
            let metrics = streaming_metrics(&app);
            let roots_this_frame =
                metrics.despawns_this_frame + metrics.lod_despawns_this_frame as u64;
            per_frame.push((
                metrics.despawns_this_frame,
                metrics.lod_despawns_this_frame as u64,
            ));
            assert!(
                roots_this_frame <= 2,
                "frame {frame} unloaded {roots_this_frame} roots against a budget of 2"
            );
            assert_eq!(metrics.streaming_invariant_failures, 0, "frame {frame}");
            if frame < 3 {
                app.update();
            }
        }
        assert_eq!(
            per_frame,
            vec![(2, 0), (0, 2), (0, 1)],
            "cells go first off the one clock, and the chunks take what is left of it"
        );
        let metrics = streaming_metrics(&app);
        assert_eq!(metrics.unloaded_cells, 2);
        assert_eq!(metrics.unloaded_lod_chunks, 3);
        assert_eq!(metrics.retiring_cells, 0);
        assert_eq!(metrics.retiring_lod_chunks, 0);
        assert_eq!(metrics.peak_retiring_lod_chunks, 3);
        assert_eq!(metrics.max_lod_despawns_per_frame, 2);
        for chunk in &chunks {
            assert!(
                app.world().get_entity(*chunk).is_err(),
                "a drained chunk's root is despawned"
            );
        }
        assert_eq!(
            app.world()
                .resource::<lod::LodStreaming>()
                .retiring_chunks_for_test(),
            0
        );
    }

    /// The wall-clock budget paces the chunks the same way it paces cells: a backlog drains a
    /// piece a frame - always at least one root, never all of it - so no single frame pays a
    /// whole world's LOD.
    #[test]
    fn the_time_budget_spreads_a_lod_chunk_backlog_over_frames() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            max_unload_ms_per_frame: 0.0001,
            ..crossing_config(0)
        });
        spawn_camera(&mut app, IVec2::ZERO);
        app.update();
        for x in 0..4 {
            let root = lod::LodStreaming::spawn_retiring_chunk_for_test(
                app.world_mut(),
                60,
                shared::lod::ChunkAnchor::new(x, 0),
                shared::lod::LodOrigin::new(0, 0),
            );
            // A chunk's root carries a subtree in the real world; a handful of children is what
            // makes a single despawn cost more than the 100 ns budget below.
            for _ in 0..200 {
                app.world_mut().spawn((Transform::default(), ChildOf(root)));
            }
        }
        let mut per_frame = Vec::new();
        loop {
            app.update();
            let metrics = streaming_metrics(&app);
            per_frame.push(metrics.lod_despawns_this_frame);
            assert!(
                metrics.lod_despawns_this_frame >= 1,
                "the drain always goes forward, got {per_frame:?}"
            );
            assert_eq!(metrics.streaming_invariant_failures, 0);
            if metrics.retiring_lod_chunks == 0 {
                break;
            }
            assert!(
                per_frame.len() < 10,
                "a 100 ns budget must still finish a four-chunk backlog: {per_frame:?}"
            );
        }
        assert_eq!(per_frame.iter().sum::<usize>(), 4);
        assert!(
            per_frame.len() > 1,
            "the budget must spread the backlog, not unload it in one frame: {per_frame:?}"
        );
        let metrics = streaming_metrics(&app);
        assert_eq!(metrics.unloaded_lod_chunks, 4);
        assert_eq!(metrics.retiring_lod_chunks, 0);
    }

    /// A cell left behind by distance is drawn no longer the frame it retires: the pacer spreads
    /// the *unload* over frames, never the drawing, so a paced unload looks like an unpaced one -
    /// off every camera the frame the cell leaves the keep-set, then despawned when the budget
    /// reaches it. The root's visibility is hidden and marked for the revive to undo.
    #[test]
    fn a_distance_retired_root_is_hidden_the_frame_it_retires() {
        let (mut app, _directory) = streaming_test_app(crossing_config(2));
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_steady_window(&mut app, IVec2::ZERO);
        app.update();

        move_camera(&mut app, IVec2::X);
        app.update();

        let world = app.world();
        let streaming = world.resource::<StreamingWorld>();
        let mut retiring = 0;
        for status in streaming.cells.values() {
            match status {
                CellStatus::Retiring { root, .. } => {
                    retiring += 1;
                    assert_eq!(
                        world.get::<Visibility>(*root),
                        Some(&Visibility::Hidden),
                        "a retiring root is still drawn the frame it retires"
                    );
                    assert!(
                        world.get::<RetireHidden>(*root).is_some(),
                        "the hide is marked, so the revive can undo exactly it"
                    );
                }
                CellStatus::Resident { root, .. } => assert_ne!(
                    world.get::<Visibility>(*root),
                    Some(&Visibility::Hidden),
                    "a resident cell beside the retirees keeps drawing"
                ),
                _ => {}
            }
        }
        assert_eq!(
            retiring, 5,
            "the crossing rode the column out in the same frame, as the budget path says"
        );
    }

    /// The visibility a retire took is given back the frame the cell revives - and only it: the
    /// marker goes with it, so a later retire hides the root afresh rather than trusting a stale
    /// original.
    #[test]
    fn a_revived_distance_retired_root_draws_again() {
        let (mut app, _directory) = streaming_test_app(crossing_config(2));
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_steady_window(&mut app, IVec2::ZERO);
        app.update();

        move_camera(&mut app, IVec2::X);
        app.update();
        let retiring: Vec<Entity> = {
            let streaming = app.world().resource::<StreamingWorld>();
            streaming
                .cells
                .values()
                .filter_map(|status| match status {
                    CellStatus::Retiring { root, .. } => Some(*root),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(retiring.len(), 5);
        for root in &retiring {
            assert_eq!(
                app.world().get::<Visibility>(*root),
                Some(&Visibility::Hidden),
                "hidden while it waits for the budget"
            );
        }

        move_camera(&mut app, IVec2::NEG_X);
        app.update();

        assert_eq!(streaming_metrics(&app).revived_cells, 5);
        for root in &retiring {
            assert_eq!(
                app.world().get::<Visibility>(*root),
                Some(&Visibility::Inherited),
                "the visibility the root had comes back on the revive"
            );
            assert!(
                app.world().get::<RetireHidden>(*root).is_none(),
                "and the marker with it, so a later retire hides afresh"
            );
        }
    }

    /// Every retiree is off the picture on *every* frame it waits, not just the frame it retires:
    /// under a one-a-frame count budget a three-cell backlog drains over three frames, and none of
    /// the cells the budget holds is ever still drawn.
    #[test]
    fn every_retiree_stays_hidden_until_the_budget_despawns_it() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            stream_radius: 1,
            terrain_radius: 1,
            unload_radius: 1,
            max_cell_unloads_per_frame: 1,
            max_unload_ms_per_frame: 0.0,
            ..EngineConfig::default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        // Three resident roots well outside the keep-set, so the plan retires all three at once.
        let roots: Vec<Entity> = (10..13)
            .map(|grid_x| spawn_mock_cell(&mut app, grid_x, 0, true))
            .collect();

        app.update();
        assert_eq!(
            streaming_metrics(&app).retiring_cells,
            2,
            "the budget rode one of the three out the same frame they retired"
        );

        let mut frames = 0;
        loop {
            let world = app.world();
            let mut present = 0;
            for root in &roots {
                if let Some(visibility) = world.get::<Visibility>(*root) {
                    present += 1;
                    assert_eq!(
                        *visibility,
                        Visibility::Hidden,
                        "frame {frames}: a retiree is still drawn while the budget holds it"
                    );
                }
            }
            if present == 0 {
                break;
            }
            frames += 1;
            assert!(
                frames <= roots.len(),
                "the backlog never drained: {present} still waiting"
            );
            app.update();
        }
        assert!(
            frames >= 2,
            "the drain took more than one frame, as the pacer is meant to"
        );
        assert_eq!(streaming_metrics(&app).unloaded_cells, 3);
    }

    /// The revival keeps the very entity that was drawing the cell: not a despawn and a respawn.
    #[test]
    fn a_revived_retiring_cell_keeps_its_root_entity() {
        let (mut app, _directory) = streaming_test_app(crossing_config(1));
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_steady_window(&mut app, IVec2::ZERO);
        app.update();
        move_camera(&mut app, IVec2::X);
        app.update();

        let retiring: Vec<(CellKey, Entity)> = {
            let streaming = app.world().resource::<StreamingWorld>();
            let mut retiring: Vec<_> = streaming
                .cells
                .iter()
                .filter_map(|(key, status)| match status {
                    CellStatus::Retiring { root, .. } => Some((*key, *root)),
                    _ => None,
                })
                .collect();
            retiring.sort_by_key(|(key, _)| cell_order_key(*key));
            retiring
        };
        assert_eq!(
            retiring.len(),
            6,
            "the budget took one of the column's seven"
        );

        move_camera(&mut app, IVec2::NEG_X);
        app.update();
        let streaming = app.world().resource::<StreamingWorld>();
        for (key, root) in &retiring {
            match streaming.cells.get(key) {
                Some(CellStatus::Resident { root: now, .. }) => assert_eq!(
                    now, root,
                    "{key:?} came back under a different root: a despawn and a respawn"
                ),
                other => panic!("{key:?} was not revived in place: {other:?}"),
            }
            assert!(
                app.world().get_entity(*root).is_ok(),
                "{key:?} was despawned and respawned instead of revived"
            );
        }
    }

    /// A cell whose replacement is in flight when it falls out by distance retires holding the root
    /// it was drawn by, at the tier that root was committed at; when the camera comes back it is
    /// replaced through the same path a resident's tier change takes, on the same root entity.
    #[test]
    fn a_retiring_cell_loading_a_replacement_keeps_the_root_it_draws() {
        let (mut app, _directory) = streaming_test_app(crossing_config(1));
        spawn_camera(&mut app, IVec2::ZERO);
        // The cell is drawn by a terrain-only root while its full replacement is in flight.
        let key = exterior_cell(7, 0);
        let root = spawn_mock_cell(&mut app, 7, 0, true);
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                key,
                CellStatus::Loading {
                    generation: 1,
                    detail: CellDetail::Full,
                    replaced: Some(root),
                },
            );
        // A resident cell closer in the budget's order takes the frame's one unload.
        let decoy = spawn_mock_cell(&mut app, 6, 0, true);
        app.update();

        assert!(
            app.world().get_entity(decoy).is_err(),
            "the decoy took the only unload this frame"
        );
        match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Retiring { root: now, detail }) => {
                assert_eq!(*now, root, "a retiring cell keeps the root it was drawn by");
                assert_eq!(
                    *detail,
                    CellDetail::Terrain,
                    "and the tier that root was committed at, not the one it is loading"
                );
            }
            other => panic!("the cell did not retire onto its old root: {other:?}"),
        }
        assert!(
            app.world().get_entity(root).is_ok(),
            "its root must not be despawned while it waits for the budget"
        );

        // The camera comes back: the plan asks for the tier the cell does not hold, and the root it
        // kept becomes the replacement's root. Nothing is spawned and nothing is despawned.
        move_camera(&mut app, IVec2::new(7, 0));
        app.update();
        match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Loading {
                detail, replaced, ..
            }) => {
                assert_eq!(*detail, CellDetail::Full);
                assert_eq!(
                    *replaced,
                    Some(root),
                    "the cell is replaced through the root it kept, not a second one"
                );
            }
            other => panic!("the returned cell did not take the replaced path: {other:?}"),
        }
        assert!(app.world().get_entity(root).is_ok());
        assert_eq!(streaming_metrics(&app).despawns_this_frame, 0);
    }

    /// A cell pinned between the plan and the pacer is revived by the pin, not despawned and
    /// requested again: the fixture's "load by id and it stays" invariant, under the pacer.
    #[test]
    fn a_pinned_retiring_cell_is_revived_with_its_root() {
        let (mut app, _directory) = streaming_test_app(crossing_config(1));
        spawn_camera(&mut app, IVec2::ZERO);
        // An interior is never paced, so one that nothing wants goes the frame it falls out.
        let unpinned = spawn_mock_interior(&mut app, 98);
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                CellKey::Interior(98),
                CellStatus::Retiring {
                    root: unpinned,
                    detail: CellDetail::Full,
                },
            );
        app.update();
        assert!(
            app.world().get_entity(unpinned).is_err(),
            "an interior left behind goes at once, whatever state it was in"
        );
        assert_eq!(streaming_metrics(&app).despawns_this_frame, 0);

        // The same cell again, but pinned this frame: `request_cell` takes it out of the backlog.
        let root = spawn_mock_interior(&mut app, 99);
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                CellKey::Interior(99),
                CellStatus::Retiring {
                    root,
                    detail: CellDetail::Full,
                },
            );
        let revived = {
            let world = app.world_mut();
            let database = world.remove_resource::<WorldDatabase>().unwrap();
            let mut metrics = world.remove_resource::<StreamingMetrics>().unwrap();
            let mut profiler = world.remove_resource::<ProfilingState>().unwrap();
            world.resource_mut::<StreamingWorld>().request_cell(
                &database,
                CellKey::Interior(99),
                &mut metrics,
                &mut profiler,
            );
            let revived = metrics.revived_cells;
            world.insert_resource(database);
            world.insert_resource(metrics);
            world.insert_resource(profiler);
            revived
        };
        assert_eq!(revived, 1, "pinning a retiring cell revives it on the spot");
        app.update();
        assert!(
            app.world().get_entity(root).is_ok(),
            "the pinned cell kept the root it was drawn by"
        );
        assert!(
            matches!(
                app.world()
                    .resource::<StreamingWorld>()
                    .cells
                    .get(&CellKey::Interior(99)),
                Some(CellStatus::Resident { root: now, .. }) if *now == root
            ),
            "the pinned cell is resident under the same root"
        );
        let metrics = streaming_metrics(&app);
        assert_eq!(metrics.despawns_this_frame, 0);
        assert_eq!(metrics.retiring_cells, 0);
        assert_eq!(metrics.unloaded_cells, 1, "only the unpinned one went");
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    #[test]
    fn retire_backlog_bound_is_one_window_at_the_unload_radius() {
        assert_eq!(retire_backlog_bound(4), 81);
        assert_eq!(retire_backlog_bound(3), 49);
        assert_eq!(retire_backlog_bound(1), 9);
        assert_eq!(retire_backlog_bound(0), 1);
    }

    #[test]
    fn a_backlog_past_the_bound_unloads_every_retiring_cell_at_once() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            stream_radius: 1,
            terrain_radius: 1,
            unload_radius: 1,
            max_cell_unloads_per_frame: 2,
            // This test paces by the cell count, the path the time budget replaces.
            max_unload_ms_per_frame: 0.0,
            ..EngineConfig::default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        // Under the bound the pacer keeps to the budget: 8 cells wait, 2 go per frame.
        for grid_x in 10..18 {
            spawn_mock_cell(&mut app, grid_x, 0, false);
        }
        app.update();
        let paced = streaming_metrics(&app);
        assert_eq!(paced.retire_backlog_overflows, 0);
        assert_eq!(paced.despawns_this_frame, 2);
        assert_eq!(paced.retiring_cells, 6);
        assert_eq!(
            resident_root_count(&mut app),
            6,
            "the waiters keep their roots"
        );

        // Past the bound the pacer gives up pacing and takes the whole backlog in one frame.
        for grid_x in 20..40 {
            spawn_mock_cell(&mut app, grid_x, 0, false);
        }
        app.update();
        let overflowed = streaming_metrics(&app);
        assert_eq!(overflowed.retire_backlog_overflows, 1);
        assert_eq!(overflowed.despawns_this_frame, 26);
        assert_eq!(overflowed.retiring_cells, 0);
        assert_eq!(overflowed.unloaded_cells, 28);
        assert_eq!(resident_root_count(&mut app), 0);
        assert_eq!(overflowed.streaming_invariant_failures, 0);
    }

    /// A retiring cell whose subtree is heavy enough that despawning it costs microseconds - far
    /// more than the time-budget tests' 100 ns - so the drain takes one cell a frame and the frame
    /// counts in those tests are exact however fast the clock reads.
    fn spawn_heavy_mock_cell(app: &mut App, grid_x: i32, grid_y: i32, children: usize) -> Entity {
        let root = app
            .world_mut()
            .spawn((
                CellRef(cell_id_of(grid_x, grid_y)),
                StreamedCellRoot,
                ExteriorCellGrid(IVec2::new(grid_x, grid_y)),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        for _ in 0..children {
            app.world_mut().spawn((Transform::default(), ChildOf(root)));
        }
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                exterior_cell(grid_x, grid_y),
                CellStatus::Retiring {
                    root,
                    detail: CellDetail::Full,
                },
            );
        root
    }

    /// A time-budget config at the bound's smallest window: 9 cells, at `unload_radius` 1. The
    /// budget of 100 ns is spent by any one subtree, so a paced frame takes one heavy cell.
    fn unload_time_config() -> EngineConfig {
        EngineConfig {
            stream_radius: 1,
            terrain_radius: 1,
            unload_radius: 1,
            max_cell_unloads_per_frame: 0,
            max_unload_ms_per_frame: 0.0001,
            ..EngineConfig::default()
        }
    }

    /// A crossing-shaped backlog - far more retiring cells than the bound's one window - drains
    /// paced at the time budget: every frame takes one budgeted cell, nothing un-bounds, and the
    /// crossing-drain frames are counted apart from the overflows, which stay at zero.
    #[test]
    fn a_crossing_backlog_drains_over_frames_at_the_time_budget() {
        let (mut app, _directory) = streaming_test_app(unload_time_config());
        spawn_camera(&mut app, IVec2::ZERO);
        // 120 retiring exteriors, more than twice the bound of 9: the shape the old override
        // unloaded all at once, on the one frame that could least afford it.
        for grid_x in 10..130 {
            spawn_heavy_mock_cell(&mut app, grid_x, 0, 200);
        }
        let mut frames = 0usize;
        loop {
            app.update();
            let metrics = streaming_metrics(&app);
            frames += 1;
            assert_eq!(
                metrics.despawns_this_frame, 1,
                "frame {frames} drained {} cells against a 100 ns budget",
                metrics.despawns_this_frame
            );
            assert_eq!(metrics.retire_backlog_overflows, 0, "frame {frames}");
            assert!(
                metrics.retiring_cells == 120 - frames,
                "frame {frames} left {} retiring",
                metrics.retiring_cells
            );
            if metrics.retiring_cells == 0 {
                break;
            }
            assert!(frames < 121, "the backlog never drained");
        }
        assert_eq!(frames, 120, "one cell a frame drains the 120 in 120 frames");
        let metrics = streaming_metrics(&app);
        assert_eq!(metrics.unloaded_cells, 120);
        // Every frame whose backlog sat above the bound of 9 counted as a drain frame: the first
        // 111 of the 120 (the last backlog above it is 10, on frame 111).
        assert_eq!(metrics.retire_backlog_drains, 111);
        assert_eq!(metrics.retire_backlog_overflows, 0);
        assert_eq!(resident_root_count(&mut app), 0);
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    /// A backlog that grows across frames - flight retiring cells faster than the budget drains
    /// them - trips the escape exactly once, and the frame it trips takes the whole backlog at
    /// once. The burst before it, which shrinks every frame, stays paced.
    #[test]
    fn a_backlog_that_grows_across_frames_trips_the_escape_once() {
        let (mut app, _directory) = streaming_test_app(unload_time_config());
        spawn_camera(&mut app, IVec2::ZERO);
        // The burst: 12 retiring cells, above the bound of 9 but never drained before, so there is
        // no last frame's backlog to have failed against. It drains paced and leaves its remainder
        // as the next frame's baseline.
        for grid_x in 10..22 {
            spawn_heavy_mock_cell(&mut app, grid_x, 0, 200);
        }
        app.update();
        let burst = streaming_metrics(&app);
        assert_eq!(burst.despawns_this_frame, 1);
        assert_eq!(burst.retiring_cells, 11);
        assert_eq!(burst.retire_backlog_overflows, 0);
        assert_eq!(burst.retire_backlog_drains, 1);

        // The growth: 12 more retire while the drain took one. The backlog is above the bound and
        // above what the last frame left, so the escape un-bounds and the whole backlog goes now.
        for grid_x in 22..34 {
            spawn_heavy_mock_cell(&mut app, grid_x, 0, 200);
        }
        app.update();
        let escaped = streaming_metrics(&app);
        assert_eq!(
            escaped.despawns_this_frame, 23,
            "un-bound: the whole backlog in one frame"
        );
        assert_eq!(escaped.retiring_cells, 0);
        assert_eq!(escaped.retire_backlog_overflows, 1);
        assert_eq!(
            escaped.retire_backlog_drains, 1,
            "the escape frame is not a drain frame"
        );
        assert_eq!(resident_root_count(&mut app), 0);

        // After the escape the pacer is paced again: a fresh backlog below the bound drains at the
        // budget, and no second trip is counted.
        for grid_x in 40..43 {
            spawn_heavy_mock_cell(&mut app, grid_x, 0, 200);
        }
        app.update();
        let settled = streaming_metrics(&app);
        assert_eq!(settled.despawns_this_frame, 1);
        assert_eq!(settled.retiring_cells, 2);
        assert_eq!(settled.retire_backlog_overflows, 1);
        assert_eq!(
            settled.retire_backlog_drains, 1,
            "3 cells sit below the bound"
        );
        assert_eq!(settled.streaming_invariant_failures, 0);
    }

    /// A space change is not paced: crossing into an interior takes the exterior window and the
    /// space left behind with it in one frame, with nothing left retiring.
    #[test]
    fn a_crossing_into_an_interior_unloads_the_space_it_left_in_one_frame() {
        let (mut app, _directory) = streaming_test_app(crossing_config(2));
        spawn_camera(&mut app, IVec2::ZERO);
        let left_behind: Vec<Entity> = (0..3)
            .map(|grid_x| spawn_mock_cell(&mut app, grid_x, 0, true))
            .collect();
        app.update();
        assert_eq!(resident_root_count(&mut app), 3);

        *app.world_mut().resource_mut::<ActiveSpace>() = ActiveSpace {
            worldspace_id: Some(60),
            interior: Some(99),
        };
        let interior = spawn_mock_interior(&mut app, 99);
        app.update();

        let metrics = streaming_metrics(&app);
        assert_eq!(
            metrics.despawns_this_frame, 0,
            "a space change is not paced: nothing waited for the budget"
        );
        assert_eq!(metrics.retiring_cells, 0);
        for root in &left_behind {
            assert!(
                app.world().get_entity(*root).is_err(),
                "an exterior of the space the camera left must go the frame it falls out"
            );
        }
        assert!(app.world().get_entity(interior).is_ok());
        assert!(
            planned_keys(&app).contains(&CellKey::Interior(99)),
            "the interior the camera is in is the whole plan"
        );
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    /// A cell already waiting in the retire backlog when the camera changes space unloads at once:
    /// the delay it was granted for a distance fall-out does not survive the space change.
    #[test]
    fn a_space_change_unloads_a_retiring_cell_at_once() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            stream_radius: 1,
            terrain_radius: 1,
            unload_radius: 1,
            max_cell_unloads_per_frame: 1,
            // This test paces by the cell count, the path the time budget replaces.
            max_unload_ms_per_frame: 0.0,
            ..EngineConfig::default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        // Two cells outside the unload radius: one takes its turn, one stays in the backlog.
        let first = spawn_mock_cell(&mut app, 3, 0, true);
        let waiting = spawn_mock_cell(&mut app, 3, 1, true);
        app.update();
        let retiring = streaming_metrics(&app);
        assert_eq!(retiring.retiring_cells, 1);
        assert_eq!(retiring.despawns_this_frame, 1);
        assert!(app.world().get_entity(first).is_err());
        assert!(app.world().get_entity(waiting).is_ok());

        // The camera crosses into an interior: the waiting cell's space is gone.
        *app.world_mut().resource_mut::<ActiveSpace>() = ActiveSpace {
            worldspace_id: Some(60),
            interior: Some(99),
        };
        spawn_mock_interior(&mut app, 99);
        app.update();

        assert!(
            app.world().get_entity(waiting).is_err(),
            "a retiring cell of a space the camera left cannot outlive the crossing"
        );
        let metrics = streaming_metrics(&app);
        assert_eq!(metrics.despawns_this_frame, 0);
        assert_eq!(metrics.retiring_cells, 0);
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    /// A full-detail cell the door gate still counts as resident (`is_resident`) while it waits
    /// for the pacer, so a gate or a `--shots` pose waiting on it does not give up on a cell that
    /// is still there - including one that is *not* pre-streamed behind a door. That is gate
    /// visibility, not render visibility: `isolate_cells` re-classifies the roots every frame and
    /// a retiring cell renders as `CellRole::Hidden`, drawn by nothing. A ring cell, which holds
    /// only its landscape, is not resident in either state.
    #[test]
    fn a_retiring_full_destination_is_still_resident() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            stream_radius: 1,
            terrain_radius: 1,
            unload_radius: 1,
            max_cell_unloads_per_frame: 1,
            // This test paces by the cell count, the path the time budget replaces.
            max_unload_ms_per_frame: 0.0,
            ..EngineConfig::default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_mock_cell(&mut app, 3, 0, true);
        let waiting_key = exterior_cell(3, 1);
        spawn_mock_cell(&mut app, 3, 1, true);
        app.update();

        let streaming = app.world().resource::<StreamingWorld>();
        assert!(
            matches!(
                streaming.cells.get(&waiting_key),
                Some(CellStatus::Retiring { .. })
            ),
            "the cell is waiting for its turn in the unload budget"
        );
        assert!(
            streaming.is_resident(&waiting_key),
            "a retiring full-detail cell is still drawn, so a door gate must still see it"
        );

        // The other branches of the same rule: a tier that is not full, in either state.
        let mut streaming = app.world_mut().resource_mut::<StreamingWorld>();
        for detail in [CellDetail::Terrain, CellDetail::Full] {
            streaming.cells.insert(
                CellKey::Interior(7),
                CellStatus::Retiring {
                    root: Entity::PLACEHOLDER,
                    detail,
                },
            );
            assert_eq!(
                streaming.is_resident(&CellKey::Interior(7)),
                detail == CellDetail::Full,
                "a retiring interior at {detail:?}"
            );
        }
    }

    /// A cell that moves from the ring into the full-detail grid and back, as the camera crosses a
    /// cell boundary: the plan asks for it again at the new detail, and the tier change is never a
    /// hole - the root the cell has keeps drawing it until its replacement is committed, and the
    /// cell still has exactly one root at every point.
    #[test]
    fn moving_the_camera_upgrades_and_downgrades_a_ring_cell_without_duplicating_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database(&path);
        let config = EngineConfig {
            stream_radius: 1,
            terrain_radius: 3,
            ..EngineConfig::default()
        };
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, plan_cells);
        // The camera cell is (0, 0): the plan wants (0..3) in full and the ring out to 3.
        let camera = app
            .world_mut()
            .spawn((Transform::default(), StreamingCamera))
            .id();
        app.update();

        let ring_cell = exterior_cell(2, 0);
        let detail_of = |app: &App, key: CellKey| match app
            .world()
            .resource::<StreamingWorld>()
            .cells
            .get(&key)
        {
            Some(CellStatus::Loading { detail, .. })
            | Some(CellStatus::Resident { detail, .. }) => Some(*detail),
            _ => None,
        };
        let cell_roots = |app: &mut App| {
            let mut roots = app.world_mut().query_filtered::<Entity, (
                With<CellRef>,
                Or<(With<StreamedCellRoot>, With<DistantTerrainRoot>)>,
            )>();
            roots.iter(app.world()).count()
        };
        assert_eq!(
            detail_of(&app, ring_cell),
            Some(CellDetail::Terrain),
            "two cells out is the ring"
        );
        assert_eq!(detail_of(&app, exterior_cell(1, 0)), Some(CellDetail::Full));

        // A committed terrain-only cell for the ring cell: a root of its own, and a reference under
        // it, both of which belong to the terrain tier.
        let root = app
            .world_mut()
            .spawn((DistantTerrainRoot, CellRef(10), Transform::default()))
            .id();
        app.world_mut().spawn((ChildOf(root), FormId(0x30)));
        consume_mock_cell_answer(&mut app, ring_cell);
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                ring_cell,
                CellStatus::Resident {
                    root,
                    detail: CellDetail::Terrain,
                },
            );

        // The camera steps one cell towards it. Two cells out is now the inner grid, which is a
        // full cell: the cell is asked for again in full, and keeps drawing the terrain it has
        // until that answer is committed - the camera never sees a hole where the cell is.
        //
        // Every other cell the plan wanted is forgotten first, so the tier-change counters below
        // count this cell's change and not the other cells the camera's step moved across the
        // inner grid's edge in the same frame.
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .retain(|key, _| *key == ring_cell);
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation = Vec3::new(CELL_SIZE, 0.0, 0.0);
        app.update();

        assert_eq!(
            detail_of(&app, ring_cell),
            Some(CellDetail::Full),
            "the cell the camera is about to stand in is loaded in full"
        );
        assert_eq!(app.world().resource::<StreamingMetrics>().cells_upgraded, 1);
        assert!(
            app.world().get_entity(root).is_ok(),
            "the terrain-only root keeps drawing the cell while the full cell loads"
        );
        assert!(
            matches!(
                app.world()
                    .resource::<StreamingWorld>()
                    .cells
                    .get(&ring_cell),
                Some(CellStatus::Loading {
                    replaced: Some(replaced),
                    ..
                }) if *replaced == root
            ),
            "the root that draws the cell is the one the loading cell is replacing"
        );
        assert_eq!(cell_roots(&mut app), 1, "one root for the cell, never two");

        // The full cell is committed, standing in for the commit the way the real one does it: the
        // root that was drawing the cell goes in the same frame the new one arrives, so the cell is
        // never drawn twice and never dropped.
        app.world_mut().entity_mut(root).despawn();
        let upgraded = app
            .world_mut()
            .spawn((StreamedCellRoot, CellRef(10), Transform::default()))
            .id();
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .retain(|key, _| *key == ring_cell);
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation = Vec3::ZERO;
        consume_mock_cell_answer(&mut app, ring_cell);
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                ring_cell,
                CellStatus::Resident {
                    root: upgraded,
                    detail: CellDetail::Full,
                },
            );
        app.update();

        // Stepping back downgrades it the other way round, with the same guarantee.
        assert_eq!(
            detail_of(&app, ring_cell),
            Some(CellDetail::Terrain),
            "the cell the camera has left is terrain only"
        );
        assert_eq!(
            app.world().resource::<StreamingMetrics>().cells_downgraded,
            1
        );
        assert!(
            app.world().get_entity(upgraded).is_ok(),
            "the full root keeps drawing the cell while the terrain-only cell loads"
        );
        assert_eq!(
            cell_roots(&mut app),
            1,
            "the cell is still drawn by exactly one root"
        );
    }

    /// A cell that failed at one tier is asked for again at the other, and never at the same one:
    /// a ring grid that held nothing is loaded in full when the camera reaches it, while the
    /// hundreds of empty grids around the camera are not asked about again every frame.
    #[test]
    fn a_failed_cell_is_asked_for_again_at_the_other_tier_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database(&path);
        let config = EngineConfig {
            stream_radius: 1,
            terrain_radius: 2,
            ..EngineConfig::default()
        };
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, plan_cells);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        // The camera is in cell (0, 0): its own cell and the eight around it are wanted in full,
        // everything out to two cells is the ring. These four failed as the plan last saw them.
        for (key, detail) in [
            // The camera's own cell, which the ring answered with nothing.
            (exterior_cell(0, 0), CellDetail::Terrain),
            // Inside the inner grid, failed in full.
            (exterior_cell(1, 0), CellDetail::Full),
            // In the ring, failed as terrain-only: the case of the hundreds of empty grids.
            (exterior_cell(0, 2), CellDetail::Terrain),
            // In the ring, failed in full: the landscape may still be drawable.
            (exterior_cell(-2, 0), CellDetail::Full),
        ] {
            app.world_mut()
                .resource_mut::<StreamingWorld>()
                .cells
                .insert(
                    key,
                    CellStatus::Failed {
                        detail,
                        attempts: 1,
                        reason: "failed for test".to_owned(),
                        retry_at: None,
                        replaced: None,
                    },
                );
        }
        app.update();

        let status = |app: &App, key: CellKey| match app
            .world()
            .resource::<StreamingWorld>()
            .cells
            .get(&key)
        {
            Some(CellStatus::Loading { detail, .. }) => format!("loading {detail:?}"),
            Some(CellStatus::Resident { detail, .. }) => format!("resident {detail:?}"),
            Some(CellStatus::Retiring { detail, .. }) => format!("retiring {detail:?}"),
            Some(CellStatus::Failed { detail, .. }) => format!("failed {detail:?}"),
            None => "gone".to_owned(),
        };
        assert_eq!(
            status(&app, exterior_cell(0, 0)),
            "loading Full",
            "the cell the camera stands in is asked for in full even though the ring found nothing"
        );
        assert_eq!(
            status(&app, exterior_cell(1, 0)),
            "failed Full",
            "a full cell that failed in full is not asked for again every frame"
        );
        assert_eq!(
            status(&app, exterior_cell(0, 2)),
            "failed Terrain",
            "a ring grid with nothing in it is not asked for again every frame"
        );
        assert_eq!(
            status(&app, exterior_cell(-2, 0)),
            "loading Terrain",
            "a full cell that failed can still be drawn as terrain by the ring, once"
        );
    }

    /// The interior cell `write_plan_database` writes.
    const INTERIOR_CELL: u32 = 99;

    /// The interior the camera stands in is asked for again after a failed full request - a bounded
    /// number of times, and then never again.
    ///
    /// It is the one cell with no other tier to fall back to: an exterior that failed in full is
    /// drawn as terrain-only instead (`full_failed`), but an interior has no landscape of its own,
    /// so without a retry the whole space the camera is in would stay empty for as long as it stood
    /// there.
    #[test]
    fn a_failed_interior_is_asked_for_again_while_the_camera_stands_in_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database(&path);
        let mut app = App::new();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: Some(INTERIOR_CELL),
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, plan_cells);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        let status = |app: &App| match app
            .world()
            .resource::<StreamingWorld>()
            .cells
            .get(&CellKey::Interior(INTERIOR_CELL))
        {
            Some(CellStatus::Loading { detail, .. }) => format!("loading {detail:?}"),
            Some(CellStatus::Resident { detail, .. }) => format!("resident {detail:?}"),
            Some(CellStatus::Retiring { detail, .. }) => format!("retiring {detail:?}"),
            Some(CellStatus::Failed { detail, .. }) => format!("failed {detail:?}"),
            None => "gone".to_owned(),
        };
        // What a failed answer leaves behind: the record `record_failed_cell` writes for a
        // transient database error, with the backoff already paid so the next plan pass may ask
        // again.
        let answer_failed = |app: &mut App| {
            consume_mock_cell_answer(app, CellKey::Interior(INTERIOR_CELL));
            app.world_mut()
                .resource_mut::<StreamingWorld>()
                .cells
                .insert(
                    CellKey::Interior(INTERIOR_CELL),
                    CellStatus::Failed {
                        detail: CellDetail::Full,
                        attempts: 1,
                        reason: "the interior could not be streamed".to_owned(),
                        retry_at: Some(Instant::now()),
                        replaced: None,
                    },
                );
        };

        answer_failed(&mut app);
        for attempt in 1..=MAX_CELL_LOAD_RETRIES {
            app.update();
            assert_eq!(
                status(&app),
                "loading Full",
                "the interior the camera stands in is asked for again, attempt {attempt}"
            );
            answer_failed(&mut app);
        }
        // The budget is spent: the last answer fails and the plan leaves the cell alone.
        app.update();
        assert_eq!(
            status(&app),
            "failed Full",
            "a cell whose data really is not there is not re-requested every frame"
        );
        app.update();
        assert_eq!(status(&app), "failed Full", "and it stays that way");
    }

    /// The interior the camera stands in is not failed by [`StreamingWorld::has_failed`] while the
    /// plan still owes it a retry: a caller that settled on the first failure (a `--shots` pose, a
    /// door's destination) would stop waiting for a cell that is about to be asked for again. Once
    /// the last retry has failed too, it is failed for good - and an interior the camera is not in,
    /// which is never retried, is failed at its first failure.
    #[test]
    fn a_retried_interior_is_not_failed_until_its_retries_run_out() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        write_plan_database(&path);
        let mut app = App::new();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: Some(INTERIOR_CELL),
            })
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, plan_cells);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        let key = CellKey::Interior(INTERIOR_CELL);
        let has_failed = |app: &App| app.world().resource::<StreamingWorld>().has_failed(&key);
        // What a failed answer leaves behind: the record `record_failed_cell` writes for a
        // transient database error, with the backoff already paid so the next plan pass may ask
        // again.
        let answer_failed = |app: &mut App| {
            consume_mock_cell_answer(app, CellKey::Interior(INTERIOR_CELL));
            app.world_mut()
                .resource_mut::<StreamingWorld>()
                .cells
                .insert(
                    key,
                    CellStatus::Failed {
                        detail: CellDetail::Full,
                        attempts: 1,
                        reason: "the interior could not be streamed".to_owned(),
                        retry_at: Some(Instant::now()),
                        replaced: None,
                    },
                );
        };

        // The first request, and every retry, fails: in each gap before the next retry the cell
        // is waited for, not given up on.
        app.update();
        for attempt in 0..=MAX_CELL_LOAD_RETRIES {
            answer_failed(&mut app);
            let retries_spent = attempt == MAX_CELL_LOAD_RETRIES;
            assert_eq!(
                has_failed(&app),
                retries_spent,
                "after failure {} of {}",
                attempt + 1,
                MAX_CELL_LOAD_RETRIES + 1
            );
            app.update();
        }
        assert!(has_failed(&app), "its budget spent, the failure is final");

        // The camera steps out to the worldspace: the same failed cell, no longer the interior it
        // stands in, is never retried, so its failure is final at once - whatever budget it has.
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .failed_retries
            .clear();
        *app.world_mut().resource_mut::<ActiveSpace>() = ActiveSpace {
            worldspace_id: Some(60),
            interior: None,
        };
        app.update();
        answer_failed(&mut app);
        assert!(
            has_failed(&app),
            "an interior the camera is not in is failed at its first failure"
        );
    }

    /// An exterior's failed full request reads as failed at once, as it always has: it is not
    /// asked for in full again while it stays planned, and is drawn as terrain-only instead.
    #[test]
    fn an_exterior_full_failure_is_failed_at_once() {
        let mut streaming = StreamingWorld {
            active_interior: Some(INTERIOR_CELL),
            ..StreamingWorld::default()
        };
        let key = exterior_cell(0, 0);
        streaming.cells.insert(
            key,
            CellStatus::Failed {
                detail: CellDetail::Full,
                attempts: 1,
                reason: "the cell could not be streamed".to_owned(),
                retry_at: Some(Instant::now() + Duration::from_secs(1)),
                replaced: None,
            },
        );
        assert!(streaming.has_failed(&key), "failed in full");
        streaming.full_failed.insert(
            key,
            FullFailure {
                attempts: 1,
                reason: "the cell could not be streamed".to_owned(),
                retry_at: Some(Instant::now() + Duration::from_secs(1)),
            },
        );
        streaming.cells.insert(
            key,
            CellStatus::Failed {
                detail: CellDetail::Terrain,
                attempts: 1,
                reason: "the ring holds nothing at this grid".to_owned(),
                retry_at: None,
                replaced: None,
            },
        );
        assert!(
            streaming.has_failed(&key),
            "and still failed while its terrain-only request is failed too"
        );
        streaming.full_failed.clear();
        assert!(
            !streaming.has_failed(&key),
            "a ring grid that held nothing as terrain-only has not failed in full"
        );
    }

    /// The plan asks for a saturated worker's cells one frame later, and a cell that is waiting for
    /// its replacement keeps the root it has: nothing is despawned before the request is accepted,
    /// so a full queue cannot leave the world with a cell it claims to be drawing and is not.
    #[test]
    fn a_saturated_request_queue_leaves_every_cell_drawn() {
        let config = EngineConfig {
            stream_radius: 1,
            terrain_radius: 2,
            ..EngineConfig::default()
        };
        let database = WorldDatabase::saturated_queue();
        // One slot, filled: the plan's first request is refused.
        assert!(
            database
                .try_request(DatabaseRequest::Load {
                    generation: 0,
                    key: exterior_cell(0, 0),
                    detail: CellDetail::Full,
                    queued_at: Instant::now(),
                })
                .is_ok()
        );
        assert!(
            !database
                .try_request(DatabaseRequest::Load {
                    generation: 0,
                    key: exterior_cell(0, 0),
                    detail: CellDetail::Full,
                    queued_at: Instant::now(),
                })
                .is_ok(),
            "the queue is full, which is the state the plan has to survive"
        );
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(database)
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, (plan_cells, validate_streaming_lifecycle).chain());
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        // The camera's own cell is resident as terrain-only, and the plan wants it in full.
        let root = app
            .world_mut()
            .spawn((DistantTerrainRoot, CellRef(1), Transform::default()))
            .id();
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                exterior_cell(0, 0),
                CellStatus::Resident {
                    root,
                    detail: CellDetail::Terrain,
                },
            );
        app.update();

        assert!(
            app.world().get_entity(root).is_ok(),
            "the root that draws the cell is still there: the request was never accepted"
        );
        assert!(matches!(
            app.world()
                .resource::<StreamingWorld>()
                .cells
                .get(&exterior_cell(0, 0)),
            Some(CellStatus::Resident {
                detail: CellDetail::Terrain,
                ..
            })
        ));
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .requests_submitted,
            0
        );
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.missing_cell_roots, 0);
        assert_eq!(metrics.orphaned_cell_roots, 0);
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    /// A tier change seen end to end, through the real plan and the real commit: the cell is drawn
    /// in every frame of it - no frame has the camera's cell missing - and the terrain-only root
    /// and the full root are never both in the world.
    #[test]
    fn a_tier_change_is_never_a_hole_in_the_world() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_terrain_cache(&cache_path, 1);
        let mut app = terrain_streaming_app(&database_path, &cache_path);
        // The camera starts in cell (1,-3), one cell west of cell 1 in (2,-3): that cell is the
        // ring's, so it streams as terrain only.
        let camera = app
            .world_mut()
            .spawn((Transform::from_xyz(-2048.0, 0.0, -2048.0), StreamingCamera))
            .id();
        let key = exterior_cell(2, -3);
        let detail_of = |app: &App| match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Loading { detail, .. })
            | Some(CellStatus::Resident { detail, .. })
            | Some(CellStatus::Retiring { detail, .. }) => Some(*detail),
            Some(CellStatus::Failed { .. }) => None,
            None => None,
        };
        run_until(&mut app, "the ring cell to be committed", |app| {
            matches!(
                app.world()
                    .resource::<StreamingWorld>()
                    .cells
                    .get(&exterior_cell(2, -3)),
                Some(CellStatus::Resident {
                    detail: CellDetail::Terrain,
                    ..
                })
            )
        });
        let terrain_root = match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Resident { root, detail }) if *detail == CellDetail::Terrain => *root,
            other => panic!("the ring cell is not resident as terrain: {other:?}"),
        };
        assert!(
            app.world()
                .get::<DistantTerrainRoot>(terrain_root)
                .is_some(),
            "the ring cell is drawn by a terrain-only root"
        );
        assert_eq!(roots_of(&mut app, 1), 1);

        // The camera steps into it. From that frame on the cell is the camera's own, which the plan
        // wants in full; the terrain the cell already has must keep being drawn until the full cell
        // is committed.
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation = Vec3::new(2048.0, 0.0, -2048.0);
        let mut frames = 0;
        loop {
            app.update();
            frames += 1;
            assert_eq!(
                roots_of(&mut app, 1),
                1,
                "frame {frames}: the cell is drawn by exactly one root from the frame the upgrade \
                 was asked for"
            );
            if detail_of(&app) == Some(CellDetail::Full)
                && matches!(
                    app.world().resource::<StreamingWorld>().cells.get(&key),
                    Some(CellStatus::Resident { .. })
                )
            {
                break;
            }
            assert!(frames < 600, "the full cell never committed");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let root = match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Resident { root, .. }) => *root,
            other => panic!("the cell is not resident in full: {other:?}"),
        };
        assert!(
            app.world().get::<StreamedCellRoot>(root).is_some(),
            "the cell is now a full cell, which the portal's isolation walks"
        );
        assert!(app.world().get::<DistantTerrainRoot>(root).is_none());
        assert_eq!(app.world().resource::<StreamingMetrics>().cells_upgraded, 1);
        assert_eq!(roots_of(&mut app, 1), 1);
    }

    /// A ring cell whose landscape does not validate is empty space for the ring, not a cell
    /// failure: a few hundred ring cells are requested at once, and one bad landscape among them
    /// must not read as a cell the camera can stand in failing to load. The same landscape, asked
    /// for by the full request that follows the camera into the cell, is a failure like any other -
    /// that is where real broken data is reported.
    #[test]
    fn a_ring_cell_with_an_invalid_landscape_is_empty_space_not_a_failure() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_invalid_terrain_cache(&cache_path, 1);
        let mut app = terrain_streaming_app(&database_path, &cache_path);
        let key = exterior_cell(2, -3);
        let commit = |app: &mut App, generation: u64, detail: CellDetail, what: &str| {
            app.world_mut()
                .resource_mut::<StreamingWorld>()
                .cells
                .insert(
                    key,
                    CellStatus::Loading {
                        generation,
                        detail,
                        replaced: None,
                    },
                );
            app.world_mut()
                .resource::<WorldDatabase>()
                .request(DatabaseRequest::Load {
                    generation,
                    key,
                    detail,
                    queued_at: Instant::now(),
                })
                .unwrap();
            run_until(app, what, |app| {
                !matches!(
                    app.world().resource::<StreamingWorld>().cells.get(&key),
                    Some(CellStatus::Loading { .. })
                )
            });
        };

        // The ring's request, which is what a cell at this distance gets.
        commit(&mut app, 1, CellDetail::Terrain, "the ring's answer");
        assert!(matches!(
            app.world().resource::<StreamingWorld>().cells.get(&key),
            Some(CellStatus::Failed {
                detail: CellDetail::Terrain,
                ..
            })
        ));
        {
            let metrics = app.world().resource::<StreamingMetrics>();
            assert_eq!(
                metrics.failed_cells, 0,
                "a ring cell with a bad landscape is not a cell failure"
            );
            assert_eq!(
                metrics.terrain_validation_failures, 0,
                "and it is not a terrain validation failure either"
            );
            assert_eq!(metrics.ring_cells_empty, 1);
            assert!(
                metrics.asset_failures.is_empty(),
                "the ring's bad landscape is not reported as a broken asset"
            );
        }
        assert_eq!(roots_of(&mut app, 1), 0, "nothing is drawn for it");

        // The same cell, asked for in full because the camera has arrived: now it is a failure.
        commit(&mut app, 2, CellDetail::Full, "the full request's answer");
        assert!(matches!(
            app.world().resource::<StreamingWorld>().cells.get(&key),
            Some(CellStatus::Failed {
                detail: CellDetail::Full,
                ..
            })
        ));
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.failed_cells, 1);
        assert_eq!(metrics.terrain_validation_failures, 1);
        assert_eq!(
            metrics.ring_cells_empty, 1,
            "the full request's failure is not counted as ring noise"
        );
        assert_eq!(metrics.asset_failures.len(), 1);
    }

    /// The streaming app over a cell cache that holds no landscape, with the camera in the
    /// fixture's cell: the cell is asked for in full, resolves, and is answered with no terrain.
    /// The directory is returned so the files outlive the app.
    fn terrain_less_full_cell_app(validate: bool) -> (App, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_empty_cell_cache(&cache_path);
        let mut app = terrain_streaming_app(&database_path, &cache_path);
        if validate {
            app.add_systems(Update, validate_streaming_lifecycle.after(collect_cells));
        }
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        run_until(&mut app, "the full cell to be committed", |app| {
            matches!(
                app.world()
                    .resource::<StreamingWorld>()
                    .cells
                    .get(&exterior_cell(2, -3)),
                Some(CellStatus::Resident {
                    detail: CellDetail::Full,
                    ..
                })
            )
        });
        (app, directory)
    }

    /// A full exterior cell whose answer carries no terrain is still committed as it always was -
    /// resident, no quadrants - but it is counted, once, and marked.
    #[test]
    fn a_full_exterior_cell_without_terrain_is_counted_once_and_stays_resident() {
        let (mut app, _directory) = terrain_less_full_cell_app(false);
        // More frames: a counter bumped by anything but the commit would keep climbing.
        for _ in 0..10 {
            app.update();
            std::thread::sleep(Duration::from_millis(5));
        }
        let key = exterior_cell(2, -3);
        let root = match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Resident {
                root,
                detail: CellDetail::Full,
            }) => *root,
            other => panic!("the terrain-less cell is not resident in full: {other:?}"),
        };
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .full_cells_without_terrain,
            1
        );
        assert_eq!(
            app.world().resource::<StreamingMetrics>().failed_cells,
            0,
            "the cell is held, not failed"
        );
        assert!(
            app.world().get::<TerrainLess>(root).is_some(),
            "the counted root carries the marker the lifecycle check reads"
        );
        assert_eq!(roots_of(&mut app, 1), 1);
        let patches = app
            .world_mut()
            .query_filtered::<(), With<TerrainPatch>>()
            .iter(app.world())
            .count();
        assert_eq!(patches, 0, "no terrain was answered, so none is drawn");
    }

    /// The lifecycle check has no complaint about a counted terrain-less cell: it is a known case,
    /// not a cell that lost its landscape.
    #[test]
    fn the_lifecycle_check_accepts_a_counted_terrain_less_cell() {
        let (mut app, _directory) = terrain_less_full_cell_app(true);
        // The validator runs after the commit that made the cell resident.
        app.update();
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(
            metrics.resident_roots, 1,
            "the check saw the terrain-less root"
        );
        assert_eq!(metrics.full_cells_without_terrain, 1);
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    /// A full exterior root that holds no terrain and was not counted as terrain-less lost its
    /// landscape after it was spawned: that is a lifecycle violation. The same cell with its
    /// landscape is not.
    #[test]
    fn the_lifecycle_check_flags_a_full_exterior_root_that_lost_its_terrain() {
        let key = exterior_cell(2, -3);
        let mut app = spawn_cells_app(
            vec![(key, untextured_fixture(1, 100.0))],
            StreamingMetrics::default(),
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .streaming_invariant_failures,
            0,
            "a cell holding its landscape is fine"
        );
        // The cell is already spawned and recorded; spawn nothing more.
        app.world_mut().resource_mut::<QueuedCells>().0.clear();
        let patches: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<TerrainPatch>>()
            .iter(app.world())
            .collect();
        assert_eq!(patches.len(), 4);
        for patch in patches {
            app.world_mut().entity_mut(patch).despawn();
        }
        app.update();
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .streaming_invariant_failures,
            1,
            "an uncounted full exterior root without terrain is a violation"
        );
    }

    /// A cell whose upgrade to full fails while the camera stands in it is drawn as terrain again,
    /// and is not asked for in full again: the failed request drops the terrain-only root it was
    /// replacing, and the cell must not stay a hole in the ground until the camera leaves it
    /// (audit finding ES-20260926-01), nor fail in full over and over.
    #[test]
    fn a_failed_full_upgrade_leaves_the_cell_drawn_as_terrain() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        // Without the reference table a full request fails and a terrain-only one does not: the
        // terrain-only answer never reads it.
        rusqlite::Connection::open(&database_path)
            .unwrap()
            .execute_batch(r#"DROP TABLE "references";"#)
            .unwrap();
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_terrain_cache(&cache_path, 1);
        let mut app = terrain_streaming_app(&database_path, &cache_path);
        let key = exterior_cell(2, -3);
        let resident_terrain = |app: &App| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Resident {
                    detail: CellDetail::Terrain,
                    ..
                })
            )
        };

        // The camera starts one cell east, so the cell is in the ring and drawn as terrain.
        let camera = app
            .world_mut()
            .spawn((Transform::from_xyz(CELL_SIZE, 0.0, 0.0), StreamingCamera))
            .id();
        run_until(&mut app, "the ring's terrain-only cell", resident_terrain);
        assert!(!app.world().resource::<StreamingWorld>().has_failed(&key));
        // The camera's own first cell is not in the database, and fails in full: counted from here.
        let failed_before = app.world().resource::<StreamingMetrics>().failed_cells;

        // The camera steps into the cell: it is asked for in full, and that request fails.
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation = Vec3::ZERO;
        run_until(&mut app, "the full request's failure", |app| {
            app.world().resource::<StreamingWorld>().has_failed(&key)
        });
        assert_eq!(
            app.world().resource::<StreamingMetrics>().failed_cells,
            failed_before + 1
        );

        // The landscape comes back as a terrain-only cell while the camera is still in it.
        run_until(
            &mut app,
            "the cell's landscape drawn again",
            resident_terrain,
        );
        assert_eq!(roots_of(&mut app, 1), 1, "the cell is drawn by one root");

        // And it stays that way: the full request is not repeated.
        let upgrades = app.world().resource::<StreamingMetrics>().cells_upgraded;
        for _ in 0..40 {
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(resident_terrain(&app), "the cell is still drawn as terrain");
        assert_eq!(roots_of(&mut app, 1), 1);
        let streaming = app.world().resource::<StreamingWorld>();
        assert!(
            streaming.has_failed(&key),
            "a caller waiting for the cell in full still learns it failed"
        );
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(upgrades, 1, "one upgrade to full was asked for");
        assert_eq!(
            metrics.cells_upgraded, upgrades,
            "the failed upgrade is not asked for again"
        );
        assert_eq!(
            metrics.failed_cells,
            failed_before + 1,
            "and so it does not fail again"
        );
    }

    /// A cell whose first full load fails transiently - the database cannot answer the reference
    /// query - is asked for again in full once its backoff passes, with the terrain root its
    /// rescue committed riding along as `replaced`, and the retry commits: the cell ends resident
    /// in full, its failure record is gone, and the retry is counted as one.
    #[test]
    fn a_failed_full_load_is_retried_after_its_backoff_and_becomes_resident() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_terrain_cache(&cache_path, 1);
        let mut app = failed_full_app(&database_path, &cache_path);
        let key = exterior_cell(2, -3);
        // The camera stands in the cell, so it is wanted in full - and that request fails.
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));

        run_until(&mut app, "the full request's failure", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Failed {
                    attempts: 1,
                    retry_at: Some(_),
                    ..
                })
            )
        });
        {
            let metrics = app.world().resource::<StreamingMetrics>();
            assert_eq!(metrics.failed_cells, 1);
            assert_eq!(metrics.failed_cell_retries, 0);
            assert_eq!(metrics.failed_cells_given_up, 0);
        }
        {
            let streaming = app.world().resource::<StreamingWorld>();
            let failure = streaming
                .full_failed
                .get(&key)
                .expect("the failure is recorded");
            assert_eq!(failure.attempts, 1);
            assert!(!failure.reason.is_empty(), "the failure keeps its reason");
            assert!(
                failure
                    .retry_at
                    .is_some_and(|retry_at| retry_at > Instant::now()),
                "the retry must not be immediate"
            );
        }

        // The landscape is drawn while the cell waits: the plan asks for the cell's terrain on its
        // own, and that commits.
        run_until(&mut app, "the cell's terrain rescue", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Resident {
                    detail: CellDetail::Terrain,
                    ..
                })
            )
        });
        let rescue_root = match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Resident {
                root,
                detail: CellDetail::Terrain,
            }) => *root,
            other => panic!("the cell is not resident as terrain: {other:?}"),
        };
        let requests_quiet = app
            .world()
            .resource::<StreamingMetrics>()
            .requests_submitted;
        for _ in 0..5 {
            app.update();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .requests_submitted,
            requests_quiet,
            "a cell inside its backoff is not asked for again"
        );
        assert_eq!(
            app.world()
                .resource::<StreamingWorld>()
                .full_failed
                .get(&key)
                .expect("still failed")
                .attempts,
            1,
            "the quiet frames asked for nothing"
        );
        assert!(
            app.world().get_entity(rescue_root).is_ok(),
            "the rescue keeps drawing the cell"
        );

        // The transient cause heals and the backoff passes: the plan asks for the cell in full
        // again, the rescue root rides along, and the retry commits.
        restore_references_table(&database_path);
        elapse_backoff(&mut app, key);
        run_until(&mut app, "the retried full cell", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Resident {
                    detail: CellDetail::Full,
                    ..
                })
            )
        });
        assert!(
            app.world().get::<SwapStandIn>(rescue_root).is_some(),
            "the commit keeps the rescue root, and the new terrain names it as its stand-in"
        );
        assert!(
            app.world().get::<DistantTerrainRoot>(rescue_root).is_none(),
            "the stand-in is no longer a streamed cell root, so the cell has one root"
        );
        let streaming = app.world().resource::<StreamingWorld>();
        assert!(
            !streaming.full_failed.contains_key(&key),
            "a cell that committed in full has no failure record"
        );
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.failed_cells, 1, "the retry committed, not failed");
        assert_eq!(metrics.failed_cell_retries, 1);
        assert_eq!(metrics.failed_cells_given_up, 0);
    }

    /// A cell that keeps failing is asked for at most [`MAX_CELL_LOAD_ATTEMPTS`] times and then
    /// given up on: the record keeps the last failure's reason and no retry time, the plan never
    /// asks for it in full again while it stays in the window, and the terrain its rescue drew
    /// keeps being drawn - giving up never takes the ground away.
    #[test]
    fn a_cell_that_keeps_failing_stops_after_the_attempt_budget() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_terrain_cache(&cache_path, 1);
        let mut app = failed_full_app(&database_path, &cache_path);
        let key = exterior_cell(2, -3);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));

        run_until(&mut app, "the first failure", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Failed {
                    attempts: 1,
                    retry_at: Some(_),
                    ..
                })
            )
        });
        // While the first backoff waits, the plan draws the cell's landscape on its own: the
        // rescue commits, and the first retry rides in on it.
        run_until(&mut app, "the first rescue", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Resident {
                    detail: CellDetail::Terrain,
                    ..
                })
            )
        });
        let mut rescue_root = match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Resident {
                root,
                detail: CellDetail::Terrain,
            }) => *root,
            other => panic!("the cell is not resident as terrain: {other:?}"),
        };
        // Each retry asks for the cell in full while it is drawn by its rescue, and a failure
        // keeps that root drawn. Between two permitted retries the downgrade commits a fresh
        // rescue; the last failure is the one that spends the budget.
        let mut kept_root = None;
        for attempt in 2..=MAX_CELL_LOAD_ATTEMPTS {
            elapse_backoff(&mut app, key);
            run_until(&mut app, &format!("attempt {attempt}'s failure"), |app| {
                matches!(
                    app.world().resource::<StreamingWorld>().cells.get(&key),
                    Some(CellStatus::Failed { attempts, .. }) if *attempts == attempt
                )
            });
            kept_root = match app.world().resource::<StreamingWorld>().cells.get(&key) {
                Some(CellStatus::Failed {
                    replaced: Some(root),
                    ..
                }) => Some(*root),
                _ => None,
            };
            assert_eq!(
                kept_root,
                Some(rescue_root),
                "attempt {attempt} failed while the cell was drawn by the rescue it rode in on, \
                 and that root stayed kept"
            );
            if attempt < MAX_CELL_LOAD_ATTEMPTS {
                run_until(&mut app, "the next rescue", |app| {
                    matches!(
                        app.world().resource::<StreamingWorld>().cells.get(&key),
                        Some(CellStatus::Resident {
                            detail: CellDetail::Terrain,
                            ..
                        })
                    )
                });
                rescue_root = match app.world().resource::<StreamingWorld>().cells.get(&key) {
                    Some(CellStatus::Resident {
                        root,
                        detail: CellDetail::Terrain,
                    }) => *root,
                    other => panic!("the cell is not resident as terrain: {other:?}"),
                };
            }
        }
        let CellStatus::Failed {
            attempts,
            reason,
            retry_at,
            replaced,
            ..
        } = app
            .world()
            .resource::<StreamingWorld>()
            .cells
            .get(&key)
            .unwrap()
        else {
            unreachable!("checked above");
        };
        assert_eq!(*attempts, MAX_CELL_LOAD_ATTEMPTS);
        assert!(!reason.is_empty(), "the given-up cell keeps its reason");
        assert!(retry_at.is_none(), "a given-up cell is never retried");
        assert_eq!(
            *replaced, kept_root,
            "the failed retry kept the root the cell is drawn by"
        );
        {
            let streaming = app.world().resource::<StreamingWorld>();
            let failure = streaming.full_failed.get(&key).unwrap();
            assert_eq!(failure.attempts, MAX_CELL_LOAD_ATTEMPTS);
            assert!(failure.retry_at.is_none());
        }
        {
            let metrics = app.world().resource::<StreamingMetrics>();
            assert_eq!(metrics.failed_cells, u64::from(MAX_CELL_LOAD_ATTEMPTS));
            assert_eq!(
                metrics.failed_cell_retries,
                u64::from(MAX_CELL_LOAD_ATTEMPTS - 1)
            );
            assert_eq!(metrics.failed_cells_given_up, 1);
        }
        assert!(
            app.world().get_entity(rescue_root).is_ok(),
            "the rescue is never taken away by giving up"
        );

        // The rescue keeps drawing the cell however long the camera stays, and no plan pass asks
        // for the cell in full again.
        run_until(&mut app, "the rescue drawn again", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Resident {
                    detail: CellDetail::Terrain,
                    ..
                })
            )
        });
        let requests_quiet = app
            .world()
            .resource::<StreamingMetrics>()
            .requests_submitted;
        let failures_quiet = app.world().resource::<StreamingMetrics>().failed_cells;
        for _ in 0..10 {
            app.update();
            std::thread::sleep(Duration::from_millis(5));
        }
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.requests_submitted, requests_quiet);
        assert_eq!(metrics.failed_cells, failures_quiet);
        assert!(matches!(
            app.world().resource::<StreamingWorld>().cells.get(&key),
            Some(CellStatus::Resident {
                detail: CellDetail::Terrain,
                ..
            })
        ));
    }

    /// A deterministic failure - the LAND data itself is invalid, so a retry would re-read the
    /// same rows from the same read-only database - is given up on at the first attempt, with no
    /// retry scheduled at all.
    #[test]
    fn invalid_terrain_data_is_not_retried() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_invalid_terrain_cache(&cache_path, 1);
        let mut app = terrain_streaming_app(&database_path, &cache_path);
        let key = exterior_cell(2, -3);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));

        run_until(&mut app, "the full request's failure", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Failed {
                    attempts: 1,
                    retry_at: None,
                    ..
                })
            )
        });
        let CellStatus::Failed { reason, .. } = app
            .world()
            .resource::<StreamingWorld>()
            .cells
            .get(&key)
            .unwrap()
        else {
            unreachable!("checked above");
        };
        assert!(
            reason.contains("normal"),
            "the reason names the invalid data: {reason}"
        );
        {
            let streaming = app.world().resource::<StreamingWorld>();
            let failure = streaming.full_failed.get(&key).unwrap();
            assert_eq!(failure.attempts, 1);
            assert!(failure.retry_at.is_none(), "no retry is scheduled");
        }
        {
            let metrics = app.world().resource::<StreamingMetrics>();
            assert_eq!(metrics.failed_cells, 1);
            assert_eq!(metrics.failed_cell_retries, 0, "no retry was scheduled");
            assert_eq!(metrics.failed_cells_given_up, 1);
            assert_eq!(metrics.terrain_validation_failures, 1);
            assert_eq!(metrics.asset_failures.len(), 1);
        }

        // The plan still draws the cell's landscape on its own, which the same data fails too -
        // ring noise, not a failure - and then everything is quiet.
        run_until(&mut app, "the rescue's answer", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Failed {
                    detail: CellDetail::Terrain,
                    ..
                })
            )
        });
        let requests_quiet = app
            .world()
            .resource::<StreamingMetrics>()
            .requests_submitted;
        for _ in 0..10 {
            app.update();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .requests_submitted,
            requests_quiet,
            "a cell with invalid data is not re-queried"
        );
    }

    /// A given-up cell that leaves the plan is forgotten: when the camera comes back the cell is
    /// asked for again from nothing, with a fresh attempt count and a fresh failure record.
    #[test]
    fn a_given_up_cell_outside_the_window_starts_fresh_when_it_returns() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_empty_cell_cache(&cache_path);
        let mut app = failed_full_app(&database_path, &cache_path);
        let key = exterior_cell(2, -3);
        // No landscape in the cache: the rescue commits nothing, so the failed cell keeps no root
        // and a fall out by distance drops the entry the frame it happens.
        let camera = app
            .world_mut()
            .spawn((Transform::default(), StreamingCamera))
            .id();

        run_until(&mut app, "the first failure", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Failed {
                    attempts: 1,
                    retry_at: Some(_),
                    ..
                })
            )
        });
        elapse_backoff(&mut app, key);
        run_until(&mut app, "the second failure", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Failed {
                    attempts: 2,
                    retry_at: Some(_),
                    ..
                })
            )
        });
        elapse_backoff(&mut app, key);
        run_until(&mut app, "the budget running out", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Failed {
                    attempts: 3,
                    retry_at: None,
                    ..
                })
            )
        });

        // Five cells west is outside the keep radius - the outer of the unload radius (1) and the
        // ring's (terrain_radius 2 plus one), three here - and past the ring (2), so nothing wants
        // the cell again while the camera is there. The drop waits for a plan pass that does not
        // break early on a full request queue.
        let failures_before_moving = app.world().resource::<StreamingMetrics>().failed_cells;
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x -= 5.0 * CELL_SIZE;
        for _ in 0..40 {
            app.update();
            if !app
                .world()
                .resource::<StreamingWorld>()
                .cells
                .contains_key(&key)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        {
            let streaming = app.world().resource::<StreamingWorld>();
            assert!(
                !streaming.cells.contains_key(&key),
                "a failed cell outside the window is forgotten"
            );
            assert!(
                !streaming.full_failed.contains_key(&key),
                "and so is its failure record"
            );
        }

        // Back in range, the cell is wanted from nothing again: a fresh request, a fresh count.
        // The cell the camera stood on while away failed once on its own - a transient database
        // error, with nothing else wanting that grid - so two more failures are on the counter.
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x += 5.0 * CELL_SIZE;
        run_until(&mut app, "the fresh failure", |app| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Failed {
                    attempts: 1,
                    retry_at: Some(_),
                    ..
                })
            )
        });
        assert_eq!(
            app.world()
                .resource::<StreamingWorld>()
                .full_failed
                .get(&key)
                .expect("a fresh failure record")
                .attempts,
            1
        );
        let metrics = app.world().resource::<StreamingMetrics>();
        // The total failure count now also carries whatever the cell the camera stood on while
        // away recorded - a race on the worker's answer, not something this test pins down. What
        // it pins down is the fresh record above, and that no new retry or give-up was spent.
        assert!(metrics.failed_cells > failures_before_moving);
        assert_eq!(metrics.failed_cell_retries, 2);
        assert_eq!(metrics.failed_cells_given_up, 1);
    }

    /// The terrain-only request end to end: the planner asks for it, the worker answers it from the
    /// cell cache, and the commit spawns the landscape and nothing else - four quadrant patches and
    /// no reference, light or door - under a root the portal's isolation does not walk.
    #[test]
    fn commits_a_terrain_only_cell_with_its_landscape_and_nothing_else() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_terrain_cache(&cache_path, 1);
        let catalogue_path = directory.path().join("catalogue.db");
        write_empty_catalogue(&catalogue_path);

        let key = CellKey::Exterior {
            worldspace_id: 60,
            grid_x: 2,
            grid_y: -3,
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .init_asset::<TerrainMaterial>()
            .init_asset::<WaterMaterial>()
            .insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(CellCache::open(&cache_path).unwrap())
            .insert_resource(AssetCatalog::open(&catalogue_path).unwrap())
            .insert_resource(WaterReflectionTexture(Handle::default()))
            .insert_resource(WorldDatabase::open(&database_path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, collect_cells);
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                key,
                CellStatus::Loading {
                    generation: 1,
                    detail: CellDetail::Terrain,
                    replaced: None,
                },
            );
        app.world_mut()
            .resource::<WorldDatabase>()
            .request(DatabaseRequest::Load {
                generation: 1,
                key,
                detail: CellDetail::Terrain,
                queued_at: Instant::now(),
            })
            .unwrap();

        // The answer is a query away on the worker's own thread.
        let committed = |app: &App| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Resident { .. })
            )
        };
        for _ in 0..400 {
            app.update();
            if committed(&app) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(committed(&app), "the terrain-only cell never committed");

        let streaming = app.world().resource::<StreamingWorld>();
        let Some(CellStatus::Resident { root, detail }) = streaming.cells.get(&key) else {
            panic!("the terrain-only cell did not commit");
        };
        assert_eq!(*detail, CellDetail::Terrain);
        let root = *root;
        assert!(
            app.world().get::<DistantTerrainRoot>(root).is_some(),
            "a terrain-only root is not a cell root the portal's isolation walks"
        );
        assert!(app.world().get::<StreamedCellRoot>(root).is_none());
        assert!(app.world().get::<CellRef>(root).is_some());

        let mut patches = 0;
        let mut references = 0;
        let mut waters = 0;
        let mut descendants = app.world_mut().query::<(
            Option<&TerrainPatch>,
            Option<&FormId>,
            Option<&WaterSurface>,
        )>();
        for (patch, form_id, water) in descendants.iter(app.world()) {
            match (patch, form_id, water) {
                (Some(_), _, _) => patches += 1,
                (_, Some(_), _) => references += 1,
                (_, _, Some(_)) => waters += 1,
                _ => {}
            }
        }
        assert_eq!(patches, 4, "the landscape of four quadrants");
        assert_eq!(references, 0, "no reference is spawned in the ring");
        assert_eq!(waters, 0, "the fixture's cell is dry");
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .terrain_validation_failures,
            0
        );
    }

    /// The detail-change hand-over end to end through the commit (design-1091 Â§3, slice C3): the
    /// ring cell is asked for in full, and the root the cell was drawn by is not despawned - it is
    /// handed over as the stand-in every patch of the new root names, and stops being a streamed
    /// cell root, so the cell has one root and the stand-in is never counted as a second.
    #[test]
    fn a_detail_change_keeps_the_old_root() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("world.db");
        write_terrain_database(&database_path);
        let cache_path = directory.path().join("cell_cache.rkyv");
        write_terrain_cache(&cache_path, 1);
        let catalogue_path = directory.path().join("catalogue.db");
        write_empty_catalogue(&catalogue_path);

        let key = CellKey::Exterior {
            worldspace_id: 60,
            grid_x: 2,
            grid_y: -3,
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<StandardMaterial>()
            .init_asset::<TerrainMaterial>()
            .init_asset::<WaterMaterial>()
            .insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .insert_resource(CellCache::open(&cache_path).unwrap())
            .insert_resource(AssetCatalog::open(&catalogue_path).unwrap())
            .insert_resource(WaterReflectionTexture(Handle::default()))
            .insert_resource(WorldDatabase::open(&database_path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, collect_cells);
        // No `plan_cells` here: the cell's status and its request are handed in, so the test drives
        // the two commits it needs and nothing else. The commit budget's one slot is given, since
        // nothing in this app refills it each frame.
        app.world_mut()
            .resource_mut::<StreamingCommitBudget>()
            .remaining = 4;

        let resident = |app: &App, detail: CellDetail| {
            matches!(
                app.world().resource::<StreamingWorld>().cells.get(&key),
                Some(CellStatus::Resident { detail: resident, .. }) if *resident == detail
            )
        };
        // The ring cell: the tier a worldspace without LOD draws its distance with.
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                key,
                CellStatus::Loading {
                    generation: 1,
                    detail: CellDetail::Terrain,
                    replaced: None,
                },
            );
        app.world_mut()
            .resource::<WorldDatabase>()
            .request(DatabaseRequest::Load {
                generation: 1,
                key,
                detail: CellDetail::Terrain,
                queued_at: Instant::now(),
            })
            .unwrap();
        run_until(&mut app, "the terrain-only commit", |app| {
            resident(app, CellDetail::Terrain)
        });
        let ring_root = match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Resident { root, .. }) => *root,
            other => panic!("the terrain-only cell did not commit: {other:?}"),
        };
        assert!(app.world().get::<DistantTerrainRoot>(ring_root).is_some());

        // The camera walks in: the same grid is asked for in full, the ring root riding along.
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                key,
                CellStatus::Loading {
                    generation: 2,
                    detail: CellDetail::Full,
                    replaced: Some(ring_root),
                },
            );
        app.world_mut()
            .resource::<WorldDatabase>()
            .request(DatabaseRequest::Load {
                generation: 2,
                key,
                detail: CellDetail::Full,
                queued_at: Instant::now(),
            })
            .unwrap();
        run_until(&mut app, "the full commit", |app| {
            resident(app, CellDetail::Full)
        });

        let full_root = match app.world().resource::<StreamingWorld>().cells.get(&key) {
            Some(CellStatus::Resident { root, detail }) => {
                assert_eq!(*detail, CellDetail::Full);
                *root
            }
            other => panic!("the full cell did not commit: {other:?}"),
        };
        assert_ne!(full_root, ring_root, "the replacement is a new root");
        // No `cells_upgraded` here: the plan is what counts a tier change, and this app hands the
        // cell's status in instead of planning it.
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .swap_hold_timeouts,
            0
        );

        // The root the cell was drawn by is kept, and it is no longer a streamed cell root: the
        // markers every root scan keys on are gone, so the lifecycle check counts one root.
        assert!(
            app.world().get::<SwapStandIn>(ring_root).is_some(),
            "the replaced root is held as the stand-in"
        );
        assert!(
            app.world().get::<DistantTerrainRoot>(ring_root).is_none(),
            "the stand-in is not a streamed cell root"
        );
        assert!(app.world().get::<StreamedCellRoot>(ring_root).is_none());
        assert!(
            app.world().get::<CellRef>(ring_root).is_some(),
            "the stand-in still names the cell it covers"
        );
        let old_terrain = app
            .world()
            .get::<Children>(ring_root)
            .map(|children| children.iter().collect::<Vec<Entity>>())
            .unwrap_or_default()
            .into_iter()
            .filter(|child| app.world().get::<TerrainPatch>(*child).is_some())
            .count();
        assert_eq!(old_terrain, 4, "the stand-in keeps its landscape drawn");

        // Every patch of the new root names it back, which is what the hand-over reads to know when
        // the hold ends.
        let named = app
            .world()
            .get::<Children>(full_root)
            .map(|children| children.iter().collect::<Vec<Entity>>())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|child| app.world().get::<SwapSurface>(child))
            .map(SwapSurface::stand_in)
            .collect::<Vec<Option<Entity>>>();
        assert_eq!(named, vec![Some(ring_root); 4]);
    }

    /// own, as the database hands one to `spawn_cell`.
    fn lit_reference(
        form_id: u32,
        light: Option<LightRow>,
        radius_override: Option<f32>,
    ) -> ReferenceRow {
        ReferenceRow {
            form_id,
            cell_id: 99,
            base_form_id: 0x200 + form_id,
            model_path: None,
            position: [100.0, 50.0, -200.0],
            rotation: [0.0; 3],
            scale: 1.0,
            bounds_min: [0.0; 3],
            bounds_max: [0.0; 3],
            bounds_valid: false,
            door: None,
            light,
            base_record_type: None,
            light_radius_override: radius_override,

            initially_enabled: true,
            emittance: None,
        }
    }

    #[derive(Resource, Default)]
    struct QueuedReferences(Vec<ReferenceRow>);

    /// The cell root the last [`spawn_queued_references`] produced.
    #[derive(Resource, Default)]
    struct SpawnedCellRoot(Option<Entity>);

    fn spawn_queued_references(
        mut commands: Commands,
        config: Res<EngineConfig>,
        queued: Res<QueuedReferences>,
        mut root: ResMut<SpawnedCellRoot>,
        asset_server: Res<AssetServer>,
        catalog: Res<AssetCatalog>,
        reflection: Res<WaterReflectionTexture>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
        mut water_materials: ResMut<Assets<WaterMaterial>>,
        mut profiler: ResMut<ProfilingState>,
    ) {
        root.0 = Some(
            spawn_cell(
                &mut commands,
                &asset_server,
                &catalog,
                &reflection,
                &mut meshes,
                &mut terrain_materials,
                &mut water_materials,
                IVec2::ZERO,
                config.lights,
                false,
                false,
                CellPayload {
                    generation: 1,
                    key: CellKey::Interior(99),
                    cell_id: 99,
                    references: queued.0.clone(),
                },
                None,
                None,
                &mut 0,
                &mut profiler,
            )
            .expect("spawn cell"),
        );
    }

    fn spawn_reference_cell_app(references: Vec<ReferenceRow>) -> App {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalogue.db");
        write_empty_catalogue(&path);
        let config = EngineConfig {
            lights: true,
            ..default()
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<TerrainMaterial>()
            .init_asset::<WaterMaterial>()
            .insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<ActiveSpace>()
            .insert_resource(AssetCatalog::open(&path).unwrap())
            .insert_resource(WaterReflectionTexture(Handle::default()))
            .insert_resource(QueuedReferences(references))
            .init_resource::<SpawnedCellRoot>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, spawn_queued_references);
        app.update();
        app
    }

    #[test]
    fn repeated_rebasing_preserves_camera_and_cell_root_locality() {
        let mut app = App::new();
        app.insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, update_render_origin);
        let camera = app
            .world_mut()
            .spawn((Transform::default(), StreamingCamera))
            .id();
        let root = app
            .world_mut()
            .spawn((ExteriorCellGrid(IVec2::new(8, -3)), Transform::default()))
            .id();
        for shift in [IVec2::new(2, 1), IVec2::new(-3, 4), IVec2::new(7, -2)] {
            {
                let mut entity = app.world_mut().entity_mut(camera);
                let mut transform = entity.get_mut::<Transform>().unwrap();
                transform.translation.x = shift.x as f32 * CELL_SIZE + 12.0;
                transform.translation.z = -(shift.y as f32 * CELL_SIZE) - 20.0;
            }
            app.update();
            let camera_transform = app.world().entity(camera).get::<Transform>().unwrap();
            assert!(camera_transform.translation.x.abs() < CELL_SIZE);
            assert!(camera_transform.translation.z.abs() < CELL_SIZE);
        }
        assert_eq!(app.world().resource::<StreamingMetrics>().origin_rebases, 3);
        let origin = app.world().resource::<RenderOrigin>().0;
        let root_transform = app.world().entity(root).get::<Transform>().unwrap();
        assert_eq!(
            root_transform.translation,
            Vec3::new(
                (8 - origin.x) as f32 * CELL_SIZE,
                0.0,
                -(-3 - origin.y) as f32 * CELL_SIZE,
            )
        );
    }

    /// A visible full-detail terrain quadrant the render world has not confirmed drawn past
    /// [`MAX_SWAP_WAIT_FRAMES`] is the hole coming back, and the run-level invariant says so; one
    /// within the bound is a quadrant on its way and is not; and a quadrant no view can see is never
    /// counted, however long it waits (design-1091 Â§4).
    #[test]
    fn lifecycle_validator_flags_a_visible_patch_stuck_past_the_swap_bound() {
        let mut app = App::new();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .init_resource::<StreamingWorld>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, validate_streaming_lifecycle);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        let root = app
            .world_mut()
            .spawn((StreamedCellRoot, CellRef(0), Transform::default()))
            .id();
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                exterior_cell(0, 0),
                CellStatus::Resident {
                    root,
                    detail: CellDetail::Full,
                },
            );
        // One waiting quadrant a view can see and one it cannot.
        app.world_mut()
            .spawn((TerrainPatch, SwapWaiting, SwapInCameraView, ChildOf(root)));
        app.world_mut()
            .spawn((TerrainPatch, SwapWaiting, ChildOf(root)));

        // Within the bound: a quadrant on its way is not a hole.
        for _ in 0..MAX_SWAP_WAIT_FRAMES {
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .streaming_invariant_failures,
            0,
            "a quadrant still inside the bound is not a violation",
        );

        // Past it: the visible quadrant alone is the violation.
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .streaming_invariant_failures,
            1,
            "the stuck visible quadrant, and not the one off-view",
        );
    }

    /// `PendingUnder` holds a crossing for a surface the render world has not confirmed drawn only
    /// while a view can see it: an interior the camera has arrived in has its surfaces in view, and
    /// a pre-streamed cell's surfaces, which no view can see and the bridge can never confirm, do
    /// not hold the door the camera is not standing in.
    #[test]
    fn pending_under_counts_a_visible_waiting_surface_and_ignores_a_hidden_one() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let visible = world
            .spawn((SwapWaiting, SwapInCameraView, ChildOf(root)))
            .id();
        let hidden = world.spawn((SwapWaiting, ChildOf(root))).id();
        let mut state = SystemState::<PendingUnder>::new(&mut world);

        let count = state.get(&world).unwrap().count(root);
        assert_eq!(
            count, 1,
            "only the surface a view can see holds the crossing"
        );

        // The old pending-work markers still count whatever their visibility.
        world.spawn((pending_marker_for_test(), ChildOf(root)));
        assert_eq!(state.get(&world).unwrap().count(root), 2);

        // Once the visible surface is drawn (its marker is gone) and the hidden one is not needed,
        // only the model marker is left.
        world.entity_mut(visible).remove::<SwapWaiting>();
        world.entity_mut(hidden).remove::<SwapWaiting>();
        assert_eq!(state.get(&world).unwrap().count(root), 1);
    }

    /// A failed model leaves no pending marker and a hidden scene; `AssetFailed` is what still says
    /// the cell under `root` is missing something, with nothing pending and nothing visible.
    #[test]
    fn pending_under_reports_a_failed_asset_after_its_pending_markers_are_gone() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let model = world.spawn((pending_marker_for_test(), ChildOf(root))).id();
        world.spawn((Visibility::Hidden, ChildOf(model)));
        let mut state = SystemState::<PendingUnder>::new(&mut world);
        assert_eq!(state.get(&world).unwrap().failed(root), 0);

        // The failure branch: pending marker removed, failure recorded.
        world
            .entity_mut(model)
            .remove::<PendingTerrainProfile>()
            .insert(AssetFailed);
        let under = state.get(&world).unwrap();
        assert_eq!(under.count(root), 0, "nothing is pending any more");
        assert_eq!(under.failed(root), 1, "the failure is still on record");
    }

    #[test]
    fn lifecycle_validator_detects_duplicate_and_orphaned_roots() {
        let mut app = App::new();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(ActiveSpace {
                worldspace_id: Some(60),
                interior: None,
            })
            .init_resource::<StreamingWorld>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, validate_streaming_lifecycle);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        let resident = app.world_mut().spawn((CellRef(7), StreamedCellRoot)).id();
        app.world_mut().spawn((CellRef(7), StreamedCellRoot));
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                CellKey::Interior(7),
                CellStatus::Resident {
                    root: resident,
                    detail: CellDetail::Full,
                },
            );
        app.update();
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.duplicate_cell_roots, 1);
        assert_eq!(metrics.orphaned_cell_roots, 1);
        assert_eq!(metrics.missing_cell_roots, 0);
        assert_eq!(metrics.streaming_invariant_failures, 2);
    }

    /// A cell that is loading at another tier keeps drawing the root it is replacing, and a full
    /// cell on its way to being terrain-only sits inside the band the plan keeps cells by: neither
    /// is an orphan and neither is out of range. The streaming fixture caught this first, with the
    /// ring filling the worker's queue while the camera crossed the inner grid's edge: three cells
    /// were still full a few cells out in the frame the camera moved, and the check called them
    /// violations.
    #[test]
    fn lifecycle_validator_accepts_a_cell_that_is_changing_tier() {
        let mut app = App::new();
        app.insert_resource(EngineConfig {
            stream_radius: 1,
            unload_radius: 2,
            terrain_radius: 4,
            ..EngineConfig::default()
        })
        .insert_resource(RenderOrigin(IVec2::ZERO))
        .insert_resource(ActiveSpace {
            worldspace_id: Some(60),
            interior: None,
        })
        .init_resource::<StreamingWorld>()
        .init_resource::<StreamingMetrics>()
        .init_resource::<ProfilingState>()
        .add_systems(Update, validate_streaming_lifecycle);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        // Three cells out: beyond the inner grid, so the plan wants it terrain-only and is loading
        // that now, and its full root is still the one drawing it.
        let replacing = app
            .world_mut()
            .spawn((StreamedCellRoot, CellRef(7), Transform::default()))
            .id();
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                exterior_cell(3, 0),
                CellStatus::Loading {
                    generation: 1,
                    detail: CellDetail::Terrain,
                    replaced: Some(replacing),
                },
            );
        // Four cells out, which is the far edge of the ring: resident as terrain-only.
        let edge = app
            .world_mut()
            .spawn((DistantTerrainRoot, CellRef(8), Transform::default()))
            .id();
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                exterior_cell(4, 0),
                CellStatus::Resident {
                    root: edge,
                    detail: CellDetail::Terrain,
                },
            );

        app.update();
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.orphaned_cell_roots, 0);
        assert_eq!(metrics.missing_cell_roots, 0);
        assert_eq!(metrics.out_of_range_cell_roots, 0);
        assert_eq!(metrics.streaming_invariant_failures, 0);

        // A cell beyond every band is still a cell the plan should have unloaded.
        let far = app
            .world_mut()
            .spawn((DistantTerrainRoot, CellRef(9), Transform::default()))
            .id();
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                exterior_cell(6, 0),
                CellStatus::Resident {
                    root: far,
                    detail: CellDetail::Terrain,
                },
            );
        app.update();
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.out_of_range_cell_roots, 1);
        assert_eq!(metrics.orphaned_cell_roots, 0);
    }

    #[test]
    fn maps_creation_position_and_rotation_through_the_same_basis() {
        assert_eq!(creation_to_bevy(Vec3::Y), Vec3::NEG_Z);
        assert_eq!(creation_to_bevy(Vec3::Z), Vec3::Y);

        // Creation yaw turns clockwise seen from above: a quarter turn takes east (+X) to south
        // (Creation -Y, runtime +Z).
        let rotation = creation_rotation_to_bevy([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
        let rotated = rotation * Vec3::X;
        assert!(rotated.abs_diff_eq(Vec3::Z, 1.0e-5));
    }

    #[test]
    fn creates_upward_wound_quadrants_with_continuous_uvs() {
        let terrain = terrain_fixture(1, 0.0);
        let mesh = build_terrain_quadrant_mesh(&terrain, 3).unwrap();
        assert_eq!(mesh.count_vertices(), 17 * 17);
        assert_eq!(mesh.indices().unwrap().len(), 16 * 16 * 6);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let [a, b, c] = [positions[0], positions[1], positions[17]];
        let normal = (Vec3::from(b) - Vec3::from(a)).cross(Vec3::from(c) - Vec3::from(a));
        assert!(normal.y > 0.0);
        let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
        else {
            panic!("terrain UVs must be Float32x2");
        };
        assert_eq!(uvs[0], [0.5, 0.5]);
        assert_eq!(uvs[16 * 17 + 16], [1.0, 1.0]);
    }

    /// The rule that replaced "reject any edge that differs": matching edges pass untouched, a
    /// crack inside the weldable range is welded onto the resident cell, and a difference past
    /// the sanity bound is still corrupt data and rejected.
    #[test]
    fn accepts_matching_neighbor_edges_and_welds_small_cracks() {
        let mut continuity = TerrainContinuity::default();
        let mut metrics = StreamingMetrics::default();
        let west = exterior_cell(0, 0);
        let east = exterior_cell(1, 0);
        let mut west_terrain = terrain_fixture(1, 10.0);
        validate_and_register_terrain_edges(west, &mut west_terrain, &mut continuity, &mut metrics)
            .unwrap();
        let mut east_terrain = terrain_fixture(2, 10.0);
        validate_and_register_terrain_edges(east, &mut east_terrain, &mut continuity, &mut metrics)
            .unwrap();
        assert_eq!(metrics.terrain_seams_validated, 1);
        assert_eq!(
            metrics.terrain_seam_points_welded, 0,
            "a matching edge moves nothing"
        );

        // A one-unit crack is a seam, not a failure: the arriving cell takes the resident heights
        // along the shared edge and keeps its own everywhere else.
        let mut farther_east_terrain = terrain_fixture(3, 11.0);
        validate_and_register_terrain_edges(
            exterior_cell(2, 0),
            &mut farther_east_terrain,
            &mut continuity,
            &mut metrics,
        )
        .unwrap();
        assert!(
            farther_east_terrain
                .heights
                .iter()
                .enumerate()
                .all(|(index, height)| *height == if index % 33 == 0 { 10.0 } else { 11.0 })
        );
        assert_eq!(metrics.terrain_seams_validated, 2);
        assert_eq!(metrics.terrain_seam_points_welded, 33);

        // Past the bound it is not a seam but two landscapes authored apart, as where a city's
        // sculpted land meets the unsculpted copy beside it: the cell is kept as authored, not
        // welded, and registered so the cells after it weld onto what is drawn.
        let authored = exterior_cell(3, 0);
        let authored_height = 11.0 + MAX_WELDABLE_EDGE_DELTA + 1.0;
        let mut authored_terrain = terrain_fixture(4, authored_height);
        validate_and_register_terrain_edges(
            authored,
            &mut authored_terrain,
            &mut continuity,
            &mut metrics,
        )
        .expect("an edge past the weld bound is drawn as authored, not a reason to drop the cell");
        assert!(
            authored_terrain
                .heights
                .iter()
                .all(|height| *height == authored_height),
            "the edge past the bound keeps its authored heights"
        );
        assert!(continuity.edges.contains_key(&authored));
        assert_eq!(metrics.terrain_edges_left_as_authored, 1);
        assert_eq!(
            metrics.terrain_seam_points_welded, 33,
            "nothing was welded onto it"
        );
    }

    /// The seam measured on real data (`tools/research/land_edges.py`: Tamriel (18,18) against
    /// (18,19), one point of the north edge 24 units off) used to reject the whole cell and leave
    /// a hole where the player stands. It must weld, spawn, and mesh exactly like its neighbour.
    #[test]
    fn welds_one_point_of_an_edge_and_spawns_the_arriving_cell() {
        let mut continuity = TerrainContinuity::default();
        let mut metrics = StreamingMetrics::default();
        let resident_key = exterior_cell(0, 0);
        let arriving_key = exterior_cell(0, 1);
        let mut resident = untextured_fixture(0x0000_8F64, 100.0);
        validate_and_register_terrain_edges(
            resident_key,
            &mut resident,
            &mut continuity,
            &mut metrics,
        )
        .unwrap();

        // The arriving cell's south edge - height-field row 0 - is the edge it shares with the
        // resident cell's north edge, and it differs at one of its 33 points by 24 units.
        let mut arriving = untextured_fixture(0x0000_8F65, 100.0);
        arriving.heights[9] = 124.0;
        validate_and_register_terrain_edges(
            arriving_key,
            &mut arriving,
            &mut continuity,
            &mut metrics,
        )
        .expect("a seam inside the weldable range must not reject the cell");

        for column in 0..33 {
            assert_eq!(
                arriving.heights[column],
                resident.heights[32 * 33 + column],
                "point {column} of the shared edge"
            );
        }
        assert_eq!(arriving.heights[9], 100.0, "the arriving cell moved");
        assert_eq!(arriving.heights[33 + 9], 100.0, "only the edge moved");
        assert_eq!(metrics.terrain_seams_validated, 1);
        assert_eq!(metrics.terrain_seam_points_welded, 1);

        // Both cells spawn, the arriving one with the welded height in its mesh, and the
        // lifecycle validator has nothing to report about either.
        let mut app = spawn_cells_app(
            vec![(resident_key, resident), (arriving_key, arriving)],
            metrics,
        );
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.terrain_seam_points_welded, 1);
        assert_eq!(metrics.streaming_invariant_failures, 0);
        let handles: Vec<Handle<Mesh>> = {
            let mut patches = app
                .world_mut()
                .query_filtered::<&Mesh3d, With<TerrainPatch>>();
            patches
                .iter(app.world())
                .map(|mesh| mesh.0.clone())
                .collect()
        };
        assert_eq!(handles.len(), 8, "two cells of four quadrants each");
        let meshes = app.world().resource::<Assets<Mesh>>();
        let mut vertices = 0usize;
        for handle in &handles {
            let mesh = meshes.get(handle).expect("a spawned quadrant has its mesh");
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
                .as_float3()
                .unwrap();
            assert!(
                positions.iter().all(|position| position[1] == 100.0),
                "the seam must not survive into the geometry"
            );
            vertices += positions.len();
        }
        assert_eq!(vertices, 8 * 17 * 17);
    }

    /// A welded point no longer lies where its stored normal was computed from, so the normal is
    /// recomputed from the welded field in the converter's packing.
    #[test]
    fn recomputes_the_normals_of_welded_points_only() {
        let mut continuity = TerrainContinuity::default();
        let mut metrics = StreamingMetrics::default();
        let mut resident = untextured_fixture(1, 100.0);
        validate_and_register_terrain_edges(
            exterior_cell(0, 0),
            &mut resident,
            &mut continuity,
            &mut metrics,
        )
        .unwrap();

        // Rows 0 and 1 of the arriving cell stand 24 units above the resident cell, so row 0 is
        // welded down onto it and row 1 stays where it is: the welded point's normal must lean
        // towards the higher row instead of keeping the flat normal it was loaded with.
        let mut arriving = untextured_fixture(2, 100.0);
        for column in 0..33 {
            arriving.heights[column] = 124.0;
            arriving.heights[33 + column] = 124.0;
        }
        validate_and_register_terrain_edges(
            exterior_cell(0, 1),
            &mut arriving,
            &mut continuity,
            &mut metrics,
        )
        .unwrap();
        assert_eq!(metrics.terrain_seam_points_welded, 33);

        // At (x, y) = (9, 0), on the south edge, the y stencil spans one 128-unit interval (main's
        // span-divided stencil): (0, -24 / 128, 1) normalized and scaled by 127 is (0, -23, 125).
        assert_eq!(arriving.normals[9 * 3..9 * 3 + 3], [0, -23, 125]);
        assert_eq!(
            arriving.normals[(33 + 9) * 3..(33 + 9) * 3 + 3],
            [0, 0, 127],
            "a point that did not move keeps the normal it was loaded with"
        );
    }

    /// The rejections that are not seams: an edge of a length the neighbour cannot share, and a
    /// non-finite height - on either side of the comparison.
    #[test]
    fn still_rejects_wrong_edge_lengths_and_non_finite_heights() {
        let mut continuity = TerrainContinuity::default();
        let mut metrics = StreamingMetrics::default();
        let mut short_neighbor = untextured_fixture(1, 10.0);
        short_neighbor.width = 17;
        short_neighbor.height = 17;
        short_neighbor.heights = vec![10.0; 17 * 17];
        validate_and_register_terrain_edges(
            exterior_cell(0, 0),
            &mut short_neighbor,
            &mut continuity,
            &mut metrics,
        )
        .unwrap();

        let mut full = untextured_fixture(2, 10.0);
        let error = validate_and_register_terrain_edges(
            exterior_cell(1, 0),
            &mut full,
            &mut continuity,
            &mut metrics,
        )
        .unwrap_err();
        assert!(error.contains("points"), "{error}");

        let mut continuity = TerrainContinuity::default();
        let mut metrics = StreamingMetrics::default();
        let mut resident = untextured_fixture(3, 10.0);
        validate_and_register_terrain_edges(
            exterior_cell(2, 0),
            &mut resident,
            &mut continuity,
            &mut metrics,
        )
        .unwrap();
        let mut broken = untextured_fixture(4, 10.0);
        broken.heights[4 * 33] = f32::NAN;
        let error = validate_and_register_terrain_edges(
            exterior_cell(3, 0),
            &mut broken,
            &mut continuity,
            &mut metrics,
        )
        .unwrap_err();
        assert!(error.contains("non-finite"), "{error}");

        // A value `f32::max` would quietly fold away is a rejection, not a weld.
        continuity.edges.insert(
            exterior_cell(3, 0),
            TerrainEdges {
                west: vec![f32::NAN; 33],
                east: vec![f32::NAN; 33],
                south: vec![10.0; 33],
                north: vec![10.0; 33],
            },
        );
        let mut arriving = untextured_fixture(5, 10.0);
        assert!(
            validate_and_register_terrain_edges(
                exterior_cell(4, 0),
                &mut arriving,
                &mut continuity,
                &mut metrics
            )
            .is_err()
        );
    }

    /// Defect B: a cell whose terrain is dry has nothing for a water plane to cover, and the
    /// plane - a second surface on the ground, aligned to the cell - shows as grid lines.
    #[test]
    fn spawns_a_water_plane_only_where_the_terrain_reaches_the_water() {
        let mut dry = untextured_fixture(1, 200.0);
        dry.water_height = Some(100.0);
        let mut wet = untextured_fixture(2, 200.0);
        wet.water_height = Some(100.0);
        wet.heights[16 * 33 + 16] = 99.0;
        let mut app = spawn_cells_app(
            vec![(exterior_cell(0, 0), dry), (exterior_cell(1, 0), wet)],
            StreamingMetrics::default(),
        );

        let mut roots = app.world_mut().query::<(&ExteriorCellGrid, &Children)>();
        let mut planes_per_cell = Vec::new();
        for (grid, children) in roots.iter(app.world()) {
            let planes = children
                .iter()
                .filter(|child| app.world().get::<WaterSurface>(*child).is_some())
                .count();
            planes_per_cell.push(((grid.0.x, grid.0.y), planes));
        }
        planes_per_cell.sort();
        assert_eq!(
            planes_per_cell,
            vec![((0, 0), 0), ((1, 0), 1)],
            "only the cell with a sample under the water gets a plane"
        );

        let mut surfaces = app.world_mut().query::<(&WaterSurface, &Transform)>();
        let heights: Vec<f32> = surfaces
            .iter(app.world())
            .map(|(_, transform)| transform.translation.y)
            .collect();
        assert_eq!(heights, vec![100.0]);
    }

    /// Welded edge and corner samples have one neighbour per axis, so their stencil spans one
    /// interval, not two. Dividing by the actual span keeps a planar slope's normal identical at
    /// the interior, every edge and every corner, matching the converter's `decode_normals`.
    #[test]
    fn welded_edge_and_corner_normals_match_the_interior_on_a_planar_slope() {
        let mut terrain = terrain_fixture(1, 0.0);
        let width = usize::from(terrain.width);
        // Rises 40 units per interval eastwards and 20 per interval northwards.
        for y in 0..width {
            for x in 0..width {
                terrain.heights[y * width + x] = 40.0 * x as f32 + 20.0 * y as f32;
            }
        }
        let last = width - 1;
        let interior = 16 * width + 16;
        let samples = [
            interior,
            16 * width,          // west edge
            16 * width + last,   // east edge
            16,                  // south edge
            last * width + 16,   // north edge
            0,                   // south-west corner
            last * width + last, // north-east corner
        ];
        recompute_packed_normals(&mut terrain, &samples);
        let expected = terrain.normals[interior * 3..interior * 3 + 3].to_vec();
        // (-40/128, -20/128, 1) normalized and scaled by 127.
        assert_eq!(expected, vec![-37, -19, 120]);
        for index in samples {
            assert_eq!(
                &terrain.normals[index * 3..index * 3 + 3],
                expected.as_slice(),
                "sample {index} disagrees with the interior normal"
            );
        }
    }

    #[test]
    fn rejects_more_than_six_layers_per_quadrant() {
        let mut terrain = terrain_fixture(1, 0.0);
        terrain
            .layers
            .extend((1..=6).map(|layer| TerrainLayerSnapshot {
                texture_form_id: u32::from(layer) + 10,
                quadrant: 0,
                layer,
                is_base: false,
                weights: Vec::new(),
            }));
        assert!(quadrant_layers(&terrain, 0).is_err());
    }

    #[test]
    fn accepts_textureless_official_land_quadrant() {
        let mut terrain = terrain_fixture(1, 0.0);
        terrain.layers.clear();
        assert!(quadrant_layers(&terrain, 0).unwrap().is_empty());
    }

    #[test]
    fn validates_loaded_material_images_and_rejects_missing_required_texture() {
        let mut images = Assets::<Image>::default();
        let base_color = images.add(Image::new_fill(
            bevy::render::render_resource::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            &[255, 255, 255, 255],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ));
        let material = StandardMaterial {
            base_color_texture: Some(base_color),
            ..default()
        };
        assert_eq!(validate_standard_material(&material, &images), Ok(1));

        let missing = StandardMaterial {
            normal_map_texture: Some(Handle::default()),
            ..default()
        };
        assert!(
            validate_standard_material(&missing, &images)
                .unwrap_err()
                .contains("normal image")
        );
    }

    #[test]
    fn rejects_invalid_alpha_and_culling_semantics() {
        let images = Assets::<Image>::default();
        assert!(
            validate_standard_material(
                &StandardMaterial {
                    alpha_mode: AlphaMode::Mask(f32::NAN),
                    ..default()
                },
                &images
            )
            .is_err()
        );
        assert!(
            validate_standard_material(
                &StandardMaterial {
                    double_sided: true,
                    ..default()
                },
                &images
            )
            .is_err()
        );
    }

    /// A far reference exercises model-space bounds without world-space precision loss.
    fn far_reference() -> Transform {
        Transform::from_rotation(Quat::from_rotation_z(0.41)).with_translation(
            Vec3::new(120_000.0, -95_000.0, 8_000.0) + Vec3::new(1.37, -2.19, 0.43),
        )
    }

    #[derive(Resource, Default)]
    struct BoundsOutcome {
        expected: Option<ExpectedModelBounds>,
        bounds: Option<Result<SpawnedBounds, String>>,
        check: Option<Result<TransformValidationSummary, String>>,
    }

    /// Runs the real validation entry point with the same queries the streaming system
    /// hands it, capturing both the bounds it measured and its verdict.
    fn run_bounds_check(
        mut outcome: ResMut<BoundsOutcome>,
        roots: Query<(
            Entity,
            &Transform,
            &GlobalTransform,
            &WorldTransform,
            Option<&ExpectedModelBounds>,
        )>,
        transforms: SpawnedNodeQuery,
        children: Query<&Children>,
        primitives: RenderPrimitiveQuery,
        meshes: Res<Assets<Mesh>>,
    ) {
        let Ok((root, local, global, world_transform, expected)) = roots.single() else {
            panic!("the fixture must spawn exactly one model root");
        };
        outcome.expected = expected.copied();
        outcome.bounds = Some(spawned_relative_bounds(
            root,
            &children,
            &transforms,
            &primitives,
            &meshes,
        ));
        outcome.check = Some(validate_spawned_transforms_and_bounds(
            root,
            local,
            global,
            world_transform,
            expected,
            &children,
            &transforms,
            &primitives,
            &meshes,
        ));
    }

    /// Spawns what a converted asset produces: a reference root, a rotated and offset
    /// intermediate node, and a rotated, scaled and offset mesh node under it, with every
    /// `GlobalTransform` composed exactly as Bevy's transform propagation composes it. The
    /// returned bounds are the aggregate model-space bounds the converter would have
    /// written for the unshifted mesh; `mesh_shift` displaces the mesh afterwards to model
    /// a genuinely wrong hierarchy.
    fn spawn_bounds_fixture(
        world: &mut World,
        root_local: Transform,
        node_local: Transform,
        mesh_local: Transform,
        mesh_shift: Vec3,
        mesh: Handle<Mesh>,
    ) -> (Entity, ExpectedModelBounds) {
        let (min, max) = {
            let aabb = world
                .resource::<Assets<Mesh>>()
                .get(&mesh)
                .and_then(|mesh| mesh.compute_aabb())
                .expect("the fixture mesh has finite POSITION bounds");
            let center = Vec3::from(aabb.center);
            let half_extents = Vec3::from(aabb.half_extents);
            let converted = InstanceBounds::transformed(
                center - half_extents,
                center + half_extents,
                node_local.to_matrix() * mesh_local.to_matrix(),
            );
            (converted.min, converted.max)
        };
        let root_global = GlobalTransform::from(root_local.to_matrix());
        let root = world
            .spawn((
                root_local,
                root_global,
                WorldTransform(root_local.to_matrix()),
                ExpectedModelBounds { min, max },
            ))
            .id();
        let node_global = root_global.mul_transform(node_local);
        let node = world.spawn((node_local, node_global, ChildOf(root))).id();
        let mut mesh_local = mesh_local;
        mesh_local.translation += mesh_shift;
        world.spawn((
            mesh_local,
            node_global.mul_transform(mesh_local),
            Mesh3d(mesh),
            ChildOf(node),
        ));
        (root, ExpectedModelBounds { min, max })
    }

    fn bounds_case(root_local: Transform, mesh_shift: Vec3) -> BoundsOutcome {
        let mut app = App::new();
        let mut meshes = Assets::<Mesh>::default();
        let mesh = meshes.add(Cuboid::new(2.0, 4.0, 6.0));
        app.insert_resource(meshes);
        spawn_bounds_fixture(
            app.world_mut(),
            root_local,
            scene_node(),
            rotated_mesh_node(),
            mesh_shift,
            mesh,
        );
        app.init_resource::<BoundsOutcome>()
            .add_systems(Update, run_bounds_check);
        app.update();
        app.world_mut()
            .remove_resource::<BoundsOutcome>()
            .expect("the bounds check system ran")
    }

    /// The intermediate node a glTF scene hangs under the instance reference: it moves and
    /// rotates the model without carrying a mesh of its own.
    fn scene_node() -> Transform {
        Transform::from_rotation(Quat::from_rotation_x(-0.55) * Quat::from_rotation_z(0.3))
            .with_translation(Vec3::new(-1.23, 4.07, 0.61))
    }

    fn rotated_mesh_node() -> Transform {
        Transform::from_rotation(
            Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.35) * Quat::from_rotation_z(-0.2),
        )
        .with_translation(Vec3::new(3.37, 0.51, -2.19))
        .with_scale(Vec3::new(1.5, 0.75, 2.0))
    }

    #[test]
    fn bounds_check_ignores_the_reference_distance_from_the_origin() {
        let mut at_origin = bounds_case(Transform::default(), Vec3::ZERO);
        let mut far = bounds_case(far_reference(), Vec3::ZERO);

        let (at_origin_check, far_check) = (
            at_origin.check.take().expect("the bounds check ran"),
            far.check.take().expect("the bounds check ran"),
        );
        assert!(at_origin_check.is_ok(), "{at_origin_check:?}");
        assert!(far_check.is_ok(), "{far_check:?}");
        let expected = far.expected.expect("the fixture carries converted bounds");
        assert_eq!(expected, at_origin.expected.unwrap());
        let at_origin = at_origin
            .bounds
            .expect("bounds were measured")
            .expect("the hierarchy has bounded meshes");
        let far = far
            .bounds
            .expect("bounds were measured")
            .expect("the hierarchy has bounded meshes");
        assert_eq!(
            at_origin.nodes, 2,
            "the fixture has two descendants under the reference"
        );
        assert_eq!(
            far.nodes, 2,
            "the fixture has two descendants under the reference"
        );

        for (label, bounds) in [("origin", at_origin), ("far from the origin", far)] {
            assert!(
                bounds.min.abs_diff_eq(expected.min, 1.0e-5)
                    && bounds.max.abs_diff_eq(expected.max, 1.0e-5),
                "measured bounds at the {label} ({:?}..{:?}) differ from the converted bounds {:?}..{:?}",
                bounds.min,
                bounds.max,
                expected.min,
                expected.max
            );
        }
        assert!(
            far.min.abs_diff_eq(at_origin.min, 1.0e-5)
                && far.max.abs_diff_eq(at_origin.max, 1.0e-5),
            "measured bounds moved with the reference: {:?}..{:?} at the origin, {:?}..{:?} far from it",
            at_origin.min,
            at_origin.max,
            far.min,
            far.max
        );
    }

    #[test]
    fn bounds_check_rejects_a_moved_mesh_at_any_distance_from_the_origin() {
        for root_local in [Transform::default(), far_reference()] {
            let outcome = bounds_case(root_local, Vec3::new(0.5, 0.0, 0.0));
            let reason = outcome
                .check
                .expect("the bounds check ran")
                .expect_err("a mesh moved 0.5 units from the converted bounds must be rejected");
            assert!(
                reason.contains("spawned hierarchy bounds diverge from conversion"),
                "unexpected rejection reason at {root_local:?}: {reason}"
            );
        }
    }

    /// The bounds check on a model whose mesh node has been turned about the vertical, as
    /// `billboard.rs` turns a hearth's flame cards toward the camera; `billboard` says whether the
    /// node carries its [`Billboard`](crate::billboard::Billboard) marker.
    fn turned_node_case(billboard: bool) -> Result<TransformValidationSummary, String> {
        let mut app = App::new();
        let mut meshes = Assets::<Mesh>::default();
        let mesh = meshes.add(Cuboid::new(2.0, 4.0, 6.0));
        app.insert_resource(meshes);
        spawn_bounds_fixture(
            app.world_mut(),
            Transform::default(),
            scene_node(),
            rotated_mesh_node(),
            Vec3::ZERO,
            mesh,
        );
        let world = app.world_mut();
        let node = world
            .query_filtered::<Entity, With<Mesh3d>>()
            .single(world)
            .expect("the fixture has one mesh node");
        let authored = rotated_mesh_node().rotation;
        let mut entity = world.entity_mut(node);
        entity.get_mut::<Transform>().unwrap().rotation = Quat::from_rotation_y(1.1) * authored;
        if billboard {
            entity.insert(crate::billboard::Billboard {
                mode: crate::billboard::BillboardMode::TurnAboutUp,
                authored,
            });
        }
        app.init_resource::<BoundsOutcome>()
            .add_systems(Update, run_bounds_check);
        app.update();
        app.world_mut()
            .remove_resource::<BoundsOutcome>()
            .expect("the bounds check system ran")
            .check
            .expect("the bounds check ran")
    }

    #[test]
    fn bounds_check_measures_a_billboard_at_its_authored_rotation() {
        let turned_billboard = turned_node_case(true);
        assert!(
            turned_billboard.is_ok(),
            "a billboard turned toward the camera must pass: {turned_billboard:?}"
        );
        let reason = turned_node_case(false)
            .expect_err("the same turn on an ordinary node is a real divergence");
        assert!(
            reason.contains("spawned hierarchy bounds diverge from conversion"),
            "{reason}"
        );
    }

    /// The bounds check on a model whose mesh node a clip has moved after its scene spawned, as
    /// `model_animation.rs` turns the mill wheel. `animated` tags the node as a clip target
    /// ([`AnimationTargetId`], as the glTF loader does under an animation root); `authored_shift`
    use bevy::world_serialization::WorldSerializationPlugin;

    /// The app the empty-model tests run in: the real readiness scan
    /// ([`track_asset_readiness`]) over an asset server and the world serialization spawner the
    /// engine uses, so a model reference is spawned and becomes ready by the same route a
    /// converted glb takes.
    ///
    /// The model is a [`WorldAsset`] really added to the asset server, so
    /// `is_loaded_with_dependencies` is true for it exactly as it is for a loaded glb. An empty
    /// `World` is what the converter's empty scene produces: no entity and no render primitive.
    fn empty_model_app() -> (App, Handle<WorldAsset>) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            WorldSerializationPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .insert_resource(EngineConfig::default())
        .init_resource::<StreamingMetrics>()
        .init_resource::<ProfilingState>()
        .init_resource::<DiagnosticFallbackAssets>()
        .init_resource::<StaticCollisionCache>()
        .init_resource::<crate::pacing::PacingTracker>()
        .add_observer(mark_world_instance_ready)
        .add_systems(Update, track_asset_readiness);
        let handle = app
            .world()
            .resource::<AssetServer>()
            .add(WorldAsset::new(World::new()));
        // The `Loaded` event is applied in the asset schedule, before the spawner reads it.
        app.update();
        (app, handle)
    }

    /// A reference as [`spawn_cell`] spawns one for a model: the root components, the asset root
    /// pointing at the loaded scene, and the pending profile the readiness scan waits on. No
    /// `ExpectedModelBounds` is inserted, which is what `statics.bounds_valid = 0` produces -
    /// the component's absence is the whole signal.
    fn spawn_model_reference(
        app: &mut App,
        handle: Handle<WorldAsset>,
        expected_bounds: Option<ExpectedModelBounds>,
    ) -> Entity {
        let transform = Transform::from_translation(Vec3::new(3.0, -4.0, 5.0));
        let mut entity = app.world_mut().spawn((
            Name::new("Reference 000F9907"),
            FormId(0x00F9907),
            CellRef(0x02D4E0),
            transform,
            GlobalTransform::from(transform),
            WorldTransform(transform.to_matrix()),
            WorldAssetRoot(handle),
            PendingAssetProfile {
                started: Instant::now(),
                scene_spawned: false,
                path: "meshes/furniture/creatureexit/wispambush.glb".to_owned(),
                form_id: 0x00F9907,
                base_form_id: 0x00EF957,
                base_record_type: Some("STAT".to_owned()),
                static_physics: false,
                cell_id: 0x02D4E0,
            },
        ));
        if let Some(bounds) = expected_bounds {
            entity.insert(bounds);
        }
        entity.id()
    }

    /// Runs the readiness scan to completion and reads the metrics back.
    fn settle_readiness(app: &mut App) -> StreamingMetrics {
        for _ in 0..8 {
            app.update();
        }
        app.world().resource::<StreamingMetrics>().clone()
    }

    // --- Paced model spawns (upstream main's `arm_pending_models`) ------------------------------

    #[test]
    fn cluster_ownership_defers_instantiation_until_the_certificate_can_decide() {
        use crate::cluster::{
            ownership::{OwnershipFacts, OwnershipReference, install},
            reader::producer_fixture_tests::published_for_loader,
        };
        let directory = published_for_loader();
        let package = std::sync::Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        let mut app = model_app();
        install(&mut app);
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 4.0, 6.0));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let model = add_converted_model(&mut app, converted_scene_with_material(mesh, material));
        let root = spawn_pending_reference(&mut app, model, 0);
        app.world_mut()
            .entity_mut(root)
            .insert(OwnershipReference(0x00F9907));
        app.update();
        assert!(app.world().get::<PendingModel>(root).is_some());
        assert!(app.world().get::<WorldAssetRoot>(root).is_none());
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .pending_asset_instances,
            1
        );
        // The certificate proves this reference is not owned; unrelated unresolved
        // certified references must not prevent its ordinary model from spawning.
        app.world_mut()
            .resource_mut::<OwnershipFacts>()
            .publish(&package, Default::default());
        app.update();
        assert!(app.world().get::<PendingModel>(root).is_none());
        assert!(app.world().get::<WorldAssetRoot>(root).is_some());
    }

    #[test]
    fn a_fixed_static_gets_a_proxy_collider_and_a_door_gets_none() {
        let mut app = model_app();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 4.0, 6.0));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let model = add_converted_model(&mut app, converted_scene_with_material(mesh, material));
        // The converted (lowercase) path `converted_model_path` returns, which the eligibility
        // rules are written in.
        let stat = spawn_pending_reference_with(
            &mut app,
            model.clone(),
            0,
            "meshes/architecture/farmhouse/farmhouse01.glb",
            "STAT",
            true,
        );
        app.world_mut().entity_mut(stat).insert(cuboid_bounds());
        let door = spawn_pending_reference_with(
            &mut app,
            model,
            1,
            "meshes/architecture/farmhouse/farmhouseldoor01.glb",
            "DOOR",
            true,
        );
        app.world_mut().entity_mut(door).insert(cuboid_bounds());

        let metrics = settle_armed_readiness(&mut app);

        assert_eq!(
            metrics.static_colliders_proxy, 1,
            "one of the two placements is known collision: {:?}",
            metrics.asset_failures
        );
        assert_eq!(
            metrics.static_colliders_skipped, 0,
            "a DOOR is not eligible for collision, which is not the same as a proxy that failed"
        );
        let world = app.world();
        let stat_entity = world.entity(stat);
        assert_eq!(
            stat_entity.get::<StaticColliderSource>(),
            Some(&StaticColliderSource::RenderProxy)
        );
        assert!(stat_entity.contains::<Collider>());
        assert!(stat_entity.contains::<RigidBody>());
        assert!(
            !stat_entity.contains::<ColliderDisabled>(),
            "a vanilla static proxy is enabled without a private space gate"
        );
        let door_entity = world.entity(door);
        assert!(
            door_entity.get::<StaticColliderSource>().is_none(),
            "a load door gets no collider"
        );
        assert!(!door_entity.contains::<Collider>());
    }

    fn model_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            WorldSerializationPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .insert_resource(EngineConfig::default())
        .init_resource::<StreamingMetrics>()
        .init_resource::<StreamingWorld>()
        .init_resource::<lod::LodStreaming>()
        .init_resource::<TerrainContinuity>()
        .init_resource::<ActiveSpace>()
        .init_resource::<ProfilingState>()
        .init_resource::<DiagnosticFallbackAssets>()
        .init_resource::<StaticCollisionCache>()
        .init_resource::<crate::pacing::PacingTracker>()
        .init_resource::<SceneSpawnBatch>()
        // The converted scene holds entities, and the spawner reads each of their components out
        // of the type registry.
        .register_type::<ChildOf>()
        .register_type::<Children>()
        .register_type::<GlobalTransform>()
        .register_type::<Mesh3d>()
        .register_type::<MeshMaterial3d<StandardMaterial>>()
        .register_type::<Name>()
        .register_type::<Transform>()
        .add_observer(mark_world_instance_ready)
        .add_systems(
            Update,
            (despawn_cells, arm_pending_models, track_asset_readiness).chain(),
        )
        .add_systems(
            SpawnScene,
            (
                begin_scene_spawn_batch.before(SceneSpawnerSystems::WorldInstanceSpawn),
                end_scene_spawn_batch.after(SceneSpawnerSystems::WorldInstanceSpawn),
            ),
        );
        app
    }

    /// Adds `scene` to the asset server as the converted model a reference points at, and runs the
    /// frame the asset system needs to publish it, so `is_loaded_with_dependencies` is true for it
    /// exactly as it is for a loaded glb.
    fn add_converted_model(app: &mut App, scene: World) -> Handle<WorldAsset> {
        let handle = app
            .world()
            .resource::<AssetServer>()
            .add(WorldAsset::new(scene));
        // The `Loaded` event is applied in the asset schedule, before the spawner reads it.
        app.update();
        handle
    }

    /// A converted model as the loader builds one for the converter's empty scene: the scene's own
    /// root entity and nothing below it, so the model has no node and no mesh.
    fn empty_converted_scene() -> World {
        let mut world = World::new();
        world.spawn((Name::new("wispambush"), Transform::default()));
        world
    }

    /// A converted model whose primitive passes readiness: the mesh the converter exported and the
    /// material it points at, so the scan has something to validate rather than an empty model.
    fn converted_scene_with_material(
        mesh: Handle<Mesh>,
        material: Handle<StandardMaterial>,
    ) -> World {
        let mut world = World::new();
        // The loader gives every node a transform, the scene's own root included: the bounds walk
        // reads one from every node below the reference.
        let root = world
            .spawn((Name::new("wispambush"), Transform::default()))
            .id();
        world.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            ChildOf(root),
        ));
        world
    }

    /// The aggregate bounds the converter writes for a `Cuboid::new(2.0, 4.0, 6.0)` primitive in a
    /// reference's own frame, which is the frame the bounds check compares them in.
    fn cuboid_bounds() -> ExpectedModelBounds {
        ExpectedModelBounds::new(Vec3::new(-1.0, -2.0, -3.0), Vec3::new(1.0, 2.0, 3.0))
            .expect("the fixture bounds are finite and not degenerate")
    }

    /// A config whose arming budget is `budget` models per frame.
    fn paced_config(budget: usize) -> EngineConfig {
        EngineConfig {
            max_model_spawns_per_frame: budget,
            ..EngineConfig::default()
        }
    }

    /// A reference as [`spawn_cell`] leaves one for [`arm_pending_models`]: the root components and
    /// the model it found, but still pending, with `sequence` its place in spawn order. The cell
    /// path assigns that sequence from [`StreamingWorld::next_model_sequence`] as it walks the
    /// cell's references, so handing the sequence in is what a test does instead of committing a
    /// cell.
    fn spawn_pending_reference(app: &mut App, handle: Handle<WorldAsset>, sequence: u64) -> Entity {
        spawn_pending_reference_with(
            app,
            handle,
            sequence,
            "meshes/furniture/creatureexit/wispambush.glb",
            "STAT",
            false,
        )
    }

    /// The same reference with the model path, base record type and walk gate a collision test
    /// needs: those three are the whole of what the static-collision rules read.
    fn spawn_pending_reference_with(
        app: &mut App,
        handle: Handle<WorldAsset>,
        sequence: u64,
        path: &str,
        record_type: &str,
        static_physics: bool,
    ) -> Entity {
        let transform = Transform::from_translation(Vec3::new(3.0, -4.0, 5.0));
        app.world_mut()
            .spawn((
                Name::new("Reference 000F9907"),
                FormId(0x00F9907),
                CellRef(0x02D4E0),
                transform,
                GlobalTransform::from(transform),
                WorldTransform(transform.to_matrix()),
                PendingModel { handle, sequence },
                PendingAssetProfile {
                    started: Instant::now(),
                    scene_spawned: false,
                    path: path.to_owned(),
                    form_id: 0x00F9907,
                    base_form_id: 0x00EF957,
                    base_record_type: Some(record_type.to_owned()),
                    static_physics,
                    cell_id: 0x02D4E0,
                },
            ))
            .id()
    }

    /// The references the arming pacer has handed to the spawner and the ones still waiting for
    /// their turn, which is the whole queue: arming removes the component that put a model in it.
    fn armed_and_pending(app: &mut App) -> (usize, usize) {
        let mut armed = app
            .world_mut()
            .query_filtered::<Entity, With<WorldAssetRoot>>();
        let armed = armed.iter(app.world()).count();
        let mut pending = app
            .world_mut()
            .query_filtered::<Entity, With<PendingModel>>();
        let pending = pending.iter(app.world()).count();
        (armed, pending)
    }

    /// Runs the readiness scan until the reference leaves the pending set, and fails the test
    /// rather than reading an unsettled metric. The world instance is spawned in `SpawnScene` and
    /// the scan runs in `Update`, so a reference settles over more than one frame. A model still
    /// waiting in the arming queue is not pending yet, so the queue has to drain too.
    fn settle_armed_readiness(app: &mut App) -> StreamingMetrics {
        for _ in 0..16 {
            app.update();
            let streaming = app.world().resource::<StreamingMetrics>();
            if streaming.pending_asset_instances == 0 && streaming.arming_queue_depth == 0 {
                break;
            }
        }
        let metrics = app.world().resource::<StreamingMetrics>().clone();
        assert_eq!(
            metrics.pending_asset_instances, 0,
            "the reference never left the pending set"
        );
        assert_eq!(
            metrics.arming_queue_depth, 0,
            "the arming queue never drained"
        );
        metrics
    }

    /// The spawn-batch measurement reads the world rather than the spawner's bookkeeping: three
    /// models whose assets are already loaded are reported as three instances on the frame Bevy
    /// writes them into the world, and as none on the frame after, while the readiness scan
    /// completes the three of them on its own, later, scan.
    #[test]
    fn the_spawn_batch_counts_the_instances_bevy_instantiated_this_frame() {
        let mut app = model_app();
        // Every model is published before any reference points at it, so all three references are
        // handed to the spawner on the same frame.
        let handles: Vec<_> = (0..3)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        for handle in handles {
            spawn_model_reference(&mut app, handle, None);
        }

        app.update();
        let arrival = streaming_metrics(&app);
        assert_eq!(
            arrival.instances_spawned_this_frame, 3,
            "three references were handed to the spawner with their assets loaded"
        );
        assert_eq!(
            arrival.instances_armed_this_frame, 3,
            "the references joining the spawner's queue are the models armed this frame"
        );
        assert_eq!(
            arrival.instances_completed_this_scan, 0,
            "the scan validates an instance on a later frame than the one it spawns on"
        );

        app.update();
        let quiet = streaming_metrics(&app);
        assert_eq!(
            quiet.instances_spawned_this_frame, 0,
            "an instance spawns once, so the frame after reports none"
        );
        assert_eq!(
            quiet.instances_armed_this_frame, 0,
            "a model arms once, so the frame after reports none"
        );
        assert_eq!(quiet.max_instances_spawned_per_frame, 3);
        assert_eq!(
            quiet.instances_completed_this_scan, 3,
            "the scan completes the three instances it accepts"
        );
        assert_eq!(quiet.max_instances_completed_per_scan, 3);
        assert_eq!(
            quiet.empty_model_references, 3,
            "the empty converted scenes are counted, not failed"
        );
    }

    /// Pacing is what a frame instantiates: a cell's models wait in the arming queue and are handed
    /// over a budget at a time, so the spawn batch is the armed batch and a drained queue stays
    /// drained.
    #[test]
    fn the_arming_budget_spreads_a_cells_models_over_frames() {
        let mut app = model_app();
        app.insert_resource(paced_config(2));
        let handles: Vec<_> = (0..5)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        for (sequence, handle) in handles.into_iter().enumerate() {
            spawn_pending_reference(&mut app, handle, sequence as u64);
        }

        app.update();
        let first = streaming_metrics(&app);
        assert_eq!(
            armed_and_pending(&mut app),
            (2, 3),
            "a budget of two hands two models over and leaves the rest queued"
        );
        assert_eq!(first.arming_queue_depth, 3);
        assert_eq!(
            first.peak_arming_queue_depth, 5,
            "the peak is the cell's whole backlog"
        );
        assert_eq!(
            first.instances_spawned_this_frame, 2,
            "the frame instantiates the models the budget armed, not the whole cell"
        );

        app.update();
        assert_eq!(armed_and_pending(&mut app), (4, 1));
        app.update();
        assert_eq!(armed_and_pending(&mut app), (5, 0));
        assert_eq!(streaming_metrics(&app).arming_queue_depth, 0);

        app.update();
        assert_eq!(
            armed_and_pending(&mut app),
            (5, 0),
            "a model left the queue when it was armed, so none is armed twice"
        );
        assert_eq!(streaming_metrics(&app).instances_spawned_this_frame, 0);
    }

    /// A backlog drains oldest first, whatever order the references were spawned in: the entities
    /// here are created newest-first, so arming in entity order would arm the wrong model.
    #[test]
    fn the_arming_budget_arms_the_oldest_reference_first() {
        let mut app = model_app();
        app.insert_resource(paced_config(1));
        let handles: Vec<_> = (0..4)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        let mut oldest_first = Vec::new();
        for (index, handle) in handles.into_iter().enumerate() {
            // The reference spawned first is the newest model: the sequences run the other way.
            oldest_first.push(spawn_pending_reference(&mut app, handle, 3 - index as u64));
        }
        oldest_first.reverse();

        for (frame, reference) in oldest_first.iter().enumerate() {
            app.update();
            assert!(
                app.world().entity(*reference).contains::<WorldAssetRoot>(),
                "frame {frame} armed a newer model while the oldest one was still waiting"
            );
            assert_eq!(armed_and_pending(&mut app), (frame + 1, 3 - frame));
        }
    }

    /// The arming budget grows with a burst so the whole window drains in about a second, never
    /// past eight times its floor; a small backlog keeps the configured floor, and 0 still means
    /// every ready model.
    #[test]
    fn the_arming_budget_grows_with_a_burst_within_its_cap() {
        assert_eq!(models_to_arm(4, 3), 3, "never more than is ready");
        assert_eq!(models_to_arm(4, 100), 4, "a small backlog keeps the floor");
        assert_eq!(models_to_arm(4, 600), 10, "600 ready drain over 60 frames");
        assert_eq!(models_to_arm(4, 2_744), 32, "capped at 8x the floor");
        assert_eq!(models_to_arm(0, 2_744), 2_744, "0 arms every ready model");
    }

    /// With a camera, the model nearest to it is armed first, whatever its age: the player sees the
    /// cell in front of them fill in before the ones behind.
    #[test]
    fn the_arming_budget_arms_the_model_nearest_the_camera_first() {
        let mut app = model_app();
        let mut config = paced_config(1);
        config.prioritize_streaming = true;
        app.insert_resource(config);
        app.world_mut().spawn((
            StreamingCamera,
            Transform::default(),
            GlobalTransform::IDENTITY,
        ));
        let far = add_converted_model(&mut app, empty_converted_scene());
        let near = add_converted_model(&mut app, empty_converted_scene());
        // The far model is the older one, so commit order alone would arm it first.
        let far = spawn_pending_reference(&mut app, far, 0);
        let near = spawn_pending_reference(&mut app, near, 1);
        let place = |app: &mut App, entity: Entity, at: Vec3| {
            let transform = Transform::from_translation(at);
            let model = app.world().get::<PendingModel>(entity).unwrap();
            let request = requests::SceneRequest {
                path: app
                    .world()
                    .get::<PendingAssetProfile>(entity)
                    .unwrap()
                    .path
                    .clone(),
                cell: None,
                coarse_terrain: false,
                retry_cleanup: None,
                content_identity: None,
                sequence: model.sequence,
                center: at.as_dvec3(),
                radius: Some(1.0),
                collision_candidate: false,
                handle: Some(model.handle.clone()),
                started: Instant::now(),
                allow_retry: false,
            };
            app.world_mut().entity_mut(entity).insert((
                transform,
                GlobalTransform::from(transform),
                request,
            ));
        };
        place(&mut app, far, Vec3::new(5_000.0, 0.0, 0.0));
        place(&mut app, near, Vec3::new(10.0, 0.0, 0.0));

        app.update();
        assert!(app.world().entity(near).contains::<WorldAssetRoot>());
        assert!(!app.world().entity(far).contains::<WorldAssetRoot>());
    }

    /// A budget is spent on the models that can spawn now: a model whose converted scene has not
    /// arrived keeps its place in the queue without holding up the models behind it, and takes its
    /// turn as soon as it can.
    #[test]
    fn a_model_still_loading_does_not_use_a_budget_slot() {
        let mut app = model_app();
        app.insert_resource(paced_config(2));
        // A handle `Assets<WorldAsset>` has no entry for is the state a loading model is in: the
        // loader has not published the scene yet. The default handle is exactly that - identity
        // without storage - and this one gets its scene before the test ends.
        let loading = Handle::<WorldAsset>::default();
        let oldest = spawn_pending_reference(&mut app, loading.clone(), 0);
        let handles: Vec<_> = (0..4)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        for (sequence, handle) in handles.into_iter().enumerate() {
            spawn_pending_reference(&mut app, handle, sequence as u64 + 1);
        }

        app.update();
        assert!(
            !app.world().entity(oldest).contains::<WorldAssetRoot>(),
            "a model whose scene has not arrived is not armed"
        );
        assert_eq!(
            armed_and_pending(&mut app),
            (2, 3),
            "the budget went to the two models that are ready, not to the loading one"
        );
        assert_eq!(streaming_metrics(&app).arming_queue_depth, 3);

        app.world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .insert(&loading, WorldAsset::new(empty_converted_scene()))
            .expect("publishing the model's scene cannot fail");
        app.update();
        assert!(
            app.world().entity(oldest).contains::<WorldAssetRoot>(),
            "the loading model is armed once its scene arrives"
        );
        assert_eq!(armed_and_pending(&mut app), (4, 1));
    }

    /// A model whose cell is unloaded before its turn needs no cleanup, and the pacer cannot trip
    /// over it: the queue is the component set, and the despawn takes it with the subtree.
    #[test]
    fn a_pending_model_whose_cell_is_despawned_leaves_nothing_queued() {
        let mut app = model_app();
        app.insert_resource(paced_config(1));
        let handle = add_converted_model(&mut app, empty_converted_scene());
        let cell = app
            .world_mut()
            .spawn((
                Name::new("Cell"),
                StreamedCellRoot,
                ExteriorCellGrid(IVec2::ZERO),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        for sequence in 0..2 {
            let reference = spawn_pending_reference(&mut app, handle.clone(), sequence);
            app.world_mut().entity_mut(reference).insert(ChildOf(cell));
        }
        // The cell is outside the unload radius and its turn in the unload budget has come.
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                exterior_cell(4, 4),
                CellStatus::Retiring {
                    root: cell,
                    detail: CellDetail::Full,
                },
            );

        app.update();

        assert_eq!(
            armed_and_pending(&mut app),
            (0, 0),
            "the cell took its pending models with it; none was armed and none is queued"
        );
        let metrics = streaming_metrics(&app);
        assert_eq!(metrics.arming_queue_depth, 0);
        assert_eq!(metrics.unloaded_cells, 1, "the cell itself was unloaded");
        assert_eq!(metrics.instances_spawned_this_frame, 0);
    }

    /// `0` is the unbudgeted behaviour: every model whose scene is ready is handed over at once.
    #[test]
    fn an_arming_budget_of_zero_arms_every_ready_model() {
        let mut app = model_app();
        app.insert_resource(paced_config(0));
        let handles: Vec<_> = (0..5)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        for (sequence, handle) in handles.into_iter().enumerate() {
            spawn_pending_reference(&mut app, handle, sequence as u64);
        }

        app.update();
        assert_eq!(armed_and_pending(&mut app), (5, 0));
        assert_eq!(
            streaming_metrics(&app).instances_spawned_this_frame,
            5,
            "without a budget the whole batch lands on one frame, as it did before the pacer"
        );
    }

    /// Pacing moves the frame a model is spawned on, not the totals: every queued model is armed
    /// once, spawned once, and validated exactly once, and no frame instantiates more than the
    /// budget.
    #[test]
    fn every_paced_model_is_spawned_and_validated_exactly_once() {
        let mut app = model_app();
        app.insert_resource(paced_config(2));
        let mut handles = Vec::new();
        for _ in 0..5 {
            let mesh = app
                .world_mut()
                .resource_mut::<Assets<Mesh>>()
                .add(Cuboid::new(2.0, 4.0, 6.0));
            let material = app
                .world_mut()
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial::default());
            handles.push(add_converted_model(
                &mut app,
                converted_scene_with_material(mesh, material),
            ));
        }
        for (sequence, handle) in handles.into_iter().enumerate() {
            let reference = spawn_pending_reference(&mut app, handle, sequence as u64);
            app.world_mut()
                .entity_mut(reference)
                .insert(cuboid_bounds());
        }

        let metrics = settle_armed_readiness(&mut app);

        assert_eq!(
            metrics.assets_ready, 5,
            "every instance is validated once: {:?}",
            metrics.asset_failures
        );
        assert_eq!(metrics.meshes_validated, 5);
        assert_eq!(metrics.materials_validated, 5);
        assert_eq!(metrics.transform_instances_validated, 5);
        assert_eq!(metrics.empty_model_references, 0);
        assert_eq!(metrics.peak_arming_queue_depth, 5);
        assert_eq!(
            metrics.max_instances_spawned_per_frame, 2,
            "no frame instantiated more than the budget allowed"
        );
        assert_eq!(armed_and_pending(&mut app), (5, 0));
        let mut instances = app
            .world_mut()
            .query_filtered::<Entity, With<WorldInstance>>();
        assert_eq!(
            instances.iter(app.world()).count(),
            5,
            "one instance per reference, paced or not"
        );
    }

    /// The converter exports an editor marker as an **empty scene** on purpose - every shape is
    /// dropped - so the model has no converted bounds and no render primitive. Two of them sit in
    /// Blackreach cell `0002D4E0` (`WispAmbush` REFR 000F9907, `FrostSpiderAmbush01` REFR
    /// 0007C068), and a run there used to fail the bounds gate on a model with nothing to draw.
    /// Such a reference is skipped and counted on its own, and nothing fails.
    #[test]
    fn an_empty_scene_model_is_skipped_and_counted_instead_of_failing() {
        let (mut app, handle) = empty_model_app();
        let reference = spawn_model_reference(&mut app, handle, None);

        let metrics = settle_readiness(&mut app);

        assert_eq!(
            metrics.transform_bounds_validation_failures, 0,
            "an invisible marker is not a conversion failure: {:?}",
            metrics.asset_failures
        );
        assert_eq!(
            metrics.empty_model_references, 1,
            "the empty model is counted on its own"
        );
        assert!(
            metrics.asset_failures.is_empty(),
            "nothing is recorded as a failed asset: {:?}",
            metrics.asset_failures
        );
        assert_eq!(
            metrics.pending_asset_instances, 0,
            "the instance stops waiting for its asset"
        );
        assert_eq!(
            metrics.bounds_validated, 0,
            "there were no converted bounds to validate"
        );
        assert_eq!(
            metrics.assets_ready, 0,
            "nothing was spawned, so the instance is not a ready asset either"
        );
        assert!(
            !app.world()
                .entity(reference)
                .contains::<PendingAssetProfile>(),
            "the reference is no longer pending"
        );
        assert!(
            app.world().entity(reference).contains::<Transform>(),
            "the empty reference keeps its own transform"
        );
    }

    /// The empty-scene rule is not a way to accept a model that does have geometry: a scene with a
    /// render primitive but no converted bounds is still a conversion defect, and still fatal.
    #[test]
    fn a_model_with_primitives_but_no_converted_bounds_still_fails() {
        let mut app = model_app();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 4.0, 6.0));
        let mut converted = World::new();
        let scene_root = converted.spawn(Transform::default()).id();
        converted.spawn((Mesh3d(mesh), Transform::default(), ChildOf(scene_root)));
        let handle = add_converted_model(&mut app, converted);
        spawn_model_reference(&mut app, handle, None);

        let metrics = settle_readiness(&mut app);

        assert_eq!(
            metrics.empty_model_references, 0,
            "a model with geometry is never counted as empty"
        );
        assert_eq!(
            metrics.transform_bounds_validation_failures, 1,
            "a model with geometry and no converted bounds must still fail: {:?}",
            metrics.asset_failures
        );
        let failure = metrics
            .asset_failures
            .first()
            .expect("the failure is recorded as an asset failure");
        assert!(
            failure
                .dependency_chain
                .iter()
                .any(|reason| reason.contains("no validated aggregate bounds")),
            "unexpected failure reason: {:?}",
            failure.dependency_chain
        );
    }

    /// The other direction of the same rule: a model the converter *did* bound, whose spawned
    /// scene turns out to hold no render primitive, is not an empty marker to wave through - the
    /// two disagree and the disagreement is fatal.
    #[test]
    fn a_bounded_model_whose_scene_is_empty_still_fails() {
        let (mut app, handle) = empty_model_app();
        spawn_model_reference(
            &mut app,
            handle,
            ExpectedModelBounds::new(Vec3::splat(-1.0), Vec3::splat(1.0)),
        );

        let metrics = settle_readiness(&mut app);

        assert_eq!(
            metrics.empty_model_references, 0,
            "only a model without converted bounds may be an empty marker"
        );
        assert_eq!(
            metrics.transform_bounds_validation_failures, 1,
            "a bound model whose hierarchy holds no mesh must still fail: {:?}",
            metrics.asset_failures
        );
        let failure = metrics
            .asset_failures
            .first()
            .expect("the failure is recorded as an asset failure");
        assert!(
            failure
                .dependency_chain
                .iter()
                .any(|reason| reason.contains("no bounded mesh")),
            "unexpected failure reason: {:?}",
            failure.dependency_chain
        );
    }

    #[test]
    fn lod_slots_are_counted_apart_from_cells_and_held_by_the_frame_time_budget() {
        let mut budget = StreamingCommitBudget {
            remaining: 1,
            lod_remaining: 2,
            ..Default::default()
        };
        // The first commit of a frame is never held by time, however late the frame is.
        budget.frame_started = Instant::now() - Duration::from_secs(1);
        assert!(budget.lod_slot_open(16_670));
        budget.spend_lod_slot();
        assert!(
            !budget.lod_slot_open(16_670),
            "the spent time budget holds the next one"
        );
        budget.frame_started = Instant::now();
        assert!(budget.lod_slot_open(16_670));
        budget.spend_lod_slot();
        assert!(!budget.lod_slot_open(16_670), "the LOD count is spent");
        assert_eq!(
            budget.remaining, 1,
            "LOD requests leave the full-cell slot alone"
        );
        assert_eq!(budget.commits, 2);
    }

    fn bare_probe_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_message::<AppExit>();
        app
    }
    fn probe_app(dir: PathBuf) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AppExit>()
            .insert_resource(EngineConfig::default())
            .init_resource::<StreamingWorld>()
            .init_resource::<StreamingMetrics>();
        install_retire_visibility_probe(&mut app, Some(dir));
        app
    }
    fn post_update_systems_len(app: &App) -> usize {
        app.get_schedule(PostUpdate)
            .map(|schedule| schedule.systems_len())
            .unwrap_or(0)
    }
    #[test]
    fn the_retire_visibility_probe_installs_only_when_asked() {
        let mut control = bare_probe_app();
        control.update();
        let baseline = post_update_systems_len(&control);

        let mut off = bare_probe_app();
        install_retire_visibility_probe(&mut off, None);
        off.update();
        assert_eq!(
            post_update_systems_len(&off),
            baseline,
            "an ordinary run installs no probe system"
        );
        assert!(
            off.world()
                .get_resource::<RetireVisibilityProbe>()
                .is_none(),
            "and no probe resource"
        );

        let mut on = probe_app(PathBuf::from("nowhere"));
        on.update();
        assert_eq!(
            post_update_systems_len(&on),
            baseline + 1,
            "the flag installs exactly the probe system and nothing else"
        );
        assert!(
            on.world().get_resource::<RetireVisibilityProbe>().is_some(),
            "and its resource"
        );
    }
    #[test]
    fn the_probe_counts_only_a_retiring_cell_that_shows() {
        let directory = tempfile::tempdir().unwrap();

        // A retiring root left visible, with one mesh child also visible: both are counted.
        let mut visible = probe_app(directory.path().join("visible"));
        let root = visible.world_mut().spawn(ViewVisibility::VISIBLE).id();
        visible.world_mut().spawn((
            Mesh3d(Handle::default()),
            ViewVisibility::VISIBLE,
            ChildOf(root),
        ));
        test_plant_retiring_cell(visible.world_mut(), exterior_cell(9, 0), root);
        visible.update();
        let probe = visible.world().resource::<RetireVisibilityProbe>();
        assert_eq!(
            probe.visible_hits, 2,
            "the retiring root and its mesh child both read as visible"
        );
        assert_eq!(probe.hit_frames, vec![1], "the frame the hit showed on");
        assert_eq!(probe.frames_probed, 1);
        assert_eq!(probe.max_backlog, 1);

        // A retiring root hidden, as a distance retire leaves one: nothing is counted.
        let mut hidden = probe_app(directory.path().join("hidden"));
        let root = hidden
            .world_mut()
            .spawn((Visibility::Hidden, ViewVisibility::HIDDEN))
            .id();
        hidden.world_mut().spawn((
            Mesh3d(Handle::default()),
            ViewVisibility::HIDDEN,
            ChildOf(root),
        ));
        test_plant_retiring_cell(hidden.world_mut(), exterior_cell(9, 0), root);
        hidden.update();
        let probe = hidden.world().resource::<RetireVisibilityProbe>();
        assert_eq!(
            probe.visible_hits, 0,
            "a hidden retiring cell shows nothing"
        );
        assert!(probe.hit_frames.is_empty());
        assert_eq!(probe.frames_probed, 1, "the backlog frame was still probed");
    }
}

#[cfg(test)]
mod upstream_tests {
    use super::*;
    use crate::{
        physics::DebugTankard,
        render::WATER_LAYER,
        world::components::{FormId, MeshHandle},
    };

    use bevy::ecs::system::RunSystemOnce;
    use bevy_rapier3d::prelude::{
        AdditionalMassProperties, ColliderMassProperties, QueryFilter, ReadRapierContext, Velocity,
    };

    #[test]
    fn a_capsule_hull_covers_the_diagonals_of_its_end_spheres() {
        let radius = 2.0;
        let capsule = CollisionShape::Capsule {
            a: [0.0, 0.0, 0.0],
            b: [0.0, 0.0, 0.0],
            radius,
        };
        let mut points = Vec::new();
        shape_hull_points(&capsule, &mut points);
        assert_eq!(points.len(), 52, "26 points at each of the two ends");
        // The hull's reach along a direction is the largest dot product over its points. Along
        // any direction it must stay close to the sphere's radius; six axis points only reach
        // 0.577 of it along a corner diagonal.
        let mut worst = f32::MAX;
        for x in -4..=4 {
            for y in -4..=4 {
                for z in -4..=4 {
                    let direction = Vec3::new(x as f32, y as f32, z as f32);
                    if direction == Vec3::ZERO {
                        continue;
                    }
                    let direction = direction.normalize();
                    let reach = points
                        .iter()
                        .map(|point| point.dot(direction))
                        .fold(f32::MIN, f32::max);
                    worst = worst.min(reach / radius);
                }
            }
        }
        assert!(worst > 0.88, "the hull falls short of the sphere: {worst}");
    }

    #[test]
    fn one_commit_budget_alternates_priority_without_starving_cells_or_lod() {
        let mut budget = StreamingCommitBudget {
            remaining: 1,
            ..Default::default()
        };
        budget.reserve_for_lod(true, 1);
        assert_eq!(budget.remaining_for_cells(), 1);
        budget.lod_priority = true;
        budget.reserve_for_lod(true, 1);
        assert_eq!(budget.remaining_for_cells(), 0);
        budget.reserve_for_lod(false, 1);
        assert_eq!(budget.remaining_for_cells(), 1);
        budget.remaining = 2;
        budget.lod_priority = false;
        budget.reserve_for_lod(true, 2);
        assert_eq!(budget.remaining_for_cells(), 1);
        budget.remaining -= 1;
        assert_eq!(
            budget.remaining_for_cells(),
            0,
            "reservation survives a near-cell commit"
        );
        budget.remaining = 0;
        assert_eq!(budget.remaining_for_cells(), 0);
    }

    /// The converter writes a sphere as a zero-length capsule (a = b); Rapier must take it as a
    /// sphere of that radius.
    #[test]
    fn zero_length_authored_capsule_becomes_a_sphere() {
        let asset = CollisionAsset {
            version: COLLISION_ASSET_VERSION,
            authored: true,
            shapes: vec![CollisionShape::Capsule {
                a: [1.0, 2.0, 3.0],
                b: [1.0, 2.0, 3.0],
                radius: 5.0,
            }],
            skipped: Vec::new(),
            bodies: Vec::new(),
        };
        let parts = collider_parts_from_authored(&asset).unwrap();
        assert_eq!(parts.len(), 1);
        let aabb = parts[0].collider.raw.compute_local_aabb();
        let (mins, maxs) = (aabb.mins, aabb.maxs);
        assert!(
            (mins - Vec3::new(-4.0, -3.0, -2.0)).length() < 1e-4
                && (maxs - Vec3::new(6.0, 7.0, 8.0)).length() < 1e-4,
            "{mins:?} {maxs:?}"
        );
    }

    #[test]
    fn multiple_authored_meshes_attach_to_one_fixed_body_without_nested_composites() {
        let mut app = crate::physics::headless::fixture_app();
        let asset = CollisionAsset {
            version: COLLISION_ASSET_VERSION,
            authored: true,
            shapes: [0.0_f32, 100.0]
                .map(|x| CollisionShape::Mesh {
                    vertices: vec![
                        [x - 20.0, 0.0, -20.0],
                        [x - 20.0, 0.0, 20.0],
                        [x + 20.0, 0.0, 20.0],
                        [x + 20.0, 0.0, -20.0],
                    ],
                    triangles: vec![[0, 1, 2], [0, 2, 3]],
                })
                .into(),
            skipped: Vec::new(),
            bodies: Vec::new(),
        };
        let parts = collider_parts_from_authored(&asset).unwrap();
        let root = app
            .world_mut()
            .spawn(Transform::from_xyz(10_000.0, 0.0, 0.0))
            .id();
        app.world_mut()
            .run_system_once(move |mut commands: Commands| {
                attach_authored_collision(&mut commands, root, &parts);
            })
            .unwrap();
        let mut parts_query = app.world_mut().query::<(Entity, &ChildOf)>();
        let parts: Vec<_> = parts_query
            .iter(app.world())
            .filter_map(|(entity, parent)| (parent.parent() == root).then_some(entity))
            .collect();
        assert_eq!(parts.len(), 2);
        for entity in std::iter::once(root).chain(parts) {
            assert!(!app.world().entity(entity).contains::<ColliderDisabled>());
        }
        for _ in 0..3 {
            app.update();
        }
        let hits = app
            .world_mut()
            .run_system_once(move |context: ReadRapierContext| {
                let context = context.single().unwrap();
                let body = context
                    .rigidbody_set
                    .bodies
                    .get(context.entity2body()[&root])
                    .unwrap();
                assert_eq!(body.colliders().len(), 2);
                [0.0_f32, 100.0].map(|x| {
                    context.cast_ray(
                        Vec3::new(10_000.0 + x, 50.0, 0.0),
                        Vec3::NEG_Y,
                        100.0,
                        true,
                        QueryFilter::default(),
                    )
                })
            })
            .unwrap();
        for hit in hits {
            let (_, distance) = hit.expect("authored mesh must be queryable");
            assert!(
                (distance - 50.0).abs() < 0.01,
                "unexpected contact: {distance}"
            );
        }
    }

    #[test]
    fn authored_collision_and_absence_override_legacy_render_proxy_policy() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>();
        assert!(fixed_collision_record_eligible(Some("FURN")));
        assert!(!fixed_collision_record_eligible(Some("MISC")));
        assert!(!static_proxy_eligible(
            Some("FURN"),
            "meshes/furniture/clutter/milllogpile.glb"
        ));
        for (shapes, expected_solid) in [
            (
                vec![CollisionShape::Box {
                    center: [0.0, 5.0, 0.0],
                    half_extents: [10.0, 5.0, 10.0],
                }],
                true,
            ),
            (Vec::new(), false),
        ] {
            let root = app.world_mut().spawn_empty().id();
            let asset = CollisionAsset {
                version: COLLISION_ASSET_VERSION,
                authored: true,
                shapes,
                skipped: Vec::new(),
                bodies: Vec::new(),
            };
            app.world_mut().spawn((
                GltfSceneExtras {
                    value: serde_json::json!({"mudcrabCollision": asset}).to_string(),
                },
                ChildOf(root),
            ));
            for record_type in ["STAT", "FURN"] {
                let decision = app
                    .world_mut()
                    .run_system_once(
                        move |children: Query<&Children>,
                              scene_extras: Query<&GltfSceneExtras>,
                              transforms: SpawnedNodeQuery,
                              primitives: RenderPrimitiveQuery,
                              meshes: Res<Assets<Mesh>>,
                              materials: Res<Assets<StandardMaterial>>| {
                            static_collision_from_hierarchy(
                                Some(record_type),
                                "meshes/architecture/farmhouse/inn01.glb",
                                root,
                                &children,
                                &scene_extras,
                                &transforms,
                                &primitives,
                                &meshes,
                                &materials,
                            )
                        },
                    )
                    .unwrap();
                assert_eq!(
                    matches!(decision, StaticCollisionDecision::Authored(_, _)),
                    expected_solid
                );
                assert!(matches!(
                    decision,
                    StaticCollisionDecision::Authored(_, _)
                        | StaticCollisionDecision::AuthoredAbsent
                ));
            }
        }
    }

    #[test]
    fn collision_authored_under_the_pre_rename_key_still_loads() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>();
        let root = app.world_mut().spawn_empty().id();
        // Built from JSON rather than a struct literal, so a field added to `CollisionAsset`
        // with a serde default does not break this test.
        let mut asset: CollisionAsset = serde_json::from_value(serde_json::json!({
            "version": COLLISION_ASSET_VERSION,
            "authored": true,
            "shapes": [],
            "skipped": [],
        }))
        .unwrap();
        asset.shapes.push(CollisionShape::Box {
            center: [0.0, 5.0, 0.0],
            half_extents: [10.0, 5.0, 10.0],
        });
        // The key is built in two halves so scripts/rename-to-mudcrab.py leaves it alone.
        let old_key = ["openSkyrim", "Collision"].concat();
        app.world_mut().spawn((
            GltfSceneExtras {
                value: serde_json::json!({ old_key: asset }).to_string(),
            },
            ChildOf(root),
        ));
        let decision = app
            .world_mut()
            .run_system_once(
                move |children: Query<&Children>,
                      scene_extras: Query<&GltfSceneExtras>,
                      transforms: SpawnedNodeQuery,
                      primitives: RenderPrimitiveQuery,
                      meshes: Res<Assets<Mesh>>,
                      materials: Res<Assets<StandardMaterial>>| {
                    static_collision_from_hierarchy(
                        Some("STAT"),
                        "meshes/architecture/farmhouse/inn01.glb",
                        root,
                        &children,
                        &scene_extras,
                        &transforms,
                        &primitives,
                        &meshes,
                        &materials,
                    )
                },
            )
            .unwrap();
        assert!(matches!(decision, StaticCollisionDecision::Authored(_, _)));
    }

    #[test]
    fn static_proxy_policy_excludes_movable_and_decorative_records() {
        assert!(static_proxy_eligible(
            Some("STAT"),
            "meshes/landscape/rocks/rockl01.glb"
        ));
        assert!(static_proxy_eligible(
            Some("STAT"),
            "meshes/architecture/farmhouse/inn01.glb"
        ));
        for path in [
            "meshes/clutter/firewood/firewoodpilelarge01.glb",
            "meshes/landscape/trees/treepineforestlog01.glb",
            "meshes/landscape/roads/roadstraightlongramp01.glb",
        ] {
            assert!(static_proxy_eligible(Some("STAT"), path), "{path}");
        }
        assert!(static_proxy_eligible(
            Some("TREE"),
            "meshes/landscape/trees/treepineforest01.glb"
        ));
        assert!(!static_proxy_eligible(
            Some("TREE"),
            "meshes/landscape/plants/clover01.glb"
        ));
        for kind in [None, Some("MISC"), Some("TREE"), Some("MSTT"), Some("DOOR")] {
            assert!(!static_proxy_eligible(
                kind,
                "meshes/architecture/farmhouse/inn01.glb"
            ));
        }
        assert!(!static_proxy_eligible(
            Some("STAT"),
            "meshes/landscape/plants/fern.glb"
        ));
        let blended = StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..default()
        };
        let masked = StandardMaterial {
            alpha_mode: AlphaMode::Mask(0.5),
            ..default()
        };
        assert!(static_proxy_material_allowed(
            "meshes/landscape/rocks/rockcliff02.glb",
            None,
            &blended
        ));
        assert!(!static_proxy_material_allowed(
            "meshes/landscape/rocks/rockl01.glb",
            None,
            &blended
        ));
        assert!(!static_proxy_material_allowed(
            "meshes/architecture/farmhouse/ivy01.glb",
            None,
            &masked
        ));
        assert!(!static_proxy_material_allowed(
            "meshes/landscape/rocks/rockcliff02.glb",
            None,
            &masked
        ));
        assert!(static_proxy_material_allowed(
            "meshes/architecture/farmhouse/lumbermill01.glb",
            Some("LumbermillMesh:19 - L1_Posts01:19"),
            &masked
        ));
        assert!(!static_proxy_material_allowed(
            "meshes/architecture/farmhouse/lumbermill01.glb",
            Some("LumbermillMesh:20"),
            &masked
        ));
        assert!(!static_proxy_material_allowed(
            "meshes/architecture/farmhouse/inn01.glb",
            Some("LumbermillMesh:19"),
            &masked
        ));
    }

    #[test]
    fn static_proxy_bakes_nested_node_transforms_and_leaves_openings() {
        use bevy_rapier3d::rapier::parry::{math::Pose, query::Ray};

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin));
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>();
        let post = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(20.0, 100.0, 10.0));
        let opaque = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let root = app.world_mut().spawn(Transform::default()).id();
        let node = app
            .world_mut()
            .spawn((Transform::from_xyz(30.0, 0.0, 0.0), ChildOf(root)))
            .id();
        for offset in [-40.0, 40.0] {
            app.world_mut().spawn((
                Mesh3d(post.clone()),
                MeshMaterial3d(opaque.clone()),
                Transform::from_xyz(offset, 0.0, 0.0),
                ChildOf(node),
            ));
        }
        app.update();
        let proxy = app
            .world_mut()
            .run_system_once(
                move |children: Query<&Children>,
                      transforms: SpawnedNodeQuery,
                      primitives: RenderPrimitiveQuery,
                      meshes: Res<Assets<Mesh>>,
                      materials: Res<Assets<StandardMaterial>>| {
                    static_proxy_from_hierarchy(
                        "meshes/architecture/farmhouse/inn01.glb",
                        root,
                        &children,
                        &transforms,
                        &primitives,
                        &meshes,
                        &materials,
                    )
                },
            )
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(proxy.as_trimesh().is_some());
        let gap = Ray::new(Vec3::new(30.0, 0.0, 50.0), Vec3::NEG_Z);
        let post = Ray::new(Vec3::new(-10.0, 0.0, 50.0), Vec3::NEG_Z);
        assert!(
            proxy
                .raw
                .cast_ray(&Pose::IDENTITY, &gap, 100.0, true)
                .is_none()
        );
        assert!(
            proxy
                .raw
                .cast_ray(&Pose::IDENTITY, &post, 100.0, true)
                .is_some()
        );
        // Rapier applies the placement rotation and scale after node transforms are baked.
        let mut physics = crate::physics::headless::fixture_app();
        let placement = physics
            .world_mut()
            .spawn((
                RigidBody::Fixed,
                proxy,
                crate::physics::world_collision_groups(),
                Transform::from_xyz(200.0, 300.0, 200.0)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::splat(2.0)),
            ))
            .id();
        for _ in 0..3 {
            physics.update();
        }
        let (post_hit, gap_hit) = physics
            .world_mut()
            .run_system_once(move |context: ReadRapierContext| {
                let context = context.single().unwrap();
                let only_placement = |entity| entity == placement;
                let filter =
                    bevy_rapier3d::prelude::QueryFilter::default().predicate(&only_placement);
                (
                    context.cast_ray(
                        Vec3::new(500.0, 300.0, 220.0),
                        Vec3::NEG_X,
                        600.0,
                        true,
                        filter,
                    ),
                    context.cast_ray(
                        Vec3::new(500.0, 300.0, 140.0),
                        Vec3::NEG_X,
                        600.0,
                        true,
                        filter,
                    ),
                )
            })
            .unwrap();
        assert!(post_hit.is_some(), "scaled, rotated post had no contact");
        assert!(gap_hit.is_none(), "scaled, rotated opening was filled");
    }

    #[test]
    fn lumbermill_walkway_mask_contributes_without_roof_mask() {
        use bevy_rapier3d::rapier::parry::{math::Pose, query::Ray};

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin));
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>();
        let slab = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(20.0, 4.0, 20.0));
        let masked = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                alpha_mode: AlphaMode::Mask(0.5),
                ..default()
            });
        let root = app.world_mut().spawn(Transform::default()).id();
        for (x, name) in [
            (0.0, "LumbermillMesh:19 - L1_Posts01:19"),
            (100.0, "LumbermillMesh:20"),
        ] {
            app.world_mut().spawn((
                Mesh3d(slab.clone()),
                MeshMaterial3d(masked.clone()),
                GltfMaterialName(name.to_owned()),
                Transform::from_xyz(x, 10.0, 0.0),
                ChildOf(root),
            ));
        }
        app.update();
        let proxy = app
            .world_mut()
            .run_system_once(
                move |children: Query<&Children>,
                      transforms: SpawnedNodeQuery,
                      primitives: RenderPrimitiveQuery,
                      meshes: Res<Assets<Mesh>>,
                      materials: Res<Assets<StandardMaterial>>| {
                    static_proxy_from_hierarchy(
                        "meshes/architecture/farmhouse/lumbermill01.glb",
                        root,
                        &children,
                        &transforms,
                        &primitives,
                        &meshes,
                        &materials,
                    )
                },
            )
            .unwrap()
            .unwrap()
            .unwrap();
        for (x, expected) in [(0.0, true), (100.0, false)] {
            let ray = Ray::new(Vec3::new(x, 50.0, 0.0), Vec3::NEG_Y);
            assert_eq!(
                proxy
                    .raw
                    .cast_ray(&Pose::IDENTITY, &ray, 100.0, true)
                    .is_some(),
                expected,
                "unexpected lumbermill proxy contact at x={x}"
            );
        }
    }

    #[test]
    fn walk_capsule_and_dynamic_tankard_contact_static_proxy() {
        use bevy_rapier3d::prelude::{ColliderMassProperties, Velocity};

        let mut app = crate::physics::headless::fixture_app();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(20.0, 200.0, 400.0));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let wall = app
            .world_mut()
            .spawn(Transform::from_xyz(250.0, 100.0, 850.0))
            .id();
        app.world_mut().spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            ChildOf(wall),
        ));
        app.update();
        let proxy = app
            .world_mut()
            .run_system_once(
                move |children: Query<&Children>,
                      transforms: SpawnedNodeQuery,
                      primitives: RenderPrimitiveQuery,
                      meshes: Res<Assets<Mesh>>,
                      materials: Res<Assets<StandardMaterial>>| {
                    static_proxy_from_hierarchy(
                        "meshes/architecture/farmhouse/inn01.glb",
                        wall,
                        &children,
                        &transforms,
                        &primitives,
                        &meshes,
                        &materials,
                    )
                },
            )
            .unwrap()
            .unwrap()
            .unwrap();
        app.world_mut().entity_mut(wall).insert((
            StaticColliderSource::RenderProxy,
            RigidBody::Fixed,
            proxy,
            crate::physics::world_collision_groups(),
        ));
        crate::physics::headless::place_player(&mut app, Vec3::new(0.0, 200.0, 900.0));
        let tankard = app
            .world_mut()
            .spawn((
                DebugTankard,
                RigidBody::Dynamic,
                crate::physics::debug_tankard_collider(),
                crate::physics::tankard_collision_groups(),
                ColliderMassProperties::Density(0.001),
                Velocity::linear(Vec3::X * 500.0),
                Transform::from_xyz(60.0, 150.0, 750.0),
            ))
            .id();
        app.insert_resource(crate::physics::WalkIntent {
            wish_dir: Vec3::X,
            target_speed: crate::physics::MovementTuning::default().run_speed,
            jump_pressed: false,
        });
        let mut max_tankard_x = f32::NEG_INFINITY;
        for _ in 0..180 {
            app.update();
            max_tankard_x =
                max_tankard_x.max(app.world().get::<Transform>(tankard).unwrap().translation.x);
        }
        let (player, _) = crate::physics::headless::player_pose(&mut app);
        assert!(
            player.x < 225.0,
            "WALK capsule crossed static wall: {player:?}"
        );
        assert!(
            max_tankard_x < 270.0,
            "dynamic tankard tunneled through static wall: {max_tankard_x}"
        );
    }

    #[test]
    fn terrain_sampler_check_rejects_default_and_clamped_images() {
        assert!(!terrain_sampler_repeats(&ImageSampler::Default));
        assert!(!terrain_sampler_repeats(&ImageSampler::linear()));
        assert!(terrain_sampler_repeats(&ImageSampler::Descriptor(
            bevy::image::ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..bevy::image::ImageSamplerDescriptor::linear()
            }
        )));
    }

    #[test]
    fn commit_budget_ignores_only_the_documented_scheduler_tolerance() {
        assert!(!commit_budget_exceeded(16_670, 16_670));
        assert!(!commit_budget_exceeded(17_670, 16_670));
        assert!(commit_budget_exceeded(17_671, 16_670));
    }

    fn terrain_fixture(cell_id: u32, height: f32) -> TerrainSnapshot {
        TerrainSnapshot {
            cell_id,
            width: 33,
            height: 33,
            heights: vec![height; 33 * 33],
            normals: (0..33 * 33).flat_map(|_| [0, 0, 127]).collect(),
            vertex_colors: vec![255; 33 * 33 * 3],
            layers: (0..4)
                .map(|quadrant| TerrainLayerSnapshot {
                    texture_form_id: u32::from(quadrant) + 1,
                    quadrant,
                    layer: 0,
                    is_base: true,
                    weights: Vec::new(),
                })
                .collect(),
            water_height: None,
            water_type_form_id: None,
        }
    }

    #[test]
    fn terrain_colliders_match_each_render_quadrant() {
        let terrain = terrain_fixture(91, 120.0);
        for quadrant in 0..4 {
            let mesh = build_terrain_quadrant_mesh(&terrain, quadrant).unwrap();
            let collider = terrain_collider_from_mesh(&mesh).unwrap();
            let trimesh = collider.as_trimesh().expect("terrain trimesh");
            let VertexAttributeValues::Float32x3(positions) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
            else {
                panic!("terrain positions")
            };
            let Indices::U32(indices) = mesh.indices().unwrap() else {
                panic!("terrain indices")
            };
            assert_eq!(trimesh.vertices().len(), positions.len());
            for (vertex, position) in trimesh.vertices().zip(positions) {
                assert_eq!(vertex, Vec3::from_array(*position));
            }
            assert_eq!(trimesh.indices().len() * 3, indices.len());
            for (triangle, indices) in trimesh.indices().iter().zip(indices.as_chunks::<3>().0) {
                assert_eq!(*triangle, [indices[0], indices[1], indices[2]]);
            }
        }
    }

    #[test]
    fn maps_nif_paths_to_converted_glb_paths() {
        assert_eq!(
            converted_model_path("meshes\\architecture\\wall.nif".into()).as_deref(),
            Some("meshes/architecture/wall.glb")
        );
        assert_eq!(
            converted_model_path("Meshes\\Landscape\\Tundra\\TundraStreamStraight01.NIF".into())
                .as_deref(),
            Some("meshes/landscape/tundra/tundrastreamstraight01.glb")
        );
        assert_eq!(
            converted_model_path("meshes/Sky/CloudShape01.nif".into()),
            None
        );
        assert_eq!(converted_model_path("meshes/Marker_Map.nif".into()), None);
        assert_eq!(
            converted_model_path("Markers/CivilWarMarkers/CWAttSpawn02.nif".into()),
            None
        );
        assert_eq!(converted_model_path("Effects/FXRapids.nif".into()), None);
        assert_eq!(
            converted_model_path("meshes/Furniture/SitLedgeMarker.nif".into()),
            None
        );
    }

    #[test]
    fn rejects_traversal_and_rooted_model_paths() {
        assert_eq!(
            converted_model_path("meshes/../../secrets.nif".into()),
            None
        );
        assert_eq!(converted_model_path("meshes//etc/passwd".into()), None);
        assert_eq!(
            converted_model_path(r"meshes\..\..\secrets.nif".into()),
            None
        );
        // Windows treats a drive-prefixed path as absolute (and `PathBuf::join`
        // would let it replace the base path entirely); Rust's path parsing is
        // OS-native, so this case only bites on the Windows target this engine
        // ships for.
        #[cfg(windows)]
        assert_eq!(
            converted_model_path("meshes/C:/Windows/evil.nif".into()),
            None
        );
        // A plain relative path is unaffected.
        assert_eq!(
            converted_model_path("meshes/architecture/wall.nif".into()).as_deref(),
            Some("meshes/architecture/wall.glb")
        );
    }

    #[test]
    fn unload_radius_removes_distant_exteriors_but_keeps_interiors() {
        let center = IVec2::new(4, -2);
        assert!(upstream_unload_contract(
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 7,
                grid_y: -5,
            },
            center,
            3,
        ));
        assert!(!upstream_unload_contract(
            CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 8,
                grid_y: -2,
            },
            center,
            3,
        ));
        assert!(upstream_unload_contract(CellKey::Interior(99), center, 0));
    }
    fn upstream_unload_contract(key: CellKey, center: IVec2, radius: i32) -> bool {
        let active = match key {
            CellKey::Interior(cell) => ActiveSpace {
                worldspace_id: Some(60),
                interior: Some(cell),
            },
            CellKey::Exterior { worldspace_id, .. } => ActiveSpace {
                worldspace_id: Some(worldspace_id),
                interior: None,
            },
        };
        super::cell_within_unload_radius(key, &active, 60, center, radius, radius.saturating_sub(1))
    }

    /// An app with the cell plan, the unload pacer and the lifecycle validator chained as they run
    /// in `StreamingPlugin`. `collect_cells` is left out, so a requested cell stays `Loading` and
    /// the test drives the resident cells itself through [`spawn_resident_cell`].
    fn streaming_test_app(mut config: EngineConfig) -> (App, tempfile::TempDir) {
        // This harness models upstream's full-cell-only window.
        // Terrain-ring contracts have their own independent tests.
        config.terrain_radius = config.stream_radius;
        config.terrain_radius_given = true;
        // These fixtures assert the count-budget contract. The independent time-budget
        // tests keep the production clock path covered.
        config.max_unload_ms_per_frame = 0.0;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.db");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE schema_info(version INTEGER NOT NULL);
                 INSERT INTO schema_info VALUES({});",
                shared::WORLD_DATABASE_SCHEMA_VERSION
            ))
            .unwrap();
        drop(connection);
        let mut app = App::new();
        app.insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(WorldDatabase::open(&path).unwrap())
            .init_resource::<StreamingWorld>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<lod::LodStreaming>()
            .init_resource::<TerrainContinuity>()
            .init_resource::<ActiveSpace>()
            .init_resource::<ProfilingState>()
            .add_systems(
                Update,
                (plan_cells, despawn_cells, validate_streaming_lifecycle).chain(),
            );
        (app, directory)
    }

    fn spawn_camera(app: &mut App, center: IVec2) {
        app.world_mut().spawn((
            Transform::from_xyz(
                center.x as f32 * CELL_SIZE,
                0.0,
                -(center.y as f32) * CELL_SIZE,
            ),
            StreamingCamera,
        ));
    }

    fn move_camera(app: &mut App, shift: IVec2) {
        let mut camera = app
            .world_mut()
            .query_filtered::<&mut Transform, With<StreamingCamera>>();
        let world = app.world_mut();
        let mut transforms = camera.query_mut(world);
        let mut transform = transforms.single_mut().unwrap();
        transform.translation.x += shift.x as f32 * CELL_SIZE;
        transform.translation.z -= shift.y as f32 * CELL_SIZE;
    }

    fn exterior_key(grid_x: i32, grid_y: i32) -> CellKey {
        CellKey::Exterior {
            worldspace_id: 0x3c,
            grid_x,
            grid_y,
        }
    }

    #[test]
    fn controlled_cell_intake_is_nearest_first_and_bounds_the_entire_outstanding_pipeline() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            max_streaming_backlog: 2,
            max_cell_commits_per_frame: 1,
            prioritize_streaming: false,
            stream_radius: 2,
            unload_radius: 2,
            ..default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        app.update();
        let streaming = app.world().resource::<StreamingWorld>();
        assert_eq!(streaming.outstanding_cells.len(), 1);
        assert!(streaming.cells.contains_key(&exterior_key(0, 0)));
        app.update();
        let streaming = app.world().resource::<StreamingWorld>();
        assert_eq!(streaming.outstanding_cells.len(), 2);
        assert!(streaming.cells.keys().all(|key| {
            matches!(key, CellKey::Exterior { grid_x, grid_y, .. }
                if grid_x * grid_x + grid_y * grid_y <= 1)
        }));
        for _ in 0..12 {
            move_camera(&mut app, IVec2::new(100, 0));
            app.update();
        }
        let streaming = app.world().resource::<StreamingWorld>();
        assert!(streaming.cells.is_empty());
        assert_eq!(streaming.outstanding_cells.len(), 2);
        assert_eq!(streaming_metrics(&app).requests_submitted, 2);
    }

    #[test]
    fn default_cell_intake_still_requests_the_whole_wanted_window() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            stream_radius: 2,
            ..default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        app.update();
        let streaming = app.world().resource::<StreamingWorld>();
        assert_eq!(streaming.cells.len(), 25);
        assert_eq!(streaming.outstanding_cells.len(), 25);
        assert_eq!(streaming_metrics(&app).requests_submitted, 25);
    }

    /// Spawns a cell root with a two-entity subtree under it and records the cell as resident or
    /// retiring, the state a committed cell is in.
    fn spawn_mock_cell(app: &mut App, grid_x: i32, grid_y: i32, resident: bool) -> Entity {
        let root = app
            .world_mut()
            .spawn((
                CellRef(cell_id_of(grid_x, grid_y)),
                StreamedCellRoot,
                ExteriorCellGrid(IVec2::new(grid_x, grid_y)),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        let child = app
            .world_mut()
            .spawn((Transform::default(), TerrainPatch, ChildOf(root)))
            .id();
        app.world_mut()
            .spawn((Transform::default(), ChildOf(child)));
        let status = if resident {
            CellStatus::Resident {
                root,
                detail: CellDetail::Full,
            }
        } else {
            CellStatus::Retiring {
                root,
                detail: CellDetail::Full,
            }
        };
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(exterior_key(grid_x, grid_y), status);
        root
    }

    /// A steady window around `center`: the 7x7 cells the camera wants plus the trailing column a
    /// crossing drops, which is the shape the resident set settles into while flying.
    fn spawn_steady_window(app: &mut App, center: IVec2) {
        for grid_y in (center.y - 3)..=(center.y + 3) {
            for grid_x in (center.x - 4)..=(center.x + 3) {
                spawn_mock_cell(app, grid_x, grid_y, true);
            }
        }
    }

    fn cell_id_of(grid_x: i32, grid_y: i32) -> u32 {
        ((grid_x + 64) as u32) * 256 + (grid_y + 64) as u32
    }

    fn resident_root_count(app: &mut App) -> usize {
        let mut query = app
            .world_mut()
            .query_filtered::<Entity, With<StreamedCellRoot>>();
        let world = app.world();
        query.iter(world).count()
    }

    fn streaming_metrics(app: &App) -> StreamingMetrics {
        app.world().resource::<StreamingMetrics>().clone()
    }

    #[test]
    fn a_zero_unload_budget_unloads_the_whole_crossing_in_one_frame() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            stream_radius: 3,
            unload_radius: 4,
            max_cell_unloads_per_frame: 0,
            ..default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_steady_window(&mut app, IVec2::ZERO);
        let roots = resident_root_count(&mut app);
        app.update();

        move_camera(&mut app, IVec2::X);
        app.update();
        let metrics = streaming_metrics(&app);
        assert_eq!(
            metrics.despawns_this_frame, 7,
            "0 means unbudgeted, as before"
        );
        assert_eq!(metrics.retiring_cells, 0);
        assert_eq!(metrics.retire_backlog_overflows, 0);
        assert_eq!(resident_root_count(&mut app), roots - 7);
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    #[test]
    fn a_crossing_unloads_cells_within_the_budget_and_finishes_within_ceil_frames() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            stream_radius: 3,
            unload_radius: 4,
            max_cell_unloads_per_frame: 2,
            ..default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_steady_window(&mut app, IVec2::ZERO);
        let roots = resident_root_count(&mut app);
        app.update();
        let settled = streaming_metrics(&app);
        assert_eq!(
            settled.unloaded_cells, 0,
            "the window fits inside the radius"
        );
        assert_eq!(settled.retiring_cells, 0);
        assert_eq!(settled.streaming_invariant_failures, 0);

        // Cross into the next cell: the whole trailing column leaves the unload radius at once.
        move_camera(&mut app, IVec2::X);
        let mut unloads_per_frame = Vec::new();
        let mut roots_left = Vec::new();
        for frame in 1..=4 {
            app.update();
            let metrics = streaming_metrics(&app);
            unloads_per_frame.push(metrics.despawns_this_frame);
            assert!(
                metrics.despawns_this_frame <= 2,
                "frame {frame} unloaded {} cells against a budget of 2",
                metrics.despawns_this_frame
            );
            assert_eq!(metrics.streaming_invariant_failures, 0, "frame {frame}");
            roots_left.push(resident_root_count(&mut app));
        }
        assert_eq!(
            unloads_per_frame,
            vec![2, 2, 2, 1],
            "the column's 7 cells must spread over ceil(7 / 2) frames"
        );
        assert_eq!(roots_left, vec![roots - 2, roots - 4, roots - 6, roots - 7]);
        let metrics = streaming_metrics(&app);
        assert_eq!(metrics.unloaded_cells, 7);
        assert_eq!(metrics.retiring_cells, 0);
        assert_eq!(metrics.max_despawns_per_frame, 2);
        assert_eq!(metrics.streaming_invariant_failures, 0);
    }

    #[test]
    fn a_reversal_revives_retiring_cells_without_another_request() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            stream_radius: 3,
            unload_radius: 4,
            max_cell_unloads_per_frame: 2,
            ..default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        spawn_steady_window(&mut app, IVec2::ZERO);
        app.update();
        move_camera(&mut app, IVec2::X);
        app.update();
        let after_crossing = streaming_metrics(&app);
        assert_eq!(
            after_crossing.retiring_cells, 5,
            "the crossing retires a column of 7 and the budget takes 2 in the same frame"
        );
        assert_eq!(after_crossing.unloaded_cells, 2);
        let roots_after_crossing = resident_root_count(&mut app);

        // Turn around before the budget has worked through the column.
        move_camera(&mut app, IVec2::NEG_X);
        app.update();
        let after_reversal = streaming_metrics(&app);
        assert_eq!(after_reversal.revived_cells, 5);
        assert_eq!(after_reversal.retiring_cells, 0);
        assert_eq!(after_reversal.despawns_this_frame, 0);
        assert_eq!(after_reversal.unloaded_cells, 2);
        assert_eq!(
            after_reversal.requests_submitted, after_crossing.requests_submitted,
            "a revived cell keeps its root, so it must not be requested again"
        );
        assert_eq!(resident_root_count(&mut app), roots_after_crossing);
        assert_eq!(after_reversal.streaming_invariant_failures, 0);
    }

    #[test]
    fn retire_backlog_bound_is_one_window_at_the_unload_radius() {
        assert_eq!(retire_backlog_bound(4), 81);
        assert_eq!(retire_backlog_bound(3), 49);
        assert_eq!(retire_backlog_bound(1), 9);
        assert_eq!(retire_backlog_bound(0), 1);
    }

    #[test]
    fn a_backlog_past_the_bound_unloads_every_retiring_cell_at_once() {
        let (mut app, _directory) = streaming_test_app(EngineConfig {
            stream_radius: 1,
            unload_radius: 1,
            max_cell_unloads_per_frame: 2,
            ..default()
        });
        spawn_camera(&mut app, IVec2::ZERO);
        // Under the bound the pacer keeps to the budget: 8 cells wait, 2 go per frame.
        for grid_x in 10..18 {
            spawn_mock_cell(&mut app, grid_x, 0, false);
        }
        app.update();
        let paced = streaming_metrics(&app);
        assert_eq!(paced.retire_backlog_overflows, 0);
        assert_eq!(paced.despawns_this_frame, 2);
        assert_eq!(paced.retiring_cells, 6);
        assert_eq!(
            resident_root_count(&mut app),
            6,
            "the waiters keep their roots"
        );

        // Past the bound the pacer gives up pacing and takes the whole backlog in one frame.
        for grid_x in 20..40 {
            spawn_mock_cell(&mut app, grid_x, 0, false);
        }
        app.update();
        let overflowed = streaming_metrics(&app);
        assert_eq!(overflowed.retire_backlog_overflows, 1);
        assert_eq!(overflowed.despawns_this_frame, 26);
        assert_eq!(overflowed.retiring_cells, 0);
        assert_eq!(overflowed.unloaded_cells, 28);
        assert_eq!(resident_root_count(&mut app), 0);
        assert_eq!(overflowed.streaming_invariant_failures, 0);
    }

    #[test]
    fn repeated_rebasing_preserves_camera_and_cell_root_locality() {
        let mut app = App::new();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<ActiveSpace>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, update_render_origin);
        let camera = app
            .world_mut()
            .spawn((Transform::default(), StreamingCamera))
            .id();
        let root = app
            .world_mut()
            .spawn((ExteriorCellGrid(IVec2::new(8, -3)), Transform::default()))
            .id();
        for shift in [IVec2::new(2, 1), IVec2::new(-3, 4), IVec2::new(7, -2)] {
            {
                let mut entity = app.world_mut().entity_mut(camera);
                let mut transform = entity.get_mut::<Transform>().unwrap();
                transform.translation.x = shift.x as f32 * CELL_SIZE + 12.0;
                transform.translation.z = -(shift.y as f32 * CELL_SIZE) - 20.0;
            }
            app.update();
            let camera_transform = app.world().entity(camera).get::<Transform>().unwrap();
            assert!(camera_transform.translation.x.abs() < CELL_SIZE);
            assert!(camera_transform.translation.z.abs() < CELL_SIZE);
        }
        assert_eq!(app.world().resource::<StreamingMetrics>().origin_rebases, 3);
        let origin = app.world().resource::<RenderOrigin>().0;
        let root_transform = app.world().entity(root).get::<Transform>().unwrap();
        assert_eq!(
            root_transform.translation,
            Vec3::new(
                (8 - origin.x) as f32 * CELL_SIZE,
                0.0,
                -(-3 - origin.y) as f32 * CELL_SIZE,
            )
        );
    }

    #[test]
    fn rebase_moves_dynamic_rapier_pose_with_tankard_transform() {
        let mut app = crate::physics::headless::fixture_app();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<ActiveSpace>()
            .add_systems(Update, update_render_origin);
        let cell = app
            .world_mut()
            .spawn((ExteriorCellGrid(IVec2::ZERO), Transform::default()))
            .id();
        let static_entity = app
            .world_mut()
            .spawn((
                StaticColliderSource::RenderProxy,
                Transform::from_xyz(700.0, 100.0, 700.0),
                ChildOf(cell),
                RigidBody::Fixed,
                Collider::cuboid(20.0, 20.0, 20.0),
                crate::physics::world_collision_groups(),
            ))
            .id();
        for _ in 0..3 {
            app.update();
        }
        let pose = |world: &mut World| {
            world.run_system_once(
                |context: ReadRapierContext, tankards: Query<(Entity, &Transform), With<DebugTankard>>| {
                    let (entity, visual) = tankards.iter().next().unwrap();
                    let context = context.single().unwrap();
                    let handle = context.entity2body()[&entity];
                    (visual.translation, context.rigidbody_set.bodies.get(handle).unwrap().translation())
                },
            ).unwrap()
        };
        let before = pose(app.world_mut());
        let static_x = |world: &mut World| {
            world
                .run_system_once(move |context: ReadRapierContext| {
                    let context = context.single().unwrap();
                    let handle = context.entity2body()[&static_entity];
                    context
                        .rigidbody_set
                        .bodies
                        .get(handle)
                        .unwrap()
                        .translation()
                        .x
                })
                .unwrap()
        };
        let before_static_x = static_x(app.world_mut());
        let camera = {
            let mut query = app
                .world_mut()
                .query_filtered::<Entity, With<StreamingCamera>>();
            query.single(app.world()).unwrap()
        };
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x += CELL_SIZE;
        app.update();
        let after = pose(app.world_mut());
        let after_static_x = static_x(app.world_mut());
        assert!((after.0.x - (before.0.x - CELL_SIZE)).abs() < 0.01);
        assert!((after.1.x - (before.1.x - CELL_SIZE)).abs() < 0.01);
        assert!((after.0.x - after.1.x).abs() < 0.01);
        assert!((after_static_x - (before_static_x - CELL_SIZE)).abs() < 0.01);
        app.world_mut().entity_mut(cell).despawn();
        app.update();
        let body_retained = app
            .world_mut()
            .run_system_once(move |context: ReadRapierContext| {
                context
                    .single()
                    .unwrap()
                    .entity2body()
                    .contains_key(&static_entity)
            })
            .unwrap();
        assert!(!body_retained, "unloaded cell retained its fixed collider");
    }

    #[test]
    fn lifecycle_validator_detects_duplicate_and_orphaned_roots() {
        let mut app = App::new();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<ActiveSpace>()
            .init_resource::<StreamingWorld>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, validate_streaming_lifecycle);
        app.world_mut()
            .spawn((Transform::default(), StreamingCamera));
        let resident = app.world_mut().spawn((CellRef(7), StreamedCellRoot)).id();
        app.world_mut().spawn((CellRef(7), StreamedCellRoot));
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                CellKey::Interior(7),
                CellStatus::Resident {
                    root: resident,
                    detail: CellDetail::Full,
                },
            );
        app.update();
        let metrics = app.world().resource::<StreamingMetrics>();
        assert_eq!(metrics.duplicate_cell_roots, 1);
        assert_eq!(metrics.orphaned_cell_roots, 1);
        assert_eq!(metrics.missing_cell_roots, 0);
        assert_eq!(metrics.streaming_invariant_failures, 2);
    }

    #[test]
    fn streaming_maps_creation_transforms_through_the_shared_contract() {
        assert_eq!(creation_to_bevy(Vec3::Y), Vec3::NEG_Z);
        assert_eq!(creation_to_bevy(Vec3::Z), Vec3::Y);

        // The shared contract decides the angle's sense, not the Bevy axes: this
        // Creation yaw takes +X to runtime +Z, where a Bevy +Y turn takes it to -Z.
        let rotation = creation_rotation_to_bevy([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
        let rotated = rotation * Vec3::X;
        assert!(rotated.abs_diff_eq(Vec3::Z, 1.0e-5));
    }

    #[test]
    fn creates_upward_wound_quadrants_with_continuous_uvs() {
        let terrain = terrain_fixture(1, 0.0);
        let mesh = build_terrain_quadrant_mesh(&terrain, 3).unwrap();
        assert_eq!(mesh.count_vertices(), 17 * 17);
        assert_eq!(mesh.indices().unwrap().len(), 16 * 16 * 6);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let [a, b, c] = [positions[0], positions[1], positions[17]];
        let normal = (Vec3::from(b) - Vec3::from(a)).cross(Vec3::from(c) - Vec3::from(a));
        assert!(normal.y > 0.0);
        let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
        else {
            panic!("terrain UVs must be Float32x2");
        };
        assert_eq!(uvs[0], [0.5, 0.5]);
        assert_eq!(uvs[16 * 17 + 16], [1.0, 1.0]);
    }

    fn exterior_cell(grid_x: i32, grid_y: i32) -> CellKey {
        CellKey::Exterior {
            worldspace_id: 60,
            grid_x,
            grid_y,
        }
    }

    /// The seam measured on real data - Tamriel (18,18) against (18,19), one point of the north
    /// edge 24 units off, across a sample step of 128 units - used to reject the whole cell and
    /// leave a hole where the player stands. It must weld, and nothing but that point may move.
    #[test]
    fn welds_one_point_of_a_shared_edge_below_the_tolerance() {
        let mut continuity = TerrainContinuity::default();
        let mut metrics = StreamingMetrics::default();
        let mut resident = terrain_fixture(1, 10.0);
        validate_and_register_terrain_edges(
            exterior_cell(0, 0),
            &mut resident,
            &mut continuity,
            &mut metrics,
        )
        .unwrap();
        assert_eq!(
            metrics.terrain_seams_validated, 0,
            "the first cell has no registered neighbor to match"
        );

        // The arriving cell matches its resident neighbour along the shared edge except at one
        // point of it, and steps up one sample in from the edge so the weld is visible in the
        // normals as well as in the heights.
        let mut arriving = terrain_fixture(2, 10.0);
        for row in 0..33 {
            arriving.heights[row * 33 + 1] = 100.0;
        }
        arriving.heights[7 * 33] = 34.0;
        let loaded = arriving.heights.clone();
        validate_and_register_terrain_edges(
            exterior_cell(1, 0),
            &mut arriving,
            &mut continuity,
            &mut metrics,
        )
        .unwrap();

        assert_eq!(metrics.terrain_seams_validated, 1);
        assert_eq!(metrics.terrain_seam_points_welded, 1);
        for (index, height) in arriving.heights.iter().enumerate() {
            if index % 33 == 0 {
                assert_eq!(*height, 10.0, "the shared edge is the resident's");
            } else {
                assert_eq!(
                    *height, loaded[index],
                    "a point that is not on the shared edge must not move"
                );
            }
        }
        // Both sides of the seam now hold the same heights, which is what keeps the block
        // watertight for the next cell to arrive.
        let registered = continuity.edges.get(&exterior_cell(1, 0)).unwrap();
        assert_eq!(registered.west, vec![10.0; 33]);
        assert_eq!(
            continuity.edges.get(&exterior_cell(0, 0)).unwrap().east,
            registered.west
        );
        // The moved point's normal was recomputed from the welded field. On the west edge the
        // x stencil spans one interval (the point itself and its east neighbour):
        // ((10 - 100) / 128, 0, 1) normalized and scaled to the `i8` range, rather than the
        // [0, 0, 127] it was loaded with.
        assert_eq!(
            &arriving.normals[7 * 33 * 3..7 * 33 * 3 + 3],
            &[-73, 0, 104]
        );
        assert_eq!(
            &arriving.normals[..3],
            &[0, 0, 127],
            "a point that did not move keeps its normal"
        );
    }

    /// A corner point is the end of two sides, so both of them would weld it; with the two
    /// residents disagreeing there the later side won, the point was counted twice, and one of the
    /// two neighbours was left with the crack. The corner follows the first of its two neighbours
    /// instead, which keeps the difference the residents already had between them and no arriving
    /// cell can close.
    #[test]
    fn welds_a_corner_once_to_the_first_resident_neighbour() {
        let mut continuity = TerrainContinuity::default();
        let mut metrics = StreamingMetrics::default();
        // Two residents sharing only the corner of the arriving cell: (0,1) to its west and (1,0)
        // to its south, 24 units apart there - the worst seam measured on real data.
        for (key, cell_id, height) in [
            (exterior_cell(0, 1), 1, 10.0),
            (exterior_cell(1, 0), 2, 34.0),
        ] {
            validate_and_register_terrain_edges(
                key,
                &mut terrain_fixture(cell_id, height),
                &mut continuity,
                &mut metrics,
            )
            .unwrap();
        }
        assert_eq!(
            metrics.terrain_seams_validated, 0,
            "the two residents are diagonal and share no edge"
        );

        // The arriving cell matches the west resident along their whole shared edge except at the
        // corner, where it holds the south resident's height.
        let mut arriving = terrain_fixture(3, 34.0);
        for row in 0..33 {
            arriving.heights[row * 33] = 10.0;
        }
        arriving.heights[0] = 34.0;
        validate_and_register_terrain_edges(
            exterior_cell(1, 1),
            &mut arriving,
            &mut continuity,
            &mut metrics,
        )
        .unwrap();

        assert_eq!(metrics.terrain_seams_validated, 2);
        assert_eq!(
            metrics.terrain_seam_points_welded, 1,
            "the corner is welded once, not once per side"
        );
        assert_eq!(
            arriving.heights[0], 10.0,
            "the corner follows West, its first side with a resident neighbour"
        );
        assert_eq!(
            arriving.heights[32], 34.0,
            "the other end of the south edge follows the south resident"
        );
        let registered = continuity.edges.get(&exterior_cell(1, 1)).unwrap();
        assert_eq!(registered.west, vec![10.0; 33]);
        let mut expected_south = vec![34.0; 33];
        expected_south[0] = 10.0;
        assert_eq!(
            registered.south, expected_south,
            "the south edge matches the south resident apart from the shared corner"
        );
        assert_eq!(
            continuity.edges.get(&exterior_cell(1, 0)).unwrap().north[0],
            34.0,
            "the residents do not move, so the corner keeps the difference they already had"
        );
        // The welded corner's normal comes from the welded field. Both stencils span one
        // 128-unit interval at a corner: ((10 - 34) / 128, (10 - 10) / 128, 1) normalized and
        // scaled by 127, rather than the [0, 0, 127] it was loaded with.
        assert_eq!(&arriving.normals[..3], &[-23, 0, 125]);
    }

    /// Past the bound the edge is not a seam: it is left as authored, the way Skyrim draws every
    /// LAND on its own. The cell is kept, its other edges within the bound are still welded, and
    /// every edge is registered.
    #[test]
    fn keeps_a_cell_whose_edge_differs_past_the_weld_bound_as_authored() {
        let mut continuity = TerrainContinuity::default();
        let mut metrics = StreamingMetrics::default();
        for (key, cell_id) in [(exterior_cell(0, 0), 1), (exterior_cell(1, 1), 3)] {
            validate_and_register_terrain_edges(
                key,
                &mut terrain_fixture(cell_id, 10.0),
                &mut continuity,
                &mut metrics,
            )
            .unwrap();
        }
        assert_eq!(
            metrics.terrain_seams_validated, 0,
            "the two residents are diagonal and share no edge"
        );

        // The west edge is a weldable seam; the north edge is past the bound, as where a city's
        // sculpted landscape meets the unsculpted land beside it.
        let mut arriving = terrain_fixture(2, 10.0);
        arriving.heights[7 * 33] = 34.0;
        let authored = 10.0 + MAX_WELDABLE_EDGE_DELTA + 1.0;
        arriving.heights[32 * 33 + 15] = authored;
        let key = exterior_cell(1, 0);
        validate_and_register_terrain_edges(key, &mut arriving, &mut continuity, &mut metrics)
            .expect(
                "an edge past the weld bound is drawn as authored, not a reason to drop the cell",
            );
        assert_eq!(
            arriving.heights[7 * 33],
            10.0,
            "the weldable west seam is welded"
        );
        assert_eq!(
            arriving.heights[32 * 33 + 15],
            authored,
            "the edge past the bound keeps its authored heights"
        );
        assert!(
            continuity.edges.contains_key(&key),
            "the kept cell registers its edges for the cells that arrive after it"
        );
        assert_eq!(metrics.terrain_edges_left_as_authored, 1);
        assert_eq!(metrics.terrain_seam_points_welded, 1);
    }

    #[test]
    fn recomputes_packed_normals_from_the_surrounding_heights() {
        let mut terrain = terrain_fixture(1, 0.0);
        let index = 5 * 33 + 5;
        terrain.heights[index + 1] = 100.0;
        recompute_packed_normals(&mut terrain, &[index]);
        // (left - right, down - up, 2 * step) = (-100, 0, 256) over a sample step of 128 units,
        // normalized (length 274.838) and scaled by 127, as the converter's `decode_normals` does.
        assert_eq!(&terrain.normals[index * 3..index * 3 + 3], &[-46, 0, 118]);
        assert_eq!(
            &terrain.normals[..3],
            &[0, 0, 127],
            "a point that was not listed keeps its normal"
        );
    }

    #[test]
    fn a_moved_point_also_refreshes_its_neighbours_normals() {
        let mut terrain = terrain_fixture(1, 0.0);
        let index = 5 * 33 + 5;
        terrain.heights[index] = 100.0;
        let points = points_and_neighbours(&terrain, &[index]);
        assert_eq!(
            points,
            vec![index - 33, index - 1, index, index + 1, index + 33]
        );
        recompute_packed_normals(&mut terrain, &points);
        // Each neighbour's normal reads the moved height: (left - right, down - up, 2 * step)
        // with 100 on one side, (100, 0, 256) normalized and scaled by 127.
        assert_eq!(
            &terrain.normals[(index + 1) * 3..(index + 1) * 3 + 3],
            &[46, 0, 118]
        );
        assert_eq!(
            &terrain.normals[(index - 1) * 3..(index - 1) * 3 + 3],
            &[-46, 0, 118]
        );
        assert_eq!(
            &terrain.normals[(index + 33) * 3..(index + 33) * 3 + 3],
            &[0, 46, 118]
        );
        assert_eq!(
            &terrain.normals[(index - 33) * 3..(index - 33) * 3 + 3],
            &[0, -46, 118]
        );
        // The moved point itself sits between equal heights, so it stays flat.
        assert_eq!(&terrain.normals[index * 3..index * 3 + 3], &[0, 0, 127]);
        // A corner has only two neighbours.
        assert_eq!(points_and_neighbours(&terrain, &[0]), vec![0, 1, 33]);
    }

    /// Welded edge and corner samples have one neighbour per axis, so their stencil spans one
    /// interval, not two. Dividing by the actual span keeps a planar slope's normal identical at
    /// the interior, every edge and every corner, matching the converter's `decode_normals`.
    #[test]
    fn welded_edge_and_corner_normals_match_the_interior_on_a_planar_slope() {
        let mut terrain = terrain_fixture(1, 0.0);
        let width = usize::from(terrain.width);
        // Rises 40 units per interval eastwards and 20 per interval northwards.
        for y in 0..width {
            for x in 0..width {
                terrain.heights[y * width + x] = 40.0 * x as f32 + 20.0 * y as f32;
            }
        }
        let last = width - 1;
        let interior = 16 * width + 16;
        let samples = [
            interior,
            16 * width,          // west edge
            16 * width + last,   // east edge
            16,                  // south edge
            last * width + 16,   // north edge
            0,                   // south-west corner
            last * width + last, // north-east corner
        ];
        recompute_packed_normals(&mut terrain, &samples);
        let expected = terrain.normals[interior * 3..interior * 3 + 3].to_vec();
        // (-40/128, -20/128, 1) normalized and scaled by 127.
        assert_eq!(expected, vec![-37, -19, 120]);
        for index in samples {
            assert_eq!(
                &terrain.normals[index * 3..index * 3 + 3],
                expected.as_slice(),
                "sample {index} disagrees with the interior normal"
            );
        }
    }

    #[test]
    fn rejects_more_than_six_layers_per_quadrant() {
        let mut terrain = terrain_fixture(1, 0.0);
        terrain
            .layers
            .extend((1..=6).map(|layer| TerrainLayerSnapshot {
                texture_form_id: u32::from(layer) + 10,
                quadrant: 0,
                layer,
                is_base: false,
                weights: Vec::new(),
            }));
        assert!(quadrant_layers(&terrain, 0).is_err());
    }

    #[test]
    fn accepts_textureless_official_land_quadrant() {
        let mut terrain = terrain_fixture(1, 0.0);
        terrain.layers.clear();
        assert!(quadrant_layers(&terrain, 0).unwrap().is_empty());
    }

    /// Zero overlays retain ordered weights and geometry validation while missing nonzero
    /// diffuse images still fail locally and a subsequent placeholder snapshot remains usable.
    #[test]
    fn terrain_zero_texture_overlay_preserves_weights_and_nonzero_image_errors() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("placeholder-catalogue.db");
        write_empty_catalogue(&path);
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("INSERT INTO landscape_textures VALUES(2116,NULL);")
            .unwrap();
        drop(connection);
        let catalog = AssetCatalog::open(&path).unwrap();
        let mut terrain = terrain_fixture(91, 12.5);
        for layer in &mut terrain.layers {
            layer.texture_form_id = 0;
        }
        terrain.layers.extend([
            TerrainLayerSnapshot {
                texture_form_id: 0,
                quadrant: 0,
                layer: 4,
                is_base: false,
                weights: vec![(3, 0.625)],
            },
            TerrainLayerSnapshot {
                texture_form_id: 0,
                quadrant: 0,
                layer: 1,
                is_base: false,
                weights: vec![(0, 0.25), (18, 0.75)],
            },
        ]);
        validate_terrain_snapshot(&terrain, &catalog)
            .expect("normalized zero overlays retain their weights without image lookup");
        let layers = quadrant_layers(&terrain, 0).unwrap();
        assert_eq!(
            layers.iter().map(|layer| layer.layer).collect::<Vec<_>>(),
            vec![0, 1, 4]
        );
        let grids = quadrant_overlay_weights(&terrain, 0).unwrap();
        assert_eq!(grids.len(), 2);
        assert_eq!(grids[0][0].to_bits(), 0.25f32.to_bits());
        assert_eq!(grids[0][18].to_bits(), 0.75f32.to_bits());
        assert_eq!(grids[1][3].to_bits(), 0.625f32.to_bits());
        assert_eq!(grids[1][0], 0.0);
        assert!(build_terrain_quadrant_mesh(&terrain, 0).is_ok());

        terrain.layers[4].texture_form_id = 0x844;
        let error = validate_terrain_snapshot(&terrain, &catalog).unwrap_err();
        assert!(error.contains("00000844") && error.contains("no converted diffuse image"));
        terrain.layers[4].texture_form_id = 0;
        assert!(validate_terrain_snapshot(&terrain, &catalog).is_ok());

        terrain.layers[4].weights.push((3, 0.5));
        assert!(
            validate_terrain_snapshot(&terrain, &catalog)
                .unwrap_err()
                .contains("duplicate VTXT")
        );
        terrain.layers[4].weights.pop();
        terrain.heights[0] = f32::NAN;
        assert!(
            validate_terrain_snapshot(&terrain, &catalog)
                .unwrap_err()
                .contains("non-finite height")
        );
    }

    #[test]
    fn overlay_weight_grids_hold_each_layers_vtxt_list_in_layer_order() {
        let mut terrain = terrain_fixture(1, 0.0);
        terrain.layers.extend([
            // Listed out of order: `quadrant_layers` sorts overlays by their ATXT layer number.
            TerrainLayerSnapshot {
                texture_form_id: 22,
                quadrant: 0,
                layer: 2,
                is_base: false,
                weights: vec![(3, 0.25)],
            },
            TerrainLayerSnapshot {
                texture_form_id: 21,
                quadrant: 0,
                layer: 1,
                is_base: false,
                weights: vec![(0, 1.0), (17 * 16 + 16, 0.5)],
            },
        ]);
        let grids = quadrant_overlay_weights(&terrain, 0).unwrap();
        assert_eq!(grids.len(), 2, "the base layer is not an overlay");
        assert_eq!(
            grids[0].len(),
            QUADRANT_WEIGHT_SAMPLES * QUADRANT_WEIGHT_SAMPLES,
            "a grid covers the quadrant's whole sample square"
        );
        assert_eq!(grids[0][0], 1.0);
        assert_eq!(grids[0][17 * 16 + 16], 0.5);
        assert_eq!(grids[0][3], 0.0, "an unnamed sample is opacity 0");
        assert_eq!(grids[1][3], 0.25, "layer 2 is the second overlay");
        assert_eq!(grids[1][0], 0.0);
        assert!(
            quadrant_overlay_weights(&terrain, 1).unwrap().is_empty(),
            "a quadrant with only a base layer has no overlays"
        );
    }

    #[test]
    fn overlay_weight_grids_reject_vtxt_outside_the_sample_square() {
        let mut terrain = terrain_fixture(1, 0.0);
        terrain.layers.push(TerrainLayerSnapshot {
            texture_form_id: 21,
            quadrant: 0,
            layer: 1,
            is_base: false,
            weights: vec![(17 * 17, 1.0)],
        });
        assert!(quadrant_overlay_weights(&terrain, 0).is_err());
    }

    #[test]
    fn validates_loaded_material_images_and_rejects_missing_required_texture() {
        let mut images = Assets::<Image>::default();
        let base_color = images.add(Image::new_fill(
            bevy::render::render_resource::Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            bevy::render::render_resource::TextureDimension::D2,
            &[255, 255, 255, 255],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ));
        let material = StandardMaterial {
            base_color_texture: Some(base_color),
            ..default()
        };
        assert_eq!(validate_standard_material(&material, &images), Ok(1));

        let missing = StandardMaterial {
            normal_map_texture: Some(Handle::default()),
            ..default()
        };
        assert!(
            validate_standard_material(&missing, &images)
                .unwrap_err()
                .contains("normal image")
        );
    }

    #[test]
    fn rejects_invalid_alpha_and_culling_semantics() {
        let images = Assets::<Image>::default();
        assert!(
            validate_standard_material(
                &StandardMaterial {
                    alpha_mode: AlphaMode::Mask(f32::NAN),
                    ..default()
                },
                &images
            )
            .is_err()
        );
        assert!(
            validate_standard_material(
                &StandardMaterial {
                    double_sided: true,
                    ..default()
                },
                &images
            )
            .is_err()
        );
    }

    use crate::render::add_placed_object_layer_propagation;
    use crate::world::database::ReferenceRow;
    use bevy::asset::{AssetApp, AssetPlugin};
    use bevy::world_serialization::WorldSerializationPlugin;

    /// The app the model tests run in: the unload pacer, the arming pacer and the real readiness
    /// scan ([`track_asset_readiness`]) over an asset server and the world serialization spawner
    /// the engine uses, so a converted model is spawned and its reference becomes ready by the same
    /// route a converted glb takes. The three run chained, in the order `StreamingPlugin` runs
    /// them, so a test can tell which frame of that chain a model answered on.
    fn model_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            WorldSerializationPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .insert_resource(EngineConfig::default())
        .init_resource::<StreamingMetrics>()
        .init_resource::<StreamingWorld>()
        .init_resource::<lod::LodStreaming>()
        .init_resource::<TerrainContinuity>()
        .init_resource::<ActiveSpace>()
        .init_resource::<ProfilingState>()
        .init_resource::<DiagnosticFallbackAssets>()
        .init_resource::<SceneSpawnBatch>()
        .init_resource::<StaticCollisionCache>()
        .init_resource::<crate::pacing::PacingTracker>()
        // The converted scene holds entities, and the spawner reads each of their components out
        // of the type registry.
        .register_type::<ChildOf>()
        .register_type::<Children>()
        .register_type::<GlobalTransform>()
        .register_type::<Mesh3d>()
        .register_type::<MeshMaterial3d<StandardMaterial>>()
        .register_type::<Name>()
        .register_type::<Transform>()
        .add_observer(mark_world_instance_ready)
        .add_systems(
            Update,
            (despawn_cells, arm_pending_models, track_asset_readiness).chain(),
        )
        .add_systems(
            SpawnScene,
            (
                begin_scene_spawn_batch.before(SceneSpawnerSystems::WorldInstanceSpawn),
                end_scene_spawn_batch.after(SceneSpawnerSystems::WorldInstanceSpawn),
            ),
        );
        app
    }

    /// Adds `scene` to the asset server as the converted model a reference points at, and runs the
    /// frame the asset system needs to publish it, so `is_loaded_with_dependencies` is true for it
    /// exactly as it is for a loaded glb.
    fn add_converted_model(app: &mut App, scene: World) -> Handle<WorldAsset> {
        let handle = app
            .world()
            .resource::<AssetServer>()
            .add(WorldAsset::new(scene));
        // The `Loaded` event is applied in the asset schedule, before the spawner reads it.
        app.update();
        handle
    }

    /// A converted model as the loader builds one for the converter's empty scene: the scene's own
    /// root entity and nothing below it, so the model has no node and no mesh.
    fn empty_converted_scene() -> World {
        let mut world = World::new();
        world.spawn((Name::new("wispambush"), Transform::default()));
        world
    }

    /// A converted model with real geometry, as the loader builds one: the scene's root and the
    /// mesh primitive below it.
    fn converted_scene_with_mesh(mesh: Handle<Mesh>) -> World {
        let mut world = World::new();
        let root = world.spawn(Name::new("wispambush")).id();
        world.spawn((Mesh3d(mesh), Transform::default(), ChildOf(root)));
        world
    }

    /// A converted model whose primitive passes readiness: the mesh the converter exported and the
    /// material it points at, so the scan has something to validate rather than an empty model.
    fn converted_scene_with_material(
        mesh: Handle<Mesh>,
        material: Handle<StandardMaterial>,
    ) -> World {
        let mut world = World::new();
        // The loader gives every node a transform, the scene's own root included: the bounds walk
        // reads one from every node below the reference.
        let root = world
            .spawn((Name::new("wispambush"), Transform::default()))
            .id();
        world.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            ChildOf(root),
        ));
        world
    }

    /// The aggregate bounds the converter writes for a `Cuboid::new(2.0, 4.0, 6.0)` primitive in a
    /// reference's own frame, which is the frame the bounds check compares them in.
    fn cuboid_bounds() -> ExpectedModelBounds {
        ExpectedModelBounds::new(Vec3::new(-1.0, -2.0, -3.0), Vec3::new(1.0, 2.0, 3.0))
            .expect("the fixture bounds are finite and not degenerate")
    }

    /// A config whose arming budget is `budget` models per frame.
    fn paced_config(budget: usize) -> EngineConfig {
        EngineConfig {
            max_model_spawns_per_frame: budget,
            ..default()
        }
    }

    #[test]
    fn ready_fixed_reference_gets_proxy_but_movable_reference_does_not() {
        let mut app = model_app();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 2.0, 2.0));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let mut scene = World::new();
        let scene_root = scene.spawn(Transform::default()).id();
        scene.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            ChildOf(scene_root),
        ));
        let handle = add_converted_model(&mut app, scene);
        let bounds = ExpectedModelBounds::new(Vec3::splat(-1.0), Vec3::splat(1.0)).unwrap();
        let fixed = spawn_model_reference(&mut app, handle.clone(), Some(bounds));
        let movable = spawn_model_reference(&mut app, handle, Some(bounds));
        for (entity, kind) in [(fixed, "STAT"), (movable, "MISC")] {
            let mut entity_mut = app.world_mut().entity_mut(entity);
            let mut pending = entity_mut.get_mut::<PendingAssetProfile>().unwrap();
            pending.path = "meshes/landscape/rocks/rockl01.glb".to_owned();
            pending.base_record_type = Some(kind.to_owned());
            pending.static_physics = true;
        }
        let metrics = settle_readiness(&mut app);
        assert_eq!(metrics.static_colliders_proxy, 1);
        assert_eq!(metrics.static_colliders_skipped, 0);
        assert_eq!(
            app.world().get::<StaticColliderSource>(fixed),
            Some(&StaticColliderSource::RenderProxy)
        );
        assert!(app.world().get::<Collider>(fixed).is_some());
        assert!(app.world().get::<Collider>(movable).is_none());
    }

    /// The reference root components every model path shares, as [`spawn_cell`] spawns them: the
    /// placement, the ids, and the pending profile the readiness scan waits on.
    fn reference_root() -> impl Bundle {
        let transform = Transform::from_translation(Vec3::new(3.0, -4.0, 5.0));
        (
            Name::new("Reference 000F9907"),
            FormId(0x00F9907),
            CellRef(0x02D4E0),
            transform,
            GlobalTransform::from(transform),
            WorldTransform(transform.to_matrix()),
            Propagate(PLACED_OBJECT_RENDER_LAYERS),
            PendingAssetProfile {
                started: Instant::now(),
                scene_spawned: false,
                path: "meshes/furniture/creatureexit/wispambush.glb".to_owned(),
                form_id: 0x00F9907,
                base_form_id: 0x00EF957,
                base_record_type: None,
                static_physics: false,
                cell_id: 0x02D4E0,
            },
        )
    }

    /// A reference as [`spawn_cell`] spawns one for a model whose asset is already loaded: the root
    /// components, the asset root pointing at the loaded scene, and the pending profile the
    /// readiness scan waits on. No `ExpectedModelBounds` is inserted, which is what
    /// `statics.bounds_valid = 0` produces - the component's absence is the whole signal.
    fn spawn_model_reference(
        app: &mut App,
        handle: Handle<WorldAsset>,
        expected_bounds: Option<ExpectedModelBounds>,
    ) -> Entity {
        let mut entity = app
            .world_mut()
            .spawn((reference_root(), WorldAssetRoot(handle)));
        if let Some(bounds) = expected_bounds {
            entity.insert(bounds);
        }
        entity.id()
    }

    /// A reference as `spawn_cell` leaves one for [`arm_pending_models`]: the same root components
    /// and the model it found, but still pending, with `sequence` its place in spawn order. The
    /// cell path assigns that sequence from [`StreamingWorld::next_model_sequence`] as it walks the
    /// cell's references, so handing the sequence in is what a test does instead of committing a
    /// cell.
    fn spawn_pending_reference(app: &mut App, handle: Handle<WorldAsset>, sequence: u64) -> Entity {
        app.world_mut()
            .spawn((reference_root(), PendingModel { handle, sequence }))
            .id()
    }

    /// The references the arming pacer has handed to the spawner and the ones still waiting for
    /// their turn, which is the whole queue: arming removes the component that put a model in it.
    fn armed_and_pending(app: &mut App) -> (usize, usize) {
        let mut armed = app
            .world_mut()
            .query_filtered::<Entity, With<WorldAssetRoot>>();
        let armed = armed.iter(app.world()).count();
        let mut pending = app
            .world_mut()
            .query_filtered::<Entity, With<PendingModel>>();
        let pending = pending.iter(app.world()).count();
        (armed, pending)
    }

    /// Runs the readiness scan until the reference leaves the pending set, and fails the test
    /// rather than reading an unsettled metric. The world instance is spawned in `SpawnScene` and
    /// the scan runs in `Update`, so a reference settles over more than one frame. A model still
    /// waiting in the arming queue is not pending yet, so the queue has to drain too.
    fn settle_readiness(app: &mut App) -> StreamingMetrics {
        for _ in 0..16 {
            app.update();
            let streaming = app.world().resource::<StreamingMetrics>();
            if streaming.pending_asset_instances == 0 && streaming.arming_queue_depth == 0 {
                break;
            }
        }
        let metrics = app.world().resource::<StreamingMetrics>().clone();
        assert_eq!(
            metrics.pending_asset_instances, 0,
            "the reference never left the pending set"
        );
        assert_eq!(
            metrics.arming_queue_depth, 0,
            "the arming queue never drained"
        );
        metrics
    }

    /// The layer a reference draws on is what keeps it out of the water reflection pass
    /// (`render::REFLECTION_VIEW_LAYERS` renders the world layer only). Only `spawn_cell` spawns
    /// references, so the propagation has to be registered there and nowhere else - in particular
    /// not on the cell root, whose other children are terrain and water.
    #[test]
    fn spawn_cell_leaves_terrain_and_water_on_their_own_layers() {
        let mut app = model_app();
        add_placed_object_layer_propagation(&mut app);
        app.init_resource::<AssetCatalog>()
            .init_asset::<TerrainMaterial>()
            .init_asset::<WaterMaterial>()
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<ActiveSpace>()
            .insert_resource(WaterReflectionTexture(Handle::default()))
            .add_systems(Update, spawn_test_cell);
        app.update();
        app.update();

        let mut references = app
            .world_mut()
            .query_filtered::<(&RenderLayers, &Propagate<RenderLayers>), With<MeshHandle>>();
        let propagated: Vec<_> = references
            .iter(app.world())
            .map(|(layers, propagate)| (layers.clone(), propagate.0.clone()))
            .collect();
        assert_eq!(propagated.len(), 1, "the cell has one model reference");
        for layers in [&propagated[0].0, &propagated[0].1] {
            assert_eq!(
                layers,
                &RenderLayers::layer(crate::render::PLACED_OBJECT_LAYER),
                "a reference and everything below it draws the placed-object layer"
            );
        }

        let mut terrain = app
            .world_mut()
            .query_filtered::<Option<&RenderLayers>, With<TerrainPatch>>();
        let quadrants: Vec<_> = terrain
            .iter(app.world())
            .map(|layers| layers.cloned().unwrap_or_default())
            .collect();
        assert_eq!(quadrants.len(), 4, "the cell has four terrain quadrants");
        for layers in quadrants {
            assert_eq!(
                layers,
                crate::render::reflectable_world_layers(),
                "streamed terrain retains the same content on both reflection routes"
            );
            assert!(
                layers.intersects(&RenderLayers::layer(crate::render::WORLD_LAYER)),
                "terrain is what the reflection pass draws"
            );
            assert!(
                !layers.intersects(&PLACED_OBJECT_RENDER_LAYERS),
                "propagating the placed-object layer from the cell root would drop the terrain \
                 out of the reflection pass"
            );
        }

        let mut water = app
            .world_mut()
            .query_filtered::<&RenderLayers, With<WaterSurface>>();
        let layers = water
            .single(app.world())
            .expect("the cell has one water surface");
        assert_eq!(layers, &RenderLayers::layer(WATER_LAYER));
    }

    /// A model reference's meshes arrive from the converted glb, frames after the reference itself,
    /// and they are what the reflection pass must not draw.
    #[test]
    fn a_reference_glb_lands_on_the_placed_object_layer() {
        let mut app = model_app();
        add_placed_object_layer_propagation(&mut app);
        let handle = add_converted_model(&mut app, converted_scene_with_mesh(Handle::default()));
        let reference = spawn_model_reference(&mut app, handle, None);
        settle_readiness(&mut app);

        let mut primitives = app.world_mut().query_filtered::<Entity, With<Mesh3d>>();
        let meshes: Vec<_> = primitives.iter(app.world()).collect();
        assert_eq!(meshes.len(), 1, "the converted model has one primitive");
        for entity in [reference, meshes[0]] {
            assert_eq!(
                app.world().entity(entity).get::<RenderLayers>(),
                Some(&PLACED_OBJECT_RENDER_LAYERS),
                "the mesh below a reference must carry the placed-object layer"
            );
        }
    }

    /// A committed cell times its real work: the terrain mesh and collider build used to be
    /// recorded against an `Instant` made on the line before, so it always read zero. The
    /// reference loop has a span of its own, and the cell reports what it carried and loaded.
    #[test]
    fn spawn_cell_times_the_terrain_build_and_the_reference_loop() {
        let mut app = model_app();
        add_placed_object_layer_propagation(&mut app);
        app.init_resource::<AssetCatalog>()
            .init_asset::<TerrainMaterial>()
            .init_asset::<WaterMaterial>()
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(WaterReflectionTexture(Handle::default()))
            .add_systems(Update, spawn_test_cell);
        app.update();

        let profiler = app.world().resource::<ProfilingState>();
        let terrain = profiler.span_samples("streaming/terrain_mesh");
        assert_eq!(terrain.len(), 1, "one terrain build for the one cell");
        assert!(terrain[0] > 0.0, "the terrain build took measurable time");
        assert_eq!(profiler.span_samples("streaming/spawn_references").len(), 1);
        assert_eq!(profiler.span_samples("streaming/spawn_cell").len(), 1);
        assert!(
            profiler.has_event_stage("spawned references=1 model_loads=1"),
            "the cell reports its reference and model-load counts"
        );
    }

    /// Spawns the cell the two layer tests above read: a textureless LAND with water, and one
    /// reference with a model path, through the same [`spawn_cell`] a streamed cell goes through.
    #[allow(clippy::too_many_arguments)]
    fn spawn_test_cell(
        mut commands: Commands,
        mut spawned: Local<bool>,
        asset_server: Res<AssetServer>,
        catalog: Res<AssetCatalog>,
        reflection: Res<WaterReflectionTexture>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
        mut water_materials: ResMut<Assets<WaterMaterial>>,
        mut profiler: ResMut<ProfilingState>,
    ) {
        if *spawned {
            return;
        }
        *spawned = true;
        let mut terrain = terrain_fixture(0x02D4E0, 0.0);
        terrain.layers.clear();
        terrain.water_height = Some(12.0);
        spawn_cell(
            &mut commands,
            &asset_server,
            &catalog,
            &reflection,
            &mut meshes,
            &mut terrain_materials,
            &mut water_materials,
            IVec2::ZERO,
            false,
            false,
            false,
            CellPayload {
                generation: 1,
                key: CellKey::Exterior {
                    worldspace_id: 60,
                    grid_x: 0,
                    grid_y: 0,
                },
                cell_id: 0x02D4E0,
                references: vec![ReferenceRow {
                    form_id: 0x00F9907,
                    initially_enabled: true,
                    cell_id: 0x02D4E0,
                    base_form_id: 0x00EF957,
                    base_record_type: None,
                    model_path: Some("meshes\\furniture\\creatureexit\\wispambush.nif".to_owned()),
                    position: [0.0, 0.0, 0.0],
                    rotation: [0.0; 3],
                    scale: 1.0,
                    bounds_min: [0.0; 3],
                    bounds_max: [0.0; 3],
                    bounds_valid: false,
                    light: None,
                    light_radius_override: None,
                    door: None,
                    emittance: None,
                }],
            },
            Some(terrain),
            None,
            &mut 0,
            &mut profiler,
        )
        .expect("spawn cell");
    }
    /// The spawn-batch measurement reads the world rather than the spawner's bookkeeping: three
    /// models whose assets are already loaded are reported as three instances on the frame Bevy
    /// writes them into the world, and as none on the frame after, while the readiness scan
    /// completes the three of them on its own, later, scan.
    #[test]
    fn the_spawn_batch_counts_the_instances_bevy_instantiated_this_frame() {
        let mut app = model_app();
        // Every model is published before any reference points at it, so all three references are
        // handed to the spawner on the same frame.
        let handles: Vec<_> = (0..3)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        for handle in handles {
            spawn_model_reference(&mut app, handle, None);
        }

        app.update();
        let arrival = streaming_metrics(&app);
        assert_eq!(
            arrival.instances_spawned_this_frame, 3,
            "three references were handed to the spawner with their assets loaded"
        );
        assert_eq!(
            arrival.instances_armed_this_frame, 3,
            "the references joining the spawner's queue are the models armed this frame"
        );
        assert_eq!(
            arrival.instances_completed_this_scan, 0,
            "the scan validates an instance on a later frame than the one it spawns on"
        );

        app.update();
        let quiet = streaming_metrics(&app);
        assert_eq!(
            quiet.instances_spawned_this_frame, 0,
            "an instance spawns once, so the frame after reports none"
        );
        assert_eq!(
            quiet.instances_armed_this_frame, 0,
            "a model arms once, so the frame after reports none"
        );
        assert_eq!(quiet.max_instances_spawned_per_frame, 3);
        assert_eq!(
            quiet.instances_completed_this_scan, 3,
            "the scan completes the three instances it accepts"
        );
        assert_eq!(quiet.max_instances_completed_per_scan, 3);
        assert_eq!(
            quiet.empty_model_references, 3,
            "the empty converted scenes are counted, not failed"
        );
    }

    /// Pacing is what a frame instantiates: a cell's models wait in the arming queue and are handed
    /// over a budget at a time, so the spawn batch is the armed batch and a drained queue stays
    /// drained.
    #[test]
    fn the_arming_budget_spreads_a_cells_models_over_frames() {
        let mut app = model_app();
        app.insert_resource(paced_config(2));
        let handles: Vec<_> = (0..5)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        for (sequence, handle) in handles.into_iter().enumerate() {
            spawn_pending_reference(&mut app, handle, sequence as u64);
        }

        app.update();
        let first = streaming_metrics(&app);
        assert_eq!(
            armed_and_pending(&mut app),
            (2, 3),
            "a budget of two hands two models over and leaves the rest queued"
        );
        assert_eq!(first.arming_queue_depth, 3);
        assert_eq!(
            first.peak_arming_queue_depth, 5,
            "the peak is the cell's whole backlog"
        );
        assert_eq!(
            first.instances_spawned_this_frame, 2,
            "the frame instantiates the models the budget armed, not the whole cell"
        );

        app.update();
        assert_eq!(armed_and_pending(&mut app), (4, 1));
        app.update();
        assert_eq!(armed_and_pending(&mut app), (5, 0));
        assert_eq!(streaming_metrics(&app).arming_queue_depth, 0);

        app.update();
        assert_eq!(
            armed_and_pending(&mut app),
            (5, 0),
            "a model left the queue when it was armed, so none is armed twice"
        );
        assert_eq!(streaming_metrics(&app).instances_spawned_this_frame, 0);
    }

    /// A backlog drains oldest first, whatever order the references were spawned in: the entities
    /// here are created newest-first, so arming in entity order would arm the wrong model.
    #[test]
    fn the_arming_budget_arms_the_oldest_reference_first() {
        let mut app = model_app();
        app.insert_resource(paced_config(1));
        let handles: Vec<_> = (0..4)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        let mut oldest_first = Vec::new();
        for (index, handle) in handles.into_iter().enumerate() {
            // The reference spawned first is the newest model: the sequences run the other way.
            oldest_first.push(spawn_pending_reference(&mut app, handle, 3 - index as u64));
        }
        oldest_first.reverse();

        for (frame, reference) in oldest_first.iter().enumerate() {
            app.update();
            assert!(
                app.world().entity(*reference).contains::<WorldAssetRoot>(),
                "frame {frame} armed a newer model while the oldest one was still waiting"
            );
            assert_eq!(armed_and_pending(&mut app), (frame + 1, 3 - frame));
        }
    }

    fn add_priority_request(
        app: &mut App,
        entity: Entity,
        center: bevy::math::DVec3,
        radius: Option<f64>,
        collision_candidate: bool,
        coarse_terrain: bool,
        cell: Option<CellKey>,
    ) {
        let model = app.world().get::<PendingModel>(entity).unwrap();
        let request = requests::SceneRequest {
            path: "fixture.glb".to_owned(),
            cell,
            coarse_terrain,
            retry_cleanup: None,
            content_identity: None,
            sequence: model.sequence,
            center,
            radius,
            collision_candidate,
            handle: Some(model.handle.clone()),
            started: Instant::now(),
            allow_retry: false,
        };
        app.world_mut().entity_mut(entity).insert(request);
    }

    #[test]
    fn priority_activation_rechecks_turns_and_shares_the_quota_with_coarse_lod() {
        use bevy::math::DVec3;
        let mut app = model_app();
        let handle = add_converted_model(&mut app, empty_converted_scene());
        app.insert_resource(EngineConfig {
            prioritize_streaming: true,
            max_model_spawns_per_frame: 1,
            ..default()
        })
        .init_resource::<requests::SceneSchedulingState>()
        .insert_resource(RenderOrigin(IVec2::new(-4, 7)));
        let camera = app
            .world_mut()
            .spawn((StreamingCamera, Transform::default()))
            .id();
        let origin = DVec3::new(
            -4.0 * f64::from(CELL_SIZE),
            0.0,
            -7.0 * f64::from(CELL_SIZE),
        );
        let coarse = spawn_pending_reference(&mut app, handle.clone(), 0);
        add_priority_request(&mut app, coarse, origin, Some(40000.0), false, true, None);
        let front = spawn_pending_reference(&mut app, handle.clone(), 1);
        add_priority_request(
            &mut app,
            front,
            origin + DVec3::NEG_Z * 1000.0,
            Some(100.0),
            false,
            false,
            None,
        );
        let back = spawn_pending_reference(&mut app, handle.clone(), 2);
        add_priority_request(
            &mut app,
            back,
            origin + DVec3::Z * 1000.0,
            Some(100.0),
            false,
            false,
            None,
        );
        let front_clutter = spawn_pending_reference(&mut app, handle, 3);
        add_priority_request(
            &mut app,
            front_clutter,
            origin + DVec3::NEG_Z * 500.0,
            Some(2.0),
            false,
            false,
            None,
        );

        app.update();
        assert!(app.world().get::<WorldAssetRoot>(front).is_some());
        assert!(app.world().get::<WorldAssetRoot>(coarse).is_none());
        assert_eq!(armed_and_pending(&mut app), (1, 3));
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .rotation = Quat::from_rotation_y(std::f32::consts::PI);
        app.update();
        assert!(app.world().get::<WorldAssetRoot>(back).is_some());
        assert!(app.world().get::<WorldAssetRoot>(front_clutter).is_none());
        assert_eq!(armed_and_pending(&mut app), (2, 2));
        // The eighth actual activation reserves service for the old coarse request.
        app.world_mut()
            .resource_mut::<requests::SceneSchedulingState>()
            .activation_choices = 7;
        app.update();
        assert!(app.world().get::<WorldAssetRoot>(coarse).is_some());
        assert_eq!(armed_and_pending(&mut app), (3, 1));
    }

    #[test]
    fn priority_activation_protects_collision_and_holds_retiring_work_until_revival() {
        use bevy::ecs::system::RunSystemOnce;
        use bevy::math::DVec3;
        let mut app = model_app();
        let handle = add_converted_model(&mut app, empty_converted_scene());
        app.insert_resource(EngineConfig {
            prioritize_streaming: true,
            max_model_spawns_per_frame: 1,
            ..default()
        })
        .init_resource::<requests::SceneSchedulingState>();
        app.world_mut()
            .spawn((StreamingCamera, Transform::default()));
        let cell = app.world_mut().spawn_empty().id();
        let key = exterior_key(-2, -3);
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                key,
                CellStatus::Retiring {
                    root: cell,
                    detail: CellDetail::Full,
                },
            );
        let retired = spawn_pending_reference(&mut app, handle.clone(), 0);
        add_priority_request(
            &mut app,
            retired,
            DVec3::NEG_Z * 20.0,
            Some(10.0),
            true,
            false,
            Some(key),
        );
        let building = spawn_pending_reference(&mut app, handle.clone(), 1);
        add_priority_request(
            &mut app,
            building,
            DVec3::NEG_Z * 1000.0,
            Some(100.0),
            false,
            false,
            None,
        );
        let collision = spawn_pending_reference(&mut app, handle, 2);
        add_priority_request(
            &mut app,
            collision,
            DVec3::Z * 100.0,
            Some(10.0),
            true,
            false,
            None,
        );

        app.world_mut().run_system_once(arm_pending_models).unwrap();
        assert!(app.world().get::<WorldAssetRoot>(collision).is_some());
        assert!(app.world().get::<WorldAssetRoot>(building).is_none());
        assert!(app.world().get::<WorldAssetRoot>(retired).is_none());
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                key,
                CellStatus::Resident {
                    root: cell,
                    detail: CellDetail::Full,
                },
            );
        app.world_mut().run_system_once(arm_pending_models).unwrap();
        assert!(app.world().get::<WorldAssetRoot>(retired).is_some());
        assert!(app.world().get::<WorldAssetRoot>(building).is_none());
    }

    /// A budget is spent on the models that can spawn now: a model whose converted scene has not
    /// arrived keeps its place in the queue without holding up the models behind it, and takes its
    /// turn as soon as it can.
    #[test]
    fn a_model_still_loading_does_not_use_a_budget_slot() {
        let mut app = model_app();
        app.insert_resource(paced_config(2));
        // A handle `Assets<WorldAsset>` has no entry for is the state a loading model is in: the
        // loader has not published the scene yet. The default handle is exactly that - identity
        // without storage - and this one gets its scene before the test ends.
        let loading = Handle::<WorldAsset>::default();
        let oldest = spawn_pending_reference(&mut app, loading.clone(), 0);
        let handles: Vec<_> = (0..4)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        for (sequence, handle) in handles.into_iter().enumerate() {
            spawn_pending_reference(&mut app, handle, sequence as u64 + 1);
        }

        app.update();
        assert!(
            !app.world().entity(oldest).contains::<WorldAssetRoot>(),
            "a model whose scene has not arrived is not armed"
        );
        assert_eq!(
            armed_and_pending(&mut app),
            (2, 3),
            "the budget went to the two models that are ready, not to the loading one"
        );
        assert_eq!(streaming_metrics(&app).arming_queue_depth, 3);

        app.world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .insert(&loading, WorldAsset::new(empty_converted_scene()))
            .expect("publishing the model's scene cannot fail");
        app.update();
        assert!(
            app.world().entity(oldest).contains::<WorldAssetRoot>(),
            "the loading model is armed once its scene arrives"
        );
        assert_eq!(armed_and_pending(&mut app), (4, 1));
    }

    /// A model whose cell is unloaded before its turn needs no cleanup, and the pacer cannot trip
    /// over it: the queue is the component set, and the despawn takes it with the subtree.
    #[test]
    fn a_pending_model_whose_cell_is_despawned_leaves_nothing_queued() {
        let mut app = model_app();
        app.insert_resource(paced_config(1));
        let handle = add_converted_model(&mut app, empty_converted_scene());
        let cell = app
            .world_mut()
            .spawn((
                Name::new("Cell"),
                StreamedCellRoot,
                ExteriorCellGrid(IVec2::ZERO),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        for sequence in 0..2 {
            let reference = spawn_pending_reference(&mut app, handle.clone(), sequence);
            app.world_mut().entity_mut(reference).insert(ChildOf(cell));
        }
        // The cell is outside the unload radius and its turn in the unload budget has come.
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .cells
            .insert(
                exterior_key(4, 4),
                CellStatus::Retiring {
                    root: cell,
                    detail: CellDetail::Full,
                },
            );

        app.update();

        assert_eq!(
            armed_and_pending(&mut app),
            (0, 0),
            "the cell took its pending models with it; none was armed and none is queued"
        );
        let metrics = streaming_metrics(&app);
        assert_eq!(metrics.arming_queue_depth, 0);
        assert_eq!(metrics.unloaded_cells, 1, "the cell itself was unloaded");
        assert_eq!(metrics.instances_spawned_this_frame, 0);
    }

    /// `0` is the unbudgeted behaviour: every model whose scene is ready is handed over at once.
    #[test]
    fn an_arming_budget_of_zero_arms_every_ready_model() {
        let mut app = model_app();
        app.insert_resource(paced_config(0));
        let handles: Vec<_> = (0..5)
            .map(|_| add_converted_model(&mut app, empty_converted_scene()))
            .collect();
        for (sequence, handle) in handles.into_iter().enumerate() {
            spawn_pending_reference(&mut app, handle, sequence as u64);
        }

        app.update();
        assert_eq!(armed_and_pending(&mut app), (5, 0));
        assert_eq!(
            streaming_metrics(&app).instances_spawned_this_frame,
            5,
            "without a budget the whole batch lands on one frame, as it did before the pacer"
        );
    }

    /// Pacing moves the frame a model is spawned on, not the totals: every queued model is armed
    /// once, spawned once, and validated exactly once, and no frame instantiates more than the
    /// budget.
    #[test]
    fn every_paced_model_is_spawned_and_validated_exactly_once() {
        let mut app = model_app();
        app.insert_resource(paced_config(2));
        let mut handles = Vec::new();
        for _ in 0..5 {
            let mesh = app
                .world_mut()
                .resource_mut::<Assets<Mesh>>()
                .add(Cuboid::new(2.0, 4.0, 6.0));
            let material = app
                .world_mut()
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial::default());
            handles.push(add_converted_model(
                &mut app,
                converted_scene_with_material(mesh, material),
            ));
        }
        for (sequence, handle) in handles.into_iter().enumerate() {
            let reference = spawn_pending_reference(&mut app, handle, sequence as u64);
            app.world_mut()
                .entity_mut(reference)
                .insert(cuboid_bounds());
        }

        let metrics = settle_readiness(&mut app);

        assert_eq!(
            metrics.assets_ready, 5,
            "every instance is validated once: {:?}",
            metrics.asset_failures
        );
        assert_eq!(metrics.meshes_validated, 5);
        assert_eq!(metrics.materials_validated, 5);
        assert_eq!(metrics.transform_instances_validated, 5);
        assert_eq!(metrics.empty_model_references, 0);
        assert_eq!(metrics.peak_arming_queue_depth, 5);
        assert_eq!(
            metrics.max_instances_spawned_per_frame, 2,
            "no frame instantiated more than the budget allowed"
        );
        assert_eq!(armed_and_pending(&mut app), (5, 0));
        let mut instances = app
            .world_mut()
            .query_filtered::<Entity, With<WorldInstance>>();
        assert_eq!(
            instances.iter(app.world()).count(),
            5,
            "one instance per reference, paced or not"
        );
    }

    /// What the emptiness rule reads: the converted model's own node and mesh count, taken from
    /// the asset rather than from anything the engine spawned.
    #[test]
    fn reads_a_converted_models_own_node_and_mesh_count() {
        let empty = WorldAsset::new(empty_converted_scene());
        assert!(
            converted_scene_contents(&empty).is_empty(),
            "the converter's empty scene is what an empty model looks like"
        );

        let mesh_model = WorldAsset::new(converted_scene_with_mesh(Handle::default()));
        let contents = converted_scene_contents(&mesh_model);
        assert_eq!(contents.meshes, 1, "one primitive: {contents:?}");
        assert_eq!(contents.nodes, 1, "one node: {contents:?}");
    }

    /// A model with no renderable geometry converts to an **empty scene**, so it has no converted
    /// bounds and nothing to draw. A streamed cell full of such models used to fail the bounds
    /// gate on a model with nothing to draw. Such a reference is skipped and counted on its own,
    /// and nothing fails.
    #[test]
    fn an_empty_scene_model_is_skipped_and_counted_instead_of_failing() {
        let mut app = model_app();
        let handle = add_converted_model(&mut app, empty_converted_scene());
        let reference = spawn_model_reference(&mut app, handle, None);

        let metrics = settle_readiness(&mut app);

        assert_eq!(
            metrics.transform_bounds_validation_failures, 0,
            "an invisible marker is not a conversion failure: {:?}",
            metrics.asset_failures
        );
        assert_eq!(
            metrics.empty_model_references, 1,
            "the empty model is counted on its own"
        );
        assert!(
            metrics.asset_failures.is_empty(),
            "nothing is recorded as a failed asset: {:?}",
            metrics.asset_failures
        );
        assert_eq!(
            metrics.bounds_validated, 0,
            "there were no converted bounds to validate"
        );
        assert_eq!(
            metrics.assets_ready, 0,
            "an empty model is not counted as a ready asset either"
        );
        assert!(
            !app.world()
                .entity(reference)
                .contains::<PendingAssetProfile>(),
            "the reference is no longer pending"
        );
        assert!(
            app.world().entity(reference).contains::<Transform>(),
            "the empty reference keeps its own transform"
        );
        assert!(
            app.world()
                .entity(reference)
                .get::<Children>()
                .is_some_and(|children| !children.is_empty()),
            "the converted scene is spawned below the reference"
        );
    }

    /// The tolerated class is narrow: the model itself must be empty. A converted scene that
    /// declares a node but no mesh - a hierarchy whose geometry an exporter dropped - is not an
    /// empty model, so it keeps failing exactly as it did before the rule existed.
    #[test]
    fn a_model_whose_converted_scene_declares_only_a_node_still_fails() {
        let mut app = model_app();
        let mut scene = World::new();
        let root = scene.spawn(Transform::default()).id();
        scene.spawn((Transform::default(), ChildOf(root)));
        let handle = add_converted_model(&mut app, scene);
        spawn_model_reference(&mut app, handle, None);

        let metrics = settle_readiness(&mut app);

        assert_eq!(
            metrics.empty_model_references, 0,
            "a scene with a node in it is not an empty model"
        );
        assert_eq!(
            metrics.transform_bounds_validation_failures, 1,
            "a model that declares a node and no mesh must still fail: {:?}",
            metrics.asset_failures
        );
        let failure = metrics
            .asset_failures
            .first()
            .expect("the failure is recorded as an asset failure");
        assert!(
            failure
                .dependency_chain
                .iter()
                .any(|reason| reason.contains("is not empty: 0 mesh primitives, 1 nodes")),
            "unexpected failure reason: {:?}",
            failure.dependency_chain
        );
    }

    /// The other side of the same evidence: a model whose converted scene really holds a mesh
    /// primitive, spawned from the asset by the engine's own spawner rather than hand-built, and
    /// which still arrives without converted bounds, is a conversion defect and stays fatal.
    #[test]
    fn a_model_whose_converted_scene_holds_a_mesh_still_fails_without_bounds() {
        let mut app = model_app();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 4.0, 6.0));
        let handle = add_converted_model(&mut app, converted_scene_with_mesh(mesh));
        let reference = spawn_model_reference(&mut app, handle, None);

        let metrics = settle_readiness(&mut app);

        assert_eq!(
            metrics.empty_model_references, 0,
            "a model with geometry is never counted as empty"
        );
        assert_eq!(
            metrics.transform_bounds_validation_failures, 1,
            "a model with geometry and no converted bounds must still fail: {:?}",
            metrics.asset_failures
        );
        let failure = metrics
            .asset_failures
            .first()
            .expect("the failure is recorded as an asset failure");
        assert!(
            failure
                .dependency_chain
                .iter()
                .any(|reason| reason.contains("is not empty: 1 mesh primitives")),
            "unexpected failure reason: {:?}",
            failure.dependency_chain
        );
        // The model's mesh is a descendant of the reference: the engine's own spawner put it
        // there, which is the shape every bounds check in this module reads.
        let mut primitives = app.world_mut().query::<(Entity, &Mesh3d)>();
        let (primitive, _) = primitives
            .iter(app.world())
            .next()
            .expect("the converted model's mesh primitive is spawned");
        let scene_root = app
            .world()
            .entity(reference)
            .get::<Children>()
            .and_then(|children| children.first().copied())
            .expect("the converted scene is spawned below the reference");
        assert_eq!(
            app.world()
                .entity(primitive)
                .get::<ChildOf>()
                .map(ChildOf::parent),
            Some(scene_root),
            "the mesh primitive hangs below the scene root the spawner attached"
        );
    }

    /// The other direction of the same rule: a model the converter *did* bound, whose spawned
    /// scene turns out to be empty, is not an empty model to wave through - the two disagree and
    /// the disagreement is fatal.
    #[test]
    fn a_bounded_model_whose_scene_is_empty_still_fails() {
        let mut app = model_app();
        let handle = add_converted_model(&mut app, empty_converted_scene());
        spawn_model_reference(
            &mut app,
            handle,
            ExpectedModelBounds::new(Vec3::splat(-1.0), Vec3::splat(1.0)),
        );

        let metrics = settle_readiness(&mut app);

        assert_eq!(
            metrics.empty_model_references, 0,
            "only a model without converted bounds may be an empty model"
        );
        assert_eq!(
            metrics.transform_bounds_validation_failures, 1,
            "a bound model whose hierarchy holds no mesh must still fail: {:?}",
            metrics.asset_failures
        );
        let failure = metrics
            .asset_failures
            .first()
            .expect("the failure is recorded as an asset failure");
        assert!(
            failure
                .dependency_chain
                .iter()
                .any(|reason| reason.contains("no bounded mesh")),
            "unexpected failure reason: {:?}",
            failure.dependency_chain
        );
    }

    /// Composes the same basis-rotation / mesh-translation chain as `HumanSkull.glb`'s node
    /// hierarchy in f64, giving a ground-truth model-space (relative-to-root) bounding box
    /// that never touches the root's large world-space position. This stands in for the
    /// converter's own independent, exact recomputation of the model's bounds.
    fn f64_relative_bounds(
        local_min: Vec3,
        local_max: Vec3,
        basis_rotation: Quat,
        mesh_translation: Vec3,
        mesh_scale: Vec3,
    ) -> (Vec3, Vec3) {
        use bevy::math::{DAffine3, DQuat, DVec3};

        let basis = DAffine3::from_quat(DQuat::from_xyzw(
            basis_rotation.x as f64,
            basis_rotation.y as f64,
            basis_rotation.z as f64,
            basis_rotation.w as f64,
        ));
        let mesh = DAffine3::from_scale_rotation_translation(
            DVec3::new(
                mesh_scale.x as f64,
                mesh_scale.y as f64,
                mesh_scale.z as f64,
            ),
            DQuat::IDENTITY,
            DVec3::new(
                mesh_translation.x as f64,
                mesh_translation.y as f64,
                mesh_translation.z as f64,
            ),
        );
        let relative = basis * mesh;
        let mut min = DVec3::splat(f64::INFINITY);
        let mut max = DVec3::splat(f64::NEG_INFINITY);
        for x in [local_min.x, local_max.x] {
            for y in [local_min.y, local_max.y] {
                for z in [local_min.z, local_max.z] {
                    let point = relative.transform_point3(DVec3::new(x as f64, y as f64, z as f64));
                    min = min.min(point);
                    max = max.max(point);
                }
            }
        }
        (
            Vec3::new(min.x as f32, min.y as f32, min.z as f32),
            Vec3::new(max.x as f32, max.y as f32, max.z as f32),
        )
    }

    // Composing each mesh's world transform and then multiplying by the root's
    // inverse world transform (the old algorithm) loses precision at real-data world
    // placements. This fixture mirrors the failing acceptance run: reference 000F6031's
    // world position/rotation, and HumanSkull.glb's node hierarchy (a -90-degree X basis
    // node with a mesh child of scale ~1.14 and translation (0, -7.7, -150.7)).
    #[test]
    fn spawned_bounds_validate_within_tolerance_despite_large_world_placement() {
        use bevy::ecs::system::SystemState;

        let root_translation = Vec3::new(20790.89, -69970.35, 10984.74);
        let root_rotation = Quat::from_euler(EulerRot::XYZ, 2.0587, 0.6207, 1.3418);
        let basis_rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let mesh_translation = Vec3::new(0.0, -7.7, -150.7);
        let mesh_scale = Vec3::splat(1.14);
        let local_min = Vec3::splat(-6.0);
        let local_max = Vec3::splat(6.0);

        let (expected_min, expected_max) = f64_relative_bounds(
            local_min,
            local_max,
            basis_rotation,
            mesh_translation,
            mesh_scale,
        );
        let expected_bounds = ExpectedModelBounds::new(expected_min, expected_max)
            .expect("fixture bounds must be finite and non-degenerate");
        let extent = (expected_max - expected_min).abs().max_element().max(1.0);
        let tolerance = (extent * 1.0e-4).max(1.0e-3);

        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        let corners: Vec<[f32; 3]> = [local_min.x, local_max.x]
            .into_iter()
            .flat_map(|x| {
                [local_min.y, local_max.y]
                    .into_iter()
                    .flat_map(move |y| [local_min.z, local_max.z].map(move |z| [x, y, z]))
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, corners);
        let mesh_handle = meshes.add(mesh);
        world.insert_resource(meshes);

        let root_local = Transform {
            translation: root_translation,
            rotation: root_rotation,
            ..Default::default()
        };
        let root_global = GlobalTransform::from(root_local);
        let root = world.spawn((root_local, root_global)).id();

        let basis_local = Transform {
            rotation: basis_rotation,
            ..Default::default()
        };
        let basis_global = root_global.mul_transform(basis_local);
        let basis = world.spawn((basis_local, basis_global, ChildOf(root))).id();

        let mesh_local = Transform {
            translation: mesh_translation,
            scale: mesh_scale,
            ..Default::default()
        };
        let mesh_global = basis_global.mul_transform(mesh_local);
        world.spawn((mesh_local, mesh_global, ChildOf(basis), Mesh3d(mesh_handle)));

        #[allow(clippy::type_complexity)]
        let mut system_state: SystemState<(
            Query<&Children>,
            SpawnedNodeQuery,
            RenderPrimitiveQuery,
        )> = SystemState::new(&mut world);
        let (children, transforms, primitives) = system_state.get(&world).unwrap();
        let meshes = world.resource::<Assets<Mesh>>();

        // The old algorithm: compose each descendant's absolute GlobalTransform (which
        // bakes in the root's huge world position), then cancel that position back out by
        // multiplying by the root's inverted absolute GlobalTransform.
        let root_inverse = root_global.affine().inverse();
        let mut old_min = Vec3::splat(f32::INFINITY);
        let mut old_max = Vec3::splat(f32::NEG_INFINITY);
        for descendant in children.iter_descendants(root) {
            let Ok((mesh_handle, _, _, _, _)) = primitives.get(descendant) else {
                continue;
            };
            let mesh = meshes.get(mesh_handle).unwrap();
            let aabb = mesh.compute_aabb().unwrap();
            let center = Vec3::from(aabb.center);
            let half_extents = Vec3::from(aabb.half_extents);
            let (_, global, _) = transforms.get(descendant).unwrap();
            let relative = Mat4::from(root_inverse * global.affine());
            let transformed =
                InstanceBounds::transformed(center - half_extents, center + half_extents, relative);
            old_min = old_min.min(transformed.min);
            old_max = old_max.max(transformed.max);
        }
        let old_error = (old_min - expected_min)
            .abs()
            .max((old_max - expected_max).abs())
            .max_element();
        assert!(
            old_error > tolerance,
            "expected the old world-transform-and-invert composition to exceed tolerance \
             {tolerance} (it should reproduce the acceptance failure), got error \
             {old_error}"
        );

        // The fix, exercised through the real validation function: composing local
        // transforms along the path from the root never forms the large-magnitude matrix,
        // so it validates within tolerance.
        let world_transform = WorldTransform(root_local.to_matrix());
        let summary = validate_spawned_transforms_and_bounds(
            root,
            &root_local,
            &root_global,
            &world_transform,
            Some(&expected_bounds),
            None,
            &children,
            &transforms,
            &primitives,
            meshes,
        )
        .unwrap_or_else(|error| {
            panic!(
                "expected local-transform composition to validate within tolerance {tolerance}: \
                 {error}"
            )
        });
        assert_eq!(summary.nodes, 2);

        // Measure the new method's own error directly (mirroring what
        // `validate_spawned_transforms_and_bounds` computes internally) to report it
        // alongside the old method's.
        let mut new_min = Vec3::splat(f32::INFINITY);
        let mut new_max = Vec3::splat(f32::NEG_INFINITY);
        let mut new_nodes = 0usize;
        let mut new_bounded_meshes = 0usize;
        if let Ok(direct_children) = children.get(root) {
            for child in direct_children.iter() {
                accumulate_relative_bounds(
                    child,
                    Affine3A::IDENTITY,
                    &children,
                    &transforms,
                    &primitives,
                    meshes,
                    &mut new_nodes,
                    &mut new_bounded_meshes,
                    &mut new_min,
                    &mut new_max,
                )
                .unwrap();
            }
        }
        let new_error = (new_min - expected_min)
            .abs()
            .max((new_max - expected_max).abs())
            .max_element();
        assert!(
            new_error <= tolerance,
            "new method exceeded tolerance {tolerance}: error {new_error}"
        );
        eprintln!(
            "bounds precision fixture: tolerance={tolerance}, old_error={old_error}, new_error={new_error}"
        );
    }

    #[test]
    fn a_deeply_nested_model_is_bounded_without_overflowing_the_stack() {
        use bevy::ecs::system::SystemState;

        // 50,000 nested nodes with one unit cube at the leaf: a recursive walk would
        // overflow a test thread's stack long before the leaf.
        const DEPTH: usize = 50_000;
        let mut world = World::new();
        let mut meshes = Assets::<Mesh>::default();
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![[-0.5, -0.5, -0.5], [0.5, 0.5, 0.5], [0.5, -0.5, 0.5]],
        );
        let mesh_handle = meshes.add(mesh);
        world.insert_resource(meshes);

        let root = world
            .spawn((Transform::default(), GlobalTransform::default()))
            .id();
        let mut parent = root;
        for _ in 0..DEPTH {
            parent = world
                .spawn((
                    Transform::default(),
                    GlobalTransform::default(),
                    ChildOf(parent),
                ))
                .id();
        }
        world.spawn((
            Transform::default(),
            GlobalTransform::default(),
            ChildOf(parent),
            Mesh3d(mesh_handle),
        ));

        #[allow(clippy::type_complexity)]
        let mut system_state: SystemState<(
            Query<&Children>,
            SpawnedNodeQuery,
            RenderPrimitiveQuery,
        )> = SystemState::new(&mut world);
        let (children, transforms, primitives) = system_state.get(&world).unwrap();
        let meshes = world.resource::<Assets<Mesh>>();

        let mut nodes = 0usize;
        let mut bounded_meshes = 0usize;
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        accumulate_relative_bounds(
            root,
            Affine3A::IDENTITY,
            &children,
            &transforms,
            &primitives,
            meshes,
            &mut nodes,
            &mut bounded_meshes,
            &mut min,
            &mut max,
        )
        .unwrap();
        assert_eq!(nodes, DEPTH + 2);
        assert_eq!(bounded_meshes, 1);
        assert_eq!((min, max), (Vec3::splat(-0.5), Vec3::splat(0.5)));
    }

    use crate::world::database::LightRow;

    fn light_row(radius: f32, color: [u8; 3], flags: u32) -> LightRow {
        LightRow {
            radius,
            color,
            flags,
        }
    }

    /// A reference in interior cell 99 with a light row and, optionally, an `XRDS` radius of its
    /// own, as the database hands one to `spawn_cell`. The model path is empty: the light does not
    /// depend on the reference having a model - that is the case it has to work for.
    fn lit_reference(
        form_id: u32,
        light: Option<LightRow>,
        radius_override: Option<f32>,
    ) -> ReferenceRow {
        ReferenceRow {
            form_id,
            initially_enabled: true,
            cell_id: 99,
            base_form_id: 0x200 + form_id,
            base_record_type: None,
            model_path: None,
            position: [100.0, 50.0, -200.0],
            rotation: [0.0; 3],
            scale: 1.0,
            bounds_min: [0.0; 3],
            bounds_max: [0.0; 3],
            bounds_valid: false,
            light,
            light_radius_override: radius_override,
            door: None,
            emittance: None,
        }
    }

    #[derive(Resource, Default)]
    struct QueuedReferences(Vec<ReferenceRow>);

    /// The cell root the last [`spawn_queued_references`] produced.
    #[derive(Resource, Default)]
    struct SpawnedCellRoot(Option<Entity>);

    #[allow(clippy::too_many_arguments)]
    fn spawn_queued_references(
        mut commands: Commands,
        config: Res<EngineConfig>,
        queued: Res<QueuedReferences>,
        mut root: ResMut<SpawnedCellRoot>,
        asset_server: Res<AssetServer>,
        catalog: Res<AssetCatalog>,
        reflection: Res<WaterReflectionTexture>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut terrain_materials: ResMut<Assets<TerrainMaterial>>,
        mut water_materials: ResMut<Assets<WaterMaterial>>,
        mut profiler: ResMut<ProfilingState>,
    ) {
        root.0 = Some(
            spawn_cell(
                &mut commands,
                &asset_server,
                &catalog,
                &reflection,
                &mut meshes,
                &mut terrain_materials,
                &mut water_materials,
                IVec2::ZERO,
                config.lights,
                false,
                false,
                CellPayload {
                    generation: 1,
                    key: CellKey::Interior(99),
                    cell_id: 99,
                    references: queued.0.clone(),
                },
                None,
                None,
                &mut 0,
                &mut profiler,
            )
            .expect("spawn cell"),
        );
    }

    /// An empty catalogue database: the three tables [`AssetCatalog::open`] reads, with no rows.
    fn write_empty_catalogue(path: &std::path::Path) {
        let connection = rusqlite::Connection::open(path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,diffuse_path TEXT);
                 CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,texture_set_id INTEGER);
                 CREATE TABLE waters(id INTEGER PRIMARY KEY,flow_normal_path TEXT);",
            )
            .unwrap();
    }

    /// An app that spawns one interior cell holding `references` through the real [`spawn_cell`],
    /// with the engine's lights flag set as `lights`.
    fn spawn_reference_cell_app(references: Vec<ReferenceRow>, lights: bool) -> App {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("catalogue.db");
        write_empty_catalogue(&path);
        let config = EngineConfig {
            lights,
            ..default()
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .init_asset::<Image>()
            .init_asset::<TerrainMaterial>()
            .init_asset::<WaterMaterial>()
            .insert_resource(config)
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<ActiveSpace>()
            .insert_resource(AssetCatalog::open(&path).unwrap())
            .insert_resource(WaterReflectionTexture(Handle::default()))
            .insert_resource(QueuedReferences(references))
            .init_resource::<SpawnedCellRoot>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, spawn_queued_references);
        app.update();
        app
    }

    // ---- Dynamic clutter from authored physics data (#104 phase a) ----

    fn clutter_body(kind: BodyKind, shapes: Vec<u32>, convex: bool) -> CollisionBody {
        CollisionBody {
            node: 1,
            target: "Clutter01".to_owned(),
            shapes,
            kind,
            havok: shared::collision::HavokBodyInfo {
                motion_system: 3,
                quality_type: 4,
                deactivator_type: 1,
                collision_layer: 4,
            },
            mass: 2.0,
            inertia: [30.0, 0.0, 0.0, 0.0, 40.0, 0.0, 0.0, 0.0, 50.0],
            center_of_mass: [0.0, 1.0, 0.0],
            linear_damping: 0.1,
            angular_damping: 0.05,
            friction: 0.6,
            restitution: 0.3,
            max_linear_velocity: 7_000.0,
            max_angular_velocity: 31.0,
            convex,
        }
    }

    fn clutter_box_asset(version: u32, kind: BodyKind) -> CollisionAsset {
        CollisionAsset {
            version,
            authored: true,
            shapes: vec![CollisionShape::Box {
                center: [0.0, 10.0, 0.0],
                half_extents: [10.0, 10.0, 10.0],
            }],
            skipped: Vec::new(),
            bodies: if version >= 2 {
                vec![clutter_body(kind, vec![0], true)]
            } else {
                Vec::new()
            },
        }
    }

    fn clutter_spec(asset: &CollisionAsset) -> ClutterSpec {
        match clutter_decision_from_asset(asset) {
            StaticCollisionDecision::Clutter(spec) => *spec,
            _ => panic!("expected a dynamic clutter body"),
        }
    }

    /// Readies one reference of `record_type` whose model carries `asset` in its scene extras.
    fn ready_clutter_reference_with(
        app: &mut App,
        record_type: &str,
        static_physics: bool,
        asset: &CollisionAsset,
    ) -> (Entity, StreamingMetrics) {
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 2.0, 2.0));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let mut scene = World::new();
        let scene_root = scene.spawn(Transform::default()).id();
        scene.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            ChildOf(scene_root),
        ));
        let handle = add_converted_model(app, scene);
        let bounds = ExpectedModelBounds::new(Vec3::splat(-1.0), Vec3::splat(1.0)).unwrap();
        let reference = spawn_model_reference(app, handle, Some(bounds));
        {
            let mut entity_mut = app.world_mut().entity_mut(reference);
            let mut pending = entity_mut.get_mut::<PendingAssetProfile>().unwrap();
            pending.path = "meshes/clutter/test/clutter01.glb".to_owned();
            pending.base_record_type = Some(record_type.to_owned());
            pending.static_physics = static_physics;
        }
        // The extras node carries a transform like every node of a converted scene: the
        // readiness scan validates that every node below the reference has one, and would
        // otherwise reject the whole model before the collision decision.
        app.world_mut().spawn((
            GltfSceneExtras {
                value: serde_json::json!({ "mudcrabCollision": asset }).to_string(),
            },
            Transform::default(),
            ChildOf(reference),
        ));
        let metrics = settle_readiness(app);
        (reference, metrics)
    }

    fn ready_clutter_reference(
        record_type: &str,
        static_physics: bool,
        asset: &CollisionAsset,
    ) -> (App, Entity, StreamingMetrics) {
        let mut app = model_app();
        let (reference, metrics) =
            ready_clutter_reference_with(&mut app, record_type, static_physics, asset);
        (app, reference, metrics)
    }

    /// The collider a body owns: the body entity itself for a single centred shape, otherwise its
    /// collider children (the shape's authored centre becomes a child translation).
    fn body_collider(app: &mut App, body: Entity) -> Option<Entity> {
        let world = app.world_mut();
        let mut query = world.query_filtered::<(Entity, Option<&ChildOf>), With<Collider>>();
        query
            .iter(world)
            .find(|(entity, parent)| {
                *entity == body || parent.is_some_and(|parent| parent.parent() == body)
            })
            .map(|(entity, _)| entity)
    }

    #[test]
    fn only_dynamic_bodies_on_movable_records_become_dynamic() {
        let dynamic = clutter_box_asset(2, BodyKind::Dynamic);
        let (mut app, misc, metrics) = ready_clutter_reference("MISC", true, &dynamic);
        assert_eq!(
            app.world().get::<RigidBody>(misc),
            Some(&RigidBody::Dynamic)
        );
        assert!(
            body_collider(&mut app, misc).is_some(),
            "the dynamic body has a collider"
        );
        assert!(
            app.world()
                .get::<crate::physics::DynamicClutter>(misc)
                .is_some()
        );
        assert!(app.world().get::<StaticColliderSource>(misc).is_none());
        assert_eq!(metrics.dynamic_clutter_bodies, 1);
        assert_eq!(metrics.dynamic_clutter_convexified, 0);
        assert_eq!(metrics.dynamic_clutter_skipped, 0);
        assert_eq!(metrics.dynamic_clutter_mass_fallbacks, 0);
        // The stable id is the FormId the reference already carries.
        assert_eq!(app.world().get::<FormId>(misc), Some(&FormId(0x00F9907)));

        // A fixed body on a movable record gets no body and no collider (as before).
        let fixed = clutter_box_asset(2, BodyKind::Fixed);
        let (app, misc, metrics) = ready_clutter_reference("MISC", true, &fixed);
        assert!(app.world().get::<RigidBody>(misc).is_none());
        assert!(app.world().get::<Collider>(misc).is_none());
        assert_eq!(metrics.dynamic_clutter_bodies, 0);

        // A dynamic body on a fixed record type stays fixed, with today's authored collider.
        let (app, stat, metrics) = ready_clutter_reference("STAT", true, &dynamic);
        assert_eq!(app.world().get::<RigidBody>(stat), Some(&RigidBody::Fixed));
        assert_eq!(
            app.world().get::<StaticColliderSource>(stat),
            Some(&StaticColliderSource::NifAuthored)
        );
        assert!(
            app.world()
                .get::<crate::physics::DynamicClutter>(stat)
                .is_none()
        );
        assert_eq!(metrics.dynamic_clutter_bodies, 0);

        // Containers (barrels, chests) and the other deferred record types stay fixed for now.
        for record in ["CONT", "MSTT", "ACTI", "FLOR", "DOOR"] {
            let (app, entity, metrics) = ready_clutter_reference(record, true, &dynamic);
            assert!(
                app.world().get::<RigidBody>(entity).is_none(),
                "{record} became a body"
            );
            assert_eq!(metrics.dynamic_clutter_bodies, 0, "{record}");
        }
    }

    fn parse_collision_extras(value: &serde_json::Value) -> CollisionAsset {
        let mut value = value.clone();
        drop_malformed_bodies(&mut value);
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_malformed_body_is_dropped_without_losing_the_models_shapes() {
        let mut value = serde_json::to_value(clutter_box_asset(2, BodyKind::Dynamic)).unwrap();
        // A non-finite mass is written as null, and an unknown kind is not a BodyKind.
        let mut bad_mass = value["bodies"][0].clone();
        bad_mass["mass"] = serde_json::Value::Null;
        let mut bad_kind = value["bodies"][0].clone();
        bad_kind["kind"] = serde_json::json!("sideways");
        value["bodies"] = serde_json::json!([bad_mass, bad_kind, value["bodies"][0].clone()]);
        let asset = parse_collision_extras(&value);
        assert_eq!(asset.shapes.len(), 1);
        assert_eq!(asset.bodies.len(), 1);
        assert_eq!(asset.skipped.len(), 2);
        assert!(asset.skipped[0].contains("rigid body 0"));
        value["bodies"] = serde_json::json!([bad_mass]);
        let asset = parse_collision_extras(&value);
        assert_eq!(asset.shapes.len(), 1);
        assert!(asset.bodies.is_empty());
    }

    #[test]
    fn extra_dynamic_bodies_are_counted_not_silently_dropped() {
        let mut asset = clutter_box_asset(2, BodyKind::Dynamic);
        asset
            .bodies
            .push(clutter_body(BodyKind::Dynamic, vec![0], true));
        asset
            .bodies
            .push(clutter_body(BodyKind::Fixed, vec![0], true));
        assert_eq!(clutter_spec(&asset).ignored_bodies, 1);
        let single = clutter_box_asset(2, BodyKind::Dynamic);
        assert_eq!(clutter_spec(&single).ignored_bodies, 0);
    }

    #[test]
    fn version_one_asset_gives_no_dynamic_bodies() {
        let legacy = clutter_box_asset(1, BodyKind::Dynamic);
        let (app, misc, metrics) = ready_clutter_reference("MISC", true, &legacy);
        assert!(app.world().get::<RigidBody>(misc).is_none());
        assert_eq!(metrics.dynamic_clutter_bodies, 0);
        assert_eq!(metrics.dynamic_clutter_skipped, 0);
    }

    /// What today's converter writes for a model with no authored dynamic body: version 2 with an
    /// empty `bodies` list. It must behave like a version 1 asset.
    #[test]
    fn version_two_asset_with_no_bodies_gives_no_dynamic_bodies() {
        let mut empty = clutter_box_asset(2, BodyKind::Dynamic);
        empty.bodies.clear();
        assert!(matches!(
            clutter_decision_from_asset(&empty),
            StaticCollisionDecision::ClutterNone
        ));
        let (app, misc, metrics) = ready_clutter_reference("MISC", true, &empty);
        assert!(app.world().get::<RigidBody>(misc).is_none());
        assert_eq!(metrics.dynamic_clutter_bodies, 0);
        assert_eq!(metrics.dynamic_clutter_skipped, 0);
    }

    #[test]
    fn non_convex_dynamic_body_becomes_one_counted_hull() {
        let asset = CollisionAsset {
            version: 2,
            authored: true,
            shapes: vec![CollisionShape::Mesh {
                vertices: vec![
                    [0.0, 0.0, 0.0],
                    [20.0, 0.0, 0.0],
                    [0.0, 20.0, 0.0],
                    [0.0, 0.0, 20.0],
                    [5.0, 5.0, 5.0],
                ],
                triangles: vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]],
            }],
            skipped: Vec::new(),
            bodies: vec![clutter_body(BodyKind::Dynamic, vec![0], false)],
        };
        let (app, misc, metrics) = ready_clutter_reference("MISC", true, &asset);
        assert_eq!(metrics.dynamic_clutter_bodies, 1);
        assert_eq!(metrics.dynamic_clutter_convexified, 1);
        let collider = app.world().get::<Collider>(misc).expect("hull collider");
        assert!(collider.as_convex_polyhedron().is_some());
        assert_eq!(
            app.world().get::<RigidBody>(misc),
            Some(&RigidBody::Dynamic)
        );
    }

    #[test]
    fn dynamic_body_without_a_hull_is_skipped_never_fixed() {
        // Collinear points have no 3D hull.
        let asset = CollisionAsset {
            version: 2,
            authored: true,
            shapes: vec![CollisionShape::Hull {
                points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            }],
            skipped: Vec::new(),
            bodies: vec![clutter_body(BodyKind::Dynamic, vec![0], false)],
        };
        let (app, misc, metrics) = ready_clutter_reference("MISC", true, &asset);
        assert_eq!(metrics.dynamic_clutter_skipped, 1);
        assert_eq!(metrics.dynamic_clutter_bodies, 0);
        assert!(app.world().get::<RigidBody>(misc).is_none());
        assert!(app.world().get::<Collider>(misc).is_none());
    }

    /// A pyramid mesh: five points with a 3D hull, and a body listing it is always convexified.
    fn pyramid_mesh() -> CollisionShape {
        CollisionShape::Mesh {
            vertices: vec![
                [0.0, 0.0, 0.0],
                [20.0, 0.0, 0.0],
                [0.0, 20.0, 0.0],
                [0.0, 0.0, 20.0],
                [5.0, 5.0, 5.0],
            ],
            triangles: vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]],
        }
    }

    /// The point limit is injectable, so these tests reach it without millions of points.
    fn clutter_decision_within(asset: &CollisionAsset, budget: usize) -> StaticCollisionDecision {
        clutter_decision_from_asset_within(asset, budget)
    }

    fn skip_reason(decision: StaticCollisionDecision) -> String {
        match decision {
            StaticCollisionDecision::ClutterSkipped(reason) => reason,
            _ => panic!("expected a skipped dynamic body"),
        }
    }

    #[test]
    fn repeated_primitive_indices_create_one_collider_and_density_contribution() {
        let mut asset = clutter_box_asset(2, BodyKind::Dynamic);
        asset.bodies[0].shapes = vec![0; 32];
        asset.bodies[0].inertia = [0.0; 9];
        assert_eq!(clutter_spec(&asset).parts.len(), 1);

        let (mut app, reference, metrics) = ready_clutter_reference("MISC", true, &asset);
        assert_eq!(metrics.dynamic_clutter_bodies, 1);
        assert_eq!(metrics.dynamic_clutter_mass_fallbacks, 1);
        let world = app.world_mut();
        let mut colliders = world.query::<(Entity, &ColliderMassProperties, Option<&ChildOf>)>();
        let owned: Vec<_> = colliders
            .iter(world)
            .filter(|(entity, _, parent)| {
                *entity == reference || parent.is_some_and(|parent| parent.parent() == reference)
            })
            .collect();
        assert_eq!(owned.len(), 1);
        assert!(
            matches!(owned[0].1, ColliderMassProperties::Density(density) if *density == 0.001)
        );
    }

    /// A corrupt body may list one in-range index thousands of times. The reviewer's fixture
    /// packs 4_096 occurrences of a 4_096-point hull into a 72 KB file; resolve it only once.
    #[test]
    fn a_body_repeating_one_shape_index_builds_one_budgeted_hull() {
        let asset = CollisionAsset {
            version: 2,
            authored: true,
            shapes: vec![CollisionShape::Hull {
                points: (0..4_096)
                    .map(|index| {
                        let index = index as f32;
                        [index, index % 64.0, index % 17.0]
                    })
                    .collect(),
            }],
            skipped: Vec::new(),
            bodies: vec![clutter_body(BodyKind::Dynamic, vec![0; 4_096], false)],
        };
        assert_eq!(shape_hull_point_count(&asset.shapes[0]), 4_096);
        // The distinct shape still exceeds this smaller budget, before any hull is built.
        let reason = skip_reason(clutter_decision_within(&asset, 1_000));
        assert!(reason.contains("too many hull points"), "{reason}");
        // Repeats would exceed the shipped limit, but the single 4_096-point shape fits exactly.
        let spec = match clutter_decision_within(&asset, 4_096) {
            StaticCollisionDecision::Clutter(spec) => *spec,
            _ => panic!("the distinct geometry fits the budget"),
        };
        assert!(spec.convexified);
        assert_eq!(spec.parts.len(), 1);
    }

    /// Each shape alone is well under the budget; what is counted is the total of all of them.
    #[test]
    fn distinct_shapes_over_the_budget_together_are_rejected() {
        let asset = CollisionAsset {
            version: 2,
            authored: true,
            shapes: vec![
                CollisionShape::Box {
                    center: [0.0, 0.0, 0.0],
                    half_extents: [1.0, 2.0, 3.0],
                },
                CollisionShape::Capsule {
                    a: [0.0, 0.0, 0.0],
                    b: [4.0, 0.0, 0.0],
                    radius: 1.0,
                },
                pyramid_mesh(),
            ],
            skipped: Vec::new(),
            bodies: vec![clutter_body(BodyKind::Dynamic, vec![0, 1, 2], false)],
        };
        assert_eq!(shape_hull_point_count(&asset.shapes[0]), 8);
        assert_eq!(shape_hull_point_count(&asset.shapes[1]), 52);
        assert_eq!(shape_hull_point_count(&asset.shapes[2]), 5);
        // 8 + 52 + 5 = 65, so 64 is one point short and 65 fits exactly.
        let reason = skip_reason(clutter_decision_within(&asset, 64));
        assert!(reason.contains("too many hull points"), "{reason}");
        assert!(matches!(
            clutter_decision_within(&asset, 65),
            StaticCollisionDecision::Clutter(_)
        ));
    }

    /// Repeated convex shape indices also resolve to one collider and one point-budget charge.
    #[test]
    fn a_convex_body_repeating_one_hull_index_builds_one_collider() {
        let asset = CollisionAsset {
            version: 2,
            authored: true,
            shapes: vec![CollisionShape::Hull {
                points: (0..64)
                    .map(|index| {
                        let index = index as f32;
                        [index, index % 8.0, index % 5.0]
                    })
                    .collect(),
            }],
            skipped: Vec::new(),
            bodies: vec![clutter_body(BodyKind::Dynamic, vec![0; 4_096], true)],
        };
        let reason = skip_reason(clutter_decision_within(&asset, 63));
        assert!(reason.contains("too many hull points"), "{reason}");
        let spec = match clutter_decision_within(&asset, 64) {
            StaticCollisionDecision::Clutter(spec) => *spec,
            _ => panic!("the single convex shape fits the budget"),
        };
        assert!(!spec.convexified);
        assert_eq!(spec.parts.len(), 1);
    }

    #[test]
    fn distinct_primitive_parts_keep_first_seen_order_and_have_a_separate_cap() {
        let mut asset = clutter_box_asset(2, BodyKind::Dynamic);
        asset.shapes.push(CollisionShape::Box {
            center: [40.0, 10.0, 0.0],
            half_extents: [10.0; 3],
        });
        asset.bodies[0].shapes = vec![1, 0, 1, 0];
        let spec = clutter_spec(&asset);
        let translations: Vec<_> = spec.parts.iter().map(|part| part.translation).collect();
        assert_eq!(
            translations,
            [Vec3::new(40.0, 10.0, 0.0), Vec3::new(0.0, 10.0, 0.0)]
        );

        asset.shapes = vec![asset.shapes[0].clone(); MAX_CLUTTER_COLLIDER_PARTS + 1];
        asset.bodies[0].shapes = (0..MAX_CLUTTER_COLLIDER_PARTS as u32).collect();
        assert_eq!(clutter_spec(&asset).parts.len(), MAX_CLUTTER_COLLIDER_PARTS);
        asset.bodies[0]
            .shapes
            .push(MAX_CLUTTER_COLLIDER_PARTS as u32);
        let reason = skip_reason(clutter_decision_from_asset(&asset));
        assert!(reason.contains("too many collider parts"), "{reason}");
    }

    /// A body inside the budget is unaffected: it still gets its one convex hull.
    #[test]
    fn a_body_just_under_the_budget_still_produces_a_hull() {
        let mut asset = CollisionAsset {
            version: 2,
            authored: true,
            shapes: vec![pyramid_mesh()],
            skipped: Vec::new(),
            bodies: vec![clutter_body(BodyKind::Dynamic, vec![0], false)],
        };
        // One occurrence costs five points: comfortably inside a budget of ten.
        assert!(matches!(
            clutter_decision_within(&asset, 10),
            StaticCollisionDecision::Clutter(_)
        ));
        // Listed twice it still costs five, which fits exactly.
        asset.bodies[0].shapes = vec![0, 0];
        let spec = match clutter_decision_within(&asset, 5) {
            StaticCollisionDecision::Clutter(spec) => *spec,
            _ => panic!("a body inside the budget must keep its hull"),
        };
        assert!(spec.convexified);
        assert_eq!(spec.parts.len(), 1);
        assert!(spec.parts[0].collider.as_convex_polyhedron().is_some());
        let reason = skip_reason(clutter_decision_within(&asset, 4));
        assert!(reason.contains("too many hull points"), "{reason}");
    }

    /// The count a budget is checked against is the number of points the copy appends, for every
    /// shape kind.
    #[test]
    fn the_counted_hull_points_match_the_appended_points() {
        let shapes = [
            pyramid_mesh(),
            CollisionShape::Hull {
                points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            },
            CollisionShape::Box {
                center: [1.0, 2.0, 3.0],
                half_extents: [1.0, 2.0, 3.0],
            },
            CollisionShape::Capsule {
                a: [0.0, 0.0, 0.0],
                b: [4.0, 0.0, 0.0],
                radius: 1.0,
            },
        ];
        // 5 mesh vertices, 3 hull points, 8 box corners, 26 points at each of two capsule ends.
        let expected = [5, 3, 8, 52];
        for (shape, expected) in shapes.iter().zip(expected) {
            let mut points = Vec::new();
            shape_hull_points(shape, &mut points);
            assert_eq!(points.len(), expected);
            assert_eq!(shape_hull_point_count(shape), points.len());
        }
        // A repeat costs its points again, the same way the copy would.
        let repeated = [&shapes[0], &shapes[0], &shapes[1]];
        assert_eq!(clutter_hull_point_total(&repeated, 13), Some(13));
        assert_eq!(clutter_hull_point_total(&repeated, 12), None);
    }

    #[test]
    fn unusable_inertia_falls_back_to_density_and_is_counted() {
        let mut asset = clutter_box_asset(2, BodyKind::Dynamic);
        asset.bodies[0].inertia = [0.0; 9];
        let (mut app, misc, metrics) = ready_clutter_reference("MISC", true, &asset);
        assert_eq!(metrics.dynamic_clutter_bodies, 1);
        assert_eq!(metrics.dynamic_clutter_mass_fallbacks, 1);
        assert!(app.world().get::<AdditionalMassProperties>(misc).is_none());
        let collider = body_collider(&mut app, misc).expect("the body's collider");
        assert!(matches!(
            app.world().get::<ColliderMassProperties>(collider),
            Some(ColliderMassProperties::Density(_))
        ));
    }

    #[test]
    fn the_live_cap_refuses_bodies_and_counts_the_overflow() {
        let dynamic = clutter_box_asset(2, BodyKind::Dynamic);
        let mut app = model_app();
        for _ in 0..crate::physics::MAX_LIVE_DYNAMIC_CLUTTER {
            app.world_mut().spawn(crate::physics::DynamicClutter {
                max_linear_velocity: 0.0,
                max_angular_velocity: 0.0,
            });
        }
        let (reference, metrics) = ready_clutter_reference_with(&mut app, "MISC", true, &dynamic);
        assert_eq!(metrics.dynamic_clutter_bodies, 0);
        assert_eq!(metrics.dynamic_clutter_overflow, 1);
        assert!(app.world().get::<RigidBody>(reference).is_none());
        assert!(app.world().get::<Collider>(reference).is_none());
    }

    #[test]
    fn non_interactive_runs_spawn_no_dynamic_bodies() {
        // `static_physics` is `EngineConfig::interactive_world_physics()` at spawn time.
        let dynamic = clutter_box_asset(2, BodyKind::Dynamic);
        let (app, misc, metrics) = ready_clutter_reference("MISC", false, &dynamic);
        assert!(app.world().get::<RigidBody>(misc).is_none());
        assert_eq!(metrics.dynamic_clutter_bodies, 0);
        for config in [
            EngineConfig {
                benchmark_only: true,
                ..default()
            },
            EngineConfig {
                acceptance_screenshot: Some(default()),
                ..default()
            },
            EngineConfig {
                headless: true,
                ..default()
            },
        ] {
            assert!(!config.interactive_world_physics());
        }
    }

    #[test]
    fn rebase_keeps_a_moving_clutter_body_pose_and_velocity() {
        let mut app = crate::physics::headless::fixture_app();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<ActiveSpace>()
            .add_systems(Update, update_render_origin);
        let cell = app
            .world_mut()
            .spawn((
                ExteriorCellGrid(IVec2::ZERO),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        let spec = clutter_spec(&clutter_box_asset(2, BodyKind::Dynamic));
        let entity = app
            .world_mut()
            .run_system_once(move |mut commands: Commands| {
                let entity = commands
                    .spawn((
                        Transform::from_translation(Vec3::new(-300.0, 20_000.0, 400.0)),
                        ChildOf(cell),
                    ))
                    .id();
                crate::physics::spawn_dynamic_clutter(
                    &mut commands,
                    entity,
                    &spec.body,
                    spec.parts
                        .iter()
                        .map(|part| (part.translation, &part.collider)),
                    spec.mass,
                    1.0,
                );
                commands.entity(entity).insert(Velocity {
                    linear: Vec3::new(40.0, 0.0, 0.0),
                    angular: Vec3::ZERO,
                });
                entity
            })
            .unwrap();
        for _ in 0..5 {
            app.update();
        }
        let body = |world: &mut World| {
            world
                .run_system_once(move |context: ReadRapierContext| {
                    let context = context.single().expect("rapier context");
                    let handle = context.entity2body()[&entity];
                    let body = context.rigidbody_set.bodies.get(handle).expect("body");
                    (body.translation(), body.linvel())
                })
                .unwrap()
        };
        let (before_pose, before_velocity) = body(app.world_mut());
        let camera = {
            let mut query = app
                .world_mut()
                .query_filtered::<Entity, With<StreamingCamera>>();
            query.single(app.world()).unwrap()
        };
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x += CELL_SIZE;
        app.update();
        let (after_pose, after_velocity) = body(app.world_mut());
        // One 60 Hz step of motion (40 u/s sideways, a gravity-sized fall) separates the samples.
        assert!(
            (after_pose.x - (before_pose.x - CELL_SIZE)).abs() < 2.0,
            "x {before_pose:?} -> {after_pose:?}"
        );
        assert!((after_pose.y - before_pose.y).abs() < 40.0);
        assert!((after_velocity.x - before_velocity.x).abs() < 1.0);
        // Moved once, not twice: the entity's world pose follows the body.
        let global = app.world().get::<GlobalTransform>(entity).unwrap();
        assert!(
            (global.translation().x - after_pose.x).abs() < 2.0,
            "entity {} body {}",
            global.translation().x,
            after_pose.x
        );

        // The body despawns with its cell.
        app.world_mut().entity_mut(cell).despawn();
        app.update();
        let retained = app
            .world_mut()
            .run_system_once(move |context: ReadRapierContext| {
                context
                    .single()
                    .expect("rapier context")
                    .entity2body()
                    .contains_key(&entity)
            })
            .unwrap();
        assert!(!retained, "unloaded cell retained its clutter body");
    }

    /// Every reference the cell spawned, by the form id it was placed with.
    fn reference_entity(app: &App, form_id: u32) -> Entity {
        let root = app
            .world()
            .resource::<SpawnedCellRoot>()
            .0
            .expect("the cell spawned");
        app.world()
            .entity(root)
            .get::<Children>()
            .expect("the cell root has the references as children")
            .iter()
            .find(|child| {
                app.world()
                    .entity(*child)
                    .get::<FormId>()
                    .is_some_and(|id| id.0 == form_id)
            })
            .expect("the reference spawned")
    }

    #[test]
    fn disabled_references_spawn_no_roots_models_lights_or_collider_work() {
        let mut hidden = lit_reference(1, Some(light_row(256.0, [255; 3], 0)), None);
        hidden.initially_enabled = false;
        hidden.model_path = Some("architecture/hidden-wall.nif".to_owned());
        hidden.base_record_type = Some("STAT".to_owned());
        let visible = lit_reference(2, Some(light_row(256.0, [255; 3], 0)), None);
        let mut app = spawn_reference_cell_app(vec![hidden, visible], true);
        let mut ids = app.world_mut().query::<&FormId>();
        assert_eq!(
            ids.iter(app.world()).map(|id| id.0).collect::<Vec<_>>(),
            [2]
        );
        assert_eq!(
            app.world_mut()
                .query::<&PointLight>()
                .iter(app.world())
                .count(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&PendingModel>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world_mut()
                .query::<&PendingAssetProfile>()
                .iter(app.world())
                .count(),
            0
        );
        assert_eq!(
            app.world_mut()
                .query::<&Collider>()
                .iter(app.world())
                .count(),
            0
        );
        let directory = tempfile::tempdir().unwrap();
        let config = EngineConfig {
            profile_output_dir: Some(directory.path().to_owned()),
            ..default()
        };
        let profiler = app.world().resource::<ProfilingState>();
        profiler
            .write_bundle(
                &config,
                &serde_json::json!({}),
                None,
                &default(),
                None,
                None,
            )
            .unwrap();
        let profile: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.path().join("cpu-spans.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(profile["counters"]["streaming/references_spawned"], 1);
        assert_eq!(profile["counters"]["streaming/references_suppressed"], 1);
    }

    #[test]
    fn a_cell_with_only_disabled_references_still_loads() {
        let mut hidden = lit_reference(1, None, None);
        hidden.initially_enabled = false;
        let mut app = spawn_reference_cell_app(vec![hidden], false);
        let root = app.world().resource::<SpawnedCellRoot>().0.unwrap();
        assert_eq!(
            app.world()
                .entity(root)
                .get::<Children>()
                .map_or(0, |children| children.len()),
            0
        );
        assert_eq!(
            app.world_mut().query::<&FormId>().iter(app.world()).count(),
            0
        );
    }

    /// A spawned reference with a resolvable `door_links` row carries `LoadDoor`; one whose link
    /// cannot be resolved, and a plain reference, do not.
    #[test]
    fn a_spawned_door_reference_carries_load_door() {
        let link = |cell| crate::world::database::DoorLinkRow {
            label: String::new(),
            destination_ref_id: 0x700,
            destination_cell_id: cell,
            destination_worldspace_id: None,
            arrival_position: [10.0, 20.0, 30.0],
            arrival_rotation: [0.0, 0.0, 1.0],
        };
        let mut door = lit_reference(0x110, None, None);
        door.door = Some(link(Some(500)));
        let mut unresolved = lit_reference(0x111, None, None);
        unresolved.door = Some(link(None));
        let app = spawn_reference_cell_app(
            vec![door, unresolved, lit_reference(0x112, None, None)],
            false,
        );

        let loaded = app
            .world()
            .entity(reference_entity(&app, 0x110))
            .get::<crate::doors::LoadDoor>()
            .cloned()
            .expect("the door reference carries LoadDoor");
        assert_eq!(loaded.ref_id, 0x110);
        assert_eq!(loaded.destination.destination_ref_id, 0x700);
        assert_eq!(loaded.destination.interior_cell_id, Some(500));
        assert_eq!(loaded.destination.arrival_position, [10.0, 20.0, 30.0]);
        for form_id in [0x111, 0x112] {
            assert!(
                app.world()
                    .entity(reference_entity(&app, form_id))
                    .get::<crate::doors::LoadDoor>()
                    .is_none()
            );
        }
    }

    /// One lit reference, one negative light, one flagged off by default and one plain reference:
    /// exactly one `PointLight` comes out, carrying the reference's own radius and colour.
    #[test]
    fn spawns_one_point_light_for_a_lit_reference() {
        let mut app = spawn_reference_cell_app(
            vec![
                lit_reference(
                    0x100,
                    Some(light_row(256.0, [255, 200, 120], 0)),
                    Some(850.8),
                ),
                lit_reference(0x101, Some(light_row(512.0, [80, 80, 90], 0x4)), None),
                lit_reference(0x102, Some(light_row(512.0, [80, 80, 90], 0x20)), None),
                lit_reference(0x103, None, None),
            ],
            true,
        );

        let lights: Vec<(Entity, PointLight, crate::lights::SkyrimLight)> = {
            let mut query = app
                .world_mut()
                .query::<(Entity, &PointLight, &crate::lights::SkyrimLight)>();
            query
                .iter(app.world())
                .map(|(entity, light, marker)| (entity, *light, *marker))
                .collect()
        };
        assert_eq!(
            lights.len(),
            1,
            "the negative and off-by-default lights spawn none"
        );
        let (light_entity, light, marker) = lights[0];
        assert_eq!(marker.form_id, 0x100);
        assert_eq!(marker.cell_id, 99);
        assert_eq!(
            light.range, 850.8,
            "the reference's XRDS radius wins over the record's 256"
        );
        assert_eq!(light.color, Color::srgb_u8(255, 200, 120));
        assert!(
            (light.intensity - crate::lights::intensity_for_radius(850.8)).abs() < 1.0e3,
            "{}",
            light.intensity
        );
        assert!(!light.shadow_maps_enabled);
        // The light opts out of its reference's propagated placed-object layer; see
        // `render::a_light_below_a_reference_keeps_the_light_layers`.
        assert_eq!(
            app.world().entity(light_entity).get::<RenderLayers>(),
            Some(&RenderLayers::from_layers(
                &crate::render::POINT_LIGHT_LAYERS
            ))
        );
        assert!(
            app.world()
                .entity(light_entity)
                .contains::<bevy::app::PropagateOver<RenderLayers>>()
        );

        // The light has to be inside the cell root: that hierarchy is what a render-origin rebase
        // moves and what a cell unload despawns.
        let root = app
            .world()
            .resource::<SpawnedCellRoot>()
            .0
            .expect("the cell spawned");
        assert!(app.world().entity(root).get::<StreamedCellRoot>().is_some());
        let reference = app
            .world()
            .entity(light_entity)
            .get::<ChildOf>()
            .expect("the light is a child of its reference")
            .parent();
        assert_eq!(
            app.world().entity(reference).get::<FormId>(),
            Some(&FormId(0x100))
        );
        assert!(
            app.world().entity(reference).get::<Visibility>().is_some(),
            "the reference of a light needs `Visibility`, or the light child can never be visible"
        );
        assert_eq!(
            app.world()
                .entity(reference)
                .get::<ChildOf>()
                .expect("the reference is a child of the cell root")
                .parent(),
            root
        );
        // The cell's other references are spawned exactly as before, and the unlit one gets no
        // child at all.
        assert!(
            app.world()
                .entity(reference_entity(&app, 0x103))
                .get::<Children>()
                .is_none(),
            "a reference with no light row spawns no light child"
        );
    }

    /// The flag is the whole switch: the same cell places no light at all without `--lights`, and
    /// the references themselves are spawned exactly as before.
    #[test]
    fn places_no_light_without_the_lights_flag() {
        let mut app = spawn_reference_cell_app(
            vec![lit_reference(
                0x100,
                Some(light_row(256.0, [255, 200, 120], 0)),
                Some(850.8),
            )],
            false,
        );

        let mut query = app
            .world_mut()
            .query_filtered::<Entity, (With<PointLight>, With<crate::lights::SkyrimLight>)>();
        assert_eq!(
            query.iter(app.world()).count(),
            0,
            "a lit reference places nothing while the flag is off"
        );
        let root = app
            .world()
            .resource::<SpawnedCellRoot>()
            .0
            .expect("the cell spawned");
        assert!(app.world().entity(root).get::<StreamedCellRoot>().is_some());
        assert_eq!(
            app.world()
                .entity(reference_entity(&app, 0x100))
                .get::<FormId>(),
            Some(&FormId(0x100)),
            "and the reference is still spawned"
        );
    }

    #[derive(Resource, Default)]
    struct BoundsOutcome {
        expected: Option<ExpectedModelBounds>,
        bounds: Option<Result<SpawnedBounds, String>>,
        check: Option<Result<TransformValidationSummary, String>>,
    }

    fn run_bounds_check(
        mut outcome: ResMut<BoundsOutcome>,
        roots: Query<(
            Entity,
            &Transform,
            &GlobalTransform,
            &WorldTransform,
            Option<&ExpectedModelBounds>,
        )>,
        transforms: SpawnedNodeQuery,
        children: Query<&Children>,
        primitives: RenderPrimitiveQuery,
        meshes: Res<Assets<Mesh>>,
    ) {
        let Ok((root, local, global, world_transform, expected)) = roots.single() else {
            panic!("the fixture must spawn exactly one model root");
        };
        outcome.expected = expected.copied();
        outcome.bounds = Some(spawned_relative_bounds(
            root,
            &children,
            &transforms,
            &primitives,
            &meshes,
        ));
        outcome.check = Some(super::validate_spawned_transforms_and_bounds(
            root,
            local,
            global,
            world_transform,
            expected,
            None,
            &children,
            &transforms,
            &primitives,
            &meshes,
        ));
    }

    fn spawn_bounds_fixture(
        world: &mut World,
        root_local: Transform,
        node_local: Transform,
        mesh_local: Transform,
        mesh_shift: Vec3,
        mesh: Handle<Mesh>,
    ) -> (Entity, ExpectedModelBounds) {
        let (min, max) = {
            let aabb = world
                .resource::<Assets<Mesh>>()
                .get(&mesh)
                .and_then(|mesh| mesh.compute_aabb())
                .expect("the fixture mesh has finite POSITION bounds");
            let center = Vec3::from(aabb.center);
            let half_extents = Vec3::from(aabb.half_extents);
            let converted = InstanceBounds::transformed(
                center - half_extents,
                center + half_extents,
                node_local.to_matrix() * mesh_local.to_matrix(),
            );
            (converted.min, converted.max)
        };
        let root_global = GlobalTransform::from(root_local.to_matrix());
        let root = world
            .spawn((
                root_local,
                root_global,
                WorldTransform(root_local.to_matrix()),
                ExpectedModelBounds { min, max },
            ))
            .id();
        let node_global = root_global.mul_transform(node_local);
        let node = world.spawn((node_local, node_global, ChildOf(root))).id();
        let mut mesh_local = mesh_local;
        mesh_local.translation += mesh_shift;
        world.spawn((
            mesh_local,
            node_global.mul_transform(mesh_local),
            Mesh3d(mesh),
            ChildOf(node),
        ));
        (root, ExpectedModelBounds { min, max })
    }

    fn scene_node() -> Transform {
        Transform::from_rotation(Quat::from_rotation_x(-0.55) * Quat::from_rotation_z(0.3))
            .with_translation(Vec3::new(-1.23, 4.07, 0.61))
    }

    fn rotated_mesh_node() -> Transform {
        Transform::from_rotation(
            Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.35) * Quat::from_rotation_z(-0.2),
        )
        .with_translation(Vec3::new(3.37, 0.51, -2.19))
        .with_scale(Vec3::new(1.5, 0.75, 2.0))
    }

    fn turned_node_case(billboard: bool) -> Result<TransformValidationSummary, String> {
        let mut app = App::new();
        let mut meshes = Assets::<Mesh>::default();
        let mesh = meshes.add(Cuboid::new(2.0, 4.0, 6.0));
        app.insert_resource(meshes);
        spawn_bounds_fixture(
            app.world_mut(),
            Transform::default(),
            scene_node(),
            rotated_mesh_node(),
            Vec3::ZERO,
            mesh,
        );
        let world = app.world_mut();
        let node = world
            .query_filtered::<Entity, With<Mesh3d>>()
            .single(world)
            .expect("the fixture has one mesh node");
        let authored = rotated_mesh_node().rotation;
        let mut entity = world.entity_mut(node);
        entity.get_mut::<Transform>().unwrap().rotation = Quat::from_rotation_y(1.1) * authored;
        if billboard {
            entity.insert(crate::billboard::Billboard {
                mode: crate::billboard::BillboardMode::TurnAboutUp,
                authored,
            });
        }
        app.init_resource::<BoundsOutcome>()
            .add_systems(Update, run_bounds_check);
        app.update();
        app.world_mut()
            .remove_resource::<BoundsOutcome>()
            .expect("the bounds check system ran")
            .check
            .expect("the bounds check ran")
    }

    #[test]
    fn bounds_check_measures_a_billboard_at_its_authored_rotation() {
        let turned_billboard = turned_node_case(true);
        assert!(
            turned_billboard.is_ok(),
            "a billboard turned toward the camera must pass: {turned_billboard:?}"
        );
        let reason = turned_node_case(false)
            .expect_err("the same turn on an ordinary node is a real divergence");
        assert!(
            reason.contains("spawned hierarchy bounds diverge from conversion"),
            "{reason}"
        );
    }
}
