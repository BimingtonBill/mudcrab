#!/usr/bin/env python3
"""How far the portal's door -> arrival map is from the geometry of the two doorways.

The portal draws the destination through an open load door by mapping the viewer's pose
through a rigid transform built from the source door's *link-derived frame* and the
destination's `XTEL` *arrival frame* (`crates/engine/src/transition.rs`). A load door in
Skyrim is a teleport, so nothing in the game's data ever had to make the two doorways agree
as geometry; this tool measures, over every reciprocal pair in the converted database, how
far apart those two maps are.

The engine's own functions are ported here line for line, so what this tool calls `M_engine`
is what the running engine builds, not a re-derivation:

    outward_from_return_link   crates/engine/src/doors.rs:87
    door_frame                 crates/engine/src/transition.rs:237
    arrival_camera_rotation    crates/engine/src/transition.rs:216
    arrival_frame              crates/engine/src/transition.rs:262
    door_to_arrival_rotation   crates/engine/src/transition.rs:287
    portal_pose                crates/engine/src/transition.rs:303
    crossing_pose              crates/engine/src/transition.rs:357

and `M_geom` is the same shape with the **destination doorway's own frame** in place of its
`XTEL` arrival frame:

    M_engine = T(arrival_p) * R(arrival) * Ry(pi) * R(frame_source)^-1  * T(-source_p)
    M_geom   = T(door_b_p)  * R(frame_b) * Ry(pi) * R(frame_source)^-1  * T(-source_p)

`frame_b` - which way door B's own doorway faces - is the question this tool exists to ask,
so it is computed three ways and reported side by side (see `Frame` and `DESTINATION_FRAME`).

Usage:
    python tools/research/portal_door_alignment.py selftest
    python tools/research/portal_door_alignment.py probe
    python tools/research/portal_door_alignment.py summary
    python tools/research/portal_door_alignment.py models [model_substring]
    python tools/research/portal_door_alignment.py pair <ref_hex> [ref_hex ...]
    python tools/research/portal_door_alignment.py trace <ref_hex>
    python tools/research/portal_door_alignment.py worst [n]
    python tools/research/portal_door_alignment.py spots [ref_hex ...]

Everything is Creation-engine units and radians unless a line says degrees. Positions are
the database's absolute Creation coordinates; the render-origin rebase the engine applies to
an exterior is a common translation on both sides of a map, so it cancels out of every
number here (the report's section on `--shots` says what a pose still has to add).
"""

from __future__ import annotations

import math
import sqlite3
import sys
from collections import defaultdict

DATABASE = "C:/Modding/SkyrimConverted/skyrim_world.db"

# crates/engine/src/doors.rs:18 and :29
OUTWARD_POINT_DISTANCE = 16.0
OUTWARD_POINT_MAX_DISTANCE = 512.0

EYE_HEIGHT = 120.0

# ---------------------------------------------------------------------------------------------
# Quaternions, in the runtime ([x, y, z, w], Y-up) convention shared::coordinates uses
# ---------------------------------------------------------------------------------------------


def q_mul(a, b):
    """`multiply_quaternions` (crates/shared/src/coordinates.rs:58)."""
    return [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]


def q_conjugate(q):
    return [-q[0], -q[1], -q[2], q[3]]


def q_normalize(q):
    length = math.sqrt(sum(value * value for value in q))
    if length == 0.0 or not math.isfinite(length):
        return [0.0, 0.0, 0.0, 1.0]
    return [value / length for value in q]


def axis_angle(axis, angle):
    half = angle * 0.5
    sine = math.sin(half)
    return [axis[0] * sine, axis[1] * sine, axis[2] * sine, math.cos(half)]


def from_rotation_y(angle):
    """`Quat::from_rotation_y` - a yaw about the runtime up axis."""
    return axis_angle([0.0, 1.0, 0.0], angle)


def creation_euler_to_runtime_quaternion(rotation):
    """`creation_euler_to_runtime_quaternion` (crates/shared/src/coordinates.rs:37).

    Creation angles turn clockwise about each axis: the stored angles describe
    `Rz * Ry * Rx` and the object is rotated by its inverse, then conjugated by the
    Creation-to-runtime basis.
    """
    x, y, z = rotation
    source = q_conjugate(
        q_mul(axis_angle([0.0, 0.0, 1.0], z), q_mul(axis_angle([0.0, 1.0, 0.0], y), axis_angle([1.0, 0.0, 0.0], x)))
    )
    basis = [-math.sqrt(0.5), 0.0, 0.0, math.sqrt(0.5)]
    return q_normalize(q_mul(q_mul(basis, source), q_conjugate(basis)))


def rotate(q, vector):
    rotated = q_mul(q_mul(q, vector + [0.0]), q_conjugate(q))
    return rotated[:3]


def creation_to_runtime(vector):
    """`creation_to_runtime_vector` (crates/shared/src/coordinates.rs:15)."""
    return [vector[0], vector[2], -vector[1]]


def creation_heading(direction):
    """The Creation heading of a horizontal Creation direction: `z` faces `(sin z, cos z)`."""
    return math.degrees(math.atan2(direction[0], direction[1]))


def wrap180(degrees):
    return (degrees + 180.0) % 360.0 - 180.0


def hypot2(a, b):
    return math.hypot(a, b)


def q_to_yaw_degrees(q):
    """The heading of a yaw-only rotation: its forward is `(sin h, cos h)` in Creation axes.

    A runtime rotation `from_rotation_y(t)` faces the Creation heading `-t`, so the heading a
    door frame or an arrival frame faces is `-2 * atan2(q.y, q.w)`.
    """
    return math.degrees(-2.0 * math.atan2(q[1], q[3]))


def q_to_yxz_degrees(q):
    """`EulerRot::YXZ` decomposition, in degrees, for a general (non-yaw) rotation."""
    x, y, z, w = q
    m02 = 2.0 * (x * z + w * y)
    m12 = 2.0 * (y * z - w * x)
    m22 = 1.0 - 2.0 * (x * x + y * y)
    m10 = 2.0 * (x * y + w * z)
    m11 = 1.0 - 2.0 * (x * x + z * z)
    pitch = math.asin(max(-1.0, min(1.0, -m12)))
    yaw = math.atan2(m02, m22)
    roll = math.atan2(m10, m11)
    return math.degrees(yaw), math.degrees(pitch), math.degrees(roll)


# ---------------------------------------------------------------------------------------------
# The engine's door frame and door -> arrival map
# ---------------------------------------------------------------------------------------------


def outward_from_return_link(door_position, arrival_position, arrival_rotation):
    """`outward_from_return_link` (crates/engine/src/doors.rs:87), plus which rule it used.

    Returns `(outward, rule)`. The arrival *point* says which way the door faces while it is
    at the door - further than `OUTWARD_POINT_DISTANCE` and nearer than
    `OUTWARD_POINT_MAX_DISTANCE` - and the arrival *heading* does otherwise.
    """
    east = arrival_position[0] - door_position[0]
    north = arrival_position[1] - door_position[1]
    distance = math.hypot(east, north)
    if (
        math.isfinite(east)
        and math.isfinite(north)
        and OUTWARD_POINT_DISTANCE < distance <= OUTWARD_POINT_MAX_DISTANCE
    ):
        return [east / distance, north / distance, 0.0], "point"
    heading = [math.sin(arrival_rotation[2]), math.cos(arrival_rotation[2]), 0.0]
    if math.isfinite(heading[0]) and math.isfinite(heading[1]) and heading[0] ** 2 + heading[1] ** 2 > 0.5:
        return heading, "heading"
    return None, "none"


def door_frame(door_rotation, outward):
    """`door_frame` (crates/engine/src/transition.rs:237): forward is the outward direction."""
    if outward is None:
        return door_rotation
    east, north = outward[0], outward[1]
    if not math.isfinite(east) or not math.isfinite(north) or math.hypot(east, north) <= 0.0:
        return door_rotation
    return from_rotation_y(-math.atan2(east, north))


def arrival_camera_rotation(rotation):
    """`arrival_camera_rotation` (crates/engine/src/transition.rs:216): only `z` is read."""
    return from_rotation_y(-rotation[2])


class Rigid:
    """A rigid transform: `p -> rotation * p + translation`, in runtime axes."""

    def __init__(self, rotation, translation):
        self.rotation = rotation
        self.translation = translation

    def apply(self, point):
        return [value + shift for value, shift in zip(rotate(self.rotation, point), self.translation)]

    def compose(self, other):
        """`self` after `other`: a point `other` maps is then mapped by `self`."""
        return Rigid(
            q_mul(self.rotation, other.rotation),
            [value + shift for value, shift in zip(rotate(self.rotation, other.translation), self.translation)],
        )

    def inverse(self):
        inverse_rotation = q_conjugate(self.rotation)
        return Rigid(inverse_rotation, [-value for value in rotate(inverse_rotation, self.translation)])


def engine_map(source_position, frame, arrival_position, arrival_rotation):
    """`portal_pose`/`door_to_arrival_rotation`: the map the engine builds (transition.rs:287).

    `source_position`/`frame` are the source door's render pose and frame, `arrival_*` the
    destination's `XTEL` arrival frame - all in runtime axes.
    """
    rotation = q_mul(arrival_rotation, q_mul(from_rotation_y(math.pi), q_conjugate(frame)))
    return Rigid(rotation, [value - shift for value, shift in zip(arrival_position, rotate(rotation, source_position))])


def geometry_map(source_position, frame, door_position, door_frame_rotation):
    """The same shape, with the destination doorway's own frame in place of the arrival frame.

    This is the transform that makes the two doorways coincide as geometry: the source
    doorway's own frame mapped onto the destination doorway's own frame, turned half a
    revolution about the vertical, so that walking into one door means walking out of the
    other.
    """
    rotation = q_mul(door_frame_rotation, q_mul(from_rotation_y(math.pi), q_conjugate(frame)))
    return Rigid(rotation, [value - shift for value, shift in zip(door_position, rotate(rotation, source_position))])


def box_height(door):
    """The doorway box centre's height above the door's own reference, in runtime units."""
    if door.bounds_centre is None:
        return 0.0
    return rotate(door.model_rotation, [value * door.scale for value in door.bounds_centre])[1]


def doorway_anchor(door):
    """The doorway's centre in runtime axes, from the model bounds the engine also measures with.

    `measured_doorway_box` (crates/engine/src/portal.rs:643) places the quad at the doorway box
    the converted model carries, so the point a map has to land in the right place is the box
    centre, not the reference's origin - they are the same point only when the model's origin
    sits in the middle of its own doorway. Falls back to the reference's origin when the base
    has no usable bounds.

    The bounds in `statics` are the *converted* model's (`MeshConverter::glb_bounds`,
    crates/converter/src/integration.rs:102), so they are already in runtime (Y-up) axes and the
    reference's runtime rotation is all they need - the same two inputs
    `crates/engine/src/streaming.rs:1205` builds `ExpectedModelBounds` from.
    """
    base = creation_to_runtime(door.position)
    if door.bounds_centre is None:
        return base
    # The reference's `XSCL` scales the model bounds before the rotation does
    # (crates/engine/src/portal.rs:651-653, `Vec3::splat(scale)`): 259 of the 2,204 doors in the
    # installed data have a scale other than 1, so this is not a formality.
    offset = rotate(door.model_rotation, [value * door.scale for value in door.bounds_centre])
    return [base[axis] + offset[axis] for axis in range(3)]


# ---------------------------------------------------------------------------------------------
# The database
# ---------------------------------------------------------------------------------------------


class Door:
    __slots__ = ("ref_id", "cell_id", "worldspace_id", "base_form_id", "model", "editor_id",
                 "position", "rotation", "bounds_centre", "has_bounds", "scale", "link", "return_link",
                 "outward", "outward_rule", "facing", "facing_model", "arrival",
                 "engine_error", "arrival_error")

    def __init__(self, row):
        (self.ref_id, self.cell_id, self.worldspace_id, self.base_form_id, self.model, self.editor_id,
         self.position, self.rotation, self.bounds_centre, self.has_bounds, self.scale) = row
        self.link = None
        self.return_link = None
        self.outward = None
        self.outward_rule = "none"
        self.facing = None
        self.facing_model = None
        self.arrival = None
        self.engine_error = None
        self.arrival_error = None

    @property
    def model_rotation(self):
        return creation_euler_to_runtime_quaternion(self.rotation)

    @property
    def frame(self):
        """The engine's frame for this door: `door_frame(rotation, outward)`. Runtime axes."""
        return door_frame(self.model_rotation, self.outward)

    def __repr__(self):
        return f"<Door {self.ref_id:08X} {self.editor_id or self.model}>"


DOOR_SELECT = """
    SELECT r.id, r.cell_id, r.worldspace_id, r.base_form_id,
           COALESCE(s.model_path, ''), COALESCE(s.editor_id, ''),
           r.pos_x, r.pos_y, r.pos_z, r.rot_x, r.rot_y, r.rot_z,
           s.bounds_min_x, s.bounds_min_y, s.bounds_min_z,
           s.bounds_max_x, s.bounds_max_y, s.bounds_max_z,
           COALESCE(s.bounds_valid, 0), COALESCE(r.scale, 1.0)
    FROM "references" r
    LEFT JOIN statics s ON s.id = r.base_form_id
    JOIN door_links d ON d.ref_id = r.id
"""

LINK_SELECT = """
    SELECT ref_id, destination_ref_id, destination_cell_id, destination_worldspace_id,
           pos_x, pos_y, pos_z, rot_x, rot_y, rot_z
    FROM door_links
"""


def door_row(row):
    position = (row[6], row[7], row[8])
    rotation = (row[9], row[10], row[11])
    centre = None
    # Columns: 12..17 are bounds_min_x..bounds_max_z, 18 is `bounds_valid`, 19 the reference scale.
    if row[18] and all(value is not None for value in row[12:18]):
        centre = (
            0.5 * (row[12] + row[15]),
            0.5 * (row[13] + row[16]),
            0.5 * (row[14] + row[17]),
        )
    return Door((row[0], row[1], row[2], row[3], row[4], row[5], position, rotation, centre,
                 bool(row[18]), row[19] if row[19] else 1.0))


def load_doors(connection):
    """Every reference with a `door_links` row, plus its return link (lowest `ref_id` wins)."""
    doors = {}
    for row in connection.execute(DOOR_SELECT):
        doors[row[0]] = door_row(row)
    links = {}
    for row in connection.execute(LINK_SELECT):
        links[row[0]] = {
            "ref_id": row[0],
            "destination": row[1],
            "cell": row[2],
            "worldspace": row[3],
            "position": (row[4], row[5], row[6]),
            "rotation": (row[7], row[8], row[9]),
        }
    by_destination = defaultdict(list)
    for link in links.values():
        by_destination[link["destination"]].append(link)
    for ref_id, door in doors.items():
        door.link = links.get(ref_id)
        # `world/database.rs`: when several links lead to the same door the lowest ref_id wins,
        # so the answer does not depend on the query planner.
        candidates = sorted(by_destination.get(ref_id, ()), key=lambda link: link["ref_id"])
        door.return_link = candidates[0] if candidates else None
    for door in doors.values():
        if door.return_link is not None:
            door.outward, door.outward_rule = outward_from_return_link(
                door.position, door.return_link["position"], door.return_link["rotation"]
            )
        door.facing = creation_heading(door.outward) if door.outward else None
    return doors, links


def annotate_doors(doors, conventions):
    """Give every door its own doorway's facing, its arrival heading, and both frame errors.

    For a door X with a return link:
      `facing_model` - X's own doorway's facing, `reference yaw + the model's convention`.
      `arrival`      - the return link's `XTEL` heading: where the game puts the arriving player.
      `engine_error` - `facing_model - facing`: how far the frame the engine builds from the
                       link data (`door_frame(model_rotation, outward)` = the link's outward
                       direction) is from the doorway's own facing. This is the error the
                       engine's map *starts* from when X is the source door of a crossing.
      `arrival_error`- `facing_model - arrival`: how far the heading the engine maps onto when X
                       is the destination is from X's doorway's own facing.
    """
    for door in doors.values():
        door.facing_model = destination_facing(door, conventions, "model")
        door.arrival = (
            wrap180(math.degrees(door.return_link["rotation"][2])) if door.return_link is not None else None
        )
        door.engine_error = (
            None if door.facing is None or door.facing_model is None
            else wrap180(door.facing_model - door.facing)
        )
        door.arrival_error = (
            None if door.arrival is None or door.facing_model is None
            else wrap180(door.facing_model - door.arrival)
        )
    return doors


def space_names(connection):
    interiors = {row[0]: (row[1] or "") for row in connection.execute("SELECT id, interior_name FROM cells")}
    worldspaces = {row[0]: row[1] for row in connection.execute("SELECT id, editor_id FROM worldspaces")}
    return interiors, worldspaces


def reciprocal_pairs(doors):
    """Every *direction* a door leads in: (source, destination) for each door with a usable link.

    Both directions of a reciprocal pair are included, and that is deliberate: the map is built
    from the *source* door's frame, so a pair can be badly wrong one way round and nearly right
    the other (Sven's House is +12.27 and +0.19 degrees), and which of the two a person sees
    depends on which side of the wall they are standing. Pairing only the lower `ref_id` would
    have hidden the direction the user actually looked in.
    """
    pairs = []
    for door in doors.values():
        link = door.link
        if link is None:
            continue
        other = doors.get(link["destination"])
        if other is None:
            continue
        pairs.append((door, other))
    return pairs


def same_model_share(doors):
    """How many directions join two doors whose base model is the same.

    For those the model's axis convention cancels out of the doorway-to-doorway map
    (`R(yawB + C) * Y180 * R(yawA + C)^-1 = R(yawB + 180 - yawA)`), so the geometry map needs
    nothing but the two references' own rotations and their doorway centres - both already in the
    engine's hands - and no per-model convention has to be established at all.
    """
    pairs = reciprocal_pairs(doors)
    return sum(1 for source, destination in pairs if source.model == destination.model), len(pairs)


# ---------------------------------------------------------------------------------------------
# Which way the destination doorway faces
# ---------------------------------------------------------------------------------------------

# The three answers to "which way does door B's own doorway face", all in Creation headings:
#   link  - the engine's own answer: the direction its return link puts the arriving player
#           (door_frame(door_b.model_rotation, door_b.outward)). This is *not* independent
#           evidence: door B's return link is A's link, so it is built from the same `XTEL`
#           numbers the arrival frame is.
#   model - door B's reference yaw plus the model's own axis convention, the circular mean of
#           (link-derived facing - reference yaw) over every placement of that model in the
#           install. This is what can disagree with the `XTEL`, and it is what the doorway's
#           own geometry (the frame around the opening) is built with.
#   raw   - door B's reference yaw alone: the pre-`impl-023` assumption that a door model's
#           local `+Y` is its front.
DESTINATION_FRAME = ("link", "model", "raw")


def model_conventions(doors, min_placements=1):
    """Per model, the circular mean of `link-derived facing - reference yaw`, over placements.

    Returns `(conventions, spreads)` with `spreads[model] = (placements, median |residual|,
    p90 |residual|, share within 15 deg)`. The share is what says whether a model's own axis
    convention is a fact about the model (most placements agree) or an average of disagreement
    (a model placed both ways - invisible markers, ship trapdoors, ladder doors - where the
    mean predicts nothing).
    """
    by_model = defaultdict(list)
    for door in doors.values():
        if door.outward is None:
            continue
        by_model[door.model].append(door)
    conventions = {}
    spreads = {}
    for model, placements in by_model.items():
        if len(placements) < min_placements:
            continue
        offsets = [wrap180(creation_heading(door.outward) - q_to_yaw_degrees(door.model_rotation))
                   for door in placements]
        sine = sum(math.sin(math.radians(offset)) for offset in offsets)
        cosine = sum(math.cos(math.radians(offset)) for offset in offsets)
        conventions[model] = math.degrees(math.atan2(sine, cosine))
        residual = [abs(wrap180(offset - conventions[model])) for offset in offsets]
        within = sum(1 for value in residual if value <= 15.0) / len(residual)
        spreads[model] = (len(placements), percentile(residual, 0.5), percentile(residual, 0.9), within)
    return conventions, spreads


def reliable_models(spreads, min_placements=4, min_share=0.8):
    """The models whose own facing can be predicted from their placements: most of them agree."""
    return {model for model, (placements, _, _, within) in spreads.items()
            if placements >= min_placements and within >= min_share}


def destination_facing(door, conventions, variant):
    """The heading the destination doorway faces, in Creation axes, by one of the three rules."""
    if variant == "link":
        return door.facing
    reference = q_to_yaw_degrees(door.model_rotation)
    if variant == "raw":
        return wrap180(reference) if door.outward is not None else None
    convention = conventions.get(door.model)
    if convention is None:
        return door.facing
    return wrap180(reference + convention)


def destination_frame(door, conventions, variant):
    """`door_frame` for the destination doorway under one of the three rules, runtime axes."""
    if variant == "link":
        return door.frame
    heading = destination_facing(door, conventions, variant)
    if heading is None:
        return door.frame
    return from_rotation_y(-math.radians(heading))


# ---------------------------------------------------------------------------------------------
# Measurement
# ---------------------------------------------------------------------------------------------


class PairMeasurement:
    """One reciprocal pair: the engine's map against the map the two doorways' geometry wants.

    Source A, destination B, A's link taken as the forward one:

      M_engine = T(arrival)      * R(arrival)      * Y180 * R(door_frame(A, A.outward))^-1
      M_geom   = T(doorway B)    * R(own facing B) * Y180 * R(own facing A)^-1

    Both map the source doorway onto a destination; `M_geom` maps A's doorway *centre* onto B's
    doorway centre so that the two doorways coincide as geometry, and `M_engine` maps A's
    reference origin onto the `XTEL` arrival point with the `XTEL` heading as the destination's
    facing. The difference's yaw is what turns the destination room behind the door; its
    displacement is what puts the room at the wrong depth.
    """

    __slots__ = (
        "source", "destination",
        "yaw_error", "source_error", "destination_error", "pitch_error", "roll_error",
        "gap_distance", "gap_lateral", "gap_depth", "gap_vertical", "arrival_offset",
        "arrival_heading", "source_facing", "destination_facing", "arrival_distance",
        "source_rule", "destination_rule", "arrival_roll", "height_gap", "own_frame_source",
        "apparent_distance", "apparent_lateral", "apparent_depth", "apparent_vertical",
        "reliable", "floor_gap", "vertical_error",
    )

    def __init__(self, source, destination, conventions, reliable=None):
        self.source = source
        self.destination = destination
        link = source.link
        # Whether both doorways' own facings come from a model whose placements agree with each
        # other. Outside that set the doorway's own facing is an average of disagreement and the
        # numbers below say more about the estimate than about the doorways.
        self.reliable = reliable is None or (source.model in reliable and destination.model in reliable)
        self.arrival_heading = wrap180(math.degrees(link["rotation"][2]))
        self.source_facing = source.facing
        self.destination_facing = destination_facing(destination, conventions, "model")
        self.source_rule = source.outward_rule
        self.destination_rule = destination.outward_rule
        self.arrival_roll = math.degrees(link["rotation"][1])
        # `own_frame_source` is False for the handful of doors whose model carries no convention
        # (the invisible markers and the like): then the source's own frame is unknown and the
        # engine's link-derived frame is used for both maps, which measures the destination half
        # of the error only.
        self.own_frame_source = source.facing_model is not None

        source_anchor = doorway_anchor(source)
        destination_anchor = doorway_anchor(destination)
        source_own_frame = destination_frame(source, conventions, "model")
        destination_own_frame = destination_frame(destination, conventions, "model")

        engine = engine_map(
            creation_to_runtime(source.position),
            source.frame,
            creation_to_runtime(link["position"]),
            arrival_camera_rotation(link["rotation"]),
        )
        geometry = geometry_map(source_anchor, source_own_frame, destination_anchor, destination_own_frame)

        difference = geometry.compose(engine.inverse())
        # The difference is a yaw about the vertical plus a translation. Its yaw, in Creation
        # headings (the convention every other angle here uses), is exactly
        # `destination term - source term`: the geometry map turns the destination room that much
        # clockwise from where the engine's map draws it, so a positive value means the room is
        # drawn that far anticlockwise of where it belongs.
        self.yaw_error = wrap180(q_to_yaw_degrees(difference.rotation))
        self.pitch_error = q_to_yxz_degrees(difference.rotation)[1]
        self.roll_error = q_to_yxz_degrees(difference.rotation)[2]
        # The whole yaw error is one source-door term minus one destination-door term, both of
        # them "the doorway's own facing against what the engine used for that door".
        self.source_error = source.engine_error
        self.destination_error = destination.arrival_error

        # The destination doorway's own centre against the point the engine lands on. Both are
        # in one frame: the displacement is what an eye sees the doorway shifted by, and the
        # rotation above is what it sees the room turned by.
        # `gap` is the arrival point relative to the destination doorway's centre, so a positive
        # depth means the game lands the player *inside the room* past the doorway (the usual
        # case: the doorway centre then sits that far behind the player).
        gap = [value - shift for value, shift in zip(creation_to_runtime(link["position"]), destination_anchor)]
        self.gap_distance = math.sqrt(sum(value * value for value in gap))
        self.gap_vertical = gap[1]
        into_room = rotate(destination_own_frame, [0.0, 0.0, -1.0])
        depth = sum(value * shift for value, shift in zip(gap, into_room))
        lateral = [value - depth * shift for value, shift in zip(gap, into_room)]
        lateral[1] = 0.0
        self.gap_depth = depth
        self.gap_lateral = math.sqrt(sum(value * value for value in lateral))

        # Where the destination doorway's centre is *drawn*, in the source's own frame: the
        # engine's map run backwards on the destination doorway's centre, against the source
        # doorway's own centre, which is where the geometry map would put it. What the viewer
        # sees through the open door is this offset plus the yaw error above.
        drawn = engine.inverse().apply(destination_anchor)
        offset = [value - shift for value, shift in zip(drawn, source_anchor)]
        look = rotate(source_own_frame, [0.0, 0.0, -1.0])
        away = [-value for value in look]
        depth = sum(value * shift for value, shift in zip(offset, away))
        lateral = [value - depth * shift for value, shift in zip(offset, away)]
        lateral[1] = 0.0
        self.apparent_distance = math.sqrt(sum(value * value for value in offset))
        self.apparent_depth = depth
        self.apparent_lateral = math.sqrt(sum(value * value for value in lateral))
        self.apparent_vertical = offset[1]

        self.arrival_offset = math.hypot(
            link["position"][0] - destination.position[0], link["position"][1] - destination.position[1]
        )
        self.arrival_distance = self.arrival_offset
        # The map keeps every height, so the destination is drawn as far above or below the
        # source doorway as the arrival point stands above or below the destination door's own
        # base. It cancels only when the game's arrival point lies at the door's own floor level.
        self.height_gap = link["position"][2] - destination.position[2]
        # The two sides' floors, each measured against its own door's base: where the player
        # stands after emerging at that door (the return link's arrival) minus the door's own z.
        # A doorway map lands the player on the *destination* floor at the height they stood
        # above the *source* floor, so this difference is the vertical error it would leave -
        # the vertical counterpart of `yaw_error`, and small wherever both doorways sit the same
        # height above their own floor (Skyrim's usual case: the reference at the threshold).
        self.floor_gap = (
            link["position"][2] - destination.position[2]
            - (source.return_link["position"][2] - source.position[2])
            if source.return_link is not None else None
        )
        # What the anchor actually leaves: the map takes the *source doorway's centre* to the
        # *destination doorway's centre*, so the player's feet (the source floor) land at
        # `dest centre + (feet - source centre)` and the error against the destination floor is
        # the two boxes' own heights above their references plus the floor gap above. Same-model
        # doors cancel it exactly, which is why Sven's House lands on the floor and a farmhouse
        # into a Nordic ruin need not.
        self.vertical_error = (
            None if self.floor_gap is None else
            self.floor_gap
            + box_height(destination) - box_height(source)
        )


def measure(doors, conventions, reliable=None):
    return [PairMeasurement(source, destination, conventions, reliable)
            for source, destination in reciprocal_pairs(doors)]


def histogram(values, buckets=(1.0, 5.0, 15.0, 45.0)):
    counts = [0] * (len(buckets) + 1)
    for value in values:
        for index, edge in enumerate(buckets):
            if value <= edge:
                counts[index] += 1
                break
        else:
            counts[-1] += 1
    return counts


def histogram_table(values, buckets=(1.0, 5.0, 15.0, 45.0)):
    counts = histogram(values, buckets)
    labels = [f"<= {buckets[0]:g}"] + [f"{buckets[i]:g}..{buckets[i + 1]:g}" for i in range(len(buckets) - 1)]
    labels.append(f"> {buckets[-1]:g}")
    total = max(1, len(values))
    return [(label, count, 100.0 * count / total) for label, count in zip(labels, counts)]


def percentile(values, fraction):
    if not values:
        return float("nan")
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, int(round(fraction * (len(ordered) - 1)))))
    return ordered[index]


# ---------------------------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------------------------


def describe(door, interiors, worldspaces):
    where = interiors.get(door.cell_id, "") or worldspaces.get(door.worldspace_id, "") or "?"
    return f"{door.ref_id:08X} {where.strip(chr(0)).strip()!r} {door.model}"


def command_selftest():
    """Ports of the engine's own tests, so a wrong port cannot pass quietly."""
    failures = []

    def check(name, condition, detail=""):
        if not condition:
            failures.append(f"{name}: {detail}")
        print(f"  {'ok  ' if condition else 'FAIL'} {name} {detail}")

    # transition.rs test `door_frame` with outward [1, 0, 0]: the frame faces east, so a camera
    # with it looks along runtime +X (creation_to_runtime([1, 0, 0])).
    frame = door_frame(q_normalize(from_rotation_y(0.3)), [1.0, 0.0, 0.0])
    forward = rotate(frame, [0.0, 0.0, -1.0])
    check("door_frame faces its outward direction", abs(forward[0] - 1.0) < 1e-6 and abs(forward[2]) < 1e-6,
          f"{forward}")
    # A heading z faces (sin z, cos z, 0); runtime is (sin z, 0, -cos z).
    frame = door_frame(q_normalize(from_rotation_y(0.7)), [math.sin(1.2), math.cos(1.2), 0.0])
    forward = rotate(frame, [0.0, 0.0, -1.0])
    check("door_frame reads a Creation heading",
          abs(forward[0] - math.sin(1.2)) < 1e-6 and abs(forward[2] + math.cos(1.2)) < 1e-6, f"{forward}")
    # arrival_camera_rotation: looking along the arrival heading.
    came = arrival_camera_rotation([0.0, 0.0, 2.5])
    forward = rotate(came, [0.0, 0.0, -1.0])
    check("arrival_camera_rotation faces the heading",
          abs(forward[0] - math.sin(2.5)) < 1e-6 and abs(forward[2] + math.cos(2.5)) < 1e-6, f"{forward}")
    # A point at the door's own position lands on the arrival point (transition.rs:296).
    mapping = engine_map([10.0, 5.0, -3.0], door_frame(from_rotation_y(0.4), [0.0, 1.0, 0.0]),
                         [200.0, 7.0, 30.0], arrival_camera_rotation([0.0, 0.0, 1.0]))
    landed = mapping.apply([10.0, 5.0, -3.0])
    check("M(door) = arrival", all(abs(landed[i] - v) < 1e-4 for i, v in enumerate([200.0, 7.0, 30.0])),
          f"{landed}")
    # The map is rigid: it keeps distances and the vertical.
    other = mapping.apply([10.0, 5.0, -3.0 + 60.0])
    check("the map is rigid", abs(other[1] - 7.0) < 1e-4, f"{other}")
    # Creation yaw convention: a reference yaw z faces (sin z, cos z) - `a_yaw_turns_clockwise_like_a_heading`
    # (crates/shared/src/coordinates.rs:172).
    model = creation_euler_to_runtime_quaternion([0.0, 0.0, 1.0])
    check("a reference yaw is a Creation heading", abs(q_to_yaw_degrees(model) - math.degrees(1.0)) < 1e-3,
          f"{q_to_yaw_degrees(model)}")
    return 1 if failures else 0


def command_probe(connection, doors, links, interiors, worldspaces):
    version = connection.execute("SELECT sqlite_version()").fetchone()[0]
    print(f"sqlite {version}")
    print(f"doors with a door_links row: {len(doors)}")
    print(f"door_links rows: {len(links)}")
    print(f"reciprocal pairs: {len(reciprocal_pairs(doors))}")
    rules = defaultdict(int)
    for door in doors.values():
        rules[door.outward_rule] += 1
    print("outward rule: " + ", ".join(f"{name} {count}" for name, count in sorted(rules.items())))
    with_bounds = sum(1 for door in doors.values() if door.bounds_centre is not None)
    print(f"doors whose base carries usable bounds: {with_bounds}")
    scaled = connection.execute(
        "SELECT count(*) FROM \"references\" r JOIN door_links d ON d.ref_id = r.id "
        "WHERE r.scale IS NOT NULL AND abs(r.scale - 1.0) > 1.0e-4"
    ).fetchone()[0]
    print(f"doors with a reference scale other than 1 (the tool assumes 1 when placing the box "
          f"centre): {scaled}")
    print(f"doors with no return link (outward None): "
          f"{sum(1 for door in doors.values() if door.return_link is None)}")
    print(f"reference rotations outside +-pi: "
          f"{sum(1 for door in doors.values() if max(abs(value) for value in door.rotation) > math.pi + 1e-3)}")
    rotations = [abs(value) for door in doors.values() for value in door.rotation]
    print(f"largest |rotation component| over doors: {max(rotations):.4f} rad ({math.degrees(max(rotations)):.1f} deg)")
    # How often the two rules of `outward_from_return_link` disagree, over every linked door.
    disagreements = []
    for door in doors.values():
        link = door.return_link
        if link is None:
            continue
        east = link["position"][0] - door.position[0]
        north = link["position"][1] - door.position[1]
        distance = math.hypot(east, north)
        if not (OUTWARD_POINT_DISTANCE < distance <= OUTWARD_POINT_MAX_DISTANCE):
            continue
        point_heading = math.degrees(math.atan2(east, north))
        heading_heading = math.degrees(link["rotation"][2])
        disagreements.append(abs(wrap180(point_heading - heading_heading)))
    print(f"doors using the point rule: {len(disagreements)}; point vs heading there: "
          f"median {percentile(disagreements, 0.5):.2f} p90 {percentile(disagreements, 0.9):.2f} "
          f"max {max(disagreements):.2f} deg")

    # Which of the two rules `outward_from_return_link` could use answers "which way does the door
    # face" better, judged against the door's own facing (reference yaw + its model's convention).
    point_error, heading_error = [], []
    for door in doors.values():
        if door.facing_model is None or door.return_link is None:
            continue
        link = door.return_link
        east = link["position"][0] - door.position[0]
        north = link["position"][1] - door.position[1]
        if not (OUTWARD_POINT_DISTANCE < math.hypot(east, north) <= OUTWARD_POINT_MAX_DISTANCE):
            continue
        point_error.append(abs(wrap180(math.degrees(math.atan2(east, north)) - door.facing_model)))
        heading_error.append(abs(wrap180(wrap180(math.degrees(link["rotation"][2])) - door.facing_model)))
    print_distribution("where the point rule applies, |the arrival point's direction - the "
                       "doorway's own facing|, deg", point_error, (1.0, 5.0, 15.0, 45.0))
    print_distribution("the same doors, |the arrival heading - the doorway's own facing|, deg",
                       heading_error, (1.0, 5.0, 15.0, 45.0))


def print_distribution(label, values, buckets=(1.0, 5.0, 15.0, 45.0), unit="deg"):
    print(f"\n== {label}")
    if not values:
        print("  (no values)")
        return
    for name, count, share in histogram_table(values, buckets):
        print(f"  {name:>14}  {count:6d}  {share:6.2f}%")
    print(f"  median {percentile(values, 0.5):.2f}  p90 {percentile(values, 0.9):.2f} "
          f"p99 {percentile(values, 0.99):.2f}  max {max(values):.2f} {unit}")


def command_summary(doors, links, interiors, worldspaces, conventions, spreads):
    print(f"doors with a door_links row: {len(doors)}")
    print(f"links: {len(links)}")
    rules = defaultdict(int)
    for door in doors.values():
        rules[door.outward_rule] += 1
    print("outward rule: " + ", ".join(f"{name} {count}" for name, count in sorted(rules.items())))
    reliable = reliable_models(spreads)
    measurements = measure(doors, conventions, reliable)
    pairs = reciprocal_pairs(doors)
    print(f"directions (each door as the source of its own link): {len(measurements)}; "
          f"of them {sum(1 for a, b in pairs if a.ref_id < b.ref_id)} are the lower-ref_id half "
          f"of a reciprocal pair")

    sure = [m for m in measurements if m.reliable]
    print(f"models with a trustworthy axis convention: {len(reliable)} of {len(spreads)} "
          f"(>=4 placements, >=80% of them within 15 deg of the mean)")
    print(f"pairs whose two doorways both come from one: {len(sure)} of {len(measurements)}")
    print_distribution("THE MAP'S YAW ERROR, over the pairs where both doorways' own facings are "
                       "known, |deg|", [abs(m.yaw_error) for m in sure])
    print_distribution("  where one or neither doorway's own facing is known, |deg|",
                       [abs(m.yaw_error) for m in measurements if not m.reliable])

    # The map's own yaw error, then the two terms it is made of: the engine's frame for the
    # source door against that doorway's own facing, and the `XTEL` arrival heading against the
    # destination doorway's own facing. `yaw error = destination term - source term`.
    print_distribution("THE MAP'S YAW ERROR - how far the destination room is turned behind the "
                       "door, |deg|", [abs(m.yaw_error) for m in measurements])
    print_distribution("  the destination-door term: the XTEL arrival heading against the "
                       "destination doorway's own facing, |deg|",
                       [abs(m.destination_error) for m in measurements])
    print_distribution("  the source-door term: the engine's frame against the source doorway's own "
                       "facing, |deg|", [abs(m.source_error) for m in measurements])
    print_distribution("the source-door term measured over every door with a return link, |deg|",
                       [abs(door.engine_error) for door in doors.values() if door.engine_error is not None])
    print_distribution("the destination-door term over every door with a return link, |deg|",
                       [abs(door.arrival_error) for door in doors.values() if door.arrival_error is not None])
    print_distribution("XTEL arrival heading against the *link-derived* facing the engine builds "
                       "(the point rule's own error), |deg|",
                       [abs(wrap180(door.arrival - door.facing)) for door in doors.values()
                        if door.arrival is not None and door.facing is not None])

    # Where the destination doorway's own centre stands from the point the engine lands on.
    print_distribution("AS DRAWN over the pairs whose two doorways are both known: the offset "
                       "between the destination doorway and where the source doorway is, |units|",
                       [m.apparent_distance for m in sure], (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("  its depth part (+ = drawn further from the viewer), units",
                       [m.apparent_depth for m in sure], (-256.0, -64.0, -16.0, 16.0), "u")
    print_distribution("  its sideways part, units", [m.apparent_lateral for m in sure],
                       (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("the arrival point against the destination doorway's centre over the same "
                       "pairs (units), 3D |gap|", [m.gap_distance for m in sure],
                       (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("where the destination doorway's centre stands from the XTEL arrival point "
                       "(units), 3D |gap|, every pair", [m.gap_distance for m in measurements],
                       (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("  its depth along the destination doorway's own facing (how far inside the "
                       "room the arrival point is), units", [abs(m.gap_depth) for m in measurements],
                       (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("  its sideways part, units", [m.gap_lateral for m in measurements],
                       (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("  its vertical part, units", [abs(m.gap_vertical) for m in measurements],
                       (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("non-yaw parts of the map (should be ~0): |pitch| deg",
                       [abs(m.pitch_error) for m in measurements], (0.5, 2.0, 10.0, 45.0))
    print_distribution("|roll| deg", [abs(m.roll_error) for m in measurements], (0.5, 2.0, 10.0, 45.0))
    print_distribution("XTEL arrival roll (rot_y), |deg| - the engine ignores it",
                       [abs(m.arrival_roll) for m in measurements], (0.5, 2.0, 10.0, 45.0))
    print_distribution("the arrival point's height against the destination door's own base (units)",
                       [abs(m.height_gap) for m in measurements], (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("THE VERTICAL ERROR a doorway anchor would leave - how far the anchored "
                       "landing would stand from the destination floor, |units|",
                       [abs(m.vertical_error) for m in measurements if m.vertical_error is not None],
                       (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("  of which the two sides' floors against their own doorways, |units|",
                       [abs(m.floor_gap) for m in measurements if m.floor_gap is not None],
                       (8.0, 32.0, 128.0, 512.0), "u")
    print_distribution("distance from the destination door's origin to the XTEL arrival point "
                       "(units)", [m.arrival_offset for m in measurements], (16.0, 64.0, 256.0, 1024.0), "u")


def command_placements(doors, interiors, worldspaces, conventions, substrings):
    """Every placement of the matching models, with the numbers its convention was fit from."""
    for model in sorted({door.model for door in doors.values()},
                        key=lambda name: -sum(1 for door in doors.values() if door.model == name)):
        if substrings and not any(text.lower() in model.lower() for text in substrings):
            continue
        placements = [door for door in doors.values() if door.model == model]
        print(f"\n== {len(placements)} placements of {model!r} "
              f"(convention {conventions.get(model)} deg)")
        print("      ref       cell                        model yaw  outward rule  outward     "
              "offset  residual")
        for door in sorted(placements, key=lambda door: door.ref_id):
            if door.outward is None:
                print(f"  {door.ref_id:08X}  no return link")
                continue
            yaw = q_to_yaw_degrees(door.model_rotation)
            offset = wrap180(door.facing - yaw)
            residual = wrap180(offset - conventions.get(model, 0.0))
            cell = (interiors.get(door.cell_id, "") or worldspaces.get(door.worldspace_id, "")
                    or "?").strip(chr(0)).strip()
            print(f"  {door.ref_id:08X}  {cell!r:28s} {yaw:9.3f}  {door.outward_rule:6s}  "
                  f"{door.facing:9.3f}  {offset:7.2f}  {residual:7.2f}")
        if substrings:
            break


def command_models(doors, conventions, spreads, substrings):
    counts = defaultdict(int)
    for door in doors.values():
        counts[door.model] += 1
    print("== each model's own convention: the local axis its placements put on the door's facing")
    print("   placements  convention   median |resid|  p90 |resid|   model")
    printed = 0
    for model in sorted(conventions, key=lambda name: -counts[name]):
        if substrings and not any(text.lower() in model.lower() for text in substrings):
            continue
        printed += 1
        if printed > 40:
            break
        placements, median, p90 = spreads[model]
        print(f"  {counts[model]:10d}  {conventions[model]:8.2f}  {median:12.1f}  {p90:10.1f}   {model}")
    if not substrings:
        print(f"  ({len(conventions)} models have at least one link-derived facing)")
    # How tightly each model's own convention predicts each placement's link-derived facing.
    residuals = sorted(
        ((abs(wrap180(creation_heading(door.outward)
                      - wrap180(q_to_yaw_degrees(door.model_rotation) + conventions[door.model]))),
          door.ref_id, door)
         for door in doors.values()
         if door.outward is not None and door.model in conventions),
        key=lambda row: row[0],
    )
    values = [value for value, _, _ in residuals]
    print_distribution("per placement, |model-implied facing - link-derived facing| deg", values,
                       (5.0, 15.0, 45.0, 90.0))
    print("\n  worst 15 placements against their own model's convention:")
    for value, _, door in residuals[::-1][:15]:
        print(f"    {door.ref_id:08X} {value:7.1f} deg  {door.model}  ({counts[door.model]} placements, "
              f"convention {conventions[door.model]:.1f})")


def command_pair(doors, interiors, worldspaces, conventions, refs):
    by_ref = {door.ref_id: door for door in doors.values()}
    for value in refs:
        ref_id = int(value, 16)
        door = by_ref.get(ref_id)
        if door is None:
            print(f"{ref_id:08X}: no door_links row")
            continue
        link = door.link
        other = by_ref.get(link["destination"]) if link else None
        print(f"\n===== {describe(door, interiors, worldspaces)}")
        print(f"  position {door.position}  rotation (rad) {door.rotation}")
        print(f"  model yaw {q_to_yaw_degrees(door.model_rotation):.2f} deg, bounds centre {door.bounds_centre}")
        if link:
            print(f"  link -> {link['destination']:08X} {describe(other, interiors, worldspaces) if other else ''}")
            print(f"       arrival {link['position']} rotation (rad) {link['rotation']}")
            print(f"       arrival heading {wrap180(math.degrees(link['rotation'][2])):.2f} deg")
        if door.return_link:
            back = door.return_link
            print(f"  return link from {back['ref_id']:08X}, arrival {back['position']}, "
                  f"heading {wrap180(math.degrees(back['rotation'][2])):.2f} deg")
            print(f"       outward {[round(v, 4) for v in door.outward]} ({door.outward_rule} rule), "
                  f"facing {door.facing:.2f} deg")
            east = back["position"][0] - door.position[0]
            north = back["position"][1] - door.position[1]
            print(f"       arrival point is {math.hypot(east, north):.1f} units away "
                  f"(point rule window {OUTWARD_POINT_DISTANCE:g}..{OUTWARD_POINT_MAX_DISTANCE:g})")
        else:
            print("  no return link: outward is None (the model's own frame is used)")
        convention = conventions.get(door.model)
        print(f"  model convention {convention if convention is None else round(convention, 2)} deg, "
              f"model-implied facing "
              f"{destination_facing(door, conventions, 'model') if convention is not None else None}")
        # The reciprocal pair, if there is one: both directions, since each has its own numbers.
        if other is not None and other.link is not None and other.link["destination"] == door.ref_id:
            for forward, backward in ((door, other), (other, door)):
                measurement = PairMeasurement(forward, backward, conventions)
                print(f"  PAIR {forward.ref_id:08X} -> {backward.ref_id:08X}")
                print(f"    arrival heading {measurement.arrival_heading:.2f} deg; destination doorway's "
                      f"own facing {measurement.destination_facing:.2f} deg")
                print(f"    MAP YAW ERROR {measurement.yaw_error:+.2f} deg  "
                      f"(destination term {measurement.destination_error:+.2f} - source term "
                      f"{measurement.source_error:+.2f})")
                print(f"    the arrival point stands {measurement.gap_distance:.1f} units from the "
                      f"destination doorway's centre (+depth = inside the room past it: "
                      f"{measurement.gap_depth:+.1f}, lateral {measurement.gap_lateral:.1f})")
                print(f"    AS DRAWN: the destination doorway appears "
                      f"{measurement.apparent_distance:.1f} units from where the source doorway is "
                      f"(+depth = further from the viewer than the source doorway: "
                      f"{measurement.apparent_depth:+.1f}, sideways {measurement.apparent_lateral:.1f})")


def command_trace(doors, interiors, worldspaces, conventions, ref):
    """Step-by-step arithmetic for one pair, so the report's numbers can be checked by hand."""
    by_ref = {door.ref_id: door for door in doors.values()}
    door = by_ref.get(int(ref, 16))
    if door is None or door.link is None:
        print(f"{ref}: no door with a link")
        return 1
    other = by_ref.get(door.link["destination"])
    if other is None:
        print(f"{ref}: destination door is not in the table")
        return 1
    if door.ref_id > other.ref_id:
        door, other = other, door
    source, destination = door, other
    measurement = PairMeasurement(source, destination, conventions)
    link = source.link
    print(f"source {describe(source, interiors, worldspaces)}")
    print(f"  pos {source.position} rot {source.rotation}")
    print(f"  return link arrival {source.return_link['position']} heading "
          f"{wrap180(math.degrees(source.return_link['rotation'][2])):.3f} deg")
    print(f"  link-derived outward {[round(v, 5) for v in source.outward]} ({source.outward_rule}) "
          f"-> facing {source.facing:.3f} deg")
    print(f"  the doorway's own facing {source.facing_model:.3f} deg "
          f"(model yaw {q_to_yaw_degrees(source.model_rotation):.3f} + convention "
          f"{conventions.get(source.model)} deg)")
    print(f"  SOURCE TERM {source.engine_error:+.3f} deg (own facing - the frame the engine builds)")
    print(f"  frame_A forward = {[round(v, 5) for v in rotate(source.frame, [0.0, 0.0, -1.0])]}")
    print(f"destination {describe(destination, interiors, worldspaces)}")
    print(f"  pos {destination.position} rot {destination.rotation}")
    print(f"  doorway centre {[round(v, 2) for v in doorway_anchor(destination)]} "
          f"(bounds centre {[round(v, 2) for v in destination.bounds_centre]} in the model's frame)")
    print(f"  XTEL arrival {link['position']} heading {measurement.arrival_heading:.3f} deg, "
          f"rot {link['rotation']}")
    print(f"  XTEL arrival sits {measurement.arrival_offset:.1f} units from the door, bearing "
          f"{creation_heading((link['position'][0] - destination.position[0], link['position'][1] - destination.position[1])):.2f} deg")
    print(f"  the doorway's own facing {destination.facing_model:.3f} deg, its link-derived outward "
          f"{destination.facing:.3f} deg")
    print(f"  DESTINATION TERM {destination.arrival_error:+.3f} deg (own facing - the arrival heading)")
    print(f"MAP YAW ERROR {measurement.yaw_error:+.3f} deg = destination term "
          f"{measurement.destination_error:+.3f} - source term {measurement.source_error:+.3f}")
    print(f"  pitch {measurement.pitch_error:+.3f} roll {measurement.roll_error:+.3f} deg")
    print(f"  the destination doorway's centre stands {measurement.gap_distance:.1f} units from the "
          f"arrival point (depth {measurement.gap_depth:+.1f}, lateral {measurement.gap_lateral:.1f}, "
          f"vertical {measurement.gap_vertical:+.1f})")
    return 0


def command_worst(doors, interiors, worldspaces, conventions, count):
    measurements = measure(doors, conventions)
    print(f"== the {count} pairs the engine's map turns furthest from the two doorways' geometry")
    ordered = sorted(measurements, key=lambda m: -abs(m.yaw_error))
    for m in ordered[:count]:
        source, destination = m.source, m.destination
        source_cell = interiors.get(source.cell_id, "") or worldspaces.get(source.worldspace_id, "") or "?"
        destination_cell = (interiors.get(destination.cell_id, "")
                            or worldspaces.get(destination.worldspace_id, "") or "?")
        print(f"  {abs(m.yaw_error):7.2f} deg  {source.ref_id:08X} {source_cell.strip(chr(0)).strip()!r}"
              f" -> {destination.ref_id:08X} {destination_cell.strip(chr(0)).strip()!r}"
              f"  (source term {m.source_error:+7.2f}, destination term {m.destination_error:+7.2f}, "
              f"gap {m.gap_distance:6.0f} u)")
    print(f"\n== the {count} pairs where the destination doorway stands furthest from the arrival point")
    ordered = sorted(measurements, key=lambda m: -m.gap_distance)
    for m in ordered[:count]:
        source, destination = m.source, m.destination
        source_cell = interiors.get(source.cell_id, "") or worldspaces.get(source.worldspace_id, "") or "?"
        destination_cell = (interiors.get(destination.cell_id, "")
                            or worldspaces.get(destination.worldspace_id, "") or "?")
        print(f"  {m.gap_distance:8.1f} u ({m.gap_lateral:6.1f} sideways, {m.gap_depth:+8.1f} deep, "
              f"{m.gap_vertical:+7.1f} vertical) {abs(m.yaw_error):7.2f} deg  "
              f"{source.ref_id:08X} -> {destination.ref_id:08X}"
              f" {destination_cell.strip(chr(0)).strip()!r}")
    print(f"\n== the {count} pairs with the worst of both (yaw error and standing gap together)")
    ordered = sorted(measurements, key=lambda m: -(abs(m.yaw_error) * 10.0 + m.gap_distance / 20.0))
    for m in ordered[:count]:
        source, destination = m.source, m.destination
        destination_cell = (interiors.get(destination.cell_id, "")
                            or worldspaces.get(destination.worldspace_id, "") or "?")
        print(f"  {abs(m.yaw_error):7.2f} deg, {m.gap_distance:8.1f} u  "
              f"{source.ref_id:08X} {destination.ref_id:08X} "
              f"{destination_cell.strip(chr(0)).strip()!r}  {destination.model}")


def doorway_camera(door, distance=200.0, eye_height=EYE_HEIGHT):
    """A camera pose that looks square through a door's doorway, in Creation axes.

    On the door's front side (the side its link-derived facing points to), `distance` out, at
    eye height over the door's own origin, looking back at the door: the pose `--shots` wants -
    `position`, `yaw` (a Creation heading, clockwise from north), `pitch`.
    """
    if door.outward is None:
        return None
    heading = door.facing
    position = [
        door.position[0] + distance * door.outward[0],
        door.position[1] + distance * door.outward[1],
        door.position[2] + eye_height,
    ]
    return position, wrap180(heading + 180.0)


def command_spots(doors, interiors, worldspaces, conventions, refs):
    """A table of check sites: the space ids, the pose to stand at, and both directions' numbers."""
    by_ref = {door.ref_id: door for door in doors.values()}
    reliable = reliable_models(model_conventions(doors)[1])
    measurements = {}
    for m in measure(doors, conventions, reliable):
        measurements[m.source.ref_id] = m
        measurements[m.destination.ref_id] = m
    print("== places to look through, with a pose to look from")
    print("   (pose: Creation position in the door's own cell/worldspace, Creation heading, pitch; "
          "the eye is 120 units over the door's own z)")
    for value in refs:
        ref_id = int(value, 16)
        door = by_ref.get(ref_id)
        if door is None:
            print(f"\n  {ref_id:08X}: no door_links row")
            continue
        cell = (interiors.get(door.cell_id, "") or worldspaces.get(door.worldspace_id, "") or "?").strip(chr(0)).strip()
        space = (f"cell {door.cell_id:08X}" if door.cell_id and interiors.get(door.cell_id) is not None
                 else f"worldspace {door.worldspace_id:08X}")
        destination = door.link["destination"] if door.link else None
        destination_door = by_ref.get(destination)
        into = ""
        if destination_door is not None:
            into_cell = (interiors.get(destination_door.cell_id, "")
                         or worldspaces.get(destination_door.worldspace_id, "") or "?").strip(chr(0)).strip()
            into = f"  ->  {destination:08X} {into_cell!r}"
        print(f"\n  {ref_id:08X} {space} {cell!r} {door.model}{into}")
        print(f"      door pos {[round(v, 1) for v in door.position]} "
              f"(model bounds centre {[round(v, 1) for v in (door.bounds_centre or ())]}), "
              f"the doorway's own facing {door.facing_model if door.facing_model is None else round(door.facing_model, 1)} deg, "
              f"the engine's frame faces {door.facing if door.facing is None else round(door.facing, 1)} deg "
              f"({door.outward_rule} rule)")
        pose = doorway_camera(door)
        if pose is None:
            print("      no link-derived facing: no pose")
        else:
            position, yaw = pose
            print(f"      LOOK FROM [{position[0]:.1f}, {position[1]:.1f}, {position[2]:.1f}]  "
                  f"yaw {yaw:.1f}  pitch 0.0   (200 units out on the engine's facing; the door's own "
                  f"z {door.position[2]:.1f})")
        m = measurements.get(ref_id)
        if m is None:
            print("      no reciprocal pair")
            continue
        for forward, backward in ((m.source, m.destination), (m.destination, m.source)):
            measured = PairMeasurement(forward, backward, conventions, reliable)
            print(f"      {forward.ref_id:08X} -> {backward.ref_id:08X}: map yaw error "
                  f"{measured.yaw_error:+.2f} deg (source term {measured.source_error:+.2f}, "
                  f"destination term {measured.destination_error:+.2f}); as drawn the destination "
                  f"doorway stands {measured.apparent_distance:.1f} u from the source doorway "
                  f"(depth {measured.apparent_depth:+.1f}, sideways {measured.apparent_lateral:.1f}); "
                  f"the arrival point is {hypot2(measured.gap_depth, measured.gap_lateral):.0f} u "
                  f"from the destination doorway" + ("" if measured.reliable else " [model unsure]"))


# Riverwood (Sven's, the Riverwood Trader, Alvor's, the Sleeping Giant Inn), Whiterun, Solitude,
# a cave, the Alftand route: the report's check sites. Filled in from the distribution.
SPOT_REFS = [
    "1CBB0", "1CBAF",          # Sven's House, the pair the user saw
    "1341F",                   # the Riverwood Trader's front door
    "1CBB2",                   # Alvor and Sigrid's House
    "1A6A7",                   # Belethor's General Goods, Whiterun
    "162E8",                   # a Solitude building
    "00015D48", "00092809", "0009256A", "0006998D",  # the Alftand route
]


def command_modes(doors, interiors, worldspaces, conventions, spreads, args):
    """What a per-door switch between the two anchors would do, over every pair.

    The proposal the report makes: a door whose own doorway is *known* (a model whose placements
    agree on an axis convention, a usable bounds box, and a game arrival that lands at the doorway
    rather than somewhere else in the cell) is drawn and crossed with the doorway-to-doorway map;
    every other door keeps today's map. This prints the split and the error each group keeps.

    `args` may be `NEAR HEIGHT` in units (defaults 256 64), the two thresholds on the arrival
    point's horizontal distance from the doorway centre and its height above the door's base.
    """
    if args and args[0] == "sweep":
        command_sweep(doors, conventions, spreads, args[1:])
        return
    near = float(args[0]) if len(args) > 0 else 256.0
    height = float(args[1]) if len(args) > 1 else 64.0
    min_placements = int(args[2]) if len(args) > 2 else 4
    min_share = float(args[3]) if len(args) > 3 else 0.8
    reliable = reliable_models(spreads, min_placements, min_share)
    measurements = measure(doors, conventions, reliable)
    tiers = {"same model (no convention needed)": [], "both conventions known": [],
             "centre only (a facing is unknown)": [], "arrival anchor (unchanged)": []}
    for m in measurements:
        door, other = m.source, m.destination
        boxes = door.bounds_centre is not None and other.bounds_centre is not None
        lands_at_the_door = (math.hypot(m.gap_lateral, m.gap_depth) <= near
                            and abs(m.vertical_error if m.vertical_error is not None else 1e9) <= height)
        if not (boxes and lands_at_the_door):
            tiers["arrival anchor (unchanged)"].append(m)
        elif door.model == other.model:
            # The model's axis convention cancels out of the doorway-to-doorway map, whatever it
            # is, so no convention has to be established for these at all.
            tiers["same model (no convention needed)"].append(m)
        elif door.model in reliable and other.model in reliable:
            tiers["both conventions known"].append(m)
        else:
            tiers["centre only (a facing is unknown)"].append(m)
    total = max(1, len(measurements))
    same_model, directions = same_model_share(doors)
    print("== a per-door choice of anchor, from the data (section 9.2 of the report)")
    print(f"   {same_model} of the {directions} directions join two doors of the same base model, "
          f"where the model's axis convention cancels out of the map entirely")
    print(f"   the arrival anchor is kept when the game's arrival does not land at the destination "
          f"doorway (> {near:g} units away in plan, or > {height:g} units off the floor)")
    for name, group in tiers.items():
        print(f"\n   {name}: {len(group):5d} of {len(measurements)} directions "
              f"({100.0 * len(group) / total:5.1f}%)")
        if name == "arrival anchor (unchanged)":
            print("     what today's map still gets wrong here:")
            print_distribution("       map yaw error kept, |deg|", [abs(m.yaw_error) for m in group])
            print_distribution("       doorway-against-doorway offset kept, units",
                               [math.hypot(m.apparent_lateral, m.apparent_depth) for m in group],
                               (8.0, 32.0, 128.0, 512.0), "u")
            reasons = defaultdict(int)
            for m in group:
                if (m.source.bounds_centre is None or m.destination.bounds_centre is None):
                    reasons["a doorway has no bounds box at all"] += 1
                elif m.vertical_error is not None and abs(m.vertical_error) > height:
                    reasons["the anchored landing would leave the destination floor"] += 1
                else:
                    reasons["the game's arrival is far from the doorway"] += 1
            for reason, count in sorted(reasons.items(), key=lambda row: -row[1]):
                print(f"       {count:5d}  {reason}")
            shapes = defaultdict(int)
            for m in group:
                model = m.source.model
                if "AutoLoadMarker" in model or "Marker" in model:
                    shapes["an invisible marker (no doorway - the arrival anchor is correct here)"] += 1
                elif "Trapdoor" in model or "Ladder" in model or "Lever" in model:
                    shapes["a trapdoor, ladder or lever"] += 1
                elif model == "":
                    shapes["no model at all"] += 1
                else:
                    shapes["a real door model"] += 1
            for shape, count in sorted(shapes.items(), key=lambda row: -row[1]):
                print(f"       {count:5d}  {shape}")
            continue
        if name == "centre only (a facing is unknown)":
            print("     the offset is fixed; the error left is the facing one:")
            print_distribution("       map yaw error left, |deg|", [abs(m.yaw_error) for m in group])
            continue
        print(f"     the doorway lines up exactly; the player lands where the game puts them to "
              f"within median {percentile([math.hypot(m.gap_lateral, m.gap_depth) for m in group], 0.5):.0f} u, "
              f"p90 {percentile([math.hypot(m.gap_lateral, m.gap_depth) for m in group], 0.9):.0f} u")
        print_distribution("       the yaw error today's map leaves at these doors, |deg|",
                           [abs(m.yaw_error) for m in group])


def command_sweep(doors, conventions, spreads, args):
    """How the anchor split moves with the two thresholds, so they can be chosen, not guessed."""
    reliable = reliable_models(spreads)
    measurements = measure(doors, conventions, reliable)
    print("== threshold sweep: fractions over all", len(measurements), "directions")
    print("   near = how far the game's arrival point may stand from the destination doorway's")
    print("   centre in plan; height = how far the anchored landing may leave the destination floor")
    print("\n   near  height   anchored   player moved: median / p90 / max (u)     fallback keeps >45 deg")
    for near in (64.0, 128.0, 256.0, 512.0):
        for height in (32.0, 64.0, 128.0):
            anchored, fallback = [], []
            for m in measurements:
                known = (m.source.model in reliable and m.destination.model in reliable
                         and m.source.bounds_centre is not None and m.destination.bounds_centre is not None)
                lands = (math.hypot(m.gap_lateral, m.gap_depth) <= near
                         and abs(m.vertical_error if m.vertical_error is not None else 1e9) <= height)
                (anchored if known and lands else fallback).append(m)
            moves = [math.hypot(m.gap_lateral, m.gap_depth) for m in anchored]
            bad = sum(1 for m in fallback if abs(m.yaw_error) > 45.0)
            print(f"  {near:6.0f} {height:6.0f}   {100.0 * len(anchored) / len(measurements):6.1f}%   "
                  f"{percentile(moves, 0.5):7.0f} /{percentile(moves, 0.9):6.0f} /{max(moves):7.0f}"
                  f"        {100.0 * bad / max(1, len(fallback)):6.1f}% of {len(fallback)}")


def command_list(doors, interiors, worldspaces, conventions, spreads, refs):
    """Every reciprocal pair whose source or destination cell name matches one of `refs`."""
    by_ref = {door.ref_id: door for door in doors.values()}
    reliable = reliable_models(spreads)
    measurements = measure(doors, conventions, reliable)
    wanted = [text.lower() for text in refs]
    printed = 0
    for m in sorted(measurements, key=lambda m: -abs(m.yaw_error)):
        source, destination = m.source, m.destination
        source_cell = (interiors.get(source.cell_id, "") or worldspaces.get(source.worldspace_id, "")
                       or "?").strip(chr(0)).strip()
        destination_cell = (interiors.get(destination.cell_id, "")
                            or worldspaces.get(destination.worldspace_id, "") or "?").strip(chr(0)).strip()
        if not any(text in source_cell.lower() or text in destination_cell.lower() for text in wanted):
            continue
        printed += 1
        print(f"  {abs(m.yaw_error):7.2f} deg, gap {m.gap_distance:7.1f} u "
              f"({'sure  ' if m.reliable else 'unsure'})  {source.ref_id:08X} {source_cell!r}"
              f" -> {destination.ref_id:08X} {destination_cell!r}")
        print(f"        {destination.model}  (arrival {m.arrival_offset:6.1f} u from the door, "
              f"height {m.height_gap:+6.1f})")
        if printed >= 40:
            break
    if printed == 0:
        print("  no pair matched")


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    command = argv[1]
    if command == "selftest":
        return command_selftest()
    connection = sqlite3.connect(f"file:{DATABASE}?mode=ro", uri=True)
    interiors, worldspaces = space_names(connection)
    doors, links = load_doors(connection)
    conventions, spreads = model_conventions(doors)
    annotate_doors(doors, conventions)
    if command == "probe":
        command_probe(connection, doors, links, interiors, worldspaces)
    elif command == "summary":
        command_summary(doors, links, interiors, worldspaces, conventions, spreads)
    elif command == "models":
        command_models(doors, conventions, spreads, argv[2:])
    elif command == "placements":
        command_placements(doors, interiors, worldspaces, conventions, argv[2:])
    elif command == "pair":
        command_pair(doors, interiors, worldspaces, conventions, argv[2:])
    elif command == "trace":
        return command_trace(doors, interiors, worldspaces, conventions, argv[2])
    elif command == "worst":
        command_worst(doors, interiors, worldspaces, conventions, int(argv[2]) if len(argv) > 2 else 20)
    elif command == "spots":
        command_spots(doors, interiors, worldspaces, conventions, argv[2:] or SPOT_REFS)
    elif command == "list":
        command_list(doors, interiors, worldspaces, conventions, spreads, argv[2:])
    elif command == "modes":
        command_modes(doors, interiors, worldspaces, conventions, spreads, argv[2:])
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
