# impl-048 - Wire the door animation state machine into the seamless crossing

Owned files: crates/engine/src/door_animation.rs,crates/engine/src/doors.rs,crates/engine/src/portal.rs,crates/engine/src/player.rs,crates/engine/src/transition.rs,crates/engine/src/app.rs,crates/engine/src/world/database.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- `docs/design/animated-doors-and-seamless-crossing.md`;
- the two landed pieces: commit `50bc0f1` (`door_animation.rs`: `DoorAnimationPlugin`,
  `DoorState` with `is_open()` / `hides_whole_reference()`, `DoorLeaf`,
  `mesh_is_out_of_the_way`) and commit `49f2df5` (seamless crossing: `OpenDoor` from E, the
  temporary `DoorOpen` component, `door_is_open`, the plane trigger, `CrossDoor`, `ActivateDoor`
  = snap for the tour).

## Goal

Make them one feature. When the player presses E, the door plays its Open clip and becomes
walkable once it is open. The player walks through, and the crossing is seamless.

## Wiring (decided by the lead)

1. `app.rs`: register `DoorAnimationPlugin` inside the existing `if interactive` block, next to
   `PortalPlugin`.
2. **One source of truth for "open".** `door_animation`'s `activate_doors` must react to
   `OpenDoor` (E), not to `ActivateDoor`, which is now the tour's snap. Reacting to both is fine,
   provided a snap does not start an animation that fights the crossing.
   - Retire the temporary `DoorOpen` component: `door_is_open` (player/transition) becomes
     `DoorState::is_open()` (`Opening | Open`). A door with no `DoorState` counts as closed.
   - Auto-load markers stay `Open` from birth.
   - Keep one small function for the question, and keep every existing test green. Rewrite the
     ones that used `DoorOpen`.
3. `portal.rs` `show_load_door_leaves`:
   - OR in `door_state.is_some_and(DoorState::hides_whole_reference)`, which covers static doors;
   - qualify the existing `open_door == Some(door)` whole-reference hide so it does **not** apply
     to an animated door. An animated door's leaves are handled per leaf by
     `door_animation.rs`: hidden at `Open` when the clip does not clear the doorway, drawn
     otherwise;
   - the portal should render through a door only while it is open (`DoorState::is_open`).
     Otherwise a closed leaf would sit in front of the quad.
4. `player.rs` walk probe: add `crate::doors::mesh_is_out_of_the_way(...)` to the ray-cast skip
   closure, so a leaf mid-swing does not block.
5. **Review-035 corrections** (on the door-front change `23a95a2`):
   - (1, medium) The doorway quad uses the outward-derived frame for its offset and normal but the
     model frame for its rotation and size. Put it on one frame: the in-plane size from the model
     bounds, projected onto the outward frame. Add a test with `outward` at 90 and 270 degrees off
     the model axis.
   - (4) `world/database.rs` `return_links` rescans `door_links` for every cell load. Build it
     once, cached on the database.
   - (5) `doors.rs` `outward_from_return_link`: a return arrival farther than 512 units is not
     trusted as a point, so use the heading.
   - (6, 7) Fix the stale test message and the module doc.

## Check (you can build and run the engine)

- `cargo build --release -p engine`.
- `target/release/engine.exe --assets C:/Modding/SkyrimConverted --demo alftand --demo-tour local/tour`
  must pass 4/4.
- `... --demo alftand --walk --demo-tour local/tour-walk` must pass 4/4 by walking, with the ±10
  frames around each swap showing no doorway-sized jump. You can see images: look at them.
- Today's assets have no clips, so every door takes the static path: E, then open in one frame,
  then walk through.
- Also add one unit test with a hand-built clip, driving the full sequence: E → `Opening` → `Open`
  → the plane crossing fires → `CrossDoor`.

## Scope

Only the owned files. No crosstalk partner. Report per the implementation template.
