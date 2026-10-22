//! Opt-in report of what the main world marks changed each frame (`--change-report`).
//!
//! Caches in this engine skip their work while Bevy's change detection says their inputs are
//! untouched. Whatever borrows a component mutably marks it changed, even when it writes the same
//! value, and every consumer that watches it then redoes its work. That cost is invisible in a
//! profile: it shows up as some unrelated system rebuilding every frame.
//!
//! This counts, for every component type, how many entities carry a change from the last frame,
//! and every [`WINDOW`] frames logs the busiest types. In a settled scene only what really moves
//! (the camera, animated objects) should appear; a cache's input listed in every frame is the
//! cause of a per-frame rebuild. The scan reads every component column once a frame, so it is
//! installed only when asked for.

use bevy::{ecs::component::ComponentId, prelude::*};
use std::collections::{HashMap, HashSet};

/// Frames per logged report.
const WINDOW: u32 = 600;
/// Component types listed per report.
const TOP: usize = 15;

pub struct ChangeReportPlugin;

impl Plugin for ChangeReportPlugin {
    fn build(&self, app: &mut App) {
        // `Last`: every other schedule of the frame has run, so its writes are all in.
        app.add_systems(Last, report_changes);
    }
}

#[derive(Default)]
struct ChangeWindow {
    /// The change tick the previous scan read; changes newer than it belong to this frame.
    last_scan: Option<bevy::ecs::change_detection::Tick>,
    frames: u32,
    /// Per component type: entities changed, summed over the window.
    changed: HashMap<ComponentId, u64>,
    /// Per component type: frames of the window in which at least one entity changed.
    frames_changed: HashMap<ComponentId, u32>,
}

fn report_changes(world: &mut World, mut window: Local<ChangeWindow>) {
    let this_scan = world.read_change_tick();
    let Some(last_scan) = window.last_scan.replace(this_scan) else {
        return;
    };
    let changed = changed_this_frame(world, last_scan, this_scan);
    for (&id, &count) in &changed {
        *window.changed.entry(id).or_default() += count;
        *window.frames_changed.entry(id).or_default() += 1;
    }
    window.frames += 1;
    if window.frames < WINDOW {
        return;
    }
    let mut busiest: Vec<_> = window.changed.iter().map(|(&id, &n)| (id, n)).collect();
    busiest.sort_unstable_by_key(|(_, total)| std::cmp::Reverse(*total));
    let components = world.components();
    let lines: Vec<String> = busiest
        .iter()
        .take(TOP)
        .map(|(id, total)| {
            let name = components
                .get_info(*id)
                .map(|info| info.name().shortname().to_string())
                .unwrap_or_else(|| format!("{id:?}"));
            format!(
                "{name}: {:.1} entities/frame, changed in {} of {} frames",
                *total as f64 / f64::from(window.frames),
                window.frames_changed.get(id).copied().unwrap_or(0),
                window.frames
            )
        })
        .collect();
    info!(
        component_types = busiest.len(),
        "main-world changes over {} frames (busiest first):\n  {}",
        window.frames,
        lines.join("\n  ")
    );
    window.frames = 0;
    window.changed.clear();
    window.frames_changed.clear();
}

/// Entities whose component changed after `last_scan`, per component type.
fn changed_this_frame(
    world: &World,
    last_scan: bevy::ecs::change_detection::Tick,
    this_scan: bevy::ecs::change_detection::Tick,
) -> HashMap<ComponentId, u64> {
    let storages = world.storages();
    let mut changed: HashMap<ComponentId, u64> = HashMap::new();
    let mut tables: HashSet<usize> = HashSet::new();
    for archetype in world.archetypes().iter() {
        if archetype.is_empty() {
            continue;
        }
        // Archetypes share tables; each table's columns are counted once.
        if tables.insert(archetype.table_id().as_usize())
            && let Some(table) = storages.tables.get(archetype.table_id())
        {
            for id in archetype.table_components() {
                let Some(ticks) = table.get_changed_ticks_slice_for(id) else {
                    continue;
                };
                // SAFETY: this exclusive system holds the world, so no system writes a tick while
                // it is read; each cell is read by value and no reference escapes.
                let count = ticks
                    .iter()
                    .filter(|tick| unsafe { *tick.get() }.is_newer_than(last_scan, this_scan))
                    .count();
                if count != 0 {
                    *changed.entry(id).or_default() += count as u64;
                }
            }
        }
        for id in archetype.sparse_set_components() {
            let Some(set) = storages.sparse_sets.get(id) else {
                continue;
            };
            let count = archetype
                .entities()
                .iter()
                .filter_map(|entity| set.get_changed_tick(entity.id()))
                // SAFETY: as above, the world is held exclusively and the tick is read by value.
                .filter(|tick| unsafe { *tick.get() }.is_newer_than(last_scan, this_scan))
                .count();
            if count != 0 {
                *changed.entry(id).or_default() += count as u64;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Default)]
    struct Moved(u32);

    #[derive(Component, Default)]
    struct Still;

    #[derive(Component, Default)]
    #[component(storage = "SparseSet")]
    struct Sparse(u32);

    #[test]
    fn counts_only_components_written_since_the_last_scan() {
        let mut world = World::new();
        let entities: Vec<Entity> = (0..4)
            .map(|_| {
                world
                    .spawn((Moved::default(), Still, Sparse::default()))
                    .id()
            })
            .collect();
        world.increment_change_tick();
        let last = world.read_change_tick();
        world.increment_change_tick();
        // Two writes through `Mut` mark two `Moved`; a read marks nothing.
        for &entity in &entities[..2] {
            world.get_mut::<Moved>(entity).unwrap().0 += 1;
        }
        world.get_mut::<Sparse>(entities[3]).unwrap().0 += 1;
        assert!(world.get::<Still>(entities[0]).is_some());
        let now = world.read_change_tick();
        let changed = changed_this_frame(&world, last, now);
        let id = |world: &World, type_id| world.components().get_id(type_id).unwrap();
        assert_eq!(
            changed.get(&id(&world, std::any::TypeId::of::<Moved>())),
            Some(&2)
        );
        assert_eq!(
            changed.get(&id(&world, std::any::TypeId::of::<Sparse>())),
            Some(&1)
        );
        assert_eq!(
            changed.get(&id(&world, std::any::TypeId::of::<Still>())),
            None
        );
    }
}
