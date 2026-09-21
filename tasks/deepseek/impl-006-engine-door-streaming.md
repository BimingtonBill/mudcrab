# impl-006 - Engine: active cell, interiors, and seamless load-door crossings

Owned files: crates/engine/src/streaming.rs,crates/engine/src/world/database.rs,crates/engine/src/transition.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`, the design
`docs/design/blackreach-demo.md`, and the contract `crates/engine/src/doors.rs` first.

## Goal

Let the engine move between worldspaces and interior cells at runtime through load doors, with the
destination already streamed in before the crossing, so going through a door never shows a loading
screen or an empty world.

## Background (verified research)

`docs/research/worldspace-transition-demo.md` section 3 lists what exists and what is missing in the
engine, with `path:line`; section 4 steps 5-8 are your recipe. In short: interior loading and
spawning exist but are never requested (`plan_cells` builds exterior keys only, from the immutable
`config.worldspace_id`); interiors are never unloaded; rebasing inside an interior would slide the
world; nothing handles doors.

The converter worker (`impl-005-converter-door-links.1`, running in parallel) adds the `door_links`
table and widens `statics` exactly as the design's "Database schema version 4" section says. Code
against that DDL. Your tests build their own fixture databases (see the existing fixtures in
`crates/engine/src/world/database.rs` tests), so you do not need the real converted data.

## What to build

1. **`ActiveCell`** resource in `streaming.rs` per the design. `plan_cells` uses it instead of
   `config.worldspace_id`. Initialise it from `EngineConfig` in `StreamingPlugin` if absent.
2. **Interiors:** while `ActiveCell.interior` is `Some`, request that interior (plus any pre-stream
   requests) and no exteriors. Rewrite `cell_within_unload_radius` so an interior unloads when it is
   neither active nor pre-streamed; update the existing test that encodes the old rule.
3. **Rebase guard:** no origin rebase while an interior is active.
4. **Doors in the database layer:** the reference load query (`world/database.rs`) also returns each
   reference's `door_links` row, the destination interior's `interior_name` and the destination
   worldspace's `editor_id` (LEFT JOINs, so databases without doors still work). A database that
   lacks the `door_links` table must still load, since the schema check is the lead's to change: detect
   the table's absence and treat every reference as a non-door. The schema-version constant itself
   comes from `shared` - do not edit `crates/shared`.
5. **Spawn `LoadDoor`** on the root entity of each reference with a resolved door link.
6. **`transition.rs`:** a `TransitionPlugin` that `StreamingPlugin` adds (register the `ActivateDoor`
   and `DoorCrossed` messages there). Pre-stream: each frame, any `LoadDoor` within 800 units of the
   `StreamingCamera` adds its destination (the interior cell, or the 3x3 exterior grid around the
   arrival point in the destination worldspace) to a resource `plan_cells` merges into `wanted`.
   Crossing: on `ActivateDoor`, set `ActiveCell`, set `RenderOrigin` for an exterior destination,
   move the camera to the arrival position and yaw (convert with the same functions the spawner uses
   for references in that kind of cell - interiors and exteriors are placed differently, so check
   `streaming.rs` rather than assuming), then write `DoorCrossed`. Record a profiler event if the
   `ProfilingState` API makes that a one-liner.
7. **No behaviour change** for acceptance, benchmark and fixture runs: no doors exist there and
   `ActiveCell` equals the config.

## Tests (each must fail before your change)

- `plan_cells` with an interior active wants exactly that interior; switching `ActiveCell` to
  another worldspace moves the wanted exterior keys there.
- An interior that stops being active is unloaded; `validate_streaming_lifecycle` reports no orphans.
- A rebase does not happen while inside; the camera-relative position of an interior reference at
  |x| > 4096 is unchanged.
- The database fixture with a `door_links` row returns the link and label; a fixture without the
  table still loads.
- A crossing, end to end in a Bevy `App` fixture: two cells and a door; move the camera next to the
  door; assert the destination is resident **before** `ActivateDoor` is sent; after it, the camera is
  at the arrival point, `ActiveCell` changed, `DoorCrossed` was sent, and there are no lifecycle
  failures.

## Crosstalk

`impl-007-player-controller.1` runs at the same time and writes `crates/engine/src/player.rs`,
which reads `LoadDoor` and sends `ActivateDoor`. Both of you build against `doors.rs` as it is and
neither edits it. Tell `impl-007-player-controller.1`, as soon as you know them: (a) where the camera
lands and its facing after a crossing (so it can reset its yaw/pitch state from `DoorCrossed`), and
(b) whether `LoadDoor`'s entity `GlobalTransform` is the door's real placement. If you need a contract
change, message it with the exact change, and put the proposal in your result instead of editing
`doors.rs`. Quote every message that changed your work in your result.

## Done when

The acceptance commands pass in your checkout. Report per the implementation template.
