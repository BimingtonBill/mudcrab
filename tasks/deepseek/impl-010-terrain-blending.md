# impl-010 - Terrain: smooth layer blending and correctly tiled terrain textures

Owned files: crates/engine/src/streaming.rs,crates/engine/src/render.rs,crates/engine/src/shaders/terrain.wgsl
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md` first. Your checkout
builds into its own `target/`; the first build takes several minutes.

## Goal

Make real Skyrim terrain look right. Two defects, both on screen in the user's demo:

**A. Blocky, stair-stepped layer blending.** Root cause established by research-004 (reviewed):
`docs/research/terrain-blending-seams.md`, "Defect A" and fix step (1). In short: the six layer weights
are packed into `ATTRIBUTE_TANGENT` (three normalized, magnitude in `w`) and `ATTRIBUTE_UV_1`
(`crates/engine/src/streaming.rs`, around the `packed_weights` / `extra_weights` code), and Bevy
re-normalizes the tangent in its vertex shader (`bevy_pbr-0.19.0/src/render/mesh_functions.wgsl`,
`mesh_tangent_local_to_world`), so interpolated weights come out wrong (0.25 instead of 0.5 mid-edge)
and transitions look like hard steps. Also the raw data is sparse and hard-masked (most opacities
exactly 0 or 1 at 128-unit spacing).

**B. Stretched, streaked terrain textures.** In the user's screenshots the ground in places shows long
parallel streaks, as if a texture were stretched along one axis, and some areas are flat grey. Look
at: `C:/Modding/MudcrabProject/local/demo/survey1/03-survey-1.png` (AlftandWorld cavern floor, dark, with long streaks),
`C:/Modding/MudcrabProject/local/demo/survey1/00-door.png` (Tamriel near the Alftand entrance, grey streaks at the bottom), and
`C:/Modding/MudcrabProject/local/acceptance-shakedown/20260922-003549-*/screenshots/rural.png` (top-down). You can see images:
`Read` them. Find the cause in the terrain UV generation (`build_terrain_quadrant_mesh` in
`streaming.rs`), the layer texture sampling in `terrain.wgsl`, the texture sampler settings (address
mode, filtering, mipmaps - check the Bevy 0.19 source in the cargo registry for defaults), or missing
layer textures falling back to something. Say which it is, with evidence.

## What to build

- Carry the six weights so they interpolate linearly and reach the fragment shader intact - either
  custom vertex attributes on the terrain material (Bevy 0.19 `MeshVertexAttribute` + material
  `specialize`), or per-quadrant 17x17 weight textures with linear filtering and clamp addressing as
  research-004 proposes. Choose, and justify in your result.
- Blend softly: a gentle smoothing of the hard 0/1 masks is acceptable (Skyrim itself blends
  bilinearly across the 128-unit sample spacing); do not blur layers across quadrant or cell edges
  such that neighbours disagree (the weld of impl-008 made edges match exactly - keep it that way).
- Fix defect B at its root.
- `CELL_CACHE_VERSION` should not need a bump (the cache holds the raw data); if it does, stop and say
  why instead of changing it - that is a shared contract.

## Tests (must fail before your change)

- CPU: the weight the shader receives at the midpoint between a 1.0 and a 0.0 sample is 0.5
  (today 0.25), and at the quarter points 0.25 / 0.75.
- Adjacent quadrants and adjacent cells produce identical weights on their shared edge.
- Defect B: a test that pins the correct UV/tiling behaviour you identified.
- Existing tests keep passing, including the weld and terrain fixture tests.

## Scope and crosstalk

Only the three owned files. `impl-009-door-portals.1` runs at the same time and owns
`crates/engine/src/portal.rs`; it may message you asking for a tiny hook in `streaming.rs` (for
example a component on cell roots carrying their `CellKey`). If the request is small and does not
conflict with your work, add it, tell it the exact name you used, and list it in your result; otherwise
decline with a reason. Quote any message that changed your work.

## Done when

Acceptance commands pass. Report per the implementation template, including which option you chose
for the weights and the root cause of defect B.
