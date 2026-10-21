//! Persistent revocation of the converted resting representation of a reference.
//!
//! Logical state writers trigger `ReferenceStateChanged` before mutating a reference.
//! A changed reference stays on the ordinary stock path for this session, including
//! after streaming unload/reload. Restoring a resting proxy requires a new certified
//! representation, rather than assuming that a later entity spawn resets game state.

use bevy::{
    camera::visibility::VisibilitySystems,
    prelude::*,
    render::extract_resource::{ExtractResource, ExtractResourcePlugin},
    transform::TransformSystems,
};
use std::{collections::BTreeSet, sync::Arc};

use super::source_identity::SourceNodePose;
use crate::world::components::FormId;

/// Emitted by gameplay when a reference is moved, disabled, harvested or otherwise changed.
/// This identifies the placed reference, never its shared mesh or base record.
#[derive(Event, Clone, Copy, Debug)]
pub struct ReferenceStateChanged {
    pub reference: u32,
}

/// The source reference's cell-local resting pose, unaffected by render-origin rebases.
#[derive(Component)]
pub(crate) struct RestingReferencePose(pub(crate) Transform);

/// Suppression is independent of entity lifetime, view, batch and mesh sharing.
#[derive(Resource, Clone, Default, ExtractResource)]
pub(crate) struct RuntimeReferenceState {
    pub(crate) revision: u64,
    references: Arc<BTreeSet<u32>>,
}

impl RuntimeReferenceState {
    pub(crate) fn contains(&self, reference: u32) -> bool {
        self.references.contains(&reference)
    }

    pub(crate) fn references(&self) -> impl Iterator<Item = u32> + '_ {
        self.references.iter().copied()
    }

    fn suppress(&mut self, reference: u32) {
        if self.references.contains(&reference) {
            return;
        }
        Arc::make_mut(&mut self.references).insert(reference);
        self.revision = self.revision.wrapping_add(1);
        info!(
            reference,
            suppressed_references = self.references.len(),
            "cluster resting reference revoked by runtime state change"
        );
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum RuntimeStateSystems {
    Observe,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<RuntimeReferenceState>()
        .add_observer(record_state_change)
        .add_observer(record_disabled_reference)
        .add_systems(
            PostUpdate,
            observe_changed_reference_poses
                .in_set(RuntimeStateSystems::Observe)
                .after(bevy::app::AnimationSystems)
                .before(TransformSystems::Propagate)
                .before(VisibilitySystems::VisibilityPropagate),
        )
        .add_plugins(ExtractResourcePlugin::<RuntimeReferenceState>::default());
}

fn record_state_change(event: On<ReferenceStateChanged>, mut state: ResMut<RuntimeReferenceState>) {
    state.suppress(event.reference);
}

fn record_disabled_reference(
    event: On<Add, bevy::ecs::entity_disabling::Disabled>,
    references: Query<
        &FormId,
        (
            With<RestingReferencePose>,
            bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>,
        ),
    >,
    mut state: ResMut<RuntimeReferenceState>,
) {
    if let Ok(reference) = references.get(event.entity) {
        state.suppress(reference.0);
    }
}

type ChangedRestingRoots<'w, 's> = Query<
    'w,
    's,
    (
        &'static FormId,
        &'static RestingReferencePose,
        &'static Transform,
        Option<&'static Visibility>,
    ),
    Or<(Changed<Transform>, Changed<Visibility>)>,
>;

/// Stock transform/visibility writers that predate the logical state event still revoke
/// the resting instance. Cell retirement and rebasing change its parent, not this pose.
fn observe_changed_reference_poses(
    roots: ChangedRestingRoots,
    changed_nodes: Query<(Entity, &Transform, &SourceNodePose), Changed<Transform>>,
    parents: Query<&ChildOf>,
    identities: Query<&FormId, With<RestingReferencePose>>,
    mut state: ResMut<RuntimeReferenceState>,
) {
    for (form, resting, current, visible) in &roots {
        if *current != resting.0 || visible == Some(&Visibility::Hidden) {
            state.suppress(form.0);
        }
    }
    for (entity, current, resting) in &changed_nodes {
        if *current == resting.0 {
            continue;
        }
        let mut at = Some(entity);
        let mut visited = std::collections::HashSet::new();
        while let Some(node) = at {
            if !visited.insert(node) {
                break;
            }
            if let Ok(reference) = identities.get(node) {
                state.suppress(reference.0);
                break;
            }
            at = parents.get(node).ok().map(ChildOf::parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        install(&mut app);
        app
    }

    #[test]
    fn logical_changes_are_immediate_idempotent_and_reference_local() {
        let mut app = app();
        app.world_mut()
            .trigger(ReferenceStateChanged { reference: 7 });
        let state = app.world().resource::<RuntimeReferenceState>();
        assert!(state.contains(7));
        assert!(!state.contains(8));
        let snapshot = state.clone();
        app.world_mut()
            .trigger(ReferenceStateChanged { reference: 7 });
        assert_eq!(app.world().resource::<RuntimeReferenceState>().revision, 1);
        app.world_mut()
            .trigger(ReferenceStateChanged { reference: 8 });
        assert!(!snapshot.contains(8));
        assert_eq!(
            app.world()
                .resource::<RuntimeReferenceState>()
                .references()
                .collect::<Vec<_>>(),
            [7, 8]
        );
    }

    #[test]
    fn changed_pose_stays_revoked_after_streaming_unload_and_respawn() {
        let mut app = app();
        let resting = Transform::from_xyz(1.0, 2.0, 3.0);
        let root = app
            .world_mut()
            .spawn((FormId(7), RestingReferencePose(resting), resting))
            .id();
        app.update();
        assert!(!app.world().resource::<RuntimeReferenceState>().contains(7));
        app.world_mut()
            .get_mut::<Transform>(root)
            .unwrap()
            .translation
            .x += 1.0;
        app.update();
        assert!(app.world().resource::<RuntimeReferenceState>().contains(7));
        app.world_mut().despawn(root);
        app.world_mut()
            .spawn((FormId(7), RestingReferencePose(resting), resting));
        app.update();
        assert!(app.world().resource::<RuntimeReferenceState>().contains(7));
    }

    #[test]
    fn parent_rebase_does_not_revoke_but_explicit_reference_hide_does() {
        let mut app = app();
        let parent = app.world_mut().spawn(Transform::default()).id();
        let root = app
            .world_mut()
            .spawn((
                FormId(7),
                RestingReferencePose(Transform::default()),
                Transform::default(),
                Visibility::Inherited,
                ChildOf(parent),
            ))
            .id();
        app.update();
        app.world_mut()
            .get_mut::<Transform>(parent)
            .unwrap()
            .translation
            .x = 4096.0;
        app.update();
        assert!(!app.world().resource::<RuntimeReferenceState>().contains(7));
        *app.world_mut().get_mut::<Visibility>(root).unwrap() = Visibility::Hidden;
        app.update();
        assert!(app.world().resource::<RuntimeReferenceState>().contains(7));
    }

    #[test]
    fn disabled_root_and_animated_child_revoke_their_own_resting_references() {
        let mut app = app();
        let root = app
            .world_mut()
            .spawn((
                FormId(7),
                RestingReferencePose(Transform::default()),
                Transform::default(),
            ))
            .id();
        let other = app
            .world_mut()
            .spawn((
                FormId(8),
                RestingReferencePose(Transform::default()),
                Transform::default(),
            ))
            .id();
        let child = app
            .world_mut()
            .spawn((
                ChildOf(other),
                SourceNodePose(Transform::default()),
                Transform::default(),
            ))
            .id();
        app.update();
        assert_eq!(app.world().resource::<RuntimeReferenceState>().revision, 0);
        app.world_mut()
            .entity_mut(root)
            .insert(bevy::ecs::entity_disabling::Disabled);
        assert!(app.world().resource::<RuntimeReferenceState>().contains(7));
        assert!(!app.world().resource::<RuntimeReferenceState>().contains(8));
        app.world_mut()
            .get_mut::<Transform>(child)
            .unwrap()
            .rotation = Quat::from_rotation_y(0.5);
        app.update();
        assert!(app.world().resource::<RuntimeReferenceState>().contains(8));
    }

    #[test]
    fn animation_writes_revoke_the_resting_reference_in_the_same_frame() {
        let mut app = app();
        let root = app
            .world_mut()
            .spawn((
                FormId(7),
                RestingReferencePose(Transform::default()),
                Transform::default(),
            ))
            .id();
        app.world_mut().spawn((
            ChildOf(root),
            SourceNodePose(Transform::default()),
            Transform::default(),
            bevy::animation::AnimationTargetId::from_name(&Name::new("moving")),
        ));
        app.add_systems(
            PostUpdate,
            (|mut nodes: Query<&mut Transform, With<bevy::animation::AnimationTargetId>>| {
                for mut pose in &mut nodes {
                    pose.translation.x += 1.0;
                }
            })
            .in_set(bevy::app::AnimationSystems),
        );
        app.update();
        assert!(app.world().resource::<RuntimeReferenceState>().contains(7));
    }
}
