//! Diagnostic native raster byte parity, before temporal postprocessing.
//!
//! A uses actual retained cluster buffers; B uses independently forced buffers
//! supplied by the engine for the same extracted frame. Each native pass is
//! emitted twice from one body. B uses independent attachments seeded with A's
//! **initial** contents, and exactly the same clear/load/store operations.
//! Shadow texture identity is outside this audit; see the separate shadow audit.

use crate::{
    render_resource::{Buffer, CachedPipelineState, Pipeline, PipelineCache, RenderPipelineId},
    renderer::{RenderContext, RenderGraph, RenderGraphSystems},
    RenderApp,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use bevy_app::{App, Plugin};
use bevy_diagnostic::FrameCount;
use bevy_ecs::prelude::*;
use bevy_material::descriptor::PipelineDescriptor;
use core::{
    cell::{Cell, RefCell},
    fmt::Write as _,
    time::Duration,
};
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock, PoisonError},
    time::Instant,
};
use wgpu::{
    BindingType, BufferBindingType, BufferDescriptor, BufferUsages, CommandEncoder, Extent3d,
    LoadOp, MapMode, Origin3d, RenderPassColorAttachment, RenderPassDepthStencilAttachment,
    RenderPassDescriptor, ShaderStages, StorageTextureAccess, StoreOp, TexelCopyBufferInfo,
    TexelCopyBufferLayout, Texture, TextureAspect, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages, TextureViewDescriptor,
};

#[derive(Debug, Default)]
struct Config {
    directory: Option<PathBuf>,
    request: Option<PathBuf>,
    min_frame: u32,
}

impl Config {
    fn from_env(mut read: impl FnMut(&str) -> Option<std::ffi::OsString>) -> Result<Self, String> {
        let Some(directory) = read("MUDCRAB_RASTER_PARITY_DIR") else {
            // Ignore all other diagnostic settings unless this audit is enabled.
            return Ok(Self::default());
        };
        if directory.is_empty() {
            return Err("MUDCRAB_RASTER_PARITY_DIR must not be empty".to_owned());
        }
        let min_frame = match read("MUDCRAB_RASTER_PARITY_MIN_FRAME") {
            Some(value) => value
                .into_string()
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .ok_or("MUDCRAB_RASTER_PARITY_MIN_FRAME must be an unsigned 32-bit integer")?,
            None => 0,
        };
        Ok(Self {
            directory: Some(PathBuf::from(directory)),
            request: read("MUDCRAB_GPU_PARITY_REQUEST").map(PathBuf::from),
            min_frame,
        })
    }
}

fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        Config::from_env(|name| std::env::var_os(name)).unwrap_or_else(|error| {
            bevy_log::error!("NATIVE RASTER PARITY FAILED: {error}; audit disabled");
            Config::default()
        })
    })
}

/// Adds copy capabilities only in an explicitly enabled diagnostic process.
pub fn raster_parity_copy_usage(samples: u32) -> TextureUsages {
    if samples == 1 && config().directory.is_some() {
        TextureUsages::COPY_SRC
    } else {
        TextureUsages::empty()
    }
}

/// Which concrete buffer snapshot the native emission currently consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RasterParityLane {
    Retained,
    Forced,
}

thread_local! {
    static LANE: Cell<Option<(Entity, RasterParityLane)>> = const { Cell::new(None) };
    static PIPELINES: RefCell<Option<PipelineScope>> = const { RefCell::new(None) };
}

thread_local! {
    static NATIVE_AUDIT: RefCell<Option<NativeAuditScope>> = const { RefCell::new(None) };
}

struct NativeAuditScope {
    view: Entity,
    allowed: Vec<RenderPipelineId>,
    error: Arc<Mutex<Option<String>>>,
}

/// Diagnostic scope for native emitters that own their own attachment pairing.
/// Validates actual pipeline binds, raw-pass escape, Skip and missing commands.
/// Does not select raster buffer lanes or grant any retained-result eligibility.
pub struct NativeRenderAudit {
    previous: Option<NativeAuditScope>,
    error: Arc<Mutex<Option<String>>>,
    // A thread-local scope must be restored on the thread that entered it.
    _thread: core::marker::PhantomData<*mut ()>,
}

impl NativeRenderAudit {
    /// Call only for a selected diagnostic frame: this scans ready descriptors.
    pub fn begin(world: &World, view: Entity) -> Self {
        Self::enter(view, read_only_pipelines(world))
    }

    fn enter(view: Entity, allowed: Vec<RenderPipelineId>) -> Self {
        let error = Arc::new(Mutex::new(None));
        let previous = NATIVE_AUDIT.with(|slot| {
            slot.replace(Some(NativeAuditScope {
                view,
                allowed,
                error: Arc::clone(&error),
            }))
        });
        Self {
            previous,
            error,
            _thread: core::marker::PhantomData,
        }
    }

    /// None means every observed native command and pipeline was admitted.
    pub fn failure(&self) -> Option<String> {
        self.error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for NativeRenderAudit {
    fn drop(&mut self) {
        NATIVE_AUDIT.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}

/// Whether this view currently emits under a native diagnostic command scope.
pub(crate) fn native_render_audit_active(view: Entity) -> bool {
    NATIVE_AUDIT.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|scope| scope.view == view)
    })
}

fn native_render_audit_error(view: Entity, message: &str) {
    NATIVE_AUDIT.with(|slot| {
        if let Some(scope) = slot.borrow().as_ref().filter(|scope| scope.view == view) {
            scope
                .error
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert_with(|| message.to_owned());
        }
    });
}

struct PipelineScope {
    allowed: Vec<RenderPipelineId>,
    state: Arc<Mutex<State>>,
}

struct PipelineGuard(Option<PipelineScope>);

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        PIPELINES.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}

// Called at the actual tracked-pass boundary, including raw-pass escape. This
// closes the gap between inspecting cached descriptors and pipelines really
// bound by custom native draw commands. No allocation/scan outside audit scope.
pub(crate) fn validate_raster_pipeline(pipeline: Option<RenderPipelineId>) {
    NATIVE_AUDIT.with(|slot| {
        if let Some(scope) = slot.borrow().as_ref()
            && !pipeline.is_some_and(|pipeline| scope.allowed.contains(&pipeline))
        {
            scope
                .error
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert_with(|| {
                    format!("Unverified native render pipeline/raw pass access: {pipeline:?}")
                });
        }
    });
    PIPELINES.with(|slot| {
        let scope = slot.borrow();
        let Some(scope) = scope.as_ref() else { return };
        if pipeline.is_some_and(|pipeline| scope.allowed.contains(&pipeline)) {
            return;
        }
        let error = format!("Unverified native raster pipeline/raw pass access: {pipeline:?}");
        bevy_log::error!("NATIVE RASTER PARITY FAILED: {error}");
        let mut state = scope.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.error.get_or_insert(error);
        write_summary(&mut state);
    });
}

/// Returns a lane only while this view's paired native emitter is running.
pub fn raster_parity_lane(view: Entity) -> Option<RasterParityLane> {
    LANE.with(|slot| {
        slot.get()
            .filter(|(entity, _)| *entity == view)
            .map(|(_, lane)| lane)
    })
}

/// Selects the engine's independent audit early/late suppression ledger.
pub fn raster_parity_forced_lane(view: Entity) -> bool {
    raster_parity_lane(view) == Some(RasterParityLane::Forced)
}

struct LaneGuard(Option<(Entity, RasterParityLane)>);

impl LaneGuard {
    fn enter(view: Entity, lane: RasterParityLane) -> Self {
        Self(LANE.with(|slot| slot.replace(Some((view, lane)))))
    }
}

impl Drop for LaneGuard {
    fn drop(&mut self) {
        LANE.with(|slot| slot.set(self.0));
    }
}

/// Registers the engine's real live/retained/forced buffer triples per view.
///
/// Inserted by [`RasterParityPlugin`]. No snapshots, copies, attachment scans or
/// file accesses occur with `MUDCRAB_RASTER_PARITY_DIR` unset. Environment is read
/// once. With a request sentinel, polling is diagnostic-only. Only registration
/// latches the actual extracted `FrameCount`, never a camera-side sentinel poll.
#[derive(Resource, Default)]
pub struct RasterParity {
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    frame: Option<u32>,
    render_frame: Option<u32>,
    views: BTreeMap<u64, RegisteredView>,
    pending: Vec<Readback>,
    outstanding: usize,
    completed: usize,
    sealed: bool,
    error: Option<String>,
    started: Option<Instant>,
    finished: bool,
}

struct RegisteredView {
    buffers: Vec<(Buffer, Buffer, Buffer)>,
    passes: BTreeSet<String>,
    attachments: BTreeSet<String>,
    late_required: bool,
}

impl RasterParity {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Eligibility predicate only: this does not select an audit frame. The
    /// producer checks its shared request gate before encoding culls, produces
    /// actual retained/forced snapshots, then arms that frame via registration.
    pub fn requested(&self, frame: u32) -> bool {
        let cfg = config();
        if cfg.directory.is_none() {
            return false;
        }
        let state = self.lock();
        if let Some(selected) = state.frame {
            return selected == frame && !state.sealed;
        }
        if frame < cfg.min_frame || cfg.request.as_ref().is_some_and(|path| !path.is_file()) {
            return false;
        }
        true
    }

    /// Whether cull buffer registration armed this exact extracted frame.
    /// Native camera passes use this, never independently select via sentinel.
    pub fn is_armed(&self, frame: u32) -> bool {
        if config().directory.is_none() {
            return false;
        }
        let state = self.lock();
        state.frame == Some(frame) && !state.sealed
    }

    /// Supply **all** writable cluster output buffers, in `(live, retained,
    /// forced)` order, after snapshot commands and before native camera passes.
    /// Snapshots must originate from a genuine cache hit and actual forced cull
    /// in this frame; distinct buffer handles alone do not establish provenance.
    /// The engine's separate cull receipts establish that provenance.
    pub fn register_view(
        &self,
        frame: u32,
        view: Entity,
        buffers: Vec<(Buffer, Buffer, Buffer)>,
    ) -> Result<(), String> {
        if !self.requested(frame) {
            return Err(format!(
                "Raster parity registration outside selected frame: {frame}"
            ));
        }
        let validate = || {
            if buffers.is_empty() {
                return Err("Missing retained/forced cluster buffer triples".to_owned());
            }
            let mut handles: Vec<&wgpu::Buffer> = Vec::new();
            for (live, retained, forced) in &buffers {
                if live.size() == 0
                    || !live.size().is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
                    || retained.size() != live.size()
                    || forced.size() != live.size()
                    || !live.usage().contains(BufferUsages::COPY_DST)
                    || !retained.usage().contains(BufferUsages::COPY_SRC)
                    || !forced.usage().contains(BufferUsages::COPY_SRC)
                {
                    return Err("Invalid or aliased raster parity buffer triple".to_owned());
                }
                // No live or snapshot may alias another output's storage.
                for buffer in [live, retained, forced] {
                    if handles.contains(&&**buffer) {
                        return Err("Aliased raster parity live/snapshot buffer".to_owned());
                    }
                    handles.push(buffer);
                }
            }
            Ok(())
        };
        if let Err(error) = validate() {
            self.fail(&error);
            return Err(error);
        }
        let mut state = self.lock();
        if state.frame.is_none() {
            state.frame = Some(frame);
            bevy_log::info!(
                frame,
                "Native raster parity armed by actual same-frame cull buffer pairs"
            );
        }
        if state.views.contains_key(&view.to_bits()) {
            let error = format!("Duplicate raster parity registration for {view:?}");
            drop(state);
            self.fail(&error);
            return Err(error);
        }
        state.views.insert(
            view.to_bits(),
            RegisteredView {
                buffers,
                passes: BTreeSet::new(),
                attachments: BTreeSet::new(),
                late_required: false,
            },
        );
        Ok(())
    }

    /// Records an audit failure without weakening or skipping ordinary rendering.
    pub fn fail(&self, message: &str) {
        bevy_log::error!("NATIVE RASTER PARITY FAILED: {message}");
        let mut state = self.lock();
        state.error.get_or_insert_with(|| message.to_owned());
        write_summary(&mut state);
    }
}

/// Installs the diagnostic resource and post-submission asynchronous map stage.
pub struct RasterParityPlugin;

impl Plugin for RasterParityPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<RasterParity>()
                .add_systems(RenderGraph, finish_frame.in_set(RenderGraphSystems::Finish));
        }
    }
}

/// Announces the native late-prepass predicate even when the pass is absent.
pub fn raster_parity_expect_late(world: &World, view: Entity, required: bool) {
    let Some(parity) = world.get_resource::<RasterParity>() else {
        return;
    };
    if !parity.is_armed(world.resource::<FrameCount>().0) {
        return;
    }
    if let Some(registered) = parity.lock().views.get_mut(&view.to_bits()) {
        registered.late_required = required;
    }
}

/// Propagates an actual native draw failure into the active audit receipt.
pub fn raster_parity_draw_error(world: &World, view: Entity, message: &str) {
    native_render_audit_error(view, message);
    if raster_parity_lane(view).is_some()
        && let Some(parity) = world.get_resource::<RasterParity>()
    {
        parity.fail(message);
    }
}

struct Scratch {
    texture: crate::render_resource::Texture,
    view: crate::render_resource::TextureView,
}

impl Scratch {
    fn new(ctx: &mut RenderContext, source: &Texture, load: bool) -> Result<Self, String> {
        if source.sample_count() != 1
            || source.dimension() != TextureDimension::D2
            || source.depth_or_array_layers() != 1
            || source.mip_level_count() != 1
            || !source.usage().contains(TextureUsages::COPY_SRC)
        {
            return Err("Unsupported MSAA/subresource or missing attachment COPY_SRC".to_owned());
        }
        if !matches!(
            source.format(),
            TextureFormat::Depth32Float
                | TextureFormat::Rgba16Float
                | TextureFormat::Rg16Float
                | TextureFormat::Rgb10a2Unorm
                | TextureFormat::Rgba8Unorm
        ) {
            return Err(format!(
                "Unsupported raw raster attachment {:?}",
                source.format()
            ));
        }
        let texture = ctx.render_device().create_texture(&TextureDescriptor {
            label: Some("native_raster_parity_scratch"),
            size: source.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: source.dimension(),
            format: source.format(),
            usage: source.usage() | TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        if load {
            ctx.command_encoder().copy_texture_to_texture(
                source.as_image_copy(),
                texture.as_image_copy(),
                source.size(),
            );
        }
        let view = texture.create_view(&TextureViewDescriptor::default());
        Ok(Self { texture, view })
    }
}

// Only explicit, read-only, ready render pipelines may be used during replay.
// Validation at the actual tracked-pass bind rejects writable/unknown pipelines
// and raw-pass escape, without rejecting unrelated unused cached pipelines.
fn read_only_render_layout(layouts: &[crate::render_resource::BindGroupLayoutDescriptor]) -> bool {
    !layouts.is_empty()
        && !layouts.iter().any(|layout| {
            layout.entries.iter().any(|entry| {
                entry
                    .visibility
                    .intersects(ShaderStages::VERTEX | ShaderStages::FRAGMENT)
                    && matches!(
                        entry.ty,
                        BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: false },
                            ..
                        } | BindingType::StorageTexture {
                            access: StorageTextureAccess::WriteOnly
                                | StorageTextureAccess::ReadWrite
                                | StorageTextureAccess::Atomic,
                            ..
                        }
                    )
            })
        })
}

fn read_only_pipelines(world: &World) -> Vec<RenderPipelineId> {
    world
        .resource::<PipelineCache>()
        .pipelines()
        .filter_map(|cached| {
            let PipelineDescriptor::RenderPipelineDescriptor(descriptor) = &cached.descriptor
            else {
                return None;
            };
            let CachedPipelineState::Ok(Pipeline::RenderPipeline(pipeline)) = &cached.state else {
                return None;
            };
            if !read_only_render_layout(&descriptor.layout) {
                return None;
            }
            Some(pipeline.id())
        })
        .collect()
}

/// Runs the responsible native emission once normally, twice only in the chosen
/// audit frame. Native attachment getters must be called **once** by the caller;
/// their first-use clear/load atomics must not be consumed again for B.
pub fn raster_parity_pass(
    world: &World,
    view: Entity,
    ctx: &mut RenderContext,
    descriptor: RenderPassDescriptor<'_>,
    mut emit: impl FnMut(&mut RenderContext, RenderPassDescriptor<'_>),
) {
    let Some(parity) = world.get_resource::<RasterParity>() else {
        emit(ctx, descriptor);
        return;
    };
    let frame = world.resource::<FrameCount>().0;
    if !parity.is_armed(frame) {
        emit(ctx, descriptor);
        return;
    }
    let label = descriptor.label.unwrap_or("unnamed native pass");
    let prepared = (|| {
        if ![
            "early prepass",
            "late prepass",
            "main_opaque_pass_3d",
            "main_transparent_pass_3d",
        ]
        .contains(&label)
        {
            return Err("Unknown native raster emission boundary".to_owned());
        }
        let pipelines = read_only_pipelines(world);
        if descriptor.timestamp_writes.is_some()
            || descriptor.occlusion_query_set.is_some()
            || descriptor.multiview_mask.is_some()
            || descriptor.depth_stencil_attachment.is_none()
        {
            return Err(
                "Unsupported pass queries/timestamps/multiview or missing native depth".to_owned(),
            );
        }
        let mut state = parity.lock();
        let registered = state.views.get_mut(&view.to_bits()).ok_or_else(|| {
            format!("Missing same-frame retained/forced buffer pair for {view:?}")
        })?;
        if !registered.passes.insert(label.to_owned()) {
            return Err(format!("Duplicate native pass {label} for {view:?}"));
        }
        let buffers = registered.buffers.clone();
        drop(state);
        let mut colors = Vec::with_capacity(descriptor.color_attachments.len());
        for (index, attachment) in descriptor.color_attachments.iter().enumerate() {
            colors.push(if let Some(attachment) = attachment {
                if index >= 2 || (label.starts_with("main_") && index != 0) {
                    return Err("Unsupported native raster color attachment slot".to_owned());
                }
                if attachment.resolve_target.is_some()
                    || attachment.depth_slice.is_some()
                    || attachment.ops.store != StoreOp::Store
                {
                    return Err("Unsupported resolve/depth-slice/discard attachment".to_owned());
                }
                if label.starts_with("main_")
                    && attachment.view.texture().format() != TextureFormat::Rgba16Float
                {
                    return Err(
                        "Main parity boundary requires native linear Rgba16Float HDR".to_owned(),
                    );
                }
                Some(Scratch::new(
                    ctx,
                    attachment.view.texture(),
                    matches!(attachment.ops.load, LoadOp::Load),
                )?)
            } else {
                None
            });
        }
        let depth = if let Some(attachment) = &descriptor.depth_stencil_attachment {
            if attachment.stencil_ops.is_some()
                || attachment
                    .depth_ops
                    .is_none_or(|ops| ops.store != StoreOp::Store)
            {
                return Err("Unsupported stencil/depth discard".to_owned());
            }
            Some(Scratch::new(
                ctx,
                attachment.view.texture(),
                attachment
                    .depth_ops
                    .is_some_and(|ops| matches!(ops.load, LoadOp::Load)),
            )?)
        } else {
            None
        };
        Ok((buffers, colors, depth, pipelines))
    })();
    let (buffers, colors, depth, pipelines) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            parity.fail(&error);
            emit(ctx, descriptor);
            return;
        }
    };
    let _pipeline_guard = PipelineGuard(PIPELINES.with(|slot| {
        slot.replace(Some(PipelineScope {
            allowed: pipelines,
            state: Arc::clone(&parity.state),
        }))
    }));

    let viewport = bevy_camera::Viewport::from_viewport_and_override(
        world
            .get::<crate::camera::ExtractedCamera>(view)
            .and_then(|camera| camera.viewport.as_ref()),
        world.get::<bevy_camera::MainPassResolutionOverride>(view),
    );

    let scratch_colors: Vec<_> = descriptor
        .color_attachments
        .iter()
        .zip(&colors)
        .map(|(original, scratch)| {
            original
                .as_ref()
                .zip(scratch.as_ref())
                .map(|(original, scratch)| RenderPassColorAttachment {
                    view: &scratch.view,
                    depth_slice: original.depth_slice,
                    resolve_target: None,
                    ops: original.ops,
                })
        })
        .collect();
    let scratch_depth = descriptor
        .depth_stencil_attachment
        .as_ref()
        .zip(depth.as_ref())
        .map(|(original, scratch)| RenderPassDepthStencilAttachment {
            view: &scratch.view,
            depth_ops: original.depth_ops,
            stencil_ops: original.stencil_ops,
        });

    select_buffers(ctx.command_encoder(), &buffers, RasterParityLane::Retained);
    {
        let _guard = LaneGuard::enter(view, RasterParityLane::Retained);
        emit(ctx, descriptor.clone());
    }
    // A custom command may expose an unknown raw pipeline, or a native phase
    // may fail. Preserve A's ordinary render and refuse replay of untracked
    // writable state. The failed receipt can never become an equality result.
    if parity.lock().error.is_some() {
        select_buffers(ctx.command_encoder(), &buffers, RasterParityLane::Retained);
        return;
    }
    select_buffers(ctx.command_encoder(), &buffers, RasterParityLane::Forced);
    {
        let _guard = LaneGuard::enter(view, RasterParityLane::Forced);
        emit(
            ctx,
            RenderPassDescriptor {
                label: descriptor.label,
                color_attachments: &scratch_colors,
                depth_stencil_attachment: scratch_depth,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            },
        );
    }
    select_buffers(ctx.command_encoder(), &buffers, RasterParityLane::Retained);

    for (index, (original, scratch)) in descriptor.color_attachments.iter().zip(&colors).enumerate()
    {
        if let Some((original, scratch)) = original.as_ref().zip(scratch.as_ref()) {
            stage_readback(
                parity,
                ctx,
                frame,
                view,
                label,
                match (label.starts_with("main_"), index) {
                    (true, 0) => "linear-hdr-color",
                    (false, 0) => "normal",
                    (false, 1) => "motion",
                    _ => "unsupported-color-slot",
                },
                original.view.texture(),
                &scratch.texture,
                &format!("{:?}; viewport={viewport:?}", original.ops),
            );
        }
    }
    if let Some((original, scratch)) = descriptor
        .depth_stencil_attachment
        .as_ref()
        .zip(depth.as_ref())
    {
        stage_readback(
            parity,
            ctx,
            frame,
            view,
            label,
            "depth",
            original.view.texture(),
            &scratch.texture,
            &format!("{:?}; viewport={viewport:?}", original.depth_ops),
        );
    }
}

fn select_buffers(
    encoder: &mut CommandEncoder,
    buffers: &[(Buffer, Buffer, Buffer)],
    lane: RasterParityLane,
) {
    for (live, retained, forced) in buffers {
        let source = match lane {
            RasterParityLane::Retained => retained,
            RasterParityLane::Forced => forced,
        };
        encoder.copy_buffer_to_buffer(source, 0, live, 0, live.size());
    }
}

struct Readback {
    buffer: Buffer,
    stem: PathBuf,
    metadata: String,
    row_bytes: usize,
    padded_row_bytes: usize,
    rows: usize,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Concrete native attachment receipt"
)]
fn stage_readback(
    parity: &RasterParity,
    ctx: &mut RenderContext,
    frame: u32,
    view: Entity,
    pass: &str,
    attachment: &str,
    a: &Texture,
    b: &Texture,
    operations: &str,
) {
    let Some(directory) = &config().directory else {
        return;
    };
    let aspect = if a.format() == TextureFormat::Depth32Float {
        TextureAspect::DepthOnly
    } else {
        TextureAspect::All
    };
    let Some(texel_bytes) = a.format().block_copy_size(Some(aspect)) else {
        parity.fail("Attachment has no exact native copy representation");
        return;
    };
    let Extent3d { width, height, .. } = a.size();
    let row_bytes = width as usize * texel_bytes as usize;
    let padded_row_bytes = crate::renderer::RenderDevice::align_copy_bytes_per_row(row_bytes);
    let lane_size = padded_row_bytes as u64 * u64::from(height);
    let buffer = ctx.render_device().create_buffer(&BufferDescriptor {
        label: Some("native_raster_parity_readback_pair"),
        size: lane_size * 2,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    for (source, offset) in [(a, 0), (b, lane_size)] {
        ctx.command_encoder().copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect,
            },
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset,
                    bytes_per_row: Some(padded_row_bytes as u32),
                    rows_per_image: Some(height),
                },
            },
            a.size(),
        );
    }
    let pass_file = pass.replace(' ', "-");
    let stem = directory.join(format!(
        "frame-{frame}-view-{}-{pass_file}-{attachment}",
        view.to_bits()
    ));
    let metadata = format!("\"kind\":\"raster_attachment\",\"frame\":{frame},\"view\":{},\"pass\":{},\"attachment\":{},\"format\":{},\"width\":{width},\"height\":{height},\"samples\":1,\"layers\":1,\"mip\":0,\"row_bytes\":{row_bytes},\"copy_row_bytes\":{padded_row_bytes},\"operations\":{},\"boundary\":\"native pass output before temporal postprocess; main color is linear HDR\",\"shadow_texture_identity_proven\":false", view.to_bits(), json_string(pass), json_string(attachment), json_string(&format!("{:?}", a.format())), json_string(operations));
    let mut state = parity.lock();
    if let Some(registered) = state.views.get_mut(&view.to_bits()) {
        registered
            .attachments
            .insert(format!("{pass}/{attachment}"));
    }
    state.outstanding += 1;
    state.pending.push(Readback {
        buffer,
        stem,
        metadata,
        row_bytes,
        padded_row_bytes,
        rows: height as usize,
    });
}

fn finish_frame(parity: Res<RasterParity>, frame: Res<FrameCount>) {
    if config().directory.is_none() {
        return;
    }
    let pending = {
        let mut state = parity.lock();
        if state.frame.is_none() || state.finished {
            return;
        }
        if !state.sealed {
            state.sealed = true;
            state.render_frame = Some(frame.0);
            state.started = Some(Instant::now());
            if state.frame != Some(frame.0) {
                let error = format!(
                    "Registered raster frame {:?} does not match actual render FrameCount {}",
                    state.frame, frame.0
                );
                bevy_log::error!("NATIVE RASTER PARITY FAILED: {error}");
                state.error.get_or_insert(error);
            }
            let missing = state.views.is_empty()
                || state.views.values().any(|view| {
                    ![
                        "early prepass",
                        "main_opaque_pass_3d",
                        "main_transparent_pass_3d",
                    ]
                    .iter()
                    .all(|pass| view.passes.contains(*pass))
                        || (view.late_required && !view.passes.contains("late prepass"))
                });
            if missing || state.outstanding == 0 {
                state
                    .error
                    .get_or_insert_with(|| "Incomplete native camera pass/buffer pairs".to_owned());
                bevy_log::error!("NATIVE RASTER PARITY FAILED: incomplete/missing camera pair");
            }
            write_summary(&mut state);
        }
        if state
            .started
            .is_some_and(|started| started.elapsed() > Duration::from_secs(30))
            && state.outstanding > 0
        {
            state.error.get_or_insert_with(|| {
                "Native raster readback timed out after 30 seconds".to_owned()
            });
            bevy_log::error!(
                "NATIVE RASTER PARITY FAILED: mapped readback incomplete after 30 seconds"
            );
            write_summary(&mut state);
            state.finished = true;
        }
        core::mem::take(&mut state.pending)
    };
    // RenderGraph Finish runs after the core pipeline submits all encoders.
    for readback in pending {
        let buffer = readback.buffer.clone();
        let state = Arc::clone(&parity.state);
        buffer.slice(..).map_async(MapMode::Read, move |result| {
            let outcome = result
                .map_err(|error| error.to_string())
                .and_then(|()| save_readback(&readback));
            readback.buffer.unmap();
            let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
            state.outstanding -= 1;
            state.completed += 1;
            if let Err(error) = outcome {
                bevy_log::error!("NATIVE RASTER PARITY FAILED: {error}");
                state.error.get_or_insert(error);
            }
            write_summary(&mut state);
        });
    }
}

fn save_readback(readback: &Readback) -> Result<(), String> {
    let slice = readback.buffer.slice(..);
    let mapped = slice.get_mapped_range();
    let lane_size = readback.padded_row_bytes * readback.rows;
    let a = unpack_rows(
        &mapped[..lane_size],
        readback.row_bytes,
        readback.padded_row_bytes,
    );
    let b = unpack_rows(
        &mapped[lane_size..],
        readback.row_bytes,
        readback.padded_row_bytes,
    );
    drop(mapped);
    let first_difference = a.iter().zip(&b).position(|(a, b)| a != b);
    let differences = a.iter().zip(&b).filter(|(a, b)| a != b).count();
    let equal = a == b;
    let directory = readback
        .stem
        .parent()
        .ok_or("Missing raster receipt directory")?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    fs::write(directory.join("OWNER.txt"), "Owner: #2153 native raster byte parity diagnostic. Keep raw bytes and receipts together.\n").map_err(|error| error.to_string())?;
    let filename = readback
        .stem
        .file_name()
        .ok_or("Missing raster receipt name")?
        .to_string_lossy();
    fs::write(
        readback
            .stem
            .with_file_name(format!("{filename}-retained.bin")),
        &a,
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        readback
            .stem
            .with_file_name(format!("{filename}-forced.bin")),
        &b,
    )
    .map_err(|error| error.to_string())?;
    let receipt = format!("{{{},\"retained_file\":{},\"forced_file\":{},\"bytes_per_lane\":{},\"different_bytes\":{differences},\"first_difference\":{},\"equal\":{equal},\"tolerance\":0}}\n", readback.metadata, json_string(&format!("{filename}-retained.bin")), json_string(&format!("{filename}-forced.bin")), a.len(), first_difference.map_or_else(|| "null".to_owned(), |index| index.to_string()));
    fs::write(readback.stem.with_extension("json"), receipt).map_err(|error| error.to_string())?;
    if !equal {
        return Err(format!(
            "{} differs in {differences} native bytes",
            readback.stem.display()
        ));
    }
    Ok(())
}

fn unpack_rows(bytes: &[u8], row_bytes: usize, stride: usize) -> Vec<u8> {
    bytes
        .chunks_exact(stride)
        .flat_map(|row| row[..row_bytes].iter().copied())
        .collect()
}

fn json_string(value: &str) -> String {
    let mut result = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(result, "\\u{:04x}", ch as u32);
            }
            ch => result.push(ch),
        }
    }
    result.push('"');
    result
}

fn write_summary(state: &mut State) {
    let (Some(directory), Some(frame)) = (&config().directory, state.frame) else {
        return;
    };
    let success =
        state.sealed && state.outstanding == 0 && state.completed > 0 && state.error.is_none();
    let status = if state.error.is_some() {
        "failed"
    } else if success {
        "equal"
    } else {
        "incomplete"
    };
    let views = state
        .views
        .iter()
        .map(|(view, registered)| {
            format!(
                "{{\"view\":{view},\"buffer_triples\":{},\"buffers\":[{}],\"passes\":[{}],\"attachments\":[{}],\"late_required\":{}}}",
                registered.buffers.len(),
                registered.buffers.iter().map(|(live, retained, forced)| format!("{{\"live\":{},\"retained\":{},\"forced\":{},\"bytes\":{}}}", json_string(&format!("{:?}", live.id())), json_string(&format!("{:?}", retained.id())), json_string(&format!("{:?}", forced.id())), live.size())).collect::<Vec<_>>().join(","),
                registered
                    .passes
                    .iter()
                    .map(|pass| json_string(pass))
                    .collect::<Vec<_>>()
                    .join(","),
                registered.attachments.iter().map(|attachment| json_string(attachment)).collect::<Vec<_>>().join(","),
                registered.late_required
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let receipt = format!("{{\"kind\":\"raster_frame\",\"frame\":{frame},\"render_frame\":{},\"status\":{},\"completed_attachment_pairs\":{},\"outstanding_attachment_pairs\":{},\"views\":[{views}],\"error\":{},\"tolerance\":0,\"shadow_texture_identity_proven\":false}}\n", state.render_frame.map_or_else(|| "null".to_owned(), |frame| frame.to_string()), json_string(status), state.completed, state.outstanding, state.error.as_ref().map_or_else(|| "null".to_owned(), |error| json_string(error)));
    let result = fs::create_dir_all(directory).and_then(|()| fs::write(directory.join("OWNER.txt"), "Owner: #2153 native raster byte parity diagnostic. Keep raw bytes and receipts together.\n")).and_then(|()| fs::write(directory.join(format!("frame-{frame}.json")), receipt));
    if let Err(error) = result {
        bevy_log::error!("NATIVE RASTER PARITY FAILED: cannot persist receipt: {error}");
        state.error.get_or_insert_with(|| error.to_string());
    }
    if success && state.error.is_none() {
        state.finished = true;
        bevy_log::info!(
            frame,
            pairs = state.completed,
            "Native raster parity exact byte equality; shadow textures audited separately"
        );
    }
}

#[cfg(test)]
#[path = "render_parity_tests.rs"]
mod tests;
