# impl-062 — glossiness becomes a Blinn-Phong exponent, not a percentage

**Task:** the lead's brief `impl-062-gloss-to-roughness` (no copy under `tasks/deepseek/` in this
worktree).

**Acceptance run here:** `cargo fmt --all -- --check` (clean), `cargo clippy --workspace
--all-targets -- -D warnings` (finished with no lint; only the pre-existing `proc-macro-error2`
future-incompat note), `cargo test -p converter` (156 passed, 0 failed in the lib, 19 ignored; all
integration binaries and doctests pass), `cargo test -p engine` (279 passed, 0 failed; 3 in
`world-inspect`).

## The mapping

```rust
fn perceptual_roughness(glossiness: f32) -> f32 {
    if !glossiness.is_finite() || glossiness < 0.0 {
        return 1.0;
    }
    (2.0 / (glossiness + 2.0)).powf(0.25)
}
```

`material.rs:513`, used once, at `material.rs:534`, for `pbrMetallicRoughness.roughnessFactor`. The
derivation in the doc comment is checkable in this checkout: Bevy's `perceptualRoughnessToRoughness`
(`bevy_pbr-0.19.0/src/render/pbr_lighting.wgsl:548-554`) returns `perceptual_roughness²`, so the
material publishes `sqrt(sqrt(2 / (n + 2)))`. It is the same fourth root the brief specifies.

| glossiness | old `1 - g/100` | new `(2/(g+2))^(1/4)` |
| ---: | ---: | ---: |
| 0 | 1.0 | 1.0 |
| 5 | 0.95 | 0.731110 |
| 30 | 0.70 | 0.5 (exactly) |
| 32 (the test fixture) | 0.68 | 0.492479 |
| 80 (Riverwood's timber, thatch, barrels) | 0.20 | 0.395188 |
| 100 | 0.0 | 0.374203 |
| 400 (largest exponent research-058 found) | 0.0 | 0.265583 |

Two consequences the lead may want for the visual check. **The old mapping never produced a literal
mirror**: Bevy clamps perceptual roughness up to 0.089 (`pbr_lighting.wgsl:552`), so every material
at glossiness >= 100 was rendered at that floor — a hard, tight highlight rather than glass. The new
mapping stays above the floor for every exponent below about 31,874, so no value in the source data
is clamped at all.

`metallicFactor` is untouched (still the literal `0.0` at `material.rs:533`), as is the
`KHR_materials_specular` publication, the alpha/`COLOR_0` path, and the `WORLD_DATABASE_SCHEMA_VERSION`.

## Cache and schema

- `crates/converter/src/cache.rs:11` — `CONVERTER_SCHEMA_VERSION` 17 → 18.
- `cache.rs:61` — the existing migration arm extended in place, not duplicated:
  `matches!(manifest.schema_version, 12..=17) && CONVERTER_SCHEMA_VERSION == 18`, comment updated
  with what 18 changes. The body is unchanged, so a schema-12..17 manifest keeps its texture, script
  and archive entries and drops every `.glb`.
- `crates/converter/src/pipeline.rs:77` — the configuration-hash rescue range widened to `12..=17`.
- `crates/engine/src/app.rs:1350` — `converter_schema_version()` 17 → 18.

## Tests added (all in `crates/converter/src/material.rs`, `mod tests`)

- `maps_glossiness_to_roughness_as_a_blinn_phong_exponent` — the brief's rows 0, 5, 30, 80, 400 plus
  100, each to 1e-3. The expected values are **recomputed**, not copied: I could not read
  `docs/research/specular-gloss-mapping.md` (see below), so I evaluated `(2/(n+2))^(1/4)` in f32 with
  a scratch script (`local/scratch/gloss_table.py`, git-ignored) and cross-checked one independent
  number from the brief: glossiness 32 gives 0.49247906 against the brief's 0.4925.
- `roughness_decreases_strictly_with_glossiness` — 20 exponents from 0 to `f32::MAX`; each step must
  be strictly lower.
- `roughness_stays_finite_and_inside_the_unit_interval` — 16 inputs including 0, `-0.0`, 1e-6,
  `f32::MAX`, `-1e30`, both infinities and NaN; every result finite and in `(0, 1]`.
- `materials_without_a_glossiness_field_are_fully_rough` — glossiness 0 is exactly 1.0, which is what
  `build_effect_material` publishes (`material.rs:995`): a `BSEffectShaderProperty` has no glossiness
  field, so it keeps the roughness the old mapping gave it. No effect material changes behaviour.
- `cache::tests::recent_schema_migrations_reuse_only_unchanged_asset_kinds` — the existing test, its
  schema list extended with 17 and an archive entry added so the "keeps archives" half of the brief is
  asserted too (textures and scripts were already covered).
- The existing assertion at `material.rs:1566` now expects 0.4924791 for the fixture's glossiness 32
  instead of 0.68.

## Two things that did not match the brief

1. **`converter_schema_version()` is not in `crates/engine/src/world/lighting.rs`.** It is a private
   `const fn` in `crates/engine/src/app.rs:1347-1351`, and `grep` finds no other definition anywhere
   in the crate. `lighting.rs` has no schema constant at all (its schema-16 mentions are prose about
   the database). I changed the one line in `app.rs` and nothing else in `crates/engine/`; if the
   lead meant a second constant somewhere, it does not exist.
2. **`docs/research/specular-gloss-mapping.md` is not in this worktree and cannot be read from
   here.** The brief says to read it first, but it is absent from the base commit `ffdbc54` and from
   its history (`git log --all -- <path>` is empty), and reading outside the working directories is
   blocked for this session, so the lead's copy is out of reach. Everything I implemented comes from
   the brief's own text — the expression, the rows, the guard requirement, the migration shape — and
   every number I asserted was recomputed from the formula rather than taken on trust. The doc's
   section 5.1 derivation is therefore unread here; if it argues for a different exponent, this
   implementation follows the brief, not the paper.

## What the lead has to do next

- **Reconvert.** 18 > 17, so the migration forces every `.glb` to rebuild while archive ingestion,
  textures and scripts are reused. The world database and cell cache are not affected
  (`WORLD_DATABASE_SCHEMA_VERSION` still 5, no table changed).
- **Until that reconversion lands, the engine will refuse to start on the assets on disk.**
  `validate_runtime_assets` (`app.rs:1329`) requires the manifest's version to equal the constant, and
  the published manifest is schema 17 — the failure is the existing "asset conversion is incomplete or
  stale; reconvert assets with converter schema 18" line, not a new one. Expected, but it is a hard
  gate, not a warning.
- The visual holdout is the lead's: the twelve `local/reference/rw1` Riverwood poses against the UESP
  references. Nothing here was rendered, and no converted asset was written.

## Not verified here

- No conversion was run and no `.glb` was inspected (out of scope), so the published numbers are
  proven by unit tests over the mapping, not by reading a converted material — the old `0.2` and the
  new `0.395188` for glossiness 80 exist only as test expectations until the reconversion.
- The distribution claims from research-058 (14,825 materials at glossiness >= 100, maximum exponent
  400) are the brief's, unverified here; the mapping's domain guard is written to hold beyond them
  anyway (`f32::MAX` maps to 2.8e-10, still finite and positive).
