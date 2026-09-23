# research-073 - What the converter silently drops

## Goal

A ranked, evidence-backed inventory of the converter's silent data loss: every NIF field the
converter reads and then does not publish, every published glTF field written from a constant rather
than from the data, and every field published in a lossy form. Rank by how many published materials
or meshes each finding affects, and state how each count was obtained (or that it is unmeasured).
This is the source material for `docs/research/converter-dropped-fields.md`.

## Context

- Repo `C:\Modding\OpenSkyrimProject`. The converter is `crates/converter/**`. The NIF parser and the
  GLB vertex serialization live in the vendored crate `vendor/project-wormhole-nif/**` (a copy of an
  upstream project - read it freely, but mark its findings "vendor-side"; it is not ours to change).
- **Why this audit exists.** Two bugs of exactly this shape had been wrong for every asset since the
  project began, and both were found by accident, by looking at a render:
  1. Skyrim's Blinn-Phong glossiness *exponent* was published as a linear percentage
     (`roughness = 1 - glossiness/100`), wrong for all 59,268 materials (fixed in impl-062).
  2. `SLSF1_VERTEX_ALPHA` was treated as opacity, so 1,337 meshes rendered translucent
     (impl-063; the vertex-colour alpha is a shader parameter, not transparency).
  A systematic list is the point of this task: find the rest.
- Published tree to count against, **read-only, never write**: `C:/Modding/SkyrimConverted/`. Its
  `conversion-manifest.json` says schema 18, complete, 22,761 NIFs, 32,945 DDS; the published
  material count is 59,268. GLBs at `meshes/**/*.glb`, textures at `textures/**/*.ktx2`, world
  database `skyrim_world.db`.
- Skyrim SE install, read-only: `C:/Games/Steam/steamapps/common/Skyrim Special Edition`.
- Another lead owns `crates/engine/**`: read it if it helps explain what a field is for, never edit.
- `crates/converter/src/bin/glb-audit.rs` and `nif-audit.rs` show what the project already knows how
  to extract; read them, but do not try to run anything.

## Scope

1. `crates/converter/src/material.rs` - every field of `ValidatedNifMaterial`; how each is built
   (`build_lighting_material`, `build_effect_material`, `alpha_contract`, `lighting_texture_slots`,
   `build_shape_material`); and every key written into `pbrMetallicRoughness`,
   `KHR_materials_specular`, `KHR_materials_emissive_strength`,
   `KHR_materials_pbrSpecularGlossiness` and `OPEN_SKYRIM_material`.
2. `crates/converter/src/mesh.rs` - vertex/index/attribute handling and everything it delegates; the
   prune paths; animation export (`crates/converter/src/nif_animation.rs` if the trail leads there).
3. The vendored serialization those call: `vendor/project-wormhole-nif/src/export/gltf.rs`,
   `model/mesh.rs`, `model/static_mesh.rs`, `model/skeletal_mesh.rs`, `bs/vertex.rs`,
   `bs/tri_shape.rs`, `bs/shader_texture_set.rs`, and the block structs in `nif_block.rs`
   (`BSLightingShaderProperty` ~:683, `BSEffectShaderProperty` ~:577, `NiAlphaProperty` ~:1404,
   `BSShaderTextureSet`, `BSTriShape`).
4. `crates/converter/src/texture.rs` only where a published texture loses information the material
   then depends on (colour space, alpha channel, cubemap faces, compression choice).

## Method

- Trace each field: read -> (transformed) -> published, or read -> dropped, or published -> constant.
  Give the read site and the write site as `file:line` for every finding.
- For counts, use your file-search tool over the published tree: each `.glb` is a JSON chunk plus a
  binary chunk, and the JSON is greppable as text, so a search for a JSON key such as
  `"metallicFactor":0.0` counts the documents carrying it. Search first to see whether
  `C:/Modding/SkyrimConverted` is reachable; previous work counted 59,268 materials and 53,190
  normal maps that way.
- If the tree is not reachable, mark the count "unmeasured" and write down the exact search that
  would measure it so the lead can run it. Never guess a number.
- Where a constant is published, give the constant, its source line, the count of documents carrying
  it, and what the real data would be if it were read.

## What to report per finding

A row: `# | field | read at (file:line) | what happens today | what it should become | affected count
| how counted | confidence`.

Separately, and clearly apart from the converter findings:
- published-from-a-constant fields (glTF key, constant, count of documents carrying it);
- fields the converter publishes correctly but the engine ignores (list them, marked
  "engine-side gap");
- vendor-side findings (fields the vendored parser never parses, so the converter cannot see them);
- the top five by likely visual impact, one sentence each on what would change on screen.

## Done when

- Every field of `ValidatedNifMaterial` and every field of the five block structs listed above is
  accounted for as published / dropped / constant, with read and write sites.
- Every count is either measured against `C:/Modding/SkyrimConverted` or explicitly unmeasured with
  the search that would measure it.
- Ranking is by measured count where measurable.

## Report

Markdown in the shape above plus a "could not do / uncertain" section. The tables must be
self-contained (file:line, counts and how they were obtained inline) because the lead transcribes
them into `docs/research/converter-dropped-fields.md` under your run id.

## Constraints

- Read-only analysis with file reading, searching and judgement; no writes of any kind, not even
  scratch files, and no builds, conversions or engine runs.
- A sibling worker, `research-074-environment-map-design.1`, is designing the environment-map
  contract; you may message it if you find environment-related dropped fields, but do not wait for it.
