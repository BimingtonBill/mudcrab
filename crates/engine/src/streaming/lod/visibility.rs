//! Which stock terrain LOD patch is drawn, kept incrementally.
//!
//! The pass answers one question per `(grid, quadrant)` key: is full-detail terrain ready there
//! in the demanded world, and if not, which ready LOD tier covers it. Both answers are counts,
//! because duplicate patches exist and one of them leaving must not remove the other. The
//! counts live in [`TerrainLodIndex`] and are updated from the entities that changed, never
//! rebuilt from every patch:
//!
//! - Each tracked patch caches its own facts (coverage, kind, readiness) and the world its
//!   ancestry resolves to. The ancestry walk is the expensive part, so it runs once per patch and
//!   again only when a node on the walked path changes.
//! - `nodes` maps every entity a walk visited to the patches that depend on it, with the
//!   ownership facts (worldspace, chunk root, parent) read at walk time. A change to a node is
//!   compared against those facts; only an actual difference walks the dependent patches again.
//! - Counts are kept per key, and only keys whose counts changed apply their members. Camera,
//!   demand, config and owned-cell changes reselect from the counts without any walk.
//!
//! Changed inputs arrive from the terrain journal (`identity_changes`), plus change-filtered
//! queries restricted to the three components that are mutated in place. Without the journal, or
//! after a gap in it, the index is rebuilt from every patch: that is the recovery path, and it is
//! what the old pass did on every refresh. In test and debug builds an audit compares the
//! incremental index with such a rebuild after every pass, as the cluster template rows are
//! checked against a full pass.
//!
//! Two situations never use the journal:
//!
//! - It is absent altogether (a run that did not install it): the pass keeps its old triggers
//!   and rebuilds only when one fires, so a `Visibility` changed from outside is left alone on a
//!   frame without a trigger, as it always was.
//! - It is present but a component is registered as a default query disabler that its observers
//!   do not follow. Such an entity can leave or enter the pass's queries without any message, so
//!   the pass rebuilds on every invocation, as the identity index does.
//!
//! The old pass also rewrote the `Visibility` of every LOD patch on every refresh, which repaired
//! a value changed from outside. The index keeps that without writing every patch: a
//! change-filtered query reports each `Visibility` that differs from what the last application
//! wrote, the entities are held in `overrides`, and any frame the old pass would have refreshed
//! on (selection, cluster, flag, removal) applies exactly those keys again.

use super::super::{
    MergedTerrainCoverage, RenderOrigin, RingTerrain, StreamingCommitBudget, StreamingMetrics,
    TerrainCoverage, TerrainPatch, TerrainSurfaceReady, TerrainWorldspace,
    identity_changes::{TerrainNodeChanged, TerrainTracking},
    terrain_audit::{AuditIndex, TerrainAudit},
};
use super::{
    LodChunkRoot, LodDemandViews, LodStreaming, chebyshev_grid_distance,
    record_terrain_batch_progress, select_ready_terrain_lod_tier, terrain_batch_budget_available,
    terrain_batching,
};
use crate::{
    config::{EngineConfig, TerrainLodDistances},
    portal::PortalState,
    profiling::ProfilingState,
};
use bevy::{
    ecs::{
        entity_disabling::DefaultQueryFilters,
        message::{MessageCursor, Messages},
        system::SystemParam,
    },
    prelude::*,
};
use shared::lod::{ChunkKey, LodTier};
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

type PatchKey = (IVec2, u8);

/// What one entity contributes to the owning-world walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnerFacts {
    world: Option<u32>,
    chunk: Option<ChunkKey>,
    parent: Option<Entity>,
}

impl OwnerFacts {
    /// No ownership facts. This alone does not make a node unobserved: a patch with coverage
    /// or readiness still announces its removal through the journal.
    const UNOBSERVED: Self = Self {
        world: None,
        chunk: None,
        parent: None,
    };
}

type LodTerrainOwner = (
    Option<&'static TerrainWorldspace>,
    Option<&'static LodChunkRoot>,
    Option<&'static ChildOf>,
);

/// The full-detail patches that count: ready ones. The filters are exactly the pass's original
/// ones, because a component registered as a default disabler is only matched by a query that
/// names it; an optional read or a `Has` would change which patches the pass sees.
type NearPatches = (Entity, &'static TerrainCoverage);
type NearPatchFilter = (With<TerrainPatch>, With<TerrainSurfaceReady>);

/// The LOD patches: the pass writes `Visibility` on each, ready or not.
type LodPatches = (
    Entity,
    &'static TerrainCoverage,
    Option<&'static TerrainSurfaceReady>,
    &'static mut Visibility,
);
type LodPatchFilter = Without<TerrainPatch>;

/// The same patches read without their `Visibility`, so reading them does not conflict with
/// writing it. `With<Visibility>` names the component exactly as the write does, which keeps the
/// default-disabler domain of the original query.
type LodPatchFacts = (
    Entity,
    &'static TerrainCoverage,
    Option<&'static TerrainSurfaceReady>,
);
type LodPatchFactsFilter = (With<Visibility>, Without<TerrainPatch>);

/// The LOD patches whose `Visibility` changed since the pass last ran, in the same domain.
type LodVisibilityChanged = (Entity, &'static Visibility);
type LodVisibilityChangedFilter = (
    Changed<Visibility>,
    With<TerrainCoverage>,
    Without<TerrainPatch>,
);

/// The ownership query keeps its original domain. This separate membership check only decides
/// whether an otherwise empty ancestor needs liveness polling; it never discovers patches.
type JournalObserved = Or<(
    With<Mesh3d>,
    With<Children>,
    With<TerrainCoverage>,
    With<TerrainSurfaceReady>,
    With<TerrainPatch>,
    With<Visibility>,
)>;

type RingPatches = (
    &'static TerrainWorldspace,
    Option<&'static TerrainCoverage>,
    Option<&'static MergedTerrainCoverage>,
    Has<TerrainSurfaceReady>,
    &'static mut Visibility,
);
type RingPatchFilter = (With<RingTerrain>, With<TerrainPatch>);
type ReplacementVisibilityChanged = (Changed<Visibility>, Without<RingTerrain>);

type LodVisibilityQueries<'w, 's> = (
    Query<'w, 's, LodPatches, LodPatchFilter>,
    Query<'w, 's, LodVisibilityChanged, LodVisibilityChangedFilter>,
    Query<'w, 's, &'static Visibility, Without<RingTerrain>>,
    Query<'w, 's, Entity, ReplacementVisibilityChanged>,
);

type DrawnMergedTerrain = (
    With<TerrainPatch>,
    With<TerrainSurfaceReady>,
    Without<crate::swap_bridge::SwapWaiting>,
);

/// Everything the pass reads from the world. Ordinary stock tier selection needs the owning
/// world, not primitive draw certification: coverage may live on a glTF node with primitive
/// children.
#[derive(SystemParam)]
pub(in crate::streaming) struct TerrainLodInputs<'w, 's> {
    owners: Query<'w, 's, LodTerrainOwner>,
    readiness_changed: Query<'w, 's, Entity, Changed<TerrainSurfaceReady>>,
    merged_changed: Query<'w, 's, Entity, Changed<MergedTerrainCoverage>>,
    removed_merged: RemovedComponents<'w, 's, MergedTerrainCoverage>,
    drawn_swaps: RemovedComponents<'w, 's, crate::swap_bridge::SwapWaiting>,
    merged: Query<
        'w,
        's,
        (
            &'static MergedTerrainCoverage,
            &'static TerrainWorldspace,
            Has<RingTerrain>,
        ),
        DrawnMergedTerrain,
    >,
    ring: Query<'w, 's, RingPatches, RingPatchFilter>,
    draw_receipts: Query<'w, 's, (), (With<Mesh3d>, With<crate::swap_bridge::SwapDrawn>)>,
    removed_drawn: RemovedComponents<'w, 's, crate::swap_bridge::SwapDrawn>,
    drawn_changed: Query<'w, 's, Entity, Changed<crate::swap_bridge::SwapDrawn>>,
    journal_observed: Query<'w, 's, (), JournalObserved>,
    near: Query<'w, 's, NearPatches, NearPatchFilter>,
    lod: Query<'w, 's, LodPatchFacts, LodPatchFactsFilter>,
    // Writing `Visibility` and asking which ones changed cannot be two plain queries.
    lod_visibility: ParamSet<'w, 's, LodVisibilityQueries<'w, 's>>,
    // The components this pass reads that are also changed in place, which no insert observer
    // sees. Each query only walks the archetypes that carry its component.
    // Keep each original query's opt-ins for components registered as default disablers.
    // A bare Changed<TerrainCoverage> would omit ready near patches or visible LOD patches.
    near_coverage_changed: Query<'w, 's, Entity, (Changed<TerrainCoverage>, NearPatchFilter)>,
    lod_coverage_changed: Query<'w, 's, Entity, (Changed<TerrainCoverage>, LodPatchFactsFilter)>,
    worldspace_changed: Query<'w, 's, Entity, Changed<TerrainWorldspace>>,
    root_changed: Query<'w, 's, Entity, Changed<LodChunkRoot>>,
    journal: Option<Res<'w, Messages<TerrainNodeChanged>>>,
    tracking: Option<Res<'w, TerrainTracking>>,
    filters: Option<Res<'w, DefaultQueryFilters>>,
    cursor: Local<'s, MessageCursor<TerrainNodeChanged>>,
    // Without the journal these are the only removal signal.
    removed_coverage: RemovedComponents<'w, 's, TerrainCoverage>,
    removed_readiness: RemovedComponents<'w, 's, TerrainSurfaceReady>,
}

impl TerrainLodInputs<'_, '_> {
    fn owner(&self, entity: Entity) -> Option<OwnerFacts> {
        let (world, root, parent) = self.owners.get(entity).ok()?;
        Some(OwnerFacts {
            world: world.map(|world| world.0),
            chunk: root.map(|root| root.terrain_key()),
            parent: parent.map(|parent| parent.parent()),
        })
    }

    /// An empty ancestry node needs polling only when none of the journal's observed
    /// components announces its removal. Ownership components are already in `owner`.
    fn needs_liveness_poll(&self, entity: Entity, owner: Option<OwnerFacts>) -> bool {
        owner == Some(OwnerFacts::UNOBSERVED) && !self.journal_observed.contains(entity)
    }

    /// Whether the pass reads this entity as a patch.
    fn is_patch(&self, entity: Entity) -> bool {
        self.near.contains(entity) || self.lod.contains(entity)
    }

    /// Every entity the pass reads as a patch.
    fn patches(&self) -> Vec<Entity> {
        self.near
            .iter()
            .map(|(entity, _)| entity)
            .chain(self.lod.iter().map(|(entity, ..)| entity))
            .collect()
    }

    /// The world a patch belongs to, with every entity the walk read recorded in `steps`.
    ///
    /// A full-detail patch (`tier` `None`) must resolve to exactly one worldspace and no chunk
    /// root; a LOD patch of `tier` to exactly one chunk root of that tier and no worldspace.
    /// Anything else - a conflicting second worldspace, two chunk roots, a cycle, a missing or
    /// disabled ancestor - gives no world. Every refusal depends only on the entities in `steps`,
    /// including the one that stopped the walk.
    fn resolve(
        &self,
        patch: Entity,
        tier: Option<LodTier>,
        steps: &mut Vec<(Entity, Option<OwnerFacts>)>,
    ) -> Option<u32> {
        let mut at = patch;
        let mut near_world = None;
        let mut chunk = None;
        // Cycle detection stays linear in the depth of the ancestry, as the original walk's was.
        let mut seen = HashSet::new();
        loop {
            if !seen.insert(at) {
                return None;
            }
            let owner = self.owner(at);
            steps.push((at, owner));
            let owner = owner?;
            if let Some(world) = owner.world
                && near_world.replace(world).is_some_and(|old| old != world)
            {
                return None;
            }
            if let Some(root) = owner.chunk
                && chunk.replace(root).is_some()
            {
                return None;
            }
            match owner.parent {
                Some(parent) => at = parent,
                None => break,
            }
        }
        match (tier, near_world, chunk) {
            (None, Some(world), None) => Some(world),
            (Some(tier), None, Some(chunk)) if chunk.tier == tier => Some(chunk.worldspace_id),
            _ => None,
        }
    }

    /// Whether the terrain journal covers every input of the pass, so changes can be applied
    /// from it instead of rebuilt.
    fn journaled(&self) -> bool {
        self.journal.is_some()
            && self
                .tracking
                .as_deref()
                .zip(self.filters.as_deref())
                .is_some_and(|(tracking, filters)| tracking.covered(filters))
    }

    /// Whether the journal is installed but its observers do not follow a component that is
    /// registered as a default query disabler. An entity can then enter or leave the pass's
    /// queries without any message. That differs from a journal that was never installed, which
    /// leaves the old triggers in charge.
    fn unsupported_domain(&self) -> bool {
        self.tracking
            .as_deref()
            .zip(self.filters.as_deref())
            .is_some_and(|(tracking, filters)| !tracking.covered(filters))
    }

    /// Appends every `Visibility` of a LOD patch that changed since the last call.
    fn changed_visibility(&mut self, out: &mut Vec<(Entity, Visibility)>) {
        out.clear();
        out.extend(
            self.lod_visibility
                .p1()
                .iter()
                .map(|(entity, visibility, ..)| (entity, *visibility)),
        );
    }

    /// Appends every entity changed since the last call. `false` if journal messages were lost
    /// before they were read: the caller must then rebuild.
    fn collect_changes(&mut self, out: &mut Vec<Entity>) -> bool {
        let Some(journal) = self.journal.as_deref() else {
            return false;
        };
        if self.cursor.missed_messages(journal) > 0 {
            self.cursor.clear(journal);
            return false;
        }
        out.extend(self.cursor.read(journal).map(|message| message.0));
        out.extend(self.near_coverage_changed.iter());
        out.extend(self.lod_coverage_changed.iter());
        out.extend(self.worldspace_changed.iter());
        out.extend(self.root_changed.iter());
        true
    }

    /// Marks every journal message so far as seen. A rebuild reads the world directly.
    fn skip_journal(&mut self) {
        if let Some(journal) = self.journal.as_deref() {
            self.cursor.clear(journal);
        }
    }

    /// Whether any patch lost its coverage or readiness. Without the journal that is the only
    /// signal of a removal.
    fn removals(&mut self, replaced: &HashSet<Entity>) -> bool {
        // Drain both streams before expiring exact generational transfer identities.
        let coverage = self
            .removed_coverage
            .read()
            .filter(|entity| !replaced.contains(entity))
            .count()
            != 0;
        let readiness = self
            .removed_readiness
            .read()
            .filter(|entity| !replaced.contains(entity))
            .count()
            != 0;
        coverage || readiness
    }
}

/// The inputs of tier selection that are not patches. A change of the world recounts every patch
/// from its cached world and applies every key; a config change applies every key; a camera or
/// render-origin move reselects every key and applies those whose tier changed.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Selection {
    world: Option<u32>,
    origin: IVec2,
    camera_grid: IVec2,
    stream_radius: i32,
    distances: TerrainLodDistances,
    own_statics: bool,
    explicit_ring: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct PatchEntry {
    coverage: TerrainCoverage,
    /// A full-detail patch rather than a LOD patch.
    near: bool,
    ready: bool,
    ring: bool,
    /// The owning world, resolved through `path`; `None` when the ancestry is refused or was
    /// not walked.
    world: Option<u32>,
    /// Every entity the ancestry walk read, the patch first. Empty when no walk was needed.
    path: Vec<Entity>,
    /// Whether this patch is counted in its key: ready, in the demanded world and, for a LOD
    /// patch, of a tier.
    counted: bool,
    /// Whether the last application selected this LOD patch.
    visible: bool,
}

impl PatchEntry {
    fn key(&self) -> PatchKey {
        (self.coverage.grid, self.coverage.quadrant)
    }

    /// The `Visibility` the last application wrote on this LOD patch.
    fn written_visibility(&self) -> Visibility {
        if self.visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    }
}

/// An entity some patch's ancestry walk read, with what the walk saw of it.
#[derive(Debug)]
struct PathNode {
    owner: Option<OwnerFacts>,
    patches: HashSet<Entity>,
}

#[derive(Debug, Default)]
struct KeyState {
    /// Counted full-detail patches: the key is covered by full detail when positive.
    near_ready: u32,
    ring_ready: u32,
    /// Counted LOD patches per tier.
    tier_ready: [u32; LodTier::ALL.len()],
    /// Every LOD patch of the key that carries `Visibility`, counted or not.
    members: HashSet<Entity>,
    /// The tier selected at the last application.
    selected: Option<LodTier>,
}

impl KeyState {
    fn is_empty(&self) -> bool {
        self.near_ready == 0 && self.ring_ready == 0 && self.members.is_empty()
    }
}

fn near_covers(state: &KeyState, key: PatchKey, selection: &Selection) -> bool {
    state.near_ready > 0
        || (state.ring_ready > 0
            && (selection.explicit_ring
                || chebyshev_grid_distance(key.0, selection.camera_grid)
                    <= selection.stream_radius))
}

fn tier_slot(tier: LodTier) -> usize {
    LodTier::ALL
        .iter()
        .position(|candidate| *candidate == tier)
        .expect("LodTier::ALL lists every tier")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Unsynced,
    /// Changes come from the terrain journal.
    Journal,
    /// No usable journal: the index is rebuilt whenever a refresh trigger fires.
    Legacy,
}

/// Cumulative work counters, published as `lod/visibility/*` gauges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerrainLodStats {
    rebuilds: u64,
    ancestry_walks: u64,
    patches_evaluated: u64,
    keys_applied: u64,
    desyncs: u64,
}

/// A failed decrement keeps the count at zero and reports the broken index invariant.
fn checked_count<T: Default>(
    next: Option<T>,
    stats: &mut TerrainLodStats,
    counter: &str,
    key: PatchKey,
    entity: Entity,
) -> T {
    next.unwrap_or_else(|| {
        count_desync(stats, counter, key, entity);
        T::default()
    })
}

fn count_desync(stats: &mut TerrainLodStats, reason: &str, key: PatchKey, entity: Entity) {
    stats.desyncs += 1;
    error!(?key, %entity, reason, "terrain LOD visibility index desynchronized");
}

/// What decides cluster terrain's share of the pass. Presence of each optional resource is part
/// of it: removing one changes the owned cells and the pending count while the world, the
/// revision and the flag stay as they were.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ClusterStamp {
    world: Option<u32>,
    readiness_revision: u64,
    own_statics: bool,
    terrain: bool,
    readiness: bool,
}

#[derive(Default)]
pub(in crate::streaming) struct TerrainLodIndex {
    mode: Mode,
    selection: Option<Selection>,
    /// The demanded world counts are taken for.
    world: Option<u32>,
    cluster_stamp: Option<ClusterStamp>,
    /// Retained batches and confirmed merged cells feed the same tier decision as source patches.
    /// Current cluster ownership is applied per render view after extraction.
    retained: HashMap<PatchKey, u8>,
    merged: HashSet<PatchKey>,
    batch_selection: HashMap<PatchKey, Option<LodTier>>,
    batched_visible: usize,
    retained_ready: usize,
    patches: HashMap<Entity, PatchEntry>,
    nodes: HashMap<Entity, PathNode>,
    /// Empty ancestry nodes carrying no journal-observed component. Only these need polling
    /// because no message announces their despawn, including hook-skipped relationships.
    unobserved: HashSet<Entity>,
    keys: HashMap<PatchKey, KeyState>,
    /// Keys whose counts changed: their members are applied again.
    dirty: HashSet<PatchKey>,
    /// LOD patches whose `Visibility` was changed from outside and not yet written again. They
    /// wait for the next frame the old pass would have refreshed on, which tracks compatibility
    /// with that pass and is not a time slice: the indices are exact on every frame.
    overrides: HashSet<Entity>,
    apply_all: bool,
    reselect_all: bool,
    ready_patches: usize,
    visible_patches: usize,
    touched: Vec<Entity>,
    /// Scratch for the `Visibility` values reported changed this frame.
    changed: Vec<(Entity, Visibility)>,
    stats: TerrainLodStats,
    published: TerrainLodStats,
    /// Consecutive missed journal reads form one warning episode.
    journal_lost: bool,
}

impl TerrainLodIndex {
    /// Takes the selection inputs of this frame: a new world recounts every patch from its cached
    /// world, any other change reselects every key. Returns whether anything differs from the
    /// last frame, which is a refresh trigger of the old pass.
    fn sync_selection(&mut self, selection: Selection) -> bool {
        let Some(previous) = self.selection.replace(selection) else {
            self.world = selection.world;
            self.apply_all = true;
            return true;
        };
        if previous.world != selection.world {
            self.world = selection.world;
            self.recount();
            self.apply_all = true;
        } else if (
            previous.stream_radius,
            previous.distances,
            previous.own_statics,
            previous.explicit_ring,
        ) != (
            selection.stream_radius,
            selection.distances,
            selection.own_statics,
            selection.explicit_ring,
        ) {
            // A config change writes every patch again, so a `Visibility` changed from outside is
            // restored by it as it always was.
            self.apply_all = true;
        } else if previous != selection {
            self.reselect_all = true;
        }
        previous != selection
    }

    /// Records which LOD patches carry a `Visibility` other than the one the index wrote, from
    /// the entities reported changed. A value equal to the written one is the pass's own write
    /// (or an outside write that restored it) and is not an override.
    fn note_overrides(&mut self) {
        for &(entity, actual) in &self.changed {
            match self.patches.get(&entity) {
                Some(entry) if !entry.near && actual != entry.written_visibility() => {
                    self.overrides.insert(entity);
                }
                _ => {
                    self.overrides.remove(&entity);
                }
            }
        }
    }

    /// Queues the keys of every overridden patch, the repair the old pass did by writing every
    /// patch on a refresh. Only these keys are applied; the rest of the world is untouched.
    fn restore_overrides(&mut self) {
        let Self {
            overrides,
            patches,
            dirty,
            ..
        } = self;
        for entity in overrides.drain() {
            if let Some(entry) = patches.get(&entity).filter(|entry| !entry.near) {
                dirty.insert(entry.key());
            }
        }
    }

    fn has_work(&self) -> bool {
        self.apply_all || self.reselect_all || !self.dirty.is_empty()
    }

    /// Forgets everything and reads every terrain patch again.
    fn rebuild(&mut self, inputs: &TerrainLodInputs) {
        self.stats.rebuilds += 1;
        self.patches.clear();
        self.nodes.clear();
        self.unobserved.clear();
        self.keys.clear();
        self.dirty.clear();
        self.overrides.clear();
        self.touched.clear();
        self.ready_patches = 0;
        self.visible_patches = 0;
        self.apply_all = true;
        for entity in inputs.patches() {
            self.evaluate(entity, true, inputs);
        }
    }

    /// Applies the entities collected in `touched`.
    ///
    /// A touched node whose ownership facts differ from what the walks saw sends the patches
    /// below it through the walk again. A touched patch, or an entity that is one now, is read
    /// again without a walk unless its own node changed.
    fn absorb(&mut self, inputs: &TerrainLodInputs) {
        let mut touched = std::mem::take(&mut self.touched);
        touched.extend(self.unobserved.iter().copied());
        let mut rewalk = HashSet::new();
        let mut reread = HashSet::new();
        for &entity in &touched {
            if let Some(node) = self.nodes.get_mut(&entity) {
                let live = inputs.owner(entity);
                if inputs.needs_liveness_poll(entity, live) {
                    self.unobserved.insert(entity);
                } else {
                    self.unobserved.remove(&entity);
                }
                if node.owner != live {
                    node.owner = live;
                    rewalk.extend(node.patches.iter().copied());
                }
            }
            if self.patches.contains_key(&entity) || inputs.is_patch(entity) {
                reread.insert(entity);
            }
        }
        for &patch in &rewalk {
            self.evaluate(patch, true, inputs);
        }
        for &patch in reread.difference(&rewalk) {
            self.evaluate(patch, false, inputs);
        }
        self.touched = touched;
        self.touched.clear();
    }

    /// Reads one entity as a terrain patch, replacing what the index knew of it.
    fn evaluate(&mut self, entity: Entity, rewalk: bool, inputs: &TerrainLodInputs) {
        self.stats.patches_evaluated += 1;
        let old = self.patches.remove(&entity);
        if let Some(old) = &old {
            self.uncount(entity, old);
        }
        // The same two queries the pass always read: a ready full-detail patch, or a LOD patch
        // that carries the `Visibility` the pass writes.
        let live = match inputs.near.get(entity) {
            Ok((_, coverage)) => Some((*coverage, true, true)),
            Err(_) => inputs
                .lod
                .get(entity)
                .ok()
                .map(|(_, coverage, ready)| (*coverage, false, ready.is_some())),
        };
        let Some((coverage, near, ready)) = live else {
            self.overrides.remove(&entity);
            if let Some(old) = old {
                self.unlink(entity, &old.path);
            }
            return;
        };
        if near {
            // A full-detail patch is not written by the pass.
            self.overrides.remove(&entity);
        }
        let tier = if near { None } else { coverage.tier };
        let (world, path) = match old {
            Some(old) if !rewalk && old.near == near && old.coverage.tier == coverage.tier => {
                (old.world, old.path)
            }
            old => {
                if let Some(old) = old {
                    self.unlink(entity, &old.path);
                }
                if near || tier.is_some() {
                    self.walk(entity, tier, inputs)
                } else {
                    (None, Vec::new())
                }
            }
        };
        let mut entry = PatchEntry {
            coverage,
            near,
            ready,
            ring: inputs.ring.contains(entity),
            world,
            path,
            counted: false,
            visible: false,
        };
        self.count(entity, &mut entry);
        self.patches.insert(entity, entry);
    }

    fn walk(
        &mut self,
        patch: Entity,
        tier: Option<LodTier>,
        inputs: &TerrainLodInputs,
    ) -> (Option<u32>, Vec<Entity>) {
        self.stats.ancestry_walks += 1;
        let mut steps = Vec::new();
        let world = inputs.resolve(patch, tier, &mut steps);
        for &(id, owner) in &steps {
            let node = self.nodes.entry(id).or_insert_with(|| PathNode {
                owner,
                patches: HashSet::new(),
            });
            // Every node a walk read has been refreshed by its own journal entry first.
            debug_assert_eq!(node.owner, owner, "a terrain ancestor changed unannounced");
            node.owner = owner;
            node.patches.insert(patch);
            if inputs.needs_liveness_poll(id, owner) {
                self.unobserved.insert(id);
            } else {
                self.unobserved.remove(&id);
            }
        }
        (world, steps.into_iter().map(|(node, _)| node).collect())
    }

    fn unlink(&mut self, patch: Entity, path: &[Entity]) {
        for node in path {
            if let Some(entry) = self.nodes.get_mut(node) {
                entry.patches.remove(&patch);
                if entry.patches.is_empty() {
                    self.nodes.remove(node);
                    self.unobserved.remove(node);
                }
            }
        }
    }

    /// Adds a patch to its key's counts and marks the key for application.
    fn count(&mut self, entity: Entity, entry: &mut PatchEntry) {
        entry.counted = entry.ready
            && entry.world.is_some()
            && entry.world == self.world
            && (entry.near || entry.coverage.tier.is_some());
        if entry.near && !entry.counted {
            return;
        }
        let key = entry.key();
        let state = self.keys.entry(key).or_default();
        if entry.near {
            if entry.ring {
                state.ring_ready += 1;
            } else {
                state.near_ready += 1;
            }
        } else {
            state.members.insert(entity);
            if let (true, Some(tier)) = (entry.counted, entry.coverage.tier) {
                state.tier_ready[tier_slot(tier)] += 1;
                self.ready_patches += 1;
            }
        }
        self.dirty.insert(key);
    }

    /// Takes a patch out of its key's counts, the inverse of [`Self::count`].
    fn uncount(&mut self, entity: Entity, entry: &PatchEntry) {
        let key = entry.key();
        if entry.visible {
            self.visible_patches = checked_count(
                self.visible_patches.checked_sub(1),
                &mut self.stats,
                "visible_patches",
                key,
                entity,
            );
        }
        let Some(state) = self.keys.get_mut(&key) else {
            // An uncounted near patch never creates a key; every other patch does.
            if !entry.near || entry.counted {
                count_desync(&mut self.stats, "missing key", key, entity);
            }
            return;
        };
        if entry.near {
            if entry.counted {
                let count = if entry.ring {
                    &mut state.ring_ready
                } else {
                    &mut state.near_ready
                };
                *count = checked_count(
                    count.checked_sub(1),
                    &mut self.stats,
                    "near_ready",
                    key,
                    entity,
                );
            }
        } else {
            if !state.members.remove(&entity) {
                count_desync(&mut self.stats, "missing key member", key, entity);
            }
            if let (true, Some(tier)) = (entry.counted, entry.coverage.tier) {
                let count = &mut state.tier_ready[tier_slot(tier)];
                *count = checked_count(
                    count.checked_sub(1),
                    &mut self.stats,
                    "tier_ready",
                    key,
                    entity,
                );
                self.ready_patches = checked_count(
                    self.ready_patches.checked_sub(1),
                    &mut self.stats,
                    "ready_patches",
                    key,
                    entity,
                );
            }
        }
        if state.is_empty() {
            self.keys.remove(&key);
        }
        self.dirty.insert(key);
    }

    /// Counts every patch again for a new demanded world, from the worlds already resolved.
    fn recount(&mut self) {
        self.keys.clear();
        self.dirty.clear();
        self.ready_patches = 0;
        let mut patches = std::mem::take(&mut self.patches);
        for (entity, entry) in &mut patches {
            self.count(*entity, entry);
        }
        self.patches = patches;
    }

    /// Selects the tier of every key that changed, or of every key when the camera, config or
    /// demand moved, and writes `Visibility` on the patches of each key whose answer changed.
    fn reselect(&mut self, lod: &mut Query<LodPatches, LodPatchFilter>, selection: &Selection) {
        let Self {
            keys,
            patches,
            dirty,
            retained,
            merged,
            overrides,
            visible_patches,
            stats,
            apply_all,
            reselect_all,
            ..
        } = self;
        let everything = *apply_all || *reselect_all;
        let mut settle = |key: PatchKey, state: &mut KeyState, forced: bool| {
            let tier = select_ready_terrain_lod_tier(
                chebyshev_grid_distance(key.0, selection.camera_grid),
                near_covers(state, key, selection) || merged.contains(&key),
                |tier| {
                    state.tier_ready[tier_slot(tier)] > 0
                        || retained
                            .get(&key)
                            .is_some_and(|bits| bits & (tier as u8) != 0)
                },
                &selection.distances,
            );
            if !forced && state.selected == tier {
                return;
            }
            state.selected = tier;
            stats.keys_applied += 1;
            for entity in &state.members {
                let Some(entry) = patches.get_mut(entity) else {
                    debug_assert!(false, "key member {entity} has no patch entry");
                    continue;
                };
                // Main-world tier selection retains stock for extraction; current-view
                // cluster replacement is decided later by terrain_handoff.
                let visible = tier.is_some() && entry.counted && entry.coverage.tier == tier;
                if visible != entry.visible {
                    entry.visible = visible;
                    if visible {
                        *visible_patches += 1;
                    } else {
                        *visible_patches = checked_count(
                            visible_patches.checked_sub(1),
                            stats,
                            "visible_patches",
                            key,
                            *entity,
                        );
                    }
                }
                if let Ok((_, _, _, mut current)) = lod.get_mut(*entity) {
                    let next = entry.written_visibility();
                    if *current != next {
                        *current = next;
                    }
                }
                overrides.remove(entity);
            }
        };
        if everything {
            for (&key, state) in keys.iter_mut() {
                settle(key, state, *apply_all || dirty.contains(&key));
            }
        } else {
            for key in dirty.iter() {
                if let Some(state) = keys.get_mut(key) {
                    settle(*key, state, true);
                }
            }
        }
        dirty.clear();
        *apply_all = false;
        *reselect_all = false;
    }

    fn publish_stats(&mut self, profiler: &mut ProfilingState) {
        if self.stats == self.published {
            return;
        }
        self.published = self.stats;
        profiler.set_gauge("lod/visibility/desyncs", self.stats.desyncs as f64);
        profiler.set_gauge("lod/visibility/rebuilds", self.stats.rebuilds as f64);
        profiler.set_gauge(
            "lod/visibility/ancestry_walks",
            self.stats.ancestry_walks as f64,
        );
        profiler.set_gauge(
            "lod/visibility/patches_evaluated",
            self.stats.patches_evaluated as f64,
        );
        profiler.set_gauge(
            "lod/visibility/keys_applied",
            self.stats.keys_applied as f64,
        );
    }

    /// Returns whether a missed read starts a new consecutive-loss warning episode.
    fn note_journal_read(&mut self, complete: bool) -> bool {
        let warn = !complete && !self.journal_lost;
        self.journal_lost = !complete;
        warn
    }

    fn audit_if_requested(&self, inputs: &mut TerrainLodInputs, audit: Option<&TerrainAudit>) {
        let due = audit.filter(|audit| audit.due(AuditIndex::Lod));
        if !cfg!(any(test, debug_assertions)) && due.is_none() {
            return;
        }
        let result = self.audit(inputs);
        #[cfg(any(test, debug_assertions))]
        assert!(result.is_ok(), "LOD visibility audit failed: {result:?}");
        if let Some(audit) = due {
            audit.record(AuditIndex::Lod, result);
        }
    }

    /// Checks the index against a fresh pass, returning a diagnostic usable in played release
    /// builds as well as the exhaustive debug assertion.
    fn audit(&self, inputs: &mut TerrainLodInputs) -> Result<(), String> {
        let mut fresh = TerrainLodIndex {
            world: self.world,
            retained: self.retained.clone(),
            merged: self.merged.clone(),
            ..default()
        };
        fresh.rebuild(inputs);
        if self.patches.len() != fresh.patches.len() {
            return Err(format!(
                "tracked patch count {} differs from full pass {}",
                self.patches.len(),
                fresh.patches.len()
            ));
        }
        for (entity, entry) in &self.patches {
            let full = fresh
                .patches
                .get(entity)
                .ok_or_else(|| format!("{entity} is tracked but a full pass drops it"))?;
            let actual = (
                entry.coverage,
                entry.near,
                entry.ready,
                entry.ring,
                entry.world,
                &entry.path,
                entry.counted,
            );
            let expected = (
                full.coverage,
                full.near,
                full.ready,
                full.ring,
                full.world,
                &full.path,
                full.counted,
            );
            if actual != expected {
                return Err(format!(
                    "patch {entity} differs from full pass: {actual:?}, expected {expected:?}"
                ));
            }
        }
        if self.keys.len() != fresh.keys.len() {
            return Err(format!(
                "key count {} differs from full pass {}",
                self.keys.len(),
                fresh.keys.len()
            ));
        }
        for (key, state) in &self.keys {
            let full = fresh
                .keys
                .get(key)
                .ok_or_else(|| format!("key {key:?} is tracked but a full pass drops it"))?;
            let actual = (
                state.near_ready,
                state.ring_ready,
                state.tier_ready,
                &state.members,
            );
            let expected = (
                full.near_ready,
                full.ring_ready,
                full.tier_ready,
                &full.members,
            );
            if actual != expected {
                return Err(format!(
                    "key {key:?} differs from full pass: {actual:?}, expected {expected:?}"
                ));
            }
            if let Some(selection) = self.selection {
                let expected_tier = select_ready_terrain_lod_tier(
                    chebyshev_grid_distance(key.0, selection.camera_grid),
                    near_covers(full, *key, &selection) || self.merged.contains(key),
                    |tier| {
                        full.tier_ready[tier_slot(tier)] > 0
                            || self
                                .retained
                                .get(key)
                                .is_some_and(|bits| bits & (tier as u8) != 0)
                    },
                    &selection.distances,
                );
                if state.selected != expected_tier {
                    return Err(format!(
                        "key {key:?} selects {:?}, expected {expected_tier:?}",
                        state.selected
                    ));
                }
                for entity in &state.members {
                    let entry = self
                        .patches
                        .get(entity)
                        .ok_or_else(|| format!("key {key:?} member {entity} has no patch entry"))?;
                    let expected_visible = expected_tier.is_some()
                        && entry.counted
                        && entry.coverage.tier == expected_tier;
                    if entry.visible != expected_visible {
                        return Err(format!(
                            "patch {entity} visibility {} differs from selected {expected_visible}",
                            entry.visible
                        ));
                    }
                }
            }
        }
        if self.nodes.len() != fresh.nodes.len() {
            return Err(format!(
                "walked node count {} differs from full pass {}",
                self.nodes.len(),
                fresh.nodes.len()
            ));
        }
        for (entity, node) in &self.nodes {
            let full = fresh
                .nodes
                .get(entity)
                .ok_or_else(|| format!("node {entity} is tracked but a full pass drops it"))?;
            if (node.owner, &node.patches) != (full.owner, &full.patches) {
                return Err(format!(
                    "node {entity} differs from full pass: {:?}, expected {:?}",
                    (node.owner, &node.patches),
                    (full.owner, &full.patches)
                ));
            }
        }
        if self.unobserved != fresh.unobserved {
            return Err(format!(
                "unobserved node set {:?} differs from full pass {:?}",
                self.unobserved, fresh.unobserved
            ));
        }
        if self.ready_patches != fresh.ready_patches {
            return Err(format!(
                "ready_patches {} differs from full pass {}",
                self.ready_patches, fresh.ready_patches
            ));
        }
        for entity in &self.overrides {
            if !self.patches.get(entity).is_some_and(|entry| !entry.near) {
                return Err(format!(
                    "{entity} is held as an override but is not a LOD patch"
                ));
            }
        }
        let visible = self.patches.values().filter(|entry| entry.visible).count();
        if self.visible_patches != visible {
            return Err(format!(
                "visible_patches {} differs from tracked visible count {visible}",
                self.visible_patches
            ));
        }
        // The cached selection is only half the output invariant. Read the original write
        // query's actual component too, without widening its default-disabler domain or marking
        // it changed. Outside edits in `overrides` intentionally wait for a refresh trigger.
        let lod = inputs.lod_visibility.p0();
        for (entity, entry) in &self.patches {
            if entry.near || self.overrides.contains(entity) {
                continue;
            }
            let (_, _, _, actual) = lod
                .get(*entity)
                .map_err(|error| format!("patch {entity} has no ECS Visibility: {error}"))?;
            let expected = entry.written_visibility();
            if *actual != expected {
                return Err(format!(
                    "patch {entity} ECS Visibility {actual:?} differs from selected {expected:?}"
                ));
            }
        }
        Ok(())
    }
}

/// Cluster terrain blocks of the demanded world whose draw batch is not ready yet.
fn pending_cluster_terrain_blocks(
    world: Option<u32>,
    terrain: Option<&crate::cluster::terrain::ClusterTerrain>,
    readiness: Option<&crate::cluster::terrain::TerrainDrawReadiness>,
) -> usize {
    terrain.map_or(0, |terrain| {
        terrain
            .blocks
            .iter()
            .filter(|block| Some(block.worldspace_id) == world)
            .filter(|block| {
                !block
                    .batch
                    .is_some_and(|batch| readiness.is_some_and(|ready| ready.contains(batch)))
            })
            .count()
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::streaming) fn update_terrain_lod_visibility(
    config: Res<EngineConfig>,
    origin: Res<RenderOrigin>,
    views: LodDemandViews,
    portal: Option<Res<PortalState>>,
    mut inputs: TerrainLodInputs,
    mut streaming: ParamSet<(Res<LodStreaming>, ResMut<LodStreaming>)>,
    mut metrics: ResMut<StreamingMetrics>,
    mut profiler: ResMut<ProfilingState>,
    cluster_terrain: Option<Res<crate::cluster::terrain::ClusterTerrain>>,
    terrain_readiness: Option<Res<crate::cluster::terrain::TerrainDrawReadiness>>,
    mut index: Local<TerrainLodIndex>,
    audit: Option<Res<TerrainAudit>>,
    mut batches: terrain_batching::LodBatchVisibility,
    mut budget: Option<ResMut<StreamingCommitBudget>>,
) {
    let started = Instant::now();
    let demand = views.current(&config, origin.0, portal.as_deref());
    let selection = Selection {
        world: demand.map(|demand| demand.world),
        origin: origin.0,
        camera_grid: demand.map_or(IVec2::ZERO, |demand| demand.center),
        stream_radius: config.stream_radius,
        distances: config.terrain_lod,
        own_statics: config.cluster_own_statics,
        explicit_ring: config.terrain_radius_given,
    };
    let cluster_stamp = ClusterStamp {
        world: selection.world,
        readiness_revision: terrain_readiness
            .as_ref()
            .map_or(0, |ready| ready.revision()),
        own_statics: selection.own_statics,
        terrain: cluster_terrain.is_some(),
        readiness: terrain_readiness.is_some(),
    };
    // The readiness set is shared state behind an `Arc`, and its revision counts changes to that
    // set only. A replaced resource can start over at the same revision with a different set, so
    // the resource being added or written is a change as well.
    let cluster_changed = index.cluster_stamp != Some(cluster_stamp)
        || cluster_terrain
            .as_ref()
            .is_some_and(|terrain| terrain.is_changed())
        || terrain_readiness
            .as_ref()
            .is_some_and(|readiness| readiness.is_changed());
    let replaced = std::mem::take(&mut streaming.p1().replaced_source_quadrants);
    let removals = inputs.removals(&replaced);
    let batches_removed = batches.removed();
    let upload_completed = batches.upload_completed();
    let submission_dirty = streaming.p0().submission_dirty;
    streaming.p1().submission_dirty = false;
    let merged_removed = inputs.removed_merged.read().count() != 0;
    let surfaces_drawn = inputs.drawn_swaps.read().count() != 0;
    let drawn_withdrawn = inputs.removed_drawn.read().count() != 0;
    // Structural observers cannot report an in-place Visibility write. Consume
    // this stream in journal mode too, scoped to cached terrain dependencies.
    let changed_visibility: Vec<_> = inputs.lod_visibility.p3().iter().collect();
    let dependency_visibility_changed = changed_visibility.into_iter().any(|entity| {
        index.nodes.contains_key(&entity)
            || inputs.draw_receipts.contains(entity)
            || inputs.owner(entity).is_some_and(|owner| {
                owner
                    .parent
                    .is_some_and(|parent| index.patches.contains_key(&parent))
            })
    });
    let legacy_draw_change = !inputs.journaled() && !inputs.drawn_changed.is_empty();
    let external_refresh = dependency_visibility_changed
        || legacy_draw_change
        || batches_removed
        || submission_dirty
        || !replaced.is_empty()
        || removals
        || selection.world != index.world
        || !inputs.root_changed.is_empty()
        || !inputs.readiness_changed.is_empty()
        || !inputs.merged_changed.is_empty()
        || merged_removed
        || surfaces_drawn
        || drawn_withdrawn
        || index.selection != Some(selection);
    let external_changed = if external_refresh {
        let mut retained = HashMap::new();
        let mut retained_ready = 0usize;
        for coverage in batches.coverage_for_world(selection.world) {
            if let Some(tier) = coverage.tier {
                *retained
                    .entry((coverage.grid, coverage.quadrant))
                    .or_insert(0u8) |= tier as u8;
                retained_ready += 1;
            }
        }
        let merged: HashSet<_> = inputs
            .merged
            .iter()
            .filter(|(coverage, world, ring)| {
                Some(world.0) == selection.world
                    && (!ring
                        || selection.explicit_ring
                        || chebyshev_grid_distance(coverage.grid, selection.camera_grid)
                            <= selection.stream_radius)
            })
            .flat_map(|(coverage, _, _)| (0..4).map(move |quadrant| (coverage.grid, quadrant)))
            .collect();
        let external_changed = index.retained != retained || index.merged != merged;
        index.retained_ready = retained_ready;
        if external_changed {
            index.retained = retained;
            index.merged = merged;
            index.reselect_all = true;
        }
        external_changed
    } else {
        false
    };
    // The flag predates the journal: each place that sets it changes an input the journal
    // reports (a patch added or removed, a surface ready), so it only matters without one.
    let flagged = streaming.p0().visibility_dirty;
    if flagged {
        streaming.p1().visibility_dirty = false;
    }

    let selection_moved = index.sync_selection(selection);
    let journaled = inputs.journaled();
    let unsupported = !journaled && inputs.unsupported_domain();
    let rebuild = if journaled {
        index.mode != Mode::Journal || {
            let mut touched = std::mem::take(&mut index.touched);
            let complete = inputs.collect_changes(&mut touched);
            index.touched = touched;
            if index.note_journal_read(complete) {
                warn!("terrain journal messages were lost; rebuilding the LOD visibility index");
            }
            !complete
        }
    } else if unsupported {
        // The journal is installed but blind to a default disabler registered after it, so any
        // entity may have entered or left the pass's queries unannounced: nothing short of a
        // rebuild is exact. Unlike an absent journal this is not a refresh trigger of the old
        // pass, it is a lost guarantee, so it holds on every invocation.
        warn_once!(
            "terrain journal does not follow every default query disabler; LOD visibility rebuilds on every pass"
        );
        true
    } else {
        // Without a journal nothing says which patch changed, so a refresh trigger rebuilds. A
        // changed selection is a trigger too: `sync_selection` has just queued its work.
        // A newly ready patch is a trigger even when retained/merged coverage did not change.
        index.mode != Mode::Legacy
            || !inputs.root_changed.is_empty()
            || !inputs.worldspace_changed.is_empty()
            || !inputs.near_coverage_changed.is_empty()
            || !inputs.lod_coverage_changed.is_empty()
            || cluster_changed
            || flagged
            || removals
            || inputs
                .readiness_changed
                .iter()
                .any(|entity| inputs.is_patch(entity))
            || batches_removed
            || external_changed
            || index.has_work()
    };
    if rebuild {
        {
            perf_span!("lod::visibility::rebuild");
            index.rebuild(&inputs);
        }
        if journaled {
            inputs.skip_journal();
        }
    } else if journaled {
        perf_span!("lod::visibility::absorb");
        index.absorb(&inputs);
    }
    index.mode = if journaled {
        Mode::Journal
    } else {
        Mode::Legacy
    };

    // A pass that rewrites every patch needs no report of what was changed from outside. Any
    // other frame notes the differences, and the frames the old pass refreshed on apply them.
    if !index.apply_all {
        let mut changed = std::mem::take(&mut index.changed);
        inputs.changed_visibility(&mut changed);
        index.changed = changed;
        index.note_overrides();
        if selection_moved
            || cluster_changed
            || flagged
            || removals
            || external_changed
            || batches_removed
        {
            index.restore_overrides();
        }
    }

    if rebuild || cluster_changed {
        metrics.pending_cluster_terrain_blocks = pending_cluster_terrain_blocks(
            selection.world,
            cluster_terrain.as_deref(),
            terrain_readiness.as_deref(),
        );
        index.cluster_stamp = Some(cluster_stamp);
    }

    let refresh = index.has_work();
    if refresh {
        {
            perf_span!("lod::visibility::apply");
            index.reselect(&mut inputs.lod_visibility.p0(), &selection);
        }
    }
    // Build the semantic mask from this one index. Batch-only keys retain their tier inputs;
    // current-view cluster ownership never removes either representation from extraction.
    if refresh
        || submission_dirty
        || external_changed
        || batches_removed
        || selection_moved
        || cluster_changed
        || removals
    {
        let mut selected: HashMap<_, _> = index
            .keys
            .iter()
            .map(|(&key, state)| (key, state.selected))
            .collect();
        for (&key, &available) in &index.retained {
            selected.entry(key).or_insert_with(|| {
                select_ready_terrain_lod_tier(
                    chebyshev_grid_distance(key.0, selection.camera_grid),
                    index.merged.contains(&key),
                    |tier| available & (tier as u8) != 0,
                    &selection.distances,
                )
            });
        }
        let semantic_changed = index.batch_selection != selected;
        if semantic_changed || selection_moved || cluster_changed || removals || batches_removed {
            let mut state = streaming.p1();
            state.visibility_revision = state.visibility_revision.wrapping_add(1);
        }
        index.batch_selection = selected;
    }
    let (batched_visible, _, changed_batches) =
        if refresh || submission_dirty || external_changed || batches_removed {
            batches.update_for_world(&index.batch_selection, selection.world)
        } else {
            if upload_completed {
                batches.finish_uploads();
            }
            (index.batched_visible, 0, 0)
        };
    index.batched_visible = batched_visible;
    profiler.increment("lod/terrain_batches_updated", changed_batches as u64);
    if refresh {
        profiler.increment("lod/selection_refreshes", 1);
    }
    let mut activated = false;
    if let Some(budget) = budget.as_deref_mut() {
        let mut state = streaming.p1();
        if !state.terrain_batch_commit_used
            && terrain_batch_budget_available(budget, config.max_commit_micros_per_frame)
        {
            if batches.finalize_one(&mut state.replaced_source_quadrants) {
                state.terrain_batch_commit_used = true;
                state.terrain_batch_activated_chunks += 1;
                budget.remaining -= 1;
                budget.commits = budget.commits.saturating_add(1);
                profiler.increment("lod/terrain_batch_chunks_activated", 1);
                activated = true;
            }
            let pending_cpu = state.terrain_batch_pending_cpu_chunks;
            record_terrain_batch_progress(
                &mut state,
                &mut profiler,
                pending_cpu,
                batches.pending_count(),
            );
        }
    }
    if refresh || submission_dirty || upload_completed || activated {
        let (retained_quadrants, generated, batched, visible) = batches.counts();
        profiler.set_gauge("lod/batched_terrain_quadrants", retained_quadrants as f64);
        profiler.set_gauge("lod/resident_terrain_batches", generated as f64);
        profiler.set_gauge("lod/visible_terrain_batches", visible as f64);
        profiler.set_gauge(
            "lod/pending_terrain_selection_uploads",
            batches.pending_selection_uploads() as f64,
        );
        profiler.set_gauge(
            "lod/batched_selected_triangles",
            (batched * shared::lod::TERRAIN_QUADRANT_INDEX_COUNT / 3) as f64,
        );
    }
    // Resident ring surfaces stay revealed for extraction. terrain_handoff makes
    // the complement independently for each render view against current assets
    // and the current cluster generation/plan, never historical readiness.
    let mut ring_ready = 0usize;
    for (world, _, _, ready, mut visibility) in &mut inputs.ring {
        if ready {
            if *visibility != Visibility::Inherited {
                *visibility = Visibility::Inherited;
            }
            ring_ready += usize::from(Some(world.0) == selection.world);
        }
    }
    profiler.set_gauge("lod/ring_retained_ready_surfaces", ring_ready as f64);
    // The no-journal contract permits cached coverage between refreshes.
    // Journal mode remains exact and is audited on every invocation. Audit a
    // legacy pass when it actually synchronized, as the demo's early return did.
    if journaled || rebuild {
        index.audit_if_requested(&mut inputs, audit.as_deref());
    }
    metrics.visible_lod_terrain_patches = index.visible_patches + batched_visible;
    {
        let any_visible = index.visible_patches > 0;
        let mut state = streaming.p1();
        if state.any_visible_logged != Some(any_visible) {
            state.any_visible_logged = Some(any_visible);
            info!(
                visible_patches = index.visible_patches,
                ready_patches = index.ready_patches,
                "terrain LOD patches drawn: {any_visible}"
            );
        }
    }
    profiler.set_gauge(
        "lod/ready_terrain_patches",
        (index.ready_patches + index.retained_ready) as f64,
    );
    profiler.set_gauge(
        "lod/visible_terrain_patches",
        metrics.visible_lod_terrain_patches as f64,
    );
    index.publish_stats(&mut profiler);
    profiler.record_elapsed("lod/visibility", started);
}

#[cfg(test)]
pub(super) fn assert_removed_sources_leave_no_ghost_keys() {
    let world = 60;
    let key = (IVec2::new(-2, 3), 2);
    let entity = Entity::from_raw_u32(1).unwrap();
    let mut index = TerrainLodIndex {
        world: Some(world),
        ..default()
    };
    let mut entry = PatchEntry {
        coverage: TerrainCoverage {
            grid: key.0,
            quadrant: key.1,
            tier: Some(LodTier::Tier4),
        },
        near: false,
        ready: true,
        ring: false,
        world: Some(world),
        path: Vec::new(),
        counted: false,
        visible: false,
    };
    index.count(entity, &mut entry);
    entry.visible = true;
    index.visible_patches = 1;
    index.patches.insert(entity, entry.clone());
    assert_eq!(index.ready_patches, 1);
    index.patches.remove(&entity);
    index.uncount(entity, &entry);
    assert!(
        index.keys.is_empty(),
        "a transferred source leaves no ghost key"
    );
    assert!(index.patches.is_empty());
    assert_eq!((index.ready_patches, index.visible_patches), (0, 0));
    assert!(
        index.dirty.contains(&key),
        "the shared batch mask is reselected after transfer"
    );
}

#[cfg(test)]
mod tests;
