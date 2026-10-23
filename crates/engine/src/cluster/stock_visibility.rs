//! Temporary filtering of one visibility class in one retained view.
//! The caller owns scheduling, view identity and the draw-readiness decision.

use bevy::{
    platform::collections::HashSet,
    prelude::Entity,
    render::{sync_world::MainEntity, view::visibility::RenderVisibleEntitiesClass},
};

type Pair = (Entity, MainEntity);

#[derive(Default)]
pub struct ClassFilter {
    previous_hidden: HashSet<MainEntity>,
    active: bool,
    saved: Option<Saved>,
    #[cfg(test)]
    replays: usize,
}

struct Saved {
    raw_cpu_len: usize,
    raw_gpu_len: usize,
    cpu: Vec<(usize, Pair)>,
    gpu: Vec<Pair>,
    added: Vec<Pair>,
    removed: Vec<Pair>,
}

impl ClassFilter {
    /// Apply once after visibility collection/specialization, before every stock
    /// queue consumer. `hidden` must already have per-view/per-pass certification.
    /// A second apply before restore is a scheduling error, including no-op frames.
    pub fn apply(&mut self, class: &mut RenderVisibleEntitiesClass, hidden: &HashSet<MainEntity>) {
        assert!(!self.active, "visibility filter applied without restore");
        self.active = true;
        if hidden.is_empty() && self.previous_hidden.is_empty() {
            self.saved = None;
            return;
        }

        // Events and lengths reject changed classes cheaply. Slot/key pairs also catch renderer
        // replacements that the raw visibility collector did not report as membership events.
        if hidden == &self.previous_hidden
            && class.added_entities.is_empty()
            && class.removed_entities.is_empty()
            && let Some(saved) = &self.saved
            && class.entities_cpu_culling.len() == saved.raw_cpu_len
            && class.entities_gpu_culling.len() == saved.raw_gpu_len
            && saved
                .cpu
                .iter()
                .all(|(slot, pair)| class.entities_cpu_culling.get(*slot) == Some(pair))
            && saved
                .gpu
                .iter()
                .all(|(render, main)| class.entities_gpu_culling.get(main) == Some(render))
        {
            Self::hide(class, saved);
            #[cfg(test)]
            {
                self.replays += 1;
            }
            return;
        }

        let mut saved = Saved {
            raw_cpu_len: class.entities_cpu_culling.len(),
            raw_gpu_len: class.entities_gpu_culling.len(),
            cpu: Vec::new(),
            gpu: Vec::new(),
            added: std::mem::take(&mut class.added_entities),
            removed: std::mem::take(&mut class.removed_entities),
        };
        let mut now_hidden = HashSet::default();
        let mut newly_hidden = Vec::new();
        let mut newly_stock = Vec::new();
        // Only entities really in this view/class may enter persistent state. Membership is looked
        // up per named entity (the CPU list is sorted by main entity, the GPU list is a map), so
        // the cost follows the hidden sets, not everything the view sees.
        // The CPU-list slots of the hidden entities, found by the same lookup, so taking them out
        // is one compaction pass rather than a hash lookup per entity the view sees.
        let search_span = ::bevy::log::info_span!("stock_visibility::apply::search").entered();
        let mut cpu_slots = Vec::new();
        for main in hidden {
            if let Some((pair, slot)) = visible_pair(class, *main) {
                now_hidden.insert(*main);
                if let Some(slot) = slot {
                    cpu_slots.push((slot, pair));
                }
                if !self.previous_hidden.contains(main) {
                    newly_hidden.push(pair);
                }
            }
        }
        for main in &self.previous_hidden {
            if !hidden.contains(main)
                && let Some((pair, _)) = visible_pair(class, *main)
            {
                newly_stock.push(pair);
            }
        }

        drop(search_span);
        let _rest_span = ::bevy::log::info_span!("stock_visibility::apply::compact").entered();
        class.added_entities.extend(
            saved
                .added
                .iter()
                .copied()
                .filter(|(_, main)| !hidden.contains(main)),
        );
        class.added_entities.extend(newly_stock);
        // A raw disappearance of something already hidden was never a stock
        // disappearance. Drop it rather than issuing the same removal twice.
        class.removed_entities.extend(
            saved
                .removed
                .iter()
                .copied()
                .filter(|(_, main)| !self.previous_hidden.contains(main)),
        );
        class.removed_entities.extend(newly_hidden);
        sorted_unique(&mut class.added_entities);
        sorted_unique(&mut class.removed_entities);

        cpu_slots.sort_unstable_by_key(|(slot, _)| *slot);
        saved.cpu = cpu_slots;
        for main in &now_hidden {
            if let Some(render) = class.entities_gpu_culling.get(main) {
                saved.gpu.push((*render, *main));
            }
        }
        Self::hide(class, &saved);
        self.previous_hidden = now_hidden;
        self.saved = Some(saved);
    }

    fn hide(class: &mut RenderVisibleEntitiesClass, saved: &Saved) {
        if !saved.cpu.is_empty() {
            let cpu = &mut class.entities_cpu_culling;
            let mut slots = saved.cpu.iter().map(|(slot, _)| *slot).peekable();
            let mut kept = 0;
            for read in 0..cpu.len() {
                if slots.next_if_eq(&read).is_some() {
                    continue;
                }
                cpu[kept] = cpu[read];
                kept += 1;
            }
            cpu.truncate(kept);
        }
        for (_, main) in &saved.gpu {
            class.entities_gpu_culling.remove(main);
        }
    }

    /// Restore in Cleanup, before next frame's CPU diff/GPU delta collectors.
    /// Queue consumers must not mutate the raw class while this filter is active.
    pub fn restore(&mut self, class: &mut RenderVisibleEntitiesClass) {
        assert!(self.active, "visibility filter restored without apply");
        self.active = false;
        let Some(saved) = self.saved.as_mut() else {
            return;
        };
        let _restore_span = ::bevy::log::info_span!("stock_visibility::restore").entered();
        let saved_cpu = &saved.cpu;
        if saved_cpu.is_empty() {
            Self::restore_gpu_and_events(class, saved);
            return;
        }
        // Reconstitute in place from the back, preserving every original slot.
        // Repeated Vec::insert would move the retained tail once per hidden
        // entity and become quadratic when a large view hands off to clusters.
        let cpu = &mut class.entities_cpu_culling;
        let mut retained = cpu.len();
        let raw_len = retained + saved_cpu.len();
        cpu.resize(raw_len, (Entity::PLACEHOLDER, Entity::PLACEHOLDER.into()));
        let mut removed = saved_cpu.iter().rev().peekable();
        for index in (0..raw_len).rev() {
            if removed
                .peek()
                .is_some_and(|(original, _)| *original == index)
            {
                cpu[index] = removed.next().expect("checked removed slot").1;
            } else {
                retained = retained
                    .checked_sub(1)
                    .expect("filtered CPU entries modified before restore");
                cpu[index] = cpu[retained];
            }
        }
        assert!(
            removed.next().is_none() && retained == 0,
            "filtered CPU entries modified before restore"
        );
        Self::restore_gpu_and_events(class, saved);
    }

    fn restore_gpu_and_events(class: &mut RenderVisibleEntitiesClass, saved: &mut Saved) {
        for &(render, main) in &saved.gpu {
            assert!(
                class.entities_gpu_culling.insert(main, render).is_none(),
                "filtered GPU entry modified before restore"
            );
        }
        class.added_entities = std::mem::take(&mut saved.added);
        class.removed_entities = std::mem::take(&mut saved.removed);
    }
}

/// The view's own pairs for those of `mains` it sees, looked up one by one.
pub fn visible_pairs<'a>(
    class: &RenderVisibleEntitiesClass,
    mains: impl IntoIterator<Item = &'a MainEntity>,
) -> Vec<Pair> {
    mains
        .into_iter()
        .filter_map(|main| visible_pair(class, *main).map(|(pair, _)| pair))
        .collect()
}

/// Whether the view sees `main`: a binary search of the CPU-culled list, then the GPU map.
pub fn sees(class: &RenderVisibleEntitiesClass, main: MainEntity) -> bool {
    visible_pair(class, main).is_some()
}

/// How many entities the view sees, both lists.
pub fn visible_len(class: &RenderVisibleEntitiesClass) -> usize {
    class.entities_cpu_culling.len() + class.entities_gpu_culling.len()
}

/// The view's own pair for `main`, and its slot in the CPU-culled list when it sits there.
fn visible_pair(
    class: &RenderVisibleEntitiesClass,
    main: MainEntity,
) -> Option<(Pair, Option<usize>)> {
    if let Ok(index) = class
        .entities_cpu_culling
        .binary_search_by_key(&main, |(_, main)| *main)
    {
        return Some((class.entities_cpu_culling[index], Some(index)));
    }
    class
        .entities_gpu_culling
        .get(&main)
        .map(|render| ((*render, main), None))
}

fn sorted_unique(events: &mut Vec<Pair>) {
    // Stable sorting preserves an original event's renderer pair when a
    // synthetic event names the same MainEntity. Never reconstruct entity IDs.
    events.sort_by_key(|(_, main)| *main);
    events.dedup_by_key(|(_, main)| *main);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(id: u32) -> Pair {
        (
            Entity::from_raw_u32(id + 100).unwrap(),
            MainEntity::from(Entity::from_raw_u32(id).unwrap()),
        )
    }
    // Entity ordering is Bevy's opaque Ord, not numeric row order.
    fn ordered(ids: &[u32]) -> Vec<Pair> {
        let mut pairs = ids.iter().map(|id| pair(*id)).collect::<Vec<_>>();
        pairs.sort_by_key(|(_, main)| *main);
        pairs
    }
    fn hidden(ids: &[u32]) -> HashSet<MainEntity> {
        ids.iter().map(|id| pair(*id).1).collect()
    }
    fn cpu_frame(class: &mut RenderVisibleEntitiesClass, ids: &[u32]) {
        class.prepare_for_new_frame();
        let mut pairs = ids.iter().map(|id| pair(*id)).collect::<Vec<_>>();
        pairs.sort_by_key(|(_, main)| *main);
        class.update_cpu_culled_entities(&pairs);
    }

    fn assert_class_eq(class: &RenderVisibleEntitiesClass, raw: &RenderVisibleEntitiesClass) {
        assert_eq!(class.entities_cpu_culling, raw.entities_cpu_culling);
        assert_eq!(class.entities_gpu_culling, raw.entities_gpu_culling);
        assert_eq!(class.added_entities, raw.added_entities);
        assert_eq!(class.removed_entities, raw.removed_entities);
    }

    #[test]
    fn settled_replay_preserves_cpu_gpu_events_and_exact_restore() {
        let mut class = RenderVisibleEntitiesClass::default();
        cpu_frame(&mut class, &[1, 2, 3]);
        class.entities_gpu_culling.insert(pair(4).1, pair(4).0);
        let mut filter = ClassFilter::default();
        let hidden = hidden(&[2, 4]);
        filter.apply(&mut class, &hidden);
        filter.restore(&mut class);

        for round in 1..=2 {
            class.prepare_for_new_frame();
            let raw = class.clone();
            filter.apply(&mut class, &hidden);
            assert_eq!(filter.replays, round);
            assert_eq!(class.entities_cpu_culling, ordered(&[1, 3]));
            assert!(class.entities_gpu_culling.is_empty());
            assert!(class.added_entities.is_empty());
            assert!(class.removed_entities.is_empty());
            filter.restore(&mut class);
            assert_class_eq(&class, &raw);
        }
    }

    #[test]
    fn raw_changes_reject_replay_and_match_the_general_path() {
        // Events, lengths, CPU slots and GPU renderer pairs independently invalidate replay.
        for change in 0..9 {
            let mut class = RenderVisibleEntitiesClass::default();
            cpu_frame(&mut class, &[1, 2, 3]);
            class.entities_gpu_culling.insert(pair(4).1, pair(4).0);
            let mut filter = ClassFilter::default();
            let hidden = hidden(&[2, 4]);
            filter.apply(&mut class, &hidden);
            filter.restore(&mut class);
            class.prepare_for_new_frame();
            match change {
                0 => class.added_entities.push(pair(1)),
                1 => class.removed_entities.push(pair(9)),
                2 => class.entities_cpu_culling = ordered(&[1, 2, 3, 5]),
                3 => {
                    class.entities_gpu_culling.insert(pair(5).1, pair(5).0);
                }
                4 => {
                    let slot = class
                        .entities_cpu_culling
                        .iter()
                        .position(|p| *p == pair(2))
                        .unwrap();
                    class.entities_cpu_culling[slot].0 = pair(20).0;
                }
                5 => {
                    class.entities_gpu_culling.insert(pair(4).1, pair(40).0);
                }
                6 => class.entities_cpu_culling = ordered(&[2, 3, 5]),
                7 => {
                    class.entities_gpu_culling.remove(&pair(4).1);
                    class.entities_gpu_culling.insert(pair(5).1, pair(5).0);
                }
                8 => {
                    class.entities_cpu_culling = ordered(&[1, 3, 4]);
                    class.entities_gpu_culling.remove(&pair(4).1);
                    class.entities_gpu_culling.insert(pair(2).1, pair(2).0);
                }
                _ => unreachable!(),
            }
            let raw = class.clone();
            let mut general_class = raw.clone();
            let mut general = ClassFilter {
                previous_hidden: hidden.clone(),
                ..Default::default()
            };
            general.apply(&mut general_class, &hidden);
            filter.apply(&mut class, &hidden);
            assert_eq!(filter.replays, 0, "change {change}");
            assert_class_eq(&class, &general_class);
            assert_eq!(filter.previous_hidden, general.previous_hidden);
            filter.restore(&mut class);
            general.restore(&mut general_class);
            assert_class_eq(&class, &raw);
            assert_class_eq(&general_class, &raw);
        }
    }

    #[test]
    fn unhide_then_empty_noop_preserves_fresh_duplicate_events() {
        let mut class = RenderVisibleEntitiesClass::default();
        cpu_frame(&mut class, &[1, 2]);
        let mut filter = ClassFilter::default();
        filter.apply(&mut class, &hidden(&[2]));
        filter.restore(&mut class);
        class.prepare_for_new_frame();
        filter.apply(&mut class, &hidden(&[]));
        assert_eq!(class.added_entities, vec![pair(2)]);
        filter.restore(&mut class);
        class.added_entities.extend([pair(1), pair(1)]);
        class.removed_entities.extend([pair(9), pair(9)]);
        let raw = class.clone();
        filter.apply(&mut class, &hidden(&[]));
        assert_class_eq(&class, &raw);
        filter.restore(&mut class);
        assert_class_eq(&class, &raw);
    }

    #[test]
    fn nested_replays_restore_each_incoming_class_after_outer_hide_changes() {
        let mut class = RenderVisibleEntitiesClass::default();
        cpu_frame(&mut class, &[1, 2, 3]);
        class.entities_gpu_culling.insert(pair(4).1, pair(4).0);
        let mut outer = ClassFilter::default();
        let mut inner = ClassFilter::default();
        for ids in [&[2][..], &[2], &[2], &[3], &[3], &[2, 4]] {
            class.prepare_for_new_frame();
            let raw = class.clone();
            outer.apply(&mut class, &hidden(ids));
            let outer_class = class.clone();
            inner.apply(&mut class, &hidden(&[4]));
            assert!(class.entities_gpu_culling.is_empty());
            inner.restore(&mut class);
            assert_class_eq(&class, &outer_class);
            outer.restore(&mut class);
            assert_class_eq(&class, &raw);
        }
        assert_eq!(outer.replays, 3);
        assert_eq!(inner.replays, 3);
    }

    #[test]
    fn cpu_hide_stable_frame_and_toggle_restore_exact_raw_lists() {
        let mut class = RenderVisibleEntitiesClass::default();
        let mut filter = ClassFilter::default();
        cpu_frame(&mut class, &[1, 2, 3]);
        let raw = class.clone();
        filter.apply(&mut class, &hidden(&[2, 999]));
        assert_eq!(class.entities_cpu_culling, ordered(&[1, 3]));
        assert_eq!(class.added_entities, ordered(&[1, 3]));
        assert_eq!(class.removed_entities, vec![pair(2)]);
        assert_eq!(filter.previous_hidden, hidden(&[2]));
        filter.restore(&mut class);
        assert_eq!(class.entities_cpu_culling, raw.entities_cpu_culling);
        assert_eq!(class.added_entities, raw.added_entities);
        assert_eq!(class.removed_entities, raw.removed_entities);

        cpu_frame(&mut class, &[1, 2, 3]);
        assert!(class.added_entities.is_empty());
        filter.apply(&mut class, &hidden(&[2]));
        assert!(class.added_entities.is_empty());
        assert!(class.removed_entities.is_empty());
        filter.restore(&mut class);
        cpu_frame(&mut class, &[1, 2, 3]);
        filter.apply(&mut class, &hidden(&[]));
        assert_eq!(class.added_entities, vec![pair(2)]);
        assert!(class.removed_entities.is_empty());
        filter.restore(&mut class);
        assert!(class.added_entities.is_empty());
    }

    #[test]
    fn gpu_raw_members_survive_a_frame_without_gpu_deltas() {
        let mut class = RenderVisibleEntitiesClass::default();
        let mut filter = ClassFilter::default();
        class.entities_gpu_culling.insert(pair(1).1, pair(1).0);
        class.add_entity(pair(1));
        class.sort_added_entities();
        filter.apply(&mut class, &hidden(&[1]));
        assert!(class.entities_gpu_culling.is_empty());
        assert!(class.added_entities.is_empty());
        filter.restore(&mut class);
        class.prepare_for_new_frame(); // GPU collector receives no change.
        filter.apply(&mut class, &hidden(&[1]));
        assert!(class.entities_gpu_culling.is_empty());
        assert!(class.removed_entities.is_empty());
        filter.restore(&mut class);
        class.prepare_for_new_frame();
        filter.apply(&mut class, &hidden(&[]));
        assert_eq!(class.added_entities, vec![pair(1)]);
        assert_eq!(class.entities_gpu_culling.get(&pair(1).1), Some(&pair(1).0));
        filter.restore(&mut class);
        assert!(class.added_entities.is_empty());
    }

    #[test]
    fn raw_removal_drops_hidden_state_and_does_not_readd_a_despawned_entity() {
        let mut class = RenderVisibleEntitiesClass::default();
        let mut filter = ClassFilter::default();
        cpu_frame(&mut class, &[1, 2]);
        filter.apply(&mut class, &hidden(&[1]));
        filter.restore(&mut class);
        cpu_frame(&mut class, &[2]);
        assert_eq!(class.removed_entities, vec![pair(1)]);
        filter.apply(&mut class, &hidden(&[]));
        assert!(class.removed_entities.is_empty());
        assert!(class.added_entities.is_empty());
        assert!(filter.previous_hidden.is_empty());
        filter.restore(&mut class);
        assert_eq!(class.removed_entities, vec![pair(1)]);
    }

    #[test]
    fn filters_are_view_local_and_events_are_sorted_unique() {
        let mut first = RenderVisibleEntitiesClass::default();
        let mut second = RenderVisibleEntitiesClass::default();
        let mut a = ClassFilter::default();
        let mut b = ClassFilter::default();
        cpu_frame(&mut first, &[1, 2, 3]);
        cpu_frame(&mut second, &[1, 2, 3]);
        first.add_entity(pair(3));
        first.add_entity(pair(1));
        first.removed_entities.extend([pair(3), pair(1), pair(3)]);
        let raw_added = first.added_entities.clone();
        let raw_removed = first.removed_entities.clone();
        a.apply(&mut first, &hidden(&[2]));
        b.apply(&mut second, &hidden(&[]));
        assert_eq!(first.added_entities, ordered(&[1, 3]));
        assert_eq!(first.removed_entities, ordered(&[1, 2, 3]));
        assert_eq!(second.entities_cpu_culling, ordered(&[1, 2, 3]));
        a.restore(&mut first);
        b.restore(&mut second);
        assert_eq!(first.added_entities, raw_added);
        assert_eq!(first.removed_entities, raw_removed);
    }

    #[test]
    fn empty_current_and_previous_hidden_are_an_exact_noop() {
        let mut class = RenderVisibleEntitiesClass::default();
        cpu_frame(&mut class, &[1, 2]);
        class.add_entity(pair(1));
        let raw = class.clone();
        let mut filter = ClassFilter::default();
        filter.apply(&mut class, &hidden(&[]));
        assert!(filter.saved.is_none());
        assert_eq!(class.added_entities, raw.added_entities);
        filter.restore(&mut class);
        assert_eq!(class.entities_cpu_culling, raw.entities_cpu_culling);
        assert_eq!(class.added_entities, raw.added_entities);
    }

    #[test]
    #[should_panic(expected = "applied without restore")]
    fn double_apply_is_a_scheduling_error_even_for_noop_frames() {
        let mut class = RenderVisibleEntitiesClass::default();
        let mut filter = ClassFilter::default();
        filter.apply(&mut class, &hidden(&[]));
        filter.apply(&mut class, &hidden(&[]));
    }

    #[test]
    #[should_panic(expected = "restored without apply")]
    fn restore_without_apply_is_a_scheduling_error() {
        ClassFilter::default().restore(&mut RenderVisibleEntitiesClass::default());
    }
}
