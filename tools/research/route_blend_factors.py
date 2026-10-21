#!/usr/bin/env python3
"""Route models whose NiAlphaProperty asks for additive or multiplicative blending.

The converter keeps only "blend enabled" from the NiAlphaProperty flag word
(crates/converter/src/material.rs:986), so a material that Skyrim draws with
src=SRC_ALPHA / dst=ONE (additive) comes out as glTF BLEND, which the renderer treats as
ordinary src-alpha / inv-src-alpha transparency.

Usage:
    python tools/research/route_blend_factors.py
"""

import collections
import os
import sys

import glb_materials
import nif_alpha
import route_blend_report
import scene_materials
import world_db


def main(_argv):
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
        glb = scene_materials.glb_path(model_path)
        if not glb:
            continue
        relative = os.path.relpath(glb, scene_materials.MESH_ROOT).replace("\\", "/")
        document = glb_materials.read_json_chunk(glb)
        if document is None:
            continue
        nif = os.path.join(route_blend_report.blend_cause_census.VFS_MESHES, relative[:-4] + ".nif")
        if not os.path.isfile(nif):
            continue
        with open(nif, "rb") as handle:
            data = handle.read()
        header = None
        for export_strings in (3, 4):
            try:
                candidate = nif_alpha.parse_header(data, export_strings)
            except Exception:  # noqa: BLE001
                continue
            if candidate["valid"]:
                header = candidate
                break
        if header is None:
            continue
        blocks = header["blocks"]
        alpha_indices = {block["index"] for block in blocks if block["type"] == "NiAlphaProperty"}
        for material in document.get("materials", []):
            if material.get("alphaMode", "OPAQUE") != "BLEND":
                continue
            shape_index = material.get("extras", {}).get("openSkyrim", {}).get("shapeBlock")
            if not isinstance(shape_index, int) or shape_index >= len(blocks):
                continue
            hits = nif_alpha.shape_alpha_references(header, blocks[shape_index], alpha_indices)
            if not hits:
                continue
            _, value = hits[-1]
            flags, _threshold = nif_alpha.read_alpha_tail(blocks[value]["data"])
            if flags & 0x0001 == 0:
                continue
            source = (flags >> 1) & 0x0F
            destination = (flags >> 5) & 0x0F
            if source == 6 and destination == 7:
                continue
            rows.append(
                (
                    count,
                    model_path,
                    material.get("name"),
                    nif_alpha.ALPHA_SRC.get(source, source),
                    nif_alpha.ALPHA_SRC.get(destination, destination),
                )
            )
    rows.sort(key=lambda row: -row[0])
    print(f"{len(rows)} route materials blend with factors other than src=SRC_ALPHA/dst=INV_SRC_ALPHA")
    print(f"references drawn by them: {sum(row[0] for row in rows)}")
    for count, model_path, name, source, destination in rows[:40]:
        print(f"  {count:5d}x src={source:14s} dst={destination:14s} {model_path} {name!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
