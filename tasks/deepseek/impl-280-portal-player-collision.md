# Goal
Track: portal. Second half of the collision port: the portal's own walking player
(`crate::player::PlayerPlugin`) walks on the rapier colliders the streamer now builds, instead of
ray-casting the render meshes. Doors keep working exactly as today. Research-275 recipe steps 4 and 7
(appended path); decisions fixed here.

# Context
- Your checkout is branch `portal-merge-main` (base e0d88d74) with the streaming colliders integrated (impl-279):
  terrain quadrants carry `TerrainCollider`, eligible statics carry `StaticColliderSource`, every
  collider spawns `ColliderDisabled`, and `crate::portal::gate_cell_colliders` (right after
  `isolate_cells`) enables only the player's space. `crate::streaming::builds_world_colliders(config)`
  is the one gate (true for a walked run, `config.walks()`).
- `crates/engine/src/physics.rs` (main's, unwired): `PhysicsCorePlugin` (adds `RapierPhysicsPlugin`
  in the fixed schedule), `world_collision_groups()` / `GROUP_WORLD`. Do not add `PlayerControlsPlugin`
  or `WorldPlayerPlugin`.
- The player: `crates/engine/src/player.rs`. `CollisionWorld` (~362) is the seam; `MeshProbe`
  (~812-845) ray-casts render meshes; `walk_step` (~433) moves the camera-as-player; door logic
  (`player_door`, `player_auto_doors`, `player_walks_through_doors`, `mesh_is_out_of_the_way` ~976)
  must stay as is.
- Recipe: `C:/Modding/OpenSkyrimProject/local/merge/research-275-report.md` (sections 1-5); second
  opinion `C:/Modding/OpenSkyrimProject/local/merge/port-prep-collision.md`.

# Decisions (fixed)
1. A composite probe behind `CollisionWorld`: rapier ray casts (`ReadRapierContext`, `QueryFilter`
   restricted to the world collision group) for ground and walls, plus the existing mesh probe used
   ONLY for load-door leaves (doors have no colliders). Ground height = the higher valid hit.
2. Only enabled colliders count (rapier skips `ColliderDisabled` already; confirm it, and add a test).
3. When no world collider is under the player (ring cell, a cell still loading, the frame a tier
   changes), `walk_step` holds position as today (`WalkOutcome::unsupported`); never fall.
4. Keep portal's walk/run speeds; `MovementTuning` stays unused (a later tuning task).
5. Runs without `builds_world_colliders` keep the mesh probe exactly as today.
6. `PhysicsCorePlugin` is added by the lead in `app.rs` under `builds_world_colliders`; do not edit
   `app.rs`. Your tests add the plugin to their own app. If the rapier ray cast needs a different
   schedule order than `walk_step` has (FixedUpdate vs Update), say so and propose the order.

# Scope
Owned files: crates/engine/src/player.rs, docs/handoff/impl-280-portal-player-collision.md
Everything else read-only. Tests only; no engine window or GPU. The handoff note is written, not
committed.

Acceptance: cargo fmt --all -- --check, cargo clippy -p engine --all-targets -- -D warnings, cargo nextest run -p engine

# Done when
- Acceptance passes; every existing player/portal/transition test still passes (list any changed).
- Tests (headless app with `PhysicsCorePlugin` or rapier directly): the player stands on a terrain
  collider; a static collider blocks a walk into it; a disabled collider (another space) neither
  supports nor blocks; a closed door leaf still blocks and an open one does not (mesh half); no
  collider under the player holds position; an unwalked run uses the mesh probe only.
- Also (review-279 asked for it, and your test app has rapier anyway): a live-rapier test that a
  render-origin rebase (`crate::streaming::update_render_origin`) moves a fixed terrain/static body
  by the same displacement as its cell root, read back from the rapier body's translation.

# Report
Commits, files, the probe's shape and schedule, tests. Whether rapier needed a physics step before a
ray cast sees a freshly enabled collider, and how you handled it. Anything in the research you found
wrong.
