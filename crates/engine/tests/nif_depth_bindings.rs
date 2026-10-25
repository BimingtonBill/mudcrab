//! Device-backed composition checks for the binding-free authored depth extension.
use bevy::{
    prelude::*,
    render::{
        render_resource::AsBindGroup,
        renderer::initialize_renderer_without_callbacks,
        settings::{Backends, RenderResources, WgpuSettings},
    },
};
use engine::nif_depth::{NifDepthExtension, NifDepthMaterial, NifDepthState};

#[test]
fn authored_depth_extension_preserves_base_layout_in_both_binding_modes() {
    let RenderResources(device, _, adapter, ..) = bevy::tasks::block_on(
        initialize_renderer_without_callbacks(Backends::VULKAN, None, &WgpuSettings::default()),
    );
    assert_ne!(adapter.device_type, wgpu::DeviceType::Cpu, "GPU required");
    eprintln!(
        "authored depth bindings adapter: {} ({:?})",
        adapter.name, adapter.backend
    );
    eprintln!(
        "base bindless supported: {}",
        StandardMaterial::bindless_supported(&device)
    );
    assert_eq!(
        NifDepthMaterial::bindless_supported(&device),
        StandardMaterial::bindless_supported(&device)
    );
    let extension = NifDepthExtension {
        state: NifDepthState {
            depth_test: true,
            depth_write: false,
            decal: true,
            alpha_mask: true,
            cast_shadows: false,
        },
    };
    for force_no_bindless in [false, true] {
        let base = StandardMaterial::bind_group_layout_entries(&device, force_no_bindless);
        let combined = NifDepthMaterial::bind_group_layout_entries(&device, force_no_bindless);
        assert_eq!(combined, base);
        // Ask wgpu to validate the actual combined layout, including its base arrays.
        let _combined_layout = device.create_bind_group_layout("authored-depth-base", &combined);
        let entries = NifDepthExtension::bind_group_layout_entries(&device, force_no_bindless);
        assert!(entries.is_empty());
        let layout = device.create_bind_group_layout("authored-depth-extension", &entries);
        let prepared = extension
            .unprepared_bind_group(&layout, &device, &mut (), force_no_bindless)
            .expect("the state-only extension must prepare without GPU resources");
        assert!(prepared.bindings.0.is_empty());
        let _bind_group = device.create_bind_group("authored-depth-extension", &layout, &[]);
    }
}
