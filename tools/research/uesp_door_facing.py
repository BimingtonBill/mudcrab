#!/usr/bin/env python3
"""Which way does each load door face?  Read it from the door link data.

For a load door D, the link *into* D is the `door_links` row whose
`destination_ref_id` is D.  That row's arrival point is where the player stands
after coming through D, which the game puts in front of the door, so the camera
that sees the door's front is on that side of it.

Usage:
    python tools/research/uesp_door_facing.py box <x> <y> <z> <radius> [model substring]
    python tools/research/uesp_door_facing.py ref <ref id hex> [<ref id hex> ...]

Read-only.  Positions are Creation units; the heading is Skyrim's, clockwise
from north (0 = +Y, 90 = +X), the same convention as the shots file.
"""

import math
import sqlite3
import sys

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"


def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def heading(dx, dy):
    """Skyrim heading of the direction (dx, dy): clockwise from +Y."""
    return math.degrees(math.atan2(dx, dy)) % 360.0


def describe(conn, ref_id, label):
    row = conn.execute(
        'select r.id, r.cell_id, r.worldspace_id, r.pos_x, r.pos_y, r.pos_z,'
        " s.editor_id, s.model_path from \"references\" r"
        " left join statics s on s.id = r.base_form_id where r.id = ?",
        (ref_id,),
    ).fetchone()
    if row is None:
        print(f"  {label}: ref {ref_id:08X} is not in the database")
        return
    print(
        f"  {label}: ref {row[0]:08X} cell {row[1]} world {row[2]} at "
        f"({row[3]:.0f},{row[4]:.0f},{row[5]:.0f}) {row[6]} {row[7]}"
    )
    # The link out of this door: where a player who uses it lands.
    out = conn.execute("select * from door_links where ref_id = ?", (ref_id,)).fetchone()
    if out is not None:
        destination = out[1]
        rotation = ",".join(
            "?" if value is None else f"{value:.3f}" for value in (out[7], out[8], out[9])
        )
        print(
            f"    -> destination ref {destination:08X} cell {out[2]} world {out[3]}"
            f"  arrival ({out[4]:.0f},{out[5]:.0f},{out[6]:.0f}) rot ({rotation})"
        )
        target = conn.execute(
            "select pos_x, pos_y from \"references\" where id = ?", (destination,)
        ).fetchone()
        if target is not None:
            print(
                f"       arrival is {math.hypot(out[4] - target[0], out[5] - target[1]):.0f}"
                f" units from the destination door, at heading"
                f" {heading(out[4] - target[0], out[5] - target[1]):.0f} of it"
            )
    # The link into this door: where a player who comes through it lands.
    for row in conn.execute(
        'select d.ref_id, r.pos_x, r.pos_y, r.pos_z, d.pos_x, d.pos_y, d.pos_z, d.rot_x,'
        " d.rot_y, d.rot_z from door_links d join \"references\" r on r.id = d.ref_id"
        " where d.destination_ref_id = ?",
        (ref_id,),
    ):
        dx, dy = row[4] - row[1], row[5] - row[2]
        print(
            f"    <- from ref {row[0]:08X} at ({row[1]:.0f},{row[2]:.0f},{row[3]:.0f}):"
            f" arrival ({row[4]:.0f},{row[5]:.0f},{row[6]:.0f}),"
            f" {math.hypot(dx, dy):.0f} units away at heading {heading(dx, dy):.0f}"
            f" of the door, XTEL heading {math.degrees(row[9]) % 360.0:.0f}"
        )


def main(argv):
    conn = connect()
    if len(argv) < 3:
        print(__doc__)
        return 2
    if argv[1] == "box":
        x, y, z, radius = (float(v) for v in argv[2:6])
        needle = argv[6].lower() if len(argv) > 6 else ""
        query = (
            "select distinct d.ref_id, r.pos_x, r.pos_y, r.pos_z, s.editor_id, s.model_path"
            " from door_links d join \"references\" r on r.id = d.ref_id"
            " left join statics s on s.id = r.base_form_id"
            " where r.pos_x between ? and ? and r.pos_y between ? and ?"
            " and r.pos_z between ? and ? order by r.pos_x"
        )
        parameters = (x - radius, x + radius, y - radius, y + radius, z - radius, z + radius)
        for row in conn.execute(query, parameters):
            text = ((row[4] or "") + " " + (row[5] or "")).lower()
            if needle and needle not in text:
                continue
            print()
            describe(conn, row[0], "door")
    elif argv[1] == "ref":
        for argument in argv[2:]:
            print()
            describe(conn, int(argument, 16), "door")
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
