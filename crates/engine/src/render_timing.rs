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
//!   `Core3d`, whose `Prepass`, `MainPass`, `EarlyPostProcess` and `PostProcess` sets hold the
//!   shadow and depth prepasses, the main opaque and transparent passes and the post-processing
//!   (`bevy_core_pipeline-0.19.0` `src/core_3d/mod.rs:145`), with the GPU binning, the bin
//!   unpacking and the indirect-draw building ordered before the prepass but in no set (see below)
//!   and the upscaling system after `PostProcess`. The *laps* are ours, the boundaries are
//!   Bevy's sets: they are ordinary systems added to those schedules and ordered between the sets,
//!   so no Bevy code is forked or patched. (The gap between the root `Render` and `Submit` sets also
//!   holds Bevy's diagnostics-encoder resolve when `RenderDiagnosticsPlugin` is added — the engine
//!   always adds it — so the views lap can carry that few microseconds.) Per cycle the record keeps
//!   `render/graph/views` (the root
//!   `Render` set: every view's schedule, the part of the phase that scales with views),
//!   `render/graph/view_count` (how many view schedules the cycle ran — a count, not a duration),
//!   the sums over those views of `render/graph/prepass`, `render/graph/main_pass`,
//!   `render/graph/early_post_process` and `render/graph/post_process` (the post-process sum covers
//!   `EarlyPostProcess`, `PostProcess` and the upscaling window together, as it covered the first
//!   two before the early stage got its own column; the early column is that first stage alone and
//!   `render/graph/upscaling` the upscaling window alone),
//!   `render/graph/gpu_preprocess` (the camera runs' early GPU-preprocessing windows, a column of
//!   their own so the prepass sum stays exclusive of them — see below),
//!   `render/graph/shadow_passes` (the same views' sun-shadow
//!   pass windows, a column of their own so the stage sums stay exclusive of them — see below),
//!   `render/graph/submit` (the graph command buffers'
//!   `queue.submit` and the uncovered-window clear) and `render/graph/remainder`, which closes the
//!   phase: everything the laps do not own — the schedule's own `Begin`/`Finish` sets and overhead,
//!   and, after the schedule, the screenshot and readback submit, the `queue.submit` of their
//!   encoder and `present` on each window (`bevy_render-0.19.0` `src/renderer/mod.rs:78-110`). Two
//!   parts are reported as not split rather than guessed: `present` cannot be separated from that
//!   remainder without patching Bevy or building it with its `trace` feature, and the tail's
//!   `msaa_writeback` and `ui_pass` are `pub(crate)` or unexported, so they and the end-of-run
//!   deferred flush stay in `render/graph/driver_overhead` (see below).
//!
//!   `render/graph/views` itself is split by the kind of root view `camera_driver` ran, because the
//!   loop runs two kinds: a **camera** view (an entry of `SortedCameras`) and an **auxiliary** root
//!   view — `RootNonCameraView`, which in Bevy 0.19 is a point or spot light's shadow map
//!   (`bevy_core_pipeline-0.19.0` `src/schedule.rs:115-123`, `:136`; `bevy_pbr-0.19.0`
//!   `src/render/light.rs:2064-2094`, `:2150-2175`). A run is counted as a shadow view when the
//!   entity the schedule is running for carries `ShadowView` (`bevy_pbr-0.19.0`
//!   `src/render/light.rs:928`), the marker of a pass that draws a shadow map: the cycle keeps
//!   `render/graph/shadow_views` and `render/graph/camera_views` (each the view kind's own time,
//!   the sum of that run's stage laps), `render/graph/shadow_view_count` (how many of
//!   `render/graph/view_count`'s runs were shadow views) and `render/graph/views_remainder` =
//!   `views` less those two sums, never negative — what the loop itself spends between runs plus
//!   whatever a run spends outside the lap windows (its schedule's own setup and teardown, and the
//!   stage systems the executor orders outside them). A **directional** light's cascade views are
//!   not root views: they are `ShadowView` entities the camera's own `Core3d` run renders through
//!   `per_view_shadow_pass` and `shared_shadow_pass` (`bevy_pbr-0.19.0` `src/lib.rs:424-444`,
//!   `src/render/light.rs:2762-2805`), so a camera's sun-shadow passes run inside its own graph
//!   schedule and their time is inside `render/graph/camera_views`, not in the shadow-view
//!   columns. They are lapped there as `render/graph/shadow_passes` ([`GRAPH_SHADOW_PASSES`]).
//!
//!   Bevy adds those four shadow-pass systems to `Core3d` with `after`/`before` against systems
//!   that are *not* in any of its stage sets (`bevy_pbr-0.19.0` `src/lib.rs:424-444`: the early two
//!   after `early_prepass_build_indirect_parameters` and before `early_downsample_depth`, the late
//!   two after `late_prepass_build_indirect_parameters` and before `main_build_indirect_parameters`
//!   and `Core3dSystems::MainPass`), so without our own ordering they are in no lap window: the
//!   executor may run one before [`start_view`]'s mark — the run's time then lands in
//!   `render/graph/views_remainder` — or inside the prepass or main-pass window, and which one it
//!   is may differ from run to run. [`add_graph_marks`] therefore orders [`start_view`] before all
//!   four and brackets each pair with a mark and a lap of its own, so a camera run's windows are:
//!   the early GPU-preprocessing chain up to `early_prepass_build_indirect_parameters` (below), the
//!   early sun-shadow pass, the rest of the prepass, the main pass up to
//!   `late_prepass_build_indirect_parameters`, the late sun-shadow pass, and the rest of the main
//!   pass (then the tail windows, below).
//!   The two shadow windows are summed into [`GRAPH_SHADOW_PASSES`] and *not* into the prepass and
//!   main-pass sums, so the stage columns stay exclusive of them and
//!   `render/graph/gpu_preprocess + render/graph/prepass + render/graph/main_pass +
//!   render/graph/post_process + render/graph/shadow_passes` tiles `render/graph/camera_views`
//!   exactly.
//!
//!   The head window is Bevy's early GPU-preprocessing chain: `clear_indirect_parameters_metadata`,
//!   `unpack_bins`, `early_gpu_preprocess` and `early_prepass_build_indirect_parameters` are added
//!   to `Core3d` as one `.chain().before(early_prepass)` tuple
//!   (`bevy_pbr-0.19.0` `src/render/gpu_preprocess.rs:436-451`), so they are in no stage set and the
//!   executor may run some of them before [`start_view`]'s mark — their time then lands in
//!   `render/graph/views_remainder` and [`GRAPH_DRIVER_OVERHEAD`] — or inside the prepass window,
//!   which differs from run to run. [`add_graph_marks`] orders [`start_view`] before the whole chain
//!   and laps it after `early_prepass_build_indirect_parameters`, the chain's last system: the
//!   window is summed into [`GRAPH_GPU_PREPROCESS`] for a camera run and kept in the prepass stage
//!   for a shadow view's own run, so the prepass sum is exclusive of it as it is of the sun passes.
//!   Whatever else the executor runs between the two marks is in the window too: nothing but our
//!   marks constrains those systems.
//!
//!   The tail window is Bevy's upscaling system, placed only `after(Core3dSystems::PostProcess)`
//!   (`bevy_core_pipeline-0.19.0` `src/core_3d/mod.rs:161`) and so free to run before the
//!   post-process lap. [`add_graph_marks`] orders that lap `before(upscaling)` and laps the system
//!   itself, so the upscaling window is summed into [`GRAPH_UPSCALING`] and, like the early
//!   post-process stage, also into [`GRAPH_POST_PROCESS`], which covers the whole tail of the run.
//!   What is left of a run after every lap — and so the whole of [`GRAPH_DRIVER_OVERHEAD`] besides
//!   the driver's own loop — is `msaa_writeback` (`bevy_post_process-0.19.0`
//!   `src/msaa_writeback.rs:32`, `pub(crate)`) and `ui_pass` (`bevy_ui_render-0.19.0`
//!   `src/lib.rs:271`), neither of them importable, plus the end-of-run deferred flush: `Core3d`
//!   sets `auto_insert_apply_deferred: false` and holds no `ApplyDeferred` system of its own
//!   (`bevy_core_pipeline-0.19.0` `src/schedule.rs:61-64`), so the executor applies every system's
//!   command buffer after the last one has run (`bevy_ecs-0.19.0`
//!   `src/schedule/executor/multi_threaded.rs:294-303`), where each view's `CommandEncoder::finish`
//!   and the queued command buffers of the whole run are taken
//!   (`bevy_render-0.19.0` `src/renderer/render_context.rs:106-126`) — after every lap of ours, so
//!   visible only as [`GRAPH_DRIVER_OVERHEAD`]. A
//!   shadow view's own run measures the same two windows (there `shared_shadow_pass` draws the
//!   point or spot light's map): that is not a camera's sun pass, so those windows are counted in
//!   the stage the run's window sits in, as they were before, and [`GRAPH_SHADOW_PASSES`] stays a
//!   camera-run column.
//!
//!   A view that runs a graph schedule other than `Core3d` is counted in `views` but not in the
//!   per-view laps, so it is the whole of its cycle's `views_remainder`.
//!
//!   The level above the view split is lapped too: [`GRAPH_CAMERA_DRIVER`] brackets the whole
//!   `camera_driver` system inside the root `Render` set — the mark just before it, the lap just
//!   after it ([`add_graph_marks`]) — so the column is the whole driver: every view's schedule it
//!   runs, the loop around them, the `CurrentView` it files per run and the `CameraWindows` it
//!   leaves for the uncovered-window clear.
//!   [`GRAPH_DRIVER_OVERHEAD`] is that less the two view-kind sums, never negative: what the loop
//!   spends between the runs plus whatever a run spends outside the per-view lap windows — the
//!   views remainder read against the driver instead of the `Render` set. With the head and tail
//!   windows lapped (above), that outside-the-windows part is the end-of-run deferred flush, the
//!   `msaa_writeback` blit and the `ui_pass` — the part of a run no lap can see. [`GRAPH_RENDER_SET_OTHER`]
//!   closes the level above: `render/graph/views` less `render/graph/camera_driver`, never negative.
//!   Bevy 0.19 puts no other system in the root `Render` set (`bevy_core_pipeline-0.19.0`
//!   `src/lib.rs:66` registers `camera_driver` and nothing else), so that column carries the set's
//!   own executor overhead — the sync points around an exclusive system — rather than another
//!   system's time; Bevy's diagnostics-encoder resolve (`bevy_render-0.19.0`
//!   `src/diagnostic/mod.rs:88-90`) sits after the set, in the gap to `Submit` where the views lap
//!   is taken too. Together the three split the 2026-10-09 04:34 window's 4.5 ms
//!   `render/graph/views_remainder` into the part inside the driver and the part outside it
//!   (`local/review/portal-1093-window-2026-10-09/`).

use bevy::{
    app::{First, Last},
    core_pipeline::{
        Core3d, Core3dSystems, mip_generation::experimental::depth::early_downsample_depth,
        schedule::camera_driver, upscaling::upscaling,
    },
    ecs::{
        schedule::{ScheduleConfigs, Schedules},
        system::ScheduleSystem,
    },
    pbr::{
        EARLY_SHADOW_PASS, LATE_SHADOW_PASS, ShadowView, clear_indirect_parameters_metadata,
        early_gpu_preprocess, early_prepass_build_indirect_parameters,
        late_prepass_build_indirect_parameters, main_build_indirect_parameters,
        per_view_shadow_pass, shared_shadow_pass, unpack_bins,
    },
    platform::time::Instant,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        pipelined_rendering::RenderExtractApp,
        render_resource::PipelineCache,
        renderer::{CurrentView, RenderGraph, RenderGraphSystems},
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
/// * [`GRAPH_CAMERA_DRIVER`], [`GRAPH_DRIVER_OVERHEAD`] and [`GRAPH_RENDER_SET_OTHER`]: the same
///   `Render` set one level in — the whole `camera_driver` system, that less the two view-kind
///   sums, and the views lap less the driver (see the module doc).
/// * [`GRAPH_VIEW_COUNT`] with [`GRAPH_PREPASS`], [`GRAPH_MAIN_PASS`], [`GRAPH_EARLY_POST_PROCESS`]
///   and [`GRAPH_POST_PROCESS`]: how many view schedules (`Core3d` runs) the cycle ran, and the
///   sums over them of the schedule's prepass, main-pass, early-post-process and post-process
///   stages. The GPU-driven binning and indirect-draw building run inside the `Prepass` set, so
///   their time is in `GRAPH_PREPASS`. `GRAPH_POST_PROCESS` now covers `EarlyPostProcess`,
///   `PostProcess` and the upscaling window together, so the 2026-10-08 window's numbers keep
///   their meaning there; `GRAPH_EARLY_POST_PROCESS` is the `EarlyPostProcess` part of it and
///   [`GRAPH_UPSCALING`] the upscaling part.
/// * [`GRAPH_SHADOW_PASSES`] and [`GRAPH_GPU_PREPROCESS`]: the two windows of a camera run that are
///   not stages — the sun shadow passes and the early GPU-preprocessing chain — each summed over
///   the camera runs and each exclusive of the stage sums it would otherwise sit inside
///   (`GRAPH_PREPASS` and `GRAPH_MAIN_PASS`), a shadow view's own run keeping them in its stages.
/// * [`GRAPH_SHADOW_VIEWS`] and [`GRAPH_CAMERA_VIEWS`]: the same runs, split by the kind of root
///   view `camera_driver` ran — a shadow view (an auxiliary root view with the `ShadowView` marker,
///   a point or spot light's map) or a camera. [`GRAPH_SHADOW_VIEW_COUNT`] says how many of
///   `GRAPH_VIEW_COUNT`'s runs were shadow views.
/// * [`GRAPH_SUBMIT`]: the root graph schedule's `Submit` set, where the graph's command buffers go
///   to the queue.
/// * [`GRAPH_REMAINDER`]: `render/graph_and_present` less the laps above — the schedule's own
///   overhead and, after it, the screenshot and readback submit, the second `queue.submit` and
///   `present` on each window. Present is not isolable without patching Bevy, so the remainder
///   carries it (see the module doc). [`GRAPH_VIEWS_REMAINDER`] is the same idea one level in:
///   `render/graph/views` less the two view-kind sums.
pub const GRAPH_VIEWS: &str = "render/graph/views";
/// How many view schedules the cycle's per-view sums cover. A count, not a duration. It keeps its
/// meaning across the view-kind split: every `Core3d` run the cycle made, camera and shadow views
/// alike (the shadow ones are [`GRAPH_SHADOW_VIEW_COUNT`] of it, the rest are cameras).
pub const GRAPH_VIEW_COUNT: &str = "render/graph/view_count";
pub const GRAPH_PREPASS: &str = "render/graph/prepass";
pub const GRAPH_MAIN_PASS: &str = "render/graph/main_pass";
/// The `Core3d` `EarlyPostProcess` stage alone, summed over the views: a subset of
/// [`GRAPH_POST_PROCESS`], split out because the stage is where temporal anti-aliasing lands
/// (`bevy_anti_alias-0.19.0` `src/taa/mod.rs:72`) and an empty stage if it is off.
pub const GRAPH_EARLY_POST_PROCESS: &str = "render/graph/early_post_process";
/// The `EarlyPostProcess` and `PostProcess` stages and Bevy's upscaling system together, summed
/// over the views (what this column has covered since impl-1075, extended by impl-1107 to the
/// upscaling window, which until then sat outside every lap and in [`GRAPH_DRIVER_OVERHEAD`]): the
/// whole tail of a run.
pub const GRAPH_POST_PROCESS: &str = "render/graph/post_process";
/// Bevy's upscaling system's window alone (`bevy_core_pipeline-0.19.0` `src/upscaling/node.rs:17`;
/// `src/core_3d/mod.rs:161` places it `after(Core3dSystems::PostProcess)`), summed over the views:
/// a subset of [`GRAPH_POST_PROCESS`], as [`GRAPH_EARLY_POST_PROCESS`] is.
pub const GRAPH_UPSCALING: &str = "render/graph/upscaling";
/// The `Core3d` runs `camera_driver` made for a shadow view: the whole of each such run's stage
/// laps, summed over the views (see the module doc — a directional light's cascades are inside a
/// camera run, not here).
pub const GRAPH_SHADOW_VIEWS: &str = "render/graph/shadow_views";
/// How many of [`GRAPH_VIEW_COUNT`]'s view schedules were shadow views. A count, not a duration.
pub const GRAPH_SHADOW_VIEW_COUNT: &str = "render/graph/shadow_view_count";
/// The `Core3d` runs `camera_driver` made for a camera: the whole of each such run's stage laps,
/// summed over the views. It carries a directional light's cascade shadow passes, which run inside
/// the camera's own schedule (see the module doc).
pub const GRAPH_CAMERA_VIEWS: &str = "render/graph/camera_views";
/// The sun (directional light) shadow-pass windows of a cycle's camera runs, summed: the time each
/// camera's own `Core3d` schedule spent in the early and in the late shadow-pass systems
/// (`per_view_shadow_pass` and `shared_shadow_pass`, `bevy_pbr-0.19.0` `src/lib.rs:424-444`,
/// `src/render/light.rs:2762-2805`), which Bevy adds to `Core3d` outside every stage set. The laps
/// bracket them inside the run ([`add_graph_marks`]), and the windows are *not* added to
/// [`GRAPH_PREPASS`] or [`GRAPH_MAIN_PASS`], so the stage columns stay exclusive of this one and
/// `gpu_preprocess + prepass + main_pass + post_process + shadow_passes` tiles
/// [`GRAPH_CAMERA_VIEWS`]. A shadow view's own run (a point or spot light's map, where
/// `shared_shadow_pass` draws that map) keeps its windows in the stage it sits in, so this column
/// is the camera runs' sun passes alone (see the module doc).
pub const GRAPH_SHADOW_PASSES: &str = "render/graph/shadow_passes";
/// The early GPU-preprocessing chain's window in a cycle's camera runs
/// (`bevy_pbr-0.19.0` `src/render/gpu_preprocess.rs:436-451`, the four systems chained and ordered
/// only `before(early_prepass)`), summed: the time from the start of a run to the end of the chain,
/// which the lap closes before the early sun-shadow pass. The window sits at the head of the
/// prepass, so this column is exclusive of [`GRAPH_PREPASS`] the way [`GRAPH_SHADOW_PASSES`] is
/// (see the module doc); a shadow view's own run keeps the window in its prepass stage, and any
/// other system the executor runs between the two marks is here too.
pub const GRAPH_GPU_PREPROCESS: &str = "render/graph/gpu_preprocess";
pub const GRAPH_SUBMIT: &str = "render/graph/submit";
pub const GRAPH_REMAINDER: &str = "render/graph/remainder";
/// `render/graph/views` less [`GRAPH_SHADOW_VIEWS`] and [`GRAPH_CAMERA_VIEWS`], never negative:
/// what `camera_driver` spends between the runs, plus whatever the runs spend outside the lap
/// windows (see the module doc).
pub const GRAPH_VIEWS_REMAINDER: &str = "render/graph/views_remainder";
/// The whole `camera_driver` system (`bevy_core_pipeline-0.19.0` `src/schedule.rs:133`), the root
/// `Render` set's only member (`src/lib.rs:66`): every view's graph schedule it runs, the
/// `CurrentView` it files per run, the `CameraWindows` it leaves behind and the loop around them.
/// The mark and the lap bracket the system inside the set ([`add_graph_marks`]), so the window also
/// holds whatever the executor orders between the two marks.
pub const GRAPH_CAMERA_DRIVER: &str = "render/graph/camera_driver";
/// `render/graph/views` less [`GRAPH_CAMERA_DRIVER`], never negative: the root `Render` set's own
/// overhead, since no other system sits in it in Bevy 0.19 — a Bevy version or a plugin that adds
/// one puts its time here (see the module doc).
pub const GRAPH_RENDER_SET_OTHER: &str = "render/graph/render_set_other";
/// [`GRAPH_CAMERA_DRIVER`] less [`GRAPH_CAMERA_VIEWS`] and [`GRAPH_SHADOW_VIEWS`], never negative:
/// what the driver spends outside the view schedules — the loop itself, the per-run `CurrentView`
/// insert and the `CameraWindows` insert — plus whatever a run spends outside the per-view lap
/// windows.
pub const GRAPH_DRIVER_OVERHEAD: &str = "render/graph/driver_overhead";

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
/// runs the render graph as schedules, so the boundaries are its own sets and systems — the root
/// schedule's `Render` and `Submit` sets, `camera_driver` within the `Render` set, and `Core3d`'s
/// stage sets — and ours are the systems ordered against them, run once per render frame, once per
/// driver and once per view. Nothing in Bevy is forked or patched, and every system is gated on
/// cycle tracking, so a run that keeps no cycles pays one atomic load per boundary and no lock.
///
/// The root hooks are added through the `RenderGraph` schedule itself; the `Core3d` hooks only
/// when that schedule exists (it is registered by `Core3dPlugin`, which a 2D-only or headless app
/// does not add), so a view that runs another graph schedule is measured by the root
/// `render/graph/views` lap alone.
///
/// The `Core3d` hooks tile each run into the windows the laps sum: from the run's start to the end
/// of Bevy's early GPU-preprocessing chain, on to the end of `Prepass`, on to the end of
/// `MainPass`, on to the end of `EarlyPostProcess`, on to the end of `PostProcess` and on to the
/// end of the upscaling system. Each window is added to its stage's cycle sum — and its own
/// view kind's ([`GRAPH_CAMERA_VIEWS`] or [`GRAPH_SHADOW_VIEWS`]), which is what makes a kind's
/// total the sum of its runs' stages. Systems outside the sets (the GPU preprocessing, and the sun
/// shadow passes, which Bevy orders against set members with `after`/`before` only) fall in
/// whichever window the executor runs them in; anything it orders outside the windows is not in
/// the per-view laps at all and shows up in [`GRAPH_VIEWS_REMAINDER`].
///
/// The sun shadow passes are one part of that outside-the-sets time this file does lap, because
/// Bevy leaves them unordered against every window: `bevy_pbr-0.19.0` `src/lib.rs:424-444` adds
/// `per_view_shadow_pass::<EARLY_SHADOW_PASS>`, `shared_shadow_pass::<EARLY_SHADOW_PASS>`,
/// `per_view_shadow_pass::<LATE_SHADOW_PASS>` and `shared_shadow_pass::<LATE_SHADOW_PASS>` to
/// `Core3d` with no set membership, each after a `*_build_indirect_parameters` system and before a
/// `*_downsample_depth` / `main_build_indirect_parameters` one — none of which is in a stage set
/// either (`bevy_pbr-0.19.0` `src/render/gpu_preprocess.rs:436-475`,
/// `bevy_core_pipeline-0.19.0` `src/mip_generation/mod.rs:296-304`). So [`start_view`] is ordered
/// before all four here — without it a pass could run before the run's mark and its time would
/// land in [`GRAPH_VIEWS_REMAINDER`] — and each pair is bracketed by a mark and a lap of its own
/// ([`lap_view_gpu_preprocess`] and [`lap_view_early_shadow`], then
/// [`lap_view_main_pass_before_late_shadow`] and [`lap_view_late_shadow`]). The mark closes the
/// window it interrupts into the stage it sits in, so the windows still tile the run, and the lap
/// puts the shadow window in [`GRAPH_SHADOW_PASSES`] instead of the stage: a camera run's prepass
/// and main-pass sums are therefore *exclusive* of the sun passes, as the prepass sum is of the
/// GPU-preprocessing head window, and
/// `gpu_preprocess + prepass + main_pass + post_process + shadow_passes` is the whole of the
/// run — the upscaling lap closing the tail the same way. Nothing in Bevy is reordered in a way it
/// does not already allow: every constraint added here sits inside the range Bevy's own
/// `after`/`before` calls leave open.
fn add_graph_marks(render_app: &mut SubApp) {
    render_app.add_systems(RenderGraph, render_graph_marks().run_if(tracks_cycles));
    if render_app.world().resource::<Schedules>().contains(Core3d) {
        render_app.add_systems(Core3d, core3d_marks().run_if(tracks_cycles));
    }
}

/// The root `RenderGraph` hooks [`add_graph_marks`] adds, as one set of systems so the schedule
/// test can add the same tuple to a schedule of its own.
///
/// The `Render` set's mark and lap bracket Bevy's `camera_driver` (`bevy_core_pipeline-0.19.0`
/// `src/lib.rs:66`, `src/schedule.rs:133`): the mark is ordered just before the system and the lap
/// just after it, both members of the set, so the window between them is the driver plus whatever
/// the executor orders between the two marks. Bevy puts nothing else in the set, so the two are
/// also its only members beside the driver.
fn render_graph_marks() -> ScheduleConfigs<ScheduleSystem> {
    (
        start_graph_frame.in_set(RenderGraphSystems::Begin),
        start_camera_driver
            .in_set(RenderGraphSystems::Render)
            .before(camera_driver),
        lap_camera_driver
            .in_set(RenderGraphSystems::Render)
            .after(camera_driver),
        lap_graph_views
            .after(RenderGraphSystems::Render)
            .before(RenderGraphSystems::Submit),
        lap_graph_submit
            .after(RenderGraphSystems::Submit)
            .before(RenderGraphSystems::Finish),
    )
        .into_configs()
}

/// The `Core3d` hooks [`add_graph_marks`] adds, as one set of systems so the schedule test can add
/// the same tuple to a schedule of its own.
fn core3d_marks() -> ScheduleConfigs<ScheduleSystem> {
    (
        start_view
            .before(Core3dSystems::Prepass)
            .before(clear_indirect_parameters_metadata)
            .before(unpack_bins)
            .before(early_gpu_preprocess)
            .before(early_prepass_build_indirect_parameters)
            .before(per_view_shadow_pass::<EARLY_SHADOW_PASS>)
            .before(shared_shadow_pass::<EARLY_SHADOW_PASS>)
            .before(per_view_shadow_pass::<LATE_SHADOW_PASS>)
            .before(shared_shadow_pass::<LATE_SHADOW_PASS>),
        // Closes the run's GPU-preprocessing window at the end of Bevy's early chain, before the
        // early shadow pass: the time from the start of the run to here is that chain
        // (`clear_indirect_parameters_metadata` → `unpack_bins` → `early_gpu_preprocess` →
        // `early_prepass_build_indirect_parameters`, `.chain().before(early_prepass)` at
        // `bevy_pbr-0.19.0` `src/render/gpu_preprocess.rs:436-451`), which a camera run's column
        // owns and a shadow view's own run keeps in its prepass stage (see [`add_view_gpu_preprocess`]).
        lap_view_gpu_preprocess
            .after(start_view)
            .after(early_prepass_build_indirect_parameters)
            .before(per_view_shadow_pass::<EARLY_SHADOW_PASS>)
            .before(shared_shadow_pass::<EARLY_SHADOW_PASS>),
        lap_view_early_shadow
            .after(per_view_shadow_pass::<EARLY_SHADOW_PASS>)
            .after(shared_shadow_pass::<EARLY_SHADOW_PASS>)
            .before(early_downsample_depth),
        lap_view_prepass
            .after(Core3dSystems::Prepass)
            .before(Core3dSystems::MainPass),
        // The same two boundaries in the main pass: the late shadow pass is ordered after the late
        // GPU preprocessing here, as Bevy's own constraints allow, so its window is the pass alone.
        lap_view_main_pass_before_late_shadow
            .after(lap_view_prepass)
            .after(late_prepass_build_indirect_parameters)
            .before(per_view_shadow_pass::<LATE_SHADOW_PASS>)
            .before(shared_shadow_pass::<LATE_SHADOW_PASS>),
        lap_view_late_shadow
            .after(per_view_shadow_pass::<LATE_SHADOW_PASS>)
            .after(shared_shadow_pass::<LATE_SHADOW_PASS>)
            .after(lap_view_main_pass_before_late_shadow)
            .before(main_build_indirect_parameters),
        lap_view_main_pass
            .after(Core3dSystems::MainPass)
            .before(Core3dSystems::EarlyPostProcess),
        lap_view_early_post_process
            .after(Core3dSystems::EarlyPostProcess)
            .before(Core3dSystems::PostProcess),
        // The `PostProcess` stage's own boundary, ordered before Bevy's upscaling system, which is
        // placed only `after(Core3dSystems::PostProcess)` (`bevy_core_pipeline-0.19.0`
        // `src/core_3d/mod.rs:158-161`): without the `before` the executor could run upscaling
        // first, its window would close empty and its time would land in [`GRAPH_DRIVER_OVERHEAD`].
        lap_view_post_process
            .after(Core3dSystems::PostProcess)
            .before(upscaling),
        // The upscaling window alone, closed by the lap after Bevy's `upscaling` system
        // (`bevy_core_pipeline-0.19.0` `src/upscaling/mod.rs:13`, `src/upscaling/node.rs:17`), which
        // is otherwise in no window of ours.
        lap_view_upscaling.after(upscaling),
    )
        .into_configs()
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
    /// The mark the mark system before `camera_driver` opened, closed by the lap after it
    /// ([`GRAPH_CAMERA_DRIVER`]). Taken, not re-armed: the driver is lapped once per render frame.
    camera_driver_mark: Option<Instant>,
    /// The mark a view's graph schedule opened, re-armed at each of its stage boundaries, so the
    /// per-view stage laps ([`GRAPH_PREPASS`], [`GRAPH_MAIN_PASS`], [`GRAPH_EARLY_POST_PROCESS`],
    /// [`GRAPH_POST_PROCESS`]) tile the view.
    view_mark: Option<Instant>,
    /// Whether the run behind [`State::view_mark`] is a shadow view ([`GRAPH_SHADOW_VIEWS`]) rather
    /// than a camera ([`GRAPH_CAMERA_VIEWS`]), read off the view's `ShadowView` marker when the run
    /// started. Each stage lap adds its window to that kind's cycle sum too.
    view_is_shadow: bool,
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
    /// * `graph_view_count`: how many view schedules the per-view sums below cover — every `Core3d`
    ///   run of the cycle, camera and shadow views alike. A count, not a duration; zeros and
    ///   millisecond fields share this record, so the count column is written as an integer.
    /// * `graph_prepass_ms`, `graph_main_pass_ms`, `graph_early_post_process_ms`,
    ///   `graph_post_process_ms`: the sums over those views of the `Core3d` schedule's prepass,
    ///   main-pass, early-post-process and post-process stages. The post-process sum now covers
    ///   `EarlyPostProcess`, `PostProcess` and the upscaling window together, as it covered the
    ///   first two before the early stage got its own column, so the three are not added side by
    ///   side. The prepass sum is exclusive of the GPU-preprocessing head window and the main pass
    ///   of the sun passes (below).
    /// * `graph_gpu_preprocess_ms`: the camera runs' early GPU-preprocessing head windows alone
    ///   ([`GRAPH_GPU_PREPROCESS`]), one of the two windows of a camera run that is not a stage: it
    ///   is excluded from the prepass sum above, which would otherwise hold it.
    /// * `graph_upscaling_ms`: the upscaling window alone ([`GRAPH_UPSCALING`]), a subset of
    ///   `graph_post_process_ms` as the early stage is.
    /// * `graph_shadow_views_ms`, `graph_camera_views_ms` and `graph_shadow_view_count`: the same
    ///   runs split by kind — a shadow view (a point or spot light's map, which `camera_driver`
    ///   drives as an auxiliary root view) or a camera — each sum being that kind's stage windows
    ///   added up. A kind that ran no view in the cycle keeps its columns empty, and
    ///   `graph_shadow_view_count` counts the shadow runs of `graph_view_count`.
    /// * `graph_shadow_passes_ms`: the camera runs' sun-shadow pass windows alone
    ///   ([`GRAPH_SHADOW_PASSES`]), the second of the two windows of a camera run that is not a
    ///   stage: it is excluded from the prepass and main-pass sums above, so those stay exclusive
    ///   of it.
    /// * `graph_submit_ms`: the graph's `queue.submit` (the root `Submit` set) and the
    ///   uncovered-window clear.
    /// * `graph_remainder_ms`: `graph_and_present_ms` less `graph_views_ms` and `graph_submit_ms`,
    ///   never negative: the schedule's own overhead and the region after it, which carries the
    ///   screenshot/readback submit, the second `queue.submit` and `present` on each window.
    /// * `graph_views_remainder_ms`: `graph_views_ms` less `graph_shadow_views_ms` and
    ///   `graph_camera_views_ms`, never negative: what `camera_driver` spends between the runs,
    ///   plus whatever the runs spend outside the per-view lap windows.
    /// * `graph_camera_driver_ms`: the whole `camera_driver` system, lapped between the mark
    ///   before it and the lap after it inside the root `Render` set ([`GRAPH_CAMERA_DRIVER`]).
    /// * `graph_render_set_other_ms`: `graph_views_ms` less `graph_camera_driver_ms`, never
    ///   negative: the `Render` set's own overhead, no other system sitting in it in Bevy 0.19
    ///   ([`GRAPH_RENDER_SET_OTHER`]).
    /// * `graph_driver_overhead_ms`: `graph_camera_driver_ms` less `graph_camera_views_ms` and
    ///   `graph_shadow_views_ms`, never negative: the driver's loop and per-run setup plus whatever
    ///   a run spends outside the per-view lap windows ([`GRAPH_DRIVER_OVERHEAD`]).
    pub graph_views_ms: Option<f64>,
    pub graph_view_count: Option<u32>,
    pub graph_prepass_ms: Option<f64>,
    pub graph_main_pass_ms: Option<f64>,
    pub graph_early_post_process_ms: Option<f64>,
    pub graph_post_process_ms: Option<f64>,
    pub graph_shadow_views_ms: Option<f64>,
    pub graph_shadow_view_count: Option<u32>,
    pub graph_camera_views_ms: Option<f64>,
    pub graph_shadow_passes_ms: Option<f64>,
    pub graph_submit_ms: Option<f64>,
    pub graph_remainder_ms: Option<f64>,
    pub graph_views_remainder_ms: Option<f64>,
    pub graph_camera_driver_ms: Option<f64>,
    pub graph_render_set_other_ms: Option<f64>,
    pub graph_driver_overhead_ms: Option<f64>,
    pub graph_gpu_preprocess_ms: Option<f64>,
    pub graph_upscaling_ms: Option<f64>,
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

    /// `camera_driver` is about to start (the mark system ordered just before it, in the root
    /// schedule's `Render` set): opens the mark [`RenderTimings::camera_driver_lapped`] closes.
    fn camera_driver_started(&self) {
        if !self.tracks_cycles() {
            return;
        }
        self.state().camera_driver_mark = Some(Instant::now());
    }

    /// `camera_driver` has returned: closes [`GRAPH_CAMERA_DRIVER`] and, the per-view laps inside
    /// the driver having closed with it, [`GRAPH_DRIVER_OVERHEAD`] against the two view-kind sums.
    /// The mark is taken rather than re-armed — the driver is lapped once per render frame — so a
    /// second lap without a new mark records nothing.
    fn camera_driver_lapped(&self) {
        if !self.tracks_cycles() {
            return;
        }
        let mut state = self.state();
        let Some(mark) = state.camera_driver_mark.take() else {
            return;
        };
        let ms = (Instant::now() - mark).as_secs_f64() * 1000.0;
        let render_cycle = state.render_cycle;
        self.write_cycle(&mut state, render_cycle, |sides| {
            sides.graph_camera_driver_ms = Some(ms);
            sides.graph_driver_overhead_ms = Some(driver_overhead_ms(ms, sides));
        });
    }

    /// Every view's graph schedule has run (the root schedule's `Render` set, Bevy's
    /// `camera_driver`): closes [`GRAPH_VIEWS`] and re-arms the mark
    /// [`RenderTimings::submit_lapped`] closes. Every run of the cycle has finished by now — the
    /// lap is ordered after the `Render` set the loop is in — so the two view-kind sums are
    /// complete and [`GRAPH_VIEWS_REMAINDER`] can close the split, and so is
    /// [`GRAPH_CAMERA_DRIVER`]'s lap inside the set, which [`GRAPH_RENDER_SET_OTHER`] is closed
    /// against.
    fn views_lapped(&self) {
        self.graph_lap(|sides, ms| {
            sides.graph_views_ms = Some(ms);
            sides.graph_views_remainder_ms = Some(views_remainder_ms(ms, sides));
            sides.graph_render_set_other_ms = Some(render_set_other_ms(ms, sides));
        });
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
    /// mark, counts the view and records its kind (`is_shadow`), which the stage laps add their
    /// windows to.
    fn view_started(&self, is_shadow: bool) {
        if !self.tracks_cycles() {
            return;
        }
        let mut state = self.state();
        state.view_mark = Some(Instant::now());
        state.view_is_shadow = is_shadow;
        let render_cycle = state.render_cycle;
        self.write_cycle(&mut state, render_cycle, |sides| {
            sides.graph_view_count = Some(sides.graph_view_count.unwrap_or(0) + 1);
            if is_shadow {
                sides.graph_shadow_view_count =
                    Some(sides.graph_shadow_view_count.unwrap_or(0) + 1);
            }
        });
    }

    /// Closes the view lap opened at the view's previous boundary — the start, then each stage
    /// boundary, and the two sun shadow-pass boundaries between them — and adds it to the cycle's
    /// sum for that stage and for the run's kind ([`GRAPH_CAMERA_VIEWS`] or [`GRAPH_SHADOW_VIEWS`]).
    /// The windows tile the run, so the kind's sum ends up as the whole of each run the cycle made
    /// of it.
    fn view_lapped(&self, add: fn(&mut CycleSides, f64)) {
        self.view_lapped_with(|sides, _is_shadow, ms| add(sides, ms));
    }

    /// A view lap whose cycle column depends on the kind of run the window is in, beside
    /// [`RenderTimings::view_lapped`]: `add` gets the run's kind as well, so the sun shadow-pass
    /// windows ([`GRAPH_SHADOW_PASSES`]) can be a camera-run column while a shadow view's own run
    /// keeps its window in the stage it sits in (see [`add_camera_run_window`]).
    fn view_lapped_with(&self, add: impl Fn(&mut CycleSides, bool, f64)) {
        if !self.tracks_cycles() {
            return;
        }
        let now = Instant::now();
        let mut state = self.state();
        let Some(mark) = state.view_mark.replace(now) else {
            return;
        };
        let ms = (now - mark).as_secs_f64() * 1000.0;
        let is_shadow = state.view_is_shadow;
        let render_cycle = state.render_cycle;
        self.write_cycle(&mut state, render_cycle, |sides| {
            add(sides, is_shadow, ms);
            add_view_kind(sides, is_shadow, ms);
        });
    }

    fn end_render(&self) {
        let mut state = self.state();
        state.render_mark = None;
        state.graph_mark = None;
        state.view_mark = None;
        state.camera_driver_mark = None;
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

fn start_camera_driver(timings: Res<RenderTimings>) {
    timings.camera_driver_started();
}

fn lap_camera_driver(timings: Res<RenderTimings>) {
    timings.camera_driver_lapped();
}

fn start_view(
    timings: Res<RenderTimings>,
    current_view: Option<Res<CurrentView>>,
    shadow_views: Query<(), With<ShadowView>>,
) {
    timings.view_started(view_is_shadow(current_view.as_deref(), &shadow_views));
}

/// Whether the `Core3d` run that is starting is a shadow view: the entity `camera_driver` inserted
/// as `CurrentView` before running the schedule (`bevy_render-0.19.0`
/// `src/renderer/render_context.rs:205`; `bevy_core_pipeline-0.19.0` `src/schedule.rs:194`) carries
/// bevy_pbr's `ShadowView` marker (`bevy_pbr-0.19.0` `src/render/light.rs:928`), which its
/// `prepare_lights` puts on the point and spot light views it spawns as root views (`:2064-2094`,
/// `:2150-2175`) and on the directional cascade views that run inside a camera's own schedule
/// (`:1869-1896`, so they never reach this test as a root run). A camera's own entity has no
/// `ShadowView`, and a run with no `CurrentView` (nothing this app does) is counted as a camera
/// rather than dropped, so no lap is lost to a missing resource.
fn view_is_shadow(
    current_view: Option<&CurrentView>,
    shadow_views: &Query<(), With<ShadowView>>,
) -> bool {
    current_view.is_some_and(|current_view| shadow_views.contains(current_view.0))
}

fn lap_view_prepass(timings: Res<RenderTimings>) {
    timings.view_lapped(add_graph_prepass);
}

/// The boundary between the run's GPU-preprocessing window and its early sun-shadow pass (see
/// [`add_graph_marks`]): closes the window opened by [`start_view`] into
/// [`GRAPH_GPU_PREPROCESS`] — or, in a shadow view's own run, into the prepass stage it sits at the
/// head of ([`add_view_gpu_preprocess`]) — and re-arms the mark for the pass, which
/// [`lap_view_early_shadow`] closes into [`GRAPH_SHADOW_PASSES`].
fn lap_view_gpu_preprocess(timings: Res<RenderTimings>) {
    timings.view_lapped_with(add_view_gpu_preprocess);
}

/// The early sun-shadow passes have run (`per_view_shadow_pass::<EARLY_SHADOW_PASS>` and
/// `shared_shadow_pass::<EARLY_SHADOW_PASS>`): adds their window and re-arms the mark, so the rest
/// of the prepass window is closed by [`lap_view_prepass`] as before.
fn lap_view_early_shadow(timings: Res<RenderTimings>) {
    timings.view_lapped_with(add_graph_early_shadow_pass);
}

/// The boundary between the run's main-pass window and its late sun-shadow pass: closes the
/// main-pass window opened by [`lap_view_prepass`] and re-arms the mark for the pass, which
/// [`lap_view_late_shadow`] closes into [`GRAPH_SHADOW_PASSES`].
fn lap_view_main_pass_before_late_shadow(timings: Res<RenderTimings>) {
    timings.view_lapped(add_graph_main_pass);
}

/// The late sun-shadow passes have run (`per_view_shadow_pass::<LATE_SHADOW_PASS>` and
/// `shared_shadow_pass::<LATE_SHADOW_PASS>`): adds their window and re-arms the mark, so the rest
/// of the main-pass window — the main opaque and transparent passes — is closed by
/// [`lap_view_main_pass`] as before.
fn lap_view_late_shadow(timings: Res<RenderTimings>) {
    timings.view_lapped_with(add_graph_late_shadow_pass);
}

fn lap_view_main_pass(timings: Res<RenderTimings>) {
    timings.view_lapped(add_graph_main_pass);
}

fn lap_view_early_post_process(timings: Res<RenderTimings>) {
    timings.view_lapped(add_graph_early_post_process);
}

fn lap_view_post_process(timings: Res<RenderTimings>) {
    timings.view_lapped(add_graph_post_process);
}

/// Bevy's upscaling system has run (`bevy_core_pipeline-0.19.0` `src/upscaling/node.rs:17`):
/// closes the upscaling window into [`GRAPH_UPSCALING`] and, through [`add_graph_upscaling`], into
/// [`GRAPH_POST_PROCESS`], which covers the whole tail of the run.
fn lap_view_upscaling(timings: Res<RenderTimings>) {
    timings.view_lapped(add_graph_upscaling);
}

/// Adds a view's stage lap to the cycle's sum for that stage (the record holds one sum per stage,
/// over every view the cycle ran).
fn add_graph_prepass(sides: &mut CycleSides, ms: f64) {
    sides.graph_prepass_ms = Some(sides.graph_prepass_ms.unwrap_or(0.0) + ms);
}

fn add_graph_main_pass(sides: &mut CycleSides, ms: f64) {
    sides.graph_main_pass_ms = Some(sides.graph_main_pass_ms.unwrap_or(0.0) + ms);
}

/// Adds the early-post-process window to the cycle's sum for that stage and to
/// [`GRAPH_POST_PROCESS`], which keeps covering `EarlyPostProcess`, `PostProcess` and upscaling
/// together (see [`GRAPH_POST_PROCESS`]).
fn add_graph_early_post_process(sides: &mut CycleSides, ms: f64) {
    sides.graph_early_post_process_ms = Some(sides.graph_early_post_process_ms.unwrap_or(0.0) + ms);
    add_graph_post_process(sides, ms);
}

fn add_graph_post_process(sides: &mut CycleSides, ms: f64) {
    sides.graph_post_process_ms = Some(sides.graph_post_process_ms.unwrap_or(0.0) + ms);
}

/// Adds the upscaling window to [`GRAPH_POST_PROCESS`], which covers the whole tail of the run, and
/// to [`GRAPH_UPSCALING`], its own column — a subset of the former, as the early post-process stage
/// is (see [`GRAPH_UPSCALING`]).
fn add_graph_upscaling(sides: &mut CycleSides, ms: f64) {
    sides.graph_upscaling_ms = Some(sides.graph_upscaling_ms.unwrap_or(0.0) + ms);
    add_graph_post_process(sides, ms);
}

/// Adds a camera run's early GPU-preprocessing window to [`GRAPH_GPU_PREPROCESS`].
fn add_graph_gpu_preprocess(sides: &mut CycleSides, ms: f64) {
    sides.graph_gpu_preprocess_ms = Some(sides.graph_gpu_preprocess_ms.unwrap_or(0.0) + ms);
}

/// The rule the two camera-run window columns share ([`GRAPH_SHADOW_PASSES`] and
/// [`GRAPH_GPU_PREPROCESS`]): the window goes to the column when the run is a camera, and to the
/// stage it sits in (`stage`) when it is a shadow view's own run. A camera's run holds the
/// directional light's cascade draw or the GPU-preprocessing chain; a shadow view's own run holds
/// `shared_shadow_pass` drawing that view's map or the same chain preprocessing it, which is that
/// run's own work and not a camera's, so it stays in the run's stage sums and the two columns stay
/// camera-run ones (see the module doc).
fn add_camera_run_window(
    sides: &mut CycleSides,
    is_shadow: bool,
    ms: f64,
    column: fn(&mut CycleSides, f64),
    stage: fn(&mut CycleSides, f64),
) {
    if is_shadow {
        stage(sides, ms);
    } else {
        column(sides, ms);
    }
}

/// The sun shadow-pass column writer, for [`add_camera_run_window`].
fn add_shadow_pass_window(sides: &mut CycleSides, ms: f64) {
    sides.graph_shadow_passes_ms = Some(sides.graph_shadow_passes_ms.unwrap_or(0.0) + ms);
}

/// The early sun shadow pass's window: [`add_camera_run_window`] with its sun-pass column and the
/// prepass stage as the shadow view's fallback.
fn add_graph_early_shadow_pass(sides: &mut CycleSides, is_shadow: bool, ms: f64) {
    add_camera_run_window(
        sides,
        is_shadow,
        ms,
        add_shadow_pass_window,
        add_graph_prepass,
    );
}

/// The late sun shadow pass's window: [`add_camera_run_window`] with its sun-pass column and the
/// main pass as the shadow view's fallback.
fn add_graph_late_shadow_pass(sides: &mut CycleSides, is_shadow: bool, ms: f64) {
    add_camera_run_window(
        sides,
        is_shadow,
        ms,
        add_shadow_pass_window,
        add_graph_main_pass,
    );
}

/// The early GPU-preprocessing window, as [`RenderTimings::view_lapped_with`] wants it
/// ([`lap_view_gpu_preprocess`]): [`add_camera_run_window`] with [`GRAPH_GPU_PREPROCESS`] as the
/// camera-run column and the prepass stage — at whose head the window sits — as the shadow view's
/// fallback.
fn add_view_gpu_preprocess(sides: &mut CycleSides, is_shadow: bool, ms: f64) {
    add_camera_run_window(
        sides,
        is_shadow,
        ms,
        add_graph_gpu_preprocess,
        add_graph_prepass,
    );
}

/// Adds a view lap to the cycle's sum for the run's kind (see [`GRAPH_SHADOW_VIEWS`]).
fn add_view_kind(sides: &mut CycleSides, is_shadow: bool, ms: f64) {
    let sum = if is_shadow {
        &mut sides.graph_shadow_views_ms
    } else {
        &mut sides.graph_camera_views_ms
    };
    *sum = Some(sum.unwrap_or(0.0) + ms);
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

/// The part of `render/graph/views` the per-view laps do not own (see [`GRAPH_VIEWS_REMAINDER`]):
/// the views lap less the two view-kind sums, never negative. The per-view laps are read inside
/// the `Core3d` runs of the views lap, so like [`graph_remainder_ms`] a small excess is a timer
/// tick, not a negative remainder.
fn views_remainder_ms(views_ms: f64, sides: &CycleSides) -> f64 {
    let parts =
        sides.graph_shadow_views_ms.unwrap_or(0.0) + sides.graph_camera_views_ms.unwrap_or(0.0);
    (views_ms - parts).max(0.0)
}

/// The part of the root `Render` set the `camera_driver` lap does not own (see
/// [`GRAPH_RENDER_SET_OTHER`]): the views lap less [`GRAPH_CAMERA_DRIVER`], never negative. The
/// driver is lapped inside the set, so like [`graph_remainder_ms`] a small excess is a timer tick,
/// not a negative remainder. No other system sits in the set in Bevy 0.19, so the column is the
/// set's own executor overhead unless a Bevy version or a plugin adds a member.
fn render_set_other_ms(views_ms: f64, sides: &CycleSides) -> f64 {
    (views_ms - sides.graph_camera_driver_ms.unwrap_or(0.0)).max(0.0)
}

/// The time `camera_driver` spends outside the view schedules it runs (see
/// [`GRAPH_DRIVER_OVERHEAD`]): their lap less [`GRAPH_CAMERA_VIEWS`] and [`GRAPH_SHADOW_VIEWS`],
/// never negative. The per-view laps are read inside the driver, so a small excess is a timer tick.
fn driver_overhead_ms(camera_driver_ms: f64, sides: &CycleSides) -> f64 {
    let runs =
        sides.graph_shadow_views_ms.unwrap_or(0.0) + sides.graph_camera_views_ms.unwrap_or(0.0);
    (camera_driver_ms - runs).max(0.0)
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
    use bevy::ecs::schedule::Schedule;

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
    /// graph's queue submit and a remainder that closes the phase — the views lap itself split
    /// into the camera runs, the shadow-view runs and a remainder, and the `Render` set one level
    /// further into `camera_driver` and the set's own overhead. The parts are taken inside the
    /// outer lap, so they cannot exceed it, and the remainders are what make the row's graph
    /// columns add up: the view-kind sums and the views remainder to `render/graph/views`, the
    /// driver's lap and the `Render` set's other time to it as well, the runs and the driver's
    /// overhead to the driver's lap, and the views lap and the submit lap with their remainder to
    /// `render/graph_and_present`. The per-view stage sums are bounded by the views lap that
    /// includes every view schedule's own overhead, and the counts say how many views they cover.
    #[test]
    fn the_graph_laps_account_for_graph_and_present() {
        let timings = RenderTimings::default();
        timings.set_recording(true);
        timings.set_cycle_tracking(true, true);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_render();
        // The root graph schedule: its `Begin` set, the mark before `camera_driver`, a camera run
        // and a shadow-view run inside it, the lap after the driver, the lap after the `Render`
        // set, the lap after the `Submit` set, then the phase's own lap (`Set::Render`).
        timings.graph_started();
        std::thread::sleep(Duration::from_millis(1));
        timings.camera_driver_started();
        for is_shadow in [false, true] {
            timings.view_started(is_shadow);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped_with(add_view_gpu_preprocess);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_prepass);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_main_pass);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_early_post_process);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_post_process);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_upscaling);
        }
        std::thread::sleep(Duration::from_millis(1));
        timings.camera_driver_lapped();
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
        // The views lap, split by the kind of run and closed by its own remainder.
        assert_eq!(
            cycle.graph_shadow_view_count,
            Some(1),
            "one of the two runs was a shadow view"
        );
        let camera = cycle
            .graph_camera_views_ms
            .expect("the camera run was lapped");
        let shadow = cycle
            .graph_shadow_views_ms
            .expect("the shadow run was lapped");
        let views_remainder = cycle
            .graph_views_remainder_ms
            .expect("the views lap writes the remainder");
        assert!(
            camera >= 6.0 && shadow >= 6.0,
            "each kind's sum is its own run's six windows: {camera} camera, {shadow} shadow"
        );
        assert!(
            camera + shadow <= views,
            "the runs were lapped inside the views lap: {camera} + {shadow} vs {views}"
        );
        assert!(
            (camera + shadow + views_remainder - views).abs() < 1e-9,
            "the runs and the views remainder are the views lap: {camera} + {shadow} + \
             {views_remainder} vs {views}"
        );
        assert!(
            views_remainder >= 1.0,
            "the sleep between the last run and the views lap is in the views remainder: \
             {views_remainder}"
        );
        // The same level one step out: the driver's own lap, the `Render` set's other time and the
        // driver's overhead close against the laps they sit in.
        let driver = cycle
            .graph_camera_driver_ms
            .expect("the camera driver lap ran");
        let set_other = cycle
            .graph_render_set_other_ms
            .expect("the views lap writes the set's other time");
        let overhead = cycle
            .graph_driver_overhead_ms
            .expect("the driver lap writes its overhead");
        assert!(
            driver >= camera + shadow + 1.0,
            "the driver lap covers both runs and the sleep after them: {driver} vs {camera} + \
             {shadow}"
        );
        assert!(
            (driver + set_other - views).abs() < 1e-9,
            "the driver and the set's other time are the views lap: {driver} + {set_other} vs \
             {views}"
        );
        assert!(
            (camera + shadow + overhead - driver).abs() < 1e-9,
            "the runs and the driver's overhead are the driver's lap: {camera} + {shadow} + \
             {overhead} vs {driver}"
        );
        assert!(
            set_other >= 1.0,
            "the sleep between the schedule's start and the driver's mark is the set's other \
             time: {set_other}"
        );
        assert!(
            overhead >= 1.0,
            "the sleep between the last run and the driver's lap is the driver's overhead: \
             {overhead}"
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
        // The head window is a camera-run column, so a shadow view's own run keeps it in its
        // prepass: the stage sums plus it are the two kind sums, which the six windows of each run
        // below make up (the post-process sum carrying the early stage and the upscaling window).
        let gpu_preprocess = cycle
            .graph_gpu_preprocess_ms
            .expect("the camera run's head window was lapped");
        assert!(
            (stages[0] + stages[1] + stages[2] + gpu_preprocess - (camera + shadow)).abs() < 1e-9,
            "the stage sums and the head window cover the same windows the view-kind sums do: {} + \
             {} + {} + {gpu_preprocess} vs {camera} + {shadow}",
            stages[0],
            stages[1],
            stages[2]
        );
        let upscaling = cycle
            .graph_upscaling_ms
            .expect("the camera run's tail window was lapped");
        assert!(
            upscaling >= 2.0 && upscaling <= cycle.graph_post_process_ms.unwrap(),
            "the upscaling window is a subset of the post-process sum, as the early stage is: \
             {upscaling} vs {:?}",
            cycle.graph_post_process_ms
        );
        assert!(
            cycle.graph_early_post_process_ms.unwrap() >= 2.0
                && cycle.graph_early_post_process_ms.unwrap()
                    <= cycle.graph_post_process_ms.unwrap(),
            "the early stage is a part of the post-process sum, which covers the early stage, the \
             post-process stage and the upscaling window: {:?} vs {:?}",
            cycle.graph_early_post_process_ms,
            cycle.graph_post_process_ms
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
        timings.camera_driver_started();
        timings.view_started(true);
        timings.view_lapped_with(add_view_gpu_preprocess);
        timings.view_lapped(add_graph_prepass);
        timings.view_lapped_with(add_graph_early_shadow_pass);
        timings.view_lapped(add_graph_prepass);
        timings.view_lapped(add_graph_main_pass);
        timings.view_lapped_with(add_graph_late_shadow_pass);
        timings.view_lapped(add_graph_main_pass);
        timings.view_lapped(add_graph_early_post_process);
        timings.view_lapped(add_graph_post_process);
        timings.view_lapped(add_graph_upscaling);
        timings.camera_driver_lapped();
        timings.views_lapped();
        timings.submit_lapped();
        assert!(
            timings.state().graph_mark.is_none()
                && timings.state().view_mark.is_none()
                && timings.state().camera_driver_mark.is_none(),
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
            GRAPH_EARLY_POST_PROCESS,
            GRAPH_POST_PROCESS,
            GRAPH_SHADOW_VIEWS,
            GRAPH_SHADOW_VIEW_COUNT,
            GRAPH_CAMERA_VIEWS,
            GRAPH_SHADOW_PASSES,
            GRAPH_SUBMIT,
            GRAPH_REMAINDER,
            GRAPH_VIEWS_REMAINDER,
            GRAPH_CAMERA_DRIVER,
            GRAPH_RENDER_SET_OTHER,
            GRAPH_DRIVER_OVERHEAD,
            GRAPH_GPU_PREPROCESS,
            GRAPH_UPSCALING,
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

    /// The views remainder is `render/graph/views` less the two view-kind sums, and never negative
    /// — the per-view laps are read inside the runs the views lap covers, so a small excess is a
    /// timer tick, not a negative remainder.
    #[test]
    fn the_views_remainder_never_goes_negative() {
        let sides = CycleSides {
            graph_camera_views_ms: Some(4.0),
            graph_shadow_views_ms: Some(1.5),
            ..Default::default()
        };
        assert_eq!(views_remainder_ms(6.0, &sides), 0.5);
        assert_eq!(
            views_remainder_ms(5.0, &sides),
            0.0,
            "the runs can exceed the views lap by less than a timer tick"
        );
        assert!(
            (views_remainder_ms(5.6, &sides) - 0.1).abs() < 1e-9,
            "the remainder is what the runs leave"
        );
        assert_eq!(
            views_remainder_ms(2.0, &CycleSides::default()),
            2.0,
            "no run was measured: the whole views lap is the remainder"
        );
    }

    /// The `Render` set's other time is `render/graph/views` less the driver's lap, and the
    /// driver's overhead is that lap less the two view-kind sums — both never negative, since the
    /// laps inside them can exceed them only by a timer tick.
    #[test]
    fn the_driver_overhead_and_the_render_sets_other_time_never_go_negative() {
        let sides = CycleSides {
            graph_camera_driver_ms: Some(4.0),
            graph_camera_views_ms: Some(3.0),
            graph_shadow_views_ms: Some(0.5),
            ..Default::default()
        };
        assert_eq!(render_set_other_ms(6.0, &sides), 2.0);
        assert_eq!(
            render_set_other_ms(3.5, &sides),
            0.0,
            "the driver can exceed the views lap by less than a timer tick"
        );
        assert_eq!(
            render_set_other_ms(4.0, &CycleSides::default()),
            4.0,
            "no driver lap was measured: the whole views lap is the set's other time"
        );
        assert!(
            (driver_overhead_ms(4.0, &sides) - 0.5).abs() < 1e-9,
            "the runs leave half a millisecond of driver overhead: {}",
            driver_overhead_ms(4.0, &sides)
        );
        assert_eq!(
            driver_overhead_ms(3.0, &sides),
            0.0,
            "the laps can exceed the driver by less than a timer tick"
        );
        assert_eq!(
            driver_overhead_ms(2.0, &CycleSides::default()),
            2.0,
            "no run was measured: the whole driver lap is overhead"
        );
    }

    /// The driver's lap needs the mark the system before `camera_driver` opens: a lap with no mark
    /// (one before the driver started, or a second lap in the same render frame) writes nothing,
    /// so the record keeps the marked window rather than stretching to the second boundary.
    #[test]
    fn the_driver_lap_records_nothing_without_its_mark() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, false);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_render();
        timings.camera_driver_lapped();
        assert!(
            timings.state().camera_driver_mark.is_none(),
            "a lap with no mark takes none"
        );
        timings.camera_driver_started();
        timings.camera_driver_lapped();
        let recorded = timings
            .state()
            .cycles
            .get(&1)
            .and_then(|sides| sides.graph_camera_driver_ms)
            .expect("the marked lap is recorded");
        assert!(
            timings
                .state()
                .cycles
                .get(&1)
                .is_some_and(|sides| sides.graph_driver_overhead_ms.is_some()),
            "the same lap writes the driver's overhead"
        );
        std::thread::sleep(Duration::from_millis(5));
        timings.camera_driver_lapped();
        assert_eq!(
            timings
                .state()
                .cycles
                .get(&1)
                .and_then(|sides| sides.graph_camera_driver_ms),
            Some(recorded),
            "the second lap had no mark and wrote nothing"
        );
    }

    /// One view run's windows, in the order [`core3d_marks`] orders them: the mark at the start of
    /// the run, the prepass up to the early sun shadow pass, the early pass's window, the rest of
    /// the prepass, the main pass up to the late sun shadow pass, the late pass's window, the rest
    /// of the main pass, then the two post-process stages. Each window is about a millisecond, so
    /// a sum says which windows went into it.
    fn one_run(timings: &RenderTimings, is_shadow: bool) {
        let sleep = || std::thread::sleep(Duration::from_millis(1));
        timings.view_started(is_shadow);
        sleep();
        timings.view_lapped_with(add_view_gpu_preprocess);
        sleep();
        timings.view_lapped(add_graph_prepass);
        sleep();
        timings.view_lapped_with(add_graph_early_shadow_pass);
        sleep();
        timings.view_lapped(add_graph_prepass);
        sleep();
        timings.view_lapped(add_graph_main_pass);
        sleep();
        timings.view_lapped_with(add_graph_late_shadow_pass);
        sleep();
        timings.view_lapped(add_graph_main_pass);
        sleep();
        timings.view_lapped(add_graph_early_post_process);
        sleep();
        timings.view_lapped(add_graph_post_process);
        sleep();
        timings.view_lapped(add_graph_upscaling);
    }

    /// The sun shadow-pass column takes a camera run's two windows, and a shadow view's own run
    /// keeps its windows in the stage they sit in, so the column is the camera runs' passes alone.
    #[test]
    fn the_sun_pass_column_takes_camera_runs_only() {
        let mut camera = CycleSides::default();
        add_graph_early_shadow_pass(&mut camera, false, 1.5);
        add_graph_late_shadow_pass(&mut camera, false, 0.5);
        assert_eq!(
            camera.graph_shadow_passes_ms,
            Some(2.0),
            "both windows are summed into the one column"
        );
        assert_eq!(
            (camera.graph_prepass_ms, camera.graph_main_pass_ms),
            (None, None),
            "a camera run's windows are exclusive of the stages around them"
        );

        let mut shadow_view = CycleSides::default();
        add_graph_early_shadow_pass(&mut shadow_view, true, 1.5);
        add_graph_late_shadow_pass(&mut shadow_view, true, 0.5);
        assert_eq!(
            shadow_view.graph_shadow_passes_ms, None,
            "a shadow view's own run is not a camera's sun pass"
        );
        assert_eq!(
            (shadow_view.graph_prepass_ms, shadow_view.graph_main_pass_ms),
            (Some(1.5), Some(0.5)),
            "its windows stay in the stage they sit in, as they were before the split"
        );
    }

    /// The head window's column takes a camera run's chain, and a shadow view's own run keeps it in
    /// the prepass stage it sits at the head of, so the column is the camera runs' chain alone; the
    /// upscaling window goes to its own column and, nested like the early stage, into the
    /// post-process sum.
    #[test]
    fn the_head_and_tail_columns_take_camera_runs_only() {
        let mut camera = CycleSides::default();
        add_view_gpu_preprocess(&mut camera, false, 1.5);
        assert_eq!(
            camera.graph_gpu_preprocess_ms,
            Some(1.5),
            "a camera run's head window is the column's"
        );
        assert_eq!(
            camera.graph_prepass_ms, None,
            "a camera run's head window is exclusive of the prepass stage"
        );

        let mut shadow_view = CycleSides::default();
        add_view_gpu_preprocess(&mut shadow_view, true, 1.5);
        assert_eq!(
            shadow_view.graph_gpu_preprocess_ms, None,
            "a shadow view's own run is not a camera's chain"
        );
        assert_eq!(
            shadow_view.graph_prepass_ms,
            Some(1.5),
            "its head window stays in the prepass stage it sits at the head of"
        );

        let mut tail = CycleSides::default();
        add_graph_upscaling(&mut tail, 0.5);
        assert_eq!(
            tail.graph_upscaling_ms,
            Some(0.5),
            "the upscaling window is its own column"
        );
        assert_eq!(
            tail.graph_post_process_ms,
            Some(0.5),
            "the upscaling window is inside the post-process sum as well, as the early stage is"
        );
    }

    /// A camera run's sun shadow passes and its GPU-preprocessing head window are lapped inside the
    /// run and kept out of the prepass and main-pass sums, so the columns tile the run exactly. The
    /// identity is the check on the exclusivity: a stage sum that also carried one of those windows
    /// would count it twice and the columns could not add up to the run. (The window sizes
    /// themselves are not asserted: this test runs beside the rest of the suite, so a window can be
    /// descheduled for far longer than its sleep.)
    #[test]
    fn the_sun_shadow_passes_of_a_camera_run_are_lapped_apart() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, false);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_render();
        one_run(&timings, false);
        timings.end_render();
        timings.start_main(0.0);
        let cycle = timings.take_previous_cycle().expect("cycle 1");

        let shadow = cycle
            .graph_shadow_passes_ms
            .expect("the sun pass laps ran in the camera run");
        let gpu_preprocess = cycle
            .graph_gpu_preprocess_ms
            .expect("the head window lap ran in the camera run");
        let prepass = cycle.graph_prepass_ms.expect("the prepass laps ran");
        let main_pass = cycle.graph_main_pass_ms.expect("the main-pass laps ran");
        let early_post = cycle
            .graph_early_post_process_ms
            .expect("the early post-process lap ran");
        let post = cycle
            .graph_post_process_ms
            .expect("the post-process lap ran");
        let camera = cycle
            .graph_camera_views_ms
            .expect("the camera run was lapped");

        assert!(
            shadow >= 2.0,
            "the column covers both sun pass windows: {shadow}"
        );
        assert!(
            gpu_preprocess >= 1.0,
            "the head window is its own column in a camera run: {gpu_preprocess}"
        );
        assert!(
            prepass >= 2.0 && main_pass >= 2.0,
            "each stage sum covers the run's own windows for it: {prepass} prepass, {main_pass} \
             main pass"
        );
        assert!(
            (gpu_preprocess + prepass + main_pass + early_post + (post - early_post) + shadow
                - camera)
                .abs()
                < 1e-9,
            "the stages, the head window and the sun passes tile the camera run: {gpu_preprocess} + \
             {prepass} + {main_pass} + {early_post} + {} + {shadow} vs {camera}",
            post - early_post
        );
    }

    /// A shadow view's own run measures the same two windows — there `shared_shadow_pass` draws
    /// that view's map — and they stay in the stage they sit in, so the run's stage sums still tile
    /// its kind and the sun pass column stays empty.
    #[test]
    fn a_shadow_views_run_tiles_without_the_sun_pass_column() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, false);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_render();
        one_run(&timings, true);
        timings.end_render();
        timings.start_main(0.0);
        let cycle = timings.take_previous_cycle().expect("cycle 1");

        assert_eq!(
            cycle.graph_shadow_passes_ms, None,
            "the column is the camera runs' sun passes alone"
        );
        assert_eq!(
            cycle.graph_gpu_preprocess_ms, None,
            "the head-window column is the camera runs' chain alone too"
        );
        let prepass = cycle.graph_prepass_ms.expect("the prepass laps ran");
        let main_pass = cycle.graph_main_pass_ms.expect("the main-pass laps ran");
        let early_post = cycle
            .graph_early_post_process_ms
            .expect("the early post-process lap ran");
        let post = cycle
            .graph_post_process_ms
            .expect("the post-process lap ran");
        let shadow_views = cycle
            .graph_shadow_views_ms
            .expect("the shadow view run was lapped");
        assert!(
            prepass >= 4.0 && main_pass >= 3.0,
            "the run's own windows sink into the stages around them — the head window and the \
             early shadow pass each, and the late shadow pass: {prepass} prepass, {main_pass} main \
             pass"
        );
        assert!(
            (prepass + main_pass + early_post + (post - early_post) - shadow_views).abs() < 1e-9,
            "the stages still tile the shadow view's run"
        );
    }

    /// The engine's `Core3d` schedule holds Bevy's four sun shadow-pass systems, each ordered
    /// against systems that are in no stage set, and [`core3d_marks`] brackets them. This builds
    /// that schedule — Bevy's own registration, copied from its plugin, plus our marks — and checks
    /// it initializes, which is Bevy's own check that no ordering names a system or set the schedule
    /// does not hold and that no two constraints contradict each other (the dependency graph it
    /// builds is acyclic).
    ///
    /// What it cannot check is the order the executor settles on: Bevy 0.19 identifies a system by
    /// `TypeId::of::<FunctionSystem<M, F>>()`, a type a test cannot name, so a node in the built
    /// graph cannot be looked up from the system function to compare positions. The constraints
    /// themselves are pinned instead by the system functions [`core3d_marks`] names — a renamed or
    /// mistyped system would not compile — and by the windows the laps above measure.
    #[test]
    fn the_core3d_marks_agree_with_bevys_shadow_pass_ordering() {
        fn stub_prepass_chain() {}
        fn stub_main_pass() {}
        // One stub per stage set, standing in for the systems Bevy adds to them (`early_prepass`
        // and the late prepasses in `bevy_core_pipeline-0.19.0` `src/core_3d/mod.rs:145-163`, the
        // main passes at `:157-159`), so our `.before(Core3dSystems::Prepass)` and
        // `.after(Core3dSystems::MainPass)` have members to order against.
        let mut schedule = Schedule::new(Core3d);
        schedule.configure_sets((Core3dSystems::Prepass, Core3dSystems::MainPass).chain());
        schedule.add_systems((
            stub_prepass_chain.in_set(Core3dSystems::Prepass),
            stub_main_pass.in_set(Core3dSystems::MainPass),
            // The systems the shadow passes are ordered against, as their own plugins add them
            // (`bevy_pbr-0.19.0` `src/render/gpu_preprocess.rs:436-475`,
            // `bevy_core_pipeline-0.19.0` `src/mip_generation/mod.rs:296-304`).
            clear_indirect_parameters_metadata,
            unpack_bins,
            early_gpu_preprocess,
            early_prepass_build_indirect_parameters,
            late_prepass_build_indirect_parameters,
            main_build_indirect_parameters,
            early_downsample_depth,
            // Bevy's upscaling system, added to `Core3d` `after(Core3dSystems::PostProcess)`
            // (`bevy_core_pipeline-0.19.0` `src/core_3d/mod.rs:161`), which the post-process and
            // upscaling laps order against.
            upscaling,
            // Bevy's registration of the four, copied from `bevy_pbr-0.19.0` `src/lib.rs:424-444`.
            per_view_shadow_pass::<EARLY_SHADOW_PASS>
                .after(early_prepass_build_indirect_parameters)
                .before(early_downsample_depth)
                .before(per_view_shadow_pass::<LATE_SHADOW_PASS>),
            per_view_shadow_pass::<LATE_SHADOW_PASS>
                .after(late_prepass_build_indirect_parameters)
                .before(main_build_indirect_parameters)
                .before(Core3dSystems::MainPass),
            shared_shadow_pass::<EARLY_SHADOW_PASS>
                .after(early_prepass_build_indirect_parameters)
                .before(early_downsample_depth)
                .before(shared_shadow_pass::<LATE_SHADOW_PASS>),
            shared_shadow_pass::<LATE_SHADOW_PASS>
                .after(late_prepass_build_indirect_parameters)
                .before(main_build_indirect_parameters)
                .before(Core3dSystems::MainPass),
        ));
        schedule.add_systems(core3d_marks().run_if(tracks_cycles));

        let mut world = World::new();
        world.insert_resource(RenderTimings::default());
        if let Err(error) = schedule.initialize(&mut world) {
            panic!("our Core3d marks do not build with Bevy's own ordering: {error}");
        }
        assert!(
            schedule.systems_len() >= 24,
            "the two stubs, Bevy's four dependency systems, its early GPU-preprocessing chain, its \
             upscaling system, its four shadow passes and our ten marks are all in the schedule \
             (Bevy inserts its own sync points around them): {}",
            schedule.systems_len()
        );
    }

    /// The engine's root `RenderGraph` schedule holds Bevy's `camera_driver` in its `Render` set,
    /// and [`render_graph_marks`] orders the mark before the system and the lap after it. This
    /// builds that schedule — Bevy's own registration, copied from its plugin, plus our marks — and
    /// checks it initializes, which is Bevy's own check that no ordering names a system or set the
    /// schedule does not hold and that no two constraints contradict each other (the dependency
    /// graph it builds is acyclic).
    ///
    /// What it cannot check is the order the executor settles on, for the reason the `Core3d` test
    /// gives: a node in the built graph cannot be looked up from the system function.
    #[test]
    fn the_render_graph_marks_agree_with_bevys_camera_driver() {
        fn stub_begin() {}
        fn stub_finish() {}
        let mut schedule = Schedule::new(RenderGraph);
        schedule.configure_sets(
            (
                RenderGraphSystems::Begin,
                RenderGraphSystems::Render,
                RenderGraphSystems::Submit,
                RenderGraphSystems::Finish,
            )
                .chain(),
        );
        schedule.add_systems((
            stub_begin.in_set(RenderGraphSystems::Begin),
            stub_finish.in_set(RenderGraphSystems::Finish),
            // Bevy's registration, copied from `bevy_core_pipeline-0.19.0` `src/lib.rs:66`. The
            // `Submit` set's two systems are `pub(crate)` there and are not needed here: our laps
            // order against the set itself, not against them.
            camera_driver.in_set(RenderGraphSystems::Render),
        ));
        schedule.add_systems(render_graph_marks().run_if(tracks_cycles));

        let mut world = World::new();
        world.insert_resource(RenderTimings::default());
        if let Err(error) = schedule.initialize(&mut world) {
            panic!("our RenderGraph marks do not build with Bevy's own registration: {error}");
        }
        assert!(
            schedule.systems_len() >= 8,
            "the two stubs, Bevy's camera_driver and our five marks are all in the schedule (Bevy \
             inserts its own sync points around them): {}",
            schedule.systems_len()
        );
    }

    /// A cycle's runs are split by kind: a shadow view's four windows go to the shadow columns and
    /// a camera's to the camera ones, the count says how many of the cycle's views were shadows,
    /// and a kind that ran no view keeps its columns empty rather than a zero.
    #[test]
    fn the_view_kind_laps_split_the_runs_of_a_cycle() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, false);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_render();
        for is_shadow in [true, false, false] {
            timings.view_started(is_shadow);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_prepass);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_main_pass);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_early_post_process);
            std::thread::sleep(Duration::from_millis(1));
            timings.view_lapped(add_graph_post_process);
        }
        timings.views_lapped();
        timings.end_render();
        timings.start_main(0.0);
        let cycle = timings.take_previous_cycle().expect("cycle 1");
        assert_eq!(cycle.graph_view_count, Some(3), "every run is counted");
        assert_eq!(
            cycle.graph_shadow_view_count,
            Some(1),
            "one of the three runs was a shadow view"
        );
        let camera = cycle.graph_camera_views_ms.expect("two camera runs");
        let shadow = cycle.graph_shadow_views_ms.expect("one shadow run");
        assert!(
            camera >= 4.0 && shadow >= 4.0,
            "each kind's sum covers every window of its runs' sleeps: {camera} camera, {shadow} \
             shadow"
        );
        // The stage sums are over both kinds together, so they equal the two kind sums added up.
        let stages = cycle.graph_prepass_ms.unwrap()
            + cycle.graph_main_pass_ms.unwrap()
            + cycle.graph_early_post_process_ms.unwrap()
            + (cycle.graph_post_process_ms.unwrap() - cycle.graph_early_post_process_ms.unwrap());
        assert!(
            (stages - (camera + shadow)).abs() < 1e-9,
            "the per-stage sums cover the same windows the kind sums do: {stages} vs {}",
            camera + shadow
        );

        // A cycle of camera runs only: the shadow columns stay empty.
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_render();
        timings.view_started(false);
        timings.view_lapped(add_graph_prepass);
        timings.view_lapped(add_graph_main_pass);
        timings.view_lapped(add_graph_early_post_process);
        timings.view_lapped(add_graph_post_process);
        timings.views_lapped();
        timings.end_render();
        timings.start_main(0.0);
        let cycle = timings.take_previous_cycle().expect("cycle 2");
        assert_eq!(cycle.graph_view_count, Some(1));
        assert_eq!(
            cycle.graph_shadow_view_count, None,
            "no shadow view ran, so the count is not a zero"
        );
        assert!(cycle.graph_camera_views_ms.is_some());
        assert!(
            cycle.graph_shadow_views_ms.is_none(),
            "a kind that ran no view keeps its sum empty"
        );
    }

    /// The run's kind comes off the entity `camera_driver` filed as `CurrentView`: a view without
    /// the `ShadowView` marker is a camera run, and a run whose `CurrentView` is missing (nothing
    /// this app does) is counted as a camera too rather than dropped.
    ///
    /// The marker itself cannot be built in a unit test — `ShadowView` carries a wgpu `TextureView`
    /// (`bevy_render-0.19.0` `src/texture/texture_attachment.rs:88-103`) — so the shadow branch of
    /// the classification is covered by the tally tests above, which drive the run's kind directly.
    #[test]
    fn a_run_without_the_shadow_view_marker_is_counted_as_a_camera() {
        let timings = RenderTimings::default();
        timings.set_cycle_tracking(true, false);
        timings.start_main(0.0);
        timings.end_main();
        timings.extracted(Duration::from_millis(1));
        timings.start_render();

        let mut app = App::new();
        app.insert_resource(timings.clone());
        app.add_systems(Update, start_view);
        // No `CurrentView` yet: the run is still counted, as a camera.
        app.update();
        let view = app.world_mut().spawn_empty().id();
        app.world_mut().insert_resource(CurrentView(view));
        app.update();
        assert!(
            timings.state().view_mark.is_some(),
            "the second run took its own mark"
        );

        timings.end_render();
        timings.start_main(0.0);
        let cycle = timings.take_previous_cycle().expect("cycle 1");
        assert_eq!(
            cycle.graph_view_count,
            Some(2),
            "both runs are counted, one with no CurrentView at all"
        );
        assert_eq!(
            cycle.graph_shadow_view_count, None,
            "a view without the marker is not a shadow view"
        );
        assert!(
            cycle.graph_camera_views_ms.is_none(),
            "no window was lapped"
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
