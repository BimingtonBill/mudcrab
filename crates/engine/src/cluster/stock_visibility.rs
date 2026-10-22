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
}

struct Saved {
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
            return;
        }

        let mut saved = Saved {
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
        let mut cpu_hidden = false;
        for main in hidden {
            if let Some((pair, cpu)) = visible_pair(class, *main) {
                now_hidden.insert(*main);
                cpu_hidden |= cpu;
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

        if cpu_hidden {
            let mut index = 0;
            class.entities_cpu_culling.retain(|pair| {
                let position = index;
                index += 1;
                if now_hidden.contains(&pair.1) {
                    saved.cpu.push((position, *pair));
                    false
                } else {
                    true
                }
            });
        }
        for main in &now_hidden {
            if let Some(render) = class.entities_gpu_culling.remove(main) {
                saved.gpu.push((render, *main));
            }
        }
        self.previous_hidden = now_hidden;
        self.saved = Some(saved);
    }

    /// Restore in Cleanup, before next frame's CPU diff/GPU delta collectors.
    /// Queue consumers must not mutate the raw class while this filter is active.
    pub fn restore(&mut self, class: &mut RenderVisibleEntitiesClass) {
        assert!(self.active, "visibility filter restored without apply");
        self.active = false;
        let Some(mut saved) = self.saved.take() else {
            return;
        };
        let saved_cpu = std::mem::take(&mut saved.cpu);
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
        let mut removed = saved_cpu.into_iter().rev().peekable();
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

    fn restore_gpu_and_events(class: &mut RenderVisibleEntitiesClass, saved: Saved) {
        for (render, main) in saved.gpu {
            assert!(
                class.entities_gpu_culling.insert(main, render).is_none(),
                "filtered GPU entry modified before restore"
            );
        }
        class.added_entities = saved.added;
        class.removed_entities = saved.removed;
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

/// The view's own pair for `main`, and whether it sits in the CPU-culled list.
fn visible_pair(class: &RenderVisibleEntitiesClass, main: MainEntity) -> Option<(Pair, bool)> {
    if let Ok(index) = class
        .entities_cpu_culling
        .binary_search_by_key(&main, |(_, main)| *main)
    {
        return Some((class.entities_cpu_culling[index], true));
    }
    class
        .entities_gpu_culling
        .get(&main)
        .map(|render| ((*render, main), false))
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
