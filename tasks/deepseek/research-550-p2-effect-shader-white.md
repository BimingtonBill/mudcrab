# Goal
Find why fire, candle flames and window light shafts render as solid white shapes, in both the
upstream Phase 2 engine and the demo engine. Report the exact cause, with evidence, and the
smallest fix. Do not implement it.

# Context
- Project: a Rust/Bevy reimplementation of Skyrim's renderer. Two code bases:
  - the upstream Phase 2 work, `C:/Modding/wkj-phase2` (branch `phase2/integration-lod-shots`,
    read-only for you);
  - the demo engine, `C:/Modding/OpenSkyrimProject` (branch `portal`).
  Both share the converter (`crates/converter`), which turns NIF meshes into GLB with
  materials (`crates/converter/src/mesh.rs`, `material.rs`), and the engine's material loading
  (`crates/engine/src/render.rs` and similar).
- The symptom, from `docs/research/look-gaps-2026-09-24.md` item 3:
  - hearths show a white shard instead of fire, window light shafts are white bars, and
    candles are unlit;
  - seen in the four Riverwood interiors, Candlehearth Hall, the Palace of the Kings, High
    Hrothgar, Whiterun and Winterhold.

  Side-by-side pictures are in `local/reference/look-gaps/`; the renders are in
  `C:/Modding/wkj-tools/shots-hand/{demo,up15}-*/`.
- The hypothesis to test: these are Skyrim effect-shader meshes (`BSEffectShaderProperty`:
  flames, light shafts, glow cards). The converter either drops their shader, so they fall back
  to a plain white opaque lit material, or exports them without additive blending, emissive
  colour or their falloff. In Skyrim they are additive or alpha-blended, unlit, with a greyscale
  texture tinted by a colour and fading by view angle.
- Converted data: `C:/Modding/SkyrimConverted-upstream15/` (GLBs under `meshes/`, read with
  the GLB tools in `tools/research/`, e.g. `glb_materials.py`) and `C:/Modding/SkyrimConverted/`.
  The game's original NIFs are inside Skyrim's BSA archives. The converter's extracted copies
  may be under the converted folder's `vfs/`.

# Method
1. Pick 3-5 concrete offending meshes: for example a hearth fire, a candle flame and a window
   light shaft in Riverwood's Sleeping Giant Inn. Find their model paths through the world
   database: `C:/Modding/SkyrimConverted/skyrim_world.db`, tables `references` and `statics`.
2. For each, show:
   - what the NIF's shader property is (`BSEffectShaderProperty` or `BSLightingShaderProperty`,
     with flags, blend and alpha settings, emissive colour and falloff);
   - what the converter wrote into the GLB material;
   - how the engine turns that into a Bevy material.
3. Name the step where the effect behaviour is lost, with file:line.
4. Propose the smallest correct fix, converter or engine or both, and say whether it needs a
   converter schema bump.

Write small python scripts for inspection under `local/research-550/` and run them as
`python <file>`. Inline `python -c` is not allowed.

# Scope
Read everything. Write only under `local/research-550/`. Change no source file. No builds.

# Report
- The cause in 3 sentences.
- A table: mesh | NIF shader and key flags | GLB material as exported | engine material.
- The fix: which files and what change, schema impact, and the risks (e.g. which other meshes
  share the code path).
- Evidence file paths.
