// Cluster vertex stage: a cluster's local triangle, its vertex reference and the vertex data come
// from storage buffers holding the payload sections as stored (no vertex or index buffer). The
// output is Bevy's own forward `VertexOutput`, so the stock pbr fragment shader and the stock
// material bind group draw it unchanged.
//
// `instance_index` is `first_instance` of the indirect draw: the draw's place in the compacted
// list, where the cull pass wrote its instance, mesh and cluster. Meshes are pooled in shared
// buffers, so every offset is the mesh's base from the `meshes` table. The stock fragment reads
// `mesh[in.instance_index]`, so the mesh uniform array bound at group 2 holds one entry per
// instance and the output instance index is the instance only.

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_functions,
    view_transformations::position_world_to_clip,
}

#ifdef CLUSTER_FAR_TERRAIN_SHADOWS
#import far_terrain_shadows::forward_io::FarVertexOutput
#import bevy_pbr::mesh_bindings
#endif

struct MeshInfo {
    vertex_base: u32,
    cluster_base: u32,
    vertex_ref_base: u32,
    triangle_base: u32,
    group_base: u32,
    cluster_count: u32,
    pad0: u32,
    pad1: u32,
    // uv_min (xy), uv_scale (zw)
    uv: vec4<f32>,
    // Whole-mesh bounding sphere, read by the cull pass.
    bounds: vec4<f32>,
}

@group(4) @binding(0) var<storage, read> positions: array<f32>;
// Per vertex, four words: packed octahedral normal, packed UV, vertex colour (unorm8 RGBA), packed
// octahedral tangent (handedness in the low bit of the second component, set when negative).
@group(4) @binding(1) var<storage, read> attributes: array<u32>;
@group(4) @binding(2) var<storage, read> clusters: array<u32>;
@group(4) @binding(3) var<storage, read> cluster_vertices: array<u32>;
@group(4) @binding(4) var<storage, read> cluster_triangles: array<u32>;
@group(4) @binding(5) var<storage, read> meshes: array<MeshInfo>;
@group(4) @binding(6) var<storage, read> visible: array<vec4<u32>>;

fn triangle_byte(index: u32) -> u32 {
    return (cluster_triangles[index >> 2u] >> ((index & 3u) * 8u)) & 0xffu;
}

// Octahedral decode of two snorm16 values (ADR-0013 section 4, Cigolle et al. 2014).
fn decode_normal(packed: u32) -> vec3<f32> {
    let f = unpack2x16snorm(packed);
    var n = vec3<f32>(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
    if n.z < 0.0 {
        let folded = (1.0 - abs(vec2<f32>(n.y, n.x))) * select(vec2<f32>(-1.0), vec2<f32>(1.0), n.xy >= vec2<f32>(0.0));
        n = vec3<f32>(folded.x, folded.y, n.z);
    }
    return normalize(n);
}

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) slot: u32,
)
#ifdef CLUSTER_FAR_TERRAIN_SHADOWS
    -> FarVertexOutput {
#else
    -> VertexOutput {
#endif
    let draw = visible[slot];
    let instance = draw.x;
    let mesh = meshes[draw.y];
    let base = (mesh.cluster_base + draw.z) * 12u;
    let vertex_offset = clusters[base + 6u];
    let triangle_offset = clusters[base + 7u];
    let local = triangle_byte(
        mesh.triangle_base + (triangle_offset + vertex_index / 3u) * 3u + vertex_index % 3u,
    );
    let source = mesh.vertex_base + cluster_vertices[mesh.vertex_ref_base + vertex_offset + local];

    let position = vec3<f32>(
        positions[source * 3u],
        positions[source * 3u + 1u],
        positions[source * 3u + 2u],
    );
    let normal = decode_normal(attributes[source * 4u]);
    let uv = mesh.uv.xy + unpack2x16unorm(attributes[source * 4u + 1u]) * mesh.uv.zw;
    let colour = unpack4x8unorm(attributes[source * 4u + 2u]);
    let tangent_word = attributes[source * 4u + 3u];
    let tangent_negative = (tangent_word & 0x10000u) != 0u;
    let tangent_direction = decode_normal(tangent_word & 0xfffeffffu);
    let tangent = vec4<f32>(tangent_direction, select(1.0, -1.0, tangent_negative));

#ifdef CLUSTER_FAR_TERRAIN_SHADOWS
    var out: FarVertexOutput;
#else
    var out: VertexOutput;
#endif
    let world_from_local = mesh_functions::get_world_from_local(instance);
    out.world_position = mesh_functions::mesh_position_local_to_world(
        world_from_local,
        vec4<f32>(position, 1.0),
    );
    out.position = position_world_to_clip(out.world_position.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(normal, instance);
#ifdef VERTEX_UVS_A
    out.uv = uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = uv;
#endif
#ifdef VERTEX_TANGENTS
    out.world_tangent = mesh_functions::mesh_tangent_local_to_world(
        world_from_local,
        tangent,
        instance,
    );
#endif
#ifdef VERTEX_COLORS
    out.color = colour;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = instance;
#endif
#ifdef CLUSTER_FAR_TERRAIN_SHADOWS
    // Preserve decoded instance-local coordinates; never recover them by
    // subtracting a large world-space translation in the fragment stage.
    out.instance_local_renderer = position;
    // Explicit validity is the only authority to interpret these default zeros.
    // They are NOT a sentinel world/anchor and must never be queried while invalid.
    out.canonical_cell_anchor = vec2<i32>(0);
    out.canonical_world = 0u;
    out.canonical_metadata_valid = 0u;
#ifdef PER_OBJECT_BUFFER_BATCH_SIZE
    let mesh_count = #{PER_OBJECT_BUFFER_BATCH_SIZE}u;
#else
    let mesh_count = arrayLength(&mesh_bindings::mesh);
#endif
    if instance < mesh_count {
        // Actual instance index, not compacted indirect slot or batch/material.
        let tag = mesh_bindings::mesh[instance].tag;
        let record_count = arrayLength(&visible);
        if tag < record_count && record_count - tag >= 10u {
            let state = visible[tag + 1u];
            // ROOT-APPROVED OPTIONAL ABI: exact f32 2.0 (0x40000000) means
            // active geometry AND CPU-validated canonical association. Existing
            // active1.0 remains drawn but invalid for F1; inactive0.0 unchanged.
            // state.yzw are raw u32 anchor-x/anchor-y/world, never numeric f32.
            // CPU writer/epoch/transform proof is required and not implemented here.
            if state.x == 0x40000000u {
                out.canonical_cell_anchor = bitcast<vec2<i32>>(state.yz);
                out.canonical_world = state.w;
                out.canonical_metadata_valid = 1u;
            }
        }
    }
#endif
    return out;
}
