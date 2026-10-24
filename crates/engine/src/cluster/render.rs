//! Cluster draws that shade through the stock material path (ADR-0013 sections 2 and 7).
//!
//! A compute pass selects clusters per view by screen-space error (clusterlod's rule: a cluster
//! is drawn when its group's error is over the threshold and the group that produced it is at or
//! under it) and writes one indirect draw argument per (instance, cluster) slot; the draw is a
//! custom `Opaque3d` item that binds Bevy's own view groups and the stock `StandardMaterial` bind
//! group of a dummy `Mesh3d` entity, and runs Bevy's own PBR fragment shader. The vertex stage
//! fetches positions, octahedral normals and quantised UVs from the payload sections in storage
//! buffers. A pooled 16-bit index buffer supplies cluster-local vertex indices.

#[path = "far.rs"]
mod far;
#[path = "gpu_contract.rs"]
mod gpu_contract;
#[path = "handoff.rs"]
mod handoff;
use gpu_contract::{CullParams, DRAW_INDEXED_INDIRECT_BYTES, SELECTED_CLUSTER_BYTES, STAGES};

#[path = "stable_order.rs"]
mod stable_order;

#[path = "pipeline_diagnostics.rs"]
mod pipeline_diagnostics;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SourceExtraction;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ViewDomains<'w> {
    sources: Res<'w, handoff::Sources>,
}
impl ViewDomains<'_> {
    pub(crate) fn world(&self, key: RetainedViewEntity) -> Option<Option<u32>> {
        self.sources
            .worlds
            .get(&key.main_entity)
            .or_else(|| self.sources.worlds.get(&key.auxiliary_entity))
            .copied()
    }
    pub(crate) fn origin(&self) -> IVec2 {
        self.sources.origin
    }
}

use super::assets::ClusterMeshAsset;
pub use super::instance_identity::ClusterInstanceIdentity;
use bevy::{
    asset::embedded_asset,
    camera::{
        primitives::Aabb,
        visibility::{self, NoFrustumCulling, VisibilityClass},
    },
    core_pipeline::core_3d::{
        AlphaMask3d, CORE_3D_DEPTH_FORMAT, Opaque3d, Opaque3dBatchSetKey, Opaque3dBinKey,
    },
    core_pipeline::prepass::{
        AlphaMask3dPrepass, Opaque3dPrepass, OpaqueNoLightmap3dBatchSetKey,
        OpaqueNoLightmap3dBinKey,
    },
    ecs::{
        query::ROQueryItem,
        system::{SystemParamItem, lifetimeless::SRes},
    },
    material::{
        RenderPhaseType,
        key::{ErasedMaterialKey, ErasedMaterialPipelineKey, ErasedMeshPipelineKey},
    },
    mesh::MeshVertexBufferLayoutRef,
    pbr::{
        LightEntity, MaterialFragmentShader, MaterialPipeline, MeshPipelineKey, PreparedMaterial,
        PrepassPipeline, PrepassPipelineSpecializer, SetMeshViewBindGroup,
        SetMeshViewBindingArrayBindGroup, SetPrepassViewBindGroup, SetPrepassViewEmptyBindGroup,
        Shadow, ShadowBatchSetKey, ShadowBinKey, ViewKeyCache, alpha_mode_pipeline_key,
    },
    prelude::*,
    render::{
        Extract, ExtractSchedule, Render, RenderApp, RenderSystems,
        diagnostic::RecordDiagnostics,
        erased_render_asset::ErasedRenderAssets,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        mesh::{RenderMesh, allocator::MeshSlabs},
        render_asset::RenderAssets,
        render_phase::{
            AddRenderCommand, BinnedRenderPhaseType, DrawFunctions, InputUniformIndex, PhaseItem,
            RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass,
            ViewBinnedRenderPhases,
        },
        render_resource::{
            BindGroup, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer,
            BufferUsages, CachedComputePipelineId, CachedPipelineState, CachedRenderPipelineId,
            CompareFunction, ComputePassDescriptor, ComputePipelineDescriptor, DepthBiasState,
            DepthStencilState, FragmentState, IndexFormat, PipelineCache, PrimitiveState,
            RenderPipelineDescriptor, ShaderStages, SpecializedMeshPipeline, StencilFaceState,
            StencilState, VertexState,
            binding_types::{
                storage_buffer_read_only_sized, storage_buffer_sized, uniform_buffer_sized,
            },
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue},
        sync_world::MainEntity,
        view::{ExtractedView, RenderVisibleEntities, RetainedViewEntity},
    },
    shader::ShaderDefVal,
};
use shared::cluster::{InstanceRecord, MclSection};

/// What the two queue systems do each frame, logged every [`QUEUE_REPORT_EVERY`] frames: how often
/// a view's plan is rebuilt in full against skipped as unchanged, and the time each system takes.
/// The queue runs on the render thread, which the frame time waits for, so its cost is reported
/// next to the counts that explain it.
pub(super) mod queue_stats {
    use std::{
        sync::atomic::{AtomicU64, Ordering::Relaxed},
        time::Instant,
    };

    pub static BATCH_FULL: AtomicU64 = AtomicU64::new(0);
    pub static BATCH_SKIPPED: AtomicU64 = AtomicU64::new(0);
    /// Views whose batches were walked (an input of the walk changed, or the last walk left a
    /// batch out for want of a prepared material, mesh or pipeline).
    pub static BATCH_WALKS: AtomicU64 = AtomicU64::new(0);
    /// Batches those walks took up (all of them, or only the changed and still incomplete ones).
    pub static BATCH_RESOLVED: AtomicU64 = AtomicU64::new(0);
    pub static BATCH_NANOS: AtomicU64 = AtomicU64::new(0);
    pub static SHADOW_FULL: AtomicU64 = AtomicU64::new(0);
    pub static SHADOW_SKIPPED: AtomicU64 = AtomicU64::new(0);
    pub static SHADOW_NANOS: AtomicU64 = AtomicU64::new(0);
    /// Source orders built (one serves every view with the same draw groups).
    pub static ORDER_BUILDS: AtomicU64 = AtomicU64::new(0);
    /// The other per-frame cluster systems: the batch loader (main world), the material and batch
    /// extraction, the GPU preparation and the cull encode (render world).
    pub static LOAD_NANOS: AtomicU64 = AtomicU64::new(0);
    pub static EXTRACT_NANOS: AtomicU64 = AtomicU64::new(0);
    pub static PREPARE_NANOS: AtomicU64 = AtomicU64::new(0);
    /// Time spent appending streamed blocks to the GPU buffers, and how many blocks were.
    pub static APPEND_NANOS: AtomicU64 = AtomicU64::new(0);
    pub static BLOCKS_APPENDED: AtomicU64 = AtomicU64::new(0);
    pub static CULL_NANOS: AtomicU64 = AtomicU64::new(0);
    /// The cull encode in parts: the per-view buffer writes and clears, the eight compute passes.
    pub static CULL_WRITE_NANOS: AtomicU64 = AtomicU64::new(0);
    pub static CULL_PASS_NANOS: AtomicU64 = AtomicU64::new(0);
    /// Actual per-view cull uniform writes in the reporting window; unchanged views add none.
    pub static CULL_PARAMS_UPLOADS: AtomicU64 = AtomicU64::new(0);
    /// Draw groups in the camera's and in the latest rebuilt shadow view's plan: each is one
    /// indirect draw (and a pipeline and bind group switch) per view, encoded every frame.
    pub static GROUPS_CAMERA: AtomicU64 = AtomicU64::new(0);
    pub static GROUPS_SHADOW: AtomicU64 = AtomicU64::new(0);
    /// What the camera's groups are made of: distinct pipelines and distinct material bind groups.
    pub static GROUP_PIPELINES: AtomicU64 = AtomicU64::new(0);
    pub static GROUP_BIND_GROUPS: AtomicU64 = AtomicU64::new(0);
    /// Batches whose material is bound through a bindless slab, and all batches drawn.
    pub static BINDLESS_BATCHES: AtomicU64 = AtomicU64::new(0);
    pub static READY_BATCHES: AtomicU64 = AtomicU64::new(0);
    pub static FRAMES: AtomicU64 = AtomicU64::new(0);

    pub const QUEUE_REPORT_EVERY: u64 = 600;

    /// Adds the time between its creation and its drop to a counter.
    pub struct Timed(pub &'static AtomicU64, pub Instant);

    impl Timed {
        pub fn start(counter: &'static AtomicU64) -> Self {
            Self(counter, Instant::now())
        }
    }

    impl Drop for Timed {
        fn drop(&mut self) {
            self.0
                .fetch_add(self.1.elapsed().as_nanos() as u64, Relaxed);
        }
    }

    pub fn bump(counter: &AtomicU64) {
        counter.fetch_add(1, Relaxed);
    }

    /// Called once per frame by the batch queue; logs and resets every [`QUEUE_REPORT_EVERY`].
    pub fn frame_done() {
        let frames = FRAMES.fetch_add(1, Relaxed) + 1;
        if !frames.is_multiple_of(QUEUE_REPORT_EVERY) {
            return;
        }
        let take = |counter: &AtomicU64| counter.swap(0, Relaxed);
        let per_frame = |nanos: u64| nanos as f64 / 1.0e6 / QUEUE_REPORT_EVERY as f64;
        bevy::log::info!(
            "cluster queue over {QUEUE_REPORT_EVERY} frames: batches {} full / {} skipped views ({} walked, {} batches taken up), {:.2} ms/frame; \
             shadows {} full / {} skipped views, {:.2} ms/frame; source orders built {}; batch loader {:.2}, extract {:.2}, prepare {:.2}, cull {:.2} ms/frame (writes {:.2}, passes {:.2}, params {} uploads); draw groups: camera {} ({} pipelines x {} material bind groups; {} of {} batches bindless), a shadow view {}",
            take(&BATCH_FULL),
            take(&BATCH_SKIPPED),
            take(&BATCH_WALKS),
            take(&BATCH_RESOLVED),
            per_frame(take(&BATCH_NANOS)),
            take(&SHADOW_FULL),
            take(&SHADOW_SKIPPED),
            per_frame(take(&SHADOW_NANOS)),
            take(&ORDER_BUILDS),
            per_frame(take(&LOAD_NANOS)),
            per_frame(take(&EXTRACT_NANOS)),
            per_frame(take(&PREPARE_NANOS)),
            per_frame(take(&CULL_NANOS)),
            per_frame(take(&CULL_WRITE_NANOS)),
            per_frame(take(&CULL_PASS_NANOS)),
            take(&CULL_PARAMS_UPLOADS),
            GROUPS_CAMERA.load(Relaxed),
            GROUP_PIPELINES.load(Relaxed),
            GROUP_BIND_GROUPS.load(Relaxed),
            BINDLESS_BATCHES.load(Relaxed),
            READY_BATCHES.load(Relaxed),
            GROUPS_SHADOW.load(Relaxed),
        );
    }
}
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    num::NonZeroU64,
    sync::Arc,
};

use crate::snow::SnowMaterial;

/// Actual stock geometry for standalone fixtures without a certified glTF template.
/// Keep the same handle that the stock path draws; runtime mutation revokes proof.
#[derive(Resource, Default)]
pub struct ClusterBatchStockMeshes(pub Vec<Handle<Mesh>>);

/// The material handle of each batch, kept alive here rather than on one entity per batch (about
/// five thousand entities cost the main world's visibility and material systems and the render
/// world's extraction a millisecond or more a frame for nothing: the batches have no mesh, so no
/// stock system acts on those entities). A material is prepared from its asset alone;
/// [`ClusterBatchMaterials`] tells the render world which asset each batch draws with.
#[derive(Resource, Default)]
pub struct ClusterBatchHandles {
    handles: Vec<Option<bevy::asset::UntypedHandle>>,
}

impl ClusterBatchHandles {
    /// Records the material `batch` draws with.
    pub fn set<M: Asset>(&mut self, batch: u32, handle: Handle<M>) {
        let slot = batch as usize;
        if self.handles.len() <= slot {
            self.handles.resize(slot + 1, None);
        }
        self.handles[slot] = Some(handle.untyped());
    }

    /// The asset of every batch's material.
    pub fn ids(&self) -> impl Iterator<Item = bevy::asset::UntypedAssetId> + '_ {
        self.handles
            .iter()
            .flatten()
            .map(bevy::asset::UntypedHandle::id)
    }
}

/// The mesh the cluster draws are specialised with: one degenerate triangle with the attributes
/// the cluster vertex stage writes. No entity draws it; its render mesh only supplies the vertex
/// layout and key bits to the material pipeline.
#[derive(Resource, Clone, ExtractResource)]
pub struct ClusterDummyMesh {
    plain: Handle<Mesh>,
    tangent: Handle<Mesh>,
}

impl ClusterDummyMesh {
    fn asset(&self, tangents: bool) -> AssetId<Mesh> {
        if tangents {
            self.tangent.id()
        } else {
            self.plain.id()
        }
    }
}

/// Bevy's erased material key omits the mesh vertex layout: stock specialization
/// caches it separately. Our manual cache must retain that discriminator itself.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ClusterMaterialPipelineKey {
    material: ErasedMaterialPipelineKey,
    tangents: bool,
}

fn tangent_layout(
    stock_tangents: bool,
    geometry: super::material_geometry::GeometryRequirements,
) -> Option<bool> {
    (geometry.supported && (!geometry.tangents || stock_tangents)).then_some(stock_tangents)
}

fn batch_tangent_layout(
    batch: &BatchGpu,
    index: usize,
    material: bevy::asset::UntypedAssetId,
    geometry: super::material_geometry::GeometryRequirements,
    sources: &handoff::Sources,
) -> Option<bool> {
    let stock =
        if material.type_id() == std::any::TypeId::of::<super::terrain::ClusterTerrainMaterial>() {
            // Baked atlas terrain has no glTF stock primitive. Its own immutable
            // payload defines geometry; material edits cannot add attributes to it.
            Some(batch.source_tangents)
        } else {
            sources.stock_tangents(index)
        }?;
    // A certified authored semantic must also be present on actual stock.
    if batch.source_tangents && !stock {
        return None;
    }
    tangent_layout(stock, geometry)
}

fn material_geometry(
    id: bevy::asset::UntypedAssetId,
    standard: &Assets<StandardMaterial>,
    snow: Option<&Assets<SnowMaterial>>,
    terrain: Option<&Assets<super::terrain::ClusterTerrainMaterial>>,
) -> Option<super::material_geometry::GeometryRequirements> {
    let material = if id.type_id() == std::any::TypeId::of::<StandardMaterial>() {
        standard.get(id.typed::<StandardMaterial>())
    } else if id.type_id() == std::any::TypeId::of::<SnowMaterial>() {
        snow?
            .get(id.typed::<SnowMaterial>())
            .map(|material| &material.base)
    } else if id.type_id() == std::any::TypeId::of::<super::terrain::ClusterTerrainMaterial>() {
        terrain?
            .get(id.typed::<super::terrain::ClusterTerrainMaterial>())
            .map(|material| &material.base)
    } else {
        None
    }?;
    Some(super::material_geometry::requirements(material))
}

/// Per batch, the asset its material lives in, as the render world sees it.
#[derive(Resource, Default)]
struct ClusterBatchMaterials {
    ids: Vec<Option<bevy::asset::UntypedAssetId>>,
    /// Updated only on handle/material events, using the actual material base.
    /// Missing assets have no entry and cannot inherit an old tangent layout.
    geometry: HashMap<bevy::asset::UntypedAssetId, super::material_geometry::GeometryRequirements>,
    /// Current material identities and the batches that draw with each, bounded by the handle
    /// table; O(1) event matching.
    named: HashMap<bevy::asset::UntypedAssetId, Vec<u32>>,
    /// The batches whose material identity changed, or whose material had an event, in the
    /// extraction that last moved `version` (by exactly one).
    changed_batches: Vec<u32>,
    /// Bumped when `ids` changes or an asset event names one of the materials in it (it was
    /// modified, removed, or finished loading): the prepared materials the plan was made from
    /// can only have changed then. The stock material table's own change tick is no use for this:
    /// something unrelated to the batches (an animated material) moves it every frame.
    version: u64,
}

/// What the render world needs to place and show the instances this frame, read from the main
/// world when the frame is extracted, so it is the same snapshot the camera and every other object
/// were extracted from (a separate system that copied the origin earlier could lag it by a frame).
#[derive(Resource, Default)]
struct ClusterFrame {
    origin_offset: Vec3,
    streaming_camera: Option<Entity>,
}

/// The offset from absolute world coordinates to the engine's rebased render coordinates.
pub fn render_origin_offset(origin: IVec2) -> Vec3 {
    Vec3::new(
        -(origin.x as f32) * crate::world::components::CELL_SIZE,
        0.0,
        origin.y as f32 * crate::world::components::CELL_SIZE,
    )
}

fn extract_cluster_frame(
    mut frame: ResMut<ClusterFrame>,
    mut timing: Local<crate::profiling::PeriodicTiming>,
    scene: Extract<Option<Res<ClusterScene>>>,
    origin: Extract<Option<Res<crate::streaming::RenderOrigin>>>,
    cameras: Extract<Query<(Entity, &Camera), With<crate::world::components::StreamingCamera>>>,
) {
    perf_span!("cluster::extract_frame::total");
    let started = std::time::Instant::now();
    frame.streaming_camera = cameras
        .single()
        .ok()
        .filter(|(_, camera)| camera.is_active)
        .map(|(entity, _)| entity);
    let extra = scene
        .as_ref()
        .map_or(Vec3::ZERO, |scene| scene.origin_offset);
    frame.origin_offset = extra
        + origin
            .as_ref()
            .map_or(Vec3::ZERO, |origin| render_origin_offset(origin.0));
    timing.record("cluster/extract_frame", started.elapsed());
}

fn create_dummy_mesh(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(ClusterDummyMesh {
        plain: meshes.add(dummy_mesh_with_tangents(false)),
        tangent: meshes.add(dummy_mesh_with_tangents(true)),
    });
}

#[derive(bevy::ecs::system::SystemParam)]
struct BatchMaterialEvents<'w, 's> {
    standard: MessageReader<'w, 's, AssetEvent<StandardMaterial>>,
    snow: MessageReader<'w, 's, AssetEvent<SnowMaterial>>,
    terrain: Option<Res<'w, Messages<AssetEvent<super::terrain::ClusterTerrainMaterial>>>>,
}

type BatchMaterialAssets<'w> = (
    Res<'w, Assets<StandardMaterial>>,
    Option<Res<'w, Assets<SnowMaterial>>>,
    Option<Res<'w, Assets<super::terrain::ClusterTerrainMaterial>>>,
);

fn extract_batch_materials(
    mut out: ResMut<ClusterBatchMaterials>,
    mut timing: Local<crate::profiling::PeriodicTiming>,
    handles: Extract<Res<ClusterBatchHandles>>,
    mut events: Extract<BatchMaterialEvents>,
    assets: Extract<BatchMaterialAssets>,
    mut terrain_cursor: Local<
        bevy::ecs::message::MessageCursor<AssetEvent<super::terrain::ClusterTerrainMaterial>>,
    >,
) {
    perf_span!("cluster::extract_materials::total");
    let started = std::time::Instant::now();
    let _timer = queue_stats::Timed::start(&queue_stats::EXTRACT_NANOS);
    let mut changed = false;
    let mut dirty = HashSet::new();
    let mut changed_batches = Vec::new();
    // The handles change only while materials are being requested, so the table is read only then,
    // and only the batches whose material changed are taken up again.
    if handles.is_changed() {
        let ids: Vec<_> = handles
            .handles
            .iter()
            .map(|handle| handle.as_ref().map(bevy::asset::UntypedHandle::id))
            .collect();
        if out.ids != ids {
            for index in 0..ids.len().max(out.ids.len()) {
                let old = out.ids.get(index).copied().flatten();
                let new = ids.get(index).copied().flatten();
                if old == new {
                    continue;
                }
                let batch = index as u32;
                if let Some(old) = old
                    && let Some(batches) = out.named.get_mut(&old)
                {
                    batches.retain(|named| *named != batch);
                    if batches.is_empty() {
                        out.named.remove(&old);
                        out.geometry.remove(&old);
                    }
                }
                if let Some(new) = new {
                    out.named.entry(new).or_default().push(batch);
                    dirty.insert(new);
                }
                changed_batches.push(batch);
            }
            out.ids = ids;
            changed = true;
        }
    }
    let handles_changed = changed;
    let mut hit = |out: &mut ClusterBatchMaterials, id: bevy::asset::UntypedAssetId| {
        let Some(batches) = out.named.get(&id) else {
            return false;
        };
        changed_batches.extend(batches.iter().copied());
        dirty.insert(id);
        true
    };
    let mut standard_hits = Vec::new();
    for event in events.standard.read() {
        if hit(&mut out, event_asset_id(event).untyped()) {
            changed = true;
            standard_hits.push(*event);
        }
    }
    let mut snow_hits = 0usize;
    for event in events.snow.read() {
        if hit(&mut out, event_asset_id(event).untyped()) {
            changed = true;
            snow_hits += 1;
        }
    }
    let mut terrain_changed = false;
    if let Some(events) = events.terrain.as_ref() {
        for event in terrain_cursor.read(events) {
            terrain_changed |= hit(&mut out, event_asset_id(event).untyped());
        }
    }
    changed |= terrain_changed;
    let (standard, snow, terrain) = &*assets;
    for id in dirty {
        match material_geometry(id, standard, snow.as_deref(), terrain.as_deref()) {
            Some(requirements) => {
                out.geometry.insert(id, requirements);
            }
            None => {
                out.geometry.remove(&id);
            }
        }
    }
    if changed {
        changed_batches.sort_unstable();
        changed_batches.dedup();
        out.changed_batches = changed_batches;
        out.version += 1;
        debug!(
            version = out.version,
            handles_changed,
            standard = ?standard_hits.iter().take(4).collect::<Vec<_>>(),
            standard_count = standard_hits.len(),
            snow_hits,
            terrain_changed,
            "cluster batch materials changed"
        );
    }
    timing.record("cluster/extract_materials", started.elapsed());
}

/// The asset an asset event is about.
fn event_asset_id<A: Asset>(event: &AssetEvent<A>) -> AssetId<A> {
    match event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::Removed { id }
        | AssetEvent::Unused { id }
        | AssetEvent::LoadedWithDependencies { id } => *id,
    }
}

/// Actual extracted identities the phase items use to issue each draw group.
#[derive(bevy::ecs::system::SystemParam)]
struct CarrierWatch<'w, 's> {
    markers: Query<'w, 's, (Entity, &'static MainEntity, &'static ClusterBatch)>,
    far_pipeline_log: Local<'s, HashMap<usize, String>>,
    carrier_scratch: Local<'s, Vec<Option<(Entity, MainEntity)>>>,
}

/// How many draw groups one view can have. Each group's phase item needs an entity to be queued
/// for (the draw looks its group up through that entity's [`ClusterBatch`]), so the main world
/// spawns this many carriers once instead of one entity per batch: about five thousand entities
/// cost the visibility, extraction and queue systems a millisecond or more a frame. A group
/// takes the carrier numbered like itself. The pool starts at this many and grows (in steps of 64,
/// up to [`MAX_CLUSTER_CARRIERS`]) when a view has more groups, which the render world reports
/// through [`CARRIER_DEMAND`]; a mod's materials that are not bindless can make groups many.
pub const CLUSTER_CARRIERS: u32 = 256;

/// The most carriers the pool grows to.
pub const MAX_CLUSTER_CARRIERS: u32 = 4096;

/// The most draw groups any view has asked for, for the main world to spawn carriers for.
static CARRIER_DEMAND: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Spawns one carrier entity.
fn spawn_carrier(commands: &mut Commands, index: u32) {
    commands.spawn((
        Visibility::default(),
        Transform::default(),
        batch_bounds(),
        ClusterBatch { index },
    ));
}

/// Grows the carrier pool to the draw groups the views have asked for, once the scene exists.
fn grow_carrier_pool(
    mut commands: Commands,
    scene: Option<Res<ClusterScene>>,
    mut spawned: Local<u32>,
) {
    if scene.is_none() {
        return;
    }
    let demand = CARRIER_DEMAND.load(std::sync::atomic::Ordering::Relaxed);
    let target = demand
        .max(CLUSTER_CARRIERS)
        .next_multiple_of(64)
        .min(MAX_CLUSTER_CARRIERS);
    while *spawned < target {
        spawn_carrier(&mut commands, *spawned);
        *spawned += 1;
    }
}

/// Marks one of the [`CLUSTER_CARRIERS`] entities a draw group's item is queued for. A group is the
/// batches (instances that share one material) that bind the same pipeline and material bind
/// group: one cull result, compacted list and indirect draw per view. `index` is the group's
/// number in the view, not a batch.
#[derive(Clone, Component, ExtractComponent)]
#[require(VisibilityClass, NoFrustumCulling)]
#[component(on_add = visibility::add_visibility_class::<ClusterBatch>)]
pub struct ClusterBatch {
    pub index: u32,
}

fn push_f32(bytes: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

fn push_u32(bytes: &mut Vec<u8>, values: &[u32]) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

/// A section's bytes padded to a whole number of words, the unit a storage buffer is read in. An
/// empty section stays empty: the bases of the meshes after it are counted in whole records, and
/// extra bytes would shift every one of them (a buffer that ends up empty is sized to one word
/// where it is created).
fn padded(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.resize(out.len().next_multiple_of(4), 0);
    out
}

/// One placed object: which pooled mesh it draws and where.
#[derive(Clone, Copy)]
pub struct ClusterInstance {
    /// Package-local CPU identity; copied intact through both reorderings.
    /// This field alone does not certify eligibility or authorize a draw.
    pub identity: ClusterInstanceIdentity,
    /// Index into [`ClusterScene::meshes`].
    pub mesh: u32,
    /// The batch (material) it is drawn with: a [`ClusterMaterialSource`] and a [`ClusterBatch`]
    /// with this index must exist.
    pub batch: u32,
    /// World-from-local.
    pub world_from_local: Mat4,
    /// Legacy source tag retained in the GPU record layout. It grants no view
    /// ownership; the certified identity and per-view mask make that decision.
    pub source_worldspace: u32,
}

/// What the cluster path draws: validated mesh payloads, pooled into shared GPU buffers, and the
/// instances that place them.
///
/// The prototype builds this once; streaming (blocks coming and going) replaces this resource.
#[derive(Resource, Clone, ExtractResource)]
pub struct ClusterScene {
    /// Retained source authority for every package-local object key, including render extraction.
    /// None is reserved for explicitly invented fixture scenes.
    pub certificate: Option<Arc<super::reader::CertifiedPackage>>,
    /// Per-instance terrain clip metadata; indexed by the terrain identity, never by shared mesh hash.
    pub terrain_blocks: Vec<super::terrain::TerrainViewBlock>,
    pub meshes: Vec<ClusterMeshAsset>,
    pub instances: Vec<ClusterInstance>,
    /// Added to every instance's translation when it is uploaded: the engine rebases its render
    /// coordinates as the camera crosses cells, and instances are stored in absolute ones.
    pub origin_offset: Vec3,
    /// The order the instances arrive on the GPU in: each block is a run of `instances`. Empty means
    /// one block of everything, available at once.
    pub blocks: Vec<SceneBlock>,
}

/// A run of [`ClusterScene::instances`] that reaches the GPU together, with the meshes it names.
///
/// The scene is indexed whole and the GPU buffers are sized for all of it, but the contents arrive
/// block by block: the main world sets `ready` once a block's meshes are prepared (their tangents
/// generated) and the render world appends it and sets `resident`, a block per frame, in the order
/// of [`ClusterScene::blocks`].
#[derive(Clone)]
pub struct SceneBlock {
    pub first_instance: u32,
    pub instance_count: u32,
    /// Indices into [`ClusterScene::meshes`] of the meshes the block's instances draw, each once.
    pub meshes: Vec<u32>,
    pub ready: Arc<std::sync::atomic::AtomicBool>,
    pub resident: Arc<std::sync::atomic::AtomicBool>,
}

impl ClusterScene {
    /// Builds a scene that draws one mesh at every record of an instance block.
    pub fn new(mesh: ClusterMeshAsset, records: &[InstanceRecord]) -> Self {
        Self {
            certificate: None,
            terrain_blocks: Vec::new(),
            origin_offset: Vec3::ZERO,
            blocks: Vec::new(),
            meshes: vec![mesh],
            instances: records
                .iter()
                .map(|record| ClusterInstance {
                    identity: super::instance_identity::ClusterInstanceIdentity::Fixture,
                    mesh: 0,
                    batch: 0,
                    world_from_local: world_from_local(record),
                    source_worldspace: record.source_worldspace_id,
                })
                .collect(),
        }
    }
}

/// The 3x4 row-major transform of an instance record as a column-major matrix.
pub fn world_from_local(record: &InstanceRecord) -> Mat4 {
    let t = &record.transform;
    Mat4::from_cols(
        Vec4::new(t[0], t[4], t[8], 0.0),
        Vec4::new(t[1], t[5], t[9], 0.0),
        Vec4::new(t[2], t[6], t[10], 0.0),
        Vec4::new(t[3], t[7], t[11], 1.0),
    )
}

/// Copies the graphics settings' cluster error thresholds into [`ClusterSettings`] whenever they
/// change, so a preset, a flag, the settings file or the graphics panel sets the cluster detail.
pub fn follow_graphics_quality(
    graphics: Option<Res<crate::graphics_settings::GraphicsSettings>>,
    settings: Option<ResMut<ClusterSettings>>,
) {
    let (Some(graphics), Some(mut settings)) = (graphics, settings) else {
        return;
    };
    if !graphics.is_changed() && !settings.is_added() {
        return;
    }
    let wanted = (
        graphics.cluster_error_pixels,
        graphics.cluster_terrain_error_pixels,
    );
    if (settings.error_pixels, settings.terrain_error_pixels) != wanted {
        (settings.error_pixels, settings.terrain_error_pixels) = wanted;
    }
}

/// Runtime settings of the cluster path.
#[derive(Resource, Clone, ExtractResource)]
pub struct ClusterSettings {
    /// Screen-space error threshold in pixels (`cluster_lod_error_pixels`, ADR section 7).
    pub error_pixels: f32,
    /// An instance whose bounding sphere spans fewer pixels than this across, from the camera, is
    /// not drawn: it cannot show more than that many pixels of detail. 0 draws every instance.
    /// Perspective views only; a shadow cascade keeps every caster.
    pub min_instance_pixels: f32,
    /// The screen-space error threshold for terrain meshes (meshes only terrain instances draw),
    /// in pixels. Terrain is broad and low in relief, so it can take a coarser threshold than
    /// objects without visible change; the same idea as a landscape LOD bias.
    pub terrain_error_pixels: f32,
    /// Draw the whole argument list with `multi_draw_indexed_indirect` even where the adapter can
    /// read the count on the GPU: keeps the fallback used by Metal testable on count-capable GPUs.
    pub fixed_count_draw: bool,
    /// Draw the cluster batches into the camera's prepass (depth, normals, motion vectors) where
    /// the camera has one. Off leaves them out of it, as before the prepass existed: what ambient
    /// occlusion, temporal anti-aliasing and the water's reflections read then has no clusters.
    pub prepass: bool,
    /// `--cluster-own-statics`: owned stock primitives are hidden once at spawn
    /// (`super::ownership`), so a view draws its selection base as it is, with no per-reference
    /// stock overlay and no stock material comparison.
    pub own_statics: bool,
}

impl Default for ClusterSettings {
    fn default() -> Self {
        Self {
            error_pixels: 1.0,
            min_instance_pixels: 0.0,
            terrain_error_pixels: 1.0,
            fixed_count_draw: false,
            prepass: true,
            own_statics: false,
        }
    }
}

/// A mesh the batch's dummy entity carries: one degenerate triangle with the attributes the
/// cluster vertex stage writes, so the specialised pipeline gets the matching shader definitions.
pub fn dummy_mesh() -> Mesh {
    dummy_mesh_with_tangents(true)
}

fn dummy_mesh_with_tangents(tangents: bool) -> Mesh {
    let mut mesh = Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32, 1.0, 0.0]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32; 2]; 3])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[1.0_f32; 4]; 3])
    .with_inserted_indices(bevy::mesh::Indices::U32(vec![0, 1, 2]));
    if tangents {
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0_f32, 0.0, 0.0, 1.0]; 3]);
    }
    mesh
}

/// The dummy entity's bounds: large enough that it is always inside the view, so the batch is
/// never dropped by the visibility pass. The cull pass decides what is drawn.
pub fn batch_bounds() -> Aabb {
    Aabb {
        center: Vec3A::ZERO,
        half_extents: Vec3A::splat(1.0e6),
    }
}

pub struct ClusterRenderPlugin;

impl Plugin for ClusterRenderPlugin {
    fn build(&self, app: &mut App) {
        super::runtime_state::install(app);
        handoff::install(app);
        embedded_asset!(app, "shaders/cluster_cull.wgsl");
        embedded_asset!(app, "shaders/cluster_draw.wgsl");
        embedded_asset!(app, "shaders/cluster_prepass.wgsl");
        embedded_asset!(app, "shaders/cluster_shadow.wgsl");
        embedded_asset!(app, "shaders/cluster_shadow_mask.wgsl");
        embedded_asset!(app, "shaders/cluster_shift.wgsl");
        app.init_resource::<ClusterBatchHandles>()
            .add_systems(Startup, create_dummy_mesh)
            .add_systems(Update, (grow_carrier_pool, follow_graphics_quality));
        app.add_plugins((
            ExtractComponentPlugin::<ClusterBatch>::default(),
            ExtractResourcePlugin::<ClusterDummyMesh>::default(),
            ExtractResourcePlugin::<ClusterScene>::default(),
            ExtractResourcePlugin::<ClusterSettings>::default(),
            ExtractResourcePlugin::<super::package::TerrainBatchIds>::default(),
            ExtractResourcePlugin::<super::terrain::TerrainDrawReadiness>::default(),
        ));
        let diagnostics = app
            .world()
            .get_resource::<crate::config::EngineConfig>()
            .is_some_and(|config| config.diagnostic_pipelines);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        pipeline_diagnostics::install(render_app, diagnostics);
        render_app
            .init_resource::<ClusterPipelines>()
            .init_resource::<ClusterSettings>()
            .init_resource::<ClusterBatchMaterials>()
            .init_resource::<ClusterFrame>()
            .add_systems(
                ExtractSchedule,
                (extract_batch_materials, extract_cluster_frame),
            )
            .add_render_command::<Opaque3d, DrawClusterBatch>()
            .add_render_command::<AlphaMask3d, DrawClusterBatch>()
            .add_render_command::<Opaque3dPrepass, DrawClusterPrepass>()
            .add_render_command::<AlphaMask3dPrepass, DrawClusterPrepass>()
            .add_render_command::<Opaque3dPrepass, DrawClusterDepthOnlyPrepass>()
            .add_render_command::<AlphaMask3dPrepass, DrawClusterDepthOnlyPrepass>()
            .add_render_command::<Shadow, DrawClusterShadow>()
            .add_render_command::<Shadow, DrawClusterShadowMask>()
            .add_systems(
                Render,
                (
                    // Freeze this frame's appended layout before queue planning and
                    // complementary stock selection. Appending in PrepareResources
                    // would change layout after the fallback's phase removal.
                    prepare_cluster_gpu
                        .in_set(RenderSystems::Queue)
                        .before(queue_cluster_batches),
                    queue_cluster_batches
                        .in_set(RenderSystems::Queue)
                        .before(RenderSystems::QueueMeshes),
                    queue_cluster_prepass
                        .in_set(RenderSystems::Queue)
                        .before(RenderSystems::QueueMeshes)
                        .after(queue_cluster_batches),
                    queue_cluster_shadows
                        .in_set(RenderSystems::Queue)
                        .before(RenderSystems::QueueMeshes)
                        .after(queue_cluster_batches),
                    publish_terrain_readiness
                        .in_set(RenderSystems::QueueMeshes)
                        .after(queue_cluster_batches)
                        .after(queue_cluster_prepass)
                        .after(queue_cluster_shadows),
                ),
            )
            // The cull is encoded into the frame's own command encoder at the start of the render
            // graph; the GPU spans are recorded into the frame's recorder, which starts here.
            .add_systems(
                RenderGraph,
                run_cluster_cull
                    .in_set(RenderGraphSystems::Begin)
                    .after(bevy::render::diagnostic::begin_diagnostics_frame),
            );
    }
}

impl ViewGpu {
    /// The plan's representative for batch `index`, if it has one.
    fn plan_rep(&self, index: u32) -> Option<&ReadyBatch> {
        self.plan_rep_positions
            .get(&index)
            .map(|position| &self.plan_reps[*position])
    }
}

/// How long a view may go unseen before its buffers are freed: long enough that a cascade, camera
/// or portal view that comes back soon finds its buffers and its plan. Wall time, not a count of
/// cull runs, so the same pause keeps the buffers at 60 fps and at 160 fps.
const VIEW_KEEP: std::time::Duration = std::time::Duration::from_secs(10);

/// What one view needs to select and draw every batch: its own parameters, argument list (one
/// slice per draw group), draw counts and list of what was selected, because every view (the
/// camera, each shadow cascade) selects differently.
struct ViewGpu {
    /// When a cull run last saw the view; a view not seen for [`VIEW_KEEP`] (a light that went
    /// away, a camera that was removed) is dropped.
    last_seen: std::time::Instant,
    /// Draw commands require a cull encoded for the current frame/epoch.
    last_cull_epoch: u32,
    selection: Option<handoff::Selection>,
    /// The visible-set-independent part of `selection` ([`handoff::SelectionBase`]).
    selection_base: Option<Arc<handoff::SelectionBase>>,
    instance_mask_bytes: Vec<u8>,
    terrain_clip_bytes: Vec<u8>,
    terrain_clip_inputs: Option<handoff::TerrainClipInputs>,
    far: Option<far::Binding>,
    prepass_pipelines: HashMap<u32, CachedRenderPipelineId>,
    shadow_pipelines: HashMap<u32, CachedRenderPipelineId>,
    args: Buffer,
    /// One draw count per draw group.
    counts: Buffer,
    params: Buffer,
    /// Exact contents last written to this allocation, including the first upload.
    params_uploaded: Option<CullParams>,
    /// Per draw: instance, mesh, cluster within the mesh. Held so the bind groups stay valid.
    #[allow(dead_code)]
    visible: Buffer,
    /// The instances that are in this view, and the indirect dispatch over them.
    #[allow(dead_code)]
    instance_list: Buffer,
    dispatch: Buffer,
    source_order: Buffer,
    /// The source order `source_order` holds, shared with every view of the same draw groups.
    order: Option<Arc<stable_order::SharedOrder>>,
    compaction_tiles: u32,
    large_scatter_count: u32,
    compaction_bind_groups: [BindGroup; 6],
    /// Per batch: whether it may draw in this view, its draw group and where the group's slice of
    /// `args` starts.
    batch_table: Buffer,
    /// What `batch_table` holds, so it is written only when the groups change.
    table: Vec<u8>,
    /// The draw of each group, by the batch whose phase item issues it.
    draws: HashMap<u32, GroupDraw>,
    /// What each batch resolved to, with the inputs it was resolved from.
    memo: Vec<Option<BatchMemo>>,
    /// A hash of the plan last queued into the phases (which batches, in which groups), the
    /// representatives it queued, and which phases it used; the bins persist between frames, so
    /// an unchanged plan queues nothing.
    plan_hash: Option<u64>,
    /// What the last walk over the batches was made from, kept only when it left nothing out for
    /// want of a prepared material, mesh or pipeline: the same again means the same plan.
    walk_stamp: Option<[u64; 10]>,
    /// `walk_stamp` without the batch materials' version and the phase occupancy, neither of
    /// which changes what a batch resolves to: under the same base, a walk takes up only the
    /// batches named by the version's step (`ClusterBatchMaterials::changed_batches`) and the
    /// ones the last walk left out for want of a prepared material, mesh or pipeline.
    walk_base: Option<[u64; 10]>,
    walked_version: u64,
    /// What each batch resolved to in the walks so far (by batch index), how far a terrain batch
    /// got (for `note_terrain_walk`), and the batches left out for want of something prepared.
    walk_ready: Vec<Option<ReadyBatch>>,
    walk_terrain: Vec<Option<u8>>,
    walk_incomplete: std::collections::BTreeSet<u32>,
    /// The cull epoch each prepass draw group last drew in (see [`DrawClusterIndirect`]'s `ONCE`).
    prepass_drawn: std::sync::Mutex<HashMap<u32, u32>>,
    walk_carriers: Vec<Option<(Entity, MainEntity)>>,
    plan_items: Vec<MainEntity>,
    plan_has_opaque: bool,
    plan_has_alpha_mask: bool,
    /// The batches that issue the draw of each group in the last plan, and a counter that moves
    /// when the plan changes: the prepass queue draws exactly these.
    plan_reps: Vec<ReadyBatch>,
    /// Position in `plan_reps` of each representative batch index.
    plan_rep_positions: HashMap<u32, usize>,
    plan_epoch: u64,
    /// Bumped by every write to what `handoff::batch_ready_with` reads of this view (its draws
    /// and batch table, memo entries, plan representatives, prepass and shadow pipelines, far
    /// binding), so a cached readiness can tell it is still exact.
    pub(super) readiness_revision: u64,
    /// The view's readiness from the last frame it was computed, with the key it was computed for.
    pub(super) readiness_cache: Option<(handoff::ReadinessKey, Arc<Vec<bool>>)>,
    /// The `plan_epoch` the prepass phases were last filled for, the items it queued, and which
    /// of the two phases they went to.
    prepass_epoch: Option<u64>,
    prepass_items: Vec<MainEntity>,
    /// The draw groups the prepass queue put an item in for, this plan: a terrain batch is
    /// drawable in the prepass when its own group is among them, whatever other groups lack.
    prepass_groups: std::collections::HashSet<u32>,
    prepass_has_opaque: bool,
    prepass_has_alpha_mask: bool,
    instance_bind_group: BindGroup,
    cull_bind_group: BindGroup,
    draw_bind_group: BindGroup,
    shadow_bind_group: BindGroup,
}

/// One indirect draw covering every batch of a draw group.
struct GroupDraw {
    /// The batch whose material bind group the group's draw binds.
    representative: u32,
    /// Where the group's slice of the argument list starts, in bytes.
    args_offset: u64,
    /// Where the group's draw count is, in bytes.
    count_offset: u64,
    /// The most clusters the group's batches can select between them.
    max_count: u32,
    far: bool,
}

/// The draw groups of one view. Batches whose draws would bind the same pipeline and the same
/// material bind group differ only in the material slot in their mesh uniforms, so one indirect
/// draw covers them all: the cost of a view follows the number of distinct pipelines and bind
/// groups, not the number of materials.
struct DrawGroups {
    /// Per group: the batch whose phase item issues the draw, and the most clusters its members
    /// can select.
    groups: Vec<(u32, u32)>,
    /// Per batch: its group, if it may draw in this view.
    batch_group: Vec<Option<u32>>,
}

impl DrawGroups {
    fn new(batch_count: usize) -> Self {
        Self {
            groups: Vec::new(),
            batch_group: vec![None; batch_count],
        }
    }

    /// Adds `batch` (with room for `slots` clusters) to `group`, or to a new group when `group` is
    /// `None`; returns the group it joined.
    fn add(&mut self, batch: u32, slots: u32, group: Option<u32>) -> u32 {
        let group = group.unwrap_or_else(|| {
            self.groups.push((batch, 0));
            (self.groups.len() - 1) as u32
        });
        self.groups[group as usize].1 += slots;
        self.batch_group[batch as usize] = Some(group);
        group
    }

    /// Where each group's slice of the argument list starts and what it draws (in group order),
    /// and the view's batch table: per batch, whether it draws, its group and where the group's
    /// slice starts.
    fn plan(&self) -> (Vec<GroupDraw>, Vec<u8>) {
        let mut region_starts = Vec::with_capacity(self.groups.len());
        let mut next = 0u32;
        let mut draws = Vec::with_capacity(self.groups.len());
        for (group, (representative, slots)) in self.groups.iter().enumerate() {
            region_starts.push(next);
            draws.push(GroupDraw {
                representative: *representative,
                args_offset: u64::from(next) * DRAW_INDEXED_INDIRECT_BYTES,
                count_offset: group as u64 * 4,
                max_count: *slots,
                far: false,
            });
            next += slots;
        }
        let mut table = Vec::with_capacity(self.batch_group.len() * 16);
        for group in &self.batch_group {
            match group {
                Some(group) => {
                    push_u32(&mut table, &[1, *group, region_starts[*group as usize], 0])
                }
                None => push_u32(&mut table, &[0, 0, 0, 0]),
            }
        }
        (draws, table)
    }

    /// Source instances grouped for the prefix pass, independent of current visibility.
    fn order(
        &self,
        batches: &[BatchGpu],
        large_instances: &[u32],
        ordered_instances: u32,
    ) -> Vec<u32> {
        perf_span!("cluster::plan::source_order");
        let mut instances = vec![Vec::new(); self.groups.len()];
        for (batch, group) in self.batch_group.iter().enumerate() {
            if let Some(group) = group {
                for range in &batches[batch].ranges {
                    instances[*group as usize].extend(range.clone());
                }
            }
        }
        let mut base = 0u32;
        let bases: Vec<_> = self
            .groups
            .iter()
            .map(|(_, slots)| {
                let start = base;
                base += slots;
                start
            })
            .collect();
        stable_order::source_order(instances, &bases, large_instances, ordered_instances)
    }

    /// The source order of these groups, built only when no view holds it already (see
    /// [`stable_order::OrderCache`]).
    fn shared_order(&self, gpu: &mut ClusterGpu) -> Arc<stable_order::SharedOrder> {
        let ClusterGpu {
            order_cache,
            batches,
            large_instances,
            total_instances,
            layout_epoch,
            ..
        } = gpu;
        let builds = order_cache.builds;
        let order =
            order_cache.get_or_build(*layout_epoch, &self.groups, &self.batch_group, || {
                self.order(batches, large_instances, *total_instances)
            });
        if order_cache.builds != builds {
            queue_stats::bump(&queue_stats::ORDER_BUILDS);
        }
        order
    }

    /// Writes immutable ordering and batch metadata only when their inputs changed.
    fn apply(
        self,
        view_gpu: &mut ViewGpu,
        queue: &RenderQueue,
        order: Arc<stable_order::SharedOrder>,
    ) {
        perf_span!("cluster::plan::apply");
        let words = &order.words;
        view_gpu.compaction_tiles = words[1];
        view_gpu.large_scatter_count = words[words[3] as usize];
        let written = view_gpu
            .order
            .as_ref()
            .is_some_and(|held| Arc::ptr_eq(held, &order) || held.words == *words);
        if !written {
            #[cfg(feature = "tracy")]
            let _span =
                bevy::log::info_span!("cluster::upload::source_order", bytes = words.len() * 4)
                    .entered();
            queue.write_buffer(&view_gpu.source_order, 0, bytemuck::cast_slice(words));
        }
        view_gpu.order = Some(order);
        let (draws, table) = self.plan();
        view_gpu.readiness_revision += 1;
        view_gpu.draws.clear();
        for (group, draw) in draws.into_iter().enumerate() {
            view_gpu.draws.insert(group as u32, draw);
        }
        if table != view_gpu.table {
            queue.write_buffer(&view_gpu.batch_table, 0, &table);
            view_gpu.table = table;
        }
    }
}

/// One batch: the instances that share a material. They arrive block by block, so a batch owns one
/// run of instance indices per block that has an instance of it.
struct BatchGpu {
    /// The runs of instance indices (into the instance buffers) this batch owns, in arrival order.
    ranges: Vec<std::ops::Range<u32>>,
    /// The most clusters one view can select from this batch, over the instances that have arrived.
    slots: u32,
    /// The material asset this batch draws with, once it is ready.
    material: Option<bevy::asset::UntypedAssetId>,
    /// The material's slot in the bindless table, written into the batch's mesh uniforms.
    material_slot: Option<u32>,
    /// What a shadow view needs to draw this batch when its material discards by alpha.
    cutout: Option<CutoutShadow>,
    /// Authored TANGENT semantic from immutable payload metadata, never the
    /// presence of generated tangent words in the GPU interleave.
    source_tangents: bool,
    /// Layout last resolved with the actual material requirements.
    tangents: Option<bool>,
}

fn invalidate_batch_material(gpu: &mut ClusterGpu, index: usize) {
    let batch = &mut gpu.batches[index];
    if batch.material.take().is_some() || batch.tangents.is_some() {
        gpu.generation = gpu.generation.wrapping_add(1);
    }
    batch.tangents = None;
    batch.cutout = None;
}

/// One load-time walk, not a scene scan per frame. The package loader partitions
/// material batches by source layout; fixture callers must preserve that invariant.
fn batch_source_tangents(scene: &ClusterScene, batches: usize) -> Result<Vec<bool>, &'static str> {
    let mut layouts = vec![None; batches];
    for instance in &scene.instances {
        let mesh = scene
            .meshes
            .get(instance.mesh as usize)
            .ok_or("batch references missing mesh")?;
        let source = mesh.flags & shared::cluster::FLAG_TANGENTS != 0;
        let layout = layouts
            .get_mut(instance.batch as usize)
            .ok_or("batch index out of range")?;
        if layout.is_some_and(|previous| previous != source) {
            return Err("cluster material batch mixes authored tangent layouts");
        }
        *layout = Some(source);
    }
    Ok(layouts
        .into_iter()
        .map(|layout| layout.unwrap_or(false))
        .collect())
}

/// The part of an alpha-masked material a shadow pipeline and draw are built from.
#[derive(Clone)]
struct CutoutShadow {
    asset: bevy::asset::UntypedAssetId,
    type_id: std::any::TypeId,
    material_key: ErasedMaterialKey,
    material_group: u32,
}

/// One block of the scene on the render side.
struct GpuBlock {
    first: u32,
    count: u32,
    meshes: Vec<u32>,
    ready: Arc<std::sync::atomic::AtomicBool>,
    resident: Arc<std::sync::atomic::AtomicBool>,
    appended: bool,
}

/// The most bytes of blocks appended in one frame: a block's meshes and instances are written to the
/// GPU in one go, so this sets the hitch a streaming block can cause. At least one block is
/// appended per frame whatever its size.
const APPEND_BYTES_PER_FRAME: u64 = 32 << 20;

/// Appends the blocks the main world has prepared, in scene order, up to the frame's byte budget.
fn append_ready_blocks(gpu: &mut ClusterGpu, scene: &ClusterScene, queue: &RenderQueue) {
    perf_span!("cluster::prepare::append_ready");
    let _timer = queue_stats::Timed::start(&queue_stats::APPEND_NANOS);
    let mut bytes = 0u64;
    while bytes < APPEND_BYTES_PER_FRAME {
        let Some(index) = gpu.blocks.iter().position(|block| {
            !block.appended && block.ready.load(std::sync::atomic::Ordering::Acquire)
        }) else {
            return;
        };
        bytes += append_block(gpu, scene, index, queue);
    }
}

/// Writes one block's meshes (the ones no earlier block brought) and instances to the GPU and
/// gives its batches their runs. Returns the bytes written.
fn append_block(
    gpu: &mut ClusterGpu,
    scene: &ClusterScene,
    index: usize,
    queue: &RenderQueue,
) -> u64 {
    let mut written = 0u64;
    let mesh_ids = gpu.blocks[index].meshes.clone();
    for mesh_id in mesh_ids {
        if std::mem::replace(&mut gpu.pooled[mesh_id as usize], true) {
            continue;
        }
        let mesh = &scene.meshes[mesh_id as usize];
        let at = gpu.cursors;
        let sections = mesh_sections(mesh);
        queue.write_buffer(&gpu.positions, at.positions, &sections.positions);
        queue.write_buffer(&gpu.attributes, at.attributes, &sections.attributes);
        queue.write_buffer(&gpu.clusters, at.clusters, &sections.clusters);
        queue.write_buffer(
            &gpu.cluster_vertices,
            at.cluster_vertices,
            &sections.cluster_vertices,
        );
        queue.write_buffer(
            &gpu.cluster_indices,
            at.cluster_indices,
            &sections.cluster_indices,
        );
        queue.write_buffer(&gpu.groups, at.groups, &sections.groups);
        queue.write_buffer(
            &gpu.meshes,
            u64::from(mesh_id) * 64,
            &mesh_info_row(&at, mesh, gpu.terrain_meshes[mesh_id as usize]),
        );
        let sizes = PoolCursors {
            positions: sections.positions.len() as u64,
            attributes: sections.attributes.len() as u64,
            clusters: sections.clusters.len() as u64,
            cluster_vertices: sections.cluster_vertices.len() as u64,
            cluster_indices: sections.cluster_indices.len() as u64,
            groups: sections.groups.len() as u64,
        };
        written += sizes.positions
            + sizes.attributes
            + sizes.clusters
            + sizes.cluster_vertices
            + sizes.cluster_indices
            + sizes.groups;
        gpu.cursors.advance(&sizes);
    }
    let first = gpu.blocks[index].first as usize;
    let count = gpu.blocks[index].count as usize;
    // Within a block the instances are sorted by batch, so a batch takes one run per block.
    let mut arrived = scene.instances[first..first + count].to_vec();
    arrived.sort_by_key(|instance| instance.batch);
    let base = gpu.total_instances;
    let offset = gpu.applied_offset;
    let mut instance_bytes = Vec::with_capacity(count * 80);
    let mut absolute_bytes = Vec::with_capacity(count * 16);
    let mut uniform_bytes = Vec::with_capacity(count * MESH_UNIFORM_BYTES);
    for (delta, instance) in arrived.iter().enumerate() {
        if let ClusterInstanceIdentity::Object(key) = instance.identity {
            gpu.object_indices.insert(key, base + delta as u32);
        }
        push_f32(
            &mut instance_bytes,
            &placed_matrix(instance, offset).to_cols_array(),
        );
        // A fixed scratch slice preserves source order independently of view/material grouping.
        push_u32(
            &mut instance_bytes,
            &[
                instance.mesh,
                instance.batch,
                gpu.next_slot,
                instance.source_worldspace,
            ],
        );
        let cluster_count = scene.meshes[instance.mesh as usize].cluster_count;
        gpu.next_slot += cluster_count;
        if cluster_count > 64 {
            gpu.large_instances.push(base + delta as u32);
        }
        let translation = instance.world_from_local.w_axis;
        push_f32(
            &mut absolute_bytes,
            &[translation.x, translation.y, translation.z, 0.0],
        );
        match gpu.batches[instance.batch as usize].material_slot {
            Some(slot) => uniform_bytes.extend_from_slice(&mesh_uniform_bytes(
                placed_matrix(instance, offset),
                MESH_FLAGS,
                slot,
                terrain_record_tag(instance, gpu.total_slots),
            )),
            None => uniform_bytes.resize(uniform_bytes.len() + MESH_UNIFORM_BYTES, 0),
        }
    }
    let mut run_start = 0usize;
    while run_start < arrived.len() {
        let batch = arrived[run_start].batch;
        let run_end = arrived[run_start..]
            .iter()
            .position(|instance| instance.batch != batch)
            .map_or(arrived.len(), |length| run_start + length);
        let slots: u32 = arrived[run_start..run_end]
            .iter()
            .map(|instance| scene.meshes[instance.mesh as usize].cluster_count)
            .sum();
        let entry = &mut gpu.batches[batch as usize];
        entry
            .ranges
            .push(base + run_start as u32..base + run_end as u32);
        entry.slots += slots;
        run_start = run_end;
    }
    if !arrived.is_empty() {
        queue.write_buffer(&gpu.instances, u64::from(base) * 80, &instance_bytes);
        queue.write_buffer(&gpu.absolute, u64::from(base) * 16, &absolute_bytes);
        queue.write_buffer(
            &gpu.mesh_uniforms,
            u64::from(base) * MESH_UNIFORM_BYTES as u64,
            &uniform_bytes,
        );
    }
    written += (instance_bytes.len() + absolute_bytes.len() + uniform_bytes.len()) as u64;
    gpu.placed.extend(arrived);
    gpu.total_instances += count as u32;
    gpu.layout_epoch += 1;
    gpu.blocks[index].appended = true;
    gpu.blocks[index]
        .resident
        .store(true, std::sync::atomic::Ordering::Release);
    queue_stats::bump(&queue_stats::BLOCKS_APPENDED);
    let quarters =
        (u64::from(gpu.total_instances) * 4 / u64::from(gpu.capacity_instances.max(1))) as u32;
    if quarters > gpu.logged_quarters {
        gpu.logged_quarters = quarters;
        info!(
            blocks_appended = gpu.blocks.iter().filter(|block| block.appended).count(),
            blocks = gpu.blocks.len(),
            instances = gpu.total_instances,
            of = gpu.capacity_instances,
            cooperative_scatter_instances = gpu.large_instances.len(),
            "cluster blocks reached the GPU"
        );
    }
    written
}

/// Everything the cluster path keeps on the GPU. Meshes are pooled: each section of every mesh is
/// appended to one shared buffer and the `meshes` table says where each one starts.
#[allow(dead_code)]
#[derive(Resource)]
struct ClusterGpu {
    /// Unique per creation of this resource: what is derived from its instances (the selection
    /// index, its bases) belongs to one creation and is rebuilt for another.
    identity: u64,
    certificate: Option<Arc<super::reader::CertifiedPackage>>,
    terrain_blocks: Vec<super::terrain::TerrainViewBlock>,
    positions: Buffer,
    /// Per vertex: packed octahedral normal, packed UV, vertex colour (three words).
    attributes: Buffer,
    clusters: Buffer,
    cluster_vertices: Buffer,
    cluster_indices: Buffer,
    groups: Buffer,
    meshes: Buffer,
    instances: Buffer,
    mesh_uniforms: Buffer,
    /// Per instance: its absolute translation, the source the shift pass places it from.
    absolute: Buffer,
    shift_layout: BindGroupLayoutDescriptor,
    shift_params: Buffer,
    cull_layout: BindGroupLayoutDescriptor,
    instance_cull_layout: BindGroupLayoutDescriptor,
    compaction_layouts: [BindGroupLayoutDescriptor; 6],
    draw_layout: BindGroupLayoutDescriptor,
    shadow_layout: BindGroupLayoutDescriptor,
    /// Built when the first batch is queued.
    mesh_bind_group: Option<BindGroup>,
    /// The instances that have arrived, in the order of the instance buffers (a block at a time,
    /// sorted by batch within the block).
    placed: Vec<ClusterInstance>,
    object_indices: HashMap<super::reader::RecordKey, u32>,
    /// The origin offset the instance and mesh uniform buffers were last written with.
    applied_offset: Vec3,
    /// Counts cull runs for per-view argument publication. Indirect scheduling is
    /// bounded by the exact GPU survivor count, including the rounded final dispatch row.
    epoch: u32,
    /// Bumped whenever a batch's material, and so what a view draws it with, is resolved anew; a
    /// shadow view whose plan was made under the same generation has nothing to redo.
    generation: u64,
    batches: Vec<BatchGpu>,
    /// The blocks of the scene, in arrival order, and whether each has been appended.
    blocks: Vec<GpuBlock>,
    /// Whether each mesh has been written into the pools yet.
    pooled: Vec<bool>,
    /// Whether each mesh is drawn only by terrain instances: its clusters are selected against
    /// [`ClusterSettings::terrain_error_pixels`].
    terrain_meshes: Vec<bool>,
    /// Where the next mesh goes in each pool.
    cursors: PoolCursors,
    /// Quarters of the scene's instances that have arrived and been logged.
    logged_quarters: u32,
    /// Bumped whenever instances arrive: the draw groups' sizes and the plans made from them are
    /// stale from then on.
    layout_epoch: u64,
    /// What the instance buffers hold room for (the whole scene), against `total_instances`, the
    /// ones that have arrived.
    capacity_instances: u32,
    total_instances: u32,
    /// The most clusters one view can select over every batch of the whole scene.
    total_slots: u32,
    next_slot: u32,
    /// Immutable scheduling metadata; large meshes retain cooperative cluster scatter.
    large_instances: Vec<u32>,
    /// Whether the adapter can read the draw count on the GPU (`multi_draw_indexed_indirect_count`); when
    /// it cannot, the draw covers the whole list and the cull pass leaves unused entries empty.
    use_count: bool,
    /// The source orders the views hold, shared between views with the same draw groups.
    order_cache: stable_order::OrderCache,
    views: HashMap<RetainedViewEntity, ViewGpu>,
}

/// The actual specialized layout decides which group-3 bind command is legal.
#[derive(Clone, Copy)]
struct CachedClusterPrepass {
    pipeline: CachedRenderPipelineId,
    empty_material: bool,
}

#[derive(Resource, Default)]
struct ClusterPipelines {
    cull: Option<CachedComputePipelineId>,
    instance_cull: Option<CachedComputePipelineId>,
    compaction: [Option<CachedComputePipelineId>; 6],
    /// Moves the instances when the render origin changes.
    shift: Option<CachedComputePipelineId>,
    /// Depth-only shadow pipelines, by whether the light needs unclipped depth (a sun cascade).
    shadow: HashMap<bool, CachedRenderPipelineId>,
    /// Shadow pipelines of alpha-masked materials: the stock prepass fragment stage of the
    /// material behind the cluster vertex stage, keyed as the stock shadow queue keys it.
    shadow_mask: HashMap<ClusterMaterialPipelineKey, CachedRenderPipelineId>,
    /// The camera prepass pipelines (depth, normals, motion vectors), by the same key; `None`
    /// where the material has no prepass pipeline to build.
    prepass: HashMap<ClusterMaterialPipelineKey, Option<CachedClusterPrepass>>,
    prepass_material_version: u64,
    /// Keyed by the material's type too: two material types with equal mesh keys still have
    /// different bind group layouts and fragment shaders.
    render: HashMap<(ClusterMaterialPipelineKey, bool), CachedRenderPipelineId>,
    /// Layout limits are fixed for this pipeline cache's device. A refused
    /// optional variant uses the ordinary terrain pipeline with the same key.
    far_layout_refusals: HashSet<ClusterMaterialPipelineKey>,
    /// Only compiled optional variants may replace an ordinary terrain draw.
    far_ready: HashSet<CachedRenderPipelineId>,
    far_readiness_revision: u64,
}

impl ClusterPipelines {
    fn refresh_prepass_refusals(&mut self, version: u64) {
        if self.prepass_material_version != version {
            self.prepass.retain(|_, pipeline| pipeline.is_some());
            self.prepass_material_version = version;
        }
    }

    fn refresh_far_readiness(&mut self, mut ready: impl FnMut(CachedRenderPipelineId) -> bool) {
        let next: HashSet<_> = self
            .render
            .iter()
            .filter_map(|((_, far), &id)| (*far && ready(id)).then_some(id))
            .collect();
        if self.far_ready != next {
            self.far_ready = next;
            self.far_readiness_revision = self.far_readiness_revision.wrapping_add(1);
        }
    }

    /// Resolve exactly the variant that can be queued. Optional layout refusal
    /// is sticky for this device/key; material specialization failures are not.
    fn forward_pipeline(
        &mut self,
        key: ClusterMaterialPipelineKey,
        far: bool,
        limits: &wgpu::Limits,
        mut build: impl FnMut(bool) -> Option<RenderPipelineDescriptor>,
        mut queue: impl FnMut(RenderPipelineDescriptor) -> CachedRenderPipelineId,
        mut ready: impl FnMut(CachedRenderPipelineId) -> bool,
    ) -> Option<(CachedRenderPipelineId, bool)> {
        let mut far = far && !self.far_layout_refusals.contains(&key);
        loop {
            if let Some(id) = self.render.get(&(key.clone(), far)) {
                if far && !ready(*id) {
                    far = false;
                    continue;
                }
                return Some((*id, far));
            }
            let descriptor = build(far)?;
            if far
                && let Err(reason) =
                    crate::far_terrain_shadows::layout::validate(&descriptor, limits)
            {
                note_pipeline_failure("canonical terrain shadow layout", &reason);
                self.far_layout_refusals.insert(key.clone());
                far = false;
                continue;
            }
            let id = queue(descriptor);
            self.render.insert((key.clone(), far), id);
            if far && !ready(id) {
                info!(
                    "optional terrain forward pipeline queued; retaining ordinary terrain until compiled"
                );
                far = false;
                continue;
            }
            return Some((id, far));
        }
    }
}

fn entry(binding: u32, buffer: &Buffer) -> BindGroupEntry<'_> {
    BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

/// The meshes of a scene appended section by section, and the table of where each one starts: the
/// whole-scene layout the streamed one (`append_block`) must equal, kept for the tests to compare
/// it with.
#[cfg(test)]
struct PooledMeshes {
    positions: Vec<u8>,
    attributes: Vec<u8>,
    clusters: Vec<u8>,
    cluster_vertices: Vec<u8>,
    cluster_indices: Vec<u8>,
    groups: Vec<u8>,
    /// `MeshInfo` rows: eight words of bases and counts, uv_min and uv_scale, then the bounding
    /// sphere.
    infos: Vec<u8>,
    /// Clusters of each mesh, to number the slots of the instances.
    cluster_counts: Vec<u32>,
}

/// Where the next mesh's sections go in each pooled buffer, in bytes.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct PoolCursors {
    positions: u64,
    attributes: u64,
    clusters: u64,
    cluster_vertices: u64,
    cluster_indices: u64,
    groups: u64,
}

impl PoolCursors {
    fn advance(&mut self, sizes: &PoolCursors) {
        self.positions += sizes.positions;
        self.attributes += sizes.attributes;
        self.clusters += sizes.clusters;
        self.cluster_vertices += sizes.cluster_vertices;
        self.cluster_indices += sizes.cluster_indices;
        self.groups += sizes.groups;
    }
}

/// The bytes one mesh takes in each pooled buffer (every section padded to words), without
/// building them: what the buffers are sized from before any mesh has arrived.
fn mesh_pool_sizes(mesh: &ClusterMeshAsset) -> PoolCursors {
    let padded_len = |section: MclSection| mesh.section(section).len().next_multiple_of(4) as u64;
    PoolCursors {
        positions: padded_len(MclSection::Positions),
        // Normal, UV, vertex colour and tangent words per vertex.
        attributes: u64::from(mesh.vertex_count) * 16,
        clusters: padded_len(MclSection::Clusters),
        cluster_vertices: padded_len(MclSection::ClusterVertices),
        cluster_indices: (mesh.section(MclSection::ClusterTriangles).len() as u64 * 2)
            .next_multiple_of(4),
        groups: padded_len(MclSection::Groups),
    }
}

/// One mesh's `MeshInfo` row: where its sections start (positions and cluster records in elements,
/// indices in u16 elements), its counts, whether only terrain draws it, uv_min and uv_scale, then its
/// bounding sphere.
fn mesh_info_row(at: &PoolCursors, mesh: &ClusterMeshAsset, terrain: bool) -> Vec<u8> {
    let mut row = Vec::with_capacity(64);
    push_u32(
        &mut row,
        &[
            (at.positions / 12) as u32,
            (at.clusters / 48) as u32,
            (at.cluster_vertices / 4) as u32,
            (at.cluster_indices / 2) as u32,
            (at.groups / 32) as u32,
            mesh.cluster_count,
            u32::from(terrain),
            0,
        ],
    );
    push_f32(
        &mut row,
        &[
            mesh.uv_min[0],
            mesh.uv_min[1],
            mesh.uv_scale[0],
            mesh.uv_scale[1],
        ],
    );
    push_f32(&mut row, &mesh.bounds);
    row
}

/// One mesh's sections as they lie in the pooled buffers.
struct MeshSections {
    positions: Vec<u8>,
    attributes: Vec<u8>,
    clusters: Vec<u8>,
    cluster_vertices: Vec<u8>,
    cluster_indices: Vec<u8>,
    groups: Vec<u8>,
}

/// Widen the payload's byte-sized local vertex indices once at upload. Uint16 is the
/// smallest portable index format; first_index addresses the pool while base_vertex stays
/// zero, so every vertex stage receives the original cluster-local index. Queue writes and
/// subsequent mesh starts remain four-byte aligned, including odd index counts.
fn cluster_index_bytes(triangles: &[u8]) -> Vec<u8> {
    let mut indices = Vec::with_capacity((triangles.len() * 2).next_multiple_of(4));
    for &index in triangles {
        indices.extend_from_slice(&u16::from(index).to_le_bytes());
    }
    indices.resize(indices.len().next_multiple_of(4), 0);
    indices
}

/// Builds the pooled form of one mesh: the per-vertex interleave, the tangent words and the
/// sections padded to words. The tangents are a read when `prepare_tangents` has run for the mesh.
fn mesh_sections(mesh: &ClusterMeshAsset) -> MeshSections {
    let normals = mesh.section(MclSection::Normals);
    let uvs = mesh.section(MclSection::Uvs);
    // Vertex colours are optional; a mesh without them is opaque white, which multiplies a
    // material's colour by one, the same as the stock path's mesh with no colour attribute.
    let colours = (mesh.flags & shared::cluster::FLAG_VERTEX_COLOURS != 0)
        .then(|| mesh.section(MclSection::Colours));
    let tangents = mesh.tangent_words();
    let mut attributes = Vec::with_capacity(mesh.vertex_count as usize * 16);
    for vertex in 0..mesh.vertex_count as usize {
        attributes.extend_from_slice(&normals[vertex * 4..vertex * 4 + 4]);
        attributes.extend_from_slice(&uvs[vertex * 4..vertex * 4 + 4]);
        match colours {
            Some(colours) => attributes.extend_from_slice(&colours[vertex * 4..vertex * 4 + 4]),
            None => attributes.extend_from_slice(&[255u8; 4]),
        }
        attributes.extend_from_slice(&tangents[vertex].to_le_bytes());
    }
    MeshSections {
        positions: padded(mesh.section(MclSection::Positions)),
        attributes,
        clusters: padded(mesh.section(MclSection::Clusters)),
        cluster_vertices: padded(mesh.section(MclSection::ClusterVertices)),
        cluster_indices: cluster_index_bytes(mesh.section(MclSection::ClusterTriangles)),
        groups: padded(mesh.section(MclSection::Groups)),
    }
}

#[cfg(test)]
fn pool_meshes(meshes: &[ClusterMeshAsset]) -> PooledMeshes {
    let mut pool = PooledMeshes {
        positions: Vec::new(),
        attributes: Vec::new(),
        clusters: Vec::new(),
        cluster_vertices: Vec::new(),
        cluster_indices: Vec::new(),
        groups: Vec::new(),
        infos: Vec::new(),
        cluster_counts: Vec::new(),
    };
    let mut at = PoolCursors::default();
    for mesh in meshes {
        pool.infos
            .extend_from_slice(&mesh_info_row(&at, mesh, false));
        pool.cluster_counts.push(mesh.cluster_count);
        let sections = mesh_sections(mesh);
        at.advance(&PoolCursors {
            positions: sections.positions.len() as u64,
            attributes: sections.attributes.len() as u64,
            clusters: sections.clusters.len() as u64,
            cluster_vertices: sections.cluster_vertices.len() as u64,
            cluster_indices: sections.cluster_indices.len() as u64,
            groups: sections.groups.len() as u64,
        });
        pool.positions.extend_from_slice(&sections.positions);
        pool.attributes.extend_from_slice(&sections.attributes);
        pool.clusters.extend_from_slice(&sections.clusters);
        pool.cluster_vertices
            .extend_from_slice(&sections.cluster_vertices);
        pool.cluster_indices
            .extend_from_slice(&sections.cluster_indices);
        pool.groups.extend_from_slice(&sections.groups);
    }
    pool
}

/// Whether the scene fits what the adapter binds and allocates: every pooled section, the instance
/// tables and per-view lists (20-byte arguments, 16-byte visible records) in one storage binding
/// each, the index pool as an index buffer, and the slot and
/// instance totals in the 32-bit indices the shaders use. The sizes are summed in 64 bits, so
/// nothing wraps before it is compared.
fn check_scene_limits(
    device: &RenderDevice,
    totals: &PoolCursors,
    scene: &ClusterScene,
) -> Result<(), String> {
    let limits = device.limits();
    let allowed = limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size);
    let instances = scene.instances.len() as u64;
    let slots: u64 = scene
        .instances
        .iter()
        .map(|instance| {
            scene
                .meshes
                .get(instance.mesh as usize)
                .map_or(0, |mesh| u64::from(mesh.cluster_count))
        })
        .sum();
    if instances > u64::from(u32::MAX) || slots > u64::from(u32::MAX) {
        return Err(format!(
            "{instances} instances and {slots} cluster slots do not fit 32-bit indices"
        ));
    }
    let view_words = slots
        .max(1)
        .checked_add(
            (scene.terrain_blocks.len() as u64)
                .checked_mul(u64::from(super::terrain::TERRAIN_VIEW_VEC4S))
                .ok_or("terrain clip record count overflow")?,
        )
        .ok_or("per-view terrain record index overflow")?;
    if view_words > u64::from(u32::MAX) {
        return Err(format!(
            "{view_words} draw/terrain words do not fit shader indices"
        ));
    }
    let sizes = [
        ("positions", totals.positions),
        ("vertex attributes", totals.attributes),
        ("cluster records", totals.clusters),
        ("cluster vertex references", totals.cluster_vertices),
        (
            "per-view indexed draw arguments",
            slots.max(1) * DRAW_INDEXED_INDIRECT_BYTES,
        ),
        ("groups", totals.groups),
        ("instance table", instances * 80),
        ("mesh uniforms", instances * MESH_UNIFORM_BYTES as u64),
        ("per-view draw list and terrain records", view_words * 16),
    ];
    for (name, bytes) in sizes {
        if bytes > allowed {
            return Err(format!(
                "the {name} need {bytes} bytes, over the {allowed} the adapter binds in one storage buffer"
            ));
        }
    }
    if totals.cluster_indices > limits.max_buffer_size
        || totals.cluster_indices / 2 > u64::from(u32::MAX)
    {
        return Err(format!(
            "the cluster index pool needs {} bytes, exceeding the adapter buffer size or 32-bit index addressing",
            totals.cluster_indices
        ));
    }
    info!(
        instances,
        slots,
        view_list_mib = view_words * 16 / (1 << 20),
        pooled_mib = (totals.positions
            + totals.attributes
            + totals.clusters
            + totals.cluster_vertices
            + totals.cluster_indices
            + totals.groups)
            / (1 << 20),
        index_pool_mib = totals.cluster_indices / (1 << 20),
        "cluster scene sizes (each view holds 20-byte arguments and 16-byte visible records per slot)"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn prepare_cluster_gpu(
    mut commands: Commands,
    scene: Option<Res<ClusterScene>>,
    existing: Option<ResMut<ClusterGpu>>,
    queue: Res<RenderQueue>,
    device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    asset_server: Res<AssetServer>,
    mut pipelines: ResMut<ClusterPipelines>,
    settings: Res<ClusterSettings>,
    frame: Res<ClusterFrame>,
    mut refused: Local<bool>,
) {
    perf_span!("cluster::prepare::total");
    let _timer = queue_stats::Timed::start(&queue_stats::PREPARE_NANOS);
    if *refused {
        return;
    }
    let Some(scene) = scene else {
        return;
    };
    if let Some(mut gpu) = existing {
        append_ready_blocks(&mut gpu, &scene, &queue);
        return;
    }
    if scene.meshes.is_empty() || scene.instances.is_empty() {
        return;
    }
    if let Err(reason) = super::gpu_support(device.features(), &device.limits()) {
        error!("the cluster path cannot run on this GPU, drawing none: {reason}");
        *refused = true;
        return;
    }
    // The buffers are sized for the whole scene now and filled block by block afterwards
    // (`append_ready_blocks`): what each pool needs is a sum over the meshes' section lengths, which
    // does not build any of them.
    let mut totals = PoolCursors::default();
    for mesh in &scene.meshes {
        totals.advance(&mesh_pool_sizes(mesh));
    }
    if let Err(why) = check_scene_limits(&device, &totals, &scene) {
        error!("the cluster scene cannot be uploaded, drawing none: {why}");
        *refused = true;
        return;
    }
    let storage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
    let make = |label: &str, size: u64| {
        device.create_buffer(&bevy::render::render_resource::BufferDescriptor {
            label: Some(label),
            // A storage buffer cannot be empty.
            size: size.max(4),
            usage: storage,
            mapped_at_creation: false,
        })
    };
    let positions = make("cluster positions", totals.positions);
    let attributes = make("cluster attributes", totals.attributes);
    let clusters = make("cluster records", totals.clusters);
    let cluster_vertices = make("cluster vertex references", totals.cluster_vertices);
    let cluster_indices = device.create_buffer(&bevy::render::render_resource::BufferDescriptor {
        label: Some("cluster local indices"),
        size: totals.cluster_indices.max(4),
        usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let groups = make("cluster groups", totals.groups);
    let meshes = make("cluster mesh table", scene.meshes.len() as u64 * 64);
    let batch_total = scene
        .instances
        .iter()
        .map(|instance| instance.batch + 1)
        .max()
        .unwrap_or(0) as usize;
    let source_layouts = match batch_source_tangents(&scene, batch_total) {
        Ok(layouts) => layouts,
        Err(reason) => {
            error!(%reason, "invalid cluster batch layout; refusing GPU scene");
            *refused = true;
            return;
        }
    };
    let batches: Vec<BatchGpu> = source_layouts
        .into_iter()
        .map(|source_tangents| BatchGpu {
            ranges: Vec::new(),
            slots: 0,
            material: None,
            material_slot: None,
            cutout: None,
            source_tangents,
            tangents: None,
        })
        .collect();
    // The most clusters one view can select from the whole scene: what the per-view lists hold.
    let slot_total: u32 = scene
        .instances
        .iter()
        .map(|instance| scene.meshes[instance.mesh as usize].cluster_count)
        .sum();
    let capacity = scene.instances.len() as u64;
    let instances = make("cluster instances", capacity * 80);
    let absolute = make("cluster absolute translations", capacity * 16);
    let shift_params = device.create_buffer(&bevy::render::render_resource::BufferDescriptor {
        label: Some("cluster shift params"),
        size: 32,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    // One MeshUniform per instance; written when the instance arrives and again once its batch's
    // material slot is known.
    let mesh_uniforms = make(
        "cluster mesh uniforms",
        capacity * MESH_UNIFORM_BYTES as u64,
    );
    let blocks: Vec<GpuBlock> = if scene.blocks.is_empty() {
        // A scene without blocks (the fixture) is one block that is available at once.
        vec![GpuBlock {
            first: 0,
            count: scene.instances.len() as u32,
            meshes: (0..scene.meshes.len() as u32).collect(),
            ready: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            resident: Arc::default(),
            appended: false,
        }]
    } else {
        scene
            .blocks
            .iter()
            .map(|block| GpuBlock {
                first: block.first_instance,
                count: block.instance_count,
                meshes: block.meshes.clone(),
                ready: block.ready.clone(),
                resident: block.resident.clone(),
                appended: false,
            })
            .collect()
    };

    let read_only = || storage_buffer_read_only_sized(false, None);
    // Separate layouts keep every entry point within the portable eight-storage-buffer limit.
    let layout = |label: &'static str, bindings: &[u32]| {
        let entries: Vec<_> = bindings
            .iter()
            .map(|&binding| {
                if binding == 0 {
                    uniform_buffer_sized(false, NonZeroU64::new(CULL_PARAMS_BYTES as u64))
                } else if matches!(binding, 1 | 2 | 3 | 6 | 10 | 13) {
                    read_only()
                } else {
                    storage_buffer_sized(false, None)
                }
                .build(binding, ShaderStages::COMPUTE)
            })
            .collect();
        BindGroupLayoutDescriptor::new(label, &entries)
    };
    let cull_layout = layout("cluster cull layout", STAGES[3].1);
    let instance_cull_layout = layout("cluster instance cull layout", STAGES[0].1);
    let compaction_layouts = [
        layout("cluster instance prefix layout", STAGES[1].1),
        layout("cluster instance scatter layout", STAGES[2].1),
        layout("cluster draw tile prefix layout", STAGES[4].1),
        layout("cluster draw group prefix layout", STAGES[5].1),
        layout("cluster draw scatter layout", STAGES[6].1),
        layout("cluster large scatter layout", STAGES[7].1),
    ];
    let mut draw_layout = BindGroupLayoutDescriptor::new(
        "cluster draw layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::VERTEX,
            (
                (0, read_only()),
                (1, read_only()),
                (2, read_only()),
                (3, read_only()),
                (5, read_only()),
                (6, read_only()),
            ),
        ),
    );
    draw_layout.entries[5].visibility = ShaderStages::VERTEX_FRAGMENT;
    let shadow_layout = BindGroupLayoutDescriptor::new(
        "cluster shadow layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::VERTEX,
            (
                (0, read_only()),
                (1, read_only()),
                (2, read_only()),
                (4, read_only()),
                (5, read_only()),
                (6, read_only()),
            ),
        ),
    );
    let shift_layout = BindGroupLayoutDescriptor::new(
        "cluster shift layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::COMPUTE,
            (
                (0, uniform_buffer_sized(false, NonZeroU64::new(32))),
                (1, read_only()),
                (2, storage_buffer_sized(false, None)),
                (3, storage_buffer_sized(false, None)),
            ),
        ),
    );
    pipelines.shift = Some(
        pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("cluster shift".into()),
            layout: vec![shift_layout.clone()],
            shader: asset_server.load("embedded://engine/cluster/shaders/cluster_shift.wgsl"),
            entry_point: Some("shift".into()),
            ..default()
        }),
    );
    let cull_shader = asset_server.load("embedded://engine/cluster/shaders/cluster_cull.wgsl");
    pipelines.cull = Some(
        pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("cluster cull".into()),
            layout: vec![cull_layout.clone()],
            shader: cull_shader.clone(),
            entry_point: Some(STAGES[3].0.into()),
            ..default()
        }),
    );
    pipelines.instance_cull = Some(pipeline_cache.queue_compute_pipeline(
        ComputePipelineDescriptor {
            label: Some("cluster instance cull".into()),
            layout: vec![instance_cull_layout.clone()],
            shader: cull_shader.clone(),
            entry_point: Some(STAGES[0].0.into()),
            ..default()
        },
    ));
    for (index, name) in COMPACTION_ENTRY_POINTS.iter().enumerate() {
        pipelines.compaction[index] = Some(pipeline_cache.queue_compute_pipeline(
            ComputePipelineDescriptor {
                label: Some((*name).into()),
                layout: vec![compaction_layouts[index].clone()],
                shader: cull_shader.clone(),
                entry_point: Some((*name).into()),
                ..default()
            },
        ));
    }
    info!(
        meshes = scene.meshes.len(),
        instances = scene.instances.len(),
        blocks = blocks.len(),
        batches = batches.len(),
        slots = slot_total,
        bytes = totals.positions
            + totals.attributes
            + totals.clusters
            + totals.cluster_vertices
            + totals.cluster_indices
            + totals.groups,
        "cluster buffers created (empty: blocks are appended as they become ready)"
    );
    static CREATED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    commands.insert_resource(ClusterGpu {
        identity: CREATED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1,
        certificate: scene.certificate.clone(),
        positions,
        attributes,
        clusters,
        cluster_vertices,
        cluster_indices,
        groups,
        meshes,
        instances,
        mesh_uniforms,
        absolute,
        shift_layout,
        shift_params,
        cull_layout,
        instance_cull_layout,
        compaction_layouts,
        draw_layout,
        shadow_layout,
        mesh_bind_group: None,
        placed: Vec::new(),
        object_indices: HashMap::new(),
        applied_offset: frame.origin_offset,
        epoch: 0,
        generation: 0,
        layout_epoch: 0,
        logged_quarters: 0,
        batches,
        blocks,
        pooled: vec![false; scene.meshes.len()],
        terrain_meshes: {
            let terrain = super::package::terrain_only_meshes(&scene.instances);
            (0..scene.meshes.len() as u32)
                .map(|mesh| terrain.contains(&mesh))
                .collect()
        },
        cursors: PoolCursors::default(),
        capacity_instances: scene.instances.len() as u32,
        total_instances: 0,
        total_slots: slot_total,
        next_slot: 0,
        large_instances: Vec::new(),
        terrain_blocks: scene.terrain_blocks.clone(),
        use_count: !settings.fixed_count_draw
            && device
                .features()
                .contains(bevy::render::settings::WgpuFeatures::MULTI_DRAW_INDIRECT_COUNT),
        order_cache: stable_order::OrderCache::default(),
        views: HashMap::new(),
    });
}

/// An instance's matrix as uploaded: its absolute transform moved by the render-origin offset.
fn placed_matrix(instance: &ClusterInstance, offset: Vec3) -> Mat4 {
    let mut matrix = instance.world_from_local;
    matrix.w_axis += offset.extend(0.0);
    matrix
}

/// Mesh flags: no visibility-range LOD (the low 16 bits are the LOD index and `u16::MAX` means
/// none; zero would name range 0 and dither fragments away by distance) and shadow receiver
/// (1 << 29). The determinant sign (1 << 31) is per instance.
const MESH_FLAGS: u32 = (u16::MAX as u32) | (1u32 << 29);

/// The shared host/WGSL cull uniform, including independent object/terrain thresholds.
const CULL_PARAMS_BYTES: usize = std::mem::size_of::<CullParams>();

/// `MeshUniform` as the stock shaders read it (see `bevy_pbr`'s `mesh_types.wgsl`): three rows of
/// the world transform, the same for the previous frame, two vec4 and a float of the inverse
/// transpose, then eight u32 (flags, lightmap rect, first vertex, skin, material slot, tag, morph).
const MESH_UNIFORM_BYTES: usize = 176;

fn terrain_record_tag(instance: &ClusterInstance, visible_slots: u32) -> u32 {
    match instance.identity {
        ClusterInstanceIdentity::Terrain { block, .. } => visible_slots
            .max(1)
            .checked_add(
                block
                    .checked_mul(super::terrain::TERRAIN_VIEW_VEC4S)
                    .expect("terrain clip offset fits"),
            )
            .expect("terrain clip index fits"),
        _ => 0,
    }
}

fn mesh_uniform_bytes(world_from_local: Mat4, flags: u32, material_slot: u32, tag: u32) -> Vec<u8> {
    let rows = |matrix: Mat4| -> [f32; 12] {
        let cols = matrix.to_cols_array_2d();
        let mut out = [0.0; 12];
        for row in 0..3 {
            for col in 0..4 {
                out[row * 4 + col] = cols[col][row];
            }
        }
        out
    };
    // Bevy sets the sign bit when the transform keeps winding (a positive determinant).
    let flags = if Mat3::from_mat4(world_from_local).determinant() >= 0.0 {
        flags | (1u32 << 31)
    } else {
        flags
    };
    let normal = Mat3::from_mat4(world_from_local).inverse().transpose();
    let n = normal.to_cols_array();
    let mut bytes = Vec::with_capacity(MESH_UNIFORM_BYTES);
    push_f32(&mut bytes, &rows(world_from_local));
    push_f32(&mut bytes, &rows(world_from_local));
    push_f32(
        &mut bytes,
        &[n[0], n[1], n[2], n[3], n[4], n[5], n[6], n[7], n[8]],
    );
    // flags, lightmap_uv_rect (2), first_vertex_index, current_skin_index, slot, tag, morph.
    push_u32(
        &mut bytes,
        &[
            flags,
            0,
            0,
            0,
            u32::MAX,
            material_slot | (u32::from(u16::MAX) << 16),
            tag,
            u32::MAX,
        ],
    );
    bytes.resize(MESH_UNIFORM_BYTES, 0);
    bytes
}

/// The stock render-world tables the batch's dummy entity is looked up in.
#[derive(bevy::ecs::system::SystemParam)]
struct StockLookups<'w> {
    sources: Res<'w, handoff::Sources>,
    materials: Res<'w, ErasedRenderAssets<PreparedMaterial>>,
    render_meshes: Res<'w, RenderAssets<RenderMesh>>,
    far: Option<Res<'w, crate::far_terrain_shadows::render::State>>,
}

/// The draw functions the camera queue registers its items with.
#[derive(bevy::ecs::system::SystemParam)]
struct QueueDrawFunctions<'w> {
    opaque: Res<'w, DrawFunctions<Opaque3d>>,
    alpha_mask: Res<'w, DrawFunctions<AlphaMask3d>>,
}

/// The bin key of a batch's draw item: distinct per batch, so no two batches share a bin.
fn batch_asset_id(index: u32) -> bevy::asset::UntypedAssetId {
    AssetId::<Mesh>::Uuid {
        uuid: bevy::asset::uuid::Uuid::from_u128(u128::from(index) + 1),
    }
    .untyped()
}

#[allow(clippy::too_many_arguments)]
fn queue_cluster_batches(
    pipeline_cache: Res<PipelineCache>,
    material_pipeline: Res<MaterialPipeline>,
    mut pipelines: ResMut<ClusterPipelines>,
    mut gpu: Option<ResMut<ClusterGpu>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    asset_server: Res<AssetServer>,
    mut opaque_phases: ResMut<ViewBinnedRenderPhases<Opaque3d>>,
    mut alpha_mask_phases: ResMut<ViewBinnedRenderPhases<AlphaMask3d>>,
    queue_draw_functions: QueueDrawFunctions,
    views: Query<&ExtractedView>,
    view_key_cache: Res<ViewKeyCache>,
    stock: StockLookups,
    batch_materials: Res<ClusterBatchMaterials>,
    dummy_mesh: Option<Res<ClusterDummyMesh>>,
    mut carriers: CarrierWatch,
) {
    perf_span!("cluster::queue_batches::total");
    let markers = carriers.markers;
    let far_pipeline_log = &mut carriers.far_pipeline_log;
    let mut carrier_scratch = carriers.carrier_scratch;
    let _timer = queue_stats::Timed::start(&queue_stats::BATCH_NANOS);
    queue_stats::frame_done();
    let StockLookups {
        sources,
        materials,
        render_meshes,
        far: far_state,
    } = stock;
    let Some(gpu) = gpu.as_deref_mut() else {
        return;
    };
    let Some(dummy_mesh) = dummy_mesh else {
        return;
    };
    if far_state.is_some() {
        // Async compilation and shader reloads do not emit material events. A
        // changed compiled optional set must wake an otherwise unchanged walk.
        pipelines.refresh_far_readiness(|id| pipeline_cache.get_render_pipeline(id).is_some());
        if bevy::log::tracing::enabled!(target: "engine::cluster::render::far_pipeline", bevy::log::tracing::Level::DEBUG)
        {
            for ((_, far), &id) in &pipelines.render {
                if !far {
                    continue;
                }
                let state = match pipeline_cache.get_render_pipeline_state(id) {
                    CachedPipelineState::Queued => "queued".to_owned(),
                    CachedPipelineState::Creating(..) => "creating".to_owned(),
                    CachedPipelineState::Ok(_) => "ready".to_owned(),
                    CachedPipelineState::Err(error) => format!("error: {error}"),
                };
                if far_pipeline_log.get(&id.id()) != Some(&state) {
                    far_pipeline_log.insert(id.id(), state.clone());
                    // A pipeline queued this frame is not in the cache's descriptor list yet, and
                    // reading its descriptor indexes past the end (as `stock_drawable` guards).
                    let descriptor = (!matches!(
                        pipeline_cache.get_render_pipeline_state(id),
                        CachedPipelineState::Queued
                    ))
                    .then(|| pipeline_cache.get_render_pipeline_descriptor(id));
                    debug!(target: "engine::cluster::render::far_pipeline", pipeline = id.id(),
                        %state, vertex = ?descriptor.map(|descriptor| &descriptor.vertex.shader),
                        fragment = ?descriptor.and_then(|descriptor| descriptor.fragment.as_ref().map(|stage| &stage.shader)),
                        "optional terrain pipeline progress");
                }
            }
        }
    }
    let draw_function = queue_draw_functions.opaque.read().id::<DrawClusterBatch>();
    let alpha_mask_draw_function = queue_draw_functions
        .alpha_mask
        .read()
        .id::<DrawClusterBatch>();
    // Every camera reads the same exact index-to-entity mapping this frame.
    carrier_scratch.clear();
    let carriers = &mut *carrier_scratch;
    for (render_entity, main_entity, marker) in &markers {
        if carriers.len() <= marker.index as usize {
            carriers.resize(marker.index as usize + 1, None);
        }
        carriers[marker.index as usize] = Some((render_entity, *main_entity));
    }
    for view in &views {
        let Some(phase) = opaque_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let mut alpha_mask_phase = alpha_mask_phases.get_mut(&view.retained_view_entity);
        // Carriers are draw-command identities, not scene geometry. Their own
        // default render layer must not exclude the destination camera. Exact
        // per-view object/world/layer selection is made by the instance mask.
        let Some(view_key) = view_key_cache.get(&view.retained_view_entity).copied() else {
            continue;
        };
        let key = view.retained_view_entity;
        if !gpu.views.contains_key(&key) {
            let view_gpu = new_view_gpu(&device, &pipeline_cache, gpu);
            gpu.views.insert(key, view_gpu);
        }
        far::refresh(gpu, key, far_state.as_deref(), &device, &pipeline_cache);
        // The walk below is skipped when everything it reads is what it read last time. The
        // batch materials' own version stands for the prepared materials (see there); the exact
        // index-to-identity carrier mapping is compared independently of query arrival order.
        let mut mesh_hasher = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(
            &(dummy_mesh.plain.id(), dummy_mesh.tangent.id()),
            &mut mesh_hasher,
        );
        let stamp: [u64; 10] = [
            sources.layout_version,
            gpu.views[&key]
                .far
                .as_ref()
                .map_or(0, |binding| binding.prepared.nonce),
            view_key.bits(),
            std::hash::Hasher::finish(&mesh_hasher),
            batch_materials.version,
            gpu.batches.len() as u64,
            u64::from(gpu.total_instances),
            gpu.generation,
            u64::from(phase.non_mesh_items.is_empty())
                | (u64::from(
                    alpha_mask_phase
                        .as_deref()
                        .is_some_and(|phase| phase.non_mesh_items.is_empty()),
                ) << 1)
                | (u64::from(render_meshes.get(dummy_mesh.plain.id()).is_some()) << 2)
                | (u64::from(render_meshes.get(dummy_mesh.tangent.id()).is_some()) << 3),
            pipelines.far_readiness_revision,
        ];
        let view_gpu = &gpu.views[&key];
        if view_gpu.walk_stamp == Some(stamp)
            && view_gpu.walk_incomplete.is_empty()
            && view_gpu.walk_carriers == *carriers
        {
            queue_stats::bump(&queue_stats::BATCH_SKIPPED);
            continue;
        }
        let batch_count = gpu.batches.len();
        let walk_base = |mut stamp: [u64; 10]| {
            stamp[4] = 0;
            stamp[8] &= !0b11;
            stamp
        };
        let same_base = view_gpu.walk_base == Some(walk_base(stamp))
            && view_gpu.walk_ready.len() == batch_count
            && view_gpu.walk_terrain.len() == batch_count;
        let changed: Option<Vec<usize>> = if !same_base {
            None
        } else if view_gpu.walked_version == batch_materials.version {
            Some(Vec::new())
        } else if view_gpu.walked_version.wrapping_add(1) == batch_materials.version {
            Some(
                batch_materials
                    .changed_batches
                    .iter()
                    .map(|batch| *batch as usize)
                    .collect(),
            )
        } else {
            None
        };
        perf_span!("cluster::queue_batches::walk");
        queue_stats::bump(&queue_stats::BATCH_WALKS);
        let view_gpu = gpu.views.get_mut(&key).unwrap();
        // By batch index, so the plan below does not depend on the order Bevy hands the visible
        // lists over in (parallel queues: a different order every frame).
        let mut by_index = std::mem::take(&mut view_gpu.walk_ready);
        // Diagnostic: how far each terrain batch gets (see `note_terrain_walk`).
        let mut terrain_stage = std::mem::take(&mut view_gpu.walk_terrain);
        let mut incomplete = std::mem::take(&mut view_gpu.walk_incomplete);
        let indices: Vec<usize> = match changed {
            Some(changed) => {
                let mut indices: std::collections::BTreeSet<usize> =
                    incomplete.iter().map(|batch| *batch as usize).collect();
                indices.extend(changed.into_iter().filter(|index| *index < batch_count));
                indices.into_iter().collect()
            }
            None => {
                by_index = vec![None; batch_count];
                terrain_stage = vec![None; batch_count];
                incomplete.clear();
                (0..batch_count).collect()
            }
        };
        queue_stats::BATCH_RESOLVED
            .fetch_add(indices.len() as u64, std::sync::atomic::Ordering::Relaxed);
        let terrain_type = std::any::TypeId::of::<super::terrain::ClusterTerrainMaterial>();
        for index in indices {
            by_index[index] = None;
            terrain_stage[index] = None;
            incomplete.remove(&(index as u32));
            if gpu.batches[index].slots == 0 {
                continue;
            }
            let Some(Some(asset_id)) = batch_materials.ids.get(index).copied() else {
                invalidate_batch_material(gpu, index);
                // Lazy batches may have no handle indefinitely. Handle changes
                // wake the walk through batch_materials.version.
                continue;
            };
            let Some(geometry) = batch_materials.geometry.get(&asset_id).copied() else {
                invalidate_batch_material(gpu, index);
                // Main-world asset arrivals/removals have material events.
                continue;
            };
            let Some(tangents) =
                batch_tangent_layout(&gpu.batches[index], index, asset_id, geometry, &sources)
            else {
                invalidate_batch_material(gpu, index);
                // A known unsupported layout cannot become supported without
                // a material event. Never turn that refusal into a frame scan.
                continue;
            };
            let mesh_asset = dummy_mesh.asset(tangents);
            let is_terrain = asset_id.type_id() == terrain_type;
            let mut stage = |reached: u8| {
                if is_terrain {
                    terrain_stage[index] = Some(reached);
                }
            };
            stage(0);
            let Some(material) = materials.get(asset_id) else {
                stage(1);
                invalidate_batch_material(gpu, index);
                incomplete.insert(index as u32);
                continue;
            };
            let Some(render_mesh) = render_meshes.get(mesh_asset) else {
                stage(2);
                invalidate_batch_material(gpu, index);
                incomplete.insert(index as u32);
                continue;
            };
            // The material's own key bits, plus the alpha mode's: an alpha-masked
            // material discards in the fragment stage only when the key says it may.
            let mut material_bits: MeshPipelineKey =
                material.properties.mesh_pipeline_key_bits.downcast();
            material_bits.insert(alpha_mode_pipeline_key(
                material.properties.alpha_mode,
                &Msaa::from_samples(view_key.msaa_samples()),
            ));
            let mesh_key = view_key
                | MeshPipelineKey::from_bits_retain(render_mesh.key_bits.bits())
                | material_bits;
            // The same key the stock queue specializes by: the mesh key, the
            // material's own key (its cull mode, double-sidedness, extension flags)
            // and its type.
            let pipeline_key = ClusterMaterialPipelineKey {
                material: ErasedMaterialPipelineKey {
                    mesh_key: ErasedMeshPipelineKey::new(mesh_key),
                    material_key: material.properties.material_key.clone(),
                    type_id: asset_id.type_id(),
                },
                tangents,
            };
            let mut far_variant = is_terrain
                && far::current(gpu, &gpu.views[&key], far_state.as_deref())
                && !pipelines.far_layout_refusals.contains(&pipeline_key);
            let selected_far = if far_variant {
                pipelines
                    .render
                    .get(&(pipeline_key.clone(), true))
                    .map(|id| pipelines.far_ready.contains(id))
            } else {
                Some(false)
            };
            // What this batch resolved to last time, when none of its inputs has changed since:
            // the same material instance, prepared material (by address, which moves when the
            // asset is prepared again) and mesh, under the same view key.
            let material_address = std::ptr::from_ref(material) as usize;
            let properties = material_properties_hash(material);
            let cached = gpu.views[&key].memo[index].filter(|memo| {
                Some(memo.resolved.far) == selected_far
                    && memo.properties == properties
                    && memo.asset_id == asset_id
                    && memo.material_address == material_address
                    && memo.material_slot == u32::from(material.binding.slot)
                    && memo.material_group == material.binding.group.0
                    && memo.alpha_mode == material.properties.alpha_mode
                    && gpu.batches[index].material == Some(asset_id)
                    && gpu.batches[index].tangents == Some(tangents)
                    && memo.tangents == tangents
                    && memo.mesh_asset == mesh_asset
                    && memo.view_key == view_key
            });
            let resolved = match cached {
                Some(memo) => memo.resolved,
                None => {
                    let pipeline = pipelines.forward_pipeline(
                        pipeline_key.clone(),
                        far_variant,
                        &device.limits(),
                        |far| {
                            build_descriptor(
                                &material_pipeline,
                                pipeline_key.clone(),
                                &render_mesh.layout,
                                material,
                                if far {
                                    &gpu.views[&key].far.as_ref().unwrap().layout
                                } else {
                                    &gpu.draw_layout
                                },
                                &asset_server,
                                far,
                            )
                        },
                        |descriptor| pipeline_cache.queue_render_pipeline(descriptor),
                        |id| pipeline_cache.get_render_pipeline(id).is_some(),
                    );
                    let Some((pipeline_id, selected_far)) = pipeline else {
                        invalidate_batch_material(gpu, index);
                        stage(2);
                        incomplete.insert(index as u32);
                        continue;
                    };
                    far_variant = selected_far;
                    gpu.batches[index].material = Some(asset_id);
                    gpu.batches[index].tangents = Some(tangents);
                    gpu.generation = gpu.generation.wrapping_add(1);
                    // The batch's mesh uniforms need its material's slot, known only now.
                    let slot = u32::from(material.binding.slot);
                    if gpu.batches[index].material_slot != Some(slot) {
                        for range in &gpu.batches[index].ranges {
                            let (first, end) = (range.start as usize, range.end as usize);
                            let mut bytes = Vec::new();
                            bytes_for_instances(
                                &mut bytes,
                                MESH_FLAGS,
                                slot,
                                &gpu.placed[first..end],
                                gpu.applied_offset,
                                gpu.total_slots,
                            );
                            queue.write_buffer(
                                &gpu.mesh_uniforms,
                                (first * MESH_UNIFORM_BYTES) as u64,
                                &bytes,
                            );
                        }
                        gpu.batches[index].material_slot = Some(slot);
                    }
                    if gpu.mesh_bind_group.is_none() {
                        gpu.mesh_bind_group =
                            Some(material_pipeline.mesh_pipeline.mesh_layouts.model_only(
                                &device,
                                &pipeline_cache,
                                &gpu.mesh_uniforms.as_entire_binding(),
                            ));
                    }
                    let phase_kind = match material.properties.render_phase_type {
                        RenderPhaseType::Opaque => Some(false),
                        RenderPhaseType::AlphaMask => Some(true),
                        // Blended and transmissive materials need back-to-front sorting, which
                        // a GPU selected draw list cannot give them. Complementary selection
                        // retains their stock primitives.
                        _ => {
                            note_pipeline_failure(
                                "a blended or transmissive material",
                                &"the cluster path draws opaque and alpha-masked materials only",
                            );
                            None
                        }
                    };
                    if phase_kind.is_none() {
                        invalidate_batch_material(gpu, index);
                        continue;
                    }
                    gpu.batches[index].cutout = (phase_kind == Some(true)).then(|| CutoutShadow {
                        asset: asset_id,
                        type_id: asset_id.type_id(),
                        material_key: material.properties.material_key.clone(),
                        material_group: material.binding.group.0,
                    });
                    let resolved = ResolvedBatch {
                        pipeline: pipeline_id,
                        far: far_variant,
                        material_group: material.binding.group.0,
                        alpha_mask: phase_kind,
                        bindless: material.properties.bindless,
                    };
                    if let Some(view_gpu) = gpu.views.get_mut(&key)
                        && let Some(entry) = view_gpu.memo.get_mut(index)
                    {
                        view_gpu.readiness_revision += 1;
                        *entry = Some(BatchMemo {
                            properties,
                            asset_id,
                            material_address,
                            material_slot: u32::from(material.binding.slot),
                            material_group: material.binding.group.0,
                            alpha_mode: material.properties.alpha_mode,
                            mesh_asset,
                            tangents,
                            view_key,
                            resolved,
                        });
                    }
                    resolved
                }
            };
            let alpha_mask = match resolved.alpha_mask {
                Some(false) => false,
                Some(true) if alpha_mask_phase.is_some() => true,
                _ => {
                    stage(3);
                    continue;
                }
            };
            stage(4);
            by_index[index] = Some(ReadyBatch {
                // The entity is the group's carrier, set where the groups are made.
                render_entity: Entity::PLACEHOLDER,
                main_entity: MainEntity::from(Entity::PLACEHOLDER),
                index: index as u32,
                pipeline: resolved.pipeline,
                far: resolved.far,
                material_group: resolved.material_group,
                alpha_mask,
                bindless: resolved.bindless,
            });
        }
        let ready_batches: Vec<ReadyBatch> = by_index.iter().flatten().copied().collect();
        let mut terrain_walk = [0u32; 7];
        for stage in terrain_stage.iter().flatten() {
            terrain_walk[0] += 1;
            if *stage > 0 {
                terrain_walk[*stage as usize] += 1;
            }
        }
        let carriers_changed = gpu.views[&key].walk_carriers != *carriers;
        // The walk's own resolutions move the generation; the stamp it leaves is the state after it.
        let mut walked = stamp;
        walked[7] = gpu.generation;
        if let Some(view_gpu) = gpu.views.get_mut(&key) {
            view_gpu.walk_stamp = Some(walked);
            view_gpu.walk_base = Some(walk_base(walked));
            view_gpu.walked_version = batch_materials.version;
            view_gpu.walk_ready = by_index;
            view_gpu.walk_terrain = terrain_stage;
            view_gpu.walk_incomplete = incomplete;
            view_gpu.walk_carriers.clone_from(carriers);
        }
        // A fingerprint of the plan: which batches draw, with what, and under which carriers.
        let plan_hash = ready_batches.iter().fold(
            0xcbf2_9ce4_8422_2325u64
                ^ gpu.layout_epoch.rotate_left(17)
                ^ batch_materials.version.rotate_left(29)
                ^ sources.layout_version.rotate_left(41),
            |hash, ready| {
                [
                    u64::from(ready.index),
                    ready.pipeline.id() as u64,
                    u64::from(ready.material_group),
                    u64::from(ready.alpha_mask),
                ]
                .into_iter()
                .fold(hash, |hash, word| {
                    (hash ^ word).wrapping_mul(0x0100_0000_01b3).rotate_left(23)
                })
            },
        );
        // The bins persist between frames: when this frame's plan is the one already queued
        // (and the phases still hold it), nothing needs to change.
        let queued = !carriers_changed
            && gpu.views.get(&key).is_some_and(|view_gpu| {
                view_gpu.plan_hash == Some(plan_hash)
                    && (!view_gpu.plan_has_opaque || !phase.non_mesh_items.is_empty())
                    && (!view_gpu.plan_has_alpha_mask
                        || alpha_mask_phase
                            .as_deref()
                            .is_some_and(|phase| !phase.non_mesh_items.is_empty()))
            });
        if queued {
            queue_stats::bump(&queue_stats::BATCH_SKIPPED);
            continue;
        }
        queue_stats::bump(&queue_stats::BATCH_FULL);
        // Batches that bind the same pipeline and material bind group draw as one group.
        let mut groups = DrawGroups::new(gpu.batches.len());
        let mut group_of_key: HashMap<(CachedRenderPipelineId, u32, bool), u32> = HashMap::new();
        for ready in &ready_batches {
            let group_key = (ready.pipeline, ready.material_group, ready.alpha_mask);
            let slots = gpu.batches[ready.index as usize].slots;
            let group = groups.add(ready.index, slots, group_of_key.get(&group_key).copied());
            group_of_key.insert(group_key, group);
        }
        let is_terrain_batch = |batch: u32| {
            gpu.batches
                .get(batch as usize)
                .and_then(|b| b.material)
                .is_some_and(|asset| asset.type_id() == terrain_type)
        };
        terrain_walk[5] = groups
            .groups
            .iter()
            .filter(|(batch, _)| is_terrain_batch(*batch))
            .count() as u32;
        // Each group is queued for the carrier numbered like it.
        let representatives: Vec<ReadyBatch> = groups
            .groups
            .iter()
            .enumerate()
            .filter_map(|(group, (batch, _))| {
                let mut ready = *ready_batches.iter().find(|ready| ready.index == *batch)?;
                let Some((render_entity, main_entity)) = carriers.get(group).copied().flatten()
                else {
                    carrier_shortage(group);
                    return None;
                };
                ready.render_entity = render_entity;
                ready.main_entity = main_entity;
                Some(ready)
            })
            .collect();
        terrain_walk[6] = representatives
            .iter()
            .filter(|ready| is_terrain_batch(ready.index))
            .count() as u32;
        note_terrain_walk(terrain_walk, groups.groups.len() as u32);
        let order = groups.shared_order(gpu);
        let Some(view_gpu) = gpu.views.get_mut(&key) else {
            continue;
        };
        for previous in view_gpu.plan_items.drain(..) {
            phase.remove(previous);
            if let Some(alpha_mask_phase) = alpha_mask_phase.as_deref_mut() {
                alpha_mask_phase.remove(previous);
            }
        }
        {
            let pipelines: std::collections::HashSet<_> =
                ready_batches.iter().map(|ready| ready.pipeline).collect();
            let bind_groups: std::collections::HashSet<_> = ready_batches
                .iter()
                .map(|ready| ready.material_group)
                .collect();
            queue_stats::BINDLESS_BATCHES.store(
                ready_batches.iter().filter(|ready| ready.bindless).count() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
            queue_stats::READY_BATCHES.store(
                ready_batches.len() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
            queue_stats::GROUP_PIPELINES
                .store(pipelines.len() as u64, std::sync::atomic::Ordering::Relaxed);
            queue_stats::GROUP_BIND_GROUPS.store(
                bind_groups.len() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
        }
        queue_stats::GROUPS_CAMERA.store(
            groups.groups.len() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        groups.apply(view_gpu, &queue, order);
        for draw in view_gpu.draws.values_mut() {
            draw.far = ready_batches
                .iter()
                .find(|ready| ready.index == draw.representative)
                .is_some_and(|ready| ready.far);
        }
        view_gpu.plan_hash = Some(plan_hash);
        view_gpu.plan_has_opaque = representatives.iter().any(|ready| !ready.alpha_mask);
        view_gpu.plan_has_alpha_mask = representatives.iter().any(|ready| ready.alpha_mask);
        view_gpu.plan_reps = representatives.clone();
        view_gpu.readiness_revision += 1;
        view_gpu.plan_rep_positions = representatives
            .iter()
            .enumerate()
            .map(|(position, ready)| (ready.index, position))
            .collect();
        view_gpu.plan_epoch += 1;
        for ready in representatives {
            view_gpu.plan_items.push(ready.main_entity);
            if ready.alpha_mask {
                if let Some(alpha_mask_phase) = alpha_mask_phase.as_deref_mut() {
                    alpha_mask_phase.add(
                        OpaqueNoLightmap3dBatchSetKey {
                            draw_function: alpha_mask_draw_function,
                            pipeline: ready.pipeline,
                            material_bind_group_index: Some(ready.material_group),
                            slabs: MeshSlabs::default(),
                        },
                        OpaqueNoLightmap3dBinKey {
                            asset_id: batch_asset_id(ready.index),
                        },
                        (ready.render_entity, ready.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            } else {
                phase.add(
                    Opaque3dBatchSetKey {
                        draw_function,
                        pipeline: ready.pipeline,
                        material_bind_group_index: Some(ready.material_group),
                        lightmap_slab: None,
                        slabs: MeshSlabs::default(),
                    },
                    Opaque3dBinKey {
                        asset_id: batch_asset_id(ready.index),
                    },
                    (ready.render_entity, ready.main_entity),
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
            }
        }
    }
}

/// Logs, when it changes, how far the terrain batches got through the camera plan walk: reached with a
/// material, material not prepared, mesh or pipeline missing, dropped by the phase, entered the plan,
/// grouped, kept after the carrier lookup; and the plan's group count.
fn note_terrain_walk(walk: [u32; 7], plan_groups: u32) {
    static LAST: std::sync::Mutex<Option<([u32; 7], u32)>> = std::sync::Mutex::new(None);
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *last != Some((walk, plan_groups)) {
        *last = Some((walk, plan_groups));
        info!(
            ?walk,
            plan_groups,
            fields = "terrain batches: reached with an id, material not prepared, mesh or pipeline missing, dropped by the phase, entered the plan, grouped, kept after the carrier lookup",
            "cluster terrain plan walk changed"
        );
    }
}

/// Says why a pipeline could not be built, once per distinct reason: the objects of that material
/// are not drawn (their stock copies are hidden), and the walk tries again every frame, so the
/// message must not repeat.
fn note_pipeline_failure(what: &str, error: &dyn std::fmt::Display) {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let message = format!("{what}: {error}");
    let mut seen = SEEN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !seen.contains(&message) {
        warn!("cluster path: a material's objects cannot be drawn, {message}");
        seen.push(message);
    }
}

/// What a prepared material's pipeline depends on, as one number: its key, its mesh pipeline key
/// bits, its render phase and whether it is bindless.
fn material_properties_hash(material: &PreparedMaterial) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    material.properties.material_key.hash(&mut hasher);
    let bits: MeshPipelineKey = material.properties.mesh_pipeline_key_bits.downcast();
    bits.bits().hash(&mut hasher);
    match material.properties.render_phase_type {
        RenderPhaseType::Opaque => 0u8,
        RenderPhaseType::AlphaMask => 1,
        _ => 2,
    }
    .hash(&mut hasher);
    material.properties.bindless.hash(&mut hasher);
    hasher.finish()
}

/// A group has no carrier yet: asks the main world for one (the pool grows to the demand) and says
/// so, once, when the demand is past the most the pool grows to, where the groups draw nothing.
/// (A carrier not spawned or extracted yet is retried.)
fn carrier_shortage(group: usize) {
    static REPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    CARRIER_DEMAND.fetch_max(
        u32::try_from(group + 1).unwrap_or(u32::MAX),
        std::sync::atomic::Ordering::Relaxed,
    );
    if group >= MAX_CLUSTER_CARRIERS as usize
        && !REPORTED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        error!(
            "a cluster view has more draw groups than the {MAX_CLUSTER_CARRIERS} carriers the pool grows to: the groups past them are not drawn"
        );
    }
}

/// What a batch's draw needs, resolved from its material.
#[derive(Clone, Copy)]
struct ResolvedBatch {
    far: bool,
    pipeline: CachedRenderPipelineId,
    material_group: u32,
    /// `Some(false)` opaque, `Some(true)` alpha mask, `None` a phase the cluster path skips.
    alpha_mask: Option<bool>,
    /// Whether the material is bound through a bindless slab (many materials per bind group).
    bindless: bool,
}

/// A batch's resolved draw with the inputs it was resolved from, for [`queue_cluster_batches`].
#[derive(Clone, Copy)]
struct BatchMemo {
    /// A hash of the material's pipeline-relevant properties (its key, its mesh key bits, its
    /// phase and bindless-ness): the prepared material of the same address can still have been
    /// replaced in place with other ones.
    properties: u64,
    asset_id: bevy::asset::UntypedAssetId,
    material_address: usize,
    material_slot: u32,
    material_group: u32,
    alpha_mode: AlphaMode,
    mesh_asset: AssetId<Mesh>,
    tangents: bool,
    view_key: MeshPipelineKey,
    resolved: ResolvedBatch,
}

/// A batch that is ready to draw in one view.
#[derive(Clone, Copy)]
struct ReadyBatch {
    far: bool,
    render_entity: Entity,
    main_entity: MainEntity,
    index: u32,
    pipeline: CachedRenderPipelineId,
    material_group: u32,
    alpha_mask: bool,
    bindless: bool,
}

fn build_descriptor(
    material_pipeline: &MaterialPipeline,
    key: ClusterMaterialPipelineKey,
    layout: &MeshVertexBufferLayoutRef,
    material: &PreparedMaterial,
    draw_layout: &BindGroupLayoutDescriptor,
    asset_server: &AssetServer,
    far: bool,
) -> Option<bevy::render::render_resource::RenderPipelineDescriptor> {
    let key = key.material;
    let mesh_key: MeshPipelineKey = key.mesh_key.downcast();
    let mut descriptor = material_pipeline
        .mesh_pipeline
        .specialize(mesh_key, layout)
        .map_err(|error| note_pipeline_failure("the forward pipeline", &error))
        .ok()?;
    let material_group = ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3);
    descriptor.vertex.shader_defs.push(material_group.clone());
    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader_defs.push(material_group);
        if let Some(shader) = material.properties.get_shader(MaterialFragmentShader) {
            fragment.shader = shader.clone();
        }
    }
    let Some(material_layout) = material.properties.material_layout.as_ref() else {
        note_pipeline_failure(
            "the forward pipeline",
            &"the material has no bind group layout",
        );
        return None;
    };
    descriptor.layout.insert(3, material_layout.clone());
    // The material's own specialization (cull mode, double sides, an extension's flags), run
    // as the stock specializer runs it, before the cluster data layout is added.
    if let Some(specialize) = material.properties.user_specialize {
        specialize(material_pipeline as &dyn Any, &mut descriptor, layout, key)
            .map_err(|error| note_pipeline_failure("the material's own specialization", &error))
            .ok()?;
    }
    descriptor.layout.push(draw_layout.clone());
    if material.properties.bindless {
        descriptor.vertex.shader_defs.push("BINDLESS".into());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.push("BINDLESS".into());
        }
    }
    descriptor.vertex.shader = asset_server.load(if far {
        "embedded://engine/far_terrain_shadows/cluster_draw.wgsl"
    } else {
        "embedded://engine/cluster/shaders/cluster_draw.wgsl"
    });
    descriptor.vertex.buffers.clear();
    if far {
        descriptor.fragment.as_mut()?.shader =
            asset_server.load("embedded://engine/far_terrain_shadows/terrain_material.wgsl");
        descriptor
            .vertex
            .shader_defs
            .push("CLUSTER_FAR_TERRAIN_SHADOWS".into());
        descriptor
            .fragment
            .as_mut()?
            .shader_defs
            .push("CLUSTER_FAR_TERRAIN_SHADOWS".into());
    }
    descriptor.label = Some("cluster pipeline".into());
    Some(descriptor)
}

const COMPACTION_ENTRY_POINTS: [&str; 6] = [
    STAGES[1].0,
    STAGES[2].0,
    STAGES[4].0,
    STAGES[5].0,
    STAGES[6].0,
    STAGES[7].0,
];

struct PreparedClusterCull<'a> {
    instance: &'a bevy::render::render_resource::ComputePipeline,
    clusters: &'a bevy::render::render_resource::ComputePipeline,
    compaction: [&'a bevy::render::render_resource::ComputePipeline; 6],
    /// Some iff this frame requires rebasing. Missing required pipeline refuses
    /// the entire pass; optional/unneeded shift compilation is irrelevant.
    shift: Option<&'a bevy::render::render_resource::ComputePipeline>,
}

fn prepared_cluster_cull<'a>(
    gpu: &ClusterGpu,
    frame: &ClusterFrame,
    pipelines: &ClusterPipelines,
    cache: &'a PipelineCache,
) -> Option<PreparedClusterCull<'a>> {
    perf_span!("cluster::cull::resolve_pipelines");
    let instance = cache.get_compute_pipeline(pipelines.instance_cull?)?;
    let clusters = cache.get_compute_pipeline(pipelines.cull?)?;
    let compaction = [
        cache.get_compute_pipeline(pipelines.compaction[0]?)?,
        cache.get_compute_pipeline(pipelines.compaction[1]?)?,
        cache.get_compute_pipeline(pipelines.compaction[2]?)?,
        cache.get_compute_pipeline(pipelines.compaction[3]?)?,
        cache.get_compute_pipeline(pipelines.compaction[4]?)?,
        cache.get_compute_pipeline(pipelines.compaction[5]?)?,
    ];
    let shift = if gpu.applied_offset != frame.origin_offset {
        Some(cache.get_compute_pipeline(pipelines.shift?)?)
    } else {
        None
    };
    Some(PreparedClusterCull {
        instance,
        clusters,
        compaction,
        shift,
    })
}

/// Exactly matches the current cull encoder's active-view predicate. Existing
/// per-view owned buffers/bind groups were constructed before the queue stage;
/// this function does not certify output visibility or GPU execution success.
fn cluster_cull_view_eligible(
    view: &ViewGpu,
    opaque_phase_present: bool,
    shadow_phase_present: bool,
) -> bool {
    (opaque_phase_present || shadow_phase_present) && !view.draws.is_empty()
}

/// The cull passes of every view that draws clusters: the 3D camera and each shadow cascade or
/// cube face. Each view owns its parameters, compact instance list, source selection scratch,
/// prefixes, argument list and draw counts. Eight stages cover all batches, preserving source
/// order at both compactions without a readback or inter-workgroup spin waiting.
#[allow(clippy::too_many_arguments)]
fn run_cluster_cull(
    mut ctx: RenderContext,
    gpu: Option<ResMut<ClusterGpu>>,
    frame: Res<ClusterFrame>,
    pipelines: Res<ClusterPipelines>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    views: Query<&ExtractedView>,
    opaque_phases: Res<ViewBinnedRenderPhases<Opaque3d>>,
    shadow_phases: Res<ViewBinnedRenderPhases<Shadow>>,
    settings: Res<ClusterSettings>,
) {
    perf_span!("cluster::cull::total");
    let _timer = queue_stats::Timed::start(&queue_stats::CULL_NANOS);
    let Some(mut gpu) = gpu else {
        return;
    };
    // Invalidate previous arguments even if a dependency vanished this frame.
    // All stock handoff decisions must also retain their dependency proof.
    gpu.epoch = gpu.epoch.wrapping_add(1).max(1);
    let epoch = gpu.epoch;
    let Some(prepared) = prepared_cluster_cull(&gpu, &frame, &pipelines, &pipeline_cache) else {
        return;
    };
    let instance_pipeline = prepared.instance;
    let pipeline = prepared.clusters;
    // Every view that exists is stamped; the ones that have not been seen for a while are freed
    // (their buffers are as big as the scene's slot count, so a leaked set is not small).
    let now = std::time::Instant::now();
    for view in &views {
        if let Some(view_gpu) = gpu.views.get_mut(&view.retained_view_entity) {
            view_gpu.last_seen = now;
        }
    }
    gpu.views
        .retain(|_, view_gpu| now.duration_since(view_gpu.last_seen) < VIEW_KEEP);
    // When the render origin moved, the instances are placed again before they are culled.
    let mut shifted = false;
    if let Some(shift_pipeline) = prepared.shift {
        perf_span!("cluster::cull::shift");
        let mut bytes = Vec::new();
        push_f32(
            &mut bytes,
            &[
                frame.origin_offset.x,
                frame.origin_offset.y,
                frame.origin_offset.z,
                0.0,
            ],
        );
        push_u32(&mut bytes, &[gpu.total_instances, 0, 0, 0]);
        queue.write_buffer(&gpu.shift_params, 0, &bytes);
        let bind_group = device.create_bind_group(
            "cluster shift bind group",
            &pipeline_cache.get_bind_group_layout(&gpu.shift_layout),
            &[
                entry(0, &gpu.shift_params),
                entry(1, &gpu.absolute),
                entry(2, &gpu.instances),
                entry(3, &gpu.mesh_uniforms),
            ],
        );
        let mut pass = ctx
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("cluster shift"),
                timestamp_writes: None,
            });
        pass.set_pipeline(shift_pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        let [x, y, z] = dispatch_dims(gpu.total_instances.div_ceil(64));
        pass.dispatch_workgroups(x, y, z);
        drop(pass);
        shifted = true;
    }
    if shifted {
        gpu.applied_offset = frame.origin_offset;
    }
    #[cfg(feature = "tracy")]
    let write_span = bevy::log::info_span!("cluster::cull::params_and_clears").entered();
    let write_timer = queue_stats::Timed::start(&queue_stats::CULL_WRITE_NANOS);
    let instance_count = gpu.total_instances;
    let instance_mask_base = gpu.batches.len().max(1) as u32;
    let use_count = gpu.use_count;
    // The views that draw opaque 3D or cast shadows: the UI camera's view has neither phase.
    for view in views.iter().filter(|view| {
        opaque_phases.contains_key(&view.retained_view_entity)
            || shadow_phases.contains_key(&view.retained_view_entity)
    }) {
        // A view with nothing to draw (no batch has a material yet) has no groups.
        let Some(view_gpu) = gpu
            .views
            .get_mut(&view.retained_view_entity)
            .filter(|view_gpu| {
                cluster_cull_view_eligible(
                    view_gpu,
                    opaque_phases.contains_key(&view.retained_view_entity),
                    shadow_phases.contains_key(&view.retained_view_entity),
                )
            })
        else {
            continue;
        };
        let params = cull_params(view, &settings, instance_count, instance_mask_base, epoch);
        if let Some(bytes) = cull_params_to_upload(&mut view_gpu.params_uploaded, params) {
            queue.write_buffer(&view_gpu.params, 0, bytes);
            queue_stats::bump(&queue_stats::CULL_PARAMS_UPLOADS);
        }
        // The arguments and the counts start empty every frame.
        let encoder = ctx.command_encoder();
        // The draw reads only as many arguments as the count says where the adapter can read it
        // on the GPU, and every one of those is written by the cull; only the fixed-count
        // fallback draws the whole list and needs the unused tail to be empty draws.
        if !use_count {
            encoder.clear_buffer(&view_gpu.args, 0, None);
        }
        encoder.clear_buffer(&view_gpu.counts, 0, None);
    }
    let active: Vec<&ViewGpu> = views
        .iter()
        .filter_map(|view| {
            let key = view.retained_view_entity;
            gpu.views.get(&key).filter(|view_gpu| {
                cluster_cull_view_eligible(
                    view_gpu,
                    opaque_phases.contains_key(&key),
                    shadow_phases.contains_key(&key),
                )
            })
        })
        .collect();
    drop(write_timer);
    #[cfg(feature = "tracy")]
    drop(write_span);
    // One pass per stage covers every view. Stage boundaries make preceding writes visible
    // before the next scan/scatter; view buffers never alias each other.
    let pass_timer = queue_stats::Timed::start(&queue_stats::CULL_PASS_NANOS);
    if !active.is_empty() {
        // Each stage is timed in benchmark gpu_passes and Tracy GPU zones. The new prefix and
        // scatter costs remain separately attributable from visibility/LOD selection.
        let diagnostics = ctx.diagnostic_recorder();
        let diagnostics = diagnostics.as_deref();
        let encoder = ctx.command_encoder();
        for (stage, name) in [
            "cluster_instance_cull",
            "cluster_instance_prefix",
            "cluster_instance_scatter",
            "cluster_cull",
            "cluster_draw_tile_prefix",
            "cluster_draw_group_prefix",
            "cluster_draw_scatter",
            "cluster_draw_large_scatter",
        ]
        .into_iter()
        .enumerate()
        {
            #[cfg(feature = "tracy")]
            let _stage_span = match stage {
                0 => bevy::log::info_span!("cluster::cull::instance_cull", views = active.len()),
                1 => bevy::log::info_span!("cluster::cull::instance_prefix", views = active.len()),
                2 => bevy::log::info_span!("cluster::cull::instance_scatter", views = active.len()),
                3 => bevy::log::info_span!("cluster::cull::clusters", views = active.len()),
                4 => bevy::log::info_span!("cluster::cull::tile_prefix", views = active.len()),
                5 => bevy::log::info_span!("cluster::cull::group_prefix", views = active.len()),
                6 => bevy::log::info_span!("cluster::cull::scatter", views = active.len()),
                7 => bevy::log::info_span!("cluster::cull::large_scatter", views = active.len()),
                _ => unreachable!(),
            }
            .entered();
            let span = diagnostics.time_span(encoder, name);
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some(name),
                timestamp_writes: None,
            });
            pass.set_pipeline(match stage {
                0 => instance_pipeline,
                3 => pipeline,
                1 => prepared.compaction[0],
                2 => prepared.compaction[1],
                4 => prepared.compaction[2],
                5 => prepared.compaction[3],
                6 => prepared.compaction[4],
                7 => prepared.compaction[5],
                _ => unreachable!(),
            });
            for view_gpu in &active {
                let bind_group = match stage {
                    0 => &view_gpu.instance_bind_group,
                    3 => &view_gpu.cull_bind_group,
                    1 => &view_gpu.compaction_bind_groups[0],
                    2 => &view_gpu.compaction_bind_groups[1],
                    4 => &view_gpu.compaction_bind_groups[2],
                    5 => &view_gpu.compaction_bind_groups[3],
                    6 => &view_gpu.compaction_bind_groups[4],
                    7 => &view_gpu.compaction_bind_groups[5],
                    _ => unreachable!(),
                };
                pass.set_bind_group(0, bind_group, &[]);
                if stage == 3 {
                    pass.dispatch_workgroups_indirect(&view_gpu.dispatch, 0);
                } else {
                    let workgroups = match stage {
                        0 | 2 | 6 => gpu.total_instances.div_ceil(64),
                        1 => 1,
                        4 => view_gpu.compaction_tiles,
                        5 => view_gpu.draws.len() as u32,
                        7 => view_gpu.large_scatter_count,
                        _ => unreachable!(),
                    };
                    let [x, y, z] = dispatch_dims(workgroups);
                    pass.dispatch_workgroups(x, y, z);
                }
            }
            drop(pass);
            span.end(encoder);
        }
    }
    drop(pass_timer);
    drop(active);
    // Only views included in the just-encoded cull may consume its arguments.
    for view in &views {
        if let Some(view_gpu) = gpu.views.get_mut(&view.retained_view_entity)
            && cluster_cull_view_eligible(
                view_gpu,
                opaque_phases.contains_key(&view.retained_view_entity),
                shadow_phases.contains_key(&view.retained_view_entity),
            )
        {
            view_gpu.last_cull_epoch = epoch;
        }
    }
}

/// Workgroups in one row of a dispatch: the limit of one dimension.
const DISPATCH_ROW: u32 = 65_535;

/// The dispatch dimensions that cover `groups` workgroups: one row up to the limit of a dimension,
/// then more rows. The shaders number a thread `y * DISPATCH_ROW * 64 + x`.
fn dispatch_dims(groups: u32) -> [u32; 3] {
    [
        groups.min(DISPATCH_ROW),
        groups.div_ceil(DISPATCH_ROW).max(1),
        1,
    ]
}

fn new_view_gpu(
    device: &RenderDevice,
    pipeline_cache: &PipelineCache,
    gpu: &ClusterGpu,
) -> ViewGpu {
    perf_span!("cluster::view::allocate");
    let make = |label: &str, size: u64, usage: BufferUsages| {
        device.create_buffer(&bevy::render::render_resource::BufferDescriptor {
            label: Some(label),
            size,
            usage,
            mapped_at_creation: false,
        })
    };
    let slots = u64::from(gpu.total_slots.max(1));
    let args = make(
        "cluster draw arguments",
        slots * DRAW_INDEXED_INDIRECT_BYTES,
        BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
    );
    let counts = make(
        "cluster draw counts",
        gpu.batches.len().max(1) as u64 * 4,
        BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
    );
    let batch_table = make(
        "cluster batch table",
        (gpu.batches.len().max(1) as u64 + u64::from(gpu.capacity_instances.div_ceil(128))) * 16,
        BufferUsages::STORAGE | BufferUsages::COPY_DST,
    );
    let params = make(
        "cluster cull params",
        CULL_PARAMS_BYTES as u64,
        BufferUsages::UNIFORM | BufferUsages::COPY_DST,
    );
    let visible = make(
        "cluster visible list and terrain records",
        (slots + gpu.terrain_blocks.len() as u64 * u64::from(super::terrain::TERRAIN_VIEW_VEC4S))
            * 16,
        BufferUsages::STORAGE | BufferUsages::COPY_DST,
    );
    let instance_list = make(
        "cluster instance list",
        u64::from(gpu.capacity_instances.max(1)) * 16,
        BufferUsages::STORAGE,
    );
    let dispatch = make(
        "cluster cull dispatch",
        16,
        BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
    );
    let prefixes = make(
        "cluster compaction prefixes",
        stable_order::prefix_capacity(gpu.capacity_instances, gpu.batches.len()),
        BufferUsages::STORAGE,
    );
    let selected_clusters = make(
        "cluster selected source slots",
        slots * SELECTED_CLUSTER_BYTES,
        BufferUsages::STORAGE,
    );
    let source_order = make(
        "cluster source order",
        stable_order::order_capacity(gpu.capacity_instances, gpu.batches.len()),
        BufferUsages::STORAGE | BufferUsages::COPY_DST,
    );
    let buffers = [
        &params,
        &gpu.clusters,
        &gpu.groups,
        &gpu.instances,
        &args,
        &counts,
        &gpu.meshes,
        &visible,
        &instance_list,
        &dispatch,
        &batch_table,
        &prefixes,
        &selected_clusters,
        &source_order,
    ];
    let bind = |label: &str, layout: &BindGroupLayoutDescriptor, indices: &[u32]| {
        device.create_bind_group(
            label,
            &pipeline_cache.get_bind_group_layout(layout),
            &indices
                .iter()
                .map(|&index| entry(index, buffers[index as usize]))
                .collect::<Vec<_>>(),
        )
    };
    let instance_bind_group = bind(
        "cluster instance cull bind group",
        &gpu.instance_cull_layout,
        STAGES[0].1,
    );
    let cull_bind_group = bind("cluster cull bind group", &gpu.cull_layout, STAGES[3].1);
    let compaction_bind_groups = [
        bind(
            "cluster instance prefix bind group",
            &gpu.compaction_layouts[0],
            STAGES[1].1,
        ),
        bind(
            "cluster instance scatter bind group",
            &gpu.compaction_layouts[1],
            STAGES[2].1,
        ),
        bind(
            "cluster draw tile prefix bind group",
            &gpu.compaction_layouts[2],
            STAGES[4].1,
        ),
        bind(
            "cluster draw group prefix bind group",
            &gpu.compaction_layouts[3],
            STAGES[5].1,
        ),
        bind(
            "cluster draw scatter bind group",
            &gpu.compaction_layouts[4],
            STAGES[6].1,
        ),
        bind(
            "cluster large scatter bind group",
            &gpu.compaction_layouts[5],
            STAGES[7].1,
        ),
    ];
    let draw_bind_group = device.create_bind_group(
        "cluster draw bind group",
        &pipeline_cache.get_bind_group_layout(&gpu.draw_layout),
        &[
            entry(0, &gpu.positions),
            entry(1, &gpu.attributes),
            entry(2, &gpu.clusters),
            entry(3, &gpu.cluster_vertices),
            entry(5, &gpu.meshes),
            entry(6, &visible),
        ],
    );
    let shadow_bind_group = device.create_bind_group(
        "cluster shadow bind group",
        &pipeline_cache.get_bind_group_layout(&gpu.shadow_layout),
        &[
            entry(0, &gpu.positions),
            entry(1, &gpu.clusters),
            entry(2, &gpu.cluster_vertices),
            entry(4, &gpu.instances),
            entry(5, &gpu.meshes),
            entry(6, &visible),
        ],
    );
    ViewGpu {
        last_seen: std::time::Instant::now(),
        last_cull_epoch: 0,
        selection: None,
        selection_base: None,
        instance_mask_bytes: Vec::new(),
        terrain_clip_bytes: Vec::new(),
        terrain_clip_inputs: None,
        far: None,
        prepass_pipelines: HashMap::new(),
        shadow_pipelines: HashMap::new(),
        args,
        counts,
        params,
        params_uploaded: None,
        visible,
        instance_list,
        dispatch,
        source_order,
        order: None,
        compaction_tiles: 0,
        large_scatter_count: 0,
        compaction_bind_groups,
        table: vec![0; gpu.batches.len().max(1) * 16],
        batch_table,
        draws: HashMap::new(),
        memo: vec![None; gpu.batches.len()],
        plan_hash: None,
        walk_stamp: None,
        walk_base: None,
        walked_version: 0,
        walk_ready: Vec::new(),
        walk_terrain: Vec::new(),
        walk_incomplete: std::collections::BTreeSet::new(),
        prepass_drawn: std::sync::Mutex::new(HashMap::new()),
        walk_carriers: Vec::new(),
        plan_items: Vec::new(),
        plan_reps: Vec::new(),
        plan_rep_positions: HashMap::new(),
        plan_epoch: 0,
        readiness_revision: 0,
        readiness_cache: None,
        prepass_epoch: None,
        prepass_items: Vec::new(),
        prepass_groups: std::collections::HashSet::new(),
        prepass_has_opaque: false,
        prepass_has_alpha_mask: false,
        plan_has_opaque: false,
        plan_has_alpha_mask: false,
        instance_bind_group,
        cull_bind_group,
        draw_bind_group,
        shadow_bind_group,
    }
}

fn cull_params_to_upload(
    uploaded: &mut Option<CullParams>,
    mut params: CullParams,
) -> Option<&[u8]> {
    // The shader keeps epoch in the ABI but never reads it. CPU cull and draw freshness
    // epochs stay live; normalize only the uploaded bytes.
    params.epoch = 0;
    if uploaded
        .as_ref()
        .is_some_and(|previous| bytemuck::bytes_of(previous) == bytemuck::bytes_of(&params))
    {
        return None;
    }
    Some(bytemuck::bytes_of(uploaded.insert(params)))
}

/// The uniform block of the cull pass for `view`: frustum planes, camera, error scale, threshold,
/// and how many instances pass 1 covers.
fn cull_params(
    view: &ExtractedView,
    settings: &ClusterSettings,
    instance_count: u32,
    instance_mask_base: u32,
    epoch: u32,
) -> CullParams {
    let clip_from_world = view
        .clip_from_world
        .unwrap_or_else(|| view.clip_from_view * view.world_from_view.to_matrix().inverse());
    let row = |i: usize| {
        let cols = clip_from_world.to_cols_array_2d();
        Vec4::new(cols[0][i], cols[1][i], cols[2][i], cols[3][i])
    };
    let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
    // An orthographic projection has no perspective divide: its fourth column ends in one.
    let orthographic = view.clip_from_view.w_axis.w == 1.0;
    // Left, right, bottom, top, near (reverse-Z: clip.z <= clip.w). An orthographic view keeps
    // only the four sides: its depth range is the light's, and the cascade's own bounds cull.
    let near_plane = if orthographic {
        Vec4::new(0.0, 0.0, 0.0, 1.0e30)
    } else {
        r3 - r2
    };
    let planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, near_plane];
    let planes = planes.map(|plane| {
        let length = plane.truncate().length().max(1.0e-12);
        (plane / length).to_array()
    });
    let viewport_height = view.viewport.w.max(1) as f32;
    // Perspective: pixels per unit of error at distance one, half the viewport height over
    // tan(fov / 2). Orthographic: pixels per unit of error, half the viewport height over the
    // half-height of the view volume.
    let projection_scale = view.clip_from_view.y_axis.y * viewport_height * 0.5;
    // The reverse-Z infinite projection keeps the near distance in its fourth column.
    let near = view.clip_from_view.w_axis.z.abs().max(1.0e-3);
    let camera = view.world_from_view.translation();
    CullParams {
        planes,
        camera: [camera.x, camera.y, camera.z, projection_scale],
        error_pixels: settings.error_pixels,
        near,
        min_instance_pixels: settings.min_instance_pixels,
        instance_count,
        orthographic: u32::from(orthographic),
        terrain_error_pixels: settings.terrain_error_pixels,
        instance_mask_base,
        epoch,
    }
}

type DrawClusterBatch = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshViewBindingArrayBindGroup<1>,
    SetClusterMeshBindGroup<2>,
    SetClusterMaterialBindGroup<3>,
    far::SetForwardData<4>,
    DrawClusterIndirect<true>,
);

/// The stock `SetMaterialBindGroup`, but resolving the material through the source entity.
struct SetClusterMaterialBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetClusterMaterialBindGroup<I> {
    type Param = (
        SRes<ClusterGpu>,
        SRes<ErasedRenderAssets<PreparedMaterial>>,
        SRes<bevy::pbr::MaterialBindGroupAllocators>,
    );
    type ViewQuery = &'static ExtractedView;
    type ItemQuery = &'static ClusterBatch;

    fn render<'w>(
        _: &P,
        view: ROQueryItem<'w, '_, Self::ViewQuery>,
        batch: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        (gpu, materials, allocators): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(batch) = batch else {
            return RenderCommandResult::Skip;
        };
        let gpu = gpu.into_inner();
        // The batch whose material this group's draw binds: the group's representative.
        let Some(asset_id) = view_gpu(gpu, view)
            .and_then(|view_gpu| view_gpu.draws.get(&batch.index))
            .and_then(|draw| gpu.batches.get(draw.representative as usize))
            .and_then(|batch| batch.material)
        else {
            return RenderCommandResult::Skip;
        };
        let Some(allocator) = allocators.into_inner().get(&asset_id.type_id()) else {
            return RenderCommandResult::Skip;
        };
        let Some(material) = materials.into_inner().get(asset_id) else {
            return RenderCommandResult::Skip;
        };
        let Some(bind_group) = allocator
            .get(material.binding.group)
            .and_then(|group| group.bind_group())
        else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(I, bind_group, &[]);
        RenderCommandResult::Success
    }
}

struct SetClusterMeshBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetClusterMeshBindGroup<I> {
    type Param = SRes<ClusterGpu>;
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        _: &P,
        _: ROQueryItem<'w, '_, Self::ViewQuery>,
        _: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        gpu: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let gpu = gpu.into_inner();
        let Some(bind_group) = gpu.mesh_bind_group.as_ref() else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(I, bind_group, &[]);
        RenderCommandResult::Success
    }
}

struct SetClusterDataBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetClusterDataBindGroup<I> {
    type Param = SRes<ClusterGpu>;
    type ViewQuery = &'static ExtractedView;
    type ItemQuery = &'static ClusterBatch;

    fn render<'w>(
        _: &P,
        view: ROQueryItem<'w, '_, Self::ViewQuery>,
        batch: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        gpu: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let gpu = gpu.into_inner();
        let _ = batch;
        let Some(view_gpu) = view_gpu(gpu, view) else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(I, &view_gpu.draw_bind_group, &[]);
        RenderCommandResult::Success
    }
}

/// This view's buffers.
fn view_gpu<'a>(gpu: &'a ClusterGpu, view: &ExtractedView) -> Option<&'a ViewGpu> {
    gpu.views.get(&view.retained_view_entity)
}

/// `ONCE`: draw a group at most once per cull epoch in this view. With occlusion culling Bevy
/// renders the camera prepass phases twice, early and late; its own meshes rewrite their indirect
/// counts in between so each is drawn once, but a cluster group's arguments are the same in both
/// passes and would draw all of its geometry twice. It draws in the early pass, whose depth also
/// feeds the occlusion culling of the stock meshes.
struct DrawClusterIndirect<const FORWARD: bool = false, const ONCE: bool = false>;

impl<P: PhaseItem, const FORWARD: bool, const ONCE: bool> RenderCommand<P>
    for DrawClusterIndirect<FORWARD, ONCE>
{
    type Param = SRes<ClusterGpu>;
    type ViewQuery = &'static ExtractedView;
    type ItemQuery = &'static ClusterBatch;

    fn render<'w>(
        _: &P,
        view: ROQueryItem<'w, '_, Self::ViewQuery>,
        batch: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        gpu: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let gpu = gpu.into_inner();
        let Some(batch) = batch else {
            return RenderCommandResult::Skip;
        };
        let Some(view_gpu) = view_gpu(gpu, view) else {
            return RenderCommandResult::Skip;
        };
        if gpu.epoch == 0 || view_gpu.last_cull_epoch != gpu.epoch {
            return RenderCommandResult::Skip;
        }
        // The draw group this batch is the representative of: its slice of the argument list and
        // its own draw count.
        let Some(draw) = view_gpu.draws.get(&batch.index) else {
            return RenderCommandResult::Skip;
        };
        if ONCE
            && view_gpu
                .prepass_drawn
                .lock()
                .expect("prepass draw record")
                .insert(batch.index, gpu.epoch)
                == Some(gpu.epoch)
        {
            return RenderCommandResult::Success;
        }
        pass.set_index_buffer(gpu.cluster_indices.slice(..), IndexFormat::Uint16);
        if gpu.use_count {
            pass.multi_draw_indexed_indirect_count(
                &view_gpu.args,
                draw.args_offset,
                &view_gpu.counts,
                draw.count_offset,
                draw.max_count,
            );
        } else {
            // No count on the GPU: draw the whole slice; the cull pass left the unused entries as
            // zero indices, so they cost a skipped draw each and nothing else.
            pass.multi_draw_indexed_indirect(&view_gpu.args, draw.args_offset, draw.max_count);
        }
        if FORWARD && draw.far {
            far::note_encoded_draw(gpu, view_gpu);
        }
        RenderCommandResult::Success
    }
}

/// The camera prepass draw: the same groups as the shadow draw of an alpha-masked batch (the
/// view's prepass bind group, the mesh uniforms, the material, then the cluster data).
type DrawClusterPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
    SetClusterMeshBindGroup<2>,
    SetClusterMaterialBindGroup<3>,
    SetClusterDataBindGroup<4>,
    DrawClusterIndirect<false, true>,
);

/// Stock opaque depth-only specialization has an empty material layout at
/// group 3. Bind the stock empty group there, while retaining mesh uniforms at
/// group 2 and cluster data at group 4. Material-backed discard passes keep the
/// existing draw tuple, including alpha-masked depth-only terrain.
type DrawClusterDepthOnlyPrepass = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
    SetClusterMeshBindGroup<2>,
    SetPrepassViewEmptyBindGroup<3>,
    SetClusterDataBindGroup<4>,
    DrawClusterIndirect<false, true>,
);

/// Inspect the descriptor after material specialization; do not infer layout
/// from camera flags alone (MAY_DISCARD/material extensions can change it).
fn prepass_empty_material_layout(descriptor: &RenderPipelineDescriptor) -> Option<bool> {
    (descriptor.layout.len() == 4).then(|| descriptor.layout[3].entries.is_empty())
}

/// The pipeline a camera's prepass draws a batch's material with: the material's own prepass
/// pipeline (its fragment stage writes the normal and the motion vector, and discards by alpha)
/// behind the cluster vertex stage, which gives it the same `VertexOutput` a stock mesh does.
fn build_prepass_descriptor(
    prepass: &PrepassPipeline,
    key: ClusterMaterialPipelineKey,
    layout: &MeshVertexBufferLayoutRef,
    material: &PreparedMaterial,
    draw_layout: &BindGroupLayoutDescriptor,
    asset_server: &AssetServer,
) -> Option<(RenderPipelineDescriptor, bool)> {
    let specializer = PrepassPipelineSpecializer {
        pipeline: prepass.clone(),
        properties: material.properties.clone(),
    };
    let mut descriptor = specializer
        .specialize(key.material, layout)
        .map_err(|error| note_pipeline_failure("the prepass pipeline", &error))
        .ok()?;
    // Stock layouts are [view, empty, mesh, material-or-empty]. The empty
    // group-3 slot already exists in Bevy 0.19; appending data keeps group 4.
    // Refuse unexpected custom layouts instead of moving existing bindings.
    let Some(empty_material) = prepass_empty_material_layout(&descriptor) else {
        warn!("cluster prepass has unsupported bind-group layout; retaining stock");
        return None;
    };
    descriptor.layout.push(draw_layout.clone());
    descriptor.vertex.shader =
        asset_server.load("embedded://engine/cluster/shaders/cluster_prepass.wgsl");
    descriptor.vertex.buffers.clear();
    descriptor.label = Some("cluster prepass pipeline".into());
    Some((descriptor, empty_material))
}

/// Adds the cluster batches to every camera's prepass phases, so what the cluster path draws is
/// in the depth, normal and motion vector textures the stock effects read (ambient occlusion,
/// temporal anti-aliasing and DLSS, water reflections). It draws the same selected clusters, in
/// the same draw groups, as the camera's main draw: only the pipeline differs.
///
/// Depth-only views retain the stock material-or-empty group-3 semantics; the
/// cluster vertex shader always reads its pooled data from group 4.
#[allow(clippy::too_many_arguments)]
fn queue_cluster_prepass(
    pipeline_cache: Res<PipelineCache>,
    prepass: Res<PrepassPipeline>,
    asset_server: Res<AssetServer>,
    mut pipelines: ResMut<ClusterPipelines>,
    mut gpu: Option<ResMut<ClusterGpu>>,
    mut opaque_phases: ResMut<ViewBinnedRenderPhases<Opaque3dPrepass>>,
    mut alpha_mask_phases: ResMut<ViewBinnedRenderPhases<AlphaMask3dPrepass>>,
    opaque_draws: Res<DrawFunctions<Opaque3dPrepass>>,
    alpha_mask_draws: Res<DrawFunctions<AlphaMask3dPrepass>>,
    views: Query<&ExtractedView>,
    view_key_cache: Res<ViewKeyCache>,
    stock: StockLookups,
    dummy_mesh: Option<Res<ClusterDummyMesh>>,
    (settings, batch_materials): (Res<ClusterSettings>, Res<ClusterBatchMaterials>),
) {
    perf_span!("cluster::queue_prepass::total");
    if !settings.prepass {
        return;
    }
    pipelines.refresh_prepass_refusals(batch_materials.version);
    let Some(gpu) = gpu.as_deref_mut() else {
        return;
    };
    let StockLookups {
        sources,
        materials,
        render_meshes,
        ..
    } = stock;
    let Some(dummy_mesh) = dummy_mesh else {
        return;
    };
    let opaque_draw = opaque_draws.read().id::<DrawClusterPrepass>();
    let alpha_mask_draw = alpha_mask_draws.read().id::<DrawClusterPrepass>();
    let opaque_empty_draw = opaque_draws.read().id::<DrawClusterDepthOnlyPrepass>();
    let alpha_mask_empty_draw = alpha_mask_draws.read().id::<DrawClusterDepthOnlyPrepass>();
    for view in &views {
        let key = view.retained_view_entity;
        let Some(opaque_phase) = opaque_phases.get_mut(&key) else {
            continue;
        };
        let mut alpha_mask_phase = alpha_mask_phases.get_mut(&key);
        let Some(view_key) = view_key_cache.get(&key).copied() else {
            continue;
        };
        let Some(view_gpu) = gpu.views.get(&key) else {
            continue;
        };
        let epoch = view_gpu.plan_epoch;
        // The bins persist between frames: a plan already queued, with its phases still holding
        // it, needs nothing.
        if view_gpu.prepass_epoch == Some(epoch)
            && (!view_gpu.prepass_has_opaque || !opaque_phase.non_mesh_items.is_empty())
            && (!view_gpu.prepass_has_alpha_mask
                || alpha_mask_phase
                    .as_deref()
                    .is_some_and(|phase| !phase.non_mesh_items.is_empty()))
        {
            continue;
        }
        perf_span!("cluster::queue_prepass::rebuild");
        let representatives = view_gpu.plan_reps.clone();
        let mut complete = true;
        let mut queued: Vec<(ReadyBatch, CachedClusterPrepass)> = Vec::new();
        for ready in &representatives {
            // `ready.index` is the batch that issues its group's draw (a source batch index, not
            // the group's number: `view_gpu.draws` is keyed by group). Its material stands for the
            // group, whose members share one pipeline and material bind group.
            let Some(asset_id) = gpu.batches[ready.index as usize].material else {
                complete = false;
                continue;
            };
            let Some(material) = materials.get(asset_id) else {
                complete = false;
                continue;
            };
            let Some(tangents) = gpu.batches[ready.index as usize].tangents.filter(|layout| {
                asset_id.type_id()
                    == std::any::TypeId::of::<super::terrain::ClusterTerrainMaterial>()
                    || sources.stock_tangents(ready.index as usize) == Some(*layout)
            }) else {
                complete = false;
                continue;
            };
            let Some(render_mesh) = render_meshes.get(dummy_mesh.asset(tangents)) else {
                complete = false;
                continue;
            };
            if !material.properties.prepass_enabled {
                continue;
            }
            let mut material_bits: MeshPipelineKey =
                material.properties.mesh_pipeline_key_bits.downcast();
            material_bits.insert(alpha_mode_pipeline_key(
                material.properties.alpha_mode,
                &Msaa::from_samples(view_key.msaa_samples()),
            ));
            let mesh_key = view_key
                | MeshPipelineKey::from_bits_retain(render_mesh.key_bits.bits())
                | material_bits;
            let pipeline_key = ClusterMaterialPipelineKey {
                material: ErasedMaterialPipelineKey {
                    mesh_key: ErasedMeshPipelineKey::new(mesh_key),
                    material_key: material.properties.material_key.clone(),
                    type_id: asset_id.type_id(),
                },
                tangents,
            };
            let pipeline = match pipelines.prepass.get(&pipeline_key) {
                Some(id) => *id,
                None => {
                    let id = build_prepass_descriptor(
                        &prepass,
                        pipeline_key.clone(),
                        &render_mesh.layout,
                        material,
                        &gpu.draw_layout,
                        &asset_server,
                    )
                    .map(|(descriptor, empty_material)| CachedClusterPrepass {
                        pipeline: pipeline_cache.queue_render_pipeline(descriptor),
                        empty_material,
                    });
                    pipelines.prepass.insert(pipeline_key, id);
                    id
                }
            };
            if let Some(pipeline) = pipeline {
                queued.push((*ready, pipeline));
            }
        }
        let Some(view_gpu) = gpu.views.get_mut(&key) else {
            continue;
        };
        for previous in view_gpu.prepass_items.drain(..) {
            opaque_phase.remove(previous);
            if let Some(alpha_mask_phase) = alpha_mask_phase.as_deref_mut() {
                alpha_mask_phase.remove(previous);
            }
        }
        view_gpu.readiness_revision += 1;
        view_gpu.prepass_pipelines = queued
            .iter()
            .map(|(ready, cached)| (ready.index, cached.pipeline))
            .collect();
        view_gpu.prepass_groups = queued.iter().map(|(ready, _)| ready.index).collect();
        view_gpu.prepass_has_opaque = queued.iter().any(|(ready, _)| !ready.alpha_mask);
        view_gpu.prepass_has_alpha_mask = queued.iter().any(|(ready, _)| ready.alpha_mask);
        for (ready, cached) in queued {
            let pipeline = cached.pipeline;
            let material_bind_group_index =
                (!cached.empty_material).then_some(ready.material_group);
            let opaque_draw = if cached.empty_material {
                opaque_empty_draw
            } else {
                opaque_draw
            };
            let alpha_mask_draw = if cached.empty_material {
                alpha_mask_empty_draw
            } else {
                alpha_mask_draw
            };
            view_gpu.prepass_items.push(ready.main_entity);
            if ready.alpha_mask {
                if let Some(alpha_mask_phase) = alpha_mask_phase.as_deref_mut() {
                    alpha_mask_phase.add(
                        OpaqueNoLightmap3dBatchSetKey {
                            draw_function: alpha_mask_draw,
                            pipeline,
                            material_bind_group_index,
                            slabs: MeshSlabs::default(),
                        },
                        OpaqueNoLightmap3dBinKey {
                            asset_id: batch_asset_id(ready.index),
                        },
                        (ready.render_entity, ready.main_entity),
                        InputUniformIndex::default(),
                        BinnedRenderPhaseType::NonMesh,
                    );
                }
            } else {
                opaque_phase.add(
                    OpaqueNoLightmap3dBatchSetKey {
                        draw_function: opaque_draw,
                        pipeline,
                        material_bind_group_index,
                        slabs: MeshSlabs::default(),
                    },
                    OpaqueNoLightmap3dBinKey {
                        asset_id: batch_asset_id(ready.index),
                    },
                    (ready.render_entity, ready.main_entity),
                    InputUniformIndex::default(),
                    BinnedRenderPhaseType::NonMesh,
                );
            }
        }
        // A plan with a material not yet prepared is queued again next frame.
        view_gpu.prepass_epoch = complete.then_some(epoch);
    }
}

type DrawClusterShadow = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetClusterShadowDataBindGroup<1>,
    DrawClusterIndirect,
);

/// The shadow draw of an alpha-masked batch: the light's prepass view, the mesh uniforms and
/// material at the groups the material's own prepass pipeline lays out, then the cluster data.
type DrawClusterShadowMask = (
    SetItemPipeline,
    SetPrepassViewBindGroup<0>,
    SetPrepassViewEmptyBindGroup<1>,
    SetClusterMeshBindGroup<2>,
    SetClusterMaterialBindGroup<3>,
    SetClusterDataBindGroup<4>,
    DrawClusterIndirect,
);

struct SetClusterShadowDataBindGroup<const I: usize>;

impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetClusterShadowDataBindGroup<I> {
    type Param = SRes<ClusterGpu>;
    type ViewQuery = &'static ExtractedView;
    type ItemQuery = &'static ClusterBatch;

    fn render<'w>(
        _: &P,
        view: ROQueryItem<'w, '_, Self::ViewQuery>,
        batch: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        gpu: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let gpu = gpu.into_inner();
        let _ = batch;
        let Some(view_gpu) = view_gpu(gpu, view) else {
            return RenderCommandResult::Skip;
        };
        pass.set_bind_group(I, &view_gpu.shadow_bind_group, &[]);
        RenderCommandResult::Success
    }
}

/// The depth-only pipeline a shadow view draws clusters with. A sun cascade (orthographic) needs
/// unclipped depth so casters in front of the cascade still write; the draw has no fragment stage
/// unless the adapter lacks depth clip control and the depth is emulated.
fn build_shadow_descriptor(
    prepass: &PrepassPipeline,
    shadow_layout: &BindGroupLayoutDescriptor,
    unclipped_depth: bool,
    asset_server: &AssetServer,
) -> RenderPipelineDescriptor {
    // Without the adapter's depth clip control the depth is emulated (see the shader): a fragment
    // stage with no colour target writes the unclamped depth.
    let emulate = unclipped_depth && !prepass.depth_clip_control_supported;
    let shader = asset_server.load("embedded://engine/cluster/shaders/cluster_shadow.wgsl");
    let shader_defs: Vec<ShaderDefVal> = if emulate {
        vec!["UNCLIPPED_DEPTH_ORTHO_EMULATION".into()]
    } else {
        Vec::new()
    };
    RenderPipelineDescriptor {
        label: Some("cluster shadow pipeline".into()),
        layout: vec![
            prepass.view_layout_no_motion_vectors.clone(),
            shadow_layout.clone(),
        ],
        vertex: VertexState {
            shader: shader.clone(),
            shader_defs: shader_defs.clone(),
            ..default()
        },
        fragment: emulate.then(|| FragmentState {
            shader,
            shader_defs,
            targets: Vec::new(),
            ..default()
        }),
        primitive: PrimitiveState {
            unclipped_depth: unclipped_depth && prepass.depth_clip_control_supported,
            ..default()
        },
        depth_stencil: Some(DepthStencilState {
            format: CORE_3D_DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::GreaterEqual),
            stencil: StencilState {
                front: StencilFaceState::IGNORE,
                back: StencilFaceState::IGNORE,
                read_mask: 0,
                write_mask: 0,
            },
            bias: DepthBiasState {
                constant: 0,
                slope_scale: 0.0,
                clamp: 0.0,
            },
        }),
        ..default()
    }
}

/// The mesh key a shadow view specializes the prepass pipeline by: the key the stock shadow queue
/// builds for a mesh that may discard (`render/light.rs`, `specialize_shadows`).
fn shadow_mesh_key(unclipped: bool, mesh_bits: MeshPipelineKey) -> MeshPipelineKey {
    let projection = if unclipped {
        MeshPipelineKey::VIEW_PROJECTION_ORTHOGRAPHIC | MeshPipelineKey::UNCLIPPED_DEPTH_ORTHO
    } else {
        MeshPipelineKey::VIEW_PROJECTION_PERSPECTIVE
    };
    MeshPipelineKey::DEPTH_PREPASS | projection | mesh_bits | MeshPipelineKey::MAY_DISCARD
}

/// The pipeline a shadow view draws an alpha-masked batch with: the material's own prepass
/// pipeline (its fragment stage tests the alpha cutoff and discards) behind the cluster vertex
/// stage, whose data group follows the material group as in the forward draw.
fn build_shadow_mask_descriptor(
    prepass: &PrepassPipeline,
    key: ClusterMaterialPipelineKey,
    layout: &MeshVertexBufferLayoutRef,
    material: &PreparedMaterial,
    draw_layout: &BindGroupLayoutDescriptor,
    asset_server: &AssetServer,
) -> Option<RenderPipelineDescriptor> {
    let specializer = PrepassPipelineSpecializer {
        pipeline: prepass.clone(),
        properties: material.properties.clone(),
    };
    let mut descriptor = specializer
        .specialize(key.material, layout)
        .map_err(|error| note_pipeline_failure("the alpha-mask shadow pipeline", &error))
        .ok()?;
    // Layout: view, empty, mesh, material; the cluster data follows at group 4.
    descriptor.layout.push(draw_layout.clone());
    descriptor.vertex.shader =
        asset_server.load("embedded://engine/cluster/shaders/cluster_shadow_mask.wgsl");
    descriptor.vertex.buffers.clear();
    descriptor.label = Some("cluster shadow mask pipeline".into());
    Some(descriptor)
}

/// One group of a shadow view's draw: its pipeline, the material bind group (alpha-masked groups
/// only), and the batch whose phase item issues the draw.
struct ShadowGroup {
    render_entity: Entity,
    main_entity: MainEntity,
    index: u32,
    pipeline: CachedRenderPipelineId,
    material_group: Option<u32>,
}

/// Adds the cluster batches to every shadow view's phase. A batch has no mesh of its own, so
/// Bevy's shadow visibility lists never name it: it is queued for every shadow-casting view each
/// frame, and each view's cull pass decides what is drawn. Opaque batches draw depth only, in one
/// group; alpha-masked ones draw through their material's own prepass fragment stage, one group
/// per pipeline and material bind group, so a tree's leaves cast the shadow of their cutout.
#[allow(clippy::too_many_arguments)]
fn queue_cluster_shadows(
    pipeline_cache: Res<PipelineCache>,
    prepass: Res<PrepassPipeline>,
    asset_server: Res<AssetServer>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut pipelines: ResMut<ClusterPipelines>,
    gpu: Option<ResMut<ClusterGpu>>,
    mut shadow_phases: ResMut<ViewBinnedRenderPhases<Shadow>>,
    draw_functions: Res<DrawFunctions<Shadow>>,
    stock: StockLookups,
    dummy_mesh: Option<Res<ClusterDummyMesh>>,
    lights: Query<(&LightEntity, &ExtractedView)>,
    batches: Query<(Entity, &MainEntity, &ClusterBatch)>,
    batch_materials: Res<ClusterBatchMaterials>,
    mut carrier_scratch: Local<Vec<Option<(Entity, MainEntity)>>>,
) {
    perf_span!("cluster::queue_shadows::total");
    let _timer = queue_stats::Timed::start(&queue_stats::SHADOW_NANOS);
    let Some(mut gpu) = gpu else {
        return;
    };
    carrier_scratch.clear();
    let carriers = &mut *carrier_scratch;
    for (entity, main_entity, marker) in &batches {
        if carriers.len() <= marker.index as usize {
            carriers.resize(marker.index as usize + 1, None);
        }
        carriers[marker.index as usize] = Some((entity, *main_entity));
    }
    let StockLookups {
        sources,
        materials,
        render_meshes,
        ..
    } = stock;
    let layouts_ready = dummy_mesh.as_ref().is_some_and(|dummy| {
        render_meshes.get(dummy.plain.id()).is_some()
            && render_meshes.get(dummy.tangent.id()).is_some()
    });
    let opaque_draw = draw_functions.read().id::<DrawClusterShadow>();
    let mask_draw = draw_functions.read().id::<DrawClusterShadowMask>();
    for (light, view) in &lights {
        let Some(phase) = shadow_phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let unclipped = matches!(light, LightEntity::Directional { .. });
        let key = view.retained_view_entity;
        if !gpu.views.contains_key(&key) {
            let view_gpu = new_view_gpu(&device, &pipeline_cache, &gpu);
            gpu.views.insert(key, view_gpu);
        }
        // The bins persist between frames: a view whose plan was made under this generation of
        // the batches, with the dummy mesh in the same state, still has it queued.
        let plan = gpu
            .generation
            .wrapping_add(gpu.layout_epoch.rotate_left(24))
            .wrapping_add(batch_materials.version.rotate_left(40))
            .wrapping_add(sources.layout_version.rotate_left(11))
            .wrapping_mul(4)
            + u64::from(unclipped)
            + 2 * u64::from(layouts_ready);
        if gpu.views.get(&key).is_some_and(|view_gpu| {
            view_gpu.plan_hash == Some(plan)
                && view_gpu.walk_carriers == *carriers
                && (view_gpu.plan_items.is_empty() || !phase.non_mesh_items.is_empty())
        }) {
            queue_stats::bump(&queue_stats::SHADOW_SKIPPED);
            continue;
        }
        perf_span!("cluster::queue_shadows::rebuild");
        queue_stats::bump(&queue_stats::SHADOW_FULL);
        let opaque_pipeline = *pipelines.shadow.entry(unclipped).or_insert_with(|| {
            pipeline_cache.queue_render_pipeline(build_shadow_descriptor(
                &prepass,
                &gpu.shadow_layout,
                unclipped,
                &asset_server,
            ))
        });
        let mut groups = DrawGroups::new(gpu.batches.len());
        let mut group_of_key: HashMap<(CachedRenderPipelineId, Option<u32>), u32> = HashMap::new();
        let mut shadow_groups: Vec<ShadowGroup> = Vec::new();
        let mut complete = true;
        for index in 0..gpu.batches.len() {
            let batch = &gpu.batches[index];
            if batch.slots == 0 || batch.material.is_none() {
                continue;
            }
            let Some(tangents) = batch
                .material
                .filter(|asset| batch_materials.ids.get(index) == Some(&Some(*asset)))
                .and_then(|asset| batch_materials.geometry.get(&asset).copied())
                .and_then(|geometry| {
                    batch_tangent_layout(batch, index, batch.material?, geometry, &sources)
                })
                .filter(|layout| batch.tangents == Some(*layout))
            else {
                complete = false;
                continue;
            };
            let render_mesh = dummy_mesh
                .as_ref()
                .and_then(|dummy| render_meshes.get(dummy.asset(tangents)));
            let mesh_key = render_mesh.map(|mesh| {
                shadow_mesh_key(
                    unclipped,
                    MeshPipelineKey::from_bits_retain(mesh.key_bits.bits()),
                )
            });
            let slots = batch.slots;
            let (pipeline, material_group) = match (&batch.cutout, mesh_key, render_mesh) {
                (Some(cutout), Some(mesh_key), Some(render_mesh)) => {
                    let key = ClusterMaterialPipelineKey {
                        material: ErasedMaterialPipelineKey {
                            type_id: cutout.type_id,
                            mesh_key: ErasedMeshPipelineKey::new(mesh_key),
                            material_key: cutout.material_key.clone(),
                        },
                        tangents,
                    };
                    let pipeline = match pipelines.shadow_mask.get(&key) {
                        Some(id) => Some(*id),
                        None => materials
                            .get(cutout.asset)
                            .and_then(|material| {
                                build_shadow_mask_descriptor(
                                    &prepass,
                                    key.clone(),
                                    &render_mesh.layout,
                                    material,
                                    &gpu.draw_layout,
                                    &asset_server,
                                )
                            })
                            .map(|descriptor| {
                                let id = pipeline_cache.queue_render_pipeline(descriptor);
                                pipelines.shadow_mask.insert(key, id);
                                id
                            }),
                    };
                    // A cutout whose pipeline cannot be built yet casts nothing this frame
                    // rather than a solid shadow; the next frame retries.
                    let Some(pipeline) = pipeline else {
                        complete = false;
                        continue;
                    };
                    (pipeline, Some(cutout.material_group))
                }
                (Some(_), ..) => {
                    complete = false;
                    continue;
                }
                (None, ..) => (opaque_pipeline, None),
            };
            let group_key = (pipeline, material_group);
            let known = group_of_key.get(&group_key).copied();
            let group = groups.add(index as u32, slots, known);
            if known.is_none() {
                group_of_key.insert(group_key, group);
                let Some((render_entity, main_entity)) =
                    carriers.get(group as usize).copied().flatten()
                else {
                    carrier_shortage(group as usize);
                    complete = false;
                    continue;
                };
                shadow_groups.push(ShadowGroup {
                    render_entity,
                    main_entity,
                    index: group,
                    pipeline,
                    material_group,
                });
            }
        }
        let order = groups.shared_order(&mut gpu);
        if let Some(view_gpu) = gpu.views.get_mut(&key) {
            for previous in view_gpu.plan_items.drain(..) {
                phase.remove(previous);
            }
            queue_stats::GROUPS_SHADOW.store(
                groups.groups.len() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
            view_gpu.readiness_revision += 1;
            view_gpu.shadow_pipelines = shadow_groups
                .iter()
                .map(|group| (group.index, group.pipeline))
                .collect();
            groups.apply(view_gpu, &queue, order);
            // A plan with a cutout that could not be built yet is made again next frame.
            view_gpu.plan_hash = complete.then_some(plan);
            view_gpu.walk_carriers.clone_from(carriers);
            view_gpu
                .plan_items
                .extend(shadow_groups.iter().map(|group| group.main_entity));
        }
        for group in shadow_groups {
            phase.add(
                ShadowBatchSetKey {
                    pipeline: group.pipeline,
                    draw_function: if group.material_group.is_some() {
                        mask_draw
                    } else {
                        opaque_draw
                    },
                    material_bind_group_index: group.material_group,
                    slabs: MeshSlabs::default(),
                },
                ShadowBinKey {
                    asset_id: batch_asset_id(group.index),
                },
                (group.render_entity, group.main_entity),
                InputUniformIndex::default(),
                BinnedRenderPhaseType::NonMesh,
            );
        }
    }
}

fn bytes_for_instances(
    bytes: &mut Vec<u8>,
    flags: u32,
    slot: u32,
    instances: &[ClusterInstance],
    offset: Vec3,
    visible_slots: u32,
) {
    for instance in instances {
        bytes.extend_from_slice(&mesh_uniform_bytes(
            placed_matrix(instance, offset),
            flags,
            slot,
            terrain_record_tag(instance, visible_slots),
        ));
    }
}

/// The readiness inputs, in the order of [`TERRAIN_READINESS_FIELDS`].
type TerrainReadinessReport = [usize; 14];

const TERRAIN_READINESS_FIELDS: &str = "gpu, ids, readiness, compute pipelines, terrain prepass pipelines,      prepass pipelines built, no shadow phases, terrain shadow pipelines, shadow pipelines built,      batches with a material, material prepared, main pass ready in some view, prepass group queued      in every prepass view, ready";

/// Logs the readiness inputs when they change, so a terrain that never becomes drawable says which
/// step it is stuck on.
fn note_terrain_readiness(
    logged: &mut Option<TerrainReadinessReport>,
    report: TerrainReadinessReport,
) {
    if *logged != Some(report) {
        *logged = Some(report);
        info!(
            ?report,
            fields = TERRAIN_READINESS_FIELDS,
            "cluster terrain readiness inputs changed"
        );
    }
}

/// Logs, when it changes, what each view's plan holds for the terrain batches: whether the view has
/// a prepass phase, its plan size, how many plan groups are terrain's, how many groups the prepass
/// queued, whether its plan counts as complete, and how many of the terrain batches its main pass
/// has a built pipeline for.
fn note_terrain_views(
    logged: &mut Vec<[usize; 13]>,
    gpu: &ClusterGpu,
    opaque_prepass: &ViewBinnedRenderPhases<Opaque3dPrepass>,
    terrain_batches: &[u32],
    cache: &PipelineCache,
    materials: &ErasedRenderAssets<PreparedMaterial>,
) {
    let mut views: Vec<[usize; 13]> = gpu
        .views
        .iter()
        .map(|(key, view)| {
            let terrain_groups = view
                .plan_reps
                .iter()
                .filter(|rep| terrain_batches.contains(&rep.index))
                .count();
            let terrain_memos = terrain_batches
                .iter()
                .filter(|&&batch| view.memo.get(batch as usize).is_some_and(Option::is_some))
                .count();
            // Separate async pipeline progress, missing plan representatives and
            // prepared-table relocation before attributing a coverage difference.
            let mut causes = [0usize; 7];
            for &batch in terrain_batches {
                let Some(memo) = view.memo.get(batch as usize).and_then(Option::as_ref) else {
                    continue;
                };
                match cache.get_render_pipeline_state(memo.resolved.pipeline) {
                    CachedPipelineState::Ok(_) => causes[0] += 1,
                    CachedPipelineState::Queued => causes[1] += 1,
                    CachedPipelineState::Creating(..) => causes[2] += 1,
                    CachedPipelineState::Err(_) => causes[3] += 1,
                }
                causes[4] += usize::from(
                    view.plan_reps
                        .iter()
                        .any(|rep| rep.pipeline == memo.resolved.pipeline),
                );
                causes[5] += usize::from(
                    view.plan_reps
                        .iter()
                        .any(|rep| rep.pipeline == memo.resolved.pipeline && rep.alpha_mask),
                );
                causes[6] += usize::from(materials.get(memo.asset_id).is_some_and(|material| {
                    memo.material_address != std::ptr::from_ref(material) as usize
                }));
            }
            [
                usize::from(opaque_prepass.contains_key(key)),
                view.plan_reps.len(),
                terrain_groups,
                view.prepass_groups.len(),
                usize::from(view.prepass_epoch == Some(view.plan_epoch)),
                terrain_memos,
                causes[0],
                causes[1],
                causes[2],
                causes[3],
                causes[4],
                causes[5],
                causes[6],
            ]
        })
        .collect();
    views.sort_unstable();
    if *logged != views {
        debug!(
            ?views,
            fields = "per view: has prepass phase, plan groups, terrain plan groups, prepass groups queued, prepass plan complete, terrain batches with a memo, pipeline ready, pipeline queued, pipeline creating, pipeline failed, pipeline has any representative, pipeline has masked representative, prepared material address changed",
            "cluster terrain per-view plan changed"
        );
        *logged = views;
    }
}

/// Publishes only terrain batches whose material and camera pipeline can actually draw.
#[allow(clippy::too_many_arguments)]
fn publish_terrain_readiness(
    far_state: Option<Res<crate::far_terrain_shadows::render::State>>,
    gpu: Option<Res<ClusterGpu>>,
    pipelines: Res<ClusterPipelines>,
    cache: Res<PipelineCache>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    shadow_phases: Res<ViewBinnedRenderPhases<Shadow>>,
    ids: Option<Res<super::package::TerrainBatchIds>>,
    readiness: Option<Res<super::terrain::TerrainDrawReadiness>>,
    opaque_prepass: Res<ViewBinnedRenderPhases<Opaque3dPrepass>>,
    mut logged: Local<Option<TerrainReadinessReport>>,
    mut logged_views: Local<Vec<[usize; 13]>>,
) {
    perf_span!("cluster::terrain_readiness::total");
    let mut report: TerrainReadinessReport = [0; 14];
    report[0] = usize::from(gpu.is_some());
    report[1] = usize::from(ids.is_some());
    report[2] = usize::from(readiness.is_some());
    let (Some(gpu), Some(ids), Some(readiness)) = (gpu, ids, readiness) else {
        note_terrain_readiness(&mut logged, report);
        return;
    };
    let compute_ready = [pipelines.cull, pipelines.instance_cull, pipelines.shift]
        .into_iter()
        .chain(pipelines.compaction)
        .all(|id| id.is_some_and(|id| cache.get_compute_pipeline(id).is_some()));
    let terrain_type = std::any::TypeId::of::<super::terrain::ClusterTerrainMaterial>();
    let terrain_prepasses: Vec<_> = pipelines
        .prepass
        .iter()
        .filter(|(key, _)| key.material.type_id == terrain_type)
        .collect();
    let terrain_shadows: Vec<_> = pipelines
        .shadow_mask
        .iter()
        .filter(|(key, _)| key.material.type_id == terrain_type)
        .collect();
    let prepass_built = terrain_prepasses
        .iter()
        .filter(|(_, cached)| {
            cached.is_some_and(|cached| cache.get_render_pipeline(cached.pipeline).is_some())
        })
        .count();
    let shadow_built = terrain_shadows
        .iter()
        .filter(|(_, id)| cache.get_render_pipeline(**id).is_some())
        .count();
    // Only a view with a prepass phase gets a prepass plan (`queue_cluster_prepass`), so a camera
    // running without one (no SSAO, no TAA) has none to wait for.
    let prepass_views: Vec<_> = gpu
        .views
        .iter()
        .filter(|(key, _)| opaque_prepass.contains_key(*key))
        .map(|(_, view)| view)
        .collect();
    let prepass_needed = !prepass_views.is_empty();
    let auxiliary_ready = compute_ready
        && (!prepass_needed
            || (!terrain_prepasses.is_empty() && prepass_built == terrain_prepasses.len()))
        && (shadow_phases.is_empty()
            || (!terrain_shadows.is_empty() && shadow_built == terrain_shadows.len()));
    report[3] = usize::from(compute_ready);
    report[4] = terrain_prepasses.len();
    report[5] = prepass_built;
    report[6] = usize::from(shadow_phases.is_empty());
    report[7] = terrain_shadows.len();
    report[8] = shadow_built;
    let mut valid: Vec<u32> = Vec::new();
    for &batch in &ids.0 {
        let Some(asset) = gpu.batches.get(batch as usize).and_then(|b| b.material) else {
            continue;
        };
        report[9] += 1;
        if materials.get(asset).is_none() {
            continue;
        }
        report[10] += 1;
        // The main pass: some view has this batch's pipeline built and a plan group drawing it.
        let main_groups: Vec<(&ViewGpu, u32)> = gpu
            .views
            .values()
            .filter_map(|view| {
                let memo = view.memo.get(batch as usize).and_then(Option::as_ref)?;
                if memo.resolved.far && !far::current(&gpu, view, far_state.as_deref()) {
                    return None;
                }
                cache
                    .get_render_pipeline(memo.resolved.pipeline)
                    .is_some()
                    .then_some(())?;
                let rep = view
                    .plan_reps
                    .iter()
                    .find(|rep| rep.pipeline == memo.resolved.pipeline && rep.alpha_mask)?;
                Some((view, rep.index))
            })
            .collect();
        if main_groups.is_empty() {
            continue;
        }
        report[11] += 1;
        // The prepass: every view that has a prepass phase must have queued this batch's group.
        // (The plan's own `prepass_epoch` also waits on every other group's material, which this
        // batch does not depend on.)
        let prepass_queued = prepass_views.iter().all(|view| {
            main_groups.iter().any(|(main, group)| {
                std::ptr::eq(*main, *view) && view.prepass_groups.contains(group)
            })
        });
        if !prepass_queued {
            continue;
        }
        report[12] += 1;
        report[13] += 1;
        if auxiliary_ready {
            valid.push(batch);
        }
    }
    // Avoid the diagnostic's per-view scans and allocations unless its log is enabled.
    if bevy::log::tracing::enabled!(bevy::log::tracing::Level::DEBUG) {
        note_terrain_views(
            &mut logged_views,
            &gpu,
            &opaque_prepass,
            &ids.0,
            &cache,
            &materials,
        );
    }
    note_terrain_readiness(&mut logged, report);
    if ids
        .0
        .iter()
        .any(|&batch| readiness.contains(batch) && !valid.contains(&batch))
    {
        readiness.clear();
    }
    for batch in valid {
        readiness.mark(batch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable;

    #[test]
    fn cull_uniform_uploads_once_until_shader_inputs_change() {
        let mut uploaded = None;
        let mut params = CullParams::zeroed();
        let first = cull_params_to_upload(&mut uploaded, params).expect("first upload");
        assert_eq!(first, &[0; CULL_PARAMS_BYTES]);
        assert!(cull_params_to_upload(&mut uploaded, params).is_none());
        params.epoch = 42;
        assert!(cull_params_to_upload(&mut uploaded, params).is_none());
        params.instance_count = 17;
        let bytes = cull_params_to_upload(&mut uploaded, params).expect("new instances");
        assert_eq!(&bytes[108..112], &17_u32.to_ne_bytes());
        assert_eq!(&bytes[124..128], &[0; 4]);
        assert!(cull_params_to_upload(&mut uploaded, params).is_none());
        uploaded = None;
        assert!(cull_params_to_upload(&mut uploaded, params).is_some());
    }

    #[test]
    fn cull_uniform_upload_observes_every_shader_input_byte() {
        let params = CullParams::zeroed();
        for byte in 0..std::mem::offset_of!(CullParams, epoch) {
            let mut uploaded = Some(params);
            let mut changed = params;
            bytemuck::bytes_of_mut(&mut changed)[byte] = 1;
            let bytes = cull_params_to_upload(&mut uploaded, changed)
                .unwrap_or_else(|| panic!("shader input byte {byte} changed"));
            assert_eq!(bytes[byte], 1);
        }
    }

    #[test]
    fn cull_uniform_upload_compares_float_bits_including_nan_and_signed_zero() {
        let mut uploaded = None;
        let mut params = CullParams::zeroed();
        params.near = f32::from_bits(0x7fc0_0001);
        assert!(cull_params_to_upload(&mut uploaded, params).is_some());
        assert!(cull_params_to_upload(&mut uploaded, params).is_none());
        params.near = f32::from_bits(0x7fc0_0002);
        assert!(cull_params_to_upload(&mut uploaded, params).is_some());
        params.near = 0.0;
        assert!(cull_params_to_upload(&mut uploaded, params).is_some());
        params.near = -0.0;
        assert!(cull_params_to_upload(&mut uploaded, params).is_some());
    }

    #[test]
    fn optional_layout_refusal_queues_ordinary_once_and_does_not_retry_the_refused_variant() {
        let draw = BindGroupLayoutDescriptor::new(
            "cluster draw fixture",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::VERTEX,
                (
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                    storage_buffer_read_only_sized(false, None),
                ),
            ),
        );
        let descriptor = |far| {
            let mut layout = vec![BindGroupLayoutDescriptor::default(); 4];
            layout[3]
                .entries
                .push(uniform_buffer_sized(false, None).build(0, ShaderStages::FRAGMENT));
            layout.push(if far {
                far::forward_layout(&draw)
            } else {
                draw.clone()
            });
            RenderPipelineDescriptor {
                layout,
                ..default()
            }
        };
        let limits = wgpu::Limits {
            max_bind_groups: 5,
            max_uniform_buffers_per_shader_stage: 3,
            ..default()
        };
        assert!(crate::far_terrain_shadows::layout::validate(&descriptor(false), &limits).is_ok());
        assert!(crate::far_terrain_shadows::layout::validate(&descriptor(true), &limits).is_err());
        let key = ClusterMaterialPipelineKey {
            material: ErasedMaterialPipelineKey {
                mesh_key: ErasedMeshPipelineKey::new(MeshPipelineKey::empty()),
                material_key: bevy::material::key::ErasedMaterialKey::new(0_u32),
                type_id: std::any::TypeId::of::<StandardMaterial>(),
            },
            tangents: true,
        };
        let mut pipelines = ClusterPipelines::default();
        let mut built = Vec::new();
        let mut queued = 0;
        for _ in 0..2 {
            let selected = pipelines.forward_pipeline(
                key.clone(),
                true,
                &limits,
                |far| {
                    built.push(far);
                    Some(descriptor(far))
                },
                |_| {
                    queued += 1;
                    CachedRenderPipelineId::INVALID
                },
                |_| true,
            );
            assert_eq!(selected, Some((CachedRenderPipelineId::INVALID, false)));
        }
        assert_eq!(built, [true, false]);
        assert_eq!(queued, 1);
        assert!(pipelines.far_layout_refusals.contains(&key));
        assert!(!pipelines.render.contains_key(&(key, true)));

        // Refusal is exact to the specialization, not a permanent global off switch.
        let other = ClusterMaterialPipelineKey {
            material: ErasedMaterialPipelineKey {
                mesh_key: ErasedMeshPipelineKey::new(MeshPipelineKey::empty()),
                material_key: bevy::material::key::ErasedMaterialKey::new(1_u32),
                type_id: std::any::TypeId::of::<StandardMaterial>(),
            },
            tangents: true,
        };
        let other_descriptor = |far| {
            let mut descriptor = descriptor(far);
            descriptor.layout[3].entries.clear();
            descriptor
        };
        assert!(
            crate::far_terrain_shadows::layout::validate(&other_descriptor(true), &limits).is_ok()
        );
        let selected = pipelines.forward_pipeline(
            other.clone(),
            true,
            &limits,
            |far| Some(other_descriptor(far)),
            |_| CachedRenderPipelineId::INVALID,
            |_| true,
        );
        assert_eq!(selected, Some((CachedRenderPipelineId::INVALID, true)));
        assert!(!pipelines.far_layout_refusals.contains(&other));
    }

    #[test]
    fn uncompiled_optional_pipeline_keeps_ordinary_and_readiness_changes_wake_the_walk() {
        use std::cell::Cell;
        let key = ClusterMaterialPipelineKey {
            material: ErasedMaterialPipelineKey {
                mesh_key: ErasedMeshPipelineKey::new(MeshPipelineKey::empty()),
                material_key: bevy::material::key::ErasedMaterialKey::new(0_u32),
                type_id: std::any::TypeId::of::<StandardMaterial>(),
            },
            tangents: true,
        };
        let mut pipelines = ClusterPipelines::default();
        let compiled = Cell::new(false);
        let mut built = Vec::new();
        let mut queued = 0;
        let mut select = |pipelines: &mut ClusterPipelines| {
            pipelines.forward_pipeline(
                key.clone(),
                true,
                &wgpu::Limits::default(),
                |far| {
                    built.push(far);
                    Some(RenderPipelineDescriptor::default())
                },
                |_| {
                    queued += 1;
                    CachedRenderPipelineId::INVALID
                },
                |_| compiled.get(),
            )
        };
        // Queued, unresolved imports and failed compilation all lack a compiled
        // pipeline. None of them may replace ordinary geometry or requeue it.
        for _ in 0..3 {
            assert_eq!(
                select(&mut pipelines),
                Some((CachedRenderPipelineId::INVALID, false))
            );
        }
        pipelines.refresh_far_readiness(|_| compiled.get());
        assert_eq!(pipelines.far_readiness_revision, 0);

        compiled.set(true);
        pipelines.refresh_far_readiness(|_| compiled.get());
        assert_eq!(pipelines.far_readiness_revision, 1);
        assert_eq!(
            select(&mut pipelines),
            Some((CachedRenderPipelineId::INVALID, true))
        );
        pipelines.refresh_far_readiness(|_| compiled.get());
        assert_eq!(pipelines.far_readiness_revision, 1);

        // A shader withdrawal/reload revokes the optional selection and wakes
        // the cached walk again, without a material event or a new pipeline.
        compiled.set(false);
        pipelines.refresh_far_readiness(|_| compiled.get());
        assert_eq!(pipelines.far_readiness_revision, 2);
        assert_eq!(
            select(&mut pipelines),
            Some((CachedRenderPipelineId::INVALID, false))
        );
        assert_eq!(built, [true, false]);
        assert_eq!(queued, 2);
        assert!(pipelines.far_layout_refusals.is_empty());
    }

    #[test]
    fn prepass_material_binding_follows_the_specialized_layout_and_keeps_group_four_free() {
        use bevy::render::render_resource::{BindGroupLayoutEntry, BindingType, BufferBindingType};
        let empty = BindGroupLayoutDescriptor::new("empty", &[]);
        let mut descriptor = RenderPipelineDescriptor {
            layout: vec![empty.clone(); 4],
            ..default()
        };
        assert_eq!(prepass_empty_material_layout(&descriptor), Some(true));
        descriptor.layout[3] = BindGroupLayoutDescriptor::new(
            "material",
            &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        );
        assert_eq!(prepass_empty_material_layout(&descriptor), Some(false));
        descriptor.layout.pop();
        assert_eq!(prepass_empty_material_layout(&descriptor), None);
        descriptor.layout.extend([empty.clone(), empty]);
        assert_eq!(prepass_empty_material_layout(&descriptor), None);
    }

    const MCL: &[u8] = include_bytes!("../../../shared/tests/data/cluster/fixture.mcl");

    fn word(bytes: &[u8], index: usize) -> u32 {
        u32::from_le_bytes([
            bytes[index * 4],
            bytes[index * 4 + 1],
            bytes[index * 4 + 2],
            bytes[index * 4 + 3],
        ])
    }

    #[test]
    fn a_dispatch_covers_every_workgroup_in_rows_within_the_limit() {
        for groups in [1u32, 65_534, 65_535, 65_536, 131_070, 131_071, 4_000_000] {
            let [x, y, z] = dispatch_dims(groups);
            assert!(
                x <= DISPATCH_ROW && y >= 1 && z == 1,
                "{groups}: {x} {y} {z}"
            );
            assert!(u64::from(x) * u64::from(y) >= u64::from(groups), "{groups}");
            assert!(
                u64::from(x) * u64::from(y - 1) < u64::from(groups),
                "{groups} has an empty row"
            );
        }
        assert_eq!(dispatch_dims(65_536), [65_535, 2, 1]);
    }

    #[test]
    fn padding_keeps_an_empty_section_empty_and_rounds_others_to_words() {
        assert!(padded(&[]).is_empty());
        assert_eq!(padded(&[1]).len(), 4);
        assert_eq!(padded(&[1; 8]).len(), 8);
    }

    #[test]
    fn local_indices_widen_losslessly_and_keep_uploads_word_aligned() {
        assert!(cluster_index_bytes(&[]).is_empty());
        assert_eq!(
            cluster_index_bytes(&[0, 255, 1]),
            [0, 0, 255, 0, 1, 0, 0, 0]
        );
        assert_eq!(cluster_index_bytes(&[255, 0]), [255, 0, 0, 0]);
    }

    #[test]
    fn indexed_fixture_expands_to_the_original_triangle_list_in_every_pooled_mesh() {
        let mesh = ClusterMeshAsset::from_bytes(MCL.to_vec()).expect("fixture is valid");
        let pool = pool_meshes(&[mesh.clone(), mesh.clone()]);
        let mut triangle_count = 0;
        for row in [0, 16] {
            let vertex_base = word(&pool.infos, row);
            let vertex_ref_base = word(&pool.infos, row + 2);
            let index_base = word(&pool.infos, row + 3);
            let view = mesh.view();
            for id in 0..mesh.cluster_count as usize {
                let cluster = view.cluster(id);
                let count = usize::from(cluster.triangle_count) * 3;
                let first_index = index_base as usize + cluster.triangle_offset as usize * 3;
                let original = (0..count)
                    .map(|vertex_index| {
                        // The former vertex shader decoded triangle/corner from vertex_index.
                        let byte = (cluster.triangle_offset as usize + vertex_index / 3) * 3
                            + vertex_index % 3;
                        let local = mesh.section(MclSection::ClusterTriangles)[byte];
                        vertex_base
                            + word(
                                &pool.cluster_vertices,
                                vertex_ref_base as usize
                                    + cluster.vertex_offset as usize
                                    + usize::from(local),
                            )
                    })
                    .collect::<Vec<_>>();
                let indexed = (first_index..first_index + count)
                    .map(|index| {
                        let local = u16::from_le_bytes(
                            pool.cluster_indices[index * 2..index * 2 + 2]
                                .try_into()
                                .unwrap(),
                        );
                        vertex_base
                            + word(
                                &pool.cluster_vertices,
                                vertex_ref_base as usize
                                    + cluster.vertex_offset as usize
                                    + usize::from(local),
                            )
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    indexed, original,
                    "mesh row {row}, cluster {id}: order and winding"
                );
                triangle_count += count / 3;
            }
        }
        assert!(triangle_count > 0, "fixture must exercise actual triangles");
    }

    #[test]
    fn streamed_meshes_land_where_the_whole_scene_pool_puts_them() {
        let mesh = ClusterMeshAsset::from_bytes(MCL.to_vec()).expect("fixture is valid");
        let meshes = [mesh.clone(), mesh.clone(), mesh];
        let pool = pool_meshes(&meshes);
        // The sizes the buffers are created with, and the rows and sections each mesh is written
        // with as it arrives, are the whole-scene pool's.
        let mut at = PoolCursors::default();
        let mut rows = Vec::new();
        for mesh in &meshes {
            rows.extend_from_slice(&mesh_info_row(&at, mesh, false));
            let sizes = mesh_pool_sizes(mesh);
            let sections = mesh_sections(mesh);
            assert_eq!(sizes.positions, sections.positions.len() as u64);
            assert_eq!(sizes.attributes, sections.attributes.len() as u64);
            assert_eq!(sizes.clusters, sections.clusters.len() as u64);
            assert_eq!(
                sizes.cluster_vertices,
                sections.cluster_vertices.len() as u64
            );
            assert_eq!(sizes.cluster_indices, sections.cluster_indices.len() as u64);
            assert_eq!(sizes.groups, sections.groups.len() as u64);
            at.advance(&sizes);
        }
        assert_eq!(rows, pool.infos);
        assert_eq!(at.positions, pool.positions.len() as u64);
        assert_eq!(at.attributes, pool.attributes.len() as u64);
        assert_eq!(at.clusters, pool.clusters.len() as u64);
        assert_eq!(at.cluster_vertices, pool.cluster_vertices.len() as u64);
        assert_eq!(at.cluster_indices, pool.cluster_indices.len() as u64);
        assert_eq!(at.groups, pool.groups.len() as u64);
    }

    #[test]
    fn pooled_meshes_start_where_the_ones_before_them_end() {
        let mesh = ClusterMeshAsset::from_bytes(MCL.to_vec()).expect("fixture is valid");
        let pool = pool_meshes(&[mesh.clone(), mesh.clone()]);
        // A MeshInfo row is eight words of bases and counts, then uv (4) and bounds (4).
        let row = 16;
        assert_eq!(pool.infos.len(), 2 * row * 4);
        assert_eq!(word(&pool.infos, 0), 0);
        assert_eq!(word(&pool.infos, row), mesh.vertex_count);
        assert_eq!(word(&pool.infos, row + 1), mesh.cluster_count);
        assert_eq!(word(&pool.infos, row + 4), mesh.group_count);
        assert_eq!(pool.cluster_counts, vec![mesh.cluster_count; 2]);
        assert_eq!(pool.positions.len(), 2 * mesh.vertex_count as usize * 12);
        assert_eq!(pool.clusters.len(), 2 * mesh.cluster_count as usize * 48);
    }

    #[test]
    fn a_mesh_uniform_has_the_size_the_stock_shaders_read_and_keeps_the_winding_bit() {
        let keeps = mesh_uniform_bytes(Mat4::IDENTITY, MESH_FLAGS, 3, 0);
        assert_eq!(keeps.len(), MESH_UNIFORM_BYTES);
        // The flags are the first of the eight trailing words (after 12 + 12 + 9 floats).
        let flags_word = 12 + 12 + 9;
        assert_ne!(word(&keeps, flags_word) & (1 << 31), 0);
        let mirrored = mesh_uniform_bytes(
            Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)),
            MESH_FLAGS,
            3,
            0,
        );
        assert_eq!(word(&mirrored, flags_word) & (1 << 31), 0);
    }

    #[test]
    fn an_instance_record_becomes_a_column_major_matrix() {
        let record = InstanceRecord {
            transform: [
                1.0, 0.0, 0.0, 10.0, 0.0, 1.0, 0.0, 20.0, 0.0, 0.0, 1.0, 30.0,
            ],
            mesh: 0,
            cell_x: 0,
            cell_y: 0,
            stable_id: [0; 16],
            flags: 0,
            base_form_id: 0,
            snow_material: 0,
            source_worldspace_id: 0,
        };
        let matrix = world_from_local(&record);
        assert_eq!(matrix.w_axis, Vec4::new(10.0, 20.0, 30.0, 1.0));
        assert_eq!(matrix.x_axis, Vec4::new(1.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn draw_groups_slice_the_argument_list_in_order_and_batches_know_their_slice() {
        let mut groups = DrawGroups::new(4);
        let first = groups.add(0, 10, None);
        let second = groups.add(1, 5, None);
        // Batch 2 shares the first group's pipeline and material bind group.
        assert_eq!(groups.add(2, 7, Some(first)), first);
        let (draws, table) = groups.plan();
        assert_eq!(draws.len(), 2);
        assert_eq!(
            (
                draws[0].args_offset,
                draws[0].max_count,
                draws[0].count_offset
            ),
            (0, 17, 0)
        );
        assert_eq!(
            (
                draws[1].args_offset,
                draws[1].max_count,
                draws[1].count_offset
            ),
            (17 * DRAW_INDEXED_INDIRECT_BYTES, 5, 4)
        );
        let entry = |batch: usize| -> [u32; 4] {
            std::array::from_fn(|index| word(&table, batch * 4 + index))
        };
        assert_eq!(entry(0), [1, first, 0, 0]);
        assert_eq!(entry(1), [1, second, 17, 0]);
        assert_eq!(entry(2), [1, first, 0, 0]);
        assert_eq!(
            entry(3),
            [0, 0, 0, 0],
            "a batch not in a group does not draw"
        );
    }

    #[test]
    fn material_slot_rewrites_keep_distinct_per_instance_terrain_record_tags() {
        let instances = [0, 2].map(|block| ClusterInstance {
            identity: ClusterInstanceIdentity::Terrain {
                block,
                worldspace: 7,
                tier: 16,
                anchor: [4, -2],
            },
            // Shared geometry/material is deliberately insufficient to select the record.
            mesh: 9,
            batch: 0,
            world_from_local: Mat4::from_translation(Vec3::new(2.0, 3.0, 4.0)),
            source_worldspace: 0,
        });
        let mut original = Vec::new();
        bytes_for_instances(
            &mut original,
            MESH_FLAGS,
            3,
            &instances,
            Vec3::new(-1.0, 0.0, 5.0),
            7,
        );
        let mut rewritten = Vec::new();
        bytes_for_instances(
            &mut rewritten,
            MESH_FLAGS,
            11,
            &instances,
            Vec3::new(-1.0, 0.0, 5.0),
            7,
        );
        assert_eq!(original.len(), 352);
        assert_eq!(rewritten.len(), 352);
        for (index, expected_tag) in [7u32, 47].into_iter().enumerate() {
            // Fixed stock MeshUniform offsets: slot 152, tag 156, stride 176.
            let base = index * 176;
            for bytes in [&original, &rewritten] {
                assert_eq!(
                    u32::from_le_bytes(bytes[base + 156..base + 160].try_into().unwrap()),
                    expected_tag,
                );
            }
            assert_eq!(
                u32::from_le_bytes(original[base + 152..base + 156].try_into().unwrap()),
                0xffff_0003,
            );
            assert_eq!(
                u32::from_le_bytes(rewritten[base + 152..base + 156].try_into().unwrap()),
                0xffff_000b,
            );
            assert_eq!(&original[base..base + 152], &rewritten[base..base + 152]);
            assert_eq!(
                &original[base + 156..base + 176],
                &rewritten[base + 156..base + 176]
            );
        }
    }
}

#[cfg(test)]
mod tangent_layout_regressions {
    use super::*;

    fn material_key(tangents: bool) -> ClusterMaterialPipelineKey {
        ClusterMaterialPipelineKey {
            material: ErasedMaterialPipelineKey {
                mesh_key: ErasedMeshPipelineKey::new(MeshPipelineKey::empty()),
                material_key: ErasedMaterialKey::new(0_u32),
                type_id: std::any::TypeId::of::<StandardMaterial>(),
            },
            tangents,
        }
    }

    #[test]
    fn plain_and_tangent_layouts_keep_the_other_vertex_attributes_identical() {
        let plain = dummy_mesh_with_tangents(false);
        let tangent = dummy_mesh_with_tangents(true);
        assert!(!plain.contains_attribute(Mesh::ATTRIBUTE_TANGENT));
        assert!(tangent.contains_attribute(Mesh::ATTRIBUTE_TANGENT));
        for attribute in [
            Mesh::ATTRIBUTE_POSITION,
            Mesh::ATTRIBUTE_NORMAL,
            Mesh::ATTRIBUTE_UV_0,
            Mesh::ATTRIBUTE_COLOR,
        ] {
            assert_eq!(plain.attribute(attribute), tangent.attribute(attribute));
        }
    }

    #[test]
    fn layout_variants_cannot_alias_in_any_material_pipeline_cache() {
        let plain = material_key(false);
        let tangent = material_key(true);
        assert_ne!(plain, tangent);
        let mut pipelines = ClusterPipelines::default();
        pipelines
            .shadow_mask
            .insert(plain.clone(), CachedRenderPipelineId::INVALID);
        pipelines.prepass.insert(plain.clone(), None);
        pipelines
            .render
            .insert((plain.clone(), false), CachedRenderPipelineId::INVALID);
        pipelines
            .render
            .insert((plain.clone(), true), CachedRenderPipelineId::INVALID);
        pipelines.far_layout_refusals.insert(plain);
        assert!(!pipelines.shadow_mask.contains_key(&tangent));
        assert!(!pipelines.prepass.contains_key(&tangent));
        assert!(!pipelines.render.contains_key(&(tangent.clone(), false)));
        assert!(!pipelines.render.contains_key(&(tangent.clone(), true)));
        assert!(!pipelines.far_layout_refusals.contains(&tangent));
    }

    #[test]
    fn material_requirements_cannot_invent_a_stock_tangent_attribute() {
        use super::super::material_geometry::GeometryRequirements;
        for authored in [false, true] {
            for required in [false, true] {
                let geometry = GeometryRequirements {
                    supported: true,
                    tangents: required,
                };
                assert_eq!(
                    tangent_layout(authored, geometry),
                    (!required || authored).then_some(authored)
                );
                assert_eq!(
                    tangent_layout(
                        authored,
                        GeometryRequirements {
                            supported: false,
                            ..geometry
                        }
                    ),
                    None
                );
            }
        }
    }

    #[test]
    fn extraction_reads_the_actual_standard_snow_and_terrain_bases() {
        use crate::cluster::terrain::ClusterTerrainMaterial;
        let mut standard = Assets::<StandardMaterial>::default();
        let mut snow = Assets::<SnowMaterial>::default();
        let mut terrain = Assets::<ClusterTerrainMaterial>::default();
        let base = StandardMaterial {
            depth_map: Some(Handle::default()),
            ..default()
        };
        let standard_handle = standard.add(base.clone());
        let snow_handle = snow.add(SnowMaterial {
            base: base.clone(),
            extension: default(),
        });
        let terrain_handle = terrain.add(ClusterTerrainMaterial {
            base,
            extension: default(),
        });
        for id in [
            standard_handle.id().untyped(),
            snow_handle.id().untyped(),
            terrain_handle.id().untyped(),
        ] {
            let requirements =
                material_geometry(id, &standard, Some(&snow), Some(&terrain)).unwrap();
            assert!(requirements.supported && requirements.tangents);
        }
        standard.get_mut(&standard_handle).unwrap().depth_map = None;
        assert!(
            !material_geometry(
                standard_handle.id().untyped(),
                &standard,
                Some(&snow),
                Some(&terrain)
            )
            .unwrap()
            .tangents
        );
        standard.remove(standard_handle.id());
        assert!(
            material_geometry(
                standard_handle.id().untyped(),
                &standard,
                Some(&snow),
                Some(&terrain)
            )
            .is_none()
        );
    }

    #[test]
    fn generated_tangent_memory_does_not_change_source_layout_classification() {
        let mesh = ClusterMeshAsset::from_bytes(
            include_bytes!("../../../shared/tests/data/cluster/fixture.mcl").to_vec(),
        )
        .unwrap();
        assert_eq!(mesh.flags & shared::cluster::FLAG_TANGENTS, 0);
        mesh.prepare_tangents();
        assert!(mesh.tangents_usable());
        assert_eq!(mesh.flags & shared::cluster::FLAG_TANGENTS, 0);
        assert!(
            !mesh
                .source_mesh()
                .contains_attribute(Mesh::ATTRIBUTE_TANGENT)
        );
    }

    #[test]
    fn a_material_shared_by_both_source_layouts_requires_separate_batches() {
        let plain = ClusterMeshAsset::from_bytes(
            include_bytes!("../../../shared/tests/data/cluster/fixture.mcl").to_vec(),
        )
        .unwrap();
        let authored = super::super::assets::authored_tangent_fixture();
        let mut scene = ClusterScene {
            certificate: None,
            terrain_blocks: Vec::new(),
            meshes: vec![plain, authored],
            instances: vec![
                ClusterInstance {
                    identity: ClusterInstanceIdentity::Fixture,
                    mesh: 0,
                    batch: 0,
                    world_from_local: Mat4::IDENTITY,
                    source_worldspace: 0,
                },
                ClusterInstance {
                    identity: ClusterInstanceIdentity::Fixture,
                    mesh: 1,
                    batch: 1,
                    world_from_local: Mat4::IDENTITY,
                    source_worldspace: 0,
                },
            ],
            origin_offset: Vec3::ZERO,
            blocks: Vec::new(),
        };
        assert_eq!(batch_source_tangents(&scene, 2).unwrap(), [false, true]);
        scene.instances[1].batch = 0;
        assert_eq!(
            batch_source_tangents(&scene, 1).unwrap_err(),
            "cluster material batch mixes authored tangent layouts"
        );
    }

    #[test]
    fn real_extraction_updates_requirements_on_events_and_rebinds() {
        use bevy::render::MainWorld;
        let mut main = MainWorld::default();
        main.init_resource::<Assets<StandardMaterial>>();
        main.init_resource::<Messages<AssetEvent<StandardMaterial>>>();
        main.init_resource::<Messages<AssetEvent<SnowMaterial>>>();
        main.init_resource::<ClusterBatchHandles>();
        let handle = main
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        main.resource_mut::<ClusterBatchHandles>()
            .set(0, handle.clone());
        let id = handle.id().untyped();
        let mut render = World::new();
        render.init_resource::<ClusterBatchMaterials>();
        render.insert_resource(main);
        let mut schedule = Schedule::new(ExtractSchedule);
        schedule.add_systems(extract_batch_materials);
        schedule.run(&mut render);
        assert!(!render.resource::<ClusterBatchMaterials>().geometry[&id].tangents);
        let version = render.resource::<ClusterBatchMaterials>().version;
        // Stable extraction does not invalidate unrelated pipelines.
        render.resource_mut::<MainWorld>().increment_change_tick();
        schedule.run(&mut render);
        assert_eq!(render.resource::<ClusterBatchMaterials>().version, version);
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            main.resource_mut::<Assets<StandardMaterial>>()
                .get_mut(&handle)
                .unwrap()
                .anisotropy_strength = 0.5;
            main.resource_mut::<Messages<AssetEvent<StandardMaterial>>>()
                .write(AssetEvent::Modified { id: handle.id() });
        }
        schedule.run(&mut render);
        assert!(render.resource::<ClusterBatchMaterials>().geometry[&id].tangents);
        assert!(render.resource::<ClusterBatchMaterials>().version > version);
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            main.resource_mut::<Assets<StandardMaterial>>()
                .remove(handle.id());
            main.resource_mut::<Messages<AssetEvent<StandardMaterial>>>()
                .write(AssetEvent::Removed { id: handle.id() });
        }
        schedule.run(&mut render);
        assert!(
            !render
                .resource::<ClusterBatchMaterials>()
                .geometry
                .contains_key(&id)
        );
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            let replacement = main
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial::default());
            main.resource_mut::<ClusterBatchHandles>()
                .set(0, replacement);
        }
        schedule.run(&mut render);
        let out = render.resource::<ClusterBatchMaterials>();
        let replacement = out.ids[0].unwrap();
        assert_ne!(replacement, id);
        assert!(!out.geometry[&replacement].tangents);
    }

    #[test]
    fn snow_and_optional_terrain_events_refresh_the_extended_material_bases() {
        use crate::cluster::terrain::ClusterTerrainMaterial;
        use bevy::render::MainWorld;
        let mut main = MainWorld::default();
        main.init_resource::<Assets<StandardMaterial>>();
        main.init_resource::<Assets<SnowMaterial>>();
        main.init_resource::<Assets<ClusterTerrainMaterial>>();
        main.init_resource::<Messages<AssetEvent<StandardMaterial>>>();
        main.init_resource::<Messages<AssetEvent<SnowMaterial>>>();
        main.init_resource::<Messages<AssetEvent<ClusterTerrainMaterial>>>();
        main.init_resource::<ClusterBatchHandles>();
        let snow = main
            .resource_mut::<Assets<SnowMaterial>>()
            .add(SnowMaterial::default());
        let terrain = main
            .resource_mut::<Assets<ClusterTerrainMaterial>>()
            .add(ClusterTerrainMaterial::default());
        main.resource_mut::<ClusterBatchHandles>()
            .set(0, snow.clone());
        main.resource_mut::<ClusterBatchHandles>()
            .set(1, terrain.clone());
        let mut render = World::new();
        render.init_resource::<ClusterBatchMaterials>();
        render.insert_resource(main);
        let mut schedule = Schedule::new(ExtractSchedule);
        schedule.add_systems(extract_batch_materials);
        schedule.run(&mut render);
        let before = render.resource::<ClusterBatchMaterials>().version;
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            main.resource_mut::<Assets<SnowMaterial>>()
                .get_mut(&snow)
                .unwrap()
                .base
                .normal_map_texture = Some(Handle::default());
            main.resource_mut::<Messages<AssetEvent<SnowMaterial>>>()
                .write(AssetEvent::Modified { id: snow.id() });
            main.resource_mut::<Assets<ClusterTerrainMaterial>>()
                .get_mut(&terrain)
                .unwrap()
                .base
                .depth_map = Some(Handle::default());
            main.resource_mut::<Messages<AssetEvent<ClusterTerrainMaterial>>>()
                .write(AssetEvent::Modified { id: terrain.id() });
        }
        schedule.run(&mut render);
        let out = render.resource::<ClusterBatchMaterials>();
        assert!(out.version > before);
        assert!(out.geometry[&snow.id().untyped()].tangents);
        assert!(out.geometry[&terrain.id().untyped()].tangents);
    }

    #[test]
    fn a_pending_material_arrival_is_reconsidered_without_a_handle_change() {
        use bevy::render::MainWorld;
        let mut main = MainWorld::default();
        main.init_resource::<Assets<StandardMaterial>>();
        main.init_resource::<Messages<AssetEvent<StandardMaterial>>>();
        main.init_resource::<Messages<AssetEvent<SnowMaterial>>>();
        main.init_resource::<ClusterBatchHandles>();
        let pending = main.resource::<Assets<StandardMaterial>>().reserve_handle();
        main.resource_mut::<ClusterBatchHandles>()
            .set(0, pending.clone());
        let mut render = World::new();
        render.init_resource::<ClusterBatchMaterials>();
        render.insert_resource(main);
        let mut schedule = Schedule::new(ExtractSchedule);
        schedule.add_systems(extract_batch_materials);
        schedule.run(&mut render);
        assert!(
            !render
                .resource::<ClusterBatchMaterials>()
                .geometry
                .contains_key(&pending.id().untyped())
        );
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            main.resource_mut::<Assets<StandardMaterial>>()
                .insert(
                    pending.id(),
                    StandardMaterial {
                        depth_map: Some(Handle::default()),
                        ..default()
                    },
                )
                .unwrap();
            main.resource_mut::<Messages<AssetEvent<StandardMaterial>>>()
                .write(AssetEvent::Added { id: pending.id() });
        }
        schedule.run(&mut render);
        let out = render.resource::<ClusterBatchMaterials>();
        assert_eq!(out.ids[0], Some(pending.id().untyped()));
        assert!(out.geometry[&pending.id().untyped()].tangents);
    }
    /// Unchanged frames move nothing; a changed handle or a material event moves the version by
    /// one and names exactly the batches it concerns, which is all the batch walk takes up again.
    #[test]
    fn material_changes_name_only_their_batches() {
        use bevy::render::MainWorld;
        let mut main = MainWorld::default();
        main.init_resource::<Assets<StandardMaterial>>();
        main.init_resource::<Messages<AssetEvent<StandardMaterial>>>();
        main.init_resource::<Messages<AssetEvent<SnowMaterial>>>();
        main.init_resource::<ClusterBatchHandles>();
        let (shared, other, late) = {
            let mut materials = main.resource_mut::<Assets<StandardMaterial>>();
            (
                materials.add(StandardMaterial::default()),
                materials.add(StandardMaterial::default()),
                materials.add(StandardMaterial::default()),
            )
        };
        {
            let mut handles = main.resource_mut::<ClusterBatchHandles>();
            handles.set(0, shared.clone());
            handles.set(1, other.clone());
            handles.set(2, shared.clone());
        }
        let mut render = World::new();
        render.init_resource::<ClusterBatchMaterials>();
        render.insert_resource(main);
        let mut schedule = Schedule::new(ExtractSchedule);
        schedule.add_systems(extract_batch_materials);
        schedule.run(&mut render);
        let first = render.resource::<ClusterBatchMaterials>().version;
        assert_eq!(
            render.resource::<ClusterBatchMaterials>().changed_batches,
            [0, 1, 2]
        );
        for _ in 0..100 {
            render.resource_mut::<MainWorld>().increment_change_tick();
            schedule.run(&mut render);
        }
        assert_eq!(render.resource::<ClusterBatchMaterials>().version, first);
        // One batch is given another material: one step, naming that batch only.
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            main.resource_mut::<ClusterBatchHandles>()
                .set(1, late.clone());
        }
        schedule.run(&mut render);
        let out = render.resource::<ClusterBatchMaterials>();
        assert_eq!(out.version, first + 1);
        assert_eq!(out.changed_batches, [1]);
        assert!(!out.named.contains_key(&other.id().untyped()));
        assert!(!out.geometry.contains_key(&other.id().untyped()));
        assert!(out.geometry.contains_key(&late.id().untyped()));
        // An event on a material two batches share names both; one on a material no batch
        // uses moves nothing.
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            let mut events = main.resource_mut::<Messages<AssetEvent<StandardMaterial>>>();
            events.write(AssetEvent::Modified { id: shared.id() });
            events.write(AssetEvent::Modified { id: other.id() });
        }
        schedule.run(&mut render);
        let out = render.resource::<ClusterBatchMaterials>();
        assert_eq!(out.version, first + 2);
        assert_eq!(out.changed_batches, [0, 2]);
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            main.resource_mut::<Messages<AssetEvent<StandardMaterial>>>()
                .write(AssetEvent::Modified { id: other.id() });
        }
        schedule.run(&mut render);
        assert_eq!(
            render.resource::<ClusterBatchMaterials>().version,
            first + 2
        );
    }

    #[test]
    fn cleared_and_shortened_material_tables_drop_every_old_binding_and_dependency() {
        use bevy::render::MainWorld;
        let mut main = MainWorld::default();
        main.init_resource::<Assets<StandardMaterial>>();
        main.init_resource::<Messages<AssetEvent<StandardMaterial>>>();
        main.init_resource::<Messages<AssetEvent<SnowMaterial>>>();
        main.init_resource::<ClusterBatchHandles>();
        let material = main
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        main.resource_mut::<ClusterBatchHandles>()
            .set(0, material.clone());
        let mut render = World::new();
        render.init_resource::<ClusterBatchMaterials>();
        render.insert_resource(main);
        let mut schedule = Schedule::new(ExtractSchedule);
        schedule.add_systems(extract_batch_materials);
        schedule.run(&mut render);
        let initial = render.resource::<ClusterBatchMaterials>().version;
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            main.resource_mut::<ClusterBatchHandles>().handles[0] = None;
        }
        schedule.run(&mut render);
        let out = render.resource::<ClusterBatchMaterials>();
        assert_eq!(out.ids, [None]);
        assert!(out.named.is_empty() && out.geometry.is_empty());
        assert!(out.version > initial);
        {
            let mut main = render.resource_mut::<MainWorld>();
            main.increment_change_tick();
            main.resource_mut::<ClusterBatchHandles>().handles.clear();
        }
        schedule.run(&mut render);
        assert!(render.resource::<ClusterBatchMaterials>().ids.is_empty());
    }

    #[test]
    fn prepass_refusal_is_stable_until_a_material_event_changes_its_version() {
        let mut pipelines = ClusterPipelines::default();
        let key = material_key(false);
        pipelines.prepass.insert(key.clone(), None);
        pipelines.refresh_prepass_refusals(0);
        assert!(pipelines.prepass.contains_key(&key));
        pipelines.refresh_prepass_refusals(1);
        assert!(!pipelines.prepass.contains_key(&key));
    }
}
