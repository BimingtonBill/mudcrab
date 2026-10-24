//! Changed-only loading proof over every applicable cluster consumer.

use super::*;
use crate::cluster::loading::{ClusterLoadingGate, ClusterWorldReadiness};
use bevy::ecs::change_detection::Tick;
use bevy::pbr::{ExtractedPointLight, PointAndSpotLightViewEntities, ViewLightEntities};
use bevy::render::MainWorld;

// UI creates an ExtractedView sharing its camera's MainEntity, but no world
// geometry role. Identify roles independently of their phases/GPU proof: missing
// required Camera3d/light proof must remain a blocking consumer.
type LoadingViews<'w, 's> =
    Query<'w, 's, SelectionViewState, Or<(With<Camera3d>, With<LightEntity>)>>;

#[derive(Clone)]
struct Consumer {
    key: RetainedViewEntity,
    world: u32,
    shadow: bool,
    layers: RenderLayers,
    readiness: Option<Arc<Vec<bool>>>,
}

impl Consumer {
    fn same(&self, other: &Self) -> bool {
        self.key == other.key
            && self.world == other.world
            && self.shadow == other.shadow
            && self.layers == other.layers
            && match (&self.readiness, &other.readiness) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

#[derive(Clone, Copy)]
struct NeededBatch {
    batch: usize,
    terrain: bool,
    casts_shadows: bool,
    invalid_owned_state: bool,
}

/// Inventory inputs only. Loading generations and source qualification belong to the aggregate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NeededCatalogKey {
    gpu_identity: u64,
    gpu_generation: u64,
    gpu_layout: u64,
    scene_changed: Tick,
    materials_revision: u64,
    refusal_revision: Option<u64>,
    layout_scope: u64,
    runtime_state_revision: Option<u64>,
}

struct Snapshot {
    package: Arc<super::super::super::reader::CertifiedPackage>,
    layout: u64,
    generation: u64,
    frame: u64,
    worlds: std::collections::HashMap<u32, ClusterWorldReadiness>,
}

#[derive(Resource, Default)]
struct Producer {
    generation: u64,
    revision: Option<[u64; 9]>,
    needed_key: Option<NeededCatalogKey>,
    needed_catalog: HashMap<u32, Vec<NeededBatch>>,
    consumers: Vec<Consumer>,
    frame: u64,
    pending: Option<Snapshot>,
    withdrawal_pending: bool,
    #[cfg(test)]
    walks: usize,
}

impl Producer {
    fn observe(
        &mut self,
        needed_key: NeededCatalogKey,
        revision: [u64; 9],
        consumers: Vec<Consumer>,
        needed: impl FnOnce() -> HashMap<u32, Vec<NeededBatch>>,
        batch_ready: impl Fn(usize) -> (bool, bool, bool),
    ) -> Option<std::collections::HashMap<u32, ClusterWorldReadiness>> {
        let catalog_changed = self.needed_key != Some(needed_key);
        if !catalog_changed
            && self.revision == Some(revision)
            && self.consumers.len() == consumers.len()
            && self
                .consumers
                .iter()
                .zip(&consumers)
                .all(|(old, new)| old.same(new))
        {
            return None;
        }
        crate::work_counters::record("cluster_drawability/non_replay");
        #[cfg(test)]
        {
            self.walks += 1;
        }
        if catalog_changed {
            self.needed_catalog = needed();
            self.needed_key = Some(needed_key);
            crate::work_counters::record("cluster_drawability/needed_catalog");
        }
        let mut worlds = std::collections::HashMap::new();
        for consumer in &consumers {
            let world = worlds
                .entry(consumer.world)
                .or_insert_with(ClusterWorldReadiness::default);
            world.applicable_cameras += usize::from(!consumer.shadow);
            if consumer.readiness.is_none() {
                world.missing_views += 1;
            }
            for need in self
                .needed_catalog
                .get(&consumer.world)
                .into_iter()
                .flatten()
            {
                let domain = ViewDomain::new(&consumer.layers, consumer.shadow);
                if !(if need.terrain {
                    domain.terrain
                } else {
                    domain.objects
                }) || (consumer.shadow && !need.casts_shadows)
                {
                    continue;
                }
                world.needed_batches += 1;
                let (compute, template, failed) = batch_ready(need.batch);
                world.pending_compute += usize::from(!compute);
                world.pending_templates += usize::from(!need.terrain && !template);
                world.failed_batches += usize::from(failed || need.invalid_owned_state);
                world.pending_pipelines += usize::from(
                    !consumer
                        .readiness
                        .as_ref()
                        .is_some_and(|ready| ready.get(need.batch) == Some(&true)),
                );
            }
        }
        self.revision = Some(revision);
        self.consumers = consumers;
        Some(worlds)
    }
}

fn complete_camera_consumers(consumers: &mut Vec<Consumer>, expected: &HashMap<MainEntity, u32>) {
    for (&main, &world) in expected {
        if !consumers
            .iter()
            .any(|consumer| !consumer.shadow && consumer.key.main_entity == main)
        {
            consumers.push(Consumer {
                key: RetainedViewEntity::new(main, None, 0),
                world,
                shadow: false,
                layers: crate::render::reflectable_world_layers(),
                readiness: None,
            });
        }
    }
}

/// View existence alone is not a world-scoped drawing proof. Missing or foreign
/// consumers remain explicit dependencies until selection can draw this world.
fn require_shadow_consumer(
    consumers: &mut Vec<Consumer>,
    key: RetainedViewEntity,
    world: u32,
    layers: &RenderLayers,
) {
    if !consumers
        .iter()
        .any(|consumer| consumer.shadow && consumer.key == key && consumer.world == world)
    {
        consumers.push(Consumer {
            key,
            world,
            shadow: true,
            layers: layers.clone(),
            readiness: None,
        });
    }
}

fn extract_generation(
    mut producer: ResMut<Producer>,
    gate: Extract<Option<Res<ClusterLoadingGate>>>,
) {
    producer.generation = gate.as_ref().map_or(0, |gate| gate.generation());
}

#[allow(clippy::too_many_arguments)]
fn observe(
    gpu: Option<Res<ClusterGpu>>,
    scene: Option<Res<ClusterScene>>,
    sources: Res<Sources>,
    frame: Res<ClusterFrame>,
    pipelines: Res<ClusterPipelines>,
    cache: Res<PipelineCache>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    runtime_state: Option<Res<crate::cluster::runtime_state::RuntimeReferenceState>>,
    views: LoadingViews,
    camera_lights: Query<&ViewLightEntities>,
    local_lights: Query<(
        &MainEntity,
        &ExtractedPointLight,
        &PointAndSpotLightViewEntities,
    )>,
    mut producer: ResMut<Producer>,
) {
    let (Some(gpu), Some(scene)) = (gpu, scene) else {
        producer.withdrawal_pending |= producer.revision.take().is_some();
        producer.pending = None;
        return;
    };
    let (Some(package), Some(layout)) = (gpu.certificate.as_ref(), sources.lazy_layout_scope)
    else {
        producer.withdrawal_pending |= producer.revision.take().is_some();
        producer.pending = None;
        return;
    };
    let same = sources
        .certificate
        .as_ref()
        .is_some_and(|source| Arc::ptr_eq(source, package));
    let compute = same && prepared_cluster_cull(&gpu, &frame, &pipelines, &cache).is_some();
    let mut consumers: Vec<_> = views
        .iter()
        .filter_map(|(_, view, light, layers)| {
            let key = view.retained_view_entity;
            let world = sources
                .worlds
                .get(&key.main_entity)
                .or_else(|| sources.worlds.get(&key.auxiliary_entity))
                .copied()
                .flatten()?;
            Some(Consumer {
                key,
                world,
                shadow: light.is_some(),
                layers: layers.cloned().unwrap_or_default(),
                readiness: gpu
                    .views
                    .get(&key)
                    .and_then(|view| view.readiness_cache.as_ref())
                    .map(|(_, ready)| ready.clone()),
            })
        })
        .collect();
    // Enumerate expected shadow views independently of the views that arrived.
    // Omitting a missing cascade/point face must never make the proof vacuously ready.
    for (entity, camera, light, layers) in &views {
        if light.is_some() {
            continue;
        }
        let Some(world) = sources
            .worlds
            .get(&camera.retained_view_entity.main_entity)
            .copied()
            .flatten()
        else {
            continue;
        };
        let layers = layers.cloned().unwrap_or_default();
        // Bevy owns the selected/capped/atlas-allocated cascade set. Its actual
        // required view list is authoritative, not the raw configured cascade count.
        match camera_lights.get(entity) {
            Ok(required) => {
                for (index, shadow_view) in required.lights.iter().enumerate() {
                    if views.get(*shadow_view).is_err() {
                        consumers.push(Consumer {
                            key: RetainedViewEntity::new(
                                camera.retained_view_entity.main_entity,
                                Some(camera.retained_view_entity.main_entity),
                                index as u32,
                            ),
                            world,
                            shadow: true,
                            layers: layers.clone(),
                            readiness: None,
                        });
                    }
                }
            }
            Err(_) => consumers.push(Consumer {
                key: camera.retained_view_entity,
                world,
                shadow: true,
                layers: layers.clone(),
                readiness: None,
            }),
        }
        match sources
            .light_dependencies
            .get(&camera.retained_view_entity.main_entity)
        {
            Some(Some(dependencies)) => {
                for dependency in dependencies {
                    let Some((main, light, shadow_views)) =
                        local_lights.iter().find(|(main, _, _)| *main == dependency)
                    else {
                        consumers.push(Consumer {
                            key: RetainedViewEntity::new(*dependency, None, 0),
                            world,
                            shadow: true,
                            layers: layers.clone(),
                            readiness: None,
                        });
                        continue;
                    };
                    if !light.shadow_maps_enabled || shadow_views.is_empty() {
                        continue;
                    }
                    let expected = if light.spot_light_angles.is_some() {
                        1
                    } else {
                        bevy::camera::primitives::CUBE_MAP_FACES.len()
                    };
                    for face in 0..expected {
                        let key = shadow_views
                            .get(face)
                            .and_then(|entity| views.get(*entity).ok())
                            .map_or_else(
                                || RetainedViewEntity::new(*main, None, face as u32),
                                |(_, view, _, _)| view.retained_view_entity,
                            );
                        require_shadow_consumer(&mut consumers, key, world, &layers);
                    }
                }
            }
            _ => consumers.push(Consumer {
                key: camera.retained_view_entity,
                world,
                shadow: true,
                layers,
                readiness: None,
            }),
        }
    }
    complete_camera_consumers(&mut consumers, &sources.expected_cameras);
    consumers.sort_unstable_by_key(|consumer| {
        (
            consumer.key.main_entity,
            consumer.key.auxiliary_entity,
            consumer.key.subview_index,
            consumer.world,
            consumer.shadow,
        )
    });
    let revision = [
        producer.generation,
        gpu.identity,
        gpu.generation,
        gpu.layout_epoch,
        sources.version,
        sources.layout_version,
        u64::from(compute),
        layout,
        runtime_state.as_ref().map_or(0, |state| state.revision),
    ];
    let needed_key = NeededCatalogKey {
        gpu_identity: gpu.identity,
        gpu_generation: gpu.generation,
        gpu_layout: gpu.layout_epoch,
        scene_changed: scene.last_changed(),
        materials_revision: materials.revision(),
        refusal_revision: sources.refusal_revision,
        layout_scope: layout,
        runtime_state_revision: runtime_state.as_ref().map(|state| state.revision),
    };
    let result = producer.observe(
        needed_key,
        revision,
        consumers,
        || {
            let mut needed: HashMap<u32, HashMap<(usize, bool), NeededBatch>> = HashMap::new();
            for instance in &scene.instances {
                if matches!(instance.identity, ClusterInstanceIdentity::Object(record)
                if sources.permanently_refused_records.contains(&record))
                {
                    continue;
                }
                let Some(world) = instance.identity.placed_worldspace(Some(package)) else {
                    continue;
                };
                let batch = instance.batch as usize;
                let terrain = matches!(instance.identity, ClusterInstanceIdentity::Terrain { .. });
                let casts_shadows = gpu
                    .batches
                    .get(batch)
                    .and_then(|batch| batch.material)
                    .and_then(|material| materials.get(material))
                    .is_none_or(|material| material.properties.shadows_enabled);
                let invalid_owned_state = match instance.identity {
                    ClusterInstanceIdentity::Object(record) => {
                        package.record(&record).is_some_and(|identity| {
                            runtime_state
                                .as_ref()
                                .is_some_and(|state| state.contains(identity.reference))
                        })
                    }
                    _ => false,
                };
                let need = needed
                    .entry(world)
                    .or_default()
                    .entry((batch, terrain))
                    .or_insert(NeededBatch {
                        batch,
                        terrain,
                        casts_shadows,
                        invalid_owned_state: false,
                    });
                need.invalid_owned_state |= invalid_owned_state;
            }
            needed
                .into_iter()
                .map(|(world, batches)| (world, batches.into_values().collect()))
                .collect()
        },
        |batch| {
            (
                compute,
                sources.templates.get(batch) == Some(&true),
                sources.source_fallback_batches.contains(&(batch as u32)),
            )
        },
    );
    if let Some(worlds) = result {
        // Attribute missing dependencies only when the aggregate changes. These are
        // observations, never permission to omit an expected consumer.
        for consumer in producer
            .consumers
            .iter()
            .filter(|consumer| consumer.readiness.is_none())
        {
            let extracted = views
                .iter()
                .find(|(_, view, _, _)| view.retained_view_entity == consumer.key);
            info!(
                key = ?consumer.key,
                world = consumer.world,
                shadow = consumer.shadow,
                layers = ?consumer.layers,
                expected_camera = sources.expected_cameras.contains_key(&consumer.key.main_entity),
                gpu_view = gpu.views.contains_key(&consumer.key),
                extracted_entity = ?extracted.as_ref().map(|(entity, _, _, _)| entity),
                extracted_shadow = ?extracted.as_ref().map(|(_, _, light, _)| light.is_some()),
                directional_views = ?extracted.as_ref().map(|(entity, _, _, _)| {
                    camera_lights.get(*entity).ok().map(|lights| lights.lights.len())
                }),
                local_light_domain = ?sources.light_dependencies.get(&consumer.key.main_entity)
                    .map(|lights| lights.as_ref().map(HashSet::len)),
                "cluster loading missing consumer"
            );
        }
        producer.withdrawal_pending = false;
        producer.frame = producer.frame.wrapping_add(1);
        producer.pending = Some(Snapshot {
            package: package.clone(),
            layout,
            generation: producer.generation,
            frame: producer.frame,
            worlds,
        });
    }
}

fn publish(mut producer: ResMut<Producer>, mut main: ResMut<MainWorld>) {
    let Some(Snapshot {
        package,
        layout,
        generation,
        frame,
        worlds,
    }) = producer.pending.take()
    else {
        if std::mem::take(&mut producer.withdrawal_pending) {
            let missing = usize::from(
                main.get_resource::<crate::streaming::ActiveSpace>()
                    .is_none_or(|space| space.interior.is_none()),
            );
            if let Some(mut gate) = main.get_resource_mut::<ClusterLoadingGate>() {
                gate.withdraw();
            }
            if let Some(mut profiler) = main.get_resource_mut::<crate::profiling::ProfilingState>()
            {
                profiler.set_gauge("cluster/owned_not_drawable", missing as f64);
            }
            if missing > 0 && crate::work_counters::world_ready() {
                error!(
                    owned_not_drawable = missing,
                    "cluster GPU/package proof was withdrawn; stock replacement is forbidden"
                );
            }
        }
        return;
    };
    let coherent = main
        .get_resource::<crate::cluster::package::ValidatedPackage>()
        .is_some_and(|current| Arc::ptr_eq(&current.0, &package))
        && main
            .get_resource::<crate::cluster::package::LazyBatches>()
            .is_some_and(|lazy| lazy.layout_scope == layout);
    let active_world = main
        .get_resource::<crate::streaming::ActiveSpace>()
        .zip(main.get_resource::<crate::config::EngineConfig>())
        .and_then(|(space, config)| {
            space
                .interior
                .is_none()
                .then_some(space.exterior_worldspace(config.worldspace_id))
        });
    let missing = if let Some(mut gate) = main.get_resource_mut::<ClusterLoadingGate>() {
        if coherent {
            gate.publish(&package, layout, generation, frame, worlds);
        } else {
            gate.withdraw();
        }
        active_world.map_or(0, |world| gate.owned_not_drawable(world))
    } else {
        return;
    };
    if let Some(mut profiler) = main.get_resource_mut::<crate::profiling::ProfilingState>() {
        profiler.set_gauge("cluster/owned_not_drawable", missing as f64);
    }
    if missing > 0 && crate::work_counters::world_ready() {
        error!(
            owned_not_drawable = missing,
            "cluster-owned geometry lost drawability; stock replacement is forbidden"
        );
    }
}

pub(super) fn install(app: &mut App) {
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .init_resource::<Producer>()
            .add_systems(ExtractSchedule, (publish, extract_generation).chain())
            .add_systems(
                Render,
                observe
                    .after(select_instances)
                    .after(RenderSystems::PrepareBindGroups)
                    .before(RenderSystems::Render),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog_key() -> NeededCatalogKey {
        NeededCatalogKey {
            gpu_identity: 11,
            gpu_generation: 13,
            gpu_layout: 17,
            scene_changed: Tick::new(5),
            materials_revision: 19,
            refusal_revision: Some(23),
            layout_scope: 29,
            runtime_state_revision: Some(31),
        }
    }

    fn consumer(id: u32, shadow: bool, ready: Option<Arc<Vec<bool>>>) -> Consumer {
        Consumer {
            key: RetainedViewEntity {
                main_entity: Entity::from_raw_u32(id).unwrap().into(),
                auxiliary_entity: Entity::PLACEHOLDER.into(),
                subview_index: 0,
            },
            world: 42,
            shadow,
            layers: crate::render::PLACED_OBJECT_RENDER_LAYERS,
            readiness: ready,
        }
    }
    fn extracted_view(main: MainEntity, subview: u32) -> ExtractedView {
        ExtractedView {
            retained_view_entity: RetainedViewEntity::new(main, None, subview),
            clip_from_view: Mat4::IDENTITY,
            world_from_view: GlobalTransform::IDENTITY,
            clip_from_world: None,
            target_format: bevy::render::render_resource::TextureFormat::Rgba16Float,
            viewport: UVec4::new(0, 0, 64, 64),
            color_grading: default(),
            invert_culling: false,
        }
    }

    #[test]
    fn loading_views_exclude_ui_subviews_and_keep_all_world_shadow_roles_without_phases() {
        use bevy::ecs::system::SystemState;
        let mut world = World::new();
        let main = Entity::from_raw_u32(100).unwrap().into();
        let camera = world
            .spawn((Camera3d::default(), extracted_view(main, 0)))
            .id();
        // Bevy UI creates a separate ExtractedView sharing the camera's main
        // entity. It has no Camera3d or light role and never draws world geometry.
        let ui = world.spawn(extracted_view(main, 1)).id();
        let light = Entity::from_raw_u32(101).unwrap();
        let mut expected = HashSet::from([camera]);
        for (subview, role) in [
            LightEntity::Directional {
                light_entity: light,
                cascade_index: 0,
            },
            LightEntity::Point {
                light_entity: light,
                face_index: 0,
            },
            LightEntity::Spot {
                light_entity: light,
            },
        ]
        .into_iter()
        .enumerate()
        {
            expected.insert(
                world
                    .spawn((role, extracted_view(light.into(), subview as u32)))
                    .id(),
            );
        }
        let mut state = SystemState::<LoadingViews>::new(&mut world);
        let views = state.get(&world).unwrap();
        let actual: HashSet<_> = views.iter().map(|(entity, _, _, _)| entity).collect();
        assert!(
            !actual.contains(&ui),
            "UI inherited a world consumer role from its camera"
        );
        assert_eq!(
            actual, expected,
            "camera/shadow roles remain required before phases arrive"
        );
    }

    fn needed() -> HashMap<u32, Vec<NeededBatch>> {
        HashMap::from([(
            42,
            vec![NeededBatch {
                batch: 0,
                terrain: false,
                casts_shadows: true,
                invalid_owned_state: false,
            }],
        )])
    }
    #[test]
    fn requested_prepasses_remain_required_when_their_phase_is_missing() {
        use bevy::core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass, NormalPrepass};
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let depth = world.spawn(DepthPrepass).id();
        let normal = world.spawn(NormalPrepass).id();
        let motion = world.spawn(MotionVectorPrepass).id();
        let plain = world.spawn_empty().id();
        world.init_resource::<ViewBinnedRenderPhases<Opaque3dPrepass>>();
        let required = world
            .run_system_once(
                move |required: RequiredPrepassViews,
                      phases: Res<ViewBinnedRenderPhases<Opaque3dPrepass>>| {
                    assert!(phases.is_empty());
                    [depth, normal, motion, plain].map(|entity| required.contains(entity))
                },
            )
            .unwrap();
        assert_eq!(required, [true, true, true, false]);
    }

    #[test]
    fn unparented_local_lights_inherit_only_a_unique_dependent_camera_world() {
        let camera: MainEntity = Entity::from_raw_u32(1).unwrap().into();
        let other: MainEntity = Entity::from_raw_u32(2).unwrap().into();
        let light: MainEntity = Entity::from_raw_u32(9).unwrap().into();
        let mut worlds = HashMap::from([(camera, Some(42))]);
        let mut dependencies = HashMap::from([(camera, Some(HashSet::from([light])))]);
        complete_light_worlds(&mut worlds, &dependencies);
        assert_eq!(worlds.get(&light), Some(&Some(42)));
        worlds.remove(&light);
        worlds.insert(other, Some(43));
        dependencies.insert(other, Some(HashSet::from([light])));
        complete_light_worlds(&mut worlds, &dependencies);
        assert_eq!(worlds.get(&light), Some(&None));
        worlds.insert(light, Some(7));
        complete_light_worlds(&mut worlds, &dependencies);
        assert_eq!(
            worlds.get(&light),
            Some(&Some(7)),
            "authored world ownership wins"
        );
    }

    #[test]
    fn local_shadow_view_without_a_world_consumer_is_missing_even_if_the_view_exists() {
        let key = RetainedViewEntity::new(Entity::from_raw_u32(9).unwrap().into(), None, 3);
        let mut consumers = vec![consumer(1, false, Some(Arc::new(vec![true])))];
        require_shadow_consumer(&mut consumers, key, 42, &RenderLayers::default());
        require_shadow_consumer(&mut consumers, key, 42, &RenderLayers::default());
        let mut producer = Producer::default();
        let result = producer
            .observe(catalog_key(), [0; 9], consumers, needed, |_| {
                (true, true, false)
            })
            .unwrap();
        assert_eq!(result[&42].missing_views, 1);
        assert_eq!(result[&42].pending_pipelines, 1);
    }

    #[test]
    fn missing_expected_camera_blocks_another_ready_camera_in_the_same_world() {
        let missing = Entity::from_raw_u32(2).unwrap().into();
        let mut consumers = vec![consumer(1, false, Some(Arc::new(vec![true])))];
        complete_camera_consumers(&mut consumers, &HashMap::from([(missing, 42)]));
        let mut producer = Producer::default();
        let result = producer
            .observe(catalog_key(), [0; 9], consumers, needed, |_| {
                (true, true, false)
            })
            .unwrap();
        assert_eq!(result[&42].applicable_cameras, 2);
        assert_eq!(result[&42].missing_views, 1);
        assert_eq!(result[&42].pending_pipelines, 1);
    }

    #[test]
    fn portal_and_unlayered_shadows_keep_owned_objects_and_terrain() {
        for (layers, shadow) in [
            (RenderLayers::layer(crate::portal::DESTINATION_LAYER), false),
            (RenderLayers::default(), true),
        ] {
            let domain = ViewDomain::new(&layers, shadow);
            assert!(domain.objects && domain.terrain);
        }
        let placed = ViewDomain::new(&crate::render::PLACED_OBJECT_RENDER_LAYERS, false);
        assert!(placed.objects);
        assert!(!placed.terrain);
        let unrelated = ViewDomain::new(&RenderLayers::none(), false);
        assert!(!unrelated.objects && !unrelated.terrain);
    }

    #[test]
    fn all_views_are_required_and_settled_observations_do_not_walk_batches() {
        let mut producer = Producer::default();
        let ready = Arc::new(vec![true]);
        let consumers = vec![
            consumer(1, false, Some(ready.clone())),
            consumer(2, true, None),
        ];
        let waiting = producer
            .observe(catalog_key(), [0; 9], consumers.clone(), needed, |_| {
                (true, true, false)
            })
            .unwrap();
        assert_eq!(waiting[&42].missing_views, 1);
        assert_eq!(waiting[&42].pending_pipelines, 1);
        assert!(
            producer
                .observe(
                    catalog_key(),
                    [0; 9],
                    consumers,
                    || panic!("settled batch scan"),
                    |_| panic!("settled readiness scan")
                )
                .is_none()
        );
        let ready_world = producer
            .observe(
                catalog_key(),
                [0; 9],
                vec![
                    consumer(1, false, Some(ready.clone())),
                    consumer(2, true, Some(ready)),
                ],
                needed,
                |_| (true, true, false),
            )
            .unwrap();
        assert_eq!(ready_world[&42].pending_pipelines, 0);
        assert_eq!(producer.walks, 2);
    }
    #[test]
    fn source_qualification_changes_refresh_readiness_without_rebuilding_the_needed_catalog() {
        let mut producer = Producer::default();
        let consumers = vec![consumer(1, false, Some(Arc::new(vec![true])))];
        let catalog_calls = std::cell::Cell::new(0);
        let catalog = || {
            catalog_calls.set(catalog_calls.get() + 1);
            needed()
        };
        let mut revision = [1, 11, 13, 17, 19, 23, 1, 29, 31];
        let initial = producer
            .observe(catalog_key(), revision, consumers.clone(), catalog, |_| {
                (true, true, false)
            })
            .unwrap();
        assert_eq!(initial[&42].pending_templates, 0);
        assert_eq!(initial[&42].failed_batches, 0);
        revision[4] += 1; // Source qualification changed; inventory did not.
        revision[5] += 1; // Source template layout qualification changed too.
        let changed = producer
            .observe(catalog_key(), revision, consumers.clone(), catalog, |_| {
                (true, false, true)
            })
            .expect("changed source qualification must publish fresh readiness");
        assert_eq!(changed[&42].needed_batches, 1);
        assert_eq!(changed[&42].pending_templates, 1);
        assert_eq!(changed[&42].failed_batches, 1);
        assert_eq!(
            catalog_calls.get(),
            1,
            "source qualification changes must reuse the needed catalog"
        );
        revision[4] += 1;
        let recovered = producer
            .observe(catalog_key(), revision, consumers, catalog, |_| {
                (true, true, false)
            })
            .unwrap();
        assert_eq!(recovered[&42].pending_templates, 0);
        assert_eq!(recovered[&42].failed_batches, 0);
        assert_eq!(catalog_calls.get(), 1);
    }

    #[test]
    fn changed_consumers_recheck_readiness_without_rebuilding_the_needed_catalog() {
        let mut producer = Producer::default();
        producer
            .observe(
                catalog_key(),
                [0; 9],
                vec![consumer(1, false, Some(Arc::new(vec![true])))],
                needed,
                |_| (true, true, false),
            )
            .unwrap();
        let changed = producer
            .observe(
                catalog_key(),
                [0; 9],
                vec![
                    consumer(1, false, Some(Arc::new(vec![false]))),
                    consumer(2, true, None),
                ],
                || panic!("consumer changes must reuse the needed catalog"),
                |_| (true, true, false),
            )
            .unwrap();
        assert_eq!(changed[&42].applicable_cameras, 1);
        assert_eq!(changed[&42].needed_batches, 2);
        assert_eq!(changed[&42].missing_views, 1);
        assert_eq!(changed[&42].pending_pipelines, 2);
    }

    #[test]
    fn new_admission_generation_republishes_without_rebuilding_the_needed_catalog() {
        let mut producer = Producer::default();
        let consumers = vec![consumer(1, false, Some(Arc::new(vec![true])))];
        let mut revision = [0; 9];
        producer
            .observe(catalog_key(), revision, consumers.clone(), needed, |_| {
                (true, true, false)
            })
            .unwrap();
        revision[0] = 1;
        let current = producer
            .observe(
                catalog_key(),
                revision,
                consumers,
                || panic!("admission generation does not change the inventory"),
                |_| (true, true, false),
            )
            .expect("a new admission generation must not replay old proof");
        assert_eq!(current[&42].pending_pipelines, 0);
        assert_eq!(current[&42].needed_batches, 1);
    }

    #[test]
    fn unchanged_catalog_and_snapshot_inputs_skip_both_callbacks() {
        let mut producer = Producer::default();
        let consumers = vec![consumer(1, false, Some(Arc::new(vec![true])))];
        producer
            .observe(catalog_key(), [0; 9], consumers.clone(), needed, |_| {
                (true, true, false)
            })
            .unwrap();
        assert!(
            producer
                .observe(
                    catalog_key(),
                    [0; 9],
                    consumers,
                    || panic!("unchanged inventory must not be walked"),
                    |_| panic!("unchanged aggregate must not be recomputed"),
                )
                .is_none()
        );
    }

    #[test]
    fn changed_inventory_scope_replaces_the_needed_catalog() {
        let base = catalog_key();
        let mut changes = [base; 8];
        changes[0].gpu_identity += 1;
        changes[1].gpu_generation += 1;
        changes[2].gpu_layout += 1;
        changes[3].scene_changed = Tick::new(6);
        changes[4].materials_revision += 1;
        changes[5].refusal_revision = Some(24);
        changes[6].layout_scope += 1;
        changes[7].runtime_state_revision = Some(32);
        for key in changes {
            let mut producer = Producer::default();
            let consumers = vec![consumer(1, false, Some(Arc::new(vec![true])))];
            producer
                .observe(base, [0; 9], consumers.clone(), needed, |_| {
                    (true, true, false)
                })
                .unwrap();
            let calls = std::cell::Cell::new(0);
            let changed = producer
                .observe(
                    key,
                    [0; 9],
                    consumers,
                    || {
                        calls.set(calls.get() + 1);
                        let mut catalog = needed();
                        catalog.get_mut(&42).unwrap()[0].invalid_owned_state = true;
                        catalog
                    },
                    |_| (true, true, false),
                )
                .expect("catalog changes must defeat an otherwise exact aggregate replay");
            assert_eq!(calls.get(), 1, "key={key:?}");
            assert_eq!(changed[&42].failed_batches, 1, "key={key:?}");
        }
    }

    #[test]
    fn optional_catalog_revisions_distinguish_absence_from_revision_zero() {
        for refusal in [true, false] {
            let mut producer = Producer::default();
            let consumers = vec![consumer(1, false, Some(Arc::new(vec![true])))];
            let mut key = catalog_key();
            let calls = std::cell::Cell::new(0);
            for (expected, value) in [None, Some(0), Some(1)].into_iter().enumerate() {
                if refusal {
                    key.refusal_revision = value;
                } else {
                    key.runtime_state_revision = value;
                }
                let result = producer.observe(
                    key,
                    [0; 9],
                    consumers.clone(),
                    || {
                        calls.set(calls.get() + 1);
                        needed()
                    },
                    |_| (true, true, false),
                );
                assert!(result.is_some(), "refusal={refusal}, revision={value:?}");
                assert_eq!(calls.get(), expected + 1);
            }
        }
    }

    #[test]
    fn changed_material_shadow_policy_replaces_the_cached_shadow_requirement() {
        let mut producer = Producer::default();
        let ready = Arc::new(vec![true]);
        let consumers = vec![
            consumer(1, false, Some(ready.clone())),
            consumer(2, true, Some(ready)),
        ];
        let mut key = catalog_key();
        let initial = producer
            .observe(key, [0; 9], consumers.clone(), needed, |_| {
                (true, true, false)
            })
            .unwrap();
        assert_eq!(initial[&42].needed_batches, 2);
        key.materials_revision += 1;
        let changed = producer
            .observe(
                key,
                [0; 9],
                consumers,
                || {
                    let mut catalog = needed();
                    catalog.get_mut(&42).unwrap()[0].casts_shadows = false;
                    catalog
                },
                |_| (true, true, false),
            )
            .unwrap();
        assert_eq!(changed[&42].applicable_cameras, 1);
        assert_eq!(changed[&42].needed_batches, 1);
        assert_eq!(changed[&42].pending_pipelines, 0);
    }

    #[test]
    fn new_permanent_refusal_removes_old_catalog_failure_contributions() {
        let mut producer = Producer::default();
        let consumers = vec![consumer(1, false, Some(Arc::new(vec![true])))];
        let mut key = catalog_key();
        let initial = producer
            .observe(key, [0; 9], consumers.clone(), needed, |_| {
                (true, false, true)
            })
            .unwrap();
        assert_eq!(initial[&42].needed_batches, 1);
        assert_eq!(initial[&42].failed_batches, 1);
        key.refusal_revision = Some(24);
        let changed = producer
            .observe(key, [0; 9], consumers, HashMap::new, |_| {
                panic!("a permanently refused batch is no longer needed")
            })
            .unwrap();
        assert_eq!(changed[&42].applicable_cameras, 1);
        assert_eq!(changed[&42].needed_batches, 0);
        assert_eq!(changed[&42].pending_templates, 0);
        assert_eq!(changed[&42].failed_batches, 0);
    }

    #[test]
    fn withdrawn_proof_requires_a_fresh_aggregate_even_with_the_same_needed_catalog() {
        use bevy::ecs::system::RunSystemOnce;
        let directory = crate::cluster::reader::producer_fixture_tests::published_for_loader();
        let package = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        let lazy = crate::cluster::package::LazyBatches::unrequested_for_test(1, Vec3::ZERO);
        let layout = lazy.layout_scope;
        let mut main = MainWorld::default();
        main.insert_resource(crate::cluster::package::ValidatedPackage(package.clone()));
        main.insert_resource(lazy);
        main.insert_resource(crate::config::EngineConfig {
            worldspace_id: 42,
            ..default()
        });
        main.init_resource::<crate::streaming::ActiveSpace>();
        main.init_resource::<ClusterLoadingGate>();
        let generation = main.resource::<ClusterLoadingGate>().generation();
        let mut render = World::new();
        render.insert_resource(main);
        render.init_resource::<Producer>();
        let consumers = vec![consumer(1, false, Some(Arc::new(vec![true])))];
        let worlds = render
            .resource_mut::<Producer>()
            .observe(catalog_key(), [0; 9], consumers.clone(), needed, |_| {
                (true, true, false)
            })
            .unwrap();
        render.resource_mut::<Producer>().pending = Some(Snapshot {
            package: package.clone(),
            layout,
            generation,
            frame: 1,
            worlds,
        });
        render.run_system_once(publish).unwrap();
        assert!(
            render
                .resource::<MainWorld>()
                .resource::<ClusterLoadingGate>()
                .world_ready(42)
        );
        // This is the producer state when observe loses its GPU/scene/scope.
        {
            let mut producer = render.resource_mut::<Producer>();
            assert!(producer.revision.take().is_some());
            producer.pending = None;
            producer.withdrawal_pending = true;
        }
        render.run_system_once(publish).unwrap();
        assert!(
            !render
                .resource::<MainWorld>()
                .resource::<ClusterLoadingGate>()
                .world_ready(42)
        );
        let restored = render
            .resource_mut::<Producer>()
            .observe(
                catalog_key(),
                [0; 9],
                consumers,
                || panic!("same-scope return may reuse inventory, never readiness proof"),
                |_| (true, true, false),
            )
            .expect("withdrawal must force a fresh aggregate before readiness can return");
        assert_eq!(restored[&42].needed_batches, 1);
        assert_eq!(restored[&42].pending_pipelines, 0);
        assert!(
            !render
                .resource::<MainWorld>()
                .resource::<ClusterLoadingGate>()
                .world_ready(42),
            "recomputation alone is not published render proof"
        );
        render.resource_mut::<Producer>().pending = Some(Snapshot {
            package,
            layout,
            generation,
            frame: 2,
            worlds: restored,
        });
        render.run_system_once(publish).unwrap();
        assert!(
            render
                .resource::<MainWorld>()
                .resource::<ClusterLoadingGate>()
                .world_ready(42)
        );
    }

    #[test]
    fn forced_owned_failure_is_reported_without_any_stock_action() {
        let mut producer = Producer::default();
        let positive = producer
            .observe(
                catalog_key(),
                [0; 9],
                vec![consumer(1, false, Some(Arc::new(vec![true])))],
                needed,
                |_| (true, true, false),
            )
            .unwrap();
        assert_eq!(positive[&42].pending_pipelines, 0);
        let failed = producer
            .observe(
                catalog_key(),
                [0; 9],
                vec![consumer(1, false, Some(Arc::new(vec![false])))],
                needed,
                |_| (true, true, false),
            )
            .unwrap();
        assert_eq!(failed[&42].pending_pipelines, 1);
    }

    #[test]
    fn forced_owned_failure_publishes_the_counter_and_no_render_bodies() {
        use bevy::ecs::system::RunSystemOnce;
        let directory = crate::cluster::reader::producer_fixture_tests::published_for_loader();
        let package = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        let lazy = crate::cluster::package::LazyBatches::unrequested_for_test(1, Vec3::ZERO);
        let layout = lazy.layout_scope;
        let mut main = MainWorld::default();
        main.insert_resource(crate::cluster::package::ValidatedPackage(package.clone()));
        main.insert_resource(lazy);
        main.insert_resource(crate::config::EngineConfig {
            worldspace_id: 42,
            ..default()
        });
        main.init_resource::<crate::streaming::ActiveSpace>();
        main.init_resource::<ClusterLoadingGate>();
        main.init_resource::<crate::profiling::ProfilingState>();
        let generation = main.resource::<ClusterLoadingGate>().generation();
        let mut render = World::new();
        render.insert_resource(main);
        render.init_resource::<Producer>();
        for (frame, pipelines) in [(1, 0), (2, 1), (3, 0)] {
            render.resource_mut::<Producer>().pending = Some(Snapshot {
                package: package.clone(),
                layout,
                generation,
                frame,
                worlds: std::collections::HashMap::from([(
                    42,
                    ClusterWorldReadiness {
                        applicable_cameras: 1,
                        needed_batches: 1,
                        pending_pipelines: pipelines,
                        ..default()
                    },
                )]),
            });
            render.run_system_once(publish).unwrap();
            let main = render.resource::<MainWorld>();
            assert_eq!(
                main.resource::<ClusterLoadingGate>().world_ready(42),
                pipelines == 0
            );
            assert_eq!(
                main.resource::<crate::profiling::ProfilingState>()
                    .gauge("cluster/owned_not_drawable"),
                Some(pipelines as f64)
            );
            assert_eq!(
                main.iter_entities()
                    .filter(|entity| entity.contains::<Mesh3d>())
                    .count(),
                0
            );
        }
    }
}
