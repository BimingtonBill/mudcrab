# Goal
Trace what loading ONE cell costs the engine on the world-data side, and list what the converter could precompute to make it cheaper. Phase 1 track. Code reading only. Goal context: the portal demo must hold 160 fps (6.25 ms frames) while travelling and through load doors, without lowering settings or distances. So any per-cell work that lands on the main thread, or that repeats per load, matters.

# Context
- Read the engine and converter in this checkout, `C:/Modding/Mudcrab/mudcrab` (branch `portal`, the demo build). Engine: `crates/engine/src/` (start at `streaming.rs`, `world/database.rs`, `world/cache.rs`, `world/*`). Converter: `crates/converter/src/` (esm/exporter.rs, integration.rs, pipeline.rs). Shared types: `crates/shared/src/`.
- Measured numbers to anchor against (read, don't rerun):
  - `local/perf-baseline/2026-10-06-main-final/baseline.md` and `baseline.json`: per-scene timings, including model readiness of about 0.9-2.8 s per scene load and DB waits of 30-130 ms.
  - `local/research/engine-foundations/analysis-1009-baseline-cpu-breakdown.md`: CPU by schedule/span.
  - `local/research/engine-foundations/research-995-engine-render-path-report.md`: how a cell is drawn.
- Related design: `D:/Mudcrab/worktrees/p1-main-ro/docs/research/derived-data-input-keys.md` (#185: per-cell derived data keyed on content hashes; "Measured" section).

# Questions (file:line evidence for each)
1. The DB path for one interior cell and one exterior cell: every SQL query issued (text, table, which index it uses or a missing index), how many per cell or per reference (N+1 patterns?), on which thread (main/ECS system vs async task), and what is decoded per row (the `references.data` rkyv blob? subrecord parsing at runtime?).
2. `cell_cache.rkyv` (LAND): what is read per cell, and what runtime work follows (terrain mesh build, normals, splat/texture blending, collider heightfield)? Which of that is deterministic from converted data and could be stored ready-made?
3. Everything recomputed per load from data that never changes between loads: transforms (Creation to Bevy), bounds, light setup, water, door links, colliders built from GLB extras, material lookups, name/model path resolution. For each, roughly how much work per reference, and whether it runs on the main thread.
4. Anything done per frame for loaded cells that depends only on static data (could be precomputed).
5. Rank the candidates the converter could precompute (an extra table or column, a per-cell packed record, a prebuilt terrain mesh, an index). For each, give an estimated saving with your reasoning (anchor to the baseline numbers where possible; say "estimate" plainly) and the main-thread share, and what the engine would have to change to use it (Phase 2 owns the engine; just describe it).

# Scope
Read only. Change nothing. No cargo, no engines, no builds.

# Report
Under 700 words: answers 1-5 with file:line evidence, then a ranked table: candidate | what the converter writes | work removed per cell | estimated saving | main-thread? | engine change needed.
