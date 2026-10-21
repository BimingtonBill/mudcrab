# Goal
Trace what loading ONE cell's models and textures costs the engine, and list what the converter could output to make it cheaper. Phase 1 track. Code reading only. Goal context: the portal demo must hold 160 fps (6.25 ms frames) while travelling and through load doors, without lowering settings or distances. So any per-load work that lands on the main or render thread, or stalls asset readiness, matters.

# Context
- Read the engine and converter in this checkout, `C:/Modding/Mudcrab/mudcrab` (branch `portal`, the demo build). Engine: `crates/engine/src/` (`streaming.rs` model spawn and the GLB scene path, materials, textures/KTX2 handling, collision/physics from GLB `scenes[0].extras.openSkyrimCollision`). Converter output formats: `crates/converter/src/` (mesh.rs GLB writer, texture/KTX2 encoding, the texture encoder options incl. the GPU encoder, cache.rs configuration).
- Prior measurements (read, don't rerun):
  - `local/research/texture-load-cost.md`: UASTC to BC7 transcode measured at 56.5 ms mean per texture, against 0.6 ms to copy a stored BC7.
  - `local/perf-baseline/2026-10-06-main-final/baseline.md`: model readiness of about 0.9-2.8 s per scene load.
  - `local/research/engine-foundations/research-995-engine-render-path-report.md`.
  - `local/research/pacing-prior-art.md`.
- Find out which texture format the portal demo's assets actually use. Look at the conversion settings recorded in a converted output's manifest, e.g. `C:/Modding/Mudcrab/SkyrimConverted-portal-lod/conversion-manifest.json`: read only its top-level fields, not the whole entry list. Do not open, list or size any other part of a `SkyrimConverted*` folder.

# Questions (file:line evidence for each)
1. GLB load per model: parse (glTF JSON, buffers), the vertex formats the converter writes versus what Bevy needs (any conversion or attribute generation at load, such as tangents, normals, index widening, vertex colour conversion), the number of entities spawned per model (node hierarchy depth), and material creation per instance versus shared.
2. Textures: the format on disk (UASTC KTX2 + zstd, BC7 native, etc.) for the portal assets; what Bevy does at load (transcode, zstd decompress, mip generation), on which thread; and the bytes uploaded to the GPU per texture. Use the measured transcode cost to estimate the per-cell cost for a typical interior (count the distinct textures per cell if the code or the DB makes that possible; otherwise estimate and say so).
3. Collision: what is parsed from GLB extras per load, and the collider construction cost (convex hulls computed at runtime?). Could the converter store ready-made collider data?
4. Anything else recomputed per load that the converter could precompute: bounds/AABBs, LOD selection data, flattened node transforms, merged static meshes per model.
5. Rank the converter-side candidates (output native GPU formats, pre-generated tangents or mips, flattened scene hierarchies, prebuilt colliders, etc.). For each, give an estimated saving with reasoning (say "estimate" plainly), the thread it frees, the disk/size trade-off, and the engine change needed (Phase 2 owns the engine; just describe it).

# Scope
Read only. Change nothing. No cargo, no engines, no builds.

# Report
Under 700 words: answers 1-5 with file:line evidence, then a ranked table: candidate | converter output | work removed per cell | estimated saving | thread | size trade-off | engine change needed.
