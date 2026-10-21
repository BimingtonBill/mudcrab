# Streaming, crossings and hardware scaling: future priorities

Recorded 2026-10-18 from the cluster demo work (branch `opt/cluster-showcase-integration`). These are
engine priorities for **after** the demo reaches its passing grade (smooth play, at least 120 fps
average). Nothing here is scheduled yet.

## Governing preferences (the user, 2026-10-18)

- Front-load as much data as possible. The initial load may take time and stutter; what counts is
  smooth gameplay after the world is first ready. Judge performance on the frames after the first
  ready latch, not on time-to-ready.
- Low-end hardware (older Android devices) is a far goal, not a current constraint, but no
  foundation may rule it out. Older hardware without the cluster path's GPU features uses baked LOD.

## Evidence these come from

Riverwood, clusters off, load-screen doors, Tracy capture `D:/Mudcrab/perf/2026-10-18-195601-lodslots-startup`:

- Distant LOD requests at one per frame: ~400 requests (184 terrain chunks, ~210 object and tree
  blocks) took ~10 s to fill at 60 fps. At 16 per frame they took 3.6 s, but frames during the fill
  ran 100-480 ms.
- The cost is not the request. It is the work after it: `world_instance_spawner_system` (glTF scene
  spawn) up to 305 ms in one frame and 5.7 s in total; asset bookkeeping systems (`track_assets` for
  many asset types) peaked at 120-155 ms while hundreds of loads were in flight; the glTF loader's
  `generate_tangents` costs up to 8 ms per model at load time.

## Priorities

Recommended order: 1, then 2, then 4.

1. **Crossing measure.** Add to the demo tour: worst frame and 99th-percentile frame time in the 2 s
   after each crossing, clusters on and off, both door modes. The baseline that every item below is
   judged against.
2. **Memory-budgeted residency (front-loading).** One residency manager with a memory budget instead
   of per-system count caps. At startup, load everything the player can reach that fits the budget,
   nearest first, and evict least recently used beyond it: all distant LOD for the worldspace, the
   exterior kept while inside an interior (`--lod-keep-hidden-in-interiors` exists), recently
   visited interiors kept. On a desktop GPU the budget holds everything (all worldspaces' native LOD
   is ~474 MB); on a small device it holds the nearest part.
3. **Prepare door destinations early.** Near a load door, load and upload the destination in the
   background, hidden, including its distant LOD. A crossing then switches visibility only. Seamless
   doors prewarm part of this already; extend it to load-screen doors. Bounded by the residency
   budget.
4. **Move runtime work into the converter.** Write tangents into the converted meshes (the same kind
   of fix as terrain tangent prep, 34.7 s to 0.8 s). Pack many small LOD files into fewer
   per-region files to cut asset counts and bookkeeping. Write platform texture formats (BC for
   desktop, ASTC for Android).
5. **No full scene spawn for static objects.** Spawn plain mesh-and-material entities for static
   references and LOD blocks instead of glTF scene hierarchies, or draw them through the cluster
   path.
6. **Budgets by measured cost and frame target.** Replace per-frame counts (one cell, 32 models, one
   LOD request) with a per-frame time share for spawning and a byte budget for uploads, both a
   fraction of the target frame time (for example 2 ms of 16.7 ms at 60 fps, of 33 ms at 30 fps),
   using sizes the converter records, and shrinking when frames overrun.
7. **Load ahead of movement.** Order requests by the player's direction and speed; shorter look-ahead
   under a smaller residency budget.
8. **Hardware check and quality tiers at startup.** Probe GPU features (indirect first instance,
   storage-buffer limits for the cluster path), VRAM, RAM and cores. These set the residency budget,
   the frame target and the quality tier: texture size, shadow resolution, LOD reach, clusters on
   or baked LOD. The graphics presets exist; the tier picks one.
9. **Low-end check on this PC.** Run with a small residency budget and a 30 fps target and require
   crossings to stay smooth, as a stand-in until a device is available.
