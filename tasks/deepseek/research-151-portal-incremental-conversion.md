# research-151-portal-incremental-conversion - Rebuild only the models a converter change affects

## Why

When the converter changes what it publishes, `CONVERTER_SCHEMA_VERSION` goes up and the cache
migration (`crates/converter/src/cache.rs`, the `matches!(manifest.schema_version, 12..=18)` arm)
drops **every** `.glb` entry, so every model is rebuilt. On 2026-09-23 schema 19 rebuilt all 22,761
models (`local/conversion-s19-publish.out.log`: "Converted 22761, reused 220208 ... in 1001228 ms"),
about 17 minutes, blocking the demo and using ~67 GB of staging.

Some changes genuinely touch every model - materials are in nearly every `.glb`. But the history
suggests many do not. From `cache.rs`'s own comment and the commit log:

| schema | change | models it should have needed |
|---|---|---|
| 15 | vertex alpha is not blend, blend factors, editor markers | materials: most |
| 16 | door `Open`/`Close` animation clips | door NIFs only (~768, of which 171 animated) |
| 17 | animation scale channels written as VEC3 | animated NIFs only |
| 18 | glossiness to roughness | materials: nearly all |
| 19 | specular mask, environment cube colour space | materials with a normal map: nearly all |

If that table is right, schemas 16 and 17 rebuilt ~22,000 models to change a few hundred.

**This is research and design. Change nothing in `crates/`.**

## What to establish

1. **Where the time goes.** For a conversion, how long does each stage take - archive ingestion,
   NIF parse, geometry, material publication, animation export, textures, validation, the world
   database, publish? Use the logs under `local/conversion-*.log` and the code
   (`crates/converter/src/pipeline.rs`). If the logs lack timestamps, say so and estimate from
   what they do show, clearly labelled. The point is to know whether a material-only change could
   be made cheap without re-parsing NIFs at all.
2. **Check the table.** Confirm or correct the "models it should have needed" column from the code:
   which NIFs actually produce animation output, which glbs carry which features.
3. **The design.** Replace the single schema number, for `.glb` outputs, with something finer: for
   example a version per feature (geometry, materials, animation, ...) recorded in each cache entry,
   plus enough about each input (does this NIF have controller sequences? does it have materials of
   kind X?) to know which entries a given bump invalidates. Say exactly what goes in the cache entry
   and how the migration decides. Keep the whole-cache schema number for changes that really do
   touch everything.
4. **The cheap path for material changes.** If most of a model's conversion time is parsing and
   geometry, could a material-only change rewrite just the glTF JSON's `materials` of an existing
   `.glb` from cached material inputs, instead of reconverting? Say whether that is sound, what it
   would need, and what could go wrong.
5. **What it would have saved** for each of schemas 15-19, as a table: models rebuilt then, models
   rebuilt under the design, estimated minutes.
6. **Risk.** A cache that skips work it should have done ships stale models silently. Say how the
   design guarantees that cannot happen - for instance a verification mode that fully converts a
   random sample and compares hashes.

## Two tracks

This converter is shared with the upstream track (`docs/pr/two-tracks.md`): the Phase 2 track ports
converter fixes to upstream. Write the design so it could land upstream, and say which parts, if
any, depend on portal-only converter changes.

## Deliverable

`docs/research/incremental-conversion.md`. Lead with the answer to "would it have made schema 16
take minutes instead of 17?", then the stage timings, the design, the savings table and the risks.

## Owned files

- `docs/research/incremental-conversion.md`
- anything under `local/research-151/` for scratch

Do not run the converter or the engine.
