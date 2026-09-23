# research-059 - The forest is invisible: alpha test versus alpha blend

**Answer in two sentences.** The trees are not arriving as `BLEND`: every foliage shape converts to glTF
`MASK` with the NIF's own threshold (`TreePineForest03.nif` carries `NiAlphaProperty` flags `0x12EC`,
alpha test enabled, threshold `112`, published as `"alphaMode":"MASK","alphaCutoff":0.43921568989753723`),
so the hypothesis in the brief is **falsified** and no reconversion is needed to fix it. They are
invisible because Bevy's PBR fragment shader builds the alpha that the test compares as
**`vertex_color.a * texture.a`**, and Skyrim's tree NIFs carry vertex colours whose **alpha channel is
not opacity** - it is the per-vertex shader parameter that `SLSF1_VERTEX_ALPHA` reads: **176 of the 296
vertices of the leaf shape `TreePineForest03_1:1` sit below the 0.4392 cutoff** (110 of them at alpha 0.0),
so most of the canopy can never pass the test and the rest survives only as the spray of dots the frames show.

---

## 1. The hypothesis, and what falsified it

| The brief's hypothesis | What the data says |
|---|---|
| An alpha-*tested* NIF reaches `render.rs`'s `StraightAlpha \| Unsupported => AlphaMode::Blend` fallback | The converter maps a test-enabled `NiAlphaProperty` to `NifAlphaMode::Cutout` **before** it looks at the blend bit (`crates/converter/src/material.rs:1109-1117`) and publishes it as `"MASK"` with a cutoff (`crates/converter/src/material.rs:536-558`). No sampled foliage material is `BLEND`. |
| Blended leaf cards fade to nothing through mip-averaged alpha | The foliage is `MASK`; the dissolve is not a mip effect - it is present on the *near* tree in the crop, whose trunk is rendered solid next to it. |
| Alpha-to-coverage dithering is not the cause (confirm only) | Confirmed: `Msaa::Off` at all six sites (`crates/engine/src/app.rs:598,847,1014,1219,1983`, `crates/engine/src/portal.rs:702`). Bevy's `alpha_discard` reaches its `MAY_DISCARD` mask path, which is a plain `discard`, not coverage dithering (`bevy_pbr-0.19.0/src/render/pbr_functions.wgsl:110-127`). |

**The natural experiment that settles it.** In `local/reference/rw1/RW-01-village-from-the-south.png`
the foreground ferns and the wall vines render through the *same* renderer, the *same* KTX2 texture
pipeline and the *same* frame: the ferns are solid and fully detailed, the tree canopy above them is a
spray of green dashes on the sky. `local/reference/rw1/RW-07-village-from-the-south-hill.png` repeats it -
houses, doors, fences, bushes and carved posts all solid, the trees behind them dissolving - and
`clover01`, `swordfern01` and `pineshrub01` (the models the bushes are) convert with **no `COLOR_0`
attribute at all**, while `treepineforest01/03/04` all carry one. Texture, mip and blend-state
explanations cannot separate those two sets; the vertex colour can.

---

## 2. The chain, end to end

| Step | Where | What it does |
|---|---|---|
| NIF flags decode | `vendor/project-wormhole-nif/src/nif_flags.rs:164-192` (the nif.xml `AlphaFlags` bitfield is quoted there) | bit 0 = Alpha Blend, bits 1-4 source factor, bits 5-8 destination factor, **bit 9 = Alpha Test**, bits 10-12 test function (default `TEST_GREATER`), bit 13 No Sorter. `AlphaFlags::test_enabled()` is `flags & 0x0200` (`:189-191`). |
| NIF block | `vendor/project-wormhole-nif/src/nif_block.rs:1404-1408` | `NiAlphaProperty { parent, flags: AlphaFlags, threshold: u8 }`. |
| Converter decision | `crates/converter/src/material.rs:1103-1117` | Test bit first: `if property.flags.test_enabled() { return (NifAlphaMode::Cutout, Some(property.threshold), Some(block)) }`; the blend bit is only consulted when the test bit is clear. Untouched by `b4ce0d9` (that commit changed only the *shader-flag* fallback below it, `:1118-1131`). |
| Converter publish | `crates/converter/src/material.rs:524-560` | `Cutout` → `"alphaMode":"MASK"`, `alphaCutoff = f32::from(threshold) / 255.0` (`:556-558`). Blend factors are published only for `BLEND` (`:531-535`), so a test material carries none. |
| glTf load | `bevy_gltf-0.19.0/src/loader/gltf_ext/material.rs:85` | `AlphaMode::Mask => AlphaMode::Mask(material.alpha_cutoff().unwrap_or(0.5))`. |
| Vertex colours load | `bevy_gltf-0.19.0/src/vertex_attributes.rs:303` | `COLOR_0` → `Mesh::ATTRIBUTE_COLOR` (RGBA), unconditionally: nothing in the engine or the loader drops it. |
| Engine override | `crates/engine/src/render.rs:767-790` | `skyrim_material` replaces the material only when the `OPEN_SKYRIM_material` extension publishes a blend pair (`render.rs:699-717`). The foliage publishes none, so `alpha_mode = skyrim_alpha_mode(..).unwrap_or(material.alpha_mode)` (`:774`) leaves `Mask(0.43921569)` exactly as loaded. Nothing overrides it. |
| Engine validation | `crates/engine/src/streaming.rs:1985-1995` | `Mask(cutoff)` is accepted when finite and in `0..=1`; no rewriting. `app.rs:650-670` (the canonical-material fixture) asserts `Mask` survives - i.e. the engine's own test suite treats a preserved mask as correct. |
| Fragment shader | `bevy_pbr-0.19.0/src/render/pbr_fragment.wgsl:54-55`, `:194` | `#ifdef VERTEX_COLORS { base_color = in.color; }` then `base_color *= textureSampleBias(base_color_texture, uv, ..)` - **the vertex alpha is multiplied into the alpha the test will see**. |
| The test | `bevy_pbr-0.19.0/src/render/pbr_functions.wgsl:107-128` | `:119-127` is the mask branch: `if color.a >= material.alpha_cutoff { color.a = 1.0 } else { discard }` (`:121`). |

So the tested alpha for every foliage fragment is `vertex_alpha(interpolated) × texture_alpha`. What Skyrim
tests there is argued rather than read: the same assets are dense in the game's own render
(`local/reference/riverwood/SR-place-Riverwood.jpg`), the flag that asks a Skyrim shader for per-vertex
alpha is `SLSF1_VERTEX_ALPHA` (shader flags 1 bit 3, `crates/converter/src/material.rs:1120-1131`), and no
shader input of that kind is an opacity multiplier - so whatever Skyrim's leaf shader does with the alpha,
it does not gate the alpha test on it. The two agree only where the art happens to have written vertex
alpha 1.0, which is why the same model renders dense there and as dots here.

`b4ce0d9` is not re-trodden and not undone by any of this: it stops a shape with **no** `NiAlphaProperty`
from becoming `BLEND` because of the `SLSF1_VERTEX_ALPHA` shader flag. That is the class-A ice/rock/snow
bug (`docs/research/transparent-and-misplaced-meshes.md:43`), those materials are `OPAQUE` today, and an
`OPAQUE` material ignores the vertex alpha (`alpha_discard` sets `color.a = 1.0`,
`pbr_functions.wgsl:110-113`). This report is about the other half: a material that *is* alpha-tested on
purpose, whose test now runs on a product the game never intended.

---

## 3. Source NIF versus converted glb (the core table)

Alpha properties were read out of the source NIF bytes in `C:/Modding/SkyrimConverted/vfs/meshes/`
(the converter's extracted copy of the game's `Data`), by dumping the file as a hex stream and locating
the `NiAlphaProperty` struct tail (`od -An -v -tx1 <nif> | tr -d ' \n' | grep -bo '<flags><threshold>'`).
Each struct's byte context was checked once by hand: for `treepineforest03.nif` at byte 31380 the dump is
`.. ff ff ff ff | ec 12 70 | ff ff ff ff 00 00 00 00 ff ff ff ff 0e 00 08 08 ..`, i.e. the three
object-net references, the 2-byte flag word and the threshold byte of `nif_block.rs:1404-1408`.

The vertex-alpha columns come from the `COLOR_0` accessor's buffer view: its byte offset from the glb's
`bufferViews` array plus the BIN chunk's start (`12 + 8 + jsonLength + 8`) is the file offset of the first
vertex, and each vertex is 16 bytes. With `od -An -v -tf4 -w16 -j <offset> -N <16 × vertices>` each line is
one vertex's RGBA, and `cut -c49-` keeps the fixed-width alpha field, so `sort -n | uniq -c` is the exact
histogram quoted below (the `-v` matters: without it `od` collapses repeated lines into `*`).

| Model (Riverwood-placed) | `NiAlphaProperty` in the source NIF | Decoded | glb materials | `COLOR_0`? | vertex alpha |
|---|---|---|---|---|---|
| `Landscape\Trees\TreePineForest01.nif` | `ec 12 70` ×2 | 0x12EC: test **on** (bit 9), `TEST_GREATER`, threshold **112**, blend off | 2 × `MASK` cutoff 0.43921569, 2 × `OPAQUE` | yes, 4 accessors | (not sampled) |
| `Landscape\Trees\TreePineForest03.nif` | `ec 12 70` ×2 | same | 2 × `MASK` 0.43921569, 2 × `OPAQUE` | yes, 4 accessors | leaf shape `:1` (mesh 3, mask): 110 at 0.0, 66 at 0.251, 38 at 0.627, 20 at 0.753, 62 at 1.0 - **176 of 296 below the cutoff**. Mesh 0 (mask): 117 at 0.0, 116 at 0.251, 30 at 0.502, 38 at 0.627, 20 at 0.753, 62 at 1.0 - **233 of 383 below** |
| `Landscape\Trees\TreePineForest04.nif` | `ec 12 70` ×2 | same | 2 × `MASK` 0.43921569, 2 × `OPAQUE` | yes, 4 accessors | mesh 0 (mask): 164 at 0.0, 107 at 0.125, 5 at 0.2, 200 at 0.251, 6 at 0.337, 22 at 0.502, 4 at 0.627, 41 at 0.753, 20 at 1.0 - **482 of 569 below the cutoff**. Mesh 3 (mask): 212 of 289 below. The byte pattern `ed ed ed 40` (237,237,237,64, the dominant colour) occurs 84 times in the NIF itself. |
| `Landscape\Plants\Clover01.nif` | `ec 12 37` | 0x12EC, threshold **55** | 1 × `MASK` 0.21568628 | **no** | - |
| `Landscape\Plants\SwordFern01.nif` | `ec 12 37` | 0x12EC, threshold **55** | 1 × `MASK` 0.21568628 | **no** | - |
| `Landscape\Plants\PineShrub01.nif` | `ec 12 80` | 0x12EC, threshold **128** | 1 × `MASK` 0.50196081 | **no** | - |
| `Architecture\Farmhouse\Farmhouse01.nif` | `ec 12 80` ×1, `ec 12 c8` ×2 | 0x12EC, thresholds **128** and **200** | 1 × `MASK` 0.50196081, 2 × `MASK` 0.78431374 | **no** | - |

Findings from the table:

* **The number of test-enabled properties in the NIF equals the number of `MASK` materials in the glb**
  for all seven models (2, 2, 2, 1, 1, 1, 1+2). The converter reads both the flag and the threshold
  faithfully, and publishes the threshold with the right arithmetic: `112/255 = 0.43921568989753723`,
  `55/255 = 0.21568627655506134`, `128/255 = 0.501960813999176`, `200/255 = 0.7843137383460999` - all
  four appear in the glbs bit-for-bit as computed, and 128 and 200 were confirmed against
  `farmhouse01.nif`'s own bytes.
* **Foliage is `MASK`, not `BLEND`.** The brief's hypothesis is dead at the glTF level.
* **The vertex colours are real NIF data, not a converter artefact**: the values are byte-quantised
  (`0.2509804 = 64/255`, `0.75294125 = 192/255`) and the dominant tuple of `treepineforest04` was found
  in the source file at the expected count. In `treepineforest04` mesh 0 the RGB is *constant*
  (0.92941177) while the alpha varies over 0.0/0.125/0.251/0.502/0.753 - a parameter channel, not shading,
  which is what a per-vertex shader input looks like.
* The blend factors authored in the same word (bits 1-4 = 6 `SRC_ALPHA`, bits 5-8 = 7 `INV_SRC_ALPHA`) are
  inert because bit 0 is clear - and the converter correctly publishes none of them
  (`crates/converter/src/material.rs:190-201`, `:531-535`).

Two small deviations from Skyrim worth recording, neither of them the cause of the picture:

* The converter ignores the test **function** (bits 10-12): any test-enabled property becomes `MASK` with
  Bevy's `>=`, so a `TEST_LESS`/`TEST_NEVER` property would be mapped to the wrong test. All twelve
  sampled properties are `TEST_GREATER` (bits 10-12 = 4).
* Skyrim's `TEST_GREATER` is strictly greater than `threshold/255`, Bevy's test is `>=`
  (`pbr_functions.wgsl:121`), which differs only for texels whose alpha is exactly the threshold byte.

---

## 4. Why the product alpha dissolves the canopy

`TreePineForest03`'s two leaf materials (`TreePineForest03_1:1` and the unnamed mesh 0) are the only
`MASK` shapes; the two `OPAQUE` shapes are the trunk/bark (`TreePineForest03_1:0`, vertex alpha 1.0
throughout, base texture `treepineforestbarkcomp`) - which is exactly why the trunks stay solid in the
crop while the needles do not. With cutoff 0.43921569:

* A fragment survives only where `vertex_alpha × texture_alpha ≥ 0.4392`.
* 59% of `TreePineForest03_1:1`'s vertices (176 of 296) and 85% of `TreePineForest04` mesh 0's (482 of 569)
  carry alpha below the cutoff, and on those triangles only the corner(s) whose interpolated alpha climbs
  (toward a 0.63/0.75/1.0 vertex) can pass, and only where the texture's own alpha is near 1. That is the
  geometric signature of the crop: isolated specks clustered along the branches where the art's alpha runs
  high, sky between them.
* The surviving specks are the densest (brightest) needle texels, and these materials are `SLSF1_Own_Emit`
  (`"emissiveFactor":[1.0,1.0,1.0]` with the *diffuse* texture in `emissiveTexture`), so what is left is
  also drawn self-lit, not shaded - hence the pale blue-white dust rather than dark green needles. The
  Own_Emit is a brightness problem on top of the geometry problem, not its cause
  (see `docs/research/visual-gaps-spec.md` gap 1 and the `is_deliberate_glow` split in
  `crates/engine/src/render.rs:757-766`).

---

## 5. Scale

| Measure | Count | How |
|---|---|---|
| material entries published `MASK` across all converted `.glb` | **12,372** | `grep -rhao '"alphaMode":"[A-Z]*"' C:/Modding/SkyrimConverted/meshes/ \| sort \| uniq -c` |
| material entries `BLEND` | 5,588 | same |
| material entries `OPAQUE` | 41,308 | same |
| converted models containing at least one `MASK` material | 6,872 | `grep -rla` |
| converted models containing a `COLOR_0` attribute | 1,337 | `grep -rla` |
| models with **both** (the population this bug can bite) | **677** | intersection of the two lists |
| of those 677, the `trees` category | 36 files | includes all `treepineforest01-05`, their snow variants, the `_lod_flat` billboards, `treeaspen01-06`, `winteraspen`, `dlc02` ash pines |

**How many of the alpha-tested materials end up `BLEND`?** None, and that is structural rather than
lucky: `alpha_contract` returns `Cutout` for a test-enabled property before it can consider the blend bit
(`crates/converter/src/material.rs:1111-1116`), so the 5,588 `BLEND` materials come from blend-enabled or
premultiplied/screen-door shapes, never from the test flag. Note the second-order collision the fix should
also consider: a `BLEND` material's opacity is scaled by the same `vertex_color.a`
(`pbr_fragment.wgsl:54-55`, `:194`), so the 677 models' effect cards are drawn thinner than authored too.

Caveat on 677: it counts *files* that have a `MASK` material somewhere and a `COLOR_0` attribute
somewhere, not the exact material pairs. The tree family was verified shape by shape; the rest is an upper
bound. Whether every one of the 677 actually has vertex alpha below its cutoff was sampled
(trees: yes; `clover01`/`swordfern01`/`pineshrub01`: no `COLOR_0` at all, so unaffected).

---

## 6. The fix, and where it belongs

**Arithmetic.** Skyrim keeps a texel when `texture_alpha > threshold_byte/255`; the converter already
computes `alphaCutoff = threshold/255` (`material.rs:556-558`), so for the foliage the recovery is
`112/255 = 0.43921569` and for the plants `55/255 = 0.21568628` - both already in the glb. **No threshold
has to be recovered, and no NIF has to be re-read** to know what the alpha test should be. What is wrong is
the *input* to that test, and the input is wrong on the CPU side of the loader, not in the data.

**Recommended: engine-side, at mesh load (no reconversion).**

1. Rewrite loaded meshes: for any `Mesh` carrying `Mesh::ATTRIBUTE_COLOR`, set the alpha component of every
   vertex to `1.0` and keep RGB. The engine uses vertex-colour *RGB* nowhere today except Bevy's own
   multiply (Skyrim's baked shading, which should stay), and it uses the alpha nowhere except
   `crates/engine/src/shaders/snow.wgsl:70` (below). A ~30-line system on `AssetEvent<Mesh>` added/updated,
   registered next to the existing material handler pattern (`crates/engine/src/render.rs:807-853`) does it
   for the 1,337 models that carry `COLOR_0`. It is idempotent, it costs one pass per mesh, and it leaves
   the wind/projection alpha recoverable on disk for a later wind implementation.
2. Repair the snow shader's premise. `crates/engine/src/shaders/snow.wgsl:66-71` says *"No converted mesh
   does today (`crates/converter/src/mesh.rs` writes no vertex colours), so this is dead code"* and then
   multiplies coverage by `in.color.a`. **That premise is false** - 1,337 converted models carry `COLOR_0`,
   and the `#ifdef VERTEX_COLORS` branch of Bevy's own PBR shader is live for them. With the data in place
   (`cfe4893` is inert only until the snow tables exist), snow coverage would be silently scaled by the
   wind-weight alpha - a 0.251-alpha roof would keep a quarter of its snow. Force the alpha to 1.0 and the
   branch becomes what it was written to be.

**Alternative: converter-side, at export.** Force the alpha component of the vertices appended at
`vendor/project-wormhole-nif/src/model/model.rs:176-186` and `:640-680` (the `COLOR_0` accessor) to 1.0,
then bump `CONVERTER_SCHEMA_VERSION` and reconvert. Same result, three costs: it needs the full
reconversion the user has not run yet, it touches `vendor/`, and it destroys the alpha channel that the
engine's own snow projection wants to read later. If the data model is to be fixed at the source, export
the parameter channel as its own attribute rather than dropping it.

**What not to do.** Do not "fix" this by re-reading `NiAlphaProperty` differently or by bumping the
converter version: the flag, the threshold, the cutoff arithmetic and the `MASK` publication are all
correct (`b4ce0d9` is a *related* but separate fix and stays as it is). Do not chase alpha-to-coverage or
mip bias - `Msaa::Off` everywhere, and the dissolve is present on near geometry.

**One line in the record worth correcting.** Three places assume the converter publishes no vertex colours
and use that as a load-bearing premise: `crates/converter/src/material.rs:1118-1131`, the comment in
`crates/engine/src/shaders/snow.wgsl:67-69`, and `docs/research/transparent-and-misplaced-meshes.md:43`
("GLB attributes are `NORMAL,POSITION,TEXCOORD_0` only"). The `COLOR_0` writer has been in the vendored
exporter since `33682e6` (2026-09-10), twelve days before research-011 was written; the class-A
conclusion still holds (those shapes are `OPAQUE`), but the premise is wrong and this bug lives in the gap
it left.

---

## 7. Frames that would show the fix, and what "correct" looks like

| Frame | Today | After the fix |
|---|---|---|
| `local/reference/rw1/RW-07-village-from-the-south-hill.png` | Trees behind the houses are a grey-green filigree of specks against the sky; the thatch, walls, doors and fences are solid | Dense, dark, closed pine canopies - the silhouette in `local/reference/riverwood/SR-place-Riverwood.jpg`; every building, bush, fence and stone in the frame unchanged (it is the control group) |
| `local/reference/rw1/RW-01-village-from-the-south.png` | Tree canopy at the top-left is green dashes on blue sky, with solid ferns in the foreground of the same frame | The canopy fills in; the ferns must not change at all |
| `local/reference/rw_sweep/A-village-19000--48800-900.png` (and its `_crop-tree.png`) | The pine reads as a spray of dots beside a solid trunk | A continuous dark-green canopy mass over the same trunk; the trunk's own colour and silhouette unchanged |
| `local/reference/rw_sweep/A-village-18500--49500-1800.png`, `...-19500--48200-1400.png` | same dissolve at two more angles | same check; useful because the sun angle differs, so the surviving-specks-brightest artefact disappears with the fix |

A correct render also keeps the trunks identical, so a before/after diff confined to the canopies is the
acceptance signal; if the trunk edges move, something other than the alpha path was touched.

---

## 8. What I could not check, and open questions for the lead

* **No DB queries.** This session refused `python`, `cargo`, `sqlite3`, `perl` and scratch scripts (a read
  block with no approval surface), so `skyrim_world.db` was not queried. Everything above was measured from
  the converted `.glb` bytes, the raw NIF bytes in `C:/Modding/SkyrimConverted/vfs/`, and the converter and
  engine sources. The Riverwood model list came from `docs/design/riverwood-demo.md` section 4 and the
  converted tree directory, not from the placement table, so "these models are placed in Riverwood" rests
  on that document. Someone with Python can confirm the reference counts directly.
* **No render, per the brief.** The mechanism is proved by code and data; the visual claim is the frames
  above, which the lead can re-shoot. The cheapest decisive test is a one-line experiment: apply the
  alpha-to-1.0 rewrite to glb meshes whose path contains `landscape/trees/` and re-render RW-07 and
  RW-01 - if the canopies fill in and the ferns, houses and trunks do not move, the diagnosis is sealed.
* **The roof, thatch and wall speckle in the same crop is *not* this cause.** `Farmhouse01.nif` carries no
  vertex colours at all (no `COLOR_0` in the glb), so its `MASK` materials test the texture's alpha alone.
  Its cutoffs are 128/255 and **200/255** - the second is aggressive, and mip-averaged alpha on a thatch
  texture at a 0.78 cutoff will erode silhouettes at distance; the other candidate is the snow overlay
  (`crates/engine/src/shaders/snow.wgsl`). Both are unverified here and neither is needed to explain the
  trees. Worth a separate task; if the two problems do share anything, it is only that both live in the
  alpha path of the same material handler.
* **Only eight NIFs were read.** A global scan of every alpha property (rather than the per-model samples
  above) would also settle whether any shipped model uses a test function other than `TEST_GREATER`, which
  the converter currently ignores.
* **No web lookup was possible.** The brief allowed one for the `NiAlphaProperty` bit layout, but this
  session has no web tool, so the layout is quoted from the vendored crate's own nif.xml block
  (`vendor/project-wormhole-nif/src/nif_flags.rs:164-192`) - the source the converter's decode is written
  against. It agrees with the flag words actually found in the assets: `0x12EC` decodes to test-on,
  `TEST_GREATER`, blend-off, which is exactly what those shapes' materials are.
