# Goal
Find why Skyrim's glacier ice (Alftand's cliffs, the northern coast) renders as grey rock with
green blotches in both the upstream Phase 2 engine and the demo engine, where Skyrim shows
glossy blue-white ice. Report the exact cause, with evidence, and the smallest fix. Do not
implement it.

# Context
- Code: `C:/Modding/wkj-phase2` (branch `phase2/integration-lod-shots`, upstream Phase 2 work,
  read-only for you) and the demo engine `C:/Modding/MudcrabProject` (branch `portal`). The
  converter's material publication is in `crates/converter/src/material.rs` and `mesh.rs`; the
  engine's material loading is in `crates/engine/src/render.rs`.
- The symptom, from `docs/research/look-gaps-2026-09-24.md` item 6: glacier ice is drawn as green
  blotches in almost every Alftand exterior. The backlog calls it "ice reads as rock, needs
  environment maps". Test that claim rather than assuming it.
- Shots:
  - `SR-place-Alftand`, `_02` to `_06` in `tools/reference/uesp_shots_4x3.json`;
  - references in `local/reference/uesp/`;
  - renders in `C:/Modding/wkj-tools/shots-hand/{demo,up15}-uesp_shots_4x3/`.
- What to establish:
  - Which meshes and materials are the ice (Alftand's glacier pieces, e.g. `Landscape\Ice\`,
    `Glacier*`, `IceCliff*`)?
  - Their NIF shader: `BSLightingShaderProperty` type (environment map? multilayer parallax?
    "Glacier"/"Snow" flags?), their texture slots (diffuse, normal, environment mask, cube map,
    multilayer inner layer), and any vertex colours.
  - What the converter publishes for them, and what the engine draws.
  - The green: is it a texture slot drawn as diffuse (a normal map or mask sampled as colour),
    vertex colour, a missing texture fallback, or the specular or environment term?
- Data: `C:/Modding/SkyrimConverted-upstream15/` and `C:/Modding/SkyrimConverted/`. Use the GLB
  tools in `tools/research/` (`glb_materials.py`) and the world DB
  (`C:/Modding/SkyrimConverted/skyrim_world.db`, tables `references` and `statics`).

# Method
1. Pick 3-4 ice meshes placed in view of `SR-place-Alftand` / `_02`.
2. For each: the NIF shader and slots, the GLB material, the engine material. Name where the
   green comes from, with file:line.
3. Say the smallest fix that gets glossy, pale ice:
   - is it a wrong slot mapping (a fix), or
   - a missing feature (environment and cube map support: say what that would take)?

Write python scripts under `local/research-554/` and run them as `python <file>`. Inline
`python -c` is not allowed.

# Scope
Read everything. Write only under `local/research-554/`. No source changes, no builds.

# Report
- The cause in 3 sentences.
- A table: mesh | NIF shader type, flags and slots | GLB material | engine result.
- The fix, or what the missing feature needs, with files and schema impact.
- Evidence paths.
