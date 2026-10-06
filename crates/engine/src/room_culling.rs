//! Interior room-and-portal culling for the main camera (upstream issue #188).
//!
//! The converter writes the rooms of an interior cell, the doorways between them and which objects
//! sit in which room (`docs/specs/converters/db-schema.md`, section 15; design
//! `docs/research/interior-room-graph-converter.md`). The tables hold facts; this module is the
//! policy that turns them into "do not draw the rooms the camera cannot see into".
//!
//! # What is drawn
//!
//! For a view, find the rooms that contain the camera and walk from them through the portals. At
//! each portal the screen-space rectangle the doorway covers is intersected with the region the
//! walk arrived with, so a room is reached only if a doorway chain leads to it inside the view.
//! An `XLRM` link (`room_links`) is an open edge: if one linked room is reached so is the other,
//! with the region unchanged.
//!
//! An object is hidden from the view only when it is contained (pivot inside the box) in exactly
//! one room and none of the rooms it belongs to - that one, plus any room its bounds merely overlap
//! (`contained = 0`) - was reached. Every other object is drawn: one that is in no room, one whose
//! pivot is in several rooms, anything outside the room graph, and everything when the camera is in
//! no room, when the cell has no rooms, or when the database has no room tables.
//!
//! # How an object is hidden, and why shadows keep it
//!
//! Bevy 0.19 has no room culling (and none is planned for 0.20; the visibility API is the same).
//! The supported hook is a system in [`VisibilitySystems::CheckVisibility`] that runs after
//! `check_visibility_cpu_culling` and edits the view's [`VisibleEntities`]
//! (`bevy_camera-0.19.0/src/visibility/mod.rs`: the system clears and refills the list at line 861
//! and is registered in the set at line 532; `bevy_render::view::visibility` then extracts the main
//! view's `RenderVisibleEntities` from it). This module removes entities from the `Mesh3d` class of
//! the camera's list and touches nothing else:
//!
//! - **Shadow views keep their casters.** A directional light's casters live in its own
//!   `CascadesVisibleEntities`, a point light's in its `CubemapVisibleEntities` and a spot light's in
//!   `VisibleMeshEntities` (`bevy_light-0.19.0/src/lib.rs`, `check_dir_light_mesh_visibility` and
//!   `check_point_light_mesh_visibility`). They are computed from the entity's `InheritedVisibility`
//!   and bounds, not from a camera's list, and this module never writes them.
//! - **`Visibility::Hidden` is not used**: it would hide the object from shadow passes and from
//!   every other camera. `ViewVisibility` is left alone for the same reason.
//! - **The light entities are not culled.** Only the `Mesh3d` class is edited, so a point light in
//!   an unreached room still reaches Bevy's light passes. Culling lights is future work.
//! - **Entities with `NoCpuCulling`** are not in any camera's [`VisibleEntities`] (they are decided
//!   on the GPU, `check_visibility_gpu_culling`, line 884), so this CPU-side filter cannot hide
//!   them. The engine does not use `NoCpuCulling` today; room culling needs a GPU-side input first
//!   if that changes.
//!
//! # What is never culled by its room
//!
//! The converter records room membership for every placed record, actors included, but a room is
//! where something *was placed*, not where it is. These are treated as in no room and stay drawn:
//!
//! - a reference carrying [`RoomCullExempt`], which `streaming::spawn_cell` inserts when the base
//!   record type is an actor, a door, an activator or a movable inventory item
//!   ([`exempt_record_type`]). **Future actor spawning hooks in here:** whatever spawns an actor
//!   or an animated door outside `spawn_cell` inserts `RoomCullExempt` on the reference entity;
//! - a mesh whose own entity, or whose reference, has a `RigidBody` that moves
//!   (`Dynamic`, `KinematicPositionBased` or `KinematicVelocityBased`). `Fixed` bodies, which is
//!   what static collision uses, are culled normally.
//!
//! This branch spawns no actors or doors yet; the rule is in place for when it does.
//!
//! # Views
//!
//! The main camera ([`StreamingCamera`]) is culled when the setting is on. Another camera, such as
//! a portal view elsewhere, opts in by carrying [`RoomCullingView`]; every other camera, and every
//! shadow view, is left untouched.
//!
//! # Not yet covered
//!
//! TODO(#188): the image-comparison test, culling on versus off on a real converted interior,
//! which proves the culled frame matches the full one. Until it exists the feature stays behind
//! `--room-culling`.
//!
//! # Coordinates
//!
//! The tables are in Creation units and Creation axes. A room box has the half-extents
//! `(half_x, half_z, half_y)` in runtime axes and the orientation
//! `creation_euler_to_runtime_quaternion(rot)`; a portal is the local X-Z rectangle in Creation axes,
//! which is the local X-Y rectangle in runtime axes. The graph is stored in the cell root's local
//! space and the walk runs there, so a render-origin rebase of the root needs no rebuild.

use crate::{
    config::EngineConfig,
    world::components::{CellRef, ExteriorCellGrid, FormId, StreamedCellRoot, StreamingCamera},
};
use bevy::{
    camera::visibility::{VisibilitySystems, VisibleEntities, check_visibility_cpu_culling},
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, futures_lite::future},
};
use bevy_rapier3d::prelude::RigidBody;
use rusqlite::{Connection, OpenFlags};
use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
    path::PathBuf,
};

/// Marks a placed reference that room culling must never hide: an actor, a door, or anything that
/// can move. See the module documentation.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RoomCullExempt;

/// Whether a base record type (the `records.record_type` of a reference's base object) is one
/// room culling must not hide: actors and their leveled lists, doors, activators, and the
/// inventory records that become loose, physics-driven objects when placed.
pub fn exempt_record_type(record_type: Option<&str>) -> bool {
    matches!(
        record_type,
        Some(
            "NPC_"
                | "LVLN"
                | "CREA"
                | "DOOR"
                | "ACTI"
                | "MISC"
                | "ALCH"
                | "AMMO"
                | "ARMO"
                | "BOOK"
                | "INGR"
                | "KEYM"
                | "SCRL"
                | "SLGM"
                | "WEAP"
        )
    )
}

/// Opt-in marker for a camera, other than the main one, that culls by room (a portal view).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RoomCullingView;

/// A doorway closer than this to the camera counts as fully open, in Creation units (about 70 per
/// metre, so 8 is roughly 11 cm).
///
/// The camera has Bevy's default 0.1 near plane, so a doorway further than that from the camera
/// still projects to something meaningful and the clipping in [`project_portal`] handles it. The
/// margin is for the camera standing in or touching the doorway, where the rectangle is edge-on or
/// straddles the lens and a projected rectangle is numerically unreliable. It is also a few units
/// of slack for marker placement. Too large a value only draws more (the walk stays conservative);
/// 8 covers the player's body leaning into a doorway without opening doorways the player is
/// clearly beside. The image comparison against a real interior is where to retune it.
const PORTAL_OPEN_DISTANCE: f32 = 8.0;

/// Smallest clip-space `w` kept when clipping a doorway against the camera; anything nearer is
/// behind the lens.
const CLIP_W_EPSILON: f32 = 1.0e-3;

/// The owner cache is emptied past this many entries, which drops the despawned meshes in it.
const OWNER_CACHE_LIMIT: usize = 100_000;

/// A room may widen its visible region this many times before it is given the whole screen, which
/// bounds the walk on cyclic portal graphs.
const MAX_REGION_WIDENINGS: u8 = 4;

/// One `room_bounds` row in cell-local runtime space.
#[derive(Debug, Clone, PartialEq)]
pub struct Room {
    /// `room_bounds.ref_id`.
    pub ref_id: u32,
    pub center: Vec3,
    pub rotation: Quat,
    /// Half-extents along the room's own runtime axes.
    pub half: Vec3,
}

impl Room {
    /// A room from table values: Creation position, Euler angles and half-extents.
    pub fn from_creation(
        ref_id: u32,
        position: [f32; 3],
        rotation: [f32; 3],
        half: [f32; 3],
    ) -> Self {
        Self {
            ref_id,
            center: Vec3::from_array(shared::coordinates::creation_to_runtime_vector(position)),
            rotation: Quat::from_array(shared::coordinates::creation_euler_to_runtime_quaternion(
                rotation,
            )),
            // Creation (x, y, z) are runtime (x, z, -y): a half-extent keeps its size.
            half: Vec3::new(half[0], half[2], half[1]),
        }
    }

    /// Whether a cell-local point is inside the box, edges included.
    pub fn contains(&self, point: Vec3) -> bool {
        let local = self.rotation.inverse() * (point - self.center);
        local.abs().cmple(self.half).all()
    }
}

/// One `room_portals` row in cell-local runtime space.
#[derive(Debug, Clone, PartialEq)]
pub struct Portal {
    /// `room_portals.ref_id`.
    pub ref_id: u32,
    /// The doorway's corners in order around the rectangle.
    pub corners: [Vec3; 4],
    /// The room indices on either side; `None` for a side the converter left empty.
    pub rooms: [Option<usize>; 2],
}

impl Portal {
    /// A portal from table values. `rooms` are `room_bounds.ref_id`s looked up in `room_index`.
    pub fn from_creation(
        ref_id: u32,
        position: [f32; 3],
        rotation: [f32; 3],
        half: [f32; 2],
        rooms: [Option<usize>; 2],
    ) -> Self {
        let center = Vec3::from_array(shared::coordinates::creation_to_runtime_vector(position));
        let rotation = Quat::from_array(shared::coordinates::creation_euler_to_runtime_quaternion(
            rotation,
        ));
        // The doorway is the marker's local X-Z rectangle in Creation axes: runtime local X and Y.
        let x = rotation * Vec3::X * half[0];
        let y = rotation * Vec3::Y * half[1];
        Self {
            ref_id,
            corners: [
                center - x - y,
                center + x - y,
                center + x + y,
                center - x + y,
            ],
            rooms,
        }
    }

    fn center(&self) -> Vec3 {
        self.corners.iter().copied().sum::<Vec3>() / 4.0
    }

    /// Distance from a point to the doorway rectangle.
    fn distance_to(&self, point: Vec3) -> f32 {
        let center = self.center();
        let along_x = self.corners[1] - self.corners[0];
        let along_y = self.corners[3] - self.corners[0];
        let (half_x, half_y) = (along_x.length() * 0.5, along_y.length() * 0.5);
        let (axis_x, axis_y) = (along_x.normalize_or_zero(), along_y.normalize_or_zero());
        let normal = axis_x.cross(axis_y).normalize_or_zero();
        let offset = point - center;
        let dx = (offset.dot(axis_x).abs() - half_x).max(0.0);
        let dy = (offset.dot(axis_y).abs() - half_y).max(0.0);
        let dn = offset.dot(normal).abs();
        Vec3::new(dx, dy, dn).length()
    }
}

/// Which rooms an object belongs to, as indices into [`RoomGraph::rooms`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Membership {
    /// Rooms whose box holds the object's pivot (`contained = 1`).
    pub contained: Vec<usize>,
    /// Rooms whose box only overlaps the object's bounds (`contained = 0`).
    pub overlapping: Vec<usize>,
}

/// The room graph of one interior cell, in the space of the cell root.
#[derive(Debug, Clone, Default)]
pub struct RoomGraph {
    pub rooms: Vec<Room>,
    pub portals: Vec<Portal>,
    /// `room_links` as undirected edges between room indices.
    pub links: Vec<(usize, usize)>,
    /// Membership by placed-reference id. The key is one number under three names: the `FormId`
    /// component of a spawned reference (`streaming::spawn_cell` inserts `FormId(reference.form_id)`),
    /// `ReferenceRow::form_id`, which the world database loader fills from `references.id`
    /// (`world::database::REFERENCE_COLUMNS` starts with `r.id`), and `reference_rooms.reference_id`.
    /// It is the placement's id, not the base object's `base_form_id`. A test proves the chain
    /// through the real loader (`the_membership_key_is_the_reference_id_the_loader_delivers`).
    pub membership: HashMap<u32, Membership>,
}

/// An axis-aligned rectangle in normalised device coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    min: Vec2,
    max: Vec2,
}

impl Rect {
    const FULL: Rect = Rect {
        min: Vec2::splat(-1.0),
        max: Vec2::splat(1.0),
    };

    fn intersect(self, other: Rect) -> Option<Rect> {
        let min = self.min.max(other.min);
        let max = self.max.min(other.max);
        (min.x <= max.x && min.y <= max.y).then_some(Rect { min, max })
    }

    fn union(self, other: Rect) -> Rect {
        Rect {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    fn contains(self, other: Rect) -> bool {
        self.min.cmple(other.min).all() && self.max.cmpge(other.max).all()
    }
}

/// The screen rectangle a doorway covers, or `None` when it is entirely behind the camera.
fn project_portal(corners: &[Vec3; 4], clip_from_local: Mat4) -> Option<Rect> {
    let clip: Vec<Vec4> = corners
        .iter()
        .map(|corner| clip_from_local * corner.extend(1.0))
        .collect();
    // Sutherland-Hodgman against the plane w = epsilon, so a doorway that crosses the lens still
    // projects to the part in front of it.
    let mut polygon: Vec<Vec4> = Vec::with_capacity(5);
    for index in 0..clip.len() {
        let (current, next) = (clip[index], clip[(index + 1) % clip.len()]);
        let (current_in, next_in) = (current.w >= CLIP_W_EPSILON, next.w >= CLIP_W_EPSILON);
        if current_in {
            polygon.push(current);
        }
        if current_in != next_in {
            let t = (CLIP_W_EPSILON - current.w) / (next.w - current.w);
            polygon.push(current + (next - current) * t);
        }
    }
    if polygon.is_empty() {
        return None;
    }
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for point in polygon {
        let ndc = Vec2::new(point.x, point.y) / point.w;
        min = min.min(ndc);
        max = max.max(ndc);
    }
    (min.is_finite() && max.is_finite()).then_some(Rect { min, max })
}

impl RoomGraph {
    /// Indices of the rooms whose box holds a cell-local point, in table order.
    pub fn rooms_containing(&self, point: Vec3) -> Vec<usize> {
        self.rooms
            .iter()
            .enumerate()
            .filter(|(_, room)| room.contains(point))
            .map(|(index, _)| index)
            .collect()
    }

    /// The rooms reachable from the rooms around `camera`, through doorways that lie inside the
    /// view. `None` when the camera is in no room, which means draw everything. `clip_from_local`
    /// maps cell-local space to clip space.
    ///
    /// Every room that contains the camera is a start, which is the deterministic superset of
    /// "pick one": a camera in a doorway is in two boxes and sees into both.
    pub fn reached_rooms(&self, camera: Vec3, clip_from_local: Mat4) -> Option<Vec<bool>> {
        let starts = self.rooms_containing(camera);
        if starts.is_empty() {
            return None;
        }
        let mut doorways: Vec<Vec<usize>> = vec![Vec::new(); self.rooms.len()];
        for (index, portal) in self.portals.iter().enumerate() {
            if let [Some(a), Some(b)] = portal.rooms
                && a != b
            {
                doorways[a].push(index);
                doorways[b].push(index);
            }
        }
        let mut linked: Vec<Vec<usize>> = vec![Vec::new(); self.rooms.len()];
        for &(a, b) in &self.links {
            if a != b && a < linked.len() && b < linked.len() {
                linked[a].push(b);
                linked[b].push(a);
            }
        }
        let mut region: Vec<Option<Rect>> = vec![None; self.rooms.len()];
        let mut widenings = vec![0u8; self.rooms.len()];
        let mut pending: Vec<(usize, Rect)> =
            starts.into_iter().map(|room| (room, Rect::FULL)).collect();
        while let Some((room, arriving)) = pending.pop() {
            let seen = match region[room] {
                None => arriving,
                Some(old) if old.contains(arriving) => continue,
                Some(old) => {
                    widenings[room] += 1;
                    if widenings[room] > MAX_REGION_WIDENINGS {
                        Rect::FULL
                    } else {
                        old.union(arriving)
                    }
                }
            };
            region[room] = Some(seen);
            for &portal_index in &doorways[room] {
                let portal = &self.portals[portal_index];
                let [Some(a), Some(b)] = portal.rooms else {
                    continue;
                };
                let other = if a == room { b } else { a };
                let through = if portal.distance_to(camera) <= PORTAL_OPEN_DISTANCE {
                    Some(seen)
                } else {
                    project_portal(&portal.corners, clip_from_local)
                        .and_then(|doorway| doorway.intersect(seen))
                };
                if let Some(through) = through {
                    pending.push((other, through));
                }
            }
            for &other in &linked[room] {
                pending.push((other, seen));
            }
        }
        Some(region.iter().map(Option::is_some).collect())
    }

    /// Whether the object placed by `form_id` is drawn when `reached` rooms are visible.
    pub fn is_drawn(&self, form_id: u32, reached: &[bool]) -> bool {
        let Some(membership) = self.membership.get(&form_id) else {
            return true;
        };
        if membership.contained.len() != 1 {
            return true;
        }
        membership
            .contained
            .iter()
            .chain(&membership.overlapping)
            .any(|&room| reached.get(room).copied().unwrap_or(true))
    }
}

/// The five tables of section 15 that the reader needs. `room_graph_stats` is a report counter and
/// is not required.
const REQUIRED_TABLES: [&str; 4] = [
    "room_bounds",
    "room_portals",
    "room_links",
    "reference_rooms",
];

/// Whether the database carries the room tables. A database converted before them still loads and
/// the feature is simply off.
pub fn room_tables_present(connection: &Connection) -> rusqlite::Result<bool> {
    let mut statement = connection
        .prepare_cached("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1")?;
    for table in REQUIRED_TABLES {
        if statement.query_row([table], |row| row.get::<_, i64>(0))? == 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Reads the room graph of one interior cell. The tables must be present
/// ([`room_tables_present`]); a cell without rooms yields an empty graph.
pub fn load_room_graph(connection: &Connection, cell_id: u32) -> rusqlite::Result<RoomGraph> {
    let mut graph = RoomGraph::default();
    let mut room_index: HashMap<u32, usize> = HashMap::new();
    {
        let mut statement = connection.prepare_cached(
            "SELECT ref_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,half_x,half_y,half_z \
             FROM room_bounds WHERE cell_id=?1 ORDER BY ref_id",
        )?;
        let rows = statement.query_map([cell_id], |row| {
            Ok(Room::from_creation(
                row.get(0)?,
                [row.get(1)?, row.get(2)?, row.get(3)?],
                [row.get(4)?, row.get(5)?, row.get(6)?],
                [row.get(7)?, row.get(8)?, row.get(9)?],
            ))
        })?;
        for room in rows {
            let room = room?;
            room_index.insert(room.ref_id, graph.rooms.len());
            graph.rooms.push(room);
        }
    }
    if graph.rooms.is_empty() {
        return Ok(graph);
    }
    {
        let mut statement = connection.prepare_cached(
            "SELECT ref_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,half_x,half_z,room_a,room_b \
             FROM room_portals WHERE cell_id=?1 ORDER BY ref_id",
        )?;
        let rows = statement.query_map([cell_id], |row| {
            let side = |index: usize| -> rusqlite::Result<Option<usize>> {
                Ok(row
                    .get::<_, Option<u32>>(index)?
                    .and_then(|id| room_index.get(&id).copied()))
            };
            Ok(Portal::from_creation(
                row.get(0)?,
                [row.get(1)?, row.get(2)?, row.get(3)?],
                [row.get(4)?, row.get(5)?, row.get(6)?],
                [row.get(7)?, row.get(8)?],
                [side(9)?, side(10)?],
            ))
        })?;
        for portal in rows {
            graph.portals.push(portal?);
        }
    }
    {
        let mut statement = connection.prepare_cached(
            "SELECT l.room_ref,l.linked_ref FROM room_links l \
             JOIN room_bounds b ON b.ref_id=l.room_ref WHERE b.cell_id=?1 \
             ORDER BY l.room_ref,l.linked_ref",
        )?;
        let rows = statement.query_map([cell_id], |row| {
            Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?))
        })?;
        for link in rows {
            let (room, linked) = link?;
            // The target is stored as written, even when it is not a room of this cell.
            if let (Some(&a), Some(&b)) = (room_index.get(&room), room_index.get(&linked)) {
                graph.links.push((a, b));
            }
        }
    }
    {
        let mut statement = connection.prepare_cached(
            "SELECT m.reference_id,m.room_ref,m.contained FROM reference_rooms m \
             JOIN room_bounds b ON b.ref_id=m.room_ref WHERE b.cell_id=?1 \
             ORDER BY m.reference_id,m.room_ref",
        )?;
        let rows = statement.query_map([cell_id], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, i64>(2)? != 0,
            ))
        })?;
        for row in rows {
            let (reference, room, contained) = row?;
            let Some(&room) = room_index.get(&room) else {
                continue;
            };
            let membership = graph.membership.entry(reference).or_default();
            if contained {
                membership.contained.push(room);
            } else {
                membership.overlapping.push(room);
            }
        }
    }
    Ok(graph)
}

/// Whether the database has the room tables, once known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableProbe {
    Unknown,
    Present,
    Absent,
}

/// A loaded interior cell root: a streamed cell without an exterior grid position.
type InteriorRoot = (With<StreamedCellRoot>, Without<ExteriorCellGrid>);

/// What a background read of one cell's room graph found.
type LoadResult = Result<Option<RoomGraph>, String>;

struct LoadedGraph {
    root: Entity,
    graph: RoomGraph,
}

/// The settings, the probe result and the graphs of the interior cells that are loaded.
#[derive(Resource)]
pub struct RoomCullingState {
    enabled: bool,
    database_path: PathBuf,
    probe: TableProbe,
    graphs: HashMap<u32, LoadedGraph>,
    pending: HashMap<u32, Task<LoadResult>>,
    /// Cells whose read failed; they are not retried.
    failed: HashSet<u32>,
    /// Entities removed from the main view in the last frame, for diagnostics.
    pub culled_last_frame: usize,
}

impl RoomCullingState {
    fn new(enabled: bool, database_path: PathBuf) -> Self {
        Self {
            enabled,
            database_path,
            probe: TableProbe::Unknown,
            graphs: HashMap::new(),
            pending: HashMap::new(),
            failed: HashSet::new(),
            culled_last_frame: 0,
        }
    }

    /// Whether the feature is on and the database has the tables (or has not been probed yet).
    pub fn active(&self) -> bool {
        self.enabled && self.probe != TableProbe::Absent
    }

    /// Registers a graph for a loaded cell root. Tests only: the loader goes through
    /// `collect_loaded_graphs`, which also records what the probe found.
    #[cfg(test)]
    pub fn insert_graph(&mut self, cell_id: u32, root: Entity, graph: RoomGraph) {
        self.graphs.insert(cell_id, LoadedGraph { root, graph });
    }
}

/// Registers room culling. With `enabled` false the plugin adds no system: a run that does not ask
/// for the feature is exactly as it was.
pub struct RoomCullingPlugin {
    pub enabled: bool,
    pub database_path: PathBuf,
}

impl RoomCullingPlugin {
    pub fn from_config(config: &EngineConfig) -> Self {
        Self {
            enabled: config.room_culling,
            database_path: config.assets_dir.join("skyrim_world.db"),
        }
    }
}

impl Plugin for RoomCullingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(RoomCullingState::new(
            self.enabled,
            self.database_path.clone(),
        ));
        if !self.enabled {
            return;
        }
        app.add_systems(
            Update,
            (
                collect_loaded_graphs,
                request_room_graphs,
                drop_stale_graphs,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            cull_room_views
                .in_set(VisibilitySystems::CheckVisibility)
                // The list is only complete once Bevy's own check has filled it.
                .after(check_visibility_cpu_culling),
        );
    }
}

/// Starts a background read for every loaded interior cell that has no graph yet.
fn request_room_graphs(
    mut state: ResMut<RoomCullingState>,
    roots: Query<(Entity, &CellRef), InteriorRoot>,
) {
    if !state.active() {
        return;
    }
    for (_, cell) in &roots {
        let cell_id = cell.0;
        if state.graphs.contains_key(&cell_id)
            || state.pending.contains_key(&cell_id)
            || state.failed.contains(&cell_id)
        {
            continue;
        }
        let path = state.database_path.clone();
        let task = AsyncComputeTaskPool::get().spawn(async move { read_graph(&path, cell_id) });
        state.pending.insert(cell_id, task);
    }
}

fn read_graph(path: &std::path::Path, cell_id: u32) -> LoadResult {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    if !room_tables_present(&connection).map_err(|error| error.to_string())? {
        return Ok(None);
    }
    load_room_graph(&connection, cell_id)
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Takes the finished background reads.
fn collect_loaded_graphs(
    mut state: ResMut<RoomCullingState>,
    roots: Query<(Entity, &CellRef), InteriorRoot>,
) {
    let finished: Vec<(u32, LoadResult)> = state
        .pending
        .iter_mut()
        .filter_map(|(&cell, task)| {
            future::block_on(future::poll_once(task)).map(|result| (cell, result))
        })
        .collect();
    for (cell_id, result) in finished {
        state.pending.remove(&cell_id);
        match result {
            Ok(None) => {
                if state.probe != TableProbe::Absent {
                    info!(
                        "room culling: {} has no room tables (converted before the room graph); \
                         the feature stays off and every object is drawn",
                        state.database_path.display()
                    );
                }
                state.probe = TableProbe::Absent;
            }
            Ok(Some(graph)) => {
                if state.probe == TableProbe::Unknown {
                    info!("room culling: room tables found");
                }
                state.probe = TableProbe::Present;
                let Some(root) = roots
                    .iter()
                    .find_map(|(entity, cell)| (cell.0 == cell_id).then_some(entity))
                else {
                    continue;
                };
                info!(
                    "room culling: cell {cell_id:08X} has {} rooms, {} portals, {} links, {} placed objects",
                    graph.rooms.len(),
                    graph.portals.len(),
                    graph.links.len(),
                    graph.membership.len()
                );
                state.graphs.insert(cell_id, LoadedGraph { root, graph });
            }
            Err(error) => {
                warn!("room culling: cell {cell_id:08X} room graph not read: {error}");
                state.failed.insert(cell_id);
            }
        }
    }
}

/// Forgets graphs, failures and reads of cells whose root was unloaded; a cell that comes back
/// is read again.
fn drop_stale_graphs(
    mut state: ResMut<RoomCullingState>,
    roots: Query<(Entity, &CellRef), With<StreamedCellRoot>>,
) {
    if state.graphs.is_empty() && state.failed.is_empty() && state.pending.is_empty() {
        return;
    }
    let live: HashSet<u32> = roots.iter().map(|(_, cell)| cell.0).collect();
    state
        .graphs
        .retain(|_, loaded| roots.get(loaded.root).is_ok());
    state.failed.retain(|cell| live.contains(cell));
    // Dropping a task cancels it.
    state.pending.retain(|cell, _| live.contains(cell));
}

/// Removes the objects of unreached rooms from each room-culling view's `Mesh3d` list.
#[allow(clippy::type_complexity)]
pub fn cull_room_views(
    mut state: ResMut<RoomCullingState>,
    roots: Query<&GlobalTransform, With<StreamedCellRoot>>,
    mut views: Query<
        (
            &GlobalTransform,
            &Projection,
            Option<&Camera>,
            &mut VisibleEntities,
        ),
        Or<(With<StreamingCamera>, With<RoomCullingView>)>,
    >,
    parents: Query<&ChildOf>,
    ids: Query<&FormId>,
    exemptions: Query<(Option<&RigidBody>, Has<RoomCullExempt>)>,
    mut owners: Local<HashMap<Entity, (u32, Entity)>>,
) {
    state.culled_last_frame = 0;
    if !state.enabled || state.graphs.is_empty() {
        return;
    }
    let mut cells: Vec<u32> = state.graphs.keys().copied().collect();
    cells.sort_unstable();
    let mut culled = 0;
    for (camera_transform, projection, camera, mut visible) in &mut views {
        if camera.is_some_and(|camera| !camera.is_active) {
            continue;
        }
        let clip_from_world =
            projection.get_clip_from_view() * camera_transform.to_matrix().inverse();
        for cell in &cells {
            let loaded = &state.graphs[cell];
            let Ok(root) = roots.get(loaded.root) else {
                continue;
            };
            let world_from_local = root.to_matrix();
            let camera_local = root
                .affine()
                .inverse()
                .transform_point3(camera_transform.translation());
            let Some(reached) = loaded
                .graph
                .reached_rooms(camera_local, clip_from_world * world_from_local)
            else {
                continue;
            };
            let list = visible.get_mut(TypeId::of::<Mesh3d>());
            let before = list.len();
            // The owner of a mesh does not change while it lives, so a found owner is remembered
            // (entity ids carry a generation, so a recycled slot is a different key). A mesh with
            // no owner is not cached: its hierarchy may still be being built.
            if owners.len() > OWNER_CACHE_LIMIT {
                owners.clear();
            }
            list.retain(|&entity| {
                let owner = match owners.get(&entity) {
                    Some(&owner) => Some(owner),
                    None => {
                        let found = owner_form_id(entity, &parents, &ids);
                        if let Some(owner) = found {
                            owners.insert(entity, owner);
                        }
                        found
                    }
                };
                let Some((form_id, reference)) = owner else {
                    return true;
                };
                // Re-read every frame: a body can be added after the mesh spawns.
                let exempted = |entity| {
                    exemptions
                        .get(entity)
                        .is_ok_and(|(body, marked)| marked || body.is_some_and(|b| body_moves(*b)))
                };
                if exempted(reference) || exempted(entity) {
                    return true;
                }
                loaded.graph.is_drawn(form_id, &reached)
            });
            culled += before - list.len();
            // One cell holds the camera; the rest cannot also contain it.
            break;
        }
    }
    state.culled_last_frame = culled;
}

/// Whether a rigid body can move, as opposed to the `Fixed` bodies static collision uses.
fn body_moves(body: RigidBody) -> bool {
    !matches!(body, RigidBody::Fixed)
}

/// The FormID of the placed reference a mesh entity belongs to, and that reference's entity: the
/// entity's own, or its nearest ancestor's.
fn owner_form_id(
    entity: Entity,
    parents: &Query<&ChildOf>,
    ids: &Query<&FormId>,
) -> Option<(u32, Entity)> {
    let mut current = entity;
    for _ in 0..32 {
        if let Ok(id) = ids.get(current) {
            return Some((id.0, current));
        }
        current = parents.get(current).ok()?.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::database::{CellKey, DatabaseRequest, WorldDatabase};
    use bevy::{
        asset::AssetPlugin,
        camera::{
            primitives::{Aabb, Frustum},
            visibility::{
                CubemapVisibleEntities, ViewVisibility, VisibilityPlugin, VisibleMeshEntities,
            },
        },
        ecs::system::RunSystemOnce,
        math::primitives::ViewFrustum,
        transform::TransformPlugin,
    };

    /// The section 15 tables, as the converter writes them.
    const SCHEMA: &str = "
        CREATE TABLE room_bounds (
            ref_id INTEGER PRIMARY KEY, cell_id INTEGER NOT NULL,
            pos_x REAL NOT NULL, pos_y REAL NOT NULL, pos_z REAL NOT NULL,
            rot_x REAL NOT NULL, rot_y REAL NOT NULL, rot_z REAL NOT NULL,
            half_x REAL NOT NULL, half_y REAL NOT NULL, half_z REAL NOT NULL,
            lighting_template_id INTEGER, image_space_id INTEGER,
            flags INTEGER NOT NULL DEFAULT 0);
        CREATE TABLE room_portals (
            ref_id INTEGER PRIMARY KEY, cell_id INTEGER NOT NULL,
            pos_x REAL NOT NULL, pos_y REAL NOT NULL, pos_z REAL NOT NULL,
            rot_x REAL NOT NULL, rot_y REAL NOT NULL, rot_z REAL NOT NULL,
            half_x REAL NOT NULL, half_z REAL NOT NULL,
            room_a INTEGER, room_b INTEGER);
        CREATE TABLE room_links (
            room_ref INTEGER NOT NULL, linked_ref INTEGER NOT NULL,
            PRIMARY KEY (room_ref, linked_ref));
        CREATE TABLE reference_rooms (
            reference_id INTEGER NOT NULL, room_ref INTEGER NOT NULL, contained INTEGER NOT NULL,
            PRIMARY KEY (reference_id, room_ref)) WITHOUT ROWID;
        CREATE TABLE room_graph_stats (name TEXT PRIMARY KEY, value INTEGER NOT NULL);";

    fn database() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        connection
    }

    fn insert_room(connection: &Connection, id: u32, cell: u32, position: [f32; 3]) {
        connection
            .execute(
                "INSERT INTO room_bounds (ref_id,cell_id,pos_x,pos_y,pos_z,rot_x,rot_y,rot_z,half_x,half_y,half_z) \
                 VALUES (?1,?2,?3,?4,?5,0,0,0,10,20,5)",
                rusqlite::params![id, cell, position[0], position[1], position[2]],
            )
            .unwrap();
    }

    #[test]
    fn a_database_without_the_tables_has_the_feature_off() {
        let connection = Connection::open_in_memory().unwrap();
        assert!(!room_tables_present(&connection).unwrap());
        // One table is not enough.
        connection
            .execute_batch("CREATE TABLE room_bounds (ref_id INTEGER);")
            .unwrap();
        assert!(!room_tables_present(&connection).unwrap());
    }

    #[test]
    fn a_database_with_the_tables_is_detected() {
        assert!(room_tables_present(&database()).unwrap());
    }

    #[test]
    fn a_cell_without_rooms_loads_as_an_empty_graph() {
        let connection = database();
        insert_room(&connection, 1, 7, [0.0; 3]);
        let graph = load_room_graph(&connection, 99).unwrap();
        assert!(graph.rooms.is_empty() && graph.portals.is_empty() && graph.membership.is_empty());
        // And an empty graph never hides anything.
        assert!(graph.reached_rooms(Vec3::ZERO, Mat4::IDENTITY).is_none());
        assert!(graph.is_drawn(5, &[]));
    }

    #[test]
    fn the_graph_is_read_in_runtime_axes() {
        let connection = database();
        insert_room(&connection, 1, 7, [100.0, 200.0, 50.0]);
        insert_room(&connection, 2, 7, [300.0, 200.0, 50.0]);
        insert_room(&connection, 3, 8, [0.0; 3]);
        connection
            .execute(
                "INSERT INTO room_portals VALUES (10,7,200,200,50,0,0,0,4,6,1,2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO room_portals VALUES (11,7,0,0,0,0,0,0,4,6,1,NULL)",
                [],
            )
            .unwrap();
        connection
            .execute("INSERT INTO room_links VALUES (1,2),(1,9999),(3,1)", [])
            .unwrap();
        connection
            .execute(
                "INSERT INTO reference_rooms VALUES (500,1,1),(500,2,0),(501,2,1),(502,3,1)",
                [],
            )
            .unwrap();
        let graph = load_room_graph(&connection, 7).unwrap();
        assert_eq!(graph.rooms.len(), 2);
        // Creation (100, 200, 50) is runtime (100, 50, -200); half-extents (10, 20, 5) become
        // (10, 5, 20).
        assert_eq!(graph.rooms[0].center, Vec3::new(100.0, 50.0, -200.0));
        assert_eq!(graph.rooms[0].half, Vec3::new(10.0, 5.0, 20.0));
        assert!(graph.rooms[0].contains(Vec3::new(105.0, 52.0, -190.0)));
        assert!(!graph.rooms[0].contains(Vec3::new(105.0, 60.0, -190.0)));
        assert_eq!(graph.portals.len(), 2);
        // The doorway is the local X-Z rectangle in Creation axes: runtime X by runtime Y.
        let portal = &graph.portals[0];
        assert_eq!(portal.rooms, [Some(0), Some(1)]);
        assert!(portal.corners[0].distance(Vec3::new(196.0, 44.0, -200.0)) < 1.0e-3);
        assert!(portal.corners[2].distance(Vec3::new(204.0, 56.0, -200.0)) < 1.0e-3);
        // A missing side stays missing.
        assert_eq!(graph.portals[1].rooms, [Some(0), None]);
        // Links outside the cell, or to a non-room, are dropped; the one from another cell too.
        assert_eq!(graph.links, vec![(0, 1)]);
        // The other cell's reference is not in this cell's membership.
        assert!(!graph.membership.contains_key(&502));
        assert_eq!(
            graph.membership[&500],
            Membership {
                contained: vec![0],
                overlapping: vec![1]
            }
        );
    }

    // --- Culling on synthetic graphs ---------------------------------------------------------

    fn room(ref_id: u32, center: Vec3, half: f32) -> Room {
        Room {
            ref_id,
            center,
            rotation: Quat::IDENTITY,
            half: Vec3::splat(half),
        }
    }

    /// A doorway in an XY plane at `center`, `half` wide and high, between two rooms.
    fn doorway(ref_id: u32, center: Vec3, half: f32, rooms: [usize; 2]) -> Portal {
        Portal {
            ref_id,
            corners: [
                center + Vec3::new(-half, -half, 0.0),
                center + Vec3::new(half, -half, 0.0),
                center + Vec3::new(half, half, 0.0),
                center + Vec3::new(-half, half, 0.0),
            ],
            rooms: [Some(rooms[0]), Some(rooms[1])],
        }
    }

    /// A doorway in a YZ plane (a side wall).
    fn side_doorway(ref_id: u32, center: Vec3, half: f32, rooms: [usize; 2]) -> Portal {
        Portal {
            ref_id,
            corners: [
                center + Vec3::new(0.0, -half, -half),
                center + Vec3::new(0.0, -half, half),
                center + Vec3::new(0.0, half, half),
                center + Vec3::new(0.0, half, -half),
            ],
            rooms: [Some(rooms[0]), Some(rooms[1])],
        }
    }

    /// The camera sits at the origin looking down -Z with a 60 degree square view.
    fn clip_from_local() -> Mat4 {
        Mat4::perspective_infinite_reverse_rh(std::f32::consts::FRAC_PI_3, 1.0, 0.1)
    }

    /// Rooms A (0), B (1) and C (2): A at the camera, B behind the doorway at z = -10 and C behind
    /// a doorway in B's far wall at `far_door`.
    fn chain(far_door: Portal) -> RoomGraph {
        RoomGraph {
            rooms: vec![
                room(1, Vec3::ZERO, 10.0),
                room(2, Vec3::new(0.0, 0.0, -20.0), 10.0),
                room(3, Vec3::new(0.0, 0.0, -40.0), 10.0),
            ],
            portals: vec![
                doorway(10, Vec3::new(0.0, 0.0, -10.0), 3.0, [0, 1]),
                far_door,
            ],
            links: vec![],
            membership: HashMap::new(),
        }
    }

    #[test]
    fn a_doorway_in_view_reaches_the_room_behind_it() {
        let graph = chain(doorway(11, Vec3::new(0.0, 0.0, -30.0), 3.0, [1, 2]));
        let reached = graph.reached_rooms(Vec3::ZERO, clip_from_local()).unwrap();
        assert_eq!(reached, vec![true, true, true]);
    }

    #[test]
    fn a_doorway_outside_the_frustum_leaves_its_room_culled() {
        // The doorway sits in B's side wall at 40 degrees to the right: outside a 30 degree
        // half-angle view.
        let graph = chain(side_doorway(11, Vec3::new(10.0, 0.0, -12.0), 1.5, [1, 2]));
        let reached = graph.reached_rooms(Vec3::ZERO, clip_from_local()).unwrap();
        assert_eq!(reached, vec![true, true, false]);
    }

    #[test]
    fn the_region_narrows_at_each_doorway() {
        // The far doorway is inside the screen but outside the part of it that the near doorway
        // lets through.
        let graph = chain(doorway(11, Vec3::new(12.0, 0.0, -30.0), 1.0, [1, 2]));
        let reached = graph.reached_rooms(Vec3::ZERO, clip_from_local()).unwrap();
        assert_eq!(reached, vec![true, true, false]);
        // Moved inside the narrowed region, it is reached.
        let graph = chain(doorway(11, Vec3::new(4.0, 0.0, -30.0), 1.0, [1, 2]));
        let reached = graph.reached_rooms(Vec3::ZERO, clip_from_local()).unwrap();
        assert_eq!(reached, vec![true, true, true]);
    }

    #[test]
    fn a_doorway_behind_the_camera_is_not_taken() {
        let mut graph = chain(doorway(11, Vec3::new(0.0, 0.0, -30.0), 3.0, [1, 2]));
        // Turn the camera round by mirroring the scene's Z: the near doorway is now behind it.
        let around = Mat4::from_rotation_y(std::f32::consts::PI);
        graph.portals[0] = doorway(10, Vec3::new(0.0, 0.0, 10.0), 3.0, [0, 1]);
        let reached = graph.reached_rooms(Vec3::ZERO, clip_from_local()).unwrap();
        assert_eq!(reached, vec![true, false, false]);
        // Looking the other way takes it.
        let reached = graph
            .reached_rooms(Vec3::ZERO, clip_from_local() * around)
            .unwrap();
        assert!(reached[1]);
    }

    #[test]
    fn a_link_is_an_open_edge() {
        let mut graph = chain(doorway(11, Vec3::new(0.0, 0.0, -30.0), 3.0, [1, 2]));
        // No portal reaches C from the camera's side, but A is linked to it.
        graph.portals.clear();
        let reached = graph.reached_rooms(Vec3::ZERO, clip_from_local()).unwrap();
        assert_eq!(reached, vec![true, false, false]);
        graph.links.push((0, 2));
        let reached = graph.reached_rooms(Vec3::ZERO, clip_from_local()).unwrap();
        assert_eq!(reached, vec![true, false, true]);
        // The link is symmetric: from C the camera's own room is reached too.
        let reached = graph
            .reached_rooms(Vec3::new(0.0, 0.0, -40.0), clip_from_local())
            .unwrap();
        assert_eq!(reached, vec![true, false, true]);
    }

    #[test]
    fn a_camera_in_no_room_culls_nothing() {
        let graph = chain(doorway(11, Vec3::new(0.0, 0.0, -30.0), 3.0, [1, 2]));
        assert!(
            graph
                .reached_rooms(Vec3::new(500.0, 0.0, 0.0), clip_from_local())
                .is_none()
        );
    }

    #[test]
    fn a_camera_in_a_doorway_sees_into_both_rooms() {
        let graph = chain(side_doorway(11, Vec3::new(10.0, 0.0, -12.0), 1.5, [1, 2]));
        // On the wall between A and B, the doorway has no usable projection.
        let reached = graph
            .reached_rooms(Vec3::new(0.0, 0.0, -10.0), clip_from_local())
            .unwrap();
        assert!(reached[0] && reached[1]);
    }

    #[test]
    fn an_object_in_two_rooms_is_drawn_when_either_is_reached() {
        let mut graph = chain(doorway(11, Vec3::new(0.0, 0.0, -30.0), 3.0, [1, 2]));
        graph.membership.insert(
            100,
            Membership {
                contained: vec![2],
                overlapping: vec![0],
            },
        );
        graph.membership.insert(
            101,
            Membership {
                contained: vec![2],
                overlapping: vec![],
            },
        );
        graph.membership.insert(
            102,
            Membership {
                contained: vec![],
                overlapping: vec![2],
            },
        );
        graph.membership.insert(
            103,
            Membership {
                contained: vec![1, 2],
                overlapping: vec![],
            },
        );
        let reached = [true, false, false];
        assert!(graph.is_drawn(100, &reached), "overlaps a reached room");
        assert!(!graph.is_drawn(101, &reached), "only in an unreached room");
        assert!(graph.is_drawn(102, &reached), "pivot in no room is drawn");
        assert!(graph.is_drawn(103, &reached), "pivot in several is drawn");
        assert!(graph.is_drawn(999, &reached), "not in the graph is drawn");
        assert!(graph.is_drawn(101, &[true, false, true]));
    }

    // --- The system ---------------------------------------------------------------------------

    struct Scene {
        app: App,
        camera: Entity,
        near: Entity,
        far: Entity,
        loose: Entity,
        light: Entity,
    }

    /// A camera in room A of a two-room cell whose only doorway points away from it, so room B's
    /// objects are culled. `near` is in A, `far` in B and `loose` has no FormId at all.
    fn scene(enabled: bool) -> Scene {
        scene_with(
            enabled,
            GlobalTransform::IDENTITY,
            GlobalTransform::IDENTITY,
        )
    }

    /// [`scene`] with the cell root at `root_transform` and the camera at `camera_transform`.
    fn scene_with(
        enabled: bool,
        root_transform: GlobalTransform,
        camera_transform: GlobalTransform,
    ) -> Scene {
        let mut app = App::new();
        app.add_plugins(RoomCullingPlugin {
            enabled,
            database_path: PathBuf::from("does-not-exist.db"),
        });
        let root = app
            .world_mut()
            .spawn((StreamedCellRoot, CellRef(7), root_transform))
            .id();
        let mut graph = RoomGraph {
            rooms: vec![
                room(1, Vec3::ZERO, 10.0),
                room(2, Vec3::new(0.0, 0.0, 20.0), 10.0),
            ],
            portals: vec![doorway(10, Vec3::new(0.0, 0.0, 10.0), 3.0, [0, 1])],
            links: vec![],
            membership: HashMap::new(),
        };
        graph.membership.insert(
            1,
            Membership {
                contained: vec![0],
                overlapping: vec![],
            },
        );
        graph.membership.insert(
            2,
            Membership {
                contained: vec![1],
                overlapping: vec![],
            },
        );
        app.world_mut()
            .resource_mut::<RoomCullingState>()
            .insert_graph(7, root, graph);
        let near_ref = app.world_mut().spawn((FormId(1), ChildOf(root))).id();
        let near = app.world_mut().spawn(ChildOf(near_ref)).id();
        let far_ref = app.world_mut().spawn((FormId(2), ChildOf(root))).id();
        let far = app.world_mut().spawn(ChildOf(far_ref)).id();
        let loose = app.world_mut().spawn(ChildOf(root)).id();
        let list = |entities: &[Entity]| {
            let mut visible = VisibleEntities::default();
            visible
                .get_mut(TypeId::of::<Mesh3d>())
                .extend_from_slice(entities);
            visible
        };
        let all = [near, far, loose];
        let camera = app
            .world_mut()
            .spawn((
                StreamingCamera,
                camera_transform,
                Projection::Perspective(PerspectiveProjection {
                    fov: std::f32::consts::FRAC_PI_3,
                    aspect_ratio: 1.0,
                    near: 0.1,
                    far: 1000.0,
                    ..default()
                }),
                list(&all),
            ))
            .id();
        // The camera looks down -Z; the doorway at +Z is behind it.
        let light = app
            .world_mut()
            .spawn((
                CubemapVisibleEntities::default(),
                VisibleMeshEntities {
                    entities: all.to_vec(),
                },
                list(&all),
            ))
            .id();
        Scene {
            app,
            camera,
            near,
            far,
            loose,
            light,
        }
    }

    fn mesh_list(app: &App, entity: Entity) -> Vec<Entity> {
        app.world()
            .get::<VisibleEntities>(entity)
            .unwrap()
            .get(TypeId::of::<Mesh3d>())
            .to_vec()
    }

    fn run_cull(app: &mut App) {
        // The system is in PostUpdate behind Bevy's visibility set; run it on its own here.
        app.world_mut()
            .run_system_once(cull_room_views)
            .expect("cull_room_views runs");
    }

    #[test]
    fn the_main_view_loses_unreached_objects_and_shadows_keep_them() {
        let mut scene = scene(true);
        run_cull(&mut scene.app);
        assert_eq!(
            mesh_list(&scene.app, scene.camera),
            vec![scene.near, scene.loose],
            "the object in the unreached room is gone from the main view, the loose one stays"
        );
        // A light's own sets, and another camera's list, are untouched.
        let light = scene.app.world().entity(scene.light);
        assert_eq!(
            light.get::<VisibleMeshEntities>().unwrap().entities,
            vec![scene.near, scene.far, scene.loose]
        );
        assert_eq!(
            mesh_list(&scene.app, scene.light),
            vec![scene.near, scene.far, scene.loose],
            "a view that did not opt in keeps its list"
        );
        assert_eq!(
            scene
                .app
                .world()
                .resource::<RoomCullingState>()
                .culled_last_frame,
            1
        );
    }

    #[test]
    fn a_camera_that_opts_in_is_culled_too() {
        let mut scene = scene(true);
        scene.app.world_mut().entity_mut(scene.light).insert((
            RoomCullingView,
            GlobalTransform::IDENTITY,
            Projection::default(),
        ));
        run_cull(&mut scene.app);
        assert!(!mesh_list(&scene.app, scene.light).contains(&scene.far));
        assert!(
            scene
                .app
                .world()
                .entity(scene.light)
                .get::<VisibleMeshEntities>()
                .unwrap()
                .entities
                .contains(&scene.far),
            "even then its shadow casters are its own"
        );
    }

    #[test]
    fn with_the_setting_off_nothing_is_culled() {
        let mut scene = scene(false);
        scene.app.update();
        run_cull(&mut scene.app);
        assert_eq!(
            mesh_list(&scene.app, scene.camera),
            vec![scene.near, scene.far, scene.loose]
        );
    }

    #[test]
    fn a_camera_in_no_room_keeps_every_object() {
        let mut scene = scene(true);
        scene
            .app
            .world_mut()
            .entity_mut(scene.camera)
            .insert(GlobalTransform::from_xyz(900.0, 0.0, 0.0));
        run_cull(&mut scene.app);
        assert_eq!(
            mesh_list(&scene.app, scene.camera),
            vec![scene.near, scene.far, scene.loose]
        );
    }

    #[test]
    fn a_read_of_a_missing_database_fails_once_and_leaves_the_view_alone() {
        AsyncComputeTaskPool::get_or_init(Default::default);
        let mut scene = scene(true);
        // No graph exists for cell 8; asking for it reads the (missing) database.
        scene.app.world_mut().spawn((StreamedCellRoot, CellRef(8)));
        for _ in 0..500 {
            scene.app.update();
            let state = scene.app.world().resource::<RoomCullingState>();
            if state.failed.contains(&8) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let state = scene.app.world().resource::<RoomCullingState>();
        assert!(state.failed.contains(&8) && state.pending.is_empty());
        assert!(state.active(), "a failed read is not an absent table");
        // Several more frames do not queue another read of the failed cell.
        scene.app.update();
        scene.app.update();
        let state = scene.app.world().resource::<RoomCullingState>();
        assert!(state.pending.is_empty());
    }

    // --- Round-1 review: the real paths -------------------------------------------------------

    /// A file named like the engine's own, holding the section 15 tables beside the tables the
    /// loader reads. Reference 500 sits on base object 20; reference 501 sits on base object 500,
    /// which is another placement's id, so a lookup by the base id would pick the wrong room.
    fn write_world_fixture(directory: &std::path::Path) -> PathBuf {
        let path = directory.join("skyrim_world.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(&format!(
                "CREATE TABLE schema_info(version INTEGER NOT NULL);
                 INSERT INTO schema_info VALUES({});
                 CREATE TABLE statics(id INTEGER PRIMARY KEY,model_path TEXT,bounds_min_x REAL,bounds_min_y REAL,bounds_min_z REAL,bounds_max_x REAL,bounds_max_y REAL,bounds_max_z REAL,bounds_valid INTEGER NOT NULL);
                 CREATE TABLE \"references\"(id INTEGER PRIMARY KEY,cell_id INTEGER,base_form_id INTEGER,pos_x REAL,pos_y REAL,pos_z REAL,rot_x REAL,rot_y REAL,rot_z REAL,scale REAL);
                 INSERT INTO statics VALUES(20,'architecture/wall.nif',-1,-2,-3,1,2,3,1);
                 INSERT INTO statics VALUES(500,'architecture/floor.nif',-1,-2,-3,1,2,3,1);
                 INSERT INTO \"references\" VALUES(500,7,20,0,0,0,0,0,0,1);
                 INSERT INTO \"references\" VALUES(501,7,500,1000,0,0,0,0,0,1);",
                shared::WORLD_DATABASE_SCHEMA_VERSION
            ))
            .unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        // Rooms 1 and 2 side by side, no doorway and no link: from room 1 only room 1 is reached.
        insert_room(&connection, 1, 7, [0.0, 0.0, 0.0]);
        insert_room(&connection, 2, 7, [1000.0, 0.0, 0.0]);
        connection
            .execute("INSERT INTO reference_rooms VALUES (500,1,1),(501,2,1)", [])
            .unwrap();
        path
    }

    /// The whole key chain through the real loader: `WorldDatabase` delivers `references.id` as
    /// `ReferenceRow::form_id` (which `streaming::spawn_cell` puts on the entity as `FormId`), and
    /// `reference_rooms.reference_id` is that same number.
    #[test]
    fn the_membership_key_is_the_reference_id_the_loader_delivers() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_world_fixture(directory.path());
        let database = WorldDatabase::open(&path).unwrap();
        database
            .request(DatabaseRequest::Load {
                generation: 1,
                key: CellKey::Interior(7),
                queued_at: std::time::Instant::now(),
            })
            .unwrap();
        let response = loop {
            if let Some(response) = database.try_response() {
                break response;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        };
        let mut references = response.result.expect("the cell loads").references;
        references.sort_by_key(|reference| reference.form_id);
        let keys: Vec<(u32, u32)> = references
            .iter()
            .map(|reference| (reference.form_id, reference.base_form_id))
            .collect();
        assert_eq!(
            keys,
            vec![(500, 20), (501, 500)],
            "form_id is references.id"
        );

        let graph = read_graph(&path, 7).unwrap().expect("the tables are there");
        assert_eq!(graph.rooms_containing(Vec3::ZERO), vec![0]);
        let reached = graph
            .reached_rooms(Vec3::ZERO, clip_from_local())
            .expect("the camera is in room 1");
        assert_eq!(reached, vec![true, false]);
        for reference in &references {
            // The key the engine uses is the delivered form_id; the base id would hit the wrong row.
            assert_eq!(
                graph.is_drawn(reference.form_id, &reached),
                reference.form_id == 500
            );
        }
        assert!(
            graph.is_drawn(references[1].base_form_id, &reached),
            "base id 500 would have made reference 501 look like reference 500"
        );
    }

    /// The production read path, from a file: the path the plugin opens is the one the engine opens
    /// (`app.rs`: `config.assets_dir.join("skyrim_world.db")`, lines 96 and 113 at this commit), the
    /// tables read back, and a file without them is `None` rather than an error.
    #[test]
    fn a_world_database_file_is_read_through_the_production_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_world_fixture(directory.path());
        let config = EngineConfig {
            assets_dir: directory.path().to_path_buf(),
            ..default()
        };
        let plugin = RoomCullingPlugin::from_config(&config);
        assert_eq!(plugin.database_path, path);
        let graph = read_graph(&plugin.database_path, 7).unwrap().unwrap();
        assert_eq!((graph.rooms.len(), graph.membership.len()), (2, 2));

        let bare = directory.path().join("bare.db");
        Connection::open(&bare)
            .unwrap()
            .execute_batch("CREATE TABLE schema_info(version INTEGER);")
            .unwrap();
        assert!(read_graph(&bare, 7).unwrap().is_none());
    }

    /// Bevy's own visibility plugin, the real plugin and `app.update()`: the system has to run in
    /// `CheckVisibility` after `check_visibility_cpu_culling`, or Bevy's refill puts the culled mesh
    /// back. Room C has no doorway, so its mesh is in the frustum but must leave the camera's list
    /// while its `ViewVisibility`, which shadow passes read, stays set.
    #[test]
    fn the_real_visibility_schedule_culls_after_bevys_own_check() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            TransformPlugin,
            VisibilityPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(RoomCullingPlugin {
            enabled: true,
            database_path: PathBuf::from("does-not-exist.db"),
        });
        let root = app
            .world_mut()
            .spawn((
                StreamedCellRoot,
                CellRef(7),
                Transform::IDENTITY,
                Visibility::default(),
            ))
            .id();
        let graph = RoomGraph {
            rooms: vec![
                room(1, Vec3::ZERO, 10.0),
                room(2, Vec3::new(0.0, 0.0, -20.0), 10.0),
                room(3, Vec3::new(0.0, 0.0, -40.0), 10.0),
            ],
            portals: vec![doorway(10, Vec3::new(0.0, 0.0, -10.0), 3.0, [0, 1])],
            links: vec![],
            membership: (0..3)
                .map(|room| {
                    (
                        room as u32 + 1,
                        Membership {
                            contained: vec![room],
                            overlapping: vec![],
                        },
                    )
                })
                .collect(),
        };
        app.world_mut()
            .resource_mut::<RoomCullingState>()
            .insert_graph(7, root, graph);
        let mesh_in = |app: &mut App, form_id: u32, z: f32| {
            let reference = app
                .world_mut()
                .spawn((FormId(form_id), Visibility::default(), ChildOf(root)))
                .id();
            app.world_mut()
                .spawn((
                    Mesh3d(Handle::default()),
                    Aabb::from_min_max(Vec3::splat(-1.0), Vec3::splat(1.0)),
                    Transform::from_xyz(0.0, 0.0, z),
                    ChildOf(reference),
                ))
                .id()
        };
        let (a, b, c) = (
            mesh_in(&mut app, 1, -3.0),
            mesh_in(&mut app, 2, -20.0),
            mesh_in(&mut app, 3, -40.0),
        );
        let projection = Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_3,
            aspect_ratio: 1.0,
            ..default()
        });
        let frustum = Frustum(ViewFrustum::from_clip_from_world(
            &projection.get_clip_from_view(),
        ));
        let camera = app
            .world_mut()
            .spawn((
                Camera::default(),
                StreamingCamera,
                Transform::IDENTITY,
                projection,
                frustum,
            ))
            .id();
        app.update();
        let visible = mesh_list(&app, camera);
        assert!(visible.contains(&a) && visible.contains(&b));
        assert!(
            !visible.contains(&c),
            "room C's mesh is in the frustum but unreached, so it leaves the main view"
        );
        assert!(
            app.world().get::<ViewVisibility>(c).unwrap().get(),
            "Bevy's own check still marks it visible, which is what shadow passes use"
        );
        assert_eq!(
            app.world().resource::<RoomCullingState>().culled_last_frame,
            1
        );
    }

    /// The A-B-C cull again with the cell root rotated and moved: the graph is root-local, so a
    /// camera at the same place in the root's frame gets the same answer, and a camera that is not
    /// in the root's frame is in no room.
    #[test]
    fn a_rotated_and_moved_cell_root_gives_the_same_answer() {
        let root = GlobalTransform::from(Transform {
            translation: Vec3::new(100.0, 5.0, -30.0),
            rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            ..default()
        });
        let mut inside = scene_with(true, root, root);
        run_cull(&mut inside.app);
        assert_eq!(
            mesh_list(&inside.app, inside.camera),
            vec![inside.near, inside.loose]
        );
        let mut outside = scene_with(true, root, GlobalTransform::IDENTITY);
        run_cull(&mut outside.app);
        assert_eq!(
            mesh_list(&outside.app, outside.camera),
            vec![outside.near, outside.far, outside.loose],
            "the world origin is not inside any room of this root"
        );
    }

    /// A doorway the camera is next to counts as open even when it points away from the view; one
    /// just beyond `PORTAL_OPEN_DISTANCE` does not.
    #[test]
    fn a_doorway_next_to_the_camera_is_open_but_one_further_away_is_not() {
        let graph = chain(doorway(11, Vec3::new(0.0, 0.0, -30.0), 3.0, [1, 2]));
        // The camera looks along +Z (away from the doorway at z = -10).
        let clip_for = |position: Vec3| {
            clip_from_local()
                * Mat4::from_rotation_y(std::f32::consts::PI)
                * Mat4::from_translation(-position)
        };
        let near = Vec3::new(0.0, 0.0, -10.0 + PORTAL_OPEN_DISTANCE - 1.0);
        let reached = graph.reached_rooms(near, clip_for(near)).unwrap();
        assert!(reached[1], "within the open distance the doorway is taken");
        let far = Vec3::new(0.0, 0.0, -10.0 + PORTAL_OPEN_DISTANCE + 1.0);
        let reached = graph.reached_rooms(far, clip_for(far)).unwrap();
        assert!(!reached[1], "beyond it, a doorway behind the camera is not");
    }

    #[test]
    fn actors_doors_and_moving_bodies_are_never_culled_by_their_room() {
        let mut scene = scene(true);
        let root = scene
            .app
            .world_mut()
            .query_filtered::<Entity, With<StreamedCellRoot>>()
            .single(scene.app.world())
            .unwrap();
        // References 3..=6 are all placed in room B, which the camera cannot reach.
        let add = |app: &mut App, form_id: u32, reference: &dyn Fn(&mut EntityWorldMut)| {
            let mut reference_entity = app.world_mut().spawn((FormId(form_id), ChildOf(root)));
            reference(&mut reference_entity);
            let reference_entity = reference_entity.id();
            let mesh = app.world_mut().spawn(ChildOf(reference_entity)).id();
            app.world_mut()
                .resource_mut::<RoomCullingState>()
                .graphs
                .get_mut(&7)
                .unwrap()
                .graph
                .membership
                .insert(
                    form_id,
                    Membership {
                        contained: vec![1],
                        overlapping: vec![],
                    },
                );
            mesh
        };
        let actor = add(&mut scene.app, 3, &|entity| {
            entity.insert(RoomCullExempt);
        });
        let dynamic = add(&mut scene.app, 4, &|_| {});
        scene
            .app
            .world_mut()
            .entity_mut(dynamic)
            .insert(RigidBody::Dynamic);
        let door = add(&mut scene.app, 5, &|entity| {
            entity.insert(RigidBody::KinematicPositionBased);
        });
        let fixed = add(&mut scene.app, 6, &|entity| {
            entity.insert(RigidBody::Fixed);
        });
        scene
            .app
            .world_mut()
            .get_mut::<VisibleEntities>(scene.camera)
            .unwrap()
            .get_mut(TypeId::of::<Mesh3d>())
            .extend([actor, dynamic, door, fixed]);
        run_cull(&mut scene.app);
        let visible = mesh_list(&scene.app, scene.camera);
        for kept in [actor, dynamic, door] {
            assert!(visible.contains(&kept), "{kept:?} must stay drawn");
        }
        assert!(
            !visible.contains(&fixed),
            "a fixed object in room B is culled"
        );
        assert!(!visible.contains(&scene.far), "so is the plain static");
    }

    #[test]
    fn actor_door_and_loose_item_records_are_exempt_and_statics_are_not() {
        for kept in ["NPC_", "LVLN", "DOOR", "ACTI", "MISC", "WEAP"] {
            assert!(exempt_record_type(Some(kept)), "{kept}");
        }
        for culled in ["STAT", "TREE", "FURN", "LIGH"] {
            assert!(!exempt_record_type(Some(culled)), "{culled}");
        }
        assert!(!exempt_record_type(None));
    }
}
