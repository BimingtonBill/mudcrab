# impl-553 — publish effect-shader materials' source texture as the emissive

Branch `phase2/material-publication` (converter schema 15). Track: Phase 2, a fix for upstream.
No schema bump, no engine change, no new dependency.

## The change, in three lines

1. `publish_material` (`crates/converter/src/material.rs`) now picks the emissive texture by
   family: an effect shader gets its **source texture** (the `Diffuse` slot), a lighting shader
   keeps the `Glow` slot it has always used.
2. The same function zeroes `pbrMetallicRoughness.baseColorFactor`'s RGB for an effect material
   **that has a source texture**, keeping its alpha (the blend coverage the published
   `SRC_ALPHA` factors multiply), so the textured emissive carries the whole look — an effect
   shader is unlit.
3. `publish_emissive` is unchanged in behaviour; its `glow` parameter is renamed
   `emissive_texture` and documented, because it now receives either slot.

`emissive_color` / `emissive_multiple` keep their names and stay the record of the effect's tint
(`base_color.rgb`) and its brightness (`base_color_scale`); the comment at the construction site in
`build_effect_material` says so, and says why the names are shared with the lighting family.
`publish_emissive` already clamps the tint to `0..1` and writes
`KHR_materials_emissive_strength` only above 1, so those two rules are unchanged.

Result for `Effects\FXfireWithEmbers01.nif` (research-550's hearth fire): `emissiveTexture` = the
sRGB alias of `fxfireatlas04.ktx2` — the same registry entry as `baseColorTexture`, so the same
image index and one `images[]` entry — `emissiveFactor [1,1,1]`, strength 1.6,
`baseColorFactor [0,0,0,1]`, `BLEND`, blend factors and the `OPEN_SKYRIM_material` extension
(including `shaderFamily: "effect"`) untouched.

## What did not change

- **Lighting-shader materials.** Neither new branch can be reached for one: the emissive slot is
  chosen by family (a lighting shader keeps `Glow`), and the albedo zeroing also requires the
  family to be `Effect`. Glow map, emissive colour, strength and base colour are what they were.
- **An effect shader with no source texture** (`Effects\TESTCandleFlame01.nif`): unchanged, as
  required. It still publishes `baseColorFactor [1,1,1,1]`, no base colour texture,
  `emissiveFactor [1,1,1]`, no strength extension and no `OPEN_SKYRIM_material` extension (the
  shape is `OPAQUE`), so it remains what research-550 describes: an opaque, self-lit white
  cylinder card — the tint it does publish is 1,1,1 and it still has no texture to attach. Its
  `COLOR_0` vertex alpha, which is what would make the card read as a flame, is still not
  exported; that is a separate task.
- **`ATTR`/bounds/texture pruning in `mesh.rs`**: `emissiveTexture` was already remapped and
  dependency-tracked there (`texture_dependencies` maps it to `TextureSemantic::Emissive`), and the
  effect emissive texture is the same registry entry as the base colour one, so no change was
  needed in `mesh.rs`.

## Notes for the reviewer

- **The strength extension is kept, by instruction.** research-550 proposed instead folding
  `base_color_scale` into `emissiveFactor` and dropping the strength, so that the fork engine's
  `is_deliberate_glow` -> `EMISSIVE_EXPOSURE = 100` (`crates/engine/src/render.rs:766`,
  `crates/engine/src/lights.rs:212`) would stop firing. This change keeps the strength, as the
  brief specifies, so on the **fork** engine an effect with a scale above 1 still has its emissive
  multiplied by 100 — the difference being that the emissive is now the flame texture rather than
  a flat colour. Upstream has no exposure step, where the published value is used as-is.
- The published `Greyscale` slot remains read by nothing in the engine (research-550's risk note);
  the effect's colour ramp still comes from the texture alone.
- `docs/specs/converters/nif-to-gltf.md:78` still describes the emissive as "Glow map (Slot 2) or
  emissive color/strength"; it does not mention the effect source texture and was left alone
  (outside this task's owned files).

## Tests

In `crates/converter/src/material.rs` (`mod tests`):

- `publishes_the_effect_source_texture_as_a_tinted_unlit_emissive` — failed on the old code.
- `keeps_the_effect_albedo_alpha_which_is_the_blend_coverage` — failed on the old code.
- `publishes_a_lighting_glow_map_as_before` — passes on both (regression guard).
- `leaves_an_untextured_effect_material_as_it_was` — passes on both (regression guard).

Verified by temporarily reverting the family condition: only the first two fail. They fail
because the old code publishes no `emissiveTexture` for an effect and leaves the albedo RGB at
the tint.

## Commands

```
cargo test -p converter            # 118 lib tests (7 ignored) + 18 fixture/integration tests pass
cargo fmt --all -- --check         # clean
cargo clippy -p converter --all-targets -- -D warnings   # clean
```
