# Goal
Build slices C1 and C5 of the approved #1055 design (terrain holes): the render-to-main "prepared and drawable" bridge, and the gauges that make the gap visible. Nothing uses the drawn signal yet for hiding or settling; that is C3, C4 and C2, which come later. This slice must change no behaviour except adding the markers and gauges. Track: Phase 2, branch `phase2/terrain-drawn-bridge`.

# Context
- The approved design, the contract: local/research/engine-foundations/design-1091-terrain-holes-prepared-bridge.md. Read §2 (the bridge), §2.3 (the pending set and cost), §2.4 (instrumentation), §5 (cost) and §7 (tests) closely. The approval includes the later rule that a settle timeout becomes a hard failure, but that is C4, not this slice.
- Precedents already in the tree to copy: `RendererProofBridge` (render.rs ~119-127, 159-233) and `RenderMeasurementProbe` (profiling.rs ~214-241, installed from metrics.rs ~60-96). Both publish render-world facts to the main world from `ExtractSchedule`.
- Bevy 0.19 (registry: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/):
  - `MeshAllocator::mesh_slabs` (bevy_render mesh/allocator.rs ~305);
  - `RenderAssets<RenderMesh>` and `RenderAssets<GpuImage>`;
  - `ErasedRenderAssets<PreparedMaterial>` (bevy_render erased_render_asset.rs ~197);
  - `SpecializedMaterialPipelineCache` (bevy_pbr material.rs);
  - `PipelineCache::get_render_pipeline_state` (pipeline_cache.rs ~276);
  - `MainWorld`, which exists only while ExtractSchedule runs.
- What gets marked in this slice: terrain quadrant patches (`TerrainPatch` entities, `track_surface_readiness` in streaming.rs). Insert `SwapWaiting` where a patch is spawned with `PendingTerrainProfile`. Insert nothing on any other entity yet; the marker is generic and later slices add more users.

# What to build
1. A new module `crates/engine/src/swap_bridge.rs` with the components `SwapWaiting`, `SwapPrepared { stand_in: Option<Entity>, since: u32 }` and `SwapDrawn`, as the design names them (you may rename if the design's names clash; say why). Add a plugin that registers the ExtractSchedule system in the render sub-app, with a cached `SystemState` over the main world's waiting entities.
2. The bridge system. For each entity with `SwapWaiting`, it checks the mesh (allocated in `MeshAllocator` and present in `RenderAssets<RenderMesh>`), every image its material uses (in `RenderAssets<GpuImage>`), and the material (in `ErasedRenderAssets<PreparedMaterial>`), and inserts `SwapPrepared`. It inserts `SwapDrawn` once the entity's specialized pipeline for a view that can see it resolves to `CachedPipelineState::Ok`. Cost is O(waiting entities): no scan of all terrain.
   - The material's image handles are needed. Find the cheapest correct way to get them for our terrain material (`TerrainMaterial` / the ExtendedMaterial in render.rs), for example the main-world handles the patch already holds. Keep it general enough that other material types can be added. Say what you chose.
3. A main-world system that removes `SwapWaiting` when `SwapDrawn` appears, and keeps the count of entities waiting.
4. C5 gauges, recorded only when profiling (the impl-1077 pattern):
   - `swap/waiting`, `swap/prepared_not_drawn`, `swap/drawn_this_frame`;
   - the number of terrain patches that are `TerrainSurfaceReady` but not `SwapDrawn`. This is THE hole-risk gauge: it measures how long today's LOD-hide rule runs ahead of the GPU.
   Add them to FRAME_SERIES_COLUMNS.
5. Tests:
   (a) the bridge's decision function on stand-in data: each missing piece (mesh, image, material, pipeline) keeps an entity waiting, and all present makes it drawn. Make it a pure function over lookups so it is unit-testable.
   (b) an app test with the render sub-app if feasible, or a test of the main-world hand-over system with `SwapDrawn` inserted by hand.
   (c) a cost test: with N waiting entities and M other entities, the bridge visits only N.
   Say plainly which parts only a GPU run can check.

# Scope
Owned files: crates/engine/src/swap_bridge.rs, crates/engine/src/lib.rs, crates/engine/src/app.rs, crates/engine/src/streaming.rs, crates/engine/src/profiling.rs
- streaming.rs: only inserting `SwapWaiting` on spawned terrain patches, and the hole-risk gauge. app.rs and lib.rs: registration only.
- No changes to lod.rs (Chef's area; slice C2 waits for an upstream announce). Do not change any hide, despawn or settle behaviour (C3 and C4).
- No new dependencies. No git commands.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast
- Set BUILD_GATE_OWNER=p2. Run `df -h /c` first; stop if C: has less than 15 GB free. A timing hold may be active tonight: the gate waits, which is expected.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
No behaviour change; cost O(waiting); generic marker; nothing hidden or delayed.

# Report
- What you built; the exact Bevy types and calls used, with registry file:line.
- How the material's images are found.
- The commands and their Summary lines.
- What only a GPU run can confirm.
