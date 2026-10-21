# impl-067 - A pruned dangling texture is not an incomplete conversion

## The bug, from a real run

Converting a stock Skyrim SE install at converter schema 18 ends with:

```
Converted 22761, reused 220208, skipped 511 in 1068213 ms (complete: false)
```

and the engine then refuses to start, because it requires `manifest.complete`
(`crates/engine/src/app.rs:1331-1335`). But the conversion is fine. What happened:

- 182 meshes reference textures that **do not exist anywhere in the game data** -
  `textures/actors/alduin/alduineyes_n`, `actors/bear/bearfurpass`,
  `_resourcepack/landscape/plants/mugopine01_d`, child-actor skins, Creation Club resource packs.
- The pipeline does the right thing with them: it **prunes the dangling reference and publishes
  the mesh anyway** (`crates/converter/src/pipeline.rs:300-330`). Verified on the run above - all
  86 distinct meshes named in `failures` are present in the published output.
- But the prune is recorded with `batch.record_skip(...)`, and `record_skip`
  (`pipeline.rs:757-758`) does `self.report.warnings.push(message); self.report.skipped += 1;`.
- `manifest.complete = report.skipped == 0 && report.warnings.is_empty();` (`pipeline.rs:381`),
  so one missing texture in the user's own game data marks the whole conversion incomplete, for
  ever, on every future run.

This is a real block: the lead had to set `complete` by hand in the manifest to run the demo.

## The distinction to draw

**"An input the game data does not contain" is not "an input we failed to convert."** The first is
a fact about the source that the user can do nothing about and that costs one texture reference on
one material. The second is a bug or a corrupt input and should stop a release.

So:

1. Give the pruned dangling references **their own category**, separate from skips and warnings.
   Add a counter to `PipelineReport` (`pipeline.rs:38-48`) - `pruned_texture_references: u64` -
   and record each one in its own map on the manifest, **not** in `manifest.failures`
   (`crates/converter/src/cache.rs`). Both are serialized, so pick names that will read clearly in
   a JSON someone is debugging, and keep `serde` defaults so an older manifest still deserializes.
2. Stop the prune path calling `record_skip`. It still has to be **loud**: keep the existing
   `eprintln!` per reference (`pipeline.rs:306-309`), keep the progress event, and add **one
   summary line** at the end of the run beside the `Converted ... skipped ...` line in
   `crates/converter/src/main.rs:96-104` saying how many references were pruned and that they are
   textures the game data does not contain. A user with a genuinely broken install must still see
   it; they just should not be blocked by it.
3. Leave `complete`'s definition alone - `skipped == 0 && warnings.is_empty()` is right once the
   prunes are out of both.

**Do not** weaken anything else: a failed archive, a mesh that will not convert, an integration
failure (`pipeline.rs:335-345`) must all still make the run incomplete. And do not touch
`crates/engine/` - the engine's gate is correct and stays as it is.

## Owned files

- `crates/converter/src/pipeline.rs`
- `crates/converter/src/cache.rs` - only the manifest field for the new map
- `crates/converter/src/main.rs` - only the summary line
- `docs/handoff/impl-067-complete-means-complete.md` - your notes

## Schema

`CONVERTER_SCHEMA_VERSION` is **18**. This changes no published asset - only the manifest's own
bookkeeping - so **do not bump it** and do not add a migration arm. The new manifest field must
deserialize from an existing schema-18 manifest that lacks it; test that.

## Tests to write

- A report with only pruned references is **complete**; the same report with one real skip, or one
  warning from another source, is **not**.
- The pruned references are counted and land in their own map, and `failures` does not grow.
- A manifest written before this change (no such field) loads, and the field defaults empty.
- Whatever you factor out of `record_skip` keeps its existing behaviour for its other callers -
  the archive failure path at `pipeline.rs:225-232` must still skip **and** warn.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p converter
cargo test -p engine
```

## Out of scope

Do not run the converter - the lead has the disk and the game data. Do not change the alpha,
material, glossiness or specular paths: `crates/converter/src/material.rs` is being edited by
another task right now. Do not change what gets pruned or when - only how it is recorded.
