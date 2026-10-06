# Goal
Track: Phase 2 (upstream, issue Mudcrab-Team/mudcrab#112, PR B "go through a load door"). On main, a
player who looks at a load door and presses E fades to black, the world switches to the door's destination
space (exterior to interior, interior to exterior, or exterior to another worldspace), the player is placed
at the door's XTEL arrival point, and the view fades back in once the landing cell is loaded. No preload yet
(that is the next PR): the fade simply holds until the destination is resident.

# Context
- Base: branch `phase2/door-preload` at the commit the lead names in "This run" (it carries the door-link
  reader: `crate::doors::LoadDoor` on every spawned load-door reference, links read from the optional
  `door_links` table). Own worktree on D: (`git -C C:/Modding/MudcrabProject worktree add
  D:/Modding-overflow/wt-693 <base> --detach`, `CARGO_TARGET_DIR=D:/Modding-overflow/wt-targets/impl-693`).
- Upstream style: subjects like `feat(engine): go through load doors with a fade`, no `[claude]` prefix,
  each commit ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. No portal code.
- What main has (research #690, `C:/Users/Brannock/.claude-deepseek/agents/runs/research-690-p2-door-preload-main.1/report.md`):
  exterior-only planning around the camera (`streaming.rs` `plan_cells`, `streaming_center`); interiors load
  only in `--streaming-fixture` (`app.rs` ~503-540, `request_cell(CellKey::Interior)`); interior roots sit at
  `Vec3::ZERO` with absolute Creation coordinates; `streaming.rs` ~401-405 says a runtime space change must
  unload the old space at once; `CameraSpace` (`sky.rs`) has no runtime writer; the only E binding is the
  debug tankard grab (`physics.rs`).
- Reference only (read, never copy portal-only parts): the portal's crossing in
  `C:/Modding/wkj-portal-merge/crates/engine/src/transition.rs` (`ActiveCell`, `switch_space` ~1216,
  `apply_door_crossings` ~1265, `destination_is_resident` ~1122, `landing_style` Snap) and research #691
  (`C:/Modding/wkj-portal-merge/local/agents/runs/research-691-p2-door-preload-portal.1/report.md`).

# Design (fixed)
1. An active-space resource (worldspace id, optional interior id). The planner wants only the active
   interior when there is one, else the exterior cells around the camera in the active worldspace. On a
   switch, every cell of the space left behind unloads at once (not paced). The streaming fixture keeps
   working as today.
2. Activation: E while the camera looks at a load door within 200 units (a ray from the camera hitting the
   door reference's meshes, or its model bounds if simpler; say which). One crossing at a time; E is ignored
   while a crossing runs. Do not change the tankard code; if E reaches both, say so in the report.
3. The crossing, one state machine: fade out (0.25 s, full-screen black `bevy_ui` node) -> at full black,
   switch the space, set the render origin for an exterior landing (the arrival's grid), send the teleport,
   set `CameraSpace` -> hold black until the landing cell is resident and nothing of it is pending (models,
   surfaces; use `StreamingMetrics`), at most 10 s -> fade in (0.25 s). Log one INFO line per crossing with
   the door, destination and the milliseconds from E to fade-in start.
4. Teleport: add to `physics.rs`, in its own block near the player-movement code (agreed with its owner):
   an event `TeleportPlayer { position, yaw }`, position in RENDER space (say so in the doc comment), applied
   by one system before the Rapier step: set the PlayerBody Transform, zero Velocity, reset the walk
   controller's vertical speed / grounded state, set the camera yaw; in noclip move only the camera. One
   headless test. Touch nothing else in physics.rs.
5. Arrival point: XTEL position/rotation are Creation coordinates; convert with the existing coordinate
   helpers (`shared::coordinates`, `grid_translation`); interiors use absolute coordinates (origin unchanged).
6. A door whose destination cannot be loaded (cell fails) fades back in where the player was, with a WARN.

# Scope
Owned files: `crates/engine/src/doors.rs`, new `crates/engine/src/door_crossing.rs` (or a name you choose),
`crates/engine/src/streaming.rs`, `crates/engine/src/app.rs` (plugin registration, fixture compatibility),
`crates/engine/src/physics.rs` (the teleport block only), `crates/engine/src/lib.rs`. Read-only: everything
else; never `console.rs`, `crates/converter/**`, `crates/shared/**`.

Acceptance: `cargo fmt --all -- --check`, `cargo nextest run -p engine`; run clippy `-p engine
--all-targets -- -D warnings` and fix only warnings in your lines (main is red from a new Rust stable).
No engine window or GPU runs; the lead takes the evidence on real data.

# Done when
Headless tests: E at a load door starts a crossing, E elsewhere does not; the space switches at full black
and the old space's cells are gone the same frame; the fade-in waits for the landing cell and starts within
a frame of it being ready; the 10 s timeout fades in anyway with a WARN; exterior landing sets the render
origin to the arrival grid; an interior landing leaves it alone; `TeleportPlayer` moves the body and zeroes
its velocity; the streaming fixture tests still pass.

# Report
Commits, files, line delta, the activation rule chosen, tests, clippy (yours vs pre-existing), and how to
start the engine near a load door for the lead's evidence run (existing flags, e.g. a start position).
Remove the target dir when done; keep the worktree.
