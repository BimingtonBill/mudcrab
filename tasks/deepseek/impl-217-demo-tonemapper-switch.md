# impl-217 - Quick switching between tonemappers in the demo

Owned files: crates/engine/src/tonemapper.rs (new),crates/engine/src/config.rs (the flag only),crates/engine/src/app.rs (plugin registration only),crates/engine/src/lib.rs,crates/engine/src/player.rs (only the controls text: one key added to `controls_text`),docs/demo/README.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal (the demo). The user's ask, relayed by QnA (2026-09-25): add quick switching between tonemappers in the demo engine to compare looks; they are curious about AgX in particular.

Context:
- Neither main nor portal sets a `Tonemapping` on the main camera, so it is Bevy 0.19's default, TonyMcMapface.
- The portal camera's `Tonemapping::None` (impl-202) must stay as it is: the main view tonemaps the composited image.
- Bevy's `tonemapping_luts` feature is on, so all of these are available: AgX, TonyMcMapface, KhronosPbrNeutral, AcesFitted, BlenderFilmic, SomewhatBoringDisplayTransform, Reinhard, ReinhardLuminance.
- `local/research/imagespace-colour-grading.md` suggests KhronosPbrNeutral as the closest match to vanilla Skyrim. AgX is an "enhancement" look.

## What to build

1. `--tonemapper <name>`: case-insensitive names as listed above, default TonyMcMapface. An unknown name fails with the list of valid names.
2. An in-game key, `T` (check it is free in player.rs and pose_capture.rs; pick another if not and say which), that cycles to the next tonemapper on the main camera. It shows the name in the HUD's notices panel (`demo_hud::Notices`, top-left, the same style) and adds it to the controls panel line.
3. `--shots` honours the flag, so screenshots can be compared side by side. Add a `tonemapper` suffix option or just document running `--shots` once per name.
4. Expected behaviour to document: changing the tonemapper recompiles the pipeline, and Bevy skips tonemapping for a frame or two while it compiles, so one untonemapped flash when cycling is normal.

## Checks

- Unit tests: name parsing (all 8 names, case-insensitive, unknown), cycling order, and the portal camera's tonemapping unchanged.
- `cargo build --profile quick --bin engine`. The short tour passes.
- Render one Riverwood pose under each tonemapper with `--shots` (use `tools/reference/riverwood_shots.json`, one shot) and make a contact sheet: `local/t217/sheet.jpg`, labelled.
- Under `local/`, read only `local/t217*` and the research file.

## Report

The key and flag; test names; the contact sheet path; the tour's last line.
