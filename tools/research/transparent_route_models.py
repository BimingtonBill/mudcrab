#!/usr/bin/env python3
"""The route's wrongly-BLEND models with the source texture alpha that makes them look thin.

A glTF BLEND material with `baseColorFactor` alpha 1.0 is only visible as transparent when
its base colour texture's own alpha is below 1, so a bug that only changes the render pass
looks harmless on models whose texture is opaque and obvious on models whose texture is not.

Usage:
    python tools/research/transparent_route_models.py [min_refs]
"""

import os
import sys

from PIL import Image

import blend_cause_census
import glb_materials
import route_blend_report
import scene_materials
import world_db

VFS_TEXTURES = "C:/Modding/SkyrimConverted/vfs/textures"
CACHE = {}


def texture_alpha(uri):
    """(min, mean, max, percent below 250) of the source DDS alpha channel."""
    if uri in CACHE:
        return CACHE[uri]
    relative = uri.replace("../../../", "")
    if relative.lower().startswith("textures/"):
        relative = relative[len("textures/") :]
    relative = relative.replace(".opensky-srgb.ktx2", ".dds").replace(".ktx2", ".dds")
    path = os.path.join(VFS_TEXTURES, relative)
    result = None
    if os.path.isfile(path):
        try:
            image = Image.open(path)
            if image.mode in ("RGBA", "LA", "PA"):
                values = list(image.getchannel("A").getdata())
                low = min(values)
                mean = sum(values) / len(values)
                below = sum(1 for value in values if value < 250) * 100.0 / len(values)
                result = (low, mean, max(values), below)
            else:
                result = "opaque"
        except Exception:  # noqa: BLE001
            result = None
    CACHE[uri] = result
    return result


def main(argv):
    min_refs = int(argv[1]) if len(argv) > 1 and argv[1].isdigit() else 1
    conn = world_db.connect()
    route = route_blend_report.main([])  # side effect only prints; re-derive the models here
    del route
    # Re-derive the model -> reference count table for the route cells.
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
    counts = {}
    for cell_id in cells:
        for record in world_db.cell_rows(conn, cell_id):
            if record["model_path"]:
                counts[record["model_path"]] = counts.get(record["model_path"], 0) + 1

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
        nif = os.path.join(blend_cause_census.VFS_MESHES, relative[:-4] + ".nif")
        header = None
        if os.path.isfile(nif):
            with open(nif, "rb") as handle:
                data = handle.read()
            for export_strings in (3, 4):
                try:
                    candidate = blend_cause_census.nif_alpha.parse_header(data, export_strings)
                except Exception:  # noqa: BLE001
                    continue
                if candidate["valid"]:
                    header = candidate
                    break
        images = document.get("images", [])
        for material in document.get("materials", []):
            cause = blend_cause_census.classify(material, header)
            if cause != "blend_from_vertex_alpha_flag":
                continue
            pbr = material.get("pbrMetallicRoughness", {})
            info = pbr.get("baseColorTexture")
            uri = ""
            if info and info.get("index") is not None and info["index"] < len(images):
                uri = images[info["index"]].get("uri", "")
            stats = texture_alpha(uri) if uri else None
            rows.append((count, model_path, material.get("name"), uri, stats))
    rows.sort(key=lambda row: -row[0])
    translucent = [row for row in rows if isinstance(row[4], tuple) and row[4][1] < 200.0]
    print(
        f"{len(rows)} materials in the blend_from_vertex_alpha_flag class on the route; "
        f"{len(translucent)} of them use a texture whose alpha averages below 200"
    )
    print(
        f"references drawn by a see-through one: {sum(row[0] for row in translucent)}"
    )
    print("worst (references, texture alpha mean):")
    for count, model_path, name, uri, stats in sorted(translucent, key=lambda row: -row[0])[:12]:
        print(f"  {count:5d}x {os.path.basename(model_path):45s} {name!r} "
              f"{os.path.basename(uri)} mean={stats[1]:.0f}")
    if "--summary" in sys.argv:
        return 0
    print("full list:")
    for count, model_path, name, uri, stats in rows:
        if isinstance(stats, tuple):
            detail = (
                f"alpha min={stats[0]} mean={stats[1]:.0f} max={stats[2]} "
                f"below250={stats[3]:.0f}%"
            )
        else:
            detail = f"texture alpha: {stats}"
        print(f"  {count:5d}x {model_path}\n         {name!r} texture={os.path.basename(uri)}: {detail}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
