# Goal
Design, without implementing, how the upstream engine's terrain shader can use each landscape
layer's normal map, so snowy slopes, rock and ice show surface relief instead of looking flat
and matte. Say whether it fits within wgpu/Bevy limits, and the smallest safe design.

# Context
- Code: `C:/Modding/wkj-phase2` (branch `phase2/integration-lod-shots`, read-only). The terrain
  shader is `crates/engine/src/shaders/terrain.wgsl`; the Rust side, `TerrainMaterial`, its
  bindings and its mesh building, is in `crates/engine/src/render.rs` and `streaming.rs`
  (search `TerrainMaterial`, `terrain.wgsl`, `ATTRIBUTE_TANGENT`, `uv_b`).
- Today:
  - the shader samples six layer diffuse textures (bindings 100-111, one sampler each) and
    blends them by weights;
  - the weights are **packed into the vertex tangent attribute** (`world_tangent.xyz`) and
    `uv_b`, so the mesh has no real tangents;
  - no layer normal map is sampled (research-554 found this).
- Landscape texture sets have normal maps: DB `landscape_textures.texture_set_id` points at
  `texture_sets.normal_path` in `C:/Modding/SkyrimConverted-upstream15/skyrim_world.db`, and the
  converter converts every DDS, so the normal KTX2s exist under `textures/landscape/`. Check.
- Constraints to check: wgpu's default `max_sampled_textures_per_shader_stage` (16) and
  `max_samplers_per_shader_stage` (16) against what StandardMaterial already binds in Bevy 0.19,
  plus the six layers; and whether Bevy raises those limits on this adapter.

# Questions
1. How many textures and samplers does the terrain material bind now, including
   StandardMaterial's? How many are left?
2. Can the six normal maps share one sampler, or go in a texture array? The layers can differ
   in size, and Skyrim's landscape textures are mostly 1024 or 2048.
3. Tangent frame with no vertex tangents: derive it in the fragment shader from the world
   normal and the terrain's UV axes (the UV runs along world X/Y), or use derivatives. What is
   correct for a height field?
4. How to blend six normals by weight (e.g. weighted sum in tangent space, then normalize),
   and the green-channel convention for Skyrim DirectX normal maps (flip Y?).
5. Cost: extra texture reads per terrain fragment. Any frame-rate risk for the Phase 2
   commit-budget acceptance?
6. Does it need a converter change or a schema bump?

Answer from the code and the Bevy/wgpu sources under
`C:/Users/Brannock/.cargo/registry/src` (read-only), with file:line.

# Scope
Read-only. Write only `local/research-556/REPORT.md`, plus any scripts you need under that
folder (run them as `python <file>`).

# Report
A short design: bindings, shader changes, the Rust changes and their files, the limits check
with numbers, the risks, and a size estimate. Then a yes or no on "fits tonight as one focused
change".
