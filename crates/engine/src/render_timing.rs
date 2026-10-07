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
//!   `render/prepare`, `render/graph_and_present`, `render/cleanup`.
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
//!   measures. With pipelined rendering the render schedule that follows the extract of main frame
//!   `n` runs during cycle `n + 1` (alongside main frame `n + 1`), so it is filed under `n + 1`;
//!   without it the render schedule runs inside cycle `n`. See [`CycleSides`].
//!
//! * `frame/remainder`: the main-thread time of a cycle that none of the sides above own — the
//!   window from the end of `Last` to the start of the next `First`, less the extract and the wait
//!   for the render thread when both ran inside it (pipelined rendering runs them there). This is
//!   the region the 2026-10-08 door-tour baseline found unaccounted on the door-crossing frames
//!   (`local/research/portal-crossing-frames-895-2666-2026-10-08.md`). Kept only while cycles are
//!   tracked (a benchmark run); on top of that it is sampled as `frame/remainder` while recording.
//!   The per-cycle records are also handed over as rows for the profile bundle's
//!   `render-cycles.csv` (see `profiling`), which joins a `frame-series.csv` row with the same
//!   frame number under the `n → n + 1` offset described above.

use bevy::{
    app::{First, Last},
    platform::time::Instant,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems, pipelined_rendering::RenderExtractApp,
        render_resource::PipelineCache, view::prepare_windows,
    },
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
/// The render schedule's phases, as span names; the per-cycle record keeps one value each (the
/// distributions live in the samples). Shared with the per-cycle CSV's column list, so the two
/// cannot drift apart.
pub const EXTRACT_COMMANDS_AND_ASSETS: &str = "render/extract_commands_and_assets";
pub const SPECIALIZE_AND_VIEWS: &str = "render/specialize_and_views";
pub const QUEUE: &str = "render/queue";
pub const PREPARE: &str = "render/prepare";
pub const GRAPH_AND_PRESENT: &str = "render/graph_and_present";
pub const CLEANUP: &str = "render/cleanup";

/// Measures the render side of each frame; shared by the main world, the render world and the
/// extract wrappers.
pub struct RenderTimingPlugin;

impl Plugin for RenderTimingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderTimings>()
            .add_systems(First, start_main_world)
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
}

/// The shared clock: a handle cloned into both worlds and the extract wrappers.
#[derive(Resource, Clone, Default)]
pub struct RenderTimings(Arc<Shared>);

#[derive(Default)]
struct Shared {
    recording: AtomicBool,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    samples: BTreeMap<&'static str, Vec<f64>>,
    main_started: Option<Instant>,
    render_started: Option<Instant>,
    render_mark: Option<Instant>,
    acquire_started: Option<Instant>,
    last_extract: Option<Duration>,
    /// Pipeline counts for the render frame in progress, set just before it ends.
    pipelines: Option<PipelineFrame>,
    frames: Vec<RenderFrame>,
    /// Whether the cycle records below are kept (a benchmark run only).
    track_cycles: bool,
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
        let mut state = self.state();
        state.track_cycles = track;
        state.pipelined = pipelined;
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
    fn write_cycle(state: &mut State, cycle: Option<u64>, write: impl Fn(&mut CycleSides)) {
        if state.track_cycles
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
        Self::write_cycle(&mut state, Some(cycle), |sides| {
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
            Self::write_cycle(&mut state, Some(cycle), |sides| {
                sides.wait_ms = Some(wait_ms);
            });
            self.record(&mut state, WAIT_FOR_RENDER_THREAD, wait);
        }
    }

    fn start_main(&self) {
        let mut state = self.state();
        if let Some(started) = state.remainder_started.take() {
            let raw_ms = started.elapsed().as_secs_f64() * 1000.0;
            let ms = remainder_ms(raw_ms, state.remainder_extract_ms, state.remainder_wait_ms);
            let cycle = state.cycle;
            Self::write_cycle(&mut state, Some(cycle), |sides| {
                sides.remainder_ms = Some(ms);
            });
            self.record(&mut state, REMAINDER, Duration::from_secs_f64(ms / 1000.0));
        }
        state.main_started = Some(Instant::now());
        state.cycle += 1;
    }

    fn end_main(&self) {
        let mut state = self.state();
        if let Some(started) = state.main_started.take() {
            let elapsed = started.elapsed();
            let cycle = state.cycle;
            if state.track_cycles {
                state.remainder_started = Some(Instant::now());
                state.remainder_extract_ms = 0.0;
                state.remainder_wait_ms = 0.0;
            }
            Self::write_cycle(&mut state, Some(cycle), |sides| {
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
                Self::write_cycle(&mut state, render_cycle, |sides| set(sides, ms));
            }
        }
    }

    fn end_render(&self) {
        let mut state = self.state();
        state.render_mark = None;
        let pipelines = state.pipelines.take();
        let render_cycle = state.render_cycle.take();
        if let Some(started) = state.render_started.take() {
            let elapsed = started.elapsed();
            Self::write_cycle(&mut state, render_cycle, |sides| {
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
            Self::write_cycle(&mut state, render_cycle, |sides| {
                sides.acquire_ms = Some(elapsed.as_secs_f64() * 1000.0);
            });
            self.record(&mut state, SWAPCHAIN_ACQUIRE, started.elapsed());
        }
    }
}

/// Opens the main world's sample. `pub(crate)` so the per-cycle series' drain can order itself
/// after the main frame has begun (a cycle's row is complete only once the next `First` has
/// closed its remainder window).
pub(crate) fn start_main_world(timings: Res<RenderTimings>) {
    timings.start_main();
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

/// The main thread's time outside `main_world`, the extract and the wait in one cycle: the raw
/// `Last`→`First` window less the extract and the wait when both ran inside it (pipelined
/// rendering runs them there), never negative.
fn remainder_ms(raw_ms: f64, extract_ms: f64, wait_ms: f64) -> f64 {
    (raw_ms - extract_ms - wait_ms).max(0.0)
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
        GRAPH_AND_PRESENT => Some(|sides, ms| sides.graph_and_present_ms = Some(ms)),
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
        timings.start_main();
        timings.end_main();
        assert!(timings.take_samples().is_empty());

        timings.set_recording(true);
        timings.start_main();
        timings.end_main();
        timings.set_recording(false);
        timings.start_main();
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
            timings.start_main();
            timings.end_main();
            timings.extracted(Duration::from_millis(1));
            timings.start_render();
            timings.start_acquire();
            timings.end_acquire();
            timings.end_render();
            // Main frame 2 starts: the frame time it measures is cycle 1's.
            timings.start_main();
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
            timings.start_main();
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
        timings.start_main();
        timings.end_main();
        timings.extracted(Duration::from_millis(3));
        timings.handed_over(Duration::from_millis(10));
        timings.start_main();
        let cycle = timings.take_previous_cycle().expect("cycle 1");
        assert_eq!(cycle.extract_ms, Some(3.0));
        assert_eq!(cycle.wait_ms, Some(7.0));
        // A hand-over without a new extract has nothing to attribute.
        timings.end_main();
        timings.handed_over(Duration::from_millis(10));
        timings.start_main();
        let next = timings.take_previous_cycle().expect("cycle 2");
        assert!(next.main_ms.is_some());
        assert_eq!(next.wait_ms, None);
    }

    #[test]
    fn cycles_are_kept_only_for_a_run_that_tracks_them() {
        let timings = RenderTimings::default();
        timings.start_main();
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_main();
        assert_eq!(timings.take_previous_cycle(), None);
    }

    #[test]
    fn taking_a_cycle_drops_the_older_ones() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, false);
        for _ in 0..5 {
            timings.start_main();
            timings.end_main();
        }
        timings.start_main();
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
        timings.start_main();
        timings.end_main();
        timings.extracted(Duration::from_millis(3));
        timings.handed_over(Duration::from_millis(10));
        timings.start_main();
        let cycle = timings.take_previous_cycle().expect("cycle 1");
        assert_eq!(cycle.extract_ms, Some(3.0));
        assert_eq!(cycle.wait_ms, Some(7.0));
        assert_eq!(
            cycle.remainder_ms,
            Some(0.0),
            "the window was extract and wait and nothing else"
        );
    }

    /// The identity that makes the next re-profile trustworthy: `main_world + extract + wait +
    /// remainder` is the whole cycle, the wall time from one main frame's start to the next's.
    #[test]
    fn the_cycle_sides_account_for_the_whole_cycle() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, true);
        timings.start_main();
        std::thread::sleep(Duration::from_millis(4));
        timings.end_main();
        let window = Instant::now();
        std::thread::sleep(Duration::from_millis(16));
        timings.extracted(Duration::from_millis(1));
        timings.handed_over(Duration::from_millis(9)); // 1 ms of extract, 8 of wait
        timings.start_main();
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
    }

    /// Without cycle tracking no window opens, so no remainder is written into a record or
    /// sampled, whatever the recording state.
    #[test]
    fn the_remainder_is_kept_only_when_cycles_are_tracked() {
        let timings = RenderTimings::default();
        timings.set_recording(true);
        timings.start_main();
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.handed_over(Duration::from_millis(5));
        timings.start_main();
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
            timings.start_main();
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
            timings.start_main();
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
                timings.start_main();
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

    /// Completed rows are handed over once, oldest first, complete (main, remainder); the
    /// ledger's takes do not touch them, and a second take is empty.
    #[test]
    fn completed_cycle_rows_are_handed_over_once_once_they_are_complete() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, true);
        for _ in 0..3 {
            timings.start_main();
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
        timings.start_main(); // cycle 4 begins: cycle 3's remainder window closed
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
}
