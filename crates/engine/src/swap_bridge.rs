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
//! Cost is O(waiting entities): the bridge iterates exactly the entities carrying the waiting
//! marker, never all terrain, and a surface leaves the set the frame it is confirmed drawn
//! (design-1091 §2.3, §5).

use bevy::pbr::{PreparedMaterial, SpecializedMaterialPipelineCache};
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

/// The render world has confirmed this surface drawable: its own specialized pipeline resolved
/// `Ok` while a view could see it, so that frame's queue pass drew it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct SwapDrawn;

/// How long a detail change may keep the root it replaced drawn, in frames of
/// [`release_detail_change_stand_ins`].
///
/// The bound is a safety net, not a deadline. The hand-over itself is the bridge's own latency -
/// two or three frames (design-1091 §2.2) - and what the hold waits for on top of it is the new
/// cell's terrain reaching the render world under the 16 MiB-a-frame upload budget. The design's
/// own figure (§3) is about 120 frames: roughly two seconds at 60 fps and 0.75 s at the portal
/// demo's 160 fps, one to two orders of magnitude above a normal swap, and short enough that a
/// stand-in left covering a replacement that never arrives is a visible count rather than a quiet
/// leak of a drawn cell root. Counted in frames rather than seconds because the swap's state is
/// counted in frames, so the hold behaves the same whatever the frame rate.
///
/// Outliving it is a failure, not a wait: the stand-in is despawned anyway, and the frame is
/// counted ([`crate::streaming::StreamingMetrics::swap_hold_timeouts`]) and warned about, so the
/// one way a hold can end without its replacement drawn is never silent.
pub const MAX_SWAP_HOLD_FRAMES: u32 = 120;

/// A root a detail change keeps drawn: the cell now draws the root that replaced this one, and
/// this one keeps covering the grid until every surface of the new root is drawn - or has
/// everything its draw needs and no view can see it, which needs no covering either.
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
    /// How many frames [`release_detail_change_stand_ins`] has held this root, bounded by
    /// [`MAX_SWAP_HOLD_FRAMES`].
    pub held_frames: u32,
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
        for (entity, mesh, surface, prepared) in query.iter() {
            let facts = if prepared.is_some() {
                SwapSurfaceFacts::assets_ready()
            } else {
                gather_surface_facts(
                    mesh,
                    surface,
                    &mesh_allocator,
                    &render_meshes,
                    &gpu_images,
                    &materials,
                )
            };
            let facts = SwapSurfaceFacts {
                pipeline_ok: pipeline_resolved(
                    MainEntity::from(entity),
                    &views,
                    &specialized,
                    &pipelines,
                ),
                ..facts
            };
            match swap_readiness(&facts) {
                SwapReadiness::Waiting => {}
                readiness => decisions.push(SwapDecision {
                    entity,
                    insert_prepared: prepared.is_none(),
                    drawn: readiness == SwapReadiness::Drawn,
                    stand_in: surface.stand_in(),
                }),
            }
        }
    }

    for decision in decisions.iter() {
        let mut entity = main_world.entity_mut(decision.entity);
        if decision.insert_prepared {
            entity.insert(SwapPrepared {
                stand_in: decision.stand_in,
                since: *frame,
            });
        }
        if decision.drawn {
            entity.insert(SwapDrawn);
        }
    }
}

/// One marker write the bridge decided on, applied after the query's borrow of the main world ends.
#[derive(Debug, Clone, Copy)]
struct SwapDecision {
    entity: Entity,
    /// Whether `SwapPrepared` still has to be written (it need not be re-written every frame a
    /// surface waits for its pipeline: that would churn change detection for nothing).
    insert_prepared: bool,
    drawn: bool,
    stand_in: Option<Entity>,
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
        material: materials.get(surface.material).is_some(),
        pipeline_ok: false,
    }
}

/// Whether the entity's specialized pipeline for a view that can see it resolves `Ok`.
///
/// The pipeline cache is keyed by view and by main-world entity, and Bevy specializes an entity
/// only for the views whose visible set holds it - but the visible set is read here too, so the
/// "a view can see it" half of the rule is a fact of this frame rather than an assumption about
/// Bevy's internals. An entity whose pipeline is `Queued` (or absent) is not drawn yet.
fn pipeline_resolved(
    entity: MainEntity,
    views: &Query<(&ExtractedView, &RenderVisibleEntities)>,
    specialized: &SpecializedMaterialPipelineCache,
    pipelines: &PipelineCache,
) -> bool {
    for (view, visible) in views.iter() {
        // The cheap half first: a view with no cache entry or no pipeline for the entity cannot
        // have drawn it, and costs one lookup each.
        let Some(cached) = specialized
            .get(&view.retained_view_entity)
            .and_then(|by_entity| by_entity.get(&entity))
            .copied()
        else {
            continue;
        };
        if !view_sees(visible, entity) {
            continue;
        }
        if matches!(
            pipelines.get_render_pipeline_state(cached),
            CachedPipelineState::Ok(_)
        ) {
            return true;
        }
    }
    false
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
        commands.entity(entity).try_remove::<SwapWaiting>();
        completed = completed.saturating_add(1);
    }
    stats.drawn_this_frame = completed;
}

/// Every waiting surface's coverage state: what it declares (its stand-in, if any), whether it is
/// revealed and what a view makes of it, and the two markers that say whether it covers itself.
type WaitingCoverage<'w, 's> = Query<
    'w,
    's,
    (
        &'static SwapSurface,
        &'static Visibility,
        Has<SwapDrawn>,
        Has<SwapPrepared>,
        &'static ViewVisibility,
    ),
    With<SwapWaiting>,
>;

/// The main-world hand-over of a detail change (design-1091 §3): the root the cell was drawn by
/// keeps covering it until the surfaces the cell now declares are covered, then goes through the
/// same `try_despawn` the commit on its own used to do in the frame it committed.
///
/// A surface is covered when the render world has confirmed it drawn, or when it has everything its
/// draw needs and no view can see it. The second half is the visibility rule (design-1091 §2.1): a
/// surface no view can see is never confirmed drawn, so a new root whose patches are all off-view
/// would hold its stand-in for ever. Nothing on screen needs covering then, so prepared-and-off-view
/// counts as covered - but only once the surface is *revealed*: a terrain quadrant is spawned
/// `Visibility::Hidden` and stays so until its textures are in `Assets<Image>`, which spans the
/// frames before the render world can have them, and its `ViewVisibility` is false throughout.
/// Reading that as "off-view" would drop the stand-in before the replacement can be drawn at all.
///
/// Cost is one pass over the waiting surfaces plus one over the live holds, and the live holds are
/// at most the cells crossing a detail boundary in one frame. Nothing is held at all in a settled
/// run, and the system returns before building the set.
fn release_detail_change_stand_ins(
    mut commands: Commands,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
    mut holds: Query<(Entity, &mut SwapStandIn)>,
    cells: Query<(&CellRef, Option<&ExteriorCellGrid>)>,
    waiting: WaitingCoverage,
    mut uncovered: Local<Vec<Entity>>,
) {
    if holds.is_empty() {
        return;
    }
    // The stand-ins a waiting surface still needs. Built from the surfaces' own declarations rather
    // than from the new root: a new root that is despawned, and one that declares no surface at all
    // (a cell with no terrain), both simply stop naming anything here.
    uncovered.clear();
    for (surface, surface_visibility, drawn, prepared, view) in &waiting {
        if drawn || (prepared && *surface_visibility != Visibility::Hidden && !view.get()) {
            continue;
        }
        if let Some(stand_in) = surface.stand_in()
            && !uncovered.contains(&stand_in)
        {
            uncovered.push(stand_in);
        }
    }
    for (stand_in, mut hold) in &mut holds {
        hold.held_frames = hold.held_frames.saturating_add(1);
        let covered = !uncovered.contains(&stand_in);
        if !covered && hold.held_frames < MAX_SWAP_HOLD_FRAMES {
            continue;
        }
        commands.entity(stand_in).try_despawn();
        if covered {
            continue;
        }
        metrics.swap_hold_timeouts = metrics.swap_hold_timeouts.saturating_add(1);
        profiler.increment("streaming/swap_hold_timeouts", 1);
        match cells.get(stand_in) {
            Ok((cell, grid)) => warn!(
                cell = format_args!("{:08X}", cell.0),
                grid = ?grid.map(|grid| grid.0),
                held_frames = hold.held_frames,
                "detail-change stand-in outlived the swap hold bound; despawning it anyway"
            ),
            Err(_) => warn!(
                stand_in = ?stand_in,
                held_frames = hold.held_frames,
                "detail-change stand-in outlived the swap hold bound; despawning it anyway"
            ),
        }
    }
}

/// Waiting surfaces that are prepared but not yet drawn.
type PreparedNotDrawn = (With<SwapWaiting>, With<SwapPrepared>, Without<SwapDrawn>);

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
    stats: Res<SwapBridgeStats>,
    mut profiler: ResMut<ProfilingState>,
) {
    profiler.set_gauge("swap/waiting", waiting.iter().count() as f64);
    profiler.set_gauge("swap/prepared_not_drawn", prepared.iter().count() as f64);
    profiler.set_gauge("swap/drawn_this_frame", f64::from(stats.drawn_this_frame));
}

/// Registers the render→main swap bridge: the extract system in the render app, the main-world
/// hand-over, and the C5 gauges.
pub struct SwapBridgePlugin;

impl Plugin for SwapBridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SwapBridgeStats>().add_systems(
            Update,
            (complete_drawn_swaps, release_detail_change_stand_ins),
        );
        // The gauges are measurement data: an ordinary play session installs none of them.
        let profiling = app
            .world()
            .get_resource::<EngineConfig>()
            .is_some_and(|config| config.profile_output_dir.is_some());
        if profiling {
            app.add_systems(Update, record_swap_gauges);
        }
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(ExtractSchedule, swap_bridge);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                        Visibility::Inherited,
                        ViewVisibility::VISIBLE,
                        ChildOf(new_root),
                    ))
                    .id()
            })
            .collect();
        let quadrants = (0..4)
            .map(|_| app.world_mut().spawn(ChildOf(stand_in)).id())
            .collect();
        (new_root, patches, quadrants)
    }

    /// Sets what the main world's visibility pass leaves on a patch: its own `Visibility` (a terrain
    /// quadrant is `Hidden` until its textures are in `Assets<Image>`) and whether a view saw it.
    fn set_visibility(app: &mut App, patch: Entity, own: Visibility, seen: bool) {
        app.world_mut().entity_mut(patch).insert((
            own,
            if seen {
                ViewVisibility::VISIBLE
            } else {
                ViewVisibility::HIDDEN
            },
        ));
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

    /// The visibility rule: a surface no view can see is never confirmed drawn, so a new root whose
    /// patches are all off-view would hold its stand-in for ever - while nothing on screen needs
    /// covering. Prepared and out of view therefore counts as covered, but only once the surface is
    /// revealed: a patch still `Visibility::Hidden` (its textures are not in `Assets<Image>` yet) is
    /// not one "no view can see", and its stand-in stays until the render world can draw it.
    #[test]
    fn a_prepared_surface_no_view_can_see_releases_its_stand_in() {
        let mut app = stand_in_app();

        let stand_in = app.world_mut().spawn(SwapStandIn::default()).id();
        let (_new_root, patches, _) = detail_change(&mut app, stand_in);
        for patch in &patches {
            app.world_mut().entity_mut(*patch).insert(SwapPrepared {
                stand_in: Some(stand_in),
                since: 1,
            });
            set_visibility(&mut app, *patch, Visibility::Inherited, false);
        }
        app.update();
        assert!(
            !alive(&app, stand_in),
            "prepared and out of view needs no covering"
        );

        let stand_in = app.world_mut().spawn(SwapStandIn::default()).id();
        let (_new_root, patches, _) = detail_change(&mut app, stand_in);
        for patch in &patches {
            app.world_mut().entity_mut(*patch).insert(SwapPrepared {
                stand_in: Some(stand_in),
                since: 1,
            });
            set_visibility(&mut app, *patch, Visibility::Inherited, true);
        }
        app.update();
        assert!(
            alive(&app, stand_in),
            "an undrawn surface a view can see keeps its covering"
        );

        let stand_in = app.world_mut().spawn(SwapStandIn::default()).id();
        let (_new_root, patches, _) = detail_change(&mut app, stand_in);
        for patch in &patches {
            app.world_mut().entity_mut(*patch).insert(SwapPrepared {
                stand_in: Some(stand_in),
                since: 1,
            });
            set_visibility(&mut app, *patch, Visibility::Hidden, false);
        }
        app.update();
        assert!(
            alive(&app, stand_in),
            "a patch that is not revealed yet is not one no view can see"
        );
    }

    /// Nothing waits for ever: a hold whose replacement never draws is released at
    /// [`MAX_SWAP_HOLD_FRAMES`] anyway, counted in the metrics and warned about - the loud end.
    #[test]
    fn a_hold_that_outlives_the_bound_is_despawned_and_counted() {
        let mut app = stand_in_app();
        let stand_in = app.world_mut().spawn(SwapStandIn::default()).id();
        // Patches that can never become prepared (they declare no assets at all) and are in view:
        // only the bound can end this hold.
        detail_change(&mut app, stand_in);

        for _ in 0..MAX_SWAP_HOLD_FRAMES - 1 {
            app.update();
            assert!(alive(&app, stand_in), "held within the bound");
        }
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .swap_hold_timeouts,
            0,
            "the last frame before the bound is not a failure"
        );

        app.update();

        assert!(!alive(&app, stand_in), "the bound releases the stand-in");
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .swap_hold_timeouts,
            1,
            "and the frame is counted"
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
