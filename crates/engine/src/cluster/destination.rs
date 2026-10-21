//! Same-frame destination handoff over the representation selected before stock queueing.
//! This publishes proof only; it never changes a queue, mask or draw at this late point.
use super::*;
use crate::swap_bridge::{
    destination::*,
    systems::{
        DestinationSystems, ExtractedRequests, PhaseMembers, QueuePhases,
        extract_destination_protocol,
    },
};
use bevy::{
    pbr::{ExtractedPointLight, PointAndSpotLightViewEntities},
    render::MainWorld,
};
type LocalShadowLights<'w, 's> = Query<
    'w,
    's,
    (
        &'static MainEntity,
        &'static ExtractedPointLight,
        &'static PointAndSpotLightViewEntities,
    ),
>;

#[derive(Default, Resource)]
struct Published {
    keys: HashSet<DestinationKey>,
}

pub(super) fn install(app: &mut App) {
    app.insert_resource(RequiredHandoff);
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render
        .init_resource::<Published>()
        .add_systems(
            ExtractSchedule,
            observe_candidates
                .after(extract_sources)
                .after(extract_batch_materials)
                .after(extract_destination_protocol),
        )
        .add_systems(
            Render,
            publish
                .after(finalize_terrain_clips)
                .after(RenderSystems::PrepareBindGroups)
                .before(DestinationSystems::Evaluate)
                .before(RenderSystems::Render),
        );
}

/// Eligibility/composition changes invalidate old reports; becoming drawable does
/// not invalidate its own new Ready report on every pipeline-preparation step.
fn observe_candidates(
    sources: Res<Sources>,
    materials: Res<ClusterBatchMaterials>,
    scene: Option<Res<ClusterScene>>,
    mut main: ResMut<MainWorld>,
    frame: Option<Res<RenderFrame>>,
    invalidations: Option<ResMut<RenderInvalidations>>,
    mut previous: Local<Option<(u64, u64, HashSet<WorldId>)>>,
) {
    if previous
        .as_ref()
        .is_some_and(|(version, material_version, _)| {
            *version == sources.candidate_version
                && *material_version == materials.version
                && !scene.as_ref().is_some_and(|scene| scene.is_changed())
        })
    {
        return;
    }
    debug!(
        source_version = sources.version,
        candidate_version = sources.candidate_version,
        material_version = materials.version,
        previous_candidate_version = previous.as_ref().map(|(source, _, _)| *source),
        previous_material_version = previous.as_ref().map(|(_, material, _)| *material),
        scene_changed = scene.as_ref().is_some_and(|scene| scene.is_changed()),
        "cluster destination candidate inputs changed"
    );
    let worlds: HashSet<_> = sources
        .certificate
        .as_ref()
        .into_iter()
        .flat_map(|certificate| certificate.records())
        .map(|(_, record)| WorldId::exterior(record.worldspace))
        .chain(
            scene
                .as_ref()
                .into_iter()
                .flat_map(|scene| scene.terrain_blocks.iter())
                .map(|block| WorldId::exterior(block.worldspace_id)),
        )
        .chain(
            sources
                .worlds
                .values()
                .flatten()
                .copied()
                .map(WorldId::exterior),
        )
        .collect();
    let global = previous
        .as_ref()
        .is_none_or(|(_, old_materials, _)| *old_materials != materials.version)
        || scene.as_ref().is_some_and(|scene| scene.is_changed());
    let candidate_changed = previous
        .as_ref()
        .is_none_or(|(version, _, _)| *version != sources.candidate_version);
    let mut changed = HashSet::new();
    if global || (candidate_changed && sources.candidate_worlds.all) {
        changed.extend(&worlds);
        if let Some((_, _, old)) = previous.as_ref() {
            changed.extend(old);
        }
    } else if candidate_changed {
        changed.extend(
            sources
                .candidate_worlds
                .worlds
                .iter()
                .copied()
                .map(WorldId::exterior),
        );
    }
    debug!(
        ?changed,
        global,
        candidate_all = sources.candidate_worlds.all,
        "cluster destination affected worlds"
    );
    if bevy::log::tracing::enabled!(bevy::log::Level::DEBUG)
        && let Some(requests) = main.get_resource::<DestinationRequests>()
    {
        for (_, request) in requests.iter() {
            debug!(key = ?request.key, coverage = ?request.coverage,
                "cluster destination invalidation coverage");
        }
    }
    if let (Some(frame), Some(mut invalidations), Some(mut generations)) =
        (frame, invalidations, main.get_resource_mut::<Generations>())
    {
        for world in changed {
            generations.bump(world, InvalidationCause::Candidate);
            invalidations.revoke(world, *frame, InvalidationCause::Candidate);
        }
    }
    *previous = Some((sources.candidate_version, materials.version, worlds));
}

fn selected(view: &ViewGpu, index: u32) -> bool {
    let offset = (index / 32) as usize * 4;
    view.instance_mask_bytes
        .get(offset..offset + 4)
        .is_some_and(|bytes| {
            u32::from_le_bytes(bytes.try_into().unwrap()) & (1 << (index % 32)) != 0
        })
}

struct ViewMembers {
    forward: PhaseMembers,
    depth: PhaseMembers,
    shadow: PhaseMembers,
}
/// Exact resources the later RenderGraph cull encoder reads. Grouping them
/// keeps publish within the system-function parameter limit.
#[derive(bevy::ecs::system::SystemParam)]
struct CullProofResources<'w> {
    far: Option<Res<'w, crate::far_terrain_shadows::render::State>>,
    frame: Res<'w, ClusterFrame>,
    pipelines: Res<'w, ClusterPipelines>,
    opaque: Res<'w, ViewBinnedRenderPhases<Opaque3d>>,
    shadow: Res<'w, ViewBinnedRenderPhases<Shadow>>,
}

struct DrawProof<'a, 'w> {
    gpu: &'a ClusterGpu,
    cull: &'a CullProofResources<'w>,
    cache: &'a PipelineCache,
    materials: &'a ErasedRenderAssets<PreparedMaterial>,
    allocators: &'a MaterialBindGroupAllocators,
    carriers: &'a HashMap<u32, MainEntity>,
    phases: &'a QueuePhases<'w>,
}
impl DrawProof<'_, '_> {
    fn members(&self, key: &RetainedViewEntity) -> ViewMembers {
        ViewMembers {
            forward: self.phases.forward(key),
            depth: self.phases.prepass(key),
            shadow: self.phases.shadow(key),
        }
    }
    fn batch(
        &self,
        key: &RetainedViewEntity,
        batch: usize,
        shadow: bool,
        prepass: bool,
        members: &ViewMembers,
    ) -> bool {
        let Some(view) = self.gpu.views.get(key) else {
            return false;
        };
        let material = self
            .gpu
            .batches
            .get(batch)
            .and_then(|batch| batch.material)
            .and_then(|id| self.materials.get(id));
        if shadow && material.is_some_and(|material| !material.properties.shadows_enabled) {
            return true;
        }
        // This is a pre-RenderGraph proof: require the exact dependencies and
        // active-view predicate the encoder will use, never a previous epoch.
        if prepared_cluster_cull(self.gpu, &self.cull.frame, &self.cull.pipelines, self.cache)
            .is_none()
            || !cluster_cull_view_eligible(
                view,
                self.cull.opaque.contains_key(key),
                self.cull.shadow.contains_key(key),
            )
        {
            return false;
        }
        // Optional shadow preparation is not geometry readiness. `batch_ready`
        // checks the exact queued variant: ordinary terrain remains drawable
        // while optional canonical shadows are waiting or unavailable.
        if !batch_ready(
            self.gpu,
            view,
            batch,
            shadow,
            prepass,
            (
                self.cache,
                self.materials,
                self.allocators,
                self.cull.far.as_deref(),
            ),
        ) {
            return false;
        }
        let Some(group) = table_group(view, batch) else {
            return false;
        };
        if shadow {
            return self
                .carriers
                .get(&group)
                .is_some_and(|main| members.shadow.queued(*main));
        }
        let Some(draw) = view.draws.get(&group) else {
            return false;
        };
        let Some(rep) = view.plan_rep(draw.representative) else {
            return false;
        };
        members.forward.queued(rep.main_entity)
            && (!prepass
                || material.is_some_and(|material| !material.properties.prepass_enabled)
                || members.depth.queued(rep.main_entity))
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn publish(
    gpu: Option<Res<ClusterGpu>>,
    sources: Res<Sources>,
    requests: Option<Res<ExtractedRequests>>,
    frame: Option<Res<RenderFrame>>,
    reports: Option<ResMut<HandoffReports>>,
    mut published: ResMut<Published>,
    cache: Res<PipelineCache>,
    cull: CullProofResources,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    allocators: Res<MaterialBindGroupAllocators>,
    phases: QueuePhases,
    carriers: Query<(&ClusterBatch, &MainEntity)>,
    views: Query<(Entity, &ExtractedView, Option<&LightEntity>)>,
    local_shadow_lights: LocalShadowLights,
) {
    perf_span!("cluster::destination::publish");
    let (Some(requests), Some(frame), Some(mut reports)) = (requests, frame, reports) else {
        return;
    };
    let wanted: HashSet<_> = requests
        .iter()
        .filter(|(request, _)| request.handoff == Handoff::Required)
        .map(|(request, _)| request.key)
        .collect();
    for old in published.keys.difference(&wanted) {
        reports.clear(old);
    }
    published.keys = wanted;
    // Withdraw obsolete reports above even when no request needs a new proof.
    if published.keys.is_empty() {
        return;
    }
    let carrier_ids = carriers
        .iter()
        .map(|(marker, main)| (marker.index, *main))
        .collect();
    for (request, generation) in requests
        .iter()
        .filter(|(request, _)| request.handoff == Handoff::Required)
    {
        perf_span!("cluster::destination::request");
        let mut state = DestinationState::Waiting(Missing::Handoff);
        if request.key.generation != generation {
            state = DestinationState::Stale;
        } else if let Some(gpu) = gpu.as_deref() {
            let same = match (&gpu.certificate, &sources.certificate) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            };
            if same
                && let Some((_, view, _)) = views.iter().find(|(_, view, light)| {
                    light.is_none() && view.retained_view_entity == request.key.view.view
                })
            {
                let key = view.retained_view_entity;
                let world = sources.worlds.get(&key.main_entity);
                if request.key.world.0 & (1 << 63) != 0 {
                    // The certified domains are exterior only. Interior geometry is
                    // intentionally stock, whose actual passes the bridge proves.
                    if world == Some(&None) {
                        state = DestinationState::Ready;
                    }
                } else if let Ok(worldspace) = u32::try_from(request.key.world.0)
                    && world == Some(&Some(worldspace))
                {
                    let proof = DrawProof {
                        gpu,
                        cull: &cull,
                        cache: &cache,
                        materials: &materials,
                        allocators: &allocators,
                        carriers: &carrier_ids,
                        phases: &phases,
                    };
                    if destination_ready(
                        &proof,
                        &sources,
                        request,
                        worldspace,
                        &views,
                        &local_shadow_lights,
                    ) {
                        state = DestinationState::Ready;
                    }
                }
            }
        }
        let report = HandoffReport {
            key: request.key,
            evaluated_render_frame: *frame,
            state,
        };
        let state = report.state.clone();
        if reports.publish(report) {
            debug!(?request.key, ?state, "cluster destination handoff changed");
        }
    }
}

/// Cluster-only handoff is legal only for Complete reference coverage. Partial
/// replacement needs every validated stock-only source primitive instantiated;
/// the generic bridge still proves their actual visible queues and passes.
fn replacement_coverage_ready(
    certificate: &super::super::super::reader::CertifiedPackage,
    sources: &Sources,
    reference: u32,
    stock_exists: bool,
) -> bool {
    certificate.reference_complete(reference)
        || (stock_exists
            && certificate
                .unclustered_primitives(reference)
                .is_some_and(|expected| {
                    expected.iter().all(|primitive| {
                        sources
                            .stock_primitives
                            .contains_key(&(reference, *primitive))
                    })
                }))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectPass {
    RetainedStock,
    ClusterRequired,
    OutsideDomain,
}

fn object_pass(
    stock_exists: bool,
    streaming_retired: bool,
    qualified_retired_domain: Option<bool>,
    cluster_selected: bool,
) -> ObjectPass {
    if streaming_retired {
        if qualified_retired_domain == Some(false) {
            ObjectPass::OutsideDomain
        } else {
            ObjectPass::ClusterRequired
        }
    } else if stock_exists && !cluster_selected {
        ObjectPass::RetainedStock
    } else {
        ObjectPass::ClusterRequired
    }
}

/// Only qualified current sources with matching actual material and a current
/// per-consumer selection can prove exclusion by layers/caster properties.
fn retired_domain(
    proof: &DrawProof,
    sources: &Sources,
    view: &ViewGpu,
    record: &super::super::super::reader::RecordKey,
    reference: u32,
    index: Option<u32>,
) -> Option<bool> {
    if !sources.retired_refs.contains(&reference) {
        return None;
    }
    let main = sources.bound.get(record)?;
    let actual = sources.retired_materials.get(main).copied().flatten()?;
    let expected = proof
        .gpu
        .batches
        .get(proof.gpu.placed.get(index? as usize)?.batch as usize)?
        .material?;
    if actual != expected {
        return None;
    }
    let selection = view.selection.as_ref()?;
    if selection.stamp[0] != sources.version
        || selection.stamp[1] != proof.gpu.layout_epoch
        || selection.stamp[2] != proof.gpu.generation
    {
        return None;
    }
    Some(selection.visible.contains(main))
}

fn destination_ready(
    proof: &DrawProof,
    sources: &Sources,
    request: &DestinationRequest,
    world: u32,
    views: &Query<(Entity, &ExtractedView, Option<&LightEntity>)>,
    local_shadow_lights: &LocalShadowLights,
) -> bool {
    let key = request.key.view.view;
    let Some(view) = proof.gpu.views.get(&key) else {
        return false;
    };
    let Some(certificate) = proof.gpu.certificate.as_ref() else {
        return false;
    };
    let prepass = [
        PassFamily::DepthPrepass,
        PassFamily::NormalPrepass,
        PassFamily::MotionPrepass,
    ]
    .into_iter()
    .any(|family| request.passes.contains(family));
    let members = proof.members(&key);
    let mut local_shadow_views = HashSet::new();
    if request.passes.contains(PassFamily::Shadow) {
        let Some(Some(dependencies)) = sources.light_dependencies.get(&key.main_entity) else {
            return false;
        };
        for dependency in dependencies {
            let Some((_, light, light_views)) = local_shadow_lights
                .iter()
                .find(|(main, _, _)| *main == dependency)
            else {
                // A main-world light whose render metadata has not arrived is unknown.
                return false;
            };
            if !light.shadow_maps_enabled {
                continue;
            }
            let Some(owner) = sources.worlds.get(dependency) else {
                return false;
            };
            if *owner != Some(world) {
                // It actually affects this camera, so foreign ownership cannot
                // silently turn a required shadow domain into an empty one.
                return false;
            }
            let expected = if light.spot_light_angles.is_some() {
                1
            } else {
                bevy::camera::primitives::CUBE_MAP_FACES.len()
            };
            if light_views.len() != expected {
                return false;
            }
            for entity in light_views.iter() {
                if !views.get(*entity).is_ok_and(|(_, _, light)| {
                    matches!(
                        light,
                        Some(LightEntity::Point { .. } | LightEntity::Spot { .. })
                    )
                }) {
                    return false;
                }
                local_shadow_views.insert(*entity);
            }
        }
    }
    let shadows: Vec<_> = views
        .iter()
        .filter(|(entity, view, light)| match light {
            Some(LightEntity::Directional { .. }) => {
                view.retained_view_entity.auxiliary_entity == key.main_entity
            }
            Some(_) => local_shadow_views.contains(entity),
            None => false,
        })
        .map(|(_, view, _)| {
            (
                view.retained_view_entity,
                proof.members(&view.retained_view_entity),
            )
        })
        .collect();
    for cell in request.coverage.iter() {
        let CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        } = *cell
        else {
            return false;
        };
        if worldspace_id != world {
            return false;
        }
        let grid = IVec2::new(grid_x, grid_y);
        for (record, identity) in certificate.records_in_placed_cell(world, grid) {
            let index = proof.gpu.object_indices.get(record).copied();
            let streaming_retired = sources.retired_present.contains(&identity.reference);
            let stock_exists = sources.present.contains(&identity.reference) && !streaming_retired;
            // An invalid retired source cannot inherit an empty generic stock
            // proof. It waits until it qualifies again or truly despawns.
            if streaming_retired && !sources.retired_refs.contains(&identity.reference) {
                return false;
            }
            // Stock is pinned whenever a runtime candidate is ineligible. The
            // generic bridge proves every visible retained stock mesh and pass.
            if object_pass(
                stock_exists,
                streaming_retired,
                retired_domain(proof, sources, view, record, identity.reference, index),
                index.is_some_and(|index| selected(view, index)),
            ) == ObjectPass::ClusterRequired
            {
                if !replacement_coverage_ready(
                    certificate,
                    sources,
                    identity.reference,
                    stock_exists,
                ) {
                    return false;
                }
                let Some(index) = index else {
                    return false;
                };
                if !selected(view, index) {
                    return false;
                }
                let batch = proof.gpu.placed[index as usize].batch as usize;
                if !proof.batch(&key, batch, false, prepass, &members) {
                    return false;
                }
            }
            if request.passes.contains(PassFamily::Shadow) {
                for (shadow_key, shadow_members) in &shadows {
                    let Some(shadow_view) = proof.gpu.views.get(shadow_key) else {
                        return false;
                    };
                    let material = index
                        .and_then(|index| {
                            proof
                                .gpu
                                .batches
                                .get(proof.gpu.placed[index as usize].batch as usize)
                        })
                        .and_then(|batch| batch.material)
                        .and_then(|id| proof.materials.get(id));
                    if material.is_some_and(|material| !material.properties.shadows_enabled) {
                        continue;
                    }
                    if object_pass(
                        stock_exists,
                        streaming_retired,
                        retired_domain(
                            proof,
                            sources,
                            shadow_view,
                            record,
                            identity.reference,
                            index,
                        ),
                        index.is_some_and(|index| selected(shadow_view, index)),
                    ) == ObjectPass::ClusterRequired
                    {
                        if !replacement_coverage_ready(
                            certificate,
                            sources,
                            identity.reference,
                            stock_exists,
                        ) {
                            return false;
                        }
                        let Some(index) = index else {
                            return false;
                        };
                        if !selected(shadow_view, index)
                            || !proof.batch(
                                shadow_key,
                                proof.gpu.placed[index as usize].batch as usize,
                                true,
                                false,
                                shadow_members,
                            )
                        {
                            return false;
                        }
                    }
                }
            }
        }
        for (block_index, block) in proof
            .gpu
            .terrain_blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.worldspace_id == world && block.cells.contains(&grid))
        {
            let cell_ready = |key: &RetainedViewEntity,
                              shadow: bool,
                              needs_depth: bool,
                              members: &ViewMembers| {
                let Some(candidate) = proof.gpu.views.get(key) else {
                    return false;
                };
                let Some(inputs) = candidate.terrain_clip_inputs.as_ref().filter(|inputs| {
                    inputs.world == Some(world) && inputs.origin == sources.origin
                }) else {
                    return false;
                };
                terrain_pass_ready(
                    inputs,
                    grid,
                    block_index as u32,
                    shadow,
                    needs_depth,
                    block.batch.is_some_and(|batch| {
                        proof.batch(key, batch as usize, shadow, needs_depth, members)
                    }),
                )
            };
            if !cell_ready(&key, false, prepass, &members) {
                return false;
            }
            if request.passes.contains(PassFamily::Shadow)
                && shadows
                    .iter()
                    .any(|(key, members)| !cell_ready(key, true, false, members))
            {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod partial_coverage_tests {
    use super::*;
    use crate::cluster::reader::producer_fixture_tests::{published, published_for_loader};

    #[test]
    fn partial_cluster_only_waits_for_every_rejected_source_primitive() {
        let directory = published();
        let certificate =
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap();
        let mut sources = Sources::default();
        let reference = 0x100;
        assert!(!replacement_coverage_ready(
            &certificate,
            &sources,
            reference,
            false
        ));
        assert!(!replacement_coverage_ready(
            &certificate,
            &sources,
            reference,
            true
        ));
        sources
            .stock_primitives
            .insert((reference, 0), Entity::from_raw_u32(1).unwrap().into());
        assert!(!replacement_coverage_ready(
            &certificate,
            &sources,
            reference,
            true
        ));
        sources
            .stock_primitives
            .insert((reference, 1), Entity::from_raw_u32(2).unwrap().into());
        assert!(replacement_coverage_ready(
            &certificate,
            &sources,
            reference,
            true
        ));
        assert!(!replacement_coverage_ready(
            &certificate,
            &sources,
            reference,
            false
        ));
        sources.stock_primitives.remove(&(reference, 1));
        assert!(!replacement_coverage_ready(
            &certificate,
            &sources,
            reference,
            true
        ));
    }

    #[test]
    fn complete_real_reference_needs_no_stock_piece_but_unknown_never_qualifies() {
        let directory = published_for_loader();
        let certificate =
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap();
        let sources = Sources::default();
        assert!(replacement_coverage_ready(
            &certificate,
            &sources,
            0x100,
            false
        ));
        assert!(!replacement_coverage_ready(
            &certificate,
            &sources,
            0x999,
            false
        ));
        assert!(!replacement_coverage_ready(
            &certificate,
            &sources,
            0x999,
            true
        ));
    }
}

/// Stock coverage is current queued/drawable coverage for this actual pass.
/// Early selection alone never proves a late material or shadow pipeline.
fn terrain_pass_ready(
    inputs: &TerrainClipInputs,
    grid: IVec2,
    block: u32,
    shadow: bool,
    needs_depth: bool,
    cluster_drawable: bool,
) -> bool {
    let forward = (0..4u8).all(|quadrant| inputs.forward_covered.contains(&(grid, quadrant)));
    let depth = (0..4u8).all(|quadrant| inputs.depth_covered.contains(&(grid, quadrant)));
    let stock = if shadow {
        depth
    } else {
        forward && (!needs_depth || depth)
    };
    stock || (inputs.ready_blocks.contains(&block) && cluster_drawable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_clip_cache_rechecks_exact_target_offset_even_with_unchanged_grid_and_far_nonce() {
        let grid_origin = IVec2::new(-4, 8);
        let offset = super::super::render_origin_offset(grid_origin);
        let inputs = TerrainClipInputs {
            world: Some(12),
            origin: grid_origin,
            forward_covered: HashSet::new(),
            depth_covered: HashSet::new(),
            ready_blocks: [0].into_iter().collect(),
            far_revision: Some((7, 0)),
            target_offset: offset.to_array().map(f32::to_bits),
        };
        assert!(inputs.target_offset_matches(offset));
        assert!(!inputs.target_offset_matches(offset + Vec3::X));
        assert!(!inputs.target_offset_matches(Vec3::new(offset.x, -0.0, offset.z)));
        assert!(!inputs.target_offset_matches(Vec3::new(offset.x, f32::NAN, offset.z)));
        // Same grid and F1 nonce deliberately remain unchanged: only the actual
        // render target offset distinguishes these cache inputs.
        assert_eq!(inputs.origin, grid_origin);
        assert_eq!(inputs.far_revision, Some((7, 0)));
    }

    #[test]
    fn terrain_selection_does_not_prove_a_missing_shadow_or_depth_draw() {
        let grid = IVec2::new(7, -9);
        let mut inputs = TerrainClipInputs {
            far_revision: None,
            target_offset: [0; 3],
            world: Some(12),
            origin: IVec2::ZERO,
            forward_covered: (0..4).map(|q| (grid, q)).collect(),
            depth_covered: HashSet::new(),
            ready_blocks: [3].into_iter().collect(),
        };
        assert!(terrain_pass_ready(&inputs, grid, 3, false, false, false));
        assert!(!terrain_pass_ready(&inputs, grid, 3, false, true, false));
        assert!(!terrain_pass_ready(&inputs, grid, 3, true, false, false));
        assert!(terrain_pass_ready(&inputs, grid, 3, true, false, true));
        assert!(!terrain_pass_ready(&inputs, grid, 4, true, false, true));
        inputs.depth_covered = (0..4).map(|q| (grid, q)).collect();
        assert!(terrain_pass_ready(&inputs, grid, 4, true, false, false));
        inputs.depth_covered.remove(&(grid, 2));
        inputs.depth_covered.insert((grid + IVec2::X, 2));
        assert!(!terrain_pass_ready(&inputs, grid, 4, true, false, false));
    }
}

#[cfg(test)]
mod retired_pass_regressions {
    use super::*;

    #[test]
    fn retirement_never_inherits_empty_stock_proof() {
        assert_eq!(
            object_pass(true, false, None, false),
            ObjectPass::RetainedStock
        );
        assert_eq!(
            object_pass(false, true, None, false),
            ObjectPass::ClusterRequired
        );
        assert_eq!(
            object_pass(false, true, Some(true), false),
            ObjectPass::ClusterRequired
        );
        assert_eq!(
            object_pass(false, true, Some(true), true),
            ObjectPass::ClusterRequired
        );
        // A qualified per-view layer/caster exclusion needs neither draw.
        assert_eq!(
            object_pass(false, true, Some(false), false),
            ObjectPass::OutsideDomain
        );
        assert_eq!(
            object_pass(false, false, None, false),
            ObjectPass::ClusterRequired
        );
    }
}

#[cfg(test)]
mod terrain_candidate_regressions {
    use super::*;
    use crate::streaming::TerrainPrimitiveIdentity;
    use bevy::ecs::schedule::Schedule;
    use shared::lod::LodTier;

    const WORLD: u32 = 12;
    const GRID: IVec2 = IVec2::new(7, -9);
    fn main(id: u32) -> MainEntity {
        Entity::from_raw_u32(id).unwrap().into()
    }
    fn near_map() -> HashMap<MainEntity, TerrainPrimitiveIdentity> {
        (0..4u8)
            .map(|quadrant| {
                (
                    main(100 + u32::from(quadrant)),
                    TerrainPrimitiveIdentity {
                        worldspace_id: WORLD,
                        grid: GRID,
                        quadrant,
                        tier: None,
                    },
                )
            })
            .collect()
    }

    struct Harness {
        render: World,
        observer: Schedule,
        key: DestinationKey,
    }
    impl Harness {
        fn ready_on_stock() -> Self {
            Self::ready_on_stock_with_map(near_map())
        }
        fn ready_on_stock_with_map(terrain: HashMap<MainEntity, TerrainPrimitiveIdentity>) -> Self {
            let inputs = TerrainClipInputs {
                far_revision: None,
                target_offset: [0; 3],
                world: Some(WORLD),
                origin: IVec2::ZERO,
                forward_covered: (0..4).map(|q| (GRID, q)).collect(),
                depth_covered: (0..4).map(|q| (GRID, q)).collect(),
                ready_blocks: HashSet::new(),
            };
            assert!(
                terrain_pass_ready(&inputs, GRID, 3, false, true, false),
                "Ready is stock-only, with no ready cluster block"
            );
            let mut render = World::new();
            let sources = Sources {
                worlds: HashMap::from([(main(1), Some(WORLD))]),
                terrain_visible: terrain.keys().copied().collect(),
                terrain: terrain.into(),
                ..Default::default()
            };
            render.insert_resource(sources);
            render.init_resource::<ClusterBatchMaterials>();
            render.insert_resource(RenderFrame(1));
            render.init_resource::<RenderInvalidations>();
            let mut main_world = MainWorld::default();
            main_world.init_resource::<Generations>();
            main_world.init_resource::<DestinationReports>();
            render.insert_resource(main_world);
            let mut observer = Schedule::default();
            observer.add_systems(observe_candidates);
            observer.run(&mut render);
            let generation = render
                .resource::<MainWorld>()
                .resource::<Generations>()
                .current(WorldId::exterior(WORLD));
            let key = DestinationKey {
                view: ViewKey {
                    view: RetainedViewEntity::new(main(1), None, 0),
                    incarnation: 1,
                },
                world: WorldId::exterior(WORLD),
                generation,
            };
            render
                .resource_mut::<MainWorld>()
                .resource_mut::<DestinationReports>()
                .set(DestinationReport {
                    key,
                    evaluated_render_frame: RenderFrame(10),
                    state: DestinationState::Ready,
                });
            render.insert_resource(RenderFrame(11));
            Self {
                render,
                observer,
                key,
            }
        }
        fn ready(&self) -> bool {
            let main = self.render.resource::<MainWorld>();
            main.resource::<DestinationReports>()
                .is_ready(&self.key, main.resource::<Generations>())
        }
        fn apply_terrain(
            &mut self,
            current: HashMap<MainEntity, TerrainPrimitiveIdentity>,
        ) -> TerrainMapChange {
            let visible = self
                .render
                .resource::<Sources>()
                .terrain_visible
                .iter()
                .filter(|entity| current.contains_key(*entity))
                .copied()
                .collect();
            self.apply_inputs(current, visible)
        }
        fn apply_inputs(
            &mut self,
            current: HashMap<MainEntity, TerrainPrimitiveIdentity>,
            visible: HashSet<MainEntity>,
        ) -> TerrainMapChange {
            let change = {
                let mut sources = self.render.resource_mut::<Sources>();
                let change = terrain_map_change(&sources.terrain, &current);
                let (visible_changed, withdrawn) =
                    terrain_visibility_change(&sources.terrain_visible, &visible);
                if change != TerrainMapChange::Unchanged || visible_changed {
                    let delta = {
                        let old = sources.candidate_inputs();
                        let new = CandidateInputs {
                            terrain: &current,
                            terrain_visible: &visible,
                            ..sources.candidate_inputs()
                        };
                        candidate_worlds(&old, &new)
                    };
                    sources.record_change(change == TerrainMapChange::Invalidating || withdrawn);
                    sources.candidate_worlds = delta;
                    sources.terrain = current.into();
                    sources.terrain_visible = visible;
                }
                change
            };
            self.observer.run(&mut self.render);
            change
        }
    }

    #[test]
    fn far_arrival_updates_selection_without_invalidating_prior_stock_ready() {
        let mut h = Harness::ready_on_stock();
        assert!(h.ready());
        let (selection, candidate) = {
            let sources = h.render.resource::<Sources>();
            (sources.version, sources.candidate_version)
        };
        let mut current = near_map();
        current.insert(
            main(999),
            TerrainPrimitiveIdentity {
                worldspace_id: WORLD,
                grid: GRID + IVec2::X,
                quadrant: 0,
                tier: Some(LodTier::Tier8),
            },
        );
        assert_eq!(h.apply_terrain(current), TerrainMapChange::FarAdditionsOnly);
        let sources = h.render.resource::<Sources>();
        assert_eq!(sources.version, selection + 1);
        assert_eq!(sources.candidate_version, candidate);
        assert!(
            h.ready(),
            "old report remains valid without any newly evaluated report"
        );
        assert_eq!(
            h.render
                .resource::<MainWorld>()
                .resource::<DestinationReports>()
                .get(&h.key)
                .unwrap()
                .evaluated_render_frame,
            RenderFrame(10)
        );
    }

    #[test]
    fn terrain_withdrawal_reassignment_and_near_arrival_revoke_before_a_new_report() {
        for mutation in ["withdraw", "reassign", "near_addition", "far_withdraw"] {
            let mut h = Harness::ready_on_stock();
            let mut current = near_map();
            match mutation {
                "withdraw" => {
                    current.remove(&main(100));
                }
                "reassign" => {
                    current.get_mut(&main(100)).unwrap().grid += IVec2::Y;
                }
                "near_addition" => {
                    current.insert(
                        main(998),
                        TerrainPrimitiveIdentity {
                            worldspace_id: WORLD,
                            grid: GRID + IVec2::X,
                            quadrant: 0,
                            tier: None,
                        },
                    );
                }
                "far_withdraw" => {
                    let far = TerrainPrimitiveIdentity {
                        worldspace_id: WORLD,
                        grid: GRID + IVec2::X,
                        quadrant: 0,
                        tier: Some(LodTier::Tier16),
                    };
                    let mut arrival = current.clone();
                    arrival.insert(main(999), far);
                    assert_eq!(h.apply_terrain(arrival), TerrainMapChange::FarAdditionsOnly);
                }
                _ => unreachable!(),
            }
            assert!(h.ready());
            assert_eq!(
                h.apply_terrain(current),
                TerrainMapChange::Invalidating,
                "{mutation}"
            );
            assert!(!h.ready(), "old Ready must fail immediately for {mutation}");
            let main = h.render.resource::<MainWorld>();
            assert_eq!(
                main.resource::<DestinationReports>()
                    .get(&h.key)
                    .unwrap()
                    .state,
                DestinationState::Ready,
                "the stored Ready is deliberately not overwritten by a newer evaluation"
            );
        }
    }

    #[test]
    fn other_world_candidate_churn_preserves_ready_and_delta_is_consumed_once() {
        let mut h = Harness::ready_on_stock();
        let foreign_world = WORLD + 1;
        h.render
            .resource_mut::<Sources>()
            .worlds
            .insert(main(2), Some(foreign_world));
        h.render.resource_mut::<Sources>().record_change(true);
        h.render.resource_mut::<Sources>().candidate_worlds = CandidateWorlds {
            all: false,
            worlds: HashSet::from([foreign_world]),
        };
        h.observer.run(&mut h.render);
        assert!(
            h.ready(),
            "a source-world change cannot stale a different destination"
        );
        let generation = h
            .render
            .resource::<MainWorld>()
            .resource::<Generations>()
            .current(WorldId::exterior(foreign_world));
        h.observer.run(&mut h.render);
        assert_eq!(
            h.render
                .resource::<MainWorld>()
                .resource::<Generations>()
                .current(WorldId::exterior(foreign_world)),
            generation
        );
        assert!(h.ready());
        // A later global material change still revokes it without a new source version.
        h.render.resource_mut::<ClusterBatchMaterials>().version += 1;
        h.observer.run(&mut h.render);
        assert!(!h.ready());
    }

    #[test]
    fn candidate_and_material_paths_still_revoke_existing_ready_reports() {
        let mut h = Harness::ready_on_stock();
        h.render.resource_mut::<Sources>().record_change(true);
        h.observer.run(&mut h.render);
        assert!(!h.ready());
        let mut h = Harness::ready_on_stock();
        h.render.resource_mut::<ClusterBatchMaterials>().version += 1;
        h.observer.run(&mut h.render);
        assert!(!h.ready());
    }

    #[test]
    fn unchanged_terrain_does_not_advance_either_counter() {
        let mut h = Harness::ready_on_stock();
        assert_eq!(h.apply_terrain(near_map()), TerrainMapChange::Unchanged);
        let sources = h.render.resource::<Sources>();
        assert_eq!((sources.version, sources.candidate_version), (0, 0));
        assert!(h.ready());
    }
    fn far_stock_map() -> HashMap<MainEntity, TerrainPrimitiveIdentity> {
        near_map()
            .into_iter()
            .map(|(entity, mut identity)| {
                identity.tier = Some(LodTier::Tier16);
                (entity, identity)
            })
            .collect()
    }

    #[test]
    fn finer_far_arrival_hiding_the_only_old_stock_revokes_stored_ready() {
        let old = far_stock_map();
        let mut h = Harness::ready_on_stock_with_map(old.clone());
        assert!(h.ready());
        let mut current = old;
        let mut new_visible = HashSet::new();
        for quadrant in 0..4u8 {
            let entity = main(200 + u32::from(quadrant));
            current.insert(
                entity,
                TerrainPrimitiveIdentity {
                    worldspace_id: WORLD,
                    grid: GRID,
                    quadrant,
                    tier: Some(LodTier::Tier8),
                },
            );
            new_visible.insert(entity);
        }
        // Identity map is a strict FAR-only superset, but visibility switches
        // from drawable old stock to new stock whose required passes are absent.
        assert_eq!(
            terrain_map_change(&h.render.resource::<Sources>().terrain, &current),
            TerrainMapChange::FarAdditionsOnly
        );
        let not_prepared = TerrainClipInputs {
            far_revision: None,
            target_offset: [0; 3],
            world: Some(WORLD),
            origin: IVec2::ZERO,
            forward_covered: HashSet::new(),
            depth_covered: HashSet::new(),
            ready_blocks: HashSet::new(),
        };
        assert!(!terrain_pass_ready(
            &not_prepared,
            GRID,
            3,
            false,
            true,
            false
        ));
        assert_eq!(
            h.apply_inputs(current, new_visible),
            TerrainMapChange::FarAdditionsOnly
        );
        assert!(
            !h.ready(),
            "old stock-only Ready must be rejected before the new Waiting is mirrored"
        );
        assert_eq!(
            h.render
                .resource::<MainWorld>()
                .resource::<DestinationReports>()
                .get(&h.key)
                .unwrap()
                .state,
            DestinationState::Ready,
            "the test intentionally preserves the older report rather than replacing it"
        );
    }

    #[test]
    fn unchanged_identities_with_raw_visible_member_lost_revoke_stored_ready() {
        let current = far_stock_map();
        let mut h = Harness::ready_on_stock_with_map(current.clone());
        let mut visible: HashSet<_> = current.keys().copied().collect();
        visible.remove(&main(100));
        assert_eq!(
            h.apply_inputs(current, visible),
            TerrainMapChange::Unchanged
        );
        assert!(!h.ready());
        let sources = h.render.resource::<Sources>();
        assert_eq!((sources.version, sources.candidate_version), (1, 1));
    }

    #[test]
    fn far_and_visible_additions_preserving_old_visible_sources_do_not_revoke() {
        let old = far_stock_map();
        let mut h = Harness::ready_on_stock_with_map(old.clone());
        let old_visible: HashSet<_> = old.keys().copied().collect();
        let mut current = old;
        current.insert(
            main(999),
            TerrainPrimitiveIdentity {
                worldspace_id: WORLD,
                grid: GRID + IVec2::X,
                quadrant: 0,
                tier: Some(LodTier::Tier8),
            },
        );
        assert_eq!(
            h.apply_inputs(current.clone(), old_visible.clone()),
            TerrainMapChange::FarAdditionsOnly
        );
        assert!(h.ready());
        let mut all_visible = old_visible;
        all_visible.insert(main(999));
        assert_eq!(
            h.apply_inputs(current, all_visible),
            TerrainMapChange::Unchanged
        );
        assert!(h.ready());
        let sources = h.render.resource::<Sources>();
        assert_eq!((sources.version, sources.candidate_version), (2, 0));
    }
}
