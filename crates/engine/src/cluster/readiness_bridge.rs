//! Changed-only loading proof over every applicable cluster consumer.

use super::*;
use crate::cluster::loading::{ClusterLoadingGate, ClusterWorldReadiness};
use bevy::pbr::{ExtractedPointLight, PointAndSpotLightViewEntities, ViewLightEntities};
use bevy::render::MainWorld;

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
        revision: [u64; 9],
        consumers: Vec<Consumer>,
        needed: impl FnOnce() -> HashMap<u32, Vec<NeededBatch>>,
        batch_ready: impl Fn(usize) -> (bool, bool, bool),
    ) -> Option<std::collections::HashMap<u32, ClusterWorldReadiness>> {
        if self.revision == Some(revision)
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
        let needed = needed();
        let mut worlds = std::collections::HashMap::new();
        for consumer in &consumers {
            let world = worlds
                .entry(consumer.world)
                .or_insert_with(ClusterWorldReadiness::default);
            world.applicable_cameras += usize::from(!consumer.shadow);
            if consumer.readiness.is_none() {
                world.missing_views += 1;
            }
            for need in needed.get(&consumer.world).into_iter().flatten() {
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
    views: Query<SelectionViewState>,
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
    let result = producer.observe(
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
            .observe([0; 9], consumers, needed, |_| (true, true, false))
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
            .observe([0; 9], consumers, needed, |_| (true, true, false))
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
            .observe([0; 9], consumers.clone(), needed, |_| (true, true, false))
            .unwrap();
        assert_eq!(waiting[&42].missing_views, 1);
        assert_eq!(waiting[&42].pending_pipelines, 1);
        assert!(
            producer
                .observe(
                    [0; 9],
                    consumers,
                    || panic!("settled batch scan"),
                    |_| panic!("settled readiness scan")
                )
                .is_none()
        );
        let ready_world = producer
            .observe(
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
    fn forced_owned_failure_is_reported_without_any_stock_action() {
        let mut producer = Producer::default();
        let positive = producer
            .observe(
                [0; 9],
                vec![consumer(1, false, Some(Arc::new(vec![true])))],
                needed,
                |_| (true, true, false),
            )
            .unwrap();
        assert_eq!(positive[&42].pending_pipelines, 0);
        let failed = producer
            .observe(
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
