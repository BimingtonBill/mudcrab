# impl-564-p2-audit-engine

Track: Phase 2. Fixed the engine findings from the 2026-09-24 standing audit that also hold on
upstream's `main`, in worktree `D:/wkj-impl/impl-561` on branch `impl/564-audit-engine` (cut from
`main`). Left uncommitted for the lead to review and commit.

## Findings

All five findings hold on `main` (confirmed by reading each file before changing it; no finding
was skipped).

### ES-01 -- `crates/engine/src/world/cache.rs:57-65` (`CellCache::terrain`)

Held: `terrain` called `rkyv::access::<ArchivedCellCache, Error>(&self.mmap)` (full bytecheck
validation) on every lookup, even though `open` (line 40) already validates the same bytes once
and `self.mmap` never changes afterwards.

Changed: `terrain` now uses `unsafe { rkyv::access_unchecked::<shared::ArchivedCellCache>(&self.mmap) }`,
with a safety comment explaining that `open`'s one-time `rkyv::access` validation covers these
exact, immutable bytes. Checked the pinned `rkyv 0.8.18` source
(`access_unchecked<T: Portable>(bytes: &[u8]) -> &T` in `rkyv-0.8.18/src/api/mod.rs`) to confirm
the signature and safety contract before using it; no newer once-validated handle type is offered
at this version, so this is the fix the brief asked for.

Every existing cache test is unchanged (`maps_and_reads_versioned_terrain`,
`rejects_previous_cache_version`, `rejects_truncated_cache`) and still passes -- `open` still
refuses a corrupted/truncated/wrong-version file, since that validation path was not touched.
Added `repeated_terrain_lookups_return_the_right_cells`, which opens a cache with two cells and
calls `terrain` on them out of order and repeatedly (including a missing id), to exercise that the
unchecked access still indexes the right cell every time rather than just once. This is a
correctness-preserving performance fix, not a bug fix, so there is no old-code-fails/new-code-passes
functional test to write here; the practical test is "still returns the right cells, still refuses
a corrupted file at `open`", which is what the brief asked for and what these four tests check.

### SC-03 -- `crates/engine/src/app.rs:1303-1308` (`initial_camera_ground_height`)

Held: opened `skyrim_world.db` with `Connection::open` (read-write, create-if-missing).

Changed: `Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)`, matching
`world/database.rs`'s `AssetCatalog::open` and `validate`.

Tests: `ground_height_query_reads_the_right_cell` (unchanged-behaviour regression: still returns
the correct height for the matching cell) and `ground_height_query_does_not_create_a_missing_database`.
The second one is the fails-on-old/passes-on-new test -- I first tried discriminating old vs. new
by marking the (existing) database file read-only on disk, but that did not distinguish them: SQLite's
own `sqlite3_open()` silently downgrades to a read-only connection when it cannot get read-write
access, so a `SELECT`-only query behaves identically either way (verified by hand -- see below).
The real observable difference is the old code's `CREATE`-if-missing flag: against a database path
that does not exist yet, the old code silently creates an empty file as a side effect before the
query fails, while an explicit read-only open fails without creating anything. I verified this by
temporarily reverting the fix and confirming `ground_height_query_does_not_create_a_missing_database`
fails (`assertion failed: !database_path.exists()`) on the old code, then restored the fix and
reran both tests to confirm they pass; final `cargo test -p engine` run below is on the restored,
correct code.

### SC-04 -- `crates/engine/src/bin/world-inspect.rs:233-239` (`inspect_world`)

Held: opened the database read-write and read `database_schema` without ever comparing it to
`shared::WORLD_DATABASE_SCHEMA_VERSION` before running further queries (the per-grid cell lookups
and `CellCache::open`).

Changed: open with `OpenFlags::SQLITE_OPEN_READ_ONLY`, and immediately after reading
`database_schema` (typed as `u32` explicitly, since `ensure!` needed a concrete type), `ensure!`
it equals `shared::WORLD_DATABASE_SCHEMA_VERSION` before any other query, with the same message
style `world/database.rs::validate` uses: `"world database schema {version} is unsupported;
reconvert assets for version {}"`.

Test: `rejects_mismatched_schema_before_any_further_query` creates a database with only a
`schema_info` table at the wrong version (deliberately no `cells` table and no `cell_cache.rkyv`),
and asserts the error message names the schema mismatch rather than some later, unrelated failure.
Verified fails-on-old/passes-on-new by temporarily reverting: on the old code the function instead
fails at `CellCache::open` (`cell_cache.rkyv` does not exist) with a wholly unrelated "system
cannot find the file specified" error, which does not mention "schema" or "unsupported" -- proving
the old code ran well past the point where it should have refused. Restored the fix afterward.

### SC-05 -- `converted_texture_path` (`crates/engine/src/world/database.rs:127-139`) and
`converted_model_path` (`crates/engine/src/streaming.rs:1281-1305`)

Held in both: after re-rooting a database-supplied path under `textures/` or `meshes/`, no check
rejected a `..` segment or an absolute/drive-prefixed path before `PathBuf::join` and
`asset_server.load`.

Changed: added a private `is_safe_relative_asset_path` helper (duplicated in each file -- the
brief's scope for SC-05 covers only these two functions, not a shared module) that rejects the
path unless every `std::path::Component` is `Component::Normal`. Both functions now call it right
after computing `without_prefix` and return `None` (the same "no path" result callers already
handle) if it fails.

Tests: `rejects_traversal_and_rooted_texture_paths` (database.rs) and
`rejects_traversal_and_rooted_model_paths` (streaming.rs), each covering a `..` traversal, a
`//`-rooted path, a backslash-form traversal, and (Windows-only, `#[cfg(windows)]`, since CI runs
on `ubuntu-latest` per `.github/workflows/run_tests.yml` and Rust's path-component parsing is
OS-native) a drive-prefixed absolute path, plus one plain relative path proving normal behaviour is
unaffected. These are straightforward old-code-panics-or-returns-Some / new-code-returns-None
cases; I did not separately hand-revert them since the function is a pure, directly unit-tested
helper and the before/after behaviour is obvious from the diff, but they follow the same shape as
the other confirmed fails-on-old tests above.

### SC-02 (low) -- `crates/engine/src/app.rs:1215-1219` (`converter_schema_version`) vs.
`crates/converter/src/cache.rs:11` (`CONVERTER_SCHEMA_VERSION`)

Held: both are hand-maintained literal `14`s with no guard tying them together.

The brief's scope for this finding is `crates/shared/src/lib.rs` only -- not `app.rs` beyond the
ground-height query, not `crates/converter/src/cache.rs`, and no new dependencies (so no dev-dependency
edge from `shared` or `engine` onto `converter` either, and Cargo.toml is not an owned file for this
task). So I could not make the engine actually depend on the converter crate, or wire either
existing constant to reference a new shared one -- either would mean editing files outside this
task's scope.

What I did: added `shared::CONVERTER_SCHEMA_VERSION` (= 14) to `crates/shared/src/lib.rs` as the
canonical value, documented as such, and added a workspace-level guard as two tests in
`crates/shared/src/lib.rs`'s own `tests` module: `converter_schema_version_matches_converter_crate`
and `converter_schema_version_matches_engine_crate`. Each uses `include_str!` to read
`crates/converter/src/cache.rs` and `crates/engine/src/app.rs` as plain text (compile-time file
access, not a Cargo dependency), parses out the respective literal, and asserts it equals
`shared::CONVERTER_SCHEMA_VERSION`. This is deliberately the "workspace-level test" option the
brief offered, chosen over "a shared constant that both use" because actually wiring the existing
constants to `shared::CONVERTER_SCHEMA_VERSION` needs edits to `app.rs` (outside "the ground-height
query only") and `converter/src/cache.rs` (not owned at all here) -- out of scope for this task.
This is not as strong as a real single source of truth, and it is a bit unusual (parsing sibling
crates' source as text), but it gives an automated guard against the three copies drifting without
touching anything outside the owned files or adding a dependency. A clean follow-up (small, but
needs `app.rs` beyond the ground-height query and `converter/cache.rs`, both out of this task's
scope) would be to have both crates `use shared::CONVERTER_SCHEMA_VERSION` directly and delete
these text-parsing tests.

## Checks

```
$ cargo fmt --all -- --check
(no output -- clean)

$ cargo clippy -p engine --all-targets -- -D warnings
    Checking shared v0.1.0 (D:\wkj-impl\impl-561\crates\shared)
    Checking engine v0.1.0 (D:\wkj-impl\impl-561\crates\engine)
    Finished `dev` profile [optimized + debuginfo] target(s) in 1.26s

$ cargo clippy -p shared --all-targets -- -D warnings
    Checking shared v0.1.0 (D:\wkj-impl\impl-561\crates\shared)
    Finished `dev` profile [optimized + debuginfo] target(s) in 0.27s

$ cargo test -p engine
running 47 tests
test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

     Running unittests src\main.rs (target\debug\deps\engine-678cd4098bdc2be7.exe)
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src\bin\world-inspect.rs (target\debug\deps\world_inspect-3bfb433de86de37a.exe)
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests engine
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

$ cargo test -p shared
running 8 tests
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests shared
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## Report

- **ES-01**: holds on `main` at `crates/engine/src/world/cache.rs:57-65`. Changed `terrain` to use
  `rkyv::access_unchecked` instead of re-validating (`rkyv::access`) on every call, with a safety
  comment. Test: `world::cache::tests::repeated_terrain_lookups_return_the_right_cells` (plus the
  three pre-existing cache tests, all kept and still passing).
- **SC-03**: holds on `main` at `crates/engine/src/app.rs:1303-1308`. Changed
  `initial_camera_ground_height` to `Connection::open_with_flags(..., OpenFlags::SQLITE_OPEN_READ_ONLY)`.
  Tests: `app::tests::ground_height_query_reads_the_right_cell` and
  `app::tests::ground_height_query_does_not_create_a_missing_database` (the latter is the one
  verified to fail on the old code).
- **SC-04**: holds on `main` at `crates/engine/src/bin/world-inspect.rs:233-239`. Opened read-only
  and added an `ensure!` on `database_schema == shared::WORLD_DATABASE_SCHEMA_VERSION` immediately
  after reading it, before any further query, with the engine's message style. Test:
  `tests::rejects_mismatched_schema_before_any_further_query` (verified to fail on the old code).
- **SC-05**: holds on `main` in both `crates/engine/src/world/database.rs:127-139`
  (`converted_texture_path`) and `crates/engine/src/streaming.rs:1281-1305`
  (`converted_model_path`). Added a private `is_safe_relative_asset_path` helper in each file that
  rejects any non-`Normal` path component (`..`, root, or a Windows drive prefix). Tests:
  `world::database::tests::rejects_traversal_and_rooted_texture_paths` and
  `streaming::tests::rejects_traversal_and_rooted_model_paths`.
- **SC-02** (low): holds on `main` between `crates/engine/src/app.rs:1215-1219`
  (`converter_schema_version`, returns a hardcoded `14`) and `crates/converter/src/cache.rs:11`
  (`CONVERTER_SCHEMA_VERSION = 14`). Added `shared::CONVERTER_SCHEMA_VERSION` to
  `crates/shared/src/lib.rs` as the canonical value, plus a workspace-level test pair
  (`tests::converter_schema_version_matches_converter_crate`,
  `tests::converter_schema_version_matches_engine_crate`) that reads both crates' source text via
  `include_str!` and checks their literals against it -- chosen over wiring the existing constants
  directly because that needs edits to `app.rs` beyond the ground-height query and to
  `crates/converter/src/cache.rs`, both outside this task's owned files. See the SC-02 section
  above for the full reasoning and the suggested follow-up.
