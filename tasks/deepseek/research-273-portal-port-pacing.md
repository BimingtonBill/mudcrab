# research-273 - Porting main's paced cell unloads and paced model spawns onto portal's streamer

Track: portal. Read-only; change nothing.

## Goal
Produce a step-by-step porting recipe for two features of upstream main's
`crates/engine/src/streaming.rs` onto the portal branch's own `streaming.rs`, whose cell state machine
is different. The two features:
1. **Paced cell unloads** (`CellStatus::Retiring`, `despawn_cells`, `max_cell_unloads_per_frame`, the
   retire backlog, revived cells).
2. **Paced model spawns** (`PendingModel`, `arm_pending_models`, `max_model_spawns_per_frame`,
   `SceneSpawnBatch`, `next_model_sequence`, and the readiness scan changes that go with them).

## Context
- Portal's streamer (the target): `C:/Modding/wkj-portal-merge/crates/engine/src/streaming.rs`. It has
  tiered cells (`CellDetail::{Full, Terrain}`), `CellStatus::{Loading{generation, detail, replaced},
  Resident{root, detail}, Failed{detail}}`, `ActiveCell` (interiors), `PrestreamCells` (door destinations),
  `full_failed`, interior retries, a `pinned` set, and a per-frame commit budget
  (`commit_waits_for_next_frame`). Its unloads are immediate (`plan_cells`'s `streaming.cells.retain`).
- Main's streamer (the source): `C:/Modding/OpenSkyrimProject/local/merge/streaming_main.rs`. The common
  ancestor is `local/merge/streaming_base.rs`; `local/merge/streaming_base_to_main.diff` is exactly what
  main changed (read this first: it isolates the two features).
- The config flags already exist on the portal side: `crates/engine/src/config.rs`
  (`max_cell_unloads_per_frame`, `max_model_spawns_per_frame`, `max_upload_mib_per_frame`). The
  `StreamingMetrics` fields main's code writes also already exist (they are zero today).
- Constraints the port must keep: a door crossing (`crates/engine/src/transition.rs`) despawns the
  worldspace just left and needs its destination resident; an interior that is neither active nor
  pre-streamed must not stay drawn; a tier change keeps drawing the old root until the replacement
  commits (`replaced`); `validate_streaming_lifecycle` must count no orphaned or out-of-range roots.

## Done when
For each feature you can say: which main functions/types/systems to bring over (file:line in
`streaming_main.rs`), where each hooks into portal's code (file:line in the portal file), how
`Retiring` combines with portal's `Loading{replaced}` and tiers (a retiring cell asked for again at
another tier, a retiring interior when the camera crosses a door, a pinned or pre-streamed cell),
which main tests to bring and which portal tests would change, and the order of steps so the build
stays green between them.

## Report
1. Feature 1 recipe: numbered steps, each with source and target file:line.
2. Feature 2 recipe: same.
3. Interactions with tiers, `ActiveCell`, `PrestreamCells`, `pinned`, door crossings: each case and the
   rule you propose.
4. Tests: main tests to port (names), portal tests affected (names).
5. Risks and anything you could not settle from the code.
Keep it under 250 lines. Cite file:line for every claim.
