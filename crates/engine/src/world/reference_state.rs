//! Session enable state shared by streamed roots and distant reference consumers.

use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    sync::Arc,
};

use crate::cluster::flat_source::{FlatSourceOwner, FlatSourcePrimitives};
use bevy::{ecs::entity_disabling::Disabled, ecs::query::Allow, prelude::*};
use color_eyre::{Result, eyre::WrapErr};
use rusqlite::Connection;

use super::{
    components::FormId,
    enable_state::{
        EnableState, EnableStateError, EnableStateResolver, InitialEnableInputs, PLAYER_REFERENCE,
    },
};

/// Immutable enable definitions and initial resolution for one winning-record database.
pub struct EnableCatalog {
    definitions: HashMap<u32, InitialEnableInputs>,
    children: HashMap<u32, Vec<u32>>,
    initial: EnableStateResolver,
}

impl EnableCatalog {
    /// Reads the winning reference table once; runtime resolution performs no database IO.
    pub fn read(connection: &Connection) -> Result<Self> {
        let count: usize = connection.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('references') \
             WHERE name IN ('header_flags','enable_parent_id','enable_parent_flags')",
            [],
            |row| row.get(0),
        )?;
        let columns = match count {
            0 => {
                warn!(
                    "world database has no reference enable-state metadata; references default to enabled; reconvert assets to filter initially disabled references"
                );
                "0,NULL,0"
            }
            3 => "header_flags,enable_parent_id,enable_parent_flags",
            _ => color_eyre::eyre::bail!(
                "incomplete reference enable-state columns: expected header_flags, enable_parent_id and enable_parent_flags"
            ),
        };
        let mut statement = connection.prepare(&format!(
            "SELECT id,{columns} FROM \"references\" ORDER BY id"
        ))?;
        let mut definitions = HashMap::new();
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut initial = EnableStateResolver::default();
        let mut rows = statement.query([])?;
        let mut order = Vec::new();
        while let Some(row) = rows.next()? {
            let reference = row.get(0)?;
            let inputs = super::database::map_enable_inputs(row, 1)
                .wrap_err_with(|| format!("enable metadata of reference {reference:08X}"))?;
            // The resolver's PlayerRef bootstrap is a terminal seed even if a row exists.
            if reference != PLAYER_REFERENCE
                && let Some(parent) = inputs.parent_id.filter(|parent| *parent != 0)
            {
                children.entry(parent).or_default().push(reference);
            }
            definitions.insert(reference, inputs);
            order.push(reference);
        }
        let mut diagnosed = HashSet::new();
        for reference in order {
            let state = resolve_definition(&mut initial, &definitions, reference);
            if let Err(error) = state
                && diagnosed.insert(error)
            {
                warn!(reference, %error, "unresolved reference enable state stays hidden");
            }
        }
        Ok(Self {
            definitions,
            children,
            initial,
        })
    }

    /// Initial state, unaffected by session overrides; unresolved or absent IDs return `None`.
    pub fn enabled(&self, reference: u32) -> Option<bool> {
        self.initial.cached(reference).and_then(Result::ok)
    }
}

fn resolve_definition(
    resolver: &mut EnableStateResolver,
    definitions: &HashMap<u32, InitialEnableInputs>,
    reference: u32,
) -> EnableState {
    if let Some(state) = resolver.cached(reference) {
        return state;
    }
    // Only known references are invalidated; PlayerRef always receives an explicit seed.
    let Some(inputs) = definitions.get(&reference).copied() else {
        return Err(EnableStateError::MissingParent(reference));
    };
    resolver
        .resolve(reference, inputs, |parent| {
            Ok::<_, Infallible>(definitions.get(&parent).copied())
        })
        .unwrap_or_else(|never| match never {})
}

/// Temporary reference gate, separate from each source node's authored visibility.
/// Also mark visibility-less pivots: moving a gated subtree then needs no body scan
/// to decide whether its old gate must be removed. Despawn cleans up the marker.
#[derive(Component, Clone, Copy)]
#[component(storage = "SparseSet")]
pub(crate) struct ReferenceVisibilityGate(pub(crate) Option<Visibility>);

struct RegisteredRoot {
    reference: u32,
    saved_visibility: Option<Visibility>,
}

/// Runtime state survives entity unload/reload and repeated binding of the same catalog.
#[derive(Resource, Default)]
pub struct ReferenceEnableState {
    catalog: Option<Arc<EnableCatalog>>,
    resolver: EnableStateResolver,
    overrides: HashMap<u32, bool>,
    revision: u64,
    roots: HashMap<u32, HashSet<Entity>>,
    entities: HashMap<Entity, RegisteredRoot>,
    #[cfg(test)]
    recomputed: Vec<u32>,
}

impl ReferenceEnableState {
    /// Changed winning inputs reset overrides and return true; recertifying the same inputs
    /// preserves session state. The caller reconciles surviving roots after a true result.
    pub fn bind(&mut self, catalog: Arc<EnableCatalog>) -> bool {
        if self.catalog.as_ref().is_some_and(|current| {
            Arc::ptr_eq(current, &catalog) || current.definitions == catalog.definitions
        }) {
            self.catalog = Some(catalog);
            return false;
        }
        self.resolver = catalog.initial.clone();
        self.catalog = Some(catalog);
        self.overrides.clear();
        self.revision = self.revision.wrapping_add(1);
        #[cfg(test)]
        self.recomputed.clear();
        true
    }

    /// Reconcile the existing identity index once when winning inputs change, never per frame.
    pub(crate) fn reconcile_roots(&mut self, world: &mut World) {
        let entities: Vec<_> = self.entities.keys().copied().collect();
        for &entity in &entities {
            if world.get::<FormId>(entity).map(|form| form.0)
                != self.entities.get(&entity).map(|root| root.reference)
            {
                continue;
            }
            let authored = world
                .get::<ReferenceVisibilityGate>(entity)
                .and_then(|gate| gate.0);
            if let Some(mut visibility) = world.get_mut::<Visibility>(entity) {
                self.apply_visibility(entity, &mut visibility, authored);
            }
        }
        for entity in entities {
            reconcile_source_subtree(world, self, entity);
        }
    }

    pub fn enabled(&self, reference: u32) -> Option<bool> {
        self.catalog.as_ref()?;
        self.resolver.cached(reference).and_then(Result::ok)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn is_bound(&self) -> bool {
        self.catalog.is_some()
    }

    fn set(&mut self, reference: u32, enabled: bool) -> Vec<ReferenceEnableChanged> {
        let Some(catalog) = self.catalog.as_ref() else {
            warn!(
                reference,
                "cannot set reference enable state without a bound catalog"
            );
            return Vec::new();
        };
        if reference != PLAYER_REFERENCE && !catalog.definitions.contains_key(&reference) {
            warn!(reference, "cannot set enable state of an unknown reference");
            return Vec::new();
        }
        if self.overrides.get(&reference) == Some(&enabled) {
            return Vec::new();
        }
        self.overrides.insert(reference, enabled);
        // Even an unchanged effective value creates a persistent explicit override.
        if self.resolver.cached(reference) == Some(Ok(enabled)) {
            return Vec::new();
        }
        let mut affected = Vec::new();
        let mut pending = vec![reference];
        let mut visited = HashSet::new();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            // An explicitly overridden child insulates its whole subtree from this change.
            if id != reference && self.overrides.contains_key(&id) {
                continue;
            }
            affected.push((id, self.resolver.cached(id).and_then(Result::ok)));
            if let Some(children) = catalog.children.get(&id) {
                pending.extend(children.iter().copied());
            }
        }
        for (id, _) in &affected {
            self.resolver.invalidate(*id);
        }
        self.resolver.seed(reference, Ok(enabled));
        let mut changes = Vec::new();
        for (id, before) in affected {
            #[cfg(test)]
            self.recomputed.push(id);
            let state = resolve_definition(&mut self.resolver, &catalog.definitions, id);
            if let Ok(enabled) = state
                && before != Some(enabled)
            {
                changes.push(ReferenceEnableChanged {
                    reference: id,
                    enabled,
                });
            }
        }
        if !changes.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        changes
    }

    fn register(
        &mut self,
        entity: Entity,
        reference: u32,
        visibility: Option<&mut Visibility>,
        authored: Option<Visibility>,
    ) {
        if self
            .entities
            .get(&entity)
            .is_some_and(|root| root.reference == reference)
        {
            if let Some(visibility) = visibility {
                self.apply_visibility(entity, visibility, authored);
            }
            return;
        }
        let mut visibility = visibility;
        self.unregister(entity, visibility.as_deref_mut());
        self.entities.insert(
            entity,
            RegisteredRoot {
                reference,
                saved_visibility: None,
            },
        );
        self.roots.entry(reference).or_default().insert(entity);
        if let Some(visibility) = visibility {
            self.apply_visibility(entity, visibility, authored);
        }
    }

    fn unregister(&mut self, entity: Entity, visibility: Option<&mut Visibility>) {
        let Some(root) = self.entities.remove(&entity) else {
            return;
        };
        if let Some(roots) = self.roots.get_mut(&root.reference) {
            roots.remove(&entity);
            if roots.is_empty() {
                self.roots.remove(&root.reference);
            }
        }
        if let Some(visibility) = visibility
            && let Some(saved) = root.saved_visibility
            && *visibility != saved
        {
            *visibility = saved;
        }
    }

    fn apply_visibility(
        &mut self,
        entity: Entity,
        visibility: &mut Visibility,
        authored: Option<Visibility>,
    ) {
        if !self.is_bound() {
            return;
        }
        let Some(reference) = self.entities.get(&entity).map(|root| root.reference) else {
            return;
        };
        let enabled = self.enabled(reference) == Some(true);
        let Some(root) = self.entities.get_mut(&entity) else {
            return;
        };
        let desired = if enabled {
            root.saved_visibility.take().unwrap_or(*visibility)
        } else {
            // A nested reference may already be hidden by an ancestor's gate.
            root.saved_visibility
                .get_or_insert(authored.unwrap_or(*visibility));
            Visibility::Hidden
        };
        if *visibility != desired {
            *visibility = desired;
        }
    }
}

/// Explicitly enable or disable a placed reference for this session.
#[derive(Event, Clone, Copy, Debug)]
pub struct SetReferenceEnabled {
    pub reference: u32,
    pub enabled: bool,
}

/// Effective enable-state change, separate from permanent pose or gameplay revocations.
#[derive(Event, Clone, Copy, Debug)]
pub struct ReferenceEnableChanged {
    pub reference: u32,
    pub enabled: bool,
}

pub fn install(app: &mut App) {
    if app.world().contains_resource::<EnableObserversInstalled>() {
        return;
    }
    app.init_resource::<ReferenceEnableState>()
        .init_resource::<EnableObserversInstalled>()
        .add_observer(set_reference_enabled)
        .add_observer(register_form_id)
        .add_observer(remove_form_id)
        .add_observer(register_visibility)
        .add_observer(discard_visibility)
        .add_observer(source_graph_inserted)
        .add_observer(source_graph_removed);
}

#[derive(Resource, Default)]
struct EnableObserversInstalled;

fn set_reference_enabled(
    event: On<SetReferenceEnabled>,
    mut state: ResMut<ReferenceEnableState>,
    mut roots: Query<(&FormId, &mut Visibility, Option<&ReferenceVisibilityGate>), Allow<Disabled>>,
    mut commands: Commands,
) {
    let changes = state.set(event.reference, event.enabled);
    for change in changes {
        let entities: Vec<_> = state
            .roots
            .get(&change.reference)
            .map(|roots| roots.iter().copied().collect())
            .unwrap_or_default();
        for entity in entities {
            if let Ok((reference, mut visibility, gate)) = roots.get_mut(entity)
                && reference.0 == change.reference
            {
                state.apply_visibility(entity, &mut visibility, gate.and_then(|gate| gate.0));
            }
            queue_source_subtree(&mut commands, entity);
        }
        commands.trigger(change);
    }
}

type RegistrationRoots<'w, 's> = Query<
    'w,
    's,
    (
        &'static FormId,
        Option<&'static mut Visibility>,
        Option<&'static ReferenceVisibilityGate>,
    ),
    Allow<Disabled>,
>;

fn register_form_id(
    event: On<Insert, FormId>,
    mut state: ResMut<ReferenceEnableState>,
    mut roots: RegistrationRoots,
    mut commands: Commands,
) {
    if let Ok((reference, mut visibility, gate)) = roots.get_mut(event.entity) {
        state.register(
            event.entity,
            reference.0,
            visibility.as_deref_mut(),
            gate.and_then(|gate| gate.0),
        );
    }
    queue_source_subtree(&mut commands, event.entity);
}

fn remove_form_id(
    event: On<Remove, FormId>,
    mut state: ResMut<ReferenceEnableState>,
    mut roots: Query<&mut Visibility, Allow<Disabled>>,
    mut commands: Commands,
) {
    let mut visibility = roots.get_mut(event.entity).ok();
    state.unregister(event.entity, visibility.as_deref_mut());
    queue_source_subtree(&mut commands, event.entity);
}

fn register_visibility(
    event: On<Insert, Visibility>,
    mut state: ResMut<ReferenceEnableState>,
    mut roots: Query<(&FormId, &mut Visibility, Option<&ReferenceVisibilityGate>), Allow<Disabled>>,
    mut commands: Commands,
) {
    if let Ok((reference, mut visibility, gate)) = roots.get_mut(event.entity) {
        state.register(
            event.entity,
            reference.0,
            Some(&mut visibility),
            gate.and_then(|gate| gate.0),
        );
    }
    queue_source_subtree(&mut commands, event.entity);
}

fn discard_visibility(
    event: On<Discard, Visibility>,
    mut state: ResMut<ReferenceEnableState>,
    mut gates: Query<&mut ReferenceVisibilityGate, Allow<Disabled>>,
) {
    if let Some(root) = state.entities.get_mut(&event.entity) {
        root.saved_visibility = None;
    }
    // A replacement is a new authored value; removing Visibility must not keep
    // a stale value that could overwrite a later insertion.
    if let Ok(mut gate) = gates.get_mut(event.entity) {
        gate.0 = None;
    }
}

fn source_graph_inserted(
    event: On<Insert, (Mesh3d, ChildOf, FlatSourceOwner)>,
    mut commands: Commands,
) {
    queue_source_subtree(&mut commands, event.entity);
}

fn source_graph_removed(event: On<Remove, (ChildOf, FlatSourceOwner)>, mut commands: Commands) {
    queue_source_subtree(&mut commands, event.entity);
}

fn queue_source_subtree(commands: &mut Commands, entity: Entity) {
    // Relationship backlinks and removals must finish before walking the graph.
    commands.queue(move |world: &mut World| {
        world.resource_scope(|world, state: Mut<ReferenceEnableState>| {
            reconcile_source_subtree(world, &state, entity);
        });
    });
}

fn source_parent(world: &World, entity: Entity) -> Option<Entity> {
    world
        .get::<ChildOf>(entity)
        .map(ChildOf::parent)
        .or_else(|| world.get::<FlatSourceOwner>(entity).map(|owner| owner.0))
}

fn reference_is_disabled(world: &World, state: &ReferenceEnableState, entity: Entity) -> bool {
    state.is_bound()
        && state.entities.get(&entity).is_some_and(|root| {
            world.get::<FormId>(entity).map(|form| form.0) == Some(root.reference)
                && state.enabled(root.reference) != Some(true)
        })
}

fn source_has_disabled_ancestor(
    world: &World,
    state: &ReferenceEnableState,
    entity: Entity,
) -> bool {
    let mut current = Some(entity);
    let mut visited = HashSet::new();
    while let Some(entity) = current {
        if !visited.insert(entity) || reference_is_disabled(world, state, entity) {
            return true;
        }
        current = source_parent(world, entity);
    }
    false
}

fn source_parent_visible(world: &World, entity: Entity) -> bool {
    let mut current = source_parent(world, entity);
    let mut visited = HashSet::new();
    while let Some(entity) = current {
        if !visited.insert(entity) {
            return false;
        }
        // Match Bevy's missing-parent-component rule, but evaluate the authored
        // chain directly: a Disabled parent's cached inherited value can be stale.
        if world.get::<InheritedVisibility>(entity).is_none() {
            return true;
        }
        match world.get::<Visibility>(entity) {
            Some(Visibility::Hidden) => return false,
            Some(Visibility::Visible) | None => return true,
            Some(Visibility::Inherited) => current = source_parent(world, entity),
        }
    }
    true
}

fn reconcile_source_subtree(world: &mut World, state: &ReferenceEnableState, root: Entity) {
    if world.get_entity(root).is_err() {
        return;
    }
    let disabled = source_has_disabled_ancestor(world, state, root);
    // Ordinary enabled scene insertion costs an ancestry walk, never a subtree
    // scan. Every gated pivot carries a marker, so reparenting can undo its gate.
    if !disabled && world.get::<ReferenceVisibilityGate>(root).is_none() {
        return;
    }
    let mut pending = vec![(root, disabled, source_parent_visible(world, root))];
    let mut visited = HashSet::new();
    while let Some((entity, ancestor_disabled, parent_visible)) = pending.pop() {
        if !visited.insert(entity) || world.get_entity(entity).is_err() {
            continue;
        }
        let disabled = ancestor_disabled || reference_is_disabled(world, state, entity);
        let gate = world.get::<ReferenceVisibilityGate>(entity).copied();
        let current = world.get::<Visibility>(entity).copied();
        let visibility = if disabled {
            let authored = state
                .entities
                .get(&entity)
                .and_then(|root| root.saved_visibility)
                .or_else(|| gate.and_then(|gate| gate.0))
                .or(current);
            if gate.is_none_or(|gate| gate.0 != authored) {
                world
                    .entity_mut(entity)
                    .insert(ReferenceVisibilityGate(authored));
            }
            current.map(|_| Visibility::Hidden)
        } else {
            if gate.is_some() {
                world.entity_mut(entity).remove::<ReferenceVisibilityGate>();
            }
            gate.and_then(|gate| gate.0).or(current)
        };
        // Mutate in place: Insert observers must not recursively schedule us.
        if let (Some(next), Some(mut current)) = (visibility, world.get_mut::<Visibility>(entity))
            && *current != next
        {
            *current = next;
        }
        let visible = !disabled
            && match visibility {
                Some(Visibility::Hidden) => false,
                Some(Visibility::Inherited) => parent_visible,
                Some(Visibility::Visible) | None => true,
            };
        if let Some(mut inherited) = world.get_mut::<InheritedVisibility>(entity) {
            let next = if visible {
                InheritedVisibility::VISIBLE
            } else {
                InheritedVisibility::HIDDEN
            };
            if *inherited != next {
                *inherited = next;
            }
        }
        let mut children: Vec<_> = world
            .get::<Children>(entity)
            .map(|children| children.iter().collect())
            .unwrap_or_default();
        if let Some(flat) = world.get::<FlatSourcePrimitives>(entity) {
            children.extend(flat.iter());
        }
        for child in children {
            // ChildOf takes precedence over flat ownership, as in source identity.
            if source_parent(world, child) == Some(entity) {
                pending.push((child, disabled, visible));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{components::FormId, enable_state::PLAYER_REFERENCE};
    use bevy::ecs::entity_disabling::Disabled;

    #[derive(Resource, Default)]
    struct Changed(Vec<(u32, bool)>);

    fn database(rows: &str) -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE \"references\" (id INTEGER PRIMARY KEY, header_flags INTEGER NOT NULL, \
                 enable_parent_id INTEGER, enable_parent_flags INTEGER); \
                 INSERT INTO \"references\" VALUES {rows};"
            ))
            .unwrap();
        connection
    }

    fn catalog(rows: &str) -> Arc<EnableCatalog> {
        Arc::new(EnableCatalog::read(&database(rows)).unwrap())
    }

    fn app(catalog: Arc<EnableCatalog>) -> App {
        let mut app = App::new();
        install(&mut app);
        app.init_resource::<Changed>().add_observer(
            |event: On<ReferenceEnableChanged>, mut changed: ResMut<Changed>| {
                changed.0.push((event.reference, event.enabled));
            },
        );
        app.world_mut()
            .resource_mut::<ReferenceEnableState>()
            .bind(catalog);
        app
    }

    fn set(app: &mut App, reference: u32, enabled: bool) {
        app.world_mut()
            .trigger(SetReferenceEnabled { reference, enabled });
        app.world_mut().flush();
    }

    fn enabled(app: &App, reference: u32) -> Option<bool> {
        app.world()
            .resource::<ReferenceEnableState>()
            .enabled(reference)
    }

    fn revision(app: &App) -> u64 {
        app.world().resource::<ReferenceEnableState>().revision()
    }

    #[test]
    fn install_is_idempotent_and_unchanged_events_leave_visibility_untouched() {
        let mut app = app(catalog("(1,0,NULL,0)"));
        let observer_count = app
            .world_mut()
            .query::<&Observer>()
            .iter(app.world())
            .count();
        install(&mut app);
        assert_eq!(
            app.world_mut()
                .query::<&Observer>()
                .iter(app.world())
                .count(),
            observer_count
        );
        let root = app.world_mut().spawn((FormId(1), Visibility::Visible)).id();
        app.world_mut().clear_trackers();
        let before = revision(&app);
        set(&mut app, 1, true);
        assert_eq!(revision(&app), before);
        assert!(
            !app.world()
                .entity(root)
                .get_ref::<Visibility>()
                .unwrap()
                .is_changed()
        );
        assert!(app.world().resource::<Changed>().0.is_empty());
    }

    #[test]
    fn initially_disabled_reference_stays_hidden_until_runtime_enable() {
        let mut app = app(catalog("(1,2048,NULL,NULL)"));
        assert!(app.world().resource::<ReferenceEnableState>().is_bound());
        assert_eq!(enabled(&app, 1), Some(false));
        let root = app
            .world_mut()
            .spawn((FormId(1), Visibility::Inherited))
            .id();
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
        let before = revision(&app);
        set(&mut app, 1, true);
        assert_eq!(enabled(&app, 1), Some(true));
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Inherited)
        );
        assert_eq!(revision(&app), before + 1);
        assert_eq!(app.world().resource::<Changed>().0, [(1, true)]);
    }

    #[test]
    fn parent_toggle_updates_only_transitive_dependents_and_opposite_chains() {
        let mut app = app(catalog(
            "(1,0,NULL,0),(2,2048,1,0),(3,0,2,1),(4,0,3,1),(9,0,NULL,0)",
        ));
        set(&mut app, 1, false);
        assert_eq!(
            (
                enabled(&app, 1),
                enabled(&app, 2),
                enabled(&app, 3),
                enabled(&app, 4)
            ),
            (Some(false), Some(false), Some(true), Some(false))
        );
        assert_eq!(enabled(&app, 9), Some(true));
        let mut changed = app.world().resource::<Changed>().0.clone();
        changed.sort_unstable();
        assert_eq!(changed, [(1, false), (2, false), (3, true), (4, false)]);
        let state = app.world().resource::<ReferenceEnableState>();
        let mut recomputed = state.recomputed.clone();
        recomputed.sort_unstable();
        assert_eq!(recomputed, [1, 2, 3, 4]);
        set(&mut app, 1, true);
        assert_eq!(
            (enabled(&app, 2), enabled(&app, 3), enabled(&app, 4)),
            (Some(true), Some(false), Some(true))
        );
    }

    #[test]
    fn explicit_override_survives_parent_changes_even_when_originally_unchanged() {
        let mut app = app(catalog("(1,0,NULL,0),(2,0,1,0),(3,0,2,1)"));
        let before = revision(&app);
        set(&mut app, 2, true);
        assert_eq!(revision(&app), before);
        assert!(app.world().resource::<Changed>().0.is_empty());
        assert!(
            app.world()
                .resource::<ReferenceEnableState>()
                .recomputed
                .is_empty()
        );
        set(&mut app, 1, false);
        assert_eq!(enabled(&app, 1), Some(false));
        assert_eq!(enabled(&app, 2), Some(true));
        assert_eq!(enabled(&app, 3), Some(false));
        assert_eq!(app.world().resource::<Changed>().0, [(1, false)]);
    }

    #[test]
    fn repeated_event_has_no_revision_notifications_or_resolution_work() {
        let mut app = app(catalog("(1,0,NULL,0),(2,0,1,0)"));
        set(&mut app, 1, false);
        let before = revision(&app);
        let work = app
            .world()
            .resource::<ReferenceEnableState>()
            .recomputed
            .clone();
        let notifications = app.world().resource::<Changed>().0.clone();
        set(&mut app, 1, false);
        assert_eq!(enabled(&app, 2), Some(false));
        assert_eq!(revision(&app), before);
        assert_eq!(
            app.world().resource::<ReferenceEnableState>().recomputed,
            work
        );
        assert_eq!(app.world().resource::<Changed>().0, notifications);
    }

    #[test]
    fn session_overrides_survive_streaming_and_same_catalog_bind() {
        let catalog = catalog("(1,2048,NULL,0)");
        let mut app = app(Arc::clone(&catalog));
        let root = app
            .world_mut()
            .spawn((FormId(1), Visibility::Inherited))
            .id();
        set(&mut app, 1, true);
        app.world_mut().despawn(root);
        let before = revision(&app);
        app.world_mut()
            .resource_mut::<ReferenceEnableState>()
            .bind(catalog);
        assert_eq!(revision(&app), before);
        let root = app.world_mut().spawn((FormId(1), Visibility::Visible)).id();
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Visible)
        );
        set(&mut app, 1, false);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn different_catalog_resets_overrides_and_preserves_live_root_registry() {
        let mut app = app(catalog("(1,2048,NULL,0)"));
        let root = app
            .world_mut()
            .spawn((FormId(1), Visibility::Inherited))
            .id();
        set(&mut app, 1, true);
        app.world_mut()
            .resource_mut::<ReferenceEnableState>()
            .bind(catalog("(1,0,NULL,0)"));
        assert_eq!(enabled(&app, 1), Some(true));
        set(&mut app, 1, false);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
        set(&mut app, 1, true);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Inherited)
        );
    }

    #[test]
    fn recertified_equal_inputs_preserve_overrides_and_changed_inputs_reconcile_surviving_roots() {
        let mut app = app(catalog("(1,0,NULL,0)"));
        let root = app.world_mut().spawn((FormId(1), Visibility::Visible)).id();
        set(&mut app, 1, false);
        let before = revision(&app);
        assert!(
            !app.world_mut()
                .resource_mut::<ReferenceEnableState>()
                .bind(catalog("(1,0,NULL,0)"))
        );
        assert_eq!(enabled(&app, 1), Some(false));
        assert_eq!(revision(&app), before);
        assert!(
            app.world_mut()
                .resource_mut::<ReferenceEnableState>()
                .bind(catalog("(1,0,NULL,0),(2,0,NULL,0)"))
        );
        app.world_mut()
            .resource_scope(|world, mut state: Mut<ReferenceEnableState>| {
                state.reconcile_roots(world);
            });
        assert_eq!(enabled(&app, 1), Some(true));
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Visible)
        );
    }

    #[test]
    fn missing_parents_cycles_and_unknown_ids_fail_closed_until_explicit_resolution() {
        let mut app = app(catalog("(1,0,99,1),(2,0,3,0),(3,0,2,1),(4,0,1,1)"));
        for reference in [1, 2, 3, 4, 99] {
            assert_eq!(enabled(&app, reference), None);
            let root = app
                .world_mut()
                .spawn((FormId(reference), Visibility::Visible))
                .id();
            assert_eq!(
                app.world().get::<Visibility>(root),
                Some(&Visibility::Hidden)
            );
        }
        let before = revision(&app);
        set(&mut app, 99, true);
        assert_eq!(revision(&app), before);
        assert_eq!(enabled(&app, 99), None);
        set(&mut app, 1, true);
        assert_eq!(enabled(&app, 1), Some(true));
        assert_eq!(enabled(&app, 4), Some(false));
        set(&mut app, 2, false);
        assert_eq!(enabled(&app, 2), Some(false));
        assert_eq!(enabled(&app, 3), Some(true));
    }

    #[test]
    fn player_bootstrap_can_be_overridden_without_a_database_row() {
        let mut app = app(catalog("(1,2048,20,0),(2,0,20,1)"));
        assert_eq!(enabled(&app, PLAYER_REFERENCE), Some(true));
        set(&mut app, PLAYER_REFERENCE, false);
        assert_eq!(enabled(&app, PLAYER_REFERENCE), Some(false));
        assert_eq!(enabled(&app, 1), Some(false));
        assert_eq!(enabled(&app, 2), Some(true));
    }

    #[test]
    fn roots_restore_their_own_prior_visibility_including_disabled_entities() {
        let mut app = app(catalog("(1,0,NULL,0)"));
        let roots: Vec<_> = [
            Visibility::Visible,
            Visibility::Inherited,
            Visibility::Hidden,
        ]
        .into_iter()
        .map(|visibility| {
            (
                app.world_mut()
                    .spawn((FormId(1), visibility, Disabled))
                    .id(),
                visibility,
            )
        })
        .collect();
        set(&mut app, 1, false);
        for (entity, _) in &roots {
            assert_eq!(
                app.world().get::<Visibility>(*entity),
                Some(&Visibility::Hidden)
            );
        }
        set(&mut app, 1, true);
        for (entity, visibility) in roots {
            assert_eq!(app.world().get::<Visibility>(entity), Some(&visibility));
        }
    }

    #[test]
    fn disabled_reference_root_gates_real_descendant_and_flat_draw_visibility() {
        use crate::cluster::flat_source::FlatSourceOwner;
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            bevy::transform::TransformPlugin,
            bevy::camera::visibility::VisibilityPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>();
        install(&mut app);
        app.world_mut()
            .resource_mut::<ReferenceEnableState>()
            .bind(catalog("(1,0,NULL,0)"));
        let root = app
            .world_mut()
            .spawn((FormId(1), Transform::default(), Visibility::Inherited))
            .id();
        let body = app
            .world_mut()
            .spawn((Mesh3d::default(), Visibility::Visible, ChildOf(root)))
            .id();
        let flat = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                Visibility::Visible,
                FlatSourceOwner(root),
            ))
            .id();
        app.update();
        assert!(app.world().get::<InheritedVisibility>(body).unwrap().get());
        app.world_mut().entity_mut(root).insert(Disabled);
        set(&mut app, 1, false);
        app.update();
        for draw in [body, flat] {
            assert_eq!(
                app.world().get::<Visibility>(draw),
                Some(&Visibility::Hidden)
            );
            assert!(!app.world().get::<InheritedVisibility>(draw).unwrap().get());
        }
        let late = app
            .world_mut()
            .spawn((Mesh3d::default(), Visibility::Visible, ChildOf(root)))
            .id();
        app.update();
        assert!(!app.world().get::<InheritedVisibility>(late).unwrap().get());
        set(&mut app, 1, true);
        app.update();
        for draw in [body, flat, late] {
            assert_eq!(
                app.world().get::<Visibility>(draw),
                Some(&Visibility::Visible)
            );
            assert!(app.world().get::<InheritedVisibility>(draw).unwrap().get());
        }
    }

    #[test]
    fn form_id_replacement_removal_and_despawn_cannot_leave_stale_visibility_writes() {
        let mut app = app(catalog("(1,2048,NULL,0),(2,0,NULL,0)"));
        let root = app.world_mut().spawn((FormId(1), Visibility::Visible)).id();
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
        app.world_mut().entity_mut(root).insert(FormId(2));
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Visible)
        );
        set(&mut app, 1, true);
        set(&mut app, 2, false);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
        app.world_mut().entity_mut(root).remove::<FormId>();
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Visible)
        );
        set(&mut app, 2, true);
        set(&mut app, 2, false);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Visible)
        );
        app.world_mut().despawn(root);
        set(&mut app, 2, true);
    }

    #[test]
    fn registry_tracks_visibility_added_after_form_id_and_roots_before_binding() {
        let mut app = App::new();
        install(&mut app);
        assert!(!app.world().resource::<ReferenceEnableState>().is_bound());
        assert_eq!(enabled(&app, 1), None);
        let root = app.world_mut().spawn(FormId(1)).id();
        app.world_mut().entity_mut(root).insert(Visibility::Visible);
        app.world_mut()
            .resource_mut::<ReferenceEnableState>()
            .bind(catalog("(1,0,NULL,0)"));
        set(&mut app, 1, false);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Hidden)
        );
        app.world_mut().entity_mut(root).remove::<Visibility>();
        app.world_mut()
            .entity_mut(root)
            .insert(Visibility::Inherited);
        set(&mut app, 1, true);
        assert_eq!(
            app.world().get::<Visibility>(root),
            Some(&Visibility::Inherited)
        );
    }

    #[test]
    fn catalog_accepts_legacy_columns_and_rejects_partial_or_invalid_enable_metadata() {
        let legacy = Connection::open_in_memory().unwrap();
        legacy.execute_batch("CREATE TABLE \"references\" (id INTEGER PRIMARY KEY); INSERT INTO \"references\" VALUES (1);").unwrap();
        let mut state = ReferenceEnableState::default();
        state.bind(Arc::new(EnableCatalog::read(&legacy).unwrap()));
        assert_eq!(state.enabled(1), Some(true));
        legacy
            .execute_batch("ALTER TABLE \"references\" ADD COLUMN header_flags INTEGER;")
            .unwrap();
        assert!(EnableCatalog::read(&legacy).is_err());
        for rows in [
            "(1,0,2,NULL)",
            "(1,-1,NULL,0)",
            "(1,0,-1,0)",
            "(1,0,NULL,-1)",
        ] {
            assert!(
                EnableCatalog::read(&database(rows)).is_err(),
                "accepted {rows}"
            );
        }
    }

    #[test]
    fn catalog_initial_results_are_immutable_when_runtime_state_changes() {
        let catalog = catalog("(1,2048,NULL,0),(2,0,1,1),(3,0,99,0)");
        assert_eq!(catalog.enabled(1), Some(false));
        assert_eq!(catalog.enabled(2), Some(true));
        assert_eq!(catalog.enabled(3), None);
        assert_eq!(catalog.enabled(99), None);
        assert_eq!(catalog.enabled(PLAYER_REFERENCE), Some(true));
        let mut app = app(Arc::clone(&catalog));
        set(&mut app, 1, true);
        assert_eq!(enabled(&app, 1), Some(true));
        assert_eq!(catalog.enabled(1), Some(false));
        assert_eq!(catalog.enabled(2), Some(true));
    }

    #[test]
    fn catalog_reads_all_references_once_and_runtime_does_not_query_database() {
        let connection = database("(1,2048,NULL,0),(2,0,1,0),(3,0,2,1)");
        let catalog = Arc::new(EnableCatalog::read(&connection).unwrap());
        connection
            .execute_batch("DROP TABLE \"references\";")
            .unwrap();
        let mut app = app(catalog);
        assert_eq!(enabled(&app, 2), Some(false));
        assert_eq!(enabled(&app, 3), Some(true));
        set(&mut app, 1, true);
        assert_eq!(enabled(&app, 2), Some(true));
        assert_eq!(enabled(&app, 3), Some(false));
    }
}
