//! Shared reference creation only. Demand, identity ownership and promotion remain caller-owned.

use super::{
    PendingAssetProfile, PendingModel, converted_model_path, creation_rotation_to_bevy,
    creation_to_bevy,
};
use crate::{
    profiling::ProfilingState,
    world::{
        components::{
            CellRef, ExpectedModelBounds, FormId, InstanceBounds, MeshHandle, WorldPosition,
            WorldTransform,
        },
        database::{CellKey, ReferenceRow},
    },
};
use bevy::prelude::*;
use std::time::Instant;

/// Inputs already selected by the caller. This helper does not establish residency or readiness.
pub(crate) struct ReferenceSpawnContext<'a> {
    pub(crate) key: CellKey,
    pub(crate) model_sequence: &'a mut u64,
    pub(crate) asset_server: &'a AssetServer,
    pub(crate) profiler: &'a mut ProfilingState,
    pub(crate) world_physics: bool,
    pub(crate) lights: bool,
    pub(crate) schedule_scenes: bool,
}

/// The ordinary loader's exact placement, also used when a retained source changes cell owner.
pub(crate) fn reference_placement(
    position: [f32; 3],
    rotation: [f32; 3],
    scale: f32,
    key: CellKey,
) -> (WorldPosition, Transform) {
    let creation_position = Vec3::from_array(position);
    let world_position = WorldPosition::from_creation_units(creation_position);
    let translation = match key {
        CellKey::Exterior { grid_x, grid_y, .. } => {
            creation_to_bevy(world_position.relative_to(IVec2::new(grid_x, grid_y)))
        }
        CellKey::Interior(_) => creation_to_bevy(creation_position),
    };
    let transform = Transform::from_translation(translation)
        .with_rotation(creation_rotation_to_bevy(rotation))
        .with_scale(Vec3::splat(scale));
    (world_position, transform)
}

/// Spawn under the caller's cell-space parent; return the reference root and whether a converted
/// model was requested. Reference identity, demand and promotion policy remain with the caller.
pub(crate) fn spawn_reference(
    parent: &mut ChildSpawnerCommands<'_>,
    reference: ReferenceRow,
    context: &mut ReferenceSpawnContext<'_>,
) -> Option<(Entity, bool)> {
    let key = context.key;
    let model_sequence = &mut *context.model_sequence;
    let asset_server = context.asset_server;
    let profiler = &mut *context.profiler;
    let world_physics = context.world_physics;
    let lights = context.lights;
    let schedule_scenes = context.schedule_scenes;
    if !reference.initially_enabled {
        return None;
    }
    let mut model_requested = false;
    let (world_position, transform) =
        reference_placement(reference.position, reference.rotation, reference.scale, key);
    let model_bounds = reference.bounds_valid.then(|| {
        ExpectedModelBounds::new(
            Vec3::from_array(reference.bounds_min),
            Vec3::from_array(reference.bounds_max),
        )
    });
    let model_bounds = model_bounds.flatten();
    let bounds = model_bounds
        .map(|bounds| InstanceBounds::transformed(bounds.min, bounds.max, transform.to_matrix()));
    let mut entity = parent.spawn((
        Name::new(format!("Reference {:08X}", reference.form_id)),
        FormId(reference.form_id),
        CellRef(reference.cell_id),
        world_position,
        WorldTransform(transform.to_matrix()),
        transform,
    ));
    if let Some(bounds) = bounds.zip(model_bounds) {
        entity.insert(bounds);
    }
    if let Some(rgb) = reference.emittance {
        entity.insert(crate::emittance::ReferenceEmittance::from_srgb(rgb));
    }
    // Descendant scenes keep the ordinary placed-object layer source on both load paths.
    entity.insert(bevy::app::Propagate(super::PLACED_OBJECT_RENDER_LAYERS));
    if let Some(door) = crate::doors::load_door(reference.form_id, reference.door.as_ref()) {
        entity.insert(door);
    }
    // A child of the reference, so the light sits where the reference is and follows it
    // through a render-origin rebase - and, because it is a descendant of the cell root,
    // through the cell unload that removes the reference.
    if let Some(light) = reference
        .light
        .as_ref()
        .and_then(|row| crate::lights::point_light(row, reference.light_radius_override))
    {
        // A reference without a model has no visibility components, so its light child
        // could never become visible: Bevy warned (B0004) and extract_lights dropped every
        // such light. The reference needs Visibility for the hierarchy to propagate.
        let marker = crate::lights::SkyrimLight {
            form_id: reference.form_id,
            cell_id: reference.cell_id,
        };
        if !lights {
            entity.insert(crate::lights::DeferredPointLight { light, marker });
        } else {
            entity.insert(Visibility::default());
            entity.with_child((
                Name::new(format!("Light {:08X}", reference.form_id)),
                light,
                marker,
                crate::render::light_render_layers(),
            ));
        }
    }
    if let Some(path) = reference.model_path.and_then(converted_model_path) {
        // This is candidate identity, not ownership permission. The certificate
        // and occurrence ordinal decide ownership on the instantiated children.
        if matches!(key, CellKey::Exterior { .. })
            && reference.base_record_type.as_deref().is_some_and(|kind| {
                shared::cluster_recipe::classify::base_type_in_scope(kind, true)
            })
        {
            entity.insert(crate::cluster::ownership::OwnershipReference(
                reference.form_id,
            ));
        }
        model_requested = true;
        let sequence = *model_sequence;
        *model_sequence = model_sequence.saturating_add(1);
        if schedule_scenes {
            let (center, radius) =
                super::requests::reference_geometry(world_position, &transform, bounds);
            entity.insert(super::requests::SceneRequest {
                path: path.clone(),
                cell: Some(key),
                coarse_terrain: false,
                retry_cleanup: None,
                content_identity: None,
                sequence,
                center,
                radius,
                collision_candidate: world_physics
                    && super::fixed_collision_record_eligible(
                        reference.base_record_type.as_deref(),
                    ),
                handle: None,
                started: Instant::now(),
                allow_retry: false,
            });
        } else {
            let handle = asset_server.load(GltfAssetLabel::Scene(0).from_asset(path.clone()));
            profiler.observe_scene(handle.id(), asset_server);
            entity.insert(PendingModel { handle, sequence });
        }
        entity.insert((
            MeshHandle(path.clone()),
            PendingAssetProfile {
                started: Instant::now(),
                scene_spawned: false,
                path,
                form_id: reference.form_id,
                base_form_id: reference.base_form_id,
                base_record_type: reference.base_record_type.clone(),
                static_physics: world_physics,
                cell_id: reference.cell_id,
            },
        ));
    }
    Some((entity.id(), model_requested))
}
