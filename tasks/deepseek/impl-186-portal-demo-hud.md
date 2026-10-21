# impl-186 - One clear demo HUD: controls panel with F12, door prompt, notices; no objective

Owned files: crates/engine/src/demo_hud.rs,crates/engine/src/player.rs (HUD code only: `setup_player_hud`, `HelpLine`, `HELP_TEXT`, `DoorPrompt`, `centered_bar` and the systems that fill or fade them),crates/engine/src/demo_tour.rs (objective code only: `DemoObjective`, `spawn_demo_objective`, `update_demo_objective`, the routes' `objective` field and its tests; another task edits the tour's repeat/dwell code in the same file at the same time, so leave everything else alone),crates/engine/src/field_notes.rs,crates/engine/src/pose_capture.rs (only where it hides the player's help line),crates/engine/src/lib.rs,docs/demo/README.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user, after playing (2026-09-25): "improve the UI for the demo, there's orange text at the top and white text at the bottom, weird. also no UI telling the user that F12 is screenshot key. look at the UI for the pose tool for inspiration. also the demo doesn't need an objective. so unify and clarify UI."

## What is on screen now (see `local/captures/2026-09-25_02-01-35/02.png` and `03.png`)

- Top left, cream/orange monospace: the demo's objective line (`demo_tour.rs` `spawn_demo_objective`/`update_demo_objective`, routes' `objective` strings: "Objective: Riverwood's four houses ...", then "Now in Tamriel. Find the next load door (E) - 6 more ...").
- Bottom centre, white with a shadow: the door prompt (`player.rs` `DoorPrompt`, 24 px) and a help line that fades after a while (`HelpLine`, `HELP_TEXT` "WASD move | Shift run | Space jump | F fly | E open | Esc cursor", 14 px).
- F12 field notes (`field_notes.rs`): a note box and a "Saved NN" notice in their own colours and places. F12 is not mentioned anywhere on screen.
- The pose tool's look (`pose_capture.rs` `setup_start_shot_hud`, `START_SHOT_HELP`): one dark translucent panel bottom-left (`srgba(0,0,0,0.55)`, 8 px padding, 12 px from the edges) with cream text (`srgb(0.95,0.92,0.8)`, 16 px) listing every key as `Key: action` groups separated by `|`, and a green notice top-left for "saved".

## The design (decided; build this)

A new module `demo_hud.rs` holds the HUD's one style (the pose tool's: cream text, dark translucent panels, 12 px margins; constants for the colours, sizes and margins) and the helpers every HUD piece uses to spawn a panel, so the pieces look like one thing.

1. **Controls panel, bottom-left**, in the pose tool's style, always visible in a walked demo:
   `Mouse: look (click the window first)  |  WASD: move  |  Shift: run  |  Space: jump`
   `E: open / close door  |  F: walk / fly  |  F12: screenshot + note  |  H: hide these keys  |  Esc: release the mouse`
   `H` toggles it (hidden shows a one-line "H: show keys" in the same corner, smaller). It replaces the fading help line. The fly mode shows the fly keys (Space / Shift: up / down, Ctrl: fast) instead of jump/run.
2. **Door prompt**, centred a little below the middle of the screen, as a small panel in the same style (not bare white text): `E  Open  Sven's House` / `E  Close` / the "(loading)" form, exactly the texts `player.rs` builds now.
3. **Notices, top-left**, one place for short-lived messages, fading after ~3 s: "Saved 03 (local/captures/...)" from F12, and the name of the place on arrival after a crossing ("Riverwood Sven's House", "Riverwood") taken from what the crossing already knows (the objective's "Now in ..." text shows where it comes from). No counts, no "find the next door".
4. **No objective.** Remove the objective line, its countdown and the routes' `objective` strings (and their tests); the tour's log line that prints the objective drops it.
5. **Note box** (F12): centred panel in the same style, a title line "Note for 03", the typed text with a caret, and a hint line "Enter: save  |  Esc: no note". While it is open the controls panel stays as it is.
6. The HUD is hidden in automated runs whose screenshots must stay clean: `--demo-tour` and `--shots` (check how the objective and help line are hidden or shown in those today and keep the same rule), and `--start-shot` keeps the pose tool's own panel (it hides the player's help today; make it hide the new controls panel instead).

## Checks

- Unit tests for the pure parts (the controls text for walk and fly, the prompt and notice texts, the H toggle state).
- `cargo build --profile quick --bin engine`; the short tour `target/quick/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --walk --demo-tour local/t186 --tour-doors 2` still ends `tour PASSED after 2 crossings (short tour)`, and its screenshots show no HUD text.
- A screenshot of the HUD in a normal walked demo: use `--field-notes-test` (it takes an F12 capture after the first crossing) together with `--demo-tour local/t186-hud --tour-doors 2 --show-window --captures-dir local/t186-captures` if the HUD shows in that run, or add the smallest hidden flag you need; show the image with the controls panel, a door prompt or a notice. Read only `local/t186*` and `local/captures/2026-09-25_02-01-35/` under `local/`.
- `docs/demo/README.md`: the controls table matches the panel; no mention of an objective.

## Report

Files changed; a picture of the new HUD (path); what each old HUD piece became; test names; the tour's last line.
