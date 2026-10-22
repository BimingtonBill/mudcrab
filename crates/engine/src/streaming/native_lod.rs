//! Current-view native LOD handoff. Residency is an input, never draw proof.
use super::{
    PendingUnder, RenderOrigin, StreamingWorld,
    lod_objects::{BlockKey, ObjectLodStreaming, ObjectRenderState, select_blocks},
};
use crate::{
    cluster::{
        stock_drawable::{StockDrawable, StockPassFamily},
        stock_visibility::ClassFilter,
    },
    portal::CellRole,
    world::database::CellKey,
};
use bevy::{
    core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass, NormalPrepass},
    prelude::*,
    render::{
        Extract, ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems,
        sync_world::MainEntity,
        view::{ExtractedView, RenderVisibleEntities, RetainedViewEntity},
    },
};
use std::collections::{HashMap, HashSet};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct NativeLodCell {
    pub world: u32,
    pub cell: IVec2,
}

/// Without qualified contributor paths/relation proof, far native ownership wins.
#[derive(Resource, Default)]
pub(crate) struct FarNativePolicy {
    pub revision: u64,
    window: Option<(u32, IVec2, i32)>,
    /// With owned statics (`--cluster-own-statics`) the cluster draws what it owns at every
    /// distance and no native far LOD draws it, so nothing is withheld for one.
    owned: bool,
}
impl FarNativePolicy {
    pub(crate) fn cluster_inside_grid(&self, world: u32, cell: IVec2) -> bool {
        self.owned
            || self.window.is_some_and(|(w, center, radius)| {
                w == world
                    && (i64::from(cell.x) - i64::from(center.x)).abs() <= i64::from(radius)
                    && (i64::from(cell.y) - i64::from(center.y)).abs() <= i64::from(radius)
            })
    }
}

/// Beyond the loaded grid, a cluster is withheld where native LOD may hold the same placement:
/// trees (billboards) and possible object LOD contributors, which need both the base's Has
/// Distant LOD flag (or missing base data) and the placement's Visible When Distant flag.
/// A placement without Visible When Distant is not in object LOD, so its cluster draws.
pub(crate) fn suppress_unqualified_far_cluster(
    inside_grid: bool,
    tree_lod_source: bool,
    base_has_distant_lod: Option<bool>,
    visible_when_distant: bool,
) -> bool {
    !inside_grid
        && (tree_lod_source || (visible_when_distant && base_has_distant_lod != Some(false)))
}

/// Entities whose forward draw proof is missing that a periodic log line names, per view.
const STUCK_SAMPLES: usize = 4;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub(crate) enum NativeLodSystems {
    Filter,
    Restore,
}

#[derive(Resource, Default)]
struct Inputs {
    native: HashMap<MainEntity, NativeLodCell>,
    ready: HashMap<NativeLodCell, bevy::camera::visibility::RenderLayers>,
    near: HashMap<MainEntity, NativeLodCell>,
    objects: Option<ObjectRenderState>,
    object_meshes: HashMap<MainEntity, BlockKey>,
    /// Ready object blocks whose root is not drawing this frame (spawned Hidden and not yet
    /// shown by the loader, or held). They cannot replace a drawing parent.
    hidden_roots: HashSet<BlockKey>,
    origin: IVec2,
    distances: crate::config::TerrainLodDistances,
    stream_radius: i32,
    /// Bumped whenever anything above changed, so a consumer can tell a frame with the same
    /// inputs from a new one without comparing them.
    generation: u64,
}
#[derive(Resource, Default)]
struct Filters {
    filters: HashMap<Entity, ClassFilter>,
    active: Vec<Entity>,
    shown: HashMap<Entity, HashSet<BlockKey>>,
    world: Option<u32>,
    coverage: HashMap<Entity, HashSet<IVec2>>,
    audited_near: HashMap<Entity, HashSet<NativeLodCell>>,
    audit_counts: HashMap<Entity, (usize, usize)>,
}

#[derive(Resource, Default)]
struct DrawStats {
    pending: usize,
    visible: usize,
    frames: u64,
    hidden_segments: usize,
    tree_cells: usize,
    object_blocks: usize,
    awaiting: usize,
    drawn_twice: usize,
    drawn_by_nothing: usize,
}

fn publish(mut main: ResMut<MainWorld>, stats: Res<DrawStats>) {
    if let Some(mut metrics) = main.get_resource_mut::<super::StreamingMetrics>() {
        metrics.pending_native_lod_draws = stats.pending;
        metrics.visible_native_lod_draws = stats.visible;
        metrics.lod_object_segments_hidden = stats.hidden_segments;
        metrics.lod_tree_cells_visible = stats.tree_cells;
        metrics.lod_object_blocks_visible = stats.object_blocks;
        metrics.native_lod_drawn_twice = stats.drawn_twice;
        metrics.native_lod_drawn_by_nothing = stats.drawn_by_nothing;
    }
}

/// What the last full evaluation saw: the input generation and the views it walked.
#[derive(Default)]
struct Evaluated {
    generation: Option<u64>,
    views: Vec<RetainedViewEntity>,
}

#[allow(clippy::type_complexity)]
fn evaluate(
    inputs: Res<Inputs>,
    stock: StockDrawable,
    phases: crate::swap_bridge::systems::QueuePhases,
    mut stats: ResMut<DrawStats>,
    views: Query<(
        &ExtractedView,
        &RenderVisibleEntities,
        Has<DepthPrepass>,
        Has<NormalPrepass>,
        Has<MotionVectorPrepass>,
    )>,
    mut evaluated: Local<Evaluated>,
) {
    let pending_now = stats.awaiting + stats.drawn_twice + stats.drawn_by_nothing;
    // Once every native draw is proven, the proof only changes with its inputs: the extracted
    // inputs, the views, or what a view sees (Bevy lists the entities a view gained or lost this
    // frame). With none of those moving, the last proof stands and nothing is walked.
    let current_views: Vec<RetainedViewEntity> = views
        .iter()
        .map(|(view, ..)| view.retained_view_entity)
        .collect();
    let visible_moved = views.iter().any(|(_, visible, ..)| {
        visible.get::<Mesh3d>().is_some_and(|class| {
            !class.added_entities().is_empty() || !class.removed_entities.is_empty()
        })
    });
    if stats.pending == 0
        && pending_now == 0
        && evaluated.generation == Some(inputs.generation)
        && evaluated.views == current_views
        && !visible_moved
    {
        stats.frames = stats.frames.wrapping_add(1);
        return;
    }
    evaluated.generation = Some(inputs.generation);
    evaluated.views = current_views;
    stats.pending = pending_now;
    stats.visible = 0;
    stats.tree_cells = 0;
    stats.object_blocks = 0;
    stats.frames = stats.frames.wrapping_add(1);
    for (view, visible, depth, normal, motion) in &views {
        let mut failure_counts = [0usize; 4];
        let mut stuck = Vec::new();
        let mut trees = HashSet::new();
        let mut blocks = HashSet::new();
        let forward = phases.forward(&view.retained_view_entity);
        let prepass = phases.prepass(&view.retained_view_entity);
        for (_, main) in visible
            .get::<Mesh3d>()
            .into_iter()
            .flat_map(|class| class.iter_visible())
        {
            if !inputs.native.contains_key(main)
                && !inputs.object_meshes.contains_key(main)
                && !inputs.near.contains_key(main)
            {
                continue;
            }
            stats.visible += 1;
            if let Some(key) = inputs.object_meshes.get(main) {
                blocks.insert(*key);
            } else if let Some(cell) = inputs.native.get(main) {
                trees.insert(*cell);
            }
            let queued_forward = forward.queued(*main);
            let proof_forward =
                stock.drawable(*main, view.retained_view_entity, StockPassFamily::Forward);
            let needs_prepass = (depth || normal || motion) && stock.prepass_required(*main);
            let queued_prepass = !needs_prepass || prepass.queued(*main);
            let proof_prepass = !needs_prepass
                || stock.drawable(*main, view.retained_view_entity, StockPassFamily::Prepass);
            for (count, missing) in failure_counts.iter_mut().zip([
                !queued_forward,
                !proof_forward,
                !queued_prepass,
                !proof_prepass,
            ]) {
                *count += usize::from(missing);
            }
            if !proof_forward && stuck.len() < STUCK_SAMPLES && stats.frames.is_multiple_of(120) {
                let source = if let Some(block) = inputs.object_meshes.get(main) {
                    format!("object block {block:?}")
                } else if let Some(cell) = inputs.native.get(main) {
                    format!("native {cell:?}")
                } else {
                    format!("near {:?}", inputs.near.get(main))
                };
                stuck.push((
                    *main,
                    source,
                    stock.missing_stage(*main, view.retained_view_entity),
                    stock.forward_pipeline_report(*main, view.retained_view_entity),
                ));
            }
            stats.pending +=
                usize::from(!(queued_forward && proof_forward && queued_prepass && proof_prepass));
        }
        stats.tree_cells += trees.len();
        stats.object_blocks += blocks.len();
        if stats.frames.is_multiple_of(120) && failure_counts.iter().any(|count| *count > 0) {
            info!(view=?view.retained_view_entity,?failure_counts,
                fields="forward queued, forward payload/material/pipeline, applicable prepass queued, applicable prepass payload/material/pipeline",
                "native/near view draw proof missing predicates");
            for (main, source, stage, pipeline) in &stuck {
                info!(view=?view.retained_view_entity, ?main, source, stage, ?pipeline,
                    "native/near view forward draw proof missing");
            }
        }
    }
    if stats.frames.is_multiple_of(120) {
        info!(
            pending = stats.pending,
            awaiting_candidates = stats.awaiting,
            visible = stats.visible,
            drawn_twice = stats.drawn_twice,
            drawn_by_nothing = stats.drawn_by_nothing,
            "native LOD current-view queued-draw proof"
        );
    }
}

pub(super) struct NativeLodPlugin;
impl Plugin for NativeLodPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<DrawStats>()
                .init_resource::<FarNativePolicy>()
                .init_resource::<Inputs>()
                .init_resource::<Filters>()
                .add_systems(ExtractSchedule, (publish, extract).chain())
                .add_systems(
                    Render,
                    // Material allocation/updates can invalidate a bindless slab
                    // every frame. The stock draw proof must inspect this frame's
                    // prepared group, before selecting either representation.
                    // Bevy's later preparation remains safe and is a no-op for
                    // unchanged slabs; buffer uploads retain their normal stage.
                    bevy::pbr::prepare_material_bind_groups
                        .in_set(RenderSystems::Queue)
                        .before(NativeLodSystems::Filter),
                )
                .add_systems(
                    Render,
                    evaluate
                        .after(RenderSystems::PrepareBindGroups)
                        .before(crate::swap_bridge::systems::DestinationSystems::Evaluate)
                        .before(RenderSystems::Render),
                )
                .add_systems(
                    Render,
                    filter
                        .in_set(RenderSystems::Queue)
                        .in_set(NativeLodSystems::Filter)
                        .before(RenderSystems::QueueMeshes),
                )
                .add_systems(
                    Render,
                    restore
                        .in_set(RenderSystems::Cleanup)
                        .in_set(NativeLodSystems::Restore),
                );
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// The descendant walks [`extract`] last made, and what they were made from. A walk visits every
/// entity under every object LOD root and every resident cell; its result changes only with the
/// hierarchy (a child or mesh added or removed, which [`super::identity_changes::StructureEpoch`]
/// counts), the roots, or the resident cells.
/// `Inputs::object_meshes` and `Inputs::near` hold the walks themselves between frames.
#[derive(Default)]
struct DescendantWalks {
    key: Option<WalkKey>,
    demand: Option<super::lod::LodDemand>,
    native_count: usize,
}

/// The hierarchy counts, the object LOD roots and the resident cells a walk was made from.
type WalkKey = (
    [usize; 2],
    HashMap<Entity, BlockKey>,
    HashMap<CellKey, Entity>,
);

/// What can change the descendant walks and the native cell map: a parent link or mesh added or
/// changed, or how many there are (a removal or a disabled entity shows as a smaller count).
/// Narrower than the global structure epoch, which moves with every visibility or material
/// component added anywhere in the world.
#[allow(clippy::type_complexity)]
#[derive(bevy::ecs::system::SystemParam)]
struct NativeChanges<'w, 's> {
    hierarchy: Query<'w, 's, (), Or<(Changed<ChildOf>, Added<Mesh3d>)>>,
    parented: Query<'w, 's, (), With<ChildOf>>,
    meshes: Query<'w, 's, (), With<Mesh3d>>,
    native: Query<'w, 's, (), Changed<NativeLodCell>>,
    native_count: Query<'w, 's, (), With<NativeLodCell>>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn extract(
    mut out: ResMut<Inputs>,
    mut policy: ResMut<FarNativePolicy>,
    native: Extract<Query<(Entity, &NativeLodCell)>>,
    cells: Extract<Res<StreamingWorld>>,
    pending: Extract<PendingUnder>,
    children: Extract<Query<&Children>>,
    meshes: Extract<Query<(), With<Mesh3d>>>,
    objects: Extract<Res<ObjectLodStreaming>>,
    config: Extract<Res<crate::config::EngineConfig>>,
    origin: Extract<Res<RenderOrigin>>,
    views: Extract<super::lod::LodDemandViews>,
    portal: Extract<Option<Res<crate::portal::PortalState>>>,
    mut timing: Local<crate::profiling::PeriodicTiming>,
    root_visibility: Extract<
        Query<(
            &Visibility,
            &InheritedVisibility,
            Option<&bevy::camera::visibility::RenderLayers>,
        )>,
    >,
    (changes, mut walks): (Extract<NativeChanges>, Local<DescendantWalks>),
) {
    let started = std::time::Instant::now();
    let window = views
        .current(&config, origin.0, portal.as_deref())
        .map(|d| (d.world, d.center, config.stream_radius));
    if policy.window != window || policy.owned != config.cluster_own_statics {
        policy.window = window;
        policy.owned = config.cluster_own_statics;
        policy.revision = policy.revision.wrapping_add(1);
    }
    // An object LOD state with no ready block (object LOD switched off, or nothing loaded yet)
    // gives the filter nothing to select or hide, and makes it walk every near cell for nothing.
    out.objects = objects
        .render_state()
        .filter(|objects| !objects.roots.is_empty());
    let demand = out.objects.as_ref().map(|objects| objects.demand);
    let mut hidden_roots = HashSet::new();
    let roots: HashMap<Entity, BlockKey> = out
        .objects
        .as_ref()
        .map(|objects| {
            objects
                .roots
                .iter()
                .map(|(root, key)| (*root, *key))
                .collect()
        })
        .unwrap_or_default();
    for (root, key) in &roots {
        if !root_visibility
            .get(*root)
            .is_ok_and(|(explicit, inherited, _)| loaded_root_visible(*explicit, inherited.get()))
        {
            hidden_roots.insert(*key);
        }
    }
    let mut moved = hidden_roots != out.hidden_roots || walks.demand != demand;
    out.hidden_roots = hidden_roots;
    walks.demand = demand;
    let resident: HashMap<CellKey, Entity> = cells.full_resident_cells().collect();
    let counts = [changes.parented.iter().len(), changes.meshes.iter().len()];
    let hierarchy_moved = !changes.hierarchy.is_empty();
    let current = Some((counts, roots, resident));
    if hierarchy_moved || walks.key != current {
        moved = true;
        perf_span!("native_lod::extract::walk");
        out.object_meshes.clear();
        out.near.clear();
        let roots: HashMap<Entity, BlockKey> = out
            .objects
            .as_ref()
            .map(|objects| {
                objects
                    .roots
                    .iter()
                    .map(|(root, key)| (*root, *key))
                    .collect()
            })
            .unwrap_or_default();
        for (root, key) in &roots {
            for entity in children.iter_descendants(*root) {
                if meshes.contains(entity) {
                    out.object_meshes.insert(entity.into(), *key);
                }
            }
        }
        for (key, root) in cells.full_resident_cells() {
            let CellKey::Exterior {
                worldspace_id,
                grid_x,
                grid_y,
            } = key
            else {
                continue;
            };
            let cell = NativeLodCell {
                world: worldspace_id,
                cell: IVec2::new(grid_x, grid_y),
            };
            for entity in children.iter_descendants(root) {
                if meshes.contains(entity) {
                    out.near.insert(entity.into(), cell);
                }
            }
        }
        walks.key = current;
    }

    let _tail_span = ::bevy::log::info_span!("native_lod::extract::tail").entered();
    if out.origin != origin.0
        || out.distances != config.terrain_lod
        || out.stream_radius != config.stream_radius
    {
        out.origin = origin.0;
        out.distances = config.terrain_lod;
        out.stream_radius = config.stream_radius;
        moved = true;
    }
    let native_count = changes.native_count.iter().len();
    if moved || !changes.native.is_empty() || walks.native_count != native_count {
        out.native = native.iter().map(|(e, c)| (e.into(), *c)).collect();
        walks.native_count = native_count;
        moved = true;
    }
    let previous_ready = std::mem::take(&mut out.ready);
    // Which near cells are drawn whole is read only by `filter`, and only when it has object LOD
    // blocks or native tree cells to decide; each check walks the cell's whole hierarchy.
    let needed = out.objects.is_some() || !out.native.is_empty();
    for (key, root) in cells.full_resident_cells().filter(|_| needed) {
        let CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        } = key
        else {
            continue;
        };
        let cell = NativeLodCell {
            world: worldspace_id,
            cell: IVec2::new(grid_x, grid_y),
        };
        if let Ok((explicit, inherited, layers)) = root_visibility.get(root)
            && loaded_root_visible(*explicit, inherited.get())
            && pending.count(root) == 0
            && pending.failed(root) == 0
        {
            out.ready.insert(cell, layers.cloned().unwrap_or_default());
        }
    }
    if moved || out.ready != previous_ready {
        out.generation = out.generation.wrapping_add(1);
    }
    timing.record("native_lod/extract", started.elapsed());
}

fn loaded_root_visible(explicit: Visibility, inherited: bool) -> bool {
    explicit != Visibility::Hidden && inherited
}

fn qualified_cell(assets_ready: bool, visible: impl IntoIterator<Item = bool>) -> bool {
    assets_ready && visible.into_iter().all(|ready| ready)
}

#[allow(clippy::type_complexity)]
fn filter(
    inputs: Res<Inputs>,
    stock: StockDrawable,
    mut filters: ResMut<Filters>,
    mut stats: ResMut<DrawStats>,
    mut timing: Local<crate::profiling::PeriodicTiming>,
    mut views: Query<(
        Entity,
        &ExtractedView,
        &mut RenderVisibleEntities,
        Has<DepthPrepass>,
        Has<NormalPrepass>,
        Has<MotionVectorPrepass>,
        Option<&bevy::camera::visibility::RenderLayers>,
    )>,
) {
    let started = std::time::Instant::now();
    assert!(
        filters.active.is_empty(),
        "native LOD visibility was not restored"
    );
    let world = inputs.objects.as_ref().map(|objects| objects.demand.world);
    if filters.world != world {
        filters.world = world;
        filters.shown.clear();
        filters.coverage.clear();
        filters.audited_near.clear();
        filters.audit_counts.clear();
    }
    stats.hidden_segments = 0;
    stats.awaiting = 0;
    stats.drawn_twice = 0;
    stats.drawn_by_nothing = 0;
    for (entity, view, mut visible, depth, normal, motion, layers) in &mut views {
        let Some(class) = visible.classes.get_mut(&std::any::TypeId::of::<Mesh3d>()) else {
            continue;
        };
        let layers = layers.cloned().unwrap_or_default();
        let objects = inputs.objects.as_ref().filter(|objects| {
            super::lod::lod_role_layers(demand_role(Some(objects.demand))).intersects(&layers)
        });
        // Near readiness only decides which native draws (object blocks, tree cells) give way.
        // A view with neither has nothing to hide or select, and nothing to audit.
        if objects.is_none() && inputs.native.is_empty() {
            filters.shown.remove(&entity);
            filters.coverage.remove(&entity);
            filters.audited_near.remove(&entity);
            filters.audit_counts.remove(&entity);
            continue;
        }
        let mut readiness: HashMap<NativeLodCell, Vec<bool>> = HashMap::new();
        for (_, main) in class.iter_visible() {
            if let Some(cell) = inputs.near.get(main) {
                readiness.entry(*cell).or_default().push(
                    stock.drawable(*main, view.retained_view_entity, StockPassFamily::Forward)
                        && (!(depth || normal || motion)
                            || !stock.prepass_required(*main)
                            || stock.drawable(
                                *main,
                                view.retained_view_entity,
                                StockPassFamily::Prepass,
                            )),
                );
            }
        }
        let covered: HashSet<_> = inputs
            .ready
            .iter()
            .filter_map(|(cell, root_layers)| {
                (root_layers.intersects(&layers)
                    && qualified_cell(true, readiness.get(cell).into_iter().flatten().copied()))
                .then_some(*cell)
            })
            .collect();
        let mut block_readiness: HashMap<BlockKey, bool> = HashMap::new();
        let mut missing_stages = std::collections::BTreeMap::<&'static str, usize>::new();
        if let Some(objects) = objects {
            // A root that is not drawing has no current draw proof, whatever its residency:
            // selecting it would retire a drawing parent for a frame with neither drawn.
            block_readiness.extend(
                objects
                    .roots
                    .values()
                    .map(|key| (*key, !inputs.hidden_roots.contains(key))),
            );
        }
        for (_, main) in class.iter_visible() {
            if let Some(key) = inputs.object_meshes.get(main) {
                let ready =
                    stock.drawable(*main, view.retained_view_entity, StockPassFamily::Forward)
                        && (!(depth || normal || motion)
                            || !stock.prepass_required(*main)
                            || stock.drawable(
                                *main,
                                view.retained_view_entity,
                                StockPassFamily::Prepass,
                            ));
                *block_readiness.entry(*key).or_insert(true) &= ready;
                if !ready && stats.frames.is_multiple_of(120) {
                    *missing_stages
                        .entry(stock.missing_stage(*main, view.retained_view_entity))
                        .or_default() += 1;
                }
            }
        }
        if !missing_stages.is_empty() {
            info!(view=?view.retained_view_entity,?missing_stages,"native visible block candidate draw predicates missing");
        }
        let position = view.world_from_view.translation();
        let view_center = inputs.origin
            + IVec2::new(
                (position.x / super::CELL_SIZE).floor() as i32,
                (-position.z / super::CELL_SIZE).floor() as i32,
            );
        let plan = objects
            .and_then(|objects| {
                let world = objects.index.worlds.get(&objects.demand.world)?;
                let center = view_center;
                let ready = block_readiness
                    .iter()
                    .filter_map(|(key, ready)| ready.then_some(*key))
                    .collect();
                Some(select_blocks(
                    world,
                    center,
                    &|level| inputs.distances.object_reach_cells(level),
                    0,
                    &ready,
                    // GPU preparation can revoke a previously drawn fine child.
                    // The retained drawable parent then owns the whole block;
                    // preserving partial fine history would open a hole.
                    &HashSet::new(),
                    inputs.stream_radius,
                    &|cell| {
                        covered.contains(&NativeLodCell {
                            world: objects.demand.world,
                            cell,
                        })
                    },
                ))
            })
            .unwrap_or_default();
        // A retained ancestor that the current view does not select is not
        // an outstanding draw. Only requested visible block candidates hold proof.
        let visible_blocks: HashSet<_> = class
            .iter_visible()
            .filter_map(|(_, main)| inputs.object_meshes.get(main).copied())
            .collect();
        stats.awaiting += plan
            .needed
            .iter()
            .filter(|key| {
                visible_blocks.contains(key) && !block_readiness.get(key).copied().unwrap_or(false)
            })
            .count();
        let shown = plan.shown;
        if objects.is_none() {
            filters.coverage.remove(&entity);
            filters.audited_near.remove(&entity);
            filters.audit_counts.remove(&entity);
        }

        if filters.shown.get(&entity) != Some(&shown)
            || filters.audited_near.get(&entity) != Some(&covered)
        {
            let mut coverage = HashMap::<IVec2, usize>::new();
            for (level, block) in &shown {
                for dx in 0..i32::from(*level) {
                    for dy in 0..i32::from(*level) {
                        *coverage.entry(*block + IVec2::new(dx, dy)).or_default() += 1;
                    }
                }
            }
            let twice = coverage.values().filter(|count| **count > 1).count();
            // The loader's level-aware audit, from this view's centre and the shown set it
            // drew last frame; near cells that draw their own models count as drawn.
            let mut missing = 0;
            if let Some(previous) = filters.shown.get(&entity)
                && let Some(objects) = objects
                && let Some(world) = objects.index.worlds.get(&objects.demand.world)
            {
                let demand_world = objects.demand.world;
                let near = |cell: IVec2| {
                    covered.contains(&NativeLodCell {
                        world: demand_world,
                        cell,
                    })
                };
                let reach = |level: u8| inputs.distances.object_reach_cells(level);
                missing = super::lod_objects::audit_departures(
                    world,
                    previous,
                    &shown,
                    &near,
                    view_center,
                    &reach,
                )
                .holes as usize;
            }
            filters
                .coverage
                .insert(entity, coverage.into_keys().collect());
            filters.audit_counts.insert(entity, (twice, missing));
            filters.audited_near.insert(entity, covered.clone());
        }
        let (twice, missing) = filters
            .audit_counts
            .get(&entity)
            .copied()
            .unwrap_or_default();
        stats.drawn_twice += twice;
        stats.drawn_by_nothing += missing;
        filters.shown.insert(entity, shown.clone());
        let mut hidden: bevy::platform::collections::HashSet<MainEntity> = class
            .iter_visible()
            .filter_map(|(_, main)| {
                inputs
                    .object_meshes
                    .get(main)
                    .filter(|key| !shown.contains(key))
                    .map(|_| *main)
            })
            .collect();
        hidden.extend(class.iter_visible().filter_map(|(_, main)| {
            let cell = inputs.native.get(main)?;
            qualified_cell(
                covered.contains(cell),
                readiness.get(cell).into_iter().flatten().copied(),
            )
            .then_some(*main)
        }));
        stats.hidden_segments += hidden
            .iter()
            .filter(|main| {
                inputs.native.contains_key(main) && inputs.object_meshes.contains_key(main)
            })
            .count();
        filters
            .filters
            .entry(entity)
            .or_default()
            .apply(class, &hidden);
        filters.active.push(entity);
    }
    timing.record("native_lod/select_and_filter", started.elapsed());
}
fn restore(mut filters: ResMut<Filters>, mut views: Query<&mut RenderVisibleEntities>) {
    for entity in std::mem::take(&mut filters.active) {
        if let Ok(mut visible) = views.get_mut(entity)
            && let Some(class) = visible.classes.get_mut(&std::any::TypeId::of::<Mesh3d>())
        {
            filters
                .filters
                .get_mut(&entity)
                .expect("active filter")
                .restore(class);
        } else {
            filters.filters.remove(&entity);
        }
    }
}

pub(super) fn publish_roles(
    commands: &mut Commands,
    roots: impl Iterator<Item = (Entity, CellRole)>,
    layers: &Query<&bevy::camera::visibility::RenderLayers>,
) -> Vec<(Entity, CellRole)> {
    roots
        .map(|(root, role)| {
            let next = super::lod::lod_role_layers(role);
            if layers.get(root).is_ok_and(|old| *old == next) {
                return (root, role);
            }
            commands
                .entity(root)
                .try_insert((next.clone(), bevy::app::Propagate(next)));
            (root, role)
        })
        .collect()
}

pub(super) fn demand_role(demand: Option<super::lod::LodDemand>) -> CellRole {
    match demand.map(|d| d.role) {
        Some(super::lod::LodDemandRole::Main) => CellRole::Active,
        Some(super::lod::LodDemandRole::Portal) => CellRole::Destination,
        _ => CellRole::Hidden,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn residency_or_one_missing_pass_never_certifies_a_cell() {
        assert!(!qualified_cell(false, [true, true]));
        assert!(!qualified_cell(true, [true, false]));
        assert!(qualified_cell(true, [true, true]));
        assert!(qualified_cell(true, []));
    }
    #[test]
    fn missing_contributor_policy_keeps_far_native_for_every_world_and_boundary() {
        let policy = FarNativePolicy {
            revision: 1,
            window: Some((7, IVec2::new(-2, 3), 2)),
            owned: false,
        };
        assert!(policy.cluster_inside_grid(7, IVec2::new(-4, 5)));
        assert!(!policy.cluster_inside_grid(7, IVec2::new(-5, 5)));
        assert!(!policy.cluster_inside_grid(8, IVec2::new(-2, 3)));
        assert!(!FarNativePolicy::default().cluster_inside_grid(7, IVec2::ZERO));
    }

    #[test]
    fn owned_statics_withhold_no_far_cluster_anywhere() {
        let policy = FarNativePolicy {
            revision: 1,
            window: None,
            owned: true,
        };
        for (world, cell) in [(7, IVec2::new(-500, 900)), (8, IVec2::ZERO)] {
            let inside = policy.cluster_inside_grid(world, cell);
            assert!(inside);
            for flag in [None, Some(false), Some(true)] {
                for vwd in [false, true] {
                    for tree in [false, true] {
                        assert!(!suppress_unqualified_far_cluster(inside, tree, flag, vwd));
                    }
                }
            }
        }
    }

    #[test]
    fn missing_contribution_proof_cannot_retire_native_geometry() {
        for flag in [None, Some(false), Some(true)] {
            for vwd in [false, true] {
                for tree in [false, true] {
                    assert!(!suppress_unqualified_far_cluster(true, tree, flag, vwd));
                }
                assert!(suppress_unqualified_far_cluster(false, true, flag, vwd));
            }
        }
        assert!(suppress_unqualified_far_cluster(false, false, None, true));
        assert!(suppress_unqualified_far_cluster(
            false,
            false,
            Some(true),
            true
        ));
        assert!(!suppress_unqualified_far_cluster(
            false,
            false,
            Some(false),
            true
        ));
        // A placement without Visible When Distant is not in object LOD: its cluster draws.
        for flag in [None, Some(false), Some(true)] {
            assert!(!suppress_unqualified_far_cluster(false, false, flag, false));
        }
    }

    #[test]
    fn held_hidden_roots_are_not_vacuous_draw_proof() {
        assert!(!loaded_root_visible(Visibility::Hidden, true));
        assert!(!loaded_root_visible(Visibility::Inherited, false));
        assert!(loaded_root_visible(Visibility::Inherited, true));
        let world = super::super::lod::lod_role_layers(CellRole::Active);
        let destination = super::super::lod::lod_role_layers(CellRole::Destination);
        assert!(!world.intersects(&destination));
    }

    #[test]
    fn nested_native_and_cluster_filters_restore_exact_visibility() {
        use bevy::render::view::visibility::RenderVisibleEntitiesClass;
        let mut ids: Vec<_> = (1..=3)
            .map(|id| {
                (
                    Entity::from_raw_u32(id + 100).unwrap(),
                    MainEntity::from(Entity::from_raw_u32(id).unwrap()),
                )
            })
            .collect();
        // Bevy hands the class its CPU list sorted by main entity (opaque Ord, not row order).
        ids.sort_by_key(|(_, main)| *main);
        let mut class = RenderVisibleEntitiesClass::default();
        class.update_cpu_culled_entities(&ids);
        let raw = class.clone();
        let mut native = ClassFilter::default();
        let mut clusters = ClassFilter::default();
        native.apply(&mut class, &[ids[0].1].into_iter().collect());
        clusters.apply(&mut class, &[ids[1].1].into_iter().collect());
        assert_eq!(class.iter_visible().count(), 1);
        assert_eq!(class.iter_visible().next().unwrap().1, &ids[2].1);
        clusters.restore(&mut class);
        native.restore(&mut class);
        assert_eq!(class.entities_cpu_culling, raw.entities_cpu_culling);
        assert_eq!(class.added_entities, raw.added_entities);
        assert_eq!(class.removed_entities, raw.removed_entities);
    }

    #[test]
    fn native_identity_distinguishes_world_and_cell() {
        let a = NativeLodCell {
            world: 1,
            cell: IVec2::ZERO,
        };
        let map = HashMap::from([(a, 42)]);
        assert_eq!(map.get(&a), Some(&42));
        assert_eq!(map.get(&NativeLodCell { world: 2, ..a }), None);
        assert_eq!(
            map.get(&NativeLodCell {
                cell: IVec2::X,
                ..a
            }),
            None
        );
    }
}
