# Goal
Track: Phase 1 (for upstream). Step 1 of moving the converter's scratch data out of its published
output: the content-addressed ingestion cache lives in a folder beside the output
(`<output>.cache/` by default, `--cache-dir <path>` to choose), not inside it, so the published
folder no longer carries it and a reconversion reuses it in place instead of re-linking or copying
156,586 blobs out of the previous output. `vfs/` stays for now (step 2 removes it).

# Context
- Your checkout is cut from `phase1/vfs-hard-links` (upstream main + `cache::link_or_copy` hard
  links between `vfs/` files and cache blobs). Upstream commit style (`feat(converter): ...`), no
  `[deepseek]` prefix, no project-internal references in code, comments, docs or messages.
- Design (read it first, sections 1, 5 and 7 step 1):
  `C:/Modding/Mudcrab/mudcrab/local/research/phase1-cache-out-of-output.md` (outside your
  checkout; open it with the Read tool). Its citations are for this code.
- Today the cache root is `<staging>/.ingestion-cache` and the previous one
  `<output>/.ingestion-cache` (`pipeline.rs` around :181-182); publish renames staging over the
  output, so the cache moves with it.

# Scope
1. `PipelineConfig.cache_dir: Option<PathBuf>` and `--cache-dir <path>`; default
   `<output>.cache` (the output folder's full name plus `.cache`, a sibling: mind dotted names, do
   not use `with_extension`). Validate it is not inside the output or the staging folder.
2. Extraction and cache hits use that folder as both the cache root and the previous cache root.
   Blobs are immutable and content-addressed: write a new blob as temp + rename so two runs sharing
   the cache cannot see a half-written one; never write through an existing blob.
3. Migration: on a run whose cache folder does not exist but whose previous output holds
   `.ingestion-cache/`, move that folder to the cache location once (rename on the same volume, else
   `link_or_copy` each file), before anything reads it. A published output no longer contains
   `.ingestion-cache/`.
4. The manifest's archive records keep working (no converter schema bump: no artifact byte changes).
   `--resume-staging` keeps working.
5. Docs: `docs/specs/converters/pipeline.md` (layout, the flag, the migration) and `--help`.
6. Tests: a first run creates `<output>.cache` and publishes no `.ingestion-cache/` inside the
   output; a second run is a cache hit reading blobs from the sibling (no copy: the `vfs` file and
   the blob are one file where the filesystem links); `--cache-dir` elsewhere works; the one-time
   migration from an old-layout output (hits, and the old folder is gone); a dotted output name;
   a cache dir inside the output is rejected. Update `crates/converter/tests/fixture_linked_extraction.rs`
   to the new blob location.

Owned files: `crates/converter/src/config.rs`, `crates/converter/src/main.rs`,
`crates/converter/src/pipeline.rs` (cache roots, migration, tests only),
`crates/converter/src/archive/mod.rs`, `crates/converter/src/cache.rs`,
`crates/converter/tests/` , `docs/specs/converters/pipeline.md`,
`docs/handoff/impl-811-p1-cache-dir.md`. If you need another file (for example the
`asset-closure` bin or `scripts/`), say so in the report instead of editing it.

# Done when
  `cargo fmt --all -- --check`
  `cargo clippy -p converter --all-targets --target-dir D:/Mudcrab/shared-check-target -- -D warnings`
  `cargo nextest run -p converter --target-dir D:/Mudcrab/shared-check-target`

# Report
Per scope point: the change and the tests (which failed before). Every other reader of
`.ingestion-cache` you found and its verdict. A suggested commit message.
