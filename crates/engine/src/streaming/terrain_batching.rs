//! Runtime terrain submission batches. Source GLBs and their quadrant contract stay unchanged.
//!
//! Vertex streams are concatenated without welding or transforming them. A batch's index buffer
//! contains precisely the quadrants selected by the ordinary LOD handoff. The two-cell tile bounds
//! limit the loss of culling granularity; batches never cross a chunk, material, or render state.

use super::{TerrainCoverage, TerrainSurfaceReady};
use crate::terrain_upload::TerrainMeshUploadReadiness;
use crate::visibility_optimization::StructuralHierarchyNode;
use bevy::ecs::change_detection::Tick;
use bevy::{
    animation::AnimatedBy,
    app::{Inherited, Propagate},
    asset::RenderAssetUsages,
    camera::{
        primitives::{Aabb, Frustum, Sphere},
        visibility::{
            NoAutoAabb, NoCpuCulling, NoFrustumCulling, RenderLayers, VisibilityClass,
            VisibilityRange,
        },
    },
    ecs::{lifecycle::HookContext, system::SystemParam, world::DeferredWorld},
    gizmos::aabb::ShowAabbGizmo,
    light::{LightProbe, NotShadowCaster, NotShadowReceiver, RectLight, TransmittedShadowReceiver},
    mesh::{
        Indices, MeshTag,
        morph::{MeshMorphWeights, MorphWeights},
        skinning::SkinnedMesh,
    },
    pbr::Lightmap,
    picking::Pickable,
    prelude::*,
    render::batching::NoAutomaticBatching,
    world_serialization::{WorldAsset, WorldAssetRoot, WorldInstance},
};
use bevy_rapier3d::prelude::{Collider, RigidBody};
use shared::lod::{LodTier, nodes};
use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    sync::Arc,
};

const BATCH_SIDE_CELLS: i32 = 2;

/// A render mesh representing at most sixteen source quadrants in a two-by-two-cell tile.
#[derive(Component)]
pub(crate) struct LodTerrainBatch;

#[derive(Component)]
pub(crate) struct PendingLodTerrainBatching {
    pub(super) source: Handle<WorldAsset>,
    pub(super) patches: Vec<(Entity, TerrainCoverage)>,
}

#[derive(Component)]
#[component(on_remove = remove_generated_meshes)]
pub(crate) struct LodTerrainBatches {
    batches: Vec<TerrainBatch>,
    source_children: Option<Vec<Entity>>,
    // Removing WorldAssetRoot unregisters the original instance before its hierarchy is flattened.
    // Retain its handle so batching never mutates or unloads the source scene's shared mesh assets.
    _source: Handle<WorldAsset>,
}

fn remove_generated_meshes(mut world: DeferredWorld, context: HookContext) {
    let ids: Vec<_> = world
        .get::<LodTerrainBatches>(context.entity)
        .into_iter()
        .flat_map(|batches| batches.batches.iter().map(|batch| batch.mesh.id()))
        .collect();
    let source_ids: Vec<_> = world
        .get::<LodTerrainBatches>(context.entity)
        .into_iter()
        .flat_map(|chunk| {
            chunk
                .batches
                .iter()
                .flat_map(|batch| batch.members.iter().map(|member| member.source.id()))
        })
        .collect();
    if let Some(mut meshes) = world.get_resource_mut::<Assets<Mesh>>() {
        for &id in &ids {
            meshes.remove(id);
        }
    }
    if let Some(readiness) = world.get_resource::<TerrainMeshUploadReadiness>() {
        for id in ids {
            readiness.forget(id);
        }
        for id in source_ids {
            readiness.forget(id);
        }
    }
}

struct TerrainBatch {
    entity: Entity,
    mesh: Handle<Mesh>,
    members: Vec<BatchMember>,
    selected: u16,
    state: RenderState,
    pending_upload: bool,
    fallback_entities: Vec<Entity>,
    fallback_selected: u16,
}

struct BatchMember {
    source_patch: Entity,
    coverage: TerrainCoverage,
    indices: Vec<u32>,
    bounds: Aabb,
    source: Handle<Mesh>,
}

#[derive(Clone, PartialEq)]
struct RenderState {
    material: Handle<StandardMaterial>,
    transform: Transform,
    layers: Option<RenderLayers>,
    tag: Option<u32>,
    not_shadow_caster: bool,
    not_shadow_receiver: bool,
    transmitted_shadow_receiver: bool,
    no_frustum_culling: bool,
}

pub(super) struct PreparedBatch {
    tile: IVec2,
    state: RenderState,
    mesh: Mesh,
    members: Vec<BatchMember>,
    selected: u16,
}

type PrimitiveRenderState = (
    &'static MeshMaterial3d<StandardMaterial>,
    Option<&'static RenderLayers>,
    Option<&'static MeshTag>,
    Has<NotShadowCaster>,
    Has<NotShadowReceiver>,
    Has<TransmittedShadowReceiver>,
    Has<NoFrustumCulling>,
);

type PrimitiveCullingState = (
    Has<NoCpuCulling>,
    Has<StructuralHierarchyNode>,
    Option<&'static VisibilityClass>,
    Has<Aabb>,
);

type UnsupportedNode = Or<(
    Or<(
        With<SkinnedMesh>,
        With<MeshMorphWeights>,
        With<MorphWeights>,
        With<AnimationPlayer>,
        With<AnimatedBy>,
        With<Lightmap>,
        With<VisibilityRange>,
        With<NoAutomaticBatching>,
        With<Camera>,
        With<PointLight>,
        With<SpotLight>,
        With<DirectionalLight>,
    )>,
    Or<(
        With<RectLight>,
        With<LightProbe>,
        With<Collider>,
        With<RigidBody>,
        With<Pickable>,
        With<ShowAabbGizmo>,
        With<Frustum>,
        With<Sphere>,
        With<Mesh2d>,
    )>,
)>;

type StructuralLayers<'w, 's> = Query<
    'w,
    's,
    (
        &'static RenderLayers,
        Option<&'static Inherited<RenderLayers>>,
        Has<Propagate<RenderLayers>>,
    ),
>;

#[derive(SystemParam)]
pub(crate) struct LodBatchPreparation<'w, 's> {
    transforms: Query<'w, 's, (&'static Transform, Option<&'static Visibility>)>,
    layers: StructuralLayers<'w, 's>,
    render_states: Query<'w, 's, PrimitiveRenderState>,
    unsupported: Query<'w, 's, Entity, UnsupportedNode>,
    culling: Query<'w, 's, PrimitiveCullingState>,
    materials: Res<'w, Assets<StandardMaterial>>,
    readiness: Option<Res<'w, TerrainMeshUploadReadiness>>,
}

impl LodBatchPreparation<'_, '_> {
    pub(super) fn upload_readiness(&self) -> Option<&TerrainMeshUploadReadiness> {
        self.readiness.as_deref()
    }

    pub(super) fn has_render_backend(&self) -> bool {
        self.readiness
            .as_deref()
            .is_some_and(TerrainMeshUploadReadiness::has_render_backend)
    }
    /// Refuse the whole conversion before changing anything when a scene has extra content or
    /// behavior that cannot be represented by the terrain-only submission contract.
    pub(super) fn prepare(
        &self,
        root: Entity,
        patches: &[(Entity, TerrainCoverage)],
        children: &Query<&Children>,
        names: &Query<&Name>,
        mesh_handles: &Query<&Mesh3d>,
        meshes: &Assets<Mesh>,
    ) -> Result<Vec<PreparedBatch>, String> {
        let mut primitives = HashMap::new();
        let patch_entities: HashSet<_> = patches.iter().map(|(entity, _)| *entity).collect();
        for &(patch, coverage) in patches {
            let primitive = children
                .get(patch)
                .ok()
                .and_then(|children| children.iter().find(|child| mesh_handles.contains(*child)))
                .ok_or("terrain primitive disappeared before batching")?;
            let selected = self
                .transforms
                .get(patch)
                .ok()
                .and_then(|(_, visibility)| visibility)
                .is_some_and(|visibility| *visibility == Visibility::Inherited);
            primitives.insert(primitive, (patch, coverage, selected));
        }

        let mut primitive_transforms = HashMap::new();
        let mut stack: Vec<(Entity, Transform, bool)> = children
            .get(root)
            .map(|children| {
                children
                    .iter()
                    .map(|child| (child, Transform::IDENTITY, true))
                    .collect()
            })
            .unwrap_or_default();
        while let Some((entity, parent_transform, scene_root)) = stack.pop() {
            if self.unsupported.contains(entity) {
                return Err(
                    "scene contains animation, skinning, lightmaps, or other render behavior"
                        .into(),
                );
            }
            let (transform, visibility) = self
                .transforms
                .get(entity)
                .map_err(|_| "scene node has no local transform")?;
            if !patch_entities.contains(&entity)
                && visibility.is_some_and(|visibility| *visibility != Visibility::Inherited)
            {
                return Err("scene node has explicit visibility".into());
            }
            // The compiler places all geometry in chunk-local coordinates and gives only its
            // scene root an axis transform. Keep that exact transform rather than baking it into
            // vertices or decomposing a composed matrix that might contain shear.
            let primitive = primitives.contains_key(&entity);
            let name = names.get(entity).ok().map(Name::as_str);
            let chunk_wrapper = !primitive && name.is_some_and(|name| name.starts_with("chunk_"));
            if *transform != Transform::IDENTITY
                && (!chunk_wrapper || parent_transform != Transform::IDENTITY)
            {
                return Err("terrain descendants have additional local transforms".into());
            }
            let combined = if *transform != Transform::IDENTITY {
                *transform
            } else {
                parent_transform
            };
            let (no_cpu, owned_structural, class, bounds) = self
                .culling
                .get(entity)
                .map_err(|_| "terrain culling state disappeared")?;
            if (no_cpu && (primitive || !owned_structural))
                || (primitive
                    && class.is_some_and(|class| {
                        class.0.iter().any(|id| *id != TypeId::of::<Mesh3d>())
                    }))
                || (!primitive && (class.is_some() || bounds))
            {
                return Err(
                    "scene contains explicit culling or additional visibility participants".into(),
                );
            }
            if primitive {
                if children
                    .get(entity)
                    .is_ok_and(|children| !children.is_empty())
                {
                    return Err("terrain primitive has extra descendants".into());
                }
                primitive_transforms.insert(entity, combined);
            } else {
                let name = names
                    .get(entity)
                    .map(Name::as_str)
                    .map_err(|_| "scene contains an unnamed non-terrain node")?;
                // RenderLayers inherited from the LOD root's engine-owned propagation are
                // routing state, not extra authored scene behavior. Preserve refusal for a
                // local source or an unproven/different layer value on structural nodes.
                let owns_layers =
                    self.layers
                        .get(entity)
                        .is_ok_and(|(layers, inherited, source)| {
                            source || inherited.is_none_or(|inherited| inherited.0 != *layers)
                        });
                if owns_layers
                    || mesh_handles.contains(entity)
                    || !(name.starts_with("chunk_")
                        || name.starts_with("cell_")
                        || name == nodes::terrain_group()
                        || name.starts_with("terrain_quadrant_"))
                {
                    return Err(format!("scene contains non-terrain node {name:?}"));
                }
                if scene_root && !chunk_wrapper {
                    return Err("scene root does not match the compiler terrain hierarchy".into());
                }
                if let Ok(children) = children.get(entity) {
                    stack.extend(children.iter().map(|child| (child, combined, false)));
                }
            }
        }
        if primitive_transforms.len() != primitives.len() {
            return Err("terrain primitives lie outside their chunk hierarchy".into());
        }

        let mut ordered: Vec<_> = primitives.into_iter().collect();
        ordered.sort_by_key(|(_, (_, coverage, _))| {
            (coverage.grid.x, coverage.grid.y, coverage.quadrant)
        });
        let mut batches: Vec<PreparedBatch> = Vec::new();
        let mut tiles = HashMap::<IVec2, Vec<usize>>::new();
        for (primitive, (source_patch, coverage, selected)) in ordered {
            let (
                material,
                layers,
                tag,
                not_shadow_caster,
                not_shadow_receiver,
                transmitted_shadow_receiver,
                no_frustum_culling,
            ) = self
                .render_states
                .get(primitive)
                .map_err(|_| "terrain primitive lacks a standard material")?;
            if self
                .materials
                .get(&material.0)
                .is_none_or(|material| material.alpha_mode != AlphaMode::Opaque)
            {
                return Err("terrain material is missing or requires transparency ordering".into());
            }
            let source = meshes
                .get(
                    &mesh_handles
                        .get(primitive)
                        .map_err(|_| "terrain mesh disappeared")?
                        .0,
                )
                .ok_or("terrain mesh asset disappeared")?;
            if source
                .try_has_morph_targets()
                .map_err(|error| error.to_string())?
                || source.skinned_mesh_bounds().is_some()
                || source.contains_attribute(Mesh::ATTRIBUTE_JOINT_INDEX)
                || source.contains_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)
            {
                return Err("terrain mesh has morph or skin data".into());
            }
            let state = RenderState {
                material: material.0.clone(),
                transform: primitive_transforms[&primitive],
                layers: layers.cloned(),
                tag: tag.map(|tag| tag.0),
                not_shadow_caster,
                not_shadow_receiver,
                transmitted_shadow_receiver,
                no_frustum_culling,
            };
            let tile = batch_tile(coverage.grid);
            let existing = tiles.get(&tile).and_then(|candidates| {
                candidates.iter().copied().find(|&index| {
                    batches[index].state == state
                        && compatible_vertex_streams(&batches[index].mesh, source)
                })
            });
            let index = match existing {
                Some(index) => index,
                None => {
                    let mut mesh = source.clone();
                    mesh.asset_usage =
                        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;
                    mesh.insert_indices(Indices::U32(Vec::new()));
                    let index = batches.len();
                    batches.push(PreparedBatch {
                        tile,
                        state,
                        mesh,
                        members: Vec::new(),
                        selected: 0,
                    });
                    tiles.entry(tile).or_default().push(index);
                    index
                }
            };
            let batch = &mut batches[index];
            // The first source's vertices are already in the cloned mesh. Subsequent sources append
            // every attribute through Mesh::merge; compatibility was checked in both directions.
            let offset = if batch.members.is_empty() {
                0
            } else {
                batch.mesh.count_vertices() as u32
            };
            let indices = source
                .indices()
                .ok_or("terrain source is not indexed")?
                .iter()
                .map(|index| index as u32 + offset)
                .collect();
            let bounds = match source.attribute(Mesh::ATTRIBUTE_POSITION) {
                Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) => {
                    Aabb::enclosing(positions.iter().copied().map(Vec3::from_array))
                        .ok_or("terrain source has no positions")?
                }
                _ => return Err("terrain source has no Float32x3 positions".into()),
            };
            if !batch.members.is_empty() {
                batch
                    .mesh
                    .merge(source)
                    .map_err(|error| error.to_string())?;
            }
            if batch.members.len() == 16 {
                return Err("terrain tile covers more than sixteen quadrants".into());
            }
            if selected {
                batch.selected |= 1 << batch.members.len();
            }
            batch.members.push(BatchMember {
                source_patch,
                coverage,
                indices,
                bounds,
                source: mesh_handles
                    .get(primitive)
                    .map_err(|_| "terrain source disappeared")?
                    .0
                    .clone(),
            });
        }
        // Copy the selector's current source masks without rebuilding the tier map. Newly ready
        // or concurrently changed coverage still takes the full selector path later in this frame,
        // before initial activation is allowed to discard the original hierarchy.
        for batch in &mut batches {
            let (indices, bounds) = selected_geometry(&batch.members, batch.selected);
            batch.mesh.insert_indices(Indices::U32(indices));
            batch.mesh.final_aabb = bounds.map(Into::into);
        }
        Ok(batches)
    }
}

fn batch_tile(grid: IVec2) -> IVec2 {
    IVec2::new(
        grid.x.div_euclid(BATCH_SIDE_CELLS),
        grid.y.div_euclid(BATCH_SIDE_CELLS),
    )
}

fn compatible_vertex_streams(left: &Mesh, right: &Mesh) -> bool {
    left.primitive_topology() == right.primitive_topology()
        && left.enable_raytracing == right.enable_raytracing
        && left.attributes().count() == right.attributes().count()
        && left.attributes().zip(right.attributes()).all(
            |((left_attribute, left_values), (right_attribute, right_values))| {
                left_attribute.id == right_attribute.id
                    && left_attribute.format == right_attribute.format
                    && std::mem::discriminant(left_values) == std::mem::discriminant(right_values)
            },
        )
}

pub(super) fn install(
    root: Entity,
    source: Handle<WorldAsset>,
    prepared: Vec<PreparedBatch>,
    children: &Query<&Children>,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    readiness: Option<&TerrainMeshUploadReadiness>,
) {
    let source_children = children
        .get(root)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    let batches = prepared
        .into_iter()
        .map(|prepared| {
            let mesh = meshes.add(prepared.mesh);
            if let Some(readiness) = readiness {
                // Spread immutable source proof requests over the bounded CPU commits rather
                // than queuing all arrived roots during the first initial-activation poll.
                for member in &prepared.members {
                    readiness.request(member.source.id());
                }
                if prepared.selected != 0 {
                    readiness.request(mesh.id());
                }
            }
            let (_, bounds) = selected_geometry(&prepared.members, prepared.selected);
            let entity = spawn_render_mesh(
                commands,
                root,
                mesh.clone(),
                &prepared.state,
                Visibility::Hidden,
                bounds,
            );
            commands.entity(entity).insert((
                LodTerrainBatch,
                Name::new(format!(
                    "LOD terrain batch {},{}",
                    prepared.tile.x, prepared.tile.y
                )),
            ));
            TerrainBatch {
                entity,
                mesh,
                members: prepared.members,
                selected: prepared.selected,
                state: prepared.state,
                pending_upload: prepared.selected != 0,
                fallback_entities: Vec::new(),
                fallback_selected: 0,
            }
        })
        .collect();
    commands.entity(root).insert((
        LodTerrainBatches {
            batches,
            source_children: Some(source_children),
            _source: source,
        },
        TerrainSurfaceReady,
    ));
}

fn spawn_render_mesh(
    commands: &mut Commands,
    root: Entity,
    mesh: Handle<Mesh>,
    state: &RenderState,
    visibility: Visibility,
    bounds: Option<Aabb>,
) -> Entity {
    let mut entity = commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(state.material.clone()),
        crate::swap_bridge::SwapSurface::new(state.material.id().untyped(), []),
        crate::swap_bridge::SwapWaiting,
        state.transform,
        visibility,
        ChildOf(root),
    ));
    // Opting out of automatic bounds is only valid together with explicit bounds: a hidden
    // empty selection has none yet, so Bevy keeps computing them until activation supplies the
    // exact selected box.
    if let Some(bounds) = bounds {
        entity.insert((NoAutoAabb, bounds));
    }
    if let Some(layers) = state.layers.clone() {
        entity.insert(layers);
    }
    if let Some(tag) = state.tag {
        entity.insert(MeshTag(tag));
    }
    if state.not_shadow_caster {
        entity.insert(NotShadowCaster);
    }
    if state.not_shadow_receiver {
        entity.insert(NotShadowReceiver);
    }
    if state.transmitted_shadow_receiver {
        entity.insert(TransmittedShadowReceiver);
    }
    if state.no_frustum_culling {
        entity.insert(NoFrustumCulling);
    }
    entity.id()
}

/// Root-level pending count used by the CPU preparation queue.
#[derive(SystemParam)]
pub(crate) struct LodBatchActivation<'w, 's> {
    chunks: Query<'w, 's, (Entity, &'static LodTerrainBatches)>,
    roots: Query<'w, 's, &'static super::LodChunkRoot>,
    views: Query<'w, 's, &'static ViewVisibility>,
    readiness: Option<Res<'w, TerrainMeshUploadReadiness>>,
    config: Option<Res<'w, crate::config::EngineConfig>>,
    cluster_terrain: Option<Res<'w, crate::cluster::terrain::ClusterTerrain>>,
    fallback: Local<'s, std::cell::RefCell<FallbackCellsCache>>,
}

impl LodBatchActivation<'_, '_> {
    /// Chunks the world-ready check waits for (see [`still_loading`]).
    pub(super) fn pending_count(&self) -> usize {
        pending_chunks(
            self.chunks.iter(),
            &self.roots,
            &self.views,
            self.readiness.as_deref(),
            || cached_fallback_cells(&self.fallback, &self.config, &self.cluster_terrain),
        )
    }
}

/// The cluster fallback cells, kept until the config or the cluster terrain changes: rebuilding
/// the set (every cell of every registered block) on each call cost milliseconds per frame while
/// a chunk waited for its hand-over.
#[derive(Default)]
struct FallbackCellsCache {
    key: Option<(Option<Tick>, Option<Tick>)>,
    cells: Arc<HashSet<(u32, IVec2)>>,
}

fn cached_fallback_cells(
    cache: &std::cell::RefCell<FallbackCellsCache>,
    config: &Option<Res<crate::config::EngineConfig>>,
    cluster_terrain: &Option<Res<crate::cluster::terrain::ClusterTerrain>>,
) -> Arc<HashSet<(u32, IVec2)>> {
    let key = (
        config.as_ref().map(|config| config.last_changed()),
        cluster_terrain
            .as_ref()
            .map(|terrain| terrain.last_changed()),
    );
    let mut cache = cache.borrow_mut();
    if cache.key != Some(key) {
        crate::work_counters::record("terrain_batching/fallback_cells");
        cache.cells = Arc::new(cluster_fallback_cells(
            config.as_deref(),
            cluster_terrain.as_deref(),
        ));
        cache.key = Some(key);
    }
    Arc::clone(&cache.cells)
}

/// How many chunks are [`still_loading`]. A settled scene holds no source hierarchy, so the
/// fallback cells are gathered only while one is held.
fn pending_chunks<'a>(
    chunks: impl Iterator<Item = (Entity, &'a LodTerrainBatches)> + Clone,
    roots: &Query<&super::LodChunkRoot>,
    views: &Query<&ViewVisibility>,
    readiness: Option<&TerrainMeshUploadReadiness>,
    fallback_cells: impl FnOnce() -> Arc<HashSet<(u32, IVec2)>>,
) -> usize {
    if !chunks
        .clone()
        .any(|(_, chunk)| chunk.source_children.is_some())
    {
        return 0;
    }
    let fallback_cells = fallback_cells();
    chunks
        .filter(|(entity, chunk)| {
            let world = roots.get(*entity).ok().map(|root| root.key.worldspace_id);
            still_loading(chunk, world, &fallback_cells, readiness, |entity| {
                views.get(entity).is_ok_and(|view| view.get())
            })
        })
        .count()
}

/// Whether a chunk still holding its source hierarchy is loading work the world-ready check waits
/// for. It is, until the only thing left is a camera drawing one of its cluster-owned batches
/// that some view sees: a chunk seen only by a shadow view keeps its source until the camera
/// turns to it, which can take as long as the camera stays still, so that wait is settled. Every
/// other step (uploads, source proofs, the hand-over of a batch no view sees) finishes on its own
/// and counts as loading; with cluster terrain off no batch is owned, so every chunk counts until
/// it hands over.
fn still_loading(
    chunk: &LodTerrainBatches,
    world: Option<u32>,
    fallback_cells: &HashSet<(u32, IVec2)>,
    readiness: Option<&TerrainMeshUploadReadiness>,
    viewed: impl Fn(Entity) -> bool,
) -> bool {
    if chunk.source_children.is_none() {
        return false;
    }
    let Some(readiness) = readiness else {
        return true;
    };
    let waits_only_for_draw = chunk.batches.iter().any(|batch| {
        owned_source_mask(batch, world, fallback_cells) != 0
            && !batch.pending_upload
            && viewed(batch.entity)
    }) && chunk.batches.iter().all(|batch| {
        batch.selected == 0 || !batch.pending_upload || readiness.is_ready(batch.mesh.id())
    }) && chunk
        .batches
        .iter()
        .flat_map(|batch| &batch.members)
        .all(|member| readiness.is_ready(member.source.id()));
    !waits_only_for_draw
}

/// The cells cluster terrain can draw in place of LOD terrain: a batch member in one is owned and
/// its source retires only after the batch is drawn or proven unviewed.
fn cluster_fallback_cells(
    config: Option<&crate::config::EngineConfig>,
    cluster_terrain: Option<&crate::cluster::terrain::ClusterTerrain>,
) -> HashSet<(u32, IVec2)> {
    if !config.is_some_and(|config| config.cluster_terrain && config.clusters.is_some()) {
        return HashSet::new();
    }
    cluster_terrain
        .iter()
        .flat_map(|terrain| &terrain.blocks)
        // Registration means this block can participate in cluster terrain selection.
        // Pending package inventory alone owns no draw and adds no retirement obligation.
        .filter(|block| block.batch.is_some())
        .flat_map(|block| {
            block
                .cells
                .iter()
                .map(move |grid| (block.worldspace_id, *grid))
        })
        .collect()
}

/// Counts of chunks by the first reason their source hierarchy cannot retire.
#[derive(Default, Debug)]
struct RetireBlockers {
    chunks: usize,
    owned: usize,
    root_not_ready: usize,
    source_patch: usize,
    source_mesh_upload: usize,
    unowned_batch_upload: usize,
    owned_batch_upload: usize,
    owned_undrawn_batch_viewed: usize,
    owned_undrawn_region_viewed: usize,
    owned_undrawn_region_observed: usize,
    owned_undrawn_not_prepared: usize,
    other: usize,
    /// The batches counted in `owned_undrawn_batch_viewed`, with the mesh they must draw, for the
    /// per-batch probe in the stall log.
    viewed_undrawn: Vec<(Entity, AssetId<Mesh>)>,
    /// The first few batches counted in `owned_undrawn_not_prepared`, with their mesh and material.
    not_prepared: Vec<(Entity, AssetId<Mesh>, bevy::asset::UntypedAssetId)>,
}

#[derive(SystemParam)]
pub(crate) struct LodBatchVisibility<'w, 's> {
    views: Query<'w, 's, &'static ViewVisibility>,
    config: Option<Res<'w, crate::config::EngineConfig>>,
    cluster_terrain: Option<Res<'w, crate::cluster::terrain::ClusterTerrain>>,
    chunks: Query<
        'w,
        's,
        (
            Entity,
            &'static mut LodTerrainBatches,
            Option<&'static TerrainSurfaceReady>,
        ),
    >,
    removed: RemovedComponents<'w, 's, LodTerrainBatches>,
    roots: Query<'w, 's, &'static super::LodChunkRoot>,
    drawn: Query<'w, 's, &'static crate::swap_bridge::SwapDrawReceipt>,
    prepared: Query<
        'w,
        's,
        (&'static Mesh3d, &'static crate::swap_bridge::SwapSurface),
        With<crate::swap_bridge::SwapPrepared>,
    >,
    coverage_requests: Query<'w, 's, (), With<crate::swap_bridge::SwapCoverageRoot>>,
    coverage_views: Query<'w, 's, &'static crate::swap_bridge::SwapCoverageInCameraView>,
    source_states: Query<'w, 's, (&'static TerrainCoverage, Has<TerrainSurfaceReady>)>,
    meshes: Option<ResMut<'w, Assets<Mesh>>>,
    commands: Commands<'w, 's>,
    readiness: Option<Res<'w, TerrainMeshUploadReadiness>>,
    fallback: Local<'s, std::cell::RefCell<FallbackCellsCache>>,
}

impl LodBatchVisibility<'_, '_> {
    fn cluster_fallback_cells(&self) -> Arc<HashSet<(u32, IVec2)>> {
        cached_fallback_cells(&self.fallback, &self.config, &self.cluster_terrain)
    }

    /// Why each chunk still holding its source hierarchy cannot retire, by first failing
    /// condition. Mirrors the retirement candidate test; read-only, for the stall log.
    fn retire_blockers(
        &self,
        current: &crate::swap_bridge::CurrentCoverageVisibility,
    ) -> RetireBlockers {
        let fallback_cells = self.cluster_fallback_cells();
        let Some(readiness) = self.readiness.as_deref() else {
            return RetireBlockers::default();
        };
        let mut out = RetireBlockers::default();
        for (entity, chunk, ready) in self.chunks.iter() {
            if chunk.source_children.is_none() {
                continue;
            }
            out.chunks += 1;
            let world = self
                .roots
                .get(entity)
                .ok()
                .map(|root| root.key.worldspace_id);
            if ready.is_none() {
                out.root_not_ready += 1;
                continue;
            }
            if chunk
                .batches
                .iter()
                .any(|batch| owned_source_mask(batch, world, &fallback_cells) != 0)
            {
                out.owned += 1;
            }
            let mut sources = 0;
            for member in chunk.batches.iter().flat_map(|batch| &batch.members) {
                if !self
                    .source_states
                    .get(member.source_patch)
                    .is_ok_and(|(coverage, ready)| ready && *coverage == member.coverage)
                {
                    sources = 1;
                } else if sources == 0 && !readiness.is_ready(member.source.id()) {
                    sources = 2;
                }
            }
            if sources == 1 {
                out.source_patch += 1;
                continue;
            }
            if sources == 2 {
                out.source_mesh_upload += 1;
                continue;
            }
            let mut stopped = false;
            for batch in &chunk.batches {
                if batch.selected == 0 {
                    continue;
                }
                if owned_source_mask(batch, world, &fallback_cells) == 0 {
                    if batch.pending_upload && !readiness.is_ready(batch.mesh.id()) {
                        out.unowned_batch_upload += 1;
                        stopped = true;
                        break;
                    }
                } else if batch.pending_upload {
                    out.owned_batch_upload += 1;
                    stopped = true;
                    break;
                } else if !self.drawn.get(batch.entity).is_ok_and(|receipt| {
                    receipt.mesh == batch.mesh.id()
                        && receipt.material == batch.state.material.id().untyped()
                }) {
                    if !current.is_unviewed(batch.entity) {
                        out.owned_undrawn_batch_viewed += 1;
                        out.viewed_undrawn.push((batch.entity, batch.mesh.id()));
                    } else if !batch
                        .members
                        .iter()
                        .all(|member| current.is_unviewed(member.source_patch))
                    {
                        out.owned_undrawn_region_viewed += 1;
                    } else if !batch.members.iter().all(|member| {
                        self.coverage_views
                            .get(member.source_patch)
                            .is_ok_and(|view| !view.0)
                    }) {
                        out.owned_undrawn_region_observed += 1;
                    } else {
                        out.owned_undrawn_not_prepared += 1;
                        if out.not_prepared.len() < 4 {
                            out.not_prepared.push((
                                batch.entity,
                                batch.mesh.id(),
                                batch.state.material.id().untyped(),
                            ));
                        }
                    }
                    stopped = true;
                    break;
                }
            }
            if !stopped {
                out.other += 1;
            }
        }
        out
    }

    /// Chunks still holding their source hierarchy, uploaded or not.
    pub(super) fn holding_source_count(&self) -> usize {
        self.chunks
            .iter()
            .filter(|(_, chunk, _)| chunk.source_children.is_some())
            .count()
    }

    /// Chunks the world-ready check waits for (see [`still_loading`]).
    pub(super) fn pending_count(&self) -> usize {
        pending_chunks(
            self.chunks.iter().map(|(entity, chunk, _)| (entity, chunk)),
            &self.roots,
            &self.views,
            self.readiness.as_deref(),
            || self.cluster_fallback_cells(),
        )
    }

    /// Immutable selection meshes still awaiting activation after their initial hierarchy transfer.
    pub(super) fn pending_selection_uploads(&self) -> usize {
        self.chunks
            .iter()
            .filter(|(_, chunk, _)| chunk.source_children.is_none())
            .flat_map(|(_, chunk, _)| &chunk.batches)
            .filter(|batch| batch.pending_upload)
            .count()
    }

    /// Run after semantic selection has updated every inactive mask. A transfer changes the
    /// representation of the same ready coverage, so its exact source removal events can be
    /// ignored by the next selector pass rather than rebuilding the tier map again.
    pub(super) fn finalize_one(&mut self, replaced_sources: &mut HashSet<Entity>) -> bool {
        self.finalize_one_with_current_visibility(replaced_sources, None)
    }

    // Only the Last-schedule caller may supply current visibility: Update precedes CheckVisibility.
    fn finalize_one_with_current_visibility(
        &mut self,
        replaced_sources: &mut HashSet<Entity>,
        current: Option<&crate::swap_bridge::CurrentCoverageVisibility>,
    ) -> bool {
        // Activation and retirement act only on chunks still holding their source hierarchy. Once
        // every chunk has handed over there is nothing to decide, and a settled frame skips the
        // fallback-cell set and the chunk walk below.
        if !self
            .chunks
            .iter()
            .any(|(_, chunk, _)| chunk.source_children.is_some())
        {
            return false;
        }
        crate::work_counters::record("terrain_batching/finalize_scan");
        let fallback_cells = self.cluster_fallback_cells();
        let Some(readiness) = self.readiness.as_deref() else {
            return false;
        };
        if !readiness.has_render_backend() {
            return false;
        }
        // Reveal uploaded initial batches while the source is still present. Waiting
        // for a draw on a Hidden staging mesh would deadlock the handoff.
        for (entity, mut chunk, ready) in &mut self.chunks {
            let world = self
                .roots
                .get(entity)
                .ok()
                .map(|root| root.key.worldspace_id);
            if ready.is_some() && chunk.source_children.is_some() {
                // Observe each owned batch's own source region, not the whole chunk: a far chunk
                // across the frustum edge has drawn batches in view and offscreen batches that
                // can never draw, and a chunk-wide observation would hold the latter forever.
                for batch in &chunk.batches {
                    if owned_source_mask(batch, world, &fallback_cells) == 0 {
                        continue;
                    }
                    for member in &batch.members {
                        if !self.coverage_requests.contains(member.source_patch) {
                            self.commands
                                .entity(member.source_patch)
                                .try_insert(crate::swap_bridge::SwapCoverageRoot);
                        }
                    }
                }
                // Marked changed only when a batch is activated: a chunk waiting for its draw is
                // scanned again every frame and must not make batch watchers redo their work.
                let mut activated = false;
                for batch in &mut chunk.bypass_change_detection().batches {
                    let owned = owned_source_mask(batch, world, &fallback_cells);
                    if owned != 0 && batch.pending_upload && readiness.is_ready(batch.mesh.id()) {
                        activate_uploaded_batch(batch, readiness, owned, &mut self.commands);
                        activated = true;
                    }
                }
                if activated {
                    chunk.set_changed();
                }
            }
        }
        let candidate = self.chunks.iter().find_map(|(entity, chunk, ready)| {
            let world = self
                .roots
                .get(entity)
                .ok()
                .map(|root| root.key.worldspace_id);
            chunk.source_children.as_ref()?;
            ready?;
            if current.is_some()
                && !chunk
                    .batches
                    .iter()
                    .any(|batch| owned_source_mask(batch, world, &fallback_cells) != 0)
            {
                return None;
            }
            // CPU source readiness does not prove GPU preparation. Keep the source scene until
            // the immutable source buffers are prepared as well: later index-mask transitions
            // use those retained buffers for their exact coverage fallback.
            let mut sources_ready = true;
            for member in chunk.batches.iter().flat_map(|batch| &batch.members) {
                // An externally changed source keeps its hierarchy and readiness lifecycle.
                // Virtual coverage may replace only the exact validated ready source set.
                if !self
                    .source_states
                    .get(member.source_patch)
                    .is_ok_and(|(coverage, ready)| ready && *coverage == member.coverage)
                {
                    return None;
                }
                if !readiness.is_ready(member.source.id()) {
                    readiness.request(member.source.id());
                    sources_ready = false;
                }
            }
            (sources_ready
                && chunk.batches.iter().all(|batch| {
                    if batch.selected == 0 {
                        true
                    } else if owned_source_mask(batch, world, &fallback_cells) == 0 {
                        !batch.pending_upload || readiness.is_ready(batch.mesh.id())
                    } else {
                        let drawn = self.drawn.get(batch.entity).is_ok_and(|receipt| {
                            receipt.mesh == batch.mesh.id()
                                && receipt.material == batch.state.material.id().untyped()
                        });
                        // Prepared offscreen transfer requires an observation of every source
                        // patch this batch replaces, never absence of the replacement batch
                        // alone: current culling and the renderer's own observation must both
                        // say unviewed. Unknown holds.
                        let unviewed_ready = current.is_some_and(|visibility| {
                            visibility.is_unviewed(batch.entity)
                                && batch.members.iter().all(|member| {
                                    visibility.is_unviewed(member.source_patch)
                                        && self
                                            .coverage_views
                                            .get(member.source_patch)
                                            .is_ok_and(|view| !view.0)
                                })
                        }) && self.prepared.get(batch.entity).is_ok_and(
                            |(mesh, surface)| {
                                mesh.0.id() == batch.mesh.id()
                                    && surface.material_id() == batch.state.material.id().untyped()
                            },
                        );
                        !batch.pending_upload && (drawn || unviewed_ready)
                    }
                }))
            .then_some(entity)
        });
        let Some(entity) = candidate else {
            return false;
        };
        let world = self
            .roots
            .get(entity)
            .ok()
            .map(|root| root.key.worldspace_id);
        let (_, mut chunk, _) = self.chunks.get_mut(entity).expect("selected chunk exists");
        // The observed source patches leave with the hierarchy below.
        // Unregister before despawning so the retained immutable GLB cannot respawn duplicates.
        self.commands.entity(entity).remove::<WorldAssetRoot>();
        self.commands.entity(entity).remove::<WorldInstance>();
        for child in chunk
            .source_children
            .take()
            .expect("initial source hierarchy exists")
        {
            self.commands.entity(child).try_despawn();
        }
        for batch in &mut chunk.batches {
            for member in &batch.members {
                replaced_sources.insert(member.source_patch);
                readiness.forget(member.source.id());
            }
            // Keep compact source primitives extracted after hierarchy retirement.
            // Current-view selection can revive these immediately if the batch's
            // pipeline, material or generation withdraws, even beyond the LAND ring.
            let owned = owned_source_mask(batch, world, &fallback_cells);
            // Stock transfer reveals the uploaded batch only after the old hierarchy is removed.
            activate_uploaded_batch(batch, readiness, owned, &mut self.commands);
            select_source_fallbacks(batch, entity, owned, &mut self.commands);
        }
        true
    }
    pub(super) fn removed(&mut self) -> bool {
        self.removed.read().count() != 0
    }

    pub(super) fn upload_completed(&self) -> bool {
        let Some(readiness) = self.readiness.as_deref() else {
            return false;
        };
        if !readiness.has_ready_uploads() {
            return false;
        }
        self.chunks.iter().any(|(_, chunk, _)| {
            chunk.source_children.is_none()
                && chunk
                    .batches
                    .iter()
                    .any(|batch| batch.pending_upload && readiness.is_ready(batch.mesh.id()))
        })
    }

    /// Active batch coverage belongs to its chunk's world, not to grid coordinates alone.
    pub(super) fn coverage_for_world(
        &self,
        world: Option<u32>,
    ) -> impl Iterator<Item = TerrainCoverage> + '_ {
        self.chunks
            .iter()
            .filter(move |(entity, chunk, ready)| {
                ready.is_some()
                    && chunk.source_children.is_none()
                    && self
                        .roots
                        .get(*entity)
                        .is_ok_and(|root| Some(root.terrain_key().worldspace_id) == world)
            })
            .flat_map(|(_, chunk, _)| {
                chunk
                    .batches
                    .iter()
                    .flat_map(|batch| batch.members.iter().map(|member| member.coverage))
            })
    }

    /// Inventory uses saved masks and member counts; it never visits vertex streams or tier maps.
    pub(super) fn counts(&self) -> (usize, usize, usize, usize) {
        self.chunks
            .iter()
            .fold((0, 0, 0, 0), |mut counts, (_, chunk, ready)| {
                let active = ready.is_some() && chunk.source_children.is_none();
                for batch in &chunk.batches {
                    counts.0 += batch.members.len();
                    counts.1 += 1;
                    if active {
                        counts.2 += batch.selected.count_ones() as usize;
                        counts.3 += usize::from(batch.selected != 0 && !batch.pending_upload);
                    }
                }
                counts
            })
    }

    /// Upload completion cannot change selected coverage. Activate only the stored current IDs;
    /// a semantic change always updates masks first through `update` instead of this fast path.
    pub(super) fn finish_uploads(&mut self) {
        let fallback_cells = self.cluster_fallback_cells();
        let Some(readiness) = self.readiness.as_deref() else {
            return;
        };
        if !readiness.has_ready_uploads() {
            return;
        }
        for (entity, mut chunk, ready) in &mut self.chunks {
            let world = self
                .roots
                .get(entity)
                .ok()
                .map(|root| root.key.worldspace_id);
            if ready.is_none() || chunk.source_children.is_some() {
                continue;
            }
            // Only a chunk whose batch is activated changes; the rest must not be marked
            // changed, or every terrain consumer that watches batches redoes its work.
            let mut activated = false;
            for batch in &mut chunk.bypass_change_detection().batches {
                if batch.pending_upload && readiness.is_ready(batch.mesh.id()) {
                    let owned = owned_source_mask(batch, world, &fallback_cells);
                    activate_uploaded_batch(batch, readiness, owned, &mut self.commands);
                    activated = true;
                }
            }
            if activated {
                chunk.set_changed();
            }
        }
    }

    /// Selects one world's coverage and clears masks in every other retained world.
    pub(super) fn update_for_world<S: std::hash::BuildHasher>(
        &mut self,
        selected_tiers: &HashMap<(IVec2, u8), Option<LodTier>, S>,
        world: Option<u32>,
    ) -> (usize, usize, usize) {
        self.update_scoped(selected_tiers, Some(world))
    }

    #[cfg(test)]
    pub(super) fn update<S: std::hash::BuildHasher>(
        &mut self,
        selected_tiers: &HashMap<(IVec2, u8), Option<LodTier>, S>,
    ) -> (usize, usize, usize) {
        self.update_scoped(selected_tiers, None)
    }

    pub(super) fn update_scoped<S: std::hash::BuildHasher>(
        &mut self,
        selected_tiers: &HashMap<(IVec2, u8), Option<LodTier>, S>,
        world: Option<Option<u32>>,
    ) -> (usize, usize, usize) {
        let fallback_cells = self.cluster_fallback_cells();
        let Some(meshes) = self.meshes.as_deref_mut() else {
            return (0, 0, 0);
        };
        let Some(readiness) = self.readiness.as_deref() else {
            return (0, 0, 0);
        };
        let mut visible_quadrants = 0;
        let mut visible_batches = 0;
        let mut changed_batches = 0;
        for (root, mut chunk, ready) in &mut self.chunks {
            let chunk_world = self.roots.get(root).ok().map(|root| root.key.worldspace_id);
            let in_world = world.is_none_or(|world| {
                self.roots
                    .get(root)
                    .is_ok_and(|root| Some(root.terrain_key().worldspace_id) == world)
            });
            let initial_upload = chunk.source_children.is_some();
            // A chunk is marked changed only when one of its batches really changes.
            let mut chunk_changed = false;
            for batch in &mut chunk.bypass_change_detection().batches {
                let selected =
                    batch
                        .members
                        .iter()
                        .enumerate()
                        .fold(0u16, |mask, (index, member)| {
                            if in_world
                                && ready.is_some()
                                && selected_tiers
                                    .get(&(member.coverage.grid, member.coverage.quadrant))
                                    == Some(&member.coverage.tier)
                            {
                                mask | (1 << index)
                            } else {
                                mask
                            }
                        });
                if !initial_upload {
                    visible_quadrants += selected.count_ones() as usize;
                }
                if selected != batch.selected {
                    let Some(source_geometry) = meshes.get(&batch.mesh) else {
                        continue;
                    };
                    let mut replacement = source_geometry.clone();
                    let (indices, bounds) = selected_geometry(&batch.members, selected);
                    replacement.insert_indices(Indices::U32(indices));
                    replacement.final_aabb = bounds.map(Into::into);
                    let replacement = meshes.add(replacement);
                    // Every pending mask has a new immutable ID. A GPU-ready acknowledgment can
                    // therefore never refer to old or deferred contents on a reused asset handle.
                    readiness.forget(batch.mesh.id());
                    meshes.remove(batch.mesh.id());
                    batch.mesh = replacement;
                    batch.selected = selected;
                    batch.pending_upload = selected != 0;
                    self.commands
                        .entity(batch.entity)
                        .remove::<(
                            crate::swap_bridge::SwapPrepared,
                            crate::swap_bridge::SwapDrawn,
                            crate::swap_bridge::SwapDrawReceipt,
                        )>()
                        .insert((
                            Mesh3d(batch.mesh.clone()),
                            Visibility::Hidden,
                            crate::swap_bridge::SwapWaiting,
                        ));
                    if !initial_upload {
                        // Preserve the entity and pipeline identity of every still-selected
                        // source. Only sources that leave semantic demand may be removed.
                        select_source_fallbacks(batch, root, selected, &mut self.commands);
                    }
                    if selected != 0 {
                        readiness.request(batch.mesh.id());
                    }
                    changed_batches += 1;
                    chunk_changed = true;
                } else if !initial_upload
                    && batch.pending_upload
                    && readiness.is_ready(batch.mesh.id())
                {
                    let owned = owned_source_mask(batch, chunk_world, &fallback_cells);
                    activate_uploaded_batch(batch, readiness, owned, &mut self.commands);
                    chunk_changed = true;
                }
                if !initial_upload {
                    if !batch.pending_upload {
                        let owned = owned_source_mask(batch, chunk_world, &fallback_cells);
                        let before = batch.fallback_selected;
                        select_source_fallbacks(batch, root, owned, &mut self.commands);
                        chunk_changed |= batch.fallback_selected != before;
                    }
                    visible_batches += usize::from(selected != 0 && !batch.pending_upload);
                }
            }
            if chunk_changed {
                chunk.set_changed();
            }
        }
        (visible_quadrants, visible_batches, changed_batches)
    }
}

/// Current visibility authority exists only after PostUpdate CheckVisibility. This shares the
/// ordinary commit/time ledger and never changes Update's draw-receipt retirement contract.
pub(in crate::streaming) fn finalize_unviewed_lod_batches(
    config: Res<crate::config::EngineConfig>,
    mut budget: ResMut<crate::streaming::StreamingCommitBudget>,
    mut streaming: ResMut<super::LodStreaming>,
    mut profiler: ResMut<crate::profiling::ProfilingState>,
    mut batches: LodBatchVisibility,
    current: crate::swap_bridge::CurrentCoverageVisibility,
) {
    if !config.cluster_terrain
        || config.clusters.is_none()
        || streaming.terrain_batch_commit_used
        || !super::terrain_handover_budget_available(&budget, config.max_commit_micros_per_frame)
    {
        return;
    }
    if batches.finalize_one_with_current_visibility(
        &mut streaming.replaced_source_quadrants,
        Some(&current),
    ) {
        streaming.terrain_batch_commit_used = true;
        streaming.terrain_batch_activated_chunks += 1;
        budget.remaining -= 1;
        budget.commits = budget.commits.saturating_add(1);
        profiler.increment("lod/terrain_batch_chunks_activated", 1);
        let pending_cpu = streaming.terrain_batch_pending_cpu_chunks;
        super::record_terrain_batch_progress(
            &mut streaming,
            &mut profiler,
            pending_cpu,
            batches.pending_count(),
        );
    }
}

/// Why chunks still holding their source hierarchy cannot retire: the blocker counts, a probe of
/// each batch a view sees but no camera has drawn, and each selected batch still waiting for its
/// upload. Read-only; logged by [`log_terrain_retire_blockers`] and [`report_terrain_stall`].
#[allow(clippy::type_complexity)]
#[derive(SystemParam)]
pub(crate) struct RetireReport<'w, 's> {
    batches: LodBatchVisibility<'w, 's>,
    current: crate::swap_bridge::CurrentCoverageVisibility<'w, 's>,
    probe: Query<
        'w,
        's,
        (
            Option<&'static Mesh3d>,
            Option<&'static crate::swap_bridge::SwapDrawReceipt>,
            Has<crate::swap_bridge::SwapWaiting>,
            Has<crate::swap_bridge::SwapPrepared>,
            Has<crate::swap_bridge::SwapDrawn>,
            Has<crate::swap_bridge::SwapInCameraView>,
            Option<&'static Visibility>,
            Option<&'static InheritedVisibility>,
            Option<&'static ViewVisibility>,
        ),
    >,
    prepared_probe: Query<
        'w,
        's,
        (
            Has<crate::swap_bridge::SwapWaiting>,
            Has<crate::swap_bridge::SwapPrepared>,
            Option<&'static Mesh3d>,
            Option<&'static crate::swap_bridge::SwapSurface>,
            Option<&'static Visibility>,
        ),
    >,
}

impl RetireReport<'_, '_> {
    fn log(&self) {
        let blockers = self.batches.retire_blockers(&self.current);
        // Why a batch a camera sees has no draw receipt for its current mesh: the bridge's
        // markers and the batch's own visibility, one line per batch.
        for (entity, mesh) in &blockers.viewed_undrawn {
            let Ok((
                mesh3d,
                receipt,
                waiting,
                prepared,
                drawn,
                in_view,
                visibility,
                inherited,
                view,
            )) = self.probe.get(*entity)
            else {
                info!(?entity, "viewed undrawn terrain batch is gone");
                continue;
            };
            info!(
                ?entity,
                mesh_current = mesh3d.is_some_and(|m| m.0.id() == *mesh),
                receipt_mesh_current = receipt.map(|r| r.mesh == *mesh),
                waiting,
                prepared,
                drawn,
                in_view,
                ?visibility,
                inherited = inherited.map(|v| v.get()),
                view = view.map(|v| v.get()),
                "viewed undrawn terrain batch"
            );
        }
        // Why an offscreen batch has no matching prepared marker: the bridge's markers on it and
        // whether the prepared one names the batch's current mesh and material.
        for (entity, mesh, material) in &blockers.not_prepared {
            info!(
                ?entity,
                waiting = self.prepared_probe.get(*entity).is_ok_and(|p| p.0),
                prepared_marker = self.prepared_probe.get(*entity).is_ok_and(|p| p.1),
                mesh_current = self
                    .prepared_probe
                    .get(*entity)
                    .ok()
                    .and_then(|p| p.2)
                    .map(|m| m.0.id() == *mesh),
                surface_material_current = self
                    .prepared_probe
                    .get(*entity)
                    .ok()
                    .and_then(|p| p.3)
                    .map(|s| s.material_id() == *material),
                visibility = ?self.prepared_probe.get(*entity).ok().and_then(|p| p.4),
                "offscreen terrain batch without a prepared marker"
            );
        }
        info!(?blockers, "terrain batches awaiting source retirement");
        // Selected batches whose upload never arrived hold the world-ready check.
        let readiness = self.batches.readiness.as_deref();
        for (root, chunk, _) in self.batches.chunks.iter() {
            if chunk.source_children.is_none() {
                continue;
            }
            for batch in &chunk.batches {
                let id = batch.mesh.id();
                if batch.selected != 0
                    && batch.pending_upload
                    && !readiness.is_some_and(|r| r.is_ready(id))
                {
                    info!(
                        ?root,
                        batch = ?batch.entity,
                        selected = batch.selected,
                        requested = readiness.is_some_and(|r| r.is_requested(id)),
                        mesh_asset_exists = self.batches.meshes.as_deref().is_some_and(|m| m.contains(id)),
                        "selected terrain batch waiting for its upload"
                    );
                }
            }
        }
    }
}

/// Profiled cluster-terrain runs only: every 300 frames while chunks still hold their source
/// hierarchy, log why each cannot retire. Ordinary runs install nothing.
pub(in crate::streaming) fn log_terrain_retire_blockers(
    report: RetireReport,
    mut frames: Local<u32>,
) {
    *frames = frames.wrapping_add(1);
    if (*frames).is_multiple_of(300) && report.batches.holding_source_count() != 0 {
        report.log();
    }
}

/// Every cluster-terrain run: when the first load stalls, say once what the terrain hand-over is
/// waiting for. Costs nothing on any other frame.
pub(in crate::streaming) fn report_terrain_stall(
    _stalled: On<crate::pacing::startup::StartupLoadStalled>,
    report: RetireReport,
) {
    if report.batches.holding_source_count() != 0 {
        report.log();
    }
}

fn owned_source_mask(
    batch: &TerrainBatch,
    world: Option<u32>,
    owned: &HashSet<(u32, IVec2)>,
) -> u16 {
    let Some(world) = world else {
        return 0;
    };
    batch
        .members
        .iter()
        .enumerate()
        .fold(0, |mask, (index, member)| {
            mask | if batch.selected & (1 << index) != 0
                && owned.contains(&(world, member.coverage.grid))
            {
                1 << index
            } else {
                0
            }
        })
}

fn select_source_fallbacks(
    batch: &mut TerrainBatch,
    root: Entity,
    selected: u16,
    commands: &mut Commands,
) {
    if batch.fallback_selected == selected & batch.selected {
        return;
    }
    let mut retained: HashMap<_, _> = batch
        .members
        .iter()
        .enumerate()
        .filter_map(|(index, _)| (batch.fallback_selected & (1 << index) != 0).then_some(index))
        .zip(std::mem::take(&mut batch.fallback_entities))
        .collect();
    for (index, member) in batch.members.iter().enumerate() {
        if selected & batch.selected & (1 << index) != 0 {
            let entity = retained.remove(&index).unwrap_or_else(|| {
                spawn_render_mesh(
                    commands,
                    root,
                    member.source.clone(),
                    &batch.state,
                    Visibility::Inherited,
                    Some(member.bounds),
                )
            });
            batch.fallback_entities.push(entity);
        }
    }
    batch.fallback_selected = selected & batch.selected;
    for entity in retained.into_values() {
        commands.entity(entity).try_despawn();
    }
}

fn activate_uploaded_batch(
    batch: &mut TerrainBatch,
    readiness: &TerrainMeshUploadReadiness,
    retained: u16,
    commands: &mut Commands,
) {
    if batch.pending_upload && !readiness.is_ready(batch.mesh.id()) {
        return;
    }
    readiness.forget(batch.mesh.id());
    batch.pending_upload = false;
    // Stock LOD transfers on GPU upload readiness, including offscreen batches.
    // Reversible persistent fallback is confined to the cluster-terrain option.
    let previous = batch.fallback_selected;
    let mut entities = std::mem::take(&mut batch.fallback_entities).into_iter();
    for index in 0..batch.members.len() {
        if previous & (1 << index) != 0
            && let Some(entity) = entities.next()
        {
            if retained & (1 << index) != 0 {
                batch.fallback_entities.push(entity);
            } else {
                commands.entity(entity).try_despawn();
            }
        }
    }
    batch.fallback_selected &= retained;
    let (_, bounds) = selected_geometry(&batch.members, batch.selected);
    if let Some(bounds) = bounds {
        commands.entity(batch.entity).insert((
            Mesh3d(batch.mesh.clone()),
            NoAutoAabb,
            bounds,
            Visibility::Inherited,
        ));
    } else {
        commands.entity(batch.entity).insert(Visibility::Hidden);
    }
}

fn selected_geometry(members: &[BatchMember], selected: u16) -> (Vec<u32>, Option<Aabb>) {
    let mut indices = Vec::new();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (index, member) in members.iter().enumerate() {
        if selected & (1 << index) != 0 {
            indices.extend_from_slice(&member.indices);
            min = min.min(member.bounds.min().into());
            max = max.max(member.bounds.max().into());
        }
    }
    let bounds = (!indices.is_empty()).then(|| Aabb::from_min_max(min, max));
    (indices, bounds)
}

impl LodTerrainBatches {
    /// Exact extraction facts for immutable current masks and their retained source fallback.
    /// The caller supplies the chunk world; no expanded rectangular coverage is inferred.
    pub(in crate::streaming) fn render_surfaces(
        &self,
    ) -> Vec<(Entity, Handle<Mesh>, Vec<TerrainCoverage>, bool)> {
        let mut out = Vec::new();
        for batch in &self.batches {
            let selected: Vec<_> = batch
                .members
                .iter()
                .enumerate()
                .filter_map(|(index, member)| {
                    (batch.selected & (1 << index) != 0).then_some(member)
                })
                .collect();
            if !selected.is_empty() {
                out.push((
                    batch.entity,
                    batch.mesh.clone(),
                    selected.iter().map(|member| member.coverage).collect(),
                    false,
                ));
            }
            let fallback_members =
                batch
                    .members
                    .iter()
                    .enumerate()
                    .filter_map(|(index, member)| {
                        (batch.fallback_selected & (1 << index) != 0).then_some(member)
                    });
            for (entity, member) in batch.fallback_entities.iter().zip(fallback_members) {
                out.push((*entity, member.source.clone(), vec![member.coverage], true));
            }
        }
        out
    }

    pub(crate) fn coverage_receipt(&self) -> Vec<serde_json::Value> {
        self.batches
            .iter()
            .map(|batch| {
                serde_json::json!({
                    "mesh_generation": format!("{:?}", batch.mesh.id()),
                    "selection_mask": batch.selected,
                    "pending_upload": batch.pending_upload,
                    "initial_upload": self.source_children.is_some(),
                    "selected_members": batch.members.iter().enumerate()
                        .filter(|(index, _)| batch.selected & (1 << index) != 0)
                        .map(|(_, member)| member.coverage.receipt()).collect::<Vec<_>>()
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LodStreaming, update_terrain_lod_visibility};
    use super::*;
    use crate::{
        config::EngineConfig,
        profiling::ProfilingState,
        streaming::{RenderOrigin, StreamingCommitBudget, StreamingMetrics},
        world::components::{CELL_SIZE, StreamingCamera, TerrainPatch},
    };
    use bevy::{ecs::system::RunSystemOnce, mesh::PrimitiveTopology};
    use shared::lod::{TERRAIN_QUADRANT_INDEX_COUNT, TERRAIN_QUADRANT_VERTEX_COUNT};

    struct Fixture {
        root: Entity,
        scene_root: Entity,
        patches: Vec<(Entity, TerrainCoverage)>,
        primitives: Vec<Entity>,
        mesh_handles: Vec<Handle<Mesh>>,
        source: Handle<WorldAsset>,
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<WorldAsset>>()
            .init_resource::<LodStreaming>()
            .init_resource::<TerrainMeshUploadReadiness>();
        app.world()
            .resource::<TerrainMeshUploadReadiness>()
            .enable_test_render_backend();
        app
    }

    fn source_mesh(grid: IVec2, quadrant: u8) -> Mesh {
        let mut positions: Vec<_> = (0..64)
            .map(|index| {
                let (x, y) = match index {
                    0..=16 => (index, 0),
                    17..=32 => (16, index - 16),
                    33..=48 => (48 - index, 16),
                    _ => (0, 64 - index),
                };
                [
                    grid.x as f32 * CELL_SIZE
                        + f32::from(quadrant % 2) * CELL_SIZE * 0.5
                        + x as f32 * CELL_SIZE / 32.0,
                    grid.y as f32 * CELL_SIZE
                        + f32::from(quadrant / 2) * CELL_SIZE * 0.5
                        + y as f32 * CELL_SIZE / 32.0,
                    10.0 + (index as f32 * 0.5),
                ]
            })
            .collect();
        positions.push([
            grid.x as f32 * CELL_SIZE
                + f32::from(quadrant % 2) * CELL_SIZE * 0.5
                + CELL_SIZE * 0.25,
            grid.y as f32 * CELL_SIZE
                + f32::from(quadrant / 2) * CELL_SIZE * 0.5
                + CELL_SIZE * 0.25,
            12.0,
        ]);
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 65])
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_UV_0,
            (0..65)
                .map(|index| [index as f32 / 64.0, 0.5])
                .collect::<Vec<_>>(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.2, 0.7]; 65])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.5, 0.75, 0.8, 1.0]; 65])
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0, 0.0, 0.0, -1.0]; 65])
        .with_inserted_indices(Indices::U32(
            (0..64)
                .flat_map(|edge| [64, edge, (edge + 1) % 64])
                .collect(),
        ))
    }

    fn fixture(app: &mut App, grids: &[IVec2], tier: LodTier) -> Fixture {
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let worldspace_id = app
            .world()
            .get_resource::<EngineConfig>()
            .map_or(60, |config| config.worldspace_id);
        let root = app
            .world_mut()
            .spawn((
                super::super::LodChunkRoot {
                    key: shared::lod::ChunkKey::new(
                        worldspace_id,
                        tier,
                        shared::lod::ChunkAnchor::new(0, 0),
                    ),
                    generation: 1,
                    origin: shared::lod::LodOrigin::new(0, 0),
                    retry_count: 0,
                },
                Transform::from_xyz(1000.0, 0.0, -2000.0),
                Visibility::Hidden,
            ))
            .id();
        let loader_scene = app
            .world_mut()
            .spawn((
                Name::new("chunk_4_0_0"),
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(root),
            ))
            .id();
        let scene_root = app
            .world_mut()
            .spawn((
                Name::new("chunk_4_0_0"),
                Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::Inherited,
                ChildOf(loader_scene),
            ))
            .id();
        let mut patches = Vec::new();
        let mut primitives = Vec::new();
        let mut mesh_handles = Vec::new();
        let mut source_world = World::new();
        for &grid in grids {
            let cell = app
                .world_mut()
                .spawn((
                    Name::new(nodes::source_cell(grid.x, grid.y)),
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(scene_root),
                ))
                .id();
            let group = app
                .world_mut()
                .spawn((
                    Name::new(nodes::terrain_group()),
                    Transform::IDENTITY,
                    Visibility::Inherited,
                    ChildOf(cell),
                ))
                .id();
            for (quadrant, name) in ["sw", "se", "nw", "ne"].into_iter().enumerate() {
                let quadrant = quadrant as u8;
                let patch = app
                    .world_mut()
                    .spawn((
                        Name::new(format!("terrain_quadrant_{name}")),
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        ChildOf(group),
                    ))
                    .id();
                let mesh = app
                    .world_mut()
                    .resource_mut::<Assets<Mesh>>()
                    .add(source_mesh(grid, quadrant));
                let primitive = app
                    .world_mut()
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        ChildOf(patch),
                    ))
                    .id();
                source_world.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
                primitives.push(primitive);
                mesh_handles.push(mesh);
                patches.push((
                    patch,
                    TerrainCoverage {
                        grid,
                        quadrant,
                        tier: Some(tier),
                    },
                ));
            }
        }
        let source = app
            .world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .add(WorldAsset::new(source_world));
        app.world_mut()
            .entity_mut(root)
            .insert(WorldAssetRoot(source.clone()));
        Fixture {
            root,
            scene_root,
            patches,
            primitives,
            mesh_handles,
            source,
        }
    }

    fn prepare(app: &mut App, fixture: &Fixture) -> Result<Vec<PreparedBatch>, String> {
        let root = fixture.root;
        let patches = fixture.patches.clone();
        app.world_mut()
            .run_system_once(
                move |preparation: LodBatchPreparation,
                      children: Query<&Children>,
                      names: Query<&Name>,
                      mesh_handles: Query<&Mesh3d>,
                      meshes: Res<Assets<Mesh>>| {
                    preparation.prepare(root, &patches, &children, &names, &mesh_handles, &meshes)
                },
            )
            .unwrap()
    }

    fn install_fixture(app: &mut App, fixture: &Fixture, mut batches: Vec<PreparedBatch>) {
        for &(entity, coverage) in &fixture.patches {
            app.world_mut()
                .entity_mut(entity)
                .insert((coverage, TerrainSurfaceReady));
        }
        app.world_mut()
            .entity_mut(fixture.root)
            .insert(Visibility::Inherited);
        let root = fixture.root;
        let source = fixture.source.clone();
        app.world_mut()
            .run_system_once(
                move |children: Query<&Children>,
                      mut commands: Commands,
                      mut meshes: ResMut<Assets<Mesh>>,
                      readiness: Res<TerrainMeshUploadReadiness>| {
                    install(
                        root,
                        source.clone(),
                        std::mem::take(&mut batches),
                        &children,
                        &mut commands,
                        &mut meshes,
                        Some(&readiness),
                    );
                },
            )
            .unwrap();
    }

    fn acknowledge_uploads(app: &App, root: Entity) {
        let batches = app.world().get::<LodTerrainBatches>(root).unwrap();
        let readiness = app.world().resource::<TerrainMeshUploadReadiness>();
        for batch in &batches.batches {
            if batch.pending_upload && !readiness.is_ready(batch.mesh.id()) {
                assert!(readiness.acknowledge(batch.mesh.id()));
            }
            if batches.source_children.is_some() {
                for member in &batch.members {
                    readiness.request(member.source.id());
                    if !readiness.is_ready(member.source.id()) {
                        assert!(readiness.acknowledge(member.source.id()));
                    }
                }
            }
        }
    }

    fn enable_cluster_fallback(app: &mut App) {
        use crate::cluster::terrain::{ClusterTerrain, TerrainBlock};
        app.init_resource::<EngineConfig>();
        let world = {
            let mut config = app.world_mut().resource_mut::<EngineConfig>();
            config.cluster_terrain = true;
            config.clusters = Some(String::new());
            config.worldspace_id
        };
        app.insert_resource(ClusterTerrain::for_blocks_test(vec![TerrainBlock {
            mesh_hash: String::new(),
            primitive_index: 0,
            worldspace_id: world,
            anchor: IVec2::ZERO,
            side_cells: 4,
            instance_path: String::new(),
            cells: vec![IVec2::ZERO, IVec2::new(2, 0)],
            material: Handle::default(),
            batch: Some(0),
        }]));
    }

    // Synthetic protocol fixtures only: these do not represent GPU execution.
    fn acknowledge_uploads_and_draws(app: &mut App, root: Entity) {
        acknowledge_uploads(app, root);
        let receipts: Vec<_> = app
            .world()
            .get::<LodTerrainBatches>(root)
            .unwrap()
            .batches
            .iter()
            .filter(|batch| batch.selected != 0)
            .map(|batch| {
                (
                    batch.entity,
                    crate::swap_bridge::SwapDrawReceipt {
                        mesh: batch.mesh.id(),
                        material: batch.state.material.id().untyped(),
                    },
                )
            })
            .collect();
        for (entity, receipt) in receipts {
            app.world_mut()
                .entity_mut(entity)
                .insert((receipt, crate::swap_bridge::SwapDrawn));
        }
    }

    fn finalize_initial(app: &mut App) -> bool {
        app.world_mut()
            .run_system_once(
                |mut batches: LodBatchVisibility, mut streaming: ResMut<LodStreaming>| {
                    batches.finalize_one(&mut streaming.replaced_source_quadrants)
                },
            )
            .unwrap()
    }

    fn finalize_unviewed_initial(app: &mut App) -> bool {
        app.world_mut()
            .run_system_once(
                |mut batches: LodBatchVisibility,
                 mut streaming: ResMut<LodStreaming>,
                 current: crate::swap_bridge::CurrentCoverageVisibility| {
                    batches.finalize_one_with_current_visibility(
                        &mut streaming.replaced_source_quadrants,
                        Some(&current),
                    )
                },
            )
            .unwrap()
    }

    /// The renderer's historical observation of every source patch (synthetic).
    fn observe_patches(app: &mut App, fixture: &Fixture, in_view: bool) {
        for &(patch, _) in &fixture.patches {
            app.world_mut()
                .entity_mut(patch)
                .insert(crate::swap_bridge::SwapCoverageInCameraView(in_view));
        }
    }

    fn unview_fixture_meshes(app: &mut App, fixture: &Fixture, batch: Entity) {
        for &entity in &fixture.primitives {
            app.world_mut()
                .entity_mut(entity)
                .insert(ViewVisibility::default());
        }
        app.world_mut()
            .entity_mut(batch)
            .insert(ViewVisibility::default());
    }

    fn reset_submission_budget(mut budget: ResMut<StreamingCommitBudget>) {
        budget.remaining = 4;
        budget.commits = 0;
        budget.frame_started = std::time::Instant::now();
    }

    fn selection_app() -> App {
        use super::super::batch_ready_lod_chunks;
        let mut app = test_app();
        app.insert_resource(EngineConfig {
            max_commit_micros_per_frame: 1_000_000,
            ..default()
        })
        .insert_resource(RenderOrigin(IVec2::ZERO))
        .init_resource::<StreamingCommitBudget>()
        .init_resource::<StreamingMetrics>()
        .init_resource::<ProfilingState>()
        .add_systems(
            Update,
            (
                reset_submission_budget,
                batch_ready_lod_chunks,
                update_terrain_lod_visibility,
            )
                .chain(),
        );
        app.world_mut().spawn((
            StreamingCamera,
            Transform::from_xyz(CELL_SIZE * 0.5, 0.0, -CELL_SIZE * 0.5),
        ));
        app
    }

    fn ready_source(app: &mut App, fixture: &Fixture) {
        for &(entity, coverage) in &fixture.patches {
            app.world_mut()
                .entity_mut(entity)
                .insert((coverage, TerrainSurfaceReady));
        }
        app.world_mut()
            .entity_mut(fixture.root)
            .insert(Visibility::Inherited);
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
    }

    fn queue_source(app: &mut App, fixture: &Fixture) {
        use super::super::LodChunkRoot;
        use shared::lod::{ChunkAnchor, ChunkKey, LodOrigin};
        let tier = fixture.patches[0].1.tier.unwrap();
        let worldspace_id = app
            .world()
            .get_resource::<EngineConfig>()
            .map_or(60, |config| config.worldspace_id);
        app.world_mut().entity_mut(fixture.root).insert((
            LodChunkRoot {
                key: ChunkKey::new(worldspace_id, tier, ChunkAnchor::new(0, 0)),
                generation: 1,
                origin: LodOrigin::new(0, 0),
                retry_count: 0,
            },
            PendingLodTerrainBatching {
                source: fixture.source.clone(),
                patches: fixture.patches.clone(),
            },
        ));
    }

    fn full_quadrant(app: &mut App, grid: IVec2, quadrant: u8) -> Entity {
        let worldspace_id = app.world().resource::<EngineConfig>().worldspace_id;
        app.world_mut()
            .spawn((
                crate::streaming::TerrainWorldspace(worldspace_id),
                TerrainPatch,
                TerrainCoverage {
                    grid,
                    quadrant,
                    tier: None,
                },
                TerrainSurfaceReady,
            ))
            .id()
    }

    #[test]
    fn handoff_incremental_batch_masks_sources_and_removal_match_fresh_extraction() {
        use crate::streaming::terrain_handoff::{RenderTerrain, extraction_schedule_for_test};
        use bevy::render::MainWorld;

        fn run(main: &mut World, render: &mut World, schedule: &mut Schedule) {
            let mut extracted = MainWorld::default();
            std::mem::swap(main, &mut *extracted);
            render.insert_resource(extracted);
            schedule.run(render);
            let mut extracted = render.remove_resource::<MainWorld>().unwrap();
            std::mem::swap(main, &mut *extracted);
        }
        fn compare(main: &mut World, render: &mut World, schedule: &mut Schedule) {
            run(main, render, schedule);
            let mut fresh = World::new();
            fresh.init_resource::<RenderTerrain>();
            run(main, &mut fresh, &mut extraction_schedule_for_test());
            assert_eq!(
                render.resource::<RenderTerrain>().surfaces,
                fresh.resource::<RenderTerrain>().surfaces
            );
        }
        let mut app = test_app();
        app.init_resource::<EngineConfig>();
        app.insert_resource(RenderOrigin(IVec2::ZERO));
        crate::streaming::identity_changes::install_terrain(app.world_mut());
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        ready_source(&mut app, &fixture);
        queue_source(&mut app, &fixture);
        let prepared = prepare(&mut app, &fixture).unwrap();
        install_fixture(&mut app, &fixture, prepared);
        app.world_mut()
            .get_mut::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0]
            .selected = 0b1111;
        let mut render = World::new();
        render.init_resource::<RenderTerrain>();
        let mut schedule = extraction_schedule_for_test();
        // This fixture supplies extraction facts only, not executed rendering or GPU coverage.
        let entities: Vec<_> = app
            .world_mut()
            .query_filtered::<Entity, With<Mesh3d>>()
            .iter(app.world())
            .collect();
        for entity in entities {
            app.world_mut()
                .entity_mut(entity)
                .insert(InheritedVisibility::VISIBLE);
        }
        compare(app.world_mut(), &mut render, &mut schedule);
        let (batch, sources) = {
            let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            let batch = &chunk.batches[0];
            (
                batch.entity,
                batch
                    .members
                    .iter()
                    .skip(2)
                    .map(|member| member.source.clone())
                    .collect::<Vec<_>>(),
            )
        };
        let fallbacks: Vec<_> = sources
            .into_iter()
            .map(|mesh| {
                app.world_mut()
                    .spawn((Mesh3d(mesh), InheritedVisibility::VISIBLE))
                    .id()
            })
            .collect();
        {
            let mut chunk = app
                .world_mut()
                .get_mut::<LodTerrainBatches>(fixture.root)
                .unwrap();
            chunk.batches[0].selected = 0b0011;
            chunk.batches[0].fallback_selected = 0b1100;
            chunk.batches[0].fallback_entities = fallbacks.clone();
        }
        compare(app.world_mut(), &mut render, &mut schedule);
        assert_eq!(
            render.resource::<RenderTerrain>().surfaces[&batch.into()]
                .keys
                .len(),
            2
        );
        for entity in fallbacks {
            assert_eq!(
                render.resource::<RenderTerrain>().surfaces[&entity.into()]
                    .keys
                    .len(),
                1
            );
        }
        app.world_mut()
            .get_mut::<super::super::LodChunkRoot>(fixture.root)
            .unwrap()
            .key =
            shared::lod::ChunkKey::new(8, LodTier::Tier4, shared::lod::ChunkAnchor::new(0, 0));
        compare(app.world_mut(), &mut render, &mut schedule);
        assert_eq!(
            render.resource::<RenderTerrain>().surfaces[&batch.into()].keys[0].world,
            8
        );
        app.world_mut()
            .entity_mut(batch)
            .insert(InheritedVisibility::HIDDEN);
        compare(app.world_mut(), &mut render, &mut schedule);
        app.world_mut()
            .entity_mut(batch)
            .insert(InheritedVisibility::VISIBLE);
        compare(app.world_mut(), &mut render, &mut schedule);
        app.world_mut()
            .entity_mut(fixture.root)
            .remove::<LodTerrainBatches>();
        compare(app.world_mut(), &mut render, &mut schedule);
        assert!(
            !render
                .resource::<RenderTerrain>()
                .surfaces
                .contains_key(&batch.into())
        );
    }

    #[test]
    fn selection_upload_gauge_separates_initial_chunks_and_tracks_activation() {
        let mut app = selection_app();
        let fixture = fixture(&mut app, &[IVec2::ZERO, IVec2::new(3, 0)], LodTier::Tier4);
        let output = tempfile::tempdir().unwrap();
        let assert_gauges = |app: &App, initial: f64, selection: f64| {
            let config = EngineConfig {
                profile_output_dir: Some(output.path().to_owned()),
                ..default()
            };
            app.world()
                .resource::<ProfilingState>()
                .write_bundle(
                    &config,
                    &serde_json::json!({"average_fps": 90.0, "frame_ms_p95": 14.0, "passed": true}),
                    Some(app.world().resource::<StreamingMetrics>()),
                    &crate::render::RendererMetrics::default(),
                    None,
                    None,
                )
                .unwrap();
            let report: serde_json::Value = serde_json::from_slice(
                &std::fs::read(output.path().join("cpu-spans.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                report["gauges"]["lod/pending_initial_terrain_upload_chunks"],
                initial
            );
            assert_eq!(
                report["gauges"]["lod/pending_terrain_selection_uploads"],
                selection
            );
        };

        ready_source(&mut app, &fixture);
        queue_source(&mut app, &fixture);
        app.update();
        // Preparation consumes this frame's commit slot; sample the retained initial queue
        // on the next update while all mesh acknowledgments are still withheld.
        app.update();
        assert_gauges(&app, 1.0, 0.0);
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.update();
        assert_gauges(&app, 0.0, 0.0);

        full_quadrant(&mut app, IVec2::ZERO, 0);
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
        app.update();
        assert_gauges(&app, 0.0, 1.0);
        full_quadrant(&mut app, IVec2::new(3, 0), 0);
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
        app.update();
        assert_gauges(&app, 0.0, 2.0);
        let revision = app.world().resource::<LodStreaming>().visibility_revision;
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.update();
        assert_gauges(&app, 0.0, 0.0);
        assert_eq!(
            app.world().resource::<LodStreaming>().visibility_revision,
            revision
        );
    }

    #[test]
    fn stable_conversion_and_transfer_preserve_mixed_empty_masks_without_tier_rebuilds() {
        let mut app = selection_app();
        let fixture = fixture(&mut app, &[IVec2::ZERO, IVec2::new(3, 0)], LodTier::Tier4);
        ready_source(&mut app, &fixture);
        full_quadrant(&mut app, IVec2::ZERO, 0);
        for quadrant in 0..4 {
            full_quadrant(&mut app, IVec2::new(3, 0), quadrant);
        }
        app.update();
        let revision = app.world().resource::<LodStreaming>().visibility_revision;
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            3
        );

        queue_source(&mut app, &fixture);
        app.update();
        assert_eq!(
            app.world().resource::<LodStreaming>().visibility_revision,
            revision
        );
        assert_eq!(app.world().resource::<StreamingCommitBudget>().commits, 1);
        let (generated, mixed_entity, empty_entity) = {
            let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            assert!(chunk.source_children.is_some());
            assert_eq!(chunk.batches.len(), 2);
            assert_eq!(chunk.batches[0].selected, 0b1110);
            assert_eq!(chunk.batches[1].selected, 0);
            assert!(chunk.batches[0].pending_upload);
            assert!(!chunk.batches[1].pending_upload);
            for batch in &chunk.batches {
                assert_eq!(
                    app.world()
                        .resource::<Assets<Mesh>>()
                        .get(&batch.mesh)
                        .unwrap()
                        .indices()
                        .unwrap()
                        .len(),
                    batch.selected.count_ones() as usize * TERRAIN_QUADRANT_INDEX_COUNT
                );
            }
            (
                chunk
                    .batches
                    .iter()
                    .map(|batch| batch.mesh.id())
                    .collect::<Vec<_>>(),
                chunk.batches[0].entity,
                chunk.batches[1].entity,
            )
        };
        assert!(
            fixture
                .primitives
                .iter()
                .all(|&entity| app.world().get_entity(entity).is_ok())
        );
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.update();
        assert_eq!(
            app.world().resource::<LodStreaming>().visibility_revision,
            revision
        );
        assert_eq!(app.world().resource::<StreamingCommitBudget>().commits, 1);
        assert!(
            fixture
                .primitives
                .iter()
                .all(|&entity| app.world().get_entity(entity).is_err())
        );
        assert_eq!(
            *app.world().get::<Visibility>(mixed_entity).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            *app.world().get::<Visibility>(empty_entity).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            3
        );
        assert_eq!(
            app.world()
                .resource::<LodStreaming>()
                .replaced_source_quadrants
                .len(),
            fixture.patches.len()
        );

        app.update();
        assert_eq!(
            app.world().resource::<LodStreaming>().visibility_revision,
            revision,
            "the two expected removal streams do not rebuild the same tier map"
        );
        assert!(
            app.world()
                .resource::<LodStreaming>()
                .replaced_source_quadrants
                .is_empty()
        );
        app.world_mut().entity_mut(fixture.root).despawn();
        assert!(
            generated
                .iter()
                .all(|&id| app.world().resource::<Assets<Mesh>>().get(id).is_none())
        );
        app.update();
        assert_eq!(
            app.world().resource::<LodStreaming>().visibility_revision,
            revision + 1,
            "real root removal still invalidates available tiers"
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            0
        );
    }

    #[test]
    fn initial_acknowledgments_wait_for_current_camera_and_readiness_masks() {
        // Both cells are inside one compiler tier-4 chunk, but initially only the first is in
        // the default twelve-cell reach. A ready old mask must not flatten the hierarchy before
        // the newly in-range second cell's nonempty replacement has uploaded.
        {
            let mut app = selection_app();
            let fixture = fixture(&mut app, &[IVec2::ZERO, IVec2::new(3, 0)], LodTier::Tier4);
            {
                let world = app.world_mut();
                let mut cameras = world.query_filtered::<&mut Transform, With<StreamingCamera>>();
                cameras.single_mut(world).unwrap().translation.x = -9.5 * CELL_SIZE;
            }
            ready_source(&mut app, &fixture);
            queue_source(&mut app, &fixture);
            app.update();
            {
                let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
                assert_eq!(chunk.batches[0].selected, 0b1111);
                assert_eq!(
                    chunk.batches[1].selected, 0,
                    "same-frame readiness corrects preparation's inherited source visibility"
                );
            }
            acknowledge_uploads_and_draws(&mut app, fixture.root);
            let old_far = app
                .world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .batches[1]
                .mesh
                .id();
            {
                let world = app.world_mut();
                let mut cameras = world.query_filtered::<&mut Transform, With<StreamingCamera>>();
                cameras.single_mut(world).unwrap().translation.x = 0.5 * CELL_SIZE;
            }
            app.update();
            let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            assert!(chunk.source_children.is_some());
            assert_eq!(chunk.batches[1].selected, 0b1111);
            assert!(chunk.batches[1].pending_upload);
            assert_ne!(chunk.batches[1].mesh.id(), old_far);
            assert!(
                fixture
                    .primitives
                    .iter()
                    .all(|&entity| app.world().get_entity(entity).is_ok())
            );
            assert_eq!(
                app.world()
                    .resource::<StreamingMetrics>()
                    .visible_lod_terrain_patches,
                8
            );
            acknowledge_uploads_and_draws(&mut app, fixture.root);
            app.update();
            assert!(
                app.world()
                    .get::<LodTerrainBatches>(fixture.root)
                    .unwrap()
                    .source_children
                    .is_none()
            );
        }

        // A full-detail surface loses readiness on the same update that yesterday's initial
        // three-quadrant upload is acknowledged. The new four-quadrant mask remains inactive.
        let mut app = selection_app();
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let near = full_quadrant(&mut app, IVec2::ZERO, 0);
        ready_source(&mut app, &fixture);
        queue_source(&mut app, &fixture);
        app.update();
        let old = app
            .world()
            .get::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0]
            .mesh
            .id();
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.world_mut()
            .entity_mut(near)
            .remove::<TerrainSurfaceReady>();
        app.update();
        let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
        assert!(chunk.source_children.is_some());
        assert_eq!(chunk.batches[0].selected, 0b1111);
        assert_ne!(chunk.batches[0].mesh.id(), old);
        assert!(
            !app.world()
                .resource::<TerrainMeshUploadReadiness>()
                .is_ready(old)
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            4
        );
        assert!(
            fixture
                .primitives
                .iter()
                .all(|&entity| app.world().get_entity(entity).is_ok())
        );
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.update();
        assert!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .source_children
                .is_none()
        );
    }

    #[test]
    fn source_readiness_loss_is_not_suppressed_and_upload_only_completion_avoids_rebuilds() {
        let mut app = selection_app();
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        ready_source(&mut app, &fixture);
        queue_source(&mut app, &fixture);
        app.update();
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        let source = fixture.patches[0].0;
        app.world_mut()
            .entity_mut(source)
            .remove::<TerrainSurfaceReady>();
        let revision = app.world().resource::<LodStreaming>().visibility_revision;
        app.update();
        assert_eq!(
            app.world().resource::<LodStreaming>().visibility_revision,
            revision + 1
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            3
        );
        assert!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .source_children
                .is_some()
        );
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.update();
        assert!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .source_children
                .is_some(),
            "an externally unready source keeps its original readiness lifecycle"
        );
        assert!(
            app.world()
                .resource::<LodStreaming>()
                .replaced_source_quadrants
                .is_empty()
        );
        app.world_mut()
            .entity_mut(source)
            .insert(TerrainSurfaceReady);
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
        app.update();
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.update();
        assert!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .source_children
                .is_none()
        );
        app.update();
        let near = full_quadrant(&mut app, IVec2::ZERO, 0);
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
        app.update();
        let revision = app.world().resource::<LodStreaming>().visibility_revision;
        // The changed mask is covered by exact temporary source copies until its upload
        // completes, then stock transfer releases them (cluster ownership is not configured).
        assert_eq!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .batches[0]
                .fallback_entities
                .len(),
            3
        );
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.update();
        assert_eq!(
            app.world().resource::<LodStreaming>().visibility_revision,
            revision,
            "ready current uploads need no new tier map"
        );
        assert_eq!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .batches[0]
                .fallback_entities
                .len(),
            0
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            3
        );
        // A later real coverage removal is independent of the retired source-ID transfer set.
        app.world_mut().entity_mut(near).remove::<TerrainCoverage>();
        app.update();
        assert_eq!(
            app.world().resource::<LodStreaming>().visibility_revision,
            revision + 1
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            4
        );
    }

    #[test]
    fn stationary_batch_retirement_reselects_held_quadrants_without_a_camera_move() {
        let mut app = selection_app();
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        // A coarser unbatched neighbour lives in the same quadrant as the Tier4 sources.
        let worldspace_id = app.world().resource::<EngineConfig>().worldspace_id;
        let held_owner = app
            .world_mut()
            .spawn((
                super::super::LodChunkRoot {
                    key: shared::lod::ChunkKey::new(
                        worldspace_id,
                        LodTier::Tier8,
                        shared::lod::ChunkAnchor::new(0, 0),
                    ),
                    generation: 0,
                    origin: shared::lod::LodOrigin::new(0, 0),
                    retry_count: 0,
                },
                Visibility::Inherited,
            ))
            .id();
        let held = app
            .world_mut()
            .spawn((
                ChildOf(held_owner),
                TerrainCoverage {
                    grid: IVec2::ZERO,
                    quadrant: 0,
                    tier: Some(LodTier::Tier8),
                },
                TerrainSurfaceReady,
                Visibility::Hidden,
            ))
            .id();
        ready_source(&mut app, &fixture);
        queue_source(&mut app, &fixture);
        app.update();
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.update();
        assert!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .source_children
                .is_none(),
            "the initial sources were replaced by the batch"
        );
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(held).unwrap(),
            Visibility::Hidden
        );
        let revision = app.world().resource::<LodStreaming>().visibility_revision;

        // Retiring the batch chunk without a camera move must reselect the held quadrant
        // from its own remaining tiers instead of waiting for the next crossing.
        app.world_mut().entity_mut(fixture.root).despawn();
        app.update();
        assert_eq!(
            app.world().resource::<LodStreaming>().visibility_revision,
            revision + 1,
            "a batch retirement refreshes the selection without a camera move"
        );
        assert_eq!(
            *app.world().get::<Visibility>(held).unwrap(),
            Visibility::Inherited,
            "the held Tier8 becomes the quadrant's selection once the batch's Tier4 is gone"
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            1
        );
    }

    #[test]
    fn transfer_tracking_expires_with_missing_events_and_without_a_camera() {
        let mut app = selection_app();
        app.update();
        let absent = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .replaced_source_quadrants
            .insert(absent);
        app.update();
        assert!(
            app.world()
                .resource::<LodStreaming>()
                .replaced_source_quadrants
                .is_empty(),
            "neither stream produced an event, so no tracking survives"
        );
        let worldspace_id = app.world().resource::<EngineConfig>().worldspace_id;
        let lod_patch = |app: &mut App, worldspace_id| {
            let owner = app
                .world_mut()
                .spawn((
                    super::super::LodChunkRoot {
                        key: shared::lod::ChunkKey::new(
                            worldspace_id,
                            LodTier::Tier4,
                            shared::lod::ChunkAnchor::new(0, 0),
                        ),
                        generation: 0,
                        origin: shared::lod::LodOrigin::new(0, 0),
                        retry_count: 0,
                    },
                    Visibility::Inherited,
                ))
                .id();
            app.world_mut()
                .spawn((
                    ChildOf(owner),
                    TerrainCoverage {
                        grid: IVec2::ZERO,
                        quadrant: 0,
                        tier: Some(LodTier::Tier4),
                    },
                    TerrainSurfaceReady,
                    Visibility::Inherited,
                ))
                .id()
        };
        let coarse = lod_patch(&mut app, worldspace_id);
        let foreign = lod_patch(&mut app, worldspace_id.wrapping_add(1));
        let real = full_quadrant(&mut app, IVec2::ZERO, 0);
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(coarse),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<Visibility>(foreign),
            Some(&Visibility::Hidden)
        );
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .replaced_source_quadrants
            .insert(absent);
        let mut cameras = app
            .world_mut()
            .query_filtered::<Entity, With<StreamingCamera>>();
        let camera = cameras.single(app.world()).unwrap();
        app.world_mut().entity_mut(camera).despawn();
        app.world_mut()
            .entity_mut(real)
            .remove::<TerrainSurfaceReady>();
        app.update();
        assert!(
            app.world()
                .resource::<LodStreaming>()
                .replaced_source_quadrants
                .is_empty()
        );
        // The maintained selector may consume invalidation without a camera. Its observable
        // obligation is exact coverage on the first resumed frame, without stale ownership.
        let camera = app
            .world_mut()
            .spawn((
                StreamingCamera,
                Transform::from_xyz(CELL_SIZE * 0.5, 0.0, -CELL_SIZE * 0.5),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(coarse),
            Some(&Visibility::Inherited)
        );
        assert_eq!(
            app.world().get::<Visibility>(foreign),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            1
        );

        app.world_mut().entity_mut(camera).despawn();
        app.world_mut()
            .entity_mut(coarse)
            .remove::<TerrainSurfaceReady>();
        app.update();
        app.world_mut().spawn((
            StreamingCamera,
            Transform::from_xyz(CELL_SIZE * 0.5, 0.0, -CELL_SIZE * 0.5),
        ));
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(coarse),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<Visibility>(foreign),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            0
        );
        assert!(
            app.world()
                .resource::<LodStreaming>()
                .replaced_source_quadrants
                .is_empty()
        );
    }

    #[test]
    fn batches_preserve_every_vertex_attribute_triangle_and_quadrant() {
        let mut app = test_app();
        let fixture = fixture(
            &mut app,
            &[IVec2::ZERO, IVec2::X, IVec2::Y, IVec2::ONE],
            LodTier::Tier4,
        );
        let prepared = prepare(&mut app, &fixture).unwrap();
        assert_eq!(prepared.len(), 1);
        let batch = &prepared[0];
        assert_eq!(batch.members.len(), 16);
        assert_eq!(
            batch.mesh.count_vertices(),
            16 * TERRAIN_QUADRANT_VERTEX_COUNT
        );
        let meshes = app.world().resource::<Assets<Mesh>>();
        let sources: HashMap<_, _> = fixture
            .patches
            .iter()
            .zip(&fixture.mesh_handles)
            .map(|((_, coverage), handle)| {
                (
                    (coverage.grid, coverage.quadrant),
                    meshes.get(handle).unwrap(),
                )
            })
            .collect();
        for (attribute, values) in batch.mesh.attributes() {
            let mut expected_bytes = Vec::new();
            for member in &batch.members {
                expected_bytes.extend_from_slice(
                    sources[&(member.coverage.grid, member.coverage.quadrant)]
                        .attribute(attribute.id)
                        .unwrap()
                        .get_bytes(),
                );
            }
            assert_eq!(
                values.get_bytes(),
                expected_bytes,
                "attribute {} is preserved byte for byte",
                attribute.name
            );
        }
        for (index, member) in batch.members.iter().enumerate() {
            let source = sources[&(member.coverage.grid, member.coverage.quadrant)];
            let expected: Vec<_> = source
                .indices()
                .unwrap()
                .iter()
                .map(|source_index| (source_index + index * TERRAIN_QUADRANT_VERTEX_COUNT) as u32)
                .collect();
            assert_eq!(member.indices, expected);
        }
        let (indices, _) = selected_geometry(&batch.members, u16::MAX);
        assert_eq!(indices.len(), 16 * TERRAIN_QUADRANT_INDEX_COUNT);
        let (partial, bounds) = selected_geometry(&batch.members, 0b1001);
        assert_eq!(
            partial,
            [
                batch.members[0].indices.clone(),
                batch.members[3].indices.clone()
            ]
            .concat()
        );
        let bounds = bounds.unwrap();
        assert_eq!(
            bounds.min(),
            batch.members[0]
                .bounds
                .min()
                .min(batch.members[3].bounds.min())
        );
        assert_eq!(
            bounds.max(),
            batch.members[0]
                .bounds
                .max()
                .max(batch.members[3].bounds.max())
        );
        assert_eq!(selected_geometry(&batch.members, 0), (Vec::new(), None));
    }

    #[test]
    fn negative_tiles_and_incompatible_materials_or_flags_remain_separate() {
        assert_eq!(batch_tile(IVec2::new(-1, -1)), IVec2::new(-1, -1));
        assert_eq!(batch_tile(IVec2::new(-2, -2)), IVec2::new(-1, -1));
        assert_eq!(batch_tile(IVec2::new(-3, 0)), IVec2::new(-2, 0));
        let mut app = test_app();
        let fixture = fixture(
            &mut app,
            &[IVec2::new(-2, -2), IVec2::new(-1, -1), IVec2::ZERO],
            LodTier::Tier4,
        );
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        app.world_mut()
            .entity_mut(fixture.primitives[0])
            .insert((MeshMaterial3d(material), NotShadowCaster));
        app.world_mut().entity_mut(fixture.primitives[1]).insert((
            NotShadowReceiver,
            RenderLayers::layer(3),
            MeshTag(7),
        ));
        let batches = prepare(&mut app, &fixture).unwrap();
        assert_eq!(
            batches.len(),
            4,
            "two tiles plus two distinct primitive states"
        );
        let shadow_excluded = batches
            .iter()
            .find(|batch| batch.state.not_shadow_caster)
            .unwrap();
        assert_eq!(shadow_excluded.members.len(), 1);
        let tagged = batches
            .iter()
            .find(|batch| batch.state.tag == Some(7))
            .unwrap();
        assert!(tagged.state.not_shadow_receiver);
        assert_eq!(tagged.state.layers, Some(RenderLayers::layer(3)));
        assert!(batches.iter().all(|batch| {
            batch
                .members
                .iter()
                .all(|member| batch_tile(member.coverage.grid) == batch.tile)
        }));
    }

    #[test]
    fn unsupported_scene_behavior_leaves_the_original_hierarchy_untouched() {
        let mut app = test_app();
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let original_entities = app.world().entities().len();
        let mut checks = 0;
        app.world_mut()
            .entity_mut(fixture.primitives[0])
            .insert(NoAutomaticBatching);
        assert!(
            prepare(&mut app, &fixture)
                .err()
                .unwrap()
                .contains("render behavior")
        );
        app.world_mut()
            .entity_mut(fixture.primitives[0])
            .remove::<NoAutomaticBatching>();
        checks += 1;
        app.world_mut()
            .entity_mut(fixture.primitives[0])
            .insert(Transform::from_scale(Vec3::new(1.0, 2.0, 1.0)));
        assert!(
            prepare(&mut app, &fixture)
                .err()
                .unwrap()
                .contains("local transforms")
        );
        app.world_mut()
            .entity_mut(fixture.primitives[0])
            .insert(Transform::IDENTITY);
        checks += 1;
        let material = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(fixture.primitives[0])
            .unwrap()
            .0
            .clone();
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&material)
            .unwrap()
            .alpha_mode = AlphaMode::Blend;
        assert!(
            prepare(&mut app, &fixture)
                .err()
                .unwrap()
                .contains("transparency ordering")
        );
        app.world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&material)
            .unwrap()
            .alpha_mode = AlphaMode::Opaque;
        checks += 1;
        let objects = app
            .world_mut()
            .spawn((
                Name::new("objects"),
                Transform::IDENTITY,
                ChildOf(fixture.scene_root),
            ))
            .id();
        assert!(
            prepare(&mut app, &fixture)
                .err()
                .unwrap()
                .contains("non-terrain node")
        );
        app.world_mut().entity_mut(objects).despawn();
        checks += 1;
        assert_eq!(checks, 4);
        assert_eq!(app.world().entities().len(), original_entities);
        assert!(app.world().get::<WorldAssetRoot>(fixture.root).is_some());
        assert!(
            fixture
                .primitives
                .iter()
                .all(|&entity| app.world().get_entity(entity).is_ok())
        );
        assert!(prepare(&mut app, &fixture).is_ok());
        app.world_mut()
            .entity_mut(fixture.primitives[0])
            .insert(NoCpuCulling);
        assert!(
            prepare(&mut app, &fixture).is_err(),
            "explicit mesh culling behavior is retained by fallback"
        );
        app.world_mut()
            .entity_mut(fixture.primitives[0])
            .remove::<NoCpuCulling>();
        app.world_mut()
            .entity_mut(fixture.scene_root)
            .insert((NoCpuCulling, StructuralHierarchyNode));
        assert!(
            prepare(&mut app, &fixture).is_ok(),
            "owned empty hierarchy optimization is compatible"
        );
        app.world_mut()
            .entity_mut(fixture.scene_root)
            .insert(Collider::ball(1.0));
        assert!(
            prepare(&mut app, &fixture).is_err(),
            "simulation content cannot be flattened"
        );
    }

    #[test]
    fn the_cluster_fallback_cells_are_rebuilt_only_when_their_inputs_change() {
        let mut app = test_app();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<LodStreaming>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>();
        enable_cluster_fallback(&mut app);
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let prepared = prepare(&mut app, &fixture).unwrap();
        install_fixture(&mut app, &fixture, prepared);
        app.update();
        // The same system instance keeps its cache from frame to frame.
        app.add_systems(Update, |activation: LodBatchActivation| {
            activation.pending_count();
        });
        let builds = || {
            crate::work_counters::snapshot()
                .before_ready
                .get("terrain_batching/fallback_cells")
                .copied()
                .unwrap_or(0)
        };
        let start = builds();
        for _ in 0..3 {
            app.update();
        }
        assert_eq!(
            builds() - start,
            1,
            "three frames with a chunk waiting build the set once"
        );
        app.world_mut()
            .resource_mut::<crate::cluster::terrain::ClusterTerrain>()
            .set_changed();
        app.update();
        assert_eq!(builds() - start, 2, "a cluster terrain change rebuilds it");
    }

    #[test]
    fn without_cluster_terrain_an_uploaded_chunk_loads_until_it_hands_over() {
        let mut app = test_app();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<LodStreaming>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, update_terrain_lod_visibility);
        app.world_mut().spawn((
            StreamingCamera,
            Transform::from_xyz(CELL_SIZE * 0.5, 0.0, -CELL_SIZE * 0.5),
        ));
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let prepared = prepare(&mut app, &fixture).unwrap();
        install_fixture(&mut app, &fixture, prepared);
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
        app.update();
        let pending = |app: &mut App| {
            app.world_mut()
                .run_system_once(|activation: LodBatchActivation| activation.pending_count())
                .unwrap()
        };
        acknowledge_uploads(&app, fixture.root);
        assert_eq!(
            pending(&mut app),
            1,
            "an uploaded chunk that has not handed over yet is still loading"
        );
        assert!(finalize_initial(&mut app));
        assert_eq!(pending(&mut app), 0);
    }

    #[test]
    fn an_uploaded_chunk_waiting_for_its_draw_is_not_loading_and_stays_unmarked() {
        let mut app = test_app();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<LodStreaming>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, update_terrain_lod_visibility);
        // Cluster terrain owns the cell, so the hand-over waits for a draw of the batch.
        enable_cluster_fallback(&mut app);
        app.world_mut().spawn((
            StreamingCamera,
            Transform::from_xyz(CELL_SIZE * 0.5, 0.0, -CELL_SIZE * 0.5),
        ));
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let prepared = prepare(&mut app, &fixture).unwrap();
        install_fixture(&mut app, &fixture, prepared);
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
        app.update();
        let pending = |app: &mut App| {
            app.world_mut()
                .run_system_once(|activation: LodBatchActivation| activation.pending_count())
                .unwrap()
        };
        assert_eq!(
            pending(&mut app),
            1,
            "the batch upload is not on the GPU yet"
        );
        acknowledge_uploads(&app, fixture.root);
        assert!(
            !finalize_initial(&mut app),
            "no draw yet: the source hierarchy stays"
        );
        assert!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .source_children
                .is_some()
        );
        assert_eq!(
            pending(&mut app),
            1,
            "an owned batch no view sees hands over unviewed by itself: still loading"
        );
        let batch_entity = app
            .world()
            .get::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0]
            .entity;
        app.world_mut()
            .entity_mut(batch_entity)
            .insert(ViewVisibility::VISIBLE);
        assert_eq!(
            pending(&mut app),
            0,
            "an uploaded chunk waiting only for its draw is not loading"
        );
        // Both progress writers agree; the visibility pass records after the batching queue.
        assert_eq!(
            app.world_mut()
                .run_system_once(|batches: LodBatchVisibility| (
                    batches.pending_count(),
                    batches.holding_source_count()
                ))
                .unwrap(),
            (0, 1)
        );
        // A finalize pass that activates nothing must not mark the batches changed.
        let before = app
            .world()
            .entity(fixture.root)
            .get_ref::<LodTerrainBatches>()
            .unwrap()
            .last_changed();
        app.world_mut().increment_change_tick();
        assert!(!finalize_initial(&mut app));
        assert_eq!(
            app.world()
                .entity(fixture.root)
                .get_ref::<LodTerrainBatches>()
                .unwrap()
                .last_changed(),
            before
        );
    }

    #[test]
    fn deferred_uploads_keep_exact_coverage_and_superseded_masks_cannot_activate() {
        let mut app = test_app();
        app.insert_resource(EngineConfig::default())
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<LodStreaming>()
            .init_resource::<StreamingMetrics>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, update_terrain_lod_visibility);
        app.world_mut().spawn((
            StreamingCamera,
            Transform::from_xyz(CELL_SIZE * 0.5, 0.0, -CELL_SIZE * 0.5),
        ));
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let prepared = prepare(&mut app, &fixture).unwrap();
        install_fixture(&mut app, &fixture, prepared);
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
        app.update();
        let entity = app
            .world()
            .get::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0]
            .entity;
        assert!(
            !finalize_initial(&mut app),
            "no GPU acknowledgment cannot discard original terrain"
        );
        assert!(
            fixture
                .primitives
                .iter()
                .all(|&entity| app.world().get_entity(entity).is_ok())
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            4
        );
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        {
            let batches = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            let readiness = app.world().resource::<TerrainMeshUploadReadiness>();
            assert!(readiness.acknowledge(batches.batches[0].mesh.id()));
        }
        assert!(
            !finalize_initial(&mut app),
            "prepared merged buffers cannot discard unprepared fallback source buffers"
        );
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        assert!(finalize_initial(&mut app));
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
        app.update();
        assert!(
            fixture
                .primitives
                .iter()
                .all(|&entity| app.world().get_entity(entity).is_err())
        );
        assert_eq!(app.world().get::<Children>(fixture.root).unwrap().len(), 1);
        assert!(app.world().get::<WorldAssetRoot>(fixture.root).is_none());
        let original = app.world().get::<Mesh3d>(entity).unwrap().0.clone();
        let full_detail = full_quadrant(&mut app, IVec2::ZERO, 0);
        app.world_mut()
            .resource_mut::<LodStreaming>()
            .visibility_dirty = true;
        app.update();
        let pending = app.world().get::<Mesh3d>(entity).unwrap().0.clone();
        assert_ne!(
            pending.id(),
            original.id(),
            "each uploaded mask has an immutable generation ID"
        );
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&original)
                .is_none()
        );
        {
            let batch = &app
                .world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .batches[0];
            assert_eq!(batch.fallback_entities.len(), 3);
            assert_eq!(
                app.world()
                    .resource::<Assets<Mesh>>()
                    .get(&pending)
                    .unwrap()
                    .indices()
                    .unwrap()
                    .len(),
                3 * TERRAIN_QUADRANT_INDEX_COUNT
            );
            assert!(batch.fallback_entities.iter().all(|&fallback| {
                *app.world().get::<Visibility>(fallback).unwrap() == Visibility::Inherited
            }));
        }
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            3
        );
        app.update();
        assert_eq!(
            app.world().get::<Mesh3d>(entity).unwrap().0.id(),
            pending.id(),
            "unchanged selection reuses its pending handle"
        );
        // A second handoff supersedes the still-delayed three-quadrant mask. Its old acknowledgment
        // must not activate stale geometry, even if the renderer was preparing it concurrently.
        app.world_mut()
            .entity_mut(full_detail)
            .remove::<TerrainSurfaceReady>();
        app.update();
        let latest = app.world().get::<Mesh3d>(entity).unwrap().0.clone();
        assert_ne!(latest.id(), pending.id());
        assert!(
            !app.world()
                .resource::<TerrainMeshUploadReadiness>()
                .acknowledge(pending.id())
        );
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&pending)
                .is_none()
        );
        assert_eq!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .batches[0]
                .fallback_entities
                .len(),
            4
        );
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        app.update();
        let batch = &app
            .world()
            .get::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0];
        assert!(batch.fallback_entities.is_empty());
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            app.world().get::<Mesh3d>(entity).unwrap().0.id(),
            latest.id()
        );
        app.world_mut()
            .entity_mut(fixture.root)
            .remove::<TerrainSurfaceReady>();
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            app.world()
                .resource::<StreamingMetrics>()
                .visible_lod_terrain_patches,
            0
        );
        assert!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .batches[0]
                .fallback_entities
                .is_empty()
        );
    }

    #[test]
    fn flattening_preserves_the_axis_transform_and_origin_rebasing() {
        let mut app = test_app();
        let fixture = fixture(&mut app, &[IVec2::new(-1, 0)], LodTier::Tier4);
        let before = *app.world().get::<Transform>(fixture.scene_root).unwrap();
        let root_transform = *app.world().get::<Transform>(fixture.root).unwrap();
        let prepared = prepare(&mut app, &fixture).unwrap();
        assert_eq!(prepared[0].state.transform, before);
        install_fixture(&mut app, &fixture, prepared);
        let batches = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
        let entity = batches.batches[0].entity;
        let batch_transform = *app.world().get::<Transform>(entity).unwrap();
        let position = Vec3::new(-CELL_SIZE, 10.0, 20.0);
        assert_eq!(
            root_transform.transform_point(before.transform_point(position)),
            root_transform.transform_point(batch_transform.transform_point(position))
        );
        let shifted = Transform::from_translation(
            root_transform.translation - Vec3::new(CELL_SIZE, 0.0, -CELL_SIZE),
        );
        app.world_mut().entity_mut(fixture.root).insert(shifted);
        assert_eq!(
            shifted.transform_point(batch_transform.transform_point(position)),
            root_transform.transform_point(before.transform_point(position))
                - Vec3::new(CELL_SIZE, 0.0, -CELL_SIZE)
        );
        assert_eq!(
            app.world().get::<ChildOf>(entity).unwrap().parent(),
            fixture.root
        );
    }

    #[test]
    fn independent_roots_keep_source_assets_and_clean_up_only_their_generated_meshes() {
        let mut app = test_app();
        let first = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let mut second = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier8);
        // Separate streaming roots can instance the same loaded GLB. Neither conversion may
        // mutate that shared geometry, and retiring one must leave the other intact.
        second.source = first.source.clone();
        app.world_mut()
            .entity_mut(second.root)
            .insert(WorldAssetRoot(second.source.clone()));
        for (&primitive, source) in second.primitives.iter().zip(&first.mesh_handles) {
            app.world_mut()
                .entity_mut(primitive)
                .insert(Mesh3d(source.clone()));
        }
        second.mesh_handles = first.mesh_handles.clone();
        let prepared = prepare(&mut app, &first).unwrap();
        install_fixture(&mut app, &first, prepared);
        let prepared = prepare(&mut app, &second).unwrap();
        install_fixture(&mut app, &second, prepared);
        let first_mesh = app
            .world()
            .get::<LodTerrainBatches>(first.root)
            .unwrap()
            .batches[0]
            .mesh
            .clone();
        let second_mesh = app
            .world()
            .get::<LodTerrainBatches>(second.root)
            .unwrap()
            .batches[0]
            .mesh
            .clone();
        assert_ne!(first_mesh.id(), second_mesh.id());
        assert_eq!(
            app.world()
                .get::<LodTerrainBatches>(first.root)
                .unwrap()
                ._source
                .id(),
            first.source.id()
        );
        app.world_mut().entity_mut(first.root).despawn();
        let meshes = app.world().resource::<Assets<Mesh>>();
        assert!(
            meshes.get(&first_mesh).is_none(),
            "generated mesh cleanup is immediate, even with a retained test handle"
        );
        assert!(meshes.get(&second_mesh).is_some());
        for source in &first.mesh_handles {
            assert!(meshes.get(source).is_some());
        }
        assert!(app.world().get::<LodTerrainBatches>(second.root).is_some());
        app.world_mut().entity_mut(second.root).despawn();
        assert!(
            app.world()
                .resource::<Assets<Mesh>>()
                .get(&second_mesh)
                .is_none()
        );
    }

    #[test]
    fn hidden_empty_selection_never_opts_out_of_bounds_without_a_box() {
        let mut app = test_app();
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let mut prepared = prepare(&mut app, &fixture).unwrap();
        prepared[0].selected = 0;
        install_fixture(&mut app, &fixture, prepared);
        let (entity, mesh) = {
            let batches = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            (batches.batches[0].entity, batches.batches[0].mesh.clone())
        };
        let unbounded = |app: &mut App| {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, (With<Mesh3d>, With<NoAutoAabb>, Without<Aabb>)>()
                .iter(world)
                .count()
        };
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        assert!(app.world().get::<NoAutoAabb>(entity).is_none());
        assert_eq!(unbounded(&mut app), 0);
        // A later nonempty selection takes the exact box and only then opts out.
        app.world_mut()
            .run_system_once(|mut batches: LodBatchVisibility| {
                batches.update(&HashMap::from([((IVec2::ZERO, 0), Some(LodTier::Tier4))]));
            })
            .unwrap();
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        assert!(finalize_initial(&mut app));
        let _ = mesh;
        assert!(app.world().get::<NoAutoAabb>(entity).is_some());
        assert!(app.world().get::<Aabb>(entity).is_some());
        assert_eq!(unbounded(&mut app), 0);
    }

    #[test]
    fn explicit_selected_bounds_survive_gpu_extraction_and_automatic_bounds_refresh() {
        let mut app = test_app();
        app.add_plugins(bevy::app::TaskPoolPlugin::default());
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let prepared = prepare(&mut app, &fixture).unwrap();
        let expected = prepared[0].members[0].bounds;
        install_fixture(&mut app, &fixture, prepared);
        app.world_mut()
            .run_system_once(|mut batches: LodBatchVisibility| {
                batches.update(&HashMap::from([((IVec2::ZERO, 0), Some(LodTier::Tier4))]));
            })
            .unwrap();
        let (entity, handle) = {
            let batches = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            (batches.batches[0].entity, batches.batches[0].mesh.clone())
        };
        {
            let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
            let _gpu_copy = meshes.get_mut(&handle).unwrap().take_gpu_data().unwrap();
            let whole_mesh_bounds: Aabb = meshes.get(&handle).unwrap().final_aabb.unwrap().into();
            assert!(whole_mesh_bounds.max().y > expected.max().y);
        }
        acknowledge_uploads_and_draws(&mut app, fixture.root);
        assert!(finalize_initial(&mut app));
        assert!(app.world().get::<NoAutoAabb>(entity).is_some());
        assert_eq!(*app.world().get::<Aabb>(entity).unwrap(), expected);
        app.world_mut()
            .run_system_once(bevy::camera::visibility::calculate_bounds)
            .unwrap();
        assert_eq!(*app.world().get::<Aabb>(entity).unwrap(), expected);
    }

    #[test]
    fn ready_chunk_conversion_obeys_the_shared_count_and_elapsed_budget() {
        use super::super::{LodChunkRoot, batch_ready_lod_chunks};
        use crate::streaming::StreamingCommitBudget;
        use shared::lod::{ChunkAnchor, ChunkKey, LodOrigin};
        let mut app = test_app();
        let config = EngineConfig {
            max_commit_micros_per_frame: 1_000_000,
            ..default()
        };
        app.insert_resource(config)
            .init_resource::<StreamingCommitBudget>()
            .init_resource::<LodStreaming>()
            .init_resource::<ProfilingState>();
        let first = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let second = fixture(&mut app, &[IVec2::new(4, 0)], LodTier::Tier4);
        for (anchor, fixture) in [(0, &first), (1, &second)] {
            app.world_mut().entity_mut(fixture.root).insert((
                LodChunkRoot {
                    key: ChunkKey::new(1, LodTier::Tier4, ChunkAnchor::new(anchor, 0)),
                    generation: 1,
                    origin: LodOrigin::new(0, 0),
                    retry_count: 0,
                },
                PendingLodTerrainBatching {
                    source: fixture.source.clone(),
                    patches: fixture.patches.clone(),
                },
                Visibility::Inherited,
            ));
            for &(patch, coverage) in &fixture.patches {
                app.world_mut()
                    .entity_mut(patch)
                    .insert((coverage, TerrainSurfaceReady));
            }
        }
        {
            let mut budget = app.world_mut().resource_mut::<StreamingCommitBudget>();
            budget.remaining = 1;
            budget.frame_started = std::time::Instant::now() - std::time::Duration::from_secs(2);
        }
        app.world_mut()
            .run_system_once(batch_ready_lod_chunks)
            .unwrap();
        assert!(app.world().get::<LodTerrainBatches>(first.root).is_none());
        assert!(app.world().get::<LodTerrainBatches>(second.root).is_none());
        app.world_mut()
            .resource_mut::<StreamingCommitBudget>()
            .frame_started = std::time::Instant::now();
        app.world_mut()
            .run_system_once(batch_ready_lod_chunks)
            .unwrap();
        assert!(app.world().get::<LodTerrainBatches>(first.root).is_some());
        assert!(app.world().get::<LodTerrainBatches>(second.root).is_none());
        assert_eq!(app.world().resource::<StreamingCommitBudget>().remaining, 0);
        assert_eq!(app.world().resource::<StreamingCommitBudget>().commits, 1);
        assert!(
            first
                .primitives
                .iter()
                .all(|&entity| app.world().get_entity(entity).is_ok()),
            "upload waits preserve original drawable terrain"
        );
        app.world_mut()
            .run_system_once(batch_ready_lod_chunks)
            .unwrap();
        assert!(
            app.world().get::<LodTerrainBatches>(second.root).is_none(),
            "exhausted count budget cannot convert another ready chunk"
        );
        {
            let mut budget = app.world_mut().resource_mut::<StreamingCommitBudget>();
            budget.remaining = 1;
            budget.commits = 0;
            budget.frame_started = std::time::Instant::now();
        }
        app.world_mut()
            .run_system_once(batch_ready_lod_chunks)
            .unwrap();
        assert!(
            app.world().get::<LodTerrainBatches>(second.root).is_some(),
            "waiting source GPU buffers keep originals; the next preparation can use the available commit"
        );
        assert!(
            app.world()
                .get::<LodTerrainBatches>(first.root)
                .unwrap()
                .source_children
                .is_some()
        );
        assert_eq!(app.world().resource::<StreamingCommitBudget>().commits, 1);
    }

    #[test]
    fn request_controls_keep_original_lod_sources_without_generated_batches() {
        use super::super::{LodChunkRoot, batch_ready_lod_chunks};
        use crate::streaming::StreamingCommitBudget;
        use shared::lod::{ChunkAnchor, ChunkKey, LodOrigin};
        let mut app = test_app();
        app.insert_resource(EngineConfig {
            prioritize_streaming: true,
            max_commit_micros_per_frame: 1_000_000,
            ..default()
        })
        .init_resource::<StreamingCommitBudget>()
        .init_resource::<LodStreaming>()
        .init_resource::<ProfilingState>();
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        app.world_mut().entity_mut(fixture.root).insert((
            LodChunkRoot {
                key: ChunkKey::new(1, LodTier::Tier4, ChunkAnchor::new(0, 0)),
                generation: 1,
                origin: LodOrigin::new(0, 0),
                retry_count: 0,
            },
            PendingLodTerrainBatching {
                source: fixture.source.clone(),
                patches: fixture.patches.clone(),
            },
            Visibility::Inherited,
        ));
        for &(patch, coverage) in &fixture.patches {
            app.world_mut()
                .entity_mut(patch)
                .insert((coverage, TerrainSurfaceReady));
        }
        {
            let mut budget = app.world_mut().resource_mut::<StreamingCommitBudget>();
            budget.remaining = 1;
            budget.frame_started = std::time::Instant::now();
        }
        app.world_mut()
            .run_system_once(batch_ready_lod_chunks)
            .unwrap();
        assert!(
            app.world().get::<LodTerrainBatches>(fixture.root).is_none(),
            "request-controlled runs must not activate generated batches"
        );
        assert!(
            !app.world()
                .entity(fixture.root)
                .contains::<PendingLodTerrainBatching>(),
            "the pending conversion is dropped rather than retried under controls"
        );
        assert!(
            fixture
                .primitives
                .iter()
                .all(|&entity| app.world().get_entity(entity).is_ok()),
            "original LOD source hierarchy stays alive under request controls"
        );
        assert_eq!(app.world().resource::<StreamingCommitBudget>().commits, 0);
        assert!(
            app.world()
                .resource::<LodStreaming>()
                .terrain_batch_controls_warned
        );
    }
    #[test]
    fn same_grid_batches_are_selected_only_in_the_demanded_world() {
        let mut app = test_app();
        let first = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let second = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier8);
        app.world_mut()
            .get_mut::<super::super::LodChunkRoot>(second.root)
            .unwrap()
            .key
            .worldspace_id = 61;
        let prepared = prepare(&mut app, &first).unwrap();
        install_fixture(&mut app, &first, prepared);
        let prepared = prepare(&mut app, &second).unwrap();
        install_fixture(&mut app, &second, prepared);
        // Retain both representations, exactly as an interior/portal crossing can.
        acknowledge_uploads_and_draws(&mut app, first.root);
        acknowledge_uploads_and_draws(&mut app, second.root);
        assert!(finalize_initial(&mut app));
        assert!(finalize_initial(&mut app));
        for (world, tier, expected) in [
            (Some(60), LodTier::Tier4, (15, 0)),
            (Some(60), LodTier::Tier8, (0, 0)),
            (Some(61), LodTier::Tier8, (0, 15)),
            (None, LodTier::Tier8, (0, 0)),
        ] {
            let coverage = app
                .world_mut()
                .run_system_once(move |mut batches: LodBatchVisibility| {
                    let coverage: Vec<_> = batches.coverage_for_world(world).collect();
                    let selected: HashMap<_, _> = (0..4u8)
                        .map(|quadrant| ((IVec2::ZERO, quadrant), Some(tier)))
                        .collect();
                    batches.update_for_world(&selected, world);
                    coverage
                })
                .unwrap();
            assert!(coverage.iter().all(|coverage| coverage.tier
                == world.map(|world| if world == 60 {
                    LodTier::Tier4
                } else {
                    LodTier::Tier8
                })));
            assert_eq!(coverage.len(), if world.is_some() { 4 } else { 0 });
            let mask = |root| {
                app.world()
                    .get::<LodTerrainBatches>(root)
                    .unwrap()
                    .batches
                    .iter()
                    .fold(0u16, |mask, batch| mask | batch.selected)
            };
            assert_eq!((mask(first.root), mask(second.root)), expected);
        }
    }

    #[test]
    fn propagated_lod_chunk_layers_prepare_without_authored_behavior_fallback() {
        use bevy::app::HierarchyPropagatePlugin;
        for layers in [
            crate::render::reflectable_world_layers(),
            RenderLayers::layer(crate::portal::DESTINATION_LAYER),
            RenderLayers::none(),
        ] {
            let mut app = selection_app();
            app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate));
            let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
            app.world_mut()
                .entity_mut(fixture.root)
                .insert((layers.clone(), Propagate(layers.clone())));
            ready_source(&mut app, &fixture);
            // Allow real propagation to stamp every chunk/cell/group/quadrant wrapper before
            // CPU admission, reproducing delayed runtime batching rather than a bare fixture.
            app.update();
            for node in [
                fixture.scene_root,
                fixture.patches[0].0,
                fixture.primitives[0],
            ] {
                assert_eq!(app.world().get::<RenderLayers>(node), Some(&layers));
                assert_eq!(
                    app.world()
                        .get::<Inherited<RenderLayers>>(node)
                        .map(|value| &value.0),
                    Some(&layers)
                );
            }
            queue_source(&mut app, &fixture);
            app.update();
            let streaming = app.world().resource::<LodStreaming>();
            assert_eq!(streaming.terrain_batch_prepared_chunks, 1);
            assert_eq!(streaming.terrain_batch_fallback_chunks, 0);
            let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            assert!(!chunk.batches.is_empty());
            assert!(
                chunk
                    .batches
                    .iter()
                    .all(|batch| batch.state.layers.as_ref() == Some(&layers))
            );
        }
    }

    #[test]
    fn authored_or_divergent_structural_layers_still_refuse_batching() {
        let mut app = test_app();
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        let original = RenderLayers::layer(1);
        let changed = RenderLayers::layer(2);
        app.world_mut()
            .entity_mut(fixture.scene_root)
            .insert(original.clone());
        assert!(prepare(&mut app, &fixture).is_err());
        app.world_mut()
            .entity_mut(fixture.scene_root)
            .insert(Inherited(changed));
        assert!(prepare(&mut app, &fixture).is_err());
        app.world_mut()
            .entity_mut(fixture.scene_root)
            .insert((Inherited(original.clone()), Propagate(original)));
        assert!(prepare(&mut app, &fixture).is_err());
    }

    #[test]
    fn stock_lod_far_cpu_preparation_queue_and_uploads_drain() {
        let mut app = selection_app();
        let mut roots = Vec::new();
        for index in 0..97 {
            let fixture = fixture(&mut app, &[IVec2::new(index * 2, 0)], LodTier::Tier4);
            ready_source(&mut app, &fixture);
            queue_source(&mut app, &fixture);
            roots.push(fixture.root);
        }
        let pending_cpu = |app: &mut App| {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, With<PendingLodTerrainBatching>>()
                .iter(world)
                .count()
        };
        assert_eq!(pending_cpu(&mut app), 97);
        // The production preparation system runs once per reset frame budget. The bound permits
        // one preparation and one initial transfer per root plus the final counter publication.
        for _ in 0..(roots.len() * 2 + 2) {
            app.update();
            for &root in &roots {
                if app.world().get::<LodTerrainBatches>(root).is_some() {
                    acknowledge_uploads(&app, root);
                }
            }
        }
        assert_eq!(pending_cpu(&mut app), 0);
        let streaming = app.world().resource::<LodStreaming>();
        assert_eq!(streaming.terrain_batch_pending_cpu_chunks, 0);
        assert_eq!(streaming.terrain_batch_prepared_chunks, roots.len() as u64);
        assert_eq!(streaming.terrain_batch_activated_chunks, roots.len() as u64);
        let pending_uploads = app
            .world_mut()
            .run_system_once(|batches: LodBatchVisibility| {
                (
                    batches.holding_source_count(),
                    batches.pending_selection_uploads(),
                )
            })
            .unwrap();
        assert_eq!(pending_uploads, (0, 0));
        for root in roots {
            let chunk = app.world().get::<LodTerrainBatches>(root).unwrap();
            assert!(chunk.source_children.is_none());
            assert!(
                chunk
                    .batches
                    .iter()
                    .all(|batch| batch.fallback_entities.is_empty())
            );
        }
    }

    #[test]
    fn last_transfer_uses_postupdate_visibility_over_previous_offscreen_observation() {
        use crate::swap_bridge::SwapPrepared;
        for changed_source in [None, Some(true), Some(false)] {
            let mut app = test_app();
            enable_cluster_fallback(&mut app);
            let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
            ready_source(&mut app, &fixture);
            let prepared = prepare(&mut app, &fixture).unwrap();
            install_fixture(&mut app, &fixture, prepared);
            acknowledge_uploads(&app, fixture.root);
            assert!(!finalize_initial(&mut app));
            let batch = app
                .world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .batches[0]
                .entity;
            unview_fixture_meshes(&mut app, &fixture, batch);
            observe_patches(&mut app, &fixture, false);
            app.world_mut().entity_mut(batch).insert(SwapPrepared {
                stand_in: None,
                since: 1,
            });
            let entering_view =
                changed_source.map(|source| if source { fixture.primitives[0] } else { batch });
            app.init_resource::<StreamingCommitBudget>()
                .init_resource::<ProfilingState>()
                .add_systems(Update, reset_submission_budget)
                .add_systems(
                    PostUpdate,
                    move |mut visibility: Query<&mut ViewVisibility>| {
                        if let Some(entity) = entering_view {
                            *visibility.get_mut(entity).unwrap() = ViewVisibility::VISIBLE;
                        }
                    },
                )
                .add_systems(Last, finalize_unviewed_lod_batches);
            app.update();
            let retired = changed_source.is_none();
            assert_eq!(
                app.world()
                    .get::<LodTerrainBatches>(fixture.root)
                    .unwrap()
                    .source_children
                    .is_none(),
                retired,
                "current visibility overrides stale false; only known-unviewed source and batch retire"
            );
            assert!(
                fixture
                    .primitives
                    .iter()
                    .all(|entity| app.world().get_entity(*entity).is_err() == retired)
            );
            assert_eq!(
                app.world()
                    .resource::<LodStreaming>()
                    .terrain_batch_activated_chunks,
                u64::from(retired)
            );
            assert_eq!(
                app.world().resource::<StreamingCommitBudget>().remaining,
                if retired { 3 } else { 4 }
            );
        }
    }

    #[test]
    fn last_transfer_keeps_unknown_current_source_visibility() {
        use crate::swap_bridge::SwapPrepared;
        let mut app = test_app();
        enable_cluster_fallback(&mut app);
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        ready_source(&mut app, &fixture);
        let prepared = prepare(&mut app, &fixture).unwrap();
        install_fixture(&mut app, &fixture, prepared);
        acknowledge_uploads(&app, fixture.root);
        assert!(!finalize_initial(&mut app));
        let batch = app
            .world()
            .get::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0]
            .entity;
        unview_fixture_meshes(&mut app, &fixture, batch);
        observe_patches(&mut app, &fixture, false);
        app.world_mut().entity_mut(batch).insert(SwapPrepared {
            stand_in: None,
            since: 1,
        });
        let unknown = fixture.primitives[0];
        app.init_resource::<StreamingCommitBudget>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, reset_submission_budget)
            .add_systems(PostUpdate, move |mut commands: Commands| {
                commands.entity(unknown).remove::<ViewVisibility>();
            })
            .add_systems(Last, finalize_unviewed_lod_batches);
        app.update();
        assert!(
            app.world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .source_children
                .is_some()
        );
        assert_eq!(
            app.world()
                .resource::<LodStreaming>()
                .terrain_batch_activated_chunks,
            0
        );
    }

    #[test]
    fn owned_prepared_offscreen_batches_retire_only_after_source_region_observation() {
        use crate::swap_bridge::{SwapCoverageRoot, SwapPrepared};
        for observed in [None, Some(true), Some(false)] {
            let mut app = test_app();
            enable_cluster_fallback(&mut app);
            let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
            ready_source(&mut app, &fixture);
            let prepared = prepare(&mut app, &fixture).unwrap();
            install_fixture(&mut app, &fixture, prepared);
            acknowledge_uploads(&app, fixture.root);
            assert!(
                !finalize_initial(&mut app),
                "unobserved hierarchy must remain"
            );
            assert!(
                fixture
                    .patches
                    .iter()
                    .all(|(patch, _)| app.world().get::<SwapCoverageRoot>(*patch).is_some())
            );
            let batch_entity = app
                .world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .batches[0]
                .entity;
            app.world_mut()
                .entity_mut(batch_entity)
                .insert(SwapPrepared {
                    stand_in: None,
                    since: 1,
                });
            if let Some(in_view) = observed {
                observe_patches(&mut app, &fixture, in_view);
            }
            unview_fixture_meshes(&mut app, &fixture, batch_entity);
            assert!(
                !finalize_initial(&mut app),
                "Update cannot use previous-frame invisibility"
            );
            let retired = finalize_unviewed_initial(&mut app);
            assert_eq!(retired, observed == Some(false));
            let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            assert_eq!(chunk.source_children.is_none(), retired);
            assert!(app.world().get::<SwapCoverageRoot>(fixture.root).is_none());
            if retired {
                assert!(
                    fixture
                        .patches
                        .iter()
                        .all(|(patch, _)| app.world().get_entity(*patch).is_err()),
                    "observed patches leave with the hierarchy"
                );
                assert_eq!(chunk.batches[0].fallback_entities.len(), 4);
                assert!(
                    fixture
                        .primitives
                        .iter()
                        .all(|entity| app.world().get_entity(*entity).is_err())
                );
            } else {
                assert!(
                    fixture
                        .primitives
                        .iter()
                        .all(|entity| app.world().get_entity(*entity).is_ok())
                );
            }
        }
    }

    #[test]
    fn partly_viewed_chunk_retires_its_offscreen_batch_on_that_batchs_own_region() {
        // A far chunk across the frustum edge: one owned batch is drawn in view, the other lies
        // wholly offscreen and can never be drawn. The offscreen batch may transfer once its own
        // source patches are unviewed now and unobserved by the renderer; a viewed sibling batch
        // must not hold it forever, and an observed offscreen patch still holds.
        use crate::swap_bridge::{
            SwapCoverageInCameraView, SwapCoverageRoot, SwapDrawReceipt, SwapPrepared,
        };
        for offscreen_observed in [false, true] {
            let mut app = test_app();
            enable_cluster_fallback(&mut app);
            let fixture = fixture(&mut app, &[IVec2::ZERO, IVec2::new(2, 0)], LodTier::Tier4);
            ready_source(&mut app, &fixture);
            let prepared = prepare(&mut app, &fixture).unwrap();
            assert_eq!(prepared.len(), 2, "two batch tiles");
            install_fixture(&mut app, &fixture, prepared);
            acknowledge_uploads(&app, fixture.root);
            assert!(!finalize_initial(&mut app));
            assert!(
                fixture
                    .patches
                    .iter()
                    .all(|(patch, _)| app.world().get::<SwapCoverageRoot>(*patch).is_some()),
                "owned regions are observed per source patch"
            );
            assert!(app.world().get::<SwapCoverageRoot>(fixture.root).is_none());
            let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            let regions: Vec<_> = chunk
                .batches
                .iter()
                .map(|batch| {
                    (
                        batch.entity,
                        SwapDrawReceipt {
                            mesh: batch.mesh.id(),
                            material: batch.state.material.id().untyped(),
                        },
                        batch.members.iter().any(|m| m.coverage.grid == IVec2::ZERO),
                        batch
                            .members
                            .iter()
                            .map(|member| member.source_patch)
                            .collect::<Vec<_>>(),
                    )
                })
                .collect();
            for (batch, receipt, viewed, patches) in regions {
                let visibility = if viewed {
                    ViewVisibility::VISIBLE
                } else {
                    ViewVisibility::default()
                };
                app.world_mut().entity_mut(batch).insert(visibility);
                if viewed {
                    app.world_mut().entity_mut(batch).insert(receipt);
                } else {
                    app.world_mut().entity_mut(batch).insert(SwapPrepared {
                        stand_in: None,
                        since: 1,
                    });
                }
                for patch in patches {
                    app.world_mut()
                        .entity_mut(patch)
                        .insert(SwapCoverageInCameraView(viewed || offscreen_observed));
                    let primitive = app.world().get::<Children>(patch).unwrap()[0];
                    app.world_mut().entity_mut(primitive).insert(visibility);
                }
            }
            let retired = finalize_unviewed_initial(&mut app);
            assert_eq!(retired, !offscreen_observed);
            assert_eq!(
                app.world()
                    .get::<LodTerrainBatches>(fixture.root)
                    .unwrap()
                    .source_children
                    .is_none(),
                retired
            );
        }
    }

    #[test]
    fn offscreen_initial_retirement_requires_current_batch_asset_identity() {
        use crate::swap_bridge::SwapPrepared;
        let mut app = test_app();
        enable_cluster_fallback(&mut app);
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        ready_source(&mut app, &fixture);
        let prepared = prepare(&mut app, &fixture).unwrap();
        install_fixture(&mut app, &fixture, prepared);
        acknowledge_uploads(&app, fixture.root);
        assert!(!finalize_initial(&mut app));
        let batch_entity = app
            .world()
            .get::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0]
            .entity;
        observe_patches(&mut app, &fixture, false);
        assert!(
            !finalize_initial(&mut app),
            "unviewed upload without preparation still holds"
        );
        app.world_mut()
            .entity_mut(batch_entity)
            .insert(SwapPrepared {
                stand_in: None,
                since: 1,
            });
        unview_fixture_meshes(&mut app, &fixture, batch_entity);
        let current = app.world().get::<Mesh3d>(batch_entity).unwrap().0.clone();
        app.world_mut()
            .entity_mut(batch_entity)
            .insert(Mesh3d(Handle::default()));
        assert!(
            !finalize_unviewed_initial(&mut app),
            "an old preparation cannot cover a changed mesh"
        );
        app.world_mut()
            .entity_mut(batch_entity)
            .insert(Mesh3d(current));
        unview_fixture_meshes(&mut app, &fixture, batch_entity);
        assert!(!finalize_initial(&mut app));
        assert!(finalize_unviewed_initial(&mut app));
    }

    #[test]
    fn partial_cluster_inventory_retains_only_owned_batch_members() {
        let mut app = test_app();
        enable_cluster_fallback(&mut app);
        let mixed = fixture(&mut app, &[IVec2::ZERO, IVec2::new(1, 0)], LodTier::Tier4);
        let unrelated = fixture(&mut app, &[IVec2::new(4, 0)], LodTier::Tier4);
        for fixture in [&mixed, &unrelated] {
            ready_source(&mut app, fixture);
            let prepared = prepare(&mut app, fixture).unwrap();
            install_fixture(&mut app, fixture, prepared);
            acknowledge_uploads(&app, fixture.root);
        }
        // Only the owned mixed batch waits for a draw; another chunk in that world transfers
        // on upload proof and acquires no persistent compact source copies.
        assert!(finalize_initial(&mut app));
        assert!(
            app.world()
                .get::<LodTerrainBatches>(unrelated.root)
                .unwrap()
                .source_children
                .is_none()
        );
        assert!(
            app.world()
                .get::<LodTerrainBatches>(unrelated.root)
                .unwrap()
                .batches
                .iter()
                .all(|batch| batch.fallback_entities.is_empty())
        );
        assert!(
            app.world()
                .get::<LodTerrainBatches>(mixed.root)
                .unwrap()
                .source_children
                .is_some()
        );
        acknowledge_uploads_and_draws(&mut app, mixed.root);
        assert!(finalize_initial(&mut app));
        let chunk = app.world().get::<LodTerrainBatches>(mixed.root).unwrap();
        assert_eq!(
            chunk
                .batches
                .iter()
                .map(|batch| batch.fallback_entities.len())
                .sum::<usize>(),
            4
        );
        let fallback_coverage: Vec<_> = chunk
            .render_surfaces()
            .into_iter()
            .filter(|(_, _, _, source)| *source)
            .flat_map(|(_, _, coverage, _)| coverage)
            .collect();
        assert_eq!(fallback_coverage.len(), 4);
        assert!(
            fallback_coverage
                .iter()
                .all(|coverage| coverage.grid == IVec2::ZERO)
        );
        // Newly selected unowned coverage still gets a temporary upload fallback, then drains.
        app.world_mut()
            .run_system_once(|mut batches: LodBatchVisibility| {
                batches.update(&HashMap::from([(
                    (IVec2::new(1, 0), 0),
                    Some(LodTier::Tier4),
                )]));
            })
            .unwrap();
        acknowledge_uploads(&app, mixed.root);
        app.world_mut()
            .run_system_once(|mut batches: LodBatchVisibility| batches.finish_uploads())
            .unwrap();
        assert!(
            app.world()
                .get::<LodTerrainBatches>(mixed.root)
                .unwrap()
                .batches
                .iter()
                .all(|batch| batch.fallback_entities.is_empty())
        );
    }

    #[test]
    fn cluster_settings_without_world_ownership_keep_stock_transfer() {
        for inventory in [0, 1, 2, 3] {
            let mut app = test_app();
            enable_cluster_fallback(&mut app);
            match inventory {
                0 => {
                    app.world_mut()
                        .remove_resource::<crate::cluster::terrain::ClusterTerrain>();
                }
                1 => {
                    app.world_mut()
                        .resource_mut::<crate::cluster::terrain::ClusterTerrain>()
                        .blocks
                        .clear();
                }
                2 => {
                    app.world_mut()
                        .resource_mut::<crate::cluster::terrain::ClusterTerrain>()
                        .blocks[0]
                        .worldspace_id += 1;
                }
                _ => {
                    app.world_mut()
                        .resource_mut::<crate::cluster::terrain::ClusterTerrain>()
                        .blocks[0]
                        .batch = None;
                }
            }
            let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
            ready_source(&mut app, &fixture);
            let prepared = prepare(&mut app, &fixture).unwrap();
            install_fixture(&mut app, &fixture, prepared);
            acknowledge_uploads(&app, fixture.root);
            assert!(finalize_initial(&mut app));
            let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            assert!(chunk.source_children.is_none());
            assert!(
                chunk
                    .batches
                    .iter()
                    .all(|batch| batch.fallback_entities.is_empty())
            );
        }
    }

    #[test]
    fn stock_lod_far_batches_drain_without_camera_draw_receipts() {
        let mut app = test_app();
        app.insert_resource(EngineConfig::default());
        let mut roots = Vec::new();
        // Many demand-selected chunks can be behind the camera. GPU upload completes for all;
        // actual camera draws are deliberately absent, as in a settled stock lod-far view.
        for index in 0..97 {
            let fixture = fixture(&mut app, &[IVec2::new(index * 2, 0)], LodTier::Tier4);
            ready_source(&mut app, &fixture);
            let prepared = prepare(&mut app, &fixture).unwrap();
            install_fixture(&mut app, &fixture, prepared);
            acknowledge_uploads(&app, fixture.root);
            roots.push(fixture.root);
        }
        let pending = |app: &mut App| {
            app.world_mut()
                .run_system_once(|batches: LodBatchVisibility| {
                    (
                        batches.holding_source_count(),
                        batches.pending_selection_uploads(),
                    )
                })
                .unwrap()
        };
        assert_eq!(pending(&mut app), (97, 0));
        for _ in &roots {
            assert!(finalize_initial(&mut app));
        }
        assert!(!finalize_initial(&mut app));
        assert_eq!(pending(&mut app), (0, 0));
        for &root in &roots {
            let chunk = app.world().get::<LodTerrainBatches>(root).unwrap();
            assert!(chunk.source_children.is_none());
            for batch in &chunk.batches {
                assert!(!batch.pending_upload);
                assert!(batch.fallback_entities.is_empty());
                assert!(
                    app.world()
                        .get::<crate::swap_bridge::SwapDrawReceipt>(batch.entity)
                        .is_none()
                );
                assert_eq!(
                    app.world().get::<Visibility>(batch.entity),
                    Some(&Visibility::Inherited)
                );
            }
        }
        // A changed mask still gets temporary exact source coverage until its immutable mesh
        // upload completes, then releases that fallback just as the upstream stock path does.
        app.world_mut()
            .run_system_once(|mut batches: LodBatchVisibility| {
                batches.update(&HashMap::from([((IVec2::ZERO, 0), Some(LodTier::Tier4))]));
            })
            .unwrap();
        assert_eq!(pending(&mut app), (0, 1));
        assert_eq!(
            app.world()
                .get::<LodTerrainBatches>(roots[0])
                .unwrap()
                .batches[0]
                .fallback_entities
                .len(),
            1
        );
        acknowledge_uploads(&app, roots[0]);
        app.world_mut()
            .run_system_once(|mut batches: LodBatchVisibility| batches.finish_uploads())
            .unwrap();
        assert_eq!(pending(&mut app), (0, 0));
        assert!(
            app.world()
                .get::<LodTerrainBatches>(roots[0])
                .unwrap()
                .batches[0]
                .fallback_entities
                .is_empty()
        );
    }

    #[test]
    fn upload_reveals_before_exact_draw_and_keeps_resident_sources() {
        let mut app = test_app();
        enable_cluster_fallback(&mut app);
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        ready_source(&mut app, &fixture);
        let prepared = prepare(&mut app, &fixture).unwrap();
        install_fixture(&mut app, &fixture, prepared);
        acknowledge_uploads(&app, fixture.root);
        assert!(!finalize_initial(&mut app), "upload cannot retire source");
        let (entity, mesh, material) = {
            let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
            let batch = &chunk.batches[0];
            assert!(chunk.source_children.is_some());
            (
                batch.entity,
                batch.mesh.id(),
                batch.state.material.id().untyped(),
            )
        };
        assert_eq!(
            *app.world().get::<Visibility>(entity).unwrap(),
            Visibility::Inherited,
            "revealed before receipt: no Hidden/SwapDrawn dependency cycle"
        );
        app.world_mut()
            .entity_mut(entity)
            .insert(crate::swap_bridge::SwapDrawn);
        assert!(
            !finalize_initial(&mut app),
            "historical marker has no exact asset identity"
        );
        app.world_mut()
            .entity_mut(entity)
            .insert(crate::swap_bridge::SwapDrawReceipt {
                mesh: Handle::<Mesh>::default().id(),
                material,
            });
        assert!(
            !finalize_initial(&mut app),
            "receipt for another immutable mesh cannot retire source"
        );
        // Synthetic protocol fixture only, not executed GPU/pixel evidence.
        app.world_mut()
            .entity_mut(entity)
            .insert(crate::swap_bridge::SwapDrawReceipt { mesh, material });
        assert!(finalize_initial(&mut app));
        let chunk = app.world().get::<LodTerrainBatches>(fixture.root).unwrap();
        assert!(chunk.source_children.is_none());
        assert_eq!(
            chunk.batches[0].fallback_entities.len(),
            4,
            "compact source fallback stays extracted after hierarchy retirement"
        );
    }
    // This is a rendered regression, not a receipt-injection protocol fixture.
    // Run alone through the parent's GPU/build gate. A missing renderer is a failure.
    #[test]
    fn rendered_complement_finishes_own_draws_before_dormancy_and_rearms_new_masks() {
        use crate::{
            streaming::terrain_handoff::{
                ClusterCoverage, ClusterViewCoverage, TerrainHandoffSystems,
            },
            swap_bridge::{SwapDrawReceipt, SwapInCameraView, SwapWaiting},
        };
        use bevy::{
            app::TerminalCtrlCHandlerPlugin,
            audio::AudioPlugin,
            camera::{RenderTarget, visibility::NoFrustumCulling},
            core_pipeline::prepass::DepthPrepass,
            gilrs::GilrsPlugin,
            log::LogPlugin,
            render::{
                Render, RenderApp, RenderPlugin, RenderSystems,
                pipelined_rendering::PipelinedRenderingPlugin,
                render_resource::TextureFormat,
                view::{ExtractedView, RenderVisibleEntities},
            },
            window::WindowPlugin,
            winit::WinitPlugin,
        };

        // Seed the cluster selector's output boundary, never stock readiness or
        // receipts. The production filter still evaluates actual queued stock.
        // Actual cluster shader/plan qualification remains a separate GPU gate.
        #[derive(Resource, Default)]
        struct SelectedClusterCells(Option<(u32, IVec2)>);
        fn selected_cells(
            selected: Res<SelectedClusterCells>,
            mut coverage: ResMut<ClusterCoverage>,
            views: Query<&ExtractedView>,
        ) {
            coverage.views.clear();
            if let Some(cell) = selected.0 {
                for view in &views {
                    coverage.views.insert(
                        view.retained_view_entity,
                        ClusterViewCoverage {
                            generation: 1,
                            layout: 1,
                            plan: 1,
                            cells: HashSet::from([cell]),
                        },
                    );
                }
            }
        }
        fn step_until(app: &mut App, label: &str, done: impl Fn(&App) -> bool) {
            for _ in 0..256 {
                app.update();
                if done(app) {
                    return;
                }
            }
            panic!("rendered terrain regression did not complete: {label}");
        }
        fn phase_state(app: &mut App, entity: Entity) -> (bool, bool) {
            app.sub_app_mut(RenderApp)
                .world_mut()
                .run_system_once(
                    move |views: Query<(&ExtractedView, &RenderVisibleEntities)>,
                          phases: crate::swap_bridge::systems::QueuePhases| {
                        let main = entity.into();
                        let mut raw = false;
                        let mut queued = false;
                        for (view, visible) in &views {
                            raw |= visible.get::<Mesh3d>().is_some_and(|class| {
                                class
                                    .iter_visible()
                                    .any(|(_, candidate)| *candidate == main)
                            });
                            queued |= phases.forward(&view.retained_view_entity).queued(main);
                        }
                        (raw, queued)
                    },
                )
                .unwrap()
        }
        fn first_queue(app: &mut App, label: &str, entities: &[Entity]) {
            // Preparation may span frames. Only a real phase entry satisfies
            // this predicate; the test budget never grants a draw receipt.
            for _ in 0..256 {
                app.update();
                if entities
                    .iter()
                    .all(|entity| phase_state(app, *entity) == (true, true))
                {
                    return;
                }
            }
            panic!("waiting terrain never reached its own queue: {label}");
        }
        fn no_waiting(app: &App, entities: &[Entity]) -> bool {
            entities.iter().all(|entity| {
                app.world().get::<SwapWaiting>(*entity).is_none()
                    && app.world().get::<SwapDrawReceipt>(*entity).is_some()
            })
        }
        fn pending(app: &mut App) -> usize {
            app.world_mut()
                .run_system_once(|waiting: crate::swap_bridge::WaitingVisibility| {
                    crate::swap_bridge::pending_drawn(&waiting)
                })
                .unwrap()
        }

        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .build()
                .disable::<WinitPlugin>()
                .disable::<LogPlugin>()
                .disable::<TerminalCtrlCHandlerPlugin>()
                .disable::<AudioPlugin>()
                .disable::<GilrsPlugin>()
                .disable::<PipelinedRenderingPlugin>()
                .set(RenderPlugin {
                    synchronous_pipeline_compilation: true,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: None,
                    ..default()
                }),
        )
        .insert_resource(EngineConfig {
            stream_radius: 0,
            cluster_terrain: true,
            clusters: Some(String::new()),
            ..default()
        })
        .insert_resource(RenderOrigin(IVec2::ZERO))
        .init_resource::<LodStreaming>()
        .init_resource::<StreamingMetrics>()
        .init_resource::<ProfilingState>()
        .init_resource::<Assets<WorldAsset>>()
        .add_plugins((
            crate::terrain_upload::TerrainUploadPlugin,
            crate::swap_bridge::SwapBridgePlugin,
            crate::streaming::terrain_handoff::TerrainHandoffPlugin,
        ));
        enable_cluster_fallback(&mut app);
        crate::streaming::identity_changes::install_terrain(app.world_mut());
        app.sub_app_mut(RenderApp)
            .init_resource::<SelectedClusterCells>()
            .add_systems(
                Render,
                selected_cells
                    .in_set(RenderSystems::Queue)
                    .in_set(TerrainHandoffSystems::Cluster),
            );
        app.finish();
        app.cleanup();
        let target =
            app.world_mut()
                .resource_mut::<Assets<Image>>()
                .add(Image::new_target_texture(
                    64,
                    64,
                    TextureFormat::Bgra8UnormSrgb,
                    None,
                ));
        app.world_mut().spawn((
            Camera3d::default(),
            RenderTarget::Image(target.into()),
            DepthPrepass,
            Transform::from_xyz(0.0, CELL_SIZE * 4.0, CELL_SIZE * 2.0)
                .looking_at(Vec3::ZERO, Vec3::Y),
            Projection::Perspective(PerspectiveProjection {
                far: CELL_SIZE * 32.0,
                ..default()
            }),
        ));
        assert!(
            app.world()
                .resource::<TerrainMeshUploadReadiness>()
                .has_render_backend()
        );

        // Cluster fallback enabled, coverage absent: execute reveal, exact receipt, source
        // retirement, compact fallback's own queue/bridge cycle, then dormancy.
        let fixture = fixture(&mut app, &[IVec2::ZERO], LodTier::Tier4);
        ready_source(&mut app, &fixture);
        for &entity in &fixture.primitives {
            app.world_mut().entity_mut(entity).insert(NoFrustumCulling);
        }
        let prepared = prepare(&mut app, &fixture).unwrap();
        install_fixture(&mut app, &fixture, prepared);
        // The source hierarchy is installed explicitly by fixture(), not by a
        // WorldAsset spawner. Avoid spawning a second copy while stepping App.
        app.world_mut()
            .entity_mut(fixture.root)
            .remove::<WorldAssetRoot>();
        let batch_entity = app
            .world()
            .get::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0]
            .entity;
        app.world_mut()
            .entity_mut(batch_entity)
            .insert(NoFrustumCulling);
        step_until(&mut app, "batch upload", |app| {
            let batch = &app
                .world()
                .get::<LodTerrainBatches>(fixture.root)
                .unwrap()
                .batches[0];
            app.world()
                .resource::<TerrainMeshUploadReadiness>()
                .is_ready(batch.mesh.id())
        });
        assert!(
            !finalize_initial(&mut app),
            "upload alone cannot retire hierarchy"
        );
        step_until(&mut app, "initial batch real receipt", |app| {
            no_waiting(app, &[batch_entity])
        });
        assert!(finalize_initial(&mut app));
        let sources = app
            .world()
            .get::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0]
            .fallback_entities
            .clone();
        assert_eq!(sources.len(), 4);
        for &source in &sources {
            app.world_mut().entity_mut(source).insert(NoFrustumCulling);
            assert!(app.world().get::<SwapWaiting>(source).is_some());
            assert!(app.world().get::<SwapDrawReceipt>(source).is_none());
        }
        // Observe the vulnerable frame before the next Extract can issue any
        // receipt: all newly created sources must still occupy their real phases.
        first_queue(&mut app, "compact sources", &sources);
        step_until(&mut app, "compact source receipts", |app| {
            no_waiting(app, &sources)
        });
        for _ in 0..4 {
            app.update();
            assert_eq!(pending(&mut app), 0);
            for &source in &sources {
                assert!(app.world().get_entity(source).is_ok());
                assert_eq!(
                    phase_state(&mut app, source),
                    (true, false),
                    "Cleanup restores raw visibility but dormant source owns no phase"
                );
            }
        }
        // Withdraw the selected batch's real material instance. The next render
        // decision must keep the already prepared compact sources, without reload.
        let material = app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(batch_entity)
            .unwrap()
            .0
            .clone();
        app.world_mut()
            .entity_mut(batch_entity)
            .remove::<MeshMaterial3d<StandardMaterial>>();
        app.update();
        for &source in &sources {
            assert_eq!(phase_state(&mut app, source), (true, true));
        }
        app.world_mut()
            .entity_mut(batch_entity)
            .insert(MeshMaterial3d(material));

        // Current cluster coverage exists before a fresh ordinary ring's first
        // receipt. Both merged and single-quadrant LAND must first draw themselves.
        let worldspace = app.world().resource::<EngineConfig>().worldspace_id;
        let cell = IVec2::new(-3, 0);
        app.sub_app_mut(RenderApp)
            .world_mut()
            .resource_mut::<SelectedClusterCells>()
            .0 = Some((worldspace, cell));
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::default());
        let mut rings = Vec::new();
        for merged in [false, true] {
            let mut entity = app.world_mut().spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                Visibility::Inherited,
                NoFrustumCulling,
                crate::streaming::RingTerrain,
                crate::streaming::TerrainWorldspace(worldspace),
                crate::swap_bridge::SwapSurface::new(material.id().untyped(), []),
                SwapWaiting,
            ));
            if merged {
                entity.insert(crate::streaming::MergedTerrainCoverage { grid: cell });
            } else {
                entity.insert(TerrainCoverage {
                    grid: cell,
                    quadrant: 0,
                    tier: None,
                });
            }
            rings.push(entity.id());
        }
        first_queue(&mut app, "new rings under selected clusters", &rings);
        step_until(
            &mut app,
            "ordinary ring receipts before cluster suppression",
            |app| no_waiting(app, &rings),
        );
        app.update();
        for &ring in &rings {
            assert_eq!(
                phase_state(&mut app, ring),
                (true, false),
                "ring may yield only after its real receipt"
            );
        }

        // Supersede an already-drawn mask through the production selector while
        // clusters are ready. Old receipt must disappear; each new immutable
        // generation must pass the real bridge before it may become dormant.
        app.sub_app_mut(RenderApp)
            .world_mut()
            .resource_mut::<SelectedClusterCells>()
            .0 = Some((worldspace, IVec2::ZERO));
        let change_mask = |app: &mut App, selected: u8| {
            app.world_mut()
                .run_system_once(move |mut batches: LodBatchVisibility| {
                    let tiers: HashMap<_, _> = (0..4)
                        .map(|q| {
                            (
                                (IVec2::ZERO, q),
                                (selected & (1 << q) != 0).then_some(LodTier::Tier4),
                            )
                        })
                        .collect();
                    batches.update(&tiers);
                })
                .unwrap();
        };
        change_mask(&mut app, 0b0111);
        let obsolete = app.world().get::<Mesh3d>(batch_entity).unwrap().0.id();
        assert!(app.world().get::<SwapDrawReceipt>(batch_entity).is_none());
        assert!(app.world().get::<SwapWaiting>(batch_entity).is_some());
        change_mask(&mut app, 0b1111);
        let current = app.world().get::<Mesh3d>(batch_entity).unwrap().0.id();
        assert_ne!(obsolete, current);
        assert!(app.world().get::<SwapDrawReceipt>(batch_entity).is_none());
        step_until(&mut app, "superseding mask upload", |app| {
            app.world()
                .resource::<TerrainMeshUploadReadiness>()
                .is_ready(current)
        });
        // Apply the unchanged mask to activate its now-real upload acknowledgment.
        change_mask(&mut app, 0b1111);
        first_queue(&mut app, "superseding batch mask", &[batch_entity]);
        step_until(&mut app, "new mask receipt under ready clusters", |app| {
            no_waiting(app, &[batch_entity])
        });
        assert_eq!(
            app.world()
                .get::<SwapDrawReceipt>(batch_entity)
                .unwrap()
                .mesh,
            current
        );
        assert!(app.world().get::<SwapInCameraView>(batch_entity).is_none());
        let sources = app
            .world()
            .get::<LodTerrainBatches>(fixture.root)
            .unwrap()
            .batches[0]
            .fallback_entities
            .clone();
        step_until(&mut app, "reselected source receipts", |app| {
            no_waiting(app, &sources)
        });
        app.update();
        assert_eq!(pending(&mut app), 0);

        let next_cell = IVec2::new(2, 0);
        app.sub_app_mut(RenderApp)
            .world_mut()
            .resource_mut::<SelectedClusterCells>()
            .0 = Some((worldspace, next_cell));
        // Use the same source-construction helper after relinquishing the local
        // fixture binding; this is a fresh hierarchy, not a new mask on the old one.
        let next = self::fixture(&mut app, &[next_cell], LodTier::Tier4);
        ready_source(&mut app, &next);
        for &entity in &next.primitives {
            app.world_mut().entity_mut(entity).insert(NoFrustumCulling);
        }
        let prepared = prepare(&mut app, &next).unwrap();
        install_fixture(&mut app, &next, prepared);
        app.world_mut()
            .entity_mut(next.root)
            .remove::<WorldAssetRoot>();
        let next_batch = app
            .world()
            .get::<LodTerrainBatches>(next.root)
            .unwrap()
            .batches[0]
            .entity;
        app.world_mut()
            .entity_mut(next_batch)
            .insert(NoFrustumCulling);
        step_until(
            &mut app,
            "initial batch upload with clusters already selected",
            |app| {
                let batch = &app
                    .world()
                    .get::<LodTerrainBatches>(next.root)
                    .unwrap()
                    .batches[0];
                app.world()
                    .resource::<TerrainMeshUploadReadiness>()
                    .is_ready(batch.mesh.id())
            },
        );
        assert!(!finalize_initial(&mut app));
        assert!(app.world().get::<SwapDrawReceipt>(next_batch).is_none());
        first_queue(
            &mut app,
            "initial batch under selected clusters",
            &[next_batch],
        );
        assert!(
            app.world()
                .get::<LodTerrainBatches>(next.root)
                .unwrap()
                .source_children
                .is_some()
        );
        step_until(
            &mut app,
            "initial batch receipt with clusters already selected",
            |app| no_waiting(app, &[next_batch]),
        );
        assert!(finalize_initial(&mut app));
        let next_sources = app
            .world()
            .get::<LodTerrainBatches>(next.root)
            .unwrap()
            .batches[0]
            .fallback_entities
            .clone();
        step_until(&mut app, "new compact receipts under clusters", |app| {
            no_waiting(app, &next_sources)
        });
        app.update();
        assert_eq!(pending(&mut app), 0);
        for &source in &next_sources {
            assert_eq!(phase_state(&mut app, source), (true, false));
        }
    }
}
