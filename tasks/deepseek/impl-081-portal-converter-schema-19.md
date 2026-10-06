# impl-078 - Converter schema 19: the specular mask, the pruned-texture index bug, and the cube's colour space

> **Status: blocked for a DeepSeek coder, ready for Claude to apply.** `lead-070-converter-materials.1`
> wrote this brief but the harness refused it twice: "a lead may not give a coder a protected file
> (crates/converter/src/material.rs matches crates/\*\*)", and `.deepseek-agents.json`'s `denyEdit`
> lists `crates/**`. The policy is not worked around here. Everything below is the implementation
> spec; the reviewer (`lead-070`) has read the impl-064 diff and the code sites it names.

## Goal

Land four converter changes in ONE patch behind ONE schema bump (18 -> 19), because every converter
change costs an 18-minute reconversion and the batch must be converted once:

1. **The specular mask** - the finished, reviewed change in the worktree
   `local/impl/impl-064-specular-mask-from-normal-alpha` (base commit `59f1f20`), re-applied onto this
   checkout.
2. **The pruned-texture index bug** in `mesh.rs`, which the impl-064 author found and could not fix
   from their owned files.
3. **The environment cube's colour space** - the label, the URI and the file must agree
   (research-077's recommendation).
4. **`CONVERTER_SCHEMA_VERSION` 18 -> 19** with both migration arms widened.

Do not use any tools outside the repo except reading; do not run the converter, the engine, or any
demo. `cargo test`/`clippy`/`fmt` are expected and required.

## Context

- Repo `C:\Modding\MudcrabProject`; the crate is `crates/converter/`. The published tree at
  `C:/Modding/SkyrimConverted` is schema 18 and **complete**, so anything that changes published
  output must bump the schema; this patch does and takes the bump.
- `CONVERTER_SCHEMA_VERSION` is at `crates/converter/src/cache.rs:11`. There are **two** places that
  treat the recent schemas as reusable-with-rebuild, and both must move together:
  `crates/converter/src/cache.rs:67` and `crates/converter/src/pipeline.rs:92`.
- The specular mask exists as a reviewed worktree. Read
  `local/impl/impl-064-specular-mask-from-normal-alpha/docs/handoff/impl-064-specular-mask-from-normal-alpha.md`
  first: it is the design record (why the factor is doubled, why model-space normals take no mask,
  what the tests assert). Then get the patch from that worktree
  (`git -C local/impl/impl-064-specular-mask-from-normal-alpha diff 59f1f20 -- crates/converter/src/material.rs`,
  or `git show` its commit) and apply it with git so line endings are normalised: **the worktree's
  files are LF and this checkout's are CRLF**, and the lead verified that material.rs is otherwise
  byte-identical to this checkout, so a wholesale file copy is the one thing not to do. Its six new
  tests come with it.
- The engine reads `KHR_materials_specular` (`specularFactor` -> `reflectance = factor * 0.5`, and
  `specularTexture` -> an extra `reflectance *= sample.a * 0.5`), which is why a masked material
  publishes `2 * strength` and an unmasked one publishes `strength`.

## What to build

### 1. Apply the impl-064 specular-mask change

Exactly as the worktree has it: `SLSF1_MODEL_SPACE_NORMALS` (bit 12) suppresses the mask; the normal
map's own texture index becomes `KHR_materials_specular.specularTexture`; `specularFactor` is
`2 * clamp(strength, 0, 1)` **iff** a mask is published, otherwise `clamp(strength, 0, 1)`; the
extension is published when `strength > 0 || slot 7 || mask`. Keep its tests.

### 2. Fix `prune_document_images` (`crates/converter/src/mesh.rs:313-400`)

`remap_texture_info` remaps `normalTexture`, `occlusionTexture`, `emissiveTexture`,
`baseColorTexture`, `metallicRoughnessTexture` and `OPEN_SKYRIM_material.textureSlots`, but nothing
under `extensions.KHR_materials_specular`. A material whose normal map's KTX2 is missing on disk
keeps a stale `specularTexture` index after the other indices shift. Fix:

- Remap `KHR_materials_specular.specularTexture` and `.specularColorTexture` through the same
  `texture_remap` (use `remap_texture_info`, which removes the key when its target is gone).
- **And keep the doubling invariant:** if `specularTexture` was present and is dropped by this
  remap, halve `specularFactor` (the published factor is `2 * strength` only *because* a mask
  follows it; a factor of 2.0 with no mask makes Bevy use `reflectance = 1.0`). Only halve when a
  `specularTexture` was actually dropped - a material that never had one keeps its factor.
- Tests: a document whose specular extension points at a texture that is pruned gets its index
  remapped when another texture survives, and gets the entry dropped **and** the factor halved when
  its own texture is the one removed; a material with no `specularTexture` keeps its factor.

### 3. The environment cube's colour space (`crates/converter/src/material.rs:680-702`)

The published KTX2 is encoded with the sRGB transfer function (`texture.rs:44-53` counts
`EnvironmentCube` in its colour set, `:927-929` sets the flag), but the same slot is labelled
`"colorSpace":"linear"` and registered with `is_srgb = false` in `publish_skyrim_extension`. Make
label, URI and file agree:

- In the slot loop, `is_srgb` for `registry.texture(...)` becomes
  `matches!(slot.semantic, NifTextureSemantic::Detail | NifTextureSemantic::EnvironmentCube)`.
- `"colorSpace"` becomes `"srgb"` for those same two semantics, `"linear"` otherwise.
- Test: a material with an `environment_cube` slot publishes `"colorSpace":"srgb"` and a URI ending
  `.opensky-srgb.ktx2`; a `Detail` slot still does; an `inner_layer` stays linear. (The alias file
  itself is published by `pipeline.rs:902-925`; nothing new is needed for it.)
- Do **not** touch `texture.rs`'s semantic sets: the file is already sRGB; only the label was wrong.

### 4. The schema bump

- `CONVERTER_SCHEMA_VERSION: u32 = 19` (`cache.rs:11`).
- `cache.rs:67`: `matches!(manifest.schema_version, 12..=18) && CONVERTER_SCHEMA_VERSION == 19`, and
  extend the comment with what 19 changes (the specular mask; the environment cube's colour space).
- `pipeline.rs:92`: `matches!(previous_manifest.schema_version, 12..=18)`.
- `cache.rs` tests: the migration test's loop `[12, 13, 14, 15, 16, 17]` gains 18. The test
  `manifests_written_before_pruned_reference_tracking_still_load` writes a `"schema_version": 18`
  fixture and asserts the manifest stays `complete`; with 18 now inside the migration arm that
  assertion is wrong. Keep the test's purpose (a manifest with old keys still deserializes) by
  moving the fixture to schema 19, and make sure the migration arm itself is still covered by the
  loop test.

## Owned files

- `crates/converter/src/material.rs`
- `crates/converter/src/mesh.rs`
- `crates/converter/src/cache.rs`
- `crates/converter/src/pipeline.rs`

Nothing else may change. `crates/engine/`, `crates/shared/`, `vendor/` and `docs/` are out of
scope - if something there needs a change, say so in your report instead of making it.

## Acceptance

Acceptance: cargo test -p converter

Also run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test -p engine` yourself and report their output: the harness only auto-runs the one
Acceptance line above, and all four are part of "done" for this project.

Report the exact counts of passed/failed/ignored tests you saw, and every command you ran.

## Report

- What changed, per file, with `file:line`.
- The impl-064 patch: did `git apply` take it cleanly, or what did you have to resolve?
- The prune fix: which behaviour you chose for a dropped `specularTexture` and the test that pins it.
- The acceptance output (counts, and the exact commands).
- Anything you could not do, anything you are unsure about, and any place where the two research
  reports this batch rests on disagree (the lead has them).
