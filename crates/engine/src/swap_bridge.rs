//! The render→main "prepared and drawable" bridge (design-1091, slice C1 of #1055).
//!
//! The engine hides a stand-in (the LOD tile under a full-detail cell, the root a detail change
//! replaces) on a purely main-world readiness flag: `TerrainSurfaceReady` says the textures are in
//! `Assets<Image>`, not that the mesh, its images, the material's bind group and its pipeline exist
//! in the render world. The frame that flag appears the stand-in goes and the replacement may not
//! be drawn - which is the hole. This module makes "drawable" a fact the main world can read.
//!
//! The direction is the sanctioned one: the render world's `ExtractSchedule` reads the entities the
//! engine has marked and writes the outcome back into the main world, exactly as Bevy's own
//! render-asset diagnostics publish their counters (`bevy_render::diagnostic`) and as this engine's
//! [`crate::render::RendererProofBridge`] and [`crate::profiling::RenderMeasurementProbe`] do.
//!
//! This slice writes the markers and records the gauges that make the gap visible; the one place a
//! stand-in is despawned on the drawn signal is slice C3's detail-change hand-over
//! ([`release_detail_change_stand_ins`] below). Nothing here hides, delays or settles anything;
//! slices C2 (the LOD swap, the unload side) and C4 (the settles) are what key on the rest of the
//! signal.
//!
//! The bridge visits waiting surfaces and the descendants of live holds or explicit coverage requests,
//! never all terrain. A surface leaves the waiting set the frame it is confirmed drawn
//! (design-1091 §2.3, §5).

pub mod assets;
pub mod destination;
pub mod systems;

use bevy::pbr::{
    MaterialBindGroupAllocators, PreparedMaterial, RenderMaterialInstances, RenderMeshInstances,
    SpecializedMaterialPipelineCache,
};
use bevy::{
    asset::{AssetId, UntypedAssetId},
    ecs::system::SystemState,
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, RenderApp,
        erased_render_asset::ErasedRenderAssets,
        mesh::{RenderMesh, allocator::MeshAllocator},
        render_asset::RenderAssets,
        render_resource::{CachedPipelineState, PipelineCache},
        sync_world::MainEntity,
        texture::GpuImage,
        view::{ExtractedView, RenderVisibleEntities, RenderVisibleEntitiesClass},
    },
};

use crate::{
    config::EngineConfig,
    profiling::ProfilingState,
    streaming::StreamingMetrics,
    world::components::{CellRef, ExteriorCellGrid},
};

/// A surface that is waiting for the render world: the engine has spawned it, the stand-in that
/// covers it is still drawn, and nothing yet knows whether it can be drawn.
///
/// This is a bare marker so the settle and the door prewarm can read one query over it. An entity
/// that carries it must also carry its [`SwapSurface`] declaration and a [`Mesh3d`]: the bridge's
/// query is the marker joined with those, and a marked entity without them is never visited.
///
/// The main-world hand-over removes the marker (and only the marker) the frame the render world's
/// confirmation arrives; a surface whose entity is despawned - a cell leaving the stream window -
/// leaves the set with it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SwapWaiting;

/// A waiting surface that a *camera* view can see, as the render world reports it: the entity is in
/// some camera's `RenderVisibleEntities`. The bridge writes it every frame (inserted or removed only
/// when it changes) for the waiting set.
///
/// This, not Bevy's `ViewVisibility`, is what "a view can see it" means for the swap. `ViewVisibility`
/// is also set by the light passes for a mesh inside a shadow cascade or cubemap
/// (bevy_light-0.19.0 lib.rs:489-493, :629, :634), and such a mesh is never in a camera's visible set,
/// so the bridge can never confirm it drawn: read through `ViewVisibility`, every off-camera terrain
/// patch in sun-shadow range would wait for ever (the portal-heavy break of 2026-10-09). The marker
/// asks the same question the bridge's drawn check asks, so the two cannot disagree.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SwapInCameraView;

/// The waiting surfaces and whether a camera can see each: what [`pending_drawn`] counts over.
pub(crate) type WaitingVisibility<'w, 's> = Query<'w, 's, Has<SwapInCameraView>, With<SwapWaiting>>;

/// How many surfaces are waiting on the render world while a camera can see them
/// ([`SwapInCameraView`]).
///
/// This is the one rule a settle (design-1091 C4), the world-ready predicate, a door prewarm and the
/// run-level invariant count through, so they cannot disagree about what a hole is: a surface no view
/// can see is never a hole - the bridge can never confirm it drawn, because Bevy specializes a
/// pipeline only for the views whose visible set holds the entity, and its stand-in covers it - so it
/// holds nothing up. A surface the stand-in still hides is not counted either: it is covered, and the
/// frame the stand-in yields the quadrant is already `Visibility::Inherited`.
///
/// [`SwapInCameraView`] is written during extract from the render world's visible sets of the frame
/// before, so an entity that becomes visible is counted one or two frames late. That can only delay
/// a settle by that much (which [`crate::shots::SETTLE_QUIET_FRAMES`] absorbs), never settle on a
/// surface that was already visible and undrawn, because a visible surface stays visible.
///
/// The set is small: only the entities the engine has marked waiting, which is the same bound the
/// bridge itself pays (design-1091 §2.3).
pub(crate) fn pending_drawn(waiting: &Query<Has<SwapInCameraView>, With<SwapWaiting>>) -> usize {
    waiting.iter().filter(|in_view| *in_view).count()
}

/// What a waiting surface's draw needs from the render world, as asset ids.
///
/// The declaration is the surface owner's (today `streaming::spawn_cell` for a terrain quadrant,
/// from the same layer images it hands to `TerrainExtension`) and it is kept true: when the owner
/// changes what the material binds, it re-declares the surface. It carries no terrain, cell or
/// worldspace: the same component serves a terrain quadrant, a LOD patch or any later "swap A for B
/// once B is drawn".
#[derive(Component, Debug, Clone)]
pub struct SwapSurface {
    /// The material asset the entity draws with, of whatever material type.
    material: UntypedAssetId,
    /// Every image the material's bind group holds, deduplicated.
    images: Vec<AssetId<Image>>,
    /// The entity that covers this surface until it is drawn (the LOD tile under the quadrant, or
    /// the root a detail change replaces), carried over into [`SwapPrepared`]. A detail-change
    /// commit is what inserts it ([`SwapSurface::with_stand_in`]), and the hand-over that despawns
    /// that root reads it back ([`release_detail_change_stand_ins`]); the LOD tile's is slice C2's.
    stand_in: Option<Entity>,
}

impl SwapSurface {
    /// A surface's draw needs: its material and every image that material binds.
    ///
    /// Repeating an image (a quadrant can sample one texture in two layers) costs the bridge one
    /// lookup per *distinct* asset per frame, so the images are sorted and deduplicated here, once.
    pub fn new(material: UntypedAssetId, images: impl IntoIterator<Item = AssetId<Image>>) -> Self {
        let mut images: Vec<AssetId<Image>> = images.into_iter().collect();
        images.sort_unstable();
        images.dedup();
        Self {
            material,
            images,
            stand_in: None,
        }
    }

    /// Re-declares the images the material binds, keeping the material and any stand-in. The owner
    /// calls this when it changes the material in a way that removes a binding.
    pub fn set_images(&mut self, images: impl IntoIterator<Item = AssetId<Image>>) {
        let mut images: Vec<AssetId<Image>> = images.into_iter().collect();
        images.sort_unstable();
        images.dedup();
        self.images = images;
    }

    /// Names the entity that covers this surface until it is drawn: the root a detail change
    /// replaces, which the commit keeps alive as the rule for its hand-over is "the new root's
    /// surfaces are covered", read back through [`SwapSurface::stand_in`].
    #[must_use]
    pub fn with_stand_in(mut self, stand_in: Entity) -> Self {
        self.stand_in = Some(stand_in);
        self
    }

    /// The surface's stand-in, if it declares one.
    pub fn stand_in(&self) -> Option<Entity> {
        self.stand_in
    }

    #[cfg(test)]
    pub(crate) fn image_ids(&self) -> &[AssetId<Image>] {
        &self.images
    }
}

/// The render world has everything this surface's draw needs, seen in the extract *after* the
/// render frame that established it: the mesh buffers and `RenderMesh`, every image the material
/// binds, and the material's prepared bind group. The stand-in may yield to it; nothing in this
/// slice acts on that.
#[derive(Component, Debug, Clone, Copy)]
pub struct SwapPrepared {
    /// The stand-in this surface replaces, if it has one (see [`SwapSurface`]).
    pub stand_in: Option<Entity>,
    /// The bridge's own extract-frame index at which the surface was prepared. Kept for the later
    /// lag gauges (design-1091 §2.4); this slice's gauges do not read it.
    pub since: u32,
}

/// Historical render-queue observation for a waiting surface: its declared mesh/material
/// instance, prepared assets and material slab matched, and every currently seeing camera
/// had queued forward membership and an Ok specialization. This is not a GPU fence or
/// pixel-coverage proof, and never authorizes current-view fallback suppression.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SwapDrawn;

/// Exact asset identity of a bridge observation. Retirement/diagnostic evidence,
/// never authority to suppress resident fallback in a current render view.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct SwapDrawReceipt {
    pub mesh: AssetId<Mesh>,
    pub material: UntypedAssetId,
}

impl SwapDrawReceipt {
    pub(crate) fn matches(&self, mesh: AssetId<Mesh>, surface: &SwapSurface) -> bool {
        self.mesh == mesh && self.material == surface.material
    }
}

/// Diagnostic threshold for a slow detail handover. Crossing it reports once;
/// elapsed frames never authorize removing terrain that still covers a replacement.
pub const MAX_SWAP_HOLD_FRAMES: u32 = 120;

/// A root a detail change keeps drawn: the cell now draws the root that replaced this one, and
/// this one keeps covering the grid until every surface of the new root is drawn.
///
/// It is carried by the old root, and the new root's surfaces name it back
/// ([`SwapSurface::stand_in`]). That is what makes "the new root is gone" (the camera left the cell
/// before the swap completed) and "the new root has no terrain" (nothing to wait for) the same
/// event for the hand-over - no waiting surface names this root any more - so neither can leave a
/// stand-in covering a cell nobody asked for.
///
/// A stand-in is not a streamed cell root: `streaming`'s commit removes the root markers
/// (`StreamedCellRoot`/`DistantTerrainRoot`) as it hands the root over, so the lifecycle check, the
/// settles and every root scan see the cell's one root - the new one - and never this one. It keeps
/// its `CellRef` and `ExteriorCellGrid`: the first says in a log which cell it covers, the second
/// keeps the render origin placing it (`streaming::reposition_cell_roots`).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SwapStandIn {
    /// Frames held, saturating at u32::MAX. The warning threshold does not bound lifetime.
    pub held_frames: u32,
}

/// Request render-camera visibility observations of this root's covering subtree.
/// The owner controls its lifetime and removes this marker and its observation when done.
/// This does not make the root a stand-in or authorize the bridge to release it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct SwapCoverageRoot;

/// Last render observation of camera visibility of a requested covering subtree.
/// Absence means unknown. `false` only says no camera saw the covering geometry; the
/// replacement owner must also establish preparation or draw readiness before retirement.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct SwapCoverageInCameraView(pub bool);

/// Current main-frame proof, read only after PostUpdate visibility checks (normally in Last).
/// Shadow visibility also keeps coverage alive. Missing visibility or an empty/invalid subtree
/// is unknown, never evidence that an old surface can retire.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct CurrentCoverageVisibility<'w, 's> {
    nodes: Query<
        'w,
        's,
        (
            Has<Mesh3d>,
            Option<&'static ViewVisibility>,
            Option<&'static Children>,
        ),
    >,
}

impl CurrentCoverageVisibility<'_, '_> {
    pub(crate) fn is_unviewed(&self, root: Entity) -> bool {
        let mut pending = vec![root];
        let mut visited = bevy::platform::collections::HashSet::<Entity>::default();
        let mut meshes = 0;
        while let Some(entity) = pending.pop() {
            if !visited.insert(entity) {
                return false;
            }
            let Ok((mesh, visibility, children)) = self.nodes.get(entity) else {
                return false;
            };
            if mesh {
                meshes += 1;
                if !visibility.is_some_and(|visibility| !visibility.get()) {
                    return false;
                }
            }
            if let Some(children) = children {
                pending.extend(children.iter());
            }
        }
        meshes != 0
    }
}

/// What the render world holds for one waiting surface, this extract: the plain snapshot
/// [`swap_readiness`] decides on. Every field is a fact this world has established, so the decision
/// is a pure function of the snapshot and can be tested without a GPU.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SwapSurfaceFacts {
    /// The mesh's vertex/index buffers exist in the GPU allocator (`MeshAllocator::mesh_slabs`).
    pub mesh_allocated: bool,
    /// The mesh's render form is prepared (`RenderAssets<RenderMesh>`).
    pub render_mesh: bool,
    /// How many of the declared images are prepared (`RenderAssets<GpuImage>`).
    pub images_ready: usize,
    /// How many images the surface declared. Zero means this surface needs no image prepared.
    pub images_declared: usize,
    /// The material's bind group exists (`ErasedRenderAssets<PreparedMaterial>`), which is the
    /// lookup that fails while any image the material binds is still missing.
    pub material: bool,
    /// A view that can see the entity also has a compiled pipeline for it
    /// (`SpecializedMaterialPipelineCache` + `PipelineCache`). False for an entity no view can see -
    /// which is why a surface nobody looks at stays prepared, never drawn.
    pub pipeline_ok: bool,
}

impl SwapSurfaceFacts {
    /// The facts a surface that is *already* prepared reads: the render world holds its buffers, its
    /// mesh, its material and every image it binds, and only the pipeline can still change.
    ///
    /// Re-reading the asset lookups for a prepared surface would cost the per-frame lookups
    /// design-1091 §2.3 bounds to one or two per item; they cannot un-prepare while the asset is
    /// unchanged, and the marker's contract is that a surface's declared assets are set once, at
    /// the moment it is marked waiting.
    pub const fn assets_ready() -> Self {
        Self {
            mesh_allocated: true,
            render_mesh: true,
            images_ready: 0,
            images_declared: 0,
            material: true,
            pipeline_ok: false,
        }
    }
}

/// What the bridge decided about one surface this extract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapReadiness {
    /// Something the draw needs is not in the render world yet.
    Waiting,
    /// Everything is there; the pipeline is not (or no view can see it yet).
    Prepared,
    /// Everything is there and a pipeline that can draw it is compiled.
    Drawn,
}

/// The rule, in one place (design-1091 §2.1): a surface is [`SwapReadiness::Prepared`] when the
/// render world holds its mesh buffers, its `RenderMesh`, every image its material binds and the
/// material's prepared bind group, and [`SwapReadiness::Drawn`] once, in addition, a view that can
/// see it has a compiled pipeline for it. Anything missing keeps it [`SwapReadiness::Waiting`], and
/// the marker's absence from the waiting set at the end of a frame is the drawn signal.
pub fn swap_readiness(facts: &SwapSurfaceFacts) -> SwapReadiness {
    if !facts.mesh_allocated
        || !facts.render_mesh
        || facts.images_ready < facts.images_declared
        || !facts.material
    {
        return SwapReadiness::Waiting;
    }
    if facts.pipeline_ok {
        SwapReadiness::Drawn
    } else {
        SwapReadiness::Prepared
    }
}

/// What the C5 gauges read: how many swaps the main-world hand-over completed in the frame that
/// just ran. A resource because the hand-over removes the markers through commands, so a query
/// cannot see the frame's own completions (see [`complete_drawn_swaps`]).
#[derive(Resource, Debug, Default)]
pub struct SwapBridgeStats {
    /// Entities that carried both the waiting and the drawn marker this frame and left the
    /// waiting set.
    pub drawn_this_frame: u32,
    /// Covering roots still retained after this frame's handovers.
    pub live_holds: usize,
}

/// The bridge's one main-world query: the entities the engine has marked waiting, with what their
/// draw needs. The cached [`SystemState`] is built from the main world on the first extract (the
/// pattern `bevy_render`'s `extract_screenshots` uses) and kept for the run.
///
/// The `With<SwapWaiting>` filter is the whole cost bound: the bridge visits this set and nothing
/// else, however many entities the world holds.
#[allow(clippy::type_complexity)]
type WaitingSurfaces = Query<
    'static,
    'static,
    (
        Entity,
        &'static Mesh3d,
        &'static SwapSurface,
        Option<&'static SwapPrepared>,
        Has<SwapInCameraView>,
    ),
    With<SwapWaiting>,
>;

/// The bridge: for every waiting surface, the render world's own answer to "is this drawable?".
///
/// It reads the state the *previous* render frame left - assets are prepared and pipelines are
/// compiled in the render world's `Render` schedule, after the extract - and writes a marker into
/// the main world, which sees it on its next frame. A surface is up in the stand-in for at least
/// those frames, so no frame shows neither (design-1091 §2.2).
#[allow(clippy::too_many_arguments)]
fn swap_bridge(
    mut main_world: ResMut<MainWorld>,
    mut waiting: Local<Option<SystemState<WaitingSurfaces>>>,
    mut frame: Local<u32>,
    mut decisions: Local<Vec<SwapDecision>>,
    mesh_allocator: Res<MeshAllocator>,
    render_meshes: Res<RenderAssets<RenderMesh>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    allocators: Res<MaterialBindGroupAllocators>,
    mesh_instances: Res<RenderMeshInstances>,
    material_instances: Res<RenderMaterialInstances>,
    phases: systems::QueuePhases,
    specialized: Option<Res<SpecializedMaterialPipelineCache>>,
    pipelines: Res<PipelineCache>,
    views: Query<(&ExtractedView, &RenderVisibleEntities)>,
) {
    // The pipeline cache is created in the render world's `RenderStartup`; before it has run
    // nothing has been drawn, so there is nothing to confirm.
    let Some(specialized) = specialized else {
        return;
    };

    *frame = frame.saturating_add(1);
    if waiting.is_none() {
        *waiting = Some(SystemState::new(&mut main_world));
    }
    let state = waiting.as_mut().expect("the state was created just above");

    decisions.clear();
    {
        let query = state
            .get(&main_world)
            .expect("the cached query state was built from the main world");
        for (entity, mesh, surface, prepared, in_view) in query.iter() {
            let seen = views
                .iter()
                .any(|(_, visible)| view_sees(visible, MainEntity::from(entity)));
            let facts = {
                gather_surface_facts(
                    mesh,
                    surface,
                    &mesh_allocator,
                    &render_meshes,
                    &gpu_images,
                    &materials,
                    &allocators,
                )
            };
            let facts = SwapSurfaceFacts {
                pipeline_ok: mesh_instances.mesh_asset_id(MainEntity::from(entity))
                    == Some(mesh.0.id())
                    && material_instances
                        .instances
                        .get(&MainEntity::from(entity))
                        .is_some_and(|instance| instance.asset_id == surface.material)
                    && pipeline_resolved(
                        MainEntity::from(entity),
                        &views,
                        &specialized,
                        &pipelines,
                        &phases,
                    ),
                ..facts
            };
            let failed = surface.stand_in().is_some()
                && mesh_instances.mesh_asset_id(MainEntity::from(entity)) == Some(mesh.0.id())
                && material_instances
                    .instances
                    .get(&MainEntity::from(entity))
                    .is_some_and(|instance| instance.asset_id == surface.material)
                && views.iter().any(|(view, visible)| {
                    view_sees(visible, MainEntity::from(entity))
                        && specialized
                            .get(&view.retained_view_entity)
                            .and_then(|cache| cache.get(&MainEntity::from(entity)))
                            .is_some_and(|pipeline| {
                                terminal_pipeline_failure(
                                    pipelines.get_render_pipeline_state(*pipeline),
                                )
                            })
                });
            let readiness = swap_readiness(&facts);
            let in_view_changed = (seen != in_view).then_some(seen);
            if failed || readiness != SwapReadiness::Waiting || in_view_changed.is_some() {
                decisions.push(SwapDecision {
                    entity,
                    mesh: mesh.0.id(),
                    material: surface.material,
                    insert_prepared: readiness != SwapReadiness::Waiting && prepared.is_none(),
                    drawn: readiness == SwapReadiness::Drawn,
                    failed,
                    stand_in: surface.stand_in(),
                    in_view: in_view_changed,
                });
            }
        }
    }

    // Observe the covering geometry too: a hidden replacement can be absent from every
    // camera while its visible old root still covers the hole. Absence on the new surface
    // alone therefore cannot authorize retirement.
    observe_coverage_roots(&mut main_world, |entity| {
        views.iter().any(|(_, visible)| view_sees(visible, entity))
    });

    for decision in decisions.iter() {
        let mut entity = main_world.entity_mut(decision.entity);
        if decision.insert_prepared {
            entity.insert(SwapPrepared {
                stand_in: decision.stand_in,
                since: *frame,
            });
        }
        if decision.failed {
            error!(surface = ?decision.entity, "replacement terrain pipeline failed; requesting detail-change rollback");
            entity.insert(crate::streaming::AssetFailed);
        }
        if decision.drawn {
            entity.insert((
                SwapDrawn,
                SwapDrawReceipt {
                    mesh: decision.mesh,
                    material: decision.material,
                },
            ));
        }
        match decision.in_view {
            Some(true) => {
                entity.insert(SwapInCameraView);
            }
            Some(false) => {
                entity.remove::<SwapInCameraView>();
            }
            None => {}
        }
    }
}

/// Observe only live holds and explicit requests. The return value counts requested roots,
/// allowing tests to prove that ordinary scene roots do not enter this work domain.
fn observe_coverage_roots(
    world: &mut World,
    mut seen_by_camera: impl FnMut(MainEntity) -> bool,
) -> usize {
    let mut query =
        world.query_filtered::<Entity, Or<(With<SwapStandIn>, With<SwapCoverageRoot>)>>();
    let roots: Vec<_> = query.iter(world).collect();
    let observed_roots = roots.len();
    for root in roots {
        let mut descendants = vec![root];
        let mut seen = false;
        while let Some(entity) = descendants.pop() {
            if seen_by_camera(entity.into()) {
                seen = true;
                break;
            }
            if let Some(children) = world.get::<Children>(entity) {
                descendants.extend(children.iter());
            }
        }
        world
            .entity_mut(root)
            .insert(SwapCoverageInCameraView(seen));
    }
    observed_roots
}

/// One marker write the bridge decided on, applied after the query's borrow of the main world ends.
#[derive(Debug, Clone, Copy)]
struct SwapDecision {
    entity: Entity,
    mesh: AssetId<Mesh>,
    material: UntypedAssetId,
    /// Whether `SwapPrepared` still has to be written (it need not be re-written every frame a
    /// surface waits for its pipeline: that would churn change detection for nothing).
    insert_prepared: bool,
    drawn: bool,
    failed: bool,
    stand_in: Option<Entity>,
    /// `Some(seen)` when whether a camera can see the surface changed this frame.
    in_view: Option<bool>,
}

/// Reads the render world's state for one surface: the five lookups of design-1091 §2.1 minus the
/// pipeline, which depends on the views and is read separately.
fn gather_surface_facts(
    mesh: &Mesh3d,
    surface: &SwapSurface,
    mesh_allocator: &MeshAllocator,
    render_meshes: &RenderAssets<RenderMesh>,
    gpu_images: &RenderAssets<GpuImage>,
    materials: &ErasedRenderAssets<PreparedMaterial>,
    allocators: &MaterialBindGroupAllocators,
) -> SwapSurfaceFacts {
    let mesh_id = mesh.0.id();
    SwapSurfaceFacts {
        mesh_allocated: mesh_allocator.mesh_slabs(&mesh_id).is_some(),
        render_mesh: render_meshes.get(mesh_id).is_some(),
        images_ready: surface
            .images
            .iter()
            .filter(|image| gpu_images.get(**image).is_some())
            .count(),
        images_declared: surface.images.len(),
        material: materials.get(surface.material).is_some_and(|material| {
            allocators
                .get(&surface.material.type_id())
                .and_then(|allocator| allocator.get(material.binding.group))
                .is_some_and(|slab| slab.bind_group().is_some())
        }),
        pipeline_ok: false,
    }
}

/// Whether every camera that can see the entity has its own queued forward entry
/// and a resolved specialized pipeline. At least one seeing camera is required.
///
/// The pipeline cache is keyed by view and by main-world entity, and Bevy specializes an entity
/// only for the views whose visible set holds it - but the visible set is read here too, so the
/// "a view can see it" half of the rule is a fact of this frame rather than an assumption about
/// Bevy's internals. An entity whose pipeline is `Queued` (or absent) is not drawn yet.
/// Bevy retries missing shaders and imports. Only processing/module errors are terminal.
fn terminal_pipeline_failure(state: &CachedPipelineState) -> bool {
    matches!(
        state,
        CachedPipelineState::Err(
            bevy::shader::ShaderCacheError::ProcessShaderError(_)
                | bevy::shader::ShaderCacheError::CreateShaderModule(_)
        )
    )
}

fn pipeline_resolved(
    entity: MainEntity,
    views: &Query<(&ExtractedView, &RenderVisibleEntities)>,
    specialized: &SpecializedMaterialPipelineCache,
    pipelines: &PipelineCache,
    phases: &systems::QueuePhases,
) -> bool {
    let mut seen = false;
    for (view, visible) in views.iter() {
        if !view_sees(visible, entity) {
            continue;
        }
        seen = true;
        if !phases.forward(&view.retained_view_entity).queued(entity) {
            return false;
        }
        // The cheap half first: a view with no cache entry or no pipeline for the entity cannot
        // have drawn it, and costs one lookup each.
        let Some(cached) = specialized
            .get(&view.retained_view_entity)
            .and_then(|by_entity| by_entity.get(&entity))
            .copied()
        else {
            return false;
        };
        if !matches!(
            pipelines.get_render_pipeline_state(cached),
            CachedPipelineState::Ok(_)
        ) {
            return false;
        }
    }
    seen
}

/// Whether a view's visible set holds the entity: it is in the CPU-culled list (a sorted list, so
/// one binary search) or, for a mesh with `NoCpuCulling`, in the GPU-culled table.
fn view_sees(visible: &RenderVisibleEntities, entity: MainEntity) -> bool {
    visible
        .get::<Mesh3d>()
        .is_some_and(|class| class_sees(class, entity))
}

/// The membership test of [`view_sees`] against one view's mesh class.
fn class_sees(class: &RenderVisibleEntitiesClass, entity: MainEntity) -> bool {
    class.entities_gpu_culling.contains_key(&entity)
        || class
            .entities_cpu_culling
            .binary_search_by_key(&entity, |(_, main_entity)| *main_entity)
            .is_ok()
}

/// The main-world half of a swap: an entity the render world has confirmed drawn leaves the waiting
/// set. Runs in `Update`, so the marker the bridge wrote during the extract between two frames is
/// visible here, in the frame whose visibility pass follows it.
///
/// The stand-in half of the hand-over is separate: a detail change's stand-in belongs to the whole
/// new root, not to one surface, so it is [`release_detail_change_stand_ins`] that despawns it.
fn complete_drawn_swaps(
    mut commands: Commands,
    mut stats: ResMut<SwapBridgeStats>,
    drawn: Query<Entity, (With<SwapWaiting>, With<SwapDrawn>)>,
) {
    let mut completed = 0u32;
    for entity in &drawn {
        commands
            .entity(entity)
            .try_remove::<(SwapWaiting, SwapInCameraView)>();
        completed = completed.saturating_add(1);
    }
    stats.drawn_this_frame = completed;
}

/// A terminal terrain validation failure cancels the replacement transaction. Streaming
/// restores the old root's ownership before disposing of the failed replacement, so the
/// covering geometry and colliders never disappear while the cell is still demanded.
type FailedReplacementSurfaces<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static SwapSurface),
    (With<SwapWaiting>, With<crate::streaming::AssetFailed>),
>;

type DetailReplacementRoots<'w, 's> = Query<
    'w,
    's,
    (),
    Or<(
        With<crate::world::components::StreamedCellRoot>,
        With<crate::world::components::DistantTerrainRoot>,
        With<SwapStandIn>,
    )>,
>;

fn rollback_failed_detail_changes(
    mut commands: Commands,
    failed: FailedReplacementSurfaces,
    parents: Query<&ChildOf>,
    roots: DetailReplacementRoots,
    holds: Query<(), With<SwapStandIn>>,
    mut transactions: Local<Vec<(Entity, Entity)>>,
) {
    transactions.clear();
    for (entity, surface) in &failed {
        let Some(stand_in) = surface.stand_in().filter(|root| holds.contains(*root)) else {
            continue;
        };
        let Some(replacement) = parents
            .iter_ancestors(entity)
            .find(|root| roots.contains(*root))
        else {
            continue;
        };
        if !transactions.contains(&(stand_in, replacement)) {
            transactions.push((stand_in, replacement));
        }
    }
    for &(stand_in, replacement) in transactions.iter() {
        commands.queue(move |world: &mut World| {
            if world.get::<SwapStandIn>(replacement).is_some() {
                // A newer committed root already owns this cell. Its failed intermediate
                // cover must yield to the earlier valid cover without rolling back the
                // current streaming generation. Redirect declarations before disposal.
                if world.get::<SwapStandIn>(stand_in).is_none() {
                    return;
                }
                let mut surfaces = world.query::<(&mut SwapSurface, Option<&mut SwapPrepared>)>();
                for (mut surface, prepared) in surfaces.iter_mut(world) {
                    if surface.stand_in == Some(replacement) {
                        surface.stand_in = Some(stand_in);
                        if let Some(mut prepared) = prepared {
                            prepared.stand_in = Some(stand_in);
                        }
                    }
                }
                error!(?replacement, ?stand_in, "retiring failed intermediate stand-in after transferring coverage to its valid predecessor");
                world.despawn(replacement);
            } else {
                crate::streaming::rollback_detail_change(world, stand_in, replacement);
            }
        });
    }
}

/// A waiting surface names its covering root until the renderer confirms a draw.
type WaitingCoverage<'w, 's> = Query<
    'w,
    's,
    (
        &'static SwapSurface,
        Has<SwapDrawn>,
        Has<SwapPrepared>,
        Has<SwapInCameraView>,
        Option<&'static ViewVisibility>,
    ),
    With<SwapWaiting>,
>;

#[derive(Default)]
struct StandInCoverage {
    uncovered: std::collections::HashSet<Entity>,
    unviewed: std::collections::HashSet<Entity>,
}

/// Retire a covering root only after all remaining replacements draw. Preparation and
/// camera invisibility alone do not prove a replacement draw. A prepared replacement
/// may release its hold only when the renderer observes neither it nor its covering subtree
/// in any camera AND current PostUpdate culling confirms both are unviewed. This runs in Last;
/// a camera turn or new view therefore overrules stale render visibility. Cancellation still releases
/// the root: no remaining surface names it once its replacement has been despawned.
///
/// Current visibility visits each historically unviewed covering subtree once per frame,
/// followed by one pass over waiting surfaces and holds. A settled run returns immediately.
#[allow(clippy::too_many_arguments)]
fn release_detail_change_stand_ins(
    mut commands: Commands,
    (mut metrics, mut stats): (ResMut<StreamingMetrics>, ResMut<SwapBridgeStats>),
    mut profiler: ResMut<ProfilingState>,
    mut holds: Query<(Entity, &mut SwapStandIn, Option<&SwapCoverageInCameraView>)>,
    cells: Query<(&CellRef, Option<&ExteriorCellGrid>)>,
    waiting: WaitingCoverage,
    current: CurrentCoverageVisibility,
    mut coverage: Local<StandInCoverage>,
) {
    stats.live_holds = 0;
    if holds.is_empty() {
        return;
    }
    // The stand-ins a waiting surface still needs. Built from the surfaces' own declarations rather
    // than from the new root: a new root that is despawned, and one that declares no surface at all
    // (a cell with no terrain), both simply stop naming anything here.
    coverage.unviewed.clear();
    for (root, _, observed) in holds.iter() {
        if observed.is_some_and(|observed| !observed.0) && current.is_unviewed(root) {
            coverage.unviewed.insert(root);
        }
    }
    coverage.uncovered.clear();
    for (surface, drawn, prepared, in_view, visibility) in &waiting {
        let unviewed_ready = prepared
            && !in_view
            && visibility.is_some_and(|visibility| !visibility.get())
            && surface
                .stand_in()
                .is_some_and(|root| coverage.unviewed.contains(&root));
        if drawn || unviewed_ready {
            continue;
        }
        if let Some(stand_in) = surface.stand_in() {
            coverage.uncovered.insert(stand_in);
        }
    }
    for (stand_in, mut hold, _) in &mut holds {
        hold.held_frames = hold.held_frames.saturating_add(1);
        let covered = !coverage.uncovered.contains(&stand_in);
        if covered {
            commands.entity(stand_in).try_despawn();
            continue;
        }
        stats.live_holds += 1;
        if hold.held_frames != MAX_SWAP_HOLD_FRAMES {
            continue;
        }
        metrics.swap_hold_timeouts = metrics.swap_hold_timeouts.saturating_add(1);
        profiler.increment("streaming/swap_hold_timeouts", 1);
        match cells.get(stand_in) {
            Ok((cell, grid)) => warn!(
                cell = format_args!("{:08X}", cell.0),
                grid = ?grid.map(|grid| grid.0),
                held_frames = hold.held_frames,
                "detail-change stand-in is still waiting for coverage; retaining it"
            ),
            Err(_) => warn!(
                stand_in = ?stand_in,
                held_frames = hold.held_frames,
                "detail-change stand-in is still waiting for coverage; retaining it"
            ),
        }
    }
}

/// Waiting surfaces that are prepared but not yet drawn.
type PreparedNotDrawn = (With<SwapWaiting>, With<SwapPrepared>, Without<SwapDrawn>);

/// Waiting surfaces a camera can see that are not yet drawn: the ones a settle waits on.
type InCameraViewNotDrawn = (
    With<SwapWaiting>,
    With<SwapInCameraView>,
    Without<SwapDrawn>,
);

/// The C5 gauges over the swap: how many surfaces are still waiting on the render world, how many
/// of those it has prepared but not confirmed drawn, and how many it confirmed drawn this frame.
///
/// `swap/waiting` excludes a surface confirmed drawn this frame whose marker removal is still
/// queued, so it is the set the settle of C4 will wait on rather than a count that can lag by one
/// frame. A surface no view can see stays waiting by design (it is prepared, never drawn), which is
/// what `swap/prepared_not_drawn` measures.
///
/// Installed only for a run that profiles: it is a query over the marked set every frame, which is
/// the same rule [`crate::profiling::ProfilingPlugin`] follows for the per-frame series.
fn record_swap_gauges(
    waiting: Query<(), (With<SwapWaiting>, Without<SwapDrawn>)>,
    prepared: Query<(), PreparedNotDrawn>,
    in_camera_view: Query<(), InCameraViewNotDrawn>,
    stats: Res<SwapBridgeStats>,
    mut profiler: ResMut<ProfilingState>,
) {
    profiler.set_gauge("swap/live_holds", stats.live_holds as f64);
    profiler.set_gauge("swap/waiting", waiting.iter().count() as f64);
    profiler.set_gauge("swap/prepared_not_drawn", prepared.iter().count() as f64);
    profiler.set_gauge("swap/drawn_this_frame", f64::from(stats.drawn_this_frame));
    // The split of `swap/waiting` the settles depend on: of the surfaces still waiting, how many a
    // camera can see (what `pending_drawn` counts - near 0 once a view has settled) against the rest,
    // which no camera sees and so are never confirmed drawn (off camera, or in a shadow view only).
    profiler.set_gauge(
        "swap/in_camera_view_not_drawn",
        in_camera_view.iter().count() as f64,
    );
}

/// Registers the render→main swap bridge: the extract system in the render app, the main-world
/// hand-over, and the C5 gauges.
pub struct SwapBridgePlugin;

impl Plugin for SwapBridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwapBridgeStats>()
            .add_systems(
                Update,
                (rollback_failed_detail_changes, complete_drawn_swaps).chain(),
            )
            // Current-frame camera/light culling finishes in PostUpdate. A historical render
            // observation alone cannot authorize retirement after the camera moves.
            .add_systems(Last, release_detail_change_stand_ins);
        // The destination readiness protocol (`destination`): idle until a consumer requests.
        app.add_plugins(systems::DestinationReadinessPlugin);
        // The gauges are measurement data: an ordinary play session installs none of them.
        let profiling = app
            .world()
            .get_resource::<EngineConfig>()
            .is_some_and(|config| config.profile_output_dir.is_some());
        if profiling {
            app.add_systems(
                Last,
                record_swap_gauges.after(release_detail_change_stand_ins),
            );
        }
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(ExtractSchedule, swap_bridge);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_loading_errors_do_not_abort_a_replacement() {
        use bevy::shader::ShaderCacheError;
        assert!(!terminal_pipeline_failure(&CachedPipelineState::Queued));
        assert!(!terminal_pipeline_failure(&CachedPipelineState::Err(
            ShaderCacheError::ShaderNotLoaded(Handle::<Shader>::default().id()),
        )));
        assert!(!terminal_pipeline_failure(&CachedPipelineState::Err(
            ShaderCacheError::ShaderImportNotYetAvailable,
        )));
        assert!(terminal_pipeline_failure(&CachedPipelineState::Err(
            ShaderCacheError::CreateShaderModule("invalid shader module".into()),
        )));
    }

    fn surface(images: &[AssetId<Image>]) -> SwapSurface {
        SwapSurface::new(
            Handle::<StandardMaterial>::default().id().untyped(),
            images.iter().copied(),
        )
    }

    /// Every piece the render world has to hold for a surface to count as drawable: each one
    /// missing keeps the surface waiting, and all of them together make it drawn. The order of the
    /// two asset steps is the one the render world establishes them in
    /// (`allocate_and_free_meshes` before `prepare_assets::<RenderMesh>`), and the two cases are
    /// listed separately because a budget-deferred frame shows exactly that state.
    #[test]
    fn prepared_requires_every_asset() {
        let all = SwapSurfaceFacts {
            mesh_allocated: true,
            render_mesh: true,
            images_ready: 1,
            images_declared: 1,
            material: true,
            pipeline_ok: true,
        };
        assert_eq!(swap_readiness(&all), SwapReadiness::Drawn);

        for (what, facts, expected) in [
            (
                "no vertex buffers",
                SwapSurfaceFacts {
                    mesh_allocated: false,
                    ..all
                },
                SwapReadiness::Waiting,
            ),
            (
                "no RenderMesh",
                SwapSurfaceFacts {
                    render_mesh: false,
                    ..all
                },
                SwapReadiness::Waiting,
            ),
            (
                "an image not prepared",
                SwapSurfaceFacts {
                    images_ready: 0,
                    ..all
                },
                SwapReadiness::Waiting,
            ),
            (
                "no prepared material",
                SwapSurfaceFacts {
                    material: false,
                    ..all
                },
                SwapReadiness::Waiting,
            ),
            (
                "no compiled pipeline",
                SwapSurfaceFacts {
                    pipeline_ok: false,
                    ..all
                },
                SwapReadiness::Prepared,
            ),
        ] {
            assert_eq!(swap_readiness(&facts), expected, "{what}");
        }

        // A surface that declares no image needs none prepared: the count decides, not a bias
        // towards waiting.
        let image_free = SwapSurfaceFacts {
            images_ready: 0,
            images_declared: 0,
            ..all
        };
        assert_eq!(swap_readiness(&image_free), SwapReadiness::Drawn);
    }

    /// An entity whose pipeline is `Ok` but which is in no view's visible set is prepared, not
    /// drawn: the stand-in keeps covering it. This is the visibility half of the rule, and the
    /// bridge reads it from the view's own set rather than assuming the pipeline cache implies it.
    #[test]
    fn confirming_needs_a_view_that_can_see_it() {
        let mut world = World::new();
        let seen = world.spawn_empty().id();
        let gpu_seen = world.spawn_empty().id();
        let unseen = world.spawn_empty().id();

        let mut class = RenderVisibleEntitiesClass {
            entities_cpu_culling: vec![(Entity::PLACEHOLDER, MainEntity::from(seen))],
            ..Default::default()
        };
        class
            .entities_gpu_culling
            .insert(MainEntity::from(gpu_seen), Entity::PLACEHOLDER);

        assert!(class_sees(&class, MainEntity::from(seen)));
        assert!(class_sees(&class, MainEntity::from(gpu_seen)));
        assert!(!class_sees(&class, MainEntity::from(unseen)));
    }

    /// The one query the bridge runs visits the marked entities and nothing else, however many
    /// entities the world holds: this is the cost bound in its smallest form.
    #[test]
    fn the_bridge_visits_only_the_marked_entities() {
        let mut world = World::new();
        let mut marked = Vec::new();
        for _ in 0..4 {
            marked.push(
                world
                    .spawn((Mesh3d(Handle::<Mesh>::default()), surface(&[]), SwapWaiting))
                    .id(),
            );
        }
        // Entities that are everything but marked: meshes the scene holds, a surface declaration
        // with nothing waiting on it, and an entity carrying only the marker.
        for _ in 0..16 {
            world.spawn((Mesh3d(Handle::<Mesh>::default()), surface(&[])));
        }
        world.spawn(SwapWaiting);

        let mut state = SystemState::<WaitingSurfaces>::new(&mut world);
        let query = state
            .get(&world)
            .expect("the state was built from this world");
        let visited: Vec<Entity> = query.iter().map(|(entity, ..)| entity).collect();
        assert_eq!(visited, marked);
    }

    /// The hand-over: a surface whose drawn marker has arrived leaves the waiting set, and the
    /// count the gauge reads is that frame's completions only.
    #[test]
    fn the_hand_over_releases_a_drawn_surface() {
        let mut app = App::new();
        // What the engine's app has before this plugin: the streaming counters the detail-change
        // hand-over counts into, and the profile it reports them through.
        app.init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_plugins(SwapBridgePlugin);
        let drawn = app.world_mut().spawn((SwapWaiting, SwapDrawn)).id();
        let waiting = app.world_mut().spawn(SwapWaiting).id();

        app.update();

        assert!(
            app.world().get::<SwapWaiting>(drawn).is_none(),
            "a drawn surface leaves the waiting set"
        );
        assert!(
            app.world().get::<SwapWaiting>(waiting).is_some(),
            "a surface still waiting keeps its marker"
        );
        assert_eq!(
            app.world().resource::<SwapBridgeStats>().drawn_this_frame,
            1
        );

        app.update();
        assert_eq!(
            app.world().resource::<SwapBridgeStats>().drawn_this_frame,
            0,
            "the count is the frame's own completions, not a running total"
        );
    }

    /// A surface's declared images are deduplicated, so the bridge pays one lookup per distinct
    /// image however many layers sample it; re-declaring them keeps the material and stand-in.
    #[test]
    fn a_surfaces_images_are_deduplicated_and_redeclared() {
        let mut world = World::new();
        world.init_resource::<Assets<Image>>();
        let (base, normal) = {
            let mut images = world.resource_mut::<Assets<Image>>();
            (images.add(Image::default()), images.add(Image::default()))
        };
        assert_ne!(base.id(), normal.id(), "two images, two ids");

        let material = Handle::<StandardMaterial>::default().id().untyped();
        let mut surface =
            SwapSurface::new(material, [base.id(), normal.id(), base.id(), normal.id()]);
        assert_eq!(
            surface.images,
            vec![base.id(), normal.id()],
            "each image once"
        );
        assert_eq!(surface.material, material);
        surface.set_images([normal.id()]);
        assert_eq!(surface.images, vec![normal.id()]);
        assert_eq!(surface.material, material);
        assert_eq!(surface.stand_in(), None);
    }

    // -------------------------------------------------------------------------------------
    // The detail-change hand-over (slice C3): the main world's own system, no render world.
    // -------------------------------------------------------------------------------------

    /// The app the hand-over's tests run in: the resources it reads and the plugin that installs it.
    fn stand_in_app() -> App {
        let mut app = App::new();
        app.init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_plugins(SwapBridgePlugin);
        app
    }

    #[test]
    fn requested_coverage_observes_only_its_subtree_without_owning_a_stand_in() {
        let mut app = stand_in_app();
        let root = app.world_mut().spawn(SwapCoverageRoot).id();
        let child = app.world_mut().spawn(ChildOf(root)).id();
        let ordinary = app.world_mut().spawn_empty().id();
        for _ in 0..128 {
            app.world_mut().spawn(ChildOf(ordinary));
        }
        let mut entity_visits = 0;
        let observed = observe_coverage_roots(app.world_mut(), |entity| {
            entity_visits += 1;
            entity == MainEntity::from(child)
        });
        assert_eq!(observed, 1);
        assert_eq!(entity_visits, 2);
        assert!(app.world().get::<SwapCoverageInCameraView>(root).unwrap().0);
        assert!(
            app.world()
                .get::<SwapCoverageInCameraView>(ordinary)
                .is_none()
        );
        assert!(app.world().get::<SwapStandIn>(root).is_none());
        assert_eq!(observe_coverage_roots(app.world_mut(), |_| false), 1);
        assert!(!app.world().get::<SwapCoverageInCameraView>(root).unwrap().0);
        app.update();
        assert!(
            alive(&app, root),
            "a request does not transfer lifecycle ownership"
        );
        assert!(alive(&app, child));
        assert_eq!(app.world().resource::<SwapBridgeStats>().live_holds, 0);
        app.world_mut()
            .entity_mut(root)
            .remove::<(SwapCoverageRoot, SwapCoverageInCameraView)>();
        assert_eq!(
            observe_coverage_roots(app.world_mut(), |_| panic!("no roots requested")),
            0
        );
    }

    /// The shape a detail-change commit leaves behind: a new root whose four terrain patches all
    /// wait and name `stand_in`, and the stand-in's own four quadrants. Returns the new root, its
    /// patches and the stand-in's quadrants. The patches are born revealed and in view but not
    /// drawn - the state the hold exists for.
    fn detail_change(app: &mut App, stand_in: Entity) -> (Entity, Vec<Entity>, Vec<Entity>) {
        let new_root = app.world_mut().spawn_empty().id();
        let patches = (0..4)
            .map(|_| {
                app.world_mut()
                    .spawn((
                        surface(&[]).with_stand_in(stand_in),
                        SwapWaiting,
                        Mesh3d(Handle::default()),
                        Visibility::Inherited,
                        ViewVisibility::HIDDEN,
                        SwapInCameraView,
                        ChildOf(new_root),
                    ))
                    .id()
            })
            .collect();
        let quadrants = (0..4)
            .map(|_| {
                app.world_mut()
                    .spawn((
                        ChildOf(stand_in),
                        Mesh3d(Handle::default()),
                        ViewVisibility::HIDDEN,
                    ))
                    .id()
            })
            .collect();
        (new_root, patches, quadrants)
    }

    /// Sets what the main world's visibility pass leaves on a patch: its own `Visibility` (a terrain
    /// quadrant is `Hidden` until its textures are in `Assets<Image>`) and whether a camera saw it
    /// ([`SwapInCameraView`], which the bridge writes).
    fn set_visibility(app: &mut App, patch: Entity, own: Visibility, seen: bool) {
        let mut patch = app.world_mut().entity_mut(patch);
        patch.insert(own);
        if seen {
            patch.insert(SwapInCameraView);
        } else {
            patch.remove::<SwapInCameraView>();
        }
    }

    /// Whether the entity is still in the world.
    fn alive(app: &App, entity: Entity) -> bool {
        app.world().get_entity(entity).is_ok()
    }

    /// The hold itself (design-1091 §3): while the new root's terrain waits, the root the cell was
    /// drawn by stays in the world, and the frame the new patches are confirmed drawn it goes -
    /// through the despawn the commit used to do in the frame the replacement committed.
    #[test]
    fn a_detail_change_holds_the_old_root_until_its_replacement_is_drawn() {
        let mut app = stand_in_app();
        let stand_in = app.world_mut().spawn(SwapStandIn::default()).id();
        let (new_root, patches, quadrants) = detail_change(&mut app, stand_in);

        for _ in 0..3 {
            app.update();
        }
        assert!(
            alive(&app, stand_in),
            "the stand-in covers the waiting terrain"
        );
        assert_eq!(
            app.world()
                .get::<SwapStandIn>(stand_in)
                .unwrap()
                .held_frames,
            3,
            "the hold counts the frames it covers"
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .swap_hold_timeouts,
            0
        );

        // The bridge confirms all four patches drawn: the next frame the stand-in goes, and its own
        // quadrants with it - the new root, which is the cell's, stays.
        for patch in &patches {
            app.world_mut().entity_mut(*patch).insert(SwapDrawn);
        }
        app.update();

        assert!(
            !alive(&app, stand_in),
            "a drawn replacement releases the stand-in"
        );
        for quadrant in &quadrants {
            assert!(
                !alive(&app, *quadrant),
                "the stand-in's terrain goes with it"
            );
        }
        assert!(alive(&app, new_root), "the cell's own root stays");
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .swap_hold_timeouts,
            0,
            "a hand-over that completes is not a failure"
        );
    }

    #[test]
    fn preparation_or_camera_invisibility_does_not_retire_undrawn_coverage() {
        let mut app = stand_in_app();
        let stand_in = app.world_mut().spawn(SwapStandIn::default()).id();
        let (_, patches, _) = detail_change(&mut app, stand_in);
        for patch in &patches {
            app.world_mut().entity_mut(*patch).insert(SwapPrepared {
                stand_in: Some(stand_in),
                since: 1,
            });
            set_visibility(&mut app, *patch, Visibility::Inherited, false);
        }
        for _ in 0..MAX_SWAP_HOLD_FRAMES + 1 {
            app.update();
        }
        assert!(alive(&app, stand_in));
        for patch in &patches {
            set_visibility(&mut app, *patch, Visibility::Inherited, true);
            app.world_mut().entity_mut(*patch).insert(SwapDrawn);
        }
        app.update();
        assert!(!alive(&app, stand_in));
    }

    #[test]
    fn ready_unviewed_replacement_releases_hold_and_settles_live_gauge() {
        let mut app = stand_in_app();
        let stand_in = app
            .world_mut()
            .spawn((SwapStandIn::default(), SwapCoverageInCameraView(false)))
            .id();
        let (_, patches, _) = detail_change(&mut app, stand_in);
        for patch in patches {
            set_visibility(&mut app, patch, Visibility::Inherited, false);
            app.world_mut().entity_mut(patch).insert(SwapPrepared {
                stand_in: Some(stand_in),
                since: 1,
            });
        }
        app.update();
        assert!(!alive(&app, stand_in));
        assert_eq!(app.world().resource::<SwapBridgeStats>().live_holds, 0);
    }

    #[test]
    fn current_visibility_after_camera_turn_overrules_previous_offscreen_observation() {
        let mut app = stand_in_app();
        let old = app
            .world_mut()
            .spawn((SwapStandIn::default(), SwapCoverageInCameraView(false)))
            .id();
        let (_, patches, quadrants) = detail_change(&mut app, old);
        for &patch in &patches {
            set_visibility(&mut app, patch, Visibility::Inherited, false);
            app.world_mut().entity_mut(patch).insert(SwapPrepared {
                stand_in: Some(old),
                since: 1,
            });
        }
        let entering_view = quadrants[0];
        app.add_systems(
            PostUpdate,
            move |mut visibility: Query<&mut ViewVisibility>| {
                if let Ok(mut visible) = visibility.get_mut(entering_view) {
                    *visible = ViewVisibility::VISIBLE;
                }
            },
        );
        app.update();
        assert!(
            alive(&app, old),
            "Last must read this frame's culling after the camera turns"
        );
        assert_eq!(app.world().resource::<SwapBridgeStats>().live_holds, 1);
        for patch in patches {
            app.world_mut().entity_mut(patch).insert(SwapDrawn);
        }
        app.update();
        assert!(
            !alive(&app, old),
            "actual replacement draw still authorizes release"
        );
    }

    #[test]
    fn unknown_current_cover_or_replacement_visibility_never_retires_coverage() {
        for unknown_cover in [true, false] {
            let mut app = stand_in_app();
            let old = app
                .world_mut()
                .spawn((SwapStandIn::default(), SwapCoverageInCameraView(false)))
                .id();
            let (_, patches, quadrants) = detail_change(&mut app, old);
            for &patch in &patches {
                set_visibility(&mut app, patch, Visibility::Inherited, false);
                app.world_mut().entity_mut(patch).insert(SwapPrepared {
                    stand_in: Some(old),
                    since: 1,
                });
            }
            let unknown = if unknown_cover {
                quadrants[0]
            } else {
                patches[0]
            };
            app.world_mut()
                .entity_mut(unknown)
                .remove::<ViewVisibility>();
            app.update();
            assert!(alive(&app, old));
            assert_eq!(app.world().resource::<SwapBridgeStats>().live_holds, 1);
        }
    }

    #[test]
    fn visible_stand_in_keeps_prepared_unviewed_replacement_covered() {
        let mut app = stand_in_app();
        let stand_in = app
            .world_mut()
            .spawn((SwapStandIn::default(), SwapCoverageInCameraView(true)))
            .id();
        let (_, patches, _) = detail_change(&mut app, stand_in);
        for patch in patches {
            set_visibility(&mut app, patch, Visibility::Inherited, false);
            app.world_mut().entity_mut(patch).insert(SwapPrepared {
                stand_in: Some(stand_in),
                since: 1,
            });
        }
        app.update();
        assert!(alive(&app, stand_in));
        assert_eq!(app.world().resource::<SwapBridgeStats>().live_holds, 1);
    }

    #[test]
    fn failed_intermediate_stand_in_transfers_visible_coverage_without_changing_resident_root() {
        use crate::{
            streaming::StreamingWorld,
            world::{components::StreamedCellRoot, database::CellKey},
        };
        let mut app = stand_in_app();
        app.init_resource::<StreamingWorld>();
        let old = app
            .world_mut()
            .spawn((SwapStandIn::default(), SwapCoverageInCameraView(true)))
            .id();
        let (intermediate, failed_patches, old_quadrants) = detail_change(&mut app, old);
        app.world_mut()
            .entity_mut(intermediate)
            .insert((SwapStandIn::default(), SwapCoverageInCameraView(false)));
        app.world_mut()
            .entity_mut(failed_patches[0])
            .insert(crate::streaming::AssetFailed);
        let (current, current_patches, _) = detail_change(&mut app, intermediate);
        app.world_mut().entity_mut(current).insert(StreamedCellRoot);
        let key = CellKey::Interior(1);
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .set_resident_for_test(key, current);
        for &patch in &current_patches {
            set_visibility(&mut app, patch, Visibility::Inherited, false);
            app.world_mut().entity_mut(patch).insert(SwapPrepared {
                stand_in: Some(intermediate),
                since: 1,
            });
        }
        app.update();
        assert!(!alive(&app, intermediate));
        assert!(
            alive(&app, old),
            "the visible valid cover cannot yield to an undrawn replacement"
        );
        assert!(old_quadrants.iter().all(|&entity| alive(&app, entity)));
        assert_eq!(
            app.world().resource::<StreamingWorld>().resident_root(key),
            Some(current)
        );
        assert_eq!(app.world().resource::<SwapBridgeStats>().live_holds, 1);
        for &patch in &current_patches {
            assert_eq!(
                app.world().get::<SwapSurface>(patch).unwrap().stand_in(),
                Some(old)
            );
            assert_eq!(
                app.world().get::<SwapPrepared>(patch).unwrap().stand_in,
                Some(old)
            );
            app.world_mut().entity_mut(patch).insert(SwapDrawn);
        }
        app.update();
        assert!(!alive(&app, old));
        assert!(alive(&app, current));
        assert_eq!(
            app.world().resource::<StreamingWorld>().resident_root(key),
            Some(current)
        );
        assert_eq!(app.world().resource::<SwapBridgeStats>().live_holds, 0);
    }

    #[test]
    fn unviewed_unprepared_replacement_keeps_hold() {
        let mut app = stand_in_app();
        let stand_in = app
            .world_mut()
            .spawn((SwapStandIn::default(), SwapCoverageInCameraView(false)))
            .id();
        let (_, patches, _) = detail_change(&mut app, stand_in);
        for patch in patches {
            set_visibility(&mut app, patch, Visibility::Inherited, false);
        }
        app.update();
        assert!(alive(&app, stand_in));
        assert_eq!(app.world().resource::<SwapBridgeStats>().live_holds, 1);
    }

    /// Slow preparation must never punch a hole in visible terrain.
    #[test]
    fn a_slow_replacement_keeps_coverage_until_every_surface_draws() {
        let mut app = stand_in_app();
        let stand_in = app.world_mut().spawn(SwapStandIn::default()).id();
        let (_, patches, quadrants) = detail_change(&mut app, stand_in);
        for _ in 0..MAX_SWAP_HOLD_FRAMES * 3 {
            app.update();
            assert!(alive(&app, stand_in));
            assert!(quadrants.iter().all(|entity| alive(&app, *entity)));
        }
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .swap_hold_timeouts,
            1
        );
        for patch in &patches[..3] {
            app.world_mut().entity_mut(*patch).insert(SwapDrawn);
        }
        app.update();
        assert!(
            alive(&app, stand_in),
            "one undrawn patch still needs coverage"
        );
        app.world_mut().entity_mut(patches[3]).insert(SwapDrawn);
        app.update();
        assert!(!alive(&app, stand_in));
        assert!(quadrants.iter().all(|entity| !alive(&app, *entity)));
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .swap_hold_timeouts,
            1
        );
    }

    /// The new root is despawned before its terrain is drawn - the camera left the cell, or the cell
    /// retired. Nothing waits on the stand-in any more, so it goes with it rather than covering a
    /// cell nobody asked for. That is a release, not a failure.
    #[test]
    fn a_new_root_despawned_releases_the_stand_in() {
        let mut app = stand_in_app();
        let stand_in = app.world_mut().spawn(SwapStandIn::default()).id();
        let (new_root, _patches, _quadrants) = detail_change(&mut app, stand_in);
        app.update();
        assert!(alive(&app, stand_in), "the hold is live");

        app.world_mut().entity_mut(new_root).despawn();
        app.update();
        assert_eq!(app.world().resource::<SwapBridgeStats>().live_holds, 0);

        assert!(
            !alive(&app, stand_in),
            "the stand-in follows the root it covered for"
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .swap_hold_timeouts,
            0,
            "that is a release, not a hold that ran out"
        );
    }

    /// A detail change whose new root has no terrain declares no waiting surface, so nothing names
    /// the stand-in and the hold has nothing to cover: it ends on the first frame the hand-over
    /// runs, rather than waiting out the bound for a replacement that never comes.
    #[test]
    fn a_new_root_without_terrain_releases_at_once() {
        let mut app = stand_in_app();
        let stand_in = app.world_mut().spawn(SwapStandIn::default()).id();
        let _new_root = app.world_mut().spawn_empty().id();

        app.update();

        assert!(
            !alive(&app, stand_in),
            "nothing waits, so there is nothing to cover"
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .swap_hold_timeouts,
            0
        );
    }
}
