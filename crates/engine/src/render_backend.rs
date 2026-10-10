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
                // Bevy applies the environment last (`WgpuSettings::default` ends with
                // `instance_flags.with_env()`), so an explicit `WGPU_*` setting still beats us.
                settings.instance_flags = settings.instance_flags.with_env();
            }
            Some(settings)
        }
    }
}

/// What `--render-backends` turns into once the machine is looked at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendChoice {
    /// The backends the renderer is built with: the requested ones, or `All` after a fallback.
    pub effective: RenderBackends,
    /// Set when the request could not be honoured; logged once the logger exists.
    pub problem: Option<String>,
}

/// Decides the backends for `requested`. `vulkan` needs a Vulkan adapter, and Bevy's `RenderPlugin`
/// panics deep inside its own setup when it finds none, so it is checked first (`vulkan_present` is
/// only asked for `vulkan`): with none, the run falls back to `all` and the problem names the flag.
pub fn resolve_backends(
    requested: RenderBackends,
    vulkan_present: impl FnOnce() -> bool,
) -> BackendChoice {
    match requested {
        RenderBackends::Vulkan if !vulkan_present() => BackendChoice {
            effective: RenderBackends::All,
            problem: Some(
                "--render-backends vulkan was asked for, but no Vulkan adapter was found on this \
                 machine; falling back to --render-backends all"
                    .to_string(),
            ),
        },
        _ => BackendChoice {
            effective: requested,
            problem: None,
        },
    }
}

/// Whether wgpu can see at least one Vulkan adapter. A missing loader or driver gives none (or a
/// panic inside the probe, which counts as none).
pub fn vulkan_adapter_present() -> bool {
    std::panic::catch_unwind(|| {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        !bevy::tasks::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN)).is_empty()
    })
    .unwrap_or(false)
}

/// Applies the process-wide part of the render switches: the parallel finish of the command
/// encoders. Called once, before the app is built and so before the first frame.
pub fn apply_process_switches(config: &EngineConfig) {
    set_parallel_finish(config.render_parallel_finish);
}

/// Registers the startup lines that name the backend wgpu chose, and any fallback from the
/// requested backends (the logger does not exist yet when the backends are decided).
pub struct RenderBackendLogPlugin {
    pub problem: Option<String>,
}

#[derive(Resource)]
struct RenderBackendProblem(String);

impl Plugin for RenderBackendLogPlugin {
    fn build(&self, app: &mut App) {
        if let Some(problem) = &self.problem {
            app.insert_resource(RenderBackendProblem(problem.clone()));
        }
        app.add_systems(Startup, log_render_backend);
    }
}

/// Logs the backend and adapter Bevy's `RenderPlugin` picked, with the two switches that shaped
/// the choice. A run with no renderer (`--headless` without a GPU) has no `RenderAdapterInfo` and
/// logs that instead of inventing one.
fn log_render_backend(
    config: Res<EngineConfig>,
    adapter: Option<Res<RenderAdapterInfo>>,
    problem: Option<Res<RenderBackendProblem>>,
) {
    if let Some(problem) = problem {
        error!("{}", problem.0);
    }
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
            // Bevy's own order: the flag is dropped, then the environment has the last word.
            let mut expected = stock.instance_flags;
            expected.remove(InstanceFlags::VALIDATION_INDIRECT_CALL);
            assert_eq!(settings.instance_flags, expected.with_env());
        }
    }

    #[test]
    fn vulkan_is_kept_when_an_adapter_exists() {
        let choice = resolve_backends(RenderBackends::Vulkan, || true);
        assert_eq!(choice.effective, RenderBackends::Vulkan);
        assert_eq!(choice.problem, None);
    }

    #[test]
    fn vulkan_without_an_adapter_falls_back_to_all_and_names_the_flag() {
        let choice = resolve_backends(RenderBackends::Vulkan, || false);
        assert_eq!(choice.effective, RenderBackends::All);
        let problem = choice.problem.expect("the fallback is reported");
        assert!(problem.contains("--render-backends vulkan"), "{problem}");
        assert!(problem.contains("falling back"), "{problem}");
        // The fallback then keeps Bevy's own settings.
        assert!(wgpu_settings(choice.effective).is_none());
    }

    #[test]
    fn all_never_probes_for_vulkan() {
        let choice = resolve_backends(RenderBackends::All, || {
            panic!("the Vulkan probe must not run for --render-backends all")
        });
        assert_eq!(choice.effective, RenderBackends::All);
        assert_eq!(choice.problem, None);
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
