//! Ambient playback coverage retained from the legacy module, adapted to public scene ownership.
use super::*;
use bevy::{
    animation::{
        AnimationPlugin, AnimationTargetId, animated_field,
        animation_curves::{AnimatableCurve, AnimatableKeyframeCurve},
    },
    time::TimeUpdateStrategy,
};
use std::time::Duration;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), AnimationPlugin))
        .init_resource::<GltfExtensionHandlers>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            0.1,
        )))
        .add_plugins(ModelAnimationPlugin);
    app
}
fn reference(app: &mut App, form_id: u32) -> Entity {
    app.world_mut()
        .spawn((
            MeshHandle("meshes/invented-wheel.glb".into()),
            FormId(form_id),
        ))
        .id()
}
fn source(app: &mut App, reference: Entity, names: &[&str], player: bool) -> Entity {
    let mut inventory = Vec::new();
    for name in names {
        let mut clip = AnimationClip::default();
        clip.add_curve_to_target(
            AnimationTargetId::from_name(&Name::new("invented leaf")),
            AnimatableCurve::new(
                animated_field!(Transform::rotation),
                AnimatableKeyframeCurve::new([
                    (0.0, Quat::IDENTITY),
                    (4.0, Quat::from_rotation_y(1.0)),
                ])
                .unwrap(),
            ),
        );
        let handle = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip);
        inventory.push((name.to_string(), handle));
    }
    let entity = app
        .world_mut()
        .spawn((SourceClips(inventory), ChildOf(reference)))
        .id();
    if player {
        app.world_mut()
            .entity_mut(entity)
            .insert(AnimationPlayer::default());
    }
    entity
}
#[test]
#[ignore = "reads the converted Skyrim models (MUDCRAB_CONVERTED_DIR)"]
fn the_wheel_names_its_clip_idle() {
    let Some(assets) = std::env::var_os("MUDCRAB_CONVERTED_DIR")
        .or_else(|| std::env::var_os("OPENSKYRIM_CONVERTED_DIR"))
        .map(std::path::PathBuf::from)
    else {
        eprintln!("skipping: set MUDCRAB_CONVERTED_DIR to a converted asset tree");
        return;
    };
    let path = assets.join("meshes/architecture/farmhouse/lumbermill01waterwheel01.glb");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("skipping: {} is not installed", path.display());
        return;
    };
    let document = bevy::gltf::gltf::Gltf::from_slice(&bytes).expect("a valid .glb");
    let names: Vec<_> = document
        .animations()
        .filter_map(|animation| animation.name())
        .collect();
    assert_eq!(names, ["Idle"]);
}
#[test]
fn the_clip_match_takes_idle_and_nothing_near_it() {
    for (names, expected) in [
        (vec!["Idle", "Open", "Close", "idle"], true),
        (vec!["Open", "Close", "idle"], false),
        (vec![], false),
    ] {
        let mut app = app();
        let reference = reference(&mut app, 17);
        source(&mut app, reference, &names, true);
        app.update();
        assert_eq!(
            app.world()
                .get::<SourceClipBinding>(reference)
                .unwrap()
                .idle
                .is_some(),
            expected
        );
    }
}
#[test]
fn the_offset_is_a_phase_inside_the_clip() {
    for form in [0, 1, 2, 0x12345, u32::MAX] {
        for duration in [0.1, 1.0, 4.0, 17.9] {
            let offset = start_offset(form, duration);
            assert!(offset >= 0.0 && offset < duration);
            assert_eq!(offset, start_offset(form, duration));
        }
    }
    for duration in [0.0, -1.0, f32::INFINITY, f32::NAN] {
        assert_eq!(start_offset(17, duration), 0.0);
    }
}
#[test]
fn two_form_ids_of_one_model_start_at_different_times() {
    assert_ne!(start_offset(17, 4.0), start_offset(18, 4.0));
}
#[test]
fn every_reference_of_a_model_plays_the_clip_from_its_own_phase() {
    let mut app = app();
    let first = reference(&mut app, 17);
    let second = reference(&mut app, 18);
    let first_player = source(&mut app, first, &["Idle"], true);
    // Two actual scene instances of one source inventory, sharing authored clip handles.
    let inventory = app
        .world()
        .get::<SourceClips>(first_player)
        .unwrap()
        .clone();
    let second_player = app
        .world_mut()
        .spawn((inventory, AnimationPlayer::default(), ChildOf(second)))
        .id();
    app.update();
    let seek = |reference, player| {
        let node = app
            .world()
            .get::<SourceClipBinding>(reference)
            .unwrap()
            .idle
            .unwrap()
            .0;
        app.world()
            .get::<AnimationPlayer>(player)
            .unwrap()
            .animation(node)
            .unwrap()
            .seek_time()
    };
    let delta = (seek(second, second_player) - seek(first, first_player)).abs();
    assert!(
        (delta - 1.0).abs() < 0.01,
        "same clock, different authored FormID phase: {delta}"
    );
}
#[test]
fn a_completed_source_with_no_idle_clip_keeps_its_binding_until_revision() {
    let mut app = app();
    let reference = reference(&mut app, 17);
    let source = source(&mut app, reference, &["LoadDust", "PileDust"], true);
    app.update();
    let initial = app
        .world()
        .entity(reference)
        .get_ref::<SourceClipBinding>()
        .unwrap()
        .last_changed();
    for _ in 0..244 {
        app.update();
    }
    let binding = app
        .world()
        .entity(reference)
        .get_ref::<SourceClipBinding>()
        .unwrap();
    assert_eq!(binding.last_changed(), initial);
    assert_eq!(binding.source, source);
    assert!(binding.graph.is_none());
}
#[test]
fn a_reference_whose_model_cannot_load_remains_unresolved_without_root_request() {
    let mut app = app();
    let reference = reference(&mut app, 17);
    for _ in 0..244 {
        app.update();
    }
    assert!(app.world().get::<SourceClipBinding>(reference).is_none());
    assert!(
        app.world()
            .resource::<AssetServer>()
            .get_path_id("meshes/invented-wheel.glb")
            .is_none()
    );
}
#[test]
fn a_scene_that_never_produces_a_player_remains_unresolved() {
    let mut app = app();
    let reference = reference(&mut app, 17);
    source(&mut app, reference, &["Idle"], false);
    for _ in 0..244 {
        app.update();
    }
    assert!(app.world().get::<SourceClipBinding>(reference).is_none());
    assert!(app.world().get::<IdleStarted>(reference).is_none());
}
#[test]
fn a_reference_despawned_before_the_commands_apply_is_not_touched() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = app();
    let reference = reference(&mut app, 17);
    source(&mut app, reference, &["Idle"], true);
    // Queue despawn ahead of the resolver's fallible entity commands. Both commands run at the
    // schedule boundary: stale references must never panic or get resurrected.
    let mut schedule = Schedule::default();
    schedule.add_systems(
        (
            move |mut commands: Commands| {
                commands.entity(reference).despawn();
            },
            resolve_source_clips,
        )
            .chain_ignore_deferred(),
    );
    schedule.run(app.world_mut());
    assert!(app.world().get_entity(reference).is_err());
    // Retain a direct system path as well, with no references left to claim.
    app.world_mut()
        .run_system_once(resolve_source_clips)
        .unwrap();
}
#[test]
fn a_load_door_is_not_claimed_for_ambient_idle() {
    let mut app = app();
    let reference = reference(&mut app, 17);
    app.world_mut()
        .entity_mut(reference)
        .insert(crate::doors::LoadDoor {
            ref_id: 17,
            label: "Invented destination".into(),
            auto_load: false,
            outward: None,
            destination: crate::doors::DoorDestination {
                destination_ref_id: 18,
                interior_cell_id: Some(19),
                worldspace_id: None,
                arrival_position: [0.0; 3],
                arrival_rotation: [0.0; 3],
            },
        });
    source(&mut app, reference, &["Idle"], true);
    app.update();
    assert!(
        app.world()
            .get::<SourceClipBinding>(reference)
            .unwrap()
            .idle
            .is_none()
    );
    assert!(app.world().get::<IdleStarted>(reference).is_none());
}
