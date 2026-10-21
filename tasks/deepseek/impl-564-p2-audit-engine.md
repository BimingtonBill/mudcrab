# Goal
Track: Phase 2 (for upstream). Fix the engine findings from the 2026-09-24 standing audit that also
hold on upstream's `main`.

# Context
- Your checkout is cut from `main`. Use upstream's commit style (`fix(engine): ...`,
  `perf(engine): ...`), with no `[deepseek]` prefix.
- The findings were filed against the fork (`portal` branch), so line numbers differ on `main`.
  **First confirm each one on `main`**; skip any that does not hold there, and say why in the report.
  1. **ES-01** `crates/engine/src/world/cache.rs` (`CellCache::terrain`): calls
     `rkyv::access::<ArchivedCellCache, Error>` on the whole mapped file on every lookup, and the
     type is `#[rkyv(bytecheck())]`, so each call re-validates every cell. It is called once per
     committed cell. The mapping is validated once in `open` and never changes. Validate once in
     `open` and use unchecked access afterwards (`rkyv::access_unchecked`), keeping the safety
     argument in a comment. If the rkyv version in the lockfile offers a better once-validated
     handle, use that instead. Keep every existing cache test.
  2. **SC-03** `crates/engine/src/app.rs` (`initial_camera_ground_height`): opens
     `skyrim_world.db` with plain `rusqlite::Connection::open` (read-write, create-if-missing).
     Open it read-only, the way `world/database.rs` does
     (`OpenFlags::SQLITE_OPEN_READ_ONLY`, plus whatever flags that file uses).
  3. **SC-04** `crates/engine/src/bin/world-inspect.rs`: opens the database read-write and queries
     before comparing its schema version with `WORLD_DATABASE_SCHEMA_VERSION`. Open read-only,
     and refuse a mismatched version before any query, with the same message style the engine uses.
  4. **SC-05** (low) `converted_texture_path` in `crates/engine/src/world/database.rs` and
     `converted_model_path` in `crates/engine/src/streaming.rs`: they re-root a database-supplied
     path under `textures/` or `meshes/` with `PathBuf::join`, with no check for `..` segments or
     absolute paths, and the result goes to `asset_server.load`. Reject such a path: return
     None, or the error the callers already handle for a missing path.
  5. **SC-02** (low): the engine's converter-schema constant has no guard against the converter's
     (`crates/converter/src/cache.rs`). If the engine may not depend on the converter crate, add
     the check where both are visible: a workspace-level test, or a shared constant in
     `crates/shared` that both use. Say which and why. Changing `crates/shared` is allowed only
     for this one constant.

# Scope
Owned files: `crates/engine/src/world/cache.rs`, `crates/engine/src/app.rs` (the ground-height
query only), `crates/engine/src/bin/world-inspect.rs`, `crates/engine/src/world/database.rs` and
`crates/engine/src/streaming.rs` (the two path functions only), `crates/shared/src/lib.rs` (SC-02
only), tests next to them, and `docs/handoff/impl-564-p2-audit-engine.md`. No converter behaviour
change, no schema bump, no new dependencies.

# Done when
- For each finding that holds on `main`: a test that fails on the old code and passes on yours,
  where one is practical. For ES-01, a test that the terrain lookup still returns the right cells
  and still refuses a corrupted file at `open`.
- `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets -- -D warnings` and
  `cargo test -p engine` pass (and `-p shared` if you touch it).

# Report
- For each finding: holds on `main` or not (with the `main` file:line), what you changed, and the
  test's name.
