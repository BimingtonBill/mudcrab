# impl-007 - Engine: a first-person player you can walk through Blackreach with

Owned files: crates/engine/src/player.rs
Acceptance: cargo test -p engine --lib player,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`, the design
`docs/design/blackreach-demo.md`, and the contract `crates/engine/src/doors.rs` first.

## Goal

A `PlayerPlugin` that turns the engine's camera into a first-person player: mouse look, walking on
the ground with gravity, not walking through walls, and pressing E to open the load door in front.
The user will play the Alftand -> Blackreach demo with it.

## The engine today

- The camera is the entity with `StreamingCamera` (`crates/engine/src/world/components.rs`). Its only
  controller is `fly_camera` in `crates/engine/src/app.rs:1366`: WASD/Space/Shift translate it, there
  is **no mouse look**. Read it for the input and profiler conventions.
- 1 engine unit = 1 Creation-engine unit (Skyrim: the player's eye is about 120 units up, walking is
  about 150 units/s, running about 350, a door is about 100 wide). Y is up in the engine.
- The world is streamed cell by cell; meshes appear some frames after a cell is requested. Terrain
  exists outdoors; interiors (Alftand's ruins) and much of Blackreach's floor are meshes, not terrain.
- The renderer is custom (`crates/engine/src/render.rs`, "Vercidium" batched/indirect drawing). Find
  out, from the code and the Bevy 0.19 source in the cargo registry, whether Bevy's mesh ray casting
  (`MeshRayCast` in `bevy_picking`) can hit this engine's spawned meshes. If it can, use it for ground
  and walls. If it cannot, fall back to the reference bounds the streamer already computes
  (`InstanceBounds` / `ExpectedModelBounds` in `components.rs`) and say so plainly in your result -
  box collision is coarse but walkable.

## What to build, in `player.rs` only

- `PlayerPlugin` (the lead adds it to the app for `--walk`; do not edit `app.rs`). A `Player`
  component with look (yaw, pitch), velocity, grounded flag and mode (`Walk` / `Fly`).
- Mouse look with the cursor grabbed on left click and released on Escape. Pitch clamped to +-89 degrees.
- Walk: WASD relative to yaw, Shift runs, Space jumps when grounded, gravity. Ground by a downward
  ray from the eye; step up small ledges (about 40 units); keep the eye 120 units above ground.
  Walls: stop horizontal motion into geometry (a few short rays at knee and chest height are
  enough). If there is no ground under the player (the cell has not streamed yet), hold position
  instead of falling forever.
- F toggles `Fly` (the old free-flight feel, with mouse look) - an escape hatch when collision
  misbehaves.
- E: the nearest `LoadDoor` within 250 units whose direction is within about 45 degrees of where the
  player looks gets an `ActivateDoor`. On `DoorCrossed`, zero the velocity and take the yaw from the
  camera's new rotation.
- A prompt on screen while a door is targeted: `E  Open  <label>` (Bevy UI text, bottom centre). And
  a small one-time help line: `WASD move · Shift run · Space jump · F fly · E open · Esc cursor`.
- Record your systems' time with `ProfilingState::record_elapsed` like `fly_camera` does.

## Tests (pure logic, in a `#[cfg(test)]` module in `player.rs`)

- Movement vector from input and yaw; pitch clamping.
- Door targeting: picks the nearest door in the cone, ignores doors behind or beyond range.
- Ground following: given a ground height function, the eye settles 120 units above it; a 30-unit
  step is climbed, a 200-unit wall is not.
- Keep the collision queries behind a small function or trait so these tests run without a GPU.

## Crosstalk

`impl-006-engine-door-streaming.1` runs at the same time and owns the streaming side: it spawns
`LoadDoor` components and performs crossings. It will tell you where the camera lands after a
crossing and whether a `LoadDoor` entity's transform is the real door placement - use what it
says, and quote its message in your result. If you need a contract change in `doors.rs`, message it
the exact change and put the proposal in your result; neither of you edits `doors.rs`. Before you
finish, if you have not heard from it, wait for its message with `wait_for_messages` for up to 10
minutes, then finish with what you have.

## Done when

The acceptance commands pass in your checkout, `player.rs` compiles as a module the lead can add
with `pub mod player;` in `lib.rs` (add a line at the top of your result saying exactly what to add
to `lib.rs` and `app.rs`), and your result says which collision approach you used and why.
