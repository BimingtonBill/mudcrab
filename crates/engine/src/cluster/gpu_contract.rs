//! Host/GPU contract shared by renderer uploads and hardware integration tests.

/// Execution order and explicit bindings of the production compaction pipelines.
/// The scatter layouts use exactly eight storage buffers, the portable limit checked
/// by gpu_support. Do not add another without splitting the stage or raising that check.
pub(crate) const STAGES: [(&str, &[u32]); 8] = [
    ("cull_instances", &[0, 3, 6, 8, 10, 11, 13]),
    ("prefix_instances", &[0, 9, 11]),
    ("scatter_instances", &[0, 8, 11]),
    ("cull", &[0, 1, 2, 3, 6, 8, 11, 12]),
    ("prefix_draw_tiles", &[0, 8, 11, 13]),
    ("prefix_draw_groups", &[5, 11, 13]),
    ("scatter_clusters", &[0, 3, 4, 6, 7, 8, 11, 12, 13]),
    ("scatter_large_clusters", &[0, 3, 4, 6, 7, 8, 11, 12, 13]),
];

/// Exact 128-byte WGSL CullParams uniform, without host-only padding or offsets.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CullParams {
    pub planes: [[f32; 4]; 5],
    pub camera: [f32; 4],
    pub error_pixels: f32,
    pub near: f32,
    pub min_instance_pixels: f32,
    pub instance_count: u32,
    pub orthographic: u32,
    pub terrain_error_pixels: f32,
    pub instance_mask_base: u32,
    pub epoch: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    #[test]
    fn cull_uniform_matches_production_wgsl_layout() {
        let module = naga_oil::compose::Composer::default()
            .make_naga_module(naga_oil::compose::NagaModuleDescriptor {
                source: include_str!("shaders/cluster_cull.wgsl"),
                file_path: "cluster_cull.wgsl",
                ..Default::default()
            })
            .expect("production shader validates");
        let (_, ty) = module
            .types
            .iter()
            .find(|(_, ty)| ty.name.as_deref() == Some("CullParams"))
            .expect("CullParams struct");
        let wgpu::naga::TypeInner::Struct { members, span } = &ty.inner else {
            panic!("CullParams must remain a struct");
        };
        let expected = [
            ("planes", offset_of!(CullParams, planes)),
            ("camera", offset_of!(CullParams, camera)),
            ("error_pixels", offset_of!(CullParams, error_pixels)),
            ("near", offset_of!(CullParams, near)),
            (
                "min_instance_pixels",
                offset_of!(CullParams, min_instance_pixels),
            ),
            ("instance_count", offset_of!(CullParams, instance_count)),
            ("orthographic", offset_of!(CullParams, orthographic)),
            (
                "terrain_error_pixels",
                offset_of!(CullParams, terrain_error_pixels),
            ),
            (
                "instance_mask_base",
                offset_of!(CullParams, instance_mask_base),
            ),
            ("epoch", offset_of!(CullParams, epoch)),
        ];
        assert_eq!(*span as usize, size_of::<CullParams>());
        assert_eq!(members.len(), expected.len());
        for (member, (name, offset)) in members.iter().zip(expected) {
            assert_eq!(member.name.as_deref(), Some(name));
            assert_eq!(member.offset as usize, offset, "{name}");
        }
    }
}
