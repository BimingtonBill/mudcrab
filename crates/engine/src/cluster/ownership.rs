//! Launch-time primitive ownership; no mesh handles or paths participate in matching.

#[cfg(test)]
use super::source_identity::SourcePrimitive;
use super::{
    instance_identity::ClusterInstanceIdentity,
    package::{BatchDemand, LazyBatches},
    reader::{CertifiedPackage, RecordKey},
    render::ClusterScene,
};
use bevy::{ecs::system::SystemParam, prelude::*};

#[path = "ownership_template.rs"]
mod template;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::Arc,
};
pub(crate) use template::{
    OwnedPrimitiveSource, arm, clone_with_stock_ordinals, fail_closed, filtered_scene,
};

#[cfg(test)]
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
    /// First terminal material/template qualification for this certificate.
    /// Runtime withdrawal and failed resources cannot change ownership.
    terminal: BTreeMap<RecordKey, bool>,
    terminal_revision: u64,
    refused_records: BTreeSet<RecordKey>,
    refused_references: BTreeSet<u32>,
    refusal_revision: u64,
}

/// Runtime reference identity, attached only to exterior static candidates.
#[derive(Component)]
pub(crate) struct OwnershipReference(pub(crate) u32);

#[derive(Component)]
pub(super) struct OwnershipDeferred;

/// Only unresolved, uninstantiated models are polled. Holding the existing pending
/// model/profile markers keeps cell readiness honest, and prevents a stock draw
/// before pre-instancing filtering has enough facts to make its one-time decision.
#[derive(SystemParam)]
pub(crate) struct OwnershipGate<'w, 's> {
    facts: Option<Res<'w, OwnershipFacts>>,
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
    owned_bodies_removed: u64,
    /// Candidate primitive decisions deferred once before instantiation, including
    /// candidates later proved not owned when the certificate arrives.
    owned_deferred: u64,
    waiting_certificate: u64,
    waiting_batches: u64,
    defer_logged: bool,
    frames: u64,
}

pub(crate) fn install(app: &mut App) {
    app.init_resource::<OwnershipFacts>()
        .init_resource::<OwnershipCounters>()
        .init_resource::<OwnershipBatchDemand>()
        .add_systems(Last, report);
    template::install(app);
    info!("cluster static ownership enabled; owned render bodies are removed before instancing");
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

    /// Update the live projection while freezing each record's first terminal
    /// qualification. Only replacing the certificate replaces ownership.
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
            self.terminal.clear();
            self.refused_records.clear();
            self.refused_references.clear();
            self.refusal_revision = self.refusal_revision.wrapping_add(1);
            self.terminal_revision = self.terminal_revision.wrapping_add(1);
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
        let mut added_terminal = false;
        let mut added_refusal = false;
        for (key, value) in records {
            if let Some(value) = value {
                if let std::collections::btree_map::Entry::Vacant(entry) = self.terminal.entry(key)
                {
                    entry.insert(value);
                    added_terminal = true;
                    if value {
                        self.refused_records.insert(key);
                        if let Some(identity) = package.record(&key) {
                            self.refused_references.insert(identity.reference);
                        }
                        added_refusal = true;
                    }
                }
                self.source_fallback.insert(key, value);
            } else {
                self.source_fallback.remove(&key);
            }
        }
        if added_refusal {
            self.refusal_revision = self.refusal_revision.wrapping_add(1);
        }
        if added_terminal {
            self.terminal_revision = self.terminal_revision.wrapping_add(1);
        }
    }

    pub(crate) fn revoke(&mut self) {
        // Losing runtime qualification is not a new certificate and cannot
        // restore stock bodies. Preserve the occurrence index and frozen facts.
        self.source_fallback.clear();
    }

    pub(crate) fn package_matches(&self, package: &Arc<CertifiedPackage>) -> bool {
        self.certificate
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, package))
    }

    #[cfg(test)]
    pub(crate) fn terminal_revision(&self) -> u64 {
        self.terminal_revision
    }

    /// Changes only when permanent stock permission changes, or the certificate is replaced.
    /// New owned facts do not invalidate refusal-only projections.
    pub(crate) fn refusal_revision(&self) -> u64 {
        self.refusal_revision
    }

    pub(crate) fn permanent_refused_records(&self) -> BTreeSet<RecordKey> {
        self.refused_records.clone()
    }

    pub(crate) fn permanent_refused_references(&self) -> BTreeSet<u32> {
        self.refused_references.clone()
    }

    pub(crate) fn permanent_refused_ordinals(&self, reference: u32) -> BTreeSet<u32> {
        self.ordinals_with_decision(reference, true)
    }

    fn ordinals_with_decision(&self, reference: u32, refused: bool) -> BTreeSet<u32> {
        let Some(package) = self.certificate.as_ref() else {
            return BTreeSet::new();
        };
        self.references
            .get(&reference)
            .into_iter()
            .flatten()
            .filter(|key| self.terminal.get(key) == Some(&refused))
            .filter_map(|key| package.record(key).map(|identity| identity.primitive))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn reference_has_owned(&self, reference: u32) -> bool {
        self.references
            .get(&reference)
            .into_iter()
            .flatten()
            .any(|key| self.terminal.get(key) == Some(&false))
    }

    pub(crate) fn owned_ordinals(&self, reference: u32) -> BTreeSet<u32> {
        self.ordinals_with_decision(reference, false)
    }

    #[cfg(test)]
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
        match self.terminal.get(key) {
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
                .all(|key| self.terminal.contains_key(key))
    }

    #[cfg(test)]
    pub(crate) fn projected_records(&self) -> &BTreeMap<RecordKey, bool> {
        &self.source_fallback
    }
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
            owned_bodies_removed = counters.owned_bodies_removed,
            owned_deferred = counters.owned_deferred,
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
    pub(super) fn fixture() -> (
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
    fn terminal_ownership_survives_runtime_failure_and_withdrawal() {
        let (_directory, package, reference, tag, key) = fixture();
        let mut facts = super::OwnershipFacts::default();
        facts.publish(
            &package,
            package.records().map(|(key, _)| (*key, false)).collect(),
        );
        facts.publish(&package, BTreeMap::from([(key, true)]));
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::Owned
        );
        facts.publish(&package, BTreeMap::new());
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::Owned
        );
        assert!(facts.can_spawn(reference));
    }

    #[test]
    fn initial_material_refusal_survives_later_qualification() {
        let (_directory, package, reference, tag, key) = fixture();
        let mut facts = super::OwnershipFacts::default();
        facts.publish(&package, BTreeMap::from([(key, true)]));
        facts.publish(&package, BTreeMap::from([(key, false)]));
        assert_eq!(
            facts.decision(reference, Some(&tag)),
            super::Decision::NotOwned
        );
    }

    #[test]
    fn refusal_revision_ignores_new_owned_facts_and_preserves_fixed_refusals() {
        let (directory, package, _, _, key) = fixture();
        let other = *package
            .records()
            .find(|(candidate, _)| **candidate != key)
            .unwrap()
            .0;
        let mut facts = super::OwnershipFacts::default();
        facts.publish_updates(&package, true, std::iter::empty());
        let initial = facts.refusal_revision();
        let terminal = facts.terminal_revision();
        facts.publish_updates(&package, false, [(key, Some(false))]);
        assert!(facts.terminal_revision() > terminal);
        assert_eq!(facts.refusal_revision(), initial);
        assert!(facts.permanent_refused_records().is_empty());
        assert!(facts.permanent_refused_references().is_empty());
        facts.publish_updates(&package, false, [(other, Some(true))]);
        let refused = facts.refusal_revision();
        assert!(refused > initial);
        assert_eq!(
            facts.permanent_refused_records(),
            std::collections::BTreeSet::from([other])
        );
        let refused_reference = package.record(&other).unwrap().reference;
        assert_eq!(
            facts.permanent_refused_references(),
            std::collections::BTreeSet::from([refused_reference])
        );
        facts.revoke();
        facts.publish_updates(&package, true, [(other, Some(false)), (key, Some(true))]);
        assert_eq!(facts.refusal_revision(), refused);
        assert_eq!(
            facts.permanent_refused_records(),
            std::collections::BTreeSet::from([other])
        );
        let replacement = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        facts.publish_updates(&replacement, true, std::iter::empty());
        assert!(facts.refusal_revision() > refused);
        assert!(facts.permanent_refused_records().is_empty());
        assert!(facts.permanent_refused_references().is_empty());
    }

    #[test]
    fn frozen_refusal_apis_and_revision_follow_terminal_changes_only() {
        let (directory, package, reference, tag, key) = fixture();
        let mut facts = super::OwnershipFacts::default();
        facts.publish(&package, BTreeMap::from([(key, true)]));
        let revision = facts.terminal_revision();
        assert_eq!(
            facts.permanent_refused_records(),
            std::collections::BTreeSet::from([key])
        );
        assert_eq!(
            facts.permanent_refused_references(),
            std::collections::BTreeSet::from([reference])
        );
        assert_eq!(
            facts.permanent_refused_ordinals(reference),
            std::collections::BTreeSet::from([tag.ordinal])
        );
        assert!(!facts.reference_has_owned(reference));
        facts.revoke();
        facts.publish(&package, BTreeMap::new());
        facts.publish(&package, BTreeMap::from([(key, false)]));
        assert_eq!(facts.terminal_revision(), revision);
        assert_eq!(
            facts.permanent_refused_records(),
            std::collections::BTreeSet::from([key])
        );
        assert_eq!(facts.projected_records(), &BTreeMap::from([(key, false)]));
        let replacement = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        facts.publish(&replacement, BTreeMap::new());
        assert!(facts.package_matches(&replacement));
        assert!(!facts.package_matches(&package));
        assert!(facts.terminal_revision() > revision);
        assert!(facts.permanent_refused_records().is_empty());
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
    fn withdrawn_qualification_keeps_owned_until_certificate_replacement() {
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
            super::Decision::Owned
        );
        assert!(facts.reference_has_owned(reference));
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
    fn actual_loader_owned_instances_have_no_render_body_before_visibility() {
        use super::super::source_identity::actual_loader_tests::{
            loader_app_with, wait_for_loaded,
        };
        use bevy::prelude::*;
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
        let source = app
            .world()
            .resource::<Assets<Gltf>>()
            .get(&handle)
            .unwrap()
            .scenes[0]
            .clone();
        let original_count = app
            .world()
            .resource::<Assets<WorldAsset>>()
            .get(&source)
            .unwrap()
            .world
            .iter_entities()
            .filter(|node| node.contains::<Mesh3d>())
            .count();
        let root = app
            .world_mut()
            .spawn((
                super::OwnershipReference(reference),
                Transform::IDENTITY,
                Visibility::Inherited,
            ))
            .id();
        super::arm(app.world_mut(), root, source.clone());
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&Mesh3d>().iter(world).count(), 0);
        assert_eq!(
            world
                .query::<&super::OwnedPrimitiveSource>()
                .iter(world)
                .count(),
            original_count
        );
        assert_eq!(
            world
                .resource::<Assets<WorldAsset>>()
                .get(&source)
                .unwrap()
                .world
                .iter_entities()
                .filter(|node| node.contains::<Mesh3d>())
                .count(),
            original_count
        );
    }
}

#[cfg(test)]
mod runtime_revocation_tests {
    use super::*;

    #[test]
    fn runtime_state_cannot_change_frozen_ownership() {
        let (_directory, package, reference, tag, key) = super::tests::fixture();
        let mut facts = OwnershipFacts::default();
        facts.publish(&package, BTreeMap::from([(key, false)]));
        let revision = facts.terminal_revision();
        facts.revoke();
        facts.publish(&package, BTreeMap::from([(key, true)]));
        assert_eq!(facts.decision(reference, Some(&tag)), Decision::Owned);
        assert!(facts.reference_has_owned(reference));
        assert!(facts.permanent_refused_records().is_empty());
        assert_eq!(facts.terminal_revision(), revision);
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
            inventory: None,
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
