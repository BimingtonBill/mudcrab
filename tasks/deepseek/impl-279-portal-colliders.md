# Goal
Track: portal. First half of the collision port: the portal streamer builds upstream main's rapier
colliders (terrain quadrants and statics), enabled only for the space the player is in, and moved
with the render-origin rebase. Nothing collides with them yet: the player half (rapier ray probe in
`player.rs`, rapier plugin in `app.rs`) is a later task. Follow research-275's recipe steps 1, 2, 3,
5 and 6 (appended path) with the decisions fixed here.

# Context
- Your checkout is branch `portal-merge-main` (base 0fa97092) with the pacing, sky-dome and terrain-normals ports
  integrated. Commit prefix `[deepseek]`.
- Main's streamer (source): `C:/Modding/MudcrabProject/local/merge/streaming_main.rs`; what main
  changed: `streaming_base_to_main.diff` (same folder). Recipe:
  `C:/Modding/MudcrabProject/local/merge/research-275-report.md` (sections 2-5). Second opinion:
  `C:/Modding/MudcrabProject/local/merge/port-prep-collision.md`.
- `crates/engine/src/physics.rs` is main's file, compiled but unwired (`world_collision_groups`,
  `PhysicsCorePlugin`); `bevy_rapier3d` is already a dependency; `shared::collision` holds the
  converted collision format the GLBs now carry (`mudcrabCollision` extras).
- Portal's streamer has tiered cells (`CellDetail::{Full, Terrain}`), interiors (`ActiveCell`),
  pre-streamed door destinations (`PrestreamCells`), paced unloads (`Retiring`) and paced model
  arming (`PendingModel`). `crate::portal::isolate_cells` writes each cell root's `CellRole`
  (Active / Destination / Hidden).

# Decisions (fixed)
1. Colliders are built only when `config.portal.walk` is set (a walked run); every other run is
   unchanged. Call the gate one function so the player half can reuse it.
2. Ring cells (`CellDetail::Terrain`) get no colliders. Full cells: terrain quadrants (`TerrainCollider`)
   and statics (main's authored-collision or proxy decision; STAT/TREE/FURN only, as main). No DOOR
   colliders (the player keeps a mesh probe for door leaves).
3. A collider building failure logs and skips that collider; it never fails the cell.
4. Every collider spawns with `ColliderDisabled`. One system, ordered right after
   `crate::portal::isolate_cells` in the same frame, removes `ColliderDisabled` exactly for colliders
   under a cell root whose role is Active, and inserts it for every other role. Surface/asset
   readiness must NOT enable a collider on its own (a pre-streamed destination becomes ready while it
   is not active: port-prep-collision point 2). The system lives in `portal.rs` beside
   `isolate_cells`, or in `streaming.rs` if it only needs a public role query; say which.
5. The rebase body shift (recipe step 5, main's `update_render_origin` body moves with
   `propagate_modified_body_positions_to_colliders`) lands in the same commit as the terrain colliders.
6. Check door frames and arches: a STAT that frames a load door must not get a proxy collider that
   narrows the doorway. Report what main's decision does for the Riverwood door frames
   (e.g. the statics around `0x0001CBB0`) from the code; if a proxy would block, prefer authored
   collision or none for that static and say how you decided.
7. Do not add rapier's plugin to the app or change `player.rs`; tests build their own app.

# Scope
Owned files: crates/engine/src/streaming.rs, crates/engine/src/portal.rs, docs/handoff/impl-279-portal-colliders.md
`portal.rs` only for the enable/disable system and its registration. Everything else read-only.
Tests only, no engine window or GPU. The handoff note is written, not committed.

Acceptance: cargo fmt --all -- --check, cargo clippy -p engine --all-targets -- -D warnings, cargo nextest run -p engine

# Done when
- Acceptance passes; every existing test still passes (list any you changed and why).
- Tests: ring cells have no colliders; a Full cell's quadrants and an eligible static get colliders
  in a walked run and none in an unwalked one; only Active-role cells' colliders are enabled, and a
  crossing flips them in the frame `isolate_cells` changes the roles; a destination cell becoming
  ready does not enable its colliders; a rebase moves fixed bodies with their roots (main's
  `rebase_moves_*` contract); a collider build failure does not fail the cell; a retiring cell's
  colliders go with its subtree.

# Report
Commits, files, systems and schedule placement, tests. The door-frame finding. Commit time added per
cell (from a test or an estimate with reasons). Anything in the research you found wrong.
