//! The door layer: a near load-door pair is drawn once, by a door that belongs to neither space
//! (`docs/design/door-layer.md`).
//!
//! A load door is two references, one in each space. Each space used to draw its own copy and the
//! portal kept the two in agreement with a mirror of one leaf drawn for both ends; every rule of
//! that set was a place for the two copies to disagree (a second closed pair of leaves behind the
//! open pair at Dragonsreach, a black doorway inside it). Here the pair is drawn **once**:
//!
//! 1. **The door-layer door.** For each load door of the player's space within
//!    [`DOOR_LAYER_RADIUS`] whose model swings, one [`DoorCopy`] of kind [`CopyKind::Main`] is
//!    spawned from the model of that door - its *driver*: frame and leaves, posed node for node from
//!    the driver's own nodes, which the driver's own clips and [`DoorState`] move
//!    ([`pose_door_copies`]). It is drawn by the main camera at full resolution on
//!    [`DOOR_LAYERS`] (layer 0, which the main camera, the water reflection and the engine sun's
//!    shadows read, plus [`DOOR_LAYER`], which marks it); the portal camera, which draws only
//!    `DESTINATION_LAYER`, never draws it.
//! 2. **Both native copies are hidden** once the door-layer door has drawn: the driver and its twin
//!    in the other space (the *follower*), which `crate::portal::isolate_cells` moves onto no layer
//!    at all ([`DoorLayer::hidden_natives`]). They stay where they are for everything that
//!    ray-casts the world by visibility - the walk probe, the doorway floor probes - so the leaf that
//!    stops the player is still the native one, whose state is the door's.
//! 3. **The doorway is filled in one place.** The portal's quad stands in the doorway behind the
//!    door-layer door and depth decides what shows. The one thing the main view cannot draw is the
//!    part of a leaf that swings *behind* the doorway plane, away from the player: the quad's depth
//!    composite hides the source space behind the plane past a wall's depth. For that the doorway
//!    image carries the leaf too - a [`CopyKind::Window`] copy of the same door on the portal
//!    camera's layer, at the pose the doorway map carries the door-layer door to, leaf only - so the
//!    part of the swing that is in the room is drawn in the room, and the rest in front of it.
//! 4. **Crossing keeps the same door.** At the swap nothing is replaced: the door-layer door's pose
//!    is re-expressed in the new active space through the driver's own doorway map
//!    ([`door_copy_pose`]) - the map the crossing moves the player by - so it stands exactly where it
//!    stood in the view. Its model stays the driver's until the switch cannot be seen
//!    ([`hand_back`]): both doors closed and the player a few metres away, or the doorway out of
//!    view. Then the copy goes and the next one is spawned from the door on the player's side.
//! 5. **Scale.** The driver's own scale, carried by the doorway map's ratio of the two sides'
//!    scales once re-expressed ([`door_copy_pose`]), so the leaf fits the arch it stands in.
//!
//! # What reads it
//!
//! [`DoorLayer`] is the one answer to "which doors are on the door layer": the isolation hides the
//! natives from it, `crate::door_animation` swings each follower with its driver (the far-door
//! drive) and sends `E` at a follower to its driver, the doorway's filler and occlusion tests look
//! at the copy instead of the hidden native, and the walk probe skips every [`DoorCopyPart`].

use crate::{
    door_animation::DoorAnimation,
    doors::{DoorAnchor, DoorLeaf, DoorState, LoadDoor},
    portal::{
        DESTINATION_LAYER, LeafOpening, PortalState, depth_composite::is_doorway_filler,
        door_has_its_own_swing, doorway_quad_transform,
    },
    streaming::RenderOrigin,
    transition::{DOOR_PRESTREAM_RADIUS, DoorMap, door_map},
    world::components::{ExpectedModelBounds, InstanceBounds, StreamingCamera},
};
use bevy::{
    animation::AnimationTargetId,
    camera::{primitives::Aabb, visibility::RenderLayers},
    math::Affine3A,
    prelude::*,
    world_serialization::WorldAssetRoot,
};
use std::collections::{HashMap, HashSet};

/// The render layer that marks the door layer's drawing. The door-layer door is drawn on
/// [`DOOR_LAYERS`], which is this and layer 0.
pub const DOOR_LAYER: usize = 3;

/// The layers every entity of a door-layer door is drawn on: layer 0, which the main camera (0 and
/// 1), the water reflection camera (0) and the engine sun's shadow cascades (a layer-0 light) all
/// read and the portal camera (`DESTINATION_LAYER` only) does not, and [`DOOR_LAYER`], which says
/// what it is.
pub fn door_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, DOOR_LAYER])
}

/// [`door_layers`], named for the docs.
pub const DOOR_LAYERS: [usize; 2] = [0, DOOR_LAYER];

/// How near the camera a load door of the player's space has to be for the door layer to draw it:
/// the distance its destination is pre-streamed within, which is also where the portal can draw
/// through it.
pub(crate) const DOOR_LAYER_RADIUS: f32 = DOOR_PRESTREAM_RADIUS;

/// How far a door-layer door already standing is kept while its driver is the player's own door: a
/// little past [`DOOR_LAYER_RADIUS`], so a player pacing its edge does not spawn and drop it.
const DOOR_LAYER_KEEP_RADIUS: f32 = DOOR_PRESTREAM_RADIUS + 200.0;

/// How far from a door-layer door the player has to be, in units, before a door whose model is the
/// other side's is handed to the model of the player's side ([`hand_back`]): about 3.5 m, a few steps
/// out of the doorway.
pub(crate) const HAND_BACK_DISTANCE: f32 = 250.0;

/// How much of a door's extent, in units, the view test grows its angle by: a door is out of view
/// only when no part of it could be on screen ([`out_of_view`]).
const DOOR_VIEW_RADIUS: f32 = 250.0;

/// Which of the two drawings of a door-layer door an entity is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyKind {
    /// The door-layer door itself: the whole model, on [`DOOR_LAYERS`], in the player's space.
    Main,
    /// The leaf of the same door in the doorway image: on the portal camera's layer, at the pose
    /// the portal's doorway map carries the door-layer door to, drawn only while the portal draws
    /// through the pair and the door swings.
    Window,
}

/// The root of a second instance of a door's model that the door layer draws.
///
/// Not a reference: a fresh entity with a scene root and nothing else - no `MeshHandle`, no
/// `LoadDoor`, no `FormId` - so `streaming.rs` does not measure it as one and `model_animation.rs`
/// does not hand it an idle clip.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct DoorCopy {
    /// The native door whose model this instances and whose nodes pose it.
    pub(crate) driver: Entity,
    pub(crate) kind: CopyKind,
    /// Whether the copy has drawn the driver's model at least once ([`pose_door_copies`]): the
    /// natives are hidden only from then on, so the frames the instance takes to spawn show them.
    pub(crate) ready: bool,
    /// Whether the one-per-copy log line has been written.
    logged: bool,
}

/// Every entity of a [`DoorCopy`]'s instance: what the walk probe and the floor probes skip, since
/// the natives are what the player collides with.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct DoorCopyPart;

/// One load-door pair on the door layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LayerPair {
    /// The native door whose model the door-layer door is and whose state and clips move it: the
    /// door on the player's side when the pair was put on the layer.
    pub(crate) driver: Entity,
    /// The driver's twin in the other space - the reference its link lands at - once it is
    /// resident.
    pub(crate) follower: Option<Entity>,
    /// The door-layer door ([`CopyKind::Main`]).
    pub(crate) copy: Entity,
    /// The doorway image's leaf ([`CopyKind::Window`]), while there is one.
    pub(crate) window: Option<Entity>,
    /// Whether [`copy`](Self::copy) has drawn: the natives are hidden from then on.
    pub(crate) ready: bool,
    /// The driver's form id, for the log.
    pub(crate) ref_id: u32,
}

impl LayerPair {
    /// Whether `door` is one of the pair's two natives.
    pub(crate) fn has(&self, door: Entity) -> bool {
        self.driver == door || self.follower == Some(door)
    }
}

/// The load-door pairs drawn on the door layer this frame ([`manage_door_layer`]).
#[derive(Resource, Debug, Default)]
pub(crate) struct DoorLayer {
    pairs: Vec<LayerPair>,
}

impl DoorLayer {
    /// The pair `door` is a native of (driver or follower), if it is on the door layer.
    pub(crate) fn pair_of(&self, door: Entity) -> Option<&LayerPair> {
        self.pairs.iter().find(|pair| pair.has(door))
    }

    /// The door whose state is the door-layer door's, for a native of a pair: the driver. `None`
    /// for a door that is not on the door layer.
    pub(crate) fn driver_of(&self, door: Entity) -> Option<Entity> {
        self.pair_of(door).map(|pair| pair.driver)
    }

    /// Whether `door` is the follower of a pair: a native whose state follows its driver's.
    pub(crate) fn is_follower(&self, door: Entity) -> bool {
        self.pairs
            .iter()
            .any(|pair| pair.follower == Some(door) && pair.driver != door)
    }

    /// The door-layer door drawing `door`'s pair, when it has drawn: what stands in the doorway in
    /// place of the hidden native.
    pub(crate) fn drawn_copy_of(&self, door: Entity) -> Option<Entity> {
        self.pair_of(door)
            .filter(|pair| pair.ready)
            .map(|pair| pair.copy)
    }

    /// The natives the isolation hides this frame: both doors of every pair whose door-layer door
    /// has drawn.
    pub(crate) fn hidden_natives(&self) -> HashSet<Entity> {
        self.pairs
            .iter()
            .filter(|pair| pair.ready)
            .flat_map(|pair| std::iter::once(pair.driver).chain(pair.follower))
            .collect()
    }

    /// The pairs whose follower swings with the driver: `(driver, follower)`.
    pub(crate) fn driven_pairs(&self) -> impl Iterator<Item = (Entity, Entity)> + '_ {
        self.pairs
            .iter()
            .filter_map(|pair| pair.follower.map(|follower| (pair.driver, follower)))
    }

    /// Every pair, for the tests and the captures.
    pub(crate) fn pairs(&self) -> &[LayerPair] {
        &self.pairs
    }

    /// A door layer holding `(driver, follower)` pairs whose door-layer doors have drawn, for other
    /// modules' tests.
    #[cfg(test)]
    pub(crate) fn holding_for_test(pairs: &[(Entity, Entity)]) -> Self {
        Self {
            pairs: pairs
                .iter()
                .map(|&(driver, follower)| LayerPair {
                    driver,
                    follower: Some(follower),
                    copy: Entity::PLACEHOLDER,
                    window: None,
                    ready: true,
                    ref_id: 0,
                })
                .collect(),
        }
    }
}

/// Registers the door layer. Added by `crate::portal::PortalPlugin` for the runs that are looked
/// at; the system that manages the pairs is ordered in the portal's own frame by that plugin.
pub(crate) struct DoorLayerPlugin;

impl Plugin for DoorLayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DoorLayer>().add_systems(
            PostUpdate,
            // The copies' poses *are* the drivers', so they are copied after the animation has
            // advanced the drivers' nodes and before the transforms propagate.
            pose_door_copies
                .after(bevy::app::AnimationSystems)
                .before(TransformSystems::Propagate),
        );
    }
}

/// Whether the pair's door-layer door, now drawn with the other side's model, may take the model of
/// the player's side: only when nobody can see the switch - the driver's side is not the player's
/// any more, both doors are shut, and the player is [`HAND_BACK_DISTANCE`] away or the doorway is
/// out of view.
pub(crate) fn hand_back(
    driver_in_players_space: bool,
    both_closed: bool,
    distance: f32,
    in_view: bool,
) -> bool {
    !driver_in_players_space && both_closed && (distance > HAND_BACK_DISTANCE || !in_view)
}

/// Whether a door at `position` is out of the view of an eye at `eye` looking along `forward`,
/// with a perspective of vertical field of view `fov` (radians) and `aspect`: no part of it within
/// [`DOOR_VIEW_RADIUS`] can be inside the view's cone around its diagonal.
pub(crate) fn out_of_view(eye: Vec3, forward: Vec3, fov: f32, aspect: f32, position: Vec3) -> bool {
    let to_door = position - eye;
    let distance = to_door.length();
    if distance <= DOOR_VIEW_RADIUS {
        return false;
    }
    let half_diagonal = ((fov * 0.5).tan() * (1.0 + aspect * aspect).sqrt()).atan();
    let grown = half_diagonal + (DOOR_VIEW_RADIUS / distance).clamp(-1.0, 1.0).asin();
    forward.angle_between(to_door) > grown
}

/// The pose of a door-layer door in the player's space, from its driver's own world pose.
///
/// While the driver is a door of the player's space, it is the driver's pose. Once the player has
/// crossed through the pair, the driver stands in the space behind the doorway, and its pose is
/// carried into the player's space through the driver's own doorway map - the map the crossing
/// moved the player by, so the door stands where it stood in their view. The map is rigid; `scale`
/// is the ratio of the follower's scale to the driver's (1 without a follower), applied about the
/// doorway the map lands on, so a door whose two sides are placed at different scales (Whiterun's
/// gate: 1.0 outside, 1.1 inside) fits the arch it now stands in.
pub(crate) fn door_copy_pose(driver: Transform, map: Option<&DoorMap>, scale: f32) -> Transform {
    let Some(map) = map else {
        return driver;
    };
    let (position, rotation) = map.pose(driver.translation, driver.rotation);
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    Transform {
        translation: map.arrival_position + (position - map.arrival_position) * scale,
        rotation,
        scale: driver.scale * scale,
    }
}

/// A load door as [`manage_door_layer`] reads it.
type LayerDoorQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Entity,
        &'static GlobalTransform,
        &'static LoadDoor,
        Option<&'static DoorState>,
        Option<&'static DoorAnimation>,
        Option<&'static WorldAssetRoot>,
    ),
    Without<DoorCopy>,
>;

/// Puts the load doors near the player on the door layer, takes them off it, hands a crossed
/// door's model back once nobody can see it, and publishes what the isolation hides.
///
/// Runs in the portal's frame after `update_portal` - the window's leaf is for the door the portal
/// picked this frame - and before `isolate_cells`, which hides the natives this publishes.
#[allow(clippy::too_many_arguments)]
pub(crate) fn manage_door_layer(
    mut commands: Commands,
    mut layer: ResMut<DoorLayer>,
    mut portal: ResMut<PortalState>,
    parents: Query<&ChildOf>,
    doors: LayerDoorQuery,
    camera: Query<(&Transform, Option<&Projection>), With<StreamingCamera>>,
    copies: Query<&DoorCopy>,
) {
    let camera = camera.single().ok();
    let eye = camera.map(|(transform, _)| transform.translation);
    let forward = camera.map(|(transform, _)| transform.rotation * Vec3::NEG_Z);
    let (fov, aspect) = match camera.and_then(|(_, projection)| projection) {
        Some(Projection::Perspective(perspective)) => (perspective.fov, perspective.aspect_ratio),
        _ => (std::f32::consts::FRAC_PI_4, 16.0 / 9.0),
    };
    let active = |door: Entity| portal.is_in_active_space(door, &parents);
    let distance = |door: Entity| {
        eye.and_then(|eye| {
            doors
                .get(door)
                .ok()
                .map(|(_, global, ..)| global.translation().distance(eye))
        })
        .unwrap_or(0.0)
    };
    let closed = |door: Option<Entity>| {
        door.is_none_or(|door| {
            doors
                .get(door)
                .ok()
                .is_none_or(|(_, _, _, state, ..)| state.is_none_or(|s| *s == DoorState::Closed))
        })
    };
    // The other end of a door's link, wherever it is resident.
    let twin_of = |driver: Entity| -> Option<Entity> {
        let (_, _, row, ..) = doors.get(driver).ok()?;
        doors.iter().find_map(|(entity, _, other, ..)| {
            (entity != driver && other.ref_id == row.destination.destination_ref_id)
                .then_some(entity)
        })
    };

    // The pairs standing: kept, updated, or dropped.
    let mut kept: Vec<LayerPair> = Vec::with_capacity(layer.pairs.len());
    for mut pair in std::mem::take(&mut layer.pairs) {
        let drop = |commands: &mut Commands, pair: &LayerPair| {
            commands.entity(pair.copy).try_despawn();
            if let Some(window) = pair.window {
                commands.entity(window).try_despawn();
            }
        };
        if !doors.contains(pair.driver) || !copies.contains(pair.copy) {
            drop(&mut commands, &pair);
            continue;
        }
        if pair
            .follower
            .is_none_or(|follower| !doors.contains(follower))
        {
            pair.follower = twin_of(pair.driver);
        }
        let driver_active = active(pair.driver);
        let follower_active = pair.follower.is_some_and(active);
        // Where the door-layer door stands in the player's space: the driver while it is theirs,
        // the follower's place once they crossed.
        let (standing, in_players_space) = if driver_active {
            (Some(pair.driver), true)
        } else if follower_active {
            (pair.follower, true)
        } else {
            (None, false)
        };
        let both_closed = closed(Some(pair.driver)) && closed(pair.follower);
        if !in_players_space {
            // Neither door is the player's any more (a teleport, or a space unloaded under it).
            drop(&mut commands, &pair);
            continue;
        }
        let far = standing.map_or(0.0, distance);
        // The nearer of the two: in the frame of a crossing the roles are still the last frame's,
        // and the player already stands at the follower.
        let nearest = pair
            .follower
            .map_or(far, |follower| far.min(distance(follower)));
        if driver_active && nearest > DOOR_LAYER_KEEP_RADIUS {
            // The player's own door, out of reach: its native draws it again, with the same model
            // in the same pose.
            drop(&mut commands, &pair);
            continue;
        }
        if !driver_active {
            let in_view = match (eye, forward, standing) {
                (Some(eye), Some(forward), Some(door)) => {
                    doors.get(door).is_ok_and(|(_, g, ..)| {
                        !out_of_view(eye, forward, fov, aspect, g.translation())
                    })
                }
                _ => true,
            };
            if hand_back(false, both_closed, far, in_view) {
                info!(
                    door = format_args!("{:08X}", pair.ref_id),
                    distance = far,
                    in_view,
                    "door layer: the crossed door takes the model of the player's side"
                );
                drop(&mut commands, &pair);
                continue;
            }
        }
        pair.ready = copies.get(pair.copy).is_ok_and(|copy| copy.ready);
        kept.push(pair);
    }
    layer.pairs = kept;

    // New pairs: every door of the player's space near enough, whose model swings, not on the
    // layer yet.
    for (entity, global, row, _, animation, scene) in &doors {
        let Some(scene) = scene else {
            continue;
        };
        if row.auto_load
            || !animation.is_some_and(DoorAnimation::swings)
            || layer.pair_of(entity).is_some()
            || !active(entity)
            || eye.is_some_and(|eye| global.translation().distance(eye) > DOOR_LAYER_RADIUS)
        {
            continue;
        }
        let follower = twin_of(entity).filter(|twin| layer.pair_of(*twin).is_none());
        let copy = commands
            .spawn((
                Name::new(format!("Door layer {:08X}", row.ref_id)),
                DoorCopy {
                    driver: entity,
                    kind: CopyKind::Main,
                    ready: false,
                    logged: false,
                },
                DoorCopyPart,
                WorldAssetRoot(scene.0.clone()),
                Transform::from(*global),
                Visibility::default(),
                door_layers(),
            ))
            .id();
        layer.pairs.push(LayerPair {
            driver: entity,
            follower,
            copy,
            window: None,
            ready: false,
            ref_id: row.ref_id,
        });
    }

    // The doorway image's leaf: for the pair the portal draws through, while its door swings.
    let drawn = portal.open_doorway().map(|doorway| doorway.door);
    for pair in &mut layer.pairs {
        let wanted = drawn.is_some_and(|door| pair.has(door))
            && pair.ready
            && doors
                .get(pair.driver)
                .is_ok_and(|(_, _, _, state, ..)| door_has_its_own_swing(state));
        match (wanted, pair.window) {
            (true, None) => {
                let Ok((_, global, row, _, _, Some(scene))) = doors.get(pair.driver) else {
                    continue;
                };
                pair.window = Some(
                    commands
                        .spawn((
                            Name::new(format!("Door layer window {:08X}", row.ref_id)),
                            DoorCopy {
                                driver: pair.driver,
                                kind: CopyKind::Window,
                                ready: false,
                                logged: false,
                            },
                            DoorCopyPart,
                            WorldAssetRoot(scene.0.clone()),
                            Transform::from(*global),
                            Visibility::default(),
                            RenderLayers::layer(DESTINATION_LAYER),
                        ))
                        .id(),
                );
            }
            (false, Some(window)) => {
                commands.entity(window).try_despawn();
                pair.window = None;
            }
            _ => {}
        }
    }

    portal.set_door_layer_hidden(layer.hidden_natives());
}

/// A node of a door's model as [`pose_door_copies`] writes it: the same components for a node of
/// the scene and for a mesh under it.
type CopyNodeQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        Has<Mesh3d>,
        &'static mut Transform,
        &'static mut Visibility,
        Option<&'static RenderLayers>,
        Option<&'static DoorLeaf>,
        Has<DoorCopyPart>,
    ),
>;

/// What [`pose_door_copies`] reads of a driver to place its copy - the door's link and doorway
/// anchor, which build its doorway map - and to find its doorway: its state, its bounds and the
/// opening its leaves measure.
type DriverQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static LoadDoor,
        Option<&'static DoorAnchor>,
        Option<&'static DoorState>,
        Option<&'static InstanceBounds>,
        Option<&'static ExpectedModelBounds>,
        Option<&'static LeafOpening>,
    ),
>;

/// Every entity's local transform and parent, for [`world_pose`].
type LocalPoseQuery<'world, 'state> =
    Query<'world, 'state, (&'static Transform, Option<&'static ChildOf>)>;

/// An entity's world pose from its own and its ancestors' local transforms: exact in the frame a
/// rebase moved a cell root, where `GlobalTransform` is still last frame's until propagation.
fn world_pose(entity: Entity, transforms: &LocalPoseQuery) -> Option<Transform> {
    let (local, parent) = transforms.get(entity).ok()?;
    let mut pose = *local;
    let mut cursor = parent.map(ChildOf::parent);
    while let Some(current) = cursor {
        let Ok((local, parent)) = transforms.get(current) else {
            break;
        };
        pose = local.mul_transform(pose);
        cursor = parent.map(ChildOf::parent);
    }
    Some(pose)
}

/// Places and draws every door-layer copy: the root where [`door_copy_pose`] (and, for the
/// doorway image's leaf, the portal's doorway map) puts it, and every node of the instance with
/// the local transform and drawing state of the driver's node it is the second instance of.
///
/// **The pairing.** The copy and the driver are instances of one asset, so a node's
/// [`AnimationTargetId`] - a hash of its name path from the scene root - is the same in both, and
/// is what the two are paired by. A copy whose node count does not match its driver's yet (the
/// instance is still spawning) is left as it is for the frame.
///
/// **What is drawn.** The door-layer door draws the whole model - the frame is the doorway's, and
/// the natives are hidden - except the model's **filler** (`crate::portal::depth_composite`'s
/// `is_doorway_filler`: a static mesh behind the plane of the driver's own doorway, the plug the
/// game hides the void behind a closed load door with) whenever the doorway is not shut or the
/// door stands in the other space: through an open doorway it would plug the way through, and
/// carried through the doorway map it stands on the player's side of the leaf, in their face (the
/// first walk-through frames of the Sven's House crossing were nothing but its inside). The window
/// copy draws only the part the door's animation moves: the leaf, since the frame's own part behind
/// the plane is the room's and the model's filler would plug the doorway image.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pose_door_copies(
    mut commands: Commands,
    mut copies: Query<(Entity, &mut DoorCopy)>,
    layer: Option<Res<DoorLayer>>,
    portal: Option<Res<PortalState>>,
    origin: Option<Res<RenderOrigin>>,
    drivers: DriverQuery,
    parents: Query<&ChildOf>,
    leaves: Query<&DoorLeaf>,
    children: Query<&'static Children>,
    targets: Query<&'static AnimationTargetId>,
    boxes: Query<&'static Aabb>,
    mut nodes: ParamSet<(CopyNodeQuery, LocalPoseQuery)>,
) {
    let (Some(layer), Some(portal)) = (layer, portal) else {
        return;
    };
    let origin = origin.map_or(IVec2::ZERO, |origin| origin.0);
    let doorway = portal.open_doorway();
    // Each pair's door-layer pose, from the drivers' own local transforms.
    let mut poses: HashMap<Entity, Transform> = HashMap::new();
    // The driver's doorway quad in the model's own space, for the copies whose filler is hidden.
    let mut filler_quads: HashMap<Entity, Transform> = HashMap::new();
    {
        let transforms = nodes.p1();
        for pair in layer.pairs() {
            let Some(driver_pose) = world_pose(pair.driver, &transforms) else {
                continue;
            };
            let crossed = !portal.is_in_active_space(pair.driver, &parents);
            let Ok((row, anchor, state, instance_bounds, expected_bounds, leaves)) =
                drivers.get(pair.driver)
            else {
                continue;
            };
            let own_map = door_map(
                driver_pose.translation,
                driver_pose.rotation,
                driver_pose.scale,
                row,
                anchor,
                origin,
            );
            if crossed || state.is_some_and(|state| *state != DoorState::Closed) {
                let quad = doorway_quad_transform(
                    driver_pose.translation,
                    driver_pose.rotation,
                    own_map.frame,
                    driver_pose.scale,
                    instance_bounds,
                    expected_bounds,
                    anchor,
                    leaves,
                );
                let model_space = driver_pose.compute_affine().inverse() * quad.compute_affine();
                filler_quads.insert(pair.copy, Transform::from_matrix(Mat4::from(model_space)));
            }
            let map = crossed.then_some(own_map);
            let scale = pair
                .follower
                .filter(|_| crossed)
                .and_then(|follower| world_pose(follower, &transforms))
                .map_or(1.0, |follower| {
                    follower.scale.x / driver_pose.scale.x.max(f32::EPSILON)
                });
            let main = door_copy_pose(driver_pose, map.as_ref(), scale);
            poses.insert(pair.copy, main);
            if let (Some(window), Some(doorway)) = (pair.window, doorway)
                && pair.has(doorway.door)
            {
                let (translation, rotation) = doorway.map.pose(main.translation, main.rotation);
                poses.insert(
                    window,
                    Transform {
                        translation,
                        rotation,
                        scale: main.scale,
                    },
                );
            }
        }
    }

    let mut nodes = nodes.p0();
    for (root, mut copy) in &mut copies {
        let driver = copy.driver;
        if let Some(pose) = poses.get(&root)
            && let Ok((_, mut transform, ..)) = nodes.get_mut(root)
            && *transform != *pose
        {
            *transform = *pose;
        }
        // The driver's own drawing state on the root: a door the portal hides whole (a door with
        // nothing to animate that was opened) is hidden in its copy too.
        let driver_visibility = nodes.get(driver).map(|(_, _, visibility, ..)| *visibility);
        if let (Ok(wanted), Ok((_, _, mut visibility, ..))) =
            (driver_visibility, nodes.get_mut(root))
            && *visibility != wanted
        {
            *visibility = wanted;
        }
        // The nodes of the driver's model that its animation moves, by id: the leaf.
        let moved: HashSet<AnimationTargetId> = std::iter::once(driver)
            .chain(children.iter_descendants(driver))
            .filter(|node| leaves.get(*node).is_ok_and(|leaf| leaf.door == driver))
            .filter_map(|node| targets.get(node).ok().copied())
            .collect();
        let source: HashMap<AnimationTargetId, Entity> = std::iter::once(driver)
            .chain(children.iter_descendants(driver))
            .filter_map(|node| targets.get(node).ok().map(|target| (*target, node)))
            .collect();
        let (copy_nodes, driver_nodes) = (
            scene_nodes(root, &children, &targets),
            scene_nodes(driver, &children, &targets),
        );
        if copy_nodes == 0 || copy_nodes != driver_nodes {
            continue;
        }
        let mut count = CopyCount::default();
        let walk = CopyWalk {
            door: driver,
            kind: copy.kind,
            layers: match copy.kind {
                CopyKind::Main => door_layers(),
                CopyKind::Window => RenderLayers::layer(DESTINATION_LAYER),
            },
            moved: &moved,
            source: &source,
            children: &children,
            targets: &targets,
            boxes: &boxes,
            filler: filler_quads.get(&root),
        };
        walk.walk(root, None, false, &mut nodes, &mut commands, &mut count);
        let drew = match copy.kind {
            CopyKind::Main => count.drawn_meshes > 0,
            CopyKind::Window => !moved.is_empty() && count.drawn_meshes > 0,
        };
        if !copy.ready && drew {
            copy.ready = true;
        }
        if !copy.logged && count.nodes > 0 {
            copy.logged = true;
            let ref_id = drivers.get(driver).map_or(0, |(row, ..)| row.ref_id);
            info!(
                door = format_args!("{:08X}", ref_id),
                kind = ?copy.kind,
                nodes = count.nodes,
                meshes = count.meshes,
                drawn_meshes = count.drawn_meshes,
                "door layer: drawn from the door's own model"
            );
        }
    }
}

/// The nodes of a model's scene under `root`: every entity the glTF loader gave an
/// [`AnimationTargetId`], which is every node and no mesh.
fn scene_nodes(
    root: Entity,
    children: &Query<&Children>,
    targets: &Query<&AnimationTargetId>,
) -> usize {
    std::iter::once(root)
        .chain(children.iter_descendants(root))
        .filter(|node| targets.get(*node).is_ok())
        .count()
}

/// What a walk of a copy found, for the log line and the readiness.
#[derive(Default)]
struct CopyCount {
    nodes: usize,
    meshes: usize,
    drawn_meshes: usize,
}

/// One walk of a copy's instance: what it copies from, and onto which layers.
struct CopyWalk<'a, 'w, 's> {
    door: Entity,
    kind: CopyKind,
    layers: RenderLayers,
    moved: &'a HashSet<AnimationTargetId>,
    source: &'a HashMap<AnimationTargetId, Entity>,
    children: &'a Query<'w, 's, &'static Children>,
    targets: &'a Query<'w, 's, &'static AnimationTargetId>,
    boxes: &'a Query<'w, 's, &'static Aabb>,
    /// The driver's doorway quad in the model's own space, while the model's filler is hidden
    /// (the door-layer door only).
    filler: Option<&'a Transform>,
}

impl CopyWalk<'_, '_, '_> {
    /// Walks one entity of a copy: its layers, its pose and drawing state from the driver's model,
    /// its markings - then the same for everything under it.
    ///
    /// `under_leaf` is whether the entity is inside a node the driver's animation moves, and
    /// `parent` the model-space pose of its parent (`None` for the copy's root, whose own pose is
    /// where the model stands rather than part of it).
    fn walk(
        &self,
        entity: Entity,
        parent: Option<Affine3A>,
        under_leaf: bool,
        nodes: &mut CopyNodeQuery,
        commands: &mut Commands,
        count: &mut CopyCount,
    ) {
        let target = self.targets.get(entity).ok().copied();
        let paired = target.and_then(|target| self.source.get(&target).copied());
        let leaf_node = target.is_some_and(|target| self.moved.contains(&target));
        let in_leaf = under_leaf || leaf_node;
        if target.is_some() {
            count.nodes += 1;
        }
        let Ok((is_mesh, _, _, layers, leaf, part)) = nodes.get(entity) else {
            return;
        };
        let (has_layers, has_leaf, has_part) = (layers.is_some(), leaf.is_some(), part);
        let on_wrong_layers = layers.is_some_and(|layers| {
            // The door-layer door's filler is moved off the cameras by the depth composite while
            // the doorway is open, and given back by it: a layer this walk leaves alone.
            *layers != self.layers && self.kind == CopyKind::Window
        });
        if !has_layers || on_wrong_layers {
            commands.entity(entity).try_insert(self.layers.clone());
        }
        if !has_part {
            commands.entity(entity).try_insert(DoorCopyPart);
        }
        if in_leaf && !has_leaf {
            // The leaf of the copy is the driver's leaf: the slab probe and the filler test leave
            // it out, and it is drawn from both sides like the native's.
            commands
                .entity(entity)
                .try_insert(DoorLeaf { door: self.door });
        }
        // This entity's pose in the model's own space, from the local transforms as this walk
        // leaves them: exact in the frame the copy moved, where `GlobalTransform` is not.
        let local = |nodes: &CopyNodeQuery| {
            nodes
                .get(entity)
                .map(|(_, transform, ..)| transform.compute_affine())
                .unwrap_or(Affine3A::IDENTITY)
        };
        if is_mesh {
            count.meshes += 1;
            let model_space = parent.unwrap_or(Affine3A::IDENTITY) * local(nodes);
            let filler = !in_leaf
                && self.filler.is_some_and(|quad| {
                    self.boxes.get(entity).is_ok_and(|aabb| {
                        is_doorway_filler(
                            Vec3::from(model_space.transform_point3a(aabb.center)),
                            quad,
                        )
                    })
                });
            let drawn = match self.kind {
                CopyKind::Main => !filler,
                CopyKind::Window => in_leaf,
            };
            if drawn {
                count.drawn_meshes += 1;
            }
            let wanted = if drawn {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if let Ok((_, _, mut visibility, ..)) = nodes.get_mut(entity)
                && *visibility != wanted
            {
                *visibility = wanted;
            }
        } else if let Some(paired) = paired
            && paired != entity
            && let Ok([copy_node, source_node]) = nodes.get_many_mut([entity, paired])
        {
            let (_, mut copy_transform, mut copy_visibility, ..) = copy_node;
            let (_, source_transform, source_visibility, ..) = source_node;
            if *copy_transform != *source_transform {
                *copy_transform = *source_transform;
            }
            // The leaf's drawing state is the driver's (`update_door_leaves` writes it there).
            if leaf_node && *copy_visibility != *source_visibility {
                *copy_visibility = *source_visibility;
            }
        }
        let pose = match parent {
            None => Affine3A::IDENTITY,
            Some(parent) => parent * local(nodes),
        };
        if let Ok(node_children) = self.children.get(entity) {
            for child in node_children.iter() {
                self.walk(child, Some(pose), in_leaf, nodes, commands, count);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn a_door_is_handed_to_the_players_side_only_when_the_switch_cannot_be_seen() {
        // The driver is still the player's own door: nothing to hand back.
        assert!(!hand_back(true, true, 1000.0, false));
        // Crossed, but a door is open or swinging: the model stays.
        assert!(!hand_back(false, false, 1000.0, false));
        // Crossed and shut, the player right by it and looking at it: the model stays.
        assert!(!hand_back(false, true, 100.0, true));
        // Shut and a few metres away: the switch.
        assert!(hand_back(false, true, HAND_BACK_DISTANCE + 1.0, true));
        // Shut and out of view, however near: the switch.
        assert!(hand_back(false, true, 50.0, false));
    }

    #[test]
    fn a_door_behind_the_view_is_out_of_view_and_one_in_front_is_not() {
        let eye = Vec3::ZERO;
        let forward = Vec3::NEG_Z;
        let fov = std::f32::consts::FRAC_PI_4;
        let aspect = 16.0 / 9.0;
        assert!(!out_of_view(
            eye,
            forward,
            fov,
            aspect,
            Vec3::new(0.0, 0.0, -600.0)
        ));
        assert!(out_of_view(
            eye,
            forward,
            fov,
            aspect,
            Vec3::new(0.0, 0.0, 600.0)
        ));
        assert!(out_of_view(
            eye,
            forward,
            fov,
            aspect,
            Vec3::new(1000.0, 0.0, 0.0)
        ));
        // Near enough that some of it is always in reach of the view: never out of view.
        assert!(!out_of_view(
            eye,
            forward,
            fov,
            aspect,
            Vec3::new(0.0, 0.0, 200.0)
        ));
    }

    /// A door map that takes the doorway at the origin facing `+Z` onto a doorway at `arrival`,
    /// turned half a turn: the shape of every crossing, whose destination doorway faces back.
    fn half_turn_map(arrival: Vec3) -> DoorMap {
        DoorMap {
            pivot: Vec3::ZERO,
            frame: Quat::IDENTITY,
            arrival_position: arrival,
            arrival_rotation: Quat::from_rotation_y(PI),
        }
    }

    #[test]
    fn a_crossed_door_is_carried_through_the_doorway_map_the_crossing_moved_the_player_by() {
        let driver = Transform::from_xyz(10.0, 0.0, -5.0)
            .with_rotation(Quat::from_rotation_y(FRAC_PI_2))
            .with_scale(Vec3::splat(1.0));
        // The player's own door: its own pose.
        assert_eq!(door_copy_pose(driver, None, 1.0), driver);
        let arrival = Vec3::new(500.0, 20.0, 300.0);
        let map = half_turn_map(arrival);
        let pose = door_copy_pose(driver, Some(&map), 1.0);
        let (expected, rotation) = map.pose(driver.translation, driver.rotation);
        assert!((pose.translation - expected).length() < 1e-3);
        assert!(pose.rotation.angle_between(rotation) < 1e-5);
        // A point of the door and the eye both go through the same rigid map: the door stands in
        // the same place relative to the player after the crossing as before it.
        let eye = Vec3::new(0.0, 120.0, 200.0);
        let (mapped_eye, _) = map.pose(eye, Quat::IDENTITY);
        assert!(
            ((pose.translation - mapped_eye).length() - (driver.translation - eye).length()).abs()
                < 1e-3
        );
    }

    #[test]
    fn a_crossed_door_takes_the_scale_of_the_side_it_stands_in_about_that_doorway() {
        let driver = Transform::from_xyz(0.0, 0.0, -10.0);
        let arrival = Vec3::new(100.0, 0.0, 0.0);
        let map = half_turn_map(arrival);
        let pose = door_copy_pose(driver, Some(&map), 1.1);
        assert!((pose.scale - Vec3::splat(1.1)).length() < 1e-5);
        // Ten units off the source doorway: eleven off the destination doorway, in the direction
        // the rigid map carries it, at the other side's scale.
        let (rigid, _) = map.pose(driver.translation, driver.rotation);
        assert!((pose.translation - (arrival + (rigid - arrival) * 1.1)).length() < 1e-3);
        assert!(((pose.translation - arrival).length() - 11.0).abs() < 1e-3);
        // A nonsense ratio keeps the driver's own scale.
        assert_eq!(
            door_copy_pose(driver, Some(&map), f32::NAN).scale,
            Vec3::ONE
        );
    }

    fn pair(driver: Entity, follower: Option<Entity>, copy: Entity, ready: bool) -> LayerPair {
        LayerPair {
            driver,
            follower,
            copy,
            window: None,
            ready,
            ref_id: 0,
        }
    }

    #[test]
    fn both_natives_are_hidden_once_the_door_layer_door_has_drawn_and_not_before() {
        let mut world = World::new();
        let [a, b, c, d, e, f] = std::array::from_fn(|_| world.spawn_empty().id());
        let layer = DoorLayer {
            pairs: vec![pair(a, Some(b), c, true), pair(d, Some(e), f, false)],
        };
        assert_eq!(layer.hidden_natives(), HashSet::from([a, b]));
        assert_eq!(layer.driver_of(b), Some(a));
        assert_eq!(layer.driver_of(a), Some(a));
        assert!(layer.is_follower(b) && !layer.is_follower(a));
        assert_eq!(layer.drawn_copy_of(b), Some(c));
        assert_eq!(layer.drawn_copy_of(e), None, "not drawn yet");
        assert_eq!(
            layer.driven_pairs().collect::<Vec<_>>(),
            vec![(a, b), (d, e)]
        );
    }
}
