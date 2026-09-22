#!/usr/bin/env python3
"""List the individual placed objects of a space or a box, one line each.

`uesp_probe.py box` collapses references into per-base averages; this prints
every reference with its own position, rotation and scale, which is what a
camera pose has to be reasoned against.

Usage:
    python tools/research/uesp_ref_list.py box <x> <y> <z> <radius> [model substring]
    python tools/research/uesp_ref_list.py cell <cell id hex> [model substring]
    python tools/research/uesp_ref_list.py world <worldspace id> <x> <y> <radius>
        [model substring]

Positions are Creation units, rotations degrees (Creation's own clockwise
convention), scale as stored on the reference.
"""

import sqlite3
import sys

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"


def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def print_rows(rows, needle):
    shown = 0
    for row in rows:
        text = ((row[6] or "") + " " + (row[7] or "")).lower()
        if needle and needle not in text:
            continue
        print(
            f"  {row[0]:08X} cell {row[1]:7d} ({row[2]:9.0f},{row[3]:9.0f},{row[4]:9.0f})"
            f" rot({row[5][0]:7.1f},{row[5][1]:7.1f},{row[5][2]:7.1f}) s{row[8]:.2f}"
            f" {(row[6] or '')[:30]:30.30} {row[7] or ''}"
        )
        shown += 1
    print(f"  ({shown} shown)")


def main(argv):
    conn = connect()
    if len(argv) < 3:
        print(__doc__)
        return 2
    command = argv[1]
    select = (
        'select r.id, r.cell_id, r.pos_x, r.pos_y, r.pos_z,'
        " r.rot_x, r.rot_y, r.rot_z, s.editor_id, s.model_path, r.scale"
        ' from "references" r left join statics s on s.id = r.base_form_id'
    )
    if command == "box":
        x, y, z, radius = (float(v) for v in argv[2:6])
        needle = argv[6].lower() if len(argv) > 6 else ""
        query = (
            select + " where r.pos_x between ? and ? and r.pos_y between ? and ?"
            " and r.pos_z between ? and ? order by r.pos_y, r.pos_x"
        )
        parameters = (x - radius, x + radius, y - radius, y + radius, z - radius, z + radius)
        rows = [
            (r[0], r[1], r[2], r[3], r[4], (r[5] or 0.0, r[6] or 0.0, r[7] or 0.0), r[8], r[9], r[10])
            for r in conn.execute(query, parameters)
        ]
        print_rows(rows, needle)
    elif command == "cell":
        cell_id = int(argv[2], 16)
        needle = argv[3].lower() if len(argv) > 3 else ""
        rows = [
            (r[0], r[1], r[2], r[3], r[4], (r[5] or 0.0, r[6] or 0.0, r[7] or 0.0), r[8], r[9], r[10])
            for r in conn.execute(select + " where r.cell_id = ? order by r.pos_y, r.pos_x", (cell_id,))
        ]
        print_rows(rows, needle)
    elif command == "world":
        world = int(argv[2])
        x, y, radius = (float(v) for v in argv[3:6])
        needle = argv[6].lower() if len(argv) > 6 else ""
        query = (
            select + " where r.worldspace_id = ? and r.pos_x between ? and ?"
            " and r.pos_y between ? and ? order by r.pos_y, r.pos_x"
        )
        parameters = (world, x - radius, x + radius, y - radius, y + radius)
        rows = [
            (r[0], r[1], r[2], r[3], r[4], (r[5] or 0.0, r[6] or 0.0, r[7] or 0.0), r[8], r[9], r[10])
            for r in conn.execute(query, parameters)
        ]
        print_rows(rows, needle)
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
