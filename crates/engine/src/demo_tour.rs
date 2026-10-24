//! A scripted walk through a demo's route (`--demo-tour <dir>`).
//!
//! It proves the demo end to end without a person at the keyboard: for each of the route's load
//! doors it waits for the world to settle, takes a screenshot, moves the camera in front of the
//! door, waits for the door's destination to pre-stream, sends [`ActivateDoor`], and takes a
//! screenshot on the far side. A `tour.txt` log records every step, and the app exits at the end.
//!
//! Which doors those are is the demo the run started in ([`route_for_run`]): the Alftand ->
//! Blackreach descent, or Riverwood's four houses, each entered and left again. A run with no
//! scripted route of its own walks the Alftand route, as `--demo-tour` always has. Alftand's Door
//! FormIDs come from `docs/research/worldspace-transition-demo.md`, section 2.2; Riverwood's from
//! the door links measured in `docs/design/riverwood-demo.md`, sections 2 and 3.
//!
//! The tour activates every door itself, exactly as before, so the run stays deterministic however
//! a person would cross it; the log says for each route door whether it is an auto-load marker
//! (which a player crosses by walking into it, see `crate::player`) or one that needs `E`.
//!
//! # The walk-through
//!
//! With a player driving the camera (`--walk --demo-tour <dir>`) the tour does not activate the
//! door at all: it does what a player does at the door - presses `E` once, stands still until the
//! door is open, then holds `W` through the doorway - and photographs every frame around the
//! crossing, ten frames before the swap and ten after it, into `walk-through/<stage>/` (with
//! `frames.txt`, the eye and the view direction of each of them).
//!
//! Why the walk waits ([`WalkAction`]): the leaf is solid until its swing is complete, so `W`
//! before the state is [`DoorState::Open`] is walking into the leaf. The press is made once and
//! never repeated, because `E` on an open door closes it and a second press cancels an opening
//! whose destination is still streaming in (`crate::door_animation`). It is also made from a
//! standoff the key reaches the door from ([`standoff_reaches_door`]): a key press is not held for
//! later, and a door that cannot be pressed from where the walk stands is a door that never opens.
//! A door that does not open within [`WALK_OPEN_SECONDS`] fails the stage.
//!
//! That is the check the seamless crossing needs. There is no load screen and no snap, so the
//! frames on either side of the swap have to be the same view of the same room: the window of
//! them tiles into one contact sheet to look at.
//!
//! # The two door checks
//!
//! The walk-through also judges the doors themselves, one line per door in `tour.txt`, because two
//! door defects the user found in play passed every tour this repo ran (`docs/research/
//! checks-for-user-found-defects.md`):
//!
//! - **The swing** (`SwingWatch`): a door whose model has an `Open` clip has to play it - the
//!   state passes through [`DoorState::Opening`], the leaf swings - and must never open with
//!   nothing to animate, which hides the whole door model and leaves a hole where the doorway was
//!   (the user, 2026-09-24; fixed in `d420bf9`).
//! - **The far door** (`far_door`): a crossing through an anchored doorway lands the player in the
//!   destination doorway, so the far door of the link has to be open a few frames after the swap
//!   (`FAR_DOOR_FRAMES`) - without that the player arrives inside a closed leaf, which is what
//!   impl-152 fixed.
//!
//! Both are read from the same door state the portal and the crossing read, so a tour that passes
//! them has watched the door do the thing rather than photographed it afterwards.
//!
//! # Waiting, and the short tour
//!
//! A stage waits for the place it is standing in to stream in, on the same counts a `--shots` pose
//! settles on ([`crate::shots`]); exceeding `SETTLE_SECONDS` fails without a capture. Initial
//! package certification pauses automation while the interactive world stays visible. Every other
//! wait is a fraction of a second, long enough for the frame a screenshot was asked for to be
//! drawn. The walk-through keeps a ring of the frames it takes on its way in and the window around
//! the swap, so a tour leaves the frames it is judged on and not a thousand more.
//!
//! `--tour-doors N` walks the first `N` doors of the route and stops: a short tour, which skips
//! the route-end look-around and the walk test and ends `tour PASSED after N crossings (short
//! tour)` or `FAILED`. Since 2026-09-25 a short tour of 1 or 2 doors is the standing automated
//! check (the user's call: the full route is more doors than the check needs); the full tour -
//! every door, the walk test and the verdict - is unchanged by it and still there when wanted.
//!
//! `--tour-repeat N` walks that same sequence `N` times over rather than once
//! ([`tour_crossings`], [`door_at_stage`]): the tour goes in and out of the same house again and
//! again, which is what reproducing a fault that only shows after a run of crossings needs. Every
//! repeat photographs its own `NN-arrived.png` on the way, so the outside after each return is
//! compared with the outside after the first one.
//!
//! # The doorway bench
//!
//! `--tour-bench <file.csv>` times the portal's cost ([`BenchState`]). At each outside door of the
//! route - the tour standing in an exterior, in front of an `E` door, after its pre-stream wait and
//! before the walk presses `E` - the view is held still and the frames are timed for
//! [`BENCH_SECONDS`], after [`BENCH_SETTLE_SECONDS`], three ways: the door closed; the door fully
//! open with the doorway on screen; and the door still open with the view turned half a turn, so
//! the doorway is off screen. The tour then walks through the (already open) door as usual. One
//! CSV row per door and state is written when the tour ends, and `tour.txt` gets one summary line
//! per state, averaged over the doors, before its verdict line. The frame times are wall-clock
//! (`Time<Real>`), so a bench run is a timing run only when this engine runs alone on the machine.

use crate::{
    cluster::package::PackageValidationStatus,
    config::EngineConfig,
    doors::{ActivateDoor, DoorAnchor, DoorCrossed, DoorState, LoadDoor},
    metrics::percentile,
    player::{Player, PlayerInput, door_aim_point, target_door},
    seamless_door_animation::DoorAnimation,
    shots::{
        SettleCounts, initial_cluster_load_ready, settle_counts, shot_camera_rotation,
        shots_settled,
    },
    streaming::{
        ActiveCell, RenderOrigin, StreamingMetrics, StreamingWorld, camera_grid, creation_to_bevy,
        render_position,
    },
    swap_bridge::{WaitingVisibility, pending_drawn},
    transition::{OpenDoor, distance_in_front_of_door, door_frame, door_is_open},
    world::{
        components::{ExpectedModelBounds, InstanceBounds, StreamingCamera},
        database::CellKey,
    },
};
use bevy::{
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    },
};
use std::{fmt::Write as _, path::PathBuf};

/// The four load doors from the Alftand entrance down to Blackreach, in order: a descent, each door
/// leaving the place the one before it arrived in.
pub const ALFTAND_ROUTE: [u32; 4] = [0x0001_5D48, 0x0009_2809, 0x0009_256A, 0x0006_998D];

/// The eight load doors of the Riverwood route, in order: the four houses on the village street,
/// each entered and left again, so the tour is back in Tamriel before it walks on to the next
/// (`docs/design/riverwood-demo.md`, section 3).
///
/// The doors pair up: `2k` is the door into a house and `2k + 1` the door inside it that leads back
/// out - which stands in the cell the crossing into `2k` arrived in, so the tour always finds the
/// next door in the space it is standing in.
const RIVERWOOD_ROUTE: [u32; 8] = [
    0x0001_CBB0, // into Sven's House (interior 0001CB84)
    0x0001_CBAF, // and back out to Tamriel
    0x0001_341F, // into the Riverwood Trader (000133C9)
    0x0001_33E9, // and back out
    0x0001_3420, // into Alvor and Sigrid's House (000133C8)
    0x0001_33FB, // and back out
    0x0001_3424, // into the Sleeping Giant Inn (000133C6)
    0x0001_3419, // and back out
];

/// A scripted route: the load doors to cross, in order.
pub struct DemoRoute {
    /// The route's own name, for the log: the demo that walks it is not always the same word
    /// (`blackreach` starts at the far end of the `alftand` route).
    pub name: &'static str,
    pub doors: &'static [u32],
    /// What the countdown between crossings counts toward, named in the route's own words.
    pub destination: &'static str,
    /// The line shown once every door of the route has been crossed.
    pub finale: &'static str,
}

/// The Alftand -> Blackreach descent: four one-way doors, no place visited twice.
static ALFTAND: DemoRoute = DemoRoute {
    name: "alftand",
    doors: &ALFTAND_ROUTE,
    destination: "Blackreach",
    finale: "You made it: Blackreach. No loading screens. Explore on foot (F to fly).",
};

/// Riverwood: four houses on the village street, each entered and left again.
static RIVERWOOD: DemoRoute = DemoRoute {
    name: "riverwood",
    doors: &RIVERWOOD_ROUTE,
    destination: "the end of the tour",
    finale: "All four houses, and no loading screen between them. Riverwood is yours (F to fly).",
};

/// The `--demo` names that have a scripted route, and the route each one walks.
static DEMO_ROUTES: [(&str, &DemoRoute); 3] = [
    ("alftand", &ALFTAND),
    // The Blackreach demo starts inside Blackreach, at the arrival point of the Alftand route's
    // last door, and walks the same four back up: one route, two starting points.
    ("blackreach", &ALFTAND),
    ("riverwood", &RIVERWOOD),
];

/// The route for a `--demo <name>` run; `None` when the name has no scripted route.
pub fn route_for_demo(name: Option<&str>) -> Option<&'static DemoRoute> {
    let name = name?;
    DEMO_ROUTES
        .iter()
        .find(|(demo, _)| *demo == name)
        .map(|(_, route)| *route)
}

/// The route a run follows: the scripted route of the demo it started in, or the Alftand route when
/// it started in none - a `--start-position` run, a benchmark, or an unknown `--demo` name, which
/// `crate::config` leaves as no demo at all. That fallback is the route `--demo-tour` has always
/// walked, so a run without a route of its own is unchanged.
pub fn route_for_run(demo: Option<&str>) -> &'static DemoRoute {
    route_for_demo(demo).unwrap_or(&ALFTAND)
}

/// The longest a freshly entered place may take to stream in before the tour fails without a
/// screenshot. [`Phase::Settle`] succeeds after streaming has been quiet for
/// [`SETTLE_QUIET_FRAMES`](crate::shots::SETTLE_QUIET_FRAMES) frames. Initial package certification
/// is a separate readiness gate and does not spend this settle window.
///
/// The condition is the one a `--shots` pose settles on (`crate::shots`), and it is both quicker
/// and stricter than the flat sleep it replaces: a place already streamed in - every stage but the
/// first, since a door's destination is pre-streamed before the crossing - settles in a few frames,
/// and one still loading is never photographed half-drawn however slow the machine is. The wait it
/// replaces is `docs/research/faster-automated-checks.md`, P1.
const SETTLE_SECONDS: f32 = 10.0;
/// Seconds between the four views of a place's survey: long enough for the frame the previous view
/// asked for to be drawn and photographed, short enough not to be waited out (`P2`).
const SURVEY_SECONDS: f32 = 0.4;
/// Seconds between the views of the route-end look-around, which is a look at where the run ended
/// rather than a survey of it (`P2`).
const LOOK_AROUND_SECONDS: f32 = 1.0;
/// Seconds to let the last screenshot reach the disk before the run exits (`P2`).
const DONE_SECONDS: f32 = 1.0;
/// Seconds to wait in front of a door so its destination pre-streams.
///
/// The floor. The wait is also adaptive (impl-286): it is held past this while the destination
/// pre-warm ([`crate::portal::PortalPrewarm`]) still has work - the destination still streaming
/// in, or its warm view still specializing the doorway's pipelines - so the first-visit burst
/// lands behind the wait instead of on the crossing frame.
const PRESTREAM_SECONDS: f32 = 6.0;
/// The longest [`Phase::Prestream`] holds past [`PRESTREAM_SECONDS`] for the pre-warm: a
/// destination whose loads never land is crossed anyway, and the log says the wait was capped -
/// the same reading [`SETTLE_SECONDS`] takes in `Phase::Settle`.
const PRESTREAM_WAIT_MAX_SECONDS: f32 = 20.0;
/// Seconds to look for a door that has not spawned yet before giving up.
const DOOR_SEARCH_SECONDS: f32 = 40.0;
/// Where the camera stands in front of a door: distance and eye height.
const DOOR_STANDOFF: f32 = 320.0;
const EYE_HEIGHT: f32 = 120.0;
/// The standoffs the walk-through tries, in front of the door, until the player is standing on
/// floor. Where that is depends on the door: the route's last door opens onto the Blackreach shaft,
/// and 320 units in front of it - a good look at the door for the photograph - is mid-air, while
/// the Alftand entrance's marker is set in an open passage where 60 units is inside the rock.
///
/// Sixty units first is about where the game puts a player who walks back out of an interior door
/// (the route's return links arrive 62-71 units out), which is floor by construction, and it puts
/// the doorway filling most of the view for the frames around the crossing.
///
/// Riverwood added the third. Its houses are single rooms 700-1500 units across, and inside
/// Alvor's the far wall is closer than 320 units: the tour failed at door 000133FB with neither
/// standoff walkable (`local/demo/riverwood/tour.txt`, first run). 160 is tried last so that the
/// Alftand route, which passes on 320, walks in from exactly where it always did.
const WALK_STANDOFFS: [f32; 3] = [60.0, 320.0, 160.0];
/// How long the player may be off the ground before the next standoff is tried.
const WALK_FALL_SECONDS: f32 = 0.7;
/// How much closer to the doorway a walk has to get, within [`WALK_STUCK_SECONDS`], for the
/// standoff it started from to count as one the player can walk in from.
const WALK_PROGRESS: f32 = 5.0;
/// How long the walk may make no progress toward the door - held keys, no closing of the distance -
/// before the next standoff is tried. A standoff inside rock leaves the player standing on
/// something and sliding along a wall, which walking cannot fix.
const WALK_STUCK_SECONDS: f32 = 2.5;
/// Seconds the walk-through may take to cross a door before it counts as a failure.
const WALK_THROUGH_SECONDS: f32 = 25.0;
/// How long the walk-through stands in front of the door it pressed `E` at before the stage fails
/// saying the door never opened.
///
/// A door opens in about a second: its `Open` clip is a few frames of swing, and a door with no
/// clip opens in one frame. The wait is generous because the press is not the only thing that has to
/// happen before the doorway is walkable: a door whose destination is still streaming in holds the
/// opening until it is there (`crate::transition::destination_is_loaded`), and the tour's own
/// pre-stream wait before the walk is what normally has it resident. Ten seconds is far longer than
/// any door needs, and it fits inside [`WALK_THROUGH_SECONDS`], which the wait runs inside, so a
/// door that never opens is named rather than walked at.
const WALK_OPEN_SECONDS: f32 = 10.0;

/// How long a door stands fully open before the tour photographs the portal's target: a few
/// frames for the portal camera to be picked, activated and to draw the destination.
const PORTAL_SHOT_SECONDS: f32 = 0.5;
// The wait is the failure a door that never opens is named by, so it has to come before the
// walk-through's own ceiling; a wait longer than that would never be reached.
const _: () = assert!(WALK_OPEN_SECONDS < WALK_THROUGH_SECONDS);
/// How close to the doorway the walk-through starts photographing: close enough that the doorway
/// fills the view, and far enough out that walking to the plane takes longer than [`WALK_WINDOW`]
/// frames of capture even at the frame rate writing the sheet holds the run to.
///
/// The tour does not make the player run (`crate::player::WALK_SPEED` is 150 units per second, and
/// the frames come about four units apart at the rate this was run at), so 200 units of approach is
/// about twenty frames of capture before the swap.
const CAPTURE_DISTANCE: f32 = 200.0;
/// The frames kept on either side of the swap: ten before, ten after.
const WALK_WINDOW: u32 = 10;
/// The walk-through frames held while walking in, before the swap.
///
/// The window's ten frames before the swap are not known until the swap happens, so the approach
/// keeps the last `WALK_RING` of its captures and the file of every older one is deleted as it
/// drops out of the ring; the frames left on the disk afterwards are the window, and the frames in
/// it are the ones the run is judged on. Thirty is the ten the window needs plus a wide margin
/// (`docs/research/faster-automated-checks.md`, P4).
const WALK_RING: usize = 30;
/// The frames a `--tour-clip` run keeps on either side of the swap: a second and a half at 60 fps,
/// enough to cut a video of the crossing from.
const CLIP_WINDOW: u32 = 90;
/// How far out a `--tour-clip` run starts photographing: past the farthest standoff, so the clip
/// holds the whole walk in.
const CLIP_DISTANCE: f32 = 1000.0;

/// The frames kept on either side of the swap, the frames the approach holds while walking in,
/// and how far out the approach starts photographing, for this run (`--tour-clip`).
fn walk_capture(config: &EngineConfig) -> (u32, usize, f32) {
    if config.portal.tour_clip {
        (CLIP_WINDOW, CLIP_WINDOW as usize + 20, CLIP_DISTANCE)
    } else {
        (WALK_WINDOW, WALK_RING, CAPTURE_DISTANCE)
    }
}
/// Where the walk-through's frames go, under the tour's output directory.
const WALK_DIRECTORY: &str = "walk-through";
/// How many frames after the swap the walk-through looks at the other end of the crossing: the far
/// door of the link, which the player arrived in front of and which has to be open by then.
///
/// The arrival open runs in the swap frame itself (`crate::door_animation`'s `open_arrival_doors`,
/// after the crossing and before the portal draws), so ten frames is generous: long enough for a
/// door that streams in a frame or two late, and short enough that the player is still standing in
/// its doorway rather than across the room.
const FAR_DOOR_FRAMES: u32 = 10;
// The far-door check is due inside the frames the walk-through keeps watching for: one due after
// the window has closed would never run, and a check that never runs passes every tour.
const _: () = assert!(FAR_DOOR_FRAMES <= WALK_WINDOW);
/// The doorway bench: how long each state is held before its frames are timed - long enough for
/// the door's screenshot, a swing's last frames and the first frames of a turned view to be out of
/// the numbers - and how long its frames are then timed for.
const BENCH_SETTLE_SECONDS: f32 = 1.0;
const BENCH_SECONDS: f32 = 3.0;
/// The first line of the bench's CSV.
const BENCH_CSV_HEADER: &str = "door,state,frames,mean_ms,p50_ms,p95_ms,p99_ms";

/// The demo's scripted tour.
///
/// Added by [`portal::PortalPlugin`](crate::portal::PortalPlugin) for the runs that are looked at
/// rather than measured, when `--demo-tour` named an output folder; a run that only walks a demo,
/// with no script driving it, adds nothing.
pub struct DemoTourPlugin {
    /// Where the tour writes its log and its screenshots; `None` for a run that only walks a demo.
    pub output_dir: Option<PathBuf>,
}

impl Plugin for DemoTourPlugin {
    fn build(&self, app: &mut App) {
        if let Some(output_dir) = self.output_dir.clone() {
            app.insert_resource(DemoTour::new(output_dir)).add_systems(
                Update,
                // Ahead of the player's own systems: the walk-through presses their keys, and a
                // press has to be in the frame the controller reads it.
                run_demo_tour.before(PlayerInput),
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// Wait for the current place to stream, then photograph it.
    Settle,
    /// Turn on the spot and photograph each place four ways, for visual audits.
    Survey(u8),
    /// `--tour-dwell <seconds>`: stand in the place for that long, then move on, instead of
    /// [`Phase::Survey`]'s four photographs. Reproduces a quick round trip through a door and
    /// back, which the survey's own pause (plus the settle before it) does not.
    Dwell,
    /// Find the next route door and stand in front of it.
    FindDoor,
    /// Wait in front of the door so its destination pre-streams, then photograph the door.
    Prestream,
    /// `--tour-bench`: hold the view still in front of an outside door and time the frames in one
    /// of the bench's states ([`BenchState`]).
    Bench(BenchState),
    /// `--tour-bench`: the closed state is timed and the door has been asked to open; wait for it
    /// to be fully open before timing it open.
    BenchOpen,
    /// Scripted crossing with a portal: the door has been asked to open so the portal camera
    /// renders through it; wait until it is fully open.
    PortalOpen,
    /// The door is open: after [`PORTAL_SHOT_SECONDS`], photograph the portal's target and the
    /// open doorway, then cross.
    PortalShot,
    /// Door photographed; activate it once the screenshot has been taken.
    Activate,
    /// Door activated; waiting for `DoorCrossed`.
    Crossing,
    /// With a player: open the door with `E` and walk through it, photographing every frame.
    WalkThrough,
    /// Walked through; keep photographing until ten frames past the swap, then move on.
    WalkThroughAfter {
        swap: u32,
    },
    /// `--tour-doors` with `--tour-dwell` on the Riverwood route: the arrival has been photographed
    /// on the frame before; move to the user's reported pose, photograph it, and end the short tour.
    /// A frame of its own, because a window takes one screenshot a frame
    /// ([`DemoTour::claim_shot`]).
    UserPose,
    /// Extra views after the last crossing.
    LookAround(u8),
    /// With the player controller active: hold W and check the player walks on the ground.
    Walk {
        start: Vec3,
    },
    Done,
}

#[derive(Resource)]
pub struct DemoTour {
    output_dir: PathBuf,
    stage: usize,
    phase: Phase,
    timer: f32,
    /// Frames since the tour started: what a walk-through capture is named after, so the window
    /// around the swap can be picked out of it.
    frame: u32,
    door: Option<Entity>,
    log: String,
    failed: bool,
    walked: bool,
    /// Frames the current [`Phase::Settle`] has seen streaming quiet for, in a row: the settle
    /// ends once there have been [`SETTLE_QUIET_FRAMES`](crate::shots::SETTLE_QUIET_FRAMES) of them,
    /// and its log line reports how long that took.
    settle_quiet: u32,
    /// DEBUG transition comparison excludes only quiet_frames; growing a quiet
    /// streak must not emit a line every frame.
    settle_debug_counts: Option<SettleCounts>,
    /// Suppresses repeated startup certification wait notes.
    package_wait_noted: bool,
    /// Initial whole-scene upload is distinct from certification and later view settles.
    initial_load_ready: bool,
    initial_upload_wait_noted: bool,
    /// The walk-through's captures this stage, oldest first: the frames whose files are on the
    /// disk. While the walk is still walking in it is a ring of the last [`WALK_RING`] frames; once
    /// the window around the swap has been written it is the window itself.
    walk_frames: Vec<WalkFrame>,
    /// Diagnostic approach number; retries retain their bounded ring separately.
    walk_attempt: usize,
    /// How many frames the walk-through photographed this stage, kept or not: the number the log
    /// line reports, so the ring's pruning does not hide how long the approach was.
    walk_captured: u32,
    /// Which of [`WALK_STANDOFFS`] the walk-through is standing off by, how long the player has
    /// been off the ground there, and how long they have gone without getting closer to the
    /// doorway: a standoff the walk gets nowhere from is left for the next one.
    walk_standoff: usize,
    walk_fell: f32,
    walk_stuck: f32,
    walk_furthest: f32,
    /// Whether the walk-through has pressed `E` at this stage's door. The press is made once, the
    /// way a player makes it: `E` on an open door closes it, and a second press cancels an opening
    /// whose destination is still streaming in (`crate::door_animation`), so a door that has been
    /// asked to open is waited on and never pressed at again.
    walk_pressed: bool,
    /// Whether the log's one line for the door the walk is at has been written this stage: the
    /// frame the door is seen open and the walk starts ([`WalkAction::Walk`]) - which is also the
    /// frame the portal's view of the destination is photographed at, whether or not the walk
    /// pressed `E` to get the door open.
    walk_open_noted: bool,
    /// The last observed walk action/state; diagnostics emit only transitions.
    walk_observed: Option<(WalkAction, Option<DoorState>)>,
    /// The swing check of the door the walk-through is opening ([`SwingWatch`]): what it has seen
    /// of the door's state, and the line it has yet to write.
    swing: SwingWatch,
    /// The crossing the walk-through is making, for the far-door check: the door the player walks
    /// through and the reference its link names as the destination - the door the player arrives in
    /// front of. `None` outside a walk-through.
    crossing: Option<Crossing>,
    /// Whether the far door of that crossing has been looked at since the swap, so the check
    /// writes its one line and not one a frame.
    far_door_checked: bool,
    /// `--tour-bench`'s CSV, copied from the configuration on the first frame so that
    /// [`DemoTour::finish`] can write it.
    bench_path: Option<PathBuf>,
    /// The frame times, in milliseconds, of the bench state being timed.
    bench_samples: Vec<f64>,
    /// The bench's rows so far, one per door and state.
    bench_rows: Vec<BenchRow>,
    /// The tour frame the last window screenshot was asked for in ([`DemoTour::claim_shot`]).
    shot_frame: Option<u32>,
}

/// One photographed frame of a walk-through: the tour frame it was asked for in, the file it was
/// written to, and the pose it shows, which goes into the `frames.txt` written beside them.
struct WalkFrame {
    frame: u32,
    path: PathBuf,
    pose: String,
}

impl DemoTour {
    fn new(output_dir: PathBuf) -> Self {
        Self {
            output_dir,
            stage: 0,
            phase: Phase::Settle,
            timer: 0.0,
            frame: 0,
            door: None,
            log: String::new(),
            failed: false,
            walked: false,
            settle_quiet: 0,
            settle_debug_counts: None,
            package_wait_noted: false,
            initial_load_ready: false,
            initial_upload_wait_noted: false,
            walk_frames: Vec::new(),
            walk_attempt: 0,
            walk_captured: 0,
            walk_standoff: 0,
            walk_fell: 0.0,
            walk_stuck: 0.0,
            walk_furthest: f32::INFINITY,
            walk_pressed: false,
            walk_open_noted: false,
            walk_observed: None,
            swing: SwingWatch::default(),
            crossing: None,
            far_door_checked: false,
            bench_path: None,
            bench_samples: Vec::new(),
            bench_rows: Vec::new(),
            shot_frame: None,
        }
    }

    /// Advances only automation; certification does not pause the world or player.
    fn advance_automation(
        &mut self,
        delta: f32,
        validation: Option<&PackageValidationStatus>,
        scene: Option<&crate::cluster::render::ClusterScene>,
        metrics: Option<&StreamingMetrics>,
    ) -> Result<bool, String> {
        if self.stage == 0 && self.phase == Phase::Settle && !self.initial_load_ready {
            match validation {
                Some(PackageValidationStatus::Pending) => {
                    if !self.package_wait_noted {
                        self.note("stage 0: waiting for requested cluster package certification");
                        self.package_wait_noted = true;
                    }
                    self.timer = 0.0;
                    self.settle_quiet = 0;
                    return Ok(false);
                }
                Some(PackageValidationStatus::Failed(reason)) => {
                    return Err(format!(
                        "requested cluster package certification failed: {reason}"
                    ));
                }
                Some(PackageValidationStatus::Ready) | None => {
                    if validation.is_some() && !initial_cluster_load_ready(scene, metrics) {
                        if !self.initial_upload_wait_noted {
                            let total = scene.map_or(0, |scene| scene.blocks.len());
                            let resident = scene.map_or(0, |scene| {
                                scene
                                    .blocks
                                    .iter()
                                    .filter(|block| {
                                        block.resident.load(std::sync::atomic::Ordering::Acquire)
                                    })
                                    .count()
                            });
                            self.note(format!(
                                "stage 0: certified; waiting for initial cluster load (resident_blocks={resident}/{total} pending_cluster_batches={:?})",
                                metrics.map(|metrics| metrics.pending_cluster_batches)
                            ));
                            self.initial_upload_wait_noted = true;
                        }
                        self.timer = 0.0;
                        self.settle_quiet = 0;
                        return Ok(false);
                    }
                    self.initial_load_ready = true;
                    if self.package_wait_noted || self.initial_upload_wait_noted {
                        self.note("stage 0: initial cluster load ready; starting view settle");
                        self.timer = 0.0;
                        self.settle_quiet = 0;
                        self.package_wait_noted = false;
                    }
                }
            }
        }
        self.frame += 1;
        self.timer += delta;
        Ok(true)
    }

    /// Claims this frame's one window screenshot: true the first time in a tour frame, false after.
    ///
    /// Bevy captures one screenshot per render target per frame and despawns any other asked for
    /// the same target in that frame ("Duplicate render target for screenshot, skipping",
    /// `bevy_render-0.19.0/src/view/window/screenshot.rs#L249`), so a second request would be
    /// dropped - and the one kept would show whatever the tour did to the view after the first.
    fn claim_shot(&mut self) -> bool {
        if self.shot_frame == Some(self.frame) {
            return false;
        }
        self.shot_frame = Some(self.frame);
        true
    }

    fn note(&mut self, line: impl AsRef<str>) {
        info!(target: "demo_tour", "{}", line.as_ref());
        let _ = writeln!(self.log, "{}", line.as_ref());
    }

    fn enter(&mut self, phase: Phase) {
        self.phase = phase;
        self.timer = 0.0;
        if phase == Phase::Settle {
            // Every settle reads the streaming state from the start; a count carried over from the
            // last one would let a place settle before this one had a single quiet frame.
            self.settle_quiet = 0;
            self.settle_debug_counts = None;
        }
    }

    /// Writes the log where the run started, and ends the run with `verdict` as its last line:
    /// `PASSED` or `FAILED`, with ` (short tour)` for a `--tour-doors` run.
    fn finish(&mut self, verdict: &str) {
        self.write_bench();
        let line = match verdict.split_once(' ') {
            Some((word, rest)) => format!("tour {word} after {} crossings {rest}", self.stage),
            None => format!("tour {verdict} after {} crossings", self.stage),
        };
        self.note(line);
        let log_path = self.output_dir.join("tour.txt");
        if let Err(error) = std::fs::write(&log_path, &self.log) {
            error!("could not write {}: {error}", log_path.display());
        }
        self.enter(Phase::Done);
    }

    /// `--tour-bench`: writes the CSV and puts one summary line per state in the log. Called by
    /// [`DemoTour::finish`] before the verdict line, which stays the log's last.
    fn write_bench(&mut self) {
        let Some(path) = self.bench_path.clone() else {
            return;
        };
        if self.bench_rows.is_empty() {
            self.note("bench: no outside door was benched");
        }
        for line in bench_summary(&self.bench_rows) {
            self.note(line);
        }
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, bench_csv(&self.bench_rows)) {
            Ok(()) => self.note(format!("bench: wrote {}", path.display())),
            Err(error) => {
                let line = format!("bench: could not write {}: {error}", path.display());
                error!("{line}");
                self.note(line);
            }
        }
    }

    /// Writes the swing check's one line for the walk-through's door, as soon as the check can
    /// write it: the door open with its clip, or opened with its whole model hidden - which fails
    /// the tour - while the walk is still going, and whatever the swing got to once `ended`.
    ///
    /// Called once a frame while the walk is going and wherever it ends, however it ends: the check
    /// writes one line per door and keeps it, so the next call writes nothing.
    fn settle_swing(&mut self, ended: bool) {
        let Some((verdict, line)) = self.swing.line(self.stage, ended) else {
            return;
        };
        self.note(line);
        if verdict == Swing::WithoutSwinging {
            self.failed = true;
        }
        self.swing.judged = true;
    }

    /// The frames kept out of the current walk-through: the window around the swap, and nothing
    /// else. The files of the frames outside it are deleted, and so is any frame file in the
    /// current attempt directory this walk does not name. Other approach attempts
    /// have separate directories and are never pruned by this window.
    fn keep_walk_window(&mut self, first: u32, last: u32) {
        let mut kept = Vec::new();
        for captured in self.walk_frames.drain(..) {
            if (first..=last).contains(&captured.frame) {
                kept.push(captured);
            } else {
                remove_walk_file(&captured.path);
            }
        }
        let directory = self.walk_directory();
        for path in stray_walk_files(&directory, &kept) {
            remove_walk_file(&path);
        }
        self.walk_frames = kept;
    }

    /// Both successful and failed walks persist the exact retained frame/pose list.
    /// This never takes a screenshot, prunes evidence, or authorizes a crossing.
    fn write_walk_metadata(&self, header: &str) -> std::io::Result<()> {
        let directory = self.walk_directory();
        std::fs::create_dir_all(&directory)?;
        let poses = self
            .walk_frames
            .iter()
            .map(|captured| captured.pose.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(
            directory.join("frames.txt"),
            format!("{header}\n\n{poses}\n"),
        )
    }

    fn save_failed_walk(&mut self, reason: &str, snapshot: &str) {
        let header = format!(
            "failed stage {} at frame {}\nreason: {reason}\n{} frames photographed; {} retained\n{snapshot}",
            self.stage,
            self.frame,
            self.walk_captured,
            self.walk_frames.len()
        );
        if let Err(error) = self.write_walk_metadata(&header) {
            self.note(format!(
                "failed walk metadata could not be written: {error}"
            ));
        }
    }

    /// Archive an unsuccessful approach without deleting files or moving pending
    /// screenshot writes. This is diagnostic evidence, never accepted continuity.
    fn retain_walk_attempt(&mut self, reason: &str, snapshot: &str) {
        let header = format!(
            "abandoned approach stage {} attempt {} at frame {}\nreason: {reason}\n{} frames photographed; {} retained\n{snapshot}",
            self.stage,
            self.walk_attempt,
            self.frame,
            self.walk_captured,
            self.walk_frames.len()
        );
        if let Err(error) = self.write_walk_metadata(&header) {
            self.note(format!("approach metadata could not be written: {error}"));
        }
        self.clear_walk_frames();
        self.walk_attempt += 1;
    }

    /// A genuinely new stage starts at its original directory, independently of
    /// the prior stage's retry count. Existing filed evidence remains untouched.
    fn begin_walk_frames(&mut self) {
        self.clear_walk_frames();
        self.walk_attempt = 0;
    }

    /// Clears the record of a walk-through whose frames are filed - the window written, or the walk
    /// not started yet - leaving the files on the disk as the ones to keep.
    fn clear_walk_frames(&mut self) {
        self.walk_frames.clear();
        self.walk_captured = 0;
    }

    /// Drops the frames older than the ring, deleting their files: a walk still walking in holds
    /// the last [`WALK_RING`] captures and nothing older. The file goes a ring's worth of frames
    /// after it was asked for - some frames after it was written - so nothing is deleted before it
    /// reaches the disk.
    fn ring_walk_frames(&mut self, ring: usize) {
        while self.walk_frames.len() > ring {
            let dropped = self.walk_frames.remove(0);
            remove_walk_file(&dropped.path);
        }
    }

    /// The first approach keeps the historical path; retries have isolated children.
    fn walk_directory(&self) -> PathBuf {
        let stage = self
            .output_dir
            .join(WALK_DIRECTORY)
            .join(format!("{:02}", self.stage));
        if self.walk_attempt == 0 {
            stage
        } else {
            stage.join(format!("attempt-{:02}", self.walk_attempt))
        }
    }
}

/// Deletes a walk-through frame, without complaining about one that is not there.
fn remove_walk_file(path: &std::path::Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        warn!("could not remove {}: {error}", path.display());
    }
}

/// The walk-through frame files of `directory` that `kept` does not name, so they can be deleted.
fn stray_walk_files(directory: &std::path::Path, kept: &[WalkFrame]) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_walk_frame_name(path) && !kept.iter().any(|kept| kept.path == *path))
        .collect()
}

/// Whether a path is one of the walk-through's own frames: `f` and five digits, `.png`.
fn is_walk_frame_name(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix('f'))
        .and_then(|rest| rest.strip_suffix(".png"))
        .is_some_and(|digits| digits.len() == 5 && digits.bytes().all(|byte| byte.is_ascii_digit()))
}

fn shoot_path(commands: &mut Commands, tour: &mut DemoTour, path: PathBuf) {
    if !tour.claim_shot() {
        // Every phase asks for at most one window screenshot a frame; this is a bug in the tour,
        // said in the log rather than left for Bevy to drop silently.
        let line = format!(
            "screenshot {} not taken: another was asked for on frame {}",
            path.display(),
            tour.frame
        );
        warn!("{line}");
        tour.note(line);
        return;
    }
    tour.note(format!("screenshot {}", path.display()));
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

/// A 0..1 display value for one linear HDR channel: clamped, so the portal target's colours below
/// white - everything the main tonemapper would also leave nearly as it is - are kept, and a
/// highlight past white saturates rather than wrapping.
fn hdr_channel_to_display(linear: f32) -> f32 {
    if linear.is_finite() {
        linear.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// An 8-bit sRGB copy of a captured image in any format `Image::get_color_at` reads - the portal
/// target's `Rgba16Float` included, which Bevy's own `save_to_disk` refuses ("Cannot save
/// screenshot", its `try_into_dynamic` has no half-float case). Each channel is clamped
/// ([`hdr_channel_to_display`]) and encoded as sRGB, alpha opaque.
fn hdr_to_srgb8(image: &Image) -> Image {
    let size = image.size();
    let mut data = Vec::with_capacity((size.x * size.y * 4) as usize);
    for y in 0..size.y {
        for x in 0..size.x {
            let linear = image
                .get_color_at(x, y)
                .map(|color| color.to_linear())
                .unwrap_or(LinearRgba::BLACK);
            let display = LinearRgba::rgb(
                hdr_channel_to_display(linear.red),
                hdr_channel_to_display(linear.green),
                hdr_channel_to_display(linear.blue),
            );
            let srgb = Srgba::from(display).to_u8_array();
            data.extend_from_slice(&[srgb[0], srgb[1], srgb[2], 255]);
        }
    }
    Image::new(
        Extent3d {
            width: size.x,
            height: size.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        default(),
    )
}

/// Bevy's `save_to_disk` for an HDR render target: the capture is converted to 8-bit sRGB first
/// ([`hdr_to_srgb8`]) and then written as the path's format.
fn save_hdr_to_disk(path: PathBuf) -> impl FnMut(On<ScreenshotCaptured>) {
    move |captured| match hdr_to_srgb8(&captured.image).try_into_dynamic() {
        Ok(image) => match image.to_rgb8().save(&path) {
            Ok(()) => info!("Screenshot saved to {}", path.display()),
            Err(error) => error!("Cannot save screenshot {}: {error}", path.display()),
        },
        Err(error) => error!("Cannot save screenshot {}: {error}", path.display()),
    }
}

/// Photographs what the portal camera itself renders - the room beyond the open door - into
/// `NN-portal-view.png`, from its HDR target ([`save_hdr_to_disk`]).
fn shoot_portal_view(
    commands: &mut Commands,
    tour: &mut DemoTour,
    portal: &crate::portal::PortalTexture,
) {
    let path = tour
        .output_dir
        .join(format!("{:02}-portal-view.png", tour.stage));
    tour.note(format!("portal render target {}", path.display()));
    commands
        .spawn(Screenshot::image(portal.0.clone()))
        .observe(save_hdr_to_disk(path));
}

fn shoot(commands: &mut Commands, tour: &mut DemoTour, name: &str) {
    let path = tour.output_dir.join(format!("{name}.png"));
    shoot_path(commands, tour, path);
}

/// Photographs the frame being drawn now and records the frame it belongs to.
///
/// The image is written a frame or two later, so the file is named after the frame it was asked
/// for: that is what puts it on the right side of the swap.
fn capture_walk_frame(commands: &mut Commands, tour: &mut DemoTour, camera: &Transform) {
    let directory = tour.walk_directory();
    if let Err(error) = std::fs::create_dir_all(&directory) {
        warn!("could not create {}: {error}", directory.display());
        return;
    }
    let path = directory.join(format!("f{:05}.png", tour.frame));
    if !tour.claim_shot() {
        warn!(
            "walk-through frame {} not taken: another screenshot was asked for on this frame",
            path.display()
        );
        return;
    }
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path.clone()));
    let forward = camera.rotation * Vec3::NEG_Z;
    tour.walk_frames.push(WalkFrame {
        frame: tour.frame,
        path,
        pose: format!(
            "frame {} eye {:.1} {:.1} {:.1} forward {:.4} {:.4} {:.4}",
            tour.frame,
            camera.translation.x,
            camera.translation.y,
            camera.translation.z,
            forward.x,
            forward.y,
            forward.z
        ),
    });
    tour.walk_captured += 1;
}

/// Snapshot only facts already owned by this system. Ground-hit identity and the
/// actual crossing trigger are deliberately not inferred here.
fn walk_pose_snapshot(eye: Vec3, player: Option<&Player>, door: Option<(Vec3, Quat)>) -> String {
    let feet = crate::player::feet_from_eye(eye);
    let relative = door.map(|(position, frame)| {
        let local = frame.inverse() * (feet - position);
        (
            eye.distance(position),
            distance_in_front_of_door(position, frame, eye),
            local,
        )
    });
    format!(
        "eye={eye:?} feet={feet:?} grounded={:?} velocity={:?} door_relative(distance,in_front,feet_local)={relative:?}",
        player.map(|player| player.grounded),
        player.map(|player| player.velocity),
    )
}

/// Points the tour's view at `position` with `rotation`: the camera always, and the player too when
/// one drives it.
///
/// `crate::player::player_look` writes the camera's rotation from the player's own yaw and pitch
/// every frame, so an aim written only to the camera is undone before the next frame is drawn. The
/// pitch is clamped exactly as the player's own look clamps it, and the player is dropped from the
/// air, so the pose the tour asks for is the pose the next frame renders.
fn point_the_view(
    camera: &mut Transform,
    player: Option<&mut Player>,
    position: Vec3,
    rotation: Quat,
) {
    camera.translation = position;
    camera.rotation = rotation;
    if let Some(player) = player {
        let (yaw, pitch, _) = rotation.to_euler(EulerRot::YXZ);
        player.yaw = yaw;
        player.pitch = crate::player::clamp_pitch(pitch);
        player.velocity = Vec3::ZERO;
        player.grounded = false;
        camera.rotation = player.look_rotation();
    }
}

/// Points the view at a door from `standoff` units in front of it, on the side the door faces.
///
/// The side comes from the door's own link data ([`LoadDoor::outward`]) rather than from the model,
/// for the reason [`crate::transition::door_frame`] gives: door models disagree about which of
/// their own axes is their front.
fn stand_in_front_of_door(
    camera: &mut Transform,
    player: Option<&mut Player>,
    geometry: DoorGeometry<'_>,
    door: &LoadDoor,
    standoff: f32,
) {
    let transform = geometry.global;
    let position = transform.translation();
    let mut away = match door.outward {
        Some(outward) => creation_to_bevy(Vec3::from_array(outward)),
        None => *transform.forward(),
    };
    away.y = 0.0;
    let away = away.try_normalize().unwrap_or_else(|| {
        let mut fallback = camera.translation - position;
        fallback.y = 0.0;
        fallback.try_normalize().unwrap_or(Vec3::Z)
    });
    let eye = position + away * standoff + Vec3::Y * EYE_HEIGHT;
    let looking = Transform::from_translation(eye).looking_at(geometry.aim(), Vec3::Y);
    point_the_view(camera, player, looking.translation, looking.rotation);
}

/// The doors a run walks: the whole route, or - for a smoke tour, `--tour-doors N` - the first `N`
/// of them. A smoke run walks at least one door, and never more than the route has.
fn walked_doors(route: &'static DemoRoute, smoke: Option<usize>) -> &'static [u32] {
    match smoke {
        Some(limit) => &route.doors[..route.doors.len().min(limit.max(1))],
        None => route.doors,
    }
}

/// How many crossings a run makes: the doors it walks, once each, or - for `--tour-repeat N` -
/// `N` times over. A repeat of zero, or of a run with no repeat flag at all, is one pass, so a
/// tour never walks nothing.
fn tour_crossings(plan: &[u32], repeat: Option<usize>) -> usize {
    plan.len() * repeat.unwrap_or(1).max(1)
}

/// The door of the run's `stage`-th crossing: the doors it walks in order, then the same again for
/// each repeat. `plan` is never empty ([`walked_doors`] keeps at least one door), so a stage inside
/// [`tour_crossings`] always names a door.
fn door_at_stage(plan: &[u32], stage: usize) -> u32 {
    plan[stage % plan.len()]
}

/// The cell the tour's camera is standing in, for the settle's residency test: the same cell
/// [`crate::streaming::plan_cells`] streams from, computed the same way - the active interior, or
/// the exterior grid the camera's Creation position falls in.
///
/// `None` when the run has no active cell yet, which the residency test reads as "not resident",
/// exactly as it reads a cell still loading.
fn camera_space_key(
    active: &ActiveCell,
    origin: Option<&RenderOrigin>,
    position: Vec3,
) -> Option<CellKey> {
    if let Some(cell_id) = active.interior {
        return Some(CellKey::Interior(cell_id));
    }
    // A rebase moves the camera in render space, so the origin goes back on before the grid is
    // taken, which is what `camera_grid` does for the plan too.
    let grid = camera_grid(position, origin?.0);
    Some(CellKey::Exterior {
        worldspace_id: active.worldspace_id,
        grid_x: grid.x,
        grid_y: grid.y,
    })
}

/// Turns the view on the spot, the tour's four-way survey.
fn turn_the_view(camera: &mut Transform, player: Option<&mut Player>, radians: f32) {
    match player {
        Some(player) => {
            player.yaw += radians;
            camera.rotation = player.look_rotation();
        }
        None => camera.rotate_y(radians),
    }
}

/// A load door as the tour reads it: its placement, its link, the state its animation owns, the
/// animation itself ([`SwingWatch`]) and the doorway anchor the crossing is mapped by
/// ([`Crossing`]).
type DoorRow = (
    Entity,
    &'static GlobalTransform,
    &'static LoadDoor,
    Option<&'static DoorState>,
    Option<&'static DoorAnimation>,
    Option<&'static DoorAnchor>,
    &'static Transform,
    Option<&'static InstanceBounds>,
    Option<&'static ExpectedModelBounds>,
);

type DoorQuery<'w, 's> = Query<'w, 's, DoorRow, Without<StreamingCamera>>;

#[derive(bevy::ecs::system::SystemParam)]
struct TourDoors<'w, 's> {
    doors: DoorQuery<'w, 's>,
    portal: Option<Res<'w, crate::portal::PortalState>>,
    parents: Query<'w, 's, &'static ChildOf>,
}

impl TourDoors<'_, '_> {
    /// The same complete candidate set and aim/range rule as the player's E targeting.
    fn targeted(&self, eye: Vec3, forward: Vec3) -> Option<Entity> {
        target_door(
            eye,
            forward,
            self.doors
                .iter()
                .filter(|(entity, global, door, ..)| {
                    !door.auto_load
                        && (global.translation() != Vec3::ZERO
                            || global.rotation() != Quat::IDENTITY
                            || global.scale() != Vec3::ONE)
                        && self
                            .portal
                            .as_deref()
                            .is_none_or(|portal| portal.is_in_active_space(*entity, &self.parents))
                })
                .map(
                    |(entity, global, door, _, _, anchor, local, instance, expected)| {
                        let aim = door_aim_point(
                            global.translation(),
                            global.rotation(),
                            local.scale,
                            anchor,
                            instance,
                            expected,
                        );
                        (entity, global.translation(), aim, door)
                    },
                ),
        )
        .map(|(entity, _)| entity)
    }
}

#[derive(Clone, Copy)]
struct DoorGeometry<'a> {
    global: &'a GlobalTransform,
    local: &'a Transform,
    anchor: Option<&'a DoorAnchor>,
    instance: Option<&'a InstanceBounds>,
    expected: Option<&'a ExpectedModelBounds>,
}

impl DoorGeometry<'_> {
    fn aim(&self) -> Vec3 {
        door_aim_point(
            self.global.translation(),
            self.global.rotation(),
            self.local.scale,
            self.anchor,
            self.instance,
            self.expected,
        )
    }
}

/// The crossing a walk-through is making, for the far-door check: the door the player walks
/// through, the door the link names as the destination, and whether the source doorway is anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Crossing {
    /// The door the player walks through, by reference id: the near end of the link.
    source: u32,
    /// The link's `destination_ref_id`: the far door, the one the player arrives in front of.
    destination: u32,
    /// Whether the source door carries a [`DoorAnchor`]. A crossing of an anchored doorway lands
    /// the player in the destination doorway itself, which is why the far door is opened with the
    /// crossing (`crate::transition` writes `OpenDestinationDoor` for exactly that case); every
    /// other crossing lands at the link's `XTEL` arrival point, clear of the far door, and leaves
    /// it closed.
    anchored: bool,
}

/// A state the doorway bench times a door in, in the order it times them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchState {
    /// The door closed, the view facing it: the portal has nothing to draw.
    Closed,
    /// The door fully open, the view facing it with the doorway on screen: the portal's view of
    /// the room beyond is drawn.
    OpenInView,
    /// The door still open, the view turned half a turn so the doorway is off screen.
    OpenBehind,
}

impl BenchState {
    const ALL: [BenchState; 3] = [
        BenchState::Closed,
        BenchState::OpenInView,
        BenchState::OpenBehind,
    ];

    /// The state's name in the CSV and the log.
    fn name(self) -> &'static str {
        match self {
            BenchState::Closed => "closed",
            BenchState::OpenInView => "open-in-view",
            BenchState::OpenBehind => "open-behind",
        }
    }

    /// The state timed after this one, or `None` after the last.
    fn next(self) -> Option<BenchState> {
        match self {
            BenchState::Closed => Some(BenchState::OpenInView),
            BenchState::OpenInView => Some(BenchState::OpenBehind),
            BenchState::OpenBehind => None,
        }
    }
}

/// Where a bench state is, `timer` seconds after it began: settling, timing frames, or done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchStep {
    Settling,
    Timing,
    Done,
}

fn bench_step(timer: f32) -> BenchStep {
    if timer < BENCH_SETTLE_SECONDS {
        BenchStep::Settling
    } else if timer < BENCH_SETTLE_SECONDS + BENCH_SECONDS {
        BenchStep::Timing
    } else {
        BenchStep::Done
    }
}

/// One row of the bench's CSV: a door, a state, and its frame times.
#[derive(Debug, Clone, PartialEq)]
struct BenchRow {
    door: u32,
    state: BenchState,
    frames: usize,
    mean_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
}

impl BenchRow {
    /// The row for `samples`, frame times in milliseconds in the order they were taken. The
    /// percentiles are [`crate::metrics`]'s, the ones a benchmark run reports.
    fn from_samples(door: u32, state: BenchState, samples: &[f64]) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        let mean_ms = if sorted.is_empty() {
            0.0
        } else {
            sorted.iter().sum::<f64>() / sorted.len() as f64
        };
        Self {
            door,
            state,
            frames: sorted.len(),
            mean_ms,
            p50_ms: percentile(&sorted, 0.50),
            p95_ms: percentile(&sorted, 0.95),
            p99_ms: percentile(&sorted, 0.99),
        }
    }

    /// The row as a CSV line, under [`BENCH_CSV_HEADER`].
    fn csv(&self) -> String {
        format!(
            "{:08X},{},{},{:.3},{:.3},{:.3},{:.3}",
            self.door,
            self.state.name(),
            self.frames,
            self.mean_ms,
            self.p50_ms,
            self.p95_ms,
            self.p99_ms
        )
    }
}

/// The bench's whole CSV: the header and one line per row.
fn bench_csv(rows: &[BenchRow]) -> String {
    let mut csv = format!("{BENCH_CSV_HEADER}\n");
    for row in rows {
        csv.push_str(&row.csv());
        csv.push('\n');
    }
    csv
}

/// One line per state that has rows: each column averaged over the doors timed in that state.
fn bench_summary(rows: &[BenchRow]) -> Vec<String> {
    BenchState::ALL
        .iter()
        .filter_map(|state| {
            let rows: Vec<_> = rows.iter().filter(|row| row.state == *state).collect();
            if rows.is_empty() {
                return None;
            }
            let average = |column: fn(&BenchRow) -> f64| {
                rows.iter().map(|row| column(row)).sum::<f64>() / rows.len() as f64
            };
            Some(format!(
                "bench {}: {} door(s), mean {:.2} ms, p50 {:.2} ms, p95 {:.2} ms, p99 {:.2} ms (averaged over the doors)",
                state.name(),
                rows.len(),
                average(|row| row.mean_ms),
                average(|row| row.p50_ms),
                average(|row| row.p95_ms),
                average(|row| row.p99_ms),
            ))
        })
        .collect()
}

/// What the swing check makes of the walk-through's door.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Swing {
    /// The door's model has no `Open` clip of its own, so there is no swing to watch for: an open
    /// door of this kind is a hole by design ([`DoorState::hides_whole_reference`]).
    NoClip,
    /// The door opened with a clip playing, or was still swinging when the player walked through
    /// it. Either way the doorway opened the way the game opens it.
    Swung,
    /// The door's model has an `Open` clip and the door opened with `animated: false` anyway: its
    /// whole model was hidden instead of swinging. This is the defect the check exists for.
    WithoutSwinging,
    /// The walk ended without the door opening at all, so the check saw no swing - and the walk's
    /// own verdict is the one that matters.
    NeverOpened,
}

/// The swing check of the walk-through: what the tour has seen of the door it is opening with `E`.
///
/// A door whose model has an `Open` clip has to play it - the state passes through
/// [`DoorState::Opening`], the leaf swings - and must never take the fallback that opens the door
/// with nothing to animate ([`DoorState::Open`] with `animated: false`): that hides the whole door
/// model ([`DoorState::hides_whole_reference`]) and leaves a hole where the doorway was. Every tour
/// this repo ran passed while every animated door did exactly that (the user, 2026-09-24;
/// `docs/research/checks-for-user-found-defects.md`, defect 1, fixed in `d420bf9`).
///
/// The check watches from the first frame of the walk at the door until the walk ends. The crossing
/// unloads the door with the cell it stood in, so a swing still in flight at the swap is judged
/// from the states the walk did see: reaching `Open { animated: true }` is what the game does, and
/// `Opening` alone is still a leaf swinging rather than a model hidden.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SwingWatch {
    /// The door the walk-through is opening, by reference id: what the check's line names. `None`
    /// while no walk is at a door.
    door: Option<u32>,
    /// Whether the door's model has an `Open` clip of its own. Read from
    /// [`DoorAnimation::swings`], and sticky across the walk: a door whose animation is being
    /// attached again after its scene was instanced a second time
    /// (`crate::door_animation`'s `forget_lost_door_players`) still has the clips it had.
    swings: bool,
    /// Whether the state has been [`DoorState::Opening`]: the swing is playing.
    opening: bool,
    /// Whether the state has been [`DoorState::Open`] with `animated: true`.
    open_animated: bool,
    /// Whether the state has been [`DoorState::Open`] with `animated: false`: the door opened with
    /// nothing to animate.
    open_unanimated: bool,
    /// Whether the check has written its line for this door. It writes one line per door and keeps
    /// it: the walk goes on at the door for as long as it takes to cross it, and the states it
    /// passes through after the line was written are not what the line was about.
    judged: bool,
}

impl SwingWatch {
    /// Watches one frame of the walk-through at the door: `swings` says whether the door's model
    /// has an `Open` clip of its own, and `state` is where the door is now.
    fn observe(&mut self, door: u32, swings: bool, state: Option<DoorState>) {
        if self.judged {
            return;
        }
        self.door = Some(door);
        self.swings |= swings;
        match state {
            Some(DoorState::Opening) => self.opening = true,
            Some(DoorState::Open { animated: true }) => self.open_animated = true,
            Some(DoorState::Open { animated: false }) => self.open_unanimated = true,
            Some(DoorState::Closed | DoorState::Closing) | None => {}
        }
    }

    /// What the check has to say, or `None` while the walk is still going and the door is still
    /// swinging. `ended` is the walk having ended, however it ended: a swing still in flight then is
    /// judged from the states it did reach.
    fn verdict(&self, ended: bool) -> Option<Swing> {
        self.door?;
        if self.open_unanimated && self.swings {
            // The defect: this door had a clip to play and opened with its whole model hidden.
            return Some(Swing::WithoutSwinging);
        }
        if self.open_animated {
            // The door reached the opening its clip plays towards, and it is the clip that opened
            // it: the swing the check is for, decided as soon as the walk sees it.
            return Some(Swing::Swung);
        }
        if !ended {
            // Still swinging, or still closed: nothing the check can say yet.
            return None;
        }
        if self.opening {
            // The player walked through a leaf that was still swinging - the doorway is open from
            // the first frame of a swing - and the crossing unloaded the door before the walk saw
            // it reach `Open`.
            return Some(Swing::Swung);
        }
        Some(if self.open_unanimated {
            // Opened with nothing to animate and no clip to animate it: a static leaf, which was
            // always a hole where its model stood.
            Swing::NoClip
        } else {
            Swing::NeverOpened
        })
    }

    /// The states the door was seen in, in order, for the line: what the check judged.
    fn seen(&self) -> String {
        let mut seen = Vec::new();
        if self.opening {
            seen.push("Opening".to_owned());
        }
        if self.open_animated {
            seen.push("Open { animated: true }".to_owned());
        }
        if self.open_unanimated {
            seen.push("Open { animated: false }".to_owned());
        }
        if seen.is_empty() {
            return "Closed".to_owned();
        }
        seen.join(" -> ")
    }

    /// The check's one line for the door it is watching, or `None` while there is nothing to write:
    /// no door walked yet, a swing still going while the walk goes with it, or a line already
    /// written for this door.
    fn line(&self, stage: usize, ended: bool) -> Option<(Swing, String)> {
        if self.judged {
            return None;
        }
        let door = self.door?;
        let verdict = self.verdict(ended)?;
        let clip = if self.swings {
            "has an Open clip of its own"
        } else {
            "has no Open clip of its own"
        };
        let seen = self.seen();
        let line = match verdict {
            Swing::Swung => {
                format!("stage {stage}: swing ok - door {door:08X} {clip}, and the walk saw {seen}")
            }
            Swing::NoClip => format!(
                "stage {stage}: swing ok - door {door:08X} {clip}, so there is nothing to swing; \
                 the walk saw {seen}"
            ),
            Swing::WithoutSwinging => format!(
                "FAIL stage {stage}: swing - door {door:08X} has an Open clip of its own but opened \
                 with animated: false, hiding its whole model instead of swinging (the walk saw \
                 {seen})"
            ),
            Swing::NeverOpened => format!(
                "stage {stage}: swing ok - door {door:08X} never opened while the walk-through was \
                 at it, so the check saw no swing"
            ),
        };
        Some((verdict, line))
    }
}

/// What the far-door check found when it looked for the other end of the crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FarDoor {
    /// The doorway the player arrived in is open: they landed in a doorway they can walk out of.
    Open,
    /// The door is in the world and not open: the crossing put the player inside a closed leaf,
    /// which is the defect impl-152 fixed.
    Closed,
    /// No load door in the world carries the reference the link names.
    Missing,
}

/// What the tour makes of the far door it looked for, from what the door query answered: the state
/// of the door whose reference the link names, or `None` when no door carries that reference.
fn far_door(found: Option<Option<DoorState>>) -> FarDoor {
    match found {
        Some(state) if door_is_open(state.as_ref()) => FarDoor::Open,
        Some(_) => FarDoor::Closed,
        None => FarDoor::Missing,
    }
}

/// What [`Phase::PortalOpen`] makes of one frame's look at the door it asked to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PortalWait {
    /// Fully open: move on to photograph the portal's target and the doorway
    /// ([`Phase::PortalShot`]).
    Open,
    /// On its way open, or not yet in the world: keep waiting.
    Waiting,
    /// The door is in the world and the wait ran out: it did not open.
    NeverOpened,
    /// The door entity is not in the query at all - its cell has not streamed in, or it was
    /// despawned - and the wait ran out. A different failure from a door that is there and did not
    /// open, and it is named separately: this case used to fall into the "did not open within ten
    /// seconds" arm on the phase's own first frame, when a tenth of a second had passed
    /// (review-258's finding 3).
    Missing,
}

/// What the portal-open wait does this frame, from the door query's answer and how long the phase
/// has waited: `state` is `Option<Option<DoorState>>` because the outer `None` - no door entity to
/// look at - is a different thing from a door that carries no state of its own (`Some(None)`, which
/// counts as closed, as [`door_is_open`] does).
///
/// The door is waited for as long as a door that is opening is ([`WALK_OPEN_SECONDS`]): a door
/// whose cell has not streamed in yet appears a frame or two later, and giving up before then would
/// skip the photograph for no reason.
fn portal_wait(state: Option<Option<DoorState>>, waited: f32) -> PortalWait {
    match state {
        Some(Some(DoorState::Open { .. })) => PortalWait::Open,
        _ if waited < WALK_OPEN_SECONDS => PortalWait::Waiting,
        Some(_) => PortalWait::NeverOpened,
        None => PortalWait::Missing,
    }
}

/// How long a scripted crossing may take before the tour calls it failed. The seamless crossing
/// lands in the frame it is activated; the vanilla load screen fades out, holds black until the
/// landing cell is ready (at most `crate::door_crossing::LANDING_TIMEOUT_SECONDS`) and reports the
/// arrival as the fade-in starts.
fn crossing_timeout(config: &EngineConfig) -> f32 {
    if config.seamless_doors() {
        5.0
    } else {
        crate::door_crossing::FADE_SECONDS + crate::door_crossing::LANDING_TIMEOUT_SECONDS + 5.0
    }
}

/// Preserve every actual settle input except the growing quiet-frame streak.
/// This is diagnostic comparison only and never participates in readiness.
fn settle_debug_snapshot(counts: &SettleCounts) -> SettleCounts {
    SettleCounts {
        quiet_frames: 0,
        ..*counts
    }
}

/// A timeout is a failed check, never permission to capture an unfinished view.
fn tour_settle_ready(counts: &SettleCounts, waited: f32) -> Result<bool, String> {
    if counts.space_failed {
        Err(format!(
            "the cell the tour stands in failed to load ({})",
            counts.describe()
        ))
    } else if counts.failed_cluster_batches != 0 {
        if waited < SETTLE_SECONDS {
            Ok(false)
        } else {
            Err(format!(
                "a demanded cluster source/material failed to load ({})",
                counts.describe()
            ))
        }
    } else if shots_settled(counts) {
        Ok(true)
    } else if waited >= SETTLE_SECONDS {
        Err(format!(
            "not settled after {SETTLE_SECONDS:.0} s ({} quiet_frames={}/{})",
            counts.describe(),
            counts.quiet_frames,
            crate::shots::SETTLE_QUIET_FRAMES
        ))
    } else {
        Ok(false)
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn run_demo_tour(
    mut commands: Commands,
    time: Res<Time>,
    config: Res<EngineConfig>,
    mut tour: ResMut<DemoTour>,
    mut camera: Query<(&mut Transform, Option<&mut Player>), With<StreamingCamera>>,
    targeting: TourDoors,
    mut crossed: MessageReader<DoorCrossed>,
    mut activate: MessageWriter<ActivateDoor>,
    mut exit: MessageWriter<AppExit>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    portal_texture: Option<Res<crate::portal::PortalTexture>>,
    streaming: Option<Res<StreamingWorld>>,
    metrics: Option<Res<StreamingMetrics>>,
    active: Option<Res<ActiveCell>>,
    origin: Option<Res<RenderOrigin>>,
    // Grouped: a system takes at most sixteen parameters.
    (real_time, mut open_door, prewarm, waiting, validation, scene, startup): (
        Res<Time<Real>>,
        MessageWriter<OpenDoor>,
        Option<Res<crate::portal::PortalPrewarm>>,
        WaitingVisibility,
        Option<Res<PackageValidationStatus>>,
        Option<Res<crate::cluster::render::ClusterScene>>,
        Option<Res<crate::pacing::startup::StartupLoad>>,
    ),
) {
    if startup.as_ref().is_some_and(|load| load.cover_pending()) {
        return;
    }
    let doors = &targeting.doors;
    match tour.advance_automation(
        time.delta_secs(),
        validation.as_deref(),
        scene.as_deref(),
        metrics.as_deref(),
    ) {
        Ok(false) => return,
        Ok(true) => {}
        Err(reason) => {
            tour.note(format!("FAIL stage 0: {reason}"));
            tour.failed = true;
            tour.finish("FAILED");
            return;
        }
    }
    // The route belongs to the demo the run started in, and does not change while it runs.
    let route = route_for_run(config.portal.demo.as_deref());
    // `--tour-doors N`: a smoke tour walks the first N doors of that route and then stops. At
    // least one door - a run of no doors would check nothing - and never more than the route has.
    let smoke = config.portal.tour_doors;
    let route_doors = walked_doors(route, smoke);
    // `--tour-repeat N`: the same doors, N times over. The run's last stage is the last crossing of
    // the last repeat, so the short tour stops there and a full tour goes on to its look-around.
    let crossings = tour_crossings(route_doors, config.portal.tour_repeat);
    if tour.frame == 1 {
        tour.bench_path = config.portal.tour_bench.clone();
        let line = format!(
            "tour route: {} ({} doors), demo {:?}",
            route.name,
            route.doors.len(),
            config.portal.demo.as_deref(),
        );
        tour.note(line);
        if smoke.is_some() {
            let line = format!(
                "short tour: the first {} of those doors only, stopping after the crossing; no look-around or walk test",
                route_doors.len()
            );
            tour.note(line);
        }
        if let Some(repeat) = config.portal.tour_repeat {
            let line = format!(
                "repeated tour: those {} doors walked {repeat} times over, {crossings} crossings in all",
                route_doors.len()
            );
            tour.note(line);
        }
    }
    let Ok((mut camera, player)) = camera.single_mut() else {
        return;
    };
    let mut player = player;
    match tour.phase {
        Phase::Settle => {
            // The settle ends when the place has streamed in, not after a flat wait: the counts are
            // the ones a `--shots` pose settles on ([`crate::shots`]). A timeout fails the tour
            // without a screenshot or a crossing attempt.
            let counts = settle_counts(
                active.as_deref().and_then(|active| {
                    camera_space_key(active, origin.as_deref(), camera.translation)
                }),
                tour.settle_quiet,
                pending_drawn(&waiting),
                streaming.as_deref(),
                metrics.as_deref(),
            );
            tour.settle_quiet = if counts.is_quiet() {
                tour.settle_quiet.saturating_add(1)
            } else {
                0
            };
            if bevy::log::tracing::enabled!(target: "demo_tour", bevy::log::Level::DEBUG) {
                let snapshot = settle_debug_snapshot(&counts);
                if tour.settle_debug_counts != Some(snapshot) {
                    debug!(
                        target: "demo_tour",
                        stage = tour.stage,
                        tour_frame = tour.frame,
                        waited_seconds = tour.timer,
                        is_quiet = counts.is_quiet(),
                        quiet_frames_before = counts.quiet_frames,
                        quiet_frames_after = tour.settle_quiet,
                        required_quiet_frames = crate::shots::SETTLE_QUIET_FRAMES,
                        ?counts,
                        "tour settle counts changed"
                    );
                    tour.settle_debug_counts = Some(snapshot);
                }
            }
            let ready = match tour_settle_ready(&counts, tour.timer) {
                Ok(ready) => ready,
                Err(reason) => {
                    let stage = tour.stage;
                    tour.note(format!("FAIL stage {stage}: {reason}"));
                    tour.failed = true;
                    tour.finish("FAILED");
                    return;
                }
            };
            if ready {
                let line = format!(
                    "stage {}: settled after {:.2} s ({} quiet frames)",
                    tour.stage, tour.timer, counts.quiet_frames
                );
                tour.note(line);
                let name = format!("{:02}-arrived", tour.stage);
                shoot(&mut commands, &mut tour, &name);
                if tour.stage >= crossings {
                    if smoke.is_some() {
                        // impl-185: with `--tour-dwell`, also photograph the user's own reported
                        // pose - `local/captures/2026-09-25_02-01-35/shots.json` shot "03",
                        // outside Sven's House on the porch boardwalk with the door still open
                        // behind - so the round trip this option reproduces can be judged
                        // against the capture that reported the defect (shadows on the boardwalk
                        // and path, compared with that capture's "02", shot from nearly the same
                        // spot before the round trip), not only the tour's own arrival pose.
                        // On the next frame: the arrival's screenshot is this frame's one, and
                        // moving the view now would put the user's pose in it.
                        if config.portal.tour_dwell.is_some()
                            && config.portal.demo.as_deref() == Some("riverwood")
                            && origin.is_some()
                        {
                            tour.enter(Phase::UserPose);
                        } else {
                            finish_short_tour(&mut tour);
                        }
                    } else {
                        tour.enter(Phase::LookAround(0));
                    }
                } else if config.portal.tour_dwell.is_some() {
                    tour.enter(Phase::Dwell);
                } else {
                    tour.enter(Phase::Survey(0));
                }
            }
        }
        Phase::Dwell => {
            if tour.timer >= config.portal.tour_dwell.unwrap_or(0.0) {
                tour.enter(Phase::FindDoor);
            }
        }
        Phase::Survey(view) => {
            if tour.timer >= SURVEY_SECONDS {
                if view < 3 {
                    turn_the_view(
                        &mut camera,
                        player.as_deref_mut(),
                        std::f32::consts::FRAC_PI_2,
                    );
                    let name = format!("{:02}-survey-{}", tour.stage, view + 1);
                    shoot(&mut commands, &mut tour, &name);
                    tour.enter(Phase::Survey(view + 1));
                } else {
                    turn_the_view(
                        &mut camera,
                        player.as_deref_mut(),
                        std::f32::consts::FRAC_PI_2,
                    );
                    tour.enter(Phase::FindDoor);
                }
            }
        }
        Phase::FindDoor => {
            // The settle sends the tour on to the look-around once the stage count reaches the
            // route's length - or, on a smoke run, its first `--tour-doors` doors - so every stage
            // that gets here names a door the run walks.
            let wanted = door_at_stage(route_doors, tour.stage);
            if let Some((entity, transform, door, _, _, anchor, local, instance, expected)) =
                doors.iter().find(|(_, _, door, ..)| door.ref_id == wanted)
            {
                let door_position = transform.translation();
                // Stand on the door's front: the side the player walks in from, where the portal
                // shows the room beyond (`stand_in_front_of_door`).
                stand_in_front_of_door(
                    &mut camera,
                    player.as_deref_mut(),
                    DoorGeometry {
                        global: transform,
                        local,
                        anchor,
                        instance,
                        expected,
                    },
                    door,
                    DOOR_STANDOFF,
                );
                let eye = camera.translation;
                tour.door = Some(entity);
                let line = format!(
                    "stage {}: door {wanted:08X} -> \"{}\" found at {door_position:?}, {} (auto_load={}), camera placed at {eye:?}",
                    tour.stage,
                    door.label,
                    if door.auto_load {
                        "auto-load marker"
                    } else {
                        "E-key door"
                    },
                    door.auto_load
                );
                tour.note(line);
                tour.enter(Phase::Prestream);
            } else if tour.timer >= DOOR_SEARCH_SECONDS {
                let line = format!(
                    "FAIL stage {}: door {wanted:08X} never spawned ({} load doors resident: {})",
                    tour.stage,
                    doors.iter().count(),
                    doors
                        .iter()
                        .map(|(_, _, door, ..)| format!("{:08X}", door.ref_id))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                tour.note(line);
                tour.failed = true;
                tour.enter(Phase::LookAround(0));
            }
        }
        Phase::Prestream => {
            // The wait is adaptive (impl-286): the floor above is kept - the pose, the screenshots
            // and the bench all read a settled run - but held past it while the pre-warm still
            // has work, so the destination's loads and its first specialization both land behind
            // the wait.
            let prewarm = prewarm.as_deref();
            let settled = prewarm.is_none_or(|prewarm| prewarm.settled());
            if tour.timer >= PRESTREAM_SECONDS
                && (settled || tour.timer >= PRESTREAM_WAIT_MAX_SECONDS)
            {
                let held = tour.timer - PRESTREAM_SECONDS;
                if held > 1.0 {
                    let line = format!(
                        "stage {}: prestream wait held {held:.1} s past the floor ({})",
                        tour.stage,
                        prewarm.map_or("the pre-warm ended late".to_string(), |prewarm| {
                            prewarm.describe().to_string()
                        }),
                    );
                    tour.note(line);
                }
                let name = format!("{:02}-door", tour.stage);
                shoot(&mut commands, &mut tour, &name);
                // A screenshot is taken on a later frame; crossing now would photograph the far side.
                //
                // `--tour-bench`: at an outside `E` door, time the doorway first - the bench's
                // settle keeps these screenshots out of its numbers - and cross afterwards.
                let bench_door = tour
                    .door
                    .and_then(|door| doors.get(door).ok())
                    .filter(|(_, _, door, ..)| !door.auto_load)
                    .map(|(_, _, door, state, ..)| (door.ref_id, door_is_open(state)));
                let outside = active
                    .as_deref()
                    .is_some_and(|active| active.interior.is_none());
                match bench_door {
                    Some((ref_id, false)) if config.portal.tour_bench.is_some() && outside => {
                        let line = format!(
                            "stage {}: bench - door {ref_id:08X}: timing {BENCH_SECONDS:.0} s of frames (after {BENCH_SETTLE_SECONDS:.0} s) closed, open in view and open behind",
                            tour.stage
                        );
                        tour.note(line);
                        stop_walking(&mut keys);
                        tour.bench_samples.clear();
                        tour.enter(Phase::Bench(BenchState::Closed));
                    }
                    Some((ref_id, true)) if config.portal.tour_bench.is_some() && outside => {
                        let line = format!(
                            "stage {}: bench - door {ref_id:08X} is already open, so it is not benched",
                            tour.stage
                        );
                        tour.note(line);
                        start_crossing(&mut tour, &mut camera, player.as_deref_mut(), &targeting);
                    }
                    // What the portal camera itself renders: it renders only through a door that
                    // is open ([`crate::portal`]), and a scripted crossing through a closed door
                    // snaps to `XTEL`, so the door is opened first and photographed open
                    // ([`Phase::PortalOpen`]). A walk-through takes the same photograph at the
                    // first frame its door stands open (`WalkAction::Walk`), whether it opened it
                    // with `E` or found it open already.
                    Some((_, _)) if portal_texture.is_some() && player.is_none() => {
                        // Only a door that is shut is asked to open: `OpenDoor` is what `E` writes,
                        // and `E` at an open door *closes* it (`crate::door_animation`) - so a door
                        // the run's own crossing already opened at the arrival
                        // ([`OpenDestinationDoor`]) is left standing open, exactly as the walk
                        // leaves it ([`walk_action`]).
                        let already_open = tour
                            .door
                            .and_then(|door| doors.get(door).ok())
                            .is_some_and(|(_, _, _, state, ..)| door_is_open(state));
                        if !already_open && let Some(entity) = tour.door {
                            open_door.write(OpenDoor { door: entity });
                        }
                        tour.enter(Phase::PortalOpen);
                    }
                    _ => start_crossing(&mut tour, &mut camera, player.as_deref_mut(), &targeting),
                }
            }
        }
        Phase::PortalOpen => {
            let state = tour
                .door
                .and_then(|door| doors.get(door).ok())
                .map(|(_, _, _, state, ..)| state.copied());
            match portal_wait(state, tour.timer) {
                // Fully open, and the portal has had frames to draw the destination into its
                // target: photograph it, and the doorway as the main camera shows it.
                PortalWait::Open => tour.enter(Phase::PortalShot),
                PortalWait::Waiting => {}
                PortalWait::NeverOpened => {
                    let line = format!(
                        "stage {}: the door did not open within {WALK_OPEN_SECONDS:.0} s, so the portal view is not photographed",
                        tour.stage
                    );
                    tour.note(line);
                    start_crossing(&mut tour, &mut camera, player.as_deref_mut(), &targeting);
                }
                PortalWait::Missing => {
                    let line = format!(
                        "stage {}: the door was not in the world at all in the {WALK_OPEN_SECONDS:.0} s the tour waited for it (not spawned, or unloaded), so the portal view is not photographed",
                        tour.stage
                    );
                    tour.note(line);
                    start_crossing(&mut tour, &mut camera, player.as_deref_mut(), &targeting);
                }
            }
        }
        Phase::PortalShot => {
            if tour.timer >= PORTAL_SHOT_SECONDS {
                if let Some(portal) = &portal_texture {
                    shoot_portal_view(&mut commands, &mut tour, portal);
                }
                let name = format!("{:02}-door-open", tour.stage);
                shoot(&mut commands, &mut tour, &name);
                // The crossing waits a second before it activates the door (`Phase::Activate`),
                // so both photographs are of this side.
                start_crossing(&mut tour, &mut camera, player.as_deref_mut(), &targeting);
            }
        }
        Phase::Bench(state) => {
            match bench_step(tour.timer) {
                BenchStep::Settling => {}
                BenchStep::Timing => {
                    let ms = real_time.delta_secs_f64() * 1000.0;
                    tour.bench_samples.push(ms);
                }
                BenchStep::Done => {
                    let door = tour.door.and_then(|door| doors.get(door).ok());
                    let ref_id = door.map_or(0, |(_, _, door, ..)| door.ref_id);
                    let row = BenchRow::from_samples(ref_id, state, &tour.bench_samples);
                    tour.bench_samples.clear();
                    let line = format!("stage {}: bench {}", tour.stage, row.csv());
                    tour.note(line);
                    tour.bench_rows.push(row);
                    match (state, state.next()) {
                        (BenchState::Closed, _) => match tour.door.filter(|_| door.is_some()) {
                            Some(entity) => {
                                open_door.write(OpenDoor { door: entity });
                                tour.enter(Phase::BenchOpen);
                            }
                            None => {
                                // The crossing's own checks name a door that went away.
                                start_crossing(
                                    &mut tour,
                                    &mut camera,
                                    player.as_deref_mut(),
                                    &targeting,
                                );
                            }
                        },
                        (BenchState::OpenInView, Some(next)) => {
                            turn_the_view(&mut camera, player.as_deref_mut(), std::f32::consts::PI);
                            tour.enter(Phase::Bench(next));
                        }
                        _ => {
                            // Face the door again; the walk-through stands itself in front of it.
                            turn_the_view(&mut camera, player.as_deref_mut(), std::f32::consts::PI);
                            start_crossing(
                                &mut tour,
                                &mut camera,
                                player.as_deref_mut(),
                                &targeting,
                            );
                        }
                    }
                }
            }
        }
        Phase::BenchOpen => {
            let state = tour
                .door
                .and_then(|door| doors.get(door).ok())
                .map(|(_, _, _, state, ..)| state.copied());
            // Fully open: `Opening` already counts as open for the portal (`DoorState::is_open`),
            // but its swing is still playing, and the bench times the doorway at rest.
            match state {
                Some(Some(DoorState::Open { .. })) => {
                    let line = format!(
                        "stage {}: bench - door fully open after {:.1} s",
                        tour.stage, tour.timer
                    );
                    tour.note(line);
                    tour.enter(Phase::Bench(BenchState::OpenInView));
                }
                Some(_) if tour.timer < WALK_OPEN_SECONDS => {}
                _ => {
                    let line = format!(
                        "stage {}: bench - the door did not open within {WALK_OPEN_SECONDS:.0} s, so its open states are not timed",
                        tour.stage
                    );
                    tour.note(line);
                    start_crossing(&mut tour, &mut camera, player.as_deref_mut(), &targeting);
                }
            }
        }
        Phase::Activate => {
            if tour.timer >= 1.0 {
                match tour.door {
                    Some(door) if doors.get(door).is_ok() => {
                        activate.write(ActivateDoor { door });
                        tour.enter(Phase::Crossing);
                    }
                    _ => {
                        let line = format!(
                            "FAIL stage {}: the door unloaded before activation",
                            tour.stage
                        );
                        tour.note(line);
                        tour.failed = true;
                        tour.enter(Phase::LookAround(0));
                    }
                }
            }
        }
        Phase::Crossing => {
            if let Some(event) = crossed.read().last() {
                let line = format!(
                    "stage {}: crossed from {:08X} into \"{}\", camera now at {:?}",
                    tour.stage, event.from_ref_id, event.label, camera.translation
                );
                tour.note(line);
                tour.stage += 1;
                tour.enter(Phase::Settle);
            } else if tour.timer >= crossing_timeout(&config) {
                let line = format!(
                    "FAIL stage {}: no DoorCrossed within {:.1} s",
                    tour.stage,
                    crossing_timeout(&config)
                );
                tour.note(line);
                tour.failed = true;
                tour.enter(Phase::LookAround(0));
            }
        }
        Phase::WalkThrough => {
            // The crossing comes first: the frame it happens in is the frame the cell just left
            // unloads in, so the door and its transform are usually gone by the time the tour sees
            // `DoorCrossed`, and looking for the door first would call that a failure.
            if let Some(event) = crossed.read().last() {
                stop_walking(&mut keys);
                let line = format!(
                    "stage {}: walked through the doorway into \"{}\", camera at {:?} on frame {}",
                    tour.stage, event.label, camera.translation, tour.frame
                );
                tour.note(line);
                let swap = tour.frame;
                // The frame this is seen in is photographed like every other one: it is the frame
                // the crossing is read in, so it is the first one on the far side of the doorway,
                // and leaving it out would leave a hole in the window either side of the swap -
                // the only frames the walk-through is judged on. Ten before the swap, the swap and
                // ten after it are all kept (`walk_frames`).
                capture_walk_frame(&mut commands, &mut tour, &camera);
                // The swing check's last word on the door the player just walked through: the
                // crossing unloads it with the cell it stood in, so the states the walk has seen
                // are all the check will ever get.
                tour.settle_swing(true);
                tour.enter(Phase::WalkThroughAfter { swap });
                return;
            }
            let Some((
                door_entity,
                door_transform,
                door,
                open,
                animation,
                anchor,
                local,
                instance,
                expected,
            )) = tour.door.and_then(|door| doors.get(door).ok())
            else {
                stop_walking(&mut keys);
                let line = format!(
                    "FAIL stage {}: the door unloaded before the walk-through",
                    tour.stage
                );
                let snapshot = walk_pose_snapshot(camera.translation, player.as_deref(), None);
                tour.save_failed_walk(&line, &snapshot);
                tour.note(line);
                tour.failed = true;
                tour.settle_swing(true);
                tour.enter(Phase::LookAround(0));
                return;
            };
            // The two door checks, from the state the portal and the crossing read.
            //
            // The swing: the walk presses `E` at this door every frame until it is open, and a door
            // whose model has an `Open` clip has to swing rather than open with its whole model
            // hidden ([`SwingWatch`]). Written as soon as the check is decided, so a door that hid
            // is named in the log at the frame it did.
            tour.swing.observe(
                door.ref_id,
                animation.is_some_and(DoorAnimation::swings),
                open.copied(),
            );
            tour.settle_swing(false);
            // The far door: the door this link leads to, which the player will arrive in front of.
            // Its check runs after the swap ([`FAR_DOOR_FRAMES`]); the link is read here, every
            // frame, because the source door is gone by then.
            tour.crossing = Some(Crossing {
                source: door.ref_id,
                destination: door.destination.destination_ref_id,
                anchored: anchor.is_some(),
            });
            // How far the player still is from the doorway, and whether walking is getting them
            // anywhere: the ladder of standoffs below is chosen from these two.
            let frame = door_frame(door_transform.rotation(), door.outward);
            let in_front =
                distance_in_front_of_door(door_transform.translation(), frame, camera.translation);
            let grounded = player.as_deref().is_some_and(|player| player.grounded);
            // What a player does at the door this frame: press `E` once, stand still until it is
            // open, then walk ([`walk_action`]). The press is only made where the player's own `E`
            // would pick the intended door ([`door_in_reach`]), which is where the standoff was chosen from.
            let state = open.copied();
            let action = walk_action(
                door.auto_load,
                state,
                tour.walk_pressed,
                door_in_reach(
                    camera.translation,
                    player.as_deref(),
                    door_entity,
                    &targeting,
                ),
            );
            if tour.walk_observed != Some((action, state)) {
                debug!(
                    stage = tour.stage,
                    door = format_args!("{:08X}", door.ref_id),
                    ?action, ?state,
                    pose = %walk_pose_snapshot(camera.translation, player.as_deref(), Some((door_transform.translation(), frame))),
                    "tour walk action/state changed"
                );
                tour.walk_observed = Some((action, state));
            }
            // The two timers the standoff ladder below is chosen from are about the walk. Standing
            // still in front of a door that is opening is what this walk is doing, not a standoff
            // the player cannot walk in from, so they do not run while it waits.
            if action == WalkAction::Walk {
                tour.walk_fell = if grounded {
                    0.0
                } else {
                    tour.walk_fell + time.delta_secs()
                };
                if in_front < tour.walk_furthest - WALK_PROGRESS {
                    tour.walk_furthest = in_front;
                    tour.walk_stuck = 0.0;
                } else {
                    tour.walk_stuck += time.delta_secs();
                }
            }

            // The walk starts at a standoff the player can stand on and walk in from, which depends
            // on the door (see [`WALK_STANDOFFS`]). This one did not work if the player is falling
            // or getting no closer to the door, so the next standoff is tried. Preserve
            // that approach's bounded diagnostic ring separately from the walk being judged.
            if tour.walk_fell >= WALK_FALL_SECONDS || tour.walk_stuck >= WALK_STUCK_SECONDS {
                tour.walk_fell = 0.0;
                tour.walk_stuck = 0.0;
                tour.walk_standoff += 1;
                let Some(standoff) = WALK_STANDOFFS.get(tour.walk_standoff) else {
                    stop_walking(&mut keys);
                    let line = format!(
                        "FAIL stage {}: the player cannot walk in from any of the standoffs in front of door {:08X} ({:?})",
                        tour.stage, door.ref_id, WALK_STANDOFFS
                    );
                    let snapshot = walk_pose_snapshot(
                        camera.translation,
                        player.as_deref(),
                        Some((door_transform.translation(), frame)),
                    );
                    tour.save_failed_walk(&line, &snapshot);
                    tour.note(line);
                    tour.failed = true;
                    tour.enter(Phase::LookAround(0));
                    return;
                };
                let was = WALK_STANDOFFS[tour.walk_standoff - 1];
                let snapshot = walk_pose_snapshot(
                    camera.translation,
                    player.as_deref(),
                    Some((door_transform.translation(), frame)),
                );
                tour.retain_walk_attempt(
                    &format!("approach from {was:.0} units made no progress; next standoff {standoff:.0}; grounded={grounded}"),
                    &snapshot,
                );
                stand_in_front_of_door(
                    &mut camera,
                    player.as_deref_mut(),
                    DoorGeometry {
                        global: door_transform,
                        local,
                        anchor,
                        instance,
                        expected,
                    },
                    door,
                    *standoff,
                );
                tour.walk_furthest = f32::INFINITY;
                let line = format!(
                    "stage {}: the walk from {was:.0} units in front of the door is going nowhere (grounded={grounded}); walking in from {standoff:.0} units instead",
                    tour.stage
                );
                tour.note(line);
                return;
            }

            // A door the walk pressed `E` at and is still standing in front of: it has its own line,
            // because nothing else in this stage would say why the walk never went anywhere.
            if action == WalkAction::Wait && tour.timer >= WALK_OPEN_SECONDS {
                stop_walking(&mut keys);
                let state = match state {
                    Some(state) => format!("state {state:?}"),
                    None => "no state of its own".to_owned(),
                };
                let outcome = if tour.walk_open_noted {
                    "opened, but the approach/crossing did not complete"
                } else {
                    "never opened"
                };
                let snapshot = walk_pose_snapshot(
                    camera.translation,
                    player.as_deref(),
                    Some((door_transform.translation(), frame)),
                );
                let line = format!(
                    "FAIL stage {}: door {:08X} {outcome} in the {WALK_OPEN_SECONDS:.0} s walk-through window ({state}, pressed_E={}; {snapshot})",
                    tour.stage, door.ref_id, tour.walk_pressed
                );
                tour.save_failed_walk(&line, &snapshot);
                tour.note(line);
                tour.failed = true;
                tour.settle_swing(true);
                tour.enter(Phase::LookAround(0));
                return;
            }

            // Do what a player does at the door. `E` is pressed once, as a fresh press (released
            // and pressed again, so the controller sees it whichever order the two systems run in),
            // and never again: `E` on an open door closes it, and a second press cancels an opening
            // whose destination is still streaming in (`crate::door_animation`). `W` goes down only
            // once the door is fully open - the leaf is solid until its swing is complete, so
            // walking at a door that is still closed or swinging is walking into it.
            match action {
                WalkAction::Press => {
                    keys.release(KeyCode::KeyE);
                    keys.press(KeyCode::KeyE);
                    tour.walk_pressed = true;
                }
                WalkAction::Wait => {
                    keys.release(KeyCode::KeyE);
                    keys.release(KeyCode::KeyW);
                }
                WalkAction::Walk => {
                    // The first frame of the walk at this door, whether or not `E` was pressed to
                    // get here: an auto-load marker, and a door the crossing opened at the arrival,
                    // are walked at with no press at all, and the portal draws the destination
                    // through them just as it does through a door the walk opened itself.
                    if !tour.walk_open_noted {
                        tour.walk_open_noted = true;
                        let line = format!(
                            "stage {}: door {:08X} open after {:.1} s (pressed_E={}); walking in from {:.0} units",
                            tour.stage,
                            door.ref_id,
                            tour.timer,
                            tour.walk_pressed,
                            WALK_STANDOFFS[tour.walk_standoff]
                        );
                        tour.note(line);
                        // The door is fully open and the player has not moved yet: the portal
                        // camera is drawing the destination, so photograph its target.
                        if let Some(portal) = &portal_texture {
                            shoot_portal_view(&mut commands, &mut tour, portal);
                        }
                    }
                    keys.press(KeyCode::KeyW);
                }
            }

            // Photograph walking near either side of the doorway plane. A requested crossing
            // can wait for its destination proof behind the plane, and those are still pre-swap
            // frames. The existing bounded ring keeps the most recent approach/wait images.
            let (_, ring, capture_distance) = walk_capture(&config);
            if walk_capture_in_range(action, in_front, capture_distance) {
                capture_walk_frame(&mut commands, &mut tour, &camera);
                // Everything older than the ring goes, files and all: the swap this walk is heading
                // for cannot use it (`WALK_RING`).
                tour.ring_walk_frames(ring);
            }

            if tour.timer >= WALK_THROUGH_SECONDS {
                stop_walking(&mut keys);
                let snapshot = walk_pose_snapshot(
                    camera.translation,
                    player.as_deref(),
                    Some((door_transform.translation(), frame)),
                );
                let line = format!(
                    "FAIL stage {}: no crossing after {WALK_THROUGH_SECONDS:.0} s of walking at the door ({} frames captured; {snapshot})",
                    tour.stage, tour.walk_captured
                );
                tour.save_failed_walk(&line, &snapshot);
                tour.note(line);
                tour.failed = true;
                tour.settle_swing(true);
                tour.enter(Phase::LookAround(0));
            }
        }
        Phase::WalkThroughAfter { swap } => {
            // The far-door check: the doorway the player arrived in has to be open. A crossing of
            // an anchored doorway lands in the destination doorway itself, so without the arrival
            // open the player arrives inside a closed leaf - the far door the portal had drawn out
            // of the way (impl-152). A door the data does not anchor lands at its `XTEL` point,
            // clear of the far door, so its crossing is not checked and the line says why.
            if let Some(crossing) = tour.crossing
                && !tour.far_door_checked
                && tour.frame >= swap + FAR_DOOR_FRAMES
            {
                tour.far_door_checked = true;
                let Crossing {
                    source,
                    destination,
                    anchored,
                } = crossing;
                let found = doors
                    .iter()
                    .find(|(_, _, door, ..)| door.ref_id == destination)
                    .map(|(_, _, _, open, ..)| open.copied());
                let line = if !anchored {
                    format!(
                        "stage {}: far door - door {source:08X} has no doorway anchor, so the \
                         crossing lands at the link's arrival point rather than in the doorway \
                         {destination:08X}: nothing to check",
                        tour.stage
                    )
                } else {
                    match far_door(found) {
                        FarDoor::Open => format!(
                            "stage {}: far door ok - {destination:08X}, the doorway the crossing \
                             through {source:08X} landed in, is open",
                            tour.stage
                        ),
                        FarDoor::Closed => {
                            tour.failed = true;
                            let state = match found.flatten() {
                                Some(state) => format!("state {state:?}"),
                                None => "no state of its own".to_owned(),
                            };
                            format!(
                                "FAIL stage {}: the far door {destination:08X} of the crossing \
                                 through {source:08X} is not open {} frames after the player arrived \
                                 in its doorway ({state})",
                                tour.stage, FAR_DOOR_FRAMES
                            )
                        }
                        FarDoor::Missing => {
                            tour.failed = true;
                            format!(
                                "FAIL stage {}: the far door {destination:08X} of the crossing \
                                 through {source:08X} is not in the world {} frames after the player \
                                 arrived in its doorway",
                                tour.stage, FAR_DOOR_FRAMES
                            )
                        }
                    }
                };
                tour.note(line);
            }
            let (walk_window, ..) = walk_capture(&config);
            if tour.frame <= swap + walk_window {
                capture_walk_frame(&mut commands, &mut tour, &camera);
                return;
            }
            // The window the frames are kept for: ten frames before the swap and ten after it.
            // Nothing else the walk photographed is kept - the ring has already deleted the older
            // frames of the approach as they fell out of it, and this deletes the rest of the tail,
            // so what is left in the folder is the window and the file that describes it.
            let first = swap.saturating_sub(walk_window);
            let last = swap + walk_window;
            tour.keep_walk_window(first, last);
            let window = tour.walk_frames.len();
            let directory = tour.walk_directory();
            let header = format!("swap frame {swap}\nwindow {first}..={last} ({window} frames)");
            if let Err(error) = tour.write_walk_metadata(&header) {
                warn!(
                    "could not write {}: {error}",
                    directory.join("frames.txt").display()
                );
            }
            let line = format!(
                "stage {}: walk-through crossed on frame {swap}; {} frames photographed, {window} kept in the window {first}..={last} ({})",
                tour.stage,
                tour.walk_captured,
                directory.display()
            );
            tour.note(line);
            tour.clear_walk_frames();
            // Both door checks of this crossing are written; the next stage's walk starts fresh.
            tour.crossing = None;
            tour.far_door_checked = false;
            tour.stage += 1;
            tour.enter(Phase::Settle);
        }
        Phase::UserPose => {
            if let Some(origin) = origin.as_deref() {
                const USER_POSE_POSITION: [f32; 3] = [20819.85, -46157.305, -2.1349945];
                const USER_POSE_YAW: f32 = -120.21704;
                const USER_POSE_PITCH: f32 = 9.327718;
                camera.translation =
                    render_position(Vec3::from_array(USER_POSE_POSITION), origin.0);
                camera.rotation = shot_camera_rotation(USER_POSE_YAW, USER_POSE_PITCH);
                tour.note(
                    "moved to the user's reported pose (capture 2026-09-25_02-01-35, shot 03)",
                );
                shoot(&mut commands, &mut tour, "02-user-pose-03");
            }
            finish_short_tour(&mut tour);
        }
        Phase::LookAround(view) => {
            if tour.timer >= LOOK_AROUND_SECONDS {
                if view < 3 {
                    turn_the_view(
                        &mut camera,
                        player.as_deref_mut(),
                        std::f32::consts::FRAC_PI_2,
                    );
                    let name = format!("{:02}-view-{}", tour.stage, view + 1);
                    shoot(&mut commands, &mut tour, &name);
                    tour.enter(Phase::LookAround(view + 1));
                } else if let Some(grounded) = player.as_deref().map(|player| player.grounded)
                    && !tour.failed
                    && !tour.walked
                {
                    // The last quarter of the survey: back to the heading the player arrived with.
                    // A crossing through an anchored doorway lands in the doorway itself, and the
                    // survey's third view there faces the door frame, not the street.
                    turn_the_view(
                        &mut camera,
                        player.as_deref_mut(),
                        std::f32::consts::FRAC_PI_2,
                    );
                    let line = format!(
                        "walk test: grounded={grounded} at {:?}; holding W for 4 s",
                        camera.translation
                    );
                    tour.note(line);
                    let start = camera.translation;
                    tour.walked = true;
                    tour.enter(Phase::Walk { start });
                } else {
                    let verdict = if tour.failed { "FAILED" } else { "PASSED" };
                    tour.finish(verdict);
                }
            }
        }
        Phase::Walk { start } => {
            if tour.timer < 4.0 {
                keys.press(KeyCode::KeyW);
            } else {
                keys.release(KeyCode::KeyW);
                let moved = camera.translation - start;
                let horizontal = Vec2::new(moved.x, moved.z).length();
                let grounded = player.as_deref().is_some_and(|player| player.grounded);
                let line = format!(
                    "walk test: moved {horizontal:.0} units horizontally, {:.0} vertically, grounded={grounded}",
                    moved.y
                );
                tour.note(line);
                if horizontal < 100.0 || !grounded {
                    tour.note("FAIL walk test: the player did not walk on the ground");
                    tour.failed = true;
                }
                shoot(&mut commands, &mut tour, "05-walked");
                tour.enter(Phase::LookAround(3));
            }
        }
        Phase::Done => {
            // Give the last screenshot a moment to reach the disk.
            if tour.timer >= DONE_SECONDS {
                exit.write(if tour.failed {
                    AppExit::error()
                } else {
                    AppExit::Success
                });
            }
        }
    }
}

/// Ends a `--tour-doors` run: a short tour stops before the route-end look-around and the walk
/// test, and its verdict says it was short.
fn finish_short_tour(tour: &mut DemoTour) {
    let verdict = if tour.failed { "FAILED" } else { "PASSED" };
    tour.finish(&format!("{verdict} (short tour)"));
}

/// Lets go of the keys the walk-through holds, whichever way it ended.
/// Leaves the front of the door the tour is standing at for the crossing: with a player, the
/// walk-through (press `E`, wait, walk in); without one, [`Phase::Activate`].
fn start_crossing(
    tour: &mut DemoTour,
    camera: &mut Transform,
    player: Option<&mut Player>,
    targeting: &TourDoors,
) {
    let doors = &targeting.doors;
    if player.is_some() {
        // A player drives the camera: open the door and walk through it instead, which
        // is the crossing this tour is here to check. The walk starts closer in than
        // the photograph was taken from (`WALK_STANDOFF`).
        tour.walk_standoff = 0;
        tour.walk_fell = 0.0;
        tour.walk_stuck = 0.0;
        tour.walk_furthest = f32::INFINITY;
        // `E` has not been pressed at this door yet, and its wait has nothing to say.
        tour.walk_pressed = false;
        tour.walk_open_noted = false;
        tour.walk_observed = None;
        // The two door checks start with the walk: a fresh swing check, and no crossing
        // looked at yet.
        tour.swing = SwingWatch::default();
        tour.crossing = None;
        tour.far_door_checked = false;
        if let Some((entity, transform, door, open, _, anchor, local, instance, expected)) =
            tour.door.and_then(|door| doors.get(door).ok())
        {
            // A door the walk has to press `E` at has to be one the key reaches from
            // where the walk stands, and the walk stands still until the door is open:
            // the standoff is the first in the list `E` reaches the door from
            // ([`standoff_reaches_door`]). A door that needs no press - an auto-load
            // marker, or one the crossing opened at the arrival - is walked at from
            // where the walk has always started.
            let geometry = DoorGeometry {
                global: transform,
                local,
                anchor,
                instance,
                expected,
            };
            tour.walk_standoff = if door.auto_load || door_is_open(open) {
                0
            } else {
                WALK_STANDOFFS
                    .iter()
                    .position(|standoff| {
                        standoff_reaches_door(
                            *standoff,
                            entity,
                            geometry,
                            door,
                            camera.translation,
                            targeting,
                        )
                    })
                    .unwrap_or(0)
            };
            stand_in_front_of_door(
                camera,
                player,
                geometry,
                door,
                WALK_STANDOFFS[tour.walk_standoff],
            );
        }
        let line = format!(
            "stage {}: walk-through - pressing E once, waiting for the door, then walking through the doorway, photographing the frames around the crossing",
            tour.stage
        );
        tour.note(line);
        tour.begin_walk_frames();
        tour.enter(Phase::WalkThrough);
    } else {
        tour.enter(Phase::Activate);
    }
}

fn stop_walking(keys: &mut ButtonInput<KeyCode>) {
    keys.release(KeyCode::KeyW);
    keys.release(KeyCode::ShiftRight);
    keys.release(KeyCode::KeyE);
}

/// What the walk-through does at the door this frame: what a player does at a door, which is to
/// press `E` at it once, stand still while it opens, and then walk through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalkAction {
    /// Ask the door to open: it is closed, in `E` reach, and this walk has not asked yet.
    Press,
    /// Stand still: the door has been asked to open and is on its way, or it is closed and out of
    /// `E` reach, which is nothing to press at.
    Wait,
    /// Hold `W`: the doorway is open and can be walked through.
    Walk,
}

/// What the walk-through does at the door, from the door's state and what the walk has already
/// done: `pressed` is whether it has pressed `E` at this door yet, and `in_reach` whether the
/// player's own `E` would pick the door from here ([`door_in_reach`]).
///
/// The press is made **once**, and the door is then waited on. Two reasons, both of them about what
/// `E` does besides opening: `E` on an open door closes it, so a press that keeps up with the door
/// shuts the doorway the walk is trying to use, and a second press on a door whose destination is
/// still streaming in cancels the opening the first one is holding
/// ([`crate::seamless_door_animation`]'s pending openings).
///
/// Only [`DoorState::Open`] is walked at. [`DoorState::Opening`] leaves the doorway usable for the
/// portal and the crossing - the leaf has started to move out of the opening - but the leaf is still
/// a body a walking player is stopped by, which is what this walk kept doing before it waited:
/// walking into the leaf from the standoff made no progress, and the standoff ladder
/// ([`WALK_STANDOFFS`]) then moved the player back until the walk succeeded, which is a spot the
/// walk never chose. A door with no state of its own at all counts as closed, as it does for
/// [`door_is_open`].
fn walk_action(
    auto_load: bool,
    state: Option<DoorState>,
    pressed: bool,
    in_reach: bool,
) -> WalkAction {
    if auto_load {
        // An auto-load marker has no leaf and no `E`: walking into it is the crossing.
        return WalkAction::Walk;
    }
    match state.unwrap_or_default() {
        DoorState::Open { .. } => WalkAction::Walk,
        DoorState::Closed if in_reach && !pressed => WalkAction::Press,
        DoorState::Closed | DoorState::Opening | DoorState::Closing => WalkAction::Wait,
    }
}

/// Capture eligibility only: an accepted walk can await proof behind the doorway
/// plane. Finite signed proximity keeps both sides inside the existing window.
fn walk_capture_in_range(action: WalkAction, signed_distance: f32, capture_distance: f32) -> bool {
    action == WalkAction::Walk
        && signed_distance.is_finite()
        && signed_distance.abs() <= capture_distance
}

/// Whether the player's actual targeting would select the intended door now.
/// Competing doors, active-space isolation and measured aim points participate.
fn door_in_reach(
    eye: Vec3,
    player: Option<&Player>,
    intended: Entity,
    targeting: &TourDoors,
) -> bool {
    player.is_some_and(|player| targeting.targeted(eye, player.forward()) == Some(intended))
}

/// Try the same pose/aim/player pitch clamp that the real stand helper will apply,
/// then ask production targeting against every eligible door. No cone is copied here.
fn standoff_reaches_door(
    standoff: f32,
    intended: Entity,
    geometry: DoorGeometry<'_>,
    door: &LoadDoor,
    fallback_eye: Vec3,
    targeting: &TourDoors,
) -> bool {
    let mut pose = Transform::from_translation(fallback_eye);
    let mut player = Player::default();
    stand_in_front_of_door(&mut pose, Some(&mut player), geometry, door, standoff);
    door_in_reach(pose.translation, Some(&player), intended, targeting)
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
    fn certification_wait_does_not_spend_the_initial_settle_window() {
        let mut tour = DemoTour::new(PathBuf::from("unused-tour-output"));
        tour.settle_quiet = 9;
        let scene = initial_scene(true);
        let metrics = StreamingMetrics::default();
        for _ in 0..30 {
            assert_eq!(
                tour.advance_automation(1.0, Some(&PackageValidationStatus::Pending), None, None),
                Ok(false)
            );
        }
        assert_eq!(tour.frame, 0);
        assert_eq!(tour.timer, 0.0);
        assert_eq!(tour.settle_quiet, 0);
        assert_eq!(tour.phase, Phase::Settle);
        assert_eq!(tour.log.lines().count(), 1, "the wait is logged once");
        assert_eq!(
            tour.advance_automation(
                0.25,
                Some(&PackageValidationStatus::Ready),
                Some(&scene),
                Some(&metrics)
            ),
            Ok(true)
        );
        assert_eq!(tour.frame, 1);
        assert_eq!(tour.timer, 0.25);
        assert_eq!(tour.settle_quiet, 0);
        assert_eq!(
            tour.advance_automation(
                0.25,
                Some(&PackageValidationStatus::Ready),
                Some(&scene),
                Some(&metrics)
            ),
            Ok(true)
        );
        assert_eq!(tour.timer, 0.5, "ordinary settling continues after release");
    }

    #[test]
    fn tour_waits_for_initial_upload_then_uses_ordinary_settle_for_later_work() {
        let mut tour = DemoTour::new(PathBuf::from("unused-tour-output"));
        let scene = initial_scene(false);
        let mut metrics = StreamingMetrics::default();
        let ready = Some(&PackageValidationStatus::Ready);
        assert_eq!(
            tour.advance_automation(20.0, ready, None, Some(&metrics)),
            Ok(false)
        );
        for _ in 0..30 {
            assert_eq!(
                tour.advance_automation(1.0, ready, Some(&scene), Some(&metrics)),
                Ok(false)
            );
        }
        assert_eq!(tour.frame, 0);
        assert_eq!(tour.timer, 0.0);
        assert!(!tour.initial_load_ready);
        scene.blocks[0]
            .resident
            .store(true, std::sync::atomic::Ordering::Release);
        metrics.pending_cluster_batches = 2;
        assert_eq!(
            tour.advance_automation(20.0, ready, Some(&scene), Some(&metrics)),
            Ok(false)
        );
        metrics.pending_cluster_batches = 0;
        assert_eq!(
            tour.advance_automation(0.25, ready, Some(&scene), Some(&metrics)),
            Ok(true)
        );
        assert!(tour.initial_load_ready);
        assert_eq!(tour.timer, 0.25);
        metrics.pending_cluster_batches = 1;
        assert_eq!(
            tour.advance_automation(SETTLE_SECONDS, ready, Some(&scene), Some(&metrics)),
            Ok(true)
        );
        let pending = SettleCounts {
            pending_cluster_batches: 1,
            ..Default::default()
        };
        assert!(
            tour_settle_ready(&pending, tour.timer).is_err(),
            "later work cannot reopen the startup gate"
        );
        assert_eq!(
            tour.log.matches("waiting for initial cluster load").count(),
            1
        );
        assert!(tour.log.contains("resident_blocks=0/0"));
    }

    #[test]
    fn failed_certification_cannot_advance_but_stock_startup_can() {
        let mut tour = DemoTour::new(PathBuf::from("unused-tour-output"));
        let failure = PackageValidationStatus::Failed("invalid certificate".into());
        let reason = tour
            .advance_automation(20.0, Some(&failure), None, None)
            .unwrap_err();
        assert!(reason.contains("invalid certificate"));
        assert_eq!(tour.frame, 0);
        assert_eq!(tour.timer, 0.0);
        assert_eq!(tour.advance_automation(0.5, None, None, None), Ok(true));
        assert_eq!(tour.frame, 1);
        assert_eq!(tour.timer, 0.5);
    }

    #[test]
    fn settle_debug_snapshot_ignores_only_the_quiet_streak() {
        let initial = SettleCounts {
            space_resident: true,
            quiet_frames: 1,
            ..Default::default()
        };
        let later_quiet = SettleCounts {
            quiet_frames: crate::shots::SETTLE_QUIET_FRAMES - 1,
            ..initial
        };
        assert_eq!(
            settle_debug_snapshot(&initial),
            settle_debug_snapshot(&later_quiet)
        );
        assert_eq!(
            later_quiet.quiet_frames,
            crate::shots::SETTLE_QUIET_FRAMES - 1,
            "diagnostics must not mutate actual counts"
        );
        let pending = SettleCounts {
            pending_lod_chunks: 1,
            ..later_quiet
        };
        assert_ne!(
            settle_debug_snapshot(&initial),
            settle_debug_snapshot(&pending)
        );
        let failed = SettleCounts {
            space_failed: true,
            ..later_quiet
        };
        assert_ne!(
            settle_debug_snapshot(&initial),
            settle_debug_snapshot(&failed)
        );
        let reason = tour_settle_ready(&later_quiet, SETTLE_SECONDS).unwrap_err();
        let expected = format!(
            "quiet_frames={}/{}",
            crate::shots::SETTLE_QUIET_FRAMES - 1,
            crate::shots::SETTLE_QUIET_FRAMES
        );
        assert!(reason.contains(&expected), "{reason}");
    }

    #[test]
    fn tour_settle_requires_current_readiness_without_timeout_capture_permission() {
        let quiet = SettleCounts {
            space_resident: true,
            quiet_frames: crate::shots::SETTLE_QUIET_FRAMES,
            ..Default::default()
        };
        let failed_cluster = SettleCounts {
            failed_cluster_batches: 1,
            ..quiet
        };
        assert!(!tour_settle_ready(&failed_cluster, 0.0).unwrap());
        assert!(
            tour_settle_ready(&failed_cluster, SETTLE_SECONDS)
                .unwrap_err()
                .contains("failed_cluster_batches=1")
        );
        assert_eq!(tour_settle_ready(&quiet, SETTLE_SECONDS), Ok(true));
        let pending = SettleCounts {
            pending_cluster_batches: 1,
            ..quiet
        };
        assert_eq!(tour_settle_ready(&pending, SETTLE_SECONDS - 0.1), Ok(false));
        let reason = tour_settle_ready(&pending, SETTLE_SECONDS).unwrap_err();
        assert!(reason.contains("pending_cluster_batches=1"), "{reason}");
    }

    /// Bevy keeps one window screenshot per frame and drops the rest, so the tour asks for one: a
    /// second request in the same tour frame - the short tour's arrival and user-pose shots used to
    /// share one - is refused and logged, and the next frame may shoot again.
    #[test]
    fn a_tour_frame_asks_for_one_window_screenshot() {
        let directory = tempfile::tempdir().expect("temporary tour directory");
        let mut tour = DemoTour::new(directory.path().to_path_buf());
        let mut world = World::new();
        let count = |world: &mut World| {
            world
                .query_filtered::<(), With<Screenshot>>()
                .iter(world)
                .count()
        };

        tour.frame = 7;
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        shoot(&mut commands, &mut tour, "07-arrived");
        shoot(&mut commands, &mut tour, "07-user-pose");
        let camera = Transform::default();
        capture_walk_frame(&mut commands, &mut tour, &camera);
        queue.apply(&mut world);
        assert_eq!(count(&mut world), 1, "one screenshot asked for on frame 7");
        assert!(tour.log.contains("07-user-pose.png not taken"));
        assert!(
            tour.walk_frames.is_empty(),
            "the walk frame was not recorded"
        );

        tour.frame = 8;
        let mut commands = Commands::new(&mut queue, &world);
        shoot(&mut commands, &mut tour, "08-user-pose");
        queue.apply(&mut world);
        assert_eq!(count(&mut world), 2, "the next frame shoots again");
    }

    /// The Riverwood route's door pairs, as `door_links` has them: `2k` leads into a house and
    /// `2k + 1` is the door inside it that leads back out to Tamriel. Written out here rather than
    /// read from the converted database, so the test runs in CI, which has no game data.
    const RIVERWOOD_PAIRS: [(u32, u32); 4] = [
        (0x0001_CBB0, 0x0001_CBAF), // Sven's House (interior 0001CB84)
        (0x0001_341F, 0x0001_33E9), // the Riverwood Trader (000133C9)
        (0x0001_3420, 0x0001_33FB), // Alvor and Sigrid's House (000133C8)
        (0x0001_3424, 0x0001_3419), // the Sleeping Giant Inn (000133C6)
    ];

    #[test]
    fn a_demo_name_resolves_to_its_route() {
        let route = |demo: Option<&str>| route_for_demo(demo).map(|route| route.name);
        assert_eq!(
            route(Some("riverwood")),
            Some("riverwood"),
            "the Riverwood demo has a route of its own"
        );
        assert_eq!(route(Some("alftand")), Some("alftand"));
        assert_eq!(
            route(Some("blackreach")),
            Some("alftand"),
            "the Blackreach demo starts at the far end of the Alftand route and walks it back"
        );
        assert_eq!(route(None), None, "a run with no --demo has no named route");
        assert_eq!(
            route(Some("nowhere")),
            None,
            "an unknown name has no route either, as `--demo` leaves no demo behind for one"
        );
    }

    #[test]
    fn a_run_without_a_route_walks_the_alftand_one() {
        let fallback = route_for_run(None);
        for demo in [None, Some("nowhere")] {
            assert_eq!(route_for_run(demo).name, fallback.name);
            assert_eq!(route_for_run(demo).doors, fallback.doors);
        }
        assert_eq!(
            fallback.doors,
            &ALFTAND_ROUTE[..],
            "the fallback is the route --demo-tour has always walked"
        );
        assert_eq!(fallback.doors.len(), 4, "the Alftand route is four doors");
        assert_eq!(route_for_run(Some("riverwood")).name, "riverwood");
    }

    #[test]
    fn the_riverwood_route_is_four_houses_entered_and_left_again() {
        let doors = route_for_demo(Some("riverwood"))
            .expect("riverwood has a scripted route")
            .doors;
        assert_eq!(doors.len(), 8, "four houses, entered and left again");

        // Every pair is the door into a house and the door inside it that leads back out, in that
        // order: the tour leaves each house by a door standing in the cell it just arrived in.
        for (pair, (there, back)) in RIVERWOOD_PAIRS.iter().enumerate() {
            assert_eq!(
                doors[2 * pair],
                *there,
                "door {} leads into house {}",
                2 * pair + 1,
                pair + 1
            );
            assert_eq!(
                doors[2 * pair + 1],
                *back,
                "door {} is the way back out of house {}",
                2 * pair + 2,
                pair + 1
            );
        }
    }

    #[test]
    fn a_smoke_run_walks_the_first_doors_only() {
        let riverwood = route_for_demo(Some("riverwood")).expect("riverwood has a scripted route");
        assert_eq!(
            walked_doors(riverwood, None),
            &RIVERWOOD_ROUTE[..],
            "without --tour-doors the whole route is walked"
        );
        assert_eq!(
            walked_doors(riverwood, Some(1)),
            &RIVERWOOD_ROUTE[..1],
            "a smoke run walks the route's first door, which stands in the cell the run starts in"
        );
        assert_eq!(walked_doors(riverwood, Some(3)).len(), 3);
        assert_eq!(
            walked_doors(riverwood, Some(0)).len(),
            1,
            "a smoke run that names no door still walks one: a run that crosses nothing checks nothing"
        );
        assert_eq!(
            walked_doors(riverwood, Some(99)),
            &RIVERWOOD_ROUTE[..],
            "asking for more doors than the route has walks the route"
        );
    }

    /// `--tour-repeat N`: the doors the run walks, N times over, in the order it walks them. The
    /// Riverwood pair is the case the flag exists for - in through the house's door and out
    /// through its other one - and the repeat puts the run back at the first door afterwards.
    #[test]
    fn a_repeated_tour_walks_the_same_doors_again() {
        let riverwood = route_for_demo(Some("riverwood")).expect("riverwood has a scripted route");
        let pair = walked_doors(riverwood, Some(2));
        assert_eq!(
            (tour_crossings(pair, Some(5)), tour_crossings(pair, None)),
            (10, 2),
            "five repeats of a two-door sequence are ten crossings; with no flag the doors are walked once"
        );
        assert_eq!(
            (0..10)
                .map(|stage| door_at_stage(pair, stage))
                .collect::<Vec<_>>(),
            vec![
                RIVERWOOD_ROUTE[0],
                RIVERWOOD_ROUTE[1],
                RIVERWOOD_ROUTE[0],
                RIVERWOOD_ROUTE[1],
                RIVERWOOD_ROUTE[0],
                RIVERWOOD_ROUTE[1],
                RIVERWOOD_ROUTE[0],
                RIVERWOOD_ROUTE[1],
                RIVERWOOD_ROUTE[0],
                RIVERWOOD_ROUTE[1],
            ],
            "the repeat walks the same two doors over and over, not the rest of the route"
        );
        assert_eq!(
            tour_crossings(pair, Some(0)),
            2,
            "a repeat of nothing is the doors walked once: a tour that crosses nothing checks nothing"
        );
        // A repeat covers the whole route when no door count is named, and every door of the route
        // is still walked once per repeat.
        assert_eq!(tour_crossings(walked_doors(riverwood, None), Some(2)), 16);
    }

    #[test]
    fn the_settle_reads_the_cell_the_camera_stands_in() {
        // An interior is the cell the active cell names, whatever the camera's position is: an
        // interior has no grid to fall in.
        let inside = ActiveCell {
            worldspace_id: 0x3c,
            interior: Some(0x1_2F12),
        };
        assert_eq!(
            camera_space_key(&inside, None, Vec3::new(100.0, 0.0, -100.0)),
            Some(CellKey::Interior(0x1_2F12))
        );

        // In an exterior the grid is the camera's Creation position's, with the render origin put
        // back on: the same cell `plan_cells` streams from, so a rebased camera reads the same key.
        let outside = ActiveCell {
            worldspace_id: 0x3c,
            interior: None,
        };
        assert_eq!(
            camera_space_key(
                &outside,
                Some(&RenderOrigin(IVec2::new(1, 2))),
                Vec3::new(100.0, 0.0, -100.0)
            ),
            Some(CellKey::Exterior {
                worldspace_id: 0x3c,
                grid_x: 1,
                grid_y: 2,
            }),
            "x 100 + 1 cell east, y 100 + 2 cells north: cell 1,2"
        );
        assert_eq!(
            camera_space_key(&outside, None, Vec3::ZERO),
            None,
            "with no render origin there is no grid to name, which the settle reads as not resident"
        );
    }

    /// The portal target's HDR capture becomes an 8-bit sRGB image Bevy can write: values under
    /// white are encoded as sRGB, a highlight past white is clamped, and a NaN is black.
    #[test]
    fn an_hdr_capture_is_converted_to_srgb8_for_saving() {
        let mut hdr = Image::new_fill(
            Extent3d {
                width: 3,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0; 8],
            TextureFormat::Rgba16Float,
            default(),
        );
        hdr.set_color_at(0, 0, Color::linear_rgba(0.214, 0.0, 1.0, 1.0))
            .unwrap();
        hdr.set_color_at(1, 0, Color::linear_rgba(8.0, 40.0, 0.5, 1.0))
            .unwrap();
        hdr.set_color_at(2, 0, Color::linear_rgba(f32::NAN, -1.0, 0.0, 1.0))
            .unwrap();
        let out = hdr_to_srgb8(&hdr);
        assert_eq!(out.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        assert_eq!(out.size(), UVec2::new(3, 1));
        let data = out.data.as_ref().unwrap();
        // Linear 0.214 is sRGB ~0.5 (127 or 128).
        assert!((127..=128).contains(&data[0]), "{}", data[0]);
        assert_eq!(&data[1..4], &[0, 255, 255]);
        assert_eq!(&data[4..6], &[255, 255], "past white clamps");
        assert_eq!(&data[8..12], &[0, 0, 0, 255], "NaN and negatives are black");
        assert!(out.try_into_dynamic().is_ok(), "a format Bevy can save");
    }

    #[test]
    fn a_walk_through_frame_is_recognised_by_its_name() {
        assert!(is_walk_frame_name(std::path::Path::new("f00123.png")));
        assert!(!is_walk_frame_name(std::path::Path::new("frames.txt")));
        assert!(
            !is_walk_frame_name(std::path::Path::new("00-arrived.png")),
            "the stage's own screenshots are not the walk-through's frames"
        );
        assert!(
            !is_walk_frame_name(std::path::Path::new("f1234.png")),
            "a frame file is f and five digits, as `capture_walk_frame` writes it"
        );
    }

    #[test]
    fn every_route_crosses_each_door_once() {
        for (demo, route) in DEMO_ROUTES {
            let mut crossed = Vec::new();
            for door in route.doors {
                assert!(
                    !crossed.contains(door),
                    "{demo}: the {} route crosses {door:08X} twice",
                    route.name
                );
                crossed.push(*door);
            }
            assert!(
                !route.doors.is_empty(),
                "{demo}: a route with no doors is not a route"
            );
        }
    }

    /// A door of the Sven's House route pair, and its state, for the swing check's tests.
    const WALKED_DOOR: u32 = 0x0001_CBB0;

    /// The swing check's failure: a door that had a clip to play and opened with `animated: false`
    /// anyway, hiding its whole model. Every tour this repo ran passed while every animated door in
    /// play did that (the user, 2026-09-24; `d420bf9`, and the check proposed in research-158).
    #[test]
    fn a_door_that_opens_without_swinging_fails_the_swing_check() {
        let mut watch = SwingWatch::default();
        watch.observe(WALKED_DOOR, true, Some(DoorState::Closed));
        assert_eq!(watch.verdict(false), None, "a closed door says nothing yet");
        watch.observe(WALKED_DOOR, true, Some(DoorState::Open { animated: false }));

        assert_eq!(
            watch.verdict(false),
            Some(Swing::WithoutSwinging),
            "the door's model has clips, so opening with nothing to animate is the defect"
        );
        let (verdict, line) = watch.line(0, false).expect("a decided check writes a line");
        assert_eq!(verdict, Swing::WithoutSwinging);
        assert!(
            line.starts_with("FAIL stage 0: "),
            "a failure says so: {line}"
        );
        assert!(
            line.contains("0001CBB0"),
            "and names the door that hid: {line}"
        );
    }

    /// The swing check's pass: the state passes through `Opening` - the leaf swings - and reaches
    /// `Open { animated: true }`, which is what a door whose model has an `Open` clip does.
    #[test]
    fn a_door_that_swings_passes_the_swing_check() {
        let mut watch = SwingWatch::default();
        watch.observe(WALKED_DOOR, true, Some(DoorState::Closed));
        watch.observe(WALKED_DOOR, true, Some(DoorState::Opening));
        assert_eq!(
            watch.verdict(false),
            None,
            "a swing under way is judged when it reaches Open, or when the walk ends"
        );
        watch.observe(WALKED_DOOR, true, Some(DoorState::Open { animated: true }));

        assert_eq!(watch.verdict(false), Some(Swing::Swung));
        let (_, line) = watch.line(3, false).expect("a decided check writes a line");
        assert!(
            line.starts_with("stage 3: swing ok - door 0001CBB0"),
            "the line names the stage and the door: {line}"
        );
        assert!(
            line.contains("Opening -> Open { animated: true }"),
            "and the states the walk saw: {line}"
        );
    }

    /// The swing is judged from the states the walk reached when it ends: the crossing unloads the
    /// door with the cell it stood in, and a leaf the player walked past mid-swing was still a door
    /// opening, not one that hid.
    #[test]
    fn a_swing_still_in_flight_when_the_walk_ends_is_no_failure() {
        let mut watch = SwingWatch::default();
        watch.observe(WALKED_DOOR, true, Some(DoorState::Opening));

        assert_eq!(watch.verdict(false), None, "the walk is still going");
        assert_eq!(
            watch.verdict(true),
            Some(Swing::Swung),
            "the swing was playing when the player walked through it"
        );
    }

    /// A door whose model has no `Open` clip of its own is what the unanimated fallback is for: an
    /// open one is a hole where its model stood, and the check does not call that a failure.
    #[test]
    fn a_door_with_no_clip_of_its_own_is_not_expected_to_swing() {
        let mut watch = SwingWatch::default();
        watch.observe(
            WALKED_DOOR,
            false,
            Some(DoorState::Open { animated: false }),
        );

        assert_eq!(watch.verdict(true), Some(Swing::NoClip));
        let (_, line) = watch.line(1, true).expect("a decided check writes a line");
        assert!(line.starts_with("stage 1: swing ok"), "it passes: {line}");
        assert!(
            line.contains("has no Open clip of its own"),
            "and says why there was nothing to swing: {line}"
        );
    }

    /// A walk that ended without the door opening at all has no swing to judge: it is a failure of
    /// the walk, which the walk reports in its own words, and not a second one here.
    #[test]
    fn a_door_the_walk_never_opened_is_not_a_swing_failure() {
        let mut watch = SwingWatch::default();
        watch.observe(WALKED_DOOR, true, Some(DoorState::Closed));

        assert_eq!(
            watch.verdict(true),
            Some(Swing::NeverOpened),
            "the door never opened while the tour was at it"
        );
        let (_, line) = watch
            .line(0, true)
            .expect("the check still writes its line");
        assert!(!line.contains("FAIL"), "and does not fail the tour: {line}");
    }

    /// The swing check writes one line per door, at the frame the check is decided, and the door is
    /// forgotten behind it: a door that hid is named once and the tour fails.
    #[test]
    fn the_swing_check_writes_one_line_per_door_and_fails_the_tour() {
        let mut tour = DemoTour::new(PathBuf::from("unused"));
        tour.swing
            .observe(WALKED_DOOR, true, Some(DoorState::Open { animated: false }));

        tour.settle_swing(false);
        assert!(tour.failed, "the door hid instead of swinging");
        assert_eq!(
            tour.log.lines().count(),
            1,
            "one line for the door: {:?}",
            tour.log
        );

        // The walk carries on at the door for as long as it takes to cross it, and the frames after
        // the line was written are not watched any more: one line per door, not one a frame.
        tour.swing
            .observe(WALKED_DOOR, true, Some(DoorState::Opening));
        tour.settle_swing(false);
        tour.settle_swing(true);
        assert_eq!(
            tour.log.lines().count(),
            1,
            "and never a second line for it: {:?}",
            tour.log
        );
    }

    /// The far-door check: what the tour makes of the door the link names, which the player arrived
    /// in front of. `Opening` counts as open - the doorway is one the player can walk out of from
    /// the first frame of the swing ([`DoorState::is_open`]) - and a door that is not there at all
    /// is a failure of its own.
    #[test]
    fn the_far_door_check_wants_the_doorway_open() {
        assert_eq!(
            far_door(Some(Some(DoorState::Open { animated: true }))),
            FarDoor::Open
        );
        assert_eq!(
            far_door(Some(Some(DoorState::Opening))),
            FarDoor::Open,
            "a far door still swinging is a doorway the player can walk out of"
        );
        assert_eq!(
            far_door(Some(Some(DoorState::Open { animated: false }))),
            FarDoor::Open,
            "a far door that opened as a hole is an open doorway too"
        );
        assert_eq!(
            far_door(Some(Some(DoorState::Closed))),
            FarDoor::Closed,
            "the player arrived inside a closed leaf, which is what impl-152 fixed"
        );
        assert_eq!(
            far_door(Some(None)),
            FarDoor::Closed,
            "a door with no state at all is as closed as one that says so"
        );
        assert_eq!(
            far_door(None),
            FarDoor::Missing,
            "no load door in the world carries the reference the link names"
        );
    }

    /// A door that is not in the world yet is waited for like a door that is opening, and given up
    /// on with a line of its own - not the "did not open within ten seconds" one, which the phase
    /// used to write on its very first frame for a door that was simply not spawned yet
    /// (review-258's finding 3). The two are told apart because the outer `None` (no door entity)
    /// and `Some(None)` (a door with no state of its own) mean different things.
    #[test]
    fn a_door_that_is_not_in_the_world_is_waited_for_and_named_separately() {
        assert_eq!(
            portal_wait(None, 0.0),
            PortalWait::Waiting,
            "a door that has not streamed in is not a door that failed to open"
        );
        assert_eq!(
            portal_wait(None, WALK_OPEN_SECONDS - 0.1),
            PortalWait::Waiting,
            "it is waited for as long as a door that is opening"
        );
        assert_eq!(
            portal_wait(None, WALK_OPEN_SECONDS),
            PortalWait::Missing,
            "and named as missing, not as one that did not open"
        );

        for present in [
            Some(DoorState::Closed),
            Some(DoorState::Opening),
            Some(DoorState::Closing),
            None,
        ] {
            assert_eq!(
                portal_wait(Some(present), 0.0),
                PortalWait::Waiting,
                "a door in the world that is not fully open is waited for ({present:?})"
            );
            assert_eq!(
                portal_wait(Some(present), WALK_OPEN_SECONDS),
                PortalWait::NeverOpened,
                "and named as one that did not open ({present:?})"
            );
        }

        for open in [
            DoorState::Open { animated: true },
            DoorState::Open { animated: false },
        ] {
            assert_eq!(
                portal_wait(Some(Some(open)), 0.0),
                PortalWait::Open,
                "an open door is photographed at once"
            );
        }
    }

    /// What a player does at a closed door: press `E` once, stand still while it opens, then walk.
    /// The press is made once and never repeated - `E` on an open door closes it
    /// ([`crate::player`]), and a second press cancels the opening the first one is holding
    /// ([`crate::seamless_door_animation`]) - so a door that has been asked to open is waited on.
    #[test]
    fn the_walk_presses_e_once_and_then_waits() {
        let closed = Some(DoorState::Closed);
        assert_eq!(
            walk_action(false, closed, false, true),
            WalkAction::Press,
            "a closed door within E reach is asked to open"
        );
        assert_eq!(
            walk_action(false, closed, true, true),
            WalkAction::Wait,
            "and a door this walk has already pressed at is waited on, never pressed at again"
        );
        assert_eq!(
            walk_action(false, None, false, true),
            WalkAction::Press,
            "a door with no state of its own counts as closed, as `door_is_open` reads it"
        );
        assert_eq!(
            walk_action(false, closed, false, false),
            WalkAction::Wait,
            "a door out of E reach is nothing to press at: the press would be lost, and E is asked \
             once"
        );
    }

    /// `W` goes down only for an open door. `Opening` means a leaf is still crossing the doorway:
    /// the portal may show the destination through it and the crossing is armed, but a player
    /// walking at it is walking into the leaf - which is what the tour used to do, and what left
    /// the first door of every run stuck against it until the standoff ladder moved the player out
    /// of E range and the walk went in from there.
    #[test]
    fn the_walk_never_holds_w_before_the_door_is_open() {
        let waiting = [
            None,
            Some(DoorState::Closed),
            Some(DoorState::Opening),
            Some(DoorState::Closing),
        ];
        for state in waiting {
            for pressed in [false, true] {
                for in_range in [false, true] {
                    let action = walk_action(false, state, pressed, in_range);
                    assert_ne!(
                        action,
                        WalkAction::Walk,
                        "holding W at {state:?} (pressed_E={pressed}, in_range={in_range}) is \
                         walking into a door that is not open"
                    );
                }
            }
        }
        for state in [
            DoorState::Open { animated: true },
            DoorState::Open { animated: false },
        ] {
            assert_eq!(
                walk_action(false, Some(state), false, true),
                WalkAction::Walk,
                "an open door is walked at without a press, however it opened - a door the crossing \
                 opened at the arrival is already open"
            );
            assert_eq!(
                walk_action(false, Some(state), true, false),
                WalkAction::Walk,
                "and one out of E reach is walked at too: there is nothing to press at an open door"
            );
        }
    }

    /// An auto-load door is crossed by walking into it: no `E` to press and nothing to wait for.
    #[test]
    fn the_walk_walks_into_an_auto_load_door() {
        for state in [DoorState::Closed, DoorState::Open { animated: false }] {
            assert_eq!(
                walk_action(true, Some(state), false, false),
                WalkAction::Walk,
                "an auto-load marker has no leaf and no E: walking into it is the crossing"
            );
        }
    }

    /// The bench times the three states in order - closed, open in view, open behind - and each
    /// one settles before its frames are timed.
    #[test]
    fn the_bench_times_closed_then_open_in_view_then_open_behind() {
        let mut order = vec![BenchState::Closed];
        while let Some(next) = order.last().and_then(|state| state.next()) {
            order.push(next);
        }
        assert_eq!(order, BenchState::ALL.to_vec());
        assert_eq!(
            order.iter().map(|state| state.name()).collect::<Vec<_>>(),
            ["closed", "open-in-view", "open-behind"]
        );

        assert_eq!(bench_step(0.0), BenchStep::Settling);
        assert_eq!(bench_step(BENCH_SETTLE_SECONDS - 0.01), BenchStep::Settling);
        assert_eq!(bench_step(BENCH_SETTLE_SECONDS), BenchStep::Timing);
        assert_eq!(
            bench_step(BENCH_SETTLE_SECONDS + BENCH_SECONDS - 0.01),
            BenchStep::Timing
        );
        assert_eq!(
            bench_step(BENCH_SETTLE_SECONDS + BENCH_SECONDS),
            BenchStep::Done
        );
    }

    /// A row's percentiles are `crate::metrics`'s nearest-rank ones, whatever order the frames came
    /// in, and the CSV line has the header's columns.
    #[test]
    fn a_bench_row_reports_the_frame_times_as_the_metrics_do() {
        let samples: Vec<f64> = (1..=100).rev().map(f64::from).collect();
        let row = BenchRow::from_samples(0x0001_CBB0, BenchState::OpenInView, &samples);
        assert_eq!(row.frames, 100);
        assert_eq!(row.mean_ms, 50.5);
        assert_eq!(row.p50_ms, 51.0);
        assert_eq!(row.p95_ms, 96.0);
        assert_eq!(row.p99_ms, 100.0);
        assert_eq!(
            row.csv(),
            "0001CBB0,open-in-view,100,50.500,51.000,96.000,100.000"
        );
        assert_eq!(
            BENCH_CSV_HEADER.split(',').count(),
            row.csv().split(',').count()
        );

        let empty = BenchRow::from_samples(1, BenchState::Closed, &[]);
        assert_eq!(empty.csv(), "00000001,closed,0,0.000,0.000,0.000,0.000");

        assert_eq!(
            bench_csv(std::slice::from_ref(&row)),
            format!("{BENCH_CSV_HEADER}\n{}\n", row.csv())
        );
        assert_eq!(bench_csv(&[]), format!("{BENCH_CSV_HEADER}\n"));
    }

    /// The summary has one line per state that was timed, each column averaged over the doors.
    #[test]
    fn the_bench_summary_averages_each_state_over_the_doors() {
        let rows = [
            BenchRow::from_samples(1, BenchState::Closed, &[2.0, 2.0]),
            BenchRow::from_samples(2, BenchState::Closed, &[4.0, 4.0]),
            BenchRow::from_samples(1, BenchState::OpenInView, &[10.0]),
        ];
        assert_eq!(
            bench_summary(&rows),
            [
                "bench closed: 2 door(s), mean 3.00 ms, p50 3.00 ms, p95 3.00 ms, p99 3.00 ms (averaged over the doors)",
                "bench open-in-view: 1 door(s), mean 10.00 ms, p50 10.00 ms, p95 10.00 ms, p99 10.00 ms (averaged over the doors)",
            ]
        );
        assert!(bench_summary(&[]).is_empty());
    }
}

#[cfg(test)]
mod failed_walk_metadata_tests {
    use super::*;

    #[test]
    fn failed_walk_uses_the_success_writer_and_preserves_retained_frames_and_poses() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("OWNER.txt"),
            "VG7 failed-walk metadata test",
        )
        .unwrap();
        let mut tour = DemoTour::new(directory.path().to_owned());
        tour.stage = 3;
        tour.frame = 1590;
        tour.walk_captured = 40;
        let folder = tour.walk_directory();
        std::fs::create_dir_all(&folder).unwrap();
        let image = folder.join("f01559.png");
        std::fs::write(&image, b"invented retained screenshot bytes").unwrap();
        tour.walk_frames.push(WalkFrame {
            frame: 1559,
            path: image.clone(),
            pose: "frame 1559 eye 1 2 3 forward 0 0 -1".into(),
        });
        tour.write_walk_metadata("swap frame 1560\nwindow 1550..=1570 (1 frames)")
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(folder.join("frames.txt")).unwrap(),
            "swap frame 1560\nwindow 1550..=1570 (1 frames)\n\nframe 1559 eye 1 2 3 forward 0 0 -1\n"
        );
        tour.save_failed_walk("opened but did not cross", "eye=now feet=now");
        let metadata = std::fs::read_to_string(folder.join("frames.txt")).unwrap();
        assert!(metadata.contains("failed stage 3 at frame 1590"));
        assert!(metadata.contains("40 frames photographed; 1 retained"));
        assert!(metadata.contains("opened but did not cross"));
        assert!(metadata.ends_with("frame 1559 eye 1 2 3 forward 0 0 -1\n"));
        assert_eq!(
            std::fs::read(&image).unwrap(),
            b"invented retained screenshot bytes"
        );
        assert_eq!(
            tour.walk_frames.len(),
            1,
            "writing diagnostic evidence cannot prune it"
        );
    }

    #[test]
    fn retries_preserve_prior_ring_and_late_writes_without_mixing_acceptance() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("OWNER.txt"),
            "VG7 approach evidence test",
        )
        .unwrap();
        let mut tour = DemoTour::new(directory.path().to_owned());
        tour.stage = 2;
        tour.frame = 40;
        tour.walk_captured = 8;
        let first = tour.walk_directory();
        std::fs::create_dir_all(&first).unwrap();
        let early = first.join("f00039.png");
        let late = first.join("f00040.png");
        std::fs::write(&early, b"first approach").unwrap();
        for (frame, path) in [(39, early.clone()), (40, late.clone())] {
            tour.walk_frames.push(WalkFrame {
                frame,
                path,
                pose: format!("frame {frame} first approach pose"),
            });
        }
        tour.retain_walk_attempt("falling", "eye=before-reposition feet=falling");
        let archived = std::fs::read_to_string(first.join("frames.txt")).unwrap();
        assert!(archived.contains("abandoned approach stage 2 attempt 0"));
        assert!(archived.contains("8 frames photographed; 2 retained"));
        assert!(archived.contains("eye=before-reposition feet=falling"));
        assert!(archived.contains("frame 40 first approach pose"));
        assert!(tour.walk_frames.is_empty());
        assert_eq!(tour.walk_captured, 0);
        assert!(
            !tour.failed,
            "diagnostic archival must not change retry verdict"
        );
        let retry = tour.walk_directory();
        assert_eq!(retry, first.join("attempt-01"));
        std::fs::create_dir_all(&retry).unwrap();
        // Simulate the asynchronous screenshot landing after the approach switch.
        std::fs::write(&late, b"late first approach").unwrap();
        let current = retry.join("f00041.png");
        std::fs::write(&current, b"retry approach").unwrap();
        tour.walk_frames.push(WalkFrame {
            frame: 41,
            path: current.clone(),
            pose: "frame 41 retry pose".into(),
        });
        tour.walk_captured = 1;
        tour.keep_walk_window(41, 41);
        tour.save_failed_walk("standoffs exhausted", "eye=retry-final");
        let failed = std::fs::read_to_string(retry.join("frames.txt")).unwrap();
        assert!(failed.contains("standoffs exhausted"));
        assert!(failed.contains("eye=retry-final"));
        assert!(failed.contains("frame 41 retry pose"));
        assert!(!failed.contains("first approach pose"));
        assert_eq!(
            std::fs::read_to_string(first.join("frames.txt")).unwrap(),
            archived
        );
        assert_eq!(std::fs::read(&early).unwrap(), b"first approach");
        assert_eq!(std::fs::read(&late).unwrap(), b"late first approach");
        assert_eq!(std::fs::read(&current).unwrap(), b"retry approach");
    }

    #[test]
    fn new_stage_resets_attempt_directory_without_deleting_prior_evidence() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("OWNER.txt"),
            "VG7 new-stage evidence test",
        )
        .unwrap();
        let mut tour = DemoTour::new(directory.path().to_owned());
        tour.walk_attempt = 2;
        let prior = tour.walk_directory();
        tour.save_failed_walk("last attempt", "old pose");
        tour.walk_frames.push(WalkFrame {
            frame: 12,
            path: prior.join("f00012.png"),
            pose: "old stage pose".into(),
        });
        tour.walk_captured = 5;
        tour.stage = 1;
        tour.begin_walk_frames();
        assert_eq!(tour.walk_attempt, 0);
        assert_eq!(
            tour.walk_directory(),
            directory.path().join(WALK_DIRECTORY).join("01")
        );
        assert!(prior.join("frames.txt").exists());
        assert!(tour.walk_frames.is_empty());
        assert_eq!(tour.walk_captured, 0);
    }

    #[test]
    fn failure_before_first_capture_still_records_current_pose_without_claiming_a_frame() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("OWNER.txt"),
            "VG7 failed-walk empty metadata test",
        )
        .unwrap();
        let mut tour = DemoTour::new(directory.path().to_owned());
        let player = Player {
            grounded: true,
            velocity: Vec3::new(0.0, -4.0, 0.0),
            ..default()
        };
        let pose = walk_pose_snapshot(
            Vec3::new(10.0, 140.0, 30.0),
            Some(&player),
            Some((Vec3::ZERO, Quat::IDENTITY)),
        );
        tour.save_failed_walk("door unavailable", &pose);
        let folder = tour.walk_directory();
        let metadata = std::fs::read_to_string(folder.join("frames.txt")).unwrap();
        assert!(metadata.contains("0 frames photographed; 0 retained"));
        assert!(metadata.contains("feet=Vec3(10.0, 20.0, 30.0)"));
        assert!(metadata.contains("grounded=Some(true)"));
        assert!(metadata.contains("velocity=Some(Vec3(0.0, -4.0, 0.0))"));
        assert_eq!(
            std::fs::read_dir(folder).unwrap().count(),
            1,
            "metadata only: no screenshot was manufactured"
        );
    }
}

#[cfg(test)]
mod production_tour_target_tests {
    use super::*;
    use bevy::ecs::system::SystemState;

    fn door(id: u32) -> LoadDoor {
        LoadDoor {
            ref_id: id,
            destination: crate::doors::DoorDestination {
                destination_ref_id: id + 100,
                interior_cell_id: Some(id),
                worldspace_id: None,
                arrival_position: [0.0; 3],
                arrival_rotation: [0.0; 3],
            },
            label: "invented targeting fixture".into(),
            auto_load: false,
            outward: None,
        }
    }
    fn spawn(
        world: &mut World,
        id: u32,
        position: Vec3,
        local_scale: Vec3,
        global_scale: Vec3,
    ) -> Entity {
        let local = Transform::from_translation(position).with_scale(local_scale);
        let global =
            GlobalTransform::from(Transform::from_translation(position).with_scale(global_scale));
        let bounds =
            ExpectedModelBounds::new(Vec3::new(-60.0, 0.0, -10.0), Vec3::new(60.0, 240.0, 30.0))
                .unwrap();
        world.spawn((door(id), local, global, bounds)).id()
    }

    #[test]
    fn close_standoff_uses_production_measured_aim_and_range_not_reference_foot_cone() {
        let mut world = World::new();
        let intended = spawn(
            &mut world,
            1,
            Vec3::new(1000.0, 0.0, -300.0),
            Vec3::ONE,
            Vec3::ONE,
        );
        let mut state = SystemState::<TourDoors>::new(&mut world);
        let targeting = state.get(&world).unwrap();
        let (_, global, link, _, _, anchor, local, instance, expected) =
            targeting.doors.get(intended).unwrap();
        let geometry = DoorGeometry {
            global,
            local,
            anchor,
            instance,
            expected,
        };
        assert!(standoff_reaches_door(
            60.0,
            intended,
            geometry,
            link,
            Vec3::ZERO,
            &targeting
        ));
        assert!(!standoff_reaches_door(
            320.0,
            intended,
            geometry,
            link,
            Vec3::ZERO,
            &targeting
        ));
        assert_eq!(
            WALK_STANDOFFS
                .iter()
                .position(|distance| standoff_reaches_door(
                    *distance,
                    intended,
                    geometry,
                    link,
                    Vec3::ZERO,
                    &targeting
                )),
            Some(0)
        );
        let mut pose = Transform::default();
        let mut player = Player::default();
        stand_in_front_of_door(&mut pose, Some(&mut player), geometry, link, 60.0);
        assert_eq!(
            targeting.targeted(pose.translation, player.forward()),
            Some(intended)
        );
        assert!(!door_in_reach(pose.translation, None, intended, &targeting));
    }

    #[test]
    fn actual_stacked_door_candidates_and_local_scale_choose_the_intended_measured_door() {
        let mut world = World::new();
        let base = Vec3::new(1000.0, 0.0, -300.0);
        let intended = spawn(&mut world, 1, base, Vec3::ONE, Vec3::splat(4.0));
        let upper = spawn(
            &mut world,
            2,
            base + Vec3::new(16.0, 224.0, 36.3),
            Vec3::ONE,
            Vec3::ONE,
        );
        let mut state = SystemState::<TourDoors>::new(&mut world);
        let targeting = state.get(&world).unwrap();
        let (_, global, link, _, _, anchor, local, instance, expected) =
            targeting.doors.get(intended).unwrap();
        let geometry = DoorGeometry {
            global,
            local,
            anchor,
            instance,
            expected,
        };
        let mut pose = Transform::default();
        let mut player = Player::default();
        stand_in_front_of_door(&mut pose, Some(&mut player), geometry, link, 160.0);
        let actual_aim = door_aim_point(
            global.translation(),
            global.rotation(),
            local.scale,
            anchor,
            instance,
            expected,
        );
        let wrong_scale_aim = door_aim_point(
            global.translation(),
            global.rotation(),
            global.scale(),
            anchor,
            instance,
            expected,
        );
        assert_ne!(
            actual_aim, wrong_scale_aim,
            "fixture detects incorrectly borrowing the global scale"
        );
        assert!(
            player
                .forward()
                .dot((actual_aim - pose.translation).normalize())
                > 0.9999
        );
        assert_eq!(
            targeting.targeted(pose.translation, player.forward()),
            Some(intended)
        );
        assert!(!door_in_reach(
            pose.translation,
            Some(&player),
            upper,
            &targeting
        ));
    }

    #[test]
    fn door_local_transform_query_is_disjoint_from_mutable_streaming_camera() {
        let mut world = World::new();
        world.spawn((Transform::default(), StreamingCamera));
        let intended = spawn(
            &mut world,
            1,
            Vec3::new(10.0, 0.0, -300.0),
            Vec3::ONE,
            Vec3::ONE,
        );
        let mut state =
            SystemState::<(Query<&mut Transform, With<StreamingCamera>>, TourDoors)>::new(
                &mut world,
            );
        let (mut cameras, targeting) = state.get_mut(&mut world).unwrap();
        assert_eq!(cameras.iter_mut().count(), 1);
        assert_eq!(targeting.doors.iter().count(), 1);
        assert!(targeting.doors.get(intended).is_ok());
    }
}

#[cfg(test)]
mod walk_capture_range_tests {
    use super::*;

    #[test]
    fn pre_swap_capture_includes_nearby_behind_plane_walks_but_excludes_unrelated_frames() {
        for window in [CAPTURE_DISTANCE, CLIP_DISTANCE] {
            for distance in [window, 1.0, 0.0, -1.0, -window] {
                assert!(
                    walk_capture_in_range(WalkAction::Walk, distance, window),
                    "both sides of the seam are pre-swap evidence: {distance} within {window}"
                );
            }
            for distance in [
                window + 1.0,
                -window - 1.0,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ] {
                assert!(!walk_capture_in_range(WalkAction::Walk, distance, window));
            }
            for action in [WalkAction::Press, WalkAction::Wait] {
                assert!(!walk_capture_in_range(action, 1.0, window));
                assert!(!walk_capture_in_range(action, -1.0, window));
            }
        }
    }
}
