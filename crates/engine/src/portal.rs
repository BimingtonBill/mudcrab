//! Portals through load doors: pre-streamed destination cells are kept off the main camera's
//! layers, only the door the portal renders through stops drawing its leaf, and the destination
//! behind that door is rendered through the doorway as a window. Every other load door draws its
//! own leaf, so a doorway the portal is not showing is a closed door and not a hole.
//!
//! # Why the isolation is needed
//!
//! `transition::plan_door_prestream` puts the destination behind every load door within
//! [`DOOR_PRESTREAM_RADIUS`] into `PrestreamCells`, and `streaming::spawn_cell` spawns those cells
//! exactly like the active ones: an interior root sits at the render origin and its references keep
//! the interior's absolute creation coordinates, while an exterior of another worldspace is placed
//! relative to the *current* [`RenderOrigin`]. Neither is where the player stands, and both are
//! drawn, so a pre-streamed cell appears as floating, misplaced geometry - and, because
//! `player::PlayerPlugin` ray-casts with `RayCastVisibility::Visible`, the player also collides
//! with it. Interiors are the worst case: while the camera is inside `Alftand01` the pre-streamed
//! Tamriel grid is placed around the render origin, which is right where the interior's own
//! references are.
//!
//! Every cell root is therefore classified each frame against the active space the streaming plan
//! uses (`streaming::cell_within_unload_radius`):
//!
//! | role | layers | visibility | seen by |
//! |---|---|---|---|
//! | [`CellRole::Active`] | its own (0, or 1 for water) | unchanged | main camera, reflection camera |
//! | [`CellRole::Destination`] | [`DESTINATION_LAYER`] | unchanged | the portal camera only |
//! | [`CellRole::Hidden`] | none | hidden | nothing |
//!
//! [`RenderLayers`] is what keeps a cell out of the main camera, which renders layers 0 and 1
//! (`app::setup_world`). Layers are read per mesh entity, not inherited, and a glTF scene spawns
//! its meshes over several frames as assets load - so the cell's role is **propagated**, not walked:
//! the root carries it as one `Propagate<RenderLayers>` (`bevy::app::HierarchyPropagatePlugin`,
//! added by `PortalPlugin` below and run in `PostUpdate` before the visibility that reads the
//! layers) and Bevy's own hierarchy propagation writes the value down the cell, including on a mesh
//! that appears frames later (the plugin's own `ChildOf` observer) and with an early-out on every
//! node already at the value. A node that has layers of its own - a reference whose model carries
//! the placed-object layer of `crate::render`, a water plane's surface layer, a light, a node a mod
//! gave layers to - is a source of its own ([`PortalLayerSource`]), carrying the cell's role applied
//! to its own layers ([`source_layers`]): it keeps them while its cell is active and follows the
//! role while it is not. The cell root's role is stamped as it is classified - and the moment
//! `streaming::spawn_cell` spawns it - so a settled cell costs one comparison per frame and no
//! writes at all. The ray cast ignores render layers, so a cell that is not the portal's destination
//! is *also* hidden with `Visibility` - that one *is* inherited, so it covers meshes that appear
//! later. The portal's destination stays visible: the portal camera renders layers, and a hidden
//! entity is skipped by every camera.
//!
//! # Doors
//!
//! A load door's own leaf is the other half of the same frame, and which frame is a doorway comes
//! from the door's state ([`crate::doors::DoorState`]): **the portal renders through a door that is
//! opening, open or closing** ([`portal_shows_through`], which `update_portal` picks its door by -
//! a closing door is one whose doorway is still the window, because its leaf is drawn swinging
//! across it), and
//! [`show_load_door_leaves`] hides a door's whole model only when there is no leaf that could be
//! drawn over the quad - a door with no animation of its own, and an auto-load marker, which has no
//! leaf at all. An **animated** door keeps its model and its leaves are
//! [`crate::door_animation`]'s: drawn while the `Open` clip swings them, hidden once it is open if
//! the swing did not clear the opening. `update_portal` publishes the door it picked in
//! [`PortalState`] and `show_load_door_leaves` puts that on the door roots in the same frame, so a
//! doorway is an opening with the destination in it or a closed door, and never a window drawn
//! over a leaf. Retargeting the portal swaps the two between one frame and the next.
//!
//! **The door layer** (`crate::door_layer`, `docs/design/door-layer.md`). A load door whose model
//! swings is not drawn by either space once the player is near it: one door-layer door, made from
//! the model of the door on the player's side and posed from that door's own nodes, stands in the
//! doorway on the main camera's layers, and both native references - this one and its twin behind
//! the doorway - are taken off every camera by [`isolate_cells`] (they stay for the ray casts). The
//! quad stands behind the door-layer door and depth decides what shows. A leaf that swings **past**
//! the doorway's plane, away from the player, is behind the quad and covered by it - the one case
//! the main view cannot draw, because the quad's depth composite hides the source space behind the
//! plane past a wall's depth - so the doorway image carries that door's leaf too: a second instance
//! on the portal camera's layer at the pose the doorway map carries the door-layer door to. The
//! quad samples the destination image by screen position, so a point at `M(P)` is drawn on the
//! pixel `P` is: the leaf seen through the doorway stands where the door-layer door's leaf is, with
//! its swing on it, depth-tested against the destination's own geometry. A crossing keeps the
//! door-layer door, re-expressed in the new space through the same map.
//!
//! Which side of a door is its front comes from the door's own link data rather than from its
//! model: [`door_frame`](crate::transition::door_frame) builds the frame the view is mapped through
//! from the door's outward direction, which the database reads off the link that leads back into
//! the door. Door models disagree about which of their own axes is their front (see
//! [`LoadDoor::outward`]), so the model's frame is only the fallback for a door nothing leads back
//! to. The doorway itself is the model's geometry, so its box is measured in the model's own frame
//! and then laid out in the front frame - the same one the view is mapped through - so that the
//! window covers the opening a player walks in through rather than standing edge-on to it
//! ([`doorway_in_frame`]). That frame, the door -> arrival map built on it and the distance in
//! front of the door are all defined in `crate::transition`, which the player's crossing (and the
//! boundary between one cell and the next) uses too: the portal and the crossing must agree on
//! where the destination is, or the swap at the doorway shows something else.
//!
//! A door the data supports carries a [`DoorAnchor`] ([`crate::doors::doorway_anchor`]), and the
//! *one* map `crate::transition`'s [`door_map`] builds from it is what places every part of this
//! module at once: the portal camera, the clip plane, the quad's frame and size, the doorway image's
//! leaf and the destination cells. The map pivots on the source **doorway's centre** and lands on the
//! destination **doorway's centre**, so the quad stands in the real doorway (not in a frame 11.6
//! degrees oblique to it) and the clip plane is the destination doorway's own plane - which is why
//! [`PortalState::destination_door`]'s model is hidden while it stands closed, and why it is swung
//! open with the door the portal draws through (impl-234) so that the window shows its own leaf.
//! A door without an anchor draws exactly what it always did.
//!
//! # The doorway image
//!
//! The window is a render target of the portal camera, sampled by the quad at the screen position
//! of each of its fragments, and it is made to be indistinguishable from the room behind it:
//!
//! * it covers the doorway's own rectangle of the screen and nothing more ([`doorway_render_rect`]):
//!   the portal camera projects only that rectangle of the main view (a `SubCameraView`), at one
//!   texel per window pixel, into a target the size of the rectangle ([`resize_portal_target`]),
//!   so the doorway has the same pixel density as the room around it and the rest of the frame
//!   is neither shaded nor, past the frustum it narrows ([`fit_portal_view`]), drawn;
//! * it carries the destination's scene-referred light rather than a clipped 8-bit copy of it
//!   ([`PORTAL_TEXTURE_FORMAT`]), so the main camera's tonemapper and bloom finish the doorway
//!   exactly as they finish the room - walked through, the same surface reads the same;
//! * it is lit and fogged by the *destination's* own records ([`update_destination_atmosphere`])
//!   rather than by the space the player is standing in - ambient, fog, clear colour and a sun,
//!   which is a `DirectionalLight` of its own on the portal camera's layer
//!   ([`PortalDestinationSun`]), because Bevy lights a view from the lights whose render layers
//!   meet that camera's and the engine's one sun carries no layers at all.
//!
//! Those are the *camera's* alone: the quad's material is unlit and does not tonemap what it
//! samples, because the frame it is composited into is tonemapped once, by the main camera.
//!
//! # Wiring
//!
//! `app.run` adds `PortalPlugin` once, for every run that opened the world, right after
//! `StreamingPlugin` (it needs `ActiveCell`, `EngineConfig`, `RenderOrigin` and `StreamingWorld`).
//! Everything the portal feature is comes through that one call:
//!
//! * the crossing ([`crate::transition::TransitionPlugin`]) in every such run - crossing a load door
//!   is not a run mode, and a benchmark run's schedule depends on it;
//! * the player ([`crate::player::PlayerPlugin`]) in a walked run, the demo tour
//!   ([`crate::demo_tour::DemoTourPlugin`]) and the shots run ([`crate::shots::ShotsPlugin`]) in the
//!   runs that asked for them;
//! * the doorway image, the door and model animation ([`crate::door_animation`],
//!   [`crate::model_animation`]) and the cell isolation in the runs that are looked at rather than
//!   measured.
//!
//! Which of those a run gets is decided here, from `EngineConfig` alone
//! ([`EngineConfig::interactive`](crate::config::EngineConfig::interactive)), so the portal block in
//! `app.rs` is one line and this plugin holds the rest. Nothing else about the rendering changes:
//! the destination cells are moved off the main camera's layers rather than the camera being granted
//! a new one, and the portal camera, its render target and the quad are all spawned here.
//!
//! `streaming::spawn_cell` inserts [`StreamedCellKey`] on the root it returns:
//!
//! ```ignore
//! root_commands.insert(crate::portal::StreamedCellKey(payload.key));
//! ```
//!
//! That is what makes the isolation exact. Without it a root's worldspace is not recoverable from
//! `CellRef` and `ExteriorCellGrid` alone, so two exterior cells at the same grid in different
//! worldspaces are told apart only by the active-space radius; the demo route does not collide
//! (Blackreach arrives at grid 5,4 while AlftandWorld has no cell there).
//!
//! # What is not done here
//!
//! One portal at a time (the nearest door). The destination view's lighting is per destination -
//! ambient, fog and clear colour on the portal camera, and a sun of its own on the portal camera's
//! layer (the engine's sun reaches no view but the main camera's and the reflection camera's, and
//! never reached this one) - but it is the *space's* sun, not the part of the destination that
//! stands in it: the direction is the engine sun's, and only the tint and the illuminance are the
//! destination's. Water surfaces of a pre-streamed cell sample the main camera's reflection
//! texture, which does not show the destination.

use crate::{
    config::EngineConfig,
    door_animation::DoorAnimation,
    door_layer::{DoorCopy, DoorCopyPart},
    doors::{
        DOORWAY_FLOOR_PROBE_DEPTH, DoorAnchor, DoorDestination, DoorLeaf, DoorState,
        DoorwayFloorsMeasured, LoadDoor,
    },
    profiling::ProfilingState,
    render::TerrainMaterial,
    // The doorway's own dome: the marker that gives its camera one, and the destination's colours.
    sky::SkyCamera,
    sky_dome::CameraSky,
    streaming::{ActiveCell, RenderOrigin, StreamingWorld, camera_grid},
    transition::{
        CrossingHeld, DOOR_PRESTREAM_RADIUS, DoorMap, destination_is_resident, destination_keys,
        distance_in_front_of_door, door_map,
    },
    world::{
        components::{
            CellRef, ExpectedModelBounds, ExteriorCellGrid, InstanceBounds, StreamedCellRoot,
            StreamingCamera, WaterSurface,
        },
        database::CellKey,
        lighting::{SpaceKey, SpaceLightingCatalog, space_key},
    },
};
use bevy::{
    app::{
        HierarchyPropagatePlugin, Inherited, Propagate, PropagateOver, PropagateSet, PropagateStop,
    },
    asset::embedded_asset,
    camera::primitives::Aabb,
    camera::{
        CameraUpdateSystems, ClearColorConfig, Hdr, RenderTarget, SubCameraView,
        primitives::Frustum,
        visibility::{RenderLayers, VisibilitySystems},
    },
    core_pipeline::{
        prepass::DepthPrepass,
        tonemapping::{DebandDither, Tonemapping},
    },
    light::{CascadeShadowConfig, CascadeShadowConfigBuilder, Cascades, SimulationLightSystems},
    math::{Affine3A, Vec3A, bounding::Aabb3d, primitives::ViewFrustum},
    mesh::{Indices, PrimitiveTopology},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    picking::mesh_picking::ray_cast::{
        Backfaces, MeshRayCast, MeshRayCastSettings, RayCastVisibility, ray_aabb_intersection_3d,
        ray_mesh_intersection,
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        occlusion_culling::OcclusionCulling,
        render_resource::{AsBindGroup, PipelineCache, TextureFormat},
    },
    shader::ShaderRef,
};
use bevy_rapier3d::prelude::{Collider, ColliderDisabled};
use std::{
    collections::{HashMap, HashSet},
    f32::consts::PI,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

/// The rendering layer pre-streamed destination cells are moved to. The main camera renders layers
/// 0 and 1 and the portal camera renders this one, so a destination is drawn in exactly one of the
/// two views.
pub const DESTINATION_LAYER: usize = 2;

/// The layers `app::setup_world` gives the main camera. A cell that belongs to the active space
/// has to end up on one of these.
#[cfg(test)]
const MAIN_CAMERA_LAYERS: [usize; 2] = [0, 1];

/// The portal camera renders before the main camera that samples its target, like the water
/// reflection camera one step later.
const PORTAL_CAMERA_ORDER: isize = -2;

/// The layer of the doorway quad. The main camera renders layers 0 and 1, and the water reflection
/// camera renders only layer 0, so a doorway quad on layer 1 - the layer the water surfaces are on
/// for the same reason - is in the view and not in the reflection of it.
const PORTAL_QUAD_LAYER: usize = 1;

/// The size the portal render target is built with before the main camera's own target size is
/// known.
///
/// [`resize_portal_target`] takes up the main camera's target size on the first frame
/// `camera_system` has computed it, which is the frame after the camera exists, and follows every
/// resize after that. This is what the target is until then, and what it keeps in a run with no
/// window at all (a headless run has no target info to read): the portal camera draws nothing
/// until a door is open, and no door is open in the frame a run starts in.
const PORTAL_TEXTURE_FALLBACK_SIZE: UVec2 = UVec2::new(1024, 576);

/// The largest the portal target may get, on either axis: the doorway image is drawn one for one
/// with the window's pixels up to this, and scaled down past it.
///
/// A doorway is one piece of the frame and the room around it is drawn at the main camera's own
/// target size, so matching that size is what makes the two indistinguishable - and a target
/// larger than the view cannot show more than one pixel per pixel of the view. The ceiling is what
/// stops a window far larger than any of the engine's own runs from allocating an absurd texture:
/// a maximised 8K display is 7680x4320, and at `Rgba16Float` that would be 265 MB for the target
/// plus the same again for the texture the camera renders into, for a doorway that covers a
/// fraction of the screen. 2560x1440 is 29.5 MB. Both axes take one factor, so a doorway keeps the
/// window's pixel aspect ratio instead of being stretched along an axis.
const PORTAL_TEXTURE_MAX_SIZE: UVec2 = UVec2::new(2560, 1440);

/// The step the doorway's screen rectangle is rounded out to, in window pixels, before the portal
/// target is sized to it ([`doorway_render_rect`]).
///
/// A target of another size is another texture (`Image` has no resize), and the doorway's
/// rectangle moves by a pixel or two with every step the player takes: sized to the exact
/// rectangle, the target would be reallocated nearly every frame of a walk up to a door. Rounded
/// out to 64 it changes only when an edge crosses a 64-pixel line, and the rectangle it renders is
/// never smaller than the doorway it is sampled by.
const PORTAL_RECT_QUANTUM: u32 = 64;

/// What the portal camera renders into, and so what the doorway quad samples.
///
/// **Float, not 8-bit.** `Rgba16Float` holds the values the destination's own shaders produced,
/// above white included. An 8-bit target clamps every value over 1.0 to 1.0 as the portal camera
/// writes it, and the main camera's tonemapper then maps all of them - a sunlit wall, a light
/// pool, a glow - to one flat value: the doorway reads as a washed-out page next to the same room
/// walked into, because the values that should have rolled off the top of the tonemapper's curve
/// arrived at its ceiling instead. The portal camera is still not tonemapped and still has no
/// bloom of its own: what it writes is the destination's scene-referred light, and the *main*
/// camera's tonemapper and bloom finish the doorway exactly as they finish the room around it.
///
/// **No second view format.** An sRGB view would encode the values on write and decode them on
/// sample - a round trip through 8-bit precision, which is the thing this format is here to avoid.
/// A float texture's view format *is* its format.
const PORTAL_TEXTURE_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// The doorway a door without converted bounds gets, in Creation units.
const DEFAULT_PORTAL_SIZE: Vec2 = Vec2::new(200.0, 300.0);

/// A doorway smaller than this on either axis is treated as a missing measurement.
const MIN_PORTAL_SIZE: f32 = 8.0;

/// How far in front of the doorway's own plane the quad sits. Zero: the doorway's plane is where
/// the window is exactly the size of the opening, and where the crossing fires
/// (`crate::player::player_walks_through_doors`), so the last frame with a window is the swap.
///
/// A standoff used to hold the quad eight units forward - over-covering the opening, with the wall
/// hiding the excess - and it left the doorway blank for the last ~16 units of the walk: the window
/// runs from the quad toward the eye, so a camera closer than the quad has walked past it, and the
/// destination is drawn nowhere else. The walk-through frames bracket it: a doorway that is black
/// over 17.9 to 8.4 units at eight, and never at zero.
const PORTAL_QUAD_OFFSET: f32 = 0.0;

/// A camera nearer than this to a door's plane - inside it, or behind it - has no portal through
/// it: the doorway's clip plane would pass through the eye, where the window has no content.
///
/// The window is what carries the destination, so the frame this drops in is also the last frame
/// the player may still be on the near side of the doorway without the swap having been made:
/// [`crate::player::player_walks_through_doors`] fires the crossing at this same distance in front
/// of the plane rather than a step later, which is the one step that would otherwise show neither
/// the window nor the destination (a black frame in the doorway).
pub(crate) const MIN_PORTAL_DOOR_DISTANCE: f32 = 1.0;

/// How many shadow cascades the doorway's own sun ([`PortalDestinationSun`]) is spawned with,
/// before [`place_destination_sun`] has seen the engine sun's: Bevy's own default, four, which is
/// also `app::SUN_SHADOW_CASCADES`.
///
/// **The count is always the engine sun's.** Bevy 0.19 cannot draw two directional lights whose
/// cascade counts differ: `check_dir_light_mesh_visibility` sizes one per-thread scratch list per
/// cascade of the light it is on, shares the lists between lights, and then reads every thread's
/// list at every cascade index of the current light - so a light with more cascades than the one
/// before it reads past the end of a list the other sized (`bevy_light-0.19.0/src/lib.rs:406`,
/// `:477`). Two cascades here and four on the engine sun panicked the first frame a doorway
/// opened ("index out of bounds: the len is 2 but the index is 2"). What makes the doorway's
/// shadows cheaper is therefore the *reach* of its cascades, not their number
/// ([`PORTAL_SUN_SHADOW_DISTANCE`]).
const PORTAL_SUN_SHADOW_CASCADES: usize = 4;

/// How far from the portal camera the doorway's sun casts shadows, in Creation units (about 43 m).
///
/// This is what makes the doorway's shadows cheap. Each cascade is a shadow pass over every caster
/// in its box, and the doorway's sun used to copy the engine sun's set whole - cascades reaching
/// 17,378 units - which is a second full shadow workload every frame a doorway is open, for a
/// window that covers a piece of the screen (research-175, section 3). What a doorway shows is
/// mostly near: the room behind a house door, the street in front of it. The same number of
/// cascades ([`PORTAL_SUN_SHADOW_CASCADES`] says why it cannot be fewer) over a sixth of the reach
/// draws a fraction of the casters, at finer texels.
///
/// The portal camera stands as far behind the destination doorway as the player stands in front of
/// the source one, so the distance is measured from where the player's eye maps to, as the engine
/// sun's is from the eye. A doorway the portal draws is within [`DOOR_PRESTREAM_RADIUS`], and what
/// is seen through it is mostly within a room's or a street's length of the doorway; past this the
/// destination is lit without shadows, which through a doorway is a few pixels of distant ground.
const PORTAL_SUN_SHADOW_DISTANCE: f32 = 3000.0;

/// How long the destination cells of a doorway keep their destination role after the portal stops
/// drawing through its door for a reason that is not the door's, in seconds (impl-225).
///
/// The portal stops drawing through a door whenever [`select_portal_door`] no longer picks it: the
/// player stepped behind the door's plane or out of its reach, or looked so that another rule
/// dropped it. Each time, the destination cells went `Destination -> Hidden` and back the moment
/// the doorway was picked again, and a role change re-layed the root's whole subtree
/// ([`isolate_cells`]; 879 nodes each way at the Riverwood Trader, impl-223 - one write on the root
/// since the layer propagation). Held, a turn away and back re-layers nothing. The hold ends at
/// once when the door stops showing a way through (it closes), when its far side is no longer
/// resident, when it is no longer a door of the active space (a crossing), and when another door's
/// destination is drawn instead.
pub(crate) const DESTINATION_HOLD_SECONDS: f64 = 2.0;

/// The far bound of the doorway sun's first cascade, in Creation units (about 8.6 m): the doorway
/// and the first steps past it, where a shadow is largest on screen and needs the finest texels.
const PORTAL_SUN_FIRST_CASCADE: f32 = 600.0;

/// The near bound of the doorway sun's first cascade, in Creation units: the engine sun's own
/// 10 cm (`app::sun_shadow_cascades`). Nothing nearer the portal camera than the doorway is drawn
/// anyway - its projection is clipped there ([`portal_projection`]).
const PORTAL_SUN_SHADOW_NEAR: f32 = 7.0;

/// The overlap between the doorway sun's cascades: the engine sun's own proportion
/// (`app::sun_shadow_cascades`), so a shadow fades from one cascade into the next the same way on
/// both sides of the doorway.
const PORTAL_SUN_CASCADE_OVERLAP: f32 = 0.2;

/// Registers the crossing, the portal shader, and the systems that isolate cells, close load doors
/// and render the destination through the nearest doorway.
///
/// Add it for every run that opened the world, after
/// [`StreamingPlugin`](crate::streaming::StreamingPlugin): the crossing is not a run mode - every
/// run that streams cells crosses load doors - and the rest of it is registered from here by asking
/// the engine's own configuration what sort of run this is
/// ([`EngineConfig::interactive`](crate::config::EngineConfig::interactive)). It belongs to one
/// `add_plugins` call, so that a merge from `main` has one portal line to keep.
pub struct PortalPlugin;

/// Where a run wants the camera to start: `--start-position` / `--start-yaw`, or the position a
/// `--demo <name>` starts at, as a transform in the run's own render space.
///
/// [`app::setup_world`](crate::app) reads this instead of the configuration, so the world's startup
/// carries no portal branch: the seam is a general one - "start the camera here, for whatever
/// reason a run has" - and a run with no start of its own simply leaves it unset and gets the
/// default placement.
#[derive(Resource, Debug, Clone, Copy)]
pub struct StartPose(pub Transform);

/// Eye height above a start position, matching the player controller's standing eye height.
const START_EYE_HEIGHT: f32 = 120.0;

/// The portal's own frame, in order: the door it renders through, the door layer, the
/// destination's sun and atmosphere, the leaves of the doors it draws, and the isolation of the
/// cells.
///
/// One set, because the whole of it has to be right for the frame it is a picture of: the frame's
/// crossing is applied before it ([`crate::transition::DoorTransition`]), and so is a door state a
/// crossing ends in - `crate::door_animation`'s arrival opening, which is what the far door of a
/// mapped crossing is drawn as in the frame the player arrives in it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PortalFrame;

impl Plugin for PortalPlugin {
    fn build(&self, app: &mut App) {
        // The crossing and the pre-stream plan: every run that opened the world, which is what
        // `StreamingPlugin` adding this used to mean. A benchmark run keeps that part of its
        // schedule, and a fixture run does not gain it.
        app.add_plugins(crate::transition::TransitionPlugin);
        // An automated run's window starts parked, with its taskbar entry, and comes on screen
        // while it has focus (`crate::window_parking`).
        if app
            .world()
            .get_resource::<EngineConfig>()
            .is_some_and(EngineConfig::window_offscreen)
        {
            app.add_plugins(crate::window_parking::WindowParkingPlugin);
        }
        // The sort of run this is, read from the configuration: a pure function of it, so this
        // plugin and `app::run` cannot disagree (H7). The shots run is a value rather than a flag -
        // `app::run` built it before the window existed, because it sizes that window (H3) - and it
        // is handed over as a resource. The start pose is built here for the same reason: it is a
        // portal idea (H5) that `app::setup_world` reads as a plain resource.
        let (interactive, walking, demo_tour, shots, start_pose) = {
            let config = app.world().resource::<EngineConfig>();
            let start_pose = config.start_position.map(|position| {
                let origin = app.world().resource::<RenderOrigin>().0;
                StartPose(
                    Transform::from_translation(
                        crate::streaming::render_position(Vec3::from_array(position), origin)
                            + Vec3::Y * START_EYE_HEIGHT,
                    )
                    .with_rotation(crate::transition::arrival_camera_rotation(
                        [0.0, 0.0, config.start_yaw],
                    )),
                )
            });
            (
                config.interactive(),
                config.walks(),
                config.portal.demo_tour.clone(),
                app.world()
                    .get_resource::<crate::shots::ShotsRun>()
                    .cloned(),
                start_pose,
            )
        };
        if let Some(pose) = start_pose {
            app.insert_resource(pose);
        }
        if walking {
            // The player drives the `StreamingCamera` itself. Every other run keeps the scripted
            // `fly_camera`, which `app::run` registers (H4).
            app.add_plugins(crate::player::PlayerPlugin);
        }
        if interactive {
            // The demo's scripted tour and the objective line a walked demo shows: the plugin adds
            // whichever of the two this run has.
            app.add_plugins(crate::demo_tour::DemoTourPlugin {
                output_dir: demo_tour,
            });
        }
        if let Some(run) = shots {
            app.add_plugins(crate::shots::ShotsPlugin { run });
        }
        if !interactive {
            return;
        }
        embedded_asset!(app, "shaders/portal.wgsl");
        // The isolation's own machinery: Bevy's propagation of `RenderLayers` down a cell, and the
        // observers that stamp a cell the frame it spawns and keep a node's own layers
        // (`add_cell_layer_isolation`, which `isolate_cells` and its docs are about).
        add_cell_layer_isolation(app);
        app.add_plugins(MaterialPlugin::<PortalMaterial>::default())
            .init_resource::<PortalState>()
            .add_plugins(crate::door_layer::DoorLayerPlugin)
            // The quad reads PortalTexture, which the camera setup inserts through commands:
            // chaining adds the sync point that applies them in between.
            .add_systems(Startup, (setup_portal_camera, setup_portal_quad).chain())
            // Each animated door's doorway, from its leaves, before the portal sizes a window by it.
            .add_systems(Update, measure_leaf_openings.before(PortalFrame))
            .add_systems(
                Update,
                (
                    // The portal picks its door from the roles of the previous frame and publishes
                    // the destination cells; the isolation below reveals them in this same frame,
                    // which is what the roles would otherwise need the next frame for. It also
                    // measures the rectangle of the screen the doorway covers.
                    update_portal,
                    // After it, in the same frame: a doorway on screen but behind a wall is not
                    // rendered either (impl-201).
                    skip_occluded_doorway,
                    // After it, in the same frame: the target is sized to the rectangle just
                    // measured, and a resize repoints the camera's target and the quad's material
                    // at one new image, so the frame the camera projects that rectangle is the
                    // frame it renders into a target of its size and the quad maps through it.
                    resize_portal_target,
                    // After it, so the doorway image's leaf is for the door the portal picked this
                    // frame; before the isolation, which hides the natives it puts on the door
                    // layer (`crate::door_layer`).
                    crate::door_layer::manage_door_layer,
                    // The doorway is drawn with the atmosphere of the space it is looking into
                    // in the same frame the door is picked.
                    update_destination_atmosphere,
                    // The doorway's own sun onto the engine sun's direction. Here rather than in
                    // `PostUpdate`: it is a light, and the light extraction of this frame is the
                    // frame it should be right for. After the atmosphere, whose illuminance
                    // decides whether it casts shadows this frame.
                    place_destination_sun,
                    // After it, so the leaf of the door the portal just picked is gone in the same
                    // frame as the quad that replaces it - and one frame after it is dropped, the
                    // leaf is back. Before the isolation, which is about cells rather than doors.
                    show_load_door_leaves,
                    isolate_cells,
                    // After it, in the same frame: the colliders of the roles it just published are
                    // enabled for the active space and disabled for every other cell.
                    gate_cell_colliders,
                    // After it: the destination is revealed, so its floor can be found.
                    anchor_on_measured_floors,
                )
                    .chain()
                    .in_set(PortalFrame)
                    // A crossing changes `ActiveCell` in this set, and the cell just entered has to
                    // be visible in the frame it is entered in.
                    .after(crate::transition::DoorTransition),
            )
            // After Bevy has written the portal camera's aspect ratio and frustum from the whole
            // projection, and before the visibility checks and the cascades read them.
            .add_systems(
                PostUpdate,
                fit_portal_view
                    .after(CameraUpdateSystems)
                    .after(VisibilitySystems::UpdateFrusta)
                    .before(VisibilitySystems::CheckVisibility)
                    .before(SimulationLightSystems::UpdateDirectionalLightCascades),
            )
            // After Bevy has built every shadowed sun's cascades for every active camera, and
            // before it culls shadow casters against them: each sun keeps only the cascades of
            // the views that draw it (impl-226).
            .add_systems(
                PostUpdate,
                keep_cascades_of_views_that_draw_their_light
                    .after(SimulationLightSystems::UpdateDirectionalLightCascades)
                    .before(SimulationLightSystems::UpdateLightFrusta),
            );
        // The doorway composited by depth (impl-211's spike, the default since impl-227): the quad
        // above gets the composite's material. `--portal-depth-composite=off` keeps the plain
        // quad, for comparison.
        if app
            .world()
            .resource::<EngineConfig>()
            .portal
            .depth_composite
        {
            app.add_plugins(depth_composite::DepthCompositePlugin);
        }
        // The graphics setting `portal_every` (`--portal-every N`, the graphics panel): the portal
        // camera renders one frame in `N` while a doorway stays in view ([`cadence`]). Always
        // added, so the panel can change it live; at the default of every frame it never skips.
        app.add_plugins(cadence::PortalCadencePlugin);
        // The eye adjusts on crossing (`crate::eye_adjust`, docs/design/door-layer.md note 05): the
        // target rises toward full resolution as the player nears the doorway - measured after
        // this frame's doorway is placed and before the target is sized - and the new space comes
        // into focus after a crossing.
        app.add_plugins(crate::eye_adjust::EyeAdjustPlugin)
            .add_systems(
                Update,
                crate::eye_adjust::track_portal_approach
                    .in_set(PortalFrame)
                    .after(update_portal)
                    .before(resize_portal_target),
            );
        // One log line per doorway opened: the frame times and the pipelines created over the
        // frames after it ([`measure_portal_opens`]). The counts are read in the render world.
        let counts = PipelineCounts::default();
        app.insert_resource(counts.clone())
            .add_systems(Update, measure_portal_opens.after(PortalFrame));
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(counts)
                .add_systems(Render, count_pipelines.in_set(RenderSystems::Cleanup));
        }
        // The state of every load door - opened by `E`, swung by the model's own `Open` clip, and
        // read back by the portal and the crossing - and a model's own looping `Idle` clip on every
        // reference that is not a door: the mill wheel the Riverwood finale looks across the river
        // at, and the dust on a log pile. Both interactive runs only, and each needs nothing but
        // the asset server.
        app.add_plugins(crate::door_animation::DoorAnimationPlugin);
        app.add_plugins(crate::model_animation::ModelAnimationPlugin);
    }
}

/// The cell a reference root belongs to, carried by the root `streaming::spawn_cell` returns.
///
/// The streaming side inserts this; without it the isolation falls back to matching a root's
/// `CellRef` (interiors, which have unique cell ids) and its `ExteriorCellGrid` (exteriors, where
/// the worldspace is not recoverable).
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component)]
pub struct StreamedCellKey(pub CellKey);

/// The render target the portal camera draws into. Public so a tool can inspect it.
#[derive(Resource, Clone)]
pub struct PortalTexture(pub Handle<Image>);

/// The second camera that renders a destination cell.
#[derive(Component)]
pub(crate) struct PortalCamera;

/// The sun the doorway image is lit by: a second [`DirectionalLight`], on the portal camera's
/// layer, carrying the destination space's own sun ([`update_destination_atmosphere`]) in the
/// engine sun's direction ([`place_destination_sun`]).
///
/// **Why the engine's sun cannot light this view.** Bevy builds each view's directional lights by
/// intersecting the light's own `RenderLayers` with the *camera's*
/// (`bevy_pbr-0.19.0/src/render/light.rs:1642-1654`; `ExtractedDirectionalLight::render_layers` is
/// the light entity's component, `:124`, `:815`). The engine's one sun (`app::setup_world`) is
/// spawned with no `RenderLayers` at all, which is layer 0, and the portal camera is
/// `RenderLayers::layer(DESTINATION_LAYER)`, layer 2 - so before this entity existed the doorway
/// was lit by no directional light whatever space stood behind it: a daylit Riverwood street seen
/// from inside a house was drawn with ambient and fog alone.
///
/// **Why this is not a change to the engine's sun.** The main camera (layers 0 and 1) and the water
/// reflection camera (layer 0) never see this light, and this light never sees the engine's - one
/// directional light per view either way, so `MAX_DIRECTIONAL_LIGHTS` is untouched, and the shadow
/// cascades are budgeted per *view*, which is why the doorway can have a cascade set of its own
/// without taking one from the main camera (`sun_shadow_cascades`'s note in `app.rs` says the same).
#[derive(Component)]
struct PortalDestinationSun;

/// The quad in the doorway that shows the portal camera's image: a window, not a wall.
///
/// The walking player must be able to pass it - the doorway it stands in is the way through, and
/// the quad is the image of the room on the far side - so [`crate::player`]'s walk probe skips it
/// exactly as it skips a water surface. The geometry behind it (the wall the door is set into, the
/// leaf of a door that is still closed) is what stops them.
#[derive(Component)]
pub(crate) struct PortalQuad;

/// The material of the doorway quad: it samples the portal render target by screen position, so
/// the doorway is a window rather than a picture.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct PortalExtension {
    #[texture(100)]
    #[sampler(101)]
    portal_texture: Option<Handle<Image>>,
    /// The rectangle of the main view the portal target holds, in window pixels: `x`, `y` of its
    /// top-left corner, then its width and height ([`doorway_render_rect`]). A fragment of the
    /// quad samples the target at its own position within this rectangle. Zero - no rectangle
    /// measured, as in a run whose main camera has no size yet - means the whole main view.
    #[uniform(102)]
    doorway_rect: Vec4,
}

impl MaterialExtension for PortalExtension {
    fn fragment_shader() -> ShaderRef {
        "embedded://engine/shaders/portal.wgsl".into()
    }
}

/// The quad's material: the portal image, unlit, sampled through screen-space UVs.
pub type PortalMaterial = ExtendedMaterial<StandardMaterial, PortalExtension>;

/// What a cell root is to the view this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellRole {
    /// Part of the active space: drawn, and collided with.
    Active,
    /// The pre-streamed destination the portal renders through: drawn by the portal camera only.
    Destination,
    /// Any other resident cell: drawn by no camera, and not collided with.
    Hidden,
}

/// The roles of the current frame, the destination cells of the current portal, and the door it is
/// rendering through.
#[derive(Resource, Default)]
pub(crate) struct PortalState {
    roles: HashMap<Entity, CellRole>,
    destination: Vec<CellKey>,
    /// The door the portal is rendering through: the quad stands in its doorway. A door with no
    /// animation of its own has no leaf that can be swung out of the way, so this is what hides its
    /// whole model ([`show_load_door_leaves`]); an animated door keeps its frame, and its leaves
    /// are [`crate::door_animation`]'s to draw or hide.
    ///
    /// `None` whenever no portal is up - no camera to place, no door it draws through in range
    /// ([`portal_shows_through`]), or a run without the portal at all - which is when every load
    /// door draws its own leaf.
    open_door: Option<Entity>,
    /// The **destination** door of the doorway the portal is drawing through: the reference the
    /// open door's link lands at, when it is spawned in one of the resident cells.
    ///
    /// Since impl-234 this is the far door whose **own leaf** the window shows:
    /// `crate::door_animation`'s far-door drive ([`drawn_door_pair`](Self::drawn_door_pair)) puts
    /// it in the near door's `Open` clip fraction, so the leaf seen through the window and the leaf
    /// seen from the far side are one entity, hinged on one jamb (research-174 note 1). The mirror
    /// is only drawn when this is `None`.
    ///
    /// Its model is hidden while it has no swing under way, and for one reason: under a
    /// doorway anchor the window's clip plane is the *destination doorway's* own plane, so the
    /// destination door - which stands in exactly that plane - would, closed, be drawn across the
    /// whole aperture, a closed door where the room should be. Today's map hides that leaf by
    /// accident, because the clip plane stands tens of units inside the room and the destination
    /// door is inside the clipped slab; moving the clip to the doorway brings it back, and this is
    /// the deliberate answer to it
    /// (`docs/research/portal-door-alignment.md` section 9.3). Nothing is hidden on a door whose
    /// map did not move - its destination door is clipped away as it always was.
    destination_door: Option<Entity>,
    /// The rectangle of the main view the doorway was last drawn in ([`doorway_render_rect`]):
    /// what the portal camera projects, what [`resize_portal_target`] sizes the target to and what
    /// the quad maps its fragments through.
    ///
    /// Kept while no doorway is drawn - the portal camera is off then and nothing samples the
    /// target - so a doorway that leaves the screen and comes back does not reallocate it. `None`
    /// until a doorway is measured, and whenever the main camera has no size to measure it
    /// against: the target then follows the main camera's whole view.
    render_rect: Option<URect>,
    /// The doorway the portal is drawing through this frame ([`OpenDoorway`]), or `None` exactly
    /// when [`open_door`](Self::open_door) is. Written by `update_portal` and read by nothing in
    /// this module: it is what other modules need to know about the doorway without a second
    /// opinion about where it is.
    doorway: Option<OpenDoorway>,
    /// The door whose destination [`destination`](Self::destination) is, and since when it has not
    /// been drawn through ([`DESTINATION_HOLD_SECONDS`]).
    hold: Option<DestinationHold>,
    /// The isolation's churn over the run, for the log ([`IsolationChurn`]).
    churn: IsolationChurn,
    /// The native load doors the door layer draws in their place this frame
    /// ([`crate::door_layer::DoorLayer::hidden_natives`]): [`isolate_cells`] makes each of them a
    /// source of no layers at all ([`PortalLayerSource`]), so their whole model - the glTF scene
    /// included - is off every camera whatever the role of their cell, while they stay where they
    /// are for the ray casts that read visibility.
    door_layer_hidden: HashSet<Entity>,
    /// The doors [`isolate_cells`] last hid for the door layer: one that leaves the set is given
    /// its cell's layers back.
    door_layer_applied: HashSet<Entity>,
}

/// The door the portal last drew through and its destination cells, kept for
/// [`DESTINATION_HOLD_SECONDS`] after the portal stops drawing through it (impl-225).
#[derive(Debug, Clone, PartialEq)]
struct DestinationHold {
    door: Entity,
    destination: Vec<CellKey>,
    /// The clock time (seconds) the portal stopped drawing through the door, or `None` while it
    /// still does.
    lost_at: Option<f64>,
}

/// How often [`isolate_cells`] re-layered a resident cell and saw a cell come back with a new root,
/// over the run: impl-225's churn counts, logged at debug level with running totals.
#[derive(Debug, Default)]
struct IsolationChurn {
    /// Role changes of a root that already had a role: each one writes the root's source and the
    /// own-layer sources under it, and never walks the cell.
    relayer_changes: u64,
    /// The own-layer sources those changes wrote ([`PortalLayerSource`]).
    relayer_sources: u64,
    /// Roots of a cell that had been seen before under another root: a despawn and a respawn.
    respawns: u64,
    /// The last root seen for each cell.
    roots: HashMap<CellIdentity, Entity>,
}

impl IsolationChurn {
    /// A root's role changed from `from` (`None` for a root seen for the first time) to `to`, at
    /// `sources` own-layer writes under it.
    fn restamped(
        &mut self,
        identity: CellIdentity,
        from: Option<CellRole>,
        to: CellRole,
        sources: u64,
    ) {
        let Some(from) = from else {
            return;
        };
        self.relayer_changes += 1;
        self.relayer_sources += sources;
        debug!(
            cell = ?identity,
            ?from,
            ?to,
            sources,
            changes = self.relayer_changes,
            changed_sources = self.relayer_sources,
            respawns = self.respawns,
            "portal churn: cell re-layered"
        );
    }

    /// A root seen for the first time: a respawn when its cell had another root before.
    fn appeared(&mut self, identity: CellIdentity, root: Entity) {
        if self
            .roots
            .insert(identity, root)
            .is_some_and(|previous| previous != root)
        {
            self.respawns += 1;
            debug!(
                cell = ?identity,
                changes = self.relayer_changes,
                changed_sources = self.relayer_sources,
                respawns = self.respawns,
                "portal churn: cell respawned with a new root"
            );
        }
    }
}

/// The doorway the portal is drawing through, as `update_portal` placed it this frame: the door,
/// the one [`DoorMap`] the window, the camera and the crossing are built on, and the quad standing
/// in the doorway (its translation is the doorway's centre, its scale the opening's width and
/// height, and it faces the side the player stands on).
///
/// `pub(crate)` for `crate::portal_spill`, which lights the two sides of the doorway from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpenDoorway {
    /// The source door: the load door in the active space whose doorway the window stands in.
    pub(crate) door: Entity,
    /// The source door -> destination map.
    pub(crate) map: DoorMap,
    /// The doorway quad's transform, in the source space.
    pub(crate) quad: Transform,
}

impl PortalState {
    /// The portal draws through `door` this frame, into `destination`: the destination role goes to
    /// those cells, and any hold of another door's ends.
    fn draw_destination(&mut self, door: Entity, destination: Vec<CellKey>) {
        self.destination.clone_from(&destination);
        self.hold = Some(DestinationHold {
            door,
            destination,
            lost_at: None,
        });
    }

    /// The portal draws through no door this frame. The destination of the door it last drew
    /// through is kept for [`DESTINATION_HOLD_SECONDS`] from the first such frame, `now`, while
    /// `still_holds(door)` says that door still shows a way through into it; otherwise it is
    /// dropped at once. A run without a clock (`now` of `None`) holds nothing.
    fn lose_destination(&mut self, now: Option<f64>, still_holds: impl FnOnce(Entity) -> bool) {
        let kept = match (&mut self.hold, now) {
            (Some(hold), Some(now)) if still_holds(hold.door) => {
                let lost_at = *hold.lost_at.get_or_insert(now);
                (now - lost_at < DESTINATION_HOLD_SECONDS).then(|| hold.destination.clone())
            }
            _ => None,
        };
        match kept {
            Some(destination) => self.destination = destination,
            None => {
                if self.hold.take().is_some_and(|hold| hold.lost_at.is_some()) {
                    debug!("portal: held destination released");
                }
                self.destination.clear();
            }
        }
    }

    /// The natives the door layer draws in place of this frame (`crate::door_layer`), for
    /// [`isolate_cells`] to hide.
    pub(crate) fn set_door_layer_hidden(&mut self, doors: HashSet<Entity>) {
        self.door_layer_hidden = doors;
    }

    /// The doorway the portal is drawing through this frame, if any ([`OpenDoorway`]). Set only
    /// while the portal draws through a door ([`portal_shows_through`]), including while that
    /// doorway is off screen.
    pub(crate) fn open_doorway(&self) -> Option<OpenDoorway> {
        self.doorway
    }

    /// The door the portal draws through this frame and the far door of its doorway
    /// ([`destination_door`](Self::destination_door)), when both are known: the pair
    /// `crate::door_animation`'s far-door drive swings together (impl-234).
    pub(crate) fn drawn_door_pair(&self) -> Option<(Entity, Entity)> {
        self.open_door.zip(self.destination_door)
    }

    /// A portal state drawing through `near` into the doorway of `far`, for the far-door drive's
    /// tests in `crate::door_animation`.
    #[cfg(test)]
    pub(crate) fn drawing_through(near: Entity, far: Entity) -> Self {
        Self {
            open_door: Some(near),
            destination_door: Some(far),
            ..Self::default()
        }
    }

    /// Whether the cell `entity` belongs to is part of the **active space**: the cells the
    /// streaming plan holds because the camera is in them, and the only ones the player walks in
    /// and takes doors from.
    ///
    /// A cell of another role is a space of its own, whose raw coordinates can lie anywhere over
    /// the player's: its meshes are drawn by the portal camera (`CellRole::Destination`) or hidden,
    /// and its references keep their components either way, so its floors, walls, load doors and
    /// invisible auto-load markers are all found by anything that walks the world. Nothing a
    /// visibility query looks at tells the two apart - `MeshRayCast`'s `RayCastVisibility::Visible`
    /// reads `InheritedVisibility`, which a destination cell keeps, and a legitimate auto-load
    /// marker is hidden on purpose.
    ///
    /// `true` for an entity that no cell root owns at all: the roles are keyed by the roots
    /// [`isolate_cells`] classifies, and geometry outside a streamed cell - and every run without a
    /// portal, where nothing is isolated - is the player's to walk in. The walk up the `ChildOf`
    /// chain is what finds the root; the map is the previous frame's, which is the space the camera
    /// is standing in now (the controller runs before the isolation, as `PlayerPlugin` and
    /// `PortalPlugin` register them).
    pub(crate) fn is_in_active_space(&self, entity: Entity, parents: &Query<&ChildOf>) -> bool {
        let mut cursor = Some(entity);
        while let Some(current) = cursor {
            if let Some(role) = self.roles.get(&current) {
                return *role == CellRole::Active;
            }
            cursor = parents.get(current).ok().map(ChildOf::parent);
        }
        true
    }
}

/// The visibility a cell root had before the isolation hid it.
#[derive(Component, Clone, Copy)]
struct PortalHiddenCell(Visibility);

/// The active space: the cells the streaming plan holds because the camera is in them, defined
/// exactly as `streaming::cell_within_unload_radius` defines it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActiveSpace {
    interior: Option<u32>,
    worldspace_id: u32,
    /// The camera's grid in `worldspace_id`.
    center: IVec2,
    radius: i32,
}

impl ActiveSpace {
    /// The active space of a camera at render-space position `camera` while `origin` is the render
    /// origin. Its centre is the grid `streaming::camera_grid` reads from that position, the one
    /// place the grid-to-render offset is read backwards, so it is the cell `streaming::plan_cells`
    /// streams around and never a hand-written copy of it that could drift.
    fn of(active: &ActiveCell, radius: i32, camera: Vec3, origin: IVec2) -> Self {
        Self {
            interior: active.interior,
            worldspace_id: active.worldspace_id,
            center: camera_grid(camera, origin),
            radius,
        }
    }

    fn contains(&self, key: CellKey) -> bool {
        match (key, self.interior) {
            (CellKey::Interior(cell_id), Some(interior)) => cell_id == interior,
            (CellKey::Interior(_), None) => false,
            (CellKey::Exterior { .. }, Some(_)) => false,
            (
                CellKey::Exterior {
                    worldspace_id,
                    grid_x,
                    grid_y,
                },
                None,
            ) => {
                worldspace_id == self.worldspace_id
                    && (grid_x - self.center.x).abs() <= self.radius
                    && (grid_y - self.center.y).abs() <= self.radius
            }
        }
    }
}

/// What a cell root says about which cell it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CellIdentity {
    /// The root carries [`StreamedCellKey`], so the cell is known exactly.
    Key(CellKey),
    /// A root without the marker and without an [`ExteriorCellGrid`] is an interior, and interior
    /// cell ids are unique.
    InteriorCell(u32),
    /// A root without the marker is an exterior whose worldspace is not in the components it
    /// carries; only its grid is known.
    ExteriorGrid(IVec2),
}

impl CellIdentity {
    fn of(cell_id: u32, grid: Option<&ExteriorCellGrid>, key: Option<&StreamedCellKey>) -> Self {
        match (key, grid) {
            (Some(key), _) => Self::Key(key.0),
            (None, Some(grid)) => Self::ExteriorGrid(grid.0),
            (None, None) => Self::InteriorCell(cell_id),
        }
    }

    fn is_active(self, space: &ActiveSpace) -> bool {
        match self {
            Self::Key(key) => space.contains(key),
            Self::InteriorCell(cell_id) => space.interior == Some(cell_id),
            Self::ExteriorGrid(grid) => {
                space.interior.is_none()
                    && (grid.x - space.center.x).abs() <= space.radius
                    && (grid.y - space.center.y).abs() <= space.radius
            }
        }
    }

    fn is_destination(self, destination: &[CellKey]) -> bool {
        destination.iter().any(|key| match (self, *key) {
            (Self::Key(identity), key) => identity == key,
            (Self::InteriorCell(cell_id), CellKey::Interior(key)) => cell_id == key,
            // Without the marker an exterior root's worldspace is unknown, so the grid decides.
            (Self::ExteriorGrid(grid), CellKey::Exterior { grid_x, grid_y, .. }) => {
                grid == IVec2::new(grid_x, grid_y)
            }
            _ => false,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// The doorway quad, clip plane and projection
// ---------------------------------------------------------------------------------------------

/// The `near_clip_plane` of the portal camera's projection, in its own view space.
///
/// The plane is the destination's side of the doorway: it passes through the destination doorway
/// (the link's `XTEL` arrival point, or the destination *doorway's* own centre under a
/// [`DoorAnchor`]) with that doorway's facing as its normal, which is the image of the source
/// doorway's plane under the mapping - exactly so under an anchor, where the map takes one doorway
/// onto the other, and the point of the whole change: on today's map the plane stands tens of units
/// inside the room and starts the visible destination there.
/// Bevy clips everything on the camera's side of it (`PerspectiveProjection::near_clip_plane`), so
/// the window shows the destination room and not the destination geometry the portal camera stands
/// among.
///
/// `w` is the *negative* distance from the camera to the plane along the normal, the sign
/// `PerspectiveProjection` expects - its default near plane is `(0, 0, -1, -near)`. `-w` is
/// therefore the distance in front of the doorway, and negative behind it.
fn doorway_clip_plane(
    portal_translation: Vec3,
    portal_rotation: Quat,
    doorway_point: Vec3,
    doorway_normal: Vec3,
) -> Vec4 {
    let view_from_world = portal_rotation.inverse();
    let normal = (view_from_world * doorway_normal).normalize();
    let distance = normal.dot(view_from_world * (doorway_point - portal_translation));
    normal.extend(-distance)
}

/// The portal camera's projection: the main camera's, clipped at the doorway.
///
/// Both the oblique plane and the near plane are put at the doorway. The oblique plane is the exact
/// one - it cuts the frustum along the doorway rather than perpendicular to the view axis - but
/// `PerspectiveProjection::adjust_perspective_matrix_for_clip_plane` skips the adjustment when the
/// plane's *normal* is exactly the view axis' `-Z`, which is exactly a camera looking straight at a
/// doorway. The near plane covers that case (and agrees with the oblique one everywhere else), so
/// either way the destination geometry between the portal camera and the doorway is clipped.
///
/// Neither touches the clip-space `w` or the projected `x`/`y` of anything beyond the doorway, so a
/// fragment of the quad samples the pixel of the destination on the same sight line and the doorway
/// lines up with the main view. A projection that is not perspective is passed through unchanged:
/// there is nothing to align, and screen-space UVs still line it up.
fn portal_projection(main: &Projection, clip_plane: Vec4, doorway_distance: f32) -> Projection {
    match main {
        Projection::Perspective(perspective) => Projection::Perspective(PerspectiveProjection {
            near: perspective.near.max(doorway_distance),
            near_clip_plane: clip_plane,
            ..*perspective
        }),
        other => other.clone(),
    }
}

/// The four corners of the doorway quad in world space, for the quad's `transform`.
///
/// The quad is [`setup_portal_quad`]'s unit plane facing `+Z`, scaled by [`update_portal`] to the
/// doorway's size, so its corners are the unit square's under that transform.
fn doorway_corners(transform: &Transform) -> [Vec3; 4] {
    [
        Vec3::new(-0.5, -0.5, 0.0),
        Vec3::new(0.5, -0.5, 0.0),
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::new(-0.5, 0.5, 0.0),
    ]
    .map(|corner| transform.transform_point(corner))
}

/// The rectangle of the window the doorway covers, in the window's pixels (origin top left, `y`
/// down, the way Bevy measures viewports), or `None` when the doorway covers none of it.
///
/// `corners` is the doorway's outline in world space ([`doorway_corners`]), `view` the main
/// camera's world pose, `projection` its projection, and `window` the size of what that camera
/// draws into. The outline is clipped against the camera's near plane before it is projected: a
/// doorway the player stands beside has corners on both sides of the eye, and a corner behind the
/// eye projects to the *opposite* side of the screen, so projecting the four corners as they are
/// would put the doorway where it is not. The clipping is done in clip space, where the near plane
/// is `w = near` under Bevy's reverse-Z perspective and the map from the world is linear, so the
/// points it adds on the edges are the edges' own. The projected outline's bounding box is then
/// clamped to the window.
///
/// `None` is every case with nothing to draw: the whole doorway behind the eye, off to a side of
/// the screen, or seen with no area at all (a camera standing in the doorway's own plane).
/// [`update_portal`] takes the no-portal path for all of them - the portal camera would render a
/// whole frame nothing samples - and the rectangle is what a doorway rendered at its own size
/// needs next.
fn doorway_screen_rect(
    corners: [Vec3; 4],
    view: &Transform,
    projection: &Projection,
    window: Vec2,
) -> Option<Rect> {
    if window.x <= 0.0 || window.y <= 0.0 {
        return None;
    }
    let clip_from_world = projection.get_clip_from_view() * view.to_matrix().inverse();
    // The near plane in clip `w`: `w` is the view depth under a perspective projection, and 1
    // under an orthographic one, which has no eye to be behind.
    let near = match projection {
        Projection::Perspective(perspective) => perspective.near,
        _ => 0.0,
    }
    .max(f32::EPSILON);
    let clip = corners.map(|corner| clip_from_world * corner.extend(1.0));

    // Sutherland-Hodgman against the one plane `w >= near`: a convex outline stays one convex
    // polygon, with at most one corner more than it had.
    let mut kept: Vec<Vec4> = Vec::with_capacity(clip.len() + 1);
    for (index, &current) in clip.iter().enumerate() {
        let next = clip[(index + 1) % clip.len()];
        let current_in = current.w >= near;
        if current_in {
            kept.push(current);
        }
        if current_in != (next.w >= near) {
            kept.push(current.lerp(next, (near - current.w) / (next.w - current.w)));
        }
    }
    if kept.is_empty() {
        return None;
    }

    let mut low = Vec2::splat(f32::INFINITY);
    let mut high = Vec2::splat(f32::NEG_INFINITY);
    for point in kept {
        let ndc = point.truncate().truncate() / point.w;
        let pixel = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * window;
        low = low.min(pixel);
        high = high.max(pixel);
    }
    let rect = Rect {
        min: low,
        max: high,
    }
    .intersect(Rect::from_corners(Vec2::ZERO, window));
    (!rect.is_empty()).then_some(rect)
}

/// The rectangle of the window the portal camera renders for a doorway covering `rect`
/// ([`doorway_screen_rect`]): `rect` rounded **out** to [`PORTAL_RECT_QUANTUM`] on every edge and
/// clamped to the window, or `None` when nothing of it is left.
///
/// Out, so every pixel the quad covers is inside it; in whole pixels, so the target's texels sit
/// on the window's pixels one for one and the doorway lines up with the room around it to the
/// pixel. The clamp comes after the rounding, so an edge at the window's own edge stays there
/// rather than rounding out past it: a rectangle touching the right or bottom edge has a size that
/// is not a multiple of the quantum, and it is still a stable size, because the window's is.
fn doorway_render_rect(rect: Rect, window: UVec2) -> Option<URect> {
    if rect.is_empty() {
        return None;
    }
    let quantum = PORTAL_RECT_QUANTUM as f32;
    let min = (rect.min / quantum).floor().max(Vec2::ZERO) * quantum;
    let max = ((rect.max / quantum).ceil() * quantum).min(window.as_vec2());
    let rect = URect::from_corners(min.as_uvec2(), max.max(min).as_uvec2());
    (rect.width() > 0 && rect.height() > 0).then_some(rect)
}

/// The portal camera's sub-view for a doorway rendered in `rect` of a main view `window` pixels
/// big: the cone through that rectangle of the main camera's frustum.
///
/// Bevy builds the projection of a sub-view as the off-axis frustum through the rectangle
/// (`PerspectiveProjection::get_clip_from_view_for_sub`) and then applies the projection's oblique
/// clip plane to it exactly as it does to the whole view, so the doorway's clip survives. What
/// lands at a pixel of the target is what the full projection lands at the matching pixel of the
/// rectangle - the same sight line, drawn by the same camera - which is why the quad can sample
/// the target by its own position in the rectangle.
fn doorway_sub_view(rect: URect, window: UVec2) -> SubCameraView {
    SubCameraView {
        full_size: window,
        offset: rect.min.as_vec2(),
        size: rect.size(),
    }
}

/// The rectangle as the quad's material carries it ([`PortalExtension::doorway_rect`]): its corner
/// and its size, in window pixels; zero for the whole main view.
fn doorway_rect_uniform(rect: Option<URect>) -> Vec4 {
    rect.map_or(Vec4::ZERO, |rect| {
        Vec4::new(
            rect.min.x as f32,
            rect.min.y as f32,
            rect.width() as f32,
            rect.height() as f32,
        )
    })
}

/// The doorway the quad covers: its size and its centre in the frame the door's front comes from
/// ([`door_frame`]).
///
/// The box itself is measured in the model's own frame ([`measured_doorway_box`]) and then laid
/// out in the frame the door faces, so the quad is one thing in one frame - the door's front
/// decides where it stands, which way it faces and how big it is ([`doorway_in_frame`]). A door
/// with no usable bounds at all (the invisible `AutoLoadDoor01` markers among them) gets
/// [`DEFAULT_PORTAL_SIZE`] standing on the reference's origin.
fn portal_quad_extents(
    instance_bounds: Option<&InstanceBounds>,
    expected_bounds: Option<&ExpectedModelBounds>,
    door_rotation: Quat,
    frame: Quat,
    scale: Vec3,
) -> (Vec2, Vec3) {
    match measured_doorway_box(instance_bounds, expected_bounds, door_rotation, scale) {
        Some((min, max)) => doorway_in_frame(min, max, door_rotation, frame),
        None => (
            DEFAULT_PORTAL_SIZE,
            Vec3::new(0.0, DEFAULT_PORTAL_SIZE.y * 0.5, 0.0),
        ),
    }
}

/// The doorway quad's transform for a door placed at `door_position`/`door_rotation` whose front
/// comes from `frame`: [`portal_quad_extents`]' size and centre, the quad standing in the doorway's
/// own plane and facing the side the player stands on.
///
/// A door with a [`DoorAnchor`] built on thresholds has its quad's bottom edge at that threshold
/// ([`anchored_threshold`], [`quad_on_threshold`]) instead of at the box's bottom: the portal camera
/// and the map's pivot stand the destination's threshold there, so that is where the destination
/// floor meets the doorway in the image. A quad reaching below it looks under the destination
/// floor, at the portal camera's clear colour (the band under Gerdur's House's doorway,
/// research-205), and a quad stopping above it leaves the source's own geometry showing between
/// the two floors.
///
/// One definition for the quad [`update_portal`] draws and the doorway it asks
/// [`select_portal_door`] about, so the doorway the pick measures on screen is the one drawn.
#[allow(clippy::too_many_arguments)]
pub(crate) fn doorway_quad_transform(
    door_position: Vec3,
    door_rotation: Quat,
    frame: Quat,
    scale: Vec3,
    instance_bounds: Option<&InstanceBounds>,
    expected_bounds: Option<&ExpectedModelBounds>,
    anchor: Option<&DoorAnchor>,
    leaves: Option<&LeafOpening>,
) -> Transform {
    let (size, centre) = portal_quad_extents(
        instance_bounds,
        expected_bounds,
        door_rotation,
        frame,
        scale,
    );
    let (size, centre) = narrowed_to_leaves(
        size,
        centre,
        leaves
            .and_then(|leaves| leaves.0)
            .map(|(min, max)| doorway_in_frame(min * scale, max * scale, door_rotation, frame)),
    );
    let (size, centre) = anchor
        .and_then(|anchor| anchored_threshold(door_rotation, frame, scale, anchor))
        .map_or((size, centre), |threshold| {
            quad_on_threshold(size, centre, threshold)
        });
    let front = frame * Vec3::NEG_Z;
    Transform {
        translation: door_position + frame * centre + front * PORTAL_QUAD_OFFSET,
        // `Plane3d` faces `+Z` and the player stands on the door's front (`-Z`).
        rotation: frame * Quat::from_rotation_y(PI),
        scale: Vec3::new(size.x, size.y, 1.0),
    }
}

/// The height in `frame` (over the door's reference) of the threshold a [`DoorAnchor`] stands its
/// source doorway on, or `None` when the anchor was built on the doorways' box *centres* and has no
/// threshold to give.
///
/// The anchor carries one height, [`DoorAnchor::source_anchor_height`]: the threshold when both
/// doorways were read as ones a floor meets, the box centre's height otherwise
/// (`crate::doors::doorway_anchor`). The two are told apart by the centre's own height. A threshold
/// is the box's bottom or a floor at most `ANCHOR_SUNK_CAP` above it, on a doorway at least
/// `ANCHOR_MIN_DOORWAY_HEIGHT` tall, so it stands well off its box centre; only a door sunk by very
/// nearly half its height could bring the two within [`THRESHOLD_FROM_CENTRE`], and that door keeps
/// the box's quad, as it did before.
///
/// The point read is the pivot `crate::transition::source_doorway_centre` places, in `frame`, so the
/// quad's bottom and the portal camera's anchor are one point.
fn anchored_threshold(
    door_rotation: Quat,
    frame: Quat,
    scale: Vec3,
    anchor: &DoorAnchor,
) -> Option<f32> {
    let box_centre = door_rotation * (Vec3::from_array(anchor.source_box_centre) * scale);
    if (anchor.source_anchor_height - box_centre.y).abs() <= THRESHOLD_FROM_CENTRE {
        return None;
    }
    let pivot = Vec3::new(box_centre.x, anchor.source_anchor_height, box_centre.z);
    let height = (frame.inverse() * pivot).y;
    height.is_finite().then_some(height)
}

/// How close to the box centre's height an anchor's height has to be to be read as the centre
/// rule's rather than a threshold ([`anchored_threshold`]), in units.
const THRESHOLD_FROM_CENTRE: f32 = 0.5;

/// The quad of `size` and `centre` (in the door's frame) with its bottom edge moved to `threshold`
/// and its top, sides and depth unchanged. A threshold that leaves less than [`MIN_PORTAL_SIZE`] of
/// doorway under the top keeps the box's quad.
///
/// No overlap below the threshold. The map stands the destination's threshold on this one, so a
/// pixel of the quad above it looks down onto the destination floor beyond the doorway's plane, and
/// a pixel below it looks under that floor, at nothing: the portal camera's clear colour. What
/// stands below the threshold on the source side (the sill, the ground) is drawn by the main
/// camera, as beside the quad's other edges.
fn quad_on_threshold(size: Vec2, centre: Vec3, threshold: f32) -> (Vec2, Vec3) {
    let top = centre.y + size.y * 0.5;
    if top - threshold < MIN_PORTAL_SIZE {
        return (size, centre);
    }
    (
        Vec2::new(size.x, top - threshold),
        Vec3::new(centre.x, (top + threshold) * 0.5, centre.z),
    )
}

/// How many frames in a row the portal may draw through a door whose floors the probe cannot find
/// before [`anchor_on_measured_floors`] gives up on it and keeps the anchor it has: about four
/// seconds at the demo's frame rate, time for the destination's floor to stream in and be
/// validated.
const FLOOR_PROBE_ATTEMPTS: u32 = 240;

/// How many quiet frames in a row the floors at a doorway have to hold before the door is marked
/// measured ([`DoorwayFloorsMeasured`]): about half a second at the demo's frame rate.
const FLOOR_PROBE_SETTLE_FRAMES: u32 = 30;

/// How far either floor at a doorway may move, in units, and still count as the same floor. A move
/// of more than this re-anchors the door at once and starts the count again.
const FLOOR_PROBE_TOLERANCE: f32 = 0.5;

/// How near either probe point, in units, a mesh that has just become hittable (its [`Aabb`] was
/// just computed) keeps a doorway's floors from settling, or clears a door's
/// [`DoorwayFloorsMeasured`]: a floor may have arrived that the probe did not see. A cell's mesh
/// spawns late when its model loads late, or when its scene is spawned again.
const FLOOR_PROBE_SETTLE_RADIUS: f32 = 512.0;

/// How far above and below the height an anchor already has the floor probe looks, in units: the
/// window [`DoorAnchor::on_measured_floors`] accepts a floor in.
const FLOOR_PROBE_REACH: f32 = crate::doors::ANCHOR_SUNK_CAP;

/// The meshes the floor probe at a doorway passes through: the quad, water and the doorway's mirror.
type NotAFloor = Or<(With<PortalQuad>, With<WaterSurface>, With<DoorCopyPart>)>;

/// The anchor the world database gave a door, kept aside the first time the portal draws through
/// it. Every floor probe starts from these heights and re-anchors from them
/// ([`anchor_on_measured_floors`]), never from a previous probe's.
#[derive(Component, Debug, Clone, PartialEq)]
struct DatabaseDoorAnchor(DoorAnchor);

/// Where the floor probe at a door stands: the floors it last anchored the door on, and for how
/// many quiet frames they have held.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
struct DoorwayFloorsProbe {
    /// The (source, destination) floors the door was last re-anchored from, each as a height over
    /// its own reference.
    applied: Option<(f32, f32)>,
    /// Quiet frames in a row the floors have held within [`FLOOR_PROBE_TOLERANCE`] of `applied`.
    held: u32,
    /// Frames in a row the probe found no floor on one side or both.
    missed: u32,
}

/// What [`DoorwayFloorsProbe::step`] tells the door to do this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
enum FloorProbeStep {
    /// Nothing yet: the floors have not held long enough, or not both were found.
    Wait,
    /// The floors moved (or were found for the first time): re-anchor the door on them.
    Apply((f32, f32)),
    /// The floors have held for [`FLOOR_PROBE_SETTLE_FRAMES`] quiet frames: mark it measured.
    Settled,
    /// No floor for [`FLOOR_PROBE_ATTEMPTS`] frames: keep the anchor it has and mark it measured.
    GiveUp,
}

impl DoorwayFloorsProbe {
    /// One frame of the probe: `floors` is what the two probes found this frame (`None` unless
    /// both found a floor), `quiet` whether no mesh near either probe point became hittable since
    /// the last frame.
    ///
    /// A frame counts towards [`FloorProbeStep::Settled`] only when it is quiet and its floors are
    /// within [`FLOOR_PROBE_TOLERANCE`] of the ones the door stands on; floors further off are
    /// applied at once and the count starts again, so a floor that arrives late (the probe fell
    /// through to the ground under a porch not yet spawned) moves the door onto it.
    fn step(&mut self, floors: Option<(f32, f32)>, quiet: bool) -> FloorProbeStep {
        let Some(floors) = floors else {
            self.held = 0;
            self.missed += 1;
            return if self.missed >= FLOOR_PROBE_ATTEMPTS {
                FloorProbeStep::GiveUp
            } else {
                FloorProbeStep::Wait
            };
        };
        self.missed = 0;
        let holds = self.applied.is_some_and(|(source, destination)| {
            (source - floors.0).abs() <= FLOOR_PROBE_TOLERANCE
                && (destination - floors.1).abs() <= FLOOR_PROBE_TOLERANCE
        });
        if !holds {
            self.applied = Some(floors);
            self.held = 0;
            return FloorProbeStep::Apply(floors);
        }
        if !quiet {
            self.held = 0;
            return FloorProbeStep::Wait;
        }
        self.held += 1;
        if self.held >= FLOOR_PROBE_SETTLE_FRAMES {
            FloorProbeStep::Settled
        } else {
            FloorProbeStep::Wait
        }
    }

    /// Measure again from the floors the door stands on: after a door marked measured is drawn
    /// through again, or a mesh near its doorway became hittable.
    fn rearm(&mut self) {
        self.held = 0;
        self.missed = 0;
    }
}

/// The door the portal is drawing through, re-anchored on the floors at its two doorways
/// ([`DoorAnchor::on_measured_floors`]): the floor just in front of the source doorway and the
/// floor just inside the destination one, each found by a ray cast straight down onto the resident
/// meshes of its own space, as the player's ground probe finds the ground.
///
/// Only a door whose database anchor stands on thresholds ([`anchored_threshold`]); a centre
/// anchor and an unanchored door are left alone. The destination point is the source point's image
/// through the door's own map, one [`DOORWAY_FLOOR_PROBE_DEPTH`] behind the doorway's plane, so the
/// two probes stand either side of the one doorway. The door's own model (its leaf and frame), the
/// doorway's mirror, the quad and water are not floors; the source probe sees only the active space
/// and the destination probe only the destination cells the portal is drawing, whose raw
/// coordinates can lie anywhere over the active space's.
///
/// **Probed until stable** (research-229, flake A). The probe only hits meshes whose bounds and
/// visibility were computed the frame before, so a floor that spawns late - a porch or platform of
/// another cell whose model is still loading - is missed and the ray falls through to the ground
/// under it. So the door is probed every frame the portal draws through it, always from the
/// database's anchor ([`DatabaseDoorAnchor`]), re-anchored whenever the floors move
/// ([`DoorwayFloorsProbe::step`]), and marked [`DoorwayFloorsMeasured`] only once the floors have
/// held for [`FLOOR_PROBE_SETTLE_FRAMES`] frames in which no cell mesh within
/// [`FLOOR_PROBE_SETTLE_RADIUS`] of either probe point became hittable. A measured door is probed
/// again when the portal starts drawing through it again, or when such a mesh appears (a scene
/// spawned again).
///
/// After [`isolate_cells`], which reveals the destination: its meshes can be hit once the visibility
/// systems have run on them, which is the frame after they are revealed. The re-anchored map takes
/// effect from the next frame's [`update_portal`], and the crossing, the mirror and the player's
/// doorway plane all read it from the same component.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn anchor_on_measured_floors(
    mut commands: Commands,
    state: Res<PortalState>,
    mut doors: Query<(
        &GlobalTransform,
        &Transform,
        &LoadDoor,
        &mut DoorAnchor,
        Option<&DatabaseDoorAnchor>,
        Option<&DoorwayFloorsProbe>,
        Has<DoorwayFloorsMeasured>,
    )>,
    mut ray_cast: MeshRayCast,
    parents: Query<&ChildOf>,
    load_doors: Query<(), With<LoadDoor>>,
    not_floors: Query<(), NotAFloor>,
    fresh_meshes: Query<(Entity, &GlobalTransform, &Aabb), Added<Aabb>>,
    mut last_door: Local<Option<Entity>>,
    mut profiler: ResMut<ProfilingState>,
) {
    // The whole body, the returns below included: the frame the portal draws through no doorway is
    // the common one, and it is the one this system's own cost has to be readable in.
    let started = Instant::now();
    let open = state.open_doorway();
    let opened_now = open.map(|doorway| doorway.door) != *last_door;
    *last_door = open.map(|doorway| doorway.door);
    let Some(doorway) = open else {
        profiler.record_elapsed("portal/floor_probe", started);
        return;
    };
    let Ok((global, local, door, mut anchor, database, probe, measured)) =
        doors.get_mut(doorway.door)
    else {
        profiler.record_elapsed("portal/floor_probe", started);
        return;
    };
    let database = match database {
        Some(DatabaseDoorAnchor(database)) => database.clone(),
        None => {
            commands
                .entity(doorway.door)
                .insert(DatabaseDoorAnchor(anchor.clone()));
            anchor.clone()
        }
    };
    let map = doorway.map;
    let door_position = global.translation();
    if anchored_threshold(global.rotation(), map.frame, local.scale, &database).is_none() {
        if !measured {
            commands.entity(doorway.door).insert(DoorwayFloorsMeasured);
        }
        profiler.record_elapsed("portal/floor_probe", started);
        return;
    }
    // The role of the cell a mesh belongs to, or `None` for a mesh that is no floor at all.
    let role_of = |entity: Entity| -> Option<CellRole> {
        let mut cursor = Some(entity);
        while let Some(current) = cursor {
            if load_doors.contains(current) || not_floors.contains(current) {
                return None;
            }
            if let Some(role) = state.roles.get(&current) {
                return Some(*role);
            }
            cursor = parents.get(current).ok().map(ChildOf::parent);
        }
        None
    };
    let front = map.frame * Vec3::NEG_Z;
    let source_point = map.pivot + front * DOORWAY_FLOOR_PROBE_DEPTH;
    let (destination_point, _) = map.pose(
        map.pivot - front * DOORWAY_FLOOR_PROBE_DEPTH,
        Quat::IDENTITY,
    );
    // Whether a cell mesh near either probe point became hittable since the last frame.
    let quiet = !fresh_meshes.iter().any(|(entity, transform, aabb)| {
        let centre = transform.transform_point(Vec3::from(aabb.center));
        let reach = FLOOR_PROBE_SETTLE_RADIUS
            + Vec3::from(aabb.half_extents).length() * transform.scale().max_element();
        let near_source = centre.distance(source_point) <= reach;
        let near_destination = centre.distance(destination_point) <= reach;
        (near_source || near_destination)
            && match role_of(entity) {
                Some(CellRole::Active) => near_source,
                Some(CellRole::Destination) => near_destination,
                _ => false,
            }
    });
    let mut probe = probe.copied();
    if measured {
        if !opened_now && quiet {
            profiler.record_elapsed("portal/floor_probe", started);
            return;
        }
        commands
            .entity(doorway.door)
            .remove::<DoorwayFloorsMeasured>();
        probe.get_or_insert_default().rearm();
        debug!(
            door = format_args!("{:08X}", door.ref_id),
            opened_now, "portal: measuring the floors at the doorway again"
        );
    }
    // Each probe looks around the height the *database* anchor gives its side, whatever a previous
    // probe moved the map to: the source over the door's reference, the destination over the
    // map's arrival point less what the current anchor moved it by.
    let mut floor_under = |point: Vec3, height: f32, role: CellRole| -> Option<f32> {
        let filter = |entity: Entity| role_of(entity) == Some(role);
        let settings = MeshRayCastSettings::default()
            .with_filter(&filter)
            .with_visibility(RayCastVisibility::Visible)
            .always_early_exit();
        let origin = Vec3::new(point.x, height + FLOOR_PROBE_REACH, point.z);
        let hits = ray_cast.cast_ray(Ray3d::new(origin, Dir3::NEG_Y), &settings);
        let (_, hit) = hits.first()?;
        (hit.distance <= 2.0 * FLOOR_PROBE_REACH).then_some(hit.point.y)
    };
    let source = floor_under(
        source_point,
        door_position.y + database.source_anchor_height,
        CellRole::Active,
    );
    let destination_offset = anchor.destination.anchor_height - database.destination.anchor_height;
    let destination = floor_under(
        destination_point,
        map.arrival_position.y - destination_offset,
        CellRole::Destination,
    );
    // Both as heights over each side's own reference: the source over the door's, the destination
    // over its own - the anchor's height there plus how far the floor stands off the anchor point.
    let floors = source.zip(destination).map(|(source, destination)| {
        (
            source - door_position.y,
            anchor.destination.anchor_height + (destination - map.arrival_position.y),
        )
    });
    let mut probe = probe.unwrap_or_default();
    let step = probe.step(floors, quiet);
    commands.entity(doorway.door).insert(probe);
    match step {
        FloorProbeStep::Wait => {}
        FloorProbeStep::Apply((source_floor, destination_floor)) => {
            let before = (
                database.source_anchor_height,
                database.destination.anchor_height,
            );
            let target = match database.on_measured_floors(source_floor, destination_floor) {
                Some(on_floors) => {
                    info!(
                        door = format_args!("{:08X}", door.ref_id),
                        source_threshold = before.0,
                        destination_threshold = before.1,
                        source_floor,
                        destination_floor,
                        step = (destination_floor - before.1) - (source_floor - before.0),
                        "portal: doorway re-anchored on the floors at its two doorways"
                    );
                    on_floors
                }
                None => {
                    info!(
                        door = format_args!("{:08X}", door.ref_id),
                        source_floor,
                        destination_floor,
                        "portal: the floors at the doorway are too far off its thresholds; kept"
                    );
                    database
                }
            };
            if *anchor != target {
                *anchor = target;
            }
        }
        FloorProbeStep::Settled => {
            commands.entity(doorway.door).insert(DoorwayFloorsMeasured);
            info!(
                door = format_args!("{:08X}", door.ref_id),
                source_floor = anchor.source_anchor_height,
                destination_floor = anchor.destination.anchor_height,
                "portal: the floors at the doorway have settled"
            );
        }
        FloorProbeStep::GiveUp => {
            commands.entity(doorway.door).insert(DoorwayFloorsMeasured);
            info!(
                door = format_args!("{:08X}", door.ref_id),
                source_found = source.is_some(),
                destination_found = destination.is_some(),
                "portal: no floor at the doorway; the anchor it has is kept"
            );
        }
    }
    profiler.record_elapsed("portal/floor_probe", started);
}

/// The doorway box as the frame the door's front comes from sees it: the size of the opening in
/// that frame's own plane, and the offset of the doorway's centre in that frame.
///
/// The doorway is the model's geometry, so its box is measured in the model's own axes; the side a
/// player walks in from is the door's *front*, which comes from the link data and disagrees with
/// the model's own axes for most doors ([`LoadDoor::outward`]). The box is therefore placed in
/// world space with the reference's rotation and read back in the frame's axes. That gives the
/// silhouette of the whole box - not just of the model's own `X`/`Y` face - because where the two
/// frames disagree by a quarter turn the face a player walks through is the box's `X`/`Z` side,
/// and a window measured off the model's face alone would stand edge-on to the doorway.
///
/// The offset keeps the box centre's depth in the frame: the plane the quad stands in is the one
/// through the doorway's centre, which is the plane [`crate::player::player_walks_through_doors`]
/// fires the crossing on. With one frame for both ([`door_frame`] returning the model's own
/// rotation, which is every door nothing leads back to) this is the box centre exactly, and the
/// quad is placed as it was before the front came from the link data.
fn doorway_in_frame(min: Vec3, max: Vec3, door_rotation: Quat, frame: Quat) -> (Vec2, Vec3) {
    let to_frame = frame.inverse() * door_rotation;
    let mut low = Vec2::splat(f32::INFINITY);
    let mut high = Vec2::splat(f32::NEG_INFINITY);
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let corner = (to_frame * Vec3::new(x, y, z)).truncate();
                low = low.min(corner);
                high = high.max(corner);
            }
        }
    }
    let depth = (to_frame * ((min + max) * 0.5)).z;
    (
        high - low,
        Vec3::new((low.x + high.x) * 0.5, (low.y + high.y) * 0.5, depth),
    )
}

/// The doorway's box in the door's own frame - the corner the model's bounds reach in each axis -
/// or `None` when the base has no usable bounds at all, the invisible `AutoLoadDoor01` markers
/// among them.
///
/// This is the one place that decides how big a door's doorway is, from the same two sources: the
/// converted model's bounds (model space, scaled by the reference) and, failing those, the placed
/// reference's [`InstanceBounds`] turned back into the door's frame. The auto-load trigger volume
/// in [`crate::player`] measures its opening with this function too
/// ([`measured_portal_extents`]), so "walking into the door" and "looking through it" are the same
/// doorway, and a door with no usable bounds gets no box and no volume of its own.
fn measured_doorway_box(
    instance_bounds: Option<&InstanceBounds>,
    expected_bounds: Option<&ExpectedModelBounds>,
    door_rotation: Quat,
    scale: Vec3,
) -> Option<(Vec3, Vec3)> {
    let measured = match (expected_bounds, instance_bounds) {
        (Some(bounds), _) => {
            let scaled = |v: Vec3| Vec3::new(v.x * scale.x, v.y * scale.y, v.z * scale.z);
            Some((scaled(bounds.min), scaled(bounds.max)))
        }
        (None, Some(bounds)) => {
            let inverse = door_rotation.inverse();
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for x in [bounds.min.x, bounds.max.x] {
                for y in [bounds.min.y, bounds.max.y] {
                    for z in [bounds.min.z, bounds.max.z] {
                        let corner = inverse * Vec3::new(x, y, z);
                        min = min.min(corner);
                        max = max.max(corner);
                    }
                }
            }
            Some((min, max))
        }
        (None, None) => None,
    };
    let (min, max) = measured?;
    // A box the wrong way round measures the same doorway as its own mirror image.
    let (min, max) = (min.min(max), min.max(max));
    (min.is_finite()
        && max.is_finite()
        && ((min + max) * 0.5).is_finite()
        && max.x - min.x >= MIN_PORTAL_SIZE
        && max.y - min.y >= MIN_PORTAL_SIZE)
        .then_some((min, max))
}

/// The doorway's size and centre in the door's own frame, or `None` when the base has no usable
/// bounds at all - the invisible `AutoLoadDoor01` markers among them, which is why their doorway
/// falls back to [`DEFAULT_PORTAL_SIZE`].
///
/// This is the model's own view of the box [`measured_doorway_box`] measures: the width and height
/// of its `X`/`Y` face and its centre. The auto-load trigger volume in [`crate::player`] builds
/// itself out of this, so a door's crossing volume is the model's doorway; the portal's window is
/// the same box seen in the frame the door faces ([`portal_quad_extents`]), which is the model's
/// own frame for a door whose link data agrees with its model.
pub(crate) fn measured_portal_extents(
    instance_bounds: Option<&InstanceBounds>,
    expected_bounds: Option<&ExpectedModelBounds>,
    door_rotation: Quat,
    scale: Vec3,
) -> Option<(Vec2, Vec3)> {
    let (min, max) = measured_doorway_box(instance_bounds, expected_bounds, door_rotation, scale)?;
    Some((Vec2::new(max.x - min.x, max.y - min.y), (min + max) * 0.5))
}

/// The box a load door's leaves stand in when the door is closed, in the door's own unscaled model
/// frame (the frame [`ExpectedModelBounds`] is in) - or `None` for a door whose leaves have no mesh
/// to measure. Measured once per door by [`measure_leaf_openings`].
///
/// This is the doorway itself, where the model's own bounds may be much more: Whiterun's main gate,
/// `WRDoorMainGate01`, carries its whole `GateHousing` - the posts and the carved panels either side
/// of the gateway - in the same model as its two leaves, so its bounds are 643 units wide around a
/// 424-unit opening. A window the width of the model drew the destination over the housing's side
/// panels (the user's capture of 2026-09-26: the carved panel beside the gate "missing" once the
/// gate opened, and a sliver of the destination's own gate standing in its place).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct LeafOpening(pub(crate) Option<(Vec3, Vec3)>);

/// How much narrower than the model's doorway box its leaves have to be before the window is
/// narrowed to them ([`narrowed_to_leaves`]): a share of the box's width. A door whose leaves fill
/// its model - every house door, whose model is a leaf and a plug the leaf's size - keeps the box's
/// window exactly as before; a model that carries a frame much wider than its opening does not.
const LEAF_NARROWING_SHARE: f32 = 0.85;

/// The doorway window's size and centre in the door's frame, narrowed across to the door's leaves
/// when they are much narrower than the box it was measured from ([`LeafOpening`]). Only the width
/// changes: the height, the depth and a threshold ([`quad_on_threshold`]) are the box's, so a door
/// whose leaves are shorter than its arch still shows the whole arch's opening.
///
/// `leaves` is the leaves' own size and centre in the same frame ([`doorway_in_frame`]). The window
/// is the part of the box the leaves span, never wider than the box.
fn narrowed_to_leaves(size: Vec2, centre: Vec3, leaves: Option<(Vec2, Vec3)>) -> (Vec2, Vec3) {
    let Some((leaf_size, leaf_centre)) = leaves else {
        return (size, centre);
    };
    if !(leaf_size.x.is_finite() && leaf_centre.x.is_finite())
        || leaf_size.x >= size.x * LEAF_NARROWING_SHARE
    {
        return (size, centre);
    }
    let low = (centre.x - size.x * 0.5).max(leaf_centre.x - leaf_size.x * 0.5);
    let high = (centre.x + size.x * 0.5).min(leaf_centre.x + leaf_size.x * 0.5);
    if high - low < MIN_PORTAL_SIZE {
        return (size, centre);
    }
    (
        Vec2::new(high - low, size.y),
        Vec3::new((low + high) * 0.5, centre.y, centre.z),
    )
}

/// The animated load doors [`measure_leaf_openings`] has not measured yet.
type UnmeasuredDoorQuery<'world, 'state> = Query<
    'world,
    'state,
    (Entity, &'static GlobalTransform, Option<&'static DoorState>),
    (With<LoadDoor>, With<DoorAnimation>, Without<LeafOpening>),
>;

/// Measures each animated load door's [`LeafOpening`] once: the meshes under the nodes its clips
/// move ([`DoorLeaf`]), in the door's own frame, while the door is closed and its leaves stand in
/// the doorway.
///
/// Waits - measures nothing, and tries again next frame - while a leaf mesh has no [`Aabb`] yet
/// (Bevy computes it after the mesh spawns), its leaves are not marked yet, or the door is not
/// closed. A door whose leaves have no mesh at all is given an empty opening, so it is not walked
/// again.
fn measure_leaf_openings(
    mut commands: Commands,
    doors: UnmeasuredDoorQuery,
    children: Query<&Children>,
    leaves: Query<&DoorLeaf>,
    meshes: Query<(&GlobalTransform, Option<&Aabb>), With<Mesh3d>>,
) {
    for (door, door_global, state) in &doors {
        if !matches!(state, Some(DoorState::Closed)) {
            continue;
        }
        let to_door = door_global.affine().inverse();
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        let mut waiting = false;
        let leaf_nodes: Vec<Entity> = children
            .iter_descendants(door)
            .filter(|node| leaves.get(*node).is_ok_and(|leaf| leaf.door == door))
            .collect();
        // The leaves are marked through commands after the animation is attached: none marked yet
        // is a door not ready to measure, not a door without leaves.
        if leaf_nodes.is_empty() {
            continue;
        }
        for node in leaf_nodes {
            for mesh in std::iter::once(node).chain(children.iter_descendants(node)) {
                let Ok((mesh_global, aabb)) = meshes.get(mesh) else {
                    continue;
                };
                let Some(aabb) = aabb else {
                    waiting = true;
                    continue;
                };
                let (min, max) = world_box(aabb, &(to_door * mesh_global.affine()));
                low = low.min(Vec3::from(min));
                high = high.max(Vec3::from(max));
            }
        }
        if waiting {
            continue;
        }
        let measured = (low.is_finite() && high.is_finite()).then_some((low, high));
        commands.entity(door).try_insert(LeafOpening(measured));
    }
}

/// Whether the portal draws its window through a door in this state: `Opening`, `Open` and
/// `Closing`.
///
/// This is the portal's own question about a [`DoorState`], and it is deliberately **wider** than
/// [`DoorState::is_open`] (the helper the crossing and the player's doorway trigger ask through
/// `crate::transition::door_is_open`, which must stay "fully open"). A `Closing` door is one the
/// player is watching shut: the `Close` clip swings the leaf back across the doorway and
/// `crate::door_animation` draws it there, and the far side belongs *behind* that leaf for the
/// whole swing - clearing the doorway the frame the swing starts emptied the doorway under the
/// closing leaf, so the player closing a door saw the source side through it (the user, in play,
/// 2026-09-25: "when closing a door, the portal stops rendering before it can fully close"). The
/// same holds while a door is `Opening`: the far side is there from the first frame of the swing,
/// not from the one the clip finishes on.
///
/// What makes `Closed` different is that nothing is drawn over the doorway any more: the window
/// stands in the only opening a door has, so a `Closed` door would put the destination image behind
/// its own closed leaf. A door with no [`DoorState`] at all counts as closed, like everywhere else.
fn portal_shows_through(state: Option<&DoorState>) -> bool {
    state.is_some_and(|state| {
        matches!(
            state,
            DoorState::Opening | DoorState::Open { .. } | DoorState::Closing
        )
    })
}

/// Whether a door in this state is one whose own animation moves its model - and therefore one with
/// a leaf that is drawn or hidden on its own account rather than by the portal
/// ([`crate::door_animation`]): the swing of an `Opening` or `Closing` door, and an `Open` one that
/// has a clip of its own.
///
/// False for a door with no clip at all, whose own model *is* its leaf (`Open { animated: false }`
/// is the state `DoorState::hides_whole_reference` answers true for), and for a closed one.
pub(crate) fn door_has_its_own_swing(state: Option<&DoorState>) -> bool {
    matches!(
        state,
        Some(DoorState::Opening | DoorState::Closing | DoorState::Open { animated: true })
    )
}

/// How much better aimed, in degrees, one on-screen doorway has to be than another before it wins
/// over a nearer one: within this the view cannot tell the two sight lines apart, and distance
/// decides. The slack the player's own door target takes
/// ([`crate::player::TARGET_AIM_SLACK_DEGREES`]).
const PORTAL_AIM_SLACK_DEGREES: f32 = crate::player::TARGET_AIM_SLACK_DEGREES;

/// The door the portal renders through: of the doors whose doorway the portal draws a window in
/// ([`portal_shows_through`] - `Opening`, `Open` or `Closing`) in the active space, whose
/// destination is resident and not itself part of the active space, and whose plane the camera is
/// on the front side of (a [`distance_in_front_of_door`] of at least
/// [`MIN_PORTAL_DOOR_DISTANCE`]), the one whose doorway the player **looks at**.
///
/// `aim` answers, per door, how far off the middle of the view its doorway is - the angle in
/// radians between the view's forward and the doorway's centre - or `None` when the doorway covers
/// none of the screen ([`doorway_screen_rect`]). A doorway on screen beats every doorway off it;
/// among those on screen the best aimed wins, the rule the player's own door target uses
/// (`crate::player::target_door`), with the nearer door taking a tie within
/// [`PORTAL_AIM_SLACK_DEGREES`]. With no doorway on screen the nearest door is kept, as before, so
/// its leaf, mirror and destination stay up while the player looks away and the frame it comes back
/// into view is the frame it is drawn in.
///
/// Nearest alone picked the wrong door wherever two open doorways stand close together: at the
/// Riverwood Trader the upper door `00070E69`, 179.8 units from the camera and 52 degrees above the
/// view, beat the front door `0001341F` the player was looking into from 220.5, so the front
/// doorway got no window and showed the door's own plug (research-193, the user's capture `01`).
/// The far door the portal hides and the mirror it places both follow this pick.
///
/// A doorway the portal draws in is the gate that makes the window a doorway: the quad stands in
/// the only opening a door has, so rendering through a `Closed` one would put the destination image
/// behind the door's own leaf. A door mid-swing keeps the window, because its leaf is drawn *over*
/// the window for the whole swing ([`portal_shows_through`] has the rest).
fn select_portal_door<'a>(
    camera: Vec3,
    doors: impl IntoIterator<
        Item = (
            Entity,
            Vec3,
            &'a LoadDoor,
            Option<&'a DoorState>,
            Option<&'a DoorAnchor>,
        ),
    >,
    destination_is_resident: impl Fn(&DoorDestination, Option<&DoorAnchor>) -> bool,
    active: &ActiveSpace,
    distance_in_front: impl Fn(Entity) -> f32,
    aim: impl Fn(Entity) -> Option<f32>,
) -> Option<Entity> {
    let slack = PORTAL_AIM_SLACK_DEGREES.to_radians();
    // The best so far: the door, how far off the view's middle its doorway is (`None` when it is
    // off screen) and how far away it stands.
    let mut best: Option<(Entity, Option<f32>, f32)> = None;
    for (entity, position, door, state, anchor) in doors {
        let distance = position.distance(camera);
        if distance > DOOR_PRESTREAM_RADIUS {
            continue;
        }
        if !portal_shows_through(state) {
            continue;
        }
        if !destination_is_resident(&door.destination, anchor) {
            continue;
        }
        // The destination is already drawn in the main view: nothing to look into. Under an anchor
        // the destination is the cell of the destination *reference*, which is what the crossing
        // lands in.
        if destination_keys(&door.destination, anchor)
            .iter()
            .any(|key| active.contains(*key))
        {
            continue;
        }
        if distance_in_front(entity) < MIN_PORTAL_DOOR_DISTANCE {
            continue;
        }
        let angle = aim(entity);
        let better = match best {
            None => true,
            Some((_, best_angle, best_distance)) => match (angle, best_angle) {
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => distance < best_distance,
                // Better aimed by more than the slack wins outright; within it the view cannot
                // tell the two apart, and the nearer one wins.
                (Some(angle), Some(best_angle)) => {
                    angle < best_angle - slack
                        || ((angle - best_angle).abs() <= slack && distance < best_distance)
                }
            },
        };
        if better {
            best = Some((entity, angle, distance));
        }
    }
    best.map(|(entity, ..)| entity)
}

// ---------------------------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------------------------

/// A resident cell root: the identity the isolation classifies, and the visibility it hid the root
/// with before.
type CellRootQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static CellRef,
        Option<&'static ExteriorCellGrid>,
        Option<&'static StreamedCellKey>,
        Option<&'static Visibility>,
        Option<&'static PortalHiddenCell>,
    ),
    With<StreamedCellRoot>,
>;

/// The main camera: the pose the portal camera follows, and the projection it copies.
///
/// Its **`Transform`**, not its `GlobalTransform`: the player moves the camera earlier in this same
/// `Update`, and `GlobalTransform` is only propagated in `PostUpdate`, so reading it here placed the
/// portal camera where the main camera was a frame ago - the doorway image trailed every move and
/// turn by one frame (the user saw it in play, 2026-09-24). The main camera is a root entity (spawned
/// on its own by `app::setup_world`), so its `Transform` is its world pose.
///
/// Its **`Camera`** is optional and read for one thing: the size of the window it draws into, which
/// [`doorway_screen_rect`] measures the doorway against. A main camera without one (the unit tests'
/// cameras), or one whose size `camera_system` has not filled in yet, is taken to see the doorway,
/// which is what the portal did before it asked.
type MainCameraQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static Transform,
        &'static Projection,
        Option<&'static Camera>,
    ),
    (
        With<StreamingCamera>,
        Without<PortalCamera>,
        Without<PortalQuad>,
    ),
>;

/// A load door reference with everything that places its doorway, and whether it is open.
type LoadDoorQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static GlobalTransform,
        &'static Transform,
        &'static LoadDoor,
        Option<&'static DoorState>,
        Option<&'static InstanceBounds>,
        Option<&'static ExpectedModelBounds>,
        Option<&'static DoorAnchor>,
    ),
    (Without<PortalCamera>, Without<PortalQuad>),
>;

/// The portal camera, and the doorway quad it draws through.
type PortalCameraQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static mut Transform,
        &'static mut Projection,
        &'static mut Camera,
    ),
    With<PortalCamera>,
>;

type PortalQuadQuery<'world, 'state> = Query<
    'world,
    'state,
    (&'static mut Transform, &'static mut Visibility),
    (With<PortalQuad>, Without<PortalCamera>),
>;

/// The main camera's own [`Camera`] component: what `camera_system` fills the render target's size
/// into, and so what [`resize_portal_target`] sizes the portal target from.
type MainCameraTargetQuery<'world, 'state> = Query<
    'world,
    'state,
    &'static Camera,
    (
        With<StreamingCamera>,
        Without<PortalCamera>,
        Without<PortalQuad>,
    ),
>;

/// The portal render target for a given size: a fresh image in [`PORTAL_TEXTURE_FORMAT`].
///
/// `Image` has no resize, so a target of another size is another texture - [`resize_portal_target`]
/// makes one and repoints the camera and the quad at it, rather than resizing the image in place.
fn portal_target_image(size: UVec2) -> Image {
    Image::new_target_texture(size.x, size.y, PORTAL_TEXTURE_FORMAT, None)
}

/// The size the portal target takes for a main camera rendering at `main` pixels, or `None` when
/// there is no size to follow.
///
/// One factor for both axes, so the doorway image keeps the main view's pixel aspect ratio: a cap
/// applied per axis on its own would stretch the doorway along whichever axis was not capped.
/// Sizes at or under the ceiling are followed exactly - the factor is then 1 - and anything larger
/// is scaled down uniformly.
///
/// `None` for a target with a zero axis, which is a minimized window rather than a size: a 0x0
/// texture is not a render target, so the caller keeps the one it has until the window comes back.
fn portal_target_size(main: UVec2, ceiling: UVec2) -> Option<UVec2> {
    if main.x == 0 || main.y == 0 {
        return None;
    }
    let main = main.as_vec2();
    let ceiling = ceiling.as_vec2();
    let scale = (ceiling / main).min_element().min(1.0);
    // Rounded rather than truncated, and clamped to the ceiling afterwards: the scale is a float,
    // so a size that should land exactly on the cap can land a fraction over it.
    let scaled = (main * scale).round().max(Vec2::ONE);
    Some(scaled.min(ceiling).as_uvec2())
}

/// The portal target's size for a rectangle `size` pixels big at the graphics settings' portal
/// `scale` ([`crate::graphics_settings::scaled_portal_size`], which holds the scale to 0.1..=1),
/// rounded to whole texels and then held to [`PORTAL_TEXTURE_MAX_SIZE`] by [`portal_target_size`].
/// At scale 1 this is `portal_target_size` itself. `None` for a zero axis, as there.
///
/// The quad samples the target by its position in the rectangle (`portal.wgsl`), not by texel,
/// so a smaller target is the same view at a lower resolution, filtered up.
fn scaled_portal_target_size(size: UVec2, scale: f32) -> Option<UVec2> {
    if size.x == 0 || size.y == 0 {
        return None;
    }
    let scaled = crate::graphics_settings::scaled_portal_size(size.as_vec2(), scale).round();
    portal_target_size(scaled.as_uvec2(), PORTAL_TEXTURE_MAX_SIZE)
}

/// Grows or shrinks the portal render target to the rectangle of the screen the doorway covers
/// ([`PortalState::render_rect`]), so the doorway is drawn one texel per window pixel - the
/// resolution of the room around it - and no larger than the doorway is.
///
/// Until a doorway has been measured the target follows the main camera's whole view, from its
/// *computed* target info (`Camera::computed.target_info`, filled by `camera_system` from the
/// camera's `RenderTarget`): the physical size of whatever that camera draws into. On the first
/// frame of a run that is not computed yet, and a run with no window has none at all: both keep
/// [`PORTAL_TEXTURE_FALLBACK_SIZE`]. Either size is held to [`PORTAL_TEXTURE_MAX_SIZE`].
///
/// **Nothing is allocated unless the size changes**, and what the decision is keyed on is the size
/// of the image the resource already points at - `Image::size`, not a copy of the last request, so
/// there is one source of truth for "how big is the target" and an unchanged rectangle does no
/// work at all; the rectangle is rounded to [`PORTAL_RECT_QUANTUM`] for exactly this. A size that
/// does change is a new image, with the camera's `RenderTarget` and the quad's material repointed
/// at it in the same frame: the material's bind group is rebuilt from the changed asset, and the
/// camera would otherwise render into the new texture while the doorway still sampled the old one.
/// The rectangle the quad maps through ([`PortalExtension::doorway_rect`]) is written in that same
/// frame too, and only when it changed.
#[allow(clippy::too_many_arguments)]
fn resize_portal_target(
    state: Option<Res<PortalState>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<PortalMaterial>>,
    mut texture: ResMut<PortalTexture>,
    main: MainCameraTargetQuery,
    mut targets: Query<&mut RenderTarget, With<PortalCamera>>,
    quad: Query<&MeshMaterial3d<PortalMaterial>, With<PortalQuad>>,
    settings: Option<Res<crate::graphics_settings::GraphicsSettings>>,
    approach: Option<Res<crate::eye_adjust::PortalApproach>>,
) {
    // `--portal-scale` / the graphics panel's knob, read every frame so a live change resizes the
    // target on the next one; raised toward full resolution as the player nears the doorway
    // (`crate::eye_adjust`), which writes the effective scale from the setting each frame.
    let scale = approach.map_or_else(
        || settings.map_or(1.0, |settings| settings.portal_scale),
        |approach| approach.scale,
    );
    // The doorway's own rectangle when [`update_portal`] has measured one this run, and the main
    // camera's whole view until it has.
    let rect = state.and_then(|state| state.render_rect);
    let size = match rect {
        Some(rect) => scaled_portal_target_size(rect.size(), scale),
        None => main.single().ok().and_then(|camera| {
            camera
                .computed
                .target_info
                .as_ref()
                .and_then(|info| scaled_portal_target_size(info.physical_size, scale))
        }),
    };
    let resized = size
        .filter(|&size| images.get(&texture.0).map(Image::size) != Some(size))
        .map(|size| {
            let image = images.add(portal_target_image(size));
            texture.0 = image.clone();
            if let Ok(mut target) = targets.single_mut() {
                *target = RenderTarget::Image(image.clone().into());
            }
            debug!(
                width = size.x,
                height = size.y,
                scale,
                "portal: render target resized to the doorway's rectangle"
            );
            image
        });

    // The rectangle the quad maps its fragments through changes with the target's contents, in the
    // same frame; the material is only touched when something did change, since a changed material
    // is a rebuilt bind group.
    let doorway_rect = doorway_rect_uniform(rect);
    let Ok(handle) = quad.single() else {
        return;
    };
    let stale = materials
        .get(handle)
        .is_some_and(|material| material.extension.doorway_rect != doorway_rect);
    if (resized.is_some() || stale)
        && let Some(mut material) = materials.get_mut(handle)
    {
        if let Some(image) = resized {
            material.extension.portal_texture = Some(image);
        }
        material.extension.doorway_rect = doorway_rect;
    }
}

/// Fits the portal camera's culling and shadow frustum to the view it actually renders, after
/// Bevy's camera update and before anything reads them.
///
/// Two of Bevy's own systems see the portal camera's *projection* without its sub-view:
///
/// * `camera_system` sets the projection's aspect ratio to the render target's, which is now the
///   doorway rectangle's and not the main view's. The sub-view's projection does not read it, but
///   the directional-light cascades are built from `Projection::get_frustum_corners`, which does:
///   the destination sun's cascades would cover a frustum as narrow as the doorway is, centred on
///   the view axis rather than on the doorway. The main camera's aspect ratio is put back, so the
///   cascades cover the main view's frustum - the one the sub-view is a piece of - as they did
///   before the doorway was rendered at its own size.
/// * `update_frusta` builds the culling frustum from the whole projection, so everything in the
///   main view's cone would still be extracted, sorted and have its vertices run. The frustum is
///   rebuilt from the sub-view's projection instead: the cone through the doorway's rectangle,
///   with the oblique doorway plane as its near plane, so what cannot be seen through the doorway
///   is not drawn at all.
fn fit_portal_view(
    main: Query<&Projection, (With<StreamingCamera>, Without<PortalCamera>)>,
    mut portal: Query<
        (&Camera, &GlobalTransform, &mut Projection, &mut Frustum),
        With<PortalCamera>,
    >,
) {
    let Ok((camera, transform, mut projection, mut frustum)) = portal.single_mut() else {
        return;
    };
    if let (Ok(Projection::Perspective(main)), Projection::Perspective(portal)) =
        (main.single(), projection.as_ref())
        && portal.aspect_ratio != main.aspect_ratio
        && let Projection::Perspective(portal) = projection.as_mut()
    {
        portal.aspect_ratio = main.aspect_ratio;
    }
    let (true, Some(sub_view)) = (camera.is_active, camera.sub_camera_view.as_ref()) else {
        return;
    };
    *frustum = sub_view_frustum(&projection, sub_view, transform);
}

/// The culling frustum of a camera that renders `sub_view` of `projection`: the frustum of the
/// sub-view's own clip matrix, with the projection's far distance.
fn sub_view_frustum(
    projection: &Projection,
    sub_view: &SubCameraView,
    transform: &GlobalTransform,
) -> Frustum {
    let clip_from_world =
        projection.get_clip_from_view_for_sub(sub_view) * transform.to_matrix().inverse();
    Frustum(ViewFrustum::from_clip_from_world_custom_far(
        &clip_from_world,
        &transform.translation(),
        &transform.back(),
        projection.far(),
    ))
}

fn setup_portal_camera(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = images.add(portal_target_image(PORTAL_TEXTURE_FALLBACK_SIZE));
    commands.insert_resource(PortalTexture(image.clone()));
    commands
        .spawn((
            Name::new("Portal camera"),
            Camera3d::default(),
            Camera {
                order: PORTAL_CAMERA_ORDER,
                is_active: false,
                ..default()
            },
            // The destination is drawn from the doorway's own clip plane to the shared far plane, and
            // handed over untonemapped ([`portal_camera_output`]).
            portal_camera_output(),
            RenderTarget::Image(image.clone().into()),
            Projection::Perspective(PerspectiveProjection::default()),
            Transform::default(),
            Msaa::Off,
            DepthPrepass,
            OcclusionCulling,
            RenderLayers::layer(DESTINATION_LAYER),
            // The destination's own atmosphere, written every frame the doorway is open by
            // [`update_destination_atmosphere`]. All three are components on this camera and not on the
            // main one, which is what lets a doorway open off a lit hall into a black cave and show the
            // cave: `AmbientLight` overrides `GlobalAmbientLight` for this view alone, `DistanceFog` is
            // per view, and so is the clear colour. They start at the engine's daylight defaults - this
            // camera draws nothing until a portal is up.
            AmbientLight::default(),
            DistanceFog::default(),
            PortalCamera,
            // The graphics settings that are part of the main view's pipeline key (SSAO, contact
            // shadows, the shadow filter, TAA's prepass and jitter) and its exposure, written by
            // `crate::graphics_settings` so the doorway's pipelines stay the main view's.
            crate::graphics_settings::GraphicsCamera::Portal,
        ))
        // The camera's own sky dome (`crate::sky_dome`): `SkyCamera` is the marker a dome is spawned
        // for, and `CameraSky` is the colours it is drawn with. Cleared here, and written by
        // [`update_destination_atmosphere`] with the destination's own the frame a doorway opens in, so
        // the dome is never drawn over a space this camera has not been told about. In a second
        // `insert`, not the spawn tuple: Bevy's tuple bundles stop at fifteen components.
        .insert((SkyCamera, CameraSky { palette: None }));
    // The doorway's own sun, next to the camera it lights and on that camera's layer alone. Off
    // until [`update_destination_atmosphere`] gives it the destination's own - the frame a doorway
    // first opens in is the frame that writes it - and pointing nowhere in particular until
    // [`place_destination_sun`] has the engine's sun to copy: `app::setup_world` spawns that one,
    // and no doorway is drawn before it has.
    commands.spawn((
        Name::new("Portal destination sun"),
        PortalDestinationSun,
        DirectionalLight {
            color: Color::WHITE,
            illuminance: 0.0,
            shadow_maps_enabled: false,
            ..default()
        },
        portal_sun_cascades(PORTAL_SUN_SHADOW_CASCADES),
        Transform::default(),
        RenderLayers::layer(DESTINATION_LAYER),
    ));
}

/// How the portal camera writes its image: HDR, untonemapped and undithered.
///
/// **Not tonemapped:** the quad's material hands the image to the main camera's tonemapper, and
/// tonemapping it here as well would darken the doorway against the room around it. What it hands
/// over is not a clipped 8-bit copy of the destination either - the target is float
/// ([`PORTAL_TEXTURE_FORMAT`]), so the values above white reach that tonemapper intact.
///
/// **`Hdr`** makes this view's mesh pipeline key the main camera's. Bevy adds the tonemap and
/// dither bits (`TONEMAP_IN_SHADER | TONEMAP_METHOD_NONE | DEBAND_DITHER`) only to a view that is
/// not HDR, and those bits were the whole difference between the two views, so every material
/// variant a destination showed was specialized and compiled again on the first open of each
/// doorway (research-197, `local/research/portal-first-open-hitch-2026-09-25.md`): 38 pipelines
/// and 18 frames over 25 ms on the Riverwood route's first door. With the bits gone the doorway
/// reuses what the main view already compiled: 11 pipelines and 3 frames over 25 ms since, and
/// those 11 are the view's own tonemapping and blit pipelines plus material and shadow variants no
/// view had drawn before (the room's own materials, which walking in would compile anyway). The
/// image does not change: the view renders into
/// an `Rgba16Float` intermediate - the format of the target - and with `Tonemapping::None` Bevy's
/// tonemapping pass returns before drawing, so what reaches the target is a straight copy of the
/// scene-referred values.
///
/// **`DebandDither::Disabled`**, so no pass of this view dithers the doorway: the main camera
/// dithers it along with the room around it. (`Camera3d` requires `DebandDither::Enabled`.)
fn portal_camera_output() -> (Hdr, Tonemapping, DebandDither) {
    (Hdr, Tonemapping::None, DebandDither::Disabled)
}

/// How many frames after a doorway opens [`measure_portal_opens`] watches.
const OPEN_WINDOW_FRAMES: u32 = 60;

/// A frame longer than this, in milliseconds, is counted as a hitch by [`measure_portal_opens`].
const OPEN_HITCH_MS: f32 = 25.0;

/// The render world's pipeline cache, as two numbers the main world can read: every render and
/// compute pipeline ever queued, and those still compiling. Written once a frame by
/// [`count_pipelines`] (the two worlds share the one allocation), read by [`measure_portal_opens`].
#[derive(Resource, Clone, Default)]
struct PipelineCounts(Arc<PipelineCountsInner>);

#[derive(Default)]
struct PipelineCountsInner {
    created: AtomicU32,
    waiting: AtomicU32,
}

impl PipelineCounts {
    fn read(&self) -> (u32, u32) {
        (
            self.0.created.load(Ordering::Relaxed),
            self.0.waiting.load(Ordering::Relaxed),
        )
    }
}

/// Render world: publishes the pipeline cache's size and its waiting count, after this frame's
/// queue has been processed.
fn count_pipelines(cache: Res<PipelineCache>, counts: Res<PipelineCounts>) {
    let created = u32::try_from(cache.pipelines().count()).unwrap_or(u32::MAX);
    let waiting = u32::try_from(cache.waiting_pipelines().count()).unwrap_or(u32::MAX);
    counts.0.created.store(created, Ordering::Relaxed);
    counts.0.waiting.store(waiting, Ordering::Relaxed);
}

/// The frames after one doorway opened, as [`measure_portal_opens`] adds them up.
#[derive(Debug, Default, Clone, PartialEq)]
struct OpenWindow {
    ref_id: u32,
    frames: u32,
    start_created: u32,
    last_created: u32,
    max_waiting: u32,
    frames_waiting: u32,
    total_ms: f32,
    max_ms: f32,
    hitches: u32,
}

impl OpenWindow {
    fn new(ref_id: u32, created: u32) -> Self {
        Self {
            ref_id,
            start_created: created,
            last_created: created,
            ..default()
        }
    }

    fn add_frame(&mut self, frame_ms: f32, created: u32, waiting: u32) {
        self.frames += 1;
        self.total_ms += frame_ms;
        self.max_ms = self.max_ms.max(frame_ms);
        if frame_ms > OPEN_HITCH_MS {
            self.hitches += 1;
        }
        self.last_created = created;
        self.max_waiting = self.max_waiting.max(waiting);
        if waiting > 0 {
            self.frames_waiting += 1;
        }
    }

    fn created(&self) -> u32 {
        self.last_created.saturating_sub(self.start_created)
    }
}

/// Logs one line per doorway opened: over the [`OPEN_WINDOW_FRAMES`] frames after the portal
/// starts rendering through a door, the frame times and how many pipelines the render world
/// created and waited on. A doorway whose view needs pipelines the main view has not compiled pays
/// for them in these frames, on the first open of each destination (`local/research/`
/// `portal-first-open-hitch-2026-09-25.md`, research-197).
fn measure_portal_opens(
    state: Res<PortalState>,
    counts: Res<PipelineCounts>,
    time: Res<Time<Real>>,
    doors: Query<&LoadDoor>,
    mut last_door: Local<Option<Entity>>,
    mut window: Local<Option<OpenWindow>>,
    mut opens: Local<u32>,
) {
    let (created, waiting) = counts.read();
    if let Some(open) = window.as_mut() {
        open.add_frame(time.delta_secs() * 1000.0, created, waiting);
        if open.frames >= OPEN_WINDOW_FRAMES {
            info!(
                open = *opens,
                door = format_args!("{:08X}", open.ref_id),
                frames = open.frames,
                mean_ms = open.total_ms / open.frames as f32,
                max_ms = open.max_ms,
                hitches = open.hitches,
                pipelines_created = open.created(),
                max_waiting = open.max_waiting,
                frames_waiting = open.frames_waiting,
                "portal: doorway open"
            );
            *window = None;
        }
    }
    let door = state.open_door;
    if door.is_some() && door != *last_door && window.is_none() {
        *opens += 1;
        let ref_id = door
            .and_then(|door| doors.get(door).ok())
            .map_or(0, |door| door.ref_id);
        *window = Some(OpenWindow::new(ref_id, created));
    }
    *last_door = door;
}

/// Gives the portal camera the atmosphere of the space behind the doorway it is showing, and the
/// doorway's own sun ([`PortalDestinationSun`]) that space's own sun.
///
/// The room on the far side of a door is lit and fogged by its own records: Alftand01's fog is
/// `(153, 210, 238)` and reaches 9,000 units, while the Tamriel it is entered from is fogged by the
/// terrain ring at a completely different distance, and Blackreach's backdrop is teal where the
/// Alftand cavern it opens off is not. Rendering the destination with the *source's* atmosphere is
/// what made a doorway look like a hole into the room the player is already standing in
/// (`docs/research/visual-gaps-spec.md`, gap 2).
///
/// The sun was the one piece this used to leave out, on the reading that the engine's single
/// [`DirectionalLight`] lights every view of a frame. It does not: Bevy selects a view's
/// directional lights by render layers ([`PortalDestinationSun`] has the mechanism and the lines),
/// and the engine's sun has none, so what the doorway image was missing was any directional light
/// at all - a daylit street seen from inside a house was drawn with ambient and fog alone. The
/// destination's own sun is now a light of this camera's own view, which is also the case that
/// matters: a player at a doorway is in the source space or the destination one, never in both.
fn update_destination_atmosphere(
    state: Res<PortalState>,
    catalog: Option<Res<SpaceLightingCatalog>>,
    config: Option<Res<EngineConfig>>,
    doors: Query<&LoadDoor>,
    mut camera: Query<
        (
            &mut Camera,
            &mut AmbientLight,
            &mut DistanceFog,
            &mut CameraSky,
        ),
        With<PortalCamera>,
    >,
    mut suns: Query<&mut DirectionalLight, With<PortalDestinationSun>>,
    mut applied: Local<Option<SpaceKey>>,
) {
    let Ok((mut camera, mut ambient, mut fog, mut sky)) = camera.single_mut() else {
        return;
    };
    let Some(config) = config else {
        return;
    };
    // The door the portal is rendering through, and so the space the camera is standing in. The
    // portal clears it whenever it has no window to draw, which is also when this camera is
    // inactive: there is nothing to keep in step while no doorway is open.
    let Some(destination) = state
        .open_door
        .and_then(|door| doors.get(door).ok())
        .map(|door| {
            space_key(
                door.destination.worldspace_id.unwrap_or_default(),
                door.destination.interior_cell_id,
            )
        })
    else {
        return;
    };
    let atmosphere = crate::atmosphere::space_atmosphere(catalog.as_deref(), destination);
    // The camera's three are written on a change of destination and not every frame: they are read
    // through change detection, and a doorway standing open for a minute would otherwise mark a
    // camera and its view changed sixty times a second for no reason. Nothing else moves them - the
    // catalog is read once at startup and the radii are the run's.
    if *applied != Some(destination) {
        *applied = Some(destination);
        camera.clear_color = ClearColorConfig::Custom(atmosphere.backdrop);
        *ambient = crate::atmosphere::ambient_light(&atmosphere);
        // No LOD reach: distant terrain is not streamed while the player is in an interior, which is
        // when a doorway shows an exterior, so the ring's own fog is what hides its edge.
        *fog = crate::atmosphere::atmosphere_fog(
            &atmosphere,
            config.stream_radius,
            config.terrain_radius,
            None,
        );
        // And the colours the doorway's own dome is drawn with, from the same destination: this
        // camera draws the space behind the door and nothing of the room the player stands in, so
        // it is the destination's sky or none at all (`crate::sky_dome`).
        sky.palette = crate::atmosphere::space_sky(catalog.as_deref(), destination);
    }
    // The sun is written whenever it is not the sun this destination wants, and not only on a
    // change of destination: `crate::atmosphere::update_atmosphere` writes *every*
    // `DirectionalLight` in the world when the space the player stands in changes - this one
    // included, since it cannot know whose it is - and a change of the active space is exactly what
    // a crossing is. A change-only write would leave the doorway carrying the space the player came
    // from until the portal retargeted, and a write on every frame would mark the light changed
    // sixty times a second. Comparing is what makes this one line self-healing instead.
    for mut sun in &mut suns {
        if sun.color != atmosphere.sun.color {
            sun.color = atmosphere.sun.color;
        }
        if sun.illuminance != atmosphere.sun.illuminance {
            sun.illuminance = atmosphere.sun.illuminance;
        }
    }
}

/// The doorway sun's own shadow cascades: `count` of them over [`PORTAL_SUN_SHADOW_DISTANCE`],
/// rather than the engine sun's set, which reaches six times as far.
fn portal_sun_cascades(count: usize) -> CascadeShadowConfig {
    CascadeShadowConfigBuilder {
        num_cascades: count.max(1),
        minimum_distance: PORTAL_SUN_SHADOW_NEAR,
        maximum_distance: PORTAL_SUN_SHADOW_DISTANCE,
        first_cascade_far_bound: PORTAL_SUN_FIRST_CASCADE,
        overlap_proportion: PORTAL_SUN_CASCADE_OVERLAP,
    }
    .build()
}

/// Puts the doorway's own sun ([`PortalDestinationSun`]) where the engine's sun is: the same
/// direction, shadows on when the engine sun's are, and as many cascades as it has.
///
/// The *direction* is the engine sun's and nothing else's. It is a run-wide constant
/// (`app::setup_world` builds it from a rotation), and copying the component rather than naming
/// that rotation again is what keeps one sun in the world: a doorway whose shadow fell the other
/// way from the room around it would be a doorway drawn at a wall angle of its own. What is the
/// *destination's* is the tint and the illuminance, and that is
/// [`update_destination_atmosphere`]'s.
///
/// It casts shadows when it lights the destination at all ([`crate::atmosphere::sun_casts_shadows`]:
/// an interior's sun gives none, and its four cascades were drawn for nothing - research-228), but
/// the cascades are the doorway's own ([`portal_sun_cascades`]): the engine sun's *count*, which
/// Bevy needs every shadowed directional light to share ([`PORTAL_SUN_SHADOW_CASCADES`]), over the
/// doorway's much shorter reach ([`PORTAL_SUN_SHADOW_DISTANCE`]). Bevy budgets cascades per *view*
/// (`bevy_pbr-0.19.0/src/render/light.rs:1323-1353`), so each set is drawn in its own camera's view
/// alone and the main camera's are untouched. The count is matched in the same write as the shadows
/// are switched on, so the doorway's sun never casts with a count of its own.
fn place_destination_sun(
    config: Option<Res<EngineConfig>>,
    engine: Query<
        (&Transform, &DirectionalLight, Option<&CascadeShadowConfig>),
        Without<PortalDestinationSun>,
    >,
    mut sun: Query<
        (
            &mut Transform,
            &mut DirectionalLight,
            &mut CascadeShadowConfig,
        ),
        With<PortalDestinationSun>,
    >,
) {
    // The one that is not ours. There is one directional light in an engine run - the fixtures
    // spawn one of their own, and no run has two - and the first that is not the doorway's is it.
    let Some((engine_transform, _, engine_cascades)) = engine.iter().next() else {
        return;
    };
    for (mut transform, mut light, mut cascades) in &mut sun {
        if transform.rotation != engine_transform.rotation {
            transform.rotation = engine_transform.rotation;
        }
        // Rebuilt only when the count differs: writing the component unconditionally would mark
        // the light changed every frame.
        let count = engine_cascades.map_or(PORTAL_SUN_SHADOW_CASCADES, |c| c.bounds.len());
        if cascades.bounds.len() != count {
            *cascades = portal_sun_cascades(count);
        }
        let shadows = config.as_deref().map_or(light.illuminance > 0.0, |config| {
            crate::atmosphere::sun_casts_shadows(light.illuminance, config)
        });
        if light.shadow_maps_enabled != shadows {
            light.shadow_maps_enabled = shadows;
        }
    }
}

/// Drops every directional light's cascades for the views that do not draw that light: a view
/// whose `RenderLayers` do not meet the light's.
///
/// **Why.** Bevy builds a shadowed sun's cascades for *every* active camera
/// (`bevy_light-0.19.0/src/cascade.rs:195-214`), and then culls every shadow caster on the sun's
/// layers against each of those cascade frusta (`check_dir_light_mesh_visibility`,
/// `bevy_light-0.19.0/src/lib.rs:336`), marking what it finds `ViewVisibility` - so extracted to
/// the render world. Only the render world asks whether the view draws the light at all
/// (`bevy_pbr-0.19.0/src/render/light.rs:1743-1771`), and skips the ones it does not. The
/// doorway's sun ([`PortalDestinationSun`], layer 2) therefore culled the whole resident
/// destination against the *main* camera's and the water reflection's cascades every frame a door
/// stood open - including every frame its doorway was behind the player or behind a wall and the
/// portal camera was off, when no view drew any of it - and the engine sun (layer 0) culled the
/// active space against the portal camera's cascades while a doorway was drawn. Neither set of
/// cascades was ever rendered.
///
/// The rule is the render world's own - the light's layers, none meaning layer 0, against the
/// view's - so a cascade it would have rendered is never dropped. A cascade of an entity that is
/// not a camera is left alone.
fn keep_cascades_of_views_that_draw_their_light(
    cameras: Query<Option<&RenderLayers>, With<Camera>>,
    mut lights: Query<(&mut Cascades, Option<&RenderLayers>), With<DirectionalLight>>,
) {
    for (mut cascades, light_layers) in &mut lights {
        let light_layers = light_layers.unwrap_or_default();
        let draws = |view: &Entity| {
            cameras
                .get(*view)
                .ok()
                .is_none_or(|view_layers| view_layers.unwrap_or_default().intersects(light_layers))
        };
        if cascades.cascades.keys().all(draws) {
            continue;
        }
        cascades.cascades.retain(|view, _| draws(view));
    }
}

fn setup_portal_quad(
    mut commands: Commands,
    texture: Res<PortalTexture>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<PortalMaterial>>,
) {
    // A vertical 1x1 plane facing +Z; Plane3d::default() is a floor (+Y normal), which left the
    // doorway quad lying flat, edge-on to the player and invisible.
    let mesh = meshes.add(Plane3d::new(Vec3::Z, Vec2::splat(0.5)).mesh());
    let material = materials.add(PortalMaterial {
        base: StandardMaterial {
            unlit: true,
            cull_mode: None,
            double_sided: true,
            ..default()
        },
        extension: PortalExtension {
            portal_texture: Some(texture.0.clone()),
            doorway_rect: doorway_rect_uniform(None),
        },
    });
    commands.spawn((
        Name::new("Portal doorway"),
        PortalQuad,
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        Visibility::Hidden,
        RenderLayers::layer(PORTAL_QUAD_LAYER),
    ));
}

/// Puts every load door's model where [`drawn_door_visibility`] says it goes: hidden where the
/// doorway is an opening with no leaf in it, drawn everywhere else.
///
/// `Visibility` is inherited, so this covers the meshes of the glTF scene that the asset loader
/// spawns under the root a frame or more later - the leaf that has not arrived yet is drawn closed
/// when it does, and the leaf of the door the portal shows never arrives on screen at all.
///
/// The write goes straight to the component rather than through `Commands`: a crossing despawns a
/// whole cell's worth of doors in the frame it happens in, and a command queued against a door that
/// the same frame despawns is applied to a dead entity, which Bevy treats as a panic. A query
/// cannot return a despawned door, and writing to one that is despawned later in the frame costs
/// nothing.
///
/// A door with no [`Visibility`] at all has no model and no light to draw - `streaming::spawn_cell`
/// gives the component to every reference that has either - so there is nothing for the portal to
/// open and the query leaves it alone.
///
/// **Ordered within [`PortalFrame`].** This is not the only writer of a door's `DoorState`, and one
/// of the others runs in the frame the state changes: `crate::door_animation`'s arrival opening is
/// ordered before [`PortalFrame`], so the answer for the door the player arrives behind is read off
/// the state they arrive in, not the closed one of the frame before
/// (`crate::transition::OpenDestinationDoor`).
fn show_load_door_leaves(
    state: Res<PortalState>,
    held: Query<(), With<CrossingHeld>>,
    mut doors: Query<(Entity, &LoadDoor, Option<&DoorState>, &mut Visibility)>,
    mut profiler: ResMut<ProfilingState>,
) {
    let started = Instant::now();
    for (door, load_door, door_state, mut visibility) in &mut doors {
        let wanted = drawn_door_visibility(
            load_door.auto_load,
            door_state,
            state.open_door == Some(door),
            held.contains(door),
            state.destination_door == Some(door),
        );
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
    profiler.record_elapsed("portal/door_leaves", started);
}

/// Whether a load door's model is drawn, from the four facts that decide it: whether it is an
/// auto-load marker, where its state is, whether the portal is rendering through it this frame, and
/// whether it is holding a crossing.
///
/// The entries below are the rows of the test
/// `a_doors_model_is_drawn_unless_it_is_the_hole_the_doorway_needs`. [`Visibility::Hidden`] is the
/// answer for exactly one kind of door: one whose doorway is an opening with nothing in it to draw.
///
/// * An **auto-load door**: an invisible marker with no leaf at all (its base is `AutoLoadDoor01`
///   and friends), so it is always hidden. The doorway a marker stands in is drawn by the door
///   beside it or by nothing.
/// * An open door with **no animation of its own** ([`DoorState::hides_whole_reference`]): it has
///   no leaf that can swing out of the opening, so the whole model has to go for the doorway to be
///   the way through that `E` promised - and its model *is* its leaf, so there is nothing else of
///   it that could stay. That covers the door [`update_portal`] picked this frame as well as the
///   open door the player is walking into - which is the state the portal itself stops rendering
///   in, one unit in front of the doorway plane (`MIN_PORTAL_DOOR_DISTANCE`), and where a leaf put
///   back would be in the player's face exactly as they walked into it (design section 4.7). This
///   is the only case that hides a whole reference, and it is kept because the alternative - a
///   closed door drawn over a doorway the crossing is about to use, or an opening with neither leaf
///   nor window in it - is worse than a door that is honestly a hole once it is asked to open.
/// * The door the portal is **rendering through**, when it is a door without an animation: the quad
///   stands in its doorway, and the model would be drawn over it.
/// * The **destination** door of the pair the portal is rendering through
///   ([`PortalState::destination_door`]) while it has no swing of its own under way: the other end
///   of the same doorway, whose closed leaf stands in the window's own clip plane under a doorway
///   anchor and would cover the room the window exists to show. Since impl-234 the far door is
///   driven to the near door's own pose (`crate::door_animation`'s far-door drive), so while the
///   near door is open the far one is `Opening` or `Open` and draws its **own** leaf in the window:
///   the leaf seen from either side of the doorway is one entity, hinged on one jamb. Only a far
///   door the drive has not reached yet (the first frame of a window) or that has no leaf of its
///   own to swing is hidden.
///
/// An **animated** door is none of these, and never becomes one of the others. Its frame *is* the
/// doorway, and
/// its leaves are [`crate::door_animation`]'s to draw or hide: a leaf that swung clear stays drawn,
/// a leaf a narrow clip left in the opening is hidden when the door is `Open`, and the swing itself
/// is drawn - so nothing here may hide its model, not even while the portal renders through it
/// (a stationary leaf mid-swing in front of the window is what an opening door looks like). This is
/// the case the Riverwood house doors are in, and the run's own log says so: they all have
/// `FarmhouseLDoor01`, whose `Open`/`Close` clips are in the converted model
/// (`tools/research/door_animation_nif.py glb`), and a demo tour of that route resolves a clip and a
/// blown-up swing for each of them (`engine::door_animation`, "load door's own swing scaled up to
/// open the doorway", door `0001CBB0`, 18 to 90 degrees) with none falling back to the static path.
/// Where the swung leaf and the window overlap on screen the **depth buffer** decides, not this
/// function: the quad is an opaque mesh of the main view and so is the door, drawn in one pass with
/// one depth buffer, so the leaf wins wherever it is in front of the doorway's own plane - the
/// leaf drawn over the window is what the doorway of an opening door looks like.
///
/// A door whose crossing is **held** ([`CrossingHeld`]) draws whatever else is true: the crossing
/// is waiting for a destination that is not streamed in, so the portal has no window to show
/// through the doorway either, and a doorway with neither is a hole in the world (design section
/// 4.4).
fn drawn_door_visibility(
    auto_load: bool,
    door_state: Option<&DoorState>,
    portal_shows_this_door: bool,
    waiting: bool,
    portal_destination: bool,
) -> Visibility {
    // Whether the door has an animation of its own, and therefore a leaf that is drawn or hidden on
    // its own account: a door with no clip never enters `Opening` or `Closing`, and
    // `Open { animated: false }` is a door whose model is the leaf.
    let animated = door_has_its_own_swing(door_state);
    let opening_with_nothing_in_it = auto_load
        || door_state.is_some_and(|state| state.hides_whole_reference())
        || (portal_shows_this_door && !animated)
        || (portal_destination && !animated);
    if opening_with_nothing_in_it && !waiting {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    }
}

/// The layers a cell's role asks of the nodes below its root: the portal camera's layer for the
/// destination, none at all for a cell no camera draws, and **nothing** for the active space - which
/// is what leaves every node there on the layers it has of its own (Bevy's layer 0 for a node that
/// was given none).
fn role_layers(role: CellRole) -> Option<RenderLayers> {
    match role {
        CellRole::Active => None,
        CellRole::Destination => Some(RenderLayers::layer(DESTINATION_LAYER)),
        CellRole::Hidden => Some(RenderLayers::none()),
    }
}

/// The layers a cell's role asks of a node that has layers of its own ([`PortalLayerSource`]): the
/// node keeps them while its cell is active, and takes the cell's value while it is not. A
/// reference's model, a water plane's surface and a light are all such a node.
fn source_layers(role: CellRole, own: &RenderLayers) -> RenderLayers {
    role_layers(role).unwrap_or_else(|| own.clone())
}

/// The role a cell has this frame.
fn role_of(identity: CellIdentity, space: &ActiveSpace, destination: &[CellKey]) -> CellRole {
    if identity.is_active(space) {
        CellRole::Active
    } else if identity.is_destination(destination) {
        CellRole::Destination
    } else {
        CellRole::Hidden
    }
}

/// The role a cell root's subtree is on, stamped when the root is classified (or the moment
/// `streaming::spawn_cell` spawns it, [`stamp_streamed_cell`]), so a later frame knows what the root
/// already carries and a role change is one write per cell instead of a walk of its nodes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
struct CellRoleStamp(CellRole);

/// A node under a cell root that has layers of its own: the propagation stops here and carries
/// [`source_layers`] - the cell's role applied to those layers - instead of the cell's value alone,
/// so a reference's model, a water plane, a light, or a node a mod gave layers to keeps them while
/// its cell is active and still follows its cell's role while it is not.
///
/// Every one of them is registered with the cell it stands in ([`CellLayerSources`]): a role change
/// is one value per source, never a walk of the cell.
#[derive(Component, Debug, Clone)]
struct PortalLayerSource {
    /// The cell root the node stands under, or `None` for a node no cell owns - a door the door
    /// layer hides that a test spawned on its own: its own layers stand whatever any role asks.
    root: Option<Entity>,
    /// The layers the node keeps while its cell is active.
    own: RenderLayers,
    /// The door the door layer is drawing in place of, while it holds it: every node of that door's
    /// model is on no layer at all, whatever its cell's role asks - its own layers would put it
    /// back on the main camera under the door-layer door standing in the doorway.
    door: Option<Entity>,
}

/// The own-layer sources of every resident cell, so a role change writes one value per source
/// instead of walking the cell, and the sources adopted since the last [`isolate_cells`].
#[derive(Resource, Default)]
struct CellLayerSources {
    /// Each cell root's own-layer sources. A cell that is gone is dropped by [`isolate_cells`] with
    /// its list, and a cell whose role is re-stamped has its list rebuilt without the ids of sources
    /// that despawned in the meantime: a dead id - which the world may have recycled for something
    /// else - never takes a role it does not belong to.
    by_root: HashMap<Entity, Vec<Entity>>,
    /// The sources of each door the door layer is drawing in place of: the nodes of its model that
    /// have layers of their own, on no layer while the door is held, given their cell's value back
    /// the frame the layer releases it.
    door_sources: HashMap<Entity, Vec<Entity>>,
    /// The sources that appeared since the last [`isolate_cells`], which is the only place that
    /// knows the frame's roles: they were adopted with the role of that moment, and the isolation -
    /// every root is classified every frame - gives them the role their cell has *now*. A source
    /// whose root is not classified yet (one spawned after the isolation ran in this frame) stays
    /// here for the next frame.
    pending: Vec<Entity>,
}

/// What one node under a cell root says about its layers: whether it is a source of its own already,
/// the layers it was given, the value it inherited from its cell, and the source it carries itself.
type NodeLayerQuery<'w, 's> = Query<
    'w,
    's,
    (
        Option<&'static PortalLayerSource>,
        Option<&'static RenderLayers>,
        Option<&'static Inherited<RenderLayers>>,
        Option<&'static Propagate<RenderLayers>>,
    ),
>;

/// The layers a node has of its own: the ones its own source keeps ([`PortalLayerSource`]), what its
/// own `Propagate<RenderLayers>` carries (a reference whose model is on the placed-object layer of
/// `crate::render`), or the `RenderLayers` it was given (a water plane's surface layer, a light's, a
/// mod's node). A node whose layers came from the propagation has none of its own - the value it
/// carries is the one it also inherits, which is the cell's.
fn own_layers(
    source: Option<&PortalLayerSource>,
    layers: Option<&RenderLayers>,
    inherited: Option<&Inherited<RenderLayers>>,
    propagate: Option<&Propagate<RenderLayers>>,
) -> RenderLayers {
    if let Some(source) = source {
        return source.own.clone();
    }
    if let Some(propagate) = propagate {
        return propagate.0.clone();
    }
    if let Some(layers) = layers
        && inherited.is_none_or(|inherited| inherited.0 != *layers)
    {
        return layers.clone();
    }
    RenderLayers::default()
}

/// The cell root `entity` stands under, the role stamped on it and the door of `hidden_doors` it
/// stands under (if any): the nearest ancestor (itself included) that is a resident cell root, and
/// the nearest hidden-door ancestor above it. `None` for a node no cell owns - the quad, a
/// door-layer door, a spill light, the portal camera - which the portal leaves on its own layers.
fn owning_cell(
    entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<Option<&CellRoleStamp>, With<StreamedCellRoot>>,
    hidden_doors: Option<&HashSet<Entity>>,
) -> Option<(Entity, Option<CellRole>, Option<Entity>)> {
    let mut door = None;
    let mut cursor = Some(entity);
    while let Some(current) = cursor {
        if let Ok(stamp) = roots.get(current) {
            return Some((current, stamp.map(|stamp| stamp.0), door));
        }
        // The door the node itself is is not one it stands under: the door path of
        // [`isolate_cells`] gives the door its own value.
        if current != entity && hidden_doors.is_some_and(|doors| doors.contains(&current)) {
            door = door.or(Some(current));
        }
        cursor = parents.get(current).ok().map(ChildOf::parent);
    }
    None
}

/// The role a cell has *now*: the classification of this frame ([`PortalState::roles`], which every
/// root is given every frame it is classified in) when it has one for the root, and the root's stamp
/// otherwise - a root the isolation has not classified yet, or one it has never seen at all (no
/// camera, no space). The stamp can be a frame older than the classification (it is written when a
/// cell spawns, with the space of that moment), so a node adopted after the isolation takes its role
/// from the classification and not from the stamp.
fn current_role(state: &PortalState, root: Entity, stamp: Option<CellRole>) -> Option<CellRole> {
    state.roles.get(&root).copied().or(stamp)
}

/// Makes `entity` a source of its own layers under the cell `root`: the propagation carries
/// [`source_layers`] from it instead of the cell's value alone, so a node that has layers of its own
/// keeps them while its cell is active and follows the cell's role while it is not.
///
/// The source is registered with its cell - a role change is then one write for it - with the door
/// it stands under, if one is held, and with the pending list, so the isolation gives it the role its
/// cell has *now*: the role at the moment of adoption can be the previous frame's (a cell spawning in
/// the frame the isolation already ran in, a crossing between the spawn and the isolation).
#[allow(clippy::too_many_arguments)]
fn adopt_layer_source(
    commands: &mut Commands,
    entity: Entity,
    root: Option<Entity>,
    role: Option<CellRole>,
    own: RenderLayers,
    door: Option<Entity>,
    registry: &mut CellLayerSources,
) {
    // A node under a door the door layer is drawing in place of is on no layer at all, whatever its
    // cell's role asks: it would otherwise stand drawn under the door-layer door that hides its own
    // (`crate::render`'s filler of the door model is such a node).
    let value = if door.is_some() {
        RenderLayers::none()
    } else {
        role.map_or_else(|| own.clone(), |role| source_layers(role, &own))
    };
    // `PropagateOver` would keep the node's own layers whatever its cell's role is - it is
    // `crate::render::light_render_layers`'s way of keeping a light's layers under a reference whose
    // model is moved to the placed-object layer - and `PropagateStop` would cut everything under the
    // node off the cell's value. The value below is what carries the node's own layers and its
    // cell's role together, so neither has a place under a cell root any more.
    commands
        .entity(entity)
        .try_insert(PortalLayerSource { root, own, door });
    commands
        .entity(entity)
        .try_remove::<PropagateOver<RenderLayers>>();
    commands
        .entity(entity)
        .try_remove::<PropagateStop<RenderLayers>>();
    commands.entity(entity).try_insert(Propagate(value));
    if let Some(root) = root {
        let list = registry.by_root.entry(root).or_default();
        if !list.contains(&entity) {
            list.push(entity);
        }
    }
    if let Some(door) = door {
        let list = registry.door_sources.entry(door).or_default();
        if !list.contains(&entity) {
            list.push(entity);
        }
    }
    registry.pending.push(entity);
}

/// Adopts `entity` where it stands, from the layers it keeps of its own: the walk up finds the cell
/// it belongs to and the door the door layer holds above it, and [`current_role`] decides the value
/// with the classification of this frame rather than with the root's stamp.
#[allow(clippy::too_many_arguments)]
fn adopt_where_placed(
    commands: &mut Commands,
    entity: Entity,
    own: RenderLayers,
    parents: &Query<&ChildOf>,
    roots: &Query<Option<&CellRoleStamp>, With<StreamedCellRoot>>,
    state: Option<&PortalState>,
    registry: &mut CellLayerSources,
) {
    let (root, stamp, door) =
        owning_cell(entity, parents, roots, state.map(|s| &s.door_layer_hidden))
            .map_or((None, None, None), |(root, stamp, door)| {
                (Some(root), stamp, door)
            });
    let role = match (root, state) {
        (Some(root), Some(state)) => current_role(state, root, stamp),
        _ => stamp,
    };
    adopt_layer_source(commands, entity, root, role, own, door, registry);
}

/// The visibility writes of a cell root's role: the role no camera draws is hidden **and** marked -
/// the layers keep a hidden cell out of the pictures, and only `Visibility` keeps it out of the ray
/// casts, which read no layers at all (`crate::player`) - and a root that becomes active or the
/// portal's destination is given back the visibility a previous frame recorded.
fn set_cell_visibility(
    commands: &mut Commands,
    root: Entity,
    role: CellRole,
    visibility: Option<&Visibility>,
    hidden: Option<&PortalHiddenCell>,
) {
    match role {
        CellRole::Hidden => {
            if !matches!(visibility, Some(Visibility::Hidden)) || hidden.is_none() {
                // Keep the visibility a previous frame recorded, so a root that something else
                // unhid is hidden again without forgetting what to restore.
                let original = hidden.map_or_else(
                    || visibility.copied().unwrap_or_default(),
                    |hidden| hidden.0,
                );
                commands
                    .entity(root)
                    .try_insert((PortalHiddenCell(original), Visibility::Hidden));
            }
        }
        CellRole::Active | CellRole::Destination => {
            if let Some(hidden) = hidden {
                commands.entity(root).try_insert(hidden.0);
                commands.entity(root).try_remove::<PortalHiddenCell>();
            }
        }
    }
}

/// The wiring the cell isolation is: Bevy's own hierarchy propagation of [`RenderLayers`], run in
/// `PostUpdate` before the visibility that reads the layers, and the observers that stamp a cell the
/// frame it spawns and turn a node with layers of its own into a source ([`PortalLayerSource`]).
///
/// Added by `PortalPlugin` for the runs that are looked at, and by this module's own test app - one
/// function, so the two cannot drift. `crate::render::add_placed_object_layer_propagation` is the
/// engine's wiring of the same plugin for the placed-object layer of a reference's model; the guard
/// below keeps a merge that carries both from adding the propagation twice.
fn add_cell_layer_isolation(app: &mut App) {
    if !app.is_plugin_added::<HierarchyPropagatePlugin<RenderLayers>>() {
        app.add_plugins(HierarchyPropagatePlugin::<RenderLayers>::new(PostUpdate));
    }
    app.configure_sets(
        PostUpdate,
        PropagateSet::<RenderLayers>::default().before(VisibilitySystems::CheckVisibility),
    )
    // The propagation's own span of this frame, `portal/propagate`: the per-node cost a cell's role
    // pays once it is stamped, which the isolation moved out of `portal/isolate`.
    .init_resource::<LayerPropagation>()
    .add_systems(
        PostUpdate,
        (
            begin_layer_propagation.before(PropagateSet::<RenderLayers>::default()),
            end_layer_propagation.after(PropagateSet::<RenderLayers>::default()),
        ),
    )
    .init_resource::<CellLayerSources>()
    .add_observer(stamp_streamed_cell)
    .add_observer(adopt_own_layer_node)
    .add_observer(adopt_layer_source_node)
    .add_observer(release_layer_source)
    .add_observer(clear_propagate_stop);
}

/// The instant [`begin_layer_propagation`] recorded, for [`end_layer_propagation`]'s span.
#[derive(Resource, Default)]
struct LayerPropagation(Option<Instant>);

/// Opens the span around Bevy's layer propagation this frame.
fn begin_layer_propagation(mut mark: ResMut<LayerPropagation>) {
    mark.0 = Some(Instant::now());
}

/// Records the layer propagation's cost of this frame as `portal/propagate`, beside `portal/isolate`:
/// the two together are what a cell's role costs, and the propagation's part is the one that scales
/// with the nodes of a cell rather than with its sources.
fn end_layer_propagation(mark: Res<LayerPropagation>, mut profiler: ResMut<ProfilingState>) {
    if let Some(started) = mark.0 {
        profiler.record_elapsed("portal/propagate", started);
    }
}

/// What a cell root says about itself the moment it spawns: the identity the isolation classifies,
/// and the visibility writes of a hidden cell ([`set_cell_visibility`]).
type NewCellQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static StreamedCellKey,
        &'static CellRef,
        Option<&'static ExteriorCellGrid>,
        Option<&'static Visibility>,
        Option<&'static PortalHiddenCell>,
    ),
>;

/// Stamps a cell root the moment `streaming::spawn_cell` gives it a [`StreamedCellKey`]: in the
/// frame it exists in the root already carries its role's source, so the meshes it spawns with - and
/// every mesh of its references, which the glTF spawner adds frames later - take their layers from
/// the propagation without anyone walking the cell. [`isolate_cells`] re-classifies every root every
/// frame and is what corrects this stamp when the space has moved on between the spawn and the
/// isolation (a crossing in the same frame, a camera a frame ahead).
#[allow(clippy::too_many_arguments)]
fn stamp_streamed_cell(
    event: On<Add, StreamedCellKey>,
    mut commands: Commands,
    active: Option<Res<ActiveCell>>,
    config: Option<Res<EngineConfig>>,
    origin: Option<Res<RenderOrigin>>,
    camera: Query<&Transform, With<StreamingCamera>>,
    roots: Query<(), With<StreamedCellRoot>>,
    cells: NewCellQuery,
    state: Option<Res<PortalState>>,
    mut profiler: Option<ResMut<ProfilingState>>,
) {
    let root = event.entity;
    // A terrain-only cell (`DistantTerrainRoot`) is landscape seen from a distance and is never
    // isolated: it keeps the layers it was spawned with.
    if roots.get(root).is_err() {
        return;
    }
    let Ok((key, cell, grid, visibility, hidden)) = cells.get(root) else {
        return;
    };
    let (Some(active), Some(config), Some(origin)) = (active, config, origin) else {
        return;
    };
    // The main camera earlier in this frame, as [`isolate_cells`] reads it: without one there is no
    // space to classify the cell against, and the isolation stamps the root in its own frame.
    let Ok(camera) = camera.single() else {
        return;
    };
    let space = ActiveSpace::of(&active, config.unload_radius, camera.translation, origin.0);
    let destination = state
        .map(|state| state.destination.clone())
        .unwrap_or_default();
    let role = role_of(
        CellIdentity::of(cell.0, grid, Some(key)),
        &space,
        &destination,
    );
    match role_layers(role) {
        Some(layers) => commands.entity(root).try_insert(Propagate(layers)),
        None => commands
            .entity(root)
            .try_remove::<Propagate<RenderLayers>>(),
    };
    commands.entity(root).try_insert(CellRoleStamp(role));
    set_cell_visibility(&mut commands, root, role, visibility, hidden);
    if let Some(profiler) = profiler.as_deref_mut() {
        profiler.increment("portal/cells_relayered", 1);
    }
}

/// A node under a cell root that has been given layers of its own: the propagation would overwrite
/// them with the cell's value in the same frame, so the node becomes a source of its own
/// ([`PortalLayerSource`]) - it keeps those layers while its cell is active and follows the cell's
/// role while it is not.
///
/// What the propagation itself writes is not this: the value it puts on a node is the one that node
/// already inherits, which is the check below - so the portal's own writes do not come back here,
/// and a mod that sets layers on its own node is not overwritten.
fn adopt_own_layer_node(
    event: On<Insert, RenderLayers>,
    mut commands: Commands,
    nodes: NodeLayerQuery,
    parents: Query<&ChildOf>,
    roots: Query<Option<&CellRoleStamp>, With<StreamedCellRoot>>,
    state: Option<Res<PortalState>>,
    mut registry: ResMut<CellLayerSources>,
) {
    let entity = event.entity;
    let Ok((source, layers, inherited, propagate)) = nodes.get(entity) else {
        return;
    };
    let Some(layers) = layers else {
        return;
    };
    if inherited.is_some_and(|inherited| inherited.0 == *layers) {
        return;
    }
    // A cell's own write is not a node asking for layers of its own: the value the portal put there
    // - `Propagate`'s, which the propagation carries onto the node - is exactly the one its cell's
    // role asks of the layers the node keeps, so the `own` below would be that value and the node's
    // real layers would be lost.
    if source.is_some() && propagate.is_some_and(|propagate| propagate.0 == *layers) {
        return;
    }
    match source {
        // A node that is a source already was given new layers of its own: they are what it keeps
        // while its cell is active from now on.
        Some(source) => {
            if source.own == *layers {
                return;
            }
            let own = layers.clone();
            adopt_where_placed(
                &mut commands,
                entity,
                own,
                &parents,
                &roots,
                state.as_deref(),
                &mut registry,
            );
        }
        // A node with a `Propagate<RenderLayers>` of its own - a reference whose model is on the
        // placed-object layer of `crate::render` - belongs to [`adopt_layer_source_node`], which
        // knows that its own layers are the ones that source carries.
        None => {
            if propagate.is_some() {
                return;
            }
            let own = own_layers(source, Some(layers), inherited, propagate);
            adopt_where_placed(
                &mut commands,
                entity,
                own,
                &parents,
                &roots,
                state.as_deref(),
                &mut registry,
            );
        }
    }
}

/// A node whose layers were taken off it again: what `hide_doorway_filler` does when the doorway it
/// hid a door model's filler for is gone. The node has no layers of its own any more, so Bevy's
/// default is what it keeps in the active space - and the removal does not leave the layers it was
/// given behind as the ones a later role change would restore.
fn release_layer_source(
    event: On<Remove, RenderLayers>,
    mut commands: Commands,
    sources: Query<&PortalLayerSource>,
    parents: Query<&ChildOf>,
    roots: Query<Option<&CellRoleStamp>, With<StreamedCellRoot>>,
    state: Option<Res<PortalState>>,
    mut registry: ResMut<CellLayerSources>,
) {
    let entity = event.entity;
    // Only a node the portal made a source of: the propagation removes the layers of a node of the
    // active space on its own account (`propagate_output`), and those have nothing of their own.
    if sources.get(entity).is_err() {
        return;
    }
    adopt_where_placed(
        &mut commands,
        entity,
        RenderLayers::default(),
        &parents,
        &roots,
        state.as_deref(),
        &mut registry,
    );
}

/// A node under a cell root whose own `Propagate<RenderLayers>` carries layers of its own: the value
/// that source carries has to be the cell's role applied to those layers ([`source_layers`]), or a
/// nested source would stop the cell's value at every reference and a destination would never reach
/// the models behind its doors.
///
/// Nothing on this branch gives a *production* node such a source: `crate::render`'s placed-object
/// propagation (render.rs:322-330, [`crate::render::add_placed_object_layer_propagation`]) wires it
/// for the model of every placed reference on the main branch, and this is the fold that lets a
/// merge keep it - a reference's model on the placed-object layer while its cell is active, on the
/// portal camera's layer while the cell is its destination, on none while it is hidden.
fn adopt_layer_source_node(
    event: On<Insert, Propagate<RenderLayers>>,
    mut commands: Commands,
    nodes: NodeLayerQuery,
    parents: Query<&ChildOf>,
    roots: Query<Option<&CellRoleStamp>, With<StreamedCellRoot>>,
    state: Option<Res<PortalState>>,
    mut registry: ResMut<CellLayerSources>,
) {
    let entity = event.entity;
    // A cell root's own source is [`stamp_streamed_cell`]'s and [`isolate_cells`]'s; a node that is
    // a source of the portal's already carries its value ([`adopt_layer_source`]).
    if roots.contains(entity) {
        return;
    }
    let Ok((source, _, _, Some(propagate))) = nodes.get(entity) else {
        return;
    };
    if source.is_some() {
        return;
    }
    let own = propagate.0.clone();
    adopt_where_placed(
        &mut commands,
        entity,
        own,
        &parents,
        &roots,
        state.as_deref(),
        &mut registry,
    );
}

/// A node under a cell root that stops the propagation at itself: nothing under it would ever take
/// its cell's value, so a hidden or destination cell's meshes below such a node would stay on the
/// layers they were spawned with - the main camera's - and a destination would draw over the space
/// the player stands in. A cell's role is the portal's to carry, so the stop is taken off; a node
/// that has layers of its own among all this is [`adopt_own_layer_node`]'s, which makes it a source
/// ([`adopt_layer_source`]) - and that takes the stop off too.
fn clear_propagate_stop(
    event: On<Insert, PropagateStop<RenderLayers>>,
    mut commands: Commands,
    parents: Query<&ChildOf>,
    roots: Query<Option<&CellRoleStamp>, With<StreamedCellRoot>>,
) {
    let entity = event.entity;
    if owning_cell(entity, &parents, &roots, None).is_none() {
        return;
    }
    commands
        .entity(entity)
        .try_remove::<PropagateStop<RenderLayers>>();
}

/// The cell root `entity` hangs under - the first of its ancestors (itself included) that has a
/// role this frame - and whether a node on the way up, other than `entity` itself and the root, is
/// in `marked`. `None` for an entity no cell root owns.
fn owning_root(
    entity: Entity,
    roles: &HashMap<Entity, CellRole>,
    parents: &Query<&ChildOf>,
    marked: &HashSet<Entity>,
) -> Option<(Entity, bool)> {
    let mut covered = false;
    let mut cursor = Some(entity);
    while let Some(current) = cursor {
        if roles.contains_key(&current) {
            return Some((current, covered));
        }
        if current != entity && marked.contains(&current) {
            covered = true;
        }
        cursor = parents.get(current).ok().map(ChildOf::parent);
    }
    None
}

/// Moves every resident cell that is not part of the active space off the main camera's layers, and
/// puts the portal's destination on the portal camera's layer.
///
/// Every root's role is recomputed each frame (a few hundred roots in a village, one classification
/// each), and nothing else happens while the roles have settled: the layers of a cell's nodes are
/// carried by Bevy's own hierarchy propagation from the root's source, which runs in `PostUpdate`
/// before the visibility that reads the layers - in the frame the role changed, and in the frame a
/// glTF scene that was still loading finishes its spawn (the propagation's own `ChildOf` observer),
/// so a shadow entity whose layers change is never one frame late (impl-191). What is left to write
/// per cell here is the root's source, one layer value per node of the cell that has layers of its
/// own ([`PortalLayerSource`], from [`CellLayerSources`] - never a walk), and the doors the door
/// layer hides. The counters measure exactly that: `portal/cells_relayered` is the roots whose role
/// this frame stamped, and `portal/nodes_relayered` **only the portal's own writes** - the own-layer
/// sources of a re-stamped cell, the nodes of a door the layer holds and the repairs of layers taken
/// off a node. The per-node cost the same role pays inside Bevy's propagation is
/// `portal/propagate` (`add_cell_layer_isolation`), and `portal/isolate` is the classification and
/// the writes above.
#[allow(clippy::too_many_arguments)]
fn isolate_cells(
    mut commands: Commands,
    active: Option<Res<ActiveCell>>,
    config: Option<Res<EngineConfig>>,
    origin: Option<Res<RenderOrigin>>,
    camera: Query<&Transform, With<StreamingCamera>>,
    roots: CellRootQuery,
    stamps: Query<Option<&CellRoleStamp>, With<StreamedCellRoot>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    nodes: NodeLayerQuery,
    mut removed_layers: RemovedComponents<RenderLayers>,
    mut state: ResMut<PortalState>,
    mut registry: ResMut<CellLayerSources>,
    mut profiler: ResMut<ProfilingState>,
) {
    let (Some(active), Some(config), Some(origin)) = (active, config, origin) else {
        return;
    };
    let Ok(camera) = camera.single() else {
        return;
    };
    // Measured from here: the two guards above are a resource read and a one-camera lookup, and a
    // frame they stop is a frame with no cell role to isolate at all.
    let started = Instant::now();
    let space = ActiveSpace::of(&active, config.unload_radius, camera.translation, origin.0);
    let destination = state.destination.clone();
    let previous = std::mem::take(&mut state.roles);
    // The native doors the door layer draws in their place (`crate::door_layer`): on no layer at
    // all, whole model, whatever their cell's role, while it holds them.
    let hidden_doors = state.door_layer_hidden.clone();
    let off_cameras = RenderLayers::none();
    // The portal's own writes this frame, for the counters at the end.
    let mut cells_stamped = 0_u64;
    let mut nodes_stamped = 0_u64;

    for (root, cell, grid, key, visibility, hidden) in &roots {
        let identity = CellIdentity::of(cell.0, grid, key);
        let role = role_of(identity, &space, &destination);
        state.roles.insert(root, role);
        if !previous.contains_key(&root) {
            state.churn.appeared(identity, root);
        }
        // A hidden cell is hidden *and* marked: the layers keep it out of the pictures, and only
        // `Visibility` keeps it out of the ray casts, which read no layers at all.
        set_cell_visibility(&mut commands, root, role, visibility, hidden);

        // A root the stamp already agrees with is left alone: its whole subtree is on the value the
        // propagation gave it and nothing about it has moved. A new root - stamped by
        // [`stamp_streamed_cell`] in the frame it spawned, or here when that had no space to
        // classify it with - and a changed one take the writes below, in that one frame.
        let stamped = stamps.get(root).ok().flatten().map(|stamp| stamp.0);
        if stamped == Some(role) {
            continue;
        }
        match role_layers(role) {
            Some(layers) => commands.entity(root).try_insert(Propagate(layers)),
            None => commands
                .entity(root)
                .try_remove::<Propagate<RenderLayers>>(),
        };
        commands.entity(root).try_insert(CellRoleStamp(role));
        // One write per node of this cell that has layers of its own; every other node of the cell
        // follows the root's value through the propagation. The list is rebuilt while it is walked,
        // without the ids of sources that despawned since it was written: a dead id - which the
        // world may have recycled for another cell's entity - never takes a role.
        let list = registry.by_root.remove(&root).unwrap_or_default();
        let mut live = Vec::with_capacity(list.len());
        let mut written = 0_u64;
        for entity in list {
            let Ok((Some(source), ..)) = nodes.get(entity) else {
                continue;
            };
            // A node of a door the layer is drawing in place of stays on no layer at all.
            let value = if source.door.is_some() {
                off_cameras.clone()
            } else {
                source_layers(role, &source.own)
            };
            commands.entity(entity).try_insert(Propagate(value));
            live.push(entity);
            written += 1;
        }
        if !live.is_empty() {
            registry.by_root.insert(root, live);
        }
        state.churn.restamped(identity, stamped, role, written);
        cells_stamped += 1;
        nodes_stamped += written;
    }

    // The door layer's natives: the whole model of a door it draws in place is off every camera by
    // one source of `none` on the door itself, rather than a write per node of it. The value is
    // written when it is not on the door already - the propagation puts it there in the frame the
    // source is set - so a door the layer holds costs nothing on the frames after the first.
    //
    // A node of that model that has layers of its own - the filler `crate::render` puts on its own
    // layer to keep the doorway's floor probe working, a light, a mod's node - is a source of its
    // own and would keep them: a door that *enters* the layer's set has its model walked once, and
    // every source under it is marked with the door and put on no layer. The walk is the door's own
    // model, not the cell, and only on the frame the door is taken over.
    let entering: Vec<Entity> = hidden_doors
        .difference(&state.door_layer_applied)
        .copied()
        .collect();
    for door in entering {
        for entity in children.iter_descendants(door) {
            let Ok((Some(source), ..)) = nodes.get(entity) else {
                continue;
            };
            commands.entity(entity).try_insert(PortalLayerSource {
                door: Some(door),
                ..source.clone()
            });
            commands
                .entity(entity)
                .try_insert(Propagate(off_cameras.clone()));
            let list = registry.door_sources.entry(door).or_default();
            if !list.contains(&entity) {
                list.push(entity);
            }
            nodes_stamped += 1;
        }
    }
    for &door in &hidden_doors {
        let Ok((source, layers, inherited, propagate)) = nodes.get(door) else {
            continue;
        };
        if source.is_none() {
            let own = own_layers(source, layers, inherited, propagate);
            let root =
                owning_cell(door, &parents, &stamps, Some(&hidden_doors)).map(|(root, ..)| root);
            adopt_layer_source(
                &mut commands,
                door,
                root,
                Some(CellRole::Hidden),
                own,
                None,
                &mut registry,
            );
            nodes_stamped += 1;
        } else if layers != Some(&off_cameras) {
            commands
                .entity(door)
                .try_insert(Propagate(off_cameras.clone()));
            nodes_stamped += 1;
        }
    }
    // A door the layer gave back takes the value its cell's role asks of its own layers again, and
    // so does every source of its model the layer held. A door no cell root owns (a test's) keeps
    // its own layers, which is what `CellRole::Active` stands for here.
    let released: Vec<Entity> = state
        .door_layer_applied
        .difference(&hidden_doors)
        .copied()
        .collect();
    for door in released {
        for entity in registry.door_sources.remove(&door).unwrap_or_default() {
            let Ok((Some(source), ..)) = nodes.get(entity) else {
                continue;
            };
            let role = source
                .root
                .and_then(|root| state.roles.get(&root).copied())
                .unwrap_or(CellRole::Active);
            commands.entity(entity).try_insert(PortalLayerSource {
                door: None,
                ..source.clone()
            });
            commands
                .entity(entity)
                .try_insert(Propagate(source_layers(role, &source.own)));
            nodes_stamped += 1;
        }
        let Ok((source, layers, inherited, propagate)) = nodes.get(door) else {
            continue;
        };
        let own = own_layers(source, layers, inherited, propagate);
        if source.is_none() {
            let root = owning_cell(door, &parents, &stamps, None).map(|(root, ..)| root);
            adopt_layer_source(
                &mut commands,
                door,
                root,
                None,
                own.clone(),
                None,
                &mut registry,
            );
        }
        let role = source
            .and_then(|source| source.root)
            .and_then(|root| state.roles.get(&root).copied())
            .unwrap_or(CellRole::Active);
        commands
            .entity(door)
            .try_insert(Propagate(source_layers(role, &own)));
        nodes_stamped += 1;
    }
    // A node whose layers were taken away while its cell still asks for a value. The propagation's
    // own removal path is the active space - it takes the value's `Inherited` with it - so anything
    // that still has one was stripped by something else and goes back on it.
    for entity in removed_layers.read() {
        if let Ok((_, _, Some(inherited), _)) = nodes.get(entity) {
            commands.entity(entity).try_insert(inherited.0.clone());
            nodes_stamped += 1;
        }
    }
    // The own-layer sources adopted since the last frame: the role their cell has *now* is what
    // their value should be, and the classification above is the answer. A source whose root is not
    // classified yet - one spawned after this system ran in this frame - waits for the next frame;
    // a door the door layer holds keeps no layers whatever its cell's role is.
    let mut pending = Vec::new();
    for entity in std::mem::take(&mut registry.pending) {
        let Ok((Some(source), ..)) = nodes.get(entity) else {
            continue;
        };
        let Some(root) = source.root else {
            continue;
        };
        let Some(role) = state.roles.get(&root).copied() else {
            pending.push(entity);
            continue;
        };
        let value = if source.door.is_some() || hidden_doors.contains(&entity) {
            off_cameras.clone()
        } else {
            source_layers(role, &source.own)
        };
        commands.entity(entity).try_insert(Propagate(value));
        nodes_stamped += 1;
    }
    registry.pending = pending;
    // The cells that are gone took their sources with them.
    registry
        .by_root
        .retain(|root, _| state.roles.contains_key(root));
    state.door_layer_applied = hidden_doors;
    // The churn of this frame, as counters: the roots whose role the portal stamped, and the
    // per-node layer writes it issued for them - one per own-layer source and per door the door
    // layer hides, never one per node of a cell.
    profiler.increment("portal/cells_relayered", cells_stamped);
    profiler.increment("portal/nodes_relayered", nodes_stamped);
    profiler.record_elapsed("portal/isolate", started);
}

/// The role each cell root's colliders were last left in, so [`gate_cell_colliders`] walks a root's
/// subtree only when its role changed.
#[derive(Default)]
struct AppliedColliderRoles(HashMap<Entity, CellRole>);

/// Enables the colliders of the cells that are part of the active space, and disables every other
/// cell's, so the player's probe (`crate::player`) hits the space it stands in and nothing else.
///
/// It runs directly after [`isolate_cells`] and therefore sees the roles that system published for
/// this frame. Colliders are born `ColliderDisabled` (`crate::streaming`), and this system is the
/// only thing that removes it: a collider may never be enabled by its own readiness, because a
/// pre-streamed destination becomes ready while its cell is not active (port-prep-collision, point
/// 2). A crossing changes `ActiveCell` before [`PortalFrame`] runs, so the destination's colliders
/// are live in the frame the player arrives in it - the same one-frame contract the mesh layers
/// have.
///
/// A root's subtree is walked only when the root's role changed since the last frame; a collider
/// that appears under a root whose role is settled (a model armed later, a destination's scene
/// finishing its spawn) is caught by its own `Added` flag the first time this system sees it.
#[allow(clippy::too_many_arguments)]
fn gate_cell_colliders(
    mut commands: Commands,
    state: Res<PortalState>,
    mut applied: Local<AppliedColliderRoles>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    colliders: Query<(), With<Collider>>,
    newly_added: Query<Entity, Added<Collider>>,
    mut profiler: ResMut<ProfilingState>,
) {
    let started = Instant::now();
    // The colliders this frame's walk issued a gate command for, counted below.
    let mut gated = 0_u64;
    // A root that is gone took its colliders with its subtree; nothing to flip.
    applied.0.retain(|root, _| state.roles.contains_key(root));
    for (&root, &role) in &state.roles {
        if applied.0.get(&root) == Some(&role) {
            continue;
        }
        applied.0.insert(root, role);
        for entity in children.iter_descendants(root) {
            gated += u64::from(apply_collider_role(&mut commands, entity, role, &colliders));
        }
    }
    let unmarked = HashSet::new();
    for entity in &newly_added {
        let Some((root, _)) = owning_root(entity, &state.roles, &parents, &unmarked) else {
            continue;
        };
        gated += u64::from(apply_collider_role(
            &mut commands,
            entity,
            state.roles[&root],
            &colliders,
        ));
    }
    profiler.increment("portal/colliders_gated", gated);
    profiler.record_elapsed("portal/colliders", started);
}

/// Puts one collider in the state its cell's role calls for: live in the active space, disabled
/// everywhere else. Anything that is not a collider is left alone (the walk covers a cell's whole
/// subtree, not only its colliders).
///
/// Answers whether the entity was a collider, so [`gate_cell_colliders`] can count the commands it
/// issued this frame (`portal/colliders_gated`).
fn apply_collider_role(
    commands: &mut Commands,
    entity: Entity,
    role: CellRole,
    colliders: &Query<(), With<Collider>>,
) -> bool {
    if colliders.get(entity).is_err() {
        return false;
    }
    match role {
        CellRole::Active => {
            commands.entity(entity).try_remove::<ColliderDisabled>();
        }
        CellRole::Destination | CellRole::Hidden => {
            commands.entity(entity).try_insert(ColliderDisabled);
        }
    }
    true
}

/// Places the portal camera in the destination of the nearest usable door and puts the doorway quad
/// where that door is.
#[allow(clippy::too_many_arguments)]
fn update_portal(
    mut state: ResMut<PortalState>,
    active: Option<Res<ActiveCell>>,
    config: Option<Res<EngineConfig>>,
    origin: Option<Res<RenderOrigin>>,
    streaming: Option<Res<StreamingWorld>>,
    main: MainCameraQuery,
    doors: LoadDoorQuery,
    parents: Query<&ChildOf>,
    mut portal_camera: PortalCameraQuery,
    mut quad: PortalQuadQuery,
    leaf_openings: Query<&LeafOpening>,
    time: Option<Res<Time>>,
    mut shown: Local<Option<u32>>,
    mut profiler: ResMut<ProfilingState>,
) {
    // The whole body is measured, guard returns included: this span is what the portal chain costs
    // a frame, and the frame a run has no portal camera or quad is a frame of that cost too.
    let started = Instant::now();
    // This frame's portal, from scratch: whenever the camera below cannot be placed there is no
    // doorway rendering the destination, and the door that was open for it has to close again in
    // this frame. Clearing before the early returns is what makes that true of every one of them.
    state.open_door = None;
    state.destination_door = None;
    state.doorway = None;
    let (Some(active), Some(config), Some(origin), Some(streaming)) =
        (active, config, origin, streaming)
    else {
        profiler.record_elapsed("portal/update", started);
        return;
    };
    let Ok((main_transform, main_projection, main_camera)) = main.single() else {
        profiler.record_elapsed("portal/update", started);
        return;
    };
    let Ok((mut camera_transform, mut camera_projection, mut camera)) = portal_camera.single_mut()
    else {
        profiler.record_elapsed("portal/update", started);
        return;
    };
    let Ok((mut quad_transform, mut quad_visibility)) = quad.single_mut() else {
        profiler.record_elapsed("portal/update", started);
        return;
    };
    let camera_position = main_transform.translation;
    let camera_rotation = main_transform.rotation;
    let space = ActiveSpace::of(&active, config.unload_radius, camera_position, origin.0);

    // How far in front of each door the camera is, along the direction that door faces. The map it
    // is read off is the one [`update_portal`] builds for the door it picks - the doorway anchor
    // where the door has one, the reference origin and the link-derived frame otherwise - and the
    // pivot it is measured from is the plane its projection clips at
    // ([`distance_in_front_of_door`]).
    let distance_in_front = |entity: Entity| -> f32 {
        let Ok((_, global, local, door, .., anchor)) = doors.get(entity) else {
            return f32::NEG_INFINITY;
        };
        let map = door_map(
            global.translation(),
            global.rotation(),
            local.scale,
            door,
            anchor,
            origin.0,
        );
        distance_in_front_of_door(map.pivot, map.frame, camera_position)
    };

    // How far off the middle of the view each door's doorway is, or `None` when the doorway covers
    // none of the screen: the doorway measured is the quad the portal would draw in it, so the
    // door picked is the one whose window the player would see. A main camera with no size to
    // measure against sees every doorway, which is what the portal did before it asked.
    let window = main_camera.and_then(Camera::physical_viewport_size);
    let forward = camera_rotation * Vec3::NEG_Z;
    let aim = |entity: Entity| -> Option<f32> {
        let (_, global, local, door, _, instance_bounds, expected_bounds, anchor) =
            doors.get(entity).ok()?;
        let map = door_map(
            global.translation(),
            global.rotation(),
            local.scale,
            door,
            anchor,
            origin.0,
        );
        let quad = doorway_quad_transform(
            global.translation(),
            global.rotation(),
            map.frame,
            local.scale,
            instance_bounds,
            expected_bounds,
            anchor,
            leaf_openings.get(entity).ok(),
        );
        if let Some(window) = window {
            doorway_screen_rect(
                doorway_corners(&quad),
                main_transform,
                main_projection,
                window.as_vec2(),
            )?;
        }
        Some(
            (quad.translation - camera_position)
                .try_normalize()
                .map_or(0.0, |direction| forward.angle_between(direction)),
        )
    };

    // Only doors of cells that are in the active space can be looked through: a door of a
    // pre-streamed cell is drawn nowhere near the space the player stands in.
    let candidates = doors
        .iter()
        .filter(|(entity, ..)| {
            state
                .roles
                .get(&parents.get(*entity).map(ChildOf::parent).unwrap_or(*entity))
                == Some(&CellRole::Active)
        })
        .map(|(entity, global, _, door, door_state, _, _, anchor)| {
            (entity, global.translation(), door, door_state, anchor)
        });
    let target = select_portal_door(
        camera_position,
        candidates,
        |destination, anchor| destination_is_resident(destination, anchor, &streaming),
        &space,
        distance_in_front,
        aim,
    );

    // Whether the door the portal last drew through still shows a way into the same destination:
    // a door of the active space, open or swinging, with its far side resident and not part of the
    // active space. What [`select_portal_door`] asks of it beyond that - being in front of it, in
    // reach, or picked over another door - is what the destination hold rides out.
    let now = time.map(|time| time.elapsed_secs_f64());
    let still_holds = |held: Entity| {
        let Ok((_, _, _, door, door_state, _, _, anchor)) = doors.get(held) else {
            return false;
        };
        let in_active_space = state
            .roles
            .get(&parents.get(held).map(ChildOf::parent).unwrap_or(held))
            == Some(&CellRole::Active);
        in_active_space
            && portal_shows_through(door_state)
            && destination_is_resident(&door.destination, anchor, &streaming)
            && !destination_keys(&door.destination, anchor)
                .iter()
                .any(|key| space.contains(*key))
    };

    let Some(target) = target else {
        if shown.take().is_some() {
            info!("portal: no door in view");
        }
        let held = state
            .hold
            .as_ref()
            .is_some_and(|hold| still_holds(hold.door));
        state.lose_destination(now, |_| held);
        *quad_visibility = Visibility::Hidden;
        camera.is_active = false;
        profiler.record_elapsed("portal/update", started);
        return;
    };

    let Ok((_, global, local, door, _, instance_bounds, expected_bounds, anchor)) =
        doors.get(target)
    else {
        let held = state
            .hold
            .as_ref()
            .is_some_and(|hold| still_holds(hold.door));
        state.lose_destination(now, |_| held);
        *quad_visibility = Visibility::Hidden;
        camera.is_active = false;
        profiler.record_elapsed("portal/update", started);
        return;
    };
    // The doorway opens here: `show_load_door_leaves` hides this door's leaf in this same frame,
    // with the quad below standing in the doorway it leaves. Nothing past this point fails, so the
    // leaf and the quad go up and down together.
    state.open_door = Some(target);
    // The destination door of the pair: the far end of this doorway, which
    // `crate::door_animation`'s far-door drive swings with this one so that the window shows its
    // own leaf (impl-234), and which is hidden only while it has no swing of its own under way -
    // closed, its leaf stands in the window's own clip plane under a doorway anchor
    // (`PortalState::destination_door`). A door whose map did not move keeps its destination door
    // as it was - the clip stands inside the room - and is drawn with the mirror instead.
    // Only a destination door the portal is drawing counts: one standing in the active space is
    // what the player sees with their own eyes.
    state.destination_door = anchor.and_then(|_| {
        doors.iter().find_map(|(entity, _, _, entry, ..)| {
            let active = state
                .roles
                .get(&parents.get(entity).map(ChildOf::parent).unwrap_or(entity))
                == Some(&CellRole::Active);
            (entity != target && !active && entry.ref_id == door.destination.destination_ref_id)
                .then_some(entity)
        })
    });
    if *shown != Some(door.ref_id) {
        *shown = Some(door.ref_id);
        info!(
            door = format_args!("{:08X}", door.ref_id),
            destination = %door.label.trim_end_matches(['\0', ' ']),
            anchor = ?anchor.map(|anchor| anchor.tier),
            "portal: looking through a load door"
        );
    }
    // The map the whole feature is one definition of: the portal camera below, the doorway quad,
    // the clip plane and - through `crate::transition`'s own call - the crossing and the mirror.
    let map = door_map(
        global.translation(),
        global.rotation(),
        local.scale,
        door,
        anchor,
        origin.0,
    );
    let door_position = global.translation();
    let door_rotation = global.rotation();
    let frame = map.frame;
    let (portal_position, portal_rotation) = map.pose(camera_position, camera_rotation);
    // The window is clipped at the *destination doorway's* plane, which under an anchor is exactly
    // the image of the source doorway's plane: the room is drawn from the doorway, not from a plane
    // tens of units inside it.
    let (destination_point, destination_normal) = map.destination_plane();
    let clip_plane = doorway_clip_plane(
        portal_position,
        portal_rotation,
        destination_point,
        destination_normal,
    );

    camera_transform.translation = portal_position;
    camera_transform.rotation = portal_rotation;
    *camera_projection = portal_projection(main_projection, clip_plane, -clip_plane.w);

    // The doorway itself is the model's, so its box is measured in the model's own frame and then
    // laid out in the frame the door's front comes from: one frame for the quad's position, its
    // orientation and its size. The quad goes where the doorway's own plane is, which is also the
    // plane `crate::player::player_walks_through_doors` fires the crossing on.
    //
    // No standoff: the window is this quad and nothing else (the destination cell is on the portal
    // camera's layer), so it can only carry the destination while it is in front of the camera. A
    // quad held forward of the doorway plane - eight units was enough - is passed by the camera
    // while the player is still walking the last stretch to it, and the doorway shows the wall the
    // door is set into for those units. With the quadrant the doorway's plane, the crossing fires
    // exactly where the window would end, so no frame of the walk shows anything but the
    // destination through it. The cost is that the window covers exactly the measured opening, so a
    // door whose bounds under-measure its doorway would show a sliver of the wall's reveal; the fix
    // for one of those is a better measurement, not a quad held out in front of it.
    *quad_transform = doorway_quad_transform(
        door_position,
        door_rotation,
        frame,
        local.scale,
        instance_bounds,
        expected_bounds,
        anchor,
        leaf_openings.get(target).ok(),
    );
    // Published for the systems that dress the doorway (`crate::portal_spill`): read only.
    state.doorway = Some(OpenDoorway {
        door: target,
        map,
        quad: *quad_transform,
    });

    // A doorway the main camera cannot see costs nothing. The portal camera renders a whole frame
    // of the destination, shadows and all, and a doorway behind the player or off a side of the
    // screen samples none of it, so the camera stops and the quad is hidden for this frame - the
    // no-portal path's two writes. The rest of the portal stays exactly as it is: the door keeps
    // its leaf hidden and its mirror (a scene that takes frames to spawn again), the destination
    // cells stay on the portal camera's layer and the destination door stays hidden, so the frame
    // the doorway comes back into view is the frame it is drawn in, with nothing to rebuild. None
    // of those can be seen from here: they are the doorway's, and the doorway is off screen.
    //
    // A doorway that is on screen is rendered at its own size: the portal camera projects only the
    // rectangle of the main view the doorway covers ([`doorway_render_rect`], [`doorway_sub_view`])
    // and [`resize_portal_target`] sizes the target to it, so a doorway a tenth of the screen wide
    // shades a tenth of the pixels. A main camera with no size to measure against keeps the whole
    // view, which is what the portal did before it asked.
    let on_screen = match window {
        None => {
            camera.sub_camera_view = None;
            state.render_rect = None;
            true
        }
        Some(window) => {
            let rect = doorway_screen_rect(
                doorway_corners(&quad_transform),
                main_transform,
                main_projection,
                window.as_vec2(),
            )
            .and_then(|rect| doorway_render_rect(rect, window));
            if let Some(rect) = rect {
                camera.sub_camera_view = Some(doorway_sub_view(rect, window));
                state.render_rect = Some(rect);
            }
            rect.is_some()
        }
    };
    camera.is_active = on_screen;
    *quad_visibility = if on_screen {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };

    state.draw_destination(target, destination_keys(&door.destination, anchor));
    profiler.record_elapsed("portal/update", started);
}

/// The sight lines [`skip_occluded_doorway`] tests a doorway with: a grid this many points wide
/// and this many high.
const OCCLUSION_GRID: usize = 4;

/// How far past each edge of the doorway the sight-line grid reaches, as a fraction of the
/// doorway's width or height. A doorway coming out from behind a wall edge shows the grid's outer
/// row or column first, so the test sees it coming before any pixel of the doorway itself is
/// uncovered.
const OCCLUSION_MARGIN: f32 = 0.25;

/// How far toward the eye, off the doorway's plane, the sight-line grid stands, in Creation units.
///
/// The points past the doorway's edges would otherwise lie on - or inside - the wall the door is
/// set into, and every sight line to them would stop on that wall: blocked, whether the doorway is
/// in plain view or not, and useless for seeing it come out. Held off the plane, a point is blocked
/// only by something standing between the eye and the doorway.
const OCCLUSION_STANDOFF: f32 = 32.0;

/// Within this distance of the doorway's plane the doorway is never skipped, in Creation units: the
/// grid would stand at or behind the eye, and a doorway this close is the one the player is about
/// to walk through.
const OCCLUSION_MIN_DISTANCE: f32 = 160.0;

/// How often a doorway in view is tested for being hidden: every this many frames. A hidden doorway
/// is tested every frame, because the frame it comes back is the frame it has to be drawn in.
const OCCLUSION_RETEST_FRAMES: u32 = 4;

/// How many tests in a row have to find every sight line blocked before the doorway stops being
/// drawn: about a fifth of a second at 60 frames a second. The hysteresis is on the way *out*
/// only; the way back is immediate.
const OCCLUSION_CONFIRM_TESTS: u32 = 3;

/// How far short of its grid point a sight line stops, in Creation units, so a surface the point
/// itself lies on does not count as standing in front of it.
const OCCLUSION_RAY_SLACK: f32 = 2.0;

/// Whether the doorway the portal draws through is hidden behind the view's own geometry, with the
/// hysteresis that keeps the decision from chattering ([`skip_occluded_doorway`]).
#[derive(Debug, Default)]
struct DoorwayOcclusion {
    /// The door the state below is about; a different door starts from "in view".
    door: Option<Entity>,
    /// Whether the doorway is skipped this frame.
    hidden: bool,
    /// Tests in a row that found every sight line blocked while the doorway was drawn.
    blocked_tests: u32,
    /// Frames left before a doorway in view is tested again.
    wait: u32,
    /// Tests run for the current door: whether its end is worth a log line.
    door_tests: u32,
    /// Frames the doorway view was skipped for, over the run: the log's count.
    skipped_frames: u64,
    /// Sight-line tests run, their total and their longest time: the log's cost of the test.
    tests: u64,
    test_time: Duration,
    longest_test: Duration,
}

impl DoorwayOcclusion {
    /// No doorway to test this frame: the next one starts from "in view".
    fn forget(&mut self) {
        self.door = None;
        self.door_tests = 0;
        self.hidden = false;
        self.blocked_tests = 0;
        self.wait = 0;
    }

    /// This frame's decision for the doorway of `door`: `true` to skip it. `blocked` runs the
    /// sight-line test, and is only called on the frames that need it: every frame while the
    /// doorway is hidden - it is drawn again in the first frame one sight line is clear - and every
    /// [`OCCLUSION_RETEST_FRAMES`] frames while it is drawn, where [`OCCLUSION_CONFIRM_TESTS`]
    /// blocked tests in a row hide it.
    fn update(&mut self, door: Entity, blocked: impl FnOnce() -> bool) -> bool {
        if self.door != Some(door) {
            self.forget();
            self.door = Some(door);
        }
        if self.hidden {
            self.door_tests += 1;
            if !blocked() {
                self.hidden = false;
                self.blocked_tests = 0;
                self.wait = OCCLUSION_RETEST_FRAMES - 1;
            }
            return self.hidden;
        }
        if self.wait > 0 {
            self.wait -= 1;
            return false;
        }
        self.wait = OCCLUSION_RETEST_FRAMES - 1;
        self.door_tests += 1;
        if blocked() {
            self.blocked_tests += 1;
            self.hidden = self.blocked_tests >= OCCLUSION_CONFIRM_TESTS;
        } else {
            self.blocked_tests = 0;
        }
        self.hidden
    }
}

/// The points the sight lines to a doorway end at: an [`OCCLUSION_GRID`] square grid over the
/// doorway quad `quad` ([`doorway_quad_transform`]) grown by [`OCCLUSION_MARGIN`] on every side,
/// held [`OCCLUSION_STANDOFF`] off the doorway's plane on the side of `eye`.
fn doorway_occlusion_samples(
    quad: &Transform,
    eye: Vec3,
) -> [Vec3; OCCLUSION_GRID * OCCLUSION_GRID] {
    let normal = quad.rotation * Vec3::Z;
    let toward_eye = if normal.dot(eye - quad.translation) >= 0.0 {
        normal
    } else {
        -normal
    };
    let reach = 0.5 + OCCLUSION_MARGIN;
    let step = 2.0 * reach / (OCCLUSION_GRID - 1) as f32;
    std::array::from_fn(|index| {
        let (column, row) = (index % OCCLUSION_GRID, index / OCCLUSION_GRID);
        let local = Vec3::new(
            -reach + step * column as f32,
            -reach + step * row as f32,
            0.0,
        );
        quad.transform_point(local) + toward_eye * OCCLUSION_STANDOFF
    })
}

/// A mesh that can hide a doorway: its model-to-world transform and its mesh.
struct Occluder<'a> {
    transform: Affine3A,
    aabb: Aabb3d,
    mesh: &'a Mesh,
}

/// The distance along `ray` to the nearest front face of `mesh` placed by `transform`, the way
/// Bevy's `MeshRayCast` measures it (`bevy_picking`'s `ray_intersection_over_mesh`, which is not
/// public): triangle lists only, back faces culled - a single-sided wall seen from behind is not
/// drawn, so it hides nothing.
fn ray_hits_mesh(mesh: &Mesh, transform: &Affine3A, ray: Ray3d) -> Option<f32> {
    if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let positions = mesh
        .try_attribute(Mesh::ATTRIBUTE_POSITION)
        .ok()?
        .as_float3()?;
    let hit = match mesh.try_indices().ok() {
        Some(Indices::U16(indices)) => ray_mesh_intersection(
            ray,
            transform,
            positions,
            None,
            Some(indices),
            None,
            Backfaces::Cull,
        ),
        Some(Indices::U32(indices)) => ray_mesh_intersection(
            ray,
            transform,
            positions,
            None,
            Some(indices),
            None,
            Backfaces::Cull,
        ),
        None => ray_mesh_intersection::<u32>(
            ray,
            transform,
            positions,
            None,
            None,
            None,
            Backfaces::Cull,
        ),
    };
    hit.map(|hit| hit.distance)
}

/// Whether every sight line from `eye` to the `samples` is blocked by one of the `occluders`
/// before it reaches its point.
///
/// Line by line, the lines nearest the middle of the grid first, each against the occluders in the
/// order given (nearest the eye first), its bounding box before its triangles. A blocked line stops
/// at the first occluder that blocks it, and the whole test stops at the first line nothing blocks:
/// a doorway behind a wall costs the triangles of the first mesh in the way of each line, and a
/// doorway in view the triangles along one or two open lines rather than all of them.
fn sight_lines_blocked(eye: Vec3, samples: &[Vec3], occluders: &[Occluder]) -> bool {
    if samples.is_empty() {
        return false;
    }
    let middle = samples.iter().copied().sum::<Vec3>() / samples.len() as f32;
    let mut order: Vec<Vec3> = samples.to_vec();
    order.sort_by(|a, b| {
        a.distance_squared(middle)
            .total_cmp(&b.distance_squared(middle))
    });
    order.into_iter().all(|sample| {
        let offset = sample - eye;
        let length = offset.length() - OCCLUSION_RAY_SLACK;
        let Ok(direction) = Dir3::new(offset) else {
            // A point at the eye itself: nothing can stand in front of it.
            return false;
        };
        let ray = Ray3d::new(eye, direction);
        length > 0.0
            && occluders.iter().any(|occluder| {
                ray_aabb_intersection_3d(ray, &occluder.aabb, &occluder.transform)
                    .is_some_and(|near| near < length)
                    && ray_hits_mesh(occluder.mesh, &occluder.transform, ray)
                        .is_some_and(|distance| distance < length)
            })
    })
}

/// The world-space box of a mesh's local `aabb` placed by `transform`.
fn world_box(aabb: &Aabb, transform: &Affine3A) -> (Vec3A, Vec3A) {
    let centre = transform.transform_point3a(aabb.center);
    let matrix = transform.matrix3;
    let half = Vec3A::new(
        matrix.row(0).abs().dot(aabb.half_extents),
        matrix.row(1).abs().dot(aabb.half_extents),
        matrix.row(2).abs().dot(aabb.half_extents),
    );
    (centre - half, centre + half)
}

/// Every mesh a doorway can be hidden behind, with what it takes to test it.
type OccluderQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static Mesh3d,
        &'static Aabb,
        &'static GlobalTransform,
        &'static InheritedVisibility,
        Option<&'static RenderLayers>,
        Option<&'static MeshMaterial3d<StandardMaterial>>,
        Has<MeshMaterial3d<TerrainMaterial>>,
    ),
>;

/// Forgets the doorway [`skip_occluded_doorway`] was testing, with one log line for a doorway that
/// was tested at all: the run's totals so far, which is the cost of the test and what it saved.
fn end_occlusion_session(occlusion: &mut DoorwayOcclusion) {
    if occlusion.door_tests > 0 {
        info!(
            door_tests = occlusion.door_tests,
            tests = occlusion.tests,
            mean_test_us = occlusion.test_time.as_micros() as u64 / occlusion.tests.max(1),
            longest_test_us = occlusion.longest_test.as_micros() as u64,
            skipped_frames = occlusion.skipped_frames,
            "portal: doorway occlusion test ends for this doorway"
        );
    }
    occlusion.forget();
}

/// Stops the doorway view while the doorway is on screen but hidden behind the view's own
/// geometry: a wall, a house corner, a hill.
///
/// [`update_portal`] has already skipped a doorway off the screen (impl-188); one on it can still
/// be covered, and then the portal camera renders the whole far side, and its four shadow cascades,
/// for a quad the depth test throws away. This runs right after it, in the same frame, and takes
/// the same no-portal path's two writes when the doorway is hidden: the portal camera stops and
/// the quad is hidden. Everything else stays as it is - the door's leaf, its mirror, the
/// destination cells resident on the portal camera's layer - so the frame a sight line clears is
/// the frame the doorway is drawn again, with nothing to rebuild.
///
/// **The test.** Sight lines from the eye to a grid of points over the doorway
/// ([`doorway_occlusion_samples`]), grown past its edges and held off its plane toward the eye, are
/// cast against the meshes the main camera draws in the active space ([`sight_lines_blocked`]).
/// The doorway is hidden only when every line is blocked. What counts as a blocker is kept to what
/// certainly hides what is behind it: opaque [`StandardMaterial`] meshes and terrain - no
/// alpha-tested foliage, no blended or additive effects, no water - drawn by the main camera and
/// not the door's own model or its mirror. Each mesh's world box is tested against the box around
/// the lines first, so a test reads the triangles of the few meshes actually in the way.
///
/// **The rate.** A doorway in view is tested every [`OCCLUSION_RETEST_FRAMES`] frames and hidden
/// after [`OCCLUSION_CONFIRM_TESTS`] blocked tests in a row; a hidden doorway is tested every frame
/// and drawn again in the first frame any line is clear ([`DoorwayOcclusion`]). The test uses this
/// frame's camera pose, so there is no frame of lag on the way back. Near the doorway
/// ([`OCCLUSION_MIN_DISTANCE`]) it is never skipped.
#[allow(clippy::too_many_arguments)]
fn skip_occluded_doorway(
    state: Res<PortalState>,
    main: MainCameraQuery,
    mut portal_camera: Query<&mut Camera, With<PortalCamera>>,
    mut quad: Query<(&Transform, &mut Visibility), With<PortalQuad>>,
    occluders: OccluderQuery,
    parents: Query<&ChildOf>,
    copies: Query<(), With<DoorCopy>>,
    meshes: Res<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
    mut occlusion: Local<DoorwayOcclusion>,
) {
    let (
        Some(door),
        Ok((main_transform, ..)),
        Ok(mut camera),
        Ok((quad_transform, mut quad_visibility)),
    ) = (
        state.open_door,
        main.single(),
        portal_camera.single_mut(),
        quad.single_mut(),
    )
    else {
        end_occlusion_session(&mut occlusion);
        return;
    };
    let eye = main_transform.translation;
    let normal = quad_transform.rotation * Vec3::Z;
    if !camera.is_active
        || normal.dot(eye - quad_transform.translation).abs() < OCCLUSION_MIN_DISTANCE
    {
        // Off screen (the portal is already skipped) or close enough to walk through.
        end_occlusion_session(&mut occlusion);
        return;
    }
    if occlusion.door.is_some_and(|tested| tested != door) {
        end_occlusion_session(&mut occlusion);
    }
    let samples = doorway_occlusion_samples(quad_transform, eye);
    let was_hidden = occlusion.hidden;
    let mut tested = false;
    let started = Instant::now();
    let hidden = occlusion.update(door, || {
        tested = true;
        // The box around every sight line: a mesh outside it cannot be in the way of any.
        let (low, high) = samples.iter().fold((eye, eye), |(low, high), &sample| {
            (low.min(sample), high.max(sample))
        });
        let (low, high) = (Vec3A::from(low), Vec3A::from(high));
        // Whether anything up the parent chain rules the mesh out: the door itself (its leaf, its
        // frame), a door-layer door (`crate::door_layer`), or a cell outside the active space.
        let excluded = |entity: Entity| {
            let mut cursor = Some(entity);
            while let Some(current) = cursor {
                if current == door || copies.contains(current) {
                    return true;
                }
                if let Some(role) = state.roles.get(&current) {
                    return *role != CellRole::Active;
                }
                cursor = parents.get(current).ok().map(ChildOf::parent);
            }
            false
        };
        let main_layer = RenderLayers::layer(0);
        let mut candidates: Vec<(f32, Occluder)> = occluders
            .iter()
            .filter_map(
                |(entity, mesh, aabb, transform, visibility, layers, standard, terrain)| {
                    if !visibility.get() || !layers.unwrap_or(&main_layer).intersects(&main_layer) {
                        return None;
                    }
                    let transform = transform.affine();
                    let (box_low, box_high) = world_box(aabb, &transform);
                    if box_low.cmpgt(high).any() || box_high.cmplt(low).any() {
                        return None;
                    }
                    let opaque = terrain
                        || standard
                            .and_then(|material| materials.get(&material.0))
                            .is_some_and(|material| material.alpha_mode == AlphaMode::Opaque);
                    // The parent walk last: it is the dearest test, and only what is in the way and
                    // opaque gets that far.
                    if !opaque || excluded(entity) {
                        return None;
                    }
                    let mesh = meshes.get(&mesh.0)?;
                    Some((
                        (box_low + box_high).distance_squared(Vec3A::from(eye) * 2.0),
                        Occluder {
                            transform,
                            aabb: Aabb3d::new(aabb.center, aabb.half_extents),
                            mesh,
                        },
                    ))
                },
            )
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let occluders: Vec<Occluder> = candidates
            .into_iter()
            .map(|(_, occluder)| occluder)
            .collect();
        sight_lines_blocked(eye, &samples, &occluders)
    });
    if tested {
        let elapsed = started.elapsed();
        occlusion.tests += 1;
        occlusion.test_time += elapsed;
        occlusion.longest_test = occlusion.longest_test.max(elapsed);
    }
    if hidden {
        occlusion.skipped_frames += 1;
        camera.is_active = false;
        *quad_visibility = Visibility::Hidden;
    }
    if hidden != was_hidden {
        info!(
            skipped_frames = occlusion.skipped_frames,
            tests = occlusion.tests,
            mean_test_us = occlusion.test_time.as_micros() as u64 / occlusion.tests.max(1),
            longest_test_us = occlusion.longest_test.as_micros() as u64,
            "portal: doorway {}",
            if hidden {
                "hidden behind the view, not drawn"
            } else {
                "back in view"
            }
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The portal's cadence (the graphics setting `portal_every`, `--portal-every N`)
// ---------------------------------------------------------------------------------------------

/// The graphics setting `portal_every` (`crate::graphics_settings::GraphicsSettings::portal_every`,
/// `--portal-every N`, the graphics panel's "Portal redraw"): the portal camera renders one frame in
/// `N` while a doorway stays in view, and the doorway shows the last image it rendered on the
/// frames between. A setting, off by default (the user's decision, 2026-09-26): `N = 1` renders
/// every frame and never skips, and the setting is read every frame, so the panel changes it live.
///
/// # Why not `Camera::is_active`
///
/// The obvious skip - `is_active = false` on the frames between - tears the view down in the render
/// world every time, and the next rendered frame rebuilds it from nothing, which costs more than a
/// skipped frame saves:
///
/// * `bevy_render-0.19.0/src/camera.rs` `extract_cameras` removes `ExtractedCamera`,
///   `ExtractedView` and `RenderVisibleEntities` from an inactive camera's render entity, so the
///   next active frame starts from an empty visible set and every visible mesh is "added";
/// * `bevy_core_pipeline-0.19.0/src/core_3d/mod.rs` `extract_core_3d_camera_phases` skips an
///   inactive camera and then `retain`s only live views, dropping the view's retained
///   opaque/alpha-mask/transparent bins;
/// * `bevy_pbr-0.19.0/src/material.rs` `specialize_material_meshes` (and `prepass/mod.rs`'s twin)
///   `retain` their per-view `SpecializedMaterialPipelineCache` to the live views, so every mesh of
///   the destination is specialized and queued again the frame the camera comes back.
///
/// That teardown is what the off-screen and occluded-doorway skips already pay, once per
/// transition; paid every other frame it would be the cost of a newly opened doorway on every
/// frame the portal does render.
///
/// # The skip: the camera's render schedule, not its extraction
///
/// The camera stays active, so extraction, specialization, queuing and the per-view prepare keep
/// their retained state and do only their usual incremental work. What is skipped is the camera's
/// render schedule: `bevy_core_pipeline-0.19.0/src/schedule.rs` `camera_driver` runs one `Core3d`
/// schedule per entry of `SortedCameras`, so the portal camera's entry is taken out of that list
/// on a skipped frame ([`drop_skipped_portal_view`], after `sort_cameras` and `prepare_lights` -
/// `bevy_pbr-0.19.0/src/render/light.rs` iterates `SortedCameras` - and before `render_system`).
/// Everything the portal camera's `Core3d` schedule encodes goes with it: the GPU mesh
/// preprocessing and indirect-parameter dispatches for its view, the shadow passes of its cascades
/// (`per_view_shadow_pass`, in `Core3d` in `bevy_pbr-0.19.0/src/lib.rs`), its depth prepass, main
/// passes, the post-processing into the portal target and the depth composite's copy. The target
/// image and the composite's depth image are persistent assets, so they keep the last frame's
/// contents; the view's cached textures are still requested by the prepare systems every frame,
/// so the texture cache does not drop them.
///
/// Not saved: extraction, specialization, queuing, sorting, batching and the per-view prepare of
/// the portal view still run each frame.
///
/// # The rule ([`PortalCadence::step`])
///
/// A frame renders when the portal camera is showing a doorway it did not show last frame (the
/// camera was off, or it is another door), when the target was replaced (a resized target is a
/// new, never-drawn image), or when `N - 1` frames have been skipped in a row. So the first frame
/// of a doorway is never a stale or black image.
///
/// # What the skip looks like
///
/// On a skipped frame the doorway quad, the doorway rectangle and the composite's clip matrix are
/// this frame's, and the image is last frame's: the destination is drawn as seen from last frame's
/// eye, mapped onto this frame's doorway. A player standing still sees nothing; a moving one sees
/// the destination lag the doorway by one frame (a parallax error of the eye's movement over one
/// frame), and the destination's own animation (flames, a turning wheel) runs at 1/N the rate.
/// That lag is what a measured saving is weighed against.
pub(crate) mod cadence {
    use super::{
        PortalCamera, PortalState, PortalTexture, resize_portal_target, skip_occluded_doorway,
    };
    use crate::graphics_settings::GraphicsSettings;
    use bevy::{
        prelude::*,
        render::{
            Extract, ExtractSchedule, Render, RenderApp, RenderSystems, camera::SortedCameras,
            renderer::render_system, sync_world::RenderEntity,
        },
    };

    /// What the portal camera showed on a frame: the door it looked through and the target image
    /// it rendered into. `None` while the camera is off.
    pub(crate) type Shown = Option<(Entity, AssetId<Image>)>;

    /// The portal camera's frame-by-frame cadence under `portal_every`: whether it renders this
    /// frame, and what it showed last frame. Main world; the render world reads
    /// [`skips_this_frame`](Self::skips_this_frame) at extraction.
    #[derive(Resource, Debug, Default)]
    pub(crate) struct PortalCadence {
        /// Render one frame in this many while a doorway stays in view; `0` and `1` both mean every
        /// frame.
        pub(crate) every: u32,
        /// What the camera showed last frame.
        last: Shown,
        /// Frames skipped in a row since the camera last rendered.
        since_render: u32,
        /// Whether the camera is showing a doorway this frame and does not render it.
        skip: bool,
        /// Frames rendered and skipped since the doorway came into view, for the log line when it
        /// goes out.
        rendered: u64,
        skipped: u64,
    }

    impl PortalCadence {
        pub(crate) fn new(every: u32) -> Self {
            Self {
                every,
                ..Self::default()
            }
        }

        /// Decides this frame from what the camera shows (`None` while it is off) and returns
        /// whether the camera renders. A doorway the camera did not show last frame - the camera
        /// was off, it is another door, or the target image was replaced - always renders; one it
        /// keeps showing renders once every [`every`](Self::every) frames.
        pub(crate) fn step(&mut self, shown: Shown) -> bool {
            let every = self.every.max(1);
            let continuing = shown.is_some() && shown == self.last;
            self.last = shown;
            if shown.is_none() {
                self.since_render = 0;
                self.skip = false;
                return false;
            }
            let render = !continuing || self.since_render + 1 >= every;
            if render {
                self.since_render = 0;
                self.rendered += 1;
            } else {
                self.since_render += 1;
                self.skipped += 1;
            }
            self.skip = !render;
            render
        }

        /// Whether the portal camera is active this frame and its render schedule is skipped.
        pub(crate) fn skips_this_frame(&self) -> bool {
            self.skip
        }
    }

    /// Adds the cadence. It starts at every frame and follows [`GraphicsSettings::portal_every`]
    /// from the first frame on.
    pub(crate) struct PortalCadencePlugin;

    impl Plugin for PortalCadencePlugin {
        fn build(&self, app: &mut App) {
            app.insert_resource(PortalCadence::new(1))
                // After both systems that switch the camera off and after the target is resized,
                // so this frame's `is_active` and target are final.
                .add_systems(
                    Update,
                    pace_portal_camera
                        .after(skip_occluded_doorway)
                        .after(resize_portal_target),
                );
            if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                render_app
                    .init_resource::<SkippedPortalView>()
                    .add_systems(ExtractSchedule, extract_portal_skip)
                    .add_systems(
                        Render,
                        drop_skipped_portal_view
                            .in_set(RenderSystems::Render)
                            .before(render_system),
                    );
            }
        }
    }

    /// Steps the cadence for this frame, at this frame's [`GraphicsSettings::portal_every`].
    fn pace_portal_camera(
        mut cadence: ResMut<PortalCadence>,
        state: Option<Res<PortalState>>,
        texture: Option<Res<PortalTexture>>,
        camera: Query<&Camera, With<PortalCamera>>,
        settings: Option<Res<GraphicsSettings>>,
    ) {
        if let Some(settings) = settings {
            let every = settings.portal_redraw_every();
            if cadence.every != every {
                info!(
                    every,
                    "portal: the portal camera renders one frame in {every} while a doorway stays                      in view"
                );
                cadence.every = every;
            }
        }
        let shown = match (camera.single(), state, texture) {
            (Ok(camera), Some(state), Some(texture)) if camera.is_active => {
                state.open_door.map(|door| (door, texture.0.id()))
            }
            _ => None,
        };
        let was_shown = cadence.last.is_some();
        cadence.step(shown);
        // At every frame there is nothing to report.
        if was_shown && shown.is_none() && (cadence.every > 1 || cadence.skipped > 0) {
            info!(
                rendered = cadence.rendered,
                skipped = cadence.skipped,
                every = cadence.every,
                "portal: doorway out of view; frames the portal camera rendered and skipped"
            );
            cadence.rendered = 0;
            cadence.skipped = 0;
        }
    }

    /// The render entity of the portal camera when its render schedule is skipped this frame.
    #[derive(Resource, Default)]
    struct SkippedPortalView(Option<Entity>);

    fn extract_portal_skip(
        mut skipped: ResMut<SkippedPortalView>,
        cadence: Extract<Option<Res<PortalCadence>>>,
        camera: Extract<Query<RenderEntity, With<PortalCamera>>>,
    ) {
        skipped.0 = cadence
            .as_ref()
            .is_some_and(|cadence| cadence.skips_this_frame())
            .then(|| camera.single().ok())
            .flatten();
    }

    /// Takes the skipped portal camera out of the list `camera_driver` runs render schedules for.
    fn drop_skipped_portal_view(
        skipped: Res<SkippedPortalView>,
        mut sorted: ResMut<SortedCameras>,
    ) {
        if let Some(entity) = skipped.0 {
            sorted.0.retain(|camera| camera.entity != entity);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn door(index: u32) -> Entity {
            Entity::from_raw_u32(index).unwrap()
        }

        fn target(index: u128) -> AssetId<Image> {
            AssetId::Uuid {
                uuid: bevy::asset::uuid::Uuid::from_u128(index),
            }
        }

        #[test]
        fn a_newly_shown_doorway_renders_its_first_frame() {
            for every in [1, 2, 3, 8] {
                let mut cadence = PortalCadence::new(every);
                assert!(!cadence.step(None));
                assert!(
                    cadence.step(Some((door(1), target(1)))),
                    "first frame of a doorway at every={every}"
                );
                assert!(!cadence.skips_this_frame());
                // Out of view and back: the first frame back renders too.
                cadence.step(None);
                assert!(!cadence.skips_this_frame(), "nothing to skip while off");
                assert!(cadence.step(Some((door(1), target(1)))));
            }
        }

        #[test]
        fn every_one_never_skips() {
            for every in [0, 1] {
                let mut cadence = PortalCadence::new(every);
                for _ in 0..10 {
                    assert!(cadence.step(Some((door(1), target(1)))));
                    assert!(!cadence.skips_this_frame());
                }
            }
        }

        #[test]
        fn every_two_alternates_while_the_doorway_stays_in_view() {
            let mut cadence = PortalCadence::new(2);
            let shown = Some((door(1), target(1)));
            let frames: Vec<bool> = (0..6).map(|_| cadence.step(shown)).collect();
            assert_eq!(frames, [true, false, true, false, true, false]);
            assert!(cadence.skips_this_frame());
        }

        #[test]
        fn every_three_renders_one_frame_in_three() {
            let mut cadence = PortalCadence::new(3);
            let shown = Some((door(1), target(1)));
            let frames: Vec<bool> = (0..7).map(|_| cadence.step(shown)).collect();
            assert_eq!(frames, [true, false, false, true, false, false, true]);
        }

        #[test]
        fn another_door_or_a_new_target_renders_at_once() {
            let mut cadence = PortalCadence::new(4);
            assert!(cadence.step(Some((door(1), target(1)))));
            assert!(!cadence.step(Some((door(1), target(1)))));
            // Another door: its destination is not in the image.
            assert!(cadence.step(Some((door(2), target(1)))));
            assert!(!cadence.step(Some((door(2), target(1)))));
            // A resized target is a new image nothing has been drawn into.
            assert!(cadence.step(Some((door(2), target(2)))));
            assert!(!cadence.skips_this_frame());
        }

        #[test]
        fn the_cadence_follows_the_graphics_setting_live() {
            let mut app = App::new();
            let mut settings = GraphicsSettings::current();
            assert_eq!(settings.portal_every, 1, "every frame by default");
            settings.portal_every = 2;
            app.insert_resource(settings)
                .insert_resource(PortalCadence::new(1))
                .add_systems(Update, pace_portal_camera);
            app.update();
            assert_eq!(app.world().resource::<PortalCadence>().every, 2);
            app.world_mut()
                .resource_mut::<GraphicsSettings>()
                .portal_every = 1;
            app.update();
            assert_eq!(app.world().resource::<PortalCadence>().every, 1);
            // A value outside the setting's range is held to it.
            app.world_mut()
                .resource_mut::<GraphicsSettings>()
                .portal_every = 9;
            app.update();
            assert_eq!(app.world().resource::<PortalCadence>().every, 3);
            // No camera, so nothing is shown and nothing is skipped.
            assert!(!app.world().resource::<PortalCadence>().skips_this_frame());
        }

        #[test]
        fn switching_back_to_every_frame_mid_doorway_stops_the_skips_at_once() {
            let mut cadence = PortalCadence::new(2);
            let shown = Some((door(1), target(1)));
            assert!(cadence.step(shown));
            assert!(!cadence.step(shown));
            cadence.every = 1;
            for _ in 0..5 {
                assert!(cadence.step(shown));
                assert!(!cadence.skips_this_frame());
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The depth composite (impl-211's spike, made the default by impl-227)
// ---------------------------------------------------------------------------------------------

/// The doorway composited by depth instead of by the quad's rectangle: the default doorway since
/// impl-227 (impl-211 was the spike), with `--portal-depth-composite=off` as the escape hatch that
/// brings back the plain quad for comparison.
///
/// The plain quad writes its own plane's depth, so everything of the source space behind that
/// plane is painted over by the rectangle - including the parts of the doorway that stand behind
/// it (the jamb's inner faces, the underside of a lintel), which is how the destination's beams
/// come to cross the exterior frame at grazing angles. Here the quad writes the depth of what the
/// portal camera actually drew at each pixel instead, so the source geometry that is nearer
/// occludes it at whatever shape the opening really has:
///
/// * the portal camera's depth buffer is copied, after its main pass, into an image the quad's
///   material binds ([`copy_portal_depth`], a system in the render world's `Core3d` schedule);
/// * the quad's shader (`portal.wgsl` under `PORTAL_DEPTH_COMPOSITE`) turns that depth back into a
///   view-space point with the inverse of the portal camera's own clip matrix (oblique near plane
///   and doorway sub-view included), and projects it with the main camera's;
/// * the written depth is never in front of the doorway plane, and never further behind it than
///   the door's **slab** ([`doorway_slab`]) - a depth measured along the plane's normal, so the
///   same slab holds at every viewing angle;
/// * the quad is left out of the main camera's depth prepass (it writes no depth there), since the
///   prepass would otherwise store the plane's depth and the main pass's `GreaterEqual` test would
///   then reject every fragment the composite pushed behind it.
///
/// # The filler off, then a slab measured per doorway
///
/// A load door's model carries the plug the game hides the void behind a closed door with -
/// `FarmhouseLDoor01`'s `DoorBlack` (an open box of near-black boards: its back 22.5 units behind
/// the doorway plane, its side walls the depth of the box), the Dwemer doors' `Plane02` and
/// `Plane04`, the small Nordic door's `Object05:7`, Whiterun's `DoorToBlack` and Riften's
/// `RiftenDoorBG`. Behind the plane, a written depth that reaches the plug lets the plug win: the
/// doorway goes black at a large slab (impl-211: 24 and 64 did, on every Riverwood house), and at
/// any slab its side and top walls draw as black strips down the jambs and across the lintel
/// (Honningbrew's `WRShackDoor01`, the dark band under Sven's lintel).
///
/// So, two steps, both general rules rather than a list of models:
///
/// 1. **The filler is taken off the main camera** while its doorway is drawn
///    ([`hide_doorway_filler`]): every static mesh of the door's own model - not under its
///    [`DoorLeaf`] - whose box centre stands behind the doorway plane ([`is_doorway_filler`]). It is
///    moved to a layer no camera draws, not hidden: the doorway's floor probe ray-casts by
///    visibility and can land on it.
/// 2. **The slab is measured per doorway** ([`measure_doorway_slab`]): rays from the doorway plane
///    straight back through the opening (a grid over its middle and a row along its sill) against
///    everything the main view still draws of the source space, both faces; the slab is the
///    nearest hit less [`COMPOSITE_SLAB_MARGIN`], at most [`COMPOSITE_SLAB_CAP`]. The filler is not
///    the only thing behind a doorway: the rays behind Riverwood's `FarmhouseLDoor01` doorways
///    meet the house's own geometry about 29 units back (slab 27), and the hillside behind
///    Chillfurrow Farm's door rises above the room's floor (slab 11, found by the sill row).
///
/// The written depth then carries the destination's own depth pulled [`COMPOSITE_TIE_BIAS`]
/// nearer, so the source's doorstep, flush with the room's floor stood on it, does not win the
/// tie.
pub(crate) mod depth_composite {
    use super::{
        PortalCamera, PortalQuad, PortalState, PortalTexture, doorway_rect_uniform,
        fit_portal_view, setup_portal_quad, world_box,
    };
    use crate::doors::DoorLeaf;
    use bevy::{
        asset::{AssetId, RenderAssetUsages},
        camera::{
            CameraUpdateSystems,
            primitives::Aabb,
            visibility::{RenderLayers, VisibilitySystems},
        },
        core_pipeline::{Core3d, Core3dSystems, core_3d::main_transparent_pass_3d},
        math::{Affine3A, Vec3A},
        mesh::MeshVertexBufferLayoutRef,
        mesh::{Indices, PrimitiveTopology},
        pbr::{
            ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
            MaterialPlugin,
        },
        picking::mesh_picking::ray_cast::{Backfaces, ray_mesh_intersection},
        prelude::*,
        render::{
            Extract, ExtractSchedule, RenderApp,
            render_asset::RenderAssets,
            render_resource::{
                AsBindGroup, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError,
                TextureDimension, TextureFormat, TextureUsages,
            },
            renderer::{RenderContext, ViewQuery},
            sync_world::RenderEntity,
            texture::GpuImage,
            view::ViewDepthTexture,
        },
        shader::ShaderRef,
    };

    /// The slab of a doorway with nothing behind its opening, and the most any doorway gets, in
    /// Creation units along the doorway plane's normal: how far behind the plane the source space
    /// may still occlude the destination. Anything of the source further behind than this - the
    /// far side of the house's shell, a tree behind it - is behind the written depth and stays
    /// hidden, as it is under the plain quad.
    ///
    /// A doorway's reveal is as deep as the wall it is set in. The plugs of the load-door models
    /// the game uses most stand 14 to 28 units behind their doorway planes (the reveal ends at the
    /// plug), so 32 lets a doorway occlude by about a wall's depth and no more.
    pub(crate) const COMPOSITE_SLAB_CAP: f32 = 32.0;

    /// How far in front of the door model's own backing geometry the slab stops, in units: the
    /// card is flat, and a written depth that ends a little short of it never ties with it.
    pub(crate) const COMPOSITE_SLAB_MARGIN: f32 = 2.0;

    /// How much nearer, in units along the view axis, the composite writes the destination's
    /// depth than it is: a source surface within this of a destination surface loses to it.
    ///
    /// A doorstep is where the two spaces meet flush - the map stands the destination's floor on
    /// the source threshold, and the step's own top is that threshold - so behind the doorway
    /// plane the two are one surface drawn twice, and the bias gives the tie to the room. Checked
    /// at Gerdur's House (the user's capture of 2026-09-25, door open): with it the sill meets the
    /// room's floor with no band of porch boards over it (`local/t227/gerdur-threshold-on.png`).
    pub(crate) const COMPOSITE_TIE_BIAS: f32 = 4.0;

    /// The probe rays per side of the doorway ([`slab_probe_rays`]): a 4x4 grid.
    const SLAB_PROBE_GRID: usize = 4;

    /// The part of the doorway's width and height the probe grid spans, around the middle: the
    /// outer 15% at each edge is left out, so the rays go through the opening and not down the
    /// jambs' own faces, which are the reveal the slab is there to keep.
    const SLAB_PROBE_SPAN: f32 = 0.7;

    /// How high above the doorway's bottom edge the sill row of probe rays starts, as a part of
    /// the doorway's height ([`slab_probe_rays`]).
    ///
    /// The quad's bottom edge is the threshold the destination floor is stood on, so source ground
    /// that rises behind the doorway plane - the terrain inside a house's footprint on a slope,
    /// which the house's shell hides in the game - stands *above* the destination's floor there.
    /// Within the slab it would occlude the floor of the room: Chillfurrow Farm's doorway drew a
    /// band of the hillside across its threshold with a slab of 32. The sill row finds it.
    const SILL_PROBE_HEIGHT: f32 = 0.02;

    /// The probe rays of a doorway quad: from points of its plane on a grid over the middle of the
    /// opening and a row just above its sill, along the plane's normal away from the side the quad
    /// faces (the player's).
    pub(crate) fn slab_probe_rays(quad: &Transform) -> Vec<Ray3d> {
        let Ok(behind) = Dir3::new(quad.rotation * Vec3::NEG_Z) else {
            return Vec::new();
        };
        let step = SLAB_PROBE_SPAN / (SLAB_PROBE_GRID - 1) as f32;
        let first = -SLAB_PROBE_SPAN * 0.5;
        let mut rays = Vec::with_capacity(SLAB_PROBE_GRID * (SLAB_PROBE_GRID + 1));
        for row in 0..SLAB_PROBE_GRID {
            for column in 0..SLAB_PROBE_GRID {
                let local = Vec3::new(first + step * column as f32, first + step * row as f32, 0.0);
                rays.push(Ray3d::new(quad.transform_point(local), behind));
            }
        }
        // And a row along the sill, for ground that rises behind the doorway.
        for column in 0..SLAB_PROBE_GRID {
            let local = Vec3::new(first + step * column as f32, SILL_PROBE_HEIGHT - 0.5, 0.0);
            rays.push(Ray3d::new(quad.transform_point(local), behind));
        }
        rays
    }

    /// The slab from the probe rays' hits ([`slab_probe_rays`], a distance behind the plane each):
    /// the nearest hit less [`COMPOSITE_SLAB_MARGIN`], never below zero (a card in the plane
    /// itself: the plain quad's depth) and never above [`COMPOSITE_SLAB_CAP`] (nothing hit at all).
    pub(crate) fn doorway_slab(hits: impl IntoIterator<Item = f32>) -> f32 {
        hits.into_iter()
            .filter(|hit| hit.is_finite())
            .map(|hit| hit - COMPOSITE_SLAB_MARGIN)
            .fold(COMPOSITE_SLAB_CAP, f32::min)
            .max(0.0)
    }

    /// How many frames a doorway keeps its measured slab before [`measure_doorway_slab`] takes it
    /// again: half a second at the demo's frame rate.
    const SLAB_REMEASURE_FRAMES: u32 = 30;

    /// The slab of the doorway the portal draws through ([`doorway_slab`]), what it was measured
    /// on (the door and its doorway quad) and how many frames ago.
    #[derive(Resource, Debug)]
    struct DoorwaySlab {
        measured: Option<(Entity, Transform)>,
        frames: u32,
        slab: f32,
    }

    impl Default for DoorwaySlab {
        fn default() -> Self {
            Self {
                measured: None,
                frames: 0,
                slab: COMPOSITE_SLAB_CAP,
            }
        }
    }

    /// How far behind the doorway plane a mesh's box centre has to stand, in units, for
    /// [`is_doorway_filler`] to call it the door model's filler.
    const FILLER_BEHIND: f32 = 0.5;

    /// Whether a static mesh of a door's own model, with its world box centred on `centre`, is the
    /// model's **filler** - the plug the game hides the void behind a closed load door with - for
    /// the doorway quad `quad`: its centre stands behind the doorway plane, on the side away from
    /// the player.
    pub(crate) fn is_doorway_filler(centre: Vec3, quad: &Transform) -> bool {
        let behind = quad.rotation * Vec3::NEG_Z;
        (centre - quad.translation).dot(behind) > FILLER_BEHIND
    }

    /// The layer [`hide_doorway_filler`] moves the filler to: one no camera renders. A layer rather
    /// than [`Visibility::Hidden`], because the filler still has to be *there* for everything that
    /// ray-casts the world by visibility: `DoorBlack`'s floor is the source floor
    /// [`super::anchor_on_measured_floors`] stands the doorway on (hidden, the probe fell 22 units
    /// through to the ground below Sven's House and the doorway image slid down with it).
    const FILLER_LAYER: usize = 31;

    /// The door-model meshes [`hide_doorway_filler`] moved off the cameras, with the layers each
    /// had (`None`: no [`RenderLayers`], which is layer 0).
    #[derive(Resource, Debug, Default)]
    struct HiddenFiller(Vec<(Entity, Option<RenderLayers>)>);

    /// Takes the filler ([`is_doorway_filler`]) of the door the portal draws through off the main
    /// camera, for as long as it does, and gives every other mesh it moved its layers back.
    ///
    /// Only a door with a leaf ([`DoorLeaf`]) is looked at: its frame stays drawn while it is
    /// open, and the leaf is `crate::door_animation`'s. A door with no animation of its own has
    /// its whole model hidden already ([`super::show_load_door_leaves`]).
    ///
    /// A cell that stops being active has its layers rewritten by [`super::isolate_cells`] every
    /// frame, so a filler mesh given back while its cell is a destination is put on the
    /// destination's layer again the next frame.
    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    fn hide_doorway_filler(
        mut commands: Commands,
        state: Option<Res<PortalState>>,
        door_layer: Option<Res<crate::door_layer::DoorLayer>>,
        mut hidden: ResMut<HiddenFiller>,
        children: Query<&Children>,
        leaves: Query<(), With<DoorLeaf>>,
        meshes: Query<(&GlobalTransform, &Aabb, Option<&RenderLayers>), With<Mesh3d>>,
        names: Query<&Name>,
    ) {
        let mut filler = Vec::new();
        if let Some(doorway) = state.and_then(|state| state.open_doorway()) {
            let mut statics = Vec::new();
            let mut has_leaf = false;
            // The door-layer door drawing the doorway's pair when there is one - its natives are
            // off the cameras already (`crate::door_layer`) - and the door's own model otherwise.
            let drawn = door_layer
                .as_ref()
                .and_then(|layer| layer.drawn_copy_of(doorway.door))
                .unwrap_or(doorway.door);
            let mut stack = vec![drawn];
            while let Some(entity) = stack.pop() {
                if leaves.contains(entity) {
                    has_leaf = true;
                    continue;
                }
                if meshes.contains(entity) {
                    statics.push(entity);
                }
                if let Ok(children) = children.get(entity) {
                    stack.extend(children.iter());
                }
            }
            if has_leaf {
                filler.extend(statics.into_iter().filter(|entity| {
                    meshes.get(*entity).is_ok_and(|(transform, aabb, _)| {
                        is_doorway_filler(
                            transform.transform_point(Vec3::from(aabb.center)),
                            &doorway.quad,
                        )
                    })
                }));
            }
        }
        // Give back what is no longer filler of the doorway drawn. A mesh despawned since (its
        // cell unloaded) has nothing to give back to.
        hidden.0.retain(|(entity, original)| {
            if filler.contains(entity) {
                return true;
            }
            if meshes.contains(*entity) {
                match original {
                    Some(layers) => {
                        commands.entity(*entity).try_insert(layers.clone());
                    }
                    None => {
                        commands.entity(*entity).try_remove::<RenderLayers>();
                    }
                }
            }
            false
        });
        for entity in filler {
            if hidden.0.iter().any(|(hidden, _)| *hidden == entity) {
                continue;
            }
            if let Ok((_, _, layers)) = meshes.get(entity) {
                hidden.0.push((entity, layers.cloned()));
                commands
                    .entity(entity)
                    .try_insert(RenderLayers::layer(FILLER_LAYER));
                debug!(
                    ?entity,
                    name = names.get(entity).map(Name::as_str).unwrap_or(""),
                    "portal: doorway filler taken off the cameras"
                );
            }
        }
    }

    /// The layers the main camera draws (`app::setup_world`): what the slab probe counts as drawn.
    const MAIN_VIEW_LAYERS: [usize; 2] = [0, 1];

    /// The distance along `ray` to the nearest face of `mesh` placed by `transform`, **either**
    /// side: a surface the main view draws double-sided stops the written depth whichever way its
    /// triangles wind, and a probe that culled back faces would measure straight past it.
    fn ray_hits_either_face(mesh: &Mesh, transform: &Affine3A, ray: Ray3d) -> Option<f32> {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
            return None;
        }
        let positions = mesh
            .try_attribute(Mesh::ATTRIBUTE_POSITION)
            .ok()?
            .as_float3()?;
        let hit = match mesh.try_indices().ok() {
            Some(Indices::U16(indices)) => ray_mesh_intersection(
                ray,
                transform,
                positions,
                None,
                Some(indices),
                None,
                Backfaces::Include,
            ),
            Some(Indices::U32(indices)) => ray_mesh_intersection(
                ray,
                transform,
                positions,
                None,
                Some(indices),
                None,
                Backfaces::Include,
            ),
            None => ray_mesh_intersection::<u32>(
                ray,
                transform,
                positions,
                None,
                None,
                None,
                Backfaces::Include,
            ),
        };
        hit.map(|hit| hit.distance)
    }

    /// Measures [`DoorwaySlab`] for the doorway the portal draws through this frame: the probe
    /// rays ([`slab_probe_rays`]) against everything the main camera draws of the active space -
    /// the house the door is set in as well as the door's own model - except the doorway quad and
    /// anything under a [`DoorLeaf`] (a leaf swings; the doorway's mirror is the destination
    /// image's), both faces of every triangle ([`ray_hits_either_face`]). The door model's filler
    /// is off the main camera by then ([`hide_doorway_filler`]), so what the rays find is the
    /// first thing of the source space that really stands behind the opening.
    ///
    /// Taken again when the door or its quad changes, and every [`SLAB_REMEASURE_FRAMES`] frames
    /// while it stays - the house around a door is a glTF scene that can arrive after the door.
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn measure_doorway_slab(
        state: Option<Res<PortalState>>,
        mut slab: ResMut<DoorwaySlab>,
        meshes: Query<(
            Entity,
            &Mesh3d,
            &Aabb,
            &GlobalTransform,
            &InheritedVisibility,
            Option<&RenderLayers>,
        )>,
        mesh_assets: Res<Assets<Mesh>>,
        parents: Query<&ChildOf>,
        leaves: Query<(), With<DoorLeaf>>,
        quads: Query<(), With<PortalQuad>>,
    ) {
        let Some(state) = state else {
            return;
        };
        let Some(doorway) = state.open_doorway() else {
            slab.measured = None;
            return;
        };
        slab.frames += 1;
        if slab.measured == Some((doorway.door, doorway.quad))
            && slab.frames < SLAB_REMEASURE_FRAMES
        {
            return;
        }
        let rays = slab_probe_rays(&doorway.quad);
        // The box the rays sweep, out to the cap: only a mesh whose box meets it can stop one.
        let reach = COMPOSITE_SLAB_CAP + COMPOSITE_SLAB_MARGIN;
        let (mut low, mut high) = (Vec3A::splat(f32::INFINITY), Vec3A::splat(f32::NEG_INFINITY));
        for ray in &rays {
            for point in [ray.origin, ray.get_point(reach)] {
                low = low.min(point.into());
                high = high.max(point.into());
            }
        }
        let main_layers = RenderLayers::from_layers(&MAIN_VIEW_LAYERS);
        let under_leaf = |entity: Entity| {
            let mut cursor = Some(entity);
            while let Some(current) = cursor {
                if leaves.contains(current) {
                    return true;
                }
                cursor = parents.get(current).ok().map(ChildOf::parent);
            }
            false
        };
        let mut blockers = Vec::new();
        for (entity, mesh, aabb, transform, visibility, layers) in &meshes {
            if !visibility.get()
                || quads.contains(entity)
                || layers.is_some_and(|layers| !layers.intersects(&main_layers))
            {
                continue;
            }
            let transform = transform.affine();
            let (min, max) = world_box(aabb, &transform);
            if min.cmpgt(high).any() || max.cmplt(low).any() {
                continue;
            }
            if under_leaf(entity) || !state.is_in_active_space(entity, &parents) {
                continue;
            }
            if let Some(mesh) = mesh_assets.get(&mesh.0) {
                blockers.push((mesh, transform));
            }
        }
        let hits = rays
            .iter()
            .filter_map(|ray| {
                blockers
                    .iter()
                    .filter_map(|(mesh, transform)| ray_hits_either_face(mesh, transform, *ray))
                    .reduce(f32::min)
            })
            .collect::<Vec<_>>();
        let measured = doorway_slab(hits.iter().copied());
        if slab.measured.map(|(door, _)| door) != Some(doorway.door)
            || (measured - slab.slab).abs() > 0.5
        {
            info!(
                door = ?doorway.door,
                hits = hits.len(),
                slab = measured,
                "portal: doorway depth slab measured"
            );
        }
        slab.measured = Some((doorway.door, doorway.quad));
        slab.frames = 0;
        slab.slab = measured;
    }

    /// The format the portal camera's depth is copied into: Bevy's own 3D depth format, which a
    /// texture-to-texture copy requires the two sides to share.
    const PORTAL_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;

    /// The copy of the portal camera's depth the quad samples.
    #[derive(Resource)]
    struct PortalDepthTexture(Handle<Image>);

    /// On the portal camera: the image its depth is copied into after its main pass.
    #[derive(Component, Clone)]
    struct PortalDepthCopy(Handle<Image>);

    /// [`PortalDepthCopy`] in the render world.
    #[derive(Component)]
    struct PortalDepthCopyTarget(AssetId<Image>);

    /// The doorway quad's material under the composite: the default's three bindings plus the
    /// portal camera's depth, the inverse of its clip matrix, and the slab.
    #[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
    pub struct PortalDepthExtension {
        #[texture(100)]
        #[sampler(101)]
        portal_texture: Option<Handle<Image>>,
        #[uniform(102)]
        doorway_rect: Vec4,
        #[texture(103, sample_type = "depth")]
        portal_depth: Option<Handle<Image>>,
        #[uniform(104)]
        portal_view_from_clip: Mat4,
        /// `x`: the doorway's slab ([`DoorwaySlab`]), in units along the doorway plane's normal;
        /// `y`: [`COMPOSITE_TIE_BIAS`].
        #[uniform(105)]
        composite: Vec4,
    }

    impl MaterialExtension for PortalDepthExtension {
        fn fragment_shader() -> ShaderRef {
            "embedded://engine/shaders/portal.wgsl".into()
        }

        fn specialize(
            _pipeline: &MaterialExtensionPipeline,
            descriptor: &mut RenderPipelineDescriptor,
            _layout: &MeshVertexBufferLayoutRef,
            _key: MaterialExtensionKey<Self>,
        ) -> Result<(), SpecializedMeshPipelineError> {
            // The depth prepass (and the shadow pass, which is the same pipeline; Bevy labels it
            // `prepass_pipeline`, and `StandardMaterial` relabels it `pbr_prepass_pipeline`) writes no depth
            // for the quad: its plane's depth there would make the main pass reject every
            // composite fragment that lies behind it.
            if descriptor
                .label
                .as_deref()
                .is_some_and(|label| label.ends_with("prepass_pipeline"))
            {
                if let Some(depth) = descriptor.depth_stencil.as_mut() {
                    depth.depth_write_enabled = Some(false);
                }
            } else if let Some(fragment) = descriptor.fragment.as_mut() {
                fragment.shader_defs.push("PORTAL_DEPTH_COMPOSITE".into());
            }
            Ok(())
        }
    }

    type PortalDepthMaterial = ExtendedMaterial<StandardMaterial, PortalDepthExtension>;

    /// Registers the composite. Added by [`super::PortalPlugin`] to every run with a doorway,
    /// unless `--portal-depth-composite=off` asked for the plain quad.
    pub(crate) struct DepthCompositePlugin;

    impl Plugin for DepthCompositePlugin {
        fn build(&self, app: &mut App) {
            app.add_plugins(MaterialPlugin::<PortalDepthMaterial>::default())
                .init_resource::<DoorwaySlab>()
                .init_resource::<HiddenFiller>()
                .add_systems(Startup, setup_depth_composite.after(setup_portal_quad))
                .add_systems(
                    PostUpdate,
                    (
                        hide_doorway_filler
                            .after(TransformSystems::Propagate)
                            .before(VisibilitySystems::VisibilityPropagate),
                        // The door model's meshes where this frame draws them.
                        measure_doorway_slab
                            .after(TransformSystems::Propagate)
                            .after(VisibilitySystems::VisibilityPropagate),
                        sync_depth_composite
                            .after(CameraUpdateSystems)
                            .after(fit_portal_view),
                    )
                        .chain(),
                );
            if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                render_app
                    .add_systems(ExtractSchedule, extract_portal_depth_copy)
                    .add_systems(
                        Core3d,
                        copy_portal_depth
                            .after(main_transparent_pass_3d)
                            .in_set(Core3dSystems::MainPass),
                    );
            }
        }
    }

    /// A depth image the portal camera's depth can be copied into and the quad can sample.
    fn portal_depth_image(size: UVec2) -> Image {
        let mut image = Image::new_uninit(
            Extent3d {
                width: size.x,
                height: size.y,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            PORTAL_DEPTH_FORMAT,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
        image
    }

    /// Swaps the quad's default material for the composite's, and points the portal camera's
    /// depth copy at a first image.
    fn setup_depth_composite(
        mut commands: Commands,
        texture: Res<PortalTexture>,
        mut images: ResMut<Assets<Image>>,
        mut materials: ResMut<Assets<PortalDepthMaterial>>,
        quad: Query<(Entity, &MeshMaterial3d<super::PortalMaterial>), With<PortalQuad>>,
        base_materials: Res<Assets<super::PortalMaterial>>,
        camera: Query<Entity, With<PortalCamera>>,
    ) {
        let size = images
            .get(&texture.0)
            .map(Image::size)
            .unwrap_or(super::PORTAL_TEXTURE_FALLBACK_SIZE);
        let depth = images.add(portal_depth_image(size));
        commands.insert_resource(PortalDepthTexture(depth.clone()));
        if let Ok(camera) = camera.single() {
            commands
                .entity(camera)
                .insert(PortalDepthCopy(depth.clone()));
        }
        let Ok((entity, handle)) = quad.single() else {
            return;
        };
        let base = base_materials
            .get(handle)
            .map(|material| material.base.clone())
            .unwrap_or_default();
        let material = materials.add(PortalDepthMaterial {
            base,
            extension: PortalDepthExtension {
                portal_texture: Some(texture.0.clone()),
                doorway_rect: doorway_rect_uniform(None),
                portal_depth: Some(depth),
                portal_view_from_clip: Mat4::IDENTITY,
                composite: Vec4::new(COMPOSITE_SLAB_CAP, COMPOSITE_TIE_BIAS, 0.0, 0.0),
            },
        });
        commands
            .entity(entity)
            .remove::<MeshMaterial3d<super::PortalMaterial>>()
            .insert(MeshMaterial3d(material));
    }

    /// Keeps the composite's material and depth image in step with the portal camera, every
    /// frame, after the camera's matrices are final: the depth image follows the colour target's
    /// size (a depth copy must cover the whole texture), and the material carries the target, the
    /// doorway rectangle and the inverse of the clip matrix the portal camera renders with this
    /// frame, and the doorway's slab ([`measure_doorway_slab`]). The material is written only when
    /// one of them changed.
    #[allow(clippy::too_many_arguments)]
    fn sync_depth_composite(
        state: Option<Res<PortalState>>,
        texture: Res<PortalTexture>,
        depth: Option<ResMut<PortalDepthTexture>>,
        mut images: ResMut<Assets<Image>>,
        mut materials: ResMut<Assets<PortalDepthMaterial>>,
        mut camera: Query<(&Camera, &mut PortalDepthCopy), With<PortalCamera>>,
        quad: Query<&MeshMaterial3d<PortalDepthMaterial>, With<PortalQuad>>,
        slab: Res<DoorwaySlab>,
    ) {
        let Some(mut depth) = depth else {
            return;
        };
        let Some(size) = images.get(&texture.0).map(Image::size) else {
            return;
        };
        if images.get(&depth.0).map(Image::size) != Some(size) {
            depth.0 = images.add(portal_depth_image(size));
        }
        let Ok((camera, mut copy)) = camera.single_mut() else {
            return;
        };
        if copy.0 != depth.0 {
            copy.0 = depth.0.clone();
        }
        let view_from_clip = camera.clip_from_view().inverse();
        let doorway_rect = doorway_rect_uniform(state.and_then(|state| state.render_rect));
        let composite = Vec4::new(slab.slab, COMPOSITE_TIE_BIAS, 0.0, 0.0);
        let Ok(handle) = quad.single() else {
            return;
        };
        let stale = materials.get(handle).is_some_and(|material| {
            let extension = &material.extension;
            extension.portal_texture.as_ref() != Some(&texture.0)
                || extension.portal_depth.as_ref() != Some(&depth.0)
                || extension.doorway_rect != doorway_rect
                || extension.portal_view_from_clip != view_from_clip
                || extension.composite != composite
        });
        if stale && let Some(mut material) = materials.get_mut(handle) {
            material.extension.portal_texture = Some(texture.0.clone());
            material.extension.portal_depth = Some(depth.0.clone());
            material.extension.doorway_rect = doorway_rect;
            material.extension.portal_view_from_clip = view_from_clip;
            material.extension.composite = composite;
        }
    }

    fn extract_portal_depth_copy(
        mut commands: Commands,
        cameras: Extract<Query<(RenderEntity, &PortalDepthCopy)>>,
    ) {
        for (entity, copy) in &cameras {
            commands
                .entity(entity)
                .insert(PortalDepthCopyTarget(copy.0.id()));
        }
    }

    /// Copies the portal camera's depth buffer, after its main pass, into the image the quad
    /// samples. Runs in every camera's `Core3d` schedule and does nothing for a view without
    /// [`PortalDepthCopyTarget`] - every view but the portal camera's. A frame whose depth image
    /// has not caught up with a resized target is skipped: the copy must cover the whole texture.
    fn copy_portal_depth(
        view: ViewQuery<(&ViewDepthTexture, &PortalDepthCopyTarget)>,
        images: Res<RenderAssets<GpuImage>>,
        mut ctx: RenderContext,
    ) {
        let (depth, target) = view.into_inner();
        let Some(image) = images.get(target.0) else {
            return;
        };
        let size = depth.texture.size();
        if image.texture.size() != size {
            return;
        }
        ctx.command_encoder().copy_texture_to_texture(
            depth.texture.as_image_copy(),
            image.texture.as_image_copy(),
            size,
        );
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::f32::consts::{FRAC_PI_2, PI};

        /// `FarmhouseLDoor01`'s doorway as a quad: 96 by 176, facing the player on `+Z`.
        fn farmhouse_quad() -> Transform {
            Transform::from_xyz(0.0, 88.0, 0.0).with_scale(Vec3::new(96.0, 176.0, 1.0))
        }

        #[test]
        fn the_slab_stops_short_of_the_nearest_card_and_is_capped_without_one() {
            assert_eq!(
                doorway_slab([]),
                COMPOSITE_SLAB_CAP,
                "nothing behind the opening"
            );
            assert_eq!(doorway_slab([22.5, 30.0]), 22.5 - COMPOSITE_SLAB_MARGIN);
            assert_eq!(
                doorway_slab([100.0]),
                COMPOSITE_SLAB_CAP,
                "a far hit is capped"
            );
            assert_eq!(
                doorway_slab([1.0]),
                0.0,
                "a card in the plane: the plain quad"
            );
            assert_eq!(doorway_slab([f32::NAN, 14.0]), 14.0 - COMPOSITE_SLAB_MARGIN);
        }

        #[test]
        fn the_probe_rays_leave_the_plane_through_the_middle_of_the_opening_away_from_the_player() {
            let quad = farmhouse_quad();
            let rays = slab_probe_rays(&quad);
            assert_eq!(rays.len(), SLAB_PROBE_GRID * (SLAB_PROBE_GRID + 1));
            for ray in &rays {
                assert!(ray.origin.z.abs() < 1e-4, "from the doorway plane");
                assert!(
                    (*ray.direction - Vec3::NEG_Z).length() < 1e-5,
                    "away from the player"
                );
                assert!(ray.origin.x.abs() <= 48.0 * SLAB_PROBE_SPAN + 1e-3);
                assert!(ray.origin.y >= 176.0 * SILL_PROBE_HEIGHT - 1e-3);
                assert!(ray.origin.y <= 88.0 + 88.0 * SLAB_PROBE_SPAN + 1e-3);
            }
            // A turned doorway turns its rays with it.
            let turned = quad.with_rotation(Quat::from_rotation_y(FRAC_PI_2));
            for ray in slab_probe_rays(&turned) {
                assert!((*ray.direction - Vec3::NEG_X).length() < 1e-5);
            }
        }

        /// `FarmhouseLDoor01` in its own frame, with the doorway plane through the model box's
        /// centre (13.5 behind the front of the leaf's box): `DoorBlack` (-36..0, centred 4.5
        /// behind the plane) is filler; the leaf (-4..9) and anything in front of the plane is not.
        #[test]
        fn the_filler_is_what_stands_behind_the_doorway_plane() {
            let quad =
                Transform::from_xyz(0.0, 88.0, -13.5).with_scale(Vec3::new(96.0, 176.0, 1.0));
            assert!(
                is_doorway_filler(Vec3::new(0.0, 88.0, -18.0), &quad),
                "DoorBlack"
            );
            assert!(
                !is_doorway_filler(Vec3::new(0.0, 88.0, 2.5), &quad),
                "the leaf's box"
            );
            assert!(
                !is_doorway_filler(Vec3::new(0.0, 88.0, -13.6), &quad),
                "a frame straddling the plane"
            );
            // Turned half a turn, the player stands on the other side and so does the filler.
            let turned = quad.with_rotation(Quat::from_rotation_y(PI));
            assert!(is_doorway_filler(Vec3::new(0.0, 88.0, 2.5), &turned));
            assert!(!is_doorway_filler(Vec3::new(0.0, 88.0, -18.0), &turned));
        }

        /// The black card of `FarmhouseLDoor01`'s `DoorBlack`, 22.5 units behind the doorway plane
        /// and facing the player, measured the way [`measure_doorway_slab`] measures it.
        #[test]
        fn a_backing_card_behind_the_doorway_sets_the_slab() {
            let card = Plane3d::new(Vec3::Z, Vec2::new(48.0, 88.0)).mesh().build();
            let placed = Affine3A::from_translation(Vec3::new(0.0, 88.0, -22.5));
            let hits = slab_probe_rays(&farmhouse_quad())
                .into_iter()
                .filter_map(|ray| ray_hits_either_face(&card, &placed, ray))
                .collect::<Vec<_>>();
            assert_eq!(
                hits.len(),
                SLAB_PROBE_GRID * (SLAB_PROBE_GRID + 1),
                "every ray hits it"
            );
            let slab = doorway_slab(hits);
            assert!(
                (slab - (22.5 - COMPOSITE_SLAB_MARGIN)).abs() < 1e-3,
                "{slab}"
            );
            // The same card turned away from the player (a double-sided surface seen from its back)
            // stops the slab just the same.
            let turned = Affine3A::from_rotation_translation(
                Quat::from_rotation_y(PI),
                Vec3::new(0.0, 88.0, -22.5),
            );
            let hits = slab_probe_rays(&farmhouse_quad())
                .into_iter()
                .filter_map(|ray| ray_hits_either_face(&card, &turned, ray));
            let slab = doorway_slab(hits);
            assert!(
                (slab - (22.5 - COMPOSITE_SLAB_MARGIN)).abs() < 1e-3,
                "{slab}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        doors::DoorCrossed,
        player::{
            EYE_HEIGHT, Player, eye_from_feet, player_auto_doors, player_door, player_walk,
            player_walks_through_doors,
        },
        profiling::ProfilingState,
        render::{RendererMetrics, TerrainMaterial, WaterMaterial, WaterReflectionTexture},
        streaming::{
            StreamingMetrics, StreamingPlugin, creation_rotation_to_bevy, creation_to_bevy,
            render_position,
        },
        transition::{
            CrossDoor, OpenDoor, TransitionPlugin, arrival_frame, door_frame, door_is_open,
            door_to_arrival_rotation, portal_pose,
        },
        world::{
            cache::CellCache,
            database::{AssetCatalog, WorldDatabase},
        },
    };
    use bevy::{
        asset::AssetPlugin, camera::CameraProjection, camera::RenderTargetInfo,
        camera::visibility::VisibilityPlugin, transform::TransformPlugin,
    };
    use std::{collections::VecDeque, path::Path, path::PathBuf, time::Duration};

    const INTERIOR_ALFTAND01: u32 = 0x0001_52C3;
    const TAMRIEL: u32 = 60;
    /// An interior cell of the demo route that the test camera does not stand in: the pre-streamed
    /// cell the tests below give a role of its own.
    const PRESTREAMED_CELL: u32 = 0x0005_6C1B;

    /// A door like the Alftand02 -> AlftandWorld one of the demo route: an exterior destination.
    fn exterior_door() -> LoadDoor {
        LoadDoor {
            ref_id: 0x9256A,
            destination: DoorDestination {
                destination_ref_id: 0x699E8,
                interior_cell_id: None,
                worldspace_id: Some(0x0006_9857),
                arrival_position: [3693.815, 3074.645, 290.530],
                arrival_rotation: [0.0, 0.0, -1.83260],
            },
            label: "AlftandWorld".into(),
            auto_load: false,
            outward: None,
        }
    }

    /// A door like the Alftand01 -> Alftand02 one: an interior destination.
    fn interior_door(cell_id: u32) -> LoadDoor {
        LoadDoor {
            ref_id: 0x92809,
            destination: DoorDestination {
                destination_ref_id: 0x5704B,
                interior_cell_id: Some(cell_id),
                worldspace_id: None,
                arrival_position: [2879.831, 2_718.83, -1828.0],
                arrival_rotation: [0.0, 0.0, 2.87979],
            },
            label: "Alftand02".into(),
            auto_load: false,
            outward: None,
        }
    }

    /// Gives every root in `roles` the role in `roles`, and nothing else: what [`isolate_cells`]
    /// publishes for the frame, without the rest of the portal running.
    fn set_roles<const N: usize>(app: &mut App, roles: [(Entity, CellRole); N]) {
        let mut state = app.world_mut().resource_mut::<PortalState>();
        state.roles.clear();
        for (root, role) in roles {
            state.roles.insert(root, role);
        }
    }

    /// Whether the collider on `entity` is live.
    fn collider_enabled(app: &App, entity: Entity) -> bool {
        let entity = app.world().entity(entity);
        entity.contains::<Collider>() && !entity.contains::<ColliderDisabled>()
    }

    /// A collider is live only while its cell is part of the active space. The roles
    /// [`isolate_cells`] publishes are the single answer, and a subtree is flipped in the same
    /// update the role changed in - which is what makes a crossing's destination collidable in the
    /// frame the player arrives in it.
    #[test]
    fn the_space_gate_enables_only_the_active_cells_colliders() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<PortalState>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, gate_cell_colliders);
        let active = app.world_mut().spawn(Name::new("active")).id();
        let destination = app.world_mut().spawn(Name::new("destination")).id();
        let hidden = app.world_mut().spawn(Name::new("hidden")).id();
        let collider = |app: &mut App, root: Entity| {
            app.world_mut()
                .spawn((Collider::cuboid(1.0, 1.0, 1.0), ChildOf(root)))
                .id()
        };
        let active_collider = collider(&mut app, active);
        let destination_collider = collider(&mut app, destination);
        let hidden_collider = collider(&mut app, hidden);
        set_roles(
            &mut app,
            [
                (active, CellRole::Active),
                (destination, CellRole::Destination),
                (hidden, CellRole::Hidden),
            ],
        );

        app.update();

        assert!(
            collider_enabled(&app, active_collider),
            "the active space's collider is live"
        );
        assert!(
            !collider_enabled(&app, destination_collider),
            "a pre-streamed destination's collider must not block the player's space"
        );
        assert!(
            !collider_enabled(&app, hidden_collider),
            "a cell no camera draws is not walked into either"
        );

        // The crossing: the space the camera stands in changes, and the two swap in one update.
        set_roles(
            &mut app,
            [
                (active, CellRole::Destination),
                (destination, CellRole::Active),
                (hidden, CellRole::Hidden),
            ],
        );
        app.update();

        assert!(
            !collider_enabled(&app, active_collider),
            "the cell left behind is disabled at once"
        );
        assert!(
            collider_enabled(&app, destination_collider),
            "the cell arrived in is live in the same update"
        );
        assert!(!collider_enabled(&app, hidden_collider));
    }

    /// A collider that appears after its cell's role settled - a model the arming pacer hands to
    /// the spawner later, a destination whose scene finishes loading after it - takes the role the
    /// cell has *now*. A destination's stays disabled: readiness is not a reason to enable a
    /// collider (port-prep-collision, point 2), so nothing re-enables it on the frames after.
    #[test]
    fn a_collider_that_appears_later_takes_its_cells_settled_role() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<PortalState>()
            .init_resource::<ProfilingState>()
            .add_systems(Update, gate_cell_colliders);
        let active = app.world_mut().spawn(Name::new("active")).id();
        let destination = app.world_mut().spawn(Name::new("destination")).id();
        set_roles(
            &mut app,
            [
                (active, CellRole::Active),
                (destination, CellRole::Destination),
            ],
        );
        app.update();

        // What a completed readiness scan leaves: a collider under its reference, still disabled.
        let late_destination = app
            .world_mut()
            .spawn((
                Collider::cuboid(1.0, 1.0, 1.0),
                ColliderDisabled,
                ChildOf(destination),
            ))
            .id();
        let late_active = app
            .world_mut()
            .spawn((
                Collider::cuboid(1.0, 1.0, 1.0),
                ColliderDisabled,
                ChildOf(active),
            ))
            .id();

        app.update();

        assert!(
            !collider_enabled(&app, late_destination),
            "a destination's collider stays disabled after its readiness completes"
        );
        assert!(
            collider_enabled(&app, late_active),
            "an active cell's collider goes live as soon as the gate sees it"
        );

        app.update();
        assert!(
            !collider_enabled(&app, late_destination),
            "and nothing enables it on a later frame either"
        );
    }

    /// An app with the visibility and transform systems the doors and the isolation rely on.
    ///
    /// [`update_portal`] is not among them: it needs a door whose destination says it is resident,
    /// and residency lives in `StreamingWorld` behind a map private to `streaming`, so no app a
    /// test can build here would ever place a portal - it would return before picking anything and
    /// write `None` over the target every frame. The door tests set [`PortalState::open_door`]
    /// where it would, and [`portal_app_running_update_portal`] adds the real system back, in the
    /// plugin's own order, to check that it is `update_portal` that owns that field.
    fn portal_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::mesh::MeshPlugin,
            TransformPlugin,
            VisibilityPlugin,
        ))
        .insert_resource(EngineConfig {
            worldspace_id: TAMRIEL,
            stream_radius: 1,
            unload_radius: 1,
            start_grid: (0, 0),
            ..EngineConfig::default()
        })
        .insert_resource(ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(INTERIOR_ALFTAND01),
        })
        .insert_resource(RenderOrigin(IVec2::new(19, 18)))
        .init_resource::<PortalState>()
        .init_resource::<ProfilingState>()
        .add_systems(Update, (show_load_door_leaves, isolate_cells).chain());
        // The isolation's machinery, as `PortalPlugin` adds it: without the propagation of
        // `RenderLayers` the roots would carry a source nothing reads.
        add_cell_layer_isolation(&mut app);
        app
    }

    /// Adds the real [`update_portal`] to a [`portal_app`], ahead of the door system and in the
    /// order `PortalPlugin` registers them. Called between frames by the one test that needs the
    /// system that really owns [`PortalState::open_door`] to run and clear it.
    fn add_update_portal(app: &mut App) {
        app.add_systems(Update, update_portal.before(show_load_door_leaves));
    }

    fn spawn_camera(app: &mut App, position: Vec3) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_translation(position),
                GlobalTransform::from_translation(position),
                StreamingCamera,
            ))
            .id()
    }

    /// A cell root with one mesh of its own, the way `streaming::spawn_cell` builds one: the root
    /// carries the cell's identity and a reference child carries a mesh that the asset loader may
    /// add a frame later.
    fn spawn_cell(
        app: &mut App,
        cell_id: u32,
        grid: Option<IVec2>,
        key: Option<CellKey>,
        mesh_layers: Option<RenderLayers>,
    ) -> (Entity, Entity) {
        let mut root = app.world_mut().spawn((
            StreamedCellRoot,
            CellRef(cell_id),
            Transform::default(),
            Visibility::default(),
        ));
        if let Some(grid) = grid {
            root.insert(ExteriorCellGrid(grid));
        }
        if let Some(key) = key {
            root.insert(StreamedCellKey(key));
        }
        let root = root.id();
        let (mesh, _) = spawn_mesh(app, root, mesh_layers);
        (root, mesh)
    }

    fn spawn_mesh(app: &mut App, parent: Entity, layers: Option<RenderLayers>) -> (Entity, Entity) {
        let mut entity = app
            .world_mut()
            .spawn((Mesh3d(Handle::default()), ChildOf(parent)));
        if let Some(layers) = layers {
            entity.insert(layers);
        }
        let mesh = entity.id();
        (mesh, parent)
    }

    /// A load door of a cell, with one mesh under it: the reference root the asset loader hangs the
    /// door's glTF scene from, and a mesh the scene spawns a frame or more later.
    fn spawn_door(app: &mut App, door: LoadDoor) -> (Entity, Entity) {
        let entity = app
            .world_mut()
            .spawn((
                Transform::default(),
                GlobalTransform::default(),
                Visibility::default(),
                door,
            ))
            .id();
        let (mesh, _) = spawn_mesh(app, entity, None);
        (entity, mesh)
    }

    /// Whether the door draws its leaf, through the inheritance the renderer itself uses.
    fn leaf_is_drawn(app: &App, mesh: Entity) -> bool {
        app.world()
            .entity(mesh)
            .get::<InheritedVisibility>()
            .unwrap()
            .get()
    }

    fn visibility_of(app: &App, door: Entity) -> Visibility {
        *app.world().entity(door).get::<Visibility>().unwrap()
    }

    /// The door the portal is rendering through, written where [`update_portal`] writes it.
    ///
    /// A portal only picks a door whose destination says it is resident, and residency lives in
    /// `StreamingWorld` behind a private map that a test in this module cannot fill, so the target
    /// is set here directly. The tests below are about what the portal's choice *shows*; that a
    /// running portal makes this choice is what `--demo-tour` and the shots check at runtime.
    fn portal_shows(app: &mut App, door: Option<Entity>) {
        app.world_mut().resource_mut::<PortalState>().open_door = door;
    }

    fn layers_of(app: &App, entity: Entity) -> RenderLayers {
        app.world()
            .entity(entity)
            .get::<RenderLayers>()
            .cloned()
            .unwrap_or_default()
    }

    fn on_main_camera(app: &App, entity: Entity) -> bool {
        layers_of(app, entity).intersects(&RenderLayers::from_layers(&MAIN_CAMERA_LAYERS))
    }

    fn update(app: &mut App, times: usize) {
        for _ in 0..times {
            app.update();
        }
    }

    // -----------------------------------------------------------------------------------------
    // The fixture `update_portal` needs to place a camera at all: a door whose destination the
    // streamer has really loaded. Every other app this module builds leaves `StreamingWorld`
    // empty, and `update_portal` places nothing over a door whose destination is not resident
    // (`a_portal_that_stops_placing_its_camera_closes_its_door` is that case, on purpose).
    // -----------------------------------------------------------------------------------------

    /// The interior the fixture's door leads into.
    const FIXTURE_INTERIOR_CELL: u32 = 99;

    /// The world database the camera-pose test streams from: one Tamriel cell holding one load
    /// door, and the interior that door's `XTEL` leads into.
    ///
    /// The same two cells as `crate::transition`'s own fixture (`write_fixture`), copied here
    /// because that one is private to that module's tests: `StreamingWorld`'s map is private to
    /// `streaming`, so the only way a test in this module can have a *resident* destination - which
    /// is what `update_portal` selects a door by - is to let the real streamer load one. The door
    /// reference stands at creation 8200, -12200, 50 of the grid (2, -3) cell, which renders at
    /// (8, 50, -88) with the origin on that grid.
    fn write_door_fixture(directory: &Path) -> (PathBuf, PathBuf) {
        let database_path = directory.join("world.db");
        let connection = rusqlite::Connection::open(&database_path).unwrap();
        connection
            .execute_batch(&format!(
                r#"CREATE TABLE schema_info(version INTEGER NOT NULL);
                INSERT INTO schema_info VALUES({version});
                CREATE TABLE worldspaces(id INTEGER PRIMARY KEY,editor_id TEXT NOT NULL,parent_world INTEGER,flags INTEGER NOT NULL);
                CREATE TABLE cells(id INTEGER PRIMARY KEY,worldspace_id INTEGER,grid_x INTEGER,grid_y INTEGER,interior_name TEXT,flags INTEGER NOT NULL);
                CREATE INDEX idx_cells_grid ON cells(worldspace_id,grid_x,grid_y);
                CREATE TABLE land(cell_id INTEGER PRIMARY KEY,heightmap BLOB NOT NULL);
                CREATE TABLE statics(id INTEGER PRIMARY KEY,editor_id TEXT,model_path TEXT,flags INTEGER NOT NULL,
                    bounds_min_x REAL NOT NULL DEFAULT -64,bounds_min_y REAL NOT NULL DEFAULT -64,bounds_min_z REAL NOT NULL DEFAULT -64,
                    bounds_max_x REAL NOT NULL DEFAULT 64,bounds_max_y REAL NOT NULL DEFAULT 64,bounds_max_z REAL NOT NULL DEFAULT 64,
                    bounds_valid INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE "references"(id INTEGER PRIMARY KEY,cell_id INTEGER NOT NULL,worldspace_id INTEGER,base_form_id INTEGER NOT NULL,
                    is_exterior INTEGER NOT NULL,pos_x REAL NOT NULL,pos_y REAL NOT NULL,pos_z REAL NOT NULL,local_x REAL,local_y REAL,
                    rot_x REAL NOT NULL,rot_y REAL NOT NULL,rot_z REAL NOT NULL,scale REAL NOT NULL DEFAULT 1.0);
                CREATE INDEX idx_references_cell ON "references"(cell_id);
                CREATE VIRTUAL TABLE exterior_spatial USING rtree(id,minX,maxX,minY,maxY,minZ,maxZ,+cell_id,+worldspace_id);
                CREATE TABLE door_links(ref_id INTEGER PRIMARY KEY,destination_ref_id INTEGER NOT NULL,
                    pos_x REAL NOT NULL,pos_y REAL NOT NULL,pos_z REAL NOT NULL,
                    rot_x REAL NOT NULL,rot_y REAL NOT NULL,rot_z REAL NOT NULL,
                    destination_cell_id INTEGER,destination_worldspace_id INTEGER);
                CREATE TABLE texture_sets(id INTEGER PRIMARY KEY,editor_id TEXT,diffuse_path TEXT,normal_path TEXT,glow_path TEXT,
                    height_path TEXT,environment_path TEXT,mask_path TEXT,specular_path TEXT,detail_path TEXT);
                CREATE TABLE landscape_textures(id INTEGER PRIMARY KEY,editor_id TEXT,texture_set_id INTEGER,
                    material_type INTEGER,friction REAL,restitution REAL);
                CREATE TABLE waters(id INTEGER PRIMARY KEY,editor_id TEXT,opacity INTEGER,flags INTEGER NOT NULL,
                    shallow_color INTEGER,deep_color INTEGER,reflection_color INTEGER,flow_normal_path TEXT,data BLOB NOT NULL);

                INSERT INTO worldspaces VALUES(60,'Tamriel',0,0);
                INSERT INTO cells VALUES(10,60,2,-3,NULL,0);
                INSERT INTO cells VALUES(99,NULL,NULL,NULL,'Alftand01',0);
                INSERT INTO "references" VALUES(30,10,60,20,1,8200,-12200,50,8,88,0,0,0,1.0);
                INSERT INTO exterior_spatial VALUES(30,8200,8200,-12200,-12200,50,50,10,60);
                INSERT INTO "references" VALUES(31,99,NULL,21,0,-947.038,3958.835,591.917,NULL,NULL,0,0,0,1.0);
                INSERT INTO door_links VALUES(30,31,-947.038,3958.835,591.917,0,0,2.96989,99,NULL);"#,
                version = shared::WORLD_DATABASE_SCHEMA_VERSION
            ))
            .unwrap();
        drop(connection);

        let cache_path = directory.join("cell_cache.rkyv");
        let cache = shared::CellCache {
            version: shared::CELL_CACHE_VERSION,
            cells: Vec::new(),
        };
        std::fs::write(
            &cache_path,
            rkyv::to_bytes::<rkyv::rancor::Error>(&cache).unwrap(),
        )
        .unwrap();
        (database_path, cache_path)
    }

    /// The fixture in an app: the streamer, the crossing that pre-streams the door's destination,
    /// a main camera standing 200 units in front of the door, and the portal's own camera and
    /// quad - the entities `update_portal` writes.
    fn doorway_camera_app(directory: &Path) -> (App, Entity, Entity) {
        let (database_path, cache_path) = write_door_fixture(directory);
        let config = EngineConfig {
            worldspace_id: TAMRIEL,
            start_grid: (2, -3),
            stream_radius: 0,
            unload_radius: 1,
            ..EngineConfig::default()
        };
        let mut app = App::new();
        // `VisibilityPlugin` is the isolation's: it is what turns a `Visibility` into the
        // `InheritedVisibility` the leaf of a door is drawn by. `MeshPlugin` is beside it for the
        // skinned-mesh bounds asset the visibility systems read - the same pair `portal_app` adds,
        // and in the same order: `MeshPlugin` registers an asset, so it has to come after the
        // plugin that owns the `AssetServer`.
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::mesh::MeshPlugin,
            TransformPlugin,
            VisibilityPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        .init_asset::<TerrainMaterial>()
        .init_asset::<WaterMaterial>()
        // The terrain LOD readiness check (#105) reads converted scenes even when none is loaded.
        .init_asset::<bevy::world_serialization::WorldAsset>()
        .insert_resource(config)
        .insert_resource(RenderOrigin(IVec2::new(2, -3)))
        .insert_resource(ActiveCell {
            worldspace_id: TAMRIEL,
            interior: None,
        })
        .insert_resource(WorldDatabase::open(&database_path).unwrap())
        .insert_resource(AssetCatalog::open(&database_path).unwrap())
        .insert_resource(CellCache::open(&cache_path).unwrap())
        .insert_resource(WaterReflectionTexture(Handle::default()))
        .init_resource::<ProfilingState>()
        .init_resource::<PortalState>()
        .init_resource::<MoveCamera>()
        // The crossing layer pre-streams what a door leads into; without it nothing but the cell
        // the camera stands in would ever be loaded and no destination would become resident.
        .add_plugins((StreamingPlugin, TransitionPlugin))
        .add_systems(Update, (show_load_door_leaves, isolate_cells).chain());
        // The isolation's machinery, as `PortalPlugin` adds it: without the propagation of
        // `RenderLayers` the roots would carry a source nothing reads.
        add_cell_layer_isolation(&mut app);
        add_update_portal(&mut app);
        app.add_systems(Update, move_the_camera.before(update_portal));

        let eye = Vec3::new(8.0, 50.0, -288.0);
        let camera = app
            .world_mut()
            .spawn((
                Transform::from_translation(eye),
                GlobalTransform::from_translation(eye),
                Projection::Perspective(PerspectiveProjection::default()),
                StreamingCamera,
            ))
            .id();
        let portal_camera = app
            .world_mut()
            .spawn((
                PortalCamera,
                Transform::default(),
                Projection::Perspective(PerspectiveProjection::default()),
                Camera::default(),
            ))
            .id();
        app.world_mut()
            .spawn((PortalQuad, Transform::default(), Visibility::default()));
        (app, camera, portal_camera)
    }

    /// Where the player's hands put the camera this frame. A system rather than a write between
    /// frames, so the move is made where the player makes it: inside `Update`, before the portal's
    /// own systems, with `GlobalTransform` still holding the pose `PostUpdate` propagated.
    #[derive(Resource, Default)]
    struct MoveCamera(Option<Transform>);

    fn move_the_camera(
        mut move_to: ResMut<MoveCamera>,
        mut cameras: Query<&mut Transform, With<StreamingCamera>>,
    ) {
        let Some(pose) = move_to.0.take() else {
            return;
        };
        let Ok(mut camera) = cameras.single_mut() else {
            return;
        };
        *camera = pose;
    }

    /// Frames until the fixture's streamer has caught up, with what it was still waiting for in
    /// the failure: a world database answers on its own thread, so a fixture that never settles has
    /// to say so rather than hang.
    fn run_until(app: &mut App, what: &str, mut condition: impl FnMut(&App) -> bool) {
        for _ in 0..500 {
            if condition(app) {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
            app.update();
        }
        let metrics = app.world().resource::<StreamingMetrics>();
        panic!(
            "timed out waiting for {what}: requests={} responses={} failed={} resident={} loading={}",
            metrics.requests_submitted,
            metrics.responses_received,
            metrics.failed_cells,
            metrics.resident_cells,
            metrics.loading_cells,
        );
    }

    /// The single load door of the fixture app, once the streamer has spawned its cell.
    fn fixture_door(app: &mut App) -> Entity {
        let doors = |app: &App| -> Vec<Entity> {
            app.world()
                .iter_entities()
                .filter(|entity| entity.get::<LoadDoor>().is_some())
                .map(|entity| entity.id())
                .collect()
        };
        run_until(app, "the fixture's load door", |app| !doors(app).is_empty());
        let doors = doors(app);
        assert_eq!(doors.len(), 1, "the fixture has one load door");
        doors[0]
    }

    /// Where `update_portal` must place the portal camera for a main camera at `pose`: the door's
    /// own map, read out of the world exactly as the system reads it.
    fn door_map_pose(app: &App, door: Entity, pose: Transform) -> (Vec3, Quat) {
        let world = app.world();
        let global = *world.get::<GlobalTransform>(door).unwrap();
        let local = *world.get::<Transform>(door).unwrap();
        let row = world.get::<LoadDoor>(door).unwrap().clone();
        let anchor = world.get::<DoorAnchor>(door).cloned();
        let origin = world.resource::<RenderOrigin>().0;
        door_map(
            global.translation(),
            global.rotation(),
            local.scale,
            &row,
            anchor.as_ref(),
            origin,
        )
        .pose(pose.translation, pose.rotation)
    }

    #[test]
    fn a_camera_in_front_of_the_door_maps_behind_the_arrival_point_looking_along_it() {
        // The pose `translate::apply_door_crossings` produces: for the interior Alftand01, the
        // XTEL arrival in absolute creation coordinates with the arrival rotation.
        let door_rotation = creation_rotation_to_bevy([0.0, 0.0, 1.0]);
        let door_position = Vec3::new(500.0, 120.0, 200.0);
        let arrival_position = creation_to_bevy(Vec3::new(-947.038, 3958.835, 591.917));
        let arrival_rotation = creation_rotation_to_bevy([0.0, 0.0, 2.96989]);
        let front = door_rotation * Vec3::NEG_Z;
        // 150 units in front of the door, looking at it.
        let camera_position = door_position + front * 150.0;
        let camera_rotation = door_rotation * Quat::from_rotation_y(PI);

        let (position, rotation) = portal_pose(
            door_position,
            door_rotation,
            arrival_position,
            arrival_rotation,
            camera_position,
            camera_rotation,
        );

        let arrival_facing = arrival_rotation * Vec3::NEG_Z;
        assert!(
            (position - (arrival_position - arrival_facing * 150.0)).length() < 1.0e-3,
            "portal camera at {position:?}"
        );
        assert!(
            ((rotation * Vec3::NEG_Z) - arrival_facing).length() < 1.0e-5,
            "the portal camera faces {:?}, the arriving player faces {arrival_facing:?}",
            rotation * Vec3::NEG_Z
        );
    }

    #[test]
    fn a_camera_at_the_door_lands_exactly_where_the_crossing_puts_it() {
        let door_rotation = creation_rotation_to_bevy([0.0, 0.0, -1.83260]);
        let door_position = Vec3::new(-4419.67, 740.95, 1304.83);
        let origin = IVec2::new(5, 4);
        let (arrival_position, arrival_rotation) =
            arrival_frame(&exterior_door().destination, origin);
        // The same arrival a crossing computes for an exterior destination: relative to the origin
        // the destination is streamed at.
        assert_eq!(
            arrival_position,
            render_position(Vec3::new(3693.815, 3074.645, 290.530), origin)
        );

        let (position, _) = portal_pose(
            door_position,
            door_rotation,
            arrival_position,
            arrival_rotation,
            door_position,
            Quat::IDENTITY,
        );
        assert!(
            position.abs_diff_eq(arrival_position, 1.0e-4),
            "standing in the doorway the portal renders from the arrival point {arrival_position:?}, got {position:?}"
        );

        // The mapping is rigid: it moves poses without stretching the destination.
        let offset = Vec3::new(80.0, 40.0, -90.0);
        let (moved, _) = portal_pose(
            door_position,
            door_rotation,
            arrival_position,
            arrival_rotation,
            door_position + offset,
            Quat::IDENTITY,
        );
        assert!(
            (moved
                - (arrival_position
                    + door_to_arrival_rotation(door_rotation, arrival_rotation) * offset))
                .length()
                < 1.0e-3
        );
        assert!(((moved - arrival_position).length() - offset.length()).abs() < 1.0e-2);
    }

    #[test]
    fn the_doorways_clip_distance_is_the_distance_in_front_of_the_door() {
        let door_position = Vec3::new(10.0, 0.0, -20.0);
        let door_rotation = creation_rotation_to_bevy([0.0, 0.0, 0.7]);
        let arrival_position = Vec3::new(-400.0, 130.0, 900.0);
        let arrival_rotation = creation_rotation_to_bevy([0.0, 0.0, -1.2]);
        let camera_position = door_position + (door_rotation * Vec3::NEG_Z) * 220.0;
        let (portal_position, portal_rotation) = portal_pose(
            door_position,
            door_rotation,
            arrival_position,
            arrival_rotation,
            camera_position,
            Quat::IDENTITY,
        );
        let plane = doorway_clip_plane(
            portal_position,
            portal_rotation,
            arrival_position,
            arrival_rotation * Vec3::NEG_Z,
        );
        assert!(
            (-plane.w - 220.0).abs() < 1.0e-2,
            "the doorway is 220 units in front of the camera, got {}",
            -plane.w
        );

        // Standing behind the door the distance is negative, which is what keeps the portal off.
        let behind = door_position + (door_rotation * Vec3::NEG_Z) * -50.0;
        let (portal_position, portal_rotation) = portal_pose(
            door_position,
            door_rotation,
            arrival_position,
            arrival_rotation,
            behind,
            Quat::IDENTITY,
        );
        let plane = doorway_clip_plane(
            portal_position,
            portal_rotation,
            arrival_position,
            arrival_rotation * Vec3::NEG_Z,
        );
        assert!(
            -plane.w < 0.0,
            "behind the door the distance in front is negative, got {}",
            -plane.w
        );
    }

    /// The ruined tower door's shape: its model points one way, the link that leads back into it
    /// says the other, and the portal has to open the side the link data names. The door model's
    /// own frame faces west; a door that leads back into it puts its arrival 32 units east, which
    /// is the side the player comes from.
    #[test]
    fn a_door_is_opened_from_the_side_its_link_data_gives_not_its_model_axis() {
        let door_position = Vec3::new(500.0, 120.0, 200.0);
        // A model frame that faces west: `Y(90) * -Z` is runtime -X, which is Creation -x.
        let model = Quat::from_rotation_y(core::f32::consts::FRAC_PI_2);
        assert!((model * Vec3::NEG_Z).abs_diff_eq(Vec3::NEG_X, 1.0e-5));
        let outward = [1.0, 0.0, 0.0];
        let frame = door_frame(model, Some(outward));
        assert!(
            (frame * Vec3::NEG_Z).abs_diff_eq(Vec3::X, 1.0e-5),
            "the frame faces the door's outward direction, not the model's"
        );

        let camera_position = door_position + Vec3::X * 300.0;
        assert!(
            distance_in_front_of_door(door_position, frame, camera_position) > 299.0,
            "the camera east of the door stands in front of it"
        );
        assert!(
            distance_in_front_of_door(door_position, door_frame(model, None), camera_position)
                < 0.0,
            "on the model's own axis the same camera is behind the door, which is the side the \
             portal used to open"
        );

        // The portal picks the door for that camera, and would not with only the model's frame.
        let door = LoadDoor {
            outward: Some(outward),
            ..interior_door(INTERIOR_ALFTAND01)
        };
        let space = ActiveSpace {
            interior: Some(0x0005_6C1B),
            worldspace_id: TAMRIEL,
            center: IVec2::ZERO,
            radius: 1,
        };
        let portal_camera = Entity::from_raw_u32(1).unwrap();
        let open = DoorState::Open { animated: false };
        let candidates = [(portal_camera, door_position, &door, Some(&open), None)];
        let front = |door: &LoadDoor| {
            distance_in_front_of_door(
                door_position,
                door_frame(model, door.outward),
                camera_position,
            )
        };
        assert_eq!(
            select_portal_door(
                camera_position,
                candidates,
                |_, _| true,
                &space,
                |_| front(&door),
                |_| Some(0.0)
            ),
            Some(portal_camera)
        );
        let model_only = LoadDoor {
            outward: None,
            ..door.clone()
        };
        assert_eq!(
            select_portal_door(
                camera_position,
                [(portal_camera, door_position, &model_only, Some(&open), None)],
                |_, _| true,
                &space,
                |_| front(&model_only),
                |_| Some(0.0)
            ),
            None,
            "behind the door on its model's axis there is no window to look through"
        );

        // Standing in front and looking into the door - west, along the frame's `+Z` - the portal
        // camera looks along the arrival heading: the tower door's link arrives facing 92 degrees.
        let arrival_position = creation_to_bevy(Vec3::new(-947.038, 3958.835, 591.917));
        let arrival_rotation = creation_rotation_to_bevy([0.0, 0.0, 92.0_f32.to_radians()]);
        let (position, rotation) = portal_pose(
            door_position,
            frame,
            arrival_position,
            arrival_rotation,
            camera_position,
            frame * Quat::from_rotation_y(PI),
        );
        let arrival_facing = arrival_rotation * Vec3::NEG_Z;
        assert!(
            (position - (arrival_position - arrival_facing * 300.0)).length() < 1.0e-3,
            "300 units in front of the door maps to 300 behind the arrival point, got {position:?}"
        );
        assert!(
            (rotation * Vec3::NEG_Z).abs_diff_eq(arrival_facing, 1.0e-5),
            "looking into the door maps onto the arrival heading, got {:?}",
            rotation * Vec3::NEG_Z
        );
    }

    /// The number that decides which side of a door the camera is on is the number its portal
    /// camera's projection clips at: the door's plane is carried onto the arrival doorway by the
    /// same mapping the camera is. `select_portal_door` and `update_portal` read this one number,
    /// so a door the portal picks is never one its own camera is standing behind.
    #[test]
    fn the_front_distance_is_the_distance_the_portal_camera_clips_at() {
        let door_position = Vec3::new(-120.0, 460.0, 880.0);
        let arrival_position = Vec3::new(700.0, 30.0, -240.0);
        let arrival_rotation = creation_rotation_to_bevy([0.0, 0.0, -2.4]);
        for frame in [
            // From the link data, from the model, and from link data that turns the model a
            // quarter turn - the three frames the portal can be mapping through.
            door_frame(
                Quat::from_rotation_y(core::f32::consts::FRAC_PI_2),
                Some([1.0, 0.0, 0.0]),
            ),
            door_frame(Quat::from_rotation_y(core::f32::consts::FRAC_PI_2), None),
            door_frame(
                creation_rotation_to_bevy([0.0, 0.0, 0.7]),
                Some([0.0, -1.0, 0.0]),
            ),
        ] {
            let front = frame * Vec3::NEG_Z;
            for camera in [
                door_position + front * 220.0,
                door_position - front * 50.0,
                door_position + Vec3::new(30.0, 40.0, 60.0),
            ] {
                let (portal_position, portal_rotation) = portal_pose(
                    door_position,
                    frame,
                    arrival_position,
                    arrival_rotation,
                    camera,
                    Quat::IDENTITY,
                );
                let plane = doorway_clip_plane(
                    portal_position,
                    portal_rotation,
                    arrival_position,
                    arrival_rotation * Vec3::NEG_Z,
                );
                let distance = distance_in_front_of_door(door_position, frame, camera);
                assert!(
                    (-plane.w - distance).abs() < 1.0e-2,
                    "the door is {distance} in front, the clip plane says {}",
                    -plane.w
                );
            }
        }
    }

    /// A main-camera perspective projection and the same one for a portal camera looking through a
    /// doorway at `doorway_point` with `doorway_normal`, in the portal camera's own view space.
    fn portal_clip_case(doorway_point: Vec3, doorway_normal: Vec3, fov: f32) -> (Mat4, Mat4, f32) {
        let main = Projection::Perspective(PerspectiveProjection {
            fov,
            aspect_ratio: 1.0,
            near: 0.1,
            far: 100_000.0,
            near_clip_plane: Vec4::new(0.0, 0.0, -1.0, -0.1),
        });
        let plane = doorway_clip_plane(Vec3::ZERO, Quat::IDENTITY, doorway_point, doorway_normal);
        let Projection::Perspective(perspective) = &main else {
            unreachable!()
        };
        let plain = perspective.get_clip_from_view();
        let Projection::Perspective(perspective) = portal_projection(&main, plane, -plane.w) else {
            unreachable!()
        };
        assert_eq!(perspective.fov, fov);
        assert_eq!(perspective.aspect_ratio, 1.0);
        assert_eq!(perspective.near_clip_plane, plane);
        assert!(
            perspective.near >= -plane.w,
            "the near plane is at least the doorway distance"
        );
        (plain, perspective.get_clip_from_view(), -plane.w)
    }

    #[test]
    fn geometry_between_the_portal_camera_and_the_doorway_is_clipped() {
        let ndc = |matrix: Mat4, point: Vec3| {
            let clip = matrix * point.extend(1.0);
            clip.truncate() / clip.w
        };
        let clipped = |matrix: Mat4, point: Vec3| {
            let clip = matrix * point.extend(1.0);
            if clip.w <= 0.0 {
                return true;
            }
            !(0.0..=1.0).contains(&(clip.z / clip.w))
        };

        // A camera looking straight at a doorway. The oblique adjustment is skipped - its early-out
        // tests only the plane's normal, and a doorway seen square on has the view axis as its
        // normal - so the near plane, put at the same distance, is what clips.
        let (plain, square, distance) = portal_clip_case(
            Vec3::new(0.0, 0.0, -150.0),
            Vec3::NEG_Z,
            core::f32::consts::FRAC_PI_2,
        );
        assert!((distance - 150.0).abs() < 1.0e-3, "{distance}");
        let between = Vec3::new(0.0, 0.0, -50.0);
        assert!(
            clipped(square, between),
            "geometry between the portal camera and the doorway must be clipped: {:?}",
            ndc(square, between)
        );
        assert!(!clipped(plain, between), "without the doorway it is drawn");

        // A doorway seen at an angle, where the oblique plane itself does the cutting: this point
        // is inside the frustum's sides and only the doorway plane can reject it.
        let (_, angled, distance) = portal_clip_case(
            Vec3::new(0.0, 0.0, -150.0),
            Vec3::new(0.0, 0.5, -0.866_025_4),
            core::f32::consts::FRAC_PI_3,
        );
        assert!((distance - 129.9).abs() < 1.0e-1, "{distance}");
        let beside = Vec3::new(0.0, 30.0, -60.0);
        assert!(
            clipped(angled, beside),
            "a point on the camera's side of a tilted doorway is clipped: {:?}",
            ndc(angled, beside)
        );

        // Everything beyond the doorway stays inside the frustum, and lands on the same pixel it
        // did without the doorway: that is what makes the doorway line up with the main view.
        for point in [
            Vec3::new(0.0, 0.0, -151.0),
            Vec3::new(200.0, -140.0, -900.0),
            Vec3::new(-1500.0, 700.0, -4000.0),
        ] {
            let far = ndc(square, point);
            assert!(
                !clipped(square, point),
                "{point:?} is beyond the doorway but got depth {far:?}"
            );
            let plain_far = ndc(plain, point);
            assert!(
                (plain_far.x - far.x).abs() < 1.0e-5 && (plain_far.y - far.y).abs() < 1.0e-5,
                "{point:?} projects to {plain_far:?} without the doorway and {far:?} with it"
            );
        }
    }

    /// The window [`doorway_screen_rect`] is measured against: square, so the default
    /// projection's aspect ratio of 1 is the window's.
    const SCREEN: Vec2 = Vec2::new(1000.0, 1000.0);

    /// A doorway `size` wide and high, centred on `centre` and facing along `normal`, as
    /// `update_portal` lays the quad out, and the rectangle a camera at the origin looking down
    /// `-Z` sees it in.
    fn doorway_on_screen(centre: Vec3, normal: Vec3, size: Vec2) -> Option<Rect> {
        let quad = Transform::from_translation(centre)
            .with_rotation(Quat::from_rotation_arc(Vec3::Z, normal))
            .with_scale(size.extend(1.0));
        doorway_screen_rect(
            doorway_corners(&quad),
            &Transform::IDENTITY,
            &Projection::Perspective(PerspectiveProjection::default()),
            SCREEN,
        )
    }

    /// Where a point `x` to the side at depth `depth` lands on [`SCREEN`], in pixels from its left
    /// edge, under the default projection.
    fn screen_x(x: f32, depth: f32) -> f32 {
        let half_width = depth * (PerspectiveProjection::default().fov * 0.5).tan();
        (x / half_width + 1.0) * 0.5 * SCREEN.x
    }

    #[test]
    fn a_doorway_in_front_of_the_camera_covers_its_own_rectangle_of_the_screen() {
        let rect = doorway_on_screen(
            Vec3::new(0.0, 0.0, -500.0),
            Vec3::Z,
            Vec2::new(200.0, 300.0),
        )
        .expect("a doorway straight ahead is on screen");
        let expected_x = (screen_x(-100.0, 500.0), screen_x(100.0, 500.0));
        // `y` runs down the window, and the doorway is taller than it is wide.
        let expected_y = (
            SCREEN.y - screen_x(150.0, 500.0),
            SCREEN.y - screen_x(-150.0, 500.0),
        );
        assert!((rect.min.x - expected_x.0).abs() < 0.01, "{rect:?}");
        assert!((rect.max.x - expected_x.1).abs() < 0.01, "{rect:?}");
        assert!((rect.min.y - expected_y.0).abs() < 0.01, "{rect:?}");
        assert!((rect.max.y - expected_y.1).abs() < 0.01, "{rect:?}");
        assert!(
            rect.min.cmpgt(Vec2::ZERO).all() && rect.max.cmplt(SCREEN).all(),
            "the whole doorway is on screen, so nothing is clamped: {rect:?}"
        );
    }

    #[test]
    fn a_doorway_behind_the_camera_is_not_on_screen() {
        assert_eq!(
            doorway_on_screen(
                Vec3::new(0.0, 0.0, 500.0),
                Vec3::NEG_Z,
                Vec2::new(200.0, 300.0)
            ),
            None
        );
    }

    #[test]
    fn a_doorway_off_to_the_side_is_not_on_screen() {
        // In front of the camera, but far outside its field of view to the right.
        assert_eq!(
            doorway_on_screen(
                Vec3::new(2000.0, 0.0, -500.0),
                Vec3::Z,
                Vec2::new(200.0, 300.0)
            ),
            None
        );
    }

    #[test]
    fn a_doorway_partly_on_screen_is_clamped_to_the_window() {
        // Straddling the right edge of the view.
        let edge = 500.0 * (PerspectiveProjection::default().fov * 0.5).tan();
        let rect = doorway_on_screen(
            Vec3::new(edge, 0.0, -500.0),
            Vec3::Z,
            Vec2::new(200.0, 300.0),
        )
        .expect("half the doorway is on screen");
        assert!((rect.min.x - screen_x(edge - 100.0, 500.0)).abs() < 0.01);
        assert_eq!(rect.max.x, SCREEN.x, "clamped to the window's right edge");

        // A doorway the camera stands beside, with its far end ahead and its near end behind the
        // eye: only the part in front of the eye is on screen. Projected without the clip, the
        // corners behind the eye would land on the *left* of the screen and the rectangle would
        // cover the whole width.
        let rect = doorway_on_screen(
            Vec3::new(50.0, 0.0, -150.0),
            Vec3::X,
            Vec2::new(500.0, 300.0),
        )
        .expect("the part ahead of the eye is on screen");
        assert!(
            (rect.min.x - screen_x(50.0, 400.0)).abs() < 0.01,
            "the doorway starts where its far end is: {rect:?}"
        );
        assert_eq!(rect.max.x, SCREEN.x, "and runs off the right edge");
        assert_eq!((rect.min.y, rect.max.y), (0.0, SCREEN.y));
    }

    #[test]
    fn a_camera_in_the_doorways_plane_sees_no_doorway() {
        // Standing in the doorway and looking through it: every corner is level with the eye.
        assert_eq!(
            doorway_on_screen(Vec3::ZERO, Vec3::Z, Vec2::new(200.0, 300.0)),
            None
        );
        // Standing in the doorway's plane beside it and looking along the plane: edge on, a line
        // with no width. The corners are written out rather than rotated into place, so the plane
        // passes through the eye exactly.
        let edge_on = [
            Vec3::new(0.0, -150.0, -50.0),
            Vec3::new(0.0, -150.0, -550.0),
            Vec3::new(0.0, 150.0, -550.0),
            Vec3::new(0.0, 150.0, -50.0),
        ];
        assert_eq!(
            doorway_screen_rect(
                edge_on,
                &Transform::IDENTITY,
                &Projection::Perspective(PerspectiveProjection::default()),
                SCREEN,
            ),
            None
        );
    }

    /// The doorway's rectangle is rounded out to the quantum on every edge - never in, so every
    /// pixel of the doorway is rendered - and held to the window, whose own edges it keeps.
    #[test]
    fn a_doorway_rectangle_is_rounded_out_and_clamped_to_the_window() {
        let window = UVec2::new(1280, 720);
        let rect = |min: Vec2, max: Vec2| Rect { min, max };
        assert_eq!(
            doorway_render_rect(
                rect(Vec2::new(100.3, 70.2), Vec2::new(300.9, 500.1)),
                window
            ),
            Some(URect::new(64, 64, 320, 512)),
            "every edge rounds outwards to a multiple of 64"
        );
        assert_eq!(
            doorway_render_rect(
                rect(Vec2::new(64.0, 128.0), Vec2::new(192.0, 256.0)),
                window
            ),
            Some(URect::new(64, 128, 192, 256)),
            "a rectangle already on the grid is itself"
        );
        assert_eq!(
            doorway_render_rect(
                rect(Vec2::new(1200.5, 650.0), Vec2::new(1280.0, 720.0)),
                window
            ),
            Some(URect::new(1152, 640, 1280, 720)),
            "an edge at the window's edge stays there, not on the next multiple past it"
        );
        let full = UVec2::new(1920, 1080);
        assert_eq!(
            doorway_render_rect(rect(Vec2::ZERO, full.as_vec2()), full),
            Some(URect::new(0, 0, 1920, 1080)),
            "a doorway filling the screen is the whole window"
        );
        assert_eq!(
            doorway_render_rect(rect(Vec2::new(10.0, 10.0), Vec2::new(10.0, 40.0)), window),
            None,
            "a rectangle with no width renders nothing"
        );

        // A small move of the doorway is not a new size: the target is only reallocated when an
        // edge crosses a line of the grid.
        let before = doorway_render_rect(
            rect(Vec2::new(401.0, 99.0), Vec2::new(611.0, 530.0)),
            window,
        );
        let after = doorway_render_rect(
            rect(Vec2::new(405.0, 101.0), Vec2::new(615.0, 533.0)),
            window,
        );
        assert_eq!(before, after);

        // The target is the rectangle's size, and the ceiling still holds: a 4K doorway filling
        // the screen is drawn at the ceiling, one factor for both axes.
        let uhd = UVec2::new(3840, 2160);
        let whole = doorway_render_rect(rect(Vec2::ZERO, uhd.as_vec2()), uhd).unwrap();
        assert_eq!(
            portal_target_size(whole.size(), PORTAL_TEXTURE_MAX_SIZE),
            Some(PORTAL_TEXTURE_MAX_SIZE)
        );
        assert_eq!(
            portal_target_size(URect::new(64, 64, 320, 512).size(), PORTAL_TEXTURE_MAX_SIZE),
            Some(UVec2::new(256, 448)),
            "under the ceiling, one texel per window pixel"
        );
    }

    /// The sub-view is the rectangle of the main view, and a point lands on the same window pixel
    /// through it as through the main projection: the doorway image is the room behind it, pixel
    /// for pixel, with the doorway's oblique clip plane still applied.
    #[test]
    fn the_doorways_sub_view_lands_every_point_on_the_main_views_pixel() {
        let window = UVec2::new(1280, 720);
        let rect = URect::new(320, 128, 640, 512);
        let sub = doorway_sub_view(rect, window);
        assert_eq!(sub.full_size, window);
        assert_eq!(sub.offset, Vec2::new(320.0, 128.0));
        assert_eq!(sub.size, UVec2::new(320, 384));
        assert_eq!(
            doorway_rect_uniform(Some(rect)),
            Vec4::new(320.0, 128.0, 320.0, 384.0)
        );
        assert_eq!(doorway_rect_uniform(None), Vec4::ZERO);

        let main = PerspectiveProjection {
            aspect_ratio: window.x as f32 / window.y as f32,
            ..default()
        };
        // The portal camera's projection: the main one with an oblique doorway plane, 150 units
        // in front of the eye and tilted, as a doorway seen at an angle is.
        let clip_plane = Vec3::new(0.3, 0.1, -1.0).normalize().extend(-150.0);
        let Projection::Perspective(portal) =
            portal_projection(&Projection::Perspective(main.clone()), clip_plane, 150.0)
        else {
            panic!("a perspective projection stays one");
        };
        let full = portal.get_clip_from_view();
        let through_rect = portal.get_clip_from_view_for_sub(&sub);
        let pixel = |clip: Vec4, size: UVec2| {
            let ndc = clip.truncate().truncate() / clip.w;
            Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * size.as_vec2()
        };
        for point in [
            Vec3::new(-60.0, 40.0, -600.0),
            Vec3::new(-20.0, -10.0, -400.0),
            Vec3::new(-110.0, 90.0, -2000.0),
        ] {
            let on_window = pixel(full * point.extend(1.0), window);
            let in_rect = pixel(through_rect * point.extend(1.0), rect.size());
            assert!(
                (in_rect + rect.min.as_vec2() - on_window).length() < 1e-2,
                "{point}: window pixel {on_window}, rectangle pixel {in_rect}"
            );
        }
    }

    /// The culling frustum of the doorway view is the cone through its rectangle: a point the
    /// main view sees outside the doorway's rectangle is culled, one inside it is kept.
    #[test]
    fn the_doorway_views_frustum_is_the_cone_through_its_rectangle() {
        let window = UVec2::new(1280, 720);
        // The left half of the screen, top to bottom.
        let rect = URect::new(0, 0, 640, 720);
        let projection = Projection::Perspective(PerspectiveProjection {
            aspect_ratio: window.x as f32 / window.y as f32,
            ..default()
        });
        let transform = GlobalTransform::IDENTITY;
        let narrowed = sub_view_frustum(&projection, &doorway_sub_view(rect, window), &transform);
        let whole = projection.compute_frustum(&transform);
        let seen = |frustum: &Frustum, point: Vec3| {
            frustum.intersects_sphere(
                &bevy::camera::primitives::Sphere {
                    center: point.into(),
                    radius: 1.0,
                },
                true,
            )
        };
        let left = Vec3::new(-100.0, 0.0, -500.0);
        let right = Vec3::new(100.0, 0.0, -500.0);
        assert!(
            seen(&whole, left) && seen(&whole, right),
            "the main view sees both"
        );
        assert!(seen(&narrowed, left), "the doorway's half is kept");
        assert!(!seen(&narrowed, right), "the other half is culled");
    }

    /// With a doorway measured, the target is the doorway's rectangle and the quad maps through
    /// it; a rectangle that moves within the same size is a new mapping and not a new image.
    #[test]
    fn the_portal_target_is_the_doorways_rectangle_once_one_is_measured() {
        let (mut app, _, camera, quad, _) = resize_app(UVec2::new(1600, 900));
        app.insert_resource(PortalState {
            render_rect: Some(URect::new(64, 128, 448, 768)),
            ..default()
        });
        update(&mut app, 1);
        let target = target_image(&app);
        assert_eq!(target_size_of(&app, &target), UVec2::new(384, 640));
        assert_eq!(camera_target_of(&app, camera), target);
        assert_eq!(quad_texture_of(&app, quad), target);
        let rect_of = |app: &App| {
            let handle = app
                .world()
                .entity(quad)
                .get::<MeshMaterial3d<PortalMaterial>>()
                .unwrap();
            app.world()
                .resource::<Assets<PortalMaterial>>()
                .get(handle)
                .unwrap()
                .extension
                .doorway_rect
        };
        assert_eq!(rect_of(&app), Vec4::new(64.0, 128.0, 384.0, 640.0));

        app.world_mut().resource_mut::<PortalState>().render_rect =
            Some(URect::new(128, 128, 512, 768));
        let images = app.world().resource::<Assets<Image>>().len();
        update(&mut app, 1);
        assert_eq!(target_image(&app), target, "the same size keeps its image");
        assert!(
            app.world().resource::<Assets<Image>>().len() <= images,
            "and allocates none"
        );
        assert_eq!(rect_of(&app), Vec4::new(128.0, 128.0, 384.0, 640.0));
    }

    #[test]
    fn a_destination_is_the_interior_or_the_grid_around_an_exterior_arrival_point() {
        assert_eq!(
            destination_keys(&interior_door(INTERIOR_ALFTAND01).destination, None),
            vec![CellKey::Interior(INTERIOR_ALFTAND01)]
        );

        // The Blackreach door of the demo route arrives at 21088.559, 18512.045, 2434.0, grid 5,4.
        let keys = destination_keys(
            &LoadDoor {
                ref_id: 0x6998D,
                destination: DoorDestination {
                    destination_ref_id: 0x4E504,
                    interior_cell_id: None,
                    worldspace_id: Some(0x0001_EE62),
                    arrival_position: [21088.559, 18512.045, 2434.0],
                    arrival_rotation: [0.0, 0.0, -1.87080],
                },
                label: "Blackreach".into(),
                auto_load: false,
                outward: None,
            }
            .destination,
            None,
        );
        assert_eq!(keys.len(), 9);
        assert!(keys.contains(&CellKey::Exterior {
            worldspace_id: 0x0001_EE62,
            grid_x: 5,
            grid_y: 4,
        }));
        assert!(!keys.contains(&CellKey::Exterior {
            worldspace_id: 0x0001_EE62,
            grid_x: 3,
            grid_y: 3,
        }));
    }

    #[test]
    fn a_destination_is_only_ready_once_its_arrival_cell_is_resident() {
        let streaming = StreamingWorld::default();
        assert!(!destination_is_resident(
            &interior_door(INTERIOR_ALFTAND01).destination,
            None,
            &streaming
        ));
        assert!(!destination_is_resident(
            &exterior_door().destination,
            None,
            &streaming
        ));
    }

    #[test]
    fn the_portal_takes_the_nearest_door_whose_destination_is_ready() {
        let camera = Vec3::ZERO;
        let space = ActiveSpace {
            interior: Some(INTERIOR_ALFTAND01),
            worldspace_id: TAMRIEL,
            center: IVec2::ZERO,
            radius: 1,
        };
        let near = LoadDoor {
            ref_id: 1,
            ..interior_door(2)
        };
        let far = LoadDoor {
            ref_id: 2,
            ..interior_door(3)
        };
        let unready = LoadDoor {
            ref_id: 3,
            ..interior_door(4)
        };
        let outside = LoadDoor {
            ref_id: 4,
            destination: DoorDestination {
                interior_cell_id: None,
                worldspace_id: None,
                ..interior_door(5).destination
            },
            ..interior_door(5)
        };
        let near_door = Entity::from_raw_u32(1).unwrap();
        let far_door = Entity::from_raw_u32(2).unwrap();
        let unready_door = Entity::from_raw_u32(3).unwrap();
        let behind_door = Entity::from_raw_u32(4).unwrap();
        let beyond_door = Entity::from_raw_u32(5).unwrap();
        let open = DoorState::Open { animated: false };
        let doors = [
            (
                near_door,
                Vec3::new(0.0, 0.0, -300.0),
                &near,
                Some(&open),
                None,
            ),
            (
                far_door,
                Vec3::new(0.0, 0.0, -700.0),
                &far,
                Some(&open),
                None,
            ),
            (
                unready_door,
                Vec3::new(0.0, 0.0, -50.0),
                &unready,
                Some(&open),
                None,
            ),
            (
                behind_door,
                Vec3::new(0.0, 0.0, -10.0),
                &outside,
                Some(&open),
                None,
            ),
        ];
        let ready = |destination: &DoorDestination, _: Option<&DoorAnchor>| {
            destination.interior_cell_id != Some(4)
        };
        // The camera is behind the last door's plane, where the window has no content.
        let front = |entity: Entity| if entity == behind_door { -5.0 } else { 100.0 };
        // Every doorway equally well in view: distance is what is left to decide by.
        let on_screen = |_: Entity| Some(0.0);

        assert_eq!(
            select_portal_door(camera, doors, ready, &space, front, on_screen),
            Some(near_door),
            "the nearer door with a ready destination wins"
        );

        // A door beyond the pre-stream radius is not a candidate even when it is the only one.
        let far_only = [(
            beyond_door,
            Vec3::new(0.0, 0.0, -(DOOR_PRESTREAM_RADIUS + 1.0)),
            &near,
            Some(&open),
            None,
        )];
        assert_eq!(
            select_portal_door(camera, far_only, ready, &space, |_| 100.0, on_screen),
            None
        );

        // A door still swinging open, one whose clip has run out and left its leaves in the
        // doorway, and one swinging **shut**: the portal draws its window through all three.
        // `portal_shows_through` is the question it asks of a state, and on a closing door it
        // answers where `is_open` does not (see
        // `a_closing_door_keeps_its_window_and_still_is_not_open`).
        for state in [
            DoorState::Opening,
            DoorState::Open { animated: true },
            DoorState::Closing,
        ] {
            assert!(
                portal_shows_through(Some(&state)),
                "{state:?} is a doorway the portal draws through"
            );
            let mut with_a_swinging_near_door = doors;
            with_a_swinging_near_door[0].3 = Some(&state);
            assert_eq!(
                select_portal_door(
                    camera,
                    with_a_swinging_near_door,
                    ready,
                    &space,
                    front,
                    on_screen
                ),
                Some(near_door),
                "the nearest such doorway is looked through: {state:?}"
            );
        }

        // A closed door is not, however close and ready its destination is: the window stands in
        // the only opening a door has, so the destination image would be behind its leaf. A door
        // with no state at all counts as closed, like everywhere else.
        for state in [None, Some(&DoorState::Closed)] {
            assert!(
                !portal_shows_through(state),
                "the state under test is one the portal must not render through: {state:?}"
            );
            let mut with_a_closed_near_door = doors;
            with_a_closed_near_door[0].3 = state;
            assert_eq!(
                select_portal_door(
                    camera,
                    with_a_closed_near_door,
                    ready,
                    &space,
                    front,
                    on_screen
                ),
                Some(far_door),
                "the open door behind the closed one is the one to look through: {state:?}"
            );
        }
    }

    /// **The user's capture `01`** (research-193, fault 1): the Riverwood Trader has two load doors
    /// into one interior, the front door `0001341F` and the upper door `00070E69` 224 units above
    /// it, and both were open. The player stood at creation `(21914.93, -45562.18, 1.02)` looking
    /// into the front doorway (heading 142.85 degrees); the upper one is nearer - 179.8 units
    /// against 220.5 - and stands 52 degrees above the view, off the screen. The portal has to look
    /// through the doorway in view, not the nearest one.
    #[test]
    fn the_portal_looks_through_the_doorway_in_view_not_the_nearest_one() {
        let camera = creation_to_bevy(Vec3::new(21914.928, -45562.176, 1.017));
        let heading = 142.849_f32.to_radians();
        let forward = creation_to_bevy(Vec3::new(heading.sin(), heading.cos(), 0.0));
        let space = ActiveSpace {
            interior: None,
            worldspace_id: TAMRIEL,
            center: IVec2::new(5, -12),
            radius: 1,
        };
        let front_door = LoadDoor {
            ref_id: 0x0001_341F,
            ..interior_door(0x0001_33C9)
        };
        let upper_door = LoadDoor {
            ref_id: 0x0007_0E69,
            ..interior_door(0x0001_33C9)
        };
        let front_entity = Entity::from_raw_u32(1).unwrap();
        let upper_entity = Entity::from_raw_u32(2).unwrap();
        let front_position = creation_to_bevy(Vec3::new(22022.217, -45713.05, -118.811));
        let upper_position = creation_to_bevy(Vec3::new(21995.43, -45684.742, 105.017));
        assert!((front_position.distance(camera) - 220.5).abs() < 0.5);
        assert!((upper_position.distance(camera) - 179.8).abs() < 0.5);

        // The doorways' centres: `FarmhouseLDoor01`'s opening is 176 units tall on its origin.
        let doorway = |entity: Entity| {
            if entity == front_entity {
                front_position + Vec3::Y * 88.0
            } else {
                upper_position + Vec3::Y * 88.0
            }
        };
        let angle = |entity: Entity| forward.angle_between(doorway(entity) - camera);
        assert!(
            angle(upper_entity) > 50.0_f32.to_radians(),
            "the upper doorway is far above the view: {} degrees",
            angle(upper_entity).to_degrees()
        );
        assert!(
            angle(front_entity) < 15.0_f32.to_radians(),
            "the front doorway is ahead: {} degrees",
            angle(front_entity).to_degrees()
        );

        let open = DoorState::Open { animated: true };
        let doors = [
            (front_entity, front_position, &front_door, Some(&open), None),
            (upper_entity, upper_position, &upper_door, Some(&open), None),
        ];
        let ready = |_: &DoorDestination, _: Option<&DoorAnchor>| true;
        let in_front = |_: Entity| 100.0;

        // As the engine measures it: the upper doorway covers none of the screen.
        let on_screen = |entity: Entity| (entity == front_entity).then(|| angle(entity));
        assert_eq!(
            select_portal_door(camera, doors, ready, &space, in_front, on_screen),
            Some(front_entity),
            "the doorway in view is looked through, though the other door is nearer"
        );

        // Both doorways on screen (a wide view): the better aimed one still wins.
        assert_eq!(
            select_portal_door(camera, doors, ready, &space, in_front, |entity| Some(
                angle(entity)
            )),
            Some(front_entity)
        );

        // Neither doorway on screen: the nearest is kept, as before, so a doorway the player looks
        // away from keeps its window and comes back into view already drawn.
        assert_eq!(
            select_portal_door(camera, doors, ready, &space, in_front, |_| None),
            Some(upper_entity)
        );

        // Two doorways the view cannot tell apart - within the slack of one another - go to the
        // nearer door.
        let slack = PORTAL_AIM_SLACK_DEGREES.to_radians();
        assert_eq!(
            select_portal_door(camera, doors, ready, &space, in_front, |entity| Some(
                if entity == front_entity {
                    0.2
                } else {
                    0.2 + slack * 0.5
                }
            )),
            Some(upper_entity)
        );
    }

    /// The one state the portal and the crossing answer differently on: a door on its way shut.
    ///
    /// The player closing a door (`E`) and a door closing itself behind them (2026-09-25, both) play
    /// the `Close` clip, about 0.6 s, with the leaf drawn swinging back across the doorway
    /// (`crate::door_animation`). The window belongs behind that leaf for the whole swing - the door
    /// reaching `Closed` is what ends it - while no crossing may be walked through a door that is on
    /// its way shut. So the portal asks [`portal_shows_through`] and the crossing keeps asking
    /// [`DoorState::is_open`] through `door_is_open`, and this is the only state where the two
    /// differ: without it the doorway emptied itself under the closing leaf, which is what the user
    /// saw in play.
    #[test]
    fn a_closing_door_keeps_its_window_and_still_is_not_open() {
        assert!(
            portal_shows_through(Some(&DoorState::Closing)),
            "the portal keeps drawing the doorway the leaf is swinging shut across"
        );
        assert!(
            !door_is_open(Some(&DoorState::Closing)),
            "and the crossing may not be walked through it: `is_open` is the crossing's question"
        );
        for state in [DoorState::Opening, DoorState::Open { animated: true }] {
            assert!(
                portal_shows_through(Some(&state)),
                "{state:?} is a doorway the portal draws through"
            );
            assert_eq!(
                portal_shows_through(Some(&state)),
                door_is_open(Some(&state)),
                "the two agree wherever the door is on its way open: {state:?}"
            );
        }
        assert!(
            !portal_shows_through(Some(&DoorState::Closed))
                && !door_is_open(Some(&DoorState::Closed)),
            "a closed door is a closed door to both"
        );
        assert!(
            !portal_shows_through(None),
            "a door with no state at all counts as closed, like everywhere else"
        );
    }

    #[test]
    fn a_doorway_is_measured_from_the_doors_own_bounds() {
        // A door whose model measures 200 x 300, scaled to 100 wide and 150 tall by the reference.
        // The model-space box is the door's own frame, so no rotation of the reference moves it.
        let model = ExpectedModelBounds {
            min: Vec3::new(-100.0, 0.0, -10.0),
            max: Vec3::new(100.0, 150.0, 10.0),
        };
        let quarter_turn = Quat::from_rotation_y(core::f32::consts::FRAC_PI_2);
        let (size, centre) = portal_quad_extents(
            None,
            Some(&model),
            quarter_turn,
            quarter_turn,
            Vec3::new(0.5, 2.0, 1.0),
        );
        assert!(
            (size.x - 100.0).abs() < 1.0e-3 && (size.y - 300.0).abs() < 1.0e-3,
            "{size:?}"
        );
        assert!((centre.y - 150.0).abs() < 1.0e-3, "{centre:?}");

        // Where both are carried the model box wins: an axis-aligned world box of a turned door
        // would measure the door's diagonal.
        let (size, _) = portal_quad_extents(
            Some(&InstanceBounds {
                min: Vec3::splat(-500.0),
                max: Vec3::splat(500.0),
            }),
            Some(&model),
            quarter_turn,
            quarter_turn,
            Vec3::new(0.5, 2.0, 1.0),
        );
        assert!(
            (size.x - 100.0).abs() < 1.0e-3 && (size.y - 300.0).abs() < 1.0e-3,
            "{size:?}"
        );

        // A reference with only a placed world box: the box is turned back into the door's frame,
        // which measures a door on an axis exactly and over-estimates one at an angle.
        let bounds = InstanceBounds {
            min: Vec3::new(-20.0, 0.0, -100.0),
            max: Vec3::new(20.0, 300.0, 100.0),
        };
        let (size, centre) =
            portal_quad_extents(Some(&bounds), None, quarter_turn, quarter_turn, Vec3::ONE);
        assert!(
            (size.x - 200.0).abs() < 1.0e-3 && (size.y - 300.0).abs() < 1.0e-3,
            "the 200-unit axis lies across the world's z here: {size:?}"
        );
        assert!((centre.y - 150.0).abs() < 1.0e-3, "{centre:?}");
        let (size, _) = portal_quad_extents(
            Some(&bounds),
            None,
            Quat::IDENTITY,
            Quat::IDENTITY,
            Vec3::ONE,
        );
        assert!(
            (size.x - 40.0).abs() < 1.0e-3,
            "unrotated the box itself is the doorway: {size:?}"
        );

        // A marker with no converted bounds, and a degenerate one, get the default doorway.
        for (instance, expected) in [
            (None, None),
            (
                Some(InstanceBounds {
                    min: Vec3::ZERO,
                    max: Vec3::new(0.0, 1.0, 0.0),
                }),
                None,
            ),
        ] {
            let (size, centre) = portal_quad_extents(
                instance.as_ref(),
                expected,
                Quat::IDENTITY,
                Quat::IDENTITY,
                Vec3::ONE,
            );
            assert_eq!(size, DEFAULT_PORTAL_SIZE);
            assert!((centre.y - DEFAULT_PORTAL_SIZE.y * 0.5).abs() < 1.0e-3);
        }
    }

    /// The doorway quad is one thing in one frame. The doorway is the model's geometry and the side
    /// a player walks in from is the link's, and where the two disagree by a quarter turn - which
    /// [`LoadDoor::outward`] says a fifth of Skyrim.esm's load doors do - they are two different
    /// planes: a quad measured in the model's frame and turned in the front frame, or the other way
    /// round, stands edge-on to the opening the player walks in through.
    ///
    /// Laid out in the front frame, the window covers that opening, and it still stands on the point
    /// the crossing fires on - the doorway's own centre, which is what the portal and
    /// [`crate::player`]'s trigger have to agree on.
    #[test]
    fn the_doorway_quad_is_laid_out_in_the_frame_the_doors_front_comes_from() {
        // A model whose doorway is 200 wide, 305 tall and 64 deep, its opening 12 units off the
        // reference along the model's own `z` (the demo's dwemer doors hang theirs eight units off).
        let bounds = ExpectedModelBounds {
            min: Vec3::new(-100.0, -5.0, -20.0),
            max: Vec3::new(100.0, 300.0, 44.0),
        };
        let centre_of_the_box = Vec3::new(0.0, 147.5, 12.0);
        // The model's own front is Creation north, so the other three directions are a quarter, a
        // half and three quarters of a turn away from it.
        let model = Quat::IDENTITY;

        for (outward, size) in [
            // On the model's axis: the doorway as the model measures it.
            ([0.0, 1.0, 0.0], Vec2::new(200.0, 305.0)),
            // A quarter turn: the aperture a player walking along the frame's front sees is the
            // box's `x`/`z` side, 64 units wide - and a window still measured off the model's own
            // `x`/`y` face would be a 200-wide slab standing across the walk.
            ([1.0, 0.0, 0.0], Vec2::new(64.0, 305.0)),
            // Half a turn - the commonest disagreement of all, and the one the Ruined Tower door
            // has: the box's own face, turned round.
            ([0.0, -1.0, 0.0], Vec2::new(200.0, 305.0)),
            ([-1.0, 0.0, 0.0], Vec2::new(64.0, 305.0)),
        ] {
            let frame = door_frame(model, Some(outward));
            let (measured, centre) =
                portal_quad_extents(None, Some(&bounds), model, frame, Vec3::ONE);
            assert!(
                measured.abs_diff_eq(size, 1.0e-3),
                "outward {outward:?}: the doorway is {size:?}, measured {measured:?}"
            );
            assert!(
                (frame * centre).abs_diff_eq(model * centre_of_the_box, 1.0e-3),
                "outward {outward:?}: the quad stands at {:?}, the doorway is at {:?} - and that \
                 point is the one the crossing's plane passes through",
                frame * centre,
                model * centre_of_the_box
            );
        }
    }

    /// A door draws its own leaf - closed - until the portal renders through it, and the portal
    /// renders through exactly one door at a time.
    ///
    /// The target is written where [`update_portal`] writes it (see [`portal_shows`]): *which* door
    /// a running portal picks is a question about the door states, and
    /// `the_portal_takes_the_nearest_door_whose_destination_is_ready` asks that one. This is what
    /// the choice shows.
    #[test]
    fn the_portal_opens_one_door_and_closes_it_again() {
        let mut app = portal_app();
        let (a, a_mesh) = spawn_door(&mut app, interior_door(0x0005_6C1B));
        let (b, b_mesh) = spawn_door(&mut app, interior_door(0x0005_6C1C));

        update(&mut app, 1);
        assert!(
            leaf_is_drawn(&app, a_mesh) && leaf_is_drawn(&app, b_mesh),
            "with no portal up, every doorway is a closed door"
        );

        // A is the door the portal picked: its leaf is gone in the frame the portal takes it, so
        // there is no frame in which the doorway is a hole with a leaf in it or a leaf with a hole.
        portal_shows(&mut app, Some(a));
        update(&mut app, 1);
        assert_eq!(visibility_of(&app, a), Visibility::Hidden);
        assert!(!leaf_is_drawn(&app, a_mesh), "A's doorway is an opening");
        assert!(
            leaf_is_drawn(&app, b_mesh),
            "a door the portal does not render through keeps its leaf"
        );

        // Retargeting to B closes A and opens B in that same frame.
        portal_shows(&mut app, Some(b));
        update(&mut app, 1);
        assert!(!leaf_is_drawn(&app, b_mesh));
        assert!(
            leaf_is_drawn(&app, a_mesh),
            "A is closed again in the frame the portal leaves it"
        );

        // Turning the portal off closes the last door too.
        portal_shows(&mut app, None);
        update(&mut app, 1);
        assert!(
            leaf_is_drawn(&app, a_mesh) && leaf_is_drawn(&app, b_mesh),
            "no portal, no open doorway"
        );
    }

    /// An auto-load door is an invisible marker, not a leaf: nothing the portal does draws it.
    #[test]
    fn an_auto_load_marker_never_draws() {
        let mut app = portal_app();
        let mut marker = interior_door(0x0005_6C1B);
        marker.auto_load = true;
        let (marker, marker_mesh) = spawn_door(&mut app, marker);
        let (door, door_mesh) = spawn_door(&mut app, interior_door(0x0005_6C1C));

        // Even as the door the portal is showing, where a real door's leaf would be hidden - and
        // the real door beside it, which the portal is not showing, keeps its leaf.
        portal_shows(&mut app, Some(marker));
        update(&mut app, 1);
        assert_eq!(visibility_of(&app, marker), Visibility::Hidden);
        assert!(!leaf_is_drawn(&app, marker_mesh));
        assert!(leaf_is_drawn(&app, door_mesh));

        // And with the portal on a real door - the player has opened it, which is the only kind a
        // running portal renders through - only that door is an opening.
        app.world_mut()
            .entity_mut(door)
            .insert(DoorState::Open { animated: false });
        portal_shows(&mut app, Some(door));
        update(&mut app, 1);
        assert!(!leaf_is_drawn(&app, door_mesh));
        assert!(!leaf_is_drawn(&app, marker_mesh));
        assert_eq!(visibility_of(&app, marker), Visibility::Hidden);
    }

    /// The door the portal showed is closed again as soon as no portal is rendering, whichever way
    /// it stops: the door here is a closed one - the target is written directly, and a running
    /// portal only ever picks a door it draws a window through ([`portal_shows_through`]) - so its
    /// leaf has to be back the frame the portal lets it go. This runs the real [`update_portal`] -
    /// here one that cannot place a camera at all,
    /// since the app has no `StreamingWorld` and so no resident destination - so it is also the
    /// test that the target a running portal has to publish is the one the door system reads.
    #[test]
    fn a_portal_that_stops_placing_its_camera_closes_its_door() {
        let mut app = portal_app();
        let (door, mesh) = spawn_door(&mut app, interior_door(0x0005_6C1B));
        portal_shows(&mut app, Some(door));
        update(&mut app, 1);
        assert!(
            !leaf_is_drawn(&app, mesh),
            "the portal is rendering through it"
        );

        add_update_portal(&mut app);
        update(&mut app, 1);
        assert_eq!(
            app.world().resource::<PortalState>().open_door,
            None,
            "a portal that cannot place its camera renders through no door"
        );
        assert!(
            leaf_is_drawn(&app, mesh),
            "so the door draws its leaf again, in that same frame"
        );
    }

    /// The doorway image is drawn from where the camera is *this* frame, not where it was last
    /// frame.
    ///
    /// `update_portal` read the main camera's `GlobalTransform`, which `TransformPlugin` only
    /// propagates in `PostUpdate`, so the portal camera stood where the main camera had been a
    /// frame earlier: every move and turn trailed one frame behind in the window (the user, in
    /// play, 2026-09-24; fixed in `63af9d3`). The main camera is a root entity, so its `Transform`
    /// is its world pose.
    ///
    /// Nothing else in this module can see that: no other test moves the main camera between
    /// updates, and `spawn_camera` gives it a `Transform` and a `GlobalTransform` with the *same*
    /// value - the one case where reading either one gives the same answer. Here the camera is
    /// moved where the player moves it (in `Update`, before the portal's systems) and the portal
    /// camera is read back against the door's own map.
    #[test]
    fn the_portal_camera_is_placed_from_this_frames_camera_pose() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, camera, portal_camera) = doorway_camera_app(directory.path());
        // A portal places nothing over a door whose destination is not there: the fixture's door
        // pre-streams the interior it leads into, which the streamer loads on its own thread.
        run_until(&mut app, "the door's destination to be resident", |app| {
            app.world()
                .resource::<StreamingWorld>()
                .is_resident(&CellKey::Interior(FIXTURE_INTERIOR_CELL))
        });
        let door = fixture_door(&mut app);
        app.world_mut()
            .entity_mut(door)
            .insert(DoorState::Open { animated: false });
        update(&mut app, 2);

        // The camera is where the test stood it up, so the portal is up and its camera is placed
        // from that pose: what follows is about the pose alone, not about a portal that never ran.
        let standing = *app.world().entity(camera).get::<Transform>().unwrap();
        let portal = *app
            .world()
            .entity(portal_camera)
            .get::<Transform>()
            .unwrap();
        let (placed, placed_rotation) = door_map_pose(&app, door, standing);
        assert_eq!(
            app.world().resource::<PortalState>().open_door,
            Some(door),
            "the portal is rendering through the fixture's door"
        );
        assert!(
            (portal.translation - placed).length() < 1.0e-3
                && portal.rotation.abs_diff_eq(placed_rotation, 1.0e-5),
            "the portal camera stands at {:?} looking {:?}; the camera it follows is at {standing:?}, \
             which the door's map takes to {placed:?}",
            portal.translation,
            portal.rotation
        );

        // The player walks and turns. The camera moves in this frame's `Update`, and its
        // `GlobalTransform` is still the pose the last `PostUpdate` propagated.
        let moved = Transform::from_translation(Vec3::new(8.0, 60.0, -250.0))
            .with_rotation(Quat::from_rotation_y(0.3));
        app.world_mut().insert_resource(MoveCamera(Some(moved)));
        update(&mut app, 1);

        let portal = *app
            .world()
            .entity(portal_camera)
            .get::<Transform>()
            .unwrap();
        let (want_position, want_rotation) = door_map_pose(&app, door, moved);
        assert!(
            (portal.translation - want_position).length() < 1.0e-3
                && portal.rotation.abs_diff_eq(want_rotation, 1.0e-5),
            "the portal camera stands at {:?} looking {:?}: the camera moved to {moved:?} this \
             frame, and the doorway image is drawn from the pose a frame behind it instead",
            portal.translation,
            portal.rotation
        );
    }

    /// The quad stands in the doorway's own plane, and that plane is the one the player's crossing
    /// fires on: the window is drawn exactly up to the frame of the swap, and no frame after it.
    ///
    /// This is the one invariant between the portal and the crossing that the swap depends on. The
    /// portal stands the quad on the doorway box's own centre (`measured_doorway_box`, laid out by
    /// [`doorway_in_frame`]) and the crossing builds its plane from the doorway volume
    /// `auto_door_trigger` builds out of the same box; they have to name the same point.
    #[test]
    fn the_window_stands_in_the_plane_the_crossing_fires_on() {
        let door_rotation = creation_rotation_to_bevy([0.0, 0.0, 0.7]);
        let position = Vec3::new(-400.0, 260.0, 900.0);
        let scale = Vec3::new(0.5, 2.0, 1.0);
        let bounds = ExpectedModelBounds {
            min: Vec3::new(-100.0, -5.0, -20.0),
            max: Vec3::new(100.0, 300.0, 44.0),
        };
        let (_, centre) =
            portal_quad_extents(None, Some(&bounds), door_rotation, door_rotation, scale);
        let window = position + door_rotation * centre;
        let trigger =
            crate::player::auto_door_trigger(position, door_rotation, scale, None, Some(&bounds));
        assert!(
            trigger.centre().abs_diff_eq(window, 1.0e-3),
            "the doorway volume is centred on {:?}, the window stands on {:?}",
            trigger.centre(),
            window
        );
        assert_eq!(
            PORTAL_QUAD_OFFSET, 0.0,
            "the window does not stand off the plane"
        );
    }

    /// A door whose anchor stands on a threshold, raised to a floor 45 units above its box's
    /// bottom, and the same door anchored on its box centre (the centre rule) and with no anchor.
    fn threshold_door() -> (Vec3, Quat, Vec3, ExpectedModelBounds, DoorAnchor) {
        let bounds = ExpectedModelBounds {
            min: Vec3::new(-60.0, -5.0, -20.0),
            max: Vec3::new(60.0, 300.0, 44.0),
        };
        let box_centre = (bounds.min + bounds.max) * 0.5;
        let anchor = DoorAnchor {
            tier: crate::doors::DoorAnchorTier::SameModel,
            source_box_centre: box_centre.to_array(),
            source_anchor_height: 40.0,
            destination: crate::doors::DoorwayGeometry {
                position: [0.0; 3],
                rotation: [0.0; 3],
                scale: 1.0,
                box_centre: box_centre.to_array(),
                anchor_height: 40.0,
            },
            destination_grid: None,
            facings: crate::doors::DoorwayFacings::Kept,
        };
        (
            Vec3::new(-400.0, 260.0, 900.0),
            creation_rotation_to_bevy([0.0, 0.0, 0.7]),
            Vec3::ONE,
            bounds,
            anchor,
        )
    }

    /// The bottom and top edges of a doorway quad, in world height.
    fn quad_bottom_and_top(quad: &Transform) -> (f32, f32) {
        (
            quad.translation.y - quad.scale.y * 0.5,
            quad.translation.y + quad.scale.y * 0.5,
        )
    }

    /// Gerdur's House (impl-206): a door set into the ground has its anchor, and so the portal
    /// camera's pivot, on the floor 45 units above its box's bottom. The quad's bottom edge is that
    /// threshold - the point `source_doorway_centre` places - and not the box's bottom, which would
    /// show the destination from under its floor; the top, sides and plane are the box's.
    #[test]
    fn an_anchored_quad_stands_on_the_threshold_the_camera_is_anchored_on() {
        let (position, rotation, scale, bounds, anchor) = threshold_door();
        let frame = rotation;
        let boxed = doorway_quad_transform(
            position,
            rotation,
            frame,
            scale,
            None,
            Some(&bounds),
            None,
            None,
        );
        let anchored = doorway_quad_transform(
            position,
            rotation,
            frame,
            scale,
            None,
            Some(&bounds),
            Some(&anchor),
            None,
        );
        let pivot = crate::transition::source_doorway_centre(position, rotation, scale, &anchor);
        let (bottom, top) = quad_bottom_and_top(&anchored);
        let (box_bottom, box_top) = quad_bottom_and_top(&boxed);
        assert!(
            (bottom - pivot.y).abs() < 1.0e-3,
            "bottom {bottom}, pivot {}",
            pivot.y
        );
        assert!((bottom - (position.y + 40.0)).abs() < 1.0e-3);
        assert!((box_bottom - (position.y - 5.0)).abs() < 1.0e-3);
        assert!((top - box_top).abs() < 1.0e-3, "the top stays the box's");
        assert_eq!(anchored.scale.x, boxed.scale.x, "the width stays the box's");
        assert_eq!(anchored.rotation, boxed.rotation);
        let depth = |quad: &Transform| (quad.translation - position).dot(frame * Vec3::NEG_Z);
        assert!(
            (depth(&anchored) - depth(&boxed)).abs() < 1.0e-3,
            "same plane"
        );
        let plan = |v: Vec3| (frame.inverse() * (v - position)).x;
        assert!((plan(anchored.translation) - plan(boxed.translation)).abs() < 1.0e-3);
    }

    /// **impl-218, Gerdur's House** (door `00013423` onto `00013407`, one `FarmhouseLDoor01`): on
    /// the database's thresholds the quad's bottom and the map's pivot stood 13.45 units above the
    /// door, on the `XTEL` arrival, while the porch is 0.30 above it - the room's floor stood 13.15
    /// units over the porch in the doorway, with the doorway's own void black under its edge. Once
    /// the anchor is on the measured floors, the quad's bottom is the porch, and a point on the porch
    /// just outside the doorway is carried onto the room's floor just inside it: one floor line.
    #[test]
    fn gerdurs_doorway_quad_and_map_stand_on_the_measured_floors() {
        let bounds = ExpectedModelBounds {
            min: Vec3::new(-48.0, 0.0, -36.0),
            max: Vec3::new(48.0, 176.0, 9.0),
        };
        let box_centre = (bounds.min + bounds.max) * 0.5;
        let database = DoorAnchor {
            tier: crate::doors::DoorAnchorTier::SameModel,
            source_box_centre: box_centre.to_array(),
            source_anchor_height: 13.445_751,
            destination: crate::doors::DoorwayGeometry {
                position: [-511.666, -292.434_66, 0.0],
                rotation: [0.0, 0.0, PI],
                scale: 1.0,
                box_centre: box_centre.to_array(),
                anchor_height: 0.0,
            },
            destination_grid: None,
            facings: crate::doors::DoorwayFacings::SameModel {
                turn: PI - 2.356_194_5,
            },
        };
        let (porch, room) = (0.298_178, -2.5e-5);
        let measured = database.on_measured_floors(porch, room).unwrap();
        // The outside door, as the streamer places it (render origin at its own cell).
        let position = Vec3::new(1234.0, -18.646_841, -567.0);
        let rotation = creation_rotation_to_bevy([0.0, 0.0, 2.356_194_5]);
        let frame = rotation;
        let bottom = |anchor: &DoorAnchor| {
            let quad = doorway_quad_transform(
                position,
                rotation,
                frame,
                Vec3::ONE,
                None,
                Some(&bounds),
                Some(anchor),
                None,
            );
            quad_bottom_and_top(&quad).0 - position.y
        };
        assert!((bottom(&database) - 13.445_751).abs() < 1.0e-3);
        assert!(
            (bottom(&measured) - porch).abs() < 1.0e-3,
            "{}",
            bottom(&measured)
        );
        // The map, as `door_map` builds it under an anchor: pivot and arrival are the two doorways'
        // anchor points, turned by the doorways' own frames.
        let pivot =
            crate::transition::source_doorway_centre(position, rotation, Vec3::ONE, &measured);
        let arrival = crate::transition::destination_doorway_centre(&measured, true, IVec2::ZERO);
        let arrival_rotation = frame * Quat::from_rotation_y(-(PI - 2.356_194_5));
        let front = frame * Vec3::NEG_Z;
        let on_porch = Vec3::new(pivot.x, position.y + porch, pivot.z) + front * 8.0;
        let behind = on_porch - front * 16.0;
        let (landed, _) = portal_pose(
            pivot,
            frame,
            arrival,
            arrival_rotation,
            behind,
            Quat::IDENTITY,
        );
        assert!(
            (landed.y - room).abs() < 1.0e-3,
            "the porch is carried onto the room's floor: {}",
            landed.y
        );
    }

    /// A door model that carries a frame much wider than its leaves has a window the width of its
    /// leaves ([`narrowed_to_leaves`]); a door whose leaf fills its model keeps the box's window.
    #[test]
    fn the_window_of_a_door_whose_model_is_wider_than_its_leaves_is_its_leaves() {
        // Whiterun's main gate, as `WRDoorMainGate01`'s bounds give it: a 643-unit housing round
        // two leaves 424 units across, both centred on the model.
        let size = Vec2::new(643.7, 374.4);
        let centre = Vec3::new(0.0, 101.0, -10.5);
        let leaves = (Vec2::new(425.4, 373.0), Vec3::new(-0.2, 101.6, -1.9));
        let (narrowed, narrowed_centre) = narrowed_to_leaves(size, centre, Some(leaves));
        assert!(
            (narrowed.x - 425.4).abs() < 1.0e-3,
            "the window is the leaves' width: {narrowed}"
        );
        assert_eq!(narrowed.y, size.y, "and the box's height");
        assert!((narrowed_centre.x - leaves.1.x).abs() < 1.0e-3);
        assert_eq!(
            (narrowed_centre.y, narrowed_centre.z),
            (centre.y, centre.z),
            "the box's height and depth are kept"
        );

        // A house door, whose leaf fills its model: nothing changes.
        let size = Vec2::new(100.0, 180.0);
        let leaf = (Vec2::new(92.0, 176.0), Vec3::new(1.0, 88.0, 0.0));
        assert_eq!(narrowed_to_leaves(size, centre, Some(leaf)), (size, centre));
        // No leaves measured, or leaves reaching past the box: the box, or the part of it they
        // span, and never wider than the box.
        assert_eq!(narrowed_to_leaves(size, centre, None), (size, centre));
        let (offset, offset_centre) = narrowed_to_leaves(
            Vec2::new(600.0, 300.0),
            Vec3::ZERO,
            Some((Vec2::new(400.0, 300.0), Vec3::new(200.0, 0.0, 0.0))),
        );
        assert!((offset.x - 300.0).abs() < 1.0e-3 && (offset_centre.x - 150.0).abs() < 1.0e-3);
    }

    /// A door with no anchor, and one anchored on its box centre (the centre rule, which has no
    /// threshold to give), keep today's quad exactly: the box's.
    #[test]
    fn an_unanchored_or_centre_anchored_quad_keeps_the_box() {
        let (position, rotation, scale, bounds, mut anchor) = threshold_door();
        let boxed = doorway_quad_transform(
            position,
            rotation,
            rotation,
            scale,
            None,
            Some(&bounds),
            None,
            None,
        );
        let (size, centre) = portal_quad_extents(None, Some(&bounds), rotation, rotation, scale);
        assert_eq!(boxed.scale, Vec3::new(size.x, size.y, 1.0));
        assert!(
            boxed
                .translation
                .abs_diff_eq(position + rotation * centre, 1.0e-3)
        );
        anchor.source_anchor_height = (rotation * Vec3::from_array(anchor.source_box_centre)).y;
        let centred = doorway_quad_transform(
            position,
            rotation,
            rotation,
            scale,
            None,
            Some(&bounds),
            Some(&anchor),
            None,
        );
        assert_eq!(centred, boxed);
    }

    /// A door the player has opened keeps its leaf hidden as they walk into it, even where the
    /// portal has stopped rendering - half a unit in front of the plane, inside
    /// [`MIN_PORTAL_DOOR_DISTANCE`] - and a door nobody opened still draws its own leaf (design
    /// section 4.7).
    #[test]
    fn an_opened_door_stays_open_with_the_camera_in_the_doorway() {
        let mut app = portal_app();
        let (door, mesh) = spawn_door(&mut app, interior_door(0x0005_6C1B));
        let (closed, closed_mesh) = spawn_door(&mut app, interior_door(0x0005_6C1C));
        // The player has walked up to the doorway and through it: half a unit in front of its
        // plane, which is where the portal gives up and clears the door it was showing.
        let camera = spawn_camera(&mut app, Vec3::new(0.0, 0.0, -0.5));
        add_update_portal(&mut app);
        app.world_mut()
            .entity_mut(door)
            .insert(DoorState::Open { animated: false });
        update(&mut app, 1);

        let camera_position = app
            .world()
            .entity(camera)
            .get::<GlobalTransform>()
            .unwrap()
            .translation();
        assert!(
            distance_in_front_of_door(Vec3::ZERO, Quat::IDENTITY, camera_position)
                < MIN_PORTAL_DOOR_DISTANCE,
            "the camera is inside the gap the portal does not render through"
        );
        assert_eq!(
            app.world().resource::<PortalState>().open_door,
            None,
            "so no portal is up for this door"
        );
        assert!(
            !leaf_is_drawn(&app, mesh),
            "the opened door's leaf is gone anyway: the doorway is the way through it"
        );
        assert!(
            leaf_is_drawn(&app, closed_mesh),
            "a door nobody opened draws its own leaf"
        );
        assert_eq!(visibility_of(&app, closed), Visibility::Inherited);
    }

    /// A door whose crossing is waiting for its destination to stream in draws its own model: the
    /// portal has no window to show through the doorway while it waits - the destination is exactly
    /// what is missing - so a door hidden here would be a hole in the world where the doorway is.
    #[test]
    fn a_door_holding_its_crossing_draws_its_own_model() {
        let mut app = portal_app();
        let (door, mesh) = spawn_door(&mut app, interior_door(0x0005_6C1B));
        app.world_mut()
            .entity_mut(door)
            .insert((DoorState::Open { animated: false }, CrossingHeld));
        update(&mut app, 1);
        assert_eq!(
            visibility_of(&app, door),
            Visibility::Inherited,
            "a door waiting for its destination is drawn as the door it is"
        );
        assert!(leaf_is_drawn(&app, mesh));

        // The hold is over: it is an open static door again, and its model goes.
        app.world_mut().entity_mut(door).remove::<CrossingHeld>();
        update(&mut app, 1);
        assert_eq!(visibility_of(&app, door), Visibility::Hidden);
        assert!(!leaf_is_drawn(&app, mesh));
    }

    /// An animated door is not the portal's to hide. Its frame *is* the doorway - a model hidden
    /// while the quad stands in it would take the door with it - and its leaves are
    /// [`crate::door_animation`]'s to draw and hide: the swing is exactly what the player asked to
    /// see, and a doorway that emptied itself the moment it was asked to open would be no better
    /// than the teleport this feature replaces.
    #[test]
    fn an_animated_doors_model_is_not_the_portals_to_hide() {
        let mut app = portal_app();
        let (animated, animated_mesh) = spawn_door(&mut app, interior_door(0x0005_6C1B));
        let (still, still_mesh) = spawn_door(&mut app, interior_door(0x0005_6C1C));

        // Both open, the animated one mid-swing and the portal rendering through it.
        app.world_mut()
            .entity_mut(animated)
            .insert(DoorState::Opening);
        app.world_mut()
            .entity_mut(still)
            .insert(DoorState::Open { animated: false });
        portal_shows(&mut app, Some(animated));
        update(&mut app, 1);

        assert_eq!(
            visibility_of(&app, animated),
            Visibility::Inherited,
            "the animated door's frame and leaves are drawn: its animation is the opening"
        );
        assert!(leaf_is_drawn(&app, animated_mesh));
        assert_eq!(
            visibility_of(&app, still),
            Visibility::Hidden,
            "and a static door with no leaf to swing out of the way is still a hole"
        );
        assert!(!leaf_is_drawn(&app, still_mesh));

        // Open with the clip finished, the animated door is still its own model's business: the
        // leaves are `door_animation`'s answer (hidden when the swing left them in the opening,
        // drawn when it swung them clear), never this system's.
        app.world_mut()
            .entity_mut(animated)
            .insert(DoorState::Open { animated: true });
        portal_shows(&mut app, None);
        update(&mut app, 1);
        assert_eq!(visibility_of(&app, animated), Visibility::Inherited);
        assert!(leaf_is_drawn(&app, animated_mesh));

        // And while it swings **shut**: the same model, the same leaves, mid-swing the other way,
        // with the portal drawing its window through the doorway the leaf is coming back into. The
        // state is one the portal draws through (`portal_shows_through`), so this is the frame the
        // user's report is about - the doorway stayed a doorway under the closing leaf.
        app.world_mut()
            .entity_mut(animated)
            .insert(DoorState::Closing);
        portal_shows(&mut app, Some(animated));
        update(&mut app, 1);
        assert_eq!(
            visibility_of(&app, animated),
            Visibility::Inherited,
            "a closing door's frame and leaves are drawn too: it is still the swing"
        );
        assert!(leaf_is_drawn(&app, animated_mesh));
    }

    #[test]
    fn a_prestreamed_cell_is_off_the_main_camera_until_it_becomes_active() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (active_root, active_mesh) = spawn_cell(&mut app, INTERIOR_ALFTAND01, None, None, None);
        // The interior behind the door the player walks to: pre-streamed, and not the portal's
        // destination, because no door in an active cell leads to it yet.
        let (prestream_root, prestream_mesh) = spawn_cell(&mut app, 0x0005_6C1B, None, None, None);
        // A water surface of that cell belongs to the reflection layer when it is active.
        let (water, _) = spawn_mesh(&mut app, prestream_root, Some(RenderLayers::layer(1)));

        update(&mut app, 2);

        assert!(
            on_main_camera(&app, active_mesh),
            "the active cell is drawn"
        );
        assert_eq!(
            *app.world().entity(active_root).get::<Visibility>().unwrap(),
            Visibility::default()
        );
        assert!(
            !on_main_camera(&app, prestream_mesh),
            "a pre-streamed cell must not be drawn where the player stands"
        );
        assert!(!on_main_camera(&app, water));
        assert_eq!(
            *app.world()
                .entity(prestream_root)
                .get::<Visibility>()
                .unwrap(),
            Visibility::Hidden,
            "and the ray cast must not hit it either"
        );

        // A mesh of the same cell appearing a frame later - the glTF scene loads over several
        // frames - is hidden too.
        let (late, _) = spawn_mesh(&mut app, prestream_root, None);
        update(&mut app, 1);
        assert!(!on_main_camera(&app, late));

        // Entering the cell shows it again, meshes and layers alike.
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(0x0005_6C1B),
        };
        update(&mut app, 2);
        assert!(on_main_camera(&app, prestream_mesh));
        assert!(on_main_camera(&app, late));
        assert_eq!(
            layers_of(&app, water),
            RenderLayers::layer(1),
            "a water surface keeps the layer it was spawned with"
        );
        assert_eq!(
            *app.world()
                .entity(prestream_root)
                .get::<Visibility>()
                .unwrap(),
            Visibility::default()
        );
        assert!(
            !on_main_camera(&app, active_mesh),
            "and the cell left is not"
        );
    }

    #[test]
    fn a_cell_of_another_worldspace_stays_hidden_when_its_grid_matches_the_active_one() {
        let mut app = portal_app();
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: None,
        };
        spawn_camera(&mut app, Vec3::ZERO);
        // The camera is at the render origin, so its grid is the origin's own.
        let grid = IVec2::new(19, 18);
        let (tamriel_root, tamriel_mesh) = spawn_cell(
            &mut app,
            TAMRIEL * 100,
            Some(grid),
            Some(CellKey::Exterior {
                worldspace_id: TAMRIEL,
                grid_x: grid.x,
                grid_y: grid.y,
            }),
            None,
        );
        // AlftandWorld's door 4 arrives at Blackreach grid (5,4) while AlftandWorld's own cells sit
        // at grid (0,0) and lower: the grids of two worldspaces can coincide, and only the key says
        // which one is the active space.
        let (blackreach_root, blackreach_mesh) = spawn_cell(
            &mut app,
            0x0001_EE62 * 100,
            Some(grid),
            Some(CellKey::Exterior {
                worldspace_id: 0x0001_EE62,
                grid_x: grid.x,
                grid_y: grid.y,
            }),
            None,
        );

        update(&mut app, 2);

        assert!(on_main_camera(&app, tamriel_mesh));
        assert_eq!(
            *app.world()
                .entity(tamriel_root)
                .get::<Visibility>()
                .unwrap(),
            Visibility::default()
        );
        assert!(!on_main_camera(&app, blackreach_mesh));
        assert_eq!(
            *app.world()
                .entity(blackreach_root)
                .get::<Visibility>()
                .unwrap(),
            Visibility::Hidden
        );
    }

    /// Puts `entity` on `layers` behind the change detection's back: nothing that reads
    /// `Changed<RenderLayers>` sees it, so only a write - the isolation's or the propagation's -
    /// would put it back. What the tests below tell a node that was left alone from one that was
    /// written.
    fn set_layers_unseen(app: &mut App, entity: Entity, layers: RenderLayers) {
        let mut entity = app.world_mut().entity_mut(entity);
        let mut current = entity.get_mut::<RenderLayers>().unwrap();
        *current.bypass_change_detection() = layers;
    }

    #[test]
    fn a_cell_whose_role_did_not_change_is_not_walked() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (_, hidden_mesh) = spawn_cell(&mut app, 0x0005_6C1B, None, None, None);
        update(&mut app, 2);
        assert_eq!(layers_of(&app, hidden_mesh), RenderLayers::none());

        // A frame in which nothing about the cell changed: its mesh is not looked at, so a layer
        // put on it unseen stays where it was put.
        set_layers_unseen(&mut app, hidden_mesh, RenderLayers::layer(0));
        update(&mut app, 3);
        assert_eq!(
            layers_of(&app, hidden_mesh),
            RenderLayers::layer(0),
            "a cell that kept its role is not walked"
        );
    }

    #[test]
    fn a_cell_whose_role_changed_is_re_layered_once() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (_, active_mesh) = spawn_cell(&mut app, INTERIOR_ALFTAND01, None, None, None);
        let (other_root, other_mesh) = spawn_cell(&mut app, 0x0005_6C1B, None, None, None);
        let (water, _) = spawn_mesh(&mut app, other_root, Some(RenderLayers::layer(1)));
        update(&mut app, 2);
        assert!(on_main_camera(&app, active_mesh));
        assert!(!on_main_camera(&app, other_mesh));
        let (cells_before, nodes_before) = isolate_counters(&app);

        // The player crosses into the other cell: both roles change, and every mesh of both - the
        // water surface, which keeps its own layer, included - takes its layers in the one frame
        // the crossing is seen in.
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(0x0005_6C1B),
        };
        update(&mut app, 1);
        assert!(on_main_camera(&app, other_mesh));
        assert_eq!(layers_of(&app, water), RenderLayers::layer(1));
        assert_eq!(layers_of(&app, active_mesh), RenderLayers::none());
        let (cells_after, nodes_after) = isolate_counters(&app);
        assert_eq!(
            cells_after - cells_before,
            2,
            "one write per root whose role changed"
        );
        assert_eq!(
            nodes_after - nodes_before,
            1,
            "and one per node with layers of its own: the water surface"
        );

        // And only in that frame: a settled cell is neither walked nor written, so a layer put on
        // one of its meshes behind the change detection's back stays where it was put.
        update(&mut app, 1);
        set_layers_unseen(&mut app, active_mesh, RenderLayers::layer(0));
        update(&mut app, 3);
        assert_eq!(
            layers_of(&app, active_mesh),
            RenderLayers::layer(0),
            "a role change is re-layered once, not every frame after it"
        );
        assert_eq!(isolate_counters(&app), (cells_after, nodes_after));
    }

    /// The two counters [`isolate_cells`] records, read the way a door-tour benchmark reads them:
    /// out of the profile bundle's `cpu-spans.json`, the counter map's only door.
    fn isolate_counters(app: &App) -> (u64, u64) {
        let directory = tempfile::tempdir().unwrap();
        let config = EngineConfig {
            profile_output_dir: Some(directory.path().to_owned()),
            ..EngineConfig::default()
        };
        app.world()
            .resource::<ProfilingState>()
            .write_bundle(
                &config,
                &serde_json::json!({}),
                None,
                &RendererMetrics::default(),
                None,
                None,
            )
            .unwrap();
        let bundle: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.path().join("cpu-spans.json")).unwrap(),
        )
        .unwrap();
        let counter = |name: &str| bundle["counters"][name].as_u64().unwrap_or(0);
        (
            counter("portal/cells_relayered"),
            counter("portal/nodes_relayered"),
        )
    }

    #[test]
    fn a_role_change_is_measured_as_an_isolate_span_and_churn_counters() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (_, active_mesh) = spawn_cell(&mut app, INTERIOR_ALFTAND01, None, None, None);
        let (other_root, _) = spawn_cell(&mut app, 0x0005_6C1B, None, None, None);
        spawn_mesh(&mut app, other_root, Some(RenderLayers::layer(1)));
        update(&mut app, 2);
        assert!(on_main_camera(&app, active_mesh));

        let spans = |app: &App| {
            app.world()
                .resource::<ProfilingState>()
                .span_samples("portal/isolate")
                .to_vec()
        };
        assert_eq!(
            spans(&app).len(),
            2,
            "one portal/isolate sample per frame the system ran"
        );
        assert!(
            spans(&app).iter().any(|ms| *ms > 0.0),
            "the walk of a first-seen root is real time, not a recorded zero"
        );
        let (cells_before, nodes_before) = isolate_counters(&app);

        // The player crosses into the other cell: both roots change role, so the one frame the
        // crossing is seen in writes both roots' sources - and, per node, only the water surface,
        // which has layers of its own; every other node of the two cells follows the propagation.
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(0x0005_6C1B),
        };
        update(&mut app, 1);
        assert_eq!(spans(&app).len(), 3, "the crossing frame is measured too");
        let (cells_after, nodes_after) = isolate_counters(&app);
        assert_eq!(
            cells_after - cells_before,
            2,
            "the two roots whose role changed are the frame's relayered cells"
        );
        assert_eq!(
            nodes_after - nodes_before,
            1,
            "one write per node with layers of its own, never one per node of a cell"
        );

        // The frame after, the roles have settled: the span is still recorded, the churn is not.
        update(&mut app, 1);
        assert_eq!(spans(&app).len(), 4);
        assert_eq!(isolate_counters(&app), (cells_after, nodes_after));
    }

    #[test]
    fn a_scene_that_finishes_spawning_under_a_settled_cell_is_isolated() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (root, _) = spawn_cell(&mut app, 0x0005_6C1B, None, None, None);
        // A reference whose glTF scene has not spawned yet: a node with no children.
        let reference = app
            .world_mut()
            .spawn((Transform::default(), Visibility::default(), ChildOf(root)))
            .id();
        update(&mut app, 3);

        // The scene arrives frames later, two levels deep, while the cell's role stays the same.
        let scene = app
            .world_mut()
            .spawn((
                Transform::default(),
                Visibility::default(),
                ChildOf(reference),
            ))
            .id();
        let (deep_mesh, _) = spawn_mesh(&mut app, scene, None);
        let (water, _) = spawn_mesh(&mut app, scene, Some(RenderLayers::layer(1)));
        update(&mut app, 1);
        assert_eq!(layers_of(&app, deep_mesh), RenderLayers::none());
        assert_eq!(layers_of(&app, water), RenderLayers::none());

        // Something else giving a node of the settled cell layers of its own is caught too.
        let (late_layered, _) = spawn_mesh(&mut app, reference, None);
        update(&mut app, 1);
        app.world_mut()
            .entity_mut(late_layered)
            .insert(RenderLayers::layer(1));
        update(&mut app, 1);
        assert_eq!(layers_of(&app, late_layered), RenderLayers::none());

        // Entering the cell gives every one of them back the layers it had.
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(0x0005_6C1B),
        };
        update(&mut app, 1);
        assert!(on_main_camera(&app, deep_mesh));
        assert_eq!(layers_of(&app, water), RenderLayers::layer(1));
        assert!(
            on_main_camera(&app, late_layered),
            "the layers it asked for with the source of its own"
        );
    }

    /// A reference under a cell root with a model hanging off it, the way `crate::render`'s
    /// placed-object propagation builds one: the reference carries a `Propagate<RenderLayers>` of
    /// its own and its model inherits it. A nested source like this stops the cell's value where it
    /// stands, which is why the isolation has to fold the cell's role into it.
    fn spawn_reference(app: &mut App, root: Entity) -> (Entity, Entity) {
        let reference = app
            .world_mut()
            .spawn((
                Propagate(crate::render::PLACED_OBJECT_RENDER_LAYERS),
                ChildOf(root),
            ))
            .id();
        let (model, _) = spawn_mesh(app, reference, None);
        (reference, model)
    }

    /// A cell is **born** on its role's layers: the frame `streaming::spawn_cell` hands the root a
    /// [`StreamedCellKey`], with the meshes it spawns with - and the model of every reference it
    /// carries, which `crate::render` puts on the placed-object layer - and no frame later. What the
    /// per-node walk used to buy, for one write per cell.
    #[test]
    fn a_cell_spawned_under_a_role_is_on_its_layers_in_its_first_frame() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let door = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<PortalState>()
            .draw_destination(door, vec![CellKey::Interior(PRESTREAMED_CELL)]);
        let key = |cell_id| Some(CellKey::Interior(cell_id));
        let (active_root, active_mesh) = spawn_cell(
            &mut app,
            INTERIOR_ALFTAND01,
            None,
            key(INTERIOR_ALFTAND01),
            None,
        );
        let (destination_root, destination_mesh) = spawn_cell(
            &mut app,
            PRESTREAMED_CELL,
            None,
            key(PRESTREAMED_CELL),
            None,
        );
        let (hidden_root, hidden_mesh) =
            spawn_cell(&mut app, 0x0006_1234, None, key(0x0006_1234), None);
        let (_, active_model) = spawn_reference(&mut app, active_root);
        let (_, destination_model) = spawn_reference(&mut app, destination_root);
        let (_, hidden_model) = spawn_reference(&mut app, hidden_root);
        let (water, _) = spawn_mesh(&mut app, destination_root, Some(RenderLayers::layer(1)));

        app.update();

        // The active space: Bevy's own layer, and the placed-object layer a reference's model
        // carries by its own source.
        assert_eq!(layers_of(&app, active_mesh), RenderLayers::default());
        assert_eq!(
            layers_of(&app, active_model),
            crate::render::PLACED_OBJECT_RENDER_LAYERS
        );
        assert!(on_main_camera(&app, active_model));
        // The portal's destination: every mesh of it on the portal camera's layer, the water
        // surface and the reference's model included, so the doorway draws the room behind it whole.
        assert_eq!(
            layers_of(&app, destination_mesh),
            RenderLayers::layer(DESTINATION_LAYER)
        );
        assert_eq!(
            layers_of(&app, destination_model),
            RenderLayers::layer(DESTINATION_LAYER)
        );
        assert_eq!(
            layers_of(&app, water),
            RenderLayers::layer(DESTINATION_LAYER)
        );
        assert!(!on_main_camera(&app, destination_mesh));
        assert!(!on_main_camera(&app, destination_model));
        // Any other resident cell: no camera draws it, and the ray casts are kept off it too.
        assert_eq!(layers_of(&app, hidden_mesh), RenderLayers::none());
        assert_eq!(layers_of(&app, hidden_model), RenderLayers::none());
        assert_eq!(
            *app.world().entity(hidden_root).get::<Visibility>().unwrap(),
            Visibility::Hidden,
            "a hidden cell is invisible to the ray casts, which read no layers"
        );
    }

    /// A committed cell costs one write - its root's source - and not one per node, and a frame in
    /// which nothing about a cell changed writes nothing at all: what the frame chain's counters
    /// (#642) are read for.
    #[test]
    fn a_committed_cell_costs_no_node_writes_and_a_settled_one_none() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        update(&mut app, 2);
        let (cells_before, nodes_before) = isolate_counters(&app);

        let (_, mesh) = spawn_cell(
            &mut app,
            PRESTREAMED_CELL,
            None,
            Some(CellKey::Interior(PRESTREAMED_CELL)),
            None,
        );
        update(&mut app, 1);
        assert_eq!(layers_of(&app, mesh), RenderLayers::none());
        let (cells_after, nodes_after) = isolate_counters(&app);
        assert_eq!(
            cells_after - cells_before,
            1,
            "a committed cell is one role stamped, not a subtree walked"
        );
        assert_eq!(
            nodes_after - nodes_before,
            0,
            "and not one node of it is written: the propagation carries the root's value"
        );

        update(&mut app, 3);
        assert_eq!(
            isolate_counters(&app),
            (cells_after, nodes_after),
            "a settled cell is not touched again"
        );
    }

    /// A role change moves a reference's model like any other mesh, in the one frame it is seen in:
    /// the reference is a source of its own, and its value is the cell's role applied to the layers
    /// that source carries, never the nested source's own value.
    #[test]
    fn a_role_change_moves_a_references_model_in_one_frame() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (root, mesh) = spawn_cell(&mut app, PRESTREAMED_CELL, None, None, None);
        let (reference, model) = spawn_reference(&mut app, root);
        let (water, _) = spawn_mesh(&mut app, root, Some(RenderLayers::layer(1)));
        update(&mut app, 2);
        assert_eq!(layers_of(&app, reference), RenderLayers::none());
        assert_eq!(layers_of(&app, model), RenderLayers::none());
        assert_eq!(layers_of(&app, water), RenderLayers::none());

        // The player walks in: in the one frame the new space is seen in, the reference and its
        // model are back on the placed-object layer and the water surface on the layer of its own.
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(PRESTREAMED_CELL),
        };
        update(&mut app, 1);
        assert!(on_main_camera(&app, mesh));
        assert_eq!(
            layers_of(&app, reference),
            crate::render::PLACED_OBJECT_RENDER_LAYERS
        );
        assert_eq!(
            layers_of(&app, model),
            crate::render::PLACED_OBJECT_RENDER_LAYERS
        );
        assert_eq!(layers_of(&app, water), RenderLayers::layer(1));

        // And back behind a doorway: the whole cell, model included, is the portal camera's alone.
        let door = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<PortalState>()
            .draw_destination(door, vec![CellKey::Interior(PRESTREAMED_CELL)]);
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(INTERIOR_ALFTAND01),
        };
        update(&mut app, 1);
        assert_eq!(
            layers_of(&app, model),
            RenderLayers::layer(DESTINATION_LAYER)
        );
        assert_eq!(
            layers_of(&app, water),
            RenderLayers::layer(DESTINATION_LAYER)
        );
        assert!(!on_main_camera(&app, mesh));
    }

    /// A node a mod gives layers to is a source of its own: the cell's role still reaches it - and
    /// everything below it - and the layers it asked for are back in the frame its cell is active in
    /// again.
    #[test]
    fn a_node_with_layers_of_its_own_keeps_them_in_its_active_space() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (root, mesh) = spawn_cell(&mut app, PRESTREAMED_CELL, None, None, None);
        update(&mut app, 2);
        // The mod's node arrives while the cell is hidden, with a layer of its own.
        let (modded, _) = spawn_mesh(&mut app, root, Some(RenderLayers::layer(1)));
        update(&mut app, 1);
        assert_eq!(
            layers_of(&app, modded),
            RenderLayers::none(),
            "a hidden cell's value is what its nodes carry"
        );

        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(PRESTREAMED_CELL),
        };
        update(&mut app, 1);
        assert_eq!(
            layers_of(&app, modded),
            RenderLayers::layer(1),
            "its own layer back, in the frame the cell is entered"
        );
        assert_eq!(
            layers_of(&app, mesh),
            RenderLayers::default(),
            "and the cell's own meshes on Bevy's layer"
        );

        // Given to a node of the settled cell too: it is adopted where it is given the layers, so
        // nothing overwrites them.
        let (settled, _) = spawn_mesh(&mut app, root, None);
        update(&mut app, 1);
        app.world_mut()
            .entity_mut(settled)
            .insert(RenderLayers::layer(1));
        update(&mut app, 1);
        assert_eq!(
            layers_of(&app, settled),
            RenderLayers::layer(1),
            "an active cell keeps a node's own layers"
        );

        // Taken off it again - what `hide_doorway_filler` does with a door model's filler - the
        // layer it was given is not the one the cell keeps for it.
        app.world_mut().entity_mut(settled).remove::<RenderLayers>();
        update(&mut app, 1);
        assert_eq!(layers_of(&app, settled), RenderLayers::default());
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(INTERIOR_ALFTAND01),
        };
        update(&mut app, 1);
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(PRESTREAMED_CELL),
        };
        update(&mut app, 1);
        assert_eq!(
            layers_of(&app, settled),
            RenderLayers::default(),
            "the layer it was given is not the one its own space gives back"
        );
    }

    /// Spawns a cell root the way `streaming::spawn_cell` does - its identity, then the key - and
    /// flushes the observer's commands: what the root is before any frame runs.
    fn spawn_stamped_cell(app: &mut App, cell_id: u32) -> Entity {
        let root = app
            .world_mut()
            .spawn((
                StreamedCellRoot,
                CellRef(cell_id),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        app.world_mut()
            .entity_mut(root)
            .insert(StreamedCellKey(CellKey::Interior(cell_id)));
        app.world_mut().flush();
        root
    }

    /// A root is stamped the moment `streaming::spawn_cell` hands it its key - before a frame runs -
    /// and the isolation agrees with what it stamped, which is why a committed cell costs a
    /// comparison and no walk: the layer source and the role are what the cell is *born* on.
    #[test]
    fn a_root_is_stamped_the_moment_it_spawns() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let door = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<PortalState>()
            .draw_destination(door, vec![CellKey::Interior(PRESTREAMED_CELL)]);

        let hidden = spawn_stamped_cell(&mut app, 0x0006_1234);
        let destination = spawn_stamped_cell(&mut app, PRESTREAMED_CELL);
        let active = spawn_stamped_cell(&mut app, INTERIOR_ALFTAND01);

        let stamp = |app: &App, root: Entity| {
            app.world()
                .entity(root)
                .get::<CellRoleStamp>()
                .map(|stamp| stamp.0)
        };
        let source = |app: &App, root: Entity| {
            app.world()
                .entity(root)
                .get::<Propagate<RenderLayers>>()
                .map(|propagate| propagate.0.clone())
        };
        assert_eq!(stamp(&app, hidden), Some(CellRole::Hidden));
        assert_eq!(source(&app, hidden), Some(RenderLayers::none()));
        assert_eq!(
            *app.world().entity(hidden).get::<Visibility>().unwrap(),
            Visibility::Hidden,
            "and the ray casts are kept off it from that same moment"
        );
        assert_eq!(stamp(&app, destination), Some(CellRole::Destination));
        assert_eq!(
            source(&app, destination),
            Some(RenderLayers::layer(DESTINATION_LAYER))
        );
        assert_eq!(stamp(&app, active), Some(CellRole::Active));
        assert_eq!(
            source(&app, active),
            None,
            "the active space has no source: its nodes keep the layers they were spawned with"
        );

        // The isolation finds the stamps already right: nothing of either cell is written again.
        let (cells_before, nodes_before) = isolate_counters(&app);
        update(&mut app, 2);
        assert_eq!(
            isolate_counters(&app),
            (cells_before, nodes_before),
            "a stamped cell is never walked, let alone re-layered"
        );
    }

    /// The role a node takes comes from the classification of the frame, and only from the root's
    /// stamp where the classification has nothing for the root - a cell the isolation has not seen
    /// yet. The stamp can be a frame older than the classification: `streaming` stamps a cell with
    /// the space of the moment it spawned in.
    #[test]
    fn the_role_a_source_takes_is_this_frames_classification() {
        let mut state = PortalState::default();
        let root = Entity::PLACEHOLDER;
        assert_eq!(
            current_role(&state, root, Some(CellRole::Hidden)),
            Some(CellRole::Hidden),
            "a root the isolation has not classified yet keeps its stamp"
        );
        assert_eq!(current_role(&state, root, None), None);
        state.roles.insert(root, CellRole::Destination);
        assert_eq!(
            current_role(&state, root, Some(CellRole::Hidden)),
            Some(CellRole::Destination),
            "the classification of the frame wins over an older stamp"
        );
    }

    /// The node a test's own system spawns *after* the isolation has run in a frame, once it is armed
    /// with the cell to spawn it under.
    #[derive(Resource, Default)]
    struct SpawnedAfterIsolation {
        root: Option<Entity>,
        spawned: Option<Entity>,
    }

    /// Spawns the node [`SpawnedAfterIsolation`] is armed with, once, after the frame's isolation.
    fn spawn_after_isolate(mut commands: Commands, mut spawned: ResMut<SpawnedAfterIsolation>) {
        let (Some(root), None) = (spawned.root, spawned.spawned) else {
            return;
        };
        spawned.spawned = Some(commands.spawn((RenderLayers::layer(1), ChildOf(root))).id());
    }

    /// A source that arrives in the frame a cell changes role takes the new role in that same frame,
    /// whether it was adopted before the isolation ran in the frame or - the case whose role cannot
    /// come from a settled frame - after it: the walk from the node finds its cell, and the role is
    /// the classification of this frame ([`current_role`]) rather than the root's stamp.
    #[test]
    fn a_source_adopted_in_the_frame_of_a_role_change_takes_the_new_role() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (root, _) = spawn_cell(&mut app, PRESTREAMED_CELL, None, None, None);
        app.init_resource::<SpawnedAfterIsolation>()
            .add_systems(Update, spawn_after_isolate.after(isolate_cells));
        update(&mut app, 2);

        // The player walks in, the node arrives after the isolation of that frame has run, and one
        // node of the cell is spawned before the frame with it.
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(PRESTREAMED_CELL),
        };
        app.world_mut().resource_mut::<SpawnedAfterIsolation>().root = Some(root);
        let before = spawn_mesh(&mut app, root, Some(RenderLayers::layer(1))).0;
        update(&mut app, 1);

        let after = app
            .world()
            .resource::<SpawnedAfterIsolation>()
            .spawned
            .expect("the armed system spawned its node");
        for (entity, when) in [
            (before, "before the isolation"),
            (after, "after the isolation"),
        ] {
            assert_eq!(
                layers_of(&app, entity),
                RenderLayers::layer(1),
                "a source of the crossing frame takes the role the frame settled on ({when})"
            );
        }
    }

    /// A node of a native door the door layer draws in place of is dark too, even when it has layers
    /// of its own (the filler `crate::render` keeps on a layer of its own, a light, a mod's node):
    /// it would otherwise stay drawn on the main camera under the door-layer door that hides the
    /// door it belongs to - and it has its own layers back the frame the layer releases the door.
    #[test]
    fn a_node_of_a_hidden_native_door_with_layers_of_its_own_is_dark_too() {
        let mut app = door_layer_app();
        let camera = spawn_camera(&mut app, Vec3::new(0.0, 120.0, 0.0));
        let case = door_layer_pair(&mut app);
        // A node of the driver's own model that has a layer of its own.
        let (modded, _) = spawn_mesh(&mut app, case.driver, Some(RenderLayers::layer(1)));
        door_layer_door(&mut app, &case);
        assert_eq!(
            layers_of(&app, modded),
            RenderLayers::none(),
            "the whole model of a door the layer draws in place of is off every camera"
        );

        // Walked away from: the pair leaves the layer, and the node is back on its own layer with it.
        move_camera(
            &mut app,
            camera,
            Transform::from_translation(Vec3::new(0.0, 120.0, 5000.0)),
        );
        update(&mut app, 3);
        assert!(layer_copies(&mut app).is_empty());
        assert_eq!(
            layers_of(&app, modded),
            RenderLayers::layer(1),
            "its own layer back, with the door that held it"
        );
    }

    /// A source that despawned is dropped from its cell's list the next time the cell's role is
    /// stamped: the world recycles an entity's id, and a recycled one must never take a role that
    /// belonged to a node which is gone.
    #[test]
    fn a_source_that_despawned_is_dropped_from_its_cells_list() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (root, _) = spawn_cell(&mut app, PRESTREAMED_CELL, None, None, None);
        let (modded, _) = spawn_mesh(&mut app, root, Some(RenderLayers::layer(1)));
        update(&mut app, 2);
        let sources = |app: &App| {
            app.world()
                .resource::<CellLayerSources>()
                .by_root
                .get(&root)
                .cloned()
                .unwrap_or_default()
        };
        assert_eq!(sources(&app), vec![modded]);

        app.world_mut().entity_mut(modded).despawn();
        // The role change of the cell (the player walks in) is what walks its list.
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(PRESTREAMED_CELL),
        };
        update(&mut app, 1);
        assert!(
            sources(&app).is_empty(),
            "a dead id is not carried into the next role change"
        );
    }

    /// A light spawned the way `crate::render::light_render_layers` builds one - its own layers and
    /// a `PropagateOver` that keeps a reference's placed-object layer off it - still follows its
    /// *cell's* role: the portal camera's layer in a destination (where `lights::budget_lights`
    /// counts it for the doorway), no layer at all in a hidden cell, and its own layers when its
    /// cell is the active space again.
    #[test]
    fn a_light_under_a_reference_takes_its_cells_role() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let door = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<PortalState>()
            .draw_destination(door, vec![CellKey::Interior(PRESTREAMED_CELL)]);
        let (destination_root, _) = spawn_cell(
            &mut app,
            PRESTREAMED_CELL,
            None,
            Some(CellKey::Interior(PRESTREAMED_CELL)),
            None,
        );
        let (hidden_root, _) = spawn_cell(
            &mut app,
            0x0006_1234,
            None,
            Some(CellKey::Interior(0x0006_1234)),
            None,
        );
        let light_of = |app: &mut App, root: Entity| {
            app.world_mut()
                .spawn((crate::render::light_render_layers(), ChildOf(root)))
                .id()
        };
        let destination_light = light_of(&mut app, destination_root);
        let hidden_light = light_of(&mut app, hidden_root);

        update(&mut app, 1);

        assert_eq!(
            layers_of(&app, destination_light),
            RenderLayers::layer(DESTINATION_LAYER)
        );
        assert_eq!(layers_of(&app, hidden_light), RenderLayers::none());
        assert!(
            app.world()
                .entity(destination_light)
                .get::<PropagateOver<RenderLayers>>()
                .is_none(),
            "the cell's role is what moves the light: the stop is off it"
        );

        // The active space gives the light the layers it was spawned with.
        *app.world_mut().resource_mut::<ActiveCell>() = ActiveCell {
            worldspace_id: TAMRIEL,
            interior: Some(PRESTREAMED_CELL),
        };
        update(&mut app, 1);
        assert_eq!(
            layers_of(&app, destination_light),
            RenderLayers::from_layers(&crate::render::LIGHT_LAYERS)
        );
    }

    /// A node that stops the propagation at itself cannot stand under a cell the portal isolates:
    /// nothing below it would ever take the cell's value, so a destination's meshes under it would
    /// stay on the layer they were spawned with - the main camera's - and draw over the space the
    /// player is standing in.
    #[test]
    fn a_propagate_stop_under_a_cell_does_not_keep_a_mesh_on_the_main_camera() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let door = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<PortalState>()
            .draw_destination(door, vec![CellKey::Interior(PRESTREAMED_CELL)]);
        let (root, _) = spawn_cell(
            &mut app,
            PRESTREAMED_CELL,
            None,
            Some(CellKey::Interior(PRESTREAMED_CELL)),
            None,
        );
        let stop = app
            .world_mut()
            .spawn((PropagateStop::<RenderLayers>::default(), ChildOf(root)))
            .id();
        let (mesh, _) = spawn_mesh(&mut app, stop, None);

        update(&mut app, 1);

        assert_eq!(
            layers_of(&app, mesh),
            RenderLayers::layer(DESTINATION_LAYER),
            "the cell's value reaches below the node that stopped it"
        );
        assert!(!on_main_camera(&app, mesh));
        assert!(
            app.world()
                .entity(stop)
                .get::<PropagateStop<RenderLayers>>()
                .is_none(),
            "the stop is taken off where it stands"
        );
    }

    /// The propagation's own cost is measured beside the isolation's: `portal/isolate` is the
    /// portal's writes, `portal/propagate` the per-node ones Bevy does for the cell's role.
    #[test]
    fn the_layer_propagation_is_measured_beside_the_isolation() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        update(&mut app, 1);
        let spans = app
            .world()
            .resource::<ProfilingState>()
            .span_samples("portal/propagate")
            .to_vec();
        assert_eq!(
            spans.len(),
            1,
            "one portal/propagate sample per frame the propagation ran"
        );
        assert!(
            spans[0] > 0.0,
            "a recorded zero would mean the span was never opened"
        );
    }

    // -----------------------------------------------------------------------------------------
    // The doorway image: its size and its range (notes 1 and 2 of the brief)
    // -----------------------------------------------------------------------------------------

    /// A camera whose computed target is `size`: what `camera_system` leaves on a camera that
    /// draws into a target of that many physical pixels.
    fn camera_targeting(size: UVec2) -> Camera {
        let mut camera = Camera::default();
        camera.computed.target_info = Some(RenderTargetInfo {
            physical_size: size,
            scale_factor: 1.0,
        });
        camera
    }

    /// An app with the three things [`resize_portal_target`] repoints - the target resource, the
    /// portal camera's `RenderTarget` and the quad's material - plus a main camera the test moves.
    fn resize_app(target: UVec2) -> (App, Entity, Entity, Entity, Handle<Image>) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<PortalMaterial>();
        let fallback = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(portal_target_image(PORTAL_TEXTURE_FALLBACK_SIZE));
        app.insert_resource(PortalTexture(fallback.clone()));
        let material = app
            .world_mut()
            .resource_mut::<Assets<PortalMaterial>>()
            .add(PortalMaterial::default());
        let camera = app
            .world_mut()
            .spawn((
                PortalCamera,
                Camera::default(),
                RenderTarget::Image(fallback.clone().into()),
            ))
            .id();
        let quad = app
            .world_mut()
            .spawn((PortalQuad, MeshMaterial3d(material)))
            .id();
        let main = app
            .world_mut()
            .spawn((StreamingCamera, camera_targeting(target)))
            .id();
        app.add_systems(Update, resize_portal_target);
        (app, main, camera, quad, fallback)
    }

    /// The image the portal's target resource points at.
    fn target_image(app: &App) -> Handle<Image> {
        app.world().resource::<PortalTexture>().0.clone()
    }

    /// The size of a target image, from the asset itself.
    fn target_size_of(app: &App, image: &Handle<Image>) -> UVec2 {
        app.world()
            .resource::<Assets<Image>>()
            .get(image)
            .expect("the portal target image")
            .size()
    }

    /// The image the portal camera renders into.
    fn camera_target_of(app: &App, camera: Entity) -> Handle<Image> {
        match app.world().entity(camera).get::<RenderTarget>().unwrap() {
            RenderTarget::Image(target) => target.handle.clone(),
            other => panic!("the portal camera draws into {other:?}"),
        }
    }

    /// The image the doorway quad's material samples.
    fn quad_texture_of(app: &App, quad: Entity) -> Handle<Image> {
        let handle = app
            .world()
            .entity(quad)
            .get::<MeshMaterial3d<PortalMaterial>>()
            .unwrap();
        app.world()
            .resource::<Assets<PortalMaterial>>()
            .get(handle)
            .expect("the quad's material")
            .extension
            .portal_texture
            .clone()
            .expect("a portal texture")
    }

    /// Sets the size the main camera's own target is.
    fn main_target_size(app: &mut App, main: Entity, size: UVec2) {
        app.world_mut()
            .entity_mut(main)
            .get_mut::<Camera>()
            .unwrap()
            .computed
            .target_info = Some(RenderTargetInfo {
            physical_size: size,
            scale_factor: 1.0,
        });
    }

    /// The size the doorway is drawn at follows the main camera's own target, resize for resize,
    /// and an unchanged size is not a resize: no image is created, and the camera and the quad keep
    /// reading the one they have.
    #[test]
    fn the_portal_target_follows_the_main_camera_and_is_only_rebuilt_when_it_changes() {
        let (mut app, main, camera, quad, fallback) = resize_app(UVec2::new(1600, 900));
        assert_eq!(
            target_size_of(&app, &fallback),
            PORTAL_TEXTURE_FALLBACK_SIZE,
            "a run starts with the fallback target"
        );

        // The first frame the main camera's target is known: the doorway is drawn at the window's
        // own resolution, and both ends of it - the camera that writes it and the quad that samples
        // it - are on the new image in that same frame.
        update(&mut app, 1);
        let window = target_image(&app);
        assert_ne!(
            window, fallback,
            "the fallback is replaced by the window's size"
        );
        assert_eq!(target_size_of(&app, &window), UVec2::new(1600, 900));
        assert_eq!(camera_target_of(&app, camera), window);
        assert_eq!(quad_texture_of(&app, quad), window);

        // Frames that change nothing allocate nothing: the same asset, and not one new image added
        // to the assets at all.
        let images = app.world().resource::<Assets<Image>>().len();
        update(&mut app, 4);
        assert_eq!(
            target_image(&app),
            window,
            "a repeated size must not ask for a new image"
        );
        assert_eq!(
            app.world().resource::<Assets<Image>>().len(),
            images,
            "and must not allocate one either"
        );

        // A resized window is a new target of the new size, repointed the same way.
        main_target_size(&mut app, main, UVec2::new(1280, 720));
        update(&mut app, 1);
        let smaller = target_image(&app);
        assert_ne!(
            smaller, window,
            "the window changed size, so the target did"
        );
        assert_eq!(target_size_of(&app, &smaller), UVec2::new(1280, 720));
        assert_eq!(camera_target_of(&app, camera), smaller);
        assert_eq!(quad_texture_of(&app, quad), smaller);

        // A window larger than the ceiling is scaled down to it, both axes by one factor.
        main_target_size(&mut app, main, UVec2::new(3840, 2160));
        update(&mut app, 1);
        assert_eq!(
            target_size_of(&app, &target_image(&app)),
            PORTAL_TEXTURE_MAX_SIZE
        );
        assert_eq!(camera_target_of(&app, camera), target_image(&app));
        assert_eq!(quad_texture_of(&app, quad), target_image(&app));
    }

    /// `--portal-scale` sizes the target at that fraction of the rectangle, and a change of the
    /// setting while running (the graphics panel's knob) resizes it on the next frame.
    #[test]
    fn the_portal_scale_setting_sizes_the_target_and_takes_effect_live() {
        let (mut app, _, camera, quad, _) = resize_app(UVec2::new(1600, 900));
        let mut settings = crate::graphics_settings::GraphicsSettings::bevy();
        settings.portal_scale = 0.5;
        app.insert_resource(settings);
        update(&mut app, 1);
        let half = target_image(&app);
        assert_eq!(target_size_of(&app, &half), UVec2::new(800, 450));
        assert_eq!(camera_target_of(&app, camera), half);
        assert_eq!(quad_texture_of(&app, quad), half);

        app.world_mut()
            .resource_mut::<crate::graphics_settings::GraphicsSettings>()
            .portal_scale = 1.0;
        update(&mut app, 1);
        let full = target_image(&app);
        assert_ne!(full, half, "the live change is a new target");
        assert_eq!(target_size_of(&app, &full), UVec2::new(1600, 900));
        assert_eq!(quad_texture_of(&app, quad), full);
    }

    /// The scale math: 1 is the unscaled size, a fraction rounds to whole texels, a scale out of
    /// range is held to 0.1..=1, the ceiling still applies, and a zero axis is still no size.
    #[test]
    fn the_scaled_target_size_rounds_clamps_and_keeps_the_ceiling() {
        let rect = UVec2::new(301, 200);
        assert_eq!(
            scaled_portal_target_size(rect, 1.0),
            portal_target_size(rect, PORTAL_TEXTURE_MAX_SIZE)
        );
        assert_eq!(
            scaled_portal_target_size(rect, 0.5),
            Some(UVec2::new(151, 100))
        );
        assert_eq!(
            scaled_portal_target_size(rect, 4.0),
            Some(rect),
            "held to 1"
        );
        assert_eq!(
            scaled_portal_target_size(rect, 0.0),
            Some(UVec2::new(30, 20)),
            "held to 0.1"
        );
        assert_eq!(
            scaled_portal_target_size(UVec2::new(2, 2), 0.1),
            Some(UVec2::ONE),
            "at least a texel a side"
        );
        assert_eq!(
            scaled_portal_target_size(UVec2::new(3840, 2160), 1.0),
            Some(PORTAL_TEXTURE_MAX_SIZE)
        );
        assert_eq!(
            scaled_portal_target_size(UVec2::new(3840, 2160), 0.5),
            Some(UVec2::new(1920, 1080))
        );
        assert_eq!(scaled_portal_target_size(UVec2::new(0, 900), 0.5), None);
    }

    /// A window that is minimized has no size to follow - its target is 0x0 and a 0x0 texture is
    /// not a render target - so the doorway keeps the image it has until the window comes back.
    #[test]
    fn a_minimized_window_leaves_the_portal_target_where_it_was() {
        let (mut app, main, camera, _, _) = resize_app(UVec2::new(1600, 900));
        update(&mut app, 1);
        let window = target_image(&app);

        main_target_size(&mut app, main, UVec2::ZERO);
        update(&mut app, 3);
        assert_eq!(target_image(&app), window);
        assert_eq!(target_size_of(&app, &window), UVec2::new(1600, 900));
        assert_eq!(camera_target_of(&app, camera), window);

        // Half a window is no size either.
        main_target_size(&mut app, main, UVec2::new(1920, 0));
        update(&mut app, 2);
        assert_eq!(target_image(&app), window);

        // Restoring it is a resize like any other.
        main_target_size(&mut app, main, UVec2::new(1600, 900));
        update(&mut app, 1);
        assert_eq!(target_image(&app), window, "the size it already has");
    }

    /// The target size for a main camera of a given resolution: followed exactly up to the ceiling,
    /// scaled by one factor past it, and `None` when the target has no size at all.
    #[test]
    fn the_target_size_follows_the_window_and_clamps_a_large_one() {
        let ceiling = PORTAL_TEXTURE_MAX_SIZE;
        let aspect = |size: UVec2| size.x as f32 / size.y as f32;
        assert_eq!(
            portal_target_size(UVec2::new(1600, 900), ceiling),
            Some(UVec2::new(1600, 900)),
            "the window's own size, one for one"
        );
        assert_eq!(
            portal_target_size(UVec2::new(1024, 576), ceiling),
            Some(UVec2::new(1024, 576)),
            "smaller than the ceiling is followed too, not scaled up"
        );
        assert_eq!(
            portal_target_size(ceiling, ceiling),
            Some(ceiling),
            "exactly the ceiling"
        );
        assert_eq!(
            portal_target_size(UVec2::new(3840, 2160), ceiling),
            Some(ceiling),
            "a 4K window is clamped to the ceiling"
        );
        assert_eq!(
            portal_target_size(UVec2::new(7680, 4320), ceiling),
            Some(ceiling),
            "and so is an 8K one"
        );
        assert_eq!(
            portal_target_size(UVec2::ZERO, ceiling),
            None,
            "a minimized window has no size to follow"
        );
        assert_eq!(
            portal_target_size(UVec2::new(1920, 0), ceiling),
            None,
            "nor has a target with one empty axis"
        );

        // A window the ceiling does not fit keeps its pixel aspect ratio: one factor for both axes.
        let ultrawide = portal_target_size(UVec2::new(3440, 1440), ceiling).unwrap();
        assert_eq!(ultrawide.x, ceiling.x, "the long axis is at the ceiling");
        assert!(ultrawide.y < ceiling.y);
        assert!(
            (aspect(ultrawide) - aspect(UVec2::new(3440, 1440))).abs() < 0.01,
            "an ultrawide doorway must not come out stretched: {ultrawide:?}"
        );
        let portrait = portal_target_size(UVec2::new(1080, 7680), ceiling).unwrap();
        assert_eq!(
            portrait.y, ceiling.y,
            "a tall window is capped on its long axis"
        );
        assert!(portrait.x <= ceiling.x);
        assert!(
            (aspect(portrait) - aspect(UVec2::new(1080, 7680))).abs() < 0.01,
            "{portrait:?}"
        );

        // The same answer every time: the system's "is this a change?" test is what keeps the
        // target from being rebuilt every frame, and it compares against this function's answer.
        assert_eq!(
            portal_target_size(UVec2::new(3440, 1440), ceiling),
            Some(ultrawide)
        );
    }

    /// The doorway image is not an 8-bit one. It holds the destination's scene-referred values -
    /// above white included - so the main camera's tonemapper gets the same range through the
    /// doorway that it gets when the player walks into the room, and a bright destination rolls off
    /// its curve instead of arriving flattened at white (note 2 of the brief).
    #[test]
    fn the_doorway_image_is_float_and_is_not_viewed_as_srgb() {
        let image = portal_target_image(UVec2::new(64, 32));
        assert_eq!(image.size(), UVec2::new(64, 32));
        assert_eq!(
            image.texture_descriptor.format, PORTAL_TEXTURE_FORMAT,
            "the target is what a camera's main pass writes into"
        );
        assert_eq!(
            PORTAL_TEXTURE_FORMAT,
            TextureFormat::Rgba16Float,
            "four half-float channels at 8 bytes a pixel: a value over 1.0 survives in the target \
             rather than clamping, and every target the engine runs on can render into it"
        );
        assert!(
            !PORTAL_TEXTURE_FORMAT.is_srgb(),
            "an sRGB target would hold an encoded copy of the values, which is the 8-bit problem"
        );
        assert!(
            image.texture_view_descriptor.is_none(),
            "no second view format: a float target must be neither encoded on write nor decoded on \
             sample, or the round trip is what the quad's sampling loses"
        );
    }

    /// One row of the door-visibility table: (`state`, `auto_load`, the portal is rendering through
    /// this door, it is holding a crossing, it is the **destination** door of the pair the portal is
    /// rendering through, its model is drawn, what the row is).
    type DoorCase = (
        Option<DoorState>,
        bool,
        bool,
        bool,
        bool,
        bool,
        &'static str,
    );

    /// The whole table of when a load door's model is drawn, run through the system that writes it.
    ///
    /// The row that regressed is in here: an **animated** door the portal is rendering through is
    /// **drawn**. A model hidden there takes the door's frame and its swinging leaf out of the
    /// doorway the window stands in, so the player sees an empty hole where the door they just
    /// opened is - and the doorway of the room they are in stops looking like a doorway.
    /// `an_animated_doors_model_is_not_the_portals_to_hide` is the same case read through the
    /// leaf's inherited visibility, which is what the renderer looks at.
    #[test]
    fn a_doors_model_is_drawn_unless_it_is_the_hole_the_doorway_needs() {
        use DoorState::{Closed, Closing, Open, Opening};
        let cases: [DoorCase; 17] = [
            (
                None,
                false,
                false,
                false,
                false,
                true,
                "a door whose state has not been written yet is the closed door it looks like",
            ),
            (
                Some(Closed),
                false,
                false,
                false,
                false,
                true,
                "a closed door draws its leaf",
            ),
            (
                Some(Closed),
                false,
                true,
                false,
                false,
                false,
                "a door the portal is somehow showing while closed has nothing to hide behind the \
                 window - the target is only ever written for a door the portal draws through \
                 (`portal_shows_through`), which a closed one is not",
            ),
            (
                Some(Opening),
                false,
                false,
                false,
                false,
                true,
                "a door mid-swing draws its leaf",
            ),
            (
                Some(Opening),
                false,
                true,
                false,
                false,
                true,
                "THE REGRESSED CASE: an animating door the portal renders through stays drawn",
            ),
            (
                Some(Closing),
                false,
                true,
                false,
                false,
                true,
                "a door closing behind the player is drawn coming back",
            ),
            (
                Some(Open { animated: true }),
                false,
                true,
                false,
                false,
                true,
                "an animated open door the portal renders through stays drawn",
            ),
            (
                Some(Open { animated: true }),
                false,
                false,
                false,
                false,
                true,
                "and one the portal is not rendering through is drawn too",
            ),
            (
                Some(Open { animated: false }),
                false,
                false,
                false,
                false,
                false,
                "a door with no clip has no leaf that can move: its model is the hole",
            ),
            (
                Some(Open { animated: false }),
                false,
                true,
                false,
                false,
                false,
                "including the door the portal is rendering through",
            ),
            (
                Some(Open { animated: false }),
                false,
                true,
                true,
                false,
                true,
                "unless its crossing is held: there is no window to be a hole in then",
            ),
            (
                Some(Closing),
                false,
                false,
                true,
                false,
                true,
                "and a held crossing draws whatever the state is",
            ),
            (
                Some(Open { animated: false }),
                true,
                true,
                false,
                false,
                false,
                "an auto-load marker is an invisible reference, portal or not",
            ),
            (
                Some(Open { animated: false }),
                true,
                true,
                true,
                false,
                true,
                "and it is drawn while its crossing is held, like any other door",
            ),
            (
                Some(Closed),
                false,
                false,
                false,
                true,
                false,
                "the DESTINATION door of the pair the portal renders through draws nothing while \
                 it is shut: its leaf stands in the window's own clip plane and covers the room",
            ),
            (
                Some(Open { animated: true }),
                false,
                false,
                false,
                true,
                true,
                "but swung open with the near door it draws its own leaf in the window (impl-234)",
            ),
            (
                Some(Open { animated: false }),
                false,
                false,
                false,
                true,
                false,
                "and one with no leaf of its own to swing is the hole it always was",
            ),
        ];

        let mut app = portal_app();
        let mut spawned = Vec::new();
        for (state, auto_load, _, waiting, _, _, _) in cases.iter() {
            let mut door = interior_door(0x0005_6C1B);
            door.auto_load = *auto_load;
            let (entity, mesh) = spawn_door(&mut app, door);
            if let Some(state) = state {
                app.world_mut().entity_mut(entity).insert(*state);
            }
            if *waiting {
                app.world_mut().entity_mut(entity).insert(CrossingHeld);
            }
            spawned.push((entity, mesh));
        }
        let wanted = |drawn: bool| {
            if drawn {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            }
        };

        // With no portal up, every row that is not the portal's to hide answers from its own state.
        portal_shows(&mut app, None);
        update(&mut app, 1);
        for ((_, _, portal, _, destination, drawn, what), (entity, mesh)) in
            cases.iter().zip(&spawned)
        {
            if *portal || *destination {
                continue;
            }
            assert_eq!(visibility_of(&app, *entity), wanted(*drawn), "{what}");
            assert_eq!(leaf_is_drawn(&app, *mesh), *drawn, "{what}");
        }

        // And then with each row holding the role it is about - the door the portal renders
        // through, the far end of that doorway, or neither.
        for ((_, _, portal, _, destination, drawn, what), (entity, mesh)) in
            cases.iter().zip(&spawned)
        {
            if !*portal && !*destination {
                continue;
            }
            portal_shows(&mut app, portal.then_some(*entity));
            app.world_mut()
                .resource_mut::<PortalState>()
                .destination_door = destination.then_some(*entity);
            update(&mut app, 1);
            assert_eq!(visibility_of(&app, *entity), wanted(*drawn), "{what}");
            assert_eq!(leaf_is_drawn(&app, *mesh), *drawn, "{what}");
        }
    }

    /// The **destination** door of the pair the portal is drawing through draws nothing while it
    /// stands shut behind the window, draws its own leaf once it swings (impl-234: the far-door
    /// drive opens it with the near door, and the leaf the window shows is the far door's own), and
    /// comes back as a door the frame the window goes.
    ///
    /// Under a doorway anchor the window's clip plane is the destination doorway's own plane
    /// (`DoorMap::destination_plane`), which is exactly where the destination door stands: drawn
    /// shut, its closed leaf fills the aperture and the room behind it is invisible. The door the
    /// portal renders *through* keeps its own model, frame and swing.
    #[test]
    fn the_destination_door_of_the_pair_draws_its_leaf_only_while_it_swings() {
        let mut app = portal_app();
        let (source, source_mesh) = spawn_door(&mut app, interior_door(0x0005_6C1B));
        let (destination, destination_mesh) = spawn_door(&mut app, interior_door(0x0001_52C3));
        app.world_mut()
            .entity_mut(source)
            .insert(DoorState::Opening);
        app.world_mut()
            .entity_mut(destination)
            .insert(DoorState::Closed);

        // No portal: both doors are their own business, and the closed one draws its leaf.
        update(&mut app, 1);
        assert!(leaf_is_drawn(&app, source_mesh));
        assert!(leaf_is_drawn(&app, destination_mesh));

        // With the window up through the source door, its own model stays - and the far end of the
        // same doorway is not drawn into it.
        portal_shows(&mut app, Some(source));
        app.world_mut()
            .resource_mut::<PortalState>()
            .destination_door = Some(destination);
        update(&mut app, 1);
        assert!(
            leaf_is_drawn(&app, source_mesh),
            "the door the portal renders through is an animated one: its frame is the doorway"
        );
        assert!(
            !leaf_is_drawn(&app, destination_mesh),
            "the destination door's shut leaf stands in the window's own clip plane"
        );

        // The far-door drive swings it open with the near door: the window shows its own leaf.
        app.world_mut()
            .entity_mut(destination)
            .insert(DoorState::Opening);
        update(&mut app, 1);
        assert!(
            leaf_is_drawn(&app, destination_mesh),
            "the far door's own leaf is the one the window shows"
        );

        // The portal drops the pair: the destination door is a door again in the same frame.
        portal_shows(&mut app, None);
        app.world_mut()
            .resource_mut::<PortalState>()
            .destination_door = None;
        update(&mut app, 1);
        assert!(leaf_is_drawn(&app, destination_mesh));
    }

    // -----------------------------------------------------------------------------------------
    // The door layer: a near load-door pair drawn once (`crate::door_layer`)
    // -----------------------------------------------------------------------------------------

    /// The scene a door's model spawns, as far as the door layer is concerned: the root the glTF loader
    /// makes for a scene, the node the door's `Open` clip moves - the leaf - with its mesh under it,
    /// and the model's static filler beside it: the game's black plug across the doorway
    /// (`DoorBlack` in `FarmhouseLDoor01`, `Plane02` in the Dwemer large load door).
    ///
    /// Every *node* carries the [`AnimationTargetId`] the loader hashes from its name path and every
    /// *mesh* carries none, which is exactly the distinction the door layer's walk draws between a node
    /// of the model and a mesh under one. The names are the loader's own for the Riverwood doors.
    struct DoorScene {
        /// The node the door's clips move.
        leaf: Entity,
        /// The leaf's own mesh.
        leaf_mesh: Entity,
        /// The static filler's mesh: the plug in the doorway.
        plug_mesh: Entity,
    }

    /// Whether an entity is drawn, through the inheritance the renderer itself uses.
    fn is_drawn(app: &App, entity: Entity) -> bool {
        app.world()
            .entity(entity)
            .get::<InheritedVisibility>()
            .expect("a visible entity")
            .get()
    }

    /// The id the loader hashes for a node, from its name path.
    fn scene_target(path: &[&str]) -> bevy::animation::AnimationTargetId {
        bevy::animation::AnimationTargetId::from_iter(path.iter().copied())
    }

    /// Spawns a model's scene under `parent`, with the loader's own names, ids and rest pose: the
    /// leaf hinged 88 units along the doorway's own plane (where `FarmhouseLDoor01` hinges it) with
    /// its mesh 96 units out along the leaf - the far edge of the door, which a swing moves.
    fn spawn_scene(app: &mut App, parent: Entity) -> DoorScene {
        let node = |app: &mut App, path: &[&str], parent: Entity, transform: Transform| {
            app.world_mut()
                .spawn((
                    Name::new(path[path.len() - 1].to_string()),
                    scene_target(path),
                    transform,
                    Visibility::default(),
                    ChildOf(parent),
                ))
                .id()
        };
        let root = node(
            app,
            &["Creation-to-glTF basis"],
            parent,
            Transform::default(),
        );
        let model = node(
            app,
            &["Creation-to-glTF basis", "FarmhouseLDoor01"],
            root,
            Transform::default(),
        );
        let leaf = node(
            app,
            &["Creation-to-glTF basis", "FarmhouseLDoor01", "Door"],
            model,
            Transform::from_xyz(48.0, 4.0, 88.0),
        );
        // The leaf's own mesh, out at the far edge of the door: a node's transform that a swing
        // carries a long way, which is what makes the tests' comparisons bite. `spawn_scene` gives
        // it no id, as the loader does not, so its own pose is the model's rest pose in both
        // instances and the parent's swing is what moves it.
        let leaf_mesh = app
            .world_mut()
            .spawn((
                Mesh3d(Handle::default()),
                Transform::from_xyz(-96.0, 0.0, 0.0),
                Visibility::default(),
                ChildOf(leaf),
            ))
            .id();
        let plug = node(
            app,
            &["Creation-to-glTF basis", "FarmhouseLDoor01", "DoorBlack"],
            model,
            Transform::from_xyz(0.0, 0.0, -18.0),
        );
        let plug_mesh = app
            .world_mut()
            .spawn((Mesh3d(Handle::default()), ChildOf(plug)))
            .id();
        DoorScene {
            leaf,
            leaf_mesh,
            plug_mesh,
        }
    }

    /// A load door with a model's scene on it and a state its own animation moves, holding the
    /// scene handle `streaming::spawn_cell` would have loaded for it.
    fn spawn_scened_door(app: &mut App, door: LoadDoor, state: DoorState) -> (Entity, DoorScene) {
        app.init_asset::<WorldAsset>();
        let (entity, _) = spawn_door(app, door);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldAsset>>()
            .add(WorldAsset::new(World::default()));
        app.world_mut()
            .entity_mut(entity)
            .insert((state, WorldAssetRoot(handle)));
        let scene = spawn_scene(app, entity);
        (entity, scene)
    }

    /// The ref id of the twin a [`door_layer_pair`]'s driver leads to.
    const TWIN_REF: u32 = 0x5704B;
    /// The interior the twin stands in.
    const TWIN_CELL: u32 = 0x0001_6000;

    /// A [`portal_app`] with the door layer's two systems where `PortalPlugin` puts them: the pairs
    /// managed in the portal's frame before the isolation that hides their natives, and the copies
    /// posed in `PostUpdate`, before the transforms propagate.
    fn door_layer_app() -> App {
        let mut app = portal_app();
        app.init_asset::<WorldAsset>()
            .init_resource::<crate::door_layer::DoorLayer>()
            .add_systems(
                Update,
                crate::door_layer::manage_door_layer.before(show_load_door_leaves),
            )
            .add_systems(
                PostUpdate,
                crate::door_layer::pose_door_copies.before(TransformSystems::Propagate),
            );
        app
    }

    /// One animated door of the player's interior, leading into [`TWIN_CELL`], and its twin
    /// there leading back, each in a cell root of its own with the same model's scene under it: a
    /// load-door pair.
    struct LayerPairCase {
        driver: Entity,
        driver_scene: DoorScene,
        twin: Entity,
        twin_scene: DoorScene,
    }

    fn door_layer_pair(app: &mut App) -> LayerPairCase {
        let (home, _) = spawn_cell(app, INTERIOR_ALFTAND01, None, None, None);
        let (away, _) = spawn_cell(app, TWIN_CELL, None, None, None);
        let driver_row = LoadDoor {
            destination: DoorDestination {
                destination_ref_id: TWIN_REF,
                ..interior_door(TWIN_CELL).destination
            },
            ..interior_door(TWIN_CELL)
        };
        let twin_row = LoadDoor {
            ref_id: TWIN_REF,
            destination: DoorDestination {
                destination_ref_id: driver_row.ref_id,
                ..interior_door(INTERIOR_ALFTAND01).destination
            },
            ..interior_door(INTERIOR_ALFTAND01)
        };
        let (driver, driver_scene) = spawn_scened_door(app, driver_row, DoorState::Closed);
        let (twin, twin_scene) = spawn_scened_door(app, twin_row, DoorState::Closed);
        let driver_pose = Transform::from_xyz(0.0, 0.0, -300.0);
        let twin_pose = Transform::from_xyz(2000.0, 50.0, 400.0)
            .with_rotation(Quat::from_rotation_y(1.0))
            .with_scale(Vec3::splat(1.1));
        for (door, parent, pose, scene) in [
            (driver, home, driver_pose, &driver_scene),
            (twin, away, twin_pose, &twin_scene),
        ] {
            app.world_mut().entity_mut(door).insert((
                ChildOf(parent),
                pose,
                crate::door_animation::DoorAnimation::swinging_for_test(),
            ));
            app.world_mut()
                .entity_mut(scene.leaf)
                .insert(DoorLeaf { door });
        }
        LayerPairCase {
            driver,
            driver_scene,
            twin,
            twin_scene,
        }
    }

    /// Every door-layer copy standing, with what it is.
    fn layer_copies(app: &mut App) -> Vec<(Entity, crate::door_layer::DoorCopy)> {
        let mut query = app
            .world_mut()
            .query::<(Entity, &crate::door_layer::DoorCopy)>();
        query
            .iter(app.world())
            .map(|(entity, copy)| (entity, *copy))
            .collect()
    }

    /// Runs the frames that spawn a door-layer door for `case.driver` and gives it the instance
    /// of the model the world asset spawner would (nothing in a test runs the spawner); returns the
    /// copy and its scene once it has drawn and its natives are hidden.
    fn door_layer_door(app: &mut App, case: &LayerPairCase) -> (Entity, DoorScene) {
        update(app, 1);
        let copies = layer_copies(app);
        assert_eq!(copies.len(), 1, "one door-layer door for the pair");
        let (copy, spawned) = copies[0];
        assert_eq!(spawned.driver, case.driver);
        assert_eq!(spawned.kind, crate::door_layer::CopyKind::Main);
        let scene = spawn_scene(app, copy);
        // The walk draws it, the next frame publishes it as drawn, the isolation hides the
        // natives, and their commands apply.
        update(app, 3);
        (copy, scene)
    }

    fn move_camera(app: &mut App, camera: Entity, transform: Transform) {
        app.world_mut().entity_mut(camera).insert(transform);
    }

    /// The crossing of a [`LayerPairCase`]: the twin's interior becomes the player's space, with the
    /// player standing just in front of the twin, looking at it.
    fn cross_to_twin(app: &mut App, camera: Entity, case: &LayerPairCase) {
        let twin_at = app
            .world()
            .entity(case.twin)
            .get::<Transform>()
            .unwrap()
            .translation;
        move_camera(
            app,
            camera,
            Transform::from_translation(twin_at + Vec3::new(0.0, 120.0, 150.0))
                .looking_at(twin_at, Vec3::Y),
        );
        app.world_mut().resource_mut::<ActiveCell>().interior = Some(TWIN_CELL);
        update(app, 2);
    }

    /// The design's first two rules: a near door is drawn once, by a door-layer door made from the
    /// door on the player's side, on the door layer and posed from that door's own nodes; and
    /// once it has drawn, **both** natives - the player's side and its twin in the other space -
    /// are off every camera, while they stay where they are for the ray casts. The frame the pair
    /// leaves the layer, the player's native is drawn again.
    #[test]
    fn a_near_door_pair_is_drawn_once_by_the_door_layer_with_both_natives_hidden() {
        let mut app = door_layer_app();
        let camera = spawn_camera(&mut app, Vec3::new(0.0, 120.0, 0.0));
        let case = door_layer_pair(&mut app);
        // A swing on the driver's leaf, where its animation writes it.
        let swing = Transform::from_xyz(48.0, 4.0, 88.0).with_rotation(Quat::from_rotation_y(1.2));
        app.world_mut()
            .entity_mut(case.driver_scene.leaf)
            .insert(swing);
        let (copy, scene) = door_layer_door(&mut app, &case);

        // The copy is the driver's model on the door layer, node for node in its pose.
        let layer = crate::door_layer::door_layers();
        assert_eq!(layers_of(&app, scene.leaf_mesh), layer);
        assert_eq!(layers_of(&app, scene.plug_mesh), layer, "the whole model");
        assert!(
            !layers_of(&app, scene.leaf_mesh).intersects(&RenderLayers::layer(DESTINATION_LAYER)),
            "never in the doorway image"
        );
        assert_eq!(
            *app.world().entity(scene.leaf).get::<Transform>().unwrap(),
            swing
        );
        assert_eq!(
            *app.world().entity(copy).get::<Transform>().unwrap(),
            Transform::from_xyz(0.0, 0.0, -300.0),
            "standing on its driver"
        );
        assert!(
            app.world()
                .entity(scene.leaf_mesh)
                .contains::<crate::door_layer::DoorCopyPart>()
        );

        // Both natives are off the cameras, and still there for the ray casts.
        for mesh in [
            case.driver_scene.leaf_mesh,
            case.driver_scene.plug_mesh,
            case.twin_scene.leaf_mesh,
        ] {
            assert_eq!(layers_of(&app, mesh), RenderLayers::none());
        }
        assert!(is_drawn(&app, case.driver_scene.leaf_mesh));
        let layer_state = app.world().resource::<crate::door_layer::DoorLayer>();
        assert_eq!(layer_state.pairs().len(), 1);
        assert_eq!(layer_state.pairs()[0].follower, Some(case.twin));

        // Walked away from: the pair leaves the layer and the player's native is drawn again.
        move_camera(
            &mut app,
            camera,
            Transform::from_translation(Vec3::new(0.0, 120.0, 5000.0)),
        );
        update(&mut app, 3);
        assert!(layer_copies(&mut app).is_empty());
        assert!(on_main_camera(&app, case.driver_scene.leaf_mesh));
    }

    /// The design's fourth rule: the crossing keeps the same door. The swap re-expresses the
    /// door-layer door in the new space through the driver's own doorway map - the map the crossing
    /// moves the player by - scaled to the side it now stands in; nothing is spawned or dropped.
    #[test]
    fn a_crossing_keeps_the_door_layer_door_and_carries_it_through_the_doorway_map() {
        let mut app = door_layer_app();
        let camera = spawn_camera(&mut app, Vec3::new(0.0, 120.0, 0.0));
        let case = door_layer_pair(&mut app);
        let (copy, _) = door_layer_door(&mut app, &case);

        // The crossing: the twin's interior is the player's space now, and the player stands at
        // its doorway looking back at it.
        cross_to_twin(&mut app, camera, &case);
        let copies = layer_copies(&mut app);
        assert_eq!(copies.len(), 1, "no second door for the pair");
        assert_eq!(copies[0].0, copy, "the same door-layer door");
        assert_eq!(copies[0].1.driver, case.driver, "still the model it was");

        let driver = *app.world().entity(case.driver).get::<Transform>().unwrap();
        let row = app
            .world()
            .entity(case.driver)
            .get::<LoadDoor>()
            .unwrap()
            .clone();
        let map = door_map(
            driver.translation,
            driver.rotation,
            driver.scale,
            &row,
            None,
            IVec2::new(19, 18),
        );
        let expected = crate::door_layer::door_copy_pose(driver, Some(&map), 1.1);
        let placed = *app.world().entity(copy).get::<Transform>().unwrap();
        assert!((placed.translation - expected.translation).length() < 1e-3);
        assert!(placed.rotation.angle_between(expected.rotation) < 1e-5);
        assert!((placed.scale - Vec3::splat(1.1)).length() < 1e-5);
        // Both natives are still off the cameras: the twin is the player's now, and still hidden.
        assert_eq!(
            layers_of(&app, case.twin_scene.leaf_mesh),
            RenderLayers::none()
        );
        assert_eq!(
            layers_of(&app, case.driver_scene.leaf_mesh),
            RenderLayers::none()
        );
    }

    /// The design's fourth rule, second half: the crossed door changes to the model of the
    /// player's side only when nobody can see it - not while a door of the pair is open, not while
    /// the player stands by it looking at it - and then the twin drives the next one.
    #[test]
    fn a_crossed_door_takes_the_players_side_model_only_when_the_switch_is_unseen() {
        let mut app = door_layer_app();
        let camera = spawn_camera(&mut app, Vec3::new(0.0, 120.0, 0.0));
        let case = door_layer_pair(&mut app);
        let (copy, _) = door_layer_door(&mut app, &case);
        cross_to_twin(&mut app, camera, &case);
        let twin_at = app
            .world()
            .entity(case.twin)
            .get::<GlobalTransform>()
            .unwrap()
            .translation();
        let looking_at = |eye: Vec3| Transform::from_translation(eye).looking_at(twin_at, Vec3::Y);

        // Right by it and looking at it, both shut: kept.
        move_camera(
            &mut app,
            camera,
            looking_at(twin_at + Vec3::new(0.0, 120.0, 150.0)),
        );
        update(&mut app, 2);
        assert_eq!(layer_copies(&mut app)[0].0, copy);

        // Far enough, but the door is open: kept.
        app.world_mut()
            .entity_mut(case.driver)
            .insert(DoorState::Open { animated: true });
        move_camera(
            &mut app,
            camera,
            looking_at(twin_at + Vec3::new(0.0, 120.0, 500.0)),
        );
        update(&mut app, 2);
        assert_eq!(layer_copies(&mut app)[0].0, copy);

        // Shut, a few metres away: handed back. The next door-layer door is the twin's.
        app.world_mut()
            .entity_mut(case.driver)
            .insert(DoorState::Closed);
        update(&mut app, 2);
        let copies = layer_copies(&mut app);
        assert_eq!(copies.len(), 1);
        assert_ne!(copies[0].0, copy, "a new door-layer door");
        assert_eq!(copies[0].1.driver, case.twin, "from the player's side");
    }

    /// A door with no swing of its own - no clip to pose a copy from - stays on its native drawing.
    #[test]
    fn a_door_with_no_swing_is_not_put_on_the_door_layer() {
        let mut app = door_layer_app();
        spawn_camera(&mut app, Vec3::new(0.0, 120.0, 0.0));
        let case = door_layer_pair(&mut app);
        app.world_mut()
            .entity_mut(case.driver)
            .insert(crate::door_animation::DoorAnimation::default());
        update(&mut app, 3);
        assert!(layer_copies(&mut app).is_empty());
        assert!(on_main_camera(&app, case.driver_scene.leaf_mesh));
    }

    // -----------------------------------------------------------------------------------------
    // The doorway's own sun: the destination's space lighting the destination's image
    // -----------------------------------------------------------------------------------------

    /// An app with the two cameras and the two suns: the portal camera and its own sun from
    /// [`setup_portal_camera`], the main camera on the layers `app::setup_world` gives it, and the
    /// engine's one sun - which, spawned as `app.rs` spawns it, carries no `RenderLayers` at all.
    struct SunApp {
        app: App,
        main_camera: Entity,
        engine_sun: Entity,
        portal_camera: Entity,
        destination_sun: Entity,
    }

    /// The engine sun's own numbers, distinct from any space's, so a test can see who wrote what.
    const ENGINE_SUN_COLOR: Color = Color::srgb(1.0, 0.25, 0.0);
    const ENGINE_SUN_ILLUMINANCE: f32 = 1234.0;

    fn sun_app() -> SunApp {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_resource::<PortalState>()
            .insert_resource(EngineConfig {
                worldspace_id: TAMRIEL,
                stream_radius: 1,
                unload_radius: 1,
                start_grid: (0, 0),
                ..EngineConfig::default()
            })
            .insert_resource(RenderOrigin(IVec2::new(19, 18)))
            .add_systems(Startup, setup_portal_camera)
            .add_systems(
                Update,
                (place_destination_sun, update_destination_atmosphere).chain(),
            );
        update(&mut app, 1);

        let main_camera = app
            .world_mut()
            .spawn((
                StreamingCamera,
                Camera::default(),
                RenderLayers::from_layers(&MAIN_CAMERA_LAYERS),
                Transform::default(),
            ))
            .id();
        let engine_sun = app
            .world_mut()
            .spawn((
                Name::new("Engine sun"),
                DirectionalLight {
                    color: ENGINE_SUN_COLOR,
                    illuminance: ENGINE_SUN_ILLUMINANCE,
                    shadow_maps_enabled: true,
                    ..default()
                },
                CascadeShadowConfig::default(),
                Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, -0.5, 0.0)),
            ))
            .id();
        let portal_camera = entity_with::<PortalCamera>(&mut app).expect("the portal camera");
        let destination_sun =
            entity_with::<PortalDestinationSun>(&mut app).expect("the doorway's sun");
        SunApp {
            app,
            main_camera,
            engine_sun,
            portal_camera,
            destination_sun,
        }
    }

    /// The first entity carrying a component.
    fn entity_with<T: Component>(app: &mut App) -> Option<Entity> {
        let mut query = app.world_mut().query_filtered::<Entity, With<T>>();
        query.iter(app.world()).next()
    }

    /// A door whose destination is a space of its own, and the space's key.
    fn door_to_space(worldspace_id: u32, interior: Option<u32>) -> LoadDoor {
        LoadDoor {
            destination: DoorDestination {
                destination_ref_id: 0x699E8,
                interior_cell_id: interior,
                worldspace_id: interior.is_none().then_some(worldspace_id),
                arrival_position: [3693.815, 3074.645, 290.530],
                arrival_rotation: [0.0, 0.0, -1.83260],
            },
            label: "a space".into(),
            auto_load: false,
            outward: None,
            ..interior_door(INTERIOR_ALFTAND01)
        }
    }

    /// Each sun keeps the cascades of the views that draw it and loses the rest (impl-226): the
    /// doorway's sun (layer 2) none of the main camera's (layers 0 and 1) or the water reflection's
    /// (no layers: layer 0), the engine's sun (no layers) none of the portal camera's. With the
    /// portal camera off Bevy builds it no cascades at all, so the doorway's sun is left with none
    /// and culls nothing - the open-door-behind-the-player case.
    #[test]
    fn a_sun_keeps_only_the_cascades_of_the_views_that_draw_it() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let main = world
            .spawn((
                Camera::default(),
                RenderLayers::from_layers(&MAIN_CAMERA_LAYERS),
            ))
            .id();
        let water = world.spawn(Camera::default()).id();
        let portal = world
            .spawn((Camera::default(), RenderLayers::layer(DESTINATION_LAYER)))
            .id();
        let cascades_for = |views: &[Entity]| Cascades {
            cascades: views.iter().map(|view| (*view, Vec::new())).collect(),
        };
        let engine_sun = world
            .spawn((
                DirectionalLight::default(),
                cascades_for(&[main, water, portal]),
            ))
            .id();
        let doorway_sun = world
            .spawn((
                DirectionalLight::default(),
                cascades_for(&[main, water, portal]),
                RenderLayers::layer(DESTINATION_LAYER),
            ))
            .id();
        let doorway_sun_off = world
            .spawn((
                DirectionalLight::default(),
                cascades_for(&[main, water]),
                RenderLayers::layer(DESTINATION_LAYER),
            ))
            .id();
        world
            .run_system_once(keep_cascades_of_views_that_draw_their_light)
            .unwrap();
        let views = |world: &World, light: Entity| {
            let mut views: Vec<Entity> = world
                .get::<Cascades>(light)
                .unwrap()
                .cascades
                .keys()
                .copied()
                .collect();
            views.sort();
            views
        };
        let mut expected = vec![main, water];
        expected.sort();
        assert_eq!(views(&world, engine_sun), expected);
        assert_eq!(views(&world, doorway_sun), vec![portal]);
        assert!(views(&world, doorway_sun_off).is_empty());
    }

    /// The portal camera is lit by the sun of its own layer and by no other light, and the main
    /// camera by the engine's - which is the whole mechanism: Bevy builds a view's directional
    /// lights by intersecting the light's layers with the *camera's*, and before the doorway had a
    /// sun of its own the engine's one (no layers at all, so layer 0) reached neither the portal
    /// camera nor, therefore, the doorway image.
    #[test]
    fn the_doorway_is_lit_by_a_sun_of_its_own_layer() {
        let SunApp {
            app,
            main_camera,
            engine_sun,
            portal_camera,
            destination_sun,
        } = sun_app();

        assert!(
            layers_of(&app, portal_camera).intersects(&layers_of(&app, destination_sun)),
            "the portal camera renders the layer the doorway's sun is on"
        );
        assert!(
            !layers_of(&app, main_camera).intersects(&layers_of(&app, destination_sun)),
            "and the main camera does not: the doorway's sun must not light the room around it"
        );
        assert!(
            !layers_of(&app, engine_sun).intersects(&layers_of(&app, portal_camera)),
            "the engine's sun carries no layers - layer 0 - so the portal camera never saw it, \
             which is the state this test is here to keep from coming back"
        );
        assert!(
            layers_of(&app, engine_sun).intersects(&layers_of(&app, main_camera)),
            "and the main camera keeps it"
        );
    }

    /// The doorway's sun takes the engine sun's direction, casts shadows while it gives light, and keeps
    /// shadow cascades of its own - the engine sun's count, over the near distance a doorway shows -
    /// rather than a copy of the engine sun's: one sun in the world, drawn in two views.
    #[test]
    fn the_doorway_sun_points_where_the_engines_does() {
        let SunApp {
            mut app,
            engine_sun,
            destination_sun,
            ..
        } = sun_app();
        update(&mut app, 1);

        let engine = *app.world().entity(engine_sun).get::<Transform>().unwrap();
        let destination = *app
            .world()
            .entity(destination_sun)
            .get::<Transform>()
            .unwrap();
        assert_eq!(destination.rotation, engine.rotation);
        // Shadows follow the doorway sun's own light: none while it gives none (an interior's
        // destination, research-228), and on once it lights the destination.
        let shadows = |app: &App| {
            let light = app
                .world()
                .entity(destination_sun)
                .get::<DirectionalLight>()
                .unwrap();
            (light.illuminance > 0.0, light.shadow_maps_enabled)
        };
        let (lit, cast) = shadows(&app);
        assert_eq!(cast, lit);
        app.world_mut()
            .entity_mut(destination_sun)
            .get_mut::<DirectionalLight>()
            .unwrap()
            .illuminance = 5_000.0;
        update(&mut app, 1);
        assert_eq!(shadows(&app), (true, true));
        let cascades = |app: &App, entity: Entity| {
            let cascades = app
                .world()
                .entity(entity)
                .get::<CascadeShadowConfig>()
                .unwrap();
            (
                cascades.bounds.clone(),
                cascades.overlap_proportion,
                cascades.minimum_distance,
            )
        };
        // The cascades are the doorway's own, not a copy of the engine sun's: as many (Bevy cannot
        // draw two shadowed directional lights with different counts), over the near distance a
        // doorway shows, and the engine sun's set is left as it was.
        let engine_default = CascadeShadowConfig::default();
        let (bounds, overlap, minimum) = cascades(&app, destination_sun);
        assert_eq!(bounds.len(), engine_default.bounds.len());
        assert_eq!(bounds.first(), Some(&PORTAL_SUN_FIRST_CASCADE));
        assert!((bounds.last().unwrap() - PORTAL_SUN_SHADOW_DISTANCE).abs() < 0.01);
        assert_eq!(overlap, PORTAL_SUN_CASCADE_OVERLAP);
        assert_eq!(minimum, PORTAL_SUN_SHADOW_NEAR);
        assert_eq!(
            cascades(&app, engine_sun),
            (
                engine_default.bounds.clone(),
                engine_default.overlap_proportion,
                engine_default.minimum_distance,
            ),
            "the engine sun's cascades are its own and untouched"
        );

        // An engine sun with another count takes the doorway's with it, over the doorway's reach.
        app.world_mut()
            .entity_mut(engine_sun)
            .insert(CascadeShadowConfig::from(CascadeShadowConfigBuilder {
                num_cascades: 2,
                ..default()
            }));
        update(&mut app, 1);
        let (bounds, ..) = cascades(&app, destination_sun);
        assert_eq!(bounds.len(), 2);
        assert!((bounds.last().unwrap() - PORTAL_SUN_SHADOW_DISTANCE).abs() < 0.01);

        // Moving the engine sun moves the doorway's with it, and the engine's own is left as it
        // was found.
        let turned = Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.4, -0.9, 0.0));
        app.world_mut().entity_mut(engine_sun).insert(turned);
        update(&mut app, 1);
        assert_eq!(
            app.world()
                .entity(destination_sun)
                .get::<Transform>()
                .unwrap()
                .rotation,
            turned.rotation
        );
        assert_eq!(
            app.world()
                .entity(engine_sun)
                .get::<Transform>()
                .unwrap()
                .rotation,
            turned.rotation
        );
    }

    /// The doorway image is lit by the *destination's* sun: the record's tint and illuminance in
    /// the doorway's own light, with the engine's sun left exactly as the space the player stands
    /// in left it.
    #[test]
    fn the_doorway_sun_carries_the_destinations_own_sun() {
        use crate::atmosphere::space_atmosphere;
        use crate::world::lighting::fixtures::{BLACKREACH, real_spaces};
        let (_directory, catalog) = real_spaces();
        let tamriel = space_atmosphere(Some(&catalog), space_key(TAMRIEL, None));
        let blackreach = space_atmosphere(Some(&catalog), space_key(BLACKREACH, None));
        assert!(
            tamriel.sun.illuminance > 0.0 && blackreach.sun.illuminance == 0.0,
            "the fixture's day and cave are the two cases this test is about"
        );

        let SunApp {
            mut app,
            engine_sun,
            destination_sun,
            ..
        } = sun_app();
        app.insert_resource(catalog);
        let day = door_to_space(TAMRIEL, None);
        let cave = door_to_space(BLACKREACH, None);
        let (day_door, _) = spawn_scened_door(&mut app, day, DoorState::Opening);
        let (cave_door, _) = spawn_scened_door(&mut app, cave, DoorState::Opening);

        portal_shows(&mut app, Some(day_door));
        update(&mut app, 2);
        let light = |app: &App, entity: Entity| {
            let light = app
                .world()
                .entity(entity)
                .get::<DirectionalLight>()
                .unwrap();
            (light.color, light.illuminance)
        };
        assert_eq!(
            light(&app, destination_sun),
            (tamriel.sun.color, tamriel.sun.illuminance),
            "the doorway is lit by the daylight the destination's own weather publishes"
        );
        assert_eq!(
            light(&app, engine_sun),
            (ENGINE_SUN_COLOR, ENGINE_SUN_ILLUMINANCE),
            "and the engine's sun is not the doorway's to write"
        );

        // A cave for a destination: the doorway's sun goes out, the same way the space's own sun
        // does when the player walks in.
        portal_shows(&mut app, Some(cave_door));
        update(&mut app, 2);
        assert_eq!(
            light(&app, destination_sun),
            (blackreach.sun.color, blackreach.sun.illuminance)
        );
        assert_eq!(
            light(&app, engine_sun),
            (ENGINE_SUN_COLOR, ENGINE_SUN_ILLUMINANCE)
        );

        // `crate::atmosphere::update_atmosphere` writes every `DirectionalLight` in the world when
        // the space the player stands in changes - this one included, since it cannot know whose it
        // is. That write is repaired rather than left standing: the doorway keeps the destination's
        // sun.
        app.world_mut()
            .entity_mut(destination_sun)
            .insert(DirectionalLight {
                color: ENGINE_SUN_COLOR,
                illuminance: ENGINE_SUN_ILLUMINANCE,
                ..default()
            });
        update(&mut app, 1);
        assert_eq!(
            light(&app, destination_sun),
            (blackreach.sun.color, blackreach.sun.illuminance),
            "the doorway's sun is written whenever it is not the destination's, not only on a \
             change of destination"
        );
    }

    /// The doorway's own sky dome is coloured from the destination it shows, not from the space the
    /// player stands in: the portal camera's `CameraSky` follows the door it looks through, and a
    /// door into a space with no sky clears it (review-277).
    #[test]
    fn the_doorway_dome_takes_the_destinations_sky() {
        use crate::atmosphere::space_sky;
        use crate::sky_dome::CameraSky;
        use crate::world::lighting::fixtures::{BLACKREACH, real_spaces};
        let (_directory, catalog) = real_spaces();
        let tamriel = space_sky(Some(&catalog), space_key(TAMRIEL, None));
        let blackreach = space_sky(Some(&catalog), space_key(BLACKREACH, None));
        assert!(tamriel.is_some() && blackreach.is_some() && tamriel != blackreach);

        let SunApp {
            mut app,
            portal_camera,
            ..
        } = sun_app();
        app.insert_resource(catalog);
        let sky = |app: &App| {
            app.world()
                .entity(portal_camera)
                .get::<CameraSky>()
                .expect("the portal camera carries its dome's colours")
                .palette
        };
        assert_eq!(sky(&app), None, "no doorway open, no sky drawn");

        let day = door_to_space(TAMRIEL, None);
        let cave = door_to_space(BLACKREACH, None);
        let room = door_to_space(TAMRIEL, Some(INTERIOR_ALFTAND01));
        let (day_door, _) = spawn_scened_door(&mut app, day, DoorState::Opening);
        let (cave_door, _) = spawn_scened_door(&mut app, cave, DoorState::Opening);
        let (room_door, _) = spawn_scened_door(&mut app, room, DoorState::Opening);

        portal_shows(&mut app, Some(day_door));
        update(&mut app, 2);
        assert_eq!(sky(&app), tamriel);
        portal_shows(&mut app, Some(cave_door));
        update(&mut app, 2);
        assert_eq!(sky(&app), blackreach);
        portal_shows(&mut app, Some(room_door));
        update(&mut app, 2);
        assert_eq!(sky(&app), None, "an interior behind the door has no sky");
    }

    // -----------------------------------------------------------------------------------------
    // The player controller in a world with a destination: only the active space collides and has
    // usable doors (impl-166). The findings these come from were written from the outside against
    // the code, so each of them is reproduced here through the systems a real frame runs, in the
    // order it runs them.
    // -----------------------------------------------------------------------------------------

    /// The player's own systems, in `PlayerPlugin`'s order, over a world with the real cell
    /// isolation: [`portal_app`]'s cells, classified by `isolate_cells` *behind* the controller -
    /// which is the order `PortalPlugin` runs it in, after `PlayerPlugin`. One frame is therefore
    /// exactly a real frame's, down to the role a step reads being the one the previous frame's
    /// isolation wrote: the space the camera is standing in now.
    fn player_app() -> App {
        let mut app = portal_app();
        app.insert_resource(ButtonInput::<KeyCode>::default())
            .init_resource::<ProfilingState>()
            .init_resource::<CameraSteps>()
            .init_resource::<Crossings>()
            .init_resource::<Opened>()
            .add_message::<CrossDoor>()
            .add_message::<OpenDoor>()
            .add_message::<DoorCrossed>()
            .add_systems(
                Update,
                (
                    advance_camera,
                    player_walk,
                    player_door,
                    player_auto_doors,
                    player_walks_through_doors,
                    collect_crossings,
                    collect_opened,
                )
                    .chain()
                    .before(isolate_cells),
            );
        app
    }

    /// Where the test camera is put this frame: one eye position per frame, written the way
    /// [`player_walk`] writes it - the `Transform` alone, with bevy's propagation filling in the
    /// `GlobalTransform` in `PostUpdate`, a frame later.
    #[derive(Resource, Default)]
    struct CameraSteps(VecDeque<Vec3>);

    /// The doors the controller asked to cross, in order.
    #[derive(Resource, Default)]
    struct Crossings(Vec<Entity>);

    /// The doors `E` was pressed on, in order.
    #[derive(Resource, Default)]
    struct Opened(Vec<Entity>);

    fn advance_camera(
        mut steps: ResMut<CameraSteps>,
        mut camera: Query<&mut Transform, With<StreamingCamera>>,
    ) {
        let Ok(mut transform) = camera.single_mut() else {
            return;
        };
        if let Some(eye) = steps.0.pop_front() {
            transform.translation = eye;
        }
    }

    fn collect_crossings(mut crossings: ResMut<Crossings>, mut requests: MessageReader<CrossDoor>) {
        for request in requests.read() {
            crossings.0.push(request.door);
        }
    }

    fn collect_opened(mut opened: ResMut<Opened>, mut requests: MessageReader<OpenDoor>) {
        for request in requests.read() {
            opened.0.push(request.door);
        }
    }

    /// Walks the test camera along a path, one eye position per frame.
    fn walk_camera(app: &mut App, eye_positions: impl IntoIterator<Item = Vec3>) {
        for eye in eye_positions {
            app.world_mut()
                .resource_mut::<CameraSteps>()
                .0
                .push_back(eye);
            app.update();
        }
    }

    /// An interior cell root the isolation can classify, with nothing in it.
    fn spawn_cell_root(app: &mut App, cell_id: u32) -> Entity {
        app.world_mut()
            .spawn((
                StreamedCellRoot,
                CellRef(cell_id),
                StreamedCellKey(CellKey::Interior(cell_id)),
                Transform::default(),
                Visibility::default(),
            ))
            .id()
    }

    /// An interior cell root with a real floor in it, `y` units up in the render space every cell
    /// shares: something for the walking probe to ray-cast.
    fn spawn_floor_cell(app: &mut App, cell_id: u32, y: f32) -> Entity {
        let root = spawn_cell_root(app, cell_id);
        let floor = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Plane3d::default().mesh().size(4000.0, 4000.0));
        app.world_mut().spawn((
            Mesh3d(floor),
            Transform::from_xyz(0.0, y, 0.0),
            ChildOf(root),
        ));
        root
    }

    /// A load door of `cell`, standing at `position` in the render space the cells share, placed as
    /// transform propagation would have left it.
    fn spawn_cell_door(
        app: &mut App,
        cell: Entity,
        position: Vec3,
        auto_load: bool,
        open: bool,
    ) -> Entity {
        let mut door = exterior_door();
        door.auto_load = auto_load;
        let transform = Transform::from_translation(position);
        let mut entity = app.world_mut().spawn((
            transform,
            GlobalTransform::from(transform),
            Visibility::default(),
            door,
            ChildOf(cell),
        ));
        if open {
            entity.insert(DoorState::Open { animated: false });
        }
        entity.id()
    }

    /// The camera the player drives, with the [`Player`] `attach_player` gives it. Yaw zero looks
    /// along `-Z`, as `Player::look_rotation` has it.
    fn spawn_player_camera(app: &mut App, eye: Vec3, yaw: f32) -> Entity {
        app.world_mut()
            .spawn((
                Transform::from_translation(eye).with_rotation(Quat::from_rotation_y(yaw)),
                GlobalTransform::from_translation(eye),
                StreamingCamera,
                Player { yaw, ..default() },
            ))
            .id()
    }

    /// The portal's destination this frame - the cells [`update_portal`] publishes for the door it
    /// is rendering through, which the isolation then moves off the main camera's layers.
    fn portal_shows_cell(app: &mut App, key: CellKey) {
        app.world_mut().resource_mut::<PortalState>().destination = vec![key];
    }

    fn eye_of(app: &App, camera: Entity) -> Vec3 {
        app.world()
            .entity(camera)
            .get::<Transform>()
            .expect("the camera is still there")
            .translation
    }

    /// A destination cell's geometry does not hold the player up.
    ///
    /// The walking probe's ray cast is Bevy's, with `RayCastVisibility::Visible`: it tests inherited
    /// visibility, and the isolation leaves a destination cell visible - it only moves it onto the
    /// portal camera's layer, which no visibility query looks at. So the raw coordinates of an
    /// interior the portal is drawing through are in play for the player: a floor of that cell that
    /// happens to sit above this one's is nearer to the eye than the floor under the player's feet.
    #[test]
    fn a_destination_cells_floor_does_not_hold_the_player_up() {
        let mut app = player_app();
        spawn_floor_cell(&mut app, INTERIOR_ALFTAND01, 0.0);
        spawn_floor_cell(&mut app, FIXTURE_INTERIOR_CELL, 30.0);
        portal_shows_cell(&mut app, CellKey::Interior(FIXTURE_INTERIOR_CELL));
        let camera = spawn_player_camera(&mut app, eye_from_feet(Vec3::ZERO), 0.0);
        // The first frame is the isolation's; the walk that reads the roles it wrote is a later one.
        update(&mut app, 4);

        let eye = eye_of(&app, camera);
        assert!(
            (eye.y - EYE_HEIGHT).abs() < 0.5,
            "the player stands on the floor of the cell they are in, not on the destination's \
             floor thirty units above it: eye {eye:?}"
        );

        // The mesh is not merely somewhere else: the same cell made the *active* space - which is
        // what the player would find if the portal were drawing through a doorway into the other
        // one - holds them up thirty units higher.
        app.world_mut().resource_mut::<ActiveCell>().interior = Some(FIXTURE_INTERIOR_CELL);
        app.world_mut()
            .resource_mut::<PortalState>()
            .destination
            .clear();
        update(&mut app, 3);
        let eye = eye_of(&app, camera);
        assert!(
            (eye.y - (30.0 + EYE_HEIGHT)).abs() < 0.5,
            "the same floor holds the player up once it is the space they are in: eye {eye:?}"
        );
    }

    /// `E` opens the door of the room the player is in, not the door of a pre-streamed cell.
    ///
    /// A pre-streamed cell keeps its door components, and its doors are drawn nowhere near the
    /// space the player stands in - but they are still load doors in the same world, whose own
    /// coordinates can put one of them nearer to the camera than the door in the room, and nearer
    /// is what targeting takes. Here the cell is only pre-streamed, not the portal's destination
    /// (the other case the tests below cover), so nothing of it is drawn at all.
    #[test]
    fn a_prestreamed_cells_door_is_not_the_one_e_opens() {
        let mut app = player_app();
        let here = spawn_cell_root(&mut app, INTERIOR_ALFTAND01);
        let there = spawn_cell_root(&mut app, FIXTURE_INTERIOR_CELL);
        // The camera looks along `-Z`: the pre-streamed cell's door stands 100 units in front of
        // it and the room's own door 200, both straight ahead and inside the range and cone.
        let elsewhere = spawn_cell_door(
            &mut app,
            there,
            Vec3::new(1000.0, 120.0, 900.0),
            false,
            false,
        );
        let ours = spawn_cell_door(
            &mut app,
            here,
            Vec3::new(1000.0, 120.0, 800.0),
            false,
            false,
        );
        spawn_player_camera(&mut app, Vec3::new(1000.0, 120.0, 1000.0), 0.0);
        update(&mut app, 2);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        update(&mut app, 1);
        assert_eq!(
            app.world().resource::<Opened>().0,
            vec![ours],
            "`E` opens the door of the room the player is in, not the pre-streamed cell's door \
             standing nearer to the camera ({elsewhere:?})"
        );
    }

    /// An auto-load marker of a pre-streamed cell crosses nothing, and a marker of the active space
    /// still takes the player - hidden or not.
    ///
    /// Skyrim's invisible `AutoLoadDoor01` markers fire on contact, which is why they are here: the
    /// marker belongs to a cell that is pre-streamed for a door the player may walk through, and
    /// the walk into it is *by coordinates* alone, so a marker anywhere in the world can be walked
    /// into. The second half is the control the test needs: an auto-load door's own model is hidden
    /// by design ([`show_load_door_leaves`] hides an auto-load door's leaf), so visibility is not
    /// what tells a foreign marker from the player's own.
    #[test]
    fn a_foreign_cells_auto_load_marker_crosses_nothing() {
        let base = Vec3::new(1000.0, 0.0, 1000.0);
        let walk = [
            400.0_f32, 300.0, 200.0, 100.0, 40.0, 20.0, 0.0, -20.0, -100.0, -400.0,
        ];
        let eyes = || {
            walk.into_iter()
                .map(|z| eye_from_feet(base + Vec3::new(0.0, 0.0, z)))
        };

        // The pre-streamed cell's marker, alone in the walk: the player walks through a door that
        // is not in the space they are in, and nothing happens.
        let mut app = player_app();
        let there = spawn_cell_root(&mut app, FIXTURE_INTERIOR_CELL);
        portal_shows_cell(&mut app, CellKey::Interior(FIXTURE_INTERIOR_CELL));
        spawn_cell_door(&mut app, there, base, true, false);
        spawn_player_camera(
            &mut app,
            eye_from_feet(base + Vec3::new(0.0, 0.0, 400.0)),
            0.0,
        );
        walk_camera(&mut app, eyes());
        assert!(
            app.world().resource::<Crossings>().0.is_empty(),
            "a marker of a cell the player is not in is not a door they walked into: {:?}",
            app.world().resource::<Crossings>().0
        );

        // The active space's own marker, hidden as an auto-load door's leaf always is: still walked
        // into.
        let mut app = player_app();
        let here = spawn_cell_root(&mut app, INTERIOR_ALFTAND01);
        let ours = spawn_cell_door(&mut app, here, base, true, false);
        app.world_mut().entity_mut(ours).insert(Visibility::Hidden);
        spawn_player_camera(
            &mut app,
            eye_from_feet(base + Vec3::new(0.0, 0.0, 400.0)),
            0.0,
        );
        walk_camera(&mut app, eyes());
        assert_eq!(
            app.world().resource::<Crossings>().0,
            vec![ours],
            "a hidden auto-load marker of the active space is still walked into"
        );
    }

    /// An open door of a pre-streamed cell is not walked through.
    ///
    /// The doorway trigger crosses the player where the doorway's own plane meets their walk, and a
    /// door the portal is drawing through has a doorway of the destination standing in the same
    /// world - at coordinates that can cross the walk the player is making in the room they are in.
    #[test]
    fn a_foreign_cells_open_door_is_not_walked_through() {
        let base = Vec3::new(1000.0, 0.0, 1000.0);
        // A door with no outward direction of its own faces `-Z`, so this is the walk through it:
        // in from the front of its plane and out the other side.
        let walk = [-400.0_f32, -300.0, -100.0, -20.0, 0.0, 20.0, 100.0, 400.0];
        let eyes = || {
            walk.into_iter()
                .map(|z| eye_from_feet(base + Vec3::new(0.0, 0.0, z)))
        };

        let mut app = player_app();
        let there = spawn_cell_root(&mut app, FIXTURE_INTERIOR_CELL);
        portal_shows_cell(&mut app, CellKey::Interior(FIXTURE_INTERIOR_CELL));
        spawn_cell_door(&mut app, there, base, false, true);
        spawn_player_camera(
            &mut app,
            eye_from_feet(base + Vec3::new(0.0, 0.0, -400.0)),
            0.0,
        );
        walk_camera(&mut app, eyes());
        assert!(
            app.world().resource::<Crossings>().0.is_empty(),
            "the destination doorway is not a way out of the room the player is in: {:?}",
            app.world().resource::<Crossings>().0
        );

        // The same doorway, in the room the player is in, is what the walk is for.
        let mut app = player_app();
        let here = spawn_cell_root(&mut app, INTERIOR_ALFTAND01);
        let ours = spawn_cell_door(&mut app, here, base, false, true);
        spawn_player_camera(
            &mut app,
            eye_from_feet(base + Vec3::new(0.0, 0.0, -400.0)),
            0.0,
        );
        walk_camera(&mut app, eyes());
        assert_eq!(
            app.world().resource::<Crossings>().0,
            vec![ours],
            "the doorway of the room the player is in is walked through"
        );
    }

    /// research-197: the portal camera's mesh pipeline key has to be the main camera's, or every
    /// material the destination shows is compiled again the first time a doorway opens onto it.
    /// Bevy adds the tonemap and dither bits only to a view that is not HDR; the main camera is
    /// HDR (`app::setup_world`), so this one must be too - and still hand its image over
    /// untonemapped and undithered, which the main camera then does for it.
    #[test]
    fn the_portal_camera_renders_with_the_main_views_pipelines_and_leaves_tonemapping_to_it() {
        let SunApp {
            app, portal_camera, ..
        } = sun_app();
        let camera = app.world().entity(portal_camera);
        assert!(
            camera.contains::<Hdr>(),
            "an HDR view, like the main camera: no TONEMAP_IN_SHADER or DEBAND_DITHER in its key"
        );
        assert_eq!(
            camera.get::<Tonemapping>(),
            Some(&Tonemapping::None),
            "not tonemapped here: the main camera tonemaps the doorway with the room around it"
        );
        assert_eq!(
            camera.get::<DebandDither>(),
            Some(&DebandDither::Disabled),
            "not dithered here either"
        );
        let RenderTarget::Image(target) = camera.get::<RenderTarget>().expect("a render target")
        else {
            panic!("the portal camera renders into the portal texture");
        };
        let image = app
            .world()
            .resource::<Assets<Image>>()
            .get(&target.handle)
            .expect("the portal texture");
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::Rgba16Float,
            "the target is the HDR view's own format, so the copy into it is exact"
        );
    }

    /// The first-open log line adds up the frames after a doorway opens: pipelines created since
    /// the open, the most waiting at once, and the frames over the hitch threshold.
    #[test]
    fn an_open_window_counts_the_pipelines_and_hitches_after_the_open() {
        let mut open = OpenWindow::new(0x0001_CBB0, 100);
        open.add_frame(16.0, 100, 0);
        open.add_frame(OPEN_HITCH_MS + 10.0, 120, 7);
        open.add_frame(30.0, 138, 2);
        open.add_frame(16.0, 138, 0);
        assert_eq!(open.frames, 4);
        assert_eq!(open.created(), 38);
        assert_eq!(open.max_waiting, 7);
        assert_eq!(open.frames_waiting, 2);
        assert_eq!(open.hitches, 2);
        assert_eq!(open.max_ms, OPEN_HITCH_MS + 10.0);
        assert_eq!(OpenWindow::new(1, 50).created(), 0);
    }

    /// Runs one frame of the occlusion decision for `door` with the sight lines `blocked` or not,
    /// and says whether the test ran and what was decided.
    fn occlusion_frame(
        occlusion: &mut DoorwayOcclusion,
        door: Entity,
        blocked: bool,
    ) -> (bool, bool) {
        let mut tested = false;
        let hidden = occlusion.update(door, || {
            tested = true;
            blocked
        });
        (tested, hidden)
    }

    #[test]
    fn a_doorway_in_view_is_hidden_only_after_several_blocked_tests_in_a_row() {
        let door = Entity::from_raw_u32(7).unwrap();
        let mut occlusion = DoorwayOcclusion::default();
        let mut frames = Vec::new();
        for _ in 0..(OCCLUSION_CONFIRM_TESTS * OCCLUSION_RETEST_FRAMES) {
            frames.push(occlusion_frame(&mut occlusion, door, true));
        }
        // Tested on the first frame and then every `OCCLUSION_RETEST_FRAMES` frames while drawn;
        // every frame once hidden.
        let tests: Vec<usize> = frames
            .iter()
            .take(((OCCLUSION_CONFIRM_TESTS - 1) * OCCLUSION_RETEST_FRAMES + 1) as usize)
            .enumerate()
            .filter_map(|(frame, (tested, _))| tested.then_some(frame))
            .collect();
        let expected: Vec<usize> = (0..OCCLUSION_CONFIRM_TESTS as usize)
            .map(|test| test * OCCLUSION_RETEST_FRAMES as usize)
            .collect();
        assert_eq!(tests, expected);
        // Drawn until the last of the confirming tests, hidden from that frame on.
        let first_hidden = frames.iter().position(|(_, hidden)| *hidden);
        assert_eq!(first_hidden, expected.last().copied());
        assert!(
            frames[first_hidden.unwrap()..]
                .iter()
                .all(|(tested, hidden)| *tested && *hidden)
        );
    }

    #[test]
    fn one_clear_test_resets_the_count_toward_hiding() {
        let door = Entity::from_raw_u32(7).unwrap();
        let mut occlusion = DoorwayOcclusion::default();
        let mut test = 0;
        for _ in 0..100 {
            // Blocked, blocked, clear, blocked, ...: never enough in a row.
            let blocked = test % OCCLUSION_CONFIRM_TESTS != OCCLUSION_CONFIRM_TESTS - 1;
            let (tested, hidden) = occlusion_frame(&mut occlusion, door, blocked);
            if tested {
                test += 1;
            }
            assert!(!hidden);
        }
    }

    #[test]
    fn a_hidden_doorway_is_tested_every_frame_and_drawn_the_frame_a_line_clears() {
        let door = Entity::from_raw_u32(7).unwrap();
        let mut occlusion = DoorwayOcclusion::default();
        while !occlusion_frame(&mut occlusion, door, true).1 {}
        for _ in 0..10 {
            assert_eq!(occlusion_frame(&mut occlusion, door, true), (true, true));
        }
        assert_eq!(occlusion_frame(&mut occlusion, door, false), (true, false));
        // Back in view, it takes the whole confirmation again to hide it.
        let mut frames = 0;
        while !occlusion_frame(&mut occlusion, door, true).1 {
            frames += 1;
        }
        assert!(frames >= ((OCCLUSION_CONFIRM_TESTS - 1) * OCCLUSION_RETEST_FRAMES) as usize);
    }

    #[test]
    fn another_door_or_a_forgotten_one_starts_in_view() {
        let (door, other) = (
            Entity::from_raw_u32(7).unwrap(),
            Entity::from_raw_u32(8).unwrap(),
        );
        let mut occlusion = DoorwayOcclusion::default();
        while !occlusion_frame(&mut occlusion, door, true).1 {}
        assert_eq!(occlusion_frame(&mut occlusion, other, true), (true, false));
        let mut occlusion = DoorwayOcclusion::default();
        while !occlusion_frame(&mut occlusion, door, true).1 {}
        occlusion.forget();
        assert_eq!(occlusion_frame(&mut occlusion, door, true), (true, false));
    }

    #[test]
    fn the_sight_line_grid_covers_the_doorway_and_its_margin_on_the_eyes_side() {
        // A doorway 200 wide and 300 high, facing +Z and facing -Z: the grid is on the eye's side
        // of either.
        let eye = Vec3::new(50.0, 20.0, 800.0);
        for rotation in [Quat::IDENTITY, Quat::from_rotation_y(PI)] {
            let quad = Transform::from_scale(Vec3::new(200.0, 300.0, 1.0)).with_rotation(rotation);
            let samples = doorway_occlusion_samples(&quad, eye);
            assert_eq!(samples.len(), OCCLUSION_GRID * OCCLUSION_GRID);
            for sample in samples {
                assert!((sample.z - OCCLUSION_STANDOFF).abs() < 1e-3, "{sample}");
            }
            let (low, high) = samples
                .iter()
                .fold((Vec3::MAX, Vec3::MIN), |(low, high), &sample| {
                    (low.min(sample), high.max(sample))
                });
            let reach = 0.5 + OCCLUSION_MARGIN;
            assert!((high.x - 200.0 * reach).abs() < 1e-3);
            assert!((low.x + 200.0 * reach).abs() < 1e-3);
            assert!((high.y - 300.0 * reach).abs() < 1e-3);
            assert!((low.y + 300.0 * reach).abs() < 1e-3);
        }
    }

    /// A box-shaped occluder of `size` standing at `centre`.
    fn box_occluder(mesh: &Mesh, centre: Vec3, size: Vec3) -> Occluder<'_> {
        Occluder {
            transform: Affine3A::from_scale_rotation_translation(size, Quat::IDENTITY, centre),
            aabb: Aabb3d::new(Vec3::ZERO, Vec3::splat(0.5)),
            mesh,
        }
    }

    #[test]
    fn a_wall_between_the_eye_and_the_doorway_blocks_every_sight_line() {
        let mesh = Mesh::from(Cuboid::new(1.0, 1.0, 1.0));
        let quad = Transform::from_scale(Vec3::new(200.0, 300.0, 1.0));
        let eye = Vec3::new(0.0, 0.0, 1000.0);
        let samples = doorway_occlusion_samples(&quad, eye);
        let halfway = Vec3::new(0.0, 0.0, 500.0);

        // A wall halfway, wider and taller than the grid's cone at that depth.
        let wall = box_occluder(&mesh, halfway, Vec3::new(600.0, 600.0, 20.0));
        assert!(sight_lines_blocked(eye, &samples, &[wall]));

        // Nothing in the way, or a wall behind the doorway: every line is open.
        assert!(!sight_lines_blocked(eye, &samples, &[]));
        let behind = box_occluder(
            &mesh,
            Vec3::new(0.0, 0.0, -200.0),
            Vec3::new(600.0, 600.0, 20.0),
        );
        assert!(!sight_lines_blocked(eye, &samples, &[behind]));

        // A post in front of the middle of the doorway hides some of it, not all.
        let post = box_occluder(&mesh, halfway, Vec3::new(40.0, 1000.0, 40.0));
        assert!(!sight_lines_blocked(eye, &samples, &[post]));

        // A wall covering the doorway but not its margin leaves the outer lines open: the doorway
        // is about to come out from behind it.
        let narrow = box_occluder(&mesh, halfway, Vec3::new(130.0, 180.0, 20.0));
        assert!(!sight_lines_blocked(eye, &samples, &[narrow]));
    }

    #[test]
    fn a_wall_seen_from_behind_hides_nothing() {
        let quad = Transform::from_scale(Vec3::new(200.0, 300.0, 1.0));
        let eye = Vec3::new(0.0, 0.0, 1000.0);
        let samples = doorway_occlusion_samples(&quad, eye);
        let wall = |mesh| Occluder {
            transform: Affine3A::from_scale_rotation_translation(
                Vec3::splat(1000.0),
                Quat::IDENTITY,
                Vec3::new(0.0, 0.0, 500.0),
            ),
            aabb: Aabb3d::new(Vec3::ZERO, Vec3::new(0.5, 0.5, 0.01)),
            mesh,
        };
        // A single-sided sheet turned away from the eye is a back face the renderer culls; turned
        // toward it, it hides everything behind it.
        let away = Mesh::from(Plane3d::new(Vec3::NEG_Z, Vec2::splat(0.5)));
        assert!(!sight_lines_blocked(eye, &samples, &[wall(&away)]));
        let toward = Mesh::from(Plane3d::new(Vec3::Z, Vec2::splat(0.5)));
        assert!(sight_lines_blocked(eye, &samples, &[wall(&toward)]));
    }

    // --- impl-225: the destination hold ---

    fn held_door(app: &mut App) -> Entity {
        app.world_mut().spawn_empty().id()
    }

    #[test]
    fn a_destination_is_held_while_the_doorway_is_out_of_view_and_let_go_after() {
        let mut app = App::new();
        let door = held_door(&mut app);
        let keys = vec![CellKey::Interior(INTERIOR_ALFTAND01)];
        let mut state = PortalState::default();
        state.draw_destination(door, keys.clone());
        assert_eq!(state.destination, keys);

        // The player turns away (or steps behind the door's plane) at t = 10 s.
        state.lose_destination(Some(10.0), |_| true);
        assert_eq!(state.destination, keys, "kept the first frame");
        state.lose_destination(Some(10.0 + DESTINATION_HOLD_SECONDS - 0.1), |_| true);
        assert_eq!(state.destination, keys, "kept for the whole hold");

        // And turns back: drawn again, the hold starts over from the next time it is lost.
        state.draw_destination(door, keys.clone());
        state.lose_destination(Some(20.0), |_| true);
        state.lose_destination(Some(20.0 + DESTINATION_HOLD_SECONDS - 0.1), |_| true);
        assert_eq!(
            state.destination, keys,
            "a fresh hold after the doorway was drawn again"
        );

        state.lose_destination(Some(20.0 + DESTINATION_HOLD_SECONDS), |_| true);
        assert!(
            state.destination.is_empty(),
            "let go once the hold has run out"
        );
        assert!(state.hold.is_none());
        state.lose_destination(Some(20.5), |_| true);
        assert!(state.destination.is_empty(), "and not picked up again");
    }

    #[test]
    fn a_door_that_closes_ends_the_hold_at_once() {
        let mut app = App::new();
        let door = held_door(&mut app);
        let keys = vec![CellKey::Interior(INTERIOR_ALFTAND01)];
        let mut state = PortalState::default();
        state.draw_destination(door, keys.clone());
        state.lose_destination(Some(1.0), |_| true);
        assert_eq!(state.destination, keys);
        // The door shut (or its far side unloaded, or the player crossed): no way through any more.
        state.lose_destination(Some(1.1), |held| held != door);
        assert!(state.destination.is_empty());
        assert!(state.hold.is_none());

        // Straight from drawing, too: a door that closes in the frame it is lost holds nothing.
        state.draw_destination(door, keys);
        state.lose_destination(Some(2.0), |_| false);
        assert!(state.destination.is_empty());
    }

    #[test]
    fn another_doors_destination_replaces_a_held_one_at_once() {
        let mut app = App::new();
        let (first, second) = (held_door(&mut app), held_door(&mut app));
        let (first_keys, second_keys) = (
            vec![CellKey::Interior(INTERIOR_ALFTAND01)],
            vec![CellKey::Interior(0x0005_6C1B)],
        );
        let mut state = PortalState::default();
        state.draw_destination(first, first_keys.clone());
        state.lose_destination(Some(1.0), |_| true);
        assert_eq!(state.destination, first_keys);
        state.draw_destination(second, second_keys.clone());
        assert_eq!(
            state.destination, second_keys,
            "the new destination at once"
        );
        // Losing the new one holds the new one - and asks about that door, not the old one.
        state.lose_destination(Some(1.5), |held| held == second);
        assert_eq!(state.destination, second_keys);
    }

    #[test]
    fn a_run_without_a_clock_holds_no_destination() {
        let mut app = App::new();
        let door = held_door(&mut app);
        let mut state = PortalState::default();
        state.draw_destination(door, vec![CellKey::Interior(INTERIOR_ALFTAND01)]);
        state.lose_destination(None, |_| true);
        assert!(state.destination.is_empty());
    }

    #[test]
    fn a_held_destination_is_not_re_layered_when_the_doorway_comes_back() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (_, mesh) = spawn_cell(&mut app, 0x0005_6C1B, None, None, None);
        let door = held_door(&mut app);
        let keys = vec![CellKey::Interior(0x0005_6C1B)];
        app.world_mut()
            .resource_mut::<PortalState>()
            .draw_destination(door, keys.clone());
        update(&mut app, 2);
        let destination = RenderLayers::layer(DESTINATION_LAYER);
        assert_eq!(layers_of(&app, mesh), destination);
        let changes = |app: &App| app.world().resource::<PortalState>().churn.relayer_changes;
        let before = changes(&app);

        // Out of view for most of the hold, and back: the cell keeps its role throughout.
        for time in [0.0, 0.5, 1.0, 1.5] {
            app.world_mut()
                .resource_mut::<PortalState>()
                .lose_destination(Some(time), |_| true);
            update(&mut app, 1);
            assert_eq!(layers_of(&app, mesh), destination, "held at {time} s");
        }
        app.world_mut()
            .resource_mut::<PortalState>()
            .draw_destination(door, keys);
        update(&mut app, 1);
        assert_eq!(
            changes(&app),
            before,
            "turning away and back re-layered nothing"
        );

        // Lost for longer than the hold: the cell is hidden, in one role change.
        for time in [10.0, 10.0 + DESTINATION_HOLD_SECONDS] {
            app.world_mut()
                .resource_mut::<PortalState>()
                .lose_destination(Some(time), |_| true);
            update(&mut app, 1);
        }
        assert_eq!(layers_of(&app, mesh), RenderLayers::none());
        assert_eq!(changes(&app), before + 1);
    }

    #[test]
    fn a_cell_that_comes_back_with_a_new_root_is_counted_as_a_respawn() {
        let mut app = portal_app();
        spawn_camera(&mut app, Vec3::ZERO);
        let (root, _) = spawn_cell(&mut app, 0x0005_6C1B, None, None, None);
        update(&mut app, 2);
        let respawns = |app: &App| app.world().resource::<PortalState>().churn.respawns;
        assert_eq!(respawns(&app), 0, "a first spawn is not a respawn");
        app.world_mut().entity_mut(root).despawn();
        update(&mut app, 1);
        spawn_cell(&mut app, 0x0005_6C1B, None, None, None);
        update(&mut app, 1);
        assert_eq!(respawns(&app), 1);
    }

    /// Steps `probe` through `frames` quiet frames that all find `floors`, and returns the steps.
    fn probe_frames(
        probe: &mut DoorwayFloorsProbe,
        floors: (f32, f32),
        frames: u32,
    ) -> Vec<FloorProbeStep> {
        (0..frames)
            .map(|_| probe.step(Some(floors), true))
            .collect()
    }

    /// Honningbrew's side door (research-229, flake A): the probe first falls through to the
    /// terrain under the platform, then the platform's meshes arrive. The later, higher floor
    /// re-anchors the door; it is not kept on the first one it found.
    #[test]
    fn a_floor_found_late_moves_the_doorway_anchor() {
        let mut probe = DoorwayFloorsProbe::default();
        let terrain = (-80.8, 0.0);
        let platform = (-69.5, 0.0);
        assert_eq!(
            probe.step(Some(terrain), true),
            FloorProbeStep::Apply(terrain)
        );
        assert!(
            probe_frames(&mut probe, terrain, 10)
                .iter()
                .all(|step| *step == FloorProbeStep::Wait)
        );
        assert_eq!(
            probe.step(Some(platform), false),
            FloorProbeStep::Apply(platform)
        );
        let steps = probe_frames(&mut probe, platform, FLOOR_PROBE_SETTLE_FRAMES);
        assert_eq!(steps.last(), Some(&FloorProbeStep::Settled));
        assert_eq!(probe.applied, Some(platform));
    }

    /// Floors that hold - within the tolerance - for the settle frames mark the door measured, and
    /// not a frame sooner.
    #[test]
    fn floors_that_hold_are_marked_measured() {
        let mut probe = DoorwayFloorsProbe::default();
        let porch = (0.0, -2.0);
        assert_eq!(probe.step(Some(porch), true), FloorProbeStep::Apply(porch));
        let wobble = (porch.0 + 0.4, porch.1 - 0.4);
        let steps: Vec<_> = (0..FLOOR_PROBE_SETTLE_FRAMES)
            .map(|frame| probe.step(Some(if frame % 2 == 0 { wobble } else { porch }), true))
            .collect();
        assert!(
            steps[..steps.len() - 1]
                .iter()
                .all(|step| *step == FloorProbeStep::Wait)
        );
        assert_eq!(steps.last(), Some(&FloorProbeStep::Settled));
        assert_eq!(probe.applied, Some(porch));
    }

    /// A mesh spawning near the doorway keeps the floors from settling, and re-arming a measured
    /// door (a scene spawned again) measures it for the full settle frames again.
    #[test]
    fn a_spawning_mesh_holds_off_the_mark_and_a_respawn_rearms_it() {
        let mut probe = DoorwayFloorsProbe::default();
        let floors = (-69.5, 0.0);
        probe.step(Some(floors), true);
        probe_frames(&mut probe, floors, FLOOR_PROBE_SETTLE_FRAMES - 2);
        assert_eq!(probe.step(Some(floors), false), FloorProbeStep::Wait);
        let steps = probe_frames(&mut probe, floors, FLOOR_PROBE_SETTLE_FRAMES);
        assert_eq!(
            steps
                .iter()
                .position(|step| *step == FloorProbeStep::Settled),
            Some(FLOOR_PROBE_SETTLE_FRAMES as usize - 1)
        );
        probe.rearm();
        let steps = probe_frames(&mut probe, floors, FLOOR_PROBE_SETTLE_FRAMES);
        assert!(
            steps[..steps.len() - 1]
                .iter()
                .all(|step| *step == FloorProbeStep::Wait)
        );
        assert_eq!(steps.last(), Some(&FloorProbeStep::Settled));
    }

    /// No floor on one side for the attempts gives up; a frame without a floor does not count
    /// towards the settle frames.
    #[test]
    fn a_doorway_without_floors_is_given_up_on() {
        let mut probe = DoorwayFloorsProbe::default();
        let steps: Vec<_> = (0..FLOOR_PROBE_ATTEMPTS)
            .map(|_| probe.step(None, true))
            .collect();
        assert_eq!(steps.last(), Some(&FloorProbeStep::GiveUp));
        assert!(
            steps[..steps.len() - 1]
                .iter()
                .all(|step| *step == FloorProbeStep::Wait)
        );

        let mut probe = DoorwayFloorsProbe::default();
        let floors = (0.0, 0.0);
        probe.step(Some(floors), true);
        probe_frames(&mut probe, floors, FLOOR_PROBE_SETTLE_FRAMES - 1);
        assert_eq!(probe.step(None, true), FloorProbeStep::Wait);
        assert_eq!(probe.step(Some(floors), true), FloorProbeStep::Wait);
    }

    /// The active space is centred on the camera's cell at a render origin that is not zero, on
    /// both sides of a cell's south-west corner: a render step north is a step along `-z`, so a
    /// camera just south of the corner is one grid lower, and one just west of it one grid left.
    /// The expected grids are worked by hand, not read back through the helper under test.
    #[test]
    fn the_active_space_is_centred_on_the_camera_cell_away_from_the_zero_origin() {
        let active = ActiveCell {
            worldspace_id: 0x3C,
            interior: None,
        };
        let origin = IVec2::new(19, -7);
        // Render (0, 0, 0) is the south-west corner of the origin's own cell.
        for (camera, expected) in [
            (Vec3::ZERO, IVec2::new(19, -7)),
            (Vec3::new(100.0, 50.0, 300.0), IVec2::new(19, -8)),
            (Vec3::new(-1.0, 0.0, -1.0), IVec2::new(18, -7)),
            (Vec3::new(4096.0, 0.0, -4097.0), IVec2::new(20, -6)),
        ] {
            let space = ActiveSpace::of(&active, 3, camera, origin);
            assert_eq!(space.center, expected, "a camera at render {camera:?}");
            assert_eq!(space.worldspace_id, 0x3C);
            assert_eq!(space.radius, 3);
        }
    }
}
