//! CPU time of the render side of a frame, for benchmark reports.
//!
//! A benchmark's frame time is far larger than the main world's CPU time plus the GPU's pass
//! time: with pipelined rendering most of a frame goes to the render thread (extract commands,
//! asset preparation, specialization, queueing, bind groups, render graph encoding, submit and
//! present) and to the main thread waiting for it. This measures those parts:
//!
//! * `main_world`: the main world's schedule, from `First` to `Last`.
//! * `wait_for_render_thread`: the main thread waiting for the render thread to hand the render
//!   world back before it can extract (pipelined rendering only).
//! * `extract`: the extract step itself, on the main thread.
//! * `render_thread`: the render schedule, and its phases between Bevy's `RenderSystems` sets:
//!   `render/extract_commands_and_assets`, `render/specialize_and_views`, `render/queue`,
//!   `render/prepare`, `render/graph_and_present`, `render/cleanup`. The phases are a lower-bound
//!   decomposition of `render_thread`, not a partition of it: each is the marked gap between two of
//!   those sets, so the unmarked time — from the start of the render frame to the first lap and
//!   between laps — belongs to no phase, and their sum is at most `render_thread`. The swapchain
//!   wait sits inside `render/prepare`.
//! * `render/swapchain_acquire`: from just before `prepare_windows` to just after it, an upper
//!   bound on the wait for a swapchain image (where a GPU-bound frame shows up on the CPU).
//!
//! * Pipeline activity: each render frame also records how many pipelines were newly queued
//!   (`PipelineCache` in the render world), how many finished creating, and how many still wait.
//!   The cache holds render and compute pipelines alike, so the counts are not render pipelines
//!   alone. [`PipelineActivity`] sets the frames with activity against the rest and lists the worst
//!   frames with their counts, which tests "the worst frames are pipelines built on first use".
//!
//! Samples are kept only while the benchmark records (after warmup). Phases are distributions,
//! not a per-frame series: a render frame runs one frame behind the main frame that fed it.
//!
//! * Cycles: a benchmark run also keeps one small record per main-thread frame ("cycle"), holding
//!   the main schedule, the extract, the render schedule and the swapchain wait that ran in that
//!   cycle, so `frame_budget` can say which side owned a frame that went over its budget. A cycle is
//!   the time from the start of one main frame to the start of the next, which is what a frame time
//!   measures; cycles are counted from one, the counter having started at zero with no window open,
//!   so a run's first recorded window is cycle 1. With pipelined rendering the render schedule that
//!   follows a window's extract runs during the next window (alongside the frame after the one that
//!   extracted), so it is filed under the next cycle; without it the render schedule runs inside its
//!   own window's cycle. See [`CycleSides`].
//!
//! * `wall_cycle_ms` and `real_ms`: the per-cycle record carries two clocks. `wall_cycle_ms` is
//!   the lap from this cycle's `First` to the next's, measured on the main thread — the number
//!   the identity `main_world + extract + wait + frame/remainder` accounts for, which the identity
//!   test checks. `real_ms` is the `Time<Real>` delta the next `First` read: under pipelined
//!   rendering Bevy feeds the main world's `Time<Real>` from the render thread (bevy_time 0.19.1
//!   `TimeReceiver`), so that reading is render-paced and lagged rather than the cycle's wall
//!   time — it is kept beside the wall lap so a profile can quantify the clock skew directly
//!   instead of assuming it. Both close when the next cycle's `First` runs, so the run's last
//!   cycle keeps neither.
//!
//! * `frame/remainder`: the main-thread time of a cycle that none of the sides above own — the
//!   window from the end of `Last` to the start of the next `First`, less the extract and the wait
//!   for the render thread when both ran inside it (pipelined rendering runs them there). This is
//!   the region the 2026-10-08 door-tour baseline found unaccounted on the door-crossing frames
//!   (`local/research/portal-crossing-frames-895-2666-2026-10-08.md`). Kept only while cycles are
//!   tracked (a benchmark run); on top of that it is sampled as `frame/remainder` while recording.
//!   The per-cycle records are also handed over as rows for the profile bundle's
//!   `render-cycles.csv` (see `profiling`). Cycles are counted from one while the per-frame series'
//!   rows are counted from zero, so the row for cycle `n` joins the `frame-series.csv` row numbered
//!   `n - 1` — the frame whose window the cycle is — not the row with the same number, which would
//!   place every crossing spike one frame late. The two files' `real_ms` columns confirm that join:
//!   both are the `Time<Real>` delta the `First` that closed the window read.
//!
//! * `render/graph/*`: the laps inside `render/graph_and_present`, the phase's own decomposition
//!   (8.85 ms p50 in the 2026-10-08 portal-heavy re-profile, with only 0.94 ms measured inside it;
//!   `local/research/portal-heavy-render-thread-2026-10-08.md` §4 R2). Bevy 0.19.0 runs the render
//!   graph as *schedules*: the root `RenderGraph` schedule, run by `render_system`
//!   (`bevy_render-0.19.0` `src/renderer/mod.rs:69`), runs every camera's and auxiliary view's
//!   graph schedule in its `Render` set — Bevy's `camera_driver`, which loops over the views and
//!   runs each one's schedule (`bevy_core_pipeline-0.19.0` `src/schedule.rs:133`, `:200`) — and
//!   submits the graph's command buffers in its `Submit` set (`:228`). A 3D view's schedule is
//!   `Core3d`, whose `Prepass`, `MainPass`, `EarlyPostProcess` and `PostProcess` sets hold the GPU
//!   binning, the mesh unpacking and the indirect-draw building, the shadow and depth prepasses, the
//!   main opaque and transparent passes and the post-processing
//!   (`bevy_core_pipeline-0.19.0` `src/core_3d/mod.rs:145`). The *laps* are ours, the boundaries are
//!   Bevy's sets: they are ordinary systems added to those schedules and ordered between the sets,
//!   so no Bevy code is forked or patched. (The gap between the root `Render` and `Submit` sets also
//!   holds Bevy's diagnostics-encoder resolve when `RenderDiagnosticsPlugin` is added — the engine
//!   always adds it — so the views lap can carry that few microseconds.) Per cycle the record keeps
//!   `render/graph/views` (the root
//!   `Render` set: every view's schedule, the part of the phase that scales with views),
//!   `render/graph/view_count` (how many view schedules the cycle ran — a count, not a duration),
//!   the sums over those views of `render/graph/prepass`, `render/graph/main_pass` and
//!   `render/graph/post_process`, `render/graph/submit` (the graph command buffers' `queue.submit`
//!   and the uncovered-window clear) and `render/graph/remainder`, which closes the phase:
//!   everything the laps do not own — the schedule's own `Begin`/`Finish` sets and overhead, and,
//!   after the schedule, the screenshot and readback submit, the `queue.submit` of their encoder and
//!   `present` on each window (`bevy_render-0.19.0` `src/renderer/mod.rs:78-110`). Two parts are
//!   reported as not split rather than guessed: `present` cannot be separated from that remainder
//!   without patching Bevy or building it with its `trace` feature, and the GPU-driven preprocessing
//!   has no set of its own inside `Prepass`, so it is inside `render/graph/prepass`. A view that
//!   runs a graph schedule other than `Core3d` is counted in `views` but not in the per-view laps.

use bevy::{
    app::{First, Last},
    core_pipeline::{Core3d, Core3dSystems},
    ecs::schedule::Schedules,
    platform::time::Instant,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        pipelined_rendering::RenderExtractApp,
        render_resource::PipelineCache,
        renderer::{RenderGraph, RenderGraphSystems},
        view::prepare_windows,
    },
    time::TimeSystems,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub const MAIN_WORLD: &str = "main_world";
pub const WAIT_FOR_RENDER_THREAD: &str = "wait_for_render_thread";
pub const EXTRACT: &str = "extract";
pub const RENDER_THREAD: &str = "render_thread";
pub const SWAPCHAIN_ACQUIRE: &str = "render/swapchain_acquire";
pub const REMAINDER: &str = "frame/remainder";
/// The cycle record's wall time, as the CSV column names it: the lap from one main frame's
/// `First` to the next's, measured on the main thread. The identity `main_world + extract + wait
/// + frame/remainder` accounts for this number, which the identity test checks.
pub const WALL_CYCLE_MS: &str = "wall_cycle_ms";
/// The cycle record's `Time<Real>` delta, as the CSV column names it, read at the next cycle's
/// `First` — under pipelined rendering a render-paced, lagged reading (see the module doc), kept
/// beside the wall lap so the skew between the two clocks is measurable per cycle.
pub const REAL_MS: &str = "real_ms";
/// The render schedule's phases, as span names; the per-cycle record keeps one value each (the
/// distributions live in the samples). Shared with the per-cycle CSV's column list, so the two
/// cannot drift apart.
pub const EXTRACT_COMMANDS_AND_ASSETS: &str = "render/extract_commands_and_assets";
pub const SPECIALIZE_AND_VIEWS: &str = "render/specialize_and_views";
pub const QUEUE: &str = "render/queue";
pub const PREPARE: &str = "render/prepare";
pub const GRAPH_AND_PRESENT: &str = "render/graph_and_present";
pub const CLEANUP: &str = "render/cleanup";

/// The laps inside `render/graph_and_present`, as span names and `render-cycles.csv` column names
/// (see the module doc). Every one is a wall lap on the render thread, taken at a boundary of the
/// render graph's own schedules, and summed into the cycle's record:
///
/// * [`GRAPH_VIEWS`]: the root graph schedule's `Render` set — Bevy's `camera_driver`, which runs
///   every camera's and auxiliary view's graph schedule. The part of the phase that scales with
///   views.
/// * [`GRAPH_VIEW_COUNT`] with [`GRAPH_PREPASS`], [`GRAPH_MAIN_PASS`] and [`GRAPH_POST_PROCESS`]:
///   how many view schedules (`Core3d` runs) the cycle ran, and the sums over them of the
///   schedule's prepass, main-pass and post-process stages. The GPU-driven binning and
///   indirect-draw building run inside the `Prepass` set, so their time is in `GRAPH_PREPASS`.
/// * [`GRAPH_SUBMIT`]: the root graph schedule's `Submit` set, where the graph's command buffers go
///   to the queue.
/// * [`GRAPH_REMAINDER`]: `render/graph_and_present` less the laps above — the schedule's own
///   overhead and, after it, the screenshot and readback submit, the second `queue.submit` and
///   `present` on each window. Present is not isolable without patching Bevy, so the remainder
///   carries it (see the module doc).
pub const GRAPH_VIEWS: &str = "render/graph/views";
/// How many view schedules the cycle's per-view sums cover. The one column that is a count rather
/// than milliseconds.
pub const GRAPH_VIEW_COUNT: &str = "render/graph/view_count";
pub const GRAPH_PREPASS: &str = "render/graph/prepass";
pub const GRAPH_MAIN_PASS: &str = "render/graph/main_pass";
pub const GRAPH_POST_PROCESS: &str = "render/graph/post_process";
pub const GRAPH_SUBMIT: &str = "render/graph/submit";
pub const GRAPH_REMAINDER: &str = "render/graph/remainder";

/// Measures the render side of each frame; shared by the main world, the render world and the
/// extract wrappers.
pub struct RenderTimingPlugin;

impl Plugin for RenderTimingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderTimings>()
            .add_systems(First, start_main_world.after(TimeSystems))
            .add_systems(Last, end_main_world);
    }

    // The extract functions are set while the render plugins build, and pipelined rendering moves
    // the render app to its thread in `cleanup`, so they are wrapped here, in between.
    fn finish(&self, app: &mut App) {
        let timings = app.world().resource::<RenderTimings>().clone();
        // The per-cycle records only feed the benchmark report's frame budget, so an ordinary play
        // session keeps none.
        let benchmark = app
            .world()
            .get_resource::<crate::config::EngineConfig>()
            .is_some_and(crate::config::EngineConfig::is_benchmark_run);
        timings.set_cycle_tracking(benchmark, app.get_sub_app(RenderExtractApp).is_some());
        // Counting the pipeline cache only feeds the pacing report, so an ordinary play session
        // does not run it every render frame.
        let samples_pipelines = app
            .world()
            .get_resource::<crate::config::EngineConfig>()
            .is_some_and(crate::config::EngineConfig::measures_pacing);
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.insert_resource(timings.clone());
            if let Some(mut extract) = render_app.take_extract() {
                let timings = timings.clone();
                render_app.set_extract(move |main_world, render_world| {
                    let started = Instant::now();
                    extract(main_world, render_world);
                    timings.extracted(started.elapsed());
                });
            }
            add_render_marks(render_app, samples_pipelines);
        }
        if let Some(extract_app) = app.get_sub_app_mut(RenderExtractApp)
            && let Some(mut hand_over) = extract_app.take_extract()
        {
            let timings = timings.clone();
            extract_app.set_extract(move |main_world, world| {
                let started = Instant::now();
                hand_over(main_world, world);
                timings.handed_over(started.elapsed());
            });
        }
    }
}

fn add_render_marks(render_app: &mut SubApp, samples_pipelines: bool) {
    // `RenderSystems::Render` shares its name with the `Render` schedule, so the sets are spelled
    // out.
    type Set = RenderSystems;
    render_app.add_systems(
        Render,
        (
            start_render_frame.before(Set::ExtractCommands),
            lap(EXTRACT_COMMANDS_AND_ASSETS)
                .after(Set::PrepareMeshes)
                .before(Set::CreateViews),
            lap(SPECIALIZE_AND_VIEWS)
                .after(Set::PrepareViews)
                .before(Set::Queue),
            lap(QUEUE).after(Set::PhaseSort).before(Set::Prepare),
            lap(PREPARE).after(Set::Prepare).before(Set::Render),
            lap(GRAPH_AND_PRESENT)
                .after(Set::Render)
                .before(Set::Cleanup),
            (
                lap(CLEANUP),
                sample_pipelines.run_if(move || samples_pipelines),
                end_render_frame,
            )
                .chain()
                .after(Set::PostCleanup),
            start_swapchain_acquire
                .in_set(Set::PrepareViews)
                .before(prepare_windows),
            end_swapchain_acquire
                .in_set(Set::PrepareViews)
                .after(prepare_windows),
        ),
    );
    add_graph_marks(render_app);
}

/// The laps inside `render/graph_and_present` (see the module doc and [`GRAPH_VIEWS`]). Bevy 0.19
/// runs the render graph as schedules, so the boundaries are its own sets — the root schedule's
/// `Render` and `Submit` sets, and `Core3d`'s stage sets — and ours are the systems ordered
/// between them, run once per render frame and once per view. Nothing in Bevy is forked or
/// patched, and every system is gated on cycle tracking, so a run that keeps no cycles pays one
/// atomic load per boundary and no lock.
///
/// The root hooks are added through the `RenderGraph` schedule itself; the `Core3d` hooks only
/// when that schedule exists (it is registered by `Core3dPlugin`, which a 2D-only or headless app
/// does not add), so a view that runs another graph schedule is measured by the root
/// `render/graph/views` lap alone.
fn add_graph_marks(render_app: &mut SubApp) {
    render_app.add_systems(
        RenderGraph,
        (
            start_graph_frame.in_set(RenderGraphSystems::Begin),
            lap_graph_views
                .after(RenderGraphSystems::Render)
                .before(RenderGraphSystems::Submit),
            lap_graph_submit
                .after(RenderGraphSystems::Submit)
                .before(RenderGraphSystems::Finish),
        )
            .run_if(tracks_cycles),
    );
    if render_app.world().resource::<Schedules>().contains(Core3d) {
        render_app.add_systems(
            Core3d,
            (
                start_view.before(Core3dSystems::Prepass),
                lap_view_prepass
                    .after(Core3dSystems::Prepass)
                    .before(Core3dSystems::MainPass),
                lap_view_main_pass
                    .after(Core3dSystems::MainPass)
                    .before(Core3dSystems::EarlyPostProcess),
                lap_view_post_process.after(Core3dSystems::PostProcess),
            )
                .run_if(tracks_cycles),
        );
    }
}

/// The shared clock: a handle cloned into both worlds and the extract wrappers.
#[derive(Resource, Clone, Default)]
pub struct RenderTimings(Arc<Shared>);

#[derive(Default)]
struct Shared {
    recording: AtomicBool,
    /// Whether the per-cycle records are kept (a benchmark run). The same flag as before every
    /// per-cycle hook would lock the state for, readable — and checkable as a system's run
    /// condition — without the lock, so the per-view laps cost one atomic load when a run keeps no
    /// cycles.
    track_cycles: AtomicBool,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    samples: BTreeMap<&'static str, Vec<f64>>,
    main_started: Option<Instant>,
    /// When the cycle in progress began, set at its `First`: the next `First` closes the cycle's
    /// wall lap against it, and reads the real clock into the same row. Kept only when cycles are
    /// tracked (a benchmark run).
    cycle_started: Option<Instant>,
    render_started: Option<Instant>,
    render_mark: Option<Instant>,
    acquire_started: Option<Instant>,
    last_extract: Option<Duration>,
    /// Pipeline counts for the render frame in progress, set just before it ends.
    pipelines: Option<PipelineFrame>,
    frames: Vec<RenderFrame>,
    /// The mark the root render graph schedule's `Begin` set opened, closed by the lap after its
    /// `Render` set and re-armed for the `Submit` lap ([`GRAPH_VIEWS`], [`GRAPH_SUBMIT`]).
    graph_mark: Option<Instant>,
    /// The mark a view's graph schedule opened, re-armed at each of its stage boundaries, so the
    /// per-view stage laps ([`GRAPH_PREPASS`], [`GRAPH_MAIN_PASS`], [`GRAPH_POST_PROCESS`]) tile
    /// the view.
    view_mark: Option<Instant>,
    /// Whether rendering is pipelined (the render world runs on its own thread).
    pipelined: bool,
    /// The main frame in progress, counted from one.
    cycle: u64,
    /// The main frame whose extract the render world last received.
    last_extract_cycle: Option<u64>,
    /// The cycle the render frame in progress belongs to.
    render_cycle: Option<u64>,
    cycles: BTreeMap<u64, CycleSides>,
    /// A mirror of `cycles` for the per-cycle series: rows stay here until they are complete and
    /// handed over, where the ledger's takes remove records as it reads them. Bounded by keeping
    /// only the cycles that can still be written or handed over.
    cycle_rows: BTreeMap<u64, CycleSides>,
    /// The newest cycle [`RenderTimings::take_completed_cycles`] has handed over, so a late write
    /// to an already-handed-over cycle cannot be handed over twice.
    cycle_rows_flushed_through: u64,
    /// The `Last`→`First` window of the cycle just ended, opened in `Last` when cycles are
    /// tracked, closed at the next `First`.
    remainder_started: Option<Instant>,
    /// The extract and the wait that ran inside the open window; they are subtracted from it, so
    /// the remainder is the main-thread time they do not own.
    remainder_extract_ms: f64,
    remainder_wait_ms: f64,
}

/// What ran in one main-thread cycle, in milliseconds; `None` is "not measured in this cycle".
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CycleSides {
    /// The cycle's wall time ([`WALL_CYCLE_MS`]): the lap from this cycle's `First` to the next
    /// cycle's `First`, measured on the main thread. `None` until the next `First` closed it —
    /// the run's last cycle keeps none.
    pub wall_ms: Option<f64>,
    /// The cycle's `Time<Real>` delta ([`REAL_MS`]), read at the next cycle's `First` — under
    /// pipelined rendering a render-paced, lagged reading, kept beside the wall lap so the skew
    /// between the two clocks is measurable per cycle.
    pub real_ms: Option<f64>,
    /// The main schedule, `First` to `Last`.
    pub main_ms: Option<f64>,
    /// The extract step on the main thread.
    pub extract_ms: Option<f64>,
    /// The render schedule that ran in this cycle (the one fed by the previous extract when
    /// rendering is pipelined).
    pub render_ms: Option<f64>,
    /// The part of that render schedule spent waiting for a swapchain image.
    pub acquire_ms: Option<f64>,
    /// How long the main thread waited for the render thread to hand the render world back before
    /// it could extract (pipelined rendering only). A diagnostic: it says whether the main thread
    /// was blocked on the render thread in this cycle, and is not a side of its own.
    pub wait_ms: Option<f64>,
    /// The main-thread time of this cycle that none of the sides above own: the window from the
    /// end of `Last` to the start of the next `First`, less the extract and the wait when both
    /// ran inside it. `None` until the next `First` closed the window.
    pub remainder_ms: Option<f64>,
    /// The render schedule's phases that ran in this cycle, one value each. The swapchain wait
    /// (`acquire_ms`) sits inside the prepare phase.
    pub extract_commands_and_assets_ms: Option<f64>,
    pub specialize_and_views_ms: Option<f64>,
    pub queue_ms: Option<f64>,
    pub prepare_ms: Option<f64>,
    pub graph_and_present_ms: Option<f64>,
    pub cleanup_ms: Option<f64>,
    /// The laps inside `graph_and_present_ms`, each summed over the render frame's views where it
    /// is a per-view lap (see [`GRAPH_VIEWS`] and the module doc):
    ///
    /// * `graph_views_ms`: the render graph's view schedules (the root `Render` set, Bevy's
    ///   `camera_driver`).
    /// * `graph_view_count`: how many view schedules the per-view sums below cover. A count, not a
    ///   duration; zeros and millisecond fields share this record, so the count column is written
    ///   as an integer.
    /// * `graph_prepass_ms`, `graph_main_pass_ms`, `graph_post_process_ms`: the sums over those
    ///   views of the `Core3d` schedule's prepass, main-pass and post-process stages (the
    ///   post-process sum covers `EarlyPostProcess` and `PostProcess`).
    /// * `graph_submit_ms`: the graph's `queue.submit` (the root `Submit` set) and the
    ///   uncovered-window clear.
    /// * `graph_remainder_ms`: `graph_and_present_ms` less `graph_views_ms` and `graph_submit_ms`,
    ///   never negative: the schedule's own overhead and the region after it, which carries the
    ///   screenshot/readback submit, the second `queue.submit` and `present` on each window.
    pub graph_views_ms: Option<f64>,
    pub graph_view_count: Option<u32>,
    pub graph_prepass_ms: Option<f64>,
    pub graph_main_pass_ms: Option<f64>,
    pub graph_post_process_ms: Option<f64>,
    pub graph_submit_ms: Option<f64>,
    pub graph_remainder_ms: Option<f64>,
}

/// Pipelines (render and compute) in one render frame. Bevy's `PipelineCache` holds both, so these
/// counts are not render pipelines alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PipelineFrame {
    /// Pipelines newly added to the cache (queued for creation) this frame.
    pub created: usize,
    /// Pipelines that stopped waiting (finished creating, or failed) this frame.
    pub became_ready: usize,
    /// Pipelines still queued or being created at the end of the frame.
    pub waiting: usize,
}

#[derive(Debug, Clone, Copy)]
struct RenderFrame {
    render_thread_ms: f64,
    pipelines: PipelineFrame,
}

/// A render frame with its pipeline counts, for matching a spike with pipeline creation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorstRenderFrame {
    pub render_thread_ms: f64,
    #[serde(flatten)]
    pub pipelines: PipelineFrame,
}

/// Whether the worst render frames are pipelines (render and compute) being built: render-thread
/// time on frames with pipeline activity against the others, and the slowest frames with their
/// counts.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PipelineActivity {
    pub frames: usize,
    pub frames_with_new_pipelines: usize,
    pub frames_with_pipelines_ready: usize,
    pub pipelines_created: u64,
    pub pipelines_became_ready: u64,
    /// Mean `render_thread` ms over frames that queued or finished a pipeline, and over the rest.
    pub render_thread_ms_mean_with_activity: Option<f64>,
    pub render_thread_ms_mean_without_activity: Option<f64>,
    /// The slowest render frames, worst first, with their pipeline counts.
    pub worst_frames: Vec<WorstRenderFrame>,
}

/// How many of the slowest render frames [`PipelineActivity`] lists.
const WORST_RENDER_FRAMES: usize = 10;

fn summarize_pipeline_frames(frames: &[RenderFrame]) -> Option<PipelineActivity> {
    if frames.is_empty() {
        return None;
    }
    let active =
        |frame: &RenderFrame| frame.pipelines.created > 0 || frame.pipelines.became_ready > 0;
    let mean = |values: Vec<f64>| {
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let mut worst: Vec<WorstRenderFrame> = frames
        .iter()
        .map(|frame| WorstRenderFrame {
            render_thread_ms: frame.render_thread_ms,
            pipelines: frame.pipelines,
        })
        .collect();
    worst.sort_by(|a, b| b.render_thread_ms.total_cmp(&a.render_thread_ms));
    worst.truncate(WORST_RENDER_FRAMES);
    Some(PipelineActivity {
        frames: frames.len(),
        frames_with_new_pipelines: frames.iter().filter(|f| f.pipelines.created > 0).count(),
        frames_with_pipelines_ready: frames
            .iter()
            .filter(|f| f.pipelines.became_ready > 0)
            .count(),
        pipelines_created: frames.iter().map(|f| f.pipelines.created as u64).sum(),
        pipelines_became_ready: frames.iter().map(|f| f.pipelines.became_ready as u64).sum(),
        render_thread_ms_mean_with_activity: mean(
            frames
                .iter()
                .filter(|f| active(f))
                .map(|f| f.render_thread_ms)
                .collect(),
        ),
        render_thread_ms_mean_without_activity: mean(
            frames
                .iter()
                .filter(|f| !active(f))
                .map(|f| f.render_thread_ms)
                .collect(),
        ),
        worst_frames: worst,
    })
}

impl RenderTimings {
    /// Keep samples from now on (true) or stop keeping them (false).
    pub fn set_recording(&self, recording: bool) {
        self.0.recording.store(recording, Ordering::Relaxed);
    }

    /// Every kept sample so far, in milliseconds, by name; the store is emptied.
    pub fn take_samples(&self) -> BTreeMap<&'static str, Vec<f64>> {
        std::mem::take(&mut self.state().samples)
    }

    /// The pipeline activity of every recorded render frame so far; the store is emptied. `None`
    /// when no frame was recorded.
    pub fn take_pipeline_activity(&self) -> Option<PipelineActivity> {
        summarize_pipeline_frames(&std::mem::take(&mut self.state().frames))
    }

    fn pipelines(&self, frame: PipelineFrame) {
        self.state().pipelines = Some(frame);
    }

    /// Keeps a [`CycleSides`] record per main-thread frame from now on (`track`), and says whether
    /// rendering is pipelined, which decides which cycle a render frame is filed under.
    pub fn set_cycle_tracking(&self, track: bool, pipelined: bool) {
        self.0.track_cycles.store(track, Ordering::Relaxed);
        self.state().pipelined = pipelined;
    }

    /// Whether this run keeps the per-cycle records (a benchmark run). Readable without the lock,
    /// so the laps inside the render graph can check it per view and, as a run condition, not even
    /// enter their systems when it is off.
    fn tracks_cycles(&self) -> bool {
        self.0.track_cycles.load(Ordering::Relaxed)
    }

    /// The record of the cycle before the one in progress, which is the cycle the frame time just
    /// measured belongs to: the duration that ends at the start of the current main frame is the
    /// previous cycle's. Taking it also drops anything older. `None` when nothing was recorded.
    pub fn take_previous_cycle(&self) -> Option<CycleSides> {
        let mut state = self.state();
        let current = state.cycle;
        let previous = current.checked_sub(1)?;
        let sides = state.cycles.remove(&previous);
        let newer = state.cycles.split_off(&current);
        state.cycles = newer;
        sides
    }

    /// The completed per-cycle rows not handed over yet, oldest first: the cycles at least one
    /// before the one in progress. A row is complete once the cycle after it has begun — its
    /// remainder window closed at that cycle's `First`, and the render schedule it owns ended
    /// before the hand-over that ended its own cycle (the render thread must finish for the main
    /// thread's next hand-over to extract into the render world). Taking drops the rows, and a
    /// row is never handed over twice, so a late write cannot duplicate it. The records behind
    /// the ledger's [`RenderTimings::take_previous_cycle`] are untouched.
    pub fn take_completed_cycles(&self) -> Vec<(u64, CycleSides)> {
        let mut state = self.state();
        let upto = state.cycle.saturating_sub(1);
        if upto <= state.cycle_rows_flushed_through {
            return Vec::new();
        }
        let drained: Vec<(u64, CycleSides)> = state
            .cycle_rows
            .range(state.cycle_rows_flushed_through + 1..=upto)
            .map(|(cycle, sides)| (*cycle, *sides))
            .collect();
        for (cycle, _) in &drained {
            state.cycle_rows.remove(cycle);
        }
        state.cycle_rows_flushed_through = upto;
        drained
    }

    /// Writes into the record of `cycle`, when cycles are tracked and the cycle is known. The
    /// mirror for the per-cycle series gets the same write; rows that can no longer be written or
    /// handed over are dropped, so a run that keeps no per-cycle series does not grow it.
    fn write_cycle(&self, state: &mut State, cycle: Option<u64>, write: impl Fn(&mut CycleSides)) {
        if self.tracks_cycles()
            && let Some(cycle) = cycle
        {
            write(state.cycles.entry(cycle).or_default());
            write(state.cycle_rows.entry(cycle).or_default());
            let keep_from = state.cycle.saturating_sub(2);
            state.cycle_rows.retain(|kept, _| *kept >= keep_from);
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn record(&self, state: &mut State, name: &'static str, elapsed: Duration) {
        if self.0.recording.load(Ordering::Relaxed) {
            state
                .samples
                .entry(name)
                .or_default()
                .push(elapsed.as_secs_f64() * 1000.0);
        }
    }

    fn extracted(&self, elapsed: Duration) {
        let extract_ms = elapsed.as_secs_f64() * 1000.0;
        let mut state = self.state();
        state.last_extract = Some(elapsed);
        let cycle = state.cycle;
        state.last_extract_cycle = Some(cycle);
        if state.remainder_started.is_some() {
            state.remainder_extract_ms += extract_ms;
        }
        self.write_cycle(&mut state, Some(cycle), |sides| {
            sides.extract_ms = Some(extract_ms);
        });
        self.record(&mut state, EXTRACT, elapsed);
    }

    /// The pipelined hand-over: waiting for the render world, then extracting into it.
    fn handed_over(&self, elapsed: Duration) {
        let mut state = self.state();
        if let Some(extract) = state.last_extract.take() {
            let wait = elapsed.saturating_sub(extract);
            let wait_ms = wait.as_secs_f64() * 1000.0;
            let cycle = state.cycle;
            if state.remainder_started.is_some() {
                state.remainder_wait_ms += wait_ms;
            }
            self.write_cycle(&mut state, Some(cycle), |sides| {
                sides.wait_ms = Some(wait_ms);
            });
            self.record(&mut state, WAIT_FOR_RENDER_THREAD, wait);
        }
    }

    /// Opens the cycle this `First` begins and closes the previous one's clocks (what
    /// [`start_main_world`] calls). `pub(crate)` so the join test between the two series' rows can
    /// drive the cycle numbering directly, without a render app.
    pub(crate) fn start_main(&self, real_ms: f64) {
        let now = Instant::now();
        let mut state = self.state();
        if let Some(started) = state.remainder_started.take() {
            let raw_ms = started.elapsed().as_secs_f64() * 1000.0;
            let ms = remainder_ms(raw_ms, state.remainder_extract_ms, state.remainder_wait_ms);
            let cycle = state.cycle;
            self.write_cycle(&mut state, Some(cycle), |sides| {
                sides.remainder_ms = Some(ms);
            });
            self.record(&mut state, REMAINDER, Duration::from_secs_f64(ms / 1000.0));
        }
        // The cycle that just ended takes its clocks, both closed by this `First` because a
        // cycle's duration is only known once the next one begins (as with the per-frame series'
        // rows): the wall lap from its own `First` to this one, measured on the main thread, and
        // the `Time<Real>` delta this `First` read — the same render-paced reading the joined
        // `frame-series.csv` row carries, the row numbered one below this cycle, so the two files
        // join clock for clock.
        if self.tracks_cycles() {
            let cycle = state.cycle;
            if let Some(started) = state.cycle_started.take() {
                let wall_ms = now.saturating_duration_since(started).as_secs_f64() * 1000.0;
                self.write_cycle(&mut state, Some(cycle), |sides| {
                    sides.wall_ms = Some(wall_ms);
                });
            }
            if real_ms.is_finite() && real_ms > 0.0 {
                self.write_cycle(&mut state, Some(cycle), |sides| {
                    sides.real_ms = Some(real_ms);
                });
            }
            state.cycle_started = Some(now);
        }
        state.main_started = Some(now);
        state.cycle += 1;
    }

    /// Closes the cycle this frame's main schedule ran in (what [`end_main_world`] calls);
    /// `pub(crate)` beside [`RenderTimings::start_main`].
    pub(crate) fn end_main(&self) {
        let mut state = self.state();
        if let Some(started) = state.main_started.take() {
            let elapsed = started.elapsed();
            let cycle = state.cycle;
            if self.tracks_cycles() {
                state.remainder_started = Some(Instant::now());
                state.remainder_extract_ms = 0.0;
                state.remainder_wait_ms = 0.0;
            }
            self.write_cycle(&mut state, Some(cycle), |sides| {
                sides.main_ms = Some(elapsed.as_secs_f64() * 1000.0);
            });
            self.record(&mut state, MAIN_WORLD, elapsed);
        }
    }

    fn start_render(&self) {
        let now = Instant::now();
        let mut state = self.state();
        state.render_started = Some(now);
        state.render_mark = Some(now);
        // The render world renders what the last extract gave it. With pipelining that happens
        // during the next cycle, without it inside the same one.
        let pipelined = state.pipelined;
        state.render_cycle = state
            .last_extract_cycle
            .map(|cycle| if pipelined { cycle + 1 } else { cycle });
    }

    fn lap(&self, name: &'static str) {
        let now = Instant::now();
        let mut state = self.state();
        if let Some(mark) = state.render_mark.replace(now) {
            let elapsed = now - mark;
            self.record(&mut state, name, elapsed);
            if let Some(set) = phase_writer(name) {
                let ms = elapsed.as_secs_f64() * 1000.0;
                let render_cycle = state.render_cycle;
                self.write_cycle(&mut state, render_cycle, |sides| set(sides, ms));
            }
        }
    }

    /// The root render graph schedule has begun (its `Begin` set): opens the mark
    /// [`RenderTimings::views_lapped`] closes. Nothing is recorded before that lap.
    fn graph_started(&self) {
        if !self.tracks_cycles() {
            return;
        }
        self.state().graph_mark = Some(Instant::now());
    }

    /// Every view's graph schedule has run (the root schedule's `Render` set, Bevy's
    /// `camera_driver`): closes [`GRAPH_VIEWS`] and re-arms the mark
    /// [`RenderTimings::submit_lapped`] closes.
    fn views_lapped(&self) {
        self.graph_lap(|sides, ms| sides.graph_views_ms = Some(ms));
    }

    /// The root schedule's `Submit` set has run (the graph's command buffers are with the queue):
    /// closes [`GRAPH_SUBMIT`].
    fn submit_lapped(&self) {
        self.graph_lap(|sides, ms| sides.graph_submit_ms = Some(ms));
    }

    /// Closes the root graph lap the previous boundary opened, re-arming its mark for the next one.
    fn graph_lap(&self, set: fn(&mut CycleSides, f64)) {
        if !self.tracks_cycles() {
            return;
        }
        let now = Instant::now();
        let mut state = self.state();
        let Some(mark) = state.graph_mark.replace(now) else {
            return;
        };
        let ms = (now - mark).as_secs_f64() * 1000.0;
        let render_cycle = state.render_cycle;
        self.write_cycle(&mut state, render_cycle, |sides| set(sides, ms));
    }

    /// A view's graph schedule (`Core3d`) has begun, before its `Prepass` set: opens the view's
    /// mark and counts the view.
    fn view_started(&self) {
        if !self.tracks_cycles() {
            return;
        }
        let mut state = self.state();
        state.view_mark = Some(Instant::now());
        let render_cycle = state.render_cycle;
        self.write_cycle(&mut state, render_cycle, |sides| {
            sides.graph_view_count = Some(sides.graph_view_count.unwrap_or(0) + 1);
        });
    }

    /// Closes the view lap opened at the view's previous boundary — the start, then each stage
    /// boundary — and adds it to the cycle's sum for that stage.
    fn view_lapped(&self, add: fn(&mut CycleSides, f64)) {
        if !self.tracks_cycles() {
            return;
        }
        let now = Instant::now();
        let mut state = self.state();
        let Some(mark) = state.view_mark.replace(now) else {
            return;
        };
        let ms = (now - mark).as_secs_f64() * 1000.0;
        let render_cycle = state.render_cycle;
        self.write_cycle(&mut state, render_cycle, |sides| add(sides, ms));
    }

    fn end_render(&self) {
        let mut state = self.state();
        state.render_mark = None;
        state.graph_mark = None;
        state.view_mark = None;
        let pipelines = state.pipelines.take();
        let render_cycle = state.render_cycle.take();
        if let Some(started) = state.render_started.take() {
            let elapsed = started.elapsed();
            self.write_cycle(&mut state, render_cycle, |sides| {
                sides.render_ms = Some(elapsed.as_secs_f64() * 1000.0);
            });
            self.record(&mut state, RENDER_THREAD, started.elapsed());
            if self.0.recording.load(Ordering::Relaxed)
                && let Some(pipelines) = pipelines
            {
                state.frames.push(RenderFrame {
                    render_thread_ms: started.elapsed().as_secs_f64() * 1000.0,
                    pipelines,
                });
            }
        }
    }

    fn start_acquire(&self) {
        self.state().acquire_started = Some(Instant::now());
    }

    fn end_acquire(&self) {
        let mut state = self.state();
        if let Some(started) = state.acquire_started.take() {
            let elapsed = started.elapsed();
            let render_cycle = state.render_cycle;
            self.write_cycle(&mut state, render_cycle, |sides| {
                sides.acquire_ms = Some(elapsed.as_secs_f64() * 1000.0);
            });
            self.record(&mut state, SWAPCHAIN_ACQUIRE, started.elapsed());
        }
    }
}

/// Opens the main world's sample. `pub(crate)` so the per-cycle series' drain can order itself
/// after the main frame has begun (a cycle's row is complete only once the next `First` has
/// closed its remainder window). Ordered after the time systems too, so the cycle's `Time<Real>`
/// delta is the same reading the per-frame series and the frame ledger read this frame — with the
/// time update unapplied the column would trail theirs by a frame.
pub(crate) fn start_main_world(timings: Res<RenderTimings>, real_time: Res<Time<Real>>) {
    timings.start_main(real_time.delta_secs_f64() * 1000.0);
}

/// Ends the main world's sample. A benchmark that stops recording in `Last` runs after this, so
/// the frame that completes the measurement keeps its `main_world` sample.
pub(crate) fn end_main_world(timings: Res<RenderTimings>) {
    timings.end_main();
}

/// Counts the pipelines (render and compute) this frame added to the cache and the ones that
/// stopped waiting.
/// `PipelineCache::pipelines` lists every pipeline ever queued (the cache never removes one), and
/// `waiting_pipelines` the ones still queued or being created, so the frame's new pipelines are the
/// growth of the first and the ones that finished are what the waiting set lost beyond the new.
/// Runs after the cache has processed its queue (`RenderSystems::Render`).
fn sample_pipelines(
    timings: Res<RenderTimings>,
    cache: Option<Res<PipelineCache>>,
    mut previous: Local<(usize, usize)>,
) {
    let Some(cache) = cache else {
        return;
    };
    let total = cache.pipelines().count();
    let waiting = cache.waiting_pipelines().count();
    let (previous_total, previous_waiting) = *previous;
    let frame = pipeline_frame(previous_total, previous_waiting, total, waiting);
    *previous = (total, waiting);
    timings.pipelines(frame);
}

fn pipeline_frame(
    previous_total: usize,
    previous_waiting: usize,
    total: usize,
    waiting: usize,
) -> PipelineFrame {
    let created = total.saturating_sub(previous_total);
    PipelineFrame {
        created,
        became_ready: (created + previous_waiting).saturating_sub(waiting),
        waiting,
    }
}

fn start_render_frame(timings: Res<RenderTimings>) {
    timings.start_render();
}

fn end_render_frame(timings: Res<RenderTimings>) {
    timings.end_render();
}

fn start_swapchain_acquire(timings: Res<RenderTimings>) {
    timings.start_acquire();
}

fn end_swapchain_acquire(timings: Res<RenderTimings>) {
    timings.end_acquire();
}

fn lap(name: &'static str) -> impl FnMut(Res<RenderTimings>) {
    move |timings: Res<RenderTimings>| timings.lap(name)
}

/// Whether this run keeps the per-cycle records (a benchmark run): the run condition of every lap
/// inside the render graph, so an ordinary play session pays one atomic load for them and not even
/// the system bodies (which check the same flag again).
fn tracks_cycles(timings: Res<RenderTimings>) -> bool {
    timings.tracks_cycles()
}

fn start_graph_frame(timings: Res<RenderTimings>) {
    timings.graph_started();
}

fn lap_graph_views(timings: Res<RenderTimings>) {
    timings.views_lapped();
}

fn lap_graph_submit(timings: Res<RenderTimings>) {
    timings.submit_lapped();
}

fn start_view(timings: Res<RenderTimings>) {
    timings.view_started();
}

fn lap_view_prepass(timings: Res<RenderTimings>) {
    timings.view_lapped(add_graph_prepass);
}

fn lap_view_main_pass(timings: Res<RenderTimings>) {
    timings.view_lapped(add_graph_main_pass);
}

fn lap_view_post_process(timings: Res<RenderTimings>) {
    timings.view_lapped(add_graph_post_process);
}

/// Adds a view's stage lap to the cycle's sum for that stage (the record holds one sum per stage,
/// over every view the cycle ran).
fn add_graph_prepass(sides: &mut CycleSides, ms: f64) {
    sides.graph_prepass_ms = Some(sides.graph_prepass_ms.unwrap_or(0.0) + ms);
}

fn add_graph_main_pass(sides: &mut CycleSides, ms: f64) {
    sides.graph_main_pass_ms = Some(sides.graph_main_pass_ms.unwrap_or(0.0) + ms);
}

fn add_graph_post_process(sides: &mut CycleSides, ms: f64) {
    sides.graph_post_process_ms = Some(sides.graph_post_process_ms.unwrap_or(0.0) + ms);
}

/// The main thread's time outside `main_world`, the extract and the wait in one cycle: the raw
/// `Last`→`First` window less the extract and the wait when both ran inside it (pipelined
/// rendering runs them there), never negative.
fn remainder_ms(raw_ms: f64, extract_ms: f64, wait_ms: f64) -> f64 {
    (raw_ms - extract_ms - wait_ms).max(0.0)
}

/// The part of `render/graph_and_present` the graph laps do not own (see [`GRAPH_REMAINDER`]): the
/// phase less [`GRAPH_VIEWS`] and [`GRAPH_SUBMIT`], never negative. The laps are read inside the
/// phase's own window on the render thread, so they can exceed it only by a timer tick; a larger
/// excess would be a bug in the laps, which the lap tests pin down, not something the clamp hides.
fn graph_remainder_ms(graph_and_present_ms: f64, sides: &CycleSides) -> f64 {
    let parts = sides.graph_views_ms.unwrap_or(0.0) + sides.graph_submit_ms.unwrap_or(0.0);
    (graph_and_present_ms - parts).max(0.0)
}

/// The cycle-record field a render-phase lap writes, by the lap's span name. The swapchain wait
/// is not a lap; [`RenderTimings::end_acquire`] writes it.
fn phase_writer(name: &str) -> Option<fn(&mut CycleSides, f64)> {
    match name {
        EXTRACT_COMMANDS_AND_ASSETS => {
            Some(|sides, ms| sides.extract_commands_and_assets_ms = Some(ms))
        }
        SPECIALIZE_AND_VIEWS => Some(|sides, ms| sides.specialize_and_views_ms = Some(ms)),
        QUEUE => Some(|sides, ms| sides.queue_ms = Some(ms)),
        PREPARE => Some(|sides, ms| sides.prepare_ms = Some(ms)),
        GRAPH_AND_PRESENT => Some(|sides, ms| {
            sides.graph_and_present_ms = Some(ms);
            sides.graph_remainder_ms = Some(graph_remainder_ms(ms, sides));
        }),
        CLEANUP => Some(|sides, ms| sides.cleanup_ms = Some(ms)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_samples_only_while_recording() {
        let timings = RenderTimings::default();
        timings.start_main(0.0);
        timings.end_main();
        assert!(timings.take_samples().is_empty());

        timings.set_recording(true);
        timings.start_main(0.0);
        timings.end_main();
        timings.set_recording(false);
        timings.start_main(0.0);
        timings.end_main();
        assert_eq!(timings.take_samples()[MAIN_WORLD].len(), 1);
        assert!(
            timings.take_samples().is_empty(),
            "taking empties the store"
        );
    }

    #[test]
    fn laps_split_the_render_frame_and_sum_to_it() {
        let timings = RenderTimings::default();
        timings.set_recording(true);
        timings.lap("render/queue");
        assert!(
            timings.take_samples().is_empty(),
            "a lap before the frame started has no mark"
        );

        timings.start_render();
        timings.lap("render/extract_commands_and_assets");
        std::thread::sleep(Duration::from_millis(2));
        timings.lap("render/queue");
        timings.end_render();
        timings.lap("render/cleanup");
        let samples = timings.take_samples();
        assert!(
            !samples.contains_key("render/cleanup"),
            "the frame had ended"
        );
        let queue = samples["render/queue"][0];
        let phases = samples["render/extract_commands_and_assets"][0] + queue;
        assert!(queue >= 2.0);
        assert!(samples[RENDER_THREAD][0] >= phases);
    }

    #[test]
    fn pipeline_counts_follow_the_cache() {
        // Nothing queued, nothing waiting.
        assert_eq!(pipeline_frame(10, 0, 10, 0), PipelineFrame::default());
        // Five queued this frame, none finished yet.
        assert_eq!(
            pipeline_frame(10, 0, 15, 5),
            PipelineFrame {
                created: 5,
                became_ready: 0,
                waiting: 5
            }
        );
        // Two new on top of five waiting; four are still waiting, so three finished.
        assert_eq!(
            pipeline_frame(15, 5, 17, 4),
            PipelineFrame {
                created: 2,
                became_ready: 3,
                waiting: 4
            }
        );
    }

    #[test]
    fn pipeline_activity_sets_active_frames_against_quiet_ones() {
        let frame = |ms, created, became_ready| RenderFrame {
            render_thread_ms: ms,
            pipelines: PipelineFrame {
                created,
                became_ready,
                waiting: 0,
            },
        };
        assert_eq!(summarize_pipeline_frames(&[]), None);
        let activity = summarize_pipeline_frames(&[
            frame(4.0, 0, 0),
            frame(6.0, 0, 0),
            frame(30.0, 3, 0),
            frame(10.0, 0, 2),
        ])
        .unwrap();
        assert_eq!(activity.frames, 4);
        assert_eq!(activity.frames_with_new_pipelines, 1);
        assert_eq!(activity.frames_with_pipelines_ready, 1);
        assert_eq!(activity.pipelines_created, 3);
        assert_eq!(activity.pipelines_became_ready, 2);
        assert_eq!(activity.render_thread_ms_mean_with_activity, Some(20.0));
        assert_eq!(activity.render_thread_ms_mean_without_activity, Some(5.0));
        assert_eq!(activity.worst_frames[0].render_thread_ms, 30.0);
        assert_eq!(activity.worst_frames[0].pipelines.created, 3);
    }

    #[test]
    fn render_frames_keep_their_pipeline_counts_only_while_recording() {
        let timings = RenderTimings::default();
        timings.start_render();
        timings.pipelines(PipelineFrame {
            created: 1,
            ..default()
        });
        timings.end_render();
        assert!(timings.take_pipeline_activity().is_none());

        timings.set_recording(true);
        timings.start_render();
        timings.pipelines(PipelineFrame {
            created: 2,
            became_ready: 1,
            waiting: 1,
        });
        timings.end_render();
        let activity = timings.take_pipeline_activity().unwrap();
        assert_eq!(activity.frames, 1);
        assert_eq!(activity.pipelines_created, 2);
    }

    /// A frame time is the duration of the previous cycle, so that is the record asked for, and a
    /// pipelined render frame belongs to the cycle after the extract that fed it.
    #[test]
    fn cycles_file_a_render_frame_under_the_cycle_it_ran_in() {
        for pipelined in [true, false] {
            let timings = RenderTimings::default();
            timings.set_cycle_tracking(true, pipelined);
            assert_eq!(timings.take_previous_cycle(), None, "no frame yet");
            // Main frame 1, its extract, and the render frame that extract fed.
            timings.start_main(0.0);
            timings.end_main();
            timings.extracted(Duration::from_millis(1));
            timings.start_render();
            timings.start_acquire();
            timings.end_acquire();
            timings.end_render();
            // Main frame 2 starts: the frame time it measures is cycle 1's.
            timings.start_main(0.0);
            let first = timings.take_previous_cycle().expect("cycle 1");
            assert!(first.main_ms.is_some());
            assert_eq!(first.extract_ms, Some(1.0));
            assert_eq!(
                (first.render_ms.is_some(), first.acquire_ms.is_some()),
                (!pipelined, !pipelined),
                "pipelined: the render frame is in cycle 2; otherwise it ran inside cycle 1"
            );
            // Frame 2 runs, extracts, and frame 3 starts: cycle 2 is the previous one.
            timings.end_main();
            timings.extracted(Duration::from_millis(2));
            timings.start_main(0.0);
            let second = timings.take_previous_cycle().expect("cycle 2");
            assert_eq!(second.extract_ms, Some(2.0));
            assert_eq!(
                (second.render_ms.is_some(), second.acquire_ms.is_some()),
                (pipelined, pipelined),
                "the render frame of extract 1 ran during cycle 2 when pipelined"
            );
            assert_eq!(timings.take_previous_cycle(), None, "taken once");
        }
    }

    /// The hand-over wait is filed under the main frame that waited, beside its extract.
    #[test]
    fn cycles_keep_the_main_threads_wait_for_the_render_thread() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, true);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(3));
        timings.handed_over(Duration::from_millis(10));
        timings.start_main(0.0);
        let cycle = timings.take_previous_cycle().expect("cycle 1");
        assert_eq!(cycle.extract_ms, Some(3.0));
        assert_eq!(cycle.wait_ms, Some(7.0));
        // A hand-over without a new extract has nothing to attribute.
        timings.end_main();
        timings.handed_over(Duration::from_millis(10));
        timings.start_main(0.0);
        let next = timings.take_previous_cycle().expect("cycle 2");
        assert!(next.main_ms.is_some());
        assert_eq!(next.wait_ms, None);
    }

    #[test]
    fn cycles_are_kept_only_for_a_run_that_tracks_them() {
        let timings = RenderTimings::default();
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_main(0.0);
        assert_eq!(timings.take_previous_cycle(), None);
    }

    #[test]
    fn taking_a_cycle_drops_the_older_ones() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, false);
        for _ in 0..5 {
            timings.start_main(0.0);
            timings.end_main();
        }
        timings.start_main(0.0);
        assert!(timings.take_previous_cycle().is_some());
        assert!(
            timings.state().cycles.is_empty(),
            "cycles 1 to 4 were older than the one taken"
        );
    }

    #[test]
    fn the_wait_is_the_hand_over_less_its_extract() {
        let timings = RenderTimings::default();
        timings.set_recording(true);
        timings.extracted(Duration::from_millis(3));
        timings.handed_over(Duration::from_millis(10));
        timings.handed_over(Duration::from_millis(10));
        let samples = timings.take_samples();
        assert_eq!(samples[EXTRACT], vec![3.0]);
        assert_eq!(
            samples[WAIT_FOR_RENDER_THREAD],
            vec![7.0],
            "a hand-over without a new extract is not counted"
        );
    }

    /// The remainder is the raw `Last`→`First` window less the extract and the wait when both ran
    /// inside it, and never negative.
    #[test]
    fn the_remainder_excludes_the_extract_and_wait_that_ran_inside_the_window() {
        assert_eq!(remainder_ms(20.0, 3.0, 7.0), 10.0);
        assert_eq!(
            remainder_ms(10.0, 3.0, 7.0),
            0.0,
            "the window was extract and wait and nothing else"
        );
        assert_eq!(
            remainder_ms(5.0, 3.0, 7.0),
            0.0,
            "the measured sides can exceed the window by less than a timer tick"
        );
        assert_eq!(
            remainder_ms(20.0, 0.0, 0.0),
            20.0,
            "nothing ran in the window but the remainder"
        );
    }

    /// The same, driven through the record: the window held nothing but the extract and the wait,
    /// so the cycle's remainder is zero.
    #[test]
    fn the_cycle_remainder_excludes_the_extract_and_wait_that_ran_in_its_window() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, true);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(3));
        timings.handed_over(Duration::from_millis(10));
        timings.start_main(0.0);
        let cycle = timings.take_previous_cycle().expect("cycle 1");
        assert_eq!(cycle.extract_ms, Some(3.0));
        assert_eq!(cycle.wait_ms, Some(7.0));
        assert_eq!(
            cycle.remainder_ms,
            Some(0.0),
            "the window was extract and wait and nothing else"
        );
        assert_eq!(
            cycle.real_ms, None,
            "a `First` that read no positive delta keeps the clock column empty"
        );
    }

    /// The identity that makes the next re-profile trustworthy: `main_world + extract + wait +
    /// remainder` is the whole cycle — the wall lap from one main frame's start to the next's,
    /// which the row carries as `wall_cycle_ms` — and the row's `real_ms` is the `Time<Real>`
    /// delta the closing `First` read, the same lagged reading the per-frame series joins with.
    #[test]
    fn the_cycle_sides_account_for_the_whole_cycle() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, true);
        timings.start_main(16.0);
        std::thread::sleep(Duration::from_millis(4));
        timings.end_main();
        let window = Instant::now();
        std::thread::sleep(Duration::from_millis(16));
        timings.extracted(Duration::from_millis(1));
        timings.handed_over(Duration::from_millis(9)); // 1 ms of extract, 8 of wait
        timings.start_main(16.0);
        let cycle = timings.take_previous_cycle().expect("cycle 1");
        let main_ms = cycle.main_ms.expect("the main schedule ran");
        let window_ms = window.elapsed().as_secs_f64() * 1000.0;
        let accounted =
            cycle.extract_ms.unwrap() + cycle.wait_ms.unwrap() + cycle.remainder_ms.unwrap();
        assert!(
            (accounted - window_ms).abs() < 2.0,
            "extract + wait + remainder must be the raw window: {accounted} vs {window_ms}"
        );
        assert!(
            main_ms >= 4.0,
            "the main schedule's sample covers its sleep"
        );
        assert!(
            cycle.remainder_ms.unwrap() >= 6.0,
            "the window held sleep on top of the extract and the wait"
        );
        let wall = cycle.wall_ms.expect("the next `First` closed the wall lap");
        assert!(
            (wall - (main_ms + window_ms)).abs() < 2.0,
            "the wall lap is the main schedule plus the window after it: {wall} vs \
             {main_ms} + {window_ms}"
        );
        assert!(
            (main_ms + accounted - wall).abs() < 2.0,
            "the sides must account for the whole wall lap: {main_ms} + {accounted} vs {wall}"
        );
        assert_eq!(
            cycle.real_ms,
            Some(16.0),
            "the row carries the Time<Real> delta the closing First read"
        );
    }

    /// Without cycle tracking no window opens, so no remainder is written into a record or
    /// sampled, whatever the recording state.
    #[test]
    fn the_remainder_is_kept_only_when_cycles_are_tracked() {
        let timings = RenderTimings::default();
        timings.set_recording(true);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.handed_over(Duration::from_millis(5));
        timings.start_main(0.0);
        let samples = timings.take_samples();
        assert!(
            !samples.contains_key(REMAINDER),
            "no tracked cycle, no remainder window"
        );
        assert!(
            samples.contains_key(EXTRACT),
            "the extract is still sampled"
        );
        assert!(timings.take_completed_cycles().is_empty());
    }

    /// The render phases land in the cycle the render frame belongs to: inside cycle 1 without
    /// pipelining, under cycle 2 with it (the render schedule of extract 1 runs alongside main
    /// frame 2), and they sum to the render schedule they are phases of.
    #[test]
    fn laps_write_their_phase_into_the_cycle_record() {
        for pipelined in [true, false] {
            let timings = RenderTimings::default();
            timings.set_cycle_tracking(true, pipelined);
            timings.start_main(0.0);
            timings.end_main();
            timings.extracted(Duration::from_millis(1));
            timings.start_render();
            timings.lap(EXTRACT_COMMANDS_AND_ASSETS);
            timings.lap(SPECIALIZE_AND_VIEWS);
            timings.lap(QUEUE);
            timings.lap(PREPARE);
            timings.lap(GRAPH_AND_PRESENT);
            timings.lap(CLEANUP);
            timings.end_render();
            timings.start_main(0.0);
            let first = timings.take_previous_cycle().expect("cycle 1");
            let (has_phases, has_render) = (
                first.extract_commands_and_assets_ms.is_some(),
                first.render_ms.is_some(),
            );
            assert_eq!(
                (has_phases, has_render),
                (!pipelined, !pipelined),
                "pipelined files the render frame under cycle 2"
            );
            if pipelined {
                timings.end_main();
                timings.extracted(Duration::from_millis(1));
                timings.start_main(0.0);
            }
            let second = if pipelined {
                timings.take_previous_cycle().expect("cycle 2")
            } else {
                first
            };
            let phases = [
                second.extract_commands_and_assets_ms,
                second.specialize_and_views_ms,
                second.queue_ms,
                second.prepare_ms,
                second.graph_and_present_ms,
                second.cleanup_ms,
            ];
            assert!(
                phases.iter().all(Option::is_some),
                "every lapped phase is in the cycle's record"
            );
            assert!(
                second.acquire_ms.is_none()
                    || second.acquire_ms.unwrap() <= second.render_ms.unwrap(),
                "the swapchain wait sits inside the render schedule"
            );
            let sum: f64 = phases.into_iter().flatten().sum();
            assert!(
                sum <= second.render_ms.unwrap() + 0.1,
                "the phases are parts of the render schedule: {sum} vs {}",
                second.render_ms.unwrap()
            );
        }
    }

    /// The graph laps split `render/graph_and_present` the way R2 asks: the view schedules, the
    /// graph's queue submit and a remainder that closes the phase. The parts are taken inside the
    /// outer lap, so they cannot exceed it, and the remainder is what makes the row's graph columns
    /// add up to `render/graph_and_present`. The per-view stage sums are bounded by the views lap
    /// that includes every view schedule's own overhead, and the count says how many views they
    /// cover.
    #[test]
    fn the_graph_laps_account_for_graph_and_present() {
        let timings = RenderTimings::default();
        timings.set_recording(true);
        timings.set_cycle_tracking(true, true);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_render();
        // The root graph schedule: its `Begin` set, two view schedules, the lap after the `Render`
        // set, the lap after the `Submit` set, then the phase's own lap (`Set::Render`).
        timings.graph_started();
        std::thread::sleep(Duration::from_millis(1));
        for _ in 0..2 {
            timings.view_started();
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_prepass);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_main_pass);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_post_process);
        }
        timings.views_lapped();
        std::thread::sleep(Duration::from_millis(1));
        timings.submit_lapped();
        std::thread::sleep(Duration::from_millis(1));
        timings.lap(GRAPH_AND_PRESENT);
        timings.end_render();
        // Pipelined: the render frame of extract 1 is filed under cycle 2.
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_main(0.0);
        let cycle = timings.take_previous_cycle().expect("cycle 2");
        let graph = cycle.graph_and_present_ms.expect("the phase was lapped");
        let views = cycle.graph_views_ms.expect("the views lap ran");
        let submit = cycle.graph_submit_ms.expect("the submit lap ran");
        let remainder = cycle
            .graph_remainder_ms
            .expect("the phase's lap writes the remainder");
        assert_eq!(cycle.graph_view_count, Some(2), "two view schedules ran");
        assert!(
            views >= 2.0 && submit >= 1.0,
            "the laps cover the sleeps between their boundaries: {views} views, {submit} submit"
        );
        assert!(
            views + submit <= graph,
            "the laps were taken inside the phase: {views} + {submit} vs {graph}"
        );
        assert!(
            (views + submit + remainder - graph).abs() < 1e-9,
            "the parts and the remainder are the phase: {views} + {submit} + {remainder} vs \
             {graph}"
        );
        assert!(
            remainder >= 1.0,
            "the sleep between the submit lap and the phase's lap is in the remainder: {remainder}"
        );
        let stages = [
            cycle.graph_prepass_ms.expect("prepass laps ran"),
            cycle.graph_main_pass_ms.expect("main-pass laps ran"),
            cycle.graph_post_process_ms.expect("post-process laps ran"),
        ];
        for stage in stages {
            assert!(
                stage >= 2.0,
                "each sum covers both views' sleep for its stage: {stage}"
            );
        }
        assert!(
            stages.iter().sum::<f64>() <= views,
            "the per-view stages are parts of the views lap: {stages:?} vs {views}"
        );
        assert!(
            timings.take_samples().contains_key(GRAPH_AND_PRESENT),
            "the phase keeps its sample"
        );
    }

    /// The graph laps cost nothing when the run keeps no cycles: no mark is taken, nothing is
    /// written and nothing is sampled, whatever the recording state (a lap inside the render graph
    /// has no per-frame sample of its own — its column is the cycle's sum).
    #[test]
    fn the_graph_laps_are_kept_only_when_cycles_are_tracked() {
        let timings = RenderTimings::default();
        timings.set_recording(true);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_render();
        timings.graph_started();
        timings.view_started();
        timings.view_lapped(add_graph_prepass);
        timings.view_lapped(add_graph_main_pass);
        timings.view_lapped(add_graph_post_process);
        timings.views_lapped();
        timings.submit_lapped();
        assert!(
            timings.state().graph_mark.is_none() && timings.state().view_mark.is_none(),
            "no mark is taken when cycles are not tracked"
        );
        timings.lap(GRAPH_AND_PRESENT);
        timings.end_render();
        assert!(
            timings.take_previous_cycle().is_none() && timings.take_completed_cycles().is_empty(),
            "no cycle record is kept"
        );
        let samples = timings.take_samples();
        assert!(samples.contains_key(GRAPH_AND_PRESENT));
        for lap in [
            GRAPH_VIEWS,
            GRAPH_VIEW_COUNT,
            GRAPH_PREPASS,
            GRAPH_MAIN_PASS,
            GRAPH_POST_PROCESS,
            GRAPH_SUBMIT,
            GRAPH_REMAINDER,
        ] {
            assert!(
                !samples.contains_key(lap),
                "the graph lap {lap} is a cycle column, not a sample"
            );
        }
    }

    /// The remainder is the phase less the laps inside it, and never negative — the laps are read
    /// inside the phase's window, so a small excess is a timer tick, not a negative remainder.
    #[test]
    fn the_graph_remainder_never_goes_negative() {
        let sides = CycleSides {
            graph_views_ms: Some(6.0),
            graph_submit_ms: Some(3.0),
            ..Default::default()
        };
        assert_eq!(graph_remainder_ms(10.0, &sides), 1.0);
        assert_eq!(
            graph_remainder_ms(8.5, &sides),
            0.0,
            "the laps can exceed the phase by less than a timer tick"
        );
        assert_eq!(
            graph_remainder_ms(9.5, &sides),
            0.5,
            "the remainder is what the laps leave"
        );
        assert_eq!(
            graph_remainder_ms(4.0, &CycleSides::default()),
            4.0,
            "no lap was measured: the whole phase is the remainder"
        );
    }

    /// Completed rows are handed over once, oldest first, complete (main, remainder); the
    /// ledger's takes do not touch them, and a second take is empty.
    #[test]
    fn completed_cycle_rows_are_handed_over_once_once_they_are_complete() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, true);
        for _ in 0..3 {
            timings.start_main(0.0);
            timings.end_main();
        }
        // Cycles 1 and 2 are complete (the cycle after each has begun); cycle 3 is the one in
        // progress and stays out.
        let rows = timings.take_completed_cycles();
        assert_eq!(
            rows.iter().map(|(cycle, _)| *cycle).collect::<Vec<_>>(),
            vec![1, 2]
        );
        for (_, sides) in &rows {
            assert!(sides.main_ms.is_some());
            assert!(sides.remainder_ms.is_some(), "the window closed");
        }
        timings.start_main(0.0); // cycle 4 begins: cycle 3's remainder window closed
        let rows = timings.take_completed_cycles();
        assert_eq!(
            rows.iter().map(|(cycle, _)| *cycle).collect::<Vec<_>>(),
            vec![3]
        );
        assert!(rows[0].1.main_ms.is_some() && rows[0].1.remainder_ms.is_some());
        // The ledger still finds its record; the rows are not handed over twice.
        assert!(timings.take_previous_cycle().is_some());
        assert!(timings.take_completed_cycles().is_empty());
    }

    /// A phase write that lands in a cycle after it was handed over cannot put its row into the
    /// series a second time: the take starts above the ledger's high-water mark, so the file never
    /// carries two rows for one cycle and a later take yields the cycles after it alone.
    #[test]
    fn a_late_write_to_a_handed_over_cycle_cannot_duplicate_its_row() {
        let timings = RenderTimings::default();
        // Not pipelined: the render frame is filed under the cycle its extract belongs to, so the
        // late laps below reach a cycle that is already handed over. (Pipelined, a render frame is
        // always filed under the next cycle, which no take has closed yet.)
        timings.set_cycle_tracking(true, false);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_main(0.0); // cycle 2 begins: cycle 1 is complete
        let rows = timings.take_completed_cycles();
        assert_eq!(
            rows.iter().map(|(cycle, _)| *cycle).collect::<Vec<_>>(),
            vec![1],
            "cycle 1 is handed over"
        );

        // A late render frame of cycle 1's extract laps after the hand-over, which writes into
        // cycle 1 again (the series' mirror takes the row it can still write).
        timings.start_render();
        timings.lap(QUEUE);
        timings.lap(PREPARE);
        timings.end_render();
        assert!(
            timings.take_completed_cycles().is_empty(),
            "a late write to cycle 1 cannot hand its row over a second time"
        );

        // The next take yields cycle 2 alone, carrying cycle 2's own record rather than the late
        // write's, and cycle 1 is not handed over again.
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_main(0.0); // cycle 3 begins: cycle 2 is complete
        let rows = timings.take_completed_cycles();
        assert_eq!(
            rows.iter().map(|(cycle, _)| *cycle).collect::<Vec<_>>(),
            vec![2],
            "only the newly completed cycle is handed over"
        );
        assert!(
            rows[0].1.queue_ms.is_none() && rows[0].1.wall_ms.is_some(),
            "the row is cycle 2's own record, not the late write to cycle 1"
        );
    }
}
