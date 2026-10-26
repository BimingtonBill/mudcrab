//! Opt-in native normal-policy-versus-forced depth texels. Never timing evidence.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use bevy_asset::UntypedAssetId;
use bevy_diagnostic::FrameCount;
use bevy_ecs::prelude::*;
use bevy_platform::collections::{HashMap, HashSet};
use bevy_render::{
    erased_render_asset::ErasedRenderAssets,
    render_asset::RenderAssets,
    render_phase::{DrawFunctions, ViewBinnedRenderPhases},
    render_resource::{
        Buffer, BufferDescriptor, BufferUsages, Extent3d, LoadOp, MapMode, Origin3d,
        TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect,
        TextureFormat, TextureId, TextureUsages,
    },
    renderer::{RenderContext, RenderDevice},
    texture::{FallbackImage, FallbackImageCubemap, FallbackImageZero, GpuImage},
    view::RetainedViewEntity,
};

use super::{
    lock, make_key, shadow_draw_producers, DirectionalShadowMapInputs, MapKey,
    RetainedDirectionalShadowTexture, ShadowMapCache, ShadowMapExternalInputs,
    ShadowReuseVerification, ShadowStockInputs,
};

// Staging AND callback-owned results remain bounded. A stalled device stops new
// audits rather than abandoning pending buffers and growing an unbounded queue.
const MAX_JOBS: usize = 4;
const MAX_STAGING_BYTES: u64 = 256 * 1024 * 1024;
const MAP_FRAME_LIMIT: u32 = 1200;
const MAX_CANDIDATE_LOG_CHARS: usize = 2048;
const MAX_CANDIDATE_LOG_ROWS_PER_KIND: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Allocation {
    view: RetainedViewEntity,
    texture: TextureId,
    slice: u32,
}

struct Observation {
    first_frame: u32,
    candidate: bool,
    outcome: &'static str,
    non_clear_texels: Option<usize>,
}

type MapResults = Arc<Mutex<[Option<Result<Vec<u8>, String>>; 2]>>;

struct Job {
    allocation: Allocation,
    frame: u32,
    key: Option<MapKey>,
    entity: Entity,
    reused: bool,
    copied_a: bool,
    producers: super::DrawProducerSnapshot,
    texture: Texture,
    buffers: [Buffer; 2],
    row_bytes: u32,
    padded_row_bytes: u32,
    size: Extent3d,
    occlusion: bool,
    early: bool,
    late: bool,
    clear_bits: Option<u32>,
    phase_empty: bool,
    copied_b: bool,
    failure: Option<&'static str>,
    results: Option<MapResults>,
    metadata: String,
    stem: String,
    candidate_preview: String,
    attachment_view: u32,
}

/// Full candidate membership belongs to a file, never an unbounded error log.
/// A map-wide difference cannot identify which member caused the difference.
struct CandidateReceipt {
    text: String,
    preview: String,
}

impl CandidateReceipt {
    fn from_rows(mut rows: Vec<String>) -> Self {
        rows.sort();
        rows.dedup();
        let mut preview = Vec::new();
        for category in [
            "stock_candidate ",
            "external_material_candidate ",
            "sampled_texture_candidate ",
            "non_mesh_candidate ",
        ] {
            preview.extend(
                rows.iter()
                    .filter(|row| row.starts_with(category))
                    .take(MAX_CANDIDATE_LOG_ROWS_PER_KIND)
                    .map(String::as_str),
            );
        }
        let preview: String = preview
            .join(" | ")
            .chars()
            .take(MAX_CANDIDATE_LOG_CHARS)
            .collect();
        Self {
            text: format!(
                "attribution=candidate_set_not_isolated_culprit\nsource_reference_scope=stock_entities_only_external_carriers_are_not_individual_source_references\ncandidate_rows={}\n{}\n",
                rows.len(),
                rows.join("\n")
            ),
            preview,
        }
    }
}

// Diagnostic-only membership. Never use these rows to admit reuse or a pair.
fn refusal_material_rows(
    world: &World,
    view: RetainedViewEntity,
    frame: u32,
) -> (Vec<String>, HashSet<UntypedAssetId>) {
    let prepared = world.resource::<ErasedRenderAssets<crate::PreparedMaterial>>();
    let declarations = world.resource::<super::ShadowCacheMaterialDependencies>();
    let names = world.get_resource::<super::ShadowMaterialTypeNames>();
    let mut rows = vec![format!(
        "REFUSAL_DIAGNOSTIC frame={frame} diagnostic_only=true eligibility_granted=false"
    )];
    let mut materials = HashSet::default();
    let mut row = |origin: &str,
                   material: UntypedAssetId,
                   unknown: bool,
                   untracked: bool,
                   stored: Option<&(u64, u64)>,
                   membership: String| {
        let name = names
            .and_then(|names| names.0.get(&material.type_id()))
            .copied()
            .unwrap_or("UNREGISTERED_MATERIAL_TYPE");
        let texture = prepared
            .get(material)
            .map(|material| material.shadow_cache_untracked_texture);
        rows.push(format!("refusal_material origin={origin} material={material:?} type_id={:?} type_path={name} unknown_id={unknown} stored_pair={stored:?} prepared_revision={:?} declaration_revision={:?} prepared_present={} texture_flag={texture:?} stored_untracked_texture={untracked} {membership}", material.type_id(), prepared.asset_revision(material), declarations.dependency(material), prepared.get(material).is_some()));
        materials.insert(material);
    };
    {
        let inputs = lock(&world.resource::<ShadowStockInputs>().0);
        if let Some(stock) = inputs.views.get(&view) {
            for material in stock
                .materials
                .keys()
                .chain(stock.unknown_materials.iter())
                .chain(stock.untracked_textures.iter())
                .copied()
                .collect::<HashSet<_>>()
            {
                let mut casters: Vec<_> = stock
                    .casters
                    .iter()
                    .filter(|(_, id)| **id == material)
                    .map(|(entity, _)| entity.to_bits())
                    .collect();
                casters.sort_unstable();
                row(
                    "stock",
                    material,
                    stock.unknown_materials.contains(&material),
                    stock.untracked_textures.contains(&material),
                    stock.material_inputs.get(&material),
                    format!(
                        "caster_main_entities={casters:?} stock_incomplete={}",
                        stock.incomplete
                    ),
                );
            }
        }
    }
    {
        let inputs = lock(&world.resource::<ShadowMapExternalInputs>().1);
        if let Some(state) = inputs.views.get(&view) {
            for material in state
                .materials
                .iter()
                .chain(state.unknown.iter())
                .chain(state.untracked.iter())
                .copied()
                .collect::<HashSet<_>>()
            {
                row("external", material, state.unknown.contains(&material), state.untracked.contains(&material), state.revisions.get(&material), format!("caster_membership=view_material_closure_not_individual_source_reference closure_frame={} closure_initialized={} current_frame={frame}", state.frame, state.initialized));
            }
        }
    }
    rows.sort();
    (rows, materials)
}

/// Called only after admission of a real reuse candidate in a requested frame.
fn candidate_membership(
    stock: &ShadowStockInputs,
    external: &ShadowMapExternalInputs,
    view: RetainedViewEntity,
    frame: u32,
) -> Result<(Vec<String>, HashSet<UntypedAssetId>), &'static str> {
    stock
        .snapshot(view)
        .map_err(|_| "diagnostic_stock_inputs_incomplete")?;
    let mut rows = Vec::new();
    let mut materials = HashSet::default();
    {
        let inputs = lock(&stock.0);
        if let Some(stock) = inputs.views.get(&view) {
            for (entity, material) in &stock.casters {
                let revision = stock
                    .material_inputs
                    .get(material)
                    .ok_or("diagnostic_stock_material_revision_missing")?;
                rows.push(format!("stock_candidate main_entity={} material={material:?} material_revision={revision:?}", entity.to_bits()));
                materials.insert(*material);
            }
        }
    }
    {
        let inputs = lock(&external.1);
        let state = inputs
            .views
            .get(&view)
            .filter(|state| {
                state.frame == frame
                    && state.initialized
                    && state.unknown.is_empty()
                    && state.untracked.is_empty()
            })
            .ok_or("diagnostic_external_material_closure_incomplete")?;
        for material in &state.materials {
            let revision = state
                .revisions
                .get(material)
                .ok_or("diagnostic_external_material_revision_missing")?;
            rows.push(format!("external_material_candidate material={material:?} material_revision={revision:?} membership=view_material_closure_not_individual_source_reference"));
            materials.insert(*material);
        }
    }
    {
        let inputs = lock(&external.0);
        let certificate = inputs
            .get(&view)
            .filter(|input| input.frame == frame && input.complete)
            .ok_or("diagnostic_external_certificate_incomplete")?;
        rows.push(format!(
            "external_certificate frame={} revision={} complete={}",
            certificate.frame, certificate.revision, certificate.complete
        ));
    }
    {
        let producers = lock(&external.2);
        for (slot, producer) in producers.certified.iter().enumerate() {
            if !producers.active[slot] {
                continue;
            }
            let producer = producer
                .as_ref()
                .ok_or("diagnostic_external_command_missing")?;
            let certificate = producer
                .views
                .get(&view)
                .filter(|input| input.frame == frame && input.complete)
                .ok_or("diagnostic_external_command_incomplete")?;
            rows.push(format!(
                "external_command slot={slot} command_type={:?} frame={} revision={} complete={}",
                producer.command, certificate.frame, certificate.revision, certificate.complete
            ));
        }
    }
    Ok((rows, materials))
}

fn candidate_receipt(
    world: &World,
    view: RetainedViewEntity,
    frame: u32,
    reused: bool,
) -> Result<CandidateReceipt, &'static str> {
    let (mut rows, materials) = if reused {
        candidate_membership(
            world.resource::<ShadowStockInputs>(),
            world.resource::<ShadowMapExternalInputs>(),
            view,
            frame,
        )?
    } else {
        refusal_material_rows(world, view, frame)
    };
    let prepared = world.resource::<ErasedRenderAssets<crate::PreparedMaterial>>();
    let images = world.resource::<RenderAssets<GpuImage>>();
    // Join physical textures, not just the default view: custom subviews may
    // sample the same allocation, and several Image assets may alias it.
    let mut textures = HashMap::<_, Vec<String>>::default();
    for (asset, image) in images.iter() {
        textures.entry(&*image.texture).or_default().push(format!(
            "image_asset={asset:?} texture_id={:?}",
            image.texture.id()
        ));
    }
    if let Some(fallback) = world.get_resource::<FallbackImage>() {
        for (name, image) in [
            ("d1", &fallback.d1),
            ("d2", &fallback.d2),
            ("d2_array", &fallback.d2_array),
            ("cube", &fallback.cube),
            ("cube_array", &fallback.cube_array),
            ("d3", &fallback.d3),
        ] {
            textures.entry(&*image.texture).or_default().push(format!(
                "fallback={name} texture_id={:?} image_asset=none",
                image.texture.id()
            ));
        }
    }
    if let Some(image) = world.get_resource::<FallbackImageZero>() {
        textures.entry(&*image.texture).or_default().push(format!(
            "fallback=zero texture_id={:?} image_asset=none",
            image.texture.id()
        ));
    }
    if let Some(image) = world.get_resource::<FallbackImageCubemap>() {
        textures.entry(&*image.texture).or_default().push(format!(
            "fallback=cubemap texture_id={:?} image_asset=none",
            image.texture.id()
        ));
    }
    for aliases in textures.values_mut() {
        aliases.sort();
    }
    for material in materials {
        let Some(material_asset) = prepared.get(material) else {
            if reused {
                return Err("diagnostic_prepared_material_missing");
            }
            rows.push(format!(
                "material_candidate material={material:?} prepared=missing diagnostic_only=true"
            ));
            continue;
        };
        let bindings = match diagnostic_bindings(material_asset) {
            Ok(bindings) => bindings,
            Err(reason) if !reused => {
                rows.push(format!("material_candidate material={material:?} bindings={reason} diagnostic_only=true"));
                continue;
            }
            Err(reason) => return Err(reason),
        };
        rows.push(format!(
            "material_candidate material={material:?} binding={:?} sampled_bindings={}",
            material_asset.binding,
            bindings.len()
        ));
        for (binding, texture_view) in bindings {
            let aliases = textures.get(texture_view.texture());
            rows.push(format!("sampled_texture_candidate material={material:?} binding={binding} texture_view_id={:?} mappings={aliases:?} mapping_status={}", texture_view.id(), if aliases.is_some() { "mapped" } else { "unmapped_fallback_or_external_view_not_an_identified_image_asset" }));
        }
    }
    let phases = world.resource::<ViewBinnedRenderPhases<crate::Shadow>>();
    let phase = phases.get(&view).ok_or("diagnostic_shadow_phase_missing")?;
    let draws = world.resource::<DrawFunctions<crate::Shadow>>().read();
    for ((batch, bin), entries) in &phase.non_mesh_items {
        let command = draws
            .draw_functions
            .get(batch.draw_function.0 as usize)
            .map(|draw| core::any::Any::type_id(draw.as_ref()))
            .ok_or("diagnostic_non_mesh_command_missing")?;
        for (main_entity, entity) in &entries.entities {
            rows.push(format!("non_mesh_candidate command_id={:?} command_type={command:?} render_entity={} carrier_main_entity={} bin_asset={:?} pipeline={:?} source_references=not_individually_identified", batch.draw_function, entity.to_bits(), main_entity.to_bits(), bin.asset_id, batch.pipeline));
        }
    }
    Ok(CandidateReceipt::from_rows(rows))
}

fn diagnostic_bindings(
    material: &crate::PreparedMaterial,
) -> Result<&[(u32, bevy_render::render_resource::TextureView)], &'static str> {
    if material.shadow_cache_untracked_texture {
        return Err("diagnostic_opaque_material_bindings");
    }
    material
        .shadow_cache_texture_views
        .as_deref()
        .ok_or("diagnostic_texture_bindings_not_retained")
}

struct Audit {
    directory: PathBuf,
    request: RequestGate,
    first_audit_frame: Option<u32>,
    observed: HashMap<Allocation, Observation>,
    jobs: Vec<Job>,
    dirty: bool,
    halted: bool,
    deferred: u64,
}

impl Default for Audit {
    fn default() -> Self {
        // A poisoned audit cannot resume a partial comparison or certify a map.
        Self {
            directory: PathBuf::default(),
            request: RequestGate::default(),
            first_audit_frame: None,
            observed: HashMap::default(),
            jobs: Vec::new(),
            dirty: false,
            halted: true,
            deferred: 0,
        }
    }
}

/// `None` is the entire disabled state: no staging, heap state, scan or mapping.
pub(crate) struct ShadowParityAudit(Option<Mutex<Audit>>);

#[derive(Default)]
struct RequestGate {
    sentinel: Option<PathBuf>,
    polled_frame: Option<u32>,
    selected_frame: Option<u32>,
}

impl RequestGate {
    fn poll(&mut self, frame: u32) -> io::Result<Option<u32>> {
        let Some(path) = &self.sentinel else {
            return Ok(Some(frame));
        };
        if let Some(selected) = self.selected_frame {
            return Ok(Some(selected));
        }
        if self.polled_frame == Some(frame) {
            return Ok(None);
        }
        let present = match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => true,
            Ok(_) => return Err(io::Error::other("parity sentinel is not a file")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error),
        };
        Ok(self.select(frame, present))
    }

    fn select(&mut self, frame: u32, present: bool) -> Option<u32> {
        self.polled_frame = Some(frame);
        if present && self.selected_frame.is_none() {
            self.selected_frame = Some(frame);
        }
        self.selected_frame
    }
}

impl ShadowParityAudit {
    pub(super) fn from_settings(settings: Option<&ShadowReuseVerification>) -> Self {
        // GPU resource construction is the sole environment read. Changing it
        // mid-frame cannot change descriptor usages or the early/late decision.
        let paths = verification_paths(settings, || {
            std::env::var_os("MUDCRAB_SHADOW_PARITY_DIR").map(|directory| {
                (
                    PathBuf::from(directory),
                    std::env::var_os("MUDCRAB_GPU_PARITY_REQUEST").map(PathBuf::from),
                )
            })
        });
        let Some((directory, request)) = paths else {
            return Self::disabled();
        };
        let audit = Audit::new(&directory, request)
            .unwrap_or_else(|error| panic!("Shadow reuse verification setup refused: {error}"));
        tracing::warn!(directory = %audit.directory.display(),
            "SHADOW PARITY AUDIT ENABLED: forced raster/copies/readback/I/O contaminate timings; no candidate is INCONCLUSIVE");
        Self(Some(Mutex::new(audit)))
    }
}

fn verification_paths(
    settings: Option<&ShadowReuseVerification>,
    legacy: impl FnOnce() -> Option<(PathBuf, Option<PathBuf>)>,
) -> Option<(PathBuf, Option<PathBuf>)> {
    settings
        .map(|settings| (settings.directory.clone(), Some(settings.request.clone())))
        .or_else(legacy)
}

impl ShadowParityAudit {
    pub(super) fn disabled() -> Self {
        Self(None)
    }

    pub(crate) fn enabled(&self) -> bool {
        self.0.is_some()
    }

    pub(super) fn request_frame(&self, frame: u32) -> Option<u32> {
        let state = self.0.as_ref()?;
        let mut audit = lock(state);
        if audit.halted {
            return None;
        }
        let previous = audit.request.selected_frame;
        match audit.request.poll(frame) {
            Ok(selected) => {
                if audit.request.selected_frame != previous {
                    audit.dirty = true;
                    tracing::warn!(
                        frame,
                        "SHADOW PARITY request latched at actual render FrameCount; paired cull/camera/resource receipts must use this frame"
                    );
                }
                selected
            }
            Err(error) => {
                audit.halted = true;
                audit.dirty = true;
                tracing::error!(%error, frame, "SHADOW PARITY INCOMPLETE: cannot poll requested sentinel; audit halted");
                None
            }
        }
    }

    /// Called inside the existing prepared-cascade loop, never a new view scan.
    pub(crate) fn observe(
        &self,
        view: RetainedViewEntity,
        texture: TextureId,
        slice: u32,
        frame: u32,
    ) {
        if self.request_frame(frame) != Some(frame) {
            return;
        }
        let Some(state) = &self.0 else { return };
        let mut audit = lock(state);
        let allocation = Allocation {
            view,
            texture,
            slice,
        };
        let mut inserted = false;
        audit.observed.entry(allocation).or_insert_with(|| {
            inserted = true;
            Observation {
                first_frame: frame,
                candidate: false,
                outcome: "INCONCLUSIVE_NO_CANDIDATE",
                non_clear_texels: None,
            }
        });
        audit.dirty |= inserted;
    }

    pub(super) fn diagnose(
        &self,
        world: &World,
        entity: Entity,
        view: RetainedViewEntity,
        frame: u32,
        admission: &Result<MapKey, super::Refusal>,
        producers: &Result<super::DrawProducerSnapshot, super::Refusal>,
    ) {
        if self.request_frame(frame) != Some(frame) {
            return;
        }
        let Some(state) = &self.0 else { return };
        let (rows, _) = refusal_material_rows(world, view, frame);
        let mut audit = lock(state);
        let name = format!(
            "refusal-{frame}-{}-{}-{}.txt",
            view.main_entity.to_bits(),
            view.auxiliary_entity.to_bits(),
            view.subview_index
        );
        audit.write(&name, format!("frame={frame}\nshadow_view_entity={}\nretained_view={view:?}\nadmission={admission:#?}\ncommand_producers={producers:?}\n{}\n", entity.to_bits(), rows.join("\n")).as_bytes());
    }

    /// Invoked after the normal early cache decision, regardless of admission.
    #[expect(
        clippy::too_many_arguments,
        reason = "One actual native pass and its cache candidate; no alternate render path"
    )]
    pub(crate) fn begin(
        &self,
        cache: &ShadowMapCache,
        world: &World,
        entity: Entity,
        view: RetainedViewEntity,
        occlusion: bool,
        phase_empty: bool,
        reused: bool,
        ctx: &mut RenderContext,
    ) -> bool {
        let Some(state) = &self.0 else { return false };
        let frame = world.resource::<FrameCount>().0;
        if self.request_frame(frame) != Some(frame) {
            return false;
        }
        let Some(inputs) = world.get::<DirectionalShadowMapInputs>(entity) else {
            return false;
        };
        let allocation = Allocation {
            view,
            texture: inputs.texture,
            slice: inputs.slice,
        };
        let mut audit = lock(state);
        if audit.halted {
            return false;
        }
        let Some(observed) = audit.observed.get_mut(&allocation) else {
            return false;
        };
        if !matches!(
            observed.outcome,
            "INCONCLUSIVE_NO_CANDIDATE" | "INCONCLUSIVE_DEFERRED"
        ) {
            return false;
        }
        observed.candidate = true;
        audit.dirty = true;
        if !reused && occlusion {
            audit.refuse(allocation, "REFUSED_FRESH_OCCLUSION_PAIR_UNSUPPORTED");
            return false;
        }
        let producers = match shadow_draw_producers(
            world.resource::<DrawFunctions<crate::Shadow>>(),
            world.resource::<ShadowMapExternalInputs>(),
            view,
            frame,
        ) {
            Ok(producers) => producers,
            Err(_) => {
                audit.refuse(allocation, "REFUSED_COMMAND_PRODUCER");
                return false;
            }
        };
        let Some((_, retained)) = &world.resource::<RetainedDirectionalShadowTexture>().texture
        else {
            audit.refuse(allocation, "REFUSED_NO_TEXTURE");
            return false;
        };
        let texture = &retained.texture;
        let Some(shadow_view) = world.get::<crate::ShadowView>(entity) else {
            audit.refuse(allocation, "REFUSED_NO_SHADOW_VIEW");
            return false;
        };
        if texture.id() != allocation.texture
            || shadow_view.depth_attachment.view.texture() != &**texture
            || texture.format() != TextureFormat::Depth32Float
            || texture.sample_count() != 1
            || !texture.usage().contains(TextureUsages::COPY_SRC)
            || allocation.slice >= texture.depth_or_array_layers()
        {
            audit.refuse(allocation, "REFUSED_TEXTURE_DESCRIPTOR_OR_ATTACHMENT");
            return false;
        }
        let Some(row_bytes) = texture.width().checked_mul(4) else {
            audit.refuse(allocation, "REFUSED_ROW_OVERFLOW");
            return false;
        };
        let padded_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes as usize) as u32;
        let bytes = u64::from(padded_row_bytes) * u64::from(texture.height());
        let live_bytes: u64 = audit.jobs.iter().map(|job| job.buffers[0].size() * 2).sum();
        if bytes == 0
            || bytes > MAX_STAGING_BYTES / 2
            || bytes > ctx.render_device().limits().max_buffer_size
        {
            audit.refuse(allocation, "REFUSED_STAGING_SIZE");
            return false;
        }
        if audit.jobs.len() >= MAX_JOBS || live_bytes + bytes * 2 > MAX_STAGING_BYTES {
            if let Some(observed) = audit.observed.get_mut(&allocation) {
                observed.outcome = "INCONCLUSIVE_DEFERRED";
            }
            audit.deferred += 1;
            return false; // Retry this exact allocation on a later genuine hit.
        }
        let candidates = match candidate_receipt(world, view, frame, reused) {
            Ok(receipt) => receipt,
            Err(reason) => {
                audit.refuse(allocation, "REFUSED_DEPENDENCY_DIAGNOSTICS");
                tracing::error!(
                    ?allocation,
                    reason,
                    "SHADOW PARITY: dependency candidate snapshot incomplete"
                );
                return false;
            }
        };
        let key = if reused {
            let Some(key) = cache.force_audit_render(view, frame) else {
                audit.refuse(allocation, "REFUSED_CACHE_ATTEMPT");
                return false;
            };
            Some(key)
        } else {
            // The ordinary refused decision stays refused. Same-frame replay
            // needs complete native commands, not reusable material inputs.
            None
        };
        let buffers = ["shadow parity A", "shadow parity B"].map(|label| {
            ctx.render_device().create_buffer(&BufferDescriptor {
                label: Some(label),
                size: bytes,
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        });
        let size = Extent3d {
            width: texture.width(),
            height: texture.height(),
            depth_or_array_layers: 1,
        };
        let texture_number = core::num::NonZero::<u32>::from(allocation.texture).get();
        let stem = format!(
            "{}-{}-{}-{}-{}-{}",
            frame,
            view.main_entity.to_bits(),
            view.auxiliary_entity.to_bits(),
            view.subview_index,
            texture_number,
            allocation.slice
        );
        let input_proof = if reused {
            "producer_revisions_and_native_view_bits_not_independent_GPU_input_byte_snapshot"
        } else {
            "same_extracted_world_readonly_native_emissions_no_reuse_admission"
        };
        let metadata = format!(
            "frame={frame}\nview_entity={}\ncamera_entity={}\nlight_entity={}\nsubview={}\ntexture={}\nslice={}\nmip=0\naspect=DepthOnly\nformat=Depth32Float\nsample_count=1\nwidth={}\nheight={}\narray_layers={}\nusage_bits={}\nlogical_row_bytes={row_bytes}\nstaging_row_bytes={padded_row_bytes}\nraw_row_padding=0\nocclusion={}\nphase_empty={}\ninput_snapshot_before={key:#?}\ninput_proof={input_proof}\nfull_camera_raster_proof=OPEN\ntiming_evidence=false\n",
            entity.to_bits(),
            view.auxiliary_entity.to_bits(),
            view.main_entity.to_bits(),
            view.subview_index,
            texture_number,
            allocation.slice,
            size.width,
            size.height,
            texture.depth_or_array_layers(),
            texture.usage().bits(),
            u8::from(occlusion),
            u8::from(phase_empty),
        );
        audit.first_audit_frame.get_or_insert(frame);
        let metadata = format!(
            "normal_policy={}\ncommand_producers_before={producers:?}\npair_input_boundary=same_extracted_world_and_original_native_attachment_descriptor\n{metadata}",
            if reused { "reuse" } else { "fresh_raster" }
        );
        let metadata = format!(
            "request_frame={frame}\nrequest_sentinel={:?}\n{metadata}",
            audit.request.sentinel
        );
        let metadata = format!(
            "retained_main_entity={}\nretained_auxiliary_entity={}\nattachment_view_id={}\n{metadata}",
            view.main_entity.to_bits(),
            view.auxiliary_entity.to_bits(),
            core::num::NonZero::<u32>::from(shadow_view.depth_attachment.view.id()).get(),
        );
        let candidate_file = format!("{stem}-candidates.txt");
        let metadata = format!(
            "dependency_candidates={candidate_file}\nattribution=candidate_set_not_isolated_culprit\n{metadata}"
        );
        // Both files must be written before copying A. A receipt failure cannot
        // later be reported as an accepted byte match.
        audit.write(
            &candidate_file,
            format!("{metadata}{}", candidates.text).as_bytes(),
        );
        let job = Job {
            allocation,
            frame,
            key,
            entity,
            reused,
            copied_a: reused,
            producers,
            texture: texture.clone(),
            buffers,
            row_bytes,
            padded_row_bytes,
            size,
            occlusion,
            early: false,
            late: false,
            clear_bits: None,
            phase_empty,
            copied_b: false,
            failure: None,
            results: None,
            metadata,
            stem,
            candidate_preview: candidates.preview,
            attachment_view: core::num::NonZero::<u32>::from(
                shadow_view.depth_attachment.view.id(),
            )
            .get(),
        };
        audit.write(
            &format!("{}.txt", job.stem),
            format!(
                "status=INCOMPLETE\nreason=awaiting_native_completion\n{}",
                job.metadata
            )
            .as_bytes(),
        );
        if audit.halted {
            cache.revoke_audit(view, (allocation.texture, allocation.slice));
            return true; // The attempt already transitioned: perform the real render.
        }
        if reused {
            job.copy(0, ctx);
        }
        if let Some(observed) = audit.observed.get_mut(&allocation) {
            observed.outcome = "INCOMPLETE";
        }
        tracing::warn!(
            frame,
            texture = texture_number,
            slice = allocation.slice,
            reused,
            "shadow parity candidate: normal policy selected; actual native depth pair pending (not timing evidence)"
        );
        audit.jobs.push(job);
        true
    }

    pub(crate) fn active(&self, world: &World, view: RetainedViewEntity) -> bool {
        self.0.as_ref().is_some_and(|state| {
            lock(state).jobs.iter().any(|job| {
                job.frame == world.resource::<FrameCount>().0
                    && job.allocation.view == view
                    && !job.copied_b
            })
        })
    }

    pub(crate) fn command_failure(&self, world: &World, view: RetainedViewEntity, message: &str) {
        let Some(state) = &self.0 else { return };
        if let Some(job) = lock(state).jobs.iter_mut().find(|job| {
            job.frame == world.resource::<FrameCount>().0 && job.allocation.view == view
        }) {
            job.failure = Some("native_command_or_pipeline_audit_failed");
            job.metadata
                .push_str(&format!("native_audit_failure={message:?}\n"));
        }
    }

    /// Observe actual attachment operations without consuming another clear.
    pub(crate) fn attachment(
        &self,
        world: &World,
        view: RetainedViewEntity,
        late: bool,
        load: Option<LoadOp<f32>>,
    ) {
        let Some(state) = &self.0 else { return };
        let mut audit = lock(state);
        if let Some(job) = audit.jobs.iter_mut().find(|job| {
            job.allocation.view == view && job.frame == world.resource::<FrameCount>().0
        }) {
            match (late, load) {
                (false, Some(LoadOp::Clear(value))) => job.clear_bits = Some(value.to_bits()),
                (true, Some(LoadOp::Load)) => {}
                _ => job.failure = Some("unexpected_native_depth_load_operation"),
            }
        }
    }

    /// The caller has dropped the tracked render pass and finished the normal
    /// producer/Skip/Failure certification before entering this method.
    pub(crate) fn finish(
        &self,
        cache: &ShadowMapCache,
        world: &World,
        view: RetainedViewEntity,
        late: bool,
        ctx: &mut RenderContext,
    ) -> bool {
        let Some(state) = &self.0 else { return false };
        let mut audit = lock(state);
        let frame = world.resource::<FrameCount>().0;
        let Some(job) = audit
            .jobs
            .iter_mut()
            .find(|job| job.allocation.view == view && job.frame == frame)
        else {
            return false;
        };
        if !job.reused {
            let producers = shadow_draw_producers(
                world.resource::<DrawFunctions<crate::Shadow>>(),
                world.resource::<ShadowMapExternalInputs>(),
                view,
                frame,
            );
            let unchanged = producers
                .as_ref()
                .is_ok_and(|producers| *producers == job.producers);
            let maps = cache.maps();
            let complete = maps.attempts.get(&view).is_some_and(|attempt| {
                attempt.frame == frame && !attempt.skip && !attempt.failed && attempt.early_complete
            });
            drop(maps);
            if late || job.occlusion || !complete || !unchanged || job.clear_bits.is_none() {
                job.failure = Some("fresh_native_pair_incomplete_or_unsupported");
            }
            if job.failure.is_some() {
                cache.revoke_audit(view, (job.allocation.texture, job.allocation.slice));
                return false;
            }
            if !job.copied_a {
                // A is the completed NORMAL raster. Never copy pre-pass pixels.
                job.copy(0, ctx);
                job.copied_a = true;
                if !cache.force_fresh_audit_render(view, frame) {
                    job.failure = Some("fresh_native_replay_transition_failed");
                    cache.revoke_audit(view, (job.allocation.texture, job.allocation.slice));
                    return false;
                }
                job.metadata.push_str("normal_native_complete=1\n");
                return true;
            }
            if job.copied_b {
                job.failure = Some("duplicate_fresh_native_completion");
                cache.revoke_audit(view, (job.allocation.texture, job.allocation.slice));
                return false;
            }
            job.early = true;
            job.copy(1, ctx);
            job.copied_b = true;
            job.metadata.push_str("forced_native_complete=1\n");
            return false;
        }
        if job.key.is_none() {
            job.failure = Some("missing_reused_map_key");
            return false;
        }
        let snapshot = make_key(world, job.entity, view, frame).and_then(|mut key| {
            key.draw_producers = shadow_draw_producers(
                world.resource::<DrawFunctions<crate::Shadow>>(),
                world.resource::<ShadowMapExternalInputs>(),
                view,
                frame,
            )?;
            Ok(key)
        });
        let unchanged = snapshot
            .as_ref()
            .is_ok_and(|key| Some(key) == job.key.as_ref());
        let maps = cache.maps();
        let attempt = maps.attempts.get(&view);
        let early = attempt.is_some_and(|attempt| attempt.frame == frame && attempt.early_complete);
        let failed = attempt.is_none_or(|attempt| attempt.skip || attempt.failed);
        drop(maps);
        if late {
            job.late = true;
        } else {
            job.early = early;
        }
        if failed || !unchanged || job.clear_bits.is_none() {
            job.failure = Some("incomplete_commands_or_changed_dependencies");
        }
        if job.failure.is_some() {
            cache.revoke_audit(view, (job.allocation.texture, job.allocation.slice));
            return false;
        }
        if !completion_ready(job.early, job.late, job.occlusion, failed, unchanged) {
            return false;
        }
        // Verify the exact actual allocation still belongs to this completed
        // attempt. Another camera must not have overwritten the slice.
        let maps = cache.maps();
        let certified = maps
            .owners
            .get(&(job.allocation.texture, job.allocation.slice))
            .is_some_and(|(owner, key)| *owner == view && Some(key) == job.key.as_ref());
        drop(maps);
        if !certified || job.copied_b {
            job.failure = Some("physical_owner_missing_or_duplicate_completion");
            cache.revoke_audit(view, (job.allocation.texture, job.allocation.slice));
            return false;
        }
        job.metadata
            .push_str(&format!("input_snapshot_after={snapshot:#?}\n"));
        job.copy(1, ctx);
        job.copied_b = true;
        false
    }
}

impl Job {
    fn copy(&self, lane: usize, ctx: &mut RenderContext) {
        ctx.command_encoder().copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: 0,
                    y: 0,
                    z: self.allocation.slice,
                },
                aspect: TextureAspect::DepthOnly,
            },
            TexelCopyBufferInfo {
                buffer: &self.buffers[lane],
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row_bytes),
                    rows_per_image: Some(self.size.height),
                },
            },
            self.size,
        );
    }

    fn map(&mut self) {
        let results: MapResults = Arc::new(Mutex::new([None, None]));
        for (lane, buffer) in self.buffers.iter().enumerate() {
            let buffer = buffer.clone();
            let callback_buffer = buffer.clone();
            let results = results.clone();
            let (row, padded, height) = (self.row_bytes, self.padded_row_bytes, self.size.height);
            buffer.slice(..).map_async(MapMode::Read, move |result| {
                let result = result.map_err(|error| error.to_string()).and_then(|()| {
                    let mapped = callback_buffer.slice(..).get_mapped_range();
                    let result = logical_rows(&mapped, row, padded, height).map_err(str::to_owned);
                    drop(mapped);
                    callback_buffer.unmap();
                    result
                });
                lock(&results)[lane] = Some(result);
            });
        }
        self.results = Some(results);
    }
}

impl Audit {
    fn new(root: &Path, sentinel: Option<PathBuf>) -> io::Result<Self> {
        if root.as_os_str().is_empty() {
            return Err(io::Error::other("empty shadow parity output directory"));
        }
        if sentinel
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(io::Error::other("empty shadow parity request path"));
        }
        fs::create_dir_all(root)?;
        owner(root)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let directory = root.join(format!("{}-{nanos}", std::process::id()));
        fs::create_dir(&directory)?;
        owner(&directory)?;
        let mut audit = Self {
            directory,
            request: RequestGate {
                sentinel,
                ..Default::default()
            },
            first_audit_frame: None,
            observed: HashMap::default(),
            jobs: Vec::new(),
            dirty: true,
            halted: false,
            deferred: 0,
        };
        audit.summary();
        if audit.halted {
            return Err(io::Error::other(
                "cannot write initial shadow parity receipt",
            ));
        }
        Ok(audit)
    }

    fn write(&mut self, name: &str, bytes: &[u8]) -> bool {
        match fs::write(self.directory.join(name), bytes) {
            Ok(()) => true,
            Err(error) => {
                self.halted = true;
                tracing::error!(%error, file = name, "SHADOW PARITY INCOMPLETE: receipt/raw write failed; further audits halted");
                false
            }
        }
    }

    fn refuse(&mut self, allocation: Allocation, reason: &'static str) {
        if let Some(observed) = self.observed.get_mut(&allocation) {
            observed.outcome = reason;
        }
        self.dirty = true;
        tracing::error!(?allocation, reason, "SHADOW PARITY REFUSED");
    }

    fn summary(&mut self) {
        let candidates = self
            .observed
            .values()
            .filter(|observation| observation.candidate)
            .count();
        let matched = self
            .observed
            .values()
            .filter(|observation| observation.outcome == "MATCH_BYTES")
            .count();
        let populated = self
            .observed
            .values()
            .filter(|observation| {
                observation.outcome == "MATCH_BYTES"
                    && observation.non_clear_texels.is_some_and(|count| count > 0)
            })
            .count();
        let status = if self.halted {
            "INCOMPLETE_HALTED"
        } else if matched > 0 && matched == self.observed.len() && populated == 0 {
            "MATCH_BYTES_CLEAR_ONLY_INCONCLUSIVE_REPRESENTATIVENESS"
        } else if matched > 0 && matched == self.observed.len() {
            "MATCH_ALL_OBSERVED_SHADOW_BYTES"
        } else {
            "INCONCLUSIVE_OR_INCOMPLETE"
        };
        let mut text = format!(
            "status={status}\nobserved_allocations={}\ncandidates={candidates}\nmatched={matched}\nin_flight={}\ndeferred_attempts={}\nmax_jobs={MAX_JOBS}\nmax_staging_bytes={MAX_STAGING_BYTES}\ntiming_evidence=false\nfull_camera_raster_proof=OPEN\n",
            self.observed.len(),
            self.jobs.len(),
            self.deferred
        );
        text.push_str(&format!(
            "request_sentinel={:?}\nrequest_frame={}\nfirst_audit_frame={}\nmatched_populated={populated}\n",
            self.request.sentinel,
            self.request.selected_frame.map_or_else(|| "none".to_owned(), |frame| frame.to_string()),
            self.first_audit_frame.map_or_else(|| "none".to_owned(), |frame| frame.to_string()),
        ));
        let mut rows: Vec<_> = self.observed.iter().collect();
        rows.sort_by_key(|(allocation, _)| {
            (
                allocation.view.main_entity.to_bits(),
                allocation.view.auxiliary_entity.to_bits(),
                allocation.view.subview_index,
                core::num::NonZero::<u32>::from(allocation.texture).get(),
                allocation.slice,
            )
        });
        for (allocation, observation) in rows {
            text.push_str(&format!("camera={} light={} subview={} texture={} slice={} first_frame={} candidate={} status={}\n",
                allocation.view.auxiliary_entity.to_bits(), allocation.view.main_entity.to_bits(), allocation.view.subview_index,
                core::num::NonZero::<u32>::from(allocation.texture).get(), allocation.slice, observation.first_frame,
                u8::from(observation.candidate), observation.outcome));
        }
        self.write("summary.txt", text.as_bytes());
        self.dirty = false;
    }
}

fn owner(directory: &Path) -> io::Result<()> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("OWNER.txt"))
    {
        Ok(mut file) => file.write_all(
            b"#2153 native shadow parity diagnostic; retain until parent retires evidence.\n",
        ),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

/// Same post-submission mapping pattern as bevy_render::gpu_readback. No poll,
/// wait or mapping happens on the render-pass path or when audit mode is off.
pub(crate) fn cleanup_shadow_parity(cache: Res<ShadowMapCache>, frame: Res<FrameCount>) {
    let Some(state) = &cache.parity().0 else {
        return;
    };
    let mut audit = lock(state);
    if audit.request.selected_frame == Some(frame.0)
        && !audit
            .observed
            .values()
            .any(|observation| observation.candidate)
    {
        tracing::error!(
            frame = frame.0,
            "SHADOW PARITY INCONCLUSIVE: requested frame had no native shadow policy-output pair candidate"
        );
    }
    let mut index = 0;
    while index < audit.jobs.len() {
        let job = &mut audit.jobs[index];
        if job.failure.is_none() && !job.copied_b {
            job.failure = Some("missing_successful_final_early_or_late_pass");
        }
        if job.failure.is_none() && job.results.is_none() {
            job.map();
        }
        let timeout = frame.0.wrapping_sub(job.frame) > MAP_FRAME_LIMIT;
        if timeout {
            job.failure = Some("map_callback_timeout");
        }
        let ready = job
            .results
            .as_ref()
            .is_some_and(|results| lock(results).iter().all(Option::is_some));
        if job.failure.is_none() && !ready {
            index += 1;
            continue;
        }
        let job = audit.jobs.swap_remove(index);
        if timeout {
            audit.halted = true;
        }
        let mut outcome = "INCOMPLETE";
        let mut non_clear_texels = None;
        let mut details = format!(
            "early_complete={}\nlate_complete={}\ncopy_a={}\ncopy_b={}\nclear_bits={}\nphase_empty={}\n",
            u8::from(job.early),
            u8::from(job.late),
            u8::from(job.copied_a),
            u8::from(job.copied_b),
            job.clear_bits
                .map_or_else(|| "missing".to_owned(), |bits| bits.to_string()),
            u8::from(job.phase_empty)
        );
        if let Some(reason) = job.failure {
            cache.revoke_audit(
                job.allocation.view,
                (job.allocation.texture, job.allocation.slice),
            );
            details.push_str(&format!("reason={reason}\n"));
        } else if let Some(results) = &job.results {
            let results = lock(results);
            match (&results[0], &results[1]) {
                (Some(Ok(a)), Some(Ok(b))) => {
                    if let Ok(difference) = compare_bytes(a, b) {
                        let wrote_a = audit.write(&format!("{}-0.bin", job.stem), a);
                        let wrote_b = audit.write(&format!("{}-1.bin", job.stem), b);
                        if wrote_a && wrote_b {
                            outcome = if difference.bytes == 0 {
                                "MATCH_BYTES"
                            } else {
                                "FAIL_BYTES"
                            };
                            details.push_str(&format!(
                                "raw_a={}-0.bin\nraw_b={}-1.bin\n",
                                job.stem, job.stem
                            ));
                        }
                        let non_clear = b
                            .chunks_exact(4)
                            .filter(|texel| {
                                job.clear_bits
                                    .is_some_and(|bits| *texel != bits.to_le_bytes())
                            })
                            .count();
                        non_clear_texels = Some(non_clear);
                        details.push_str(&format!("logical_bytes={}\ndiffering_bytes={}\nfirst_differing_byte={}\nnon_clear_texels={non_clear}\n",
                            a.len(), difference.bytes, difference.first.map_or_else(|| "none".to_owned(), |byte| byte.to_string())));
                        details.push_str(&format!("clear_only={}\n", u8::from(non_clear == 0)));
                    } else {
                        details.push_str("reason=logical_length_mismatch\n");
                    }
                }
                _ => details.push_str(&format!(
                    "reason=map_error\nmap_a={:?}\nmap_b={:?}\n",
                    results[0].as_ref().and_then(|r| r.as_ref().err()),
                    results[1].as_ref().and_then(|r| r.as_ref().err())
                )),
            }
        }
        let report = format!("status={outcome}\n{}{}", job.metadata, details);
        if !audit.write(&format!("{}.txt", job.stem), report.as_bytes()) {
            outcome = "INCOMPLETE";
        }
        if outcome != "MATCH_BYTES" {
            cache.revoke_audit(
                job.allocation.view,
                (job.allocation.texture, job.allocation.slice),
            );
            tracing::error!(
                frame = job.frame,
                texture = core::num::NonZero::<u32>::from(job.allocation.texture).get(),
                slice = job.allocation.slice,
                shadow_view_entity = job.entity.to_bits(),
                retained_view = ?job.allocation.view,
                attachment_view = job.attachment_view,
                outcome,
                receipt = %audit.directory.join(format!("{}.txt", job.stem)).display(),
                dependency_candidates = %audit.directory.join(format!("{}-candidates.txt", job.stem)).display(),
                candidate_preview = %job.candidate_preview,
                "SHADOW PARITY: incomplete/refused/mismatch; candidate set, not an isolated culprit; full IDs and asset mappings in receipt"
            );
        } else {
            tracing::warn!(
                frame = job.frame,
                stem = job.stem,
                "SHADOW PARITY MATCH_BYTES: exact native depth only; timing and full raster acceptance excluded"
            );
        }
        if let Some(observed) = audit.observed.get_mut(&job.allocation) {
            observed.outcome = outcome;
            observed.non_clear_texels = non_clear_texels;
        }
        audit.dirty = true;
    }
    if audit.dirty {
        audit.summary();
    }
}

fn completion_ready(
    early: bool,
    late: bool,
    occlusion: bool,
    failed: bool,
    unchanged: bool,
) -> bool {
    early && (!occlusion || late) && !failed && unchanged
}

fn logical_rows(data: &[u8], row: u32, padded: u32, height: u32) -> Result<Vec<u8>, &'static str> {
    if row == 0 || height == 0 || padded < row {
        return Err("invalid_depth_rows");
    }
    let required = u64::from(padded) * u64::from(height - 1) + u64::from(row);
    if required > data.len() as u64 {
        return Err("truncated_depth_rows");
    }
    let mut logical = Vec::with_capacity(row as usize * height as usize);
    for y in 0..height as usize {
        let start = y * padded as usize;
        logical.extend_from_slice(&data[start..start + row as usize]);
    }
    Ok(logical)
}

#[derive(Debug, PartialEq, Eq)]
struct Difference {
    bytes: usize,
    first: Option<usize>,
}

fn compare_bytes(a: &[u8], b: &[u8]) -> Result<Difference, &'static str> {
    if a.is_empty() || a.len() != b.len() {
        return Err("missing_or_different_logical_bytes");
    }
    let mut difference = Difference {
        bytes: 0,
        first: None,
    };
    for (index, (a, b)) in a.iter().zip(b).enumerate() {
        if a != b {
            difference.bytes += 1;
            difference.first.get_or_insert(index);
        }
    }
    Ok(difference)
}

#[cfg(test)]
#[path = "shadow_cache_parity_tests.rs"]
mod tests;
