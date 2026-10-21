# research-158 - Checks that would have caught what only the user caught

## Goal

Every one of these defects passed all automated checks and was found by the user playing or by the
lead looking at pictures (2026-09-23/24). For each, say what automated check would have caught it,
how cheaply, and where it would live:

1. **Load doors never swung**: pressing E hid the door instead of animating it, on every animated
   door, since the animated-door work landed. Cause: the door's scene was re-instanced after its
   animation attached, so its recorded `AnimationPlayer` was gone and `activate_doors` fell back
   to `DoorState::Open { animated: false }` (`crates/engine/src/door_animation.rs`). The tours
   passed because a hidden door still lets the player walk through.
2. **The doorway image lagged the camera by one frame**: `portal.rs` read the main camera's
   `GlobalTransform` in `Update` before propagation.
3. **The wrong part of the world streamed behind exterior doors**: a door reference in the
   persistent cell has grid (0, 0); the anchor used the cell's grid (doorway shown as plain sky).
4. **A crossing landed the player inside the closed far door** (the tour's walk test did catch the
   effect, but only at the very end, and not which crossing).
5. **Water renders as a flat white sheet; interiors lit by fire are too dark** (visual; the reference
   sheets show it but nothing measures it).

Then propose the **3-5 highest-value new checks** overall (for example: the tour asserting each
door's state goes Closed -> Opening -> Open { animated: true }; a same-frame pose assertion between
the main and portal cameras; per-crossing assertions at the swap frame; image metrics on the
reference sheets), each with its cost in run time, where it plugs in (unit test, tour assertion,
shots metric), and a sketch of the code. Read the code to make the sketches concrete (file:line).

## Context

Rust workspace, Bevy 0.19 engine (`crates/engine`), converter (`crates/converter`), shared contracts.
Read `AGENTS.md` ("While working", the parallel-engine rules) and `docs/demo/README.md` first.
Automated testing today:
- `cargo test -p engine --lib` (~344 unit tests), plus converter/shared tests; CI runs
  `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, tests.
- Demo tours: `target/release/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --walk --demo-tour <dir>`
  (8 door crossings, then a walk test) and `--demo alftand` (4 crossings). About 5 min each, run
  one after another after a ~4 min release build. `crates/engine/src/demo_tour.rs`.
- Reference shots: `--shots tools/reference/*.json --shots-out <dir>` and `tools/compare_shots.py`
  (`crates/engine/src/shots.rs`, `docs/design/reference-shots.md`); door shots with `open_door`.
- Python tests under `tests/` (`python -m unittest discover -s tests`).

You may run `cargo test`, `cargo check`, `cargo build --release -p engine` and the release engine
(assets are read-only; write output only under `local/<your task name>/`). Do not edit tracked files;
this is research - your report is the deliverable. Keep to about 80 tool calls.
