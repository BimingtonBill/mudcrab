# impl-009 - See into the next room: hide load doors, render the destination through the doorway

Owned files: crates/engine/src/portal.rs,crates/engine/src/shaders/portal.wgsl
Acceptance: cargo test -p engine --lib portal,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`, the demo design
`docs/design/blackreach-demo.md`, and the door contract `crates/engine/src/doors.rs` first. Your
checkout builds into its own `target/`; the first build takes several minutes.

## Goal

The user walks from the Alftand entrance down to Blackreach through four load doors (`--demo alftand
--walk`). They asked: "either don't render [doors] or render them open, I want to be able to see into
the next interior". Deliver three things in `portal.rs`:

1. **Isolate pre-streamed cells (a bug fix, do this first).** When the camera is within 800 units
   of a `LoadDoor`, `crates/engine/src/transition.rs` pre-streams the door's destination (an
   interior cell, or a 3x3 exterior grid of another worldspace). Those cells are spawned into the
   same render space as the active cell - interiors at their absolute creation coordinates
   (`transition.rs` `apply_door_crossings` comment: "an interior root sits at the render origin"),
   exteriors relative to the current `RenderOrigin` - so today their geometry is drawn **overlapping
   the place the player stands in**, as floating, misplaced objects. Verify this (read
   `streaming.rs` `wanted_cells` / `PrestreamCells` / `spawn_cell` and `transition.rs`), then make
   every entity of a cell that is **not part of the active space** (`streaming::ActiveCell`: the active
   interior, or exteriors of the active worldspace) invisible to the main camera, and visible again
   the moment it becomes active (after a crossing). Use `RenderLayers` (note: they are not inherited;
   every mesh entity needs them - cells spawn their meshes over several frames as assets load, so
   re-apply on newly added descendants) or another mechanism you justify. The main camera currently
   renders layers 0 and 1 (`crates/engine/src/app.rs`, `RenderLayers::from_layers(&[0, 1])`; layer 1
   is the water reflection).
2. **Hide load doors.** Every reference root with a `doors::LoadDoor` gets its meshes hidden, so the
   doorway is open. (Many route doors are already invisible `AutoLoadDoor01` markers; the Alftand
   interior doors are `DweDoorLarge01Load` meshes.)
3. **A portal in each nearby door.** For the nearest `LoadDoor` within the pre-stream radius whose
   destination is resident, render the destination through the doorway: a second camera placed in the
   destination space at the pose that corresponds to the player's pose relative to the source door,
   rendering only the destination cells' layer into a texture, shown on a quad in the doorway with a
   screen-space-UV material (`shaders/portal.wgsl`) so it lines up exactly with the main view. The
   correspondence is: the source door's frame maps to the `XTEL` arrival frame
   (`LoadDoor.destination.arrival_position`/`arrival_rotation`, converted exactly as
   `transition.rs::apply_door_crossings` converts them for the camera), so walking through the quad and
   pressing E must land the player where the portal showed. Size the quad from the door reference's
   bounds (`InstanceBounds` / `ExpectedModelBounds` in `crates/engine/src/world/components.rs`), with a
   sensible default (about 200 wide x 300 tall) for bounds-less markers. Avoid the portal camera
   drawing destination geometry that sits between it and the doorway (an oblique near plane, or a near
   plane at the doorway distance - justify your choice). One portal at a time is enough.

Register the shader inside `portal.rs` (`embedded_asset!` like `crates/engine/src/render.rs:34`
does for its shaders - do not edit `render.rs`). Expose `pub struct PortalPlugin`; the lead adds it
to the app for interactive runs and wires `lib.rs`/`app.rs`. Read the real Bevy 0.19 APIs in the
cargo registry (render targets, `Camera { target: RenderTarget::Image(..) }`, `RenderLayers`,
`ExtendedMaterial`/`Material`), and the existing water reflection camera in `render.rs` as the
in-repo precedent for a second camera rendering to a texture.

## Tests (logic, in `portal.rs`, runnable without a GPU)

- The pose mapping: a camera 150 units in front of a source door, looking at it, maps to a point 150
  units behind the arrival point looking along the arrival facing (or state and test the convention
  you derive - but it must agree with where `apply_door_crossings` puts the camera).
- Layer isolation: in a Bevy `App` fixture with an active cell and a pre-streamed cell, the
  pre-streamed cell's mesh entities (including one added a frame later) are not on the main camera's
  layers, and flipping `ActiveCell` makes them visible.
- Door hiding: a `LoadDoor` root's mesh descendants end up hidden.

## Scope and crosstalk

Only the two owned files. `crates/engine/src/streaming.rs`, `render.rs` and `shaders/terrain.wgsl`
belong to `impl-010-terrain-blending.1`, running at the same time; you need nothing from it and must
not edit them. If isolation is impossible without a hook in `streaming.rs` (e.g. a marker component
on cell roots carrying their `CellKey`), message `impl-010-terrain-blending.1` with the exact
3-line change you need, put it in your result, and code against a clearly marked fallback meanwhile.
Quote any message that changed your work.

## Done when

Acceptance commands pass. In your result, say exactly what the lead must add to `app.rs` (plugin,
camera changes), and list what you could not verify without a GPU.
