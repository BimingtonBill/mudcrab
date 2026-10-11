# Goal
Track: Phase 2. Find what the main world (Bevy's main schedule) does on the frames where streamed
models arrive, and which of it is unbudgeted, so a coder can spread it over frames. Read-only.

# Context
- Engine: upstream main, read at `git -C C:/Modding/wkj-phase2 show origin/main:<path>` (the
  `crates/engine/src/` files: streaming.rs, app.rs, render.rs, lights.rs if present, model loading).
  Bevy 0.19 vendored sources: `C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`
  (bevy_scene-0.19.0, bevy_gltf-0.19.0, bevy_asset-0.19.0, bevy_ecs-0.19.1, bevy_transform-0.19.0).
- Measured (stress scenario: camera flying, stream radius 3; ~12,000 entities, ~15 model instances
  per cell, 54 references per cell; C:/Modding/Mudcrab/worktrees/p2-tools/crossing-ab/summary.txt has all numbers):
  frame p50 ~4.7-6.2 ms, worst ~18-25 ms. The worst frames all carry `asset_ready` events (models
  finishing their load) and usually a cell commit. The world-origin rebase costs nothing. Throttling
  GPU uploads (Bevy's RenderAssetBytesPerFrame) halved the render thread's asset phase, but the main
  world (First..Last) still peaks at 13-16 ms on those frames (p99 ~7 ms, mean ~1.1 ms).
- Already budgeted: cell commits (`max_cell_commits_per_frame` = 1, a microsecond budget), and (on a
  local branch, not main) cell unloads at 2 per frame. Measured spans: streaming/cell_commit mean 2.8,
  worst 7.7 ms; assets/readiness_scan worst ~0.9 ms.

# Questions
1. When a glTF model finishes loading, what runs in the main world, in order, and on which frame:
   the engine's `SceneRoot`/scene handle insertion, Bevy's scene spawner (`scene_spawner_system`:
   does it instantiate every ready scene in one frame, unbudgeted?), hierarchy/transform propagation
   of the new subtree, the engine's readiness validation (`track_asset_readiness`: per-instance
   transform/bounds/material/image checks), material/mesh component setup, and any per-instance work
   in render.rs or other plugins (e.g. materials rebuilt, water/terrain hooks, lights).
2. For each: cost driver (entities, nodes, primitives, validation passes), whether it is already
   limited per frame, and a cost estimate from the numbers above (mark measured vs estimated).
3. Where can a budget go? E.g. cap how many scene instances the engine lets Bevy spawn per frame
   (insert `SceneRoot` for at most N ready models per frame), spread validation, defer non-visible
   work. Name the file/function and the invariant each must keep (no model missing forever, the
   streaming readiness metrics still converge, no double spawn).
4. What instrumentation would confirm it (a span around the scene spawner, per-frame instance count).

# Scope
Read only. Write nothing except your report. Do not run cargo or the engine.

# Report
Sections: Arrival frame (ordered steps with file:line), Costs (table: step | driver | limited? |
estimate | measured/estimated), Budget options (ranked, with invariants and the smallest first
commit), Instrumentation, Doubts.
