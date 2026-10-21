#!/usr/bin/env python3
"""Cross the placed references of a cell with the alpha mode of the GLB each one loads.

This answers "which of the models in this room are drawn in the transparent pass", which is
what decides whether an object looks see-through on screen.

Usage:
    python tools/research/scene_materials.py <cell_form_id_hex> [refs_limit]
    python tools/research/scene_materials.py grid <world_hex> <grid_x> <grid_y> [radius]
"""

import os
import sys

import glb_materials
import world_db

MESH_ROOT = "C:/Modding/SkyrimConverted/meshes"


def glb_path(model_path):
    """The GLB a NIF model path was converted to, or None if it was not converted."""
    if not model_path:
        return None
    normalized = model_path.replace("\\", "/").lower()
    for prefix in ("meshes/", "data/meshes/"):
        if normalized.startswith(prefix):
            normalized = normalized[len(prefix) :]
    candidate = os.path.join(MESH_ROOT, normalized)
    if candidate.endswith(".nif"):
        candidate = candidate[:-4] + ".glb"
    return candidate if os.path.isfile(candidate) else None


def material_summary(path):
    document = glb_materials.read_json_chunk(path)
    if document is None:
        return None
    return [glb_materials.material_info(m) for m in document.get("materials", [])]


def report(conn, cells, references_limit=400):
    counts = {}
    modes = {}
    for cell_id in cells:
        for record in world_db.cell_rows(conn, cell_id):
            key = record["model_path"]
            counts[key] = counts.get(key, 0) + 1
            modes.setdefault(key, set()).add(cell_id)
    print(f"{len(counts)} distinct models placed in {len(cells)} cell(s)")
    rows = []
    for model_path, count in counts.items():
        path = glb_path(model_path)
        materials = material_summary(path) if path else None
        if materials is None:
            rows.append((model_path, count, None, None))
            continue
        alpha_modes = sorted({m["alphaMode"] for m in materials})
        blends = [m for m in materials if m["alphaMode"] == "BLEND"]
        rows.append((model_path, count, alpha_modes, blends))
    rows.sort(key=lambda row: (row[2] != ["BLEND"], -row[1]))
    for model_path, count, alpha_modes, blends in rows[:references_limit]:
        if alpha_modes is None:
            print(f"  {count:5d}x (no glb)      {model_path}")
            continue
        flag = "".join(mode[0] for mode in alpha_modes)
        detail = ""
        if blends:
            detail = "  " + ", ".join(
                f"{m['name']}@a={m['alpha']:.2f}"
                + (f" cutoff={m['alphaCutoff']:.2f}" if m["alphaCutoff"] is not None else "")
                for m in blends[:6]
            )
        print(f"  {count:5d}x [{flag:>4}] {model_path}{detail}")


def main(argv):
    conn = world_db.connect()
    if len(argv) < 2:
        print(__doc__)
        return 2
    if argv[1] == "grid":
        world = int(argv[2], 16)
        grid_x, grid_y = int(argv[3]), int(argv[4])
        radius = int(argv[5]) if len(argv) > 5 else 0
        cells = [
            row[0]
            for dx in range(-radius, radius + 1)
            for dy in range(-radius, radius + 1)
            for row in conn.execute(
                "select id from cells where worldspace_id=? and grid_x=? and grid_y=?",
                (world, grid_x + dx, grid_y + dy),
            )
        ]
    else:
        cells = [int(argv[1], 16)]
    if not cells:
        print("no cells matched")
        return 1
    for cell_id in cells:
        row = conn.execute(
            "select worldspace_id, grid_x, grid_y, interior_name from cells where id=?",
            (cell_id,),
        ).fetchone()
        print(f"=== cell {cell_id:08X} worldspace {row[0]} grid ({row[1]}, {row[2]}) {row[3]!r}")
    report(conn, cells, int(argv[2]) if len(argv) > 2 and argv[1] != "grid" else 400)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
