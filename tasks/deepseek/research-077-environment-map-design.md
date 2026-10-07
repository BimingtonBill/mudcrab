# research-077 - What the converter should publish for Skyrim's environment cubes (attempt 2 of research-074)

## Goal

A design, not code: say precisely what the converter must publish so the engine can render the
environment response of Skyrim's environment-mapped materials ("ice reads as rock" is the visible
symptom), and say precisely where the converter's job ends and what the engine must then do. The
answer "publish X and the engine needs Y" is expected; stop at the converter boundary.

**Turn budget.** You have up to 150 turns. A first attempt spent all 60 reading and never wrote a
report. Spend at most the first 90 turns reading, then write the final report; after turn 120 write
what you have even if unfinished. Never end without a report.

## Context

- Repo `C:\Modding\MudcrabProject`. Converter `crates/converter/**` (this task's territory).
  Engine `crates/engine/**` is another lead's - read, never edit. `vendor/project-wormhole-nif/**`
  (the NIF parser) is a vendored copy, read-only for us.
- The gap: `docs/research/visual-gaps-spec.md` gap 3 (the Alftand ice set under
  `C:/Modding/Mudcrab/SkyrimConverted/meshes/dungeons/caves/ice/`, base colour `icefrozen01`; the Dwemer roof
  `meshes/dungeons/dwemer/facades/dwefacadetowerroof01.glb` is the other named example). The engine
  binds no `EnvironmentMapLight`, `LightProbe` or `IrradianceVolume` anywhere.
- `docs/research/specular-gloss-mapping.md` sections 5.1 and 7: 7,823 materials carry an
  `environment_cube` slot, 7,152 declare `"lightingShaderType":"environment_map"`, and **metalness
  must not be set before the engine binds an environment map** (Dwemer metal would get darker).
- What the converter does today (a sibling audit established these; verify what you depend on):
  - slot classification in `lighting_texture_slots` (`crates/converter/src/material.rs:1034-1081`);
  - a DDS cubemap becomes a real KTX2 cubemap (`crates/converter/src/texture.rs`,
    `combine_ktx2_cubemap_faces` around `:701`); 44 cubemaps exist under `textures/cubemaps/`;
  - slots are published in `OPEN_SKYRIM_material.textureSlots` (`material.rs:680-731`), which nothing
    reads; `dwefacadetowerroof01.glb` material `DweFacadeTowerRoof01:6` carries
    `{"slot":4,"semantic":"environment_cube","texture":2,"required":true,"colorSpace":"linear"}` with
    uri `textures/cubemaps/bronze_e.ktx2`;
  - **the label and the file disagree**: `material.rs:694-700` writes `"colorSpace":"linear"` for the
    cube, but `texture.rs:44-53` treats `EnvironmentCube` as a colour semantic and encodes the KTX2
    with the sRGB transfer function (`texture.rs:927-929`). The KTX2 header decides decoding; do not
    trust the extension label.
  - never read at all: `lighting_effect_1`/`lighting_effect_2` (`vendor/.../nif_block.rs:701-702`),
    `refraction_strength` (`:697`); `SLSF1_ENVIRONMENT_MAPPING` (bit 7) only classifies slots.
  - `BSLightingShaderProperty` in the vendored parser (`nif_block.rs:683-703`) ends at
    `lighting_effect_2`, so Skyrim's **Environment Map Scale** (documented in
    `vendor/.../nif_flags.rs:45`, "Environment mapping (uses Envmap Scale)") is not parsed; the only
    `env_map_scale` in that crate belongs to `BSSPWetnessParams`, a Fallout 4 block (`:705-714`).
    Establish whether the value is reachable at all, and where it sits in the block bytes.
- Budget: ONE reconversion for the whole batch (schema 18 -> 19). A converter change whose payoff
  needs engine work is acceptable if that work is stated exactly.
- Bevy 0.19 source is in the local registry:
  `C:\Users\Brannock\.cargo\registry\src\index.crates.io-*\bevy_pbr-0.19.0`, `bevy_gltf-0.19.0`,
  `bevy_image-0.19.0`; the engine's material handler is `crates/engine/src/render.rs`.

## Questions to answer

1. **What Skyrim does.** For `lightingShaderType` `environment_map`, `eye_environment_map`,
   `multi_layer_parallax` and the `SLSF1_ENVIRONMENT_MAPPING` bit: what is the cube used for - a
   reflection added to the specular term, the colour source, refraction - and what does the mask slot
   (5) modulate? Say what is documented versus inferred.
2. **What is missing.** Every quantity a correct binding needs (cube, mask, strength/scale, tint,
   per-material vs per-space, the normal map as perturbation) marked: read already / published
   unused / unreadable.
3. **The published contract.** Recommend exactly one option, with the exact JSON for one real ice
   material and one Dwemer roof material, and the `file:line` of every change point. Consider
   (a) keep `textureSlots` and document its semantics; (b) a dedicated object such as
   `OPEN_SKYRIM_material.environment = {"texture": <index>, "mask": <index|null>, "scale": <number>}`
   where every field is real data - a constant is a defect; (c) standard glTF extensions
   (`KHR_materials_ior`, specular tweaks). Say whether the recommendation needs the schema bump at
   all, and how many materials it touches.
4. **The engine's half.** Bevy 0.19 only: what binds the cube, per material or per space? Name the
   exact components/APIs and what must be true of the published image (face count, mip chain, colour
   space, transfer function, compression) for Bevy to load it as an environment map. State plainly
   whether the engine can do it today with zero converter change.
5. **What not to do yet**: metalness, and anything that presumes a binding that does not exist.
6. **The holdout check**: real glb paths to look at after a reconversion, and what "correct" looks
   like in each.

## Done when

- The recommendation fits on one page with exact JSON and change points.
- The "engine needs Y" statement is precise enough to implement without asking a question.
- The environment-map-scale question is answered with evidence.
- The report exists.

## Report

Markdown: (1) recommendation in ten lines; (2) evidence for what Skyrim does; (3) contract JSON;
(4) change points; (5) the engine's half; (6) counts; (7) uncertainties; (8) the holdout check.
