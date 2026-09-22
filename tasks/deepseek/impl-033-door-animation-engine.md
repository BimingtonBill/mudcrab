# impl-033 - Engine: door animation state machine (Closed / Opening / Open / Closing)

Owned files: crates/engine/src/door_animation.rs,crates/engine/src/doors.rs,crates/engine/src/lib.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- the design `docs/design/animated-doors-and-seamless-crossing.md`: sections 2.3, 3, 4.6, 4.7 and
  5, and task 3 in section 6.

## Lead's decisions that change the design

- **No database column** (the design's task 2 is dropped). The engine loads each door model's
  `Gltf` asset, and `Gltf::named_animations` says whether an `Open` (and a `Close`) clip exists.
  Look it up case-insensitively, falling back to the first clip for `Open`, as in the design's
  section 5. Find how `streaming.rs` loads a reference's model (the scene label) and load the same
  path's root `Gltf` handle for **load doors only**. Do it from your own module by reading the
  model path the door reference already has. Do not edit `streaming.rs`: if you need something
  from it, it must be public already; otherwise describe the hook in your report.
- Clips come from impl-032's converter change (`Open` and `Close` glTF animations on the named
  nodes). Until the assets are reconverted there are no clips, so every door behaves as a static
  door: it is promoted to `Open` in the same frame, which is today's behaviour. That fallback must
  work.

## What to build

- A `DoorAnimationPlugin` with the section 3 state machine per load door:
  - build an `AnimationGraph` from the door's clips, and insert the `AnimationGraphHandle` next to
    the `AnimationPlayer` the glTF loader creates (design section 2.3: the loader does not attach a
    graph);
  - `ActivateDoor` from the player's E starts `Opening`. Keep the `ActivateDoor` message contract
    as it is: impl-034 changes what the crossing does, not you;
  - `Open` once the clip reaches `OPEN_FRACTION = 0.5` (it keeps playing to the end and holds);
  - E again on an `Open` door plays `Close`;
  - auto-load doors never animate.
- A public `DoorState` component on each load door, which the portal and the crossing (impl-034)
  will read. The leaf is hidden while the door is `Opening` or `Open`. This is a property of the
  door state, not of the portal (design section 4.7, row 1). Coordinate with `portal.rs`'s
  `show_load_door_leaves` through the component: do not edit `portal.rs`. In your report, write
  exactly how portal.rs should read `DoorState`, and the lead will wire it.
- `DoorLeaf` marks the animated subtree (the entities with `AnimationTargetId`) so the walk probe
  can skip it while the door is open (design section 4.6). The probe change itself is in
  `player.rs`, which you do not own: describe it.
- `doors.rs`: add what the contract needs (`DoorState`, `DoorLeaf`, and anything else), with doc
  comments.

## Tests (write them to fail before your change)

These follow the design's task 3:
- activate → `Opening`;
- advance time → `Open` at `OPEN_FRACTION`;
- the clip finished → still `Open`;
- a static door (no clip) reaches `Open` in one frame;
- a re-activation starts `Close`;
- `DoorLeaf` is set on `AnimationTargetId` entities only;
- an auto-load door never changes state.

Use a hand-built `AnimationClip`, `AnimationGraph` and `AnimationPlayer` with no GPU.

## Check it

Run `cargo build --release -p engine` and `--demo-tour` (`target/release/engine.exe --assets C:/Modding/SkyrimConverted --demo alftand --demo-tour local/tour`).
It must still pass 4/4: the current assets have no clips, so every door takes the static path.

## Scope

Only the owned files. No crosstalk partner. Report per the implementation template, including
the exact wiring the lead must add in `portal.rs` and `player.rs`.
