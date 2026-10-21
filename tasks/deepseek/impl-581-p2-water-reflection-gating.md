# Goal
Track: Phase 2 (for upstream). The water reflection camera renders only while a water surface can
be seen by the main camera, so frames where no water is on screen skip one whole scene view and its
four sun shadow cascades.

# Context
- Your checkout is cut from `main` (upstream). Upstream commit style (`perf(engine): ...`), no
  `[deepseek]` prefix, no project-internal references (portal, DeepSeek, task numbers) in code or docs.
- Facts from a frame-cost audit (read against a fork of this engine; find the matching code on
  `main` yourself in `crates/engine/src/render.rs`: `setup_water_reflection`,
  `update_water_reflection_camera`, the `WaterSurface` component):
  - The reflection camera renders at a fixed 1024x576 (`Rgba8Unorm`), order -1, the whole scene
    except the water planes, with `DepthPrepass` and `OcclusionCulling`.
  - It is active whenever any `WaterSurface` entity exists anywhere in the streamed world, on screen
    or not; only water's total absence turns it off. Water planes are spawned for full-detail cells
    and for terrain-ring cells that reach the water level (`crates/engine/src/streaming.rs`).
  - Bevy 0.19 builds sun shadow cascades per (light, active camera), so while the reflection camera
    is active the sun renders 4 extra 2048x2048 cascades (`bevy_pbr-0.19.0/src/render/light.rs`
    around lines 1745-1821; `bevy_light-0.19.0/src/cascade.rs` around 195-232, the `is_active` gate).
  - Estimated saving in a river town facing inland: order 15-25% of the frame.
- The gate: in `update_water_reflection_camera`, set the reflection camera active only when at
  least one water plane's bounds intersect the main camera's view frustum. Test the plane's bounds
  (its `Aabb` in world space, or the plane's rectangle), not its centre, so the reflection is
  already rendered by the frame water first enters the view. Add a margin (one frame of
  camera movement is enough; a named constant with a comment) so a fast turn cannot show one stale
  reflection frame. Reuse Bevy's own frustum (`Frustum` on the main camera, `intersects_obb` /
  `Aabb`) rather than hand-written plane maths if it fits.
- Keep the reflection texture and its material binding as today when inactive (the last image stays
  bound); only `Camera::is_active` changes. If the planes' reflection uniforms are updated only
  while active, say so and make sure a re-activated camera is updated in the same frame.
- The `RendererMetrics` / profiling reports may count views or passes; if a test or report asserts
  the reflection camera is always active when water exists, update it and say why.

# Scope
Owned files: `crates/engine/src/render.rs`, tests inside it, and your handoff note
`docs/handoff/impl-581-p2-water-reflection-gating.md`. If the frustum test needs a small helper in
another engine file, stop and ask in your report instead of editing it. No new dependencies.

# Done when
- Unit tests (a Bevy `App` with `MinimalPlugins` or the existing test harness in render.rs, no
  GPU): water in front of the camera → active; water behind the camera or beyond the far plane →
  inactive; a plane whose centre is outside the frustum but whose edge is inside → active; no water
  → inactive (today's behaviour kept); the reflection camera's transform is still updated on the
  frame it becomes active.
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p engine --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings`
  `cargo test -p engine --target-dir C:/Modding/shared-check-phase2`

# Report
Diff summary, the exact gate (which bounds, which frustum, the margin), test names and results,
and what a timing run should compare (scenario, expected change).
