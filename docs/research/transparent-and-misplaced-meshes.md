# Objects that render transparent or misplaced on the Alftand -> Blackreach demo

Research note for `tasks/deepseek/research-011-transparent-and-misplaced-meshes.md`. Every
claim is marked **verified** (a file, a byte or a screenshot was read) or **inferred** (a
conclusion drawn from several verified facts). Numbers come from scripts under
`tools/research/` (section 5) run against the converted set
`C:/Modding/SkyrimConverted` and the source NIFs in its `vfs/meshes`.

Short version: three separate defects make solid objects look transparent, one makes whole
rooms appear where they do not belong, and one that was suspected (LOD beside full meshes)
does not exist. The biggest single lever is one line of the converter's alpha contract.

## 1. What the screenshots show

`local/demo/survey1/*.png` are four-way views from each stage of the tour (`tour.txt` has
the camera positions). Read as images, not logs:

* `00-survey-1.png` / the 2x crop of the same frame: the Stockade scaffold platforms and
  rope in the foreground stay crisp, and big pale grey-white shapes are painted **over**
  them at ~40-60% opacity with straight polygon edges; the ice around the platform is
  visible as a ghost you can see the planks through.
* `00-survey-2.png`, `00-survey-3.png`: whole-screen pale sheets over rock and hard-set
  ice chunks; again the scaffolding reads through them.
* `01-survey-1.png`, `01-survey-2.png`: a Dwemer/ice interior whose big surfaces (floor and
  walls) look washed out where the detail should be, with dark voids showing through
  geometry that should be solid.
* `03-survey-1.png`: the AlftandWorld cavern view is almost entirely a dark blue veil with
  faint structure behind it.
* `03-survey-2.png`: untextured-looking flat white shapes sit on top of the Dwemer
  stonework, plus thin bright rods and wire-like slivers.
* `01-door.png`, `02-door.png`: long thin brown bars criss-cross the door views. They are
  `Dungeons\Dwemer\Pipes\DwePipe*` and the mine woodwork (about 200 pipe pieces are placed
  in Alftand01), i.e. clutter from the source data, **not** a defect - recorded here so the
  next reader does not re-open it.

`local/demo/survey1.err` carries no warning about geometry or materials at all (two
`LAND edge welded` warnings and `gilrs` noise only), so none of this is visible in the log.

## 2. Defect classes

| # | What it looks like | Example models (converted path) | Population | Root cause | Status |
|---|---|---|---|---|---|
| A | Solid ice, rock, snow, floors and walls drawn translucent, pale and unsorted; you see the world through them | `Landscape\Ice\IcePileM02.nif` shape `:1`, `Landscape\Ice\GlacierEndLg01.nif` `:0`, `Landscape\Ice\GlacierCaveEntrance02.nif` `:0`, `Landscape\Rocks\RockPileS04.nif` `:0`, `Landscape\Mountains\MountainTrim01.nif` `:4`, `Dungeons\Dwemer\Rubble\DweFloorRaised01.nif` `:1`/`:16`, `DweRubblePile04.nif` `:27`, `DweFacadeHallSmExBg01.nif` `:11` | 2,632 materials of 60,414 in the converted set; on the tour's 32 cells 730 placed references (140 materials), 259 of them visibly see-through | `crates/converter/src/material.rs:990` - a shape with no `NiAlphaProperty` but the `SLSF1_VERTEX_ALPHA` shader flag is published as glTF `BLEND`; the per-vertex alpha it was meant to use is not exported (GLB attributes are `NORMAL,POSITION,TEXCOORD_0` only), so the base colour texture's alpha channel becomes the opacity | **verified** |
| B | Glow and light cards (lanterns, sconces, chandeliers, torches, cobwebs, snow-blowing planes, oil sheen) render as dull grey/blue veils instead of bright glows | `Dungeons\Dwemer\DweFloorLanternOn.nif` (`ColumnFacingGlow110`, `DwLighttest*`, `testfogcircle*`), `DweWallSconceSmall01On.nif`, `DweChandelier02On.nif`, `Weapons\Torch\torch.nif` (`GlowAddMesh`), `Traps\OilTrapPuddle01\OilTrapPuddle01.nif` | 4,335 additive (`SRC_ALPHA`/`ONE`) and 435 multiplicative (`ZERO`/`SRC_COLOR`) of 36,129 `NiAlphaProperty` blocks - 23.9% of the 19,942 blend-enabled ones; on the tour 706 placed references (139 materials) | `crates/converter/src/material.rs:986` keeps only "blend enabled" and drops the source/destination blend factors that `vendor/project-wormhole-nif/src/nif_flags.rs:164` defines; glTF `BLEND` is always straight alpha-over, so additive and multiply surfaces become normal transparency | **verified** |
| C | Flat untextured white/grey shapes stuck to ordinary objects, and marker geometry drawn in the world | `Dungeons\Dwemer\Partitions\DwePtnDoor01.nif` and `RuinsLever02\DweLever02.nif` are real objects that carry an `EditorMarker` shape; `Weapons\Torch`, `Traps\SpikeTrap`, every `Effects\FX*` NIF has one | 783 `BLEND` + 772 `OPAQUE` untextured materials; on the tour 44 untextured materials, ~330 placed references | The converter publishes an `EditorMarker` shape like any other shape (`crates/converter/src/material.rs:239`); the engine filters whole model families - `meshes/sky/`, `markers/`, `effects/` and any file name containing `marker` (`crates/engine/src/streaming.rs:1610`) - but nothing inside a model that is otherwise legitimate | **verified** (class), **inferred** (that the flat white shapes in `03-survey-2.png` are these - residency cannot be decided from the database alone, see D) |
| D | Whole rooms' geometry drawn where the player stands: walls, floors, traps and pipes of unrelated interiors overlapping the current cell, or a chunk of the next worldspace floating at a raw grid offset | At the AlftandWorld arrival (`door 0009256A` arrival `(3693.8, 3074.6, 290.5)`), references within 250 units belong to `AbandonedPrison01` (Imperial floors at 85 units), `WarehouseTraps`, `SunderstoneGorge01`, `KilkreathRuins01`, `IlinaltasDeep01`, `DA14Interior`, `CragwallowSlope01` | Every frame a door is within 800 units: 1 interior cell, or a 3x3 exterior grid, or the cascade below | Pre-streamed cells are spawned into the active cell's render space: an interior's references keep their absolute creation coordinates (`crates/engine/src/streaming.rs:769`) and an exterior cell root is placed from the *current* `RenderOrigin` (`crates/engine/src/streaming.rs:1568`), while the destination grid was computed in the destination worldspace's own coordinates (`crates/engine/src/transition.rs:85`); nothing hides them from the camera | **verified** |
| E | Ice `MASK` shapes that vanish or keep hard edges | `Landscape\Ice\GlacierRubbleTrim01.nif`, `Clutter\Hay\HayMound01.nif` | 179 materials blend *and* test with no cutoff published for the blend case | `crates/converter/src/material.rs:983` returns `Cutout` when the property has a test, but 179 materials also carry the blend bit; those become `BLEND` with no `alphaCutoff` | **verified** (small) |
| F | LOD mesh drawn beside the full mesh | - | 0 | The engine never reads the `lod` table: no `lod`/`LOD` identifier outside the sampler clamp check in `crates/engine/src/streaming.rs:1467` | **refuted** |
| G | Rotation/scale conversion errors, dropped NIF node transforms | - | 0 found | The engine spawns the whole converted scene (`GltfAssetLabel::Scene(0)`, `crates/engine/src/streaming.rs:803`), so node transforms survive with it; the tour's own door positions reproduce the database's creation coordinates exactly through `cell_translation` (`crates/engine/src/streaming.rs:1568`) and `render_position` (`crates/engine/src/streaming.rs:1591`) for doors `00015D48` (origin cell 18,18) and `0009256A` (origin cell 0,0) | **verified** |

### A in detail: one shader flag decides the render pass

`alpha_contract` (`crates/converter/src/material.rs:975`) is the whole decision:

```rust
if property.flags.test_enabled()  { return (NifAlphaMode::Cutout, Some(property.threshold), ...); }
if property.flags.blend_enabled() { return (NifAlphaMode::Blend, None, ...); }
let shader_requires_blend =
    shader_flags_1 & (SLSF1_VERTEX_ALPHA | SLSF1_SCREENDOOR_ALPHA_FADE) != 0
        || shader_flags_2 & SLSF2_PREMULTIPLIED_ALPHA != 0;
```

and `publish_material` (`crates/converter/src/material.rs:425`) turns a sub-1.0 shader alpha
into `BLEND` as well. The `SLSF1_VERTEX_ALPHA` branch is the one that matters here, and it is
pinned by its own test, `shader_alpha_flags_require_blending_without_an_alpha_property`
(`crates/converter/src/material.rs:1466`), which asserts `Blend` for `(None,
SLSF1_VERTEX_ALPHA, 0)`.

Reading the source blocks of the affected models (script 5.2) shows the shape of the bug -
two shapes of the same mesh differ only in that one bit:

| Model | Shape | NIF alpha property | NIF shader flags 1 | glTF result | base texture (mean alpha) |
|---|---|---|---|---|---|
| `Landscape\Ice\IcePileM02.nif` | `IcePileM02:1` | none (NULL) | `0x82400309` = vertex alpha | `BLEND`, alpha 1.0 | `snow01.dds` (165/255) |
| `Landscape\Ice\IcePileM02.nif` | `IcePileM02:0 - L2_IcePileM02:0` | none | `0x82400301` | `OPAQUE` | `glacierslab.dds` |
| `Dungeons\Imperial\ExteriorIce\ImpExtIceBldgICorner01.nif` | `:16` | `flags=0x12EC` test > 93 | `0x8E400309` | `MASK` cutoff 0.365 | - |
| `Dungeons\Imperial\ExteriorIce\ImpExtIceBldgICorner01.nif` | `:2`, `:15`, `:17` | none (NULL) | `0x82400309` | `BLEND`, alpha 1.0 | - |

The `MASK` row is the control: the cutoff `0.365` is exactly the property's threshold `93`
over 255, so the converter's mapping and the reader used here are both correct, and the
`BLEND` rows really do have no alpha property at all. In Skyrim the blend state comes from
the `NiAlphaProperty`; the shader flag only asks the shader to read per-vertex alpha, and
the converter exports no vertex colours, so there is no alpha to read.

Because the material's own alpha is 1.0, the surface is only see-through where the **base
colour texture's alpha channel** is below 1 - and in Skyrim that channel is often a shader
mask (glacier subsurface, snow sparkle), not opacity. Measured source textures (script 5.4):

| texture | alpha min | mean | max | used by |
|---|---|---|---|---|
| `landscape\glacierslab.dds` | 11 | **51** | 111 | `GlacierCaveEntrance02:0`, `GlacierEndLg01:0` |
| `landscape\dirt02.dds` | 0 | **59** | 255 | `RockPileS04:0`, `RockPileM02:0`, `RockPileS01:0`, `RockCliff02:0`, `RockPileL02:0` |
| `landscape\snowrocks01.dds` | 38 | **93** | 207 | `IcePileL01:5` |
| `landscape\rocks01.dds` | 32 | **111** | 255 | `RockPileM02:9`, `RockPileL01:9`, `RockPileL03:9` |
| `landscape\snow01.dds` | 102 | **165** | 255 | every `IcePile*:1`, `MountainTrim01:4`, `GlacierRubbleTrim02:1` |
| `dungeons\rubblepile02.dds` | 0 | **179** | 255 | `DweFloorRaised01:16`, `DweFloorRaised02:16`, `DweRubbleColumn01:16` |
| `dungeons\dwestonefloor02.dds` | 255 | 255 | 255 | `DweFloorRaised01:1`, `DweHallLgEndCap01:1`, `DweHallBg*` |

So a rock pile textured with `dirt02` renders at about a quarter opacity, and a Dwemer floor
plate with `rubblepile02` at about 70%; the architecture whose textures are opaque looks
solid but still lands in the transparent pass, where it does not write depth and is sorted
per object by distance to the camera.

The census (`tools/research/blend_cause_census.py`, whole converted set - 18,597 models,
60,414 materials):

| cause | materials |
|---|---|
| `opaque` | 39,118 |
| `mask_from_alpha_test` | 11,735 |
| `blend_from_alpha_property` (legitimate) | 6,060 |
| **`blend_from_vertex_alpha_flag`** | **2,632** |
| `mask_other` (MASK with no test found) | 652 |
| `blend_with_test_enabled` | 179 |
| `blend_from_shader_alpha` (alpha < 1) | 34 |
| `blend_without_alpha_property_or_flag` | 4 |

Folders of the 2,632: `dungeons` 1,132, `architecture` 479, `dlc02` 267, `dlc01` 194,
`landscape` 114, `lod` 100 - note the LOD family, i.e. distant terrain is affected too.

### D in detail: the mechanism, and the cascade

Verified pieces:

* `plan_door_prestream` (`crates/engine/src/transition.rs:64`) takes every `LoadDoor` whose
  **render** transform is within `DOOR_PRESTREAM_RADIUS = 800` (`crates/engine/src/transition.rs:27`)
  and requests the destination: one interior cell, or a 3x3 exterior grid computed from the
  arrival point in the destination's own coordinates (`crates/engine/src/transition.rs:85`).
* `wanted_cells` merges those requests into the resident set (`crates/engine/src/streaming.rs:573`)
  and `cell_within_unload_radius` keeps a pre-streamed cell alive (`crates/engine/src/streaming.rs:606`).
* `spawn_cell` then places an interior's references at `creation_to_bevy(creation_position)`
  with no origin subtraction (`crates/engine/src/streaming.rs:769`; the interior cell root is
  at `Vec3::ZERO`, `crates/engine/src/streaming.rs:1575`), and places an exterior cell root at
  `cell_translation(key, origin)` against the **current** `RenderOrigin`
  (`crates/engine/src/streaming.rs:1568`). Nothing turns them off: only water is on a
  non-default render layer (`crates/engine/src/streaming.rs:757`) and the camera draws layers
  0 and 1 (`crates/engine/src/app.rs:1439`).

Consequences, both **verified** against the database:

1. **Interiors share one coordinate space.** The AlftandWorld arrival sits inside the
   authored volume of seven unrelated interiors at once; the nearest of them,
   `AbandonedPrison01` (`Dungeons\Imperial\ClutterKits\ImpFloorMid02.nif`) is 85 units from
   the camera, and `DweFloorRaised01`-style floor plates of the active cell are at 115. If
   any of those cells is the pre-streamed destination, its rooms are drawn inside the
   current one.
2. **The destination grid of another worldspace is dropped at raw grid coordinates.** The
   destination grid is computed in the destination worldspace's coordinates but rendered
   against the source worldspace's `RenderOrigin`; with a blackreach arrival grid of
   `(gx, gy)` the cells land around `(gx - origin.x) * 4096` in the source's space. This is
   also why the geometry visibly *jumps* at a crossing: `reposition_cell_roots` re-places the
   roots once the origin moves (`crates/engine/src/transition.rs:370` covers this in test
   form).

**The cascade** (inferred): a pre-streamed door is spawned at *its* coordinates, so the
distance test in `plan_door_prestream` measures a misplaced door; that door's destination is
then pre-streamed too, and so on, so one misplaced cell can pull in its neighbours. This is
the reason the *set* of cells drawn in a given frame cannot be decided from the database
alone - the streaming plan would have to be simulated - and the reason the survey frames
were not matched model-by-model here.

`impl-009-door-portals` owns this defect (isolation by render layer, door hiding, the
portal). This note only confirms it: the brief's lead is **verified**, not refuted.

## 3. Fix recipes

### A - do not blend a shape that has no alpha property (converter, one function)

`crates/converter/src/material.rs:975` `alpha_contract`: drop `SLSF1_VERTEX_ALPHA` from
`shader_requires_blend`, leaving screendoor and premultiplied alpha as the only
flag-driven blends, and update the test at `crates/converter/src/material.rs:1466`.

* Edge cases: a shape may blend *and* test (class E) - keep the test branch first, and if a
  blend is requested with no property, publish `BLEND` with `alphaCutoff` unset only when
  some other input justifies it. `SLSF2_PREMULTIPLIED_ALPHA`/`SLSF1_SCREENDOOR_ALPHA_FADE`
  stay (they are genuine transparency hints, and the extension in
  `crates/converter/src/material.rs:581` already reports them).
* Test: a NIF fixture with two shapes that differ only in `SLSF1_VERTEX_ALPHA` must publish
  one `OPAQUE` and one `BLEND` **before** the change and two `OPAQUE` **after**; plus a unit
  test on `alpha_contract(None, SLSF1_VERTEX_ALPHA, 0) == Opaque`.
* Acceptance: `blend_cause_census.py` must report `blend_from_vertex_alpha_flag` = 0 and
  `blend_from_alpha_property` unchanged (around 6,060).
* Second half of the same defect: where a texture's alpha is a shader mask and the material
  is `OPAQUE`/`MASK`, the alpha channel is ignored by the renderer anyway; if a later change
  puts such a shape back into the transparent pass, the converter must publish an
  alpha-free base colour (encode the KTX2 without alpha) or a `baseColorFactor` the renderer
  can honour.

### B - keep the additive and multiplicative blend factors (converter + engine)

* Converter: publish the pair in `OPEN_SKYRIM_material` (the extension built at
  `crates/converter/src/material.rs:581` already carries `shaderFlags1/2`), and set glTF
  `alphaMode` so a renderer can tell them apart - `dst = ONE` is additive, `ZERO/SRC_COLOR`
  is a multiply.
* Engine: read the extension in the material loader that validates glTF materials
  (`crates/engine/src/streaming.rs:1398`) and build the Bevy material accordingly
  (`AlphaMode::Add`/`AlphaMode::Multiply` exist in Bevy's `StandardMaterial`; if the version
  in `Cargo.lock` lacks them, a small custom material as in `crates/engine/src/render.rs`).
* Test: converter unit test over a NIF with `flags = 0x0CC1` (`SRC_ALPHA`/`ONE`) asserting
  the published extension; engine unit test asserting the material kind for a fixture GLB;
  acceptance: `route_blend_factors.py` must report 0 additive/multiplicative materials left
  unmarked on the route.

### C - do not publish editor markers (converter)

Extend the existing exclusion path (`NifMaterialDisposition::Excluded`,
`crates/converter/src/material.rs:249` and its publication at
`crates/converter/src/material.rs:298`) to shapes whose name is `EditorMarker` or whose
base-colour texture is the marker texture. This reuses machinery the pipeline already tests
(`publishes_exclusions_without_reusing_an_exporter_material`,
`crates/converter/src/material.rs:1384`), and the engine already treats a shape with
`extras.openSkyrim.materialExclusion` as non-rendering
(`crates/engine/src/streaming.rs:1387`).

* Edge case: the marker shape is often the mesh that gives a reference its bounds, so keep
  exporting the geometry (as the exclusion does) and only replace the material.
* Engine alternative (if the converter must stay untouched): skip shapes whose material name
  is `EditorMarker` or whose material carries no texture *and* the marker texture path, at
  scene-spawn time next to `converted_model_path` (`crates/engine/src/streaming.rs:1606`).
* Test: converter test on `Traps\SpikeTrap\SpikeTrapDwemer.nif` (a real object with a marker
  shape) asserting the marker mesh gets the non-rendering material and the real shapes keep
  theirs; acceptance: `untextured_scene.py` must stop listing `EditorMarker`.

### D - isolate pre-streamed cells (engine, owned by `impl-009-door-portals`)

Nothing to add to that brief except the two facts that make it urgent: the overlapping
interiors of section 2.D.1, and the cascade - isolating a cell must also stop its doors from
feeding `plan_door_prestream` (`crates/engine/src/transition.rs:64`), or a hidden cell keeps
pre-streaming its neighbours through the misplaced transform it no longer draws.

* Test: an `App` fixture with an active cell and a pre-streamed cell whose own door is within
  800 units: after isolation the second cell's references are not on the main camera's
  layers, and no third cell was requested from the hidden door.

### E - blend and test at once (converter, small)

`crates/converter/src/material.rs:983`: when the test is enabled, also publish `alphaCutoff`
if the material is later forced to `BLEND`, or split the property into "test wins" only when
the two do not conflict. 179 materials.

## 4. What is not answered

* Which single reference makes the flat white shapes in `03-survey-2.png`. The database
  cannot say which cells are resident in a frame (section 2.D), and the tour log records no
  cell census; the frame would have to be re-run with per-entity logging or a
  `world-inspect` dump of the resident cells.
* Whether Skyrim draws any of the class-A shapes blended *because of* their vertex colours.
  The format says the blend state comes from the `NiAlphaProperty` and these shapes have
  none, but no Bethesda executable was run to confirm the on-screen result; the class is
  nonetheless a defect either way, because the vertex colours it was meant to read are not
  exported.
* `mask_other` (652 materials): the shapes have a `MASK` output but no alpha test was found
  in the block their `shapeBlock` names, which is the one place this reader is heuristic
  (it looks for the alpha block index inside the shape block rather than parsing the shape
  layout). Treat the number as an upper bound.

## 5. How to reproduce the numbers

All scripts are in `tools/research/` and read only the converted set and the game data.

1. `python tools/research/glb_materials.py summary` - material counts by alpha mode over all
   22,761 GLBs; `... list BLEND`, `... model <substring>`, `... detail <substring>`,
   `... untextured <n>`, `... hist`.
2. `python tools/research/nif_alpha.py --shapes <nif>` - reads a Skyrim NIF's header (block
   sizes give every block's offset) and prints each `NiAlphaProperty` with its flags decoded
   (blend enable, source/destination factor, test enable/function/threshold) and the shapes
   that reference it. `--head <nif>` dumps the first bytes.
3. `python tools/research/material_pipeline.py <model substring>` - joins each GLB material
   back to its NIF shape and shader blocks (`extras.openSkyrim`), printing `alphaMode`,
   cutoff, base alpha, the `NiAlphaProperty` flags and the shader flags - this is the table
   in section 2.A.
4. `python tools/research/dds_alpha.py <texture>` or `--glb <model>` - source DDS format and
   alpha-channel statistics for the textures each material uses.
5. `python tools/research/blend_cause_census.py` - classifies every converted material into
   the table in section 2.A; `python tools/research/alpha_blend_census.py` - flag-word
   inventory over all 36,129 `NiAlphaProperty` blocks.
6. `python tools/research/route_blend_report.py`,
   `transparent_route_models.py <min_refs> --summary`, `route_blend_factors.py`,
   `untextured_scene.py <min_refs>` - the same measurements restricted to the tour's 32
   cells (the Tamriel 5x5 grid around Alftand's exterior, Alftand01/02, the AlftandWorld
   cells, the Blackreach arrival cells).
7. `python tools/research/world_db.py <command>` - the converted database, read-only:
   `cells`, `named`, `stats`, `doors`, `arrival`, `around <x> <y> <z> <radius> <limit>
   [model=<substring>]`, `find`, `grid`. Positions are creation units; a render-space
   camera position becomes creation with `(x, -z, y)` plus `4096 * origin grid`, and the
   interior cells of the tour have no origin offset.
8. `python tools/research/scene_materials.py <cell|grid ...>` and
   `scene_size_report.py <x> <y> <z> [radius]` - placed models with their alpha modes, and
   the same ordered by GLB size, which is how the candidates above were picked out.
9. `python tools/research/crop_image.py <src.png> <dst.png> <x0> <y0> <x1> <y1> [scale]` -
   zoom into a screenshot; crops were written under `local/demo/survey1/`.
