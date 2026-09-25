# research-190 - What does one frame draw in the Riverwood demo, and what can be dropped?

Track: portal (the doorway items), with main-engine items to hand to Phase 2. Read-only: change nothing.

## Goal

A list of every render pass the engine runs per frame in the Riverwood walked demo, outside, with one load door open, and a ranked list of cheap wins **outside** the doorway work already planned. The doorway view (portal camera) is covered by `local/agents/research-175.out` section 3 and briefs `tasks/deepseek/impl-188-*.md` / `impl-189-*.md`; do not redo it, only refer to it.

## Known from a capture

The user's F12 state snapshot (`local/captures/2026-09-25_02-01-35/02.json`) lists three active cameras: the main camera (order 0, layers [0,1]), the portal camera (order -2, layer [2]), and a third camera (order -1, layer [0]) - research-175 says it is the water reflection (`render.rs` ~586-598). Two directional lights with shadows (the main sun and the portal sun). Frame time 13-16 ms at 1600x900.

## Questions (answer each with file:line in `crates/engine/src/` and, where it matters, the Bevy 0.19 source in `%USERPROFILE%\.cargo\registry\src\*\bevy_*-0.19.*`)

1. Every camera: what it renders, at what resolution, which render layers, whether it has depth/normal/motion prepasses, occlusion culling, HDR, bloom, tonemapping, MSAA; and **when it runs** (always, or only when something needs it). In particular the water reflection camera: does it render every frame even with no water on screen, at full resolution, with the whole scene and shadows?
2. Every shadow pass: which lights cast (sun, portal sun, point lights: `lights.rs`'s budget - how many point lights cast shadows?), cascade count and map sizes, and whether casters include small clutter that could opt out.
3. Post effects and extras per frame: bloom, fog, atmosphere, SSAO or others, the terrain ring (`--terrain-radius`), snow, effect palettes; anything that runs a full-screen pass.
4. CPU side in `Update`/`PostUpdate`: systems that loop over every entity each frame (streaming, visibility, door checks, billboards `billboard.rs`, material animation `material_animation.rs`, model animation) - the ones likely to show up in a profile, with why.
5. Rank the cheap wins: for each, the change in one sentence, the expected saving (rough), the risk, and **whose track**: portal (portal.rs, doors, demo) or main engine (Phase 2: render.rs water, lights, terrain, materials).

## Context

- Read only `local/agents/research-175.out` and `local/captures/2026-09-25_02-01-35/*.json` under `local/`.
- No runs: this is a code and source reading.

## Report

The pass list (1-4) as a table, then the ranked wins (5).
