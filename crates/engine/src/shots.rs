//! Reference shots: `engine.exe --assets <dir> --shots <file.json> [--shots-out <dir>]` renders
//! each camera pose in the file to `<out>/<name>.png` and exits.
//!
//! The file format is the contract in `docs/design/reference-shots.md`. Angles are degrees and
//! positions are Creation-engine units, the coordinates `skyrim_world.db` stores, so a pose can be
//! read off the database and edited by hand. The run renders the same view as a reference
//! screenshot of the real game, so the pose, the field of view and the aspect have to match the
//! reference rather than merely look plausible: the camera sits at `position` exactly (no eye
//! height), yaw uses the same clockwise heading as [`shot_camera_rotation`], and the vertical
//! field of view is derived from `hfov` at the file's aspect.
//!
//! For each shot the camera moves into the shot's space exactly as a door crossing does, the run
//! waits until streaming has nothing pending **for that view**, and only then is the screenshot
//! requested. The app exits once the last image is on disk, not when it was requested.
//!

use crate::{
    cluster::package::PackageValidationStatus,
    render::RendererMetrics,
    streaming::{ActiveSpace, RenderOrigin, StreamingMetrics, StreamingWorld},
    swap_bridge::{SwapInCameraView, SwapWaiting, pending_drawn},
    world::{
        components::{CELL_SIZE, StreamingCamera},
        database::CellKey,
    },
};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::{ecs::system::SystemParam, prelude::*, window::WindowResolution};
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashSet},
    fmt, fs,
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// The steepest pitch a shot may ask for, in degrees, negative looking up: Skyrim's player X angle
/// stops just short of straight down and straight up.
pub const MIN_PITCH_DEGREES: f32 = -89.0;
pub const MAX_PITCH_DEGREES: f32 = 89.0;

/// The narrowest and widest horizontal field of view a shot may ask for, in degrees.
pub const MIN_HFOV_DEGREES: f32 = 10.0;
pub const MAX_HFOV_DEGREES: f32 = 170.0;

/// How long a view may take to settle before the shot fails without a capture.
pub const SETTLE_TIMEOUT_SECONDS: f32 = 30.0;

/// Requests a one-shot capture of the primary window into `path`, the same mechanism a `--shots`
/// run photographs a pose with: the [`Screenshot`] entity's observer writes the file after that
/// frame renders. Used by the retire-visibility probe (`crate::streaming`) to photograph a frame
/// that is draining retiring cells - a small hook so the probe reuses this capture path rather
/// than a second one.
pub(crate) fn capture_primary_window(commands: &mut Commands, path: PathBuf) {
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

/// Frames of a quiet streaming state after which the view counts as settled, on top of the quiet
/// state itself, so lights, shadows and the first drawn frame have caught up.
pub const SETTLE_QUIET_FRAMES: u32 = 10;

/// Seconds after start-up before any shot is taken, as for the acceptance screenshot: the first
/// frames compile pipelines, and a mesh whose pipeline is not ready yet is simply not drawn.
pub const WARM_UP_SECONDS: f32 = 2.0;

/// How long to wait for a requested screenshot to reach the disk.
pub const CAPTURE_TIMEOUT_SECONDS: f32 = 60.0;

/// Frames a run waits for the streaming camera to exist before it gives up: without one no shot
/// can be posed, and the run would otherwise never end.
pub const MISSING_CAMERA_FRAMES: u32 = 600;

/// The longest side of a shot's frame, in pixels.
///
/// The window is opened at the file's frame, and a swapchain wider or taller than the limits of
/// common hardware cannot be created: the run would fail after the window opened, with an error
/// from the renderer rather than from the file. The check is on each side, not the pixel count, so
/// a very wide frame is refused like a very tall one - and a frame at the cap on both sides is a
/// 8192x8192 (256 MB) framebuffer encoded on the main thread.
pub const MAX_FRAME_SIDE_PIXELS: u32 = 8192;

/// A shots file: the window size every shot is rendered at, and the shots in order.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ShotsFile {
    /// Window width in pixels, and the width of every PNG.
    pub width: u32,
    /// Window height in pixels, and the height of every PNG.
    pub height: u32,
    pub shots: Vec<Shot>,
}

/// One camera pose, rendered to `<out>/<name>.png`.
///
/// `reference` and `note` are carried for the comparison tools and humans; the engine ignores
/// them, as it ignores any field it does not know.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Shot {
    /// Output file stem: the engine writes `<out>/<name>.png`.
    pub name: String,
    /// The worldspace of an exterior shot, or `None` for an interior one.
    #[serde(default)]
    pub worldspace_id: Option<u32>,
    /// The interior cell of an interior shot, or `None` for an exterior one.
    #[serde(default)]
    pub interior_cell_id: Option<u32>,
    /// The camera's eye, Creation units, absolute (not relative to a cell or the render origin).
    pub position: [f32; 3],
    /// Skyrim heading in degrees: 0 looks north (Creation `+Y`), 90 looks east (`+X`), clockwise
    /// seen from above.
    pub yaw: f32,
    /// Skyrim player X angle in degrees: positive looks down, negative up.
    pub pitch: f32,
    /// Horizontal field of view in degrees at this file's aspect.
    pub hfov: f32,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

impl ShotsFile {
    /// Reads and validates a shots file. An unreadable, malformed or impossible file is an error:
    /// the engine exits non-zero rather than rendering something other than what was asked for.
    pub fn load(path: &Path) -> Result<Self, ShotsError> {
        let text = std::fs::read_to_string(path).map_err(|error| {
            ShotsError::new(format!("failed to read {}: {error}", path.display()))
        })?;
        Self::parse(&text).map_err(|error| ShotsError::new(format!("{}: {error}", path.display())))
    }

    /// Parses the strict authoring format: explicit spaces and player-angle limits.
    /// `parse` also accepts upstream files that inherit the configured exterior.
    pub fn from_json(text: &str) -> Result<Self, ShotsError> {
        let file: Self = serde_json::from_str(text)
            .map_err(|error| ShotsError::new(format!("not a shots file: {error}")))?;
        file.validate()?;
        for shot in &file.shots {
            if shot.worldspace_id.is_none() && shot.interior_cell_id.is_none() {
                return Err(ShotsError::new(
                    "set worldspace_id (an exterior) or interior_cell_id",
                ));
            }
            if !(MIN_PITCH_DEGREES..=MAX_PITCH_DEGREES).contains(&shot.pitch) {
                return Err(ShotsError::new(format!(
                    "pitch {} is outside {MIN_PITCH_DEGREES}..{MAX_PITCH_DEGREES} degrees",
                    shot.pitch
                )));
            }
            if !(MIN_HFOV_DEGREES..=MAX_HFOV_DEGREES).contains(&shot.hfov) {
                return Err(ShotsError::new(format!(
                    "hfov {} is outside {MIN_HFOV_DEGREES}..{MAX_HFOV_DEGREES} degrees",
                    shot.hfov
                )));
            }
        }
        Ok(file)
    }

    /// Parses the upstream format; an exterior may inherit the run's worldspace.
    pub fn parse(text: &str) -> Result<Self, ShotsError> {
        let file: Self = serde_json::from_str(text)
            .map_err(|error| ShotsError::new(format!("not a shots file: {error}")))?;
        file.validate()?;
        Ok(file)
    }

    /// Initial window size matching the requested image target. Display scaling may alter the
    /// window, while the dedicated capture target retains the exact requested dimensions.
    pub fn window_resolution(&self) -> WindowResolution {
        WindowResolution::new(self.width, self.height).with_scale_factor_override(1.0)
    }

    /// Where the run starts streaming: the worldspace the first exterior shot names (`None` leaves
    /// it to `--worldspace`) and the grid square its camera stands over. `None` when the file has
    /// no exterior shot.
    pub fn start(&self) -> Option<(Option<u32>, (i32, i32))> {
        let shot = self.shots.iter().find(|shot| shot.is_exterior())?;
        Some((shot.worldspace_id, shot.grid()))
    }

    pub fn validate(&self) -> Result<(), ShotsError> {
        if self.width == 0 || self.height == 0 {
            let frame = format!("{}x{}", self.width, self.height);
            let message = format!("the frame is {frame}: a screenshot of no pixels is not a shot");
            return Err(ShotsError::new(message));
        }
        if self.width > MAX_FRAME_SIDE_PIXELS || self.height > MAX_FRAME_SIDE_PIXELS {
            let frame = format!("{}x{}", self.width, self.height);
            let message = format!(
                "the frame is {frame}: a side longer than {MAX_FRAME_SIDE_PIXELS} pixels is more \
                 than the window can be"
            );
            return Err(ShotsError::new(message));
        }
        if self.shots.is_empty() {
            return Err(ShotsError::new("the file has no shots".to_owned()));
        }
        // Names are compared ignoring case: on Windows "Tower.png" and "tower.png" are one file.
        let mut names = HashSet::new();
        for shot in &self.shots {
            shot.validate()?;
            if !names.insert(shot.name.to_lowercase()) {
                let message = format!(
                    "two shots are named \"{}\" (ignoring case): the second image would replace \
                     the first",
                    shot.name
                );
                return Err(ShotsError::new(message));
            }
        }
        Ok(())
    }

    /// The aspect every shot in this file is rendered at (width / height).
    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height as f32
    }
}

impl Shot {
    /// Whether this is an exterior pose, which is what the streamer holds cells for.
    pub fn is_exterior(&self) -> bool {
        self.interior_cell_id.is_none()
    }

    /// The exterior grid square the camera stands over.
    pub fn grid(&self) -> (i32, i32) {
        let [x, y, _] = self.position;
        (
            (x / CELL_SIZE).floor() as i32,
            (y / CELL_SIZE).floor() as i32,
        )
    }

    /// The exact public streaming cell at the pose. An unnamed exterior is resolved by the run.
    pub fn space_key(&self) -> Option<CellKey> {
        match (self.interior_cell_id, self.worldspace_id) {
            (Some(id), _) => Some(CellKey::Interior(id)),
            (None, Some(worldspace_id)) => {
                let (grid_x, grid_y) = self.grid();
                Some(CellKey::Exterior {
                    worldspace_id,
                    grid_x,
                    grid_y,
                })
            }
            (None, None) => None,
        }
    }

    pub fn validate(&self) -> Result<(), ShotsError> {
        if self.worldspace_id.is_some() && self.interior_cell_id.is_some() {
            return Err(ShotsError::new(
                "set worldspace_id or interior_cell_id, not both",
            ));
        }
        let name = &self.name;
        if name.is_empty() {
            return Err(ShotsError::new(
                "a shot has no name to name its image".to_owned(),
            ));
        }
        // The name is a file stem inside the output folder, never a path out of it: the separators,
        // and the characters Windows refuses in a file name, are all malformed here, so that the
        // run stops before the window opens rather than at the shot's write.
        let plain = name.chars().all(|character| {
            !matches!(
                character,
                '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'
            ) && !character.is_control()
        }) && name != "."
            && name != "..";
        if !plain {
            let message = format!("shot \"{name}\" has a name that is not a plain file name");
            return Err(ShotsError::new(message));
        }
        // Windows cannot create a name that ends in a dot or a space: it strips the trailing
        // characters, so the image would be written under a name the file never asked for.
        if name.ends_with('.') || name.ends_with(' ') {
            let message = format!(
                "shot \"{name}\" has a name that ends in a dot or a space, which Windows cannot \
                 create as a file"
            );
            return Err(ShotsError::new(message));
        }
        if is_reserved_device_name(name) {
            let message = format!(
                "shot \"{name}\" is a reserved device name on Windows, which cannot name a file"
            );
            return Err(ShotsError::new(message));
        }
        let finite = self.position.iter().all(|value| value.is_finite())
            && self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.hfov.is_finite();
        if !finite {
            let message = format!("shot \"{name}\" has a pose that is not finite (not a number)");
            return Err(ShotsError::new(message));
        }
        if self.hfov <= 0.0 || self.hfov >= 180.0 {
            let hfov = self.hfov;
            let message = format!(
                "shot \"{name}\" asks for a horizontal field of view (hfov) of {hfov} degrees, \
                 which is not one between 0 and 180"
            );
            return Err(ShotsError::new(message));
        }
        // The pitch is a Skyrim player X angle: straight down is 90, straight up -90, and a value
        // outside that range is not a pose the game can hold.
        if self.pitch < -90.0 || self.pitch > 90.0 {
            let pitch = self.pitch;
            let message = format!(
                "shot \"{name}\" asks for a pitch of {pitch} degrees, which is not one between \
                 -90 and 90"
            );
            return Err(ShotsError::new(message));
        }
        Ok(())
    }
}

/// Whether a name is one of the DOS device names Windows refuses to create, whatever extension it
/// carries: `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9` and `LPT1`-`LPT9`, ignoring case. Windows
/// drops the extension before the check, so `con.png` and `Nul.x` are reserved like `NUL` is.
fn is_reserved_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let digit = stem.as_bytes().get(3).copied();
    stem.len() == 4
        && matches!(stem.get(..3), Some("COM" | "LPT"))
        && matches!(digit, Some(b'1'..=b'9'))
}

/// Why a shots file could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShotsError(pub(crate) String);

impl ShotsError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ShotsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ShotsError {}

/// Where the images and `shots.log` go when `--shots-out` is not given: a folder named after the
/// shots file, next to it (`reference/riverwood_shots.json` -> `reference/riverwood_shots-shots/`).
pub fn default_output_dir(shots_path: &Path) -> PathBuf {
    let stem = shots_path.file_stem().map_or_else(
        || "shots".into(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let directory = format!("{stem}-shots");
    match shots_path.parent() {
        Some(parent) => parent.join(directory),
        None => PathBuf::from(directory),
    }
}

/// The camera rotation for a Skyrim pose.
///
/// `yaw` is a Creation heading in degrees, measured clockwise from north (`+Y`) seen from above -
/// the same convention as [`shot_camera_rotation`] and a player's `GetAngle Z`. `pitch` is
/// Skyrim's player X angle, positive looking **down**. At yaw 0 / pitch 0 the camera looks along
/// Creation `+Y`; at yaw 90 it looks along `+X`; at pitch 30 its forward has a Creation `Z`
/// component of `-sin 30`.
///
pub fn shot_camera_rotation(yaw_degrees: f32, pitch_degrees: f32) -> Quat {
    // The yaw term uses the Creation heading convention, so a shot's camera
    // turns the same way an arrival camera or a door frame does; the pitch turns that same
    // forward down by `pitch` on top of it.
    Quat::from_rotation_y(-yaw_degrees.to_radians())
        * Quat::from_rotation_x(-pitch_degrees.to_radians())
}

/// The vertical field of view for a horizontal one at `aspect` (width / height), in degrees.
pub fn vertical_fov_degrees(hfov_degrees: f32, aspect: f32) -> f32 {
    let half_horizontal = (hfov_degrees.to_radians() * 0.5).tan();
    (2.0 * (half_horizontal / aspect).atan()).to_degrees()
}

/// Where a shot's camera stands in render space: the pose in Creation units converted to runtime
/// axes, measured from the floating origin, which is how the streamer places every cell.
pub fn shot_camera_translation(position: [f32; 3], origin: IVec2) -> Vec3 {
    let axes = shared::coordinates::creation_to_runtime_vector(position);
    let offset_x = origin.x as f32 * CELL_SIZE;
    let offset_z = -(origin.y as f32 * CELL_SIZE);
    Vec3::from_array(axes) - Vec3::new(offset_x, 0.0, offset_z)
}

/// Why a shot is passed over rather than rendered, or `None` when it can be rendered in a run that
/// streams `worldspace_id`. The streamer holds the exterior cells of one worldspace per run.
pub fn skip_reason(shot: &Shot, worldspace_id: u32) -> Option<String> {
    if !shot.is_exterior() {
        return Some("interior".to_owned());
    }
    match shot.worldspace_id {
        Some(named) if named != worldspace_id => Some(format!(
            "worldspace {named:08X}, not the {worldspace_id:08X} this run streams"
        )),
        _ => None,
    }
}

/// Which evidence this observation owns. Shared tour/door callers observe a view;
/// capture receipts observe renderer work; the shot runner requires both.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub enum SettleScope {
    #[default]
    View,
    Capture,
    ViewAndCapture,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct SettleCounts {
    pub scope: SettleScope,
    /// Loaded models still waiting for their turn in the arming budget.
    pub arming_queue_depth: usize,
    /// Cells out of range still waiting for their turn in the unload budget: still drawn.
    pub retiring_cells: usize,
    /// Cells the database refused, or whose terrain or water failed validation, since this shot
    /// started settling (the run's counter is cumulative; see [`Failures`]).
    pub failed_cells: u64,
    /// Assets that failed to load at all, since this shot started settling.
    pub asset_load_failures: u64,
    /// Material, terrain, water, transform-bounds and renderer validation failures together: a
    /// view with one of these was drawn from something the engine refused, and a shot of it is
    /// not the reference the run is trying to reproduce. Counted since this shot started settling.
    pub validation_failures: u64,
    /// The renderer's final path is running and the warm-up has passed.
    pub renderer_ready: bool,
    /// Failed LOD queries and chunks since this shot started settling (the run's counters are
    /// cumulative; see [`Failures`]).
    pub failed_lod_work: u64,
    pub outstanding_terrain_uploads: usize,
    pub pending_batch_cpu: usize,
    pub pending_batch_initial: usize,
    pub pending_batch_selection: usize,
    pub pending_specializations: Option<usize>,
    pub pending_render_transfers: Option<usize>,

    /// Whether the cell the shot's camera stands in is streamed in and spawned.
    pub space_resident: bool,
    /// Whether that cell's request failed and is not repeated (its landscape failed validation,
    /// say): it will never be resident. A `--shots` pose whose own cell is failed is a terminal
    /// failure ([`SettleStep::Failed`]): it is never photographed, because a photo of the ground
    /// that is missing is exactly what must not sign off. The demo tour and the portal bench read
    /// the same count through [`is_quiet`] and still treat it as quiet - nothing more is coming for
    /// it - so a failed cell there does not hang them.
    pub space_failed: bool,
    /// Cells of any space submitted to the database but not yet resident.
    pub loading_cells: usize,
    /// Database requests in flight.
    pub active_requests: usize,
    /// Spawned model scenes whose assets have not finished loading.
    pub pending_asset_instances: usize,
    /// Terrain and water surfaces whose textures have not finished loading.
    pub pending_surface_instances: usize,
    /// Surfaces a view can see that the render world has not confirmed drawable: spawned and covered
    /// by a stand-in, but not confirmed drawn (`crate::swap_bridge::SwapWaiting`). A view is not
    /// settled while it has one, because the surface may be photographed as a hole (design-1091 §4).
    /// A waiting surface no view can see is not counted: it is not a hole and holds nothing up.
    pub pending_drawn: usize,
    /// LOD queries submitted to the world database but not answered.
    pub pending_lod_queries: usize,
    /// LOD chunks requested but not yet spawned.
    pub pending_lod_chunks: usize,
    /// Cluster batches whose material a view wants and that have not loaded.
    pub pending_cluster_batches: usize,
    /// Demanded cluster source/material loads that cannot complete.
    pub failed_cluster_batches: usize,
    /// Exact distant stock requests not yet submitted to the database.
    pub queued_far_stock_queries: usize,
    /// Exact distant stock requests submitted but not yet committed.
    pub pending_far_stock_queries: usize,
    /// Current distant stock still awaiting fresh scoped render qualification feedback.
    pub pending_far_stock_qualification: usize,
    /// Current demanded distant stock sources that cannot complete.
    pub failed_far_stock_sources: usize,
    /// Distant object blocks and tree billboard blocks requested but not yet spawned.
    pub pending_lod_objects: usize,
    /// LOD chunks whose load failed and was never recovered: a terminal failure, counted so a
    /// settled view's log shows it, not a pending count that holds the settle (a chunk that will
    /// never arrive is settled the way a failed cell is, see `space_failed`).
    pub failed_lod_chunks: u64,
    /// LOD queries whose load failed and was never recovered.
    pub unrecovered_lod_queries: usize,
    /// LOD chunks whose load failed and was never recovered.
    pub unrecovered_lod_chunks: usize,
    /// Frames the counts have been quiet for, before this frame.
    pub quiet_frames: u32,
}

/// The run-wide failure counters at one moment. They only ever grow, so a shot compares them with
/// the snapshot taken when it started settling: a failure from an earlier shot's view must not
/// hold every later shot back until the timeout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Failures {
    pub failed_cells: u64,
    pub asset_load_failures: u64,
    pub validation_failures: u64,
    pub failed_lod_work: u64,
}

impl Failures {
    /// The cumulative counters as the streamer and the renderer report them now.
    pub fn read(streaming: &StreamingMetrics, renderer: &RendererMetrics) -> Self {
        Self {
            failed_cells: streaming.failed_cells,
            asset_load_failures: streaming.asset_load_failures,
            validation_failures: streaming
                .material_validation_failures
                .saturating_add(streaming.terrain_validation_failures)
                .saturating_add(streaming.water_validation_failures)
                .saturating_add(streaming.transform_bounds_validation_failures)
                .saturating_add(renderer.renderer_validation_failures),
            failed_lod_work: streaming
                .failed_lod_queries
                .saturating_add(streaming.failed_lod_chunks),
        }
    }
}

impl SettleCounts {
    /// What the streamer and the renderer say is still pending, with the quiet window so far. The
    /// failure fields hold only the failures that happened after `baseline`.
    pub fn read(
        streaming: &StreamingMetrics,
        renderer: &RendererMetrics,
        baseline: Failures,
        renderer_ready: bool,
        quiet_frames: u32,
    ) -> Self {
        let now = Failures::read(streaming, renderer);
        Self {
            scope: SettleScope::Capture,
            pending_cluster_batches: streaming.pending_cluster_batches,
            failed_cluster_batches: streaming.failed_cluster_batches,
            queued_far_stock_queries: streaming.queued_far_stock_queries,
            pending_far_stock_queries: streaming.pending_far_stock_queries,
            pending_far_stock_qualification: streaming.pending_far_stock_qualification,
            failed_far_stock_sources: streaming.failed_far_stock_sources,
            pending_lod_objects: streaming.pending_lod_object_blocks
                + streaming.pending_lod_tree_blocks
                + streaming.unrecovered_lod_object_blocks
                + streaming.unrecovered_lod_tree_blocks
                + streaming.pending_native_lod_draws,
            loading_cells: streaming.loading_cells,
            active_requests: streaming.active_requests,
            pending_asset_instances: streaming.pending_asset_instances,
            pending_surface_instances: streaming.pending_surface_instances,
            arming_queue_depth: streaming.arming_queue_depth,
            retiring_cells: streaming.retiring_cells,
            failed_cells: now.failed_cells.saturating_sub(baseline.failed_cells),
            asset_load_failures: now
                .asset_load_failures
                .saturating_sub(baseline.asset_load_failures),
            validation_failures: now
                .validation_failures
                .saturating_sub(baseline.validation_failures),
            renderer_ready,
            quiet_frames,
            pending_lod_queries: streaming.pending_lod_queries,
            pending_lod_chunks: streaming.pending_lod_chunks,
            failed_lod_work: now.failed_lod_work.saturating_sub(baseline.failed_lod_work),
            ..default()
        }
    }

    /// Adds current per-view residency and draw acknowledgments to renderer evidence.
    /// Both observations must come from the same main/extraction frame.
    pub(crate) fn with_view(mut self, view: Self) -> Self {
        self.scope = SettleScope::ViewAndCapture;
        self.space_resident = view.space_resident;
        self.space_failed = view.space_failed;
        self.pending_drawn = view.pending_drawn;
        self.failed_lod_chunks = view.failed_lod_chunks;
        self.unrecovered_lod_queries = view.unrecovered_lod_queries;
        self.unrecovered_lod_chunks = view.unrecovered_lod_chunks;
        self
    }

    /// Nothing is pending: every load the current view asked for has landed, and every surface a
    /// view can see is confirmed drawn. The LOD pending counts are part of this: a chunk still on
    /// its way is work that will change the frame, so a view with one is not settled (design-1091
    /// §4). Legacy LOD failure counts do not hold it: a chunk that failed for good is as settled as
    /// a cell that failed for good, and both are reported through `describe`. Demanded cluster and
    /// exact distant stock failures do hold it. A quiet view whose own cell is
    /// failed is still a hard failure for a `--shots` pose ([`SettleStep::Failed`]), which reads
    /// `space_failed` ahead of this.
    pub fn is_quiet(&self) -> bool {
        (self.scope == SettleScope::Capture || self.space_resident || self.space_failed)
            && (self.scope == SettleScope::View || self.capture_work_is_quiet())
            && self.loading_cells == 0
            && self.active_requests == 0
            && self.pending_asset_instances == 0
            && self.pending_surface_instances == 0
            && self.pending_drawn == 0
            && self.pending_lod_queries == 0
            && self.pending_lod_chunks == 0
            && self.pending_cluster_batches == 0
            && self.failed_cluster_batches == 0
            && self.queued_far_stock_queries == 0
            && self.pending_far_stock_queries == 0
            && self.pending_far_stock_qualification == 0
            && self.failed_far_stock_sources == 0
            && self.pending_lod_objects == 0
    }

    fn capture_work_is_quiet(&self) -> bool {
        self.failed_cells == 0
            && self.asset_load_failures == 0
            && self.validation_failures == 0
            && self.failed_lod_work == 0
            && self.arming_queue_depth == 0
            && self.retiring_cells == 0
            && self.renderer_ready
            && self.outstanding_terrain_uploads == 0
            && self.pending_batch_cpu == 0
            && self.pending_batch_initial == 0
            && self.pending_batch_selection == 0
            && self.pending_specializations == Some(0)
            && self.pending_render_transfers == Some(0)
    }

    /// The pending work, for the log line of a shot that never settled.
    pub fn describe(&self) -> String {
        format!(
            "cell_resident={} cell_failed={} loading_cells={} active_requests={} \
             pending_assets={} pending_surfaces={} pending_drawn={} pending_lod_queries={} pending_lod_chunks={} \
             pending_cluster_batches={} failed_cluster_batches={} queued_far_stock_queries={} pending_far_stock_queries={} pending_far_stock_qualification={} failed_far_stock_sources={} \
             failed_lod_chunks={} unrecovered_lod_queries={} \
             unrecovered_lod_chunks={} pending_lod_objects={}",
            self.space_resident,
            self.space_failed,
            self.loading_cells,
            self.active_requests,
            self.pending_asset_instances,
            self.pending_surface_instances,
            self.pending_drawn,
            self.pending_lod_queries,
            self.pending_lod_chunks,
            self.pending_cluster_batches,
            self.failed_cluster_batches,
            self.queued_far_stock_queries,
            self.pending_far_stock_queries,
            self.pending_far_stock_qualification,
            self.failed_far_stock_sources,
            self.failed_lod_chunks,
            self.unrecovered_lod_queries,
            self.unrecovered_lod_chunks,
            self.pending_lod_objects
        ) + &format!(
            " scope={:?} arming={} retiring={} failed_cells={} asset_load_failures={} validation_failures={} renderer_ready={} lod_failures={} terrain_transfers={} batch_cpu={} initial_uploads={} selection_uploads={} specializations={:?} render_transfers={:?}",
            self.scope,
            self.arming_queue_depth,
            self.retiring_cells,
            self.failed_cells,
            self.asset_load_failures,
            self.validation_failures,
            self.renderer_ready,
            self.failed_lod_work,
            self.outstanding_terrain_uploads,
            self.pending_batch_cpu,
            self.pending_batch_initial,
            self.pending_batch_selection,
            self.pending_specializations,
            self.pending_render_transfers
        )
    }
}

/// The settle state one frame on: the frame counted into the quiet window when nothing was
/// pending, the window started again when something was.
pub fn advance_settle(counts: SettleCounts) -> SettleCounts {
    let quiet_frames = if counts.is_quiet() {
        counts.quiet_frames.saturating_add(1)
    } else {
        0
    };
    SettleCounts {
        quiet_frames,
        ..counts
    }
}

/// Whether a view may be photographed: nothing pending, and nothing pending for a few frames
/// after that, so the first drawn frames, lights and shadows have caught up.
pub fn shots_settled(counts: &SettleCounts) -> bool {
    counts.is_quiet() && counts.quiet_frames >= SETTLE_QUIET_FRAMES
}

/// What the settle phase does with one frame's counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettleStep {
    /// Still coming: look again next frame.
    Waiting,
    /// The view is settled: photograph it.
    Ready,
    /// The view has waited out [`SETTLE_TIMEOUT_SECONDS`] and is still not settled. The shot fails
    /// with this reason - no screenshot is taken, so the run cannot sign off a blank or a hole -
    /// and the run exits non-zero (design-1091 §4).
    TimedOut(String),
    /// The cell the shot's camera stands in failed to load and is not retried: it will never be
    /// resident, so the view would photograph missing ground. A terminal failure at once, never a
    /// silent screenshot - no PNG is taken and the run exits non-zero. Only the shot's *own* cell
    /// fails it: a failed neighbour is outside the pose and is still photographed (the pose draws
    /// what is there, and the acceptance path reports the cell separately).
    Failed(String),
}

/// The settle phase's rule (design-1091 §4): settled the moment the view is quiet for
/// [`SETTLE_QUIET_FRAMES`], failed once [`SETTLE_TIMEOUT_SECONDS`] have passed without it, and
/// waiting in between. The shot's own cell having failed is checked first and fails the shot at
/// once ([`SettleStep::Failed`]): a cell that will never be resident is not worth waiting a window
/// for, and photographing where it should be would sign off missing ground. Either failure is hard,
/// never a silent screenshot: the reason carries the counts so the log says what was wrong.
pub fn settle_step(shot: &Shot, counts: &SettleCounts, settled: bool, waited: f32) -> SettleStep {
    settle_step_with_timeout(shot, counts, settled, waited, SETTLE_TIMEOUT_SECONDS)
}

fn settle_step_with_timeout(
    shot: &Shot,
    counts: &SettleCounts,
    settled: bool,
    waited: f32,
    timeout: f32,
) -> SettleStep {
    if counts.space_failed {
        SettleStep::Failed(format!(
            "shot \"{}\": the cell it stands in failed to load ({})",
            shot.name,
            counts.describe()
        ))
    } else if counts.failed_cluster_batches != 0 || counts.failed_far_stock_sources != 0 {
        // The producer may still describe the previous pose in this Update.
        // Never capture a failed view, but allow fresh demand facts to replace
        // an old failure before the existing settle deadline.
        if waited < timeout {
            SettleStep::Waiting
        } else {
            SettleStep::Failed(format!(
                "shot \"{}\": a demanded cluster or distant stock source failed to load ({})",
                shot.name,
                counts.describe()
            ))
        }
    } else if settled {
        SettleStep::Ready
    } else if waited >= timeout {
        SettleStep::TimedOut(format!(
            "shot \"{}\": view did not settle within {timeout:.0} s ({})",
            shot.name,
            counts.describe()
        ))
    } else {
        SettleStep::Waiting
    }
}

// ---------------------------------------------------------------------------------------------
// ---------------------------------------------------------------------------------------------

/// One line of `shots.log`: the shot's name, how long its view took to settle, whether that settle
/// timed out, and how many meshes were resident when the screenshot was requested.
pub fn log_line(
    name: &str,
    settle_seconds: f32,
    timed_out: bool,
    resident_meshes: usize,
) -> String {
    format!(
        "{name} settle={settle_seconds:.2}s resident_meshes={resident_meshes} timed_out={}",
        if timed_out { "yes" } else { "no" }
    )
}

/// Runs public reference shots before Update streaming planning. The app installs streaming,
/// capture extraction and renderer readiness resources before adding this plugin.
pub struct ShotsPlugin {
    pub run: ShotsRun,
}

/// Exact-size render target shared by the world camera and screenshot requests.
#[derive(Resource)]
pub(crate) struct ShotOutputImage(pub Handle<Image>);

impl Plugin for ShotsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Assets<Image>>();
        let image = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                self.run.file.width,
                self.run.file.height,
                bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
                None,
            ));
        app.insert_resource(ShotOutputImage(image));
        app.insert_resource(self.run.clone())
            .add_systems(PreUpdate, run_shots);
    }
}

/// The state of a `--shots` run: what to render, where it goes, and how far it has got.
#[derive(Resource, Debug, Clone)]
pub struct ShotsRun {
    pub file: ShotsFile,
    pub output_dir: PathBuf,
    /// The worldspace the run streams; shots in another one are skipped.
    pub worldspace_id: u32,
    pub(crate) settle_timeout_seconds: f32,
    pub(crate) route_checkpoints: Option<BTreeSet<usize>>,
    transition_capture: bool,

    shot: usize,
    phase: Phase,
    /// Real seconds in the current phase; never the clamped virtual game clock.
    timer: f32,
    /// The current screenshot's actual request and completed disk-write times.
    capture: Option<CaptureRun>,
    /// Frames the current shot has waited for its view to settle, for the log.
    frames: u32,
    /// What was last pending, for the log of a shot that timed out.
    counts: SettleCounts,
    /// The run's failure counters when the current shot started settling.
    failure_baseline: Failures,
    timed_out: bool,
    directory_made: bool,
    window_size: Option<(u32, u32)>,
    log_started: bool,
    log_failed: bool,
    frames_without_camera: u32,
    /// Seconds the current shot's view took to settle, for the log.
    startup_held: bool,
    package_wait_noted: bool,
    initial_load_ready: bool,
    initial_upload_wait_noted: bool,
    settle_seconds: f32,
    quiet_frames: u32,
    resident_meshes: usize,
    log: String,
    written: bool,
    failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Move the camera into the shot's space and pose it, then settle.
    Move,
    /// Hold a route step through an extraction before advancing.
    Traverse,
    /// Wait for the view to settle (or time out).
    Settle,
    /// The screenshot has been requested; wait for it on disk.
    Capture,
    /// Write the log and exit.
    Done,
}

#[derive(Debug, Clone)]
struct CaptureRun {
    requested_at: Instant,
    completed_at: Option<Instant>,
    result: Option<Result<(), String>>,
}

#[derive(Debug, PartialEq, Eq)]
enum CaptureStep {
    Waiting,
    Ready,
    TimedOut,
}

impl CaptureRun {
    /// Completion means the synchronous save observer returned success. A save
    /// completed before the deadline remains valid when a slow frame observes it later. A file
    /// first completed at/after the deadline cannot pass merely because it now exists.
    fn step(&self, now: Instant) -> CaptureStep {
        let observed = self.completed_at.unwrap_or(now);
        if observed.duration_since(self.requested_at)
            >= Duration::from_secs_f32(CAPTURE_TIMEOUT_SECONDS)
        {
            CaptureStep::TimedOut
        } else if self.completed_at.is_some() {
            CaptureStep::Ready
        } else {
            CaptureStep::Waiting
        }
    }
}

impl ShotsRun {
    pub fn new(file: ShotsFile, output_dir: PathBuf, worldspace_id: u32) -> Self {
        Self {
            file,
            output_dir,
            worldspace_id,
            settle_timeout_seconds: SETTLE_TIMEOUT_SECONDS,
            route_checkpoints: None,
            transition_capture: false,
            frames: 0,
            counts: SettleCounts::default(),
            failure_baseline: Failures::default(),
            timed_out: false,
            directory_made: false,
            window_size: None,
            log_started: false,
            log_failed: false,
            frames_without_camera: 0,
            shot: 0,
            phase: Phase::Move,
            timer: 0.0,
            capture: None,
            startup_held: false,
            package_wait_noted: false,
            initial_load_ready: false,
            initial_upload_wait_noted: false,
            settle_seconds: 0.0,
            quiet_frames: 0,
            resident_meshes: 0,
            log: String::new(),
            written: false,
            failed: false,
        }
    }

    fn note_initial_upload_wait(
        &mut self,
        scene: Option<&crate::cluster::render::ClusterScene>,
        metrics: Option<&StreamingMetrics>,
    ) {
        if self.initial_upload_wait_noted {
            return;
        }
        let total = scene.map_or(0, |scene| scene.blocks.len());
        let resident = scene.map_or(0, |scene| {
            scene
                .blocks
                .iter()
                .filter(|block| block.resident.load(std::sync::atomic::Ordering::Acquire))
                .count()
        });
        self.note(format!(
            "certified; waiting for initial cluster load (resident_blocks={resident}/{total} pending_cluster_batches={:?})",
            metrics.map(|metrics| metrics.pending_cluster_batches)
        ));
        self.initial_upload_wait_noted = true;
    }

    fn advance_view_automation(
        &mut self,
        delta: f32,
        validation: Option<&PackageValidationStatus>,
        scene: Option<&crate::cluster::render::ClusterScene>,
        metrics: Option<&StreamingMetrics>,
    ) -> bool {
        if self.shot == 0 && !self.initial_load_ready {
            match validation {
                Some(PackageValidationStatus::Pending) => {
                    if !self.package_wait_noted {
                        self.note("waiting for requested cluster package certification");
                        self.package_wait_noted = true;
                    }
                    self.timer = 0.0;
                    self.quiet_frames = 0;
                    self.startup_held = true;
                    return false;
                }
                Some(PackageValidationStatus::Failed(reason)) => {
                    self.fail(format!(
                        "requested cluster package certification failed: {reason}"
                    ));
                    self.enter(Phase::Done);
                    return false;
                }
                Some(PackageValidationStatus::Ready) | None => {
                    if validation.is_some() && !initial_cluster_load_ready(scene, metrics) {
                        self.note_initial_upload_wait(scene, metrics);
                        self.timer = 0.0;
                        self.quiet_frames = 0;
                        self.startup_held = true;
                        return false;
                    }
                    self.initial_load_ready = true;
                    if self.package_wait_noted || self.initial_upload_wait_noted {
                        self.note("initial cluster load ready; starting view readiness window");
                        self.timer = 0.0;
                        self.quiet_frames = 0;
                        self.package_wait_noted = false;
                    }
                }
            }
        }
        // The first ready observation ends this phase's hold; the preceding frame interval
        // still belonged to startup. Later frames use the full unclamped real delta.
        if !std::mem::take(&mut self.startup_held) {
            self.timer += delta;
        }
        true
    }

    /// The window size the file asks for, in physical pixels: the scale factor is pinned to 1, so
    /// the window (and so the PNG) is exactly this many pixels whatever the display is set to.
    pub fn window_resolution(&self) -> WindowResolution {
        WindowResolution::new(self.file.width, self.file.height).with_scale_factor_override(1.0)
    }

    pub fn aspect(&self) -> f32 {
        self.file.aspect()
    }

    pub fn shot_path(&self, shot: &Shot) -> PathBuf {
        let suffix = if self.transition_capture {
            "-transition"
        } else {
            ""
        };
        self.output_dir.join(format!("{}{suffix}.png", shot.name))
    }

    pub fn log_path(&self) -> PathBuf {
        self.output_dir.join("shots.log")
    }

    /// One line of `shots.log`: the shot's name, how many frames it waited to settle, whether it
    /// settled or the timeout took it - and with what still pending, when it did - and its image.
    /// A window that is not the file's frame is named too: the image is then not the shape the
    /// reference is, which a comparison tool reading the log has to know.
    pub fn log_line(&self, shot: &Shot, path: &Path) -> String {
        let outcome = if self.transition_capture {
            "transition".to_owned()
        } else if self.timed_out {
            format!("timed_out ({})", self.counts.describe())
        } else {
            "settled".to_owned()
        };
        let mut line = format!(
            "{} frames={} {outcome} path={}",
            shot.name,
            self.frames,
            path.display()
        );
        if let Some((width, height)) = self.window_size
            && (width, height) != (self.file.width, self.file.height)
        {
            line.push_str(&format!(" window={width}x{height}"));
        }
        line
    }

    /// The path to show for one of this run's own output files, in `shots.log` and in the engine
    /// log: relative to `output_dir` rather than absolute, so a log or note never carries the
    /// user's file-system layout. `shot_path` and `log_path` only ever hand out children of
    /// `output_dir`, so this is ordinarily just the file name; the absolute path is the fallback
    /// for anything else, rather than a path with no name at all.
    fn relative_to_output<'a>(&self, path: &'a Path) -> &'a Path {
        path.strip_prefix(&self.output_dir).unwrap_or(path)
    }

    /// Records a line of another module's in `shots.log` and in the engine log: the graphics
    /// settings the run renders with (`crate::graphics_settings`).
    pub fn note_line(&mut self, line: impl AsRef<str>) {
        self.note(line);
    }

    /// Records a line in `shots.log` and in the engine log.
    fn note(&mut self, line: impl AsRef<str>) {
        let line = line.as_ref();
        info!(target: "shots", "{line}");
        self.log.push_str(line);
        self.log.push('\n');
        self.append_log(line);
    }

    /// Appends one line to `shots.log`, creating the file on the first line (and truncating what an
    /// earlier run left there). Writing each line as it happens is what keeps a run that is killed
    /// from losing the record of the shots it had already taken.
    fn append_log(&mut self, line: &str) {
        if self.log_failed {
            return;
        }
        if let Err(error) = self.write_log_line(line) {
            self.log_failed = true;
            self.failed = true;
            error!(
                target: "shots",
                "could not append to {}: {error}",
                self.log_path().display()
            );
        }
    }

    fn write_log_line(&mut self, line: &str) -> std::io::Result<()> {
        fs::create_dir_all(&self.output_dir)?;
        let path = self.log_path();
        let mut file = if self.log_started {
            fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)?
        } else {
            fs::File::create(&path)?
        };
        writeln!(file, "{line}")?;
        self.log_started = true;
        Ok(())
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.timer = 0.0;
        self.capture = None;
    }

    /// Starts a fresh quiet window for the current public camera pose.
    fn enter_settle(&mut self) {
        self.quiet_frames = 0;
        self.frames = 0;
        self.counts = SettleCounts::default();
        self.timed_out = false;
        self.enter(Phase::Settle);
    }

    /// Moves on to the next shot in the file, or to the end when this was the last one. A shot
    /// that was given up on has been marked failed by its caller ([`ShotsRun::fail`]) before it
    /// gets here.
    fn next_shot(&mut self) {
        self.shot += 1;
        if self.shot < self.file.shots.len() {
            self.enter(Phase::Move);
        } else {
            self.enter(Phase::Done);
        }
    }

    fn fail(&mut self, reason: impl AsRef<str>) {
        self.failed = true;
        self.note(format!("FAILED: {}", reason.as_ref()));
    }
}

#[derive(SystemParam)]
struct CaptureWorld<'w> {
    output: Res<'w, ShotOutputImage>,
    renderer: Res<'w, RendererMetrics>,
    profiler: Res<'w, crate::profiling::ProfilingState>,
    uploads: Res<'w, crate::terrain_upload::TerrainMeshUploadReadiness>,
    retries: Res<'w, crate::capture_frame::RetryReadiness>,
    transfers: Res<'w, crate::capture_frame::TransferReadiness>,
    frame: Res<'w, bevy::diagnostic::FrameCount>,
}

#[allow(clippy::too_many_arguments)]
fn run_shots(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut run: ResMut<ShotsRun>,
    mut active: ResMut<ActiveSpace>,
    mut origin: ResMut<RenderOrigin>,
    mut roots: crate::streaming::CellRootTransforms,
    mut camera: Query<(&mut Transform, &mut Projection), With<StreamingCamera>>,
    streaming: Option<Res<StreamingWorld>>,
    metrics: Option<Res<StreamingMetrics>>,
    validation: Option<Res<PackageValidationStatus>>,
    scene: Option<Res<crate::cluster::render::ClusterScene>>,
    meshes: Query<(), With<Mesh3d>>,
    capture_world: CaptureWorld,
    // The surfaces the render world has not confirmed drawn while a view can see them: the settle
    // waits for these ([`crate::swap_bridge::pending_drawn`]).
    waiting: Query<Has<SwapInCameraView>, With<SwapWaiting>>,
    mut exit: MessageWriter<AppExit>,
) {
    if run.phase == Phase::Done {
        if !run.written {
            run.written = true;
            finish_log(&mut run);
        }
        exit.write(if run.failed {
            AppExit::error()
        } else {
            AppExit::Success
        });
        return;
    }
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        run.frames_without_camera = run.frames_without_camera.saturating_add(1);
        if run.frames_without_camera >= MISSING_CAMERA_FRAMES {
            run.fail(format!(
                "no single streaming camera to pose within {MISSING_CAMERA_FRAMES} frames"
            ));
            run.enter(Phase::Done);
        }
        return;
    };
    run.frames_without_camera = 0;
    // Automation deadlines measure real waits, including slow/loading frames, independent of
    // the virtual clock's maximum delta, pause and speed. Startup holds remain explicit below.
    let delta = time.delta_secs();
    match run.phase {
        Phase::Move => {
            let Some(mut shot) = run.file.shots.get(run.shot).cloned() else {
                run.fail("there is no shot to render");
                run.enter(Phase::Done);
                return;
            };
            if shot.worldspace_id.is_none() && shot.interior_cell_id.is_none() {
                shot.worldspace_id = Some(run.worldspace_id);
            }
            let Some(target) = shot.space_key() else {
                run.fail(format!("shot \"{}\" names no space", shot.name));
                run.enter(Phase::Done);
                return;
            };
            if !run.directory_made {
                if let Err(error) = fs::create_dir_all(&run.output_dir) {
                    run.fail(format!("could not make the output directory: {error}"));
                    run.enter(Phase::Done);
                    return;
                }
                run.directory_made = true;
            }
            place_camera(
                &shot,
                run.aspect(),
                target,
                &mut active,
                &mut origin,
                &mut roots,
                &mut transform,
                &mut projection,
            );
            run.quiet_frames = 0;
            run.settle_seconds = 0.0;
            run.frames = 0;
            run.counts = SettleCounts::default();
            run.timed_out = false;
            run.enter_settle();
            run.failure_baseline = metrics
                .as_deref()
                .map_or_else(Failures::default, |metrics| {
                    Failures::read(metrics, &capture_world.renderer)
                });
            if let Some(checkpoints) = &run.route_checkpoints {
                if checkpoints.contains(&run.shot) {
                    run.transition_capture = true;
                    request_capture(
                        &mut commands,
                        &mut run,
                        capture_world.frame.0,
                        &capture_world.output.0,
                    );
                } else {
                    run.enter(Phase::Traverse);
                }
            }
        }
        Phase::Traverse => {
            let mut shot = run.file.shots[run.shot].clone();
            if shot.worldspace_id.is_none() && shot.interior_cell_id.is_none() {
                shot.worldspace_id = Some(run.worldspace_id);
            }
            if let Some(target) = shot.space_key() {
                place_camera(
                    &shot,
                    run.aspect(),
                    target,
                    &mut active,
                    &mut origin,
                    &mut roots,
                    &mut transform,
                    &mut projection,
                );
            }
            let line = format!(
                "route_step={} main_frame={} position={:?} elapsed_seconds={}",
                run.shot,
                capture_world.frame.0,
                shot.position,
                time.elapsed_secs_f64()
            );
            run.note(line);
            run.next_shot();
        }
        Phase::Settle => {
            let Some(mut shot) = run.file.shots.get(run.shot).cloned() else {
                run.fail("there is no shot to render");
                run.enter(Phase::Done);
                return;
            };
            if shot.worldspace_id.is_none() && shot.interior_cell_id.is_none() {
                shot.worldspace_id = Some(run.worldspace_id);
            }
            let Some(target) = shot.space_key() else {
                run.fail(format!("shot \"{}\" names no space", shot.name));
                run.enter(Phase::Done);
                return;
            };
            // Re-assert the pose every frame: a render-origin rebase moves the camera in render
            // space, and the Creation position is what has to be photographed.
            place_camera(
                &shot,
                run.aspect(),
                target,
                &mut active,
                &mut origin,
                &mut roots,
                &mut transform,
                &mut projection,
            );
            if !run.advance_view_automation(
                delta,
                validation.as_deref(),
                scene.as_deref(),
                metrics.as_deref(),
            ) {
                return;
            }
            let view_counts = settle_counts(
                shot.space_key(),
                run.quiet_frames,
                pending_drawn(&waiting),
                streaming.as_deref(),
                metrics.as_deref(),
            );
            let mut counts = metrics
                .as_deref()
                .map_or_else(SettleCounts::default, |metrics| {
                    SettleCounts::read(
                        metrics,
                        &capture_world.renderer,
                        run.failure_baseline,
                        capture_world.renderer.final_path_active()
                            && time.elapsed_secs() >= WARM_UP_SECONDS,
                        run.quiet_frames,
                    )
                });
            counts = counts.with_view(view_counts);
            counts.outstanding_terrain_uploads = capture_world.uploads.outstanding();
            counts.pending_batch_cpu = capture_world
                .profiler
                .gauge("lod/pending_terrain_batch_chunks")
                .unwrap_or(0.0) as usize;
            counts.pending_batch_initial = capture_world
                .profiler
                .gauge("lod/pending_initial_terrain_upload_chunks")
                .unwrap_or(0.0) as usize;
            counts.pending_batch_selection = capture_world
                .profiler
                .gauge("lod/pending_terrain_selection_uploads")
                .unwrap_or(0.0) as usize;
            counts.pending_specializations = capture_world
                .retries
                .0
                .lock()
                .ok()
                .and_then(|pending| *pending);
            counts.pending_render_transfers = capture_world
                .transfers
                .0
                .lock()
                .ok()
                .and_then(|pending| *pending);
            run.counts = advance_settle(counts);
            run.quiet_frames = run.counts.quiet_frames;
            run.frames = run.frames.saturating_add(1);
            let settled = shots_settled(&run.counts);
            match settle_step_with_timeout(
                &shot,
                &run.counts,
                settled,
                run.timer,
                run.settle_timeout_seconds,
            ) {
                SettleStep::Waiting => {}
                SettleStep::Ready => {
                    run.settle_seconds = run.timer;
                    run.resident_meshes = meshes.iter().count();
                    run.transition_capture = false;
                    request_capture(
                        &mut commands,
                        &mut run,
                        capture_world.frame.0,
                        &capture_world.output.0,
                    );
                }
                SettleStep::TimedOut(reason) => {
                    // A view that has not settled in the window is a hard failure, never a silent
                    // screenshot: the reason carries the counts, no PNG is written for this shot
                    // (so a blank or holed frame cannot pass for it), and the run exits non-zero
                    // ([`ShotsRun::fail`], the run's `AppExit::error`). The shots after it are
                    // still rendered: one bad pose should not cost the rest of a sweep.
                    run.fail(reason);
                    run.next_shot();
                }
                SettleStep::Failed(reason) => {
                    // The shot's own cell failed to load and will never be resident: a photograph
                    // of the pose would be of missing ground, so this is a hard failure with no
                    // PNG, the reason carries the counts, and the run exits non-zero
                    // ([`ShotsRun::fail`]). Exactly the timeout path's shape: the shots after it
                    // are still rendered: one bad cell should not cost the rest of a sweep.
                    run.fail(reason);
                    run.next_shot();
                }
            }
        }
        Phase::Capture => {
            let Some(mut shot) = run.file.shots.get(run.shot).cloned() else {
                run.fail("there is no shot to render");
                run.enter(Phase::Done);
                return;
            };
            if shot.worldspace_id.is_none() && shot.interior_cell_id.is_none() {
                shot.worldspace_id = Some(run.worldspace_id);
            }
            if let Some(target) = shot.space_key() {
                place_camera(
                    &shot,
                    run.aspect(),
                    target,
                    &mut active,
                    &mut origin,
                    &mut roots,
                    &mut transform,
                    &mut projection,
                );
            }
            let path = run.shot_path(&shot);
            let capture = run.capture.as_ref().expect("capture phase has a request");
            let now = Instant::now();
            let step = capture.step(now);
            run.timer = now.duration_since(capture.requested_at).as_secs_f32();
            if step == CaptureStep::Ready {
                // A shot only reaches the capture after a settle that did not time out (a timeout
                // fails it), so `timed_out` is always `no` for a line that carries a screenshot.
                let line = log_line(&shot.name, run.settle_seconds, false, run.resident_meshes);
                run.note(line);
                let line = run.log_line(&shot, run.relative_to_output(&path));
                run.note(line);
                if run.transition_capture {
                    run.transition_capture = false;
                    run.enter_settle();
                } else {
                    run.next_shot();
                }
            } else if step == CaptureStep::TimedOut {
                let line = log_line(&shot.name, run.settle_seconds, false, run.resident_meshes);
                let logged_path = run.relative_to_output(&path).display().to_string();
                run.note(format!(
                    "{line} FAILED: screenshot at {logged_path} did not complete within \
                     {CAPTURE_TIMEOUT_SECONDS:.0} real s"
                ));
                run.failed = true;
                run.enter(Phase::Done);
            } else if let Some(Err(reason)) = run
                .capture
                .as_ref()
                .and_then(|capture| capture.result.as_ref())
            {
                let reason = reason.clone();
                run.fail(format!("shot \"{}\": {reason}", shot.name));
                run.enter(Phase::Done);
            }
        }
        Phase::Done => {}
    }
}

/// A serialized runner has one image-target request in flight. Keep the ticket on that
/// entity until its readback arrives; callback arrival time is deliberately not its frame ID.
fn request_capture(
    commands: &mut Commands,
    run: &mut ShotsRun,
    request_frame: u32,
    output: &Handle<Image>,
) {
    let shot = &run.file.shots[run.shot];
    let path = run.shot_path(shot);
    if let Err(reason) = discard_previous_shot(&path) {
        run.fail(reason);
        run.enter(Phase::Done);
        return;
    }
    let ticket = crate::capture_frame::CaptureTicket::new(run.failure_baseline);
    let observed = ticket.clone();
    let position = shot.position;
    let rotation = shot_camera_rotation(shot.yaw, shot.pitch).to_array();
    let fov = vertical_fov_degrees(shot.hfov, run.file.aspect()).to_radians();
    let index = run.shot;
    let transition = run.transition_capture;
    let dimensions = (run.file.width, run.file.height);
    let counts = run.counts;
    run.enter(Phase::Capture);
    run.capture = Some(CaptureRun {
        requested_at: Instant::now(),
        completed_at: None,
        result: None,
    });
    commands
        .spawn((Screenshot::image(output.clone()), ticket))
        .observe(move |trigger: On<ScreenshotCaptured>, mut run: ResMut<ShotsRun>| {
            if run.shot != index
                || run.transition_capture != transition
                || run.phase != Phase::Capture
            {
                return;
            }
            let result = crate::capture_frame::validate_receipt(&observed, position)
                .and_then(|receipt| {
                    if receipt.main_frame < request_frame
                        || receipt.rotation_runtime_xyzw != rotation
                        || receipt.vertical_fov_radians != Some(fov)
                    {
                        return Err(
                            "captured frame precedes request or differs in camera orientation/projection"
                                .into(),
                        );
                    }
                    let size = trigger.image.texture_descriptor.size;
                    if (size.width, size.height) != dimensions {
                        return Err("captured image dimensions differ from requested dimensions".into());
                    }
                    if !transition && (receipt.work.scope != SettleScope::ViewAndCapture
                        || receipt.work.space_failed || !receipt.work.is_quiet()) {
                        return Err("captured settled frame has pending or unavailable work".into());
                    }
                    let grading = receipt.grading.as_ref().ok_or("captured frame has no grading source receipt")?;
                    write_shot(trigger.image.clone(), &path)?;
                    let line = serde_json::json!({
                        "capture": index,
                        "kind": if transition { "transition" } else { "settled" },
                        "requested_main_frame": request_frame,
                        "requested_camera_creation": position,
                        "requested_rotation_runtime_xyzw": rotation,
                        "requested_vertical_fov_radians": fov,
                        "captured_frame": receipt,
                        "settle_counts": counts,
                        "timed_out": run.timed_out,
                        "image": path,
                        "dimensions": dimensions,
                        "color_grade": grading.color_grade,
                        "visual_preset": grading.visual_preset,
                        "preset_saturation": grading.preset_saturation,
                        "grading_source": grading.source,
                        "resolved_grade": grading.resolved_grade,
                        "water_time_seconds": if run.route_checkpoints.is_none() { Some(1.0) } else { None }
                    });
                    run.note(line.to_string());
                    if run.log_failed {
                        return Err("capture receipt could not be saved".into());
                    }
                    Ok(())
                });
            let completed_at = result.as_ref().ok().map(|_| Instant::now());
            if let Some(capture) = run.capture.as_mut() {
                capture.completed_at = completed_at;
                capture.result = Some(result);
            }
        });
}

/// Writes one captured image to `path`, or says why the image is not there to be compared.
///
/// The run writes its screenshots itself rather than through Bevy's `save_to_disk`, which logs its
/// error and drops it: a save that dies after creating the file - a full disk, an encoding error -
/// would otherwise be logged as a settled shot and the run would exit 0. The write is done here so
/// the shot is judged on the `Result` itself. The file is RGB, not RGBA, exactly as Bevy 0.19.0's
/// `save_to_disk` writes it (`bevy_render` `view/window/screenshot.rs:143`: `dyn_img.to_rgb8()`,
/// "discard the alpha channel which stores brightness values when HDR is enabled"), so the PNGs
/// match what the screenshot helper would have produced. The file is then checked to be a
/// complete, non-empty PNG.
///
/// `image` is taken by value because `Image::try_into_dynamic` consumes it (bevy_image 0.19.0,
/// `image_texture_conversion.rs:168`); the observer only has a borrow of the event, so it clones,
/// as Bevy's own `save_to_disk` does (`screenshot.rs:137`).
fn write_shot(image: Image, path: &Path) -> Result<(), String> {
    let dynamic = image
        .try_into_dynamic()
        .map_err(|error| format!("the captured image could not be understood: {error}"))?;
    // Written to a temporary file next to the target and renamed only once it is a complete PNG,
    // so a failed or partial save never leaves a file at the shot's path.
    let partial = partial_path(path);
    let mut encoded = Cursor::new(Vec::new());
    dynamic
        .to_rgb8()
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| format!("the image could not be encoded: {error}"))?;
    let written = fs::write(&partial, encoded.into_inner())
        .map_err(|error| format!("the image could not be written: {error}"))
        .and_then(|()| {
            if png_is_complete(&partial) {
                Ok(())
            } else {
                Err(format!("{} is not a complete PNG", partial.display()))
            }
        })
        .and_then(|()| {
            fs::rename(&partial, path).map_err(|error| {
                format!(
                    "the image could not be moved to {}: {error}",
                    path.display()
                )
            })
        });
    if written.is_err() {
        let _ = fs::remove_file(&partial);
    }
    written
}

/// The temporary file a shot's PNG is written to before it is renamed into place: `<name>.partial..png`.
/// It keeps the `.png` extension, which is how the encoder picks the format, and its stem
/// (`<name>.partial.`) ends in a dot, which [`Shot::validate`] refuses in a shot's name. So no other
/// shot's image can have this path, and writing one shot never touches another shot's file.
fn partial_path(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{stem}.partial..png"))
}

/// Whether a file is a whole PNG stream: the signature at its head and the end-of-image chunk at
/// its tail. A save that died part way leaves one or the other out (or an empty file), which is
/// the failure the file's existence alone cannot see.
fn png_is_complete(path: &Path) -> bool {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    const END: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 8];
    if file.read_exact(&mut head).is_err() || head != SIGNATURE {
        return false;
    }
    if file.seek(SeekFrom::End(-(END.len() as i64))).is_err() {
        return false;
    }
    let mut tail = [0u8; 12];
    file.read_exact(&mut tail).is_ok() && tail == END
}

/// Removes a PNG an earlier run left at this shot's path, so that the file appearing is proof that
/// this screenshot reached the disk. A file that cannot be removed (on Windows, one open in an
/// image viewer) is an error: it would otherwise be logged as this shot.
fn discard_previous_shot(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "could not remove the previous {}: {error}",
            path.display()
        )),
    }
}

/// Ends `shots.log`: the lines were appended as the shots ended, so this only makes sure a file of
/// shots leaves a log at all (a run whose every line failed to write reports the error) and says
/// where it is. A run killed part way through never reaches this, and keeps the lines written.
fn finish_log(run: &mut ShotsRun) {
    let path = run.log_path();
    if run.log_failed {
        return;
    }
    let opened = fs::create_dir_all(&run.output_dir).and_then(|()| {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map(|_| ())
    });
    if let Err(error) = opened {
        error!(target: "shots", "could not write {}: {error}", path.display());
        run.failed = true;
    } else {
        // A route checkpoint takes a transition and a settled capture; indices past the
        // file's shots are never reached, so they cannot count toward the expected total.
        let expected_captures = run
            .route_checkpoints
            .as_ref()
            .map_or(run.file.shots.len(), |checkpoints| {
                checkpoints.range(..run.file.shots.len()).count() * 2
            });
        run.note(serde_json::json!({"run_complete": !run.failed, "poses_finished": run.shot, "expected_poses": run.file.shots.len(), "expected_captures": expected_captures}).to_string());
        info!(target: "shots", "shots log written to {}", path.display());
    }
}

/// Moves the camera into the shot's space (as a door crossing does) and poses it there.
///
/// The camera sits at `position` exactly: unlike a crossing there is no eye-height offset, because
/// a reference pose is a camera, not a player's feet.
#[allow(clippy::too_many_arguments)]
fn place_camera(
    shot: &Shot,
    aspect: f32,
    target: CellKey,
    active: &mut ActiveSpace,
    origin: &mut RenderOrigin,
    roots: &mut crate::streaming::CellRootTransforms,
    transform: &mut Transform,
    projection: &mut Projection,
) {
    transform.translation = place_shot_space(target, shot.position, active, origin, roots);
    transform.rotation = shot_camera_rotation(shot.yaw, shot.pitch);
    if let Projection::Perspective(perspective) = projection {
        perspective.fov = vertical_fov_degrees(shot.hfov, aspect).to_radians();
        perspective.aspect_ratio = aspect;
    }
}

/// What the streaming code says is still pending for a view of the cell `key`.
///
/// The rule is shared: the shot runner reads it to decide when to photograph a pose, the door
/// sequence to decide when a view is ready to be asked to open its door, the demo tour's own settle
/// to decide when a place has streamed in (`crate::demo_tour`) and the portal bench to decide when a
/// door state is quiet enough to time. `pending_drawn` is the one count the streaming state cannot
/// answer - it is a render-world fact the main world reads ([`crate::swap_bridge::pending_drawn`]),
/// so every caller passes it in.
pub(crate) fn initial_cluster_load_ready(
    scene: Option<&crate::cluster::render::ClusterScene>,
    metrics: Option<&StreamingMetrics>,
) -> bool {
    scene.is_some_and(|scene| {
        scene
            .blocks
            .iter()
            .all(|block| block.resident.load(std::sync::atomic::Ordering::Acquire))
    }) && metrics.is_some_and(|metrics| metrics.pending_cluster_batches == 0)
}

pub(crate) fn settle_counts(
    key: Option<CellKey>,
    quiet_frames: u32,
    pending_drawn: usize,
    streaming: Option<&StreamingWorld>,
    metrics: Option<&StreamingMetrics>,
) -> SettleCounts {
    SettleCounts {
        space_resident: key
            .is_some_and(|key| streaming.is_some_and(|world| world.is_resident(&key))),
        space_failed: key.is_some_and(|key| streaming.is_some_and(|world| world.has_failed(&key))),
        loading_cells: metrics.map_or(0, |metrics| metrics.loading_cells),
        active_requests: metrics.map_or(0, |metrics| metrics.active_requests),
        pending_asset_instances: metrics.map_or(0, |metrics| metrics.pending_asset_instances),
        pending_surface_instances: metrics.map_or(0, |metrics| metrics.pending_surface_instances),
        pending_drawn,
        pending_lod_queries: metrics.map_or(0, |metrics| metrics.pending_lod_queries),
        pending_lod_chunks: metrics.map_or(0, |metrics| metrics.pending_lod_chunks),
        pending_cluster_batches: metrics.map_or(0, |metrics| metrics.pending_cluster_batches),
        failed_cluster_batches: metrics.map_or(0, |metrics| metrics.failed_cluster_batches),
        queued_far_stock_queries: metrics.map_or(0, |metrics| metrics.queued_far_stock_queries),
        pending_far_stock_queries: metrics.map_or(0, |metrics| metrics.pending_far_stock_queries),
        pending_far_stock_qualification: metrics
            .map_or(0, |metrics| metrics.pending_far_stock_qualification),
        failed_far_stock_sources: metrics.map_or(0, |metrics| metrics.failed_far_stock_sources),
        pending_lod_objects: metrics.map_or(0, |metrics| {
            metrics.pending_lod_object_blocks
                + metrics.pending_lod_tree_blocks
                + metrics.unrecovered_lod_object_blocks
                + metrics.unrecovered_lod_tree_blocks
                + metrics.pending_native_lod_draws
        }),
        failed_lod_chunks: metrics.map_or(0, |metrics| metrics.failed_lod_chunks),
        unrecovered_lod_queries: metrics.map_or(0, |metrics| metrics.unrecovered_lod_queries),
        unrecovered_lod_chunks: metrics.map_or(0, |metrics| metrics.unrecovered_lod_chunks),
        quiet_frames,
        ..default()
    }
}

fn place_shot_space(
    target: CellKey,
    position: [f32; 3],
    active: &mut ActiveSpace,
    origin: &mut RenderOrigin,
    roots: &mut crate::streaming::CellRootTransforms,
) -> Vec3 {
    match target {
        CellKey::Interior(cell_id) => {
            active.interior = Some(cell_id);
            Vec3::from_array(shared::coordinates::creation_to_runtime_vector(position))
        }
        CellKey::Exterior {
            worldspace_id,
            grid_x,
            grid_y,
        } => {
            active.worldspace_id = Some(worldspace_id);
            active.interior = None;
            let next_origin = IVec2::new(grid_x, grid_y);
            if origin.0 != next_origin {
                origin.0 = next_origin;
                crate::streaming::reposition_cell_roots(origin.0, roots);
            }
            shot_camera_translation(position, origin.0)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn initial_scene(resident: bool) -> crate::cluster::render::ClusterScene {
        use crate::cluster::render::{ClusterScene, SceneBlock};
        use std::sync::{Arc, atomic::AtomicBool};
        ClusterScene {
            certificate: None,
            terrain_blocks: Vec::new(),
            meshes: Vec::new(),
            instances: Vec::new(),
            origin_offset: Vec3::ZERO,
            blocks: vec![SceneBlock {
                first_instance: 0,
                instance_count: 0,
                meshes: Vec::new(),
                ready: Arc::new(AtomicBool::new(true)),
                resident: Arc::new(AtomicBool::new(resident)),
            }],
        }
    }

    #[test]
    fn demanded_cluster_asset_failure_cannot_be_accepted_as_a_settled_shot() {
        let metrics = StreamingMetrics {
            failed_cluster_batches: 1,
            ..default()
        };
        let counts = settle_counts(None, SETTLE_QUIET_FRAMES, 0, None, Some(&metrics));
        assert_eq!(counts.failed_cluster_batches, 1);
        let failed = SettleCounts {
            space_resident: true,
            ..counts
        };
        assert!(!shots_settled(&failed));
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).unwrap();
        // Even a stale caller-supplied settled flag cannot bypass terminal failure.
        assert_eq!(
            settle_step(&file.shots[0], &failed, true, 0.0),
            SettleStep::Waiting
        );
        let SettleStep::Failed(reason) =
            settle_step(&file.shots[0], &failed, true, SETTLE_TIMEOUT_SECONDS)
        else {
            panic!("a failed demanded cluster load must never arm a capture");
        };
        assert!(reason.contains("failed_cluster_batches=1"), "{reason}");
    }

    #[test]
    fn distant_stock_metrics_map_and_each_current_work_state_holds_settling() {
        let metrics = StreamingMetrics {
            queued_far_stock_queries: 2,
            pending_far_stock_queries: 3,
            pending_far_stock_qualification: 7,
            failed_far_stock_sources: 5,
            ..default()
        };
        let counts = settle_counts(None, SETTLE_QUIET_FRAMES, 0, None, Some(&metrics));
        assert_eq!(counts.queued_far_stock_queries, 2);
        assert_eq!(counts.pending_far_stock_queries, 3);
        assert_eq!(counts.pending_far_stock_qualification, 7);
        assert_eq!(counts.failed_far_stock_sources, 5);
        let description = counts.describe();
        for field in [
            "queued_far_stock_queries=2",
            "pending_far_stock_queries=3",
            "pending_far_stock_qualification=7",
            "failed_far_stock_sources=5",
        ] {
            assert!(description.contains(field), "{description}");
        }

        for hold in [
            (|counts: &mut SettleCounts| counts.queued_far_stock_queries = 1)
                as fn(&mut SettleCounts),
            |counts| counts.pending_far_stock_queries = 1,
            |counts| counts.pending_far_stock_qualification = 7,
            |counts| counts.failed_far_stock_sources = 1,
        ] {
            let mut counts = quiet_counts();
            hold(&mut counts);
            assert!(!counts.is_quiet(), "{}", counts.describe());
            assert!(!shots_settled(&counts));
        }
        assert!(shots_settled(&quiet_counts()));
        let counts = SettleCounts {
            pending_far_stock_qualification: 7,
            ..quiet_counts()
        };
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).unwrap();
        assert_eq!(
            settle_step(&file.shots[0], &counts, false, 0.0),
            SettleStep::Waiting
        );
        let SettleStep::TimedOut(reason) =
            settle_step(&file.shots[0], &counts, false, SETTLE_TIMEOUT_SECONDS)
        else {
            panic!("missing qualification stays pending until the existing settle deadline");
        };
        assert!(
            reason.contains("pending_far_stock_qualification=7"),
            "{reason}"
        );
    }

    #[test]
    fn distant_stock_failure_waits_for_fresh_demand_but_never_arms_capture() {
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).unwrap();
        let shot = &file.shots[0];
        let failed = SettleCounts {
            failed_far_stock_sources: 1,
            ..quiet_counts()
        };
        // A stale ready flag cannot capture, and a prior pose's failure gets the existing window
        // in which the producer can replace it with this pose's current demand.
        for waited in [0.0, SETTLE_TIMEOUT_SECONDS - 1.0] {
            assert_eq!(
                settle_step(shot, &failed, true, waited),
                SettleStep::Waiting
            );
        }
        assert_eq!(
            settle_step(shot, &quiet_counts(), true, 1.0),
            SettleStep::Ready
        );
        let SettleStep::Failed(reason) = settle_step(shot, &failed, true, SETTLE_TIMEOUT_SECONDS)
        else {
            panic!("a current demanded distant stock failure must never arm a capture");
        };
        assert!(reason.contains("failed_far_stock_sources=1"), "{reason}");
    }

    #[test]
    fn initial_shot_certification_wait_preserves_the_capture_window() {
        let phase = Phase::Settle;
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).expect("fixture parses");
        let mut run = ShotsRun::new(file, PathBuf::from("unused-shots-output"), 60);
        assert_eq!(
            run.phase,
            Phase::Move,
            "the target pose is placed before waiting"
        );
        run.enter(phase);
        run.quiet_frames = SETTLE_QUIET_FRAMES;
        let scene = initial_scene(true);
        let metrics = StreamingMetrics::default();
        for _ in 0..40 {
            assert!(!run.advance_view_automation(
                1.0,
                Some(&PackageValidationStatus::Pending),
                None,
                None
            ));
        }
        assert_eq!(
            run.phase, phase,
            "no capture or route advance during certification"
        );
        assert_eq!(run.shot, 0);
        assert_eq!(run.timer, 0.0);
        assert_eq!(run.quiet_frames, 0);
        assert_eq!(run.log.lines().count(), 1);
        assert!(run.advance_view_automation(
            0.25,
            Some(&PackageValidationStatus::Ready),
            Some(&scene),
            Some(&metrics)
        ));
        assert_eq!(run.timer, 0.0, "the release frame's interval was held");
        assert_eq!(run.quiet_frames, 0);
        assert!(run.advance_view_automation(
            0.25,
            Some(&PackageValidationStatus::Ready),
            Some(&scene),
            Some(&metrics)
        ));
        assert_eq!(run.timer, 0.25, "ordinary window advances after release");
    }

    #[test]
    fn certified_shots_wait_for_initial_residency_then_keep_ordinary_timeouts() {
        let scene = initial_scene(false);
        let mut metrics = StreamingMetrics::default();
        assert!(!initial_cluster_load_ready(None, Some(&metrics)));
        assert!(!initial_cluster_load_ready(Some(&scene), None));
        assert!(!initial_cluster_load_ready(Some(&scene), Some(&metrics)));
        let mut empty_scene = initial_scene(true);
        empty_scene.blocks.clear();
        assert!(
            initial_cluster_load_ready(Some(&empty_scene), Some(&metrics)),
            "a present legitimately empty scene needs no uploads"
        );
        let phase = Phase::Settle;
        scene.blocks[0]
            .resident
            .store(false, std::sync::atomic::Ordering::Release);
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).expect("fixture parses");
        let mut run = ShotsRun::new(file, PathBuf::from("unused-shots-output"), 60);
        run.enter(phase);
        let ready = Some(&PackageValidationStatus::Ready);
        for _ in 0..40 {
            assert!(!run.advance_view_automation(1.0, ready, Some(&scene), Some(&metrics)));
        }
        assert_eq!(run.timer, 0.0);
        assert_eq!(run.quiet_frames, 0);
        assert_eq!(run.phase, phase);
        assert_eq!(run.shot, 0);
        scene.blocks[0]
            .resident
            .store(true, std::sync::atomic::Ordering::Release);
        metrics.pending_cluster_batches = 1;
        assert!(!run.advance_view_automation(40.0, ready, Some(&scene), Some(&metrics)));
        metrics.pending_cluster_batches = 0;
        assert!(initial_cluster_load_ready(Some(&scene), Some(&metrics)));
        assert!(run.advance_view_automation(0.25, ready, Some(&scene), Some(&metrics)));
        assert_eq!(run.timer, 0.0, "the release frame's interval was held");
        assert!(run.initial_load_ready);
        metrics.pending_cluster_batches = 1;
        assert!(run.advance_view_automation(
            SETTLE_TIMEOUT_SECONDS,
            ready,
            Some(&scene),
            Some(&metrics)
        ));
        let pending = SettleCounts {
            pending_cluster_batches: 1,
            ..quiet_counts()
        };
        assert!(matches!(
            settle_step(&run.file.shots[0], &pending, false, run.timer),
            SettleStep::TimedOut(_)
        ));
        assert_eq!(
            run.log.matches("waiting for initial cluster load").count(),
            1
        );
        assert!(run.log.contains("resident_blocks=0/1"));
    }

    #[test]
    fn shot_certification_failure_cannot_capture_and_stock_startup_is_unchanged() {
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).expect("fixture parses");
        let mut run = ShotsRun::new(file.clone(), PathBuf::from("unused-shots-output"), 60);
        run.enter(Phase::Settle);
        let failure = PackageValidationStatus::Failed("invalid certificate".into());
        assert!(!run.advance_view_automation(20.0, Some(&failure), None, None));
        assert_eq!(run.phase, Phase::Done);
        assert!(run.failed);
        assert!(run.log.contains("invalid certificate"));
        assert_eq!(run.shot, 0);
        assert_eq!(run.timer, 0.0);
        let mut stock = ShotsRun::new(file, PathBuf::from("unused-shots-output"), 60);
        stock.enter(Phase::Settle);
        assert!(stock.advance_view_automation(0.5, None, None, None));
        assert_eq!(stock.phase, Phase::Settle);
        assert_eq!(stock.timer, 0.5);
        assert!(!stock.failed);
    }

    use crate::world::components::ExteriorCellGrid;
    use bevy::ecs::system::SystemState;
    use shared::coordinates::runtime_to_creation_vector;

    /// The shape of the example in `docs/design/reference-shots.md`, with a portable `reference`
    /// path, plus a field the engine must ignore.
    const DESIGN_EXAMPLE: &str = r#"{
      "width": 1400,
      "height": 1050,
      "shots": [
        {
          "name": "SR-place-Alftand_02",
          "worldspace_id": 60,
          "interior_cell_id": null,
          "position": [77000.0, 77500.0, -5200.0],
          "yaw": 135.0,
          "pitch": 20.0,
          "hfov": 75.0,
          "reference": "references/alftand-02.jpg",
          "note": "free text, ignored by the engine",
          "future_tool_field": {"anything": true}
        }
      ]
    }"#;

    fn interior_example() -> String {
        r#"{"width": 1050, "height": 1400, "shots": [
            {"name": "SR-map-Alftand01", "interior_cell_id": 86723,
             "position": [-947.0, 3958.0, 712.0], "yaw": 170.0, "pitch": -10.0, "hfov": 60.0}
        ]}"#
        .to_owned()
    }

    #[test]
    fn parses_the_design_example_and_ignores_unknown_fields() {
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).unwrap();
        assert_eq!((file.width, file.height), (1400, 1050));
        assert!((file.aspect() - 4.0 / 3.0).abs() < 1.0e-6);
        assert_eq!(file.shots.len(), 1);
        let shot = &file.shots[0];
        assert_eq!(shot.name, "SR-place-Alftand_02");
        assert_eq!(shot.worldspace_id, Some(60));
        assert_eq!(shot.interior_cell_id, None);
        assert_eq!(shot.position, [77000.0, 77500.0, -5200.0]);
        assert_eq!(shot.yaw, 135.0);
        assert_eq!(shot.pitch, 20.0);
        assert_eq!(shot.hfov, 75.0);
        assert_eq!(shot.reference.as_deref(), Some("references/alftand-02.jpg"));
        assert_eq!(
            shot.space_key(),
            Some(CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 18,
                grid_y: 18
            })
        );
    }

    #[test]
    fn parses_an_interior_shot_without_the_exterior_field() {
        let file = ShotsFile::from_json(&interior_example()).unwrap();
        let shot = &file.shots[0];
        assert_eq!(shot.worldspace_id, None);
        assert_eq!(shot.interior_cell_id, Some(86723));
        assert_eq!(shot.space_key(), Some(CellKey::Interior(86723)));
        assert_eq!(shot.space_key(), Some(CellKey::Interior(86723)));
        assert_eq!(shot.reference, None);
    }

    #[test]
    fn a_shot_must_name_exactly_one_space() {
        for (reason, shot) in [
            (
                "both",
                r#"{"name": "both", "worldspace_id": 60, "interior_cell_id": 86723,
                    "position": [0.0, 0.0, 0.0], "yaw": 0.0, "pitch": 0.0, "hfov": 75.0}"#,
            ),
            (
                "neither",
                r#"{"name": "neither", "position": [0.0, 0.0, 0.0],
                    "yaw": 0.0, "pitch": 0.0, "hfov": 75.0}"#,
            ),
        ] {
            let text = format!(r#"{{"width": 1400, "height": 1050, "shots": [{shot}]}}"#);
            let error = ShotsFile::from_json(&text).unwrap_err();
            assert!(
                error.message().contains("worldspace_id"),
                "{reason}: {error}"
            );
        }
    }

    #[test]
    fn a_shot_must_have_a_usable_name() {
        let missing = r#"{"width": 1400, "height": 1050, "shots": [
            {"worldspace_id": 60, "position": [0.0, 0.0, 0.0],
             "yaw": 0.0, "pitch": 0.0, "hfov": 75.0}]}"#;
        let error = ShotsFile::from_json(missing).unwrap_err();
        assert!(error.message().contains("name"), "{error}");

        // A name is a file stem in the output folder, never a path: "../evil" would write outside
        // the folder the caller asked for.
        for name in ["", "..", "sub/shot", "sub\\shot"] {
            let text = format!(
                r#"{{"width": 1400, "height": 1050, "shots": [
                    {{"name": "{name}", "worldspace_id": 60, "position": [0.0, 0.0, 0.0],
                      "yaw": 0.0, "pitch": 0.0, "hfov": 75.0}}]}}"#
            );
            assert!(
                ShotsFile::from_json(&text).is_err(),
                "name {name:?} must be rejected"
            );
        }
    }

    #[test]
    fn a_pitch_outside_the_player_limit_is_rejected() {
        for pitch in ["95.0", "-95.0", "89.5"] {
            let text = format!(
                r#"{{"width": 1400, "height": 1050, "shots": [
                    {{"name": "steep", "worldspace_id": 60, "position": [0.0, 0.0, 0.0],
                      "yaw": 0.0, "pitch": {pitch}, "hfov": 75.0}}]}}"#
            );
            let error = ShotsFile::from_json(&text).unwrap_err();
            assert!(error.message().contains("pitch"), "pitch {pitch}: {error}");
        }
        // The limits themselves are allowed.
        for pitch in ["-89.0", "89.0"] {
            let text = format!(
                r#"{{"width": 1400, "height": 1050, "shots": [
                    {{"name": "steep", "worldspace_id": 60, "position": [0.0, 0.0, 0.0],
                      "yaw": 0.0, "pitch": {pitch}, "hfov": 75.0}}]}}"#
            );
            assert!(ShotsFile::from_json(&text).is_ok(), "pitch {pitch}");
        }
    }

    #[test]
    fn an_hfov_outside_the_render_range_is_rejected() {
        for hfov in ["9.0", "171.0", "0.0", "-75.0"] {
            let text = format!(
                r#"{{"width": 1400, "height": 1050, "shots": [
                    {{"name": "wide", "worldspace_id": 60, "position": [0.0, 0.0, 0.0],
                      "yaw": 0.0, "pitch": 0.0, "hfov": {hfov}}}]}}"#
            );
            let error = ShotsFile::from_json(&text).unwrap_err();
            assert!(error.message().contains("hfov"), "hfov {hfov}: {error}");
        }
    }

    #[test]
    fn a_number_that_is_not_finite_is_rejected() {
        // `1e40` is valid JSON but not a finite f32, and a NaN or an infinity in a pose renders
        // nothing at all.
        let text = r#"{"width": 1400, "height": 1050, "shots": [
            {"name": "overflow", "worldspace_id": 60, "position": [1e40, 0.0, 0.0],
             "yaw": 0.0, "pitch": 0.0, "hfov": 75.0}]}"#;
        let error = ShotsFile::from_json(text).unwrap_err();
        assert!(
            error.message().contains("finite") || error.message().contains("number out of range"),
            "the overflowing position must be rejected: {error}"
        );
    }

    #[test]
    fn a_file_needs_a_size_and_at_least_one_shot() {
        for text in [
            r#"{"width": 0, "height": 1050, "shots": []}"#,
            r#"{"width": 1400, "height": 1050, "shots": []}"#,
        ] {
            assert!(ShotsFile::from_json(text).is_err(), "{text}");
        }
    }

    #[test]
    fn the_default_output_folder_is_the_file_stem_beside_the_file() {
        assert_eq!(
            default_output_dir(Path::new("shots/tamriel.json")),
            PathBuf::from("shots/tamriel-shots")
        );
        assert_eq!(
            default_output_dir(Path::new("shots.json")),
            PathBuf::from("shots-shots")
        );
        assert_eq!(
            default_output_dir(Path::new("shots/poses")),
            PathBuf::from("shots/poses-shots"),
            "a file with no extension cannot name a folder the same as itself"
        );
    }

    /// The camera's forward in Creation coordinates, whatever the runtime basis is.
    fn creation_forward(rotation: Quat) -> Vec3 {
        Vec3::from_array(runtime_to_creation_vector(
            (rotation * Vec3::NEG_Z).to_array(),
        ))
    }

    #[test]
    fn yaw_zero_looks_north_and_ninety_looks_east() {
        assert!(
            creation_forward(shot_camera_rotation(0.0, 0.0)).abs_diff_eq(Vec3::Y, 1.0e-5),
            "yaw 0 is Creation +Y (north)"
        );
        assert!(
            creation_forward(shot_camera_rotation(90.0, 0.0)).abs_diff_eq(Vec3::X, 1.0e-5),
            "yaw 90 is Creation +X (east)"
        );
        assert!(
            creation_forward(shot_camera_rotation(180.0, 0.0)).abs_diff_eq(Vec3::NEG_Y, 1.0e-5)
        );
        assert!(
            creation_forward(shot_camera_rotation(270.0, 0.0)).abs_diff_eq(Vec3::NEG_X, 1.0e-5)
        );
    }

    #[test]
    fn positive_pitch_looks_down() {
        let forward = creation_forward(shot_camera_rotation(0.0, 30.0));
        assert!(
            (forward.z + 30.0_f32.to_radians().sin()).abs() < 1.0e-5,
            "Creation Z is {}, expected -sin 30",
            forward.z
        );
        assert!(
            (forward.y - 30.0_f32.to_radians().cos()).abs() < 1.0e-5,
            "the heading is unchanged by the pitch"
        );

        let up = creation_forward(shot_camera_rotation(0.0, -45.0));
        assert!(
            (up.z - 45.0_f32.to_radians().sin()).abs() < 1.0e-5,
            "a negative pitch looks up: {up:?}"
        );

        // Pitch turns the same forward up or down whatever the heading.
        for yaw in [0.0_f32, 45.0, 135.0, -170.0] {
            let forward = creation_forward(shot_camera_rotation(yaw, 20.0));
            assert!(
                (forward.z + 20.0_f32.to_radians().sin()).abs() < 1.0e-5,
                "yaw {yaw}: {forward:?}"
            );
        }
    }

    #[test]
    fn the_vertical_fov_of_a_four_three_view_is_narrower_than_the_horizontal_one() {
        // 75 degrees horizontal at 4:3 is 2*atan(tan(37.5 deg)/1.3333) = 59.84 degrees vertical.
        let vertical = vertical_fov_degrees(75.0, 4.0 / 3.0);
        assert!(
            (vertical - 59.84).abs() < 0.01,
            "vertical fov is {vertical}, expected about 59.84"
        );
        // A square view has equal fields of view, and a wide one a taller vertical field.
        assert!((vertical_fov_degrees(75.0, 1.0) - 75.0).abs() < 1.0e-4);
        assert!(vertical_fov_degrees(75.0, 16.0 / 9.0) < 75.0);
    }

    fn quiet_counts() -> SettleCounts {
        SettleCounts {
            space_resident: true,
            quiet_frames: SETTLE_QUIET_FRAMES,
            ..default()
        }
    }

    #[test]
    fn capture_readiness_requires_both_view_and_renderer_evidence() {
        let mut capture = SettleCounts::read(
            &StreamingMetrics::default(),
            &RendererMetrics::default(),
            Failures::default(),
            true,
            SETTLE_QUIET_FRAMES,
        );
        capture.pending_specializations = Some(0);
        capture.pending_render_transfers = Some(0);
        assert!(
            capture.is_quiet(),
            "renderer-only evidence remains usable by its callers"
        );
        let view = quiet_counts();
        let combined = capture.with_view(view);
        assert_eq!(combined.scope, SettleScope::ViewAndCapture);
        assert!(shots_settled(&combined));
        assert!(!shots_settled(&capture.with_view(SettleCounts {
            pending_drawn: 1,
            ..view
        })));
        assert!(!shots_settled(&capture.with_view(SettleCounts {
            space_resident: false,
            ..view
        })));
        assert!(!shots_settled(&SettleCounts {
            pending_specializations: None,
            ..combined
        }));
        assert!(!shots_settled(&SettleCounts {
            pending_render_transfers: None,
            ..combined
        }));
        assert!(!shots_settled(&SettleCounts {
            validation_failures: 1,
            ..combined
        }));
        assert!(
            view.is_quiet(),
            "view-only tour/door callers do not fabricate renderer acknowledgments"
        );
    }

    #[test]
    fn public_runs_allocate_the_exact_requested_image_target() {
        for file in [
            ShotsFile::from_json(DESIGN_EXAMPLE).unwrap(),
            ShotsFile::from_json(&interior_example()).unwrap(),
        ] {
            let dimensions = (file.width, file.height);
            let output = tempfile::tempdir().unwrap();
            let app = automation_app(ShotsRun::new(file, output.path().into(), 60));
            let target = &app.world().resource::<ShotOutputImage>().0;
            let images = app.world().resource::<Assets<Image>>();
            let size = images.get(target).unwrap().texture_descriptor.size;
            assert_eq!((size.width, size.height), dimensions);
        }
    }

    /// The actual shot system and Bevy time plugin, without rendering or asset loading.
    fn automation_app(run: ShotsRun) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ActiveSpace>()
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .init_resource::<StreamingMetrics>()
            .init_resource::<RendererMetrics>()
            .init_resource::<crate::profiling::ProfilingState>()
            .init_resource::<crate::terrain_upload::TerrainMeshUploadReadiness>()
            .init_resource::<crate::capture_frame::RetryReadiness>()
            .init_resource::<crate::capture_frame::TransferReadiness>()
            .add_plugins(ShotsPlugin { run });
        app.world_mut()
            .spawn((StreamingCamera, Transform::default(), Projection::default()));
        app
    }

    #[test]
    fn unclamped_real_time_expires_a_public_settle_without_capture() {
        let output = tempfile::tempdir().unwrap();
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).unwrap();
        let mut run = ShotsRun::new(file, output.path().into(), 60);
        run.enter_settle();
        let mut app = automation_app(run);
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            Duration::from_secs(31),
        ));
        app.update();
        app.update();
        let run = app.world().resource::<ShotsRun>();
        assert!(run.failed, "{}", run.log);
        assert_eq!(run.phase, Phase::Done);
        assert!(run.log.contains("did not settle within 30 s"));
        assert!(run.capture.is_none());
        assert!(!run.shot_path(&run.file.shots[0]).exists());
    }

    /// Reach capture through the real settle consumer, then inject its renderer completion event.

    #[test]
    fn a_route_checkpoint_freezes_pose_then_advances_only_after_settled_capture() {
        let output = tempfile::tempdir().unwrap();
        let mut file = ShotsFile::from_json(DESIGN_EXAMPLE).unwrap();
        let mut next = file.shots[0].clone();
        next.name = "second".into();
        next.position[0] += 64.0;
        file.shots.push(next);
        let mut run = ShotsRun::new(file, output.path().into(), 60);
        run.route_checkpoints = Some(BTreeSet::from([0]));
        let mut app = automation_app(run);
        app.update();
        assert_eq!(app.world().resource::<ShotsRun>().phase, Phase::Capture);
        assert!(app.world().resource::<ShotsRun>().transition_capture);
        app.world_mut().resource_mut::<RenderOrigin>().0 = IVec2::new(3, -2);
        app.update();
        let mut query = app
            .world_mut()
            .query_filtered::<&Transform, With<StreamingCamera>>();
        let transform = query.single(app.world()).unwrap();
        assert_eq!(
            transform.translation,
            shot_camera_translation([77000.0, 77500.0, -5200.0], IVec2::new(18, 18))
        );
        let completed = || {
            let now = Instant::now();
            CaptureRun {
                requested_at: now,
                completed_at: Some(now),
                result: Some(Ok(())),
            }
        };
        app.world_mut().resource_mut::<ShotsRun>().capture = Some(completed());
        app.update();
        assert_eq!(app.world().resource::<ShotsRun>().phase, Phase::Settle);
        assert_eq!(app.world().resource::<ShotsRun>().shot, 0);
        {
            let mut run = app.world_mut().resource_mut::<ShotsRun>();
            run.enter(Phase::Capture);
            run.capture = Some(completed());
        }
        app.update();
        assert_eq!(app.world().resource::<ShotsRun>().shot, 1);
        app.update();
        assert_eq!(app.world().resource::<ShotsRun>().phase, Phase::Traverse);
        app.update();
        assert_eq!(app.world().resource::<ShotsRun>().phase, Phase::Done);
    }

    #[test]
    fn an_unremovable_previous_image_never_arms_a_capture() {
        let output = tempfile::tempdir().unwrap();
        let mut run = ShotsRun::new(
            ShotsFile::from_json(DESIGN_EXAMPLE).unwrap(),
            output.path().into(),
            60,
        );
        let path = run.shot_path(&run.file.shots[0]);
        fs::create_dir(&path).unwrap();
        let mut world = World::new();
        request_capture(&mut world.commands(), &mut run, 0, &Handle::default());
        world.flush();
        assert!(run.failed);
        assert_eq!(run.phase, Phase::Done);
        assert_eq!(world.query::<&Screenshot>().iter(&world).count(), 0);
    }

    #[test]
    fn mismatched_or_unavailable_receipts_refuse_the_image_and_fail_the_run() {
        for invalid in 0..9 {
            let (mut app, _output, path, request) = capture_app();
            complete_screenshot_with(&mut app, request, |receipt, image| match invalid {
                0 => receipt.camera_creation[0] += 1.0,
                1 => receipt.main_frame = 0,
                2 => receipt.rotation_runtime_xyzw = [0.0; 4],
                3 => receipt.vertical_fov_radians = None,
                4 => image.texture_descriptor.size.width += 1,
                5 => receipt.work.scope = SettleScope::Capture,
                6 => receipt.work.space_failed = true,
                7 => receipt.work.pending_specializations = None,
                8 => receipt.grading = None,
                _ => unreachable!(),
            });
            let capture = app.world().resource::<ShotsRun>().capture.as_ref().unwrap();
            assert!(
                capture.result.as_ref().unwrap().is_err(),
                "invalid case {invalid}"
            );
            assert!(capture.completed_at.is_none());
            assert!(
                !path.exists(),
                "invalid case {invalid} must not publish an image"
            );
            app.update();
            assert!(app.world().resource::<ShotsRun>().failed);
            assert_eq!(app.world().resource::<ShotsRun>().phase, Phase::Done);
        }
    }

    #[test]
    fn a_missing_receipt_is_not_a_successful_screenshot() {
        let (mut app, _output, path, request) = capture_app();
        app.world_mut().trigger(ScreenshotCaptured {
            entity: request,
            image: Image::default_uninit(),
        });
        app.update();
        let run = app.world().resource::<ShotsRun>();
        assert!(run.failed);
        assert!(run.log.contains("no captured render-frame receipt"));
        assert!(!path.exists());
    }

    #[test]
    fn every_public_pending_barrier_resets_the_quiet_window() {
        let ready = SettleCounts {
            scope: SettleScope::ViewAndCapture,
            space_resident: true,
            renderer_ready: true,
            pending_specializations: Some(0),
            pending_render_transfers: Some(0),
            quiet_frames: SETTLE_QUIET_FRAMES,
            ..default()
        };
        assert!(shots_settled(&ready));
        let pending = [
            SettleCounts {
                loading_cells: 1,
                ..ready
            },
            SettleCounts {
                active_requests: 1,
                ..ready
            },
            SettleCounts {
                pending_asset_instances: 1,
                ..ready
            },
            SettleCounts {
                pending_surface_instances: 1,
                ..ready
            },
            SettleCounts {
                pending_drawn: 1,
                ..ready
            },
            SettleCounts {
                pending_lod_queries: 1,
                ..ready
            },
            SettleCounts {
                pending_lod_chunks: 1,
                ..ready
            },
            SettleCounts {
                pending_lod_objects: 1,
                ..ready
            },
            SettleCounts {
                arming_queue_depth: 1,
                ..ready
            },
            SettleCounts {
                retiring_cells: 1,
                ..ready
            },
            SettleCounts {
                failed_cells: 1,
                ..ready
            },
            SettleCounts {
                asset_load_failures: 1,
                ..ready
            },
            SettleCounts {
                validation_failures: 1,
                ..ready
            },
            SettleCounts {
                failed_lod_work: 1,
                ..ready
            },
            SettleCounts {
                outstanding_terrain_uploads: 1,
                ..ready
            },
            SettleCounts {
                pending_batch_cpu: 1,
                ..ready
            },
            SettleCounts {
                pending_batch_initial: 1,
                ..ready
            },
            SettleCounts {
                pending_batch_selection: 1,
                ..ready
            },
            SettleCounts {
                pending_specializations: None,
                ..ready
            },
            SettleCounts {
                pending_specializations: Some(1),
                ..ready
            },
            SettleCounts {
                pending_render_transfers: None,
                ..ready
            },
            SettleCounts {
                pending_render_transfers: Some(1),
                ..ready
            },
            SettleCounts {
                space_resident: false,
                ..ready
            },
            SettleCounts {
                renderer_ready: false,
                ..ready
            },
        ];
        for counts in pending {
            assert!(!shots_settled(&counts), "{counts:?}");
            assert_eq!(advance_settle(counts).quiet_frames, 0, "{counts:?}");
        }
    }

    #[test]
    fn a_failed_own_cell_skips_capture_and_keeps_the_remaining_poses() {
        let output = tempfile::tempdir().unwrap();
        let mut file = ShotsFile::from_json(DESIGN_EXAMPLE).unwrap();
        let own = file.shots[0].space_key().unwrap();
        let mut next = file.shots[0].clone();
        next.name = "next".into();
        next.position[0] += CELL_SIZE;
        file.shots.push(next);
        let run = ShotsRun::new(file, output.path().into(), 60);
        let mut app = automation_app(run);
        let mut streaming = StreamingWorld::default();
        streaming.set_failed_for_test(own);
        app.insert_resource(streaming);
        app.update(); // Pose the first shot.
        app.update(); // Refuse its failed cell without requesting an image.
        let run = app.world().resource::<ShotsRun>();
        assert!(run.failed);
        assert_eq!(run.shot, 1);
        assert_eq!(run.phase, Phase::Move);
        assert!(run.log.contains("the cell it stands in failed to load"));
        assert!(run.capture.is_none());
        assert_eq!(
            app.world_mut()
                .query::<&Screenshot>()
                .iter(app.world())
                .count(),
            0
        );
        app.update();
        assert_eq!(app.world().resource::<ShotsRun>().phase, Phase::Settle);
        assert_eq!(app.world().resource::<ShotsRun>().shot, 1);
    }
    fn capture_app() -> (App, tempfile::TempDir, PathBuf, Entity) {
        let output = tempfile::tempdir().expect("temporary screenshot output");
        std::fs::write(
            output.path().join("OWNER.txt"),
            "shots capture regression\n",
        )
        .expect("test output owner");
        let mut file = ShotsFile::from_json(DESIGN_EXAMPLE).expect("fixture parses");
        file.width = 1;
        file.height = 1;
        let own = file.shots[0].space_key().expect("shot space");
        let mut run = ShotsRun::new(file, output.path().to_owned(), 60);
        let path = run.shot_path(&run.file.shots[0]);
        run.enter_settle();
        run.quiet_frames = SETTLE_QUIET_FRAMES;
        let mut streaming = StreamingWorld::default();
        streaming.set_resident_for_test(own, Entity::PLACEHOLDER);
        let mut app = automation_app(run);
        app.insert_resource(streaming).insert_resource(
            bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_secs(3)),
        );
        *app.world_mut()
            .resource::<crate::capture_frame::RetryReadiness>()
            .0
            .lock()
            .unwrap() = Some(0);
        *app.world_mut()
            .resource::<crate::capture_frame::TransferReadiness>()
            .0
            .lock()
            .unwrap() = Some(0);
        app.insert_resource(RendererMetrics {
            gpu_preprocessing_active: true,
            gpu_culling_active: true,
            indirect_drawing_active: true,
            occlusion_culling_views: 1,
            hzb_views: 1,
            indirect_phase_buffers: 1,
            indirect_batch_sets: 1,
            proof_frames: 1,
            ..default()
        });
        app.update(); // First Time<Real> update has zero elapsed time.
        app.update(); // Real warm-up has passed, but the quiet window restarts.
        app.world_mut().resource_mut::<ShotsRun>().quiet_frames = SETTLE_QUIET_FRAMES;
        app.update();
        assert_eq!(app.world().resource::<ShotsRun>().phase, Phase::Capture);
        assert!(!path.exists(), "a request is not a completed disk write");
        let mut screenshots = app.world_mut().query_filtered::<Entity, With<Screenshot>>();
        let request = screenshots.single(app.world()).expect("one shot requested");
        (app, output, path, request)
    }

    fn complete_screenshot(app: &mut App, entity: Entity) {
        complete_screenshot_with(app, entity, |_, _| {});
    }

    fn complete_screenshot_with(
        app: &mut App,
        entity: Entity,
        mutate: impl FnOnce(&mut crate::capture_frame::CaptureFrame, &mut Image),
    ) {
        use bevy::{
            asset::RenderAssetUsages,
            render::render_resource::{Extent3d, TextureDimension, TextureFormat},
        };
        let mut image = Image::new_fill(
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[23, 45, 67, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let run = app.world().resource::<ShotsRun>();
        let shot = &run.file.shots[run.shot];
        let mut receipt = crate::capture_frame::CaptureFrame {
            main_frame: app.world().resource::<bevy::diagnostic::FrameCount>().0,
            render_frame: 1,
            camera_creation: shot.position,
            rotation_runtime_xyzw: shot_camera_rotation(shot.yaw, shot.pitch).to_array(),
            vertical_fov_radians: Some(vertical_fov_degrees(shot.hfov, run.aspect()).to_radians()),
            render_origin: app.world().resource::<RenderOrigin>().0.to_array(),
            pending_specializations: Some(0),
            work: run.counts,
            terrain_coverage: default(),
            terrain_gauges: default(),
            grading: Some(crate::capture_frame::CaptureGrade {
                color_grade: "snapshot-srgb".into(),
                visual_preset: "test".into(),
                preset_saturation: 1.0,
                resolved_grade: serde_json::json!({}),
                source: serde_json::json!({"fixture": true}),
            }),
        };
        mutate(&mut receipt, &mut image);
        app.world()
            .get::<crate::capture_frame::CaptureTicket>(entity)
            .expect("request ticket")
            .set_receipt(receipt);
        app.world_mut()
            .trigger(ScreenshotCaptured { entity, image });
    }

    #[test]
    fn capture_deadline_uses_completed_save_time_instead_of_late_file_polling() {
        // Normal success, a save first completed after the deadline, and a timely save whose
        // completed output the next shot update only observes after a very slow frame.
        for (late_save, late_poll) in [(false, false), (true, false), (false, true)] {
            let (mut app, _output, path, request) = capture_app();
            let over = Duration::from_secs_f32(CAPTURE_TIMEOUT_SECONDS + 1.0);
            if late_save {
                app.world_mut()
                    .resource_mut::<ShotsRun>()
                    .capture
                    .as_mut()
                    .expect("capture request")
                    .requested_at -= over;
            }
            complete_screenshot(&mut app, request);
            let png = std::fs::read(&path).expect("observer wrote PNG");
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            assert!(png.ends_with(b"\0\0\0\0IEND\xae\x42\x60\x82"));
            if late_poll {
                let mut run = app.world_mut().resource_mut::<ShotsRun>();
                let capture = run.capture.as_mut().expect("capture request");
                capture.requested_at -= over;
                capture.completed_at = capture.completed_at.map(|completed| completed - over);
            }
            app.update();
            let run = app.world().resource::<ShotsRun>();
            assert_eq!(run.phase, Phase::Done);
            assert_eq!(run.failed, late_save, "{}", run.log);
            assert_eq!(run.log.contains("did not complete within"), late_save);
            assert_eq!(run.shot, usize::from(!late_save));
        }
    }

    #[test]
    fn a_failed_disk_write_cannot_complete_a_capture() {
        let (mut app, _output, path, request) = capture_app();
        // Create the obstruction after the real system requested the screenshot, so the valid
        // image reaches encoding and the actual filesystem write must fail.
        std::fs::create_dir(&path).expect("obstruct screenshot file with a directory");
        std::fs::write(path.join("OWNER.txt"), "shots failed-write regression\n")
            .expect("test obstruction owner");
        complete_screenshot(&mut app, request);
        {
            let mut run = app.world_mut().resource_mut::<ShotsRun>();
            let capture = run.capture.as_mut().expect("capture request");
            assert!(capture.completed_at.is_none(), "disk write failed");
            capture.requested_at -= Duration::from_secs_f32(CAPTURE_TIMEOUT_SECONDS);
        }
        app.update();
        let run = app.world().resource::<ShotsRun>();
        assert!(run.failed, "an encoded PNG is not a completed disk write");
        assert_eq!(run.phase, Phase::Done);
        assert!(run.log.contains("did not complete within"), "{}", run.log);
        assert!(
            path.is_dir(),
            "the failed request did not replace the obstruction"
        );
    }

    #[test]
    fn an_unconfirmed_file_cannot_pass_an_expired_capture() {
        for failed_event in [false, true] {
            let (mut app, _output, path, request) = capture_app();
            std::fs::write(&path, b"not this request's completed screenshot").expect("late file");
            if failed_event {
                app.world_mut().trigger(ScreenshotCaptured {
                    entity: request,
                    image: Image::default_uninit(),
                });
            }
            {
                let mut run = app.world_mut().resource_mut::<ShotsRun>();
                let capture = run.capture.as_mut().expect("capture request");
                assert!(
                    capture.completed_at.is_none(),
                    "a failed save is not complete"
                );
                capture.requested_at -= Duration::from_secs_f32(CAPTURE_TIMEOUT_SECONDS);
            }
            app.update();
            let run = app.world().resource::<ShotsRun>();
            assert!(run.failed, "file existence cannot bypass the deadline");
            assert_eq!(run.phase, Phase::Done);
            assert!(run.log.contains("did not complete within"), "{}", run.log);

            // A renderer event delivered after the request expired cannot overwrite that path.
            complete_screenshot(&mut app, request);
            assert_eq!(
                std::fs::read(&path).expect("unchanged late file"),
                b"not this request's completed screenshot"
            );
        }
    }

    #[test]
    fn a_view_is_settled_only_when_nothing_is_pending_for_ten_frames() {
        assert!(shots_settled(&quiet_counts()));

        // One frame short of the quiet window.
        assert!(!shots_settled(&SettleCounts {
            quiet_frames: SETTLE_QUIET_FRAMES - 1,
            ..quiet_counts()
        }));

        // Every pending count holds the shot back on its own.
        assert!(
            !shots_settled(&SettleCounts {
                space_resident: false,
                ..quiet_counts()
            }),
            "the shot's own cell must be resident"
        );
        assert!(
            shots_settled(&SettleCounts {
                space_resident: false,
                space_failed: true,
                ..quiet_counts()
            }),
            "or have failed for good: nothing more is coming for it"
        );
        assert!(
            !shots_settled(&SettleCounts {
                space_resident: false,
                space_failed: true,
                loading_cells: 1,
                ..quiet_counts()
            }),
            "a failed cell still waits for the rest of the view"
        );
        assert!(!shots_settled(&SettleCounts {
            loading_cells: 1,
            ..quiet_counts()
        }));
        assert!(!shots_settled(&SettleCounts {
            active_requests: 1,
            ..quiet_counts()
        }));
        assert!(!shots_settled(&SettleCounts {
            pending_asset_instances: 1,
            ..quiet_counts()
        }));
        assert!(!shots_settled(&SettleCounts {
            pending_surface_instances: 1,
            ..quiet_counts()
        }));
        // A surface a view can see that the render world has not confirmed drawn holds the view: it
        // may be photographed as a stand-in or a hole (design-1091 §4).
        assert!(!shots_settled(&SettleCounts {
            pending_drawn: 1,
            ..quiet_counts()
        }));
        // LOD work in flight does too: a chunk on its way changes the frame.
        assert!(!shots_settled(&SettleCounts {
            pending_lod_queries: 1,
            ..quiet_counts()
        }));
        assert!(!shots_settled(&SettleCounts {
            pending_lod_chunks: 1,
            ..quiet_counts()
        }));
        assert!(!shots_settled(&SettleCounts {
            pending_lod_objects: 1,
            ..quiet_counts()
        }));
        // A terminal LOD failure is settled, like a failed cell: nothing more is coming, and the
        // counts that report it (`describe`) do not hold a view that otherwise has everything.
        assert!(shots_settled(&SettleCounts {
            failed_lod_chunks: 1,
            unrecovered_lod_queries: 1,
            unrecovered_lod_chunks: 1,
            ..quiet_counts()
        }));

        assert!(
            !SettleCounts::default().is_quiet(),
            "nothing is resident yet"
        );
        assert!(quiet_counts().is_quiet());
    }

    /// What `pending_drawn` counts: a waiting surface a camera can see ([`SwapInCameraView`]), and
    /// not one it cannot - including one Bevy's light passes marked `ViewVisibility` visible because
    /// it is in a shadow cascade, which no camera draws (the portal-heavy break of 2026-10-09).
    #[test]
    fn settle_counts_counts_a_visible_waiting_surface_and_ignores_an_off_view_one() {
        let mut world = World::new();
        world.spawn((SwapWaiting, SwapInCameraView));
        world.spawn((SwapWaiting, SwapInCameraView));
        world.spawn(SwapWaiting);
        // Visible only to a shadow view: `ViewVisibility` says visible, no camera sees it.
        world.spawn((
            SwapWaiting,
            bevy::camera::visibility::ViewVisibility::VISIBLE,
        ));
        // A camera-visible entity that is not waiting is not a surface at all.
        world.spawn(SwapInCameraView);
        let mut state =
            SystemState::<Query<Has<SwapInCameraView>, With<SwapWaiting>>>::new(&mut world);
        let counts = {
            let query = state
                .get(&world)
                .expect("the state was built from this world");
            settle_counts(None, SETTLE_QUIET_FRAMES, pending_drawn(&query), None, None)
        };
        assert_eq!(
            counts.pending_drawn, 2,
            "the two surfaces a view can see, not the hidden one or the entity that is not waiting"
        );
        // Those two alone hold the settle: everything else is quiet.
        assert!(!shots_settled(&SettleCounts {
            pending_drawn: counts.pending_drawn,
            ..quiet_counts()
        }));
    }

    /// A settle that reaches the window without settling is a hard failure: the reason carries the
    /// counts, no screenshot is asked for (the settle never reports [`SettleStep::Ready`]), and the
    /// run exits non-zero.
    #[test]
    fn a_settle_timeout_fails_the_shot() {
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).expect("the design example parses");
        let shot = &file.shots[0];
        // Every count quiet except a surface the render world never confirms drawn.
        let never = SettleCounts {
            space_resident: true,
            pending_drawn: 1,
            quiet_frames: SETTLE_QUIET_FRAMES,
            ..default()
        };
        assert!(
            !shots_settled(&never),
            "a view with an undrawn visible surface is not settled"
        );

        // Before the window: the shot waits.
        assert_eq!(
            settle_step(shot, &never, false, SETTLE_TIMEOUT_SECONDS - 0.1),
            SettleStep::Waiting
        );
        // A settled view is photographed even once the window has passed.
        assert_eq!(
            settle_step(shot, &never, true, SETTLE_TIMEOUT_SECONDS),
            SettleStep::Ready
        );
        // At the window: the shot fails, with the counts in the reason.
        let step = settle_step(shot, &never, false, SETTLE_TIMEOUT_SECONDS);
        let SettleStep::TimedOut(reason) = step else {
            panic!("a view that never settles must time out, not settle: {step:?}");
        };
        assert!(reason.contains("did not settle"), "{reason}");
        assert!(reason.contains("pending_drawn=1"), "{reason}");

        // The run records the failure and moves on; the settle armed no capture, so no PNG is
        // written for it.
        let mut run = ShotsRun::new(file, tempfile::tempdir().unwrap().keep(), 60);
        run.fail(reason);
        assert!(run.failed);
        assert!(run.log.contains("FAILED"), "{}", run.log);
    }

    /// The blank-PNG path Portal Dev found (2026-10-08): a shot whose own cell never became resident
    /// used to reach the window, be "shot anyway" and write a blank PNG with the run passing. It
    /// fails now, and no screenshot is asked for.
    #[test]
    fn a_shot_whose_cell_never_arrived_fails_instead_of_writing_a_blank_png() {
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).expect("the design example parses");
        let shot = &file.shots[0];
        let never_arrived = SettleCounts {
            space_resident: false,
            loading_cells: 1,
            quiet_frames: 0,
            ..default()
        };
        let step = settle_step(shot, &never_arrived, false, SETTLE_TIMEOUT_SECONDS);
        let SettleStep::TimedOut(reason) = step else {
            panic!("a shot whose cell never arrived must fail, not be photographed: {step:?}");
        };
        assert!(reason.contains("cell_resident=false"), "{reason}");
        assert!(reason.contains("loading_cells=1"), "{reason}");
    }

    /// A shot whose own cell FAILED to load must fail the run, not settle and photograph whatever
    /// is there (review-1111, the C4 follow-up): the cell will never be resident, so the pose would
    /// be a photo of missing ground, and the acceptance path already fails on any failed cell. The
    /// settle fails at once, before the window, shaped like the timeout.
    #[test]
    fn a_shot_whose_own_cell_failed_fails_the_run_without_a_screenshot() {
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).expect("the design example parses");
        let shot = &file.shots[0];
        // Everything quiet except the shot's own cell, which failed for good.
        let failed = SettleCounts {
            space_resident: false,
            space_failed: true,
            quiet_frames: SETTLE_QUIET_FRAMES,
            ..default()
        };
        // This is a settled, quiet view by `is_quiet` alone: the failed-cell rule is the only thing
        // that stops it being photographed.
        assert!(shots_settled(&failed), "the failure is what must stop it");

        // A failed cell is terminal at once, not at the window: it will never arrive.
        let step = settle_step(shot, &failed, false, 0.0);
        let SettleStep::Failed(reason) = step else {
            panic!("a shot whose own cell failed must fail, not settle or time out: {step:?}");
        };
        assert!(reason.contains("failed to load"), "{reason}");
        assert!(reason.contains("cell_failed=true"), "{reason}");

        // The run records the failure and moves on; the settle armed no capture (it never reported
        // `Ready`), so no PNG is written for it.
        let mut run = ShotsRun::new(file, tempfile::tempdir().unwrap().keep(), 60);
        run.fail(reason);
        assert!(run.failed);
        assert!(run.log.contains("FAILED"), "{}", run.log);
    }

    /// A failed cell *elsewhere* in the stream window is not what the shot photographs: its own
    /// cell is resident, so the view is settled and photographed, and the neighbour does not fail
    /// the shot. Built from a real `StreamingWorld` so `space_failed` is the engine's own answer
    /// for the camera's key, not a restatement of it.
    #[test]
    fn a_failed_neighbour_cell_does_not_fail_the_shot() {
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).expect("the design example parses");
        let shot = &file.shots[0];
        let own = shot.space_key().expect("the shot names a space");
        let neighbour = CellKey::Exterior {
            worldspace_id: 0x3C,
            grid_x: 1,
            grid_y: 1,
        };
        let mut world = StreamingWorld::default();
        world.set_resident_for_test(own, Entity::PLACEHOLDER);
        world.set_failed_for_test(neighbour);
        assert!(world.has_failed(&neighbour), "the neighbour really failed");
        assert!(!world.has_failed(&own), "the shot's own cell did not");

        let counts = settle_counts(Some(own), SETTLE_QUIET_FRAMES, 0, Some(&world), None);
        assert!(
            counts.space_resident && !counts.space_failed,
            "{}",
            counts.describe()
        );
        // The view is settled and photographed: the failed neighbour neither holds nor fails it.
        assert!(shots_settled(&counts));
        assert_eq!(settle_step(shot, &counts, true, 0.0), SettleStep::Ready);
    }

    #[test]
    fn a_log_line_carries_the_four_fields_of_the_design() {
        assert_eq!(
            log_line("SR-place-Alftand_02", 2.345, false, 1543),
            "SR-place-Alftand_02 settle=2.35s resident_meshes=1543 timed_out=no"
        );
        assert_eq!(
            log_line("SR-map-Blackreach", 30.0, true, 7),
            "SR-map-Blackreach settle=30.00s resident_meshes=7 timed_out=yes"
        );
    }

    #[test]
    fn a_shots_pose_lands_exactly_where_the_file_asked_and_switches_the_space() {
        // Exercise public pose placement and streaming root rebasing.
        let file = ShotsFile::from_json(&interior_example()).unwrap();
        let shot = &file.shots[0];
        let mut active = ActiveSpace {
            worldspace_id: Some(60),
            interior: None,
        };
        let mut origin = RenderOrigin(IVec2::new(18, 18));
        let mut world = World::new();
        world.spawn((ExteriorCellGrid(IVec2::new(18, 18)), Transform::default()));
        let mut state = SystemState::<crate::streaming::CellRootTransforms>::new(&mut world);
        let mut roots = state.get_mut(&mut world).unwrap();

        let translation = place_shot_space(
            shot.space_key().unwrap(),
            shot.position,
            &mut active,
            &mut origin,
            &mut roots,
        );

        assert_eq!(active.interior, Some(86723));
        assert_eq!(
            origin.0,
            IVec2::new(18, 18),
            "an interior is placed at absolute creation coordinates"
        );
        assert_eq!(
            translation,
            crate::streaming::creation_to_bevy(Vec3::from_array(shot.position)),
            "the camera is at the shot's position exactly, with no eye height"
        );
        let placed: Vec<_> = roots
            .iter()
            .filter_map(|(grid, _, transform)| Some((grid?.0, transform.translation)))
            .collect();
        assert_eq!(
            placed,
            vec![(IVec2::new(18, 18), Vec3::ZERO)],
            "an interior shot does not move exterior roots"
        );
    }

    #[test]
    fn an_exterior_shot_takes_the_render_origin_to_its_own_cell() {
        let file = ShotsFile::from_json(DESIGN_EXAMPLE).unwrap();
        let shot = &file.shots[0];
        let mut active = ActiveSpace {
            worldspace_id: Some(0x1EE62),
            interior: None,
        };
        let mut origin = RenderOrigin(IVec2::new(5, 4));
        let mut world = World::new();
        // The shot is at Creation 77000, 77500: grid (18, 18) of Tamriel.
        world.spawn((
            ExteriorCellGrid(IVec2::new(18, 18)),
            Transform::from_xyz(1.0, 0.0, 1.0),
        ));
        let mut state = SystemState::<crate::streaming::CellRootTransforms>::new(&mut world);
        let mut roots = state.get_mut(&mut world).unwrap();

        let translation = place_shot_space(
            shot.space_key().unwrap(),
            shot.position,
            &mut active,
            &mut origin,
            &mut roots,
        );

        assert_eq!(
            active,
            ActiveSpace {
                worldspace_id: Some(60),
                interior: None
            }
        );
        assert_eq!(origin.0, IVec2::new(18, 18));
        assert_eq!(
            shot.space_key(),
            Some(CellKey::Exterior {
                worldspace_id: 60,
                grid_x: 18,
                grid_y: 18,
            })
        );
        assert_eq!(
            translation,
            Vec3::new(
                77000.0 - 18.0 * CELL_SIZE,
                -5200.0,
                -(77500.0 - 18.0 * CELL_SIZE)
            ),
            "the camera stands at the shot's position in the new render space"
        );
        let placed: Vec<_> = roots
            .iter()
            .filter_map(|(grid, _, transform)| Some((grid?.0, transform.translation)))
            .collect();
        assert_eq!(
            placed,
            vec![(IVec2::new(18, 18), Vec3::ZERO)],
            "the shot's own cell now sits on the render origin"
        );
    }
}
#[cfg(test)]
mod upstream_tests {
    use super::*;
    use bevy::{
        asset::RenderAssetUsages,
        render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    };

    /// A shots file with fields this engine ignores - a long `note` and a comparison tool's own
    /// annotation - and both kinds of shot in one file.
    const REFERENCE_FILE: &str = r#"{
     "width": 1400,
     "height": 1050,
     "shots": [
      {
       "name": "tower-east-0",
       "worldspace_id": 60,
       "interior_cell_id": null,
       "position": [74950, 78431, -5050],
       "yaw": 270.0,
       "pitch": 2.0,
       "hfov": 75.0,
       "reference": "reference/ruined-tower.jpg",
       "note": "Tamriel, the small Dwemer tower east of the ravine",
       "confidence": "matched"
      },
      {
       "name": "y-05f",
       "worldspace_id": null,
       "interior_cell_id": 355355,
       "position": [7900.0, -4450.0, -4500.0],
       "yaw": 270.0,
       "pitch": 80.0,
       "hfov": 75.0
      }
     ]
    }"#;

    fn reference_file() -> ShotsFile {
        ShotsFile::parse(REFERENCE_FILE).expect("the reference shape parses")
    }

    /// A one-shot file with `shot` spliced in as the shot's fields.
    fn one_shot(shot: &str) -> String {
        format!(r#"{{"width": 100, "height": 100, "shots": [{{{shot}}}]}}"#)
    }

    /// Where a camera posed this way looks: a camera faces its own -Z.
    fn forward(rotation: Quat) -> Vec3 {
        rotation * Vec3::NEG_Z
    }

    fn assert_close(actual: Vec3, expected: Vec3) {
        let difference = (actual - expected).abs().max_element();
        assert!(difference < 1.0e-5, "{actual:?} != {expected:?}");
    }

    #[test]
    fn a_yaw_turns_clockwise_from_north() {
        // Creation axes as the runtime sees them: north is -Z, east is +X (shared::coordinates).
        let cases = [
            (0.0, Vec3::new(0.0, 0.0, -1.0)),
            (90.0, Vec3::new(1.0, 0.0, 0.0)),
            (180.0, Vec3::new(0.0, 0.0, 1.0)),
            (270.0, Vec3::new(-1.0, 0.0, 0.0)),
        ];
        for (yaw, expected) in cases {
            assert_close(forward(shot_camera_rotation(yaw, 0.0)), expected);
        }
        // The heading is the converted Creation direction it names, `(sin yaw, cos yaw, 0)`.
        for yaw in [12.0_f32, 137.0, 250.0, 355.0] {
            let radians = yaw.to_radians();
            let axes = [radians.sin(), radians.cos(), 0.0];
            let direction = shared::coordinates::creation_to_runtime_vector(axes);
            let rotation = shot_camera_rotation(yaw, 0.0);
            assert_close(forward(rotation), Vec3::from_array(direction).normalize());
        }
    }

    #[test]
    fn a_positive_pitch_looks_down() {
        for pitch in [5.0_f32, 30.0, 80.0] {
            let radians = pitch.to_radians();
            let expected = Vec3::new(0.0, -radians.sin(), -radians.cos());
            assert_close(forward(shot_camera_rotation(0.0, pitch)), expected);
        }
        assert!(forward(shot_camera_rotation(0.0, -30.0)).y > 0.0);
        // Yaw and pitch together: a heading of 90 (east) pitched 30 down looks east and down.
        let pitched = 30.0_f32.to_radians();
        let expected = Vec3::new(pitched.cos(), -pitched.sin(), 0.0);
        assert_close(forward(shot_camera_rotation(90.0, 30.0)), expected);
    }

    #[test]
    fn the_vertical_field_of_view_follows_the_aspect() {
        assert!((vertical_fov_degrees(75.0, 1.0) - 75.0).abs() < 1.0e-4);
        let vertical = vertical_fov_degrees(75.0, 4.0 / 3.0);
        assert!(
            (vertical - 59.83).abs() < 0.05,
            "75 degrees at 4:3 is {vertical}"
        );
        assert!(vertical_fov_degrees(75.0, 2.0) < vertical);
        assert!(vertical_fov_degrees(75.0, 0.75) > vertical);
        assert!((vertical_fov_degrees(90.0, 2.0) - 53.130_1).abs() < 1.0e-3);
    }

    #[test]
    fn a_pose_lands_where_the_streamer_measures_it() {
        // Creation (x, y, z) is runtime (x, z, -y), and the streamer reads a camera back as
        // `translation + origin * CELL`.
        let position = [74_950.0, 78_431.0, -5_050.0];
        for (x, y) in [(0, 0), (18, 19), (-4, 7)] {
            let translation = shot_camera_translation(position, IVec2::new(x, y));
            let world_x = translation.x + x as f32 * CELL_SIZE;
            let world_y = -translation.z + y as f32 * CELL_SIZE;
            assert!((world_x - position[0]).abs() < 1.0e-2);
            assert!((world_y - position[1]).abs() < 1.0e-2);
            assert!((translation.y - position[2]).abs() < 1.0e-2);
            // The streamer's own reading of that camera is the shot's grid square.
            let center = crate::streaming::streaming_center(translation, IVec2::new(x, y));
            let grid = Shot {
                position,
                ..reference_file().shots[0].clone()
            }
            .grid();
            assert_eq!((center.x, center.y), grid);
        }
    }

    #[test]
    fn the_reference_shape_parses_and_names_where_the_run_starts() {
        let file = reference_file();
        assert_eq!((file.width, file.height), (1400, 1050));
        assert_eq!(file.shots.len(), 2);

        let exterior = &file.shots[0];
        assert_eq!(exterior.worldspace_id, Some(60));
        assert!(exterior.is_exterior());
        assert_eq!(exterior.grid(), (18, 19));
        let expected = "reference/ruined-tower.jpg";
        assert_eq!(exterior.reference.as_deref(), Some(expected));
        assert!(!file.shots[1].is_exterior());

        // The run starts on the first exterior shot, whatever order the kinds come in.
        assert_eq!(file.start(), Some((Some(60), (18, 19))));
        let interiors_only = ShotsFile {
            shots: vec![file.shots[1].clone()],
            ..file.clone()
        };
        assert_eq!(interiors_only.start(), None);

        // A negative position floors into the square below zero.
        let west = Shot {
            position: [-1.0, -4097.0, 0.0],
            ..exterior.clone()
        };
        assert_eq!(west.grid(), (-1, -2));

        // The window is exactly the file's frame, whatever the display's scale factor.
        let resolution = file.window_resolution();
        let physical = (resolution.physical_width(), resolution.physical_height());
        assert_eq!(physical, (1400, 1050));
    }

    #[test]
    fn a_malformed_file_is_an_error_that_says_what_is_wrong() {
        let good = r#""name": "x", "position": [0, 0, 0], "yaw": 0, "pitch": 0, "hfov": 75"#;
        let broken = [
            (String::new(), "not a shots file"),
            ("not json".to_owned(), "not a shots file"),
            ("[]".to_owned(), "not a shots file"),
            (r#"{"width": 100, "height": 100}"#.to_owned(), "shots"),
            (
                r#"{"width": -1, "height": 100, "shots": []}"#.to_owned(),
                "not a shots file",
            ),
            (
                r#"{"width": 0, "height": 10, "shots": []}"#.to_owned(),
                "frame",
            ),
            (
                r#"{"width": 8193, "height": 100, "shots": []}"#.to_owned(),
                "8192",
            ),
            (
                r#"{"width": 100, "height": 9000, "shots": []}"#.to_owned(),
                "8192",
            ),
            (
                r#"{"width": 10, "height": 10, "shots": []}"#.to_owned(),
                "no shots",
            ),
            (
                one_shot(r#""name": "x", "position": [0, 0], "yaw": 0, "pitch": 0, "hfov": 75"#),
                "not a shots file",
            ),
            (
                one_shot(r#""name": "x", "position": [0, 0, 0], "pitch": 0, "hfov": 75"#),
                "yaw",
            ),
            (
                one_shot(
                    r#""name": "x", "position": [0, 0, 0], "yaw": "north", "pitch": 0, "hfov": 75"#,
                ),
                "not a shots file",
            ),
            (
                one_shot(
                    r#""name": "x", "position": [0, 0, 0], "yaw": 0, "pitch": 0, "hfov": 180"#,
                ),
                "field of view",
            ),
            (
                one_shot(
                    r#""name": "x", "position": [0, 0, 0], "yaw": 0, "pitch": 0, "hfov": 1e39"#,
                ),
                // float_roundtrip rejects f32 overflow before Shot::validate can see it.
                "number out of range",
            ),
            (
                one_shot(
                    r#""name": "x", "position": [0, 0, 0], "yaw": 0, "pitch": 91, "hfov": 75"#,
                ),
                "pitch",
            ),
            (
                one_shot(
                    r#""name": "x", "position": [0, 0, 0], "yaw": 0, "pitch": -90.5, "hfov": 75"#,
                ),
                "pitch",
            ),
            (
                one_shot(
                    r#""name": "x", "position": [0, 0, 0], "yaw": 0, "pitch": 1e9, "hfov": 75"#,
                ),
                "pitch",
            ),
            (
                one_shot(r#""name": "", "position": [0, 0, 0], "yaw": 0, "pitch": 0, "hfov": 75"#),
                "no name",
            ),
            (
                one_shot(
                    r#""name": "../escape", "position": [0, 0, 0], "yaw": 0, "pitch": 0, "hfov": 75"#,
                ),
                "plain file name",
            ),
            (
                format!(r#"{{"width": 10, "height": 10, "shots": [{{{good}}}, {{{good}}}]}}"#),
                "two shots are named",
            ),
            (
                format!(
                    r#"{{"width": 10, "height": 10, "shots": [{{{good}}}, {{{}}}]}}"#,
                    good.replace(r#""name": "x""#, r#""name": "X""#)
                ),
                "ignoring case",
            ),
        ];
        for (text, expected) in broken {
            let error = ShotsFile::parse(&text).expect_err("this is not a shots file");
            let message = error.to_string();
            assert!(
                message.contains(expected),
                "{expected:?} is missing from {message:?}"
            );
        }
        assert!(ShotsFile::parse(&one_shot(good)).is_ok());
    }

    #[test]
    fn a_non_finite_programmatic_field_of_view_is_rejected() {
        let valid = reference_file().shots[0].clone();
        for hfov in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let shot = Shot {
                hfov,
                ..valid.clone()
            };
            let message = shot
                .validate()
                .expect_err("a programmatic non-finite pose is invalid too")
                .to_string();
            assert!(message.contains("not a number"), "{message}");
        }
    }

    #[test]
    fn names_windows_cannot_create_are_refused_before_the_window_opens() {
        let pose = r#""position": [0, 0, 0], "yaw": 0, "pitch": 0, "hfov": 75"#;
        let refused = [
            ("a<b", "plain file name"),
            ("a>b", "plain file name"),
            ("a\"b", "plain file name"),
            ("a|b", "plain file name"),
            ("a?b", "plain file name"),
            ("a*b", "plain file name"),
            ("trailing.", "ends in a dot"),
            ("trailing ", "ends in a dot"),
            ("CON", "reserved device name"),
            ("con", "reserved device name"),
            ("con.png", "reserved device name"),
            ("Nul.x", "reserved device name"),
            ("COM1", "reserved device name"),
            ("lpt9.png", "reserved device name"),
        ];
        for (name, expected) in refused {
            let quoted = serde_json::to_string(name).unwrap();
            let text = one_shot(&format!("\"name\": {quoted}, {pose}"));
            let error = ShotsFile::parse(&text).expect_err("Windows cannot create this name");
            let message = error.to_string();
            assert!(message.contains(expected), "{name:?} is missing: {message}");
        }
        // Names that only look like a device name, or carry an extension, are ordinary files.
        for name in ["console", "com0", "lpt10", "auxiliary", "tower.east", "a b"] {
            let quoted = serde_json::to_string(name).unwrap();
            let text = one_shot(&format!("\"name\": {quoted}, {pose}"));
            assert!(ShotsFile::parse(&text).is_ok(), "{name:?} is a file name");
        }
    }

    #[test]
    fn a_frame_at_the_size_cap_is_accepted() {
        let shot = r#""name": "cap", "position": [0, 0, 0], "yaw": 0, "pitch": 0, "hfov": 75"#;
        let text = format!(
            r#"{{"width": {MAX_FRAME_SIDE_PIXELS}, "height": {MAX_FRAME_SIDE_PIXELS}, "shots": [{{{shot}}}]}}"#
        );
        let file = ShotsFile::parse(&text).expect("the cap itself is a frame the window can be");
        assert_eq!(
            (file.width, file.height),
            (MAX_FRAME_SIDE_PIXELS, MAX_FRAME_SIDE_PIXELS)
        );
    }

    #[test]
    fn each_log_line_reaches_the_file_as_the_shot_ends() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("out");
        fs::create_dir_all(&output).unwrap();
        let path = output.join("shots.log");
        // What an earlier run left there is replaced by this run's first line, not appended to.
        fs::write(&path, "stale from an earlier run\n").unwrap();
        let mut run = ShotsRun::new(reference_file(), output, 60);

        run.note("tower-east-0 frames=10 settled path=out/tower-east-0.png");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "tower-east-0 frames=10 settled path=out/tower-east-0.png\n"
        );
        // A failure's line is in the file the moment it happens: a killed run keeps it.
        run.fail("shot \"y-05f\": no screenshot reached the disk");
        assert!(run.failed);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "tower-east-0 frames=10 settled path=out/tower-east-0.png\n\
             FAILED: shot \"y-05f\": no screenshot reached the disk\n"
        );
    }

    #[test]
    fn a_previous_shot_that_cannot_be_removed_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(
            discard_previous_shot(&directory.path().join("missing.png")),
            Ok(())
        );
        let stale = directory.path().join("stale.png");
        fs::write(&stale, b"old").unwrap();
        assert_eq!(discard_previous_shot(&stale), Ok(()));
        assert!(!stale.exists());
        // A folder where the image goes cannot be removed as a file, like a PNG a viewer holds.
        let blocked = directory.path().join("blocked.png");
        fs::create_dir(&blocked).unwrap();
        let message = discard_previous_shot(&blocked).expect_err("a folder is not removed");
        assert!(message.contains("blocked.png"), "{message}");
    }

    #[test]
    fn a_file_that_cannot_be_read_names_its_path() {
        let path = Path::new("no-such-folder/no-such-shots.json");
        let message = ShotsFile::load(path).expect_err("missing").to_string();
        assert!(message.contains("no-such-shots.json"), "{message}");
    }

    #[test]
    fn the_output_directory_defaults_to_the_file_stem_shots() {
        let cases = [
            (
                "reference/riverwood_shots.json",
                "reference/riverwood_shots-shots",
            ),
            ("shots.json", "shots-shots"),
            ("poses/east/tower_east.json", "poses/east/tower_east-shots"),
        ];
        for (path, expected) in cases {
            assert_eq!(default_output_dir(Path::new(path)), PathBuf::from(expected));
        }
    }

    #[test]
    fn interiors_and_other_worldspaces_are_skipped_with_a_reason() {
        let file = reference_file();
        let exterior = &file.shots[0];
        assert_eq!(skip_reason(exterior, 60), None);
        assert_eq!(skip_reason(exterior, 0x3c), None, "0x3c is 60");
        let other = skip_reason(exterior, 0x0001_EE62).expect("another worldspace is skipped");
        assert!(other.contains("0000003C"), "{other}");
        assert_eq!(skip_reason(&file.shots[1], 60).as_deref(), Some("interior"));
        // A shot that names no worldspace takes the one being streamed.
        let unnamed = Shot {
            worldspace_id: None,
            ..exterior.clone()
        };
        assert_eq!(skip_reason(&unnamed, 0x0001_EE62), None);
    }

    #[test]
    fn the_settle_rule_needs_a_quiet_window_and_every_count_at_zero() {
        let ready = SettleCounts {
            scope: SettleScope::Capture,
            renderer_ready: true,
            pending_specializations: Some(0),
            pending_render_transfers: Some(0),
            ..SettleCounts::default()
        };
        assert!(
            !shots_settled(&ready),
            "a window of no frames is not quiet yet"
        );
        let quiet = SettleCounts {
            quiet_frames: SETTLE_QUIET_FRAMES,
            ..ready
        };
        assert!(shots_settled(&quiet));
        let short = SettleCounts {
            quiet_frames: SETTLE_QUIET_FRAMES - 1,
            ..ready
        };
        assert!(!shots_settled(&short));

        // Each kind of pending work on its own holds the shot back and starts the window again.
        let pending = [
            SettleCounts {
                loading_cells: 1,
                ..ready
            },
            SettleCounts {
                active_requests: 1,
                ..ready
            },
            SettleCounts {
                pending_asset_instances: 3,
                ..ready
            },
            SettleCounts {
                pending_surface_instances: 1,
                ..ready
            },
            SettleCounts {
                arming_queue_depth: 2,
                ..ready
            },
            SettleCounts {
                retiring_cells: 1,
                ..ready
            },
            SettleCounts {
                renderer_ready: false,
                ..ready
            },
            // A failure is not pending work that will clear: the view is held back and the
            // timeout gives up on the shot, rather than the shot being logged as settled.
            SettleCounts {
                failed_cells: 1,
                ..ready
            },
            SettleCounts {
                asset_load_failures: 1,
                ..ready
            },
            SettleCounts {
                validation_failures: 1,
                ..ready
            },
        ];
        for counts in pending {
            assert!(!counts.is_quiet(), "{counts:?}");
            let counted = advance_settle(SettleCounts {
                quiet_frames: SETTLE_QUIET_FRAMES,
                ..counts
            });
            assert_eq!(counted.quiet_frames, 0, "the window starts again");
            assert!(!shots_settled(&counted));
        }

        // The window counts up while quiet, and reaches a settled view after ten frames.
        let mut counts = ready;
        for frame in 1..=SETTLE_QUIET_FRAMES {
            counts = advance_settle(counts);
            assert_eq!(counts.quiet_frames, frame);
            assert_eq!(shots_settled(&counts), frame >= SETTLE_QUIET_FRAMES);
        }

        // The streamer's and renderer's metrics are read field by field, failures included.
        let metrics = StreamingMetrics {
            loading_cells: 1,
            active_requests: 2,
            pending_asset_instances: 3,
            pending_surface_instances: 4,
            arming_queue_depth: 5,
            retiring_cells: 6,
            failed_cells: 1,
            asset_load_failures: 2,
            material_validation_failures: 1,
            terrain_validation_failures: 1,
            water_validation_failures: 1,
            transform_bounds_validation_failures: 1,
            failed_lod_queries: 2,
            failed_lod_chunks: 3,
            ..StreamingMetrics::default()
        };
        let renderer = RendererMetrics {
            renderer_validation_failures: 1,
            ..RendererMetrics::default()
        };
        let read = SettleCounts::read(&metrics, &renderer, Failures::default(), true, 7);
        let expected = SettleCounts {
            loading_cells: 1,
            active_requests: 2,
            pending_asset_instances: 3,
            pending_surface_instances: 4,
            arming_queue_depth: 5,
            retiring_cells: 6,
            failed_cells: 1,
            asset_load_failures: 2,
            validation_failures: 5,
            failed_lod_work: 5,
            scope: SettleScope::Capture,
            renderer_ready: true,
            quiet_frames: 7,
            ..default()
        };
        assert_eq!(read, expected);
        assert!(!read.is_quiet(), "a failed cell is not a settled view");

        // A failure that was already counted when the shot started settling is history, not a
        // reason to hold the shot back; one that happens during the settle is.
        let baseline = Failures::read(&metrics, &renderer);
        let quiet = SettleCounts::read(&metrics, &renderer, baseline, true, 0);
        assert_eq!(
            (
                quiet.failed_cells,
                quiet.asset_load_failures,
                quiet.validation_failures,
                quiet.failed_lod_work
            ),
            (0, 0, 0, 0)
        );
        let idle = StreamingMetrics {
            failed_cells: metrics.failed_cells,
            asset_load_failures: metrics.asset_load_failures,
            material_validation_failures: metrics.material_validation_failures,
            terrain_validation_failures: metrics.terrain_validation_failures,
            water_validation_failures: metrics.water_validation_failures,
            transform_bounds_validation_failures: metrics.transform_bounds_validation_failures,
            failed_lod_queries: metrics.failed_lod_queries,
            failed_lod_chunks: metrics.failed_lod_chunks,
            ..StreamingMetrics::default()
        };
        let mut idle_counts = SettleCounts::read(&idle, &renderer, baseline, true, 0);
        idle_counts.pending_specializations = Some(0);
        idle_counts.pending_render_transfers = Some(0);
        assert!(idle_counts.is_quiet());
        let later = StreamingMetrics {
            failed_cells: metrics.failed_cells + 1,
            failed_lod_chunks: metrics.failed_lod_chunks + 1,
            ..idle
        };
        let fresh = SettleCounts::read(&later, &renderer, baseline, true, 0);
        assert_eq!((fresh.failed_cells, fresh.failed_lod_work), (1, 1));
        assert!(!fresh.is_quiet());
    }

    /// The baseline frozen into a capture request is the one its receipt is judged against, so
    /// the settled frame's own check sees the same failures as the settle rule did.
    #[test]
    fn a_capture_request_freezes_the_shot_failure_baseline() {
        let output = tempfile::tempdir().unwrap();
        let mut run = ShotsRun::new(reference_file(), output.path().into(), 60);
        run.failure_baseline = Failures {
            failed_cells: 1,
            asset_load_failures: 2,
            validation_failures: 3,
            failed_lod_work: 4,
        };
        let mut world = bevy::ecs::world::World::new();
        {
            let mut commands = world.commands();
            request_capture(&mut commands, &mut run, 7, &Handle::<Image>::default());
        }
        world.flush();
        let mut query = world.query::<&crate::capture_frame::CaptureTicket>();
        let ticket = query
            .single(&world)
            .expect("the request spawned its ticket");
        assert_eq!(ticket.baseline(), run.failure_baseline);
        assert!(ticket.receipt().is_none(), "nothing has been captured yet");
        assert_eq!(run.phase, Phase::Capture);
        assert!(!output.path().join("tower-east-0.png").exists());
    }

    #[test]
    fn settle_requires_lod_upload_and_render_retry_acknowledgments() {
        let ready = SettleCounts {
            scope: SettleScope::Capture,
            renderer_ready: true,
            pending_specializations: Some(0),
            pending_render_transfers: Some(0),
            quiet_frames: 10,
            ..default()
        };
        assert!(shots_settled(&ready));
        for counts in [
            SettleCounts {
                pending_lod_queries: 1,
                ..ready
            },
            SettleCounts {
                pending_lod_chunks: 1,
                ..ready
            },
            SettleCounts {
                outstanding_terrain_uploads: 1,
                ..ready
            },
            SettleCounts {
                pending_batch_cpu: 1,
                ..ready
            },
            SettleCounts {
                pending_batch_initial: 1,
                ..ready
            },
            SettleCounts {
                pending_batch_selection: 1,
                ..ready
            },
            SettleCounts {
                pending_specializations: Some(1),
                ..ready
            },
            SettleCounts {
                pending_specializations: None,
                ..ready
            },
            SettleCounts {
                pending_render_transfers: Some(1),
                ..ready
            },
            SettleCounts {
                pending_render_transfers: None,
                ..ready
            },
        ] {
            assert!(!shots_settled(&counts));
            assert_eq!(advance_settle(counts).quiet_frames, 0);
        }
    }

    /// An image the size a shot's capture arrives at, so the writer can be exercised without a
    /// window or a GPU.
    fn test_image() -> Image {
        Image::new(
            Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![0x40; 2 * 2 * 4],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD,
        )
    }

    #[test]
    fn a_captured_image_is_written_and_a_write_that_fails_is_an_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shot.png");
        write_shot(test_image(), &path).expect("the shot's PNG is written");
        assert!(path.is_file(), "the PNG is on disk");
        assert!(png_is_complete(&path), "the PNG is a whole stream");
        assert!(
            !partial_path(&path).exists(),
            "the temporary file was renamed into place"
        );

        // A shot named `a.partial` beside one named `a`: writing `a` must not touch the other
        // shot's image, so the temporary name can never be a shot's own file name.
        let other_shot = directory.path().join("a.partial.png");
        write_shot(test_image(), &other_shot).expect("the a.partial shot is written");
        let before = fs::read(&other_shot).unwrap();
        write_shot(test_image(), &directory.path().join("a.png")).expect("the a shot is written");
        assert_eq!(
            fs::read(&other_shot).expect("a.partial's image is still there"),
            before,
            "writing shot a left shot a.partial's image alone"
        );
        let stem = partial_path(&directory.path().join("a.png"))
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let pose = r#""position": [0, 0, 0], "yaw": 0, "pitch": 0, "hfov": 75"#;
        let quoted = serde_json::to_string(&stem).unwrap();
        assert!(
            ShotsFile::parse(&one_shot(&format!("\"name\": {quoted}, {pose}"))).is_err(),
            "the temporary stem {stem:?} must be a name the shots file refuses"
        );

        // A directory that is not there: the save's own error is the shot's failure - Bevy's
        // `save_to_disk` would have logged it and left the shot looking settled.
        let missing = directory.path().join("no-such-folder").join("shot.png");
        let message = write_shot(test_image(), &missing).expect_err("nothing was written");
        assert!(message.contains("could not be written"), "{message}");
        assert!(
            !missing.exists(),
            "a failed save leaves nothing at the shot's path"
        );

        // A file that is there but is not a whole PNG - the 0-byte or truncated file a failed
        // save leaves - is not a shot.
        let empty = directory.path().join("empty.png");
        fs::write(&empty, b"").unwrap();
        assert!(!png_is_complete(&empty));
        let truncated = directory.path().join("truncated.png");
        fs::write(&truncated, b"\x89PNG\r\n\x1a\n").unwrap();
        assert!(!png_is_complete(&truncated));
        let other = directory.path().join("other.png");
        fs::write(
            &other,
            b"not a png at all, but long enough to have a head and a tail",
        )
        .unwrap();
        assert!(!png_is_complete(&other));
    }

    #[test]
    fn a_log_line_says_what_the_shot_did() {
        let file = reference_file();
        let shot = file.shots[0].clone();
        let output = PathBuf::from("reference/riverwood_shots-shots");
        let mut run = ShotsRun::new(file, output, 60);
        let path = run.shot_path(&shot);
        let display = path.display();

        run.frames = 42;
        let expected = format!("tower-east-0 frames=42 settled path={display}");
        assert_eq!(run.log_line(&shot, &path), expected);

        run.frames = 1_812;
        run.timed_out = true;
        run.counts = SettleCounts {
            loading_cells: 2,
            pending_asset_instances: 7,
            ..SettleCounts::default()
        };
        let timed_out = run.log_line(&shot, &path);
        assert!(timed_out.starts_with("tower-east-0 frames=1812 timed_out ("));
        assert!(timed_out.contains("loading_cells=2"));
        assert!(timed_out.contains("pending_assets=7"));
        assert!(timed_out.ends_with(&format!("path={display}")));
        assert_eq!(
            timed_out.lines().count(),
            1,
            "one line per shot: {timed_out}"
        );

        // The window's size is on the line when it is not the file's frame - the image is then not
        // the shape the reference is - and absent when it is.
        run.timed_out = false;
        run.window_size = Some((1400, 1050));
        let same = run.log_line(&shot, &path);
        assert!(!same.contains("window="), "the file's frame needs no note");
        run.window_size = Some((1920, 1080));
        let resized = run.log_line(&shot, &path);
        assert!(resized.ends_with(" window=1920x1080"), "{resized}");
        assert_eq!(resized.lines().count(), 1, "one line per shot: {resized}");
    }
}
