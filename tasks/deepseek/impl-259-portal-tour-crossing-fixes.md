# impl-259 - Demo tour: crossing through an opened door, missing-door wait, portal shots on every walked door

Owned files: crates/engine/src/demo_tour.rs,crates/engine/src/transition.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user approved this batch on 2026-09-27. Source: review-258, items 1, 3 and 4 (`local/research/review-258-portal-demo-2026-09-26.md`; read those sections).

## Goal
Three small fixes to the demo tour and the scripted crossing.

1. **Scripted crossing through a door the tour opened (review item 1).** Since impl-241 the scripted (non-walk) tour opens each door (`Phase::PortalOpen`, demo_tour.rs ~1486) before its scripted crossing (`ActivateDoor`, transition.rs ~1200-1204), whose comment says a scripted run "never opens the door it crosses" and so lands on the `XTEL` point. Now the portal draws the doorway-anchored view while the snap lands elsewhere. Make a scripted crossing through a door that is open (or opening) land the way a player's walked crossing does: through the doorway map (the anchored landing), so the frame after the swap matches the portal view. A scripted crossing through a closed door keeps the `XTEL` snap. Update the comment, and keep or update the tests that pin the XTEL landing so they still pin it for the closed case.
2. **A door not found is not a door that failed to open (item 3).** In `Phase::PortalOpen`, `state == None` (door entity not in the query yet) currently takes the "did not open within 10 s" arm at t=0. Give it its own arm: wait up to `WALK_OPEN_SECONDS` for the door to appear, with a distinct note if it never does.
3. **Walked tours photograph every door's portal view (item 4).** The walk path's portal shot sits behind `tour.walk_pressed && !tour.walk_open_noted`, so doors opened without E (auto-load, or already open) get no `NN-portal-view.png`. Take the shot at the first frame the door stands open in the walk, whether or not E was pressed; fix the comment that claims it already does.

## Checks
- Unit tests for (1) the landing choice (open door: doorway map; closed: XTEL) and (2) the missing-door arm.
- Run the short tours: `<exe> --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --demo-tour <dir> --tour-doors 2` (scripted) and the same with `--walk`. Both pass; each writes `00-portal-view.png` and `01-portal-view.png`. Open one scripted `NN-portal-view.png` and the tour's first frame after that swap and say whether they match.

## Build and run rules
- Build: `cargo build --profile quick --bin engine` in your own worktree with its own target (the harness sets it), `CARGO_BUILD_JOBS=6`. Check `quick/engine.d` names your worktree before running the exe.
- Tests run in your worktree's target; check that your new tests appear in the run.
- Assets: `C:\Modding\Mudcrab\SkyrimConverted` (read-only).
- No timing or benchmark runs. If the lead messages HOLD, finish the command in hand and start no cargo or engine command until told to resume.

## Commits
Do not commit; the lead integrates. Leave a handoff note in `docs/handoff/impl-259.md`.

## Report
The landing rule and where it lives (file:line), test names, both tours' last lines, and whether the scripted portal view matches the post-swap frame.
