# Goal
Track: portal. Port upstream main's terrain normal maps onto the portal branch: each landscape
layer's normal map is bound to the terrain material and blended by the layer weights in
`terrain.wgsl`, so lit slopes show their surface detail. Follow the recipe of research-274 and the
second opinion appended below.

# Context
- Your checkout is branch `portal-merge-main` (base 6d7434d6) with the pacing port and the sky dome integrated. Commit prefix
  `[deepseek]`.
- Main's copies (source): `C:/Modding/OpenSkyrimProject/local/merge/main_render.rs`,
  `main_shaders_terrain.wgsl`, `streaming_main.rs`; what main changed in render.rs + terrain.wgsl:
  `render_base_to_main.diff` (same folder). Research and second opinion:
  `C:/Modding/OpenSkyrimProject/local/merge/research-274-report.md` (section 1, normals) and
  `C:/Modding/OpenSkyrimProject/local/merge/port-prep-normals.md`.
- The converted data carries landscape normal maps; `AssetCatalog::landscape_normal(form_id)` exists
  (`crates/engine/src/world/database.rs`).
- Fixed decisions:
  1. The weight field is already on portal: port only the normal-specific half (uniform fields
     `normal_layers_0/1`, texture bindings 113-118 `normal_0..5`, `set_normal_layers`,
     `drop_normal_maps`/`has_normal_maps`, `TerrainImages`, the wgsl `layer_normal`/`terrain_tbn`
     blend, `NORMAL_GREEN_SIGN`) plus main's grid clamp helper (`GRID_LAST_SAMPLE`/`grid_point`) and
     its two clamp tests (second opinion, point 1).
  2. Keep portal's own terrain code: `quadrant_base_texture_form_id` base-texture borrowing (a
     borrowed base takes the borrowed form id's normal map too), `TerrainExtension::fixture(textures)`,
     the `vertex_weights_only` path, and portal's `weight_source: Vec4::X` in `for_quadrant`.
  3. Normal maps load linear (`is_srgb = false`) with the repeating terrain sampler.
  4. A normal map that fails to load drops all normal maps of that material (`drop_normal_maps`, which
     must also clear the flags) and the quadrant still becomes ready; count it under a
     `terrain/normal_map_fallbacks` profiler counter. A missing normal map is never bound as Bevy's
     1x1 white.
  5. Port main's `SurfaceImageKind { Terrain, TerrainNormal, Water }` colour-space + sampler check in
     `validate_surface_dependencies` (two call sites).
  6. `images_validated` keeps its meaning: say in the report whether normals are now counted.
- The pacing port changed `spawn_cell` and the readiness systems in `streaming.rs`; build on it, do
  not undo any of it.

# Scope
Owned files: crates/engine/src/render.rs, crates/engine/src/shaders/terrain.wgsl, crates/engine/src/streaming.rs, docs/handoff/impl-278-portal-terrain-normals.md
Everything else read-only. Tests only; no engine window or GPU. The handoff note is written, not
committed.

Acceptance: cargo fmt --all -- --check, cargo clippy -p engine --all-targets -- -D warnings, cargo nextest run -p engine

# Done when
- Acceptance passes; every portal test still passes (list any you had to change and why).
- Ported tests: `dropping_normal_maps_unbinds_them_and_clears_their_flags`,
  `terrain_uniform_layout_matches_the_shader_source` (with its shader-parsing helpers), the grid clamp
  tests, `terrain_sampler_check_rejects_default_and_clamped_images`; plus new: a borrowed base
  quadrant binds the borrowed layer's normal map; a quadrant whose layer has no normal map leaves
  that flag 0.

# Report
Commits (research steps 1-4 as separate commits, each green), files and tests. Where the normal-map
flag is set and where it is cleared (file:line). Anything in the research you found wrong.
