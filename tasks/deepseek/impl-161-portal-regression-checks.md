# impl-161 - Two regression checks: the doorway camera's pose, and broken-looking reference renders

Owned files: crates/engine/src/portal.rs,tools/research/exposure_stats.py,tools/compare_shots.py,tests/test_exposure_check.py

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check,python -m unittest discover -s tests

Track: portal. Read first: `docs/research/checks-for-user-found-defects.md` (research-158), proposals
**#2** and **#5**; they have file:line sketches.

1. **#2, a unit test in `portal.rs`'s test module:** the portal camera is placed from the main
   camera's pose of *this* frame. Move the main camera's `Transform` in `Update` (a test system
   ordered before the portal's systems, the way the player moves it), run one update, and assert the
   portal camera's `Transform` equals the door map applied to the *new* pose. It must fail if
   `MainCameraQuery` reads `GlobalTransform` again (the bug fixed in `63af9d3`) - demonstrate that
   by temporarily reverting that line, and say so in the report. research-158 notes `update_portal`
   needs a `StreamingWorld`; use the existing portal test fixtures (look for how other portal tests
   build one) rather than changing production code. No production change in `portal.rs` unless a
   `#[cfg(test)]` helper is unavoidable.
2. **#5, image checks on reference renders:** `exposure_stats.py` gains a `flat_white` statistic
   (share of pixels with luminance >= 0.9 and saturation < 0.05) and a `--check` mode that reads
   optional per-shot `"metrics"` bands from the shots file (`flat_white_max`, `mean_luma_min`,
   `mean_luma_max`) and exits non-zero with one line per failing shot naming the shot, the value and
   the band. Shots without `metrics` are reported, not failed. `tests/test_exposure_check.py` covers
   the statistic and the check on small generated images (no game data). Then run it over the
   existing renders `local/reference/cmp-103/riverwood_shots/` with
   `tools/reference/riverwood_shots.json` and report each shot's `flat_white` and mean luma - the
   lead will choose the bands from those numbers. Do **not** edit `tools/reference/*.json`.
