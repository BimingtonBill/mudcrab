# impl-265 - audit leftovers: one heading helper, one doorway geometry owner, no production panic

**Track:** portal. **Status:** complete, tests green, short walked tour passes. Not committed (the
lead integrates).

Three open audit findings, each in its own owned file. No behaviour change is intended anywhere: the
change is that production code stops carrying its own copy of a convention or of a doorway frame.

## 1. CC-20260926-01 - the doorway frames go through `heading_to_rotation`

`crates/engine/src/transition.rs`

| Site | Change |
| :--- | :--- |
| `doorway_frames` (now `:905`) | the `Known` arm's two headings and the `SameModel` arm's `turn` now go through `heading_to_rotation` instead of three hand-rolled `Quat::from_rotation_y(-x)` |
| `source_doorway_frame` (now `:957`) | its `Known` arm likewise |

Line numbers are the ones in the patched file. Both functions are the ones the finding named as
`:885-892` and `:933`.

The guard test `heading_to_rotation_agrees_with_every_former_call_shape`
(`crates/engine/src/transition.rs:3358`) now also covers these call shapes: for each heading in the
loop it builds a `DoorAnchor` whose `DoorwayFacings::Known` the test sets to that heading and to a
quarter turn off it, then checks

* `doorway_frames(...).0` faces the source heading,
* `doorway_frames(...).1` faces the destination heading,
* `source_doorway_frame(..., Some(&anchor))` faces the source heading,

all measured as the direction each rotation faces (so `q` and `-q` agree, as the test's existing
comment explains).

**Honest limit:** this is a guard against future divergence, not a test that fails on the pre-fix
code. `Quat::from_rotation_y(-x)` and `heading_to_rotation(x)` are the same value by construction, so
a revert cannot be observed from the outside; what the assertions pin is that both functions keep
turning a Creation heading the one way `heading_to_rotation` does. The evidence that this particular
refactor changed no behaviour is that every existing transition test passes unchanged (below), and
above all that the walk-through tour still passes.

## 2. DCP-20260926-01 - the walk-through plane takes its frame and centre from the map's source side

`crates/engine/src/transition.rs`, `crates/engine/src/player.rs`

* New `transition::source_doorway_plane(door_position, door_rotation, door_scale, door, anchor) ->
  (Vec3, Quat)` (`transition.rs:880`): the point a door's doorway plane stands on and the frame it is
  measured in. It is now the **one** place that answers that question.
* `door_map` (`transition.rs:838`) builds its `pivot` and `frame` from it, and takes only the
  destination's frame (`.1`) from `doorway_frames`, with a comment saying why.
* New `player::doorway_walk_plane(...)` (`player.rs:1371`), which the walk-through system calls
  (`player.rs:1497`): its frame, and its centre under a `DoorAnchor`, come from
  `source_doorway_plane`; a door **without** an anchor keeps `trigger.centre()` - the centre of the
  volume the door's own model measures (`auto_door_trigger`), which is not the map's pivot on such a
  door and is where that plane has always stood. `player.rs` no longer imports
  `source_doorway_centre`/`source_doorway_frame` at all.

The brief allowed "call `door_map`, or the smallest shared function it and `door_map` both use". I
took the shared function: `door_map` also needs the destination side and a `RenderOrigin` for it,
neither of which the walk plane uses, and the walk plane's `origin`-free inputs are exactly the
anchor and the door row. The one owner for the source side is `source_doorway_plane`.

New test `the_walk_through_plane_is_the_maps_source_side` (`transition.rs:2379`) runs the Sven's
House door with an anchor and without, and for each compares `door_map` with `doorway_walk_plane`:
the frames must agree (1e-6), an anchored centre must equal the map's `pivot`, an unanchored centre
must equal the trigger's. The unanchored case also asserts the two points are more than a unit apart,
so the comparison cannot pass vacuously; the trigger is built from hand-made `InstanceBounds` whose
box centre sits 80 units off the reference.

**The test does fail on a broken implementation** - I checked both halves by mutating `door_map` and
running it:

* frame taken from `door_frame(...)` for an anchored door: fails, "different frames:
  `Quat(0.0, -0.26005608, 0.0, -0.9655935)` against `Quat(0.0, 0.1608245, 0.0, 0.986983)`" (about
  24 degrees apart);
* pivot taken from the reference origin for an anchored door: fails, "an anchored walk plane stands
  at `Vec3(20676.025, -121.862, 46406.297)`, the map's pivot at `Vec3(20669.508, -122.135,
  46394.473)`".

Both mutations were reverted before the final test run.

## 3. config.rs - `from_args` is test-only

`crates/engine/src/config.rs:154`. `EngineConfig::from_args` (`:155`) is now `#[cfg(test)]`, so no
production code path in the module can panic on a flag value; `from_env` (`:138`) is the run's entry
and still exits 2 with the message `try_from_args` returns. The doc comments on both were rewritten
to say so.

I chose `#[cfg(test)]` over removing the method because six files call it - `config.rs`, `app.rs`,
`graphics_settings.rs`, `pose_capture.rs`, `render.rs` - and only `config.rs` is mine; `#[cfg(test)]`
keeps every call site compiling and meaning exactly what it did. Each call site was checked to be
inside a `#[cfg(test)] mod tests` (the earliest marker in those files is `app.rs:1934`,
`render.rs:1214`, `graphics_settings.rs:1316`, `pose_capture.rs:1269`; the first call site is
`app.rs:2187`). The `#[should_panic(expected = ...)]` test
`from_args_stops_the_run_on_a_bad_reflection_value` (`config.rs:1574`) is unchanged and still passes,
which is the point of keeping the method.

## Checks run

| Command | Result |
| :--- | :--- |
| `cargo test -p engine --lib` | 681 passed, 0 failed, 3 ignored |
| `cargo test -p engine --lib -- the_walk_through_plane_is_the_maps_source_side heading_to_rotation_agrees_with_every_former_call_shape` | 2 passed (both named tests really ran; 682 filtered out) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean (only the pre-existing `proc-macro-error2` future-incompat note) |
| `cargo fmt --all -- --check` | clean |
| `cargo build --profile quick --bin engine` | `target/quick/engine.exe`, `engine.d` names this worktree |
| `<quick>\engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour <worktree>\target\impl-265-tour --tour-doors 2` | last line: `tour PASSED after 2 crossings (short tour)` |

The tour log (`target/impl-265-tour/tour.txt`, not committed) also has both walk-throughs crossing
on their own frame (100 and 175) with every photographed frame kept, and one standing note that stage
0 did not settle in 10 s and was photographed anyway - the same pre-existing streaming behaviour the
other short tours show, not a crossing failure.

## Not done / open

* `CARGO_BUILD_JOBS=6` was **not** set: this harness refuses env-var prefixes on commands, so the
  build ran with the default job count. Nothing else about the build differed.
* Two other hand-rolled heading turns remain outside this brief's scope and are not touched:
  `crates/engine/src/app.rs:1053` (a hardcoded demo yaw, not a Creation heading conversion) and
  `crates/engine/src/portal.rs:6993` (test code; `portal.rs` is out of scope for this task).
* The test-module expectations in `transition.rs` (`:1897`, `:2248`, `:2255`, `:2330`, `:2363`) still
  write `Quat::from_rotation_y(-x)` by hand on purpose: an expectation that is an independent
  expression of the convention is what makes those tests able to catch a wrong turn.
* `local/agents/memory/findings.jsonl`, where the brief says the evidence for each id lives, is
  outside this worktree and blocked by the session's read permissions; the work is from the brief's
  own description of the findings.
