use super::*;

#[test]
fn scene_state_inventory_is_sampled_from_the_requested_world() {
    let mut world = ready_scene_world();
    let mesh = world.spawn(Mesh3d::default()).id();
    {
        let mut profiler = world.resource_mut::<crate::profiling::ProfilingState>();
        profiler.increment("lod/exact_stock_spawned_references", 17);
        for field in [
            "queued_cells",
            "pending_cells",
            "ready_cells",
            "awaiting_response_cells",
            "failed_cells",
        ] {
            profiler.set_gauge(format!("lod/exact_stock_{field}"), 0.0);
        }
    }
    let first = scene_state(&world, crate::shots::Failures::default());
    assert_eq!(first["entity_count"], world.entity_count());
    assert_eq!(first["mesh_entities"], 1);
    assert_eq!(first["distant_stock"]["spawned_references"], 17);
    assert_eq!(first["distant_stock"]["pending_cells"], 0);
    world.despawn(mesh);
    world
        .resource_mut::<crate::profiling::ProfilingState>()
        .set_gauge("lod/exact_stock_pending_cells", 2.0);
    let second = scene_state(&world, crate::shots::Failures::default());
    assert_eq!(second["entity_count"], world.entity_count());
    assert_eq!(second["mesh_entities"], 0);
    assert_eq!(second["distant_stock"]["pending_cells"], 2);
}

#[test]
fn scene_state_accounts_only_exact_capture_resource_entities() {
    let mut world = ready_scene_world();
    let baseline = world.entity_count() - 2;
    let (_sender, receiver) = mpsc::sync_channel(1);
    world.insert_resource(IncomingCommands(Mutex::new(receiver)));
    let first = scene_state(&world, crate::shots::Failures::default());
    assert_eq!(first["entity_count"], baseline + 3);
    assert_eq!(first["entity_count_without_capture_resources"], baseline);
    let resources = first["capture_resource_entities"].as_array().unwrap();
    assert_eq!(resources.len(), 3);
    assert!(
        resources
            .iter()
            .all(|row| row["only_resource_component"] == true)
    );
    for (role, type_name) in [
        ("stdin_commands", std::any::type_name::<IncomingCommands>()),
        (
            "capture_retry_readiness",
            std::any::type_name::<crate::capture_frame::RetryReadiness>(),
        ),
        (
            "capture_transfer_readiness",
            std::any::type_name::<crate::capture_frame::TransferReadiness>(),
        ),
    ] {
        let row = resources.iter().find(|row| row["role"] == role).unwrap();
        assert_eq!(row["type_name"], type_name);
    }
    let ids = resources
        .iter()
        .map(|row| row["entity"].as_u64().unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), resources.len());

    let ordinary = world.spawn_empty().id();
    let second = scene_state(&world, crate::shots::Failures::default());
    assert_eq!(
        second["entity_count_without_capture_resources"],
        baseline + 1
    );
    world.entity_mut(ordinary).insert(Mesh3d::default());
    let third = scene_state(&world, crate::shots::Failures::default());
    assert_eq!(
        third["entity_count_without_capture_resources"],
        baseline + 1
    );
    assert_eq!(third["mesh_entities"], 1);

    let incoming = world
        .resource_entities()
        .get(world.component_id::<IncomingCommands>().unwrap())
        .unwrap();
    world.entity_mut(incoming).insert(Mesh3d::default());
    let contaminated = scene_state(&world, crate::shots::Failures::default());
    assert!(contaminated["entity_count_without_capture_resources"].is_null());
    assert_eq!(contaminated["entity_count"], world.entity_count());
}
