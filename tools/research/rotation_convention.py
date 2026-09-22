#!/usr/bin/env python3
"""Which Euler order and signs do Skyrim `REFR` rotations use?

The engine's `creation_euler_to_runtime_quaternion` (`crates/shared/src/coordinates.rs`)
turns the stored `DATA` angles of a reference into a rotation. Its yaw half is proven
from load doors; the tilt half (the order in which x and y combine, and their signs)
is not. Tilted objects that stand on sloped ground are the evidence: their local up
axis should follow the terrain normal of the LAND heightmap underneath them.

This script

  * ports the engine's quaternion function exactly (quaternion multiply and conjugate
    are copied from `crates/shared/src/coordinates.rs`, including the Creation-to-runtime
    basis), and checks the port against every candidate convention with a matrix;
  * samples tilted references in exterior cells, bilinear-samples the LAND heightmap at
    each one for the ground height and the terrain normal, and keeps the ones that stand
    on the ground (so "the object was tilted to follow the slope" is a fair assumption);
  * scores every convention (order x sign) by the angle between the reference's up axis
    and the terrain normal: median, mean, fraction within 10 degrees, per object group.

Usage:
    python tools/research/rotation_convention.py census [--world 60] [--cells 200]
    python tools/research/rotation_convention.py audit [--world 60] [--cells 400]
        [--tilt 3] [--ground 128] [--slope 5] [--min-samples 200] [--json out.json]

Angles in the database are radians; `--tilt`, `--slope` and every reported angle are degrees.
"""

import json
import math
import sqlite3
import sys

import numpy as np

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"
CELL_SIZE = 4096.0
SIDE = 33          # LAND vertices per side
VERTEX_STEP = 128.0
DEG = math.pi / 180.0

# Every convention is (order, sign of x, sign of y, sign of z). "XYZ" multiplies the
# matrix left to right, so the vector sees Rz first; "ZYX" applies Rx first.
SIGNS = [
    (-1, -1, -1),
    (-1, -1, 1),
    (-1, 1, -1),
    (-1, 1, 1),
    (1, -1, -1),
    (1, -1, 1),
    (1, 1, -1),
    (1, 1, 1),
]
CONVENTIONS = [(order, sign) for order in ("XYZ", "ZYX") for sign in SIGNS]

NAMED = {
    "a": ("XYZ", (-1, -1, -1)),
    "b": ("ZYX", (-1, -1, -1)),
    "c": ("XYZ", (1, 1, -1)),
    "d": ("ZYX", (1, 1, -1)),
    "e": ("ZYX", (1, 1, 1)),
}


def label(convention):
    order, signs = convention
    names = [name for name, value in NAMED.items() if value == convention]
    text = f"{order} {'/'.join('+' if s > 0 else '-' for s in signs)}"
    if names:
        text += f"  [{'/'.join(names)}]"
        if convention == ("XYZ", (-1, -1, -1)):
            text += " <- engine today"
        if convention == ("ZYX", (1, 1, 1)):
            text += " <- engine before 933b95f"
    return text


# --------------------------------------------------------------------------------------
# The engine function, ported line for line from crates/shared/src/coordinates.rs
# --------------------------------------------------------------------------------------

def axis_angle(axis, angle):
    half = angle * 0.5
    sine = math.sin(half)
    return np.array([axis[0] * sine, axis[1] * sine, axis[2] * sine, math.cos(half)])


def multiply_quaternions(a, b):
    return np.array(
        [
            a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
            a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
            a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
            a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
        ]
    )


def conjugate_quaternion(q):
    return np.array([-q[0], -q[1], -q[2], q[3]])


def normalize_quaternion(q):
    length = math.sqrt(float(np.dot(q, q)))
    if length == 0.0:
        return np.array([0.0, 0.0, 0.0, 1.0])
    return q / length


CREATION_TO_RUNTIME_ROTATION = np.array([-math.sqrt(0.5), 0.0, 0.0, math.sqrt(0.5)])


def creation_euler_to_runtime_quaternion(rotation):
    x, y, z = rotation
    source = conjugate_quaternion(
        multiply_quaternions(
            axis_angle([0.0, 0.0, 1.0], z),
            multiply_quaternions(
                axis_angle([0.0, 1.0, 0.0], y), axis_angle([1.0, 0.0, 0.0], x)
            ),
        )
    )
    basis = CREATION_TO_RUNTIME_ROTATION
    return normalize_quaternion(
        multiply_quaternions(
            multiply_quaternions(basis, source), conjugate_quaternion(basis)
        )
    )


def rotate(q, vector):
    """The test helper of the same module: q * v * conj(q)."""
    vector_q = np.array([vector[0], vector[1], vector[2], 0.0])
    rotated = multiply_quaternions(multiply_quaternions(q, vector_q), conjugate_quaternion(q))
    return rotated[:3]


def creation_to_runtime_vector(v):
    return np.array([v[0], v[2], -v[1]])


def quaternion_matrix(q):
    x, y, z, w = q
    return np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ]
    )


def rot_x(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]])


def rot_y(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]])


def rot_z(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]])


def convention_matrix(convention, rotation):
    """Creation-space rotation matrix of a convention, applied to a column vector."""
    order, (sx, sy, sz) = convention
    x, y, z = rotation
    rx = rot_x(sx * x)
    ry = rot_y(sy * y)
    rz = rot_z(sz * z)
    return rx @ ry @ rz if order == "XYZ" else rz @ ry @ rx


def basis_matrix():
    """`creation_to_runtime_vector` as a matrix: [x, y, z] -> [x, z, -y]."""
    return np.array([[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]])


def check_port(samples=256):
    """Identify the ported function as one of the conventions, and by how much.

    The engine's quaternion acts on runtime vectors; the terrain comparison works in
    Creation space, so the quaternion is read back as `B * M * B^-1` where `B` is
    `creation_to_runtime_vector` as a matrix. Returns (convention, error, next best
    error) after checking that the same convention wins for every sample.
    """
    rng = np.random.default_rng(20260922)
    basis = basis_matrix()
    rotations = [tuple(float(v) for v in rng.uniform(-3.2, 3.2, 3)) for _ in range(samples)]
    rotations += [
        (0.0, 0.0, 0.0),
        (0.3, -0.7, 1.1),
        (0.0, 0.0, math.pi / 2),
        (math.pi / 2, 0.0, 0.0),
        (0.0, math.pi / 2, 0.0),
        (0.6, 0.6, 0.6),
    ]
    worst = {candidate: 0.0 for candidate in CONVENTIONS}
    for rotation in rotations:
        runtime_matrix = quaternion_matrix(creation_euler_to_runtime_quaternion(rotation))
        for candidate in CONVENTIONS:
            expected = basis @ convention_matrix(candidate, rotation) @ basis.T
            worst[candidate] = max(
                worst[candidate], float(np.abs(runtime_matrix - expected).max())
            )
    ranking = sorted(CONVENTIONS, key=lambda candidate: worst[candidate])
    return ranking[0], worst[ranking[0]], worst[ranking[1]]


def selftest():
    """The two assertions of the Rust module's own tests, run on this port."""
    failures = []
    quarter = math.pi / 2
    yaw = creation_euler_to_runtime_quaternion([0.0, 0.0, quarter])
    got = rotate(yaw, [1.0, 0.0, 0.0])
    if np.abs(got - np.array([0.0, 0.0, 1.0])).max() > 1.0e-5:
        failures.append(f"creation_z_rotation_becomes_runtime_y_rotation: {got}")
    for z in (0.4, 1.3, 2.9, -1.8):
        q = creation_euler_to_runtime_quaternion([0.0, 0.0, z])
        forward = rotate(q, creation_to_runtime_vector([0.0, 1.0, 0.0]))
        expected = creation_to_runtime_vector([math.sin(z), math.cos(z), 0.0])
        if np.abs(forward - expected).max() > 1.0e-5:
            failures.append(f"a_yaw_turns_clockwise_like_a_heading at z={z}: {forward}")
    x_rotation = creation_euler_to_runtime_quaternion([quarter, 0.0, 0.0])
    got = rotate(x_rotation, [0.0, 0.0, -1.0])
    if np.abs(got - np.array([0.0, -1.0, 0.0])).max() > 1.0e-5:
        failures.append(f"rotations_about_creation_x: {got}")
    y_rotation = creation_euler_to_runtime_quaternion([0.0, quarter, 0.0])
    got = rotate(y_rotation, [1.0, 0.0, 0.0])
    if np.abs(got - np.array([0.0, 1.0, 0.0])).max() > 1.0e-5:
        failures.append(f"rotations_about_creation_y: {got}")
    return failures


# --------------------------------------------------------------------------------------
# Data
# --------------------------------------------------------------------------------------

def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def decode_heightmap(blob):
    """Absolute heights of a 33x33 LAND grid (same decoding as the converter)."""
    if blob is None or len(blob) < 4 + SIDE * SIDE:
        return None
    offset = np.frombuffer(blob[:4], dtype="<f4")[0] * 8.0
    deltas = np.frombuffer(blob[4 : 4 + SIDE * SIDE], dtype="i1").astype("f8")
    heights = np.zeros(SIDE * SIDE)
    row_origin = float(offset)
    for row in range(SIDE):
        row_origin += deltas[row * SIDE] * 8.0
        height = row_origin
        heights[row * SIDE] = height
        for column in range(1, SIDE):
            height += deltas[row * SIDE + column] * 8.0
            heights[row * SIDE + column] = height
    return heights.reshape(SIDE, SIDE)


def load_land(conn, worldspace_id):
    """cell_id -> (grid_x, grid_y, heights) for every LAND cell of a worldspace."""
    cells = {}
    query = (
        "select c.id, c.grid_x, c.grid_y, l.heightmap from cells c"
        " join land l on l.cell_id = c.id where c.worldspace_id = ?"
    )
    for cell_id, grid_x, grid_y, blob in conn.execute(query, (worldspace_id,)):
        if grid_x is None or grid_y is None:
            continue
        heights = decode_heightmap(blob)
        if heights is None:
            continue
        cells[cell_id] = (grid_x, grid_y, heights)
    return cells


def sample_terrain(cell, x, y):
    """(ground height, terrain normal) at a Creation-space point, or None outside."""
    grid_x, grid_y, heights = cell
    u = (x - grid_x * CELL_SIZE) / VERTEX_STEP
    v = (y - grid_y * CELL_SIZE) / VERTEX_STEP
    if not (-0.5 <= u <= SIDE - 0.5 and -0.5 <= v <= SIDE - 0.5):
        return None
    column = min(max(int(round(u)), 0), SIDE - 1)
    row = min(max(int(round(v)), 0), SIDE - 1)
    left = max(column - 1, 0)
    right = min(column + 1, SIDE - 1)
    down = max(row - 1, 0)
    up = min(row + 1, SIDE - 1)
    dz_dx = (heights[row][right] - heights[row][left]) / ((right - left) * VERTEX_STEP)
    dz_dy = (heights[up][column] - heights[down][column]) / ((up - down) * VERTEX_STEP)
    normal = np.array([-dz_dx, -dz_dy, 1.0])
    normal /= math.sqrt(float(np.dot(normal, normal)))
    # Bilinear height, so an object standing on the ground is not compared with a vertex.
    fu = min(max(u, 0.0), SIDE - 1.0)
    fv = min(max(v, 0.0), SIDE - 1.0)
    i0, j0 = int(fv), int(fu)
    i1, j1 = min(i0 + 1, SIDE - 1), min(j0 + 1, SIDE - 1)
    tu, tv = fu - j0, fv - i0
    height = (
        heights[i0][j0] * (1 - tu) * (1 - tv)
        + heights[i0][j1] * tu * (1 - tv)
        + heights[i1][j0] * (1 - tu) * tv
        + heights[i1][j1] * tu * tv
    )
    return float(height), normal


GROUPS = (
    # Order matters: a snow-covered shrub is a shrub, not a snow drift.
    (
        "flora",
        (
            "plants/",
            "flora",
            "shrub",
            "grass",
            "flower",
            "fern",
            "thicket",
            "vine",
            "moss",
            "bush",
            "nettle",
            "lichen",
            "kelp",
            "clover",
            "gourd",
        ),
    ),
    ("snow", ("drift", "snow", "ice")),
    (
        "rock",
        (
            "rocks/",
            "rock",
            "cliff",
            "boulder",
            "crag",
            "cairn",
            "mountain",
            "stone",
        ),
    ),
    (
        "tree",
        (
            "trees/",
            "tree",
            "pine",
            "aspen",
            "birch",
            "stump",
            "log",
            "branch",
            "trunk",
            "root",
            "treestump",
        ),
    ),
    ("building", ("farm", "house", "wall", "fence", "tower", "bridge", "ruin", "tomb")),
    ("clutter", ("clutter", "barrel", "crate", "bucket", "cart", "wheel", "basket", "pot")),
)


def is_hidden_model(model_path):
    """The engine does not spawn these: markers, effects and references with no model."""
    if not model_path:
        return True
    lowered = model_path.replace("\\", "/").lower()
    return lowered.startswith(("meshes/markers/", "markers/", "meshes/effects/", "effects/"))



def group_of(model_path, editor_id):
    text = f"{model_path or ''} {editor_id or ''}".lower()
    for name, needles in GROUPS:
        if any(needle in text for needle in needles):
            return name
    return "other"


def reference_query(worldspace_id):
    return (
        "select r.cell_id, r.pos_x, r.pos_y, r.pos_z, r.rot_x, r.rot_y, r.rot_z,"
        " r.scale, s.model_path, s.editor_id"
        ' from "references" r join cells c on c.id = r.cell_id'
        " left join statics s on s.id = r.base_form_id"
        " where c.worldspace_id = ? and r.is_exterior = 1"
    )


def select_cells(conn, worldspace_id, limit):
    """The cells to audit: those with land, at most `limit`, spread over the world."""
    ids = [
        row[0]
        for row in conn.execute(
            "select c.id from cells c join land l on l.cell_id = c.id"
            " where c.worldspace_id = ? order by c.grid_y, c.grid_x",
            (worldspace_id,),
        )
    ]
    if limit and len(ids) > limit:
        step = len(ids) / float(limit)
        ids = [ids[int(index * step)] for index in range(limit)]
    return ids


# --------------------------------------------------------------------------------------
# Modes
# --------------------------------------------------------------------------------------

def census(conn, worldspace_id, cell_limit, tilt_degrees):
    cells = load_land(conn, worldspace_id)
    chosen = set(select_cells(conn, worldspace_id, cell_limit))
    print(f"worldspace {worldspace_id}: {len(cells)} LAND cells, auditing {len(chosen)}")
    tilt = tilt_degrees * DEG
    offsets = []
    tilted = 0
    total = 0
    groups = {}
    models = {}
    slopes = []
    for cell_id, x, y, z, rx, ry, rz, scale, model, editor in conn.execute(
        reference_query(worldspace_id), (worldspace_id,)
    ):
        if cell_id not in chosen:
            continue
        total += 1
        if abs(rx) < tilt and abs(ry) < tilt:
            continue
        tilted += 1
        sample = sample_terrain(cells[cell_id], x, y)
        if sample is None:
            continue
        ground, normal = sample
        offsets.append(z - ground)
        slopes.append(math.degrees(math.acos(max(-1.0, min(1.0, normal[2])))))
        name = group_of(model, editor)
        groups[name] = groups.get(name, 0) + 1
        models[model or "(none)"] = models.get(model or "(none)", 0) + 1
    print(f"  {total} exterior references, {tilted} tilted more than {tilt_degrees} deg")
    if offsets:
        values = np.abs(np.array(offsets))
        print(
            "  |pos_z - terrain|: "
            + " ".join(
                f"p{p}={np.percentile(values, p):.0f}" for p in (5, 25, 50, 75, 90, 95)
            )
            + f" units, {int((values <= 128).sum())} within 128"
        )
        slope_values = np.array(slopes)
        print(
            "  terrain slope at those points: "
            + " ".join(
                f"p{p}={np.percentile(slope_values, p):.1f}" for p in (5, 25, 50, 75, 90, 95)
            )
            + " deg"
        )
    print("  groups: " + ", ".join(f"{k}={v}" for k, v in sorted(groups.items())))
    print("  most common models:")
    for model, count in sorted(models.items(), key=lambda item: -item[1])[:25]:
        print(f"    {count:6d}  {model}")


def audit(conn, worldspace_id, cell_limit, tilt_degrees, ground_limit, slope_min, options):
    failures = selftest()
    convention, error, runner_up_error = check_port()
    print(
        f"Port check: the ported function equals convention {label(convention)}"
        f" (max difference {error:.1e}; nearest other convention {runner_up_error:.2e})"
    )
    print(
        "Rust module self-test on this port: "
        + ("PASS" if not failures else "FAIL: " + "; ".join(failures))
    )
    cells = load_land(conn, worldspace_id)
    chosen = set(select_cells(conn, worldspace_id, cell_limit))
    tilt = tilt_degrees * DEG

    tilted = ground_kept = sloped_kept = hidden = 0
    collected_up = []
    collected_normal = []
    collected_group = []
    collected_rot = []
    collected_slope = []
    offsets = []
    for cell_id, x, y, z, rx, ry, rz, scale, model, editor in conn.execute(
        reference_query(worldspace_id), (worldspace_id,)
    ):
        if cell_id not in chosen:
            continue
        if is_hidden_model(model):
            hidden += 1
            continue
        if abs(rx) < tilt and abs(ry) < tilt:
            continue
        tilted += 1
        sample = sample_terrain(cells[cell_id], x, y)
        if sample is None:
            continue
        ground, normal = sample
        offset = z - ground
        if abs(offset) > ground_limit:
            continue
        ground_kept += 1
        slope = math.degrees(math.acos(max(-1.0, min(1.0, float(normal[2])))))
        if slope < slope_min:
            continue
        sloped_kept += 1
        rotation = (rx, ry, rz)
        collected_up.append(
            np.array(
                [
                    convention_matrix(candidate, rotation) @ np.array([0.0, 0.0, 1.0])
                    for candidate in CONVENTIONS
                ]
            )
        )
        collected_normal.append(normal)
        collected_group.append(group_of(model, editor))
        collected_rot.append(rotation)
        collected_slope.append(slope)
        offsets.append(offset)

    print(
        f"  tilted references: {tilted} ({hidden} marker/effect references skipped); "
        f"standing on the ground (|dz| <= {ground_limit}): {ground_kept}; "
        f"on ground sloped more than {slope_min} deg: {sloped_kept}"
    )
    if not collected_up:
        print("  nothing to score")
        return
    ups = np.array(collected_up)                    # (n, conventions, 3)
    norms = np.array(collected_normal)              # (n, 3)
    groups = np.array(collected_group)
    rotations = np.array(collected_rot)
    slopes = np.array(collected_slope)
    offsets = np.array(offsets)

    dots = np.einsum("ncj,nj->nc", ups, norms)
    angles = np.degrees(np.arccos(np.clip(dots, -1.0, 1.0)))

    print(f"\n  scored {len(angles)} references; median terrain slope {np.median(slopes):.1f} deg")
    print(
        f"  |dz| median {np.median(np.abs(offsets)):.0f} units, "
        f"tilt median {np.median(np.degrees(np.hypot(rotations[:, 0], rotations[:, 1]))):.1f} deg"
    )
    summary = []
    print("\n  convention                          median   mean   <10deg   <20deg")
    for index, candidate in enumerate(CONVENTIONS):
        column = angles[:, index]
        entry = {
            "convention": [candidate[0], list(candidate[1])],
            "label": label(candidate),
            "median": float(np.median(column)),
            "mean": float(np.mean(column)),
            "within10": float((column <= 10.0).mean()),
            "within20": float((column <= 20.0).mean()),
        }
        summary.append(entry)
        print(
            f"  {label(candidate):36.36} {entry['median']:7.2f} {entry['mean']:7.2f}"
            f" {entry['within10'] * 100:7.1f}% {entry['within20'] * 100:7.1f}%"
        )

    order = sorted(range(len(CONVENTIONS)), key=lambda i: summary[i]["median"])
    print(
        "  best by median: " + label(CONVENTIONS[order[0]])
        + f" ({summary[order[0]]['median']:.2f} deg); worst: "
        + f"{label(CONVENTIONS[order[-1]])} ({summary[order[-1]]['median']:.2f} deg)"
    )

    best = order[0]
    # Conventions that differ only in the z sign have exactly the same up vector and tie;
    # the runner-up is the first genuinely worse one.
    runner_up = next(
        index
        for index in order
        if summary[index]["median"] > summary[best]["median"] + 1.0e-9
    )
    print("\n  per group, median angle in degrees (best-ranked and worst conventions):")
    print("    group            n     " + "  ".join(f"{name:>12}" for name in ("best", "worst")))
    group_summary = {}
    for name in sorted(set(groups)):
        mask = groups == name
        if mask.sum() < 50:
            continue
        median_best = float(np.median(angles[mask, best]))
        median_worst = float(np.median(angles[mask, order[-1]]))
        group_summary[name] = {
            "count": int(mask.sum()),
            "median_best": median_best,
            "median_worst": median_worst,
            "median_by_convention": {
                label(CONVENTIONS[index]): float(np.median(angles[mask, index]))
                for index in range(len(CONVENTIONS))
            },
        }
        print(
            f"    {name:14.14} {int(mask.sum()):6d} {median_best:12.2f} {median_worst:14.2f}"
        )

    # The order question is a second-order effect: Rx and Ry commute to first order, so
    # only references tilted in both axes tell the two orders apart. Look at those alone.
    tilt_pair = np.degrees(np.minimum(np.abs(rotations[:, 0]), np.abs(rotations[:, 1])))
    print("\n  order discrimination: references tilted in both axes")
    print("    both-axis tilt       n    gap XYZ vs ZYX   median XYZ   median ZYX   winner")
    for threshold in (3.0, 10.0, 20.0, 30.0):
        mask = tilt_pair >= threshold
        if mask.sum() < 20:
            print(f"    >={threshold:4.1f} deg {int(mask.sum()):7d}   too few")
            continue
        for sign in ((-1, -1, -1), (1, 1, -1), (1, 1, 1)):
            first = CONVENTIONS.index(("XYZ", sign))
            second = CONVENTIONS.index(("ZYX", sign))
            gap = np.degrees(
                np.arccos(np.clip(np.einsum("nj,nj->n", ups[:, first], ups[:, second]), -1.0, 1.0))
            )
            median_first = float(np.median(angles[mask, first]))
            median_second = float(np.median(angles[mask, second]))
            print(
                f"    >={threshold:4.1f} deg {int(mask.sum()):7d}   "
                f"{np.median(gap[mask]):7.2f} deg     {median_first:7.2f}     "
                f"{median_second:7.2f}   "
                + ("XYZ" if median_first < median_second else "ZYX")
                + f"  signs {'+' if sign[0] > 0 else '-'}x"
                f"{'+' if sign[1] > 0 else '-'}y{'+' if sign[2] > 0 else '-'}z"
            )

    # Paired test: per reference, which of two conventions puts its up axis closer to the
    # terrain normal? A sign test over thousands of references needs no assumption about
    # how well objects follow slopes - only that the answer is the same for each of them.
    print("\n  paired sign test (per reference: which convention is closer to the normal?)")
    pairs = [
        ("a", "b"),
        ("a", "c"),
        ("a", "d"),
        ("a", "e"),
        ("b", "e"),
    ]
    paired_summary = {}
    for first_name, second_name in pairs:
        first = CONVENTIONS.index(NAMED[first_name])
        second = CONVENTIONS.index(NAMED[second_name])
        entry = {}
        for subset_name, mask in (
            ("all", np.ones(len(angles), dtype=bool)),
            ("both axes tilted >10 deg", tilt_pair >= 10.0),
        ):
            if mask.sum() < 20:
                continue
            left = angles[mask, first]
            right = angles[mask, second]
            wins = int((left < right - 1.0e-9).sum())
            losses = int((right < left - 1.0e-9).sum())
            total = wins + losses
            z = (wins - losses) / math.sqrt(total) if total else 0.0
            entry[subset_name] = {
                "n": int(mask.sum()),
                "wins": wins,
                "losses": losses,
                "fraction": wins / total if total else 0.0,
                "z": z,
            }
            print(
                f"    ({first_name}) beats ({second_name}) on {wins / total * 100:5.1f}%"
                f" of {total:5d} references  ({subset_name}, z={z:6.1f})"
            )
        paired_summary[f"{first_name}_vs_{second_name}"] = entry

    path = options.get("json")
    if path:
        payload = {
            "worldspace_id": worldspace_id,
            "cells": len(chosen),
            "tilt_degrees": tilt_degrees,
            "ground_limit": ground_limit,
            "slope_min": slope_min,
            "counts": {
                "tilted": tilted,
                "on_ground": ground_kept,
                "on_slope": sloped_kept,
            },
            "samples": int(len(angles)),
            "median_slope_degrees": float(np.median(slopes)),
            "median_tilt_degrees": float(
                np.median(np.degrees(np.hypot(rotations[:, 0], rotations[:, 1])))
            ),
            "conventions": summary,
            "best": summary[best]["label"],
            "runner_up_different_convention": summary[runner_up]["label"],
            "groups": group_summary,
            "paired": paired_summary,
            "port_check": {
                "equals": label(convention),
                "max_difference": error,
                "nearest_other": runner_up_error,
                "rust_selftest_failures": failures,
            },
        }
        import os

        if "/" in path:
            os.makedirs(path.rsplit("/", 1)[0], exist_ok=True)
        with open(path, "w") as handle:
            json.dump(payload, handle, indent=1)
        print(f"\n  wrote {path}")


def model_rotation_transforms(rotation, scale, bounds, position):
    """The eight Creation-space corners of a reference's model box, as the engine places it.

    `bounds` is `(min_x, min_y, min_z, max_x, max_y, max_z)` in runtime coordinates, the
    form `statics` stores (measured from the converted glTF); it is mapped to Creation
    coordinates and then scaled, rotated and translated exactly like
    `crates/engine/src/streaming.rs` does for the transform.
    """
    min_x, min_y, min_z, max_x, max_y, max_z = bounds
    creation_min = np.array([min_x, -max_z, min_y])
    creation_max = np.array([max_x, -min_z, max_y])
    matrix = convention_matrix(("XYZ", (-1, -1, -1)), rotation)
    corners = np.array(
        [
            [creation_min[0] if i & 1 == 0 else creation_max[0],
             creation_min[1] if i & 2 == 0 else creation_max[1],
             creation_min[2] if i & 4 == 0 else creation_max[2]]
            for i in range(8)
        ]
    )
    scaled = corners * scale
    return (scaled @ matrix.T) + np.array(position)


def placement(conn, worldspace_id, cell_limit, ground_limit, options):
    """Position/scale audit: does a reference stand where the terrain says it should?"""
    box_tilt = float(options.get("box_tilt", 3.0)) * DEG
    cells = load_land(conn, worldspace_id)
    chosen = sorted(select_cells(conn, worldspace_id, cell_limit))
    allowed = set(chosen)
    cell_keys = list(cells)
    rng = np.random.default_rng(20260922)
    controls = {cell_id: cell_keys[int(rng.integers(len(cell_keys)))] for cell_id in chosen}
    query = (
        "select r.cell_id, r.pos_x, r.pos_y, r.pos_z, r.rot_x, r.rot_y, r.rot_z, r.scale,"
        " s.model_path, s.editor_id, s.bounds_min_x, s.bounds_min_y, s.bounds_min_z,"
        " s.bounds_max_x, s.bounds_max_y, s.bounds_max_z, s.bounds_valid"
        ' from "references" r join cells c on c.id = r.cell_id'
        " left join statics s on s.id = r.base_form_id"
        " where c.worldspace_id = ? and r.is_exterior = 1"
    )
    offsets = []
    control_offsets = []
    scales = []
    ground_rows = []           # (group, model, lowest corner above the terrain)
    for row in conn.execute(query, (worldspace_id,)):
        cell_id, x, y, z = row[0], row[1], row[2], row[3]
        rotation, scale = (row[4], row[5], row[6]), row[7]
        model, editor = row[8], row[9]
        bounds_valid = row[16]
        if cell_id not in allowed or is_hidden_model(model):
            continue
        scales.append(scale)
        sample = sample_terrain(cells[cell_id], x, y)
        if sample is None:
            continue
        ground, _ = sample
        offsets.append(z - ground)
        other = controls.get(cell_id)
        if other is not None and other != cell_id:
            grid_x, grid_y, _ = cells[other]
            control = sample_terrain(
                cells[other],
                grid_x * CELL_SIZE + (x % CELL_SIZE),
                grid_y * CELL_SIZE + (y % CELL_SIZE),
            )
            if control is not None:
                control_offsets.append(z - control[0])
        # The model box is axis-aligned in model space, so under tilt it is larger than the
        # object and dips below it: only upright references measure the geometry itself.
        upright = abs(rotation[0]) < box_tilt and abs(rotation[1]) < box_tilt
        if not bounds_valid or not upright or abs(z - ground) > ground_limit:
            continue
        bounds = (row[10], row[11], row[12], row[13], row[14], row[15])
        corners = model_rotation_transforms(rotation, scale, bounds, (x, y, z))
        if not np.isfinite(corners).all():
            continue
        ground_rows.append(
            (group_of(model, editor), model or "(none)", float(corners[:, 2].min() - ground))
        )

    offsets = np.abs(np.array(offsets))
    control_offsets = np.abs(np.array(control_offsets))
    print(f"\n  references with a model and terrain under them: {len(offsets)}")
    print(
        "  |reference origin z - terrain z|: "
        + " ".join(f"p{p}={np.percentile(offsets, p):.0f}" for p in (25, 50, 75, 90))
        + f" units, {int((offsets <= 64).sum() * 100.0 / len(offsets))}% within 64"
    )
    print(
        "  control (terrain of a random other cell): "
        + " ".join(f"p{p}={np.percentile(control_offsets, p):.0f}" for p in (25, 50, 75, 90))
        + f" units, {int((control_offsets <= 64).sum() * 100.0 / len(control_offsets))}% within 64"
    )
    scales = np.array(scales)
    print(
        f"  scale column: {len(scales)} values, min {scales.min():.3f}, max {scales.max():.3f}, "
        f"{(scales == 1.0).mean() * 100:.1f}% exactly 1, "
        f"{int((scales <= 0).sum())} at or below zero"
    )

    values = np.array([row[2] for row in ground_rows])
    print(
        f"\n  model box bottom above the terrain, for upright references standing on the "
        f"ground (|dz| <= {ground_limit}, tilt < {math.degrees(box_tilt):.0f} deg): n={len(values)}"
    )
    if len(values):
        print(
            "    "
            + " ".join(f"p{p}={np.percentile(values, p):+.0f}" for p in (10, 25, 50, 75, 90))
            + f" units, {int((np.abs(values) <= 64).mean() * 100)}% within +-64"
        )
        by_group = {}
        for group, _, offset in ground_rows:
            by_group.setdefault(group, []).append(offset)
        for group, group_values in sorted(by_group.items(), key=lambda item: -len(item[1])):
            if len(group_values) < 50:
                continue
            array = np.array(group_values)
            print(
                f"    {group:10.10} n={len(array):6d} median {np.median(array):+7.0f} units "
                f"p10 {np.percentile(array, 10):+7.0f} p90 {np.percentile(array, 90):+7.0f}"
            )
        by_model = {}
        for _, model, offset in ground_rows:
            by_model.setdefault(model, []).append(offset)
        worst = sorted(
            (
                (float(np.median(np.array(v))), k, len(v))
                for k, v in by_model.items()
                if len(v) >= 20
            ),
            key=lambda item: -abs(item[0]),
        )
        print("    models with >=20 placements whose box bottom is furthest from the terrain:")
        for offset, model, count in worst[:15]:
            print(f"      {offset:+8.0f} units  n={count:4d}  {model}")


def model_report(conn, names):
    """Where a converted model's geometry sits relative to its own origin.

    Prints the POSITION accessor bounds of the glb (model space, before any node
    transform), the node transforms on the path from the scene root, and the bounds the
    converter stored in `statics` (which is what the engine culls and grounds with).
    """
    import glb_materials
    import scene_materials

    for name in names:
        print(f"== {name}")
        rows = list(
            conn.execute(
                "select id, model_path, bounds_min_x, bounds_min_y, bounds_min_z,"
                " bounds_max_x, bounds_max_y, bounds_max_z, bounds_valid from statics"
                " where model_path like ? collate nocase limit 1",
                (f"%{name}%",),
            )
        )
        if not rows:
            print("   no statics row matches")
            continue
        row = rows[0]
        print(f"   statics {row[0]:08X} {row[1]} bounds_valid={row[8]}")
        print(
            "   statics bounds (runtime space): min "
            f"({row[2]:.0f}, {row[3]:.0f}, {row[4]:.0f}) max ({row[5]:.0f}, {row[6]:.0f}, {row[7]:.0f})"
        )
        path = scene_materials.glb_path(row[1])
        if not path:
            print("   no converted glb")
            continue
        document = glb_materials.read_json_chunk(path)
        if document is None:
            print(f"   cannot read {path}")
            continue
        accessors = document.get("accessors", [])
        low = [float("inf")] * 3
        high = [float("-inf")] * 3
        for mesh in document.get("meshes", []):
            for primitive in mesh.get("primitives", []):
                index = primitive.get("attributes", {}).get("POSITION")
                if index is None or index >= len(accessors):
                    continue
                accessor = accessors[index]
                minimum, maximum = accessor.get("min"), accessor.get("max")
                if not minimum or not maximum:
                    continue
                for axis in range(3):
                    low[axis] = min(low[axis], minimum[axis])
                    high[axis] = max(high[axis], maximum[axis])
        print(
            f"   glb accessor bounds:          min ({low[0]:.0f}, {low[1]:.0f}, {low[2]:.0f})"
            f" max ({high[0]:.0f}, {high[1]:.0f}, {high[2]:.0f})"
        )
        for number, mesh in enumerate(document.get("meshes", [])):
            for primitive in mesh.get("primitives", []):
                index = primitive.get("attributes", {}).get("POSITION")
                if index is None or index >= len(accessors):
                    continue
                accessor = accessors[index]
                minimum, maximum = accessor.get("min"), accessor.get("max")
                if not minimum or not maximum:
                    continue
                attributes = ",".join(sorted(primitive.get("attributes", {})))
                print(
                    f"   mesh {number} {mesh.get('name', '')!r} ({accessor.get('count')} verts):"
                    f" min ({minimum[0]:.0f}, {minimum[1]:.0f}, {minimum[2]:.0f})"
                    f" max ({maximum[0]:.0f}, {maximum[1]:.0f}, {maximum[2]:.0f})"
                    f"  [{attributes}]"
                )
        nodes = document.get("nodes", [])
        children = {child for node in nodes for child in node.get("children", [])}
        for number, node in enumerate(nodes):
            if number in children:
                continue
            print(f"   root node {number} {node.get('name', '')!r}")
            print_node_tree(nodes, number, "     ")
        print(
            f"   {len(nodes)} nodes, {len(document.get('meshes', []))} meshes, "
            f"{len(document.get('skins', []))} skins, "
            f"extensions {sorted(document.get('extensionsUsed', []))}"
        )


def print_node_tree(nodes, number, indent, depth=0):
    if depth > 6:
        print(f"{indent}...")
        return
    node = nodes[number]
    parts = []
    if "mesh" in node:
        parts.append(f"mesh {node['mesh']}")
    if node.get("skin") is not None:
        parts.append(f"skin {node['skin']}")
    for key in ("translation", "rotation", "scale", "matrix"):
        if key in node:
            parts.append(
                key + "=[" + ", ".join(f"{value:.2f}" for value in node[key]) + "]"
            )
    print(f"{indent}node {number} {node.get('name', '')!r} {' '.join(parts)}")
    for child in node.get("children", []):
        print_node_tree(nodes, child, indent + "  ", depth + 1)


def main(argv):
    options = {}
    positional = []
    index = 1
    while index < len(argv):
        argument = argv[index]
        if argument.startswith("--"):
            key = argument[2:]
            value = argv[index + 1]
            options[key] = value
            index += 2
        else:
            positional.append(argument)
            index += 1
    if not positional:
        print(__doc__)
        return 2
    mode = positional[0]
    worldspace_id = int(options.get("world", 60))
    cell_limit = int(options.get("cells", 400))
    tilt_degrees = float(options.get("tilt", 3.0))
    conn = connect()
    if mode == "selftest":
        failures = selftest()
        convention, error, runner_up = check_port()
        print("Rust module self-test on this port: " + ("PASS" if not failures else "FAIL"))
        for failure in failures:
            print(f"  {failure}")
        print(
            f"Ported function equals {label(convention)} (max difference {error:.1e}; "
            f"nearest other convention {runner_up:.2e})"
        )
    elif mode == "census":
        census(conn, worldspace_id, cell_limit, tilt_degrees)
    elif mode == "audit":
        audit(
            conn,
            worldspace_id,
            cell_limit,
            tilt_degrees,
            float(options.get("ground", 128.0)),
            float(options.get("slope", 5.0)),
            options,
        )
    elif mode == "model":
        model_report(conn, positional[1:])
    elif mode == "placement":
        placement(
            conn, worldspace_id, cell_limit, float(options.get("ground", 128.0)), options
        )
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
