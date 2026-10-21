# Goal

Explain, with file:line evidence, why sun shadows go missing or wrong in the portal demo, and lay out how the doorway view's rendering cost could be cut to "render only at the doorway's size and only what is seen through it". Track: portal. Change no files.

# Context

Mudcrab is a Rust/Bevy 0.19 reimplementation of Skyrim's engine. On the `portal` branch, looking through an open load door shows the destination cell live: a second camera (the portal camera) renders the destination space into a texture drawn on the doorway. The world is split between the two cameras with `RenderLayers` (the destination's cells are on the portal camera's layer; cells the player can't see are on no layer). Each space has its own lighting (sun, ambient, fog) from `crate::atmosphere`.

The user's notes from playing the demo (2026-09-25):
1. **No sun shadows through the doorway:** standing inside a house and looking out through the open door, the exterior shows no sun shadows. It should.
2. **Shadows break after crossings:** sun shadows sometimes stop rendering correctly after a crossing.
3. **Performance isn't great:** the doorway view should render only at the doorway's size on screen, and only what the player can actually see through it (occlusion culling, as the Portal games do).

Code to read (all under `C:\Modding\MudcrabProject`):
- `crates/engine/src/portal.rs`: the portal camera, its render target and size, its layers, the doorway quad, its own sun ("the doorway's own sun").
- `crates/engine/src/atmosphere.rs`: the sun (`DirectionalLight`), cascades and shadow settings, per-space lighting, and what happens on a crossing.
- `crates/engine/src/transition.rs`: the crossing.
- `crates/engine/src/render.rs`: renderer setup.
- `crates/engine/src/app.rs`: camera setup.
- Design notes: `docs/design/portal-performance.md` (the existing plan: items 1-4) and `docs/research/portal-frames.md`.
- Bevy 0.19's sources are in the cargo registry (`%USERPROFILE%\.cargo\registry\src\*\bevy_pbr-0.19.0`, `bevy_render-0.19.0`, `bevy_light-0.19.0` or whichever crate holds `DirectionalLight` and `CascadeShadowConfig`). Check there how directional-light shadow cascades are computed per view (per camera), whether a camera rendering to a texture gets its own cascades, and how `RenderLayers` on a light affect shadows.

Do not read anything under `local/`, `target/` or `tasks/deepseek/` except this brief.

# Scope

Read-only. Change no files.

# Done when

Notes 1 and 2 each have a cause traced to specific lines (or an honest "could not find", with the likeliest candidates), and note 3 has a concrete plan checked against what the code does today.

# Report

- **Note 1:** which directional light(s) exist; which camera renders shadows for which light; why the portal camera's view has no sun shadows; the fix. Cite our code and Bevy's.
- **Note 2:** what changes on a crossing that could break shadows (a light's layers switched, a light despawned and respawned, cascades tied to the old camera, the shadow map resolution, the render origin rebasing); the likeliest cause and how to confirm it.
- **Note 3:** today, what resolution the portal camera renders at, whether it renders when the doorway is off screen, and whether anything culls geometry that can't be seen through the doorway. Then a plan, ordered by payoff and effort:
  - (a) render target sized to the doorway's on-screen rectangle (or a scissor/viewport);
  - (b) skip rendering when the doorway is off screen or occluded;
  - (c) cull through the doorway: an oblique near plane and/or a frustum narrowed to the doorway's outline, so only what's visible through the opening is drawn;
  - (d) cheaper shadows for the portal view.
  For each item: what exists in Bevy 0.19 for it (e.g. `Camera::viewport`, custom frusta, `Frustum`/`NoFrustumCulling`, sub-view), with citations, and an effort estimate.
- **Confidence:** for each finding.
