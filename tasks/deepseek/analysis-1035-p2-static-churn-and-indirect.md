# Goal
Two code-reading checks for Mudcrab's 160 fps goal (render-thread CPU is the bottleneck): (a) is Bevy 0.19's GPU-driven indirect drawing path really active for static world meshes, and (b) does any system touch static entities' Transform, GlobalTransform or Aabb (or other extracted components) every frame, so Bevy's change detection marks them changed and re-extracts them?

# Context
- Checkout: C:/Modding/Mudcrab/worktrees/p2-main (origin/main f40ca1df, detached). Rust, Bevy 0.19.0 (bevy_pbr / bevy_render sources in C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ for checking what Bevy keys its extraction on).
- Static world = cell references spawned by streaming (crates/engine/src/streaming.rs, app.rs, render.rs, world/*). Background: C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/websearch-1033-render-thread-cost.md.
- Known valid per-frame movers: the camera, actors, doors animating, physics bodies that move, water/sky. Static references, terrain patches and LOD chunks should not change after spawn.

# Scope
Read only. Change nothing.

# Questions (file:line for each)
(a) 1. Any NoIndirectDrawing, NoAutomaticBatching, NoCpuCulling/NoFrustumCulling, or anything that disables GPU preprocessing (GpuPreprocessingSupport, settings on RenderPlugin/PbrPlugin, WgpuSettings features/limits such as MULTI_DRAW_INDIRECT(_COUNT)) anywhere in the engine. 2. Which backends we run (Vulkan in timing runs, DX12 default): what Bevy 0.19 decides about GPU preprocessing and multi-draw on each (read bevy_render's gpu_preprocessing support detection) and what that means for us. 3. Anything that makes static meshes ineligible for batching: per-entity unique materials, MeshTag, lightmaps, per-entity material handles cloned vs shared, different RenderLayers per entity.
(b) 4. Every system that runs every frame and takes `Query<&mut Transform>`, `&mut GlobalTransform`, `&mut Aabb`, `&mut Visibility`, `&mut ViewVisibility`, `&mut Mesh3d`, `&mut MeshMaterial3d<_>` or uses `Mut`/`set_changed`/`insert` on static world entities, including through broad queries that deref-mut without real changes (a `for mut t in &mut query { t.translation = ... }` that writes the same value still marks it changed). Include origin rebasing (RenderOrigin shifts), visibility/fade systems, LOD ring hiding, room culling, water, and anything run on all children of a cell root. 5. Transform propagation: does a change on a cell root (e.g. a rebase moving the root) dirty every child's GlobalTransform each frame or only on rebase? 6. What Bevy 0.19 re-extracts when GlobalTransform/Aabb/Visibility change (read bevy_pbr's extract_meshes_for_gpu_building change filters) so we know which writes matter.

# Done when
Every question is answered with file:line in our code (and Bevy's where relevant); each per-frame writer on statics is listed with how often it really changes values, and whether it writes unconditionally; and you give a short ranked fix list (root-cause fixes only: write only when the value changes, use `set_if_neq`, gate on `Changed`, move statics out of a broad query).

# Report
(a) findings, (b) findings table (system, file:line, query, runs when, writes unconditionally?, entity count affected), then the ranked fix list, then what you could not determine.
