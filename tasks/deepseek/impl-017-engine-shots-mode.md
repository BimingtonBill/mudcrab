# impl-017 - Engine `--shots`: render exact camera poses to PNG files

Owned files: crates/engine/src/shots.rs,crates/engine/src/lib.rs,crates/engine/src/config.rs,crates/engine/src/app.rs,crates/engine/src/transition.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md` and the design
`docs/design/reference-shots.md` first. The design's shots-file format and engine behaviour are the
contract: keep the field names, units and sign conventions exactly.

## Goal

`engine.exe --assets <dir> --shots <file.json> [--shots-out <dir>]` renders each camera pose in the
file to `<out>/<name>.png`, waiting for each view to finish streaming, then exits. It is used to
render the same views as reference screenshots of the real game.

## What to build

1. **Config** (`config.rs`): `--shots <path>` and `--shots-out <path>`, with parse tests. The
   default output folder is described in the design.
2. **Shots module** (`shots.rs`, new, registered in `lib.rs`): parse the file with the workspace's
   existing JSON dependency (check `crates/engine/Cargo.toml`; do not add dependencies). Validate
   it: exactly one of `worldspace_id` / `interior_cell_id`, finite numbers, a pitch within
   -89..89, and an hfov within 10..170. A `ShotsPlugin` then runs the sequence described in the
   design.
   - **Camera rotation:** yaw uses the same clockwise heading as
     `transition::arrival_camera_rotation`; pitch positive looks down. Write it as a pure
     `fn shot_camera_rotation(yaw_deg, pitch_deg) -> Quat`, and test it. At yaw 0 / pitch 0 the
     forward is Creation `+Y`; at yaw 90 it is `+X`; with pitch 30 the forward has a negative
     Creation `Z` component of `sin 30`.
   - **FOV:** vertical fov = `2 * atan(tan(hfov/2) / aspect)`. Test it.
   - **Moving to the shot's space:** factor the space switch out of `apply_door_crossings`
     (`transition.rs`) into a `pub(crate)` function that both the crossing and the shots use.
     It sets `ActiveCell`, moves the origin for exteriors, re-places the cell roots, and returns
     the render-space camera translation for a Creation position. The existing transition tests
     must still pass unchanged, which proves the refactor kept behaviour. The shot camera sits at
     `position` exactly, without the eye-height offset a crossing adds.
   - **Settling:** do not just wait a fixed time. Find what the streaming code exposes about
     pending work (`StreamingWorld`, `StreamingMetrics`, the cell cache, scene spawning in
     `streaming.rs`: read them, do not edit them) and wait until the shot's space has no pending
     cell loads and no scenes still loading, then 10 more frames. Keep a 30 s timeout. Write the
     settle condition as a pure function over the counts, and test it.
   - **Exit** after the last screenshot has been **written to disk**, not merely requested:
     screenshots are asynchronous (see how `demo_tour.rs` uses `Screenshot` and `save_to_disk`,
     and wait for the file or an observer before exiting). Write `shots.log` as the design says.
3. **App wiring** (`app.rs`): a shots run is interactive in the sense of the existing
   `walk || demo_tour.is_some()` checks. It gets the portal and lights plugins, the atmosphere
   and the sky clear colour. Add the condition wherever those checks appear, preferably through
   one helper. It gets no `PlayerPlugin`, no `fly_camera`, no demo objective and no help text.
   The window resolution comes from the file's `width` and `height`: read the file early enough
   to set it when the window is created, and find where `WindowPlugin` is configured.

## Tests (write them to fail before your change)

- Parsing: a valid file; a shot with both or neither space fields; pitch 95; a missing `name`.
- `shot_camera_rotation` directions, as listed above (compare with a tolerance).
- The vertical FOV formula at 4:3 with hfov 75 (expect about 59.84 degrees).
- The settle predicate.
- The transition tests still pass unchanged.

## Manual check (do it if you can; say in the report if you could not)

The converted assets are at `C:/Modding/Mudcrab/SkyrimConverted` (read-only). The lead will run the real
render. If your tools allow it, a debug run of two shots (one Tamriel exterior near
(77583, 77411, -5700), yaw 45, and one in interior 86723 at (-947, 3958, 712), yaw 170) is worth
more than any unit test. Do not run the profiling or acceptance scripts.

## Scope

The owned files only. Do not edit `streaming.rs`, `portal.rs`, `demo_tour.rs` or `player.rs`. If
you need a hook in one of them, stop and describe it in the report instead. No crosstalk partner.
Report per the implementation template.
