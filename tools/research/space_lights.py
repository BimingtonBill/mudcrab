#!/usr/bin/env python3
"""Report the `LIGH` references of a space, and how they cover a camera pose.

Usage:
    python tools/research/space_lights.py cell <cell form id hex> [<x> <y> <z>]
    python tools/research/space_lights.py world <worldspace id> [<x> <y> <z>]
    python tools/research/space_lights.py coverage <space> <x> <y> <z>

`cell` / `world` list every `LIGH` reference in the space: its form id, position, the radius the
engine will give it (the reference's `XRDS` override, else the record's radius), and its colour.
With a position argument the rows are sorted by distance from it and each light's reach is
compared with that distance.

`coverage` answers the question the light calibration turns on: **how much of the space around a
pose is inside some light's radius?** For a pose it samples the space's references (the surfaces),
and for each surface within `--view` units of the pose reports how many lights reach it and what
their total contribution would be if each were scaled to deliver the interior ambient at half its
radius. If most surfaces are out of every light, brightening the lights cannot match a reference
that is lit everywhere, and the ambient is the knob instead.

Read-only.  Positions are Creation units, radii and distances the same.
"""

import argparse
import math
import sqlite3
import sys

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"

# From `crates/engine/src/app.rs`: the interior and underground ambient brightnesses, and the
# `EXPOSURE_CALIBRATION` of `crates/engine/src/lights.rs` that scales a light's target illuminance.
INTERIOR_AMBIENT = 140.0
OFF_BY_DEFAULT = 0x0000_0020
NEGATIVE = 0x0000_0004
HALF_RADIUS_WINDOW = 225.0 / 256.0


def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def light_rows(conn, cell_id=None, worldspace_id=None):
    """Every LIGH reference of a space with the data the engine converts it with."""
    where, parameters = ("c.id = ?", (cell_id,)) if cell_id is not None else (
        "c.worldspace_id = ?",
        (worldspace_id,),
    )
    query = (
        "select r.id, r.cell_id, r.pos_x, r.pos_y, r.pos_z, "
        "       COALESCE(r.radius_override, l.radius), l.radius, l.color_r, l.color_g, l.color_b, "
        "       l.flags, l.editor_id, l.falloff, l.fade "
        'from "references" r '
        "join cells c on c.id = r.cell_id "
        "join lights l on l.id = r.base_form_id "
        f"where {where}"
    )
    for row in conn.execute(query, parameters):
        yield {
            "form_id": row[0],
            "cell_id": row[1],
            "position": (row[2], row[3], row[4]),
            "radius": row[5],
            "record_radius": row[6],
            "color": (row[7], row[8], row[9]),
            "flags": row[10],
            "editor_id": row[11] or "",
            "falloff": row[12],
            "fade": row[13],
        }


def usable(light):
    """The engine's own filter: negative and off-by-default lights spawn nothing, and a radius
    that is not positive and finite lights nothing."""
    if light["flags"] & (NEGATIVE | OFF_BY_DEFAULT):
        return False
    radius = light["radius"]
    return radius is not None and math.isfinite(radius) and radius > 0.0


def distance(a, b):
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


def describe(light, center=None):
    x, y, z = light["position"]
    row = (
        f"  {light['form_id']:08X} cell {light['cell_id']:08X} ({x:9.0f}, {y:9.0f}, {z:9.0f}) "
        f"r={light['radius'] if light['radius'] is not None else float('nan'):8.1f} "
        f"rgb=({light['color'][0]:3d},{light['color'][1]:3d},{light['color'][2]:3d}) "
        f"flags=0x{light['flags']:04X} falloff={light['falloff']:.1f} {light['editor_id'][:28]}"
    )
    if center is not None:
        d = distance(light["position"], center)
        inside = "IN " if d <= light["radius"] else "   "
        row += f" d={d:8.0f} {inside}"
    return row


def command_space(args, cell_id=None, worldspace_id=None):
    center = tuple(args.position) if args.position else None
    lights = [light for light in light_rows(conn=args.connection, cell_id=cell_id,
                                            worldspace_id=worldspace_id)]
    live = [light for light in lights if usable(light)]
    if center is not None:
        live.sort(key=lambda light: distance(light["position"], center))
    print(
        f"space: {len(lights)} LIGH references, {len(live)} the engine lights "
        f"({len(lights) - len(live)} skipped by flags or radius)"
    )
    for light in live:
        print(describe(light, center))
    skipped = [light for light in lights if not usable(light)]
    for light in skipped[:10]:
        print(describe(light) + "   SKIPPED")
    if center is not None and live:
        radii = sorted(light["radius"] for light in live)
        median = radii[len(radii) // 2]
        within = sum(1 for light in live if distance(light["position"], center) <= light["radius"])
        print(
            f"  radii: min {radii[0]:.0f} median {median:.0f} max {radii[-1]:.0f}; "
            f"{within} of {len(live)} reach the pose"
        )


def command_coverage(args, cell_id=None, worldspace_id=None):
    """How much of the space around a pose a LIGH reference reaches."""
    center = tuple(args.position)
    lights = [
        light
        for light in light_rows(conn=args.connection, cell_id=cell_id,
                                worldspace_id=worldspace_id)
        if usable(light)
    ]
    query = (
        'select pos_x, pos_y, pos_z from "references" where '
        + ("cell_id = ?" if cell_id is not None else "1=1")
    )
    parameters = (cell_id,) if cell_id is not None else ()
    if worldspace_id is not None:
        query = (
            'select r.pos_x, r.pos_y, r.pos_z from "references" r '
            "join cells c on c.id = r.cell_id where c.worldspace_id = ?"
            " and r.pos_x between ? and ? and r.pos_y between ? and ? and r.pos_z between ? and ?"
        )
        parameters = (
            worldspace_id,
            center[0] - args.view,
            center[0] + args.view,
            center[1] - args.view,
            center[1] + args.view,
            center[2] - args.view,
            center[2] + args.view,
        )

    total = reached = 0
    nearest_outside = []
    for surface in args.connection.execute(query, parameters):
        if distance(surface, center) > args.view:
            continue
        total += 1
        best = None
        for light in lights:
            d = max(distance(light["position"], surface), 1.0)
            if d <= light["radius"]:
                # The illuminance the engine would deliver, in units of the interior ambient.
                window = (1.0 - (d / light["radius"]) ** 4) ** 2
                value = (light["radius"] * 0.5) ** 2 / HALF_RADIUS_WINDOW * window / (d * d)
                best = value if best is None else max(best, value)
        if best is None:
            nearest_outside.append(distance(surface, center))
        else:
            reached += 1
    print(f"  {args.space}: {total} surfaces within {args.view:.0f} of the pose")
    if total:
        print(f"    inside some light's radius: {reached} ({reached / total * 100:.1f}%)")
        if nearest_outside:
            nearest_outside.sort()
            print(
                f"    of the {len(nearest_outside)} that no light reaches, distance from the "
                f"pose: median {nearest_outside[len(nearest_outside) // 2]:.0f}, "
                f"max {nearest_outside[-1]:.0f}"
            )


def command_falloff(connection):
    """What `DATA`'s falloff exponent and `FNAM` fade actually hold, for every LIGH record.

    The engine loads both and applies neither (`lights.rs`, "What is not done here"): Bevy decays
    inverse-square with a range window. If the records mostly hold 1.0, nothing is lost; if they do
    not, then a light's real reach is not the one the engine gives it.
    """
    print(f"  {'falloff exponent':>18}  {'records':>8}  {'fade present':>12}")
    for row in connection.execute(
        "select falloff, count(*), sum(fade is not null) from lights "
        "group by falloff order by count(*) desc limit 20"
    ):
        print(f"  {row[0]:>18.3f}  {row[1]:>8}  {row[2]:>12}")
    total = connection.execute("select count(*) from lights").fetchone()[0]
    with_fade = connection.execute("select count(*) from lights where fade is not null").fetchone()[0]
    print(f"  {total} LIGH records, {with_fade} with an FNAM fade")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("cell", "world", "coverage", "falloff"))
    parser.add_argument("space", nargs="?", help="cell form id in hex, or a worldspace id")
    parser.add_argument("position", nargs="*", type=float, help="x y z of the camera pose")
    parser.add_argument("--view", type=float, default=4096.0,
                        help="surfaces within this distance of the pose (coverage)")
    args = parser.parse_args()

    connection = connect()
    try:
        args.connection = connection
        if args.command == "falloff":
            command_falloff(connection)
            return 0
        if args.command == "cell":
            cell_id = int(args.space, 16)
            args.space = f"cell {cell_id:08X}"
            command_space(args, cell_id=cell_id)
        elif args.command == "world":
            worldspace_id = int(args.space, 16)
            args.space = f"world {worldspace_id}"
            command_space(args, worldspace_id=worldspace_id)
        else:
            if len(args.position) != 3:
                raise SystemExit("coverage needs x y z")
            # `coverage <cell hex>` or `coverage world:<id>`.
            if args.space.startswith("world:"):
                args.space = f"world {int(args.space.split(':', 1)[1], 16)}"
                command_coverage(args, worldspace_id=int(args.space.split()[-1]))
            else:
                cell_id = int(args.space, 16)
                args.space = f"cell {cell_id:08X}"
                command_coverage(args, cell_id=cell_id)
    finally:
        connection.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
