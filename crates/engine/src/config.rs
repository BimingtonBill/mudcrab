use crate::{skyrim_ini::SkyrimIni, world::components::CELL_SIZE};
use bevy::prelude::Resource;
use shared::lod::LodTier;
use std::{fmt, path::PathBuf};

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

/// Bevy Remote Protocol's default HTTP port, used by `--remote` unless `--remote-port` is given.
pub const DEFAULT_REMOTE_PORT: u16 = 15702;

#[derive(Debug, Clone, Resource)]
pub struct EngineConfig {
    pub assets_dir: PathBuf,
    pub worldspace_id: u32,
    pub start_grid: (i32, i32),
    pub stream_radius: i32,
    pub unload_radius: i32,
    pub terrain_lod: TerrainLodDistances,
    /// How far the terrain-only ring reaches, in cells from the camera cell: a cell outside
    /// [`stream_radius`](Self::stream_radius) but within this distance streams its landscape and its
    /// water plane and nothing else, so the world does not end at the full-detail edge.
    ///
    /// A value at or below `stream_radius` leaves the ring empty, which is the engine without a
    /// ring (`--stream-radius 2` with the default) and what a benchmark run that wants the old
    /// numbers passes. Interior cells are never terrain-only.
    pub terrain_radius: i32,
    pub max_cell_commits_per_frame: usize,
    pub max_commit_micros_per_frame: u64,
    /// Cells outside the unload radius a frame may despawn. `0` despawns every one at once, which
    /// is the unbudgeted behaviour unloading used to have.
    pub max_cell_unloads_per_frame: usize,
    /// Converted models a frame may hand to Bevy's scene spawner. `0` arms every model whose asset
    /// is loaded, which is the unbudgeted behaviour a single spawn batch used to have.
    pub max_model_spawns_per_frame: usize,
    /// MiB of newly loaded render assets (meshes, textures) the renderer may
    /// prepare per frame. `0` prepares every asset the frame extracted.
    pub max_upload_mib_per_frame: usize,
    pub headless: bool,
    pub benchmark_only: bool,
    pub benchmark_frames: Option<u32>,
    pub benchmark_duration_secs: Option<f64>,
    pub benchmark_warmup_frames: u32,
    pub benchmark_output: PathBuf,
    /// Where to write every measured frame time, in order, as CSV (`--benchmark-frame-times`).
    /// Off by default: the report's summary is what acceptance reads; the series is for choosing
    /// run lengths and spotting drift within a run.
    pub benchmark_frame_times: Option<PathBuf>,
    /// `--run-label <text>`: names an automated run in its window title, e.g. a benchmark's
    /// variant and round ([`EngineConfig::window_title`]).
    pub run_label: Option<String>,
    pub accept_min_fps: f64,
    pub accept_p95_ms: f64,
    pub accept_max_memory_growth_gib: f64,
    pub auto_fly_speed: f32,
    pub allow_incomplete_assets: bool,
    pub synthetic_instances: usize,
    pub profile_output_dir: Option<PathBuf>,
    pub profile_scenario: String,
    pub profile_run_id: String,
    pub profile_commit: String,
    pub profile_dirty_worktree: bool,
    pub profile_hardware: String,
    pub acceptance_screenshot: Option<PathBuf>,
    /// The water reflection camera's cost options ([`ReflectionOptions`]).
    pub reflection: ReflectionOptions,
    pub screenshot_camera_offset: Option<(f32, f32, f32)>,
    /// `--shots <file>`: render the camera poses in a shots file, one PNG each, and exit instead of
    /// running interactively. See [`crate::shots`].
    pub shots: Option<PathBuf>,
    /// `--shots-out <dir>`: where the PNGs and `shots.log` go. `None` is
    /// [`crate::shots::default_output_dir`], a `<file stem>-shots/` folder beside the shots file.
    pub shots_out: Option<PathBuf>,
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
    pub physics_fixture: bool,
}

/// The default reach of the terrain-only ring, in cells: eight cells is 32,768 units of landscape
/// beyond the full-detail grid, drawn from 264 cells.
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
    fn default() -> Self {
        Self {
            assets_dir: PathBuf::from("modern_assets"),
            worldspace_id: 0x3c,
            start_grid: (0, 0),
            // Skyrim's default `uGridsToLoad=5`.
            stream_radius: 2,
            unload_radius: 3,
            terrain_lod: TerrainLodDistances::default(),
            terrain_radius: DEFAULT_TERRAIN_RADIUS,
            max_cell_commits_per_frame: 1,
            max_commit_micros_per_frame: 16_670,
            max_cell_unloads_per_frame: 2,
            // A cell holds on the order of 15 references with a model and cells commit one per
            // frame, so 15 models is the largest batch a frame can be handed at once. Bevy
            // instantiates a batch like that in an estimated 5-8 ms on the stress scenario, which is
            // most of a 60 fps frame; arming 4 per frame keeps a batch near 1.5 ms and a whole
            // cell's models armed within four frames (~67 ms at 60 fps). `0` arms the batch whole,
            // as the engine did before this budget existed.
            max_model_spawns_per_frame: 4,
            // Three 2K BC7/UASTC textures with a full mip chain (~5.3 MiB each):
            // a cell's new textures spread over a few frames instead of landing
            // in one 13 ms upload burst, and at 60 fps the budget still admits
            // far more new assets than the streaming radius can produce.
            max_upload_mib_per_frame: 16,
            headless: false,
            benchmark_only: false,
            benchmark_frames: None,
            benchmark_duration_secs: None,
            benchmark_warmup_frames: 60,
            benchmark_output: PathBuf::from("benchmark-report.json"),
            benchmark_frame_times: None,
            run_label: None,
            accept_min_fps: 60.0,
            accept_p95_ms: 16.67,
            accept_max_memory_growth_gib: 0.5,
            auto_fly_speed: 0.0,
            allow_incomplete_assets: false,
            synthetic_instances: 250_000,
            profile_output_dir: None,
            profile_scenario: "adhoc".into(),
            profile_run_id: "run-1".into(),
            profile_commit: "unknown".into(),
            profile_dirty_worktree: false,
            profile_hardware: "unspecified".into(),
            acceptance_screenshot: None,
            reflection: ReflectionOptions::default(),
            screenshot_camera_offset: None,
            shots: None,
            shots_out: None,
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
            physics_fixture: false,
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
  --allow-incomplete-assets             run despite a failed manifest or integration-report gate
  --headless                            run without opening a window
  --lights                              place a point light for each streamed LIGH reference

Streaming:
  --ini <file>                        layer Skyrim INI settings; CLI options override files
  --stream-radius <cells>               cells streamed around the camera (default: 2)
  --max-commit-ms <ms>                  cell commit time allowed per frame (default: 16.67)
  --max-unloads-per-frame <count>       cells despawned per frame; 0 despawns all at once (default: 2)
  --max-model-spawns-per-frame <count>  models spawned per frame; 0 spawns all at once (default: 4)
  --max-upload-mib-per-frame <mib>      render-asset upload budget per frame; 0 is unlimited (default: 16)
  --auto-fly-speed <units/s>            fly the camera forward at this speed; 0 holds it still
  --terrain-radius <cells>              terrain-only ring around the streamed cells (default: 8)

Benchmark and profiling:
  --benchmark-only                      run the synthetic benchmark; opens no world database
  --benchmark-frames <count>            stop after this many measured frames
  --benchmark-duration <seconds>        stop after this many measured seconds
  --benchmark-warmup-frames <count>     frames discarded before measuring (default: 60)
  --benchmark-output <file>             benchmark report path (default: benchmark-report.json)
  --benchmark-frame-times [<file>]      write every measured frame time to this CSV file
  --run-label [<text>]                  name the run in the window title
  --synthetic-instances <count>         instances in the synthetic benchmark (default: 250000)
  --accept-min-fps <fps>                fail the run below this average frame rate (default: 60)
  --accept-p95-ms <ms>                  fail the run above this p95 frame time (default: 16.67)
  --accept-max-memory-growth-gib <gib>  fail the run above this memory growth (default: 0.5)
  --acceptance-screenshot <file>        write a screenshot when the run ends
  --screenshot-camera-offset <x,y,z>    camera offset for the acceptance screenshot
  --shots <file>                        render the camera poses in a shots file, one PNG each, then exit
  --shots-out <dir>                     where the shots' PNGs and shots.log go (default: beside the shots file)
  --profile-output <dir>                profile bundle directory (default: no bundle)
  --profile-scenario <name>             scenario name recorded in the profile (default: adhoc)
  --profile-run-id <id>                 run id recorded in the profile (default: run-1)
  --profile-commit <hash>               commit recorded in the profile (default: unknown)
  --profile-dirty-worktree              record uncommitted worktree changes in the profile
  --profile-hardware <text>             hardware recorded in the profile (default: unspecified)

Diagnostics:
  --diagnostic-asset-fallbacks          log every fallback asset substitution
  --material-fixture                    run the material fixture (no world database)
  --terrain-water-fixture               run the terrain-water fixture (no world database)
  --transform-bounds-fixture            run the transform-bounds fixture (no world database)
  --renderer-fixture                    run the renderer fixture (no world database)
  --physics-fixture                     run the physics fixture (no world database)
  --streaming-fixture                   stream the fixture world instead of the full asset set

Remote:
  --remote                              serve the Bevy Remote Protocol on 127.0.0.1 (needs the remote feature)
  --remote-port <port>                  port for --remote (default: 15702); implies --remote

Portal (this branch's doors, demos and tools):
  --walk                                walk as a first-person player; E opens load doors
  --show-window                         keep an automated run's window on screen
  --portal-depth-composite              composite the portal by depth (the default)
  --portal-depth-composite=on           the same as --portal-depth-composite
  --portal-depth-composite=off          draw the portal without depth compositing
  --demo <alftand|blackreach|riverwood> start at a named demo position
  --start-position <x> <y> <z>          start at a Creation-unit position
  --start-yaw <radians>                 start heading, with --start-position
  --start-shot <file> <name>            start at a shot's pose, in its space
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
    /// The per-frame render-asset upload budget in bytes, or `None` when
    /// uploads are unlimited (`--max-upload-mib-per-frame 0`).
    pub fn max_upload_bytes_per_frame(&self) -> Option<usize> {
        (self.max_upload_mib_per_frame != 0)
            .then(|| self.max_upload_mib_per_frame.saturating_mul(1024 * 1024))
    }

    /// Interactive exterior play owns the mouse/controller; automated camera paths keep their
    /// existing movement and framing.
    pub fn interactive_world_physics(&self) -> bool {
        !self.headless
            && !self.benchmark_only
            && self.benchmark_frames.is_none()
            && self.benchmark_duration_secs.is_none()
            && self.acceptance_screenshot.is_none()
            && self.shots.is_none()
            && self.auto_fly_speed <= 0.0
            && !self.material_fixture
            && !self.terrain_water_fixture
            && !self.transform_bounds_fixture
            && !self.renderer_fixture
            && !self.streaming_fixture
            && !self.physics_fixture
    }

    pub fn from_env() -> Result<ConfigAction, ConfigError> {
        Self::from_args(std::env::args().skip(1))
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
        let mut terrain_radius_given = false;
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
                        terrain_radius_given = true;
                    }
                }
                "--headless" => config.headless = true,
                "--max-unloads-per-frame" => {
                    config.max_cell_unloads_per_frame = take_value(
                        "--max-unloads-per-frame",
                        "a cell count, 0 for unlimited",
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
                "--max-upload-mib-per-frame" => {
                    config.max_upload_mib_per_frame = take_value(
                        "--max-upload-mib-per-frame",
                        "a size in MiB, 0 for unlimited",
                        args.next(),
                    )?;
                }
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
                "--profile-dirty-worktree" => config.profile_dirty_worktree = true,
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
                "--shots" => {
                    config.shots = Some(take_value("--shots", "a shots file path", args.next())?);
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
                "--streaming-fixture" => config.streaming_fixture = true,
                "--remote" => config.remote = true,
                "--remote-port" => {
                    config.remote = true;
                    if let Some(value) = args.next().and_then(|value| value.parse().ok()) {
                        config.remote_port = value;
                    }
                }
                "--lights" => config.lights = true,
                "--physics-fixture" => config.physics_fixture = true,
                unknown => return Err(ConfigError::unrecognized(unknown)),
            }
        }
        // Benchmark and acceptance runs measure the full-detail grid the Phase 2 gates were set
        // for, so the terrain ring is off there unless a run asks for it by name.
        // `--shots` and `--shots-out` are main's options (parsed above, with main's checks); the
        // portal's shots run reads them from its own options.
        config.portal.shots = config.shots.clone();
        config.portal.shots_out = config.shots_out.clone();
        if !terrain_radius_given
            && (config.benchmark_frames.is_some() || config.benchmark_duration_secs.is_some())
        {
            config.terrain_radius = config.stream_radius;
        }
        Ok(ConfigAction::Run(Box::new(config)))
    }
}

/// The widest `--stream-radius` the engine takes: the unload ring is one cell wider, and the
/// streamer measures that ring as `2 * unload_radius + 1` cells across in an `i32`, which this
/// keeps from overflowing.
pub const MAX_STREAM_RADIUS: i32 = (i32::MAX - 3) / 2;

/// What `--stream-radius` takes, with [`MAX_STREAM_RADIUS`] spelled out; a test holds the two
/// equal.
const STREAM_RADIUS_EXPECTED: &str = "a cell count from 0 to 1073741822";

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
    /// `--graphics <preset>`: `current` (the default), `bevy` or `custom`
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
/// Both fields are optional because the flag's own parser cannot fail: `--start-shot examples.json`
/// with no name after it, and `--start-shot` at the end of the line, are requests nothing can carry
/// out. They are refused by name before the window exists, in
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
            // Both of the flag's arguments are taken, and either may be missing: what a request
            // that names no file, or no shot, is told is `pose_capture::start_shot_run`'s, which
            // is the first place that can say so with a message.
            "--start-shot" => {
                config.portal.start_shot = Some(StartShot {
                    file: args.next().map(PathBuf::from),
                    name: args.next(),
                });
            }
            "--demo" => {
                if let Some(name) = args.next()
                    && let Some(demo) = DemoStart::named(&name)
                {
                    demo.apply(config);
                    config.portal.demo = Some(name);
                }
            }
            "--start-position" => {
                let values: Vec<f32> = (0..3)
                    .filter_map(|_| args.next().and_then(|value| value.parse().ok()))
                    .collect();
                if let [x, y, z] = values[..] {
                    config.start_position = Some([x, y, z]);
                    config.start_grid = grid_of(x, y);
                }
            }
            "--start-yaw" => {
                if let Some(value) = args.next().and_then(|value| value.parse().ok()) {
                    config.start_yaw = value;
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
    /// one named after the shots file, next to it. `None` when there is no shots run.
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

    /// The window's title: what kind of automated run this is, and its `--run-label`, so a run on
    /// the taskbar says what it is (the user asked, 2026-09-26). An interactive run is plain
    /// "Mudcrab".
    pub fn window_title(&self) -> String {
        let kind = if self.portal.portal_bench.is_some() {
            Some("portal bench")
        } else if self.portal.tour_bench.is_some() {
            Some("tour bench")
        } else if self.benchmark_frames.is_some() || self.benchmark_duration_secs.is_some() {
            Some("benchmark")
        } else if self.portal.demo_tour.is_some() {
            Some("demo tour")
        } else if self.portal.shots.is_some() {
            Some("shots")
        } else if self.streaming_fixture {
            Some("streaming fixture")
        } else {
            None
        };
        match (kind, self.run_label.as_deref()) {
            (Some(kind), Some(label)) => format!("Mudcrab - {kind}: {label}"),
            (Some(kind), None) => format!("Mudcrab - {kind}"),
            (None, Some(label)) => format!("Mudcrab - {label}"),
            (None, None) => "Mudcrab".to_string(),
        }
    }

    /// A run that walks: `--walk` with the camera left to the player's own controller. A
    /// benchmark, an auto-flight run, a `--shots` run and a `--start-shot` run keep the scripted
    /// camera whatever `--walk` was given, which is the rule `app::run` has always applied.
    pub fn walks(&self) -> bool {
        // A `--start-shot` run is driven by the player's own controller too - it starts in free
        // flight at the shot's pose (`crate::pose_capture`) - because the scripted fly camera has
        // no mouse look, and the person lining up a shot has to turn as well as move.
        (self.portal.walk || self.portal.start_shot.is_some())
            && self.benchmark_frames.is_none()
            && self.benchmark_duration_secs.is_none()
            && self.auto_fly_speed <= 0.0
            && self.portal.shots.is_none()
            && self.portal.portal_bench.is_none()
    }

    /// A run that is looked at rather than measured: sky and underground lighting, portals and
    /// lights, and no acceptance capture.
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
/// The default is the cheaper reflection (the user's decision, 2026-09-26): no depth prepass (so no
/// occlusion culling) and no shadow cascades, which cut the render thread 1.0-1.4 ms at Riften's
/// water door (`local/bench/portal-bench-2026-09-26_16-42-33`). The full reflection as it was
/// before comes back with `--reflection-prepass=on --reflection-cascades all`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReflectionOptions {
    /// Whether the reflection camera has a `DepthPrepass`: off by default, `--reflection-prepass=on`
    /// opts back in. Bevy's occlusion culling needs the prepass, so without it
    /// [`occlusion`](Self::occlusion) has no effect.
    pub prepass: bool,
    /// `--reflection-occlusion=off`: no `OcclusionCulling` on the reflection camera even when it
    /// has its depth prepass (on by default, so `--reflection-prepass=on` brings occlusion culling
    /// back with it).
    pub occlusion: bool,
    /// `--reflection-cascades N` (0 to [`MAX_REFLECTION_CASCADES`], or `all`): the reflection view
    /// keeps only the first `N` shadow cascades of the sun; 0 (the default) draws the reflection
    /// unshadowed. `None` (`all`, or `N` = [`MAX_REFLECTION_CASCADES`]) keeps every cascade the sun
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
            prepass: false,
            occlusion: true,
            cascades: Some(0),
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
        let side = |full: u32| ((full as f32 * self.scale).round() as u32).max(MIN_REFLECTION_SIDE);
        (side(REFLECTION_SIZE.0), side(REFLECTION_SIZE.1))
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
    use super::*;

    /// This file, so the tests can read the parser's own match arms back out of
    /// it instead of keeping a second list of option names in step by hand.
    const CONFIG_SOURCE: &str = include_str!("config.rs");

    /// Cargo, Git, `world-inspect`, dynamic-loader and audit-tool flags the
    /// scripts also spell on a command line. Every other `--flag` in those
    /// scripts goes to the engine. None of the audit tools starts the engine,
    /// so their own options must not be mistaken for engine options.
    const NON_ENGINE_FLAGS: &[&str] = &[
        "--all",                 // cargo fmt
        "--all-targets",         // cargo test, cargo clippy
        "--bin",                 // cargo test
        "--bins",                // cargo build
        "--check",               // cargo fmt
        "--release",             // cargo build
        "--workspace",           // cargo build, cargo test, cargo clippy
        "--ignore-submodules",   // git diff
        "--quiet",               // git diff
        "--short",               // git rev-parse
        "--output",              // world-inspect
        "--radius",              // world-inspect
        "--library-path",        // ld-linux
        "--meshes",              // audit-collision.py
        "--min-x",               // audit-collision.py
        "--max-x",               // audit-collision.py
        "--min-y",               // audit-collision.py
        "--max-y",               // audit-collision.py
        "--out",                 // audit-collision.py
        "--expect-solid",        // audit-collision.py
        "--expect-passable",     // audit-collision.py
        "--original",            // audit_asset_sizes.py
        "--json",                // audit_asset_sizes.py
        "--candidate-inventory", // audit-riverwood-reuse.py
        "--reference-inventory", // audit-riverwood-reuse.py
        "--manifest",            // audit-riverwood-reuse.py
        "--locked",              // cargo run
        "--manifest-path",       // cargo run
        "--cpu-jobs",            // converter
        "--io-jobs",             // converter
        "--binary",              // git diff
        "--porcelain",           // git status
    ];

    fn run_config(arguments: &[&str]) -> EngineConfig {
        EngineConfig::run_from_args(arguments.iter().map(|argument| (*argument).to_owned()))
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
        assert_eq!(config.max_model_spawns_per_frame, 4);
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
    fn a_negative_budget_is_refused() {
        for option in [
            "--max-unloads-per-frame",
            "--max-model-spawns-per-frame",
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
            "Mudcrab - benchmark: main rural r1"
        );
        assert_eq!(
            args(&["--benchmark-frames", "600"]).window_title(),
            "Mudcrab - benchmark"
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
            "Mudcrab - streaming fixture"
        );
        assert_eq!(
            args(&["--shots", "poses.json", "--run-label", "riverwood"]).window_title(),
            "Mudcrab - shots: riverwood"
        );
        assert_eq!(args(&[]).window_title(), "Mudcrab");
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
                    // Tests inspect script text, including deliberately unsupported engine
                    // options; they are not utility command lines.
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

    /// Every utility `.ps1`, `.sh` and `.py` file in `scripts/`, subdirectories
    /// included, is read from disk, so a script added later cannot pass an
    /// option the parser refuses without failing this test. The scan is skipped
    /// when the repository's `scripts/` is not next to this crate.
    #[test]
    fn the_scripts_only_pass_options_the_parser_accepts() {
        let options = parser_options();
        let Some(scripts) = scripts_directory() else {
            return;
        };
        let mut checked = 0;
        for path in script_paths(&scripts) {
            let script = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
            for flag in option_tokens(&script) {
                if NON_ENGINE_FLAGS.contains(&flag) {
                    continue;
                }
                assert!(
                    options.contains(&flag),
                    "{} passes {flag}, which the parser refuses",
                    path.display()
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 20,
            "the script scan found {checked} engine flags; the scripts moved or changed shape"
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
    fn script_v89_scan_excludes_tests_and_keeps_nested_utilities() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("tests")).unwrap();
        std::fs::create_dir(directory.path().join("utils")).unwrap();
        std::fs::write(directory.path().join("tests/check.py"), "--log-file").unwrap();
        let utility = directory.path().join("utils/capture.sh");
        std::fs::write(&utility, "--headless").unwrap();
        assert_eq!(script_paths(directory.path()), vec![utility]);
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
            Some(PathBuf::from("shots/tamriel")),
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

        // Either of the flag's two arguments may be absent, and a request nothing can carry out is
        // refused where a message can be written rather than by the parser
        // (`pose_capture::start_shot_run`).
        assert_eq!(
            EngineConfig::run_from_args(["--start-shot", "a.json"].map(str::to_owned))
                .portal
                .start_shot,
            Some(StartShot {
                file: Some(PathBuf::from("a.json")),
                name: None
            })
        );
        assert_eq!(
            EngineConfig::run_from_args(["--start-shot"].map(str::to_owned))
                .portal
                .start_shot,
            Some(StartShot {
                file: None,
                name: None
            })
        );
        assert_eq!(EngineConfig::default().portal.start_shot, None);
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
            "--portal-bench",
            "doors.json",
            "--run-label",
            "default r1",
            "--bench-seconds",
            "2",
        ]);
        assert!(!bench.window_offscreen(), "a bench is watched");
        assert_eq!(bench.window_title(), "Mudcrab - portal bench: default r1");
        assert_eq!(bench.portal.bench_seconds, Some(2.0));
        assert!(!args(&["--demo-tour", "t", "--tour-bench", "b"]).window_offscreen());
        assert_eq!(
            args(&["--demo-tour", "t"]).window_title(),
            "Mudcrab - demo tour"
        );
        assert_eq!(
            args(&["--shots", "s.json"]).window_title(),
            "Mudcrab - shots"
        );
        assert_eq!(args(&["--walk"]).window_title(), "Mudcrab");
        assert_eq!(
            args(&["--portal-bench", "d", "--bench-seconds", "-1"])
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

    #[test]
    fn a_tour_bench_flag_names_the_csv_to_write() {
        let config = EngineConfig::run_from_args(
            [
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
    fn reflection_options_default_to_no_prepass_and_no_cascades() {
        let options = EngineConfig::run_from_args(Vec::<String>::new()).reflection;
        assert_eq!(options, ReflectionOptions::default());
        assert!(!options.prepass);
        assert!(!options.occlusion_culling());
        assert_eq!(options.cascades, Some(0));
        assert_eq!(options.target_size(), REFLECTION_SIZE);
    }

    #[test]
    fn the_opt_back_in_flags_give_the_full_reflection() {
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
        assert!(!options.prepass);
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
