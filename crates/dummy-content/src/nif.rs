//! Deterministic NIF static-shape fixtures for Skyrim SE (`20.2.0.7`).
//!
//! The writer emits the minimal block set the converter renders:
//! `BSFadeNode` → `BSTriShape` → `BSLightingShaderProperty` → `BSShaderTextureSet`.
//! Geometry is validated before serialization and the output is byte-stable
//! for identical input.

use crate::bytes::{push_u16, push_u32, push_u64};
use color_eyre::{
    Result,
    eyre::{ensure, eyre},
};

const NIF_VERSION: u32 = 0x1402_0007;
const USER_VERSION: u32 = 12;
const BETHESDA_VERSION: u32 = 100;
const NULL_REF: u32 = u32::MAX;
const SHADER_TYPE_DEFAULT: u32 = 0;
const VERTEX_FLAGS: u16 = 0x0001 | 0x0002 | 0x0008;
const VERTEX_STRIDE: u8 = 6;
const VERTEX_COLOR_FLAG: u16 = 0x0020;
const UV_OFFSET: u64 = 4;
const NORMAL_OFFSET: u64 = 5;
const COLOR_OFFSET: u64 = 6;
const SHAPE_BLOCK_HEADER: usize = 116;
const STATIC_SHADER_BLOCK: u32 = 2;

/// `KeyType::QuadraticKey`: keys carry a forward and a backward tangent.
const KEY_TYPE_QUADRATIC: u32 = 2;

/// A triangle mesh rendered as a single static shape.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticShape<'a> {
    /// Shape name stored in the NIF string table.
    pub name: &'a str,
    /// Vertex positions.
    pub positions: &'a [[f32; 3]],
    /// Per-vertex normals; must match `positions`.
    pub normals: &'a [[f32; 3]],
    /// Per-vertex UVs; must match `positions`.
    pub uvs: &'a [[f32; 2]],
    /// Triangle indices into `positions`.
    pub indices: &'a [[u16; 3]],
    /// Diffuse texture path, for example `textures/generated_color.dds`.
    pub diffuse: &'a str,
    /// Normal texture path.
    pub normal_texture: &'a str,
}

/// A Havok box rigid body attached to its own `NiNode` under the root, written as a
/// `bhkCollisionObject` -> `bhkRigidBody` (or `bhkRigidBodyT`) -> `bhkBoxShape` chain.
/// Every value is stored raw, in Havok units (a Creation unit is 1/70 of one).
#[derive(Debug, Clone, PartialEq)]
pub struct BoxBody<'a> {
    /// Name of the `NiNode` the collision object targets.
    pub node_name: &'a str,
    /// Box half extents (`bhkBoxShape` dimensions).
    pub half_extents: [f32; 3],
    /// `Some((translation, rotation xyzw))` writes a `bhkRigidBodyT`, else a `bhkRigidBody`.
    pub transform: Option<([f32; 3], [f32; 4])>,
    pub collision_layer: u8,
    /// `CollisionFilterFlags` and `hkResponseType` of the `bhkWorldObject`/`bhkEntityCInfo` copy.
    pub collision_flags: u8,
    pub collision_response: u8,
    /// The same two values in the `bhkRigidBodyCInfo2010` copy.
    pub inner_collision_flags: u8,
    pub inner_collision_response: u8,
    /// The body's shape: a box with `half_extents`, or a sphere or cylinder.
    pub shape: BodyShape,
    /// Raw `hkMotionType`, `hkDeactivatorType` and `hkQualityType` values (nif.xml).
    pub motion_system: u8,
    pub deactivator_type: u8,
    pub quality_type: u8,
    pub mass: f32,
    /// Row-major inertia tensor.
    pub inertia: [f32; 9],
    pub center_of_mass: [f32; 3],
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
    pub max_linear_velocity: f32,
    pub max_angular_velocity: f32,
}

/// Shape block written for a [`BoxBody`] (Havok units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BodyShape {
    /// `bhkBoxShape` with the body's `half_extents`.
    Box,
    /// `bhkSphereShape`.
    Sphere { radius: f32 },
    /// `bhkMultiSphereShape`: the first `count` of `spheres` (centre, radius).
    MultiSphere {
        count: u32,
        spheres: [([f32; 3], f32); 8],
    },
    /// `bhkCylinderShape` from `a` to `b`.
    Cylinder {
        a: [f32; 3],
        b: [f32; 3],
        radius: f32,
    },
}

/// Generates a minimal static NIF containing one triangle mesh.
pub fn static_shape(shape: &StaticShape<'_>) -> Result<Vec<u8>> {
    static_shape_with_colors(shape, &[])
}

/// Generates a static NIF with authored RGBA vertex data and both color flags enabled.
pub fn static_shape_with_colors(shape: &StaticShape<'_>, colors: &[[u8; 4]]) -> Result<Vec<u8>> {
    build_static_shape(shape, colors, &[])
}

/// Like [`static_shape`], plus one collision body per entry. Each body hangs off its own
/// `NiNode` child of the root, so the converter's glTF node order is root, shape, then the
/// body nodes in order.
pub fn static_shape_with_bodies(
    shape: &StaticShape<'_>,
    bodies: &[BoxBody<'_>],
) -> Result<Vec<u8>> {
    build_static_shape(shape, &[], bodies)
}

fn build_static_shape(
    shape: &StaticShape<'_>,
    colors: &[[u8; 4]],
    bodies: &[BoxBody<'_>],
) -> Result<Vec<u8>> {
    validate(shape)?;
    ensure!(
        colors.is_empty() || colors.len() == shape.positions.len(),
        "vertex color count mismatch"
    );
    for body in bodies {
        validate_body(body)?;
    }
    let mut strings = vec![shape.name];
    strings.extend(bodies.iter().map(|body| body.node_name));
    let first_body_block = 4_u32;
    let mut children = vec![1_u32];
    children.extend((0..bodies.len()).map(|i| first_body_block + 4 * i as u32));
    let mut blocks = vec![
        fade_node_with_children(&children),
        triangle_shape(shape, colors)?,
        lighting_shader_property(!colors.is_empty()),
        texture_set(shape)?,
    ];
    let mut block_types = vec![
        "BSFadeNode",
        "BSTriShape",
        "BSLightingShaderProperty",
        "BSShaderTextureSet",
    ];
    for (index, body) in bodies.iter().enumerate() {
        let node_block = first_body_block + 4 * index as u32;
        let mut node = Vec::new();
        push_av_object_with(&mut node, 1 + index as u32, node_block + 1);
        push_u32(&mut node, 0); // children
        push_u32(&mut node, 0); // effects
        blocks.push(node);
        block_types.push("NiNode");
        let mut object = Vec::new();
        push_u32(&mut object, node_block); // target
        push_u16(&mut object, 1); // flags
        push_u32(&mut object, node_block + 2); // body
        blocks.push(object);
        block_types.push("bhkCollisionObject");
        blocks.push(rigid_body(body, node_block + 3));
        block_types.push(if body.transform.is_some() {
            "bhkRigidBodyT"
        } else {
            "bhkRigidBody"
        });
        let (shape, kind) = body_shape(body);
        blocks.push(shape);
        block_types.push(kind);
    }

    write_nif(&block_types, &blocks, &strings)
}

/// One shader float controller of a fixture effect shape.
///
/// One entry becomes a `NiTimeController` → `NiFloatInterpolator` →
/// `NiFloatData` triple, chained through `next_controller` in list order.
#[derive(Debug, Clone, PartialEq)]
pub struct FloatController<'a> {
    /// `NiTimeController` flags; bits 1-2 are the cycle mode.
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start_time: f32,
    pub stop_time: f32,
    /// Controlled variable index, as `EffectShaderControlledVariable` numbers it.
    pub variable: u32,
    /// Key type as `KeyType` numbers it: `1` linear, `2` quadratic, `5` constant.
    pub key_type: u32,
    /// `(time, value, forward tangent, backward tangent)` per key. The tangents
    /// are written only for the quadratic key type.
    pub keys: &'a [[f32; 4]],
}

/// Generates a minimal effect-shader NIF whose shader property is animated.
///
/// The block set is `BSFadeNode` → `BSTriShape` → `BSEffectShaderProperty`,
/// followed by one interpolator, one float data and one controller block per
/// entry in `controllers`, in that order. The shader property's controller
/// reference points at the first controller; each controller's `target` points
/// back at the shader property and its `next_controller` at the following one.
pub fn effect_shape_with_controllers(
    shape: &StaticShape<'_>,
    controllers: &[FloatController<'_>],
) -> Result<Vec<u8>> {
    validate(shape)?;
    let mut blocks = vec![
        fade_node(),
        triangle_shape_with_shader(shape, 2)?,
        effect_shader_property(shape, controller_block_index(0))?,
    ];
    for (index, controller) in controllers.iter().enumerate() {
        ensure!(
            !controller.keys.is_empty(),
            "NIF float controller {index} has no keys"
        );
        let interpolator = interpolator_block_index(index);
        let next_controller = if index + 1 < controllers.len() {
            controller_block_index(index + 1)
        } else {
            NULL_REF
        };
        blocks.push(float_interpolator(interpolator + 1));
        blocks.push(float_data(controller)?);
        blocks.push(float_controller(controller, interpolator, next_controller));
    }
    let mut block_types = vec![
        "BSFadeNode".to_owned(),
        "BSTriShape".to_owned(),
        "BSEffectShaderProperty".to_owned(),
    ];
    for _ in controllers {
        block_types.extend(
            [
                "NiFloatInterpolator",
                "NiFloatData",
                "BSEffectShaderPropertyFloatController",
            ]
            .map(str::to_owned),
        );
    }
    let block_types = block_types.iter().map(String::as_str).collect::<Vec<_>>();
    write_nif(&block_types, &blocks, &[shape.name])
}

/// One `BSEffectShaderPropertyColorController` on an effect shape's emissive colour, with its
/// `NiPoint3Interpolator` and `NiPosData`.
pub struct ColorController<'a> {
    /// `NiTimeController` flags; bits 1-2 are the cycle mode.
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start_time: f32,
    pub stop_time: f32,
    /// Key type as `KeyType` numbers it: `1` linear, `2` quadratic, `5` constant.
    pub key_type: u32,
    /// `(time, r, g, b, forward rgb, backward rgb)` per key. The tangents are written only for
    /// the quadratic key type.
    pub keys: &'a [[f32; 10]],
}

/// An effect shape whose controller chain starts with an emissive-colour controller and
/// continues with `floats`, in that order.
pub fn effect_shape_with_color_controller(
    shape: &StaticShape<'_>,
    color: &ColorController<'_>,
    floats: &[FloatController<'_>],
) -> Result<Vec<u8>> {
    validate(shape)?;
    ensure!(!color.keys.is_empty(), "NIF colour controller has no keys");
    let mut blocks = vec![
        fade_node(),
        triangle_shape_with_shader(shape, 2)?,
        effect_shader_property(shape, controller_block_index(0))?,
    ];
    let next = if floats.is_empty() {
        NULL_REF
    } else {
        controller_block_index(1)
    };
    blocks.push(point3_interpolator(interpolator_block_index(0) + 1));
    blocks.push(pos_data(color)?);
    blocks.push(color_controller(color, interpolator_block_index(0), next));
    for (index, controller) in floats.iter().enumerate() {
        let index = index + 1;
        ensure!(
            !controller.keys.is_empty(),
            "NIF float controller {index} has no keys"
        );
        let interpolator = interpolator_block_index(index);
        let next_controller = if index < floats.len() {
            controller_block_index(index + 1)
        } else {
            NULL_REF
        };
        blocks.push(float_interpolator(interpolator + 1));
        blocks.push(float_data(controller)?);
        blocks.push(float_controller(controller, interpolator, next_controller));
    }
    let mut block_types = [
        "BSFadeNode",
        "BSTriShape",
        "BSEffectShaderProperty",
        "NiPoint3Interpolator",
        "NiPosData",
        "BSEffectShaderPropertyColorController",
    ]
    .map(str::to_owned)
    .to_vec();
    for _ in floats {
        block_types.extend(
            [
                "NiFloatInterpolator",
                "NiFloatData",
                "BSEffectShaderPropertyFloatController",
            ]
            .map(str::to_owned),
        );
    }
    let block_types = block_types.iter().map(String::as_str).collect::<Vec<_>>();
    write_nif(&block_types, &blocks, &[shape.name])
}

/// Block indices of one controller's triple: the interpolator at block 3, its
/// float data at 4 and the controller itself at 5, then three blocks per
/// further controller.
fn interpolator_block_index(index: usize) -> u32 {
    u32::try_from(3 + index * 3).unwrap_or(NULL_REF)
}

fn controller_block_index(index: usize) -> u32 {
    interpolator_block_index(index).wrapping_add(2)
}

/// Writes an effect shape's `BSTriShape` with an explicit shader-property
/// reference, on top of the shared [`write_shape_block`] writer rather than
/// duplicating it.
fn triangle_shape_with_shader(shape: &StaticShape<'_>, shader_property: u32) -> Result<Vec<u8>> {
    write_shape_block(&geometry(shape, &[]), shader_property)
}

struct Geometry<'a> {
    positions: &'a [[f32; 3]],
    normals: &'a [[f32; 3]],
    uvs: &'a [[f32; 2]],
    indices: &'a [[u16; 3]],
    /// Per-vertex RGBA colours, written only when present.
    colors: Option<&'a [[u8; 4]]>,
}

fn geometry<'a>(shape: &StaticShape<'a>, colors: &'a [[u8; 4]]) -> Geometry<'a> {
    Geometry {
        positions: shape.positions,
        normals: shape.normals,
        uvs: shape.uvs,
        indices: shape.indices,
        // An empty slice means the caller authored no vertex colours.
        colors: (!colors.is_empty()).then_some(colors),
    }
}

fn write_shape_block(geometry: &Geometry<'_>, shader_property: u32) -> Result<Vec<u8>> {
    let vertex_stride = VERTEX_STRIDE + u8::from(geometry.colors.is_some());
    let vertex_bytes = geometry
        .positions
        .len()
        .checked_mul(usize::from(vertex_stride) * 4)
        .ok_or_else(|| eyre!("NIF vertex data overflow"))?;
    let triangle_bytes = geometry
        .indices
        .len()
        .checked_mul(6)
        .ok_or_else(|| eyre!("NIF triangle data overflow"))?;
    let data_size = vertex_bytes
        .checked_add(triangle_bytes)
        .ok_or_else(|| eyre!("NIF geometry size overflow"))?;

    let (center, radius) = bounds(geometry.positions);
    let mut block = Vec::with_capacity(SHAPE_BLOCK_HEADER + data_size);
    push_av_object(&mut block, 0);
    for value in center {
        block.extend_from_slice(&value.to_le_bytes());
    }
    block.extend_from_slice(&radius.to_le_bytes());
    push_u32(&mut block, NULL_REF);
    push_u32(&mut block, shader_property);
    push_u32(&mut block, NULL_REF);
    let color_offset = if geometry.colors.is_some() {
        COLOR_OFFSET
    } else {
        0
    };
    let vertex_flags = VERTEX_FLAGS
        | if geometry.colors.is_some() {
            VERTEX_COLOR_FLAG
        } else {
            0
        };
    let descriptor = u64::from(vertex_stride)
        | (UV_OFFSET << 8)
        | (NORMAL_OFFSET << 16)
        | (color_offset << 24)
        | (u64::from(vertex_flags) << 44);
    push_u64(&mut block, descriptor);
    push_u16(
        &mut block,
        u16::try_from(geometry.indices.len()).map_err(|_| eyre!("NIF triangle count overflow"))?,
    );
    push_u16(
        &mut block,
        u16::try_from(geometry.positions.len()).map_err(|_| eyre!("NIF vertex count overflow"))?,
    );
    push_u32(
        &mut block,
        u32::try_from(data_size).map_err(|_| eyre!("NIF geometry size overflow"))?,
    );
    let empty_colors = [[0u8; 4]; 0];
    let colors = geometry.colors.unwrap_or(&empty_colors);
    for (index, (position, (normal, uv))) in geometry
        .positions
        .iter()
        .zip(geometry.normals.iter().zip(geometry.uvs.iter()))
        .enumerate()
    {
        for value in position {
            block.extend_from_slice(&value.to_le_bytes());
        }
        block.extend_from_slice(&0.0f32.to_le_bytes());
        block.extend_from_slice(&encode_half(uv[0]).to_le_bytes());
        block.extend_from_slice(&encode_half(uv[1]).to_le_bytes());
        block.push(pack_normal(normal[0]));
        block.push(pack_normal(normal[1]));
        block.push(pack_normal(normal[2]));
        block.push(0);
        if let Some(color) = colors.get(index) {
            block.extend_from_slice(color);
        }
    }
    for triangle in geometry.indices {
        for index in triangle {
            block.extend_from_slice(&index.to_le_bytes());
        }
    }
    Ok(block)
}

fn write_nif(block_types: &[&str], blocks: &[Vec<u8>], strings: &[&str]) -> Result<Vec<u8>> {
    ensure!(
        block_types.len() == blocks.len(),
        "NIF has {} block types for {} blocks",
        block_types.len(),
        blocks.len()
    );
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    push_u32(&mut bytes, NIF_VERSION);
    bytes.push(1);
    push_u32(&mut bytes, USER_VERSION);
    push_u32(
        &mut bytes,
        u32::try_from(blocks.len()).map_err(|_| eyre!("NIF block count overflow"))?,
    );
    push_u32(&mut bytes, BETHESDA_VERSION);
    push_string8(&mut bytes, "Mudcrab dummy-content");
    push_string8(&mut bytes, "");
    push_string8(&mut bytes, "");
    push_u16(
        &mut bytes,
        u16::try_from(block_types.len()).map_err(|_| eyre!("NIF block type overflow"))?,
    );
    for block_type in block_types {
        push_u32(
            &mut bytes,
            u32::try_from(block_type.len()).map_err(|_| eyre!("NIF block type overflow"))?,
        );
        bytes.extend_from_slice(block_type.as_bytes());
    }
    for index in 0..blocks.len() {
        push_u16(
            &mut bytes,
            u16::try_from(index).map_err(|_| eyre!("NIF block index overflow"))?,
        );
    }
    for block in blocks {
        push_u32(
            &mut bytes,
            u32::try_from(block.len()).map_err(|_| eyre!("NIF block size overflow"))?,
        );
    }
    push_u32(
        &mut bytes,
        u32::try_from(strings.len()).map_err(|_| eyre!("NIF string count overflow"))?,
    );
    push_u32(&mut bytes, max_string_length(strings));
    for value in strings {
        push_u32(
            &mut bytes,
            u32::try_from(value.len()).map_err(|_| eyre!("NIF string overflow"))?,
        );
        bytes.extend_from_slice(value.as_bytes());
    }
    push_u32(&mut bytes, 0);
    for block in blocks {
        bytes.extend_from_slice(block);
    }
    Ok(bytes)
}

fn validate(shape: &StaticShape<'_>) -> Result<()> {
    ensure!(!shape.name.is_empty(), "NIF shape name is empty");
    ensure!(
        shape.name.bytes().all(|byte| (0x20..0x7f).contains(&byte)),
        "NIF shape name is not printable ASCII: {:?}",
        shape.name
    );
    ensure!(!shape.positions.is_empty(), "NIF shape has no positions");
    ensure!(
        shape.positions.len() == shape.normals.len(),
        "NIF shape has {} positions and {} normals",
        shape.positions.len(),
        shape.normals.len()
    );
    ensure!(
        shape.positions.len() == shape.uvs.len(),
        "NIF shape has {} positions and {} UVs",
        shape.positions.len(),
        shape.uvs.len()
    );
    ensure!(
        shape.positions.len() <= u16::MAX as usize,
        "NIF shape exceeds 65535 vertices"
    );
    ensure!(
        shape.indices.len() <= u16::MAX as usize,
        "NIF shape exceeds 65535 triangles"
    );
    ensure!(!shape.indices.is_empty(), "NIF shape has no triangles");
    ensure!(
        shape
            .positions
            .iter()
            .all(|position| position.iter().all(|value| value.is_finite())),
        "NIF shape contains a non-finite position"
    );
    ensure!(
        shape
            .uvs
            .iter()
            .all(|uv| uv.iter().all(|value| value.is_finite())),
        "NIF shape contains a non-finite UV"
    );
    let vertex_count = shape.positions.len();
    ensure!(
        shape
            .indices
            .iter()
            .flatten()
            .all(|index| { usize::from(*index) < vertex_count }),
        "NIF shape contains an out-of-range triangle index"
    );
    ensure!(
        !shape.diffuse.is_empty(),
        "NIF shape needs a diffuse texture"
    );
    for texture in [shape.diffuse, shape.normal_texture] {
        ensure!(
            texture.bytes().all(|byte| (0x20..0x7f).contains(&byte)),
            "NIF texture path is not printable ASCII: {texture:?}"
        );
    }
    Ok(())
}

fn fade_node() -> Vec<u8> {
    fade_node_with_children(&[1])
}

fn fade_node_with_children(children: &[u32]) -> Vec<u8> {
    let mut block = Vec::with_capacity(84);
    push_av_object(&mut block, NULL_REF);
    push_u32(&mut block, children.len() as u32);
    for child in children {
        push_u32(&mut block, *child);
    }
    push_u32(&mut block, 0);
    block
}

fn validate_body(body: &BoxBody<'_>) -> Result<()> {
    ensure!(
        !body.node_name.is_empty()
            && body
                .node_name
                .bytes()
                .all(|byte| (0x20..0x7f).contains(&byte)),
        "NIF body node name must be printable ASCII"
    );
    ensure!(
        body.shape != BodyShape::Box || body.half_extents.iter().all(|v| v.is_finite() && *v > 0.0),
        "NIF box body needs positive half extents"
    );
    let finite = body
        .inertia
        .iter()
        .chain(&body.center_of_mass)
        .all(|v| v.is_finite())
        && [
            body.mass,
            body.linear_damping,
            body.angular_damping,
            body.friction,
            body.restitution,
            body.max_linear_velocity,
            body.max_angular_velocity,
        ]
        .iter()
        .all(|v| v.is_finite());
    ensure!(finite, "NIF body contains a non-finite value");
    Ok(())
}

/// `bhkRigidBody` block: `bhkWorldObject`, `bhkEntityCInfo`, then `bhkRigidBodyCInfo2010`
/// (nif.xml, Skyrim), then constraints and body flags.
fn rigid_body(body: &BoxBody<'_>, shape: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(250);
    push_u32(&mut b, shape);
    b.extend_from_slice(&[body.collision_layer, body.collision_flags, 0, 0]); // havok filter
    b.extend_from_slice(&[0; 20]); // world object info
    b.extend_from_slice(&[body.collision_response, 0, 0xff, 0xff]); // entity info
    b.extend_from_slice(&[0; 4]); // unused
    b.extend_from_slice(&[body.collision_layer, body.inner_collision_flags, 0, 0]); // havok filter
    b.extend_from_slice(&[0; 4]); // unused
    push_u32(&mut b, 0); // unknown int
    b.extend_from_slice(&[body.inner_collision_response, 0, 0xff, 0xff]); // response, unused, callback delay
    let (translation, rotation) = body.transform.unwrap_or(([0.0; 3], [0.0, 0.0, 0.0, 1.0]));
    for value in translation.into_iter().chain([0.0]) {
        push_f32(&mut b, value);
    }
    for value in rotation {
        push_f32(&mut b, value);
    }
    b.extend_from_slice(&[0; 32]); // linear and angular velocity
    for row in body.inertia.chunks(3) {
        for value in row.iter().copied().chain([0.0]) {
            push_f32(&mut b, value);
        }
    }
    for value in body.center_of_mass.into_iter().chain([0.0]) {
        push_f32(&mut b, value);
    }
    for value in [
        body.mass,
        body.linear_damping,
        body.angular_damping,
        1.0, // time factor
        1.0, // gravity factor
        body.friction,
        0.0, // rolling friction multiplier
        body.restitution,
        body.max_linear_velocity,
        body.max_angular_velocity,
        0.15, // penetration depth
    ] {
        push_f32(&mut b, value);
    }
    b.extend_from_slice(&[
        body.motion_system,
        body.deactivator_type,
        1, // solver deactivation off
        body.quality_type,
        0, // auto remove level
        0, // response modifier flags
        3, // shape keys in contact point
        0, // force collided onto PPU
    ]);
    b.extend_from_slice(&[0; 12]);
    push_u32(&mut b, 0); // constraints
    push_u16(&mut b, 0); // body flags
    b
}

/// The body's shape block and its block type (nif.xml: material, convex radius, then fields).
fn body_shape(body: &BoxBody<'_>) -> (Vec<u8>, &'static str) {
    let mut b = Vec::with_capacity(64);
    push_u32(&mut b, 0); // material
    match body.shape {
        BodyShape::Box => {
            push_f32(&mut b, 0.0); // radius
            b.extend_from_slice(&[0; 8]);
            for value in body.half_extents {
                push_f32(&mut b, value);
            }
            push_f32(&mut b, 0.0);
            (b, "bhkBoxShape")
        }
        BodyShape::Sphere { radius } => {
            push_f32(&mut b, radius);
            (b, "bhkSphereShape")
        }
        BodyShape::MultiSphere { count, spheres } => {
            assert!(
                count as usize <= spheres.len(),
                "a fixture holds at most 8 spheres"
            );
            b.extend_from_slice(&[0; 12]); // shape property
            push_u32(&mut b, count);
            for (center, radius) in spheres.iter().take(count as usize) {
                for value in center.iter().copied().chain([*radius]) {
                    push_f32(&mut b, value);
                }
            }
            (b, "bhkMultiSphereShape")
        }
        BodyShape::Cylinder { a, b: end, radius } => {
            push_f32(&mut b, 0.0); // convex radius
            b.extend_from_slice(&[0; 8]);
            for point in [a, end] {
                for value in point.into_iter().chain([0.0]) {
                    push_f32(&mut b, value);
                }
            }
            push_f32(&mut b, radius);
            b.extend_from_slice(&[0; 12]);
            (b, "bhkCylinderShape")
        }
    }
}

fn triangle_shape(shape: &StaticShape<'_>, colors: &[[u8; 4]]) -> Result<Vec<u8>> {
    write_shape_block(&geometry(shape, colors), STATIC_SHADER_BLOCK)
}

fn lighting_shader_property(colors: bool) -> Vec<u8> {
    let mut block = Vec::with_capacity(100);
    push_u32(&mut block, SHADER_TYPE_DEFAULT);
    push_u32(&mut block, NULL_REF);
    push_u32(&mut block, 0);
    push_u32(&mut block, NULL_REF);
    push_u32(&mut block, 0);
    push_u32(&mut block, 0);
    push_f32(&mut block, 0.0);
    push_f32(&mut block, 0.0);
    push_f32(&mut block, 1.0);
    push_f32(&mut block, 1.0);
    push_u32(&mut block, 3);
    push_f32(&mut block, 0.0);
    push_f32(&mut block, 0.0);
    push_f32(&mut block, 0.0);
    push_f32(&mut block, 1.0);
    push_u32(&mut block, 0);
    push_f32(&mut block, 1.0);
    push_f32(&mut block, 0.0);
    push_f32(&mut block, 80.0);
    push_f32(&mut block, 1.0);
    push_f32(&mut block, 1.0);
    push_f32(&mut block, 1.0);
    push_f32(&mut block, 1.0);
    push_f32(&mut block, 0.3);
    push_f32(&mut block, 2.0);
    if colors {
        block[16..20].copy_from_slice(&(1u32 << 3).to_le_bytes());
        block[20..24].copy_from_slice(&(1u32 << 5).to_le_bytes());
    }
    debug_assert_eq!(block.len(), 100);
    block
}

/// A `BSEffectShaderProperty` whose controller reference is `controller`.
///
/// Field order is the vendored parser's (`vendor/project-wormhole-nif`):
/// the inherited `NiObjectNET` header, the flags, the UV transform, the source
/// texture, four clamp/lighting bytes, four falloff floats, the base colour,
/// the base colour scale, the soft falloff depth and the greyscale texture.
fn effect_shader_property(shape: &StaticShape<'_>, controller: u32) -> Result<Vec<u8>> {
    let mut block = Vec::with_capacity(96);
    push_u32(&mut block, NULL_REF);
    push_u32(&mut block, 0);
    push_u32(&mut block, controller);
    push_u32(&mut block, 0);
    push_u32(&mut block, 0);
    for value in [0.0f32, 0.0, 1.0, 1.0] {
        push_f32(&mut block, value);
    }
    push_sized_string(&mut block, shape.diffuse)?;
    block.extend_from_slice(&[0, 0, 0, 0]);
    for value in [0.0f32, 0.0, 1.0, 0.0] {
        push_f32(&mut block, value);
    }
    for value in [1.0f32, 1.0, 1.0, 1.0, 1.0, 0.0] {
        push_f32(&mut block, value);
    }
    push_sized_string(&mut block, "")?;
    Ok(block)
}

/// An `NiTimeController` → `BSEffectShaderPropertyFloatController` block.
fn float_controller(
    controller: &FloatController<'_>,
    interpolator: u32,
    next_controller: u32,
) -> Vec<u8> {
    let mut block = Vec::with_capacity(40);
    push_u32(&mut block, next_controller);
    push_u16(&mut block, controller.flags);
    for value in [
        controller.frequency,
        controller.phase,
        controller.start_time,
        controller.stop_time,
    ] {
        push_f32(&mut block, value);
    }
    // The controller targets the shader property at block 2.
    push_u32(&mut block, 2);
    push_u32(&mut block, interpolator);
    push_u32(&mut block, controller.variable);
    block
}

fn color_controller(
    controller: &ColorController<'_>,
    interpolator: u32,
    next_controller: u32,
) -> Vec<u8> {
    let mut block = Vec::with_capacity(40);
    push_u32(&mut block, next_controller);
    push_u16(&mut block, controller.flags);
    for value in [
        controller.frequency,
        controller.phase,
        controller.start_time,
        controller.stop_time,
    ] {
        push_f32(&mut block, value);
    }
    // The controller targets the shader property at block 2.
    push_u32(&mut block, 2);
    push_u32(&mut block, interpolator);
    // `EffectShaderControlledColor`: 0, the emissive colour.
    push_u32(&mut block, 0);
    block
}

fn point3_interpolator(data: u32) -> Vec<u8> {
    let mut block = Vec::with_capacity(16);
    for _ in 0..3 {
        push_f32(&mut block, 0.0);
    }
    push_u32(&mut block, data);
    block
}

fn pos_data(controller: &ColorController<'_>) -> Result<Vec<u8>> {
    let stride = if controller.key_type == KEY_TYPE_QUADRATIC {
        10
    } else {
        4
    };
    let mut block = Vec::with_capacity(8 + stride * 4 * controller.keys.len());
    push_u32(
        &mut block,
        u32::try_from(controller.keys.len()).map_err(|_| eyre!("NIF key count overflow"))?,
    );
    push_u32(&mut block, controller.key_type);
    for key in controller.keys {
        for value in key.iter().take(stride) {
            push_f32(&mut block, *value);
        }
    }
    Ok(block)
}

fn float_interpolator(data: u32) -> Vec<u8> {
    let mut block = Vec::with_capacity(8);
    push_f32(&mut block, 0.0);
    push_u32(&mut block, data);
    block
}

fn float_data(controller: &FloatController<'_>) -> Result<Vec<u8>> {
    let stride = if controller.key_type == KEY_TYPE_QUADRATIC {
        4
    } else {
        2
    };
    let mut block = Vec::with_capacity(8 + stride * 4 * controller.keys.len());
    push_u32(
        &mut block,
        u32::try_from(controller.keys.len()).map_err(|_| eyre!("NIF key count overflow"))?,
    );
    push_u32(&mut block, controller.key_type);
    for key in controller.keys {
        for value in key.iter().take(stride) {
            push_f32(&mut block, *value);
        }
    }
    Ok(block)
}

fn push_sized_string(out: &mut Vec<u8>, value: &str) -> Result<()> {
    push_u32(
        out,
        u32::try_from(value.len()).map_err(|_| eyre!("NIF string overflow"))?,
    );
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

fn texture_set(shape: &StaticShape<'_>) -> Result<Vec<u8>> {
    let slots = [
        shape.diffuse,
        shape.normal_texture,
        "",
        "",
        "",
        "",
        "",
        "",
        "",
    ];
    let mut block = Vec::new();
    push_u32(
        &mut block,
        u32::try_from(slots.len()).map_err(|_| eyre!("NIF texture slot overflow"))?,
    );
    for slot in slots {
        push_u32(
            &mut block,
            u32::try_from(slot.len()).map_err(|_| eyre!("NIF texture path overflow"))?,
        );
        block.extend_from_slice(slot.as_bytes());
    }
    Ok(block)
}

fn push_av_object(out: &mut Vec<u8>, name: u32) {
    push_av_object_with(out, name, NULL_REF);
}

fn push_av_object_with(out: &mut Vec<u8>, name: u32, collision_object: u32) {
    push_u32(out, name);
    push_u32(out, NULL_REF);
    push_u32(out, NULL_REF);
    push_u32(out, 0);
    for value in [0.0f32, 0.0, 0.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in [1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&1.0f32.to_le_bytes());
    push_u32(out, collision_object);
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_string8(out: &mut Vec<u8>, value: &str) {
    out.push((value.len() + 1) as u8);
    out.extend_from_slice(value.as_bytes());
    out.push(0);
}

fn max_string_length(strings: &[&str]) -> u32 {
    strings
        .iter()
        .map(|value| u32::try_from(value.len()).unwrap_or(u32::MAX))
        .max()
        .unwrap_or(0)
}

fn pack_normal(value: f32) -> u8 {
    let scaled = ((value + 1.0) * 0.5 * 255.0).round();
    scaled.clamp(0.0, 255.0) as u8
}

/// Encodes an `f32` as an IEEE 754 half-precision value (round to nearest).
fn encode_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x007f_ffff;
    if exponent <= 0 {
        if exponent < -10 {
            return sign;
        }
        let mantissa = (mantissa | 0x0080_0000) >> (1 - exponent + 13);
        return sign | mantissa as u16;
    }
    if exponent >= 31 {
        return sign | 0x7c00 | u16::from(mantissa != 0) << 9;
    }
    sign | ((exponent as u16) << 10) | ((mantissa >> 13) as u16)
}

fn bounds(positions: &[[f32; 3]]) -> ([f32; 3], f32) {
    let count = positions.len() as f32;
    let mut center = [0.0f32; 3];
    for position in positions {
        for (axis, value) in position.iter().enumerate() {
            center[axis] += value / count;
        }
    }
    let radius = positions
        .iter()
        .map(|position| {
            let dx = position[0] - center[0];
            let dy = position[1] - center[1];
            let dz = position[2] - center[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        })
        .fold(0.0f32, f32::max);
    (center, radius)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad() -> StaticShape<'static> {
        StaticShape {
            name: "GeneratedQuad",
            positions: &[
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            normals: &[[0.0, 0.0, 1.0]; 4],
            uvs: &[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            indices: &[[0, 1, 2], [0, 2, 3]],
            diffuse: "textures/generated_color.dds",
            normal_texture: "textures/generated_normal.dds",
        }
    }

    #[test]
    fn writes_the_expected_header() {
        let bytes = static_shape(&quad()).unwrap();
        let line = b"Gamebryo File Format, Version 20.2.0.7\n";
        assert!(bytes.starts_with(line));
        let version = u32::from_le_bytes(bytes[line.len()..line.len() + 4].try_into().unwrap());
        assert_eq!(version, NIF_VERSION);
        assert_eq!(bytes[line.len() + 4], 1);
        let user = u32::from_le_bytes(bytes[line.len() + 5..line.len() + 9].try_into().unwrap());
        assert_eq!(user, USER_VERSION);
        let blocks = u32::from_le_bytes(bytes[line.len() + 9..line.len() + 13].try_into().unwrap());
        assert_eq!(blocks, 4);
    }

    #[test]
    fn output_is_deterministic() {
        assert_eq!(
            static_shape(&quad()).unwrap(),
            static_shape(&quad()).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_geometry() {
        let mut shape = quad();
        shape.normals = &[];
        assert!(static_shape(&shape).is_err());
        let mut shape = quad();
        shape.indices = &[[0, 1, 9]];
        assert!(static_shape(&shape).is_err());
        let mut shape = quad();
        shape.diffuse = "";
        assert!(static_shape(&shape).is_err());
    }
}
