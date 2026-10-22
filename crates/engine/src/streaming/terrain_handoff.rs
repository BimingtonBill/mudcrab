//! Current-render-view terrain complement. Main-world residency is never draw proof.
//! All fallback primitives stay available to extraction. Filter only after current
//! specialization, restore raw visibility in Cleanup, and rebuild from this frame's
//! immutable masks and cluster selection (including generation and plan identity).
use super::{
    MergedTerrainCoverage, RenderOrigin, RingTerrain, TerrainIdentityAccess, TerrainWorldspace,
    lod::{LodChunkRoot, terrain_batching::LodTerrainBatches},
};
use crate::{
    cluster::{
        stock_drawable::{StockDrawable, StockPassFamily},
        stock_visibility::ClassFilter,
    },
    swap_bridge::{SwapDrawReceipt, SwapStandIn, SwapSurface, SwapWaiting},
};
use bevy::{
    core_pipeline::{
        core_3d::{AlphaMask3d, Opaque3d, Transparent3d},
        prepass::{
            AlphaMask3dPrepass, DepthPrepass, MotionVectorPrepass, NormalPrepass, Opaque3dPrepass,
        },
    },
    ecs::system::SystemParam,
    pbr::{
        LightEntity, PendingMeshMaterialQueues, PendingPrepassMeshMaterialQueues,
        PendingShadowQueues, RenderMeshInstances, Shadow, Transmissive3d,
    },
    prelude::*,
    render::{
        Extract, ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems,
        render_phase::{ViewBinnedRenderPhases, ViewSortedRenderPhases},
        sync_world::MainEntity,
        view::{
            ExtractedView, RenderShadowMapVisibleEntities, RenderVisibleEntities,
            RetainedViewEntity,
        },
    },
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SurfaceKey {
    pub world: u32,
    pub grid: IVec2,
    pub quadrant: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Detail,
    Ring,
    Native,
    Batch,
    Source,
}

pub(crate) struct Surface {
    pub keys: Vec<SurfaceKey>,
    pub(crate) mesh: bevy::asset::AssetId<Mesh>,
    kind: Kind,
    /// Outstanding own-draw obligations and live named holds cannot yield.
    protected: bool,
}

#[derive(Resource, Default)]
pub(crate) struct RenderTerrain {
    pub surfaces: HashMap<MainEntity, Surface>,
    /// Bumped whenever `surfaces` is rebuilt, so a consumer can keep what it derived from them.
    revision: u64,
    origin: IVec2,
    radius: i32,
    explicit: bool,
}

/// Written ONLY by current cluster selection in Render, cleared before every
/// selection invocation. Not extracted from TerrainDrawReadiness and never retained
/// as evidence across frames. The plan's selected blocks already include its exact
/// current compute/material/forward/prepass/shadow qualification.
#[derive(Resource, Default)]
pub(crate) struct ClusterCoverage {
    pub views: HashMap<RetainedViewEntity, ClusterViewCoverage>,
}

pub(crate) struct ClusterViewCoverage {
    pub generation: u64,
    pub layout: u64,
    pub plan: u64,
    pub cells: HashSet<(u32, IVec2)>,
}

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TerrainHandoffSystems {
    Cluster,
    /// Before mesh queueing: fallback terrain under this frame's selected cluster cells.
    Gate,
    Filter,
    Restore,
    ClusterRestore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Domain {
    Camera(Entity),
    Shadow(Entity, RetainedViewEntity),
}

#[derive(Resource, Default)]
struct Filters {
    saved: HashMap<Domain, ClassFilter>,
    active: Vec<Domain>,
}

/// The pre-queue stage of the terrain complement. It hides fallback terrain whose every cell is
/// in the current cluster plan, so Bevy's retained queues dequeue it once and then never visit
/// it again until the plan withdraws it.
#[derive(Resource, Default)]
struct GateFilters(Filters);

/// Profiling inventory only: exact renderable native and compact LOD meshes. It carries no
/// coverage, ancestry, ownership or stand-in state and never authorizes draw suppression.
#[derive(Resource, Default)]
struct LodDrawInventory {
    resident: HashSet<MainEntity>,
    meshes: HashMap<MainEntity, bevy::asset::AssetId<Mesh>>,
    #[cfg(test)]
    child_visits: usize,
}

#[derive(SystemParam)]
struct LodDiagnosticInputs<'w, 's> {
    native: Query<'w, 's, (&'static super::TerrainCoverage, &'static Children)>,
    batches: Query<'w, 's, &'static Children, With<LodTerrainBatches>>,
    meshes: Query<'w, 's, (&'static Mesh3d, Option<&'static InheritedVisibility>)>,
}

impl LodDrawInventory {
    fn collect(&mut self, inputs: &LodDiagnosticInputs) {
        self.resident.clear();
        self.meshes.clear();
        #[cfg(test)]
        {
            self.child_visits = 0;
        }
        // Validated native nodes have their primitive as an immediate child. Generated batches
        // and compact source primitives are immediate children of their owning batching root.
        // Neither join needs the terrain identity index or a hierarchy walk.
        let children = inputs
            .native
            .iter()
            .filter(|(coverage, _)| coverage.tier.is_some())
            .map(|(_, children)| children)
            .chain(inputs.batches.iter());
        for children in children {
            for entity in children.iter() {
                #[cfg(test)]
                {
                    self.child_visits += 1;
                }
                if let Ok((mesh, visibility)) = inputs.meshes.get(entity) {
                    self.resident.insert(entity.into());
                    if visibility.is_some_and(|visibility| visibility.get()) {
                        self.meshes.insert(entity.into(), mesh.0.id());
                    }
                }
            }
        }
    }
}

fn extract_lod_inventory(mut out: ResMut<LodDrawInventory>, inputs: Extract<LodDiagnosticInputs>) {
    perf_span!("terrain_lod::diagnostics::inventory");
    out.collect(&inputs);
}

/// Actual extracted instances and queued entity memberships before terrain complement filtering.
/// Membership counts deduplicate entities within each view/pass family. They do not count queue
/// insertion attempts, mesh subdraws, or draw commands, and are collected only for profile output.
#[derive(Resource, Default)]
struct TerrainDrawCounts {
    sampled: bool,
    resident_lod: usize,
    extracted_lod: usize,
    queued_forward: usize,
    queued_prepass: usize,
    queued_shadow: usize,
}

fn publish_counts(mut main: ResMut<MainWorld>, counts: Res<TerrainDrawCounts>) {
    if !counts.sampled {
        return;
    }
    if let Some(mut metrics) = main.get_resource_mut::<super::StreamingMetrics>() {
        metrics.terrain_lod_counts_measured = true;
        metrics.extracted_terrain_lod_items = counts.extracted_lod;
        metrics.queued_terrain_lod_forward_items = counts.queued_forward;
        metrics.queued_terrain_lod_prepass_items = counts.queued_prepass;
        metrics.queued_terrain_lod_shadow_items = counts.queued_shadow;
    }
    if let Some(mut profiler) = main.get_resource_mut::<crate::profiling::ProfilingState>() {
        profiler.set_gauge(
            "lod/resident_terrain_mesh_entities",
            counts.resident_lod as f64,
        );
        profiler.set_gauge("lod/extracted_terrain_items", counts.extracted_lod as f64);
        profiler.set_gauge(
            "lod/queued_terrain_forward_items",
            counts.queued_forward as f64,
        );
        profiler.set_gauge(
            "lod/queued_terrain_prepass_items",
            counts.queued_prepass as f64,
        );
        profiler.set_gauge(
            "lod/queued_terrain_shadow_items",
            counts.queued_shadow as f64,
        );
    }
}

#[allow(clippy::type_complexity)]
fn collect_counts(
    inventory: Res<LodDrawInventory>,
    instances: Res<RenderMeshInstances>,
    queued: crate::swap_bridge::systems::QueuePhases,
    views: Query<(&ExtractedView, Has<LightEntity>)>,
    mut counts: ResMut<TerrainDrawCounts>,
) {
    counts.sampled = true;
    counts.resident_lod = inventory.resident.len();
    counts.extracted_lod = inventory
        .meshes
        .iter()
        .filter(|(main, mesh)| instances.mesh_asset_id(**main) == Some(**mesh))
        .count();
    counts.queued_forward = 0;
    counts.queued_prepass = 0;
    counts.queued_shadow = 0;
    if inventory.meshes.is_empty() {
        return;
    }
    let mut visited = HashSet::new();
    for (view, is_shadow) in &views {
        let key = view.retained_view_entity;
        if !visited.insert((key, is_shadow)) {
            continue;
        }
        if is_shadow {
            let shadow = queued.shadow(&key);
            counts.queued_shadow += inventory
                .meshes
                .keys()
                .filter(|main| shadow.queued(**main))
                .count();
        } else {
            let forward = queued.forward(&key);
            let prepass = queued.prepass(&key);
            for main in inventory.meshes.keys() {
                counts.queued_forward += usize::from(forward.queued(*main));
                counts.queued_prepass += usize::from(prepass.queued(*main));
            }
        }
    }
}

pub(super) struct TerrainHandoffPlugin;
impl Plugin for TerrainHandoffPlugin {
    fn build(&self, app: &mut App) {
        let config = app.world().get_resource::<crate::config::EngineConfig>();
        let profiling = config.is_some_and(|config| config.profile_output_dir.is_some());
        // Cluster package/terrain are startup options, not mutable graphics-menu controls.
        // Install no terrain complement systems when their owner is absent.
        let enabled =
            config.is_some_and(|config| config.clusters.is_some() && config.cluster_terrain);
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        if profiling {
            render
                .init_resource::<TerrainDrawCounts>()
                .init_resource::<LodDrawInventory>()
                .add_systems(ExtractSchedule, (publish_counts, extract_lod_inventory))
                .add_systems(
                    Render,
                    collect_counts
                        .after(RenderSystems::Queue)
                        .before(TerrainHandoffSystems::Filter),
                );
        }
        if !enabled {
            return;
        }
        render
            .init_resource::<RenderTerrain>()
            .init_resource::<ClusterCoverage>()
            .init_resource::<Filters>()
            .init_resource::<GateFilters>()
            .add_systems(ExtractSchedule, extract)
            .add_systems(
                Render,
                gate.in_set(RenderSystems::Queue)
                    .in_set(TerrainHandoffSystems::Gate)
                    .after(super::native_lod::NativeLodSystems::Filter)
                    .after(TerrainHandoffSystems::Cluster)
                    .before(RenderSystems::QueueMeshes),
            )
            .add_systems(
                Render,
                filter
                    .after(RenderSystems::Queue)
                    .in_set(TerrainHandoffSystems::Filter)
                    .after(super::native_lod::NativeLodSystems::Filter)
                    .after(TerrainHandoffSystems::Gate)
                    .after(TerrainHandoffSystems::Cluster)
                    .before(RenderSystems::PhaseSort),
            )
            .add_systems(
                Render,
                restore
                    .in_set(RenderSystems::Cleanup)
                    .in_set(TerrainHandoffSystems::Restore)
                    .before(TerrainHandoffSystems::ClusterRestore)
                    .before(super::native_lod::NativeLodSystems::Restore),
            );
    }
}

type SurfaceObligations<'w, 's> = Query<
    'w,
    's,
    (
        &'static SwapSurface,
        Option<&'static SwapDrawReceipt>,
        Has<SwapWaiting>,
    ),
>;

fn protected(
    entity: Entity,
    mesh: bevy::asset::AssetId<Mesh>,
    surfaces: &SurfaceObligations,
    holds: &Query<(), With<SwapStandIn>>,
) -> bool {
    surfaces
        .get(entity)
        .is_ok_and(|(surface, receipt, waiting)| {
            // Historical receipts only discharge this surface's own obligation. They
            // never certify a replacement in another view or this frame's complement.
            (waiting && !receipt.is_some_and(|receipt| receipt.matches(mesh, surface)))
                || surface.stand_in().is_some_and(|root| holds.contains(root))
        })
}

#[derive(SystemParam)]
struct TerrainInputs<'w, 's> {
    visibility: Query<'w, 's, &'static InheritedVisibility>,
    meshes: Query<'w, 's, &'static Mesh3d>,
    rings: Query<'w, 's, (), With<RingTerrain>>,
    merged: Query<
        'w,
        's,
        (
            Entity,
            &'static MergedTerrainCoverage,
            &'static TerrainWorldspace,
        ),
    >,
    batches: Query<'w, 's, (&'static LodChunkRoot, &'static LodTerrainBatches)>,
    surfaces: SurfaceObligations<'w, 's>,
    holds: Query<'w, 's, (), With<SwapStandIn>>,
    origin: Res<'w, RenderOrigin>,
    config: Res<'w, crate::config::EngineConfig>,
}

/// What can change the fallback surface map. Visibility, mesh and swap state are tracked on every
/// streamed mesh, so a change there counts only on a terrain entity the map draws from; terrain
/// coverage, batches and named holds count wherever they change, and a smaller count of them is a
/// removal. Read every extraction, so a frame in which none of it moved keeps last frame's map
/// instead of rebuilding it from every terrain identity.
#[allow(clippy::type_complexity)]
#[derive(SystemParam)]
struct TerrainChanges<'w, 's> {
    visibility: Query<'w, 's, Entity, (With<Mesh3d>, Changed<InheritedVisibility>)>,
    meshes: Query<'w, 's, Entity, Changed<Mesh3d>>,
    swaps: Query<
        'w,
        's,
        Entity,
        Or<(
            Changed<SwapSurface>,
            Changed<SwapDrawReceipt>,
            Added<SwapWaiting>,
        )>,
    >,
    terrain: Query<
        'w,
        's,
        (),
        Or<(
            Added<SwapStandIn>,
            Changed<MergedTerrainCoverage>,
            Changed<TerrainWorldspace>,
            Changed<LodTerrainBatches>,
            Changed<LodChunkRoot>,
            Added<RingTerrain>,
        )>,
    >,
    waiting: Query<'w, 's, Entity, With<SwapWaiting>>,
    holds: Query<'w, 's, (), With<SwapStandIn>>,
    merged: Query<'w, 's, (), With<MergedTerrainCoverage>>,
    batches: Query<'w, 's, (), With<LodTerrainBatches>>,
    rings: Query<'w, 's, (), With<RingTerrain>>,
}

impl TerrainChanges<'_, '_> {
    /// How many of each terrain input exist; only terrain entities' waits are counted.
    fn counts(&self, terrain: impl Fn(Entity) -> bool) -> [usize; 5] {
        [
            self.waiting
                .iter()
                .filter(|entity| terrain(*entity))
                .count(),
            self.holds.iter().count(),
            self.merged.iter().count(),
            self.batches.iter().count(),
            self.rings.iter().count(),
        ]
    }

    fn moved(&self, terrain: impl Fn(Entity) -> bool) -> bool {
        !self.terrain.is_empty()
            || self.visibility.iter().any(&terrain)
            || self.meshes.iter().any(&terrain)
            || self.swaps.iter().any(&terrain)
    }
}

/// The identity snapshot, batch draws and input counts the current surface map was built from.
#[derive(Default)]
struct BuiltFrom {
    identities: Option<std::sync::Arc<HashMap<MainEntity, super::TerrainPrimitiveIdentity>>>,
    batch_entities: HashSet<Entity>,
    counts: [usize; 5],
}

fn extract(
    mut out: ResMut<RenderTerrain>,
    mut identities: Extract<TerrainIdentityAccess>,
    inputs: Extract<TerrainInputs>,
    changes: Extract<TerrainChanges>,
    mut built: Local<BuiltFrom>,
) {
    let TerrainInputs {
        visibility,
        meshes,
        rings,
        merged,
        batches,
        surfaces,
        holds,
        origin,
        config,
    } = &*inputs;
    out.origin = origin.0;
    out.radius = config.stream_radius;
    out.explicit = config.terrain_radius_given;
    // The identity index keeps its snapshot while nothing changed; holding the previous one here
    // makes a new snapshot a new allocation, so pointer identity says whether it moved.
    let snapshot = identities.identities_shared();
    // A terrain entity: an identity primitive, merged LAND, or a batch draw.
    let is_terrain = |entity: Entity| {
        snapshot.contains_key(&MainEntity::from(entity))
            || merged.contains(entity)
            || built.batch_entities.contains(&entity)
    };
    let counts = changes.counts(is_terrain);
    let unchanged = built
        .identities
        .as_ref()
        .is_some_and(|previous| std::sync::Arc::ptr_eq(previous, &snapshot))
        && built.counts == counts
        && !changes.moved(is_terrain);
    built.identities = Some(std::sync::Arc::clone(&snapshot));
    built.counts = counts;
    if unchanged {
        return;
    }
    perf_span!("terrain_handoff::extract::rebuild");
    out.revision = out.revision.wrapping_add(1);
    out.surfaces.clear();
    // Reuse the journaled exact identity index, including its ancestry validation.
    // InheritedVisibility is refreshed by Bevy for primitive/root/intermediate
    // in-place changes; a change to it on any mesh rebuilds the map.
    for (main, identity) in snapshot.iter() {
        if !visibility.get(main.id()).is_ok_and(|visible| visible.get()) {
            continue;
        }
        let Ok(mesh) = meshes.get(main.id()) else {
            continue;
        };
        out.surfaces.insert(
            *main,
            Surface {
                mesh: mesh.0.id(),
                keys: vec![SurfaceKey {
                    world: identity.worldspace_id,
                    grid: identity.grid,
                    quadrant: identity.quadrant,
                }],
                kind: if identity.tier.is_some() {
                    Kind::Native
                } else if rings.contains(main.id()) {
                    Kind::Ring
                } else {
                    Kind::Detail
                },
                protected: protected(main.id(), mesh.0.id(), surfaces, holds),
            },
        );
    }
    // Merged LAND is one draw, therefore all four exact quadrants must agree.
    for (entity, coverage, world) in merged {
        if !visibility.get(entity).is_ok_and(|visible| visible.get()) {
            continue;
        }
        let Ok(mesh) = meshes.get(entity) else {
            continue;
        };
        out.surfaces.insert(
            entity.into(),
            Surface {
                mesh: mesh.0.id(),
                keys: (0..4)
                    .map(|quadrant| SurfaceKey {
                        world: world.0,
                        grid: coverage.grid,
                        quadrant,
                    })
                    .collect(),
                kind: if rings.contains(entity) {
                    Kind::Ring
                } else {
                    Kind::Detail
                },
                protected: protected(entity, mesh.0.id(), surfaces, holds),
            },
        );
    }
    built.batch_entities.clear();
    for (root, chunk) in batches {
        for (entity, mesh, coverage, source) in chunk.render_surfaces() {
            built.batch_entities.insert(entity);
            if !visibility.get(entity).is_ok_and(|visible| visible.get()) {
                continue;
            }
            if !meshes
                .get(entity)
                .is_ok_and(|actual| actual.0.id() == mesh.id())
            {
                continue;
            }
            out.surfaces.insert(
                entity.into(),
                Surface {
                    mesh: mesh.id(),
                    keys: coverage
                        .into_iter()
                        .map(|key| SurfaceKey {
                            world: root.terrain_key().worldspace_id,
                            grid: key.grid,
                            quadrant: key.quadrant,
                        })
                        .collect(),
                    kind: if source { Kind::Source } else { Kind::Batch },
                    protected: protected(entity, mesh.id(), surfaces, holds),
                },
            );
        }
    }
}

fn domain(entity: Entity, key: RetainedViewEntity, light: Option<&LightEntity>) -> Domain {
    match light {
        None => Domain::Camera(entity),
        Some(LightEntity::Directional { light_entity, .. }) => Domain::Shadow(*light_entity, key),
        Some(LightEntity::Point {
            light_entity,
            face_index,
        }) => Domain::Shadow(
            *light_entity,
            RetainedViewEntity {
                auxiliary_entity: Entity::PLACEHOLDER.into(),
                subview_index: *face_index as u32,
                ..key
            },
        ),
        Some(LightEntity::Spot { light_entity }) => Domain::Shadow(
            *light_entity,
            RetainedViewEntity {
                auxiliary_entity: Entity::PLACEHOLDER.into(),
                subview_index: 0,
                ..key
            },
        ),
    }
}

/// A fallback whose whole coverage the current cluster plan draws. The plan already contains its
/// own exact compute, material, forward, prepass, shadow and far-binding qualification, so no
/// stock queue proof is needed to yield. Protected surfaces (outstanding own-draw obligations and
/// live named holds) never yield.
fn cluster_owned(surface: &Surface, cluster: &HashSet<(u32, IVec2)>) -> bool {
    matches!(surface.kind, Kind::Native | Kind::Batch | Kind::Source)
        && !surface.protected
        && !surface.keys.is_empty()
        && surface
            .keys
            .iter()
            .all(|key| cluster.contains(&(key.world, key.grid)))
}

/// Pure complement rule. `drawable` is the current view's prepared stock proof;
/// `cluster` is this invocation's exact selected plan, never asynchronous history.
fn hidden_for_view(
    terrain: &RenderTerrain,
    visible: &HashSet<MainEntity>,
    drawable: &HashSet<MainEntity>,
    cluster: &HashSet<(u32, IVec2)>,
    center: IVec2,
) -> bevy::platform::collections::HashSet<MainEntity> {
    let mut hidden = bevy::platform::collections::HashSet::default();
    let mut batch_keys = HashSet::new();
    for main in visible.intersection(drawable) {
        if let Some(surface) = terrain.surfaces.get(main)
            && surface.kind == Kind::Batch
        {
            batch_keys.extend(surface.keys.iter().copied());
        }
    }
    // First choose native/batch/source ownership. Only whole draws can yield;
    // partially replaced immutable batches stay and clip cluster terrain below.
    for main in visible {
        let Some(surface) = terrain.surfaces.get(main) else {
            continue;
        };
        let far = matches!(surface.kind, Kind::Native | Kind::Batch | Kind::Source);
        if far
            && !surface.protected
            && !surface.keys.is_empty()
            && surface.keys.iter().all(|key| {
                cluster.contains(&(key.world, key.grid))
                    || (surface.kind != Kind::Batch && batch_keys.contains(key))
            })
        {
            hidden.insert(*main);
        }
    }
    let native_keys: HashSet<_> = visible
        .intersection(drawable)
        .filter(|main| !hidden.contains(*main))
        .filter_map(|main| terrain.surfaces.get(main))
        .filter(|surface| matches!(surface.kind, Kind::Native | Kind::Batch | Kind::Source))
        .flat_map(|surface| surface.keys.iter().copied())
        .collect();
    for main in visible {
        let Some(surface) = terrain.surfaces.get(main) else {
            continue;
        };
        if surface.kind == Kind::Ring
            && !surface.protected
            && !terrain.explicit
            && !surface.keys.is_empty()
            && surface.keys.iter().all(|key| {
                super::lod::chebyshev_grid_distance(key.grid, center) > terrain.radius
                    && (native_keys.contains(key) || cluster.contains(&(key.world, key.grid)))
            })
        {
            hidden.insert(*main);
        }
    }
    hidden
}

/// Remove only after actual queue membership is available, before phase sorting
/// and batching. Phase removal is paired with Bevy's pending retry queues so
/// stable visible fallback is reconsidered next frame if replacement withdraws.
/// Do not remove after instance batches/GPU phase buffers have been prepared.
#[derive(SystemParam)]
struct TerrainPhases<'w> {
    opaque: Option<ResMut<'w, ViewBinnedRenderPhases<Opaque3d>>>,
    mask: Option<ResMut<'w, ViewBinnedRenderPhases<AlphaMask3d>>>,
    transmissive: Option<ResMut<'w, ViewSortedRenderPhases<Transmissive3d>>>,
    transparent: Option<ResMut<'w, ViewSortedRenderPhases<Transparent3d>>>,
    prepass: Option<ResMut<'w, ViewBinnedRenderPhases<Opaque3dPrepass>>>,
    mask_prepass: Option<ResMut<'w, ViewBinnedRenderPhases<AlphaMask3dPrepass>>>,
    shadow: Option<ResMut<'w, ViewBinnedRenderPhases<Shadow>>>,
    forward_retry: Option<ResMut<'w, PendingMeshMaterialQueues>>,
    prepass_retry: Option<ResMut<'w, PendingPrepassMeshMaterialQueues>>,
    shadow_retry: Option<ResMut<'w, PendingShadowQueues>>,
}

impl TerrainPhases<'_> {
    fn retry(&mut self, key: RetainedViewEntity, shadow: bool, pairs: &[(Entity, MainEntity)]) {
        // Bevy 0.19 iter_to_queue visits additions, dirty specializations and
        // pending retries; removing a bin alone does not revisit stable visible
        // entities. Keep suppressed draws in the existing retry lifecycle, so
        // stock queues reconsider them before next frame's complement decision.
        // PendingQueues swaps current_frame into prev_frame at specialization.
        if shadow {
            if let Some(queues) = self.shadow_retry.as_deref_mut() {
                queues
                    .entry(key)
                    .or_default()
                    .current_frame
                    .extend(pairs.iter().copied());
            }
        } else {
            if let Some(queues) = self.forward_retry.as_deref_mut() {
                queues
                    .entry(key)
                    .or_default()
                    .current_frame
                    .extend(pairs.iter().copied());
            }
            if let Some(queues) = self.prepass_retry.as_deref_mut() {
                queues
                    .entry(key)
                    .or_default()
                    .current_frame
                    .extend(pairs.iter().copied());
            }
        }
    }

    fn remove(
        &mut self,
        key: RetainedViewEntity,
        hidden: &bevy::platform::collections::HashSet<MainEntity>,
    ) {
        macro_rules! binned {
            ($field:ident) => {
                if let Some(phase) = self
                    .$field
                    .as_deref_mut()
                    .and_then(|phases| phases.get_mut(&key))
                {
                    for main in hidden {
                        phase.remove(*main);
                    }
                }
            };
        }
        binned!(opaque);
        binned!(mask);
        binned!(prepass);
        binned!(mask_prepass);
        binned!(shadow);
        if let Some(phase) = self
            .transmissive
            .as_deref_mut()
            .and_then(|phases| phases.get_mut(&key))
        {
            phase.items.retain(|(_, main), _| !hidden.contains(main));
        }
        if let Some(phase) = self
            .transparent
            .as_deref_mut()
            .and_then(|phases| phases.get_mut(&key))
        {
            phase.items.retain(|(_, main), _| !hidden.contains(main));
        }
    }
}

type TerrainViews<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ExtractedView,
        Option<&'static LightEntity>,
        Option<&'static mut RenderVisibleEntities>,
        Has<DepthPrepass>,
        Has<NormalPrepass>,
        Has<MotionVectorPrepass>,
    ),
>;

/// Hide cluster-owned fallbacks BEFORE Bevy queues meshes. `ClassFilter` reports a newly hidden
/// entity as removed (its bins are emptied once) and an entity that stops being hidden as added,
/// so a stable owned fallback costs no queue work per frame while its extracted instance and
/// allocations stay resident, and a withdrawn plan re-arms it in this same frame: the plan is
/// rebuilt before this system runs and mesh queueing runs after it.
/// The fallbacks one view's cluster plan owns, and the terrain revision and plan they were
/// derived from. Neither moves in a settled frame, so the set is kept rather than re-derived.
struct OwnedSurfaces {
    terrain: u64,
    plan: (u64, u64, u64),
    owned: bevy::platform::collections::HashSet<MainEntity>,
}

fn gate(
    terrain: Res<RenderTerrain>,
    clusters: Res<ClusterCoverage>,
    mut filters: ResMut<GateFilters>,
    mut views: TerrainViews,
    mut shadows: Query<&mut RenderShadowMapVisibleEntities>,
    mut owned_by_view: Local<HashMap<RetainedViewEntity, OwnedSurfaces>>,
) {
    let filters = &mut filters.0;
    assert!(filters.active.is_empty(), "terrain gate was not restored");
    owned_by_view.retain(|key, _| clusters.views.contains_key(key));
    let empty = bevy::platform::collections::HashSet::default();
    let mut wanted: HashMap<
        Domain,
        std::borrow::Cow<bevy::platform::collections::HashSet<MainEntity>>,
    > = HashMap::new();
    for (_, view, ..) in &views {
        let key = view.retained_view_entity;
        if let Some(coverage) = clusters.views.get(&key) {
            let plan = (coverage.generation, coverage.layout, coverage.plan);
            let stale = owned_by_view
                .get(&key)
                .is_none_or(|kept| kept.terrain != terrain.revision || kept.plan != plan);
            if stale {
                // Which entities the plan owns is a property of the surfaces alone; whether each is
                // in this view is left to the class filter, which looks each one up.
                let owned = if coverage.cells.is_empty() {
                    Default::default()
                } else {
                    terrain
                        .surfaces
                        .iter()
                        .filter(|(_, surface)| cluster_owned(surface, &coverage.cells))
                        .map(|(main, _)| *main)
                        .collect()
                };
                owned_by_view.insert(
                    key,
                    OwnedSurfaces {
                        terrain: terrain.revision,
                        plan,
                        owned,
                    },
                );
            }
        }
    }
    for (entity, view, light, ..) in &views {
        let key = view.retained_view_entity;
        let domain = domain(entity, key, light);
        let owned = owned_by_view.get(&key).map_or(&empty, |kept| &kept.owned);
        // Shared point/spot domains yield only after every consumer agrees.
        wanted
            .entry(domain)
            .and_modify(|joint| joint.to_mut().retain(|main| owned.contains(main)))
            .or_insert(std::borrow::Cow::Borrowed(owned));
    }
    filters
        .saved
        .retain(|domain, _| wanted.contains_key(domain));
    for (domain, hidden) in wanted {
        let class = match domain {
            Domain::Camera(entity) => views
                .get_mut(entity)
                .ok()
                .and_then(|(_, _, _, visible, ..)| visible)
                .map(|visible| visible.into_inner()),
            Domain::Shadow(light, key) => shadows
                .get_mut(light)
                .ok()
                .and_then(|shadow| shadow.into_inner().subviews.get_mut(&key)),
        }
        .and_then(|visible| visible.classes.get_mut(&std::any::TypeId::of::<Mesh3d>()));
        if let Some(class) = class {
            filters
                .saved
                .entry(domain)
                .or_default()
                .apply(class, &hidden);
            filters.active.push(domain);
        }
    }
}

fn filter(
    terrain: Res<RenderTerrain>,
    clusters: Res<ClusterCoverage>,
    stock: StockDrawable,
    mut filters: ResMut<Filters>,
    mut views: TerrainViews,
    mut shadows: Query<&mut RenderShadowMapVisibleEntities>,
    mut phases: ParamSet<(crate::swap_bridge::systems::QueuePhases, TerrainPhases)>,
) {
    assert!(
        filters.active.is_empty(),
        "terrain complement was not restored"
    );
    let mut wanted: HashMap<Domain, bevy::platform::collections::HashSet<MainEntity>> =
        HashMap::new();
    let mut consumers: HashMap<Domain, Vec<RetainedViewEntity>> = HashMap::new();
    {
        let queued = phases.p0();
        for (entity, view, light, visible, depth, normal, motion) in &views {
            let key = view.retained_view_entity;
            let domain = domain(entity, key, light);
            consumers.entry(domain).or_default().push(key);
            let visible = match domain {
                Domain::Camera(_) => visible,
                Domain::Shadow(light, key) => shadows
                    .get(light)
                    .ok()
                    .and_then(|shadow| shadow.subviews.get(&key)),
            };
            // Only terrain surfaces can be complemented, so only they are collected.
            let visible: HashSet<_> = visible
                .and_then(|visible| visible.get::<Mesh3d>())
                .into_iter()
                .flat_map(|class| class.iter_visible())
                .map(|(_, main)| *main)
                .filter(|main| terrain.surfaces.contains_key(main))
                .collect();
            let forward = queued.forward(&key);
            let prepass = queued.prepass(&key);
            let shadow = queued.shadow(&key);
            let drawable = visible
                .iter()
                .copied()
                .filter(|main| {
                    terrain
                        .surfaces
                        .get(main)
                        .is_some_and(|surface| stock.matches_mesh(*main, surface.mesh))
                        && if light.is_some() {
                            shadow.queued(*main)
                                && stock.drawable(*main, key, StockPassFamily::Shadow)
                        } else {
                            forward.queued(*main)
                                && stock.drawable(*main, key, StockPassFamily::Forward)
                                && (!(depth || normal || motion)
                                    || !stock.prepass_required(*main)
                                    || (prepass.queued(*main)
                                        && stock.drawable(*main, key, StockPassFamily::Prepass)))
                        }
                })
                .collect();
            let position = view.world_from_view.translation();
            let local = IVec2::new(
                (position.x / super::CELL_SIZE).floor() as i32,
                (-position.z / super::CELL_SIZE).floor() as i32,
            );
            let center = match (
                terrain.origin.x.checked_add(local.x),
                terrain.origin.y.checked_add(local.y),
            ) {
                (Some(x), Some(y)) => IVec2::new(x, y),
                _ => {
                    warn_once!("terrain handoff view center exceeds supported grid coordinates");
                    // An unrepresentable view cannot certify replacement. A shared
                    // shadow domain must keep its fallback for every consumer.
                    wanted.entry(domain).or_default().clear();
                    continue;
                }
            };
            let empty = HashSet::new();
            let cluster = clusters.views.get(&key).map_or(&empty, |coverage| {
                // Retained to make identity explicit in debugger/diagnostics. These
                // facts are produced anew after the current generation's queue walk.
                let _identity = (coverage.generation, coverage.layout, coverage.plan);
                &coverage.cells
            });
            let hidden = hidden_for_view(&terrain, &visible, &drawable, cluster, center);
            wanted
                .entry(domain)
                .and_modify(|joint| joint.retain(|main| hidden.contains(main)))
                .or_insert(hidden);
        }
    }
    filters
        .saved
        .retain(|domain, _| wanted.contains_key(domain));
    for (domain, hidden) in wanted {
        // Shared point/spot domains yield only after every consumer agrees.
        for key in &consumers[&domain] {
            phases.p1().remove(*key, &hidden);
        }
        let class = match domain {
            Domain::Camera(entity) => views
                .get_mut(entity)
                .ok()
                .and_then(|(_, _, _, visible, _, _, _)| visible)
                .map(|visible| visible.into_inner()),
            Domain::Shadow(light, key) => shadows
                .get_mut(light)
                .ok()
                .and_then(|shadow| shadow.into_inner().subviews.get_mut(&key)),
        }
        .and_then(|visible| visible.classes.get_mut(&std::any::TypeId::of::<Mesh3d>()));
        if let Some(class) = class {
            let pairs = crate::cluster::stock_visibility::visible_pairs(class, &hidden);
            for key in &consumers[&domain] {
                phases
                    .p1()
                    .retry(*key, matches!(domain, Domain::Shadow(..)), &pairs);
            }
            filters
                .saved
                .entry(domain)
                .or_default()
                .apply(class, &hidden);
            filters.active.push(domain);
        }
    }
}

fn restore(
    mut filters: ResMut<Filters>,
    mut gate: ResMut<GateFilters>,
    mut views: Query<&mut RenderVisibleEntities>,
    mut shadows: Query<&mut RenderShadowMapVisibleEntities>,
) {
    // Reverse of application order: the post-queue filter, then the pre-queue gate.
    restore_filters(&mut filters, &mut views, &mut shadows);
    restore_filters(&mut gate.0, &mut views, &mut shadows);
}

fn restore_filters(
    filters: &mut Filters,
    views: &mut Query<&mut RenderVisibleEntities>,
    shadows: &mut Query<&mut RenderShadowMapVisibleEntities>,
) {
    for domain in std::mem::take(&mut filters.active) {
        let class = match domain {
            Domain::Camera(entity) => views
                .get_mut(entity)
                .ok()
                .map(|visible| visible.into_inner()),
            Domain::Shadow(light, key) => shadows
                .get_mut(light)
                .ok()
                .and_then(|shadow| shadow.into_inner().subviews.get_mut(&key)),
        }
        .and_then(|visible| visible.classes.get_mut(&std::any::TypeId::of::<Mesh3d>()));
        if let Some(class) = class {
            filters
                .saved
                .get_mut(&domain)
                .expect("active terrain filter")
                .restore(class);
        } else {
            filters.saved.remove(&domain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::AssetPlugin, camera::visibility::VisibilityPlugin, mesh::MeshPlugin,
        transform::TransformPlugin,
    };

    fn scheduled_systems(
        world: &mut World,
        label: impl bevy::ecs::schedule::ScheduleLabel + Clone,
    ) -> Vec<std::any::TypeId> {
        if !world.resource::<Schedules>().contains(label.clone()) {
            return Vec::new();
        }
        // Extract parameters read the main world while a schedule is initialized.
        if !world.contains_resource::<MainWorld>() {
            world.insert_resource(MainWorld::default());
        }
        world.schedule_scope(label, |world, schedule| {
            schedule.initialize(world).unwrap();
            schedule
                .systems()
                .unwrap()
                .map(|(_, system)| bevy::ecs::system::System::system_type(&**system))
                .collect()
        })
    }

    fn identity<M>(system: impl IntoSystem<(), (), M>) -> std::any::TypeId {
        bevy::ecs::system::System::system_type(&IntoSystem::into_system(system))
    }

    #[test]
    fn disabled_cluster_terrain_installs_no_handoff_systems_even_when_profiling() {
        for clusters in [None, Some(String::new())] {
            for profiling in [false, true] {
                let mut app = App::new();
                app.insert_resource(crate::config::EngineConfig {
                    clusters: clusters.clone(),
                    cluster_terrain: false,
                    profile_output_dir: profiling.then(|| std::path::PathBuf::from("profile-test")),
                    ..default()
                });
                let mut render = bevy::app::SubApp::new();
                render.add_schedule(Render::base_schedule());
                app.insert_sub_app(RenderApp, render);
                app.add_plugins(TerrainHandoffPlugin);
                let world = app.get_sub_app_mut(RenderApp).unwrap().world_mut();
                let extracted = scheduled_systems(world, ExtractSchedule);
                let rendered = scheduled_systems(world, Render);
                assert!(!extracted.contains(&identity(extract)));
                assert!(!rendered.contains(&identity(gate)));
                assert!(!rendered.contains(&identity(filter)));
                assert!(!rendered.contains(&identity(restore)));
                assert!(!world.contains_resource::<RenderTerrain>());
                assert!(!world.contains_resource::<Filters>());
                assert_eq!(
                    extracted.contains(&identity(extract_lod_inventory)),
                    profiling
                );
                assert_eq!(rendered.contains(&identity(collect_counts)), profiling);
            }
        }
        // A stray terrain flag cannot activate the option without a cluster package.
        let mut app = App::new();
        app.insert_resource(crate::config::EngineConfig {
            cluster_terrain: true,
            ..default()
        });
        let mut render = bevy::app::SubApp::new();
        render.add_schedule(Render::base_schedule());
        app.insert_sub_app(RenderApp, render);
        app.add_plugins(TerrainHandoffPlugin);
        let world = app.get_sub_app_mut(RenderApp).unwrap().world_mut();
        assert!(!scheduled_systems(world, ExtractSchedule).contains(&identity(extract)));
        assert!(!scheduled_systems(world, Render).contains(&identity(filter)));
    }

    #[test]
    fn enabled_cluster_terrain_installs_the_handoff_systems() {
        let mut app = App::new();
        app.insert_resource(crate::config::EngineConfig {
            clusters: Some(String::new()),
            cluster_terrain: true,
            ..default()
        });
        let mut render = bevy::app::SubApp::new();
        render.add_schedule(Render::base_schedule());
        app.insert_sub_app(RenderApp, render);
        app.add_plugins(TerrainHandoffPlugin);
        let world = app.get_sub_app_mut(RenderApp).unwrap().world_mut();
        assert!(scheduled_systems(world, ExtractSchedule).contains(&identity(extract)));
        let rendered = scheduled_systems(world, Render);
        assert!(rendered.contains(&identity(gate)));
        assert!(rendered.contains(&identity(filter)));
        assert!(rendered.contains(&identity(restore)));
        assert!(world.contains_resource::<RenderTerrain>());
        assert!(world.contains_resource::<Filters>());
        assert!(!world.contains_resource::<LodDrawInventory>());
    }

    #[test]
    fn profiling_inventory_visits_only_immediate_lod_children() {
        let mut world = World::new();
        for _ in 0..512 {
            world.spawn((Mesh3d(Handle::default()), InheritedVisibility::VISIBLE));
        }
        let near = world
            .spawn(super::super::TerrainCoverage {
                grid: IVec2::ZERO,
                quadrant: 0,
                tier: None,
            })
            .id();
        for _ in 0..64 {
            world.spawn((
                Mesh3d(Handle::default()),
                InheritedVisibility::VISIBLE,
                ChildOf(near),
            ));
        }
        let native = world
            .spawn(super::super::TerrainCoverage {
                grid: IVec2::ZERO,
                quadrant: 0,
                tier: Some(shared::lod::LodTier::Tier4),
            })
            .id();
        let primitive = world
            .spawn((
                Mesh3d(Handle::default()),
                InheritedVisibility::VISIBLE,
                ChildOf(native),
            ))
            .id();
        world.spawn((
            Mesh3d(Handle::default()),
            InheritedVisibility::HIDDEN,
            ChildOf(native),
        ));
        let mut state = bevy::ecs::system::SystemState::<LodDiagnosticInputs>::new(&mut world);
        let mut inventory = LodDrawInventory::default();
        inventory.collect(&state.get(&world).unwrap());
        assert_eq!(inventory.child_visits, 2);
        assert_eq!(
            inventory.resident.len(),
            2,
            "inherited-hidden meshes remain resident"
        );
        assert_eq!(inventory.meshes.len(), 1);
        assert!(inventory.meshes.contains_key(&primitive.into()));
        assert!(!world.contains_resource::<RenderTerrain>());
    }

    #[test]
    fn terrain_queue_counts_reach_the_benchmark_metrics() {
        let mut main = MainWorld::default();
        main.init_resource::<super::super::StreamingMetrics>();
        main.init_resource::<crate::profiling::ProfilingState>();
        let mut app = App::new();
        app.insert_resource(main)
            .insert_resource(TerrainDrawCounts {
                sampled: true,
                resident_lod: 9,
                extracted_lod: 7,
                queued_forward: 3,
                queued_prepass: 2,
                queued_shadow: 11,
            })
            .add_systems(Update, publish_counts);
        app.update();
        let metrics = app
            .world()
            .resource::<MainWorld>()
            .resource::<super::super::StreamingMetrics>();
        let report = serde_json::to_value(metrics).unwrap();
        assert_eq!(report["terrain_lod_counts_measured"], true);
        assert_eq!(report["extracted_terrain_lod_items"], 7);
        assert_eq!(report["queued_terrain_lod_forward_items"], 3);
        assert_eq!(report["queued_terrain_lod_prepass_items"], 2);
        assert_eq!(report["queued_terrain_lod_shadow_items"], 11);
        let main = app.world().resource::<MainWorld>();
        let profiler = main.resource::<crate::profiling::ProfilingState>();
        let directory = tempfile::tempdir().unwrap();
        let config = crate::config::EngineConfig {
            profile_output_dir: Some(directory.path().to_owned()),
            ..default()
        };
        profiler
            .write_bundle(
                &config,
                &serde_json::json!({}),
                Some(metrics),
                &crate::render::RendererMetrics::default(),
                None,
                None,
            )
            .unwrap();
        let cpu_spans: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.path().join("cpu-spans.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            cpu_spans["gauges"]["lod/resident_terrain_mesh_entities"],
            9.0
        );
        assert_eq!(cpu_spans["gauges"]["lod/extracted_terrain_items"], 7.0);
    }

    fn main(id: u32) -> MainEntity {
        Entity::from_raw_u32(id).unwrap().into()
    }
    fn key(world: u32, quadrant: u8) -> SurfaceKey {
        SurfaceKey {
            world,
            grid: IVec2::new(-3, 0),
            quadrant,
        }
    }
    fn surface(kind: Kind, keys: Vec<SurfaceKey>) -> Surface {
        Surface {
            keys,
            kind,
            mesh: Handle::<Mesh>::default().id(),
            protected: false,
        }
    }
    fn terrain() -> RenderTerrain {
        RenderTerrain {
            radius: 1,
            surfaces: HashMap::from([
                (main(1), surface(Kind::Ring, vec![key(7, 0)])),
                (main(2), surface(Kind::Native, vec![key(7, 0)])),
            ]),
            ..default()
        }
    }

    fn gate_world(cells: &[(u32, IVec2)]) -> (World, Entity, RetainedViewEntity) {
        use bevy::render::view::{ColorGrading, RenderVisibleEntitiesClass};
        let mut world = World::new();
        let covered = key(7, 0);
        let other = SurfaceKey {
            grid: IVec2::new(9, 9),
            ..covered
        };
        let mut protected = surface(Kind::Source, vec![covered]);
        protected.protected = true;
        world.insert_resource(RenderTerrain {
            surfaces: HashMap::from([
                // Owned: every cell in the plan.
                (main(1), surface(Kind::Source, vec![covered])),
                (main(2), surface(Kind::Batch, vec![covered])),
                // Never yield: ring, detail, protected, partly covered, uncovered.
                (main(3), surface(Kind::Ring, vec![covered])),
                (main(4), surface(Kind::Detail, vec![covered])),
                (main(5), protected),
                (main(6), surface(Kind::Source, vec![covered, other])),
                (main(7), surface(Kind::Source, vec![other])),
            ]),
            ..default()
        });
        let view = RetainedViewEntity::new(main(50), None, 0);
        let mut coverage = ClusterCoverage::default();
        if !cells.is_empty() {
            coverage.views.insert(
                view,
                ClusterViewCoverage {
                    generation: 1,
                    layout: 1,
                    plan: 1,
                    cells: cells.iter().copied().collect(),
                },
            );
        }
        world.insert_resource(coverage);
        world.init_resource::<GateFilters>();
        let mut visible = RenderVisibleEntities::default();
        visible.classes.insert(
            std::any::TypeId::of::<Mesh3d>(),
            RenderVisibleEntitiesClass {
                // Sorted by main entity, as Bevy's extraction hands it over.
                entities_cpu_culling: {
                    let mut pairs: Vec<_> = (1..=7)
                        .map(|id| (Entity::from_raw_u32(100 + id).unwrap(), main(id)))
                        .collect();
                    pairs.sort_by_key(|(_, main)| *main);
                    pairs
                },
                ..default()
            },
        );
        let camera = world
            .spawn((
                ExtractedView {
                    retained_view_entity: view,
                    clip_from_view: Mat4::IDENTITY,
                    world_from_view: GlobalTransform::IDENTITY,
                    clip_from_world: None,
                    target_format: bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                    viewport: UVec4::new(0, 0, 8, 8),
                    color_grading: ColorGrading::default(),
                    invert_culling: false,
                },
                visible,
            ))
            .id();
        (world, camera, view)
    }

    fn gate_class(
        world: &World,
        camera: Entity,
    ) -> &bevy::render::view::RenderVisibleEntitiesClass {
        world
            .get::<RenderVisibleEntities>(camera)
            .unwrap()
            .get::<Mesh3d>()
            .unwrap()
    }

    fn gate_frame(world: &mut World, camera: Entity, plan: &[(u32, IVec2)]) {
        use bevy::ecs::system::RunSystemOnce;
        // Render's next frame starts from the raw visible set with empty deltas.
        let mut visible = world.get_mut::<RenderVisibleEntities>(camera).unwrap();
        let class = visible
            .classes
            .get_mut(&std::any::TypeId::of::<Mesh3d>())
            .unwrap();
        class.added_entities.clear();
        class.removed_entities.clear();
        let view = world
            .get::<ExtractedView>(camera)
            .unwrap()
            .retained_view_entity;
        let mut coverage = world.resource_mut::<ClusterCoverage>();
        coverage.views.clear();
        if !plan.is_empty() {
            coverage.views.insert(
                view,
                ClusterViewCoverage {
                    generation: 1,
                    layout: 1,
                    plan: 1,
                    cells: plan.iter().copied().collect(),
                },
            );
        }
        world.run_system_once(gate).unwrap();
    }

    fn gate_restore(world: &mut World) {
        use bevy::ecs::system::RunSystemOnce;
        world.init_resource::<Filters>();
        world.run_system_once(restore).unwrap();
    }

    fn delta_ids(pairs: &[(Entity, MainEntity)]) -> Vec<u32> {
        let mut ids: Vec<_> = pairs
            .iter()
            .map(|(_, main)| main.id().index_u32())
            .collect();
        ids.sort_unstable();
        ids
    }

    fn gate_visible(world: &World, camera: Entity) -> Vec<u32> {
        let mut ids: Vec<_> = gate_class(world, camera)
            .iter_visible()
            .map(|(_, main)| main.id().index_u32())
            .collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn cluster_owned_fallbacks_leave_queueing_once_and_return_in_the_withdrawal_frame() {
        let covered = key(7, 0);
        let plan = [(covered.world, covered.grid)];
        let (mut world, camera, _) = gate_world(&plan);
        // Frame 1: the plan covers entities 1 and 2 only. Each is dequeued exactly once.
        gate_frame(&mut world, camera, &plan);
        assert_eq!(gate_visible(&world, camera), vec![3, 4, 5, 6, 7]);
        let class = gate_class(&world, camera);
        assert!(class.added_entities.is_empty());
        assert_eq!(delta_ids(&class.removed_entities), vec![1, 2]);
        gate_restore(&mut world);
        assert_eq!(gate_visible(&world, camera), vec![1, 2, 3, 4, 5, 6, 7]);
        // Frames 2..N: an unchanged plan produces no queue delta at all, so Bevy's retained
        // queues do no specialization or queue work for the owned fallbacks.
        for _ in 0..5 {
            gate_frame(&mut world, camera, &plan);
            assert_eq!(gate_visible(&world, camera), vec![3, 4, 5, 6, 7]);
            let class = gate_class(&world, camera);
            assert!(class.added_entities.is_empty() && class.removed_entities.is_empty());
            gate_restore(&mut world);
        }
        // The plan withdraws: both fallbacks are visible and ADDED in this very frame, which
        // is what makes mesh queueing, running after the gate, draw them again.
        gate_frame(&mut world, camera, &[]);
        assert_eq!(gate_visible(&world, camera), vec![1, 2, 3, 4, 5, 6, 7]);
        let class = gate_class(&world, camera);
        assert_eq!(delta_ids(&class.added_entities), vec![1, 2]);
        assert!(class.removed_entities.is_empty());
        gate_restore(&mut world);
        // And a withdrawn-then-restored plan is a single further dequeue, not per-frame churn.
        gate_frame(&mut world, camera, &plan);
        assert_eq!(gate_class(&world, camera).removed_entities.len(), 2);
        gate_restore(&mut world);
        gate_frame(&mut world, camera, &plan);
        assert!(gate_class(&world, camera).removed_entities.is_empty());
        gate_restore(&mut world);
    }

    #[test]
    fn cluster_gate_never_hides_rings_detail_protected_or_partly_covered_fallbacks() {
        let covered = key(7, 0);
        let other = SurfaceKey {
            grid: IVec2::new(9, 9),
            ..covered
        };
        let terrain = gate_world(&[])
            .0
            .remove_resource::<RenderTerrain>()
            .unwrap();
        let cells: HashSet<_> = [(covered.world, covered.grid)].into_iter().collect();
        let owned: Vec<_> = (1..=7)
            .filter(|id| cluster_owned(&terrain.surfaces[&main(*id)], &cells))
            .collect();
        assert_eq!(owned, vec![1, 2]);
        let both: HashSet<_> = cells
            .iter()
            .copied()
            .chain([(other.world, other.grid)])
            .collect();
        let owned: Vec<_> = (1..=7)
            .filter(|id| cluster_owned(&terrain.surfaces[&main(*id)], &both))
            .collect();
        // Protected, ring and detail surfaces still never yield; the partly covered source does.
        assert_eq!(owned, vec![1, 2, 6, 7]);
        assert!(!cluster_owned(&surface(Kind::Source, Vec::new()), &both));
    }

    #[test]
    fn current_view_phase_removal_keeps_other_view_and_requeues_withdrawn_fallback() {
        use bevy::{
            core_pipeline::core_3d::{Opaque3dBatchSetKey, Opaque3dBinKey},
            ecs::system::RunSystemOnce,
            render::{
                batching::gpu_preprocessing::GpuPreprocessingMode,
                mesh::allocator::MeshSlabs,
                render_phase::{BinnedRenderPhaseType, DrawFunctionId, InputUniformIndex},
                render_resource::CachedRenderPipelineId,
            },
        };
        let mut world = World::new();
        world.init_resource::<ViewBinnedRenderPhases<Opaque3d>>();
        let main_view = RetainedViewEntity::new(main(10), None, 0);
        let secondary_view = RetainedViewEntity::new(main(20), None, 0);
        let batch = Opaque3dBatchSetKey {
            pipeline: CachedRenderPipelineId::INVALID,
            draw_function: DrawFunctionId(0),
            material_bind_group_index: None,
            slabs: MeshSlabs::default(),
            lightmap_slab: None,
        };
        let bin = Opaque3dBinKey {
            asset_id: Handle::<Mesh>::default().id().untyped(),
        };
        {
            let mut phases = world.resource_mut::<ViewBinnedRenderPhases<Opaque3d>>();
            for view in [main_view, secondary_view] {
                phases.prepare_for_new_frame(view, GpuPreprocessingMode::None);
                phases.get_mut(&view).unwrap().add(
                    batch.clone(),
                    bin.clone(),
                    (Entity::from_raw_u32(101).unwrap(), main(1)),
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
            }
        }
        world
            .run_system_once(move |mut phases: TerrainPhases| {
                phases.remove(
                    secondary_view,
                    &HashSet::from([main(1)]).into_iter().collect(),
                );
            })
            .unwrap();
        let check = move |phases: crate::swap_bridge::systems::QueuePhases| {
            (
                phases.forward(&main_view).queued(main(1)),
                phases.forward(&secondary_view).queued(main(1)),
            )
        };
        assert_eq!(world.run_system_once(check).unwrap(), (true, false));
        // The next stock queue can add the same still-extracted fallback to a
        // retained phase whose previous current-plan removal invalidated its entry.
        world
            .resource_mut::<ViewBinnedRenderPhases<Opaque3d>>()
            .get_mut(&secondary_view)
            .unwrap()
            .add(
                batch,
                bin,
                (Entity::from_raw_u32(101).unwrap(), main(1)),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        assert_eq!(world.run_system_once(check).unwrap(), (true, true));
    }

    #[test]
    fn removed_stable_stock_is_revisited_by_real_bevy_retry_iterators() {
        use bevy::{
            ecs::system::RunSystemOnce,
            render::{
                camera::{DirtySpecializations, PendingQueues},
                view::RenderVisibleEntitiesClass,
            },
        };
        let mut world = World::new();
        world.init_resource::<PendingMeshMaterialQueues>();
        world.init_resource::<PendingPrepassMeshMaterialQueues>();
        world.init_resource::<PendingShadowQueues>();
        let key = RetainedViewEntity::new(main(10), None, 0);
        let pair = (Entity::from_raw_u32(100).unwrap(), main(1));
        world
            .run_system_once(move |mut phases: TerrainPhases| {
                phases.retry(key, false, &[pair]);
                phases.retry(key, true, &[pair]);
            })
            .unwrap();
        // No newly-visible or dirty event exists. This is precisely the dormant
        // stable fallback that a bin removal alone cannot restore.
        let visible = RenderVisibleEntitiesClass {
            entities_cpu_culling: vec![pair],
            ..default()
        };
        let dirty = DirtySpecializations::default();
        assert_eq!(dirty.iter_to_queue(key, &visible, &default()).count(), 0);
        fn check(
            queues: &mut PendingQueues,
            key: RetainedViewEntity,
            visible: &RenderVisibleEntitiesClass,
            pair: (Entity, MainEntity),
        ) {
            let retry = queues.prepare_for_new_frame(key);
            let dirty = DirtySpecializations::default();
            let queued: Vec<_> = dirty
                .iter_to_queue(key, visible, &retry.prev_frame)
                .map(|(render, main)| (*render, *main))
                .collect();
            assert_eq!(queued, vec![pair]);
        }
        check(
            &mut world.resource_mut::<PendingMeshMaterialQueues>().0,
            key,
            &visible,
            pair,
        );
        check(
            &mut world.resource_mut::<PendingPrepassMeshMaterialQueues>().0,
            key,
            &visible,
            pair,
        );
        check(
            &mut world.resource_mut::<PendingShadowQueues>().0,
            key,
            &visible,
            pair,
        );
    }

    #[test]
    fn main_and_secondary_views_have_independent_current_queued_replacement_decisions() {
        let terrain = terrain();
        let visible = HashSet::from([main(1), main(2)]);
        let empty = HashSet::new();
        let secondary = hidden_for_view(
            &terrain,
            &visible,
            &HashSet::from([main(2)]),
            &empty,
            IVec2::ZERO,
        );
        let main_view = hidden_for_view(
            &terrain,
            &visible,
            &HashSet::new(),
            &HashSet::new(),
            IVec2::ZERO,
        );
        assert!(secondary.contains(&main(1)));
        assert!(
            !main_view.contains(&main(1)),
            "secondary specialization/queue cannot hide main fallback"
        );
        assert!(
            !hidden_for_view(
                &terrain,
                &visible,
                &HashSet::new(),
                &HashSet::new(),
                IVec2::ZERO
            )
            .contains(&main(1)),
            "withdrawal after a settled frame restores fallback in this decision"
        );
    }

    #[test]
    fn cluster_plan_withdrawal_and_world_identity_keep_resident_ring() {
        let terrain = terrain();
        let visible = HashSet::from([main(1)]);
        let empty = HashSet::new();
        assert!(
            hidden_for_view(
                &terrain,
                &visible,
                &empty,
                &HashSet::from([(7, key(7, 0).grid)]),
                IVec2::ZERO
            )
            .contains(&main(1))
        );
        assert!(
            !hidden_for_view(
                &terrain,
                &visible,
                &empty,
                &HashSet::from([(8, key(7, 0).grid)]),
                IVec2::ZERO
            )
            .contains(&main(1))
        );
        assert!(
            !hidden_for_view(
                &terrain,
                &visible,
                &HashSet::new(),
                &HashSet::new(),
                IVec2::ZERO
            )
            .contains(&main(1)),
            "missing current generation/plan/material binding is not historical coverage"
        );
    }

    #[test]
    fn merged_ring_requires_four_current_quadrants_and_explicit_ring_stays() {
        let mut terrain = terrain();
        terrain.surfaces.get_mut(&main(1)).unwrap().keys = (0..4).map(|q| key(7, q)).collect();
        let mut visible = HashSet::from([main(1)]);
        let mut drawable = HashSet::new();
        for quadrant in 0..4 {
            let source = main(10 + u32::from(quadrant));
            terrain
                .surfaces
                .insert(source, surface(Kind::Native, vec![key(7, quadrant)]));
            visible.insert(source);
            drawable.insert(source);
            assert_eq!(
                hidden_for_view(&terrain, &visible, &drawable, &HashSet::new(), IVec2::ZERO)
                    .contains(&main(1)),
                quadrant == 3
            );
        }
        terrain.explicit = true;
        assert!(
            !hidden_for_view(&terrain, &visible, &drawable, &HashSet::new(), IVec2::ZERO)
                .contains(&main(1))
        );
    }

    #[test]
    fn clusters_off_batch_source_is_complementary_and_restores_on_withdrawal() {
        let mut terrain = terrain();
        terrain
            .surfaces
            .insert(main(3), surface(Kind::Batch, vec![key(7, 0)]));
        terrain
            .surfaces
            .insert(main(4), surface(Kind::Source, vec![key(7, 0)]));
        let visible = HashSet::from([main(1), main(2), main(3), main(4)]);
        let empty = HashSet::new();
        let ready = hidden_for_view(
            &terrain,
            &visible,
            &HashSet::from([main(3), main(4)]),
            &empty,
            IVec2::ZERO,
        );
        assert!(ready.contains(&main(4)) && ready.contains(&main(2)) && ready.contains(&main(1)));
        let withdrawn = hidden_for_view(
            &terrain,
            &visible,
            &HashSet::from([main(4)]),
            &empty,
            IVec2::ZERO,
        );
        assert!(!withdrawn.contains(&main(4)));
        assert!(
            withdrawn.contains(&main(1)),
            "still-drawable exact source covers ring"
        );
        let absent = hidden_for_view(
            &terrain,
            &visible,
            &HashSet::new(),
            &HashSet::new(),
            IVec2::ZERO,
        );
        assert!(!absent.contains(&main(1)) && !absent.contains(&main(4)));
    }

    #[test]
    fn live_named_downgrade_hold_pins_merged_and_quadrant_ring_until_own_draw() {
        use crate::{
            profiling::ProfilingState,
            streaming::StreamingMetrics,
            swap_bridge::{SwapBridgePlugin, SwapDrawn, SwapWaiting},
        };
        for merged in [false, true] {
            let mut app = App::new();
            app.init_resource::<ProfilingState>()
                .init_resource::<StreamingMetrics>()
                .add_plugins(SwapBridgePlugin);
            let old = app.world_mut().spawn(SwapStandIn::default()).id();
            let replacement = app
                .world_mut()
                .spawn((
                    SwapSurface::new(Handle::<StandardMaterial>::default().id().untyped(), [])
                        .with_stand_in(old),
                    SwapWaiting,
                ))
                .id();
            let mut terrain = terrain();
            terrain.surfaces.get_mut(&main(1)).unwrap().keys = if merged {
                (0..4).map(|q| key(7, q)).collect()
            } else {
                vec![key(7, 0)]
            };
            let visible = HashSet::from([main(1)]);
            let cluster = HashSet::from([(7, key(7, 0).grid)]);
            app.update();
            let mut state = bevy::ecs::system::SystemState::<(
                SurfaceObligations,
                Query<(), With<SwapStandIn>>,
            )>::new(app.world_mut());
            let (surfaces, holds) = state.get(app.world()).unwrap();
            terrain.surfaces.get_mut(&main(1)).unwrap().protected = protected(
                replacement,
                Handle::<Mesh>::default().id(),
                &surfaces,
                &holds,
            );
            assert!(
                !hidden_for_view(&terrain, &visible, &HashSet::new(), &cluster, IVec2::ZERO)
                    .contains(&main(1))
            );
            // Synthetic bridge completion tests the hold protocol, never GPU coverage.
            app.world_mut().entity_mut(replacement).insert(SwapDrawn);
            app.update();
            assert!(app.world().get_entity(old).is_err());
            terrain.surfaces.get_mut(&main(1)).unwrap().protected = false;
            assert!(
                hidden_for_view(&terrain, &visible, &HashSet::new(), &cluster, IVec2::ZERO)
                    .contains(&main(1))
            );
        }
    }

    #[test]
    fn journal_settled_in_place_root_primitive_and_intermediate_visibility_propagate() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            MeshPlugin,
            TransformPlugin,
            VisibilityPlugin,
        ));
        super::super::identity_changes::install_terrain(app.world_mut());
        app.finish();
        app.cleanup();
        let root = app
            .world_mut()
            .spawn((Transform::IDENTITY, Visibility::Inherited))
            .id();
        let intermediate = app
            .world_mut()
            .spawn((Transform::IDENTITY, Visibility::Inherited, ChildOf(root)))
            .id();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Mesh::from(Cuboid::default()));
        let primitive = app
            .world_mut()
            .spawn((
                Mesh3d(mesh),
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(intermediate),
            ))
            .id();
        app.update();
        app.update(); // settle before in-place changes; no structural insert follows
        for node in [root, primitive, intermediate] {
            *app.world_mut().get_mut::<Visibility>(node).unwrap() = Visibility::Hidden;
            app.update();
            assert!(
                !app.world()
                    .get::<InheritedVisibility>(primitive)
                    .unwrap()
                    .get()
            );
            *app.world_mut().get_mut::<Visibility>(node).unwrap() = Visibility::Inherited;
            app.update();
            assert!(
                app.world()
                    .get::<InheritedVisibility>(primitive)
                    .unwrap()
                    .get()
            );
        }
    }
}
