# impl-179 - A door left open closes by itself, and its far side stays loaded until it has shut

Owned files: crates/engine/src/door_animation.rs,crates/engine/src/transition.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user's demo note of 2026-09-25 (`docs/roadmap/backlog-engine-and-portal.md`, Portal plugin > Correctness, note 7): "doors should automatically do the close animation if the player opens it and walks away from it. It should close before the interior is unloaded, so the player doesn't see." Also performance item 5 in `docs/design/portal-performance.md` (read its auto-close part). research-174 traced it (quoted below; the lead checked the citations). impl-176 (merged just before this task) made `E` close an open door through the existing state machine and added `destination_is_loaded` and the `PendingDoorOpens` hold; build on those.

## Now

- Closing happens only through `activate_doors` / `run_activation` in `door_animation.rs`, on an `OpenDoor` message for an `Open` door. There is no auto-close system.
- The far side unloads by distance alone: `plan_door_prestream` (`transition.rs` ~236-266) rebuilds `PrestreamCells` every frame from the load doors within `DOOR_PRESTREAM_RADIUS = 800` of the camera, **ignoring the door's state**; `streaming::cell_within_unload_radius` keeps a cell only if it is pre-streamed, the active interior, or an exterior within radius. So an open door's far side is dropped the first frame the camera is more than 800 units away, and the portal stops drawing through it (the doorway suddenly empties). The door's **own** cell can also unload, despawning the door mid-swing.

## What to build

1. **Auto-close** (a new system in `door_animation.rs`, in the same `Update` chain as `activate_doors`, ordered like `open_arrival_doors`): a door in `Open { .. }` that the camera is more than `AUTO_CLOSE_DISTANCE` from (a new constant, a doc comment saying why its value; start at 600 units, i.e. well inside the 800-unit pre-stream radius, so the close finishes while the far side is still loaded) starts closing through the same path `E` uses (write `OpenDoor`, or call the shared activation, whichever is cleaner; say which). Do not auto-close a door whose `Close` clip is missing (`DoorAnimation::can_close` from impl-176), a door whose far side is not loaded, or a door the player stands in (reuse whatever impl-176 used).
   Include the far door the crossing opened at the arrival (`open_arrival_doors`): walking away from it closes it too.
2. **Hold the far side while closing:** make `plan_door_prestream` also request the destination of every door in `Closing`, and of every `Open` door (any distance), **regardless of the 800-unit radius**, dropping it only when the door reaches `Closed`. `cell_within_unload_radius` already keeps any pre-streamed key alive, so no streaming change is needed; `streaming.rs` is not yours - if you find you need a change there, stop and say so.
   Say in the report whether the door's **own** cell can still unload under a closing door (for an exterior door, its exterior cell is kept by the exterior radius; for an interior door the player just left, check) and, if it can, keep it with the same mechanism.
3. Keep the mirror/portal code in `portal.rs` untouched (not yours): when the door leaves `Open`, the portal already stops drawing through it the same frame.

## Checks

- Unit tests in the style of the existing ones: an open door beyond `AUTO_CLOSE_DISTANCE` starts `Closing`; one inside it stays open; a closing door's destination stays in `PrestreamCells` beyond 800 units and leaves it once `Closed`; a door with no Close clip is never auto-closed.
- `cargo build --profile quick --bin engine`, then the short tour: `target/quick/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --walk --demo-tour local/t179 --tour-doors 2` must end `tour PASSED after 2 crossings (short tour)`. Read only `local/t179` under `local/`. (The tour does not walk away from doors, so it only proves nothing broke; say so.)

## Report

Files changed and why; the distance chosen; how the close is triggered; how the far side (and the door's own cell) are held; test names; the tour's last line.
