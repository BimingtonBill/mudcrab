# research-197 - What does opening a door for the first time cost, and how can it be paid before the door opens?

Track: portal. Read-only: change nothing.

## Goal

The user's backlog idea (`docs/roadmap/backlog-engine-and-portal.md`, "Portal plugin > Performance" item 6): prepare an interior before its door is opened, including shaders and GPU buffers, not only its cell data. The cell data part already exists: `plan_door_prestream` in transition.rs pre-streams within 800 units.

Find out what still happens on the first frames a door opens, and design how to move it earlier.

## Questions

1. **What is created or compiled when the portal camera first draws a destination?** Answer from the code, with file:line:
   - render pipelines specialized per view (Bevy 0.19 `SpecializedMeshPipelines`, `PipelineCache`: are pipelines keyed per view, so the portal camera's HDR-off, MSAA-off, no-tonemap view needs its own variants of every material pipeline?);
   - the portal render target: `resize_portal_target` (portal.rs);
   - the destination's lights and shadows (the portal sun's cascades);
   - the meshes and textures uploaded when the cells stream.
   Which of these happen only on the first open, and which again on every open?
2. **Can the portal camera render the destination before the door opens,** at a tiny size or into a 1x1 target, with the same view settings? That would make Bevy compile the pipelines and allocate buffers early. What would it cost per frame while a door is merely near?
3. **Does Bevy 0.19 have a pipeline cache warm-up, or a way to queue pipeline compilation without drawing?** Check `PipelineCache` and `bevy_render`'s shader cache.
4. **Recommendation:** the cheapest design that removes the first-open hitch, plus what to measure to prove it (e.g. frame time over the first 30 frames after pressing E, first open against second open).

## Context

- Code: `crates/engine/src/portal.rs`, `transition.rs`, `streaming.rs`.
- Bevy source: `C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.
- Under `local/`, read only `local/agents/research-175.out`.

## Report

The answers with file:line, and the design in about ten lines.
