#!/usr/bin/env python3
"""lead-071 scratch: read the lighting-relevant rows of the converted world database.

Usage:
    python tools/research/lead071_db.py spaces <hex-id> [<hex-id> ...]
    python tools/research/lead071_db.py lights <hex-cell-id> [<hex-cell-id> ...]
    python tools/research/lead071_db.py schema
    python tools/research/lead071_db.py refs <form-id-hex>            # one reference's placement

Read-only.  The database is `C:/Modding/SkyrimConverted/skyrim_world.db` unless --db is given.
"""

import os
import sqlite3
import sys

DB = os.environ.get("OPENSKYRIM_DB", r"C:\Modding\SkyrimConverted\skyrim_world.db")


def connect(path):
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True)


def columns(connection, table):
    return [row[1] for row in connection.execute(f'PRAGMA table_info("{table}")')]


def spaces(connection, ids):
    for space_id in ids:
        row = connection.execute(
            "SELECT * FROM space_lighting WHERE space_id = ?", (space_id,)
        ).fetchone()
        if row is None:
            print(f"{space_id:#010x}: no row")
            continue
        names = [description[0] for description in connection.execute(
            "SELECT * FROM space_lighting LIMIT 1").description]
        fields = []
        for name, value in zip(names, row):
            if name.endswith(("ambient", "directional", "fog", "sun", "upper", "lower")):
                if isinstance(value, int):
                    value = "({},{},{})".format(
                        value & 0xFF, (value >> 8) & 0xFF, (value >> 16) & 0xFF
                    )
            fields.append(f"{name}={value}")
        print(f"{space_id:#010x}: " + " ".join(fields))


def lights(connection, cell_ids):
    print("lights columns:", columns(connection, "lights"))
    print("references columns:", columns(connection, "references"))
    for cell_id in cell_ids:
        rows = connection.execute(
            'SELECT r.base_form_id, r.pos_x, r.pos_y, r.pos_z, r.radius_override, '
            '       l.radius, l.color_r, l.color_g, l.color_b, l.flags, l.fade, l.editor_id '
            'FROM "references" r JOIN lights l ON l.id = r.base_form_id '
            "WHERE r.cell_id = ?",
            (cell_id,),
        ).fetchall()
        print(f"cell {cell_id:#010x}: {len(rows)} light references")
        for row in rows:
            base, x, y, z, override, radius, r, g, b, flags, fade, editor = row
            rgb = f"({r},{g},{b})"
            print(
                f"  base {base:#010x} at ({x:.0f},{y:.0f},{z:.0f}) record radius {radius} "
                f"override {override} colour {rgb} flags {flags:#x} fade {fade}"
            )


def schema(connection):
    for name, kind in connection.execute(
        "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') ORDER BY name"
    ):
        print(f"{kind:<6} {name}")
    try:
        print("publish report:", connection.execute("SELECT * FROM conversion_report").fetchall())
    except sqlite3.Error as error:
        print("no conversion_report:", error)


def xrds(connection, cell_ids, limit=6):
    """The raw record bytes of the light references in a cell, hex, to check XRDS's layout."""
    for cell_id in cell_ids:
        rows = connection.execute(
            'SELECT id, base_form_id, radius_override, data FROM "references" '
            "WHERE cell_id = ? AND radius_override IS NOT NULL LIMIT ?",
            (cell_id, limit),
        ).fetchall()
        print(f"cell {cell_id:#010x}:")
        for row in rows:
            reference_id, base, override, data = row
            raw = bytes(data) if data is not None else b""
            print(f"  ref id {reference_id} base {base:#010x} override {override}")
            print(f"    data[{len(raw)}] = {raw.hex(' ')}")


def main(argv):
    if not argv:
        print(__doc__)
        return 1
    path = DB
    if "--db" in argv:
        index = argv.index("--db")
        path = argv[index + 1]
        argv = argv[:index] + argv[index + 2 :]
    connection = connect(path)
    mode, rest = argv[0], argv[1:]
    if mode == "overrides":
        rows = connection.execute(
            'SELECT COUNT(*), SUM(radius_override < 0), SUM(radius_override > 0), '
            'MIN(radius_override), MAX(radius_override) FROM "references" '
            "WHERE radius_override IS NOT NULL"
        ).fetchone()
        print("all references: total, negative, positive, min, max =", rows)
        rows = connection.execute(
            'SELECT COUNT(*), SUM(r.radius_override < 0), SUM(r.radius_override > 0) '
            'FROM "references" r JOIN lights l ON l.id = r.base_form_id '
            "WHERE r.radius_override IS NOT NULL"
        ).fetchone()
        print("light references only: total, negative, positive =", rows)
        rows = connection.execute(
            'SELECT l.editor_id, COUNT(*) AS n, SUM(r.radius_override < 0) AS negative, '
            'AVG(ABS(r.radius_override)) FROM "references" r JOIN lights l ON l.id = r.base_form_id '
            "WHERE r.radius_override IS NOT NULL GROUP BY l.editor_id ORDER BY n DESC LIMIT 25"
        ).fetchall()
        for row in rows:
            print("  ", row)
    elif mode == "base":
        for a in rest:
            base = int(a, 16)
            row = connection.execute(
                "SELECT id, editor_id, radius, color_r, color_g, color_b, flags, falloff, fade "
                "FROM lights WHERE id = ?", (base,)
            ).fetchone()
            print(f"{base:#010x}: {row}")
    elif mode == "xrds":
        xrds(connection, [int(a, 16) for a in rest])
    elif mode == "spaces":
        spaces(connection, [int(a, 16) for a in rest])
    elif mode == "lights":
        lights(connection, [int(a, 16) for a in rest])
    elif mode == "schema":
        schema(connection)
    elif mode == "refs":
        for a in rest:
            form_id = int(a, 16)
            names = columns(connection, "references")
            row = connection.execute(
                f"SELECT {','.join(names)} FROM references WHERE form_id = ?", (form_id,)
            ).fetchone()
            print(dict(zip(names, row)) if row else f"{form_id:#x}: not in references")
    else:
        print(__doc__)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
