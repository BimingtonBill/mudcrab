# What the converter silently drops

**Source:** `research-076-converter-dropped-fields.1` (read-only audit by a DeepSeek worker over
`crates/converter/**` and `vendor/project-wormhole-nif/**`), reviewed and augmented by
`lead-070-converter-materials.1`, whose tree-wide counts are marked **measured** below. Measured on
`C:/Modding/SkyrimConverted` at schema 18 (complete): 22,761 GLBs, 1.3 GB, 59,268 materials.

**Why this exists.** Two bugs of exactly this shape - a field read and then silently reinterpreted -
had been wrong for every asset since the project began, and both were found by accident, by looking
at a render:

1. Skyrim's Blinn-Phong glossiness *exponent* was published as a linear percentage
   (`roughnessFactor = 1 - glossiness/100`), wrong for all 59,268 materials (fixed in impl-062).
2. `SLSF1_VERTEX_ALPHA` was treated as opacity, so 1,337 meshes rendered translucent (impl-063; the
   vertex-colour alpha is a shader parameter, not transparency).

The list below is the systematic version. Everything in it is *current* behaviour at schema 18.

**Counts.** The counts marked **measured** are occurrences in the published GLB JSON, counted by the
lead with `grep -rahoE '<pattern>' C:/Modding/SkyrimConverted/meshes | sort | uniq -c` on 2026-09-23.
A GLB's JSON chunk is plain text; one occurrence is normally one material or one mesh attribute, but
`extensionsUsed` adds per-file occurrences for the `KHR_*` names, so material counts from those keys
are lower bounds. Counts the audit could not measure are marked **unmeasured**, with the search that
would measure them.

---

## 1. Ranked inventory: read, then dropped or lossy

| # | field | read at | what happens today | what it should become | affected | confidence |
|---|---|---|---|---|---|---|
| 1 | **inline vertex colours** - `BSVertexData.vertex_colors` (`vendor/.../bs/vertex.rs:15`, parsed `:143-155`) | used **only** by the skin-partition fallback (`vendor/.../nif_file.rs:620-632`); `tri_shape_to_mesh` (`:442-458`) copies positions/normals/UVs only and `BSTriShape` has no `get_vertex_colors` | every static shape with inline vertex data loses its colours; only partition-backed and legacy geometry reaches `COLOR_0` | copy the colours in `tri_shape_to_mesh` (**vendor-side**) | **8,856** material entries declare `shaderFlags2` bit 5 `Vertex_Colors` against **2,660** measured `COLOR_0` attributes; the flag is only published for a subset, so the gap is a floor *(measured, both sides)* | high |
| 2 | **environment cube** - `EnvironmentCube` slot 4 | `material.rs:1034-1081`, published in `OPEN_SKYRIM_material.textureSlots` | published, real KTX2 cubemap, **nothing reads it**; and it is labelled `"colorSpace":"linear"` while the same file is written with the sRGB transfer function (`texture.rs:44-53`, `:927-929`) | label/URI agree with the file (research-077: two one-line fixes at `material.rs:694-700`); engine binds it (see `docs/design/environment-map-publishing.md`) | **7,823** cubes, **3,096** env masks *(measured)* | high |
| 3 | `uv_offset`, `uv_scale` (lighting `vendor/.../nif_block.rs:690-691`; effect `:581-582`) | **never** (no hit under `crates/converter/src`) | dropped: a tiled or scrolled surface renders 1:1 at UV 0 | `KHR_texture_transform` on the base-colour texture (Bevy 0.19 reads it there only) plus the raw values in the extension | **unmeasured**; `KHR_texture_transform` appears **0** times *(measured)* - `grep -rahoE '"uvOffset"'` on the source NIFs would size the need | high |
| 4 | `specular_strength`, `specular_color` | `material.rs:926`, `:925` | clamped to `[0,1]` before publishing (`:658-659`); Bevy reads `specularFactor` as `reflectance = factor * 0.5`, so everything above 1 is lost, not saturated | keep the raw value in `OPEN_SKYRIM_material` and let the engine scale | **42,935** materials sit exactly at `specularFactor 1.0` (the clamped population) *(measured)*; how many were authored > 1 is unmeasured | high |
| 5 | `emissive_multiple` / effect `base_color_scale` (`nif_block.rs:694`, `:593`) | `material.rs:931`, `:999` | `emissiveStrength = max(1.0)` and published only when `> 1` (`:636-641`): every authored value **below** 1 is raised to 1 (a 0.5 glow renders at full strength). 0 is the deliberate guard against effect cards | publish whenever `!= 1` (`emissiveStrength` accepts < 1) | **unmeasured** (4,201 materials publish a strength > 1; the < 1 population is invisible because the key is omitted) | high |
| 6 | **tangents and bitangent sign** (`bs/vertex.rs:9`, `:13`, parsed `:60-138`; legacy `legacy.rs:95-101`) | parsed, then thrown away; the vendored crate has **no** `Semantic::Tangents` write site anywhere | no published GLB can carry `TANGENT`; Bevy derives the normal-map frame from UVs, so mirrored UV islands invert lighting | publish `TANGENT` (xyz + w from the bitangent) (**vendor-side**) | `TANGENT` appears **0** times against **52,919** `normalTexture` references *(measured)* | high |
| 7 | second UV set (`legacy.rs:113-120` keeps set 0 only) | parsed then discarded | only `TEXCOORD_0` is ever written; decals/lightmaps that use UV1 cannot be expressed | publish `TEXCOORD_1` where a shader uses it | `TEXCOORD_1` appears **0** times *(measured)* | high |
| 8 | `texture_clamp_mode` (lighting `nif_block.rs:695`, effect `:584`) | **never** | the converter writes **no texture `samplers` array**; every texture samples REPEAT+linear regardless of the NIF's clamp mode (decals, atlases, `BlackPlane01`-style tricks) | write per-texture samplers | **unmeasured**; the 1,951 `"samplers"` occurrences are animation samplers (839 files carry `animations`) | high |
| 9 | `lighting_effect_1`, `lighting_effect_2` (`nif_block.rs:701-702`) | **never** | dropped; nif.xml names `EnvironmentMapScale` as an animatable float of this block (`nif_enum.rs:1930`, `:1951`) and the env shader types "use Envmap Scale" (`nif_flags.rs:45`), so the pair are the only unnamed floats left in the block and the only candidates | measure the two floats on env-mapped NIFs and controls, then publish the static value (and its controller) if it is real data | **unmeasured** - one dump settles it (research-077 §7) | medium |
| 10 | `refraction_strength` (`nif_block.rs:697`) | **never** | dropped; the refraction *flags* survive only as raw bits | publish, or route to Bevy's `specular_transmission` | **unmeasured** | high |
| 11 | effect-shader falloff and depth fields: `lighting_influence`, `env_map_min_lod`, `falloff_start/stop_angle`, `falloff_start/stop_opacity`, `soft_falloff_depth` (`nif_block.rs:585-594`) | **never** | dropped; soft-particle depth fade and lighting influence are invisible | publish | **unmeasured** (6,485 effect cards is the ceiling, cited from research-058) | high |
| 12 | material float/colour controllers - `BSLightingShaderPropertyFloatController.controlled_variable` (`nif_block.rs:742-746`), `NiVisController`, `NiFloatInterpolator`, `NiUVData` | **never**; `nif_animation.rs:746-808` returns `Ok(None)` for any interpolator that is not a `NiTransformData` | a NIF that scrolls a texture, pulses a glow or its alpha publishes the static value only | export float/colour tracks, or at least record them | **unmeasured** | high |
| 13 | hidden/no-render flags - `NiAVObjectFlags.hidden()` (`nif_block.rs:1639`) | defined, **never called** | a shape or node flagged hidden still renders | skip hidden shapes/nodes | **unmeasured** | high |
| 14 | unsupported node types - `NiBillboardNode`, `NiSwitchNode`, `BSMultiBoundNode`, `BSOrderedNode`, `BSTreeNode`, `BSLeafAnimNode`, `BSValueNode`, `BSBlastNode`, `NiLODNode` (`nif_block.rs:8-116`) | `populate_static_scene` (`vendor/.../nif_file.rs:490-575`) recognises `NiNode` and `BSFadeNode` only | their transforms are dropped and their children re-rooted, so anything under them is placed as if the parent were at the origin | walk those node types as nodes (**vendor-side**) | **unmeasured** | high |
| 15 | sequence metadata: `weight`, text keys, `cycle type` (loop/clamp), frequency, priority | `nif_animation.rs:671-707` | dropped; only node T/R/S becomes a clip | document; `cycle type` is real behaviour | **unmeasured** | high |
| 16 | `texture_set_block`, `alpha_property_block` (`ValidatedNifMaterial`) | `material.rs:915-916` | never published anywhere; they exist only inside a run | extras for provenance/debugging | **unmeasured** (12 grep hits, none a write site) | high |

### Two more lossy paths that are deliberate

| path | today | note |
|---|---|---|
| dangling-texture prune (`mesh.rs:313-400`, impl-067) | an image whose file is missing is removed, and every material slot pointing at it is removed too - including a required base colour | deliberate and counted (182 meshes / 511 references in the run that was measured), but the loss is not recorded in the GLB |
| deferred dynamic categories (`mesh.rs:466-477`: `/actors/`, `/magic/`, `/effects/`) | those NIFs publish empty-scene GLBs | **4,478** published `"nodes":[]` GLBs *(measured)* - 20 % of the tree is intentionally empty until Phase 4 |

## 2. Published from a constant

| key | constant | source | note |
|---|---|---|---|
| `pbrMetallicRoughness.metallicFactor` | `0.0` | `material.rs:533`, `:359` | correct for now (research-058 §5.1): Skyrim has no metalness channel and the engine binds no environment map yet, so `1.0` would darken Dwemer metal. **59,268 materials** *(measured)* |
| `normalTexture.scale` | `1.0` | `material.rs:582` | no NIF equivalent; fine |
| `texCoord` on every texture reference | `0` | `material.rs:539, :582, :633, :664` | forced by the missing UV1 (row 7) |
| `colorSpace` in `textureSlots` | `"linear"` for everything except `Detail` | `material.rs:700` | **measured: `"srgb"` appears 0 times, `"linear"` 15,426** - so the env cube (row 2) is the whole of the mislabelled population; `Detail` is unused (`"semantic":"detail"` 0 times) |
| `slot.required` | `true` only for slot 0 and env slots 4/5 | `material.rs:1077` | matches the NIF's own required set |
| `emissiveStrength` | `max(1.0)` | `material.rs:636` | row 5 |
| no `samplers` array | - | nothing writes it | row 8 |
| exclusion material | `MASK`, `alphaCutoff 1.0`, `baseColorFactor [0,0,0,0]` | `material.rs:353-367` | renders nothing because alpha 0 fails a cutoff of 1 |

## 3. Published correctly, ignored by the engine (engine-side gaps)

- **All of `OPEN_SKYRIM_material` except the blend pair.** The engine reads the extension
  (`render.rs:602`) and `published_blend_pair` (`render.rs:698-723`) only, so `textureSlots` (every
  `environment_cube`, `environment_mask`, `inner_layer`, `greyscale`, `height`, `unclassified`),
  `shaderFamily`, `lightingShaderType`, `shaderFlags1/2`, `premultipliedAlpha` and
  `screenDoorAlphaFade` are inert. `environment_cube` in particular has no consumer.
- `KHR_materials_specular` and `KHR_materials_emissive_strength` are **not** gaps: Bevy applies both
  (`bevy_gltf-0.19.0/src/loader/mod.rs:1483-1495`, `:1415`), which is what makes rows 4 and 5 real
  losses today rather than dormant ones.

Published slot semantics *(measured)*: `environment_cube` 7,823; `greyscale` 3,221;
`environment_mask` 3,096; `inner_layer` 655; `unclassified` 620; `height` 11; `detail` 0.
Published `lightingShaderType` (extension subset only): `environment_map` 7,152; `default` 855;
`multi_layer_parallax` 662; `glow` 348; `eye_environment_map` 18; `parallax` 11; `hair_tint` 2.

## 4. Vendor-side findings (`vendor/project-wormhole-nif/**`; not the converter's to fix)

1. No `TANGENT` write site exists at all (row 6).
2. `tri_shape_to_mesh` drops inline vertex colours (row 1).
3. `populate_static_scene` walks one node type plus `BSFadeNode`; every other node block's transform
   is lost and its children re-rooted (row 14).
4. `BSLightingShaderProperty` parsing stops at `lighting_effect_2`; whether a shader-type trailer
   exists (parallax scale/passes, MLP layer thickness, inner-layer UV scale, env-map scale) is
   **not established** - one NIF with its declared block size settles it (research-076 addendum).
5. `BSSPWetnessParams` (including its `metalness` and `env_map_scale`) is dead code: nothing
   constructs it.
6. `NiAVObjectFlags::hidden()` is never called (row 13).
7. `export/gltf.rs` is a single comment line - the material writer is `model/model.rs` and
   `nif_file.rs`, not that file.

## 5. Top five by likely visual impact

1. **Inline vertex colours** (row 1): `multi_layer_parallax` ice and the DLC1 `_1layer` icebergs
   would blend their inner layer the way Bethesda authored it instead of drawing one flat layer.
2. **Environment cubes** (row 2): the Dwemer halls' bronze, the Alftand ice doors and the DLC1
   icebergs would stop reading as flat matte rock.
3. **`uv_scale`/`uv_offset`** (row 3): effect cards, banners, water and lava scroll would tile at
   their authored scale.
4. **Specular clamping** (row 4): metal, glass and ice lose highlight intensity the NIF asks for.
5. **No `TANGENT` and no samplers** (rows 6, 8): normal-mapped surfaces use Bevy-generated frames
   and every texture samples REPEAT+linear.

## 6. Unmeasured, and how to measure it

All of these are single read-only passes over the 1.3 GB tree; the lead measured the counts above in
under two minutes with:

```sh
grep -rahoE '"semantic":"[a-z_]+"' C:/Modding/SkyrimConverted/meshes | sort | uniq -c | sort -rn
```

NIF-side fields need one pass over `C:/Modding/SkyrimConverted/vfs/meshes/**` (the source NIFs are
kept in the staging VFS): `uv_offset/uv_scale != default`, `texture_clamp_mode` values,
`lighting_effect_1/2` values, `refraction_strength`, and the shader block's declared size. The
converter's own `nif-audit` binary (`cargo run -p converter --bin nif-audit <vfs/meshes> <out.json>`)
counts block types and exposes the parser's "bytes left over" warnings in one run.

## 7. Corrections and open disputes

- **The shader-block payload layout is disputed.** `research-077` derived 97 bytes + `4·extra_data`
  with `alpha` at +61 (its clamp mode treated as one byte); `research-076` derived 100 bytes with
  `alpha` at +64, because `TexClampMode` is `#[repr(u32)]` in the lighting shader
  (`nif_enum.rs:875-883`) while only `BSEffectShaderProperty` uses a byte (`nif_block.rs:584`).
  `research-076`'s version is corroborated by published data: `dwefacadetowerroof01.glb`'s
  `roughnessFactor` back-solves to glossiness exactly `400.0`, and every probed material has
  `baseColorFactor` alpha exactly `1.0` - a 3-byte-early read would be garbage. **Treat 100+4n as
  the working layout and the trailer question as open** until one NIF is dumped.
- `research-076` withdrew its stronger claim that a trailer certainly exists; the two reports agree
  the byte values are unmeasured.
- `docs/specs/converters/nif-to-gltf.md:76` still documents the pre-impl-062 roughness formula. Do
  not transcribe from that section; it is stale.
- **Sibling flag (lead-071, not this audit's finding):** `crates/converter/src/esm/exporter.rs:712`
  reads the first `f32` of `XRDS` as a light radius; over 13,677 references that carry an override,
  3,320 are negative and the positives run to 3.7 M units, so the field looks unreliable as a
  radius. No converter change is proposed here; it is recorded so the post-reconversion lighting fit
  knows why the engine guards it.
