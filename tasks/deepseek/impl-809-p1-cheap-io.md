# Goal
Track: Phase 1 (for upstream). Remove avoidable I/O from the converter's hot paths without changing
what it publishes: (1) no per-file durable flush for rebuildable intermediate files, (2) no second
hash of an extracted source the extractor already hashed, (3) output hashing moved off the single
result-consumer task, (4) plugins parsed and merged once per run. One commit per item, each with a
test, in upstream style.

# Context
- Your checkout is cut from `phase1/vfs-hard-links` (upstream main + hard links between `vfs/` and
  `.ingestion-cache/`). Upstream commit style (`perf(converter): ...`), no `[deepseek]` prefix, no
  project-internal references in code, comments, docs or messages.
- An external source review, confirmed by the lead against this code:
  1. `crates/converter/src/archive/mod.rs` `atomic_write` calls `file.sync_all()` for every
     extracted file (~173,000 for Skyrim SE), and `crates/converter/src/mesh.rs` (`write_glb_atomic`
     or near it) does the same for every GLB. These files live in the staging folder, which is
     rebuilt or re-validated if a run dies; the publish step is a directory rename. Keep
     temp-file + rename (atomic against a process crash); drop the per-file `sync_all` for these
     staging files only. Keep `sync_all` where a file is the durable record (the manifest, reports,
     the world database, the cell cache: `cache.rs`, `main.rs`, `esm/cell_cache.rs`). Say in the
     doc comment what is and is not guaranteed after a power loss.
  2. Extraction computes `hash_bytes(&data)` for every entry (`ExtractedFile.sha256`, and the
     cache-hit path knows each file's hash from the manifest), but `ConversionBatch::convert_kind`
     in `pipeline.rs` calls `hash_file(&source)` again on each extracted source. Carry the known
     hashes (path -> hash, from the extraction outcomes) into the batch and use them; loose files
     overlaid from Data (`overlay_loose_assets`) still get hashed. Mesh dependency files hashed per
     referencing mesh: memoise per run.
  3. After a worker finishes an asset, the async result consumer calls `hash_file(&target)` before
     taking the next result. Compute the output hash in the worker (rayon) thread and send it with
     the outcome, so the consumer only records.
  4. `merge_plugins` runs twice: inside `convert_plugins` (esm/mod.rs) and again in
     `pipeline.rs` for the cell cache. Merge once and pass the result to both.
- Do not touch the texture encoder output path in `texture.rs` (another branch rewrites it), the
  progress events, `staging_path`/`publish_directory`, or the manifest schema.

# Measure
Before and after item 1, time the extraction of ONE real archive with a small `#[ignore]`d test or
an example you run by hand (e.g. `Skyrim - Textures0.bsa` or the largest `.bsa` under
`C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data`, read-only) into a temp dir under
`D:/Mudcrab/phase1-scratch/` (create it; delete what you write), three runs each way,
reporting files, bytes and seconds. Do not commit the numbers into code; put them in the report.

Owned files: `crates/converter/src/archive/mod.rs`, `crates/converter/src/mesh.rs` (the GLB write
only), `crates/converter/src/pipeline.rs` (hashing and plugin merge only),
`crates/converter/src/esm/mod.rs`, `crates/converter/tests/` (new files only),
`docs/specs/converters/pipeline.md`, `docs/handoff/impl-809-p1-cheap-io.md`. If you need another
file, say so in the report instead of editing it.

# Done when
  `cargo fmt --all -- --check`
  `cargo clippy -p converter --all-targets --target-dir D:/Mudcrab/shared-check-target -- -D warnings`
  `cargo nextest run -p converter --target-dir D:/Mudcrab/shared-check-target`
Tests: an extracted source's recorded hash equals `hash_file` of it (so reuse is sound) and
`convert_kind` does not re-read it (count reads or inject the hasher); the consumer records the
worker's hash; one merge per run (count calls); outputs and manifest identical to before on the
fixture conversion (compare the published manifest's hashes).

# Report
Per item: the change, the test, and the measured before/after for item 1. A suggested commit
message per item. Anything you chose not to do and why.
