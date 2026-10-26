//! Feature-only inventory evidence for explicit SceneState requests.

use super::*;

/// Diagnostic plugins add singleton entities in Bevy 0.19. Account for only
/// their exact resource entities; gameplay components must never be subtracted.
/// A resource has its value and Bevy's mandatory, matching IsResource marker.
fn capture_resource_entity<R: Resource>(world: &World, role: &str) -> Option<serde_json::Value> {
    let component = world.component_id::<R>()?;
    let entity = world.resource_entities().get(component)?;
    let resource = world.get_entity(entity).ok()?;
    let only_resource_component = resource.archetype().component_count() == 2
        && resource.contains::<R>()
        && resource
            .get::<bevy::ecs::resource::IsResource>()
            .is_some_and(|marker| marker.resource_component_id() == component);
    Some(serde_json::json!({
        "entity": entity.to_bits(),
        "role": role,
        "type_name": std::any::type_name::<R>(),
        "only_resource_component": only_resource_component,
    }))
}

pub(super) fn extend(world: &World, mut state: serde_json::Value) -> serde_json::Value {
    let profiler = world.get_resource::<crate::profiling::ProfilingState>();
    // This command is an explicit diagnostic request. Count the requested
    // world's current meshes rather than reusing a periodic benchmark sample.
    // The same query filter is used by the benchmark inventory producer.
    let mesh_entities = world
        .try_query_filtered::<(), With<Mesh3d>>()
        .map_or(0, |mut meshes| meshes.iter(world).count());
    let capture_resource_entities = [
        capture_resource_entity::<IncomingCommands>(world, "stdin_commands"),
        capture_resource_entity::<crate::capture_frame::RetryReadiness>(
            world,
            "capture_retry_readiness",
        ),
        capture_resource_entity::<crate::capture_frame::TransferReadiness>(
            world,
            "capture_transfer_readiness",
        ),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    let entity_count_without_capture_resources = capture_resource_entities
        .iter()
        .all(|resource| resource["only_resource_component"] == true)
        .then(|| {
            world
                .entity_count()
                .checked_sub(capture_resource_entities.len() as u32)
        })
        .flatten();
    let distant_gauge = |name: &str| {
        profiler
            .and_then(|profiler| profiler.gauge(name))
            .filter(|value| value.is_finite() && *value >= 0.0 && value.fract() == 0.0)
            .map(|value| value as u64)
    };
    let fields = serde_json::json!({
        "main_frame": profiler.map(crate::profiling::ProfilingState::main_frame),
        "entity_count": world.entity_count(),
        "entity_count_without_capture_resources": entity_count_without_capture_resources,
        "capture_resource_entities": capture_resource_entities,
        "mesh_entities": mesh_entities,
        "distant_stock": {
            "spawned_references": profiler.and_then(|profiler| profiler.counter("lod/exact_stock_spawned_references")),
            "queued_cells": distant_gauge("lod/exact_stock_queued_cells"),
            "pending_cells": distant_gauge("lod/exact_stock_pending_cells"),
            "ready_cells": distant_gauge("lod/exact_stock_ready_cells"),
            "awaiting_response_cells": distant_gauge("lod/exact_stock_awaiting_response_cells"),
            "failed_cells": distant_gauge("lod/exact_stock_failed_cells"),
        },
    });
    if let (Some(state), Some(fields)) = (state.as_object_mut(), fields.as_object()) {
        state.extend(fields.clone());
    }
    state
}
