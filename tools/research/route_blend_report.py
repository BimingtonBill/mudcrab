#!/usr/bin/env python3
"""How many objects on the Alftand -> Blackreach route are wrongly transparent.

For every cell the demo tour passes through this counts the placed references whose model
has a material the converter marked BLEND *without* an alpha property asking for it - the
`blend_from_vertex_alpha_flag` class from blend_cause_census.py - and lists the worst models.

Usage:
    python tools/research/route_blend_report.py
"""

import collections
import os
import sys

import blend_cause_census
import nif_alpha
import scene_materials
import world_db

# The cells the demo tour visits (see docs/design/blackreach-demo.md): the Tamriel grid
# around Alftand's exterior doors, the three interiors, and the Blackreach arrival cell.
TAILMERE_GRID = [(x, y) for x in range(17, 22) for y in range(16, 21)]
INTERIOR_CELLS = [
    0x000152C3,  # Alftand01
    0x00056C1B,  # Alftand02
    0x00069881,  # AlftandWorldStart
    0x0006987F,  # AlftandWorldOrigin
    0x0006987A,  # AlftandWorldEnd
    0x0001_0000,  # placeholder, dropped if unknown
]
WORLDSPACES = {60: "Tamriel", 126562: "Blackreach", 432215: "AlftandWorld"}


def classify_model(relative):
    path = os.path.join(scene_materials.MESH_ROOT, relative)
    if not os.path.isfile(path):
        return None
    import glb_materials  # noqa: PLC0415

    document = glb_materials.read_json_chunk(path)
    if document is None:
        return None
    nif = os.path.join(blend_cause_census.VFS_MESHES, relative)
    nif = nif[:-4] + ".nif" if nif.endswith(".glb") else nif
    header = None
    if os.path.isfile(nif):
        with open(nif, "rb") as handle:
            data = handle.read()
        for export_strings in (3, 4):
            try:
                candidate = nif_alpha.parse_header(data, export_strings)
            except Exception:  # noqa: BLE001
                continue
            if candidate["valid"]:
                header = candidate
                break
    return [blend_cause_census.classify(m, header) for m in document.get("materials", [])]


def main(_argv):
    conn = world_db.connect()
    cells = []
    for grid_x, grid_y in TAILMERE_GRID:
        for row in conn.execute(
            "select id from cells where worldspace_id=60 and grid_x=? and grid_y=?",
            (grid_x, grid_y),
        ):
            cells.append(row[0])
    for cell_id in INTERIOR_CELLS:
        if conn.execute("select 1 from cells where id=?", (cell_id,)).fetchone():
            cells.append(cell_id)
    for row in conn.execute(
        "select id, interior_name from cells where worldspace_id=126562 and "
        "interior_name like '%Alftand%'"
    ):
        cells.append(row[0])
    print(f"{len(cells)} cells on the route: {[f'{cell:08X}' for cell in cells]}")

    per_model_refs = collections.Counter()
    per_model_cell = collections.defaultdict(set)
    for cell_id in cells:
        for record in world_db.cell_rows(conn, cell_id):
            path = record["model_path"]
            if not path:
                continue
            per_model_refs[path] += 1
            per_model_cell[path].add(cell_id)

    causes = collections.Counter()
    refs_by_cause = collections.Counter()
    examples = collections.defaultdict(list)
    unknown = 0
    for model_path, count in per_model_refs.items():
        relative = scene_materials.glb_path(model_path)
        if not relative:
            unknown += count
            continue
        relative = os.path.relpath(relative, scene_materials.MESH_ROOT).replace("\\", "/")
        model_causes = classify_model(relative)
        if not model_causes:
            unknown += count
            continue
        flags = set()
        for cause in model_causes:
            causes[cause] += 1
            flags.add(cause)
            if len(examples[cause]) < 8 and count > 0:
                examples[cause].append(f"{count}x {model_path}")
        for cause in flags:
            refs_by_cause[cause] += count
    print(f"placed references with no converted model: {unknown}")
    print("references per cause (a model can feed several causes):")
    for cause, count in refs_by_cause.most_common():
        print(f"  {count:6d} refs  {cause}")
        print(f"           e.g. {examples[cause][:5]}")
    print("materials per cause:")
    for cause, count in causes.most_common():
        print(f"  {count:6d}  {cause}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
