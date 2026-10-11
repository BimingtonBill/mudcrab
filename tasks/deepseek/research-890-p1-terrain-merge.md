# Goal
Could the converter pre-merge terrain into fewer, larger meshes (per cell, per group of cells or per LOD chunk) with the same detail and textures, so the engine draws far fewer terrain patches? Work out what that output looks like and what the engine would need to read. Phase 1 track (converter side; Phase 2 owns the engine side). Code reading plus web research. The portal demo goal is a stable 160 fps (6.25 ms per frame) while travelling and through load doors, with no shortcuts (no lower detail or view distance).

# Measured problem (Portal Dev, 2026-10-07)
The terrain ring costs 14.2 ms of render-thread time against 7.2 ms with a small ring (1,156 terrain patches against 100), because each patch is its own draw with its own material. Look for the numbers in `local/perf-baseline/2026-10-07-scene-set-b/baseline.md` and `local/research/engine-foundations/analysis-1009-baseline-cpu-breakdown.md` (search "terrain").

# Context
- The engine builds 4 quadrant meshes per loaded cell at runtime from `cell_cache.rkyv`, plus a terrain material extension per quadrant and a Rapier trimesh collider per quadrant: `crates/engine/src/streaming.rs` (around 4305, 4357, 4485, 3880), `crates/engine/src/render.rs` (around 504), the terrain shader `assets/shaders/terrain.wgsl` or wherever `terrain.wgsl` lives. Earlier summary: `local/research/engine-foundations/analysis-882-883-cell-load-cost.md` (section "Terrain, main thread").
- Converter side: `crates/converter/src/` (search `cell_cache`, `quadrant`, `LAND`, `VTXT`, `ATXT`, `BTXT`). Skyrim's per-quadrant texture layers (BTXT base + up to 8 ATXT layers with 17x17 VTXT weights per quadrant) are why the engine splits by quadrant; any merge must keep every layer's texture and weights.
- Terrain LOD meshes (distant, from #105) are a separate path: `lod/` in the converted output, `crates/converter/src/` (search `lod`). Say whether near terrain could share their chunking.
- #185 derived data: per-object identity, content-hashed keys (`cell_key` from record hashes plus the plugin master list), storage per cell for interiors and per LOD-chunk group for exteriors, rebuilt automatically, and the engine must still run without it. See `docs/handoff/LOG.md` (search "#185") and `local/research/engine-foundations/analysis-882-883-cell-load-cost.md` ("Ties to #185").
- Prior research to reuse (read, don't redo): `local/research/engine-foundations/research-995-engine-render-path-report.md`, `websearch-993-bevy-gpu-driven-report.md`, `websearch-1008-bevy-020-gaps-report.md`, `local/research/lod-hiding-under-loaded-cells.md`, `docs/research/skyrim-lod-formats.md`. Search `local/research/INDEX.md` for "terrain", "batching", "multi-draw".
- Record reference rule: for any LAND field (VHGT, VNML, VCLR, BTXT, ATXT, VTXT), cite how xEdit defines it, "xEdit 4.1.5f defs, Core/wbDefinitionsTES5.pas:<line>". The file at tag xedit-4.1.5f: https://raw.githubusercontent.com/TES5Edit/TES5Edit/xedit-4.1.5f/Core/wbDefinitionsTES5.pas
- Engine is Bevy 0.19. For any Bevy 0.19 gap you find (for example multi-draw indirect, bindless textures or texture arrays for terrain layers), check Bevy 0.20, main and open PRs and mark it "in 0.20: yes / PR #N / no".

# Questions
1. Where exactly does the draw count come from: one draw per quadrant mesh (4 per cell)? Are materials unique per quadrant (so Bevy cannot batch them)? file:line.
2. Options, with their trade-offs: (a) merge the 4 quadrants of a cell into one mesh, with the layer textures in a texture array and per-vertex or per-texel layer indices and weights; (b) merge N x N cells into one chunk mesh; (c) a single splat-map texture per cell or chunk instead of per-quadrant layer lists; (d) keep the meshes and rely on GPU-driven batching (bindless, multi-draw), no converter change. For each: the expected draw count for the measured ring, what the converter writes, what the engine reads, the memory and disk cost, and the effect on streaming granularity (load and unload per cell) and on collision.
3. How do other engines and Skyrim tools do it (Skyrim's own BTR terrain LOD, Unreal and Unity terrain, open-source Bevy terrain crates)? Cite sources.
4. How would the output fit #185: key, storage unit, rebuild trigger, fallback when missing.
5. Recommend one option for the converter, with a rough size (small / medium / large) and what Phase 2 would need to build.

# Scope
Read only. No cargo, no engines, no file writes. Web research allowed. Never open, list or size anything inside a `SkyrimConverted*` folder.

# Report
Return the full findings note (cited, estimates marked "estimate"; the lead files it as `local/research/engine-foundations/research-890-terrain-merge.md`), starting with a summary under 500 words: answers 1-5, a comparison table (option | draws for the ring | converter output | engine change | cost | #185 fit), and the one-line recommendation.
