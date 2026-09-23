# research-058 - Why every surface reads wet: the specular/gloss mapping

## What was seen

The first Riverwood renders (`local/reference/rw1/`, twelve poses from
`tools/reference/riverwood_shots.json`) show weathered timber, barrels, stone pillars and thatch
with a strong wet, varnished sheen. `local/reference/rw1/_crop-gloss.png` is the clearest: the
porch posts and the barrel outside the Sleeping Giant Inn carry broad white specular highlights
that Skyrim's own materials do not have. `_crop-gloss2.png` shows the same on the inn's interior
pillars. Compare `local/reference/riverwood/SR-place-Sleeping_Giant_Inn.jpg` and
`SR-interior-Sleeping_Giant_Inn_Common_Room.jpg`: the same wood is matte there.

This was **not** in `docs/research/visual-gaps-spec.md` and not predicted by research-055. It
affects every render the project has made, not only Riverwood - it was simply harder to see on
Dwemer metal and ice than on wood.

**This is a research task. Do not change any code.** Produce a document and a recommendation; the
lead decides what to implement and briefs it separately.

## The question

Skyrim's `BSLightingShaderProperty` carries, among other fields, a **glossiness** (the vanilla
default is around 80, and the value is an exponent, not a linear roughness), a **specular
strength** and a **specular colour**, and many materials put a specular mask in the **alpha
channel of the normal map**. Bevy's `StandardMaterial` instead takes `perceptual_roughness`
(default **0.5**), `metallic` (default **0.5**), `reflectance` (default 0.5) and an optional
`metallic_roughness_texture` whose green channel is roughness.

Find out, from the code and the data:

1. **What the converter actually writes.** Which of those NIF shader fields does
   `crates/converter/` read, and what does it put in the glTF material - `pbrMetallicRoughness`'s
   `roughnessFactor`, `metallicFactor`, any `metallicRoughnessTexture`, `KHR_materials_specular`?
   Cite `path:line` for each. If a field is read and then dropped, say where it is dropped.
2. **What the engine then does.** Does `crates/engine/` override, scale or ignore any of it when it
   builds the `StandardMaterial`? Cite `path:line`.
3. **What the resulting numbers are on real assets.** Open the glb JSON chunks of at least
   `Architecture/Farmhouse/Farmhouse02.nif`'s and `Inn01.nif`'s converted materials, a barrel
   (`Clutter/Barrel01` or whatever the Riverwood references actually use - find it in
   `C:/Modding/SkyrimConverted/skyrim_world.db`), and the inn's interior pillars, and report the
   actual `roughnessFactor` / `metallicFactor` values and whether a
   `metallicRoughnessTexture` is present. A table of model → values is the deliverable here. If
   every material has the same pair of numbers, say so plainly: that is the answer.
4. **What the NIFs actually carry.** For the same handful of models, read the
   `BSLightingShaderProperty` out of the source NIF (the BSA reader is
   `crates/converter/src/archive/bsa.rs`; a read-only Python or Rust scratch script under
   `local/` is fine) and report glossiness, specular strength, specular colour, and whether the
   normal map has an alpha channel. This is what tells us whether the data is there and unused, or
   genuinely absent.
5. **The mapping to recommend.** Given 1-4, propose the conversion from Skyrim's glossiness to
   `perceptual_roughness`, with the arithmetic written out, and say what `metallic` should be for
   a non-metal (Bevy's 0.5 default is almost certainly wrong for wood and thatch). Name the
   handful of materials that genuinely *are* metal, if the data distinguishes them. Say how you
   would check the result: which two or three of the twelve Riverwood poses would show it, and
   what a correct render would look like against the UESP reference.
6. **Scope of the blast radius.** How many distinct converted materials are affected, and would a
   change here force a reconversion or is it engine-side only? The answer decides how expensive
   the fix is, so be precise and say which it is.

## Web lookups

You may search the web for the `BSLightingShaderProperty` field layout, the meaning of Skyrim's
glossiness exponent, and how other open reimplementations map it to PBR roughness. Cite what you
use. Do not copy code from another project into this repository.

## Deliverable

`docs/research/specular-gloss-mapping.md`, and nothing else. Lead with a two-sentence answer to
"why does everything look wet", then the evidence, then the recommendation. Tables over prose for
the measurements. If the evidence does not support the wet-look diagnosis - if the materials turn
out correct and something else causes it - say that instead; a negative result found early is
worth more than a confident wrong one.

## Owned files

- `docs/research/specular-gloss-mapping.md`
- `local/research-058/` - any scratch scripts and dumps

## Out of scope

No changes to `crates/`. No reconversion. Do not touch the shots file or render anything yourself
(the lead has the GPU); reason from `local/reference/rw1/` and the crops named above.
