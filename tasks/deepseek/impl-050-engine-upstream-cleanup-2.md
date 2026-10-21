# impl-050 - Engine cleanup for upstream, part 2: player, transition, demo tour

Owned files: crates/engine/src/player.rs,crates/engine/src/transition.rs,crates/engine/src/demo_tour.rs,crates/engine/src/lib.rs
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
- In the report, list the remaining private names in `app.rs`, `portal.rs` and
  `world/database.rs`, which another task owns right now, so part 3 can be briefed.

Part 1 (commit `62c0f7b`) did the same job on the other engine files: follow its wording choices,
for example "measured against the UESP reference screenshots (2026-09)" and
`MUDCRAB_CONVERTED_DIR`.

## Scope

Only the owned files. No crosstalk partner. Report per the implementation template.
