//! Current-frame source and visibility facts for complementary cluster selection.
//! Certification stays at package open; this module never hashes source files per view.
use super::*;
use crate::{
    cluster::{
        package::{LazyBatches, ValidatedPackage},
        reader::RecordKey,
        source_identity::{
            SourceNodePose, SourcePrimitive, SourcePrimitiveMesh, SourcePrimitiveTransform,
            SourceStaticModel,
        },
        stock_visibility::ClassFilter,
    },
    streaming::{ActiveSpace, RenderOrigin},
    world::{
        components::{CellRef, FormId, MeshHandle, StreamedCellKey},
        database::CellKey,
    },
};
use bevy::{
    camera::visibility::RenderLayers,
    ecs::{
        entity::{EntityHashMap, EntityHashSet},
        system::SystemParam,
    },
    pbr::{MaterialBindGroupAllocators, RenderMaterialInstances},
    render::view::{RenderShadowMapVisibleEntities, RenderVisibleEntitiesClass},
};
use std::collections::{BTreeSet, HashSet};

#[path = "destination.rs"]
mod destination;

#[path = "source_fallback_bridge.rs"]
mod source_fallback_bridge;

#[path = "candidate_worlds.rs"]
mod candidate_worlds;
#[path = "source_index.rs"]
mod source_index;
use candidate_worlds::{CandidateInputs, CandidateWorlds, candidate_worlds};

type StockMeshCandidate = (AssetId<Mesh>, MainEntity);

#[derive(Resource, Default)]
pub(super) struct Sources {
    /// Every selection input change, including stock terrain readiness arrivals.
    version: u64,
    /// Eligibility/composition changes; only monotonic far terrain additions are
    /// exempt. Withdrawals and changed identities still revoke old reports.
    candidate_version: u64,
    candidate_worlds: CandidateWorlds,
    certificate: Option<Arc<super::super::reader::CertifiedPackage>>,
    present: BTreeSet<u32>,
    /// Complete, unchanged stock sources hidden ONLY by their owning cell's
    /// streaming retirement. History/bindings remain for material proof/revival.
    retired_refs: HashSet<u32>,
    /// Any known stock source hidden by streaming retirement, even if current
    /// source qualification fails. Such a source is never drawable stock proof.
    retired_present: HashSet<u32>,
    retired_materials: HashMap<MainEntity, Option<bevy::asset::UntypedAssetId>>,
    retired_eligibility: HashMap<MainEntity, RetiredEligibility>,
    expected_primitives: HashMap<u32, BTreeSet<u32>>,
    seen_roots: HashMap<Entity, BTreeSet<u32>>,
    qualification_refreshes: u64,
    /// Candidate dependencies include duplicate ordinals: invalidating one duplicate can
    /// make its remaining sibling uniquely bindable. Rebuilt only with ECS proof.
    stock_meshes: HashMap<AssetId<Mesh>, HashSet<(u32, u32)>>,
    stock_candidates: HashMap<(u32, u32), Vec<StockMeshCandidate>>,
    stock_members: HashMap<u32, Vec<MainEntity>>,
    proof_keys: HashMap<u32, HashSet<(u32, u32)>>,
    template_meshes: TemplateMeshDependencies,
    last_work: SourceExtractionWork,
    #[cfg(test)]
    force_full: bool,
    terrain: Arc<HashMap<MainEntity, crate::streaming::TerrainPrimitiveIdentity>>,
    /// Raw main-world eligibility, independent of frustum/ViewVisibility and GPU
    /// preparation. Withdrawing an old visible source revokes prior stock proof.
    terrain_visible: HashSet<MainEntity>,
    pub(super) origin: IVec2,
    bound: HashMap<RecordKey, MainEntity>,
    /// Valid instantiated source ordinals, including expected primitives rejected
    /// by the cluster builder. Unique per reference/ordinal, not per mesh hash.
    stock_primitives: HashMap<(u32, u32), MainEntity>,
    templates: Vec<bool>,
    normal_maps: Vec<bool>,
    /// Actual certified stock attribute, including retained glTF-generated tangents.
    stock_tangents: Vec<Option<bool>>,
    pub(super) layout_version: u64,
    lazy_layout_scope: Option<u64>,
    fixture_layout_scope: bool,
    /// Definitive global template/material refusal, separate from loading false values.
    source_fallback_batches: BTreeSet<u32>,
    pub(super) worlds: HashMap<MainEntity, Option<u32>>,
    /// None is an unknown local-light dependency domain, distinct from proven empty.
    light_dependencies: HashMap<MainEntity, Option<HashSet<MainEntity>>>,
    invalid_meshes: HashSet<AssetId<Mesh>>,
    record_lookup: HashMap<(u32, u32), RecordKey>,
    template_cache: HashMap<AssetId<WorldAsset>, bool>,
    invalid_templates: HashSet<AssetId<WorldAsset>>,
    geometry_cache:
        HashMap<bevy::asset::UntypedAssetId, super::super::material_geometry::GeometryRequirements>,
    /// Which batches each cached material and template decision was made for, so a cache entry an
    /// asset event drops sends exactly those batches back through [`template_row`].
    template_rows: TemplateRows,
    template_inputs: Vec<(
        Option<bevy::asset::UntypedAssetId>,
        Option<AssetId<WorldAsset>>,
    )>,
}

/// Dependencies belong to the loaded template, including every mesh of a refused template.
/// Collect them independently of validation; otherwise an early refusal hides dependencies.
#[derive(Default)]
struct TemplateMeshDependencies {
    by_scene: HashMap<AssetId<WorldAsset>, HashSet<AssetId<Mesh>>>,
    by_mesh: HashMap<AssetId<Mesh>, HashSet<AssetId<WorldAsset>>>,
    /// Ambiguous ordinal domains refuse layout proof. All meshes remain in the
    /// reverse graph, including duplicate ordinals and refused templates.
    by_ordinal: HashMap<AssetId<WorldAsset>, HashMap<u32, AssetId<Mesh>>>,
}

impl TemplateMeshDependencies {
    fn register(&mut self, scene: AssetId<WorldAsset>, asset: &WorldAsset) {
        self.remove(scene);
        let mut meshes = HashSet::new();
        let mut bindings = HashMap::new();
        let mut duplicate = false;
        // Exactly template_valid's mesh domain and identity readability. Making
        // SourcePrimitive a required query input could admit a disabled entity
        // that template_valid never inspected.
        if let Some(mut query) = asset.world.try_query::<(Entity, &Mesh3d)>() {
            for (entity, mesh) in query.iter(&asset.world) {
                meshes.insert(mesh.0.id());
                if let Some(tag) = asset.world.get::<SourcePrimitive>(entity) {
                    duplicate |= bindings.insert(tag.ordinal, mesh.0.id()).is_some();
                }
            }
        }
        if duplicate {
            bindings.clear();
        }
        self.by_ordinal.insert(scene, bindings);
        for &mesh in &meshes {
            self.by_mesh.entry(mesh).or_default().insert(scene);
        }
        self.by_scene.insert(scene, meshes);
    }

    fn remove(&mut self, scene: AssetId<WorldAsset>) {
        self.by_ordinal.remove(&scene);
        if let Some(meshes) = self.by_scene.remove(&scene) {
            for mesh in meshes {
                if let Some(scenes) = self.by_mesh.get_mut(&mesh) {
                    scenes.remove(&scene);
                    if scenes.is_empty() {
                        self.by_mesh.remove(&mesh);
                    }
                }
            }
        }
    }
}

/// Work actually done in one extraction, separate from the invalidation event counts.
/// Kept in all builds so tests and ordinary INFO logs attribute work without Tracy.
#[derive(Clone, Copy, Debug, Default)]
struct SourceExtractionWork {
    scene_events: u64,
    material_events: u64,
    mesh_modified: u64,
    mesh_removed: u64,
    scene_modified: u64,
    scene_removed: u64,
    stock_rows_revoked: u64,
    stock_rows_requalified: u64,
    roots_scanned: u64,
    meshes_scanned: u64,
    primitives_scanned: u64,
    template_rows: u64,
    qualification_full: bool,
    affected_references: u64,
    journal_entities: u64,
    journal_events: u64,
    journal_complete: bool,
    retired_proof_keys_visited: u64,
    subjects_updated: u64,
    ancestry_edges: u64,
    discovery_roots: u64,
    discovery_meshes: u64,
    indexed_subjects: u64,
    indexed_nodes: u64,
    indexed_reference_groups: u64,
}

// The caller supplies one affected reference's proof keys, never the global map.
fn retired_reference_proof(
    keys: Option<&HashSet<(u32, u32)>>,
    stock: &HashMap<(u32, u32), MainEntity>,
    work: &mut SourceExtractionWork,
) -> (HashSet<MainEntity>, BTreeSet<u32>) {
    let mut proven = HashSet::new();
    let mut instantiated = BTreeSet::new();
    if let Some(keys) = keys {
        for key in keys {
            work.retired_proof_keys_visited += 1;
            if let Some(&entity) = stock.get(key) {
                proven.insert(entity);
                instantiated.insert(key.1);
            }
        }
    }
    (proven, instantiated)
}

/// The batches behind each cache entry [`template_row`] reads, and those it could not settle yet.
#[derive(Default)]
struct TemplateRows {
    /// Every batch must be decided again (first pass, new handles, a new certificate,
    /// or a scope reset). Mesh events instead use template -> mesh dependencies.
    stale: bool,
    by_material: HashMap<bevy::asset::UntypedAssetId, BTreeSet<u32>>,
    by_scene: HashMap<AssetId<WorldAsset>, BTreeSet<u32>>,
    /// Batches whose material or template asset is still loading: decided again every frame.
    pending: BTreeSet<u32>,
    fixture_by_mesh: HashMap<AssetId<Mesh>, BTreeSet<u32>>,
    fixture_by_row: HashMap<u32, AssetId<Mesh>>,
}

impl TemplateRows {
    fn remove(
        &mut self,
        index: u32,
        input: (
            Option<bevy::asset::UntypedAssetId>,
            Option<AssetId<WorldAsset>>,
        ),
    ) {
        self.pending.remove(&index);
        if let Some(mesh) = self.fixture_by_row.remove(&index)
            && let Some(rows) = self.fixture_by_mesh.get_mut(&mesh)
        {
            rows.remove(&index);
            if rows.is_empty() {
                self.fixture_by_mesh.remove(&mesh);
            }
        }
        if let Some(id) = input.0
            && let Some(rows) = self.by_material.get_mut(&id)
        {
            rows.remove(&index);
            if rows.is_empty() {
                self.by_material.remove(&id);
            }
        }
        if let Some(id) = input.1
            && let Some(rows) = self.by_scene.get_mut(&id)
        {
            rows.remove(&index);
            if rows.is_empty() {
                self.by_scene.remove(&id);
            }
        }
    }
}

/// One batch's template decision: whether its material needs tangents, whether its source
/// template is valid, whether it is definitively refused (`fallback`), and whether any of that is
/// still waiting on a loading asset (`pending`).
#[derive(Clone, Copy, Default)]
struct TemplateRow {
    normal_map: bool,
    stock_tangents: Option<bool>,
    template: bool,
    fallback: bool,
    pending: bool,
}

/// The parts of [`Sources`] a [`template_row`] reads and fills.
struct TemplateCaches<'a> {
    geometry: &'a mut HashMap<
        bevy::asset::UntypedAssetId,
        super::super::material_geometry::GeometryRequirements,
    >,
    templates: &'a mut HashMap<AssetId<WorldAsset>, bool>,
    template_meshes: &'a mut TemplateMeshDependencies,
    invalid_templates: &'a HashSet<AssetId<WorldAsset>>,
    invalid_meshes: &'a HashSet<AssetId<Mesh>>,
    rows: &'a mut TemplateRows,
}

/// Borrowed qualification inputs; no additional per-frame source traversal.
struct TemplateAssets<'a> {
    standards: &'a Assets<StandardMaterial>,
    lazy: Option<&'a LazyBatches>,
    scenes: Option<&'a Assets<WorldAsset>>,
    meshes: &'a Assets<Mesh>,
    fixtures: Option<&'a ClusterBatchStockMeshes>,
}

fn template_row(
    index: u32,
    handle: &bevy::asset::UntypedHandle,
    caches: TemplateCaches<'_>,
    inputs: &TemplateAssets<'_>,
) -> TemplateRow {
    let TemplateAssets {
        standards,
        lazy,
        scenes: assets,
        ..
    } = *inputs;
    let id = handle.id();
    caches.rows.by_material.entry(id).or_default().insert(index);
    let geometry = caches.geometry.get(&id).copied().or_else(|| {
        let material = if id.type_id() == std::any::TypeId::of::<StandardMaterial>() {
            standards.get(id.typed::<StandardMaterial>())
        } else {
            None
        }?;
        let requirements = super::super::material_geometry::requirements(material);
        caches.geometry.insert(id, requirements);
        Some(requirements)
    });
    let Some(geometry) = geometry else {
        return TemplateRow {
            pending: true,
            ..default()
        };
    };
    if !geometry.supported {
        return TemplateRow {
            fallback: true,
            ..default()
        };
    }
    let mut row = TemplateRow {
        normal_map: geometry.tangents,
        ..default()
    };
    let Some(source) = lazy.and_then(|lazy| lazy.source_scene(index)) else {
        // Standalone fixtures provide the very same Mesh handle stock would draw.
        // No MCL flag or material-derived layout is accepted as a substitute.
        if let Some(mesh) = inputs
            .fixtures
            .and_then(|sources| sources.0.get(index as usize))
        {
            caches
                .rows
                .fixture_by_mesh
                .entry(mesh.id())
                .or_default()
                .insert(index);
            caches.rows.fixture_by_row.insert(index, mesh.id());
            row.stock_tangents = inputs
                .meshes
                .get(mesh)
                .map(|mesh| mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT));
            row.pending = row.stock_tangents.is_none();
            row.fallback = caches.invalid_meshes.contains(&mesh.id())
                || row
                    .stock_tangents
                    .is_some_and(|present| geometry.tangents && !present);
            if row.fallback {
                row.stock_tangents = None;
            }
        }
        row.normal_map |= row.stock_tangents == Some(true);
        return row;
    };
    caches
        .rows
        .by_scene
        .entry(source.id())
        .or_default()
        .insert(index);
    if caches.invalid_templates.contains(&source.id()) {
        row.fallback = true;
        return row;
    }
    let valid = match caches.templates.get(&source.id()).copied() {
        Some(valid) => valid,
        None => {
            let asset = assets.and_then(|assets| assets.get(source));
            let valid = asset.is_some_and(|asset| template_valid(asset, caches.invalid_meshes));
            // An absent/loading template must be retried; only a loaded
            // asset has an event-scoped result that can be retained.
            if let Some(asset) = asset {
                caches.template_meshes.register(source.id(), asset);
                caches.templates.insert(source.id(), valid);
            } else {
                row.pending = true;
            }
            valid
        }
    };
    row.template = valid;
    if valid {
        // Validation and ordinal binding share the same event-scoped graph.
        let mesh = lazy
            .and_then(|lazy| lazy.source_primitive(index))
            .and_then(|ordinal| {
                caches
                    .template_meshes
                    .by_ordinal
                    .get(&source.id())?
                    .get(&ordinal)
            });
        row.stock_tangents = mesh
            .and_then(|id| inputs.meshes.get(*id))
            .map(|mesh| mesh.contains_attribute(Mesh::ATTRIBUTE_TANGENT));
        row.pending |= mesh.is_some() && row.stock_tangents.is_none();
        row.fallback |= mesh.is_none()
            || row
                .stock_tangents
                .is_some_and(|present| geometry.tangents && !present);
        if row.fallback || row.pending {
            row.template = false;
            row.stock_tangents = None;
        }
    }
    row.normal_map |= row.stock_tangents == Some(true);
    if !valid && caches.templates.get(&source.id()) == Some(&false) {
        row.fallback = true;
    }
    row
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerrainMapChange {
    Unchanged,
    FarAdditionsOnly,
    Invalidating,
}

fn terrain_map_change(
    previous: &HashMap<MainEntity, crate::streaming::TerrainPrimitiveIdentity>,
    current: &HashMap<MainEntity, crate::streaming::TerrainPrimitiveIdentity>,
) -> TerrainMapChange {
    // The common unchanged case performs exactly one equality comparison; do
    // not first compare in the extractor and then repeat it in this classifier.
    if previous == current {
        return TerrainMapChange::Unchanged;
    }
    if previous
        .iter()
        .any(|(entity, identity)| current.get(entity) != Some(identity))
        || current
            .iter()
            .any(|(entity, identity)| !previous.contains_key(entity) && identity.tier.is_none())
    {
        TerrainMapChange::Invalidating
    } else {
        TerrainMapChange::FarAdditionsOnly
    }
}

/// Equality is checked once for unchanged inputs; only changed sets require
/// the additional withdrawal test. No count/hash stands in for exact members.
fn terrain_visibility_change(
    previous: &HashSet<MainEntity>,
    current: &HashSet<MainEntity>,
) -> (bool, bool) {
    let changed = previous != current;
    (changed, changed && !previous.is_subset(current))
}

fn collect_terrain_visibility(
    terrain: &HashMap<MainEntity, crate::streaming::TerrainPrimitiveIdentity>,
    visibility: &Query<(&Visibility, &InheritedVisibility)>,
) -> HashSet<MainEntity> {
    terrain
        .keys()
        .copied()
        .filter(|entity| {
            visibility
                .get(entity.id())
                .is_ok_and(|(explicit, inherited)| {
                    *explicit != Visibility::Hidden && inherited.get()
                })
        })
        .collect()
}

impl Sources {
    pub(super) fn stock_tangents(&self, batch: usize) -> Option<bool> {
        self.stock_tangents.get(batch).copied().flatten()
    }

    /// Mesh invalidity is sticky in the existing certification contract. Reclassify exactly
    /// the affected ordinals, including a duplicate becoming uniquely bindable.
    /// No root/presence/primitive query is needed. Keep the old snapshot until candidate_worlds
    /// has compared it with the new one, just like ordinary qualification.
    fn invalidate_stock_meshes(
        &mut self,
        meshes: &HashSet<AssetId<Mesh>>,
        work: &mut SourceExtractionWork,
    ) -> Option<QualifiedFacts> {
        let affected: HashSet<_> = meshes
            .iter()
            .filter_map(|mesh| self.stock_meshes.remove(mesh))
            .flatten()
            .collect();
        if affected.is_empty() {
            return None;
        }
        let mut facts = QualifiedFacts {
            present: self.present.clone(),
            retired_refs: self.retired_refs.clone(),
            retired_present: self.retired_present.clone(),
            retired_materials: self.retired_materials.clone(),
            retired_eligibility: self.retired_eligibility.clone(),
            bound: self.bound.clone(),
            stock_primitives: self.stock_primitives.clone(),
        };
        for primitive in affected {
            let winner = self
                .stock_candidates
                .get_mut(&primitive)
                .and_then(|candidates| {
                    candidates.retain(|(mesh, _)| !meshes.contains(mesh));
                    (candidates.len() == 1).then(|| candidates[0].1)
                });
            let previous = facts.stock_primitives.remove(&primitive);
            work.stock_rows_revoked += u64::from(previous.is_some() && previous != winner);
            work.stock_rows_requalified += u64::from(winner.is_some() && previous != winner);
            if let Some(entity) = winner {
                facts.stock_primitives.insert(primitive, entity);
            }
            if let Some(key) = self.record_lookup.get(&primitive) {
                facts.bound.remove(key);
                if let Some(entity) = winner {
                    facts.bound.insert(*key, entity);
                }
            }
            // A single invalid ordinal revokes the complete retired-reference exception,
            // including the material/layer facts of its other, still valid primitives.
            if previous != winner
                && facts.retired_refs.remove(&primitive.0)
                && let Some(members) = self.stock_members.get(&primitive.0)
            {
                for member in members {
                    facts.retired_materials.remove(member);
                    facts.retired_eligibility.remove(member);
                }
            }
        }
        self.qualification_refreshes += 1;
        Some(facts)
    }

    fn record_change(&mut self, candidate_changed: bool) {
        self.version = self.version.wrapping_add(1);
        if candidate_changed {
            // Unscoped callers fail closed. Extraction replaces this with an
            // exact delta only after comparing every old/new candidate input.
            self.candidate_worlds = CandidateWorlds {
                all: true,
                ..default()
            };
            self.candidate_version = self.candidate_version.wrapping_add(1);
        }
    }
}

/// Actual primitive properties, not inherited/root approximations.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RetiredEligibility {
    layers: RenderLayers,
    casts_shadows: bool,
}

fn retired_allowed_for_view(
    sources: &HashMap<MainEntity, RetiredEligibility>,
    view_layers: &RenderLayers,
    shadow: bool,
) -> bevy::platform::collections::HashSet<MainEntity> {
    sources
        .iter()
        .filter_map(|(main, source)| {
            (source.layers.intersects(view_layers) && (!shadow || source.casts_shadows))
                .then_some(*main)
        })
        .collect()
}

type NodeState = (
    &'static Transform,
    Option<&'static SourceNodePose>,
    Option<&'static ChildOf>,
    Option<&'static bevy::gltf::GltfExtras>,
    Has<crate::billboard::Billboard>,
    Has<bevy::mesh::skinning::SkinnedMesh>,
    Has<bevy::mesh::morph::MorphWeights>,
    Has<bevy::animation::AnimationTargetId>,
    Has<bevy::mesh::morph::MeshMorphWeights>,
);

type RootState = (
    Entity,
    &'static FormId,
    Option<&'static CellRef>,
    Option<&'static WorldAssetRoot>,
    Option<&'static MeshHandle>,
    Option<&'static Transform>,
    Option<&'static ChildOf>,
    Option<&'static InheritedVisibility>,
    Has<crate::streaming::AssetFailed>,
);
type CameraState = (
    Entity,
    &'static Camera,
    Has<crate::render::WaterReflectionCamera>,
    Has<crate::world::components::StreamingCamera>,
    Option<&'static bevy::light::cluster::Clusters>,
    Option<&'static bevy::camera::visibility::VisibleEntities>,
);
type LocalLightQuery<'w, 's> =
    Query<'w, 's, (Entity, Option<&'static ChildOf>), Or<(With<PointLight>, With<SpotLight>)>>;

type SourceMaterialState = (
    Option<&'static MeshMaterial3d<StandardMaterial>>,
    Option<&'static RenderLayers>,
    Has<bevy::light::NotShadowCaster>,
);

type SelectionViewState = (
    Entity,
    &'static ExtractedView,
    Option<&'static LightEntity>,
    Option<&'static mut RenderVisibleEntities>,
    Option<&'static RenderLayers>,
);

#[derive(Default)]
struct QualificationCache {
    structure: Option<u64>,
    evaluated: Option<bevy::ecs::change_detection::Tick>,
    origin: IVec2,
    identities: EntityHashMap<(Option<u32>, Option<Entity>)>,
    hot: EntityHashMap<HotFacts>,
    index: source_index::SourceIndex,
    cursor:
        bevy::ecs::message::MessageCursor<crate::streaming::identity_changes::SourceNodeChanged>,
}

#[derive(PartialEq)]
struct HotFacts {
    literal_pose: bool,
    pose: Option<Transform>,
    visible: Option<Visibility>,
    inherited: Option<bool>,
    layers: Option<RenderLayers>,
    materials: Option<AssetId<StandardMaterial>>,
}

type HotFactState<'a> = (
    Option<&'a Transform>,
    Option<&'a SourceNodePose>,
    Option<&'a Visibility>,
    Option<&'a InheritedVisibility>,
    Option<&'a RenderLayers>,
    Option<&'a MeshMaterial3d<StandardMaterial>>,
);

fn hot_facts(state: HotFactState<'_>, literal_pose: bool) -> HotFacts {
    let (pose, baseline, visible, inherited, layers, standard) = state;
    // model_matrix rejects a node unless it equals its authored baseline.
    // All other poses have the same refused proof; returning to the baseline
    // still changes this snapshot and forces fresh qualification.
    let pose = pose
        .filter(|pose| literal_pose || baseline.is_some_and(|baseline| baseline.0 == **pose))
        .copied();
    HotFacts {
        literal_pose,
        pose,
        visible: visible.copied(),
        inherited: inherited.map(|visible| visible.get()),
        layers: layers.cloned(),
        materials: standard.map(|material| material.id()),
    }
}

#[derive(Default)]
struct QualifiedFacts {
    present: BTreeSet<u32>,
    retired_refs: HashSet<u32>,
    retired_present: HashSet<u32>,
    retired_materials: HashMap<MainEntity, Option<bevy::asset::UntypedAssetId>>,
    retired_eligibility: HashMap<MainEntity, RetiredEligibility>,
    bound: HashMap<RecordKey, MainEntity>,
    stock_primitives: HashMap<(u32, u32), MainEntity>,
}

type SourceFactChanges = Or<(
    Changed<CellRef>,
    Changed<WorldAssetRoot>,
    Changed<MeshHandle>,
    Changed<Mesh3d>,
    Changed<SourcePrimitive>,
    Changed<SourcePrimitiveMesh>,
    Changed<SourcePrimitiveTransform>,
    Changed<SourceStaticModel>,
    Changed<SourceNodePose>,
    Changed<StreamedCellKey>,
    Or<(
        Changed<bevy::gltf::GltfExtras>,
        Changed<crate::streaming::RetireHidden>,
    )>,
)>;

type SourceIdentityChanges = Or<(Changed<FormId>, Changed<ChildOf>)>;
type SourceVisibilityChanges = Or<(Changed<Visibility>, Changed<InheritedVisibility>)>;
type HotFactChanges = Or<(
    Changed<Transform>,
    Changed<Visibility>,
    Changed<InheritedVisibility>,
    Changed<RenderLayers>,
    Changed<MeshMaterial3d<StandardMaterial>>,
)>;

// Invalidation is a superset of the actual proof-role domains. Required reads,
// With and Has can opt a role into a registered disabler; optional reads cannot.
// Cover that union and all Changed inputs here so change filters and semantic
// snapshots share one default-filter policy and see every proof role.
// These filters MUST NOT be used for qualification or dependency readability.
type SourceObservationDomain = (
    (
        bevy::ecs::query::Allow<FormId>,
        bevy::ecs::query::Allow<Mesh3d>,
        bevy::ecs::query::Allow<Transform>,
        bevy::ecs::query::Allow<ChildOf>,
        bevy::ecs::query::Allow<Visibility>,
        bevy::ecs::query::Allow<InheritedVisibility>,
        bevy::ecs::query::Allow<StreamedCellKey>,
        bevy::ecs::query::Allow<crate::streaming::RetireHidden>,
        bevy::ecs::query::Allow<crate::streaming::AssetFailed>,
        bevy::ecs::query::Allow<SourcePrimitive>,
        bevy::ecs::query::Allow<SourcePrimitiveMesh>,
        bevy::ecs::query::Allow<SourcePrimitiveTransform>,
        bevy::ecs::query::Allow<SourceStaticModel>,
    ),
    (
        bevy::ecs::query::Allow<crate::billboard::Billboard>,
        bevy::ecs::query::Allow<bevy::mesh::skinning::SkinnedMesh>,
        bevy::ecs::query::Allow<bevy::mesh::morph::MorphWeights>,
        bevy::ecs::query::Allow<bevy::animation::AnimationTargetId>,
        bevy::ecs::query::Allow<bevy::mesh::morph::MeshMorphWeights>,
        bevy::ecs::query::Allow<bevy::light::NotShadowCaster>,
    ),
    (
        bevy::ecs::query::Allow<CellRef>,
        bevy::ecs::query::Allow<WorldAssetRoot>,
        bevy::ecs::query::Allow<MeshHandle>,
        bevy::ecs::query::Allow<SourceNodePose>,
        bevy::ecs::query::Allow<bevy::gltf::GltfExtras>,
        bevy::ecs::query::Allow<RenderLayers>,
        bevy::ecs::query::Allow<MeshMaterial3d<StandardMaterial>>,
    ),
);

#[derive(SystemParam)]
struct SourceQueries<'w, 's> {
    structure: Option<Res<'w, crate::streaming::identity_changes::StructureEpoch>>,
    query_filters: Res<'w, bevy::ecs::entity_disabling::DefaultQueryFilters>,
    journal: Option<Res<'w, Messages<crate::streaming::identity_changes::SourceNodeChanged>>>,
    fact_changes: Query<'w, 's, Entity, (SourceObservationDomain, SourceFactChanges)>,
    hot_changes: Query<'w, 's, Entity, (SourceObservationDomain, HotFactChanges)>,
    hot_facts: Query<'w, 's, HotFactState<'static>, SourceObservationDomain>,
    identity_changes: Query<'w, 's, Entity, (SourceObservationDomain, SourceIdentityChanges)>,
    identity_state:
        Query<'w, 's, (Option<&'static FormId>, Option<&'static ChildOf>), SourceObservationDomain>,
    terrain_visibility_changes: Query<'w, 's, Entity, SourceVisibilityChanges>,
    ticks: bevy::ecs::system::SystemChangeTick,
    cache: Local<'s, QualificationCache>,

    roots: Query<'w, 's, RootState>,
    // Entity allocation metadata is unfiltered; it is never a role/dependency oracle.
    entities: &'w bevy::ecs::entity::Entities,
    retired_cells: crate::streaming::RetiredCellAccess<'w, 's>,
    explicit_visibility: Query<'w, 's, &'static Visibility>,
    terrain_visibility: Query<'w, 's, (&'static Visibility, &'static InheritedVisibility)>,
    source_materials: Query<'w, 's, SourceMaterialState>,
    terrain: crate::streaming::TerrainIdentityAccess<'w, 's>,
    all_meshes: Query<'w, 's, Entity, With<Mesh3d>>,
    primitives: Query<
        'w,
        's,
        (
            Entity,
            &'static SourcePrimitive,
            &'static SourcePrimitiveMesh,
            &'static SourcePrimitiveTransform,
            &'static SourceStaticModel,
            &'static Mesh3d,
        ),
    >,
    nodes: Query<'w, 's, NodeState>,
    cells: Query<
        'w,
        's,
        (
            &'static StreamedCellKey,
            &'static Transform,
            Option<&'static ChildOf>,
        ),
    >,
    cameras: Query<'w, 's, CameraState>,
    light_parents: LocalLightQuery<'w, 's>,
    parents: Query<'w, 's, &'static ChildOf>,
}

/// CPU assignment is current-frame exact. GPU clustering exposes no CPU list;
/// use Bevy's current camera visibility class as a conservative dependency set.
/// Missing facts never mean an empty domain.
fn local_light_dependencies(
    clusters: Option<&bevy::light::cluster::Clusters>,
    visible: Option<&bevy::camera::visibility::VisibleEntities>,
    mut is_local_light: impl FnMut(Entity) -> bool,
) -> Option<HashSet<MainEntity>> {
    use bevy::light::cluster::{ClusterVisibilityClass, ClusterableObjects};
    match &clusters?.clusterable_objects {
        ClusterableObjects::Cpu(objects) => Some(
            objects
                .iter()
                .flat_map(|cluster| cluster.iter())
                .copied()
                .filter(|entity| is_local_light(*entity))
                .map(MainEntity::from)
                .collect(),
        ),
        ClusterableObjects::Gpu => Some(
            visible?
                .get(std::any::TypeId::of::<ClusterVisibilityClass>())
                .iter()
                .copied()
                .filter(|entity| is_local_light(*entity))
                .map(MainEntity::from)
                .collect(),
        ),
    }
}

fn cell_world(key: CellKey) -> Option<u32> {
    match key {
        CellKey::Exterior { worldspace_id, .. } => Some(worldspace_id),
        CellKey::Interior(_) => None,
    }
}

fn expected_reference_pose(
    pose: &super::super::reader::ReferencePose,
    key: CellKey,
) -> Option<Transform> {
    let CellKey::Exterior { .. } = key else {
        return None;
    };
    Some(crate::streaming::reference_placement(pose.position, pose.rotation, pose.scale, key).1)
}

/// Nearest reference-query root in one immutable extraction, independent of certification.
/// Every matching root is a barrier, including unrelated or currently invalid references.
/// Full generational entity keys and a fresh instance per extraction avoid stale ancestry facts.
#[derive(Default)]
struct ReferenceRootMemo {
    resolved: EntityHashMap<Option<Entity>>,
    path: Vec<Entity>,
    visiting: EntityHashSet,
}

impl ReferenceRootMemo {
    fn resolve(
        &mut self,
        entity: Entity,
        mut is_root: impl FnMut(Entity) -> bool,
        mut parent: impl FnMut(Entity) -> Option<Entity>,
    ) -> Option<Entity> {
        // Retain scratch allocations without clearing the hash table's retained capacity.
        self.path.clear();
        debug_assert!(self.visiting.is_empty());
        let mut at = entity;
        let root = loop {
            if let Some(root) = self.resolved.get(&at) {
                break *root;
            }
            if !self.visiting.insert(at) {
                break None;
            }
            self.path.push(at);
            if is_root(at) {
                break Some(at);
            }
            let Some(next) = parent(at) else {
                break None;
            };
            at = next;
        };
        for &entity in &self.path {
            let was_visiting = self.visiting.remove(&entity);
            debug_assert!(was_visiting);
            self.resolved.insert(entity, root);
        }
        debug_assert!(self.visiting.is_empty());
        root
    }
}

/// Presence is conservative across invalid reparenting: any genuine retired
/// cell ancestor means the stock cannot be credited as currently drawable.
/// Qualification below still requires the original direct owning cell.
fn under_retired_cell(
    entity: Entity,
    parents: &Query<&ChildOf>,
    retired_cells: &crate::streaming::RetiredCellAccess,
) -> bool {
    let mut at = entity;
    let mut seen = HashSet::new();
    while seen.insert(at) {
        if retired_cells.contains(at) {
            return true;
        }
        let Ok(parent) = parents.get(at) else {
            return false;
        };
        at = parent.parent();
    }
    false
}

/// Ignore only the owning cell's explicit hide; every descendant/reference
/// explicit hide remains a runtime exclusion. Missing/cyclic ancestry refuses.
fn explicitly_hidden_below_cell(
    entity: Entity,
    cell: Entity,
    parents: &Query<&ChildOf>,
    visibility: &Query<&Visibility>,
) -> bool {
    let mut at = entity;
    let mut seen = HashSet::new();
    while at != cell {
        if !seen.insert(at)
            || visibility
                .get(at)
                .is_ok_and(|value| *value == Visibility::Hidden)
        {
            return true;
        }
        let Ok(parent) = parents.get(at) else {
            return true;
        };
        at = parent.parent();
    }
    false
}

/// The model-space matrix uses exactly the same parent-first products as the
/// loader baseline. A changed local pose or a changed hierarchy revokes binding.
fn model_matrix(entity: Entity, root: Entity, nodes: &Query<NodeState>) -> Option<Mat4> {
    let mut at = entity;
    let mut matrix = Mat4::IDENTITY;
    let mut seen = HashSet::new();
    while at != root {
        if !seen.insert(at) {
            return None;
        }
        let (pose, baseline, parent, extras, billboard, skin, morph, _animation, mesh_morph) =
            nodes.get(at).ok()?;
        if billboard
            || skin
            || morph
            || mesh_morph
            || extras.is_some_and(|extras| {
                crate::billboard::BillboardMode::from_extras(&extras.value).is_some()
            })
        {
            return None;
        }
        if baseline?.0 != *pose {
            return None;
        }
        matrix = pose.to_matrix() * matrix;
        at = parent?.parent();
    }
    matrix.is_finite().then_some(matrix)
}

fn template_valid(asset: &WorldAsset, invalid: &HashSet<AssetId<Mesh>>) -> bool {
    let world = &asset.world;
    let Some(mut meshes) = world.try_query::<(Entity, &Mesh3d)>() else {
        return false;
    };
    let mut count = 0;
    for (entity, mesh) in meshes.iter(world) {
        count += 1;
        let (Some(tag), Some(source_mesh), Some(source_transform), Some(static_model)) = (
            world.get::<SourcePrimitive>(entity),
            world.get::<SourcePrimitiveMesh>(entity),
            world.get::<SourcePrimitiveTransform>(entity),
            world.get::<SourceStaticModel>(entity),
        ) else {
            return false;
        };
        if !tag.matches_default_scene
            || !static_model.0
            || source_mesh.0 != mesh.0
            || invalid.contains(&mesh.0.id())
        {
            return false;
        }
        let mut at = entity;
        let mut matrix = Mat4::IDENTITY;
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(at) {
                return false;
            }
            let Some(pose) = world.get::<Transform>(at) else {
                return false;
            };
            if world
                .get::<SourceNodePose>(at)
                .is_none_or(|baseline| baseline.0 != *pose)
                || world.get::<crate::billboard::Billboard>(at).is_some()
                || world.get::<bevy::mesh::skinning::SkinnedMesh>(at).is_some()
                || world.get::<bevy::mesh::morph::MorphWeights>(at).is_some()
                || world
                    .get::<bevy::mesh::morph::MeshMorphWeights>(at)
                    .is_some()
                || world
                    .get::<bevy::gltf::GltfExtras>(at)
                    .is_some_and(|extras| {
                        crate::billboard::BillboardMode::from_extras(&extras.value).is_some()
                    })
            {
                return false;
            }
            matrix = pose.to_matrix() * matrix;
            match world.get::<ChildOf>(at) {
                Some(parent) => at = parent.parent(),
                None => break,
            }
        }
        if matrix != source_transform.0 {
            return false;
        }
    }
    count > 0
}

#[derive(SystemParam)]
struct SourceEvents<'w, 's> {
    meshes: MessageReader<'w, 's, AssetEvent<Mesh>>,
    scenes: Option<MessageReader<'w, 's, AssetEvent<WorldAsset>>>,
    standards: MessageReader<'w, 's, AssetEvent<StandardMaterial>>,
}

#[derive(Default)]
struct SourceExtractionTiming {
    total: crate::profiling::PeriodicTiming,
    excluding_terrain: crate::profiling::PeriodicTiming,
    terrain: crate::profiling::PeriodicTiming,
    terrain_snapshot: Option<Arc<HashMap<MainEntity, crate::streaming::TerrainPrimitiveIdentity>>>,
    calls: u64,
    reused: u64,
    hot_refreshes: u64,
    identity_refreshes: u64,
    other_refreshes: u64,
    structural_refreshes: u64,
}

type SourceGeometryAssets<'w> = (
    Option<Res<'w, Assets<WorldAsset>>>,
    Res<'w, Assets<Mesh>>,
    Option<Res<'w, ClusterBatchStockMeshes>>,
);

#[allow(clippy::too_many_arguments)]
fn extract_sources(
    mut out: ResMut<Sources>,
    package: Extract<Option<Res<ValidatedPackage>>>,
    scene: Extract<Option<Res<ClusterScene>>>,
    lazy: Extract<Option<Res<LazyBatches>>>,
    geometry_assets: Extract<SourceGeometryAssets>,
    handles: Extract<Res<ClusterBatchHandles>>,
    standards: Extract<Res<Assets<StandardMaterial>>>,
    mut queries: Extract<SourceQueries>,
    active: Extract<Option<Res<ActiveSpace>>>,
    config: Extract<Option<Res<crate::config::EngineConfig>>>,
    origin: Extract<Option<Res<RenderOrigin>>>,
    mut events: Extract<SourceEvents>,
    asset_server: Extract<Option<Res<AssetServer>>>,
    (mut timing, settings): (Local<SourceExtractionTiming>, Option<Res<ClusterSettings>>),
) {
    perf_span!("cluster::handoff::extract_sources");
    // PI1 suppression is independent of extraction. Lifecycle and hot changes
    // invalidate stock proof in either ownership mode.
    let own_statics = settings.is_some_and(|settings| settings.own_statics);
    let (assets, stock_meshes, fixture_meshes) = &*geometry_assets;
    let out = &mut *out;
    let queries = &mut *queries;
    let started = std::time::Instant::now();
    #[cfg(test)]
    let forced_full = std::mem::take(&mut out.force_full);
    #[cfg(not(test))]
    let forced_full = false;
    if forced_full {
        out.template_cache.clear();
        out.geometry_cache.clear();
    }
    let mut work = SourceExtractionWork::default();
    let mut dropped_meshes = HashSet::new();
    // The batches whose cached template decision an event below drops.
    let mut dropped_materials = Vec::new();
    let mut dropped_scenes = Vec::new();
    let mut dropped_fixture_rows = BTreeSet::new();
    for event in events.meshes.read() {
        let id = event_asset_id(event);
        // Added/LoadedWithDependencies wake pending layout proofs without making
        // geometry invalid. Modified/Removed remain sticky certification refusals.
        if let Some(rows) = out.template_rows.fixture_by_mesh.get(&id) {
            dropped_fixture_rows.extend(rows.iter().copied());
        }
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            debug!(?event, path=?asset_server.as_ref().and_then(|server| server.get_path(*id)), "cluster source mesh invalidated");
            work.mesh_modified += u64::from(matches!(event, AssetEvent::Modified { .. }));
            work.mesh_removed += u64::from(matches!(event, AssetEvent::Removed { .. }));
            dropped_meshes.insert(*id);
            out.invalid_meshes.insert(*id);
            if let Some(scenes) = out.template_meshes.by_mesh.get(id) {
                for &scene in scenes {
                    out.template_cache.remove(&scene);
                    dropped_scenes.push(scene);
                }
            }
        }
    }
    for event in events
        .scenes
        .as_mut()
        .into_iter()
        .flat_map(|events| events.read())
    {
        let id = event_asset_id(event);
        work.scene_events += 1;
        out.template_cache.remove(&id);
        out.template_meshes.remove(id);
        dropped_scenes.push(id);
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } = event {
            debug!(?event, path=?asset_server.as_ref().and_then(|server| server.get_path(*id)), "cluster source scene invalidated");
            work.scene_modified += u64::from(matches!(event, AssetEvent::Modified { .. }));
            work.scene_removed += u64::from(matches!(event, AssetEvent::Removed { .. }));
            out.invalid_templates.insert(*id);
        }
    }
    for event in events.standards.read() {
        work.material_events += 1;
        out.geometry_cache.remove(&event_asset_id(event).untyped());
        dropped_materials.push(event_asset_id(event).untyped());
    }
    let certificate = package.as_ref().map(|package| package.0.clone());
    let same = match (&certificate, &out.certificate) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    };
    let structure = queries
        .structure
        .as_ref()
        .and_then(|epoch| epoch.covered_version(&queries.query_filters));
    let current_origin = origin.as_ref().map_or(IVec2::ZERO, |origin| origin.0);
    let changes_span = ::bevy::log::info_span!("cluster::sources::change_queries").entered();
    let fetch_gap = queries.cache.evaluated != Some(queries.ticks.last_run());
    let missed = queries
        .journal
        .as_ref()
        .is_some_and(|journal| queries.cache.cursor.missed_messages(journal) != 0);
    let mut changed = EntityHashSet::default();
    let mut covered_epoch = queries.cache.structure;
    let mut journal_contiguous = covered_epoch.is_some();
    let mut journal_events = 0_u64;
    if let Some(journal) = &queries.journal {
        for event in queries.cache.cursor.read(journal) {
            journal_events += 1;
            journal_contiguous &=
                covered_epoch.is_some_and(|previous| event.epoch == previous.wrapping_add(1));
            covered_epoch = Some(event.epoch);
            changed.insert(event.entity);
        }
    }
    // A nonempty suffix cannot prove coverage: check every sequence edge AND
    // both endpoints of the prior/current epoch interval. Missing, expired or
    // replaced journals recover even when surviving events are all unrelated.
    let journal_complete =
        queries.journal.is_some() && journal_contiguous && covered_epoch == structure;
    let journal_entities = changed.len();
    let mut identity_changed = false;
    for entity in &queries.identity_changes {
        let identity = queries
            .identity_state
            .get(entity)
            .ok()
            .map(|(form, parent)| (form.map(|form| form.0), parent.map(ChildOf::parent)))
            .unwrap_or_default();
        if queries.cache.identities.get(&entity) != Some(&identity) {
            changed.insert(entity);
            identity_changed = true;
        }
    }
    let other_changed = queries.fact_changes.iter().next().is_some();
    changed.extend(queries.fact_changes.iter());
    let mut hot_changed = false;
    for entity in &queries.hot_changes {
        if queries.cache.index.contains_node(entity)
            && queries.cache.hot.get(&entity).is_none_or(|previous| {
                queries
                    .hot_facts
                    .get(entity)
                    .ok()
                    .map(|state| hot_facts(state, previous.literal_pose))
                    .as_ref()
                    != Some(previous)
            })
        {
            changed.insert(entity);
            hot_changed = true;
        }
    }
    // StructureEpoch is coverage/repair evidence, never a list of affected roots.
    // Sequence coverage is independent of subject membership and event count.
    let structural_changed = structure.is_none() || structure != queries.cache.structure;
    let unexplained_structure = !journal_complete;
    let refresh = forced_full
        || !same
        || !queries.cache.index.initialized
        || missed
        || fetch_gap
        || queries.query_filters.is_changed()
        || structure.is_none()
        || queries.journal.is_none()
        || unexplained_structure;
    if refresh {
        warn!(
            certificate_changed = !same,
            forced_full,
            initialized = queries.cache.index.initialized,
            missed,
            fetch_gap,
            filters_changed = queries.query_filters.is_changed(),
            uncovered = structure.is_none(),
            missing_journal = queries.journal.is_none(),
            unexplained_structure,
            journal_events,
            prior_epoch = ?queries.cache.structure,
            current_epoch = ?structure,
            covered_epoch = ?covered_epoch,
            origin_changed = queries.cache.origin != current_origin,
            "cluster source qualification full recovery"
        );
    }
    if refresh {
        // Cursor offsets belong to one Messages instance. A fresh/replaced
        // resource may have a lower count; clear resets in either direction.
        if let Some(journal) = &queries.journal {
            queries.cache.cursor.clear(journal);
        } else {
            queries.cache.cursor = default();
        }
    }
    if queries.cache.origin != current_origin {
        changed.extend(queries.cache.index.roots());
    }
    if !same {
        out.seen_roots.clear();
    }
    let mut index = std::mem::take(&mut queries.cache.index);
    let affected = index.update(
        queries,
        &out.seen_roots,
        &changed,
        refresh,
        certificate.as_deref(),
    );
    queries.cache.index = index;
    work.qualification_full = refresh;
    work.affected_references = affected.references.len() as u64;
    work.journal_entities = journal_entities as u64;
    work.journal_events = journal_events;
    work.journal_complete = journal_complete;
    work.subjects_updated = affected.subjects_updated;
    work.ancestry_edges = affected.ancestry_edges;
    work.discovery_roots = affected.discovery_roots;
    work.discovery_meshes = affected.discovery_meshes;
    let (subjects, nodes, groups) = queries.cache.index.sizes();
    work.indexed_subjects = subjects as u64;
    work.indexed_nodes = nodes as u64;
    work.indexed_reference_groups = groups as u64;
    timing.hot_refreshes += u64::from(hot_changed);
    timing.identity_refreshes += u64::from(identity_changed);
    timing.other_refreshes += u64::from(other_changed);
    timing.structural_refreshes += u64::from(structural_changed);
    timing.calls += 1;

    drop(changes_span);
    let _refresh_span = ::bevy::log::info_span!("cluster::sources::refresh").entered();
    // Asset deltas are applied before merging affected ECS reference groups. The
    // immutable previous snapshot stays intact until candidate_worlds compares it.
    let asset_facts = out.invalidate_stock_meshes(&dropped_meshes, &mut work);
    let fresh = if refresh || !affected.references.is_empty() {
        let mut facts = asset_facts.unwrap_or_else(|| QualifiedFacts {
            present: out.present.clone(),
            retired_refs: out.retired_refs.clone(),
            retired_present: out.retired_present.clone(),
            retired_materials: out.retired_materials.clone(),
            retired_eligibility: out.retired_eligibility.clone(),
            bound: out.bound.clone(),
            stock_primitives: out.stock_primitives.clone(),
        });
        if refresh {
            facts = QualifiedFacts::default();
        }
        let refs = &affected.references;
        if refresh {
            out.stock_candidates.clear();
            out.stock_meshes.clear();
            out.stock_members.clear();
            out.proof_keys.clear();
        } else {
            for reference in refs {
                facts.present.remove(reference);
                facts.retired_refs.remove(reference);
                facts.retired_present.remove(reference);
                if let Some(members) = out.stock_members.remove(reference) {
                    for member in members {
                        facts.retired_materials.remove(&member);
                        facts.retired_eligibility.remove(&member);
                    }
                }
                if let Some(keys) = out.proof_keys.remove(reference) {
                    for key in keys {
                        facts.stock_primitives.remove(&key);
                        if let Some(record) = out.record_lookup.get(&key) {
                            facts.bound.remove(record);
                        }
                        if let Some(candidates) = out.stock_candidates.remove(&key) {
                            for (mesh, _) in candidates {
                                if let Some(keys) = out.stock_meshes.get_mut(&mesh) {
                                    keys.remove(&key);
                                    if keys.is_empty() {
                                        out.stock_meshes.remove(&mesh);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let QualifiedFacts {
            mut present,
            mut retired_refs,
            mut retired_present,
            mut retired_materials,
            mut retired_eligibility,
            mut bound,
            mut stock_primitives,
        } = facts;
        if !same {
            out.seen_roots.clear();
        }
        for root in &affected.nodes {
            if !queries.entities.contains_spawned(*root) {
                out.seen_roots.remove(root);
            }
        }
        let history: Vec<_> = if refresh {
            out.seen_roots.keys().copied().collect()
        } else {
            affected.history.iter().copied().collect()
        };
        for root in history {
            if let Some(references) = out.seen_roots.get(&root) {
                present.extend(references.iter().copied());
                if under_retired_cell(root, &queries.parents, &queries.retired_cells) {
                    retired_present.extend(references.iter().copied());
                }
            }
        }
        if let Some(certificate) = &certificate {
            if !same {
                out.record_lookup.clear();
                out.expected_primitives.clear();
                for (key, identity) in certificate.records() {
                    out.record_lookup
                        .insert((identity.reference, identity.primitive), *key);
                    out.expected_primitives
                        .entry(identity.reference)
                        .or_default()
                        .insert(identity.primitive);
                }
                out.template_cache.clear();
                out.template_meshes = TemplateMeshDependencies::default();
                out.template_rows.stale = true;
            }
            let mut roots = HashMap::new();
            let mut duplicates = HashSet::new();
            let mut valid_roots = HashMap::new();
            let mut retired_candidates = HashMap::new();
            let mut mesh_roots: HashMap<Entity, Vec<Entity>> = HashMap::new();
            for root in &affected.roots {
                let Ok((entity, form, cell, asset, mesh_path, actual, parent, visible, failed)) =
                    queries.roots.get(*root)
                else {
                    continue;
                };
                work.roots_scanned += 1;
                if certificate.reference(form.0).is_none() {
                    continue;
                }
                if under_retired_cell(entity, &queries.parents, &queries.retired_cells) {
                    retired_present.insert(form.0);
                }
                if roots.insert(form.0, entity).is_some() {
                    duplicates.insert(form.0);
                }
                let valid = (|| {
                    if failed {
                        return None;
                    }
                    let snapshot = certificate.reference(form.0)?;
                    let pose = certificate.reference_pose(form.0)?;
                    let expected_path = certificate.reference_model_path(form.0)?;
                    if mesh_path?.0 != expected_path {
                        return None;
                    }
                    if let Some(asset) = asset
                        && asset.0.path().is_none_or(|path| {
                            path.path().to_string_lossy().replace('\\', "/") != expected_path
                        })
                    {
                        return None;
                    }
                    let (cell_key, cell_pose, cell_parent) =
                        queries.cells.get(parent?.parent()).ok()?;
                    if cell_parent.is_some()
                        || cell?.0 != snapshot.source_cell_id
                        || cell_world(cell_key.0) != Some(snapshot.placed_worldspace_id)
                    {
                        return None;
                    }
                    let CellKey::Exterior { grid_x, grid_y, .. } = cell_key.0 else {
                        return None;
                    };
                    let expected_cell =
                        Transform::from_translation(crate::streaming::grid_translation(
                            IVec2::new(grid_x, grid_y),
                            origin.as_ref().map_or(IVec2::ZERO, |origin| origin.0),
                        ));
                    if *actual? != expected_reference_pose(pose, cell_key.0)?
                        || *cell_pose != expected_cell
                    {
                        return None;
                    }
                    Some(())
                })()
                .is_some();
                if valid {
                    valid_roots.insert(entity, form.0);
                    if let Some(parent) = parent
                        && queries.retired_cells.contains(parent.parent())
                        && certificate.reference_complete(form.0)
                    {
                        retired_candidates.insert(entity, (form.0, parent.parent()));
                    }
                }
                if !valid || visible.is_some_and(|visible| !visible.get()) {
                    present.insert(form.0);
                }
            }
            // Actual descendants, including untagged/replaced meshes, establish stock
            // presence. A valid root awaiting instancing keeps its ready cluster.
            let mut reference_roots = ReferenceRootMemo::default();
            let presence_span = bevy::log::tracing::info_span!("cluster_source_presence").entered();
            for &entity in &affected.meshes {
                work.meshes_scanned += 1;
                let Some(root) = reference_roots.resolve(
                    entity,
                    |at| queries.roots.contains(at),
                    |at| queries.parents.get(at).ok().map(ChildOf::parent),
                ) else {
                    continue;
                };
                if let Ok((_, form, ..)) = queries.roots.get(root) {
                    // This index describes certified replacement candidates only.
                    // Other stock references belong to the generic stock proof;
                    // treating their arrival as unknown cluster ownership revokes
                    // unrelated destinations on every streaming frame.
                    if certificate.reference(form.0).is_some() {
                        present.insert(form.0);
                        out.seen_roots.entry(root).or_default().insert(form.0);
                        if retired_candidates.contains_key(&root) {
                            mesh_roots.entry(root).or_default().push(entity);
                        }
                    }
                }
            }

            drop(presence_span);
            present.extend(duplicates.iter().copied());
            let lookup = &out.record_lookup;
            let mut ambiguous = HashSet::new();

            let qualification_span =
                bevy::log::tracing::info_span!("cluster_source_qualification").entered();
            for primitive_entity in &affected.meshes {
                let Ok((entity, tag, source_mesh, source_transform, static_model, mesh)) =
                    queries.primitives.get(*primitive_entity)
                else {
                    continue;
                };
                work.primitives_scanned += 1;
                if !tag.matches_default_scene
                    || !static_model.0
                    || source_mesh.0 != mesh.0
                    || out.invalid_meshes.contains(&mesh.0.id())
                {
                    continue;
                }
                let Some(root) = reference_roots.resolve(
                    entity,
                    |at| queries.roots.contains(at),
                    |at| queries.parents.get(at).ok().map(ChildOf::parent),
                ) else {
                    continue;
                };
                let Some(form) = valid_roots.get(&root) else {
                    continue;
                };
                if duplicates.contains(form) {
                    continue;
                }
                let Some(expected_path) = certificate.reference_model_path(*form) else {
                    continue;
                };
                if source_mesh.0.path().is_none_or(|path| {
                    path.path().to_string_lossy().replace('\\', "/") != expected_path
                }) {
                    continue;
                }
                if model_matrix(entity, root, &queries.nodes) != Some(source_transform.0) {
                    continue;
                }
                let primitive = (*form, tag.ordinal);
                out.proof_keys.entry(*form).or_default().insert(primitive);
                out.stock_candidates
                    .entry(primitive)
                    .or_default()
                    .push((mesh.0.id(), entity.into()));
                out.stock_meshes
                    .entry(mesh.0.id())
                    .or_default()
                    .insert(primitive);
                if ambiguous.contains(&primitive) {
                    continue;
                }
                if stock_primitives
                    .insert(primitive, MainEntity::from(entity))
                    .is_some()
                {
                    stock_primitives.remove(&primitive);
                    ambiguous.insert(primitive);
                    if let Some(key) = lookup.get(&primitive) {
                        bound.remove(key);
                    }
                    continue;
                }
                // An unmapped expected primitive must still prove the same actual
                // root/path/handle/pose chain before satisfying partial coverage.
                if let Some(key) = lookup.get(&primitive) {
                    bound.insert(*key, MainEntity::from(entity));
                }
            }
            drop(qualification_span);
            // A retired source remains present/history-tracked. Only a fully
            // instantiated, currently proven model may bypass visible-list presence;
            // missing/extra/modified meshes, duplicates and explicit child hides pin.
            if !retired_candidates.is_empty() {
                for (root, (reference, cell)) in retired_candidates {
                    // Read only this affected reference's proof candidates. Ambiguous
                    // keys stay indexed but contribute no unique instantiated proof.
                    let (proven, instantiated) = retired_reference_proof(
                        out.proof_keys.get(&reference),
                        &stock_primitives,
                        &mut work,
                    );
                    let meshes = mesh_roots.get(&root).map(Vec::as_slice).unwrap_or(&[]);
                    let Some(expected) = out.expected_primitives.get(&reference) else {
                        continue;
                    };
                    if !duplicates.contains(&reference)
                        && !expected.is_empty()
                        && &instantiated == expected
                        && meshes.len() == expected.len()
                        && meshes.iter().all(|entity| {
                            proven.contains(&MainEntity::from(*entity))
                                && !explicitly_hidden_below_cell(
                                    *entity,
                                    cell,
                                    &queries.parents,
                                    &queries.explicit_visibility,
                                )
                        })
                    {
                        retired_refs.insert(reference);
                        for mesh in meshes {
                            let material = queries.source_materials.get(*mesh).ok().and_then(
                                |(standard, layers, not_caster)| {
                                    retired_eligibility.insert(
                                        MainEntity::from(*mesh),
                                        RetiredEligibility {
                                            layers: layers.cloned().unwrap_or_default(),
                                            casts_shadows: !not_caster,
                                        },
                                    );
                                    standard.map(|material| material.id().untyped())
                                },
                            );
                            retired_materials.insert(MainEntity::from(*mesh), material);
                        }
                    }
                }
            }
        }

        for reference in refs {
            if let Some(keys) = out.proof_keys.get(reference) {
                let members = keys
                    .iter()
                    .filter_map(|key| stock_primitives.get(key).copied())
                    .collect();
                out.stock_members.insert(*reference, members);
            }
        }
        out.qualification_refreshes += 1;
        Some(QualifiedFacts {
            present,
            retired_refs,
            retired_present,
            retired_materials,
            retired_eligibility,
            bound,
            stock_primitives,
        })
    } else {
        asset_facts
    };
    let qualification_updated = fresh.is_some();
    for entity in &affected.nodes {
        if queries.cache.index.contains_node(*entity) {
            let identity = queries
                .identity_state
                .get(*entity)
                .ok()
                .map(|(form, parent)| (form.map(|form| form.0), parent.map(ChildOf::parent)))
                .unwrap_or_default();
            queries.cache.identities.insert(*entity, identity);
            let literal = queries.roots.contains(*entity) || queries.cells.contains(*entity);
            if let Ok(state) = queries.hot_facts.get(*entity) {
                queries.cache.hot.insert(*entity, hot_facts(state, literal));
            } else {
                queries.cache.hot.remove(entity);
            }
        } else {
            queries.cache.hot.remove(entity);
            queries.cache.identities.remove(entity);
        }
    }
    timing.reused += u64::from(!qualification_updated);
    let qualification_elapsed = started.elapsed();
    queries.cache.structure = structure;
    queries.cache.origin = current_origin;
    queries.cache.evaluated = Some(queries.ticks.this_run());
    let present = fresh.as_ref().map_or(&out.present, |facts| &facts.present);
    let retired_refs = fresh
        .as_ref()
        .map_or(&out.retired_refs, |facts| &facts.retired_refs);
    let retired_present = fresh
        .as_ref()
        .map_or(&out.retired_present, |facts| &facts.retired_present);
    let retired_materials = fresh
        .as_ref()
        .map_or(&out.retired_materials, |facts| &facts.retired_materials);
    let retired_eligibility = fresh
        .as_ref()
        .map_or(&out.retired_eligibility, |facts| &facts.retired_eligibility);
    let bound = fresh.as_ref().map_or(&out.bound, |facts| &facts.bound);
    let stock_primitives = fresh
        .as_ref()
        .map_or(&out.stock_primitives, |facts| &facts.stock_primitives);

    let templates_started = std::time::Instant::now();
    let templates_span = ::bevy::log::info_span!("cluster::sources::templates").entered();
    // Each batch's decision depends only on its own handle, material, template and the caches
    // above. Retry pending rows, event dependencies and rows whose material/source IDs
    // changed. Only a certificate/scope reset decides every row again. Mesh events
    // reach exactly their indexed source scenes.
    let batches = handles.handles.len();
    let template_stale = out.template_rows.stale;
    let layout_scope = lazy.as_ref().map(|lazy| lazy.layout_scope);
    let full = template_stale
        || !same
        || forced_full
        || out.lazy_layout_scope != layout_scope
        || out.fixture_layout_scope != fixture_meshes.is_some()
        || fixture_meshes
            .as_ref()
            .is_some_and(|meshes| meshes.is_changed());
    out.lazy_layout_scope = layout_scope;
    out.fixture_layout_scope = fixture_meshes.is_some();
    let (mut templates, mut normal_maps, mut source_fallback_batches, redo) = if full {
        out.template_rows = TemplateRows::default();
        (
            vec![false; batches],
            vec![false; batches],
            BTreeSet::new(),
            (0..batches as u32).collect::<BTreeSet<u32>>(),
        )
    } else {
        let mut redo = std::mem::take(&mut out.template_rows.pending);
        if handles.is_changed()
            || lazy.as_ref().is_some_and(|lazy| lazy.is_changed())
            || out.template_inputs.len() != batches
        {
            for index in 0..batches {
                let input = (
                    handles.handles[index].as_ref().map(|handle| handle.id()),
                    lazy.as_deref()
                        .and_then(|lazy| lazy.source_scene(index as u32))
                        .map(|scene| scene.id()),
                );
                if out.template_inputs.get(index) != Some(&input) {
                    redo.insert(index as u32);
                }
            }
            // Truncation must remove old inverse memberships and pending rows too.
            for index in batches..out.template_inputs.len() {
                out.template_rows
                    .remove(index as u32, out.template_inputs[index]);
            }
        }
        for id in &dropped_materials {
            redo.extend(out.template_rows.by_material.get(id).into_iter().flatten());
        }
        for id in &dropped_scenes {
            redo.extend(out.template_rows.by_scene.get(id).into_iter().flatten());
        }
        (
            out.templates.clone(),
            out.normal_maps.clone(),
            out.source_fallback_batches.clone(),
            redo,
        )
    };
    templates.resize(batches, false);
    normal_maps.resize(batches, false);
    let mut stock_tangents = if full {
        vec![None; batches]
    } else {
        out.stock_tangents.clone()
    };
    stock_tangents.resize(batches, None);
    let inputs = TemplateAssets {
        standards: &standards,
        lazy: lazy.as_deref(),
        scenes: assets.as_deref(),
        meshes: stock_meshes,
        fixtures: fixture_meshes.as_deref(),
    };
    let mut redo = redo;
    redo.extend(dropped_fixture_rows);
    source_fallback_batches.retain(|index| (*index as usize) < batches);
    out.template_inputs.truncate(batches);
    for index in redo {
        if index as usize >= batches {
            continue;
        }
        if let Some(&previous) = out.template_inputs.get(index as usize) {
            out.template_rows.remove(index, previous);
        }
        work.template_rows += 1;
        let Some(handle) = handles.handles.get(index as usize) else {
            continue;
        };
        let row = match handle {
            Some(handle) => template_row(
                index,
                handle,
                TemplateCaches {
                    geometry: &mut out.geometry_cache,
                    templates: &mut out.template_cache,
                    template_meshes: &mut out.template_meshes,
                    invalid_templates: &out.invalid_templates,
                    invalid_meshes: &out.invalid_meshes,
                    rows: &mut out.template_rows,
                },
                &inputs,
            ),
            None => TemplateRow::default(),
        };
        let input = (
            handle.as_ref().map(|handle| handle.id()),
            lazy.as_deref()
                .and_then(|lazy| lazy.source_scene(index))
                .map(|scene| scene.id()),
        );
        if out.template_inputs.len() <= index as usize {
            out.template_inputs.resize(index as usize + 1, (None, None));
        }
        out.template_inputs[index as usize] = input;
        templates[index as usize] = row.template;
        normal_maps[index as usize] = row.normal_map;
        stock_tangents[index as usize] = row.stock_tangents;
        if row.fallback {
            source_fallback_batches.insert(index);
        } else {
            source_fallback_batches.remove(&index);
        }
        if row.pending {
            out.template_rows.pending.insert(index);
        }
    }
    let layout_changed = out.stock_tangents != stock_tangents;
    if layout_changed {
        out.stock_tangents = stock_tangents;
        out.layout_version = out.layout_version.wrapping_add(1);
    }
    // Pending=false -> loaded-invalid=false does not change templates. Keep its
    // negative demand independently so that transition cannot be lost.
    out.source_fallback_batches = source_fallback_batches;
    drop(templates_span);
    let templates_elapsed = templates_started.elapsed();
    let terrain_started = std::time::Instant::now();
    let terrain_span = bevy::log::tracing::info_span!("cluster_terrain_identity").entered();
    let terrain_snapshot = queries.terrain.identities_shared();
    let fresh_terrain = timing
        .terrain_snapshot
        .as_ref()
        .is_none_or(|old| !Arc::ptr_eq(old, &terrain_snapshot))
        .then(|| Arc::clone(&terrain_snapshot));
    timing.terrain_snapshot = Some(terrain_snapshot);
    let terrain = fresh_terrain.as_ref().unwrap_or(&out.terrain);
    drop(terrain_span);
    let terrain_elapsed = terrain_started.elapsed();
    let fresh_terrain_visible = (refresh
        || fresh_terrain.is_some()
        || queries
            .terrain_visibility_changes
            .iter()
            .any(|entity| terrain.contains_key(&MainEntity::from(entity))))
    .then(|| collect_terrain_visibility(terrain, &queries.terrain_visibility));
    let terrain_visible = fresh_terrain_visible
        .as_ref()
        .unwrap_or(&out.terrain_visible);
    out.origin = current_origin;
    let mut worlds = HashMap::new();
    let active_world = active
        .as_ref()
        .filter(|active| active.interior.is_none())
        .and_then(|active| {
            active
                .worldspace_id
                .or_else(|| config.as_ref().map(|config| config.worldspace_id))
        });
    let lights_span = ::bevy::log::info_span!("cluster::sources::lights_worlds").entered();
    let mut light_dependencies = HashMap::new();
    for (entity, camera, reflection, streaming, clusters, visible) in &queries.cameras {
        if !camera.is_active {
            continue;
        }
        light_dependencies.insert(
            entity.into(),
            local_light_dependencies(clusters, visible, |light| {
                queries.light_parents.contains(light)
            }),
        );
        if streaming || reflection {
            worlds.insert(entity.into(), active_world);
        }
    }
    for (entity, parent) in &queries.light_parents {
        let mut at = parent.map(ChildOf::parent);
        let mut seen = HashSet::new();
        while let Some(ancestor) = at {
            if !seen.insert(ancestor) {
                break;
            }
            if let Some(world) = worlds.get(&MainEntity::from(ancestor)).copied() {
                worlds.insert(entity.into(), world);
                break;
            }
            if let Ok((cell, _, _)) = queries.cells.get(ancestor) {
                worlds.insert(entity.into(), cell_world(cell.0));
                break;
            }
            at = queries.parents.get(ancestor).ok().map(ChildOf::parent);
        }
    }
    drop(lights_span);
    let compare_span = ::bevy::log::info_span!("cluster::sources::compare").entered();
    let certificate_changed = !same;
    let present_changed = fresh
        .as_ref()
        .is_some_and(|facts| out.present != facts.present);
    let retired_refs_changed = fresh
        .as_ref()
        .is_some_and(|facts| out.retired_refs != facts.retired_refs);
    let retired_present_changed = fresh
        .as_ref()
        .is_some_and(|facts| out.retired_present != facts.retired_present);
    let retired_materials_changed = fresh
        .as_ref()
        .is_some_and(|facts| out.retired_materials != facts.retired_materials);
    let retired_eligibility_changed = fresh
        .as_ref()
        .is_some_and(|facts| out.retired_eligibility != facts.retired_eligibility);
    let bound_changed = fresh.as_ref().is_some_and(|facts| out.bound != facts.bound);
    let stock_primitives_changed = fresh
        .as_ref()
        .is_some_and(|facts| out.stock_primitives != facts.stock_primitives);
    let templates_changed = out.templates != templates;
    if templates_changed && bevy::log::tracing::enabled!(bevy::log::tracing::Level::DEBUG) {
        for (index, &valid) in templates.iter().enumerate() {
            if out.templates.get(index).copied().unwrap_or(false) == valid {
                continue;
            }
            let source = lazy
                .as_ref()
                .and_then(|lazy| lazy.source_scene(index as u32));
            debug!(batch=index, previous=out.templates.get(index), valid,
                source=?source.and_then(Handle::path),
                source_present=source.is_some_and(|source| assets.as_ref().is_some_and(|assets| assets.contains(source.id()))),
                source_invalid=source.is_some_and(|source| out.invalid_templates.contains(&source.id())),
                progress=?lazy.as_ref().and_then(|lazy| lazy.source_progress(index)),
                "cluster source template qualification changed");
        }
    }
    let normal_maps_changed = out.normal_maps != normal_maps;
    let worlds_changed = out.worlds != worlds;
    let light_dependencies_changed = out.light_dependencies != light_dependencies;
    let terrain_change = fresh_terrain
        .as_ref()
        .map_or(TerrainMapChange::Unchanged, |terrain| {
            terrain_map_change(&out.terrain, terrain)
        });
    let (terrain_visible_changed, terrain_visible_withdrawn) = fresh_terrain_visible
        .as_ref()
        .map_or((false, false), |visible| {
            terrain_visibility_change(&out.terrain_visible, visible)
        });
    let candidate_changed = certificate_changed
        || present_changed
        || retired_refs_changed
        || retired_present_changed
        || retired_materials_changed
        || retired_eligibility_changed
        || bound_changed
        || stock_primitives_changed
        || templates_changed
        || normal_maps_changed
        || worlds_changed
        || light_dependencies_changed
        || terrain_change == TerrainMapChange::Invalidating
        || terrain_visible_withdrawn;
    if candidate_changed
        || layout_changed
        || terrain_change == TerrainMapChange::FarAdditionsOnly
        || terrain_visible_changed
    {
        let delta = candidate_changed.then(|| {
            candidate_worlds(
                &out.candidate_inputs(),
                &CandidateInputs {
                    certificate: certificate.as_ref(),
                    present,
                    retired_refs,
                    retired_present,
                    retired_materials,
                    retired_eligibility,
                    bound,
                    stock_primitives,
                    templates: &templates,
                    normal_maps: &normal_maps,
                    worlds: &worlds,
                    light_dependencies: &light_dependencies,
                    terrain,
                    terrain_visible,
                },
            )
        });
        drop(compare_span);
        out.record_change(candidate_changed);
        if let Some(delta) = delta {
            out.candidate_worlds = delta;
        }
        if bevy::log::tracing::enabled!(bevy::log::Level::DEBUG) {
            debug!(
                source_version = out.version,
                candidate_version = out.candidate_version,
                candidate_changed,
                certificate_changed,
                present_changed,
                retired_refs_changed,
                retired_present_changed,
                retired_materials_changed,
                retired_eligibility_changed,
                bound_changed,
                stock_primitives_changed,
                templates_changed,
                normal_maps_changed,
                worlds_changed,
                light_dependencies_changed,
                terrain_changed = terrain_change != TerrainMapChange::Unchanged,
                terrain_far_additions_only = terrain_change == TerrainMapChange::FarAdditionsOnly,
                terrain_invalidating = terrain_change == TerrainMapChange::Invalidating,
                terrain_visible_changed,
                terrain_visible_withdrawn,
                "cluster source inputs changed"
            );
        }
        out.certificate = certificate;
        if let Some(facts) = fresh {
            out.present = facts.present;
            out.retired_refs = facts.retired_refs;
            out.retired_present = facts.retired_present;
            out.retired_materials = facts.retired_materials;
            out.retired_eligibility = facts.retired_eligibility;
            out.bound = facts.bound;
            out.stock_primitives = facts.stock_primitives;
        }
        out.templates = templates;
        out.normal_maps = normal_maps;
        out.worlds = worlds;
        out.light_dependencies = light_dependencies;
        if let Some(terrain) = fresh_terrain {
            out.terrain = terrain;
        }
        if let Some(visible) = fresh_terrain_visible {
            out.terrain_visible = visible;
        }
    }
    // Extracted scene and certificate must describe the same generation.
    if let Some(scene) = scene.as_ref() {
        let scoped = match (&scene.certificate, &out.certificate) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !scoped {
            queries.cache.evaluated = None;
            out.bound.clear();
            out.stock_primitives.clear();
            out.templates.fill(false);
            if out.stock_tangents.iter().any(Option::is_some) {
                out.stock_tangents.fill(None);
                out.layout_version = out.layout_version.wrapping_add(1);
            }
            out.template_rows.stale = true;
            out.candidate_worlds = CandidateWorlds {
                all: true,
                ..default()
            };
            out.record_change(true);
            if bevy::log::tracing::enabled!(bevy::log::Level::DEBUG) {
                debug!(
                    source_version = out.version,
                    candidate_version = out.candidate_version,
                    certificate_scope_mismatch = true,
                    "cluster source inputs changed"
                );
            }
        }
    }
    out.last_work = work;
    if qualification_updated
        || full
        || work.mesh_modified + work.mesh_removed + work.scene_events + work.material_events > 0
    {
        info!(
            extraction = timing.calls,
            certificate_changed = !same,
            qualification_full = work.qualification_full,
            affected_references = work.affected_references,
            journal_entities = work.journal_entities,
            journal_events = work.journal_events,
            journal_complete = work.journal_complete,
            retired_proof_keys_visited = work.retired_proof_keys_visited,
            subjects_updated = work.subjects_updated,
            ancestry_edges = work.ancestry_edges,
            discovery_roots = work.discovery_roots,
            discovery_meshes = work.discovery_meshes,
            indexed_subjects = work.indexed_subjects,
            indexed_nodes = work.indexed_nodes,
            indexed_reference_groups = work.indexed_reference_groups,
            template_full = full,
            template_stale,
            handles_changed = handles.is_changed(),
            own_statics,
            query_filters_changed = queries.query_filters.is_changed(),
            structural_changed,
            fetch_gap,
            hot_changed,
            identity_changed,
            other_changed,
            scene_events = work.scene_events,
            material_events = work.material_events,
            mesh_modified = work.mesh_modified,
            mesh_removed = work.mesh_removed,
            scene_modified = work.scene_modified,
            scene_removed = work.scene_removed,
            stock_rows_revoked = work.stock_rows_revoked,
            stock_rows_requalified = work.stock_rows_requalified,
            roots_scanned = work.roots_scanned,
            meshes_scanned = work.meshes_scanned,
            primitives_scanned = work.primitives_scanned,
            template_rows = work.template_rows,
            qualification_ms = qualification_elapsed.as_secs_f64() * 1000.0,
            templates_ms = templates_elapsed.as_secs_f64() * 1000.0,
            source_version = out.version,
            candidate_version = out.candidate_version,
            "cluster source invalidation work"
        );
    }
    if timing.calls.is_multiple_of(120) {
        info!(
            calls = timing.calls,
            qualification_reused = timing.reused,
            qualification_refreshes = out.qualification_refreshes,
            hot_refreshes = timing.hot_refreshes,
            identity_refreshes = timing.identity_refreshes,
            other_refreshes = timing.other_refreshes,
            structural_refreshes = timing.structural_refreshes,
            "cluster source qualification cache measurements"
        );
    }
    let elapsed = started.elapsed();
    timing
        .total
        .record("cluster::handoff::extract_sources", elapsed);
    timing.excluding_terrain.record(
        "cluster::handoff::extract_sources_excluding_terrain",
        elapsed.saturating_sub(terrain_elapsed),
    );
    timing.terrain.record(
        "cluster::handoff::terrain_identity_and_copy",
        terrain_elapsed,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Domain {
    Camera(Entity),
    Shadow(Entity, RetainedViewEntity),
}

#[derive(Default)]
struct DomainFacts {
    consumers: Vec<RetainedViewEntity>,
    visible: bevy::platform::collections::HashSet<MainEntity>,
}

#[derive(Resource, Default)]
struct Filters {
    frames: u64,
    /// One [`SelectionIndex`] per worldspace a view has selected in.
    indexes: HashMap<Option<u32>, SelectionIndex>,
    /// The current [`SelectionBase`]s, shared by views with the same inputs.
    bases: Vec<Arc<SelectionBase>>,
    saved: HashMap<Domain, ClassFilter>,
    active: HashSet<Domain>,
}

pub(super) struct Selection {
    stamp: [u64; 7],
    visible: bevy::platform::collections::HashSet<MainEntity>,
    readiness: Vec<bool>,
    stock_materials: HashMap<MainEntity, Option<bevy::asset::UntypedAssetId>>,
    mask: super::super::instance_mask::InstanceMask,
    hidden: bevy::platform::collections::HashSet<MainEntity>,
    terrain_blocks: HashSet<u32>,
    suppressed_contributors: usize,
    suppressed_trees: usize,
}

/// Exact late-evaluation inputs for immutable terrain geometry in one ViewGpu.
/// Asset readiness and queued membership are still evaluated every frame before
/// comparing these resulting sets; only record encoding is skipped.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct TerrainClipInputs {
    world: Option<u32>,
    origin: IVec2,
    forward_covered: HashSet<(IVec2, u8)>,
    depth_covered: HashSet<(IVec2, u8)>,
    ready_blocks: HashSet<u32>,
    target_offset: [u32; 3],
}

impl TerrainClipInputs {
    fn target_offset_matches(&self, offset: Vec3) -> bool {
        self.target_offset == offset.to_array().map(f32::to_bits)
    }
}

pub(super) fn install(app: &mut App) {
    crate::streaming::identity_changes::install(app.world_mut());
    destination::install(app);
    source_fallback_bridge::install(app);
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render
        .init_resource::<Sources>()
        .init_resource::<Filters>()
        .add_systems(
            ExtractSchedule,
            extract_sources.in_set(super::SourceExtraction),
        )
        .add_systems(
            Render,
            select_instances
                .after(crate::streaming::native_lod::NativeLodSystems::Filter)
                .in_set(RenderSystems::Queue)
                .before(RenderSystems::QueueMeshes)
                .after(queue_cluster_batches)
                .after(queue_cluster_prepass)
                .after(queue_cluster_shadows),
        )
        .add_systems(
            Render,
            finalize_terrain_clips
                .after(RenderSystems::PrepareBindGroups)
                .before(crate::swap_bridge::systems::DestinationSystems::Evaluate)
                .before(RenderSystems::Render),
        )
        .add_systems(
            Render,
            restore_stock
                .in_set(RenderSystems::Cleanup)
                .before(crate::streaming::native_lod::NativeLodSystems::Restore),
        );
}

/// Final stock preparation can finish after the early replacement choice. Clip
/// against actual queued stock only after its bind groups exist, without changing
/// either representation's queue or instance selection at this late point.
#[allow(clippy::too_many_arguments)]
fn finalize_terrain_clips(
    frame: Res<ClusterFrame>,
    gpu: Option<ResMut<ClusterGpu>>,
    sources: Res<Sources>,
    queue: Res<RenderQueue>,
    stock: super::super::stock_drawable::StockDrawable,
    phases: crate::swap_bridge::systems::QueuePhases,
    views: Query<(
        Entity,
        &ExtractedView,
        Option<&LightEntity>,
        Option<&RenderVisibleEntities>,
    )>,
    shadows: Query<&RenderShadowMapVisibleEntities>,
) {
    perf_span!("cluster::terrain_clips::total");
    use super::super::stock_drawable::StockPassFamily;
    let Some(mut gpu) = gpu else {
        return;
    };
    if gpu.terrain_blocks.is_empty() {
        return;
    }
    let clip_offset = u64::from(gpu.total_slots.max(1)) * 16;
    for (entity, view, light, visible) in &views {
        let key = view.retained_view_entity;
        let Some(selection) = gpu.views.get(&key).and_then(|view| view.selection.as_ref()) else {
            continue;
        };
        let world = sources
            .worlds
            .get(&key.main_entity)
            .or_else(|| sources.worlds.get(&key.auxiliary_entity))
            .copied()
            .flatten();
        let domain = light.map_or(Domain::Camera(entity), |light| shadow_domain(light, key));
        let visible = match domain {
            Domain::Camera(_) => visible,
            Domain::Shadow(light, key) => shadows
                .get(light)
                .ok()
                .and_then(|shadow| shadow.subviews.get(&key)),
        };
        let shadow = light.is_some();
        #[cfg(feature = "tracy")]
        let membership_span =
            bevy::log::info_span!("cluster::terrain_clips::phase_membership").entered();
        let forward_members = phases.forward(&key);
        let depth_members = if shadow {
            phases.shadow(&key)
        } else {
            phases.prepass(&key)
        };
        #[cfg(feature = "tracy")]
        drop(membership_span);
        // Separate records are essential: stock can be ready in forward while
        // its depth pipeline is still compiling, or vice versa. Actual queued
        // shadow membership also proves the stock queue's render-layer check.
        #[cfg(feature = "tracy")]
        let coverage_span = bevy::log::info_span!("cluster::terrain_clips::coverage").entered();
        let mut forward_covered = HashSet::new();
        let mut depth_covered = HashSet::new();
        for (_, main) in visible
            .and_then(|visible| visible.get::<Mesh3d>())
            .into_iter()
            .flat_map(|class| class.iter_visible())
        {
            let Some(identity) = sources.terrain.get(main) else {
                continue;
            };
            if Some(identity.worldspace_id) != world {
                continue;
            }
            let quadrant = (identity.grid, identity.quadrant);
            if !shadow
                && forward_members.queued(*main)
                && stock.drawable(*main, key, StockPassFamily::Forward)
            {
                forward_covered.insert(quadrant);
            }
            if depth_members.queued(*main)
                && stock.drawable(
                    *main,
                    key,
                    if shadow {
                        StockPassFamily::Shadow
                    } else {
                        StockPassFamily::Prepass
                    },
                )
            {
                depth_covered.insert(quadrant);
            }
        }
        #[cfg(feature = "tracy")]
        drop(coverage_span);
        if gpu.views[&key]
            .terrain_clip_inputs
            .as_ref()
            .is_some_and(|inputs| {
                inputs.world == world
                    && inputs.target_offset_matches(frame.origin_offset)
                    && inputs.origin == sources.origin
                    && inputs.forward_covered == forward_covered
                    && inputs.depth_covered == depth_covered
                    && inputs.ready_blocks == selection.terrain_blocks
            })
        {
            continue;
        }
        perf_span!("cluster::terrain_clips::rebuild");
        // Clone ready blocks only after an exact input change. The new coverage
        // sets move into the cache; neither counts nor hashes stand in for them.
        let inputs = TerrainClipInputs {
            target_offset: frame.origin_offset.to_array().map(f32::to_bits),
            world,
            origin: sources.origin,
            forward_covered,
            depth_covered,
            ready_blocks: selection.terrain_blocks.clone(),
        };
        let mut bytes = Vec::new();
        for (index, block) in gpu.terrain_blocks.iter().enumerate() {
            let ready = inputs.ready_blocks.contains(&(index as u32));
            block.append_view_records(
                inputs.world,
                inputs.origin,
                &inputs.forward_covered,
                &inputs.depth_covered,
                ready,
                &mut bytes,
            );
        }
        if light.is_none() && bevy::log::tracing::enabled!(bevy::log::tracing::Level::DEBUG) {
            use std::hash::{Hash, Hasher};
            let mut selected: Vec<_> = inputs
                .ready_blocks
                .iter()
                .map(|&index| {
                    let block = &gpu.terrain_blocks[index as usize];
                    (
                        block.worldspace_id,
                        block.anchor.x,
                        block.anchor.y,
                        block.side_cells,
                    )
                })
                .collect();
            selected.sort_unstable();
            let mut forward: Vec<_> = inputs
                .forward_covered
                .iter()
                .map(|(grid, q)| (grid.x, grid.y, *q))
                .collect();
            let mut depth: Vec<_> = inputs
                .depth_covered
                .iter()
                .map(|(grid, q)| (grid.x, grid.y, *q))
                .collect();
            forward.sort_unstable();
            depth.sort_unstable();
            let mut hidden_stock: Vec<_> = sources
                .stock_primitives
                .iter()
                .filter(|(_, main)| selection.hidden.contains(*main))
                .map(|(identity, _)| *identity)
                .collect();
            hidden_stock.sort_unstable();
            let mut hash = std::hash::DefaultHasher::new();
            hidden_stock.hash(&mut hash);
            debug!(?key, ?world, origin=?inputs.origin, camera=?view.world_from_view.to_matrix(),
                projection=?view.clip_from_view, ?selected, ?forward, ?depth,
                hidden_stock_count=hidden_stock.len(), hidden_stock_hash=hash.finish(),
                clip_hash={let mut h=std::hash::DefaultHasher::new(); bytes.hash(&mut h); h.finish()},
                "cluster terrain representation inputs changed");
        }
        let view = gpu.views.get_mut(&key).unwrap();
        if bytes != view.terrain_clip_bytes {
            #[cfg(feature = "tracy")]
            let _span =
                bevy::log::info_span!("cluster::upload::terrain_clips", bytes = bytes.len())
                    .entered();
            queue.write_buffer(&view.visible, clip_offset, &bytes);
            view.terrain_clip_bytes = bytes;
        }
        view.terrain_clip_inputs = Some(inputs);
    }
}

fn shadow_domain(light: &LightEntity, key: RetainedViewEntity) -> Domain {
    match light {
        LightEntity::Directional { light_entity, .. } => Domain::Shadow(*light_entity, key),
        LightEntity::Point {
            light_entity,
            face_index,
        } => Domain::Shadow(
            *light_entity,
            RetainedViewEntity {
                auxiliary_entity: Entity::PLACEHOLDER.into(),
                subview_index: *face_index as u32,
                ..key
            },
        ),
        LightEntity::Spot { light_entity } => Domain::Shadow(
            *light_entity,
            RetainedViewEntity {
                auxiliary_entity: Entity::PLACEHOLDER.into(),
                subview_index: 0,
                ..key
            },
        ),
    }
}

fn table_group(view: &ViewGpu, batch: usize) -> Option<u32> {
    let row = view
        .table
        .get(batch.checked_mul(16)?..batch.checked_mul(16)?.checked_add(16)?)?;
    (u32::from_le_bytes(row[..4].try_into().ok()?) != 0)
        .then(|| u32::from_le_bytes(row[4..8].try_into().unwrap()))
}

/// Every batch's [`batch_ready`] for one view. Each check is a handful of independent lookups and
/// a scene has thousands of batches, every one checked for every view every frame, so the batches
/// are split across the compute task pool; the result is in batch order, as a sequential pass
/// would give it.
/// [`views_batch_readiness`], reusing the view's last answer while nothing it was decided from has
/// changed. A batch's readiness reads the view's plan (draws, memo, representatives, prepass and
/// shadow pipelines) and the prepared materials. The plan comes from the queue
/// walk, whose stamp is kept only when the walk left nothing out and moves with the batch
/// materials' version (bumped by any asset event naming a batch material), the instance layout
/// and generation; the plan epoch moves with the plan; the pipeline maps are hashed here. Under the
/// same inputs a ready batch stays ready, so only the batches that were not are checked again
/// (a pipeline still compiling, a material still preparing).
fn cached_batch_readiness(
    gpu: &mut ClusterGpu,
    key: RetainedViewEntity,
    materials_version: Option<u64>,
    shadow: bool,
    prepass: bool,
    dependencies: (
        &PipelineCache,
        &ErasedRenderAssets<PreparedMaterial>,
        &MaterialBindGroupAllocators,
    ),
) -> Vec<bool> {
    use std::hash::{Hash, Hasher};
    let view = &gpu.views[&key];
    // A camera view's plan is the walk's; a shadow view's comes from the shadow queue, whose
    // plan hash covers the batches' generation and layout but not their materials, so the
    // materials' version is part of the stamp too.
    let stamp = (shadow || view.walk_stamp.is_some()).then(|| {
        perf_span!("cluster::readiness::stamp");
        let mut hasher = std::hash::DefaultHasher::new();
        view.walk_stamp.hash(&mut hasher);
        view.plan_hash.hash(&mut hasher);
        materials_version.hash(&mut hasher);
        view.plan_epoch.hash(&mut hasher);
        for pipelines in [&view.prepass_pipelines, &view.shadow_pipelines] {
            let mut sorted: Vec<_> = pipelines
                .iter()
                .map(|(group, id)| (*group, id.id()))
                .collect();
            sorted.sort_unstable();
            sorted.hash(&mut hasher);
        }
        (shadow, prepass).hash(&mut hasher);
        (gpu.generation, gpu.layout_epoch, gpu.batches.len()).hash(&mut hasher);
        hasher.finish()
    });
    let readiness = match (&view.readiness_memo, stamp) {
        (Some((previous, cached)), Some(stamp)) if *previous == stamp => {
            let mut readiness = cached.clone();
            for (batch, ready) in readiness.iter_mut().enumerate() {
                if !*ready {
                    *ready = batch_ready(gpu, view, batch, shadow, prepass, dependencies);
                }
            }
            readiness
        }
        _ => views_batch_readiness(gpu, view, shadow, prepass, dependencies),
    };
    // Preserve the cache's allocation on a hit without weakening its exact stamp.
    let memo = &mut gpu.views.get_mut(&key).unwrap().readiness_memo;
    if let (Some((previous, cached)), Some(stamp)) = (memo.as_mut(), stamp) {
        *previous = stamp;
        cached.clone_from(&readiness);
    } else {
        *memo = stamp.map(|stamp| (stamp, readiness.clone()));
    }
    readiness
}

fn views_batch_readiness(
    gpu: &ClusterGpu,
    view: &ViewGpu,
    shadow: bool,
    prepass: bool,
    dependencies: (
        &PipelineCache,
        &ErasedRenderAssets<PreparedMaterial>,
        &MaterialBindGroupAllocators,
    ),
) -> Vec<bool> {
    /// Below this many batches a chunk costs more to hand out than it saves.
    const CHUNK: usize = 512;
    let batches = gpu.batches.len();
    let ready = |range: std::ops::Range<usize>| -> Vec<bool> {
        range
            .map(|batch| batch_ready(gpu, view, batch, shadow, prepass, dependencies))
            .collect()
    };
    let pool = bevy::tasks::ComputeTaskPool::get();
    let threads = pool.thread_num().max(1);
    if batches <= CHUNK || threads == 1 {
        return ready(0..batches);
    }
    let chunk = batches.div_ceil(threads).max(CHUNK);
    pool.scope(|scope| {
        for start in (0..batches).step_by(chunk) {
            let ready = &ready;
            scope.spawn(async move { ready(start..(start + chunk).min(batches)) });
        }
    })
    .concat()
}

fn batch_ready(
    gpu: &ClusterGpu,
    view: &ViewGpu,
    batch: usize,
    shadow: bool,
    prepass: bool,
    dependencies: (
        &PipelineCache,
        &ErasedRenderAssets<PreparedMaterial>,
        &MaterialBindGroupAllocators,
    ),
) -> bool {
    let (cache, materials, allocators) = dependencies;
    let Some(group) = table_group(view, batch) else {
        return false;
    };
    let Some(draw) = view.draws.get(&group) else {
        return false;
    };
    let Some(asset) = gpu.batches[batch].material else {
        return false;
    };
    let Some(material) = materials.get(asset) else {
        return false;
    };
    if allocators
        .get(&asset.type_id())
        .and_then(|allocator| allocator.get(material.binding.group))
        .is_none_or(|slab| slab.bind_group().is_none())
    {
        return false;
    }
    // A slot/group rewrite or a hot-reloaded specialization cannot inherit an old plan.
    if gpu.batches[batch].material_slot != Some(u32::from(material.binding.slot)) {
        return false;
    }
    if shadow {
        if !material.properties.shadows_enabled {
            return false;
        }
        match (material.properties.alpha_mode, &gpu.batches[batch].cutout) {
            (AlphaMode::Opaque, None) => {}
            (AlphaMode::Mask(_), Some(cutout))
                if cutout.asset == asset
                    && cutout.material_group == material.binding.group.0
                    && cutout.material_key == material.properties.material_key => {}
            _ => return false,
        }
        return view
            .shadow_pipelines
            .get(&group)
            .is_some_and(|pipeline| cache.get_render_pipeline(*pipeline).is_some());
    }
    let Some(memo) = view.memo.get(batch).copied().flatten() else {
        return false;
    };
    if memo.asset_id != asset
        || memo.material_group != material.binding.group.0
        || memo.material_slot != u32::from(material.binding.slot)
        || memo.material_address != std::ptr::from_ref(material) as usize
        || memo.alpha_mode != material.properties.alpha_mode
    {
        return false;
    }
    let Some(representative) = view.plan_rep(draw.representative) else {
        return false;
    };
    cache.get_render_pipeline(representative.pipeline).is_some()
        && (!prepass
            || !material.properties.prepass_enabled
            || view
                .prepass_pipelines
                .get(&representative.index)
                .is_some_and(|pipeline| cache.get_render_pipeline(*pipeline).is_some()))
}

/// One worldspace's placed instances, sorted once by batch with every per-instance lookup in the
/// certificate already made, so a [`SelectionBase`] rebuild walks arrays. Instances only ever
/// arrive at the end of `ClusterGpu::placed`, so the index appends the new ones; it is rebuilt
/// only for another `ClusterGpu` (`ClusterGpu::identity`). What it reads of the scene is fixed for
/// the life of one: a placed instance's mesh and certificate record never change, and a mesh's
/// tangent check is computed once.
pub(super) struct SelectionIndex {
    /// The `ClusterGpu::identity` the index was built from.
    gpu: u64,
    /// How many of `ClusterGpu::placed` are indexed.
    indexed: usize,
    batches: Vec<BatchInstances>,
    /// Positions into `batches[batch].objects` of each reference's instances.
    by_reference: HashMap<u32, Vec<(u32, u32)>>,
}

/// The CPU data used to build the selection index and base. Kept separate from `ClusterGpu`,
/// whose buffers require a render device and cannot be constructed in unit tests.
#[derive(Clone, Copy)]
struct SelectionData<'a> {
    identity: u64,
    placed: &'a [ClusterInstance],
    batch_count: usize,
    certificate: Option<&'a super::super::reader::CertifiedPackage>,
    terrain_blocks: &'a [super::super::terrain::TerrainViewBlock],
    capacity: u32,
    meshes: &'a [ClusterMeshAsset],
}

#[derive(Default)]
struct BatchInstances {
    /// Terrain instances of this worldspace's blocks, with their block.
    terrain: Vec<(u32, u32)>,
    /// Scene fixtures (no certificate): enabled whenever their batch is ready.
    fixtures: Vec<u32>,
    objects: Vec<IndexedObject>,
}

struct IndexedObject {
    index: u32,
    /// The certified record and reference, for an object instance.
    object: Option<(RecordKey, u32)>,
    /// The instance's mesh has no usable tangents, so a normal-mapped batch cannot draw it.
    lacks_tangents: bool,
    /// The placement facts the native far-LOD rule reads, for an object instance.
    far: Option<FarFacts>,
}

#[derive(Clone, Copy)]
struct FarFacts {
    /// Placed worldspace and cell, when the certificate has the placement.
    cell: Option<(u32, IVec2)>,
    is_tree: bool,
    base_has_distant_lod: Option<bool>,
    visible_when_distant: bool,
}

impl FarFacts {
    /// Whether the native far LOD owns this placement where the grid is now
    /// ([`crate::streaming::native_lod::suppress_unqualified_far_cluster`]).
    fn suppressed(&self, native: Option<&crate::streaming::native_lod::FarNativePolicy>) -> bool {
        let inside = self.cell.is_some_and(|(world, cell)| {
            native.is_some_and(|policy| policy.cluster_inside_grid(world, cell))
        });
        crate::streaming::native_lod::suppress_unqualified_far_cluster(
            inside,
            self.is_tree,
            self.base_has_distant_lod,
            self.visible_when_distant,
        )
    }
}

impl SelectionIndex {
    fn new(identity: u64) -> Self {
        Self {
            gpu: identity,
            indexed: 0,
            batches: Vec::new(),
            by_reference: HashMap::new(),
        }
    }

    /// Indexes the instances that arrived since the last call.
    fn append(&mut self, data: SelectionData<'_>, world: Option<u32>) {
        if self.batches.len() < data.batch_count {
            self.batches
                .resize_with(data.batch_count, BatchInstances::default);
        }
        let certificate = data.certificate;
        for (index, instance) in data.placed.iter().enumerate().skip(self.indexed) {
            let index = index as u32;
            let Some(entry) = self.batches.get_mut(instance.batch as usize) else {
                continue;
            };
            if matches!(instance.identity, ClusterInstanceIdentity::Fixture)
                && certificate.is_none()
            {
                entry.fixtures.push(index);
                continue;
            }
            if world.is_none() || instance.identity.placed_worldspace(certificate) != world {
                continue;
            }
            if let ClusterInstanceIdentity::Terrain { block, .. } = instance.identity {
                if data
                    .terrain_blocks
                    .get(block as usize)
                    .is_some_and(|metadata| Some(metadata.worldspace_id) == world)
                {
                    entry.terrain.push((index, block));
                }
                continue;
            }
            let lacks_tangents = data
                .meshes
                .get(instance.mesh as usize)
                .is_none_or(|mesh| !mesh.tangents_usable());
            let mut object = None;
            let mut far = None;
            if let ClusterInstanceIdentity::Object(record) = instance.identity {
                let Some(identity) =
                    certificate.and_then(|certificate| certificate.record(&record))
                else {
                    continue;
                };
                // The data set currently has no normalized per-level BTO paths
                // or qualified tree relation. Native parity owns far geometry.
                // Exact source IDs are still replaced normally inside the loaded grid.
                let placed = certificate.and_then(|p| p.placed_reference(identity.reference));
                far = Some(FarFacts {
                    cell: placed.map(|placed| {
                        (
                            placed.snapshot.placed_worldspace_id,
                            IVec2::new(placed.snapshot.cell.0, placed.snapshot.cell.1),
                        )
                    }),
                    is_tree: placed.is_some_and(|placed| placed.is_tree),
                    base_has_distant_lod: placed.and_then(|placed| placed.base_has_distant_lod),
                    // No placement data: assume a contributor (conservative).
                    visible_when_distant: placed.is_none_or(|placed| placed.visible_when_distant),
                });
                object = Some((record, identity.reference));
                self.by_reference
                    .entry(identity.reference)
                    .or_default()
                    .push((instance.batch, entry.objects.len() as u32));
            }
            entry.objects.push(IndexedObject {
                index,
                object,
                lacks_tangents,
                far,
            });
        }
        self.indexed = data.placed.len();
    }
}

/// The part of a view's selection that depends neither on its visible set nor on which stock
/// references are instantiated: every instance of the view's worldspace whose batch is ready,
/// template loaded and tangents usable, from the [`SelectionIndex`]. Views with the same inputs
/// share one. The per-frame pass takes the instantiated references' instances out of it again and
/// decides them against the visible set and the stock material.
///
/// Batches become ready and their templates load one at a time for minutes while the views move,
/// and cluster blocks keep arriving, so a base is updated per changed batch and per new instance
/// instead of rebuilt; a rebuild is left for another package, worldspace, drawability or grid.
#[derive(Clone)]
pub(super) struct SelectionBase {
    inputs: BaseInputs,
    /// What each batch's instances were last decided with.
    states: Vec<BatchState>,
    /// How many of the index's instances the base covers.
    indexed: usize,
    /// Eligible instances.
    mask: super::super::instance_mask::InstanceMask,
    /// Enabled terrain instances per terrain block.
    terrain_counts: HashMap<u32, u32>,
    terrain_blocks: HashSet<u32>,
    /// Per batch: object instances withheld for native far LOD, (contributors, trees).
    suppressed: Vec<(usize, usize)>,
    suppressed_contributors: usize,
    suppressed_trees: usize,
}

/// What forces a [`SelectionBase`] rebuild.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BaseInputs {
    gpu: u64,
    world: Option<u32>,
    drawable: bool,
    native_revision: u64,
}

/// What a batch's instances are decided with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BatchState {
    ready: bool,
    template: bool,
    normal_mapped: bool,
}

impl SelectionBase {
    fn states(sources: &Sources, readiness: &[bool], batches: usize) -> Vec<BatchState> {
        (0..batches)
            .map(|batch| BatchState {
                ready: readiness.get(batch).copied().unwrap_or(false),
                template: sources.templates.get(batch).copied().unwrap_or(false),
                normal_mapped: sources.normal_maps.get(batch).copied().unwrap_or(true),
            })
            .collect()
    }

    /// Whether this base already answers for these inputs.
    fn matches(&self, inputs: BaseInputs, states: &[BatchState], index: &SelectionIndex) -> bool {
        self.inputs == inputs && self.indexed == index.indexed && self.states == states
    }

    fn build(
        data: &SelectionData<'_>,
        index: &SelectionIndex,
        native: Option<&crate::streaming::native_lod::FarNativePolicy>,
        inputs: BaseInputs,
        states: Vec<BatchState>,
    ) -> Self {
        let mut base = Self {
            inputs,
            states: vec![BatchState::default(); states.len()],
            indexed: 0,
            mask: super::super::instance_mask::InstanceMask::new(data.capacity),
            terrain_counts: HashMap::new(),
            terrain_blocks: HashSet::new(),
            suppressed: vec![(0, 0); states.len()],
            suppressed_contributors: 0,
            suppressed_trees: 0,
        };
        base.advance(index, native, &states);
        base
    }

    /// The base for `inputs` and `states`, from this one: only the batches whose state changed and
    /// the instances indexed since are decided again.
    fn update(
        &self,
        data: &SelectionData<'_>,
        index: &SelectionIndex,
        native: Option<&crate::streaming::native_lod::FarNativePolicy>,
        inputs: BaseInputs,
        states: Vec<BatchState>,
    ) -> Self {
        if self.inputs != inputs
            || self.states.len() > states.len()
            || self.mask.capacity() != data.capacity
        {
            return Self::build(data, index, native, inputs, states);
        }
        let mut base = self.clone();
        base.advance(index, native, &states);
        #[cfg(debug_assertions)]
        {
            let full = Self::build(data, index, native, inputs, states);
            debug_assert!(
                base.mask == full.mask
                    && base.terrain_blocks == full.terrain_blocks
                    && base.suppressed == full.suppressed,
                "an updated selection base differs from a full rebuild"
            );
        }
        base
    }

    /// Brings the base to `states` over every instance of `index`.
    fn advance(
        &mut self,
        index: &SelectionIndex,
        native: Option<&crate::streaming::native_lod::FarNativePolicy>,
        states: &[BatchState],
    ) {
        let previous = self.indexed;
        self.states.resize(states.len(), BatchState::default());
        self.suppressed.resize(states.len(), (0, 0));
        for (batch, entry) in index.batches.iter().enumerate() {
            let state = states[batch];
            if self.states[batch] != state {
                // Withdraw what the old state enabled, then decide every covered instance again.
                self.clear_batch(entry, batch, previous);
                self.states[batch] = state;
                self.enable(entry, batch, state, native, 0, previous);
            }
            // Instances indexed since the last update.
            if index.indexed > previous {
                self.enable(entry, batch, state, native, previous, usize::MAX);
            }
        }
        self.indexed = index.indexed;
        self.terrain_blocks = self.terrain_counts.keys().copied().collect();
        self.suppressed_contributors = self.suppressed.iter().map(|counts| counts.0).sum();
        self.suppressed_trees = self.suppressed.iter().map(|counts| counts.1).sum();
    }

    /// Clears every instance of `batch` among the first `covered` placed instances.
    fn clear_batch(&mut self, entry: &BatchInstances, batch: usize, covered: usize) {
        for &(instance, block) in span(&entry.terrain, |item| item.0, 0, covered) {
            if self.mask.get(instance) {
                self.mask
                    .set(instance, false)
                    .expect("terrain instance fits capacity");
                if let Some(count) = self.terrain_counts.get_mut(&block) {
                    *count -= 1;
                    if *count == 0 {
                        self.terrain_counts.remove(&block);
                    }
                }
            }
        }
        for &instance in span(&entry.fixtures, |item| *item, 0, covered) {
            self.mask
                .set(instance, false)
                .expect("GPU instance fits allocated capacity");
        }
        for object in span(&entry.objects, |item| item.index, 0, covered) {
            self.mask
                .set(object.index, false)
                .expect("GPU instance fits allocated capacity");
        }
        self.suppressed[batch] = (0, 0);
    }

    /// Enables what `state` allows of `batch`'s instances whose placed index is in `from..to`.
    fn enable(
        &mut self,
        entry: &BatchInstances,
        batch: usize,
        state: BatchState,
        native: Option<&crate::streaming::native_lod::FarNativePolicy>,
        from: usize,
        to: usize,
    ) {
        if !self.inputs.drawable || !state.ready {
            return;
        }
        for &(instance, block) in span(&entry.terrain, |item| item.0, from, to) {
            self.mask
                .set(instance, true)
                .expect("terrain instance fits capacity");
            *self.terrain_counts.entry(block).or_default() += 1;
        }
        for &instance in span(&entry.fixtures, |item| *item, from, to) {
            self.mask
                .set(instance, true)
                .expect("GPU instance fits allocated capacity");
        }
        if !state.template {
            return;
        }
        for object in span(&entry.objects, |item| item.index, from, to) {
            if state.normal_mapped && object.lacks_tangents {
                continue;
            }
            if let Some(far) = object.far
                && far.suppressed(native)
            {
                if far.is_tree {
                    self.suppressed[batch].1 += 1;
                } else {
                    self.suppressed[batch].0 += 1;
                }
                continue;
            }
            self.mask
                .set(object.index, true)
                .expect("GPU instance fits allocated capacity");
        }
    }
}

/// The items of `items`, in placed-instance order, whose placed index is in `from..to`.
fn span<T>(items: &[T], placed: impl Fn(&T) -> u32, from: usize, to: usize) -> &[T] {
    let start = items.partition_point(|item| (placed(item) as usize) < from);
    let end = items.partition_point(|item| (placed(item) as usize) < to);
    &items[start..end]
}

#[cfg(test)]
mod incremental_selection_tests {
    use super::*;

    struct Expected {
        mask: super::super::super::instance_mask::InstanceMask,
        terrain_blocks: HashSet<u32>,
        contributors: usize,
        trees: usize,
    }

    // Deliberately reads each placed instance directly; it does not use SelectionIndex or spans.
    fn reference(
        data: &SelectionData<'_>,
        world: Option<u32>,
        drawable: bool,
        states: &[BatchState],
        native: Option<&crate::streaming::native_lod::FarNativePolicy>,
    ) -> Expected {
        let mut expected = Expected {
            mask: super::super::super::instance_mask::InstanceMask::new(data.capacity),
            terrain_blocks: HashSet::new(),
            contributors: 0,
            trees: 0,
        };
        if !drawable {
            return expected;
        }
        for (at, instance) in data.placed.iter().enumerate() {
            let Some(state) = states.get(instance.batch as usize) else {
                continue;
            };
            if !state.ready {
                continue;
            }
            match instance.identity {
                ClusterInstanceIdentity::Fixture if data.certificate.is_none() => {
                    expected.mask.set(at as u32, true).unwrap();
                }
                ClusterInstanceIdentity::Terrain { block, .. }
                    if instance.identity.placed_worldspace(data.certificate) == world
                        && data
                            .terrain_blocks
                            .get(block as usize)
                            .is_some_and(|metadata| Some(metadata.worldspace_id) == world) =>
                {
                    expected.mask.set(at as u32, true).unwrap();
                    expected.terrain_blocks.insert(block);
                }
                _ => {}
            }
        }
        let _ = native;
        expected
    }

    fn fixture(batch: u32, identity: ClusterInstanceIdentity) -> ClusterInstance {
        ClusterInstance {
            identity,
            mesh: 0,
            batch,
            world_from_local: Mat4::IDENTITY,
            source_worldspace: 0,
        }
    }

    #[test]
    fn appended_fixture_selection_updates_match_direct_reference_after_each_transition() {
        let metadata = vec![
            super::super::super::terrain::TerrainViewBlock {
                worldspace_id: 7,
                anchor: IVec2::ZERO,
                side_cells: 1,
                cells: vec![],
                batch: Some(1),
            },
            super::super::super::terrain::TerrainViewBlock {
                worldspace_id: 7,
                anchor: IVec2::ONE,
                side_cells: 1,
                cells: vec![],
                batch: Some(1),
            },
        ];
        let mut placed = vec![fixture(0, ClusterInstanceIdentity::Fixture)];
        let world = Some(7);
        let native = None;
        let mut inputs = BaseInputs {
            gpu: 11,
            world,
            drawable: true,
            native_revision: 0,
        };
        let mut states = vec![BatchState::default(); 3];
        let (mut index, mut base) = {
            let data = SelectionData {
                identity: 11,
                placed: &placed,
                batch_count: 3,
                certificate: None,
                terrain_blocks: &metadata,
                capacity: 8,
                meshes: &[],
            };
            let mut index = SelectionIndex::new(data.identity);
            index.append(data, world);
            let base = SelectionBase::build(&data, &index, native, inputs, states.clone());
            (index, base)
        };

        let steps: &[(Option<ClusterInstance>, usize, BatchState, bool)] = &[
            (
                Some(fixture(
                    1,
                    ClusterInstanceIdentity::Terrain {
                        block: 0,
                        worldspace: 7,
                        tier: 0,
                        anchor: [0, 0],
                    },
                )),
                1,
                BatchState {
                    ready: true,
                    template: false,
                    normal_mapped: true,
                },
                true,
            ),
            (
                Some(fixture(0, ClusterInstanceIdentity::Fixture)),
                0,
                BatchState {
                    ready: true,
                    template: false,
                    normal_mapped: true,
                },
                true,
            ),
            (
                Some(fixture(
                    1,
                    ClusterInstanceIdentity::Terrain {
                        block: 1,
                        worldspace: 7,
                        tier: 0,
                        anchor: [1, 0],
                    },
                )),
                1,
                BatchState {
                    ready: true,
                    template: true,
                    normal_mapped: false,
                },
                true,
            ),
            (
                None,
                0,
                BatchState {
                    ready: false,
                    template: false,
                    normal_mapped: true,
                },
                true,
            ),
            (
                None,
                2,
                BatchState {
                    ready: true,
                    template: true,
                    normal_mapped: true,
                },
                false,
            ),
        ];
        for (addition, batch, state, drawable) in steps {
            if let Some(instance) = addition {
                placed.push(*instance);
            }
            let data = SelectionData {
                identity: 11,
                placed: &placed,
                batch_count: 3,
                certificate: None,
                terrain_blocks: &metadata,
                capacity: 8,
                meshes: &[],
            };
            states[*batch] = *state;
            inputs.drawable = *drawable;
            index.append(data, world);
            base = base.update(&data, &index, native, inputs, states.clone());
            let expected = reference(&data, world, *drawable, &states, native);
            assert_eq!(
                base.mask, expected.mask,
                "mask after step {batch} / {drawable}"
            );
            assert_eq!(
                base.terrain_blocks, expected.terrain_blocks,
                "terrain after step {batch} / {drawable}"
            );
            assert_eq!(
                (base.suppressed_contributors, base.suppressed_trees),
                (expected.contributors, expected.trees)
            );
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn select_instances(
    gpu: Option<ResMut<ClusterGpu>>,
    sources: Res<Sources>,
    frame: Res<ClusterFrame>,
    pipelines: Res<ClusterPipelines>,
    cache: Res<PipelineCache>,
    queue: Res<RenderQueue>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    allocators: Res<MaterialBindGroupAllocators>,
    stock_materials: Res<RenderMaterialInstances>,
    prepass_phases: Res<ViewBinnedRenderPhases<Opaque3dPrepass>>,
    mut views: Query<SelectionViewState>,
    mut shadows: Query<&mut RenderShadowMapVisibleEntities>,
    scene: Option<Res<ClusterScene>>,
    mut filters: ResMut<Filters>,
    (native, settings, batch_materials, runtime_state): (
        Option<Res<crate::streaming::native_lod::FarNativePolicy>>,
        Option<Res<ClusterSettings>>,
        Option<Res<super::ClusterBatchMaterials>>,
        Option<Res<super::super::runtime_state::RuntimeReferenceState>>,
    ),
) {
    let Some(mut gpu) = gpu else {
        return;
    };
    perf_span!("cluster::select::total");
    let own_statics = settings
        .as_ref()
        .is_some_and(|settings| settings.own_statics);
    filters.frames = filters.frames.wrapping_add(1);
    // Bases no view holds any more are dropped; the views keep theirs alive.
    filters.bases.retain(|base| Arc::strong_count(base) > 1);
    let mut domains: HashMap<Domain, DomainFacts> = HashMap::new();
    let mut layers_by_view = HashMap::new();
    for (entity, view, light, visible, layers) in &views {
        let key = view.retained_view_entity;
        if !gpu.views.contains_key(&key) {
            continue;
        }
        layers_by_view.insert(key, layers.cloned().unwrap_or_default());
        let domain = light.map_or(Domain::Camera(entity), |light| shadow_domain(light, key));
        let visible = match domain {
            Domain::Camera(_) => visible,
            Domain::Shadow(light, key) => shadows
                .get(light)
                .ok()
                .and_then(|shadow| shadow.subviews.get(&key)),
        };
        let facts = domains.entry(domain).or_default();
        facts.consumers.push(key);
        // With owned statics no selection reads the visible set (see `selection_inputs`).
        if own_statics {
            continue;
        }
        if let Some(class) = visible.and_then(|visible| visible.get::<Mesh3d>()) {
            facts
                .visible
                .extend(class.iter_visible().map(|(_, main)| *main));
        }
    }
    let compute_ready = prepared_cluster_cull(&gpu, &frame, &pipelines, &cache).is_some();
    let same_certificate = match (&gpu.certificate, &sources.certificate) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    };
    let mut wanted = HashMap::new();
    // The views whose selection this frame rebuilt; only their masks can have changed.
    let mut rebuilt = HashSet::new();
    for (domain, facts) in &domains {
        perf_span!("cluster::select::domain");
        let shadow = matches!(domain, Domain::Shadow(..));
        let mut joint: Option<bevy::platform::collections::HashSet<MainEntity>> = None;
        for key in &facts.consumers {
            let world = sources
                .worlds
                .get(&key.main_entity)
                .or_else(|| sources.worlds.get(&key.auxiliary_entity))
                .copied()
                .flatten();
            // A batch needs a prepass draw only where the cluster path draws into the prepass at
            // all (`--no-cluster-prepass` leaves the camera's prepass to the stock meshes).
            let prepass = prepass_phases.contains_key(key)
                && settings.as_ref().is_none_or(|settings| settings.prepass);
            let retired_allowed = retired_allowed_for_view(
                &sources.retired_eligibility,
                &layers_by_view[key],
                shadow,
            );
            // With owned statics nothing in the selection depends on the visible set: owned stock
            // is hidden at spawn and stock terrain under cluster terrain is hidden by cell below.
            let mut selection_inputs = if own_statics {
                bevy::platform::collections::HashSet::default()
            } else {
                perf_span!("cluster::select::inputs_clone");
                facts.visible.clone()
            };
            selection_inputs.extend(retired_allowed.iter().copied());
            let readiness_span = ::bevy::log::info_span!("cluster::select::readiness").entered();
            let readiness = cached_batch_readiness(
                &mut gpu,
                *key,
                batch_materials.as_ref().map(|materials| materials.version),
                shadow,
                prepass,
                (&cache, &materials, &allocators),
            );
            drop(readiness_span);
            let stock_span = ::bevy::log::info_span!("cluster::select::stock_ids").entered();
            let retired_bound = sources.retired_materials.keys();
            let stock_ids: HashMap<_, _> = if own_statics {
                HashMap::new()
            } else {
                facts
                    .visible
                    .iter()
                    .chain(retired_bound)
                    .map(|main| {
                        (
                            *main,
                            match sources.retired_materials.get(main) {
                                Some(material) => *material,
                                None => stock_materials
                                    .instances
                                    .get(main)
                                    .map(|instance| instance.asset_id),
                            },
                        )
                    })
                    .collect()
            };
            drop(stock_span);
            let stamp = [
                sources.version,
                gpu.layout_epoch,
                gpu.generation,
                world.map_or(0, |world| u64::from(world) + 1),
                u64::from(compute_ready && same_certificate),
                native.as_ref().map_or(0, |policy| policy.revision),
                runtime_state.as_ref().map_or(0, |state| state.revision),
            ];
            // The visible set changes nearly every frame and the instantiated references change
            // while cells load; neither touches most of the selection. The base pass reruns only
            // when its own inputs change and is shared by views with the same inputs; per frame,
            // only the instantiated references' instances are decided against the visible set.
            let data = SelectionData {
                identity: gpu.identity,
                placed: &gpu.placed,
                batch_count: gpu.batches.len(),
                certificate: gpu.certificate.as_deref(),
                terrain_blocks: &gpu.terrain_blocks,
                capacity: gpu.capacity_instances,
                meshes: scene
                    .as_deref()
                    .map_or(&[], |scene| scene.meshes.as_slice()),
            };
            let index = filters
                .indexes
                .entry(world)
                .or_insert_with(|| SelectionIndex::new(data.identity));
            if index.gpu != gpu.identity {
                *index = SelectionIndex::new(data.identity);
            }
            if index.indexed != data.placed.len() {
                index.append(data, world);
            }
            let inputs = BaseInputs {
                gpu: gpu.identity,
                world,
                drawable: compute_ready && same_certificate,
                native_revision: native.as_ref().map_or(0, |policy| policy.revision),
            };
            let filters_now = &mut *filters;
            let index = &filters_now.indexes[&world];
            let states_span = ::bevy::log::info_span!("cluster::select::states").entered();
            let states = SelectionBase::states(&sources, &readiness, index.batches.len());
            let current = gpu.views[key].selection_base.clone();
            let base_stale = current
                .as_ref()
                .is_none_or(|base| !base.matches(inputs, &states, index));
            drop(states_span);
            if base_stale {
                perf_span!("cluster::select::base_update");
                let base = match filters_now
                    .bases
                    .iter()
                    .find(|base| base.matches(inputs, &states, index))
                {
                    Some(base) => base.clone(),
                    None => {
                        let base = Arc::new(match current.as_deref() {
                            Some(previous) => {
                                previous.update(&data, index, native.as_deref(), inputs, states)
                            }
                            None => SelectionBase::build(
                                &data,
                                index,
                                native.as_deref(),
                                inputs,
                                states,
                            ),
                        });
                        filters_now.bases.push(base.clone());
                        base
                    }
                };
                gpu.views.get_mut(key).unwrap().selection_base = Some(base);
            }
            let view = &gpu.views[key];
            let matches_span = ::bevy::log::info_span!("cluster::select::matches").entered();
            let stale = view.selection.as_ref().is_none_or(|selection| {
                !selection.matches(stamp, &selection_inputs, &readiness, &stock_ids)
            });
            drop(matches_span);
            if stale {
                perf_span!("cluster::select::overlay");
                rebuilt.insert(*key);
                let base = view.selection_base.as_ref().unwrap();
                let index = &filters.indexes[&world];
                let mut mask = base.mask.clone();
                let mut hidden = bevy::platform::collections::HashSet::default();
                // With owned statics the stock copies are hidden at spawn: the base already is
                // what the view draws for them, and there is no stock binding to decide.
                let present: &BTreeSet<u32> = if own_statics {
                    &BTreeSet::new()
                } else {
                    &sources.present
                };
                for reference in present {
                    if runtime_state
                        .as_ref()
                        .is_some_and(|state| state.contains(*reference))
                    {
                        continue;
                    }
                    for &(batch, position) in
                        index.by_reference.get(reference).into_iter().flatten()
                    {
                        let object = &index.batches[batch as usize].objects[position as usize];
                        let Some((record, reference)) = object.object else {
                            continue;
                        };
                        if !base.mask.get(object.index) {
                            continue;
                        }
                        mask.set(object.index, false)
                            .expect("GPU instance fits allocated capacity");
                        let retired = sources.retired_refs.contains(&reference);
                        let Some(main) = sources.bound.get(&record).filter(|main| {
                            if retired {
                                retired_allowed.contains(*main)
                            } else {
                                facts.visible.contains(*main)
                            }
                        }) else {
                            continue;
                        };
                        let material = if retired {
                            sources.retired_materials.get(main).copied().flatten()
                        } else {
                            stock_materials
                                .instances
                                .get(main)
                                .map(|instance| instance.asset_id)
                        };
                        if material != gpu.batches[batch as usize].material {
                            continue;
                        }
                        if !retired {
                            hidden.insert(*main);
                        }
                        mask.set(object.index, true)
                            .expect("GPU instance fits allocated capacity");
                    }
                }
                // Revoke every occurrence of this reference, including unloaded sources,
                // before publishing ANY view's stock-hide decision. Shared meshes are untouched.
                for reference in runtime_state
                    .as_ref()
                    .into_iter()
                    .flat_map(|state| state.references())
                {
                    for &(batch, position) in
                        index.by_reference.get(&reference).into_iter().flatten()
                    {
                        let object = &index.batches[batch as usize].objects[position as usize];
                        mask.set(object.index, false)
                            .expect("GPU instance fits allocated capacity");
                    }
                }
                let terrain_blocks = base.terrain_blocks.clone();
                let (suppressed_contributors, suppressed_trees) =
                    (base.suppressed_contributors, base.suppressed_trees);
                // Only far stock is replaced. Near stock remains and clips the cluster
                // in this view once its own mesh/material/passes are drawable.
                // With owned statics cluster terrain owns the far terrain it covers: the terrain
                // LOD pass does not draw those stock patches (`update_terrain_lod_visibility`),
                // so no view filters them here.
                if !own_statics {
                    let covered: HashSet<IVec2> = terrain_blocks
                        .iter()
                        .flat_map(|index| gpu.terrain_blocks[*index as usize].cells.iter().copied())
                        .collect();
                    let hides = |identity: &crate::streaming::TerrainPrimitiveIdentity| {
                        identity.tier.is_some()
                            && Some(identity.worldspace_id) == world
                            && covered.contains(&identity.grid)
                    };
                    for main in &facts.visible {
                        if sources.terrain.get(main).is_some_and(hides) {
                            hidden.insert(*main);
                        }
                    }
                }
                gpu.views.get_mut(key).unwrap().selection = Some(Selection {
                    stamp,
                    visible: selection_inputs,
                    readiness,
                    stock_materials: stock_ids,
                    mask,
                    hidden,
                    terrain_blocks,
                    suppressed_contributors,
                    suppressed_trees,
                });
            }
            let selected = gpu.views[key].selection.as_ref().unwrap();
            if filters.frames.is_multiple_of(120)
                && (selected.suppressed_contributors + selected.suppressed_trees > 0)
            {
                info!(view=?key,contributor_data_missing=selected.suppressed_contributors,tree_relation_proof_missing=selected.suppressed_trees,
                    "native far LOD retained; certified far clusters suppressed");
            }
            let hidden = &selected.hidden;
            perf_span!("cluster::select::joint");
            joint = Some(intersect_hidden(joint, hidden));
        }
        let joint = joint.unwrap_or_default();
        // Several dependent views can share one point/spot visibility table.
        // Suppress its stock primitive only when EVERY consumer independently chose it.
        for key in &facts.consumers {
            // With owned statics no stock copy is bound (the stock binding is not tracked), so
            // a view's mask is its selection's mask, which changes only when it is rebuilt.
            if own_statics
                && !rebuilt.contains(key)
                && !gpu.views[key].instance_mask_bytes.is_empty()
            {
                continue;
            }
            perf_span!("cluster::select::mask_upload");
            let selection = gpu.views[key].selection.as_ref().unwrap();
            let mask = complementary_mask(
                selection,
                &joint,
                gpu.placed
                    .iter()
                    .enumerate()
                    .filter_map(|(index, instance)| {
                        if let ClusterInstanceIdentity::Object(record) = instance.identity {
                            if gpu
                                .certificate
                                .as_ref()
                                .and_then(|package| package.record(&record))
                                .is_some_and(|identity| {
                                    sources.retired_refs.contains(&identity.reference)
                                })
                            {
                                return None;
                            }
                            sources
                                .bound
                                .get(&record)
                                .copied()
                                .map(|main| (index as u32, main))
                        } else {
                            None
                        }
                    }),
            );
            let mut bytes = Vec::new();
            mask.append_le_bytes(&mut bytes);
            debug_assert_eq!(mask.capacity(), gpu.capacity_instances);
            debug_assert!(mask.enabled_count() <= gpu.total_instances);
            let offset = gpu.batches.len().max(1) as u64 * 16;
            let view = gpu.views.get_mut(key).unwrap();
            if bytes != view.instance_mask_bytes {
                #[cfg(feature = "tracy")]
                let _span =
                    bevy::log::info_span!("cluster::upload::instance_mask", bytes = bytes.len())
                        .entered();
                queue.write_buffer(&view.batch_table, offset, &bytes);
                view.instance_mask_bytes = bytes;
                let world = sources
                    .worlds
                    .get(&key.main_entity)
                    .or_else(|| sources.worlds.get(&key.auxiliary_entity))
                    .copied()
                    .flatten();
                debug!(
                    ?key,
                    ?domain,
                    ?world,
                    eligible_instances = mask.enabled_count(),
                    stock_hidden_joint = joint.len(),
                    selected_terrain_blocks = view
                        .selection
                        .as_ref()
                        .map_or(0, |selection| selection.terrain_blocks.len()),
                    "cluster eligibility mask changed"
                );
            }
        }
        wanted.insert(*domain, joint);
    }
    filters
        .saved
        .retain(|domain, _| domains.contains_key(domain));
    assert!(
        filters.active.is_empty(),
        "stock visibility was not restored after the previous frame"
    );
    let _apply_span = ::bevy::log::info_span!("cluster::select::apply").entered();
    for (domain, hidden) in wanted {
        let class = match domain {
            Domain::Camera(entity) => views
                .get_mut(entity)
                .ok()
                .and_then(|(_, _, _, visible, _)| visible)
                .map(|visible| visible.into_inner()),
            Domain::Shadow(light, key) => shadows
                .get_mut(light)
                .ok()
                .and_then(|shadow| shadow.into_inner().subviews.get_mut(&key)),
        }
        .and_then(|visible| visible.classes.get_mut(&std::any::TypeId::of::<Mesh3d>()));
        if let Some(class) = class {
            filters.apply_class(domain, class, &hidden);
        }
    }
}

fn restore_stock(
    mut filters: ResMut<Filters>,
    mut cameras: Query<&mut RenderVisibleEntities>,
    mut shadows: Query<&mut RenderShadowMapVisibleEntities>,
) {
    let active = filters.take_active();
    for domain in active {
        let class = match domain {
            Domain::Camera(entity) => cameras
                .get_mut(entity)
                .ok()
                .map(|visible| visible.into_inner()),
            Domain::Shadow(light, key) => shadows
                .get_mut(light)
                .ok()
                .and_then(|shadow| shadow.into_inner().subviews.get_mut(&key)),
        }
        .and_then(|visible| visible.classes.get_mut(&std::any::TypeId::of::<Mesh3d>()));
        filters.restore_class(domain, class);
    }
}

impl Selection {
    fn matches(
        &self,
        stamp: [u64; 7],
        visible: &bevy::platform::collections::HashSet<MainEntity>,
        readiness: &[bool],
        stock_materials: &HashMap<MainEntity, Option<bevy::asset::UntypedAssetId>>,
    ) -> bool {
        self.stamp == stamp
            && self.visible == *visible
            && self.readiness == readiness
            && self.stock_materials == *stock_materials
    }
}

fn intersect_hidden(
    previous: Option<bevy::platform::collections::HashSet<MainEntity>>,
    current: &bevy::platform::collections::HashSet<MainEntity>,
) -> bevy::platform::collections::HashSet<MainEntity> {
    match previous {
        None => current.clone(),
        Some(previous) => previous.intersection(current).copied().collect(),
    }
}

fn complementary_mask(
    selection: &Selection,
    joint: &bevy::platform::collections::HashSet<MainEntity>,
    bound_instances: impl IntoIterator<Item = (u32, MainEntity)>,
) -> super::super::instance_mask::InstanceMask {
    let mut mask = selection.mask.clone();
    if selection.hidden != *joint {
        for (index, main) in bound_instances {
            if selection.hidden.contains(&main) && !joint.contains(&main) {
                mask.set(index, false)
                    .expect("GPU instance fits allocated capacity");
            }
        }
    }
    mask
}

impl Filters {
    fn apply_class(
        &mut self,
        domain: Domain,
        class: &mut RenderVisibleEntitiesClass,
        hidden: &bevy::platform::collections::HashSet<MainEntity>,
    ) {
        self.saved.entry(domain).or_default().apply(class, hidden);
        assert!(self.active.insert(domain), "domain applied twice");
    }

    fn take_active(&mut self) -> HashSet<Domain> {
        std::mem::take(&mut self.active)
    }

    fn restore_class(&mut self, domain: Domain, class: Option<&mut RenderVisibleEntitiesClass>) {
        if let Some(class) = class {
            self.saved
                .get_mut(&domain)
                .expect("applied filter exists")
                .restore(class);
        } else {
            self.saved.remove(&domain);
        }
    }
}

#[cfg(test)]
mod reference_root_memo_tests {
    use super::*;

    fn entity(id: u32) -> Entity {
        Entity::from_raw_u32(id).unwrap()
    }

    fn resolve(
        memo: &mut ReferenceRootMemo,
        start: Entity,
        roots: &HashSet<Entity>,
        parents: &HashMap<Entity, Entity>,
    ) -> Option<Entity> {
        memo.resolve(
            start,
            |at| roots.contains(&at),
            |at| parents.get(&at).copied(),
        )
    }

    // Independent uncached baseline: preserve the old stop-at-first-query-root walk.
    fn uncached(
        start: Entity,
        roots: &HashSet<Entity>,
        parents: &HashMap<Entity, Entity>,
    ) -> Option<Entity> {
        let mut at = start;
        let mut seen = HashSet::new();
        while seen.insert(at) {
            if roots.contains(&at) {
                return Some(at);
            }
            at = *parents.get(&at)?;
        }
        None
    }

    #[test]
    fn nearest_root_barriers_missing_parents_and_cycles_match_uncached_walks() {
        let outer = entity(1);
        let unrelated = entity(2);
        let child = entity(3);
        let missing_tail = entity(4);
        let missing = entity(5);
        let cycle_a = entity(6);
        let cycle_b = entity(7);
        let cycle_tail = entity(8);
        let self_cycle = entity(9);
        let root_in_cycle = entity(10);
        let roots = HashSet::from([outer, unrelated, root_in_cycle]);
        let parents = HashMap::from([
            (unrelated, outer),
            (child, unrelated),
            (missing_tail, missing),
            (cycle_a, cycle_b),
            (cycle_b, cycle_a),
            (cycle_tail, cycle_a),
            (self_cycle, self_cycle),
            (root_in_cycle, root_in_cycle),
        ]);
        for order in [
            vec![
                child,
                outer,
                unrelated,
                missing_tail,
                cycle_tail,
                self_cycle,
                root_in_cycle,
                cycle_b,
                missing,
            ],
            vec![
                missing,
                cycle_b,
                root_in_cycle,
                self_cycle,
                cycle_tail,
                missing_tail,
                unrelated,
                outer,
                child,
            ],
        ] {
            let mut memo = ReferenceRootMemo::default();
            for start in order {
                assert_eq!(
                    resolve(&mut memo, start, &roots, &parents),
                    uncached(start, &roots, &parents)
                );
            }
            assert_eq!(resolve(&mut memo, child, &roots, &parents), Some(unrelated));
            assert_eq!(resolve(&mut memo, outer, &roots, &parents), Some(outer));
            assert_eq!(
                resolve(&mut memo, root_in_cycle, &roots, &parents),
                Some(root_in_cycle)
            );
            assert_eq!(resolve(&mut memo, cycle_a, &roots, &parents), None);
        }
    }

    #[test]
    fn repeated_meshes_and_shared_positive_or_negative_tails_reuse_resolved_ancestry() {
        let root = entity(1);
        let trunk = entity(2);
        let first = entity(3);
        let sibling = entity(4);
        let orphan = entity(5);
        let orphan_sibling = entity(6);
        let missing = entity(7);
        let roots = HashSet::from([root]);
        let parents = HashMap::from([
            (trunk, root),
            (first, trunk),
            (sibling, trunk),
            (orphan, missing),
            (orphan_sibling, missing),
        ]);
        let mut root_queries = Vec::new();
        let mut parent_queries = Vec::new();
        let mut memo = ReferenceRootMemo::default();
        let mut lookup = |memo: &mut ReferenceRootMemo, start| {
            memo.resolve(
                start,
                |at| {
                    root_queries.push(at);
                    roots.contains(&at)
                },
                |at| {
                    parent_queries.push(at);
                    parents.get(&at).copied()
                },
            )
        };
        // Presence discovers the chain; the primitive pass needs no new root/parent query.
        assert_eq!(lookup(&mut memo, first), Some(root));
        assert_eq!(lookup(&mut memo, first), Some(root));
        assert_eq!(lookup(&mut memo, sibling), Some(root));
        assert_eq!(lookup(&mut memo, orphan), None);
        assert_eq!(lookup(&mut memo, orphan_sibling), None);
        assert_eq!(lookup(&mut memo, orphan), None);
        assert_eq!(
            root_queries,
            vec![first, trunk, root, sibling, orphan, missing, orphan_sibling]
        );
        assert_eq!(
            parent_queries,
            vec![first, trunk, sibling, orphan, missing, orphan_sibling]
        );
    }

    #[test]
    fn a_deep_walk_drains_markers_before_many_short_fresh_walks() {
        const DEPTH: u32 = 1024;
        const SHORT_WALKS: u32 = 128;
        let root = entity(1);
        let roots = HashSet::from([root]);
        let mut parents: HashMap<_, _> = (2..=DEPTH + 1)
            .map(|id| (entity(id), entity(id - 1)))
            .collect();
        for id in DEPTH + 2..DEPTH + 2 + SHORT_WALKS {
            parents.insert(entity(id), root);
        }
        let mut root_queries = 0usize;
        let mut parent_queries = 0usize;
        let mut lookup = |memo: &mut ReferenceRootMemo, start| {
            memo.resolve(
                start,
                |at| {
                    root_queries += 1;
                    roots.contains(&at)
                },
                |at| {
                    parent_queries += 1;
                    parents.get(&at).copied()
                },
            )
        };
        let mut memo = ReferenceRootMemo::default();
        assert_eq!(lookup(&mut memo, entity(DEPTH + 1)), Some(root));
        assert!(memo.visiting.is_empty());
        for id in DEPTH + 2..DEPTH + 2 + SHORT_WALKS {
            assert_eq!(lookup(&mut memo, entity(id)), Some(root));
            assert!(memo.visiting.is_empty());
        }
        assert_eq!(root_queries, (DEPTH + 1 + SHORT_WALKS) as usize);
        assert_eq!(parent_queries, (DEPTH + SHORT_WALKS) as usize);
    }

    #[test]
    fn fresh_extractions_observe_reparenting_root_removal_and_missing_parents() {
        let outer = entity(1);
        let inner = entity(2);
        let child = entity(3);
        let mut roots = HashSet::from([outer, inner]);
        let mut parents = HashMap::from([(child, inner), (inner, outer)]);
        let lookup = |roots: &HashSet<Entity>, parents: &HashMap<Entity, Entity>| {
            resolve(&mut ReferenceRootMemo::default(), child, roots, parents)
        };
        assert_eq!(lookup(&roots, &parents), Some(inner));
        parents.insert(child, outer);
        assert_eq!(lookup(&roots, &parents), Some(outer));
        parents.insert(child, inner);
        assert_eq!(lookup(&roots, &parents), Some(inner));
        roots.remove(&inner);
        assert_eq!(lookup(&roots, &parents), Some(outer));
        parents.remove(&inner);
        assert_eq!(lookup(&roots, &parents), None);
    }

    #[test]
    fn cached_entities_keep_their_full_generational_identity() {
        // Index reuse is an allocator policy, not a prerequisite for this cache invariant.
        let original = entity(1);
        let replacement = Entity::from_index_and_generation(
            original.index(),
            original.generation().after_versions(1),
        );
        assert_eq!(original.index(), replacement.index());
        assert_ne!(original.generation(), replacement.generation());
        let roots = HashSet::from([original]);
        let parents = HashMap::new();
        let mut memo = ReferenceRootMemo::default();
        assert_eq!(
            resolve(&mut memo, original, &roots, &parents),
            Some(original)
        );
        assert_eq!(resolve(&mut memo, replacement, &roots, &parents), None);
    }
}

#[cfg(test)]
mod selection_regressions {
    use super::*;
    use bevy::platform::collections::HashSet as VisibleSet;

    fn main(id: u32) -> MainEntity {
        Entity::from_raw_u32(id).unwrap().into()
    }

    fn visible(ids: &[u32]) -> VisibleSet<MainEntity> {
        ids.iter().map(|id| main(*id)).collect()
    }

    #[test]
    fn retired_view_domain_obeys_layers_caster_and_exact_selection_input() {
        let eligible = main(41);
        let other_layer = main(42);
        let non_caster = main(43);
        let sources = HashMap::from([
            (
                eligible,
                RetiredEligibility {
                    layers: RenderLayers::layer(2),
                    casts_shadows: true,
                },
            ),
            (
                other_layer,
                RetiredEligibility {
                    layers: RenderLayers::layer(3),
                    casts_shadows: true,
                },
            ),
            (
                non_caster,
                RetiredEligibility {
                    layers: RenderLayers::layer(2),
                    casts_shadows: false,
                },
            ),
        ]);
        let camera = retired_allowed_for_view(&sources, &RenderLayers::layer(2), false);
        assert_eq!(camera, visible(&[41, 43]));
        let shadow = retired_allowed_for_view(&sources, &RenderLayers::layer(2), true);
        assert_eq!(shadow, visible(&[41]));
        assert_eq!(
            retired_allowed_for_view(&sources, &RenderLayers::layer(3), true),
            visible(&[42])
        );
        assert!(retired_allowed_for_view(&sources, &RenderLayers::default(), true).is_empty());
        // Same count, different exact member is a different consumer input.
        let mut cached = selection(&[], 1);
        cached.visible = shadow.clone();
        assert!(cached.matches(
            cached.stamp,
            &shadow,
            &cached.readiness,
            &cached.stock_materials
        ));
        assert!(!cached.matches(
            cached.stamp,
            &visible(&[42]),
            &cached.readiness,
            &cached.stock_materials
        ));
    }

    fn selection(ids: &[u32], capacity: u32) -> Selection {
        Selection {
            stamp: [7, 11, 13, 17, 1, 0, 0],
            visible: visible(ids),
            readiness: vec![true],
            stock_materials: ids.iter().map(|id| (main(*id), None)).collect(),
            mask: super::super::super::instance_mask::InstanceMask::new(capacity),
            hidden: VisibleSet::default(),
            terrain_blocks: HashSet::new(),
            suppressed_contributors: 0,
            suppressed_trees: 0,
        }
    }

    #[test]
    fn same_entity_with_replaced_material_invalidates_selection() {
        let mut assets = Assets::<StandardMaterial>::default();
        let first = assets.add(StandardMaterial::default());
        let second = assets.add(StandardMaterial::default());
        let mut cached = selection(&[1], 1);
        cached
            .stock_materials
            .insert(main(1), Some(first.id().untyped()));
        assert!(cached.matches(
            cached.stamp,
            &cached.visible,
            &[true],
            &cached.stock_materials
        ));
        let mut current = cached.stock_materials.clone();
        current.insert(main(1), Some(second.id().untyped()));
        assert!(!cached.matches(cached.stamp, &cached.visible, &[true], &current));
        current.insert(main(1), None);
        assert!(!cached.matches(cached.stamp, &cached.visible, &[true], &current));
    }

    #[test]
    fn different_equal_sum_visibility_sets_do_not_reuse_selection() {
        let cached = selection(&[1, 4], 2);
        let replacement = visible(&[2, 3]);
        // Establish the actual old-key collision without relying on numeric Entity ordering.
        let old_fold = |set: &VisibleSet<MainEntity>| {
            set.iter().fold(0u64, |hash, entity| {
                hash.wrapping_add(entity.id().to_bits().wrapping_mul(0x9e3779b97f4a7c15))
            })
        };
        assert_eq!(old_fold(&cached.visible), old_fold(&replacement));
        // Hold every other cache input fixed to prove exact visibility is responsible.
        assert!(!cached.matches(cached.stamp, &replacement, &[true], &cached.stock_materials));
        assert!(!cached.matches(
            cached.stamp,
            &cached.visible,
            &[false],
            &cached.stock_materials
        ));
    }

    #[test]
    fn a_shared_shadow_class_keeps_stock_until_every_consumer_replaces_it() {
        let mut first = selection(&[1, 2], 3);
        first.hidden = visible(&[1, 2]);
        // Index 2 represents a distant instance with no stock entity in this domain.
        for index in 0..3 {
            first.mask.set(index, true).unwrap();
        }
        let mut second = selection(&[1, 2], 3);
        second.hidden = visible(&[2]);
        second.mask.set(1, true).unwrap();
        second.mask.set(2, true).unwrap();
        let joint = intersect_hidden(Some(first.hidden.clone()), &second.hidden);
        assert_eq!(joint, visible(&[2]));
        let mut class = RenderVisibleEntitiesClass {
            entities_cpu_culling: vec![
                (Entity::from_raw_u32(101).unwrap(), main(1)),
                (Entity::from_raw_u32(102).unwrap(), main(2)),
            ],
            ..default()
        };
        class.entities_cpu_culling.sort_by_key(|(_, main)| *main);
        let original = class.entities_cpu_culling.clone();
        let mut filter = ClassFilter::default();
        filter.apply(&mut class, &joint);
        assert_eq!(
            class
                .iter_visible()
                .map(|(_, main)| *main)
                .collect::<VisibleSet<_>>(),
            visible(&[1])
        );
        for selection in [&first, &second] {
            let mask = complementary_mask(selection, &joint, [(0, main(1)), (1, main(2))]);
            let mut bytes = Vec::new();
            mask.append_le_bytes(&mut bytes);
            assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 0b110);
        }
        filter.restore(&mut class);
        assert_eq!(class.entities_cpu_culling, original);
        // A missing readiness report is an empty consumer, not vacuous agreement.
        assert!(intersect_hidden(Some(joint), &VisibleSet::default()).is_empty());
    }

    #[test]
    fn a_domain_without_apply_is_not_restored() {
        let domain = Domain::Camera(Entity::from_raw_u32(10).unwrap());
        let mut filters = Filters::default();
        filters.saved.insert(domain, ClassFilter::default());
        let mut class = RenderVisibleEntitiesClass::default();
        for domain in filters.take_active() {
            filters.restore_class(domain, Some(&mut class));
        }
        // Inactive saved state remains usable; an accidental restore would already panic.
        filters.apply_class(domain, &mut class, &VisibleSet::default());
        for domain in filters.take_active() {
            filters.restore_class(domain, Some(&mut class));
        }
        assert!(filters.active.is_empty());
    }

    #[test]
    fn disappearing_applied_class_does_not_leak_into_recreated_domain() {
        let domain = Domain::Camera(Entity::from_raw_u32(10).unwrap());
        let mut filters = Filters::default();
        let mut old = RenderVisibleEntitiesClass::default();
        old.entities_cpu_culling
            .push((Entity::from_raw_u32(101).unwrap(), main(1)));
        filters.apply_class(domain, &mut old, &visible(&[1]));
        assert!(old.iter_visible().next().is_none());
        for domain in filters.take_active() {
            filters.restore_class(domain, None);
        }
        assert!(!filters.saved.contains_key(&domain));
        let mut fresh = RenderVisibleEntitiesClass::default();
        fresh
            .entities_cpu_culling
            .push((Entity::from_raw_u32(102).unwrap(), main(2)));
        let original = fresh.entities_cpu_culling.clone();
        filters.apply_class(domain, &mut fresh, &VisibleSet::default());
        for domain in filters.take_active() {
            filters.restore_class(domain, Some(&mut fresh));
        }
        assert_eq!(fresh.entities_cpu_culling, original);
        assert!(fresh.added_entities.is_empty());
        assert!(fresh.removed_entities.is_empty());
    }
}

// Append to cluster/handoff.rs. CPU source-proof regressions, no renderer or fake certificate.
#[cfg(test)]
mod source_proof_tests {
    use super::*;
    use bevy::{
        animation::AnimationTargetId,
        ecs::system::SystemState,
        gltf::GltfExtras,
        mesh::{
            morph::{MeshMorphWeights, MorphWeights},
            skinning::SkinnedMesh,
        },
    };

    struct SourceFixture {
        asset: WorldAsset,
        root: Entity,
        parent: Entity,
        primitive: Entity,
        model_matrix: Mat4,
    }

    fn fixture() -> SourceFixture {
        let mut world = World::new();
        let root_pose =
            Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
        let parent_pose = Transform::from_xyz(2.0, 0.0, 0.0);
        let local_pose = Transform::from_xyz(0.0, 3.0, 0.0);
        let model_matrix =
            root_pose.to_matrix() * (parent_pose.to_matrix() * local_pose.to_matrix());
        let root = world.spawn((root_pose, SourceNodePose(root_pose))).id();
        let parent = world
            .spawn((parent_pose, SourceNodePose(parent_pose), ChildOf(root)))
            .id();
        let handle = Handle::<Mesh>::Uuid(
            bevy::asset::uuid::Uuid::from_u128(123),
            std::marker::PhantomData,
        );
        let primitive = world
            .spawn((
                Mesh3d(handle.clone()),
                local_pose,
                SourceNodePose(local_pose),
                ChildOf(parent),
                SourcePrimitive {
                    scene_index: 0,
                    ordinal: 0,
                    mesh_index: 0,
                    primitive_index: 0,
                    matches_default_scene: true,
                },
                SourcePrimitiveMesh(handle),
                SourcePrimitiveTransform(model_matrix),
                SourceStaticModel(true),
            ))
            .id();
        SourceFixture {
            asset: WorldAsset::new(world),
            root,
            parent,
            primitive,
            model_matrix,
        }
    }

    fn evaluate_model(world: &mut World, entity: Entity, root: Entity) -> Option<Mat4> {
        let mut state: SystemState<Query<NodeState>> = SystemState::new(world);
        let nodes = state.get(world).expect("source query is available");
        model_matrix(entity, root, &nodes)
    }

    fn add_wrapper(fixture: &mut SourceFixture) -> Entity {
        let wrapper = fixture
            .asset
            .world
            .spawn((
                WorldAssetRoot(Handle::default()),
                Transform::from_xyz(100.0, 200.0, 300.0),
            ))
            .id();
        fixture
            .asset
            .world
            .entity_mut(fixture.root)
            .insert(ChildOf(wrapper));
        wrapper
    }

    #[test]
    fn an_unchanged_template_and_runtime_chain_keep_the_exact_source_matrix() {
        let mut input = fixture();
        assert!(template_valid(&input.asset, &HashSet::new()));
        let wrapper = add_wrapper(&mut input);
        assert_eq!(
            evaluate_model(&mut input.asset.world, input.primitive, wrapper),
            Some(input.model_matrix)
        );
        input
            .asset
            .world
            .entity_mut(wrapper)
            .insert(Transform::from_xyz(-900.0, 25.0, 4.0));
        assert_eq!(
            evaluate_model(&mut input.asset.world, input.primitive, wrapper),
            Some(input.model_matrix)
        );
    }

    #[test]
    fn source_billboard_extras_are_rejected_before_a_billboard_component_exists() {
        let mut input = fixture();
        input
            .asset
            .world
            .entity_mut(input.parent)
            .insert(GltfExtras {
                value: r#"{"openSkyrim":{"billboard":"rotateAboutUp"}}"#.into(),
            });
        assert!(
            input
                .asset
                .world
                .get::<crate::billboard::Billboard>(input.parent)
                .is_none()
        );
        assert!(!template_valid(&input.asset, &HashSet::new()));
        let wrapper = add_wrapper(&mut input);
        assert_eq!(
            evaluate_model(&mut input.asset.world, input.primitive, wrapper),
            None
        );
    }

    #[test]
    fn unrelated_extras_do_not_invalidate_an_unchanged_static_template() {
        let mut input = fixture();
        input
            .asset
            .world
            .entity_mut(input.parent)
            .insert(GltfExtras {
                value: r#"{"unrelated":"billboard","openSkyrim":{"other":true}}"#.into(),
            });
        assert!(template_valid(&input.asset, &HashSet::new()));
        let wrapper = add_wrapper(&mut input);
        assert_eq!(
            evaluate_model(&mut input.asset.world, input.primitive, wrapper),
            Some(input.model_matrix)
        );
    }

    #[test]
    fn deformation_revokes_proof_but_rigid_animation_targets_keep_only_their_rest_pose() {
        for variant in 0..5 {
            let mut input = fixture();
            {
                let mut primitive = input.asset.world.entity_mut(input.primitive);
                match variant {
                    0 => {
                        primitive.insert(crate::billboard::Billboard {
                            mode: crate::billboard::BillboardMode::FaceCamera,
                            authored: Quat::IDENTITY,
                        });
                    }
                    1 => {
                        primitive.insert(SkinnedMesh::default());
                    }
                    2 => {
                        primitive.insert(MorphWeights::new(vec![0.5], None).unwrap());
                    }
                    3 => {
                        primitive.insert(AnimationTargetId::from_name(&Name::new("dynamic")));
                    }
                    // Direct per-mesh weights need no MorphWeights ancestor.
                    4 => {
                        primitive.insert(MeshMorphWeights::Value { weights: vec![0.5] });
                    }
                    _ => unreachable!(),
                }
            }
            let rigid_rest_pose = variant == 3;
            assert_eq!(
                template_valid(&input.asset, &HashSet::new()),
                rigid_rest_pose,
                "variant {variant}"
            );
            let wrapper = add_wrapper(&mut input);
            assert_eq!(
                evaluate_model(&mut input.asset.world, input.primitive, wrapper),
                rigid_rest_pose.then_some(input.model_matrix),
                "variant {variant}"
            );
            if rigid_rest_pose {
                input
                    .asset
                    .world
                    .get_mut::<Transform>(input.primitive)
                    .unwrap()
                    .translation
                    .x += 1.0;
                assert!(!template_valid(&input.asset, &HashSet::new()));
                assert_eq!(
                    evaluate_model(&mut input.asset.world, input.primitive, wrapper),
                    None
                );
            }
        }
    }

    #[test]
    fn missing_or_changed_ancestor_poses_revoke_both_proofs() {
        for variant in 0..3 {
            let mut input = fixture();
            match variant {
                0 => {
                    input
                        .asset
                        .world
                        .entity_mut(input.parent)
                        .remove::<SourceNodePose>();
                }
                1 => {
                    input
                        .asset
                        .world
                        .entity_mut(input.parent)
                        .insert(Transform::from_xyz(8.0, 0.0, 0.0));
                }
                2 => {
                    input
                        .asset
                        .world
                        .entity_mut(input.parent)
                        .remove::<Transform>();
                }
                _ => unreachable!(),
            }
            assert!(!template_valid(&input.asset, &HashSet::new()));
            let wrapper = add_wrapper(&mut input);
            assert_eq!(
                evaluate_model(&mut input.asset.world, input.primitive, wrapper),
                None
            );
        }
    }

    #[test]
    fn reparenting_unchanged_authored_nodes_cannot_inherit_the_old_model_matrix() {
        let mut input = fixture();
        let pose = Transform::from_xyz(9.0, 0.0, 0.0);
        let new_parent = input
            .asset
            .world
            .spawn((pose, SourceNodePose(pose), ChildOf(input.root)))
            .id();
        input
            .asset
            .world
            .entity_mut(input.primitive)
            .insert(ChildOf(new_parent));
        for entity in [input.root, input.parent, new_parent, input.primitive] {
            assert_eq!(
                input.asset.world.get::<SourceNodePose>(entity).unwrap().0,
                *input.asset.world.get::<Transform>(entity).unwrap()
            );
        }
        assert!(!template_valid(&input.asset, &HashSet::new()));
        let wrapper = add_wrapper(&mut input);
        let current = evaluate_model(&mut input.asset.world, input.primitive, wrapper).unwrap();
        assert_ne!(
            current,
            input
                .asset
                .world
                .get::<SourcePrimitiveTransform>(input.primitive)
                .unwrap()
                .0
        );
    }

    #[test]
    fn wrong_mesh_mutation_invalidity_and_incomplete_identity_are_not_valid_templates() {
        for variant in 0..5 {
            let mut input = fixture();
            let mut invalid = HashSet::new();
            match variant {
                0 => {
                    input
                        .asset
                        .world
                        .entity_mut(input.primitive)
                        .insert(Mesh3d(Handle::default()));
                }
                1 => {
                    invalid.insert(
                        input
                            .asset
                            .world
                            .get::<Mesh3d>(input.primitive)
                            .unwrap()
                            .0
                            .id(),
                    );
                }
                2 => {
                    input
                        .asset
                        .world
                        .entity_mut(input.primitive)
                        .remove::<SourcePrimitiveMesh>();
                }
                3 => {
                    input
                        .asset
                        .world
                        .entity_mut(input.primitive)
                        .insert(SourceStaticModel(false));
                }
                4 => {
                    input
                        .asset
                        .world
                        .entity_mut(input.primitive)
                        .get_mut::<SourcePrimitive>()
                        .unwrap()
                        .matches_default_scene = false;
                }
                _ => unreachable!(),
            }
            assert!(!template_valid(&input.asset, &invalid), "variant {variant}");
        }
        let mut input = fixture();
        input.asset.world.spawn(Mesh3d(Handle::default()));
        assert!(
            !template_valid(&input.asset, &HashSet::new()),
            "an extra untagged primitive cannot be ignored"
        );
    }

    #[test]
    fn cell_local_reference_pose_is_exact_and_wrapper_motion_is_separate_from_model_space() {
        let pose = super::super::super::reader::ReferencePose {
            position: [8320.0, -3840.0, 512.0],
            rotation: [0.0; 3],
            scale: 1.5,
        };
        let key = CellKey::Exterior {
            worldspace_id: 70,
            grid_x: 2,
            grid_y: -1,
        };
        let expected = expected_reference_pose(&pose, key).unwrap();
        assert_eq!(expected.translation, Vec3::new(128.0, 512.0, -256.0));
        assert_eq!(expected.rotation, Quat::IDENTITY);
        assert_eq!(expected.scale, Vec3::splat(1.5));
        assert!(expected_reference_pose(&pose, CellKey::Interior(12)).is_none());
        // A root moved by one unit no longer equals the authoritative stock pose,
        // even though its model-local descendants continue to match their baselines.
        let mut input = fixture();
        let wrapper = add_wrapper(&mut input);
        input.asset.world.entity_mut(wrapper).insert(expected);
        assert_eq!(
            evaluate_model(&mut input.asset.world, input.primitive, wrapper),
            Some(input.model_matrix)
        );
        input
            .asset
            .world
            .entity_mut(wrapper)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x += 1.0;
        assert_ne!(
            *input.asset.world.get::<Transform>(wrapper).unwrap(),
            expected
        );
        assert_eq!(
            evaluate_model(&mut input.asset.world, input.primitive, wrapper),
            Some(input.model_matrix)
        );
    }
}

// Real producer certificate + Bevy GLTF loader + world spawner + Extract schedule.
#[cfg(test)]
mod extract_source_tests {
    use super::*;
    use crate::cluster::{
        reader::producer_fixture_tests::published_for_loader,
        source_identity::actual_loader_tests::{loader_app, wait_for_loaded},
    };
    use bevy::{
        gltf::Gltf,
        render::{ExtractSchedule, MainWorld},
        world_serialization::{WorldInstance, WorldInstanceSpawner},
    };
    use std::time::{Duration, Instant};

    const REFERENCE: u32 = 0x100;
    const MODEL: &str = "meshes/arch/rock.glb";

    struct Fixture {
        // Drop asset worlds/certificate guards before removing the directory.
        app: App,
        render: World,
        schedule: Schedule,
        _directory: tempfile::TempDir,
        root: Entity,
        cell: Entity,
        key: RecordKey,
        root_pose: Transform,
    }

    impl Fixture {
        fn pending() -> Self {
            Self::pending_with_disablers(|_| {})
        }

        fn pending_with_disablers(register: impl FnOnce(&mut World)) -> Self {
            let directory = published_for_loader();
            let certificate = Arc::new(
                crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
            );
            let key = *certificate
                .records()
                .find(|(_, identity)| identity.reference == REFERENCE)
                .unwrap()
                .0;
            assert_eq!((key.worldspace, key.tier, key.anchor), (0x70, 16, [2, 2]));
            assert_eq!(certificate.record(&key).unwrap().primitive, 0);
            let mut app = loader_app(directory.path());
            app.init_asset::<StandardMaterial>()
                .insert_resource(ValidatedPackage(certificate))
                .init_resource::<ClusterBatchHandles>();
            let handle: Handle<Gltf> = app.world().resource::<AssetServer>().load(MODEL);
            wait_for_loaded(&mut app, &handle);
            // Loader/spawner systems initialize during loading. Register custom
            // filters before tracking/extraction, without changing the loader's
            // unrelated already-constructed query domains.
            register(app.world_mut());
            crate::streaming::identity_changes::install(app.world_mut());
            let scene = app
                .world()
                .resource::<Assets<Gltf>>()
                .get(&handle)
                .unwrap()
                .scenes[0]
                .clone();
            let cell = app
                .world_mut()
                .spawn((
                    StreamedCellKey(CellKey::Exterior {
                        worldspace_id: 0x70,
                        grid_x: 0,
                        grid_y: 0,
                    }),
                    Transform::IDENTITY,
                ))
                .id();
            // Independent literal placement from the real producer fixture's core row.
            let root_pose = Transform::from_xyz(100.0, 10.0, -100.0);
            let root = app
                .world_mut()
                .spawn((
                    FormId(REFERENCE),
                    CellRef(0x30),
                    MeshHandle(MODEL.into()),
                    WorldAssetRoot(scene),
                    root_pose,
                    ChildOf(cell),
                    InheritedVisibility::VISIBLE,
                ))
                .id();
            let mut render = World::new();
            render.init_resource::<Sources>();
            let mut schedule = Schedule::new(ExtractSchedule);
            schedule.add_systems(extract_sources);
            Self {
                app,
                render,
                schedule,
                _directory: directory,
                root,
                cell,
                key,
                root_pose,
            }
        }

        fn extract(&mut self) {
            // MainWorld's tuple field is private. Its public Default + DerefMut
            // lets us borrow the real App world exactly for this extraction.
            let mut main = MainWorld::default();
            std::mem::swap(self.app.world_mut(), &mut *main);
            self.render.insert_resource(main);
            self.schedule.run(&mut self.render);
            let mut main = self.render.remove_resource::<MainWorld>().unwrap();
            std::mem::swap(self.app.world_mut(), &mut *main);
        }

        fn sources(&self) -> &Sources {
            self.render.resource::<Sources>()
        }

        fn spawn_stock(&mut self) -> Entity {
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                self.app.update();
                let world = self.app.world();
                let ready = world
                    .get::<WorldInstance>(self.root)
                    .is_some_and(|instance| {
                        world
                            .resource::<WorldInstanceSpawner>()
                            .instance_is_ready(**instance)
                    });
                if ready {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "real stock GLTF instance did not become ready"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            let world = self.app.world_mut();
            let mut primitives =
                world.query_filtered::<Entity, (With<SourcePrimitive>, With<Mesh3d>)>();
            let entities = primitives.iter(world).collect::<Vec<_>>();
            assert_eq!(entities.len(), 1, "one real loader-tagged primitive");
            entities[0]
        }

        fn assert_blocked(&mut self) {
            self.extract();
            assert!(self.sources().present.contains(&REFERENCE));
            assert!(!self.sources().bound.contains_key(&self.key));
        }

        fn assert_bound(&mut self, primitive: Entity) {
            self.extract();
            assert!(self.sources().present.contains(&REFERENCE));
            assert_eq!(
                self.sources().bound.get(&self.key),
                Some(&MainEntity::from(primitive))
            );
        }
    }

    /// Force the independent full ECS loops, preserving certificate/history/sticky
    /// asset invalidations. Replacing the certificate Arc would erase history and
    /// is therefore not a valid oracle for removed-identity transitions.
    fn assert_full_oracle(input: &mut Fixture) {
        let snapshot = |s: &Sources| {
            (
                s.present.clone(),
                s.bound.clone(),
                s.stock_primitives.clone(),
                s.retired_refs.clone(),
                s.retired_present.clone(),
                s.retired_materials.clone(),
                s.retired_eligibility.clone(),
                s.templates.clone(),
                s.normal_maps.clone(),
                s.source_fallback_batches.clone(),
                s.seen_roots.clone(),
                s.stock_tangents.clone(),
            )
        };
        let expected = snapshot(input.sources());
        let version = input.sources().candidate_version;
        input.render.resource_mut::<Sources>().force_full = true;
        input.extract();
        assert!(input.sources().last_work.qualification_full);
        assert_eq!(snapshot(input.sources()), expected);
        assert_eq!(
            input.sources().candidate_version,
            version,
            "a full oracle must not publish a correction to the preceding delta"
        );
    }

    fn assert_delta_only(input: &Fixture) {
        let work = input.sources().last_work;
        assert!(!work.qualification_full);
        assert_eq!((work.discovery_roots, work.discovery_meshes), (0, 0));
    }

    fn duplicate_primitive(input: &mut Fixture, primitive: Entity) -> Entity {
        let world = input.app.world();
        let mesh = world.get::<Mesh3d>(primitive).unwrap().clone();
        let source = world.get::<SourcePrimitiveMesh>(primitive).unwrap().clone();
        let tag = *world.get::<SourcePrimitive>(primitive).unwrap();
        let matrix = *world.get::<SourcePrimitiveTransform>(primitive).unwrap();
        let static_model = *world.get::<SourceStaticModel>(primitive).unwrap();
        let baseline = *world.get::<SourceNodePose>(primitive).unwrap();
        let pose = *world.get::<Transform>(primitive).unwrap();
        let parent = world.get::<ChildOf>(primitive).unwrap().parent();
        input
            .app
            .world_mut()
            .spawn((
                mesh,
                source,
                tag,
                matrix,
                static_model,
                baseline,
                pose,
                ChildOf(parent),
            ))
            .id()
    }

    #[test]
    fn one_root_edit_never_visits_unrelated_stock_in_either_mode() {
        for own_statics in [false, true] {
            let mut input = Fixture::pending();
            let primitive = input.spawn_stock();
            input.render.insert_resource(ClusterSettings {
                own_statics,
                ..default()
            });
            let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().clone();
            for ordinal in 0..256 {
                let root = input
                    .app
                    .world_mut()
                    .spawn((
                        FormId(u32::MAX - ordinal),
                        Transform::IDENTITY,
                        ChildOf(input.cell),
                    ))
                    .id();
                input
                    .app
                    .world_mut()
                    .spawn((mesh.clone(), Transform::IDENTITY, ChildOf(root)));
            }
            input.assert_bound(primitive);
            input
                .app
                .world_mut()
                .get_mut::<Transform>(input.root)
                .unwrap()
                .translation
                .x += 1.0;
            input.assert_blocked();
            assert_delta_only(&input);
            let work = input.sources().last_work;
            assert_eq!(
                (
                    work.roots_scanned,
                    work.meshes_scanned,
                    work.primitives_scanned
                ),
                (1, 1, 1)
            );
            assert_eq!(work.affected_references, 1);
            assert_full_oracle(&mut input);
            input
                .app
                .world_mut()
                .entity_mut(input.root)
                .insert(input.root_pose);
            input.assert_bound(primitive);
            assert_delta_only(&input);
            assert_full_oracle(&mut input);
        }
    }

    #[test]
    fn duplicate_root_and_ordinal_removal_promote_only_the_survivor() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let duplicate = input
            .app
            .world_mut()
            .spawn((
                FormId(REFERENCE),
                CellRef(0x30),
                MeshHandle(MODEL.into()),
                input.root_pose,
                ChildOf(input.cell),
                InheritedVisibility::VISIBLE,
            ))
            .id();
        input.assert_blocked();
        assert_delta_only(&input);
        assert_eq!(input.sources().last_work.roots_scanned, 2);
        assert_full_oracle(&mut input);
        input.app.world_mut().despawn(duplicate);
        input.assert_bound(primitive);
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        let duplicate = duplicate_primitive(&mut input, primitive);
        input.assert_blocked();
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input.app.world_mut().despawn(duplicate);
        input.assert_bound(primitive);
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
    }

    #[test]
    fn ancestry_identity_and_component_removals_match_the_full_oracle() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let parent = input
            .app
            .world()
            .get::<ChildOf>(primitive)
            .unwrap()
            .parent();
        let wrapper = input
            .app
            .world_mut()
            .spawn((
                Transform::IDENTITY,
                SourceNodePose(Transform::IDENTITY),
                ChildOf(parent),
            ))
            .id();
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(ChildOf(wrapper));
        input.assert_bound(primitive);
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        // New root barrier on an already-indexed node; no Children discovery.
        input
            .app
            .world_mut()
            .entity_mut(wrapper)
            .insert(FormId(u32::MAX));
        input.assert_blocked();
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input.app.world_mut().entity_mut(wrapper).remove::<FormId>();
        input.assert_bound(primitive);
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input
            .app
            .world_mut()
            .entity_mut(wrapper)
            .remove::<ChildOf>();
        input.extract();
        assert!(!input.sources().bound.contains_key(&input.key));
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input
            .app
            .world_mut()
            .entity_mut(wrapper)
            .insert(ChildOf(parent));
        input.assert_bound(primitive);
        let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().clone();
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .remove::<Mesh3d>();
        input.assert_blocked();
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input.app.world_mut().entity_mut(primitive).insert(mesh);
        input.assert_bound(primitive);
        // Removed FormId must keep instantiated history, including on repeated calls.
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .remove::<FormId>();
        input.assert_blocked();
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input.assert_blocked();
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(FormId(REFERENCE));
        input.assert_bound(primitive);
        assert_full_oracle(&mut input);
    }

    #[test]
    fn changed_handle_rows_remove_inverse_edges_and_skip_unchanged_rows() {
        let mut input = Fixture::pending();
        input.spawn_stock();
        source_template(&mut input);
        let first = input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        input
            .app
            .world_mut()
            .resource_mut::<ClusterBatchHandles>()
            .set(1, first.clone());
        input.extract();
        input.extract();
        let second = input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        input
            .app
            .world_mut()
            .resource_mut::<ClusterBatchHandles>()
            .set(1, second.clone());
        input.extract();
        assert_eq!(input.sources().last_work.template_rows, 1);
        assert!(
            !input
                .sources()
                .template_rows
                .by_material
                .contains_key(&first.id().untyped())
        );
        assert!(input.sources().template_rows.by_material[&second.id().untyped()].contains(&1));
        assert_full_oracle(&mut input);
        input
            .app
            .world_mut()
            .resource_mut::<ClusterBatchHandles>()
            .set(1, second);
        input.extract();
        assert_eq!(input.sources().last_work.template_rows, 0);
        input
            .app
            .world_mut()
            .resource_mut::<ClusterBatchHandles>()
            .handles
            .truncate(1);
        input.extract();
        assert_eq!(input.sources().last_work.template_rows, 0);
        assert_eq!(input.sources().templates.len(), 1);
        assert_full_oracle(&mut input);
    }

    #[test]
    fn mixed_lifecycle_sequences_match_full_qualification_in_both_modes() {
        for own_statics in [false, true] {
            for seed in [1_u64, 17, 101, 4099] {
                let mut input = Fixture::pending();
                let primitive = input.spawn_stock();
                input.render.insert_resource(ClusterSettings {
                    own_statics,
                    ..default()
                });
                input.assert_bound(primitive);
                let baseline = *input.app.world().get::<SourceNodePose>(primitive).unwrap();
                let tag = *input.app.world().get::<SourcePrimitive>(primitive).unwrap();
                let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().clone();
                let mut state = seed;
                let mut duplicate = None;
                for _ in 0..128 {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    match (state >> 32) % 12 {
                        0 => {
                            input
                                .app
                                .world_mut()
                                .entity_mut(primitive)
                                .insert(baseline.0);
                        }
                        1 => {
                            let mut pose = baseline.0;
                            pose.translation.x += 1.0;
                            input.app.world_mut().entity_mut(primitive).insert(pose);
                        }
                        2 => {
                            input
                                .app
                                .world_mut()
                                .entity_mut(primitive)
                                .remove::<SourceNodePose>();
                        }
                        3 => {
                            input.app.world_mut().entity_mut(primitive).insert(baseline);
                        }
                        4 => {
                            input
                                .app
                                .world_mut()
                                .entity_mut(primitive)
                                .remove::<SourcePrimitive>();
                        }
                        5 => {
                            input.app.world_mut().entity_mut(primitive).insert(tag);
                        }
                        6 => {
                            input
                                .app
                                .world_mut()
                                .entity_mut(primitive)
                                .remove::<Mesh3d>();
                        }
                        7 => {
                            input
                                .app
                                .world_mut()
                                .entity_mut(primitive)
                                .insert(mesh.clone());
                        }
                        8 => {
                            input
                                .app
                                .world_mut()
                                .entity_mut(input.root)
                                .insert(FormId(u32::MAX));
                        }
                        9 => {
                            input
                                .app
                                .world_mut()
                                .entity_mut(input.root)
                                .insert(FormId(REFERENCE));
                        }
                        10 => {
                            input
                                .app
                                .world_mut()
                                .entity_mut(primitive)
                                .insert(Visibility::Hidden);
                        }
                        _ => {
                            input
                                .app
                                .world_mut()
                                .entity_mut(primitive)
                                .insert(Visibility::Inherited);
                        }
                    }
                    // Unrelated lifecycle interleaves with changed/refused stock facts.
                    if state & 3 == 0 {
                        if let Some(entity) = duplicate.take() {
                            input.app.world_mut().despawn(entity);
                        } else {
                            duplicate = Some(
                                input
                                    .app
                                    .world_mut()
                                    .spawn((
                                        FormId(REFERENCE),
                                        input.root_pose,
                                        ChildOf(input.cell),
                                    ))
                                    .id(),
                            );
                        }
                    }
                    input.extract();
                    assert_delta_only(&input);
                    assert_full_oracle(&mut input);
                }
            }
        }
    }

    #[test]
    fn journal_loss_is_an_explicit_recovery_and_next_change_is_incremental() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .remove::<SourceNodePose>();
        for _ in 0..2 {
            input
                .app
                .world_mut()
                .resource_mut::<Messages<crate::streaming::identity_changes::SourceNodeChanged>>()
                .update();
        }
        input.assert_blocked();
        assert!(input.sources().last_work.qualification_full);
        assert_full_oracle(&mut input);
        let pose = *input.app.world().get::<Transform>(primitive).unwrap();
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(SourceNodePose(pose));
        input.assert_bound(primitive);
        assert_delta_only(&input);
    }
    #[test]
    fn retired_descendant_hides_and_sticky_mesh_refusals_match_full_oracle() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(input.cell)
            .insert(Visibility::Inherited);
        crate::streaming::set_distance_retired_for_cluster_test(
            input.app.world_mut(),
            input.cell,
            true,
        );
        input.app.update();
        input.extract();
        assert!(input.sources().retired_refs.contains(&REFERENCE));
        assert_eq!(input.sources().last_work.retired_proof_keys_visited, 1);
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(Visibility::Hidden);
        input.extract();
        assert!(!input.sources().retired_refs.contains(&REFERENCE));
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(Visibility::Inherited);
        input.extract();
        assert!(input.sources().retired_refs.contains(&REFERENCE));
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert((RenderLayers::layer(2), bevy::light::NotShadowCaster));
        input.extract();
        let eligibility = &input.sources().retired_eligibility[&MainEntity::from(primitive)];
        assert!(!eligibility.casts_shadows);
        assert_eq!(eligibility.layers, RenderLayers::layer(2));
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().0.id();
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Modified { id: mesh });
        input.assert_blocked();
        assert!(!input.sources().retired_refs.contains(&REFERENCE));
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        // An ECS repair must not recertify an asset invalidated earlier in the run.
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .remove::<bevy::light::NotShadowCaster>();
        input.assert_blocked();
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
    }

    #[test]
    fn hook_skipped_reparenting_uses_exact_childof_dependencies() {
        use bevy::ecs::relationship::RelationshipHookMode;
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let original = input
            .app
            .world()
            .get::<ChildOf>(primitive)
            .unwrap()
            .parent();
        let other = input
            .app
            .world_mut()
            .spawn((
                Transform::IDENTITY,
                SourceNodePose(Transform::IDENTITY),
                FormId(u32::MAX),
            ))
            .id();
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert_with_relationship_hook_mode(ChildOf(other), RelationshipHookMode::Skip);
        input.extract();
        assert!(!input.sources().bound.contains_key(&input.key));
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert_with_relationship_hook_mode(ChildOf(original), RelationshipHookMode::Skip);
        input.assert_bound(primitive);
        assert_delta_only(&input);
        assert_full_oracle(&mut input);
    }
    #[test]
    fn unrelated_mesh_despawn_reclaims_ancestry_and_reference_memberships() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().clone();
        let before = input.sources().last_work;
        let version = input.sources().candidate_version;
        for _ in 0..16 {
            let root = input
                .app
                .world_mut()
                .spawn((Transform::IDENTITY, FormId(u32::MAX), ChildOf(input.cell)))
                .id();
            input
                .app
                .world_mut()
                .spawn((mesh.clone(), Transform::IDENTITY, ChildOf(root)));
            input.extract();
            assert_delta_only(&input);
            assert_eq!(input.sources().last_work.roots_scanned, 0);
            assert_eq!(
                input.sources().last_work.indexed_subjects,
                before.indexed_subjects + 2
            );
            input.app.world_mut().despawn(root);
            input.extract();
            assert_delta_only(&input);
            let after = input.sources().last_work;
            assert_eq!(
                (
                    after.indexed_subjects,
                    after.indexed_nodes,
                    after.indexed_reference_groups
                ),
                (
                    before.indexed_subjects,
                    before.indexed_nodes,
                    before.indexed_reference_groups
                )
            );
            assert_eq!(input.sources().candidate_version, version);
        }
        assert_full_oracle(&mut input);
    }
    #[test]
    fn disabled_root_and_mesh_membership_changes_use_deltas() {
        use bevy::ecs::entity_disabling::Disabled;
        for own_statics in [false, true] {
            let mut input = Fixture::pending();
            let primitive = input.spawn_stock();
            input.render.insert_resource(ClusterSettings {
                own_statics,
                ..default()
            });
            input.assert_bound(primitive);
            for entity in [primitive, input.root] {
                input.app.world_mut().entity_mut(entity).insert(Disabled);
                input.extract();
                assert!(!input.sources().bound.contains_key(&input.key));
                assert_delta_only(&input);
                assert_full_oracle(&mut input);
                input
                    .app
                    .world_mut()
                    .entity_mut(entity)
                    .remove::<Disabled>();
                input.assert_bound(primitive);
                assert_delta_only(&input);
                assert_full_oracle(&mut input);
            }
        }
    }

    #[test]
    fn form_id_disabler_preserves_root_domain_hot_observation_and_history() {
        use bevy::ecs::entity_disabling::Disabled;
        for own_statics in [false, true] {
            let mut input = Fixture::pending_with_disablers(|world| {
                world.register_disabling_component::<FormId>();
            });
            let primitive = input.spawn_stock();
            input.render.insert_resource(ClusterSettings {
                own_statics,
                ..default()
            });
            input.assert_bound(primitive);
            let before = input.sources().last_work.indexed_subjects;
            // Explicit required FormId admits this root while Query<Entity> does not.
            let world = input.app.world_mut();
            let mut generic = world.query::<Entity>();
            assert!(generic.get(world, input.root).is_err());
            assert!(world.entities().contains_spawned(input.root));
            // Several ordinary deltas BEFORE the full oracle can repair the index.
            let tag = *input.app.world().get::<SourcePrimitive>(primitive).unwrap();
            input.app.world_mut().entity_mut(primitive).insert(tag);
            input.assert_bound(primitive);
            assert_delta_only(&input);
            assert_eq!(input.sources().last_work.roots_scanned, 1);
            assert_eq!(input.sources().last_work.indexed_subjects, before);
            input
                .app
                .world_mut()
                .get_mut::<Transform>(input.root)
                .unwrap()
                .translation
                .x += 1.0;
            input.assert_blocked();
            assert_delta_only(&input);
            assert_eq!(input.sources().last_work.roots_scanned, 1);
            input
                .app
                .world_mut()
                .entity_mut(input.root)
                .insert(input.root_pose);
            input.assert_bound(primitive);
            assert_delta_only(&input);
            // Same-value root writes remain semantically quiet after the aligned snapshot.
            input
                .app
                .world_mut()
                .entity_mut(input.root)
                .insert(input.root_pose);
            input.assert_bound(primitive);
            assert_delta_only(&input);
            assert_eq!(input.sources().last_work.roots_scanned, 0);
            assert_full_oracle(&mut input);
            input
                .app
                .world_mut()
                .entity_mut(input.root)
                .insert(Disabled);
            input.extract();
            assert_delta_only(&input);
            assert!(!input.sources().bound.contains_key(&input.key));
            assert!(input.sources().seen_roots[&input.root].contains(&REFERENCE));
            assert!(input.sources().present.contains(&REFERENCE));
            assert_full_oracle(&mut input);
            input
                .app
                .world_mut()
                .entity_mut(input.root)
                .remove::<Disabled>();
            input.assert_bound(primitive);
            assert_delta_only(&input);
            input.app.world_mut().despawn(input.root);
            input.extract();
            assert_delta_only(&input);
            assert!(!input.sources().seen_roots.contains_key(&input.root));
            assert_full_oracle(&mut input);
        }
    }

    #[test]
    fn mesh_disabler_indexes_mesh_role_without_broadening_parent_or_node_reads() {
        for own_statics in [false, true] {
            let mut input = Fixture::pending_with_disablers(|world| {
                world.register_disabling_component::<Mesh3d>();
            });
            let primitive = input.spawn_stock();
            input.render.insert_resource(ClusterSettings {
                own_statics,
                ..default()
            });
            input.extract();
            // Mesh enumeration admits Mesh3d; dependency reads deliberately do not.
            assert!(!input.sources().bound.contains_key(&input.key));
            let before = input.sources().last_work.indexed_subjects;
            let world = input.app.world_mut();
            let mut meshes = world.query_filtered::<Entity, With<Mesh3d>>();
            assert!(meshes.get(world, primitive).is_ok());
            let mut generic = world.query::<Entity>();
            assert!(generic.get(world, primitive).is_err());
            let mut parents = world.query::<&ChildOf>();
            assert!(parents.get(world, primitive).is_err());
            let mesh = world.get::<Mesh3d>(primitive).unwrap().clone();
            let added = world
                .spawn((
                    mesh,
                    Transform::IDENTITY,
                    SourceNodePose(Transform::IDENTITY),
                    ChildOf(input.root),
                ))
                .id();
            input.extract();
            assert_delta_only(&input);
            assert_eq!(input.sources().last_work.indexed_subjects, before + 1);
            input.extract();
            assert_delta_only(&input);
            assert_eq!(input.sources().last_work.subjects_updated, 0);
            *input.app.world_mut().get_mut::<Transform>(added).unwrap() = Transform::IDENTITY;
            input.extract();
            assert_delta_only(&input);
            assert_eq!(input.sources().last_work.subjects_updated, 0);
            // An in-place hot edit on an indexed Mesh3d must still be observed,
            // even though no original dependency query can certify this primitive.
            // The authored baseline makes this a semantic eligibility change;
            // unbaselined poses are all the same refused proof.
            input
                .app
                .world_mut()
                .get_mut::<Transform>(added)
                .unwrap()
                .translation
                .x += 1.0;
            input.extract();
            assert_delta_only(&input);
            assert!(input.sources().last_work.subjects_updated > 0);
            assert_full_oracle(&mut input);
            input
                .app
                .world_mut()
                .get_mut::<Transform>(added)
                .unwrap()
                .translation
                .x += 1.0;
            input.extract();
            assert_delta_only(&input);
            assert_eq!(input.sources().last_work.subjects_updated, 0);
            *input.app.world_mut().get_mut::<Transform>(added).unwrap() = Transform::IDENTITY;
            input.extract();
            assert_delta_only(&input);
            assert!(input.sources().last_work.subjects_updated > 0);
            assert_full_oracle(&mut input);
            let tag = *input.app.world().get::<SourcePrimitive>(primitive).unwrap();
            input.app.world_mut().entity_mut(primitive).insert(tag);
            input.extract();
            assert_delta_only(&input);
            assert!(input.sources().last_work.subjects_updated > 0);
            assert!(!input.sources().bound.contains_key(&input.key));
            assert_full_oracle(&mut input);
            input.app.world_mut().entity_mut(added).remove::<Mesh3d>();
            input.extract();
            assert_delta_only(&input);
            assert_eq!(input.sources().last_work.indexed_subjects, before);
            input.app.world_mut().despawn(added);
            input.extract();
            assert_delta_only(&input);
            assert_full_oracle(&mut input);
        }
    }

    #[test]
    fn replacement_journal_with_unrelated_suffix_requires_full_epoch_coverage() {
        use crate::streaming::identity_changes::{SourceNodeChanged, StructureEpoch};
        for own_statics in [false, true] {
            let mut input = Fixture::pending();
            let primitive = input.spawn_stock();
            input.render.insert_resource(ClusterSettings {
                own_statics,
                ..default()
            });
            input.assert_bound(primitive);
            let mut stale = input
                .app
                .world()
                .resource::<Messages<SourceNodeChanged>>()
                .get_cursor_current();
            // Epoch count bounds the old cursor count even if older messages expired.
            let unrelated = usize::try_from(input.app.world().resource::<StructureEpoch>().version)
                .unwrap()
                + 1;
            input
                .app
                .world_mut()
                .entity_mut(primitive)
                .remove::<SourceNodePose>();
            input
                .app
                .world_mut()
                .insert_resource(Messages::<SourceNodeChanged>::default());
            for _ in 0..unrelated {
                input.app.world_mut().spawn(Transform::IDENTITY);
            }
            let journal = input.app.world().resource::<Messages<SourceNodeChanged>>();
            assert_eq!(stale.missed_messages(journal), 0);
            let surviving: Vec<_> = stale.read(journal).copied().collect();
            assert!(!surviving.is_empty());
            assert!(surviving.iter().all(|event| event.entity != primitive));
            input.assert_blocked();
            let work = input.sources().last_work;
            assert!(work.qualification_full);
            assert!(work.journal_events > 0);
            assert!(!work.journal_complete);
            // Resynchronization is checked immediately, without an intervening oracle.
            let pose = *input.app.world().get::<Transform>(primitive).unwrap();
            input
                .app
                .world_mut()
                .entity_mut(primitive)
                .insert(SourceNodePose(pose));
            input.assert_bound(primitive);
            assert_delta_only(&input);
            assert!(input.sources().last_work.journal_complete);
            assert_full_oracle(&mut input);
        }
    }

    #[test]
    fn missing_or_lower_count_journal_recovers_and_resynchronizes_before_next_delta() {
        use crate::streaming::identity_changes::SourceNodeChanged;
        for missing_at_extract in [false, true] {
            let mut input = Fixture::pending();
            let primitive = input.spawn_stock();
            input.assert_bound(primitive);
            input
                .app
                .world_mut()
                .remove_resource::<Messages<SourceNodeChanged>>();
            // The epoch still advances if the journal resource itself is absent.
            input
                .app
                .world_mut()
                .entity_mut(primitive)
                .remove::<SourceNodePose>();
            if !missing_at_extract {
                input
                    .app
                    .world_mut()
                    .insert_resource(Messages::<SourceNodeChanged>::default());
            }
            input.assert_blocked();
            assert!(input.sources().last_work.qualification_full);
            assert!(!input.sources().last_work.journal_complete);
            if missing_at_extract {
                input
                    .app
                    .world_mut()
                    .insert_resource(Messages::<SourceNodeChanged>::default());
            }
            let pose = *input.app.world().get::<Transform>(primitive).unwrap();
            input
                .app
                .world_mut()
                .entity_mut(primitive)
                .insert(SourceNodePose(pose));
            input.assert_bound(primitive);
            assert_delta_only(&input);
            assert!(input.sources().last_work.journal_complete);
            assert_full_oracle(&mut input);
        }
    }

    #[test]
    fn retired_proof_visits_only_reference_keys_including_ambiguous_keys() {
        let mut stock = HashMap::new();
        let member = MainEntity::from(Entity::from_raw_u32(1).unwrap());
        stock.insert((REFERENCE, 0), member);
        for reference in 0..256 {
            stock.insert((u32::MAX - reference, 0), member);
        }
        let keys = HashSet::from([(REFERENCE, 0), (REFERENCE, 1)]);
        let mut work = SourceExtractionWork::default();
        let (proven, instantiated) = retired_reference_proof(Some(&keys), &stock, &mut work);
        assert_eq!(proven, HashSet::from([member]));
        assert_eq!(instantiated, BTreeSet::from([0]));
        assert_eq!(work.retired_proof_keys_visited, 2);
        let (proven, instantiated) = retired_reference_proof(None, &stock, &mut work);
        assert!(proven.is_empty());
        assert!(instantiated.is_empty());
        assert_eq!(work.retired_proof_keys_visited, 2);
    }

    fn layout_batch() -> (Fixture, Handle<StandardMaterial>, Handle<Mesh>) {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        let stock = input
            .app
            .world()
            .get::<Mesh3d>(primitive)
            .unwrap()
            .0
            .clone();
        let scene = input
            .app
            .world()
            .get::<WorldAssetRoot>(input.root)
            .unwrap()
            .0
            .clone();
        let material = input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        input
            .app
            .world_mut()
            .resource_mut::<ClusterBatchHandles>()
            .set(0, material.clone());
        input
            .app
            .insert_resource(LazyBatches::for_source_test(MODEL, scene));
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), Some(false));
        (input, material, stock)
    }

    #[test]
    fn actual_extract_material_events_refuse_hot_depth_and_restore_plain_stock_layout() {
        let (mut input, material, _) = layout_batch();
        input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&material)
            .unwrap()
            .depth_map = Some(Handle::default());
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<StandardMaterial>>>()
            .write(AssetEvent::Modified { id: material.id() });
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), None);
        assert!(input.sources().source_fallback_batches.contains(&0));
        input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&material)
            .unwrap()
            .depth_map = None;
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<StandardMaterial>>>()
            .write(AssetEvent::Modified { id: material.id() });
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), Some(false));
        assert!(!input.sources().source_fallback_batches.contains(&0));
    }

    #[test]
    fn actual_extract_mesh_mutation_revokes_layout_and_wakes_only_dependent_templates() {
        let (mut input, _, stock) = layout_batch();
        let version = input.sources().layout_version;
        let unrelated = input
            .app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(dummy_mesh());
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Modified { id: unrelated.id() });
        input.extract();
        assert_eq!(input.sources().layout_version, version);
        assert_eq!(input.sources().last_work.template_rows, 0);
        assert_eq!(input.sources().last_work.primitives_scanned, 0);
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Modified { id: stock.id() });
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), None);
        assert!(input.sources().source_fallback_batches.contains(&0));
        assert!(input.sources().layout_version > version);
        assert_eq!(input.sources().last_work.template_rows, 1);
        assert_eq!(input.sources().last_work.primitives_scanned, 0);
    }

    #[test]
    fn actual_extract_material_removal_and_handle_clear_revoke_old_layout() {
        let (mut input, material, _) = layout_batch();
        input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .remove(material.id());
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<StandardMaterial>>>()
            .write(AssetEvent::Removed { id: material.id() });
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), None);
        input
            .app
            .world_mut()
            .resource_mut::<ClusterBatchHandles>()
            .handles[0] = None;
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), None);
        assert!(!input.sources().template_rows.pending.contains(&0));
    }

    #[test]
    fn actual_extract_source_scope_rebinding_and_scene_removal_revoke_layout() {
        let (mut input, _, _) = layout_batch();
        let original = input
            .app
            .world()
            .resource::<LazyBatches>()
            .source_scene(0)
            .unwrap()
            .clone();
        let missing = input
            .app
            .world()
            .resource::<Assets<WorldAsset>>()
            .reserve_handle();
        input
            .app
            .insert_resource(LazyBatches::for_source_test(MODEL, missing));
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), None);
        assert!(input.sources().template_rows.pending.contains(&0));
        input
            .app
            .insert_resource(LazyBatches::for_source_test(MODEL, original.clone()));
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), Some(false));
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<WorldAsset>>>()
            .write(AssetEvent::Removed { id: original.id() });
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), None);
        assert!(input.sources().source_fallback_batches.contains(&0));
    }

    #[test]
    fn unrequested_material_does_not_hide_a_stock_mesh_qualification_invalidation() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        assert!(
            input
                .app
                .world()
                .resource::<ClusterBatchHandles>()
                .handles
                .is_empty()
        );
        let stock = input.app.world().get::<Mesh3d>(primitive).unwrap().0.id();
        assert!(input.sources().stock_meshes.contains_key(&stock));
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Modified { id: stock });
        input.assert_blocked();
    }

    #[test]
    fn retained_generated_layout_survives_material_deltas_and_journal_recovery() {
        use crate::streaming::identity_changes::SourceNodeChanged;
        for own_statics in [false, true] {
            let mut input = Fixture::pending();
            let primitive = input.spawn_stock();
            let stock = input
                .app
                .world()
                .get::<Mesh3d>(primitive)
                .unwrap()
                .0
                .clone();
            // Model the initial glTF-generated layout before certification. This is
            // not runtime regeneration and does not claim a glTF loader E2E test.
            {
                let mut meshes = input.app.world_mut().resource_mut::<Assets<Mesh>>();
                let mut mesh = meshes.get_mut(&stock).unwrap();
                let vertices = mesh.count_vertices();
                mesh.try_insert_attribute(
                    Mesh::ATTRIBUTE_TANGENT,
                    vec![[1.0_f32, 0.0, 0.0, 1.0]; vertices],
                )
                .unwrap();
            }
            source_template(&mut input);
            input.render.insert_resource(ClusterSettings {
                own_statics,
                ..default()
            });
            input.assert_bound(primitive);
            assert_eq!(input.sources().stock_tangents(0), Some(true));
            let material = input.app.world().resource::<ClusterBatchHandles>().handles[0]
                .as_ref()
                .unwrap()
                .id()
                .typed::<StandardMaterial>();
            // Keep an unaffected row to detect a competing whole-table invalidator.
            let unrelated = input
                .app
                .world_mut()
                .resource_mut::<Assets<StandardMaterial>>()
                .add(StandardMaterial::default());
            input
                .app
                .world_mut()
                .resource_mut::<ClusterBatchHandles>()
                .set(1, unrelated);
            input.extract();
            let version = input.sources().layout_version;
            for normal_map in [Some(Handle::default()), None] {
                input
                    .app
                    .world_mut()
                    .resource_mut::<Assets<StandardMaterial>>()
                    .get_mut(material)
                    .unwrap()
                    .normal_map_texture = normal_map;
                input
                    .app
                    .world_mut()
                    .resource_mut::<Messages<AssetEvent<StandardMaterial>>>()
                    .write(AssetEvent::Modified { id: material });
                input.assert_bound(primitive);
                assert_eq!(input.sources().last_work.template_rows, 1);
                assert_eq!(input.sources().last_work.primitives_scanned, 0);
                assert_eq!(input.sources().stock_tangents(0), Some(true));
                assert!(input.sources().normal_maps[0]);
                assert!(!input.sources().source_fallback_batches.contains(&0));
            }
            // Layout stays stable while an uncovered identity interval revokes stock
            // proof. Recover immediately, without a full oracle repairing the delta.
            input
                .app
                .world_mut()
                .remove_resource::<Messages<SourceNodeChanged>>();
            input
                .app
                .world_mut()
                .entity_mut(primitive)
                .remove::<SourceNodePose>();
            input
                .app
                .world_mut()
                .insert_resource(Messages::<SourceNodeChanged>::default());
            input.assert_blocked();
            assert!(input.sources().last_work.qualification_full);
            assert!(!input.sources().last_work.journal_complete);
            assert_eq!(input.sources().stock_tangents(0), Some(true));
            let pose = *input.app.world().get::<Transform>(primitive).unwrap();
            input
                .app
                .world_mut()
                .entity_mut(primitive)
                .insert(SourceNodePose(pose));
            input.assert_bound(primitive);
            assert_delta_only(&input);
            assert!(input.sources().last_work.journal_complete);
            assert_eq!(input.sources().last_work.template_rows, 0);
            assert_eq!(input.sources().stock_tangents(0), Some(true));
            assert_eq!(input.sources().layout_version, version);
            input
                .app
                .world_mut()
                .resource_mut::<Messages<AssetEvent<Mesh>>>()
                .write(AssetEvent::Modified { id: stock.id() });
            input.assert_blocked();
            assert_eq!(input.sources().stock_tangents(0), None);
            assert_eq!(input.sources().last_work.template_rows, 1);
            assert_eq!(input.sources().last_work.primitives_scanned, 0);
            assert!(input.sources().source_fallback_batches.contains(&0));
            input.render.resource_mut::<Sources>().force_full = true;
            input.extract();
            assert_eq!(input.sources().stock_tangents(0), None);
            assert!(input.sources().source_fallback_batches.contains(&0));
        }
    }

    #[test]
    fn fixture_rebinding_retires_reverse_edges_and_removal_revokes_layout() {
        let (mut input, _, stock) = layout_batch();
        input.app.world_mut().remove_resource::<LazyBatches>();
        input
            .app
            .insert_resource(ClusterBatchStockMeshes(vec![stock.clone()]));
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), Some(false));
        assert_eq!(
            input.sources().template_rows.fixture_by_mesh[&stock.id()],
            BTreeSet::from([0])
        );
        let replacement = input
            .app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&stock)
            .unwrap()
            .clone();
        let replacement = input
            .app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(replacement);
        input
            .app
            .insert_resource(ClusterBatchStockMeshes(vec![replacement.clone()]));
        input.extract();
        assert!(
            !input
                .sources()
                .template_rows
                .fixture_by_mesh
                .contains_key(&stock.id())
        );
        assert_eq!(
            input.sources().template_rows.fixture_by_row[&0],
            replacement.id()
        );
        let version = input.sources().layout_version;
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Modified { id: stock.id() });
        input.extract();
        assert_eq!(input.sources().last_work.template_rows, 0);
        assert_eq!(input.sources().stock_tangents(0), Some(false));
        assert_eq!(input.sources().layout_version, version);
        input
            .app
            .world_mut()
            .remove_resource::<ClusterBatchStockMeshes>();
        input.extract();
        assert_eq!(input.sources().stock_tangents(0), None);
        assert!(input.sources().template_rows.fixture_by_mesh.is_empty());
        assert!(input.sources().template_rows.fixture_by_row.is_empty());
    }

    fn source_template(input: &mut Fixture) -> Handle<WorldAsset> {
        let scene = input
            .app
            .world()
            .get::<WorldAssetRoot>(input.root)
            .unwrap()
            .0
            .clone();
        let material = input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        input
            .app
            .world_mut()
            .resource_mut::<ClusterBatchHandles>()
            .set(0, material);
        input
            .app
            .insert_resource(LazyBatches::for_source_test(MODEL, scene.clone()));
        scene
    }

    #[test]
    fn unrelated_mesh_events_do_no_qualification_work_in_either_ownership_mode() {
        for own_statics in [false, true] {
            let mut input = Fixture::pending();
            let primitive = input.spawn_stock();
            input.render.insert_resource(ClusterSettings {
                own_statics,
                ..default()
            });
            source_template(&mut input);
            input.assert_bound(primitive);
            input.extract();
            assert!(input.sources().templates[0]);
            let version = input.sources().candidate_version;
            let refreshes = input.sources().qualification_refreshes;
            let mesh = Handle::<Mesh>::Uuid(
                bevy::asset::uuid::Uuid::from_u128(0xabcdef),
                std::marker::PhantomData,
            );
            for event in [
                AssetEvent::Modified { id: mesh.id() },
                AssetEvent::Removed { id: mesh.id() },
            ] {
                input
                    .app
                    .world_mut()
                    .resource_mut::<Messages<AssetEvent<Mesh>>>()
                    .write(event);
                input.assert_bound(primitive);
                let work = input.sources().last_work;
                assert_eq!(
                    (
                        work.roots_scanned,
                        work.meshes_scanned,
                        work.primitives_scanned,
                        work.template_rows,
                        work.stock_rows_revoked
                    ),
                    (0, 0, 0, 0, 0)
                );
                assert_eq!(input.sources().qualification_refreshes, refreshes);
                assert_eq!(input.sources().candidate_version, version);
                assert!(input.sources().templates[0]);
            }
        }
    }

    #[test]
    fn a_bound_mesh_modification_revokes_stock_and_its_template_without_a_scene_scan() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        source_template(&mut input);
        input.assert_bound(primitive);
        input.extract();
        assert!(input.sources().templates[0]);
        let version = input.sources().candidate_version;
        let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().0.id();
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Modified { id: mesh });
        input.assert_blocked();
        let work = input.sources().last_work;
        assert_eq!(
            (
                work.roots_scanned,
                work.meshes_scanned,
                work.primitives_scanned
            ),
            (0, 0, 0)
        );
        assert_eq!((work.stock_rows_revoked, work.template_rows), (1, 1));
        assert!(!input.sources().templates[0]);
        assert!(input.sources().source_fallback_batches.contains(&0));
        assert!(
            !input
                .sources()
                .stock_primitives
                .contains_key(&(REFERENCE, 0))
        );
        assert!(input.sources().candidate_version > version);
        let revoked_version = input.sources().candidate_version;
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Removed { id: mesh });
        input.assert_blocked();
        assert_eq!(input.sources().last_work.stock_rows_revoked, 0);
        assert_eq!(input.sources().candidate_version, revoked_version);
    }

    #[test]
    fn a_scene_modification_redecides_only_its_template_and_preserves_live_stock_proof() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        let scene = source_template(&mut input);
        input.assert_bound(primitive);
        input.extract();
        let refreshes = input.sources().qualification_refreshes;
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<WorldAsset>>>()
            .write(AssetEvent::Modified { id: scene.id() });
        input.assert_bound(primitive);
        let work = input.sources().last_work;
        assert_eq!(
            (
                work.roots_scanned,
                work.meshes_scanned,
                work.primitives_scanned,
                work.stock_rows_revoked,
                work.template_rows
            ),
            (0, 0, 0, 0, 1)
        );
        assert_eq!(input.sources().qualification_refreshes, refreshes);
        assert!(!input.sources().templates[0]);
    }

    #[test]
    fn simultaneous_ecs_mutation_and_asset_invalidation_keep_runtime_root_checks() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().0.id();
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x += 1.0;
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Modified { id: mesh });
        input.assert_blocked();
        assert!(input.sources().last_work.roots_scanned > 0);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(input.root_pose);
        input.assert_blocked(); // Returning the pose does not recertify the modified mesh.
    }

    #[test]
    fn a_mesh_withdrawal_revokes_the_entire_retired_reference_exception() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(input.cell)
            .insert(Visibility::Inherited);
        crate::streaming::set_distance_retired_for_cluster_test(
            input.app.world_mut(),
            input.cell,
            true,
        );
        input.app.update();
        input.extract();
        assert!(input.sources().retired_refs.contains(&REFERENCE));
        input.extract();
        let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().0.id();
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Removed { id: mesh });
        input.assert_blocked();
        assert!(!input.sources().retired_refs.contains(&REFERENCE));
        assert!(input.sources().retired_present.contains(&REFERENCE));
        assert!(input.sources().retired_materials.is_empty());
        assert!(input.sources().retired_eligibility.is_empty());
        assert_eq!(input.sources().last_work.meshes_scanned, 0);
    }

    #[test]
    fn asset_deltas_match_forced_full_qualification_and_preserve_an_unaffected_row() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        let scene = source_template(&mut input);
        // A second material row has no source scene. It must not be redecided for row 0's mesh.
        let material = input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        input
            .app
            .world_mut()
            .resource_mut::<ClusterBatchHandles>()
            .set(1, material);
        input.assert_bound(primitive);
        input.extract();
        assert!(input.sources().templates[0]);
        let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().0.id();
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Removed { id: mesh });
        input.extract();
        assert_eq!(input.sources().last_work.template_rows, 1);
        let incremental = input.sources();
        let expected = (
            incremental.present.clone(),
            incremental.bound.clone(),
            incremental.stock_primitives.clone(),
            incremental.retired_refs.clone(),
            incremental.retired_materials.clone(),
            incremental.retired_eligibility.clone(),
            incremental.templates.clone(),
            incremental.normal_maps.clone(),
            incremental.source_fallback_batches.clone(),
        );
        // Replacing only the Arc forces the independent existing full ECS path.
        let replacement = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(input._directory.path(), false).unwrap(),
        );
        input.app.insert_resource(ValidatedPackage(replacement));
        input.extract();
        let rebuilt = input.sources();
        assert_eq!(
            expected,
            (
                rebuilt.present.clone(),
                rebuilt.bound.clone(),
                rebuilt.stock_primitives.clone(),
                rebuilt.retired_refs.clone(),
                rebuilt.retired_materials.clone(),
                rebuilt.retired_eligibility.clone(),
                rebuilt.templates.clone(),
                rebuilt.normal_maps.clone(),
                rebuilt.source_fallback_batches.clone()
            )
        );
        assert!(rebuilt.template_meshes.by_scene.contains_key(&scene.id()));
    }

    #[test]
    fn invalidating_one_ambiguous_candidate_binds_the_survivor_without_a_scene_scan() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let original_mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().0.id();
        // An independent labelled handle in the same model path exercises differing asset
        // dependencies of duplicated runtime ordinals. No asset I/O needs to finish.
        let alternative: Handle<Mesh> = input
            .app
            .world()
            .resource::<AssetServer>()
            .load(format!("{MODEL}#Mesh999/Primitive0"));
        let world = input.app.world();
        let pose = *world.get::<Transform>(primitive).unwrap();
        let baseline = *world.get::<SourceNodePose>(primitive).unwrap();
        let parent = world.get::<ChildOf>(primitive).unwrap().parent();
        let tag = *world.get::<SourcePrimitive>(primitive).unwrap();
        let matrix = *world.get::<SourcePrimitiveTransform>(primitive).unwrap();
        let static_model = *world.get::<SourceStaticModel>(primitive).unwrap();
        let duplicate = input
            .app
            .world_mut()
            .spawn((
                Mesh3d(alternative.clone()),
                SourcePrimitiveMesh(alternative),
                pose,
                baseline,
                ChildOf(parent),
                tag,
                matrix,
                static_model,
            ))
            .id();
        input.assert_blocked();
        input
            .app
            .world_mut()
            .resource_mut::<Messages<AssetEvent<Mesh>>>()
            .write(AssetEvent::Modified { id: original_mesh });
        input.assert_bound(duplicate);
        assert_eq!(input.sources().last_work.stock_rows_requalified, 1);
        assert_eq!(input.sources().last_work.meshes_scanned, 0);
        let replacement = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(input._directory.path(), false).unwrap(),
        );
        input.app.insert_resource(ValidatedPackage(replacement));
        input.assert_bound(duplicate);
    }

    #[test]
    fn refused_template_dependencies_include_all_meshes_and_remove_only_the_replaced_scene() {
        let mesh_a = Handle::<Mesh>::Uuid(
            bevy::asset::uuid::Uuid::from_u128(701),
            std::marker::PhantomData,
        );
        let mesh_b = Handle::<Mesh>::Uuid(
            bevy::asset::uuid::Uuid::from_u128(702),
            std::marker::PhantomData,
        );
        let mut first = World::new();
        first.spawn(Mesh3d(mesh_a.clone())); // Missing identity: validation refuses immediately.
        first.spawn(Mesh3d(mesh_b.clone()));
        let mut second = World::new();
        second.spawn(Mesh3d(mesh_b.clone()));
        let mut assets = Assets::<WorldAsset>::default();
        let first = assets.add(WorldAsset::new(first));
        let second = assets.add(WorldAsset::new(second));
        assert!(!template_valid(
            assets.get(&first).unwrap(),
            &HashSet::new()
        ));
        let mut dependencies = TemplateMeshDependencies::default();
        dependencies.register(first.id(), assets.get(&first).unwrap());
        dependencies.register(second.id(), assets.get(&second).unwrap());
        assert_eq!(
            dependencies.by_mesh[&mesh_b.id()],
            HashSet::from([first.id(), second.id()])
        );
        assert_eq!(
            dependencies.by_mesh[&mesh_a.id()],
            HashSet::from([first.id()])
        );
        dependencies.remove(first.id());
        assert!(!dependencies.by_mesh.contains_key(&mesh_a.id()));
        assert_eq!(
            dependencies.by_mesh[&mesh_b.id()],
            HashSet::from([second.id()])
        );
        dependencies.remove(second.id());
        assert!(dependencies.by_scene.is_empty());
        assert!(dependencies.by_mesh.is_empty());
    }

    #[test]
    fn warm_qualification_reuses_and_pose_changes_revoke_immediately() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let refreshes = input.sources().qualification_refreshes;
        input.assert_bound(primitive);
        assert_eq!(input.sources().qualification_refreshes, refreshes);
        input.app.world_mut().entity_mut(input.root).insert((
            input.root_pose,
            FormId(REFERENCE),
            InheritedVisibility::VISIBLE,
        ));
        input.assert_bound(primitive);
        assert_eq!(
            input.sources().qualification_refreshes,
            refreshes,
            "marking unchanged facts must not redo source qualification"
        );

        input
            .app
            .world_mut()
            .get_mut::<Transform>(input.root)
            .unwrap()
            .translation
            .x += 1.0;
        input.extract();
        assert!(input.sources().bound.is_empty());
        assert!(input.sources().qualification_refreshes > refreshes);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(input.root_pose);
        input.assert_bound(primitive);
    }

    #[test]
    fn moving_an_already_refused_node_stays_refused_until_its_authored_pose_returns() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let pose = *input.app.world().get::<Transform>(primitive).unwrap();
        input
            .app
            .world_mut()
            .get_mut::<Transform>(primitive)
            .unwrap()
            .translation
            .x += 1.0;
        input.extract();
        assert!(input.sources().bound.is_empty());
        let refreshes = input.sources().qualification_refreshes;
        input
            .app
            .world_mut()
            .get_mut::<Transform>(primitive)
            .unwrap()
            .translation
            .x += 1.0;
        input.extract();
        assert!(input.sources().bound.is_empty());
        assert_eq!(input.sources().qualification_refreshes, refreshes);
        input.app.world_mut().entity_mut(primitive).insert(pose);
        input.assert_bound(primitive);
    }

    #[test]
    fn certificate_scope_catchup_rebuilds_after_a_mismatch_clears_bindings() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let certificate = input.app.world().resource::<ValidatedPackage>().0.clone();
        input.app.insert_resource(ClusterScene {
            certificate: None,
            terrain_blocks: Vec::new(),
            meshes: Vec::new(),
            instances: Vec::new(),
            origin_offset: Vec3::ZERO,
            blocks: Vec::new(),
        });
        input.extract();
        assert!(input.sources().bound.is_empty());
        input
            .app
            .world_mut()
            .resource_mut::<ClusterScene>()
            .certificate = Some(certificate);
        input.assert_bound(primitive);
    }

    #[test]
    fn actual_distance_retirement_preserves_proof_and_revive_restores_stock_rules() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        // The shared geometry-only fixture intentionally has no material; this
        // regression also exercises current material identity while retired.
        let initial_material = input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(MeshMaterial3d(initial_material));
        input.assert_bound(primitive);
        let original_version = input.sources().version;
        input
            .app
            .world_mut()
            .entity_mut(input.cell)
            .insert(Visibility::Inherited);
        crate::streaming::set_distance_retired_for_cluster_test(
            input.app.world_mut(),
            input.cell,
            true,
        );
        input.app.update();
        input.extract();
        assert!(input.sources().retired_refs.contains(&REFERENCE));
        assert!(input.sources().retired_present.contains(&REFERENCE));
        assert!(input.sources().present.contains(&REFERENCE));
        assert_eq!(
            input.sources().bound.get(&input.key),
            Some(&MainEntity::from(primitive))
        );
        assert!(input.sources().version > original_version);
        let retired_version = input.sources().version;

        // The retirement exception must never overrule a reference/child hide.
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(Visibility::Hidden);
        input.extract();
        assert!(!input.sources().retired_refs.contains(&REFERENCE));
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(Visibility::Inherited);
        input.extract();
        assert!(input.sources().retired_refs.contains(&REFERENCE));
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(Visibility::Hidden);
        input.extract();
        assert!(!input.sources().retired_refs.contains(&REFERENCE));
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(Visibility::Inherited);
        input.extract();
        assert!(input.sources().retired_refs.contains(&REFERENCE));

        let eligibility_version = input.sources().version;
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(RenderLayers::layer(2));
        input.extract();
        assert_eq!(
            input.sources().retired_eligibility[&MainEntity::from(primitive)].layers,
            RenderLayers::layer(2)
        );
        assert!(input.sources().version > eligibility_version);
        let caster_version = input.sources().version;
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(bevy::light::NotShadowCaster);
        input.extract();
        assert!(!input.sources().retired_eligibility[&MainEntity::from(primitive)].casts_shadows);
        assert!(input.sources().version > caster_version);
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .remove::<bevy::light::NotShadowCaster>()
            .remove::<RenderLayers>();
        input.extract();

        let original_mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().clone();
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .remove::<Mesh3d>();
        input.extract();
        assert!(!input.sources().retired_refs.contains(&REFERENCE));
        assert!(input.sources().present.contains(&REFERENCE));
        assert!(input.sources().retired_present.contains(&REFERENCE));
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(original_mesh);
        input.extract();
        assert!(input.sources().retired_refs.contains(&REFERENCE));

        let original_material = input
            .app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(primitive)
            .unwrap()
            .clone();
        let changed_material = input
            .app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let material_version = input.sources().version;
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(MeshMaterial3d(changed_material.clone()));
        input.extract();
        assert_eq!(
            input
                .sources()
                .retired_materials
                .get(&MainEntity::from(primitive)),
            Some(&Some(changed_material.id().untyped()))
        );
        assert!(input.sources().version > material_version);
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(original_material);
        input.extract();

        // The real marker's removal immediately restores the ordinary path.
        crate::streaming::set_distance_retired_for_cluster_test(
            input.app.world_mut(),
            input.cell,
            false,
        );
        input.app.update();
        input.assert_bound(primitive);
        assert!(!input.sources().retired_refs.contains(&REFERENCE));
        assert!(input.sources().version > retired_version);
        assert!(input.sources().seen_roots.contains_key(&input.root));
    }

    #[test]
    fn reparented_stock_under_retired_cell_is_not_credited_as_drawable_stock() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(input.cell)
            .insert(Visibility::Inherited);
        crate::streaming::set_distance_retired_for_cluster_test(
            input.app.world_mut(),
            input.cell,
            true,
        );
        input.app.update();
        input.extract();
        assert!(input.sources().retired_refs.contains(&REFERENCE));
        let wrapper = input
            .app
            .world_mut()
            .spawn((
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(input.cell),
            ))
            .id();
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(ChildOf(wrapper));
        input.extract();
        assert!(input.sources().retired_present.contains(&REFERENCE));
        assert!(!input.sources().retired_refs.contains(&REFERENCE));
        assert!(input.sources().present.contains(&REFERENCE));
        assert!(input.sources().seen_roots.contains_key(&input.root));
        assert!(!input.sources().bound.contains_key(&input.key));
    }

    #[test]
    fn unrelated_stock_meshes_never_enter_the_cluster_candidate_history() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let version = input.sources().candidate_version;
        let unrelated_id = u32::MAX;
        let unrelated = input
            .app
            .world_mut()
            .spawn((
                FormId(unrelated_id),
                Transform::IDENTITY,
                ChildOf(input.cell),
            ))
            .id();
        let mesh = input.app.world().get::<Mesh3d>(primitive).unwrap().clone();
        input
            .app
            .world_mut()
            .spawn((mesh, Transform::IDENTITY, ChildOf(unrelated)));
        input.extract();
        assert!(!input.sources().present.contains(&unrelated_id));
        assert!(!input.sources().seen_roots.contains_key(&unrelated));
        assert_eq!(input.sources().candidate_version, version);
        input.app.world_mut().entity_mut(unrelated).despawn();
        input.extract();
        assert_eq!(input.sources().candidate_version, version);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x += 1.0;
        input.assert_blocked();
        assert!(input.sources().candidate_version > version);
        assert!(!input.sources().candidate_worlds.all);
        assert_eq!(
            input.sources().candidate_worlds.worlds,
            HashSet::from([input.key.worldspace])
        );
    }

    #[test]
    fn nested_unrelated_reference_is_a_barrier_and_its_removal_rebinds_next_extract() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let parent = input
            .app
            .world()
            .get::<ChildOf>(primitive)
            .unwrap()
            .parent();
        let unrelated = input
            .app
            .world_mut()
            .spawn((
                FormId(u32::MAX),
                Transform::IDENTITY,
                SourceNodePose(Transform::IDENTITY),
                ChildOf(parent),
            ))
            .id();
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(ChildOf(unrelated));
        // The identity wrapper preserves the exact authored matrix. It is the nearer unrelated
        // reference barrier, rather than a pose/material failure, that must prevent binding.
        let mut state =
            bevy::ecs::system::SystemState::<Query<NodeState>>::new(input.app.world_mut());
        let nodes = state.get(input.app.world()).unwrap();
        assert_eq!(
            model_matrix(primitive, input.root, &nodes),
            Some(
                input
                    .app
                    .world()
                    .get::<SourcePrimitiveTransform>(primitive)
                    .unwrap()
                    .0
            )
        );
        input.assert_blocked();
        assert!(!input.sources().seen_roots.contains_key(&unrelated));
        input
            .app
            .world_mut()
            .entity_mut(unrelated)
            .remove::<FormId>();
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(unrelated)
            .insert(FormId(u32::MAX));
        input.assert_blocked();
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .insert(ChildOf(parent));
        input.assert_bound(primitive);
    }

    #[test]
    fn actual_object_and_certificate_changes_advance_candidate_version() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let old = input.sources().candidate_version;
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(Transform::from_translation(
                input.root_pose.translation + Vec3::X,
            ));
        input.assert_blocked();
        assert!(input.sources().candidate_version > old);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(input.root_pose);
        input.assert_bound(primitive);
        let old = input.sources().candidate_version;
        let replacement = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(input._directory.path(), false).unwrap(),
        );
        input
            .app
            .world_mut()
            .insert_resource(ValidatedPackage(replacement));
        input.extract();
        assert!(input.sources().candidate_version > old);
    }

    #[test]
    fn pending_then_real_spawn_binds_and_root_identity_mutations_revoke_binding() {
        let mut input = Fixture::pending();
        input.extract();
        assert!(
            !input.sources().present.contains(&REFERENCE),
            "valid pending stock root must retain its cluster candidate"
        );
        assert!(input.sources().bound.is_empty());
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x += 1.0;
        input.assert_blocked();
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(input.root_pose);
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .remove::<CellRef>();
        input.assert_blocked();
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(CellRef(0x30));
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .remove::<ChildOf>();
        input.assert_blocked();
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(ChildOf(input.cell));
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .remove::<MeshHandle>();
        input.assert_blocked();
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(MeshHandle(MODEL.into()));
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .remove::<Transform>();
        input.assert_blocked();
    }

    #[test]
    fn an_instantiated_root_cannot_become_initially_pending_by_losing_identity_or_meshes() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .remove::<FormId>();
        input.assert_blocked();
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(FormId(REFERENCE));
        input.assert_bound(primitive);
        input
            .app
            .world_mut()
            .entity_mut(primitive)
            .remove::<Mesh3d>();
        input.assert_blocked();
        input.app.world_mut().entity_mut(input.root).despawn();
        input.extract();
        assert!(!input.sources().present.contains(&REFERENCE));
    }

    #[test]
    fn an_untagged_mesh_establishes_presence_before_tagged_stock_is_ready() {
        let mut input = Fixture::pending();
        input.extract();
        assert!(!input.sources().present.contains(&REFERENCE));
        // Use an actual loaded source mesh handle, not a made-up asset id.
        let scene = input
            .app
            .world()
            .get::<WorldAssetRoot>(input.root)
            .unwrap()
            .0
            .clone();
        let mesh = {
            let mut assets = input.app.world_mut().resource_mut::<Assets<WorldAsset>>();
            let mut asset = assets.get_mut(&scene).unwrap();
            let mut meshes = asset.world.query::<&Mesh3d>();
            meshes.iter(&asset.world).next().unwrap().0.clone()
        };
        let untagged = input
            .app
            .world_mut()
            .spawn((Mesh3d(mesh), ChildOf(input.root)))
            .id();
        assert!(input.app.world().get::<SourcePrimitive>(untagged).is_none());
        input.assert_blocked();
        input.app.world_mut().entity_mut(untagged).despawn();
        // This root has already displayed stock. Removing its last mesh is a
        // runtime change, not proof that it is initially awaiting instancing.
        input.assert_blocked();
        input
            .app
            .world_mut()
            .entity_mut(input.root)
            .insert(InheritedVisibility::HIDDEN);
        input.assert_blocked();
    }

    #[test]
    fn two_and_three_duplicate_primitive_claims_never_restore_a_unique_binding() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let copied = {
            let world = input.app.world();
            (
                *world.get::<SourcePrimitive>(primitive).unwrap(),
                world.get::<SourcePrimitiveMesh>(primitive).unwrap().clone(),
                *world.get::<SourcePrimitiveTransform>(primitive).unwrap(),
                *world.get::<SourceStaticModel>(primitive).unwrap(),
                world.get::<Mesh3d>(primitive).unwrap().clone(),
                *world.get::<Transform>(primitive).unwrap(),
                *world.get::<SourceNodePose>(primitive).unwrap(),
                ChildOf(world.get::<ChildOf>(primitive).unwrap().parent()),
            )
        };
        input.app.world_mut().spawn(copied.clone());
        input.assert_blocked();
        input.app.world_mut().spawn(copied);
        input.assert_blocked();
    }

    #[test]
    fn runtime_billboard_extras_revoke_a_previously_real_bound_primitive() {
        let mut input = Fixture::pending();
        let primitive = input.spawn_stock();
        input.assert_bound(primitive);
        let parent = input
            .app
            .world()
            .get::<ChildOf>(primitive)
            .unwrap()
            .parent();
        input
            .app
            .world_mut()
            .entity_mut(parent)
            .insert(bevy::gltf::GltfExtras {
                value: r#"{"openSkyrim":{"billboard":"rotateAboutUp"}}"#.into(),
            });
        // No BillboardPlugin update runs between adding extras and extraction.
        assert!(
            input
                .app
                .world()
                .get::<crate::billboard::Billboard>(parent)
                .is_none()
        );
        input.assert_blocked();
    }
}

#[cfg(test)]
mod local_light_dependency_tests {
    use super::*;
    use bevy::{
        camera::visibility::VisibleEntities,
        light::cluster::{
            ClusterVisibilityClass, ClusterableObjects, Clusters, ObjectsInClusterCpu,
        },
    };

    fn entity(id: u32) -> Entity {
        Entity::from_raw_u32(id).unwrap()
    }
    fn cpu_cluster(light: Entity, unrelated: Entity) -> Clusters {
        let mut object = ObjectsInClusterCpu::default();
        object.add_point_light(light);
        object.add_point_light(light); // duplicate cluster occurrences deduplicate
        object.add_reflection_probe(unrelated);
        Clusters {
            clusterable_objects: ClusterableObjects::Cpu(vec![object]),
            ..default()
        }
    }

    #[test]
    fn missing_and_gpu_unknown_are_distinct_from_cpu_and_gpu_proven_empty() {
        assert_eq!(local_light_dependencies(None, None, |_| true), None);
        let mut clusters = Clusters {
            clusterable_objects: ClusterableObjects::Cpu(Vec::new()),
            ..default()
        };
        assert_eq!(
            local_light_dependencies(Some(&clusters), None, |_| true),
            Some(HashSet::new())
        );
        clusters.clusterable_objects = ClusterableObjects::Gpu;
        assert_eq!(
            local_light_dependencies(Some(&clusters), None, |_| true),
            None
        );
        let visible = VisibleEntities::default();
        assert_eq!(
            local_light_dependencies(Some(&clusters), Some(&visible), |_| true),
            Some(HashSet::new())
        );
    }

    #[test]
    fn each_cpu_camera_uses_its_own_exact_public_cluster_list() {
        let first = entity(1);
        let second = entity(2);
        let probe = entity(3);
        let first_camera = cpu_cluster(first, probe);
        let second_camera = cpu_cluster(second, probe);
        let is_light = |entity| entity == first || entity == second;
        assert_eq!(
            local_light_dependencies(Some(&first_camera), None, is_light),
            Some(HashSet::from([MainEntity::from(first)]))
        );
        assert_eq!(
            local_light_dependencies(Some(&second_camera), None, is_light),
            Some(HashSet::from([MainEntity::from(second)]))
        );
    }

    #[test]
    fn gpu_fallback_uses_only_that_cameras_actual_cluster_visibility_class() {
        let point = entity(1);
        let spot = entity(2);
        let unrelated = entity(3);
        let clusters = Clusters {
            clusterable_objects: ClusterableObjects::Gpu,
            ..default()
        };
        let mut first = VisibleEntities::default();
        first.push(point, std::any::TypeId::of::<ClusterVisibilityClass>());
        first.push(unrelated, std::any::TypeId::of::<ClusterVisibilityClass>());
        let mut second = VisibleEntities::default();
        second.push(spot, std::any::TypeId::of::<ClusterVisibilityClass>());
        let is_light = |entity| entity == point || entity == spot;
        assert_eq!(
            local_light_dependencies(Some(&clusters), Some(&first), is_light),
            Some(HashSet::from([MainEntity::from(point)]))
        );
        assert_eq!(
            local_light_dependencies(Some(&clusters), Some(&second), is_light),
            Some(HashSet::from([MainEntity::from(spot)]))
        );
    }
}

#[cfg(test)]
mod terrain_visibility_regressions {
    use super::*;
    use bevy::{
        asset::AssetPlugin, camera::visibility::VisibilityPlugin, mesh::MeshPlugin,
        transform::TransformPlugin,
    };

    fn collect(
        world: &mut World,
        identified: &HashMap<MainEntity, crate::streaming::TerrainPrimitiveIdentity>,
    ) -> HashSet<MainEntity> {
        let mut state =
            bevy::ecs::system::SystemState::<Query<(&Visibility, &InheritedVisibility)>>::new(
                world,
            );
        let visibility = state.get(world).unwrap();
        collect_terrain_visibility(identified, &visibility)
    }

    #[test]
    fn real_visibility_propagation_and_missing_components_control_raw_terrain_eligibility() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            MeshPlugin,
            TransformPlugin,
            VisibilityPlugin,
        ));
        app.finish();
        app.cleanup();
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Mesh::from(Cuboid::default()));
        let parent = app
            .world_mut()
            .spawn((Transform::IDENTITY, Visibility::Inherited))
            .id();
        let child = app
            .world_mut()
            .spawn((
                Mesh3d(mesh),
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(parent),
            ))
            .id();
        let main = MainEntity::from(child);
        let identified = HashMap::from([(
            main,
            crate::streaming::TerrainPrimitiveIdentity {
                worldspace_id: 12,
                grid: IVec2::new(7, -9),
                quadrant: 0,
                tier: Some(shared::lod::LodTier::Tier16),
            },
        )]);
        app.update();
        assert!(app.world().get::<InheritedVisibility>(child).unwrap().get());
        assert!(collect(app.world_mut(), &identified).contains(&main));
        // No camera renders this entity: ViewVisibility/frustum readiness must
        // not become a proxy for the raw source-composition observation.
        assert!(
            !app.world()
                .get::<bevy::camera::visibility::ViewVisibility>(child)
                .unwrap()
                .get()
        );
        app.world_mut()
            .entity_mut(child)
            .remove::<bevy::camera::visibility::ViewVisibility>();
        assert!(collect(app.world_mut(), &identified).contains(&main));

        app.world_mut()
            .entity_mut(parent)
            .insert(Visibility::Hidden);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(child).unwrap(),
            Visibility::Inherited
        );
        assert!(!app.world().get::<InheritedVisibility>(child).unwrap().get());
        assert!(!collect(app.world_mut(), &identified).contains(&main));
        app.world_mut()
            .entity_mut(parent)
            .insert(Visibility::Inherited);
        app.update();
        assert!(collect(app.world_mut(), &identified).contains(&main));

        app.world_mut().entity_mut(child).insert(Visibility::Hidden);
        app.update();
        assert!(!collect(app.world_mut(), &identified).contains(&main));
        app.world_mut()
            .entity_mut(child)
            .insert(Visibility::Inherited);
        app.update();
        assert!(collect(app.world_mut(), &identified).contains(&main));
        app.world_mut().entity_mut(child).remove::<Visibility>();
        assert!(!collect(app.world_mut(), &identified).contains(&main));
        app.world_mut()
            .entity_mut(child)
            .insert(Visibility::Inherited);
        app.update();
        assert!(collect(app.world_mut(), &identified).contains(&main));
        app.world_mut()
            .entity_mut(child)
            .remove::<InheritedVisibility>();
        assert!(!collect(app.world_mut(), &identified).contains(&main));
    }
}

#[cfg(test)]
mod actual_stock_layout_regressions {
    use super::*;

    struct Fixture {
        standards: Assets<StandardMaterial>,
        meshes: Assets<Mesh>,
        scenes: Assets<WorldAsset>,
        lazy: LazyBatches,
        material: bevy::asset::UntypedHandle,
        stock: Handle<Mesh>,
        source: Handle<WorldAsset>,
        state: Sources,
    }

    impl Fixture {
        fn new(generated: bool) -> Self {
            let payload = ClusterMeshAsset::from_bytes(
                include_bytes!("../../../shared/tests/data/cluster/fixture.mcl").to_vec(),
            )
            .unwrap();
            assert_eq!(payload.flags & shared::cluster::FLAG_TANGENTS, 0);
            let mut mesh = payload.source_mesh();
            if generated {
                mesh.generate_tangents().unwrap();
            }
            let mut meshes = Assets::default();
            let stock = meshes.add(mesh);
            let mut world = World::new();
            world.spawn((
                Mesh3d(stock.clone()),
                Transform::IDENTITY,
                SourcePrimitive {
                    scene_index: 0,
                    ordinal: 0,
                    mesh_index: 0,
                    primitive_index: 0,
                    matches_default_scene: true,
                },
                SourcePrimitiveMesh(stock.clone()),
                SourcePrimitiveTransform(Mat4::IDENTITY),
                SourceNodePose(Transform::IDENTITY),
                SourceStaticModel(true),
            ));
            let mut scenes = Assets::default();
            let source = scenes.add(WorldAsset::new(world));
            let lazy = LazyBatches::for_source_test("test/primitive.glb", source.clone());
            let mut standards = Assets::default();
            let material = standards.add(StandardMaterial::default()).untyped();
            Self {
                standards,
                meshes,
                scenes,
                lazy,
                material,
                stock,
                source,
                state: Sources::default(),
            }
        }

        fn row(&mut self) -> TemplateRow {
            template_row(
                0,
                &self.material,
                TemplateCaches {
                    geometry: &mut self.state.geometry_cache,
                    templates: &mut self.state.template_cache,
                    template_meshes: &mut self.state.template_meshes,
                    invalid_templates: &self.state.invalid_templates,
                    invalid_meshes: &self.state.invalid_meshes,
                    rows: &mut self.state.template_rows,
                },
                &TemplateAssets {
                    standards: &self.standards,
                    lazy: Some(&self.lazy),
                    scenes: Some(&self.scenes),
                    meshes: &self.meshes,
                    fixtures: None,
                },
            )
        }

        fn edit_material(&mut self, edit: impl FnOnce(&mut StandardMaterial)) {
            edit(
                &mut self
                    .standards
                    .get_mut(self.material.id().typed::<StandardMaterial>())
                    .unwrap(),
            );
            self.state.geometry_cache.remove(&self.material.id());
        }
    }

    #[test]
    fn generated_stock_tangents_persist_when_the_normal_map_is_removed() {
        let mut input = Fixture::new(true);
        input.edit_material(|material| material.normal_map_texture = Some(Handle::default()));
        assert_eq!(input.row().stock_tangents, Some(true));
        input.edit_material(|material| material.normal_map_texture = None);
        let row = input.row();
        assert!(row.template && !row.fallback && !row.pending);
        assert_eq!(row.stock_tangents, Some(true));
        assert!(
            input
                .meshes
                .get(&input.stock)
                .unwrap()
                .contains_attribute(Mesh::ATTRIBUTE_TANGENT)
        );
    }

    #[test]
    fn hot_normal_depth_and_anisotropy_additions_cannot_invent_stock_geometry() {
        for consumer in 0..3 {
            let mut input = Fixture::new(false);
            assert_eq!(input.row().stock_tangents, Some(false));
            input.edit_material(|material| match consumer {
                0 => material.normal_map_texture = Some(Handle::default()),
                1 => material.depth_map = Some(Handle::default()),
                _ => material.anisotropy_strength = 0.5,
            });
            let row = input.row();
            assert!(
                row.fallback && !row.template && !row.pending,
                "consumer {consumer}"
            );
            assert_eq!(row.stock_tangents, None);
            assert!(
                !input
                    .meshes
                    .get(&input.stock)
                    .unwrap()
                    .contains_attribute(Mesh::ATTRIBUTE_TANGENT)
            );
            input.edit_material(|material| *material = StandardMaterial::default());
            assert_eq!(input.row().stock_tangents, Some(false));
        }
    }

    #[test]
    fn material_only_refresh_reuses_binding_cache_and_retains_mesh_dependencies() {
        let mut input = Fixture::new(true);
        assert_eq!(input.row().stock_tangents, Some(true));
        let cached = &input.state.template_meshes.by_ordinal[&input.source.id()];
        let storage = cached as *const _;
        assert!(
            input.state.template_meshes.by_mesh[&input.stock.id()].contains(&input.source.id())
        );
        assert!(input.state.template_rows.by_scene[&input.source.id()].contains(&0));
        input.edit_material(|material| material.double_sided = true);
        assert_eq!(input.row().stock_tangents, Some(true));
        assert_eq!(
            &input.state.template_meshes.by_ordinal[&input.source.id()] as *const _,
            storage
        );
    }

    #[test]
    fn missing_mesh_is_pending_and_runtime_mutation_revokes_certification() {
        let mut input = Fixture::new(true);
        let mesh = input.meshes.remove(input.stock.id()).unwrap();
        let row = input.row();
        assert!(row.pending && !row.fallback && !row.template);
        assert_eq!(row.stock_tangents, None);
        input.meshes.insert(input.stock.id(), mesh).unwrap();
        assert_eq!(input.row().stock_tangents, Some(true));
        input.state.invalid_meshes.insert(input.stock.id());
        input.state.template_cache.clear();
        input.state.template_meshes = TemplateMeshDependencies::default();
        let row = input.row();
        assert!(row.fallback && !row.template);
        assert_eq!(row.stock_tangents, None);
    }
}

#[cfg(test)]
mod layout_dependency_regressions {
    use super::*;

    fn tag(ordinal: u32) -> SourcePrimitive {
        SourcePrimitive {
            scene_index: 0,
            ordinal,
            mesh_index: 0,
            primitive_index: 0,
            matches_default_scene: true,
        }
    }

    #[test]
    fn ordinal_bindings_never_expand_the_template_validation_query_domain() {
        let mut world = World::new();
        world.register_disabling_component::<SourcePrimitive>();
        let mesh = Handle::<Mesh>::default();
        world.spawn((Mesh3d(mesh.clone()), tag(0)));
        let mut wider = world.query::<(&SourcePrimitive, &Mesh3d)>();
        assert_eq!(wider.iter(&world).count(), 1);
        let asset = WorldAsset::new(world);
        assert!(!template_valid(&asset, &HashSet::new()));
        let scene = Handle::<WorldAsset>::default().id();
        let mut dependencies = TemplateMeshDependencies::default();
        dependencies.register(scene, &asset);
        assert!(dependencies.by_scene[&scene].is_empty());
        assert!(dependencies.by_ordinal[&scene].is_empty());
        assert!(!dependencies.by_mesh.contains_key(&mesh.id()));
    }

    #[test]
    fn duplicate_ordinals_refuse_the_layout_domain_but_keep_every_reverse_mesh_edge() {
        let mut meshes = Assets::<Mesh>::default();
        let a = meshes.add(dummy_mesh());
        let b = meshes.add(dummy_mesh());
        let mut world = World::new();
        world.spawn((Mesh3d(a.clone()), tag(0)));
        world.spawn((Mesh3d(b.clone()), tag(0)));
        world.spawn((Mesh3d(b.clone()), tag(1)));
        let asset = WorldAsset::new(world);
        let scene = Handle::<WorldAsset>::default().id();
        let mut dependencies = TemplateMeshDependencies::default();
        dependencies.register(scene, &asset);
        assert!(dependencies.by_ordinal[&scene].is_empty());
        assert_eq!(
            dependencies.by_scene[&scene],
            HashSet::from([a.id(), b.id()])
        );
        for mesh in [a.id(), b.id()] {
            assert_eq!(dependencies.by_mesh[&mesh], HashSet::from([scene]));
        }
        dependencies.remove(scene);
        assert!(dependencies.by_ordinal.is_empty());
        assert!(dependencies.by_mesh.is_empty());
    }
}
