//! Exact source fallback for examined cluster references, using the ordinary reference loader.
//! Coverage selects demand only; render-world handoff still decides every primitive/view/pass.

use super::{
    PendingAssetProfile, RenderOrigin, StreamingCommitBudget, StreamingWorld,
    lod::{LodDemand, LodDemandRole, LodDemandViews, LodStreaming},
    reference_spawn::{ReferenceSpawnContext, spawn_reference},
};
use crate::cluster::{reader::StockRefusalClass, source_fallback::SourceFallbackDemand};
use crate::{
    cluster::{package::ValidatedPackage, reader::CertifiedPackage},
    config::EngineConfig,
    portal::{CellRole, DistantRoleRoot, PortalState, StreamedCellKey},
    profiling::ProfilingState,
    snow::DirectionalSnowCatalog,
    world::{
        components::{
            CellRef, ExpectedModelBounds, ExteriorCellGrid, FormId, InstanceBounds, WorldTransform,
        },
        database::{CellKey, ReferenceResponse, ReferenceRow, WorldDatabase},
    },
};
use bevy::prelude::*;
use crossbeam_channel::{Receiver, TryRecvError};
use shared::cluster_mapping::CoverageState;
use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Instant,
};

type Identity = (u32, u32); // placing world, reference ID; package is retained once by the registry.

/// A failed ownership transfer must preserve its source until the failed run exits.
#[derive(Component)]
pub(super) struct FailedOwnerRoot;

#[derive(Clone)]
struct Owner {
    entity: Entity,
    parent: Entity,
    key: CellKey,
    near_key: Option<CellKey>,
    near_root: Option<Entity>,
    far: bool,
    row: Option<ReferenceRow>,
    physics_requested: bool,
}

struct Pending {
    ids: BTreeSet<u32>,
    receiver: Receiver<ReferenceResponse>,
    ready: Option<ReferenceResponse>,
}

/// Run-lifetime totals, independent of demand generations. Periodic snapshots allow rates to be
/// computed even when a ready response is held by another streaming system's commit budget.
#[derive(Default)]
struct FarStockMeasurements {
    started: Option<Instant>,
    last_log: Option<Instant>,
    collect_frames: u64,
    responses: u64,
    response_rows: u64,
    query_micros: u64,
    queue_wait_micros: u64,
    total_request_micros: u64,
    committed_cells: u64,
    spawned_references: u64,
    requests_submitted: u64,
    dispatch_refusals: u64,
    publication_attempts: u64,
    apply_micros: u64,
    max_apply_micros: u64,
    stop_admission_pressure: u64,
    stop_admission_time: u64,
    stop_time_budget: u64,
    stop_nothing_ready: u64,
}

#[derive(Resource, Default)]
pub(crate) struct FarStock {
    package: Option<Arc<CertifiedPackage>>,
    demand: Option<LodDemand>,
    /// None scans the whole worldspace; only look-refused scenery may use that reach.
    reach: Option<i32>,
    stream_radius: i32,
    generation: u64,
    desired: HashMap<Identity, CellKey>,
    owners: HashMap<Identity, Owner>,
    /// Parent and remembered near-root associations, including borrowed far owners.
    by_root: HashMap<Entity, HashSet<Identity>>,
    /// Remembered near-cell associations survive replacement of a near root.
    by_near_key: HashMap<CellKey, HashSet<Identity>>,
    holders: HashMap<CellKey, Entity>,
    pending: HashMap<CellKey, Pending>,
    failed: HashMap<CellKey, String>,
    submission_failure: Option<String>,
    queued: VecDeque<(CellKey, BTreeSet<u32>)>,
    /// Planning groups requests by cell, so each queued cell occurs exactly once.
    queued_keys: HashSet<CellKey>,
    near_roots: HashMap<CellKey, Entity>,
    near_roles: HashMap<Entity, CellRole>,
    fallback_revision: Option<u64>,
    refusal_revision: Option<u64>,
    fallback_references: BTreeSet<u32>,
    counted_worlds: HashSet<u32>,
    qualification_pending: bool,
    measurements: FarStockMeasurements,
}

pub(crate) struct FarStockPlugin;
impl Plugin for FarStockPlugin {
    fn build(&self, app: &mut App) {
        super::partial_stock::install(app);
        app.init_resource::<FarStock>()
            .init_resource::<SourceFallbackDemand>()
            .add_systems(
                Update,
                plan.after(super::sync_active_space)
                    .before(super::plan_cells),
            )
            .add_systems(
                Update,
                collect
                    .after(super::lod::collect_lod_chunks)
                    .before(super::lod::sync_lod_roles)
                    .before(super::arm_pending_models),
            )
            .add_systems(
                Update,
                publish_work
                    .after(super::track_asset_readiness)
                    .before(crate::pacing::track_world_ready),
            );
    }
}

fn cell_world(key: CellKey) -> Option<u32> {
    match key {
        CellKey::Exterior { worldspace_id, .. } => Some(worldspace_id),
        _ => None,
    }
}

// Identical near-first order for sparse request planning and ready publication.
fn sparse_cell_order(key: CellKey, center: IVec2) -> (i64, u32, i32, i32) {
    match key {
        CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        } => (
            (i64::from(grid_x) - i64::from(center.x))
                .abs()
                .max((i64::from(grid_y) - i64::from(center.y)).abs()),
            worldspace_id,
            grid_x,
            grid_y,
        ),
        CellKey::Interior(_) => unreachable!("sparse sources require exterior position grids"),
    }
}

fn role(demand: Option<LodDemand>) -> CellRole {
    match demand.map(|d| d.role) {
        Some(LodDemandRole::Main) => CellRole::Active,
        Some(LodDemandRole::Portal) => CellRole::Destination,
        _ => CellRole::Hidden,
    }
}

fn needs_stock(reference: &crate::cluster::reader::PlacedReference) -> bool {
    // Complete has no missing source primitive. All other examined verdicts retain the source;
    // unknown expected sets are never interpreted as empty. The retained path uses the ordinary
    // loader's resolver policy: marker/effect/no-model references request no scene on either path.
    // A known path to a missing converted file still demands stock and reports its load failure.
    reference.coverage != CoverageState::Complete && reference.model_path.is_some()
}

fn full_stock_reach(reference: &crate::cluster::reader::PlacedReference) -> bool {
    needs_stock(reference)
        && reference.is_static_scenery
        && reference.refusal_class == StockRefusalClass::Look
}

#[derive(Debug, Default, PartialEq, Eq)]
struct RadiusCounts {
    inside: usize,
    beyond: usize,
}

impl RadiusCounts {
    fn add(&mut self, inside: bool) {
        if inside {
            self.inside += 1;
        } else {
            self.beyond += 1;
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct StockReferenceCounts {
    refused: RadiusCounts,
    look_refused: RadiusCounts,
    runtime_refused: RadiusCounts,
    other_refused: RadiusCounts,
    fallback: RadiusCounts,
    /// Union of refused and runtime fallback references; overlaps count only once.
    total: RadiusCounts,
}

/// Model-bearing demand candidates, before the ordinary loader checks enable state or assets.
fn stock_reference_counts(
    package: &CertifiedPackage,
    demand: LodDemand,
    stream_radius: i32,
    fallback_references: &BTreeSet<u32>,
) -> StockReferenceCounts {
    let mut counts = StockReferenceCounts::default();
    for (grid, ids) in package.placed_cells_in_window(demand.world, demand.center, None) {
        let inside = demand.contains_cell(demand.world, grid, stream_radius.max(0));
        for id in ids {
            let Some(reference) = package.placed_reference(*id) else {
                continue;
            };
            if reference.model_path.is_none() {
                continue;
            }
            let refused = needs_stock(reference);
            let fallback = fallback_references.contains(id);
            if refused {
                counts.refused.add(inside);
                match reference.refusal_class {
                    StockRefusalClass::Runtime => counts.runtime_refused.add(inside),
                    StockRefusalClass::Look if reference.is_static_scenery => {
                        counts.look_refused.add(inside);
                    }
                    _ => counts.other_refused.add(inside),
                }
            }
            if fallback {
                counts.fallback.add(inside);
            }
            if refused || fallback {
                counts.total.add(inside);
            }
        }
    }
    counts
}

fn complete_source_key(
    package: &CertifiedPackage,
    demand: LodDemand,
    stream_radius: i32,
    reference_id: u32,
    source_cell: u32,
) -> Option<CellKey> {
    let reference = package.placed_reference(reference_id)?;
    let snapshot = &reference.snapshot;
    (reference.coverage == CoverageState::Complete
        && reference.model_path.is_some()
        && snapshot.source_cell_id == source_cell
        && snapshot.placed_worldspace_id == demand.world
        && demand.contains_cell(
            snapshot.placed_worldspace_id,
            IVec2::new(snapshot.cell.0, snapshot.cell.1),
            stream_radius,
        ))
    .then_some(CellKey::Exterior {
        worldspace_id: demand.world,
        grid_x: snapshot.cell.0,
        grid_y: snapshot.cell.1,
    })
}

type ExistingReferences<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static FormId, &'static CellRef, &'static ChildOf),
    Without<StreamedCellKey>,
>;

#[allow(clippy::too_many_arguments)]
fn plan(
    mut commands: Commands,
    config: Res<EngineConfig>,
    origin: Res<RenderOrigin>,
    views: LodDemandViews,
    portal: Option<Res<PortalState>>,
    mut lod: ResMut<LodStreaming>,
    package: Option<Res<ValidatedPackage>>,
    mut fallback: Option<ResMut<SourceFallbackDemand>>,
    ownership: Option<Res<crate::cluster::ownership::OwnershipFacts>>,
    mut far: ResMut<FarStock>,
    references: ExistingReferences,
    parents: Query<
        &StreamedCellKey,
        (
            With<crate::world::components::StreamedCellRoot>,
            Without<crate::swap_bridge::SwapStandIn>,
        ),
    >,
    mut removed_references: RemovedComponents<FormId>,
    changed_references: Query<Entity, Changed<FormId>>,
) {
    let changed: HashSet<_> = removed_references
        .read()
        .chain(changed_references.iter())
        .collect();
    if !changed.is_empty() {
        commands.queue(move |world: &mut World| {
            with_far(world, |world, far| {
                let lost: Vec<_> = far
                    .owners
                    .iter()
                    .filter(|(id, owner)| {
                        changed.contains(&owner.entity)
                            && world.get::<FormId>(owner.entity).map(|f| f.0) != Some(id.1)
                    })
                    .map(|(&id, owner)| (id, owner.clone()))
                    .collect();
                for (id, owner) in lost {
                    far.remove_owner(&id);
                    ownership_failed(
                        world,
                        far,
                        &owner,
                        "owned source identity was removed".into(),
                    );
                }
            });
        });
    }
    commands.queue(|world: &mut World| with_far(world, reconcile_near));
    let Some(package) = package else {
        if far.package.is_some() {
            let owners = far.take_owners();
            let holders = std::mem::take(&mut far.holders);
            let package = far.package.take();
            commands
                .queue(move |world: &mut World| retire_sources(world, package, owners, holders));
            far.near_roots.clear();
            far.near_roles.clear();
            far.desired.clear();
            far.pending.clear();
            far.clear_queued();
            far.failed.clear();
            far.submission_failure = None;
            far.demand = None;
            far.fallback_revision = None;
            far.refusal_revision = None;
            far.fallback_references.clear();
            far.counted_worlds.clear();
            far.qualification_pending = false;
        }
        return;
    };
    let changed_package = far
        .package
        .as_ref()
        .is_none_or(|p| !Arc::ptr_eq(p, &package.0));
    if changed_package {
        far.fallback_revision = None;
        far.fallback_references.clear();
        far.counted_worlds.clear();
    }
    let refusal_revision = ownership
        .as_deref()
        .filter(|facts| facts.package_matches(&package.0))
        .map(|facts| facts.refusal_revision());
    let mut fallback_changed = changed_package || far.refusal_revision != refusal_revision;
    far.refusal_revision = refusal_revision;
    far.qualification_pending = true;
    if let Some((revision, references)) = fallback
        .as_mut()
        .and_then(|feedback| feedback.consume_for_package(&package.0))
    {
        far.qualification_pending = false;
        if far.fallback_revision != Some(revision) {
            far.fallback_revision = Some(revision);
            far.fallback_references.clone_from(references);
            fallback_changed = true;
        }
    }
    let actual = views.current(&config, origin.0, portal.as_deref());
    let demand = views.residency_demand(&config, actual, portal.as_deref(), &mut lod);
    // Owned clusters have no distance cutoff. Only recorded look-only STAT/TREE refusals
    // and fixed material refusals inherit that reach. Runtime-state and unknown
    // source feedback keep the grid.
    // Native LOD stays disabled in owned mode, including for grid-bound references.
    let stream_radius = config.stream_radius.max(0);
    let reach = (!config.cluster_own_statics).then_some(stream_radius);
    if !changed_package
        && !fallback_changed
        && far.demand == demand
        && far.reach == reach
        && far.stream_radius == stream_radius
    {
        return;
    }
    if let Some(demand) = demand
        && far.fallback_revision.is_some()
        && far.counted_worlds.insert(demand.world)
    {
        let counts = stock_reference_counts(
            &package.0,
            demand,
            config.stream_radius,
            &far.fallback_references,
        );
        info!(
            worldspace = demand.world,
            center = ?demand.center,
            stream_radius = config.stream_radius.max(0),
            ?reach,
            refused_inside = counts.refused.inside,
            refused_beyond = counts.refused.beyond,
            look_refused_inside = counts.look_refused.inside,
            look_refused_beyond = counts.look_refused.beyond,
            runtime_refused_inside = counts.runtime_refused.inside,
            runtime_refused_beyond = counts.runtime_refused.beyond,
            other_refused_inside = counts.other_refused.inside,
            other_refused_beyond = counts.other_refused.beyond,
            distant_look_stock = if config.cluster_own_statics { counts.look_refused.beyond } else { 0 },
            distant_runtime_stock = 0usize,
            distant_material_stock = if config.cluster_own_statics { counts.fallback.beyond } else { 0 },
            fallback_inside = counts.fallback.inside,
            fallback_beyond = counts.fallback.beyond,
            total_inside = counts.total.inside,
            total_beyond = counts.total.beyond,
            "stock candidate inventory and classified distant demand at first qualification (once per package/worldspace)"
        );
    }
    // Only frozen material refusals inherit unbounded static-scenery reach.
    // Unclassified runtime source feedback has no authority to extend the grid.
    // This projection runs only after a changed demand/qualification, never settled frames.
    let permanent_refusals = ownership
        .as_deref()
        .filter(|facts| facts.package_matches(&package.0))
        .map(|facts| facts.permanent_refused_references())
        .unwrap_or_default();
    let mut desired = HashMap::new();
    if let Some(demand) = demand {
        for (grid, ids) in package
            .0
            .placed_cells_in_window(demand.world, demand.center, reach)
        {
            let inside = demand.contains_cell(demand.world, grid, stream_radius);
            let key = CellKey::Exterior {
                worldspace_id: demand.world,
                grid_x: grid.x,
                grid_y: grid.y,
            };
            for id in ids {
                if package.0.placed_reference(*id).is_some_and(|reference| {
                    reference.model_path.is_some()
                        && (inside
                            || (config.cluster_own_statics
                                && (full_stock_reach(reference)
                                    || (reference.is_static_scenery
                                        && permanent_refusals.contains(id)))))
                        && (needs_stock(reference)
                            || far.fallback_references.contains(id)
                            || permanent_refusals.contains(id)
                            || (!changed_package && far.owners.contains_key(&(demand.world, *id))))
                }) {
                    desired.insert((demand.world, *id), key);
                }
            }
        }
        // An existing near source is not a redundant scene. Retain its exact
        // entity through retirement, including Complete references with runtime edits.
        for (_, form, cell, parent) in &references {
            let Ok(parent_key) = parents.get(parent.parent()) else {
                continue;
            };
            if cell_world(parent_key.0) != Some(demand.world) {
                continue;
            }
            if let Some(key) =
                complete_source_key(&package.0, demand, stream_radius, form.0, cell.0)
            {
                desired.insert((demand.world, form.0), key);
            }
        }
    }
    // Another world's negative template arrival must not cancel current exact-ID work.
    if !changed_package
        && far.demand == demand
        && far.reach == reach
        && far.stream_radius == stream_radius
        && far.desired == desired
    {
        return;
    }
    far.generation = far.generation.wrapping_add(1);
    far.pending.clear();
    far.failed.clear();
    far.submission_failure = None;
    if changed_package {
        let owners = far.take_owners();
        let holders = std::mem::take(&mut far.holders);
        let old_package = far.package.take();
        commands
            .queue(move |world: &mut World| retire_sources(world, old_package, owners, holders));
        far.near_roots.clear();
        far.near_roles.clear();
        far.package = Some(package.0.clone());
    }
    far.demand = demand;
    far.reach = reach;
    far.stream_radius = stream_radius;
    far.near_roles.clear();
    let retired: HashMap<_, _> = far
        .retain_owners(|id, _| desired.contains_key(id))
        .into_iter()
        .filter(|(_, owner)| owner.far)
        .collect();
    if !retired.is_empty() {
        let old_package = far.package.clone();
        commands.queue(move |world: &mut World| {
            retire_sources(world, old_package, retired, HashMap::new());
        });
    }
    far.desired = desired;
    // Certification can complete after the ordinary cells. Discover those exact direct roots
    // once per demand, retaining their source entity instead of creating a duplicate far scene.
    let mut conflicts = HashSet::new();
    for (entity, form, cell, parent) in &references {
        let Ok(parent_key) = parents.get(parent.parent()) else {
            continue;
        };
        let Some(world) = cell_world(parent_key.0) else {
            continue;
        };
        let id = (world, form.0);
        if !far.desired.contains_key(&id) {
            continue;
        }
        let Some(snapshot) = package.0.placed_reference(form.0) else {
            continue;
        };
        if snapshot.snapshot.source_cell_id != cell.0 {
            continue;
        }
        if let Some(owner) = far.owners.get(&id) {
            if owner.entity != entity {
                conflicts.insert(id);
            }
            continue;
        }
        far.near_roots.insert(parent_key.0, parent.parent());
        far.put_owner(
            id,
            Owner {
                entity,
                parent: parent.parent(),
                key: parent_key.0,
                near_key: Some(parent_key.0),
                near_root: Some(parent.parent()),
                far: false,
                row: None,
                physics_requested: super::builds_world_colliders(&config),
            },
        );
    }
    for id in conflicts {
        far.remove_owner(&id);
        let key = far.desired[&id];
        far.failed.insert(
            key,
            format!(
                "duplicate existing reference {:08X} in placing world {:08X}",
                id.1, id.0
            ),
        );
    }
    let mut wanted = HashMap::<CellKey, BTreeSet<u32>>::new();
    for (&id, &key) in &far.desired {
        if !far.owners.contains_key(&id) && !far.failed.contains_key(&key) {
            wanted.entry(key).or_default().insert(id.1);
        }
    }
    let center = demand.map_or(IVec2::ZERO, |d| d.center);
    let mut wanted: Vec<_> = wanted.into_iter().collect();
    wanted.sort_by_key(|(key, _)| sparse_cell_order(*key, center));
    far.set_queued(wanted.into());
    commands.queue(|world: &mut World| with_far(world, refresh_holders));
    info!(
        generation = far.generation,
        desired = far.desired.len(),
        resident = far.owners.len(),
        failed_cells = far.failed.len(),
        ?demand,
        "exact distant source demand changed"
    );
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct CollectParams<'w, 's> {
    commands: Commands<'w, 's>,
    config: Res<'w, EngineConfig>,
    ownership: Option<Res<'w, crate::cluster::ownership::OwnershipFacts>>,
    database: Res<'w, WorldDatabase>,
    asset_server: Res<'w, AssetServer>,
    snow: Res<'w, DirectionalSnowCatalog>,
    origin: Res<'w, RenderOrigin>,
    stamps: Option<Res<'w, crate::portal::StreamedCellLayerStamps>>,
    far: ResMut<'w, FarStock>,
    streaming: ResMut<'w, StreamingWorld>,
    budget: ResMut<'w, StreamingCommitBudget>,
    profiler: ResMut<'w, ProfilingState>,
    runtime: Option<Res<'w, super::runtime::StreamingRuntime>>,
}

/// IO responses are polled independently of publication. Publication applies one
/// exact response before admitting the next, so the shared clock includes hooks
/// and command application as well as validation and command construction.
pub(super) fn collect(
    world: &mut World,
    state: &mut bevy::ecs::system::SystemState<CollectParams>,
) {
    let mut received = 0_u64;
    let mut received_rows = 0_u64;
    let mut query_micros = 0_u64;
    let mut queue_wait_micros = 0_u64;
    let mut total_request_micros = 0_u64;
    let mut committed_cells = 0_u64;
    let mut spawned_references = 0_u64;
    let mut publication_attempts = 0_u64;
    let mut requests_submitted = 0_u64;
    let mut dispatch_refusals = 0_u64;
    let mut apply_micros = 0_u64;
    let mut max_apply_micros = 0_u64;
    let mut stop_time_budget = false;
    let mut stop_admission_pressure = false;
    let mut stop_admission_time = false;
    let ready_keys = {
        let CollectParams {
            mut far,
            mut profiler,
            ..
        } = state
            .get_mut(world)
            .expect("sparse collector resources must remain present");
        if far.package.is_none() {
            return;
        }
        let generation = far.generation;
        let mut ready = Vec::new();
        for (&key, pending) in &mut far.pending {
            if pending.ready.is_none() {
                match pending.receiver.try_recv() {
                    Ok(response) => {
                        received += 1;
                        received_rows += response.result.as_ref().map_or(0, Vec::len) as u64;
                        query_micros = query_micros.saturating_add(response.query_micros);
                        queue_wait_micros =
                            queue_wait_micros.saturating_add(response.queue_wait_micros);
                        total_request_micros =
                            total_request_micros.saturating_add(response.total_request_micros);
                        profiler.set_gauge(
                            "lod/exact_stock_query_ms",
                            response.query_micros as f64 / 1_000.0,
                        );
                        profiler.set_gauge(
                            "lod/exact_stock_queue_wait_ms",
                            response.queue_wait_micros as f64 / 1_000.0,
                        );
                        profiler.set_gauge(
                            "lod/exact_stock_total_request_ms",
                            response.total_request_micros as f64 / 1_000.0,
                        );
                        pending.ready = Some(response);
                    }
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => {
                        // Service a disconnected request as failed work under
                        // the same publication budget, without losing its key.
                        pending.ready = Some(ReferenceResponse {
                            generation,
                            key,
                            result: Err("sparse database response disconnected".into()),
                            query_micros: 0,
                            queue_wait_micros: 0,
                            total_request_micros: 0,
                        });
                    }
                }
            }
            if pending.ready.is_some() {
                ready.push(key);
            }
        }
        let center = far.demand.map_or(IVec2::ZERO, |demand| demand.center);
        ready.sort_by_key(|key| sparse_cell_order(*key, center));
        ready
    };
    for key in ready_keys {
        {
            let CollectParams {
                mut commands,
                config,
                ownership,
                asset_server,
                snow,
                origin,
                stamps,
                mut far,
                mut streaming,
                mut budget,
                mut profiler,
                runtime,
                ..
            } = state
                .get_mut(world)
                .expect("sparse collector resources must remain present");
            if !far
                .pending
                .get(&key)
                .is_some_and(|pending| pending.ready.is_some())
            {
                continue;
            }
            if (budget.commits > 0
                && config.max_commit_micros_per_frame > 0
                && budget.frame_started.elapsed().as_micros()
                    >= u128::from(config.max_commit_micros_per_frame))
                || budget.elapsed_limit_reached(runtime.as_deref())
            {
                stop_time_budget = true;
                break;
            }
            // Invalid and failed responses cost real work too. Full-cell slots
            // belong only to full-cell commits and are not spent here.
            budget.commits = budget.commits.saturating_add(1);
            publication_attempts += 1;
            let pending = far.pending.remove(&key).unwrap();
            let response = pending.ready.unwrap();
            if response.generation != far.generation || response.key != key {
                far.failed.insert(
                    key,
                    "sparse response generation or cell differs from request".into(),
                );
                continue;
            }
            let rows = match response.result {
                Ok(rows) => rows,
                Err(reason) => {
                    error!(?key,%reason,"exact distant source query failed");
                    far.failed.insert(key, reason);
                    continue;
                }
            };
            let ids: BTreeSet<_> = rows.iter().map(|r| r.form_id).collect();
            if ids != pending.ids || rows.len() != ids.len() {
                far.failed
                    .insert(key, "sparse response identities differ from request".into());
                continue;
            }
            let package = far.package.as_ref().unwrap().clone();
            let verified = rows.iter().all(|row| row_matches(&package, row));
            if !verified {
                far.failed.insert(
                    key,
                    "sparse response disagrees with retained authoritative placement".into(),
                );
                continue;
            }
            let current_role = role(far.demand);
            let holder = *far.holders.entry(key).or_insert_with(|| {
                let CellKey::Exterior { grid_x, grid_y, .. } = key else {
                    unreachable!()
                };
                commands
                    .spawn((
                        Name::new(format!("Exact distant sources {key:?}")),
                        DistantRoleRoot,
                        StreamedCellKey(key),
                        ExteriorCellGrid(IVec2::new(grid_x, grid_y)),
                        Transform::from_translation(super::grid_translation(
                            IVec2::new(grid_x, grid_y),
                            origin.0,
                        )),
                        holder_visibility(current_role),
                    ))
                    .id()
            });
            commands.queue(move |world: &mut World| {
                if let Err(reason) = sync_holder_role(world, holder, current_role) {
                    fatal(world, reason);
                }
            });
            // Use the ordinary spawner's placed-object source unless portal isolation owns stamps.
            let layers = stamps.as_ref().map(|_| match current_role {
                CellRole::Active => crate::render::reflectable_world_layers(),
                CellRole::Destination => {
                    bevy::camera::visibility::RenderLayers::layer(crate::portal::DESTINATION_LAYER)
                }
                CellRole::Hidden => bevy::camera::visibility::RenderLayers::none(),
            });
            let mut spawned = Vec::new();
            let mut missing_models = Vec::new();
            commands.entity(holder).with_children(|parent| {
                let mut context = ReferenceSpawnContext {
                    key,
                    stamp: layers.as_ref(),
                    snow: &snow,
                    model_sequence: &mut streaming.next_model_sequence,
                    asset_server: &asset_server,
                    profiler: &mut profiler,
                    world_physics: false,
                    lights: config.lights,
                    schedule_scenes: super::requests::enabled(&config),
                };
                for row in rows {
                    let id = (cell_world(key).unwrap(), row.form_id);
                    if far.owners.contains_key(&id) || !far.desired.contains_key(&id) {
                        continue;
                    }
                    let Some((entity, requested)) =
                        spawn_reference(parent, row.clone(), &mut context)
                    else {
                        continue;
                    };
                    let beyond_grid = far.demand.is_some_and(|demand| {
                        let CellKey::Exterior { grid_x, grid_y, .. } = key else {
                            return false;
                        };
                        !demand.contains_cell(
                            demand.world,
                            IVec2::new(grid_x, grid_y),
                            far.stream_radius,
                        )
                    });
                    if let Some(partial) = super::partial_stock::PartialStock::for_demand(
                        &package,
                        &row,
                        config.cluster_own_statics,
                        beyond_grid,
                        ownership.as_deref(),
                    ) {
                        parent.commands().entity(entity).insert(partial);
                    }
                    if !requested {
                        warn!(
                            reference = row.form_id,
                            ?key,
                            "distant stock source has no ordinary model request"
                        );
                        missing_models.push(entity);
                        far.failed.insert(
                            key,
                            format!(
                                "reference {:08X} has no ordinary model request",
                                row.form_id
                            ),
                        );
                    }
                    spawned.push((
                        id,
                        Owner {
                            entity,
                            parent: holder,
                            key,
                            near_key: None,
                            near_root: None,
                            far: true,
                            row: Some(row),
                            physics_requested: false,
                        },
                    ));
                }
            });
            for entity in missing_models {
                commands.entity(entity).insert(super::AssetFailed);
            }
            spawned_references += spawned.len() as u64;
            for (id, owner) in spawned {
                far.put_owner(id, owner);
            }
            committed_cells += 1;
            profiler.increment("lod/exact_stock_cells_committed", 1);
        }
        // All SystemState borrows end above. Every resource, including FarStock
        // and StreamingCommitBudget, stays installed while commands and observers
        // run. Reacquire the live clock only after this response has applied.
        let apply_started = Instant::now();
        state.apply(world);
        let elapsed = apply_started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        apply_micros = apply_micros.saturating_add(elapsed);
        max_apply_micros = max_apply_micros.max(elapsed);
    }
    let CollectParams {
        config,
        database,
        mut far,
        streaming,
        mut profiler,
        runtime,
        ..
    } = state
        .get_mut(world)
        .expect("sparse collector resources must remain present");
    let dispatch_started = Instant::now();
    let mut dispatch_considered = 0_u64;
    while !far.queued.is_empty() {
        // Reuse existing controlled admission and backlog policy. Ready replies
        // retain their outstanding place until publication, not just worker IO.
        let backlog_limit = config.streaming_backlog_limit();
        if runtime
            .as_deref()
            .is_some_and(|runtime| !runtime.cell_requests_allowed())
            || (backlog_limit != 0
                && far
                    .pending
                    .len()
                    .saturating_add(streaming.outstanding_cells.len())
                    >= backlog_limit)
        {
            stop_admission_pressure = true;
            break;
        }
        if dispatch_considered > 0
            && runtime
                .as_deref()
                .and_then(|runtime| runtime.decision)
                .is_some_and(|decision| {
                    dispatch_started.elapsed().as_micros()
                        >= u128::from(decision.budgets.max_admission_micros)
                })
        {
            stop_admission_time = true;
            break;
        }
        let Some((key, mut ids)) = far.pop_queued() else {
            break;
        };
        dispatch_considered += 1;
        let placing_world = cell_world(key).unwrap();
        ids.retain(|id| !far.owners.contains_key(&(placing_world, *id)));
        if ids.is_empty() {
            continue;
        }
        match database.request_references(far.generation, key, ids.clone()) {
            Ok(receiver) => {
                far.pending.insert(
                    key,
                    Pending {
                        ids,
                        receiver,
                        ready: None,
                    },
                );
                far.submission_failure = None;
                requests_submitted += 1;
            }
            Err(error) => {
                dispatch_refusals += 1;
                let reason = format!("{error:#}");
                if far.submission_failure.as_ref() != Some(&reason) {
                    warn!(%reason,"exact distant source request remains pending");
                    far.submission_failure = Some(reason);
                }
                far.requeue_front(key, ids);
                break;
            }
        }
    }
    let queued_cells = far.queued.len();
    let pending_cells = far.pending.len();
    let ready_cells = far
        .pending
        .values()
        .filter(|pending| pending.ready.is_some())
        .count();
    let awaiting_response_cells = pending_cells - ready_cells;
    let failed_cells = far.failed.len();
    let stop_nothing_ready = !stop_time_budget && ready_cells == 0;
    profiler.increment("lod/exact_stock_requests_submitted", requests_submitted);
    profiler.increment("lod/exact_stock_dispatch_refusals", dispatch_refusals);
    profiler.increment("lod/exact_stock_publication_attempts", publication_attempts);
    profiler.increment("lod/exact_stock_responses_received", received);
    profiler.increment("lod/exact_stock_response_rows", received_rows);
    profiler.increment("lod/exact_stock_spawned_references", spawned_references);
    profiler.increment(
        "lod/exact_stock_stop_time_budget",
        u64::from(stop_time_budget),
    );
    profiler.increment(
        "lod/exact_stock_stop_admission_pressure",
        u64::from(stop_admission_pressure),
    );
    profiler.increment(
        "lod/exact_stock_stop_admission_time",
        u64::from(stop_admission_time),
    );
    profiler.increment(
        "lod/exact_stock_stop_nothing_ready",
        u64::from(stop_nothing_ready),
    );
    profiler.set_gauge("lod/exact_stock_queued_cells", queued_cells as f64);
    profiler.set_gauge("lod/exact_stock_pending_cells", pending_cells as f64);
    profiler.set_gauge("lod/exact_stock_ready_cells", ready_cells as f64);
    profiler.set_gauge(
        "lod/exact_stock_awaiting_response_cells",
        awaiting_response_cells as f64,
    );
    profiler.set_gauge("lod/exact_stock_failed_cells", failed_cells as f64);
    if committed_cells != 0 {
        profiler.record_micros("lod/exact_stock_apply", apply_micros);
        profiler.set_gauge(
            "lod/exact_stock_max_apply_ms",
            max_apply_micros as f64 / 1_000.0,
        );
    }
    let now = Instant::now();
    let generation = far.generation;
    let measurements = &mut far.measurements;
    let started = *measurements.started.get_or_insert(now);
    measurements.collect_frames += 1;
    measurements.requests_submitted = measurements
        .requests_submitted
        .saturating_add(requests_submitted);
    measurements.dispatch_refusals = measurements
        .dispatch_refusals
        .saturating_add(dispatch_refusals);
    measurements.publication_attempts = measurements
        .publication_attempts
        .saturating_add(publication_attempts);
    measurements.apply_micros = measurements.apply_micros.saturating_add(apply_micros);
    measurements.max_apply_micros = measurements.max_apply_micros.max(max_apply_micros);
    measurements.responses = measurements.responses.saturating_add(received);
    measurements.response_rows = measurements.response_rows.saturating_add(received_rows);
    measurements.query_micros = measurements.query_micros.saturating_add(query_micros);
    measurements.queue_wait_micros = measurements
        .queue_wait_micros
        .saturating_add(queue_wait_micros);
    measurements.total_request_micros = measurements
        .total_request_micros
        .saturating_add(total_request_micros);
    measurements.committed_cells = measurements.committed_cells.saturating_add(committed_cells);
    measurements.spawned_references = measurements
        .spawned_references
        .saturating_add(spawned_references);
    measurements.stop_time_budget += u64::from(stop_time_budget);
    measurements.stop_admission_pressure += u64::from(stop_admission_pressure);
    measurements.stop_admission_time += u64::from(stop_admission_time);
    measurements.stop_nothing_ready += u64::from(stop_nothing_ready);
    if measurements
        .last_log
        .is_none_or(|last| now.duration_since(last).as_secs() >= 5)
    {
        measurements.last_log = Some(now);
        info!(
            generation,
            elapsed_ms = now.duration_since(started).as_millis() as u64,
            collect_frames = measurements.collect_frames,
            requests_submitted = measurements.requests_submitted,
            dispatch_refusals = measurements.dispatch_refusals,
            publication_attempts = measurements.publication_attempts,
            apply_micros = measurements.apply_micros,
            max_apply_micros = measurements.max_apply_micros,
            responses = measurements.responses,
            response_rows = measurements.response_rows,
            query_micros = measurements.query_micros,
            queue_wait_micros = measurements.queue_wait_micros,
            total_request_micros = measurements.total_request_micros,
            committed_cells = measurements.committed_cells,
            spawned_references = measurements.spawned_references,
            stop_time_budget = measurements.stop_time_budget,
            stop_admission_pressure = measurements.stop_admission_pressure,
            stop_admission_time = measurements.stop_admission_time,
            stop_nothing_ready = measurements.stop_nothing_ready,
            queued_cells,
            pending_cells,
            ready_cells,
            awaiting_response_cells,
            failed_cells,
            "exact distant source measurements (cumulative)"
        );
    }
}

pub(super) fn publish_work(
    mut far: ResMut<FarStock>,
    mut metrics: ResMut<super::StreamingMetrics>,
    failures: Query<(Entity, &FormId), With<super::AssetFailed>>,
) {
    if let Some(world) = far.demand.map(|d| d.world) {
        for (entity, form) in &failures {
            if let Some(owner) = far.owners.get(&(world, form.0))
                && owner.entity == entity
            {
                let key = far
                    .desired
                    .get(&(world, form.0))
                    .copied()
                    .unwrap_or(owner.key);
                far.failed.entry(key).or_insert_with(|| {
                    format!("reference {:08X} failed ordinary asset validation", form.0)
                });
            }
        }
    }
    metrics.queued_far_stock_queries = far.queued.len();
    metrics.pending_far_stock_queries = far.pending.len();
    metrics.pending_far_stock_qualification =
        usize::from(far.package.is_some() && far.demand.is_some() && far.qualification_pending);
    metrics.failed_far_stock_sources = far.failed.len();
}

/// Extra residency facts only. Drawable geometry/material/pass proof remains in the render world.
#[derive(Default)]
pub(crate) struct FarCellState {
    pub(crate) pending: usize,
    pub(crate) failed: bool,
    pub(crate) holder: Option<Entity>,
}

fn row_matches(package: &CertifiedPackage, row: &ReferenceRow) -> bool {
    let Some(placed) = package.placed_reference(row.form_id) else {
        return false;
    };
    row.base_form_id == placed.snapshot.base_form_id
        && row.cell_id == placed.snapshot.source_cell_id
        && row.position.map(f32::to_bits) == placed.pose.position.map(f32::to_bits)
        && row.rotation.map(f32::to_bits) == placed.pose.rotation.map(f32::to_bits)
        && row.scale.to_bits() == placed.pose.scale.to_bits()
        && row
            .model_path
            .as_deref()
            .and_then(shared::cluster_recipe::classify::model_glb_path)
            == placed.model_path
}

fn fatal(world: &mut World, reason: String) {
    error!(%reason,"exact distant source ownership failed");
    world.write_message(AppExit::error());
}

fn ownership_failed(world: &mut World, far: &mut FarStock, owner: &Owner, reason: String) {
    if let Ok(mut parent) = world.get_entity_mut(owner.parent) {
        parent.insert(FailedOwnerRoot);
    }
    far.failed.insert(owner.key, reason.clone());
    fatal(world, reason);
}

pub(super) fn removal_blocked(world: &World, root: Entity) -> bool {
    world.get::<FailedOwnerRoot>(root).is_some()
}

pub(super) fn queue_despawn(commands: &mut Commands, root: Entity) {
    commands.queue(move |world: &mut World| {
        super::despawn_subtree(world, root);
    });
}

fn with_far(world: &mut World, action: impl FnOnce(&mut World, &mut FarStock)) {
    let Some(mut far) = world.remove_resource::<FarStock>() else {
        return;
    };
    action(world, &mut far);
    world.insert_resource(far);
}

impl FarStock {
    fn put_owner(&mut self, id: Identity, owner: Owner) {
        self.remove_owner(&id);
        for root in [Some(owner.parent), owner.near_root].into_iter().flatten() {
            self.by_root.entry(root).or_default().insert(id);
        }
        if let Some(key) = owner.near_key {
            self.by_near_key.entry(key).or_default().insert(id);
        }
        self.owners.insert(id, owner);
    }

    fn remove_owner(&mut self, id: &Identity) -> Option<Owner> {
        let owner = self.owners.remove(id)?;
        for root in [Some(owner.parent), owner.near_root].into_iter().flatten() {
            if let Some(ids) = self.by_root.get_mut(&root) {
                ids.remove(id);
                if ids.is_empty() {
                    self.by_root.remove(&root);
                }
            }
        }
        if let Some(key) = owner.near_key
            && let Some(ids) = self.by_near_key.get_mut(&key)
        {
            ids.remove(id);
            if ids.is_empty() {
                self.by_near_key.remove(&key);
            }
        }
        Some(owner)
    }

    fn take_owners(&mut self) -> HashMap<Identity, Owner> {
        self.by_root.clear();
        self.by_near_key.clear();
        std::mem::take(&mut self.owners)
    }

    /// Return removed owners so the caller can retire borrowed sources with their original pose.
    fn retain_owners(
        &mut self,
        mut keep: impl FnMut(&Identity, &Owner) -> bool,
    ) -> HashMap<Identity, Owner> {
        let removed: Vec<_> = self
            .owners
            .iter()
            .filter_map(|(id, owner)| (!keep(id, owner)).then_some(*id))
            .collect();
        removed
            .into_iter()
            .map(|id| (id, self.remove_owner(&id).unwrap()))
            .collect()
    }

    fn ids_for_root(&self, root: Entity) -> impl Iterator<Item = Identity> + '_ {
        self.by_root
            .get(&root)
            .into_iter()
            .flat_map(|ids| ids.iter().copied())
    }

    fn set_queued(&mut self, queued: VecDeque<(CellKey, BTreeSet<u32>)>) {
        self.queued_keys = queued.iter().map(|(key, _)| *key).collect();
        self.queued = queued;
    }

    fn clear_queued(&mut self) {
        self.queued.clear();
        self.queued_keys.clear();
    }

    fn pop_queued(&mut self) -> Option<(CellKey, BTreeSet<u32>)> {
        let next = self.queued.pop_front()?;
        self.queued_keys.remove(&next.0);
        Some(next)
    }

    fn requeue_front(&mut self, key: CellKey, ids: BTreeSet<u32>) {
        self.queued.push_front((key, ids));
        self.queued_keys.insert(key);
    }

    pub(crate) fn cell_state(&self, key: CellKey) -> FarCellState {
        FarCellState {
            pending: usize::from(self.pending.contains_key(&key))
                + usize::from(
                    self.qualification_pending
                        && self
                            .demand
                            .is_some_and(|demand| cell_world(key) == Some(demand.world)),
                )
                + usize::from(self.queued_keys.contains(&key)),
            failed: self.failed.contains_key(&key),
            holder: self.holders.get(&key).copied(),
        }
    }
    pub(super) fn roles(&self) -> impl Iterator<Item = (Entity, CellRole)> + '_ {
        self.holders
            .values()
            .copied()
            .map(|root| (root, role(self.demand)))
    }
    /// Reserve an already existing source before the ordinary spawn loop; no second scene load.
    pub(super) fn take_adoptions(
        &mut self,
        key: CellKey,
        references: &mut Vec<ReferenceRow>,
        alive: impl Fn(Entity) -> bool,
    ) -> Vec<u32> {
        let Some(world) = cell_world(key) else {
            return Vec::new();
        };
        let mut adopted = Vec::new();
        references.retain(|row| {
            let id = (world, row.form_id);
            if let Some(owner) = self.owners.get(&id) {
                if alive(owner.entity) {
                    adopted.push(row.form_id);
                    return false;
                }
                self.remove_owner(&id);
            }
            true
        });
        adopted
    }
}

fn holder(world: &mut World, far: &mut FarStock, key: CellKey) -> Result<Entity, String> {
    if let Some(&root) = far.holders.get(&key)
        && world.get_entity(root).is_ok()
    {
        return Ok(root);
    }
    let CellKey::Exterior { grid_x, grid_y, .. } = key else {
        return Err("interior sparse holder".into());
    };
    let origin = world.resource::<RenderOrigin>().0;
    let root = world
        .spawn((
            Name::new(format!("Exact distant sources {key:?}")),
            DistantRoleRoot,
            StreamedCellKey(key),
            ExteriorCellGrid(IVec2::new(grid_x, grid_y)),
            Transform::from_translation(super::grid_translation(
                IVec2::new(grid_x, grid_y),
                origin,
            )),
            holder_visibility(role(far.demand)),
        ))
        .id();
    sync_holder_role(world, root, role(far.demand))?;
    far.holders.insert(key, root);
    Ok(root)
}

fn holder_visibility(role: CellRole) -> Visibility {
    if role == CellRole::Hidden {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    }
}

fn sync_holder_role(world: &mut World, root: Entity, role: CellRole) -> Result<(), String> {
    // Retain role-owner validation, transfer/collider authority and the seamless source registry.
    crate::portal::sync_distant_role(world, root, role)?;
    if !world.contains_resource::<crate::portal::StreamedCellLayerStamps>() {
        // Ordinary stock is a placed-object hierarchy, including scenes arriving after a refresh
        // or transfer. Visibility carries Hidden; portal roles must not replace this layer source.
        world
            .entity_mut(root)
            .insert(bevy::app::Propagate(super::PLACED_OBJECT_RENDER_LAYERS));
    }
    Ok(())
}

fn move_owner(
    world: &mut World,
    owner: &mut Owner,
    to: Entity,
    key: CellKey,
    far: bool,
    authoritative_pose: Option<crate::cluster::reader::ReferencePose>,
) -> Result<(), String> {
    let old_parent = *world
        .get::<Transform>(owner.parent)
        .ok_or("old source parent has no transform")?;
    let new_parent = *world
        .get::<Transform>(to)
        .ok_or("new source parent has no transform")?;
    if old_parent.rotation != Quat::IDENTITY
        || new_parent.rotation != Quat::IDENTITY
        || old_parent.scale != Vec3::ONE
        || new_parent.scale != Vec3::ONE
    {
        return Err("source owner is not an unrotated cell translation".into());
    }
    let mut pose = *world
        .get::<Transform>(owner.entity)
        .ok_or("source root disappeared")?;
    let original_pose =
        authoritative_pose.ok_or("source reference lost its authoritative placement")?;
    let expected_old = super::reference_placement(
        original_pose.position,
        original_pose.rotation,
        original_pose.scale,
        owner.key,
    )
    .1;
    if pose == expected_old {
        // Recompute from raw placement instead of adding a rounded grid delta: handoff requires
        // the identical ordinary-loader bits even for persistent references and large origins.
        pose = super::reference_placement(
            original_pose.position,
            original_pose.rotation,
            original_pose.scale,
            key,
        )
        .1;
    } else {
        // Preserve an edited source as stock; moving ownership must not reset runtime edits.
        pose.translation += old_parent.translation - new_parent.translation;
    }
    let target_role =
        crate::portal::owned_root_role(world, to).ok_or("new source owner has no proved role")?;
    crate::portal::rebind_reference_layers(world, owner.entity, owner.parent, to, target_role)?;
    if !far {
        super::partial_stock::promote(world, owner.entity);
    }
    let expected = world.get::<ExpectedModelBounds>(owner.entity).copied();
    let matrix = pose.to_matrix();
    world
        .entity_mut(owner.entity)
        .insert((pose, WorldTransform(matrix)));
    if let Some(expected) = expected {
        world
            .entity_mut(owner.entity)
            .insert(InstanceBounds::transformed(
                expected.min,
                expected.max,
                matrix,
            ));
    }
    owner.parent = to;
    owner.key = key;
    owner.far = far;
    if !far {
        owner.near_key = Some(key);
        owner.near_root = Some(to);
        if !owner.physics_requested
            && super::builds_world_colliders(world.resource::<EngineConfig>())
        {
            if let Some(mut pending) = world.get_mut::<PendingAssetProfile>(owner.entity) {
                pending.static_physics = true;
            } else if let Some(row) = &owner.row
                && let Some(path) = row.model_path.clone().and_then(super::converted_model_path)
            {
                world.entity_mut(owner.entity).insert(PendingAssetProfile {
                    started: Instant::now(),
                    scene_spawned: true,
                    path,
                    form_id: row.form_id,
                    base_form_id: row.base_form_id,
                    base_record_type: row.base_record_type.clone(),
                    static_physics: true,
                    cell_id: row.cell_id,
                });
            }
            owner.physics_requested = true;
        }
    }
    Ok(())
}

/// Must execute before hiding/removing a near root or making it a SwapStandIn.
pub(super) fn release(world: &mut World, root: Entity) {
    with_far(world, |world, far| {
        let ids: Vec<_> = far
            .ids_for_root(root)
            .filter(|id| {
                far.owners
                    .get(id)
                    .is_some_and(|owner| owner.parent == root && !owner.far)
            })
            .collect();
        for id in ids {
            let Some(&key) = far.desired.get(&id) else {
                far.remove_owner(&id);
                continue;
            };
            let target = match holder(world, far, key) {
                Ok(root) => root,
                Err(reason) => {
                    if let Some(owner) = far.owners.get(&id).cloned() {
                        ownership_failed(world, far, &owner, reason);
                    }
                    continue;
                }
            };
            let Some(mut owner) = far.remove_owner(&id) else {
                continue;
            };
            if let Err(reason) = move_owner(
                world,
                &mut owner,
                target,
                key,
                true,
                far.package
                    .as_ref()
                    .and_then(|p| p.placed_reference(id.1))
                    .map(|p| p.pose),
            ) {
                ownership_failed(world, far, &owner, reason);
            }
            far.put_owner(id, owner);
        }
    });
}

pub(super) fn queue_release(commands: &mut Commands, root: Entity) {
    commands.queue(move |world: &mut World| release(world, root));
}

pub(super) fn queue_adopt(commands: &mut Commands, root: Entity, key: CellKey, ids: Vec<u32>) {
    commands.queue(move |world: &mut World| {
        with_far(world, |world, far| {
            let Some(placing_world) = cell_world(key) else {
                return;
            };
            for id in ids {
                let identity = (placing_world, id);
                let Some(mut owner) = far.remove_owner(&identity) else {
                    fatal(world, "reserved source owner vanished".into());
                    continue;
                };
                owner.near_key = Some(key);
                owner.near_root = Some(root);
                if crate::portal::owned_root_role(world, root) == Some(role(far.demand))
                    && role(far.demand) != CellRole::Hidden
                    && let Err(reason) = move_owner(
                        world,
                        &mut owner,
                        root,
                        key,
                        false,
                        far.package
                            .as_ref()
                            .and_then(|p| p.placed_reference(id))
                            .map(|p| p.pose),
                    )
                {
                    ownership_failed(world, far, &owner, reason);
                }
                far.put_owner(identity, owner);
            }
            register_near(world, far, root, key);
            reconcile_near(world, far);
            refresh_holders(world, far);
        })
    });
}

fn register_near(world: &mut World, far: &mut FarStock, root: Entity, key: CellKey) {
    far.near_roots.insert(key, root);
    far.near_roles.remove(&root);
    let Some(placing_world) = cell_world(key) else {
        return;
    };
    let children: Vec<_> = world
        .get::<Children>(root)
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for entity in children {
        let Some(form) = world.get::<FormId>(entity) else {
            continue;
        };
        let id = (placing_world, form.0);
        // Ordinary admission can occur without camera/report movement. Enroll
        // Complete arrivals here rather than waiting for the planner's next scan.
        if !far.desired.contains_key(&id)
            && let (Some(package), Some(demand), Some(cell)) =
                (&far.package, far.demand, world.get::<CellRef>(entity))
            && placing_world == demand.world
            && let Some(key) =
                complete_source_key(package, demand, far.stream_radius, form.0, cell.0)
        {
            far.desired.insert(id, key);
        }
        if !far.desired.contains_key(&id) {
            continue;
        }
        if let Some(existing) = far.owners.get(&id) {
            if existing.entity != entity {
                fatal(
                    world,
                    "ordinary source spawn duplicated an exact distant owner".into(),
                );
            }
            continue;
        }
        far.put_owner(
            id,
            Owner {
                entity,
                parent: root,
                key,
                near_key: Some(key),
                near_root: Some(root),
                far: false,
                row: None,
                physics_requested: super::builds_world_colliders(world.resource::<EngineConfig>()),
            },
        );
    }
}

pub(super) fn queue_revive(commands: &mut Commands, root: Entity, key: CellKey) {
    commands.queue(move |world: &mut World| {
        with_far(world, |world, far| {
            far.near_roots.insert(key, root);
            far.near_roles.remove(&root);
            let ids: Vec<_> = far
                .by_near_key
                .get(&key)
                .into_iter()
                .flat_map(|ids| ids.iter().copied())
                .collect();
            for id in ids {
                let mut owner = far.remove_owner(&id).unwrap();
                owner.near_root = Some(root);
                far.put_owner(id, owner);
            }
            reconcile_near(world, far);
        })
    });
}

fn source_pose(far: &FarStock, id: u32) -> Option<crate::cluster::reader::ReferencePose> {
    far.package
        .as_ref()
        .and_then(|p| p.placed_reference(id))
        .map(|p| p.pose)
}

fn refresh_holders(world: &mut World, far: &mut FarStock) {
    // Read the actual post-transfer children before selecting roots for role writes.
    far.holders.retain(|_, root| {
        let empty = world.get::<Children>(*root).is_none_or(|c| c.is_empty());
        if empty && let Ok(entity) = world.get_entity_mut(*root) {
            entity.despawn();
        }
        !empty
    });
    for &root in far.holders.values() {
        let visible = holder_visibility(role(far.demand));
        world.entity_mut(root).insert(visible);
        if let Err(reason) = sync_holder_role(world, root, role(far.demand)) {
            fatal(world, reason);
        }
    }
}

fn reconcile_near(world: &mut World, far: &mut FarStock) {
    let roots: Vec<_> = far
        .near_roots
        .iter()
        .map(|(&key, &root)| (key, root))
        .collect();
    let drawing_role = role(far.demand);
    let mut moved = false;
    for (key, root) in roots {
        if world
            .get::<crate::world::components::StreamedCellRoot>(root)
            .is_none()
        {
            far.near_roots.remove(&key);
            far.near_roles.remove(&root);
            continue;
        }
        let Some(current) = crate::portal::owned_root_role(world, root) else {
            continue;
        };
        if far.near_roles.insert(root, current) == Some(current) {
            continue;
        }
        if drawing_role == CellRole::Hidden {
            continue;
        }
        let ids: Vec<_> = far
            .ids_for_root(root)
            .filter(|id| far.desired.contains_key(id))
            .collect();
        for id in ids {
            let owner = far.owners.get(&id).unwrap();
            let target_near = current == drawing_role;
            if (target_near && owner.parent == root) || (!target_near && owner.far) {
                continue;
            }
            let target = if target_near {
                Ok((root, key))
            } else {
                let key = far.desired[&id];
                holder(world, far, key).map(|root| (root, key))
            };
            let (target, target_key) = match target {
                Ok(target) => target,
                Err(reason) => {
                    let owner = far.owners[&id].clone();
                    ownership_failed(world, far, &owner, reason);
                    continue;
                }
            };
            let mut owner = far.remove_owner(&id).unwrap();
            if let Err(reason) = move_owner(
                world,
                &mut owner,
                target,
                target_key,
                !target_near,
                source_pose(far, id.1),
            ) {
                ownership_failed(world, far, &owner, reason);
            } else {
                moved = true;
            }
            far.put_owner(id, owner);
        }
    }
    if moved {
        refresh_holders(world, far);
    }
}

fn retire_sources(
    world: &mut World,
    package: Option<Arc<CertifiedPackage>>,
    mut owners: HashMap<Identity, Owner>,
    holders: HashMap<CellKey, Entity>,
) {
    let mut restored = HashMap::new();
    for (id, owner) in &mut owners {
        if !owner.far {
            continue;
        }
        let near = owner.near_root.zip(owner.near_key).filter(|(root, _)| {
            world
                .get::<crate::world::components::StreamedCellRoot>(*root)
                .is_some()
        });
        let Some((root, key)) = near else {
            if let Ok(entity) = world.get_entity_mut(owner.entity) {
                entity.despawn();
            }
            continue;
        };
        let pose = package
            .as_ref()
            .and_then(|p| p.placed_reference(id.1))
            .map(|p| p.pose);
        if let Err(reason) = move_owner(world, owner, root, key, false, pose) {
            with_far(world, |world, far| {
                ownership_failed(world, far, owner, reason)
            });
            return;
        }
        restored.insert(key, root);
    }
    for (_, root) in holders {
        if let Ok(entity) = world.get_entity_mut(root) {
            entity.despawn();
        }
    }
    with_far(world, |world, far| {
        for (key, root) in restored {
            register_near(world, far, root, key);
        }
        reconcile_near(world, far);
    });
}

#[cfg(test)]
#[path = "far_stock_tests.rs"]
mod tests;
