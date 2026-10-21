//! Ordinary load-screen door swings. Crossing owns loading; animation owns only source clips.
use crate::{
    doors::{LoadDoor, OpenDoor},
    model_animation::SourceClipBinding,
};
use bevy::prelude::*;

pub struct DoorAnimationPlugin;
#[derive(Resource, Default)]
pub struct PendingDoorOpens(pub Vec<Entity>);
impl Plugin for DoorAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenDoor>()
            .init_resource::<PendingDoorOpens>()
            // The accepted keyboard request is written in PostUpdate. Consume it in the next
            // Update after source binding, without delaying crossing or requiring far-side residency.
            .add_systems(
                Update,
                activate_doors.after(crate::model_animation::resolve_source_clips),
            );
    }
}
fn activate_doors(
    mut open: MessageReader<OpenDoor>,
    mut pending: ResMut<PendingDoorOpens>,
    doors: Query<Option<&SourceClipBinding>, With<LoadDoor>>,
    mut players: Query<&mut AnimationPlayer>,
) {
    for request in open.read() {
        if !pending.0.contains(&request.door) {
            pending.0.push(request.door);
        }
    }
    pending.0.retain(|door| {
        let Ok(binding) = doors.get(*door) else {
            return false;
        };
        let Some(binding) = binding else {
            return true;
        };
        // A resolved inventory with no Open clip is an authored static leaf.
        let Some(node) = binding.open else {
            return false;
        };
        if binding.players.is_empty() || binding.players.iter().any(|p| players.get(*p).is_err()) {
            return true;
        }
        for player in &binding.players {
            if let Ok(mut player) = players.get_mut(*player) {
                player.stop_all();
                player.start(node);
            }
        }
        false
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        doors::DoorDestination,
        model_animation::{ModelAnimationPlugin, SourceClips},
        world::components::{FormId, MeshHandle},
    };
    use bevy::{
        animation::{
            AnimatedBy, AnimationPlugin, AnimationTargetId, animated_field,
            animation_curves::{AnimatableCurve, AnimatableKeyframeCurve},
        },
        gltf::extensions::GltfExtensionHandlers,
        time::TimeUpdateStrategy,
    };
    use std::time::Duration;
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            TransformPlugin,
            AnimationPlugin,
        ))
        .init_resource::<GltfExtensionHandlers>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            0.1,
        )))
        .add_plugins((ModelAnimationPlugin, DoorAnimationPlugin));
        app
    }
    fn door(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                LoadDoor {
                    ref_id: 7,
                    label: "destination".into(),
                    destination: DoorDestination {
                        destination_ref_id: 8,
                        interior_cell_id: Some(9),
                        worldspace_id: None,
                        arrival_position: [0.0; 3],
                        arrival_rotation: [0.0; 3],
                    },
                },
                MeshHandle("meshes/test.glb".into()),
                FormId(7),
                Transform::default(),
            ))
            .id()
    }
    fn source(app: &mut App, reference: Entity, name: &str) -> (Entity, Entity) {
        let target = AnimationTargetId::from_name(&Name::new("leaf"));
        let mut clip = AnimationClip::default();
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(
                animated_field!(Transform::rotation),
                AnimatableKeyframeCurve::new([
                    (0.0, Quat::IDENTITY),
                    (1.0, Quat::from_rotation_y(1.0)),
                ])
                .unwrap(),
            ),
        );
        let handle = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip);
        let source = app
            .world_mut()
            .spawn((
                SourceClips(vec![(name.into(), handle)]),
                AnimationPlayer::default(),
                Transform::default(),
                ChildOf(reference),
            ))
            .id();
        let leaf = app
            .world_mut()
            .spawn((
                Name::new("leaf"),
                target,
                AnimatedBy(source),
                Transform::default(),
                ChildOf(source),
            ))
            .id();
        (source, leaf)
    }
    #[test]
    fn ordinary_source_open_plays_a_real_clip_without_destination_residency() {
        let mut app = app();
        let door = door(&mut app);
        let (player, leaf) = source(&mut app, door, "Open");
        app.world_mut().write_message(OpenDoor { door });
        for _ in 0..4 {
            app.update();
        }
        let binding = app.world().get::<SourceClipBinding>(door).unwrap();
        assert!(
            app.world()
                .get::<AnimationPlayer>(player)
                .unwrap()
                .is_playing_animation(binding.open.unwrap())
        );
        assert_ne!(
            app.world().get::<Transform>(leaf).unwrap().rotation,
            Quat::IDENTITY
        );
        assert!(app.world().resource::<PendingDoorOpens>().0.is_empty());
        assert!(
            !app.world()
                .contains_resource::<crate::streaming::StreamingWorld>()
        );
    }
    #[test]
    fn unresolved_source_queues_without_static_answer_then_plays_once() {
        let mut app = app();
        let door = door(&mut app);
        app.world_mut().write_message(OpenDoor { door });
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(app.world().resource::<PendingDoorOpens>().0, vec![door]);
        assert!(app.world().get::<SourceClipBinding>(door).is_none());
        let (player, _) = source(&mut app, door, "open");
        app.update();
        assert!(app.world().resource::<PendingDoorOpens>().0.is_empty());
        let node = app
            .world()
            .get::<SourceClipBinding>(door)
            .unwrap()
            .open
            .unwrap();
        for _ in 0..3 {
            app.update();
        }
        assert!(
            app.world()
                .get::<AnimationPlayer>(player)
                .unwrap()
                .animation(node)
                .unwrap()
                .elapsed()
                > 0.15
        );
    }
    #[test]
    fn unload_discards_pending_and_only_inventory_proves_a_static_leaf() {
        let mut app = app();
        let first = door(&mut app);
        app.world_mut().write_message(OpenDoor { door: first });
        app.update();
        app.world_mut().entity_mut(first).despawn();
        app.update();
        assert!(app.world().resource::<PendingDoorOpens>().0.is_empty());
        let second = door(&mut app);
        app.world_mut()
            .spawn((SourceClips::default(), ChildOf(second)));
        app.world_mut().write_message(OpenDoor { door: second });
        app.update();
        assert!(app.world().get::<SourceClipBinding>(second).is_some());
        assert!(app.world().resource::<PendingDoorOpens>().0.is_empty());
    }
    #[test]
    fn scene_replacement_rebinds_and_ambient_idle_uses_its_own_clip() {
        let mut app = app();
        let reference = app
            .world_mut()
            .spawn((
                MeshHandle("meshes/wheel.glb".into()),
                FormId(17),
                Transform::default(),
            ))
            .id();
        let (old, _) = source(&mut app, reference, "Idle");
        app.update();
        assert!(
            app.world()
                .get::<SourceClipBinding>(reference)
                .unwrap()
                .idle
                .is_some()
        );
        app.world_mut().entity_mut(old).despawn();
        let (new, leaf) = source(&mut app, reference, "Idle");
        for _ in 0..4 {
            app.update();
        }
        let binding = app.world().get::<SourceClipBinding>(reference).unwrap();
        assert_eq!(binding.players, vec![new]);
        assert_ne!(
            app.world().get::<Transform>(leaf).unwrap().rotation,
            Quat::IDENTITY
        );
    }
}
