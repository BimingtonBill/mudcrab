//! The seamless portal's destination requests: the view the portal camera renders (a doorway shown
//! on screen, or the pre-warm's destination view) is a view whose drawable state a replacement
//! owner has to vouch for, so it asks the destination readiness protocol
//! ([`crate::swap_bridge::destination`]) about it.
//!
//! **Only when a policy exists.** Everything here runs behind `resource_exists::<RequiredHandoff>`:
//! a run without a replacement representation adds one scheduled system whose condition is false,
//! makes no request, and settles the pre-warm on the pipeline cache alone, exactly as before.
//!
//! One request per rendered portal view: the view is the portal camera (its retained view entity and
//! an incarnation that changes when the camera is respawned), the world and coverage are those of
//! the **one door** being rendered (never the held aggregate of [`PortalState::destination`], which
//! can include a held door's cells and the pre-warm's cells at once), the passes are the ones the
//! camera actually runs, and the handoff is [`Handoff::Required`].

use super::{LoadDoorQuery, PortalCamera, PortalPrewarm, PortalState};
use crate::{
    doors::DoorCrossed,
    swap_bridge::destination::{
        DestinationKey, DestinationReports, DestinationRequest, DestinationRequests,
        DestinationState, Failure, Generations, Handoff, InvalidationCause, PassFamilies,
        PassFamily, RequestId, RequiredHandoff, ViewKey, WorldId,
    },
    transition::destination_keys,
    world::database::CellKey,
};
use bevy::{
    camera::visibility::RenderLayers,
    core_pipeline::prepass::{DepthPrepass, MotionVectorPrepass, NormalPrepass},
    ecs::system::SystemParam,
    prelude::*,
    render::{sync_world::MainEntity, view::RetainedViewEntity},
};
use std::sync::Arc;

/// The policy and the generations, for systems that already take many parameters: whether a
/// replacement policy exists, and whether a door's portal view was proved for the destination's
/// current generation.
#[derive(SystemParam)]
pub(super) struct ProofParams<'w> {
    policy: Option<Res<'w, RequiredHandoff>>,
    generations: Option<Res<'w, Generations>>,
    reports: Option<Res<'w, DestinationReports>>,
    pending: Option<Res<'w, crate::transition::PendingCrossing>>,
}

impl ProofParams<'_> {
    pub(super) fn required(&self) -> bool {
        self.policy.is_some()
    }

    /// Re-evaluate a cached gate against the reports and generation visible now.
    /// Missing facts stay waiting; cached Ready/Failed values are never reused.
    pub(super) fn refresh_gate(&self, entry: &GateEntry) -> Option<GateEntry> {
        self.reports
            .as_deref()
            .zip(self.generations.as_deref())
            .map(|(reports, generations)| GateEntry {
                door: entry.door,
                key: entry.key,
                gate: gate_of(reports, &entry.key, generations),
            })
    }

    /// Only a replacement-policy crossing owns a dedicated live landing proof view.
    pub(super) fn pending_door(&self) -> Option<Entity> {
        self.required()
            .then(|| self.pending.as_deref()?.door())
            .flatten()
    }

    pub(super) fn prospective_landing(
        &self,
        door: (Entity, &crate::doors::LoadDoor),
        anchor: Option<&crate::doors::DoorAnchor>,
        state: Option<&crate::doors::DoorState>,
        source: &GlobalTransform,
        camera: &Transform,
        origin: IVec2,
    ) -> Option<crate::transition::ProspectiveLanding> {
        if !self.required() {
            return None;
        }
        self.pending
            .as_deref()?
            .prospective_landing(door, anchor, state, source, camera, origin)
    }

    /// Whether the failure recorded for `door` is still current: the destination's generation has
    /// not moved since, so nothing about what it is made of has changed.
    pub(super) fn failure_stands(
        &self,
        prewarm: &PortalPrewarm,
        door: Entity,
        destination: &crate::doors::DoorDestination,
    ) -> bool {
        let Some(target) = crate::transition::SpaceTarget::of_destination(destination) else {
            return false;
        };
        let world = match target {
            crate::transition::SpaceTarget::Interior(cell) => WorldId::interior(cell),
            crate::transition::SpaceTarget::Exterior(world) => WorldId::exterior(world),
        };
        self.generations
            .as_deref()
            .is_some_and(|generations| prewarm.failure_stands(door, world, generations))
    }
}

/// The current report of the portal view's door, with the key it was read for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GateEntry {
    pub door: Entity,
    pub gate: ViewGate,
    /// The exact key (view, world, generation) the gate was read for.
    pub key: DestinationKey,
}

/// What the pre-warm settles on, from the current destination report of its door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ViewGate {
    /// The report is not ready (any waiting or stale state); the text is its description.
    Waiting(String),
    /// The report is ready for the current key.
    Ready,
    /// The destination failed for good (a cell or asset failed, or the owner's handoff failed).
    Failed(String),
}

/// What the pre-warm's settlement does this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Settle {
    /// The pipeline cache decides, as it always did.
    ByPipelineCache,
    /// The report is not ready: stay up, whatever the pipeline cache says.
    Wait,
    /// The destination failed: end the warm view without calling the door warmed.
    Fail,
}

/// How the pre-warm settles, from whether the policy exists and the gate of the door it is warming.
/// A policy with no gate yet (the request is made after the pre-warm in the frame) waits; a
/// failure is never a ready.
pub(super) fn settle(policy: bool, gate: Option<&ViewGate>) -> Settle {
    if !policy {
        return Settle::ByPipelineCache;
    }
    match gate {
        Some(ViewGate::Ready) => Settle::ByPipelineCache,
        Some(ViewGate::Failed(_)) => Settle::Fail,
        Some(ViewGate::Waiting(_)) | None => Settle::Wait,
    }
}

/// The door whose destination the portal camera renders this frame: the pre-warm's door while its
/// warm view runs, else the doorway the portal draws through, but only while the portal camera is
/// active. A doorway the portal picked and left off screen has no rendering camera, and the
/// pre-warm may be warming another door in the same frame.
pub(super) fn rendered_door(
    warming: Option<Entity>,
    doorway: Option<Entity>,
    camera_active: bool,
) -> Option<Entity> {
    warming.or_else(|| doorway.filter(|_| camera_active))
}

/// What a request for the rendered view should say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Wanted {
    pub door: Entity,
    pub world: WorldId,
    pub view: ViewKey,
    pub passes: PassFamilies,
    pub coverage: Arc<[CellKey]>,
    /// Configured shadow-light ownership for this exact camera's layers. Stable
    /// entity identities, not counts or asynchronous render-world arrivals.
    shadow_directionals: Vec<Entity>,
}

/// The request this module holds, if any.
#[derive(Resource, Default, Debug)]
pub(super) struct PortalViewRequest {
    current: Option<(RequestId, Wanted)>,
}

/// What to do about the request this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Plan {
    /// Nothing is rendered and nothing is held.
    Nothing,
    /// The held request already says it.
    Keep,
    /// First request for a rendered view.
    Create(Wanted),
    /// Same world, something else changed (door, view, passes, coverage): the request is updated
    /// and the world's generation moves.
    Update(Wanted),
    /// A different world: the old request is withdrawn and a new one made.
    Replace(Wanted),
    /// Nothing is rendered any more.
    Withdraw,
}

pub(super) fn plan(current: Option<&Wanted>, wanted: Option<Wanted>) -> Plan {
    match (current, wanted) {
        (None, None) => Plan::Nothing,
        (Some(_), None) => Plan::Withdraw,
        (None, Some(wanted)) => Plan::Create(wanted),
        (Some(current), Some(wanted)) if *current == wanted => Plan::Keep,
        (Some(current), Some(wanted)) if current.world == wanted.world => Plan::Update(wanted),
        (Some(_), Some(wanted)) => Plan::Replace(wanted),
    }
}

/// The gate from the current report of `key`.
pub(super) fn gate_of(
    reports: &DestinationReports,
    key: &DestinationKey,
    generations: &Generations,
) -> ViewGate {
    if reports.is_ready(key, generations) {
        return ViewGate::Ready;
    }
    if key.generation != generations.current(key.world)
        || reports.get(key).is_some_and(|report| {
            reports
                .invalidation(key.world)
                .is_some_and(|at| report.evaluated_render_frame < at)
        })
    {
        return ViewGate::Waiting(reports.describe(key));
    }
    match reports.get(key).map(|report| &report.state) {
        Some(DestinationState::Failed(failure)) => ViewGate::Failed(match failure {
            Failure::CellFailed(cell) => format!("cell {cell:?} failed"),
            Failure::AssetFailed(cell) => format!("an asset in cell {cell:?} failed"),
            Failure::HandoffFailed(reason) => format!("the handoff failed: {reason}"),
        }),
        _ => ViewGate::Waiting(reports.describe(key)),
    }
}

fn withdraw(
    held: &mut PortalViewRequest,
    requests: &mut DestinationRequests,
    reports: &mut DestinationReports,
) {
    if let Some((id, _)) = held.current.take() {
        if let Some(key) = requests
            .iter()
            .find(|(held_id, _)| *held_id == id)
            .map(|(_, request)| request.key)
        {
            reports.remove(&key);
        }
        requests.withdraw(id);
    }
}

fn create(
    wanted: Wanted,
    held: &mut PortalViewRequest,
    requests: &mut DestinationRequests,
    generations: &mut Generations,
) {
    let generation = generations.bump(wanted.world, InvalidationCause::Composition);
    let id = requests.request(DestinationRequest {
        key: DestinationKey {
            view: wanted.view,
            world: wanted.world,
            generation,
        },
        passes: wanted.passes,
        coverage: wanted.coverage.clone(),
        handoff: Handoff::Required,
    });
    held.current = Some((id, wanted));
}

/// Applies `plan` to the protocol's resources and the request this module holds.
pub(super) fn apply(
    plan: Plan,
    held: &mut PortalViewRequest,
    requests: &mut DestinationRequests,
    reports: &mut DestinationReports,
    generations: &mut Generations,
) {
    match plan {
        Plan::Nothing | Plan::Keep => {}
        Plan::Withdraw => withdraw(held, requests, reports),
        Plan::Create(wanted) => create(wanted, held, requests, generations),
        Plan::Replace(wanted) => {
            withdraw(held, requests, reports);
            create(wanted, held, requests, generations);
        }
        Plan::Update(wanted) => {
            if let Some((id, current)) = held.current.as_mut() {
                // The request does not name the door: two doors can share a world, a view, passes
                // and coverage (exterior doors into the same grid) while their windows are
                // different images. A change of door alone therefore moves the generation here,
                // explicitly, so a report made for the old door cannot be read as the new door's.
                // The same applies to this view's enabled directional-shadow domain:
                // the public request fields do not encode those light identities.
                let owner_composition_changed = current.door != wanted.door
                    || current.shadow_directionals != wanted.shadow_directionals;
                requests.update(
                    *id,
                    generations,
                    wanted.view,
                    wanted.passes,
                    wanted.coverage.clone(),
                    Handoff::Required,
                );
                if owner_composition_changed {
                    if let Some(old) = requests
                        .iter()
                        .find(|(held_id, _)| *held_id == *id)
                        .map(|(_, request)| request.key)
                    {
                        reports.remove(&old);
                    }
                    generations.bump(wanted.world, InvalidationCause::Composition);
                }
                *current = wanted;
            }
        }
    }
}

/// Makes, updates, rebases and withdraws the request for the portal camera's view, and publishes
/// the gate the pre-warm settles on. Runs after the portal chain has decided this frame's door and
/// camera state.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn drive_portal_destination(
    state: Res<PortalState>,
    mut prewarm: ResMut<PortalPrewarm>,
    camera: Query<
        (
            Entity,
            &Camera,
            Has<DepthPrepass>,
            Has<NormalPrepass>,
            Has<MotionVectorPrepass>,
            Option<&RenderLayers>,
        ),
        With<PortalCamera>,
    >,
    doors: LoadDoorQuery,
    lights: Query<(Entity, &DirectionalLight, Option<&RenderLayers>)>,
    mut held: ResMut<PortalViewRequest>,
    mut requests: ResMut<DestinationRequests>,
    mut reports: ResMut<DestinationReports>,
    mut generations: ResMut<Generations>,
    mut crossed: MessageReader<DoorCrossed>,
) {
    // A proof belongs to the portal's view of a door. Crossing makes the main camera's view the one
    // that matters, so nothing proved for the portal view carries over into the new space.
    if crossed.read().count() > 0 {
        prewarm.proved.clear();
    }
    let wanted = camera.single().ok().and_then(
        |(camera_entity, camera, depth, normal, motion, camera_layers)| {
            let door = rendered_door(
                prewarm.rendering_door(),
                state.open_doorway().map(|doorway| doorway.door),
                camera.is_active,
            )?;
            let (_, _, _, load_door, _, _, _, anchor) = doors.get(door).ok()?;
            let coverage = destination_keys(&load_door.destination, anchor);
            if coverage.is_empty() {
                return None;
            }
            let world = match (
                load_door.destination.interior_cell_id,
                load_door.destination.worldspace_id,
            ) {
                (Some(cell), _) => WorldId::interior(cell),
                (None, Some(worldspace)) => WorldId::exterior(worldspace),
                (None, None) => return None,
            };
            let mut passes = PassFamilies::NONE.with(PassFamily::Forward);
            if depth {
                passes = passes.with(PassFamily::DepthPrepass);
            }
            if normal {
                passes = passes.with(PassFamily::NormalPrepass);
            }
            if motion {
                passes = passes.with(PassFamily::MotionPrepass);
            }
            if lights.iter().any(|(_, light, _)| light.shadow_maps_enabled) {
                passes = passes.with(PassFamily::Shadow);
            }
            let view_layers = camera_layers.unwrap_or_default();
            let mut shadow_directionals: Vec<_> = lights
                .iter()
                .filter(|(_, light, layers)| {
                    light.shadow_maps_enabled && layers.unwrap_or_default().intersects(view_layers)
                })
                .map(|(entity, _, _)| entity)
                .collect();
            // Bevy does not exclude illuminance==0 from enabled shadow ownership.
            // Its per-view light preparation filters the flag and RenderLayers;
            // actual visibility, selection caps and pipeline arrival remain proof facts.
            shadow_directionals.sort_unstable();
            Some(Wanted {
                door,
                world,
                view: ViewKey {
                    view: RetainedViewEntity::new(MainEntity::from(camera_entity), None, 0),
                    // Retained entity stays the actual camera. Projection modes are
                    // distinct proof views even though that camera resource is reused.
                    incarnation: camera_entity.generation().to_bits()
                        ^ u32::from(prewarm.pending_landing),
                },
                passes,
                coverage: Arc::from(coverage),
                shadow_directionals,
            })
        },
    );
    let step = plan(held.current.as_ref().map(|(_, wanted)| wanted), wanted);
    apply(
        step,
        &mut held,
        &mut requests,
        &mut reports,
        &mut generations,
    );
    prewarm.gate = held.current.as_ref().and_then(|(id, wanted)| {
        let key = requests.rebase(*id, &generations)?;
        Some(GateEntry {
            door: wanted.door,
            gate: gate_of(&reports, &key, &generations),
            key,
        })
    });
    // The proof follows the gate every frame: Ready records the exact key, anything else (waiting,
    // failed, stale, revoked) removes the door's proof, and only the door being requested keeps
    // one, so a withdrawn or replaced request leaves nothing behind.
    match prewarm.gate.clone() {
        Some(entry) if entry.gate == ViewGate::Ready => {
            prewarm.proved.insert(entry.door, entry.key);
        }
        Some(entry) => {
            prewarm.proved.remove(&entry.door);
        }
        None => {}
    }
    let requested = held.current.as_ref().map(|(_, wanted)| wanted.door);
    prewarm.proved.retain(|door, _| Some(*door) == requested);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::swap_bridge::destination::{DestinationReport, RenderFrame};

    fn entity(bits: u32) -> Entity {
        Entity::from_raw_u32(bits).unwrap()
    }

    fn wanted(door: u32, world: u64, cells: &[u32]) -> Wanted {
        Wanted {
            door: entity(door),
            world: WorldId(world),
            view: ViewKey {
                view: RetainedViewEntity::new(MainEntity::from(entity(99)), None, 0),
                incarnation: 0,
            },
            passes: PassFamilies::NONE.with(PassFamily::Forward),
            shadow_directionals: Vec::new(),
            coverage: Arc::from(
                cells
                    .iter()
                    .map(|id| CellKey::Interior(*id))
                    .collect::<Vec<_>>(),
            ),
        }
    }

    /// The warm view's door wins while it runs; a drawn doorway counts only with an active camera,
    /// so an off-screen doorway (camera inactive) renders nothing and its request goes away.
    #[test]
    fn the_rendered_door_follows_the_cameras_activity() {
        let (warm, shown) = (Some(entity(1)), Some(entity(2)));
        assert_eq!(rendered_door(warm, None, true), warm);
        assert_eq!(rendered_door(None, shown, true), shown);
        assert_eq!(
            rendered_door(None, shown, false),
            None,
            "picked, off screen"
        );
        assert_eq!(
            rendered_door(warm, shown, false),
            warm,
            "the pre-warm may warm another door while a picked doorway is off screen"
        );
        assert_eq!(rendered_door(None, None, true), None);
    }

    #[test]
    fn the_plan_follows_the_wanted_view() {
        let a = wanted(1, 7, &[1]);
        assert_eq!(plan(None, None), Plan::Nothing);
        assert_eq!(plan(Some(&a), None), Plan::Withdraw);
        assert_eq!(plan(None, Some(a.clone())), Plan::Create(a.clone()));
        assert_eq!(plan(Some(&a), Some(a.clone())), Plan::Keep);
        let other_door = wanted(2, 7, &[2]);
        assert_eq!(
            plan(Some(&a), Some(other_door.clone())),
            Plan::Update(other_door)
        );
        let other_world = wanted(3, 8, &[3]);
        assert_eq!(
            plan(Some(&a), Some(other_world.clone())),
            Plan::Replace(other_world)
        );
    }

    fn resources() -> (
        PortalViewRequest,
        DestinationRequests,
        DestinationReports,
        Generations,
    ) {
        Default::default()
    }

    /// Create, keep, update, replace and withdraw on the real protocol resources: a request exists
    /// only while a view is rendered, it requires the handoff, a change moves the generation and a
    /// withdrawal removes the request and its report.
    #[test]
    fn the_request_follows_the_rendered_view_and_leaves_with_it() {
        let (mut held, mut requests, mut reports, mut generations) = resources();
        let a = wanted(1, 7, &[1]);
        apply(
            Plan::Create(a.clone()),
            &mut held,
            &mut requests,
            &mut reports,
            &mut generations,
        );
        let (id, request) = requests
            .iter()
            .next()
            .map(|(id, r)| (id, r.clone()))
            .unwrap();
        assert_eq!(request.handoff, Handoff::Required);
        assert_eq!(request.key.world, WorldId(7));
        let first = request.key.generation;
        assert_eq!(generations.current(WorldId(7)), first);

        apply(
            Plan::Keep,
            &mut held,
            &mut requests,
            &mut reports,
            &mut generations,
        );
        assert_eq!(generations.current(WorldId(7)), first, "keep bumps nothing");

        let b = wanted(2, 7, &[2]);
        apply(
            Plan::Update(b.clone()),
            &mut held,
            &mut requests,
            &mut reports,
            &mut generations,
        );
        assert!(
            generations.current(WorldId(7)) > first,
            "a changed view moves the generation"
        );
        assert_eq!(requests.iter().count(), 1);
        assert_eq!(requests.iter().next().unwrap().0, id, "updated in place");

        reports.set(DestinationReport {
            key: requests.rebase(id, &generations).unwrap(),
            evaluated_render_frame: RenderFrame(1),
            state: DestinationState::Ready,
        });
        let c = wanted(3, 8, &[3]);
        apply(
            Plan::Replace(c.clone()),
            &mut held,
            &mut requests,
            &mut reports,
            &mut generations,
        );
        assert_eq!(requests.iter().count(), 1);
        assert_eq!(requests.iter().next().unwrap().1.key.world, WorldId(8));

        apply(
            Plan::Withdraw,
            &mut held,
            &mut requests,
            &mut reports,
            &mut generations,
        );
        assert!(requests.is_empty());
        assert!(held.current.is_none());
    }

    /// A change of door alone (same world, view, passes and coverage) is still a different image:
    /// the generation moves and the old door's ready report cannot satisfy the new door.
    #[test]
    fn a_door_only_change_does_not_inherit_the_old_doors_ready_report() {
        let (mut held, mut requests, mut reports, mut generations) = resources();
        let a = wanted(1, 7, &[1]);
        let mut b = a.clone();
        b.door = entity(2);
        assert_eq!(plan(Some(&a), Some(b.clone())), Plan::Update(b.clone()));
        apply(
            Plan::Create(a),
            &mut held,
            &mut requests,
            &mut reports,
            &mut generations,
        );
        let id = requests.iter().next().unwrap().0;
        let old_key = requests.rebase(id, &generations).unwrap();
        reports.set(DestinationReport {
            key: old_key,
            evaluated_render_frame: RenderFrame(1),
            state: DestinationState::Ready,
        });
        assert_eq!(gate_of(&reports, &old_key, &generations), ViewGate::Ready);

        apply(
            Plan::Update(b),
            &mut held,
            &mut requests,
            &mut reports,
            &mut generations,
        );
        let new_key = requests.rebase(id, &generations).unwrap();
        assert!(
            new_key.generation > old_key.generation,
            "the generation moved"
        );
        assert!(
            matches!(
                gate_of(&reports, &new_key, &generations),
                ViewGate::Waiting(_)
            ),
            "the new door starts unproved"
        );
        assert!(
            !reports.is_ready(&old_key, &generations),
            "and the old key is no longer current"
        );
    }

    /// The gate is Ready only for a ready report of the current generation; a failure is explicit;
    /// anything else waits.
    #[test]
    fn the_gate_reads_the_current_report() {
        let mut generations = Generations::default();
        let world = WorldId(7);
        let generation = generations.bump(world, InvalidationCause::Composition);
        let key = DestinationKey {
            view: wanted(1, 7, &[1]).view,
            world,
            generation,
        };
        let mut reports = DestinationReports::default();
        assert!(matches!(
            gate_of(&reports, &key, &generations),
            ViewGate::Waiting(_)
        ));
        reports.set(DestinationReport {
            key,
            evaluated_render_frame: RenderFrame(1),
            state: DestinationState::Ready,
        });
        assert_eq!(gate_of(&reports, &key, &generations), ViewGate::Ready);
        generations.bump(world, InvalidationCause::Candidate);
        assert!(
            matches!(gate_of(&reports, &key, &generations), ViewGate::Waiting(_)),
            "a moved generation voids the ready report"
        );
        let key = DestinationKey {
            generation: generations.current(world),
            ..key
        };
        reports.set(DestinationReport {
            key,
            evaluated_render_frame: RenderFrame(2),
            state: DestinationState::Failed(Failure::AssetFailed(CellKey::Interior(1))),
        });
        assert!(matches!(
            gate_of(&reports, &key, &generations),
            ViewGate::Failed(_)
        ));
    }

    #[test]
    fn crossing_failures_require_the_exact_current_destination_and_fresh_report() {
        let door = entity(5);
        let other_door = entity(6);
        let world = WorldId::interior(9);
        let wrong_world = WorldId::exterior(9);
        let mut generations = Generations::default();
        let generation = generations.bump(world, InvalidationCause::Composition);
        let key = DestinationKey {
            view: wanted(5, 9, &[1]).view,
            world,
            generation,
        };
        let mut prewarm = PortalPrewarm {
            gate: Some(GateEntry {
                door,
                key,
                gate: ViewGate::Ready,
            }),
            ..Default::default()
        };
        let mut reports = DestinationReports::default();
        reports.set(DestinationReport {
            key,
            evaluated_render_frame: RenderFrame(10),
            state: DestinationState::Failed(Failure::AssetFailed(CellKey::Interior(9))),
        });
        assert!(prewarm.crossing_failed(door, world, &reports, &generations));
        assert!(!prewarm.crossing_failed(other_door, world, &reports, &generations));
        assert!(!prewarm.crossing_failed(door, wrong_world, &reports, &generations));
        let mut invalidations = crate::swap_bridge::destination::RenderInvalidations::default();
        invalidations.revoke(world, RenderFrame(11), InvalidationCause::Render);
        reports.mirror_invalidations(&invalidations);
        assert!(!prewarm.crossing_failed(door, world, &reports, &generations));
        assert!(matches!(
            gate_of(&reports, &key, &generations),
            ViewGate::Waiting(_)
        ));
        reports.set(DestinationReport {
            key,
            evaluated_render_frame: RenderFrame(11),
            state: DestinationState::Failed(Failure::AssetFailed(CellKey::Interior(9))),
        });
        assert!(prewarm.crossing_failed(door, world, &reports, &generations));
        assert!(matches!(
            gate_of(&reports, &key, &generations),
            ViewGate::Failed(_)
        ));
        prewarm.gate.as_mut().unwrap().gate = ViewGate::Failed("cached failure".into());
        reports.set(DestinationReport {
            key,
            evaluated_render_frame: RenderFrame(12),
            state: DestinationState::Ready,
        });
        assert!(
            !prewarm.crossing_failed(door, world, &reports, &generations),
            "cached gate is not failure authority"
        );
        reports.set(DestinationReport {
            key,
            evaluated_render_frame: RenderFrame(12),
            state: DestinationState::Failed(Failure::AssetFailed(CellKey::Interior(9))),
        });
        generations.bump(world, InvalidationCause::Candidate);
        assert!(!prewarm.crossing_failed(door, world, &reports, &generations));
        assert!(matches!(
            gate_of(&reports, &key, &generations),
            ViewGate::Waiting(_)
        ));
        prewarm.failed_doors.insert(door, (world, generation));
        assert!(
            !prewarm.crossing_failed(door, world, &reports, &generations),
            "old failure latch is stale too"
        );
        prewarm
            .failed_doors
            .insert(door, (world, generations.current(world)));
        reports.remove(&key);
        assert!(
            prewarm.crossing_failed(door, world, &reports, &generations),
            "validated terminal latch survives report withdrawal"
        );
        assert!(!prewarm.crossing_failed(door, wrong_world, &reports, &generations));
    }

    /// Without the policy the pipeline cache decides, as before. With it a missing or waiting gate
    /// holds the warm view up, a ready one hands back to the cache, and a failure ends the view
    /// without a ready.
    #[test]
    fn the_prewarm_settles_on_the_report_only_with_a_policy() {
        let waiting = ViewGate::Waiting("x".into());
        let failed = ViewGate::Failed("y".into());
        for gate in [None, Some(&waiting), Some(&failed), Some(&ViewGate::Ready)] {
            assert_eq!(settle(false, gate), Settle::ByPipelineCache);
        }
        assert_eq!(settle(true, None), Settle::Wait);
        assert_eq!(settle(true, Some(&waiting)), Settle::Wait);
        assert_eq!(
            settle(true, Some(&ViewGate::Ready)),
            Settle::ByPipelineCache
        );
        assert_eq!(settle(true, Some(&failed)), Settle::Fail);
    }

    /// A proof or a failure stands only for the generation it was made for: a moved generation
    /// (any change of what the destination is made of) voids the proof and lets a failed door be
    /// tried again; a crossing drops every proof.
    #[test]
    fn proofs_and_failures_stand_only_for_their_generation_and_a_crossing_drops_proofs() {
        let door = entity(5);
        let world = WorldId(9);
        let mut generations = Generations::default();
        let generation = generations.bump(world, InvalidationCause::Composition);
        let key = DestinationKey {
            view: wanted(5, 9, &[1]).view,
            world,
            generation,
        };
        let mut reports = DestinationReports::default();
        reports.set(DestinationReport {
            key,
            evaluated_render_frame: RenderFrame(10),
            state: DestinationState::Ready,
        });
        let mut prewarm = PortalPrewarm::default();
        prewarm.prove_for_test(door, key);
        prewarm.failed_doors.insert(door, (world, generation));
        assert!(prewarm.is_proved(door, &reports, &generations));
        assert!(prewarm.failure_stands(door, world, &generations));

        // The render world revokes the world WITHOUT a generation bump: the cached key is the same,
        // but the report is no longer current, so the door is no longer proved.
        let mut invalidations = crate::swap_bridge::destination::RenderInvalidations::default();
        invalidations.revoke(world, RenderFrame(11), InvalidationCause::Render);
        reports.mirror_invalidations(&invalidations);
        assert_eq!(generations.current(world), generation, "no bump");
        assert!(
            !prewarm.is_proved(door, &reports, &generations),
            "a same-generation render revocation voids the proof"
        );
        reports.set(DestinationReport {
            key,
            evaluated_render_frame: RenderFrame(12),
            state: DestinationState::Ready,
        });
        assert!(
            prewarm.is_proved(door, &reports, &generations),
            "a report evaluated after the revocation proves it again"
        );
        // A withdrawn request takes its report with it.
        reports.remove(&key);
        assert!(!prewarm.is_proved(door, &reports, &generations));
        reports.set(DestinationReport {
            key,
            evaluated_render_frame: RenderFrame(13),
            state: DestinationState::Ready,
        });
        generations.bump(world, InvalidationCause::Candidate);
        assert!(
            !prewarm.is_proved(door, &reports, &generations),
            "stale proof"
        );
        assert!(
            !prewarm.failure_stands(door, world, &generations),
            "a failed door is retried once its composition changed"
        );

        let mut app = App::new();
        app.init_resource::<PortalState>()
            .init_resource::<PortalViewRequest>()
            .init_resource::<DestinationRequests>()
            .init_resource::<DestinationReports>()
            .init_resource::<Generations>()
            .add_message::<DoorCrossed>()
            .add_systems(Update, drive_portal_destination);
        let mut prewarm = PortalPrewarm::default();
        prewarm.prove_for_test(door, key);
        app.insert_resource(prewarm);
        app.world_mut().write_message(DoorCrossed {
            from_ref_id: 1,
            label: String::new(),
        });
        app.update();
        assert!(
            app.world().resource::<PortalPrewarm>().proved.is_empty(),
            "no proof carries over a crossing"
        );
    }

    #[test]
    fn directional_shadow_ownership_changes_need_fresh_ready_even_with_shadow_always_required() {
        use crate::{
            doors::{DoorDestination, LoadDoor},
            portal::PrewarmPhase,
        };
        let mut app = App::new();
        app.init_resource::<PortalState>()
            .init_resource::<PortalPrewarm>()
            .init_resource::<PortalViewRequest>()
            .init_resource::<DestinationRequests>()
            .init_resource::<DestinationReports>()
            .init_resource::<Generations>()
            .insert_resource(RequiredHandoff)
            .add_message::<DoorCrossed>()
            .add_systems(Update, drive_portal_destination);
        let door = app
            .world_mut()
            .spawn((
                LoadDoor {
                    ref_id: 1,
                    destination: DoorDestination {
                        destination_ref_id: 2,
                        interior_cell_id: Some(77),
                        worldspace_id: None,
                        arrival_position: [0.0; 3],
                        arrival_rotation: [0.0; 3],
                    },
                    label: String::new(),
                    auto_load: false,
                    outward: None,
                },
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                PortalCamera,
                Camera {
                    is_active: true,
                    ..default()
                },
                RenderLayers::layer(2),
            ))
            .id();
        let main_sun = app
            .world_mut()
            .spawn(DirectionalLight {
                shadow_maps_enabled: true,
                illuminance: 10000.0,
                ..default()
            })
            .id(); // Missing layers is the engine's default layer0.
        let portal_sun = app
            .world_mut()
            .spawn((
                DirectionalLight {
                    shadow_maps_enabled: false,
                    illuminance: 0.0,
                    ..default()
                },
                RenderLayers::layer(2),
            ))
            .id();
        let unrelated = app
            .world_mut()
            .spawn((
                DirectionalLight {
                    shadow_maps_enabled: true,
                    ..default()
                },
                RenderLayers::layer(7),
            ))
            .id();
        {
            let mut prewarm = app.world_mut().resource_mut::<PortalPrewarm>();
            prewarm.phase = PrewarmPhase::Warming;
            prewarm.door = Some(door);
        }
        let request = |app: &App| {
            app.world()
                .resource::<DestinationRequests>()
                .iter()
                .next()
                .unwrap()
                .1
                .clone()
        };
        let prove = |app: &mut App| {
            let key = app
                .world()
                .resource::<DestinationRequests>()
                .iter()
                .next()
                .unwrap()
                .1
                .key;
            app.world_mut()
                .resource_mut::<DestinationReports>()
                .set(DestinationReport {
                    key,
                    evaluated_render_frame: RenderFrame(1),
                    state: DestinationState::Ready,
                });
            app.update();
        };
        let is_proved = |app: &App| {
            app.world().resource::<PortalPrewarm>().is_proved(
                door,
                app.world().resource::<DestinationReports>(),
                app.world().resource::<Generations>(),
            )
        };
        app.update();
        let empty = request(&app);
        assert!(
            empty.passes.contains(PassFamily::Shadow),
            "main sun keeps Shadow required before portal sun enables"
        );
        assert!(
            app.world()
                .resource::<PortalViewRequest>()
                .current
                .as_ref()
                .unwrap()
                .1
                .shadow_directionals
                .is_empty()
        );
        prove(&mut app);
        assert!(is_proved(&app));
        app.world_mut()
            .get_mut::<DirectionalLight>(unrelated)
            .unwrap()
            .shadow_maps_enabled = false;
        app.update();
        assert_eq!(
            request(&app).key,
            empty.key,
            "other-layer changes do not own this view"
        );
        // Enabling a zero-illuminance light STILL adds its shadow ownership in Bevy.
        app.world_mut()
            .get_mut::<DirectionalLight>(portal_sun)
            .unwrap()
            .shadow_maps_enabled = true;
        app.update();
        let enabled = request(&app);
        assert_eq!(enabled.passes, empty.passes);
        assert!(enabled.key.generation > empty.key.generation);
        assert_eq!(
            app.world()
                .resource::<PortalViewRequest>()
                .current
                .as_ref()
                .unwrap()
                .1
                .shadow_directionals,
            vec![portal_sun]
        );
        assert!(
            !app.world()
                .resource::<DestinationReports>()
                .is_ready(&empty.key, app.world().resource::<Generations>())
        );
        assert!(!is_proved(&app));
        assert!(matches!(
            app.world()
                .resource::<PortalPrewarm>()
                .gate
                .as_ref()
                .unwrap()
                .gate,
            ViewGate::Waiting(_)
        ));
        for _ in 0..3 {
            app.update();
            assert_eq!(request(&app).key, enabled.key);
        }
        app.world_mut()
            .get_mut::<DirectionalLight>(portal_sun)
            .unwrap()
            .illuminance = 10000.0;
        app.update();
        assert_eq!(
            request(&app).key,
            enabled.key,
            "brightness is not shadow-domain membership"
        );
        prove(&mut app);
        assert!(is_proved(&app), "fresh exact report works");

        // Same COUNT, different light ENTITY: count-only ownership would wrongly
        // retain the previous light's Ready report.
        let replacement = app
            .world_mut()
            .spawn((
                DirectionalLight {
                    shadow_maps_enabled: true,
                    ..default()
                },
                RenderLayers::layer(2),
            ))
            .id();
        app.world_mut()
            .get_mut::<DirectionalLight>(portal_sun)
            .unwrap()
            .shadow_maps_enabled = false;
        app.update();
        let swapped = request(&app);
        assert_eq!(swapped.passes, enabled.passes);
        assert!(swapped.key.generation > enabled.key.generation);
        assert_eq!(
            app.world()
                .resource::<PortalViewRequest>()
                .current
                .as_ref()
                .unwrap()
                .1
                .shadow_directionals,
            vec![replacement]
        );
        assert!(!is_proved(&app));
        app.world_mut()
            .get_mut::<DirectionalLight>(portal_sun)
            .unwrap()
            .shadow_maps_enabled = true;
        app.update();
        let mut expected = vec![portal_sun, replacement];
        expected.sort_unstable();
        assert_eq!(
            app.world()
                .resource::<PortalViewRequest>()
                .current
                .as_ref()
                .unwrap()
                .1
                .shadow_directionals,
            expected
        );
        let both = request(&app);
        app.update();
        assert_eq!(request(&app).key, both.key, "sorted domain is stable");

        // Camera None layers is also default0, so its domain becomes main_sun.
        app.world_mut().entity_mut(camera).remove::<RenderLayers>();
        app.update();
        let default_layers = request(&app);
        assert_eq!(default_layers.passes, empty.passes);
        assert!(default_layers.key.generation > both.key.generation);
        assert_eq!(
            app.world()
                .resource::<PortalViewRequest>()
                .current
                .as_ref()
                .unwrap()
                .1
                .shadow_directionals,
            vec![main_sun]
        );
        app.world_mut()
            .entity_mut(camera)
            .insert(RenderLayers::layer(0));
        app.update();
        assert_eq!(
            request(&app).key,
            default_layers.key,
            "explicit0 and absent layers have the same domain"
        );
        app.world_mut()
            .get_mut::<DirectionalLight>(unrelated)
            .unwrap()
            .shadow_maps_enabled = true;
        app.update();
        assert_eq!(request(&app).key, default_layers.key);
        prove(&mut app);
        assert!(is_proved(&app));
    }

    /// The system on a real world: a warming door makes one Required request for that door's own
    /// destination keys with the passes the camera carries; an inactive camera and an idle pre-warm
    /// withdraw it and clear the gate.
    #[test]
    fn the_system_requests_the_warmed_door_and_withdraws_when_the_view_goes_away() {
        use crate::{
            doors::{DoorDestination, LoadDoor},
            portal::PrewarmPhase,
        };
        let mut app = App::new();
        app.init_resource::<PortalState>()
            .init_resource::<PortalPrewarm>()
            .init_resource::<PortalViewRequest>()
            .init_resource::<DestinationRequests>()
            .init_resource::<DestinationReports>()
            .init_resource::<Generations>()
            .add_message::<DoorCrossed>()
            .add_systems(Update, drive_portal_destination);
        let door = app
            .world_mut()
            .spawn((
                LoadDoor {
                    ref_id: 1,
                    destination: DoorDestination {
                        destination_ref_id: 2,
                        interior_cell_id: Some(77),
                        worldspace_id: None,
                        arrival_position: [0.0; 3],
                        arrival_rotation: [0.0; 3],
                    },
                    label: String::new(),
                    auto_load: false,
                    outward: None,
                },
                GlobalTransform::default(),
                Transform::default(),
            ))
            .id();
        let camera = app
            .world_mut()
            .spawn((
                Camera {
                    is_active: true,
                    ..default()
                },
                PortalCamera,
                DepthPrepass,
            ))
            .id();
        {
            let mut prewarm = app.world_mut().resource_mut::<PortalPrewarm>();
            prewarm.phase = PrewarmPhase::Warming;
            prewarm.door = Some(door);
        }
        app.update();
        {
            let requests = app.world().resource::<DestinationRequests>();
            assert_eq!(requests.iter().count(), 1);
            let (_, request) = requests.iter().next().unwrap();
            assert_eq!(request.coverage.as_ref(), &[CellKey::Interior(77)]);
            assert_eq!(request.key.world, WorldId::interior(77));
            assert_eq!(request.handoff, Handoff::Required);
            assert!(request.passes.contains(PassFamily::Forward));
            assert!(request.passes.contains(PassFamily::DepthPrepass));
            assert!(!request.passes.contains(PassFamily::NormalPrepass));
            assert_eq!(request.key.view.incarnation, camera.generation().to_bits());
            let prewarm = app.world().resource::<PortalPrewarm>();
            assert!(
                matches!(&prewarm.gate, Some(entry) if entry.door == door && matches!(entry.gate, ViewGate::Waiting(_)))
            );
        }
        // A second frame with nothing changed keeps the same request and generation.
        let before = app
            .world()
            .resource::<Generations>()
            .current(WorldId::interior(77));
        app.update();
        assert_eq!(
            app.world().resource::<DestinationRequests>().iter().count(),
            1
        );
        assert_eq!(
            app.world()
                .resource::<Generations>()
                .current(WorldId::interior(77)),
            before
        );
        // The warm view ends: the camera goes inactive and the phase idle.
        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Camera>()
            .unwrap()
            .is_active = false;
        app.world_mut().resource_mut::<PortalPrewarm>().phase = PrewarmPhase::Idle;
        app.update();
        assert!(app.world().resource::<DestinationRequests>().is_empty());
        assert!(app.world().resource::<PortalPrewarm>().gate.is_none());
    }
}
