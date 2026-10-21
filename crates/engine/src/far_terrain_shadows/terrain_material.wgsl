// Forward fragment stage of the cluster terrain material: Bevy's standard material, plus the
// near-stock clip (`terrain_clip.wgsl`). The lighting, the fog and the tone path are the stock
// ones; the only additions are that the alpha is forced opaque (the material is an alpha mask with
// a cutoff of 0 only so that the prepass and shadow passes run a fragment stage, see
// `cluster/terrain.rs`) and that fragments over quadrants the stock terrain draws are discarded.
//
// The clip comes after the base-colour sample and the stock alpha test, as `snow.wgsl` places its
// own work: a `discard` before a derivative-using texture sample would break its uniformity.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}
#import cluster_terrain::clip::terrain_clip

#ifdef CLUSTER_FAR_TERRAIN_SHADOWS
#import far_terrain_shadows::forward_io::{FarVertexOutput, stock_vertex_output}
#import far_terrain_shadows::lighting::light_terrain
#endif

@fragment
#ifdef CLUSTER_FAR_TERRAIN_SHADOWS
fn fragment(far_in: FarVertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    let in = stock_vertex_output(far_in);
    // Optional canonical XY visibility affects only the independently selected
    // sun beyond its actual final cascade; invalid metadata leaves geometry intact.
#else
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    if terrain_clip(in.world_position, in.instance_index, 0u) {
        discard;
    }
    pbr_input.material.base_color.a = 1.0;
    var out: FragmentOutput;
#ifdef CLUSTER_FAR_TERRAIN_SHADOWS
    out.color = light_terrain(pbr_input, far_in.instance_local_renderer,
        far_in.canonical_cell_anchor, far_in.canonical_world, far_in.canonical_metadata_valid);
#else
    out.color = apply_pbr_lighting(pbr_input);
#endif
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
