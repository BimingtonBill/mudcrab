use crate::{
    extract_resource::ExtractResource,
    render_resource::{ShaderType, UniformBuffer},
    renderer::{RenderDevice, RenderQueue},
    Extract, ExtractSchedule, GpuResourceAppExt, Render, RenderApp, RenderSystems,
};
use bevy_app::{App, Plugin};
use bevy_diagnostic::FrameCount;
use bevy_ecs::prelude::*;
use bevy_reflect::prelude::*;
use bevy_shader::load_shader_library;
use bevy_time::Time;

pub struct GlobalsPlugin;

impl Plugin for GlobalsPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "globals.wgsl");
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_gpu_resource::<GlobalsBuffer>()
                .init_resource::<Time>()
                .add_systems(ExtractSchedule, (extract_frame_count, extract_time))
                .add_systems(
                    Render,
                    prepare_globals_buffer.in_set(RenderSystems::PrepareResources),
                );
        }
    }
}

fn extract_frame_count(mut commands: Commands, frame_count: Extract<Res<FrameCount>>) {
    commands.insert_resource(**frame_count);
}

fn extract_time(mut commands: Commands, time: Extract<Res<Time>>) {
    commands.insert_resource(**time);
}

/// Contains global values useful when writing shaders.
/// Currently only contains values related to time.
#[derive(Default, Clone, Resource, ExtractResource, Reflect, ShaderType)]
#[reflect(Resource, Default, Clone)]
pub struct GlobalsUniform {
    /// The time since startup in seconds.
    /// Wraps to 0 after 1 hour.
    time: f32,
    /// The delta time since the previous frame in seconds
    delta_time: f32,
    /// Frame count since the start of the app.
    /// It wraps to zero when it reaches the maximum value of a u32.
    frame_count: u32,
    /// Nonwrapping time for animations whose phase cannot repeat at the clock wrap.
    /// Occupies the fourth word; the Globals uniform remains 16-byte aligned.
    time_unwrapped: f32,
}

impl GlobalsUniform {
    fn update_time(&mut self, time: &Time) {
        self.time = time.elapsed_secs_wrapped();
        self.time_unwrapped = time.elapsed_secs();
        self.delta_time = time.delta_secs();
    }
}

/// The buffer containing the [`GlobalsUniform`]
#[derive(Resource, Default)]
pub struct GlobalsBuffer {
    pub buffer: UniformBuffer<GlobalsUniform>,
}

fn prepare_globals_buffer(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut globals_buffer: ResMut<GlobalsBuffer>,
    time: Res<Time>,
    frame_count: Res<FrameCount>,
) {
    let buffer = globals_buffer.buffer.get_mut();
    buffer.update_time(&time);
    buffer.frame_count = frame_count.0;

    globals_buffer
        .buffer
        .write_buffer(&render_device, &render_queue);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonwrapping_animation_time_crosses_both_previous_water_wrap_boundaries() {
        for seconds in [3600, 12 * 3600] {
            let mut time = Time::default();
            let mut globals = GlobalsUniform::default();
            time.set_wrap_period(core::time::Duration::from_secs(seconds));
            time.advance_by(
                core::time::Duration::from_secs(seconds) - core::time::Duration::from_millis(16),
            );
            globals.update_time(&time);
            let before = globals.time_unwrapped;
            time.advance_by(core::time::Duration::from_millis(32));
            globals.update_time(&time);
            let after = globals.time_unwrapped;
            assert!(globals.time < 1.0);
            assert!((after - before - 0.032).abs() < 0.004);
            // The animation phase continues instead of resetting with globals.time.
            let phase = |t: f32| (t * 0.15 * 0.8).cos();
            assert!((phase(after) - phase(before)).abs() < 0.005);
        }
    }
}
