#!/usr/bin/env python3
"""Which placed models are big and translucent around a camera position.

For every reference within a radius of a creation-space point this prints the model's
converted GLB size (POSITION accessor bounds times the reference scale), how far it is, and
the alpha modes of its materials. That is how the large see-through shapes in a screenshot
get a name.

Usage:
    python tools/research/scene_size_report.py <x> <y> <z> [radius] [limit]
    python tools/research/scene_size_report.py --cell <cell_form_id_hex> [limit]

Positions are creation-engine units, the space of the `references` table. The camera in
`local/demo/survey1/tour.txt` is in render space: add 4096 * (origin grid) to x and y for
the exterior, or use the value unchanged inside an interior cell.
"""

import os
import sys

import glb_materials
import scene_materials
import world_db


def glb_bounds(model_path):
    path = scene_materials.glb_path(model_path)
    if not path:
        return None
    document = glb_materials.read_json_chunk(path)
    if document is None:
        return None
    accessors = document.get("accessors", [])
    low = [float("inf")] * 3
    high = [float("-inf")] * 3
    for mesh in document.get("meshes", []):
        for primitive in mesh.get("primitives", []):
            index = primitive.get("attributes", {}).get("POSITION")
            if index is None or index >= len(accessors):
                continue
            accessor = accessors[index]
            minimum, maximum = accessor.get("min"), accessor.get("max")
            if not minimum or not maximum:
                continue
            for axis in range(3):
                low[axis] = min(low[axis], minimum[axis])
                high[axis] = max(high[axis], maximum[axis])
    if low[0] == float("inf"):
        return None
    return low, high


def rows(conn, center, radius, limit):
    for record in world_db.cell_rows(conn, None, center, radius):
        bounds = glb_bounds(record["model_path"])
        if bounds is None:
            continue
        low, high = bounds
        size = [high[axis] - low[axis] for axis in range(3)]
        scale = record["scale"] or 1.0
        footprint = max(size) * scale
        record["_bounds"] = bounds
        record["_size"] = [value * scale for value in size]
        record["_footprint"] = footprint
    # Second pass so the print order is by apparent size, not by distance.
    records = world_db.cell_rows(conn, None, center, radius)
    enriched = []
    for record in records:
        bounds = glb_bounds(record["model_path"])
        if bounds is None:
            continue
        low, high = bounds
        scale = record["scale"] or 1.0
        size = [(high[axis] - low[axis]) * scale for axis in range(3)]
        record["_size"] = size
        record["_footprint"] = max(size)
        record["_distance"] = sum((a - b) ** 2 for a, b in zip(record["position"], center)) ** 0.5
        enriched.append(record)
    enriched.sort(key=lambda r: -r["_footprint"])
    print(f"{len(enriched)} models with geometry within {radius:.0f} units; largest first")
    for record in enriched[:limit]:
        path = scene_materials.glb_path(record["model_path"])
        modes = ""
        if path:
            materials = scene_materials.material_summary(path)
            if materials:
                blends = [m for m in materials if m["alphaMode"] == "BLEND"]
                modes = "/".join(sorted({m["alphaMode"][0] for m in materials}))
                if blends:
                    modes += " blend:" + ",".join(
                        f"{m['name']}@a={m['alpha']:.2f}" for m in blends[:3]
                    )
        print(
            f"  size {record['_size'][0]:8.0f} {record['_size'][1]:8.0f} {record['_size'][2]:8.0f} "
            f"d={record['_distance']:7.0f} scale={record['scale']:.2f} [{modes}] "
            f"{record['model_path']}"
        )


def main(argv):
    conn = world_db.connect()
    if len(argv) < 2:
        print(__doc__)
        return 2
    if argv[1] == "--cell":
        cell_id = int(argv[2], 16)
        center = (0.0, 0.0, 0.0)
        records = world_db.cell_rows(conn, cell_id)
        if not records:
            print("no references")
            return 1
        # Interior: measure from the reference centroid, which is where the room is.
        count = len(records)
        center = tuple(sum(r["position"][axis] for r in records) / count for axis in range(3))
        print(f"centroid ({center[0]:.0f}, {center[1]:.0f}, {center[2]:.0f}), {count} references")
        rows(conn, center, 1.0e9, int(argv[3]) if len(argv) > 3 else 40)
        return 0
    center = (float(argv[1]), float(argv[2]), float(argv[3]))
    radius = float(argv[4]) if len(argv) > 4 else 2000.0
    limit = int(argv[5]) if len(argv) > 5 else 40
    rows(conn, center, radius, limit)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
