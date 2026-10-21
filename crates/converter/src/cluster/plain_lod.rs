//! Stand-in for baked object LOD, the comparison side of the cluster LOD stage.
//!
//! No baked object chunk builder exists in the converter, so this module does what xLODGen or
//! DynDOLOD style baked object LOD does, in simplified form: every eligible primitive gets a plain
//! discrete LOD chain, and for each block of cells the placed instances are merged into world
//! space GLB batches (one per material key), each instance using the single chain level that a
//! fixed far distance and pixel error select. Its build time, bytes and triangle counts are what
//! the report sets against the cluster build.
//!
//! The chain is the "clod" mode of the VG5 comparison harness: meshoptimizer's simplifier with
//! clusterlod's own options and fallback rule (see `simplify()` in `demo/clusterlod.h`).
//!
//! The simplifier is not wrapped by the `meshopt-cluster` crate, but its build script compiles all
//! of meshoptimizer v1.3 into the static library linked into this binary, so the C entry points
//! declared below resolve from there.

use std::collections::{BTreeMap, HashMap};

use color_eyre::Result;
use color_eyre::eyre::{bail, ensure, eyre};
use meshopt_cluster as _;

use super::model::{AlphaMode, PlacedPrimitive, PrimitiveGeometry};

// Option bits from meshoptimizer.h.
const SIMPLIFY_SPARSE: u32 = 1 << 1;
const SIMPLIFY_ERROR_ABSOLUTE: u32 = 1 << 2;
const SIMPLIFY_PERMISSIVE: u32 = 1 << 5;
const SIMPLIFY_ERROR_CLAMPED: u32 = 1 << 8;

/// The options clusterlod's `simplify()` passes with its default configuration.
const CLOD_OPTIONS: u32 =
    SIMPLIFY_SPARSE | SIMPLIFY_ERROR_ABSOLUTE | SIMPLIFY_ERROR_CLAMPED | SIMPLIFY_PERMISSIVE;

const POSITION_STRIDE: usize = 12;
/// clusterlod's appearance factor applied to the sloppy simplifier's relative error.
const SLOPPY_ERROR_FACTOR: f32 = 2.0;
/// A level that keeps more than this share of the previous level's indices is a stuck chain.
const STUCK_RATIO: f64 = 0.85;
const MIN_LEVEL_TRIANGLES: usize = 12;

unsafe extern "C" {
    fn meshopt_simplify(
        destination: *mut u32,
        indices: *const u32,
        index_count: usize,
        vertex_positions: *const f32,
        vertex_count: usize,
        vertex_positions_stride: usize,
        target_index_count: usize,
        target_error: f32,
        options: u32,
        result_error: *mut f32,
    ) -> usize;
    fn meshopt_simplifySloppy(
        destination: *mut u32,
        indices: *const u32,
        index_count: usize,
        vertex_positions: *const f32,
        vertex_count: usize,
        vertex_positions_stride: usize,
        vertex_lock: *const u8,
        target_index_count: usize,
        target_error: f32,
        result_error: *mut f32,
    ) -> usize;
    fn meshopt_simplifyScale(
        vertex_positions: *const f32,
        vertex_count: usize,
        vertex_positions_stride: usize,
    ) -> f32;
}

/// A plain discrete LOD chain of one primitive.
#[derive(Debug, Clone)]
pub struct PlainChain {
    /// Level 0 is the source indices; each later level has fewer triangles and a larger error.
    pub levels: Vec<PlainLevel>,
}

#[derive(Debug, Clone)]
pub struct PlainLevel {
    pub indices: Vec<u32>,
    /// Cumulative absolute error in mesh units: the maximum of the previous level's error and
    /// the error this level's simplification achieved. Zero for level 0.
    pub error: f32,
}

impl PlainChain {
    /// Builds the chain from the primitive's positions and triangle indices. Never panics: empty
    /// or degenerate input (no triangles, indices out of range, non-finite positions) yields a
    /// single level that holds the source indices unchanged.
    pub fn build(geometry: &PrimitiveGeometry) -> PlainChain {
        let source = PlainLevel {
            indices: geometry.indices.clone(),
            error: 0.0,
        };
        let vertex_count = geometry.positions.len();
        let usable = !geometry.indices.is_empty()
            && geometry.indices.len().is_multiple_of(3)
            && vertex_count > 0
            && geometry
                .indices
                .iter()
                .all(|&index| (index as usize) < vertex_count)
            && geometry
                .positions
                .iter()
                .all(|position| position.iter().all(|c| c.is_finite()));
        if !usable {
            return PlainChain {
                levels: vec![source],
            };
        }

        let positions: Vec<f32> = geometry.positions.iter().flatten().copied().collect();
        // SAFETY: `positions` holds `vertex_count` tightly packed xyz triples.
        let scale =
            unsafe { meshopt_simplifyScale(positions.as_ptr(), vertex_count, POSITION_STRIDE) };

        let mut levels = vec![source];
        // The simplifier writes at most as many indices as it reads.
        let mut destination = vec![0u32; geometry.indices.len()];
        loop {
            let previous = levels.last().expect("chain always has level 0");
            let previous_count = previous.indices.len();
            let target_count = (previous_count / 2) / 3 * 3;
            if target_count < MIN_LEVEL_TRIANGLES * 3 {
                break;
            }

            let mut error = 0f32;
            // SAFETY: the indices were checked against `vertex_count` above and every later level
            // is a subset of them; `destination` has room for `previous_count` indices.
            let mut kept = unsafe {
                meshopt_simplify(
                    destination.as_mut_ptr(),
                    previous.indices.as_ptr(),
                    previous_count,
                    positions.as_ptr(),
                    vertex_count,
                    POSITION_STRIDE,
                    target_count,
                    f32::MAX,
                    CLOD_OPTIONS,
                    &mut error,
                )
            };
            let mut achieved = error;

            if kept > target_count {
                let mut sloppy_error = 0f32;
                // SAFETY: same buffers and bounds as the call above; no vertex lock is passed.
                let sloppy_kept = unsafe {
                    meshopt_simplifySloppy(
                        destination.as_mut_ptr(),
                        previous.indices.as_ptr(),
                        previous_count,
                        positions.as_ptr(),
                        vertex_count,
                        POSITION_STRIDE,
                        std::ptr::null(),
                        target_count,
                        f32::MAX,
                        &mut sloppy_error,
                    )
                };
                if sloppy_kept > (previous_count as f64 * STUCK_RATIO) as usize {
                    break;
                }
                kept = sloppy_kept;
                achieved = sloppy_error * scale * SLOPPY_ERROR_FACTOR;
            }
            if kept / 3 < MIN_LEVEL_TRIANGLES {
                break;
            }

            let level = PlainLevel {
                indices: destination[..kept].to_vec(),
                error: previous.error.max(achieved),
            };
            levels.push(level);
        }
        PlainChain { levels }
    }
}

/// Camera and error budget the baked stand-in selects levels with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakedSettings {
    /// Distance from the camera to the instance's bounding sphere centre, in game units.
    pub distance: f32,
    /// Largest screen error a chosen level may project to.
    pub error_px: f32,
    pub fov_y_radians: f32,
    pub viewport_height_px: f32,
}

impl Default for BakedSettings {
    fn default() -> Self {
        Self {
            distance: 35_000.0,
            error_px: 1.0,
            fov_y_radians: 45f32.to_radians(),
            viewport_height_px: 900.0,
        }
    }
}

impl BakedSettings {
    /// Pixels per unit of error at one unit of distance: `height / (2 tan(fov / 2))`.
    fn projection_scale(&self) -> f32 {
        self.viewport_height_px / (2.0 * (self.fov_y_radians / 2.0).tan())
    }
}

/// Picks the coarsest level whose error projects to at most `settings.error_px` at
/// `settings.distance`, for a bounding sphere of `radius`. Level 0 is the fallback.
pub fn select_level(chain: &PlainChain, radius: f32, settings: &BakedSettings) -> usize {
    let scale = settings.projection_scale();
    let distance = (settings.distance - radius).max(1.0);
    let mut chosen = 0;
    for (index, level) in chain.levels.iter().enumerate() {
        if level.error / distance * scale <= settings.error_px {
            chosen = index;
        }
    }
    chosen
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BakedBlockStats {
    pub instances: usize,
    pub batches: usize,
    /// Triangles of the instances' full-detail geometry.
    pub input_triangles: u64,
    /// Triangles written to the GLB.
    pub output_triangles: u64,
    /// Vertices written to the GLB.
    pub output_vertices: u64,
}

#[derive(Debug, Clone)]
pub struct BakedBlock {
    pub glb: Vec<u8>,
    pub stats: BakedBlockStats,
}

/// One merged batch: every instance that shares material key, alpha mode and sidedness.
#[derive(Default)]
struct Batch {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colours: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

/// Merges the placed instances of one block into world-space batches and writes them as one GLB.
///
/// `chains` maps `PlacedPrimitive::mesh_key` to its chain; a missing key is an error that names
/// it. Output is deterministic: batches are ordered by material key, then alpha mode and
/// sidedness, and instances keep their input order inside a batch. Vertices are not deduplicated
/// across instances.
pub fn build_baked_block(
    placed: &[PlacedPrimitive<'_>],
    chains: &HashMap<String, PlainChain>,
    settings: &BakedSettings,
) -> Result<BakedBlock> {
    build_baked_block_with(placed, &|key| chains.get(key), settings)
}

/// [`build_baked_block`] with the chain lookup supplied by the caller, so a caller that holds the
/// chains elsewhere (the converter stage keeps them per model) does not copy them into a map.
pub fn build_baked_block_with<'c>(
    placed: &[PlacedPrimitive<'_>],
    chains: &dyn Fn(&str) -> Option<&'c PlainChain>,
    settings: &BakedSettings,
) -> Result<BakedBlock> {
    ensure!(!placed.is_empty(), "baked block has no instances");

    let mut groups: BTreeMap<(&str, u8, bool), Vec<&PlacedPrimitive<'_>>> = BTreeMap::new();
    for instance in placed {
        ensure!(
            chains(instance.mesh_key).is_some(),
            "no plain LOD chain for mesh key {}",
            instance.mesh_key
        );
        let geometry = instance.geometry;
        ensure!(
            geometry.normals.len() == geometry.positions.len()
                && geometry.uvs.len() == geometry.positions.len(),
            "primitive {} has attribute arrays of different lengths",
            instance.mesh_key
        );
        groups
            .entry((
                instance.material_key,
                alpha_rank(instance.alpha),
                instance.double_sided,
            ))
            .or_default()
            .push(instance);
    }

    let mut stats = BakedBlockStats {
        instances: placed.len(),
        input_triangles: placed
            .iter()
            .map(|instance| instance.geometry.triangle_count() as u64)
            .sum(),
        ..BakedBlockStats::default()
    };

    let mut batches: Vec<(&str, AlphaMode, bool, Batch)> = Vec::new();
    for ((material_key, _, double_sided), instances) in &groups {
        let with_colours = instances
            .iter()
            .all(|instance| instance.geometry.colours.is_some());
        let mut batch = Batch::default();
        for instance in instances {
            let chain = chains(instance.mesh_key).expect("checked above");
            let radius = bounding_radius(&instance.geometry.positions);
            let level = &chain.levels[select_level(
                chain,
                radius * instance_scale(&instance.transform),
                settings,
            )];
            append_instance(&mut batch, instance, &level.indices, with_colours)?;
        }
        if batch.indices.is_empty() {
            continue;
        }
        stats.output_triangles += (batch.indices.len() / 3) as u64;
        stats.output_vertices += batch.positions.len() as u64;
        batches.push((material_key, instances[0].alpha, *double_sided, batch));
    }
    stats.batches = batches.len();
    if batches.is_empty() {
        bail!("baked block has no triangles to write");
    }

    let glb = write_glb(&batches)?;
    Ok(BakedBlock { glb, stats })
}

fn alpha_rank(alpha: AlphaMode) -> u8 {
    match alpha {
        AlphaMode::Opaque => 0,
        AlphaMode::Mask => 1,
        AlphaMode::Blend => 2,
    }
}

fn alpha_name(alpha: AlphaMode) -> &'static str {
    match alpha {
        AlphaMode::Opaque => "OPAQUE",
        AlphaMode::Mask => "MASK",
        AlphaMode::Blend => "BLEND",
    }
}

/// Radius of the sphere around the box centre that holds every vertex.
fn bounding_radius(positions: &[[f32; 3]]) -> f32 {
    if positions.is_empty() {
        return 0.0;
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for position in positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    let centre = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    positions
        .iter()
        .map(|position| {
            let delta = [
                position[0] - centre[0],
                position[1] - centre[1],
                position[2] - centre[2],
            ];
            (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
        })
        .fold(0.0, f32::max)
}

/// Largest axis scale of the transform's 3x3 part, so a scaled instance's radius and error are in
/// world units like the distance.
fn instance_scale(transform: &[f32; 12]) -> f32 {
    (0..3)
        .map(|column| {
            let x = transform[column];
            let y = transform[4 + column];
            let z = transform[8 + column];
            (x * x + y * y + z * z).sqrt()
        })
        .fold(0.0, f32::max)
}

/// Adds the instance's chosen level to the batch in world space, keeping only the vertices that
/// level references. Triangle winding flips for mirroring transforms.
fn append_instance(
    batch: &mut Batch,
    instance: &PlacedPrimitive<'_>,
    indices: &[u32],
    with_colours: bool,
) -> Result<()> {
    let geometry = instance.geometry;
    let vertex_count = geometry.positions.len();
    ensure!(
        indices.len().is_multiple_of(3),
        "primitive {} has an index count that is not a multiple of 3",
        instance.mesh_key
    );
    let matrix = &instance.transform;
    let normal_matrix = normal_matrix(matrix);
    let mirrored = determinant(matrix) < 0.0;

    let mut remap = vec![u32::MAX; vertex_count];
    let base = batch.positions.len() as u32;
    let mut added = 0u32;
    let mut mapped = Vec::with_capacity(indices.len());
    for &index in indices {
        let slot = remap.get_mut(index as usize).ok_or_else(|| {
            eyre!(
                "primitive {} has index {index} outside its {vertex_count} vertices",
                instance.mesh_key
            )
        })?;
        if *slot == u32::MAX {
            *slot = base + added;
            added += 1;
            let source = index as usize;
            batch
                .positions
                .push(transform_point(matrix, geometry.positions[source]));
            batch
                .normals
                .push(transform_normal(&normal_matrix, geometry.normals[source]));
            batch.uvs.push(geometry.uvs[source]);
            if with_colours && let Some(colours) = &geometry.colours {
                batch.colours.push(colours[source]);
            }
        }
        mapped.push(*slot);
    }
    for triangle in mapped.as_chunks::<3>().0 {
        if mirrored {
            batch
                .indices
                .extend_from_slice(&[triangle[0], triangle[2], triangle[1]]);
        } else {
            batch.indices.extend_from_slice(&triangle[..]);
        }
    }
    Ok(())
}

fn transform_point(m: &[f32; 12], p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
        m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
        m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
    ]
}

fn determinant(m: &[f32; 12]) -> f32 {
    m[0] * (m[5] * m[10] - m[6] * m[9]) - m[1] * (m[4] * m[10] - m[6] * m[8])
        + m[2] * (m[4] * m[9] - m[5] * m[8])
}

/// Inverse transpose of the 3x3 part, row-major; the matrix itself when it is singular.
fn normal_matrix(m: &[f32; 12]) -> [f32; 9] {
    let linear = [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]];
    let det = determinant(m);
    if !det.is_finite() || det.abs() < 1e-20 {
        return linear;
    }
    // The cofactor matrix divided by the determinant is the inverse transpose.
    let cofactor = [
        m[5] * m[10] - m[6] * m[9],
        m[6] * m[8] - m[4] * m[10],
        m[4] * m[9] - m[5] * m[8],
        m[2] * m[9] - m[1] * m[10],
        m[0] * m[10] - m[2] * m[8],
        m[1] * m[8] - m[0] * m[9],
        m[1] * m[6] - m[2] * m[5],
        m[2] * m[4] - m[0] * m[6],
        m[0] * m[5] - m[1] * m[4],
    ];
    cofactor.map(|value| value / det)
}

fn transform_normal(matrix: &[f32; 9], n: [f32; 3]) -> [f32; 3] {
    let x = matrix[0] * n[0] + matrix[1] * n[1] + matrix[2] * n[2];
    let y = matrix[3] * n[0] + matrix[4] * n[1] + matrix[5] * n[2];
    let z = matrix[6] * n[0] + matrix[7] * n[1] + matrix[8] * n[2];
    let length = (x * x + y * y + z * z).sqrt();
    if length > 1e-20 && length.is_finite() {
        [x / length, y / length, z / length]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn push_f32s<const N: usize>(binary: &mut Vec<u8>, values: &[[f32; N]]) {
    for value in values {
        for component in value {
            binary.extend_from_slice(&component.to_le_bytes());
        }
    }
}

/// Writes the batches as one GLB 2.0: a node, mesh, primitive and material per batch.
fn write_glb(batches: &[(&str, AlphaMode, bool, Batch)]) -> Result<Vec<u8>> {
    let mut binary: Vec<u8> = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut materials = Vec::new();
    let mut nodes = Vec::new();

    let mut add_view = |binary: &mut Vec<u8>, start: usize, target: u32| {
        buffer_views.push(serde_json::json!({
            "buffer": 0,
            "byteOffset": start,
            "byteLength": binary.len() - start,
            "target": target,
        }));
        buffer_views.len() - 1
    };

    for (batch_index, (material_key, alpha, double_sided, batch)) in batches.iter().enumerate() {
        let vertex_count = batch.positions.len();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for position in &batch.positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
        }

        let mut attributes = serde_json::Map::new();
        let mut add_attribute =
            |name: &str, view: usize, kind: &str, extent: Option<([f32; 3], [f32; 3])>| -> usize {
                let mut accessor = serde_json::json!({
                    "bufferView": view,
                    "componentType": 5126,
                    "count": vertex_count,
                    "type": kind,
                });
                if let Some((low, high)) = extent {
                    accessor["min"] = serde_json::json!(low);
                    accessor["max"] = serde_json::json!(high);
                }
                accessors.push(accessor);
                attributes.insert(name.to_owned(), serde_json::json!(accessors.len() - 1));
                accessors.len() - 1
            };

        let start = binary.len();
        push_f32s(&mut binary, &batch.positions);
        let view = add_view(&mut binary, start, 34962);
        add_attribute("POSITION", view, "VEC3", Some((min, max)));

        let start = binary.len();
        push_f32s(&mut binary, &batch.normals);
        let view = add_view(&mut binary, start, 34962);
        add_attribute("NORMAL", view, "VEC3", None);

        let start = binary.len();
        push_f32s(&mut binary, &batch.uvs);
        let view = add_view(&mut binary, start, 34962);
        add_attribute("TEXCOORD_0", view, "VEC2", None);

        if batch.colours.len() == vertex_count {
            let start = binary.len();
            push_f32s(&mut binary, &batch.colours);
            let view = add_view(&mut binary, start, 34962);
            add_attribute("COLOR_0", view, "VEC4", None);
        }

        let start = binary.len();
        for index in &batch.indices {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        let view = add_view(&mut binary, start, 34963);
        accessors.push(serde_json::json!({
            "bufferView": view,
            "componentType": 5125,
            "count": batch.indices.len(),
            "type": "SCALAR",
        }));
        let index_accessor = accessors.len() - 1;

        materials.push(serde_json::json!({
            "name": format!("baked_{batch_index}"),
            "alphaMode": alpha_name(*alpha),
            "doubleSided": double_sided,
            "pbrMetallicRoughness": {
                "baseColorFactor": [1.0, 1.0, 1.0, 1.0],
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
            },
            "extras": { "mudcrabBakedBatch": { "materialKey": material_key } },
        }));
        meshes.push(serde_json::json!({
            "name": format!("baked_batch_{batch_index}"),
            "primitives": [{
                "attributes": attributes,
                "indices": index_accessor,
                "material": batch_index,
                "mode": 4,
            }],
        }));
        nodes.push(serde_json::json!({
            "name": format!("baked_batch_{batch_index}"),
            "mesh": batch_index,
        }));
    }

    ensure!(
        binary.len() < u32::MAX as usize / 2,
        "baked block binary chunk is too large for one GLB"
    );
    let json = serde_json::json!({
        "asset": { "version": "2.0", "generator": "Mudcrab baked object LOD stand-in" },
        "scene": 0,
        "scenes": [{ "nodes": (0..nodes.len()).collect::<Vec<_>>() }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": buffer_views,
        "buffers": [{ "byteLength": binary.len() }],
    });
    let mut json_bytes = serde_json::to_vec(&json)?;
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }

    let total_length = 12 + 8 + json_bytes.len() + 8 + binary.len();
    let mut glb = Vec::with_capacity(total_length);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total_length as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\x00");
    glb.extend_from_slice(&binary);
    Ok(glb)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: [f32; 12] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];

    /// A bumpy `side` x `side` vertex grid, `spacing` units apart, on the XZ plane.
    fn grid(side: usize, spacing: f32, with_colours: bool) -> PrimitiveGeometry {
        let mut geometry = PrimitiveGeometry::default();
        for row in 0..side {
            for column in 0..side {
                let (x, z) = (column as f32 * spacing, row as f32 * spacing);
                let y = 3.0 * spacing * (x * 0.07 / spacing).sin() * (z * 0.05 / spacing).cos();
                geometry.positions.push([x, y, z]);
                geometry.normals.push([0.0, 1.0, 0.0]);
                geometry.uvs.push([column as f32, row as f32]);
            }
        }
        for row in 0..side - 1 {
            for column in 0..side - 1 {
                let a = (row * side + column) as u32;
                let b = a + 1;
                let c = a + side as u32;
                let d = c + 1;
                geometry.indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        if with_colours {
            geometry.colours = Some(vec![[0.5, 0.5, 0.5, 1.0]; side * side]);
        }
        geometry
    }

    fn translated(x: f32, y: f32, z: f32) -> [f32; 12] {
        let mut transform = IDENTITY;
        transform[3] = x;
        transform[7] = y;
        transform[11] = z;
        transform
    }

    fn placed<'a>(
        geometry: &'a PrimitiveGeometry,
        material_key: &'a str,
        transform: [f32; 12],
        mesh_key: &'a str,
    ) -> PlacedPrimitive<'a> {
        PlacedPrimitive {
            geometry,
            material_key,
            alpha: AlphaMode::Opaque,
            double_sided: false,
            transform,
            mesh_key,
        }
    }

    fn read_positions(glb: &[u8]) -> Vec<Vec<[f32; 3]>> {
        let gltf = gltf::Gltf::from_slice(glb).expect("valid GLB");
        let blob = gltf.blob.as_deref().expect("BIN chunk");
        gltf.meshes()
            .map(|mesh| {
                let primitive = mesh.primitives().next().expect("one primitive");
                let reader = primitive.reader(|_| Some(blob));
                reader.read_positions().expect("positions").collect()
            })
            .collect()
    }

    #[test]
    fn chain_of_a_grid_is_monotone_and_deterministic() {
        let geometry = grid(64, 1.0, false);
        let chain = PlainChain::build(&geometry);
        assert!(chain.levels.len() > 3, "levels: {}", chain.levels.len());
        assert_eq!(chain.levels[0].indices, geometry.indices);
        assert_eq!(chain.levels[0].error, 0.0);
        for pair in chain.levels.windows(2) {
            assert!(pair[1].error >= pair[0].error);
            assert!(pair[1].indices.len() < pair[0].indices.len());
        }

        let again = PlainChain::build(&geometry);
        assert_eq!(chain.levels.len(), again.levels.len());
        for (left, right) in chain.levels.iter().zip(&again.levels) {
            assert_eq!(left.indices, right.indices);
            assert_eq!(left.error.to_bits(), right.error.to_bits());
        }
    }

    #[test]
    fn degenerate_input_gives_one_level() {
        let empty = PlainChain::build(&PrimitiveGeometry::default());
        assert_eq!(empty.levels.len(), 1);
        assert!(empty.levels[0].indices.is_empty());

        let mut broken = grid(4, 1.0, false);
        broken.indices.push(0);
        assert_eq!(PlainChain::build(&broken).levels.len(), 1);

        let mut out_of_range = grid(4, 1.0, false);
        out_of_range.indices[0] = 9_999;
        assert_eq!(PlainChain::build(&out_of_range).levels.len(), 1);

        let mut non_finite = grid(4, 1.0, false);
        non_finite.positions[0][0] = f32::NAN;
        assert_eq!(PlainChain::build(&non_finite).levels.len(), 1);
    }

    #[test]
    fn farther_distance_selects_a_coarser_level() {
        let chain = PlainChain::build(&grid(64, 10.0, false));
        let near = BakedSettings {
            distance: 2_000.0,
            ..BakedSettings::default()
        };
        let far = BakedSettings::default();
        assert_eq!(far.distance, 35_000.0);
        let near_level = select_level(&chain, 10.0, &near);
        let far_level = select_level(&chain, 10.0, &far);
        assert!(far_level > near_level, "near {near_level}, far {far_level}");
    }

    #[test]
    fn baked_block_merges_by_material_and_reduces_triangles() {
        let geometry = grid(64, 10.0, true);
        let other = grid(16, 10.0, true);
        let mut chains = HashMap::new();
        chains.insert("grid-p0".to_owned(), PlainChain::build(&geometry));
        chains.insert("other-p0".to_owned(), PlainChain::build(&other));
        let instances = [
            placed(
                &geometry,
                "material-a",
                translated(0.0, 0.0, 0.0),
                "grid-p0",
            ),
            placed(
                &geometry,
                "material-a",
                translated(100.0, 0.0, 0.0),
                "grid-p0",
            ),
            placed(
                &other,
                "material-b",
                translated(0.0, 0.0, 500.0),
                "other-p0",
            ),
        ];
        let settings = BakedSettings::default();

        let block = build_baked_block(&instances, &chains, &settings).unwrap();
        assert_eq!(block.stats.instances, 3);
        assert_eq!(block.stats.batches, 2);
        assert_eq!(
            block.stats.input_triangles,
            (2 * geometry.triangle_count() + other.triangle_count()) as u64
        );
        assert!(block.stats.output_triangles < block.stats.input_triangles);
        assert!(block.stats.output_triangles > 0);

        let gltf = gltf::Gltf::from_slice(&block.glb).expect("valid GLB");
        assert_eq!(gltf.meshes().count(), 2);
        assert_eq!(gltf.materials().count(), 2);
        let mut vertices = 0u64;
        let mut triangles = 0u64;
        for mesh in gltf.meshes() {
            let primitive = mesh.primitives().next().unwrap();
            let position = primitive.get(&gltf::Semantic::Positions).unwrap();
            assert!(primitive.get(&gltf::Semantic::Normals).is_some());
            assert!(primitive.get(&gltf::Semantic::TexCoords(0)).is_some());
            let colours = primitive.get(&gltf::Semantic::Colors(0)).unwrap();
            assert_eq!(colours.count(), position.count());
            vertices += position.count() as u64;
            triangles += primitive.indices().unwrap().count() as u64 / 3;
        }
        assert_eq!(vertices, block.stats.output_vertices);
        assert_eq!(triangles, block.stats.output_triangles);
        let materials: Vec<_> = gltf.materials().collect();
        assert!(materials.iter().all(|m| !m.double_sided()));
        assert!(
            materials
                .iter()
                .all(|m| m.alpha_mode() == gltf::material::AlphaMode::Opaque)
        );

        let again = build_baked_block(&instances, &chains, &settings).unwrap();
        assert_eq!(block.glb, again.glb);
        assert_eq!(block.stats, again.stats);
    }

    #[test]
    fn colours_are_dropped_when_one_instance_has_none() {
        let coloured = grid(8, 1.0, true);
        let plain = grid(8, 1.0, false);
        let mut chains = HashMap::new();
        chains.insert("coloured".to_owned(), PlainChain::build(&coloured));
        chains.insert("plain".to_owned(), PlainChain::build(&plain));
        let instances = [
            placed(&coloured, "m", IDENTITY, "coloured"),
            placed(&plain, "m", translated(20.0, 0.0, 0.0), "plain"),
        ];
        let block = build_baked_block(&instances, &chains, &BakedSettings::default()).unwrap();
        let gltf = gltf::Gltf::from_slice(&block.glb).unwrap();
        let primitive = gltf.meshes().next().unwrap().primitives().next().unwrap();
        assert!(primitive.get(&gltf::Semantic::Colors(0)).is_none());
    }

    #[test]
    fn transform_moves_vertices_and_rotates_normals() {
        let mut geometry = PrimitiveGeometry {
            positions: vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            normals: vec![[1.0, 0.0, 0.0]; 3],
            uvs: vec![[0.0, 0.0]; 3],
            tangents: None,
            colours: None,
            indices: vec![0, 1, 2],
        };
        geometry.normals[1] = [0.0, 1.0, 0.0];
        let mut chains = HashMap::new();
        chains.insert("tri".to_owned(), PlainChain::build(&geometry));
        // 90 degrees about Z (x -> y, y -> -x), then a translation of (10, 20, 30).
        let transform = [
            0.0, -1.0, 0.0, 10.0, 1.0, 0.0, 0.0, 20.0, 0.0, 0.0, 1.0, 30.0,
        ];
        let instances = [placed(&geometry, "m", transform, "tri")];
        let block = build_baked_block(&instances, &chains, &BakedSettings::default()).unwrap();

        let positions = &read_positions(&block.glb)[0];
        assert_eq!(positions.len(), 3);
        let expected = [[10.0, 21.0, 30.0], [9.0, 20.0, 30.0], [10.0, 20.0, 31.0]];
        for (position, wanted) in positions.iter().zip(expected) {
            for axis in 0..3 {
                assert!((position[axis] - wanted[axis]).abs() < 1e-5, "{position:?}");
            }
        }

        let gltf = gltf::Gltf::from_slice(&block.glb).unwrap();
        let blob = gltf.blob.as_deref().unwrap();
        let primitive = gltf.meshes().next().unwrap().primitives().next().unwrap();
        let normals: Vec<[f32; 3]> = primitive
            .reader(|_| Some(blob))
            .read_normals()
            .unwrap()
            .collect();
        assert!((normals[0][1] - 1.0).abs() < 1e-5 && normals[0][0].abs() < 1e-5);
        assert!((normals[1][0] + 1.0).abs() < 1e-5 && normals[1][1].abs() < 1e-5);
    }

    #[test]
    fn mirroring_transform_flips_winding() {
        let geometry = PrimitiveGeometry {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 3],
            uvs: vec![[0.0, 0.0]; 3],
            tangents: None,
            colours: None,
            indices: vec![0, 1, 2],
        };
        let mut chains = HashMap::new();
        chains.insert("tri".to_owned(), PlainChain::build(&geometry));
        let mirror = [-1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let instances = [placed(&geometry, "m", mirror, "tri")];
        let block = build_baked_block(&instances, &chains, &BakedSettings::default()).unwrap();
        let gltf = gltf::Gltf::from_slice(&block.glb).unwrap();
        let blob = gltf.blob.as_deref().unwrap();
        let primitive = gltf.meshes().next().unwrap().primitives().next().unwrap();
        let indices: Vec<u32> = primitive
            .reader(|_| Some(blob))
            .read_indices()
            .unwrap()
            .into_u32()
            .collect();
        assert_eq!(indices, vec![0, 2, 1]);
    }

    #[test]
    fn missing_chain_and_empty_block_are_errors() {
        let geometry = grid(4, 1.0, false);
        let instances = [placed(&geometry, "m", IDENTITY, "absent-p0")];
        let error = build_baked_block(&instances, &HashMap::new(), &BakedSettings::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("absent-p0"), "{error}");

        let empty = build_baked_block(&[], &HashMap::new(), &BakedSettings::default());
        assert!(empty.is_err());
    }
}
