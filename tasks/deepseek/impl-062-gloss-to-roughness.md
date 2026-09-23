# impl-062 - Convert Skyrim's glossiness to roughness as an exponent, not a percentage

## Why

`docs/research/specular-gloss-mapping.md` (research-058) is the diagnosis; **read it first**, in
full. The short version: `crates/converter/src/material.rs:516` writes

```rust
roughnessFactor = 1 - (glossiness / 100).clamp(0, 1)
```

which reads Skyrim's Blinn-Phong **specular exponent** as a linear percentage. Glossiness 80 - what
Riverwood's timber, porch posts, thatch and barrels all carry - becomes `perceptual_roughness 0.2`,
the equivalent of an exponent of about 1,250, and every material at glossiness >= 100 (14,825 of
them) becomes 0.0, which Bevy clamps to a mirror. This is why every render the project has made
looks wet: `local/reference/rw1/_crop-gloss.png`.

## What to change

**One expression**, plus its tests and the cache plumbing.

```rust
// Skyrim's glossiness is the exponent of a Blinn-Phong specular lobe; Bevy's
// perceptual_roughness is sqrt of the GGX alpha, and alpha = sqrt(2 / (n + 2)).
perceptual_roughness = (2.0 / (glossiness + 2.0)).powf(0.25)
```

Monotone over the whole domain, `glossiness 0` gives 1.0, and it needs no clamp for any value in
the data (the largest research-058 found in a source NIF is 400). research-058 section 5.1 has the
derivation and a table of the values it produces; reproduce a few rows of that table as test cases.

Guard the domain rather than trusting it: a negative or non-finite glossiness must not produce a
NaN roughness. `material.rs:1182-1199` already rejects negatives; make sure whatever you write
cannot divide by zero or take a root of a negative, and test the edge.

**Do not change `metallicFactor`.** It is hardcoded 0.0 and that is correct for this engine
version and for wood, thatch, stone, cloth and hide. research-058 section 5.1 explains why setting
metalness before the environment-map binding exists would make Dwemer metal *darker*, not shinier.
Metalness is a later, separate task.

**Do not touch the specular/gloss mask.** That is the other half of the fix (research-058 section
5.2) and it is waiting on `websearch-060`, which is running. It will be its own task.

## Cache and schema

The number lives in the published `.glb`, so every mesh has to be rebuilt while textures, archives
and scripts are kept. The repository already has the idiom - follow it exactly:

- Bump `CONVERTER_SCHEMA_VERSION` from **17 to 18** in `crates/converter/src/cache.rs:11-19`.
- Add the migration arm beside the existing one at `cache.rs:61-74`, in the same shape: it keeps
  every non-`.glb` entry and drops the `.glb` ones. Read how `matches!(manifest.schema_version,
  12..=16) && CONVERTER_SCHEMA_VERSION == 17` is written and extend the range rather than
  inventing a second mechanism.
- `crates/converter/src/pipeline.rs` has the matching range for the configuration-hash rescue
  (`matches!(previous_manifest.schema_version, 12..=16)`); extend it the same way.
- `WORLD_DATABASE_SCHEMA_VERSION` does **not** change: no table changes.
- `crates/engine/src/world/lighting.rs`'s `converter_schema_version()` returns 17 and must return
  18. That one line in `crates/engine/` is yours; change nothing else there.

## Owned files

- `crates/converter/src/material.rs`
- `crates/converter/src/cache.rs`
- `crates/converter/src/pipeline.rs`
- `crates/engine/src/world/lighting.rs` - **only** `converter_schema_version()`
- `docs/handoff/impl-062-gloss-to-roughness.md` - your notes

## Tests to write

- The mapping's own table: glossiness 0 -> 1.0, and at least four of research-058's rows
  (5, 30, 80, 400) to within 1e-3. Update `material.rs:1448-1451`, which asserts 0.68 for
  glossiness 32; the new value is 0.4925 - compute it, do not copy that number on trust.
- Monotone: a higher glossiness always gives a strictly lower roughness, across a sweep.
- The result is finite and inside `(0, 1]` for every glossiness in a wide sweep including 0,
  negatives, very large values, infinity and NaN.
- The schema-18 migration keeps texture, archive and script entries and drops `.glb` entries, in
  whatever style `cache.rs`'s existing migration test is written.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p converter
cargo test -p engine
```

## Out of scope

Do not run the converter (the reconversion is the lead's, on the lead's GPU and disk), do not
touch `crates/engine/` beyond that one function, do not touch `vendor/`, and do not change the
`KHR_materials_specular` publication or the alpha/`COLOR_0` path - research-059 is a separate fix
in that area and the lead is handling it.
