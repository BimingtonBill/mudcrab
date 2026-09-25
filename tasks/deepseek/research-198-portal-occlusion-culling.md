# research-198 - Occlusion culling: what the engine culls today, what Skyrim's own data offers, and a design

Track: both. The doorway parts are portal; the main-view parts are main engine (hand them to Phase 2). Read-only: change nothing.

## Goal

The user: "occlusion culling and rendering only what the player sees", for the whole view and not only through doorways. Find what is culled today, what Skyrim itself uses, and design the cheapest effective culling.

## Questions

1. **Today.** All three cameras have `OcclusionCulling` and `DepthPrepass` (main `app.rs` ~1598-1634, portal `portal.rs` ~1146-1178, water reflection `render.rs` ~577-601), plus frustum culling.
   - Does Bevy 0.19's two-phase GPU occlusion culling actually cull our meshes? It needs `Aabb`s, and has requirements on material and mesh types; check whether our streamed glTF meshes, the terrain quadrants, the effect palette and snow materials qualify (`bevy_pbr` / `bevy_render` `gpu_preprocess`, `mip_generation`).
   - Can we read how many instances it culls (Bevy's `GpuPreprocessingSupport`, indirect parameters, or a debug counter)? If there is a way, say exactly how, so a coder can log it.
2. **Skyrim's own data.**
   - Interiors are split into rooms by `RoomMarker` references connected by `PortalMarker`s (`XRMR`, `XLRM`, `XPOD`, `XPTL` subrecords on REFRs).
   - Exteriors carry occlusion planes (`PlaneMarker` / occlusion box references, `XOCP`) and multibounds (`BSMultiBoundNode` in NIFs; research-190 mentions portal code handling them).
   - Does the converter keep any of this? Look at `crates/converter/src/esm/`, the world DB (`C:/Modding/SkyrimConverted/skyrim_world.db`, read-only `mode=ro`: count references whose base is a RoomMarker, PortalMarker or PlaneMarker, and whose data blob contains XRMR, XPOD or XOCP) and `docs/`.
   - How many interiors in Skyrim.esm have room markers?
3. **The doorway (portal).**
   - After impl-188 (skip when off screen) and impl-189 (render only the doorway's screen rectangle, `sub_camera_view`), what is left? The doorway can be on screen but hidden behind a wall.
   - How can we skip the portal render then? Options: a ray cast to the doorway centre and corners (`MeshRayCast`, as player.rs uses); reading the main view's previous-frame depth pyramid (`ViewDepthPyramid`, already imported in render.rs) at the doorway rect; or Bevy's own occlusion of the doorway quad (can a camera be switched on by another view's visibility result?).
4. **Design, ranked by payoff and effort:** for each step, the change, the expected saving, the risk, and the track. Include:
   - room/portal culling for interiors, from Skyrim's markers;
   - occlusion planes for exteriors;
   - the doorway-occluded skip;
   - any fix that makes Bevy's existing GPU culling actually apply.

## Context

- Under `local/`, read only `local/research/frame-cost-audit-2026-09-25.md` and `local/agents/research-175.out`.
- Bevy source: `C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.
- Skyrim data (read-only): `C:\Games\Steam\steamapps\common\Skyrim Special Edition\Data`.
- Existing ESM helper: `tools/research/esm_route.py`, whose `walk` gives each record's flags and payload. Put any script you write in your scratch folder.

## Report

Answers 1-3 with file:line and counts, then the ranked design (4).
