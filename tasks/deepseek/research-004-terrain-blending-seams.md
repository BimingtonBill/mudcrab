# research-004 - Why real terrain renders with blocky blending and visible grid seams

Read `tasks/deepseek/context.md` first. This is a **research** task: you write documentation and may
write analysis scripts under `tools/research/`; you do not change engine or converter code.

## Goal

Find the root cause of two terrain defects visible in the first real-asset acceptance screenshots, and
write a fix recipe the lead can hand to an implementer. Phase 2 cannot be signed off with them
(milestone M4 in `docs/roadmap/02-completion-plan.md`: "ausência de rachaduras e bordas invertidas;
seis camadas coerentes com os dados de origem").

## Evidence - look at these images first

Screenshots from a real-asset run on commit `adb5b09` (top-down camera, Skyrim SE Tamriel worldspace
`0x3c`), in
`local/acceptance-shakedown/20260922-003549-AMD-Ryzen-7-5800X-8-Core-Processor--NVIDIA-GeForce-RTX-3090-driver-32.0.16.1692/screenshots/`:

- `rural.png` - grid (0,0). A white patch in the centre with a **staircase/pixelated** outline, and a
  **regular grid of lines** across the whole ground.
- `dense.png` - grid (4,-21), Helgen. Snow/rock transitions with blocky, stair-stepped edges.
- `water.png` - grid (21,-3). The same stair-stepped snow edges; faint grid lines.
- Compare `terrain-water.png` in the same folder (a synthetic fixture) - does it show either defect?

**Defect A - blocky blending:** layer transitions look like nearest-neighbour sampling of a coarse
weight grid rather than a smooth interpolation.
**Defect B - grid lines:** thin lines at regular spacing. Measure the spacing in pixels in
`rural.png`, and work out whether it matches cell boundaries (4096 units), quadrant boundaries
(2048 units; Skyrim `VTXT`/`ATXT` layers are per quadrant), or something else. The run's logs are in
the sibling `logs/` folder (`rural-run-1.log`) if you need the camera height or view setup.

## Where to look

- Terrain in the converter: `crates/converter/src/esm/` (`LAND`: `VHGT`, `VNML`, `VCLR`, `BTXT`,
  `ATXT`, `VTXT`) and `crates/converter/src/esm/cell_cache.rs` (what the cell cache stores per cell:
  the "six channels" of layer weights).
- Terrain in the engine: `crates/engine/src/streaming.rs` (terrain mesh generation: grep for
  `terrain`, `quadrant`, `weights`, `layer`), `crates/engine/src/world/cache.rs`, `crates/engine/src/render.rs`,
  and any terrain shader (`Glob` for `*.wgsl` across the repo).
- Existing tests already named for this: `accepts_matching_neighbor_edges_and_rejects_cracks`,
  `creates_upward_wound_quadrants_with_continuous_uvs` (streaming tests). Say what they cover and
  why they did not catch A and B.
- Bevy 0.19 source in the cargo registry for sampler defaults (`ImageSampler`, filter modes,
  address modes, mipmaps) - check the real defaults, do not recall them.
- The real converted data at `C:/Modding/Mudcrab/SkyrimConverted` (read-only reference folder): you may
  read `skyrim_world.db` with a script (`sqlite3.connect('file:...?mode=ro', uri=True)`) to look at
  the `land` rows of cell (0,0) in worldspace 60, e.g. how many weight samples per quadrant exist.

## Questions to answer

1. How the layer weights travel: from `VTXT` (per-quadrant, 17x17 points, sparse) through the
   converter and cell cache into the GPU - their resolution at each step, and how the shader samples
   them (texture with which filter? per-vertex attribute? which interpolation?). Where does the
   staircase come from? Cite `path:line` at every step.
2. What draws the grid lines: UV seams between quadrant meshes, texture address mode (repeat vs
   clamp) at the borders of a weight texture, mip/border bleeding, normals discontinuous at quadrant
   edges, a debug overlay, or something else. Cite the code.
3. How Skyrim itself blends (the base layer `BTXT` per quadrant plus up to 8 additional layers,
   each with opacities at 17x17 points, bilinear across the quadrant). Cite a public format
   reference (UESP LAND page) for the layout.
4. The fix, as ordered steps an implementer can code from: which files and functions change, the
   data format change if any (and whether the cell cache version must bump), and a **test that fails
   today** for each defect - preferably a deterministic synthetic fixture rendered or sampled
   off-screen, or a CPU-side check of the weights at quadrant borders.

## Scope

- Write only `docs/research/terrain-blending-seams.md`, `docs/handoff/research-004-terrain-blending-seams.md`,
  and scripts under `tools/research/`. Read-only everywhere else. Do not run the engine or `scripts/`.

## Done when

- The note answers 1-4 with `path:line` for every code claim, marks each claim verified or inferred,
  and includes your pixel measurement for defect B.
- `python tools/check_citations.py docs/research/terrain-blending-seams.md` reports nothing broken.
- Committed: `git add <paths>` then
  `git commit -m "[deepseek] research-004: terrain blending and seams root cause" -- <paths>`.

## Report

300-600 words for the lead: the root cause of each defect with the decisive `path:line`, how
confident you are, the fix steps, and the failing tests to write.
