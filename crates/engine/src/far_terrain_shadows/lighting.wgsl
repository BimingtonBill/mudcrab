#define_import_path far_terrain_shadows::lighting
#import far_terrain_shadows::receiving::{f1_receive_at_origin, f1_visibility}
#import far_terrain_shadows::pbr_functions::apply_far_terrain_lighting
#import bevy_pbr::{pbr_types::PbrInput, mesh_view_bindings, mesh_view_types, mesh_types}
struct Selection { first: vec4<u32>, second: vec4<u32>, direction: vec4<f32>, }
@group(4) @binding(10) var<uniform> selection: Selection;
fn light_terrain(input: PbrInput, local: vec3<f32>, anchor: vec2<i32>, world: u32, valid: u32) -> vec4<f32> {
    var visibility = 1.0;
    let index = selection.first.x;
    if valid == 1u && selection.first.y == 1u && index < mesh_view_bindings::lights.n_directional_lights {
        let sun = mesh_view_bindings::lights.directional_lights[index];
        let view_z = (mesh_view_bindings::view.view_from_world * input.world_position).z;
        if sun.num_cascades > 0u && sun.num_cascades <= #{MAX_CASCADES_PER_LIGHT}u
            && (sun.flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u
            && (input.flags & mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u
            && all(sun.direction_to_light == selection.direction.xyz) {
            if -view_z >= sun.cascades[sun.num_cascades - 1u].far_bound {
                visibility = f1_visibility(f1_receive_at_origin(local, world, anchor,
                    selection.first.zw, selection.second.xy, selection.second.z, selection.second.w));
            }
        }
    }
    return apply_far_terrain_lighting(input, index, visibility);
}
