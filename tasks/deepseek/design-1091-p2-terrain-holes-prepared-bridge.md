# Goal
Design the root fix for ground near the camera that doesn't load or shows holes (#1055, queue item C1). The user called it important on 2026-10-08. Track: Phase 2. Design only: read the code, write the design, change no code.

# Context
- Prior work (read first):
  - local/research/engine-foundations/analysis-1055-terrain-missing-cells.md: causes on main f40ca1df. LOD tiles hide on the CPU-side `TerrainSurfaceReady` while `RenderAssetBytesPerFrame` still defers the GPU prepare, so neither draws; `--shots` settle ignores LOD and GPU prepare; `CellStatus::Failed` is sticky; there is a silent terrain-less exterior cell.
  - local/research/engine-foundations/websearch-1056-bevy-prepared-feedback.md: Bevy 0.19/0.20 has no render-to-main "prepared" signal. The clean bridge is an ExtractSchedule system reading `RenderAssets`/`MeshAllocator` with `ResMut<MainWorld>`.
- Code: the checkout C:/Modding/Mudcrab/mudcrab, branch `portal` (head at least 2c7a0ea0). Since analysis-1055, portal has:
  - impl-1057's bounded failed-cell retry;
  - impl-190/196's LOD ring changes (the ring follows the worldspace's LOD data);
  - impl-1059's unload pacer;
  - impl-1073's landing-cell prewarm gate.
  impl-1060/1060.1, which paces model loads and keeps queued loads pending, is on phase2/asset-load-pacing and phase2/prewarm-waits-on-loads, not yet in portal.
- Files: crates/engine/src/streaming.rs (`track_surface_readiness`, `TerrainSurfaceReady`, `validate_surface_dependencies`), streaming/lod.rs (`full_ready`, `select_terrain_lod_tier`, the visibility pass; Chef's area upstream), shots.rs (`SettleCounts`, `is_quiet`), render.rs and app.rs (`RenderAssetBytesPerFrame`), the acceptance and world-ready settle code.
- Bevy 0.19 sources: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ (bevy_render, bevy_pbr, bevy_asset).

# Scope
Read only; change no code. Write the design to local/research/engine-foundations/design-1091-terrain-holes-prepared-bridge.md.

# Done when
The design answers each of these with file:line:
1. **The causes on portal's head.** For each cause in analysis-1055, does it still hold on portal, changed, or fixed? Cover the 1057 retry, the 190/196 LOD ring, and any new path, with evidence.
2. **The prepared bridge.**
   - An ExtractSchedule (or render-world) system that, for main-world entities marked as waiting (terrain quadrants, merged terrain cells from #186, LOD chunks, later any swap), checks whether their mesh is allocated (`MeshAllocator` / `RenderAssets<RenderMesh>`), their images are prepared (`RenderAssets<GpuImage>`), and their material bind group is prepared (`RenderMaterials` / the erased render assets for our material types). When all are, it marks them drawn in the main world through `MainWorld`, with at most one frame of latency.
   - Which Bevy 0.19 types and functions exactly, how the main-world marker is written safely during extract, and what happens on a pipeline not compiled yet (a mesh whose pipeline is not ready is not drawn either: include `PipelineCache` readiness, or say why not).
   - It must be general: any "swap A for B once B is drawn" uses the same marker (terrain now, merged terrain and object LOD later).
3. **The LOD swap.** LOD hides a tile only when its full-detail replacement is drawn, not on `TerrainSurfaceReady`, and the reverse on unload: full detail stays until LOD is drawn. No frame shows neither.
4. **The settle.** `--shots`, acceptance and world-ready wait on the same drawn signal, plus pending LOD queries and chunks, so a capture never photographs a hole. No frame counts and no timeouts that pass silently.
5. **Cost.** Per-frame work scales with the number of pending items, not with all terrain: a pending set, removed on drawn. Estimate the cost at a crossing's worst pending count.
6. **The remaining causes.** The sticky failed cell (1057 covers what?) and the silent terrain-less exterior cell (count it, make it visible). Propose small, separate fixes.
7. **Tests.** Which unit and app tests prove it, including a test where the upload budget defers a quadrant's images, and LOD stays visible until they are prepared.
8. **The work split.** Separate commits or branches, which parts touch lod.rs (Chef's area upstream: announce before sending), and the order.

# Foundations check (answer explicitly)
No delaying or hiding of loading (the budget stays; nothing is dropped; LOD covers until real detail draws). Data-driven, works for any worldspace and mod, cost scales with pending items. What shortcut would a quick fix take here (a frame-count settle, raising the budget, forcing a prepare), and why is it rejected?

# Report
Status / Summary / Left / Next, then the design file path and a 5-line summary of the design.
