//! The systems of the destination readiness protocol ([`super::destination`]): the render world
//! evaluates every pending request after its queue step, the next extract copies the result into the
//! main world, and a main-world system combines it with residency, assets and the replacement
//! owner's handoff into a [`DestinationReport`].
//!
//! Frame order, on the render clock ([`RenderFrame`]): extract N increments the clock, copies the
//! pending requests into the render world and copies the evaluation of frame N-1 back; the render
//! schedule of frame N evaluates (after the queue systems have specialized and queued and the material
//! bind groups were prepared, before the passes draw);
//! extract N+1 reports it. So a report describes the previously evaluated frame and is labelled so.
//!
//! Cost: nothing runs for facts 3 and 4 while no request is pending. With a request pending the
//! render evaluation visits what the requested view will draw (its visible mesh entities, and the
//! entities of its dependent shadow views when shadows are required): O(visible), for the few
//! frames a consumer waits.

use super::{
    assets::LandingAssets,
    destination::{
        DestinationFacts, DestinationKey, DestinationReport, DestinationReports,
        DestinationRequest, DestinationRequests, DestinationState, Generation, Generations,
        HandoffReports, PassFamily, RenderFrame, RenderInvalidations, RenderProof, RequestId,
        Residency, destination_state, handoff_for_frame,
    },
};
use crate::streaming::{PendingUnder, StreamingWorld};
use bevy::{
    camera::visibility::RenderLayers,
    core_pipeline::{
        core_3d::{AlphaMask3d, Opaque3d, Transparent3d},
        prepass::{AlphaMask3dPrepass, Opaque3dPrepass},
    },
    ecs::system::SystemParam,
    pbr::{
        LightEntity, MaterialBindGroupAllocators, PreparedMaterial, RenderMaterialInstances,
        RenderMeshInstanceFlags, RenderMeshInstances, Shadow, SpecializedMaterialPipelineCache,
        SpecializedPrepassMaterialPipelineCache, SpecializedShadowMaterialPipelineCache,
        Transmissive3d, ViewLightEntities,
    },
    prelude::*,
    render::{
        ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems,
        erased_render_asset::ErasedRenderAssets,
        mesh::{RenderMesh, allocator::MeshAllocator},
        render_asset::RenderAssets,
        render_phase::{
            BinnedPhaseItem, BinnedRenderPhase, SortedPhaseItem, SortedRenderPhase,
            ViewBinnedRenderPhases, ViewSortedRenderPhases,
        },
        render_resource::{CachedPipelineState, CachedRenderPipelineId, PipelineCache},
        sync_world::MainEntity,
        view::{
            ExtractedView, RenderShadowMapVisibleEntities, RenderVisibleEntities,
            RenderVisibleEntitiesClass, RetainedViewEntity,
        },
    },
};
use std::collections::{HashMap, HashSet};

/// What the render world holds for one visible entity, as plain facts: the proof is a pure function
/// of these, tested without a GPU.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntityFacts {
    /// The mesh's vertex and index buffers exist in the allocator.
    pub mesh_allocated: bool,
    /// The mesh's render form is prepared.
    pub render_mesh: bool,
    /// The material is drawable: prepared, with a slab and a bind group the draw command can set
    /// (which needs every image it binds).
    pub material_prepared: bool,
    /// Per pass family (by [`PassFamily::index`]): `None` when the entity needs no pipeline in that
    /// family (the family is not required, or the material has no such pass); `Some(true)` when a
    /// queued pipeline resolves; `Some(false)` when it is missing.
    pub passes: [Option<bool>; 5],
}

/// Folds entity facts into one [`RenderProof`].
#[derive(Default, Debug)]
pub struct ProofAccumulator {
    proof: RenderProof,
    pass_missing: [bool; 5],
}

impl ProofAccumulator {
    /// Counts one entity.
    pub fn add(&mut self, facts: &EntityFacts) {
        self.proof.checked += 1;
        if !facts.mesh_allocated || !facts.render_mesh {
            self.proof.geometry_missing += 1;
        }
        if !facts.material_prepared {
            self.proof.material_missing += 1;
        }
        for (missing, pass) in self.pass_missing.iter_mut().zip(facts.passes) {
            *missing |= pass == Some(false);
        }
    }

    /// Marks a required family as missing without an entity to blame: the thing that would carry its
    /// pipelines does not exist yet.
    pub fn mark_pass_missing(&mut self, family: PassFamily) {
        self.pass_missing[family.index()] = true;
    }

    /// The proof, naming the first missing pass family in family order.
    pub fn finish(mut self) -> RenderProof {
        self.proof.pass_missing = PassFamily::ALL
            .into_iter()
            .find(|family| self.pass_missing[family.index()]);
        self.proof
    }
}

/// What the render world proved for one request in one render frame.
#[derive(Clone, Debug)]
pub struct EvaluatedProof {
    pub key: DestinationKey,
    /// The render frame whose queue step this describes.
    pub frame: RenderFrame,
    /// The world's generation the render world recorded when it extracted the request.
    pub extracted_generation: Generation,
    pub proof: RenderProof,
    /// The replacement owner's state for this key **in this frame** (fact 5), if it published one.
    pub handoff: Option<DestinationState>,
}

/// Main-world consumers run after this set to read the newly combined render report.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DestinationReportsUpdated;

/// The ordering seam of the render-world frame. The replacement owner adds its handoff publication
/// to [`DestinationSystems::PublishHandoff`] (after the queue and planning steps it depends on, which
/// it orders itself, in `RenderSystems::PhaseSort`); the bridge's evaluation is in
/// [`DestinationSystems::Evaluate`], which runs after it and after `RenderSystems::PrepareBindGroups`
/// (before the passes draw), so it reads this frame's final material bind groups. Nothing here names
/// the owner's code.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DestinationSystems {
    /// Handoff publication for the current render frame.
    PublishHandoff,
    /// Evaluation of the pending requests for the current render frame.
    Evaluate,
}

/// Main world: what the render world copied back at the last extract.
#[derive(Resource, Default, Debug)]
pub struct RenderResults {
    /// Whether a renderer exists at all. Without one (a headless run, a test) there is nothing to
    /// prove drawable, and facts 3 and 4 hold vacuously, as a settle's drawn count does.
    pub renderer: bool,
    /// The render frame of the last extract.
    pub frame: RenderFrame,
    proofs: HashMap<RequestId, EvaluatedProof>,
}

struct ExtractedRequest {
    id: RequestId,
    request: DestinationRequest,
    extracted_generation: Generation,
}

/// Render world: the requests as of this frame's extract.
#[derive(Resource, Default)]
pub(crate) struct ExtractedRequests {
    list: Vec<ExtractedRequest>,
}

impl ExtractedRequests {
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&DestinationRequest, Generation)> {
        self.list
            .iter()
            .map(|pending| (&pending.request, pending.extracted_generation))
    }
}

/// Render world: the newest evaluation of each pending request.
#[derive(Resource, Default)]
pub(crate) struct Evaluations {
    map: HashMap<RequestId, EvaluatedProof>,
}

/// Increments the render clock, copies the pending requests into the render world and the previous
/// evaluation (with the handoff and invalidation records of the requested keys) back into the main
/// world.
pub(crate) fn extract_destination_protocol(
    mut main_world: ResMut<MainWorld>,
    mut frame: ResMut<RenderFrame>,
    mut extracted: ResMut<ExtractedRequests>,
    mut evaluations: ResMut<Evaluations>,
    invalidations: Res<RenderInvalidations>,
) {
    frame.0 += 1;
    extracted.list.clear();
    {
        let requests = main_world.resource::<DestinationRequests>();
        let generations = main_world.resource::<Generations>();
        for (id, request) in requests.iter() {
            extracted.list.push(ExtractedRequest {
                id,
                request: request.clone(),
                extracted_generation: generations.current(request.key.world),
            });
        }
    }
    evaluations
        .map
        .retain(|id, _| extracted.list.iter().any(|request| request.id == *id));
    // The render world's invalidation record is mirrored whenever it moved, whether or not a request
    // is pending: a ready report stored earlier must see a late revocation.
    if invalidations.is_changed() {
        main_world
            .resource_mut::<DestinationReports>()
            .mirror_invalidations(&invalidations);
    }
    let mut results = main_world.resource_mut::<RenderResults>();
    results.frame = *frame;
    results.proofs.clear();
    for request in extracted.list.iter() {
        if let Some(proof) = evaluations.map.get(&request.id) {
            results.proofs.insert(request.id, proof.clone());
        }
    }
}

fn pipeline_ok(pipelines: &PipelineCache, id: CachedRenderPipelineId) -> bool {
    matches!(
        pipelines.get_render_pipeline_state(id),
        CachedPipelineState::Ok(_)
    )
}

/// The visible mesh entities of one view or subview: the CPU-culled list and the GPU-culled table.
fn visible_meshes(visible: &RenderVisibleEntities) -> impl Iterator<Item = MainEntity> + '_ {
    let class: Option<&RenderVisibleEntitiesClass> = visible.get::<Mesh3d>();
    class.into_iter().flat_map(|class| {
        class
            .entities_cpu_culling
            .iter()
            .map(|(_, main)| *main)
            .chain(class.entities_gpu_culling.keys().copied())
    })
}

/// The entities one render phase of one view holds right now: the batchable and unbatchable bins,
/// the multidrawable batch sets (through the read-only accessor the vendored `bevy_render` adds) and
/// the sorted items. An entity absent from all of them is not queued, whatever else the phase holds.
#[derive(Default)]
pub(crate) struct PhaseMembers {
    entities: HashSet<MainEntity>,
}

impl PhaseMembers {
    fn of_binned<BPI: BinnedPhaseItem>(phase: Option<&BinnedRenderPhase<BPI>>) -> Self {
        let mut members = Self::default();
        let Some(phase) = phase else {
            return members;
        };
        for bin in phase.batchable_meshes.values() {
            members.entities.extend(bin.entities().keys().copied());
        }
        for bin in phase.unbatchable_meshes.values() {
            members.entities.extend(bin.entities.keys().copied());
        }
        for set in phase.multidrawable_meshes.values() {
            members.entities.extend(set.entities());
        }
        for bin in phase.non_mesh_items.values() {
            members.entities.extend(bin.entities.keys().copied());
        }
        members
    }

    fn of_sorted<SPI: SortedPhaseItem>(phase: Option<&SortedRenderPhase<SPI>>) -> Self {
        let mut members = Self::default();
        if let Some(phase) = phase {
            members
                .entities
                .extend(phase.items.keys().map(|(_, main)| *main));
        }
        members
    }

    fn merge(&mut self, other: Self) {
        self.entities.extend(other.entities);
    }

    /// Whether `main` is queued in one of the phases.
    pub(crate) fn queued(&self, main: MainEntity) -> bool {
        self.entities.contains(&main)
    }
}

/// The render phases the proof reads, optional because a pass the app never added has none.
#[derive(SystemParam)]
pub(crate) struct QueuePhases<'w> {
    opaque: Option<Res<'w, ViewBinnedRenderPhases<Opaque3d>>>,
    alpha_mask: Option<Res<'w, ViewBinnedRenderPhases<AlphaMask3d>>>,
    transmissive: Option<Res<'w, ViewSortedRenderPhases<Transmissive3d>>>,
    transparent: Option<Res<'w, ViewSortedRenderPhases<Transparent3d>>>,
    opaque_prepass: Option<Res<'w, ViewBinnedRenderPhases<Opaque3dPrepass>>>,
    alpha_mask_prepass: Option<Res<'w, ViewBinnedRenderPhases<AlphaMask3dPrepass>>>,
    shadow: Option<Res<'w, ViewBinnedRenderPhases<Shadow>>>,
}

impl QueuePhases<'_> {
    /// What the view's main (forward) phases hold.
    pub(crate) fn forward(&self, view: &RetainedViewEntity) -> PhaseMembers {
        let mut members = PhaseMembers::of_binned(self.opaque.as_ref().and_then(|p| p.get(view)));
        members.merge(PhaseMembers::of_binned(
            self.alpha_mask.as_ref().and_then(|p| p.get(view)),
        ));
        members.merge(PhaseMembers::of_sorted(
            self.transmissive.as_ref().and_then(|p| p.get(view)),
        ));
        members.merge(PhaseMembers::of_sorted(
            self.transparent.as_ref().and_then(|p| p.get(view)),
        ));
        members
    }

    /// What the view's prepass phases hold.
    pub(crate) fn prepass(&self, view: &RetainedViewEntity) -> PhaseMembers {
        let mut members =
            PhaseMembers::of_binned(self.opaque_prepass.as_ref().and_then(|p| p.get(view)));
        members.merge(PhaseMembers::of_binned(
            self.alpha_mask_prepass.as_ref().and_then(|p| p.get(view)),
        ));
        members
    }

    /// What one shadow subview's phase holds.
    pub(crate) fn shadow(&self, view: &RetainedViewEntity) -> PhaseMembers {
        PhaseMembers::of_binned(self.shadow.as_ref().and_then(|p| p.get(view)))
    }
}

/// Whether the stock shadow queue places a mesh in a shadow view: the mesh casts shadows
/// (`RenderMeshInstanceFlags::SHADOW_CASTER`, cleared by `NotShadowCaster`) and its render layers
/// intersect the shadow view's own (`bevy_pbr` `queue_shadows`). A mesh the queue rejects for either
/// is not part of that view's domain: its missing pipeline is not a missing pass, and the
/// requirement that a member of the domain be queued is not relaxed.
fn shadow_required(
    casts_shadows: bool,
    mesh_layers: Option<&RenderLayers>,
    view_layers: Option<&RenderLayers>,
) -> bool {
    casts_shadows
        && view_layers
            .cloned()
            .unwrap_or_default()
            .intersects(&mesh_layers.cloned().unwrap_or_default())
}

/// The mesh and material resources the proof reads for one entity.
#[derive(SystemParam)]
struct DrawResources<'w> {
    mesh_instances: Option<Res<'w, RenderMeshInstances>>,
    material_instances: Option<Res<'w, RenderMaterialInstances>>,
    mesh_allocator: Res<'w, MeshAllocator>,
    render_meshes: Res<'w, RenderAssets<RenderMesh>>,
    materials: Res<'w, ErasedRenderAssets<PreparedMaterial>>,
    bind_groups: Option<Res<'w, MaterialBindGroupAllocators>>,
}

impl DrawResources<'_> {
    /// Whether the stock queue would place `main` in the shadow view whose own layers are
    /// `view_layers` ([`shadow_required`]). A mesh the render world has no queue data for yet is
    /// still required: the queue waits for it rather than rejecting it, and its missing geometry
    /// is counted by [`Self::entity_facts`].
    fn shadow_required(&self, main: MainEntity, view_layers: Option<&RenderLayers>) -> bool {
        let Some(data) = self
            .mesh_instances
            .as_ref()
            .and_then(|instances| instances.render_mesh_queue_data(main))
        else {
            return true;
        };
        shadow_required(
            data.flags()
                .contains(RenderMeshInstanceFlags::SHADOW_CASTER),
            data.render_layers.as_ref(),
            view_layers,
        )
    }

    /// The geometry and material facts of one entity, following what the draw commands themselves
    /// read: the mesh must be in the allocator and prepared; the material must be reachable through
    /// the same chain `SetMaterialBindGroup` walks (instance, allocator for the material type,
    /// prepared material, slab for its bind group, the bind group itself), or the draw is skipped.
    /// Returns the prepared material too, for the pass checks.
    fn entity_facts(&self, main: MainEntity) -> (EntityFacts, Option<&PreparedMaterial>) {
        let mesh = self
            .mesh_instances
            .as_ref()
            .and_then(|instances| instances.mesh_asset_id(main));
        let instance = self
            .material_instances
            .as_ref()
            .and_then(|instances| instances.instances.get(&main));
        let prepared = instance.and_then(|instance| self.materials.get(instance.asset_id));
        let bind_group_ready = instance.zip(prepared).is_some_and(|(instance, prepared)| {
            self.bind_groups
                .as_ref()
                .and_then(|allocators| allocators.get(&instance.asset_id.type_id()))
                .and_then(|allocator| allocator.get(prepared.binding.group))
                .is_some_and(|slab| slab.bind_group().is_some())
        });
        if !bind_group_ready {
            let step = match (instance, prepared) {
                (None, _) => "no material instance",
                (Some(_), None) => "material not prepared",
                (Some(instance), Some(prepared)) => match self
                    .bind_groups
                    .as_ref()
                    .and_then(|allocators| allocators.get(&instance.asset_id.type_id()))
                {
                    None => "no allocator for the material type",
                    Some(allocator) => match allocator.get(prepared.binding.group) {
                        None => "no slab for the binding group",
                        Some(_) => "slab has no bind group",
                    },
                },
            };
            debug!(entity = ?main, step, "destination proof: material not drawable");
        }
        let facts = EntityFacts {
            mesh_allocated: mesh.is_some_and(|id| self.mesh_allocator.mesh_slabs(&id).is_some()),
            render_mesh: mesh.is_some_and(|id| self.render_meshes.get(id).is_some()),
            material_prepared: bind_group_ready,
            passes: [None; 5],
        };
        (facts, prepared)
    }
}

/// Whether a required shadow family is still missing its domain. `subviews` is how many shadow
/// subviews depend on the view, `lights_prepared` whether the light preparation has run for the view
/// (it inserts `ViewLightEntities` every frame, with an empty list when no light applies), and
/// `cache_present` whether the shadow pipeline cache exists.
///
/// With subviews, the family needs its cache. Without any, the domain is **proven empty** (an
/// unlit interior, a camera whose render layers exclude the light) only once the lights were
/// prepared for the view; before that the lights are simply not extracted yet, which is waiting, not
/// an empty domain. An empty iterator alone never proves it.
fn shadow_domain_missing(subviews: usize, lights_prepared: bool, cache_present: bool) -> bool {
    if subviews > 0 {
        !cache_present
    } else {
        !lights_prepared
    }
}

/// Evaluates every pending request after the queue step: for the requested view, every visible mesh
/// entity must hold its buffers, mesh, a drawable material bind group and, in each required pass,
/// both a specialized pipeline that resolves and a place in the view's queued phase; with shadows
/// required, the same holds for the entities of the view's dependent shadow subviews. A required
/// pass whose cache is absent is missing, not vacuously fine. A view the render world does not
/// extract (inactive, not yet spawned) is not evaluated, and the request stays
/// [`super::destination::Missing::NotEvaluated`].
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn evaluate_destinations(
    extracted: Res<ExtractedRequests>,
    mut evaluations: ResMut<Evaluations>,
    frame: Res<RenderFrame>,
    draw: DrawResources,
    phases: QueuePhases,
    forward: Option<Res<SpecializedMaterialPipelineCache>>,
    prepass: Option<Res<SpecializedPrepassMaterialPipelineCache>>,
    shadow: Option<Res<SpecializedShadowMaterialPipelineCache>>,
    pipelines: Res<PipelineCache>,
    handoffs: Res<HandoffReports>,
    views: Query<(
        &ExtractedView,
        &RenderVisibleEntities,
        Option<&ViewLightEntities>,
    )>,
    lights: Query<&RenderShadowMapVisibleEntities>,
    shadow_views: Query<(&ExtractedView, Option<&RenderLayers>), With<LightEntity>>,
    mut timing: Local<crate::profiling::PeriodicTiming>,
) {
    if extracted.list.is_empty() {
        return;
    }
    if draw.mesh_instances.is_none() || draw.material_instances.is_none() {
        return;
    }
    perf_span!("swap::evaluate_destinations");
    let started = std::time::Instant::now();
    for pending in extracted.list.iter() {
        let request = &pending.request;
        let wanted = request.passes;
        let Some((view, visible, view_lights)) = views
            .iter()
            .find(|(view, _, _)| view.retained_view_entity == request.key.view.view)
        else {
            evaluations.map.remove(&pending.id);
            continue;
        };
        let mut proof = ProofAccumulator::default();
        let wants_prepass = wanted.contains(PassFamily::DepthPrepass)
            || wanted.contains(PassFamily::NormalPrepass)
            || wanted.contains(PassFamily::MotionPrepass);
        let forward_members = wanted
            .contains(PassFamily::Forward)
            .then(|| phases.forward(&view.retained_view_entity));
        let prepass_members = wants_prepass.then(|| phases.prepass(&view.retained_view_entity));
        for main in visible_meshes(visible) {
            let (mut facts, prepared) = draw.entity_facts(main);
            if let Some(members) = forward_members.as_ref() {
                let pipeline_ok = forward
                    .as_ref()
                    .and_then(|cache| cache.get(&view.retained_view_entity))
                    .and_then(|by_entity| by_entity.get(&main))
                    .is_some_and(|id| pipeline_ok(&pipelines, *id));
                facts.passes[PassFamily::Forward.index()] =
                    Some(pipeline_ok && members.queued(main));
            }
            // The prepass pipelines exist only for materials that have the pass: opaque and
            // alpha-masked with the prepass enabled, never transmissive. A material without it is
            // not missing a pipeline.
            let has_prepass = prepared.is_some_and(|prepared| {
                let properties = &prepared.properties;
                properties.prepass_enabled
                    && !properties.reads_view_transmission_texture
                    && matches!(
                        properties.alpha_mode,
                        AlphaMode::Opaque | AlphaMode::Mask(_) | AlphaMode::AlphaToCoverage
                    )
            });
            if let Some(members) = prepass_members.as_ref()
                && has_prepass
            {
                let queued = prepass
                    .as_ref()
                    .and_then(|cache| cache.get(&view.retained_view_entity))
                    .and_then(|by_entity| by_entity.get(&main))
                    .is_some_and(|(_, id, _)| pipeline_ok(&pipelines, *id))
                    && members.queued(main);
                for family in [
                    PassFamily::DepthPrepass,
                    PassFamily::NormalPrepass,
                    PassFamily::MotionPrepass,
                ] {
                    if wanted.contains(family) {
                        facts.passes[family.index()] = Some(queued);
                    }
                }
            }
            proof.add(&facts);
        }
        if wanted.contains(PassFamily::Shadow) {
            // The light views that depend on this view: a directional light's cascades carry the
            // camera as their auxiliary entity, a point light's cube faces carry a placeholder
            // and belong to every camera.
            let mut subviews = 0;
            for light in lights.iter() {
                for (retained, visible) in light.subviews.iter() {
                    let depends = retained.auxiliary_entity
                        == view.retained_view_entity.main_entity
                        || retained.auxiliary_entity == MainEntity::from(Entity::PLACEHOLDER);
                    if !depends {
                        continue;
                    }
                    subviews += 1;
                    let members = phases.shadow(retained);
                    // The layers of this very shadow view (cascade, cube face or spot), found by
                    // its retained key: a point or spot light's views are shared between cameras,
                    // so the camera's layers say nothing about what they queue.
                    let view_layers = shadow_views
                        .iter()
                        .find(|(shadow_view, _)| shadow_view.retained_view_entity == *retained)
                        .and_then(|(_, layers)| layers);
                    for main in visible_meshes(visible) {
                        let (mut facts, prepared) = draw.entity_facts(main);
                        if !prepared.is_none_or(|prepared| prepared.properties.shadows_enabled)
                            || !draw.shadow_required(main, view_layers)
                        {
                            continue;
                        }
                        let pipeline_ok = shadow
                            .as_ref()
                            .and_then(|cache| cache.get(retained))
                            .and_then(|by_entity| by_entity.get(&main))
                            .is_some_and(|(id, _)| pipeline_ok(&pipelines, *id));
                        facts.passes[PassFamily::Shadow.index()] =
                            Some(pipeline_ok && members.queued(main));
                        proof.add(&facts);
                    }
                }
            }
            if shadow_domain_missing(subviews, view_lights.is_some(), shadow.is_some()) {
                proof.mark_pass_missing(PassFamily::Shadow);
            }
        }
        evaluations.map.insert(
            pending.id,
            EvaluatedProof {
                key: request.key,
                frame: *frame,
                extracted_generation: pending.extracted_generation,
                proof: proof.finish(),
                handoff: handoff_for_frame(&handoffs, &request.key, *frame),
            },
        );
    }
    timing.record("swap/evaluate", started.elapsed());
}

/// Combines what the render world proved with residency, assets and the handoff into one report per
/// pending request. Runs only while a request is pending, so an idle run pays nothing.
#[allow(clippy::too_many_arguments)]
fn update_destination_reports(
    requests: Res<DestinationRequests>,
    generations: Res<Generations>,
    results: Res<RenderResults>,
    streaming: Option<Res<StreamingWorld>>,
    pending_under: PendingUnder,
    assets: LandingAssets,
    mut reports: ResMut<DestinationReports>,
    mut timing: Local<crate::profiling::PeriodicTiming>,
    mut profiler: Option<ResMut<crate::profiling::ProfilingState>>,
) {
    perf_span!("swap/reports");
    let started = std::time::Instant::now();
    for (id, request) in requests.iter() {
        let (mut residency, mut assets_pending) =
            streaming
                .as_deref()
                .map_or((Residency::Resident, 0), |streaming| {
                    let mut residency = Residency::Resident;
                    let mut assets_pending = 0;
                    for cell in request.coverage.iter() {
                        if streaming.is_failed(*cell) {
                            residency = Residency::Failed(*cell);
                            break;
                        }
                        match streaming.resident_root(*cell) {
                            Some(root) => {
                                if pending_under.failed(root) > 0
                                    && residency == Residency::Resident
                                {
                                    residency = Residency::AssetsFailed(*cell);
                                }
                                assets_pending +=
                                    pending_under.count(root) + assets.inspect_under(root).waiting;
                            }
                            None => residency = Residency::Pending,
                        }
                    }
                    (residency, assets_pending)
                });
        let evaluated = results
            .proofs
            .get(&id)
            .filter(|proof| proof.key == request.key);
        let render = if results.renderer {
            evaluated.map(|proof| proof.proof)
        } else {
            Some(RenderProof::default())
        };
        let facts = DestinationFacts {
            key: request.key,
            handoff: request.handoff,
            current_generation: generations.current(request.key.world),
            extracted_generation: evaluated
                .map_or(request.key.generation, |proof| proof.extracted_generation),
            evaluated: evaluated.map_or(results.frame, |proof| proof.frame),
            last_invalidation: reports.invalidation(request.key.world),
            residency,
            assets_pending,
            render,
            handoff_report: evaluated.and_then(|proof| proof.handoff.clone()),
        };
        reports.set(DestinationReport {
            key: request.key,
            evaluated_render_frame: facts.evaluated,
            state: destination_state(&facts),
        });
    }
    let elapsed = started.elapsed();
    timing.record("swap/reports", elapsed);
    if let Some(profiler) = profiler.as_mut() {
        profiler.record_ms("swap/reports", elapsed.as_secs_f64() * 1000.0);
        profiler.set_gauge("swap/report_requests", requests.iter().count() as f64);
    }
}

/// Installs the protocol: the resources both worlds hold, the render-world evaluation and the
/// main-world combination.
pub struct DestinationReadinessPlugin;

impl Plugin for DestinationReadinessPlugin {
    fn build(&self, app: &mut App) {
        let renderer = app.get_sub_app(RenderApp).is_some();
        app.init_resource::<Generations>()
            .init_resource::<DestinationRequests>()
            .init_resource::<DestinationReports>()
            .insert_resource(RenderResults {
                renderer,
                ..default()
            })
            .add_systems(
                Update,
                update_destination_reports
                    .in_set(DestinationReportsUpdated)
                    .run_if(|requests: Res<DestinationRequests>| !requests.is_empty()),
            );
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<RenderFrame>()
                .init_resource::<ExtractedRequests>()
                .init_resource::<Evaluations>()
                .init_resource::<RenderInvalidations>()
                .init_resource::<HandoffReports>()
                .configure_sets(
                    Render,
                    (
                        DestinationSystems::PublishHandoff.in_set(RenderSystems::PhaseSort),
                        // After the material bind groups were rebuilt this frame: a slab that
                        // gained a material has no bind group until `PrepareBindGroups`, so an
                        // earlier evaluation would call a drawable material missing.
                        DestinationSystems::Evaluate
                            .after(RenderSystems::PrepareBindGroups)
                            .before(RenderSystems::Render),
                    )
                        .chain(),
                )
                .add_systems(ExtractSchedule, extract_destination_protocol)
                .add_systems(
                    Render,
                    evaluate_destinations.in_set(DestinationSystems::Evaluate),
                );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swap_bridge::destination::PassFamilies;

    fn drawn(passes: [Option<bool>; 5]) -> EntityFacts {
        EntityFacts {
            mesh_allocated: true,
            render_mesh: true,
            material_prepared: true,
            passes,
        }
    }

    #[test]
    fn a_fully_prepared_entity_adds_no_missing_fact() {
        let mut proof = ProofAccumulator::default();
        proof.add(&drawn([Some(true), Some(true), None, None, Some(true)]));
        let proof = proof.finish();
        assert_eq!(proof.checked, 1);
        assert_eq!(
            (
                proof.geometry_missing,
                proof.material_missing,
                proof.pass_missing
            ),
            (0, 0, None)
        );
    }

    #[test]
    fn missing_geometry_material_and_passes_are_counted_and_the_first_pass_is_named() {
        let mut proof = ProofAccumulator::default();
        proof.add(&EntityFacts {
            mesh_allocated: false,
            ..drawn([Some(true), None, None, None, None])
        });
        proof.add(&EntityFacts {
            render_mesh: false,
            ..drawn([Some(true), None, None, None, None])
        });
        proof.add(&EntityFacts {
            material_prepared: false,
            ..drawn([Some(true), None, None, None, None])
        });
        proof.add(&drawn([Some(true), None, None, None, Some(false)]));
        proof.add(&drawn([Some(true), Some(false), None, None, Some(false)]));
        let proof = proof.finish();
        assert_eq!(proof.checked, 5);
        assert_eq!(proof.geometry_missing, 2);
        assert_eq!(proof.material_missing, 1);
        assert_eq!(proof.pass_missing, Some(PassFamily::DepthPrepass));
    }

    /// A required family with nothing to carry its pipelines is missing, with no entity to blame.
    #[test]
    fn a_required_family_marked_missing_is_named_with_no_entities() {
        let mut proof = ProofAccumulator::default();
        proof.mark_pass_missing(PassFamily::Shadow);
        assert_eq!(proof.finish().pass_missing, Some(PassFamily::Shadow));
    }

    fn members(entities: &[u32]) -> PhaseMembers {
        PhaseMembers {
            entities: entities.iter().map(|bits| main_entity(*bits)).collect(),
        }
    }

    fn main_entity(bits: u32) -> MainEntity {
        MainEntity::from(Entity::from_raw_u32(bits).unwrap())
    }

    /// An entity is queued only when a phase holds it: an unrelated queued batch proves nothing.
    #[test]
    fn queue_membership_needs_the_entity_itself() {
        let phase = members(&[1, 2]);
        assert!(phase.queued(main_entity(1)));
        assert!(
            !phase.queued(main_entity(3)),
            "another entity is queued, not this one"
        );
        let mut merged = members(&[1]);
        merged.merge(members(&[5]));
        assert!(merged.queued(main_entity(5)) && !merged.queued(main_entity(6)));
    }

    #[test]
    fn real_non_mesh_phase_membership_tracks_the_exact_carrier_and_removal() {
        use bevy::{
            core_pipeline::core_3d::{Opaque3dBatchSetKey, Opaque3dBinKey},
            render::{
                batching::gpu_preprocessing::GpuPreprocessingMode,
                mesh::allocator::MeshSlabs,
                render_phase::{BinnedRenderPhaseType, DrawFunctionId, InputUniformIndex},
            },
        };
        let carrier = main_entity(1);
        let other = main_entity(2);
        let view = RetainedViewEntity::new(main_entity(10), None, 0);
        let mut phases = ViewBinnedRenderPhases::<Opaque3d>::default();
        // BinnedRenderPhase::new is private; this is Bevy's public retained-view constructor.
        phases.prepare_for_new_frame(view, GpuPreprocessingMode::None);
        let phase = phases.get_mut(&view).unwrap();
        let batch_key = Opaque3dBatchSetKey {
            pipeline: CachedRenderPipelineId::INVALID,
            draw_function: DrawFunctionId(0),
            material_bind_group_index: None,
            slabs: MeshSlabs::default(),
            lightmap_slab: None,
        };
        let mut meshes = Assets::<Mesh>::default();
        let mesh = meshes.add(Mesh::new(
            bevy::mesh::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        ));
        let bin_key = Opaque3dBinKey {
            asset_id: mesh.id().untyped(),
        };
        phase.add(
            batch_key.clone(),
            bin_key.clone(),
            (Entity::from_raw_u32(101).unwrap(), carrier),
            InputUniformIndex::default(),
            BinnedRenderPhaseType::NonMesh,
        );
        assert_eq!(phase.non_mesh_items.len(), 1);
        assert!(phase.batchable_meshes.is_empty());
        assert!(phase.unbatchable_meshes.is_empty());
        assert!(phase.multidrawable_meshes.is_empty());
        let members = PhaseMembers::of_binned(Some(phase));
        assert!(members.queued(carrier));
        assert!(!members.queued(other));
        // Retained bins survive frame preparation; proof must read real membership.
        phase.prepare_for_new_frame();
        assert!(PhaseMembers::of_binned(Some(phase)).queued(carrier));
        phase.add(
            batch_key,
            bin_key,
            (Entity::from_raw_u32(102).unwrap(), other),
            InputUniformIndex::default(),
            BinnedRenderPhaseType::NonMesh,
        );
        phase.remove(carrier);
        let members = PhaseMembers::of_binned(Some(phase));
        assert!(
            !members.queued(carrier),
            "a remaining non-mesh bin cannot certify another carrier"
        );
        assert!(members.queued(other));
        phase.remove(other);
        assert!(phase.non_mesh_items.is_empty());
        let members = PhaseMembers::of_binned(Some(phase));
        assert!(!members.queued(carrier) && !members.queued(other));
    }

    /// The shadow domain is missing until the lights were prepared for the view; a prepared view
    /// with no dependent subview is a proven empty domain; subviews need their cache.
    #[test]
    fn an_empty_shadow_domain_counts_only_once_the_lights_were_prepared() {
        assert!(
            shadow_domain_missing(0, false, true),
            "lights not extracted yet"
        );
        assert!(
            !shadow_domain_missing(0, true, false),
            "proven empty, nothing to cache"
        );
        assert!(
            shadow_domain_missing(2, true, false),
            "casters but no cache"
        );
        assert!(!shadow_domain_missing(2, true, true));
    }

    /// The stock shadow queue rejects a mesh that does not cast shadows or whose layers do not
    /// intersect the shadow view's own; such a mesh is outside that view's domain even when the
    /// specialization cache holds a pipeline for it, and a member of the domain is still required
    /// to be queued.
    #[test]
    fn a_mesh_the_shadow_queue_rejects_is_outside_the_domain() {
        let layer = |n: usize| RenderLayers::layer(n);
        // Same cache entry, wrong layer: not required.
        assert!(!shadow_required(true, Some(&layer(1)), Some(&layer(0))));
        // Right layer: required.
        assert!(shadow_required(true, Some(&layer(1)), Some(&layer(1))));
        // Default layers on both sides intersect (layer 0).
        assert!(shadow_required(true, None, None));
        // A view with no layer component is layer 0; a mesh on another layer is rejected.
        assert!(!shadow_required(true, Some(&layer(2)), None));
        // NotShadowCaster: rejected whatever the layers.
        assert!(!shadow_required(false, None, None));
        // A shared (point or spot) view carries its own layers, whatever the camera's.
        assert!(shadow_required(
            true,
            Some(&layer(3)),
            Some(&layer(3).with(4))
        ));
    }

    /// A pass the entity does not need (`None`) is never reported missing.
    #[test]
    fn a_pass_an_entity_does_not_need_is_not_missing() {
        let mut proof = ProofAccumulator::default();
        proof.add(&drawn([None; 5]));
        assert_eq!(proof.finish().pass_missing, None);
    }

    #[test]
    fn the_wanted_families_come_out_in_family_order() {
        let wanted = PassFamilies::NONE
            .with(PassFamily::Shadow)
            .with(PassFamily::Forward)
            .with(PassFamily::NormalPrepass);
        assert_eq!(
            wanted.iter().collect::<Vec<_>>(),
            vec![
                PassFamily::Forward,
                PassFamily::NormalPrepass,
                PassFamily::Shadow
            ]
        );
        assert!(
            PassFamily::ALL
                .iter()
                .enumerate()
                .all(|(index, family)| family.index() == index)
        );
    }

    /// Without a renderer (no `RenderApp`) facts 3 and 4 hold vacuously and the report comes from
    /// residency and assets alone; the whole chain runs in a headless app.
    #[test]
    fn a_headless_app_reports_ready_once_residency_and_assets_hold() {
        use crate::{
            streaming::StreamingWorld,
            swap_bridge::destination::{
                DestinationRequest, Handoff, InvalidationCause, ViewKey, WorldId,
            },
            world::database::CellKey,
        };
        use bevy::render::view::RetainedViewEntity;
        use std::sync::Arc;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_resource::<StreamingWorld>()
            .add_plugins(DestinationReadinessPlugin);
        let cell = CellKey::Interior(9);
        let world = WorldId(1);
        let generation = app
            .world_mut()
            .resource_mut::<Generations>()
            .bump(world, InvalidationCause::Composition);
        let key = DestinationKey {
            view: ViewKey {
                view: RetainedViewEntity::new(Entity::from_raw_u32(1).unwrap().into(), None, 0),
                incarnation: 0,
            },
            world,
            generation,
        };
        app.world_mut()
            .resource_mut::<DestinationRequests>()
            .request(DestinationRequest {
                key,
                passes: PassFamilies::NONE.with(PassFamily::Forward),
                coverage: Arc::from(vec![cell]),
                handoff: Handoff::NotRequired,
            });
        app.update();
        let generations = app.world().resource::<Generations>();
        let reports = app.world().resource::<DestinationReports>();
        assert!(
            !reports.is_ready(&key, generations),
            "{}",
            reports.describe(&key)
        );
        let root = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .set_resident_for_test(cell, root);
        app.update();
        let generations = app.world().resource::<Generations>();
        let reports = app.world().resource::<DestinationReports>();
        assert!(
            reports.is_ready(&key, generations),
            "{}",
            reports.describe(&key)
        );
        // A bump of the world's generation voids the report.
        app.world_mut()
            .resource_mut::<Generations>()
            .bump(world, InvalidationCause::Candidate);
        let generations = app.world().resource::<Generations>();
        let reports = app.world().resource::<DestinationReports>();
        assert!(!reports.is_ready(&key, generations));
    }

    /// A resident cell whose only trace of a failed model is a hidden child with `AssetFailed`:
    /// nothing is pending and no visible mesh is missing, yet the destination is not ready.
    #[test]
    fn a_resident_cell_with_a_hidden_failed_asset_reports_failed_not_ready() {
        use crate::{
            streaming::{AssetFailed, StreamingWorld},
            swap_bridge::destination::{
                DestinationRequest, Failure, Handoff, InvalidationCause, ViewKey, WorldId,
            },
            world::database::CellKey,
        };
        use bevy::render::view::RetainedViewEntity;
        use std::sync::Arc;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_resource::<StreamingWorld>()
            .add_plugins(DestinationReadinessPlugin);
        let cell = CellKey::Interior(9);
        let world = WorldId(1);
        let generation = app
            .world_mut()
            .resource_mut::<Generations>()
            .bump(world, InvalidationCause::Composition);
        let key = DestinationKey {
            view: ViewKey {
                view: RetainedViewEntity::new(Entity::from_raw_u32(1).unwrap().into(), None, 0),
                incarnation: 0,
            },
            world,
            generation,
        };
        app.world_mut()
            .resource_mut::<DestinationRequests>()
            .request(DestinationRequest {
                key,
                passes: PassFamilies::NONE.with(PassFamily::Forward),
                coverage: Arc::from(vec![cell]),
                handoff: Handoff::NotRequired,
            });
        let root = app.world_mut().spawn_empty().id();
        let model = app.world_mut().spawn(ChildOf(root)).id();
        app.world_mut()
            .spawn((Visibility::Hidden, AssetFailed, ChildOf(model)));
        app.world_mut()
            .resource_mut::<StreamingWorld>()
            .set_resident_for_test(cell, root);
        app.update();
        let generations = app.world().resource::<Generations>();
        let reports = app.world().resource::<DestinationReports>();
        assert!(
            !reports.is_ready(&key, generations),
            "{}",
            reports.describe(&key)
        );
        assert_eq!(
            reports.get(&key).map(|report| report.state.clone()),
            Some(DestinationState::Failed(Failure::AssetFailed(cell)))
        );
    }
}
