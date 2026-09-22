#!/usr/bin/env python3
"""Query the converter's output database (read-only) to name what a screenshot shows.

Usage:
    python tools/research/world_db.py schema
    python tools/research/world_db.py cell <cell_form_id_hex> [radius]
    python tools/research/world_db.py near <cell_form_id_hex> <x> <y> <z> <radius>
    python tools/research/world_db.py model <substring> [limit]
    python tools/research/world_db.py base <base_form_id_hex>

Positions are creation-engine units (the same space as the engine's `tour.txt` camera
positions and the `reference.position` JSON in the database).
"""

import json
import sqlite3
import sys

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"


def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def schema(conn):
    for name, sql in conn.execute(
        "select name, sql from sqlite_master where type in ('table', 'view') order by name"
    ):
        print(f"== {name}")
        print((sql or "").replace("\n", " "))


def columns(conn, table):
    return [row[1] for row in conn.execute(f'pragma table_info("{table}")')]


REFERENCE_SELECT = (
    'select r.id, r.cell_id, r.base_form_id, s.model_path, s.editor_id, '
    "r.pos_x, r.pos_y, r.pos_z, r.rot_x, r.rot_y, r.rot_z, r.scale "
    'from "references" r left join statics s on s.id = r.base_form_id '
)


def cell_rows(conn, cell_id=None, center=None, radius=None):
    query = REFERENCE_SELECT + ("where r.cell_id = ?" if cell_id is not None else "")
    rows = []
    parameters = (cell_id,) if cell_id is not None else ()
    for row in conn.execute(query, parameters):
        record = {
            "form_id": row[0],
            "cell_id": row[1],
            "base_form_id": row[2],
            "model_path": row[3],
            "editor_id": row[4],
            "position": (row[5], row[6], row[7]),
            "rotation": (row[8], row[9], row[10]),
            "scale": row[11],
        }
        if center is not None:
            distance = sum((a - b) ** 2 for a, b in zip(record["position"], center)) ** 0.5
            record["_distance"] = distance
            if radius is not None and distance > radius:
                continue
        rows.append(record)
    if center is not None:
        rows.sort(key=lambda r: r.get("_distance", 0.0))
    return rows


def show(record):
    position = record["position"]
    formatted = "(" + ", ".join(f"{v:9.0f}" for v in position) + ")"
    print(
        f"  {record['form_id']:08X} cell {record['cell_id']:08X} base {record['base_form_id']:08X} "
        f"{formatted} scale {record['scale']:.3f} {record['editor_id'] or ''} "
        f"{record['model_path']}"
        + (f"  d={record['_distance']:.0f}" if "_distance" in record else "")
    )


def model_rows(conn, substring, limit):
    query = (
        "select s.model_path, count(*) from \"references\" r "
        "join statics s on s.id = r.base_form_id where s.model_path like ? "
        "group by s.model_path order by count(*) desc limit ?"
    )
    for path, count in conn.execute(query, (f"%{substring}%", limit)):
        print(f"  {count:7d} refs  {path}")


def base_rows(conn, base_id):
    for row in conn.execute(
        "select id, editor_id, model_path, flags from statics where id = ?", (base_id,)
    ):
        print(f"  base {row[0]:08X} {row[1]!r} flags {row[3]} model {row[2]}")
    for row in conn.execute(
        'select id, worldspace_id, grid_x, grid_y, interior_name from cells where id = ?',
        (base_id,),
    ):
        print(f"  cell {row[0]:08X} worldspace {row[1]} grid ({row[2]}, {row[3]}) {row[4]!r}")


def main(argv):
    conn = connect()
    if len(argv) < 2:
        print(__doc__)
        return 2
    command = argv[1]
    if command == "schema":
        schema(conn)
    elif command == "cell":
        cell_id = int(argv[2], 16)
        for row in conn.execute(
            "select id, worldspace_id, grid_x, grid_y, interior_name from cells where id=?",
            (cell_id,),
        ):
            print(f"cell {row[0]:08X} worldspace {row[1]} grid ({row[2]}, {row[3]}) {row[4]!r}")
        for record in cell_rows(conn, cell_id):
            show(record)
    elif command == "near":
        cell_id = int(argv[2], 16)
        center = (float(argv[3]), float(argv[4]), float(argv[5]))
        radius = float(argv[6]) if len(argv) > 6 else 512.0
        for record in cell_rows(conn, cell_id, center, radius):
            show(record)
    elif command == "grid":
        world = int(argv[2], 16)
        grid_x, grid_y = int(argv[3]), int(argv[4])
        for row in conn.execute(
            "select id from cells where worldspace_id=? and grid_x=? and grid_y=?",
            (world, grid_x, grid_y),
        ):
            print(f"cell {row[0]:08X}")
            cell_id = row[0]
            for record in cell_rows(conn, cell_id):
                show(record)
    elif command == "cells":
        for value in argv[2:]:
            cell_id = int(value, 16)
            row = conn.execute(
                "select worldspace_id, grid_x, grid_y, interior_name from cells where id=?",
                (cell_id,),
            ).fetchone()
            if row is None:
                print(f"  cell {cell_id:08X}: not in the database")
                continue
            count = conn.execute(
                'select count(*) from "references" where cell_id=?', (cell_id,)
            ).fetchone()[0]
            print(
                f"  cell {cell_id:08X}: worldspace {row[0]} grid ({row[1]}, {row[2]}) "
                f"name {row[3]!r} refs {count}"
            )
    elif command == "stats":
        # Where a cell's geometry actually sits, and which cells overlap that box.
        cell_id = int(argv[2], 16)
        row = conn.execute(
            "select count(*), min(pos_x), max(pos_x), min(pos_y), max(pos_y), "
            "min(pos_z), max(pos_z), avg(pos_x), avg(pos_y), avg(pos_z) "
            'from "references" where cell_id = ?',
            (cell_id,),
        ).fetchone()
        print(
            f"  cell {cell_id:08X}: {row[0]} refs, x {row[1]:.0f}..{row[2]:.0f}, "
            f"y {row[3]:.0f}..{row[4]:.0f}, z {row[5]:.0f}..{row[6]:.0f}, "
            f"centroid ({row[7]:.0f}, {row[8]:.0f}, {row[9]:.0f})"
        )
        if len(argv) > 5:
            center = (float(argv[3]), float(argv[4]), float(argv[5]))
            radius = float(argv[6]) if len(argv) > 6 else 1024.0
            nearest = conn.execute(
                REFERENCE_SELECT
                + "where r.cell_id = ? order by (r.pos_x-?)*(r.pos_x-?)+(r.pos_y-?)*(r.pos_y-?)"
                "+(r.pos_z-?)*(r.pos_z-?) limit 5",
                (
                    cell_id,
                    center[0],
                    center[0],
                    center[1],
                    center[1],
                    center[2],
                    center[2],
                ),
            )
            print(f"  nearest to {center} within {radius:.0f}:")
            for row in nearest:
                record = {
                    "form_id": row[0],
                    "cell_id": row[1],
                    "base_form_id": row[2],
                    "model_path": row[3],
                    "editor_id": row[4],
                    "position": (row[5], row[6], row[7]),
                    "scale": row[11],
                }
                record["_distance"] = sum(
                    (a - b) ** 2 for a, b in zip(record["position"], center)
                ) ** 0.5
                if record["_distance"] <= radius:
                    show(record)
            for row in conn.execute(
                "select r.cell_id, count(*) from \"references\" r "
                "where r.pos_x between ? and ? and r.pos_y between ? and ? "
                "and r.pos_z between ? and ? and r.cell_id <> ? group by r.cell_id "
                "order by count(*) desc limit 8",
                (
                    center[0] - radius,
                    center[0] + radius,
                    center[1] - radius,
                    center[1] + radius,
                    center[2] - radius,
                    center[2] + radius,
                    cell_id,
                ),
            ):
                print(f"    cell {row[0]:08X} has {row[1]} references inside that box")
    elif command == "doors":
        # Every load door in a cell, where it leads, and how far it is from a point.
        cell_id = int(argv[2], 16)
        center = (
            (float(argv[3]), float(argv[4]), float(argv[5])) if len(argv) > 5 else None
        )
        query = (
            "select r.id, d.destination_ref_id, d.destination_cell_id, d.destination_worldspace_id,"
            " r.pos_x, r.pos_y, r.pos_z, s.model_path,"
            " COALESCE(NULLIF(dc.interior_name,''),NULLIF(ws.editor_id,''),'') "
            'from door_links d join "references" r on r.id = d.ref_id'
            " left join statics s on s.id = r.base_form_id"
            " left join cells dc on dc.id = d.destination_cell_id"
            " left join worldspaces ws on ws.id = d.destination_worldspace_id"
            " where r.cell_id = ?"
        )
        for row in conn.execute(query, (cell_id,)):
            position = (row[4], row[5], row[6])
            distance = ""
            if center:
                distance = (
                    f" d={sum((a - b) ** 2 for a, b in zip(position, center)) ** 0.5:7.0f}"
                )
            print(
                f"  door {row[0]:08X} -> ref {row[1]:08X} cell {row[2] or 0:08X} "
                f"world {row[3]} {row[8]!r} at ({row[4]:.0f}, {row[5]:.0f}, {row[6]:.0f}) "
                f"{row[7]}{distance}"
            )
    elif command == "named":
        # Interior cells whose name matches a substring, with their reference counts.
        for row in conn.execute(
            "select id, worldspace_id, interior_name, (select count(*) from \"references\" r "
            "where r.cell_id = cells.id) from cells where interior_name like ? order by id",
            (f"%{argv[2]}%",),
        ):
            print(f"  cell {row[0]:08X} worldspace {row[1]} {row[2]!r} refs {row[3]}")
    elif command == "around":
        # Creation-space box search over every reference, whatever cell it is in.
        center = (float(argv[2]), float(argv[3]), float(argv[4]))
        radius = float(argv[5]) if len(argv) > 5 else 1024.0
        limit = int(argv[6]) if len(argv) > 6 else 200
        model_filter = None
        for argument in argv:
            if argument.startswith("model="):
                model_filter = argument.split("=", 1)[1].lower()
        # Sort by the true squared distance from the centre: every placeholder is a plain
        # '?' so SQLite binds them in the order they appear.
        query = (
            REFERENCE_SELECT
            + "where r.pos_x between ? and ? and r.pos_y between ? and ? "
            "and r.pos_z between ? and ? order by (r.pos_x-?)*(r.pos_x-?)"
            "+(r.pos_y-?)*(r.pos_y-?)+(r.pos_z-?)*(r.pos_z-?) limit ?"
        )
        rows = conn.execute(
            query,
            (
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
                limit,
            ),
        )
        shown = 0
        total = 0
        for row in rows:
            total += 1
            if model_filter and model_filter not in (row[3] or "").lower():
                continue
            record = {
                "form_id": row[0],
                "cell_id": row[1],
                "base_form_id": row[2],
                "model_path": row[3],
                "editor_id": row[4],
                "position": (row[5], row[6], row[7]),
                "rotation": (row[8], row[9], row[10]),
                "scale": row[11],
                "_distance": sum((a - b) ** 2 for a, b in zip((row[5], row[6], row[7]), center))
                ** 0.5,
            }
            show(record)
            shown += 1
        if model_filter:
            print(f"  ({shown} of {total} rows matched model={model_filter})")
    elif command == "arrival":
        # The XTEL arrival point a door writes, in creation units.
        for row in conn.execute(
            "select ref_id, destination_ref_id, destination_cell_id, destination_worldspace_id,"
            " pos_x, pos_y, pos_z, rot_x, rot_y, rot_z from door_links where ref_id = ?",
            (int(argv[2], 16),),
        ):
            print(
                f"  door {row[0]:08X} -> ref {row[1]:08X} cell {row[2] or 0:08X} "
                f"world {row[3]} arrival ({row[4]:.3f}, {row[5]:.3f}, {row[6]:.3f}) "
                f"rotation ({row[7]:.3f}, {row[8]:.3f}, {row[9]:.3f})"
            )
    elif command == "find":
        # Which cells hold this reference (a door, typically).
        for row in conn.execute(
            REFERENCE_SELECT + "where r.id = ?", (int(argv[2], 16),)
        ):
            show(
                {
                    "form_id": row[0],
                    "cell_id": row[1],
                    "base_form_id": row[2],
                    "model_path": row[3],
                    "editor_id": row[4],
                    "position": (row[5], row[6], row[7]),
                    "rotation": (row[8], row[9], row[10]),
                    "scale": row[11],
                }
            )
    elif command == "model":
        model_rows(conn, argv[2], int(argv[3]) if len(argv) > 3 else 40)
    elif command == "base":
        base_rows(conn, int(argv[2], 16))
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
