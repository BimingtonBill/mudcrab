# impl-067 — a pruned dangling texture is not an incomplete conversion

**Task:** the lead's brief `impl-067-complete-means-complete` (pasted into the worker prompt; there is
no copy under `tasks/deepseek/` in this worktree).

**Acceptance run here:** `cargo fmt --all -- --check` (clean), `cargo clippy --workspace
--all-targets -- -D warnings` (finished, no lint; only the pre-existing `proc-macro-error2`
future-incompat note), `cargo test -p converter` (lib: **160 passed, 0 failed, 19 ignored**; every bin
and integration binary passes; doctests 0), `cargo test -p engine` (**296 passed, 0 failed, 3
ignored**; `world-inspect` 3 passed).

## What changed

| file | change |
| --- | --- |
| `crates/converter/src/pipeline.rs:49` | `PipelineReport::pruned_texture_references: u64`, `#[serde(default)]`, next to `skipped`. |
| `crates/converter/src/pipeline.rs:62` | `fn conversion_is_complete(&report) -> bool` — the **same** expression as before, `skipped == 0 && warnings.is_empty()`, given a name so the completeness rule can be tested directly. Used once, at `pipeline.rs:394`. |
| `crates/converter/src/pipeline.rs:334` | the prune loop calls the new `record_pruned_texture_reference` instead of `record_skip`. The `eprintln!` per reference and the progress event are untouched; `resolve_asset_uri` still resolves the URI to its staging-relative path, which is now the value recorded instead of a `failures` key. |
| `crates/converter/src/pipeline.rs:779` | `ConversionBatch::record_pruned_texture_reference` — progress event, `report.pruned_texture_references += 1`, and `manifest.pruned_texture_references[glb].push(texture)`. No warning, no skip, no `failures` entry. |
| `crates/converter/src/cache.rs:49` | `ConversionManifest::pruned_texture_references: BTreeMap<String, Vec<String>>`, `#[serde(default)]`. |
| `crates/converter/src/main.rs:99` | one summary line after the `Converted … skipped … (complete: …)` line, only when the count is non-zero. |

`record_skip` itself is unchanged, as are its three other callers and the archive-failure path
(`pipeline.rs:240-248`), which still skips **and** warns. `crates/engine/` is untouched,
`CONVERTER_SCHEMA_VERSION` stays 18, and no migration arm was added.

## The manifest shape

```json
"failures": {},
"pruned_texture_references": {
  "meshes/actors/bear/bearfurfemale.glb": [
    "textures/actors/bear/bearfurpass.ktx2"
  ]
},
```

Keyed by the published mesh, valued by the staging-relative texture path it dropped, so both
questions a debugger has ("which mesh did this happen to", "which texture is missing") are answered
by the object itself. The values are the resolved paths, the same shape as `failures` keys and
`entries` keys; the raw glTF URI (`../textures/…`) is what appears in the stderr line. A schema-18
manifest without the field loads and defaults empty (`#[serde(default)]`), asserted by
`cache::tests::manifests_written_before_pruned_reference_tracking_still_load`, which parses a
hand-written JSON manifest that has never seen the field.

The runtime gate cannot see the new key either: `RuntimeManifest`
(`crates/engine/src/app.rs:1302-1306`) deserializes only `schema_version` and `complete`, and it does
not deny unknown fields.

## Tests (all in `crates/converter/src/pipeline.rs`, `mod tests`)

- `pruned_texture_references_do_not_make_a_run_incomplete` — a report with only prunes (182) is
  complete; one `skipped` is not; one warning from another source (the integration-failure message
  the pipeline itself pushes at `pipeline.rs:351`) is not.
- `publishes_meshes_with_missing_textures_and_stays_complete` — end to end: a generated NIF whose
  normal map does not exist converts, is published, `skipped == 0`, `warnings` empty,
  `report.complete`, `pruned_texture_references == 1`, `manifest.failures` empty, the manifest map
  holds `meshes/missing_normal.glb → ["textures/absent_n.ktx2"]`, the same shape is asserted on the
  **serialized JSON** rather than the struct, and the published GLB no longer names the missing
  texture.
- `failed_archives_still_skip_and_warn` — a garbage `.bsa` still gives `skipped == 1`,
  `warnings.len() == 1`, `complete == false`, one `failures` entry and an empty
  `pruned_texture_references`.
- `cache::tests::manifests_written_before_pruned_reference_tracking_still_load` (see above).

**Negative control.** Before settling, I temporarily made `record_pruned_texture_reference` also push
a warning and bump `skipped` — the old `record_skip` behaviour — and re-ran
`publishes_meshes_with_missing_textures_and_stays_complete`: it failed at
`assert_eq!(report.skipped, 0)` with `left: 1, right: 0`, and the stderr line named
`../textures/absent_n.ktx2` in `meshes/missing_normal.glb`. So the test really does exercise the
prune path and really does catch the reported bug. The temporary lines were removed; the final diff
has no trace of them (`git diff` checked).

## Three choices worth the lead's eye

1. **The progress event still says `"Asset skipped"`.** The brief said to keep the progress event, so
   I kept it byte-for-byte (same stage, counters and file). It is arguably mislabelled now that the
   reference is neither counted as a skip nor written to `failures`; changing it would be a
   one-line edit if the lead prefers `"Texture reference pruned"`.
2. **The summary line is printed only when the count is non-zero.** A clean run stays one line; a run
   with prunes gets both. If the lead wants the count in every log for greppability, drop the `if`.
3. **The counter and the map are both updated in one place**, so the map's total always equals
   `report.pruned_texture_references` (one element per reference; two meshes referencing the same
   missing texture produce two entries under two keys).

## What the lead has to do next

- **Re-run the conversion** (a normal run, not `--invalidate-cache`) to have the manifest rewritten
  with `complete: true` — the engine's gate (`app.rs:1331-1335`) is unchanged, so the hand-edited
  `complete` is still the only thing letting the current output directory start. No published asset
  changes: the prune itself is untouched, GLBs are byte-identical, and no schema bump is involved.
- **The 511 skips of the reported run may not all have been prunes.** This fix makes the run complete
  only for skips that were pruned references; anything else that skipped (a failed archive, a NIF that
  will not convert) still lands in `failures`/`warnings` and still makes the run incomplete, which is
  the intended boundary. The new summary line says how many of the 511 were prunes; the rest are
  named in `conversion-manifest.json`'s `failures`.
- **Expect the line to reappear on every run** for the affected meshes. Inferred from the code, not
  observed: staging is rebuilt per run (`pipeline.rs:1106`) and a cache hit is only taken when the
  *published* file's hash equals the recorded `output_hash` (`pipeline.rs:549-556`), which for a
  pruned mesh was computed before the rewrite — so those meshes are reconverted and re-pruned each
  time, which is exactly why the bug was permanent.

## Not verified here

- No conversion of real game data was run and no real manifest was rewritten (out of scope): the
  182 meshes and 511 skips are the brief's numbers. The end-to-end proof above uses
  `dummy-content` fixtures only.
- Nothing in `crates/engine/` was built or run beyond `cargo test -p engine`.
- `crates/converter/src/material.rs` was not touched and not reviewed; the dangling URIs it publishes
  are taken as given.
