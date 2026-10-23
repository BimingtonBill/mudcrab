//! Forward-only optional binding. Depth/shadow data groups stay unchanged.
use super::*;
use crate::far_terrain_shadows::render::{Prepared, State};
use bevy::render::render_resource::BufferId;
use std::sync::Arc;
pub(super) struct Binding {
    pub(super) key: RetainedViewEntity,
    pub(super) prepared: Arc<Prepared>,
    buffers: [BufferId; 7],
    /// Diagnostic only: distinguishes an append after this frame's queue.
    queued_epoch: u64,
    pub(super) layout: BindGroupLayoutDescriptor,
    group: BindGroup,
}
pub(super) fn current(gpu: &ClusterGpu, view: &ViewGpu, state: Option<&State>) -> bool {
    let Some(binding) = &view.far else {
        return false;
    };
    binding.buffers == geometry_buffers(gpu, view)
        && gpu
            .certificate
            .as_ref()
            .is_some_and(|package| binding.prepared.source.matches_package(package))
        && state
            .and_then(|state| state.active.get(&binding.key))
            .is_some_and(|prepared| Arc::ptr_eq(prepared, &binding.prepared))
}
fn geometry_buffers(gpu: &ClusterGpu, view: &ViewGpu) -> [BufferId; 7] {
    // These allocations are sized for the entire scene. Appends change their
    // contents and the selection layout, not the objects a bind group owns.
    // An actual allocation replacement still revokes the binding immediately.
    [
        gpu.positions.id(),
        gpu.attributes.id(),
        gpu.clusters.id(),
        gpu.cluster_vertices.id(),
        gpu.cluster_triangles.id(),
        gpu.meshes.id(),
        view.visible.id(),
    ]
}
pub(super) fn refresh(
    gpu: &mut ClusterGpu,
    key: RetainedViewEntity,
    state: Option<&State>,
    device: &RenderDevice,
    cache: &PipelineCache,
) {
    let prepared = state.and_then(|state| state.active.get(&key));
    if prepared.is_none()
        || gpu
            .certificate
            .as_ref()
            .is_none_or(|package| !prepared.unwrap().source.matches_package(package))
    {
        let view = gpu.views.get_mut(&key).unwrap();
        if view.far.take().is_some() {
            view.readiness_revision += 1;
            view.terrain_clip_inputs = None;
        }
        return;
    }
    if current(gpu, &gpu.views[&key], state) {
        gpu.views
            .get_mut(&key)
            .unwrap()
            .far
            .as_mut()
            .unwrap()
            .queued_epoch = gpu.layout_epoch;
        return;
    }
    perf_span!("cluster::far::bind_group");
    let prepared = prepared.unwrap().clone();
    let layout = forward_layout(&gpu.draw_layout);
    let view = &gpu.views[&key];
    let buffers = geometry_buffers(gpu, view);
    let group = device.create_bind_group(
        "canonical terrain forward",
        &cache.get_bind_group_layout(&layout),
        &[
            entry(0, &gpu.positions),
            entry(1, &gpu.attributes),
            entry(2, &gpu.clusters),
            entry(3, &gpu.cluster_vertices),
            entry(4, &gpu.cluster_triangles),
            entry(5, &gpu.meshes),
            entry(6, &view.visible),
            entry(7, &prepared.gpu.header),
            entry(8, &prepared.gpu.words),
            entry(9, &prepared.context),
            entry(10, &prepared.selection),
        ],
    );
    let view = gpu.views.get_mut(&key).unwrap();
    view.terrain_clip_inputs = None;
    view.readiness_revision += 1;
    view.far = Some(Binding {
        key,
        prepared,
        buffers,
        queued_epoch: gpu.layout_epoch,
        layout,
        group,
    });
}

/// Called only after an actual forward indirect draw command has been encoded.
/// This proves selection/command continuity, not nonzero GPU survivor counts.
pub(super) fn note_encoded_draw(gpu: &ClusterGpu, view: &ViewGpu) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static FIRST: AtomicBool = AtomicBool::new(false);
    static AFTER_APPEND: AtomicBool = AtomicBool::new(false);
    if !FIRST.swap(true, Ordering::Relaxed) {
        info!("optional terrain forward indirect draw encoded");
    }
    if view
        .far
        .as_ref()
        .is_some_and(|binding| binding.queued_epoch != gpu.layout_epoch)
        && !AFTER_APPEND.swap(true, Ordering::Relaxed)
    {
        info!(
            layout_epoch = gpu.layout_epoch,
            "optional terrain forward indirect draw survives same-frame block append"
        );
    }
}
pub(super) fn forward_layout(draw_layout: &BindGroupLayoutDescriptor) -> BindGroupLayoutDescriptor {
    let mut entries = draw_layout.entries.clone();
    for (binding, factory) in [
        (
            7,
            uniform_buffer_sized(false, std::num::NonZeroU64::new(64)),
        ),
        (8, storage_buffer_read_only_sized(false, None)),
        (
            9,
            uniform_buffer_sized(false, std::num::NonZeroU64::new(400)),
        ),
        (
            10,
            uniform_buffer_sized(false, std::num::NonZeroU64::new(48)),
        ),
    ] {
        let mut entry = factory.build(binding, ShaderStages::FRAGMENT);
        entry.count = None;
        entries.push(entry);
    }
    BindGroupLayoutDescriptor::new("canonical terrain forward", &entries)
}

pub(super) struct SetForwardData<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetForwardData<I> {
    type Param = (SRes<ClusterGpu>, Option<SRes<State>>);
    type ViewQuery = &'static ExtractedView;
    type ItemQuery = &'static ClusterBatch;
    fn render<'w>(
        _: &P,
        view: ROQueryItem<'w, '_, Self::ViewQuery>,
        batch: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        (gpu, state): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let gpu = gpu.into_inner();
        let Some(view) = view_gpu(gpu, view) else {
            return RenderCommandResult::Skip;
        };
        let Some(draw) = batch.and_then(|batch| view.draws.get(&batch.index)) else {
            return RenderCommandResult::Skip;
        };
        if draw.far {
            if !current(gpu, view, state.map(|state| state.into_inner())) {
                return RenderCommandResult::Skip;
            }
            pass.set_bind_group(I, &view.far.as_ref().unwrap().group, &[]);
        } else {
            pass.set_bind_group(I, &view.draw_bind_group, &[]);
        }
        RenderCommandResult::Success
    }
}

pub(super) fn stamp_records(gpu: &ClusterGpu, origin: IVec2, offset: Vec3, bytes: &mut [u8]) {
    stamp_placements(&gpu.terrain_blocks, &gpu.placed, origin, offset, bytes);
}

/// Placement certification reads every arrived claim, not only ready blocks.
/// Keep the append revision in the late record cache even though appending does
/// not invalidate the binding's buffer objects. Off mode has no such dependency.
pub(super) fn record_revision(nonce: Option<u64>, append_revision: u64) -> Option<(u64, u64)> {
    nonce.map(|nonce| (nonce, append_revision))
}
fn stamp_placements(
    blocks: &[super::super::terrain::TerrainViewBlock],
    placed: &[ClusterInstance],
    origin: IVec2,
    offset: Vec3,
    bytes: &mut [u8],
) {
    use crate::far_terrain_shadows::placement;
    let mut metadata = HashMap::new();
    let mut duplicate = std::collections::HashSet::new();
    for instance in placed {
        let ClusterInstanceIdentity::Terrain { block, .. } = instance.identity else {
            continue;
        };
        let Some(source) = blocks.get(block as usize) else {
            continue;
        };
        if metadata.contains_key(&block) {
            duplicate.insert(block);
        }
        let value = placement::validate(block, source, instance, origin, offset).ok();
        // Count all claims, including invalid ones, before certifying shared metadata.
        if metadata.insert(block, value).is_some() {
            duplicate.insert(block);
        }
    }
    for (&block, &value) in &metadata {
        let Some(value) = value else {
            continue;
        };
        if duplicate.contains(&block) {
            continue;
        }
        if let Some(record) = bytes.get_mut(block as usize * 320..block as usize * 320 + 160) {
            // Inactive records remain inactive; invalid placement remains explicitly Unknown.
            let _ = placement::stamp(record, value);
        }
    }
}

#[cfg(test)]
mod placement_regressions {
    use super::*;
    use crate::{cluster::terrain::TerrainViewBlock, world::components::CELL_SIZE};

    fn block() -> TerrainViewBlock {
        TerrainViewBlock {
            worldspace_id: 12,
            anchor: IVec2::new(-4, 8),
            side_cells: 4,
            cells: vec![IVec2::new(-4, 8)],
            batch: Some(7),
        }
    }

    fn instance(block: &TerrainViewBlock) -> ClusterInstance {
        ClusterInstance {
            identity: ClusterInstanceIdentity::Terrain {
                block: 0,
                worldspace: block.worldspace_id,
                tier: block.side_cells,
                anchor: [17, -23], // Header-relative, intentionally not absolute SW.
            },
            mesh: 9,
            batch: 7,
            world_from_local: Mat4::from_translation(Vec3::new(
                block.anchor.x as f32 * CELL_SIZE,
                0.0,
                -(block.anchor.y as f32) * CELL_SIZE,
            )),
            source_worldspace: 0,
        }
    }

    fn records(block: &TerrainViewBlock, origin: IVec2) -> Vec<u8> {
        let mut bytes = Vec::new();
        block.append_view_records(
            Some(block.worldspace_id),
            origin,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
            true,
            &mut bytes,
        );
        assert_eq!(bytes.len(), 320);
        assert_eq!(word(&bytes, 16), 1.0_f32.to_bits());
        bytes
    }

    fn word(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn one_exact_placement_stamps_only_forward_and_invalid_or_duplicate_claims_never_do() {
        let block = block();
        let origin = IVec2::new(2, -3);
        let offset = render_origin_offset(origin);
        let valid = instance(&block);
        let mut invalid = valid;
        invalid.world_from_local.w_axis.y = 1.0;

        let before = records(&block, origin);
        let mut accepted = before.clone();
        stamp_placements(
            std::slice::from_ref(&block),
            &[valid],
            origin,
            offset,
            &mut accepted,
        );
        assert_eq!(word(&accepted, 16), 2.0_f32.to_bits());
        assert_eq!(word(&accepted, 20), block.anchor.x as u32);
        assert_eq!(word(&accepted, 24), block.anchor.y as u32);
        assert_eq!(word(&accepted, 28), block.worldspace_id);
        assert_eq!(&accepted[..16], &before[..16]);
        assert_eq!(&accepted[32..], &before[32..]);

        for claims in [
            vec![invalid],
            vec![valid, invalid],
            vec![invalid, valid],
            vec![valid, valid],
        ] {
            let mut refused = before.clone();
            stamp_placements(
                std::slice::from_ref(&block),
                &claims,
                origin,
                offset,
                &mut refused,
            );
            assert_eq!(
                refused, before,
                "ambiguous/invalid shared tag must stay Unknown"
            );
        }
    }

    #[test]
    fn a_late_duplicate_claim_revokes_cached_metadata_without_disabling_geometry() {
        let block = block();
        let origin = IVec2::new(2, -3);
        let offset = render_origin_offset(origin);
        let valid = instance(&block);
        let before = records(&block, origin);
        let first_key = record_revision(Some(9), 1);
        let mut first = before.clone();
        stamp_placements(
            std::slice::from_ref(&block),
            &[valid],
            origin,
            offset,
            &mut first,
        );
        assert_eq!(word(&first, 16), 2.0_f32.to_bits());

        // Same ready block and same optional source, but another claim arrived
        // after the early queue. The late finalizer must not reuse the old stamp.
        let next_key = record_revision(Some(9), 2);
        assert_ne!(first_key, next_key);
        let mut next = before.clone();
        stamp_placements(
            std::slice::from_ref(&block),
            &[valid, valid],
            origin,
            offset,
            &mut next,
        );
        assert_eq!(next, before);
        assert_eq!(word(&next, 16), 1.0_f32.to_bits());
        assert_eq!(record_revision(None, 1), record_revision(None, 2));
    }

    #[test]
    fn extra_scene_offset_revokes_metadata_without_disabling_geometry_or_touching_depth() {
        let block = block();
        let origin = IVec2::new(2, -3);
        let valid = instance(&block);
        let before = records(&block, origin);
        let mut shifted = before.clone();
        stamp_placements(
            std::slice::from_ref(&block),
            &[valid],
            origin,
            render_origin_offset(origin) + Vec3::X,
            &mut shifted,
        );
        assert_eq!(shifted, before);
        assert_eq!(
            word(&shifted, 16),
            1.0_f32.to_bits(),
            "ordinary geometry remains active"
        );
    }
}
