# research-074 - What the converter should publish for Skyrim's environment cubes

## Goal

A design, not code: say precisely what the converter must publish so the engine can render the
environment response of Skyrim's environment-mapped materials - the visible symptom is "ice reads as
rock" - and say precisely where the converter's job ends and what the engine must then do. The answer
"publish X and the engine needs Y" is expected and acceptable; stop at the converter boundary.

## Context

- Repo `C:\Modding\OpenSkyrimProject`. Converter `crates/converter/**` (this task's territory).
  Engine `crates/engine/**` belongs to another lead - read it, never edit it. Vendored NIF parser
  `vendor/project-wormhole-nif/**` is read-only for us (a copy of an upstream project).
- The gap: `docs/research/visual-gaps-spec.md` gap 3. The Alftand ice set is `meshes/dungeons/caves/
  ice/**` (about 508 glbs) with base colour `icefrozen01`, a normal map, and slot 6
  `icecavesubsurfacetint01`; the Dwemer roof `dwefacadetowerroof01.glb` is the other named example
  (base colour `dwemetaltiles01`, cube `textures/cubemaps/bronze_e.ktx2`, glossiness 400). The engine
  binds no environment lighting anywhere (no `EnvironmentMapLight`, `LightProbe` or
  `IrradianceVolume`), so the cube the converter already emits is discarded.
- `docs/research/specular-gloss-mapping.md` sections 5.1 and 7 are the current state of the argument:
  7,823 materials carry an `environment_cube` slot, 7,152 declare
  `"lightingShaderType":"environment_map"`, and **metalness must not be set before the engine binds
  an environment map** or Dwemer metal gets darker, not shinier.
- What the converter does today:
  - classifies texture slots (`lighting_texture_slots`, `crates/converter/src/material.rs:1034-1081`:
    slot 4 `EnvironmentCube`, slot 5 `EnvironmentMask`, slot 6 `InnerLayer`);
  - converts a DDS cubemap into a real KTX2 cubemap (`crates/converter/src/texture.rs`,
    `combine_ktx2_cubemap_faces` around `:701`);
  - publishes slots 4/5/6 as `OPEN_SKYRIM_material.textureSlots` entries
    (`material.rs:680-731`) that nothing reads.
- A live suspicion, to confirm or kill with evidence: Skyrim's shader block carries an **Environment
  Map Scale** (`vendor/project-wormhole-nif/src/nif_flags.rs:45`, "Environment mapping (uses Envmap
  Scale)"), but the vendored `BSLightingShaderProperty` (`nif_block.rs:683-703`) ends at
  `lighting_effect_2` and has no such field; the only `env_map_scale` in that crate is in
  `BSSPWetnessParams`, a Fallout 4 block (`:705-714`). Establish whether the value is reachable at
  all; if not, say where it lives in the file bytes and what change would expose it.
- Budget: ONE reconversion for this whole batch (schema 18 -> 19). A converter change whose payoff
  needs engine work is acceptable if that engine work is stated exactly.
- Published tree, read-only: `C:/Modding/SkyrimConverted/` (`meshes/**/*.glb`). Skyrim SE install,
  read-only: `C:/Games/Steam/steamapps/common/Skyrim Special Edition`. Bevy 0.19 source is in the
  local registry: `C:\Users\Brannock\.cargo\registry\src\index.crates.io-*\bevy_*`.

## Questions to answer

1. **What Skyrim does.** For `lightingShaderType` `environment_map`, `eye_environment_map` and
   `multi_layer_parallax`, and for the `SLSF1_ENVIRONMENT_MAPPING` bit 7: what is the cube used for in
   the shader - a reflection added on top of the specular term, the colour source, or refraction -
   and what does the mask slot (5) modulate? Use the flag table, Creation Kit / UESP / NifSkope
   documentation if it is in the repo or the game docs, and say what is documented versus inferred.
2. **What is missing.** List every quantity a correct binding needs (cube image, mask image,
   strength/scale, tint, per-material vs per-space, the normal map as the perturbation), and mark
   each: read by the converter already / published but unused / not readable at all.
3. **The published contract.** Recommend exactly one option, with the exact JSON for two real
   materials (one ice, one Dwemer roof) and the `file:line` of every code point that would change.
   Consider at least:
   (a) keep `textureSlots` as published and document its semantics for the engine;
   (b) add a dedicated object such as `OPEN_SKYRIM_material.environment = {"texture": <index>,
   "mask": <index|null>, "scale": <number>, ...}` - and every field in it must be real data; a field
   written from a constant is a defect (the sibling audit, `research-073-converter-dropped-fields.1`,
   is ranking exactly those);
   (c) publish standard glTF extensions instead (`KHR_materials_ior`, specular tweaks).
   Give the count of materials each option touches, and prefer the one with the smallest honest
   change. There is exactly one schema bump available, so say whether the recommendation needs one at
   all.
4. **The engine's half.** Bevy 0.19 only. What would the engine bind, and is it per material or per
   space? Read the vendored Bevy source (`C:\Users\Brannock\.cargo\registry\src\index.crates.io-*\
   bevy_pbr-0.19.0`, `bevy_gltf-0.19.0`, `bevy_image-0.19.0`) and `crates/engine/src/render.rs`.
   Name the exact components/APIs, and state what has to be true of the published image (face count,
   mip chain, colour space, transfer function, compression) for Bevy to load it as an environment
   map. Say plainly whether the engine can do it today with zero converter change.
5. **What not to do yet**, briefly: metalness (there is a stated reason), and anything that presumes a
   binding that does not exist.
6. **The holdout check.** Which real glb paths under `C:/Modding/SkyrimConverted/meshes/` to look at
   after the reconversion, and what "correct" looks like in each.

## Done when

- The recommendation fits on one page with exact JSON and change points.
- The "engine needs Y" statement is precise enough that another worker could implement Y without
  asking a question.
- Whether the environment-map scale is reachable is answered with evidence, not a guess.
- The uncertainty list says what could not be verified and what would settle it.

## Report

Markdown: (1) the recommendation in ten lines; (2) evidence for what Skyrim does; (3) contract JSON;
(4) change points as `file:line`; (5) the engine's half; (6) counts; (7) uncertainties; (8) the
holdout check.

## Constraints

- Read-only: no writes of any kind.
- Do not build, run cargo, run the converter or the engine.
- Do not propose engine changes as if they were done here; the deliverable is a spec.
- "There is nothing useful to publish yet" is an acceptable outcome if it is argued from the data.
