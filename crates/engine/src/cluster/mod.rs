//! Cluster LOD drawing (ADR-0013). Prototype: `--clusters` wires it into the engine.
//!
//! The cluster path is an option (default off, distant objects are the baked LOD): without a
//! package nothing here is registered, so a frame with clusters off carries no system, pass, view
//! or payload of it. Where the GPU cannot run it, [`gpu_support`] says why, for the settings menu
//! to grey the option with the reason and for the engine to refuse with the same words.

use bevy::render::settings::{WgpuFeatures, WgpuLimits};

/// Storage buffers the cull's second pass binds in one stage (the cluster, group, instance, mesh
/// and instance-list tables, the argument and visible lists, and the counts).
const CULL_STORAGE_BUFFERS: u32 = 8;

/// Bind groups the draw pipelines use: the view, the stock mesh group, the material at 3 and the
/// cluster data at 4.
const DRAW_BIND_GROUPS: u32 = 5;

/// Whether an adapter can run the cluster path, or the first thing it lacks. The path needs the
/// draw's first instance to number its cluster (`INDIRECT_FIRST_INSTANCE`), enough storage
/// buffers per stage and bind groups, and a dispatch dimension of 65,535 (the cull spreads past
/// that over rows, but a smaller limit would not hold even one row). Drawing with the count read on
/// the GPU (`MULTI_DRAW_INDIRECT_COUNT`) is not required: the fixed-count draw covers its absence.
pub fn gpu_support(features: WgpuFeatures, limits: &WgpuLimits) -> Result<(), String> {
    if !features.contains(WgpuFeatures::INDIRECT_FIRST_INSTANCE) {
        return Err(
            "the GPU does not support indirect draws with a first instance (INDIRECT_FIRST_INSTANCE)"
                .into(),
        );
    }
    if limits.max_storage_buffers_per_shader_stage < CULL_STORAGE_BUFFERS {
        return Err(format!(
            "the GPU binds {} storage buffers per shader stage; the cluster cull needs {CULL_STORAGE_BUFFERS}",
            limits.max_storage_buffers_per_shader_stage
        ));
    }
    if limits.max_bind_groups < DRAW_BIND_GROUPS {
        return Err(format!(
            "the GPU allows {} bind groups; the cluster draw needs {DRAW_BIND_GROUPS}",
            limits.max_bind_groups
        ));
    }
    if limits.max_compute_workgroups_per_dimension < 65_535 {
        return Err(format!(
            "the GPU allows {} compute workgroups per dimension; the cluster cull needs 65535",
            limits.max_compute_workgroups_per_dimension
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capable() -> (WgpuFeatures, WgpuLimits) {
        (
            WgpuFeatures::INDIRECT_FIRST_INSTANCE,
            WgpuLimits {
                max_storage_buffers_per_shader_stage: 16,
                max_bind_groups: 8,
                max_compute_workgroups_per_dimension: 65_535,
                ..WgpuLimits::default()
            },
        )
    }

    #[test]
    fn a_capable_gpu_is_supported() {
        let (features, limits) = capable();
        assert_eq!(gpu_support(features, &limits), Ok(()));
    }

    #[test]
    fn each_missing_capability_is_named() {
        let (features, limits) = capable();
        let reason = gpu_support(WgpuFeatures::empty(), &limits).unwrap_err();
        assert!(reason.contains("INDIRECT_FIRST_INSTANCE"), "{reason}");
        let few = WgpuLimits {
            max_storage_buffers_per_shader_stage: 4,
            ..limits.clone()
        };
        assert!(
            gpu_support(features, &few)
                .unwrap_err()
                .contains("storage buffers")
        );
        let groups = WgpuLimits {
            max_bind_groups: 4,
            ..limits.clone()
        };
        assert!(
            gpu_support(features, &groups)
                .unwrap_err()
                .contains("bind groups")
        );
        let dispatch = WgpuLimits {
            max_compute_workgroups_per_dimension: 256,
            ..limits
        };
        assert!(
            gpu_support(features, &dispatch)
                .unwrap_err()
                .contains("workgroups")
        );
    }
}

pub mod assets;
pub mod envelope;
pub(crate) mod flat_source;
mod instance_identity;
mod instance_mask;
mod mesh_payload;
#[cfg(test)]
pub(crate) mod overlay_fixture;
pub(crate) mod ownership;
pub mod package;
pub mod reader;
pub mod render;
pub mod runtime_state;
/// The plan for drawing a shadow view's clusters in buckets of equal triangle count.
pub mod shadow_buckets;
pub(crate) mod source_fallback;
pub mod source_identity;
pub mod stock_visibility;

/// GPU-free residency bookkeeping for block streaming: range allocator, mesh ledger, protocol states, planner.
pub mod residency;

/// Optional clustered terrain material, metadata and near-coverage clipping.
pub mod terrain;

mod material_geometry;

pub(crate) mod stock_drawable;
