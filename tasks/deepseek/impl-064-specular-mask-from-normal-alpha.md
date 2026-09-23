# impl-064 - Publish Skyrim's specular mask (the normal map's alpha)

## Why

Read `docs/research/specular-gloss-mapping.md` section 5.2 and
`docs/research/gloss-mask-semantics.md` in full before starting.

Skyrim multiplies its specular term by a mask stored in the **alpha channel of the normal map**.
The converter drops it. On Riverwood's porch posts and barrels that alpha is 0.06-0.14, so the
engine draws those surfaces with up to ten times more specular than the game does - the difference
between weathered timber and varnished timber (`local/reference/rw1/_crop-gloss.png`).

websearch-060 settled the semantics with high confidence: the mask scales the specular
**intensity**, never the exponent. So it can be published as a glTF
`KHR_materials_specular.specularTexture` pointing at the normal map's own texture index - glTF
samples that texture's alpha, which is exactly the mask. **No texture is derived, decoded or
re-encoded**; the KTX2s already carry the alpha.

This is the second half of a fix whose first half (`impl-062`, the glossiness-to-roughness
mapping) is already integrated. Do not redo it.

## What to change, in `crates/converter/src/material.rs`

Where `KHR_materials_specular` is published (research-058 puts it at `material.rs:635-648`):

1. Add `specularTexture` pointing at **the same texture index the material already uses for its
   normal map**, when the material has a normal map.
2. Bevy applies `reflectance *= sample.a * 0.5` (`bevy_pbr`'s
   `khr_materials_specular.rs` and `pbr_fragment.wgsl`), so publish `specularFactor` as
   **`2 * specular_strength`** to cancel that 0.5. research-058 section 5.2 cites Bevy's own
   comment documenting this workaround. Note in a code comment that 2.0 is outside the glTF
   spec's `[0, 1]` range for that field and is only safe because this engine is the sole consumer
   of these files - somebody will otherwise "fix" it later.
3. The extension is currently published only when `specular_strength > 0.0` or a slot-7 texture
   exists. A material with a normal map and a mask now has a reason to publish it too; widen the
   condition accordingly, and make sure a material with **no** normal map is completely unchanged.

**Branch on model-space normals.** `gloss-mask-semantics.md` records that model-space-normal
meshes take the mask from **texture slot 7's R channel**, not from the normal map's alpha. Find
how that flag is read in the converter (the NifTools name is `Model_Space_Normals`, and slot 7 is
already handled as `NifTextureSemantic::Specular` at `material.rs:1052`). For those materials,
keep whatever the converter does today with slot 7 and do **not** point `specularTexture` at the
normal map. If you cannot establish which flag bit it is from the vendored crate, say so in your
result and leave those materials alone rather than guessing - a wrong branch here silently
wrecks every model-space-normal mesh.

**Meshes without `SLSF1_Specular`** get no specular term at all in Skyrim. Check what the
converter does with them today; if they already land at `specular_strength 0`, nothing to do -
say so. Do not change their behaviour in this task.

## Cache and schema - do NOT bump

`CONVERTER_SCHEMA_VERSION` is **18** and the reconversion for it has **not been run yet**. This
change rides the same bump: leave the version, the migration arm and
`crates/engine/src/world/lighting.rs` exactly as `impl-062` left them. If you find yourself
wanting to bump it, stop and say why in your result instead.

## Owned files

- `crates/converter/src/material.rs`
- `docs/handoff/impl-064-specular-mask-from-normal-alpha.md` - your notes

## Tests to write

- A material with a normal map and `specular_strength 0.5` publishes `specularTexture` at the
  normal map's texture index and `specularFactor == 1.0` (i.e. `2 * 0.5`).
- A material with **no** normal map publishes exactly what it publishes today - assert against the
  old shape, so the test fails if the condition is widened too far.
- `specularFactor` is clamped sensibly at the top of the range: `specular_strength 1.0` gives 2.0,
  and a strength above 1.0 does not produce something absurd. Decide the cap, justify it in a
  comment, and test it.
- A model-space-normal material does not get its `specularTexture` pointed at the normal map.
- The existing `KHR_materials_specular` tests still pass, and any that encode the old
  `specularFactor` are updated with computed values, not copied numbers.

## Acceptance

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p converter
cargo test -p engine
```

## Out of scope

Do not run the converter - the reconversion is the lead's, and it runs once after this lands. Do
not touch the roughness mapping, `metallicFactor`, the environment-map/cubemap path, the alpha
mode classification, or anything in `crates/engine/`.
