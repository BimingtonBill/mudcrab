//! The first load of a run, unpaced (impl-1124).
//!
//! From the first frame until the world is first ready (the pacing report's own latch,
//! [`super::world_ready_at_frame_end`] held for [`super::READY_FRAMES_REQUIRED`] frames, and a
//! requested cluster package certified and loaded) the streaming pacing budgets are lifted: the
//! render-asset upload budget, the model spawn cap and the unload budgets. The configured values
//! come back on the ready frame (or once the load has made no progress for
//! [`STARTUP_LIFT_TIMEOUT_SECONDS`]). Cluster mode always holds a black cover until the required
//! batches and views are drawable; restoring pacing after a stall never releases that cover.
//! Other modes retain the optional `--startup-cover` behavior. The cover shares the door
//! crossing's upload-budget and overlay helpers ([`crate::door_crossing`]).
//! The lift is on by default. `--paced-startup` preserves configured pacing; cluster admission
//! still runs. Capture automation waits until the mandatory cluster cover has faded.

use super::{READY_FRAMES_REQUIRED, world_ready_inputs_at_frame_end};
use crate::{
    config::EngineConfig,
    door_crossing::{FADE_SECONDS, set_alpha, set_upload_budget},
    streaming::{ActiveSpace, RenderOrigin, StreamingMetrics, StreamingWorld},
    swap_bridge::{WaitingVisibility, pending_drawn},
    world::components::StreamingCamera,
};
use bevy::{prelude::*, render::render_asset::RenderAssetBytesPerFrame};

/// Longest the first load may go without any progress before the lift ends anyway, in seconds. A
/// safety net against a load that has stopped, not a limit on a long one: the portal-heavy start
/// is ready in about 5-7 s, but a run that certifies a cluster package and loads every cluster
/// batch behind the cover (`--cluster-preload-batches`) takes about two minutes, all of it moving.
/// Progress is any change in what the world-ready predicate reads, or a cluster package still
/// certifying. A run that trips this restores configured pacing. Cluster mode keeps its cover
/// until readiness is proved, even after this timeout.
pub const STARTUP_LIFT_TIMEOUT_SECONDS: f32 = 60.0;

/// Blocked admission is reported on the real clock even while loading makes progress.
const STARTUP_BLOCKED_REPORT_INTERVAL_SECONDS: f64 = 60.0;

/// The full-screen black cover the first load happens behind: spawned full black on the first frame
/// and faded out (over [`FADE_SECONDS`], the crossing's own fade) once admission succeeds. A
/// non-cluster run also retains the existing timeout recovery.
///
/// Its own node, not the crossing's [`FadeOverlay`]: that overlay is the crossing's (and this build
/// does not register the crossing), and two writers on one node would fight over its alpha.
#[derive(Component)]
pub struct StartupCover;

/// The configured pacing values the startup lift puts aside, restored as one when the world is
/// first ready.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ConfiguredPacing {
    spawns_per_frame: usize,
    lod_requests_per_frame: usize,
    unload_ms_per_frame: f64,
    unloads_per_frame: usize,
    upload_bytes_per_frame: Option<usize>,
}

/// The startup lift's own state, advanced once a frame by [`drive_startup_load`].
/// The first load made no progress in time. Triggered once, when the stall is reported, so
/// observers can say what they are waiting for at no cost on any other frame.
#[derive(Event, Debug, Clone, Copy)]
pub struct StartupLoadStalled;

/// Diagnostic-only notification for each periodic blocked-admission report.
/// Observers can explain their cached blockers without changing loading recovery.
#[derive(Event, Debug, Clone, Copy)]
pub struct StartupLoadBlocked;

#[derive(Resource, Default)]
pub(crate) struct StartupLoad {
    /// Cluster mode always covers its initial load. Other modes use the optional cover policy
    /// ([`EngineConfig::covers_startup_load`]).
    covers: bool,
    /// `Some` while the lift is in force, holding the configured values to restore.
    configured: Option<ConfiguredPacing>,
    /// Consecutive frames the world-ready predicate has held;
    /// [`crate::pacing::READY_FRAMES_REQUIRED`] of them latch it, the rule the pacing report's own
    /// first-ready latch uses.
    ready_frames: u32,
    /// Seconds since the first frame on Bevy's virtual clock (fade/stall behavior).
    elapsed: f32,
    /// Monotonic first-load measurement; virtual time clamps long loading frames.
    wall_started: Option<std::time::Instant>,
    /// Unclamped real seconds since startup, independent of loading progress.
    real_elapsed: f64,
    /// Real elapsed time of the last periodic blocked-admission report.
    last_blocked_report: f64,
    /// What the world-ready predicate read last frame, to tell a moving load from a stopped one.
    last_inputs: Option<crate::pacing::WorldReadyInputs>,
    /// Seconds since the load last made progress, for [`STARTUP_LIFT_TIMEOUT_SECONDS`].
    stalled: f32,
    /// `Some(elapsed)` once the lift is over and the cover is fading out; `None` before.
    fade: Option<f32>,
    /// The cover node, despawned when the fade ends.
    cover: Option<Entity>,
    /// The cover has faded out and gone: the lift is over for the rest of the run.
    finished: bool,
    /// A timed-out lift is restored once; cluster admission still holds its cover.
    stall_reported: bool,
}

impl StartupLoad {
    pub(crate) fn cover_pending(&self) -> bool {
        self.covers && !self.finished
    }
}

/// Lifts streaming pacing for the first load when configured. Cluster mode always holds a black
/// cover, including paced and capture runs. Other modes retain their optional cover policy
/// ([`EngineConfig::unpaced_startup`] / [`EngineConfig::covers_startup_load`]).
///
/// The lift is the crossing's own kind of trade - behind black a stutter is invisible, so the load
/// may spend its bursts - with the same mechanism, [`set_upload_budget`], extended to the two
/// streaming budgets:
///
/// * the render-asset upload budget to `None`,
/// * the model spawn cap to `0` (every ready model armed each frame),
/// * the unload budgets to `0` and `0` (the one pair that is exactly the unbudgeted path, see
///   [`EngineConfig::max_unload_ms_per_frame`]).
///
/// All three are restored to the configured values on the frame the world-ready latch fires (two
/// consecutive ready frames, the pacing report's own predicate and rule, plus the cluster load) or
/// once the load has stalled for [`STARTUP_LIFT_TIMEOUT_SECONDS`], whichever comes first. Cluster
/// mode fades and despawns its cover only after admission; other modes retain timeout recovery.
/// The latch happens once: a later loss of ready never lifts again.
pub struct StartupLoadPlugin;

impl Plugin for StartupLoadPlugin {
    fn build(&self, app: &mut App) {
        let (lifts, covers, clusters) =
            app.world()
                .get_resource::<EngineConfig>()
                .map_or((false, false, false), |config| {
                    let clusters = config.clusters.is_some();
                    (
                        config.unpaced_startup(),
                        config.covers_startup_load() || clusters,
                        clusters,
                    )
                });
        if !lifts && !clusters {
            return;
        }
        app.insert_resource(StartupLoad {
            covers,
            ..default()
        })
        .add_systems(Startup, begin_startup_load)
        .add_systems(PostUpdate, drive_startup_load);
    }
}

/// Takes the three budgets down and spawns the cover, on the first frame.
fn begin_startup_load(
    mut commands: Commands,
    mut config: ResMut<EngineConfig>,
    mut budget: Option<ResMut<RenderAssetBytesPerFrame>>,
    mut load: ResMut<StartupLoad>,
) {
    load.wall_started = Some(std::time::Instant::now());
    let configured = ConfiguredPacing {
        spawns_per_frame: config.max_model_spawns_per_frame,
        lod_requests_per_frame: config.max_lod_requests_per_frame,
        unload_ms_per_frame: config.max_unload_ms_per_frame,
        unloads_per_frame: config.max_cell_unloads_per_frame,
        upload_bytes_per_frame: config.max_upload_bytes_per_frame(),
    };
    if config.unpaced_startup() {
        config.max_model_spawns_per_frame = 0;
        config.max_lod_requests_per_frame = 0;
        config.max_unload_ms_per_frame = 0.0;
        config.max_cell_unloads_per_frame = 0;
        set_upload_budget(&mut budget, None);
        load.configured = Some(configured);
    }
    if load.covers {
        load.cover = Some(
            commands
                .spawn((
                    Name::new("Startup cover"),
                    StartupCover,
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 1.0)),
                    // Under the crossing's own fade (`i32::MAX - 1`): the more specific cover wins.
                    GlobalZIndex(i32::MAX - 2),
                ))
                .id(),
        );
    }
    if load.configured.is_some() {
        info!(
        spawns_per_frame = configured.spawns_per_frame,
        lod_requests_per_frame = configured.lod_requests_per_frame,
        unload_ms_per_frame = configured.unload_ms_per_frame,
        unloads_per_frame = configured.unloads_per_frame,
        upload_bytes_per_frame = ?configured.upload_bytes_per_frame,
        "startup load: streaming pacing lifted until the world is first ready"
        );
    } else {
        info!("startup load: cluster admission holds the cover with configured pacing");
    }
}

/// Puts the configured pacing back, as one: the two streaming budgets and the upload budget.
fn restore_startup_pacing(
    config: &mut EngineConfig,
    budget: &mut Option<ResMut<RenderAssetBytesPerFrame>>,
    configured: ConfiguredPacing,
) {
    config.max_model_spawns_per_frame = configured.spawns_per_frame;
    config.max_lod_requests_per_frame = configured.lod_requests_per_frame;
    config.max_unload_ms_per_frame = configured.unload_ms_per_frame;
    config.max_cell_unloads_per_frame = configured.unloads_per_frame;
    set_upload_budget(budget, configured.upload_bytes_per_frame);
}

/// Holds the cover, watches the world-ready predicate, restores the configured pacing on the frame
/// it latches (or at the timeout), then fades the cover out.
///
/// The predicate is read at frame end, through [`world_ready_at_frame_end`] - the same function
/// [`crate::pacing::track_world_ready`] reads it through a moment earlier in the frame's `Update`,
/// so the two latch on the same frame.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_startup_load(
    time: Res<Time>,
    real_time: Res<Time<Real>>,
    mut config: ResMut<EngineConfig>,
    origin: Res<RenderOrigin>,
    streaming: Res<StreamingWorld>,
    space: Res<ActiveSpace>,
    metrics: Res<StreamingMetrics>,
    camera: Query<&Transform, With<StreamingCamera>>,
    waiting: WaitingVisibility,
    mut overlay: Query<&mut BackgroundColor, With<StartupCover>>,
    mut budget: Option<ResMut<RenderAssetBytesPerFrame>>,
    mut commands: Commands,
    mut load: ResMut<StartupLoad>,
    (validation, scene, mut gate, mode, hold): (
        Option<Res<crate::cluster::package::PackageValidationStatus>>,
        Option<Res<crate::cluster::render::ClusterScene>>,
        Option<ResMut<crate::cluster::loading::ClusterLoadingGate>>,
        Option<Res<crate::physics::MoveMode>>,
        Option<Res<crate::physics::WalkSpawnHold>>,
    ),
) {
    if load.finished {
        return;
    }
    let delta = time.delta_secs();
    load.elapsed += delta;
    load.real_elapsed += real_time.delta_secs_f64();
    if config.clusters.is_some()
        && load.cover_pending()
        && let (Ok(camera), Some(gate)) = (camera.single(), gate.as_deref_mut())
    {
        gate.observe_loading_pose(camera, origin.0, *space);
    }
    let physics_ready = config.clusters.is_none()
        || crate::cluster::loading::physics_settled(mode.as_deref(), hold.as_deref());
    if !physics_ready
        || !crate::cluster::loading::ready_for_active(&config, &space, gate.as_deref())
    {
        // A dependency withdrawn during the fade cannot expose an unproved view.
        load.fade = None;
        set_alpha(&mut overlay, 1.0);
    }

    // The lift is over; the cover is on its way out.
    if let Some(fade) = load.fade {
        let fade = fade + delta;
        if fade >= FADE_SECONDS {
            if let Some(cover) = load.cover.take() {
                commands.entity(cover).despawn();
            }
            load.finished = true;
            info!("startup load: cover fade finished");
        } else {
            set_alpha(&mut overlay, 1.0 - fade / FADE_SECONDS);
            load.fade = Some(fade);
        }
        return;
    }

    // Loading: full black over the burst frames the lift is paying for.
    set_alpha(&mut overlay, 1.0);
    let inputs = camera.single().ok().map(|camera| {
        world_ready_inputs_at_frame_end(
            &config,
            origin.0,
            &streaming,
            &space,
            &metrics,
            camera.translation,
            pending_drawn(&waiting),
            gate.as_deref(),
        )
    });
    // A requested cluster package is part of the first load: the world is not ready while it is
    // certifying or its blocks and batches are still arriving (the rule the demo tour and the shot
    // runner wait on, `initial_cluster_load_ready`).
    let certifying = validation
        .as_deref()
        .is_some_and(|status| *status == crate::cluster::package::PackageValidationStatus::Pending);
    let clusters_ready = match validation.as_deref() {
        None => config.clusters.is_none(),
        Some(crate::cluster::package::PackageValidationStatus::Failed(_)) => false,
        Some(_) => crate::shots::initial_cluster_load_ready(scene.as_deref(), Some(&metrics)),
    };
    let ready = physics_ready
        && clusters_ready
        && !certifying
        && inputs.is_some_and(|inputs| inputs.is_ready());
    load.ready_frames = if ready { load.ready_frames + 1 } else { 0 };
    let latched = load.ready_frames >= READY_FRAMES_REQUIRED;
    let moved = certifying || inputs != load.last_inputs;
    load.last_inputs = inputs;
    load.stalled = if moved { 0.0 } else { load.stalled + delta };
    if !latched
        && load.real_elapsed - load.last_blocked_report >= STARTUP_BLOCKED_REPORT_INTERVAL_SECONDS
    {
        load.last_blocked_report = load.real_elapsed;
        commands.trigger(StartupLoadBlocked);
        warn!(
            wall_seconds = load.real_elapsed,
            stalled_seconds = load.stalled,
            physics_ready,
            clusters_ready,
            certification = ?validation.as_deref(),
            camera_available = inputs.is_some(),
            cover_pending = load.cover_pending(),
            active_space = ?*space,
            render_origin = ?origin.0,
            inputs = ?inputs,
            pending_lod_chunk_parts = ?metrics.pending_lod_chunk_parts,
            cluster_gate = ?gate.as_deref().map(|gate| {
                gate.diagnostics(space.exterior_worldspace(config.worldspace_id))
            }),
            "startup load: admission remains blocked"
        );
    }
    if !latched && load.stalled < STARTUP_LIFT_TIMEOUT_SECONDS {
        return;
    }
    if let Some(configured) = load.configured.take() {
        restore_startup_pacing(&mut config, &mut budget, configured);
    }
    // A timeout restores pacing, never cluster readiness. Non-cluster runs retain
    // their existing recovery policy; a requested cluster run remains covered.
    if latched || config.clusters.is_none() {
        if load.covers {
            load.fade = Some(0.0);
        } else {
            load.finished = true;
        }
    }
    if latched {
        crate::work_counters::mark_world_ready();
        info!(
            seconds = load.elapsed,
            // `seconds` stays for existing parsers; the virtual clock clamps long loading frames.
            wall_seconds = load.wall_started.map(|start| start.elapsed().as_secs_f64()),
            "startup load: the world is ready; the configured streaming pacing is back"
        );
    } else if !load.stall_reported {
        load.stall_reported = true;
        // What held the world back, so a stall names its cause. Systems with more detail (the
        // terrain hand-over) report it once through the event.
        commands.trigger(StartupLoadStalled);
        warn!(
            pending_lod_chunk_parts = ?metrics.pending_lod_chunk_parts,
            seconds = load.elapsed,
            stalled_seconds = load.stalled,
            timeout_seconds = STARTUP_LIFT_TIMEOUT_SECONDS,
            clusters_ready,
            inputs = ?load.last_inputs,
            "startup load: the load made no progress in time; the configured streaming pacing is back"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::database::CellKey;
    use bevy::{
        ecs::schedule::SingleThreadedExecutor,
        log::tracing_subscriber::{Layer, Registry, layer::Context, prelude::*},
        time::TimeUpdateStrategy,
    };
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use tracing::{
        Event as TracingEvent, Subscriber,
        field::{Field, Visit},
    };

    const DT: f32 = 0.05;

    /// The configured pacing the lift must put back: all three values differ from the defaults, so
    /// a restore to the wrong ones cannot pass.
    fn configured_pacing() -> EngineConfig {
        EngineConfig {
            max_model_spawns_per_frame: 7,
            max_lod_requests_per_frame: 2,
            max_unload_ms_per_frame: 2.5,
            max_cell_unloads_per_frame: 3,
            // The tests below exercise the cover too; it is opt-in.
            startup_cover: true,
            ..default()
        }
    }

    /// A headless app with the startup lift registered, one camera, and the default interior
    /// window (the camera's own cell, so residency alone decides readiness).
    fn startup_app(config: EngineConfig) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin));
        app.init_resource::<StreamingMetrics>()
            .init_resource::<StreamingWorld>()
            .insert_resource(ActiveSpace::default())
            .insert_resource(RenderAssetBytesPerFrame {
                max_bytes: config.max_upload_bytes_per_frame(),
            })
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                DT,
            )))
            .insert_resource(config)
            .add_plugins(StartupLoadPlugin);
        app.world_mut().spawn((
            StreamingCamera,
            Transform::default(),
            GlobalTransform::default(),
        ));
        app.finish();
        app
    }

    /// Puts the camera inside interior `cell`, whose cell is resident (the world reads ready) or
    /// still loading (it does not).
    fn set_interior(app: &mut App, cell: u32, resident: bool) {
        app.world_mut().resource_mut::<ActiveSpace>().interior = Some(cell);
        if resident {
            let root = app.world_mut().spawn_empty().id();
            app.world_mut()
                .resource_mut::<StreamingWorld>()
                .set_resident_for_test(CellKey::Interior(cell), root);
        }
    }

    fn cover_alpha(app: &mut App) -> Option<f32> {
        let mut query = app
            .world_mut()
            .query_filtered::<&BackgroundColor, With<StartupCover>>();
        query
            .iter(app.world())
            .next()
            .map(|colour| colour.0.to_srgba().alpha)
    }

    fn frames_until_faded() -> u32 {
        (FADE_SECONDS / DT).ceil() as u32 + 1
    }

    #[test]
    fn the_first_frame_lifts_the_pacing_and_raises_the_cover() {
        let mut app = startup_app(configured_pacing());
        set_interior(&mut app, 77, false);
        app.update();
        let config = app.world().resource::<EngineConfig>();
        assert_eq!(config.max_model_spawns_per_frame, 0);
        assert_eq!(config.max_lod_requests_per_frame, 0);
        assert_eq!(config.max_unload_ms_per_frame, 0.0);
        assert_eq!(config.max_cell_unloads_per_frame, 0);
        assert_eq!(
            app.world().resource::<RenderAssetBytesPerFrame>().max_bytes,
            None,
            "the upload budget is the crossing's own lift"
        );
        assert_eq!(
            cover_alpha(&mut app),
            Some(1.0),
            "the cover is up from the first frame"
        );
        assert!(app.world().resource::<StartupLoad>().configured.is_some());
    }

    #[test]
    fn the_configured_pacing_is_back_on_the_ready_frame() {
        let mut app = startup_app(configured_pacing());
        set_interior(&mut app, 77, true);
        app.update();
        // One ready frame is not the latch: the report's own rule wants two consecutive ones.
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            0
        );
        app.update();
        let config = app.world().resource::<EngineConfig>();
        assert_eq!(config.max_model_spawns_per_frame, 7);
        assert_eq!(config.max_lod_requests_per_frame, 2);
        assert_eq!(config.max_unload_ms_per_frame, 2.5);
        assert_eq!(config.max_cell_unloads_per_frame, 3);
        assert_eq!(
            app.world().resource::<RenderAssetBytesPerFrame>().max_bytes,
            Some(16 * 1024 * 1024)
        );
        assert_eq!(
            cover_alpha(&mut app),
            Some(1.0),
            "still black on the restore frame"
        );
        // The fade runs out over FADE_SECONDS and takes the cover with it.
        for _ in 0..frames_until_faded() {
            app.update();
        }
        assert_eq!(cover_alpha(&mut app), None, "the cover is gone");
        assert!(app.world().resource::<StartupLoad>().finished);
    }

    #[test]
    fn world_ready_route_waits_for_startup_cover_to_finish_before_moving() {
        use crate::route::{RouteGround, RoutePlan, RouteRunner, drive_route};

        let mut config = configured_pacing();
        config.benchmark_duration_secs = Some(10.0);
        let mut app = startup_app(config);
        set_interior(&mut app, 77, true);
        let plan = RoutePlan::parse(
            r#"{"name":"fade","worldspace":60,"speed":400,"waypoints":[[0,0],[1000,0]]}"#,
        )
        .unwrap();
        let mut tracker = crate::pacing::PacingTracker::default();
        tracker.observe(0.0, true, false, 16.0);
        tracker.observe(16.0, true, false, 16.0);
        assert!(tracker.world_ready_reached());
        app.insert_resource(tracker)
            .insert_resource(RouteRunner::new(plan, RouteGround::from_cells([])))
            .add_systems(PreUpdate, drive_route);
        app.update();
        for _ in 0..frames_until_faded() + 2 {
            if !app.world().resource::<StartupLoad>().cover_pending() {
                break;
            }
            assert!(
                !app.world().resource::<RouteRunner>().report().started,
                "a ready latch must not start traversal during the startup fade"
            );
            app.update();
        }
        assert!(!app.world().resource::<StartupLoad>().cover_pending());
        app.update();
        app.update();
        let route = app.world().resource::<RouteRunner>().report();
        assert!(route.started);
        assert!(route.distance_travelled_units > 0.0);
    }

    #[test]
    fn an_explicit_immediate_route_keeps_its_startup_stress_behavior() {
        use crate::route::{RouteGround, RoutePlan, RouteRunner, drive_route};

        let mut config = configured_pacing();
        config.benchmark_duration_secs = Some(10.0);
        let mut app = startup_app(config);
        set_interior(&mut app, 77, true);
        let plan = RoutePlan::parse(
            r#"{"name":"immediate","worldspace":60,"speed":400,"start":"immediately","waypoints":[[0,0],[1000,0]]}"#,
        )
        .unwrap();
        app.insert_resource(RouteRunner::new(plan, RouteGround::from_cells([])))
            .add_systems(PreUpdate, drive_route);
        app.update();
        assert!(app.world().resource::<StartupLoad>().cover_pending());
        assert!(app.world().resource::<RouteRunner>().report().started);
    }

    #[test]
    fn a_later_loss_of_ready_never_lifts_again() {
        let mut app = startup_app(configured_pacing());
        set_interior(&mut app, 77, true);
        for _ in 0..frames_until_faded() + 2 {
            app.update();
        }
        assert_eq!(app.world().resource::<StartupLoad>().configured, None);
        // The world falls out of ready and comes back: the latch has happened, so nothing lifts.
        set_interior(&mut app, 78, false);
        app.update();
        set_interior(&mut app, 77, true);
        for _ in 0..3 {
            app.update();
        }
        let config = app.world().resource::<EngineConfig>();
        assert_eq!(config.max_model_spawns_per_frame, 7);
        assert_eq!(config.max_unload_ms_per_frame, 2.5);
        assert_eq!(config.max_cell_unloads_per_frame, 3);
        assert_eq!(
            app.world().resource::<RenderAssetBytesPerFrame>().max_bytes,
            Some(16 * 1024 * 1024)
        );
        assert_eq!(cover_alpha(&mut app), None);
    }

    #[test]
    fn by_default_the_load_is_lifted_and_shown_without_a_cover() {
        // The user, 2026-10-09: a dev build shows the world while it loads; the cover is opt-in.
        let mut app = startup_app(EngineConfig {
            startup_cover: false,
            ..configured_pacing()
        });
        set_interior(&mut app, 77, false);
        app.update();
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            0,
            "the lift is on"
        );
        assert_eq!(cover_alpha(&mut app), None, "and no cover with it");
        set_interior(&mut app, 77, true);
        app.update();
        app.update();
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            7
        );
        assert!(app.world().resource::<StartupLoad>().finished);
    }

    #[test]
    fn a_capture_run_lifts_without_a_cover() {
        // `--shots` (and `--demo-tour`): the load is faster, but nothing is drawn over the frames,
        // because a photograph must never find a black cover up.
        let mut app = startup_app(EngineConfig {
            portal: crate::config::PortalOptions {
                shots: Some("poses.json".into()),
                ..default()
            },
            ..configured_pacing()
        });
        set_interior(&mut app, 77, false);
        app.update();
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            0,
            "the lift is on"
        );
        assert_eq!(cover_alpha(&mut app), None, "and no cover with it");
        set_interior(&mut app, 77, true);
        app.update();
        app.update();
        let config = app.world().resource::<EngineConfig>();
        assert_eq!(config.max_model_spawns_per_frame, 7);
        assert_eq!(config.max_unload_ms_per_frame, 2.5);
        assert_eq!(config.max_cell_unloads_per_frame, 3);
        assert!(app.world().resource::<StartupLoad>().finished);
        assert_eq!(cover_alpha(&mut app), None);
    }

    #[test]
    fn paced_startup_keeps_the_budgets_in_force_from_the_first_frame() {
        let mut app = startup_app(EngineConfig {
            paced_startup: true,
            ..configured_pacing()
        });
        set_interior(&mut app, 77, false);
        app.update();
        let config = app.world().resource::<EngineConfig>();
        assert_eq!(config.max_model_spawns_per_frame, 7);
        assert_eq!(config.max_unload_ms_per_frame, 2.5);
        assert_eq!(config.max_cell_unloads_per_frame, 3);
        assert_eq!(
            app.world().resource::<RenderAssetBytesPerFrame>().max_bytes,
            Some(16 * 1024 * 1024)
        );
        assert_eq!(cover_alpha(&mut app), None);
        assert!(app.world().get_resource::<StartupLoad>().is_none());
    }

    #[test]
    fn a_load_that_keeps_moving_stays_lifted_past_the_timeout_and_stops_when_it_stalls() {
        let mut app = startup_app(configured_pacing());
        const STEP: f32 = 0.25;
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            STEP,
        )));
        set_interior(&mut app, 77, false);
        let frames_to_timeout = (STARTUP_LIFT_TIMEOUT_SECONDS / STEP).ceil() as usize;
        for frame in 0..frames_to_timeout * 2 {
            app.world_mut()
                .resource_mut::<StreamingMetrics>()
                .loading_cells = frame % 3 + 1;
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            0,
            "a load still moving after twice the timeout keeps the lift"
        );
        for _ in 0..=frames_to_timeout {
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            7,
            "a load that stops moving for the timeout gets its pacing back"
        );
    }

    #[test]
    fn selected_clusters_cover_paced_startup_without_the_optional_cover_flag() {
        let config = EngineConfig {
            clusters: Some("selected".into()),
            cluster_own_statics: true,
            paced_startup: true,
            startup_cover: false,
            ..configured_pacing()
        };
        let mut app = startup_app(config);
        set_interior(&mut app, 77, false);
        app.update();
        assert_eq!(cover_alpha(&mut app), Some(1.0));
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            7
        );
        assert_eq!(
            app.world().resource::<RenderAssetBytesPerFrame>().max_bytes,
            Some(16 * 1024 * 1024)
        );
    }

    #[test]
    fn a_failed_cluster_load_never_uncovers_an_otherwise_resident_world() {
        let mut config = configured_pacing();
        config.clusters = Some("selected".into());
        config.cluster_own_statics = true;
        let mut app = startup_app(config);
        set_interior(&mut app, 77, true);
        app.insert_resource(crate::cluster::package::PackageValidationStatus::Failed(
            "requested package failed".into(),
        ));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            0.25,
        )));
        for _ in 0..(STARTUP_LIFT_TIMEOUT_SECONDS / 0.25).ceil() as usize + 10 {
            app.update();
        }
        assert_eq!(
            cover_alpha(&mut app),
            Some(1.0),
            "a failed requested cluster package cannot be replaced by a timeout"
        );
    }

    #[test]
    fn a_certifying_cluster_package_holds_an_otherwise_ready_world_lifted() {
        let mut app = startup_app(configured_pacing());
        set_interior(&mut app, 77, true);
        app.insert_resource(crate::cluster::package::PackageValidationStatus::Pending);
        for _ in 0..(READY_FRAMES_REQUIRED as usize + 20) {
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            0,
            "the world is not ready while its requested cluster package certifies"
        );
        app.world_mut()
            .remove_resource::<crate::cluster::package::PackageValidationStatus>();
        for _ in 0..=(READY_FRAMES_REQUIRED as usize) {
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            7
        );
    }

    #[derive(Clone, Default)]
    struct BlockedLogs(Arc<Mutex<Vec<BTreeMap<String, String>>>>);

    #[derive(Default)]
    struct LogFields(BTreeMap<String, String>);

    impl Visit for LogFields {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.insert(field.name().into(), format!("{value:?}"));
        }
    }

    impl<S: Subscriber> Layer<S> for BlockedLogs {
        fn on_event(&self, event: &TracingEvent<'_>, _: Context<'_, S>) {
            if event.metadata().target() != "engine::pacing::startup" {
                return;
            }
            let mut fields = LogFields::default();
            event.record(&mut fields);
            if fields
                .0
                .get("message")
                .is_some_and(|message| message.contains("admission remains blocked"))
            {
                self.0.lock().unwrap().push(fields.0);
            }
        }
    }

    impl BlockedLogs {
        fn capture(&self, run: impl FnOnce()) {
            tracing::subscriber::with_default(Registry::default().with(self.clone()), run);
        }

        fn snapshot(&self) -> Vec<BTreeMap<String, String>> {
            self.0.lock().unwrap().clone()
        }
    }

    #[derive(Resource, Default)]
    struct StallReports(usize);

    #[derive(Resource, Default)]
    struct BlockedReports(usize);

    fn blocked_startup_app() -> App {
        let mut config = configured_pacing();
        config.clusters = Some("selected".into());
        config.worldspace_id = 60;
        let mut app = startup_app(config);
        // Keep the real PostUpdate system on the scoped subscriber's thread.
        app.edit_schedule(PostUpdate, |schedule| {
            schedule.set_executor(SingleThreadedExecutor::new());
        });
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
            .init_resource::<crate::cluster::loading::ClusterLoadingGate>()
            .insert_resource(crate::physics::MoveMode::Walk)
            .init_resource::<StallReports>()
            .init_resource::<BlockedReports>()
            .add_observer(
                |_: On<StartupLoadBlocked>, mut reports: ResMut<BlockedReports>| {
                    reports.0 += 1;
                },
            )
            .add_observer(
                |_: On<StartupLoadStalled>, mut reports: ResMut<StallReports>| {
                    reports.0 += 1;
                },
            );
        app
    }

    #[test]
    fn blocked_report_uses_real_seconds_even_while_loading_inputs_change() {
        let mut app = blocked_startup_app();
        {
            let mut metrics = app.world_mut().resource_mut::<StreamingMetrics>();
            metrics.pending_asset_instances = 7;
            metrics.pending_lod_chunks = 163;
            metrics.pending_lod_chunk_parts = [0, 17, 146];
            metrics.pending_far_stock_queries = 1;
        }
        let logs = BlockedLogs::default();
        logs.capture(|| {
            app.update(); // The first real-time update has zero delta.
            for second in 1..=120 {
                app.world_mut()
                    .resource_mut::<StreamingMetrics>()
                    .queued_far_stock_queries = 2451 + second % 2;
                app.update();
                assert_eq!(
                    logs.snapshot().len(),
                    second / 60,
                    "a blocked admission reports every 60 real seconds despite progress; second={second}"
                );
                assert_eq!(app.world().resource::<BlockedReports>().0, second / 60);
            }
        });
        let reports = logs.snapshot();
        assert_eq!(reports[0]["physics_ready"], "false");
        assert_eq!(reports[0]["clusters_ready"], "false");
        assert_eq!(reports[0]["certification"], "None");
        assert_eq!(reports[0]["pending_lod_chunk_parts"], "[0, 17, 146]");
        assert!(reports[0]["inputs"].contains("pending_asset_instances: 7"));
        assert!(reports[0]["inputs"].contains("pending_lod_chunks: 163"));
        assert!(reports[0]["inputs"].contains("queued_far_stock_queries: 2451"));
        assert!(reports[0]["inputs"].contains("pending_far_stock_queries: 1"));
        assert!(reports[0].contains_key("active_space"));
        assert!(reports[0].contains_key("render_origin"));
        assert!(reports[0].contains_key("wall_seconds"));
        assert_eq!(cover_alpha(&mut app), Some(1.0));
        assert_eq!(app.world().resource::<StallReports>().0, 0);
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            0,
            "periodic diagnostics must not restore pacing"
        );
    }

    #[test]
    fn blocked_report_continues_during_pending_certification() {
        let mut app = blocked_startup_app();
        app.insert_resource(crate::cluster::package::PackageValidationStatus::Pending);
        let logs = BlockedLogs::default();
        logs.capture(|| {
            app.update();
            for _ in 0..120 {
                app.update();
            }
        });
        let reports = logs.snapshot();
        assert_eq!(
            reports.len(),
            2,
            "Pending certification must not reset the blocked-report clock"
        );
        assert_eq!(reports[0]["certification"], "Some(Pending)");
        assert_eq!(reports[1]["certification"], "Some(Pending)");
        assert_eq!(app.world().resource::<BlockedReports>().0, 2);
        assert_eq!(app.world().resource::<StallReports>().0, 0);
        assert_eq!(cover_alpha(&mut app), Some(1.0));
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            0
        );
    }

    #[test]
    fn blocked_report_exposes_cached_gate_scope_and_withdrawal_without_releasing_cover() {
        use crate::cluster::loading::{ClusterLoadingGate, ClusterWorldReadiness};
        let directory = crate::cluster::reader::producer_fixture_tests::published_for_loader();
        let package = Arc::new(
            crate::cluster::overlay_fixture::open_fixture(directory.path(), false).unwrap(),
        );
        let mut gate = ClusterLoadingGate::default();
        for _ in 0..6 {
            gate.invalidate();
        }
        assert!(gate.publish(
            &package,
            9,
            7,
            31,
            std::collections::HashMap::from([(
                60,
                ClusterWorldReadiness {
                    applicable_cameras: 2,
                    missing_views: 3,
                    needed_batches: 11,
                    pending_materials: 5,
                    pending_templates: 7,
                    pending_compute: 13,
                    pending_pipelines: 17,
                    failed_batches: 19,
                },
            )]),
        ));
        let mut app = blocked_startup_app();
        app.insert_resource(gate);
        let logs = BlockedLogs::default();
        logs.capture(|| {
            app.update();
            for _ in 0..60 {
                app.update();
            }
            let reports = logs.snapshot();
            assert_eq!(reports.len(), 1, "the current cached gate must be reported");
            let current = &reports[0]["cluster_gate"];
            for expected in [
                "world: 60",
                "package_scope: Some(",
                "layout: 9",
                "generation: 7",
                "snapshot_generation: Some(7)",
                "render_frame: Some(31)",
                "applicable_cameras: 2",
                "missing_views: 3",
                "needed_batches: 11",
                "pending_materials: 5",
                "pending_templates: 7",
                "pending_compute: 13",
                "pending_pipelines: 17",
                "failed_batches: 19",
            ] {
                assert!(current.contains(expected), "missing {expected}: {current}");
            }
            app.world_mut()
                .resource_mut::<ClusterLoadingGate>()
                .invalidate();
            for _ in 0..180 {
                app.update();
            }
        });
        let reports = logs.snapshot();
        assert_eq!(
            reports.len(),
            4,
            "blocked reports continue after the stall timeout"
        );
        let withdrawn = &reports[1]["cluster_gate"];
        assert!(withdrawn.contains("generation: 8"));
        assert!(withdrawn.contains("snapshot_generation: None"));
        assert!(withdrawn.contains("counts: None"));
        assert!(!withdrawn.contains("pending_pipelines: 17"));
        assert_eq!(app.world().resource::<BlockedReports>().0, 4);
        assert_eq!(app.world().resource::<StallReports>().0, 1);
        assert_eq!(cover_alpha(&mut app), Some(1.0));
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            7,
            "the existing 60-virtual-second stall still restores pacing"
        );
    }

    #[test]
    fn blocked_report_stops_after_startup_admission_finishes() {
        let mut app = startup_app(configured_pacing());
        app.edit_schedule(PostUpdate, |schedule| {
            schedule.set_executor(SingleThreadedExecutor::new());
        });
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(1)))
            .init_resource::<BlockedReports>()
            .add_observer(
                |_: On<StartupLoadBlocked>, mut reports: ResMut<BlockedReports>| {
                    reports.0 += 1;
                },
            );
        set_interior(&mut app, 77, false);
        let logs = BlockedLogs::default();
        logs.capture(|| {
            app.update();
            for _ in 0..60 {
                app.update();
            }
            assert_eq!(logs.snapshot().len(), 1);
            set_interior(&mut app, 77, true);
            for _ in 0..120 {
                app.update();
            }
        });
        assert!(app.world().resource::<StartupLoad>().finished);
        assert_eq!(app.world().resource::<BlockedReports>().0, 1);
        assert_eq!(
            logs.snapshot().len(),
            1,
            "a finished startup must stop reporting"
        );
    }

    #[test]
    fn a_world_that_never_becomes_ready_gets_its_pacing_back_at_the_timeout() {
        let mut app = startup_app(configured_pacing());
        // Virtual time clamps each frame's delta to its 0.25 s maximum, so the timeout is reached
        // in frames of that length; Bevy's first update advances time by zero.
        const STEP: f32 = 0.25;
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            STEP,
        )));
        set_interior(&mut app, 77, false);
        #[derive(Resource, Default)]
        struct Stalls(u32);
        app.init_resource::<Stalls>()
            .add_observer(|_: On<StartupLoadStalled>, mut stalls: ResMut<Stalls>| stalls.0 += 1);
        let frames_to_timeout = (STARTUP_LIFT_TIMEOUT_SECONDS / STEP).ceil() as usize;
        for _ in 0..frames_to_timeout {
            app.update();
        }
        assert_eq!(
            app.world()
                .resource::<EngineConfig>()
                .max_model_spawns_per_frame,
            0,
            "still lifted before the timeout"
        );
        assert_eq!(app.world().resource::<Stalls>().0, 0);
        app.update();
        assert_eq!(
            app.world().resource::<Stalls>().0,
            1,
            "the stall is announced once, so detailed reports run only then"
        );
        let config = app.world().resource::<EngineConfig>();
        assert_eq!(config.max_model_spawns_per_frame, 7);
        assert_eq!(config.max_unload_ms_per_frame, 2.5);
        assert_eq!(config.max_cell_unloads_per_frame, 3);
        assert_eq!(
            app.world().resource::<RenderAssetBytesPerFrame>().max_bytes,
            Some(16 * 1024 * 1024),
            "the timeout path restores every budget too"
        );
        app.update();
        assert_eq!(
            cover_alpha(&mut app),
            None,
            "and the cover still comes down"
        );
        assert_eq!(app.world().resource::<Stalls>().0, 1, "and never again");
    }
}
