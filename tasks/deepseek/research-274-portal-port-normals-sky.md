# research-274 - Porting main's terrain normal maps and sky onto the portal branch

Track: portal. Read-only; change nothing.

## Goal
A step-by-step porting recipe for two upstream-main engine features onto the portal branch, whose
render and atmosphere code differ from main's:
1. **Terrain normal maps**: main's `TerrainImages` (colour + normal handles), `normal_0..5` material
   bindings, `set_normal_layers`, the weight-field changes in `TerrainSettings`, and the shader side in
   `terrain.wgsl`.
2. **The sky**: main's `crates/engine/src/sky.rs` (`SkyPlugin`, `SkyCamera`, `FogCamera`) against the
   portal's own `crates/engine/src/atmosphere.rs` (per-space lighting, fog, interior/underground colours,
   destination atmosphere on the portal camera).

## Scope
Read only. Portal tree: `C:/Modding/wkj-portal-merge` (`crates/engine/src/render.rs`,
`shaders/terrain.wgsl`, `streaming.rs` (search `TerrainExtension::from_quadrant`, `spawn_cell`),
`atmosphere.rs`, `portal.rs` (destination atmosphere), `sky.rs` (main's, compiled but not wired),
`app.rs`). Main's copies and the common ancestor are in `C:/Modding/MudcrabProject/local/merge/`:
`main_render.rs`, `main_shaders_terrain.wgsl`, `main_app.rs`, `base_render.rs`,
`base_shaders_terrain.wgsl`, `base_app.rs`, `render_base_to_main.diff` (what main changed in render.rs
and terrain.wgsl: read it first), `streaming_main.rs` (main's `spawn_cell` uses `TerrainImages`).
The converted data already carries landscape normal maps: `AssetCatalog::landscape_normal` in
`crates/engine/src/world/database.rs`.

## Web questions
none

## Done when
For normal maps: the exact list of main's changes to bring (file:line in the main copies), where each
lands in portal's files (file:line), what portal's own terrain changes must be kept (its
`quadrant_base_texture_form_id` base-texture borrowing, its `vertex_weights_only` fixture path,
`TerrainExtension::fixture(textures)`), and the order of steps keeping the build green.
For the sky: what `SkyPlugin` draws and controls, what `atmosphere.rs` already does, where they would
fight (clear colour, fog, ambient, sun, camera components), and a recommendation: wire SkyPlugin, port
parts of it into atmosphere.rs, or leave it out, with reasons and the steps for the recommendation.
Both must keep the portal camera's destination atmosphere working (`portal.rs`).

## Report
1. Normal maps recipe: numbered steps with source and target file:line.
2. Sky: comparison table (feature, main sky.rs, portal atmosphere.rs), recommendation, steps.
3. Tests to port (names) and portal tests affected.
4. Risks and open questions.
Under 250 lines. Cite file:line for every claim.
