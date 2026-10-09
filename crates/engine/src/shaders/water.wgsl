#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}
#import bevy_pbr::mesh_view_bindings::{view, globals}
#ifdef WATER_SSR
#import bevy_pbr::{
    mesh_view_bindings::{view_transmission_texture, view_transmission_sampler},
    view_transformations::{ndc_to_uv, perspective_camera_near, position_ndc_to_world},
}
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils
#endif
#endif

struct WaterSettings {
    // x = wave scale, y = wave speed, z = wave strength. w unused.
    wave_scale_speed_strength: vec4<f32>,
    // xy = flow direction, z unused, w = 1 when the flow normal texture is present.
    flow_direction: vec4<f32>,
    // x = Fresnel Amount (Schlick F0), y = Reflectivity Amount (WATR DNAM, per-water; falls back
    // to Skyrim's DefaultWater 0.10 / 0.8 when a water has no decoded colours). z/w unused.
    fresnel_reflectivity: vec4<f32>,
    // x = pinned clock in seconds, y = 1 to use it instead of the shared clock. z/w unused.
    clock: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: WaterSettings;

@group(#{MATERIAL_BIND_GROUP}) @binding(101) var water_reflection: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var water_reflection_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var water_flow_normal: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var water_flow_normal_sampler: sampler;

#ifdef WATER_SSR
// Screen-space reflection (`--water-reflection ssr`, crates/engine/src/water_ssr.rs).
//
// The view's `view_transmission_texture` is, for a view that draws water, a copy of the opaque
// colour taken after the opaque pass (rows 0..height) plus one extra row at the bottom that holds
// the sky ramp and a sentinel (`water_ssr.rs`, `prepare_water_ssr_textures`). The march reads the
// opaque depth prepass, so water a view draws sees that view's own depth and colour, whichever
// view that is.

// Texels in the sky ramp; the sentinel is the texel after it. Must equal `SKY_RAMP_TEXELS`.
const WATER_SSR_RAMP_TEXELS: u32 = 32u;
// Steps of the march, from `--water-ssr-steps`: shader definitions, so the loops have constant
// bounds.
const WATER_SSR_LINEAR_STEPS: u32 = #{WATER_SSR_LINEAR_STEPS}u;
const WATER_SSR_BISECTION_STEPS: u32 = #{WATER_SSR_BISECTION_STEPS}u;
// Rays are followed this far in world units (4096 to a cell) and a hit fades out over the last
// half of it, so a reflection never ends in a hard edge.
const WATER_SSR_MAX_DISTANCE: f32 = 16384.0;
// How thick a surface in the depth buffer is taken to be, so a ray that passes behind an object
// misses it instead of hitting it: a constant in world units plus a share of the surface's own
// depth, because the depth buffer resolves less at a distance.
const WATER_SSR_THICKNESS_UNITS: f32 = 32.0;
const WATER_SSR_THICKNESS_RELATIVE: f32 = 0.02;
// Multiplies the depth the ray is compared with, against self-intersection from rounding.
const WATER_SSR_DEPTH_BIAS: f32 = 0.000002;
// A hit within this share of the screen from an edge fades to the sky.
const WATER_SSR_EDGE_FADE: f32 = 0.08;
// The reflection ray's dot with the view direction must pass 0.0 and reach this to be fully used:
// a ray pointing back at the camera fades out.
const WATER_SSR_FACING_FULL: f32 = 0.3;

// ---- The sky fallback: this branch's fill for a ray that misses ---------------------------------
// The caller of `water_ssr_reflect` decides what fills a miss; this is what this branch fills it
// with. A branch with another fill (a planar reflection texture) leaves this section out.

// True when this view's transmission texture is the one `water_ssr.rs` made: it ends in a ramp row
// whose texel after the ramp is the sentinel. Bevy's own transmission texture (a view with glass
// that nothing marked) and the 1x1 fallback fail the test, and the water then has no reflection.
fn water_ssr_has_ramp(size: vec2<u32>) -> bool {
    if size.x <= WATER_SSR_RAMP_TEXELS || size.y < 2u {
        return false;
    }
    let sentinel = textureLoad(
        view_transmission_texture,
        vec2<i32>(i32(WATER_SSR_RAMP_TEXELS), i32(size.y) - 1),
        0,
    );
    return all(sentinel == vec4<f32>(1.0, 0.5, 0.25, 0.125));
}

// The sky the dome draws in this direction, from the ramp row. The ramp is spaced by the square
// root of the elevation, so it has texels close together at the horizon, where the dome fades
// from the clear colour to the sky.
fn water_ssr_sky(direction: vec3<f32>, ramp_row: i32) -> vec3<f32> {
    let elevation = degrees(asin(clamp(direction.y, 0.0, 1.0)));
    let position = sqrt(elevation / 90.0) * f32(WATER_SSR_RAMP_TEXELS - 1u);
    let below = floor(position);
    let first = i32(below);
    let second = min(first + 1, i32(WATER_SSR_RAMP_TEXELS) - 1);
    let a = textureLoad(view_transmission_texture, vec2<i32>(first, ramp_row), 0).rgb;
    let b = textureLoad(view_transmission_texture, vec2<i32>(second, ramp_row), 0).rgb;
    return mix(a, b, position - below);
}

// ---- The march: nothing below this line knows what a miss is filled with -------------------------

fn water_ssr_noise(pixel: vec2<f32>) -> f32 {
    // Interleaved gradient noise: a fixed pattern, so the reflection does not shimmer, that breaks
    // up the banding of a coarse linear step.
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

#ifdef DEPTH_PREPASS
struct WaterSsrSample {
    // Negative where the ray is behind the surface in the depth buffer.
    gap: f32,
    // How far behind that surface the ray is (negative in front of it).
    penetration: f32,
    // The surface's depth in units of the near plane.
    scene_depth: f32,
    // The sampled position in target pixels.
    pixel: vec2<f32>,
}

fn water_ssr_inverse_depth(ndc_z: f32) -> f32 {
    // Reverse-Z with the far plane at infinity: ndc z is the near distance over the view depth, so
    // its inverse is the depth in units of the near plane, which is what a straight line on screen
    // interpolates. A sky pixel (z = 0) comes out far away.
    return 1.0 / max(ndc_z, 1.0e-9);
}

fn water_ssr_sample(ndc: vec3<f32>) -> WaterSsrSample {
    let uv = ndc_to_uv(ndc.xy);
    let pixel = view.viewport.xy + uv * view.viewport.zw;
    let texel = clamp(
        floor(pixel),
        view.viewport.xy,
        view.viewport.xy + view.viewport.zw - vec2<f32>(1.0),
    );
    let scene = water_ssr_inverse_depth(prepass_utils::prepass_depth(vec4<f32>(texel, 0.0, 1.0), 0u));
    let ray = water_ssr_inverse_depth(ndc.z);
    var out: WaterSsrSample;
    out.gap = scene * (1.0 + WATER_SSR_DEPTH_BIAS) - ray;
    out.penetration = ray - scene;
    out.scene_depth = scene;
    out.pixel = pixel;
    return out;
}
#endif

// Marches the reflected ray through the opaque depth: equal linear steps along the ray's line on
// screen find where it first passes behind a surface, bisection narrows that to the surface, and a
// thickness test throws the hit away when the ray went behind an object instead of meeting it.
// The hit's colour is the opaque copy at that pixel.
//
// Returns the reflected colour in rgb and how sure the hit is in a: 0 for a miss (a ray that left
// the screen, went behind something, pointed back at the camera or ran too far; rgb is 0), up to 1
// for a hit nothing fades. A caller blends it over whatever it fills misses with:
// `mix(fill, reflected.rgb, reflected.a)`. `copy_size` is the size of the opaque copy texture in
// texels; its top rows are the opaque colour of the target, pixel for pixel.
fn water_ssr_reflect(
    world_position: vec3<f32>,
    ray_direction: vec3<f32>,
    fragment_xy: vec2<f32>,
    copy_size: vec2<f32>,
) -> vec4<f32> {
    var result = vec4<f32>(0.0);
#ifdef DEPTH_PREPASS
    // A ray that points back at the camera sees things between the water and the viewer, from
    // behind: fade it out rather than march it.
    let forward = -normalize(view.world_from_view[2].xyz);
    let facing = smoothstep(0.0, WATER_SSR_FACING_FULL, dot(ray_direction, forward));
    if facing <= 0.0 {
        return result;
    }

    // The ray's line in normalized device space. Where it ends is found in clip space: the clip
    // position of the start plus the clip-space direction, divided through, is a point on the
    // same line, and a line is all the march needs.
    let clip_start = view.clip_from_world * vec4<f32>(world_position, 1.0);
    if clip_start.w <= 1.0e-6 {
        return result;
    }
    let start = clip_start.xyz / clip_start.w;
    let clip_direction = view.clip_from_world * vec4<f32>(ray_direction, 0.0);
    let line_end = vec4<f32>(start, 1.0) + clip_direction;
    let delta = line_end.xyz / max(line_end.w, 1.0e-6) - start;

    // Follow the line to where it leaves the frustum, or to the farthest distance, whichever is
    // first.
    let direction_sign = select(vec3<f32>(-1.0), vec3<f32>(1.0), delta >= vec3<f32>(0.0));
    let safe_delta = direction_sign * max(abs(delta), vec3<f32>(1.0e-6));
    let far_edge = select(vec3<f32>(-1.0, -1.0, 0.0), vec3<f32>(1.0), delta >= vec3<f32>(0.0));
    let to_edge = (far_edge - start) / safe_delta;
    let to_frustum_edge = min(min(to_edge.x, to_edge.y), to_edge.z);
    let clip_far = clip_start + clip_direction * WATER_SSR_MAX_DISTANCE;
    let far_point = clip_far.xyz / max(clip_far.w, 1.0e-6);
    let to_max_distance = dot(far_point - start, delta) / max(dot(delta, delta), 1.0e-12);
    let reach = max(min(to_frustum_edge, to_max_distance), 0.0);
    let span = delta * reach;

    let span_pixels = length(span.xy * 0.5 * view.viewport.zw);
    if span_pixels < 2.0 {
        return result;
    }
    // No more steps than pixels the ray crosses.
    let steps = clamp(u32(span_pixels), 2u, WATER_SSR_LINEAR_STEPS);

    // Linear search for the first sample behind the depth buffer. Jittered so the steps of
    // neighbouring pixels land in different places.
    let jitter = water_ssr_noise(fragment_xy);
    var in_front = 0.0;
    var behind = 0.0;
    var found = false;
    for (var index = 0u; index < WATER_SSR_LINEAR_STEPS; index += 1u) {
        if index >= steps {
            break;
        }
        let t = (f32(index) + jitter) / f32(steps);
        if water_ssr_sample(start + span * t).gap < 0.0 {
            behind = t;
            found = true;
            break;
        }
        in_front = t;
    }
    if !found {
        return result;
    }

    // Bisection between the last sample in front and the first behind.
    for (var refine = 0u; refine < WATER_SSR_BISECTION_STEPS; refine += 1u) {
        let middle = 0.5 * (in_front + behind);
        if water_ssr_sample(start + span * middle).gap < 0.0 {
            behind = middle;
        } else {
            in_front = middle;
        }
    }
    let hit_ndc = start + span * behind;
    let hit = water_ssr_sample(hit_ndc);

    // A ray that went behind something thicker than a surface did not hit it.
    let thickness = WATER_SSR_THICKNESS_RELATIVE * hit.scene_depth
        + WATER_SSR_THICKNESS_UNITS / perspective_camera_near();
    if hit.penetration > thickness {
        return result;
    }

    let hit_uv = ndc_to_uv(hit_ndc.xy);
    let edge_distance = min(min(hit_uv.x, 1.0 - hit_uv.x), min(hit_uv.y, 1.0 - hit_uv.y));
    let edge_fade = smoothstep(0.0, WATER_SSR_EDGE_FADE, edge_distance);
    let hit_length = distance(position_ndc_to_world(hit_ndc), world_position);
    let length_fade = 1.0 - smoothstep(0.5 * WATER_SSR_MAX_DISTANCE, WATER_SSR_MAX_DISTANCE, hit_length);
    let penetration_fade = 1.0 - smoothstep(0.5, 1.0, hit.penetration / thickness);

    let colour = textureSampleLevel(
        view_transmission_texture,
        view_transmission_sampler,
        hit.pixel / copy_size,
        0.0,
    ).rgb;
    result = vec4<f32>(colour, facing * edge_fade * length_fade * penetration_fade);
#endif
    return result;
}
#endif

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let scale = water.wave_scale_speed_strength.x;
    // The shared view clock: no per-material time, so the material is never rewritten per frame.
    let time = select(globals.time, water.clock.x, water.clock.y > 0.5);
    let phase = time * water.wave_scale_speed_strength.y;
    let strength = water.wave_scale_speed_strength.z;
    let p = in.world_position.xz * scale + water.flow_direction.xy * phase;
    var dx = cos(p.x + sin(p.y * 1.7)) * strength;
    var dz = cos(p.y * 1.3 + sin(p.x)) * strength;
    if water.flow_direction.w > 0.5 {
        let flow = textureSample(water_flow_normal, water_flow_normal_sampler, fract(p * 0.25)).xy * 2.0 - 1.0;
        dx += flow.x * strength;
        dz += flow.y * strength;
    }
    pbr_input.N = normalize(vec3<f32>(-dx, 1.0, -dz));
#ifdef WATER_SSR
    // The reflection is found on screen: the sky where the ray leaves it or misses, the opaque
    // colour where it hits, blended by how sure the hit is. A view the water module did not give a
    // copy has no reflection, as with the planar reflection switched off.
    var reflection_color = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let copy_size = textureDimensions(view_transmission_texture);
    if water_ssr_has_ramp(copy_size) {
        let ray_direction = normalize(reflect(-normalize(pbr_input.V), pbr_input.N));
        let traced = water_ssr_reflect(
            in.world_position.xyz,
            ray_direction,
            in.position.xy,
            vec2<f32>(copy_size),
        );
        // The one place a miss is filled: with the sky.
        let miss_fill = water_ssr_sky(ray_direction, i32(copy_size.y) - 1);
        reflection_color = vec4<f32>(mix(miss_fill, traced.rgb, traced.a), 1.0);
    }
#else
    let viewport_uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;
    let reflection_uv = clamp(vec2<f32>(viewport_uv.x, 1.0 - viewport_uv.y), vec2<f32>(0.0), vec2<f32>(1.0));
    let reflection_color = textureSample(water_reflection, water_reflection_sampler, reflection_uv);
#endif
    // Schlick with F0 = this water's WATR "Fresnel Amount" (DNAM; DefaultWater is 0.10 after
    // Update.esm).
    let water_fresnel_f0 = water.fresnel_reflectivity.x;
    let n_dot_v = clamp(dot(normalize(pbr_input.V), pbr_input.N), 0.0, 1.0);
    let fresnel = water_fresnel_f0 + (1.0 - water_fresnel_f0) * pow(1.0 - n_dot_v, 5.0);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var out: FragmentOutput;
    // The water's own (dark) colour is lit; the reflection is light that already left the scene, so
    // it is blended in after lighting rather than lit a second time as albedo, which washed the
    // water out to white.
    out.color = apply_pbr_lighting(pbr_input);
    let reflected = fresnel * water.fresnel_reflectivity.y;
    out.color = vec4<f32>(mix(out.color.rgb, reflection_color.rgb, reflected), max(out.color.a, fresnel));
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
