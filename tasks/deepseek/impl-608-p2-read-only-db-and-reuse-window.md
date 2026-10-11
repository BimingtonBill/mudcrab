# Update 2026-09-27 (lead)
Part (1) is DROPPED: `phase2/audit-engine` 9e40e93 already makes every engine-side database open
read-only (the one remaining `Connection::open` builds a fixture database and must stay writable).
Do only part (2), the single reuse window, from `main`. Ignore every instruction below about part (1).

# Goal
Track: Phase 2 (for upstream). Two small contract fixes: (1) the engine opens the converted world
database read-only everywhere, and (2) the converter's "which older converter schemas can be reused"
rule lives in one place and is pinned by a test over its whole range.

# Context
- Your checkout is cut from `main` (upstream). Upstream commit style (`fix(engine): ...`,
  `refactor(converter): ...`, `test(converter): ...`), no `[deepseek]` prefix, no project-internal
  references.
- (1) Converted assets are read-only input to the engine. Most opens already use
  `OpenFlags::SQLITE_OPEN_READ_ONLY` (see `crates/engine/src/world/database.rs`), but these use a
  plain `Connection::open` (read-write, and it creates an empty file if the path is missing):
  `crates/engine/src/app.rs` (two sites on main, around :225 and :1308; one is the initial camera
  ground-height probe) and `crates/engine/src/bin/world-inspect.rs` (~:239). Make every engine-side
  open read-only (`Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY |
  OpenFlags::SQLITE_OPEN_NO_MUTEX)` or the flags the existing read-only sites use; match them).
  Add a test that opening a missing database path fails instead of creating a file, for the
  helper you route these through (one shared helper is welcome if it removes duplication).
- (2) The reuse window is written twice: `crates/converter/src/cache.rs` (~:61, `matches!(
  manifest.schema_version, 12 | 13) && CONVERTER_SCHEMA_VERSION == 14`) and
  `crates/converter/src/pipeline.rs` (~:77, the same condition for the previous manifest). A bump
  that extends one leaves the other stale and silently forces a full reconversion. Move it to one
  named function (with a doc comment saying what changed in those schemas that makes reuse safe,
  taken from the existing comment) used by both, and a test that iterates every schema from 0 to
  CONVERTER_SCHEMA_VERSION + 1 and pins exactly which are reusable.

# Scope
Owned files: `crates/engine/src/app.rs` (only the database opens), `crates/engine/src/bin/world-inspect.rs`,
`crates/engine/src/world/database.rs` (only if the shared helper lives there),
`crates/converter/src/cache.rs`, `crates/converter/src/pipeline.rs` (only the reuse condition), and
`docs/handoff/impl-608-p2-read-only-db-and-reuse-window.md`. Say in the report if you need anything else.

# Done when
- No `Connection::open(` remains in `crates/engine` (grep), and the new tests pass.
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p engine -p converter --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings`
  `cargo nextest run -p engine -p converter --target-dir C:/Modding/shared-check-phase2`
- Two commits (one per part), upstream style.

# Shared build folder caveat
`C:/Modding/shared-check-phase2` is shared by several Phase 2 branches, and cargo can reuse another
branch's build of the `shared` crate there (different schema constants). If a test fails naming a
schema or version number, re-run that test without `--target-dir` (the checkout's own `target/`)
before believing it, and report both results.

# Report
Per part: files changed, test names, and the grep result for (1).
