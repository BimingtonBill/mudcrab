//! Launch-time primitive ownership; no mesh handles or paths participate in matching.

use super::{
    reader::{CertifiedPackage, RecordKey},
    source_identity::SourcePrimitive,
};
use bevy::{ecs::system::SystemParam, prelude::*, world_serialization::WorldInstanceReady};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Owned,
    Deferred,
    NotOwned,
}

/// One live certificate's identity index and terminal batch qualifications.
/// Missing batch entries are pending, never an implicit permission to hide.
#[derive(Resource, Default)]
pub(crate) struct OwnershipFacts {
    certificate: Option<Arc<CertifiedPackage>>,
    lookup: HashMap<(u32, u32), RecordKey>,
    references: HashMap<u32, Vec<RecordKey>>,
    source_fallback: BTreeMap<RecordKey, bool>,
}

/// Runtime reference identity, attached only to exterior static candidates.
#[derive(Component)]
pub(crate) struct OwnershipReference(pub(crate) u32);

#[derive(Component)]
struct OwnershipApplied;

type OwnershipPrimitiveQuery<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static SourcePrimitive>,
        &'static mut Visibility,
        Has<OwnershipApplied>,
    ),
    With<Mesh3d>,
>;

#[derive(Component)]
struct OwnershipDeferred;

/// Only unresolved, uninstantiated models are polled. Holding the existing pending
/// model/profile markers keeps cell readiness honest, and prevents a stock draw
/// before the ready observer has enough facts to make its one-time decision.
#[derive(SystemParam)]
pub(crate) struct OwnershipGate<'w, 's> {
    facts: Option<Res<'w, OwnershipFacts>>,
    counters: Option<ResMut<'w, OwnershipCounters>>,
    references: Query<'w, 's, &'static OwnershipReference>,
    deferred: Query<'w, 's, (), With<OwnershipDeferred>>,
}

impl OwnershipGate<'_, '_> {
    pub(crate) fn begin_frame(&mut self) {
        if let Some(counters) = self.counters.as_deref_mut() {
            counters.waiting_certificate = 0;
            counters.waiting_batches = 0;
        }
    }

    pub(crate) fn can_arm(
        &mut self,
        entity: Entity,
        template: Option<&WorldAsset>,
        commands: &mut Commands,
    ) -> bool {
        let Some(facts) = self.facts.as_deref() else {
            return true;
        };
        let Ok(reference) = self.references.get(entity) else {
            return true;
        };
        if facts.can_spawn(reference.0) {
            return true;
        }
        // An asset load failure must still reach the ordinary readiness/error
        // path. There is no instantiated primitive to hide in this case.
        let Some(template) = template else {
            return true;
        };
        let first_count = if self.deferred.contains(entity) {
            None
        } else {
            let count = template
                .world
                .try_query::<&Mesh3d>()
                .map_or(0, |mut meshes| meshes.iter(&template.world).count() as u64);
            if count == 0 {
                return true;
            }
            Some(count)
        };
        let Some(counters) = self.counters.as_deref_mut() else {
            return false;
        };
        if facts.certificate.is_none() {
            counters.waiting_certificate = counters.waiting_certificate.saturating_add(1);
        } else {
            counters.waiting_batches = counters.waiting_batches.saturating_add(1);
        }
        if let Some(count) = first_count {
            counters.owned_deferred = counters.owned_deferred.saturating_add(count);
            commands.entity(entity).try_insert(OwnershipDeferred);
        }
        false
    }

    pub(crate) fn finish_frame(&mut self) {
        if let Some(counters) = self.counters.as_deref_mut() {
            counters.log_deferral();
        }
    }
}

#[derive(Resource, Default)]
struct OwnershipCounters {
    owned_hidden: u64,
    /// Candidate primitive decisions deferred once before instantiation, including
    /// candidates later proved not owned when the certificate arrives.
    owned_deferred: u64,
    not_owned: u64,
    waiting_certificate: u64,
    waiting_batches: u64,
    defer_logged: bool,
    frames: u64,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<OwnershipFacts>()
        .init_resource::<OwnershipCounters>()
        .add_observer(hide_owned_instance)
        .add_systems(Last, report);
    info!("cluster static ownership experiment enabled");
}

impl OwnershipFacts {
    #[cfg(test)]
    pub(crate) fn publish(
        &mut self,
        package: &Arc<CertifiedPackage>,
        records: BTreeMap<RecordKey, bool>,
    ) {
        self.publish_updates(
            package,
            true,
            records.into_iter().map(|(key, value)| (key, Some(value))),
        );
    }

    /// Change only the affected instance decisions, retaining this live
    /// certificate's occurrence lookup. `None` withdraws a terminal decision.
    pub(crate) fn publish_updates(
        &mut self,
        package: &Arc<CertifiedPackage>,
        reset: bool,
        records: impl IntoIterator<Item = (RecordKey, Option<bool>)>,
    ) {
        if !self
            .certificate
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, package))
        {
            self.source_fallback.clear();
            self.lookup.clear();
            self.references.clear();
            for (key, identity) in package.records() {
                self.lookup
                    .insert((identity.reference, identity.primitive), *key);
                self.references
                    .entry(identity.reference)
                    .or_default()
                    .push(*key);
            }
            self.certificate = Some(package.clone());
        }
        if reset {
            self.source_fallback.clear();
        }
        for (key, value) in records {
            if let Some(value) = value {
                self.source_fallback.insert(key, value);
            } else {
                self.source_fallback.remove(&key);
            }
        }
    }

    pub(crate) fn revoke(&mut self) {
        *self = Self::default();
    }

    fn decision(&self, reference: u32, tag: Option<&SourcePrimitive>) -> Decision {
        if self.certificate.is_none() {
            return Decision::Deferred;
        }
        let Some(tag) = tag.filter(|tag| tag.matches_default_scene) else {
            return Decision::NotOwned;
        };
        let Some(key) = self.lookup.get(&(reference, tag.ordinal)) else {
            return Decision::NotOwned;
        };
        match self.source_fallback.get(key) {
            Some(false) => Decision::Owned,
            Some(true) => Decision::NotOwned,
            None => Decision::Deferred,
        }
    }

    pub(crate) fn can_spawn(&self, reference: u32) -> bool {
        self.certificate.is_some()
            && self
                .references
                .get(&reference)
                .into_iter()
                .flatten()
                .all(|key| self.source_fallback.contains_key(key))
    }

    #[cfg(test)]
    pub(crate) fn projected_records(&self) -> &BTreeMap<RecordKey, bool> {
        &self.source_fallback
    }
}

fn hide_owned_instance(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    facts: Res<OwnershipFacts>,
    mut counters: ResMut<OwnershipCounters>,
    references: Query<&OwnershipReference>,
    children: Query<&Children>,
    mut primitives: OwnershipPrimitiveQuery,
) {
    let Ok(reference) = references.get(ready.entity) else {
        return;
    };
    // WorldInstanceReady is flushed in SpawnScene, before PostUpdate visibility
    // propagation and render extraction. Write Visibility directly here, on the
    // cloned primitive only; no root, mesh, material or template is changed.
    for entity in children.iter_descendants(ready.entity) {
        let Ok((tag, mut visibility, applied)) = primitives.get_mut(entity) else {
            continue;
        };
        if applied {
            continue;
        }
        match facts.decision(reference.0, tag) {
            Decision::Owned => {
                *visibility = Visibility::Hidden;
                counters.owned_hidden = counters.owned_hidden.saturating_add(1);
            }
            Decision::NotOwned => counters.not_owned = counters.not_owned.saturating_add(1),
            Decision::Deferred => {
                // The ordinary streamer holds such models before instantiation.
                // An external spawner must not turn missing authority into a hide.
                counters.owned_deferred = counters.owned_deferred.saturating_add(1);
                continue;
            }
        }
        commands.entity(entity).try_insert(OwnershipApplied);
    }
    counters.log_deferral();
}

impl OwnershipCounters {
    fn log_deferral(&mut self) {
        if self.owned_deferred > 0 && !self.defer_logged {
            self.defer_logged = true;
            info!(
                owned_deferred = self.owned_deferred,
                waiting_certificate = self.waiting_certificate,
                waiting_batches = self.waiting_batches,
                "cluster ownership deferred: certificate or batch/template/tangent qualification unavailable; models remain pending"
            );
        }
    }
}

fn report(mut counters: ResMut<OwnershipCounters>) {
    counters.frames = counters.frames.wrapping_add(1);
    if counters.frames.is_multiple_of(600) {
        info!(
            owned_hidden = counters.owned_hidden,
            owned_deferred = counters.owned_deferred,
            not_owned = counters.not_owned,
            waiting_certificate = counters.waiting_certificate,
            waiting_batches = counters.waiting_batches,
            "cluster static ownership counters"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        reader::{CertifiedPackage, RecordKey, producer_fixture_tests::published_for_loader},
        source_identity::SourcePrimitive,
    };
    use std::{collections::BTreeMap, sync::Arc};

    // The production matcher must use occurrence identity, retain refused batches,
    // and distinguish unavailable authority from a certified negative.
    fn fixture() -> (
        tempfile::TempDir,
        Arc<CertifiedPackage>,
        u32,
        SourcePrimitive,
        RecordKey,
    ) {
        let directory = published_for_loader();
        let package = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        let (key, identity) = package.records().next().unwrap();
        let result = (
            identity.reference,
            SourcePrimitive {
                scene_index: 0,
                ordinal: identity.primitive,
                mesh_index: 0,
                primitive_index: 0,
                matches_default_scene: true,
            },
            *key,
        );
        (directory, package, result.0, result.1, result.2)
    }

    #[test]
    fn certified_reference_and_occurrence_are_owned() {
        let (_directory, package, reference, tag, key) = fixture();
        let mut facts = super::OwnershipFacts::default();
        facts.publish(&package, BTreeMap::from([(key, false)]));
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::Owned
        );
        let another_occurrence = SourcePrimitive {
            ordinal: u32::MAX,
            ..tag
        };
        assert_eq!(
            facts.decision(reference, Some(&another_occurrence)),
            super::Decision::NotOwned
        );
        assert_eq!(
            facts.decision(u32::MAX, Some(&tag)),
            super::Decision::NotOwned
        );
    }

    #[test]
    fn refused_batch_and_nondefault_scene_stay_stock() {
        let (_directory, package, reference, tag, key) = fixture();
        let mut facts = super::OwnershipFacts::default();
        facts.publish(&package, BTreeMap::from([(key, true)]));
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::NotOwned
        );
        facts.publish(&package, BTreeMap::from([(key, false)]));
        let nondefault = SourcePrimitive {
            matches_default_scene: false,
            ..tag
        };
        assert_eq!(
            facts.decision(reference, Some(&nondefault)),
            super::Decision::NotOwned
        );
        assert_eq!(facts.decision(reference, None), super::Decision::NotOwned);
    }

    #[test]
    fn unavailable_certificate_or_batch_decision_defers_without_guessing() {
        let (_directory, package, reference, tag, _key) = fixture();
        let mut facts = super::OwnershipFacts::default();
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::Deferred
        );
        assert!(!facts.can_spawn(reference));
        facts.publish(&package, BTreeMap::new());
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::Deferred
        );
        assert!(!facts.can_spawn(reference));
        assert!(facts.can_spawn(u32::MAX));
    }

    #[test]
    fn withdrawn_qualification_does_not_reuse_a_previous_owned_decision() {
        let (directory, package, reference, tag, key) = fixture();
        let mut facts = super::OwnershipFacts::default();
        facts.publish(&package, BTreeMap::from([(key, false)]));
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::Owned
        );
        facts.publish(&package, BTreeMap::new());
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::Deferred
        );
        assert!(!facts.can_spawn(reference));
        facts.publish(&package, BTreeMap::from([(key, false)]));
        let replacement = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        facts.publish(&replacement, BTreeMap::new());
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::Deferred
        );
    }

    #[test]
    fn real_instance_is_hidden_before_its_first_visibility_pass_and_keeps_meshes() {
        use super::super::source_identity::actual_loader_tests::{
            loader_app_with, wait_for_loaded,
        };
        use bevy::{camera::visibility::VisibilitySystems, prelude::*};
        let (directory, package, reference, _tag, _key) = fixture();
        let path = package.reference_model_path(reference).unwrap();
        let mut app = loader_app_with(directory.path(), |app| {
            app.add_plugins((TransformPlugin, bevy::camera::visibility::VisibilityPlugin));
        });
        super::install(&mut app);
        app.world_mut()
            .resource_mut::<super::OwnershipFacts>()
            .publish(
                &package,
                package.records().map(|(key, _)| (*key, false)).collect(),
            );
        let handle: Handle<Gltf> = app.world().resource::<AssetServer>().load(path.to_owned());
        wait_for_loaded(&mut app, &handle);
        let scene = app
            .world()
            .resource::<Assets<Gltf>>()
            .get(&handle)
            .unwrap()
            .scenes[0]
            .clone();
        // The loader's primitive template may lack Visibility: the destination
        // world's VisibilityPlugin supplies it as a Mesh3d required component.
        let template_visibility_before = {
            let template = &app
                .world()
                .resource::<Assets<WorldAsset>>()
                .get(&scene)
                .unwrap()
                .world;
            let mut meshes = template
                .try_query::<(Entity, &Mesh3d, Option<&Visibility>)>()
                .unwrap();
            meshes
                .iter(template)
                .map(|(entity, _, visibility)| (entity, visibility.copied()))
                .collect::<BTreeMap<_, _>>()
        };
        let root = app
            .world_mut()
            .spawn((
                super::OwnershipReference(reference),
                WorldAssetRoot(scene.clone()),
                Transform::IDENTITY,
                Visibility::Inherited,
            ))
            .id();
        app.add_systems(
            PostUpdate,
            (move |children: Query<&Children>, meshes: Query<&Visibility, With<Mesh3d>>| {
                for child in children.iter_descendants(root) {
                    if let Ok(visibility) = meshes.get(child) {
                        assert_eq!(
                            *visibility,
                            Visibility::Hidden,
                            "first visibility pass saw an owned stock mesh"
                        );
                    }
                }
            })
            .before(VisibilitySystems::VisibilityPropagate),
        );
        app.update();
        let world = app.world();
        let mut meshes = world.try_query::<(&Mesh3d, &Visibility)>().unwrap();
        assert!(
            meshes
                .iter(world)
                .any(|(_, visibility)| *visibility == Visibility::Hidden)
        );
        assert_eq!(world.get::<Visibility>(root), Some(&Visibility::Inherited));
        let template = &world
            .resource::<Assets<WorldAsset>>()
            .get(&scene)
            .unwrap()
            .world;
        let mut template_meshes = template
            .try_query::<(Entity, &Mesh3d, Option<&Visibility>)>()
            .unwrap();
        let template_visibility_after = template_meshes
            .iter(template)
            .map(|(entity, _, visibility)| (entity, visibility.copied()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(template_visibility_after, template_visibility_before);
        let template_count = template_visibility_before.len() as u64;
        assert_eq!(meshes.iter(world).count() as u64, template_count);
        assert_eq!(
            world.resource::<super::OwnershipCounters>().owned_hidden,
            template_count
        );
        // A duplicate ready notification must not reapply ownership or count it twice.
        let instance_id = **world
            .get::<bevy::world_serialization::WorldInstance>(root)
            .unwrap();
        app.world_mut()
            .trigger(bevy::world_serialization::WorldInstanceReady {
                entity: root,
                instance_id,
            });
        app.world_mut().flush();
        assert_eq!(
            app.world()
                .resource::<super::OwnershipCounters>()
                .owned_hidden,
            template_count
        );
    }
}
