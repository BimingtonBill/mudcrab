# impl-068 - the sun's shadow cascades, fitted to a 4096-unit world

**Task:** the lead's brief `impl-068 - Make shadows work`.

**Acceptance run here** (final state of the files below): `cargo fmt --all -- --check` (no output),
`cargo clippy --workspace --all-targets -- -D warnings` (clean), `cargo test -p engine` (301 tests:
298 passed, 0 failed, 3 ignored, plus 3 in the integration group and 4 ignored doctests). Two new
tests, both in `app::tests`.

## 0. The defaults, measured (what the brief asked for first)

All from the vendored crate, `bevy_light-0.19.0`:

- **`CascadeShadowConfig::default()`** is `CascadeShadowConfigBuilder::default().into()`
  (`src/cascade.rs:135-158`): `num_cascades: 4`, `minimum_distance: 0.1`, `maximum_distance: 150.0`,
  `first_cascade_far_bound: 10.0`, `overlap_proportion: 0.2`. `calculate_cascade_bounds`
  (`src/cascade.rs:41-56`) spaces the far bounds **geometrically** between the first bound and the
  maximum, so the default far bounds are **10 / 24.66 / 60.81 / 150**.
- The comment above those defaults (`src/cascade.rs:137-140`) says where they come from: Unity
  (first bound 10.05, maximum 150), Unreal 5 (200) and Godot (first bound 10, maximum 100). They are
  a metre-scale consensus.
- **In Skyrim units (1 unit = 1.43 cm): the sun's whole shadow map covered a 2.14 m sphere around the
  camera, and the first cascade ended 14.3 cm in front of the lens.** A Riverwood house is hundreds
  of units across, the full-detail grid is 12,288 units wide. **The hypothesis in the brief holds,
  and more strongly than "a patch smaller than a doorstep": nothing the camera can be pointed at was
  ever inside any cascade, so the shadow map was invisible in every frame the engine has ever drawn.**
- `DirectionalLightShadowMap::default().size == 2048` (`src/directional_light.rs:202-206`).
- `DirectionalLight::DEFAULT_SHADOW_DEPTH_BIAS = 0.02`, `DEFAULT_SHADOW_NORMAL_BIAS = 1.8`
  (`src/directional_light.rs:163-168`).
- How the two are applied (`bevy_pbr-0.19.0/src/render/shadows.wgsl:190-193`):
  `normal_offset = shadow_normal_bias * cascade.texel_size * N` and
  `depth_offset = shadow_depth_bias * direction_to_light`, with the normal bias scaled by `sqrt(2)`
  at extraction (`src/render/light.rs:811`). So the **normal bias is texel-relative and therefore
  scale-free**, and the **depth bias is a world-space step** - 0.02 units is 0.29 mm here.
- Shadow maps are `Depth32Float` (`bevy_core_pipeline-0.19.0/src/core_3d/mod.rs:5`), so depth
  precision is not what limits this scale.

The in-repo precedent for what shadow-pass geometry costs: `crates/engine/src/streaming.rs:1122-1130`
- taking the ring's terrain out of the shadow pass took a twelve-cell Pale view from 30 to 59 frames
a second.

## 1. What changed

`crates/engine/src/app.rs` only:

- the import `bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap}`;
- three constants beside the other world-scale derivations: `CREATION_UNITS_PER_METRE = 70.0` (the
  same 70:1 `crate::lights` converts point-light intensity at), `SUN_SHADOW_CASCADES = 4`,
  `SUN_SHADOW_MAP_SIZE = 2048`;
- **`sun_shadow_cascades(config) -> CascadeShadowConfig`** - the configuration, derived:
  - `minimum_distance = 0.1 m` = **7 units**, `first_cascade_far_bound = 10 m` = **700 units**: Bevy's
    own two near distances, read in metres and converted;
  - `maximum_distance = sqrt(2) * CELL_SIZE * (stream_radius + 1)` = **17,378 units (248 m)** at the
    default `stream_radius` 2. `streaming::wanted_cells` holds the camera's cell plus `stream_radius`
    cells, so the camera - anywhere inside its own cell - is at most `stream_radius + 1` cells from
    the grid's far edge and `sqrt(2)` times that from its far corner. Every mesh the streamer holds
    in full is then in the last cascade whichever way the camera faces;
  - the far bounds that fall out: **700 / 2,042 / 5,959 / 17,378**, which at the engine's 45 degree
    vertical field of view is **0.96 cm / 2.8 cm / 8.2 cm / 24 cm of world per shadow texel**.
- `setup_world`: the sun's spawn gains `sun_shadow_cascades(&config)`, and the system inserts
  `DirectionalLightShadowMap { size: SUN_SHADOW_MAP_SIZE }` (Bevy's own default value, written out so
  that it is chosen with the cascades rather than by a future Bevy release).

The ring is **not** covered, on purpose: it is `NotShadowCaster` (deliberately, per the brief) so
extending the range over it would add no caster and only take texel size away from the shadows that
are visible. The comment says so; if the lead wants the ring to *receive* shadows from the far edge
of the grid, the knob is `maximum_distance` (and the cost is texel size in the last two cascades).

## 2. Where the numbers came from, and what they cost

| | Bevy default | here | why |
|---|---|---|---|
| cascades | 4 | 4 | `MAX_CASCADES_PER_LIGHT` (`bevy_pbr-0.19.0/src/render/light.rs:228`); a 5th is clamped with a warning, and fewer would coarsen everything for a 248 m range |
| shadow map | 2048 | 2048 | the texels above; 4096 is 4x the memory and 4x the fill for detail this world's geometry does not have |
| first cascade | 10 units | 700 units | Bevy's 10 m, in Creation units |
| near clamp | 0.1 | 7 units | Bevy's 10 cm, in Creation units |
| max distance | 150 units | 17,378 units | the full-detail grid's far corner - see above |
| overlap | 0.2 | 0.2 | the blend band should stay a proportion of the cascade |
| depth bias | 0.02 | **unchanged** | a world-space step, 0.29 mm here - three orders below the texel-sized error it would correct; a constant that cannot be right at both 1 m and 248 m |
| normal bias | 1.8 | **unchanged** | already texel-relative (2.5 texels of offset along the normal), i.e. the same physical trade Bevy's metre-scale default makes |

Tuning was therefore not blind in either direction: the two biases were *left* because the arithmetic
says the normal bias is scale-free and the depth bias is negligible at this scale, not because there
was nothing to compare against.

**Expected frame-rate cost.** The union of the four cascades is the camera's frustum out to 17,378
units, which is everything the full-detail grid holds, so the shadow pass now draws about what the
main opaque pass draws: the dense profile in `local/acceptance-shakedown` records 1.15 M vertex
shader invocations a frame for the main opaque pass, and the same geometry enters the shadow pass.
Expect the frame's geometry work to roughly double (the ring-off baseline is 180 FPS ≈ 5.6 ms, so of
the order of 1-3 ms more), and the `streaming.rs` 30 → 59 FPS measurement is the house precedent for
how quickly shadow-pass geometry adds up. **The knobs, in the order they help**: `maximum_distance`
in `sun_shadow_cascades` (the geometry the pass draws at all), `SUN_SHADOW_CASCADES` (how many
passes that geometry is split over, and how much per-pass overhead), `SUN_SHADOW_MAP_SIZE` (fill
rate, not geometry).

## 3. Decisions the lead may want to overrule

1. **Only the world's sun got the configuration; the four fixture suns did not.** `run()` has five
   `DirectionalLight` spawns today, not the two the brief names: `setup_material_fixture`,
   `setup_terrain_water_fixture`, `setup_transform_bounds_fixture`, `setup_renderer_fixture` and
   `setup_world`. The first four are metre-scale scenes - 2.2-unit cubes with the camera 16 units
   away - which is exactly what Bevy's defaults are fitted to, and the world-scale config would make
   their shadows 30 times coarser. The one that draws Skyrim, the demo, the shots and the benchmark
   runs is `setup_world`'s, and that is the one this task is about. (A *fixture* camera can still end
   up sharing it if `setup_world` and a fixture run together; nothing asserts on fixture shadows.)
2. **"It still covers the streaming radius at a larger `--terrain-radius`" is read as "the
   configuration stays right for every radius the command line accepts", not "the shadow range grows
   with the ring".** The range is tied to the full-detail grid, which is where casters exist; the
   test pins that a 2, 8, 16 or 32-cell ring leaves the range identical, that a wider
   `--stream-radius` moves it out, and that the coverage holds in each case. Growing the range with
   the ring would put the ring's 32,768 units into four cascades - a 4.9 m shadow texel at
   `MAX_TERRAIN_RADIUS` - and buy nothing, because the ring casts nothing.
3. **The four fixture scenes and the ambient/emissive/point-light paths are untouched**, as the brief
   requires: `lights.rs:185` stays `shadow_maps_enabled: false`, `streaming.rs:1130` stays
   `NotShadowCaster`, and no command-line switch was added.

## 4. Not done, and what I could not check

- **No engine run and no GPU**, per the brief. Everything visual below is the lead's to check.
- **The first thing to look at: the portal camera's shadows.** Every *view* gets its own cascades
  (`bevy_pbr-0.19.0/src/render/light.rs:1763-1853`, a fresh `directional_depth_texture_array_index`
  per view), while the depth texture array is sized by the **max** over views and the per-view base
  index restarts at 0 (`:1351-1353`, `:1708`). Two views therefore write and sample the same four
  layers, and a view's own shadow passes are what it samples only if they run immediately before its
  main pass in its own subgraph. In principle they do - and this did not matter before, because the
  cascades held nothing worth looking at - but it was **not verified on a GPU**. If the doorway image
  or the main image shows shadows from the wrong place while the portal is open, that is this, and
  the fix is on the `portal.rs` side (the other task's file, not mine).
- **Benchmark baselines will move.** `run()`'s last branch runs `setup_world` for a benchmark-only
  run, so the Phase 2 runs pay for the shadow pass from now on. The Phase 2 frame-time gates were
  taken with a shadow pass that drew nothing; they may need re-baselining, or the lead may want the
  benchmark scene to keep the old configuration - which, since the brief forbids a shadows switch, is
  a decision for the lead and not a line I should have written.
- **An aerial pose further than 248 m from the village will be outside the shadow range.** All twelve
  poses in `tools/reference/riverwood_shots.json` are at eye height (the third coordinate is
  −62 … −3) and well inside it; the elevated `rw_sweep*` candidates were not checked.
- **The far bounds are exact to about a thousandth of a unit, not to the bit**: the geometric series
  in `calculate_cascade_bounds` reaches `maximum_distance` by three multiplications, so the test
  asserts the last bound is within 1 unit of the requested distance (measured: 12,288.001 for a
  requested 12,288 in the mutation run below) rather than exactly equal.
