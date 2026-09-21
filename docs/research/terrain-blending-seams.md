# Terrain blending and grid seams: root cause of the two acceptance defects

Task: research-004. Evidence: the real-asset acceptance screenshots of run
`20260922-003549` (commit `adb5b09`), top-down camera over Tamriel (worldspace `0000003C`),
plus the converted assets in `C:/Modding/SkyrimConverted` and the raw `Skyrim.esm` LAND
records. Every claim below is marked **verified** (I read the code, the bytes or the
pixels) or **inferred** (reasoning from those facts).

Summary:

- **Defect A (blocky/stair-stepped blending)** - the six layer weights reach the GPU as raw,
  unfiltered 17x17-per-quadrant samples. The real `VTXT` opacities are hard masks (0 or 1 at
  most samples), so a layer boundary *is* a one-sample jump, and the engine's way of carrying
  the third-to-fifth weights through `Mesh::ATTRIBUTE_TANGENT` is lossy under interpolation:
  it makes the reconstructed weight `t^2`-shaped instead of linear, sharpening every
  transition further. **Not** a texture-filtering problem: no weight texture exists.
- **Defect B (grid lines)** - the engine spawns a blended `4096x4096` water plane for every
  cell whose `CELL/XCLW` is a real height, at that height
  (`crates/engine/src/streaming.rs:470`). In the acceptance cell `(0,0)` the water is at
  `-6070`, the terrain spans `-6208..-5576`, 130 of 1089 height samples are *below* the
  water level and the closest sample is **2 units** from it (measured from `Skyrim.esm`).
  At the acceptance camera distance (20000 units up) the f32 depth buffer cannot separate
  two surfaces 500 units apart, so the water plane and the terrain z-fight over the flat
  ground; the winner is decided per terrain triangle (128 world units = 7 px) which is the
  regular grid, and the water plane's own edges - exactly at cell boundaries - are the
  coarse grid. The water shader mixes up to 80% of a reflection texture
  (`crates/engine/src/shaders/water.wgsl:38`), which is why the affected cell reads white
  rather than blue.

The synthetic `terrain-water` fixture shows neither defect because its camera is close
(depth precision is ~0.05 units there) and its weights are empty; `synthetic.png` is the
indirect-draw instance benchmark, not terrain.

## 1. How the layer weights travel

| Step | Resolution | Where |
|---|---|---|
| ESM `VTXT` | sparse list of 17x17-grid points per quadrant/layer | `crates/converter/src/esm/cell_cache.rs:189` |
| Cell cache (`rkyv`) | same sparse list, `(vertex: u16, opacity: f32)` | `crates/shared/src/lib.rs:21` |
| Runtime snapshot | same list, unpacked to `(u16, f32)` | `crates/engine/src/world/cache.rs:75` |
| Quadrant mesh | dense `17*17` grid per overlay slot | `crates/engine/src/streaming.rs:1462` |
| GPU | 5 vertex attributes: 3 weights in the tangent, 2 in `UV_1` | `crates/engine/src/streaming.rs:1503` |
| Fragment | reconstructed, sixth weight derived | `crates/engine/src/shaders/terrain.wgsl:38` |

**Verified, converter.** `VTXT` entries are 8 bytes: `u16` position, `u16` unknown, `f32`
opacity (`crates/converter/src/esm/cell_cache.rs:190`, reading `entry[..2]` and
`entry[4..8]`). I checked this against the real bytes: for Tamriel `(0,0)` every `VTXT`
payload length is a multiple of 8, every position is `< 289`, and every 4-byte float at
offset 4 lies in `[0, 1]` (e.g. 52 entries for quadrant 0 layer 0). The converter rejects a
position `>= 17 * 17` (`crates/converter/src/esm/cell_cache.rs:196`) and an opacity outside
`[0, 1]` (`crates/converter/src/esm/cell_cache.rs:200`), then stores it verbatim
(`crates/converter/src/esm/cell_cache.rs:204`). **The weights are sparse**: unlisted grid
points mean opacity 0, and nothing densifies them. In quadrant 0 layer 0 only 52 of the 289
points are listed; in quadrant 1 layer 4 only 14 are.

**Verified, hard masks.** The opacities are not a smooth field: quadrant 1 layer 0 of the
same cell has values `0.114, 1.0, 1.0, 0.773, 1.0, 1.0, 0.526, 0.013, 1.0, 1.0, 0.004,
0.001, ...` and quadrant 0 layer 3 is 25 of 29 samples exactly `1.0`. A layer's region
therefore ends in a single-sample 1.0 -> 0.0 jump at the 128-unit grid, and with 17 samples
per 2048-unit quadrant the step is 128 units.

**Verified, engine.** `quadrant_layers` filters, sorts (base first) and caps the overlays at
five (`crates/engine/src/streaming.rs:1344`). `build_terrain_quadrant_mesh` writes each
listed opacity into a dense `17*17` slot array indexed by the raw `vertex` value
(`crates/engine/src/streaming.rs:1462`), reads it back with `local = local_y * 17 + local_x`
(`crates/engine/src/streaming.rs:1479`) and then packs the weights into the vertex
attributes: weights 1-3 into `ATTRIBUTE_TANGENT` as `(w / |w|, |w|)`
(`crates/engine/src/streaming.rs:1503`) and weights 4-5 into `ATTRIBUTE_UV_1`
(`crates/engine/src/streaming.rs:1511`). The material declares no other channel: the base
layer's weight is derived as `1 - sum(others)` (`crates/engine/src/shaders/terrain.wgsl:48`);
the five bindings that would carry real textures are the *diffuse* layer images
(`crates/engine/src/render.rs:168`), not weights; `tiling_and_layer_count.xy` is hard-coded
to `8.0` texels-pairs per cell (`crates/engine/src/render.rs:232`). There is **no weight
texture and no filtering of the weight field anywhere**: the shader samples five diffuse
images and multiplies them by interpolated vertex attributes
(`crates/engine/src/shaders/terrain.wgsl:51`). So the only smoothing the weights ever get is
GPU-linear interpolation across the two triangles of each 128-unit quad.

**Verified. The tangent carrier is lossy.** Bevy re-normalizes `ATTRIBUTE_TANGENT.xyz` in the
vertex shader: in the registry source `bevy_pbr-0.19.0/src/render/mesh_functions.wgsl`,
`mesh_tangent_local_to_world` (lines 101-117) wraps the xyz in `normalize(...)`, and
`bevy_pbr-0.19.0/src/render/mesh.wgsl` writes `world_tangent` from it (lines 85-93). That is
why the mesh stores a unit vector plus `|w|` in `w` - and why it *cannot* store the weights
directly. Interpolating a unit vector linearly shortens it: across an edge where
overlay A goes 1.0 -> 0.0, the middle of the quad has `|interpolated direction| = 0.5` and
`interpolated w = 0.5`, so the shader reconstructs `0.5 * 0.5 = 0.25` where the true bilinear
weight is `0.5` (**inferred** from the code, arithmetic shown here). The weight ramps as
`t^2` instead of `t`: transitions are pushed towards binary, which is exactly the
"nearest-neighbour" look. A vertex whose first three overlays are all zero stores
`(0, 0, 0, 0)` (`crates/engine/src/streaming.rs:1505`); Bevy's guard keeps that from
becoming NaN (`if any(vertex_tangent != vec4<f32>(0.0))` in `bevy_pbr-0.19.0/src/render/`
`mesh_functions.wgsl`, line 109), and the vertex is worth exactly 0 at every neighbouring
blend.

**Pixel scale, and what I could not isolate.** At the top-down acceptance camera (20000 units
above the ground, default 45 degree FOV; `crates/engine/src/app.rs:1259`,
`crates/engine/src/app.rs:1272`) one pixel is 18.4 world units at the terrain below the
camera (`tools/research/terrain_world_map.py`), so one weight sample is ~7 px and a layer
boundary that jumps between two adjacent samples should render as a 7-px axis-aligned step.
I could **not** isolate that step in the two available frames: in rural.png the dominant
hard-edged feature is the water plane of section 2, and in dense.png (no water anywhere in
that grid, section 2) the crisp 1-2 px pixelation sits on the snow material and reads as
texture content, not as a 7-px weight grid. The mechanism below is therefore **code- and
data-verified, not pixel-verified**; the measurement that would settle it is under *Open
questions*. For the record, the profiles I took across the rural patch boundary show 1-px
transitions (`79 -> 213` in one row at image column 688) and 4-px ramps at other columns
(column 774: `89, 102, 149, 170, 174`), and the patch is a mosaic of axis-aligned rectangles
17-23 px across (`tools/research/terrain_steps.py`) - all of which belong to the water plane,
not to the weights.

**Why the existing tests miss it.** `creates_upward_wound_quadrants_with_continuous_uvs`
(`crates/engine/src/streaming.rs:1912`) only checks the winding and that `UV_0` is
continuous; `accepts_matching_neighbor_edges_and_rejects_cracks`
(`crates/engine/src/streaming.rs:1947`) only compares the *heights* of neighbouring cells.
Neither looks at a weight value, at the interpolation of a weight, or at the pixels. The
fixture used by every streaming test has `weights: Vec::new()`
(`crates/engine/src/streaming.rs:1779`), so the whole overlay path is dead code in the test
suite: a mesh built from that fixture has `(0, 0, 0, 0)` tangents everywhere and interpolates
nothing.

## 2. What draws the grid lines

**Measurement (rural.png, 1600x900).** High-passing the frame (image minus a 20-px box blur,
`tools/research/terrain_enhance.py`) leaves the thin lines behind: 1-2 px wide, running
along world X and world Z, with a frayed "zipper" appearance in places. The most persistent
ones are at image x = 263, 685 and 1411, with weaker repeats at 915-932 and 1151-1180
(`tools/research/terrain_bands.py`, bands of 20 rows each). Calibrating the projection two
independent ways gives 17.2-18.4 world units per pixel at the ground below the camera:
18.43 from the camera model (20000 units up, 45 degree default FOV,
`crates/engine/src/app.rs:1272`), 17.2 from the loaded block of `2 * stream_radius + 1 = 5`
cells (`crates/engine/src/config.rs:46`) spanning the observed 1193 px. At that scale:

- one cell (4096 units, `crates/engine/src/world/components.rs:3`) is 222-238 px;
- one quadrant (2048 units) is 111-119 px;
- one LAND sample (128 units) is 7 px;
- one diffuse tile (`tiling_and_layer_count = 8`, `crates/engine/src/render.rs:232`) is 28 px.

The measured persistent lines sit **~230 px apart, i.e. on cell boundaries**, and image x =
685 is exactly world X = 0. **No persistent lines appear at the 111 px quadrant period**, so
`BTXT`/`ATXT` per-quadrant texture sets and per-quadrant weight grids are not what the eye
sees; and the strong line at 685 is exactly the boundary between the dry cell (-1, 0) and
the wet cell (0, 0) (see below). Spacings of ~421 px and ~726 px between my three strongest
lines are 2 and 3 cells, not 1, which is what a water-plane edge pattern predicts.

**Verified: it is not the terrain data.** I compared the LAND records of cell (0,0) with all
four neighbours in `Skyrim.esm` (`tools/research/land_edges.py`): `VHGT` matches exactly on
all four edges (max difference 0.0 units over 33 samples) and `VNML` matches exactly (0 of 99
components differ). `VCLR` is exact on three edges and differs by at most 69/255 in 3 of 99
bytes on the east edge. Height cracks, normal discontinuities and vertex-colour steps are
therefore **ruled out** as the cause of the grid; the seam validator
(`crates/engine/src/streaming.rs:1548`) and its test
(`crates/engine/src/streaming.rs:1947`) cover exactly this and correctly pass.

**Verified: every cell with a real water height gets a blended 4096x4096 plane.**
`crates/engine/src/streaming.rs:474` builds `Plane3d::size(CELL_SIZE, CELL_SIZE)`, spawns it
at the cell's `XCLW` with a transparent blue material and `RenderLayers::layer(1)`
(`crates/engine/src/streaming.rs:500`), and `crates/engine/src/shaders/water.wgsl:38`
replaces up to 80% of its colour with a screen-space reflection sample. The spawn condition
is only "`XCLW` is finite and not the FLT_MAX sentinel"
(`crates/engine/src/streaming.rs:470`, `crates/converter/src/esm/cell_cache.rs:97`).

**Measured from the real plugin (`tools/research/cell_water.py`).** In the 5x5 cells around
grid (0,0) of Tamriel, four cells carry water and the rest do not: (0,0) water `-6070` with
terrain `-6208..-5576` and **130/1089 samples below the water**, (2,0) water `-5400` with
`0/1089` below and `152` units to the nearest sample, (2,1) water `-8050` well clear of its
terrain, (2,2) water `-5150` with `303/1089` below. The nearest terrain sample to the water
level in (0,0) is **2 units** away.

**This predicts the lines exactly.** The water planes in the 5x5 block have vertical edges
only at world X = 0, 4096, 8192 and 12288 (the east edge of (0,0), the west and east edges of
the (2,*) column, and the edge of the block). Projected through the acceptance camera
(`tools/research/terrain_world_map.py`) those edges land at image x = 685, 915, 1165 and
1411; the persistent lines I measured are at 685, 915-932, 1151-1180 and 1411. Four predicted
edges, four measured lines, and no predicted line anywhere in between - which is why the
spacing reads as ~230 px (one cell) and not as the 111 px quadrant period.

**Negative control (dense.png).** The Helgen grid (4,-21) and all eight neighbours have **no
water plane at all** (`tools/research/cell_water.py` against `Skyrim.esm`; terrain
`8224..11568`), so no water can be blamed for that frame. Its blocky snow/rock edges are
therefore the layer-weight staircase of section 1 - the two defects are distinct, and each
screenshot happens to show one of them.

**Inferred mechanism.** At 20000 units the f32 depth buffer (reversed-Z, near plane 0.1)
resolves about `d^2 / (near * 2^23) = 4e8 / 8.4e5 = 477` units, so terrain within ~500 units
of the water level cannot be separated from the water plane at all: the two coplanar
surfaces z-fight everywhere in the flat parts of the cell. The winner is decided per
triangle and the terrain's triangles are 128 units (7 px) on screen, which produces a
*regular fine grid* over the ground; where the terrain is genuinely below the water the plane
simply covers it (a hard-edged, cell-bounded patch), and the plane's own edges are exactly at
cell boundaries, which is the coarse grid. This also explains the white patch: it is the
water plane, whose shader takes up to 80% of its colour from the reflection texture, and the
frame has no reflection producer, so that sample is the fallback image. The blue water
visible just above the patch is the deeper part of the same body of water.

**Why the synthetic fixture misses it.** `setup_terrain_water_fixture`
(`crates/engine/src/app.rs:588`) uses a 2200x2200 plane at `water_height: Some(12.0)`
(`crates/engine/src/app.rs:666`) viewed from a close camera, where the same depth formula
gives ~0.05 units of error - no z-fighting. Its weights are empty, so defect A cannot occur
either. `synthetic.png` is the instanced indirect-draw benchmark
(`crates/engine/src/app.rs:1221`), not terrain.

## 3. How Skyrim itself blends

**Format reference** (UESP's *LAND record* page under the Skyrim modding namespace, e.g.
`https://en.uesp.net/wiki/Skyrim_Mod:Landscape`; this machine has no network access, so the
URL is quoted as a pointer, not fetched - the layout below is what I *verified* against the
real plugin bytes, and it agrees with the converter's parser):
`LAND` splits the cell into four 17x17-vertex quadrants. Each quadrant has one `BTXT` base
layer and up to 8 additional `ATXT` layers, each with an ordered set of `VTXT` entries
`(position, unused, opacity)`; the position identifies a vertex of the quadrant's 17x17 grid
and the opacity is bilinear across the quadrant, blending the layer over the layers under it.
The engine's interpretation is the same (base + ordered overlays, opacity per grid point,
`crates/engine/src/streaming.rs:1344`); its runtime cap of six simultaneous layers is an
engine limit documented in its own error message
(`crates/engine/src/streaming.rs:1364`), and the converter substitutes a dummy base layer
when the plugin leaves `BTXT` null (`crates/converter/src/esm/cell_cache.rs:245`).

Two consequences the current implementation does not honour: Skyrim interpolates the opacity
*between grid points* (bilinear across the quadrant, with the quadrant's own 17x17 samples as
the corners), and unlisted points contribute opacity 0 without affecting the *derived* base
weight, which is exactly what a weight texture does naturally and what the
`1 - sum(others)` formulation only approximates when the overlays are linearly interpolated.

## 4. Fix recipe

Ordered; each step is independent and testable on its own.

**Step 1 - carry the weights as a texture, not as vertex attributes (defect A).**
`crates/engine/src/streaming.rs`: in `build_terrain_quadrant_mesh`, stop packing weights into
`ATTRIBUTE_TANGENT` and `ATTRIBUTE_UV_1`; keep `POSITION`, `NORMAL`, `UV_0`, `COLOR` only.
Add a function that builds two `17x17` `Rgba8Unorm` images per quadrant from
`overlay_weights` (slots 0-3 in the first texture, slots 4-5 in the second), with
`ImageSampler::linear` and `ImageAddressMode::ClampToEdge`, and register them in
`crates/engine/src/render.rs`'s `TerrainExtension` at two spare bindings as
`#[texture(N)] #[sampler(N+1)]`. `crates/engine/src/shaders/terrain.wgsl`: sample both
textures at the quadrant-local uv
(`uv = (in.uv * 4.0 - vec2(select(0.0, 1.0, in.uv.x > 0.5), ...))`, i.e. `fract(in.uv * 2.0)`
scaled to the quadrant) with `textureSampleLevel(..., 0.0)` so mip selection cannot blur the
mask, and keep `weights[0] = max(0, 1 - sum(others))` with the `total` normalization.
Clamp-to-edge reproduces Skyrim's per-quadrant bilinear exactly and keeps quadrant edges
consistent. This removes the `t^2` sharpening, gives linear interpolation of every weight,
and lifts the five-overlay ceiling.
*No cell cache version bump*: `TerrainWeight`/`TerrainLayer`/`CachedLand` are unchanged
(`crates/shared/src/lib.rs:13`), and the textures are built at load time from the existing
sparse lists.

**Step 2 - only draw water where it can be seen (defect B).**
`crates/engine/src/streaming.rs:470`: skip the plane when the cell's terrain is entirely
above the water (`water_height >= max(terrain.heights)`); that alone removes the artifact
from every dry cell. For the cells that do carry water, replace the full-cell quad with the
submerged area: either build the water mesh from the terrain quads whose four heights are
below the water level (trapezoidal clipping per quad, cheap at 32x32 per cell), or keep the
quad but add `DepthBias` (state `.depth_bias`) and lower the plane by ~1 unit so it cannot
tie with the terrain. The generated mesh also removes the cell-aligned edges, because the
water boundary becomes the shore contour rather than a 4096-unit rectangle.

**Step 3 - give the reflection a definition (same defect).**
`crates/engine/src/shaders/water.wgsl:36` samples whatever the reflection handle points at;
with no producer it is the fallback image, which is why the water reads white. Either render
the reflection target, or bind a sky-coloured 1x1 image as the fallback and reduce the mix
(`0.25 + fresnel * 0.55`) to something a fallback can support.

**Tests that fail today.**

1. *(A, CPU, new unit test in `crates/engine/src/streaming.rs`)* Build a quadrant whose three
   first overlays are `(1.0, 0.0, 0.0)` at the corner `(0,0)` and `(0.0, 0.0, 0.0)` at
   `(1,0)`, then reconstruct the weight at the quad's midpoint the way the shader does
   (`normalized * length`, using the stored tangent) and assert `0.5 +- 0.02`. Today it
   returns `0.25`. After step 1, sample the weight texture bilinearly instead and assert the
   same value.
2. *(A, CPU, same file)* Assert the reconstructed weight is monotone and equals the linear
   interpolant to within 2% at the quarter points of every edge of a quad - the property the
   packing breaks.
3. *(B, CPU, new unit test)* For a `TerrainSnapshot` whose heights are all above the water
   height, assert no water entity is spawned (or that the generated water mesh is empty).
   Today the plane is always spawned when `XCLW` is a real number.
4. *(B, CPU, same file)* For a cell with a channel (four corners above the water, the centre
   below), assert every vertex of the generated water mesh lies inside the cell's submerged
   contour, i.e. that no vertex sits on the cell's outer boundary.
5. *(B, off-screen acceptance, `scripts/`-level, for the lead)* Capture the rural grid twice,
   once with the water entities hidden, and assert the cell-aligned line energy from
   `tools/research/terrain_enhance.py` drops by a factor of 5 in the terrain-only image. This
   is the check that proves the lines came from the water plane and not from the terrain.

## Open questions

- The missing pixel measurement for defect A: capture the rural grid twice, once with the
  water entities hidden, and look for a 7-px axis-aligned staircase on a layer boundary over
  dry flat ground (or capture a dry cell such as grid (1,0) from the same camera). That is
  the frame in which the weight grid is the only possible source of steps.
- The `VTXT` position encoding: every real value is `< 289` so both `y * 17 + x` and
  `x * 17 + y` validate. I could not find a primary source that settles which axis is which;
  the data is equally coherent under both, because a transposed patch is still a patch. If
  the convention is `x * 17 + y`, every overlay is mirrored inside its quadrant and would
  show as a mismatch against the neighbouring quadrant's copy of the same texture (quadrant 0
  layer 0 and quadrant 1 layer 3 of cell (0, 0) are both texture `000300E4`). Worth one
  experiment: render one layer's opacity grid both ways and compare against the game's own
  in-game terrain, or find the convention in UESP's `VTXT` description.
- I did not verify the reflection texture's actual content at capture time; the white patch is
  consistent with a fallback image, but a one-line experiment (bind a magenta fallback) would
  settle it.
- I did not measure the fine z-fighting grid period pixel by pixel; the 128-unit (7 px)
  figure comes from the depth-precision arithmetic and the terrain's triangle size, not from
  a direct period measurement.

Tools written for this analysis (read-only, no side effects beyond `local/`):
`tools/research/land_vtxt.py`, `tools/research/land_edges.py`, `tools/research/cell_water.py`
(plugin readers), `tools/research/terrain_enhance.py`, `tools/research/terrain_bands.py`,
`tools/research/terrain_boundary_steps.py`, `tools/research/terrain_steps.py`,
`tools/research/terrain_world_map.py`, `tools/research/terrain_seam_measure.py`,
`tools/research/terrain_crop.py` (pixel measurements).
