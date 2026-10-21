# impl-047 - Engine cleanup for upstream, part 1: no private names, no machine paths (free files only)

Owned files: crates/engine/src/lights.rs,crates/engine/src/render.rs,crates/engine/src/streaming.rs,crates/engine/src/config.rs,crates/engine/src/shots.rs,crates/engine/src/snow.rs,crates/engine/src/doors.rs,crates/engine/src/door_animation.rs,crates/engine/src/metrics.rs,crates/engine/src/world/components.rs,crates/engine/src/world/cache.rs,crates/engine/src/shaders/terrain.wgsl,crates/engine/src/shaders/portal.wgsl,crates/engine/src/shaders/snow.wgsl
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`, and
`docs/pr/upstream-pr-plan.md` (its cleanup items and risks).

## Goal

The engine changes will go upstream. In the **owned files only**, remove every reference to the
fork's private tooling and process from production code and doc comments:
- `tasks/deepseek/...`, `impl-0NN`, `research-0NN`, `review-0NN`, `lead-0NN`;
- "DeepSeek", "worker", "the lead";
- `docs/handoff`, and `local/` paths such as `local/calib/calibration.md`;
- user names and machine paths.

Keep all the technical content: the measurements, the reasoning and the data facts. Rewrite
provenance as data or dates, for example "measured against the UESP reference screenshots
(2026-09)", and point to committed docs (`docs/research/...`, `docs/design/...`) where the
evidence lives. Check that each linked doc exists.

Also:
- tests that read `C:/Modding/Mudcrab/SkyrimConverted/...` or other machine paths use
  `MUDCRAB_CONVERTED_DIR` (for converted output) or `MUDCRAB_SKYRIM_DATA` (for the game
  install), skip when unset, and keep no machine-path fallback. The converter uses the same
  convention (commit `410ce31`; ADR-0002);
- keep behaviour byte-identical. Only comments, docs and test plumbing may change.

## Check

- The acceptance commands must pass. Keep every test: counts may not shrink.
- With `MUDCRAB_CONVERTED_DIR=C:/Modding/Mudcrab/SkyrimConverted`, run
  `cargo test -p engine --lib -- --include-ignored` and report what ran.
- In the report, list the remaining private names in the engine files you do **not** own
  (`app.rs`, `portal.rs`, `player.rs`, `transition.rs`, `demo_tour.rs`, `world/database.rs`,
  `lib.rs`), so part 2 can be briefed.

## Scope

Only the owned files. No crosstalk partner. Report per the implementation template.
