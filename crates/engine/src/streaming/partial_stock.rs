//! Render-only distant stock with resident complementary fallback draws.

use super::{AssetFailed, PendingAssetProfile, PendingModel};
use crate::{
    cluster::{batch_drawability::BatchDrawability, reader::CertifiedPackage},
    snow::DirectionalSnow,
    world::{
        components::{ExpectedModelBounds, InstanceBounds},
        database::ReferenceRow,
    },
};
use bevy::{
    asset::{AssetId, LoadState, RecursiveDependencyLoadState},
    camera::primitives::MeshAabb,
    prelude::*,
    world_serialization::WorldAssetRoot,
};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

#[derive(Component)]
pub(super) struct PartialStock {
    package: Arc<CertifiedPackage>,
    row: ReferenceRow,
    refused: BTreeSet<u32>,
    pub(super) source: Option<Handle<WorldAsset>>,
    original_bounds: Option<ExpectedModelBounds>,
    snow: Option<DirectionalSnow>,
}

pub(crate) use crate::cluster::flat_source::OriginalStockScene;

#[derive(Clone)]
struct SelectedTemplate {
    scene: Handle<WorldAsset>,
    bounds: ExpectedModelBounds,
}

impl PartialStock {
    pub(super) fn for_demand(
        package: &Arc<CertifiedPackage>,
        row: &ReferenceRow,
        owned_mode: bool,
        beyond_grid: bool,
    ) -> Option<Self> {
        (owned_mode && beyond_grid)
            .then(|| Self::candidate(package, row))
            .flatten()
    }

    pub(super) fn candidate(package: &Arc<CertifiedPackage>, row: &ReferenceRow) -> Option<Self> {
        let placed = package.placed_reference(row.form_id)?;
        // Mapping membership proves at least one accepted primitive. A wholly
        // refused expanded source must keep ordinary whole-reference spawning.
        let refused = placed
            .stock_primitives
            .as_ref()
            .filter(|set| !set.is_empty())?;
        package.reference(row.form_id)?;
        Some(Self {
            package: package.clone(),
            row: row.clone(),
            refused: refused.clone(),
            source: None,
            original_bounds: None,
            snow: None,
        })
    }
}

#[derive(Resource, Default)]
struct Templates {
    selected: HashMap<(AssetId<WorldAsset>, Vec<u32>), SelectedTemplate>,
}

#[derive(Resource, Default)]
struct Counters {
    partial_spawns: u64,
    refused_primitives_spawned: u64,
    full_spawns_avoided: u64,
    startup_logged: bool,
}

pub(super) fn install(app: &mut App) {
    super::flat_stock::install(app);
    app.init_resource::<BatchDrawability>()
        .init_resource::<Templates>()
        .init_resource::<Counters>()
        .add_systems(
            Update,
            (source_changes, resting_revocations)
                .chain()
                .before(super::arm_pending_models)
                .after(super::far_stock::collect),
        )
        .add_systems(Update, log_inventory.after(super::far_stock::publish_work));
}

fn log_inventory(mut counters: ResMut<Counters>) {
    if crate::work_counters::world_ready() && !counters.startup_logged {
        info!(
            distant_partial_spawns = counters.partial_spawns,
            refused_primitives_spawned = counters.refused_primitives_spawned,
            full_spawns_avoided = counters.full_spawns_avoided,
            "distant flat stock spawn inventory at world ready; owned fallback draws stay resident"
        );
        counters.startup_logged = true;
    }
}

fn template(
    world: &mut World,
    source: &Handle<WorldAsset>,
    ordinals: &BTreeSet<u32>,
) -> Result<SelectedTemplate, String> {
    let key = (source.id(), ordinals.iter().copied().collect::<Vec<_>>());
    if let Some(handle) = world.resource::<Templates>().selected.get(&key) {
        return Ok(handle.clone());
    }
    let registry = world.resource::<AppTypeRegistry>().clone();
    let selected = {
        let assets = world.resource::<Assets<WorldAsset>>();
        let source = assets
            .get(source)
            .ok_or("partial stock source scene disappeared")?;
        super::partial_stock_template::selected_scene(source, ordinals, &registry)?
    };
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    let meshes = world.resource::<Assets<Mesh>>();
    for entity in selected.world.iter_entities() {
        let Some(mesh) = entity.get::<Mesh3d>() else {
            continue;
        };
        let model = entity
            .get::<crate::cluster::source_identity::SourcePrimitiveTransform>()
            .ok_or("selected stock primitive has no model transform")?
            .0;
        let aabb = meshes
            .get(&mesh.0)
            .and_then(Mesh::compute_aabb)
            .ok_or("selected stock primitive has no loaded finite mesh bounds")?;
        let center = Vec3::from(aabb.center);
        let extent = Vec3::from(aabb.half_extents);
        let bounds = InstanceBounds::transformed(center - extent, center + extent, model);
        min = min.min(bounds.min);
        max = max.max(bounds.max);
    }
    let bounds = ExpectedModelBounds::new(min, max)
        .ok_or("selected stock scene has no finite nonempty bounds")?;
    let scene = world.resource_mut::<Assets<WorldAsset>>().add(selected);
    let template = SelectedTemplate { scene, bounds };
    world
        .resource_mut::<Templates>()
        .selected
        .insert(key, template.clone());
    Ok(template)
}

fn validate_source_bounds(
    expected: ExpectedModelBounds,
    actual: ExpectedModelBounds,
) -> Result<(), String> {
    ExpectedModelBounds::new(expected.min, expected.max)
        .ok_or("partial stock source bounds are non-finite, empty, or inverted")?;
    // Keep the ordinary full-scene validation tolerance even when ready bodies
    // need not be instantiated. Derived subset bounds cannot replace that proof.
    let extent = (expected.max - expected.min).abs().max_element().max(1.0);
    let tolerance = (extent * 1.0e-4).max(1.0e-3);
    let error = (actual.min - expected.min)
        .abs()
        .max((actual.max - expected.max).abs())
        .max_element();
    if !error.is_finite() || error > tolerance {
        return Err("partial stock source bounds diverge from conversion".to_owned());
    }
    Ok(())
}

/// Called at the ordinary scene-arming budget boundary, after its unchanged ownership gate.
pub(super) fn arm(world: &mut World, root: Entity, source: Handle<WorldAsset>) {
    let failed = world
        .get_resource::<AssetServer>()
        .and_then(|server| server.get_load_states(source.id()))
        .is_some_and(|(load, _, recursive)| {
            matches!(load, LoadState::Failed(_))
                || matches!(recursive, RecursiveDependencyLoadState::Failed(_))
        });
    let deforming = world
        .get_resource::<Assets<WorldAsset>>()
        .and_then(|assets| assets.get(&source))
        .is_some_and(|scene| {
            scene.world.iter_entities().any(|entity| {
                entity.contains::<bevy::mesh::skinning::SkinnedMesh>()
                    || entity.contains::<bevy::mesh::morph::MorphWeights>()
                    || entity.contains::<bevy::mesh::morph::MeshMorphWeights>()
            })
        });
    let full_fallback = failed
        || deforming
        || world.get::<PartialStock>(root).is_some_and(|partial| {
            world
                .get_resource::<crate::cluster::source_fallback::SourceFallbackDemand>()
                .and_then(|demand| demand.for_package(&partial.package))
                .is_some_and(|(_, references)| references.contains(&partial.row.form_id))
                || world
                    .get_resource::<crate::cluster::runtime_state::RuntimeReferenceState>()
                    .is_some_and(|state| state.contains(partial.row.form_id))
        });
    if full_fallback {
        debug!(
            ?root,
            failed, deforming, "distant partial reference retains ordinary stock handling"
        );
        world
            .entity_mut(root)
            .remove::<PartialStock>()
            .insert(WorldAssetRoot(source))
            .remove::<PendingModel>();
        return;
    }
    if let Err(reason) = arm_inner(world, root, source) {
        error!(?root, %reason, "distant partial stock instancing failed");
        world
            .entity_mut(root)
            .despawn_children()
            .insert((AssetFailed, Visibility::Hidden))
            .remove::<(PendingModel, PendingAssetProfile, PartialStock)>();
    }
}

fn arm_inner(world: &mut World, root: Entity, source: Handle<WorldAsset>) -> Result<(), String> {
    let original_bounds = *world
        .get::<ExpectedModelBounds>(root)
        .ok_or("partial stock source has no expected model bounds")?;
    // Keep every draw, including unknown new ordinals after a source edit. The
    // complementary renderer requires the exact live owned-body binding even
    // while its cluster is ready. A positive observation is never a lifetime lease.
    let ordinals: Result<BTreeSet<_>, _> = world
        .resource::<Assets<WorldAsset>>()
        .get(&source)
        .ok_or("partial stock source disappeared")?
        .world
        .iter_entities()
        .filter(|entity| entity.contains::<Mesh3d>())
        .map(|entity| {
            entity
                .get::<crate::cluster::source_identity::SourcePrimitive>()
                .map(|tag| tag.ordinal)
                .ok_or("partial stock source mesh has no ordinal")
        })
        .collect();
    let selected = template(world, &source, &ordinals?)?;
    validate_source_bounds(original_bounds, selected.bounds)?;
    let snow = world.get::<DirectionalSnow>(root).copied();
    let refused_count = {
        let mut partial = world
            .get_mut::<PartialStock>(root)
            .ok_or("partial stock root disappeared")?;
        partial.source = Some(source.clone());
        partial.original_bounds = Some(original_bounds);
        partial.snow = snow;
        partial.refused.len() as u64
    };
    // The ordinary gate already qualified this reference. Keep its draws
    // extract-eligible for #2136's per-consumer complement, including readiness
    // withdrawal. Restore ordinary ownership handling on loaded-grid promotion.
    world
        .entity_mut(root)
        .insert((WorldAssetRoot(selected.scene), OriginalStockScene(source)))
        .remove::<(PendingModel, crate::cluster::ownership::OwnershipReference)>();
    let mut counters = world.resource_mut::<Counters>();
    counters.partial_spawns += 1;
    counters.refused_primitives_spawned += refused_count;
    counters.full_spawns_avoided += 1;
    if world.get::<PendingAssetProfile>(root).is_none() {
        super::flat_stock::queue(world, root);
    }
    Ok(())
}

/// Restore the ordinary complete scene before adopting this reference into the loaded grid.
pub(super) fn promote(world: &mut World, root: Entity) {
    let Some(partial) = world.entity_mut(root).take::<PartialStock>() else {
        return;
    };
    let Some(source) = partial.source else {
        return;
    };
    let flat = world
        .get::<crate::cluster::flat_source::FlatSourcePrimitives>(root)
        .map(|members| members.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    for entity in flat {
        world.despawn(entity);
    }
    world.entity_mut(root).despawn_children();
    let row = partial.row;
    if let Some(snow) = partial.snow {
        world.entity_mut(root).insert(snow);
    }
    if let Some(bounds) = partial.original_bounds {
        if let Some(pose) = world.get::<Transform>(root).copied() {
            world.entity_mut(root).insert(InstanceBounds::transformed(
                bounds.min,
                bounds.max,
                pose.to_matrix(),
            ));
        }
        world.entity_mut(root).insert(bounds);
    }
    world
        .entity_mut(root)
        .remove::<OriginalStockScene>()
        .insert((
            WorldAssetRoot(source),
            PendingAssetProfile {
                started: std::time::Instant::now(),
                scene_spawned: false,
                path: row
                    .model_path
                    .and_then(super::converted_model_path)
                    .unwrap_or_default(),
                form_id: row.form_id,
                base_form_id: row.base_form_id,
                base_record_type: row.base_record_type,
                static_physics: false,
                cell_id: row.cell_id,
            },
            crate::cluster::ownership::OwnershipReference(row.form_id),
        ));
}

fn source_changes(
    mut events: MessageReader<AssetEvent<WorldAsset>>,
    partials: Query<(Entity, &PartialStock, Option<&PendingModel>)>,
    mut commands: Commands,
) {
    let changed: HashSet<_> = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(*id),
            _ => None,
        })
        .collect();
    if changed.is_empty() {
        return;
    }
    for (root, partial, pending) in &partials {
        if partial
            .source
            .as_ref()
            .or_else(|| pending.map(|pending| &pending.handle))
            .is_some_and(|source| changed.contains(&source.id()))
        {
            // A source edit invalidates every derived template. Return to the full
            // ordinary source until a new certificate establishes selective ownership.
            commands.queue(move |world: &mut World| promote(world, root));
        }
    }
    commands.queue(move |world: &mut World| {
        world
            .resource_mut::<Templates>()
            .selected
            .retain(|(source, _), _| !changed.contains(source))
    });
}

fn resting_revocations(
    state: Option<Res<crate::cluster::runtime_state::RuntimeReferenceState>>,
    mut previous: Local<u64>,
    partials: Query<(Entity, &PartialStock)>,
    mut commands: Commands,
) {
    let Some(state) = state else {
        return;
    };
    if *previous == state.revision {
        return;
    }
    *previous = state.revision;
    for (root, partial) in &partials {
        if state.contains(partial.row.form_id) {
            commands.queue(move |world: &mut World| promote(world, root));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("partial_stock_readiness_tests.rs");
    include!("partial_stock_load_failure_tests.rs");
    use crate::{
        cluster::reader::producer_fixture_tests::published_for_inventory,
        world::database::{CellKey, WorldDatabase},
    };
    fn fixture() -> (tempfile::TempDir, Arc<CertifiedPackage>, ReferenceRow) {
        let directory = published_for_inventory();
        let package = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        let reference = package
            .records()
            .map(|(_, identity)| identity.reference)
            .find(|id| {
                package.placed_reference(*id).is_some_and(|placed| {
                    placed
                        .stock_primitives
                        .as_ref()
                        .is_some_and(|p| !p.is_empty())
                })
            })
            .unwrap();
        let snapshot = &package.placed_reference(reference).unwrap().snapshot;
        let key = CellKey::Exterior {
            worldspace_id: snapshot.placed_worldspace_id,
            grid_x: snapshot.cell.0,
            grid_y: snapshot.cell.1,
        };
        let database = WorldDatabase::open(&directory.path().join("skyrim_world.db")).unwrap();
        let row = database
            .request_references(1, key, BTreeSet::from([reference]))
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap()
            .result
            .unwrap()
            .pop()
            .unwrap();
        (directory, package, row)
    }

    #[test]
    fn in_grid_and_baked_demands_keep_full_spawning() {
        let (_directory, package, row) = fixture();
        assert!(PartialStock::for_demand(&package, &row, true, true).is_some());
        assert!(PartialStock::for_demand(&package, &row, true, false).is_none());
        assert!(PartialStock::for_demand(&package, &row, false, true).is_none());
    }

    #[test]
    fn completely_refused_reference_keeps_full_spawning() {
        let (directory, package, _) = fixture();
        let (grid, ids) = package
            .placed_cells_in_window(0x70, IVec2::ZERO, None)
            .find(|(_, ids)| {
                ids.iter().any(|id| {
                    package
                        .placed_reference(*id)
                        .is_some_and(|p| p.model_path.is_some() && package.reference(*id).is_none())
                })
            })
            .unwrap();
        let reference = *ids
            .iter()
            .find(|id| {
                package
                    .placed_reference(**id)
                    .is_some_and(|p| p.model_path.is_some() && package.reference(**id).is_none())
            })
            .unwrap();
        let database = WorldDatabase::open(&directory.path().join("skyrim_world.db")).unwrap();
        let row = database
            .request_references(
                1,
                CellKey::Exterior {
                    worldspace_id: 0x70,
                    grid_x: grid.x,
                    grid_y: grid.y,
                },
                BTreeSet::from([reference]),
            )
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap()
            .result
            .unwrap()
            .pop()
            .unwrap();
        assert!(PartialStock::for_demand(&package, &row, true, true).is_none());
    }

    #[test]
    fn real_scene_instancing_keeps_all_draws_and_promotion_restores_full_source() {
        use crate::{
            cluster::source_identity::{SourcePrimitive, SourcePrimitiveTransform},
            world::components::WorldTransform,
        };
        use bevy::{
            asset::AssetPlugin,
            camera::visibility::VisibilityClass,
            transform::{TransformPlugin, components::TransformTreeChanged},
            world_serialization::WorldSerializationPlugin,
        };
        let (_directory, package, row) = fixture();
        let refused = package
            .placed_reference(row.form_id)
            .unwrap()
            .stock_primitives
            .clone()
            .unwrap();
        let owned: BTreeSet<_> = package
            .records()
            .filter_map(|(_, identity)| {
                (identity.reference == row.form_id).then_some(identity.primitive)
            })
            .collect();
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            TransformPlugin,
            WorldSerializationPlugin,
        ))
        .init_asset::<Mesh>()
        .register_type::<Name>()
        .register_type::<Transform>()
        .register_type::<GlobalTransform>()
        .register_type::<TransformTreeChanged>()
        .register_type::<ChildOf>()
        .register_type::<Children>()
        .register_type::<Visibility>()
        .register_type::<InheritedVisibility>()
        .register_type::<ViewVisibility>()
        .register_type::<VisibilityClass>()
        .register_type::<Mesh3d>()
        .register_type::<MeshMaterial3d<StandardMaterial>>()
        .register_type::<SourcePrimitive>()
        .register_type::<SourcePrimitiveTransform>()
        .init_resource::<Templates>()
        .init_resource::<Counters>()
        .init_resource::<BatchDrawability>();
        super::super::flat_stock::install(&mut app);
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 2.0, 2.0));
        let material = Handle::<StandardMaterial>::default();
        let mut source = World::new();
        for ordinal in owned.union(&refused) {
            source.spawn((
                Transform::IDENTITY,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                SourcePrimitive {
                    scene_index: 0,
                    ordinal: *ordinal,
                    mesh_index: 0,
                    primitive_index: *ordinal,
                    matches_default_scene: true,
                },
                SourcePrimitiveTransform(Mat4::IDENTITY),
            ));
        }
        let source = app
            .world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .add(WorldAsset::new(source));
        // A post-certification interior mesh must remain stock even when its
        // addition changes neither placement nor the original aggregate bounds.
        {
            let mut assets = app.world_mut().resource_mut::<Assets<WorldAsset>>();
            let mut scene = assets.get_mut(&source).unwrap();
            scene.world.spawn((
                Transform::IDENTITY,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                SourcePrimitive {
                    scene_index: 0,
                    ordinal: u32::MAX,
                    mesh_index: 0,
                    primitive_index: 0,
                    matches_default_scene: true,
                },
                SourcePrimitiveTransform(Mat4::IDENTITY),
            ));
        }
        let pose = Transform::from_xyz(17.0, 23.0, -9.0)
            .with_rotation(Quat::from_rotation_y(0.4))
            .with_scale(Vec3::splat(1.75));
        let root = app
            .world_mut()
            .spawn((
                pose,
                WorldTransform(pose.to_matrix()),
                Visibility::Inherited,
                PartialStock::candidate(&package, &row).unwrap(),
                crate::cluster::ownership::OwnershipReference(row.form_id),
                ExpectedModelBounds::new(Vec3::splat(-1.0), Vec3::ONE).unwrap(),
            ))
            .id();
        arm_inner(app.world_mut(), root, source.clone()).unwrap();
        assert!(
            app.world()
                .get::<crate::cluster::ownership::OwnershipReference>(root)
                .is_none()
        );
        app.update();
        let draws: Vec<_> = app
            .world_mut()
            .query::<(
                &SourcePrimitive,
                &MeshMaterial3d<StandardMaterial>,
                &GlobalTransform,
            )>()
            .iter(app.world())
            .map(|(tag, material, global)| (tag.ordinal, material.0.clone(), global.to_matrix()))
            .collect();
        assert_eq!(draws.len(), owned.union(&refused).count() + 1);
        for (ordinal, actual_material, matrix) in draws {
            assert!(refused.contains(&ordinal) || owned.contains(&ordinal) || ordinal == u32::MAX);
            assert_eq!(actual_material, material);
            assert_eq!(matrix, pose.to_matrix());
        }
        for entity in app
            .world()
            .get::<crate::cluster::flat_source::FlatSourcePrimitives>(root)
            .unwrap()
            .iter()
        {
            assert!(app.world().get::<Transform>(entity).is_none());
            assert!(app.world().get::<ChildOf>(entity).is_none());
        }
        promote(app.world_mut(), root);
        app.update();
        assert!(app.world().get::<PartialStock>(root).is_none());
        assert_eq!(
            app.world()
                .get::<crate::cluster::ownership::OwnershipReference>(root)
                .unwrap()
                .0,
            row.form_id
        );
        assert_eq!(app.world().get::<WorldAssetRoot>(root).unwrap().0, source);
        let expected_count = package
            .records()
            .filter(|(_, identity)| identity.reference == row.form_id)
            .count()
            + refused.len()
            + 1;
        assert_eq!(
            app.world_mut().query::<&Mesh3d>().iter(app.world()).count(),
            expected_count
        );
    }

    #[test]
    fn subset_bounds_cannot_mask_invalid_original_conversion_bounds() {
        let bounds = ExpectedModelBounds::new(Vec3::splat(-1.0), Vec3::ONE).unwrap();
        assert!(validate_source_bounds(bounds, bounds).is_ok());
        let wrong = ExpectedModelBounds::new(Vec3::ZERO, Vec3::ONE).unwrap();
        assert!(validate_source_bounds(wrong, bounds).is_err());
    }

    #[test]
    fn missing_original_bounds_fails_before_deriving_subset_assets() {
        let (_directory, package, row) = fixture();
        let mut world = World::new();
        let root = world
            .spawn(PartialStock::candidate(&package, &row).unwrap())
            .id();
        assert_eq!(
            arm_inner(&mut world, root, Handle::default()).unwrap_err(),
            "partial stock source has no expected model bounds"
        );
    }

    #[test]
    fn skin_and_morph_sources_keep_their_ordinary_dependency_graphs() {
        use bevy::{
            asset::AssetPlugin,
            mesh::{
                morph::{MeshMorphWeights, MorphWeights},
                skinning::SkinnedMesh,
            },
            transform::components::TransformTreeChanged,
            world_serialization::WorldSerializationPlugin,
        };
        let (_directory, package, row) = fixture();
        for morph in [false, true] {
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                AssetPlugin::default(),
                WorldSerializationPlugin,
            ))
            .register_type::<Transform>()
            .register_type::<GlobalTransform>()
            .register_type::<TransformTreeChanged>()
            .register_type::<ChildOf>()
            .register_type::<Children>()
            .register_type::<Mesh3d>()
            .register_type::<SkinnedMesh>()
            .register_type::<MorphWeights>()
            .register_type::<MeshMorphWeights>();
            let mut scene = World::new();
            let support = scene.spawn(Transform::from_xyz(1.0, 2.0, 3.0)).id();
            let draw = scene.spawn(Mesh3d::default()).id();
            if morph {
                scene
                    .entity_mut(support)
                    .insert(MorphWeights::new(vec![0.5], None).unwrap());
                scene
                    .entity_mut(draw)
                    .insert((ChildOf(support), MeshMorphWeights::Reference(support)));
            } else {
                // The joint is intentionally not an ancestor of the mesh.
                scene.entity_mut(draw).insert(SkinnedMesh {
                    inverse_bindposes: Handle::default(),
                    joints: vec![support],
                });
            }
            let source = app
                .world_mut()
                .resource_mut::<Assets<WorldAsset>>()
                .add(WorldAsset::new(scene));
            let root = app
                .world_mut()
                .spawn((
                    Transform::IDENTITY,
                    PartialStock::candidate(&package, &row).unwrap(),
                ))
                .id();
            arm(app.world_mut(), root, source.clone());
            assert!(app.world().get::<PartialStock>(root).is_none());
            assert_eq!(app.world().get::<WorldAssetRoot>(root).unwrap().0, source);
            app.update();
            if morph {
                let weights = app
                    .world_mut()
                    .query::<&MeshMorphWeights>()
                    .single(app.world())
                    .unwrap();
                let MeshMorphWeights::Reference(parent) = weights else {
                    panic!("lost morph reference")
                };
                assert_eq!(
                    app.world().get::<MorphWeights>(*parent).unwrap().weights(),
                    &[0.5]
                );
            } else {
                let skin = app
                    .world_mut()
                    .query::<&SkinnedMesh>()
                    .single(app.world())
                    .unwrap();
                assert_eq!(skin.joints.len(), 1);
                assert_eq!(
                    app.world()
                        .get::<Transform>(skin.joints[0])
                        .unwrap()
                        .translation,
                    Vec3::new(1.0, 2.0, 3.0)
                );
            }
        }
    }

    #[test]
    fn pending_source_edit_returns_to_ordinary_full_spawning() {
        let (_directory, package, row) = fixture();
        let mut app = App::new();
        app.init_resource::<Templates>()
            .add_message::<AssetEvent<WorldAsset>>()
            .add_systems(Update, source_changes);
        let handle = Handle::<WorldAsset>::default();
        let root = app
            .world_mut()
            .spawn((
                PartialStock::candidate(&package, &row).unwrap(),
                PendingModel {
                    handle: handle.clone(),
                    sequence: 0,
                },
            ))
            .id();
        app.world_mut()
            .write_message(AssetEvent::<WorldAsset>::Modified { id: handle.id() });
        app.update();
        assert!(app.world().get::<PartialStock>(root).is_none());
        assert_eq!(
            app.world().get::<PendingModel>(root).unwrap().handle,
            handle
        );
        assert!(app.world().get::<WorldAssetRoot>(root).is_none());
    }

    #[test]
    fn batch_observations_are_scoped_changed_only_demand() {
        let (_directory, package, _) = fixture();
        let mut facts = BatchDrawability::default();
        assert!(!facts.drawable(&package, 7, 0));
        facts.publish(&package, 7, [(0, true)]);
        assert!(facts.drawable(&package, 7, 0));
        assert!(!facts.drawable(&package, 8, 0));
        assert!(!facts.drawable(&package, 7, 1));
        assert_eq!(facts.take_changes(), (true, BTreeSet::from([0])));
        facts.publish(&package, 7, [(0, true)]);
        assert_eq!(facts.take_changes(), (false, BTreeSet::new()));
        facts.publish(&package, 8, []);
        assert!(!facts.drawable(&package, 8, 0));
        assert_eq!(facts.take_changes(), (true, BTreeSet::new()));
    }

    #[test]
    fn definitive_runtime_fallback_uses_ordinary_full_scene() {
        let (_directory, package, row) = fixture();
        let mut world = World::new();
        world.init_resource::<crate::cluster::source_fallback::SourceFallbackDemand>();
        world
            .resource_mut::<crate::cluster::source_fallback::SourceFallbackDemand>()
            .update(Some(&package), BTreeSet::from([row.form_id]));
        let root = world
            .spawn(PartialStock::candidate(&package, &row).unwrap())
            .id();
        let source = Handle::<WorldAsset>::default();
        arm(&mut world, root, source.clone());
        assert!(world.get::<PartialStock>(root).is_none());
        assert_eq!(world.get::<WorldAssetRoot>(root).unwrap().0, source);
    }
}
