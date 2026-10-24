// Included by partial_stock.rs's existing tests module.
mod partial_stock_readiness {
    use super::*;
    use crate::{
        cluster::{
            batch_drawability::BatchDrawability,
            flat_source::{FlatSourceOwner, FlatSourcePrimitive, FlatSourcePrimitives},
            source_identity::{
                SourceNodePose, SourcePrimitive, SourcePrimitiveMesh, SourcePrimitiveTransform,
                SourceStaticModel,
            },
        },
        snow::{SnowCoverage, SnowMaterial, SnowMaterialCache},
        streaming::{
            PendingAssetProfile, PendingModel, apply_directional_snow, converted_model_path,
            matrix_max_difference,
            tests::{add_converted_model, model_app, settle_readiness, spawn_pending_reference},
            track_asset_readiness,
        },
        world::components::{CellRef, FormId, WorldTransform},
    };
    use bevy::{
        asset::AssetApp,
        camera::visibility::VisibilityClass,
        transform::{TransformPlugin, components::TransformTreeChanged},
    };
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Resource, Default)]
    struct BeforeFlatten(
        BTreeMap<Entity, (SourcePrimitive, Handle<SnowMaterial>, GlobalTransform)>,
    );

    // Observe the real swap while hierarchy still exists. Moving the swap after
    // flattening loses its descendants and must fail this regression.
    #[allow(clippy::type_complexity)]
    fn observe_converted_draws(
        mut observed: ResMut<BeforeFlatten>,
        draws: Query<
            (
                Entity,
                &SourcePrimitive,
                &MeshMaterial3d<SnowMaterial>,
                &GlobalTransform,
                Has<Transform>,
                Has<ChildOf>,
            ),
            Without<FlatSourceOwner>,
        >,
    ) {
        for (entity, identity, material, global, local, parent) in &draws {
            assert!(local && parent, "the material swap must precede flattening");
            observed
                .0
                .entry(entity)
                .or_insert((*identity, material.0.clone(), *global));
        }
    }

    #[test]
    fn partial_readiness_converts_materials_before_flattening_without_losing_source_affine() {
        let (_directory, package, row) = fixture();
        let reference = row.form_id;
        let placed = package.placed_reference(reference).unwrap();
        let refused = placed.stock_primitives.clone().unwrap();
        let accepted: BTreeSet<_> = package
            .records()
            .filter_map(|(_, identity)| {
                (identity.reference == reference).then_some(identity.primitive)
            })
            .collect();
        assert!(!accepted.is_empty() && !refused.is_empty());
        assert!(accepted.is_disjoint(&refused));
        let ordinals: BTreeSet<_> = accepted.union(&refused).copied().collect();
        let path = converted_model_path(row.model_path.clone().unwrap()).unwrap();

        let mut app = model_app();
        app.add_plugins(TransformPlugin)
            .init_asset::<SnowMaterial>()
            .init_resource::<SnowMaterialCache>()
            .init_resource::<BeforeFlatten>()
            .register_type::<TransformTreeChanged>()
            .register_type::<Visibility>()
            .register_type::<InheritedVisibility>()
            .register_type::<ViewVisibility>()
            .register_type::<VisibilityClass>()
            .register_type::<SourceNodePose>()
            .register_type::<SourcePrimitive>()
            .register_type::<SourcePrimitiveMesh>()
            .register_type::<SourcePrimitiveTransform>()
            .register_type::<SourceStaticModel>()
            .add_systems(Update, apply_directional_snow.after(track_asset_readiness))
            .add_systems(
                Update,
                observe_converted_draws.after(apply_directional_snow),
            );
        install(&mut app);
        let mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Cuboid::new(2.0, 4.0, 6.0));
        let base_color = Color::srgb(0.4, 0.3, 0.2);
        let base = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color,
                ..default()
            });
        let parent_pose = Transform::from_scale(Vec3::new(2.0, 1.0, 0.5));
        let primitive_pose = Transform::from_xyz(0.25, 0.5, -0.75)
            .with_rotation(Quat::from_rotation_z(0.61) * Quat::from_rotation_x(0.29));
        let model_matrix = parent_pose.to_matrix() * primitive_pose.to_matrix();
        assert!(
            model_matrix
                .x_axis
                .truncate()
                .dot(model_matrix.y_axis.truncate())
                .abs()
                > 0.1,
            "the fixture must contain shear that a decomposed Transform cannot retain"
        );
        let identities: BTreeMap<_, _> = ordinals
            .iter()
            .map(|&ordinal| {
                (
                    ordinal,
                    SourcePrimitive {
                        scene_index: 0,
                        ordinal,
                        mesh_index: 0,
                        primitive_index: ordinal,
                        matches_default_scene: true,
                    },
                )
            })
            .collect();
        let mut scene = World::new();
        let parent = scene.spawn((parent_pose, SourceNodePose(parent_pose))).id();
        for identity in identities.values() {
            scene.spawn((
                primitive_pose,
                SourceNodePose(primitive_pose),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(base.clone()),
                *identity,
                SourcePrimitiveMesh(mesh.clone()),
                SourcePrimitiveTransform(model_matrix),
                SourceStaticModel(true),
                ChildOf(parent),
            ));
        }
        // Independently transform all eight cuboid corners. These are full-model
        // bounds, not a refused subset's replacement for the original contract.
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for x in [-1.0, 1.0] {
            for y in [-2.0, 2.0] {
                for z in [-3.0, 3.0] {
                    let corner = model_matrix.transform_point3(Vec3::new(x, y, z));
                    min = min.min(corner);
                    max = max.max(corner);
                }
            }
        }
        let bounds = ExpectedModelBounds::new(min, max).unwrap();
        let source = add_converted_model(&mut app, scene);
        let scope = 7;
        let full = spawn_pending_reference(&mut app, source.clone(), 0);
        let partial = spawn_pending_reference(&mut app, source.clone(), 1);
        let placement = Transform::from_xyz(17.0, 23.0, -9.0)
            .with_rotation(Quat::from_rotation_y(0.4))
            .with_scale(Vec3::splat(1.75));
        let snow = DirectionalSnow {
            static_form_id: row.base_form_id,
            coverage: SnowCoverage {
                up: Vec3::Y,
                cos_max_angle: -0.5,
                falloff_scale: 0.35,
                falloff_bias: 0.4,
                normal_dampener: 0.4,
                color: [107, 116, 126],
                single_pass: true,
            },
        };
        for root in [full, partial] {
            app.world_mut().entity_mut(root).insert((
                placement,
                GlobalTransform::from(placement),
                WorldTransform(placement.to_matrix()),
                Visibility::Inherited,
                FormId(row.form_id),
                CellRef(row.cell_id),
                bounds,
                snow,
            ));
            let mut entity = app.world_mut().entity_mut(root);
            let mut pending = entity.get_mut::<PendingAssetProfile>().unwrap();
            pending.path.clone_from(&path);
            pending.form_id = row.form_id;
            pending.base_form_id = row.base_form_id;
            pending.cell_id = row.cell_id;
            pending.base_record_type.clone_from(&row.base_record_type);
            pending.static_physics = false;
        }
        app.world_mut()
            .entity_mut(partial)
            .insert(PartialStock::candidate(&package, &row).unwrap());
        assert!(app.world().get::<PendingAssetProfile>(partial).is_some());
        assert!(app.world().get::<WorldAssetRoot>(partial).is_none());
        let metrics = settle_readiness(&mut app);
        assert_eq!(metrics.assets_ready, 2);
        assert_eq!(metrics.bounds_validated, 2);
        assert_eq!(metrics.meshes_validated, 2 * ordinals.len() as u64);
        assert_eq!(metrics.asset_load_failures, 0);
        assert_eq!(metrics.material_validation_failures, 0);
        assert_eq!(metrics.transform_bounds_validation_failures, 0);
        assert_eq!(
            app.world_mut()
                .query::<&AssetFailed>()
                .iter(app.world())
                .count(),
            0
        );
        for root in [full, partial] {
            assert!(app.world().get::<PendingAssetProfile>(root).is_none());
            assert!(app.world().get::<PendingModel>(root).is_none());
            assert!(app.world().get::<DirectionalSnow>(root).is_none());
        }
        assert_eq!(
            app.world().get::<OriginalStockScene>(partial).unwrap().0,
            source
        );
        let full_draws: BTreeMap<_, _> = app
            .world_mut()
            .query_filtered::<(
                &SourcePrimitive,
                &MeshMaterial3d<SnowMaterial>,
                &GlobalTransform,
            ), Without<FlatSourceOwner>>()
            .iter(app.world())
            .map(|(identity, material, global)| {
                (identity.ordinal, (*identity, material.0.clone(), *global))
            })
            .collect();
        assert_eq!(full_draws.len(), ordinals.len());
        let flattened: Vec<_> = app
            .world()
            .get::<FlatSourcePrimitives>(partial)
            .unwrap()
            .iter()
            .collect();
        assert_eq!(flattened.len(), ordinals.len());
        for entity in &flattened {
            let draw = app.world().entity(*entity);
            let identity = *draw.get::<SourcePrimitive>().unwrap();
            let material = &draw.get::<MeshMaterial3d<SnowMaterial>>().unwrap().0;
            let global = *draw.get::<GlobalTransform>().unwrap();
            assert_eq!(identity, identities[&identity.ordinal]);
            assert_eq!(
                (identity, material.clone(), global),
                full_draws[&identity.ordinal]
            );
            assert_eq!(
                &(identity, material.clone(), global),
                &app.world().resource::<BeforeFlatten>().0[entity],
                "flattening must retain the same already-converted draw"
            );
            assert!(draw.get::<MeshMaterial3d<StandardMaterial>>().is_none());
            assert!(draw.get::<Transform>().is_none());
            assert!(draw.get::<ChildOf>().is_none());
            assert_eq!(draw.get::<FlatSourceOwner>().unwrap().0, partial);
            assert_eq!(
                draw.get::<SourcePrimitiveTransform>().unwrap().0,
                model_matrix
            );
            let proof = draw.get::<FlatSourcePrimitive>().unwrap();
            assert_eq!(proof.model_matrix, model_matrix);
            assert_eq!(proof.expected_global, global);
            let expected = placement.to_matrix() * model_matrix;
            assert!(matrix_max_difference(global.to_matrix(), expected) < 1.0e-4);
            let converted = app
                .world()
                .resource::<Assets<SnowMaterial>>()
                .get(material)
                .unwrap();
            assert_eq!(converted.base.base_color, base_color);
            assert_eq!(
                converted.extension.axis_and_cos_max().w,
                snow.coverage.cos_max_angle
            );
        }
        assert_eq!(
            app.world().resource::<BeforeFlatten>().0.len(),
            2 * ordinals.len()
        );

        app.world_mut()
            .resource_mut::<BatchDrawability>()
            .publish(&package, scope, [(0, true)]);
        for _ in 0..8 {
            app.update();
        }
        let remaining: BTreeSet<_> = app
            .world()
            .get::<FlatSourcePrimitives>(partial)
            .unwrap()
            .iter()
            .map(|entity| app.world().get::<SourcePrimitive>(entity).unwrap().ordinal)
            .collect();
        assert_eq!(
            remaining, ordinals,
            "positive demand observations must preserve every live fallback binding"
        );
        for entity in flattened {
            let ordinal = app.world().resource::<BeforeFlatten>().0[&entity].0.ordinal;
            assert_eq!(
                app.world().get_entity(entity).is_ok(),
                ordinals.contains(&ordinal)
            );
        }
        assert_eq!(
            app.world_mut()
                .query::<&AssetFailed>()
                .iter(app.world())
                .count(),
            0
        );
        app.world_mut()
            .resource_mut::<BatchDrawability>()
            .publish(&package, scope, [(0, false)]);
        for _ in 0..8 {
            app.update();
        }
        let restored: Vec<_> = app
            .world()
            .get::<FlatSourcePrimitives>(partial)
            .unwrap()
            .iter()
            .collect();
        assert_eq!(restored.len(), ordinals.len());
        for entity in restored {
            let draw = app.world().entity(entity);
            let identity = *draw.get::<SourcePrimitive>().unwrap();
            let material = draw
                .get::<MeshMaterial3d<SnowMaterial>>()
                .unwrap()
                .0
                .clone();
            let global = *draw.get::<GlobalTransform>().unwrap();
            assert_eq!((identity, material, global), full_draws[&identity.ordinal]);
            assert!(draw.get::<Transform>().is_none());
            assert!(draw.get::<ChildOf>().is_none());
        }
        assert!(app.world().get::<AssetFailed>(partial).is_none());
        let after_drop: BTreeSet<_> = app
            .world()
            .get::<FlatSourcePrimitives>(partial)
            .unwrap()
            .iter()
            .collect();
        assert_eq!(
            after_drop,
            app.world()
                .resource::<BeforeFlatten>()
                .0
                .keys()
                .filter(|entity| app.world().get::<FlatSourceOwner>(**entity).is_some())
                .copied()
                .collect()
        );
        let counters = crate::work_counters::snapshot();
        for recorded in [&counters.before_ready, &counters.after_ready] {
            assert_eq!(
                recorded
                    .get("far_stock/fallback_respawn_after_drop")
                    .copied()
                    .unwrap_or(0),
                0
            );
        }
    }
}
