# research-076 - What the converter silently drops (attempt 2 of research-073)

## Goal

A ranked, evidence-backed inventory of the converter's silent data loss: every NIF field the
converter reads and then does not publish, every published glTF field written from a constant rather
than from the data, and every field published in a lossy form. Rank by how many published materials
or meshes each finding affects. This is the source material for
`docs/research/converter-dropped-fields.md`.

**Turn budget, and it is the reason this is attempt 2.** You have up to 150 turns. A first attempt
spent all 60 of its turns reading and never wrote a report, so: spend at most the first 90 turns
reading and searching, then write the final report; if you are still short of turns after turn 120,
write the report with whatever is finished and name what is missing. Never end without a report.

## Context

- Repo `C:\Modding\OpenSkyrimProject`. The converter is `crates/converter/**`. The NIF parser and the
  GLB vertex serialization are `vendor/project-wormhole-nif/**` (a vendored copy of an upstream
  project): read it freely, but mark its findings "vendor-side"; it is not ours to change.
- **Why this audit exists.** Two bugs of this shape had been wrong for every asset since the project
  began, and both were found by accident, in a render:
  1. Skyrim's Blinn-Phong glossiness *exponent* was published as a linear percentage
     (`roughness = 1 - glossiness/100`) - wrong for all 59,268 materials (fixed in impl-062).
  2. `SLSF1_VERTEX_ALPHA` was treated as opacity, so 1,337 meshes rendered translucent (impl-063;
     the vertex-colour alpha is a shader parameter, not transparency).
- Published tree to count against, **read-only, never write**: `C:/Modding/SkyrimConverted/`
  (manifest: schema 18, complete, 22,761 NIFs, 32,945 DDS, 59,268 materials). GLBs at
  `meshes/**/*.glb`, textures at `textures/**/*.ktx2`.
- Skyrim SE install, read-only: `C:/Games/Steam/steamapps/common/Skyrim Special Edition`.
- Another lead owns `crates/engine/**`: read it to explain what a field is for, never edit it.

## Starting points from the first attempt (verify each; do not rediscover)

These were established in attempt 1 before it ran out of turns. Lead-checked against the code sites
named, but re-verify anything you build on:

1. The environment cube **is** published: `meshes/dungeons/dwemer/facades/dwefacadetowerroof01.glb`,
   material `DweFacadeTowerRoof01:6`, carries `OPEN_SKYRIM_material.textureSlots` with
   `{"slot":4,"semantic":"environment_cube","texture":2,"required":true,"colorSpace":"linear"}` and
   image uri `textures/cubemaps/bronze_e.ktx2`; 44 cubemaps exist under `textures/cubemaps/`.
2. **Label and file disagree** for the cube: `crates/converter/src/material.rs:694-700` passes
   `is_srgb = matches!(semantic, Detail)`, so an environment cube gets a linear URI and
   `"colorSpace":"linear"`, but `crates/converter/src/texture.rs:44-53` counts `EnvironmentCube` as a
   colour semantic, so the published KTX2 is written with the sRGB transfer function
   (`texture.rs:927-929`). Chase every semantic through that pair of functions; the same class of
   mismatch may exist elsewhere.
3. Never read by the converter (zero hits for the names under `crates/converter/src`):
   `lighting_effect_1` / `lighting_effect_2` (`vendor/.../nif_block.rs:701-702`) and
   `refraction_strength` (`:697`). `SLSF1_ENVIRONMENT_MAPPING` (bit 7) is read only to classify slot
   4/5 semantics (`material.rs:1040-1045`), never published as a strength.
4. The legacy geometry path (`vendor/project-wormhole-nif/src/legacy.rs`, `NiTriShapeData`) parses
   tangents and then drops them (`:95-101`), and keeps only the first UV set
   (`uv_sets.into_iter().take(vertex_count)`, `:113-120`). Colours are parsed there too - find out
   where they go.
5. `MeshConverter::prune_dangling_texture_uris` deliberately drops a material's texture reference
   when its KTX2 is missing on disk (impl-067 only changed the bookkeeping): 182 meshes / 511
   references in the run that was measured.

## Scope

1. `crates/converter/src/material.rs` - every field of `ValidatedNifMaterial`; how each is built
   (`build_lighting_material`, `build_effect_material`, `alpha_contract`, `lighting_texture_slots`),
   and every key written into `pbrMetallicRoughness`, `KHR_materials_specular`,
   `KHR_materials_emissive_strength`, `KHR_materials_pbrSpecularGlossiness` and
   `OPEN_SKYRIM_material`.
2. `crates/converter/src/mesh.rs` - vertices, indices, attributes, everything it delegates, the prune
   path, animation export (`nif_animation.rs`).
3. The vendored code those drive: `export/gltf.rs`, `model/mesh.rs`, `model/static_mesh.rs`,
   `model/skeletal_mesh.rs`, `bs/vertex.rs`, `bs/tri_shape.rs`, `bs/shader_texture_set.rs`,
   `legacy.rs`, and the block structs in `nif_block.rs` (`BSLightingShaderProperty` ~:683,
   `BSEffectShaderProperty` ~:577, `NiAlphaProperty` ~:1404).
4. `crates/converter/src/texture.rs` only where a published texture loses information the material
   then depends on (colour space, alpha, cubemap faces, compression).

## Method

- Trace each field: read -> (transformed) -> published, or read -> dropped, or published -> constant.
  Give the read site and the write site as `file:line`.
- Counts: each `.glb` is a JSON chunk you can search as text. Grep with the files-with-matches mode
  reports `totalMatches` even when the hit list is truncated, and Glob reports `totalMatches` for
  file-name patterns (that is how 44 cubemaps and 149 `_g.ktx2` glow maps were counted). Use those
  two shapes; do not guess a number. Mark anything you could not count "unmeasured" and give the
  search that would measure it.
- For a constant, give the value, the source line, how many documents carry it, and what the real
  data is if it is read.

## What to report per finding

Rows of: `# | field | read at (file:line) | what happens today | what it should become | affected
count | how counted | confidence`.

Then, apart from the converter findings: published-from-a-constant fields; fields published but
ignored by the engine ("engine-side gap"); vendor-side findings; and the top five by likely visual
impact, one sentence each on what would change on screen.

## Done when

- Every field of `ValidatedNifMaterial` and of the five block structs is accounted for, with read and
  write sites.
- Counts are measured against `C:/Modding/SkyrimConverted` or explicitly unmeasured with the search
  that would measure them.
- The report exists, however much is finished.

## Report

Markdown in the shape above plus a "could not do / uncertain" section, self-contained enough that the
lead can transcribe the tables into `docs/research/converter-dropped-fields.md` under your run id.
