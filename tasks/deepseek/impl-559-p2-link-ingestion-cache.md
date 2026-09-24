# Goal
Track: Phase 2 (for upstream). A reconversion hard-links the unchanged archive-extraction blobs
(`.ingestion-cache/`) and the extracted `vfs/` files from the previous output, instead of copying
them. Today they are copied, so a reconversion needs about 47 GB of extra free disk (25 GB `vfs`,
22 GB cache); on 2026-09-24 that filled the disk and crashed a reconversion.

# Context
- Your checkout is cut from `phase2/staging-hard-links` (commit `a3792e6`). That branch added
  `link_or_copy(from, to)` in `crates/converter/src/pipeline.rs` (about line 1075): a hard link
  with a copy fallback, removing an existing `to` first. It is used for reused converted outputs.
  Use upstream's commit style (`fix(converter): ...`), with no `[deepseek]` prefix.
- A design review (read-only, file:line from this branch) found:
  - `AssetPipeline::run_into` (pipeline.rs:126) calls `ArchiveExtractor::extract_cached`
    (`crates/converter/src/archive/mod.rs:45-88`) with the previous output's `.ingestion-cache`
    and the staging one. The reuse key is the archive's whole-file SHA-256
    (archive/mod.rs:53) against `IngestionCacheEntry.source_hash` (cache.rs:27-30).
  - Blobs are content-addressed: `blob_path` = `cache_root/sha256/<hash[..2]>/<hash>`
    (archive/mod.rs:221-226). A blob cannot go stale: different content means a different path.
    Nothing writes a blob in place.
  - On a cache hit, `restore_cached_files` (archive/mod.rs:173-205) validates each blob's size
    (and re-hashes it with `verify_cache`), then `copy_if_missing` old blob → new blob (194-195) and
    `copy_file` new blob → `vfs` destination (196-197), both plain `fs::copy`.
    `persist_cache_blobs` (207-219) copies vfs → blob the same way.
  - The hazard: `overlay_loose_assets` (pipeline.rs:963-991) runs after extraction and writes
    loose Data-folder files over `vfs` entries with `fs::copy` (line 989). `fs::copy` truncates
    and writes through the existing file. If that file is a hard link to a cache blob, the write
    would corrupt the blob and the previous output. `copy_file` has the same gap when
    `--resume-staging` lands on a file an interrupted run left behind.
- The fix:
  1. Move `link_or_copy` to a place `archive/mod.rs` can use (e.g. `cache.rs`, `pub(crate)`), and
     keep pipeline.rs's callers working.
  2. In `archive/mod.rs`, replace the `fs::copy` in `copy_if_missing` / `copy_file` (and their
     callers above) with `link_or_copy`. Keep every existing size/hash validation.
  3. Make every write into `vfs` remove-then-write, so no write can go through a shared file:
     `overlay_loose_assets` at least, and any other writer you find. Say in the handoff which
     writers you checked.
- On Windows a hard link shares the file, so a later in-place write to either name changes both.
  That is the whole risk; every test below is about it.

# Scope
Owned files:
- `crates/converter/src/archive/mod.rs`
- `crates/converter/src/cache.rs`
- `crates/converter/src/pipeline.rs`
- tests inside those files, or a new `crates/converter/tests/fixture_link_cache.rs`
- `docs/handoff/impl-559-p2-link-ingestion-cache.md`

No engine changes, no schema bump, no new dependencies.

# Done when
- Tests that fail on the old code and pass on yours:
  1. A cache hit links (on a filesystem that supports it) the blob and the `vfs` file instead of
     copying: the two share the file. Use the same "where the filesystem can" check as
     `link_or_copy_shares_the_file_where_the_filesystem_can`.
  2. A loose-file override written over a linked `vfs` entry leaves the previous output's blob and
     `vfs` bytes untouched.
  3. The existing corruption test (`reuses_verified_archive_blobs_and_recovers_from_corruption`)
     still passes: a corrupted blob is still detected and re-extracted.
- `cargo fmt --all -- --check`, `cargo clippy -p converter --all-targets -- -D warnings` and
  `cargo test -p converter` pass.

# Report
- Files changed and the approach in 4 lines.
- Every writer into `vfs` or `.ingestion-cache` you found, and how each now avoids writing
  through a link.
- Each test's name and what it proves.
