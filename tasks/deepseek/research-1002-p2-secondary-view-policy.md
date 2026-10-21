# Goal
Design study for upstream issue #187 (Phase 2 track): a policy layer for secondary cameras (first the water reflection camera) that bounds their cost: target size, update rate, priority, shadows on/off, allowed staleness, and invalidation on teleports and door crossings. Output: a concrete design that fits the current engine on stock Bevy 0.19, ready for a coder brief.

# Context
- Code: the upstream main checkout is read-only at D:/Mudcrab/worktrees/p2-fixture (upstream main 1b70b9b0 plus one test change; treat it as main). The water reflection camera lives in crates/engine/src/render.rs (about lines 705-801: `update_water_reflection_camera`, `surface_in_view`, `WaterSurface`), cameras are set up in crates/engine/src/app.rs, streaming and the render origin in crates/engine/src/streaming.rs, the player and teleports in crates/engine/src/physics.rs.
- Bevy 0.19.0 sources are in the cargo registry: C:/Users/Brannock/.cargo/registry/src/ (bevy_render-0.19.0, bevy_pbr-0.19.0, bevy_camera if present, bevy_core_pipeline-0.19.0).
- Earlier findings (read, do not redo): C:/Modding/Mudcrab/mudcrab/local/research/frame-cost-audit-2026-09-25.md (one Riverwood frame runs three scene views and 12 directional shadow cascades, much of it for the reflection image), C:/Modding/Mudcrab/mudcrab/local/agents/runs/research-269-portal-water-indoors.1/report.md (the reflection camera renders inside closed houses: surface_in_view is a pure frustum test), C:/Modding/Mudcrab/mudcrab/local/research/water.md.
- The issue's own text is in the brief's Goal; key points: per-view policy (size, update rate: every frame / every other frame / on demand, priority, shadows, staleness), refresh at once on teleport / door crossing / large camera moves, each view keeps its own culling, a view can later be marked "traced" for a path-traced mode, start on stock Bevy, a general per-view budget may later go to Bevy.

# Questions to answer, each with file:line evidence from the code or Bevy's source
1. What does the reflection camera cost today, structurally: which passes does it run (prepass, shadows: does it render its own directional cascades or share the main camera's?), at what resolution, and when is it active?
2. How can a Bevy 0.19 camera be throttled: `Camera::is_active` toggling per frame, a render-target that is not re-rendered, `RenderTarget` image reuse; what goes stale or breaks (TAA/history, exposure, prepass textures) when a camera skips frames? How do you stop a camera rendering shadows (cascade config per view, `NotShadowCaster`/visibility layers, `ShadowFilteringMethod`, or a per-camera light filter)? Cite the Bevy source.
3. A data model: a `ViewPolicy` component (fields and defaults) and one system that applies it each frame; where it sits in the schedule; how it reads a global frame budget later.
4. Invalidation: which existing events or state changes mean "the view is wrong now" (a teleport, a render-origin rebase, an interior/exterior switch, a camera jump larger than X); list the existing types/messages, or say none exist.
5. How to measure the benefit with the existing benchmark tools (per-pass GPU time `render/<pass>/elapsed_gpu` from RenderDiagnosticsPlugin; name the reflection camera's pass paths).
6. Risks and what is left for a Bevy contribution.

# Scope
Read only. Change nothing.

# Report
Status, Verdict, Summary; then one section per question with file:line citations (mark anything you could not verify as UNVERIFIED); then a proposed implementation plan as 2-4 small commits, each with owned files and tests.
