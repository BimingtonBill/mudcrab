# impl-271 - The last grid-offset copies go through streaming's helpers

Owned files: crates/engine/src/portal.rs,crates/engine/src/portal_bench.rs,crates/engine/src/transition.rs,docs/handoff/impl-271.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Follow-up of impl-268 (commit 523683d), which made `crates/engine/src/streaming.rs` own the grid-to-render offset: `grid_translation(grid, origin)`, its inverse `absolute_render_position(render_position, origin)` and `camera_grid(render_position, origin)`. Read those three and their doc comments first.

## Goal
Route the three copies of the same formula left in files impl-268 did not own through those helpers, with no behaviour change:
1. `portal.rs` ~932-937, `ActiveSpace::camera_grid`: use `streaming::camera_grid`.
2. `portal_bench.rs` ~773-774, `camera_cell` (`grid_of` on the offset position): use `camera_grid`.
3. `transition.rs` ~1004-1008, `creation_from_render`: its exterior arm uses `absolute_render_position`.
Then search `crates/engine/src` for any remaining hand-written `* CELL_SIZE` with an origin or grid next to it that converts between grid and render space (not Creation-axis helpers in `world/components.rs`), and list them in your report without changing files you do not own.

## Checks
- `cargo test -p engine --lib` in your worktree; the existing tests of each function must still pass unchanged. If a function has no test that pins it at a non-zero origin, add one.
- Short walked tour: `target/quick/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --walk --demo-tour <dir under your worktree> --tour-doors 2`. Report its last line.

## Build and run rules
- Build: `cargo build --profile quick --bin engine` in your own worktree with its own target, `CARGO_BUILD_JOBS=6`; check `quick/engine.d` names your worktree.
- No timing runs. If the lead messages HOLD, finish the command in hand and start no cargo or engine command until told to resume.

## Commits
Do not commit; the lead integrates. Handoff note in `docs/handoff/impl-271.md`.

## Report
Each call site changed (file:line), tests added, the tour's last line, and the list of remaining copies elsewhere.
