# research-204 - Where a doorway's image meets its frame: what gives the seam away, and how to hide it

Track: portal. Read-only: change nothing.

## Goal

The user wants the doorway to be visually seamless: the edge where the doorway image (the portal quad, drawn from the portal camera) meets the real door frame should not show. List every visible artefact at that edge, give each a cause in the code, and design fixes, ranked.

## Material

- **Per-frame captures of two crossings**, fresh on the current build: `local/demo/flicker/walk-through/00` and `01` (PNGs, one per frame), plus `local/demo/flicker/*-door.png` and `*-arrived.png`. Look at the doorway edges closely: crop and zoom with Python/Pillow if installed; put scripts in your scratch folder.
- **The user's captures** with doorways in view: `local/captures/2026-09-25_03-57-58/` (01, 04, 06, 13, 14; `notes.md`). Some of these predate today's fixes (impl-189 sub-view, impl-199 threshold anchors, impl-192 double-sided leaves).
- **Code:**
  - `crates/engine/src/portal.rs`: the quad (`doorway_quad_transform`, the plug and quad sizing, `doorway_render_rect`, the oblique clip plane `doorway_clip_plane`, `portal_projection`, `fit_portal_view`);
  - `crates/engine/src/shaders/portal.wgsl`;
  - `crates/engine/src/doors.rs` (`DoorAnchor`, the doorway box);
  - `docs/research/portal-frames.md` and `docs/design/portal-visuals.md` if present.

## Questions

1. **Catalogue the seam artefacts you can see**, each with an image path and pixel coordinates. Candidates include, among others:
   - a gap or overlap between the quad and the frame;
   - a halo or line along the quad edge (sampling, clamp, filtering or aliasing);
   - a brightness or colour step across the edge (exposure, tonemapping, bloom or fog differ between the two views; the portal camera is HDR with `Tonemapping::None` since impl-202, and the main view tonemaps the quad);
   - the far side's own frame and wall showing inside the doorway, or clipped wrongly by the oblique plane;
   - shadows and lighting that stop at the doorway;
   - depth: the quad is a flat plane, so objects near the doorway (the swinging leaf, the player's camera close to the plane) cut or overlap wrongly;
   - bloom and effects that do not cross the edge;
   - the one-frame swap lag, which impl-203 is fixing (mention it, but do not re-analyse it).
2. **For each artefact, the cause** with file:line.
3. **The design, ranked by visible payoff against effort.** Consider:
   - a stencil/depth-masked doorway drawn from the doorway's real opening mesh instead of a rectangle;
   - a slightly oversized quad hidden behind the frame by depth;
   - matching exposure, tonemapping, fog and bloom across the two views (tonemap once, after compositing);
   - MSAA or edge filtering;
   - writing the portal image's depth so depth-based effects (fog, SSAO, depth of field) continue through the doorway.
   Say which parts Bevy 0.19 supports directly (check the source: `C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`).

## Context

- Under `local/`, read only the folders named above.
- A coder will implement your top items in portal.rs and portal.wgsl, so be concrete: the function to change and what to compute.

## Report

The artefact table (artefact | where it shows | cause | fix), then the ranked design in about 15 lines.
