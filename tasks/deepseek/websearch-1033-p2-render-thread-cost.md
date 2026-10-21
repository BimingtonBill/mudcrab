# Goal
Find what cuts Bevy 0.19's render-thread CPU cost (extract, prepare, queue/specialize, and command encoding/submit) for a world with many static meshes, so Mudcrab's 160 fps goal (6.25 ms per frame while travelling) can be met without lowering settings. Mark every technique "in Bevy 0.20: yes / PR #N / no".

# Context
- Mudcrab: an open-source Skyrim engine on Bevy 0.19 (wgpu 29), RTX 3090, Vulkan in timing runs. The world is mostly static meshes from converted Skyrim cells, one entity per mesh primitive.
- Measured (our profiler, render world spans): travel frames over 6.25 ms are all owned by the render thread. render/graph_and_present 4-8.6 ms, render/prepare 2.6-5.5 ms, about 70,000 mesh entities in view range. The terrain ring alone: 14.2 ms with 1,156 terrain patches versus 7.2 ms with 100. GPU passes themselves total under 1 ms.
- Already known (read these first, do not repeat them): C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/websearch-993-bevy-gpu-driven-report.md (Bevy 0.19 has GPU culling + multi-draw indirect on by default, occlusion culling, partial bindless, experimental meshlets) and websearch-994-moddable-baking-report.md (precombines/HLOD/instancing trade-offs for moddable worlds). This report must go deeper on the CPU side.
- Related upstream issue: #186 (static rendering experiment).

# Scope
Web research only (Bevy source on GitHub, docs.rs, Bevy release notes and migration guides for 0.19 and 0.20, Bevy PRs and issues, Bevy Discord summaries quoted in issues, talks/blogs from Bevy rendering contributors). Read-only on the project. Write one report file: C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/websearch-1033-render-thread-cost.md.

# Questions (answer each, with sources)
1. Where does per-entity render-world CPU time go in Bevy 0.19 for a static Mesh3d + StandardMaterial entity: extraction (which systems, change detection, `RenderMeshInstances`), prepare (bind groups, mesh/material preparation, uniform buffers), queue/specialize (pipeline specialization per view per phase, binning, `BinnedRenderPhase`), and command encoding? What is cached across frames for unchanged entities, and what is redone every frame?
2. Retained/cached rendering: which 0.19 mechanisms skip work for entities that did not change (retained render world, cached specialization, "unchanged bins"), and what breaks the cache (per-view changes, Visibility changes, transform changes)? What does 0.20 add?
3. Batching and instancing of static meshes: automatic batching rules in 0.19 (same mesh + material + pipeline), what prevents batching (different material instances, lightmaps, per-instance data), manual GPU instancing options, and how multi-draw indirect changes per-draw CPU cost.
4. Mesh merging per cell or chunk: costs and benefits (CPU draw count vs culling granularity vs memory vs moddability), and what Bevy offers or lacks for it.
5. Fewer entities: the cost per entity of the visibility pipeline (`check_visibility`, `VisibleEntities`, `RenderLayers`, AABB culling on the CPU vs GPU culling), and of shadow views (cascades × entities). Any known per-view scaling problems (several cameras: reflections, shadows).
6. Large counts of small terrain patches (about 1,000 patches): why 1,156 patches might cost 7 ms more render-thread time than 100, and the fix options (merging, texture arrays, a single terrain draw).
7. Profiling: which Bevy features/tools show the render-thread breakdown per system (tracing spans, `trace_tracy` alternatives), useful for confirming fixes.

# Done when
Every question has an answer with at least one primary source link (code, PR, docs), every technique has its "in Bevy 0.20: yes / PR #N / no" mark, and the report ends with a ranked list of 5-8 concrete changes Mudcrab could try, each with expected impact and risk.

# Report
The report file in the shape above (sources inline), and in your final answer: the ranked list plus any claim you could not confirm from a primary source, marked UNVERIFIED.
