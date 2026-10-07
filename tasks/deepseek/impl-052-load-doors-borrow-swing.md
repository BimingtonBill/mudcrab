# impl-052 - Load doors swing like their non-load twin

Owned files: crates/engine/src/door_animation.rs,crates/engine/src/doors.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`, and
`docs/design/animated-doors-and-seamless-crossing.md` (sections 1.4 and 3).

## Why

In Skyrim a load door barely moves before the loading screen hides it: the demo route's
`DweDoorLarge01Load` turns its two leaves 5.4 and 8.7 degrees. Its non-load twin
(`dwemerlargedoor01.nif`) swings 86-88 degrees. The engine has no loading screen - the player
walks straight through - so the user wants **every load door to open like a real door**.

## What to build

When a load door's own `Open` clip does not clear the doorway (`DoorAnimation::clears_doorway`
is false, the existing 45-degree rule), look for the door's **non-load twin model** and use its
`Open`/`Close` clips instead, on the same node names.

- The twin's model path is the door's own with the "load" marker removed, for example
  `dungeons/dwemer/door/dwemerlargedoorload01.nif` → `dwemerlargedoor01.nif`. **Derive the rule
  from the data, not from one example:** query `statics` in
  `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db` (read-only, a script under `tools/research/`) for
  every load door's `model_path`, work out which ones have a plausible twin, and report the
  coverage: how many load doors get a swing this way, how many do not, and the patterns you
  found (`...Load01`, `...LoadDoor`, and so on).
- The clips must target node names that exist in the door's own model. Check that on the real
  data where you can, and skip the twin when its animated node names do not match; then the door
  keeps its own clip. Report how many pass and fail that check.
- Keep everything else: the state machine, `clears_doorway` (recomputed from whichever clip is
  used), the leaf-hiding rule, reversals, auto-load markers (no model, unchanged).
- Log once per model when a twin's clips are borrowed.

## Verification, and an important limitation

The published assets **have no animation clips at all** until the pending reconversion, so this
cannot be checked at runtime yet. Therefore:
- prove the behaviour with unit tests on hand-built clips and on a fake model catalogue: a door
  whose own clip is narrow borrows the twin's; one whose own clip clears the doorway does not; a
  missing twin, or a twin with different node names, falls back to the door's own clip;
- prove the path rule with a table test over the real model paths you dumped;
- run `--demo-tour` (both modes) to show nothing regressed: with no clips anywhere, every door
  still takes the static path and the tour passes 4/4.

## Report

Per the implementation template, including the coverage table (how many of the game's load doors
get a real swing) and the exact path rule.
