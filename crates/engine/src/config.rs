use crate::{
    color_pipeline::{SCENE_GRADE_VALUES, SceneGradeMode, SceneVisualPreset},
    skyrim_ini::SkyrimIni,
    tone_mapping::SceneToneMapMode,
    world::components::CELL_SIZE,
    world::imagespace::{ImageSpaceSelection, ImageSpaceSlot},
};
use bevy::prelude::Resource;
use shared::lod::LodTier;
use std::{fmt, path::PathBuf};

/// Most IO threads `--io-threads` accepts; `0` still asks for the automatic size. Each thread
/// reserves 128 MiB of address space (`app::IO_TASK_STACK_BYTES`), so an unbounded count could
/// exhaust it and make Bevy panic spawning threads. 64 is well past the 4 the automatic size
/// uses and the 8 that were measured.
pub(crate) const MAX_IO_THREADS: usize = 64;

/// What `--io-threads` takes, with [`MAX_IO_THREADS`] spelled out; a test holds the two equal.
const IO_THREADS_EXPECTED: &str = "a thread count from 0 to 64";

/// How far each terrain LOD tier draws, in Skyrim's `[TerrainManager]` terms:
/// a tier reaches its block distance times `fSplitDistanceMult`, in Creation
/// units from the camera. `--ini` reads these from a `SkyrimPrefs.ini`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainLodDistances {
    /// `fBlockLevel0Distance`: level-4 tier.
    pub block_level0_distance: f32,
    /// `fBlockLevel1Distance`: level-8 tier.
    pub block_level1_distance: f32,
    /// `fBlockMaximumDistance`: level-16 tier.
    pub block_maximum_distance: f32,
    /// `fSplitDistanceMult`: terrain multiplier on the three block distances.
    pub split_distance_mult: f32,
}

impl Default for TerrainLodDistances {
    /// Skyrim Special Edition's `SkyrimPrefs.ini` defaults, so an install without
    /// an INI draws as far as the game does: about 12, 25 and 91 cells for
    /// tiers 4, 8 and 16.
    fn default() -> Self {
        Self {
            block_level0_distance: 35_000.0,
            block_level1_distance: 70_000.0,
            block_maximum_distance: 250_000.0,
            split_distance_mult: 1.5,
        }
    }
}

impl TerrainLodDistances {
    /// The Chebyshev cell distance from the camera's cell out to which `tier` draws.
    pub fn reach_cells(&self, tier: LodTier) -> i32 {
        let block = match tier {
            LodTier::Tier4 => self.block_level0_distance,
            LodTier::Tier8 => self.block_level1_distance,
            LodTier::Tier16 => self.block_maximum_distance,
        };
        // `as` saturates, so an enormous configured distance means "everything".
        (f64::from(block) * f64::from(self.split_distance_mult) / f64::from(CELL_SIZE)).floor()
            as i32
    }

    /// The farthest reach of any tier.
    pub fn max_reach_cells(&self) -> i32 {
        LodTier::ALL
            .map(|tier| self.reach_cells(tier))
            .into_iter()
            .max()
            .unwrap_or(0)
    }
}

impl TerrainLodDistances {
    /// The Chebyshev cell distance from the camera's cell out to which an object LOD block of
    /// `level` cells draws: Skyrim's block distances without the terrain multiplier (the object
    /// LOD distances times `fSplitDistanceMult` are the terrain LOD distances, so the object
    /// ones are the block distances as written). Levels 4 and 8 follow `fBlockLevel0Distance` and
    /// `fBlockLevel1Distance`; 16 and 32 follow `fBlockMaximumDistance`.
    pub fn object_reach_cells(&self, level: u8) -> i32 {
        // A hand-tuned INI may order the distances oddly; a coarser level never reaches less far
        // than a finer one, or the ring between them would draw nothing.
        let level0 = self.block_level0_distance;
        let level1 = level0.max(self.block_level1_distance);
        let maximum = level1.max(self.block_maximum_distance);
        let block = match level {
            0..=4 => level0,
            5..=8 => level1,
            _ => maximum,
        };
        (f64::from(block) / f64::from(CELL_SIZE)).floor() as i32
    }
}

/// How far the distant tree billboards of the package's Skyrim LOD inventory reach. Whether the
/// objects and the trees draw at all is a graphics setting (`--lod-objects`, `--lod-trees`, the
/// panel; see `crate::graphics_settings`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectLodSettings {
    /// `fTreeLoadDistance` (`[TerrainManager]`): how far tree billboards draw, in Creation units.
    pub tree_load_distance: f32,
}

impl Default for ObjectLodSettings {
    /// Skyrim Special Edition's `fTreeLoadDistance` of 75,000 units.
    fn default() -> Self {
        Self {
            tree_load_distance: 75_000.0,
        }
    }
}

impl ObjectLodSettings {
    /// The Chebyshev cell distance out to which tree billboards draw.
    pub fn tree_reach_cells(&self) -> i32 {
        (f64::from(self.tree_load_distance) / f64::from(CELL_SIZE)).floor() as i32
    }
}

/// Bevy Remote Protocol's default HTTP port, used by `--remote` unless `--remote-port` is given.
pub const DEFAULT_REMOTE_PORT: u16 = 15702;

#[derive(Debug, Clone, Resource)]
pub struct EngineConfig {
    /// Explicit launch options. Saved interactive settings defer to these and to `--ini`.
    pub settings_overrides: Vec<String>,
    pub assets_dir: PathBuf,
    /// Explicit local report destination. `None` uses the per-user application-data directory.
    pub bug_report_dir: Option<PathBuf>,
    pub worldspace_id: u32,
    pub start_grid: (i32, i32),
    pub stream_radius: i32,
    pub unload_radius: i32,
    /// Optional upper bound for sun-shadow cascades from `[Display] fShadowDistance`.
    pub sun_shadow_distance_limit: Option<f32>,
    /// Optional sun bearing clockwise from north and elevation above the horizon, in degrees.
    pub sun_position: Option<(f32, f32)>,
    pub terrain_lod: TerrainLodDistances,
    /// Which distant objects are drawn from the package's Skyrim LOD inventory.
    pub object_lod: ObjectLodSettings,
    /// How far the terrain-only ring reaches, in cells from the camera cell: a cell outside
    /// [`stream_radius`](Self::stream_radius) but within this distance streams its landscape and its
    /// water plane and nothing else, so the world does not end at the full-detail edge.
    ///
    /// A value at or below `stream_radius` leaves the ring empty, which is the engine without a
    /// ring (`--stream-radius 2` with the default) and what a benchmark run that wants the old
    /// numbers passes. Interior cells are never terrain-only.
    ///
    /// This is the residency envelope, whether explicit or [`DEFAULT_TERRAIN_RADIUS`]. Only actual
    /// LAND cells require terrain requests. The LOD selector can hide a resident stand-in once
    /// its exact replacement draws, and restore it if that coverage withdraws.
    pub terrain_radius: i32,
    /// Whether `--terrain-radius` named the detailed ring. Explicit detailed terrain remains
    /// authoritative; an automatic ring may yield visually to draw-confirmed LOD while retaining
    /// its allocation for coverage withdrawal. Benchmark defaults still choose an empty ring.
    pub terrain_radius_given: bool,
    /// The cluster overlay (ADR-0013 output, `<assets>/derived/clusters-<settings-id>`) to draw
    /// objects from, as clusters in place of the stock models it owns. `--clusters [label |
    /// settings-id | path]`: the text names one overlay of `--assets`, empty for the only one.
    /// Unset draws nothing extra.
    pub clusters: Option<String>,
    /// Experiment: hide certified owned exterior primitives once at instance spawn.
    pub cluster_own_statics: bool,
    /// Draw packaged terrain clusters, clipping against ready near terrain.
    pub cluster_terrain: bool,
    /// `--cluster-preload-batches`: request every cluster batch's material during the startup
    /// load, nearest first, instead of as each comes into view. The startup cover then waits for
    /// all of them, and nothing a view or a crossing proof depends on changes during play.
    pub cluster_preload_batches: bool,
    /// Extend the selected sun beyond CSM using certified canonical terrain.
    pub cluster_far_terrain_shadows: bool,
    /// `--cluster-min-instance-pixels`: instances whose bounding sphere spans fewer pixels than
    /// this across are not drawn (default 0: every instance).
    pub cluster_min_instance_pixels: Option<f32>,
    /// `--no-cluster-prepass`: leave the cluster batches out of the camera's prepass.
    pub cluster_prepass: bool,
    pub max_cell_commits_per_frame: usize,
    pub max_commit_micros_per_frame: u64,
    /// Retiring roots - cell roots and LOD chunks - a frame may despawn. `0` despawns every one at
    /// once, which is the unbudgeted behaviour unloading used to have - and, with
    /// [`Self::max_unload_ms_per_frame`] `0` too, the only setting that reproduces it exactly
    /// (with the retire's hide, which the old path did not do, but no pacing). With this count
    /// budget pacing instead, the drain keeps its overflow escape (streaming's retire-backlog
    /// drain), which un-bounds a frame whose retiring backlog stopped shrinking - a trip on drain
    /// failure, not on size, that the old unbudgeted path never had. The escape counts cells, not
    /// chunks: it exists for sustained flight retiring cells faster than the drain, and a chunk
    /// backlog - one entry per LOD window the camera leaves, drained at least one a frame - is
    /// what it is by the chunk grid, not by speed. Paces the unload only while
    /// [`Self::max_unload_ms_per_frame`] is `0`: the wall-clock budget replaces it once set.
    pub max_cell_unloads_per_frame: usize,
    /// Wall-clock milliseconds a frame may spend despawning retiring cells. The despawn paces
    /// itself by the clock: it unloads retiring cells in stable order until the budget is spent,
    /// always at least one of them, so a backlog bigger than the budget drains over several frames
    /// instead of landing on one - fast flight away from the camera retires a whole ring of cells
    /// at once, far past what a per-frame cell count can pace.
    ///
    /// Scope: this paces the *distance* retires of the space the camera stands in - the far cells
    /// a flight leaves behind - and the LOD chunks that space unloads, including the whole
    /// exterior's distant terrain when the camera walks through a door into an interior. A door
    /// crossing's unload of the *cells* of the space the camera left happens in `plan_cells` in
    /// the frame it falls out (`paces_its_unload` is false for it) and is not paced here.
    ///
    /// Replay stability: the order the roots unload in is stable (`cell_order_key`, and
    /// `ChunkKey`'s own order for the chunks), so a replayed crossing unloads the same roots in
    /// the same order; which frame each one unloads on depends on machine speed, because the
    /// budget is wall-clock.
    ///
    /// `0` turns the time budget off and paces by [`Self::max_cell_unloads_per_frame`]; only that
    /// count at `0` as well is exactly the old unbudgeted path, since the drain keeps its overflow
    /// escape (a trip on drain failure, not on size) under the count budget. The default is
    /// provisional; see the comment on it in `Default for EngineConfig`.
    pub max_unload_ms_per_frame: f64,
    /// `--lod-keep-hidden-in-interiors` (off by default): on a move into a space with no distant
    /// terrain - an interior - the exterior LOD chunks stop being drawn but stay resident and
    /// hidden instead of retiring under the unload budget, so the return to that exterior shows
    /// them the frame it happens instead of streaming a whole world's LOD in again. Off, the
    /// chunks retire under the same paced unload the cells use, and a return revives whatever the
    /// budget has not reached yet.
    ///
    /// The price is memory: the chunks' entities, world assets and meshes stay held for as long
    /// as the visit lasts. There is deliberately no timeout - a timeout would put the retire cost
    /// back on whichever frame it fired, which is what this flag exists to keep off the return,
    /// and a long interior visit is exactly what the measurement prices.
    pub lod_keep_hidden_in_interiors: bool,
    /// Converted models a frame may hand to Bevy's scene spawner. `0` arms every model whose asset
    /// is loaded, which is the unbudgeted behaviour a single spawn batch used to have.
    pub max_model_spawns_per_frame: usize,
    /// Distant LOD requests (terrain LOD chunks, object and tree LOD blocks) a frame may start,
    /// apart from the full-cell commits. `0` starts every queued request the frame's commit time
    /// budget allows. A request is cheap, but each one loads and spawns a scene, and hundreds at
    /// once stall the frames that land them (100-480 ms frames at Riverwood with 16 per frame,
    /// against 16 ms at one), so while the world shows they go one a frame; the first load lifts
    /// this with the other pacing budgets ([`crate::pacing::startup::StartupLoadPlugin`]).
    pub max_lod_requests_per_frame: usize,
    /// `--paced-startup`: keep the streaming pacing budgets - [`Self::max_model_spawns_per_frame`],
    /// [`Self::max_lod_requests_per_frame`],
    /// the two unload budgets and the render-asset upload budget - in force from the first frame.
    /// Off by default: until the world is first ready the first load runs with all three lifted, so
    /// it is paid for at load speed ([`crate::pacing::startup::StartupLoadPlugin`]).
    pub paced_startup: bool,
    /// `--startup-cover`: hold a black cover over the unpaced first load and fade it out once the
    /// world is ready, as a door crossing does. Off by default (the user, 2026-10-09: a dev build
    /// shows the world while it loads); kept for a release build.
    pub startup_cover: bool,
    /// Camera and bounds based queue ordering. Off preserves fixed FIFO activation.
    pub prioritize_streaming: bool,
    /// Outstanding unique Scene(0) jobs, including recursive CPU dependencies.
    /// Zero retains unlimited admission; this count does not bound memory bytes.
    pub max_scene_loads: usize,
    /// Opt-in startup, walking and recovery budgets. Also enables spatial priority.
    pub adaptive_streaming: bool,
    /// Downstream queue high watermark; zero disables it outside adaptive mode.
    pub max_streaming_backlog: usize,
    /// Estimated streaming allocation allowance. Zero disables memory admission
    /// outside adaptive mode, which otherwise uses a finite automatic allowance.
    pub streaming_memory_mib: usize,
    /// Override for converter-generated streaming cost metadata.
    pub streaming_costs: Option<PathBuf>,
    /// Free system memory reserved for the rest of the process and other apps.
    pub streaming_headroom_mib: usize,
    /// Controller frame target. Benchmark acceptance keeps its separate thresholds.
    pub streaming_frame_ms: f64,
    /// Threads in the asset IO pool. `0` sizes it to the machine: a quarter of the hardware
    /// threads, at least one and at most four. A nonzero count must be at most
    /// `MAX_IO_THREADS`; a larger one is refused like a malformed number.
    pub io_threads: usize,
    /// MiB of newly loaded render assets (meshes, textures) the renderer may
    /// prepare per frame. `0` prepares every asset the frame extracted.
    pub max_upload_mib_per_frame: usize,
    pub headless: bool,
    /// Keep a renderable window hidden for automated screenshot capture.
    pub hidden_window: bool,
    /// Select the cinematic A/B experiment without changing scene lighting.
    pub color_grade: SceneGradeMode,
    /// Named sky, fog and grading look; Vivid preserves the approved configuration.
    pub visual_preset: SceneVisualPreset,
    /// Optional approved-look JSON override; authored records are retained separately.
    pub grade_preset: Option<PathBuf>,
    /// Display transform selection; native-curve experiments require explicit parameters.
    pub tone_map: SceneToneMapMode,
    pub tone_map_params: Option<PathBuf>,
    pub image_space: Option<u32>,
    pub image_space_weather: Option<u32>,
    pub image_space_slot: Option<ImageSpaceSlot>,
    /// Seed a live world camera from a validated one-pose shots file.
    pub start_shot: Option<PathBuf>,
    /// Accept developer console commands on this process's stdin.
    pub stdin_console: bool,
    /// Disable Bevy's GPU timing and pipeline-statistics query collection.
    pub no_render_diagnostics: bool,
    pub benchmark_only: bool,
    pub benchmark_frames: Option<u32>,
    pub benchmark_duration_secs: Option<f64>,
    pub benchmark_warmup_frames: u32,
    /// Keep the interactive app's presentation mode for an opt-in benchmark.
    pub benchmark_vsync: bool,
    /// Dedicated post-settlement camera route; Some(0) measures its stationary tail.
    pub streaming_benchmark_route_speed: Option<f64>,
    pub streaming_benchmark_route_secs: f64,
    pub streaming_benchmark_tail_secs: f64,
    pub streaming_benchmark_settle_secs: f64,
    pub streaming_benchmark_expected_models: Option<u64>,
    pub streaming_benchmark_expected_lod_chunks: Option<usize>,
    pub streaming_benchmark_expected_terrain: Option<u64>,
    pub streaming_benchmark_expected_water: Option<u64>,
    /// `--benchmark-jump <grid-x>,<grid-y>`: once the world around the camera is first fully
    /// loaded, move the camera to the centre of that cell of the same worldspace and measure how
    /// long the world takes to become fully loaded again (`time_to_world_ready_after_jump_ms`).
    /// Coordinates must be within [`JUMP_GRID_LIMIT`]; anything further out is ignored with a
    /// warning, since the position maths would not survive it.
    pub benchmark_jump: Option<(i32, i32)>,
    pub benchmark_output: PathBuf,
    /// Where to write every measured frame time, in order, as CSV (`--benchmark-frame-times`).
    /// Off by default: the report's summary is what acceptance reads; the series is for choosing
    /// run lengths and spotting drift within a run.
    pub benchmark_frame_times: Option<PathBuf>,
    /// `--run-label <text>`: names an automated run in its window title, e.g. a benchmark's
    /// variant and round ([`EngineConfig::window_title`]).
    pub run_label: Option<String>,
    /// `--run-owner <text>`: the thread or person running an automated run, last in its window
    /// title so several engines on the taskbar can be told apart.
    pub run_owner: Option<String>,
    pub accept_min_fps: f64,
    pub accept_p95_ms: f64,
    pub accept_max_memory_growth_gib: f64,
    pub auto_fly_speed: f32,
    /// `--route <file>`: the camera follows a route file's waypoints at its speed (see
    /// [`crate::route`]). It owns the camera, like `--auto-fly-speed`.
    pub route: Option<PathBuf>,
    /// `--frame-budget-ms <ms>`: the frame time the report counts frames over (see
    /// [`crate::frame_budget`]). The default is 6.25, 160 frames per second.
    pub frame_budget_ms: f64,
    pub allow_incomplete_assets: bool,
    pub synthetic_instances: usize,
    /// `--terrain-audit`: compare incremental terrain indices with full rebuilds once per
    /// wall-clock second. Logs errors and counts desynchronizations; disabled by default.
    pub terrain_audit: bool,
    pub profile_output_dir: Option<PathBuf>,
    pub profile_gpu_inventory: bool,
    pub profile_scenario: String,
    pub profile_run_id: String,
    pub profile_commit: String,
    pub profile_dirty_worktree: bool,
    pub profile_hardware: String,
    /// `--probe-retire-visibility <dir>`: a measurement-only run that records whether a cell held
    /// in the retire backlog - already hidden, draining over frames under the unload budget - ever
    /// shows in a view. On each frame with a nonzero backlog it counts the retiring roots (and
    /// their mesh descendants) whose `ViewVisibility` is set, photographs the first few backlog
    /// frames, then writes `retire-visibility.json` and exits when the backlog has drained
    /// (non-zero on any hit, or if no backlog ever appeared). `None` (the default) installs no
    /// probe system, so an ordinary run is unchanged. See [`crate::streaming`].
    pub probe_retire_visibility: Option<PathBuf>,
    pub acceptance_screenshot: Option<PathBuf>,
    /// The water reflection camera's cost options ([`ReflectionOptions`]).
    pub reflection: ReflectionOptions,
    /// `--cull-counter`: read the GPU culling result back every few frames and report how many
    /// instances the main views submit and draw (`renderer.json`, `cull`). Off by default: it asks
    /// Bevy to let the indirect draw buffers be copied, which Bevy documents as a debugging
    /// feature that may cost performance. See [`crate::cull_counter`].
    pub cull_counter: bool,
    /// Whether the main camera carries `OcclusionCulling`: on, unless `--no-main-occlusion`. The
    /// switch exists to measure the culling by running a route with and without it.
    pub main_occlusion: bool,
    pub screenshot_camera_offset: Option<(f32, f32, f32)>,
    /// `--shots <file>`: render the camera poses in a shots file, one PNG each, and exit instead of
    /// running interactively. See [`crate::shots`].
    pub shots: Option<PathBuf>,
    /// Exact, fixed-step profiling traversal. Mutually exclusive with other camera drivers.
    pub matched_route: Option<PathBuf>,
    /// `--shots-out <dir>`: where the PNGs and `shots.log` go. `None` is
    /// [`crate::shots::default_output_dir`], a `<file stem>-shots/` folder beside the shots file.
    pub shots_out: Option<PathBuf>,
    /// Maximum settling time per shot, in seconds.
    pub shots_settle_timeout: f32,
    pub diagnostic_asset_fallbacks: bool,
    pub material_fixture: bool,
    pub terrain_water_fixture: bool,
    pub transform_bounds_fixture: bool,
    pub renderer_fixture: bool,
    pub streaming_fixture: bool,
    /// Exact start in Creation-engine units, overriding the default camera placement.
    pub start_position: Option<[f32; 3]>,
    /// Start heading in Creation-engine radians (rotation about Z), used with start_position.
    pub start_yaw: f32,
    /// The portal's own options: the walk, the demo starts, the scripted tour and the shots run.
    /// One struct, parsed by one block at the end of this file ([`PortalOptions`]).
    pub portal: PortalOptions,
    /// Serve the Bevy Remote Protocol on 127.0.0.1 (`--remote`). Needs the `remote` feature.
    pub remote: bool,
    pub remote_port: u16,
    /// Whether a streamed `LIGH` reference places a point light (`--lights`). Off by default, so
    /// every run that does not ask for lights renders exactly as it did before.
    pub lights: bool,
    /// Whether a measured run is presented as a played one (`--as-played`): the sky, fog and
    /// per-space lighting, the door and model animations and the portals an interactive run has
    /// ([`Self::interactive`]), so a route or benchmark measures what a player sees. Never the
    /// walking player: the run's camera stays scripted. Off by default, so a measured run keeps
    /// the flat lighting its gates were set for.
    pub as_played: bool,
    /// Whether the main camera culls interior objects by room and portal (`--room-culling`). Off
    /// by default; it needs the converter's room tables (`db-schema.md` section 15) and does
    /// nothing without them.
    pub room_culling: bool,
    pub physics_fixture: bool,
    /// `--water-reflection-preset <baseline|water>`: which [`crate::view_policy::ViewPolicy`] the
    /// water reflection camera is given. `baseline` is the full-size, every-frame reflection the
    /// engine has always drawn.
    pub water_reflection_preset: WaterReflectionPreset,
    /// `--water-reflection <on|off|planar|ssr>`: whether the water reflection view is drawn at
    /// all (`off` is the measurement switch; `on` and `planar` draw it, and `ssr` is the Water Plus
    /// reflection of [`Self::water_reflection_mode`]). See [`HELP_TEXT`].
    pub water_reflection: bool,
    /// `--water-reflection <planar|ssr>`: how the water reflects. `ssr` (the Plus default, the
    /// user's call 2026-10-10) marches the screen in the water shader, and the reflection camera is
    /// never demanded ([`crate::water_ssr`]); `planar` is the mirrored reflection view the engine
    /// drew before.
    pub water_reflection_mode: WaterReflectionMode,
    /// `--water-ssr-steps <linear>[,<bisection>]`: how many steps the `ssr` march takes per water
    /// pixel. Only read with `--water-reflection ssr`.
    pub water_ssr_steps: WaterSsrSteps,
    /// `--shadows <on|off>`: whether the sun casts shadows. A measurement switch; see
    /// [`HELP_TEXT`].
    pub shadows: bool,
    /// `--render-parallel-finish`: finish the render command encoders in parallel (the Mudcrab
    /// patch to the vendored `bevy_render`, impl-1132). On by default in Plus (the user's call
    /// 2026-10-10); `--no-render-parallel-finish` is stock Bevy.
    pub render_parallel_finish: bool,
    /// `--render-backends <all|vulkan>`: which graphics backends wgpu may pick from. `all` is
    /// Bevy's own choice and the default; `vulkan` is optional (its window gained 0.56 ms alone and
    /// nothing on top of the parallel finish, so it stayed off, impl-1132).
    pub render_backends: RenderBackends,
}

/// The graphics backends the renderer may use (`--render-backends`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderBackends {
    /// Bevy's default: every backend wgpu has for the platform (`WGPU_BACKEND` still applies).
    #[default]
    All,
    /// Vulkan only. wgpu then never needs the DX12 indirect-call validation pass, so the engine
    /// also drops `VALIDATION_INDIRECT_CALL` in release builds (see `render::wgpu_settings`).
    Vulkan,
}

impl RenderBackends {
    /// Every choice, in the order the help lists them.
    pub const ALL: [Self; 2] = [Self::All, Self::Vulkan];

    /// The spelling the command line and run records use.
    pub const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Vulkan => "vulkan",
        }
    }
}

/// How the water reflects (`--water-reflection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterReflectionMode {
    /// The mirrored reflection camera the engine has always drawn.
    #[default]
    Planar,
    /// Water Plus: a screen-space ray march in the water shader ([`crate::water_ssr`]). The
    /// reflection camera is never demanded.
    Ssr,
}

impl WaterReflectionMode {
    /// The spelling the command line and run records use.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Planar => "planar",
            Self::Ssr => "ssr",
        }
    }
}

/// The step counts of the screen-space water reflection's ray march (`--water-ssr-steps`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterSsrSteps {
    /// Equal steps along the ray to find the first place it passes behind the depth buffer.
    pub linear: u32,
    /// Halvings of the last linear step that refine that place.
    pub bisection: u32,
}

impl WaterSsrSteps {
    /// 16 linear and 6 bisection steps.
    pub const DEFAULT: Self = Self {
        linear: 16,
        bisection: 6,
    };
    /// The most linear steps `--water-ssr-steps` accepts. The counts are shader definitions, so
    /// the loops have constant bounds and this bounds the variants a run can ask for.
    pub const MAX_LINEAR: u32 = 128;
    /// The most bisection steps it accepts: each halves the last linear step, and 32 halvings are
    /// past what an `f32` can tell apart.
    pub const MAX_BISECTION: u32 = 32;

    /// Parses `<linear>` or `<linear>,<bisection>`; an omitted bisection count keeps the default.
    /// `None` for a count that is not a number, 0 or more than [`Self::MAX_LINEAR`] linear steps,
    /// or more than [`Self::MAX_BISECTION`] bisection steps.
    pub fn parse(raw: &str) -> Option<Self> {
        let (linear, bisection) = match raw.split_once(',') {
            Some((linear, bisection)) => (linear, Some(bisection)),
            None => (raw, None),
        };
        let linear: u32 = linear.parse().ok()?;
        let bisection: u32 = match bisection {
            Some(bisection) => bisection.parse().ok()?,
            None => Self::DEFAULT.bisection,
        };
        ((1..=Self::MAX_LINEAR).contains(&linear) && bisection <= Self::MAX_BISECTION)
            .then_some(Self { linear, bisection })
    }
}

impl Default for WaterSsrSteps {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The view policy the water reflection camera is given (`--water-reflection-preset`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterReflectionPreset {
    /// [`crate::view_policy::ViewPolicy::baseline`]: today's full-size, every-frame reflection.
    #[default]
    Baseline,
    /// [`crate::view_policy::ViewPolicy::water`]: a quarter of the pixels, every other frame, no
    /// shadows of its own, and a camera without occlusion history or MSAA.
    Water,
}

impl WaterReflectionPreset {
    /// Every preset, in the order the help lists them.
    pub const ALL: [Self; 2] = [Self::Baseline, Self::Water];

    /// The spelling the command line and run records use.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Water => "water",
        }
    }
}

/// The default reach of the terrain-only ring, in cells: eight cells is 32,768 units of landscape
/// beyond the full-detail grid, drawn from 264 cells.
///
/// The default only where the worldspace has no terrain LOD data of its own. Where it has - the
/// game's own worlds do - a run that did not name a ring draws no ring at all: the LOD already
/// covers that distance, and the ring is a second copy of landscape the game itself draws only as
/// LOD (`--terrain-radius` restores it for a comparison). Vanilla's view is therefore the default
/// one: full detail around the camera, the LOD beyond it, and no ring in between.
///
/// Chosen by measurement, on a `--shots` camera pose over the Pale from the Alftand ruins
/// (`far-pale-yaw40`), release build, one run each, warm:
///
/// | `terrain_radius` | ring cells | average FPS | 95th percentile frame |
/// |---|---|---|---|
/// | 2 (no ring) | 0 | 180 | 6.6 ms |
/// | 8 | 264 | 97 | 12.1 ms |
/// | 10 | 400 | 59 | 26.5 ms |
/// | 12 | 600 | 59 | 18.9 ms |
/// | 16 | 1,064 | 20 | 54.2 ms |
///
/// Each ring cell is four landscape draws of its own - one per quadrant, each with its own
/// material - and costs the frame about 20 µs, so a ring's price is its cell count. Eight is the
/// largest that stays clearly above the sixty-frame acceptance gate with the whole Pale in view;
/// ten and twelve sit on the gate and sixteen falls through it. Frames were measured while other
/// work ran on this machine and single runs differ by a quarter either way, so read the table as
/// order of magnitude rather than to the frame.
pub const DEFAULT_TERRAIN_RADIUS: i32 = 8;

/// The largest ring `--terrain-radius` accepts. A ring this wide is 4,225 cells and already more
/// landscape than a run holds; the clamp is a guard against a typo asking for millions of cells.
pub const MAX_TERRAIN_RADIUS: i32 = 32;

impl Default for EngineConfig {
    /// Start with the standard engine settings and optional profiling diagnostics disabled.
    fn default() -> Self {
        Self {
            settings_overrides: Vec::new(),
            assets_dir: PathBuf::from("modern_assets"),
            bug_report_dir: None,
            worldspace_id: 0x3c,
            start_grid: (0, 0),
            // Skyrim's default `uGridsToLoad=5`.
            stream_radius: 2,
            unload_radius: 3,
            sun_shadow_distance_limit: None,
            sun_position: None,
            terrain_lod: TerrainLodDistances::default(),
            object_lod: ObjectLodSettings::default(),
            terrain_radius: DEFAULT_TERRAIN_RADIUS,
            clusters: None,
            cluster_own_statics: false,
            cluster_terrain: false,
            cluster_preload_batches: false,
            cluster_far_terrain_shadows: false,
            cluster_min_instance_pixels: None,
            cluster_prepass: true,
            terrain_radius_given: false,
            max_cell_commits_per_frame: 1,
            max_commit_micros_per_frame: 16_670,
            max_cell_unloads_per_frame: 2,
            // PROVISIONAL until the shared timing-window A/B picks the value: an estimate from
            // the 2026-10-08 re-profile, which measured ~0.47 ms of despawn per retiring cell
            // (204.5 ms of `Update` unloading 434 cells on one frame). 3 ms of despawn a frame is
            // therefore about 6 cells, so a several-hundred-cell flight backlog drains over
            // frames instead of landing 136-205 ms on one; it is still only a few ms on a frame
            // that is already over budget, and it shrinks with the work instead of guessing a
            // cell count. The 434-cell unload is where the per-cell cost was measured, not a
            // backlog this budget drains: a crossing's space-change unload runs unpaced in
            // `plan_cells`, and this budget covers distance retires only. `0` paces by
            // `max_cell_unloads_per_frame`, with the drain-failure escape the old path never had.
            max_unload_ms_per_frame: 3.0,
            // The alternative to the paced LOD retire itself: hold the exterior's LOD chunks
            // through an interior visit instead of retiring them. Off by default - the paced
            // retire is the behaviour every run keeps unless a measurement asks for the other.
            lod_keep_hidden_in_interiors: false,
            // After a jump, the models of several cells are already loaded and reach the scene
            // spawner together: on the stress scenario the backlog runs to about 2,000 models. At 4
            // a frame that backlog took ~500 frames to clear; at 32 the world is ready 2.5x sooner
            // with the same worst frame, because a frame hands over more of the queue without
            // instantiating so many models at once that the frame blows past 60 fps. The limit
            // still spreads a burst over frames; `0` arms the whole batch at once, the behaviour
            // before this budget existed (docs/roadmap/02-profiling.md, "Load speed defaults").
            max_model_spawns_per_frame: 32,
            max_lod_requests_per_frame: 1,
            prioritize_streaming: false,
            max_scene_loads: 0,
            adaptive_streaming: false,
            max_streaming_backlog: 0,
            streaming_memory_mib: 0,
            streaming_costs: None,
            streaming_headroom_mib: 2_048,
            streaming_frame_ms: 16.67,
            // A quarter of the hardware threads, chosen at startup (see `app::io_pool_threads`).
            io_threads: 0,
            paced_startup: false,
            startup_cover: false,
            // Three 2K BC7/UASTC textures with a full mip chain (~5.3 MiB each):
            // a cell's new textures spread over a few frames instead of landing
            // in one 13 ms upload burst, and at 60 fps the budget still admits
            // far more new assets than the streaming radius can produce.
            max_upload_mib_per_frame: 16,
            headless: false,
            hidden_window: false,
            color_grade: SceneGradeMode::SnapshotSrgb,
            visual_preset: SceneVisualPreset::CoolGray,
            grade_preset: None,
            tone_map: SceneToneMapMode::Tony,
            tone_map_params: None,
            image_space: None,
            image_space_weather: None,
            image_space_slot: None,
            start_shot: None,
            stdin_console: false,
            no_render_diagnostics: false,
            benchmark_only: false,
            benchmark_frames: None,
            benchmark_duration_secs: None,
            benchmark_warmup_frames: 60,
            benchmark_vsync: false,
            streaming_benchmark_route_speed: None,
            streaming_benchmark_route_secs: 45.0,
            streaming_benchmark_tail_secs: 10.0,
            streaming_benchmark_settle_secs: 5.0,
            streaming_benchmark_expected_models: None,
            streaming_benchmark_expected_lod_chunks: None,
            streaming_benchmark_expected_terrain: None,
            streaming_benchmark_expected_water: None,
            benchmark_jump: None,
            benchmark_output: PathBuf::from("benchmark-report.json"),
            benchmark_frame_times: None,
            run_label: None,
            run_owner: None,
            accept_min_fps: 60.0,
            accept_p95_ms: 16.67,
            accept_max_memory_growth_gib: 0.5,
            auto_fly_speed: 0.0,
            route: None,
            frame_budget_ms: crate::frame_budget::DEFAULT_FRAME_BUDGET_MS,
            allow_incomplete_assets: false,
            synthetic_instances: 250_000,
            terrain_audit: false,
            profile_output_dir: None,
            profile_gpu_inventory: false,
            profile_scenario: "adhoc".into(),
            profile_run_id: "run-1".into(),
            profile_commit: "unknown".into(),
            profile_dirty_worktree: false,
            profile_hardware: "unspecified".into(),
            probe_retire_visibility: None,
            acceptance_screenshot: None,
            reflection: ReflectionOptions::default(),
            cull_counter: false,
            main_occlusion: true,
            screenshot_camera_offset: None,
            shots: None,
            matched_route: None,
            shots_out: None,
            shots_settle_timeout: crate::shots::SETTLE_TIMEOUT_SECONDS,
            diagnostic_asset_fallbacks: false,
            material_fixture: false,
            terrain_water_fixture: false,
            transform_bounds_fixture: false,
            renderer_fixture: false,
            streaming_fixture: false,
            start_position: None,
            start_yaw: 0.0,
            portal: PortalOptions::default(),
            remote: false,
            remote_port: DEFAULT_REMOTE_PORT,
            lights: false,
            as_played: false,
            room_culling: false,
            physics_fixture: false,
            water_reflection_preset: WaterReflectionPreset::Baseline,
            water_reflection: true,
            water_reflection_mode: WaterReflectionMode::Ssr,
            water_ssr_steps: WaterSsrSteps::DEFAULT,
            shadows: true,
            render_parallel_finish: true,
            render_backends: RenderBackends::All,
        }
    }
}

/// The text `--help` prints, and the list of options the engine accepts.
///
/// This and the `match` in [`EngineConfig::from_args`] are the two places an
/// option is named; the tests read the match arms back out of this file and
/// check the two agree, so neither can drift from the other.
pub const HELP_TEXT: &str = "\
Mudcrab engine

Usage: engine [options]

Streams converted cells from a world database. With no options it reads
\"modern_assets\" and starts in worldspace 0x3c at cell (0, 0).

Assets and start position:
  --assets <dir>                        converted asset directory to stream (default: modern_assets)
  --worldspace <id>                     worldspace form id, decimal or 0x-hex (default: 0x3c)
  --grid-x <cell>                       starting cell x (default: 0)
  --grid-y <cell>                       starting cell y (default: 0)
  --start-shot <file> [name]           start at the sole exterior pose, or a named pose in its space
  --stdin-console                     accept developer console commands on stdin
  --no-render-diagnostics             disable GPU timing and pipeline-statistics queries
  --allow-incomplete-assets             run despite a failed manifest or integration-report gate
  --headless                            run without opening a window
  --hidden-window                       render screenshots without showing a window
  --lights                              place a point light for each streamed LIGH reference
  --as-played                           present a measured run as a played one (sky, fog, animations, portals)
  --room-culling                        cull interior objects by room and portal (needs the room tables)

Streaming:
  --ini <file>                        layer Skyrim INI settings; CLI options override files
  --stream-radius <cells>               cells streamed around the camera (default: 2)
  --max-commit-ms <ms>                  cell commit time allowed per frame (default: 16.67)
  --max-unloads-per-frame <count>       retiring cell roots and LOD chunks despawned per frame; 0
                                        despawns all at once (default: 2)
  --max-unload-ms-per-frame <ms>        wall-clock time a frame may spend despawning retiring roots
                                        (cell roots and LOD chunks); 0 paces by --max-unloads-per-frame
                                        instead (default: 3)
  --lod-keep-hidden-in-interiors        keep exterior LOD chunks resident but hidden across an
                                        interior visit instead of retiring them; off, they retire
                                        under the unload budget and a return revives what is left
  --max-model-spawns-per-frame <count>  models spawned per frame; 0 spawns all at once (default: 32)
  --max-lod-requests-per-frame <count>  distant LOD requests started per frame, lifted during the
                                        first load; 0 starts all at once (default: 1)
  --prioritize-streaming               prioritize nearby collision and view-facing bounds
  --max-scene-loads <count>             outstanding unique scene jobs; 0 is unlimited (default: 0)
  --adaptive-streaming                 adapt startup/walking budgets; enables spatial priority
  --max-streaming-backlog <count>      downstream queue limit; 0 disables it, adaptive uses 256
  --streaming-memory-mib <mib>         estimated allowance; 0 disables it unless adaptive
                                     adaptive: half detected RAM, at most 16 GiB (fallback: 8 GiB)
  --streaming-costs <file>             override streaming cost metadata (default: asset pack metadata)
  --streaming-headroom-mib <mib>       system memory kept free (default: 2048)
  --streaming-frame-ms <ms>            controller frame target (default: 16.67)
  --max-upload-mib-per-frame <mib>      render-asset upload budget per frame; 0 is unlimited (default: 16)
  --paced-startup                       pace the first load like any other; the default runs it
                                        unpaced until the world is first ready
  --startup-cover                       hide the unpaced first load behind a black cover that fades out
                                        once the world is ready
  --io-threads <count>                  asset IO threads, 0 to 64; 0 sizes the pool automatically (default: 0)
  --auto-fly-speed <units/s>            fly the camera forward at this speed; 0 holds it still
  --terrain-radius <cells>              terrain-only ring around the streamed cells (default: 8, off
                                        where the worldspace has terrain LOD data; vanilla's view)
  --clusters [label|settings-id|path]   draw this cluster overlay of --assets (built by converter
                                        derive-clusters under <assets>/derived) as GPU-selected
                                        clusters in place of the stock models it owns (ADR-0013)
  --cluster-terrain                     draw terrain included in the cluster overlay
  --cluster-own-statics                 hide certified owned exterior primitives at spawn
                                        (requires --clusters; ownership experiment)
  --cluster-preload-batches             load every cluster batch during the startup load instead
                                        of as it comes into view (requires --clusters)
  --cluster-min-instance-pixels <px>    skip instances whose bounding sphere spans fewer pixels (default: 0, none)
  --no-cluster-prepass                  leave cluster batches out of the camera's depth/normal/motion prepass
  --route <file>                        follow a route file's waypoints at its speed (a JSON file; see scripts/routes)

Benchmark and profiling:
  --benchmark-only                      run the synthetic benchmark; opens no world database
  --benchmark-frames <count>            stop after this many measured frames
  --benchmark-duration <seconds>        stop after this many measured seconds (a route run: real seconds since start)
  --benchmark-warmup-frames <count>     frames discarded before measuring (default: 60)
  --benchmark-vsync                     use the interactive app's vsync during benchmarks
  --streaming-benchmark-route-speed <units/s>  start a real-time route after CPU settlement; 0 is stationary
  --streaming-benchmark-route-secs <seconds>   moving route duration (default: 45)
  --streaming-benchmark-tail-secs <seconds>    stationary tail duration (default: 10)
  --streaming-benchmark-settle-secs <seconds>  continuous CPU-settled wait before starting (default: 5)
  --streaming-benchmark-expected-models <count>       optional initial validated model count
  --streaming-benchmark-expected-lod-chunks <count>   optional initial resident LOD chunk count
  --streaming-benchmark-expected-terrain <count>      optional initial validated terrain patch count
  --streaming-benchmark-expected-water <count>        optional initial validated water surface count
  --benchmark-jump <x,y>                after the world first loads, jump to this grid cell and time the reload
  --benchmark-output <file>             benchmark report path (default: benchmark-report.json)
  --frame-budget-ms <ms>                frame time the report counts frames over (default: 6.25, 160 fps)
  --benchmark-frame-times [<file>]      write every measured frame time to this CSV file
  --run-label [<text>]                  name the run in the window title
  --run-owner [<text>]                  name who runs it (the thread) at the end of the window title
  --synthetic-instances <count>         instances in the synthetic benchmark (default: 250000)
  --accept-min-fps <fps>                fail the run below this average frame rate (default: 60)
  --accept-p95-ms <ms>                  fail the run above this p95 frame time (default: 16.67)
  --accept-max-memory-growth-gib <gib>  fail the run above this memory growth (default: 0.5)
  --acceptance-screenshot <file>        write a screenshot when the run ends
  --screenshot-camera-offset <x,y,z>    camera offset for the acceptance screenshot
  --matched-route <file>                execute exact route steps with transition and settled checkpoints
  --color-grade <mode>                 off|saturation|full|snapshot-srgb|snapshot-linear|record-srgb (default: snapshot-srgb)
  --grade-preset <json>                approved-look JSON override (snapshot-srgb and Cool Gray sky)
  --tone-map <tony|retail-curve>       display transform (default: tony); retail curve is experimental
  --tone-map-params <json>             explicit curve/scales and fixed luminance samples; requires selected IMGS
  --image-space <FormID>               load an authored IMGS record for record-srgb experiments
  --image-space-weather <FormID>       select an authored WTHR image-space slot, not live weather
  --image-space-slot <slot>            sunrise|day|sunset|night; requires --image-space-weather
  --visual-preset <vivid|cool-gray>      select sky and grading look (default: cool-gray)
  --sun-position <azimuth,elevation>     sun bearing clockwise from north and elevation above horizon, degrees
  --shots <file>                        render the camera poses in a shots file, one PNG each, then exit
  --shots-settle-timeout <seconds>      maximum settling time per shot (default: 30, maximum: 600)
  --shots-out <dir>                     where the shots' PNGs and shots.log go (default: beside the shots file)
  --cull-counter                        report submitted and drawn instances per render phase from the GPU
                                        culling result (renderer.json, cull); Bevy debug copies, off by default
  --no-main-occlusion                   leave OcclusionCulling off the main camera (to measure what it culls)
  --profile-output <dir>                profile bundle directory (default: no bundle)
  --profile-gpu-inventory               expensive GPU inventory diagnostic; requires --profile-output
  --profile-scenario <name>             scenario name recorded in the profile (default: adhoc)
  --profile-run-id <id>                 run id recorded in the profile (default: run-1)
  --profile-commit <hash>               commit recorded in the profile (default: unknown)
  --profile-dirty-worktree              record uncommitted worktree changes in the profile
  --profile-hardware <text>             hardware recorded in the profile (default: unspecified)
  --water-reflection-preset <name>      water reflection cost: baseline (full size, every frame, shadowed)
                                        or water (quarter size, every other frame, shadowless) (default: baseline);
                                        ignored with --water-reflection ssr
  --probe-retire-visibility <dir>       record whether a draining retiring cell ever shows, write retire-visibility.json and PNGs here, then exit

Diagnostics:
  --bug-report-dir <dir>                local bug-report directory (default: per-user application data)
  --diagnostic-asset-fallbacks          log every fallback asset substitution
  --water-reflection <planar|ssr|on|off>  measurement switch and Water Plus: ssr (default) marches the screen in the
                                        water shader, never renders the reflection view and copies the opaque colour
                                        once per view that draws water; planar (the same as on) renders the mirrored
                                        reflection view; off never renders the reflection view, and the water gets
                                        darker (a black reflection is blended in), not unreflective
  --water-ssr-steps <n>[,<m>]           ssr only: n linear (1-128) and m bisection (0-32) march steps per water
                                        pixel (default: 16,6); ignored with planar
  --cluster-far-terrain-shadows         optional canonical distant sun shadows (requires --cluster-terrain)
  --shadows <on|off>                    measurement switch: off turns off the sun's shadow maps (default: on)
  --material-fixture                    run the material fixture (no world database)
  --terrain-water-fixture               run the terrain-water fixture (no world database)
  --transform-bounds-fixture            run the transform-bounds fixture (no world database)
  --renderer-fixture                    run the renderer fixture (no world database)
  --physics-fixture                     run the physics fixture (no world database)
  --terrain-audit                       audit incremental terrain indices against full rebuilds once per second
  --streaming-fixture                   stream the fixture world instead of the full asset set

Remote:
  --remote                              serve the Bevy Remote Protocol on 127.0.0.1 (needs the remote feature)
  --remote-port <port>                  port for --remote (default: 15702); implies --remote

Portal (this branch's doors, demos and tools):
  --load-doors <load-screen|seamless>   how load doors cross: the vanilla load screen (default) or the seamless portal
  --walk                                walk as a first-person player; E opens load doors
  --show-window                         keep an automated run's window on screen
  --portal-depth-composite              composite the portal by depth (the default)
  --portal-depth-composite=on           the same as --portal-depth-composite
  --portal-depth-composite=off          draw the portal without depth compositing
  --demo <alftand|blackreach|riverwood> start at a named demo position
  --start-position <x> <y> <z>          start at a Creation-unit position
  --start-yaw <radians>                 start heading, with --start-position
  --demo-tour <dir>                     walk the scripted door tour, writing frames and a log here
  --tour-doors <count>                  walk only the first doors of the tour
  --tour-repeat <count>                 walk the tour's doors this many times
  --tour-dwell <seconds>                wait only this long after a crossing
  --tour-clip                           keep a clip's worth of frames around each crossing
  --tour-bench <file>                   time the tour and write the report here
  --portal-bench <file>                 time the portal at the doors in this file
  --bench-out <file>                    where --portal-bench writes its report
  --bench-seconds <seconds>             timing window per door for --portal-bench
  --captures-dir <dir>                  where pose and field-note captures go
  --field-notes-test                    field notes test mode
  --portal-light-spill                  spill light through the doorway the portal draws
  --dark-sun-shadows                    sun shadows in the dark spaces too
  --tonemapper <name>                   tonemapper to start with
  --graphics <preset>                   graphics preset
  --graphics-file <file>                graphics settings file
  --graphics-cycle <seconds>            step the anti-aliasing through every setting
  --render-parallel-finish              finish the render command encoders in parallel on the compute pool
                                        (the vendored bevy_render patch; same image and submission order) (default: on)
  --no-render-parallel-finish           finish them one by one on the render thread, as stock Bevy does
  --render-backends <all|vulkan>        graphics backends the renderer may use: all (Bevy's default) or vulkan
                                        only, which also drops the indirect-draw validation pass; with no Vulkan
                                        adapter the run logs an error and falls back to all (default: all)

Help:
  -h, --help                            print this message and exit

Example (the dummy-content fixture's worldspace is 1):
  engine --assets modern_assets --worldspace 1
";

/// What a parsed command line asks the process to do.
#[derive(Debug, Clone)]
pub enum ConfigAction {
    /// Run the engine. The configuration is boxed to keep this enum small.
    Run(Box<EngineConfig>),
    /// Print [`HELP_TEXT`] and exit successfully.
    Help,
}

/// A command line the engine cannot use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// An option the engine does not accept, with the known option it is
    /// closest to when one is within two edits.
    UnknownOption {
        argument: String,
        suggestion: Option<&'static str>,
    },
    /// A bare argument where the engine expects an option.
    UnexpectedArgument { argument: String },
    /// A value-taking option whose value is missing, is another option, or
    /// cannot be read.
    InvalidValue {
        option: &'static str,
        /// The value that was given, if the command line had one to give.
        value: Option<String>,
        /// What the option needs, phrased for the message.
        expected: &'static str,
    },
    /// A value one of the portal's or the reflection's flags refused, already phrased as a message.
    Message(String),
    /// An option that only works together with another option that was not given.
    MissingRequirement {
        option: &'static str,
        requires: &'static str,
    },
}

impl ConfigError {
    /// Classifies an argument the parser did not match, naming the nearest
    /// option when the argument looks like a mistyped one.
    fn unrecognized(argument: &str) -> Self {
        if argument.starts_with('-') {
            Self::UnknownOption {
                argument: argument.to_owned(),
                suggestion: nearest_option(argument),
            }
        } else {
            Self::UnexpectedArgument {
                argument: argument.to_owned(),
            }
        }
    }

    fn invalid_value(option: &'static str, value: Option<String>, expected: &'static str) -> Self {
        Self::InvalidValue {
            option,
            value,
            expected,
        }
    }
}

impl fmt::Display for ConfigError {
    /// Format a configuration error with the relevant option and usage guidance.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
            Self::UnknownOption {
                argument,
                suggestion: Some(suggestion),
            } => write!(
                formatter,
                "unknown option '{argument}'; did you mean '{suggestion}'? \
                 Run with --help to list every option."
            ),
            Self::UnknownOption {
                argument,
                suggestion: None,
            } => write!(
                formatter,
                "unknown option '{argument}'. Run with --help to list every option."
            ),
            Self::UnexpectedArgument { argument } => write!(
                formatter,
                "unexpected argument '{argument}'. Run with --help to list every option."
            ),
            Self::MissingRequirement { option, requires } => write!(
                formatter,
                "option '{option}' requires '{requires}'. Run with --help to list every option."
            ),
            Self::InvalidValue {
                option,
                value: None,
                expected,
            } => write!(
                formatter,
                "option '{option}' needs a value: expected {expected}. \
                 Run with --help to list every option."
            ),
            // A value shaped like an option is almost always the next option
            // with the value of this one forgotten.
            Self::InvalidValue {
                option,
                value: Some(value),
                expected,
            } if value.starts_with("--") => write!(
                formatter,
                "option '{option}' needs a value: expected {expected}, but '{value}' is another \
                 option. Run with --help to list every option."
            ),
            Self::InvalidValue {
                option,
                value: Some(value),
                expected,
            } => write!(
                formatter,
                "option '{option}' does not accept '{value}': expected {expected}. \
                 Run with --help to list every option."
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl EngineConfig {
    pub fn streaming_controls_enabled(&self) -> bool {
        self.adaptive_streaming || self.max_streaming_backlog != 0 || self.streaming_memory_mib != 0
    }

    pub fn streaming_backlog_limit(&self) -> usize {
        if self.max_streaming_backlog == 0 && self.adaptive_streaming {
            256
        } else {
            self.max_streaming_backlog
        }
    }

    /// A limit on estimated streaming allocations, not process RSS. Without an
    /// explicit adaptive limit this returns the 8 GiB fallback; the runtime
    /// selects half detected RAM within the 16 GiB and system-headroom limits.
    /// Available system memory is checked separately before reservations.
    pub fn memory_budget_bytes(&self) -> Option<u64> {
        let mib = if self.streaming_memory_mib != 0 {
            self.streaming_memory_mib
        } else if self.adaptive_streaming {
            8_192
        } else {
            return None;
        };
        Some(
            mib.checked_mul(1024 * 1024)
                .expect("streaming memory size must fit bytes") as u64,
        )
    }

    pub fn streaming_headroom_bytes(&self) -> u64 {
        self.streaming_headroom_mib
            .checked_mul(1024 * 1024)
            .expect("streaming headroom size must fit bytes") as u64
    }

    /// Select only explicitly authored data; this does not simulate effective weather.
    pub fn image_space_selection(&self) -> color_eyre::Result<Option<ImageSpaceSelection>> {
        use color_eyre::eyre::ensure;
        ensure!(
            self.image_space.is_none()
                || (self.image_space_weather.is_none() && self.image_space_slot.is_none()),
            "--image-space cannot be combined with --image-space-weather or --image-space-slot"
        );
        ensure!(
            self.image_space_weather.is_some() == self.image_space_slot.is_some(),
            "--image-space-weather and --image-space-slot must be supplied together"
        );
        let selection = self
            .image_space
            .map(ImageSpaceSelection::Direct)
            .or_else(|| {
                self.image_space_weather
                    .zip(self.image_space_slot)
                    .map(|(form_id, slot)| ImageSpaceSelection::Weather { form_id, slot })
            });
        ensure!(
            self.color_grade != SceneGradeMode::RecordSrgb || selection.is_some(),
            "record-srgb requires an explicit --image-space or --image-space-weather with --image-space-slot"
        );
        ensure!(
            self.tone_map != SceneToneMapMode::RetailCurve || selection.is_some(),
            "retail-curve requires an explicit --image-space or --image-space-weather with --image-space-slot"
        );
        Ok(selection)
    }

    /// The per-frame render-asset upload budget in bytes, or `None` when
    /// uploads are unlimited (`--max-upload-mib-per-frame 0`).
    pub fn max_upload_bytes_per_frame(&self) -> Option<usize> {
        (self.max_upload_mib_per_frame != 0)
            .then(|| self.max_upload_mib_per_frame.saturating_mul(1024 * 1024))
    }

    /// Residency envelope for reversible LAND stand-ins. Authored LOD inventory cannot prove
    /// drawable replacement coverage, so it never shrinks this envelope. The existing argument
    /// remains source-compatible; the LOD visibility index alone decides which surface draws.
    pub fn effective_terrain_radius(&self, _has_lod: bool) -> i32 {
        self.terrain_radius
    }

    /// Interactive exterior play owns the mouse/controller; automated camera paths keep their
    /// existing movement and framing. A `--benchmark-jump` run is one of those even without a frame
    /// limit: the jump moves only the camera, and in WALK the camera follow would put it straight
    /// back at the player's body.
    pub fn interactive_world_physics(&self) -> bool {
        !self.headless
            && !self.hidden_window
            && self.benchmark_jump.is_none()
            && !self.benchmark_only
            && self.benchmark_frames.is_none()
            && self.benchmark_duration_secs.is_none()
            && self.acceptance_screenshot.is_none()
            && self.shots.is_none()
            && self.matched_route.is_none()
            && self.auto_fly_speed <= 0.0
            && self.route.is_none()
            && self.streaming_benchmark_route_speed.is_none()
            && !self.material_fixture
            && !self.terrain_water_fixture
            && !self.transform_bounds_fixture
            && !self.renderer_fixture
            && !self.streaming_fixture
            && !self.physics_fixture
    }

    /// The unscripted load-screen capsule controller, in WALK or NOCLIP.
    pub(crate) fn has_capsule_player(&self) -> bool {
        !self.seamless_doors()
            && self.interactive_world_physics()
            && self.portal.demo_tour.is_none()
            && self.portal.start_shot.is_none()
    }

    pub fn from_env() -> Result<ConfigAction, ConfigError> {
        Self::from_args(std::env::args().skip(1))
    }

    /// Whether this is a benchmark run: `--benchmark-frames` or `--benchmark-duration` was given.
    /// The same test the report uses to decide whether to write itself (`metrics::collect_and_finish`),
    /// so anything that only makes sense for a benchmark can key off it.
    pub fn is_benchmark_run(&self) -> bool {
        self.benchmark_frames.is_some()
            || self.benchmark_duration_secs.is_some()
            || self.streaming_benchmark_route_speed.is_some()
    }

    /// Whether the pacing instrumentation should run at all. It is measurement only, and only a
    /// benchmark report or the profiling campaign (itself a benchmark run) reads it, so an ordinary
    /// play session installs none of it. A jump without a frame limit or duration still installs it:
    /// the jump is documented to run without a report and warn that it has none.
    pub fn measures_pacing(&self) -> bool {
        self.is_benchmark_run() || self.benchmark_jump.is_some()
    }

    /// Whether the first load runs with the streaming pacing lifted
    /// ([`crate::pacing::startup::StartupLoadPlugin`]): the default; `--paced-startup` runs the old
    /// way. A capture run is lifted too - the spawn cap binds on its startup like any run's (the
    /// `portal-heavy` window: 342 frames to ready).
    pub fn unpaced_startup(&self) -> bool {
        !self.paced_startup
    }

    /// Whether the unpaced first load is also hidden behind the black cover: only with
    /// `--startup-cover`, and never in a capture run (`--shots`, `--demo-tour`), whose own settle
    /// gate already holds it past the load and whose photographs a cover still up would black out.
    pub fn covers_startup_load(&self) -> bool {
        self.unpaced_startup()
            && self.startup_cover
            && self.portal.shots.is_none()
            && self.portal.demo_tour.is_none()
    }

    /// Whether the run moves the camera on its own: `--auto-fly-speed` or `--route`.
    pub fn flies(&self) -> bool {
        self.auto_fly_speed > 0.0 || self.route.is_some()
    }

    /// Whether the readiness scan records how far behind the flying camera each model finishes
    /// loading. Only the pacing report reads it, so a plain `--auto-fly-speed` run records nothing.
    pub fn records_fly_lag(&self) -> bool {
        self.flies() && self.measures_pacing()
    }

    /// Parses a command line into a [`ConfigAction`], or names the argument it
    /// does not recognise.
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<ConfigAction, ConfigError> {
        let args: Vec<String> = args.into_iter().collect();
        // Help wins wherever it appears, so `--typo --help` prints the options
        // rather than failing on the earlier argument.
        if args
            .iter()
            .any(|argument| argument == "-h" || argument == "--help")
        {
            return Ok(ConfigAction::Help);
        }
        let mut config = Self::default();
        let mut ini = SkyrimIni::default();
        for pair in args.windows(2) {
            if pair[0] == "--ini" {
                let path = PathBuf::from(take_raw(
                    "--ini",
                    "a Skyrim INI file path",
                    Some(pair[1].clone()),
                )?);
                if let Err(error) = ini.merge_file(&path) {
                    eprintln!("warning: ignoring --ini {}: {error}", path.display());
                }
            }
        }
        ini.apply(&mut config);
        config.settings_overrides.extend(
            args.iter()
                .filter(|argument| argument.starts_with("--"))
                .cloned(),
        );
        let mut args = args.into_iter().peekable();

        while let Some(argument) = args.next() {
            // The water reflection's cost flags ([`ReflectionOptions`]); a value that is not one is
            // refused rather than left at its default.
            match ReflectionOptions::parse_flag(&mut config.reflection, &argument, &mut args) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => return Err(ConfigError::Message(error)),
            }
            // The portal's own flags, parsed by one block at the end of this file.
            if PortalOptions::parse_flag(&mut config, &argument, &mut args) {
                continue;
            }
            match argument.as_str() {
                // The pre-scan above answers both spellings; these arms keep the
                // option list the help-drift tests read complete.
                "-h" => return Ok(ConfigAction::Help),
                "--help" => return Ok(ConfigAction::Help),
                // A bare `--` is the conventional end-of-options marker. The
                // engine takes no positional arguments, so it has nothing to
                // end, and main ignored it: keep ignoring it.
                "--" => {}
                "--ini" => {
                    take_raw("--ini", "a Skyrim INI file path", args.next())?;
                }
                "--assets" => {
                    config.assets_dir = take_value(
                        "--assets",
                        "a directory holding converted assets",
                        args.next(),
                    )?;
                }
                "--bug-report-dir" => {
                    let path = take_raw(
                        "--bug-report-dir",
                        "a nonempty local report directory path",
                        args.next(),
                    )?;
                    if path.trim().is_empty() {
                        return Err(ConfigError::invalid_value(
                            "--bug-report-dir",
                            Some(path),
                            "a nonempty local report directory path",
                        ));
                    }
                    config.bug_report_dir = Some(PathBuf::from(path));
                }
                "--worldspace" => {
                    let value = take_raw(
                        "--worldspace",
                        "a worldspace form id, decimal or 0x-hex",
                        args.next(),
                    )?;
                    config.worldspace_id = parse_u32(&value).ok_or_else(|| {
                        ConfigError::invalid_value(
                            "--worldspace",
                            Some(value),
                            "a worldspace form id, decimal or 0x-hex",
                        )
                    })?;
                }
                "--grid-x" => {
                    config.start_grid.0 = take_value("--grid-x", "a cell coordinate", args.next())?;
                }
                "--grid-y" => {
                    config.start_grid.1 = take_value("--grid-y", "a cell coordinate", args.next())?;
                }
                "--stream-radius" => {
                    let raw = take_raw("--stream-radius", STREAM_RADIUS_EXPECTED, args.next())?;
                    let value = raw
                        .parse::<i32>()
                        .ok()
                        .filter(|value| (0..=MAX_STREAM_RADIUS).contains(value))
                        .ok_or_else(|| {
                            ConfigError::invalid_value(
                                "--stream-radius",
                                Some(raw),
                                STREAM_RADIUS_EXPECTED,
                            )
                        })?;
                    config.stream_radius = value;
                    config.unload_radius = value + 1;
                }
                "--terrain-radius" => {
                    if let Some(value) = args.next().and_then(|value| value.parse::<i32>().ok()) {
                        config.terrain_radius = value.clamp(0, MAX_TERRAIN_RADIUS);
                        // A value the run named - even one the default would have chosen - is
                        // drawn as given in every worldspace (`effective_terrain_radius`).
                        config.terrain_radius_given = true;
                    }
                }
                "--cluster-terrain" => config.cluster_terrain = true,
                "--cluster-own-statics" => config.cluster_own_statics = true,
                "--cluster-preload-batches" => config.cluster_preload_batches = true,
                "--cluster-far-terrain-shadows" => config.cluster_far_terrain_shadows = true,
                "--clusters" => {
                    // The selector is optional: bare `--clusters` means the only overlay.
                    let named = args.peek().is_some_and(|next| !next.starts_with("--"));
                    let selector = if named {
                        args.next().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    config.clusters = Some(selector);
                }
                "--no-cluster-prepass" => config.cluster_prepass = false,
                "--cluster-min-instance-pixels" => {
                    let raw = take_raw(
                        "--cluster-min-instance-pixels",
                        "a pixel count, 0 for every instance",
                        args.next(),
                    )?;
                    config.cluster_min_instance_pixels = Some(
                        raw.parse::<f32>()
                            .ok()
                            .filter(|pixels| pixels.is_finite() && *pixels >= 0.0)
                            .ok_or_else(|| {
                                ConfigError::invalid_value(
                                    "--cluster-min-instance-pixels",
                                    Some(raw),
                                    "a pixel count, 0 for every instance",
                                )
                            })?,
                    );
                }
                "--headless" => config.headless = true,
                "--hidden-window" => config.hidden_window = true,
                "--max-unloads-per-frame" => {
                    config.max_cell_unloads_per_frame = take_value(
                        "--max-unloads-per-frame",
                        "a cell count, 0 for unlimited",
                        args.next(),
                    )?;
                }
                "--max-unload-ms-per-frame" => {
                    config.max_unload_ms_per_frame = take_number(
                        "--max-unload-ms-per-frame",
                        "a number of milliseconds, 0 for the old count budget",
                        Bound::NonNegative,
                        args.next(),
                    )?;
                }
                "--lod-keep-hidden-in-interiors" => config.lod_keep_hidden_in_interiors = true,
                "--max-lod-requests-per-frame" => {
                    config.max_lod_requests_per_frame = take_value(
                        "--max-lod-requests-per-frame",
                        "a request count, 0 for unlimited",
                        args.next(),
                    )?;
                }
                "--max-model-spawns-per-frame" => {
                    config.max_model_spawns_per_frame = take_value(
                        "--max-model-spawns-per-frame",
                        "a model count, 0 for unlimited",
                        args.next(),
                    )?;
                }
                "--prioritize-streaming" => config.prioritize_streaming = true,
                "--max-scene-loads" => {
                    config.max_scene_loads = take_value(
                        "--max-scene-loads",
                        "a scene job count, 0 for unlimited",
                        args.next(),
                    )?;
                }
                "--adaptive-streaming" => {
                    config.adaptive_streaming = true;
                    config.prioritize_streaming = true;
                }
                "--max-streaming-backlog" => {
                    config.max_streaming_backlog = take_value(
                        "--max-streaming-backlog",
                        "a downstream queue count, 0 to disable",
                        args.next(),
                    )?;
                }
                "--streaming-memory-mib" => {
                    config.streaming_memory_mib = take_mib(
                        "--streaming-memory-mib",
                        "a MiB size that fits bytes, 0 to disable",
                        args.next(),
                    )?;
                }
                "--streaming-costs" => {
                    config.streaming_costs = Some(take_value(
                        "--streaming-costs",
                        "a file path for streaming cost metadata",
                        args.next(),
                    )?);
                }
                "--streaming-headroom-mib" => {
                    config.streaming_headroom_mib = take_mib(
                        "--streaming-headroom-mib",
                        "a MiB size that fits bytes, 0 to reserve no headroom",
                        args.next(),
                    )?;
                }
                "--streaming-frame-ms" => {
                    config.streaming_frame_ms = take_number(
                        "--streaming-frame-ms",
                        "a positive number of milliseconds",
                        Bound::Positive,
                        args.next(),
                    )?;
                }
                // 0 keeps the automatic size; a count past the cap is refused like a value that
                // fails to parse.
                "--io-threads" => {
                    let raw = take_raw("--io-threads", IO_THREADS_EXPECTED, args.next())?;
                    config.io_threads = raw
                        .parse::<usize>()
                        .ok()
                        .filter(|value| *value <= MAX_IO_THREADS)
                        .ok_or_else(|| {
                            ConfigError::invalid_value(
                                "--io-threads",
                                Some(raw),
                                IO_THREADS_EXPECTED,
                            )
                        })?;
                }
                "--max-upload-mib-per-frame" => {
                    config.max_upload_mib_per_frame = take_value(
                        "--max-upload-mib-per-frame",
                        "a size in MiB, 0 for unlimited",
                        args.next(),
                    )?;
                }
                "--paced-startup" => config.paced_startup = true,
                "--startup-cover" => config.startup_cover = true,
                "--max-commit-ms" => {
                    let millis: f64 = take_number(
                        "--max-commit-ms",
                        "a positive number of milliseconds",
                        Bound::Positive,
                        args.next(),
                    )?;
                    config.max_commit_micros_per_frame =
                        (millis * 1_000.0).round().clamp(1.0, u64::MAX as f64) as u64;
                }
                "--benchmark-only" => config.benchmark_only = true,
                "--cull-counter" => config.cull_counter = true,
                "--no-main-occlusion" => config.main_occlusion = false,
                "--benchmark-frames" => {
                    config.benchmark_frames = Some(take_value(
                        "--benchmark-frames",
                        "a frame count",
                        args.next(),
                    )?);
                }
                "--benchmark-duration" => {
                    config.benchmark_duration_secs = Some(take_number(
                        "--benchmark-duration",
                        "a positive number of seconds",
                        Bound::Positive,
                        args.next(),
                    )?);
                }
                "--benchmark-warmup-frames" => {
                    config.benchmark_warmup_frames =
                        take_value("--benchmark-warmup-frames", "a frame count", args.next())?;
                }
                "--benchmark-vsync" => config.benchmark_vsync = true,
                "--streaming-benchmark-route-speed" => {
                    config.streaming_benchmark_route_speed = Some(take_number(
                        "--streaming-benchmark-route-speed",
                        "a finite speed in units per second, 0 or more",
                        Bound::NonNegative,
                        args.next(),
                    )?);
                }
                "--streaming-benchmark-route-secs" => {
                    config.streaming_benchmark_route_secs = take_number(
                        "--streaming-benchmark-route-secs",
                        "a positive finite number of seconds",
                        Bound::Positive,
                        args.next(),
                    )?;
                }
                "--streaming-benchmark-tail-secs" => {
                    config.streaming_benchmark_tail_secs = take_number(
                        "--streaming-benchmark-tail-secs",
                        "a finite number of seconds, 0 or more",
                        Bound::NonNegative,
                        args.next(),
                    )?;
                }
                "--streaming-benchmark-settle-secs" => {
                    config.streaming_benchmark_settle_secs = take_number(
                        "--streaming-benchmark-settle-secs",
                        "a finite number of seconds, 0 or more",
                        Bound::NonNegative,
                        args.next(),
                    )?;
                }
                "--streaming-benchmark-expected-models" => {
                    config.streaming_benchmark_expected_models = Some(take_value(
                        "--streaming-benchmark-expected-models",
                        "a model count",
                        args.next(),
                    )?);
                }
                "--streaming-benchmark-expected-lod-chunks" => {
                    config.streaming_benchmark_expected_lod_chunks = Some(take_value(
                        "--streaming-benchmark-expected-lod-chunks",
                        "a LOD chunk count",
                        args.next(),
                    )?);
                }
                "--streaming-benchmark-expected-terrain" => {
                    config.streaming_benchmark_expected_terrain = Some(take_value(
                        "--streaming-benchmark-expected-terrain",
                        "a terrain patch count",
                        args.next(),
                    )?);
                }
                "--streaming-benchmark-expected-water" => {
                    config.streaming_benchmark_expected_water = Some(take_value(
                        "--streaming-benchmark-expected-water",
                        "a water surface count",
                        args.next(),
                    )?);
                }
                // Unlike its neighbours, a bad jump only warns: the jump is an add-on to a
                // benchmark run, which still measures and writes its report without it.
                "--benchmark-jump" => match args.next_if(|value| !value.starts_with("--")) {
                    Some(raw) => match parse_grid(&raw) {
                        Some(value) if in_jump_grid_range(value) => {
                            config.benchmark_jump = Some(value)
                        }
                        Some((x, y)) => eprintln!(
                            "warning: ignoring --benchmark-jump {x},{y}: grid coordinates must be within {}..={JUMP_GRID_LIMIT}",
                            -JUMP_GRID_LIMIT
                        ),
                        None => eprintln!(
                            "warning: ignoring malformed --benchmark-jump {raw:?}; expected \"<grid-x>,<grid-y>\" integers"
                        ),
                    },
                    None => eprintln!(
                        "warning: missing value for --benchmark-jump; expected \"<grid-x>,<grid-y>\" integers"
                    ),
                },
                "--benchmark-output" => {
                    config.benchmark_output = take_value(
                        "--benchmark-output",
                        "a file path for the benchmark report",
                        args.next(),
                    )?;
                }
                // The value of these two may be left out, and a value left out must not swallow
                // the next option.
                "--run-label" => {
                    config.run_label = args.next_if(|value| !value.starts_with("--"));
                }
                "--run-owner" => {
                    config.run_owner = args.next_if(|value| !value.starts_with("--"));
                }
                "--benchmark-frame-times" => {
                    if let Some(value) = args.next_if(|value| !value.starts_with("--")) {
                        config.benchmark_frame_times = Some(value.into());
                    }
                }
                "--accept-min-fps" => {
                    config.accept_min_fps = take_number(
                        "--accept-min-fps",
                        "a frame rate, 0 or more",
                        Bound::NonNegative,
                        args.next(),
                    )?;
                }
                "--accept-p95-ms" => {
                    config.accept_p95_ms = take_number(
                        "--accept-p95-ms",
                        "a frame time in milliseconds, 0 or more",
                        Bound::NonNegative,
                        args.next(),
                    )?;
                }
                "--accept-max-memory-growth-gib" => {
                    config.accept_max_memory_growth_gib = take_number(
                        "--accept-max-memory-growth-gib",
                        "a memory size in GiB, 0 or more",
                        Bound::NonNegative,
                        args.next(),
                    )?;
                }
                "--auto-fly-speed" => {
                    config.auto_fly_speed = take_number(
                        "--auto-fly-speed",
                        "a speed in units per second, 0 or more",
                        Bound::NonNegative,
                        args.next(),
                    )?;
                }
                "--route" => {
                    config.route = Some(take_value("--route", "a route file path", args.next())?);
                }
                "--frame-budget-ms" => {
                    config.frame_budget_ms = take_number(
                        "--frame-budget-ms",
                        "a positive number of milliseconds",
                        Bound::Positive,
                        args.next(),
                    )?;
                }
                "--allow-incomplete-assets" => config.allow_incomplete_assets = true,
                "--synthetic-instances" => {
                    config.synthetic_instances =
                        take_value("--synthetic-instances", "an instance count", args.next())?;
                }
                "--profile-output" => {
                    config.profile_output_dir = Some(take_value(
                        "--profile-output",
                        "a directory path for the profile bundle",
                        args.next(),
                    )?);
                }
                "--profile-scenario" => {
                    config.profile_scenario =
                        take_value("--profile-scenario", "a scenario name", args.next())?;
                }
                "--profile-run-id" => {
                    config.profile_run_id =
                        take_value("--profile-run-id", "a run id", args.next())?;
                }
                "--profile-commit" => {
                    config.profile_commit =
                        take_value("--profile-commit", "a commit hash", args.next())?;
                }
                "--probe-retire-visibility" => {
                    config.probe_retire_visibility = Some(take_value(
                        "--probe-retire-visibility",
                        "a directory for the retire-visibility probe's JSON and PNGs",
                        args.next(),
                    )?);
                }
                "--profile-dirty-worktree" => config.profile_dirty_worktree = true,
                "--profile-gpu-inventory" => config.profile_gpu_inventory = true,
                "--profile-hardware" => {
                    config.profile_hardware = take_value(
                        "--profile-hardware",
                        "a description of the hardware",
                        args.next(),
                    )?;
                }
                "--acceptance-screenshot" => {
                    config.acceptance_screenshot = Some(take_value(
                        "--acceptance-screenshot",
                        "a file path for the screenshot",
                        args.next(),
                    )?);
                }
                "--screenshot-camera-offset" => {
                    const EXPECTED: &str = "three finite numbers \"x,y,z\"";
                    let raw = take_raw("--screenshot-camera-offset", EXPECTED, args.next())?;
                    config.screenshot_camera_offset =
                        Some(parse_offset(&raw).ok_or_else(|| {
                            ConfigError::invalid_value(
                                "--screenshot-camera-offset",
                                Some(raw),
                                EXPECTED,
                            )
                        })?);
                }
                // A path left out is an error, and must not swallow the next option: a `--shots`
                // with no path is no mode at all, and continuing would silently launch an ordinary
                // interactive run.
                "--matched-route" => {
                    config.matched_route = Some(take_value(
                        "--matched-route",
                        "a route fixture",
                        args.next(),
                    )?);
                }
                "--color-grade" => {
                    let raw = take_raw("--color-grade", SCENE_GRADE_VALUES, args.next())?;
                    config.color_grade = SceneGradeMode::parse(&raw).ok_or_else(|| {
                        ConfigError::invalid_value("--color-grade", Some(raw), SCENE_GRADE_VALUES)
                    })?;
                }
                "--grade-preset" => {
                    config.grade_preset =
                        Some(take_value("--grade-preset", "a JSON file", args.next())?);
                }
                "--tone-map" => {
                    let raw = take_raw("--tone-map", "tony or retail-curve", args.next())?;
                    config.tone_map = SceneToneMapMode::parse(&raw).ok_or_else(|| {
                        ConfigError::invalid_value("--tone-map", Some(raw), "tony or retail-curve")
                    })?;
                }
                "--tone-map-params" => {
                    config.tone_map_params =
                        Some(take_value("--tone-map-params", "a JSON file", args.next())?);
                }
                "--image-space" => {
                    let raw =
                        take_raw("--image-space", "a FormID, decimal or 0x-hex", args.next())?;
                    config.image_space = Some(parse_u32(&raw).ok_or_else(|| {
                        ConfigError::invalid_value(
                            "--image-space",
                            Some(raw),
                            "a FormID, decimal or 0x-hex",
                        )
                    })?);
                }
                "--image-space-weather" => {
                    let raw = take_raw(
                        "--image-space-weather",
                        "a FormID, decimal or 0x-hex",
                        args.next(),
                    )?;
                    config.image_space_weather = Some(parse_u32(&raw).ok_or_else(|| {
                        ConfigError::invalid_value(
                            "--image-space-weather",
                            Some(raw),
                            "a FormID, decimal or 0x-hex",
                        )
                    })?);
                }
                "--image-space-slot" => {
                    let raw = take_raw(
                        "--image-space-slot",
                        "sunrise, day, sunset or night",
                        args.next(),
                    )?;
                    config.image_space_slot =
                        Some(ImageSpaceSlot::parse(&raw).ok_or_else(|| {
                            ConfigError::invalid_value(
                                "--image-space-slot",
                                Some(raw),
                                "sunrise, day, sunset or night",
                            )
                        })?);
                }
                "--visual-preset" => {
                    let raw = take_raw("--visual-preset", "vivid or cool-gray", args.next())?;
                    config.visual_preset = SceneVisualPreset::parse(&raw).ok_or_else(|| {
                        ConfigError::invalid_value(
                            "--visual-preset",
                            Some(raw),
                            "vivid or cool-gray",
                        )
                    })?;
                }
                "--sun-position" => {
                    let raw = take_raw("--sun-position", SUN_POSITION_EXPECTED, args.next())?;
                    config.sun_position = Some(parse_sun_position(&raw).ok_or_else(|| {
                        ConfigError::invalid_value(
                            "--sun-position",
                            Some(raw),
                            SUN_POSITION_EXPECTED,
                        )
                    })?);
                }
                "--shots" => {
                    config.shots = Some(take_value("--shots", "a shots file path", args.next())?);
                }
                "--start-position" => {
                    let mut position = [0.0f32; 3];
                    for slot in &mut position {
                        let raw =
                            take_raw("--start-position", "three finite coordinates", args.next())?;
                        *slot = raw
                            .parse::<f32>()
                            .ok()
                            .filter(|v| v.is_finite())
                            .ok_or_else(|| {
                                ConfigError::invalid_value(
                                    "--start-position",
                                    Some(raw),
                                    "three finite coordinates",
                                )
                            })?;
                    }
                    config.start_position = Some(position);
                    config.start_grid = (
                        (position[0] / CELL_SIZE).floor() as i32,
                        (position[1] / CELL_SIZE).floor() as i32,
                    );
                }
                "--start-yaw" => {
                    let raw = take_raw("--start-yaw", "a finite heading in radians", args.next())?;
                    config.start_yaw = raw
                        .parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| {
                            ConfigError::invalid_value(
                                "--start-yaw",
                                Some(raw),
                                "a finite heading in radians",
                            )
                        })?;
                }
                "--start-shot" => {
                    let file: PathBuf =
                        take_value("--start-shot", "a shots file path", args.next())?;
                    // The shared flag retains main's single-pose form and the demo's named pose.
                    // Peek before taking a name so a following option remains an option.
                    if let Some(name) = args.next_if(|value| !value.starts_with('-')) {
                        config.start_shot = None;
                        config.portal.start_shot = Some(StartShot {
                            file: Some(file),
                            name: Some(name),
                        });
                    } else {
                        config.portal.start_shot = None;
                        config.start_shot = Some(file);
                    }
                }
                "--stdin-console" => config.stdin_console = true,
                "--no-render-diagnostics" => config.no_render_diagnostics = true,
                "--shots-settle-timeout" => {
                    const EXPECTED: &str = "a positive number of seconds up to 600";
                    let raw = take_raw("--shots-settle-timeout", EXPECTED, args.next())?;
                    config.shots_settle_timeout = raw
                        .parse::<f32>()
                        .ok()
                        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 600.0)
                        .ok_or_else(|| {
                            ConfigError::invalid_value(
                                "--shots-settle-timeout",
                                Some(raw),
                                EXPECTED,
                            )
                        })?;
                }
                "--shots-out" => {
                    config.shots_out = Some(take_value(
                        "--shots-out",
                        "a directory for the shots' images and log",
                        args.next(),
                    )?);
                }
                "--diagnostic-asset-fallbacks" => config.diagnostic_asset_fallbacks = true,
                "--material-fixture" => config.material_fixture = true,
                "--terrain-water-fixture" => config.terrain_water_fixture = true,
                "--transform-bounds-fixture" => config.transform_bounds_fixture = true,
                "--renderer-fixture" => config.renderer_fixture = true,
                "--terrain-audit" => config.terrain_audit = true,
                "--streaming-fixture" => config.streaming_fixture = true,
                "--remote" => config.remote = true,
                "--remote-port" => {
                    config.remote = true;
                    if let Some(value) = args.next().and_then(|value| value.parse().ok()) {
                        config.remote_port = value;
                    }
                }
                "--lights" => config.lights = true,
                "--as-played" => config.as_played = true,
                "--room-culling" => config.room_culling = true,
                "--physics-fixture" => config.physics_fixture = true,
                "--water-reflection" => {
                    const EXPECTED: &str = "on, off, planar or ssr";
                    let raw = take_raw("--water-reflection", EXPECTED, args.next())?;
                    // Not a `match` on string literals: the help-drift tests read every
                    // `"..." =>` line of this file as an option arm. `on` is `planar` under its
                    // older name, so the measurement switch keeps working as it was written.
                    if raw == "on" || raw == "planar" {
                        config.water_reflection = true;
                        config.water_reflection_mode = WaterReflectionMode::Planar;
                    } else if raw == "off" {
                        config.water_reflection = false;
                        config.water_reflection_mode = WaterReflectionMode::Planar;
                    } else if raw == "ssr" {
                        config.water_reflection = true;
                        config.water_reflection_mode = WaterReflectionMode::Ssr;
                    } else {
                        return Err(ConfigError::invalid_value(
                            "--water-reflection",
                            Some(raw),
                            EXPECTED,
                        ));
                    }
                }
                "--water-ssr-steps" => {
                    const EXPECTED: &str =
                        "<linear>[,<bisection>], 1 to 128 linear and 0 to 32 bisection steps";
                    let raw = take_raw("--water-ssr-steps", EXPECTED, args.next())?;
                    config.water_ssr_steps = WaterSsrSteps::parse(&raw).ok_or_else(|| {
                        ConfigError::invalid_value("--water-ssr-steps", Some(raw), EXPECTED)
                    })?;
                }
                "--shadows" => config.shadows = take_on_off("--shadows", args.next())?,
                "--water-reflection-preset" => {
                    const EXPECTED: &str = "baseline or water";
                    let raw = take_raw("--water-reflection-preset", EXPECTED, args.next())?;
                    config.water_reflection_preset = WaterReflectionPreset::ALL
                        .into_iter()
                        .find(|preset| preset.name() == raw)
                        .ok_or_else(|| {
                            ConfigError::invalid_value(
                                "--water-reflection-preset",
                                Some(raw),
                                EXPECTED,
                            )
                        })?;
                }
                // A value that is not one stops the run rather than leaving the vanilla crossing
                // in place of the one asked for.
                "--load-doors" => {
                    let raw = take_raw("--load-doors", LoadDoors::EXPECTED, args.next())?;
                    config.portal.load_doors = LoadDoors::parse(&raw).ok_or_else(|| {
                        ConfigError::invalid_value("--load-doors", Some(raw), LoadDoors::EXPECTED)
                    })?;
                }
                "--render-parallel-finish" => config.render_parallel_finish = true,
                "--no-render-parallel-finish" => config.render_parallel_finish = false,
                "--render-backends" => {
                    const EXPECTED: &str = "all or vulkan";
                    let raw = take_raw("--render-backends", EXPECTED, args.next())?;
                    config.render_backends = RenderBackends::ALL
                        .into_iter()
                        .find(|backends| backends.name() == raw)
                        .ok_or_else(|| {
                            ConfigError::invalid_value("--render-backends", Some(raw), EXPECTED)
                        })?;
                }
                unknown => return Err(ConfigError::unrecognized(unknown)),
            }
        }
        // Benchmark and acceptance runs measure the full-detail grid the Phase 2 gates were set
        // for, so the terrain ring is off there unless a run asks for it by name.
        // `--shots` and `--shots-out` are main's options (parsed above, with main's checks); the
        // portal's shots run reads them from its own options.
        config.portal.shots = config.shots.clone();
        config.portal.shots_out = config.shots_out.clone();
        if !config.terrain_radius_given
            && (config.benchmark_frames.is_some() || config.benchmark_duration_secs.is_some())
        {
            config.terrain_radius = config.stream_radius;
        }
        if config.streaming_benchmark_route_speed.is_some()
            && (config.benchmark_only
                || config.acceptance_screenshot.is_some()
                || config.shots.is_some()
                || config.matched_route.is_some()
                || config.route.is_some()
                || config.auto_fly_speed > 0.0
                || config.material_fixture
                || config.terrain_water_fixture
                || config.transform_bounds_fixture
                || config.renderer_fixture
                || config.streaming_fixture
                || config.physics_fixture)
        {
            return Err(ConfigError::invalid_value(
                "--streaming-benchmark-route-speed",
                config
                    .streaming_benchmark_route_speed
                    .map(|speed| speed.to_string()),
                "a world camera route without screenshots, shots, a route file, synthetic mode or fixtures",
            ));
        }
        // A route waits for CPU settlement with no other bound, so it needs the real-time deadline.
        if config.streaming_benchmark_route_speed.is_some()
            && config.benchmark_duration_secs.is_none()
        {
            return Err(ConfigError::MissingRequirement {
                option: "--streaming-benchmark-route-speed",
                requires: "--benchmark-duration",
            });
        }
        if let Some(speed) = config.streaming_benchmark_route_speed
            && (!(speed * config.streaming_benchmark_route_secs).is_finite()
                || !(config.streaming_benchmark_route_secs + config.streaming_benchmark_tail_secs)
                    .is_finite())
        {
            return Err(ConfigError::invalid_value(
                "--streaming-benchmark-route-secs",
                Some(config.streaming_benchmark_route_secs.to_string()),
                "a duration whose route distance and total duration remain finite",
            ));
        }
        // The inventory is written into the profile bundle; without one it would do nothing.
        if config.profile_gpu_inventory && config.profile_output_dir.is_none() {
            return Err(ConfigError::MissingRequirement {
                option: "--profile-gpu-inventory",
                requires: "--profile-output",
            });
        }
        config.drop_jump_the_run_would_overwrite();
        if config.cluster_terrain && !config.cluster_prepass {
            return Err(ConfigError::invalid_value(
                "--cluster-terrain",
                None,
                "the cluster prepass enabled",
            ));
        }
        if config.cluster_own_statics && config.clusters.is_none() {
            return Err(ConfigError::invalid_value(
                "--cluster-own-statics",
                None,
                "--clusters",
            ));
        }
        if config.cluster_preload_batches && config.clusters.is_none() {
            return Err(ConfigError::invalid_value(
                "--cluster-preload-batches",
                None,
                "--clusters",
            ));
        }
        if config.cluster_terrain && config.clusters.is_none() {
            return Err(ConfigError::invalid_value(
                "--cluster-terrain",
                None,
                "--clusters naming an overlay with terrain",
            ));
        }
        if config.cluster_far_terrain_shadows && !config.cluster_terrain {
            return Err(ConfigError::invalid_value(
                "--cluster-far-terrain-shadows",
                None,
                "--cluster-terrain",
            ));
        }
        config.check_seamless_only_flags()?;
        Ok(ConfigAction::Run(Box::new(config)))
    }

    /// The reason text for a fixture run that builds no streaming runtime (see `app.rs`), or
    /// `None` when none of those fixtures is selected.
    fn non_streaming_fixture(&self) -> Option<&'static str> {
        if self.material_fixture {
            Some("--material-fixture creates no streaming runtime to jump in")
        } else if self.terrain_water_fixture {
            Some("--terrain-water-fixture creates no streaming runtime to jump in")
        } else if self.transform_bounds_fixture {
            Some("--transform-bounds-fixture creates no streaming runtime to jump in")
        } else if self.renderer_fixture {
            Some("--renderer-fixture creates no streaming runtime to jump in")
        } else if self.physics_fixture {
            Some("--physics-fixture creates no streaming runtime to jump in")
        } else {
            None
        }
    }

    /// `--benchmark-jump` moves the camera once, so a run that keeps driving it would undo the
    /// jump: auto-fly keeps moving it, the streaming fixture moves it every frame, a shots run poses
    /// it for each shot, and a screenshot run anchors streaming on the start cell. A synthetic
    /// `--benchmark-only` run loads no world, so nothing would carry the jump out.
    fn drop_jump_the_run_would_overwrite(&mut self) {
        let reason = if self.benchmark_only {
            Some("--benchmark-only loads no world to jump in")
        } else if let Some(fixture) = self.non_streaming_fixture() {
            Some(fixture)
        } else if self.auto_fly_speed > 0.0 {
            Some("--auto-fly-speed keeps driving the camera")
        } else if self.route.is_some() {
            Some("--route keeps driving the camera")
        } else if self.acceptance_screenshot.is_some() {
            Some("--acceptance-screenshot anchors streaming on the start cell")
        } else if self.streaming_fixture {
            Some("--streaming-fixture moves the camera every frame")
        } else if self.shots.is_some() {
            Some("--shots sets the camera pose for each shot")
        } else if self.streaming_benchmark_route_speed.is_some() {
            Some("--streaming-benchmark-route-speed owns the camera")
        } else if self.matched_route.is_some() {
            Some("--matched-route sets the camera pose for each route step")
        } else {
            None
        };
        let Some(reason) = reason else {
            // The jump still runs, but with no frame limit or duration no benchmark report (which
            // carries the pacing fields) is written, so there is nothing to read the jump out of.
            if let Some((x, y)) = self.benchmark_jump
                && !self.is_benchmark_run()
            {
                eprintln!(
                    "warning: --benchmark-jump {x},{y} has no report to appear in without --benchmark-frames or --benchmark-duration"
                );
            }
            return;
        };
        if let Some((x, y)) = self.benchmark_jump.take() {
            eprintln!("warning: ignoring --benchmark-jump {x},{y}: {reason}");
        }
    }
}

/// The widest `--stream-radius` the engine takes: the unload ring is one cell wider, and the
/// streamer measures that ring as `2 * unload_radius + 1` cells across in an `i32`, which this
/// keeps from overflowing.
pub const MAX_STREAM_RADIUS: i32 = (i32::MAX - 3) / 2;

/// What `--stream-radius` takes, with [`MAX_STREAM_RADIUS`] spelled out; a test holds the two
/// equal.
const STREAM_RADIUS_EXPECTED: &str = "a cell count from 0 to 1073741822";

/// What `--sun-position` takes: azimuth clockwise from north, then elevation above the horizon.
const SUN_POSITION_EXPECTED: &str =
    "two finite angles in degrees: azimuth,elevation (elevation 0 to 90)";

/// Test shorthand for a command line that must ask for a run.
#[cfg(test)]
impl EngineConfig {
    pub(crate) fn run_from_args(args: impl IntoIterator<Item = String>) -> Self {
        match Self::from_args(args) {
            Ok(ConfigAction::Run(config)) => *config,
            other => panic!("expected a run configuration, got {other:?}"),
        }
    }
}

/// How a load door takes the player to the space behind it (the "Load doors" option, group
/// "Doors"; restart to change).
///
/// The vanilla value is [`LoadDoors::LoadScreen`]: Skyrim fades to a load screen and puts the
/// player on the door's `XTEL` arrival point, which is `crate::door_crossing` (PR #128) with main's
/// walking player. [`LoadDoors::Seamless`] is Mudcrab's own: the room behind an open door drawn
/// live through the doorway and the walk carried across it (`crate::transition`, `crate::portal`,
/// `crate::door_layer`). A run gets the systems of one of the two, never both: two `E` handlers
/// would cross twice, and the off path must cost nothing.
///
/// Shaped like an entry of upstream's settings inventory (PR 229 `SettingSpec`, a `Choice`):
/// label "Load doors", group "Doors", vanilla default "load screen", differs from vanilla when
/// seamless, applies on restart (it decides which plugins a run is built with).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoadDoors {
    /// Vanilla: fade out, teleport to the arrival point, fade in once the landing cell is ready.
    #[default]
    LoadScreen,
    /// The seamless portal: the destination drawn through the open doorway, crossed on foot.
    Seamless,
}

impl LoadDoors {
    /// The values `--load-doors` takes, in the order the help lists them.
    pub const EXPECTED: &'static str = "load-screen (vanilla) or seamless";

    /// The flag value for this setting.
    pub fn name(self) -> &'static str {
        match self {
            Self::LoadScreen => "load-screen",
            Self::Seamless => "seamless",
        }
    }

    /// Reads a `--load-doors` value, any case.
    pub fn parse(value: &str) -> Option<Self> {
        [Self::LoadScreen, Self::Seamless]
            .into_iter()
            .find(|doors| doors.name().eq_ignore_ascii_case(value.trim()))
    }
}

/// The portal's own options, and everything that reads them: the walk, the demo starts, the
/// scripted tour, the shots run, the start shot, and the run modes the wiring asks about.
///
/// One struct and one parser, so that a merge from `main` finds the whole of the portal's
/// configuration in one contiguous region - plus [`EngineConfig::portal`], its default, and the two
/// lines in [`EngineConfig::from_args`] that call [`PortalOptions::parse_flag`]
/// (docs/design/portal-plugin.md).
#[derive(Debug, Clone)]
pub struct PortalOptions {
    /// Interactive first-person player (mouse look, walking, E opens load doors) instead of the
    /// free-flight camera. Never used by acceptance or benchmark runs.
    pub walk: bool,
    /// Scripted walk through the Alftand -> Blackreach doors, writing screenshots and a log here.
    pub demo_tour: Option<PathBuf>,
    /// `--tour-doors N`: walk only the first `N` doors of the `--demo-tour` route and stop. A
    /// smoke tour for iterating on the engine - it never prints the full tour's verdict word, so
    /// it cannot be mistaken for a sign-off - and inert unless `--demo-tour` names an output
    /// folder for it.
    pub tour_doors: Option<usize>,
    /// `--tour-repeat N`: walk the doors `--tour-doors` names - or, without that flag, the whole
    /// route - `N` times over instead of once.
    ///
    /// The sequence is the one the tour already walks, repeated: a route whose first doors are a
    /// house entered and left again ([`crate::demo_tour`]'s Riverwood route) is walked in and out
    /// `N` times, which is what a check that a run of crossings leaves the engine as it found it
    /// needs. Inert unless `--demo-tour` names an output folder for it, and never more than
    /// [`EngineConfig::portal`]'s own door count per repeat.
    pub tour_repeat: Option<usize>,
    /// `--tour-dwell <seconds>`: after a crossing, wait only this long before walking to the next
    /// door, instead of the four-way [`Phase::Survey`](crate::demo_tour) of the place. Reproduces a
    /// user's quick round trip through a door and back (`tasks/deepseek/impl-185-...md`): the
    /// ordinary tour's settle-then-survey pause between crossings is several seconds, long enough
    /// to hide a defect that only shows up when the player does not linger. Inert unless
    /// `--demo-tour` names an output folder for it.
    pub tour_dwell: Option<f32>,
    /// `--tour-clip`: the walk-through keeps a clip's worth of frames around each crossing instead
    /// of the twenty a check needs - the whole walk in from the standoff and a second and a half
    /// after the swap - for making a video of the crossing. Inert unless `--demo-tour` is given.
    pub tour_clip: bool,
    /// `--tour-bench <file.csv>`: at each outside door of the `--demo-tour` route, before the walk
    /// presses `E`, hold the view still and time the frames with the door closed, open in view and
    /// open behind the camera, then write one CSV row per door and state here
    /// (`crate::demo_tour`'s doorway bench). Not a timing run unless the engine runs alone. Inert
    /// unless `--demo-tour` names an output folder for it.
    pub tour_bench: Option<PathBuf>,
    /// `--portal-bench <doors.json>`: time the portal at each door of this file - closed, open in
    /// view, open behind and open behind a wall - in GPU, CPU and frame time, write the CSV and
    /// exit (`crate::portal_bench`). A run of its own: it teleports, it does not walk a demo route.
    pub portal_bench: Option<PathBuf>,
    /// `--bench-out <file.csv>`: where a `--portal-bench` run writes its CSV (and
    /// `<file.csv>.summary.txt`); [`DEFAULT_PORTAL_BENCH_OUT`] without it.
    pub bench_out: Option<PathBuf>,
    /// `--bench-seconds <s>`: how long a `--portal-bench` run times each state
    /// (`crate::portal_bench::DEFAULT_TIMING_SECONDS` without it).
    pub bench_seconds: Option<f32>,
    /// The --demo start that was chosen, if any (drives the on-screen objective).
    pub demo: Option<String>,
    /// Render each camera pose in this file to a PNG, then exit (see
    /// docs/design/reference-shots.md).
    pub shots: Option<PathBuf>,
    /// Where a shots run writes its images and `shots.log`. Defaults to a folder named after the
    /// shots file, next to it.
    pub shots_out: Option<PathBuf>,
    /// `--start-shot <shots.json> <name>`: start the run at a shot's pose, in its space
    /// (`crate::pose_capture`).
    pub start_shot: Option<StartShot>,
    /// `--show-window`: keep the window on screen in an automated run (`--demo-tour`, `--shots`),
    /// which otherwise opens it off-screen ([`EngineConfig::window_offscreen`]).
    pub show_window: bool,
    /// `--captures-dir <dir>`: where `F12` writes a run's field notes (`crate::field_notes`),
    /// instead of the default [`DEFAULT_CAPTURES_DIR`].
    pub captures_dir: PathBuf,
    /// `--field-notes-test`: a hidden acceptance flag for `crate::field_notes` - after the first
    /// door crossing and a few seconds more, takes a capture with the note "test" and exits, so a
    /// script can check a real capture without a person at the keyboard.
    pub field_notes_test: bool,
    /// `--load-doors <load-screen|seamless>`: how load doors cross ([`LoadDoors`]). The vanilla
    /// load screen by default; the depth composite, light spill, portal scale, portal cadence and
    /// eye focus are sub-options of [`LoadDoors::Seamless`] and build nothing without it.
    pub load_doors: LoadDoors,
    /// The doorway depth composite (impl-211's spike, the default since impl-227): the doorway quad
    /// writes the destination's own depth, read from the portal camera's depth buffer, instead of
    /// its own plane's, so the source geometry just behind the doorway plane (a jamb, a lintel)
    /// occludes the destination by depth rather than being painted over by the quad's rectangle
    /// (`crate::portal`'s depth composite). On by default; `--portal-depth-composite=off` turns it
    /// off, for comparison with the plain quad, and `--portal-depth-composite[=on]` back on.
    pub depth_composite: bool,
    /// `--portal-light-spill`: light spills through an open doorway both ways
    /// (`crate::portal_spill`). Off by default until it holds on held-out doorways.
    pub light_spill: bool,
    /// `--dark-sun-shadows`: keep a sun's shadow maps on while it gives no light (an interior's
    /// sun, or a doorway into one), as before research-228. Only for timing the saving
    /// (`atmosphere::sun_casts_shadows`); off by default.
    pub dark_sun_shadows: bool,
    /// `--tonemapper <name>`: the main camera's tonemapper, any case (`crate::tonemapper`). Kept
    /// as written; a name that is not one is refused before the window exists. `None` is Bevy's
    /// default, TonyMcMapface.
    pub tonemapper: Option<String>,
    /// `--graphics <preset>`: `vanilla-high` (the default), `vanilla-low`,
    /// `vanilla-medium`, `vanilla-ultra`, `showcase` (also `current`: the demo's older look), `bevy`
    /// or `custom`
    /// (`crate::graphics_settings`). Kept as written; checked before the window exists.
    pub graphics: Option<String>,
    /// `--graphics-file <path.toml|path.json>`: a settings file, read instead of the default
    /// `local/graphics.toml`.
    pub graphics_file: Option<PathBuf>,
    /// The per-knob graphics flags in command-line order, as `(name without --, value)`, e.g.
    /// `("aa", "smaa")` (`crate::graphics_settings::KNOBS`). A flag at the end of the line with no
    /// value keeps an empty one, which is refused with the valid values.
    pub graphics_knobs: Vec<(String, String)>,
    /// `--graphics-cycle <seconds>`: a debug tool that steps the anti-aliasing through every
    /// transition between its four modes, one step every this many seconds, without a keyboard
    /// (`crate::graphics_settings::GraphicsCycle`). A value that is not a positive number is
    /// ignored.
    pub graphics_cycle: Option<f32>,
}

/// Where a capture's run folder is made when `--captures-dir` is not given
/// (`local/captures/<run start>/`, `crate::field_notes`).
pub const DEFAULT_CAPTURES_DIR: &str = "local/captures";

/// Where a `--portal-bench` run writes its CSV when `--bench-out` is not given.
pub const DEFAULT_PORTAL_BENCH_OUT: &str = "local/bench/portal-bench.csv";

impl Default for PortalOptions {
    fn default() -> Self {
        Self {
            walk: false,
            demo_tour: None,
            tour_doors: None,
            tour_repeat: None,
            tour_dwell: None,
            tour_clip: false,
            tour_bench: None,
            portal_bench: None,
            bench_out: None,
            bench_seconds: None,
            demo: None,
            shots: None,
            shots_out: None,
            start_shot: None,
            show_window: false,
            captures_dir: PathBuf::from(DEFAULT_CAPTURES_DIR),
            field_notes_test: false,
            load_doors: LoadDoors::LoadScreen,
            depth_composite: true,
            light_spill: false,
            dark_sun_shadows: false,
            tonemapper: None,
            graphics: None,
            graphics_file: None,
            graphics_knobs: Vec::new(),
            graphics_cycle: None,
        }
    }
}

/// A `--start-shot` request as the command line wrote it: a shots file, and the name of the shot in
/// it to start the run at.
///
/// The shared command-line parser creates this request only when a name follows the file.
/// A file alone uses main's single-exterior-pose mode; a missing file is a parser error. Optional
/// fields still allow manually supplied requests to be validated before the window exists, in
/// [`pose_capture::start_shot_run`](crate::pose_capture::start_shot_run), which is also where the
/// shot is read and where the flags that pose the camera beside this one are refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartShot {
    /// The shots file the shot is in.
    pub file: Option<PathBuf>,
    /// The shot's name, as the file writes it.
    pub name: Option<String>,
}

impl PortalOptions {
    /// Parses one portal flag, taking the arguments it needs from `args`, and says whether
    /// `argument` was one of them.
    ///
    /// It takes the whole configuration because two of these flags are not the portal's alone:
    /// `--demo` and `--start-position` write the worldspace, the start grid and the start pose a
    /// run streams and starts in, which the streaming plan reads directly (and which a nested
    /// struct must not swallow).
    pub fn parse_flag(
        config: &mut EngineConfig,
        argument: &str,
        args: &mut impl Iterator<Item = String>,
    ) -> bool {
        match argument {
            "--walk" => config.portal.walk = true,
            "--show-window" => config.portal.show_window = true,
            "--portal-depth-composite" => config.portal.depth_composite = true,
            "--portal-depth-composite=on" => config.portal.depth_composite = true,
            "--portal-depth-composite=off" => config.portal.depth_composite = false,
            "--demo-tour" => config.portal.demo_tour = args.next().map(PathBuf::from),
            "--tour-doors" => {
                config.portal.tour_doors = args.next().and_then(|value| value.parse().ok());
            }
            "--tour-repeat" => {
                config.portal.tour_repeat = args.next().and_then(|value| value.parse().ok());
            }
            "--tour-dwell" => {
                config.portal.tour_dwell = args.next().and_then(|value| value.parse().ok());
            }
            "--tour-clip" => config.portal.tour_clip = true,
            "--tour-bench" => config.portal.tour_bench = args.next().map(PathBuf::from),
            "--portal-bench" => config.portal.portal_bench = args.next().map(PathBuf::from),
            "--bench-out" => config.portal.bench_out = args.next().map(PathBuf::from),
            "--bench-seconds" => {
                config.portal.bench_seconds = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|seconds: &f32| *seconds > 0.0);
            }
            "--demo" => {
                if let Some(name) = args.next()
                    && let Some(demo) = DemoStart::named(&name)
                {
                    demo.apply(config);
                    config.portal.demo = Some(name);
                }
            }
            "--captures-dir" => {
                if let Some(value) = args.next() {
                    config.portal.captures_dir = PathBuf::from(value);
                }
            }
            "--field-notes-test" => config.portal.field_notes_test = true,
            "--portal-light-spill" => config.portal.light_spill = true,
            "--dark-sun-shadows" => config.portal.dark_sun_shadows = true,
            "--tonemapper" => config.portal.tonemapper = args.next(),
            "--graphics" => config.portal.graphics = args.next(),
            "--graphics-file" => config.portal.graphics_file = args.next().map(PathBuf::from),
            "--graphics-cycle" => {
                config.portal.graphics_cycle = args
                    .next()
                    .and_then(|value| value.parse().ok())
                    .filter(|seconds: &f32| seconds.is_finite() && *seconds > 0.0);
            }
            knob if knob
                .strip_prefix("--")
                .is_some_and(crate::graphics_settings::is_knob) =>
            {
                let name = knob.trim_start_matches("--").to_owned();
                let value = args.next().unwrap_or_default();
                config.portal.graphics_knobs.push((name, value));
            }
            _ => return false,
        }
        true
    }

    /// Where a `--shots` run writes its images and `shots.log`: the folder `--shots-out` names, or
    /// the file's stem followed by `-shots`, next to it. `None` when there is no shots run.
    pub fn shots_output_dir(&self) -> Option<PathBuf> {
        let path = self.shots.as_ref()?;
        Some(
            self.shots_out
                .clone()
                .unwrap_or_else(|| crate::shots::default_output_dir(path)),
        )
    }
}

impl EngineConfig {
    /// Whether the window opens off-screen: in the runs nobody watches - a scripted tour and a shots
    /// run - so they do not cover the user's screen or take focus while they work (the user asked
    /// for it, 2026-09-24). The window is still a real, rendered window of its full size, so frames
    /// and screenshots are as before; `--show-window` keeps it on screen.
    pub fn window_offscreen(&self) -> bool {
        // A timing run is not parked: it opens on screen, in the middle (`app::run`).
        !self.portal.show_window
            && !self.times_frames()
            && (self.portal.demo_tour.is_some() || self.portal.shots.is_some())
    }

    /// The window's title: the run's class, what kind of automated run it is, its `--run-label`
    /// and its `--run-owner`, so a run on the taskbar says what it is and whose it is (the user
    /// asked, 2026-09-26 and 2026-10-14). An interactive run is plain "Mudcrab".
    ///
    /// The class comes from the run's own flags, never from the caller: `[TIMING]` for a run whose
    /// numbers are read ([`EngineConfig::times_frames`]; one at a time on the machine, under a
    /// build-gate timing hold), `[TEST]` for any other automated run (bug repro, visual check,
    /// shots; several at once, `docs/process/run-classes.md`).
    pub fn window_title(&self) -> String {
        let kind = if self.portal.portal_bench.is_some() {
            Some("portal bench")
        } else if self.portal.tour_bench.is_some() {
            Some("tour bench")
        } else if self.benchmark_frames.is_some() || self.benchmark_duration_secs.is_some() {
            Some("benchmark")
        } else if self.portal.demo_tour.is_some() {
            Some("demo tour")
        } else if self.matched_route.is_some() {
            Some("matched-route")
        } else if self.portal.shots.is_some() || self.shots.is_some() {
            Some("shots")
        } else if self.route.is_some() {
            Some("route")
        } else if self.portal.start_shot.is_some() {
            Some("start shot")
        } else if self.streaming_fixture {
            Some("streaming fixture")
        } else {
            None
        };
        let class = if self.times_frames() {
            "TIMING"
        } else {
            "TEST"
        };
        let mut title = match (kind, self.run_label.as_deref()) {
            (Some(kind), Some(label)) => format!("Mudcrab [{class}] {kind}: {label}"),
            (Some(kind), None) => format!("Mudcrab [{class}] {kind}"),
            (None, Some(label)) => format!("Mudcrab - {label}"),
            (None, None) => "Mudcrab".to_string(),
        };
        if let (Some(_), Some(owner)) = (kind, self.run_owner.as_deref()) {
            title.push_str(&format!(" | {owner}"));
        }
        title
    }

    /// Whether load doors cross seamlessly ([`LoadDoors::Seamless`]): the run is built with the
    /// portal, the door layer, the doorway image and the carried crossing instead of the vanilla
    /// load screen.
    pub fn seamless_doors(&self) -> bool {
        self.portal.load_doors == LoadDoors::Seamless
    }

    /// Refuses a flag that only means something with seamless doors on a load-screen run, rather
    /// than running without what it asked for: the portal bench and the tour bench time the
    /// doorway image, and the light spill is light through it.
    fn check_seamless_only_flags(&self) -> Result<(), ConfigError> {
        if self.seamless_doors() {
            return Ok(());
        }
        let asked = [
            ("--portal-bench", self.portal.portal_bench.is_some()),
            ("--tour-bench", self.portal.tour_bench.is_some()),
            ("--portal-light-spill", self.portal.light_spill),
        ];
        match asked.into_iter().find(|(_, given)| *given) {
            Some((flag, _)) => Err(ConfigError::Message(format!(
                "{flag} needs --load-doors seamless (load doors default to the vanilla load screen)"
            ))),
            None => Ok(()),
        }
    }

    /// A run that walks: `--walk` with the camera left to the player's own controller. A
    /// benchmark, an auto-flight run, a `--route` run, a `--shots` run and a `--start-shot` run keep the scripted
    /// camera whatever `--walk` was given, which is the rule `app::run` has always applied.
    pub fn walks(&self) -> bool {
        // A `--start-shot` run is driven by the player's own controller too - it starts in free
        // flight at the shot's pose (`crate::pose_capture`) - because the scripted fly camera has
        // no mouse look, and the person lining up a shot has to turn as well as move.
        (self.portal.walk || self.portal.start_shot.is_some())
            && self.benchmark_frames.is_none()
            && self.benchmark_duration_secs.is_none()
            && self.auto_fly_speed <= 0.0
            && self.route.is_none()
            && self.portal.shots.is_none()
            && self.portal.portal_bench.is_none()
    }

    /// A run that is looked at rather than measured: sky and underground lighting, portals and
    /// lights, and no acceptance capture. A measured run with `--as-played` is presented this way
    /// too, so it measures what a player sees.
    ///
    /// A `--start-shot` run is one of these: it flies a camera of its own to a pose, which is what
    /// a measured run does not do.
    ///
    /// Both this and [`walks`](Self::walks) are pure functions of the configuration
    /// (`docs/design/portal-plugin.md`, H7), so the wiring that asks them - `app::run`'s lighting
    /// gate, `crate::portal::PortalPlugin`, `crate::demo_tour::DemoTourPlugin` - agrees whatever it
    /// is asked from.
    pub fn interactive(&self) -> bool {
        self.walks()
            || self.as_played
            || self.portal.demo_tour.is_some()
            || self.portal.shots.is_some()
            || self.portal.start_shot.is_some()
            || self.portal.portal_bench.is_some()
    }

    /// A run that times frames against the renderer rather than the display: a benchmark, a
    /// `--tour-bench` or a `--portal-bench`. It presents unsynced and runs its event loop
    /// continuously, so neither V-Sync nor an unfocused window's throttle sets its frame time.
    pub fn times_frames(&self) -> bool {
        self.benchmark_frames.is_some()
            || self.benchmark_duration_secs.is_some()
            || self.portal.tour_bench.is_some()
            || self.portal.portal_bench.is_some()
    }
}

/// Named starting points for interactive demos (--demo <name>). Positions are Creation-engine
/// units taken from Skyrim.esm (see docs/research/worldspace-transition-demo.md, section 2.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DemoStart {
    pub worldspace_id: u32,
    pub position: [f32; 3],
    pub yaw: f32,
}

impl DemoStart {
    pub fn named(name: &str) -> Option<Self> {
        match name {
            // Outside the Alftand entrance in the Pale, exactly where the game puts a player who walks
            // out of Alftand01: door 000152CF's XTEL arrival point and facing (out over the tundra;
            // the entrance, auto-load door 00015D48, is behind).
            "alftand" => Some(Self {
                worldspace_id: 0x3c,
                position: [77583.23, 77411.89, -5817.21],
                yaw: -0.603_385_3,
            }),
            // Straight into Blackreach: the arrival point of Alftand's lower door (0006998D).
            "blackreach" => Some(Self {
                worldspace_id: 0x0001_EE62,
                position: [21088.559, 18512.045, 2434.0],
                yaw: -1.870_80,
            }),
            // On the Helgen road south-west of Riverwood, looking up the street at Sven's house
            // and the village (docs/design/riverwood-demo.md, section 3). The position is the
            // Creation foot point on the road chunk 0002C699; the engine adds the eye height.
            "riverwood" => Some(Self {
                worldspace_id: 0x3c,
                position: [19600.0, -46650.0, -153.0],
                yaw: 1.336_0,
            }),
            _ => None,
        }
    }

    pub fn apply(self, config: &mut EngineConfig) {
        config.worldspace_id = self.worldspace_id;
        config.start_position = Some(self.position);
        config.start_yaw = self.yaw;
        config.start_grid = grid_of(self.position[0], self.position[1]);
    }
}

/// The water reflection camera's cost options: each drops one piece of the reflection's per-view
/// work, so the portal bench can measure them one at a time (`crate::render`'s
/// `setup_water_reflection` and `limit_water_reflection_cascades` read them).
///
/// The default preserves upstream's depth prepass, occlusion culling, full target size and all
/// shadow cascades. Cost reductions require explicit flags or the optional water preset.
#[derive(Debug, Clone, PartialEq)]
pub struct ReflectionOptions {
    /// Whether the reflection camera has a `DepthPrepass`: on by default.
    /// `--reflection-prepass=off` disables it. Without the prepass Bevy's occlusion culling
    /// cannot run, so [`occlusion`](Self::occlusion) has no effect.
    pub prepass: bool,
    /// `--reflection-occlusion=off`: no `OcclusionCulling` on the reflection camera even when it
    /// has its depth prepass. On by default; disabling the prepass also disables occlusion culling.
    pub occlusion: bool,
    /// `--reflection-cascades N` (0 to [`MAX_REFLECTION_CASCADES`], or `all`): the reflection view
    /// keeps only the first `N` shadow cascades of the sun; 0 draws the reflection unshadowed.
    /// The default `None` (`all`, or `N` = [`MAX_REFLECTION_CASCADES`]) keeps every cascade the sun
    /// has and installs no limiting system.
    pub cascades: Option<usize>,
    /// `--reflection-scale S` (above 0, at most 1): the reflection target's size as a fraction of
    /// [`REFLECTION_SIZE`] per axis.
    pub scale: f32,
}

/// The water reflection target's full size, in pixels.
pub const REFLECTION_SIZE: (u32, u32) = (1024, 576);
/// The most cascades Bevy gives a directional light (`MAX_CASCADES_PER_LIGHT`).
pub const MAX_REFLECTION_CASCADES: usize = 4;
/// The smallest side a scaled reflection target is given, in pixels.
const MIN_REFLECTION_SIDE: u32 = 16;

impl Default for ReflectionOptions {
    fn default() -> Self {
        Self {
            prepass: true,
            occlusion: true,
            cascades: None,
            scale: 1.0,
        }
    }
}

impl ReflectionOptions {
    /// Parses one reflection flag, taking the argument it needs from `args`, and says whether
    /// `argument` was one of them.
    ///
    /// A value that is not one is an [`Err`] naming the flag, the value and what is accepted, and
    /// [`EngineConfig::from_args`] stops the run on it: every one of these flags drops work from the
    /// reflection view, so a run that asked for one by name would otherwise measure the default
    /// without a word (`GraphicsSettings::set` refuses a bad knob the same way). The value flags
    /// take their value either as the next argument or after `=`, so no spelling of a reflection
    /// flag is ignored.
    pub fn parse_flag(
        options: &mut Self,
        argument: &str,
        args: &mut impl Iterator<Item = String>,
    ) -> Result<bool, String> {
        match argument {
            // The switches carry their value: a bare `--reflection-prepass` (or any value but
            // `on`/`off`) is an error rather than the default.
            flag if flag.starts_with("--reflection-prepass") => {
                options.prepass = reflection_switch(flag, "--reflection-prepass")?;
            }
            flag if flag.starts_with("--reflection-occlusion") => {
                options.occlusion = reflection_switch(flag, "--reflection-occlusion")?;
            }
            flag if flag.starts_with("--reflection-cascades") => {
                let accepted = cascades_accepted();
                let value = reflection_value(flag, "--reflection-cascades", &accepted, args)?;
                options.cascades = if value.value == "all" {
                    None
                } else {
                    let count = value
                        .value
                        .parse::<usize>()
                        .ok()
                        .filter(|count| *count <= MAX_REFLECTION_CASCADES)
                        .ok_or_else(|| value.rejected(&accepted))?;
                    // Every cascade Bevy gives a light is the same as no limit at all.
                    (count < MAX_REFLECTION_CASCADES).then_some(count)
                };
            }
            flag if flag.starts_with("--reflection-scale") => {
                let accepted = "a fraction above 0, at most 1";
                let value = reflection_value(flag, "--reflection-scale", accepted, args)?;
                options.scale = value
                    .value
                    .parse::<f32>()
                    .ok()
                    .filter(|scale| scale.is_finite() && *scale > 0.0 && *scale <= 1.0)
                    .ok_or_else(|| value.rejected(accepted))?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Whether the reflection camera carries `OcclusionCulling`: asked for, and with the depth
    /// prepass Bevy's occlusion culling requires (`bevy_render-0.19.0/src/occlusion_culling/mod.rs`,
    /// "Occlusion culling currently requires a `DepthPrepass`").
    pub fn occlusion_culling(&self) -> bool {
        self.prepass && self.occlusion
    }

    /// The reflection target's size in pixels.
    pub fn target_size(&self) -> (u32, u32) {
        self.scaled_size(REFLECTION_SIZE)
    }

    /// `full`, a reflection target size in pixels, scaled by [`scale`](Self::scale) per axis: what
    /// `--reflection-scale` does to whichever size the `--water-reflection-preset` starts from.
    pub fn scaled_size(&self, full: (u32, u32)) -> (u32, u32) {
        let side = |full: u32| ((full as f32 * self.scale).round() as u32).max(MIN_REFLECTION_SIDE);
        (side(full.0), side(full.1))
    }
}

/// What `--reflection-cascades` accepts, for the message a value that is not one is refused with.
/// The count is [`MAX_REFLECTION_CASCADES`], so the message cannot drift from the limit.
fn cascades_accepted() -> String {
    format!("0 to {MAX_REFLECTION_CASCADES} (Bevy 0.19's limit), or all")
}

/// The `=on`/`=off` a reflection switch carries: the value is part of the argument, so a bare flag
/// or anything else after its name is an error naming what the flag accepts.
fn reflection_switch(argument: &str, flag: &str) -> Result<bool, String> {
    match argument
        .strip_prefix(flag)
        .and_then(|value| value.strip_prefix('='))
    {
        Some("on") => Ok(true),
        Some("off") => Ok(false),
        _ => Err(format!("{argument}: valid: {flag}=on, {flag}=off")),
    }
}

/// A reflection flag's value, and the flag with that value as it was written, for the message a
/// value that is not one is refused with: `--reflection-scale "wide"` for `--flag value`, and
/// `--reflection-scale=wide` for `--flag=value`.
struct ReflectionValue {
    value: String,
    written: String,
}

impl ReflectionValue {
    /// The message refusing this value: what was written, and what the flag accepts.
    fn rejected(&self, accepted: &str) -> String {
        format!("{}: {accepted}", self.written)
    }
}

/// The value of a reflection flag that takes one, written `--flag value` or `--flag=value`. A flag
/// with nothing after its name, with no value following it, or with anything else after its name
/// (`--flag`, a name that is not this flag) is an error naming the flag and what it accepts.
fn reflection_value(
    argument: &str,
    flag: &str,
    accepted: &str,
    args: &mut impl Iterator<Item = String>,
) -> Result<ReflectionValue, String> {
    match argument.strip_prefix(flag) {
        Some("") => {
            let value = args
                .next()
                .ok_or_else(|| format!("{flag}: the value is missing; valid: {accepted}"))?;
            Ok(ReflectionValue {
                written: format!("{flag} {value:?}"),
                value,
            })
        }
        Some(value) => match value.strip_prefix('=') {
            Some(value) => Ok(ReflectionValue {
                value: value.to_owned(),
                written: argument.to_owned(),
            }),
            None => Err(format!("{argument}: valid: {flag} <value>, {accepted}")),
        },
        None => Err(format!("{argument}: valid: {flag} <value>, {accepted}")),
    }
}

/// The exterior cell grid containing a Creation-engine position (cells are 4096 units).
pub fn grid_of(x: f32, y: f32) -> (i32, i32) {
    ((x / 4096.0).floor() as i32, (y / 4096.0).floor() as i32)
}

fn parse_offset(value: &str) -> Option<(f32, f32, f32)> {
    let mut parts = value.split(',');
    let x: f32 = parts.next()?.trim().parse().ok()?;
    let y: f32 = parts.next()?.trim().parse().ok()?;
    let z: f32 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return None;
    }
    Some((x, y, z))
}

/// The furthest grid coordinate `--benchmark-jump` accepts. The jump position maths subtracts the
/// render origin and scales by the cell size, so an extreme `i32` would overflow; half the distance
/// to `i32::MAX` in cells is far past any converted world.
const JUMP_GRID_LIMIT: i32 = 512;

fn in_jump_grid_range((x, y): (i32, i32)) -> bool {
    (-JUMP_GRID_LIMIT..=JUMP_GRID_LIMIT).contains(&x)
        && (-JUMP_GRID_LIMIT..=JUMP_GRID_LIMIT).contains(&y)
}

fn parse_grid(value: &str) -> Option<(i32, i32)> {
    let mut parts = value.split(',');
    let x: i32 = parts.next()?.trim().parse().ok()?;
    let y: i32 = parts.next()?.trim().parse().ok()?;
    parts.next().is_none().then_some((x, y))
}

fn parse_sun_position(value: &str) -> Option<(f32, f32)> {
    let mut parts = value.split(',');
    let azimuth: f32 = parts.next()?.trim().parse().ok()?;
    let elevation: f32 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some()
        || !azimuth.is_finite()
        || !elevation.is_finite()
        || !(0.0..=90.0).contains(&elevation)
    {
        return None;
    }
    Some((azimuth.rem_euclid(360.0), elevation))
}

fn parse_u32(value: &str) -> Option<u32> {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map_or_else(
            || value.parse().ok(),
            |hex| u32::from_str_radix(hex, 16).ok(),
        )
}

/// The value of a value-taking option, as written.
///
/// A missing value, or one that is another option, is an error rather than
/// something to ignore: `--profile-hardware --worldspace` would otherwise
/// swallow the second option, leaving the run in a state neither spelling
/// asked for.
fn take_raw(
    option: &'static str,
    expected: &'static str,
    value: Option<String>,
) -> Result<String, ConfigError> {
    match value {
        Some(value) if !value.starts_with("--") => Ok(value),
        // Only `--` marks an option: a path or a negative number may start
        // with a single dash.
        other => Err(ConfigError::invalid_value(option, other, expected)),
    }
}

/// An `on` or `off` value, for the measurement switches.
fn take_on_off(option: &'static str, value: Option<String>) -> Result<bool, ConfigError> {
    const EXPECTED: &str = "on or off";
    let raw = take_raw(option, EXPECTED, value)?;
    // Not a `match` on string literals: the help-drift tests read every `"..." =>` line of this file
    // as an option arm.
    if raw == "on" {
        Ok(true)
    } else if raw == "off" {
        Ok(false)
    } else {
        Err(ConfigError::invalid_value(option, Some(raw), EXPECTED))
    }
}

/// As [`take_raw`], parsed into the type the option feeds.
fn take_value<T: std::str::FromStr>(
    option: &'static str,
    expected: &'static str,
    value: Option<String>,
) -> Result<T, ConfigError> {
    let value = take_raw(option, expected, value)?;
    value
        .parse::<T>()
        .map_err(|_| ConfigError::invalid_value(option, Some(value), expected))
}

fn take_mib(
    option: &'static str,
    expected: &'static str,
    value: Option<String>,
) -> Result<usize, ConfigError> {
    let raw = take_raw(option, expected, value)?;
    raw.parse::<usize>()
        .ok()
        .filter(|mib| mib.checked_mul(1024 * 1024).is_some())
        .ok_or_else(|| ConfigError::invalid_value(option, Some(raw), expected))
}

/// The range a numeric option's value must fall in. Neither admits NaN or an infinity.
#[derive(Debug, Clone, Copy)]
enum Bound {
    /// Greater than zero.
    Positive,
    /// Zero or greater.
    NonNegative,
}

/// As [`take_value`] for a floating-point option: the value must also be finite and within
/// `bound`, or it is refused as written.
fn take_number<T>(
    option: &'static str,
    expected: &'static str,
    bound: Bound,
    value: Option<String>,
) -> Result<T, ConfigError>
where
    T: std::str::FromStr + Copy + Into<f64>,
{
    let raw = take_raw(option, expected, value)?;
    let accepted = raw.parse::<T>().ok().filter(|value| {
        let value: f64 = (*value).into();
        value.is_finite()
            && match bound {
                Bound::Positive => value > 0.0,
                Bound::NonNegative => value >= 0.0,
            }
    });
    accepted.ok_or_else(|| ConfigError::invalid_value(option, Some(raw), expected))
}

/// The option `argument` most likely meant, if it is within two edits of one.
/// Ties go to the first option the help text lists.
///
/// The candidates are [`HELP_TEXT`]'s option tokens, which the tests hold equal
/// to the parser's match arms.
fn nearest_option(argument: &str) -> Option<&'static str> {
    let mut nearest: Option<(usize, &'static str)> = None;
    for option in option_tokens(HELP_TEXT) {
        let distance = edit_distance(argument, option);
        if distance <= 2 && nearest.is_none_or(|(best, _)| distance < best) {
            nearest = Some((distance, option));
        }
    }
    nearest.map(|(_, option)| option)
}

/// Every `--option` token in `text`, so the help text and the parser arms can be
/// compared without a third list of option names to keep in step.
///
/// A token counts only where it starts a word (see [`starts_a_word`]), so
/// `word--word` and `0--10` inside a string are not read as options.
fn option_tokens(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("--") {
        let after = &rest[start + 2..];
        let length = if after.starts_with(|character: char| character.is_ascii_alphanumeric()) {
            after
                .find(|character: char| !is_option_character(character))
                .unwrap_or(after.len())
        } else {
            // A bare `--` separator, or a rule of dashes, is not an option.
            0
        };
        if length > 0 && starts_a_word(rest, start) {
            tokens.push(&rest[start..start + 2 + length]);
        }
        // Advance past the token, or past one dash of a separator so a rule of
        // dashes cannot loop. A `--` at the very end of the text ends the scan.
        rest = after.get(length.max(1)..).unwrap_or_default();
    }
    tokens
}

/// True when `index` in `text` starts a word: the text's start, a whitespace
/// boundary, the far side of a quote (how the PowerShell scripts spell an
/// option), or shell punctuation such as `(`, `=` or `;`.
fn starts_a_word(text: &str, index: usize) -> bool {
    text[..index].chars().next_back().is_none_or(|previous| {
        previous.is_whitespace()
            || matches!(previous, '"' | '\'' | '(' | '=' | ';' | '|' | '&' | ',')
    })
}

/// Options are spelled with ASCII lower case letters, digits and inner dashes.
fn is_option_character(character: char) -> bool {
    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
}

/// Levenshtein distance, used only to propose a correction for a mistyped
/// option.
fn edit_distance(left: &str, right: &str) -> usize {
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (index, left_character) in left.chars().enumerate() {
        current[0] = index + 1;
        for (offset, right_character) in right.iter().enumerate() {
            let substitution = previous[offset] + usize::from(left_character != *right_character);
            current[offset + 1] = substitution
                .min(previous[offset + 1] + 1)
                .min(current[offset] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    #[test]
    fn object_reach_never_shrinks_toward_a_coarser_level() {
        let distances = TerrainLodDistances {
            block_level0_distance: 70_000.0,
            block_level1_distance: 35_000.0,
            block_maximum_distance: 20_000.0,
            ..TerrainLodDistances::default()
        };
        let reaches: Vec<i32> = [4, 8, 16, 32]
            .into_iter()
            .map(|level| distances.object_reach_cells(level))
            .collect();
        assert_eq!(reaches, [17, 17, 17, 17]);
        assert_eq!(TerrainLodDistances::default().object_reach_cells(4), 8);
        assert_eq!(TerrainLodDistances::default().object_reach_cells(16), 61);
    }

    use super::*;

    /// This file, so the tests can read the parser's own match arms back out of
    /// it instead of keeping a second list of option names in step by hand.
    const CONFIG_SOURCE: &str = include_str!("config.rs");

    /// Flags consumed by Cargo, Git, `world-inspect`, the dynamic loader,
    /// audit tools and launch wrappers. Wrappers also contain engine invocations,
    /// so every other scanned `--flag` must be accepted by the engine parser.
    const NON_ENGINE_FLAGS: &[&str] = &[
        "--all",                        // cargo fmt
        "--all-targets",                // cargo test, cargo clippy
        "--bin",                        // cargo test
        "--bins",                       // cargo build
        "--check",                      // cargo fmt
        "--release",                    // cargo build
        "--workspace",                  // cargo build, cargo test, cargo clippy
        "--ignore-submodules",          // git diff
        "--quiet",                      // git diff
        "--short",                      // git rev-parse
        "--output",                     // world-inspect, repeat-terrain-startup.py
        "--radius",                     // world-inspect
        "--library-path",               // ld-linux
        "--meshes",                     // audit-collision.py
        "--min-x",                      // audit-collision.py
        "--max-x",                      // audit-collision.py
        "--min-y",                      // audit-collision.py
        "--max-y",                      // audit-collision.py
        "--out",                        // audit-collision.py
        "--expect-solid",               // audit-collision.py
        "--expect-passable",            // audit-collision.py
        "--original",                   // audit_asset_sizes.py
        "--json",                       // audit_asset_sizes.py
        "--candidate-inventory",        // audit-riverwood-reuse.py
        "--reference-inventory",        // audit-riverwood-reuse.py
        "--manifest",                   // audit-riverwood-reuse.py
        "--locked",                     // cargo run
        "--manifest-path",              // cargo run
        "--cpu-jobs",                   // converter
        "--io-jobs",                    // converter
        "--binary",                     // git diff
        "--porcelain",                  // git status
        "--engine",                     // repeat-terrain-startup.py
        "--repeats",                    // repeat-terrain-startup.py
        "--upload-budgets",             // repeat-terrain-startup.py
        "--warmup",                     // repeat-terrain-startup.py
        "--duration",                   // repeat-terrain-startup.py
        "--timeout",                    // repeat-terrain-startup.py
        "--launch-prefix",              // repeat-terrain-startup.py
        "--commit",                     // repeat-terrain-startup.py
        "--bench-baked",                // cluster_bench.py
        "--cluster-triangle-threshold", // cluster_bench.py
        "--converted",                  // cluster_bench.py
        "--exe",                        // cluster_bench.py
        "--keep-work",                  // cluster_bench.py
        "--lod-mode",                   // cluster_bench.py
        "--package-seconds",            // cluster_bench.py
        "--plugin",                     // cluster_bench.py
        "--previous",                   // cluster_bench.py
        "--simulate-plugin-change",     // cluster_bench.py
        "--terrain-lod-seconds",        // cluster_bench.py
        "--threshold",                  // cluster_bench.py
        "--work",                       // cluster_bench.py
        "--work-root",                  // cluster_bench.py
        "--jobs",                       // cluster_bench.py
        "--overlay",                    // cluster_repro_check.py, extension_hash.py
        "--pairs-output",               // correlate_drawables.py
        "--include",                    // audit-z-fighting.py
        "--materials-only",             // audit-z-fighting.py
        "--no-placements",              // audit-z-fighting.py
        "--plane-tolerance",            // audit-z-fighting.py
        "--min-overlap-area",           // audit-z-fighting.py
        "--max-triangles",              // audit-z-fighting.py
        "--max-comparisons",            // audit-z-fighting.py
        "--max-examples",               // audit-z-fighting.py
        "--progress",                   // audit-z-fighting.py
        "--costs",                      // compare-streaming-modes.py
        "--thermal-probe",              // compare-streaming-modes.py
        "--ablation-repeats",           // compare-streaming-modes.py
        "--resume",                     // compare-streaming-modes.py
        "--seed",                       // streaming comparison scripts
        "--smoke",                      // compare-streaming-modes.py
        "--experiment",                 // analyze-streaming-comparison.py
        "--output-dir",                 // analyze-streaming-comparison.py
        "--bootstrap-samples",          // analyze-streaming-comparison.py
        "--comparison",                 // analyze-streaming-comparison.py
        "--native-preset",              // prepare_skyrim_comparison.py
        "--calibration",                // prepare_skyrim_comparison.py
        "--user",                       // systemctl
        "--property",                   // systemctl
        "--value",                      // systemctl
        "--onlyvisible",                // xdotool
        "--name",                       // xdotool
        "--sync",                       // xdotool
        "--psm",                        // tesseract
        "--config",                     // capture_tonemap_tuning.py
        "--validate-only",              // capture_tonemap_tuning.py
        "--clearmodifiers",             // xdotool
        "--delay",                      // xdotool
        "--console-open",               // capture_skyrim_tonemap_state.py
        "--event",            // inspect_skyrim_gpu_capture.py, sample_skyrim_tonemap_gpu.py
        "--max-actions",      // inspect_skyrim_gpu_capture.py
        "--max-resource-mib", // inspect_skyrim_gpu_capture.py
        "--max-resources",    // inspect_skyrim_gpu_capture.py
        "--max-total-mib",    // inspect_skyrim_gpu_capture.py
        "--previews",         // inspect_skyrim_gpu_capture.py
        "--tail",             // inspect_skyrim_gpu_capture.py
        "--pid",              // inspect_skyrim_tonemap_memory.py
        "--atol",             // replay_skyrim_tonemap.py
        "--dot-mode",         // replay_skyrim_tonemap.py
        "--input",            // replay_skyrim_tonemap.py
        "--mad-mode",         // replay_skyrim_tonemap.py
        "--rtol",             // replay_skyrim_tonemap.py
        "--capture-sha256",   // sample_skyrim_tonemap_gpu.py
        "--output-id",        // sample_skyrim_tonemap_gpu.py
        "--pixel",            // sample_skyrim_tonemap_gpu.py
        "--shader-id",        // sample_skyrim_tonemap_gpu.py
        "--source-shader",    // sample_skyrim_tonemap_gpu.py
    ];

    fn run_config(arguments: &[&str]) -> EngineConfig {
        EngineConfig::run_from_args(arguments.iter().map(|argument| (*argument).to_owned()))
    }

    #[test]
    fn authored_start_parses_finite_feet_and_heading() {
        let config = EngineConfig::run_from_args(
            [
                "--start-position",
                "-100",
                "8200",
                "5",
                "--start-yaw",
                "1.5",
            ]
            .map(String::from),
        );
        assert_eq!(config.start_position, Some([-100.0, 8200.0, 5.0]));
        assert_eq!(config.start_grid, (-1, 2));
        assert_eq!(config.start_yaw, 1.5);
        for arguments in [
            ["--start-position", "1", "2"].as_slice(),
            ["--start-position", "NaN", "0", "0"].as_slice(),
            ["--start-yaw", "inf"].as_slice(),
        ] {
            assert!(EngineConfig::from_args(arguments.iter().map(|a| (*a).to_owned())).is_err());
        }
    }

    #[test]
    fn terrain_audit_flag_is_accepted_without_clusters() {
        assert!(!EngineConfig::default().terrain_audit);
        assert!(EngineConfig::from_args(["--terrain-audit".to_owned()]).is_ok());
        assert!(run_config(&["--terrain-audit"]).terrain_audit);
    }

    #[test]
    fn cluster_ownership_requires_a_package_and_is_accepted_with_one() {
        assert!(!EngineConfig::default().cluster_own_statics);
        assert!(EngineConfig::from_args(["--cluster-own-statics".to_owned()]).is_err());
        assert!(
            run_config(&["--clusters", "fixture", "--cluster-own-statics"]).cluster_own_statics
        );
    }

    #[test]
    fn batch_preload_is_off_by_default_and_requires_a_package() {
        assert!(!EngineConfig::default().cluster_preload_batches);
        assert!(EngineConfig::from_args(["--cluster-preload-batches".to_owned()]).is_err());
        let config = run_config(&["--clusters", "fixture", "--cluster-preload-batches"]);
        assert!(config.cluster_preload_batches);
    }

    #[test]
    fn clusters_takes_an_optional_selector() {
        assert_eq!(EngineConfig::default().clusters, None);
        assert_eq!(
            run_config(&["--clusters", "dense"]).clusters.as_deref(),
            Some("dense")
        );
        // Bare, before another option or at the end: the only overlay.
        assert_eq!(run_config(&["--clusters"]).clusters.as_deref(), Some(""));
        let config = run_config(&["--clusters", "--cluster-own-statics"]);
        assert_eq!(config.clusters.as_deref(), Some(""));
        assert!(config.cluster_own_statics);
        // The old option is gone.
        assert!(EngineConfig::from_args(["--cluster-package".to_owned(), "x".to_owned()]).is_err());
    }

    #[test]
    fn clustered_terrain_requires_an_explicit_package_and_prepass() {
        assert!(!EngineConfig::default().cluster_terrain);
        assert!(EngineConfig::from_args(["--cluster-terrain".to_owned()]).is_err());
        let config = run_config(&["--clusters", "fixture", "--cluster-terrain"]);
        assert!(config.cluster_terrain && config.cluster_prepass);
        assert!(
            EngineConfig::from_args(
                [
                    "--clusters",
                    "fixture",
                    "--cluster-terrain",
                    "--no-cluster-prepass",
                ]
                .map(str::to_owned)
            )
            .is_err()
        );
    }

    fn parse_error(arguments: &[&str]) -> ConfigError {
        match EngineConfig::from_args(arguments.iter().map(|argument| (*argument).to_owned())) {
            Err(error) => error,
            Ok(action) => panic!("expected an error, got {action:?}"),
        }
    }

    /// The option literals the parser's `match` arms name, in source order.
    fn parser_options() -> Vec<&'static str> {
        CONFIG_SOURCE
            .lines()
            .filter_map(|line| {
                let (option, rest) = leading_quoted(line)?;
                // The bare `--` arm is a no-op marker, not an option to document.
                // Only option literals: the portal's demo-name arms (`"alftand" =>`) are values.
                (rest.trim_start().starts_with("=>") && option.starts_with('-') && option != "--")
                    .then_some(option)
            })
            .collect()
    }

    /// The quoted literal `line` begins with, and the rest of the line.
    fn leading_quoted(line: &str) -> Option<(&str, &str)> {
        let (option, rest) = line.trim().strip_prefix('"')?.split_once('"')?;
        Some((option, rest))
    }

    #[test]
    fn defaults_to_one_cell_commit_within_a_sixty_fps_frame() {
        let config = EngineConfig::default();
        assert_eq!(config.max_cell_commits_per_frame, 1);
        assert_eq!(config.max_commit_micros_per_frame, 16_670);
        assert_eq!(config.max_cell_unloads_per_frame, 2);
        // The despawn's wall-clock budget, committed provisionally at 3.0: an estimate from the
        // 2026-10-08 re-profile's ~0.47 ms per retiring cell, until the timing-window A/B picks
        // the value.
        assert_eq!(config.max_unload_ms_per_frame, 3.0);
        assert_eq!(config.max_model_spawns_per_frame, 32);
        assert_eq!(config.max_lod_requests_per_frame, 1);
        assert_eq!(config.io_threads, 0);
    }

    /// Verify GPU inventory is opt-in and requires a profile output directory.
    #[test]
    fn gpu_inventory_is_separate_from_normal_profile_output() {
        assert!(!EngineConfig::default().profile_gpu_inventory);
        assert!(!run_config(&["--profile-output", "profiles/test"]).profile_gpu_inventory);
        let diagnostic = run_config(&[
            "--profile-gpu-inventory",
            "--profile-output",
            "profiles/test",
        ]);
        assert!(diagnostic.profile_gpu_inventory);
        assert_eq!(
            diagnostic.profile_output_dir,
            Some(PathBuf::from("profiles/test"))
        );
        // The switch alone must not silently do nothing or implicitly enable profile collection.
        let error = parse_error(&["--profile-gpu-inventory"]);
        assert!(matches!(
            error,
            ConfigError::MissingRequirement {
                option: "--profile-gpu-inventory",
                requires: "--profile-output",
            }
        ));
        assert_eq!(
            error.to_string(),
            "option '--profile-gpu-inventory' requires '--profile-output'. \
             Run with --help to list every option."
        );
    }

    #[test]
    fn demand_scheduling_is_opt_in_and_keeps_zero_unlimited() {
        let defaults = EngineConfig::default();
        assert!(!defaults.prioritize_streaming);
        assert_eq!(defaults.max_scene_loads, 0);
        let candidate = run_config(&["--prioritize-streaming", "--max-scene-loads", "32"]);
        assert!(candidate.prioritize_streaming);
        assert_eq!(candidate.max_scene_loads, 32);
        assert_eq!(run_config(&["--max-scene-loads", "0"]).max_scene_loads, 0);
        assert!(matches!(
            parse_error(&["--max-scene-loads", "-1"]),
            ConfigError::InvalidValue { .. }
        ));
    }

    #[test]
    fn pipeline_controls_default_off_and_preserve_fixed_limits() {
        let config = run_config(&[]);
        assert!(!config.adaptive_streaming);
        assert!(!config.streaming_controls_enabled());
        assert_eq!(config.streaming_backlog_limit(), 0);
        assert_eq!(config.memory_budget_bytes(), None);
        assert_eq!(config.streaming_headroom_bytes(), 2_048 * 1024 * 1024);
        assert_eq!(config.streaming_frame_ms, 16.67);
        assert!(config.streaming_costs.is_none());
        assert_eq!(config.max_scene_loads, 0);
        assert_eq!(config.max_model_spawns_per_frame, 32);
        assert_eq!(config.max_upload_bytes_per_frame(), Some(16 * 1024 * 1024));
    }

    #[test]
    fn dedicated_streaming_benchmark_route_is_opt_in_and_accepts_stationary_speed() {
        let defaults = run_config(&[]);
        assert!(!defaults.benchmark_vsync);
        assert!(defaults.streaming_benchmark_route_speed.is_none());
        assert_eq!(defaults.streaming_benchmark_settle_secs, 5.0);
        let config = run_config(&[
            "--streaming-benchmark-route-speed",
            "0",
            "--streaming-benchmark-route-secs",
            "45",
            "--streaming-benchmark-tail-secs",
            "10",
            "--streaming-benchmark-settle-secs",
            "5",
            "--streaming-benchmark-expected-models",
            "2029",
            "--streaming-benchmark-expected-lod-chunks",
            "176",
            "--streaming-benchmark-expected-terrain",
            "100",
            "--streaming-benchmark-expected-water",
            "25",
            "--benchmark-vsync",
            "--benchmark-duration",
            "210",
        ]);
        assert_eq!(config.streaming_benchmark_route_speed, Some(0.0));
        assert_eq!(config.streaming_benchmark_route_secs, 45.0);
        assert_eq!(config.streaming_benchmark_tail_secs, 10.0);
        assert_eq!(config.streaming_benchmark_expected_models, Some(2029));
        assert_eq!(config.streaming_benchmark_expected_lod_chunks, Some(176));
        assert_eq!(config.streaming_benchmark_expected_terrain, Some(100));
        assert_eq!(config.streaming_benchmark_expected_water, Some(25));
        assert!(config.benchmark_vsync);
        assert!(!config.interactive_world_physics());
        assert!(!config.streaming_controls_enabled());
    }

    #[test]
    fn benchmark_route_requires_a_real_time_deadline() {
        let error = parse_error(&["--streaming-benchmark-route-speed", "370"]);
        assert!(matches!(
            error,
            ConfigError::MissingRequirement {
                option: "--streaming-benchmark-route-speed",
                requires: "--benchmark-duration",
            }
        ));
        let config = run_config(&[
            "--streaming-benchmark-route-speed",
            "370",
            "--benchmark-duration",
            "210",
        ]);
        assert_eq!(config.benchmark_duration_secs, Some(210.0));
    }

    #[test]
    fn benchmark_route_rejects_invalid_numbers_and_anchored_camera_modes() {
        for option in [
            "--streaming-benchmark-route-speed",
            "--streaming-benchmark-tail-secs",
            "--streaming-benchmark-settle-secs",
        ] {
            for value in ["-1", "NaN", "inf"] {
                assert!(matches!(
                    parse_error(&[option, value]),
                    ConfigError::InvalidValue { .. }
                ));
            }
        }
        for value in ["0", "-1", "NaN", "inf"] {
            assert!(matches!(
                parse_error(&["--streaming-benchmark-route-secs", value]),
                ConfigError::InvalidValue { .. }
            ));
        }
        for args in [
            vec![
                "--streaming-benchmark-route-speed",
                "370",
                "--acceptance-screenshot",
                "out.png",
            ],
            vec![
                "--streaming-benchmark-route-speed",
                "370",
                "--shots",
                "poses.json",
            ],
            vec![
                "--streaming-benchmark-route-speed",
                "370",
                "--benchmark-only",
            ],
            vec![
                "--streaming-benchmark-route-speed",
                "370",
                "--physics-fixture",
            ],
            vec![
                "--streaming-benchmark-route-speed",
                "1e308",
                "--streaming-benchmark-route-secs",
                "1e308",
            ],
        ] {
            let mut args = args;
            args.extend(["--benchmark-duration", "60"]);
            assert!(matches!(
                parse_error(&args),
                ConfigError::InvalidValue { .. }
            ));
        }
    }

    #[test]
    fn adaptive_mode_enables_priority_and_finite_automatic_pipeline_limits() {
        let config = run_config(&["--adaptive-streaming"]);
        assert!(config.adaptive_streaming);
        assert!(config.prioritize_streaming);
        assert!(config.streaming_controls_enabled());
        assert_eq!(config.streaming_backlog_limit(), 256);
        assert_eq!(config.memory_budget_bytes(), Some(8_192 * 1024 * 1024));
        // Legacy fields keep their meaning; the runtime translates unlimited
        // values into finite controller ceilings only for adaptive mode.
        assert_eq!(config.max_scene_loads, 0);
        assert_eq!(config.max_model_spawns_per_frame, 32);
    }

    #[test]
    fn explicit_pipeline_limits_override_adaptive_fallbacks() {
        let config = run_config(&[
            "--adaptive-streaming",
            "--max-streaming-backlog",
            "48",
            "--streaming-memory-mib",
            "4096",
            "--streaming-headroom-mib",
            "1024",
            "--streaming-frame-ms",
            "33.33",
            "--streaming-costs",
            "metadata/custom-costs.json",
        ]);
        assert_eq!(config.streaming_backlog_limit(), 48);
        assert_eq!(config.memory_budget_bytes(), Some(4_096 * 1024 * 1024));
        assert_eq!(config.streaming_headroom_bytes(), 1_024 * 1024 * 1024);
        assert_eq!(config.streaming_frame_ms, 33.33);
        assert_eq!(
            config.streaming_costs,
            Some(PathBuf::from("metadata/custom-costs.json"))
        );
    }

    #[test]
    fn backlog_and_memory_admission_can_be_enabled_without_adaptation() {
        for args in [
            vec!["--max-streaming-backlog", "64"],
            vec!["--streaming-memory-mib", "1024"],
        ] {
            let config = run_config(&args);
            assert!(config.streaming_controls_enabled());
            assert!(!config.adaptive_streaming);
            assert!(!config.prioritize_streaming);
        }
        for args in [
            vec!["--max-streaming-backlog", "0"],
            vec!["--streaming-memory-mib", "0"],
            vec!["--streaming-costs", "costs.json"],
            vec!["--streaming-frame-ms", "8.33"],
            vec!["--streaming-headroom-mib", "0"],
        ] {
            assert!(!run_config(&args).streaming_controls_enabled());
        }
    }

    #[test]
    fn zero_pipeline_limits_select_automatic_limits_only_in_adaptive_mode() {
        let config = run_config(&[
            "--adaptive-streaming",
            "--streaming-memory-mib",
            "0",
            "--max-streaming-backlog",
            "0",
            "--streaming-headroom-mib",
            "0",
        ]);
        assert_eq!(config.memory_budget_bytes(), Some(8_192 * 1024 * 1024));
        assert_eq!(config.streaming_backlog_limit(), 256);
        assert_eq!(config.streaming_headroom_bytes(), 0);
    }

    #[test]
    fn pipeline_mib_values_must_fit_bytes_and_negative_values_are_rejected() {
        let overflow = (usize::MAX / (1024 * 1024) + 1).to_string();
        let maximum = (usize::MAX / (1024 * 1024)).to_string();
        for option in ["--streaming-memory-mib", "--streaming-headroom-mib"] {
            for invalid in ["-1", "NaN", "inf", overflow.as_str()] {
                assert!(matches!(
                    parse_error(&[option, invalid]),
                    ConfigError::InvalidValue { .. }
                ));
            }
            let config = run_config(&[option, &maximum]);
            if option == "--streaming-memory-mib" {
                assert_eq!(
                    config.memory_budget_bytes(),
                    Some((usize::MAX / (1024 * 1024) * (1024 * 1024)) as u64)
                );
            } else {
                assert_eq!(
                    config.streaming_headroom_bytes(),
                    (usize::MAX / (1024 * 1024) * (1024 * 1024)) as u64
                );
            }
        }
        assert!(matches!(
            parse_error(&["--max-streaming-backlog", "-1"]),
            ConfigError::InvalidValue { .. }
        ));
    }

    #[test]
    fn pipeline_frame_target_is_positive_finite_and_independent_of_acceptance() {
        for value in ["0", "-1", "NaN", "inf", "-inf", "1e999"] {
            assert!(matches!(
                parse_error(&["--streaming-frame-ms", value]),
                ConfigError::InvalidValue { .. }
            ));
        }
        let config = run_config(&["--streaming-frame-ms", "33.33"]);
        assert_eq!(config.streaming_frame_ms, 33.33);
        assert_eq!(config.accept_p95_ms, 16.67);
        assert_eq!(config.accept_min_fps, 60.0);
    }

    #[test]
    fn pipeline_options_require_values_without_swallowing_following_flags() {
        for option in [
            "--max-streaming-backlog",
            "--streaming-memory-mib",
            "--streaming-costs",
            "--streaming-headroom-mib",
            "--streaming-frame-ms",
        ] {
            assert!(matches!(
                parse_error(&[option]),
                ConfigError::InvalidValue { value: None, .. }
            ));
            assert!(matches!(
                parse_error(&[option, "--headless"]),
                ConfigError::InvalidValue { value: Some(value), .. } if value == "--headless"
            ));
        }
    }

    #[test]
    fn help_shows_the_model_spawn_default_the_engine_uses() {
        let line = HELP_TEXT
            .lines()
            .find(|line| {
                line.trim_start()
                    .starts_with("--max-model-spawns-per-frame ")
            })
            .expect("--help has a --max-model-spawns-per-frame line");
        let default = EngineConfig::default().max_model_spawns_per_frame;
        assert!(line.ends_with(&format!("(default: {default})")), "{line}");
    }

    #[test]
    fn parses_live_start_camera_and_opt_in_stdin_console() {
        let config = run_config(&[
            "--start-shot",
            "camera.json",
            "--stdin-console",
            "--no-render-diagnostics",
        ]);
        assert_eq!(config.start_shot, Some(PathBuf::from("camera.json")));
        assert!(config.stdin_console);
        assert!(config.no_render_diagnostics);
        assert!(config.interactive_world_physics());
        assert!(!EngineConfig::default().stdin_console);
        assert!(!EngineConfig::default().no_render_diagnostics);
        assert!(
            parse_error(&["--start-shot"])
                .to_string()
                .contains("--start-shot")
        );
    }

    #[test]
    fn parses_visual_preset_without_changing_diagnostic_grade() {
        assert_eq!(
            EngineConfig::default().visual_preset,
            SceneVisualPreset::CoolGray
        );
        for (value, expected) in [
            ("vivid", SceneVisualPreset::Vivid),
            ("COOL-GRAY", SceneVisualPreset::CoolGray),
        ] {
            let config = run_config(&["--visual-preset", value, "--color-grade", "saturation"]);
            assert_eq!(config.visual_preset, expected);
            assert_eq!(config.color_grade, SceneGradeMode::Saturation);
        }
        for args in [
            &["--visual-preset"][..],
            &["--visual-preset", "invalid"][..],
        ] {
            assert!(parse_error(args).to_string().contains("--visual-preset"));
        }
    }

    #[test]
    fn parses_explicit_image_space_sources_and_rejects_ambiguous_selections() {
        let direct = run_config(&[
            "--color-grade",
            "record-srgb",
            "--image-space",
            "0x12F88",
            "--grade-preset",
            "look.json",
        ]);
        assert_eq!(
            direct.image_space_selection().unwrap(),
            Some(ImageSpaceSelection::Direct(0x12F88))
        );
        assert_eq!(direct.grade_preset, Some(PathBuf::from("look.json")));
        let weather = run_config(&["--image-space-weather", "2074", "--image-space-slot", "DAY"]);
        assert_eq!(
            weather.image_space_selection().unwrap(),
            Some(ImageSpaceSelection::Weather {
                form_id: 0x81A,
                slot: ImageSpaceSlot::Day
            })
        );
        for args in [
            &["--color-grade", "record-srgb"][..],
            &[
                "--image-space",
                "100",
                "--image-space-weather",
                "2074",
                "--image-space-slot",
                "day",
            ][..],
            &["--image-space-weather", "2074"][..],
            &["--image-space-slot", "day"][..],
        ] {
            assert!(run_config(args).image_space_selection().is_err());
        }
        for args in [
            &["--image-space", "0xZZ"][..],
            &["--image-space-slot", "noon"][..],
            &["--grade-preset"][..],
        ] {
            assert!(EngineConfig::from_args(args.iter().map(|v| (*v).into())).is_err());
        }
    }

    #[test]
    fn tone_map_selection_is_explicit_and_keeps_the_approved_default() {
        let default = EngineConfig::default();
        assert_eq!(default.tone_map, SceneToneMapMode::Tony);
        assert!(default.tone_map_params.is_none());
        assert_eq!(default.color_grade, SceneGradeMode::SnapshotSrgb);
        let experiment = run_config(&[
            "--tone-map",
            "retail-curve",
            "--tone-map-params",
            "curve.json",
            "--image-space",
            "0x12F87",
        ]);
        assert_eq!(experiment.tone_map, SceneToneMapMode::RetailCurve);
        assert_eq!(
            experiment.tone_map_params,
            Some(PathBuf::from("curve.json"))
        );
        assert!(experiment.image_space_selection().unwrap().is_some());
        assert!(
            run_config(&["--tone-map", "retail-curve"])
                .image_space_selection()
                .is_err()
        );
        assert!(EngineConfig::from_args(["--tone-map".into(), "native".into()]).is_err());
    }

    #[test]
    fn parses_color_grade_and_rejects_unknown_modes() {
        for (value, wanted) in [
            ("off", SceneGradeMode::Off),
            ("saturation", SceneGradeMode::Saturation),
            ("FULL", SceneGradeMode::Full),
            ("SNAPSHOT-SRGB", SceneGradeMode::SnapshotSrgb),
            ("snapshot-linear", SceneGradeMode::SnapshotLinear),
        ] {
            assert_eq!(run_config(&["--color-grade", value]).color_grade, wanted);
        }
        assert_eq!(
            EngineConfig::default().color_grade,
            SceneGradeMode::SnapshotSrgb
        );
        assert!(
            parse_error(&["--color-grade", "bad"])
                .to_string()
                .contains(SCENE_GRADE_VALUES)
        );
        assert!(
            parse_error(&["--color-grade"])
                .to_string()
                .contains("--color-grade")
        );
    }

    /// Hidden capture selects rendering without enabling interactive mouse ownership.
    #[test]
    fn parses_hidden_window_capture_mode() {
        assert!(!EngineConfig::default().hidden_window);
        let config = run_config(&["--hidden-window", "--acceptance-screenshot", "capture.png"]);
        assert!(config.hidden_window);
        assert!(!config.headless);
        assert!(!config.interactive_world_physics());
        assert_eq!(
            config.acceptance_screenshot,
            Some(PathBuf::from("capture.png"))
        );
    }

    #[test]
    fn parses_the_model_spawn_budget_and_lets_zero_mean_unlimited() {
        let config = run_config(&["--max-model-spawns-per-frame", "12"]);
        assert_eq!(config.max_model_spawns_per_frame, 12);

        let unlimited = run_config(&["--max-model-spawns-per-frame", "0"]);
        assert_eq!(unlimited.max_model_spawns_per_frame, 0);
    }

    #[test]
    fn parses_the_unload_budget_and_lets_zero_mean_unlimited() {
        assert_eq!(
            run_config(&["--max-unloads-per-frame", "5"]).max_cell_unloads_per_frame,
            5
        );
        assert_eq!(
            run_config(&["--max-unloads-per-frame", "0"]).max_cell_unloads_per_frame,
            0
        );
    }

    #[test]
    fn the_startup_load_runs_unpaced_unless_asked_otherwise() {
        assert!(EngineConfig::default().unpaced_startup());
        assert!(!run_config(&["--paced-startup"]).unpaced_startup());
        // A capture run still loads unpaced, but is never covered, so a photograph cannot find a
        // black cover up.
        let shots = run_config(&["--shots", "poses.json"]);
        assert!(shots.unpaced_startup());
        assert!(!shots.covers_startup_load());
        assert!(!run_config(&["--demo-tour", "out", "--startup-cover"]).covers_startup_load());
        // The cover is opt-in.
        assert!(!EngineConfig::default().covers_startup_load());
        assert!(run_config(&["--startup-cover"]).covers_startup_load());
        assert!(!run_config(&["--paced-startup", "--startup-cover"]).covers_startup_load());
    }

    #[test]
    fn parses_the_io_thread_count_and_refuses_one_past_the_cap() {
        assert_eq!(run_config(&["--io-threads", "6"]).io_threads, 6);
        assert_eq!(run_config(&["--io-threads", "64"]).io_threads, 64);
        // 0 still asks for the automatic size.
        assert_eq!(run_config(&["--io-threads", "0"]).io_threads, 0);
        // Past the cap is refused like a malformed number.
        assert!(matches!(
            parse_error(&["--io-threads", "65"]),
            ConfigError::InvalidValue { value: Some(ref value), .. } if value == "65"
        ));
        assert_eq!(
            IO_THREADS_EXPECTED,
            format!("a thread count from 0 to {MAX_IO_THREADS}")
        );
        // `--help` spells the cap out too, so a bump must update it.
        assert!(
            HELP_TEXT.contains(&format!("asset IO threads, 0 to {MAX_IO_THREADS};")),
            "--help states a different --io-threads cap"
        );
        // A count left out is an error naming `--io-threads`; it does not swallow the option
        // after it (`--headless` would otherwise be lost and a window opened).
        assert!(matches!(
            parse_error(&["--io-threads", "--headless"]),
            ConfigError::InvalidValue { value: Some(ref value), .. } if value == "--headless"
        ));
    }

    #[test]
    fn defaults_to_a_sixteen_mib_upload_budget_per_frame() {
        assert_eq!(
            EngineConfig::default().max_upload_bytes_per_frame(),
            Some(16 * 1024 * 1024)
        );
    }

    #[test]
    fn parses_the_upload_budget_and_lets_zero_mean_unlimited() {
        let config = run_config(&["--max-upload-mib-per-frame", "4"]);
        assert_eq!(config.max_upload_mib_per_frame, 4);
        assert_eq!(config.max_upload_bytes_per_frame(), Some(4 * 1024 * 1024));

        let unlimited = run_config(&["--max-upload-mib-per-frame", "0"]);
        assert_eq!(unlimited.max_upload_mib_per_frame, 0);
        assert_eq!(unlimited.max_upload_bytes_per_frame(), None);
    }

    #[test]
    fn parses_the_benchmark_jump_target() {
        let config = EngineConfig::run_from_args(["--benchmark-jump", "12,-7"].map(str::to_owned));
        assert_eq!(config.benchmark_jump, Some((12, -7)));
        assert_eq!(EngineConfig::default().benchmark_jump, None);
        for bad in ["12", "12,", "a,b", "1,2,3", "1.5,2", ""] {
            let config = EngineConfig::run_from_args(["--benchmark-jump", bad].map(str::to_owned));
            assert_eq!(config.benchmark_jump, None, "{bad:?}");
        }
        // A missing value sets no target and does not swallow the next option.
        let config = EngineConfig::run_from_args(["--benchmark-jump"].map(str::to_owned));
        assert_eq!(config.benchmark_jump, None);
        let config =
            EngineConfig::run_from_args(["--benchmark-jump", "--headless"].map(str::to_owned));
        assert_eq!(config.benchmark_jump, None);
        assert!(config.headless);
    }

    #[test]
    fn a_jump_grid_coordinate_is_dropped_when_out_of_range() {
        let args =
            |list: &[&str]| EngineConfig::run_from_args(list.iter().map(|v| (*v).to_owned()));
        // The ends of the accepted range are kept.
        assert_eq!(
            args(&["--benchmark-jump", "512,-512"]).benchmark_jump,
            Some((512, -512))
        );
        assert_eq!(
            args(&["--benchmark-jump", "-512,512"]).benchmark_jump,
            Some((-512, 512))
        );
        // One past either end is dropped, including the extremes that would overflow the position
        // maths, and a flag after it is still read.
        for bad in [
            "513,0",
            "0,513",
            "-513,0",
            "0,-513",
            "2147483647,0",
            "0,-2147483648",
        ] {
            let config = args(&["--benchmark-jump", bad, "--benchmark-frames", "60"]);
            assert_eq!(config.benchmark_jump, None, "{bad:?}");
            assert_eq!(
                config.benchmark_frames,
                Some(60),
                "{bad:?} ate the next flag"
            );
        }
    }

    #[test]
    fn the_water_reflection_preset_defaults_to_baseline_and_parses_both_names() {
        assert_eq!(
            EngineConfig::default().water_reflection_preset,
            WaterReflectionPreset::Baseline
        );
        assert_eq!(
            run_config(&[]).water_reflection_preset,
            WaterReflectionPreset::Baseline
        );
        for (name, preset) in [
            ("baseline", WaterReflectionPreset::Baseline),
            ("water", WaterReflectionPreset::Water),
        ] {
            let config = run_config(&["--water-reflection-preset", name]);
            assert_eq!(config.water_reflection_preset, preset);
            assert_eq!(preset.name(), name);
        }
    }

    #[test]
    fn parses_the_unload_time_budget_and_lets_zero_mean_the_count_budget() {
        assert_eq!(
            run_config(&["--max-unload-ms-per-frame", "2.5"]).max_unload_ms_per_frame,
            2.5
        );
        assert_eq!(
            run_config(&["--max-unload-ms-per-frame", "0"]).max_unload_ms_per_frame,
            0.0
        );
    }

    /// The held-LOD alternative is off unless asked for: the paced retire stays the default
    /// every run gets.
    #[test]
    fn the_lod_keep_hidden_flag_is_off_by_default_and_parses_as_a_switch() {
        assert!(!EngineConfig::default().lod_keep_hidden_in_interiors);
        assert!(run_config(&["--lod-keep-hidden-in-interiors"]).lod_keep_hidden_in_interiors);
    }

    #[test]
    fn an_unknown_water_reflection_preset_or_a_missing_value_is_refused() {
        assert!(matches!(
            parse_error(&["--water-reflection-preset", "fast"]),
            ConfigError::InvalidValue { option: "--water-reflection-preset", value: Some(ref value), .. }
                if value == "fast"
        ));
        assert!(matches!(
            parse_error(&["--water-reflection-preset"]),
            ConfigError::InvalidValue {
                option: "--water-reflection-preset",
                value: None,
                ..
            }
        ));
        assert!(matches!(
            parse_error(&["--water-reflection-preset", "--headless"]),
            ConfigError::InvalidValue {
                option: "--water-reflection-preset",
                ..
            }
        ));
    }

    #[test]
    fn the_measurement_switches_default_on_and_take_on_or_off() {
        let config = run_config(&[]);
        assert!(config.water_reflection);
        assert!(config.shadows);

        let config = run_config(&["--water-reflection", "off", "--shadows", "off"]);
        assert!(!config.water_reflection);
        assert!(!config.shadows);

        let config = run_config(&["--water-reflection", "off", "--water-reflection", "on"]);
        assert!(config.water_reflection, "the last value wins");
        assert!(run_config(&["--shadows", "on"]).shadows);
    }

    #[test]
    fn a_measurement_switch_refuses_any_other_value() {
        for option in ["--water-reflection", "--shadows"] {
            for value in ["no", "true", "0", "OFF"] {
                assert!(
                    matches!(
                        parse_error(&[option, value]),
                        ConfigError::InvalidValue { value: Some(ref given), .. } if given == value
                    ),
                    "{option} accepted {value}"
                );
            }
            assert!(matches!(
                parse_error(&[option, "--headless"]),
                ConfigError::InvalidValue { value: Some(ref given), .. } if given == "--headless"
            ));
            assert!(matches!(
                parse_error(&[option]),
                ConfigError::InvalidValue { value: None, .. }
            ));
        }
    }

    #[test]
    fn help_documents_the_measurement_switches_as_such() {
        assert!(help_names("--water-reflection"));
        assert!(help_names("--shadows"));
        let rows: Vec<_> = HELP_TEXT
            .lines()
            .filter(|line| {
                line.trim_start().starts_with("--water-reflection <")
                    || line.trim_start().starts_with("--shadows <")
            })
            .collect();
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter().all(|row| row.contains("measurement switch")),
            "the A/B switches are for measurement and the help says so"
        );
    }

    #[test]
    fn help_documents_the_water_reflection_preset() {
        assert!(help_names("--water-reflection-preset"));
    }

    #[test]
    fn the_water_reflection_is_ssr_unless_asked_otherwise() {
        let config = run_config(&[]);
        assert_eq!(config.water_reflection_mode, WaterReflectionMode::Ssr);
        assert!(config.water_reflection);
        assert_eq!(config.water_ssr_steps, WaterSsrSteps::DEFAULT);
        assert_eq!(
            (
                config.water_ssr_steps.linear,
                config.water_ssr_steps.bisection
            ),
            (16, 6)
        );

        let config = run_config(&["--water-reflection", "planar"]);
        assert_eq!(config.water_reflection_mode, WaterReflectionMode::Planar);
        assert!(config.water_reflection);

        let config = run_config(&["--water-reflection", "ssr"]);
        assert_eq!(config.water_reflection_mode, WaterReflectionMode::Ssr);
        assert!(config.water_reflection);
    }

    #[test]
    fn the_last_water_reflection_value_wins_across_the_spellings() {
        let config = run_config(&["--water-reflection", "ssr", "--water-reflection", "planar"]);
        assert_eq!(config.water_reflection_mode, WaterReflectionMode::Planar);
        let config = run_config(&["--water-reflection", "ssr", "--water-reflection", "off"]);
        assert_eq!(config.water_reflection_mode, WaterReflectionMode::Planar);
        assert!(!config.water_reflection);
        let config = run_config(&["--water-reflection", "off", "--water-reflection", "ssr"]);
        assert_eq!(config.water_reflection_mode, WaterReflectionMode::Ssr);
        assert!(config.water_reflection);
    }

    #[test]
    fn a_bad_water_reflection_value_is_an_error() {
        for value in ["screen", "SSR", "Planar", "true", ""] {
            assert!(
                matches!(
                    parse_error(&["--water-reflection", value]),
                    ConfigError::InvalidValue {
                        option: "--water-reflection",
                        value: Some(ref given),
                        ..
                    } if given == value
                ),
                "--water-reflection accepted {value:?}"
            );
        }
        assert!(matches!(
            parse_error(&["--water-reflection"]),
            ConfigError::InvalidValue {
                option: "--water-reflection",
                value: None,
                ..
            }
        ));
    }

    #[test]
    fn the_preset_flag_still_parses_beside_ssr() {
        let config = run_config(&[
            "--water-reflection",
            "ssr",
            "--water-reflection-preset",
            "water",
        ]);
        assert_eq!(config.water_reflection_mode, WaterReflectionMode::Ssr);
        assert_eq!(config.water_reflection_preset, WaterReflectionPreset::Water);
    }

    #[test]
    fn water_ssr_steps_take_a_linear_count_and_an_optional_bisection_count() {
        let steps = |raw: &str| run_config(&["--water-ssr-steps", raw]).water_ssr_steps;
        assert_eq!(steps("16,6"), WaterSsrSteps::DEFAULT);
        assert_eq!(
            steps("32"),
            WaterSsrSteps {
                linear: 32,
                bisection: 6
            },
            "an omitted bisection count keeps the default"
        );
        assert_eq!(
            steps("1,0"),
            WaterSsrSteps {
                linear: 1,
                bisection: 0
            }
        );
        assert_eq!(
            steps("128,32"),
            WaterSsrSteps {
                linear: 128,
                bisection: 32
            }
        );
    }

    #[test]
    fn water_ssr_steps_refuse_zero_too_many_and_nonsense() {
        for value in [
            "0", "129", "0,6", "129,6", "16,33", "-1", "16,-1", "1.5", "16,", ",6", "16,6,2", "x",
            "",
        ] {
            assert!(
                matches!(
                    parse_error(&["--water-ssr-steps", value]),
                    ConfigError::InvalidValue {
                        option: "--water-ssr-steps",
                        value: Some(ref given),
                        ..
                    } if given == value
                ),
                "--water-ssr-steps accepted {value:?}"
            );
        }
        assert!(matches!(
            parse_error(&["--water-ssr-steps"]),
            ConfigError::InvalidValue {
                option: "--water-ssr-steps",
                value: None,
                ..
            }
        ));
        assert!(matches!(
            parse_error(&["--water-ssr-steps", "--shadows"]),
            ConfigError::InvalidValue {
                option: "--water-ssr-steps",
                ..
            }
        ));
    }

    #[test]
    fn help_says_the_ssr_flags_only_apply_with_ssr() {
        assert!(help_names("--water-ssr-steps"));
        let steps: Vec<_> = HELP_TEXT
            .lines()
            .skip_while(|line| !line.trim_start().starts_with("--water-ssr-steps"))
            .take(2)
            .collect();
        assert!(
            steps.iter().any(|line| line.contains("ssr only")),
            "the steps row says it only applies with ssr: {steps:?}"
        );
        let preset: Vec<_> = HELP_TEXT
            .lines()
            .skip_while(|line| !line.trim_start().starts_with("--water-reflection-preset"))
            .take(3)
            .collect();
        assert!(
            preset
                .iter()
                .any(|line| line.contains("ignored with --water-reflection ssr")),
            "the preset row says ssr ignores it: {preset:?}"
        );
    }

    #[test]
    fn a_negative_budget_is_refused() {
        for option in [
            "--max-unloads-per-frame",
            "--max-unload-ms-per-frame",
            "--max-model-spawns-per-frame",
            "--max-lod-requests-per-frame",
            "--max-upload-mib-per-frame",
        ] {
            assert!(
                matches!(
                    parse_error(&[option, "-1"]),
                    ConfigError::InvalidValue { value: Some(ref value), .. } if value == "-1"
                ),
                "{option} accepted -1"
            );
        }
    }

    #[test]
    fn pacing_is_measured_only_for_a_benchmark_or_a_jump() {
        assert!(!EngineConfig::default().measures_pacing());
        assert!(
            EngineConfig {
                benchmark_frames: Some(60),
                ..EngineConfig::default()
            }
            .measures_pacing()
        );
        assert!(
            EngineConfig {
                benchmark_duration_secs: Some(30.0),
                ..EngineConfig::default()
            }
            .measures_pacing()
        );
        // A jump without a frame limit or duration still runs, and warns that it has no report.
        assert!(
            EngineConfig {
                benchmark_jump: Some((3, 4)),
                ..EngineConfig::default()
            }
            .measures_pacing()
        );
    }

    #[test]
    fn fly_lag_is_recorded_only_while_flying_in_a_measured_run() {
        let flying = EngineConfig {
            auto_fly_speed: 900.0,
            ..EngineConfig::default()
        };
        assert!(!flying.records_fly_lag(), "flying alone measures nothing");
        assert!(
            EngineConfig {
                benchmark_frames: Some(60),
                ..flying.clone()
            }
            .records_fly_lag()
        );
        assert!(
            !EngineConfig {
                benchmark_frames: Some(60),
                ..EngineConfig::default()
            }
            .records_fly_lag(),
            "a benchmark that does not fly has no fly lag"
        );
        let routed = EngineConfig {
            route: Some("travel.json".into()),
            benchmark_duration_secs: Some(60.0),
            ..EngineConfig::default()
        };
        assert!(routed.flies());
        assert!(routed.records_fly_lag(), "a route moves the camera too");
    }

    #[test]
    fn parses_the_route_and_the_frame_budget() {
        let config = EngineConfig::run_from_args(
            [
                "--route",
                "scripts/routes/travel.json",
                "--frame-budget-ms",
                "8.33",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        assert_eq!(
            config.route.as_deref(),
            Some(std::path::Path::new("scripts/routes/travel.json"))
        );
        assert_eq!(config.frame_budget_ms, 8.33);
        let defaults = EngineConfig::default();
        assert_eq!(defaults.route, None);
        assert_eq!(defaults.frame_budget_ms, 6.25);
        for bad in [
            ["--frame-budget-ms", "0"],
            ["--frame-budget-ms", "-3"],
            ["--frame-budget-ms", "fast"],
            ["--frame-budget-ms", "--headless"],
            ["--route", "--headless"],
        ] {
            assert!(
                EngineConfig::from_args(bad.map(str::to_owned)).is_err(),
                "{bad:?} should be refused"
            );
        }
        assert!(EngineConfig::from_args(["--route".to_owned()]).is_err());
    }

    /// A route owns the camera as auto-fly does: it is never an interactive play session, and a
    /// jump it would undo is dropped.
    #[test]
    fn a_route_owns_the_camera() {
        assert!(EngineConfig::default().interactive_world_physics());
        let routed = EngineConfig::run_from_args(
            [
                "--route",
                "travel.json",
                "--benchmark-jump",
                "3,4",
                "--benchmark-duration",
                "30",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        assert!(!routed.interactive_world_physics());
        assert_eq!(routed.benchmark_jump, None, "the route would overwrite it");
        let without_report = EngineConfig {
            route: Some("travel.json".into()),
            ..EngineConfig::default()
        };
        assert!(!without_report.interactive_world_physics());
    }

    #[test]
    fn a_jump_is_ignored_when_the_run_drives_the_camera() {
        let args =
            |list: &[&str]| EngineConfig::run_from_args(list.iter().map(|v| (*v).to_owned()));
        // In either order of the flags.
        for list in [
            ["--benchmark-jump", "3,4", "--auto-fly-speed", "900"],
            ["--auto-fly-speed", "900", "--benchmark-jump", "3,4"],
        ] {
            let config = args(&list);
            assert_eq!(config.benchmark_jump, None, "{list:?}");
            assert_eq!(config.auto_fly_speed, 900.0);
        }
        let config = args(&[
            "--benchmark-jump",
            "3,4",
            "--acceptance-screenshot",
            "a.png",
        ]);
        assert_eq!(config.benchmark_jump, None);
        // The streaming fixture drives the camera every frame too.
        let config = args(&["--benchmark-jump", "3,4", "--streaming-fixture"]);
        assert_eq!(config.benchmark_jump, None);
        assert!(config.streaming_fixture);
        // A shots run poses the camera for each shot, and a jump would move it off a pose.
        let config = args(&[
            "--benchmark-jump",
            "3,4",
            "--benchmark-frames",
            "600",
            "--shots",
            "shots.json",
        ]);
        assert_eq!(config.benchmark_jump, None);
        assert!(config.shots.is_some());
        for list in [
            ["--benchmark-jump", "3,4", "--matched-route", "route.json"],
            ["--matched-route", "route.json", "--benchmark-jump", "3,4"],
        ] {
            let config = args(&list);
            assert_eq!(config.benchmark_jump, None, "{list:?}");
            assert_eq!(config.matched_route, Some(PathBuf::from("route.json")));
        }
        // A synthetic benchmark loads no world, so no streaming system would issue the jump.
        let config = args(&[
            "--benchmark-only",
            "--benchmark-frames",
            "120",
            "--benchmark-jump",
            "1,2",
        ]);
        assert_eq!(config.benchmark_jump, None);
        assert!(config.benchmark_only);
        // The single-scene fixtures create no streaming runtime either.
        for flag in [
            "--material-fixture",
            "--terrain-water-fixture",
            "--transform-bounds-fixture",
            "--renderer-fixture",
            "--physics-fixture",
        ] {
            let config = args(&["--benchmark-jump", "3,4", "--benchmark-frames", "600", flag]);
            assert_eq!(config.benchmark_jump, None, "{flag}");
        }
        assert_eq!(
            args(&["--benchmark-jump", "3,4"]).benchmark_jump,
            Some((3, 4))
        );
    }

    /// Without a frame limit or duration no report is written, so the jump has nowhere to appear:
    /// the run warns but keeps the target (an interactive run may still be worth watching).
    #[test]
    fn a_jump_without_a_report_keeps_the_target() {
        let config = EngineConfig::run_from_args(["--benchmark-jump", "3,4"].map(str::to_owned));
        assert_eq!(config.benchmark_jump, Some((3, 4)));
        assert!(!config.is_benchmark_run());
        // The jump is an automated camera path: no player whose WALK camera follow would undo it.
        assert!(!config.interactive_world_physics());
        let config = EngineConfig::run_from_args(
            ["--benchmark-jump", "3,4", "--benchmark-frames", "600"].map(str::to_owned),
        );
        assert_eq!(config.benchmark_jump, Some((3, 4)));
        assert!(config.is_benchmark_run());
    }

    #[test]
    fn parses_screenshot_camera_offset() {
        let config = run_config(&["--screenshot-camera-offset", "0,6000,12000"]);
        assert_eq!(
            config.screenshot_camera_offset,
            Some((0.0, 6000.0, 12000.0))
        );
        let config = run_config(&["--screenshot-camera-offset", "-10, 2.5 ,0"]);
        assert_eq!(config.screenshot_camera_offset, Some((-10.0, 2.5, 0.0)));
        assert_eq!(EngineConfig::default().screenshot_camera_offset, None);
    }

    #[test]
    fn parses_the_optional_sun_bearing_and_elevation() {
        let config = run_config(&["--sun-position", "450,35"]);
        assert_eq!(config.sun_position, Some((90.0, 35.0)));

        for value in ["0", "north,35", "0,91", "0,-1", "0,35,2", "NaN,35"] {
            assert!(
                matches!(
                    parse_error(&["--sun-position", value]),
                    ConfigError::InvalidValue {
                        option: "--sun-position",
                        ..
                    }
                ),
                "{value} was accepted"
            );
        }
    }

    #[test]
    fn a_malformed_screenshot_camera_offset_is_refused() {
        assert_eq!(
            parse_error(&["--screenshot-camera-offset", "0,6000"]).to_string(),
            "option '--screenshot-camera-offset' does not accept '0,6000': expected three finite \
             numbers \"x,y,z\". Run with --help to list every option."
        );
        for invalid in ["0,0,0,0", "a,b,c", "NaN,0,0"] {
            assert!(
                matches!(
                    parse_error(&["--screenshot-camera-offset", invalid]),
                    ConfigError::InvalidValue { .. }
                ),
                "{invalid} was accepted"
            );
        }
        assert_eq!(
            parse_error(&["--screenshot-camera-offset"]),
            ConfigError::InvalidValue {
                option: "--screenshot-camera-offset",
                value: None,
                expected: "three finite numbers \"x,y,z\"",
            }
        );
    }

    #[test]
    fn rejects_non_finite_screenshot_camera_offset_components() {
        for invalid in [
            "NaN,0,0",
            "0,NaN,0",
            "0,0,NaN",
            "inf,0,0",
            "0,-inf,0",
            "0,0,Infinity",
        ] {
            assert_eq!(parse_offset(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn an_automated_run_says_what_it_is_in_its_title() {
        let args = |list: &[&str]| run_config(list);
        assert_eq!(
            args(&["--benchmark-duration", "20", "--run-label", "main rural r1"]).window_title(),
            "Mudcrab [TIMING] benchmark: main rural r1"
        );
        assert_eq!(
            args(&["--benchmark-frames", "600"]).window_title(),
            "Mudcrab [TIMING] benchmark"
        );
        // A label left out does not swallow the next option.
        let config = args(&["--run-label", "--benchmark-frames", "600"]);
        assert_eq!(config.run_label, None);
        assert_eq!(config.benchmark_frames, Some(600));
        let config = args(&["--benchmark-frame-times", "--benchmark-frames", "600"]);
        assert_eq!(config.benchmark_frame_times, None);
        assert_eq!(config.benchmark_frames, Some(600));
        // Either may also end the command line with its value left out.
        assert_eq!(args(&["--run-label"]).run_label, None);
        assert_eq!(
            args(&["--benchmark-frame-times"]).benchmark_frame_times,
            None
        );
        assert_eq!(
            args(&["--streaming-fixture"]).window_title(),
            "Mudcrab [TEST] streaming fixture"
        );
        assert_eq!(
            args(&["--shots", "poses.json", "--run-label", "riverwood"]).window_title(),
            "Mudcrab [TEST] shots: riverwood"
        );
        assert_eq!(args(&[]).window_title(), "Mudcrab");
    }

    #[test]
    fn a_run_title_names_its_class_and_owner() {
        let args = |list: &[&str]| run_config(list);
        assert_eq!(
            args(&[
                "--benchmark-frames",
                "600",
                "--run-label",
                "main r1",
                "--run-owner",
                "Fix: doors"
            ])
            .window_title(),
            "Mudcrab [TIMING] benchmark: main r1 | Fix: doors"
        );
        assert_eq!(
            args(&["--shots", "s.json", "--run-owner", "Build: water"]).window_title(),
            "Mudcrab [TEST] shots | Build: water"
        );
        assert_eq!(
            args(&["--route", "r.json"]).window_title(),
            "Mudcrab [TEST] route"
        );
        // An owner left out does not swallow the next option; an interactive run stays plain.
        let config = args(&["--run-owner", "--benchmark-frames", "600"]);
        assert_eq!(config.run_owner, None);
        assert_eq!(
            args(&["--walk", "--run-owner", "x"]).window_title(),
            "Mudcrab"
        );
    }

    /// A file route cannot share the camera with the streaming benchmark route. Refused while the
    /// arguments are parsed, before any asset or device work starts.
    #[test]
    fn a_file_route_refuses_the_streaming_benchmark_route() {
        let error = parse_error(&[
            "--route",
            "travel.json",
            "--streaming-benchmark-route-speed",
            "370",
            "--benchmark-duration",
            "30",
        ]);
        assert!(
            matches!(
                error,
                ConfigError::InvalidValue {
                    option: "--streaming-benchmark-route-speed",
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn matched_route_is_a_separate_automated_camera_mode() {
        let config = run_config(&["--matched-route", "route.json"]);
        assert_eq!(config.matched_route, Some(PathBuf::from("route.json")));
        assert!(!config.interactive_world_physics());
        assert!(
            run_config(&["--matched-route", "route.json", "--run-label", "test"])
                .window_title()
                .contains("matched-route")
        );
    }

    #[test]
    fn shots_settle_timeout_is_finite_positive_and_bounded() {
        assert_eq!(run_config(&[]).shots_settle_timeout, 30.0);
        assert_eq!(
            run_config(&["--shots-settle-timeout", "120"]).shots_settle_timeout,
            120.0
        );
        for value in ["0", "-1", "NaN", "inf", "601"] {
            assert!(
                parse_error(&["--shots-settle-timeout", value])
                    .to_string()
                    .contains("--shots-settle-timeout")
            );
        }
    }

    /// A shots path left out does not swallow the next option, and it is an error rather than a
    /// silent fallback: continuing would run the engine interactively, a mode nobody asked for.
    #[test]
    fn a_valueless_shots_flag_is_an_error() {
        let error = parse_error(&["--shots", "--shots-out", "--lights"]).to_string();
        assert!(error.contains("--shots"), "{error}");
        let error = parse_error(&["--shots-out", "--lights"]).to_string();
        assert!(error.contains("--shots-out"), "{error}");
        let error = parse_error(&["--shots"]).to_string();
        assert!(error.contains("--shots"), "{error}");
    }

    #[test]
    fn parses_runtime_options() {
        let config = run_config(&[
            "--assets",
            "converted",
            "--worldspace",
            "0x3c",
            "--grid-x",
            "4",
            "--stream-radius",
            "5",
            "--headless",
            "--max-commit-ms",
            "8.5",
            "--max-unloads-per-frame",
            "3",
            "--max-model-spawns-per-frame",
            "6",
            "--max-upload-mib-per-frame",
            "8",
            "--run-label",
            "stress r2",
            "--benchmark-frame-times",
            "out/frames.csv",
            "--profile-output",
            "profiles/run-1",
            "--profile-scenario",
            "stress",
            "--profile-run-id",
            "run-3",
            "--profile-commit",
            "abc123",
            "--profile-dirty-worktree",
            "--profile-hardware",
            "test-machine",
            "--acceptance-screenshot",
            "evidence/rural.png",
            "--screenshot-camera-offset",
            "0,6000,12000",
            "--shots",
            "reference/riverwood_shots.json",
            "--shots-out",
            "evidence/riverwood",
            "--diagnostic-asset-fallbacks",
            "--material-fixture",
            "--terrain-water-fixture",
            "--transform-bounds-fixture",
            "--renderer-fixture",
            "--streaming-fixture",
            "--lights",
            "--room-culling",
            "--physics-fixture",
        ]);
        assert_eq!(config.assets_dir, PathBuf::from("converted"));
        assert_eq!(config.worldspace_id, 0x3c);
        assert_eq!(config.start_grid, (4, 0));
        assert_eq!((config.stream_radius, config.unload_radius), (5, 6));
        assert!(config.headless);
        assert_eq!(config.max_commit_micros_per_frame, 8_500);
        assert_eq!(config.max_cell_unloads_per_frame, 3);
        assert_eq!(config.max_model_spawns_per_frame, 6);
        assert_eq!(config.max_upload_mib_per_frame, 8);
        assert_eq!(config.run_label.as_deref(), Some("stress r2"));
        assert_eq!(
            config.benchmark_frame_times,
            Some(PathBuf::from("out/frames.csv"))
        );
        assert_eq!(
            config.profile_output_dir,
            Some(PathBuf::from("profiles/run-1"))
        );
        assert_eq!(config.profile_scenario, "stress");
        assert_eq!(config.profile_run_id, "run-3");
        assert_eq!(config.profile_commit, "abc123");
        assert!(config.profile_dirty_worktree);
        assert_eq!(config.profile_hardware, "test-machine");
        assert_eq!(
            config.acceptance_screenshot,
            Some(PathBuf::from("evidence/rural.png"))
        );
        assert_eq!(
            config.screenshot_camera_offset,
            Some((0.0, 6000.0, 12000.0))
        );
        assert_eq!(
            config.shots,
            Some(PathBuf::from("reference/riverwood_shots.json"))
        );
        assert_eq!(config.shots_out, Some(PathBuf::from("evidence/riverwood")));
        assert!(config.diagnostic_asset_fallbacks);
        assert!(config.material_fixture);
        assert!(config.terrain_water_fixture);
        assert!(config.transform_bounds_fixture);
        assert!(config.renderer_fixture);
        assert!(config.streaming_fixture);
        assert!(config.lights);
        assert!(config.room_culling);
        assert!(config.physics_fixture);
    }

    /// `--ini` files layer in order and every other option overrides them,
    /// wherever it appears on the command line.
    #[test]
    fn ini_files_apply_in_order_and_command_line_options_override_them() {
        let directory = tempfile::tempdir().unwrap();
        let skyrim = directory.path().join("Skyrim.ini");
        let prefs = directory.path().join("SkyrimPrefs.ini");
        std::fs::write(&skyrim, "[General]\nuGridsToLoad=7\n").unwrap();
        std::fs::write(
            &prefs,
            "[General]\nuGridsToLoad=9\n[TerrainManager]\nfSplitDistanceMult=1.5\n",
        )
        .unwrap();
        let path = |path: &std::path::Path| path.display().to_string();
        let layered = run_config(&["--ini", &path(&skyrim), "--ini", &path(&prefs)]);
        assert_eq!((layered.stream_radius, layered.unload_radius), (4, 5));
        assert_eq!(layered.terrain_lod.split_distance_mult, 1.5);

        let overridden = run_config(&["--stream-radius", "1", "--ini", &path(&prefs)]);
        assert_eq!((overridden.stream_radius, overridden.unload_radius), (1, 2));
        assert_eq!(overridden.terrain_lod.split_distance_mult, 1.5);

        let missing = run_config(&[
            "--ini",
            &path(&directory.path().join("absent.ini")),
            "--headless",
        ]);
        assert!(missing.headless, "a missing file is reported and skipped");
        assert_eq!(missing.terrain_lod, TerrainLodDistances::default());
    }

    /// Lights are opt-in: the flag is off unless it is given, so the default run - and every
    /// acceptance or benchmark baseline taken from one - is unchanged.
    #[test]
    fn lights_are_off_until_the_flag_is_given() {
        assert!(!EngineConfig::default().lights);
        assert!(
            !run_config(&["--headless"]).lights,
            "another flag does not turn lights on"
        );
        assert!(run_config(&["--lights"]).lights);
    }

    /// Room culling is opt-in like lights: nothing changes unless the flag is given.
    #[test]
    fn room_culling_is_off_until_the_flag_is_given() {
        assert!(!EngineConfig::default().room_culling);
        assert!(!run_config(&["--headless"]).room_culling);
        assert!(run_config(&["--room-culling"]).room_culling);
    }

    /// The parallel encoder finish (impl-1132) is the Plus default; `--no-render-parallel-finish`
    /// is stock Bevy, and the last of the two spellings wins.
    #[test]
    fn parallel_finish_is_on_until_turned_off() {
        assert!(EngineConfig::default().render_parallel_finish);
        assert!(run_config(&["--headless"]).render_parallel_finish);
        assert!(!run_config(&["--no-render-parallel-finish"]).render_parallel_finish);
        assert!(
            run_config(&["--no-render-parallel-finish", "--render-parallel-finish"])
                .render_parallel_finish
        );
        assert!(help_names("--render-parallel-finish"));
        assert!(help_names("--no-render-parallel-finish"));
    }

    /// The backends default to every backend (today's behaviour) until a run names one.
    #[test]
    fn render_backends_default_to_all_and_parse_both_names() {
        assert_eq!(EngineConfig::default().render_backends, RenderBackends::All);
        assert_eq!(run_config(&[]).render_backends, RenderBackends::All);
        for (name, backends) in [
            ("all", RenderBackends::All),
            ("vulkan", RenderBackends::Vulkan),
        ] {
            let config = run_config(&["--render-backends", name]);
            assert_eq!(config.render_backends, backends);
            assert_eq!(backends.name(), name);
        }
        assert_eq!(
            run_config(&["--render-backends", "vulkan", "--render-backends", "all"])
                .render_backends,
            RenderBackends::All,
            "the last value wins"
        );
        assert!(help_names("--render-backends"));
    }

    #[test]
    fn an_unknown_render_backend_or_a_missing_value_is_refused() {
        for value in ["dx12", "Vulkan", "VULKAN", "gl", ""] {
            assert!(
                matches!(
                    parse_error(&["--render-backends", value]),
                    ConfigError::InvalidValue { option: "--render-backends", value: Some(ref given), .. }
                        if given == value
                ),
                "--render-backends accepted {value:?}"
            );
        }
        assert!(matches!(
            parse_error(&["--render-backends"]),
            ConfigError::InvalidValue {
                option: "--render-backends",
                value: None,
                ..
            }
        ));
        assert!(matches!(
            parse_error(&["--render-backends", "--headless"]),
            ConfigError::InvalidValue {
                option: "--render-backends",
                ..
            }
        ));
    }

    #[test]
    fn interactive_physics_preserves_automated_camera_paths() {
        assert!(EngineConfig::default().interactive_world_physics());
        for config in [
            EngineConfig {
                headless: true,
                ..EngineConfig::default()
            },
            EngineConfig {
                benchmark_frames: Some(60),
                ..EngineConfig::default()
            },
            EngineConfig {
                auto_fly_speed: 900.0,
                ..EngineConfig::default()
            },
            EngineConfig {
                acceptance_screenshot: Some("shot.png".into()),
                ..EngineConfig::default()
            },
            EngineConfig {
                streaming_fixture: true,
                ..EngineConfig::default()
            },
            EngineConfig {
                shots: Some("poses.json".into()),
                ..EngineConfig::default()
            },
            EngineConfig {
                benchmark_jump: Some((3, 4)),
                ..EngineConfig::default()
            },
            EngineConfig {
                physics_fixture: true,
                ..EngineConfig::default()
            },
        ] {
            assert!(!config.interactive_world_physics());
        }
    }

    #[test]
    fn help_is_requested_by_either_spelling() {
        for spelling in ["--help", "-h"] {
            let action = EngineConfig::from_args([spelling.to_owned()])
                .expect("the help flags are accepted options");
            assert!(
                matches!(action, ConfigAction::Help),
                "{spelling} did not ask for help"
            );
        }
    }

    #[test]
    fn help_wins_over_the_rest_of_the_command_line() {
        for arguments in [
            vec!["--assets", "converted", "--help"],
            // Even an unknown option before it must not fail the run.
            vec!["--typo", "--help"],
            vec!["--typo", "-h", "riverwood"],
        ] {
            match EngineConfig::from_args(arguments.iter().map(|argument| (*argument).to_owned())) {
                Ok(ConfigAction::Help) => {}
                other => panic!("{arguments:?} did not ask for help: {other:?}"),
            }
        }
    }

    #[test]
    fn a_bare_end_of_options_marker_is_ignored() {
        // `main` ignored it, and the parser has no positional arguments for it
        // to end.
        let config = run_config(&["--", "--headless", "--", "--grid-x", "3", "--"]);
        assert!(config.headless);
        assert_eq!(config.start_grid.0, 3);
    }

    #[test]
    fn the_option_scan_ignores_a_trailing_separator_and_dashes_inside_words() {
        // The script scan reads whole files, which may end with a bare `--`.
        assert_eq!(option_tokens("--"), Vec::<&str>::new());
        assert_eq!(option_tokens("engine --assets dir --"), vec!["--assets"]);
        // `--` inside a word or a number is not an option.
        assert!(option_tokens("word--word and 0--10").is_empty());
        // A token after a separator is still found.
        assert_eq!(option_tokens("run --  --grid-x 1"), vec!["--grid-x"]);
        // So is a quoted one, which is how the PowerShell scripts spell them.
        assert_eq!(option_tokens("\"--grid-y\""), vec!["--grid-y"]);
        // And one after shell punctuation, as in a bash array or assignment.
        assert_eq!(
            option_tokens("args+=(--assets \"$d\"); x=--headless"),
            vec!["--assets", "--headless"]
        );
    }

    #[test]
    fn an_unknown_option_is_refused_and_the_nearest_option_is_named() {
        assert_eq!(
            parse_error(&["--grid-z", "3"]).to_string(),
            "unknown option '--grid-z'; did you mean '--grid-x'? \
             Run with --help to list every option."
        );
    }

    #[test]
    fn an_unknown_option_with_no_close_match_is_still_named() {
        assert_eq!(
            parse_error(&["--frobnicate"]).to_string(),
            "unknown option '--frobnicate'. Run with --help to list every option."
        );
    }

    #[test]
    fn a_positional_argument_is_refused() {
        assert_eq!(
            parse_error(&["riverwood"]).to_string(),
            "unexpected argument 'riverwood'. Run with --help to list every option."
        );
    }

    #[test]
    fn ini_v88_refuses_missing_or_option_shaped_paths() {
        for args in [&["--ini"][..], &["--ini", "--headless"]] {
            assert!(matches!(
                parse_error(args),
                ConfigError::InvalidValue {
                    option: "--ini",
                    ..
                }
            ));
        }
        assert!(matches!(
            EngineConfig::from_args(["--ini".into(), "--help".into()]),
            Ok(ConfigAction::Help)
        ));
    }

    #[test]
    fn bug_report_directory_override_preserves_relative_absolute_and_spaced_paths() {
        assert!(EngineConfig::default().bug_report_dir.is_none());
        for path in ["reports", "/tmp/Mudcrab Reports", "-reports", r"C:\Reports"] {
            assert_eq!(
                run_config(&["--bug-report-dir", path]).bug_report_dir,
                Some(PathBuf::from(path))
            );
        }
        assert_eq!(
            run_config(&["--bug-report-dir", "old", "--bug-report-dir", "new"]).bug_report_dir,
            Some(PathBuf::from("new"))
        );
    }

    #[test]
    fn bug_report_directory_override_refuses_missing_empty_and_option_shaped_values() {
        for args in [
            &["--bug-report-dir"][..],
            &["--bug-report-dir", ""],
            &["--bug-report-dir", "  "],
            &["--bug-report-dir", "--headless"],
        ] {
            assert!(matches!(
                parse_error(args),
                ConfigError::InvalidValue {
                    option: "--bug-report-dir",
                    ..
                }
            ));
        }
    }

    #[test]
    fn a_missing_value_is_refused() {
        assert_eq!(
            parse_error(&["--grid-x"]).to_string(),
            "option '--grid-x' needs a value: expected a cell coordinate. \
             Run with --help to list every option."
        );
    }

    #[test]
    fn an_option_where_a_value_belongs_is_refused() {
        // Without the check the second option would become the first one's
        // value, and neither option would do anything.
        assert_eq!(
            parse_error(&["--profile-hardware", "--wroldspace"]).to_string(),
            "option '--profile-hardware' needs a value: expected a description of the hardware, \
             but '--wroldspace' is another option. Run with --help to list every option."
        );
    }

    #[test]
    fn a_value_that_does_not_parse_is_refused() {
        assert_eq!(
            parse_error(&["--grid-x", "abc"]).to_string(),
            "option '--grid-x' does not accept 'abc': expected a cell coordinate. \
             Run with --help to list every option."
        );
        assert_eq!(
            parse_error(&["--stream-radius", "wide"]).to_string(),
            "option '--stream-radius' does not accept 'wide': expected a cell count from 0 to \
             1073741822. Run with --help to list every option."
        );
    }

    #[test]
    fn a_value_outside_the_range_the_option_allows_is_refused() {
        assert_eq!(
            parse_error(&["--max-commit-ms", "0"]).to_string(),
            "option '--max-commit-ms' does not accept '0': expected a positive number of \
             milliseconds. Run with --help to list every option."
        );
        assert_eq!(
            parse_error(&["--worldspace", "0xzz"]).to_string(),
            "option '--worldspace' does not accept '0xzz': expected a worldspace form id, \
             decimal or 0x-hex. Run with --help to list every option."
        );
    }

    #[test]
    fn the_stream_radius_bound_is_spelled_out_in_its_message() {
        assert_eq!(
            STREAM_RADIUS_EXPECTED,
            format!("a cell count from 0 to {MAX_STREAM_RADIUS}")
        );
    }

    #[test]
    fn the_stream_radius_is_bounded_so_the_unload_ring_cannot_overflow() {
        let widest = run_config(&["--stream-radius", "1073741822"]);
        assert_eq!(widest.stream_radius, MAX_STREAM_RADIUS);
        assert_eq!(widest.unload_radius, MAX_STREAM_RADIUS + 1);
        assert!(
            widest
                .unload_radius
                .checked_mul(2)
                .and_then(|across| across.checked_add(1))
                .is_some()
        );
        assert_eq!(run_config(&["--stream-radius", "0"]).unload_radius, 1);

        assert_eq!(
            parse_error(&["--stream-radius", "2147483647"]).to_string(),
            "option '--stream-radius' does not accept '2147483647': expected a cell count from 0 \
             to 1073741822. Run with --help to list every option."
        );
        for refused in ["1073741823", "-1", "-4", "99999999999"] {
            assert!(
                matches!(
                    parse_error(&["--stream-radius", refused]),
                    ConfigError::InvalidValue { value: Some(ref value), .. } if value == refused
                ),
                "--stream-radius accepted {refused}"
            );
        }
    }

    #[test]
    fn numeric_options_refuse_nan_and_infinity() {
        for option in [
            "--max-commit-ms",
            "--benchmark-duration",
            "--accept-min-fps",
            "--accept-p95-ms",
            "--accept-max-memory-growth-gib",
            "--auto-fly-speed",
        ] {
            // `1e39` is finite as an f64 but overflows the f32 `--auto-fly-speed` feeds.
            let too_large = if option == "--auto-fly-speed" {
                "1e39"
            } else {
                "1e309"
            };
            for refused in ["NaN", "nan", "inf", "-inf", "infinity", too_large] {
                assert!(
                    matches!(
                        parse_error(&[option, refused]),
                        ConfigError::InvalidValue { value: Some(ref value), .. } if value == refused
                    ),
                    "{option} accepted {refused}"
                );
            }
        }
    }

    #[test]
    fn numeric_options_refuse_values_below_their_range() {
        for (option, refused) in [
            ("--max-commit-ms", "0"),
            ("--benchmark-duration", "0"),
            ("--benchmark-duration", "-5"),
            ("--accept-min-fps", "-1"),
            ("--accept-p95-ms", "-0.5"),
            ("--accept-max-memory-growth-gib", "-1"),
            ("--auto-fly-speed", "-500"),
        ] {
            assert!(
                matches!(
                    parse_error(&[option, refused]),
                    ConfigError::InvalidValue { value: Some(ref value), .. } if value == refused
                ),
                "{option} accepted {refused}"
            );
        }
        assert_eq!(
            parse_error(&["--benchmark-duration", "0"]).to_string(),
            "option '--benchmark-duration' does not accept '0': expected a positive number of \
             seconds. Run with --help to list every option."
        );
    }

    #[test]
    fn numeric_options_keep_zero_where_it_is_valid() {
        let config = run_config(&[
            "--accept-min-fps",
            "0",
            "--accept-p95-ms",
            "0",
            "--accept-max-memory-growth-gib",
            "0",
            "--auto-fly-speed",
            "0",
            "--benchmark-duration",
            "0.5",
        ]);
        assert_eq!(config.accept_min_fps, 0.0);
        assert_eq!(config.accept_p95_ms, 0.0);
        assert_eq!(config.accept_max_memory_growth_gib, 0.0);
        assert_eq!(config.auto_fly_speed, 0.0);
        assert_eq!(config.benchmark_duration_secs, Some(0.5));
    }

    #[test]
    fn a_single_dash_still_starts_a_value() {
        // Paths and negative numbers are values; only `--` marks an option.
        let config = run_config(&["--grid-y", "-12", "--profile-output", "-design/run-1"]);
        assert_eq!(config.start_grid.1, -12);
        assert_eq!(
            config.profile_output_dir,
            Some(PathBuf::from("-design/run-1"))
        );
    }

    #[test]
    fn the_arm_scan_still_finds_the_parser_arms() {
        // A floor, not an inventory: it fails if a reformat or a move leaves the
        // scan looking at nothing, which would make the drift tests below pass
        // without checking anything.
        let options = parser_options();
        assert!(
            options.len() >= 40,
            "only {} parser arms were found in config.rs",
            options.len()
        );
        for option in [
            "--assets",
            "--help",
            "-h",
            "--max-unloads-per-frame",
            "--max-model-spawns-per-frame",
            "--max-upload-mib-per-frame",
            "--run-label",
            "--benchmark-frame-times",
            "--screenshot-camera-offset",
            "--lights",
            "--physics-fixture",
        ] {
            assert!(options.contains(&option), "the scan missed {option}");
        }
    }

    #[test]
    fn help_lists_every_option_the_parser_accepts() {
        for option in parser_options() {
            assert!(help_names(option), "--help never names {option}");
        }
    }

    /// The option name on its own: a new arm called `--grid` must not be
    /// satisfied by the `--grid-x` and `--grid-y` lines.
    #[test]
    fn help_names_an_option_only_as_a_whole_option() {
        assert!(help_names("--grid-x"));
        assert!(help_names("--assets"));
        assert!(help_names("-h"));
        assert!(!help_names("--grid"));
        assert!(!help_names("--profile"));
        assert!(!help_names("--benchmark"));
        assert!(!help_names("--stream"));
    }

    /// True when the help text spells `option` as an option of its own, and not
    /// as the prefix of a longer one.
    fn help_names(option: &str) -> bool {
        HELP_TEXT.match_indices(option).any(|(start, _)| {
            let before = &HELP_TEXT[..start];
            // The comma is the separator in the `-h, --help` row.
            let after = &HELP_TEXT[start + option.len()..];
            (before.is_empty() || before.ends_with(char::is_whitespace))
                && (after.is_empty() || after.starts_with(|c: char| c.is_whitespace() || c == ','))
        })
    }

    /// The token scan sees `--` options only; the short `-h` is covered by
    /// `help_lists_every_option_the_parser_accepts`.
    #[test]
    fn help_names_no_option_the_parser_rejects() {
        let options = parser_options();
        for option in option_tokens(HELP_TEXT) {
            assert!(
                options.contains(&option),
                "--help names {option}, which the parser refuses"
            );
        }
    }

    /// The workspace `scripts/` directory, or `None` when this crate is built
    /// outside the repository: a packaged or vendored source tree has no
    /// scripts to read, and these scans are about this repository.
    fn scripts_directory() -> Option<std::path::PathBuf> {
        let scripts = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts");
        scripts.is_dir().then_some(scripts)
    }

    /// Utility `.ps1`, `.sh` and `.py` files under `root`, subdirectories included,
    /// in a stable order. Hidden directories and script tests are excluded.
    /// Converter-only profiling tools stay in the scan; their flags are checked
    /// only on lines that invoke the engine (see `checked_script_engine_flags`).
    fn script_paths(root: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        let mut pending = vec![root.to_owned()];
        while let Some(directory) = pending.pop() {
            let entries = std::fs::read_dir(&directory)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()));
            for entry in entries {
                let entry = entry.expect("reading a scripts entry");
                let path = entry.path();
                // `file_type` does not follow links, so a linked folder cannot
                // loop the walk; hidden folders (`.venv`) hold no project scripts.
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    let name = entry.file_name();
                    // Tests inspect deliberately unsupported engine options.
                    if !name.to_string_lossy().starts_with('.') && name != "tests" {
                        pending.push(path);
                    }
                } else if matches!(
                    path.extension().and_then(|extension| extension.to_str()),
                    Some("ps1" | "sh" | "py")
                ) {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        paths
    }

    fn checked_script_engine_flags(script: &str, path: &std::path::Path) -> usize {
        let options = parser_options();
        let mut checked = 0;
        // lod-performance tools parse converter options and lod-experiment
        // tools parse their own offline options. Only their lines that name the
        // engine are engine invocations; the rest is not checked.
        let converter_tool = path.components().any(|part| {
            matches!(
                part.as_os_str().to_str(),
                Some("lod-performance" | "lod-experiment")
            )
        });
        let engine_lines: String;
        let scanned = if converter_tool {
            engine_lines = script
                .lines()
                .filter(|line| {
                    line.split(|c: char| !c.is_alphanumeric())
                        .any(|word| word == "engine")
                })
                .collect::<Vec<_>>()
                .join("\n");
            engine_lines.as_str()
        } else {
            script
        };
        for flag in script_cli_options(scanned) {
            // This database-only tool owns --verify. Keep it scoped to the tool:
            // an engine launcher passing --verify must still fail the audit.
            let observation_option = path.file_name().is_some_and(|name| {
                name == "reference-enable-observations.py" && flag == "--verify"
            });
            if NON_ENGINE_FLAGS.contains(&flag) || observation_option {
                continue;
            }
            assert!(
                options.contains(&flag)
                    || flag
                        .strip_prefix("--")
                        .is_some_and(crate::graphics_settings::is_knob),
                "{} passes {flag}, which the parser refuses",
                path.display()
            );
            checked += 1;
        }
        checked
    }

    #[test]
    fn the_script_audit_scopes_observation_options_and_rejects_unknown_engine_flags() {
        let observation = std::path::Path::new("reference-enable-observations.py");
        assert_eq!(
            checked_script_engine_flags(
                "--output manifest.json --verify manifest.json",
                observation
            ),
            0
        );
        assert_eq!(
            checked_script_engine_flags("engine --headless", observation),
            1
        );
        assert!(
            std::panic::catch_unwind(|| {
                checked_script_engine_flags("engine --unknown-engine-option", observation)
            })
            .is_err()
        );
        assert!(
            std::panic::catch_unwind(|| {
                checked_script_engine_flags(
                    "engine --verify manifest.json",
                    std::path::Path::new("launch.sh"),
                )
            })
            .is_err()
        );
    }

    /// Every utility `.ps1`, `.sh` and `.py` file in `scripts/`, subdirectories
    /// included, is read from disk, so a script added later cannot pass an
    /// option the parser refuses without failing this test. The scan is skipped
    /// when the repository's `scripts/` is not next to this crate.
    #[test]
    fn the_scripts_only_pass_options_the_parser_accepts() {
        let Some(scripts) = scripts_directory() else {
            return;
        };
        let mut checked = 0;
        for path in script_paths(&scripts) {
            let script = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
            checked += checked_script_engine_flags(&script, &path);
        }
        assert!(
            checked >= 20,
            "the script scan found {checked} engine flags; the scripts moved or changed shape"
        );
    }

    fn script_cli_options(script: &str) -> Vec<&str> {
        option_tokens(script)
            .into_iter()
            .filter(|flag| {
                // HTML-producing utilities use CSS custom properties. Keep an
                // identically named CLI argument visible to the parser check.
                let offset = flag.as_ptr() as usize - script.as_ptr() as usize;
                let prefix = &script[..offset];
                !(prefix.trim_end().ends_with("var(")
                    || prefix.ends_with(".setProperty('")
                    || prefix.ends_with(".setProperty(\""))
            })
            .collect()
    }

    #[test]
    fn script_scan_ignores_css_references_but_keeps_identically_named_cli_arguments() {
        let script = "color:var(--foreground); x.style.setProperty('--aspect','16/9'); args=['--foreground','--assets','pack'];";
        assert_eq!(script_cli_options(script), ["--foreground", "--assets"]);
    }

    #[test]
    fn checked_script_audit_ignores_css_but_rejects_the_same_unknown_cli_option() {
        let path = std::path::Path::new("capture_tonemap_tuning.py");
        let script = "color:var(--foreground); x.style.setProperty('--aspect','16/9'); engine --headless --assets pack";
        assert_eq!(checked_script_engine_flags(script, path), 2);

        assert!(
            std::panic::catch_unwind(|| {
                checked_script_engine_flags("color:var(--foreground); engine --foreground", path)
            })
            .is_err()
        );
    }

    /// The numbers a script writes after an engine option still parse. Values a
    /// script computes (`$StreamRadius`, `$(...)`, a bare sh variable name),
    /// paths and names are not read, and a Python audit script's `add_argument`
    /// declarations are not a command line, so only numbers in the shell and
    /// PowerShell scripts are checked: best effort where the flag scan above is
    /// exact.
    #[test]
    fn the_scripts_literal_values_still_parse() {
        let options = parser_options();
        let Some(scripts) = scripts_directory() else {
            return;
        };
        let mut checked = 0;
        for path in script_paths(&scripts) {
            if !matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("ps1" | "sh")
            ) {
                continue;
            }
            let script = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
            // An option and its value are paired only within one line, and
            // comment lines (usage notes such as `--max-commit-ms <ms>`) are
            // skipped.
            for line in script
                .lines()
                .filter(|line| !line.trim_start().starts_with('#'))
            {
                let tokens = script_tokens(line);
                for pair in tokens.windows(2) {
                    let (option, value) = (pair[0].as_str(), pair[1].as_str());
                    if !options.contains(&option) || !is_number_literal(value) {
                        continue;
                    }
                    match EngineConfig::from_args([option.to_owned(), value.to_owned()]) {
                        Ok(_) => checked += 1,
                        // A flag that takes no value is followed by the next word.
                        Err(ConfigError::UnexpectedArgument { .. }) => {}
                        Err(error) => panic!(
                            "{} passes {option} {value}, which the parser refuses: {error}",
                            path.display()
                        ),
                    }
                }
            }
        }
        assert!(
            checked >= 8,
            "the value scan found {checked} literal values; the scripts moved or changed shape"
        );
    }

    /// The words of one script line, split at whitespace, quotes and
    /// punctuation: a rough split, not a shell's, but enough to find a number
    /// written after an option.
    fn script_tokens(script: &str) -> Vec<String> {
        script
            .split(|character: char| {
                character.is_whitespace()
                    || matches!(
                        character,
                        '"' | '\'' | '(' | ')' | ';' | '=' | '[' | ']' | '`'
                    )
            })
            // Commas at the edges delimit PowerShell lists. Internal commas belong
            // to a CSV value such as a camera offset and must stay together.
            .map(|token| token.trim_matches(','))
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn script_v89_token_scan_preserves_csv_values_and_list_delimiters() {
        assert_eq!(
            script_tokens(r#"--screenshot-camera-offset "0,6000,8000""#),
            ["--screenshot-camera-offset", "0,6000,8000"]
        );
        assert_eq!(script_tokens("('--grid-x',5)"), ["--grid-x", "5"]);
    }

    #[test]
    fn script_v89_scan_excludes_other_tool_scopes_and_keeps_nested_utilities() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("tests")).unwrap();
        std::fs::create_dir(directory.path().join("lod-performance")).unwrap();
        std::fs::create_dir(directory.path().join("utils")).unwrap();
        std::fs::write(directory.path().join("tests/check.py"), "--log-file").unwrap();
        std::fs::write(
            directory.path().join("lod-performance/audit.py"),
            "--data-root --expected-chunks --lod-encoder",
        )
        .unwrap();
        let utility = directory.path().join("utils/capture.sh");
        std::fs::write(&utility, "--headless").unwrap();
        assert_eq!(
            script_paths(directory.path()),
            vec![directory.path().join("lod-performance/audit.py"), utility]
        );
    }

    #[test]
    fn lod_performance_flags_are_checked_only_on_engine_lines() {
        let tool = std::path::Path::new("scripts/lod-performance/run_conversion_observed.py");
        assert_eq!(
            checked_script_engine_flags("converter --data-root x --lod-encoder cpu", tool),
            0
        );
        assert_eq!(
            checked_script_engine_flags("engine --headless\nconverter --data-root x", tool),
            1
        );
        assert!(
            std::panic::catch_unwind(|| {
                checked_script_engine_flags("engine --unknown-engine-option", tool)
            })
            .is_err()
        );
    }

    #[test]
    fn lod_experiment_flags_are_checked_only_on_engine_lines() {
        let tool = std::path::Path::new("scripts/lod-experiment/build.py");
        assert_eq!(
            checked_script_engine_flags(
                "builder --meshoptimizer x --max-cluster-triangles 124 --output out",
                tool
            ),
            0
        );
        assert_eq!(
            checked_script_engine_flags("engine --headless\nbuilder --meshoptimizer x", tool),
            1
        );
        assert!(
            std::panic::catch_unwind(|| {
                checked_script_engine_flags("engine --unknown-engine-option", tool)
            })
            .is_err()
        );
    }

    #[test]
    fn unregistered_lod_experiment_utilities_stay_in_the_scan() {
        let directory = tempfile::tempdir().unwrap();
        let experiment = directory.path().join("lod-experiment");
        std::fs::create_dir(&experiment).unwrap();
        let registered = experiment.join("build.py");
        let utility = experiment.join("unregistered.py");
        std::fs::write(&registered, "--meshoptimizer x").unwrap();
        std::fs::write(&utility, "--meshoptimizer x").unwrap();
        assert_eq!(
            script_paths(directory.path()),
            vec![registered.clone(), utility.clone()]
        );
        assert_eq!(
            checked_script_engine_flags(&std::fs::read_to_string(&utility).unwrap(), &utility),
            0
        );
    }

    /// True when a script spells out a finite number.
    fn is_number_literal(value: &str) -> bool {
        value.parse::<f64>().is_ok_and(f64::is_finite)
    }

    #[test]
    fn the_doorway_depth_composite_is_on_unless_turned_off() {
        let parse = |args: &[&str]| {
            EngineConfig::run_from_args(args.iter().map(|arg| (*arg).to_owned()))
                .portal
                .depth_composite
        };
        assert!(parse(&[]), "the default doorway is the composite");
        assert!(!parse(&["--portal-depth-composite=off"]));
        assert!(parse(&["--portal-depth-composite"]));
        assert!(parse(&["--portal-depth-composite=on"]));
        assert!(
            parse(&["--portal-depth-composite=off", "--portal-depth-composite"]),
            "the last flag wins"
        );
    }

    #[test]
    fn defaults_to_a_terrain_ring_beyond_the_full_detail_grid() {
        let config = EngineConfig::default();
        assert_eq!(config.stream_radius, 2);
        assert_eq!(config.terrain_radius, DEFAULT_TERRAIN_RADIUS);
        assert!(
            config.terrain_radius > config.stream_radius,
            "the default ring is a second, wider tier around the full-detail grid"
        );
    }

    #[test]
    fn a_benchmark_run_keeps_the_ring_off_unless_asked() {
        let bench = EngineConfig::run_from_args(["--benchmark-frames", "100"].map(str::to_owned));
        assert_eq!(bench.terrain_radius, bench.stream_radius);
        let asked = EngineConfig::run_from_args(
            ["--benchmark-frames", "100", "--terrain-radius", "8"].map(str::to_owned),
        );
        assert_eq!(asked.terrain_radius, 8);
    }

    /// Inventory, explicit configuration, benchmark defaults and invalid input all use the
    /// configured residency envelope; renderer coverage only controls reversible visibility.
    #[test]
    fn lod_inventory_never_shrinks_the_resident_terrain_envelope() {
        let plain = EngineConfig::default();
        assert!(!plain.terrain_radius_given, "nothing named a ring");
        assert_eq!(
            plain.effective_terrain_radius(true),
            plain.terrain_radius,
            "inventory never removes the resident LAND stand-ins"
        );
        assert_eq!(
            plain.effective_terrain_radius(false),
            DEFAULT_TERRAIN_RADIUS,
            "with no LOD data the ring is what keeps the world from ending in a cliff"
        );

        // A named ring is drawn as given whatever the worldspace holds - including one that names
        // the default, and one that names a ring the auto rule would have turned off.
        let named = EngineConfig::run_from_args(["--terrain-radius", "8"].map(str::to_owned));
        assert!(named.terrain_radius_given);
        assert_eq!(named.effective_terrain_radius(true), 8);
        assert_eq!(named.effective_terrain_radius(false), 8);
        let named_off = EngineConfig::run_from_args(["--terrain-radius", "2"].map(str::to_owned));
        assert_eq!(
            named_off.effective_terrain_radius(false),
            2,
            "a named empty ring is drawn (nothing) even where there is no LOD"
        );

        // A benchmark run's ring is written without naming it, so the rule above still decides:
        // both answers are the empty ring it has always measured.
        let bench = EngineConfig::run_from_args(["--benchmark-frames", "100"].map(str::to_owned));
        assert!(!bench.terrain_radius_given);
        assert_eq!(bench.effective_terrain_radius(true), bench.stream_radius);
        assert_eq!(bench.effective_terrain_radius(false), bench.stream_radius);

        // A value that does not parse named nothing: the default and the auto choice stand.
        let typo =
            EngineConfig::run_from_args(["--terrain-radius", "not-a-number"].map(str::to_owned));
        assert!(!typo.terrain_radius_given);
        assert_eq!(typo.terrain_radius, DEFAULT_TERRAIN_RADIUS);
        assert_eq!(typo.effective_terrain_radius(true), DEFAULT_TERRAIN_RADIUS);
        assert_eq!(typo.effective_terrain_radius(false), DEFAULT_TERRAIN_RADIUS);
    }

    #[test]
    fn parses_and_clamps_the_terrain_radius() {
        let config = EngineConfig::run_from_args(["--terrain-radius", "16"].map(str::to_owned));
        assert_eq!(config.terrain_radius, 16);
        assert_eq!(
            config.stream_radius, 2,
            "the ring is a knob of its own, not a multiple of the full-detail grid"
        );

        // A ring at or inside the inner grid is empty: the engine as it was before the ring.
        let config = EngineConfig::run_from_args(
            ["--stream-radius", "5", "--terrain-radius", "2"].map(str::to_owned),
        );
        assert_eq!((config.stream_radius, config.unload_radius), (5, 6));
        assert_eq!(config.terrain_radius, 2);

        // A mistyped radius is clamped rather than asking the planner for millions of cells.
        assert_eq!(
            EngineConfig::run_from_args(["--terrain-radius", "100000"].map(str::to_owned))
                .terrain_radius,
            MAX_TERRAIN_RADIUS
        );
        assert_eq!(
            EngineConfig::run_from_args(["--terrain-radius", "-4"].map(str::to_owned))
                .terrain_radius,
            0
        );
        assert_eq!(
            EngineConfig::run_from_args(["--terrain-radius", "not-a-number"].map(str::to_owned))
                .terrain_radius,
            DEFAULT_TERRAIN_RADIUS,
            "an unparsable value leaves the default in place"
        );
    }

    #[test]
    fn demo_starts_set_worldspace_grid_and_position() {
        let config = EngineConfig::run_from_args(["--demo", "alftand", "--walk"].map(String::from));
        assert!(config.portal.walk);
        assert_eq!(config.worldspace_id, 0x3c);
        assert_eq!(config.start_grid, (18, 18));
        assert_eq!(config.start_position, Some([77583.23, 77411.89, -5817.21]));
        // Bethesda's own exit facing for Alftand01 -> Tamriel (door 000152CF's XTEL).
        assert_eq!(config.start_yaw, -0.603_385_3);
        assert_eq!(config.portal.demo.as_deref(), Some("alftand"));

        let config = EngineConfig::run_from_args(["--demo", "blackreach"].map(String::from));
        assert!(!config.portal.walk);
        assert_eq!(config.worldspace_id, 0x0001_EE62);
        assert_eq!(config.start_grid, (5, 4));

        let config = EngineConfig::run_from_args(["--demo", "nowhere"].map(String::from));
        assert_eq!(config.start_position, None);
        assert_eq!(config.worldspace_id, 0x3c);

        let config = EngineConfig::run_from_args(
            [
                "--start-position",
                "-100",
                "8200",
                "5",
                "--start-yaw",
                "1.5",
            ]
            .map(String::from),
        );
        assert_eq!(config.start_position, Some([-100.0, 8200.0, 5.0]));
        assert_eq!(config.start_grid, (-1, 2));
        assert_eq!(config.start_yaw, 1.5);
    }

    #[test]
    fn shots_options_parse_and_default_the_output_folder() {
        let config = EngineConfig::run_from_args(
            ["--assets", "converted", "--shots", "shots/tamriel.json"].map(str::to_owned),
        );
        assert_eq!(
            config.portal.shots,
            Some(PathBuf::from("shots/tamriel.json"))
        );
        assert_eq!(config.portal.shots_out, None);
        assert_eq!(
            config.portal.shots_output_dir(),
            Some(PathBuf::from("shots/tamriel-shots")),
            "the default output folder is named after the shots file, next to it"
        );

        let config = EngineConfig::run_from_args(
            ["--shots", "a.json", "--shots-out", "shots/out"].map(str::to_owned),
        );
        assert_eq!(
            config.portal.shots_output_dir(),
            Some(PathBuf::from("shots/out"))
        );

        // Nothing writes anywhere unless there is a shots file to render.
        assert_eq!(EngineConfig::default().portal.shots, None);
        assert_eq!(EngineConfig::default().portal.shots_output_dir(), None);
    }

    #[test]
    fn start_shot_options_parse_a_file_and_a_shot_name() {
        let config = EngineConfig::run_from_args(
            [
                "--assets",
                "converted",
                "--start-shot",
                "tools/reference/riverwood_shots.json",
                "RW-04-inn-front",
            ]
            .map(str::to_owned),
        );
        assert_eq!(
            config.portal.start_shot,
            Some(StartShot {
                file: Some(PathBuf::from("tools/reference/riverwood_shots.json")),
                name: Some("RW-04-inn-front".to_owned()),
            })
        );
        // The flag is a start pose and nothing else: it starts no shots run, no tour and no walk.
        assert_eq!(config.portal.shots, None);
        assert_eq!(config.portal.demo_tour, None);
        assert!(!config.portal.walk);
        assert_eq!(config.start_position, None);

        // A file alone keeps main's single-pose form. A following option must not become a name,
        // and a missing file is rejected before any window or asset loading starts.
        let single = run_config(&["--start-shot", "a.json", "--stdin-console"]);
        assert_eq!(single.start_shot, Some(PathBuf::from("a.json")));
        assert_eq!(single.portal.start_shot, None);
        assert!(single.stdin_console);
        assert!(
            parse_error(&["--start-shot"])
                .to_string()
                .contains("--start-shot")
        );
        assert_eq!(EngineConfig::default().portal.start_shot, None);
    }

    #[test]
    fn repeated_start_shot_forms_keep_only_the_last_camera_request() {
        let named = run_config(&[
            "--start-shot",
            "earlier-missing.json",
            "--start-shot",
            "named.json",
            "selected",
        ]);
        assert_eq!(named.start_shot, None);
        assert_eq!(
            named.portal.start_shot,
            Some(StartShot {
                file: Some(PathBuf::from("named.json")),
                name: Some("selected".to_owned()),
            })
        );

        let single = run_config(&[
            "--start-shot",
            "earlier-missing.json",
            "selected",
            "--start-shot",
            "single.json",
            "--stdin-console",
        ]);
        assert_eq!(single.start_shot, Some(PathBuf::from("single.json")));
        assert_eq!(single.portal.start_shot, None);
        assert!(single.stdin_console);
    }

    #[test]
    fn a_start_shot_run_is_looked_at_and_driven_by_the_player() {
        let config =
            EngineConfig::run_from_args(["--start-shot", "a.json", "shot"].map(str::to_owned));
        assert!(
            config.interactive(),
            "a start-shot run gets the lighting and the portal"
        );
        assert!(
            config.walks(),
            "the player's controller drives it (in free flight), so the person can look around"
        );

        // `--walk` beside it changes nothing: the controller is the player's either way.
        let walked = EngineConfig::run_from_args(
            ["--walk", "--start-shot", "a.json", "shot"].map(str::to_owned),
        );
        assert!(walked.interactive());
        assert!(walked.walks());

        // A measured run is a measured run: nothing about this flag makes a benchmark a walk.
        let measured = EngineConfig::run_from_args(
            ["--start-shot", "a.json", "shot", "--benchmark-frames", "10"].map(str::to_owned),
        );
        assert!(!measured.walks());
        assert!(measured.interactive());
    }

    #[test]
    fn an_as_played_measured_run_is_presented_like_a_played_one_but_never_walks() {
        let measured = |extra: &[&str]| {
            let mut args = vec!["--benchmark-duration", "10"];
            args.extend_from_slice(extra);
            EngineConfig::run_from_args(args.into_iter().map(str::to_owned))
        };
        assert!(
            !measured(&[]).interactive(),
            "a measured run stays flat by default"
        );
        let played = measured(&["--as-played"]);
        assert!(played.as_played);
        assert!(
            played.interactive(),
            "the sky, fog, animations and portals a player sees"
        );
        assert!(!played.walks(), "the run's camera stays scripted");
        assert!(!measured(&["--walk", "--as-played"]).walks());
    }

    #[test]
    fn automated_runs_open_their_window_off_screen_unless_asked_not_to() {
        let args = |list: &[&str]| {
            EngineConfig::run_from_args(list.iter().map(|value| (*value).to_owned()))
        };
        assert!(args(&["--demo-tour", "local/tour"]).window_offscreen());
        assert!(args(&["--shots", "shots.json"]).window_offscreen());
        assert!(!args(&["--demo-tour", "local/tour", "--show-window"]).window_offscreen());
        assert!(
            !args(&["--walk"]).window_offscreen(),
            "a run someone plays is on screen"
        );
        assert!(
            !args(&["--start-shot", "shots.json", "a"]).window_offscreen(),
            "the fitting tool is looked at"
        );
    }

    #[test]
    fn a_timing_run_opens_on_screen_and_its_title_says_what_it_is() {
        let args = |list: &[&str]| {
            EngineConfig::run_from_args(list.iter().map(|value| (*value).to_owned()))
        };
        let bench = args(&[
            "--load-doors",
            "seamless",
            "--portal-bench",
            "doors.json",
            "--run-label",
            "default r1",
            "--bench-seconds",
            "2",
        ]);
        assert!(!bench.window_offscreen(), "a bench is watched");
        assert_eq!(
            bench.window_title(),
            "Mudcrab [TIMING] portal bench: default r1"
        );
        assert_eq!(bench.portal.bench_seconds, Some(2.0));
        assert!(
            !args(&[
                "--load-doors",
                "seamless",
                "--demo-tour",
                "t",
                "--tour-bench",
                "b"
            ])
            .window_offscreen()
        );
        assert_eq!(
            args(&["--demo-tour", "t"]).window_title(),
            "Mudcrab [TEST] demo tour"
        );
        assert_eq!(
            args(&["--shots", "s.json"]).window_title(),
            "Mudcrab [TEST] shots"
        );
        assert_eq!(args(&["--walk"]).window_title(), "Mudcrab");
        assert_eq!(
            args(&[
                "--load-doors",
                "seamless",
                "--portal-bench",
                "d",
                "--bench-seconds",
                "-1"
            ])
            .portal
            .bench_seconds,
            None
        );
    }

    #[test]
    fn a_smoke_tour_flag_names_how_many_doors_to_walk() {
        let config = EngineConfig::run_from_args(
            [
                "--demo",
                "riverwood",
                "--walk",
                "--demo-tour",
                "out",
                "--tour-doors",
                "1",
            ]
            .map(str::to_owned),
        );
        assert_eq!(config.portal.tour_doors, Some(1));
        assert_eq!(
            config.portal.demo_tour,
            Some(PathBuf::from("out")),
            "a smoke tour still needs the folder the full tour writes its log and shots to"
        );

        // Without the flag there is no smoke tour, and the route is walked whole.
        assert_eq!(EngineConfig::default().portal.tour_doors, None);
        assert_eq!(
            EngineConfig::run_from_args(["--demo-tour", "out"].map(str::to_owned))
                .portal
                .tour_doors,
            None
        );
    }

    #[test]
    fn a_tour_repeat_flag_names_how_many_times_to_walk_them() {
        let config = EngineConfig::run_from_args(
            [
                "--demo",
                "riverwood",
                "--walk",
                "--demo-tour",
                "out",
                "--tour-doors",
                "2",
                "--tour-repeat",
                "5",
            ]
            .map(str::to_owned),
        );
        assert_eq!(config.portal.tour_repeat, Some(5));
        assert_eq!(
            config.portal.tour_doors,
            Some(2),
            "the repeat does not stand in for the doors it repeats"
        );

        // Without the flag a tour walks what it always did: its doors once.
        assert_eq!(EngineConfig::default().portal.tour_repeat, None);
        assert_eq!(
            EngineConfig::run_from_args(["--demo-tour", "out"].map(str::to_owned))
                .portal
                .tour_repeat,
            None
        );
    }

    #[test]
    fn a_tour_clip_flag_asks_for_a_clip_of_each_crossing() {
        let config =
            EngineConfig::run_from_args(["--demo-tour", "out", "--tour-clip"].map(str::to_owned));
        assert!(config.portal.tour_clip);
        assert!(!EngineConfig::default().portal.tour_clip);
    }

    #[test]
    fn a_tour_dwell_flag_names_how_long_to_wait_before_the_next_door() {
        let config = EngineConfig::run_from_args(
            [
                "--demo",
                "riverwood",
                "--walk",
                "--demo-tour",
                "out",
                "--tour-doors",
                "2",
                "--tour-dwell",
                "2",
            ]
            .map(str::to_owned),
        );
        assert_eq!(config.portal.tour_dwell, Some(2.0));
        assert_eq!(
            config.portal.tour_doors,
            Some(2),
            "the dwell does not stand in for the doors to walk"
        );

        // Without the flag a tour surveys the place between crossings, as it always did.
        assert_eq!(EngineConfig::default().portal.tour_dwell, None);
        assert_eq!(
            EngineConfig::run_from_args(["--demo-tour", "out"].map(str::to_owned))
                .portal
                .tour_dwell,
            None
        );
    }

    /// The load doors are the vanilla load screen unless asked; a value that is not one, or a flag
    /// that only means something with seamless doors, stops the run.
    #[test]
    fn load_doors_default_to_the_load_screen_and_refuse_seamless_only_flags() {
        let run = |list: &[&str]| EngineConfig::from_args(list.iter().map(|a| a.to_string()));
        assert_eq!(
            EngineConfig::default().portal.load_doors,
            LoadDoors::LoadScreen
        );
        assert!(!EngineConfig::default().seamless_doors());
        assert!(matches!(
            run(&["--load-doors", "SEAMLESS"]),
            Ok(ConfigAction::Run(config)) if config.seamless_doors()
        ));
        assert_eq!(
            run(&["--load-doors", "portal"])
                .expect_err("not a value")
                .to_string(),
            ConfigError::invalid_value(
                "--load-doors",
                Some("portal".to_owned()),
                LoadDoors::EXPECTED
            )
            .to_string()
        );
        for flag in [
            &["--portal-bench", "doors.json"][..],
            &["--demo-tour", "out", "--tour-bench", "b.csv"][..],
            &["--portal-light-spill"][..],
        ] {
            let error = run(flag).expect_err("needs seamless doors").to_string();
            assert!(error.contains("needs --load-doors seamless"), "{error}");
        }
    }

    #[test]
    fn a_tour_bench_flag_names_the_csv_to_write() {
        let config = EngineConfig::run_from_args(
            [
                "--load-doors",
                "seamless",
                "--demo-tour",
                "out",
                "--tour-doors",
                "2",
                "--tour-bench",
                "out/bench.csv",
            ]
            .map(str::to_owned),
        );
        assert_eq!(
            config.portal.tour_bench,
            Some(PathBuf::from("out/bench.csv"))
        );
        assert_eq!(config.portal.tour_doors, Some(2));
        assert_eq!(EngineConfig::default().portal.tour_bench, None);
    }

    #[test]
    fn a_portal_bench_flag_names_the_doors_and_makes_an_unsynced_looked_at_run() {
        let config = EngineConfig::run_from_args(
            [
                "--load-doors",
                "seamless",
                "--portal-bench",
                "tools/bench/portal_bench_doors.json",
                "--bench-out",
                "local/bench/run.csv",
                "--walk",
            ]
            .map(str::to_owned),
        );
        assert_eq!(
            config.portal.portal_bench,
            Some(PathBuf::from("tools/bench/portal_bench_doors.json"))
        );
        assert_eq!(
            config.portal.bench_out,
            Some(PathBuf::from("local/bench/run.csv"))
        );
        // It draws the portal and the lights, it opens on screen to be watched (the user,
        // 2026-09-26), it poses the camera itself rather than walking, and it is timed unsynced.
        assert!(config.interactive());
        assert!(!config.window_offscreen());
        assert!(!config.walks());
        assert!(config.times_frames());
        let plain = EngineConfig::default();
        assert_eq!(plain.portal.portal_bench, None);
        assert!(!plain.times_frames());
    }

    #[test]
    fn graphics_cycle_takes_a_positive_period_only() {
        let period = |args: &[&str]| {
            EngineConfig::run_from_args(args.iter().map(|arg| (*arg).to_owned()))
                .portal
                .graphics_cycle
        };
        assert_eq!(period(&[]), None);
        assert_eq!(period(&["--graphics-cycle", "0.5"]), Some(0.5));
        assert_eq!(period(&["--graphics-cycle", "0"]), None);
        assert_eq!(period(&["--graphics-cycle", "-1"]), None);
        assert_eq!(period(&["--graphics-cycle", "soon"]), None);
    }

    #[test]
    fn parses_remote_options() {
        let config = EngineConfig::run_from_args(["--remote"].map(str::to_owned));
        assert!(config.remote);
        assert_eq!(config.remote_port, DEFAULT_REMOTE_PORT);

        let config =
            EngineConfig::run_from_args(["--remote", "--remote-port", "16000"].map(str::to_owned));
        assert!(config.remote);
        assert_eq!(config.remote_port, 16_000);

        // A port on its own asks for the server too rather than being silently ignored.
        let config = EngineConfig::run_from_args(["--remote-port", "16001"].map(str::to_owned));
        assert!(config.remote);
        assert_eq!(config.remote_port, 16_001);
    }

    #[test]
    fn reflection_options_preserve_upstream_prepass_and_all_cascades() {
        let options = EngineConfig::run_from_args(Vec::<String>::new()).reflection;
        assert_eq!(options, ReflectionOptions::default());
        assert!(options.prepass);
        assert!(options.occlusion);
        assert!(options.occlusion_culling());
        assert_eq!(options.cascades, None);
        assert_eq!(options.scale, 1.0);
        assert_eq!(options.target_size(), REFLECTION_SIZE);
    }

    #[test]
    fn explicit_full_reflection_flags_preserve_the_default() {
        let parse = |args: &[&str]| {
            EngineConfig::run_from_args(args.iter().map(|arg| (*arg).to_owned())).reflection
        };
        for cascades in ["all", "4"] {
            let options = parse(&["--reflection-prepass=on", "--reflection-cascades", cascades]);
            assert!(options.prepass);
            assert!(options.occlusion_culling());
            assert_eq!(options.cascades, None, "--reflection-cascades {cascades}");
            assert_eq!(options.target_size(), REFLECTION_SIZE);
        }
    }

    #[test]
    fn parses_the_reflection_cost_flags() {
        let parse = |args: &[&str]| {
            EngineConfig::run_from_args(args.iter().map(|arg| (*arg).to_owned())).reflection
        };
        let options = parse(&[
            "--reflection-occlusion=off",
            "--reflection-cascades",
            "1",
            "--reflection-scale",
            "0.5",
        ]);
        assert!(options.prepass);
        assert!(!options.occlusion_culling());
        assert_eq!(options.cascades, Some(1));
        assert_eq!(options.target_size(), (512, 288));
        assert_eq!(parse(&["--reflection-cascades", "2"]).cascades, Some(2));
        assert_eq!(
            parse(&["--reflection-cascades=2"]).cascades,
            Some(2),
            "the value may follow the flag after `=`"
        );
        assert_eq!(parse(&["--reflection-scale=0.5"]).target_size(), (512, 288));
        assert_eq!(
            parse(&["--reflection-scale", "0.001"]).target_size(),
            (16, 16),
            "a tiny scale keeps a usable target"
        );
    }

    /// Every reflection flag with a value that is not one stops the run with a message naming the
    /// flag, the value and what is accepted, instead of leaving the option at its default
    /// (review-258 item 2; `parses_the_reflection_cost_flags` used to assert the swallowing).
    #[test]
    fn a_reflection_flag_value_that_is_not_one_is_refused() {
        let refused = |args: &[&str]| {
            EngineConfig::from_args(args.iter().map(|arg| (*arg).to_owned()))
                .err()
                .map(|error| error.to_string())
                .unwrap_or_else(|| panic!("{args:?} is refused, but it parsed"))
        };
        assert_eq!(
            refused(&["--reflection-cascades", "5"]),
            "--reflection-cascades \"5\": 0 to 4 (Bevy 0.19's limit), or all"
        );
        assert_eq!(
            refused(&["--reflection-cascades", "two"]),
            "--reflection-cascades \"two\": 0 to 4 (Bevy 0.19's limit), or all"
        );
        assert_eq!(
            refused(&["--reflection-cascades", "-1"]),
            "--reflection-cascades \"-1\": 0 to 4 (Bevy 0.19's limit), or all"
        );
        assert_eq!(
            refused(&["--reflection-cascades"]),
            "--reflection-cascades: the value is missing; valid: 0 to 4 (Bevy 0.19's limit), or all"
        );
        assert_eq!(
            refused(&["--reflection-cascades=5"]),
            "--reflection-cascades=5: 0 to 4 (Bevy 0.19's limit), or all"
        );
        assert_eq!(
            refused(&["--reflection-scale", "0"]),
            "--reflection-scale \"0\": a fraction above 0, at most 1"
        );
        assert_eq!(
            refused(&["--reflection-scale", "1.5"]),
            "--reflection-scale \"1.5\": a fraction above 0, at most 1"
        );
        assert_eq!(
            refused(&["--reflection-scale", "wide"]),
            "--reflection-scale \"wide\": a fraction above 0, at most 1"
        );
        assert_eq!(
            refused(&["--reflection-scale"]),
            "--reflection-scale: the value is missing; valid: a fraction above 0, at most 1"
        );
        assert_eq!(
            refused(&["--reflection-prepass"]),
            "--reflection-prepass: valid: --reflection-prepass=on, --reflection-prepass=off"
        );
        assert_eq!(
            refused(&["--reflection-prepass=maybe"]),
            "--reflection-prepass=maybe: valid: --reflection-prepass=on, --reflection-prepass=off"
        );
        assert_eq!(
            refused(&["--reflection-occlusion"]),
            "--reflection-occlusion: valid: --reflection-occlusion=on, --reflection-occlusion=off"
        );
        assert_eq!(
            refused(&["--reflection-occlusion=yes"]),
            "--reflection-occlusion=yes: valid: --reflection-occlusion=on, \
             --reflection-occlusion=off"
        );
        assert_eq!(
            refused(&["--reflection-prepass=on", "--reflection-scale", "2"]),
            "--reflection-scale \"2\": a fraction above 0, at most 1",
            "the run stops on the bad flag whatever good ones came before it"
        );
    }

    /// `from_args` is the parse the run uses (`from_env`, `main`): a bad value has to stop it with
    /// the message, not hand back a configuration that quietly differs from the one asked for.
    #[test]
    fn from_args_stops_the_run_on_a_bad_reflection_value() {
        let error = EngineConfig::from_args(["--reflection-cascades", "5"].map(str::to_owned))
            .expect_err("a bad reflection value stops the run");
        assert_eq!(
            error.to_string(),
            "--reflection-cascades \"5\": 0 to 4 (Bevy 0.19's limit), or all"
        );
    }

    #[test]
    fn the_reflection_prepass_off_takes_occlusion_culling_with_it() {
        let options = EngineConfig::run_from_args(
            ["--reflection-prepass=on", "--reflection-prepass=off"].map(str::to_owned),
        )
        .reflection;
        assert!(!options.prepass);
        assert!(options.occlusion, "the flag itself is left as given");
        assert!(!options.occlusion_culling());
    }
}
