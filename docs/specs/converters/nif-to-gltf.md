# NIF to glTF 2.0 / GLB Transformation Specification

This document details the technical specification for converting Bethesda NetImmerse (`.nif`) 3D mesh files into modern, GPU-ready **glTF 2.0 (`.glb`)** binary files.

---

## 1. Overview & Objectives

- **Input:** Skyrim `.nif` file (NiHeader, BSTriShape / NiTriShape, BSLightingShaderProperty).
- **Output:** Standalone glTF 2.0 binary file (`.glb`).
- **Goal:** Convert legacy proprietary 3D geometry into standard PBR-compatible glTF primitives that render zero-copy inside Bevy.

---

## 2. Block Mapping Reference Table

| Skyrim NIF Block Type                            | glTF 2.0 Equivalent     | Conversion Logic                                                                  |
| :----------------------------------------------- | :---------------------- | :-------------------------------------------------------------------------------- |
| **`NiHeader`**                                   | `asset` metadata        | Copy generator & version tags                                                     |
| **`NiNode` / `BSFadeNode`**                      | `nodes`                 | Convert local transform matrix (`translation`, `rotation` quaternion, `scale`)    |
| **`NiBillboardNode`**                            | `nodes`                 | As `NiNode`, plus node extras `{"openSkyrim": {"billboard": <mode>}}`: nif.xml's `BillboardMode` name in camelCase (`rotateAboutUp`, ...), or its number |
| **`BSTriShape` / `NiTriShape`**                  | `meshes` + `primitives` | Extract vertex positions, normals, UVs, tangents, and index buffers               |
| **`BSLightingShaderProperty`**                   | `materials`             | Map Bethesda shader flags to glTF PBR Metallic Roughness properties               |
| **`BSShaderTextureSet`**                         | `textures` + `images`   | Map Skyrim texture slots (`_d.dds`, `_n.dds`, `_s.dds`) to glTF URIs/KTX2 handles |
| **`NiSkinInstance` / `BSDismemberSkinInstance`** | `skins`                 | Map bone indices (`JOINTS_0`) and vertex weights (`WEIGHTS_0`)                    |

---

## 3. Detailed Data Extraction Steps

```
┌──────────────────────┐
│  Skyrim NIF File     │
└──────────┬───────────┘
           │
           ▼ (Binary Reader / nom)
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. Extract Geometry Buffers (BSTriShape)                                    │
│    - Positions:  Vec3<f32>  ➔ glTF Accessor "POSITION"                     │
│    - UV Map:     Vec2<f32>  ➔ glTF Accessor "TEXCOORD_0"                   │
│    - Normals:    Vec3<f32>  ➔ glTF Accessor "NORMAL"                       │
│    - Tangents:   Vec4<f32>  ➔ glTF Accessor "TANGENT"                      │
│    - Indices:    u16 / u32  ➔ glTF Accessor "ELEMENT_ARRAY_BUFFER"         │
└──────────────────────────┬──────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. Map Material & Textures (BSLightingShaderProperty)                       │
│    - Slot 0 (Diffuse)       ➔ baseColorTexture                             │
│    - Slot 1 (Normal Map)    ➔ normalTexture                                │
│    - Slot 2 (Subsurface/Env)➔ metallicRoughnessTexture                     │
│    - Alpha Flags            ➔ alphaMode ("OPAQUE" / "MASK" / "BLEND")      │
└──────────────────────────┬──────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. Build & Write glTF 2.0 Binary (.glb)                                     │
│    - Write JSON Chunk (Nodes, Meshes, Materials, Accessors, Views)          │
│    - Write BIN Chunk  (Interleaved Vertex & Index Buffers)                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Material Parameter Conversion Matrix

Before glTF publication, Mudcrab builds a validated material contract for every reachable shape.
The contract follows the shape's explicit shader, texture-set and alpha-property block references;
block order and filename suffixes are not used to associate or classify materials. Unsupported
properties are recorded as explicit exclusions, while invalid references and non-finite values fail
conversion with the source file, shape block and shader block in the diagnostic.

| Skyrim Shader Feature    | Skyrim Flag / Value                           | glTF PBR Property                                                                                       |
| :----------------------- | :-------------------------------------------- | :------------------------------------------------------------------------------------------------------ |
| **Base Color**           | Diffuse texture (`Slot 0`) + material alpha   | `pbrMetallicRoughness.baseColorTexture` + `baseColorFactor`, interpreted by glTF as sRGB color + alpha |
| **Normal Map**           | Normal texture (`Slot 1`)                     | `normalTexture`, interpreted by glTF as linear data                                                     |
| **Roughness / Specular** | Glossiness value + specular texture (`Slot 7`)| `roughnessFactor = 1.0 - clamp(glossiness / 100.0)` + `KHR_materials_specular`                          |
| **Metallic Factor**      | No validated metalness input in the SSE IR    | Fixed to `0.0`; environment mapping is not misclassified as metalness                                   |
| **Emissive / Glow**      | Glow map (`Slot 2`) or emissive color/strength| `emissiveTexture`, `emissiveFactor` and `KHR_materials_emissive_strength`                               |
| **Two-Sided Rendering**  | `SLSF2_Double_Sided` flag                     | `doubleSided: true` only when the flag is set                                                            |
| **Alpha Transparency**   | `NiAlphaProperty` and alpha-related flags     | `alphaMode: "MASK"` with normalized threshold, or `"BLEND"`                                           |

Height/detail, environment, environment-mask, inner-layer and greyscale slots remain in the
`OPEN_SKYRIM_material` extension because core glTF has no equivalent Skyrim shader semantics.
The extension also records premultiplied-alpha and screen-door-alpha requirements. Texture URIs
always target the canonical KTX2 hierarchy; the semantic DDS-to-KTX2 encoding itself is closed by
the following conversion stage. Once that stage has published the hierarchy, the pipeline prunes
from every GLB the texture URIs whose source texture is absent from the installed game data -
including a base-color URI - together with every core or `OPEN_SKYRIM_material` slot that referenced
them, so a NIF naming a texture Bethesda never shipped still publishes and renders with its
remaining maps. The dropped paths are recorded per mesh under `pruned_texture_references` in
`conversion-manifest.json` and do not make the conversion incomplete.

Both shader families also carry a static UV transform (`BSLightingShaderProperty.uv_offset` /
`.uv_scale`, `BSEffectShaderProperty.uv_offset` / `.uv_scale` in the vendored parser), published as
`uvOffset: [u, v]` and `uvScale: [u, v]` on `MUDCRAB_material` (data-set extension
`mudcrab.material`, version 1). `OPEN_SKYRIM_material` is exactly the core extension of `main` (same
keys, same values, skipped for a material with no auxiliary texture slot and neither alpha flag), so
a mesh from `main`'s converter has no UV keys, no falloff and no blend factors, and a reader treats
an absent `MUDCRAB_material` (or key) as identity UVs, no falloff and the default blend. The
extension is written for a material only when it has a non-default value among its fields (a UV
transform other than the identity `[0, 0]`/`[1, 1]`, effect falloff, blend factors); it is listed in
`extensionsUsed`, never `extensionsRequired`. A
`uOffset`/`vOffset`/`uScale`/`vScale` animation channel (below) replaces the matching one of these
two components at runtime; the other component keeps its static value from here. Every other
animated variable (`alpha`, `emissiveMultiple`, `glossiness`, ...) replaces its own static field
instead, wherever this document or the material contract publishes it.

An effect-shader material also carries its view-angle fade and soft-edge depth exactly as the NIF
stores them, in `MUDCRAB_material`: `falloffStartAngle`, `falloffStopAngle`, `falloffStartOpacity`, `falloffStopOpacity`
and `softFalloffDepth`. Despite their names the two angles are stored as cosines, and the shader
compares them with `|N·V|` directly: `s = smoothstep(saturate((|N·V| - start) / (stop - start)))`,
then `opacity = lerp(startOpacity, stopOpacity, s)`. The fade applies only when `shaderFlags1` has
`Use_Falloff` (bit 6), and the soft edge only with `Soft_Effect` (bit 30). The fields share their
names with the float-controller variables that animate them, so an animated channel replaces its
static field like any other. A material with a non-finite value omits all five.

### 4.1 Material animation (`OPEN_SKYRIM_material_animation`)

Skyrim animates hearth flames, lava, steam and glow cards by driving one shader variable from a
keyframe controller: a float controller on the shader property's `NiObjectNET` controller reference,
chained to further controllers through `next_controller`. Nothing in glTF animates a material
variable, so the channels are published on the shape's material as their own extension, beside
`OPEN_SKYRIM_material` and `MUDCRAB_material` (§4):

```json
"extensions": {
  "OPEN_SKYRIM_material_animation": {
    "channels": [{
      "variable": "vOffset",
      "interpolation": "LINEAR" | "QUADRATIC" | "STEP",
      "times": [0.0, 5.6667],
      "values": [0.0, 1.0],
      "tangents": [[1.0, 0.0], [0.0, 1.0]],
      "loop": "cycle" | "reverse" | "clamp",
      "frequency": 1.0, "phase": 0.0, "start": 0.0, "stop": 5.6667
    }]
  }
}
```

* One channel per float controller in the chain, in chain order.
* `variable` is the controlled shader variable in the camelCase spelling of the nif.xml enums.
  Effect shaders (`EffectShaderControlledVariable`): `emissiveMultiple`, `falloffStartAngle`,
  `falloffStopAngle`, `falloffStartOpacity`, `falloffStopOpacity`, `alpha`, `uOffset`, `uScale`,
  `vOffset`, `vScale`. Lighting shaders (`LightingShaderControlledVariable`) share those names for
  the variables the enums share and add `refractionStrength`, `environmentMapScale`, `glossiness`
  and `specularStrength`.
* `interpolation` is the key type: `LINEAR` (1) and `STEP` (5) keys store `(time, value)`;
  `QUADRATIC` (2) keys also store a forward and a backward tangent, published as `tangents`.
* A `uOffset`/`vOffset`/`uScale`/`vScale` channel replaces the matching component of
  `MUDCRAB_material`'s `uvOffset`/`uvScale` (§4) at playback; the other, unanimated component
  keeps its static value from there. Every other channel variable (`alpha`, `emissiveMultiple`,
  `glossiness`, ...) replaces its own static field the same way, wherever this document or the
  material contract publishes it.
* Units and timing, from the controller's `NiTimeController` fields:
  - `times`, `start` and `stop` are in seconds, as Skyrim's animation clock ticks them.
  - `phase` is also in seconds; it is added to the point in the cycle after scaling by
    `frequency`, i.e. the evaluated time is `now * frequency + phase`, wrapped into `[start, stop]`
    by `loop`.
  - `loop` is the cycle mode in flags bits 1-2 (0 cycle, 1 reverse, 2 clamp): `cycle` restarts at
    `start` once playback passes `stop`; `reverse` means ping-pong - playback runs forward to
    `stop`, then backward to `start`, and repeats; `clamp` holds the value at `stop` (or `start`,
    running backward) once playback reaches it, instead of restarting.
* `tangents` (`QUADRATIC` only) is one `[outgoing, incoming]` pair per key, in value units per
  key interval (the segment parameter `x` runs 0 to 1). The segment from key i to key i+1 is the
  Hermite curve `v = v_i (2x^3 - 3x^2 + 1) + v_{i+1} (-2x^3 + 3x^2) + t1 (x^3 - 2x^2 + x) +
  t2 (x^3 - x^2)` with `t1 = tangents[i][0]` and `t2 = tangents[i+1][1]`. The NIF names the two
  fields the other way round: `outgoing` is the key's `Backward` field and `incoming` its `Forward`
  field, as NifSkope's evaluator reads them (`src/gl/glcontroller.cpp`). The hearth flames publish
  `[[1, 0], [0, 1]]`: a steady scroll.
* Colour controllers (`BSEffectShaderPropertyColorController`, `BSLightingShaderPropertyColorController`,
  keyed by an `NiPoint3Interpolator` → `NiPosData`) publish `emissiveColor` (effect colour 0,
  lighting colour 1) or `specularColor` (lighting colour 0) with `"components": 3`. `values` then
  holds three floats per key, key-major (`[r0, g0, b0, r1, ...]`), in the space the static
  colour is published in; `tangents` holds one `[outgoing, incoming]` pair per component, in the
  same order. Each component evaluates like a float channel. `components` is omitted when it is 1.
* The extension is listed in `extensionsUsed`, never `extensionsRequired`: a consumer that does not
  play it renders the shape's still frame.
* A controller with no interpolator, no float data, an unknown variable, an unsupported key type,
  an unknown cycle mode or non-finite key data is dropped and counted in the mesh stage's
  `NifParseDiagnostics::animation_skipped_channels` (reason → count). Animation never fails a
  conversion.

---

## 5. Distant object LOD segments (`.bto`)

Skyrim ships distant object LOD (`meshes/terrain/**/objects/*.bto`) as NIFs in a different
container from ordinary models: a `BSMultiBoundNode` (bounded by a `BSMultiBound` ->
`BSMultiBoundAABB` pair) holding one or more shapes. At LOD level 4, every shape is a
`BSSubIndexTriShape`: the ordinary `BSTriShape` triangle payload, followed by a table of up to 16
segments, one per cell of the block's 4x4 grid. Segment index `i` = `4*dx + dy`, where `dx`, `dy`
(0..3) are the owner cell's offset from the block's south-west cell. Each table entry is `u8 flag,
u32 (unused), u32 primitive count`; a segment's start is the sum of every earlier entry's count
(a triangle offset, not a byte offset), and a table shorter than 16 means the trailing cells are
empty. Level 8 and level 16 blocks have a single segment and are never split. See
`local/research/lod-hiding-under-loaded-cells.md` for how this was measured from the shipped game
data, and `crates/converter/src/mesh/lod_segments.rs` for the implementation.

When a `.bto` shape's segment table has **2 or more non-empty entries**, the converter exports one
glTF **primitive per non-empty segment** instead of the usual one primitive per shape:

- Every split primitive keeps the shape's mesh: it shares the shape's `POSITION`/`NORMAL`/
  `TEXCOORD_0`/`COLOR_0` accessors and its material, and only its `indices` accessor differs, set to
  that segment's contiguous triangle range (`indices` accessor count = 3x the segment's triangle
  count). Splitting only adds accessors and buffer views over the shape's existing index buffer; no
  vertex or index bytes are duplicated or moved, so the mesh's recorded bounds (read from the shared
  `POSITION` accessor) are identical to the unsplit shape's.
- Each split primitive carries glTF primitive extras:

  ```json
  { "extras": { "openSkyrim": { "lodSegment": 5 } } }
  ```

  where the value is the segment's index in the table (0..15), i.e. `4*dx + dy`. This is how the
  engine half of distant LOD (a separate, later task) knows which cell each primitive belongs to,
  so it can hide the part of a block that belongs to a cell whose full models have loaded.
- Shapes named `*-LargeRef` (large references, e.g. `obj-LargeRef`) carry their own, separate set of
  segments; the converter does not special-case the name and treats them like any other segmented
  shape.
- A shape with 0 or 1 non-empty segments (most `.bto` shapes: LODGen already collapses a block with
  nothing else nearby to one segment covering the whole mesh), and every non-`.bto` model, is
  exported exactly as before: one primitive, with no `lodSegment` extra.

`crates/converter/src/material.rs`'s material publication assigns the shape's material (or, for an
excluded shape, the non-rendering material and its `shapeBlock`/`materialExclusion` extras) to
*every* primitive of a mesh, not just the first; for a split shape it merges those fields into each
primitive's existing `lodSegment` extra rather than overwriting it.

---

## 6. Rust Implementation (`mesh_tools` Builder Architecture)

We use the **`mesh_tools`** crate (`GltfBuilder`), which provides an incredibly clean, ergonomic API for assembling vertices, normals, UVs, and PBR materials into binary `.glb` files.

```rust
use mesh_tools::GltfBuilder;

pub struct NifToGltfConverter;

impl NifToGltfConverter {
    /// Converts a parsed Skyrim NIF structure into a binary GLB file
    pub fn convert_and_export(nif: &SkyrimNif, output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut builder = GltfBuilder::new();

        // 1. Create PBR Material
        let material = builder.add_pbr_material(
            Some("SkyrimMaterial".to_string()),
            Some([1.0, 1.0, 1.0, 1.0]), // Base Color (RGBA)
            Some(nif.material.roughness),
            Some(nif.material.metallic),
        );

        // 2. Add Mesh Primitives (Positions, Normals, UVs, Indices)
        let mesh_index = builder.add_custom_mesh(
            Some("SkyrimMesh".to_string()),
            &nif.positions, // Vec<[f32; 3]>
            &nif.normals,   // Vec<[f32; 3]>
            &nif.uvs,       // Vec<[f32; 2]>
            &nif.indices,   // Vec<u32>
            Some(material),
        );

        // 3. Create Scene Node with Transform
        let node_index = builder.add_node(
            Some("RootNode".to_string()),
            Some(mesh_index),
            Some(nif.translation), // [x, y, z]
            Some(nif.rotation),    // Quaternion [x, y, z, w]
            Some(nif.scale),       // [sx, sy, sz]
        );

        builder.add_scene(
            Some("SkyrimScene".to_string()),
            Some(vec![node_index]),
        );

        // 4. Export binary GLB directly to disk
        builder.export_glb(output_path)?;

        Ok(())
    }
}
```

## 6. Collision extras and rigid-body dynamics

Scene extras carry `mudcrabCollision` (`shared::collision::CollisionAsset`): the authored
collision `shapes`, the `skipped` blocks, and, since version 2, a `bodies` array (#104 phase a).
A version 1 asset has no `bodies`; readers treat every shape in it as fixed and ignore body
fields they do not know.

Each `bhkCollisionObject` whose rigid body (`bhkRigidBody` or `bhkRigidBodyT`) yields shapes
produces one `CollisionBody`:

| Field | Source (byte offset in the Skyrim SE body block) | Conversion |
| --- | --- | --- |
| `node`, `target` | the collision object's target `NiNode` | glTF node index and NIF node name |
| `shapes` | the shapes extracted for this body | indices into `shapes` |
| `havok.collision_layer` | havok filter layer (4) | raw |
| `havok.motion_system`, `deactivator_type`, `quality_type` | 224, 225, 227 | raw (`hkMotionType`, `hkDeactivatorType`, `hkQualityType`) |
| `mass` | 180 | unchanged (kg) |
| `inertia` | `hkMatrix3`, three rows of four floats (116) | R I R^T for the shapes' transform, x 70^2, then the Creation-to-runtime basis |
| `center_of_mass` | `Vector4` (164) | x 70, same transform as the shapes, then the runtime basis |
| `linear_damping`, `angular_damping` | 184, 188 | unchanged |
| `friction`, `restitution` | 200, 208 | unchanged |
| `max_linear_velocity` | 212 | x 70 |
| `max_angular_velocity` | 216 | unchanged (rad/s) |

Shapes are read from the body's shape block, through `bhkMoppBvTreeShape`,
`bhkTransformShape`/`bhkConvexTransformShape` and `bhkListShape` wrappers, and map onto
`shared::collision::CollisionShape`:

| NIF block | Shape | Conversion |
| --- | --- | --- |
| `bhkCompressedMeshShape` | `Mesh` | decompressed vertices and triangles |
| `bhkNiTriStripsShape` | `Mesh` | strip data as triangles |
| `bhkBoxShape` | `Hull` | the eight transformed half-extent corners (16) |
| `bhkConvexVerticesShape` | `Hull` | vertices from 36 |
| `bhkCylinderShape` | `Hull` | two 16-point rings around the A-B axis (A @16, B @32), radius @48 |
| `bhkCapsuleShape` | `Capsule` | A @16 and B @32, radius max(Radius 1 @28, Radius 2 @44) |
| `bhkSphereShape` | `Capsule` | zero-length capsule (a = b = the shape origin), radius @4 |
| `bhkMultiSphereShape` | `Capsule` per sphere | count @16 (1..=8), `NiBound {centre, radius}` from @20 |

Points are Havok units scaled by 70 before the Creation-to-runtime basis; radii are scaled by
70 and the transform scale, like the capsule arm. A sphere or multi-sphere under a non-uniform
or sheared transform has no single radius and is skipped. A degenerate cylinder (A = B, radius
below or equal to zero, non-finite), an out-of-range multi-sphere count and any malformed or
unsupported shape land in `skipped` instead of becoming collision.

Only bodies on physical Skyrim layers are read (`SkyrimLayer` in nif.xml): 0 UNIDENTIFIED,
1 STATIC, 2 ANIMSTATIC, 3 TRANSPARENT, 4 CLUTTER, 5 WEAPON, 9 TREES, 10 PROPS, 13 TERRAIN,
17 GROUND, 26 TRANSPARENT_SMALL, 27 INVISIBLE_WALL, 28 TRANSPARENT_SMALL_ANIM, 31 STAIRHELPER
and 35 COLLISIONBOX. Layer 15 NONCOLLIDABLE (harvestable flora and other nonphysical objects)
is dropped silently; layer 12 TRIGGER is skipped with the reason "trigger volume (layer 12) is
not physical"; every other layer is reported as unsupported.

`kind` is `dynamic` when the motion system is a simulated one (dynamic, sphere or box inertia,
plain or stabilized, thin box: 1, 2, 3, 4, 5, 8), the quality type is a moving one (debris,
moving, critical, bullet: 3, 4, 5, 6) and the mass is finite and above zero; `keyframed` for
`MO_SYS_KEYFRAMED` (6) with mass 0; otherwise `fixed`. `convex` is true when every shape of the
body is a box, capsule or hull (a compressed or strip mesh makes it false).

The `node` index and `target` name are checked against the GLB that is actually written, not the
source NIF's static node list (skeletal and effect NIFs lay their nodes out differently): a body
is kept only when `target` is non-empty and the GLB's node at `node` has that name. A name
used by more than one node is trusted at its index only when the GLB's nodes start in the
same order as the NIF's static scene, which the index was predicted from; otherwise, and
always when the collision is added to a GLB built elsewhere (`annotate`), the name must be
unique.

A body on an unsupported layer, a trigger, a non-colliding body (a `HavokFilter` with the
"No Collision" flag, or a collision response of 2 RESPONSE_REPORTING or 3 RESPONSE_NONE, in
either of the two copies a body stores) or a body whose target node cannot be resolved is listed
in `skipped` and contributes no collision. A body whose shapes cannot all be read is listed in
`skipped`; the shapes it did read stay as fixed collision. A body whose dynamics cannot be read,
or whose glTF node fails the check above, is listed in `skipped` but its shapes stay as fixed
collision.

A cylinder's 16-point rings are inscribed in the true circle (about 2% inside at the chord
midpoints); like the box and hull arms, the Havok convex radius (a thin shell) is not added.

Collision coverage (sphere, multi-sphere and cylinder shapes) moved the converter to schema 27:
schema 24 and 26 outputs keep their verified scripts and archive ingestion and regenerate GLBs, textures
and world data, so no cached GLB keeps the older collision extras.
