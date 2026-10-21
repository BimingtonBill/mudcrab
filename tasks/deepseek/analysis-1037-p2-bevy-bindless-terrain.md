# Goal
Work out from Bevy 0.19.0's source how Mudcrab's merged-terrain material should bind its land-texture arrays through Bevy's bindless / binding-array support, and what the fallback looks like, with the Bevy 0.20 status.

# Context
- Mudcrab (Bevy 0.19.0, wgpu 29). The approved terrain contract (v3): a game-wide set of KTX2 texture ARRAYS, one per used class (usage diffuse/normal x block format BC1/BC3/BC5/BC7 x size 1024/2048/4096; vanilla uses about 6), plus a lookup table id -> (diffuse class, layer; normal class, layer). One shared terrain material for all cells; per-vertex u16 land-texture ids (8 or 16 slots) and unorm8 weights. Contract file: C:/Modding/Mudcrab/worktrees/p1-terrain-merge/crates/shared/src/terrain_mesh.rs. Today's terrain material: C:/Modding/Mudcrab/worktrees/p2-main/crates/engine/src/render.rs (TerrainExtension, ExtendedMaterial<StandardMaterial, TerrainExtension>) and shaders/terrain.wgsl; background C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/analysis-1034-terrain-path-map.md.
- Primary path wanted: ONE binding per usage holding all classes (binding_array<texture_2d_array<f32>>), indexed by class id per slot (non-uniform per fragment), when the adapter supports it. Fallback: shader-def specialisation binding only the used classes as separate texture_2d_array bindings. Refuse clearly if the fallback exceeds max_sampled_textures_per_shader_stage.
- Bevy source: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ (bevy_render-0.19.0, bevy_render_macros-0.19.0, bevy_pbr-0.19.0, bevy_image-0.19.0, wgpu-*).

# Scope
Read only. Change nothing.

# Questions (file:line each)
1. AsBindGroup in 0.19: can `#[texture(N, dimension = "2d_array", ...)]` be combined with a binding array of N textures (the `binding_array(...)` modifier, `#[bindless]`), and how does Bevy's bindless material machinery (render_resource/bindless.rs, bindless.wgsl, material_bind_groups.rs) slab textures: does it support texture_2d_array resources and a variable count? What does a material using it look like (attributes, WGSL declarations, indexing helpers)?
2. Feature detection: which wgpu features/limits Bevy checks for bindless (TEXTURE_BINDING_ARRAY, SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING, PARTIALLY_BOUND_BINDING_ARRAY, max_binding_array_elements_per_shader_stage), whether Bevy requests them by default, and what it does when they are missing (automatic fallback to non-bindless?). Does an ExtendedMaterial<StandardMaterial, _> work with bindless in 0.19?
3. Is Bevy's bindless model (one big slab of many material instances) even what we need? We have ONE material instance with an array of texture arrays; maybe a plain custom bind group layout with `BindingType::Texture { view_dimension: D2Array }` and `count: Some(n)` (wgpu binding arrays) via a manual `AsBindGroup` impl is simpler. Compare the two and recommend one, with the exact wiring (layout entries, how to build the bind group from Handle<Image>s, WGSL declarations `binding_array<texture_2d_array<f32>>`, sampler, and non-uniform index usage in WGSL/naga).
4. Requesting device limits: how to raise max_texture_array_layers and max_sampled_textures_per_shader_stage / binding-array limits and enable the features through Bevy 0.19's RenderPlugin/WgpuSettings, and how to read the adapter's real limits at startup to refuse clearly.
5. Bevy 0.20 status for each of the above: "in 0.20: yes / PR #N / no" (check the bevy main branch sources if present in the registry; otherwise say UNVERIFIED and what to check on GitHub).

# Done when
Every question is answered with file:line evidence, and the report ends with the recommended wiring as a short spec a coder can implement (Rust types, layout, WGSL snippet, feature/limit requests, fallback rule).

# Report
Findings per question, then the wiring spec, then anything unverified.

# Foundations check (answer briefly at the end)
What shortcut would each option take, and what would break for a mod (more classes) or another GPU (16-texture limit, no binding arrays)?
