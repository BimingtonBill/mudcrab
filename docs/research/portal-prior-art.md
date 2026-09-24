# Seamless portals: prior art (Valve's Portal, Immersive Portals, Bevy crates)

**Status:** web research, 2026-09-24. DeepSeek websearch-155 (Valve) and websearch-156 (Immersive
Portals and other projects); the full reports are in `local/agents/runs/websearch-15{5,6}*/`.
Claude spot-checked the Bevy light-layers issue (#23264: open, "RenderLayers do not work for
Lights", reported against 0.18.1). One worker also read a mirror of **leaked** Source engine code;
nothing here rests on it or links to it - the same points are made in Valve's own commentary.

Our design: a second camera renders the destination into a texture, a doorway quad samples it,
a rigid yaw map places the camera, a clip at the destination doorway, a one-frame swap on crossing.
The letters below are the open problems in `docs/roadmap/backlog-engine-and-portal.md`.

## The two families

- **Render to texture** (ours; Sebastian Lague's Unity project; the Bevy crates `bevy_easy_portals`
  and `bevy_basic_portals`). Simple, works with any engine, but the portal view is a separate image
  with its own post-processing, and it does not nest well.
- **Stencil into the main framebuffer** (Portal 1 as shipped, Portal 2 by default, Immersive
  Portals by default). The destination is drawn straight into the main image, masked to the portal's
  pixels. Portal 1 **started** with two offscreen textures and moved away: they were incompatible
  with antialiasing, cost video memory, and did not work well with HDR bloom (Kircher, Portal
  developer commentary, 2007). Immersive Portals keeps render-to-texture only as a compatibility mode.

Neither project found a reason to go back to textures; both chose the stencil path for quality and
cost. For us the texture path is what Bevy offers without renderer work, so it stays - but its known
costs (separate exposure/bloom, a whole extra frame, no nesting) are ours to manage.

## By problem

**A - the doorway image lags the camera.** No source calls this inherent. Valve never lags because
the view is drawn inline in the same frame. `bevy_easy_portals` orders its portal camera before the
main one (`order: -1`) and updates it in `PostUpdate` after transform propagation - same frame.
Hobbyist Unity ports that update "too late" report exactly our symptom. **Ours was a one-frame read of
the main camera's `GlobalTransform` in `Update` (fixed 2026-09-24, `portal.rs` `MainCameraQuery`).**

**B - performance.**
- Valve's headline optimisation (Kircher): render only what is visible through the opposite
  portal's field of view, and exclude everything between the virtual camera and the destination
  portal plane. Frustum culling on the portal's edges; distance-based LOD had to be measured through
  the portal.
- Recursion: shipped default 2, practical ceiling 9, deeper levels are copies of the previous
  frame. For a door preview, one level is enough.
- Portal 2's "world imposters" (McTaggart): one low-cost atlased texture of the level's lit
  surfaces, used for **distant portals and portals two levels deep**. A template for a cheap
  far-door preview.
- Immersive Portals: occlusion queries predicted from earlier frames so the CPU never waits, three
  separate frustum culls, and a conservative mode after mispredictions.
- **Caution:** Bevy #8777 (open since 2023, 0.10) - toggling `Camera::is_active` on portal cameras
  made performance *worse* and visibly "jumped". Measure on 0.19 before relying on "skip when not
  visible".
- **Oblique near plane: Immersive Portals tried and rejected it** ("does not work well with the
  things that use pixel depth"); it clips in the vertex shader instead. `bevy_easy_portals` sets the
  frustum's near half-space at the destination doorway. Worth knowing before changing our clip.

**C - crossing.**
- Immersive Portals teleports **before rendering each frame, when the camera crosses** the plane,
  not the player body.
- Collision at the threshold, Valve and Immersive Portals alike: trace on **both sides** and merge
  (Portal 2: the player box traced both sides of the portal; Portal 1: temporary physics "bubbles"
  around portals, 500 ms to 10 ms; objects cloned across). Relevant when NPCs and physics arrive.
- Depth clamp while drawing the portal shape stops wrong clipping when the camera is very close
  (Immersive Portals) - bears on "the camera can clip into the door frame".
- `bevy_easy_portals` copies `Tonemapping`, `DebandDither`, `ColorGrading` and `Exposure` from the
  main camera onto the portal camera, so both views grade the same.

**D - lighting across the doorway.** Bevy lights do **not** respect `RenderLayers` today (issue
#23264, open; fix PR #23925 unmerged, dropped from the 0.19 milestone, blocked on per-camera shadow
maps). So our "destination lights leak" item is a Bevy limitation, not our bug. Valve never moved
light through portals (baked lighting, soft ambient); the seam is hidden, not solved. Mitigations
for us: gate light energy by active space rather than by layer.

**E - streaming and lifecycle.** Nothing from Valve (whole levels loaded; they blocked portaling
across level loads). Immersive Portals loads chunks visible through portals and keeps "global
portals" always loaded.

**F - polish.** No source covers eye adaptation or vignettes for portals. Immersive Portals moves
sound through the nearest portal point, which is the audio half of "the ambient crossfade".

## Bevy code worth reading

- `bevy_basic_portals` 0.11.0 (Bevy 0.19, published 2026-09-13): visual only - no crossing, no
  recursion, one-way.
- `bevy_easy_portals` 0.2.1 (Bevy 0.14): the same-frame camera recipe and the frustum near-plane clip.

## The ideas worth acting on, ranked

1. Same-frame portal camera (done: the lag fix).
2. Treat D as Bevy's: gate destination lights by active space; watch PR #23925.
3. Before touching the clip plane, compare our clip with a frustum near half-space; avoid an oblique
   projection with a depth-based pipeline.
4. Copy the main camera's exposure and grading onto the portal camera (eye-adaptation continuity).
5. A cheap far-door preview (Portal 2's imposters) and portal-edge culling before the bigger
   performance ideas; measure `is_active` toggling on 0.19 first.
