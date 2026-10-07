# Goal
Track: Phase 2. Decide from evidence whether Skyrim's lighting shader multiplies vertex alpha into
the alpha test for alpha-tested (NiAlphaProperty test, no blend), non-tree-animated shapes, and if
it does, what distinguishes the shapes that still draw solid (Whiterun walls) from those that fade
out (rock gravel skirts). Change no code.

# Context
- The converter decides per shape how glTF's COLOR_0 alpha is used
  (`crates/converter/src/material.rs`, `ValidatedNifMaterial::vertex_colour_use`; the checkout is
  `C:\Modding\wkj-phase2`, branch `main` or `bundle/portal-s26-fix`). The current rule: no
  SLSF2_Vertex_Colors (bit 5) → colours dropped; alpha mode not Blend, or lighting shader with
  SLSF2_Tree_Anim (bit 29) → alpha forced to 1; otherwise alpha is opacity.
- Why alpha is forced to 1 on alpha-tested shapes: Whiterun's alpha-tested walls and roofs carry
  vertex alpha 0.5 and 0.08 against cutoffs 0.31 and 0.5, yet draw solid in Skyrim. With the alpha
  in the test they rendered with holes.
- New observation: rock gravel skirts, e.g. `C:\Modding\Mudcrab\SkyrimConverted\vfs\meshes\landscape\rocks\rockpilem01.nif`
  shape `RockPileM01:8` (glTF material MASK, cutoff 0.1333, shaderFlags1 2386559752,
  shaderFlags2 32808, COLOR_0 alpha 0..1 in the source), now end in hard polygon edges. In Skyrim
  their rim appears to fade into the terrain. Renders: `C:\Modding\MudcrabProject\local\gallery\s26-check\cap-07.png`
  (now) and `...\s25-check\cap-07.png` (alpha kept; the faded rim shows sky colour because of an
  engine depth-prepass issue, ignore that colour). Look at both.
- Find the Whiterun wall/roof NIFs with the low vertex alpha yourself: `C:\Modding\Mudcrab\SkyrimConverted\skyrim_world.db`
  (SQLite, open read-only with a `file:...?mode=ro` URI; tables `references` (worldspace_id,
  pos_x/y/z, base_form_id) and `statics` (id, model_path)); WhiterunWorld is worldspace 0x1A26F
  (107119). Source NIFs are under `C:\Modding\Mudcrab\SkyrimConverted\vfs\meshes\`.
- Community Shaders' Lighting.hlsl (github.com/doodlum/skyrim-community-shaders) is the best
  public reconstruction of the vanilla shader. Also relevant: NifSkope/nif.xml flag names
  (BSLightingShaderProperty SLSF1/SLSF2), UESP and the Nexus/NifTools wikis.

# Scope
Read-only for the project. You may write and run Python scripts in your own scratch folder to
parse NIF blocks (flags, NiAlphaProperty flags and threshold, vertex colours). Do not modify
anything under C:\Modding. Web: read the sources above.

# Done when
- For the shader: quote the exact lines (with file and URL) that decide whether `input.Color.w`
  enters the alpha that the alpha test compares, and under which defines/techniques.
- For at least 3 Whiterun wall/roof shapes and 3 rock-skirt shapes: shader flags 1 and 2 (named
  bits), NiAlphaProperty flags (blend on/off, test on/off, test function) and threshold, and the
  vertex-alpha range. A table.
- A rule, as a function of those fields, that makes the walls solid and the skirts fade, checked
  against every shape in the table. If no field separates them, say so and give the best
  alternative explanation (e.g. the walls' alpha is unused because of a missing flag, or the
  skirts fade some other way, such as a texture alpha or a LOD technique).
- A count, from the world DB plus NIF parsing of a sample, of how many placed shapes each branch
  of the proposed rule would change.

# Report
1. Shader evidence (quotes + URLs). 2. The table. 3. The proposed rule and its check.
4. Counts. 5. Confidence per claim (high/medium/low) and what would settle the uncertain ones.
