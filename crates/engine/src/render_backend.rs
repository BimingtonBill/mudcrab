//! The two renderer switches of impl-1132 (Mudcrab Plus): which graphics backends wgpu may pick
//! from (`--render-backends`), and the parallel finish of the render command encoders
//! (`--render-parallel-finish`), which is a patch to the vendored `bevy_render`
//! (`vendor/bevy_render-0.19.0/MUDCRAB-README.md`).
//!
//! Both default to today's behaviour: `all` backends, and stock Bevy's serial encoder finish.

use crate::config::{EngineConfig, RenderBackends};
use bevy::{
    prelude::*,
    render::{
        renderer::{RenderAdapterInfo, set_parallel_finish},
        settings::{Backends, InstanceFlags, WgpuSettings},
    },
};

/// The settings to build Bevy's `RenderPlugin` with, or `None` when the run keeps Bevy's own
/// defaults (`--render-backends all`, today's behaviour, which also still honours `WGPU_BACKEND`).
///
/// `vulkan` restricts wgpu to the Vulkan backend. Bevy's `WgpuSettings::default()` decides whether
/// to keep wgpu's `VALIDATION_INDIRECT_CALL` flag from the backend list it computes *itself*
/// (`bevy_render` `settings.rs`: kept whenever DX12 is among them, and the default list is all
/// backends), so naming only the backends here would leave the flag on. The flag costs a
/// validation compute pass per indirect draw and is needed only by DX12, so a Vulkan-only release
/// build removes it, exactly as Bevy does for a backend list without DX12. Debug builds keep it,
/// as Bevy's own defaults do.
pub fn wgpu_settings(backends: RenderBackends) -> Option<WgpuSettings> {
    match backends {
        RenderBackends::All => None,
        RenderBackends::Vulkan => {
            let mut settings = WgpuSettings {
                backends: Some(Backends::VULKAN),
                ..default()
            };
            if !cfg!(debug_assertions) {
                settings
                    .instance_flags
                    .remove(InstanceFlags::VALIDATION_INDIRECT_CALL);
            }
            Some(settings)
        }
    }
}

/// Applies the process-wide part of the render switches: the parallel finish of the command
/// encoders. Called once, before the app is built and so before the first frame.
pub fn apply_process_switches(config: &EngineConfig) {
    set_parallel_finish(config.render_parallel_finish);
}

/// Registers the startup line that names the backend wgpu chose.
pub struct RenderBackendLogPlugin;

impl Plugin for RenderBackendLogPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, log_render_backend);
    }
}

/// Logs the backend and adapter Bevy's `RenderPlugin` picked, with the two switches that shaped
/// the choice. A run with no renderer (`--headless` without a GPU) has no `RenderAdapterInfo` and
/// logs that instead of inventing one.
fn log_render_backend(config: Res<EngineConfig>, adapter: Option<Res<RenderAdapterInfo>>) {
    match adapter {
        Some(adapter) => info!(
            "render backend: {:?} on adapter \"{}\" (--render-backends {}, --render-parallel-finish {})",
            adapter.backend,
            adapter.name,
            config.render_backends.name(),
            if config.render_parallel_finish {
                "on"
            } else {
                "off"
            },
        ),
        None => info!("render backend: none (the run has no renderer adapter)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_backends_keep_bevys_own_settings() {
        assert!(wgpu_settings(RenderBackends::All).is_none());
    }

    #[test]
    fn vulkan_reaches_the_wgpu_backends() {
        let settings =
            wgpu_settings(RenderBackends::Vulkan).expect("vulkan overrides the settings");
        assert_eq!(settings.backends, Some(Backends::VULKAN));
    }

    #[test]
    fn vulkan_only_release_builds_drop_the_indirect_call_validation() {
        let settings =
            wgpu_settings(RenderBackends::Vulkan).expect("vulkan overrides the settings");
        let stock = WgpuSettings::default();
        if cfg!(debug_assertions) {
            assert_eq!(settings.instance_flags, stock.instance_flags);
        } else {
            assert!(
                !settings
                    .instance_flags
                    .contains(InstanceFlags::VALIDATION_INDIRECT_CALL)
            );
            assert!(
                stock
                    .instance_flags
                    .contains(InstanceFlags::VALIDATION_INDIRECT_CALL)
                    || std::env::var_os("WGPU_VALIDATION_INDIRECT_CALL").is_some(),
                "Bevy's own defaults keep the flag with every backend listed"
            );
        }
    }

    #[test]
    fn the_other_settings_stay_bevys_defaults() {
        let settings =
            wgpu_settings(RenderBackends::Vulkan).expect("vulkan overrides the settings");
        let stock = WgpuSettings::default();
        assert_eq!(settings.power_preference, stock.power_preference);
        assert_eq!(settings.features, stock.features);
        assert_eq!(
            settings.limits.max_bind_groups,
            stock.limits.max_bind_groups
        );
        assert_eq!(
            settings.force_fallback_adapter,
            stock.force_fallback_adapter
        );
    }
}
