//! A mesh that opted out of Bevy's automatic bounds and has none is never drawn.
//!
//! `NoAutoAabb` tells Bevy's `calculate_bounds` to leave an entity's `Aabb` alone, so an entity
//! that carries it and no `Aabb` has no bounds at all. Bevy's GPU culling gives such a mesh
//! infinite half-extents, and its frustum test then rejects it: measured on this engine with
//! `examples/occlusion_mod_probe.rs` (RTX 3090, Vulkan), the mesh is drawn by neither a camera
//! with `OcclusionCulling` nor one without. The mesh vanishes with no message anywhere.
//!
//! Nothing in the engine spawns `NoAutoAabb` today. The first thing that will is a mod or script
//! path that sets its own bounds (the converter's data, a future Luau API) or a wind or morph
//! effect that must grow the box. This plugin makes the mistake loud: it counts the entities in
//! that state every frame ([`UnboundedMeshes`]) and logs one error when the count grows.
//!
//! The rule it protects, from the O2 occlusion measurement: a mod mesh's bounds come from the
//! converter or from Bevy, never from a script, and a mesh whose vertices move beyond the rest
//! pose (wind, morph targets) needs a displacement bound added to its box.

use bevy::{
    camera::{primitives::Aabb, visibility::NoAutoAabb},
    prelude::*,
};

/// How many meshes carry `NoAutoAabb` and no `Aabb`, as of the last frame.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UnboundedMeshes(pub usize);

pub struct AabbGuardPlugin;

impl Plugin for AabbGuardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UnboundedMeshes>()
            .add_systems(PostUpdate, report_unbounded_meshes);
    }
}

/// A mesh that asked Bevy not to compute its bounds and has none.
type UnboundedMesh = (With<Mesh3d>, With<NoAutoAabb>, Without<Aabb>);

fn report_unbounded_meshes(
    meshes: Query<Entity, UnboundedMesh>,
    mut count: ResMut<UnboundedMeshes>,
) {
    let now = meshes.iter().count();
    if now > count.0 {
        error!(
            "{now} mesh(es) have NoAutoAabb and no Aabb, so Bevy's GPU culling never draws them; \
             give each an Aabb that covers every vertex it can be drawn at, or remove NoAutoAabb"
        );
    }
    if now != count.0 {
        count.0 = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(AabbGuardPlugin);
        app
    }

    #[test]
    fn a_mesh_with_no_auto_bounds_and_no_box_is_counted() {
        let mut app = app();
        app.world_mut()
            .spawn((Mesh3d(Handle::default()), NoAutoAabb));
        app.update();
        assert_eq!(
            *app.world().resource::<UnboundedMeshes>(),
            UnboundedMeshes(1)
        );
    }

    #[test]
    fn a_mesh_with_its_own_box_or_automatic_bounds_is_not() {
        let mut app = app();
        app.world_mut().spawn((
            Mesh3d(Handle::default()),
            NoAutoAabb,
            Aabb::from_min_max(Vec3::splat(-1.0), Vec3::splat(1.0)),
        ));
        app.world_mut().spawn(Mesh3d(Handle::default()));
        app.update();
        assert_eq!(
            *app.world().resource::<UnboundedMeshes>(),
            UnboundedMeshes(0)
        );
    }

    #[test]
    fn the_count_falls_again_when_the_box_arrives() {
        let mut app = app();
        let mesh = app
            .world_mut()
            .spawn((Mesh3d(Handle::default()), NoAutoAabb))
            .id();
        app.update();
        app.world_mut()
            .entity_mut(mesh)
            .insert(Aabb::from_min_max(Vec3::splat(-1.0), Vec3::splat(1.0)));
        app.update();
        assert_eq!(
            *app.world().resource::<UnboundedMeshes>(),
            UnboundedMeshes(0)
        );
    }
}
