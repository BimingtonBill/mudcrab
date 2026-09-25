# impl-182 - The full Riverwood tour fails at the Trader's door: E never opens it

Owned files: crates/engine/src/demo_tour.rs,crates/engine/src/player.rs,crates/engine/src/door_animation.rs,crates/engine/src/transition.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Found by impl-180 (2026-09-25): the whole Riverwood route (`--demo riverwood --walk --demo-tour <dir>` with no `--tour-doors`, 8 doors) now fails at the Riverwood Trader's door `0001341F`: "FAIL stage N: door 0001341F never opened in the 10 s the walk-through waited at it (state Closed, pressed_E=true)". The 2-door short tour (Sven's House) passes, so the standing check did not see it.

Today's changes that could be responsible (all merged on portal):
- impl-176 (`player.rs`, `door_animation.rs`, `transition.rs`): `E` targets settled doors; a closed load door's opening is **held** in `PendingDoorOpens` until `destination_is_loaded` (every key of `destination_keys` resident at Full, or failed for good); a second `E` cancels the hold.
- impl-178 (`demo_tour.rs`): the walk-through presses `E` **once**, from the first standoff where the player's own targeting reaches the door (range `DOOR_RANGE` 250, cone `DOOR_CONE_DEGREES` 45 from the eye, `player::target_door`), usually 160 units, and then waits up to `WALK_OPEN_SECONDS` (10 s) for `Open`.
- impl-179 (`door_animation.rs`, `transition.rs`): auto-close beyond 600 units; `plan_door_prestream` holds the far side of every Open/Closing door.

## What to do

1. Reproduce: `cargo build --profile quick --bin engine`, then `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t182`. Confirm the failure and at which stage.
2. Find why the press does not open the door. Candidates, test each with logs or a run: the press was refused by `player_door` (the target was a different door, or out of cone/range at that door's geometry - the Trader's doorway may sit differently from Sven's); the press was held (`PendingDoorOpens`) because `destination_is_loaded` never became true (which key is missing, and is it resident / failed / never requested?); the hold was cancelled by a second press; auto-close or `run_activation` undid it. Say which, with the run's evidence.
3. Fix the real cause. If it is the tour's press logic, fix the tour; if it is a door or streaming rule that a player would hit too, fix that (it matters more). Keep every earlier behaviour: E closes an open door; an opening waits for the far side; the short tour and the Alftand tours pass.

## Checks

- A unit test that fails before your fix, where one is possible.
- The full Riverwood tour (`local/t182-after`) must end `tour PASSED after 8 crossings` (or say exactly what else fails); the short tour `--tour-doors 2` and the Alftand short tour must still pass. Read only `local/t182*` under `local/`.

## Report

The failing stage and cause with evidence; the fix; test names; the three tours' last lines.
