//! Distant tree billboards from the package's Skyrim tree LOD (`.lst` types, `.btt` instances),
//! the tree half of the baked LOD (ADR-0013; companion of [`super::lod_objects`]).
//!
//! The converter writes one crossed-quad GLB per tree type, cut from the worldspace's tree atlas,
//! and one row per instance (`lod_tree_type`, `lod_tree_instance`). The engine draws an instance
//! as that quad pair placed with the instance's position, yaw and scale. Drawing tens of
//! thousands of instances as entities of their own would make the entity count, not the GPU, the
//! cost; instead each instance block's rows are merged on the CPU into **one mesh per cell**
//! (per atlas), all sharing the atlas material, so a tree block is a handful of draws and a cell
//! is the unit that hands off when its full-detail model is drawable (current render-view proof).
//!
//! The type geometry is read back from the converter's own GLBs ([`TypeGeometry`]), not
//! recomputed, so the converter stays the one place that knows how a billboard is built. A type
//! whose mesh is not a plain triangle list with matching positions, normals and UVs fails; so does
//! a block whose instances name a type the worldspace does not list or carry a value the engine
//! cannot place. A block that fails draws nothing, is counted, and retries a transient database
//! error with the terrain LOD's bounded backoff.
//!
//! Distance is `fTreeLoadDistance`, 75,000 units by default (`--ini` reads it). Tree blocks leave
//! with a margin, and despawn at once: a block is at most sixteen cells of one entity per atlas, nothing like a
//! scene's hundreds, so it does not go through the paced unload.

use super::{
    RenderOrigin, StreamingCommitBudget, StreamingMetrics,
    lod::{
        LodChunkFailureClass, LodChunkGridOrigin, LodDemand, LodDemandViews, lod_retry_deadline,
        lod_ring_layers,
    },
    lod_objects::rect_distance,
};
use crate::{
    config::EngineConfig,
    graphics_settings::GraphicsSettings,
    profiling::ProfilingState,
    world::{
        components::CELL_SIZE,
        object_lod::{
            IndexState, ObjectLodSource, TREE_BLOCK_LEVEL, TreeBlockResponse, TreeInstance,
        },
    },
};
use bevy::{
    asset::{AssetPath, LoadState, RenderAssetUsages},
    camera::primitives::Aabb,
    gltf::GltfAssetLabel,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use shared::coordinates::{creation_euler_to_runtime_quaternion, creation_to_runtime_vector};
use std::{
    collections::{BTreeMap, HashMap},
    time::Instant,
};

/// The margin, in cells, a resident tree block keeps past its reach before it leaves.
const TREE_UNLOAD_MARGIN_CELLS: i32 = 2;

/// Tree block reads allowed in flight at once.
const TREE_READS_IN_FLIGHT: usize = 8;

/// A billboard type's geometry as the converter wrote it: a plain indexed triangle list in
/// runtime space, its base at the origin.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TypeGeometry {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl TypeGeometry {
    /// Reads and checks a type mesh. `Err` names what is wrong with it.
    pub(super) fn from_mesh(mesh: &Mesh) -> Result<Self, String> {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
            return Err("mesh is not a triangle list".to_owned());
        }
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return Err("mesh has no Float32x3 positions".to_owned());
        };
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            return Err("mesh has no Float32x3 normals".to_owned());
        };
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            return Err("mesh has no Float32x2 UVs".to_owned());
        };
        if positions.is_empty() || positions.len() != normals.len() || positions.len() != uvs.len()
        {
            return Err("mesh attributes do not agree in length".to_owned());
        }
        let indices: Vec<u32> = match mesh.indices() {
            Some(Indices::U16(values)) => values.iter().map(|index| u32::from(*index)).collect(),
            Some(Indices::U32(values)) => values.clone(),
            None => return Err("mesh has no indices".to_owned()),
        };
        if indices.is_empty()
            || !indices.len().is_multiple_of(3)
            || indices
                .iter()
                .any(|index| *index as usize >= positions.len())
        {
            return Err("mesh indices are not whole triangles inside the vertex list".to_owned());
        }
        let finite = positions
            .iter()
            .chain(normals)
            .flatten()
            .chain(uvs.iter().flatten())
            .all(|value| value.is_finite());
        if !finite {
            return Err("mesh has a non-finite vertex value".to_owned());
        }
        Ok(Self {
            positions: positions.clone(),
            normals: normals.clone(),
            uvs: uvs.clone(),
            indices,
        })
    }
}

/// One instance block's merged geometry for one cell and one atlas.
#[derive(Debug, PartialEq)]
pub(super) struct MergedCell {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    min: Vec3,
    max: Vec3,
}

impl Default for MergedCell {
    /// An empty cell, with inverted bounds so the first instance sets both corners.
    fn default() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        }
    }
}

impl MergedCell {
    /// Appends one instance of `geometry`, placed relative to the corner of `cell`.
    fn push(&mut self, geometry: &TypeGeometry, instance: &TreeInstance, cell: IVec2) {
        let corner = [cell.x as f32 * CELL_SIZE, cell.y as f32 * CELL_SIZE, 0.0];
        let local = creation_to_runtime_vector([
            instance.position[0] - corner[0],
            instance.position[1] - corner[1],
            instance.position[2],
        ]);
        let [x, y, z, w] = creation_euler_to_runtime_quaternion([0.0, 0.0, instance.rotation]);
        let rotation = Quat::from_xyzw(x, y, z, w);
        let base = self.positions.len() as u32;
        for ((position, normal), uv) in geometry
            .positions
            .iter()
            .zip(&geometry.normals)
            .zip(&geometry.uvs)
        {
            let placed = rotation * (Vec3::from(*position) * instance.scale) + Vec3::from(local);
            self.min = self.min.min(placed);
            self.max = self.max.max(placed);
            self.positions.push(placed.into());
            self.normals.push((rotation * Vec3::from(*normal)).into());
            self.uvs.push(*uv);
        }
        self.indices
            .extend(geometry.indices.iter().map(|index| base + index));
    }

    #[cfg(test)]
    pub(super) fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    fn into_mesh(self) -> (Mesh, Aabb) {
        let aabb = Aabb::from_min_max(self.min, self.max);
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_indices(Indices::U32(self.indices));
        (mesh, aabb)
    }
}

/// The absolute cell an instance stands in.
pub(super) fn instance_cell(position: [f32; 3]) -> IVec2 {
    IVec2::new(
        (position[0] / CELL_SIZE).floor() as i32,
        (position[1] / CELL_SIZE).floor() as i32,
    )
}

/// Merges instances into one mesh per `(cell, atlas)`. `groups` maps a type index to its atlas
/// group; an instance of a type with no geometry is an error, never skipped, so a half-drawn
/// forest cannot pass for a whole one.
pub(super) fn merge_instances(
    instances: &[TreeInstance],
    geometry: &BTreeMap<u32, (usize, TypeGeometry)>,
) -> Result<BTreeMap<(i32, i32, usize), MergedCell>, String> {
    let mut cells: BTreeMap<(i32, i32, usize), MergedCell> = BTreeMap::new();
    for instance in instances {
        let (group, shape) = geometry
            .get(&instance.tree_index)
            .ok_or_else(|| format!("tree type {} has no usable geometry", instance.tree_index))?;
        let cell = instance_cell(instance.position);
        cells
            .entry((cell.x, cell.y, *group))
            .or_default()
            .push(shape, instance, cell);
    }
    Ok(cells)
}

enum TypeState {
    Loading,
    Ready {
        group: usize,
        geometry: TypeGeometry,
    },
    Failed,
}

struct TypeSlot {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    state: TypeState,
}

/// The billboard types of the worldspace being streamed, loaded once per worldspace.
#[derive(Default)]
struct TreeCatalog {
    worldspace_id: Option<u32>,
    types: BTreeMap<u32, TypeSlot>,
    /// One material per atlas image, in the order the types first named it.
    groups: Vec<(Option<AssetId<Image>>, Handle<StandardMaterial>)>,
    settled: bool,
    logged: bool,
}

impl TreeCatalog {
    fn usable(&self) -> bool {
        // Even an entirely failed catalog must read demanded blocks: merging
        // names the missing type and records failure against that block.
        self.settled
    }

    fn geometry(&self) -> BTreeMap<u32, (usize, TypeGeometry)> {
        self.types
            .iter()
            .filter_map(|(index, slot)| match &slot.state {
                TypeState::Ready { group, geometry } => Some((*index, (*group, geometry.clone()))),
                _ => None,
            })
            .collect()
    }
}

enum TreeBlockStatus {
    /// The instance read is in flight.
    Requested {
        retry_count: u8,
    },
    Ready {
        root: Entity,
    },
    /// A block that failed. `retry_at` is a transient failure's next attempt; `None` is final.
    Failed {
        retry_count: u8,
        retry_at: Option<Instant>,
    },
}

#[derive(Resource, Default)]
pub(super) struct TreeLodStreaming {
    worldspace_id: Option<u32>,
    center: Option<IVec2>,
    catalog: TreeCatalog,
    blocks: HashMap<IVec2, TreeBlockStatus>,
    queue: Vec<IVec2>,
    wanted: std::collections::HashSet<IVec2>,
    plan_dirty: bool,
    demand: Option<LodDemand>,
    stopped: bool,
    /// `--lod-keep-hidden-in-interiors` is holding the ready blocks hidden and the catalog loaded
    /// for the return to the exterior; the return shows them again.
    held: bool,
}

#[derive(Component)]
struct TreeLodBlockRoot;

pub(super) struct TreeLodPlugin;

impl Plugin for TreeLodPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TreeLodStreaming>().add_systems(
            Update,
            (
                plan_tree_lod
                    .after(super::lod::plan_lod_chunks)
                    .before(super::collect_cells),
                collect_tree_lod
                    .after(super::lod::collect_lod_chunks)
                    .before(super::arm_pending_models),
            ),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_tree_lod(
    mut commands: Commands,
    config: Res<EngineConfig>,
    views: LodDemandViews,
    asset_server: Res<AssetServer>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    origin: Res<RenderOrigin>,

    settings: Option<Res<GraphicsSettings>>,
    source: Option<ResMut<ObjectLodSource>>,
    mut streaming: ResMut<TreeLodStreaming>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    let started = Instant::now();
    let Some(mut source) = source else {
        return;
    };
    let index = match source.poll_index() {
        IndexState::Ready(index) => index.clone(),
        _ => return,
    };
    let actual = views.current(&config, origin.0);
    let demand = actual;
    let world = settings
        .as_ref()
        .is_none_or(|s| s.lod_trees)
        .then_some(demand)
        .flatten()
        .map(|d| d.world)
        .and_then(|id| index.worlds.get(&id).map(|world| (id, world)))
        .filter(|(_, world)| !world.tree_types.is_empty() && !world.tree_blocks.is_empty());
    if streaming.demand != demand {
        streaming.plan_dirty = true;
    }
    streaming.demand = demand;
    for status in streaming.blocks.values() {
        if let TreeBlockStatus::Ready { root } = status {
            commands.entity(*root).try_insert(if demand.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
    let Some((worldspace_id, world)) = world else {
        if !streaming.stopped {
            streaming.stopped = true;
            info!("distant tree LOD stopped: nothing to draw in the active space");
        }
        let holding = config.lod_keep_hidden_in_interiors
            && actual.is_none()
            && settings.as_ref().is_none_or(|settings| settings.lod_trees)
            && streaming.catalog.worldspace_id.is_some();
        if holding {
            for status in streaming.blocks.values() {
                if let TreeBlockStatus::Ready { root } = status {
                    commands.entity(*root).try_insert(Visibility::Hidden);
                }
            }
            streaming.held = true;
        } else {
            clear_blocks(&mut commands, &mut streaming);
            streaming.catalog = TreeCatalog::default();
            streaming.held = false;
        }
        streaming.worldspace_id = None;
        streaming.center = None;
        update_counts(&streaming, &mut metrics);
        return;
    };
    if streaming.held {
        // Back in an exterior: the held blocks of the same worldspace draw again; those of another
        // are cleared below with the catalog.
        streaming.held = false;
        if streaming.catalog.worldspace_id == Some(worldspace_id) {
            for status in streaming.blocks.values() {
                if let TreeBlockStatus::Ready { root } = status {
                    commands.entity(*root).try_insert(Visibility::Inherited);
                }
            }
        }
    }
    if streaming.stopped {
        streaming.stopped = false;
        info!(
            worldspace_id,
            "distant tree LOD resumed in the active exterior"
        );
    }

    // The billboard types of this worldspace, loaded once.
    if streaming.catalog.worldspace_id != Some(worldspace_id) {
        clear_blocks(&mut commands, &mut streaming);
        let mut catalog = TreeCatalog {
            worldspace_id: Some(worldspace_id),
            ..TreeCatalog::default()
        };
        for tree in world.tree_types.values() {
            catalog.types.insert(
                tree.index,
                TypeSlot {
                    mesh: asset_server.load(
                        GltfAssetLabel::Primitive {
                            mesh: 0,
                            primitive: 0,
                        }
                        .from_asset(tree.mesh_path.clone()),
                    ),
                    // The stock glTF loader publishes the `StandardMaterial` it builds from
                    // `Material0` under the `/std` sub-label (see `render.rs`'s material handler).
                    material: asset_server
                        .load(AssetPath::from(tree.mesh_path.clone()).with_label("Material0/std")),
                    state: TypeState::Loading,
                },
            );
        }
        streaming.catalog = catalog;
        streaming.plan_dirty = true;
    }
    if !streaming.catalog.settled {
        settle_catalog(&mut streaming.catalog, &asset_server, &meshes, &materials);
        if streaming.catalog.settled && !streaming.catalog.logged {
            streaming.catalog.logged = true;
            let ready = streaming
                .catalog
                .types
                .values()
                .filter(|slot| matches!(slot.state, TypeState::Ready { .. }))
                .count();
            let total = streaming.catalog.types.len();
            if ready == 0 {
                error!(
                    worldspace_id,
                    total, "distant tree LOD is off: no billboard type loaded"
                );
            } else {
                info!(
                    worldspace_id,
                    ready,
                    total,
                    groups = streaming.catalog.groups.len(),
                    "distant tree billboard types loaded"
                );
            }
            streaming.plan_dirty = true;
        }
    }
    if !streaming.catalog.usable() {
        update_counts(&streaming, &mut metrics);
        return;
    }

    let center = demand.expect("wanted world has demand").center;
    if streaming.center != Some(center) || streaming.worldspace_id != Some(worldspace_id) {
        streaming.plan_dirty = true;
    }
    streaming.worldspace_id = Some(worldspace_id);
    streaming.center = Some(center);
    if streaming.plan_dirty {
        streaming.plan_dirty = false;
        let reach = config.object_lod.tree_reach_cells();
        let wanted: std::collections::HashSet<IVec2> = world
            .tree_blocks
            .iter()
            .copied()
            .filter(|block| rect_distance(center, *block, TREE_BLOCK_LEVEL) <= reach)
            .collect();
        // Leave with a margin so standing on the edge does not load and drop a block every frame.
        let keep = |block: &IVec2| {
            rect_distance(center, *block, TREE_BLOCK_LEVEL) <= reach + TREE_UNLOAD_MARGIN_CELLS
        };
        let mut left = 0u64;
        streaming.blocks.retain(|block, status| {
            if keep(block) {
                return true;
            }
            if let TreeBlockStatus::Ready { root } = status {
                commands.entity(*root).try_despawn();
                left += 1;
            }
            false
        });
        if left > 0 {
            profiler.increment("lod/tree_blocks_unloaded", left);
        }
        let mut queue: Vec<IVec2> = wanted
            .iter()
            .filter(|block| !streaming.blocks.contains_key(*block))
            .copied()
            .collect();
        queue.sort_by_key(|block| {
            (
                rect_distance(center, *block, TREE_BLOCK_LEVEL),
                block.x,
                block.y,
            )
        });
        streaming.queue = queue;
        streaming.wanted = wanted;
    }

    // Transient failures whose backoff passed are asked for again.
    let now = Instant::now();
    let retries: Vec<IVec2> = streaming
        .blocks
        .iter()
        .filter_map(|(block, status)| match status {
            TreeBlockStatus::Failed {
                retry_at: Some(retry_at),
                ..
            } if *retry_at <= now && streaming.wanted.contains(block) => Some(*block),
            _ => None,
        })
        .collect();
    // A due retry that already waits in the queue (no read slot was free yet) is not queued twice.
    let waiting: std::collections::HashSet<IVec2> = streaming.queue.iter().copied().collect();
    streaming
        .queue
        .extend(retries.into_iter().filter(|block| !waiting.contains(block)));

    // Reads in flight are bounded, so a fast camera does not queue the whole forest.
    let in_flight = streaming
        .blocks
        .values()
        .filter(|status| matches!(status, TreeBlockStatus::Requested { .. }))
        .count();
    let mut room = TREE_READS_IN_FLIGHT.saturating_sub(in_flight);
    while room > 0 && !streaming.queue.is_empty() {
        let block = streaming.queue.remove(0);
        let retry_count = match streaming.blocks.get(&block) {
            None => 0,
            Some(TreeBlockStatus::Failed {
                retry_count,
                retry_at: Some(retry_at),
            }) if *retry_at <= now => retry_count.saturating_add(1),
            Some(_) => continue,
        };
        if source.request_tree_block(worldspace_id, block).is_err() {
            // The queue is full; try again next frame.
            streaming.queue.insert(0, block);
            break;
        }
        streaming
            .blocks
            .insert(block, TreeBlockStatus::Requested { retry_count });
        metrics.lod_tree_blocks_requested = metrics.lod_tree_blocks_requested.saturating_add(1);
        profiler.increment("lod/tree_blocks_requested", 1);
        room -= 1;
    }
    update_counts(&streaming, &mut metrics);
    profiler.record_elapsed("lod/trees_plan", started);
}

/// Despawns every tree block and forgets the requests: a change of worldspace, or LOD stopping.
fn clear_blocks(commands: &mut Commands, streaming: &mut TreeLodStreaming) {
    for (_, status) in streaming.blocks.drain() {
        if let TreeBlockStatus::Ready { root } = status {
            commands.entity(root).try_despawn();
        }
    }
    streaming.queue.clear();
    streaming.wanted.clear();
}

/// Moves the type slots whose meshes and materials have loaded to ready or failed, and settles
/// the catalog once none is still loading.
fn settle_catalog(
    catalog: &mut TreeCatalog,
    asset_server: &AssetServer,
    meshes: &Assets<Mesh>,
    materials: &Assets<StandardMaterial>,
) {
    let mut loading = false;
    let indices: Vec<u32> = catalog.types.keys().copied().collect();
    for index in indices {
        let slot = catalog.types.get_mut(&index).expect("listed above");
        if !matches!(slot.state, TypeState::Loading) {
            continue;
        }
        let failed = matches!(asset_server.load_state(&slot.mesh), LoadState::Failed(_))
            || matches!(
                asset_server.load_state(&slot.material),
                LoadState::Failed(_)
            );
        if failed {
            warn!(index, "a tree billboard type failed to load");
            slot.state = TypeState::Failed;
            continue;
        }
        let loaded = asset_server.is_loaded_with_dependencies(&slot.mesh)
            && asset_server.is_loaded_with_dependencies(&slot.material);
        let (Some(mesh), Some(material)) = (meshes.get(&slot.mesh), materials.get(&slot.material))
        else {
            loading = true;
            continue;
        };
        if !loaded {
            loading = true;
            continue;
        }
        match TypeGeometry::from_mesh(mesh) {
            Ok(geometry) => {
                let atlas = material.base_color_texture.as_ref().map(Handle::id);
                let group = match catalog.groups.iter().position(|(known, _)| *known == atlas) {
                    Some(group) => group,
                    None => {
                        catalog.groups.push((atlas, slot.material.clone()));
                        catalog.groups.len() - 1
                    }
                };
                slot.state = TypeState::Ready { group, geometry };
            }
            Err(reason) => {
                warn!(index, %reason, "a tree billboard type is not usable");
                slot.state = TypeState::Failed;
            }
        }
    }
    catalog.settled = !loading;
}

#[allow(clippy::too_many_arguments)]
fn collect_tree_lod(
    mut commands: Commands,
    config: Res<EngineConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    origin: Res<RenderOrigin>,
    source: Option<Res<ObjectLodSource>>,
    mut streaming: ResMut<TreeLodStreaming>,
    mut budget: ResMut<StreamingCommitBudget>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
) {
    let Some(source) = source else {
        return;
    };
    let started = Instant::now();
    while budget.lod_slot_open(config.max_commit_micros_per_frame) {
        let Some(response) = source.try_tree_response() else {
            break;
        };
        let TreeBlockResponse {
            worldspace_id,
            block,
            result,
        } = response;
        // An answer for a block the plan no longer wants, or for another worldspace, is dropped.
        let retry_count = match streaming.blocks.get(&block) {
            Some(TreeBlockStatus::Requested { retry_count })
                if streaming.worldspace_id == Some(worldspace_id)
                    && streaming.wanted.contains(&block) =>
            {
                *retry_count
            }
            _ => {
                if streaming.worldspace_id == Some(worldspace_id)
                    && matches!(
                        streaming.blocks.get(&block),
                        Some(TreeBlockStatus::Requested { .. })
                    )
                {
                    streaming.blocks.remove(&block);
                }
                metrics.lod_tree_stale_responses =
                    metrics.lod_tree_stale_responses.saturating_add(1);
                continue;
            }
        };
        budget.spend_lod_slot();
        let built = result
            .map_err(|failure| {
                let class = if failure.transient {
                    LodChunkFailureClass::Transient
                } else {
                    LodChunkFailureClass::Terminal
                };
                (class, failure.reason)
            })
            .and_then(|instances| {
                let geometry = streaming.catalog.geometry();
                merge_instances(&instances, &geometry)
                    .map(|merged| (merged, instances.len()))
                    .map_err(|reason| (LodChunkFailureClass::Terminal, reason))
            });
        match built {
            Ok((merged, instance_count)) => {
                let root = spawn_block(
                    &mut commands,
                    &mut meshes,
                    &streaming,
                    origin.0,
                    block,
                    merged,
                );
                streaming
                    .blocks
                    .insert(block, TreeBlockStatus::Ready { root });
                metrics.lod_tree_blocks_ready = metrics.lod_tree_blocks_ready.saturating_add(1);
                metrics.lod_tree_instances = metrics
                    .lod_tree_instances
                    .saturating_add(instance_count as u64);
                profiler.increment("lod/tree_blocks_ready", 1);
                profiler.increment("lod/tree_instances", instance_count as u64);
            }
            Err((class, reason)) => {
                let retry_at = lod_retry_deadline(class, retry_count, Instant::now());
                warn!(?block, ?class, %reason, retrying = retry_at.is_some(), "distant tree block failed");
                streaming.blocks.insert(
                    block,
                    TreeBlockStatus::Failed {
                        retry_count,
                        retry_at,
                    },
                );
                metrics.lod_tree_blocks_failed = metrics.lod_tree_blocks_failed.saturating_add(1);
                profiler.increment("lod/tree_blocks_failed", 1);
            }
        }
    }
    update_counts(&streaming, &mut metrics);
    profiler.record_elapsed("lod/trees_collect", started);
}

/// Spawns a block root and one child per merged `(cell, atlas)` mesh.
fn spawn_block(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    streaming: &TreeLodStreaming,
    origin: IVec2,
    block: IVec2,
    merged: BTreeMap<(i32, i32, usize), MergedCell>,
) -> Entity {
    let (ring_layers, ring_propagation) = lod_ring_layers();
    let root = commands
        .spawn((
            Name::new(format!("LOD trees block {},{}", block.x, block.y)),
            TreeLodBlockRoot,
            Transform::IDENTITY,
            if streaming.demand.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            ring_layers,
            ring_propagation,
        ))
        .id();
    for ((cell_x, cell_y, group), cell_mesh) in merged {
        let cell = IVec2::new(cell_x, cell_y);
        let Some((_, material)) = streaming.catalog.groups.get(group) else {
            continue;
        };
        let (mesh, aabb) = cell_mesh.into_mesh();
        let visibility = Visibility::Inherited;
        let child = commands
            .spawn((
                Name::new(format!("LOD trees cell {cell_x},{cell_y}")),
                super::native_lod::NativeLodCell {
                    world: streaming.catalog.worldspace_id.expect("catalog world"),
                    cell,
                },
                LodChunkGridOrigin {
                    grid_x: i64::from(cell_x),
                    grid_y: i64::from(cell_y),
                },
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                aabb,
                Transform::from_xyz(
                    (cell_x - origin.x) as f32 * CELL_SIZE,
                    0.0,
                    -((cell_y - origin.y) as f32) * CELL_SIZE,
                ),
                visibility,
                NotShadowCaster,
            ))
            .id();
        commands.entity(root).add_child(child);
    }
    root
}

fn update_counts(streaming: &TreeLodStreaming, metrics: &mut StreamingMetrics) {
    metrics.resident_lod_tree_blocks = streaming
        .blocks
        .values()
        .filter(|status| matches!(status, TreeBlockStatus::Ready { .. }))
        .count();
    metrics.pending_lod_tree_blocks = streaming
        .blocks
        .values()
        .filter(|status| {
            matches!(
                status,
                TreeBlockStatus::Requested { .. }
                    | TreeBlockStatus::Failed {
                        retry_at: Some(_),
                        ..
                    }
            )
        })
        .count()
        + streaming.queue.len()
        + usize::from(streaming.catalog.worldspace_id.is_some() && !streaming.catalog.settled);
    metrics.unrecovered_lod_tree_blocks = streaming
        .blocks
        .iter()
        .filter(|(block, status)| {
            streaming.wanted.contains(block)
                && matches!(status, TreeBlockStatus::Failed { retry_at: None, .. })
        })
        .count();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_tree_types_hold_only_blocks_that_demand_them() {
        let mut streaming = TreeLodStreaming::default();
        streaming.catalog.settled = true;
        streaming.catalog.types.insert(
            1,
            TypeSlot {
                mesh: Handle::default(),
                material: Handle::default(),
                state: TypeState::Failed,
            },
        );
        assert!(streaming.catalog.usable());
        let mut metrics = StreamingMetrics::default();
        update_counts(&streaming, &mut metrics);
        assert_eq!(metrics.unrecovered_lod_tree_blocks, 0);
        assert!(
            merge_instances(&[tree([0.0; 3], 0.0, 1.0)], &streaming.catalog.geometry()).is_err()
        );
        streaming.blocks.insert(
            IVec2::ZERO,
            TreeBlockStatus::Failed {
                retry_count: 0,
                retry_at: None,
            },
        );
        update_counts(&streaming, &mut metrics);
        assert_eq!(metrics.unrecovered_lod_tree_blocks, 0);
        streaming.wanted.insert(IVec2::ZERO);
        update_counts(&streaming, &mut metrics);
        assert_eq!(metrics.unrecovered_lod_tree_blocks, 1);
    }

    fn quad_pair() -> TypeGeometry {
        // The converter's shape: two crossed quads, 8 vertices, 12 indices, base at the origin.
        let positions = vec![
            [-50.0, 0.0, 0.0],
            [50.0, 0.0, 0.0],
            [50.0, 200.0, 0.0],
            [-50.0, 200.0, 0.0],
            [0.0, 0.0, -50.0],
            [0.0, 0.0, 50.0],
            [0.0, 200.0, 50.0],
            [0.0, 200.0, -50.0],
        ];
        TypeGeometry {
            normals: vec![[0.0, 0.0, 1.0]; 8],
            uvs: vec![[0.25, 0.5]; 8],
            positions,
            indices: vec![0, 2, 1, 0, 3, 2, 4, 6, 5, 4, 7, 6],
        }
    }

    fn tree(position: [f32; 3], rotation: f32, scale: f32) -> TreeInstance {
        TreeInstance {
            tree_index: 0,
            position,
            rotation,
            scale,
        }
    }

    #[test]
    fn an_instance_lands_relative_to_its_cell_corner_in_runtime_space() {
        let geometry = BTreeMap::from([(0, (0, quad_pair()))]);
        // Cell (-8, 3): corner (-32768, 12288). The tree stands 100 units east and 200 north of
        // the corner, 50 up.
        let position = [-32768.0 + 100.0, 12288.0 + 200.0, 50.0];
        let merged = merge_instances(&[tree(position, 0.0, 1.0)], &geometry).unwrap();
        assert_eq!(merged.len(), 1);
        let cell = &merged[&(-8, 3, 0)];
        assert_eq!(cell.vertex_count(), 8);
        // Creation (100, 200, 50) is runtime (100, 50, -200); the quad's first vertex sits half
        // its width west of the base.
        let first = cell.positions[0];
        assert!((first[0] - 50.0).abs() < 1e-3, "{first:?}");
        assert!((first[1] - 50.0).abs() < 1e-3, "{first:?}");
        assert!((first[2] + 200.0).abs() < 1e-3, "{first:?}");
        assert_eq!(cell.indices.len(), 12);
        assert!(cell.min.x <= cell.max.x && cell.min.y <= cell.max.y);
    }

    #[test]
    fn instances_in_two_cells_make_two_meshes_and_indices_stay_inside_each() {
        let geometry = BTreeMap::from([(0, (0, quad_pair()))]);
        let merged = merge_instances(
            &[
                tree([100.0, 100.0, 0.0], 0.0, 1.0),
                tree([200.0, 100.0, 0.0], 1.0, 2.0),
                tree([4096.0 + 10.0, 100.0, 0.0], 0.0, 1.0),
            ],
            &geometry,
        )
        .unwrap();
        assert_eq!(merged.len(), 2);
        let first = &merged[&(0, 0, 0)];
        assert_eq!(first.vertex_count(), 16);
        assert!(
            first
                .indices
                .iter()
                .all(|index| (*index as usize) < first.vertex_count())
        );
        assert_eq!(
            first.indices[12], 8,
            "the second instance's indices are offset"
        );
        assert_eq!(merged[&(1, 0, 0)].vertex_count(), 8);
    }

    #[test]
    fn scale_and_yaw_move_the_vertices() {
        let geometry = BTreeMap::from([(0, (0, quad_pair()))]);
        let plain = merge_instances(&[tree([0.0, 0.0, 0.0], 0.0, 1.0)], &geometry).unwrap();
        let big = merge_instances(&[tree([0.0, 0.0, 0.0], 0.0, 2.0)], &geometry).unwrap();
        let turned = merge_instances(
            &[tree([0.0, 0.0, 0.0], std::f32::consts::FRAC_PI_2, 1.0)],
            &geometry,
        )
        .unwrap();
        let (plain, big, turned) = (&plain[&(0, 0, 0)], &big[&(0, 0, 0)], &turned[&(0, 0, 0)]);
        assert!(
            (big.max.y - 2.0 * plain.max.y).abs() < 1e-3,
            "scale doubles the height"
        );
        // A quarter turn about the vertical swaps the horizontal extents of the first quad.
        assert!((plain.max.x - turned.max.x).abs() < 1e-3 || plain.max.z != turned.max.z);
        assert!(
            (turned.max.y - plain.max.y).abs() < 1e-3,
            "yaw leaves the height alone"
        );
    }

    #[test]
    fn a_type_the_catalog_does_not_have_fails_the_block() {
        let geometry = BTreeMap::from([(0, (0, quad_pair()))]);
        let mut stray = tree([0.0, 0.0, 0.0], 0.0, 1.0);
        stray.tree_index = 7;
        let error =
            merge_instances(&[tree([0.0, 0.0, 0.0], 0.0, 1.0), stray], &geometry).unwrap_err();
        assert!(error.contains("tree type 7"), "{error}");
    }

    #[test]
    fn instance_cells_use_floor_division() {
        assert_eq!(instance_cell([-1.0, 0.0, 0.0]), IVec2::new(-1, 0));
        assert_eq!(instance_cell([4095.9, -4096.0, 0.0]), IVec2::new(0, -1));
        assert_eq!(instance_cell([-4096.1, 4096.0, 0.0]), IVec2::new(-2, 1));
    }

    #[test]
    fn type_meshes_must_be_plain_indexed_triangle_lists() {
        let shape = quad_pair();
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, shape.positions.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, shape.normals.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, shape.uvs.clone());
        mesh.insert_indices(Indices::U16(
            shape.indices.iter().map(|i| *i as u16).collect(),
        ));
        assert_eq!(TypeGeometry::from_mesh(&mesh).unwrap(), shape);

        let mut out_of_range = mesh.clone();
        out_of_range.insert_indices(Indices::U16(vec![0, 1, 99]));
        assert!(TypeGeometry::from_mesh(&out_of_range).is_err());

        let mut no_uvs = mesh.clone();
        no_uvs.remove_attribute(Mesh::ATTRIBUTE_UV_0);
        assert!(TypeGeometry::from_mesh(&no_uvs).is_err());

        let mut lines = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
        lines.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; 2]);
        assert!(TypeGeometry::from_mesh(&lines).is_err());
    }
}
