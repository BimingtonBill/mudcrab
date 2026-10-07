# impl-150-portal-shots-open-door - A reference shot that looks through an open doorway

## Why

`--shots <file>` renders exact camera poses to PNGs once streaming settles
(`crates/engine/src/shots.rs`; the contract is `docs/design/reference-shots.md`). It is how this
project checks rendering against reference screenshots without a person at the keyboard.

It has one blind spot, and a real bug lived in it. **A shot cannot open a door**, and the portal -
the view *through* a load door into the next space - only exists while a door is open. So nothing
automated has ever rendered a view through a doorway from a chosen angle. The user found the bug by
hand: standing outside Sven's House in Riverwood with its door open, the whole interior seen through
the doorway is at the wrong angle (`local/demo/user-portal-angle.png`). Every existing test and the
scripted tour missed it, because the tour only ever looks through a doorway level, square to the
door, from inside the doorway plane - the one place a wrong mapping cannot show.

A separate lead (`lead-102`) is measuring the misalignment across the world's 2,204 door links and
will pick check sites; a fix follows. **This task builds the instrument those checks need**, and the
regression test the bug class never had.

## What to build

An optional field on a shot, in the shots file's JSON:

```json
{ "name": "sven-doorway-oblique", "worldspace_id": 60, "position": [...], "yaw": 40, "pitch": 5,
  "hfov": 75, "open_door": "0x0001CBB0" }
```

When a shot names `open_door` (a door reference FormID, hex string like the rest of the
project's FormIDs - check `docs/design/reference-shots.md` for how the existing fields are written
and match it), the shots run must, before capturing that shot:

1. place the camera at the pose, as today;
2. open that door through the engine's own door machinery - **do not invent a second way to open a
   door.** `crates/engine/src/demo_tour.rs` opens doors (`ActivateDoor`, and the walk-through
   presses `E`), and `crates/engine/src/door_animation.rs` / `transition.rs` own the door state
   (`DoorState`, `OpenDoor`, `CrossDoor`). Find the message or state a player's `E` produces and
   produce exactly that. **It must open the door, not cross it** - a shot is taken from outside;
3. wait until the portal is actually rendering through that door: the door's state is open, its
   destination has streamed in, and the portal camera has produced its image. Read `portal.rs`
   (`PortalState::open_door` and whatever marks the portal target as drawn) and wait on the real
   condition, not a timer. Keep a timeout so a door that never opens fails the shot loudly - say so
   in `shots.log` with the reason - instead of hanging the run;
4. let the door's opening animation finish, so the leaf is where a player would see it;
5. capture, then close or leave the door for the next shot - say which and why.

A shot with no `open_door` must behave **exactly** as it does today: same settle, same frames, same
bytes. Existing shots files are relied on by comparisons against earlier renders.

## Verify it by rendering - you can

Coding workers may run the release engine (`target/release/engine.exe`, in `implAllowTools`).
Build it and render at least:

- **Sven's House, through its open door, from outside:** door `0x0001CBB0`. Its position is in
  `door_links` / `references` in `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db` (worldspace 60), and
  `docs/design/riverwood-demo.md` section 2 has the door and its arrival. Take one pose square to the
  door and one at an oblique angle (30-45 degrees off its axis). **The render must reproduce the
  user's defect** - the interior drawn at the wrong angle - because that is what proves the
  instrument can see this bug. If it looks correct, say so plainly and show the frame: either the
  instrument is not doing what it should, or the defect is not what we think.
- **One more doorway** anywhere else, square and oblique.

Put the shots file you used in `tools/reference/doorway_shots.json` and the renders under `local/`
(git-ignored). Report the paths.

Engine rules (`AGENTS.md`): engines may run beside other engines, but **none may hold a file inside
a folder a conversion is publishing into** - if `C:\Modding\Mudcrab\SkyrimConverted.staging-*` exists or a
`converter.exe` is running against `SkyrimConverted`, wait. Timing runs are not yours to do.

## Owned files

- `crates/engine/src/shots.rs`
- `docs/design/reference-shots.md` - document the new field in the contract
- `tools/reference/doorway_shots.json` - new
- `docs/handoff/impl-150-portal-shots-open-door.md` - your notes

If opening a door from a shot genuinely needs a change in `portal.rs`, `door_animation.rs`,
`transition.rs` or `doors.rs`, **do not make it** - describe the exact change in your report and
Claude will make it. Those files belong to work that is about to change them.

## Tests to write

CI has no GPU and no game data, so test the logic:

- the new field parses from JSON, is optional, and a shots file without it parses to exactly what
  it parsed to before (compare against the existing parse);
- a malformed `open_door` is rejected at load with a clear error, like the other fields;
- whatever state machine sequences open -> wait -> settle -> capture: a door that opens proceeds,
  a door that never opens times out and fails that shot, and a shot with no door skips it entirely.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
```

## Report

What you built with `path:line`, the exact JSON field, the render paths and **what they show** -
in particular whether the Sven's House oblique render reproduces the defect.
