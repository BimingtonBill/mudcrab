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
    /// Per component type: one entity it changed on this window, to name its owner.
    sample: HashMap<ComponentId, Entity>,
    /// Entities that gained a `Name`, by name with its digits masked: what keeps being spawned.
    named: HashMap<String, u64>,
}

fn report_changes(world: &mut World, mut window: Local<ChangeWindow>) {
    let this_scan = world.read_change_tick();
    let Some(last_scan) = window.last_scan.replace(this_scan) else {
        return;
    };
    let changed = changed_this_frame(world, last_scan, this_scan);
    for (&id, &(count, entity)) in &changed {
        *window.changed.entry(id).or_default() += count;
        *window.frames_changed.entry(id).or_default() += 1;
        window.sample.insert(id, entity);
    }
    let mut names = world.query::<Ref<Name>>();
    for name in names.iter(world) {
        if name.added().is_newer_than(last_scan, this_scan) {
            *window.named.entry(name_pattern(name.as_str())).or_default() += 1;
        }
    }
    window.frames += 1;
    if window.frames < WINDOW {
        return;
    }
    let mut busiest: Vec<_> = window.changed.iter().map(|(&id, &n)| (id, n)).collect();
    busiest.sort_unstable_by_key(|(_, total)| std::cmp::Reverse(*total));
    let components = world.components();
    let registry = world.get_resource::<AppTypeRegistry>().map(|r| r.read());
    let describe_type = |id: ComponentId| -> String {
        let info = components.get_info(id);
        info.and_then(|info| info.type_id())
            .zip(registry.as_ref())
            .and_then(|(type_id, registry)| registry.get(type_id))
            .map(|registration| {
                registration
                    .type_info()
                    .type_path_table()
                    .short_path()
                    .to_owned()
            })
            .or_else(|| info.map(|info| info.name().shortname().to_string()))
            .unwrap_or_else(|| format!("{id:?}"))
    };
    // Release builds strip Bevy's component names (its `debug` feature); the reflection registry
    // keeps the type path of every registered type, so it names most components there.
    // The owner of a sample entity: its name, or its parent's, and its other reflected
    // components, so even an unnamed component type says whose it is.
    let owner = |entity: Entity| -> String {
        let named = world
            .get::<Name>(entity)
            .map(|name| name.as_str().to_owned())
            .or_else(|| {
                world
                    .get::<ChildOf>(entity)
                    .and_then(|parent| world.get::<Name>(parent.parent()))
                    .map(|name| format!("child of {name}"))
            })
            .unwrap_or_else(|| "unnamed".to_owned());
        let kinds: Vec<String> = world
            .inspect_entity(entity)
            .map(|infos| {
                infos
                    .filter(|info| {
                        info.type_id()
                            .zip(registry.as_ref())
                            .is_some_and(|(type_id, registry)| registry.get(type_id).is_some())
                    })
                    .take(10)
                    .map(|info| describe_type(info.id()))
                    .collect()
            })
            .unwrap_or_default();
        format!("{entity:?} {named} [{}]", kinds.join(", "))
    };
    let lines: Vec<String> = busiest
        .iter()
        .take(TOP)
        .map(|(id, total)| {
            format!(
                "{}: {:.1} entities/frame, changed in {} of {} frames, e.g. {}",
                describe_type(*id),
                *total as f64 / f64::from(window.frames),
                window.frames_changed.get(id).copied().unwrap_or(0),
                window.frames,
                window
                    .sample
                    .get(id)
                    .map_or_else(|| "-".to_owned(), |entity| owner(*entity))
            )
        })
        .collect();
    info!(
        component_types = busiest.len(),
        "main-world changes over {} frames (busiest first):\n  {}",
        window.frames,
        lines.join("\n  ")
    );
    let mut named: Vec<_> = window.named.iter().collect();
    named.sort_unstable_by_key(|(_, count)| std::cmp::Reverse(**count));
    if !named.is_empty() {
        let lines: Vec<String> = named
            .iter()
            .take(TOP)
            .map(|(pattern, count)| format!("{pattern}: {count}"))
            .collect();
        info!(
            "main-world entities named over {} frames (most first):
  {}",
            window.frames,
            lines.join(
                "
  "
            )
        );
    }
    window.frames = 0;
    window.changed.clear();
    window.frames_changed.clear();
    window.named.clear();
    window.sample.clear();
}

/// A name with every run of digits replaced by `#`, so "LOD terrain batch 12,7" and
/// "LOD terrain batch 3,1" count as one kind.
fn name_pattern(name: &str) -> String {
    let mut pattern = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_digit() {
            if !pattern.ends_with('#') {
                pattern.push('#');
            }
        } else {
            pattern.push(c);
        }
    }
    pattern
}

/// Entities whose component changed after `last_scan`, per component type.
fn changed_this_frame(
    world: &World,
    last_scan: bevy::ecs::change_detection::Tick,
    this_scan: bevy::ecs::change_detection::Tick,
) -> HashMap<ComponentId, (u64, Entity)> {
    let storages = world.storages();
    let mut changed: HashMap<ComponentId, (u64, Entity)> = HashMap::new();
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
                let mut first = None;
                let mut count = 0u64;
                for (row, tick) in ticks.iter().enumerate() {
                    if unsafe { *tick.get() }.is_newer_than(last_scan, this_scan) {
                        first.get_or_insert(row);
                        count += 1;
                    }
                }
                if let Some(row) = first {
                    let entity = table.entities()[row];
                    changed.entry(id).or_insert((0, entity)).0 += count;
                }
            }
        }
        for id in archetype.sparse_set_components() {
            let Some(set) = storages.sparse_sets.get(id) else {
                continue;
            };
            for entity in archetype.entities() {
                let entity = entity.id();
                // SAFETY: as above, the world is held exclusively and the tick is read by value.
                if set
                    .get_changed_tick(entity)
                    .is_some_and(|tick| unsafe { *tick.get() }.is_newer_than(last_scan, this_scan))
                {
                    changed.entry(id).or_insert((0, entity)).0 += 1;
                }
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
    fn names_count_as_one_kind_whatever_their_numbers() {
        assert_eq!(
            name_pattern("LOD terrain batch 12,7"),
            "LOD terrain batch #,#"
        );
        assert_eq!(name_pattern("cell 0x3c"), "cell #x#c");
    }

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
            changed
                .get(&id(&world, std::any::TypeId::of::<Moved>()))
                .map(|(count, _)| *count),
            Some(2)
        );
        assert_eq!(
            changed
                .get(&id(&world, std::any::TypeId::of::<Sparse>()))
                .map(|(count, sample)| (*count, *sample)),
            Some((1, entities[3]))
        );
        assert_eq!(
            changed.get(&id(&world, std::any::TypeId::of::<Still>())),
            None
        );
    }
}
