# research-272 - A proper cascade-count setting for the portal view's sun

Track: portal. Read-only; change nothing.

## Goal
Design (do not implement) a setting that gives the portal camera's destination sun fewer shadow cascades than the main view's sun (for example 1 or 2 instead of 4), without the hack of the unmerged branch `claude/portal-sun` (commit fc0f29f), which padded Bevy's per-view cascade frusta.

## Context
- The destination sun: `crates/engine/src/portal.rs` (search `place_destination_sun`, `update_destination_atmosphere`, `portal_sun_cascades`, the `PortalCamera` and the sun's `RenderLayers`). A const `portal_sun_cascades` exists (~324, ~2580).
- The reflection view already limits its sun's cascades per view: `crates/engine/src/render.rs`, search `reflection_cascades`, `Cascades`, `CascadesFrusta`, and the comment explaining why the per-view frusta must stay full (Bevy's `check_dir_light_mesh_visibility` indexes per-thread lists by the previous view's cascade count and panics otherwise). The test `the_reflection_view_keeps_only_the_cascades_asked_for_and_every_frustum` pins that design.
- Bevy 0.19 source is readable under `C:\Users\Brannock\.cargo\registry\src\` (bevy_light-0.19.*, bevy_pbr-0.19.*): how `Cascades` are built per (light, view), how `CascadeShadowConfig` is read, and whether every shadowed directional light seen by one view must share a cascade count.
- fc0f29f is in the repository's git history (branch `claude/portal-sun`); you cannot run git, so if you need it, say what you would check.
- The bench that will measure it: `tools/bench/portal_bench_doors_out.json` with states `closed-out` / `open-out-view` (`crates/engine/src/portal_bench.rs`).

## Done when
You can say whether the reflection view's approach (truncate the per-view `Cascades` after Bevy builds them, keep `CascadesFrusta` full, run in `PostUpdate` after `SimulationLightSystems::UpdateLightFrusta`) works for the portal camera's destination sun too, or why not, and give the exact systems, schedule placement, config flag name and default, and the tests that would pin it.

## Report
1. How Bevy 0.19 builds cascades per view (file:line) and the constraint that broke fc0f29f.
2. The proposed design, the files and functions to change, the flag (`--portal-sun-cascades N`, a graphics setting, or both) and its default.
3. The tests (unit, no GPU) and the bench A/B command.
4. Risks.
