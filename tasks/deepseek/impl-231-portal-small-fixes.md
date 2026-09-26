# impl-231 - Three small demo fixes: bevy preset exposure, the tour's double screenshot, the twin-door log

Owned files: crates/engine/src/graphics_settings.rs,crates/engine/src/demo_tour.rs,crates/engine/src/door_animation.rs (twin_model_path and its caller only)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal (the demo). The user affirmed these queue items on 2026-09-26. They are three independent, small fixes; do them as three commits.

## 1. The `bevy` graphics preset is overexposed
- impl-219 (`git log --grep impl-219`) added `GraphicsSettings` with the presets current / bevy / custom.
- The `bevy` preset turns on `AutoExposure` with Bevy's defaults, and the image comes out overexposed.
- The project calibrates light for a fixed exposure: see `lights.rs` `EXPOSURE_CALIBRATION` and `render.rs` `EMISSIVE_EXPOSURE`.

Pick auto-exposure settings, or a compensation curve, so that `bevy` averages close to `current`'s brightness in a daylight exterior and a dim interior, while still adapting between them.
- Poses: one Riverwood exterior from `tools/reference/riverwood_shots.json`, plus one interior (Sven's House or the Sleeping Giant, from `tools/reference/` or `local/gallery/glow-check.json`).
- Measure the mean luminance with `python tools/research/exposure_stats.py`, or a short PIL script over the PNGs.
- Auto exposure needs some frames to converge: check how `--shots` settles, and hold the pose long enough.

## 2. The demo tour writes two screenshots in the same frame
- `demo_tour.rs` sometimes asks for a second screenshot in the frame it already took one, so one image is dropped or duplicated (noted at 07:09 in `local/team/2026-09-26.md`).
- Find the path that requests twice. Make each request once per frame, with a unit test.

## 3. `twin_model_path` logs a missing asset for doors with no twin
- `door_animation.rs`: for example `Dungeons/Mines/MineDoor01.glb`, which never existed (see research-583).
- Check that the path exists before loading it. Log "no twin" at debug level, not as an asset failure. Add a test.

## Build and run rules
- Build: `cargo build --profile quick --bin engine` with `CARGO_TARGET_DIR=D:/Modding-overflow/portal-quick-target` and `CARGO_BUILD_JOBS=6`.
  - Other portal coders share that target dir, so cargo's lock runs one build at a time; the machine's memory commit only fits one.
  - Straight after each build, copy `engine.exe` to `local/t231/engine.exe` and run the copy, because the next coder's build overwrites the shared one.
- Tests and clippy use `--target-dir D:/Modding-overflow/shared-check-target` (portal's shared check target only).
- Assets: `C:\Modding\SkyrimConverted`.
- No timing or benchmark runs.
- Under `local/`, read only `local/t231*`, `local/team/2026-09-26.md` and `local/gallery/`.

## Commits
Subjects start `[claude] impl-231:`; trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. No amend, rebase or force-push.

## Report
- The exposure numbers, current against bevy, for both poses, with the images;
- the double-screenshot cause;
- test names;
- the commit hashes.
