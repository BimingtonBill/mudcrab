# impl-201 - The doorway view is not rendered when a wall hides the doorway

Owned files: crates/engine/src/portal.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. After impl-188 (off-screen skip), impl-189 and impl-199 (same file). Design and options: `local/research/occlusion-culling-2026-09-25.md` (research-198, the doorway section).

The doorway can be on screen (impl-188's rect is non-empty) yet hidden behind a wall or a house corner; the portal camera still renders the whole far side. Skip it then: test visibility of the doorway (centre and four corners) from the eye, cheaply and rate-limited (e.g. `MeshRayCast` against the active space's meshes, a few times a second, with hysteresis so the image does not flicker on at the edge; or the main view's previous-frame depth pyramid if research-198 shows it can be read here). While hidden: portal camera inactive, quad hidden. Visible again: back on the next frame, with the far side still resident (so no pop).

## Checks
- Unit tests for the visibility decision and hysteresis.
- impl-194's `--tour-bench` (if merged) adds an "open-occluded" case or a before/after with the doorway behind a wall; otherwise log how many frames were skipped on the short tour.
- Short tour passes. Read only `local/t201*` and the research file under `local/`.

## Report
Mechanism, cost per frame of the test, test names, numbers, the tour's last line.
