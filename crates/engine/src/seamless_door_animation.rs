//! Load doors that open with their own model's animation: the state machine behind
//! [`DoorState`](crate::doors::DoorState).
//!
//! A door model's NIF carries the `Open` and `Close` controller sequences that swing it, and the
//! converter bakes them into glTF animation clips on the same `.glb` the model is loaded from.
//! This module finds those clips on a load door's model, builds the door an
//! [`AnimationGraph`] of its own - the glTF loader gives the spawned scene an [`AnimationPlayer`]
//! but no graph, and `advance_animations` needs both - and drives
//! `Closed -> Opening -> Open -> Closing` from `E` ([`OpenDoor`](crate::transition::OpenDoor)) and
//! from the playing clip's own clock.
//!
//! # A door with no animation
//!
//! Most door models carry no clips at all (596 of the 768 door NIFs in this install), and until the
//! assets are reconverted none does. Such a door is promoted to [`DoorState::Open`] in the frame it
//! is activated: the doorway opens in the same frame the player asks for it, which is what a door
//! did before there were clips.
//!
//! # The door that barely moves, and the twin it borrows its swing from
//!
//! A load door is the door the game hides behind a loading screen, so its own `Open` clip only has
//! to move the leaf far enough to be seen before the screen comes down: the demo route's
//! `DweDoorLarge01Load` turns its two leaves 5.4 and 8.7 degrees. The engine has no loading screen,
//! and a door that turns 5 degrees is a wall with a wall behind it - while the game ships a
//! non-load twin of the same door (`dwemerlargedoor01.nif` beside `dwemerlargedoorload01.nif`)
//! whose `Open` clip is the real 86-degree swing.
//!
//! So when a door's own `Open` clip does not clear the doorway
//! ([`DOORWAY_CLEAR_DEGREES`](crate::doors::DOORWAY_CLEAR_DEGREES)), the engine gives it a swing in
//! two steps.
//!
//! First it looks the twin up - the same model path with the marker removed ([`twin_model_path`]) -
//! and plays the twin's `Open`/`Close` clips instead, rebuilt onto the door's own nodes, because an
//! [`AnimationTargetId`] is a hash of a node's whole name path and the two models name their roots
//! differently. A twin is used only when every node its clips move is a node the door's own model
//! has, and that is the exception rather than the rule: 17 of the install's 19 twinned pairs name
//! their leaves something else entirely - the demo route's own door turns `Object02` and `Object43`
//! and its twin turns `Object45` and `Object47` (`tools/research/load_door_twin_coverage.py pairs`).
//!
//! Then, for every door the twin cannot help, the door's **own** clip is scaled up until it opens
//! the doorway ([`swung_open_clips`]): the nodes, the hinge each leaf turns about and the way it
//! turns are already in the clip, and only the distance is short, so each leaf is turned as many
//! times further from the pose it starts on as it takes to stand open ([`swing_factors`]: each leaf
//! of a double door by its own factor, and a handle on a leaf not at all). That is what gives every
//! load door its swing,
//! and it changes nothing but how far the door opens. A clip that already clears the doorway is
//! never touched, and a door with no clip at all stays the static door it is.
//!
//! # What is drawn while a door opens
//!
//! The leaf is the point of the animation, so it is drawn while the `Open` clip plays: a door whose
//! leaf vanished the moment it was asked to open would be no better than the teleport this
//! replaces. What happens when the clip is done depends on how far it turned the leaf
//! ([`DOORWAY_CLEAR_DEGREES`](crate::doors::DOORWAY_CLEAR_DEGREES)):
//!
//! * a leaf that swung clear of the opening (a Nordic or Imperial door, 115-135 degrees) stays
//!   drawn, swung open, like a real door;
//! * a leaf that did not (the demo route's `DweDoorLarge01Load` turns its leaves 5-9 degrees) is
//!   hidden at `Open`, so the doorway is the opening the leaf failed to make - and only the leaves
//!   are hidden, never the frame, which is what keeps the doorway looking like a doorway;
//! * a door with no clip of its own has no leaf that can move at all, and there the whole model has
//!   to go - but that line is the portal's to write, from
//!   [`hides_whole_reference`](crate::doors::DoorState::hides_whole_reference).
//!
//! While the `Close` clip plays the leaves are drawn again from its first frame: a leaf that is
//! coming back is a leaf.
//!
//! # A door waits for the space behind it
//!
//! An opening asked for at a **closed load door whose destination is not streamed in** is held
//! ([`PendingDoorOpens`]) rather than started: a door that swings before the room behind it is
//! there opens onto a hole. [`destination_is_loaded`] is the question - every cell the plan
//! pre-streams for that door ([`destination_keys`](crate::transition::destination_keys)), resident
//! in full - and the held request starts the swing the frame it is true. A door with no
//! destination, and every door whose far side is already there, opens at once as it always has;
//! a close is never held, and neither is a door that is already swinging.
//!
//! # The door left open
//!
//! A door the player opens and walks away from closes itself ([`auto_close_doors`], the user's demo
//! note of 2026-09-25): once the camera is [`AUTO_CLOSE_DISTANCE`] away the door is asked for its
//! close through the message `E` writes, so the swing, the pose it starts from and the state it
//! lands in are the same as for the key. The far side it opens onto stays streamed in until the
//! leaf is home ([`crate::transition::plan_door_prestream`] holds the destination of every open and
//! closing door wherever the camera is), which is what keeps the player from seeing the room
//! behind the doorway disappear: what they see, if they look back, is a door shutting.
//!
//! # The far door of a crossing
//!
//! A crossing does not only move the camera: a [`CrossDoor`] through a doorway-anchored door lands
//! the player at the **destination doorway's plane** (`crate::transition`), which is where the far
//! door of the link stands, so that door has to be out of the way in the frame the player arrives
//! ([`OpenDestinationDoor`], [`open_arrival_doors`]). It is opened the way the window showed it - at
//! the point of its own `Open` clip that the source door's own clip had reached - and it is left
//! open afterwards, which is also what lets the portal render back through it once the player has
//! walked on.
//!
//! # What the rest of the engine reads
//!
//! [`DoorState`](crate::doors::DoorState) sits on the door reference next to
//! [`LoadDoor`](crate::doors::LoadDoor) and is the one answer to "is this door open":
//! [`is_open`](crate::doors::DoorState::is_open) is true from the first frame of the swing onward,
//! and that is what the crossing gates on.
//! [`DoorLeaf`](crate::doors::DoorLeaf) marks the moving part of the model - the nodes the clips
//! have curves for, not the frame around them - and
//! [`mesh_is_out_of_the_way`](crate::doors::mesh_is_out_of_the_way) is what the walk probe asks
//! about a mesh it hit: a drawn leaf mid-swing must not trap the player.
//!
//! # What wires this module up
//!
//! `PortalPlugin` adds this plugin for interactive runs (`docs/design/portal-plugin.md`), so a run
//! that is looked at rather than measured has it and a benchmark run does not:
//!
//! ```ignore
//! app.add_plugins(crate::seamless_door_animation::SeamlessDoorAnimationPlugin);
//! ```
//!
//! The rest of the wiring is one line each in the modules that ask the question:
//! `portal::show_load_door_leaves` hides the leaf of an open door and the whole model of a *static*
//! one, `player::player_walk` skips a leaf its door has taken out of the doorway, and
//! [`crate::transition::door_is_open`] - which the portal's door choice, the player's plane trigger
//! and the demo tour's walk-through all read - is [`DoorState::is_open`].

use crate::{
    config::EngineConfig,
    doors::{DOORWAY_CLEAR_DEGREES, DoorAnchor, DoorLeaf, DoorState, LoadDoor, OPEN_FRACTION},
    model_animation::{InvalidSourceClip, SourceClipBinding, SourceClips},
    player::{auto_door_trigger, feet_from_eye},
    streaming::{PendingModel, StreamingWorld},
    transition::{CrossingHeld, OpenDestinationDoor, OpenDoor, destination_is_loaded},
    world::components::{ExpectedModelBounds, InstanceBounds, MeshHandle, StreamingCamera},
};
use bevy::{
    animation::{
        AnimationClip, AnimationPlayer, AnimationTargetId, animated_field,
        animation_curves::{AnimatableCurve, AnimatableKeyframeCurve},
        graph::{AnimationGraph, AnimationGraphHandle, AnimationNodeIndex},
        transition::AnimationTransitions,
    },
    prelude::*,
    transform::helper::TransformHelper,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// The clip name a door's opening sequence is expected to carry. Matched case-insensitively:
/// `Open` is the convention, `open` exists too, and `Forward`/`Backward` are other names entirely
/// (`tools/research/door_animation_nif.py census`).
const OPEN_CLIP: &str = "open";

/// The clip name a door's closing sequence is expected to carry, matched the same way.
const CLOSE_CLIP: &str = "close";

/// How long a door takes to cross-fade from one clip into the other (design section 2.3).
///
/// A reversal starts the new clip at the pose the old one reached, so the fade covers the mismatch
/// between the two sequences rather than the swing: the demo route's large door opens in 0.6 s and
/// closes in 0.6333 s, and a door that is halfway open when it is told to close is given the rest
/// of the swing as a fresh heading rather than a jump.
const CLIP_FADE: Duration = Duration::from_millis(150);

/// How far the camera has to be from a door left open before the door closes itself
/// ([`auto_close_doors`]), in Creation units: the user's demo note of 2026-09-25, "doors should
/// automatically do the close animation if the player opens it and walks away from it. It should
/// close before the interior is unloaded, so the player doesn't see", and idea 5 of
/// `docs/design/portal-performance.md` - an open door left behind keeps its far side streamed in
/// and its doorway rendered for nothing.
///
/// **Inside [`DOOR_PRESTREAM_RADIUS`](crate::transition::DOOR_PRESTREAM_RADIUS) (800) on purpose,
/// which is the other way round from what idea 5 asked for.** That idea wanted the distance beyond
/// the pre-stream radius so a player turning back never sees a door close in their face; what the
/// user's note asks for is that the player never sees the far side *unload*, and the close is what
/// unloads it: [`crate::transition::plan_door_prestream`] now holds an open or closing door's
/// destination wherever the camera is, so the far side is there for the whole swing. At 600 units
/// the door is still inside the plan's own reach as well, so the request that keeps the far side
/// there does not depend on the state machine and the plan agreeing on the same frame.
///
/// 600 is four seconds of walking ([`WALK_SPEED`](crate::player::WALK_SPEED) is 150 units a
/// second) and two and a half times [`DOOR_RANGE`](crate::player::DOOR_RANGE), the reach `E`
/// targets a door from: by the time a door counts as left behind, the player is past any chance of
/// pressing `E` at it again.
const AUTO_CLOSE_DISTANCE: f32 = 600.0;

/// How many frames a load door waits for its twin's own model asset before asking the asset server
/// for it ([`twin_swing`]).
///
/// The twin's file may be being loaded for references of it in the same cell load - the non-load
/// door of the same hall, the static that model is also placed as - and a request landing inside
/// that load is a second load of the twin's file, which re-instances every drawn copy of the twin
/// (the separately requested twin only). Much shorter than the wait for the door's own model: the door's swing is
/// what the player can see missing, so a twin that nothing is loading is asked for after a fifth of
/// a second rather than two.
const TWIN_ROOT_WAIT_FRAMES: u32 = 10;

/// How many frames a load door waits for its non-load twin before giving up on it and scaling its
/// own clip instead ([`twin_swing`]).
///
/// Much shorter than the source readiness boundary, because of what is being waited for: the door's own
/// scene is the thing the player can see is missing, and a door that never resolves is a door that
/// never opens - while a twin is the second thing tried, a door without one has a swing from its own
/// clip to fall back on, and until it falls back the door is still `Closed`: a player who reaches it
/// in that window gets a door that opens the way it did before any of this. The twin's path is
/// derived from the door's own model path, so its load starts in the same frame the door's own
/// model resolved, and half a second is several times what a converted model takes to arrive.
const TWIN_WAIT_FRAMES: u32 = 30;

/// How far a load door's leaves stand once the engine has given a door whose own clip barely turns
/// them a swing of its own: a door standing open, well past
/// [`DOORWAY_CLEAR_DEGREES`](crate::doors::DOORWAY_CLEAR_DEGREES) so that the doorway is unarguably
/// one and the leaf is visibly out of it.
const SWUNG_OPEN_DEGREES: f32 = 90.0;

/// The most a load door's own clip may be scaled up by ([`swing_scale`]).
///
/// A clip that hardly moves at all - `RiftenRWDoorLoad01` turns its leaf 2.9 degrees, and how far
/// that leaf's hinge is from its pivot is not in the clip - would need 31 times its own swing to
/// stand at [`SWUNG_OPEN_DEGREES`]. A swing blown up that far is inventing motion the model never
/// described, so the leaves stop here (that door's 2.9 degrees becomes 58) rather than being flung
/// past their hinges. Doors whose own swing is within this of the target - 4.5 degrees and up, which
/// is nearly all of them - reach it exactly.
const MAX_SWING_SCALE: f32 = 20.0;

/// The most a door's `Close` clip may start from where its `Open` clip ended, and end from where
/// `Open` started, and still count as the same swing run backwards ([`close_mirrors_open`]), in
/// degrees. The install's doors agree to a hundredth of a degree; this is the width of "the same".
const CLOSE_MIRROR_DEGREES: f32 = 1.0;

/// How many samples a door's own clip is rebuilt with per second of its length when it is scaled or
/// reversed.
const SWING_SAMPLES_PER_SECOND: f32 = 60.0;

/// The fewest and the most samples a rebuilt clip gets: a clip shorter than a frame is not a swing
/// at all, and a long one - the census's longest door sequence is 17.9 seconds - is not worth a
/// thousand keys to reproduce sample for sample.
const MIN_SWING_SAMPLES: u32 = 8;
const MAX_SWING_SAMPLES: u32 = 120;

/// The token in a load door's file name, and in the name of its model's own root node, that makes
/// it the door the game hides behind a loading screen. Its non-load twin - the model with the real
/// swing in it - is the same name with this taken out ([`twin_model_path`]).
const LOAD_MARKER: &str = "load";

/// The path of the model a load door would have if it were not a load door, or `None` when its own
/// path carries no [`LOAD_MARKER`] to take out - and so has no twin to look for.
///
/// The rule is read off the install rather than off one example
/// (`tools/research/load_door_twin_coverage.py`): of the 103 models Skyrim's load doors use, 31
/// carry a `load` marker in the file name and, for 19 of those, the same name without it is a model
/// that exists and is converted. The markers themselves are `Load01` (658 door references),
/// `LoadMarker01` (315, every one of them an invisible auto-load marker), `Load02` (123), `Load`
/// (54), `LoadUp01` (23), `LoadDown01` (21), `LoadDoor01` (2) and `LoadExt` (2) - always a `load`
/// immediately before the token that distinguishes the load door, which is why taking out the first
/// `load` of the file stem is the whole rule. The marker is case-insensitive (`AutoLoadMarker01`).
fn twin_model_path(model_path: &str) -> Option<String> {
    let (directory, file) = match model_path.rfind(['/', '\\']) {
        Some(separator) => model_path.split_at(separator + 1),
        None => ("", model_path),
    };
    let (stem, extension) = match file.rfind('.') {
        Some(dot) => file.split_at(dot),
        None => (file, ""),
    };
    Some(format!(
        "{directory}{}{extension}",
        strip_load_marker(stem)?
    ))
}

/// What [`twin_model_on_disk`] found for a load door's twin.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TwinLookup {
    /// The model's name carries no load marker: there is no twin to look for.
    NoMarker,
    /// The name has a marker, but the twin it derives was never converted - `MineDoor01.glb`
    /// (research-583), like 12 of the install's 31 marked models. The path is kept for the log.
    Missing(String),
    /// The twin's path, and it is there to load.
    Found(String),
}

/// The twin [`twin_model_path`] derives for `model_path`, checked against the converted asset tree
/// at `assets_root` before anything asks the asset server for it: a twin that does not exist is no
/// twin, not an asset failure for the log. With no root to check against (a test app with no
/// [`EngineConfig`]) the derived path is taken as found, which is how it behaved before the check.
fn twin_model_on_disk(model_path: &str, assets_root: Option<&std::path::Path>) -> TwinLookup {
    let Some(path) = twin_model_path(model_path) else {
        return TwinLookup::NoMarker;
    };
    match assets_root {
        Some(root) if !root.join(&path).is_file() => TwinLookup::Missing(path),
        _ => TwinLookup::Found(path),
    }
}

/// `name` with its first `load` - whatever its case - taken out, or `None` when it carries none.
///
/// This is the marker rule in both the places it is needed: on a model's file name, to find the
/// twin ([`twin_model_path`]), and on the name of the model's own root node, to turn a door node's
/// [`AnimationTargetId`] into the one the twin's clips use for it ([`twin_target_ids`]).
fn strip_load_marker(name: &str) -> Option<String> {
    let start = name.to_ascii_lowercase().find(LOAD_MARKER)?;
    let end = start + LOAD_MARKER.len();
    let stripped = format!("{}{}", &name[..start], &name[end..]);
    (!stripped.is_empty()).then_some(stripped)
}

/// The animation a load door's model resolved to, built once the model's `Gltf` asset and its clips
/// have loaded. Its absence means "still being worked out"; a door with every field empty is a
/// static leaf, and one with no clips at all never gets a clip to play.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(crate) struct DoorAnimation {
    /// The entity the glTF loader put the door's [`AnimationPlayer`] on: the animation root of the
    /// spawned scene, a descendant of the door reference. `None` for a model with no clips.
    player: Option<Entity>,
    /// The door's `Open` clip and the graph node that plays it, if the model has one.
    open: Option<DoorClip>,
    /// The door's `Close` clip, if the model has one. A model with only `Open` plays that and never
    /// closes (design section 5).
    close: Option<DoorClip>,
    /// Whether the `Open` clip swings the leaves far enough to leave the doorway open
    /// ([`DOORWAY_CLEAR_DEGREES`]). False for a static door, and for a clip that barely turns its
    /// leaves: both leave a doorway that is not one, so the leaves are hidden when the door is
    /// `Open`.
    clears_doorway: bool,
}

/// Explicit failure view for capture consumers. This marker never supplies static proof.
/// Kept for compatibility with existing failure/graph ownership fixtures.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct DoorAnimationGaveUp;

/// What a caller that wants to open a load door can rely on about the door's animation, read from
/// the `DoorAnimation` this module resolves and not guessed from how long the view has been quiet.
///
/// The absence of an [`AnimationPlayer`] on the door proves nothing: it is the state of a door still
/// resolving as much as of a door with no clip. `DoorAnimation` is the module's own answer, and
/// this is the question put to it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DoorActivation {
    /// No `DoorAnimation` yet (the model, its clips or its scene are still on their way, or the
    /// animation was lost and is being attached again), or one that swings but whose player and
    /// graph are not attached. Opening the door now would open it the static way, with no leaf.
    #[default]
    Resolving,
    /// The animation gave up resolving (`DoorAnimationGaveUp`). Terminal, and not ready: the door
    /// opens the static way for a player, but a caller that needs the real swing must not take it
    /// for a door that has none.
    GaveUp,
    /// Resolved, with no `Open` clip: a static leaf, whose opening is the only one it has.
    Static,
    /// Resolved with an `Open` clip, and the player carries the graph that plays it.
    Animated,
}

impl DoorActivation {
    /// `animation` is the door's [`DoorAnimation`] if it has one, `gave_up` whether it carries
    /// [`DoorAnimationGaveUp`], and `graph_player` whether an [`AnimationPlayer`] with its
    /// [`AnimationGraphHandle`] stands in the door's scene.
    pub(crate) fn of(animation: Option<&DoorAnimation>, gave_up: bool, graph_player: bool) -> Self {
        let Some(animation) = animation else {
            return Self::Resolving;
        };
        if gave_up {
            Self::GaveUp
        } else if !animation.swings() {
            Self::Static
        } else if graph_player {
            Self::Animated
        } else {
            Self::Resolving
        }
    }

    /// Whether the door can be asked to open and do what its model says: animated with its graph
    /// attached, or resolved static. Resolving and given-up doors cannot.
    pub fn ready(self) -> bool {
        matches!(self, Self::Static | Self::Animated)
    }
}

impl DoorAnimation {
    /// Whether the door's own model has an `Open` clip, so opening the door plays a swing rather
    /// than leaving it with nothing to animate.
    ///
    /// This is what tells a door that opens into a hole from one whose leaf moves: a door that
    /// answers false here is a static leaf, and [`DoorState::Open`] with `animated: false` is the
    /// only opening it has ([`DoorState::hides_whole_reference`]). The demo tour reads it to check
    /// that a door it opens with `E` really swings (research-158,
    /// `docs/research/checks-for-user-found-defects.md`).
    pub(crate) fn swings(&self) -> bool {
        self.open.is_some()
    }

    /// The entity this door recorded its [`AnimationPlayer`] on when it attached its clips, or
    /// `None` for a door with no clips. Callers that ask about the door's player read this one,
    /// rather than whichever player happens to stand under the door: a different graph-carrying
    /// descendant is not the player this animation drives.
    pub(crate) fn player(&self) -> Option<Entity> {
        self.player
    }

    /// The same animation with `player` recorded as its player, for tests that need the recorded
    /// player to differ from (or to be) a player they spawned.
    #[cfg(test)]
    pub(crate) fn with_player_for_test(mut self, player: Entity) -> Self {
        self.player = Some(player);
        self
    }

    /// A door animation with an `Open` and a `Close` clip and no player, for other modules' tests
    /// that only ask whether a door swings.
    #[cfg(test)]
    pub(crate) fn swinging_for_test() -> Self {
        let clip = DoorClip {
            node: AnimationNodeIndex::new(1),
            seconds: 1.0,
        };
        Self {
            player: None,
            open: Some(clip),
            close: Some(clip),
            clears_doorway: true,
        }
    }

    /// Whether `E` at this door while it is open would close it: the model has a `Close` clip to
    /// run, or no clip at all - the one-frame close a static leaf gets, the mirror of its
    /// one-frame opening.
    ///
    /// False for a model with an `Open` clip and no `Close`: activating that one replays its
    /// opening (design section 5), which is not a door closing, so
    /// [`crate::player`] offers no Close for it - the prompt would be a lie.
    pub(crate) fn can_close(&self) -> bool {
        self.close.is_some() || self.open.is_none()
    }
}

/// One of a door's clips: the node of the door's own animation graph that plays it, and its length.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DoorClip {
    /// The graph node [`AnimationPlayer::start`] takes to play this clip.
    node: AnimationNodeIndex,
    /// The clip's length in seconds (`AnimationClip::duration`), the denominator of
    /// [`OPEN_FRACTION`].
    seconds: f32,
}

/// A seamless derived swing waiting on the public source binding, or its optional twin.
#[derive(Component, Debug, Clone)]
struct PendingDoorModel {
    path: String,
    /// Binding identity for optional-twin work; a new source revision starts fresh.
    binding: Option<SourceClipBinding>,
    twin: Option<TwinModel>,
    /// Optional-twin wait only. Never a deadline for the streamed original source.
    waiting: u32,
    /// Synthetic Gltf fixture input, never used by production source resolution.
    #[cfg(test)]
    model: Option<Handle<Gltf>>,
}

/// Public binding revision from which this private active graph was derived.
#[derive(Component, Clone)]
struct DerivedSource(SourceClipBinding);

#[derive(Component)]
pub(crate) struct DerivedGraph(pub Handle<AnimationGraph>);

/// A door's non-load twin, loading or loaded: the model whose `Open`/`Close` clips are the door's
/// real swing.
#[derive(Component, Debug, Clone)]
struct TwinModel {
    /// The twin's path, as [`twin_model_path`] derived it - kept for the log line.
    path: String,
    /// The twin model's root `Gltf` asset.
    model: Handle<Gltf>,
}

/// What the engine did to a load door whose own `Open` clip does not clear the doorway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwingSource {
    /// The door's non-load twin's clips, rebuilt onto the door's own nodes
    /// ([`borrow_swing`]).
    Twin,
    /// The door's own clip, turned further than it was baked to turn
    /// ([`swung_open_clips`]).
    Scaled,
}

/// What the engine has done to each door model it has had to help, so that the log says what
/// happened to a model once rather than once per door of it.
#[derive(Resource, Debug, Default)]
struct AdjustedSwings(HashMap<String, SwingSource>);

/// What an animating door model turned out to be: the clips to play, the entity the loader put the
/// door's [`AnimationPlayer`] on, and the nodes the clips move.
///
/// It is built the same way whether the clips are the model's own or a twin's rebuilt onto its
/// nodes, which is what keeps the state machine, the leaf marking and the graph out of that
/// question.
#[derive(Debug)]
struct ResolvedModel {
    player: Entity,
    /// The `Open` clip.
    open: Handle<AnimationClip>,
    /// The `Close` clip, or `None` for a model that has none.
    close: Option<Handle<AnimationClip>>,
    /// The `Open` clip's length in seconds.
    open_seconds: f32,
    /// How far the `Open` clip turns the leaves, in degrees: the measure behind
    /// [`DoorAnimation::clears_doorway`].
    open_swing: f32,
    /// The `Close` clip's length in seconds, or `None` for a model that has no `Close` clip.
    close_seconds: Option<f32>,
    /// The animation targets the clips have curves for: the leaves.
    moved: HashSet<AnimationTargetId>,
    /// The nodes of [`moved`](Self::moved) that hang from no other node the clips move
    /// ([`hinged_leaves`]): the leaves turning on their own hinges, whose swing is what opens the
    /// doorway. A handle or lock turning on a leaf is not one of them.
    hinged: HashSet<AnimationTargetId>,
}

/// A twin's clips rebuilt onto the door's own nodes, with what replaces the twin's own measurement
/// of its swing - the rebuilt clips are what plays, so they are what says whether the doorway opens.
#[derive(Debug)]
struct BorrowedClips {
    open: AnimationClip,
    close: Option<AnimationClip>,
    /// The rebuilt `Open` clip's length in seconds.
    seconds: f32,
    /// How far the rebuilt `Open` clip turns the door's leaves, in degrees.
    swing: f32,
    /// The rebuilt `Close` clip's length in seconds, or `None` when the twin has no `Close` clip.
    close_seconds: Option<f32>,
    /// The door's own nodes the rebuilt clips move.
    moved: HashSet<AnimationTargetId>,
    /// The nodes of `moved` that hang from no other moved node ([`hinged_leaves`]).
    hinged: HashSet<AnimationTargetId>,
}

/// A load door reference that has not been given its animation yet: the door itself, and the model
/// path to look the clips up on.
type UnresolvedDoorQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static LoadDoor,
        Option<&'static MeshHandle>,
        Option<&'static DoorState>,
    ),
    (Without<DoorAnimation>, Without<PendingDoorModel>),
>;

/// A load door reference still waiting for its animation, with whether `streaming.rs` still holds
/// its scene back ([`PendingModel`]): a scene that has not been armed cannot have a player yet.
type AwaitingDoorQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static LoadDoor,
        &'static mut PendingDoorModel,
        Has<PendingModel>,
        Option<&'static MeshHandle>,
    ),
    Without<DoorAnimation>,
>;

/// Takes an already loaded optional twin root. Never called for the original source.
fn existing_twin_root(server: &AssetServer, path: &str) -> Option<Handle<Gltf>> {
    server.get_handle::<Gltf>(path.to_owned())
}

/// Logs a model file that the asset server loaded a second time.
///
/// Bevy reports a re-load of an asset that is already there as [`AssetEvent::Modified`] (its first
/// load is `Added`, `bevy_asset-0.19.0/src/assets.rs#L329-L341`), and a second load of a glTF
/// re-inserts every sub-asset of the file - the scene among them, which the spawner answers by
/// re-instancing every drawn copy of the model
/// (`bevy_world_serialization-0.19.0/src/world_asset_spawner.rs#L593`) and deleting the
/// `AnimationPlayer` a door was in the middle of using ([`forget_lost_door_players`]). Nothing in
/// the engine mutates `Assets<Gltf>`, so a `Modified` here is a double load and nothing else: this
/// is the line to count when checking that the modules wanting a model's clips stop asking for it a
/// second time.
fn report_reloaded_models(
    mut events: MessageReader<AssetEvent<Gltf>>,
    asset_server: Res<AssetServer>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } = event {
            info!(
                model = ?asset_server.get_path(*id),
                "model asset loaded a second time: every drawn copy of it is re-instanced"
            );
        }
    }
}

/// Lets a load door open with its own model's `Open`/`Close` animation.
///
/// Added by [`PortalPlugin`](crate::portal::PortalPlugin) for the runs that are looked at rather
/// than measured. It needs nothing but the asset server: a run that never writes [`OpenDoor`] never
/// changes a door, so a `--shots` run is unaffected.
pub struct SeamlessDoorAnimationPlugin;

impl Plugin for SeamlessDoorAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenDoor>()
            // Written by `crate::transition`'s crossing (and registered there too): this plugin is
            // the one that opens the door, so it is the one that reads the message - a run that adds
            // this plugin without the transition has no crossing and never sees one.
            .add_message::<OpenDestinationDoor>()
            .init_resource::<AdjustedSwings>()
            .init_resource::<DoubleSidedLeafMaterials>()
            .init_resource::<ArrivalOpenings>()
            .init_resource::<PendingDoorOpens>()
            .init_resource::<FarDoorDrive>()
            .add_systems(
                Update,
                (
                    forget_lost_door_players,
                    request_door_models,
                    attach_door_animations,
                    // Just after the clips are attached, and before any of the state the door is
                    // read through this frame: a door whose animation was lost is put back in the
                    // state it was in, at the pose that state holds.
                    restore_lost_swings,
                    // The leaves are marked by the system before, so they are drawn from behind
                    // from the frame their door is resolved in.
                    draw_leaves_double_sided,
                    // A door left open asks for its close with the message `E` writes, so this runs
                    // before `activate_doors`: the frame the door is found far enough away is the
                    // frame the leaf starts coming back, through the one state machine. After the
                    // crossing, like `open_arrival_doors`, so the distance is measured from where
                    // the camera ended up this frame.
                    auto_close_doors
                        .after(crate::transition::DoorTransition)
                        .before(crate::portal::PortalFrame),
                    activate_doors,
                    // After the crossing that asks for it, and before the portal draws anything:
                    // the far door of a mapped crossing has to be open in the frame the player
                    // arrives in its doorway, which is the frame the crossing is applied in - and
                    // the portal's own answer for that door, and the window it may open through it,
                    // are read off that state in the same frame.
                    open_arrival_doors
                        .after(crate::transition::DoorTransition)
                        .before(crate::portal::PortalFrame),
                    advance_door_states,
                    // After the states have moved on, so a near door that finished closing this
                    // frame is read as closed; before the leaves are drawn, so the far door's leaf
                    // is drawn for the state it was just put in. Before the portal, which reads the
                    // far door's state to decide whether its leaf is drawn in the window.
                    drive_far_doors.before(crate::portal::PortalFrame),
                    update_door_leaves,
                )
                    .chain()
                    .after(crate::model_animation::resolve_source_clips),
            )
            // Reads the asset server's own events rather than any state of this module's, so it
            // sits outside the chain: it reports a model loaded twice however the load was asked
            // for.
            .add_systems(Update, report_reloaded_models);
    }
}

/// Initializes seamless state. Authored clip readiness belongs exclusively to the public binding.
/// Auto-load markers have no moving leaf and start open; model-free references are static.
fn request_door_models(mut commands: Commands, doors: UnresolvedDoorQuery) {
    for (entity, door, model, state) in &doors {
        if door.auto_load {
            commands.entity(entity).try_insert((
                DoorState::Open { animated: false },
                DoorAnimation::default(),
            ));
            continue;
        }
        if state.is_none() {
            commands.entity(entity).try_insert(DoorState::Closed);
        }
        match model {
            Some(MeshHandle(path)) => {
                commands.entity(entity).try_insert(PendingDoorModel {
                    binding: None,
                    #[cfg(test)]
                    model: None,
                    path: path.clone(),
                    twin: None,
                    waiting: 0,
                });
            }
            // Nothing to animate: a load door whose base has no model is a static door.
            None => {
                commands.entity(entity).try_insert(DoorAnimation::default());
            }
        }
    }
}

/// Derives the seamless graph from the public scene inventory. An unresolved source has no
/// deadline and never becomes a static answer. Only the optional twin has a bounded wait.
#[allow(clippy::too_many_arguments)]
fn attach_door_animations(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    models: Res<Assets<Gltf>>,
    mut clips: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut adjustments: ResMut<AdjustedSwings>,
    mut doors: AwaitingDoorQuery,
    bindings: Query<&SourceClipBinding>,
    inventories: Query<&SourceClips>,
    invalid: Query<(), With<InvalidSourceClip>>,
    children: Query<&Children>,
    names: Query<&Name>,
    players: Query<(), With<AnimationPlayer>>,
    targets: Query<&AnimationTargetId>,
    config: Option<Res<EngineConfig>>,
) {
    for (door, door_row, mut pending, scene_queued, mesh) in &mut doors {
        if scene_queued || invalid.contains(door) {
            continue;
        }
        let Ok(binding) = bindings.get(door) else {
            continue;
        };
        let changed = pending.binding.as_ref().is_some_and(|old| {
            old.source != binding.source
                || old.graph != binding.graph
                || old.players != binding.players
        });
        let path_changed = mesh.is_some_and(|mesh| pending.path != mesh.0);
        if changed || path_changed {
            pending.twin = None;
            pending.waiting = 0;
        }
        if path_changed && let Some(mesh) = mesh {
            pending.path = mesh.0.clone();
        }
        if pending.binding.is_none() || changed {
            pending.binding = Some(binding.clone());
        }
        let Ok(inventory) = inventories.get(binding.source) else {
            continue;
        };
        let named = |wanted: &str| {
            inventory
                .0
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
                .map(|(_, clip)| clip.clone())
        };
        let (open, close) = (named(OPEN_CLIP), named(CLOSE_CLIP));
        let Some(open) = open else {
            // The completed public inventory names no Open: this is the authored static leaf
            // answer. A missing inventory or binding never reaches this branch.
            commands
                .entity(door)
                .try_insert((DoorAnimation::default(), DerivedSource(binding.clone())))
                .try_remove::<(PendingDoorModel, DoorAnimationGaveUp)>();
            continue;
        };
        let Some(resolved) = resolve_model(
            binding.source,
            &open,
            close.as_ref(),
            &clips,
            &children,
            &players,
            &targets,
        ) else {
            // Missing targets, clips or player remain unresolved, without a static fallback.
            continue;
        };
        if !binding.players.contains(&resolved.player) {
            continue;
        }

        // The door's own clip opens the doorway: most doors' do, and those never look for a twin.
        // One that does not is a load door in a hurry to be hidden by a loading screen, and its
        // non-load twin is where the real swing is.
        let borrowed = if resolved.open_swing >= DOORWAY_CLEAR_DEGREES {
            None
        } else {
            match twin_swing(
                &mut pending,
                resolved.player,
                &asset_server,
                &models,
                &clips,
                &children,
                &names,
                &targets,
                config.as_deref().map(|config| config.assets_dir.as_path()),
            ) {
                TwinSwing::Waiting => continue,
                TwinSwing::NoTwin => None,
                TwinSwing::Unusable(twin) => {
                    debug!(
                        door = format_args!("{:08X}", door_row.ref_id),
                        twin = %twin,
                        "no swing to borrow; keeping the door's own clip"
                    );
                    None
                }
                TwinSwing::Borrowed {
                    path,
                    clips: borrowed,
                } => {
                    if adjustments
                        .0
                        .insert(pending.path.clone(), SwingSource::Twin)
                        .is_none()
                    {
                        info!(
                            door = format_args!("{:08X}", door_row.ref_id),
                            model = %pending.path,
                            twin = %path,
                            degrees = borrowed.swing,
                            "load door borrows its non-load twin's swing"
                        );
                    }
                    Some(*borrowed)
                }
            }
        };

        // Whichever swing the door ended up with, it is the door's animation from here on: the same
        // state machine and the same leaf hiding, told a clip that opens the doorway.
        let resolved = match borrowed {
            Some(borrowed) => ResolvedModel {
                player: resolved.player,
                open: clips.add(borrowed.open),
                close: borrowed.close.map(|clip| clips.add(clip)),
                open_seconds: borrowed.seconds,
                open_swing: borrowed.swing,
                close_seconds: borrowed.close_seconds,
                moved: borrowed.moved,
                hinged: borrowed.hinged,
            },
            // No twin, or one that is no use to this door: the door's own clip is the swing it has,
            // and the leaves in it are already turning the right way about the right hinges.
            None => swung_open(
                resolved,
                &mut clips,
                &mut adjustments,
                door_row.ref_id,
                &pending.path,
            ),
        };
        commands
            .entity(door)
            .try_insert(DerivedSource(binding.clone()));
        attach_animation(
            &mut commands,
            &mut graphs,
            door,
            resolved,
            &children,
            &targets,
        );
    }
}

/// What came of asking a load door's non-load twin for its swing.
#[derive(Debug)]
enum TwinSwing {
    /// The twin, or one of its clips, is still on its way: try again next frame.
    Waiting,
    /// The model's name carries no load marker, so there is no twin to look for.
    NoTwin,
    /// There is one and it is no use: it never loaded, or its clips move nodes this door's model has
    /// not got. Its path is carried for the log.
    Unusable(String),
    /// The twin's `Open`/`Close` rebuilt onto the door's own nodes, and the twin's path for the log.
    ///
    /// Boxed because the clips are large and one of this for a door is not worth carrying in every
    /// `Waiting` of every door that never gets one.
    Borrowed {
        path: String,
        clips: Box<BorrowedClips>,
    },
}

/// The swing a door's non-load twin has for it, if it has one the door can use.
///
/// The twin is looked up once - the same model path with the load marker taken out of it, loaded
/// then and there - and from the next frame this is the question "has it arrived, and does it fit?".
/// [`TwinSwing::Waiting`] answers for as long as something is still on its way; a twin that never
/// arrives, and one whose clips move other nodes, are [`TwinSwing::Unusable`] and the door keeps its
/// own clip.
#[allow(clippy::too_many_arguments)]
fn twin_swing(
    pending: &mut PendingDoorModel,
    player: Entity,
    asset_server: &AssetServer,
    models: &Assets<Gltf>,
    clips: &Assets<AnimationClip>,
    children: &Query<&Children>,
    names: &Query<&Name>,
    targets: &Query<&AnimationTargetId>,
    assets_root: Option<&std::path::Path>,
) -> TwinSwing {
    let Some(twin) = pending.twin.clone() else {
        let path = match twin_model_on_disk(&pending.path, assets_root) {
            TwinLookup::NoMarker => return TwinSwing::NoTwin,
            TwinLookup::Missing(path) => {
                debug!(
                    model = %pending.path,
                    twin = %path,
                    "no twin: the derived non-load model was never converted"
                );
                return TwinSwing::NoTwin;
            }
            TwinLookup::Found(path) => path,
        };
        // The twin's file may be being loaded for references of it in the same cell load, and a
        // request landing inside that load is a second load of the twin's file
        // (the separately requested twin only): take the root if it is there, and ask for it only once nothing
        // else is loading it.
        if let Some(model) = existing_twin_root(asset_server, &path) {
            // The twin's own wait starts here, and is [`TWIN_WAIT_FRAMES`] from now on: the frames
            // the door's own model and scene took are not these.
            pending.waiting = 0;
            pending.twin = Some(TwinModel { model, path });
            return TwinSwing::Waiting;
        }
        if pending.waiting < TWIN_ROOT_WAIT_FRAMES {
            pending.waiting += 1;
            return TwinSwing::Waiting;
        }
        let model: Handle<Gltf> = asset_server.load(path.clone());
        pending.waiting = 0;
        pending.twin = Some(TwinModel { model, path });
        return TwinSwing::Waiting;
    };

    let Some(twin_model) = models.get(&twin.model) else {
        // A twin whose load failed is not going to arrive, and one that has outlasted
        // [`TWIN_WAIT_FRAMES`] is not either.
        if asset_server.load_state(&twin.model).is_failed() {
            return TwinSwing::Unusable(twin.path);
        }
        pending.waiting += 1;
        return if pending.waiting < TWIN_WAIT_FRAMES {
            TwinSwing::Waiting
        } else {
            TwinSwing::Unusable(twin.path)
        };
    };

    // The twin's clips are named sub-assets of its model and can arrive a frame after it, the way
    // the door's own did - and they are the whole reason for waiting for a twin at all.
    let (open, close) = door_clips(twin_model);
    let clips_are_here = open.as_ref().is_some_and(|open| clips.get(open).is_some())
        && close
            .as_ref()
            .is_none_or(|close| clips.get(close).is_some());
    if !clips_are_here {
        pending.waiting += 1;
        return if pending.waiting < TWIN_WAIT_FRAMES {
            TwinSwing::Waiting
        } else {
            TwinSwing::Unusable(twin.path)
        };
    }

    match open.and_then(|open| {
        borrow_swing(
            player,
            &open,
            close.as_ref(),
            clips,
            children,
            names,
            targets,
        )
    }) {
        Some(clips) => TwinSwing::Borrowed {
            path: twin.path,
            clips: Box::new(clips),
        },
        None => TwinSwing::Unusable(twin.path),
    }
}

/// Gives a door its animation: a graph of the clips it is to play, the leaves marked, and the
/// [`DoorAnimation`] the state machine reads from then on.
fn attach_animation(
    commands: &mut Commands,
    graphs: &mut Assets<AnimationGraph>,
    door: Entity,
    resolved: ResolvedModel,
    children: &Query<&Children>,
    targets: &Query<&AnimationTargetId>,
) {
    let mut clip_handles = vec![resolved.open.clone()];
    if let Some(close) = &resolved.close {
        clip_handles.push(close.clone());
    }
    let (graph, nodes) = AnimationGraph::from_clips(clip_handles);
    let graph = graphs.add(graph);
    let has_close = resolved.close.is_some();

    mark_leaf_nodes(commands, door, &resolved.moved, children, targets);

    commands.entity(resolved.player).try_insert((
        AnimationGraphHandle(graph.clone()),
        AnimationTransitions::new(),
    ));
    commands.entity(door).try_insert(DerivedGraph(graph));
    commands
        .entity(door)
        .try_insert(DoorAnimation {
            player: Some(resolved.player),
            open: Some(DoorClip {
                node: nodes[0],
                seconds: resolved.open_seconds,
            }),
            close: has_close.then(|| DoorClip {
                node: nodes[1],
                seconds: resolved.close_seconds.unwrap_or(0.0),
            }),
            clears_doorway: resolved.open_swing >= DOORWAY_CLEAR_DEGREES,
        })
        .try_remove::<(PendingDoorModel, DoorAnimationGaveUp)>();
}

/// What a door model's clips come to: their lengths, how far the `Open` one turns the leaves, the
/// entity the loader put the door's [`AnimationPlayer`] on, and the nodes they move. `None` while
/// any of that is still on its way - the clip sub-assets are loaded after the model, and the scene
/// the player lives in is spawned after that.
fn resolve_model(
    door: Entity,
    open: &Handle<AnimationClip>,
    close: Option<&Handle<AnimationClip>>,
    clips: &Assets<AnimationClip>,
    children: &Query<&Children>,
    players: &Query<(), With<AnimationPlayer>>,
    targets: &Query<&AnimationTargetId>,
) -> Option<ResolvedModel> {
    let open_clip = clips.get(open)?;
    let close_clip = match close {
        Some(close) => Some(clips.get(close)?),
        None => None,
    };
    // The loader puts the door's player on the animation root of the scene, which it spawns under
    // the door reference a frame or more after the model itself is loaded.
    let player = std::iter::once(door)
        .chain(children.iter_descendants(door))
        .find(|entity| players.contains(*entity))?;
    let moved = clip_targets(std::iter::once(open_clip).chain(close_clip));
    let hinged = hinged_leaves(player, &moved, children, targets);
    Some(ResolvedModel {
        player,
        open: open.clone(),
        close: close.cloned(),
        open_seconds: open_clip.duration(),
        open_swing: swing_degrees(open_clip, &hinged),
        close_seconds: close_clip.map(AnimationClip::duration),
        moved,
        hinged,
    })
}

/// The nodes of `moved` that hang from no other node in `moved`, in the scene under `player`: the
/// leaves that turn on hinges of their own.
///
/// A door's clips move its leaves and, now and then, something mounted on a leaf - a handle, a
/// lock, a ring (14 of the install's 156 animated door models: `RiftenRWDoorJailLoad01` turns its
/// `Door` 23 degrees and the `Handle` on it 69). Such a node turns with its leaf and then on its
/// own, so how far it turns is not how far the door opens: measured with the leaves it would call
/// that narrow door a doorway, and scaled with them it would spin. So the doorway is measured, and
/// the swing scaled, on these nodes only, and a node mounted on one keeps its own motion.
///
/// A node the clips name but the scene has not got counts as hinged: nothing says it hangs from
/// anything.
fn hinged_leaves(
    player: Entity,
    moved: &HashSet<AnimationTargetId>,
    children: &Query<&Children>,
    targets: &Query<&AnimationTargetId>,
) -> HashSet<AnimationTargetId> {
    let is_moved = |node: Entity| targets.get(node).ok().filter(|id| moved.contains(*id));
    let mut mounted = HashSet::new();
    for node in std::iter::once(player).chain(children.iter_descendants(player)) {
        if is_moved(node).is_none() {
            continue;
        }
        mounted.extend(
            children
                .iter_descendants(node)
                .filter_map(is_moved)
                .copied(),
        );
    }
    moved.difference(&mounted).copied().collect()
}

/// The non-load twin's swing, rebuilt onto the door's own nodes - or `None` when the twin is not
/// the same door: a clip that moves a node this model has not got would swing nothing, and one that
/// moves only some of them would swing half a door, which is worse than a door that keeps its own
/// clip.
///
/// The clips are rebuilt rather than borrowed as they are because an [`AnimationTargetId`] is a
/// hash of a node's **whole** name path from the scene root (`bevy_gltf-0.19.0/src/loader/mod.rs`
/// `#L559`, `#L1558`) and the two models name their roots differently: `DwemerLargeDoorLoad01`
/// against `DwemerLargeDoor01`. Every id in the twin's clip therefore names a node the door's scene
/// does not have, and the door's `Object02` is not the twin's `Object02` either. Rebuilding under
/// the door's own ids is what makes the twin's curves drive the door's leaves.
fn borrow_swing(
    player: Entity,
    twin_open: &Handle<AnimationClip>,
    twin_close: Option<&Handle<AnimationClip>>,
    clips: &Assets<AnimationClip>,
    children: &Query<&Children>,
    names: &Query<&Name>,
    targets: &Query<&AnimationTargetId>,
) -> Option<BorrowedClips> {
    let open = clips.get(twin_open)?;
    let close = match twin_close {
        Some(close) => Some(clips.get(close)?),
        None => None,
    };
    // Everything the twin's clips move, and how each of those nodes is named in the door's own
    // model. Empty means the twin animates nothing, which is no swing to borrow.
    let wanted: HashSet<AnimationTargetId> = std::iter::once(open)
        .chain(close)
        .flat_map(|clip| clip.curves().keys().copied())
        .collect();
    let mapping = twin_node_mapping(&scene_node_paths(player, children, names, targets), &wanted);
    if wanted.is_empty() || mapping.len() != wanted.len() {
        return None;
    }

    let open = rebuild_clip(open, &mapping);
    let close = close.map(|close| rebuild_clip(close, &mapping));
    let moved: HashSet<AnimationTargetId> = mapping.values().copied().collect();
    let hinged = hinged_leaves(player, &moved, children, targets);
    let seconds = open.duration();
    let close_seconds = close.as_ref().map(AnimationClip::duration);
    let swing = swing_degrees(&open, &hinged);
    Some(BorrowedClips {
        open,
        close,
        seconds,
        swing,
        close_seconds,
        moved,
        hinged,
    })
}

/// Where the twin's clips say each of the door's own nodes is: the twin's id for a node, against
/// the id the door's scene gives it.
///
/// A twin's clip can only be played on the door's nodes if both models call those nodes the same
/// thing, and the only way to ask "which of my nodes does this id name?" is to work out what the
/// paths of the twin's ids are. The two models are the same door - the load one keeping the leaves
/// still while the loading screen comes down - so the twin's path for a node is the door's path
/// with the model's own root name written the way the twin writes it
/// ([`twin_target_ids`]). An id that matches is proof the twin names that node: the id hashes the
/// whole path, so nothing but the same names can produce it.
fn twin_node_mapping(
    nodes: &[(AnimationTargetId, Vec<Name>)],
    wanted: &HashSet<AnimationTargetId>,
) -> HashMap<AnimationTargetId, AnimationTargetId> {
    let mut mapping = HashMap::new();
    for (door, path) in nodes {
        for twin in twin_target_ids(path) {
            if wanted.contains(&twin) {
                mapping.insert(twin, *door);
            }
        }
    }
    mapping
}

/// The name path from a door's animation root down to each node under it, with the node's own
/// [`AnimationTargetId`].
///
/// This is the path the loader hashed to make that id - the root's own name first
/// (`bevy_gltf-0.19.0/src/loader/mod.rs#L1545`) - rebuilt from the spawned scene, so that the same
/// path can be asked for under another model's root node name.
fn scene_node_paths(
    root: Entity,
    children: &Query<&Children>,
    names: &Query<&Name>,
    targets: &Query<&AnimationTargetId>,
) -> Vec<(AnimationTargetId, Vec<Name>)> {
    let mut nodes = Vec::new();
    let mut stack = vec![(root, Vec::new())];
    while let Some((entity, mut path)) = stack.pop() {
        let Ok(name) = names.get(entity) else {
            // The loader names every node it spawns; one that is not named is not a path the twin
            // could be asking about either.
            continue;
        };
        path.push(name.clone());
        if let Ok(target) = targets.get(entity) {
            nodes.push((*target, path.clone()));
        }
        if let Ok(kids) = children.get(entity) {
            stack.extend(kids.iter().map(|kid| (kid, path.clone())));
        }
    }
    nodes
}

/// The ids a twin's clips would use for a node the door's own model has at `path`.
///
/// The twin is the same model under a root name carrying no marker: for the demo route's doors,
/// `DwemerLargeDoorLoad01` here is `DwemerLargeDoor01` there. Taking the marker out of a
/// marker-carrying element of the path - the model's root node is named after the file the marker
/// was taken out of - turns the door's id into the twin's. Which element that is comes from the
/// path itself rather than from a rule about where a converter puts a model's root, and a path with
/// no marker in it yields nothing: a model whose names have nothing to do with its twin's is left
/// alone.
fn twin_target_ids(path: &[Name]) -> Vec<AnimationTargetId> {
    let mut ids = Vec::new();
    for index in 0..path.len() {
        let Some(stripped) = strip_load_marker(path[index].as_str()) else {
            continue;
        };
        let mut twin_path = path.to_vec();
        twin_path[index] = Name::new(stripped);
        ids.push(AnimationTargetId::from_names(twin_path.iter()));
    }
    ids
}

/// One of the twin's clips with every curve it has for a node of the door's model moved to that
/// node's own id, and the curves for anything else dropped - there are none, `mapping` covers the
/// clip exactly.
fn rebuild_clip(
    twin: &AnimationClip,
    mapping: &HashMap<AnimationTargetId, AnimationTargetId>,
) -> AnimationClip {
    let mut rebuilt = AnimationClip::default();
    for (target, curves) in twin.curves() {
        let Some(door) = mapping.get(target) else {
            continue;
        };
        for curve in curves {
            rebuilt.add_variable_curve_to_target(*door, curve.clone());
        }
    }
    // The same length as the clip it came from, whatever its curves say: this is the denominator of
    // `OPEN_FRACTION` and the time `Close` has to reach.
    rebuilt.set_duration(twin.duration());
    rebuilt
}

/// Gives a door whose own `Open` clip does not clear the doorway a swing of its own, by turning
/// that clip further than it was baked to turn.
///
/// This is the answer for the load doors no twin can help - which is very nearly all of them: a load
/// door's clip is short because the game only had to move the leaf before the loading screen covered
/// it, and the two models of a load/twin pair name their leaves differently as often as not. The
/// door's own clip already has everything a swing needs except the distance - the leaves, the hinge
/// each one turns about, the direction and the timing - and [`swung_open_clips`] stretches it until
/// each of its leaves stands at [`SWUNG_OPEN_DEGREES`] ([`swing_factors`]).
///
/// The door comes back unchanged when there is nothing to scale (a clip that turns nothing, which
/// has no rotation to stretch, and one that already clears the doorway, which is never scaled down)
/// or when its clips cannot be rebuilt; its leaves are then hidden when it opens, as before.
fn swung_open(
    resolved: ResolvedModel,
    clips: &mut Assets<AnimationClip>,
    adjustments: &mut AdjustedSwings,
    ref_id: u32,
    model_path: &str,
) -> ResolvedModel {
    let Some(factors) = clips
        .get(&resolved.open)
        .map(|open| swing_factors(open, &resolved.hinged))
    else {
        return resolved;
    };
    let factor = factors.values().copied().fold(1.0_f32, f32::max);
    if factor <= 1.0 {
        return resolved;
    }
    let Some((open, close)) = swung_open_clips(
        clips.get(&resolved.open),
        resolved.close.as_ref().and_then(|close| clips.get(close)),
        &resolved.moved,
        &factors,
    ) else {
        debug!(
            door = format_args!("{ref_id:08X}"),
            model = %model_path,
            "the door's own clip could not be scaled; keeping it as it is"
        );
        return resolved;
    };

    let swing = swing_degrees(&open, &resolved.hinged);
    let open_seconds = open.duration();
    let close_seconds = close.as_ref().map(AnimationClip::duration);
    if adjustments
        .0
        .insert(model_path.to_owned(), SwingSource::Scaled)
        .is_none()
    {
        info!(
            door = format_args!("{ref_id:08X}"),
            model = %model_path,
            from = resolved.open_swing,
            to = swing,
            factor,
            "load door's own swing scaled up to open the doorway"
        );
    }
    ResolvedModel {
        player: resolved.player,
        open: clips.add(open),
        close: close.map(|clip| clips.add(clip)),
        open_seconds,
        open_swing: swing,
        close_seconds,
        moved: resolved.moved,
        hinged: resolved.hinged,
    }
}

/// How much each node of a door's own `Open` clip is turned further by ([`swung_open_clips`]),
/// keyed by the node's [`AnimationTargetId`]. A node that is not in it keeps its own motion.
type SwingFactors = HashMap<AnimationTargetId, f32>;

/// How far each hinged leaf of a door's own `Open` clip is scaled: every leaf by its own
/// [`swing_scale`], so that each one stands at [`SWUNG_OPEN_DEGREES`].
///
/// One factor per leaf rather than one for the whole clip, because a door's two leaves often turn
/// different distances and a double door is only open when both are out of the way: the Honningbrew
/// Meadery's `WRDragonDoor01` turns its leaves 9.4 and 17.0 degrees, and one factor from the wider
/// leaf stood the other at 50 degrees, half across the doorway (the user's capture of 2026-09-25,
/// "this type of door doesn't open the whole way"). 54 of the 90 door models in the install with
/// two or more hinged leaves turn them by different amounts.
///
/// **The leaves of one door open together.** [`MAX_SWING_SCALE`] caps how far one leaf's own
/// twitch is blown up, but a double door's leaves are one door: the leaf the cap lets reach the
/// furthest says how far this model's door opens ([`double_door_target`]), and every other leaf is
/// scaled to stand there too, past the cap if it has to. Whiterun's main gate, `WRDoorMainGate01`,
/// turns its leaves 5.0 and 2.2 degrees: capped leaf by leaf, the second stood at 44 degrees,
/// square across half the gateway (the user's capture of 2026-09-26, "the door doesn't open all the
/// way"). A single leaf is still capped: nothing else in the model says how far it opens.
///
/// Empty - nothing scaled - when the clip already opens the doorway: its widest leaf clears it, and
/// a door whose own swing is a doorway is never touched. Nodes mounted on a leaf are not in
/// `hinged` ([`hinged_leaves`]) and so are never scaled: they turn with their leaf already.
fn swing_factors(clip: &AnimationClip, hinged: &HashSet<AnimationTargetId>) -> SwingFactors {
    if swing_degrees(clip, hinged) >= DOORWAY_CLEAR_DEGREES {
        return SwingFactors::new();
    }
    let swings: Vec<(AnimationTargetId, f32)> = hinged
        .iter()
        .filter_map(|target| Some((*target, node_swing_degrees(clip, *target)?)))
        .collect();
    let target = double_door_target(swings.iter().map(|(_, degrees)| *degrees));
    swings
        .into_iter()
        .map(|(leaf, degrees)| {
            let factor = match target {
                // A leaf that turns at all is scaled to where its door's widest-reaching leaf
                // stands; `swing_scale` keeps what it says for a leaf with nothing to scale.
                Some(target) if swing_scale(degrees) > 1.0 => target / degrees,
                _ => swing_scale(degrees),
            };
            (leaf, factor)
        })
        .collect()
}

/// Where every leaf of a door with two or more turning leaves is stood by [`swing_factors`]: as far
/// as the cap lets the widest-turning leaf go, which is [`SWUNG_OPEN_DEGREES`] for any door whose
/// widest leaf turns at least a twentieth of that. `None` for a door with fewer than two leaves
/// that turn at all - a single leaf keeps its own capped [`swing_scale`].
fn double_door_target(swings: impl Iterator<Item = f32>) -> Option<f32> {
    let reached: Vec<f32> = swings
        .filter(|degrees| swing_scale(*degrees) > 1.0)
        .map(|degrees| degrees * swing_scale(degrees))
        .collect();
    (reached.len() >= 2).then(|| reached.into_iter().fold(0.0_f32, f32::max))
}

/// How much a door leaf's own swing is scaled by, given how far its `Open` clip turns it.
///
/// A factor of 1 means "leave it alone", and two things get one: a swing that already opens the
/// doorway - scaling a door's swing *down* would be as wrong as scaling a narrow one up - and a
/// swing of nothing at all, which has no rotation to scale. The rest are scaled to
/// [`SWUNG_OPEN_DEGREES`], and capped at [`MAX_SWING_SCALE`].
fn swing_scale(swing_degrees: f32) -> f32 {
    if swing_degrees.is_nan() || swing_degrees <= 0.0 || swing_degrees >= DOORWAY_CLEAR_DEGREES {
        return 1.0;
    }
    (SWUNG_OPEN_DEGREES / swing_degrees).min(MAX_SWING_SCALE)
}

/// A door's own `Open` and `Close` with their swing scaled up until the doorway opens.
///
/// The nodes, the hinge each leaf turns about and the way it turns are all in the clip already; what
/// a load door's clip does not have is the distance, because the game only had to move the leaf
/// before the loading screen covered it. So each leaf's rotation curve is turned its own factor
/// ([`swing_factors`]) times as far from the pose the clip starts on, about the same axis: `t = 0`
/// is the pose the door stands in and does not move, the clip keeps its own length, and its last key
/// stands each leaf at about [`SWUNG_OPEN_DEGREES`]. A node with no factor - a handle mounted on a
/// leaf - keeps its own rotation, and nothing else in the clip is touched: a door that slides
/// something while it swings keeps sliding it exactly as far.
///
/// `Close` is always the scaled `Open` **run backwards**: closing is the door retracing the swing it
/// just made. A `Close` of the door's own cannot be scaled into a close (see the body), so a node
/// only the `Close` moves - a latch, if a model has one - keeps the pose the `Open` left it in.
///
/// `None` when there is no clip to scale or a curve cannot be rebuilt (times that are not a curve,
/// keys that are not finite), which leaves the door with the clip it has.
fn swung_open_clips(
    open: Option<&AnimationClip>,
    close: Option<&AnimationClip>,
    moved: &HashSet<AnimationTargetId>,
    factors: &SwingFactors,
) -> Option<(AnimationClip, Option<AnimationClip>)> {
    let open = open?;
    let swung = rebuilt_clip(open, Resample::SwungOpen(factors))?;
    // A scaled door closes by retracing its scaled swing, whatever its own `Close` clip is. A
    // `Close` that mirrors the `Open` is that already; one that is a motion of its own - a few
    // degrees from a pose near the open one to a pose near the rest pose - cannot be scaled into a
    // close at all: turned about its first pose it ends forty times its few degrees from where it
    // started, and turned about the rest pose any degree it misses the rest pose by is multiplied
    // too. Dragonsreach's great doors (`WRCastleDoor01`, scaled 40 times) "closed" to a leaf
    // standing open across the doorway with no window behind it (the user's capture of
    // 2026-09-26 17:26, note 02).
    let close = match close {
        Some(close) => {
            if !close_mirrors_open(open, close, moved) {
                debug!(
                    "a scaled door's own close is a motion of its own; it retraces its scaled open instead"
                );
            }
            Some(rebuilt_clip(&swung, Resample::Reversed)?)
        }
        None => None,
    };
    Some((swung, close))
}

/// Whether a door's `Close` clip is its `Open` clip run backwards: where the `Open` clip ends is
/// where `Close` starts, and where `Open` starts is where `Close` ends, for every node the swing
/// turns to within [`CLOSE_MIRROR_DEGREES`].
///
/// This is what a door's two controller sequences are in the install - `Close` plays the `Open`
/// sequence's keys from the other end - and it is the difference between a door that closes by
/// retracing its swing and one whose `Close` is a second motion in its own right.
fn close_mirrors_open(
    open: &AnimationClip,
    close: &AnimationClip,
    moved: &HashSet<AnimationTargetId>,
) -> bool {
    // A `Close` that moves a node the `Open` does not is a motion of its own, whatever its poses
    // are: reversing the `Open` would leave that node standing.
    if close
        .curves()
        .keys()
        .any(|target| !open.curves().contains_key(target))
    {
        return false;
    }
    let rotation = animated_field!(Transform::rotation);
    let mut compared = 0;
    for target in moved {
        let Some(rest) = open.sample_clamped(rotation.clone(), *target, 0.0) else {
            continue;
        };
        let Some(swung) = open.sample_clamped(rotation.clone(), *target, open.duration()) else {
            continue;
        };
        let (Some(closed), Some(reopened)) = (
            close.sample_clamped(rotation.clone(), *target, 0.0),
            close.sample_clamped(rotation.clone(), *target, close.duration()),
        ) else {
            // The `Close` clip does not cover a node the swing turns: not the same motion.
            return false;
        };
        let within = |a: Quat, b: Quat| a.angle_between(b).to_degrees() <= CLOSE_MIRROR_DEGREES;
        if !within(closed, swung) || !within(reopened, rest) {
            return false;
        }
        compared += 1;
    }
    compared > 0
}

/// What a clip is being rebuilt for ([`rebuilt_clip`]).
#[derive(Debug, Clone, Copy)]
enum Resample<'a> {
    /// Each node's rotation turned its own factor times as far from the pose the clip starts on
    /// ([`swung_open_clips`]); a node with no factor keeps its rotation.
    SwungOpen(&'a SwingFactors),
    /// Every sample taken the same distance from the *other* end of the clip: the clip run
    /// backwards, which is how a door's scaled `Close` is made from its scaled `Open`.
    Reversed,
}

impl Resample<'_> {
    /// The time of the original clip a rebuilt sample at `time` comes from.
    fn source(self, time: f32, duration: f32) -> f32 {
        match self {
            Resample::SwungOpen(_) => time,
            Resample::Reversed => duration - time,
        }
    }

    /// The pose a rebuilt clip holds for `target` where the original reaches `pose`, given the pose
    /// the original starts on.
    ///
    /// The swing is a rotation about one axis, so turning it further is turning the same rotation
    /// further: the rotation from the rest pose to the pose, multiplied by the node's factor about
    /// the same axis. `Quat::slerp` from the identity does exactly that, and is what keeps a leaf's
    /// hinge and direction exactly as the clip had them. A pose that *is* the rest pose stays there
    /// whatever the factor is, which is what makes `t = 0` the one sample a scaled clip shares with
    /// the clip it came from.
    fn rotation(self, target: AnimationTargetId, rest: Quat, pose: Quat) -> Quat {
        match self {
            Resample::SwungOpen(factors) => match factors.get(&target) {
                Some(&factor) => rest * Quat::slerp(Quat::IDENTITY, rest.inverse() * pose, factor),
                None => pose,
            },
            Resample::Reversed => pose,
        }
    }
}

/// One of a door's own clips rebuilt sample by sample ([`Resample`]): every rotation, translation
/// and scale curve it has, at [`sample_times`] from its start to its end. Anything else a clip
/// carries is copied over untouched.
///
/// Sampling rather than reading the clip's own keys is what lets a curve be turned about the pose it
/// starts on and run backwards, and the times are the same for every curve, so the rebuilt clip
/// keeps the shape of the motion. `None` when a rotation curve cannot be sampled, or when the
/// samples are not a curve at all (times that do not increase, values that are not finite) - the
/// caller's cue to leave the door with the clip it has rather than half a swing.
fn rebuilt_clip(clip: &AnimationClip, resample: Resample) -> Option<AnimationClip> {
    let duration = clip.duration();
    let times = sample_times(duration);
    let rotation = animated_field!(Transform::rotation);
    let translation = animated_field!(Transform::translation);
    let scale = animated_field!(Transform::scale);
    let rotation_id = rotation.evaluator_id();
    let translation_id = translation.evaluator_id();
    let scale_id = scale.evaluator_id();

    let mut rebuilt = AnimationClip::default();
    for (target, curves) in clip.curves() {
        for curve in curves {
            let field = curve.0.evaluator_id();
            if field == rotation_id {
                let rest = clip.sample_clamped(rotation.clone(), *target, 0.0)?;
                let mut samples = resampled(clip, rotation.clone(), *target, &times, resample)?;
                for (_, pose) in &mut samples {
                    *pose = resample.rotation(*target, rest, *pose);
                }
                let samples = AnimatableKeyframeCurve::new(samples).ok()?;
                rebuilt
                    .add_curve_to_target(*target, AnimatableCurve::new(rotation.clone(), samples));
            } else if field == translation_id {
                let samples = resampled(clip, translation.clone(), *target, &times, resample)?;
                let samples = AnimatableKeyframeCurve::new(samples).ok()?;
                rebuilt.add_curve_to_target(
                    *target,
                    AnimatableCurve::new(translation.clone(), samples),
                );
            } else if field == scale_id {
                let samples = resampled(clip, scale.clone(), *target, &times, resample)?;
                let samples = AnimatableKeyframeCurve::new(samples).ok()?;
                rebuilt.add_curve_to_target(*target, AnimatableCurve::new(scale.clone(), samples));
            } else {
                // Not a field this knows how to rebuild: a clip's other curves are not the swing,
                // so they are carried over as they are.
                rebuilt.add_variable_curve_to_target(*target, curve.clone());
            }
        }
    }
    rebuilt.set_duration(duration);
    Some(rebuilt)
}

/// One animatable field of one node, sampled at the rebuilt clip's times, from wherever
/// [`Resample`] says to read the original.
fn resampled<A: Animatable>(
    clip: &AnimationClip,
    property: impl AnimatableProperty<Property = A> + Clone,
    target: AnimationTargetId,
    times: &[f32],
    resample: Resample,
) -> Option<Vec<(f32, A)>> {
    let duration = clip.duration();
    let mut samples = Vec::with_capacity(times.len());
    for time in times {
        let value =
            clip.sample_clamped(property.clone(), target, resample.source(*time, duration))?;
        samples.push((*time, value));
    }
    Some(samples)
}

/// The times a rebuilt clip is sampled at, from its start to its end, both included: sixty to the
/// second of the clip's own length, never fewer than [`MIN_SWING_SAMPLES`] nor more than
/// [`MAX_SWING_SAMPLES`].
///
/// The door's clip is a baked sequence of linear keys at a handful of times, and a door swing is a
/// short smooth motion, so a sample a frame or so apart keeps the shape of it. Both ends are
/// included because both are load-bearing: the first sample is the pose the door stands in (and is
/// the one a scaled clip leaves exactly where it was), and the last is the pose a scaled clip opens
/// the door to.
fn sample_times(duration: f32) -> Vec<f32> {
    let wanted = (duration * SWING_SAMPLES_PER_SECOND).ceil();
    let count = if wanted.is_finite() {
        (wanted as u32).clamp(MIN_SWING_SAMPLES, MAX_SWING_SAMPLES)
    } else {
        MIN_SWING_SAMPLES
    };
    let last = (count - 1) as f32;
    (0..count)
        .map(|sample| duration * sample as f32 / last)
        .collect()
}

/// How far a door's `Open` clip turns its leaves, in degrees: the widest angle any node the clip
/// moves sweeps from the clip's first key to its last.
///
/// The clip knows the pose it starts from and the pose it ends on, and those two are what decide
/// whether the doorway ends up open: a leaf that ends 8 degrees from where it started has moved
/// about as far as its own thickness, and the doorway behind it is still a wall. Read here rather
/// than watched at runtime, because the answer is a fact about the clip - and because a door's
/// leaves must not be hidden partway through a swing that is going to clear the opening.
///
/// A clip with no rotation curves at all - a door that slides, or one that only moves something
/// that is not the leaf - turns nothing, so it does not clear.
fn swing_degrees(clip: &AnimationClip, moved: &HashSet<AnimationTargetId>) -> f32 {
    moved
        .iter()
        .filter_map(|target| node_swing_degrees(clip, *target))
        .fold(0.0_f32, f32::max)
}

/// How far `clip` turns one node from its first key to its last, in degrees, or `None` when the
/// clip has no rotation for it or the angle is not a number.
fn node_swing_degrees(clip: &AnimationClip, target: AnimationTargetId) -> Option<f32> {
    let rotation =
        |time: f32| clip.sample_clamped(animated_field!(Transform::rotation), target, time);
    let degrees = rotation(0.0)?
        .angle_between(rotation(clip.duration())?)
        .to_degrees();
    degrees.is_finite().then_some(degrees)
}

/// Where a door whose `AnimationPlayer` was lost is to be put back: the state it was in, restored by
/// [`restore_lost_swings`] once its animation is attached again.
///
/// The pose itself is not recorded, because the state says it: a door that was [`DoorState::Open`]
/// goes back to the `Open` clip's last key - the pose its swing ended on and the one its doorway is
/// being used at - and a door that was [`DoorState::Closing`] goes back to the rest pose it was
/// heading for, which is where its leaf already is. The two swings in flight are put at the end of
/// the swing they were making, which is the direction they were going and the safe one: a doorway
/// in use stays open, a door being shut shuts. What is lost is only how far into that swing the door
/// was - at most the last fraction of a clip under a second long - which is a pose the leaf jumps to
/// once, in a frame that is already a scene being re-instanced.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct ReattachPose(DoorState);

/// Drops a door's animation when the `AnimationPlayer` it recorded no longer exists, so that
/// [`request_door_models`] and [`attach_door_animations`] give the door its clips again on the model
/// that is there now.
///
/// The door's scene can be instanced again under the same door entity after its animation was
/// attached, and the player recorded then is gone. [`activate_doors`] could not play the `Open` clip
/// on it and fell back to `Open { animated: false }`, which hides the whole door: every animated
/// door in play vanished instead of swinging (the user, 2026-09-24; the recorded player was missing
/// at every activation of the Riverwood tour).
///
/// A door that was not closed is given a [`ReattachPose`] on the way out, so that the clips
/// [`attach_door_animations`] attaches next frame are played at the pose the door was in rather than
/// at the rest pose. Without it the door comes back closed - the re-instanced leaf stands at its
/// rest pose, and `DoorState::Closed` is what the re-claim writes over whatever the door was - which
/// is a door that lies about itself: it is open, its doorway is the one the player is walking
/// through, and its leaf is drawn across it ([`restore_lost_swings`]).
fn forget_lost_door_players(
    mut commands: Commands,
    doors: Query<(
        Entity,
        &LoadDoor,
        &DoorAnimation,
        &DoorState,
        Option<&DerivedSource>,
        Option<&SourceClipBinding>,
    )>,
    inventories: Query<&SourceClips>,
    derived_graphs: Query<&DerivedGraph>,
    graph_handles: Query<&AnimationGraphHandle>,
    players: Query<(), With<AnimationPlayer>>,
    everything: Query<(Option<&Name>, Option<&ChildOf>)>,
    children: Query<&Children>,
) {
    for (entity, door, animation, state, derived, binding) in &doors {
        let revision_valid = derived.is_none_or(|old| {
            binding.is_some_and(|new| {
                old.0.source == new.source
                    && old.0.graph == new.graph
                    && old.0.players == new.players
                    && inventories.contains(new.source)
            })
        });
        let player = animation.player;
        let graph_valid = derived_graphs.get(entity).map_or(true, |expected| {
            player.is_some_and(|p| {
                graph_handles
                    .get(p)
                    .is_ok_and(|actual| actual.0 == expected.0)
            })
        });
        if revision_valid && graph_valid && player.is_none_or(|p| players.contains(p)) {
            continue;
        }
        // The state to put back, or `None` for a door whose rest pose is the pose it was in, which
        // has nothing to restore: the door the re-claim closes is closed already.
        let restore = match *state {
            DoorState::Closed => None,
            // An open doorway, however it opened: the swing is put at the end of the clip, so the
            // leaf is out of the opening again and the doorway is the one it was.
            DoorState::Opening | DoorState::Open { .. } => Some(DoorState::Open { animated: true }),
            // A door on its way back: the leaf is already at the rest pose the close was heading
            // for, so the door is shut and stays shut rather than swinging open again.
            DoorState::Closing => Some(DoorState::Closed),
        };
        let lost = player.and_then(|p| everything.get(p).ok());
        let now: Vec<Entity> = children
            .iter_descendants(entity)
            .filter(|node| players.contains(*node))
            .collect();
        info!(
            door = format_args!("{:08X}", door.ref_id),
            ?player,
            ?state,
            restore = ?restore,
            lost_exists = lost.is_some(),
            lost_name = ?lost.and_then(|(name, _)| name.map(|name| name.as_str().to_owned())),
            lost_parent = ?lost.and_then(|(_, parent)| parent.map(ChildOf::parent)),
            ?now,
            "door: its animation player was replaced; attaching the animation again"
        );
        if let Some(restore) = restore {
            commands.entity(entity).try_insert(ReattachPose(restore));
        }
        if let Some(player) = player
            && let Ok(graph) = derived_graphs.get(entity)
        {
            let graph = graph.0.clone();
            // A source can change without replacing its scene/player. Withdraw our playback
            // only while this player still uses our graph; the public owner or another system
            // may already have installed a replacement. Check when the command applies too.
            // This runs before attachment in the chain, so a newly derived graph wins afterward.
            commands.queue(move |world: &mut World| {
                let Ok(mut entity) = world.get_entity_mut(player) else {
                    return;
                };
                if !entity
                    .get::<AnimationGraphHandle>()
                    .is_some_and(|actual| actual.0 == graph)
                {
                    return;
                }
                if let Some(mut player) = entity.get_mut::<AnimationPlayer>() {
                    player.stop_all();
                }
                entity.remove::<(AnimationGraphHandle, AnimationTransitions)>();
            });
        }
        // The marker describes the animation being withdrawn, so it goes with it: the door that
        // resolves again is judged on that resolution.
        commands.entity(entity).try_remove::<(
            DoorAnimation,
            DoorAnimationGaveUp,
            DerivedSource,
            DerivedGraph,
        )>();
    }
}

/// Puts a door whose animation was lost ([`forget_lost_door_players`]) back where it was: the state
/// it was in, and - for a door that was open - the `Open` clip started at its last key, which is the
/// pose its swing ended on.
///
/// The clips arrive a frame or two after the loss, with the same resolution every other door goes
/// through ([`attach_door_animations`]), so this waits for [`DoorAnimation`] to be there: until then
/// there is nothing to play. The seek is [`open_arrival_door`]'s - the clip is *started at* the pose
/// rather than played from the rest pose, so the leaf never swings through a doorway the player is
/// using - and the state is the one the loss recorded, which is what makes the doorway usable and
/// the leaf out of it in the frame the animation comes back.
///
/// A door whose model turns out to have no clip to play (the resolution gives up on it as a static
/// one) is left as `Open { animated: false }` if it was open: the hole `run_activation` leaves for a
/// door with nothing to animate.
fn restore_lost_swings(
    mut commands: Commands,
    mut doors: Query<(
        Entity,
        &ReattachPose,
        Option<&DoorAnimation>,
        &mut DoorState,
    )>,
    mut players: Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
) {
    for (door, pose, animation, mut state) in &mut doors {
        let Some(animation) = animation else {
            continue;
        };
        *state = match pose.0 {
            // A door whose close was interrupted: the leaf is at its rest pose already, and the
            // close it was making is over.
            DoorState::Closed => DoorState::Closed,
            // The swing out of the doorway, carried on to its end.
            DoorState::Open { .. } => {
                let played = animation.open.is_some_and(|open| {
                    play_clip(&mut players, *animation, animation.open, open.seconds)
                });
                DoorState::Open { animated: played }
            }
            // Not a pose this module records: the loss puts every other state at one of the two
            // above.
            DoorState::Opening | DoorState::Closing => pose.0,
        };
        commands.entity(door).try_remove::<ReattachPose>();
    }
}

// ---------------------------------------------------------------------------------------------
// A door left open closes itself
// ---------------------------------------------------------------------------------------------

/// Closes a load door the player opened and walked away from ([`AUTO_CLOSE_DISTANCE`] away).
///
/// The close is the one `E` runs: this system writes [`OpenDoor`] - the message the key writes, for
/// both directions - and [`activate_doors`], which is chained after it and so reads the message in
/// the frame it is written, runs [`run_activation`] on the door. There is no second close path: the
/// `Close` clip, the pose the reversal starts from, the cross-fade and the state the door lands in
/// are whatever `E` at the same door would have done.
///
/// A door is left alone when `E` itself would have nothing to ask it
/// ([`crate::player::door_action`]): it is not standing open, it is an auto-load marker, its model
/// has an `Open` clip and no `Close` one, its far side is not streamed in, or the player is
/// standing in its doorway. The distance is the one thing `E` does not know about.
///
/// This is also what unpins a door's destination: [`crate::transition::plan_door_prestream`] holds
/// the far side of every open and closing door wherever the camera is, so the doorway never
/// empties while the leaf is on its way home, and the cell is dropped once the state reaches
/// [`DoorState::Closed`] - which is this system having closed the door and no longer holding it
/// open.
#[allow(clippy::too_many_arguments)]
fn auto_close_doors(
    camera: Query<Entity, With<StreamingCamera>>,
    transforms: TransformHelper,
    doors: DoorAutoCloseQuery,
    streaming: Option<Res<StreamingWorld>>,
    drive: Res<FarDoorDrive>,
    door_layer: Option<Res<crate::door_layer::DoorLayer>>,
    mut open: MessageWriter<OpenDoor>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    // DoorTransition may have moved the camera and exterior ancestors this Update.
    // GlobalTransform propagation has not run yet: every measurement must use
    // the current local hierarchy, including a camera that itself has a parent.
    let camera_pose = match transforms.compute_global_transform(camera) {
        Ok(pose) => pose,
        Err(error) => {
            debug!(?camera, %error, "door auto-close skipped: current camera pose unavailable");
            return;
        }
    };
    let eye = camera_pose.translation();
    let feet = feet_from_eye(eye);
    let streaming = streaming.as_deref();
    let door_layer = door_layer.as_deref();
    for (
        entity,
        _global,
        local,
        door,
        state,
        anchor,
        animation,
        instance_bounds,
        expected_bounds,
    ) in &doors
    {
        // Only a door standing open. `Opening` and `Closing` are swings in flight, and
        // `run_activation` tells a door mid-swing nothing anyway; a door still opening on its way
        // to `Open` is caught on the frame it gets there.
        if !matches!(state, DoorState::Open { .. }) {
            continue;
        }
        // An auto-load door is born `Open` too, but it is an invisible marker with no leaf: the
        // player is crossed by walking into it (`crate::player::player_auto_doors`), so there is
        // nothing to swing and closing it would take the doorway away for good.
        if door.auto_load {
            continue;
        }
        // The far door of a doorway the player opened from the other side: it is open because its
        // near door is ([`drive_far_doors`]), and it closes with that door rather than on its own.
        // Its distance from the eye is a distance across two spaces, which says nothing.
        if drive.holds_open(entity) || door_layer.is_some_and(|layer| layer.is_follower(entity)) {
            continue;
        }
        // The two ends of a doorway are one door to the player: a door-layer pair
        // (`crate::door_layer`), or a pair the far-door drive swings together. The door is near,
        // and the player may be in its doorway, if either end is: after a crossing the player
        // stands at the other end - and in the frame of the crossing itself the camera has already
        // moved there while this door is still read at its old place - so measuring this end alone
        // closed the doorway the player had just walked through (the tours' "far door not open
        // after the player arrived", state `Closing`).
        let partner = door_layer
            .and_then(|layer| layer.pair_of(entity))
            .and_then(|pair| {
                if pair.driver == entity {
                    pair.follower
                } else {
                    Some(pair.driver)
                }
            })
            .or_else(|| drive.partner_of(entity));
        let global = match transforms.compute_global_transform(entity) {
            Ok(pose) => pose,
            Err(error) => {
                debug!(?entity, door = format_args!("{:08X}", door.ref_id), %error,
                    "door auto-close skipped: current source pose unavailable");
                continue;
            }
        };
        let mut ends = vec![(global, local, instance_bounds, expected_bounds)];
        if let Some(partner) = partner {
            let Ok((_, _, local, _, _, _, _, instance_bounds, expected_bounds)) =
                doors.get(partner)
            else {
                debug!(
                    ?entity,
                    ?partner,
                    door = format_args!("{:08X}", door.ref_id),
                    "door auto-close skipped: known partner facts unavailable"
                );
                continue;
            };
            let global = match transforms.compute_global_transform(partner) {
                Ok(pose) => pose,
                Err(error) => {
                    debug!(?entity, ?partner, door = format_args!("{:08X}", door.ref_id), %error,
                        "door auto-close skipped: current partner pose unavailable");
                    continue;
                }
            };
            ends.push((global, local, instance_bounds, expected_bounds));
        }
        if ends.iter().any(|(global, ..)| {
            global.translation().distance_squared(eye) <= AUTO_CLOSE_DISTANCE.powi(2)
        }) {
            continue;
        }
        // The refusal `E` makes at a model with an `Open` clip and no `Close` one: `run_activation`
        // would replay the opening rather than close anything (design section 5), so this is not a
        // door the engine can shut - asking would play the swing again at a distance instead.
        if !animation.is_none_or(DoorAnimation::can_close) {
            continue;
        }
        // The far side has to be there. It is, in every ordinary case - the plan holds it for as
        // long as the door is open - but a door that swung shut over a space that is not streamed
        // in is the very unload the player is not supposed to see.
        if !destination_is_loaded(&door.destination, anchor, streaming) {
            continue;
        }
        // And nobody may be standing in the doorway. The leaf turns solid again the moment the
        // state leaves `Open` (`mesh_is_out_of_the_way`, which the walk probe reads the same way),
        // so pulling it through the player would shut them into it - the same refusal `E` makes.
        if ends
            .iter()
            .any(|(global, local, instance_bounds, expected_bounds)| {
                in_the_doorway(global, local, *instance_bounds, *expected_bounds, feet)
            })
        {
            continue;
        }
        info!(
            door = format_args!("{:08X}", door.ref_id),
            distance = global.translation().distance(eye),
            destination = %door.label,
            "door: left open and walked away from; closing it"
        );
        open.write(OpenDoor { door: entity });
    }
}

/// Whether the player whose feet are at `feet` stands in the door's own doorway: the volume the
/// walk-through trigger fires in ([`crate::player::auto_door_trigger`]), which is the box the
/// doorway's own model measures.
///
/// `crate::player` refuses `E`'s close in that volume grown by a body's slack
/// (`stands_in_doorway`); that growth is private to the player module, and what it is for - a
/// player whose *body* is still in an opening their centre has just left - is not this question.
/// This one is asked only of a camera more than [`AUTO_CLOSE_DISTANCE`] away, and the box itself is
/// what answers it: a doorway box is 60 units deep however wide it is
/// ([`AUTO_DOOR_TRIGGER_DEPTH`](crate::player::AUTO_DOOR_TRIGGER_DEPTH)), so being far from the
/// door's own reference is not the same thing as being outside the doorway - a model whose doorway
/// stands far from its reference has a box reaching that far with it.
fn in_the_doorway(
    global: &GlobalTransform,
    local: &Transform,
    instance_bounds: Option<&InstanceBounds>,
    expected_bounds: Option<&ExpectedModelBounds>,
    feet: Vec3,
) -> bool {
    auto_door_trigger(
        global.translation(),
        global.rotation(),
        local.scale,
        instance_bounds,
        expected_bounds,
    )
    .contains(feet)
}

/// A load door with everything the auto-close reads: which entity it is (the message names one),
/// where it stands and how it is turned, its link - whose destination is the space that has to be
/// there before it shuts - its state, its doorway anchor, what its model resolved to, and the two
/// bounds [`crate::player::auto_door_trigger`] measures the doorway box from.
type DoorAutoCloseQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static GlobalTransform,
        &'static Transform,
        &'static LoadDoor,
        &'static DoorState,
        Option<&'static DoorAnchor>,
        Option<&'static DoorAnimation>,
        Option<&'static InstanceBounds>,
        Option<&'static ExpectedModelBounds>,
    ),
>;

/// `E` on a load door, or a script's request: runs the door's state machine for it
/// ([`run_activation`]) - opening a closed door, closing an open one - unless the space behind the
/// door is not there yet.
///
/// The message is [`OpenDoor`], which is what the player's `E` writes for both directions: the same
/// press means "open" at a closed door and "close" at an open one, and the door's own state is what
/// decides which. [`ActivateDoor`] is not read here on purpose: that is a scripted run's crossing,
/// it moves the camera in the same frame, and starting a swing for a door the camera has already
/// left is work nobody sees ([`crate::doors::ActivateDoor`]).
///
/// A second activation while the swing is in flight does nothing: a door that is already opening
/// cannot be told anything new, and one that is closing is on its way back to the rest pose, which
/// is the only pose a `Close` clip is allowed to run from.
///
/// A **closed seamless load door whose destination is not streamed in** does not open onto nothing:
/// the request is held in [`PendingDoorOpens`] until [`destination_is_loaded`] is true.
/// Load-screen activation may swing immediately; its crossing owns destination loading behind black. A close is never held - it is about the space the player
/// stands in, which is loaded by definition. `pub(crate)` so that the hold can be driven end to end
/// in one test: `crate::transition`'s tests run this against the real streamer, which is the only
/// place a cell becomes resident.
pub(crate) fn activate_doors(
    mut requests: MessageReader<OpenDoor>,
    mut doors: DoorActivationQuery,
    mut pending: ResMut<PendingDoorOpens>,
    streaming: Option<Res<StreamingWorld>>,
    config: Option<Res<crate::config::EngineConfig>>,
    door_layer: Option<Res<crate::door_layer::DoorLayer>>,
    mut players: Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
) {
    let streaming = streaming.as_deref();
    // Only a seamless doorway exposes the destination while its source leaf opens.
    // A load-screen crossing owns destination readiness behind its opaque cover.
    let needs_destination = config
        .as_deref()
        .is_none_or(|config| config.seamless_doors());
    for request in requests.read() {
        // A door on the door layer is the door-layer door, and its state is its driver's: `E` at
        // the follower - the native on the player's side after a crossing - works the driver, and
        // the far-door drive swings the follower with it (`crate::door_layer`).
        let target = door_layer
            .as_deref()
            .and_then(|layer| layer.driver_of(request.door))
            .unwrap_or(request.door);
        let Ok((entity, door, mut state, animation, anchor)) = doors.get_mut(target) else {
            continue;
        };
        // An invisible `AutoLoadDoor01` marker has no leaf to open: `player_auto_doors` crosses it
        // on contact, and the portal never draws it.
        if door.auto_load {
            continue;
        }
        // A second `E` at a door whose opening is waiting for its far side: a cancel. Nothing opens
        // for either press - the player asked twice and the door is not coming - and the next press
        // asks again.
        if let Some(index) = pending.doors.iter().position(|held| *held == entity) {
            pending.doors.swap_remove(index);
            debug!(
                door = format_args!("{:08X}", door.ref_id),
                "door: its held opening was cancelled"
            );
            continue;
        }
        // A **closed load door** whose far side is not streamed in yet does not swing onto nothing:
        // the request waits in `pending`, and the loop below opens it the frame its destination is
        // there. Only an opening is held - a close is about the space the player is standing in,
        // and a door mid-swing has nothing to hold.
        if *state == DoorState::Closed
            && needs_destination
            && !destination_is_loaded(&door.destination, anchor, streaming)
        {
            if !pending.doors.contains(&entity) {
                pending.doors.push(entity);
            }
            debug!(
                door = format_args!("{:08X}", door.ref_id),
                destination = %door.label,
                "door: holding the opening until its destination is loaded"
            );
            continue;
        }
        if animation.is_none() {
            pending.doors.push(entity);
            debug!(
                door = format_args!("{:08X}", door.ref_id),
                "door: waiting for source animation resolution"
            );
            continue;
        }
        let has_animation = animation.is_some();
        let (has_open_clip, player_found) = animation.map_or((false, false), |animation| {
            (
                animation.open.is_some(),
                animation
                    .player
                    .is_some_and(|player| players.get(player).is_ok()),
            )
        });
        let before = *state;
        run_activation(&mut state, animation, &mut players);
        debug!(
            door = format_args!("{:08X}", door.ref_id),
            has_animation,
            has_open_clip,
            player_found,
            ?before,
            after = ?*state,
            "door: asked to open or close"
        );
    }

    // The held requests: each opens the door it was asked for the first frame the space behind it
    // is streamed in. One whose door is gone by then, or that is no longer closed - something else
    // opened it - is dropped: there is nothing left to open.
    pending.doors.retain(|entity| {
        let Ok((_, door, mut state, animation, anchor)) = doors.get_mut(*entity) else {
            return false;
        };
        // Something else opened it, or it is mid-swing: nothing left to open.
        if *state != DoorState::Closed {
            return false;
        }
        if (needs_destination && !destination_is_loaded(&door.destination, anchor, streaming))
            || animation.is_none()
        {
            return true;
        }
        let before = *state;
        run_activation(&mut state, animation, &mut players);
        info!(
            door = format_args!("{:08X}", door.ref_id),
            destination = %door.label,
            ?before,
            after = ?*state,
            "door: its destination is loaded; opening it"
        );
        false
    });
}

/// The state machine one activation runs, whatever asked for it: `E` at the door, or a held
/// request's destination arriving.
///
/// * `Closed` opens: the resolved `Open` clip starts from rest, or a resolved static leaf opens
///   in one frame. A missing derived view/player remains unresolved and cannot open statically.
/// * `Open` closes: the `Close` clip starts at the pose the `Open` clip reached (the reversal is
///   continuous whatever the two sequences' timings are), a model with no clips at all shuts in
///   this frame, and a model with an `Open` clip and no `Close` replays its opening - it never
///   closes (design section 5).
/// * `Opening` and `Closing` are left exactly as they are: a door that is already swinging cannot
///   be told anything new, and the rest pose is the only pose a `Close` clip may run from.
fn run_activation(
    state: &mut DoorState,
    animation: Option<&DoorAnimation>,
    players: &mut Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
) {
    let Some(animation) = animation.copied() else {
        return;
    };
    if animation.swings() && animation.player.is_none_or(|p| players.get(p).is_err()) {
        return;
    }
    match *state {
        DoorState::Closed => {
            // The rest pose is the pose the `Open` clip starts from, so the swing begins at zero.
            *state = if play_clip(players, animation, animation.open, 0.0) {
                DoorState::Opening
            } else {
                DoorState::Open { animated: false }
            };
        }
        DoorState::Open { .. } => {
            if let Some(close) = animation.close {
                let reached = clip_fraction_of(players, animation, animation.open);
                let seek = (1.0 - reached) * close.seconds;
                if play_clip(players, animation, Some(close), seek) {
                    *state = DoorState::Closing;
                }
            } else if animation.open.is_none() {
                // A door with no clips at all: there is nothing to play, so the doorway shuts in
                // this frame - the mirror of the one-frame opening above, and the only close such
                // a door has.
                *state = DoorState::Closed;
            } else if play_clip(players, animation, animation.open, 0.0) {
                // A model with an `Open` clip and no `Close`: re-activation replays the open, and
                // the door never closes (design section 5). There is no pose to match - a replay
                // starts at the rest pose - so this is the one reversal that moves.
                *state = DoorState::Opening;
            }
        }
        DoorState::Opening | DoorState::Closing => {}
    }
}

/// The `E` presses waiting for the space behind a door.
///
/// A door opens onto a doorway, and the doorway has to show what is behind it: a swing started
/// before the destination is streamed in opens onto a hole with nothing there - the user's demo
/// note of 2026-09-25, "doors should always wait for the world they transition into to be fully
/// loaded before opening". [`destination_is_loaded`] is that question - every cell the plan
/// pre-streams for the door, resident in full - and it is asked again every frame here, since the
/// answer changes while the player walks up.
///
/// The list is empty except while a load door's destination streams in: a door that has no
/// destination, or whose destination is already there, opens at once as it always has. Nothing
/// times a request out. A request is dropped when the door it names is gone, when the door is no
/// longer closed (something else opened it), and when the player presses `E` at it again - a
/// cancel: they asked twice, and the door is not coming. The cell behind a door the plan
/// pre-streams is a cell that arrives, so there is nothing else to give up on.
#[derive(Resource, Default)]
pub(crate) struct PendingDoorOpens {
    /// The closed doors whose openings are waiting, oldest first.
    doors: Vec<Entity>,
}

/// A load door with everything an activation reads: which entity it is (a held request names one),
/// its link - whose destination is the space an opening waits for - its state, what its model
/// resolved to, and its doorway anchor, which is what the destination's own cell is read from.
type DoorActivationQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static LoadDoor,
        &'static mut DoorState,
        Option<&'static DoorAnimation>,
        Option<&'static DoorAnchor>,
    ),
>;

/// How many frames a crossing's far door may be waited for before the opening is dropped.
///
/// The destination is streamed in before a crossing is applied
/// ([`crate::transition::destination_is_ready`]), so the far door is normally spawned frames before
/// the player walks through the source doorway; the wait is the backstop for the door that never
/// turns up (a link whose reference is not in the world at all). Giving up is silent: there is
/// nothing to do about it here, and the door is left exactly as it was.
const ARRIVAL_OPEN_WAIT_FRAMES: u32 = 60;

/// The far doors of crossings that have landed but could not be opened yet: normally empty, one
/// entry for a door that is still to be spawned.
#[derive(Resource, Default)]
struct ArrivalOpenings {
    /// One per crossing waiting for its far door, oldest first.
    openings: Vec<PendingArrivalOpen>,
}

/// A far door a crossing landed through, waiting to be opened: which reference it is, the point of
/// its `Open` clip the player arrived at, and how long it has been waiting.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PendingArrivalOpen {
    /// `door_links.destination_ref_id` of the link the crossing was made through: the reference that
    /// has to be got out of the player's way.
    destination_ref_id: u32,
    /// How far through the far door's own `Open` clip it is put when it is found
    /// ([`reached_fraction`], or the end of the clip when the source door's own swing cannot be
    /// asked).
    fraction: f32,
    /// Frames this has waited for its door, against [`ARRIVAL_OPEN_WAIT_FRAMES`].
    frames: u32,
}

/// Opens the far door of a crossing in the frame the player arrives through it.
///
/// A mapped crossing of an anchored door lands the player **in the destination doorway's own
/// plane** (`crate::transition`), which is where the far door of the link stands. Through the source
/// doorway that door was out of the way: [`drive_far_doors`] swung it open with the source door
/// (impl-234), or - a far door it did not reach - the portal hid its leaf and mirrored the source
/// door's onto the destination doorway. Normally it is already open here, and left as it is. So the
/// frame of the swap has to leave the far door out of the way too, or the first thing the player
/// sees in the new space is the back of a closed leaf - at point-blank range - and the walk out of
/// the doorway is blocked by it.
///
/// The pose is the window's: the far door's own `Open` clip, started at the point the source door's
/// own clip has reached ([`reached_fraction`]), so a door that was half open in the window is half
/// open in the room and a door that was fully open is fully open - no swing through the player
/// either way. A door the far side cannot ask - no animation of its own, no clip, a player that is
/// gone - is opened **as far as its clip goes**, which [`open_arrival_door`] does by starting the
/// clip at its end. A door with nothing to animate at all is [`DoorState::Open`] with `animated`
/// false, which the portal already draws as a hole
/// ([`DoorState::hides_whole_reference`]).
///
/// A door that is already open ([`DoorState::is_open`]) is left where it is: whatever opened it, the
/// window showed the player a doorway and the doorway is what they walked into.
///
/// This is not an [open in the sense of `E`](OpenDoor): it never runs a swing from the rest pose and
/// it never closes anything. Nothing here opens a door on the *near* side of a crossing, and the far
/// door stays open afterwards until something else closes it (`crate::transition`'s design note: the
/// portal then renders back through it at the player's back, which is the doorway being a doorway).
fn open_arrival_doors(
    mut requests: MessageReader<OpenDestinationDoor>,
    mut pending: ResMut<ArrivalOpenings>,
    mut doors: Query<(Entity, &LoadDoor, &mut DoorState, Option<&DoorAnimation>)>,
    mut players: Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
) {
    for request in requests.read() {
        // The pose the window showed: the source door's own clip, or the far door's clip end when it
        // cannot be asked. Read here, in the frame of the crossing, while the door that carries it
        // is certainly still there.
        let fraction = doors
            .get(request.door)
            .ok()
            .and_then(|(_, _, _, animation)| animation)
            .and_then(|animation| reached_fraction(animation, &players))
            .unwrap_or(1.0);
        pending.openings.push(PendingArrivalOpen {
            destination_ref_id: request.destination_ref_id,
            fraction,
            frames: 0,
        });
    }

    pending.openings.retain_mut(|opening| {
        for (_, row, mut state, animation) in &mut doors {
            if row.ref_id != opening.destination_ref_id {
                continue;
            }
            if animation
                .is_none_or(|a| a.swings() && a.player.is_none_or(|p| players.get(p).is_err()))
            {
                return true;
            }
            // The far door of the *link*, which is the one `crate::portal::update_portal` finds the
            // same way: the reference the crossing's link names. A load door's reference is spawned
            // once, wherever in the resident grid its cell is.
            if open_arrival_door(&mut state, animation, &mut players, opening.fraction) {
                info!(
                    door = format_args!("{:08X}", row.ref_id),
                    fraction = opening.fraction,
                    state = ?*state,
                    "arrival: opened the far door of a crossing"
                );
            }
            return false;
        }
        // The missing destination reference keeps the existing bounded arrival wait. Once a
        // reference exists, its source readiness has no timeout or static conversion.
        opening.frames += 1;
        opening.frames <= ARRIVAL_OPEN_WAIT_FRAMES
    });
}

/// Puts a far door in the state a crossing arrives at: `fraction` of the way through its own `Open`
/// clip, or an open door with nothing left to animate.
///
/// The clip is *started at* that point rather than played from the rest pose, so the leaf is where
/// the window showed it from the first frame and never swings through the player. The state follows
/// the pose the clip is at ([`OPEN_FRACTION`] is where a swing counts as an open doorway), so the
/// leaves of a clip that did not clear the opening are hidden by the frame's
/// [`update_door_leaves`] and the doorway is walkable either way. A door with nothing to play - a
/// resolved static leaf - opens in this frame exactly as it does when `E` opens it. A missing
/// source/player stays pending and cannot be converted into a hole.
///
/// Returns whether the door was opened by this call: a door that was already open is left where it
/// is, and says so by answering false.
fn open_arrival_door(
    state: &mut DoorState,
    animation: Option<&DoorAnimation>,
    players: &mut Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
    fraction: f32,
) -> bool {
    if state.is_open() {
        return false;
    }
    let Some(animation) = animation else {
        return false;
    };
    if animation.swings() && animation.player.is_none_or(|p| players.get(p).is_err()) {
        return false;
    }
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let seek = fraction * animation.open.map_or(0.0, |open| open.seconds);
    let played = play_clip(players, *animation, animation.open, seek);
    *state = if !played {
        // A resolved static leaf opens in one frame, which the portal draws as a hole.
        // Missing sources and players returned above; they never reach this branch.
        DoorState::Open { animated: false }
    } else if fraction >= OPEN_FRACTION {
        DoorState::Open { animated: true }
    } else {
        // Still swinging: `advance_door_states` moves it on at [`OPEN_FRACTION`] like any other
        // swing, and until then the doorway is one a player may walk through ([`DoorState::is_open`]).
        DoorState::Opening
    };
    true
}

/// How far through its own `Open` clip a door's swing has got, or `None` when it cannot be asked: a
/// door with no animation of its own, no `Open` clip, no player where the animation says one is, or
/// a clip that is not playing at all. A finished clip counts as the whole way through
/// ([`clip_fraction`]), which is the ordinary case: the source doorway's door has been standing open
/// since the player walked up to it.
fn reached_fraction(
    animation: &DoorAnimation,
    players: &Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
) -> Option<f32> {
    let (player, open) = (animation.player?, animation.open?);
    let (player, _) = players.get(player).ok()?;
    clip_fraction(player, open)
}

// ---------------------------------------------------------------------------------------------
// The far door of the doorway the portal draws through swings with the near one (impl-234)
// ---------------------------------------------------------------------------------------------

/// The pairs of doors [`drive_far_doors`] swings together: the door the portal draws through (the
/// **near** door) and the far door of its doorway, whose own leaf the window shows.
///
/// A pair is taken from [`crate::portal::PortalState::drawn_door_pair`] the first frame the portal
/// draws through it and kept after the portal stops, so a near door that closes out of the
/// portal's sight - walked away from and closed by [`auto_close_doors`] - still closes its far
/// door. It is dropped when both doors are at rest closed, when either is gone, when the near door
/// is no longer in the active space (the player crossed: the far door is theirs now, and the
/// reverse pair is the one that counts), or when the portal draws through the same doorway the
/// other way round.
#[derive(Resource, Debug, Default)]
pub(crate) struct FarDoorDrive {
    pairs: Vec<DrivenPair>,
}

impl FarDoorDrive {
    /// Whether `door` is held open as the far door of a pair whose near door is open: a door the
    /// auto-close leaves to its near door.
    fn holds_open(&self, door: Entity) -> bool {
        self.pairs
            .iter()
            .any(|pair| pair.far == door && pair.near_open == Some(true))
    }

    /// The other door of the pair `door` is in, if it is driven with one.
    fn partner_of(&self, door: Entity) -> Option<Entity> {
        self.pairs.iter().find_map(|pair| {
            if pair.near == door {
                Some(pair.far)
            } else if pair.far == door {
                Some(pair.near)
            } else {
                None
            }
        })
    }
}

/// One doorway's two doors, and what the drive last saw of the near one.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DrivenPair {
    near: Entity,
    far: Entity,
    /// Whether the near door was open ([`DoorState::is_open`]) the last time the far door was put
    /// in line with it, or `None` before the first time. The drive acts on a change of this and on
    /// nothing else, so a far door the player works with their own hands after a crossing is not
    /// fought over.
    near_open: Option<bool>,
    /// Whether the pair is a door-layer pair (`crate::door_layer`): kept for as long as the layer
    /// holds it rather than dropped when both doors are shut or the player crosses.
    layered: bool,
}

/// What the far door of a pair is asked to do this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
enum FarDoorFollow {
    /// Open, `fraction` of the way through its own `Open` clip: the point the near door's has
    /// reached ([`open_arrival_door`]).
    Open { fraction: f32 },
    /// Close, from wherever its swing has got to ([`close_far_door`]).
    Close,
}

/// What the far door does when its near door is in `near`, `near_fraction` of the way through its
/// own `Open` clip (`None` when that cannot be asked), and was open or not (`was_open`) when the
/// far door was last put in line with it.
///
/// Only a change asks anything: a near door that opens opens the far one at the same point of the
/// swing, and one that starts closing - or shuts in one frame, a door with no clips - closes it.
/// `Opening` to `Open` and `Closing` to `Closed` are the same swing going on, which the far door's
/// own clip plays out at the same speed.
fn far_door_follow(
    was_open: Option<bool>,
    near: DoorState,
    near_fraction: Option<f32>,
) -> Option<FarDoorFollow> {
    let open = near.is_open();
    if was_open == Some(open) {
        return None;
    }
    Some(if open {
        FarDoorFollow::Open {
            fraction: near_fraction.unwrap_or(match near {
                DoorState::Opening => 0.0,
                _ => 1.0,
            }),
        }
    } else {
        FarDoorFollow::Close
    })
}

/// Closes a far door with its near door: its `Close` clip started at the pose its `Open` clip has
/// reached, as `E` does ([`run_activation`]), whether it is standing open or still swinging open.
/// A door with no clips shuts in this frame; one with an `Open` clip and no `Close` never closes
/// (design section 5), and is left as it is.
///
/// Returns whether the door was put on its way closed.
fn close_far_door(
    state: &mut DoorState,
    animation: Option<&DoorAnimation>,
    players: &mut Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
) -> bool {
    if !state.is_open() {
        return false;
    }
    let Some(animation) = animation.copied() else {
        return false;
    };
    if let Some(close) = animation.close {
        let reached = clip_fraction_of(players, animation, animation.open);
        if play_clip(
            players,
            animation,
            Some(close),
            (1.0 - reached) * close.seconds,
        ) {
            *state = DoorState::Closing;
            return true;
        }
        false
    } else if animation.open.is_none() {
        *state = DoorState::Closed;
        true
    } else {
        false
    }
}

/// A load door as [`drive_far_doors`] reads and moves it: its state, its animation, and what
/// [`in_the_doorway`] measures its doorway from.
type DrivenDoorQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static LoadDoor,
        &'static mut DoorState,
        Option<&'static DoorAnimation>,
        &'static GlobalTransform,
        &'static Transform,
        Option<&'static InstanceBounds>,
        Option<&'static ExpectedModelBounds>,
    ),
>;

/// Swings the far door of the doorway the portal draws through with the near door (impl-234).
///
/// The two ends of a doorway are two references, one per space. Through the window the player
/// used to see a *mirror* - a second instance of the near door carried through the door map, which
/// turns it half a turn - while from the far side they see the far door's own leaf: the hinged leaf
/// stood on one jamb from outside and on the other from inside (research-174 note 1). Now the
/// window shows the far door's own leaf ([`crate::portal::PortalState::destination_door`] is drawn
/// while it swings), and this puts that leaf where the near door's is: the far door opens at the
/// point of its own `Open` clip the near door's has reached ([`open_arrival_door`], the crossing's
/// own opening), and closes when the near door starts closing - `E`, or [`auto_close_doors`].
///
/// A close is not made on a player standing in the far door's doorway (the refusal `E` and the
/// auto-close make): that is the frame of a crossing, and the pair is dropped the frame after.
fn drive_far_doors(
    portal: Option<Res<crate::portal::PortalState>>,
    door_layer: Option<Res<crate::door_layer::DoorLayer>>,
    parents: Query<&ChildOf>,
    camera: Query<&Transform, With<StreamingCamera>>,
    mut drive: ResMut<FarDoorDrive>,
    mut doors: DrivenDoorQuery,
    mut players: Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
) {
    let portal = portal.as_deref();
    let door_layer = door_layer.as_deref();
    // A pair on the door layer is driven by its driver for as long as it is on the layer, whatever
    // the portal draws through: the door-layer door is posed from the driver alone, so the
    // follower's state - which the crossing, the walk probe and the portal read on the player's side
    // after a crossing - has to be the driver's too (`crate::door_layer`).
    let layered: Vec<(Entity, Entity)> = door_layer
        .map(|layer| layer.driven_pairs().collect())
        .unwrap_or_default();
    for &(near, far) in &layered {
        if !drive
            .pairs
            .iter()
            .any(|pair| pair.near == near && pair.far == far)
        {
            drive.pairs.retain(|pair| {
                pair.near != near && pair.near != far && pair.far != near && pair.far != far
            });
            drive.pairs.push(DrivenPair {
                near,
                far,
                near_open: None,
                layered: true,
            });
        }
    }
    let on_layer = |door: Entity| door_layer.is_some_and(|layer| layer.pair_of(door).is_some());
    if let Some((near, far)) = portal.and_then(crate::portal::PortalState::drawn_door_pair)
        && !on_layer(near)
        && !on_layer(far)
        && !drive
            .pairs
            .iter()
            .any(|pair| pair.near == near && pair.far == far)
    {
        // The same doorway the other way round, or another pair sharing a door with this one: the
        // pair the portal draws through now is the one that counts.
        drive.pairs.retain(|pair| {
            pair.near != near && pair.near != far && pair.far != near && pair.far != far
        });
        drive.pairs.push(DrivenPair {
            near,
            far,
            near_open: None,
            layered: false,
        });
    }

    let feet = camera
        .single()
        .ok()
        .map(|camera| feet_from_eye(camera.translation));
    drive.pairs.retain_mut(|pair| {
        let Ok((_, near_state, near_animation, ..)) = doors.get(pair.near) else {
            return false;
        };
        let near_state = *near_state;
        let near_fraction = near_animation.and_then(|animation| {
            let (player, open) = (animation.player?, animation.open?);
            let (player, _) = players.get(player).ok()?;
            clip_fraction(player, open)
        });
        let Ok((_, far_state, ..)) = doors.get(pair.far) else {
            return false;
        };
        if pair.layered {
            // Kept while the door layer holds the pair, in every state and across a crossing.
            if !layered.contains(&(pair.near, pair.far)) {
                return false;
            }
        } else {
            if near_state == DoorState::Closed && *far_state == DoorState::Closed {
                return false;
            }
            if portal.is_some_and(|portal| !portal.is_in_active_space(pair.near, &parents)) {
                return false;
            }
        }
        let Some(follow) = far_door_follow(pair.near_open, near_state, near_fraction) else {
            return true;
        };
        let Ok((far_row, mut far_state, far_animation, global, local, instance, expected)) =
            doors.get_mut(pair.far)
        else {
            return false;
        };
        match follow {
            FarDoorFollow::Open { fraction } => {
                if open_arrival_door(&mut far_state, far_animation, &mut players, fraction) {
                    debug!(
                        door = format_args!("{:08X}", far_row.ref_id),
                        fraction,
                        state = ?*far_state,
                        "door: the far door of the doorway opens with the near one"
                    );
                }
            }
            FarDoorFollow::Close => {
                if feet.is_some_and(|feet| in_the_doorway(global, local, instance, expected, feet))
                {
                    // Not through the player: asked again next frame.
                    return true;
                }
                if close_far_door(&mut far_state, far_animation, &mut players) {
                    debug!(
                        door = format_args!("{:08X}", far_row.ref_id),
                        state = ?*far_state,
                        "door: the far door of the doorway closes with the near one"
                    );
                }
            }
        }
        pair.near_open = Some(near_state.is_open());
        true
    });
}

/// Moves a door on when its clip reaches the point that matters: `Opening` becomes [`DoorState::Open`]
/// at [`OPEN_FRACTION`] of the `Open` clip, and `Closing` becomes [`DoorState::Closed`] when the
/// `Close` clip finishes.
///
/// The clip's own clock decides both, not a timer of this module's: `advance_animations` is the one
/// place that adds time to an animation, so a long frame moves the door on by exactly the clip time
/// that frame played, and a clip that is finished holds its last key - the door standing open -
/// without the state having to notice that it stopped.
fn advance_door_states(
    mut doors: Query<(&mut DoorState, &DoorAnimation)>,
    players: Query<&AnimationPlayer>,
) {
    for (mut state, animation) in &mut doors {
        let Some(player) = animation.player.and_then(|player| players.get(player).ok()) else {
            continue;
        };
        match *state {
            DoorState::Opening => {
                if let Some(open) = animation.open
                    && clip_fraction(player, open).is_some_and(|f| f >= OPEN_FRACTION)
                {
                    *state = DoorState::Open { animated: true };
                }
            }
            DoorState::Closing => {
                if let Some(close) = animation.close
                    && clip_fraction(player, close).is_some_and(|f| f >= 1.0)
                {
                    *state = DoorState::Closed;
                }
            }
            DoorState::Closed | DoorState::Open { .. } => {}
        }
    }
}

/// Draws or hides a door's leaves for the state the door is in.
///
/// The swing is what the player asked to see, so the leaves are drawn the whole time the `Open`
/// clip plays - `Closed`, `Opening` and `Closing` all draw them - and the only frame that changes
/// that is the one where the door becomes `Open`, where the answer is whether the clip left the
/// leaves in the opening ([`DoorAnimation::clears_doorway`]).
///
/// The write goes straight to the component rather than through `Commands`: a crossing despawns a
/// whole cell's worth of doors in the frame it happens in, and a command queued against a leaf the
/// same frame despawns is applied to a dead entity, which Bevy treats as a panic (the same note as
/// `portal::show_load_door_leaves`). Writing only on a change keeps the visibility hierarchy from
/// being recomputed for every leaf every frame.
fn update_door_leaves(
    doors: Query<(&DoorState, Option<&DoorAnimation>)>,
    held: Query<(), With<CrossingHeld>>,
    mut leaves: Query<(&DoorLeaf, &mut Visibility)>,
) {
    for (leaf, mut visibility) in &mut leaves {
        let Ok((state, animation)) = doors.get(leaf.door) else {
            // The door is gone (its cell unloaded): its leaves are going with it.
            continue;
        };
        let wanted = if leaves_are_drawn(*state, animation, held.contains(leaf.door)) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

/// Whether a door's leaves are drawn in this state, for a door whose animation resolved to this and
/// which is (or is not) holding a crossing of its own.
fn leaves_are_drawn(state: DoorState, animation: Option<&DoorAnimation>, held: bool) -> bool {
    if held {
        // The crossing is waiting for its destination to stream in: the door is drawn as it was -
        // leaves and all - because there is no window through the doorway yet either, and a
        // doorway with neither is a hole in the world (`crate::transition::CrossingHeld`).
        return true;
    }
    match state {
        // A door that is `Open` and has an animation of its own shows its leaves only if the swing
        // took them out of the opening; one that turned its leaves a few degrees is a wall with a
        // wall behind it, so its leaves go.
        DoorState::Open { animated: true } => {
            animation.is_none_or(|animation| animation.clears_doorway)
        }
        // `Opening` and `Closing` are the swing itself, and a `Closed` door is a door: the leaves
        // are drawn. `Open { animated: false }` is a door with no leaf of its own to draw - and
        // its whole model is the portal's to hide, from `DoorState::hides_whole_reference`.
        DoorState::Closed
        | DoorState::Opening
        | DoorState::Closing
        | DoorState::Open { animated: false } => true,
    }
}

/// Starts one of a door's clips on its own player, `seek_time` seconds in, cross-fading from
/// whatever the door was playing.
///
/// The fade (design section 2.3) is what keeps a reversal from snapping: the clip that is being
/// left is faded out over [`CLIP_FADE`] rather than stopped dead, so a door told to close halfway
/// through opening continues from where it is rather than jumping to the pose `Close` happens to
/// start on. Both clips drive the same node transforms, so an un-faded pair would fight over the
/// pose at weight 1.0 each.
///
/// Returns false when there is nothing to play: no such clip, or no player (`None` for a model with
/// no clips, and a query miss for one whose cell unloaded in this same frame).
fn play_clip(
    players: &mut Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
    animation: DoorAnimation,
    start: Option<DoorClip>,
    seek_time: f32,
) -> bool {
    let (Some(player), Some(start)) = (animation.player, start) else {
        return false;
    };
    let Ok((mut player, transitions)) = players.get_mut(player) else {
        return false;
    };
    let seek_time = if seek_time.is_finite() {
        seek_time.clamp(0.0, start.seconds.max(0.0))
    } else {
        0.0
    };
    match transitions {
        Some(mut transitions) => {
            transitions
                .play(&mut player, start.node, CLIP_FADE)
                // `set_seek_time` rather than `seek_to`: the part of the clip that is being skipped
                // should not fire its events, and a door that is closing again should not replay
                // the sound of opening.
                .set_seek_time(seek_time);
        }
        // A player without the transitions component - a hand-built one - still plays the clip, it
        // just switches without a fade. `attach_door_animations` always inserts both together.
        None => {
            let playing = player.play(start.node);
            playing.replay();
            playing.set_seek_time(seek_time);
        }
    }
    true
}

/// How far through its clip a door's playing animation is, as a fraction of the clip's length, or
/// `None` while that node is not playing at all.
///
/// The clock is the one the pose is drawn from: `animate_targets` evaluates every curve at the
/// animation's `seek_time` (`bevy_animation-0.19.0/src/lib.rs#L1235`), while its `elapsed` is the
/// time the animation has been *playing*. The two agree on a clip that was started at its own zero,
/// which is every door the player opened, and they part company the moment anything seeks one -
/// which the reversal (`activate_doors`, a door told to close mid-swing) and a crossing's arrival
/// ([`open_arrival_door`], which puts the far door at the pose the window showed) both do. A state
/// read off the wrong clock says `Opening` at a pose that has already passed the mark, and that is a
/// leaf drawn in a doorway the player is walking through.
///
/// A finished clip counts as the whole way through, and so does a clip of no length: a model that
/// exported a zero-second sequence would otherwise leave its door mid-swing for good.
fn clip_fraction(player: &AnimationPlayer, clip: DoorClip) -> Option<f32> {
    let animation = player.animation(clip.node)?;
    if animation.is_finished() || clip.seconds <= 0.0 {
        return Some(1.0);
    }
    Some(animation.seek_time() / clip.seconds)
}

/// The same fraction for a clip whose animation has to be looked up through the door's player
/// first, which the activation path needs when it has to line a reversal up with the pose the door
/// has reached. Zero when there is no player to ask or no clip playing on it.
fn clip_fraction_of(
    players: &Query<(&mut AnimationPlayer, Option<&mut AnimationTransitions>)>,
    animation: DoorAnimation,
    clip: Option<DoorClip>,
) -> f32 {
    let (Some(player), Some(clip)) = (animation.player, clip) else {
        return 0.0;
    };
    let Ok((player, _)) = players.get(player) else {
        return 0.0;
    };
    clip_fraction(player, clip).unwrap_or(0.0)
}

/// The `Open` and `Close` clips a door model carries, by case-insensitive name.
///
/// `Open`/`Close` is a convention rather than a guarantee, so a model that names neither gets its
/// first clip as the opening one, and a model with no clips at all gets none - it is a static door.
fn door_clips(model: &Gltf) -> (Option<Handle<AnimationClip>>, Option<Handle<AnimationClip>>) {
    let named = |wanted: &str| {
        model
            .named_animations
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
            .map(|(_, clip)| clip.clone())
    };
    (
        named(OPEN_CLIP).or_else(|| model.animations.first().cloned()),
        named(CLOSE_CLIP),
    )
}

/// The animation targets a door's clips have curves for: exactly the nodes they move.
///
/// An [`AnimationTargetId`] on its own does not say which nodes those are, because `bevy_gltf` gives
/// one to every node under the animation root - the frame and the arch with it
/// (`bevy_gltf-0.19.0/src/loader/mod.rs#L1558`) - so the leaf is the intersection of the ids the
/// scene carries and the ids the clips name.
fn clip_targets<'a>(
    clips: impl IntoIterator<Item = &'a AnimationClip>,
) -> HashSet<AnimationTargetId> {
    clips
        .into_iter()
        .flat_map(|clip| clip.curves().keys().copied())
        .collect()
}

/// Marks the nodes a door's clips move with [`DoorLeaf`], so the walk probe can tell the moving
/// leaf from the frame it hangs in.
fn mark_leaf_nodes(
    commands: &mut Commands,
    door: Entity,
    moved: &HashSet<AnimationTargetId>,
    children: &Query<&Children>,
    targets: &Query<&AnimationTargetId>,
) {
    if moved.is_empty() {
        return;
    }
    for node in std::iter::once(door).chain(children.iter_descendants(door)) {
        if let Ok(target) = targets.get(node)
            && moved.contains(target)
        {
            commands.entity(node).try_insert(DoorLeaf { door });
        }
    }
}

/// The double-sided copy of each material a door leaf is drawn with, by the material it was copied
/// from, or `None` for a material that is kept as it is ([`draw_leaves_double_sided`]).
///
/// One copy per source material, and the source materials are the model's own - every drawn copy of
/// a door model shares them - so this is one copy per door model's leaf material for the whole run,
/// made the first time a leaf with it is marked.
#[derive(Resource, Debug, Default)]
struct DoubleSidedLeafMaterials(
    HashMap<AssetId<StandardMaterial>, Option<Handle<StandardMaterial>>>,
);

/// Draws every door leaf's meshes from both sides.
///
/// A load door's leaf is modelled one-sided, because the game never shows its back: a load door is
/// a loading screen, and the player is never on the far side of the leaf. A seamless door is seen
/// from both sides - from the room behind it once it is open, and through the portal - and from
/// behind, a one-sided leaf is only its edges (the user's captures of 2026-09-25: Bleak Falls
/// Barrow's great door, "a bit broken", and Cracked Tusk Keep's, "no polygons or textures").
///
/// So each mesh under a node marked [`DoorLeaf`] is moved onto a double-sided copy of its material
/// ([`double_sided`]); the frame, and every other model sharing the material, keeps the original.
/// Bevy flips the normal of a double-sided material's back faces, so the back of a leaf is lit as a
/// surface facing the other way. A material the engine drives by its asset path - a palette effect,
/// an animated texture ([`crate::streaming`] finds those by the material's path, which a copy has
/// not got) - is left alone.
#[allow(clippy::too_many_arguments)]
fn draw_leaves_double_sided(
    mut commands: Commands,
    // `None` in an app without the renderer's material collection (the door tests' own apps):
    // nothing is drawn there, so there is nothing to draw from behind.
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    asset_server: Option<Res<AssetServer>>,
    palettes: Option<Res<crate::effect_palette::EffectPaletteRegistry>>,
    animations: Option<Res<crate::material_animation::MaterialAnimationRegistry>>,
    mut copies: ResMut<DoubleSidedLeafMaterials>,
    leaves: Query<Entity, Added<DoorLeaf>>,
    children: Query<&Children>,
    primitives: Query<&MeshMaterial3d<StandardMaterial>>,
) {
    let Some(mut materials) = materials else {
        return;
    };
    let driven_by_path = |id: AssetId<StandardMaterial>| {
        let Some(path) = asset_server.as_ref().and_then(|server| server.get_path(id)) else {
            return false;
        };
        let path = path.to_string();
        palettes
            .as_ref()
            .is_some_and(|palettes| palettes.get(&path).is_some())
            || animations
                .as_ref()
                .is_some_and(|animations| animations.get(&path).is_some())
    };
    for leaf in &leaves {
        for node in std::iter::once(leaf).chain(children.iter_descendants(leaf)) {
            let Ok(material) = primitives.get(node) else {
                continue;
            };
            let id = material.id();
            let copy = match copies.0.get(&id) {
                Some(copy) => copy.clone(),
                None => {
                    // A material still on its way has nothing to copy yet; the leaf keeps it.
                    let Some(source) = materials.get(id) else {
                        continue;
                    };
                    let copy = if driven_by_path(id) {
                        None
                    } else {
                        double_sided(source).map(|copy| materials.add(copy))
                    };
                    copies.0.insert(id, copy.clone());
                    copy
                }
            };
            if let Some(copy) = copy {
                commands.entity(node).try_insert(MeshMaterial3d(copy));
            }
        }
    }
}

/// `material` drawn from both sides, or `None` when it already is.
///
/// Both fields, and they agree: `cull_mode: None` is what stops the back faces being culled, and
/// `double_sided` is what flips their normals so they are lit - and what the streaming readiness
/// check holds the two to.
fn double_sided(material: &StandardMaterial) -> Option<StandardMaterial> {
    if material.double_sided && material.cull_mode.is_none() {
        return None;
    }
    Some(StandardMaterial {
        double_sided: true,
        cull_mode: None,
        ..material.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        doors::{DoorCrossed, DoorDestination, DoorTarget},
        player::{EYE_HEIGHT, Player, eye_from_feet, player_walks_through_doors},
        profiling::ProfilingState,
        transition::CrossDoor,
        world::components::StreamingCamera,
    };
    use bevy::{
        animation::{
            AnimatedBy, animated_field,
            animation_curves::{AnimatableCurve, AnimatableKeyframeCurve},
        },
        asset::AssetApp,
        ecs::system::SystemState,
        time::TimeUpdateStrategy,
    };
    use std::time::Duration;

    /// How long the door's clips last in these tests.
    const CLIP_SECONDS: f32 = 1.0;

    /// The clip time one `app.update()` plays with [`TimeUpdateStrategy::ManualDuration`] - a tenth
    /// of a clip, so "0.4 s in" is four frames and the boundary at [`OPEN_FRACTION`] is between
    /// frames rather than on one.
    const STEP_SECONDS: f32 = 0.1;

    /// A leaf that swings clear of the doorway, like a Nordic or Imperial door (115-135 degrees).
    const WIDE_SWING_DEGREES: f32 = 120.0;

    /// A leaf that barely moves, like the demo route's `DweDoorLarge01Load` (5-9 degrees).
    const NARROW_SWING_DEGREES: f32 = 8.0;

    /// A model path with no `load` marker in it, and so no twin to look for.
    const PLAIN_MODEL_PATH: &str = "meshes/fixtures/door/door01.glb";

    /// A hand-built load door: the reference, the player and the scene nodes the loader would have
    /// spawned for a model whose `Open` clip moves one node.
    struct Door {
        door: Entity,
        player: Entity,
        /// The node the clips move - the leaf - with a mesh primitive under it.
        leaf: Entity,
        leaf_mesh: Entity,
        /// The door's frame: a node under the same animation root, carrying the same
        /// [`AnimationTargetId`] the loader gives every node there, but named by no clip.
        frame: Entity,
        frame_mesh: Entity,
        open_node: AnimationNodeIndex,
        close_node: AnimationNodeIndex,
    }

    /// A door with a model's clips, before [`attach_door_animations`] has run over it.
    struct ModelDoor {
        door: Entity,
        player: Entity,
        /// The node the `Open` clip moves, and its mesh primitive.
        moved: Entity,
        moved_mesh: Entity,
        /// A node under the animation root that no clip names.
        frame: Entity,
    }

    #[test]
    fn same_player_static_revision_withdraws_private_graph_and_stops_its_real_curve() {
        let mut app = door_app();
        let model = door_with_model(&mut app, WIDE_SWING_DEGREES);
        app.update();
        activate(&mut app, model.door);
        step(&mut app, 2);
        assert!(
            player(&app, model.player)
                .playing_animations()
                .next()
                .is_some()
        );
        let pose = *app.world().get::<Transform>(model.moved).unwrap();
        assert!(leaf_degrees(&app, model.moved) > 0.0);
        let authored = app
            .world()
            .get::<SourceClipBinding>(model.door)
            .unwrap()
            .clone();
        let authored_graph = authored.graph.clone().unwrap();
        app.world_mut()
            .entity_mut(authored.source)
            .insert(SourceClips::default());

        app.update();

        assert_eq!(state(&app, model.door), DoorState::Open { animated: false });
        assert!(
            !app.world()
                .get::<DoorAnimation>(model.door)
                .unwrap()
                .swings()
        );
        assert!(app.world().get::<DerivedGraph>(model.door).is_none());
        assert!(
            app.world()
                .get::<AnimationGraphHandle>(model.player)
                .is_none()
        );
        assert!(
            app.world()
                .get::<AnimationTransitions>(model.player)
                .is_none()
        );
        assert!(
            player(&app, model.player)
                .playing_animations()
                .next()
                .is_none()
        );
        assert!(
            app.world()
                .resource::<Assets<AnimationGraph>>()
                .get(&authored_graph)
                .is_some()
        );
        assert!(
            app.world()
                .get::<SourceClips>(authored.source)
                .unwrap()
                .0
                .is_empty()
        );
        step(&mut app, 3);
        assert_eq!(*app.world().get::<Transform>(model.moved).unwrap(), pose);
        assert!(app.world().get_entity(model.moved_mesh).is_ok());
    }

    #[test]
    fn same_player_unresolved_revision_withdraws_private_graph_until_real_curve_returns() {
        let mut app = door_app();
        let model = door_with_model(&mut app, WIDE_SWING_DEGREES);
        app.update();
        activate(&mut app, model.door);
        step(&mut app, 2);
        let authored = app
            .world()
            .get::<SourceClipBinding>(model.door)
            .unwrap()
            .clone();
        let inventory = app
            .world()
            .get::<SourceClips>(authored.source)
            .unwrap()
            .0
            .clone();
        let old_graph = app
            .world()
            .get::<DerivedGraph>(model.door)
            .unwrap()
            .0
            .clone();
        let pose = *app.world().get::<Transform>(model.moved).unwrap();
        let missing = {
            let mut clips = app.world_mut().resource_mut::<Assets<AnimationClip>>();
            let handle = clips.add(AnimationClip::default());
            clips.remove(handle.id());
            handle
        };
        app.world_mut()
            .entity_mut(authored.source)
            .insert(SourceClips(vec![(OPEN_CLIP.into(), missing.clone())]));

        app.update();

        assert!(app.world().get::<SourceClipBinding>(model.door).is_none());
        assert!(app.world().get::<DoorAnimation>(model.door).is_none());
        assert!(app.world().get::<DerivedGraph>(model.door).is_none());
        assert!(app.world().get::<DerivedSource>(model.door).is_none());
        assert!(
            app.world()
                .get::<AnimationGraphHandle>(model.player)
                .is_none()
        );
        assert!(
            app.world()
                .get::<AnimationTransitions>(model.player)
                .is_none()
        );
        assert!(
            player(&app, model.player)
                .playing_animations()
                .next()
                .is_none()
        );
        assert_eq!(
            app.world().get::<ReattachPose>(model.door).unwrap().0,
            DoorState::Open { animated: true }
        );
        assert_eq!(
            app.world().get::<SourceClips>(authored.source).unwrap().0[0].1,
            missing
        );
        step(&mut app, UNRESOLVED_OBSERVATION_FRAMES);
        assert!(app.world().get::<DoorAnimation>(model.door).is_none());
        assert_eq!(*app.world().get::<Transform>(model.moved).unwrap(), pose);

        app.world_mut()
            .entity_mut(authored.source)
            .insert(SourceClips(inventory));
        app.update();
        let graph = &app.world().get::<DerivedGraph>(model.door).unwrap().0;
        assert_ne!(*graph, old_graph);
        assert_eq!(
            &app.world()
                .get::<AnimationGraphHandle>(model.player)
                .unwrap()
                .0,
            graph
        );
        assert_eq!(state(&app, model.door), DoorState::Open { animated: true });
        assert!(app.world().get::<ReattachPose>(model.door).is_none());
        assert!((leaf_degrees(&app, model.moved) - WIDE_SWING_DEGREES).abs() < 0.1);
        assert!(
            player(&app, model.player)
                .playing_animations()
                .next()
                .is_some()
        );
    }

    #[test]
    fn private_withdrawal_preserves_replacement_graph_and_its_playback() {
        let mut app = door_app();
        let model = door_with_model(&mut app, WIDE_SWING_DEGREES);
        app.update();
        activate(&mut app, model.door);
        let authored = app
            .world()
            .get::<SourceClipBinding>(model.door)
            .unwrap()
            .clone();
        // A foreign owner gets a distinct asset identity, not the withdrawn authored handle.
        let replacement_value = app
            .world()
            .resource::<Assets<AnimationGraph>>()
            .get(authored.graph.as_ref().unwrap())
            .unwrap()
            .clone();
        let replacement = app
            .world_mut()
            .resource_mut::<Assets<AnimationGraph>>()
            .add(replacement_value);
        app.world_mut()
            .entity_mut(model.player)
            .insert(AnimationGraphHandle(replacement.clone()));
        app.world_mut()
            .entity_mut(authored.source)
            .insert(SourceClips::default());

        app.update();

        assert_eq!(
            app.world()
                .get::<AnimationGraphHandle>(model.player)
                .unwrap()
                .0,
            replacement
        );
        assert!(
            player(&app, model.player)
                .playing_animations()
                .next()
                .is_some()
        );
        assert!(
            app.world()
                .get::<AnimationTransitions>(model.player)
                .is_some()
        );
        assert!(app.world().get::<DerivedGraph>(model.door).is_none());
        assert_eq!(state(&app, model.door), DoorState::Open { animated: false });
    }

    #[test]
    fn public_source_revision_rebuilds_private_graph_without_changing_authored_clips_or_live_meshes()
     {
        let mut app = door_app();
        let model = door_with_model(&mut app, WIDE_SWING_DEGREES);
        app.update();
        activate(&mut app, model.door);
        step(&mut app, 20);
        assert!(state(&app, model.door).is_open());
        let old_binding = app
            .world()
            .get::<SourceClipBinding>(model.door)
            .unwrap()
            .clone();
        let old_derived = app
            .world()
            .get::<DerivedGraph>(model.door)
            .unwrap()
            .0
            .clone();
        let target = *app.world().get::<AnimationTargetId>(model.moved).unwrap();
        let replacement = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip_swinging(CLIP_SECONDS, target, 0.0, WIDE_SWING_DEGREES));
        app.world_mut()
            .entity_mut(old_binding.source)
            .insert(SourceClips(vec![("Open".into(), replacement.clone())]));
        app.update();
        let binding = app.world().get::<SourceClipBinding>(model.door).unwrap();
        let derived = app.world().get::<DerivedGraph>(model.door).unwrap();
        assert_ne!(binding.graph, old_binding.graph);
        assert_ne!(derived.0, old_derived);
        assert_ne!(binding.graph.as_ref(), Some(&derived.0));
        assert_eq!(
            app.world()
                .get::<AnimationGraphHandle>(model.player)
                .unwrap()
                .0,
            derived.0,
            "the newly attached private graph remains authoritative after withdrawal"
        );
        assert_eq!(
            app.world().get::<SourceClips>(binding.source).unwrap().0[0].1,
            replacement
        );
        assert!(app.world().get_entity(model.moved_mesh).is_ok());
        assert!(
            state(&app, model.door).is_open(),
            "restore the existing doorway pose on the new binding"
        );
    }

    #[test]
    fn seamless_request_and_arrival_remain_pending_until_source_binding_resolves() {
        let mut app = door_app();
        let source = animated_door(&mut app);
        let far_row = far_row(&app, source.door);
        // A streamed modeled door with no completed source inventory/player yet.
        let far = app
            .world_mut()
            .spawn((
                far_row,
                MeshHandle(PLAIN_MODEL_PATH.into()),
                DoorState::Closed,
            ))
            .id();
        activate(&mut app, far);
        assert_eq!(state(&app, far), DoorState::Closed);
        assert!(
            app.world()
                .resource::<PendingDoorOpens>()
                .doors
                .contains(&far)
        );
        cross(&mut app, source.door);
        step(&mut app, UNRESOLVED_OBSERVATION_FRAMES);
        assert_eq!(state(&app, far), DoorState::Closed);
        assert!(app.world().get::<DoorAnimation>(far).is_none());
        assert!(
            !app.world()
                .resource::<ArrivalOpenings>()
                .openings
                .is_empty()
        );
        app.world_mut()
            .spawn((SourceClips::default(), ChildOf(far)));
        step(&mut app, 2);
        assert_eq!(state(&app, far), DoorState::Open { animated: false });
        assert!(
            app.world()
                .resource::<ArrivalOpenings>()
                .openings
                .is_empty()
        );
    }

    // Synthetic fixtures describe clips with Gltf values. Project those values into the same
    // public inventory/binding used by production. Real-loader evidence lives in
    // model_animation/source_readiness_tests.rs and never uses this fixture bridge.
    fn publish_fixture_inventory(
        mut commands: Commands,
        pending: Query<(Entity, &PendingDoorModel)>,
        models: Res<Assets<Gltf>>,
        children: Query<&Children>,
        players: Query<(), With<AnimationPlayer>>,
        inventories: Query<(), With<SourceClips>>,
    ) {
        for (door, pending) in &pending {
            let Some(model) = pending.model.as_ref().and_then(|h| models.get(h)) else {
                continue;
            };
            if children
                .iter_descendants(door)
                .any(|e| inventories.contains(e))
            {
                continue;
            }
            let inventory = SourceClips(
                model
                    .named_animations
                    .iter()
                    .map(|(n, h)| (n.to_string(), h.clone()))
                    .collect(),
            );
            let player = children
                .iter_descendants(door)
                .find(|e| players.contains(*e));
            if let Some(player) = player {
                commands.entity(player).insert(inventory);
            } else if model.animations.is_empty() {
                commands.spawn((inventory, ChildOf(door)));
            } else {
                continue;
            }
            commands
                .entity(door)
                .insert(MeshHandle(pending.path.clone()));
        }
    }

    const UNRESOLVED_OBSERVATION_FRAMES: u32 = 240;

    /// A door app with no window, no renderer and no GPU: the two animation asset collections,
    /// Bevy's own animation ticking, and a clock the test moves by [`STEP_SECONDS`] a frame.
    fn door_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::animation::AnimationPlugin,
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            STEP_SECONDS,
        )))
        .init_asset::<Gltf>()
        .add_plugins(SeamlessDoorAnimationPlugin)
        .add_systems(
            Update,
            publish_fixture_inventory.before(crate::model_animation::resolve_source_clips),
        )
        .add_systems(Update, crate::model_animation::resolve_source_clips);
        app
    }

    /// Reuse the real clip fixture and animation clock; only supply the player's headless inputs.
    fn keyboard_door_app() -> App {
        let mut app = door_app();
        app.init_asset::<Mesh>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<bevy::input::mouse::AccumulatedMouseMotion>()
            .init_resource::<ProfilingState>()
            .insert_resource(crate::config::EngineConfig::run_from_args(vec![
                "--load-doors".into(),
                "seamless".into(),
            ]))
            .add_plugins(crate::player::PlayerPlugin);
        app
    }

    fn prompt_display(app: &mut App) -> Display {
        let world = app.world_mut();
        let mut prompt = world.query_filtered::<&Node, With<crate::demo_hud::DoorPrompt>>();
        prompt
            .single(world)
            .expect("one shared door prompt")
            .display
    }

    #[test]
    fn keyboard_swings_refresh_the_seamless_prompt_before_the_update_ends() {
        let mut app = keyboard_door_app();
        let door = animated_door(&mut app);
        let base = Vec3::new(1000.0, 0.0, 1000.0);
        app.world_mut().entity_mut(door.door).insert((
            Transform::from_translation(base),
            GlobalTransform::from_translation(base),
        ));
        let camera = spawn_camera(&mut app, eye_from_feet(base + Vec3::Z * 200.0));
        app.world_mut().entity_mut(camera).insert(Player {
            mode: crate::player::PlayerMode::Fly,
            ..default()
        });
        app.update();
        assert_eq!(state(&app, door.door), DoorState::Closed);
        assert_eq!(
            app.world()
                .resource::<DoorTarget>()
                .0
                .as_ref()
                .unwrap()
                .door,
            door.door
        );
        assert_eq!(prompt_display(&mut app), Display::Flex);

        assert_eq!(
            app.world()
                .resource::<DoorTarget>()
                .0
                .as_ref()
                .unwrap()
                .prompt,
            crate::doors::open_prompt(&load_door(false).label)
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert_eq!(state(&app, door.door), DoorState::Opening);
        assert!(player(&app, door.player).is_playing_animation(door.open_node));
        assert!(app.world().resource::<DoorTarget>().0.is_none());
        assert_eq!(prompt_display(&mut app), Display::None);

        // Reach Open through the real clip clock, never by inserting the desired state.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        step(&mut app, 12);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        assert_eq!(
            app.world()
                .resource::<DoorTarget>()
                .0
                .as_ref()
                .unwrap()
                .prompt,
            format!("E  Close  {}", load_door(false).label)
        );
        assert_eq!(prompt_display(&mut app), Display::Flex);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert_eq!(state(&app, door.door), DoorState::Closing);
        assert!(player(&app, door.player).is_playing_animation(door.close_node));
        assert!(app.world().resource::<DoorTarget>().0.is_none());
        assert_eq!(prompt_display(&mut app), Display::None);
    }

    #[test]
    fn load_screen_keyboard_crossing_starts_the_real_clip_without_a_resident_destination() {
        use crate::{
            door_crossing::{DoorCrossing, DoorCrossingPlugin},
            physics::CursorCapture,
            streaming::{ActiveSpace, RenderOrigin},
            world::database::CellKey,
        };
        use bevy::ecs::message::Messages;

        let mut app = door_app();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<StreamingWorld>()
            .init_resource::<crate::streaming::StreamingMetrics>()
            .init_resource::<crate::streaming::TerrainContinuity>()
            .init_resource::<ActiveSpace>()
            .insert_resource(RenderOrigin(IVec2::ZERO))
            .insert_resource(CursorCapture::Captured)
            .insert_resource(crate::config::EngineConfig::default())
            .add_plugins(DoorCrossingPlugin);
        let door = animated_door(&mut app);
        app.world_mut().entity_mut(door.door).insert((
            Transform::from_xyz(0.0, 0.0, -150.0),
            GlobalTransform::from_translation(Vec3::new(0.0, 0.0, -150.0)),
        ));
        spawn_camera(&mut app, Vec3::new(0.0, EYE_HEIGHT, 0.0));
        app.update();
        assert!(!app.world().resource::<DoorCrossing>().is_active());
        assert_eq!(
            app.world()
                .resource::<DoorTarget>()
                .0
                .as_ref()
                .unwrap()
                .door,
            door.door
        );
        assert_eq!(prompt_display(&mut app), Display::Flex);
        let mut openings = app
            .world()
            .resource::<Messages<OpenDoor>>()
            .get_cursor_current();

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert!(app.world().resource::<DoorCrossing>().is_active());
        assert_eq!(
            openings
                .read(app.world().resource::<Messages<OpenDoor>>())
                .map(|request| request.door)
                .collect::<Vec<_>>(),
            vec![door.door]
        );
        assert!(app.world().resource::<DoorTarget>().0.is_none());
        assert_eq!(prompt_display(&mut app), Display::None);

        // The crossing writes in PostUpdate; the animation consumer runs in the next Update.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();
        let destination = CellKey::Interior(load_door(false).destination.interior_cell_id.unwrap());
        assert!(
            !app.world()
                .resource::<StreamingWorld>()
                .is_resident(&destination)
        );
        assert_eq!(state(&app, door.door), DoorState::Opening);
        assert!(player(&app, door.player).is_playing_animation(door.open_node));
        let elapsed = player(&app, door.player)
            .animation(door.open_node)
            .unwrap()
            .elapsed();
        assert!(elapsed > 0.0, "Bevy advanced the actual open clip");
        assert!(
            leaf_degrees(&app, door.leaf) > 0.0,
            "Bevy applied the real clip's curve to the leaf"
        );
        assert!(app.world().resource::<PendingDoorOpens>().doors.is_empty());

        // A further keyboard edge during this crossing neither starts another crossing nor swing.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        app.update();
        assert!(app.world().resource::<DoorCrossing>().is_active());
        assert_eq!(
            openings
                .read(app.world().resource::<Messages<OpenDoor>>())
                .count(),
            0
        );
        assert!(
            player(&app, door.player)
                .animation(door.open_node)
                .unwrap()
                .elapsed()
                > elapsed
        );
        assert!(app.world().resource::<DoorTarget>().0.is_none());
        assert_eq!(prompt_display(&mut app), Display::None);
        assert!(
            !app.world()
                .contains_resource::<crate::portal::PortalState>()
        );
        assert!(
            !app.world()
                .contains_resource::<crate::transition::PendingCrossing>()
        );
        let world = app.world_mut();
        assert!(world.query::<&Player>().iter(world).next().is_none());
    }

    /// The `LoadDoor` of a reference with a destination, so the door is one the crossing would use.
    fn load_door(auto_load: bool) -> LoadDoor {
        LoadDoor {
            ref_id: 0x0009_2809,
            destination: DoorDestination {
                destination_ref_id: 0x0006_998D,
                interior_cell_id: Some(7),
                worldspace_id: None,
                arrival_position: [0.0; 3],
                arrival_rotation: [0.0; 3],
            },
            label: "Alftand Great Lift".into(),
            auto_load,
            outward: Some([1.0, 0.0, 0.0]),
        }
    }

    /// A door whose leaf swings well clear of the doorway: the state machine does not care how far
    /// it moves, so this is the fixture for everything but the leaf-visibility tests.
    fn animated_door(app: &mut App) -> Door {
        animated_door_swinging(app, WIDE_SWING_DEGREES)
    }

    /// A door whose model has both clips, with the player, the graph, the transitions and the
    /// moving node the loader and [`attach_door_animations`] would have left behind: the `Open`
    /// clip turns the leaf `swing_degrees` over [`CLIP_SECONDS`], the `Close` clip turns it back,
    /// and the leaf's transform is there for `animate_targets` to write the pose into.
    fn animated_door_swinging(app: &mut App, swing_degrees: f32) -> Door {
        let leaf_target = AnimationTargetId::from_name(&Name::new("Object02"));
        let open = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip_swinging(CLIP_SECONDS, leaf_target, 0.0, swing_degrees));
        let close = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip_swinging(CLIP_SECONDS, leaf_target, swing_degrees, 0.0));
        let (graph, nodes) = AnimationGraph::from_clips([open, close]);
        let graph = app
            .world_mut()
            .resource_mut::<Assets<AnimationGraph>>()
            .add(graph);

        let door = app
            .world_mut()
            .spawn((
                load_door(false),
                DoorState::Closed,
                // Where a spawned reference stands: the auto-close measures the camera's distance
                // from it, and `crate::player`'s doorway volume is built around it.
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id();
        let player = app
            .world_mut()
            .spawn((
                Name::new("DwemerLargeDoorLoad01"),
                AnimationPlayer::default(),
                AnimationGraphHandle(graph.clone()),
                AnimationTransitions::new(),
                ChildOf(door),
            ))
            .id();
        let leaf = app
            .world_mut()
            .spawn((
                Name::new("Object02"),
                leaf_target,
                AnimatedBy(player),
                Transform::default(),
                Visibility::default(),
                DoorLeaf { door },
                ChildOf(player),
            ))
            .id();
        let leaf_mesh = app.world_mut().spawn(ChildOf(leaf)).id();
        // The frame: under the animation root, so the loader gives it an `AnimationTargetId` too,
        // but no clip names it - which is what keeps it out of the leaf marking.
        let frame = app
            .world_mut()
            .spawn((
                Name::new("Plane04"),
                AnimationTargetId::from_name(&Name::new("Plane04")),
                ChildOf(player),
            ))
            .id();
        let frame_mesh = app.world_mut().spawn(ChildOf(frame)).id();

        app.world_mut().entity_mut(door).insert(DoorAnimation {
            player: Some(player),
            open: Some(DoorClip {
                node: nodes[0],
                seconds: CLIP_SECONDS,
            }),
            close: Some(DoorClip {
                node: nodes[1],
                seconds: CLIP_SECONDS,
            }),
            clears_doorway: swing_degrees >= DOORWAY_CLEAR_DEGREES,
        });
        Door {
            door,
            player,
            leaf,
            leaf_mesh,
            frame,
            frame_mesh,
            open_node: nodes[0],
            close_node: nodes[1],
        }
    }

    /// A door whose model has no clips: its state and its resolution are all it has, standing where
    /// a spawned reference stands (the auto-close measures the camera's distance from it).
    fn static_door(app: &mut App) -> Entity {
        app.world_mut()
            .spawn((
                load_door(false),
                DoorState::Closed,
                DoorAnimation::default(),
                Transform::default(),
                GlobalTransform::default(),
            ))
            .id()
    }

    /// A clip of `seconds`, with no curves: enough for the state machine, which reads its length and
    /// the player's own clock.
    fn clip(seconds: f32) -> AnimationClip {
        let mut clip = AnimationClip::default();
        clip.set_duration(seconds);
        clip
    }

    /// A clip of `seconds` that turns `target` from `from_degrees` to `to_degrees` over its length,
    /// as the converter bakes a door's swing: one rotation curve on the node the clip moves.
    fn clip_swinging(
        seconds: f32,
        target: AnimationTargetId,
        from_degrees: f32,
        to_degrees: f32,
    ) -> AnimationClip {
        let mut clip = AnimationClip::default();
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(
                animated_field!(Transform::rotation),
                AnimatableKeyframeCurve::new([
                    (0.0, Quat::from_rotation_y(from_degrees.to_radians())),
                    (seconds, Quat::from_rotation_y(to_degrees.to_radians())),
                ])
                .expect("two keys at different times"),
            ),
        );
        clip
    }

    /// A door and the scene of a model whose `Open` clip turns one node by `swing_degrees`, spawned
    /// the way the loader spawns it: a player on the animation root, an `AnimationTargetId` on every
    /// node under it (the loader gives one to the whole animation-root subtree, frame included), and
    /// the moving node's mesh primitive under that.
    fn door_with_model(app: &mut App, swing_degrees: f32) -> ModelDoor {
        let moved_target = AnimationTargetId::from_name(&Name::new("Object02"));
        let open = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip_swinging(
                CLIP_SECONDS,
                moved_target,
                0.0,
                swing_degrees,
            ));
        let model = app.world_mut().resource_mut::<Assets<Gltf>>().add(gltf(
            std::slice::from_ref(&open),
            &[(OPEN_CLIP, open.clone())],
        ));

        let door = app
            .world_mut()
            .spawn((
                load_door(false),
                DoorState::Closed,
                // The path a reference's model was converted to, which is where `streaming.rs` will
                // have asked for the scene: a door re-claimed after a lost player looks its model up
                // through it.
                crate::world::components::MeshHandle(PLAIN_MODEL_PATH.to_owned()),
                pending(model),
            ))
            .id();
        let player = app
            .world_mut()
            .spawn((
                Name::new("DwemerLargeDoorLoad01"),
                AnimationPlayer::default(),
                AnimationTargetId::from_name(&Name::new("DwemerLargeDoorLoad01")),
                ChildOf(door),
            ))
            .id();
        let moved = app
            .world_mut()
            .spawn((
                Name::new("Object02"),
                moved_target,
                AnimatedBy(player),
                Transform::default(),
                Visibility::default(),
                ChildOf(player),
            ))
            .id();
        let moved_mesh = app.world_mut().spawn(ChildOf(moved)).id();
        let frame = app
            .world_mut()
            .spawn((
                Name::new("Plane04"),
                AnimationTargetId::from_name(&Name::new("Plane04")),
                ChildOf(player),
            ))
            .id();
        ModelDoor {
            door,
            player,
            moved,
            moved_mesh,
            frame,
        }
    }

    /// A door waiting for its model, the way `request_door_models` leaves it - with no twin chosen
    /// yet, since a twin is only looked up once the door's own clip has turned out not to clear the
    /// doorway. The path decides whether there is one to look for: no `load` marker in it, no twin.
    fn pending(model: Handle<Gltf>) -> PendingDoorModel {
        PendingDoorModel {
            binding: None,
            model: Some(model),
            path: PLAIN_MODEL_PATH.to_owned(),
            twin: None,
            waiting: 0,
        }
    }

    /// A `Gltf` asset with no scenes, meshes or nodes - the test only needs its animations, which
    /// are the handles the loader names from the glTF document.
    fn gltf(clips: &[Handle<AnimationClip>], named: &[(&str, Handle<AnimationClip>)]) -> Gltf {
        Gltf {
            scenes: Vec::new(),
            named_scenes: Default::default(),
            meshes: Vec::new(),
            named_meshes: Default::default(),
            materials: Vec::new(),
            named_materials: Default::default(),
            nodes: Vec::new(),
            named_nodes: Default::default(),
            skins: Vec::new(),
            named_skins: Default::default(),
            default_scene: None,
            animations: clips.to_vec(),
            named_animations: named
                .iter()
                .map(|(name, clip)| (Box::from(*name), clip.clone()))
                .collect(),
            source: None,
        }
    }

    /// One frame with the `E` press for `door` in it, written before the frame runs the way the
    /// player's `E` and the demo tour's walk-through write it (the tour presses the key, the
    /// controller writes this message).
    fn activate(app: &mut App, door: Entity) {
        app.world_mut().write_message(OpenDoor { door });
        app.update();
    }

    /// `frames` frames, each of [`STEP_SECONDS`] of clip time.
    fn step(app: &mut App, frames: u32) {
        for _ in 0..frames {
            app.update();
        }
    }

    /// The streaming camera the systems that measure from the player read, where it stands. Its
    /// `Transform` is the eye, which is what `crate::player` reads too (the walk moved it earlier
    /// in the same `Update`, and `GlobalTransform` is only propagated in `PostUpdate`).
    fn spawn_camera(app: &mut App, eye: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_translation(eye),
                GlobalTransform::from_translation(eye),
                StreamingCamera,
            ))
            .id()
    }

    fn state(app: &App, door: Entity) -> DoorState {
        *app.world()
            .get::<DoorState>(door)
            .expect("a load door carries its state")
    }

    fn player(app: &App, door: Entity) -> &AnimationPlayer {
        app.world()
            .get::<AnimationPlayer>(door)
            .expect("the door's player")
    }

    /// How far the leaf is swung, in degrees, read off the transform `animate_targets` wrote for it:
    /// the yaw the fixture's clips turn it by.
    fn leaf_degrees(app: &App, leaf: Entity) -> f32 {
        let rotation = app
            .world()
            .get::<Transform>(leaf)
            .expect("the leaf's transform")
            .rotation;
        let (yaw, _, _) = rotation.to_euler(EulerRot::YXZ);
        yaw.to_degrees().abs()
    }

    fn leaf_visibility(app: &App, leaf: Entity) -> Visibility {
        *app.world()
            .get::<Visibility>(leaf)
            .expect("the leaf's visibility")
    }

    /// A door whose scene is instanced again after its animation was attached - the loader does it
    /// once the door's own model has loaded - has lost the `AnimationPlayer` it recorded. Kept, that
    /// animation cannot play, and `activate_doors` hides the whole door instead of swinging it: every
    /// animated door in play vanished (the user, 2026-09-24). A closed door drops it, so it is
    /// attached again on the scene that is there now.
    #[test]
    fn a_door_that_lost_its_animation_player_is_given_its_animation_again() {
        let mut app = door_app();
        let model = door_with_model(&mut app, 90.0);
        step(&mut app, 2);
        assert!(
            app.world()
                .get::<DoorAnimation>(model.door)
                .is_some_and(|animation| animation.player == Some(model.player)),
            "the fixture's animation is attached to its player"
        );

        app.world_mut()
            .entity_mut(model.player)
            .remove::<AnimationPlayer>();
        step(&mut app, 1);
        assert!(
            app.world().get::<DoorAnimation>(model.door).is_none(),
            "a closed door whose player is gone gives up the stale animation, to be attached again"
        );
        assert!(
            app.world().get::<DoorState>(model.door) == Some(&DoorState::Closed),
            "and it is still the closed door it was"
        );
        assert!(
            app.world().get::<ReattachPose>(model.door).is_none(),
            "a closed door has no pose to be put back at"
        );

        // The frame after, the scene the loader would have instanced has its own player, and the
        // animation is attached again on it.
        app.world_mut()
            .entity_mut(model.player)
            .insert(AnimationPlayer::default());
        step(&mut app, 2);
        let animation = *app
            .world()
            .get::<DoorAnimation>(model.door)
            .expect("the animation is attached again");
        assert_eq!(
            animation.player,
            Some(model.player),
            "on the player the scene that is there now spawned"
        );
        assert_eq!(state(&app, model.door), DoorState::Closed);
    }

    /// The other half of flake B: the door's own model is loaded by `streaming.rs` for the scene it
    /// draws, and the loader makes the root `Gltf` asset late in that load. A door that asks the
    /// asset server for the file itself inside that window starts a *second* load of it, and a
    /// second load re-inserts the file's scene as `Modified` - which re-instances every drawn copy
    /// of the model and takes the `AnimationPlayer` out from under every door of it
    /// (research-229). Nothing is loaded at all while the model is not there: the request is what
    /// would be the second load.
    #[test]
    fn a_door_never_asks_for_a_model_the_streaming_load_has_not_made_yet() {
        let mut app = door_app();
        let door = app
            .world_mut()
            .spawn((
                load_door(false),
                crate::world::components::MeshHandle(PLAIN_MODEL_PATH.to_owned()),
            ))
            .id();
        // The load that would be `streaming.rs`'s is asked for by nobody here: the path has no
        // handle of any kind. On the frame the door is claimed it would have one - the asset server
        // creates a handle for a path the moment it is asked for it - so this is the assertion that
        // fails on an engine that asks.
        let asset_server = app.world().resource::<AssetServer>();
        assert!(
            asset_server
                .get_path_id(PLAIN_MODEL_PATH.to_owned())
                .is_none(),
            "the fixture's model path starts with no handle at all"
        );
        step(&mut app, 5);
        assert!(
            app.world().get::<PendingDoorModel>(door).is_some(),
            "the door is claimed and waiting"
        );
        assert!(
            app.world()
                .resource::<AssetServer>()
                .get_path_id(PLAIN_MODEL_PATH.to_owned())
                .is_none(),
            "the door asked for a model nothing is loading: that request is the second load"
        );
    }

    /// The flake this fixes: a door whose scene is re-instanced while it is **open** used to come
    /// back closed - the leaf stood at its rest pose across a doorway that was open, the state said
    /// `Closed`, and the `Close` clip could no longer be played on the dead player, so the door
    /// stayed that way (research-229, flake B). The door is put back in the state it was in, with
    /// its `Open` clip started at the pose that state holds, and its leaf leaves the opening again.
    #[test]
    fn a_door_that_lost_its_player_while_open_comes_back_open() {
        let mut app = door_app();
        let model = door_with_model(&mut app, WIDE_SWING_DEGREES);
        step(&mut app, 2);
        activate(&mut app, model.door);
        step(&mut app, 12);
        assert_eq!(
            state(&app, model.door),
            DoorState::Open { animated: true },
            "the fixture's door is open"
        );
        let open_degrees = leaf_degrees(&app, model.moved);
        assert!(
            open_degrees > WIDE_SWING_DEGREES / 2.0,
            "its leaf is out of the doorway ({open_degrees} degrees)"
        );

        // The scene is instanced again: the player the door recorded is gone, and its leaf is back
        // at the rest pose.
        app.world_mut()
            .entity_mut(model.player)
            .remove::<AnimationPlayer>();
        app.world_mut()
            .entity_mut(model.moved)
            .insert(Transform::default());
        step(&mut app, 1);
        assert!(app.world().get::<ReattachPose>(model.door).is_some());

        // The frame after, the scene the loader would have instanced has its own player.
        app.world_mut()
            .entity_mut(model.player)
            .insert(AnimationPlayer::default());
        step(&mut app, 2);
        assert_eq!(
            state(&app, model.door),
            DoorState::Open { animated: true },
            "the door is open again, not closed"
        );
        assert!(
            app.world().get::<ReattachPose>(model.door).is_none(),
            "and it has been put back"
        );
        let degrees = leaf_degrees(&app, model.moved);
        assert!(
            degrees > WIDE_SWING_DEGREES / 2.0,
            "the leaf is out of the doorway again ({degrees} degrees), not at its rest pose"
        );
    }

    /// A door lost on its way **shut** is the other half of the same rule: the leaf is at the rest
    /// pose the close was heading for, so the door is shut and stays shut - it does not swing open
    /// again over a player who asked it to close.
    #[test]
    fn a_door_that_lost_its_player_while_closing_comes_back_closed() {
        let mut app = door_app();
        let model = door_with_model(&mut app, WIDE_SWING_DEGREES);
        step(&mut app, 2);
        // The state a close leaves the door in, handed to it rather than played out: the close
        // itself is `reactivating_an_open_door_closes_it`'s coverage, and this is about what the
        // loss does with a door in it.
        app.world_mut()
            .entity_mut(model.door)
            .insert(DoorState::Closing);
        app.world_mut()
            .entity_mut(model.player)
            .remove::<AnimationPlayer>();
        step(&mut app, 1);
        assert_eq!(
            app.world().get::<ReattachPose>(model.door),
            Some(&ReattachPose(DoorState::Closed)),
            "a door on its way shut is put back shut"
        );

        app.world_mut()
            .entity_mut(model.player)
            .insert(AnimationPlayer::default());
        step(&mut app, 2);
        assert_eq!(state(&app, model.door), DoorState::Closed);
    }

    /// The one read surface the demo tour's swing check needs: a door whose model has an `Open`
    /// clip answers [`DoorAnimation::swings`], and a static leaf does not. It is what tells a door
    /// that swings when opened from one that opens with its whole model hidden
    /// (`docs/research/checks-for-user-found-defects.md`, defect 1).
    #[test]
    fn a_door_swings_only_when_its_model_has_an_open_clip() {
        let mut app = door_app();
        let animated = animated_door(&mut app);
        let still = static_door(&mut app);
        let swings = |app: &App, door: Entity| {
            app.world()
                .get::<DoorAnimation>(door)
                .expect("a resolved door carries its animation")
                .swings()
        };

        assert!(
            swings(&app, animated.door),
            "a door whose model has an `Open` clip swings when it is opened"
        );
        assert!(
            !swings(&app, still),
            "a static leaf has no clip to swing with: an open one is a hole"
        );
    }

    #[test]
    fn activating_a_closed_door_starts_it_opening() {
        let mut app = door_app();
        let door = animated_door(&mut app);
        assert_eq!(state(&app, door.door), DoorState::Closed);

        activate(&mut app, door.door);

        assert_eq!(state(&app, door.door), DoorState::Opening);
        assert!(
            player(&app, door.player).is_playing_animation(door.open_node),
            "the open clip is the one being played"
        );
        assert!(
            !player(&app, door.player).is_playing_animation(door.close_node),
            "and the close clip is not"
        );
    }

    #[test]
    fn the_door_is_open_once_the_clip_reaches_the_open_fraction() {
        let mut app = door_app();
        let door = animated_door(&mut app);
        activate(&mut app, door.door);

        // Four frames more: the clip is 0.4 s into its second, just short of the mark. (The clock
        // lives in `advance_animations`, which runs in `PostUpdate` after this module has looked -
        // and the app's first frame plays no clip time at all - so a frame lags the clip's own
        // time by one.)
        step(&mut app, 4);
        let elapsed = player(&app, door.player)
            .animation(door.open_node)
            .expect("the open clip is playing")
            .elapsed();
        assert!(
            elapsed < OPEN_FRACTION * CLIP_SECONDS,
            "{elapsed} is short of the mark"
        );
        assert_eq!(state(&app, door.door), DoorState::Opening);

        // Two more: the clip passes the mark, and is still playing - it runs to its end and holds
        // there.
        step(&mut app, 2);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        assert!(
            player(&app, door.player).is_playing_animation(door.open_node),
            "the open clip keeps playing past the mark"
        );
    }

    #[test]
    fn a_clip_that_ran_to_its_end_leaves_the_door_open() {
        let mut app = door_app();
        let door = animated_door(&mut app);
        activate(&mut app, door.door);
        step(&mut app, 12);

        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        assert!(
            player(&app, door.player)
                .animation(door.open_node)
                .unwrap()
                .is_finished(),
            "the clip has run out, holding its last key"
        );

        step(&mut app, 5);
        assert_eq!(
            state(&app, door.door),
            DoorState::Open { animated: true },
            "a finished clip does not close the door"
        );
    }

    #[test]
    fn a_door_with_no_clips_is_open_in_the_frame_it_is_activated() {
        let mut app = door_app();
        let door = static_door(&mut app);
        assert_eq!(state(&app, door), DoorState::Closed);

        activate(&mut app, door);

        assert_eq!(state(&app, door), DoorState::Open { animated: false });
    }

    /// A door whose model has no clips at all closes in the frame it is activated again: there is
    /// no leaf to swing and no clip to run, so the doorway shuts - the mirror of its one-frame
    /// opening, and the only close a static door has.
    #[test]
    fn a_door_with_no_clips_closes_in_the_frame_it_is_activated_again() {
        let mut app = door_app();
        let door = static_door(&mut app);
        activate(&mut app, door);
        assert_eq!(state(&app, door), DoorState::Open { animated: false });

        activate(&mut app, door);

        assert_eq!(
            state(&app, door),
            DoorState::Closed,
            "the doorway shuts again"
        );
    }

    /// What `E` at an open door does is decided by whether the model has anything to close with: a
    /// `Close` clip, or no clip at all (the one-frame shut above) - but not an `Open` clip on its
    /// own, which is a door the state machine only ever replays the opening of.
    #[test]
    fn a_door_can_close_with_a_close_clip_or_with_no_clips_at_all() {
        let mut app = door_app();
        let animated = animated_door(&mut app);
        let with_both = *app
            .world()
            .get::<DoorAnimation>(animated.door)
            .expect("the fixture's animation");
        assert!(
            with_both.can_close(),
            "a `Close` clip is something to close with"
        );

        let mut open_only = with_both;
        open_only.close = None;
        assert!(
            !open_only.can_close(),
            "an `Open` clip with no `Close` replays its opening instead of closing"
        );

        assert!(
            DoorAnimation::default().can_close(),
            "no clips at all is the one-frame shut"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // An opening waits for the space behind the door
    // ---------------------------------------------------------------------------------------------

    /// A closed load door whose destination is not streamed in is not opened: the swing is held,
    /// and starts the frame the space behind the door is there.
    ///
    /// The far side arriving is driven here by taking the streamer away, which is the answer
    /// [`destination_is_loaded`] gives a run with no streaming at all - everything is loaded - and
    /// the one way a test with no world database can turn that answer round. The real thing, a cell
    /// that streams in while the player walks up to the door, is driven end to end against the real
    /// streamer in `crate::transition`'s `a_door_waits_for_its_destination_and_opens_when_it_arrives`.
    #[test]
    fn load_screen_waits_for_source_clip_resolution_without_opening_a_static_standin() {
        let mut app = door_app();
        app.insert_resource(crate::config::EngineConfig::default());
        app.init_resource::<StreamingWorld>();
        let door = door_with_model(&mut app, WIDE_SWING_DEGREES);
        let model = app
            .world()
            .get::<PendingDoorModel>(door.door)
            .unwrap()
            .model
            .clone()
            .unwrap();
        let asset = app
            .world_mut()
            .resource_mut::<Assets<Gltf>>()
            .remove(model.id())
            .unwrap();
        activate(&mut app, door.door);
        assert_eq!(state(&app, door.door), DoorState::Closed);
        assert_eq!(
            app.world().resource::<PendingDoorOpens>().doors,
            vec![door.door]
        );
        assert!(app.world().get::<DoorAnimation>(door.door).is_none());
        app.world_mut()
            .resource_mut::<Assets<Gltf>>()
            .insert(model.id(), asset)
            .unwrap();
        step(&mut app, 1);
        assert_eq!(state(&app, door.door), DoorState::Opening);
        let animation = app.world().get::<DoorAnimation>(door.door).unwrap();
        assert_eq!(animation.player, Some(door.player));
        assert!(player(&app, door.player).is_playing_animation(animation.open.unwrap().node));
    }

    #[test]
    fn load_screen_animation_starts_without_prestream_but_seamless_keeps_waiting() {
        for seamless in [false, true] {
            let mut app = door_app();
            let args: Vec<String> = if seamless {
                vec!["--load-doors".into(), "seamless".into()]
            } else {
                vec![]
            };
            app.insert_resource(crate::config::EngineConfig::run_from_args(args));
            app.init_resource::<StreamingWorld>();
            let door = animated_door(&mut app);
            activate(&mut app, door.door);
            assert_eq!(
                state(&app, door.door),
                if seamless {
                    DoorState::Closed
                } else {
                    DoorState::Opening
                }
            );
            assert_eq!(
                player(&app, door.player).is_playing_animation(door.open_node),
                !seamless
            );
            assert_eq!(
                app.world().resource::<PendingDoorOpens>().doors.is_empty(),
                !seamless
            );
        }
    }

    #[test]
    fn an_opening_waits_for_the_space_behind_the_door() {
        let mut app = door_app();
        // The streamer is there - this is not the no-streaming case - and no cell of this app is
        // resident, which is every cell the fixture's link could lead to.
        app.init_resource::<StreamingWorld>();
        let door = animated_door(&mut app);
        assert_eq!(
            load_door(false).destination.interior_cell_id,
            Some(7),
            "the fixture's door does lead somewhere"
        );

        activate(&mut app, door.door);

        assert_eq!(
            state(&app, door.door),
            DoorState::Closed,
            "a door that swung here would swing onto a hole"
        );
        assert!(
            !player(&app, door.player).is_playing_animation(door.open_node),
            "and its clip never started"
        );
        assert_eq!(
            app.world().resource::<PendingDoorOpens>().doors,
            vec![door.door],
            "the request waits for the space behind the door"
        );

        // The far side is there now: the door opens in that same frame, and is not held any more.
        app.world_mut().remove_resource::<StreamingWorld>();
        step(&mut app, 1);

        assert_eq!(state(&app, door.door), DoorState::Opening);
        assert!(player(&app, door.player).is_playing_animation(door.open_node));
        assert!(
            app.world().resource::<PendingDoorOpens>().doors.is_empty(),
            "the swing is under way, so nothing is waiting"
        );
    }

    /// A second `E` at a door whose opening is waiting cancels it - the player asked twice, and the
    /// door is not coming - and nothing opens for either press. The next press asks again.
    #[test]
    fn a_second_press_cancels_a_held_opening() {
        let mut app = door_app();
        app.init_resource::<StreamingWorld>();
        let door = animated_door(&mut app);
        activate(&mut app, door.door);
        assert_eq!(
            app.world().resource::<PendingDoorOpens>().doors,
            vec![door.door]
        );

        activate(&mut app, door.door);

        assert!(
            app.world().resource::<PendingDoorOpens>().doors.is_empty(),
            "the second press cancels the wait"
        );
        assert_eq!(
            state(&app, door.door),
            DoorState::Closed,
            "and neither press opened the door"
        );

        activate(&mut app, door.door);
        assert_eq!(
            app.world().resource::<PendingDoorOpens>().doors,
            vec![door.door],
            "the door is still one the player wants open"
        );
    }

    /// A held opening whose door is gone by the time its far side arrives is dropped: there is
    /// nothing left to open.
    #[test]
    fn a_held_opening_is_dropped_when_its_door_is_gone() {
        let mut app = door_app();
        app.init_resource::<StreamingWorld>();
        let door = animated_door(&mut app);
        activate(&mut app, door.door);
        assert_eq!(app.world().resource::<PendingDoorOpens>().doors.len(), 1);

        app.world_mut().entity_mut(door.door).despawn();
        step(&mut app, 1);

        assert!(
            app.world().resource::<PendingDoorOpens>().doors.is_empty(),
            "the door is not there to open any more"
        );
    }

    /// A door with no destination at all opens at once, however empty the streamer is: there is
    /// nothing behind it to wait for.
    #[test]
    fn a_door_with_no_destination_opens_at_once() {
        let mut app = door_app();
        app.init_resource::<StreamingWorld>();
        let mut row = load_door(false);
        row.destination.interior_cell_id = None;
        let door = app
            .world_mut()
            .spawn((row, DoorState::Closed, DoorAnimation::default()))
            .id();

        activate(&mut app, door);

        assert_eq!(
            state(&app, door),
            DoorState::Open { animated: false },
            "a door that leads nowhere has no far side to wait for"
        );
        assert!(app.world().resource::<PendingDoorOpens>().doors.is_empty());
    }

    // ---------------------------------------------------------------------------------------------
    // A door left open closes itself
    // ---------------------------------------------------------------------------------------------

    /// A door the player opened and walked away from asks for its close through the same message
    /// `E` writes, so the state machine runs the `Close` clip from the frame the camera is
    /// [`AUTO_CLOSE_DISTANCE`] away - and a door the player is still standing near is left open.
    #[test]
    fn a_door_left_open_closes_when_the_player_walks_away() {
        let mut app = door_app();
        let door = animated_door(&mut app);
        let camera = spawn_camera(
            &mut app,
            Vec3::new(0.0, 0.0, -(AUTO_CLOSE_DISTANCE - 100.0)),
        );
        activate(&mut app, door.door);
        step(&mut app, 12);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });

        step(&mut app, 4);
        assert_eq!(
            state(&app, door.door),
            DoorState::Open { animated: true },
            "a door the player is near is left open"
        );

        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .expect("the fixture camera's transform")
            .translation = Vec3::new(0.0, 0.0, -(AUTO_CLOSE_DISTANCE + 1.0));
        step(&mut app, 1);

        assert_eq!(
            state(&app, door.door),
            DoorState::Closing,
            "walked away from: the close starts the frame the camera is past the distance"
        );
        assert!(
            player(&app, door.player).is_playing_animation(door.close_node),
            "and it is the door's own `Close` clip that plays, the one `E` would run"
        );
    }

    /// Current local poses deliberately disagree with stored GlobalTransforms, as they
    /// do between an exterior origin change and PostUpdate propagation.
    fn current_pose_auto_close_fixture() -> (App, Entity, Entity, Entity, Entity) {
        let mut app = door_app();
        let near = static_door(&mut app);
        let far = static_door(&mut app);
        let far_row = far_row(&app, near);
        let distance = AUTO_CLOSE_DISTANCE;
        let far_parent = app
            .world_mut()
            .spawn((
                Transform::from_xyz(distance * 8.0, 0.0, 0.0),
                GlobalTransform::from_translation(Vec3::X * distance * 8.0),
            ))
            .id();
        app.world_mut().entity_mut(near).insert((
            DoorState::Open { animated: false },
            Transform::from_xyz(distance * 5.0, 0.0, 0.0),
            GlobalTransform::from_translation(Vec3::X * distance * 5.0),
        ));
        app.world_mut().entity_mut(far).insert((
            far_row,
            DoorState::Open { animated: false },
            Transform::from_xyz(distance, 0.0, 0.0),
            GlobalTransform::from_translation(Vec3::X * distance * 9.0),
            ChildOf(far_parent),
        ));
        let camera_parent = app
            .world_mut()
            .spawn((
                Transform::from_xyz(distance * 5.0, 0.0, 0.0),
                GlobalTransform::from_translation(Vec3::X * distance * 5.0),
            ))
            .id();
        app.world_mut().spawn((
            StreamingCamera,
            Transform::from_xyz(0.0, EYE_HEIGHT, 0.0),
            GlobalTransform::from_translation(Vec3::new(distance * 5.0, EYE_HEIGHT, 0.0)),
            ChildOf(camera_parent),
        ));
        app.world_mut()
            .resource_mut::<FarDoorDrive>()
            .pairs
            .push(DrivenPair {
                near,
                far,
                near_open: Some(true),
                layered: false,
            });
        // Same-frame local/ancestor changes, without a propagation pass. Far is now
        // at world zero and the parent's camera is only 20 units across from it.
        app.world_mut()
            .entity_mut(far_parent)
            .insert(Transform::from_xyz(-distance * 2.0, 0.0, 0.0));
        app.world_mut()
            .entity_mut(far)
            .insert(Transform::from_xyz(distance * 2.0, 0.0, 0.0));
        app.world_mut()
            .entity_mut(camera_parent)
            .insert(Transform::from_xyz(20.0, 0.0, 0.0));
        (app, near, far, far_parent, camera_parent)
    }

    #[test]
    fn auto_close_uses_current_camera_and_partner_hierarchies_before_propagation() {
        let (mut app, near, far, _, camera_parent) = current_pose_auto_close_fixture();
        let stale_far = *app.world().get::<GlobalTransform>(far).unwrap();
        assert!(stale_far.translation().x > AUTO_CLOSE_DISTANCE);
        step(&mut app, 1);
        assert_eq!(
            state(&app, near),
            DoorState::Open { animated: false },
            "the driver must remain open because its current partner is near the current eye"
        );
        assert_eq!(state(&app, far), DoorState::Open { animated: false });
        assert_eq!(
            *app.world().get::<GlobalTransform>(far).unwrap(),
            stale_far,
            "fixture does not accidentally repair the stale globals by propagating transforms"
        );
        app.world_mut()
            .entity_mut(camera_parent)
            .insert(Transform::from_xyz(AUTO_CLOSE_DISTANCE * 16.0, 0.0, 0.0));
        step(&mut app, 1);
        assert_eq!(
            state(&app, near),
            DoorState::Closed,
            "moving the parented eye away from both ends still uses the ordinary activation close"
        );
        assert_eq!(
            state(&app, far),
            DoorState::Closed,
            "the established far-door drive still follows the driver's close"
        );
    }

    #[test]
    fn auto_close_does_not_use_stale_globals_when_a_current_hierarchy_is_unknown() {
        for missing_camera_pose in [true, false] {
            let (mut app, near, far, far_parent, camera_parent) = current_pose_auto_close_fixture();
            app.world_mut()
                .entity_mut(camera_parent)
                .insert(Transform::from_xyz(AUTO_CLOSE_DISTANCE * 16.0, 0.0, 0.0));
            app.world_mut()
                .entity_mut(if missing_camera_pose {
                    camera_parent
                } else {
                    far_parent
                })
                .remove::<Transform>();
            step(&mut app, 1);
            assert_eq!(
                state(&app, near),
                DoorState::Open { animated: false },
                "unknown current camera/partner hierarchy cannot authorize optional auto-close"
            );
            assert_eq!(state(&app, far), DoorState::Open { animated: false });
        }
    }

    /// A model with an `Open` clip and no `Close` one is never closed by anything: activating it
    /// replays the opening (design section 5), and at a distance that is a swing played again for
    /// nobody rather than a door shutting.
    #[test]
    fn a_door_whose_model_has_no_close_clip_is_never_closed_by_itself() {
        let mut app = door_app();
        let door = animated_door(&mut app);
        let mut animation = *app
            .world()
            .get::<DoorAnimation>(door.door)
            .expect("the fixture's animation");
        animation.close = None;
        app.world_mut().entity_mut(door.door).insert(animation);
        spawn_camera(
            &mut app,
            Vec3::new(0.0, 0.0, -(AUTO_CLOSE_DISTANCE + 100.0)),
        );

        activate(&mut app, door.door);
        step(&mut app, 12);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });

        step(&mut app, 4);

        assert_eq!(
            state(&app, door.door),
            DoorState::Open { animated: true },
            "there is nothing to close this door with"
        );
        assert!(
            player(&app, door.player)
                .animation(door.open_node)
                .is_some_and(|animation| animation.is_finished()),
            "and its opening was not replayed from the start at a distance"
        );
    }

    /// A door whose model has no clips at all - a static leaf - is part of the same rule: an open
    /// one a walk away from the player shuts in a single frame, which is the one-frame close it
    /// gets from `E` too, and at a distance nothing is there to see.
    #[test]
    fn a_static_door_is_closed_by_itself_in_one_frame() {
        let mut app = door_app();
        let door = static_door(&mut app);
        spawn_camera(
            &mut app,
            Vec3::new(0.0, 0.0, -(AUTO_CLOSE_DISTANCE + 100.0)),
        );

        activate(&mut app, door);
        assert_eq!(state(&app, door), DoorState::Open { animated: false });

        step(&mut app, 1);

        assert_eq!(
            state(&app, door),
            DoorState::Closed,
            "the doorway shuts again, as it does when `E` is pressed at it"
        );
    }

    /// The one door the distance cannot speak for: a player **inside** the doorway. A doorway box
    /// is 60 units deep however wide it is (`AUTO_DOOR_TRIGGER_DEPTH`), so a door whose model
    /// measures its opening far from the reference is one the camera can be
    /// [`AUTO_CLOSE_DISTANCE`] away from and still standing in - and there the door stays open, the
    /// same refusal `E` makes at a door the player stands in.
    #[test]
    fn a_door_is_not_shut_on_a_player_standing_in_its_doorway() {
        let mut app = door_app();
        let door = animated_door(&mut app);
        // A doorway standing 600 units along +X of its own reference: the camera is 611.9 units
        // from the reference - past the distance - and inside the opening.
        app.world_mut()
            .entity_mut(door.door)
            .insert(InstanceBounds {
                min: Vec3::new(450.0, -100.0, -30.0),
                max: Vec3::new(750.0, 100.0, 30.0),
            });
        spawn_camera(&mut app, Vec3::new(600.0, EYE_HEIGHT, 0.0));

        activate(&mut app, door.door);
        step(&mut app, 12);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });

        step(&mut app, 4);

        assert_eq!(
            state(&app, door.door),
            DoorState::Open { animated: true },
            "the player is standing in the doorway: pulling the leaf through them would shut them in"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // The far door of a crossing
    // ---------------------------------------------------------------------------------------------

    /// The reference of the door at the far end of `source`'s link: what
    /// [`OpenDestinationDoor`] names and the far door is found by.
    fn far_ref(app: &App, source: Entity) -> u32 {
        app.world()
            .get::<LoadDoor>(source)
            .expect("the source door's link")
            .destination
            .destination_ref_id
    }

    /// The far door of the link as the destination cell spawns it: the reference [`far_ref`] names.
    fn far_row(app: &App, source: Entity) -> LoadDoor {
        let mut row = load_door(false);
        row.ref_id = far_ref(app, source);
        row
    }

    /// The far door of the link, with a swing of its own.
    fn spawn_far_animated_door(app: &mut App, source: Entity) -> Door {
        let far = animated_door(app);
        let row = far_row(app, source);
        app.world_mut().entity_mut(far.door).insert(row);
        far
    }

    /// The far door of the link for a model with nothing to animate.
    fn spawn_far_static_door(app: &mut App, source: Entity) -> Entity {
        let row = far_row(app, source);
        app.world_mut()
            .spawn((row, DoorState::Closed, DoorAnimation::default()))
            .id()
    }

    /// The crossing of `source`'s doorway: the [`OpenDestinationDoor`] `crate::transition` writes
    /// the frame the player's feet reach the doorway's plane.
    fn cross(app: &mut App, source: Entity) {
        let destination_ref_id = far_ref(app, source);
        app.world_mut().write_message(OpenDestinationDoor {
            door: source,
            destination_ref_id,
        });
        app.update();
    }

    /// How far through its `Open` clip a door's swing has got: the pose the window shows in the
    /// doorway it is drawn in, which is the animation's own seek time (`animate_targets` evaluates
    /// every curve there) and not its elapsed time - a clip a crossing seeked into would read as
    /// barely started.
    fn swing_fraction(app: &App, door: &Door) -> Option<f32> {
        let animation = player(app, door.player).animation(door.open_node)?;
        Some((animation.seek_time() / CLIP_SECONDS).clamp(0.0, 1.0))
    }

    /// A mapped crossing through an anchored door arrives **in the destination doorway**, which is
    /// where the far door of the link stands. Through the source doorway that door was out of the
    /// way, so it has to be out of the way in the frame the player arrives too - at the pose the
    /// window left it: the point of its own `Open` clip the source door's clip had reached. A door
    /// swung from its rest pose there would swing *through* the player, and a door left closed is the
    /// back of a leaf at point-blank range with the walk out of the doorway blocked behind it.
    #[test]
    fn a_mapped_crossing_opens_the_far_door_where_the_window_left_it() {
        let mut app = door_app();
        let source = animated_door(&mut app);
        let far = spawn_far_animated_door(&mut app, source.door);
        // `E` and four frames of the swing: the doorway is a way through
        // ([`DoorState::is_open`]) with the clip still short of the mark the state calls open, so
        // the window is drawing a half-open door in the destination doorway.
        activate(&mut app, source.door);
        step(&mut app, 4);
        let showed = swing_fraction(&app, &source).expect("the source door is swinging");
        assert!(showed < OPEN_FRACTION, "the swing is under way: {showed}");
        assert_eq!(
            state(&app, far.door),
            DoorState::Closed,
            "and the far door is shut to begin with"
        );

        cross(&mut app, source.door);

        assert_eq!(
            state(&app, far.door),
            DoorState::Opening,
            "the far door arrives where the window left it, mid-swing rather than shut"
        );
        let arrived = swing_fraction(&app, &far).expect("the far door's clip is playing");
        assert!(
            (arrived - showed).abs() <= 2.0 * STEP_SECONDS,
            "the far door picks its swing up at {showed}, the point the window showed, rather than \
             at the rest pose: {arrived}"
        );
        assert!(
            probe_skips(&mut app, far.leaf_mesh),
            "so the player's landing is not inside the far door's leaf: the doorway is one to walk \
             out of"
        );

        // And its swing carries on from there: it is `Open` the frame the clip passes the mark,
        // exactly as a door the player opened themselves is.
        step(&mut app, 3);
        assert_eq!(state(&app, far.door), DoorState::Open { animated: true });
    }

    /// The ordinary arrival: the door the player is walking through has stood open since they asked
    /// for it, so the window showed a doorway with nothing in it and the far door is simply open -
    /// put at the end of its own clip, not swung through the player.
    #[test]
    fn a_crossing_through_an_open_door_arrives_at_an_open_far_door() {
        let mut app = door_app();
        let source = animated_door(&mut app);
        let far = spawn_far_animated_door(&mut app, source.door);
        activate(&mut app, source.door);
        step(&mut app, 12);
        assert_eq!(
            state(&app, source.door),
            DoorState::Open { animated: true },
            "the door the player walks through has stood open"
        );

        cross(&mut app, source.door);

        assert_eq!(
            state(&app, far.door),
            DoorState::Open { animated: true },
            "the far door arrives open"
        );
        assert_eq!(
            swing_fraction(&app, &far),
            Some(1.0),
            "at the end of its own clip, the pose the open source door's mirror was drawn in"
        );
        assert!(
            probe_skips(&mut app, far.leaf_mesh),
            "and its leaf is out of the doorway the player lands in"
        );
    }

    /// A far door with no animation of its own has no leaf that can swing out of the doorway, so the
    /// whole reference goes: the hole a static door opens as, and the one the window showed (the
    /// portal hid that reference while it drew the destination through the source doorway).
    #[test]
    fn a_crossing_arrives_at_a_far_door_with_nothing_to_animate_as_a_hole() {
        let mut app = door_app();
        let source = animated_door(&mut app);
        let far = spawn_far_static_door(&mut app, source.door);
        activate(&mut app, source.door);
        step(&mut app, 12);

        cross(&mut app, source.door);

        assert_eq!(state(&app, far), DoorState::Open { animated: false });
        assert!(
            state(&app, far).hides_whole_reference(),
            "which is the hole `crate::portal` draws an open door with no animation as"
        );
    }

    /// A far door that is already open is left exactly where it is: whatever opened it, the window
    /// showed the player a doorway, and the doorway is what they walked into. The arrival is not a
    /// second activation of a door that never shut.
    #[test]
    fn an_arrival_leaves_a_far_door_that_is_already_open_as_it_is() {
        let mut app = door_app();
        let source = animated_door(&mut app);
        let far = spawn_far_animated_door(&mut app, source.door);
        activate(&mut app, far.door);
        step(&mut app, 12);
        assert_eq!(state(&app, far.door), DoorState::Open { animated: true });
        activate(&mut app, source.door);
        step(&mut app, 2);

        cross(&mut app, source.door);

        assert!(
            player(&app, far.player)
                .animation(far.open_node)
                .expect("the far door's clip")
                .is_finished(),
            "the far door's own swing is left where it was - the arrival does not restart it"
        );
        assert_eq!(state(&app, far.door), DoorState::Open { animated: true });
    }

    /// The destination is streamed in before a crossing is applied, so the far door is normally
    /// there. When it is not, the arrival is held for it and applied the frame it appears; a crossing
    /// whose door never turns up is dropped without a word, because there is nothing to open and
    /// nothing to report.
    #[test]
    fn an_arrival_waits_for_its_far_door_and_gives_up_silently() {
        let mut app = door_app();
        let source = animated_door(&mut app);
        activate(&mut app, source.door);
        step(&mut app, 12);

        // The crossing lands before the destination cell has spawned the door it names.
        cross(&mut app, source.door);
        assert_eq!(
            app.world().resource::<ArrivalOpenings>().openings.len(),
            1,
            "with no door to open, the arrival is held for it"
        );

        step(&mut app, 2);
        let far = spawn_far_animated_door(&mut app, source.door);
        assert_eq!(state(&app, far.door), DoorState::Closed);
        step(&mut app, 1);
        assert_eq!(
            state(&app, far.door),
            DoorState::Open { animated: true },
            "the held arrival opens the door the frame it is there"
        );
        assert!(
            app.world()
                .resource::<ArrivalOpenings>()
                .openings
                .is_empty(),
            "and is not held any more"
        );

        // A door that never turns up (a link into a cell whose reference is not in the world): the
        // arrival is given up on, and the door that is there is not touched.
        let missing = far_ref(&app, source.door) + 1;
        app.world_mut().write_message(OpenDestinationDoor {
            door: source.door,
            destination_ref_id: missing,
        });
        app.update();
        assert_eq!(app.world().resource::<ArrivalOpenings>().openings.len(), 1);
        step(&mut app, ARRIVAL_OPEN_WAIT_FRAMES + 1);
        assert!(
            app.world()
                .resource::<ArrivalOpenings>()
                .openings
                .is_empty(),
            "a far door that never comes is given up on"
        );
        assert_eq!(
            state(&app, far.door),
            DoorState::Open { animated: true },
            "and nothing else was opened in the meantime"
        );
    }

    #[test]
    fn reactivating_an_open_door_closes_it() {
        let mut app = door_app();
        let door = animated_door(&mut app);
        activate(&mut app, door.door);
        step(&mut app, 6);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });

        activate(&mut app, door.door);

        assert_eq!(state(&app, door.door), DoorState::Closing);
        assert!(
            player(&app, door.player).is_playing_animation(door.close_node),
            "the close clip plays"
        );
        assert!(
            player(&app, door.player).is_playing_animation(door.open_node),
            "and the open one is still there, fading out rather than stopping dead"
        );

        // A second `E` while the door is closing is ignored: the rest pose is the pose a `Close`
        // clip starts from, so a door cannot be turned around to open again mid-close.
        activate(&mut app, door.door);
        assert_eq!(state(&app, door.door), DoorState::Closing);

        step(&mut app, 11);
        assert_eq!(
            state(&app, door.door),
            DoorState::Closed,
            "the close clip runs out and the door is shut again"
        );
        assert!(
            !player(&app, door.player).is_playing_animation(door.open_node),
            "the faded-out clip is stopped once it has no weight left"
        );
    }

    /// Reversing a door must not throw it to the other end of the swing: `Open` is reached halfway
    /// through the clip, so a door told to close at 0.6 s is 60% of the way open, and starting
    /// `Close` at its own zero (which *is* fully open) would move the leaf further in one frame than
    /// the whole swing's remaining time allows.
    #[test]
    fn a_reversal_continues_from_the_pose_the_door_reached() {
        let mut app = door_app();
        let door = animated_door(&mut app);
        activate(&mut app, door.door);
        step(&mut app, 6);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });

        let reached = player(&app, door.player)
            .animation(door.open_node)
            .expect("the open clip")
            .elapsed()
            / CLIP_SECONDS;
        assert!(reached > 0.0 && reached < 1.0, "mid-swing: {reached}");
        // The leaf really is part way through the swing - otherwise "the pose did not move much"
        // would be true of a leaf that never moved at all, and the test would prove nothing.
        let before = leaf_degrees(&app, door.leaf);
        assert!(
            before > WIDE_SWING_DEGREES * 0.4 && before < WIDE_SWING_DEGREES,
            "the leaf is part way open, not at either end: {before}"
        );

        activate(&mut app, door.door);

        assert_eq!(state(&app, door.door), DoorState::Closing);
        let close = player(&app, door.player)
            .animation(door.close_node)
            .expect("the close clip");
        assert!(
            (close.seek_time() - (1.0 - reached)).abs() <= STEP_SECONDS + 0.01,
            "the close clip starts at the pose the open one reached: {:?} for {reached}",
            close.seek_time()
        );
        let fading = player(&app, door.player)
            .animation(door.open_node)
            .expect("the open clip is still in the player while it fades")
            .weight();
        assert!(
            fading < 1.0,
            "the clip being left fades out over {} ms: weight {fading}",
            CLIP_FADE.as_millis()
        );

        let after = leaf_degrees(&app, door.leaf);
        let moved = (after - before).abs();
        let per_frame = STEP_SECONDS * WIDE_SWING_DEGREES;
        assert!(
            moved <= 2.0 * per_frame,
            "the leaf moved {moved} degrees in the reversal frame, where one frame of the swing is \
             {per_frame} degrees and starting `Close` at its own zero would have snapped the rest \
             of the swing in that one frame: {before} -> {after}"
        );
    }

    /// The model as `bevy_gltf` builds it for a door whose `Open` clip moves `Object02`: the clips
    /// name that node only, while the loader's `AnimationTargetId` is on every node under the
    /// animation root, the frame included.
    #[test]
    fn door_leaf_marks_the_nodes_the_clips_move_and_not_the_door_frame() {
        let mut app = door_app();
        let model = door_with_model(&mut app, NARROW_SWING_DEGREES);
        let ModelDoor {
            door,
            player,
            moved,
            moved_mesh,
            frame,
        } = model;

        app.update();

        let world = app.world();
        assert!(
            world
                .get::<DoorLeaf>(moved)
                .is_some_and(|leaf| leaf.door == door),
            "the node the clip moves is the leaf"
        );
        assert!(
            world.get::<DoorLeaf>(frame).is_none(),
            "a node with an AnimationTargetId the clips never name is not"
        );
        assert!(
            world.get::<DoorLeaf>(door).is_none() && world.get::<DoorLeaf>(moved_mesh).is_none(),
            "and neither is the door reference nor a mesh primitive - the leaf is the marked node"
        );
        let leaves: Vec<&DoorLeaf> = world
            .iter_entities()
            .filter_map(|e| e.get::<DoorLeaf>())
            .collect();
        assert_eq!(leaves.len(), 1, "exactly the marked node: {leaves:?}");
        assert!(
            world
                .get::<AnimationGraphHandle>(player)
                .is_some_and(|handle| world
                    .resource::<Assets<AnimationGraph>>()
                    .contains(handle.id())),
            "the graph the loader does not attach is on the player entity"
        );
        assert!(
            world.get::<AnimationTransitions>(player).is_some(),
            "and the transitions that fade a reversal are beside it"
        );
        assert!(
            world.get::<PendingDoorModel>(door).is_none(),
            "a resolved door is no longer waiting for its model"
        );
    }

    /// Whether a door's leaves are hidden when it opens is a fact about the `Open` clip it ends up
    /// with - how far that clip turns them - so it is read off the clip at the moment the door is
    /// given its animation, not guessed from the state or watched while the door swings.
    ///
    /// The clip a door ends up with is not always the clip its model carried, of course: a narrow
    /// swing is scaled up to open the doorway ([`swung_open_clips`]), so a model whose clip turns
    /// its leaf 8 degrees also arrives clearing it. A clip that turns its leaf nowhere at all has
    /// nothing to scale and does not clear - which is the case this test keeps for the leaves being
    /// hidden at all.
    #[test]
    fn the_clip_tells_the_door_whether_its_leaves_clear_the_doorway() {
        for (swing, clears) in [
            (0.0, false),
            (NARROW_SWING_DEGREES, true),
            (DOORWAY_CLEAR_DEGREES, true),
            (WIDE_SWING_DEGREES, true),
        ] {
            let mut app = door_app();
            let model = door_with_model(&mut app, swing);

            app.update();

            let animation = *app
                .world()
                .get::<DoorAnimation>(model.door)
                .expect("a resolved door");
            assert_eq!(
                animation.clears_doorway,
                clears,
                "{swing} degrees {} clear the doorway: {animation:?}",
                if clears { "does" } else { "does not" }
            );
        }
    }

    /// `Open`/`Close` is a convention rather than a guarantee - the install also has `open`,
    /// `close`, `Forward`, `Backward`, `Idle` and `Stage1` - so the engine matches the two names
    /// case-insensitively and a model that names neither gets its first clip as its opening one.
    #[test]
    fn door_clips_are_named_case_insensitively_with_the_first_clip_as_a_fallback() {
        let mut app = door_app();
        let mut add = |named: bool| {
            [
                (
                    if named { "open" } else { "Stage1" },
                    app.world_mut()
                        .resource_mut::<Assets<AnimationClip>>()
                        .add(clip(CLIP_SECONDS)),
                ),
                (
                    "Close",
                    app.world_mut()
                        .resource_mut::<Assets<AnimationClip>>()
                        .add(clip(CLIP_SECONDS)),
                ),
            ]
        };

        let lower = add(true);
        let model = gltf(&[lower[0].1.clone(), lower[1].1.clone()], &lower);
        let (open, close) = door_clips(&model);
        assert_eq!(open, Some(lower[0].1.clone()), "`open` is found by name");
        assert_eq!(close, Some(lower[1].1.clone()), "`Close` too");

        let other = add(false);
        let model = gltf(&[other[0].1.clone(), other[1].1.clone()], &other);
        let (open, close) = door_clips(&model);
        assert_eq!(
            open,
            Some(other[0].1.clone()),
            "a model that names no `open` opens with its first clip"
        );
        assert_eq!(close, Some(other[1].1.clone()), "and closes with `Close`");

        let model = gltf(&[], &[]);
        assert_eq!(
            (door_clips(&model).0, door_clips(&model).1),
            (None, None),
            "no clips at all: a static door"
        );
    }

    /// The state the assets are in while the converted door models carry no clips yet: the
    /// resolution has to come out static rather than wait for a clip that will never arrive.
    #[test]
    fn a_model_with_no_clips_resolves_to_a_static_door() {
        let mut app = door_app();
        let model = app
            .world_mut()
            .resource_mut::<Assets<Gltf>>()
            .add(gltf(&[], &[]));
        let door = app
            .world_mut()
            .spawn((load_door(false), DoorState::Closed, pending(model)))
            .id();

        app.update();

        let animation = *app
            .world()
            .get::<DoorAnimation>(door)
            .expect("a resolved door");
        assert!(
            animation.open.is_none() && animation.player.is_none(),
            "nothing to play: {animation:?}"
        );
        assert!(app.world().get::<PendingDoorModel>(door).is_none());

        activate(&mut app, door);
        assert_eq!(state(&app, door), DoorState::Open { animated: false });
    }

    /// A load door whose base has no model at all: static from its first frame, with no asset
    /// server in the path.
    #[test]
    fn a_door_with_no_model_is_static_from_its_first_frame() {
        let mut app = door_app();
        let door = app.world_mut().spawn(load_door(false)).id();

        app.update();

        assert_eq!(state(&app, door), DoorState::Closed);
        assert!(app.world().get::<DoorAnimation>(door).is_some());
        assert!(app.world().get::<PendingDoorModel>(door).is_none());

        activate(&mut app, door);
        assert_eq!(state(&app, door), DoorState::Open { animated: false });
    }

    /// A missing scene/player stays unresolved and closed even beyond the old timeout. Elapsed
    /// frames never turn an animated source into a static leaf.
    #[test]
    fn a_door_whose_scene_never_appears_remains_unresolved_and_closed() {
        let mut app = door_app();
        let open = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip(CLIP_SECONDS));
        let model = app.world_mut().resource_mut::<Assets<Gltf>>().add(gltf(
            std::slice::from_ref(&open),
            &[(OPEN_CLIP, open.clone())],
        ));
        let door = app
            .world_mut()
            .spawn((load_door(false), DoorState::Closed, pending(model)))
            .id();

        // A scene that is a frame or two late must not be written off.
        step(&mut app, 5);
        assert!(app.world().get::<DoorAnimation>(door).is_none());
        assert!(app.world().get::<PendingDoorModel>(door).is_some());

        step(&mut app, UNRESOLVED_OBSERVATION_FRAMES);
        assert!(app.world().get::<PendingDoorModel>(door).is_some());
        assert!(app.world().get::<DoorAnimation>(door).is_none());
        assert!(app.world().get::<DoorAnimationGaveUp>(door).is_none());
        activate(&mut app, door);
        assert_eq!(state(&app, door), DoorState::Closed);
        assert_eq!(
            DoorActivation::of(None, false, false),
            DoorActivation::Resolving
        );
    }

    /// A model `streaming.rs` has not armed yet has no scene, so nothing can be missing from it: the
    /// door's scene wait is not charged while [`PendingModel`] is on the reference, however many
    /// frames the spawn budget keeps it there, and the real player attaches once it is released.
    #[test]
    fn a_door_whose_model_is_still_queued_is_not_charged_the_scene_wait() {
        let mut app = door_app();
        let model = door_with_model(&mut app, 90.0);
        // The scene is not instanced: no player under the door, and the model is queued.
        app.world_mut().entity_mut(model.player).remove::<ChildOf>();
        app.world_mut()
            .entity_mut(model.door)
            .insert(PendingModel::for_test());

        step(&mut app, UNRESOLVED_OBSERVATION_FRAMES + 30);
        assert!(app.world().get::<DoorAnimation>(model.door).is_none());
        assert!(
            app.world().get::<DoorAnimationGaveUp>(model.door).is_none(),
            "a queued model is not a scene that never appeared"
        );
        assert_eq!(
            app.world()
                .get::<PendingDoorModel>(model.door)
                .expect("still resolving")
                .waiting,
            0,
            "no scene-wait frame was charged while the model was queued"
        );

        // The queue releases the model and the loader puts the player under the door.
        app.world_mut()
            .entity_mut(model.door)
            .remove::<PendingModel>();
        app.world_mut()
            .entity_mut(model.player)
            .insert(ChildOf(model.door));
        step(&mut app, 2);
        let animation = *app
            .world()
            .get::<DoorAnimation>(model.door)
            .expect("the real player attaches the animation");
        assert_eq!(animation.player(), Some(model.player));
        assert!(
            app.world()
                .get::<AnimationGraphHandle>(model.player)
                .is_some()
        );
        assert!(app.world().get::<DoorAnimationGaveUp>(model.door).is_none());
    }

    /// Releasing the streaming queue does not certify a missing scene as static, even after
    /// the former timeout window. A real source/player must still arrive.
    #[test]
    fn a_door_released_by_the_queue_remains_unresolved_without_its_scene() {
        let mut app = door_app();
        let model = door_with_model(&mut app, 90.0);
        app.world_mut().entity_mut(model.player).remove::<ChildOf>();
        app.world_mut()
            .entity_mut(model.door)
            .insert(PendingModel::for_test());
        step(&mut app, UNRESOLVED_OBSERVATION_FRAMES + 30);
        assert!(app.world().get::<DoorAnimationGaveUp>(model.door).is_none());

        app.world_mut()
            .entity_mut(model.door)
            .remove::<PendingModel>();
        step(&mut app, 5);
        assert!(
            app.world().get::<DoorAnimationGaveUp>(model.door).is_none(),
            "the count starts at the release, not at the queue"
        );
        step(&mut app, UNRESOLVED_OBSERVATION_FRAMES);
        assert!(app.world().get::<DoorAnimationGaveUp>(model.door).is_none());
        assert!(app.world().get::<DoorAnimation>(model.door).is_none());
    }

    /// The marker describes one resolution. A door that gave up and then resolves again - its
    /// animation withdrawn and its scene back - is judged on the new resolution, not on the old
    /// marker.
    #[test]
    fn a_door_that_resolves_again_after_giving_up_loses_the_given_up_marker() {
        let mut app = door_app();
        let model = door_with_model(&mut app, 90.0);
        app.world_mut().entity_mut(model.player).remove::<ChildOf>();
        step(&mut app, UNRESOLVED_OBSERVATION_FRAMES + 5);
        assert!(app.world().get::<DoorAnimationGaveUp>(model.door).is_none());
        assert!(app.world().get::<DoorAnimation>(model.door).is_none());

        app.world_mut()
            .entity_mut(model.door)
            .insert(DoorAnimationGaveUp);
        app.world_mut()
            .entity_mut(model.door)
            .remove::<DoorAnimation>();
        app.world_mut()
            .entity_mut(model.player)
            .insert(ChildOf(model.door));
        step(&mut app, 3);
        assert!(
            app.world().get::<DoorAnimationGaveUp>(model.door).is_none(),
            "the stale marker does not outlive the resolution it described"
        );
        let animation = *app
            .world()
            .get::<DoorAnimation>(model.door)
            .expect("resolved afresh");
        assert_eq!(animation.player(), Some(model.player));
        assert!(
            DoorActivation::of(Some(&animation), false, true) == DoorActivation::Animated,
            "and the door reads as animated"
        );
    }

    /// The same for the other way a door resolves: a model with no clip at all.
    #[test]
    fn a_clip_free_door_resolving_afresh_loses_a_stale_given_up_marker() {
        let mut app = door_app();
        let model = app
            .world_mut()
            .resource_mut::<Assets<Gltf>>()
            .add(gltf(&[], &[]));
        let door = app
            .world_mut()
            .spawn((
                load_door(false),
                DoorState::Closed,
                pending(model),
                DoorAnimationGaveUp,
            ))
            .id();
        step(&mut app, 2);
        assert!(
            app.world()
                .get::<DoorAnimation>(door)
                .is_some_and(|animation| !animation.swings())
        );
        assert!(app.world().get::<DoorAnimationGaveUp>(door).is_none());
    }

    /// Withdrawing a door's animation ([`forget_lost_door_players`]) takes the marker with it.
    #[test]
    fn withdrawing_a_door_animation_takes_the_given_up_marker_with_it() {
        let mut app = door_app();
        let model = door_with_model(&mut app, 90.0);
        step(&mut app, 2);
        app.world_mut()
            .entity_mut(model.door)
            .insert(DoorAnimationGaveUp);
        app.world_mut()
            .entity_mut(model.player)
            .remove::<AnimationPlayer>();
        step(&mut app, 1);
        assert!(app.world().get::<DoorAnimation>(model.door).is_none());
        assert!(app.world().get::<DoorAnimationGaveUp>(model.door).is_none());
        assert!(
            app.world()
                .get::<AnimationGraphHandle>(model.player)
                .is_none()
        );
        assert!(
            app.world()
                .get::<AnimationTransitions>(model.player)
                .is_none()
        );
    }

    /// What a caller that wants to open a door may rely on: the door's resolved animation, never the
    /// absence of a player.
    #[test]
    fn door_activation_reads_the_resolved_animation_and_never_a_missing_player() {
        let swings = DoorAnimation::swinging_for_test();
        let none = DoorAnimation::default();

        // Nothing resolved yet: no player proves nothing, whatever else is true.
        assert_eq!(
            DoorActivation::of(None, false, false),
            DoorActivation::Resolving
        );
        assert_eq!(
            DoorActivation::of(None, true, true),
            DoorActivation::Resolving,
            "a marker without an animation is not a resolution"
        );
        // A resolved static leaf and a resolved animated door are ready; the rest are not.
        assert_eq!(
            DoorActivation::of(Some(&none), false, false),
            DoorActivation::Static
        );
        assert_eq!(
            DoorActivation::of(Some(&swings), false, true),
            DoorActivation::Animated
        );
        // An animated door whose graph has been withdrawn is resolving again, not static.
        assert_eq!(
            DoorActivation::of(Some(&swings), false, false),
            DoorActivation::Resolving
        );
        // A door that gave up is never ready, with or without a player.
        assert_eq!(
            DoorActivation::of(Some(&none), true, false),
            DoorActivation::GaveUp
        );
        assert_eq!(
            DoorActivation::of(Some(&swings), true, true),
            DoorActivation::GaveUp
        );
        for (activation, ready) in [
            (DoorActivation::Resolving, false),
            (DoorActivation::GaveUp, false),
            (DoorActivation::Static, true),
            (DoorActivation::Animated, true),
        ] {
            assert_eq!(activation.ready(), ready, "{activation:?}");
        }
    }

    /// The module's own resolutions, end to end: a model with clips resolves to an animated door
    /// once its graph is on the player, and neither a clip-free model nor a door with no model
    /// carries the given-up marker.
    #[test]
    fn resolved_doors_report_activation_without_manufacturing_a_failure_marker() {
        let mut app = door_app();
        let animated = door_with_model(&mut app, 90.0);
        let before = DoorActivation::of(
            app.world().get::<DoorAnimation>(animated.door),
            false,
            false,
        );
        assert_eq!(before, DoorActivation::Resolving, "nothing attached yet");
        step(&mut app, 2);
        assert!(
            app.world()
                .get::<AnimationGraphHandle>(animated.player)
                .is_some()
        );
        assert_eq!(
            DoorActivation::of(
                app.world().get::<DoorAnimation>(animated.door),
                app.world()
                    .get::<DoorAnimationGaveUp>(animated.door)
                    .is_some(),
                true
            ),
            DoorActivation::Animated
        );

        let model = app
            .world_mut()
            .resource_mut::<Assets<Gltf>>()
            .add(gltf(&[], &[]));
        let clip_free = app
            .world_mut()
            .spawn((load_door(false), DoorState::Closed, pending(model)))
            .id();
        let no_model = app.world_mut().spawn(load_door(false)).id();
        step(&mut app, 2);
        for door in [clip_free, no_model] {
            assert!(app.world().get::<DoorAnimationGaveUp>(door).is_none());
            assert_eq!(
                DoorActivation::of(app.world().get::<DoorAnimation>(door), false, false),
                DoorActivation::Static
            );
        }
    }

    /// The published door models of the demo route, read the way [`crate::render`]'s tests read
    /// glTF: whatever clips they turn out to carry, a door model with clips has to resolve to one
    /// the engine can play, and a model without any has to stay static.
    ///
    /// Today the route's models carry none - the `Open`/`Close` clips are not converted yet - and
    /// the assertion is written so that it keeps meaning something on the day they do.
    ///
    /// The converted assets are game data and are never committed, so this test is opt-in: it is
    /// `#[ignore]`d and skips - printing why - when `MUDCRAB_CONVERTED_DIR` does not name a
    /// converted asset tree, so CI never needs proprietary data (ADR-0002).
    #[test]
    #[ignore = "reads the converted Skyrim door models (MUDCRAB_CONVERTED_DIR)"]
    fn the_route_door_models_resolve_to_a_clip_whenever_they_carry_one() {
        let Some(assets) = std::env::var_os("MUDCRAB_CONVERTED_DIR")
            .or_else(|| std::env::var_os("OPENSKYRIM_CONVERTED_DIR"))
            .map(std::path::PathBuf::from)
        else {
            eprintln!("skipping: set MUDCRAB_CONVERTED_DIR to a converted asset tree");
            return;
        };
        if !assets.is_dir() {
            eprintln!("skipping: {} is not a directory", assets.display());
            return;
        }
        const MODELS: [&str; 2] = [
            "meshes/dungeons/dwemer/door/dwemerlargedoorload01.glb",
            "meshes/dungeons/dwemer/door/dwemersmalldoorload01.glb",
        ];
        for model_path in MODELS {
            let path = assets.join(model_path);
            let Ok(bytes) = std::fs::read(&path) else {
                eprintln!("skipped: {} is not installed", path.display());
                continue;
            };
            let document = bevy::gltf::gltf::Gltf::from_slice(&bytes).expect("a valid .glb");

            // The model as the loader would hand it over: its animations, named as the glTF
            // document names them.
            let mut app = door_app();
            let mut handles = Vec::new();
            let mut named = Vec::new();
            for animation in document.animations() {
                let handle = app
                    .world_mut()
                    .resource_mut::<Assets<AnimationClip>>()
                    .add(clip(CLIP_SECONDS));
                if let Some(name) = animation.name() {
                    named.push((name.to_string(), handle.clone()));
                }
                handles.push(handle);
            }
            let named: Vec<(&str, Handle<AnimationClip>)> = named
                .iter()
                .map(|(name, handle)| (name.as_str(), handle.clone()))
                .collect();
            let model = gltf(&handles, &named);

            let (open, _) = door_clips(&model);
            assert_eq!(
                open.is_some(),
                !handles.is_empty(),
                "{model_path} carries {} clips: a door model with clips must resolve to one the \
                 engine can play, and one without must stay static",
                handles.len()
            );
        }
    }

    /// The route's own door, as the converter actually bakes it, through the real glTF loader: its
    /// `Open` clip turns its two leaves 5.36 and 8.74 degrees, its `Close` is that swing run
    /// backwards, and scaling the `Open` the way the engine scales a load door's clip stands the
    /// door open at [`SWUNG_OPEN_DEGREES`] with every leaf's first pose exactly where it was.
    ///
    /// This is the test the reconversion makes possible - until the door models carried clips there
    /// was nothing to scale - and it is the closest thing to a runtime check of path 2 that does not
    /// need a window: it measures the clip the engine would play, not a fixture that stands in for
    /// it. Opt-in like the test above, and skipped when `MUDCRAB_CONVERTED_DIR` is not set
    /// (ADR-0002).
    #[test]
    #[ignore = "reads the converted Skyrim door models (MUDCRAB_CONVERTED_DIR)"]
    fn the_route_door_s_own_clip_scales_to_a_doorway() {
        let Some(assets) = std::env::var_os("MUDCRAB_CONVERTED_DIR")
            .or_else(|| std::env::var_os("OPENSKYRIM_CONVERTED_DIR"))
            .map(std::path::PathBuf::from)
        else {
            eprintln!("skipping: set MUDCRAB_CONVERTED_DIR to a converted asset tree");
            return;
        };
        if !assets.is_dir() {
            eprintln!("skipping: {} is not a directory", assets.display());
            return;
        }

        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                file_path: assets.to_string_lossy().into_owned(),
                ..default()
            },
            bevy::gltf::GltfPlugin::default(),
            bevy::animation::AnimationPlugin,
            bevy::scene::ScenePlugin,
            bevy::render::mesh::MeshPlugin,
        ));
        // The glTF loader produces these asset kinds; without their types registered the load
        // fails before a clip is ever built.
        app.init_asset::<bevy::image::Image>()
            .init_asset::<bevy::pbr::StandardMaterial>();
        let handle: Handle<Gltf> = app
            .world()
            .resource::<AssetServer>()
            .load("meshes/dungeons/dwemer/door/dwemerlargedoorload01.glb");
        // Loading is asynchronous, and what this test is about is the clip the loader builds.
        for _ in 0..600 {
            app.update();
            if app
                .world()
                .resource::<Assets<Gltf>>()
                .get(&handle)
                .is_some()
            {
                break;
            }
            // The load runs on the IO pool; spinning `update` alone outpaces it.
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let Some(model) = app.world().resource::<Assets<Gltf>>().get(&handle) else {
            // A headless test app is not the engine: the glTF loader needs render-side plugins
            // this app cannot add, so a model that loads perfectly at runtime can fail to build
            // here. The engine's own runs are what check this end to end (the demo tour opens
            // every route door). Skip rather than fail on a harness limit.
            eprintln!(
                "skipping: the glTF loader did not build the model in a headless app ({:?})",
                app.world()
                    .resource::<AssetServer>()
                    .get_load_state(&handle)
            );
            return;
        };
        let (open, close) = door_clips(model);
        let (open, close) = (
            open.expect("`DweDoorLarge01Load` carries an `Open` clip"),
            close.expect("and a `Close` clip"),
        );
        let clips = app.world().resource::<Assets<AnimationClip>>();
        let open = clips.get(&open).expect("the `Open` clip's asset");
        let close = clips.get(&close).expect("the `Close` clip's asset");

        // What the design note measured in the NIF: 5.36 and 8.74 degrees (design section 1.4),
        // each leaf scaled by its own factor. Both are hinged: neither hangs from the other.
        let moved = clip_targets([open, close]);
        let swing = swing_degrees(open, &moved);
        assert!(
            (swing - 8.74).abs() < 0.05,
            "the door's own swing is {swing} degrees, and the design note says 8.74"
        );
        assert!(
            close_mirrors_open(open, close, &moved),
            "`DweDoorLarge01Load`'s `Close` is its `Open` run backwards"
        );

        let factor = swing_scale(swing);
        assert!(
            (factor - 10.30).abs() < 0.05,
            "so the wider leaf is scaled by {factor}, which is {} times its own swing",
            SWUNG_OPEN_DEGREES / swing
        );
        let factors = swing_factors(open, &moved);
        let (swung, closed) = swung_open_clips(Some(open), Some(close), &moved, &factors)
            .expect("the door's own clips are a rotation each, and scale");
        for target in &moved {
            let opened = node_swing_degrees(&swung, *target).expect("a leaf the clip turns");
            assert!(
                (opened - SWUNG_OPEN_DEGREES).abs() < 1.0,
                "the scaled swing stands each leaf at {SWUNG_OPEN_DEGREES}, not {opened} degrees"
            );
        }
        assert_eq!(
            swung.curves().keys().copied().collect::<HashSet<_>>(),
            moved,
            "and turns the same leaves the door's own clip turned"
        );

        // Every leaf starts exactly where the door's own clip starts it: the swing is longer, not
        // moved.
        let rotation = animated_field!(Transform::rotation);
        let closed = closed.expect("the scaled close");
        for target in &moved {
            for clip in [&swung, &closed] {
                let (Some(rest), Some(scaled_rest)) = (
                    open.sample_clamped(rotation.clone(), *target, 0.0),
                    clip.sample_clamped(rotation.clone(), *target, 0.0),
                ) else {
                    panic!("both clips turn every leaf the door's own clip turned");
                };
                let drift = rest.angle_between(scaled_rest).to_degrees();
                assert!(
                    drift < 0.01,
                    "first pose of a leaf moved by {drift} degrees"
                );
            }
        }

        // And one leaf of the door's model, named as the loader names it: the path hash the twin's
        // clips have to be rebuilt onto (`twin_target_ids`) is this one, computed from the glTF's
        // own node names - `Creation-to-glTF basis` over `DwemerLargeDoorLoad01` over `Object02`.
        let leaf_path: Vec<Name> = [
            "Creation-to-glTF basis",
            "DwemerLargeDoorLoad01",
            "Object02",
        ]
        .iter()
        .map(|name| Name::new(*name))
        .collect();
        assert!(
            moved.contains(&AnimationTargetId::from_names(leaf_path.iter())),
            "the loader's target id is a hash of the node's whole name path"
        );
    }

    #[test]
    fn an_auto_load_door_is_born_open_and_never_changes() {
        let mut app = door_app();
        let door = app
            .world_mut()
            .spawn((
                load_door(true),
                MeshHandle("meshes/dungeons/dwemer/door/dwemerlargedoorload01.glb".into()),
            ))
            .id();

        app.update();
        assert_eq!(state(&app, door), DoorState::Open { animated: false });
        assert!(
            app.world().get::<PendingDoorModel>(door).is_none(),
            "an invisible marker loads no model"
        );

        activate(&mut app, door);
        step(&mut app, 5);
        assert_eq!(
            state(&app, door),
            DoorState::Open { animated: false },
            "a marker crosses the player on contact and has no animation to run"
        );
    }

    /// The walk probe's question: is this mesh part of a door that is open? A closed door is solid,
    /// leaf and frame; an open door is not something to walk into at all, because some models carry
    /// static parts in the opening that no clip moves (the Dwemer doors' `Plane02` panel). The price
    /// is that an open door's own frame posts no longer stop the player; the house's walls still do.
    #[test]
    fn a_mesh_of_a_door_is_out_of_the_way_only_while_the_door_is_open() {
        let mut app = door_app();
        let door = animated_door(&mut app);

        // The fixture is what `attach_door_animations` leaves behind: the moving node marked, the
        // frame beside it not.
        assert!(app.world().get::<DoorLeaf>(door.leaf).is_some());
        assert!(app.world().get::<DoorLeaf>(door.frame).is_none());
        assert!(
            !probe_skips(&mut app, door.leaf_mesh),
            "a closed door's leaf is something to walk into"
        );
        assert!(
            !probe_skips(&mut app, door.frame_mesh),
            "the frame is not a leaf at all"
        );

        activate(&mut app, door.door);

        assert!(
            probe_skips(&mut app, door.leaf_mesh),
            "an open door's leaf is not something to walk into, drawn or not: {door:?}",
            door = state(&app, door.door)
        );
        assert!(
            probe_skips(&mut app, door.frame_mesh),
            "and neither is the rest of the open door's model, which a static panel in the opening              would otherwise leave solid"
        );
    }

    /// [`mesh_is_out_of_the_way`] as `player_walk` calls it, with the three queries it needs.
    fn probe_skips(app: &mut App, mesh: Entity) -> bool {
        let mut state = SystemState::<(Query<&ChildOf>, Query<&DoorLeaf>, Query<&DoorState>)>::new(
            app.world_mut(),
        );
        let (parents, leaves, states) = state.get(app.world()).expect("read-only queries");
        crate::doors::mesh_is_out_of_the_way(mesh, &parents, &leaves, &states)
    }

    /// The doors the player's doorway trigger asked to cross, in order.
    #[derive(Resource, Default)]
    struct Crossed(Vec<Entity>);

    fn collect_crossings(mut crossed: ResMut<Crossed>, mut requests: MessageReader<CrossDoor>) {
        for message in requests.read() {
            crossed.0.push(message.door);
        }
    }

    /// One frame of the player's walk: their eye is put where a player whose feet are at `feet`
    /// would have it - the trigger reads the camera, not the keys - and the frame runs.
    fn walk(app: &mut App, camera: Entity, feet: Vec3) {
        let eye = eye_from_feet(feet);
        app.world_mut().entity_mut(camera).insert((
            Transform::from_translation(eye),
            GlobalTransform::from_translation(eye),
        ));
        app.update();
    }

    /// The feature end to end, in one test: `E` on a door whose model carries an `Open` clip starts
    /// that clip, the door becomes a way through halfway through the swing, and the player's
    /// doorway trigger crosses them the frame their feet reach the plane. Neither half says anything
    /// on its own - the state machine that opens a door nobody walks through, or a crossing that
    /// fires on a door that never opened - so this is the test that the two are one feature.
    ///
    /// The `E` press is the message the controller writes for it ([`crate::player::player_door`]),
    /// and the trigger is the real one, added over the door's own app the way `PlayerPlugin` adds it.
    #[test]
    fn e_opens_the_door_and_the_walk_through_it_crosses_once_it_is_open() {
        // The doorway the fixture's door measures with no bounds of its own: `AUTO_DOOR_MARKER_SIZE`
        // around the reference, which is 160 x 240 wide and tall - so a player at 300 units is
        // outside it and 30 units past the reference is through the plane.
        let base = Vec3::new(1000.0, 0.0, 1000.0);
        // The fixture's door faces east (`outward` is Creation +x), so the player walks in from the
        // east and the doorway's plane is the reference's own.
        let in_front = base + Vec3::X * 300.0;
        let through = base - Vec3::X * 30.0;

        let mut app = door_app();
        let door = animated_door(&mut app);
        app.world_mut().entity_mut(door.door).insert((
            Transform::from_translation(base),
            GlobalTransform::from_translation(base),
        ));
        let camera = app
            .world_mut()
            .spawn((
                StreamingCamera,
                Player::default(),
                Transform::from_translation(eye_from_feet(in_front)),
                GlobalTransform::from_translation(eye_from_feet(in_front)),
            ))
            .id();
        app.init_resource::<ProfilingState>()
            .init_resource::<Crossed>()
            .add_message::<CrossDoor>()
            .add_message::<DoorCrossed>()
            .add_systems(
                Update,
                (player_walks_through_doors, collect_crossings).chain(),
            );

        // A closed door has no way through it: the walk over its plane fires nothing at all.
        assert_eq!(state(&app, door.door), DoorState::Closed);
        app.update();
        walk(&mut app, camera, through);
        assert!(
            app.world().resource::<Crossed>().0.is_empty(),
            "a closed door is not walked through, however it is crossed"
        );

        // `E` on it, which is `OpenDoor`: the door's own clip starts, and the doorway is not a way
        // through because `E` was pressed - it is one because the door is open.
        walk(&mut app, camera, in_front);
        app.world_mut().write_message(OpenDoor { door: door.door });
        app.update();
        assert_eq!(
            state(&app, door.door),
            DoorState::Opening,
            "E starts the swing; it does not open the door in one frame"
        );
        assert!(
            player(&app, door.player).is_playing_animation(door.open_node),
            "and the clip playing is the door model's own `Open`"
        );

        // Half a clip later it is open, holding its last key, and the player has still not moved.
        step(&mut app, 6);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        assert!(
            app.world().resource::<Crossed>().0.is_empty(),
            "nothing crosses while the player stands in front of the doorway"
        );

        // The same walk over the same plane, now that the door is open, crosses them - on the
        // doorway's plane, which is where the window the portal draws through ends.
        walk(&mut app, camera, through);
        assert_eq!(
            app.world().resource::<Crossed>().0,
            vec![door.door],
            "the open door is the one the crossing fires on"
        );
    }

    /// A door holding a crossing draws its leaves however far the clip swung them: the doorway is a
    /// shut door until the destination it is waiting for streams in, and there is no window through
    /// it either.
    #[test]
    fn a_door_holding_its_crossing_keeps_its_leaves_drawn() {
        let narrow = DoorAnimation {
            clears_doorway: false,
            ..default()
        };
        assert!(
            !leaves_are_drawn(DoorState::Open { animated: true }, Some(&narrow), false),
            "an 8-degree clip leaves its leaves in the doorway, and they go once it is open"
        );
        assert!(
            leaves_are_drawn(DoorState::Open { animated: true }, Some(&narrow), true),
            "unless the door is waiting for its destination, where the doorway is a shut door"
        );
    }

    /// A door with an open clip and no close clip replays the open one instead of closing
    /// (design section 5).
    #[test]
    fn a_door_without_a_close_clip_replays_its_open_instead_of_closing() {
        let mut app = door_app();
        let door = animated_door(&mut app);
        app.world_mut()
            .entity_mut(door.door)
            .get_mut::<DoorAnimation>()
            .expect("the fixture's animation")
            .close = None;
        activate(&mut app, door.door);
        step(&mut app, 6);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        let held = player(&app, door.player)
            .animation(door.open_node)
            .expect("the open clip")
            .elapsed();

        activate(&mut app, door.door);

        assert_eq!(state(&app, door.door), DoorState::Opening);
        let replay = player(&app, door.player)
            .animation(door.open_node)
            .expect("the open clip is playing again");
        assert!(
            !replay.is_finished(),
            "the clip runs again rather than staying finished where it stopped"
        );
        assert!(
            replay.elapsed() < held,
            "and from the beginning: {} after {}",
            replay.elapsed(),
            held
        );
    }

    /// The swing is what the player asked to see, so a door's leaves are drawn while the `Open`
    /// clip plays - and a clip that turns them a few degrees leaves them in the opening, so they are
    /// hidden when the door is `Open`. Only the leaves: the frame is not a `DoorLeaf`.
    #[test]
    fn a_narrow_clip_leaves_its_leaves_drawn_through_the_swing_and_hidden_once_open() {
        let mut app = door_app();
        let door = animated_door_swinging(&mut app, NARROW_SWING_DEGREES);
        assert!(
            !app.world()
                .get::<DoorAnimation>(door.door)
                .expect("the fixture's animation")
                .clears_doorway,
            "an 8 degree swing does not clear a doorway"
        );
        assert_ne!(
            leaf_visibility(&app, door.leaf),
            Visibility::Hidden,
            "a closed door's leaf is drawn"
        );

        activate(&mut app, door.door);
        step(&mut app, 4);
        assert_eq!(state(&app, door.door), DoorState::Opening);
        assert_ne!(
            leaf_visibility(&app, door.leaf),
            Visibility::Hidden,
            "and it is still drawn while it swings"
        );

        step(&mut app, 2);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        assert_eq!(
            leaf_visibility(&app, door.leaf),
            Visibility::Hidden,
            "the clip did not open the doorway, so the leaf goes"
        );

        // Closing draws it again from the first frame: a leaf that is coming back is a leaf.
        activate(&mut app, door.door);
        assert_eq!(state(&app, door.door), DoorState::Closing);
        assert_ne!(
            leaf_visibility(&app, door.leaf),
            Visibility::Hidden,
            "the closing swing is drawn"
        );

        step(&mut app, 11);
        assert_eq!(state(&app, door.door), DoorState::Closed);
        assert_ne!(leaf_visibility(&app, door.leaf), Visibility::Hidden);
    }

    /// A door that swings its leaf clear of the opening keeps it: the doorway is open, and the leaf
    /// next to it is what a real door looks like when it is open.
    #[test]
    fn a_clip_that_swings_its_leaf_clear_of_the_doorway_leaves_it_drawn() {
        let mut app = door_app();
        let door = animated_door_swinging(&mut app, WIDE_SWING_DEGREES);

        activate(&mut app, door.door);
        step(&mut app, 6);

        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        assert_ne!(
            leaf_visibility(&app, door.leaf),
            Visibility::Hidden,
            "the leaf swung out of the doorway and stays drawn: {} degrees",
            leaf_degrees(&app, door.leaf)
        );
    }

    /// A static door has no leaf to move, so an open one is a hole where its model was - and that
    /// is the portal's line, driven by [`DoorState::hides_whole_reference`]. An animated door never
    /// answers true, whatever its leaves are doing.
    #[test]
    fn only_a_static_door_hides_its_whole_reference() {
        assert!(!DoorState::Closed.hides_whole_reference());
        assert!(!DoorState::Opening.hides_whole_reference());
        assert!(!DoorState::Closing.hides_whole_reference());
        assert!(
            !DoorState::Open { animated: true }.hides_whole_reference(),
            "an animating door keeps its frame: its leaves are the animation's business"
        );
        assert!(
            DoorState::Open { animated: false }.hides_whole_reference(),
            "a door with no clip of its own has nothing to swing out of the way"
        );

        // And `is_open` is what the crossing gates on: usable from the first frame of the swing,
        // for either kind of door, and not while it is closed or closing.
        assert!(!DoorState::Closed.is_open());
        assert!(DoorState::Opening.is_open());
        assert!(!DoorState::Closing.is_open());
        assert!(DoorState::Open { animated: true }.is_open());
        assert!(DoorState::Open { animated: false }.is_open());
    }

    /// The converter's wrapper node, which every converted model's scene hangs under, and the names
    /// of the two models of one load/twin pair - the `CasExFreeSmDoorLoad01` pair, one of the two
    /// pairs in the install whose models really do name their leaves the same thing.
    const BASIS: &str = "Creation-to-glTF basis";
    const MODEL_ROOT: &str = "CasExFreeSmDoorLoad01";
    const TWIN_ROOT: &str = "CasExFreeSmDoor01";
    /// The node the door's own `Open` clip turns.
    const OWN_LEAF: &str = "Door01";

    /// How long the twin's clips last in these tests: twice the door's own, so which of the two
    /// clips a door ended up with is in the state machine's clock and not only in the angle.
    const TWIN_CLIP_SECONDS: f32 = 2.0;

    /// The id the loader would give the node at `path` under the animation root: a hash of the whole
    /// path, the scene root's own name first.
    fn target_id(path: &[&str]) -> AnimationTargetId {
        let names: Vec<Name> = path
            .iter()
            .map(|name| Name::new(name.to_string()))
            .collect();
        AnimationTargetId::from_names(names.iter())
    }

    /// The name path of a node of a converted model, as the loader builds it.
    fn node_path(name: &str) -> Vec<Name> {
        [BASIS, MODEL_ROOT, name]
            .iter()
            .map(|name| Name::new(name.to_string()))
            .collect()
    }

    /// A door of a load/twin pair, and the nodes the two models' clips turn.
    struct TwinDoor {
        door: Entity,
        /// The model path the door carries, which is what the borrow log dedupes on.
        path: String,
        /// The node the door's own `Open` clip turns, a node of the door's own model by
        /// construction.
        own_leaf: Entity,
        /// The node the twin's `Open` clip turns, where the door's model has one by that name.
        twin_leaf: Option<Entity>,
    }

    /// A load door of `MODEL_ROOT` with a non-load twin loaded beside it, laid out the way the
    /// loader lays out a converted model: the animation root is the converter's basis wrapper, the
    /// model's own root node hangs under it, and the leaves under that.
    ///
    /// The two models' clips are aimed at different nodes on purpose. The door's own `Open` clip
    /// turns `OWN_LEAF` by `own_swing` degrees over [`CLIP_SECONDS`]; the twin's turns `twin_leaf` by
    /// `twin_swing` over [`TWIN_CLIP_SECONDS`]. `nodes` are the node names the door's own model has
    /// below its root, so `twin_leaf` is a node of the door's model only when the caller puts it
    /// there - and that is the whole of whether the twin's clip fits this door.
    fn door_with_twin(
        app: &mut App,
        own_swing: f32,
        twin_swing: f32,
        nodes: &[&str],
        twin_leaf: &str,
    ) -> TwinDoor {
        let own = target_id(&[BASIS, MODEL_ROOT, OWN_LEAF]);
        let open = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip_swinging(CLIP_SECONDS, own, 0.0, own_swing));
        let close = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip_swinging(CLIP_SECONDS, own, own_swing, 0.0));
        let model = app.world_mut().resource_mut::<Assets<Gltf>>().add(gltf(
            &[open.clone(), close.clone()],
            &[(OPEN_CLIP, open), (CLOSE_CLIP, close)],
        ));

        let twin = target_id(&[BASIS, TWIN_ROOT, twin_leaf]);
        let twin_open = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip_swinging(TWIN_CLIP_SECONDS, twin, 0.0, twin_swing));
        let twin_close = app
            .world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .add(clip_swinging(TWIN_CLIP_SECONDS, twin, twin_swing, 0.0));
        let twin_model = app.world_mut().resource_mut::<Assets<Gltf>>().add(gltf(
            &[twin_open.clone(), twin_close.clone()],
            &[(OPEN_CLIP, twin_open), (CLOSE_CLIP, twin_close)],
        ));

        let path = format!("meshes/dungeons/castle/lghalls/{MODEL_ROOT}.glb");
        let door = app
            .world_mut()
            .spawn((
                load_door(false),
                DoorState::Closed,
                PendingDoorModel {
                    binding: None,
                    model: Some(model),
                    path: path.clone(),
                    twin: Some(TwinModel {
                        path: format!("meshes/dungeons/castle/lghalls/{TWIN_ROOT}.glb"),
                        model: twin_model,
                    }),
                    waiting: 0,
                },
            ))
            .id();
        let player = app
            .world_mut()
            .spawn((
                Name::new(BASIS),
                target_id(&[BASIS]),
                AnimationPlayer::default(),
                ChildOf(door),
            ))
            .id();
        let root = app
            .world_mut()
            .spawn((
                Name::new(MODEL_ROOT),
                target_id(&[BASIS, MODEL_ROOT]),
                AnimatedBy(player),
                Transform::default(),
                ChildOf(player),
            ))
            .id();
        let leaves: Vec<(String, Entity)> = nodes
            .iter()
            .map(|node| {
                let entity = app
                    .world_mut()
                    .spawn((
                        Name::new(node.to_string()),
                        target_id(&[BASIS, MODEL_ROOT, node]),
                        AnimatedBy(player),
                        Transform::default(),
                        Visibility::default(),
                        ChildOf(root),
                    ))
                    .id();
                ((*node).to_owned(), entity)
            })
            .collect();
        let leaf = |wanted: &str| {
            leaves
                .iter()
                .find(|(node, _)| node == wanted)
                .map(|(_, entity)| *entity)
        };
        TwinDoor {
            door,
            path,
            own_leaf: leaf(OWN_LEAF).expect("the door's own model has its own leaf"),
            twin_leaf: leaf(twin_leaf),
        }
    }

    /// The door's own clip of this fixture is the demo route's problem in miniature: it turns the
    /// leaf 8 degrees, which is not a doorway. The twin's clip is the same door's real swing, and it
    /// is rebuilt onto the door's own node and played there.
    #[test]
    fn a_load_door_whose_own_clip_is_narrow_borrows_its_twins_swing() {
        let mut app = door_app();
        let door = door_with_twin(
            &mut app,
            NARROW_SWING_DEGREES,
            WIDE_SWING_DEGREES,
            &[OWN_LEAF, "Plane02"],
            OWN_LEAF,
        );

        app.update();

        let animation = *app
            .world()
            .get::<DoorAnimation>(door.door)
            .expect("a resolved door");
        assert!(
            animation.clears_doorway,
            "the twin's 120 degrees open the doorway where the door's own 8 did not: {animation:?}"
        );
        assert_eq!(
            animation.open.expect("an open clip").seconds,
            TWIN_CLIP_SECONDS,
            "the clip the door ended up with is the twin's, which is twice as long as its own"
        );
        assert!(
            app.world().get::<DoorLeaf>(door.own_leaf).is_some(),
            "the node the borrowed clip turns is marked as the leaf"
        );
        assert!(
            app.world().get::<PendingDoorModel>(door.door).is_none(),
            "and the door is resolved"
        );
        assert_eq!(
            app.world().resource::<AdjustedSwings>().0.get(&door.path),
            Some(&SwingSource::Twin),
            "the twin's clips were the first thing tried, and they were good enough: the door's own \
             clip was never scaled"
        );

        // It is the borrowed clip that plays, on the door's own node: the leaf ends where the twin's
        // clip leaves it, not where the door's own 8-degree clip did.
        activate(&mut app, door.door);
        step(&mut app, 24);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        let degrees = leaf_degrees(&app, door.own_leaf);
        assert!(
            (degrees - WIDE_SWING_DEGREES).abs() < 1.0,
            "the leaf is where the twin's clip took it ({degrees} degrees), not where the door's \
             own clip left it ({NARROW_SWING_DEGREES})"
        );
        assert_ne!(
            leaf_visibility(&app, door.own_leaf),
            Visibility::Hidden,
            "and a leaf that swung clear stays drawn"
        );
    }

    /// A door that can open itself never looks for a twin: its own clip is the swing the player
    /// sees, whichever of the two would open the doorway.
    #[test]
    fn a_door_whose_own_clip_opens_the_doorway_keeps_its_own_clip() {
        let mut app = door_app();
        let door = door_with_twin(
            &mut app,
            WIDE_SWING_DEGREES,
            NARROW_SWING_DEGREES,
            &[OWN_LEAF],
            OWN_LEAF,
        );

        app.update();

        let animation = *app
            .world()
            .get::<DoorAnimation>(door.door)
            .expect("a resolved door");
        assert!(animation.clears_doorway);
        assert_eq!(
            animation.open.expect("an open clip").seconds,
            CLIP_SECONDS,
            "the door's own clip, not the twin's: {animation:?}"
        );
        assert!(
            app.world().resource::<AdjustedSwings>().0.is_empty(),
            "nothing was done to this door's swing: its own clip already opens the doorway"
        );

        activate(&mut app, door.door);
        step(&mut app, 12);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        let degrees = leaf_degrees(&app, door.own_leaf);
        assert!(
            (degrees - WIDE_SWING_DEGREES).abs() < 1.0,
            "the door turned its leaf its own way: {degrees} degrees"
        );
    }

    /// The twin's clips have to move nodes the door's own model has, because a clip that names a
    /// node the door has not got swings nothing there.
    ///
    /// This is the demo route's own pair, at the size of the real data: its load door turns
    /// `Object02` and `Object43` and its twin turns `Object45` and `Object47`, so the twin is no use
    /// to it (`tools/research/load_door_twin_coverage.py pairs` finds 17 of the install's 19
    /// twinned models like this, the demo route's among them). The door then gets its swing the
    /// second way instead: its own clip, scaled up.
    #[test]
    fn a_twin_whose_clips_move_other_nodes_is_not_borrowed() {
        let mut app = door_app();
        let door = door_with_twin(
            &mut app,
            NARROW_SWING_DEGREES,
            WIDE_SWING_DEGREES,
            &[OWN_LEAF],
            "Object45",
        );
        assert!(
            door.twin_leaf.is_none(),
            "the door's model has no `Object45`: the twin's own leaf is not in this scene"
        );

        app.update();

        let animation = *app
            .world()
            .get::<DoorAnimation>(door.door)
            .expect("a resolved door");
        assert_eq!(
            animation.open.expect("an open clip").seconds,
            CLIP_SECONDS,
            "the door's own clip, not the twin's (which is twice as long)"
        );
        assert_eq!(
            app.world().resource::<AdjustedSwings>().0.get(&door.path),
            Some(&SwingSource::Scaled),
            "the door's own clip was scaled, not the twin's borrowed"
        );

        // And it is the door's own swing, turned further: its own clip takes the leaf to 8 degrees
        // and the scaled one to 90, and the twin's untouched would have taken it to 120 over two
        // seconds.
        activate(&mut app, door.door);
        step(&mut app, 12);
        assert_eq!(state(&app, door.door), DoorState::Open { animated: true });
        let degrees = leaf_degrees(&app, door.own_leaf);
        assert!(
            (degrees - SWUNG_OPEN_DEGREES).abs() < 1.0,
            "the door's own swing, scaled to open the doorway: {degrees} degrees"
        );
        assert_ne!(
            leaf_visibility(&app, door.own_leaf),
            Visibility::Hidden,
            "a swing that clears the doorway leaves its leaf drawn"
        );
    }

    /// A twin that never loads is not waited for forever: the door gives up on it after
    /// [`TWIN_WAIT_FRAMES`] - much sooner than it would give up on its own scene, because until it
    /// gives up it is a door nobody can walk through - and gets its swing from its own clip instead.
    #[test]
    fn a_twin_that_never_arrives_is_given_up_on_and_the_door_keeps_its_own_clip() {
        let mut app = door_app();
        let door = door_with_twin(
            &mut app,
            NARROW_SWING_DEGREES,
            WIDE_SWING_DEGREES,
            &[OWN_LEAF],
            OWN_LEAF,
        );
        // A handle the asset server never fills in: the model it names is not there.
        let missing = app.world().resource::<Assets<Gltf>>().reserve_handle();
        app.world_mut()
            .entity_mut(door.door)
            .get_mut::<PendingDoorModel>()
            .expect("the fixture's pending door")
            .twin = Some(TwinModel {
            path: "meshes/fixtures/door/door01twin.glb".to_owned(),
            model: missing,
        });

        step(&mut app, 5);
        assert!(
            app.world().get::<PendingDoorModel>(door.door).is_some(),
            "a twin that is a frame or two late must not be written off"
        );

        step(&mut app, TWIN_WAIT_FRAMES);

        let animation = *app
            .world()
            .get::<DoorAnimation>(door.door)
            .expect("a resolved door");
        assert_eq!(
            animation.open.expect("an open clip").seconds,
            CLIP_SECONDS,
            "the door's own clip, not the twin's: {animation:?}"
        );
        assert_eq!(
            app.world().resource::<AdjustedSwings>().0.get(&door.path),
            Some(&SwingSource::Scaled),
            "and it was scaled, since the twin never came"
        );
        assert!(app.world().get::<PendingDoorModel>(door.door).is_none());
    }

    /// A twin's clips are sub-assets of its model and can arrive a frame after it, the way the
    /// door's own clips did: a door waits for them rather than writing the twin off as one that does
    /// not fit and settling for a clip that does not open its doorway.
    #[test]
    fn a_twin_whose_clips_are_a_frame_late_is_waited_for() {
        let mut app = door_app();
        let door = door_with_twin(
            &mut app,
            NARROW_SWING_DEGREES,
            WIDE_SWING_DEGREES,
            &[OWN_LEAF],
            OWN_LEAF,
        );
        // The twin's model, naming clips that have not been loaded yet.
        let open = app
            .world()
            .resource::<Assets<AnimationClip>>()
            .reserve_handle();
        let close = app
            .world()
            .resource::<Assets<AnimationClip>>()
            .reserve_handle();
        let twin = target_id(&[BASIS, TWIN_ROOT, OWN_LEAF]);
        let twin_model = app.world_mut().resource_mut::<Assets<Gltf>>().add(gltf(
            &[open.clone(), close.clone()],
            &[(OPEN_CLIP, open.clone()), (CLOSE_CLIP, close.clone())],
        ));
        app.world_mut()
            .entity_mut(door.door)
            .get_mut::<PendingDoorModel>()
            .expect("the fixture's pending door")
            .twin = Some(TwinModel {
            path: "meshes/fixtures/door/door01twin.glb".to_owned(),
            model: twin_model,
        });

        app.update();
        assert!(
            app.world().get::<DoorAnimation>(door.door).is_none(),
            "the twin's clips are not here yet, so neither is the door's animation"
        );

        // They arrive.
        app.world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .insert(
                open.id(),
                clip_swinging(TWIN_CLIP_SECONDS, twin, 0.0, WIDE_SWING_DEGREES),
            )
            .expect("the clip arrives under the id the twin's model already names");
        app.world_mut()
            .resource_mut::<Assets<AnimationClip>>()
            .insert(
                close.id(),
                clip_swinging(TWIN_CLIP_SECONDS, twin, WIDE_SWING_DEGREES, 0.0),
            )
            .expect("and so does the closing clip");
        step(&mut app, 2);

        let animation = *app
            .world()
            .get::<DoorAnimation>(door.door)
            .expect("a resolved door");
        assert!(
            animation.clears_doorway,
            "the twin's swing was borrowed once its clips arrived: {animation:?}"
        );
        assert_eq!(
            animation.open.expect("an open clip").seconds,
            TWIN_CLIP_SECONDS,
            "and it is the twin's clip, not the door's own"
        );
    }

    /// A model path with no `load` marker in it is a door with no twin at all - the door does not
    /// look for one, and does not wait for one either: its own clip is the only one there is, and it
    /// resolves in the frame it is asked. 72 of the 103 models load doors use are like this
    /// (`tools/research/load_door_twin_coverage.py models`), and their swing is path 2's.
    #[test]
    fn a_door_whose_model_name_carries_no_marker_has_no_twin_to_look_for() {
        let mut app = door_app();
        let door = door_with_twin(
            &mut app,
            NARROW_SWING_DEGREES,
            WIDE_SWING_DEGREES,
            &[OWN_LEAF],
            OWN_LEAF,
        );
        // The twin the fixture loaded is taken away with the marker: the question is whether the
        // door goes looking for one at all, not whether one it was handed is good enough.
        {
            let mut entity = app.world_mut().entity_mut(door.door);
            let mut pending = entity
                .get_mut::<PendingDoorModel>()
                .expect("the fixture's pending door");
            pending.path = "meshes/fixtures/door/door01.glb".to_owned();
            pending.twin = None;
        }

        app.update();

        let animation = *app
            .world()
            .get::<DoorAnimation>(door.door)
            .expect("a resolved door in the frame it was asked");
        assert_eq!(
            animation.open.expect("an open clip").seconds,
            CLIP_SECONDS,
            "the door's own clip, resolved at once: {animation:?}"
        );
        assert_eq!(
            app.world()
                .resource::<AdjustedSwings>()
                .0
                .get("meshes/fixtures/door/door01.glb"),
            Some(&SwingSource::Scaled),
            "a door with no twin still gets a swing, from its own clip"
        );
    }

    /// A door model with no clips of its own has nothing to borrow onto, however good its twin's
    /// swing is: `bevy_gltf` gives a scene with no animations no [`AnimationPlayer`] and no
    /// [`AnimationTargetId`]s (`bevy_gltf-0.19.0/src/loader/mod.rs#L1090`, `#L1545`), so there is no
    /// animation root to hang a borrowed clip on. Such a door stays the static door it is today.
    #[test]
    fn a_model_with_no_clips_of_its_own_has_nothing_to_borrow_onto() {
        let mut app = door_app();
        let model = app
            .world_mut()
            .resource_mut::<Assets<Gltf>>()
            .add(gltf(&[], &[]));
        let door = app
            .world_mut()
            .spawn((load_door(false), DoorState::Closed, pending(model)))
            .id();
        app.world_mut()
            .entity_mut(door)
            .get_mut::<PendingDoorModel>()
            .expect("the fixture's pending door")
            .path = format!("meshes/dungeons/castle/lghalls/{MODEL_ROOT}.glb");

        app.update();

        let animation = *app
            .world()
            .get::<DoorAnimation>(door)
            .expect("a resolved door");
        assert!(
            animation.open.is_none() && animation.player.is_none(),
            "a static door, not one waiting on a twin: {animation:?}"
        );
        assert!(
            app.world().resource::<AdjustedSwings>().0.is_empty(),
            "and there was no clip to borrow or to scale"
        );

        activate(&mut app, door);
        assert_eq!(state(&app, door), DoorState::Open { animated: false });
    }

    /// The log says what was done to a model's swing once, not once per door of that model.
    #[test]
    fn an_adjusted_swing_is_logged_once_per_model_and_not_once_per_door() {
        let mut app = door_app();
        let first = door_with_twin(
            &mut app,
            NARROW_SWING_DEGREES,
            WIDE_SWING_DEGREES,
            &[OWN_LEAF],
            OWN_LEAF,
        );
        step(&mut app, 2);
        let second = door_with_twin(
            &mut app,
            NARROW_SWING_DEGREES,
            WIDE_SWING_DEGREES,
            &[OWN_LEAF],
            OWN_LEAF,
        );
        step(&mut app, 2);

        for door in [first.door, second.door] {
            let animation = *app
                .world()
                .get::<DoorAnimation>(door)
                .expect("a resolved door");
            assert!(
                animation.clears_doorway,
                "both doors of the model borrowed the swing: {animation:?}"
            );
        }
        let adjusted = app.world().resource::<AdjustedSwings>();
        assert_eq!(
            adjusted.0.iter().collect::<Vec<_>>(),
            vec![(&first.path, &SwingSource::Twin)],
            "one entry per model, keyed by the door's own model path, saying which path it took"
        );
    }

    /// The path rule, over the real model paths of the install's load doors
    /// (`tools/research/load_door_twin_coverage.py models`, which dumped them from
    /// `skyrim_world.db`): the marker is taken out wherever it sits in the file name, it is
    /// case-insensitive, and a name without one has no twin.
    #[test]
    fn the_twin_path_is_the_model_path_with_the_load_marker_taken_out() {
        for (model, twin) in [
            // The demo route's own door: its twin's leaves are named Object45/Object47, so the
            // twin's clips do not fit this model and it keeps its own clip.
            (
                "meshes/Dungeons/Dwemer/Door/DwemerLargeDoorLoad01.glb",
                Some("meshes/Dungeons/Dwemer/Door/DwemerLargeDoor01.glb"),
            ),
            (
                "meshes/Dungeons/Dwemer/Door/DwemerSmallDoorLoad01.glb",
                Some("meshes/Dungeons/Dwemer/Door/DwemerSmallDoor01.glb"),
            ),
            // `...Load02`: the second door of a pair.
            (
                "meshes/Dungeons/Nordic/Doors/Animated/SmDoor02/NorDoorSmLoad02.glb",
                Some("meshes/Dungeons/Nordic/Doors/Animated/SmDoor02/NorDoorSm02.glb"),
            ),
            // The marker is not always followed by a number.
            (
                "meshes/Dungeons/Imperial/Door/ImpWoodDoorHoleDoorLoad.glb",
                Some("meshes/Dungeons/Imperial/Door/ImpWoodDoorHoleDoor.glb"),
            ),
            // And it can have a direction in it: `LoadUp01`, `LoadDown01`, `LoadExt`.
            (
                "meshes/Dungeons/Dwemer/Facades/DweFacadeLiftLeverLoadUp01.glb",
                Some("meshes/Dungeons/Dwemer/Facades/DweFacadeLiftLeverUp01.glb"),
            ),
            (
                "meshes/Dungeons/Dwemer/Facades/DweFacadeLiftLeverLoadDown01.glb",
                Some("meshes/Dungeons/Dwemer/Facades/DweFacadeLiftLeverDown01.glb"),
            ),
            (
                "meshes/DLC02/Dungeons/IceCastle/DLC02IceDoorLoadExt.glb",
                Some("meshes/DLC02/Dungeons/IceCastle/DLC02IceDoorExt.glb"),
            ),
            // No number and no direction, just the marker before the `Door`: `LoadDoor01`.
            (
                "meshes/Dungeons/Nordic/Exterior/Animated/NorLabyrinthianDoor/\
                 NorLabyrinthianLoadDoor01.glb",
                Some(
                    "meshes/Dungeons/Nordic/Exterior/Animated/NorLabyrinthianDoor/\
                     NorLabyrinthianDoor01.glb",
                ),
            ),
            // The case-insensitive one, on the model of the install's auto-load markers: its derived
            // twin is not there, and an auto-load door has no model loaded at all.
            (
                "meshes/AutoLoadMarker01.glb",
                Some("meshes/AutoMarker01.glb"),
            ),
            // A door whose model name carries no marker has no twin.
            ("meshes/Architecture/Farmhouse/FarmhouseLDoor01.glb", None),
            ("meshes/Dungeons/Riften/RatwayHall/RiftenDoor01.glb", None),
        ] {
            assert_eq!(
                twin_model_path(model).as_deref(),
                twin,
                "{model}: the twin is the same path with the first `load` out of the file name"
            );
        }
    }

    /// A twin is asked of the asset server only when it is on disk: `MineDoor01.glb`, the derived
    /// twin of the mine's load door, was never converted (research-583), and loading it anyway
    /// logged an asset failure for every mine door.
    #[test]
    fn a_twin_that_was_never_converted_is_no_twin() {
        let root = tempfile::tempdir().expect("temporary asset root");
        let found = "meshes/Dungeons/Dwemer/Door/DwemerLargeDoor01.glb";
        std::fs::create_dir_all(root.path().join("meshes/Dungeons/Dwemer/Door")).unwrap();
        std::fs::write(root.path().join(found), b"glb").unwrap();

        assert_eq!(
            twin_model_on_disk(
                "meshes/Dungeons/Dwemer/Door/DwemerLargeDoorLoad01.glb",
                Some(root.path())
            ),
            TwinLookup::Found(found.to_owned())
        );
        assert_eq!(
            twin_model_on_disk(
                "meshes/Dungeons/Mines/MineDoorLoad01.glb",
                Some(root.path())
            ),
            TwinLookup::Missing("meshes/Dungeons/Mines/MineDoor01.glb".to_owned())
        );
        assert_eq!(
            twin_model_on_disk(
                "meshes/Architecture/Farmhouse/FarmhouseLDoor01.glb",
                Some(root.path())
            ),
            TwinLookup::NoMarker
        );
        // No asset root to check against (a test app): the derived path is taken as it was.
        assert_eq!(
            twin_model_on_disk("meshes/Dungeons/Mines/MineDoorLoad01.glb", None),
            TwinLookup::Found("meshes/Dungeons/Mines/MineDoor01.glb".to_owned())
        );
    }

    /// The same marker rule on the names of the models' own root nodes, which is what turns a door
    /// node's id into the id the twin's clips use for it. Real root node names, read from the models
    /// themselves (`tools/research/load_door_twin_coverage.py pairs`).
    #[test]
    fn the_twin_root_is_the_model_root_with_the_marker_taken_out() {
        for (model_root, twin_root) in [
            ("DwemerLargeDoorLoad01", Some("DwemerLargeDoor01")),
            ("DwemerSmallDoorLoad01", Some("DwemerSmallDoor01")),
            ("CasExFreeSmDoorLoad01", Some("CasExFreeSmDoor01")),
            ("CasExFreeLgDoorLoad01", Some("CasExFreeLgDoor01")),
            ("NorDoorSmallLoad02", Some("NorDoorSmall02")),
            // Roots that do not follow the rule - the twin of `NorDoorSmallLoad02` is named
            // `Dummy01` - find nothing to borrow from, which is what a model whose names are
            // unrelated to its twin's should do. 6 of the install's 19 twinned pairs are like this.
            ("Dummy12", None),
            ("ImpDoorDouble01", None),
        ] {
            assert_eq!(
                strip_load_marker(model_root).as_deref(),
                twin_root,
                "{model_root}"
            );
        }

        // And the ids themselves: a node of the door's model has the id the twin gives it, with the
        // model's root written the way the twin writes it.
        let door = node_path(OWN_LEAF);
        assert_eq!(
            twin_target_ids(&door),
            vec![target_id(&[BASIS, TWIN_ROOT, OWN_LEAF])],
            "one marker-carrying element in this path, so one id the twin could have for the node"
        );
        assert!(
            !twin_target_ids(&door).contains(&target_id(&[BASIS, TWIN_ROOT, "Object45"])),
            "and it is not the twin's id for some other node"
        );
        assert!(
            twin_target_ids(&[Name::new("Creation-to-glTF basis"), Name::new(OWN_LEAF)]).is_empty(),
            "a path with no marker in it has no twin ids at all"
        );
    }

    /// The pose of `target` in `clip` at `time`, or a panic - the tests below are all about where a
    /// clip puts a leaf.
    fn pose(clip: &AnimationClip, target: AnimationTargetId, time: f32) -> Quat {
        clip.sample_clamped(animated_field!(Transform::rotation), target, time)
            .expect("the clip turns this node")
    }

    /// How far `clip` turns `target` from its first pose, at `time`, in degrees.
    fn swung_from_rest(clip: &AnimationClip, target: AnimationTargetId, time: f32) -> f32 {
        pose(clip, target, 0.0)
            .angle_between(pose(clip, target, time))
            .to_degrees()
    }

    /// A narrow clip is scaled until its widest leaf stands at [`SWUNG_OPEN_DEGREES`], about the
    /// hinges it already had: `t = 0` is the pose the door stands in and does not move, the clip
    /// keeps its length and its nodes, and only the distance it turns is different.
    #[test]
    fn a_narrow_clip_is_scaled_until_its_widest_leaf_opens_the_doorway() {
        let leaf = target_id(&[BASIS, MODEL_ROOT, OWN_LEAF]);
        let open = clip_swinging(CLIP_SECONDS, leaf, 0.0, NARROW_SWING_DEGREES);
        let moved = HashSet::from([leaf]);
        let factors = swing_factors(&open, &moved);
        let factor = factors[&leaf];
        assert!((factor - SWUNG_OPEN_DEGREES / NARROW_SWING_DEGREES).abs() < 1.0e-4);

        let (swung, close) = swung_open_clips(Some(&open), None, &moved, &factors)
            .expect("a rotation curve is what scaling is for");

        // The same length, the same node, and the same pose to start from: the swing is longer, not
        // different.
        assert_eq!(swung.duration(), open.duration());
        assert_eq!(
            swung.curves().keys().copied().collect::<HashSet<_>>(),
            open.curves().keys().copied().collect::<HashSet<_>>(),
            "the scaled clip turns the door's own nodes and no others"
        );
        let rest_drift = pose(&swung, leaf, 0.0)
            .angle_between(pose(&open, leaf, 0.0))
            .to_degrees();
        assert!(
            rest_drift < 0.01,
            "t = 0 is the pose the door stands in, and stays it: {rest_drift} degrees of drift"
        );

        // And it opens the doorway: the widest leaf stands at `SWUNG_OPEN_DEGREES` where the clip it
        // came from managed 8.
        assert!(
            (swung_from_rest(&swung, leaf, swung.duration()) - SWUNG_OPEN_DEGREES).abs() < 0.5,
            "the scaled swing: {} degrees",
            swung_from_rest(&swung, leaf, swung.duration())
        );
        assert!(
            swing_degrees(&swung, &HashSet::from([leaf])) >= DOORWAY_CLEAR_DEGREES,
            "which is a doorway, so the leaves stay drawn when the door is open"
        );
        assert!(close.is_none(), "the door's own clip has no close to scale");
    }

    /// `clip` with a second rotation curve on `target`, turning it from `from_degrees` to
    /// `to_degrees` about the vertical over the clip's length.
    fn with_swing(
        mut clip: AnimationClip,
        target: AnimationTargetId,
        from_degrees: f32,
        to_degrees: f32,
    ) -> AnimationClip {
        let seconds = clip.duration();
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(
                animated_field!(Transform::rotation),
                AnimatableKeyframeCurve::new([
                    (0.0, Quat::from_rotation_y(from_degrees.to_radians())),
                    (seconds, Quat::from_rotation_y(to_degrees.to_radians())),
                ])
                .expect("two keys at different times"),
            ),
        );
        clip
    }

    /// Each leaf of a double door is scaled by its own factor, so both stand open: the Honningbrew
    /// Meadery's `WRDragonDoor01` turns its leaves 9.4 and 17.0 degrees, and one factor taken from
    /// the wider leaf stood the narrower one at 50 degrees, half across the doorway (the user's
    /// capture of 2026-09-25, "this type of door doesn't open the whole way").
    #[test]
    fn each_leaf_of_a_double_door_is_scaled_until_it_stands_open() {
        let left = target_id(&[BASIS, "WRDragonDoor01", "WRdragonDoorNewL"]);
        let right = target_id(&[BASIS, "WRDragonDoor01", "WRdragonDoorNewR"]);
        let open = with_swing(
            clip_swinging(CLIP_SECONDS, left, 0.0, 9.4),
            right,
            0.0,
            17.0,
        );
        let moved = HashSet::from([left, right]);

        let factors = swing_factors(&open, &moved);
        assert!((factors[&left] - SWUNG_OPEN_DEGREES / 9.4).abs() < 1.0e-3);
        assert!((factors[&right] - SWUNG_OPEN_DEGREES / 17.0).abs() < 1.0e-3);
        let (swung, _) = swung_open_clips(Some(&open), None, &moved, &factors).expect("two leaves");

        for (leaf, name) in [(left, "left"), (right, "right")] {
            let degrees = swung_from_rest(&swung, leaf, swung.duration());
            assert!(
                (degrees - SWUNG_OPEN_DEGREES).abs() < 0.5,
                "the {name} leaf stands at {degrees} degrees, not {SWUNG_OPEN_DEGREES}"
            );
        }
    }

    /// A node mounted on a leaf - `RiftenRWDoorJailLoad01`'s `Handle`, turning 69 degrees on a
    /// `Door` that turns 23 - is neither what says the doorway is open nor scaled with the leaf: the
    /// leaf is scaled to stand open, and the handle keeps its own motion on it.
    #[test]
    fn a_handle_on_a_leaf_neither_opens_the_doorway_nor_is_scaled() {
        let mut world = World::new();
        let door_target = target_id(&[BASIS, "RiftenRWDoorJailLoad01", "Door"]);
        let handle_target = target_id(&[BASIS, "RiftenRWDoorJailLoad01", "Door", "Handle"]);
        let frame_target = target_id(&[BASIS, "RiftenRWDoorJailLoad01", "Frame"]);
        let player = world.spawn(AnimationPlayer::default()).id();
        let leaf = world.spawn((door_target, ChildOf(player))).id();
        world.spawn((handle_target, ChildOf(leaf)));
        world.spawn((frame_target, ChildOf(player)));
        let moved = HashSet::from([door_target, handle_target]);

        let mut state =
            SystemState::<(Query<&Children>, Query<&AnimationTargetId>)>::new(&mut world);
        let (children, targets) = state.get(&world).expect("a read-only query pair");
        let hinged = hinged_leaves(player, &moved, &children, &targets);
        assert_eq!(
            hinged,
            HashSet::from([door_target]),
            "the handle hangs from the leaf, and the frame is not moved at all"
        );

        let open = with_swing(
            clip_swinging(CLIP_SECONDS, door_target, 0.0, 23.2),
            handle_target,
            0.0,
            68.9,
        );
        assert!(
            swing_degrees(&open, &moved) >= DOORWAY_CLEAR_DEGREES,
            "measured with the handle, this door would count as open already"
        );
        assert!(
            swing_degrees(&open, &hinged) < DOORWAY_CLEAR_DEGREES,
            "measured on its leaf, it does not"
        );

        let factors = swing_factors(&open, &hinged);
        assert_eq!(factors.len(), 1, "only the leaf is scaled: {factors:?}");
        let (swung, _) = swung_open_clips(Some(&open), None, &moved, &factors).expect("a leaf");
        let leaf_degrees = swung_from_rest(&swung, door_target, swung.duration());
        assert!(
            (leaf_degrees - SWUNG_OPEN_DEGREES).abs() < 0.5,
            "the leaf stands open: {leaf_degrees} degrees"
        );
        let handle_degrees = swung_from_rest(&swung, handle_target, swung.duration());
        assert!(
            (handle_degrees - 68.9).abs() < 0.5,
            "the handle turns as far as it did: {handle_degrees} degrees"
        );
    }

    /// A clip whose widest leaf already clears the doorway is left alone, even when its other leaf
    /// does not: a door whose own swing is a doorway is not the engine's to change.
    #[test]
    fn a_double_door_whose_widest_leaf_clears_is_not_scaled() {
        let left = target_id(&[BASIS, MODEL_ROOT, "Left"]);
        let right = target_id(&[BASIS, MODEL_ROOT, "Right"]);
        let open = with_swing(
            clip_swinging(CLIP_SECONDS, left, 0.0, 30.0),
            right,
            0.0,
            80.0,
        );
        assert!(swing_factors(&open, &HashSet::from([left, right])).is_empty());
    }

    /// A double door's leaves open together: Whiterun's main gate, `WRDoorMainGate01`, turns its
    /// leaves 5.0 and 2.2 degrees, and the narrower leaf is scaled past the cap to stand where the
    /// wider one does rather than stopping at 44 degrees across half the gateway. A single leaf is
    /// still capped.
    #[test]
    fn a_double_door_s_leaves_open_together_past_the_cap() {
        let wide = target_id(&[BASIS, "WRDoorMainGate01", "FrontGateDoor08"]);
        let narrow = target_id(&[BASIS, "WRDoorMainGate01", "FrontGateDoor06"]);
        let open = with_swing(
            clip_swinging(CLIP_SECONDS, wide, 0.0, -5.0),
            narrow,
            0.0,
            2.2,
        );
        let hinged = HashSet::from([wide, narrow]);
        let factors = swing_factors(&open, &hinged);
        assert!(
            factors[&narrow] > MAX_SWING_SCALE,
            "the narrow leaf goes past the cap: {factors:?}"
        );
        let (swung, _) =
            swung_open_clips(Some(&open), None, &hinged, &factors).expect("two leaves");
        for (name, leaf) in [("wide", wide), ("narrow", narrow)] {
            let degrees = swung_from_rest(&swung, leaf, swung.duration());
            assert!(
                (degrees - SWUNG_OPEN_DEGREES).abs() < 0.5,
                "the {name} leaf stands at {degrees} degrees, not {SWUNG_OPEN_DEGREES}"
            );
        }

        // The door the leaves are scaled to is the one the cap lets the widest leaf reach: two
        // leaves that barely move (2.9 and 2.0) both stand at the widest one's 58 degrees.
        assert_eq!(double_door_target([2.9_f32, 2.0].into_iter()), Some(58.0));
        // One leaf keeps its own capped scale; a leaf that turns nothing is not a leaf of the pair.
        assert_eq!(double_door_target([2.9_f32].into_iter()), None);
        assert_eq!(double_door_target([2.9_f32, 0.0].into_iter()), None);
    }

    /// A clip that already clears the doorway is never scaled - not up, and not down either - and
    /// neither is one that turns nothing at all, which has no rotation to scale.
    #[test]
    fn a_clip_that_already_clears_the_doorway_is_never_scaled() {
        assert_eq!(swing_scale(WIDE_SWING_DEGREES), 1.0);
        assert_eq!(swing_scale(DOORWAY_CLEAR_DEGREES), 1.0);
        assert_eq!(
            swing_scale(0.0),
            1.0,
            "a clip that turns nothing has nothing to scale"
        );
        assert_eq!(
            swing_scale(f32::NAN),
            1.0,
            "and neither has one whose swing cannot be measured"
        );

        // And a door whose own clip turns its leaf 120 degrees keeps that clip: it is not the door's
        // swing that is wrong, and scaling it would be inventing motion just as surely.
        let mut app = door_app();
        let model = door_with_model(&mut app, WIDE_SWING_DEGREES);
        app.update();
        let animation = *app
            .world()
            .get::<DoorAnimation>(model.door)
            .expect("a resolved door");
        assert!(animation.clears_doorway);
        assert!(
            app.world().resource::<AdjustedSwings>().0.is_empty(),
            "nothing was done to this door's swing at all"
        );
    }

    /// A clip that barely moves at all is not blown up past its own geometry: the widest leaf is
    /// taken to `SWUNG_OPEN_DEGREES` when the clip's swing is close enough for the factor to stay
    /// under [`MAX_SWING_SCALE`], and stops at the cap when it is not.
    #[test]
    fn the_scale_factor_is_capped() {
        // `RiftenRWDoorLoad01` turns its leaf 2.9 degrees: 31 times its own swing would be 90, and
        // 20 times it is 58 - still a doorway, still this door's own swing.
        let factor = swing_scale(2.9);
        assert_eq!(factor, MAX_SWING_SCALE, "capped");

        let leaf = target_id(&[BASIS, MODEL_ROOT, OWN_LEAF]);
        let open = clip_swinging(CLIP_SECONDS, leaf, 0.0, 2.9);
        let moved = HashSet::from([leaf]);
        let factors = swing_factors(&open, &moved);
        assert_eq!(factors[&leaf], factor);
        let (swung, _) = swung_open_clips(Some(&open), None, &moved, &factors).expect("one leaf");
        let degrees = swung_from_rest(&swung, leaf, swung.duration());
        assert!(
            (degrees - 2.9 * MAX_SWING_SCALE).abs() < 0.5,
            "the cap's swing: {degrees} degrees"
        );
        assert!(
            degrees >= DOORWAY_CLEAR_DEGREES,
            "which still opens the doorway: {degrees} degrees"
        );

        // The demo route's own door, whose leaves turn 5.36 and 8.74 degrees: each leaf has its own
        // factor, and neither is near the cap. These are the numbers the report quotes.
        let factor = swing_scale(8.74);
        assert!(
            (factor - 10.30).abs() < 0.01,
            "the demo route's `DweDoorLarge01Load` wider leaf is scaled by {factor}"
        );
        let factor = swing_scale(5.36);
        assert!(
            (factor - 16.79).abs() < 0.01,
            "and its narrower one by {factor}"
        );

        // Bleak Falls Barrow's `Ruins_LargeDoor01` turns its leaves 4.5 and 4.2 degrees: the first
        // is scaled to the target at the cap, the second stops at the cap, 84 degrees.
        assert!((swing_scale(4.5) - MAX_SWING_SCALE).abs() < 1.0e-4);
        assert_eq!(swing_scale(4.2), MAX_SWING_SCALE);
    }

    /// A door whose `Close` is its `Open` run backwards - every load door in the install that has
    /// both - closes by retracing the scaled swing, not by running `Close` through the same scaling
    /// and carrying the leaf past closed.
    /// A `Close` that is a motion of its own - here one that also moves a node the `Open` does not,
    /// as Dragonsreach's `WRCastleDoor01` does not mirror its `Open` - is scaled about the pose the
    /// door closes to, so a scaled door that closes stands shut: scaled about its own first pose
    /// (the open one) it ended the leaf far past the frame, standing open in a "closed" doorway.
    #[test]
    fn a_close_of_its_own_ends_on_the_pose_the_door_stands_in() {
        let leaf = target_id(&[BASIS, MODEL_ROOT, OWN_LEAF]);
        let latch = target_id(&[BASIS, MODEL_ROOT, "Latch"]);
        let open = clip_swinging(CLIP_SECONDS, leaf, 0.0, NARROW_SWING_DEGREES);
        let mut close = clip_swinging(CLIP_SECONDS, leaf, NARROW_SWING_DEGREES, 0.0);
        let latch_turn = clip_swinging(CLIP_SECONDS, latch, 0.0, 3.0);
        for (target, curves) in latch_turn.curves() {
            for curve in curves {
                close.add_variable_curve_to_target(*target, curve.clone());
            }
        }
        let moved = HashSet::from([leaf]);
        assert!(!close_mirrors_open(&open, &close, &moved));

        let factors = swing_factors(&open, &moved);
        let (swung, closed) = swung_open_clips(Some(&open), Some(&close), &moved, &factors)
            .expect("a swinging door with a close of its own");
        let closed = closed.expect("the close came back");
        let from = pose(&closed, leaf, 0.0)
            .angle_between(pose(&swung, leaf, swung.duration()))
            .to_degrees();
        assert!(
            from < 0.5,
            "the close starts where the swing ended, not {from} degrees off"
        );
        let stopped = pose(&closed, leaf, closed.duration())
            .angle_between(pose(&open, leaf, 0.0))
            .to_degrees();
        assert!(stopped < 0.5, "and stands shut, not {stopped} degrees open");
    }

    #[test]
    fn a_mirrored_close_is_the_scaled_open_run_backwards() {
        let leaf = target_id(&[BASIS, MODEL_ROOT, OWN_LEAF]);
        let open = clip_swinging(CLIP_SECONDS, leaf, 0.0, NARROW_SWING_DEGREES);
        let close = clip_swinging(CLIP_SECONDS, leaf, NARROW_SWING_DEGREES, 0.0);
        let moved = HashSet::from([leaf]);
        assert!(
            close_mirrors_open(&open, &close, &moved),
            "the fixture's close is the open run backwards"
        );

        let factors = swing_factors(&open, &moved);
        let (swung, closed) = swung_open_clips(Some(&open), Some(&close), &moved, &factors)
            .expect("a swinging door with a mirror close");
        let closed = closed.expect("the close came back");

        assert_eq!(
            closed.duration(),
            swung.duration(),
            "a reversed clip is the same length as the one it came from"
        );
        // Closing starts where the swing ended and ends where the door stands.
        let from = swung_from_rest(&swung, leaf, swung.duration());
        assert!(
            (from - SWUNG_OPEN_DEGREES).abs() < 0.5,
            "the scaled swing opens to {from} degrees"
        );
        let closing = pose(&closed, leaf, 0.0)
            .angle_between(pose(&swung, leaf, swung.duration()))
            .to_degrees();
        assert!(
            closing < 0.5,
            "closing starts from the pose the swing ended on, not {closing} degrees from it"
        );
        let stopped = pose(&closed, leaf, closed.duration())
            .angle_between(pose(&open, leaf, 0.0))
            .to_degrees();
        assert!(
            stopped < 0.5,
            "and ends where the door stands, not {stopped} degrees from it"
        );

        // Every sample of the close is a sample of the swing, taken from the other end: closing
        // retraces exactly the motion opening made.
        for step in 0..=10 {
            let fraction = step as f32 / 10.0;
            let closing_pose = pose(&closed, leaf, closed.duration() * fraction);
            let opening_pose = pose(&swung, leaf, swung.duration() * (1.0 - fraction));
            let apart = closing_pose.angle_between(opening_pose).to_degrees();
            assert!(
                apart < 0.5,
                "at {fraction} of the way back the door is {apart} degrees from where it was on \
                 the way out"
            );
        }
    }

    /// A scaled door whose own `Close` is a motion of its own - starting 5 degrees off where the
    /// swing ended - closes by retracing its scaled swing: scaling that motion would end it tens of
    /// degrees from the pose the door stands in.
    #[test]
    fn a_scaled_door_with_a_close_of_its_own_retraces_its_swing() {
        let leaf = target_id(&[BASIS, MODEL_ROOT, OWN_LEAF]);
        let open = clip_swinging(CLIP_SECONDS, leaf, 0.0, NARROW_SWING_DEGREES);
        let close = clip_swinging(CLIP_SECONDS, leaf, 5.0, 0.0);
        let moved = HashSet::from([leaf]);
        assert!(
            !close_mirrors_open(&open, &close, &moved),
            "5 degrees is not where the swing ended at 8"
        );
        let factors = swing_factors(&open, &moved);
        let (swung, closed) = swung_open_clips(Some(&open), Some(&close), &moved, &factors)
            .expect("a swinging door with a close of its own");
        let closed = closed.expect("the close came back");
        let from = pose(&closed, leaf, 0.0)
            .angle_between(pose(&swung, leaf, swung.duration()))
            .to_degrees();
        assert!(
            from < 0.5,
            "the close starts where the swing ended, not {from} degrees off"
        );
        let stopped = pose(&closed, leaf, closed.duration())
            .angle_between(pose(&open, leaf, 0.0))
            .to_degrees();
        assert!(stopped < 0.5, "and stands shut, not {stopped} degrees open");
    }

    /// A door whose own clip is too narrow to open the doorway is given its own swing, scaled: the
    /// leaves it turns are the leaves of its own model, they stay drawn once the door is open, and
    /// the door takes exactly as long about it as its own clip did.
    #[test]
    fn a_door_whose_own_clip_barely_moves_is_given_a_swing_of_its_own() {
        let mut app = door_app();
        let model = door_with_model(&mut app, NARROW_SWING_DEGREES);

        app.update();

        let animation = *app
            .world()
            .get::<DoorAnimation>(model.door)
            .expect("a resolved door");
        assert!(
            animation.clears_doorway,
            "the door's own 8 degrees scaled to open the doorway: {animation:?}"
        );
        assert_eq!(
            app.world()
                .resource::<AdjustedSwings>()
                .0
                .get(PLAIN_MODEL_PATH),
            Some(&SwingSource::Scaled)
        );
        assert!(
            app.world().get::<DoorLeaf>(model.moved).is_some(),
            "the leaf the scaled clip turns is the node its own clip turned"
        );

        activate(&mut app, model.door);
        step(&mut app, 4);
        let part_way = leaf_degrees(&app, model.moved);
        assert!(
            (0.0..=WIDE_SWING_DEGREES).contains(&part_way),
            "the swing is under way: {part_way} degrees"
        );

        step(&mut app, 8);
        assert_eq!(state(&app, model.door), DoorState::Open { animated: true });
        let degrees = leaf_degrees(&app, model.moved);
        assert!(
            (degrees - SWUNG_OPEN_DEGREES).abs() < 1.0,
            "and it ends standing open: {degrees} degrees"
        );
        assert_ne!(
            leaf_visibility(&app, model.moved),
            Visibility::Hidden,
            "a door that swings open keeps its leaf"
        );
    }

    /// A material comes out drawn from both sides - both culling fields, agreeing - and one that
    /// already is needs no copy.
    #[test]
    fn a_double_sided_copy_culls_nothing_and_keeps_the_rest() {
        let one_sided = StandardMaterial {
            base_color: Color::srgb(0.4, 0.3, 0.2),
            perceptual_roughness: 0.7,
            ..default()
        };
        assert!(one_sided.cull_mode.is_some() && !one_sided.double_sided);

        let copy = double_sided(&one_sided).expect("a one-sided material is copied");
        assert!(copy.double_sided);
        assert!(copy.cull_mode.is_none());
        assert_eq!(copy.base_color, one_sided.base_color);
        assert_eq!(copy.perceptual_roughness, one_sided.perceptual_roughness);
        assert!(
            double_sided(&copy).is_none(),
            "a double-sided material is drawn as it is"
        );
    }

    /// Every mesh under a door's leaf is moved onto a double-sided copy of its material, and the
    /// frame's mesh keeps the original - even where the two share it, as a model's meshes often do.
    /// Two doors of one model share one copy.
    #[test]
    fn a_leaf_is_drawn_from_both_sides_and_its_frame_is_not() {
        let mut app = door_app();
        app.init_asset::<StandardMaterial>();
        let shared = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let doors = [
            door_with_model(&mut app, NARROW_SWING_DEGREES),
            door_with_model(&mut app, NARROW_SWING_DEGREES),
        ];
        let mut frame_meshes = Vec::new();
        for door in &doors {
            app.world_mut()
                .entity_mut(door.moved_mesh)
                .insert(MeshMaterial3d(shared.clone()));
            frame_meshes.push(
                app.world_mut()
                    .spawn((MeshMaterial3d(shared.clone()), ChildOf(door.frame)))
                    .id(),
            );
        }

        app.update();

        let material_of = |app: &App, mesh: Entity| {
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(mesh)
                .expect("the mesh keeps a standard material")
                .0
                .clone()
        };
        let leaf_materials: Vec<_> = doors
            .iter()
            .map(|door| {
                assert!(app.world().get::<DoorLeaf>(door.moved).is_some());
                material_of(&app, door.moved_mesh)
            })
            .collect();
        assert_ne!(leaf_materials[0], shared, "the leaf is drawn with a copy");
        assert_eq!(
            leaf_materials[0], leaf_materials[1],
            "one copy for every door of the model"
        );
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        let copy = materials.get(&leaf_materials[0]).expect("the copy's asset");
        assert!(copy.double_sided && copy.cull_mode.is_none());

        for frame_mesh in frame_meshes {
            assert_eq!(
                material_of(&app, frame_mesh),
                shared,
                "the frame keeps the material it had"
            );
        }
        let original = materials.get(&shared).expect("the original's asset");
        assert!(
            !original.double_sided && original.cull_mode.is_some(),
            "and the original is not changed under it"
        );
    }

    // -----------------------------------------------------------------------------------------
    // The far door of the doorway swings with the near one (impl-234)
    // -----------------------------------------------------------------------------------------

    /// The portal drawing through `near` into the doorway of `far`: the pair `update_portal`
    /// publishes, and the one [`drive_far_doors`] reads.
    fn portal_draws_through(app: &mut App, near: Entity, far: Entity) {
        app.insert_resource(crate::portal::PortalState::drawing_through(near, far));
    }

    /// The portal drawing through nothing.
    fn portal_draws_nothing(app: &mut App) {
        app.insert_resource(crate::portal::PortalState::default());
    }

    /// The rule the drive plays: only a change of the near door asks the far door anything, an
    /// opening at the point of the swing the near door has reached, and a close for a near door
    /// that starts closing or shuts outright.
    #[test]
    fn the_far_door_is_asked_to_follow_only_a_change_of_the_near_door() {
        use DoorState::*;
        let open = |fraction| Some(FarDoorFollow::Open { fraction });
        let cases = [
            (
                None,
                Opening,
                Some(0.25),
                open(0.25),
                "a first look at an opening door",
            ),
            (
                None,
                Opening,
                None,
                open(0.0),
                "an opening door that cannot be asked starts it",
            ),
            (
                None,
                Open { animated: true },
                None,
                open(1.0),
                "an open one opens it fully",
            ),
            (
                None,
                Open { animated: false },
                None,
                open(1.0),
                "a door with no clip too",
            ),
            (
                None,
                Closing,
                Some(0.5),
                Some(FarDoorFollow::Close),
                "a closing one closes it",
            ),
            (
                Some(false),
                Opening,
                Some(0.1),
                open(0.1),
                "the near door opens",
            ),
            (
                Some(true),
                Open { animated: true },
                Some(1.0),
                None,
                "the same swing going on",
            ),
            (
                Some(true),
                Closing,
                None,
                Some(FarDoorFollow::Close),
                "the near door closes",
            ),
            (
                Some(true),
                Closed,
                None,
                Some(FarDoorFollow::Close),
                "or shuts in one frame",
            ),
            (
                Some(false),
                Closed,
                None,
                None,
                "and a closing swing going on asks nothing",
            ),
        ];
        for (was_open, near, fraction, wanted, what) in cases {
            assert_eq!(far_door_follow(was_open, near, fraction), wanted, "{what}");
        }
    }

    /// The window shows the far door's own leaf, so it has to stand where the near door's does: the
    /// far door opens with the near one, at the same point of its own `Open` clip, and the two
    /// swings go on together.
    #[test]
    fn the_far_door_opens_with_the_near_door_at_the_same_point_of_the_swing() {
        let mut app = door_app();
        let near = animated_door(&mut app);
        let far = spawn_far_animated_door(&mut app, near.door);
        activate(&mut app, near.door);
        step(&mut app, 2);
        assert_eq!(
            state(&app, far.door),
            DoorState::Closed,
            "a far door the portal is not drawing into is left alone"
        );

        portal_draws_through(&mut app, near.door, far.door);
        step(&mut app, 1);
        assert_eq!(state(&app, near.door), DoorState::Opening);
        assert_eq!(state(&app, far.door), DoorState::Opening);
        let (near_at, far_at) = (
            swing_fraction(&app, &near).expect("the near door swings"),
            swing_fraction(&app, &far).expect("the far door swings"),
        );
        assert!(
            (near_at - far_at).abs() <= STEP_SECONDS / CLIP_SECONDS + 1e-4,
            "the far door is put where the near door's swing is: {near_at} against {far_at}"
        );
        assert!(
            app.world().resource::<FarDoorDrive>().holds_open(far.door),
            "and the auto-close leaves it to its near door"
        );

        step(&mut app, 12);
        assert_eq!(state(&app, near.door), DoorState::Open { animated: true });
        assert_eq!(state(&app, far.door), DoorState::Open { animated: true });
        assert_eq!(swing_fraction(&app, &far), Some(1.0));
    }

    /// And it closes with it: `E` at the near door, or the auto-close, starts the far door's own
    /// `Close` clip - also once the portal has stopped drawing through the doorway, which a door
    /// closed from far away has - and the pair is let go once both are shut.
    #[test]
    fn the_far_door_closes_with_the_near_door() {
        let mut app = door_app();
        let near = animated_door(&mut app);
        let far = spawn_far_animated_door(&mut app, near.door);
        portal_draws_through(&mut app, near.door, far.door);
        activate(&mut app, near.door);
        step(&mut app, 12);
        assert_eq!(state(&app, far.door), DoorState::Open { animated: true });

        portal_draws_nothing(&mut app);
        activate(&mut app, near.door);
        assert_eq!(state(&app, near.door), DoorState::Closing);
        assert_eq!(
            state(&app, far.door),
            DoorState::Closing,
            "the far door closes in the frame the near one starts to"
        );
        assert!(!app.world().resource::<FarDoorDrive>().holds_open(far.door));

        step(&mut app, 12);
        assert_eq!(state(&app, near.door), DoorState::Closed);
        assert_eq!(state(&app, far.door), DoorState::Closed);
        assert!(
            app.world().resource::<FarDoorDrive>().pairs.is_empty(),
            "a doorway shut at both ends is let go"
        );
    }

    /// A pair on the door layer is one door: `E` at the follower - the door on the player's side
    /// after a crossing - works the driver, whose clips pose the door-layer door, and the follower
    /// swings with it; the pair is held through a close, so the next `E` works it again
    /// (`crate::door_layer`).
    #[test]
    fn e_at_a_door_layer_follower_works_its_driver_and_the_follower_follows() {
        let mut app = door_app();
        let driver = animated_door(&mut app);
        let follower = spawn_far_animated_door(&mut app, driver.door);
        app.insert_resource(crate::door_layer::DoorLayer::holding_for_test(&[(
            driver.door,
            follower.door,
        )]));
        activate(&mut app, follower.door);
        step(&mut app, 1);
        assert_eq!(
            state(&app, driver.door),
            DoorState::Opening,
            "the driver opens"
        );
        assert_eq!(
            state(&app, follower.door),
            DoorState::Opening,
            "and the follower with it"
        );
        step(&mut app, 12);
        assert_eq!(state(&app, driver.door), DoorState::Open { animated: true });
        assert_eq!(
            state(&app, follower.door),
            DoorState::Open { animated: true }
        );

        activate(&mut app, follower.door);
        step(&mut app, 12);
        assert_eq!(state(&app, driver.door), DoorState::Closed);
        assert_eq!(state(&app, follower.door), DoorState::Closed);
        assert_eq!(
            app.world().resource::<FarDoorDrive>().pairs.len(),
            1,
            "a door-layer pair is held while the layer holds it"
        );
        activate(&mut app, follower.door);
        step(&mut app, 1);
        assert_eq!(state(&app, follower.door), DoorState::Opening);
    }

    /// A far door the player works with their own hands is theirs: the drive acts on a change of
    /// the near door and on nothing else, so it does not open again a far door closed under an
    /// open near door (the player crossed and shut the door behind them).
    #[test]
    fn the_drive_does_not_fight_a_far_door_closed_by_hand() {
        let mut app = door_app();
        let near = animated_door(&mut app);
        let far = spawn_far_animated_door(&mut app, near.door);
        portal_draws_through(&mut app, near.door, far.door);
        activate(&mut app, near.door);
        step(&mut app, 12);
        portal_draws_nothing(&mut app);

        activate(&mut app, far.door);
        step(&mut app, 12);
        assert_eq!(state(&app, near.door), DoorState::Open { animated: true });
        assert_eq!(
            state(&app, far.door),
            DoorState::Closed,
            "the far door stays shut"
        );
    }
}
