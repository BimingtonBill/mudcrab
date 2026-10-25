use bevy::ecs::{
    change_detection::Tick,
    component::{Component, StorageType},
    storage::TableRow,
    world::World,
};

pub(super) struct TickProbe {
    pub(super) changed: bool,
    pub(super) rows_checked: u64,
}

/// A conservative precheck for native `Changed<T>` queries.
///
/// `false` proves no stored value changed in this tick window. `true` only
/// requests the existing native query; its domain and invalidation policy still
/// decide what to do. Disabled entities are included in this advisory superset.
pub(super) fn component_changes<T: Component>(
    world: &World,
    last_run: Tick,
    this_run: Tick,
) -> TickProbe {
    let mut probe = TickProbe {
        changed: false,
        rows_checked: 0,
    };
    let Some(component_id) = world.component_id::<T>() else {
        return probe;
    };
    let Some(info) = world.components().get_info(component_id) else {
        probe.changed = true;
        return probe;
    };
    if info.storage_type() != StorageType::Table {
        probe.changed = true;
        return probe;
    }

    for table in world.storages().tables.iter() {
        let Some(column) = table.get_column(component_id) else {
            continue;
        };
        for row in 0..table.entity_count() {
            let Ok(row) = row.try_into() else {
                probe.changed = true;
                return probe;
            };
            // SAFETY: World table invariants give every column the same
            // initialized row length as the table's entity count. This row is
            // in that range. The shared World borrow excludes writers (the
            // parent system registers read-all access), so rows and ticks stay
            // valid throughout the copy. Only copied ticks are used locally;
            // no storage reference, cell, or pointer escapes this function.
            let ticks = unsafe { column.get_ticks_unchecked(TableRow::new(row)) };
            probe.rows_checked += 1;
            if ticks.changed.is_newer_than(last_run, this_run) {
                probe.changed = true;
                return probe;
            }
        }
    }
    probe
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::prelude::*;

    #[derive(Component, Clone, Copy, PartialEq, Debug)]
    struct Value(u64);
    use bevy::ecs::change_detection::Tick;
    use bevy::ecs::entity_disabling::{DefaultQueryFilters, Disabled};

    #[derive(Component)]
    struct Other;

    #[derive(Component)]
    struct Hidden;

    #[derive(Component)]
    #[component(storage = "SparseSet")]
    struct Sparse;

    // These tests compare the helper's early-out decision with Bevy's own filter.
    // The explicit tick API is unsafe only inside this test oracle.
    fn native_changes<T: Component>(world: &mut World, last: Tick, now: Tick) -> Vec<Entity> {
        let mut query = world.query_filtered::<Entity, Changed<T>>();
        // SAFETY: The query was created from this world and only reads Entity and
        // Changed<T>. The shared world borrow excludes writers for the entire query.
        unsafe {
            query
                .query_unchecked_with_ticks(world.as_unsafe_world_cell_readonly(), last, now)
                .iter()
                .collect()
        }
    }

    fn assert_native<T: Component>(world: &mut World, last: Tick, now: Tick, expected: bool) {
        let native = !native_changes::<T>(world, last, now).is_empty();
        assert_eq!(
            native, expected,
            "native fixture does not match expected boundary"
        );
        assert_eq!(component_changes::<T>(world, last, now).changed, native);
    }

    fn quiet_world() -> (World, Entity, Tick, Tick) {
        let mut world = World::new();
        let entity = world.spawn(Value(7)).id();
        let last = world.read_change_tick();
        world.increment_change_tick();
        let now = world.read_change_tick();
        (world, entity, last, now)
    }

    #[test]
    fn quiet_table_returns_false_and_counts_every_copied_row() {
        let (mut world, _, last, now) = quiet_world();
        assert_native::<Value>(&mut world, last, now, false);
        assert_eq!(
            component_changes::<Value>(&world, last, now).rows_checked,
            1
        );
    }

    #[test]
    fn unregistered_type_is_quiet_without_registering_it() {
        let world = World::new();
        let probe = component_changes::<Value>(&world, Tick::new(0), world.read_change_tick());
        assert!(!probe.changed);
        assert_eq!(probe.rows_checked, 0);
        assert!(world.component_id::<Value>().is_none());
    }

    #[test]
    fn registered_type_without_a_table_is_quiet() {
        let mut world = World::new();
        world.register_component::<Value>();
        let probe = component_changes::<Value>(&world, Tick::new(0), world.read_change_tick());
        assert!(!probe.changed);
        assert_eq!(probe.rows_checked, 0);
    }

    #[test]
    fn previously_populated_empty_table_is_quiet() {
        let (mut world, entity, last, now) = quiet_world();
        world.despawn(entity);
        assert_native::<Value>(&mut world, last, now, false);
        assert_eq!(
            component_changes::<Value>(&world, last, now).rows_checked,
            0
        );
    }

    #[test]
    fn insertion_and_equal_replacement_follow_native_changes() {
        let (mut world, entity, last, now) = quiet_world();
        assert_native::<Value>(&mut world, Tick::new(0), now, true);
        world.entity_mut(entity).insert(Value(7));
        assert_native::<Value>(&mut world, last, now, true);
        assert_eq!(
            component_changes::<Value>(&world, last, now).rows_checked,
            1
        );
    }

    #[test]
    fn world_get_mut_write_follows_native_changes() {
        let (mut world, entity, last, now) = quiet_world();
        world.get_mut::<Value>(entity).unwrap().0 = 8;
        assert_native::<Value>(&mut world, last, now, true);
    }

    #[test]
    fn query_write_follows_native_changes() {
        let (mut world, _, last, now) = quiet_world();
        for mut value in world.query::<&mut Value>().iter_mut(&mut world) {
            value.0 += 1;
        }
        assert_native::<Value>(&mut world, last, now, true);
    }

    #[test]
    fn equal_mutable_assignment_still_follows_native_changes() {
        let (mut world, entity, last, now) = quiet_world();
        *world.get_mut::<Value>(entity).unwrap() = Value(7);
        assert_native::<Value>(&mut world, last, now, true);
    }

    #[test]
    fn mutable_fetch_without_deref_mut_is_quiet() {
        let (mut world, entity, last, now) = quiet_world();
        let value = world.get_mut::<Value>(entity).unwrap();
        assert_eq!(value.0, 7);
        assert_native::<Value>(&mut world, last, now, false);
        for value in world.query::<&mut Value>().iter_mut(&mut world) {
            assert_eq!(value.0, 7);
        }
        assert_native::<Value>(&mut world, last, now, false);
    }

    #[test]
    fn shared_reads_are_quiet() {
        let (mut world, entity, last, now) = quiet_world();
        assert_eq!(world.get::<Value>(entity).unwrap().0, 7);
        for value in world.query::<&Value>().iter(&world) {
            assert_eq!(value.0, 7);
        }
        assert_native::<Value>(&mut world, last, now, false);
    }

    #[test]
    fn bypass_change_detection_remains_quiet_at_native_boundary() {
        let (mut world, entity, last, now) = quiet_world();
        world
            .get_mut::<Value>(entity)
            .unwrap()
            .bypass_change_detection()
            .0 = 99;
        assert_eq!(world.get::<Value>(entity).unwrap().0, 99);
        assert_native::<Value>(&mut world, last, now, false);
    }

    #[test]
    fn unrelated_component_mutation_is_quiet() {
        let (mut world, entity, last, now) = quiet_world();
        world.entity_mut(entity).insert(Other);
        assert_native::<Value>(&mut world, last, now, false);
    }

    #[test]
    fn row_movement_and_despawn_preserve_change_detection() {
        let mut world = World::new();
        let entities: Vec<_> = (0..8).map(|i| world.spawn(Value(i)).id()).collect();
        let last = world.read_change_tick();
        world.increment_change_tick();
        let now = world.read_change_tick();
        world.despawn(entities[0]);
        world.entity_mut(entities[3]).insert(Other);
        assert_native::<Value>(&mut world, last, now, false);
        assert_eq!(
            component_changes::<Value>(&world, last, now).rows_checked,
            7
        );
        world.get_mut::<Value>(entities[7]).unwrap().0 = 10;
        assert_native::<Value>(&mut world, last, now, true);
        world.despawn(entities[7]);
        assert_native::<Value>(&mut world, last, now, false);
        assert_eq!(
            component_changes::<Value>(&world, last, now).rows_checked,
            6
        );
    }

    #[test]
    fn new_tables_after_prior_probe_are_observed() {
        let (mut world, _, last, now) = quiet_world();
        assert_native::<Value>(&mut world, last, now, false);
        world.spawn((Value(8), Other));
        assert_native::<Value>(&mut world, last, now, true);
    }

    #[test]
    fn counts_rows_across_tables_but_skips_unrelated_tables() {
        let mut world = World::new();
        world.spawn(Value(1));
        world.spawn((Value(2), Other));
        world.spawn(Other);
        let now = world.read_change_tick();
        let probe = component_changes::<Value>(&world, now, now);
        assert!(!probe.changed);
        assert_eq!(probe.rows_checked, 2);
    }

    #[test]
    fn short_circuit_counts_only_copied_rows() {
        let mut world = World::new();
        world.spawn_batch((0..10).map(Value));
        let probe = component_changes::<Value>(&world, Tick::new(0), world.read_change_tick());
        assert!(probe.changed);
        assert_eq!(probe.rows_checked, 1);
    }

    #[test]
    fn consumers_use_independent_tick_windows() {
        let (mut world, entity, old, first_change) = quiet_world();
        world.get_mut::<Value>(entity).unwrap().0 = 9;
        world.increment_change_tick();
        let now = world.read_change_tick();
        assert_native::<Value>(&mut world, old, now, true);
        assert_native::<Value>(&mut world, first_change, now, false);
        assert_native::<Value>(&mut world, old, now, true);
        assert_native::<Value>(&mut world, now, now, false);
    }

    #[test]
    fn explicit_wrapped_ticks_match_native_filter() {
        let (mut world, entity, _, _) = quiet_world();
        world
            .get_mut::<Value>(entity)
            .unwrap()
            .set_last_changed(Tick::new(u32::MAX - 2));
        assert_native::<Value>(&mut world, Tick::new(u32::MAX - 5), Tick::new(3), true);
        assert_native::<Value>(&mut world, Tick::new(0), Tick::new(3), false);
    }

    #[test]
    fn expired_tick_ages_match_native_filter() {
        let (mut world, entity, _, _) = quiet_world();
        world
            .get_mut::<Value>(entity)
            .unwrap()
            .set_last_changed(Tick::new(1));
        let now = Tick::new(Tick::MAX.get() + 50);
        assert_native::<Value>(&mut world, Tick::new(0), now, false);
    }

    #[test]
    fn disabled_entities_are_a_conservative_superset() {
        let mut world = World::new();
        world.spawn((Value(1), Disabled));
        let now = world.read_change_tick();
        assert!(native_changes::<Value>(&mut world, Tick::new(0), now).is_empty());
        assert!(component_changes::<Value>(&world, Tick::new(0), now).changed);
        assert!(!component_changes::<Value>(&world, now, now).changed);
    }

    #[test]
    fn custom_disabler_is_a_conservative_superset() {
        let mut world = World::new();
        let hidden = world.register_component::<Hidden>();
        world
            .resource_mut::<DefaultQueryFilters>()
            .register_disabling_component(hidden);
        world.spawn((Value(1), Hidden));
        let now = world.read_change_tick();
        assert!(native_changes::<Value>(&mut world, Tick::new(0), now).is_empty());
        assert!(component_changes::<Value>(&world, Tick::new(0), now).changed);
        assert!(!component_changes::<Value>(&world, now, now).changed);
    }

    #[test]
    fn unregistered_sparse_type_is_quiet() {
        let world = World::new();
        let probe = component_changes::<Sparse>(&world, Tick::new(0), world.read_change_tick());
        assert!(!probe.changed);
        assert_eq!(probe.rows_checked, 0);
    }

    #[test]
    fn registered_sparse_type_always_falls_back_without_scanning() {
        let mut world = World::new();
        world.register_component::<Sparse>();
        let now = world.read_change_tick();
        let probe = component_changes::<Sparse>(&world, now, now);
        assert!(probe.changed);
        assert_eq!(probe.rows_checked, 0);
        world.spawn(Sparse);
        assert!(native_changes::<Sparse>(&mut world, now, now).is_empty());
        let probe = component_changes::<Sparse>(&world, now, now);
        assert!(probe.changed);
        assert_eq!(probe.rows_checked, 0);
    }
}
