# Goal
Track: Phase 2. Decide how a lighting-shader shape with `SLSF1_Vertex_Alpha` (shaderFlags1 bit 3)
but **no NiAlphaProperty** should be exported: opaque (alpha forced to 1) or alpha-blended with its
vertex alpha as opacity. Change no project code.

# Context
- Current converter rule (per shape): no `SLSF2_Vertex_Colors` (shaderFlags2 bit 5) → colours
  dropped; lighting + `SLSF2_Tree_Anim` (shaderFlags2 bit 29) → alpha 1 (wind amplitude);
  lighting + Vertex_Alpha → alpha is opacity; otherwise alpha 1. The glTF alpha **mode** comes from
  the NiAlphaProperty: test enabled → MASK with its threshold; blend enabled → BLEND; no property →
  OPAQUE. So a Vertex_Alpha shape with no alpha property keeps its vertex alpha in COLOR_0 but is
  drawn OPAQUE, i.e. the alpha has no effect.
- The case that shows it: `C:\Modding\SkyrimConverted\vfs\meshes\landscape\rocks\rockpilem01.nif`,
  shape `RockPileM01:0` (a dark dirt skirt under the rock): shaderFlags1 2185233161 (bit 3 set),
  shaderFlags2 32801, no NiAlphaProperty, vertex alpha 0..0.55. It renders as a hard dark polygon
  on the terrain; in Skyrim it fades into the ground. Render:
  `C:\Modding\OpenSkyrimProject\local\gallery\s27-check\cap-07.png` (look at it).
- History: the upstream converter originally made "Vertex_Alpha (or screen-door fade) without an
  alpha property" a BLEND material. On 2026-09-22 that was removed because 2,632 converted materials
  became see-through (trees and rocks drawn transparent). It is not known how many of those were
  tree-animated (now excluded from opacity anyway) versus other shapes.
- Parse NIFs with your own Python (SSE NIF 20.2.0.7, user version 12, BS version 100): BSTriShape
  vertex data (the vertex descriptor says whether colours are present), BSLightingShaderProperty
  flags, NiAlphaProperty presence/flags. Source NIFs: `C:\Modding\SkyrimConverted\vfs\meshes\`.
  World DB: `C:\Modding\SkyrimConverted\skyrim_world.db` (SQLite, open read-only with
  `file:...?mode=ro`; `statics(id, model_path)`, `references(base_form_id, ...)`), to weight by
  placements.
- Vanilla shader reference (from an earlier web worker, Community Shaders `Lighting.hlsl`):
  vertex alpha multiplies the fragment alpha except under TREE_ANIM / LODOBJECTS / LODOBJECTSHD;
  whether the shape is blended is decided by the NiAlphaProperty (engine render state), so a shape
  with no alpha property is not blended by that property. Find in the data whether such shapes
  nevertheless look faded in the game, e.g. whether their vertex alpha is ever below 1 where the
  model clearly means an edge fade (skirts, decals), and whether any look like they must stay solid.

# Scope
Read-only. Write and run Python scripts only in your working folder.

# Done when
- Count, over every NIF used by a placed static in the world DB (parse each distinct NIF once): lighting
  shapes with Vertex_Colors, Vertex_Alpha set, no NiAlphaProperty, not Tree_Anim; how many have any
  vertex alpha < 1, < 0.5; weighted by placements too. Split by top-level folder and by the 15 most
  common diffuse textures.
- Twelve examples across folders (landscape, architecture, dungeons, clutter), each: NIF, shape,
  flags, vertex alpha range and where the low alpha sits (outer ring? random? whole shape?), and
  your call: fade intended / solid intended, with the reason.
- The same count for Tree_Anim shapes (to say how many of the 2,632 they were likely to be).
- A recommendation: keep OPAQUE, or BLEND with vertex alpha, or a narrower rule (a field that
  separates the fade cases from the solid ones), checked against every example.

# Report
1. Counts (tables). 2. Examples. 3. The recommendation and its check. 4. Confidence per claim.

# Web questions
none
