# research-004 handoff - terrain blending and grid seams

Task: find the root cause of the two terrain defects in the first real-asset acceptance
screenshots and write a fix recipe. Note: `docs/research/terrain-blending-seams.md`.

## Verified

- The whole weight path, with `path:line` at every step: `VTXT` 8-byte entries parsed at
  `crates/converter/src/esm/cell_cache.rs:189`, sparse `(vertex, opacity)` stored in the cell
  cache (`crates/shared/src/lib.rs:21`, `crates/engine/src/world/cache.rs:75`), densified into
  a 17x17 slot grid (`crates/engine/src/streaming.rs:1462`), three weights packed into
  `ATTRIBUTE_TANGENT` as `(w / |w|, |w|)` (`crates/engine/src/streaming.rs:1503`), two into
  `ATTRIBUTE_UV_1` (`crates/engine/src/streaming.rs:1511`), reconstructed per fragment with the
  sixth weight derived (`crates/engine/src/shaders/terrain.wgsl:38`). There is no weight
  texture and no filtering of the weight field anywhere.
- Real `VTXT` data: sparse (52 of 289 points in quadrant 0 layer 0) and hard-masked
  (opacity exactly 0.0 or 1.0 at most samples), hence 128-unit jumps at layer boundaries.
- Bevy 0.19 re-normalizes `ATTRIBUTE_TANGENT.xyz` in the vertex shader, which is why the mesh
  pre-normalizes and stores `|w|` in `w` - and why the reconstructed weight ramps as `t^2`
  instead of `t` (0.25 instead of 0.5 at the middle of a 0 -> 1 edge). This is the
  "nearest-neighbour" sharpening of defect A.
- Heights and normals are *exactly* continuous across cell boundaries in the real plugin
  (`VHGT` 0/33 samples differ, `VNML` 0/99 components differ on all four edges of cell (0,0));
  `VCLR` differs only on the east edge (max 69/255, 3 of 99 bytes). Cracks, normal seams and
  vertex-colour seams are ruled out as causes of the grid lines.
- Cell (0,0) of Tamriel, the rural frame, has a water plane at `-6070` while its terrain spans
  `-6208..-5576`: 130 of 1089 samples are below the water and the nearest is **2 units** away.
  The water planes in the 5x5 loaded block have vertical edges only at world X = 0, 4096, 8192
  and 12288, which project to image x = 685, 915, 1165, 1411 - the four persistent lines I
  measured (685, 915-932, 1151-1180, 1411) and nothing in between.
- The Helgen grid (4,-21) and all eight neighbours have no water plane at all, so nothing in
  the water hypothesis can explain that frame: its snow blockiness is defect A.

## Unknown / not measured

- No pixel measurement of defect A's 7-px staircase: in rural.png the hard edges belong to the
  water plane, and in dense.png the crisp pixelation reads as texture content. Needs a
  capture with the water hidden, or a dry flat cell from the same camera.
- The water shader's reflection texture was not inspected at capture time; the white patch is
  consistent with the fallback image being bound, but a magenta fallback would prove it.
- The fine z-fighting grid was not measured period by period; the 128-unit (7 px) figure comes
  from the depth-precision arithmetic (477 units of error at 20000 units, reversed-Z, near
  0.1) and the terrain's triangle size.
- The `VTXT` position encoding (`y * 17 + x` versus `x * 17 + y`) is undecided: every real
  value fits both, and a transposed patch is still a coherent patch.

## Scripts written (read-only analysis, under `tools/research/`)

`land_vtxt.py`, `land_edges.py`, `cell_water.py` - read `Skyrim.esm` (never write).
`terrain_enhance.py`, `terrain_bands.py`, `terrain_steps.py`, `terrain_boundary_steps.py`,
`terrain_world_map.py`, `terrain_seam_measure.py`, `terrain_crop.py` - pixel measurements on
the acceptance screenshots; crops are written to `local/research-004/` (git-ignored).

## Next step

Give the implementer section 4 of the note: step 1 (weights into a 17x17 clamped weight
texture, no cache version bump) fixes defect A; steps 2-3 (skip/clip the per-cell water plane
and give the reflection a definition) fix defect B. Tests 1-4 are CPU-side and fail today;
test 5 is the off-screen check that proves the grid lines came from the water plane.
