//! impl-1132: the render command encoders may be finished in parallel (`--render-parallel-finish`,
//! the Mudcrab patch to the vendored `bevy_render`), and the command buffers must still reach the queue
//! in exactly the order the render systems produced them.
//!
//! The test runs a small schedule of render systems the way `Core3d` runs (deferred buffers applied
//! once, at the end, in system order), each system copying one buffer into the next. A copy only
//! moves data if the copy before it already ran, so the last buffer holds the marker only if every
//! command buffer was submitted in order. It is run with the switch off (stock) and on.
//!
//! It needs a GPU adapter and fails loudly without one, so a green run always means the submission
//! order was exercised on a device. A machine that really has no adapter sets
//! `MUDCRAB_ALLOW_NO_GPU=1` to let the test skip (with a message). The ordering logic itself is
//! covered without a device by `finish_order_logic.rs`.
//!
//! With the switch on, the test also checks that the encoders really went through the task pool
//! (`last_parallel_finish_count`), so it cannot pass on the serial fallback.

use bevy::ecs::schedule::{Schedule, ScheduleBuildSettings};
use bevy::prelude::*;
use bevy::render::render_resource::{
    BufferDescriptor, BufferUsages, CommandEncoderDescriptor, MapMode, PollType,
};
use bevy::render::renderer::{
    PendingCommandBuffers, RenderContext, RenderDevice, RenderQueue, last_parallel_finish_count,
    parallel_finish, set_parallel_finish,
};
use bevy::render::settings::{Backends, RenderResources, WgpuSettings};
use bevy::tasks::{ComputeTaskPool, TaskPool};

const MARKER: u8 = 0xAA;
const BUFFER_COUNT: usize = 7;

#[derive(Resource)]
struct Chain(Vec<bevy::render::render_resource::Buffer>);

/// Records `chain[from] -> chain[to]` into a standalone encoder, finishes it, and adds it to the context
/// with `add_command_buffer`.
fn add_finished_copy(context: &mut RenderContext, chain: &Chain, from: usize, to: usize) {
    let mut encoder = context
        .render_device()
        .create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(&chain.0[from], 0, &chain.0[to], 0, 4);
    context.add_command_buffer(encoder.finish());
}

/// Step 1: the context encoder, left for the deferred apply to finish (or to queue unfinished).
fn copy_0_to_1(mut context: RenderContext, chain: Res<Chain>) {
    context
        .command_encoder()
        .copy_buffer_to_buffer(&chain.0[0], 0, &chain.0[1], 0, 4);
}

/// Step 2: a command buffer finished by the system itself and added with `add_command_buffer`.
fn copy_1_to_2(mut context: RenderContext, chain: Res<Chain>) {
    add_finished_copy(&mut context, &chain, 1, 2);
}

/// Step 3: the context encoder again.
fn copy_2_to_3(mut context: RenderContext, chain: Res<Chain>) {
    context
        .command_encoder()
        .copy_buffer_to_buffer(&chain.0[2], 0, &chain.0[3], 0, 4);
}

/// Steps 4 and 5 in one system: the context encoder, then `add_command_buffer`, which has to
/// flush that encoder first.
fn copy_3_to_4_and_4_to_5(mut context: RenderContext, chain: Res<Chain>) {
    context
        .command_encoder()
        .copy_buffer_to_buffer(&chain.0[3], 0, &chain.0[4], 0, 4);
    add_finished_copy(&mut context, &chain, 4, 5);
}

fn gpu() -> Option<RenderResources> {
    let created = std::panic::catch_unwind(|| {
        bevy::tasks::block_on(bevy::render::renderer::initialize_renderer(
            Backends::all(),
            None,
            &WgpuSettings::default(),
        ))
    });
    created.ok()
}

/// Runs the four systems, then the diagnostics-style `push_encoder` step 6, takes the pending list,
/// submits it, and returns the marker byte that arrived in the last buffer, the buffer count, and how
/// many encoders `take()` finished through the task pool.
fn run_chain(device: &RenderDevice, queue: &RenderQueue) -> (u8, usize, usize) {
    let buffers = (0..BUFFER_COUNT)
        .map(|index| {
            device.create_buffer(&BufferDescriptor {
                label: Some(&format!("chain-{index}")),
                size: 4,
                usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        })
        .collect::<Vec<_>>();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("readback"),
        size: 4,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffers[0], 0, &[MARKER; 4]);

    let mut world = World::new();
    world.insert_resource(device.clone());
    world.init_resource::<PendingCommandBuffers>();
    world.insert_resource(Chain(buffers.clone()));

    let mut schedule = Schedule::default();
    // As `Core3d`: no sync points between the systems, so every deferred buffer is applied once, at
    // the end, in system order.
    schedule.set_build_settings(ScheduleBuildSettings {
        auto_insert_apply_deferred: false,
        ..default()
    });
    schedule.add_systems(
        (
            copy_0_to_1,
            copy_1_to_2,
            copy_2_to_3,
            copy_3_to_4_and_4_to_5,
        )
            .chain(),
    );
    schedule.run(&mut world);

    // The diagnostics recorder's encoder: handed over with `push_encoder`, after the systems.
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(&buffers[5], 0, &buffers[6], 0, 4);
    world
        .resource_mut::<PendingCommandBuffers>()
        .push_encoder(encoder);

    let taken = world.resource_mut::<PendingCommandBuffers>().take();
    let count = taken.len();
    let pooled = last_parallel_finish_count();
    queue.submit(taken);

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(&buffers[6], 0, &readback, 0, 4);
    queue.submit([encoder.finish()]);
    readback.slice(..).map_async(MapMode::Read, |result| {
        result.expect("the readback buffer maps");
    });
    device
        .poll(PollType::wait_indefinitely())
        .expect("the device polls to completion");
    let marker = readback.slice(..).get_mapped_range()[0];
    readback.unmap();
    (marker, count, pooled)
}

/// Puts the process-wide switch back to stock when the test ends, also when an assertion fails.
struct SwitchGuard;

impl Drop for SwitchGuard {
    fn drop(&mut self) {
        set_parallel_finish(false);
    }
}

#[test]
fn command_buffers_reach_the_queue_in_system_order_with_the_switch_off_and_on() {
    let Some(RenderResources(device, queue, ..)) = gpu() else {
        assert!(
            std::env::var_os("MUDCRAB_ALLOW_NO_GPU").is_some_and(|value| value == "1"),
            "no GPU adapter could be created, so the submission order cannot be exercised; \
             set MUDCRAB_ALLOW_NO_GPU=1 to allow skipping on a machine that has none"
        );
        eprintln!(
            "SKIPPED (MUDCRAB_ALLOW_NO_GPU=1): no GPU adapter; the submission order was not exercised"
        );
        return;
    };
    ComputeTaskPool::get_or_init(TaskPool::default);
    let _guard = SwitchGuard;

    set_parallel_finish(false);
    assert!(!parallel_finish());
    let stock = run_chain(&device, &queue);

    set_parallel_finish(true);
    assert!(parallel_finish());
    let parallel = run_chain(&device, &queue);

    // Steps 1, 2, 3, 4, 5 and the pushed encoder are six command buffers either way. With the switch
    // on, the encoders of steps 1, 3 and 4 and the pushed one (four) are finished through the pool;
    // with it off none are.
    assert_eq!(stock, (MARKER, 6, 0), "stock path");
    assert_eq!(parallel, (MARKER, 6, 4), "parallel-finish path");
}
