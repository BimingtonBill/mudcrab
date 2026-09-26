# Goal
Find out whether Skyrim's LAND vertex colours (VCLR) are applied by this engine's terrain, and if
not, exactly where and how they should be, so a fix can be briefed. User report: grass and
landscape texture hues around Whiterun do not match the game.

# Context
- Repository checkout: this folder (`main` branch of an open-source Skyrim-data renderer in Rust/Bevy).
- The converter writes `land.vclr` (BLOB) in `crates/converter/src/esm/exporter.rs` (~line 48, 173)
  and `cell_cache.rs` reads `VCLR` (~line 22). The engine's terrain is `crates/engine/src/render.rs`
  (TerrainMaterial / TerrainExtension), `crates/engine/src/streaming.rs` (terrain mesh build) and
  `crates/engine/src/shaders/terrain.wgsl`.
- Screenshots of the problem (read them): `C:/Modding/OpenSkyrimProject/local/captures/2026-09-26_11-44-01/06.png`,
  `07.png`, `08.png`, with `06.json` etc. (pose/settings) and `notes.md`.

# Questions
1. Does any engine code read `vclr` / vertex colours for terrain? Trace from the database/cell
   cache to the terrain mesh and shader. Quote file:line.
2. VCLR's layout (33x33 RGB bytes per cell, row order) and how the game combines it with the
   texture layers (multiply? on which layers?). Use UESP "Skyrim Mod:Mod File Format/LAND" if you
   can reach it; otherwise say what the repository's docs/spec (`docs/specs/`) say. Cite.
3. Where the fix goes: mesh vertex colour attribute vs a texture, the shader line to change, and
   whether converted data already has what is needed (no reconversion) - check the cell cache format.
4. Anything else in the screenshots that could explain the hue difference (e.g. missing texture
   layers, wrong colour space) - say what you see.

# Scope
Read-only. Write nothing.

# Report
Answers per question with file:line and citations; then a 5-line fix plan with the files to change.
