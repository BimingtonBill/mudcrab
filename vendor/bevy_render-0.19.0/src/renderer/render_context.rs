use super::WgpuWrapper;
use crate::diagnostic::internal::DiagnosticsRecorder;
use crate::render_phase::TrackedRenderPass;
use crate::render_resource::{CommandEncoder, RenderPassDescriptor};
use crate::renderer::RenderDevice;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::change_detection::Tick;
use bevy_ecs::component::ComponentId;
use bevy_ecs::prelude::*;
use bevy_ecs::query::{FilteredAccessSet, QueryData, QueryFilter, QueryState};
use bevy_ecs::system::{
    Deferred, SystemBuffer, SystemMeta, SystemParam, SystemParamValidationError,
};
use bevy_ecs::world::unsafe_world_cell::UnsafeWorldCell;
use bevy_ecs::world::DeferredWorld;
#[cfg(feature = "trace")]
use bevy_log::info_span;
use bevy_tasks::ComputeTaskPool;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use wgpu::CommandBuffer;

// MUDCRAB PATCH (impl-1132, analysis-1131; see MUDCRAB-README.md at the crate root). Everything in this file
// that mentions `Pending`, `PARALLEL_FINISH`, `LAST_PARALLEL_FINISH_COUNT`, `finish_or_defer`, `finish_pending` or
// `finish_in_order` is Mudcrab's; the rest is stock bevy_render 0.19.0.

/// Whether render systems hand their command encoders over unfinished, so that
/// [`PendingCommandBuffers::take`] can finish them in parallel. Off (the default) is stock Bevy.
static PARALLEL_FINISH: AtomicBool = AtomicBool::new(false);

/// Turns parallel finishing of render command encoders on or off (Mudcrab patch).
///
/// With it off, every encoder is finished where Bevy always finished it: when its render system's deferred
/// buffer is applied. With it on, the encoder is queued unfinished, in the same position of the same ordered
/// list, and all unfinished encoders are finished together on the compute task pool when the pending command
/// buffers are taken for submission. The buffers come back in exactly the order they would have had.
///
/// Set it once at startup, before the first frame; changing it mid-run is safe but only affects later frames.
pub fn set_parallel_finish(enabled: bool) {
    PARALLEL_FINISH.store(enabled, Ordering::Relaxed);
}

/// Whether parallel finishing of render command encoders is on (see [`set_parallel_finish`]).
pub fn parallel_finish() -> bool {
    PARALLEL_FINISH.load(Ordering::Relaxed)
}

/// How many encoders the most recent [`PendingCommandBuffers::take`] (or [`RenderContextState::finish`])
/// finished through the task pool. 0 when it finished serially: the switch off, fewer than two encoders, or
/// no compute task pool. A cheap proof that the parallel path ran; it is not a metric.
static LAST_PARALLEL_FINISH_COUNT: AtomicUsize = AtomicUsize::new(0);

/// The number of encoders the last submission list finished in parallel (see [`set_parallel_finish`]).
pub fn last_parallel_finish_count() -> usize {
    LAST_PARALLEL_FINISH_COUNT.load(Ordering::Relaxed)
}

/// One entry of the ordered submission list: an already finished command buffer, or an encoder whose
/// `finish()` is still to be done (only queued unfinished while [`parallel_finish`] is on).
///
/// Generic only so the ordering logic ([`finish_in_order`]) can be tested without a GPU device.
#[doc(hidden)]
pub enum Pending<B = CommandBuffer, E = NamedEncoder> {
    Buffer(B),
    Encoder(E),
}

/// An encoder waiting to be finished, with the system that recorded it when tracing is on, so the
/// `render::encoder_finish` span says whose commands took the time.
#[doc(hidden)]
pub struct NamedEncoder {
    encoder: CommandEncoder,
    #[cfg(feature = "trace")]
    owner: Option<alloc::sync::Arc<str>>,
}

impl NamedEncoder {
    fn new(encoder: CommandEncoder) -> Self {
        Self {
            encoder,
            #[cfg(feature = "trace")]
            owner: None,
        }
    }

    fn finish(self) -> CommandBuffer {
        #[cfg(feature = "trace")]
        let _span = info_span!(
            "render::encoder_finish",
            owner = self.owner.as_deref().unwrap_or("unnamed")
        )
        .entered();
        self.encoder.finish()
    }
}

/// Queues `encoder` unfinished when parallel finishing is on, finished (the stock behaviour) otherwise.
fn finish_or_defer(encoder: CommandEncoder) -> Pending {
    if parallel_finish() {
        Pending::Encoder(NamedEncoder::new(encoder))
    } else {
        Pending::Buffer(NamedEncoder::new(encoder).finish())
    }
}

/// Turns an ordered list into finished command buffers in the same order. Encoders are finished in parallel
/// on the compute task pool when parallel finishing is on, there are at least two, and the pool exists;
/// otherwise one after another, in order.
fn finish_pending(pending: Vec<Pending>) -> Vec<CommandBuffer> {
    finish_in_order(pending, parallel_finish(), NamedEncoder::finish)
}

/// The ordering logic of [`finish_pending`], over any buffer and encoder types: returns one buffer per
/// entry, in the order of `pending`. With `use_pool` true, at least two encoders and a compute task pool,
/// the encoders are finished concurrently on the pool; the results go back into their own places.
#[doc(hidden)]
pub fn finish_in_order<B, E, F>(pending: Vec<Pending<B, E>>, use_pool: bool, finish: F) -> Vec<B>
where
    B: Send + 'static,
    E: Send,
    F: Fn(E) -> B + Sync,
{
    let encoder_count = pending
        .iter()
        .filter(|entry| matches!(entry, Pending::Encoder(_)))
        .count();
    let pool = if use_pool && encoder_count >= 2 {
        ComputeTaskPool::try_get()
    } else {
        None
    };
    let Some(pool) = pool else {
        LAST_PARALLEL_FINISH_COUNT.store(0, Ordering::Relaxed);
        return pending
            .into_iter()
            .map(|entry| match entry {
                Pending::Buffer(buffer) => buffer,
                Pending::Encoder(encoder) => finish(encoder),
            })
            .collect();
    };

    #[cfg(feature = "trace")]
    let _span = info_span!("finish_pending_parallel", encoders = encoder_count).entered();

    // `None` marks the place of an encoder; each task returns its place with its buffer.
    let mut slots: Vec<Option<B>> = Vec::with_capacity(pending.len());
    let mut encoders = Vec::with_capacity(encoder_count);
    for (index, entry) in pending.into_iter().enumerate() {
        match entry {
            Pending::Buffer(buffer) => slots.push(Some(buffer)),
            Pending::Encoder(encoder) => {
                slots.push(None);
                encoders.push((index, encoder));
            }
        }
    }
    let finish = &finish;
    let finished = pool.scope(|scope| {
        for (index, encoder) in encoders {
            scope.spawn(async move { (index, finish(encoder)) });
        }
    });
    for (index, buffer) in finished {
        slots[index] = Some(buffer);
    }
    debug_assert!(
        slots.iter().all(Option::is_some),
        "every queued encoder must come back finished"
    );
    LAST_PARALLEL_FINISH_COUNT.store(encoder_count, Ordering::Relaxed);
    slots.into_iter().flatten().collect()
}

#[derive(Default)]
struct PendingCommandBuffersInner {
    buffers: Vec<Pending>,
    encoders: Vec<CommandEncoder>,
}

/// A resource that holds command buffers and encoders that are pending submission to the render queue.
#[derive(Resource)]
pub struct PendingCommandBuffers(WgpuWrapper<PendingCommandBuffersInner>);

impl Default for PendingCommandBuffers {
    fn default() -> Self {
        Self(WgpuWrapper::new(PendingCommandBuffersInner::default()))
    }
}

impl PendingCommandBuffers {
    pub fn push(&mut self, buffers: impl IntoIterator<Item = CommandBuffer>) {
        self.0
            .buffers
            .extend(buffers.into_iter().map(Pending::Buffer));
    }

    fn push_pending(&mut self, pending: impl IntoIterator<Item = Pending>) {
        self.0.buffers.extend(pending);
    }

    pub fn push_encoder(&mut self, encoder: CommandEncoder) {
        self.0.encoders.push(encoder);
    }

    pub fn take(&mut self) -> Vec<CommandBuffer> {
        // Same order as stock: the pushed buffers, then the encoders handed over with `push_encoder`.
        let mut pending = core::mem::take(&mut self.0.buffers);
        pending.extend(
            self.0
                .encoders
                .drain(..)
                .map(|encoder| Pending::Encoder(NamedEncoder::new(encoder))),
        );
        finish_pending(pending)
    }

    pub fn is_empty(&self) -> bool {
        self.0.buffers.is_empty() && self.0.encoders.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.buffers.len() + self.0.encoders.len()
    }
}

#[derive(Default)]
struct RenderContextStateInner {
    command_encoder: Option<CommandEncoder>,
    command_buffers: Vec<Pending>,
    render_device: Option<RenderDevice>,
}

/// A resource that holds the current render context state, including command encoder and command buffers.
/// This is used internally by the [`RenderContext`] system parameter. Implements [`SystemBuffer`] to flush
/// command buffers at the end of each render system in topological system order.
pub struct RenderContextState(WgpuWrapper<RenderContextStateInner>);

impl Default for RenderContextState {
    fn default() -> Self {
        Self(WgpuWrapper::new(RenderContextStateInner::default()))
    }
}

impl RenderContextState {
    fn flush_encoder(&mut self) {
        if let Some(encoder) = self.0.command_encoder.take() {
            self.0.command_buffers.push(finish_or_defer(encoder));
        }
    }

    fn command_encoder(&mut self) -> &mut CommandEncoder {
        let render_device = self
            .0
            .render_device
            .clone()
            .expect("RenderDevice must be set before accessing command_encoder");

        self.0.command_encoder.get_or_insert_with(|| {
            render_device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        })
    }

    pub fn finish(&mut self) -> Vec<CommandBuffer> {
        self.flush_encoder();
        finish_pending(core::mem::take(&mut self.0.command_buffers))
    }
}

impl SystemBuffer for RenderContextState {
    fn queue(&mut self, _system_meta: &SystemMeta, mut world: DeferredWorld) {
        #[cfg(feature = "trace")]
        let _span =
            info_span!("RenderContextState::apply", system = %_system_meta.name()).entered();

        let inner = &mut *self.0;

        // flush to ensure correct submission order
        if let Some(encoder) = inner.command_encoder.take() {
            inner.command_buffers.push(finish_or_defer(encoder));
        }

        #[cfg(feature = "trace")]
        if inner
            .command_buffers
            .iter()
            .any(|entry| matches!(entry, Pending::Encoder(_)))
        {
            let owner: alloc::sync::Arc<str> = _system_meta.name().to_string().into();
            for entry in &mut inner.command_buffers {
                if let Pending::Encoder(named) = entry {
                    named.owner = Some(owner.clone());
                }
            }
        }

        if !inner.command_buffers.is_empty() {
            let mut pending = world.resource_mut::<PendingCommandBuffers>();
            pending.push_pending(core::mem::take(&mut inner.command_buffers));
        }

        inner.render_device = None;
    }
}

/// A system parameter that provides access to a command encoder and render device for issuing
/// rendering commands inside any system running beneath the root [`super::RenderGraph`] schedule in the
/// [`super::render_system`] system.
#[derive(SystemParam)]
pub struct RenderContext<'w, 's> {
    state: Deferred<'s, RenderContextState>,
    render_device: Res<'w, RenderDevice>,
    diagnostics_recorder: Option<Res<'w, DiagnosticsRecorder>>,
}

impl<'w, 's> RenderContext<'w, 's> {
    fn ensure_device(&mut self) {
        if self.state.0.render_device.is_none() {
            self.state.0.render_device = Some(self.render_device.clone());
        }
    }

    /// Returns the render device associated with this render context.
    pub fn render_device(&self) -> &RenderDevice {
        &self.render_device
    }

    /// Returns the diagnostics recorder, if available.
    pub fn diagnostic_recorder(&self) -> Option<Res<'w, DiagnosticsRecorder>> {
        self.diagnostics_recorder.as_ref().map(Res::clone)
    }

    /// Returns the current command encoder, creating one if it does not already exist.
    pub fn command_encoder(&mut self) -> &mut CommandEncoder {
        self.ensure_device();
        self.state.command_encoder()
    }

    /// Begins a tracked render pass with the given descriptor.
    pub fn begin_tracked_render_pass<'a>(
        &'a mut self,
        descriptor: RenderPassDescriptor<'_>,
    ) -> TrackedRenderPass<'a> {
        self.ensure_device();

        let command_encoder = self.state.0.command_encoder.get_or_insert_with(|| {
            self.render_device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
        });

        let render_pass = command_encoder.begin_render_pass(&descriptor);
        TrackedRenderPass::new(&self.render_device, render_pass)
    }

    /// Adds a finished command buffer to be submitted later.
    pub fn add_command_buffer(&mut self, command_buffer: CommandBuffer) {
        self.state.flush_encoder();
        self.state
            .0
            .command_buffers
            .push(Pending::Buffer(command_buffer));
    }
}

/// A system parameter that can be used to explicitly flush pending command buffers to the render queue.
/// This is typically not necessary, as command buffers are automatically flushed at the end of each
/// render system. However, in some cases it may be useful to flush command buffers earlier.
#[derive(SystemParam)]
pub struct FlushCommands<'w> {
    pending: ResMut<'w, PendingCommandBuffers>,
    queue: Res<'w, super::RenderQueue>,
}

impl<'w> FlushCommands<'w> {
    /// Flushes all pending command buffers to the render queue.
    pub fn flush(&mut self) {
        let buffers = {
            #[cfg(feature = "trace")]
            let _span =
                info_span!("render::pending_finish", pending = self.pending.len()).entered();
            self.pending.take()
        };
        if !buffers.is_empty() {
            #[cfg(feature = "trace")]
            let _span = info_span!("render::queue_submit", buffers = buffers.len()).entered();
            self.queue.submit(buffers);
        }
    }
}

/// The entity corresponding to the current view being rendered.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Deref, DerefMut)]
pub struct CurrentView(pub Entity);

/// A query that fetches components for the entity corresponding to the current view being rendered,
/// as defined by the [`CurrentView`] resource, equivalent to `query.get(current_view.entity())`.
pub struct ViewQuery<'w, 's, D: QueryData, F: QueryFilter = ()> {
    entity: Entity,
    item: D::Item<'w, 's>,
    _filter: PhantomData<F>,
}

impl<'w, 's, D: QueryData, F: QueryFilter> ViewQuery<'w, 's, D, F> {
    #[inline]
    pub fn entity(&self) -> Entity {
        self.entity
    }

    #[inline]
    pub fn into_inner(self) -> D::Item<'w, 's> {
        self.item
    }
}

pub struct ViewQueryState<D: QueryData, F: QueryFilter> {
    resource_id: ComponentId,
    query_state: QueryState<D, F>,
}

// SAFETY: ViewQuery accesses the CurrentView resource (read) and query components.
// Access is properly registered in init_access.
unsafe impl<'a, D: QueryData + 'static, F: QueryFilter + 'static> SystemParam
    for ViewQuery<'a, '_, D, F>
{
    type State = ViewQueryState<D, F>;
    type Item<'w, 's> = ViewQuery<'w, 's, D, F>;

    fn init_state(world: &mut World) -> Self::State {
        ViewQueryState {
            resource_id: world
                .components_registrator()
                .register_component::<CurrentView>(),
            query_state: QueryState::new(world),
        }
    }

    fn init_access(
        state: &Self::State,
        system_meta: &mut SystemMeta,
        component_access_set: &mut FilteredAccessSet,
        world: &mut World,
    ) {
        component_access_set.add_resource_read(state.resource_id);

        <Query<'_, '_, D, F> as SystemParam>::init_access(
            &state.query_state,
            system_meta,
            component_access_set,
            world,
        );
    }

    #[inline]
    unsafe fn get_param<'w, 's>(
        state: &'s mut Self::State,
        _system_meta: &SystemMeta,
        world: UnsafeWorldCell<'w>,
        _change_tick: Tick,
    ) -> Result<Self::Item<'w, 's>, SystemParamValidationError> {
        // SAFETY: We have registered resource read access in init_access
        let current_view = unsafe { world.get_resource::<CurrentView>() };

        let Some(current_view) = current_view else {
            return Err(SystemParamValidationError::skipped::<Self>(
                "CurrentView resource not present",
            ));
        };

        let entity = current_view.entity();

        // SAFETY: Query state access is properly registered in init_access.
        // The caller ensures the world matches the one used in init_state.
        let item = unsafe { state.query_state.get_unchecked(world, entity) }.map_err(|_| {
            SystemParamValidationError::skipped::<Self>("Current view entity does not match query")
        })?;

        Ok(ViewQuery {
            entity,
            item,
            _filter: PhantomData,
        })
    }
}

// SAFETY: ViewQuery with ReadOnlyQueryData only reads from the world.
unsafe impl<'w, 's, D: bevy_ecs::query::ReadOnlyQueryData + 'static, F: QueryFilter + 'static>
    bevy_ecs::system::ReadOnlySystemParam for ViewQuery<'w, 's, D, F>
{
}
