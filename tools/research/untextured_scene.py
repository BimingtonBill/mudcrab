#!/usr/bin/env python3
"""Models on the demo route whose material has no base colour texture.

A material without a base colour texture renders as a flat factor colour (white when the
factor is 1.0), which is what an untextured or failed-to-convert shape looks like on screen.

Usage:
    python tools/research/untextured_scene.py [min_refs]
"""

import collections
import os
import sys

import glb_materials
import route_blend_report
import scene_materials
import world_db


def main(argv):
    min_refs = int(argv[1]) if len(argv) > 1 else 1
    conn = world_db.connect()
    cells = []
    for grid_x, grid_y in route_blend_report.TAILMERE_GRID:
        for row in conn.execute(
            "select id from cells where worldspace_id=60 and grid_x=? and grid_y=?",
            (grid_x, grid_y),
        ):
            cells.append(row[0])
    for cell_id in route_blend_report.INTERIOR_CELLS:
        if conn.execute("select 1 from cells where id=?", (cell_id,)).fetchone():
            cells.append(cell_id)
    for row in conn.execute(
        "select id from cells where worldspace_id=126562 and interior_name like '%Alftand%'"
    ):
        cells.append(row[0])
    counts = collections.Counter()
    for cell_id in cells:
        for record in world_db.cell_rows(conn, cell_id):
            if record["model_path"]:
                counts[record["model_path"]] += 1
    rows = []
    for model_path, count in counts.items():
        if count < min_refs:
            continue
        glb = scene_materials.glb_path(model_path)
        if not glb:
            continue
        relative = os.path.relpath(glb, scene_materials.MESH_ROOT).replace("\\", "/")
        document = glb_materials.read_json_chunk(glb)
        if document is None:
            continue
        images = document.get("images", [])
        textures = document.get("textures", [])
        for material in document.get("materials", []):
            info = glb_materials.material_info(material)
            if info["hasBaseColorTexture"] or "non-rendering" in (info["name"] or ""):
                continue
            rows.append((count, relative, info["name"], info["alphaMode"], info["alpha"]))
    rows.sort(key=lambda row: -row[0])
    print(f"{len(rows)} untextured materials on the route; references shown below")
    for count, relative, name, mode, alpha in rows:
        print(f"  {count:5d}x {relative} {name!r} {mode} alpha={alpha:.2f}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
