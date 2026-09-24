# impl-164 - Make the fit-by-hand tool usable: fly freely, see the reference, see the controls

Owned files: crates/engine/src/pose_capture.rs,crates/engine/src/player.rs,crates/engine/src/app.rs,crates/engine/src/config.rs,docs/design/reference-shots.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user tried `--start-shot` (impl-159, `pose_capture.rs`) to line up reference
screenshots by hand, and reported three problems. Fix all three.

1. **"I can't move around."** `--start-shot` starts on the scripted `fly_camera` and refuses
   `--walk`, so nothing the user presses moves the camera. A `--start-shot` run must use the player
   controller (`player.rs`, `PlayerPlugin`) **starting in its free-flight mode** at the shot's pose
   (mouse look, WASD, Space up, Shift down, Ctrl fast - the existing flight), with no gravity and no
   ground snap until the user presses `F`. The camera must start exactly at the shot's pose (yaw and
   pitch included), not the controller's default. Keep `P` saving the pose as it does today.
2. **Show the reference photo in the tool,** scaled down in a corner (about a quarter of the window
   width, keeping its aspect, top-right), so the user can compare without switching windows. The
   photo is the shot's `reference` field if it has one, else `local/reference/uesp/<name>.jpg` (the
   rule `tools/compare_shots.py` uses). `R` toggles it; `[` / `]` make it smaller / bigger. A missing
   or unreadable file shows a one-line notice instead of failing the run.
3. **The controls on screen,** as a small help line/panel: move, up/down, fast, look, F walk/fly,
   P save pose (and a brief "saved" confirmation for a couple of seconds after P), R reference
   picture, [ ] its size, Esc release the mouse. Only in `--start-shot` runs (normal play keeps its
   own help line).

Tests where logic allows (the reference path rule; the start pose reaching the controller unchanged).
Then build (`cargo build --release -p engine`) and run
`target/release/engine.exe --assets C:\Modding\SkyrimConverted --start-shot tools/reference/riverwood_shots.json RW-03-trader-front`
for a few seconds with `--acceptance-screenshot local/t164/start.png --benchmark-duration 8` (or
another self-exiting way) and look at the screenshot: the reference in the corner and the help text
must be visible and the view must be the shot's. Say plainly that key presses cannot be tested
headlessly, and what you checked instead. Do not break the Riverwood or Alftand demo tours
(`--demo riverwood --walk --demo-tour local/t164-rw` must still pass).
