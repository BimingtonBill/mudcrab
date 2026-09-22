#!/usr/bin/env python3
"""The models behind the demo route's load doors, straight from the converted database.

research-027 needs to know whether the doors the Alftand demo actually walks through are
animated models or static ones. The door FormIDs are `demo_tour::ALFTAND_ROUTE` in
`crates/engine/src/demo_tour.rs`.
"""

from __future__ import annotations

import sqlite3
import sys

DATABASE = "C:/Modding/SkyrimConverted/skyrim_world.db"
ROUTE = [0x0001_5D48, 0x0009_2809, 0x0009_256A, 0x0006_998D]


def main() -> int:
    connection = sqlite3.connect(f"file:{DATABASE}?mode=ro", uri=True)
    tables = [
        row[0]
        for row in connection.execute(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
        )
    ]
    print("tables: " + ", ".join(tables))

    def columns(table: str) -> list[str]:
        return [row[1] for row in connection.execute(f'PRAGMA table_info("{table}")')]

    for table in ("references", "statics", "door_links"):
        if table in tables:
            print(f"{table}: {', '.join(columns(table))}")

    query = """
        SELECT r.id, r.base_form_id, s.editor_id, s.model_path,
               d.rot_x, d.rot_y, d.rot_z,
               l.destination_ref_id, l.rot_z, l.destination_cell_id, l.destination_worldspace_id
        FROM "references" r
        JOIN statics s ON s.id = r.base_form_id
        LEFT JOIN door_links d ON d.ref_id = r.id
        LEFT JOIN door_links l ON l.ref_id = d.destination_ref_id
        WHERE r.id IN ({placeholders})
        ORDER BY r.id
    """.format(placeholders=",".join("?" * len(ROUTE)))
    print(f"\nroute doors ({len(ROUTE)}):")
    for row in connection.execute(query, ROUTE):
        (ref_id, base, editor_id, model, rx, ry, rz, dest_ref, dest_yaw, dest_cell, dest_world) = row
        print(
            f"  {ref_id:08X} base={base:08X} {editor_id!r}\n"
            f"      model={model}\n"
            f"      door rot=({rx}, {ry}, {rz}) -> ref {dest_ref and format(dest_ref, '08X')} "
            f"yaw={dest_yaw} cell={dest_cell} world={dest_world}"
        )

    print("\nevery door base on the route, by model:")
    for row in connection.execute(
        """
        SELECT s.model_path, COUNT(*)
        FROM "references" r JOIN statics s ON s.id = r.base_form_id
        JOIN door_links l ON l.ref_id = r.id
        GROUP BY s.model_path ORDER BY COUNT(*) DESC LIMIT 15
        """
    ):
        print(f"  {row[1]:5d}  {row[0]}")
    connection.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
