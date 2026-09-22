# impl-051 - Engine cleanup for upstream, part 3: app, portal, database

Owned files: crates/engine/src/app.rs,crates/engine/src/portal.rs,crates/engine/src/world/database.rs,crates/engine/src/world/lighting.rs
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
- tests that read `C:/Modding/SkyrimConverted/...` or other machine paths use
  `OPENSKYRIM_CONVERTED_DIR` (for converted output) or `OPENSKYRIM_SKYRIM_DATA` (for the game
  install), skip when unset, and keep no machine-path fallback. The converter uses the same
  convention (commit `410ce31`; ADR-0002);
- keep behaviour byte-identical. Only comments, docs and test plumbing may change.

## Check

- The acceptance commands must pass. Keep every test: counts may not shrink.
- With `OPENSKYRIM_CONVERTED_DIR=C:/Modding/SkyrimConverted`, run
  `cargo test -p engine --lib -- --include-ignored` and report what ran.
- This is the last part. In the report, confirm with a repo-wide grep over `crates/` that no
  private name is left in any Rust file (`impl-0`, `research-0`, `review-0`, `lead-0`,
  `deepseek`, `tasks/deepseek`, `docs/handoff`, `local/`, `Brannock`, `C:/Users`, `C:/Modding`,
  `C:/Games`), and list anything you had to leave.

Parts 1 and 2 (commits `62c0f7b`, `944250e`) did the same job on the other engine files: follow its wording choices,
for example "measured against the UESP reference screenshots (2026-09)" and
`OPENSKYRIM_CONVERTED_DIR`.

## Scope

Only the owned files. No crosstalk partner. Report per the implementation template.
