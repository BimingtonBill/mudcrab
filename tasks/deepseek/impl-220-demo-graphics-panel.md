# impl-220 - An in-game graphics panel that switches the settings live

Owned files: crates/engine/src/graphics_panel.rs (new),crates/engine/src/demo_hud.rs (styling helpers only),crates/engine/src/player.rs (controls text line only),crates/engine/src/lib.rs,crates/engine/src/app.rs (registration only),docs/demo/README.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal (the demo). After impl-219 (the `GraphicsSettings` resource). Read `local/research/fun/rendering-plan.md` step 1 and impl-219's commit.

## What to build

A panel on a hotkey (`G`, if free: check player.rs, pose_capture.rs, tonemapper.rs and field_notes.rs):
- The layout, in the demo HUD's style (`demo_hud`), lists every knob with its current value and the active preset.
- Arrow keys, or `Tab` and `+`/`-`, move between knobs and change them live.
- `1`/`2`/`3` switch to the `current` / `bevy` / `custom` preset.
- `S` saves to `local/graphics.toml`.
- While the panel is open, game keys are blocked, the way field_notes blocks them while the note box is open.
- A one-frame flash on a pipeline change is expected. Say so in the panel's footer.

## Checks

- Unit tests: navigating the knobs, changing values within bounds, switching presets, and the save and load round trip.
- A screenshot of the panel from an automated run. Add the smallest hidden flag that opens it, if you need one.
- The short tour passes.

## Report

The key; the layout; test names; the screenshot path; the tour's last line.
