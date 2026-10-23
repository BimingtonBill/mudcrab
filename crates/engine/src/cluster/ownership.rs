//! Launch-time primitive ownership; no mesh handles or paths participate in matching.

use super::{
    instance_identity::ClusterInstanceIdentity,
    package::{BatchDemand, LazyBatches},
    reader::{CertifiedPackage, RecordKey},
    render::ClusterScene,
    source_identity::SourcePrimitive,
};
use bevy::{ecs::system::SystemParam, prelude::*, world_serialization::WorldInstanceReady};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
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
struct OwnershipApplied(Option<Visibility>);

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
pub(super) struct OwnershipDeferred;

/// Only unresolved, uninstantiated models are polled. Holding the existing pending
/// model/profile markers keeps cell readiness honest, and prevents a stock draw
/// before the ready observer has enough facts to make its one-time decision.
#[derive(SystemParam)]
pub(crate) struct OwnershipGate<'w, 's> {
    facts: Option<Res<'w, OwnershipFacts>>,
    runtime_state: Option<Res<'w, super::runtime_state::RuntimeReferenceState>>,
    counters: Option<ResMut<'w, OwnershipCounters>>,
    demand: Option<ResMut<'w, OwnershipBatchDemand>>,
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
        if self
            .runtime_state
            .as_ref()
            .is_some_and(|state| state.contains(reference.0))
            || facts.can_spawn(reference.0)
        {
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
            // Its decision waits for its batches, which a view alone may never request.
            if let Some(demand) = self.demand.as_deref_mut() {
                demand.push(reference.0);
            }
        }
        false
    }

    pub(crate) fn finish_frame(&mut self) {
        if let Some(counters) = self.counters.as_deref_mut() {
            counters.log_deferral();
        }
    }
}

/// The references the gate holds for their one-time decision, turned into requests for the
/// cluster batches that decide them. A batch is otherwise requested only once a view sees one of
/// its instances, while a held reference anywhere in the loaded grid (behind the camera, or with
/// no placed world a view can match) waits for its batches' qualification: without this demand it
/// is never decided and the world never becomes ready.
#[derive(Resource, Default)]
pub(crate) struct OwnershipBatchDemand {
    /// Held references not yet turned into batch requests.
    pending: Vec<u32>,
    /// Every reference ever demanded. Batches are never unloaded, so a reference that comes
    /// back with its cell needs no second request; a new batch layout re-demands them all.
    demanded: HashSet<u32>,
    /// The [`LazyBatches`] layout the demanded references were resolved against.
    layout_scope: Option<u64>,
    /// Batches this demand had to request because no view had.
    queued_batches: u64,
    /// Demanded batches whose source or material failed terminally.
    failed_batches: u64,
    /// Certified records with no instance in the cluster scene: no batch can decide them.
    unplaced_records: u64,
}

impl OwnershipBatchDemand {
    fn push(&mut self, reference: u32) {
        if self.demanded.insert(reference) {
            self.pending.push(reference);
        }
    }

    fn needs_resolution(&self, layout_scope: u64) -> bool {
        !self.pending.is_empty() || self.layout_scope != Some(layout_scope)
    }
}

/// The batch of every certified object record in one cluster scene, by record block, so a
/// held reference's records map to their batches without a pass over the scene's instances.
pub(super) struct RecordBatches {
    by_block: HashMap<(u32, u32, [i32; 2]), Vec<Option<u32>>>,
}

impl RecordBatches {
    fn new(scene: &ClusterScene) -> Self {
        let mut by_block: HashMap<_, Vec<Option<u32>>> = HashMap::new();
        for instance in &scene.instances {
            let ClusterInstanceIdentity::Object(key) = instance.identity else {
                continue;
            };
            let ordinals = by_block
                .entry((key.worldspace, key.tier, key.anchor))
                .or_default();
            let ordinal = key.ordinal as usize;
            if ordinals.len() <= ordinal {
                ordinals.resize(ordinal + 1, None);
            }
            ordinals[ordinal] = Some(instance.batch);
        }
        Self { by_block }
    }

    fn batch(&self, key: &RecordKey) -> Option<u32> {
        self.by_block
            .get(&(key.worldspace, key.tier, key.anchor))?
            .get(key.ordinal as usize)
            .copied()
            .flatten()
    }
}

/// Runs the resolver only while held references wait for it, or the batch layout changed.
pub(super) fn batch_demand_pending(
    demand: Option<Res<OwnershipBatchDemand>>,
    lazy: Option<Res<LazyBatches>>,
) -> bool {
    demand
        .zip(lazy)
        .is_some_and(|(demand, lazy)| demand.needs_resolution(lazy.layout_scope))
}

/// Requests the batches of every held reference's certified records, regardless of the views.
/// Event-driven: it runs only for newly held references (see [`batch_demand_pending`]), and keeps
/// them until the certificate their records come from is published for the current scene.
pub(super) fn demand_owned_batches(
    mut demand: ResMut<OwnershipBatchDemand>,
    mut lazy: ResMut<LazyBatches>,
    facts: Option<Res<OwnershipFacts>>,
    scene: Option<Res<ClusterScene>>,
    held: Query<&OwnershipReference, With<OwnershipDeferred>>,
    mut index: Local<Option<RecordBatches>>,
) {
    let demand = &mut *demand;
    if demand.layout_scope != Some(lazy.layout_scope) {
        // A new batch layout lost every earlier request: demand again the references still held,
        // not every one ever held (a released reference needs no batch).
        if demand.layout_scope.is_some() {
            demand.demanded = held.iter().map(|reference| reference.0).collect();
            demand.pending = demand.demanded.iter().copied().collect();
        }
        demand.layout_scope = Some(lazy.layout_scope);
    }
    if demand.pending.is_empty() {
        return;
    }
    let (Some(facts), Some(scene)) = (facts, scene) else {
        return;
    };
    // Records name batches only in the scene of the certificate they were published from.
    let coherent = facts
        .certificate
        .as_ref()
        .zip(scene.certificate.as_ref())
        .is_some_and(|(facts, scene)| Arc::ptr_eq(facts, scene));
    if !coherent {
        return;
    }
    if index.is_none() || scene.is_changed() {
        *index = Some(RecordBatches::new(&scene));
    }
    let Some(index) = index.as_ref() else {
        return;
    };
    let references = std::mem::take(&mut demand.pending);
    let mut batches = BTreeSet::new();
    let mut unplaced = 0u64;
    for reference in &references {
        for key in facts.references.get(reference).into_iter().flatten() {
            match index.batch(key) {
                Some(batch) => {
                    batches.insert(batch);
                }
                None => unplaced += 1,
            }
        }
    }
    let (mut queued, mut failed) = (0u64, 0u64);
    for &batch in &batches {
        match lazy.demand(batch) {
            Some(BatchDemand::Queued) => queued += 1,
            Some(BatchDemand::Failed) => failed += 1,
            Some(BatchDemand::Underway) => {}
            // A scene instance naming a batch the layout lacks is an unplaced record too.
            None => unplaced += 1,
        }
    }
    demand.queued_batches += queued;
    demand.failed_batches += failed;
    demand.unplaced_records += unplaced;
    if queued > 0 {
        info!(
            references = references.len(),
            batches = batches.len(),
            queued,
            "cluster ownership requested the batches that decide held references, which no view had requested"
        );
    }
    if failed > 0 || unplaced > 0 {
        error!(
            references = references.len(),
            failed_batches = failed,
            unplaced_records = unplaced,
            "cluster ownership cannot decide held references: their batches failed or their records have no scene instance; the references stay pending"
        );
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
        .init_resource::<OwnershipBatchDemand>()
        .add_observer(hide_owned_instance)
        .add_observer(revoke_owned_reference)
        .add_systems(
            PostUpdate,
            restore_changed_owned_references
                .after(super::runtime_state::RuntimeStateSystems::Observe)
                .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
        )
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
    (facts, runtime_state): (
        Res<OwnershipFacts>,
        Option<Res<super::runtime_state::RuntimeReferenceState>>,
    ),
    mut counters: ResMut<OwnershipCounters>,
    references: Query<&OwnershipReference>,
    children: Query<&Children>,
    mut primitives: OwnershipPrimitiveQuery,
) {
    let Ok(reference) = references.get(ready.entity) else {
        return;
    };
    if runtime_state
        .as_ref()
        .is_some_and(|state| state.contains(reference.0))
    {
        return;
    }
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
        let original = *visibility;
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
        commands.entity(entity).try_insert(OwnershipApplied(
            (*visibility == Visibility::Hidden && original != Visibility::Hidden)
                .then_some(original),
        ));
    }
    counters.log_deferral();
}

/// Restore only visibility this ownership experiment changed, before propagation.
/// The runtime state survives streaming, so a later ready observer cannot hide it again.
type IncludeDisabled = bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>;

#[derive(SystemParam)]
struct RestoreOwnership<'w, 's> {
    commands: Commands<'w, 's>,
    references: Query<'w, 's, (Entity, &'static OwnershipReference), IncludeDisabled>,
    children: Query<'w, 's, &'static Children, IncludeDisabled>,
    primitives:
        Query<'w, 's, (&'static mut Visibility, &'static mut OwnershipApplied), IncludeDisabled>,
}

impl RestoreOwnership<'_, '_> {
    fn matching(&mut self, matches: impl Fn(u32) -> bool) {
        for (root, reference) in &self.references {
            if !matches(reference.0) {
                continue;
            }
            for entity in self.children.iter_descendants(root) {
                if let Ok((mut visibility, mut applied)) = self.primitives.get_mut(entity) {
                    // Consume the permission immediately, even before deferred removal.
                    // A state writer may set a new hide after triggering revocation.
                    if let Some(original) = applied.0.take() {
                        *visibility = original;
                    }
                    self.commands.entity(entity).remove::<OwnershipApplied>();
                }
            }
        }
    }
}

fn revoke_owned_reference(
    event: On<super::runtime_state::ReferenceStateChanged>,
    mut restore: RestoreOwnership,
) {
    restore.matching(|reference| reference == event.reference);
}

fn restore_changed_owned_references(
    state: Option<Res<super::runtime_state::RuntimeReferenceState>>,
    mut previous: Local<u64>,
    mut restore: RestoreOwnership,
) {
    let Some(state) = state else {
        return;
    };
    if *previous == state.revision {
        return;
    }
    *previous = state.revision;
    restore.matching(|reference| state.contains(reference));
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

fn report(mut counters: ResMut<OwnershipCounters>, demand: Option<Res<OwnershipBatchDemand>>) {
    counters.frames = counters.frames.wrapping_add(1);
    if counters.frames.is_multiple_of(600) {
        let demand = demand.as_deref();
        info!(
            owned_hidden = counters.owned_hidden,
            owned_deferred = counters.owned_deferred,
            not_owned = counters.not_owned,
            waiting_certificate = counters.waiting_certificate,
            waiting_batches = counters.waiting_batches,
            demanded_references = demand.map_or(0, |demand| demand.demanded.len()),
            demand_queued_batches = demand.map_or(0, |demand| demand.queued_batches),
            demand_failed_batches = demand.map_or(0, |demand| demand.failed_batches),
            demand_unplaced_records = demand.map_or(0, |demand| demand.unplaced_records),
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

#[cfg(test)]
mod runtime_revocation_tests {
    use super::*;
    use crate::cluster::runtime_state::{ReferenceStateChanged, RuntimeReferenceState};

    #[test]
    fn revocation_restores_owned_visibility_but_preserves_authored_hides() {
        let mut app = App::new();
        crate::cluster::runtime_state::install(&mut app);
        install(&mut app);
        let root = app.world_mut().spawn(OwnershipReference(7)).id();
        let owned = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Visibility::Hidden,
                OwnershipApplied(Some(Visibility::Inherited)),
            ))
            .id();
        let authored = app
            .world_mut()
            .spawn((ChildOf(root), Visibility::Hidden, OwnershipApplied(None)))
            .id();
        let other_root = app.world_mut().spawn(OwnershipReference(8)).id();
        let other = app
            .world_mut()
            .spawn((
                ChildOf(other_root),
                Visibility::Hidden,
                OwnershipApplied(Some(Visibility::Inherited)),
            ))
            .id();
        app.world_mut()
            .trigger(ReferenceStateChanged { reference: 7 });
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(owned),
            Some(&Visibility::Inherited)
        );
        assert_eq!(
            app.world().get::<Visibility>(authored),
            Some(&Visibility::Hidden)
        );
        assert_eq!(
            app.world().get::<Visibility>(other),
            Some(&Visibility::Hidden)
        );
        assert!(app.world().get::<OwnershipApplied>(owned).is_none());
        assert!(app.world().resource::<RuntimeReferenceState>().contains(7));
    }

    #[test]
    fn synchronous_revocation_preserves_a_later_gameplay_hide() {
        let mut app = App::new();
        crate::cluster::runtime_state::install(&mut app);
        install(&mut app);
        let root = app.world_mut().spawn(OwnershipReference(7)).id();
        let child = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Visibility::Hidden,
                OwnershipApplied(Some(Visibility::Inherited)),
            ))
            .id();
        app.world_mut()
            .trigger(ReferenceStateChanged { reference: 7 });
        assert_eq!(
            app.world().get::<Visibility>(child),
            Some(&Visibility::Inherited)
        );
        *app.world_mut().get_mut::<Visibility>(child).unwrap() = Visibility::Hidden;
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(child),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn disabled_reference_restores_its_stock_before_reenable() {
        use crate::cluster::runtime_state::RestingReferencePose;
        use bevy::ecs::entity_disabling::Disabled;
        let mut app = App::new();
        crate::cluster::runtime_state::install(&mut app);
        install(&mut app);
        let root = app
            .world_mut()
            .spawn((
                OwnershipReference(7),
                crate::world::components::FormId(7),
                RestingReferencePose(Transform::default()),
                Transform::default(),
            ))
            .id();
        let child = app
            .world_mut()
            .spawn((
                ChildOf(root),
                Visibility::Hidden,
                OwnershipApplied(Some(Visibility::Inherited)),
            ))
            .id();
        app.world_mut().entity_mut(root).insert(Disabled);
        app.update();
        app.world_mut().entity_mut(root).remove::<Disabled>();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(child),
            Some(&Visibility::Inherited)
        );
        assert!(app.world().get::<OwnershipApplied>(child).is_none());
        assert!(app.world().resource::<RuntimeReferenceState>().contains(7));
    }
}

#[cfg(test)]
mod batch_demand_tests {
    use super::*;
    use crate::cluster::render::ClusterInstance;
    use bevy::ecs::system::RunSystemOnce;

    struct Fixture {
        _directory: tempfile::TempDir,
        package: Arc<CertifiedPackage>,
        reference: u32,
    }

    fn fixture() -> Fixture {
        let directory = super::super::reader::producer_fixture_tests::published_for_loader();
        let package = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        let reference = package.records().next().unwrap().1.reference;
        Fixture {
            _directory: directory,
            package,
            reference,
        }
    }

    /// A scene placing each of the package's records in `batch`, or none at all.
    fn scene(package: &Arc<CertifiedPackage>, batch: Option<u32>) -> ClusterScene {
        ClusterScene {
            certificate: Some(package.clone()),
            terrain_blocks: Vec::new(),
            meshes: Vec::new(),
            instances: batch
                .into_iter()
                .flat_map(|batch| {
                    package.records().map(move |(key, _)| ClusterInstance {
                        identity: ClusterInstanceIdentity::Object(*key),
                        mesh: 0,
                        batch,
                        world_from_local: Mat4::IDENTITY,
                        source_worldspace: key.worldspace,
                    })
                })
                .collect(),
            origin_offset: Vec3::ZERO,
            blocks: Vec::new(),
        }
    }

    /// Facts published with no batch decision yet: the fixture reference is held.
    fn world(fixture: &Fixture, batch: Option<u32>) -> World {
        let mut world = World::new();
        let mut facts = OwnershipFacts::default();
        facts.publish(&fixture.package, BTreeMap::new());
        assert!(!facts.can_spawn(fixture.reference));
        world.insert_resource(facts);
        world.insert_resource(scene(&fixture.package, batch));
        world.insert_resource(LazyBatches::unrequested_for_test(3, Vec3::ZERO));
        world.init_resource::<OwnershipBatchDemand>();
        world
    }

    fn resolve(world: &mut World) {
        let pending = world.run_system_once(batch_demand_pending).unwrap();
        if pending {
            world.run_system_once(demand_owned_batches).unwrap();
        }
    }

    #[test]
    fn held_reference_demands_exactly_the_batch_of_its_records() {
        let fixture = fixture();
        let mut world = world(&fixture, Some(2));
        world
            .resource_mut::<OwnershipBatchDemand>()
            .push(fixture.reference);
        resolve(&mut world);
        {
            let mut lazy = world.resource_mut::<LazyBatches>();
            assert_eq!(lazy.demand(2), Some(BatchDemand::Underway));
            assert_eq!(
                lazy.demand(0),
                Some(BatchDemand::Queued),
                "only batch 2 was demanded"
            );
        }
        let demand = world.resource::<OwnershipBatchDemand>();
        assert!(demand.pending.is_empty());
        assert_eq!(demand.queued_batches, 1);
        assert_eq!(demand.unplaced_records, 0);
        assert!(
            !world.run_system_once(batch_demand_pending).unwrap(),
            "a resolved demand does no further work"
        );
    }

    #[test]
    fn demand_waits_for_the_certificate_of_the_current_scene() {
        let fixture = fixture();
        let mut world = world(&fixture, Some(1));
        world.insert_resource(OwnershipFacts::default());
        world
            .resource_mut::<OwnershipBatchDemand>()
            .push(fixture.reference);
        resolve(&mut world);
        assert_eq!(
            world.resource::<OwnershipBatchDemand>().pending,
            [fixture.reference],
            "no certificate: the records and their batches are unknown"
        );
        world
            .resource_mut::<OwnershipFacts>()
            .publish(&fixture.package, BTreeMap::new());
        resolve(&mut world);
        assert!(world.resource::<OwnershipBatchDemand>().pending.is_empty());
        assert_eq!(
            world.resource_mut::<LazyBatches>().demand(1),
            Some(BatchDemand::Underway)
        );
    }

    #[test]
    fn record_without_scene_instance_is_reported_not_guessed() {
        let fixture = fixture();
        let mut world = world(&fixture, None);
        world
            .resource_mut::<OwnershipBatchDemand>()
            .push(fixture.reference);
        resolve(&mut world);
        let demand = world.resource::<OwnershipBatchDemand>();
        assert!(demand.unplaced_records > 0);
        assert_eq!(demand.queued_batches, 0);
        assert!(
            !world
                .resource::<OwnershipFacts>()
                .can_spawn(fixture.reference),
            "an unplaced record is never turned into a decision"
        );
    }

    #[test]
    fn new_batch_layout_demands_only_the_references_still_held() {
        let fixture = fixture();
        let mut world = world(&fixture, Some(0));
        world
            .resource_mut::<OwnershipBatchDemand>()
            .push(fixture.reference);
        resolve(&mut world);
        world
            .resource_mut::<OwnershipBatchDemand>()
            .push(fixture.reference);
        assert!(
            !world.run_system_once(batch_demand_pending).unwrap(),
            "a reference that comes back with its cell is not demanded twice"
        );
        // A reference demanded once and since released (its model spawned or dropped) holds no
        // `OwnershipDeferred` marker; only the one still held is demanded again.
        let released = fixture.reference.wrapping_add(1);
        world.resource_mut::<OwnershipBatchDemand>().push(released);
        resolve(&mut world);
        world.spawn((OwnershipReference(fixture.reference), OwnershipDeferred));
        world.insert_resource(LazyBatches::unrequested_for_test(3, Vec3::ZERO));
        resolve(&mut world);
        assert_eq!(
            world.resource_mut::<LazyBatches>().demand(0),
            Some(BatchDemand::Underway)
        );
        assert_eq!(world.resource::<OwnershipBatchDemand>().queued_batches, 2);
        assert_eq!(
            world.resource::<OwnershipBatchDemand>().demanded,
            HashSet::from([fixture.reference])
        );
    }

    #[test]
    fn gate_demands_the_batches_of_a_reference_it_holds() {
        use super::super::source_identity::actual_loader_tests::{
            loader_app_with, wait_for_loaded,
        };
        use bevy::gltf::Gltf;
        let fixture = fixture();
        let path = fixture
            .package
            .reference_model_path(fixture.reference)
            .unwrap();
        let mut app = loader_app_with(fixture._directory.path(), |_| {});
        install(&mut app);
        app.world_mut()
            .resource_mut::<OwnershipFacts>()
            .publish(&fixture.package, BTreeMap::new());
        let handle: Handle<Gltf> = app.world().resource::<AssetServer>().load(path.to_owned());
        wait_for_loaded(&mut app, &handle);
        let scene = app
            .world()
            .resource::<Assets<Gltf>>()
            .get(&handle)
            .unwrap()
            .scenes[0]
            .clone();
        let entity = app
            .world_mut()
            .spawn(OwnershipReference(fixture.reference))
            .id();
        let arm = move |mut gate: OwnershipGate,
                        mut commands: Commands,
                        assets: Res<Assets<WorldAsset>>| {
            gate.begin_frame();
            let armed = gate.can_arm(entity, assets.get(&scene), &mut commands);
            gate.finish_frame();
            armed
        };
        for _ in 0..2 {
            assert!(!app.world_mut().run_system_once(arm.clone()).unwrap());
        }
        assert_eq!(
            app.world().resource::<OwnershipBatchDemand>().pending,
            [fixture.reference],
            "a held reference is demanded once"
        );
    }
}
