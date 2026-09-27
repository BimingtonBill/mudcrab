# review-265.1 - Second opinion: one owner for the doorway plane

Track: portal. Read-only; change nothing.

## Goal
Check that a refactor kept behaviour identical. It was already reviewed once; you are the second reviewer, so look for what a first reviewer would miss.

## Context
The change (commit "impl-265", now in the tree):
- `crates/engine/src/transition.rs`: new `source_doorway_plane(door_position, door_rotation, door_scale, door, anchor) -> (Vec3, Quat)`. `door_map` now takes its `pivot` and `frame` from it, and takes only the arrival rotation from `doorway_frames`. `doorway_frames` and `source_doorway_frame` now call `heading_to_rotation(h)` (defined as `Quat::from_rotation_y(-h)`) where they used `Quat::from_rotation_y(-h)` directly.
- `crates/engine/src/player.rs`: new `doorway_walk_plane(...)`; `player_walks_through_doors` uses it instead of calling `source_doorway_frame` and `source_doorway_centre` itself. Without an anchor the walk plane still stands on `trigger.centre()`.
- `crates/engine/src/config.rs`: `EngineConfig::from_args` is now `#[cfg(test)]`.
- New test `the_walk_through_plane_is_the_maps_source_side`; the test `heading_to_rotation_agrees_with_every_former_call_shape` gained doorway-frame cases.

Before the change, `door_map` set `pivot = door_position`, `frame = door_frame(door_rotation, door.outward)`, then with an anchor overwrote `pivot` with `source_doorway_centre(...)` and `frame` with the first element of `doorway_frames(...)`.

## Done when
For each `DoorwayFacings` case (`Known`, `SameModel`, `Kept`) and for no anchor, you have compared the old and new `door_map` pivot and frame, and the old and new walk plane, and said whether each is identical. Also check: any non-test caller of `from_args` left anywhere in the workspace; any other call site that still hand-rolls a Creation heading turn in these two files.

## Report
Start with a verdict line (identical / differs). Then one line per case: case - old - new - same?. Then any finding as `path:line - issue`.
