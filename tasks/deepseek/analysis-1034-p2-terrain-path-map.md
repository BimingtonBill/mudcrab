# Goal
Map exactly how Mudcrab's engine builds and shades exterior terrain today (main branch), so Phase 2 can specify a merged per-cell terrain mesh plus one shared terrain material without changing the look.

# Context
- Checkout: C:/Modding/Mudcrab/worktrees/p2-main (origin/main f40ca1df, detached). Rust, Bevy 0.19.
- Measured problem: about 1,156 terrain patch entities (4 quadrants per cell, 17x17 cells) cost about 7 ms of render-thread CPU; each quadrant has its own material, so Bevy cannot multi-draw them. Plan (approved): the converter writes one merged mesh per cell plus a per-worldspace land-texture array; the engine draws all terrain with ONE shared material.
- Background: C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/research-890-terrain-merge.md (record layouts BTXT/ATXT/VTXT/VHGT/VNML/VCLR; quadrant numbering).

# Scope
Read only. Change nothing.

# Questions (answer each with file:line)
1. Mesh build: where and how the 4 quadrant meshes per cell are built from cell_cache.rkyv (vertex count per quadrant, shared/duplicated edge vertices, positions, normals, vertex colours, UVs and their tiling, any skirts or seams handling, index layout).
2. Material: the terrain material type (ExtendedMaterial? custom Material?), its bind group layout, every uniform/texture it carries, and what makes each quadrant's material unique (base texture, layer textures, layer weights as textures or vertex data?).
3. Shader: the terrain WGSL. How base and up to 8 ATXT layers are blended (order, normalisation, alpha rules), how normals/normal maps are applied, vertex colour use, any special cases (missing texture fallbacks, default land texture, sRGB handling, the .opensky-srgb aliases), and the texture sampling (UV scale per layer?).
4. Textures: the land texture (LTEX -> TXST) resolution, format (BC/KTX2), mip counts and colour space as converted; are they uniform enough for a 2D texture array, and what would need resizing.
5. Layer counts: how the engine represents per-vertex layer weights today, and the maximum number of layers per quadrant it supports.
6. Physics: the terrain collider per quadrant (shape, build site, cost) and what depends on quadrant granularity (raycasts, ground follow, streaming bookkeeping, `pending_surface_instances`, LOD ring hand-off, water).
7. Everything else that assumes 4 terrain entities per cell: streaming counters, metrics (`Terrain patches` in the baseline), tests, the LOD near/far seam, the terrain-patch count in reports.

# Done when
Every question has file:line answers, and the report ends with: the minimal per-vertex and per-worldspace data a shared material would need to reproduce today's look exactly (attribute list with formats), and a list of every code site that must change.

# Report
Findings per question, then the data list, then the change-site list, then anything you could not determine.
