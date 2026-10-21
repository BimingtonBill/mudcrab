//! Exact world ownership of changed replacement inputs. Storage blocks may belong
//! to a parent world; invalidation follows each reference's placing world.
use super::*;

#[derive(Default, Debug, PartialEq, Eq)]
pub(super) struct CandidateWorlds {
    pub(super) all: bool,
    pub(super) worlds: HashSet<u32>,
}

pub(super) struct CandidateInputs<'a> {
    pub(super) certificate: Option<&'a Arc<crate::cluster::reader::CertifiedPackage>>,
    pub(super) present: &'a BTreeSet<u32>,
    pub(super) retired_refs: &'a HashSet<u32>,
    pub(super) retired_present: &'a HashSet<u32>,
    pub(super) retired_materials: &'a HashMap<MainEntity, Option<bevy::asset::UntypedAssetId>>,
    pub(super) retired_eligibility: &'a HashMap<MainEntity, RetiredEligibility>,
    pub(super) bound: &'a HashMap<RecordKey, MainEntity>,
    pub(super) stock_primitives: &'a HashMap<(u32, u32), MainEntity>,
    pub(super) templates: &'a [bool],
    pub(super) normal_maps: &'a [bool],
    pub(super) worlds: &'a HashMap<MainEntity, Option<u32>>,
    pub(super) light_dependencies: &'a HashMap<MainEntity, Option<HashSet<MainEntity>>>,
    pub(super) terrain: &'a HashMap<MainEntity, crate::streaming::TerrainPrimitiveIdentity>,
    pub(super) terrain_visible: &'a HashSet<MainEntity>,
}

impl Sources {
    pub(super) fn candidate_inputs(&self) -> CandidateInputs<'_> {
        CandidateInputs {
            certificate: self.certificate.as_ref(),
            present: &self.present,
            retired_refs: &self.retired_refs,
            retired_present: &self.retired_present,
            retired_materials: &self.retired_materials,
            retired_eligibility: &self.retired_eligibility,
            bound: &self.bound,
            stock_primitives: &self.stock_primitives,
            templates: &self.templates,
            normal_maps: &self.normal_maps,
            worlds: &self.worlds,
            light_dependencies: &self.light_dependencies,
            terrain: &self.terrain,
            terrain_visible: &self.terrain_visible,
        }
    }
}

fn changed_keys<'a, K: Eq + std::hash::Hash, V: PartialEq>(
    old: &'a HashMap<K, V>,
    new: &'a HashMap<K, V>,
) -> impl Iterator<Item = &'a K> {
    old.iter()
        .filter(|(key, value)| new.get(key) != Some(*value))
        .map(|(key, _)| key)
        .chain(new.keys().filter(|key| !old.contains_key(key)))
}

impl CandidateWorlds {
    fn reference(&mut self, input: &CandidateInputs<'_>, reference: u32) {
        if let Some(reference) = input.certificate.and_then(|c| c.reference(reference)) {
            self.worlds.insert(reference.placed_worldspace_id);
        } else {
            self.all = true;
        }
    }

    fn entities(&mut self, input: &CandidateInputs<'_>, entities: &HashSet<MainEntity>) {
        if entities.is_empty() {
            return;
        }
        let mut found = HashSet::new();
        for (key, owner) in input.bound {
            if entities.contains(owner) {
                found.insert(*owner);
                if let Some(record) = input.certificate.and_then(|c| c.record(key)) {
                    self.worlds.insert(record.worldspace);
                } else {
                    self.all = true;
                }
            }
        }
        for ((reference, _), owner) in input.stock_primitives {
            if entities.contains(owner) {
                found.insert(*owner);
                self.reference(input, *reference);
            }
        }
        self.all |= !entities.is_subset(&found);
    }
}

pub(super) fn candidate_worlds(
    old: &CandidateInputs<'_>,
    new: &CandidateInputs<'_>,
) -> CandidateWorlds {
    let same_certificate = match (old.certificate, new.certificate) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    };
    let mut delta = CandidateWorlds {
        all: !same_certificate
            || old.templates != new.templates
            || old.normal_maps != new.normal_maps,
        ..default()
    };
    if delta.all {
        return delta;
    }
    for reference in old.present.symmetric_difference(new.present) {
        delta.reference(old, *reference);
        delta.reference(new, *reference);
    }
    for reference in old
        .retired_refs
        .symmetric_difference(new.retired_refs)
        .chain(
            old.retired_present
                .symmetric_difference(new.retired_present),
        )
    {
        delta.reference(old, *reference);
        delta.reference(new, *reference);
    }
    for key in changed_keys(old.bound, new.bound) {
        if let Some(record) = old.certificate.and_then(|c| c.record(key)) {
            delta.worlds.insert(record.worldspace);
        } else {
            delta.all = true;
        }
    }
    for (reference, _) in changed_keys(old.stock_primitives, new.stock_primitives) {
        delta.reference(old, *reference);
        delta.reference(new, *reference);
    }
    let changed_entities: HashSet<_> = changed_keys(old.retired_materials, new.retired_materials)
        .chain(changed_keys(
            old.retired_eligibility,
            new.retired_eligibility,
        ))
        .copied()
        .collect();
    for input in [old, new] {
        let owned: HashSet<_> = changed_entities
            .iter()
            .copied()
            .filter(|entity| {
                input.retired_materials.contains_key(entity)
                    || input.retired_eligibility.contains_key(entity)
            })
            .collect();
        delta.entities(input, &owned);
    }
    for entity in changed_keys(old.worlds, new.worlds) {
        delta
            .worlds
            .extend(old.worlds.get(entity).copied().flatten());
        delta
            .worlds
            .extend(new.worlds.get(entity).copied().flatten());
    }
    for entity in changed_keys(old.light_dependencies, new.light_dependencies) {
        for input in [old, new] {
            if input.light_dependencies.contains_key(entity) {
                match input.worlds.get(entity) {
                    Some(world) => delta.worlds.extend(world),
                    None => delta.all = true,
                }
            }
        }
    }
    if terrain_map_change(old.terrain, new.terrain) == TerrainMapChange::Invalidating {
        for entity in changed_keys(old.terrain, new.terrain) {
            debug!(?entity, previous = ?old.terrain.get(entity), current = ?new.terrain.get(entity),
                "cluster terrain candidate identity changed");
            for input in [old, new] {
                delta
                    .worlds
                    .extend(input.terrain.get(entity).map(|t| t.worldspace_id));
            }
        }
    }
    for entity in old.terrain_visible.difference(new.terrain_visible) {
        debug!(?entity, previous = ?old.terrain.get(entity), current = ?new.terrain.get(entity),
            "cluster terrain candidate visibility withdrawn");
        if let Some(identity) = old.terrain.get(entity) {
            delta.worlds.insert(identity.worldspace_id);
        } else {
            delta.all = true;
        }
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::reader::producer_fixture_tests::published_for_loader;

    #[test]
    fn object_changes_follow_placed_world_and_preserve_other_world_ready_reports() {
        use crate::swap_bridge::destination::*;
        let directory = published_for_loader();
        let certificate = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        let (key, child) = certificate
            .records()
            .find(|(key, record)| key.worldspace != record.worldspace)
            .unwrap();
        let parent_world = WorldId::exterior(key.worldspace);
        let child_world = WorldId::exterior(child.worldspace);
        let mut old = Sources {
            certificate: Some(certificate.clone()),
            ..default()
        };
        let entity = MainEntity::from(Entity::from_bits(123));
        old.present.insert(child.reference);
        old.bound.insert(*key, entity);
        let new = Sources {
            certificate: Some(certificate.clone()),
            ..default()
        };
        let delta = candidate_worlds(&old.candidate_inputs(), &new.candidate_inputs());
        assert!(!delta.all);
        assert_eq!(delta.worlds, HashSet::from([child.worldspace]));
        let mut generations = Generations::default();
        let ready_key = DestinationKey {
            view: ViewKey {
                view: RetainedViewEntity::new(MainEntity::from(Entity::from_bits(456)), None, 0),
                incarnation: 0,
            },
            world: parent_world,
            generation: generations.current(parent_world),
        };
        let mut reports = DestinationReports::default();
        reports.set(DestinationReport {
            key: ready_key,
            evaluated_render_frame: RenderFrame(5),
            state: DestinationState::Ready,
        });
        for world in delta.worlds {
            generations.bump(WorldId::exterior(world), InvalidationCause::Candidate);
        }
        assert!(reports.is_ready(&ready_key, &generations));
        assert_ne!(generations.current(child_world), Generation::default());
        // Equal-count entity replacement still changes the child's binding.
        let mut replacement = Sources {
            certificate: Some(certificate.clone()),
            ..default()
        };
        replacement.present = old.present.clone();
        replacement
            .bound
            .insert(*key, MainEntity::from(Entity::from_bits(124)));
        let delta = candidate_worlds(&old.candidate_inputs(), &replacement.candidate_inputs());
        assert_eq!(delta.worlds, HashSet::from([child.worldspace]));
        assert!(!delta.all);
    }

    #[test]
    fn terrain_and_light_withdrawals_revoke_only_their_old_and_new_worlds() {
        let entity = MainEntity::from(Entity::from_bits(123));
        let identity = crate::streaming::TerrainPrimitiveIdentity {
            worldspace_id: 100,
            grid: IVec2::ZERO,
            quadrant: 0,
            tier: None,
        };
        let mut old = Sources::default();
        Arc::make_mut(&mut old.terrain).insert(entity, identity);
        old.terrain_visible.insert(entity);
        let mut new = Sources::default();
        Arc::make_mut(&mut new.terrain).insert(
            entity,
            crate::streaming::TerrainPrimitiveIdentity {
                worldspace_id: 200,
                ..identity
            },
        );
        let delta = candidate_worlds(&old.candidate_inputs(), &new.candidate_inputs());
        assert_eq!(delta.worlds, HashSet::from([100, 200]));
        assert!(!delta.all);
        old.worlds.insert(entity, Some(100));
        new.worlds.insert(entity, Some(200));
        old.light_dependencies.insert(entity, Some(HashSet::new()));
        new.light_dependencies.insert(entity, None);
        let delta = candidate_worlds(&old.candidate_inputs(), &new.candidate_inputs());
        assert_eq!(delta.worlds, HashSet::from([100, 200]));
        assert!(!delta.all);
    }

    #[test]
    fn unknown_ownership_and_global_templates_fail_closed() {
        let old = Sources::default();
        let mut new = Sources::default();
        new.present.insert(999);
        assert!(candidate_worlds(&old.candidate_inputs(), &new.candidate_inputs()).all);
        new.present.clear();
        new.templates.push(true);
        assert!(candidate_worlds(&old.candidate_inputs(), &new.candidate_inputs()).all);
        new.templates.clear();
        new.retired_materials
            .insert(MainEntity::from(Entity::from_bits(123)), None);
        assert!(candidate_worlds(&old.candidate_inputs(), &new.candidate_inputs()).all);
    }
}
