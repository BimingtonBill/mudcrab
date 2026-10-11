# Goal
Track: Phase 1 (for upstream). A contributor can ask the converter "is this converted folder still
good?" without reconverting: `converter check <output> [--full]`. The fast mode takes seconds; `--full`
re-hashes every artifact. It prints "all good" or the list of problems, and its exit code says which.
The converter window gets a Check button later, so the check is a library function with a progress
callback; the CLI is a thin wrapper.

# Context
- Your checkout is cut from upstream `main` (6f583cd). Upstream commit style
  (`feat(converter): ...`), no `[deepseek]` prefix, no project-internal references or machine paths.
- `crates/converter/src/cache.rs`: `ConversionManifest` (`schema_version`, `complete`, `failures`,
  `entries: BTreeMap<String, CacheEntry>`), `CacheEntry { source_hash, output, output_size,
  output_hash }`, `hash_file`. Find where the manifest is written and read (`conversion-manifest.json`
  in the output root) and what `output` is relative to; reuse the existing reader, do not re-parse.
- `CONVERTER_SCHEMA_VERSION` is the schema the running converter writes.
- `crates/converter/src/main.rs`: hand-written `parse_cli` and `usage()`; today the positional form is
  `converter <data> <output> [options]`. Add `check` as a first-argument subcommand without changing
  the existing form.
- A reconversion already re-converts an artifact whose published file no longer matches its manifest
  entry (the cache-hit path re-hashes the published artifact). Confirm this in the pipeline; the check's
  advice ("run the converter again; only the listed files are redone") depends on it. If it is not true,
  say so in the report and do not print that advice.

# Scope
1. New module `crates/converter/src/check.rs`, exported from `lib.rs`:
   - `pub enum CheckMode { Quick, Full }`
   - `pub fn check_output(output: &Path, mode: CheckMode, progress: impl Fn(usize, usize) + Sync) -> Result<CheckReport>`
     (`progress(done, total)`, called from worker threads is fine).
   - `CheckReport`: files checked, bytes, elapsed, and a list of problems. Problem kinds: manifest
     missing or unreadable (an `Err`), schema version differs from `CONVERTER_SCHEMA_VERSION` (report,
     keep checking), manifest not `complete`, recorded conversion failures (count), artifact missing,
     wrong size, wrong hash (Full only), world database `skyrim_world.db` missing. `is_ok()`.
   - Quick: existence + size per entry (metadata only). Full: also `hash_file` against `output_hash`,
     in parallel with rayon (already a dependency).
   - Never write to the output folder, never create files, never follow paths outside it: an entry whose
     `output` is absolute or contains `..` is a problem, not a read.
2. CLI: `converter check <output> [--full]`. Print a one-line result and, on problems, up to 20 lines
   (`missing: meshes/x.glb`, `wrong size: ...`, `wrong hash: ...`) plus "and N more". Exit 0 when all
   good, 1 when problems were found, 2 when the manifest cannot be read. `--help` text and `usage()`
   list it.
3. Docs: a short "Checking a converted output" section in `docs/specs/converters/pipeline.md`.
4. Tests (`crates/converter/tests/output_check.rs`, new): convert the existing test fixture the way
   the other integration tests do (look at `tests/fixture_linked_extraction.rs`), then:
   all good in both modes; a deleted artifact is `missing`; a truncated one is `wrong size`; a same-size
   byte flip passes Quick and fails Full as `wrong hash`; a manifest with another schema version reports
   it; an entry with `../` is refused without reading outside; the check writes nothing (the tree's file
   list and modification times are unchanged). Plus CLI parse tests next to the existing ones.

Owned files: `crates/converter/src/check.rs` (new), `crates/converter/src/lib.rs`,
`crates/converter/src/main.rs`, `crates/converter/tests/output_check.rs` (new),
`docs/specs/converters/pipeline.md`, `docs/handoff/impl-827-p1-output-check.md`.
Read-only: everything else, especially `cache.rs` and `pipeline.rs` (ask in the report if they need a
change, e.g. a public manifest reader).

# Done when
Own target dir: `D:/Mudcrab/shared-check-target/impl-827`.
  `cargo fmt --all -- --check`
  `cargo clippy --workspace --all-targets --target-dir D:/Mudcrab/shared-check-target/impl-827 -- -D warnings`
  `cargo nextest run -p converter --target-dir D:/Mudcrab/shared-check-target/impl-827`

# Report
The change, the tests, the answer to the reconversion-repair question with its code reference, a
suggested commit message.
