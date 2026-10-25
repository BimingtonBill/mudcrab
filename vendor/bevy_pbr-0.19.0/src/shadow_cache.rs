//! Exact directional shadow-map retention. Producers own invalidation; settled
//! checks compare view inputs and revisions, never walk the caster population.

use core::{any::TypeId, marker::PhantomData};
use std::sync::{Mutex, MutexGuard};

use bevy_asset::UntypedAssetId;
use bevy_diagnostic::FrameCount;
use bevy_ecs::{
    prelude::*,
    query::ROQueryItem,
    system::{lifetimeless::SRes, SystemParamItem},
};
use bevy_platform::collections::{HashMap, HashSet};
use bevy_render::{
    erased_render_asset::ErasedRenderAssets,
    mesh::{allocator::MeshAllocator, RenderMesh},
    render_asset::RenderAssets,
    render_phase::{
        DrawFunctions, RenderCommand, RenderCommandResult, RenderCommandState, TrackedRenderPass,
    },
    render_resource::{Extent3d, PipelineCache, TextureId},
    sync_world::{MainEntity, MainEntityHashMap},
    texture::{CachedTexture, GpuImage},
    view::{ExtractedView, RetainedViewEntity},
};

use crate::{PreparedMaterial, RenderMeshInstances, Shadow, StandardMaterial};

#[path = "shadow_cache_parity.rs"]
pub(crate) mod parity;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic during dependency publication cannot safely resume map reuse.
    mutex.lock().expect("Shadow dependency state poisoned")
}

fn advance(revision: &mut u64) {
    *revision = revision.checked_add(1).expect("Shadow revision exhausted");
}

fn gpu_texture_contents_untracked(usage: bevy_render::render_resource::TextureUsages) -> bool {
    use bevy_render::render_resource::TextureUsages;
    usage.intersects(TextureUsages::RENDER_ATTACHMENT | TextureUsages::STORAGE_BINDING)
}

pub(crate) fn untracked_texture_bindings(
    bindings: &bevy_render::render_resource::BindingResources,
) -> bool {
    bindings.iter().any(|(_, resource)| match resource {
        bevy_render::render_resource::OwnedBindingResource::TextureView(_, view) => {
            gpu_texture_contents_untracked(view.texture().usage())
        }
        _ => false,
    })
}

#[cfg(test)]
mod texture_usage_tests {
    use super::gpu_texture_contents_untracked;
    use bevy_render::render_resource::TextureUsages;

    #[test]
    fn managed_uploads_remain_eligible_but_gpu_written_bindings_refuse() {
        let managed = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
        assert!(!gpu_texture_contents_untracked(managed));
        for writable in [
            TextureUsages::RENDER_ATTACHMENT,
            TextureUsages::STORAGE_BINDING,
        ] {
            assert!(gpu_texture_contents_untracked(managed | writable));
        }
    }
}

#[derive(Clone, Copy)]
struct ExternalInput {
    frame: u32,
    revision: u64,
    complete: bool,
}

#[derive(Default)]
struct ExternalMaterials {
    frame: u32,
    initialized: bool,
    materials: HashSet<UntypedAssetId>,
    untracked: HashSet<UntypedAssetId>,
    unknown: HashSet<UntypedAssetId>,
    revisions: HashMap<UntypedAssetId, (u64, u64)>,
    revision: u64,
}

#[derive(Default)]
struct ExternalMaterialInputs {
    views: HashMap<RetainedViewEntity, ExternalMaterials>,
    material_views: HashMap<UntypedAssetId, HashSet<RetainedViewEntity>>,
    type_materials: HashMap<TypeId, HashSet<UntypedAssetId>>,
    pending: HashSet<UntypedAssetId>,
}

struct CertifiedShadowDraw {
    command: TypeId,
    views: HashMap<RetainedViewEntity, ExternalInput>,
}

#[derive(Default)]
struct ShadowDrawProducers {
    // The two cluster shadow command tuples. This is a closed certificate
    // budget, not a general-purpose custom-producer declaration API.
    certified: [Option<CertifiedShadowDraw>; 2],
    declarations_revision: u64,
    registry_revision: Option<u64>,
    unknown: usize,
    active: [bool; 2],
    #[cfg(test)]
    classifications: u64,
}

type DrawProducerSnapshot = (u64, u64, u64, u64);

/// Current-frame dependency certificates for non-stock shadow producers.
///
/// Publish after culling, including frames that reuse the cull output. The
/// revision must cover the exact selected geometry, transforms, materials,
/// shader inputs, and resource replacements. `complete` also certifies that all
/// external draw pipelines and bindings are ready. Empty content needs an
/// explicit publication. Missing or old certificates refuse reuse.
#[derive(Resource, Default)]
pub struct ShadowMapExternalInputs(
    Mutex<HashMap<RetainedViewEntity, ExternalInput>>,
    Mutex<ExternalMaterialInputs>,
    Mutex<ShadowDrawProducers>,
);

impl ShadowMapExternalInputs {
    /// Certifies one external shadow command's complete dependencies for this
    /// view and frame. `C` is the inner command registered as
    /// `ShadowCacheTracking<C>`, not the wrapper itself.
    ///
    /// Publish after all of the producer's queue/input mutations, including
    /// insertion, removal, an empty output, and unchanged-output frames. The
    /// revision must cover every geometry, binding, uniform, shader, and resource
    /// input that can change this command's shadow output. Missing, incomplete,
    /// or stale certificates refuse reuse globally while this draw is registered.
    /// Wrapping or publishing the aggregate view certificate alone does not
    /// authorize a custom command. Raw `Draw` implementations cannot be certified.
    /// Only two distinct external command types can be certified (the opaque
    /// and masked cluster draws); any further registered producer refuses reuse.
    pub fn publish_draw<C: RenderCommand<Shadow> + 'static>(
        &self,
        view: RetainedViewEntity,
        frame: u32,
        revision: u64,
        complete: bool,
    ) {
        let command = TypeId::of::<RenderCommandState<Shadow, ShadowCacheTracking<C>>>();
        let mut producers = lock(&self.2);
        let slot = producers
            .certified
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|slot| slot.command == command))
            .or_else(|| producers.certified.iter().position(Option::is_none));
        let Some(slot) = slot else {
            // No declaration for extra types: the registry gate refuses them.
            return;
        };
        if producers.certified[slot].is_none() {
            producers.certified[slot] = Some(CertifiedShadowDraw {
                command,
                views: HashMap::default(),
            });
            advance(&mut producers.declarations_revision);
            producers.registry_revision = None;
        }
        if let Some(slot) = &mut producers.certified[slot] {
            slot.views.insert(
                view,
                ExternalInput {
                    frame,
                    revision,
                    complete,
                },
            );
        }
    }

    /// Marks freshness, retaining the previous plan's exact material closure.
    /// Invoke before the existing queue's unchanged-plan early return.
    pub fn begin_materials(&self, view: RetainedViewEntity, frame: u32) {
        lock(&self.1).views.entry(view).or_default().frame = frame;
    }

    /// Starts a new material closure only when the queue plan changes. Record
    /// its materials with `observe_material`, including an explicit empty plan.
    pub fn reset_materials(&self, view: RetainedViewEntity, frame: u32) {
        let mut inputs = lock(&self.1);
        if let Some(state) = inputs.views.get_mut(&view)
            && state.initialized
            && state.materials.is_empty()
        {
            // Empty publication is also the GPU-absent path. Refresh freshness
            // without changing exact content identity on every stock-only frame.
            state.frame = frame;
            return;
        }
        remove_external_view(&mut inputs, view);
        let state = inputs.views.entry(view).or_default();
        state.frame = frame;
        state.initialized = true;
        advance(&mut state.revision);
    }

    /// Records an already-fetched material in the existing queue loop. This is
    /// a constant-time flag check; it does not traverse textures or materials.
    pub fn observe_material(
        &self,
        view: RetainedViewEntity,
        frame: u32,
        id: UntypedAssetId,
        material: &PreparedMaterial,
    ) {
        let mut inputs = lock(&self.1);
        if let Some(state) = inputs
            .views
            .get_mut(&view)
            .filter(|inputs| inputs.frame == frame && inputs.initialized)
        {
            state.materials.insert(id);
            state.unknown.insert(id);
            if material.shadow_cache_untracked_texture {
                state.untracked.insert(id);
            }
            inputs.material_views.entry(id).or_default().insert(view);
            inputs
                .type_materials
                .entry(id.type_id())
                .or_default()
                .insert(id);
            inputs.pending.insert(id);
        }
    }

    /// Publishes the aggregate certificate for all non-stock producers of a view.
    pub fn publish(&self, view: RetainedViewEntity, frame: u32, revision: u64, complete: bool) {
        lock(&self.0).insert(
            view,
            ExternalInput {
                frame,
                revision,
                complete,
            },
        );
    }

    pub(crate) fn retain(&self, live: &HashSet<RetainedViewEntity>) {
        lock(&self.0).retain(|view, _| live.contains(view));
        for slot in lock(&self.2).certified.iter_mut().flatten() {
            slot.views.retain(|view, _| live.contains(view));
        }
        let mut materials = lock(&self.1);
        let expired: Vec<_> = materials
            .views
            .keys()
            .copied()
            .filter(|view| !live.contains(view))
            .collect();
        for view in expired {
            remove_external_view(&mut materials, view);
            materials.views.remove(&view);
        }
    }
}

fn remove_external_view(inputs: &mut ExternalMaterialInputs, view: RetainedViewEntity) {
    if let Some(state) = inputs.views.get_mut(&view) {
        for material in state.materials.drain() {
            if let Some(views) = inputs.material_views.get_mut(&material) {
                views.remove(&view);
                if views.is_empty() {
                    inputs.material_views.remove(&material);
                    if let Some(materials) = inputs.type_materials.get_mut(&material.type_id()) {
                        materials.remove(&material);
                    }
                }
            }
        }
        state.untracked.clear();
        state.unknown.clear();
        state.revisions.clear();
    }
}

fn route_external_material_change(
    inputs: &mut ExternalMaterialInputs,
    material: UntypedAssetId,
    revision: Option<(u64, u64)>,
    untracked: bool,
) {
    let ExternalMaterialInputs {
        views,
        material_views,
        ..
    } = inputs;
    let Some(affected) = material_views.get(&material) else {
        return;
    };
    for view in affected {
        let Some(state) = views.get_mut(view) else {
            continue;
        };
        if untracked {
            state.untracked.insert(material);
        } else {
            state.untracked.remove(&material);
        }
        if let Some(revision) = revision {
            if state.revisions.get(&material) != Some(&revision) {
                state.revisions.insert(material, revision);
                advance(&mut state.revision);
            }
            state.unknown.remove(&material);
        } else {
            state.revisions.remove(&material);
            state.unknown.insert(material);
        }
    }
}

/// Declared dependencies of custom stock shadow shaders.
///
/// StandardMaterial is known automatically. Custom shader types fail closed
/// until declared. Asset-specific revisions cover animation or external
/// storage/uniform inputs, leaving unrelated cascades reusable.
#[derive(Resource)]
pub struct ShadowCacheMaterialDependencies {
    types: HashSet<TypeId>,
    assets: HashMap<UntypedAssetId, u64>,
    changed_assets: HashSet<UntypedAssetId>,
    changed_types: HashSet<TypeId>,
}

impl Default for ShadowCacheMaterialDependencies {
    fn default() -> Self {
        Self {
            types: [TypeId::of::<StandardMaterial>()].into_iter().collect(),
            assets: HashMap::default(),
            changed_assets: HashSet::default(),
            changed_types: HashSet::default(),
        }
    }
}

impl ShadowCacheMaterialDependencies {
    /// Declares no shadow dependencies beyond the ordinary mesh, image,
    /// prepared-material, and view producers tracked by PBR.
    pub fn declare_static<M: 'static>(&mut self) {
        if self.types.insert(TypeId::of::<M>()) {
            self.changed_types.insert(TypeId::of::<M>());
        }
    }

    /// Declares one asset's additional dependencies and their current revision.
    /// Call from their mutation owner before the shadow pass.
    pub fn set_revision(&mut self, material: UntypedAssetId, revision: u64) {
        if self.assets.get(&material) != Some(&revision) {
            self.assets.insert(material, revision);
            self.changed_assets.insert(material);
        }
    }

    pub(crate) fn dependency(&self, material: UntypedAssetId) -> Option<u64> {
        self.assets
            .get(&material)
            .copied()
            .or_else(|| self.types.contains(&material.type_id()).then_some(0))
    }
}

#[derive(Default)]
struct StockView {
    revision: u64,
    casters: MainEntityHashMap<UntypedAssetId>,
    materials: HashMap<UntypedAssetId, usize>,
    material_inputs: HashMap<UntypedAssetId, (u64, u64)>,
    material_revision: u64,
    unknown_materials: HashSet<UntypedAssetId>,
    untracked_textures: HashSet<UntypedAssetId>,
    incomplete: bool,
}

#[derive(Default)]
struct StockInputs {
    views: HashMap<RetainedViewEntity, StockView>,
    caster_views: MainEntityHashMap<HashSet<RetainedViewEntity>>,
    material_views: HashMap<UntypedAssetId, HashSet<RetainedViewEntity>>,
    type_materials: HashMap<TypeId, HashSet<UntypedAssetId>>,
    morph_weights: MainEntityHashMap<Vec<u32>>,
}

/// Diagnostic names only. Registration never declares shadow dependencies.
#[derive(Resource, Default)]
pub(crate) struct ShadowMaterialTypeNames(pub(crate) HashMap<TypeId, &'static str>);

impl ShadowMaterialTypeNames {
    pub(crate) fn register<M: bevy_reflect::TypePath + 'static>(&mut self) {
        self.0.insert(TypeId::of::<M>(), M::type_path());
    }
}

/// Producer-owned stock caster revisions, updated in existing extraction/queue
/// delta loops. The reverse index visits only views affected by a mutation.
#[derive(Resource, Default)]
#[doc(hidden)]
pub struct ShadowStockInputs(Mutex<StockInputs>);

impl ShadowStockInputs {
    pub(crate) fn caster_changed(&self, entity: MainEntity) {
        let mut inputs = lock(&self.0);
        let StockInputs {
            views,
            caster_views,
            ..
        } = &mut *inputs;
        if let Some(affected) = caster_views.get(&entity) {
            for view in affected {
                if let Some(view) = views.get_mut(view) {
                    advance(&mut view.revision);
                }
            }
        }
    }

    pub(crate) fn set_caster(
        &self,
        view: RetainedViewEntity,
        entity: MainEntity,
        material: UntypedAssetId,
        prepared_revision: Option<u64>,
        extra_revision: Option<u64>,
        untracked_texture: bool,
    ) {
        let mut inputs = lock(&self.0);
        let stock = inputs.views.entry(view).or_default();
        if stock.casters.get(&entity) == Some(&material) {
            // Requeue can change pipeline, geometry, layers, or uniform index.
            advance(&mut stock.revision);
            return;
        }
        let old_material = stock.casters.insert(entity, material);
        let mut remove_old_material = None;
        if let Some(old_material) = old_material {
            if let Some(count) = stock.materials.get_mut(&old_material) {
                *count -= 1;
                if *count == 0 {
                    stock.materials.remove(&old_material);
                    stock.material_inputs.remove(&old_material);
                    stock.unknown_materials.remove(&old_material);
                    stock.untracked_textures.remove(&old_material);
                    remove_old_material = Some(old_material);
                }
            }
        }
        *stock.materials.entry(material).or_default() += 1;
        update_material(
            stock,
            material,
            prepared_revision.zip(extra_revision),
            untracked_texture,
        );
        advance(&mut stock.revision);
        if let Some(old_material) = remove_old_material {
            remove_material_view(&mut inputs, old_material, view);
        }
        inputs.caster_views.entry(entity).or_default().insert(view);
        inputs
            .material_views
            .entry(material)
            .or_default()
            .insert(view);
        inputs
            .type_materials
            .entry(material.type_id())
            .or_default()
            .insert(material);
    }

    pub(crate) fn remove_caster(&self, view: RetainedViewEntity, entity: MainEntity) {
        let mut inputs = lock(&self.0);
        let Some(stock) = inputs.views.get_mut(&view) else {
            return;
        };
        let Some(material) = stock.casters.remove(&entity) else {
            return;
        };
        let mut remove_material = false;
        if let Some(count) = stock.materials.get_mut(&material) {
            *count -= 1;
            if *count == 0 {
                stock.materials.remove(&material);
                stock.material_inputs.remove(&material);
                stock.unknown_materials.remove(&material);
                stock.untracked_textures.remove(&material);
                remove_material = true;
            }
        }
        advance(&mut stock.revision);
        if remove_material {
            remove_material_view(&mut inputs, material, view);
        }
        if let Some(views) = inputs.caster_views.get_mut(&entity) {
            views.remove(&view);
            if views.is_empty() {
                inputs.caster_views.remove(&entity);
            }
        }
    }

    pub(crate) fn observe_morph(&self, entity: MainEntity, weights: &[f32], source_changed: bool) {
        let changed = {
            let mut inputs = lock(&self.0);
            // The extraction producer owns change ticks. Settled inputs need
            // only a membership lookup, never another walk over weight bytes.
            if !source_changed && inputs.morph_weights.contains_key(&entity) {
                return;
            }
            let old = inputs.morph_weights.entry(entity).or_default();
            if old
                .iter()
                .copied()
                .eq(weights.iter().map(|weight| weight.to_bits()))
            {
                false
            } else {
                old.clear();
                old.extend(weights.iter().map(|weight| weight.to_bits()));
                true
            }
        };
        if changed {
            self.caster_changed(entity);
        }
    }

    pub(crate) fn remove_morph(&self, entity: MainEntity) {
        let removed = lock(&self.0).morph_weights.remove(&entity).is_some();
        if removed {
            self.caster_changed(entity);
        }
    }

    pub(crate) fn retain(&self, live: &HashSet<RetainedViewEntity>) {
        let mut inputs = lock(&self.0);
        let StockInputs {
            views,
            caster_views,
            material_views,
            type_materials,
            ..
        } = &mut *inputs;
        views.retain(|view, stock| {
            if live.contains(view) {
                return true;
            }
            for entity in stock.casters.keys() {
                if let Some(affected) = caster_views.get_mut(entity) {
                    affected.remove(view);
                    if affected.is_empty() {
                        caster_views.remove(entity);
                    }
                }
            }
            for material in stock.materials.keys() {
                if let Some(affected) = material_views.get_mut(material) {
                    affected.remove(view);
                    if affected.is_empty() {
                        material_views.remove(material);
                        if let Some(materials) = type_materials.get_mut(&material.type_id()) {
                            materials.remove(material);
                        }
                    }
                }
            }
            false
        });
    }

    pub(crate) fn begin_view(&self, view: RetainedViewEntity) {
        lock(&self.0).views.entry(view).or_default().incomplete = false;
    }

    pub(crate) fn refuse_view(&self, view: RetainedViewEntity) {
        lock(&self.0).views.entry(view).or_default().incomplete = true;
    }

    fn snapshot(&self, view: RetainedViewEntity) -> Result<(u64, u64), Refusal> {
        let inputs = lock(&self.0);
        let Some(stock) = inputs.views.get(&view) else {
            return Ok((0, 0));
        };
        if stock.incomplete {
            return Err(Refusal::PendingStock);
        }
        if !stock.unknown_materials.is_empty() {
            return Err(Refusal::UnknownMaterial);
        }
        if !stock.untracked_textures.is_empty() {
            return Err(Refusal::UntrackedTexture);
        }
        Ok((stock.revision, stock.material_revision))
    }
}

fn remove_material_view(
    inputs: &mut StockInputs,
    material: UntypedAssetId,
    view: RetainedViewEntity,
) {
    if let Some(views) = inputs.material_views.get_mut(&material) {
        views.remove(&view);
        if views.is_empty() {
            inputs.material_views.remove(&material);
            if let Some(materials) = inputs.type_materials.get_mut(&material.type_id()) {
                materials.remove(&material);
            }
        }
    }
}

fn update_material(
    stock: &mut StockView,
    material: UntypedAssetId,
    revision: Option<(u64, u64)>,
    untracked_texture: bool,
) {
    if untracked_texture {
        stock.untracked_textures.insert(material);
    } else {
        stock.untracked_textures.remove(&material);
    }
    if let Some(revision) = revision {
        if stock.material_inputs.get(&material) != Some(&revision) {
            stock.material_inputs.insert(material, revision);
            advance(&mut stock.material_revision);
        }
        stock.unknown_materials.remove(&material);
    } else {
        stock.material_inputs.remove(&material);
        stock.unknown_materials.insert(material);
    }
}

fn route_material_change(
    inputs: &mut StockInputs,
    material: UntypedAssetId,
    revision: Option<(u64, u64)>,
    untracked_texture: bool,
) {
    let StockInputs {
        views,
        material_views,
        ..
    } = inputs;
    let Some(affected) = material_views.get(&material) else {
        return;
    };
    for view in affected {
        if let Some(view) = views.get_mut(view) {
            update_material(view, material, revision, untracked_texture);
        }
    }
}

/// Routes producer mutation IDs only to cascades that use those materials.
/// There is no walk over unchanged materials or caster entities.
pub(crate) fn apply_shadow_material_changes(
    mut materials: ResMut<ErasedRenderAssets<PreparedMaterial>>,
    mut declarations: ResMut<ShadowCacheMaterialDependencies>,
    stock_inputs: Res<ShadowStockInputs>,
    external_inputs: Res<ShadowMapExternalInputs>,
) {
    let mut changed = materials.take_changed_assets();
    changed.extend(declarations.changed_assets.drain());
    let mut inputs = lock(&stock_inputs.0);
    let mut external = lock(&external_inputs.1);
    changed.extend(external.pending.drain());
    for material_type in declarations.changed_types.drain() {
        if let Some(materials) = inputs.type_materials.get(&material_type) {
            changed.extend(materials.iter().copied());
        }
        if let Some(materials) = external.type_materials.get(&material_type) {
            changed.extend(materials.iter().copied());
        }
    }
    for material in changed {
        let revision = materials
            .asset_revision(material)
            .zip(declarations.dependency(material));
        let untracked_texture = materials
            .get(material)
            .is_none_or(|material| material.shadow_cache_untracked_texture);
        route_material_change(&mut inputs, material, revision, untracked_texture);
        route_external_material_change(&mut external, material, revision, untracked_texture);
    }
}

/// Persistent physical directional/spot array, reset on device recovery.
#[derive(Resource, Default)]
#[doc(hidden)]
pub struct RetainedDirectionalShadowTexture {
    pub(crate) texture: Option<(Extent3d, CachedTexture)>,
}

/// Exact view/light/allocation inputs recorded by `prepare_lights`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirectionalShadowMapInputs {
    pub(crate) texture: TextureId,
    pub(crate) slice: u32,
    pub(crate) matrices: [[u32; 16]; 7],
    pub(crate) light: [u32; 9],
    pub(crate) camera_layers: bevy_camera::visibility::RenderLayers,
    pub(crate) light_layers: bevy_camera::visibility::RenderLayers,
}

/// Exact uniforms of the shadow view, including renderer-extension changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ShadowViewInputs {
    viewport: [u32; 4],
    world_from_view: [u32; 16],
    clip_from_view: [u32; 16],
    clip_from_world: [u32; 16],
    mip_bias: u32,
    lod_origin: [u32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MapKey {
    entity: Entity,
    inputs: DirectionalShadowMapInputs,
    shadow_view: ShadowViewInputs,
    external: u64,
    external_materials: u64,
    stock: (u64, u64),
    geometry: u64,
    meshes: u64,
    images: u64,
    pipelines: u64,
    draw_producers: DrawProducerSnapshot,
}

// Reclassify only on a registry/declaration mutation. Draw storage is read-only
// outside its mutation owner, so this stamp covers insertion, replacement,
// removal, and resource recreation. Actual types cannot be spoofed by `add_with`
// aliases. Settled checks read a cached unknown count and at most two current
// certificates: no registry/phase scan, type checks, or allocation.
fn shadow_draw_producers(
    draws: &DrawFunctions<Shadow>,
    external: &ShadowMapExternalInputs,
    view: RetainedViewEntity,
    frame: u32,
) -> Result<DrawProducerSnapshot, Refusal> {
    let draws = draws.read();
    let registry_revision = draws.draw_functions.revision();
    let mut producers = lock(&external.2);
    if producers.registry_revision != Some(registry_revision) {
        let stock = [
            TypeId::of::<RenderCommandState<Shadow, ShadowCacheTracking<crate::DrawPrepass>>>(),
            TypeId::of::<
                RenderCommandState<Shadow, ShadowCacheTracking<crate::DrawDepthOnlyPrepass>>,
            >(),
        ];
        let mut unknown = 0;
        let mut active = [false; 2];
        for draw in draws.draw_functions.iter() {
            let command = core::any::Any::type_id(draw.as_ref());
            if stock.contains(&command) {
                continue;
            }
            if let Some(slot) = producers
                .certified
                .iter()
                .position(|slot| slot.as_ref().is_some_and(|slot| slot.command == command))
            {
                active[slot] = true;
            } else {
                unknown += 1;
            }
        }
        producers.unknown = unknown;
        producers.active = active;
        producers.registry_revision = Some(registry_revision);
        #[cfg(test)]
        {
            producers.classifications += 1;
        }
    }
    if producers.unknown != 0 {
        return Err(Refusal::UnsupportedProducer);
    }
    let mut revisions = [0; 2];
    for (slot, revision) in revisions.iter_mut().enumerate() {
        if !producers.active[slot] {
            continue;
        }
        let input = producers.certified[slot]
            .as_ref()
            .and_then(|producer| producer.views.get(&view))
            .filter(|input| input.frame == frame && input.complete)
            .ok_or(Refusal::UnsupportedProducer)?;
        *revision = input.revision;
    }
    Ok((
        registry_revision,
        producers.declarations_revision,
        revisions[0],
        revisions[1],
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refusal {
    External,
    UnknownMaterial,
    UnsupportedProducer,
    PendingStock,
    UntrackedTexture,
}

/// Monotonic counters. Reuse skips clear and draw; refusals are separate from
/// ordinary dependency misses and incomplete-command retries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShadowMapCacheCounters {
    pub reused: u64,
    pub rendered: u64,
    pub invalidated: u64,
    pub external_incomplete: u64,
    pub unknown_material: u64,
    pub unsupported_producer: u64,
    pub stock_pending: u64,
    pub incomplete_draw: u64,
    pub forced: u64,
    pub untracked_texture: u64,
    /// Real renders induced by the byte audit, excluded from `reused`.
    pub audit_forced: u64,
}

impl ShadowMapCacheCounters {
    fn begin(&mut self, key: &Result<MapKey, Refusal>, skip: bool, forced: bool) {
        match key {
            Ok(_) if skip => self.reused += 1,
            Ok(_) => self.invalidated += 1,
            Err(Refusal::External) => self.external_incomplete += 1,
            Err(Refusal::UnknownMaterial) => self.unknown_material += 1,
            Err(Refusal::UnsupportedProducer) => self.unsupported_producer += 1,
            Err(Refusal::PendingStock) => self.stock_pending += 1,
            Err(Refusal::UntrackedTexture) => self.untracked_texture += 1,
        }
        if !skip {
            self.rendered += 1;
            self.forced += u64::from(forced);
        }
    }
}

/// Scalar deltas owned by the retained attempt, not by a caster or dependency
/// scan. `rendered` counts view/frame render decisions, including a late pass
/// that revokes early reuse; it is not a count of early and late GPU passes.
struct ShadowViewWindow {
    pass_name: String,
    counters: ShadowMapCacheCounters,
    start_frame: u32,
    last_observed_frame: Option<u32>,
    observed_frames: u32,
    label_changes: u64,
    label_valid: bool,
}

#[derive(Clone, Copy)]
struct ShadowViewWindowReport {
    counters: ShadowMapCacheCounters,
    start_frame: u32,
    end_frame: u32,
    frames: u32,
    observed_frames: u32,
    label_changes: u64,
    label_valid: bool,
    lifecycle_valid: bool,
}

impl ShadowViewWindowReport {
    fn zero_render_proof(&self) -> bool {
        let c = &self.counters;
        self.label_valid
            && self.lifecycle_valid
            && self.observed_frames != 0
            && self.observed_frames == self.frames
            && c.reused == u64::from(self.observed_frames)
            && c.rendered == 0
            && c.invalidated == 0
            && c.external_incomplete == 0
            && c.unknown_material == 0
            && c.unsupported_producer == 0
            && c.stock_pending == 0
            && c.untracked_texture == 0
            && c.incomplete_draw == 0
            && c.forced == 0
            && c.audit_forced == 0
    }
}

impl ShadowViewWindow {
    fn has_pending(&self) -> bool {
        self.observed_frames != 0 || self.counters != ShadowMapCacheCounters::default()
    }

    fn new(frame: u32) -> Self {
        Self {
            pass_name: String::new(),
            counters: ShadowMapCacheCounters::default(),
            start_frame: frame,
            last_observed_frame: None,
            observed_frames: 0,
            label_changes: 0,
            label_valid: true,
        }
    }

    fn observe(&mut self, frame: u32, pass_name: &str) {
        if self.last_observed_frame != Some(frame) {
            self.observed_frames += 1;
            self.last_observed_frame = Some(frame);
        }
        if self.pass_name != pass_name {
            if !self.pass_name.is_empty() {
                self.label_changes += 1;
                self.label_valid = false;
            }
            // Retain storage between windows. No label allocation or copy on
            // an unchanged view, and no temporary String in the render path.
            self.pass_name.clear();
            self.pass_name.push_str(pass_name);
        }
        self.label_valid &= !pass_name.is_empty();
    }

    fn record(&mut self, update: impl FnOnce(&mut ShadowMapCacheCounters)) {
        // Executable semantic RED for the parent, without altering decisions or
        // global diagnostics. This branch and environment access are test-only.
        #[cfg(test)]
        if std::env::var_os("MUDCRAB_SHADOW_VIEW_COUNTERS_RED").is_some_and(|value| value == "1") {
            return;
        }
        update(&mut self.counters);
    }

    fn close(&mut self, frame: u32, lifecycle_valid: bool) -> ShadowViewWindowReport {
        let report = ShadowViewWindowReport {
            counters: core::mem::take(&mut self.counters),
            start_frame: self.start_frame,
            end_frame: frame,
            frames: frame.wrapping_sub(self.start_frame).wrapping_add(1),
            observed_frames: self.observed_frames,
            label_changes: self.label_changes,
            label_valid: self.label_valid,
            lifecycle_valid,
        };
        self.start_frame = frame.wrapping_add(1);
        self.observed_frames = 0;
        self.label_changes = 0;
        self.label_valid = !self.pass_name.is_empty();
        report
    }

    fn report(&mut self, view: RetainedViewEntity, frame: u32, reason: &'static str) {
        let report = self.close(frame, reason == "periodic");
        let c = report.counters;
        tracing::info!(
            main_entity = view.main_entity.id().to_bits(),
            auxiliary_entity = view.auxiliary_entity.id().to_bits(),
            subview_index = view.subview_index,
            pass_name = self.pass_name.as_str(),
            window_start_frame = report.start_frame,
            window_end_frame = report.end_frame,
            frames = report.frames,
            observed_frames = report.observed_frames,
            label_changes = report.label_changes,
            label_valid = report.label_valid,
            lifecycle_valid = report.lifecycle_valid,
            zero_render_proof = report.zero_render_proof(),
            reason,
            reused = c.reused,
            rendered = c.rendered,
            invalidated = c.invalidated,
            external_incomplete = c.external_incomplete,
            unknown_material = c.unknown_material,
            unsupported_producer = c.unsupported_producer,
            stock_pending = c.stock_pending,
            incomplete_draw = c.incomplete_draw,
            forced = c.forced,
            audit_forced = c.audit_forced,
            untracked_texture = c.untracked_texture,
            "shadow map per-view reuse window"
        );
    }
}

struct Attempt {
    frame: u32,
    skip: bool,
    failed: bool,
    early_complete: bool,
    key: Option<MapKey>,
    window: ShadowViewWindow,
}

#[derive(Default)]
struct Maps {
    owners: HashMap<(TextureId, u32), (RetainedViewEntity, MapKey)>,
    attempts: HashMap<RetainedViewEntity, Attempt>,
    counters: ShadowMapCacheCounters,
}

/// Retained maps and their physical last-writer certificates.
#[derive(Resource)]
pub struct ShadowMapCache(Mutex<Maps>, bool, parity::ShadowParityAudit);

/// Opt-in native depth verification. Insert in the render world before
/// `RenderStartup`, including on device recovery. This overrides the legacy
/// shadow audit environment controls; an absent resource preserves them.
///
/// Create `request` after content settles. Its first observation selects one
/// actual render FrameCount, shared through `ShadowMapCache::parity_request_frame`.
/// The audit creates its output directory and enables depth copies before shadow
/// allocation. It contaminates timings and cannot identify a culprit by itself.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ShadowReuseVerification {
    /// Parent directory for uniquely named audit receipts and raw depth bytes.
    pub directory: std::path::PathBuf,
    /// Common sentinel file for paired cull, raster and shadow verification.
    pub request: std::path::PathBuf,
}

impl FromWorld for ShadowMapCache {
    fn from_world(world: &mut World) -> Self {
        // Diagnostic control is read once, never in a per-view cache check.
        Self(
            Mutex::default(),
            std::env::var_os("MUDCRAB_FORCE_SHADOW_RECOMPUTE").is_some_and(|value| value == "1"),
            parity::ShadowParityAudit::from_settings(
                world.get_resource::<ShadowReuseVerification>(),
            ),
        )
    }
}

impl ShadowMapCache {
    /// Shared diagnostic request gate for a paired native cull/shadow audit.
    /// Call before encoding the cull audit, with render-world `FrameCount.0`.
    /// With `ShadowReuseVerification` or `MUDCRAB_GPU_PARITY_REQUEST`, only the
    /// first frame that observes the sentinel is eligible. No shadow audit
    /// configured means no file I/O.
    pub fn parity_request_frame(&self, frame: u32) -> Option<u32> {
        self.2.request_frame(frame)
    }

    pub(crate) fn parity(&self) -> &parity::ShadowParityAudit {
        &self.2
    }

    // A normal early `begin` must have selected reuse before this transition.
    // Keep the same key and command tracking for BOTH early and late passes.
    fn force_audit_render(&self, view: RetainedViewEntity, frame: u32) -> Option<MapKey> {
        let mut maps = lock(&self.0);
        let attempt = maps.attempts.get_mut(&view)?;
        if attempt.frame != frame || !attempt.skip || attempt.failed {
            return None;
        }
        let key = attempt.key.clone()?;
        attempt.skip = false;
        attempt.window.record(|c| {
            c.reused -= 1;
            c.rendered += 1;
            c.audit_forced += 1;
        });
        maps.owners.remove(&(key.inputs.texture, key.inputs.slice));
        maps.counters.reused -= 1;
        maps.counters.rendered += 1;
        maps.counters.audit_forced += 1;
        Some(key)
    }

    // The normal raster already completed. This is a second GPU emission,
    // not another view/frame decision, and does not grant a refused key.
    fn force_fresh_audit_render(&self, view: RetainedViewEntity, frame: u32) -> bool {
        let mut maps = lock(&self.0);
        let Some(attempt) = maps.attempts.get_mut(&view) else {
            return false;
        };
        if attempt.frame != frame || attempt.skip || attempt.failed || !attempt.early_complete {
            return false;
        }
        attempt.early_complete = false;
        attempt.window.record(|c| c.audit_forced += 1);
        maps.counters.audit_forced += 1;
        true
    }

    fn revoke_audit(&self, view: RetainedViewEntity, allocation: (TextureId, u32)) {
        self.incomplete(view);
        lock(&self.0).owners.remove(&allocation);
    }

    /// Reads cumulative counters without scanning maps or casters.
    pub fn counters(&self) -> ShadowMapCacheCounters {
        lock(&self.0).counters
    }

    pub(crate) fn invalidate_physical_maps(&self) {
        let mut maps = lock(&self.0);
        maps.owners.clear();
        for (view, attempt) in &mut maps.attempts {
            if attempt.window.has_pending() {
                attempt
                    .window
                    .report(*view, attempt.frame, "physical_reset");
            }
        }
        maps.attempts.clear();
    }

    pub(crate) fn retain(&self, live: &HashSet<RetainedViewEntity>) {
        let mut maps = lock(&self.0);
        maps.owners.retain(|_, (view, _)| live.contains(view));
        maps.attempts.retain(|view, attempt| {
            if live.contains(view) {
                return true;
            }
            if attempt.window.has_pending() {
                attempt.window.report(*view, attempt.frame, "view_removed");
            }
            false
        });
    }

    pub(crate) fn begin(
        &self,
        world: &World,
        entity: Entity,
        view: RetainedViewEntity,
        late: bool,
        pass_name: &str,
    ) -> bool {
        let frame = world.resource::<FrameCount>().0;
        let producers = shadow_draw_producers(
            world.resource::<DrawFunctions<Shadow>>(),
            world.resource::<ShadowMapExternalInputs>(),
            view,
            frame,
        );
        if late {
            if !self.validate_draw_producers(view, producers) {
                return false;
            }
            return lock(&self.0)
                .attempts
                .get(&view)
                .is_some_and(|attempt| attempt.frame == frame && attempt.skip);
        }
        let key = make_key(world, entity, view, frame);
        let allocation = world
            .get::<DirectionalShadowMapInputs>(entity)
            .map(|inputs| (inputs.texture, inputs.slice));
        self.parity()
            .diagnose(world, entity, view, frame, &key, &producers);
        self.begin_key(view, frame, key, allocation, producers, pass_name)
    }

    fn begin_key(
        &self,
        view: RetainedViewEntity,
        frame: u32,
        key: Result<MapKey, Refusal>,
        allocation: Option<(TextureId, u32)>,
        producers: Result<DrawProducerSnapshot, Refusal>,
        pass_name: &str,
    ) -> bool {
        let key = key.and_then(|mut key| {
            key.draw_producers = producers?;
            Ok(key)
        });
        let mut maps = lock(&self.0);
        let skip = key.as_ref().is_ok_and(|key| {
            !self.1
                && maps
                    .owners
                    .get(&(key.inputs.texture, key.inputs.slice))
                    .is_some_and(|(owner, previous)| *owner == view && previous == key)
        });
        maps.counters.begin(&key, skip, self.1);
        if !skip {
            // Clear invalidates the old writer even when the new phase skips.
            if let Some(allocation) = allocation {
                maps.owners.remove(&allocation);
            }
        }
        let attempt = maps.attempts.entry(view).or_insert_with(|| Attempt {
            frame,
            skip,
            failed: false,
            early_complete: false,
            key: None,
            window: ShadowViewWindow::new(frame),
        });
        attempt.window.observe(frame, pass_name);
        attempt.window.record(|c| c.begin(&key, skip, self.1));
        attempt.frame = frame;
        attempt.skip = skip;
        attempt.failed = false;
        attempt.early_complete = false;
        attempt.key = key.ok();
        skip
    }

    fn incomplete(&self, view: RetainedViewEntity) {
        let mut maps = lock(&self.0);
        if let Some(attempt) = maps.attempts.get_mut(&view) {
            attempt.failed = true;
            attempt.window.record(|c| c.incomplete_draw += 1);
            maps.counters.incomplete_draw += 1;
        }
    }

    fn command_result(&self, view: RetainedViewEntity, result: &RenderCommandResult) {
        if !matches!(result, RenderCommandResult::Success) {
            self.incomplete(view);
        }
    }

    fn validate_draw_producers(
        &self,
        view: RetainedViewEntity,
        producers: Result<DrawProducerSnapshot, Refusal>,
    ) -> bool {
        let mut maps = lock(&self.0);
        let Some(attempt) = maps.attempts.get_mut(&view) else {
            return false;
        };
        let Some(key) = &attempt.key else {
            return false;
        };
        if producers
            .as_ref()
            .is_ok_and(|producers| *producers == key.draw_producers)
        {
            return true;
        }
        let allocation = (key.inputs.texture, key.inputs.slice);
        let was_reused = attempt.skip;
        attempt.window.record(|c| {
            if was_reused {
                // Late validation can revoke an early pass's reuse decision.
                // Count the newly required work before any pass is encoded.
                c.reused -= 1;
                c.rendered += 1;
            }
            if producers.is_err() {
                c.unsupported_producer += 1;
            } else {
                c.invalidated += 1;
            }
        });
        attempt.failed = true;
        attempt.skip = false;
        maps.owners.remove(&allocation);
        if producers.is_err() {
            maps.counters.unsupported_producer += 1;
        } else {
            maps.counters.invalidated += 1;
        }
        false
    }

    pub(crate) fn finish_with_world(
        &self,
        world: &World,
        view: RetainedViewEntity,
        late: bool,
        occlusion: bool,
        complete: bool,
    ) {
        let producers = shadow_draw_producers(
            world.resource::<DrawFunctions<Shadow>>(),
            world.resource::<ShadowMapExternalInputs>(),
            view,
            world.resource::<FrameCount>().0,
        );
        self.validate_draw_producers(view, producers);
        self.finish(view, late, occlusion, complete);
    }

    pub(crate) fn finish(
        &self,
        view: RetainedViewEntity,
        late: bool,
        occlusion: bool,
        complete: bool,
    ) {
        let mut maps = lock(&self.0);
        let Some(attempt) = maps.attempts.get_mut(&view) else {
            return;
        };
        attempt.failed |= !complete;
        if !complete {
            attempt.window.record(|c| c.incomplete_draw += 1);
        }
        if !late {
            attempt.early_complete = complete;
        }
        if attempt.skip || attempt.failed || !attempt.early_complete || (occlusion && !late) {
            return;
        }
        if let Some(key) = attempt.key.clone() {
            maps.owners
                .insert((key.inputs.texture, key.inputs.slice), (view, key));
        }
    }
}

pub(crate) fn report_shadow_cache_counters(
    cache: Res<ShadowMapCache>,
    frame: Res<FrameCount>,
    mut previous: Local<(u32, ShadowMapCacheCounters)>,
) {
    if frame.0.wrapping_sub(previous.0) < 300 {
        return;
    }
    let current = cache.counters();
    let old = previous.1;
    tracing::info!(
        frames = frame.0.wrapping_sub(previous.0),
        reused = current.reused.saturating_sub(old.reused),
        rendered = current.rendered.saturating_sub(old.rendered),
        invalidated = current.invalidated.saturating_sub(old.invalidated),
        external_incomplete = current
            .external_incomplete
            .saturating_sub(old.external_incomplete),
        unknown_material = current
            .unknown_material
            .saturating_sub(old.unknown_material),
        unsupported_producer = current
            .unsupported_producer
            .saturating_sub(old.unsupported_producer),
        stock_pending = current.stock_pending.saturating_sub(old.stock_pending),
        incomplete_draw = current.incomplete_draw.saturating_sub(old.incomplete_draw),
        forced = current.forced.saturating_sub(old.forced),
        audit_forced = current.audit_forced.saturating_sub(old.audit_forced),
        untracked_texture = current
            .untracked_texture
            .saturating_sub(old.untracked_texture),
        "shadow map reuse window"
    );
    // Cleanup follows all early/late rendering and audit hooks. Closing a
    // window here cannot lose a late reuse-to-render correction. This walks
    // retained attempts only once per report, never caster/dependency data.
    for (view, attempt) in &mut lock(&cache.0).attempts {
        attempt.window.report(*view, frame.0, "periodic");
    }
    *previous = (frame.0, current);
}

fn make_key(
    world: &World,
    entity: Entity,
    view: RetainedViewEntity,
    frame: u32,
) -> Result<MapKey, Refusal> {
    let external = lock(&world.resource::<ShadowMapExternalInputs>().0)
        .get(&view)
        .copied()
        .filter(|input| input.frame == frame && input.complete)
        .ok_or(Refusal::External)?;
    let external_materials = {
        let materials = lock(&world.resource::<ShadowMapExternalInputs>().1);
        let inputs = materials
            .views
            .get(&view)
            .filter(|inputs| inputs.frame == frame && inputs.initialized)
            .ok_or(Refusal::External)?;
        if !inputs.untracked.is_empty() {
            return Err(Refusal::UntrackedTexture);
        }
        if !inputs.unknown.is_empty() {
            return Err(Refusal::UnknownMaterial);
        }
        inputs.revision
    };
    if !matches!(
        world.resource::<RenderMeshInstances>(),
        RenderMeshInstances::GpuBuilding(_)
    ) {
        return Err(Refusal::UnsupportedProducer);
    }
    if world
        .resource::<crate::PendingShadowQueues>()
        .get(&view)
        .is_some_and(|pending| !pending.current_frame.is_empty())
    {
        return Err(Refusal::PendingStock);
    }
    let stock = world.resource::<ShadowStockInputs>().snapshot(view)?;
    let inputs = world
        .get::<DirectionalShadowMapInputs>(entity)
        .cloned()
        .ok_or(Refusal::UnsupportedProducer)?;
    // Include actual shadow-view state too, in case a renderer extension sets
    // per-shadow uniforms after prepare_lights recorded the camera/light inputs.
    let shadow_view = world
        .get::<ExtractedView>(entity)
        .ok_or(Refusal::UnsupportedProducer)?;
    let shadow_view = ShadowViewInputs {
        viewport: shadow_view.viewport.to_array(),
        world_from_view: shadow_view
            .world_from_view
            .to_matrix()
            .to_cols_array()
            .map(f32::to_bits),
        clip_from_view: shadow_view.clip_from_view.to_cols_array().map(f32::to_bits),
        clip_from_world: shadow_view
            .clip_from_world
            .unwrap_or_else(|| {
                shadow_view.clip_from_view * shadow_view.world_from_view.to_matrix().inverse()
            })
            .to_cols_array()
            .map(f32::to_bits),
        mip_bias: world
            .get::<bevy_render::camera::MipBias>(entity)
            .map_or(0.0, |bias| bias.0)
            .to_bits(),
        lod_origin: world
            .resource::<bevy_render::view::RenderShadowLodOrigin>()
            .0
            .to_array()
            .map(f32::to_bits),
    };
    Ok(MapKey {
        entity,
        inputs,
        shadow_view,
        external: external.revision,
        external_materials,
        stock,
        geometry: world.resource::<MeshAllocator>().content_revision(),
        meshes: world.resource::<RenderAssets<RenderMesh>>().revision(),
        images: world.resource::<RenderAssets<GpuImage>>().revision(),
        pipelines: world.resource::<PipelineCache>().readiness_revision(),
        draw_producers: (0, 0, 0, 0),
    })
}

/// Wraps a shadow command tuple so `Skip` cannot certify a partially rendered
/// map. Register and look up this wrapped type in `DrawFunctions`.
pub struct ShadowCacheTracking<C>(PhantomData<C>);

impl<C: RenderCommand<Shadow>> RenderCommand<Shadow> for ShadowCacheTracking<C> {
    type Param = (C::Param, SRes<ShadowMapCache>);
    type ViewQuery = (C::ViewQuery, &'static ExtractedView);
    type ItemQuery = C::ItemQuery;

    fn render<'w>(
        item: &Shadow,
        (view, extracted): ROQueryItem<'w, '_, Self::ViewQuery>,
        entity: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        (param, cache): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let result = C::render(item, view, entity, param, pass);
        cache.command_result(extracted.retained_view_entity, &result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_asset::AssetId;
    use bevy_ecs::entity::EntityIndex;

    fn entity(index: u32) -> Entity {
        Entity::from_index(EntityIndex::from_raw_u32(index).unwrap())
    }

    fn view(index: u32) -> RetainedViewEntity {
        RetainedViewEntity::new(entity(1).into(), Some(entity(index).into()), 0)
    }

    fn key() -> MapKey {
        MapKey {
            entity: entity(5),
            inputs: DirectionalShadowMapInputs {
                texture: TextureId::new(),
                slice: 0,
                matrices: [[0; 16]; 7],
                light: [0; 9],
                camera_layers: Default::default(),
                light_layers: Default::default(),
            },
            shadow_view: ShadowViewInputs::default(),
            external: 1,
            external_materials: 1,
            stock: (1, 1),
            geometry: 1,
            meshes: 1,
            images: 1,
            pipelines: 1,
            draw_producers: (0, 0, 0, 0),
        }
    }

    fn cache() -> ShadowMapCache {
        ShadowMapCache(
            Mutex::default(),
            false,
            parity::ShadowParityAudit::disabled(),
        )
    }

    #[test]
    fn fresh_shadow_audit_counts_contamination_without_second_view_decision() {
        let cache = cache();
        let view = view(8);
        let key = key();
        let allocation = (key.inputs.texture, key.inputs.slice);
        assert!(!cache.begin_key(
            view,
            42,
            Err(Refusal::UnknownMaterial),
            Some(allocation),
            Ok(key.draw_producers),
            "cascade"
        ));
        cache.finish(view, false, false, true);
        assert!(cache.force_fresh_audit_render(view, 42));
        let counters = cache.counters();
        assert_eq!(counters.rendered, 1);
        assert_eq!(counters.audit_forced, 1);
        assert_eq!(counters.unknown_material, 1);
        assert!(lock(&cache.0).attempts[&view].key.is_none());
        assert!(lock(&cache.0).owners.is_empty());
        assert!(!cache.force_fresh_audit_render(view, 42));
        assert!(!cache.force_fresh_audit_render(view, 43));
        cache.finish(view, false, false, true);
        let maps = lock(&cache.0);
        assert_eq!(maps.attempts[&view].window.counters.rendered, 1);
        assert_eq!(maps.attempts[&view].window.counters.audit_forced, 1);
        assert!(maps.owners.is_empty());
        drop(maps);
        // Complete A/B never makes an unknown material reusable next frame.
        assert!(!cache.begin_key(
            view,
            43,
            Err(Refusal::UnknownMaterial),
            Some(allocation),
            Ok(key.draw_producers),
            "cascade"
        ));
        assert_eq!(cache.counters().unknown_material, 2);
        assert_eq!(cache.counters().rendered, 2);
    }

    #[test]
    fn fresh_shadow_audit_never_replays_incomplete_normal_commands() {
        let cache = cache();
        let view = view(8);
        assert!(!begin(&cache, view, 42, key()));
        assert!(!cache.force_fresh_audit_render(view, 42));
        cache.finish(view, false, false, true);
        cache.command_result(view, &RenderCommandResult::Skip);
        assert!(!cache.force_fresh_audit_render(view, 42));
        assert_eq!(cache.counters().audit_forced, 0);
    }

    #[test]
    fn material_diagnostic_names_grant_no_reuse_eligibility() {
        let mut names = ShadowMaterialTypeNames::default();
        names.register::<StandardMaterial>();
        names.register::<()>();
        assert_eq!(
            names.0[&TypeId::of::<StandardMaterial>()],
            <StandardMaterial as bevy_reflect::TypePath>::type_path()
        );
        assert_eq!(
            names.0[&TypeId::of::<()>()],
            <() as bevy_reflect::TypePath>::type_path()
        );
        let declarations = ShadowCacheMaterialDependencies::default();
        assert!(!declarations.types.contains(&TypeId::of::<()>()));
    }

    #[test]
    fn shadow_parity_force_withdraws_owner_and_requires_complete_early_and_late() {
        let cache = cache();
        let view = view(8);
        let key = key();
        assert!(!begin(&cache, view, 1, key.clone()));
        cache.finish(view, false, false, true);
        assert!(begin(&cache, view, 2, key.clone()));
        assert!(cache.force_audit_render(view, 2).is_some());
        assert!(!lock(&cache.0).attempts[&view].skip);
        assert!(lock(&cache.0).owners.is_empty());
        cache.finish(view, false, true, true);
        assert!(lock(&cache.0).owners.is_empty());
        cache.finish(view, true, true, true);
        assert_eq!(lock(&cache.0).owners.len(), 1);
        assert_eq!(cache.counters().reused, 0);
        assert_eq!(cache.counters().audit_forced, 1);
        assert_eq!(cache.counters().rendered, 2);
    }

    #[test]
    fn shadow_parity_forced_late_skip_cannot_certify_partial_map() {
        let cache = cache();
        let view = view(8);
        let key = key();
        assert!(!begin(&cache, view, 1, key.clone()));
        cache.finish(view, false, false, true);
        assert!(begin(&cache, view, 2, key));
        assert!(cache.force_audit_render(view, 2).is_some());
        cache.finish(view, false, true, true);
        cache.command_result(view, &RenderCommandResult::Skip);
        cache.finish(view, true, true, true);
        assert!(lock(&cache.0).owners.is_empty());
    }

    #[test]
    fn shadow_parity_force_requires_a_current_positive_reuse_candidate() {
        let cache = cache();
        let view = view(8);
        assert!(!begin(&cache, view, 1, key()));
        assert!(cache.force_audit_render(view, 1).is_none());
        cache.finish(view, false, false, true);
        assert!(cache.force_audit_render(view, 2).is_none());
    }

    fn begin(cache: &ShadowMapCache, view: RetainedViewEntity, frame: u32, key: MapKey) -> bool {
        let allocation = (key.inputs.texture, key.inputs.slice);
        let producers = key.draw_producers;
        cache.begin_key(
            view,
            frame,
            Ok(key),
            Some(allocation),
            Ok(producers),
            "cascade",
        )
    }

    fn rendered(cache: &ShadowMapCache, view: RetainedViewEntity, key: MapKey) {
        assert!(!begin(cache, view, 1, key));
        cache.finish(view, false, false, true);
    }

    mod view_windows {
        use super::*;

        #[test]
        fn failure_after_window_close_remains_pending_without_another_view_frame() {
            let cache = cache();
            let view = view(2);
            rendered(&cache, view, key());
            close(&cache, view, 1);
            assert!(!lock(&cache.0).attempts[&view].window.has_pending());
            cache.incomplete(view);
            assert!(lock(&cache.0).attempts[&view].window.has_pending());
            let report = close(&cache, view, 2);
            assert_eq!(report.observed_frames, 0);
            assert_eq!(report.counters.incomplete_draw, 1);
            assert!(!report.zero_render_proof());
        }

        fn close(
            cache: &ShadowMapCache,
            view: RetainedViewEntity,
            frame: u32,
        ) -> ShadowViewWindowReport {
            lock(&cache.0)
                .attempts
                .get_mut(&view)
                .unwrap()
                .window
                .close(frame, true)
        }

        // RED: set MUDCRAB_SHADOW_VIEW_COUNTERS_RED=1 when running this module.
        // The test-only switch suppresses telemetry mutations, not cache decisions.
        #[test]
        fn one_changed_view_does_not_hide_other_views_zero_render_windows() {
            let cache = cache();
            let keys = [key(), key(), key()];
            for (index, key) in keys.iter().enumerate() {
                let view = view(index as u32 + 2);
                rendered(&cache, view, key.clone());
                close(&cache, view, 1);
            }
            for frame in 2..=301 {
                for (index, key) in keys.iter().enumerate() {
                    let mut key = key.clone();
                    if index == 0 {
                        key.external = u64::from(frame);
                    }
                    let view = view(index as u32 + 2);
                    assert_eq!(begin(&cache, view, frame, key), index != 0);
                    cache.finish(view, false, false, true);
                }
            }
            assert_eq!(cache.counters().rendered, 303);
            let changed = close(&cache, view(2), 301);
            assert_eq!(
                (changed.counters.rendered, changed.counters.invalidated),
                (300, 300)
            );
            assert!(!changed.zero_render_proof());
            for view in [view(3), view(4)] {
                let quiet = close(&cache, view, 301);
                assert_eq!((quiet.counters.reused, quiet.counters.rendered), (300, 0));
                assert!(quiet.zero_render_proof());
            }
        }

        #[test]
        fn stable_labels_keep_storage_and_windows_reset_without_old_counts() {
            let cache = cache();
            let key = key();
            rendered(&cache, view(2), key.clone());
            let label = lock(&cache.0).attempts[&view(2)].window.pass_name.as_ptr();
            let first = close(&cache, view(2), 1);
            assert_eq!(
                (first.start_frame, first.end_frame, first.counters.rendered),
                (1, 1, 1)
            );
            assert!(begin(&cache, view(2), 2, key.clone()));
            let second = close(&cache, view(2), 2);
            assert_eq!(
                (
                    second.start_frame,
                    second.observed_frames,
                    second.counters.rendered
                ),
                (2, 1, 0)
            );
            assert!(second.zero_render_proof());
            assert!(begin(&cache, view(2), 3, key));
            assert_eq!(
                lock(&cache.0).attempts[&view(2)].window.pass_name.as_ptr(),
                label
            );
            assert_eq!(close(&cache, view(2), 3).counters.reused, 1);
        }

        #[test]
        fn changed_labels_refuse_zero_proof_until_next_stable_window() {
            let cache = cache();
            let key = key();
            rendered(&cache, view(2), key.clone());
            close(&cache, view(2), 1);
            assert!(cache.begin_key(
                view(2),
                2,
                Ok(key.clone()),
                Some((key.inputs.texture, key.inputs.slice)),
                Ok(key.draw_producers),
                "cascade_changed"
            ));
            let changed = close(&cache, view(2), 2);
            assert_eq!(changed.counters.rendered, 0);
            assert_eq!(changed.label_changes, 1);
            assert!(!changed.label_valid);
            assert!(!changed.zero_render_proof());
            assert!(cache.begin_key(
                view(2),
                3,
                Ok(key.clone()),
                Some((key.inputs.texture, key.inputs.slice)),
                Ok(key.draw_producers),
                "cascade_changed"
            ));
            assert!(close(&cache, view(2), 3).zero_render_proof());
        }

        #[test]
        fn audit_replaces_reuse_with_render_in_the_same_window() {
            let cache = cache();
            let key = key();
            rendered(&cache, view(2), key.clone());
            close(&cache, view(2), 1);
            assert!(begin(&cache, view(2), 2, key));
            assert!(cache.force_audit_render(view(2), 2).is_some());
            cache.command_result(view(2), &RenderCommandResult::Skip);
            let report = close(&cache, view(2), 2);
            assert_eq!(
                (
                    report.counters.reused,
                    report.counters.rendered,
                    report.counters.audit_forced,
                    report.counters.incomplete_draw
                ),
                (0, 1, 1, 1)
            );
            assert!(!report.zero_render_proof());
        }

        #[test]
        fn command_failure_and_audit_revocation_count_each_incomplete_event() {
            for result in [
                RenderCommandResult::Skip,
                RenderCommandResult::Failure("test command failed"),
            ] {
                let cache = cache();
                let key = key();
                rendered(&cache, view(2), key.clone());
                close(&cache, view(2), 1);
                assert!(begin(&cache, view(2), 2, key.clone()));
                assert!(cache.force_audit_render(view(2), 2).is_some());
                cache.command_result(view(2), &result);
                cache.revoke_audit(view(2), (key.inputs.texture, key.inputs.slice));
                let report = close(&cache, view(2), 2);
                assert_eq!(report.counters.incomplete_draw, 2);
                assert_eq!(report.counters.rendered, 1);
                assert!(!report.zero_render_proof());
                assert!(lock(&cache.0).owners.is_empty());
            }
        }

        #[test]
        fn late_producer_revocation_counts_render_and_refusal_without_double_render() {
            for producers in [Err(Refusal::UnsupportedProducer), Ok((1, 0, 0, 0))] {
                let cache = cache();
                let key = key();
                rendered(&cache, view(2), key.clone());
                close(&cache, view(2), 1);
                assert!(begin(&cache, view(2), 2, key));
                assert!(!cache.validate_draw_producers(view(2), producers));
                assert!(!cache.validate_draw_producers(view(2), producers));
                cache.finish(view(2), true, true, false);
                let report = close(&cache, view(2), 2);
                assert_eq!(
                    (
                        report.counters.reused,
                        report.counters.rendered,
                        report.counters.incomplete_draw
                    ),
                    (0, 1, 1)
                );
                assert_eq!(
                    report.counters.unsupported_producer + report.counters.invalidated,
                    2
                );
                assert!(!report.zero_render_proof());
            }
        }

        #[test]
        fn refusal_force_and_phase_failure_are_owned_by_the_view() {
            for refusal in [
                Refusal::External,
                Refusal::UnknownMaterial,
                Refusal::UnsupportedProducer,
                Refusal::PendingStock,
                Refusal::UntrackedTexture,
            ] {
                let cache = cache();
                assert!(!cache.begin_key(
                    view(2),
                    1,
                    Err(refusal),
                    None,
                    Ok((0, 0, 0, 0)),
                    "cascade"
                ));
                cache.finish(view(2), false, false, false);
                let report = close(&cache, view(2), 1);
                let c = report.counters;
                assert_eq!(
                    c.external_incomplete
                        + c.unknown_material
                        + c.unsupported_producer
                        + c.stock_pending
                        + c.untracked_texture,
                    1
                );
                assert_eq!((c.rendered, c.incomplete_draw), (1, 1));
                assert!(!report.zero_render_proof());
            }
            let cache = ShadowMapCache(
                Mutex::default(),
                true,
                parity::ShadowParityAudit::disabled(),
            );
            rendered(&cache, view(2), key());
            assert_eq!(close(&cache, view(2), 1).counters.forced, 1);
        }

        #[test]
        fn missing_frames_and_lifecycle_boundaries_refuse_zero_proof() {
            let cache = cache();
            let key = key();
            rendered(&cache, view(2), key.clone());
            close(&cache, view(2), 1);
            assert!(begin(&cache, view(2), 3, key.clone()));
            assert!(!close(&cache, view(2), 3).zero_render_proof());
            assert!(begin(&cache, view(2), 4, key.clone()));
            let terminal = lock(&cache.0)
                .attempts
                .get_mut(&view(2))
                .unwrap()
                .window
                .close(4, false);
            assert!(!terminal.zero_render_proof());
            cache.invalidate_physical_maps();
            assert!(lock(&cache.0).attempts.is_empty());
            assert!(!begin(&cache, view(2), 5, key.clone()));
            assert_eq!(close(&cache, view(2), 5).counters.rendered, 1);
            cache.retain(&HashSet::default());
            assert!(!begin(&cache, view(2), 6, key));
            assert_eq!(close(&cache, view(2), 6).counters.rendered, 1);
        }
    }

    mod draw_producer_regressions {
        use super::*;
        use bevy_render::render_phase::{Draw, DrawError};

        // Represents a NonMesh producer whose uniforms live outside the stock
        // mesh/material producers. No GPU is needed to exercise the cache gate.
        struct UnknownDraw;

        impl Draw<Shadow> for UnknownDraw {
            fn draw<'w>(
                &mut self,
                _: &'w World,
                _: &mut TrackedRenderPass<'w>,
                _: Entity,
                _: &Shadow,
            ) -> Result<(), DrawError> {
                unreachable!("cache admission tests do not rasterize")
            }
        }

        struct ExternalCommand<const N: usize = 0>;

        impl<const N: usize> RenderCommand<Shadow> for ExternalCommand<N> {
            type Param = ();
            type ViewQuery = ();
            type ItemQuery = ();

            fn render<'w>(
                _: &Shadow,
                _: ROQueryItem<'w, '_, Self::ViewQuery>,
                _: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
                _: SystemParamItem<'w, '_, Self::Param>,
                _: &mut TrackedRenderPass<'w>,
            ) -> RenderCommandResult {
                RenderCommandResult::Skip
            }
        }

        fn add_external(draws: &DrawFunctions<Shadow>) {
            add_external_command::<0>(draws);
        }

        fn add_external_command<const N: usize>(draws: &DrawFunctions<Shadow>) {
            let mut world = World::new();
            world.insert_resource(cache());
            let command =
                RenderCommandState::<Shadow, ShadowCacheTracking<ExternalCommand<N>>>::new(
                    &mut world,
                );
            draws
                .write()
                .add_with::<ShadowCacheTracking<ExternalCommand<N>>, _>(command);
        }

        fn begin_registered(
            cache: &ShadowMapCache,
            draws: &DrawFunctions<Shadow>,
            external: &ShadowMapExternalInputs,
            frame: u32,
            key: &MapKey,
        ) -> bool {
            cache.begin_key(
                view(2),
                frame,
                Ok(key.clone()),
                Some((key.inputs.texture, key.inputs.slice)),
                shadow_draw_producers(draws, external, view(2), frame),
                "cascade",
            )
        }

        #[test]
        fn unknown_nonmesh_insertion_refuses_previously_settled_map_in_same_frame() {
            let cache = cache();
            let key = key();
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            assert!(!begin_registered(&cache, &draws, &external, 1, &key));
            cache.finish(view(2), false, false, true);
            assert!(begin_registered(&cache, &draws, &external, 2, &key));
            draws.write().add(UnknownDraw);
            assert!(!begin_registered(&cache, &draws, &external, 2, &key));
            assert_eq!(cache.counters().unsupported_producer, 1);
        }

        #[test]
        fn unknown_removal_must_render_before_reuse_can_recover() {
            let cache = cache();
            let key = key();
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            assert!(!begin_registered(&cache, &draws, &external, 1, &key));
            cache.finish(view(2), false, false, true);
            draws.write().add(UnknownDraw);
            assert!(!begin_registered(&cache, &draws, &external, 2, &key));
            cache.finish(view(2), false, false, true);
            draws.write().clear();
            assert!(!begin_registered(&cache, &draws, &external, 2, &key));
            cache.finish(view(2), false, false, true);
            assert!(begin_registered(&cache, &draws, &external, 3, &key));
        }

        #[test]
        fn aggregate_certificate_cannot_authorize_unknown_external_uniform_inputs() {
            let cache = cache();
            let key = key();
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            draws.write().add(UnknownDraw);
            external.publish(view(2), 1, 7, true);
            assert!(!begin_registered(&cache, &draws, &external, 1, &key));
            cache.finish(view(2), false, false, true);
            // The aggregate cluster certificate remains unchanged while a
            // custom producer changes a uniform that it alone owns.
            external.publish(view(2), 2, 7, true);
            assert!(!begin_registered(&cache, &draws, &external, 2, &key));
            assert_eq!(cache.counters().unsupported_producer, 2);
        }

        #[test]
        fn stock_registration_alias_cannot_hide_a_custom_draw() {
            let draws = DrawFunctions::default();
            draws
                .write()
                .add_with::<ShadowCacheTracking<crate::DrawPrepass>, _>(UnknownDraw);
            assert_eq!(
                shadow_draw_producers(&draws, &ShadowMapExternalInputs::default(), view(2), 1),
                Err(Refusal::UnsupportedProducer),
            );
        }

        #[test]
        fn wrapped_external_draw_still_requires_its_own_current_complete_certificate() {
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            add_external(&draws);
            external.publish(view(2), 1, 7, true);
            assert_eq!(
                shadow_draw_producers(&draws, &external, view(2), 1),
                Err(Refusal::UnsupportedProducer)
            );
            external.publish_draw::<ExternalCommand>(view(2), 1, 9, false);
            assert_eq!(
                shadow_draw_producers(&draws, &external, view(2), 1),
                Err(Refusal::UnsupportedProducer)
            );
            external.publish_draw::<ExternalCommand>(view(2), 1, 9, true);
            assert!(shadow_draw_producers(&draws, &external, view(2), 1).is_ok());
            assert_eq!(
                shadow_draw_producers(&draws, &external, view(2), 2),
                Err(Refusal::UnsupportedProducer)
            );
            assert_eq!(
                shadow_draw_producers(&draws, &external, view(3), 1),
                Err(Refusal::UnsupportedProducer)
            );
        }

        #[test]
        fn certified_external_input_change_and_removal_invalidate_in_same_frame() {
            let cache = cache();
            let key = key();
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            add_external(&draws);
            external.publish_draw::<ExternalCommand>(view(2), 1, 9, true);
            assert!(!begin_registered(&cache, &draws, &external, 1, &key));
            cache.finish(view(2), false, false, true);
            external.publish_draw::<ExternalCommand>(view(2), 2, 9, true);
            assert!(begin_registered(&cache, &draws, &external, 2, &key));
            external.publish_draw::<ExternalCommand>(view(2), 2, 10, true);
            assert!(!begin_registered(&cache, &draws, &external, 2, &key));
            cache.finish(view(2), false, false, true);
            draws.write().clear();
            assert!(!begin_registered(&cache, &draws, &external, 2, &key));
        }

        #[test]
        fn direct_registry_replacement_is_not_hidden_by_unchanged_length_or_aliases() {
            let cache = cache();
            let key = key();
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            add_external(&draws);
            external.publish_draw::<ExternalCommand>(view(2), 1, 9, true);
            assert!(!begin_registered(&cache, &draws, &external, 1, &key));
            cache.finish(view(2), false, false, true);
            assert!(draws
                .write()
                .replace(bevy_render::render_phase::DrawFunctionId(0), UnknownDraw));
            assert!(!begin_registered(&cache, &draws, &external, 1, &key));
        }

        #[test]
        fn registration_after_early_reuse_invalidates_late_pass_and_physical_owner() {
            let cache = cache();
            let key = key();
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            assert!(!begin_registered(&cache, &draws, &external, 1, &key));
            cache.finish(view(2), false, false, true);
            assert!(begin_registered(&cache, &draws, &external, 2, &key));
            draws.write().add(UnknownDraw);
            assert!(!cache.validate_draw_producers(
                view(2),
                shadow_draw_producers(&draws, &external, view(2), 2)
            ));
            assert!(lock(&cache.0).owners.is_empty());
            cache.finish(view(2), true, true, true);
            assert!(lock(&cache.0).owners.is_empty());
        }

        #[test]
        fn certified_skip_never_commits_a_map() {
            let cache = cache();
            let key = key();
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            add_external(&draws);
            external.publish_draw::<ExternalCommand>(view(2), 1, 9, true);
            assert!(!begin_registered(&cache, &draws, &external, 1, &key));
            cache.command_result(view(2), &RenderCommandResult::Skip);
            cache.finish(view(2), false, false, true);
            external.publish_draw::<ExternalCommand>(view(2), 2, 9, true);
            assert!(!begin_registered(&cache, &draws, &external, 2, &key));
            assert_eq!(cache.counters().incomplete_draw, 1);
        }

        #[test]
        fn settled_registry_checks_do_not_reclassify_after_unchanged_publications() {
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            add_external(&draws);
            external.publish_draw::<ExternalCommand>(view(2), 1, 9, true);
            let baseline = shadow_draw_producers(&draws, &external, view(2), 1).unwrap();
            for frame in 2..20 {
                external.publish_draw::<ExternalCommand>(view(2), frame, 9, true);
                assert_eq!(
                    shadow_draw_producers(&draws, &external, view(2), frame).unwrap(),
                    baseline
                );
            }
            assert_eq!(lock(&external.2).classifications, 1);
        }

        #[test]
        fn registry_recreation_changes_the_revision_even_with_identical_contents() {
            let cache = cache();
            let key = key();
            let external = ShadowMapExternalInputs::default();
            let draws = DrawFunctions::default();
            assert!(!begin_registered(&cache, &draws, &external, 1, &key));
            cache.finish(view(2), false, false, true);
            let replacement = DrawFunctions::default();
            assert!(!begin_registered(&cache, &replacement, &external, 1, &key));
        }

        #[test]
        fn preparing_and_fetching_draws_preserve_the_registry_revision() {
            let draws = DrawFunctions::default();
            let mut functions = draws.write();
            let id = functions.add(UnknownDraw);
            let revision = functions.draw_functions.revision();
            functions.prepare(&World::new());
            assert!(functions.get_mut(id).is_some());
            assert_eq!(functions.draw_functions.revision(), revision);
        }

        #[test]
        fn third_external_command_cannot_expand_the_certified_pair() {
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            external.publish_draw::<ExternalCommand<0>>(view(2), 1, 9, true);
            external.publish_draw::<ExternalCommand<1>>(view(2), 1, 9, true);
            external.publish_draw::<ExternalCommand<2>>(view(2), 1, 9, true);
            add_external_command::<2>(&draws);
            assert_eq!(
                shadow_draw_producers(&draws, &external, view(2), 1),
                Err(Refusal::UnsupportedProducer)
            );
        }

        #[test]
        fn external_certificate_alias_cannot_authorize_a_raw_draw() {
            let draws = DrawFunctions::default();
            let external = ShadowMapExternalInputs::default();
            external.publish_draw::<ExternalCommand>(view(2), 1, 9, true);
            draws
                .write()
                .add_with::<ShadowCacheTracking<ExternalCommand>, _>(UnknownDraw);
            assert_eq!(
                shadow_draw_producers(&draws, &external, view(2), 1),
                Err(Refusal::UnsupportedProducer)
            );
        }
    }

    #[test]
    fn settled_map_skips_both_clear_and_render() {
        let cache = cache();
        let key = key();
        let mut clears = 0;
        let mut draws = 0;
        for frame in 1..5 {
            if !begin(&cache, view(2), frame, key.clone()) {
                clears += 1;
                draws += 1;
                cache.finish(view(2), false, false, true);
            }
        }
        assert_eq!((clears, draws, cache.counters().reused), (1, 1, 3));
    }

    #[test]
    fn every_exact_scalar_input_invalidates_in_the_same_frame() {
        let baseline = key();
        // Covers all seven matrices and nine light/LOD/bias/extent words.
        for input in 0..(7 * 16 + 9) {
            let cache = cache();
            rendered(&cache, view(2), baseline.clone());
            let mut changed = baseline.clone();
            *changed
                .inputs
                .matrices
                .iter_mut()
                .flatten()
                .chain(changed.inputs.light.iter_mut())
                .nth(input)
                .unwrap() ^= 1;
            assert!(!begin(&cache, view(2), 2, changed), "input word {input}");
        }
    }

    #[test]
    fn every_exact_shadow_view_word_invalidates_in_the_same_frame() {
        let baseline = key();
        // Together with prepare_lights' 121 words, these cover all 177 words.
        for input in 0..(4 + 3 * 16 + 1 + 3) {
            let cache = cache();
            rendered(&cache, view(2), baseline.clone());
            let mut changed = baseline.clone();
            let shadow_view = &mut changed.shadow_view;
            *shadow_view
                .viewport
                .iter_mut()
                .chain(shadow_view.world_from_view.iter_mut())
                .chain(shadow_view.clip_from_view.iter_mut())
                .chain(shadow_view.clip_from_world.iter_mut())
                .chain(core::iter::once(&mut shadow_view.mip_bias))
                .chain(shadow_view.lod_origin.iter_mut())
                .nth(input)
                .unwrap() ^= 1;
            assert!(
                !begin(&cache, view(2), 2, changed),
                "shadow-view word {input}"
            );
        }
    }

    #[test]
    fn every_producer_revision_invalidates_independently() {
        let baseline = key();
        for field in 0..10 {
            let cache = cache();
            rendered(&cache, view(2), baseline.clone());
            let mut changed = baseline.clone();
            match field {
                0 => changed.external += 1,
                1 => changed.external_materials += 1,
                2 => changed.stock.0 += 1,
                3 => changed.stock.1 += 1,
                4 => changed.geometry += 1,
                5 => changed.meshes += 1,
                6 => changed.images += 1,
                7 => changed.pipelines += 1,
                8 => changed.entity = entity(6),
                _ => changed.draw_producers.0 += 1,
            }
            assert!(
                !begin(&cache, view(2), 2, changed),
                "producer field {field}"
            );
        }
    }

    #[test]
    fn camera_and_light_layers_invalidate_independently() {
        for camera in [true, false] {
            let baseline = key();
            let cache = cache();
            rendered(&cache, view(2), baseline.clone());
            let mut changed = baseline;
            if camera {
                changed.inputs.camera_layers = bevy_camera::visibility::RenderLayers::layer(1);
            } else {
                changed.inputs.light_layers = bevy_camera::visibility::RenderLayers::layer(1);
            }
            assert!(!begin(&cache, view(2), 2, changed));
        }
    }

    #[test]
    fn physical_allocation_and_slice_changes_invalidate() {
        for reallocate in [true, false] {
            let baseline = key();
            let cache = cache();
            rendered(&cache, view(2), baseline.clone());
            let mut changed = baseline;
            if reallocate {
                changed.inputs.texture = TextureId::new();
            } else {
                changed.inputs.slice += 1;
            }
            assert!(!begin(&cache, view(2), 2, changed));
        }
    }

    #[test]
    fn another_camera_overwriting_a_slice_prevents_reuse() {
        let cache = cache();
        let key = key();
        rendered(&cache, view(2), key.clone());
        rendered(&cache, view(3), key.clone());
        assert!(!begin(&cache, view(2), 2, key));
    }

    #[test]
    fn removed_and_recreated_views_cannot_reuse_old_maps() {
        let cache = cache();
        let key = key();
        rendered(&cache, view(2), key.clone());
        cache.retain(&HashSet::default());
        assert!(!begin(&cache, view(2), 2, key));
    }

    #[test]
    fn incomplete_commands_never_commit_and_ready_retry_can_recover() {
        let cache = cache();
        let key = key();
        assert!(!begin(&cache, view(2), 1, key.clone()));
        cache.incomplete(view(2)); // RenderCommandResult::Skip or Failure.
        cache.finish(view(2), false, false, true); // phase render returns Ok on Skip.
        assert!(!begin(&cache, view(2), 2, key.clone()));
        cache.finish(view(2), false, false, true);
        assert!(begin(&cache, view(2), 3, key));
    }

    #[test]
    fn occlusion_requires_successful_early_and_late_passes() {
        let cache = cache();
        let key = key();
        assert!(!begin(&cache, view(2), 1, key.clone()));
        cache.finish(view(2), false, true, true);
        assert!(lock(&cache.0).owners.is_empty());
        cache.incomplete(view(2));
        cache.finish(view(2), true, true, true);
        assert!(lock(&cache.0).owners.is_empty());
        assert!(!begin(&cache, view(2), 2, key.clone()));
        cache.finish(view(2), false, true, true);
        cache.finish(view(2), true, true, true);
        assert!(begin(&cache, view(2), 3, key));
    }

    #[test]
    fn refusals_invalidate_previous_physical_owner() {
        for refusal in [
            Refusal::External,
            Refusal::UnknownMaterial,
            Refusal::UnsupportedProducer,
            Refusal::PendingStock,
            Refusal::UntrackedTexture,
        ] {
            let cache = cache();
            let key = key();
            rendered(&cache, view(2), key.clone());
            assert!(!cache.begin_key(
                view(2),
                2,
                Err(refusal),
                Some((key.inputs.texture, key.inputs.slice)),
                Ok((0, 0, 0, 0)),
                "cascade",
            ));
            cache.finish(view(2), false, false, true);
            assert!(!begin(&cache, view(2), 3, key));
        }
    }

    #[test]
    fn forced_recompute_uses_the_same_inputs_without_reuse() {
        let cache = ShadowMapCache(
            Mutex::default(),
            true,
            parity::ShadowParityAudit::disabled(),
        );
        let key = key();
        rendered(&cache, view(2), key.clone());
        assert!(!begin(&cache, view(2), 2, key));
        assert_eq!(cache.counters().forced, 2);
    }

    fn material() -> UntypedAssetId {
        AssetId::<StandardMaterial>::default().untyped()
    }

    #[test]
    fn caster_mutations_only_invalidate_views_that_use_them() {
        let inputs = ShadowStockInputs::default();
        inputs.set_caster(
            view(2),
            entity(10).into(),
            material(),
            Some(1),
            Some(0),
            false,
        );
        inputs.set_caster(
            view(3),
            entity(11).into(),
            material(),
            Some(1),
            Some(0),
            false,
        );
        let before = (
            inputs.snapshot(view(2)).unwrap(),
            inputs.snapshot(view(3)).unwrap(),
        );
        inputs.caster_changed(entity(10).into());
        assert_ne!(inputs.snapshot(view(2)).unwrap(), before.0);
        assert_eq!(inputs.snapshot(view(3)).unwrap(), before.1);
    }

    #[test]
    fn stock_removal_and_morph_changes_invalidate_without_population_scans() {
        let inputs = ShadowStockInputs::default();
        inputs.set_caster(
            view(2),
            entity(10).into(),
            material(),
            Some(1),
            Some(0),
            false,
        );
        inputs.observe_morph(entity(10).into(), &[0.25], true);
        let before = inputs.snapshot(view(2)).unwrap();
        inputs.observe_morph(entity(10).into(), &[0.25], false);
        assert_eq!(inputs.snapshot(view(2)).unwrap(), before);
        inputs.observe_morph(entity(10).into(), &[0.5], true);
        assert_ne!(inputs.snapshot(view(2)).unwrap(), before);
        let before = inputs.snapshot(view(2)).unwrap();
        inputs.remove_caster(view(2), entity(10).into());
        assert_ne!(inputs.snapshot(view(2)).unwrap(), before);
    }

    #[test]
    fn morph_producer_ticks_gate_byte_inspection_and_reappearance() {
        let inputs = ShadowStockInputs::default();
        let caster = entity(10).into();
        inputs.set_caster(view(2), caster, material(), Some(1), Some(0), false);
        inputs.observe_morph(caster, &[0.25], true);
        let before = inputs.snapshot(view(2)).unwrap();
        // A settled producer certificate prevents reading the supplied bytes.
        // Actual ECS writes always mark either the value or reference changed.
        inputs.observe_morph(caster, &[0.5], false);
        assert_eq!(inputs.snapshot(view(2)).unwrap(), before);
        assert_eq!(lock(&inputs.0).morph_weights[&caster], [0.25f32.to_bits()]);
        inputs.observe_morph(caster, &[0.5], true);
        assert_ne!(inputs.snapshot(view(2)).unwrap(), before);
        inputs.remove_morph(caster);
        inputs.observe_morph(caster, &[0.75], false);
        assert_eq!(lock(&inputs.0).morph_weights[&caster], [0.75f32.to_bits()]);
    }

    #[test]
    fn unrelated_material_mutations_do_not_touch_selected_cascade_revisions() {
        let inputs = ShadowStockInputs::default();
        inputs.set_caster(
            view(2),
            entity(10).into(),
            material(),
            Some(1),
            Some(0),
            false,
        );
        let before = inputs.snapshot(view(2)).unwrap();
        let unrelated = AssetId::<StandardMaterial>::invalid().untyped();
        route_material_change(&mut lock(&inputs.0), unrelated, Some((9, 3)), false);
        assert_eq!(inputs.snapshot(view(2)).unwrap(), before);
        route_material_change(&mut lock(&inputs.0), material(), Some((2, 0)), false);
        assert_ne!(inputs.snapshot(view(2)).unwrap(), before);
    }

    #[test]
    fn opaque_texture_bindings_refuse_until_inspectable_material_replacement() {
        let inputs = ShadowStockInputs::default();
        inputs.set_caster(
            view(2),
            entity(10).into(),
            material(),
            Some(1),
            Some(0),
            true,
        );
        assert_eq!(inputs.snapshot(view(2)), Err(Refusal::UntrackedTexture));
        route_material_change(&mut lock(&inputs.0), material(), Some((2, 0)), false);
        assert!(inputs.snapshot(view(2)).is_ok());
    }

    #[test]
    fn external_closure_survives_settled_frames_and_receives_late_material_mutations() {
        let external = ShadowMapExternalInputs::default();
        external.reset_materials(view(2), 1);
        {
            let mut inputs = lock(&external.1);
            inputs
                .views
                .get_mut(&view(2))
                .unwrap()
                .materials
                .insert(material());
            inputs
                .material_views
                .entry(material())
                .or_default()
                .insert(view(2));
            route_external_material_change(&mut inputs, material(), Some((1, 0)), false);
        }
        external.begin_materials(view(2), 2);
        let before = lock(&external.1).views[&view(2)].revision;
        assert!(lock(&external.1).views[&view(2)]
            .materials
            .contains(&material()));
        route_external_material_change(&mut lock(&external.1), material(), Some((2, 0)), true);
        assert!(lock(&external.1).views[&view(2)]
            .untracked
            .contains(&material()));
        assert_ne!(lock(&external.1).views[&view(2)].revision, before);
        external.reset_materials(view(2), 3);
        assert!(lock(&external.1).views[&view(2)].untracked.is_empty());
        assert!(!lock(&external.1).material_views.contains_key(&material()));
    }

    #[test]
    fn repeated_empty_external_closures_preserve_revision_and_transitions_invalidate() {
        let external = ShadowMapExternalInputs::default();
        external.reset_materials(view(2), 1);
        let empty_revision = lock(&external.1).views[&view(2)].revision;
        for frame in 2..10 {
            external.reset_materials(view(2), frame);
            let inputs = lock(&external.1);
            let state = &inputs.views[&view(2)];
            assert_eq!(state.frame, frame);
            assert!(state.initialized);
            assert_eq!(
                state.revision, empty_revision,
                "unchanged empty frame {frame}"
            );
        }
        // Mirror the existing queue's membership publication, without needing
        // a GPU PreparedMaterial to exercise closure lifetime and invalidation.
        {
            let mut inputs = lock(&external.1);
            let state = inputs.views.get_mut(&view(2)).unwrap();
            state.materials.insert(material());
            inputs
                .material_views
                .entry(material())
                .or_default()
                .insert(view(2));
            route_external_material_change(&mut inputs, material(), Some((1, 0)), false);
        }
        let populated_revision = lock(&external.1).views[&view(2)].revision;
        assert_ne!(populated_revision, empty_revision);
        external.reset_materials(view(2), 10);
        let cleared_revision = lock(&external.1).views[&view(2)].revision;
        assert_ne!(cleared_revision, populated_revision);
        assert!(!lock(&external.1).material_views.contains_key(&material()));
        external.reset_materials(view(2), 11);
        assert_eq!(lock(&external.1).views[&view(2)].revision, cleared_revision);
    }
}
