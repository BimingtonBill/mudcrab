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
        PendingShadowQueues, Shadow, Transmissive3d,
    },
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
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

pub(super) struct TerrainHandoffPlugin;
impl Plugin for TerrainHandoffPlugin {
    fn build(&self, app: &mut App) {
        // Cluster package/terrain are startup options, not mutable graphics-menu controls.
        // Install no terrain complement systems when their owner is absent.
        let enabled = app
            .world()
            .get_resource::<crate::config::EngineConfig>()
            .is_some_and(|config| config.clusters.is_some() && config.cluster_terrain);
        if !enabled {
            return;
        }
        let Some(render) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render
            .init_resource::<RenderTerrain>()
            .init_resource::<ClusterCoverage>()
            .init_resource::<Filters>()
            .add_systems(ExtractSchedule, extract)
            .add_systems(
                Render,
                filter
                    .after(RenderSystems::Queue)
                    .in_set(TerrainHandoffSystems::Filter)
                    .after(super::native_lod::NativeLodSystems::Filter)
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

fn extract(
    mut out: ResMut<RenderTerrain>,
    mut identities: Extract<TerrainIdentityAccess>,
    inputs: Extract<TerrainInputs>,
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
    out.surfaces.clear();
    out.origin = origin.0;
    out.radius = config.stream_radius;
    out.explicit = config.terrain_radius_given;
    // Reuse the journaled exact identity index, including its ancestry validation.
    // InheritedVisibility is refreshed by Bevy for primitive/root/intermediate
    // in-place changes; it is read every extraction, without another ancestry cache.
    for (main, identity) in identities.identities_shared().iter() {
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
    for (root, chunk) in batches {
        for (entity, mesh, coverage, source) in chunk.render_surfaces() {
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
            let visible: HashSet<_> = visible
                .and_then(|visible| visible.get::<Mesh3d>())
                .into_iter()
                .flat_map(|class| class.iter_visible())
                .map(|(_, main)| *main)
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
            let pairs: Vec<_> = class
                .iter_visible()
                .filter(|(_, main)| hidden.contains(*main))
                .map(|(entity, main)| (*entity, *main))
                .collect();
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
    mut views: Query<&mut RenderVisibleEntities>,
    mut shadows: Query<&mut RenderShadowMapVisibleEntities>,
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
