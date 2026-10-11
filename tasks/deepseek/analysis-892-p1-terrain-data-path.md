# Goal
Map exactly how terrain goes from the converter's output to the screen on current main, so Phase 1 can propose a merged-mesh-per-cell format (#186). Phase 1 track. Code reading only.

# Context
- Checkout: `C:/Modding/Mudcrab/worktrees/p1-terrain-merge` (branch phase1/terrain-merge = origin/main f40ca1df). Read only.
- Converter: `crates/converter/src/` (search `cell_cache`, `rkyv`, `LAND`, `VHGT`, `VNML`, `VCLR`, `BTXT`, `ATXT`, `VTXT`, `quadrant`, `land`). Shared types: `crates/shared/src/` (search `cell_cache`, `Terrain`, `Land`).
- Engine: `crates/engine/src/streaming.rs` (terrain build, around the quadrant mesh code), `crates/engine/src/render.rs` (terrain material, around 504), the terrain shader (`terrain.wgsl`, find it), plus any terrain module.
- Measured data (lead, 2026-10-07): `local/research/terrain-merge/land-layers-2026-10-07.md`. Up to 8 distinct textures at one merged-cell vertex, up to 7 per quadrant vertex, up to 13 per cell.
- Plan context: `local/research/engine-foundations/research-890-terrain-merge.md` (one merged mesh per cell, a worldspace texture array, one shared material; Phase 2 does the engine side).
- Record rule: when you name a LAND field, use xEdit 4.1.5f's names (LAND: `Core/wbDefinitionsTES5.pas:10417-10437`; layers: `Core/wbDefinitionsCommon.pas:1371-1400`).

# Questions (file:line for each)
1. Converter output: which terrain data is written where (cell_cache.rkyv struct and fields, the world DB `land` table, anything else)? Exact types, sizes per cell, and how layers are stored (per quadrant? max count? order kept? opacity format?). How is the default texture (no BTXT) handled?
2. Engine build: from that data to meshes. Vertices per quadrant mesh, attributes (position, normal, UV, colour, layer weights: how packed), index format, and where normals and edge welding happen.
3. Material and shader: what the per-quadrant material holds (textures, uniforms, layer count limit), how terrain.wgsl blends layers (order, base, opacity), and any limit (for example a maximum of N layers per quadrant, and what happens above it).
4. Texture loading: how land textures (LTEX to TXST to diffuse/normal paths) are resolved and loaded, per quadrant or shared? Same images reused across quadrants?
5. Collision: the per-quadrant trimesh, its inputs, and whether it could come from a per-cell mesh unchanged.
6. LOD terrain (#105) on main: what it uses, and whether near and far terrain share anything.
7. Version and compatibility: which schema number or version field covers cell_cache.rkyv, and how the engine reacts to an older file.

# Scope
Read only. No cargo, no builds, no writes.

# Report
Under 700 words: answers 1-7 with file:line, then a short "what a merged per-cell format must preserve" list (everything the current path encodes that the new one must keep identical, for an image-diff match).
