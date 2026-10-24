//! Common loading admission over changed render-world facts.
//!
//! This module never searches batches or pipelines. The renderer owns the exact
//! needed batch/view sets and publishes their aggregate only when those inputs
//! change. A transition generation prevents old-pose proof from releasing a new
//! teleport, including a teleport within the same worldspace.

use super::reader::CertifiedPackage;
use crate::{
    config::EngineConfig,
    pacing::world_ready_inputs_at_frame_end,
    streaming::{ActiveSpace, RenderOrigin, StreamingMetrics, StreamingWorld},
    swap_bridge::{WaitingVisibility, pending_drawn},
    world::components::StreamingCamera,
};
use bevy::prelude::*;
use std::{collections::HashMap, sync::Arc};

/// Counts for all applicable consumers of one exterior worldspace. Needed batches
/// count batch/view pairs. A missing GPU view remains a missing consumer, even if
/// none of its batches can be enumerated yet. At least one camera is required.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClusterWorldReadiness {
    pub applicable_cameras: usize,
    pub missing_views: usize,
    pub needed_batches: usize,
    pub pending_materials: usize,
    pub pending_templates: usize,
    pub pending_compute: usize,
    pub pending_pipelines: usize,
    pub failed_batches: usize,
}

impl ClusterWorldReadiness {
    fn missing(self) -> usize {
        usize::from(self.applicable_cameras == 0)
            .saturating_add(self.missing_views)
            .saturating_add(self.pending_materials)
            .saturating_add(self.pending_templates)
            .saturating_add(self.pending_compute)
            .saturating_add(self.pending_pipelines)
            .saturating_add(self.failed_batches)
    }
}

/// Main-world loading facts, scoped to the current certified package and layout.
/// Publishing is not authority to reveal stock or to hide loading during play.
#[derive(Resource)]
pub(crate) struct ClusterLoadingGate {
    package: Option<Arc<CertifiedPackage>>,
    layout: u64,
    generation: u64,
    snapshot_generation: Option<u64>,
    render_frame: Option<u64>,
    worlds: HashMap<u32, ClusterWorldReadiness>,
    teleport_cover: Option<Entity>,
    loading_pose: Option<LoadingPose>,
}

/// A borrowed diagnostic view of the cached scope and one world's aggregate.
/// Formatting visits no batches, views, or pipelines.
pub(crate) struct ClusterLoadingDiagnostics<'a> {
    gate: &'a ClusterLoadingGate,
    world: u32,
}

impl std::fmt::Debug for ClusterLoadingDiagnostics<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClusterLoadingDiagnostics")
            .field("world", &self.world)
            .field(
                "package_scope",
                &self.gate.package.as_ref().map(Arc::as_ptr),
            )
            .field("layout", &self.gate.layout)
            .field("generation", &self.gate.generation)
            .field("snapshot_generation", &self.gate.snapshot_generation)
            .field("render_frame", &self.gate.render_frame)
            .field("counts", &self.gate.worlds.get(&self.world))
            .finish()
    }
}

#[derive(Clone, Copy, PartialEq)]
struct LoadingPose {
    camera: Transform,
    origin: IVec2,
    space: ActiveSpace,
}

impl Default for ClusterLoadingGate {
    fn default() -> Self {
        Self {
            package: None,
            layout: 0,
            generation: 1,
            snapshot_generation: None,
            render_frame: None,
            worlds: HashMap::new(),
            teleport_cover: None,
            loading_pose: None,
        }
    }
}

impl ClusterLoadingGate {
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn diagnostics(&self, world: u32) -> ClusterLoadingDiagnostics<'_> {
        ClusterLoadingDiagnostics { gate: self, world }
    }

    /// The renderer captures this generation before planning the new view pose.
    /// A completed snapshot from an earlier generation cannot answer this request.
    pub(crate) fn invalidate(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.withdraw();
        self.generation
    }

    pub(crate) fn withdraw(&mut self) {
        self.snapshot_generation = None;
        self.worlds.clear();
    }

    /// The producer must check package/layout against the current main-world
    /// scope before publishing. `generation` belongs to the evaluated render
    /// snapshot, never the generation read later when extracting its result.
    pub(crate) fn publish(
        &mut self,
        package: &Arc<CertifiedPackage>,
        layout: u64,
        generation: u64,
        render_frame: u64,
        worlds: HashMap<u32, ClusterWorldReadiness>,
    ) -> bool {
        if generation != self.generation
            || self
                .render_frame
                .is_some_and(|previous| render_frame < previous)
        {
            return false;
        }
        if self.layout != layout
            || self
                .package
                .as_ref()
                .is_none_or(|old| !Arc::ptr_eq(old, package))
        {
            self.withdraw();
            self.package = Some(package.clone());
            self.layout = layout;
        }
        self.worlds = worlds;
        self.snapshot_generation = Some(generation);
        self.render_frame = Some(render_frame);
        true
    }

    pub(crate) fn world_ready(&self, world: u32) -> bool {
        self.owned_not_drawable(world) == 0
    }

    /// Missing dependency count, with one sentinel for absent/stale observations.
    /// Counts can overlap; this is not a distinct-object count.
    pub(crate) fn owned_not_drawable(&self, world: u32) -> usize {
        if self.snapshot_generation != Some(self.generation) {
            return 1;
        }
        self.worlds.get(&world).map_or(1, |world| world.missing())
    }

    pub(crate) fn teleport_pending(&self) -> bool {
        self.teleport_cover.is_some()
    }

    /// Called only while an explicit loading cover is pending. Ground placement
    /// and origin changes must receive render proof for their final camera pose.
    /// Recording the initial pose uses the admission generation already armed.
    pub(crate) fn observe_loading_pose(
        &mut self,
        camera: &Transform,
        origin: IVec2,
        space: ActiveSpace,
    ) {
        let pose = LoadingPose {
            camera: *camera,
            origin,
            space,
        };
        if self
            .loading_pose
            .as_ref()
            .is_some_and(|previous| *previous != pose)
        {
            self.invalidate();
        }
        self.loading_pose = Some(pose);
    }

    /// Called before applying an explicit teleport, while its destination has
    /// not yet been extracted. Ordinary camera movement never calls this.
    pub(crate) fn begin_teleport(&mut self, commands: &mut Commands) {
        self.invalidate();
        self.loading_pose = None;
        if self.teleport_cover.is_none() {
            self.teleport_cover = Some(
                commands
                    .spawn((
                        Name::new("Cluster teleport loading cover"),
                        TeleportCover,
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::BLACK),
                        GlobalZIndex(i32::MAX - 1),
                    ))
                    .id(),
            );
        }
    }
}

/// Interiors have no certified cluster primitives. Their ordinary destination
/// assets and passes remain the responsibility of the destination protocol.
pub(crate) fn ready_for_active(
    config: &EngineConfig,
    space: &ActiveSpace,
    gate: Option<&ClusterLoadingGate>,
) -> bool {
    config.clusters.is_none()
        || space.interior.is_some()
        || gate
            .is_some_and(|gate| gate.world_ready(space.exterior_worldspace(config.worldspace_id)))
}

#[derive(Component)]
struct TeleportCover;

pub(crate) fn install(app: &mut App) {
    app.init_resource::<ClusterLoadingGate>()
        .add_systems(PostUpdate, finish_teleport_load);
}

pub(crate) fn teleport_controls_allowed(
    gate: Option<Res<ClusterLoadingGate>>,
    startup: Option<Res<crate::pacing::startup::StartupLoad>>,
) -> bool {
    gate.is_none_or(|gate| {
        !gate.teleport_pending() && startup.is_none_or(|startup| !startup.cover_pending())
    })
}

pub(crate) fn physics_settled(
    mode: Option<&crate::physics::MoveMode>,
    hold: Option<&crate::physics::WalkSpawnHold>,
) -> bool {
    mode != Some(&crate::physics::MoveMode::Walk) || hold.is_some_and(|hold| !hold.is_active())
}

#[allow(clippy::too_many_arguments)]
fn finish_teleport_load(
    config: Option<Res<EngineConfig>>,
    space: Option<Res<ActiveSpace>>,
    origin: Option<Res<RenderOrigin>>,
    streaming: Option<Res<StreamingWorld>>,
    metrics: Option<Res<StreamingMetrics>>,
    camera: Query<&Transform, With<StreamingCamera>>,
    waiting: WaitingVisibility,
    mut gate: ResMut<ClusterLoadingGate>,
    mut commands: Commands,
    (mode, hold): (
        Option<Res<crate::physics::MoveMode>>,
        Option<Res<crate::physics::WalkSpawnHold>>,
    ),
) {
    if !gate.teleport_pending() {
        return;
    }
    let (Some(config), Some(space), Some(origin), Some(streaming), Some(metrics)) =
        (config, space, origin, streaming, metrics)
    else {
        return;
    };
    let Ok(camera) = camera.single() else {
        return;
    };
    gate.observe_loading_pose(camera, origin.0, *space);
    if !physics_settled(mode.as_deref(), hold.as_deref()) {
        return;
    }
    let ready = world_ready_inputs_at_frame_end(
        &config,
        origin.0,
        &streaming,
        &space,
        &metrics,
        camera.translation,
        pending_drawn(&waiting),
        Some(&gate),
    )
    .is_ready();
    if ready && let Some(cover) = gate.teleport_cover.take() {
        commands.entity(cover).despawn();
    }
}

#[cfg(test)]
#[path = "loading_tests.rs"]
mod tests;
