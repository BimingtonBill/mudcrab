#!/usr/bin/env python3
"""Probe the converter database for the pieces the UESP reference shots show.

Usage:
    python tools/research/uesp_probe.py cells <name substring>
    python tools/research/uesp_probe.py space <cell id hex>|world:<worldspace id> [model substring]
    python tools/research/uesp_probe.py doors <cell id hex>
    python tools/research/uesp_probe.py route
    python tools/research/uesp_probe.py extents <cell id hex>
    python tools/research/uesp_probe.py models <cell id hex> <substring>
    python tools/research/uesp_probe.py base-near <cell id hex> <x> <y> <z> <radius>

Read-only.  Positions are Creation units.
"""

import sqlite3
import sys

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"


def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def cells(conn, needle):
    query = (
        "select id, worldspace_id, grid_x, grid_y, interior_name,"
        " (select count(*) from \"references\" r where r.cell_id = cells.id)"
        " from cells where interior_name like ? order by id"
    )
    for row in conn.execute(query, (f"%{needle}%",)):
        interior = "interior" if row[1] is None or row[1] == 0 else f"world {row[1]}"
        print(
            f"  cell {row[0]:9d} 0x{row[0]:08X} {interior:>12} grid ({row[2]}, {row[3]}) "
            f"refs {row[5]:6d} {row[4]!r}"
        )


def refs(conn, cell_id, model=None, limit=4000, world=None):
    where = "where r.worldspace_id = ?" if world is not None else "where r.cell_id = ?"
    key = world if world is not None else cell_id
    query = (
        "select r.id, r.cell_id, r.base_form_id, s.model_path, s.editor_id,"
        " r.pos_x, r.pos_y, r.pos_z, r.rot_x, r.rot_y, r.rot_z, r.scale"
        ' from "references" r left join statics s on s.id = r.base_form_id '
        + where
        + " order by r.pos_z desc limit ?"
    )
    count = 0
    for row in conn.execute(query, (key, limit)):
        model_path = row[3] or ""
        editor = row[4] or ""
        if model and model.lower() not in (model_path + " " + editor).lower():
            continue
        print(
            f"  {row[0]:08X} cell {row[1]:9d} base {row[2]:08X} "
            f"({row[5]:9.0f},{row[6]:9.0f},{row[7]:9.0f}) rot({row[8]:6.1f},{row[9]:6.1f},"
            f"{row[10]:6.1f}) s{row[11]:.2f} {editor:32.32} {model_path}"
        )
        count += 1
    print(f"  ({count} shown)")


def extents(conn, cell_id, world=None):
    where = "worldspace_id = ?" if world is not None else "cell_id = ?"
    key = world if world is not None else cell_id
    row = conn.execute(
        "select count(*), min(pos_x), max(pos_x), min(pos_y), max(pos_y), min(pos_z),"
        " max(pos_z), avg(pos_x), avg(pos_y), avg(pos_z) from \"references\" where " + where,
        (key,),
    ).fetchone()
    print(
        f"  {row[0]} refs, x {row[1]:.0f}..{row[2]:.0f}, y {row[3]:.0f}..{row[4]:.0f}, "
        f"z {row[5]:.0f}..{row[6]:.0f}, centroid ({row[7]:.0f},{row[8]:.0f},{row[9]:.0f})"
    )


def doors(conn, cell_id):
    query = (
        "select r.id, d.destination_ref_id, d.destination_cell_id,"
        " d.destination_worldspace_id, r.pos_x, r.pos_y, r.pos_z,"
        " c.interior_name, w.editor_id, r.base_form_id"
        ' from door_links d join "references" r on r.id = d.ref_id'
        " left join cells c on c.id = d.destination_cell_id"
        " left join worldspaces w on w.id = d.destination_worldspace_id"
        " where r.cell_id = ?"
    )
    for row in conn.execute(query, (cell_id,)):
        destination = row[7] or row[8] or ""
        print(
            f"  door ref {row[0]:08X} base {row[9]:08X} -> cell {row[2] or 0} world "
            f"{row[3]} {destination!r} at ({row[4]:.0f},{row[5]:.0f},{row[6]:.0f})"
        )


def route(conn):
    for door in (0x15D48, 0x92809, 0x9256A, 0x6998D):
        row = conn.execute(
            "select r.id, r.cell_id, r.pos_x, r.pos_y, r.pos_z, d.destination_ref_id,"
            " d.destination_cell_id, d.destination_worldspace_id, d.pos_x, d.pos_y, d.pos_z,"
            " d.rot_x, d.rot_y, d.rot_z, s.model_path"
            ' from door_links d join "references" r on r.id = d.ref_id'
            " left join statics s on s.id = r.base_form_id where r.id = ?",
            (door,),
        ).fetchone()
        if row is None:
            print(f"  door {door:08X}: no door link (maybe the destination end)")
        else:
            print(
                f"  door {row[0]:08X} cell {row[1]} ({row[2]:.0f},{row[3]:.0f},{row[4]:.0f}) "
                f"-> ref {row[5]:08X} cell {row[6]} world {row[7]} "
                f"arrival ({row[8]:.0f},{row[9]:.0f},{row[10]:.0f}) rot ({row[11]:.1f},"
                f"{row[12]:.1f},{row[13]:.1f})\n      {row[14]}"
            )
        other = conn.execute(
            "select d.ref_id, r.cell_id, d.pos_x, d.pos_y, d.pos_z"
            " from door_links d join \"references\" r on r.id = d.ref_id"
            " where d.destination_ref_id = ?",
            (door,),
        ).fetchone()
        if other is not None:
            print(
                f"    (other end: ref {other[0]:08X} in cell {other[1]}, its arrival "
                f"({other[2]:.0f},{other[3]:.0f},{other[4]:.0f}))"
            )


def main(argv):
    conn = connect()
    if len(argv) < 2:
        print(__doc__)
        return 2
    command = argv[1]
    if command == "cells":
        cells(conn, argv[2])
    elif command == "space":
        spec = argv[2]
        model = argv[3] if len(argv) > 3 else None
        if spec.startswith("world:"):
            refs(conn, None, model, world=int(spec.split(":", 1)[1]))
        else:
            refs(conn, int(spec, 16), model)
    elif command == "doors":
        doors(conn, int(argv[2], 16))
    elif command == "route":
        route(conn)
    elif command == "extents":
        spec = argv[2]
        if spec.startswith("world:"):
            extents(conn, None, world=int(spec.split(":", 1)[1]))
        else:
            extents(conn, int(spec, 16))
    elif command == "bases":
        # Every base used in a space, with its reference count and where those references sit.
        spec = argv[2]
        world = int(spec.split(":", 1)[1]) if spec.startswith("world:") else None
        column = "worldspace_id" if world is not None else "cell_id"
        key = world if world is not None else int(spec, 16)
        minimum = int(argv[3]) if len(argv) > 3 else 1
        query = (
            "select s.editor_id, s.model_path, count(*), min(r.pos_z), max(r.pos_z),"
            " avg(r.pos_x), avg(r.pos_y), avg(r.pos_z)"
            ' from "references" r left join statics s on s.id = r.base_form_id'
            f" where r.{column} = ? group by s.id having count(*) >= ? order by count(*) desc"
        )
        for row in conn.execute(query, (key, minimum)):
            print(
                f"  {row[2]:5d}  z {row[3]:8.0f}..{row[4]:8.0f} "
                f"at ({row[5]:8.0f},{row[6]:8.0f},{row[7]:7.0f})  "
                f"{(row[0] or '')[:34]:34.34} {row[1] or ''}"
            )
    elif command == "intocell":
        # Every load door that leads into a cell, one line each.
        cell_id = int(argv[2], 16)
        query = (
            "select r.id, r.cell_id, r.pos_x, r.pos_y, r.pos_z, s.model_path,"
            " d.destination_ref_id, c.interior_name, w.editor_id"
            ' from door_links d join "references" r on r.id = d.ref_id'
            " left join statics s on s.id = r.base_form_id"
            " left join cells c on c.id = d.destination_cell_id"
            " left join worldspaces w on w.id = d.destination_worldspace_id"
            " where d.destination_cell_id = ?"
        )
        for row in conn.execute(query, (cell_id,)):
            print(
                f"  door ref {row[0]:08X} in cell {row[1]:9d} at ({row[2]:.0f},{row[3]:.0f},"
                f"{row[4]:.0f}) -> {row[8] or row[7] or ''} {row[5]}"
            )
    elif command == "landinfo":
        # How many LAND rows a worldspace has and how big each heightmap blob is.
        world = int(argv[2])
        for row in conn.execute(
            "select c.grid_x, c.grid_y, length(l.heightmap), length(l.vtex), length(l.vclr)"
            " from land l join cells c on c.id = l.cell_id where c.worldspace_id = ?"
            " order by c.grid_y, c.grid_x limit 12",
            (world,),
        ):
            print(f"  grid ({row[0]},{row[1]}) heightmap {row[2]} bytes vtex {row[3]} vclr {row[4]}")
        total = conn.execute(
            "select count(*) from land l join cells c on c.id = l.cell_id where c.worldspace_id = ?",
            (world,),
        ).fetchone()[0]
        print(f"  {total} LAND rows in worldspace {world}")
    elif command == "worldcells":
        # Every cell of a worldspace, with its grid, name and reference count.
        world = int(argv[2])
        query = (
            "select c.id, c.grid_x, c.grid_y, c.interior_name,"
            " (select count(*) from \"references\" r where r.cell_id = c.id),"
            " (select min(r.pos_z) from \"references\" r where r.cell_id = c.id),"
            " (select max(r.pos_z) from \"references\" r where r.cell_id = c.id)"
            " from cells c where c.worldspace_id = ? order by c.grid_y, c.grid_x"
        )
        for row in conn.execute(query, (world,)):
            name = (row[3] or "").strip("\x00")
            print(
                f"  grid ({row[1]:3},{row[2]:3}) cell {row[0]:9d} 0x{row[0]:08X} "
                f"refs {row[4]:5d} z {row[5] if row[5] is not None else 0:8.0f}.."
                f"{row[6] if row[6] is not None else 0:8.0f} {name!r}"
            )
    elif command == "biggest":
        # The largest placed objects in a space, by model height times scale.
        spec = argv[2]
        world = int(spec.split(":", 1)[1]) if spec.startswith("world:") else None
        column = "worldspace_id" if world is not None else "cell_id"
        key = world if world is not None else int(spec, 16)
        limit = int(argv[3]) if len(argv) > 3 else 40
        query = (
            "select s.editor_id, s.model_path, r.pos_x, r.pos_y, r.pos_z,"
            " (s.bounds_max_y - s.bounds_min_y) * r.scale as height, r.scale,"
            " s.bounds_min_x, s.bounds_max_x"
            ' from "references" r join statics s on s.id = r.base_form_id'
            f" where r.{column} = ? and s.bounds_valid = 1 and s.bounds_max_y > 200"
            " order by height desc limit ?"
        )
        for row in conn.execute(query, (key, limit)):
            print(
                f"  h {row[5]:7.0f} (x {row[7]:6.0f}..{row[8]:6.0f}) at "
                f"({row[2]:8.0f},{row[3]:8.0f},{row[4]:8.0f}) scale {row[6]:.2f} "
                f"{(row[0] or '')[:30]:30.30} {row[1]}"
            )
    elif command == "ligh":
        # Placed LIGH references in a box, with their editor ids, radii and colours.
        x, y, z, radius = (float(v) for v in argv[2:6])
        needle = argv[6].lower() if len(argv) > 6 else None
        world = None
        for argument in argv:
            if argument.startswith("world="):
                world = int(argument.split("=", 1)[1])
        query = (
            "select l.editor_id, r.pos_x, r.pos_y, r.pos_z, l.radius, l.color_r, l.color_g,"
            " l.color_b, r.cell_id from \"references\" r join lights l on l.id = r.base_form_id"
            " where r.pos_x between ? and ? and r.pos_y between ? and ?"
            " and r.pos_z between ? and ?"
            + (" and r.worldspace_id = ?" if world is not None else "")
            + " order by l.radius desc"
        )
        parameters = (
            x - radius,
            x + radius,
            y - radius,
            y + radius,
            z - radius,
            z + radius,
        ) + ((world,) if world is not None else ())
        for row in conn.execute(query, parameters):
            text = (row[0] or "").lower()
            if needle and needle not in text:
                continue
            print(
                f"  ({row[1]:8.0f},{row[2]:8.0f},{row[3]:8.0f}) cell {row[8]:9d} "
                f"radius {row[4]:8.0f} rgb ({row[5]},{row[6]},{row[7]}) {row[0]}"
            )
    elif command == "bounds":
        # Local bounding boxes of base objects, so a piece's height above its origin is known.
        pattern = f"%{argv[2]}%"
        for row in conn.execute(
            "select id, editor_id, model_path, bounds_min_x, bounds_min_y, bounds_min_z,"
            " bounds_max_x, bounds_max_y, bounds_max_z, bounds_valid from statics"
            " where model_path like ? or editor_id like ? order by model_path",
            (pattern, pattern),
        ):
            print(
                f"  {row[0]:08X} valid={row[9]} x {row[3]:8.0f}..{row[6]:8.0f} "
                f"y {row[4]:8.0f}..{row[7]:8.0f} z {row[5]:8.0f}..{row[8]:8.0f}  "
                f"{row[1]!s:34.34} {row[2]}"
            )
    elif command == "box":
        # Distinct bases inside a Creation-space box, with counts and where they sit.
        x, y, z, radius = (float(v) for v in argv[2:6])
        minimum = int(argv[6]) if len(argv) > 6 else 1
        needle = argv[7].lower() if len(argv) > 7 else None
        query = (
            "select s.editor_id, s.model_path, count(*), min(r.pos_z), max(r.pos_z),"
            " avg(r.pos_x), avg(r.pos_y), avg(r.pos_z)"
            ' from "references" r left join statics s on s.id = r.base_form_id'
            " where r.pos_x between ? and ? and r.pos_y between ? and ?"
            " and r.pos_z between ? and ? group by s.id having count(*) >= ? order by count(*)"
        )
        parameters = (x - radius, x + radius, y - radius, y + radius, z - radius, z + radius, minimum)
        for row in conn.execute(query, parameters):
            text = (row[0] or "") + " " + (row[1] or "")
            if needle and needle not in text.lower():
                continue
            print(
                f"  {row[2]:5d}  z {row[3]:8.0f}..{row[4]:8.0f} "
                f"at ({row[5]:8.0f},{row[6]:8.0f},{row[7]:7.0f})  "
                f"{(row[0] or '')[:34]:34.34} {row[1] or ''}"
            )
    elif command == "models":
        cell_id = int(argv[2], 16)
        query = (
            "select s.editor_id, s.model_path, count(*) from \"references\" r"
            " join statics s on s.id = r.base_form_id where r.cell_id = ? and"
            " (s.model_path like ? or s.editor_id like ?) group by s.editor_id, s.model_path"
            " order by count(*) desc"
        )
        pattern = f"%{argv[3]}%"
        for row in conn.execute(query, (cell_id, pattern, pattern)):
            print(f"  {row[2]:6d}  {row[0]!s:40.40} {row[1]}")
    elif command == "base-near":
        cell_id = int(argv[2], 16)
        center = (float(argv[3]), float(argv[4]), float(argv[5]))
        radius = float(argv[6])
        query = (
            "select r.id, s.editor_id, s.model_path, r.pos_x, r.pos_y, r.pos_z"
            ' from "references" r left join statics s on s.id = r.base_form_id'
            " where r.cell_id = ? and r.pos_x between ? and ? and r.pos_y between ? and ?"
            " and r.pos_z between ? and ? order by (r.pos_x-?)*(r.pos_x-?)"
            "+(r.pos_y-?)*(r.pos_y-?)+(r.pos_z-?)*(r.pos_z-?)"
        )
        parameters = (
            cell_id,
            center[0] - radius,
            center[0] + radius,
            center[1] - radius,
            center[1] + radius,
            center[2] - radius,
            center[2] + radius,
            center[0],
            center[0],
            center[1],
            center[1],
            center[2],
            center[2],
        )
        for row in conn.execute(query, parameters):
            print(
                f"  {row[0]:08X} ({row[3]:9.0f},{row[4]:9.0f},{row[5]:9.0f}) "
                f"{row[1] or '':40.40} {row[2] or ''}"
            )
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
