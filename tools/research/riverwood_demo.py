#!/usr/bin/env python3
"""What a Riverwood demo would contain, read straight out of the converted data.

Read-only over C:/Modding/SkyrimConverted/skyrim_world.db (converter schema 17,
DB schema 5).  Coordinates are Creation units, the same space as the engine's
`DemoStart`, the shots file and `world_db.py`.

Usage:
    python tools/research/riverwood_demo.py survey
    python tools/research/riverwood_demo.py cell <cell_form_id_hex> [--refs]
    python tools/research/riverwood_demo.py doors <cell_form_id_hex>
    python tools/research/riverwood_demo.py around <x> <y> <radius>
    python tools/research/riverwood_demo.py route
    python tools/research/riverwood_demo.py gaps
    python tools/research/riverwood_demo.py validate
"""

import collections
import math
import sqlite3
import sys

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"
TAMRIEL = 0x3C
CELL_SIZE = 4096.0

# Interior names anywhere near the village.  The matching is by display name
# (FULL), so it is what a player sees in the load screen.
NAME_PATTERNS = (
    "%Riverwood%",
    "%Sleeping Giant%",
    "%Alvor%",
    "%Sigrid%",
    "%Faendal%",
    "%Gerdur%",
    "%Hod%",
    "%Trader%",
    "%Guardian Stone%",
    "%Bleak Falls%",
    "%Embershard%",
)


def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def hexid(value):
    return f"{value:08X}" if value else "-"


def refs_of(conn, cell_id):
    return conn.execute(
        'select count(*) from "references" where cell_id = ?', (cell_id,)
    ).fetchone()[0]


def cell_row(conn, cell_id):
    return conn.execute(
        "select id, worldspace_id, grid_x, grid_y, interior_name, flags from cells where id = ?",
        (cell_id,),
    ).fetchone()


def cell_label(conn, cell_id):
    row = cell_row(conn, cell_id)
    if row is None:
        return hexid(cell_id)
    if row[4]:
        return f"{hexid(row[0])} {row[4]!r}"
    return f"{hexid(row[0])} grid({row[2]},{row[3]})"


def doors_leaving(conn, cell_id):
    """Load doors placed inside `cell_id` and where they lead."""
    return conn.execute(
        "select d.ref_id, d.destination_ref_id, d.destination_cell_id, "
        "d.destination_worldspace_id, d.pos_x, d.pos_y, d.pos_z, "
        "d.rot_x, d.rot_y, d.rot_z, r.pos_x, r.pos_y, r.pos_z, r.scale, "
        "s.model_path, s.editor_id "
        'from door_links d join "references" r on r.id = d.ref_id '
        "left join statics s on s.id = r.base_form_id "
        "where r.cell_id = ?",
        (cell_id,),
    ).fetchall()


def doors_arriving(conn, cell_id):
    """Load doors anywhere whose XTEL points into `cell_id`."""
    return conn.execute(
        "select d.ref_id, r.cell_id, r.pos_x, r.pos_y, r.pos_z, s.model_path "
        "from door_links d join \"references\" r on r.id = d.ref_id "
        "left join statics s on s.id = r.base_form_id "
        "where d.destination_cell_id = ?",
        (cell_id,),
    ).fetchall()


def model_census(conn, cell_ids):
    """Reference counts per model path across a set of cells."""
    marks = ",".join("?" * len(cell_ids))
    return conn.execute(
        "select coalesce(s.model_path, '<no model>'), s.editor_id, count(*) "
        'from "references" r left join statics s on s.id = r.base_form_id '
        f"where r.cell_id in ({marks}) group by s.model_path, s.editor_id "
        "order by count(*) desc",
        tuple(cell_ids),
    ).fetchall()


def classify(path):
    """Which family of asset a model path belongs to, for counting."""
    low = (path or "").lower()
    if not low:
        return "no model (marker, or no statics row)"
    if "grass" in low:
        return "grass"
    if low.startswith("trees\\") or "\\trees\\" in low:
        return "tree"
    if "water" in low:
        return "water plane"
    if low.startswith("landscape\\"):
        return "landscape prop"
    if low.startswith("architecture\\") or low.startswith("clutter\\"):
        return "building / clutter"
    if low.startswith("dungeons\\") or low.startswith("caves\\"):
        return "dungeon"
    if low.startswith("actors\\") or low.startswith("character\\"):
        return "actor"
    if "marker" in low:
        return "marker"
    return "other"


def kind_census(conn, cell_ids):
    """The engine's spawn kind per reference: what the model path says it is."""
    marks = ",".join("?" * len(cell_ids))
    rows = conn.execute(
        'select coalesce(s.model_path, ""), count(*) from "references" r '
        "left join statics s on s.id = r.base_form_id "
        f"where r.cell_id in ({marks}) group by s.model_path",
        tuple(cell_ids),
    ).fetchall()
    buckets = collections.Counter()
    for path, count in rows:
        buckets[classify(path)] += count
    return buckets


# ---------------------------------------------------------------------------


def survey(conn):
    patterns = NAME_PATTERNS
    where = " or ".join("interior_name like ?" for _ in patterns)
    rows = conn.execute(
        f"select id, worldspace_id, interior_name from cells where {where} order by id",
        patterns,
    ).fetchall()
    print(f"== interiors matching {len(patterns)} name patterns: {len(rows)}")
    interiors = []
    for cell_id, world_id, name in rows:
        count = refs_of(conn, cell_id)
        print(f"  {hexid(cell_id)} world {world_id} {name!r} refs {count}")
        interiors.append(cell_id)

    print("\n== doors that arrive in those interiors, and where they are placed")
    exterior_doors = {}
    for cell_id in interiors:
        arrivals = doors_arriving(conn, cell_id)
        if not arrivals:
            print(f"  {cell_label(conn, cell_id)}: no door arrives here")
            continue
        for ref_id, source_cell, x, y, z, model in arrivals:
            source = cell_row(conn, source_cell)
            grid = f"grid({source[2]},{source[3]})" if source and source[2] is not None else "interior"
            print(
                f"  {cell_label(conn, cell_id)}  <- door {hexid(ref_id)} in cell "
                f"{hexid(source_cell)} {grid} at ({x:.0f}, {y:.0f}, {z:.0f})  {model}"
            )
            exterior_doors.setdefault(cell_id, []).append((ref_id, source_cell, x, y, z))

    print("\n== the exterior cells those doors stand in")
    grid_cells = collections.Counter()
    xs, ys, zs = [], [], []
    for cell_id, doors in exterior_doors.items():
        for ref_id, source_cell, x, y, z in doors:
            grid_cells[(source_cell, x, y, z)] = 1
            xs.append(x)
            ys.append(y)
            zs.append(z)
    for (source_cell, x, y, z) in sorted(grid_cells, key=lambda k: k[1]):
        row = cell_row(conn, source_cell)
        print(
            f"  cell {hexid(source_cell)} world {row[1]} grid ({row[2]}, {row[3]}) "
            f"refs {refs_of(conn, source_cell)}  door at ({x:.0f}, {y:.0f}, {z:.0f})"
        )
    if xs:
        cx, cy, cz = sum(xs) / len(xs), sum(ys) / len(ys), sum(zs) / len(zs)
        print(
            f"\n  door centroid ({cx:.0f}, {cy:.0f}, {cz:.0f}); "
            f"x {min(xs):.0f}..{max(xs):.0f} y {min(ys):.0f}..{max(ys):.0f} "
            f"z {min(zs):.0f}..{max(zs):.0f}"
        )


def cell_report(conn, cell_id, dump_refs):
    row = cell_row(conn, cell_id)
    if row is None:
        print(f"cell {hexid(cell_id)} is not in the database")
        return
    grid = f"grid ({row[2]}, {row[3]})" if row[2] is not None else "interior"
    print(
        f"cell {hexid(row[0])} world {row[1]} {grid} name {row[4]!r} flags {row[5]} "
        f"refs {refs_of(conn, cell_id)}"
    )
    print("\n-- kinds")
    for kind, count in kind_census(conn, [cell_id]).most_common():
        print(f"  {count:6d}  {kind}")
    print("\n-- models")
    for path, editor, count in model_census(conn, [cell_id])[:40]:
        print(f"  {count:6d}  {path}  {editor or ''}")
    print("\n-- doors leaving")
    for row2 in doors_leaving(conn, cell_id):
        (
            ref_id,
            dest_ref,
            dest_cell,
            dest_world,
            ax,
            ay,
            az,
            rx,
            ry,
            rz,
            px,
            py,
            pz,
            scale,
            model,
            editor,
        ) = row2
        print(
            f"  door {hexid(ref_id)} placed ({px:.0f}, {py:.0f}, {pz:.0f}) scale {scale:.2f} "
            f"{editor or ''} {model}\n"
            f"      -> {hexid(dest_ref)} cell {cell_label(conn, dest_cell)} "
            f"world {dest_world} arrival ({ax:.1f}, {ay:.1f}, {az:.1f}) "
            f"rot ({rx:.3f}, {ry:.3f}, {rz:.3f})"
        )
    if dump_refs:
        print("\n-- references")
        for row2 in conn.execute(
            'select r.id, r.base_form_id, s.editor_id, s.model_path, r.pos_x, r.pos_y, '
            "r.pos_z, r.rot_z, r.scale from \"references\" r "
            "left join statics s on s.id = r.base_form_id where r.cell_id = ? "
            "order by r.pos_x, r.pos_y",
            (cell_id,),
        ):
            print(
                f"  {hexid(row2[0])} base {hexid(row2[1])} ({row2[4]:9.0f}, {row2[5]:9.0f}, "
                f"{row2[6]:8.0f}) rz {row2[7]:.3f} s {row2[8]:.2f} {row2[2] or ''} {row2[3]}"
            )


def doors_report(conn, cell_id):
    print(f"== doors placed in {cell_label(conn, cell_id)}")
    for row in doors_leaving(conn, cell_id):
        (ref_id, dest_ref, dest_cell, dest_world, ax, ay, az, rx, ry, rz, px, py, pz,
         scale, model, editor) = row
        print(
            f"  {hexid(ref_id)} at ({px:.0f}, {py:.0f}, {pz:.0f}) -> "
            f"{cell_label(conn, dest_cell)} world {dest_world} ref {hexid(dest_ref)} "
            f"arrival ({ax:.1f}, {ay:.1f}, {az:.1f}) rot_z {rz:.4f} scale {scale:.2f} {model}"
        )
    print(f"== doors that arrive in it")
    for ref_id, source_cell, x, y, z, model in doors_arriving(conn, cell_id):
        print(
            f"  in-door {hexid(ref_id)} cell {cell_label(conn, source_cell)} "
            f"at ({x:.0f}, {y:.0f}, {z:.0f}) {model}"
        )


def box(conn, x0, y0, x1, y1):
    """Which cells hold references inside a Creation-space box, and the grid convention."""
    rows = conn.execute(
        "select r.cell_id, c.worldspace_id, c.grid_x, c.grid_y, c.interior_name, count(*), "
        "min(r.pos_x), max(r.pos_x), min(r.pos_y), max(r.pos_y), "
        "min(r.local_x), max(r.local_x), min(r.local_y), max(r.local_y), r.is_exterior "
        'from "references" r left join cells c on c.id = r.cell_id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ? "
        "group by r.cell_id order by count(*) desc limit 40",
        (x0, x1, y0, y1),
    ).fetchall()
    for row in rows:
        print(
            f"  cell {hexid(row[0])} world {row[1]} grid ({row[2]}, {row[3]}) "
            f"{row[4]!r} is_exterior {row[14]} refs {row[5]}\n"
            f"      x {row[6]:.0f}..{row[7]:.0f} y {row[8]:.0f}..{row[9]:.0f} "
            f"local_x {row[10]:.0f}..{row[11]:.0f} local_y {row[12]:.0f}..{row[13]:.0f}"
        )
    total = conn.execute(
        'select count(*) from "references" where pos_x between ? and ? and pos_y between ? and ?',
        (x0, x1, y0, y1),
    ).fetchone()[0]
    print(f"  ({total} references in the box in total)")
    print("\n  Tamriel cell grid dump (id, grid, refs):")
    for row in conn.execute(
        "select c.id, c.grid_x, c.grid_y, count(r.id) from cells c "
        'left join "references" r on r.cell_id = c.id where c.worldspace_id = 60 '
        "group by c.id order by c.grid_x, c.grid_y limit 200"
    ):
        print(f"    {hexid(row[0])} grid ({row[1]}, {row[2]}) refs {row[3]}")


def door_places(conn, worldspace):
    """Which cell holds each exterior load door of a worldspace (persistent cell?)."""
    rows = conn.execute(
        "select r.cell_id, c.grid_x, c.grid_y, count(*) from door_links d "
        'join "references" r on r.id = d.ref_id left join cells c on c.id = r.cell_id '
        "where r.worldspace_id = ? group by r.cell_id order by count(*) desc",
        (worldspace,),
    ).fetchall()
    for row in rows:
        print(f"  cell {hexid(row[0])} grid ({row[1]}, {row[2]}) exterior doors {row[3]}")
    print("  doors whose placement cell grid does not match their own position:")
    for row in conn.execute(
        "select d.ref_id, r.cell_id, c.grid_x, c.grid_y, r.pos_x, r.pos_y, r.local_x, r.local_y "
        "from door_links d join \"references\" r on r.id = d.ref_id "
        "left join cells c on c.id = r.cell_id where r.worldspace_id = ? "
        "order by r.pos_x limit 12",
        (worldspace,),
    ):
        print(
            f"    door {hexid(row[0])} cell {hexid(row[1])} grid ({row[2]}, {row[3]}) "
            f"pos ({row[4]:.0f}, {row[5]:.0f}) local ({row[6]:.0f}, {row[7]:.0f})"
        )


def keyword_census(conn, box, keywords):
    """Which models in a Creation-space box match each keyword, and how many refs each has."""
    for keyword in keywords:
        rows = conn.execute(
            "select s.model_path, count(*) from \"references\" r "
            "left join statics s on s.id = r.base_form_id "
            "where r.pos_x between ? and ? and r.pos_y between ? and ? "
            "and lower(coalesce(s.model_path,'')) like ? "
            "group by s.model_path order by count(*) desc limit 12",
            (box[0], box[2], box[1], box[3], f"%{keyword.lower()}%"),
        ).fetchall()
        total = conn.execute(
            "select count(*) from \"references\" r left join statics s on s.id = r.base_form_id "
            "where r.pos_x between ? and ? and r.pos_y between ? and ? "
            "and lower(coalesce(s.model_path,'')) like ?",
            (box[0], box[2], box[1], box[3], f"%{keyword.lower()}%"),
        ).fetchone()[0]
        print(f"\n-- keyword {keyword!r}: {total} references, {len(rows)} distinct models")
        for path, count in rows:
            print(f"  {count:6d}  {path}")


def gaps(conn, box):
    """Every count the brief's question 4 and question 5 need, for one Creation-space box."""
    refs = conn.execute(
        "select count(*) from \"references\" where pos_x between ? and ? and pos_y between ? and ?",
        (box[0], box[2], box[1], box[3]),
    ).fetchone()[0]
    lights = conn.execute(
        'select count(*) from "references" r join lights l on l.id = r.base_form_id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ?",
        (box[0], box[2], box[1], box[3]),
    ).fetchone()[0]
    doors = conn.execute(
        'select count(*) from "references" r join door_links d on d.ref_id = r.id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ?",
        (box[0], box[2], box[1], box[3]),
    ).fetchone()[0]
    npc_refs = conn.execute(
        'select count(*) from "references" r join npcs n on n.id = r.base_form_id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ?",
        (box[0], box[2], box[1], box[3]),
    ).fetchone()[0]
    no_model = conn.execute(
        'select count(*) from "references" r left join statics s on s.id = r.base_form_id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ? and s.model_path is null",
        (box[0], box[2], box[1], box[3]),
    ).fetchone()[0]
    empty_model = conn.execute(
        'select count(*) from "references" r join statics s on s.id = r.base_form_id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ? and s.model_path = ''",
        (box[0], box[2], box[1], box[3]),
    ).fetchone()[0]
    invalid = conn.execute(
        'select count(*) from "references" r join statics s on s.id = r.base_form_id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ? and s.bounds_valid = 0 "
        "and s.model_path <> ''",
        (box[0], box[2], box[1], box[3]),
    ).fetchone()[0]
    snow = conn.execute(
        'select count(*) from "references" r join statics s on s.id = r.base_form_id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ? "
        "and s.material_object is not null",
        (box[0], box[2], box[1], box[3]),
    ).fetchone()[0]
    print(
        f"box x {box[0]:.0f}..{box[2]:.0f} y {box[1]:.0f}..{box[3]:.0f}: "
        f"{refs} references, {lights} light refs, {doors} load doors, {npc_refs} NPC refs, "
        f"{no_model} with no statics row, {empty_model} with an empty model, "
        f"{invalid} with bounds_valid=0, {snow} with a material_object (snow candidate)"
    )
    print("\n-- kinds")
    rows = conn.execute(
        "select coalesce(s.model_path,''), count(*) from \"references\" r "
        "left join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? group by s.model_path",
        (box[0], box[2], box[1], box[3]),
    ).fetchall()
    buckets = collections.Counter()
    for path, count in rows:
        buckets[classify(path)] += count
    for kind, count in buckets.most_common():
        print(f"  {count:6d}  {kind}")
    print("\n-- named statics (landmark candidates)")
    for row in conn.execute(
        "select s.editor_id, s.model_path, count(*) from \"references\" r "
        "left join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? "
        "and s.editor_id is not null group by s.editor_id order by count(*) desc limit 60",
        (box[0], box[2], box[1], box[3]),
    ):
        print(f"  {row[2]:6d}  {row[0]}  {row[1]}")
    print("\n-- cells the box covers")
    for row in conn.execute(
        "select c.id, c.grid_x, c.grid_y, c.interior_name, count(r.id) from cells c "
        'join "references" r on r.cell_id = c.id '
        "where c.worldspace_id = 60 and r.pos_x between ? and ? and r.pos_y between ? and ? "
        "group by c.id order by c.grid_x, c.grid_y",
        (box[0], box[2], box[1], box[3]),
    ):
        print(f"  {hexid(row[0])} grid ({row[1]}, {row[2]}) {row[3]!r} refs {row[4]}")
    print("\n-- land / lod coverage of those grids")
    for row in conn.execute(
        "select c.grid_x, c.grid_y, "
        "(select count(*) from land l where l.cell_id = c.id), "
        "(select count(*) from lod d where d.cell_id = c.id) "
        "from cells c where c.worldspace_id = 60 and c.grid_x between ? and ? "
        "and c.grid_y between ? and ? order by c.grid_x, c.grid_y",
        (
            int(box[0] // 4096),
            int(box[2] // 4096),
            int(box[1] // 4096),
            int(box[3] // 4096),
        ),
    ):
        print(f"  grid ({row[0]}, {row[1]}) land rows {row[2]} lod rows {row[3]}")


def models_near(conn, substring, x, y, radius):
    """Instances of a model whose path matches, near a point, with their cell and distance."""
    rows = conn.execute(
        "select r.id, r.cell_id, s.editor_id, s.model_path, r.pos_x, r.pos_y, r.pos_z, r.rot_z, "
        "r.scale from \"references\" r join statics s on s.id = r.base_form_id "
        "where (lower(s.model_path) like ? or lower(coalesce(s.editor_id,'')) like ?) "
        "and abs(r.pos_x - ?) < ? and abs(r.pos_y - ?) < ? "
        "order by (r.pos_x-?)*(r.pos_x-?)+(r.pos_y-?)*(r.pos_y-?) limit 30",
        (
            f"%{substring.lower()}%",
            f"%{substring.lower()}%",
            x,
            radius,
            y,
            radius,
            x,
            x,
            y,
            y,
        ),
    ).fetchall()
    total = conn.execute(
        "select count(*) from \"references\" r join statics s on s.id = r.base_form_id "
        "where lower(s.model_path) like ? or lower(coalesce(s.editor_id,'')) like ?",
        (f"%{substring.lower()}%", f"%{substring.lower()}%"),
    ).fetchone()[0]
    print(f"\n-- model ~ {substring!r}: {total} references in Tamriel, {len(rows)} within {radius:.0f}")
    for row in rows:
        distance = ((row[4] - x) ** 2 + (row[5] - y) ** 2) ** 0.5
        print(
            f"  {hexid(row[0])} cell {hexid(row[1])} ({row[4]:8.0f}, {row[5]:8.0f}, "
            f"{row[6]:8.0f}) rz {row[7]:.3f} s {row[8]:.2f} d {distance:7.0f} "
            f"{row[2] or ''} {row[3]}"
        )


def ascii_map(conn, box, step):
    """A coarse top-down map of a Creation-space box: D door, W water, B building,
    T tree, ~ fence/clutter, . everything else, space nothing."""
    x0, y0, x1, y1 = box
    columns = int((x1 - x0) / step) + 1
    rows = int((y1 - y0) / step) + 1
    grid = [[" "] * columns for _ in range(rows)]
    for row in conn.execute(
        "select r.pos_x, r.pos_y, coalesce(s.model_path,''), "
        "case when d.ref_id is not null then 1 else 0 end, r.id "
        'from "references" r left join statics s on s.id = r.base_form_id '
        "left join door_links d on d.ref_id = r.id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ?",
        (x0, x1, y0, y1),
    ):
        column = int((row[0] - x0) / step)
        line = int((row[1] - y0) / step)
        if not (0 <= column < columns and 0 <= line < rows):
            continue
        path = row[2].lower()
        if row[3]:
            mark = "D"
        elif "water" in path:
            mark = "W"
        elif "road" in path or "path" in path:
            mark = "="
        elif path.startswith("architecture\\farmhouse") or "lumbermill" in path:
            mark = "B"
        elif path.startswith("landscape\\trees") or path.startswith("trees\\"):
            mark = "T"
        elif "fence" in path or "stonewall" in path or "walkway" in path:
            mark = "~"
        elif path.startswith("landscape\\plants") or path.startswith("plants\\"):
            mark = ","
        elif not path:
            mark = "o"
        else:
            mark = "."
        # Doors, water and buildings win over scenery so the landmarks stay visible.
        priority = {" ": 0, ",": 1, "o": 2, ".": 2, "=": 3, "T": 3, "~": 3, "B": 4, "W": 5, "D": 6}
        if priority[mark] >= priority[grid[line][column]]:
            grid[line][column] = mark
    # Printed north up: Creation +Y is north, and it is the top of the map.
    print(
        f"map x {x0:.0f}..{x1:.0f} (west->east), y {y0:.0f}..{y1:.0f} (south->north, "
        f"printed north up), {step:.0f} units per character"
    )
    print("         " + "".join(str(int((x0 + i * step) // 1000) % 10) for i in range(columns)))
    print("         (thousands digit of x at the left edge of each column)")
    for index in range(rows - 1, -1, -1):
        y = int(y0 + index * step)
        print(f"{y // 1000:5d}k   " + "".join(grid[index]))
    print("legend: D load door, W water, B farmhouse/lumbermill, T tree, ~ fence/wall, "
          ", plant, . clutter/rock, o no model, blank nothing")


def invalid_models(conn, box):
    """Model paths whose converted glb failed bounds validation, inside a box."""
    print("-- references with bounds_valid = 0 (the engine's hard gate)")
    for row in conn.execute(
        "select s.editor_id, s.model_path, count(*), min(s.bounds_min_x) from \"references\" r "
        "join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? and s.bounds_valid = 0 "
        "group by s.model_path order by count(*) desc",
        (box[0], box[2], box[1], box[3]),
    ):
        print(f"  {row[2]:6d}  {row[0]}  {row[1]}")
    print("\n-- references whose base has no statics row at all")
    for row in conn.execute(
        'select r.base_form_id, count(*) from "references" r '
        "left join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? and s.id is null "
        "group by r.base_form_id order by count(*) desc limit 25",
        (box[0], box[2], box[1], box[3]),
    ):
        print(f"  {hexid(row[0])} x{row[1]}")
    print("\n-- what those base records are (by record type)")
    for row in conn.execute(
        "select w.record_type, count(*) from records w where w.form_id in "
        "(select r.base_form_id from \"references\" r left join statics s on s.id = r.base_form_id "
        " where r.pos_x between ? and ? and r.pos_y between ? and ? and s.id is null) "
        "group by w.record_type order by count(*) desc",
        (box[0], box[2], box[1], box[3]),
    ):
        print(f"  {row[1]:6d}  {row[0]}")


def interiors_reachable(conn, centre, radius):
    """Every interior a load door in the exterior box leads into, and its door pair."""
    x, y = centre
    rows = conn.execute(
        "select distinct d.destination_cell_id from door_links d "
        'join "references" r on r.id = d.ref_id '
        "where r.worldspace_id = 60 and abs(r.pos_x - ?) < ? and abs(r.pos_y - ?) < ? "
        "and d.destination_cell_id is not null",
        (x, radius, y, radius),
    ).fetchall()
    cell_ids = sorted(row[0] for row in rows)
    print(f"== {len(cell_ids)} interiors reachable from within {radius:.0f} units of "
          f"({x:.0f}, {y:.0f})")
    for cell_id in cell_ids:
        row = cell_row(conn, cell_id)
        name = (row[4] or "").rstrip("\x00") if row else "?"
        count = refs_of(conn, cell_id)
        lights = conn.execute(
            'select count(*) from "references" r join lights l on l.id = r.base_form_id '
            "where r.cell_id = ?",
            (cell_id,),
        ).fetchone()[0]
        door_count = conn.execute(
            "select count(*) from door_links d join \"references\" r on r.id = d.ref_id "
            "where r.cell_id = ?",
            (cell_id,),
        ).fetchone()[0]
        print(
            f"\n  cell {hexid(cell_id)} {name!r}: {count} references, {lights} light refs, "
            f"{door_count} doors inside"
        )
        for door in doors_arriving(conn, cell_id):
            ref_id, source_cell, dx, dy, dz, model = door
            print(
                f"    outside door {hexid(ref_id)} in {cell_label(conn, source_cell)} "
                f"at ({dx:.1f}, {dy:.1f}, {dz:.1f})  {model}"
            )
        for door in doors_leaving(conn, cell_id):
            (ref_id, dest_ref, dest_cell, dest_world, ax, ay, az, rx, ry, rz, px, py, pz,
             scale, model, editor) = door
            print(
                f"    inside  door {hexid(ref_id)} at ({px:.1f}, {py:.1f}, {pz:.1f}) "
                f"scale {scale:.2f} -> {cell_label(conn, dest_cell)} world {dest_world}\n"
                f"        arrival in Tamriel ({ax:.2f}, {ay:.2f}, {az:.2f}) "
                f"rot_z {rz:.4f}  ref {hexid(dest_ref)}"
            )


PLUGIN = "C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/Skyrim.esm"
SIDE = 33
SAMPLE = 4096.0 / (SIDE - 1)


def land_cache():
    """LAND height samples per grid, read straight from the plugin (read-only)."""
    import esm_route
    import land_edges
    import land_vtxt

    plugin = esm_route.Plugin(PLUGIN)
    cache = {}

    def height_at(world, x, y):
        grid = (math.floor(x / 4096.0), math.floor(y / 4096.0))
        if grid not in cache:
            payloads = land_vtxt.land_records(plugin, world, grid[0], grid[1])
            cache[grid] = (
                land_edges.decode_vhgt(land_edges.fields(payloads[0]).get(b"VHGT", b""))
                if payloads
                else None
            )
        heights = cache[grid]
        if heights is None:
            return None
        column = min(max((x - grid[0] * 4096.0) / SAMPLE, 0.0), SIDE - 1.0)
        row = min(max((y - grid[1] * 4096.0) / SAMPLE, 0.0), SIDE - 1.0)
        c0, r0 = int(column), int(row)
        c1, r1 = min(c0 + 1, SIDE - 1), min(r0 + 1, SIDE - 1)
        tx, ty = column - c0, row - r0
        return (
            heights[r0 * SIDE + c0] * (1 - tx) * (1 - ty)
            + heights[r0 * SIDE + c1] * tx * (1 - ty)
            + heights[r1 * SIDE + c0] * (1 - tx) * ty
            + heights[r1 * SIDE + c1] * tx * ty
        )

    return height_at


def decode_vhgt_alt(data, offset_scale):
    """VHGT with the header float scaled by `offset_scale` instead of 8."""
    import struct

    count = SIDE * SIDE
    if len(data) < 4 + count:
        return None
    origin = struct.unpack_from("<f", data, 0)[0] * offset_scale
    deltas = data[4 : 4 + count]
    heights = [0.0] * count
    row_origin = origin
    for row in range(SIDE):
        row_origin += struct.unpack_from("<b", deltas, row * SIDE)[0] * 8.0
        height = row_origin
        heights[row * SIDE] = height
        for column in range(1, SIDE):
            height += struct.unpack_from("<b", deltas, row * SIDE + column)[0] * 8.0
            heights[row * SIDE + column] = height
    return heights


def bilinear(heights, x, y):
    grid_x = math.floor(x / 4096.0)
    grid_y = math.floor(y / 4096.0)
    column = min(max((x - grid_x * 4096.0) / SAMPLE, 0.0), SIDE - 1.0)
    row = min(max((y - grid_y * 4096.0) / SAMPLE, 0.0), SIDE - 1.0)
    c0, r0 = int(column), int(row)
    c1, r1 = min(c0 + 1, SIDE - 1), min(r0 + 1, SIDE - 1)
    tx, ty = column - c0, row - r0
    return (
        heights[r0 * SIDE + c0] * (1 - tx) * (1 - ty)
        + heights[r0 * SIDE + c1] * tx * (1 - ty)
        + heights[r1 * SIDE + c0] * (1 - tx) * ty
        + heights[r1 * SIDE + c1] * tx * ty
    )


def vhgt(world_hex, arguments):
    """The VHGT header float and both candidate decodings, against known ground points.

    arguments are triples: x y expected_z (the z of a reference known to stand on the
    ground, such as a house door). The decode that lands the terrain on those z's is the
    right one.
    """
    import struct

    import esm_route
    import land_edges
    import land_vtxt

    plugin = esm_route.Plugin(PLUGIN)
    world = int(world_hex, 16)
    cache = {}
    for index in range(0, len(arguments), 3):
        x, y, expected = (float(value) for value in arguments[index : index + 3])
        grid = (math.floor(x / 4096.0), math.floor(y / 4096.0))
        if grid not in cache:
            payloads = land_vtxt.land_records(plugin, world, grid[0], grid[1])
            data = land_edges.fields(payloads[0]).get(b"VHGT", b"") if payloads else b""
            header = struct.unpack_from("<f", data, 0)[0] if len(data) >= 4 else float("nan")
            cache[grid] = (
                header,
                decode_vhgt_alt(data, 8.0),
                decode_vhgt_alt(data, 1.0),
            )
        header, times8, times1 = cache[grid]
        print(
            f"  ({x:9.0f}, {y:9.0f}) ref z {expected:9.1f} | grid {grid} header {header:9.3f} | "
            f"x8 {bilinear(times8, x, y):9.1f}  x1 {bilinear(times1, x, y):9.1f}"
        )
    for grid, (header, times8, times1) in sorted(cache.items()):
        print(
            f"  grid {grid}: header {header:.3f}, x8 terrain {min(times8):.0f}..{max(times8):.0f}, "
            f"x1 terrain {min(times1):.0f}..{max(times1):.0f}"
        )


def route(world_hex, arguments):
    """Walk a polyline: per-leg heading, length, and the terrain under every step.

    `arguments` are x y pairs; the first is the start. Water height -250 (the Riverwood
    cells' XCLW) is used to flag a step that is under water.
    """
    height_at = land_cache()
    world = int(world_hex, 16)
    water = -250.0
    points = [
        (float(arguments[index]), float(arguments[index + 1]))
        for index in range(0, len(arguments) - 1, 2)
    ]
    total = 0.0
    print(f"== route in worldspace {world_hex}, {len(points)} waypoints")
    for index, (x, y) in enumerate(points):
        z = height_at(world, x, y)
        if index:
            px, py = points[index - 1]
            dx, dy = x - px, y - py
            length = math.hypot(dx, dy)
            total += length
            heading = math.degrees(math.atan2(dx, dy)) % 360.0
            print(
                f"\n  leg {index}: {length:7.0f} units, heading {heading:6.1f} deg "
                f"({math.radians(heading):+.4f} rad) from ({px:.0f}, {py:.0f})"
            )
            steps = max(int(length / 100.0), 1)
            lowest, highest, under = 1.0e9, -1.0e9, 0
            for step in range(1, steps + 1):
                fraction = step / steps
                sx, sy = px + dx * fraction, py + dy * fraction
                sz = height_at(world, sx, sy)
                if sz is None:
                    continue
                lowest, highest = min(lowest, sz), max(highest, sz)
                if sz < water:
                    under += 1
            print(
                f"      terrain along the leg {lowest:8.1f} .. {highest:8.1f} "
                f"(start {height_at(world, px, py):8.1f}, end {height_at(world, x, y):8.1f}); "
                f"{under}/{steps} steps under the {water:.0f} water level"
            )
        print(f"  wp{index}: ({x:9.1f}, {y:9.1f}) ground z {z:9.1f}" if z is not None else
              f"  wp{index}: ({x:9.1f}, {y:9.1f}) no LAND")
    print(f"\n  total {total:.0f} units ({total * 0.01428:.0f} m at 1.428 cm per unit)")


def river(world_hex, x0, x1, y0, y1, y_step, water):
    """Where the river is: for each y, the wet x-span (terrain below `water`)."""
    height_at = land_cache()
    world = int(world_hex, 16)
    step = 50.0
    print(
        f"== water below {water:.0f} in worldspace {world_hex}, "
        f"x {x0:.0f}..{x1:.0f}, y {y0:.0f}..{y1:.0f}"
    )
    y = float(y0)
    while y <= y1:
        spans, current = [], None
        x = float(x0)
        lowest = 1.0e9
        while x <= x1:
            z = height_at(world, x, y)
            if z is not None:
                lowest = min(lowest, z)
                if z < water:
                    current = [x, x] if current is None else [current[0], x]
                elif current is not None:
                    spans.append(current)
                    current = None
            x += step
        if current is not None:
            spans.append(current)
        text = ", ".join(f"{a:.0f}..{b:.0f}" for a, b in spans) if spans else "dry across"
        print(f"  y {y:9.0f}: lowest {lowest:8.1f} | wet x {text}")
        y += y_step


def ground(world_hex, points):
    """Ground Z (VHGT, from Skyrim.esm) at each x,y point, or the water level."""
    height_at = land_cache()
    world = int(world_hex, 16)
    print(f"== ground heights in worldspace {world_hex} (VHGT samples, bilinear)")
    for (x, y) in points:
        height = height_at(world, x, y)
        shown = "no LAND" if height is None else f"{height:9.1f}"
        print(f"  ({x:9.1f}, {y:9.1f})  z {shown}")


def notes(conn, substring):
    print("-- record types in the database")
    for row in conn.execute(
        "select record_type, count(*) from records group by record_type order by count(*) desc"
    ):
        print(f"  {row[1]:8d}  {row[0]}")
    print("\n-- waters")
    for row in conn.execute(
        "select id, editor_id, opacity, shallow_color, deep_color, flow_normal_path from waters"
    ):
        print(
            f"  {hexid(row[0])} {row[1]!r} opacity {row[2]} shallow {row[3]} deep {row[4]} "
            f"flow {row[5]}"
        )
    print(f"\n-- statics matching {substring!r}")
    for row in conn.execute(
        "select id, editor_id, model_path, bounds_min_x, bounds_min_y, bounds_min_z, "
        "bounds_max_x, bounds_max_y, bounds_max_z, bounds_valid, material_object, "
        "material_max_angle from statics where lower(model_path) like ? or lower(editor_id) "
        "like ? limit 40",
        (f"%{substring.lower()}%", f"%{substring.lower()}%"),
    ):
        print(
            f"  {hexid(row[0])} {row[1]!r} {row[2]}\n"
            f"      bounds ({row[3]:.0f}, {row[4]:.0f}, {row[5]:.0f}) .. "
            f"({row[6]:.0f}, {row[7]:.0f}, {row[8]:.0f}) valid {row[9]} "
            f"mato {hexid(row[10]) if row[10] else '-'} angle {row[11]}"
        )


CONVERTED = "C:/Modding/SkyrimConverted"


def converted_meshes():
    """Every converted meshes/ path, lowercased, plus a basename index."""
    import os

    paths, names = set(), set()
    for root, _dirs, files in os.walk(f"{CONVERTED}/meshes"):
        for name in files:
            full = os.path.join(root, name).replace("\\", "/").lower()
            paths.add(full)
            names.add(name.lower())
    return paths, names


def assets(conn, box):
    """Which models the route's references need, and whether a converted glb exists."""
    paths, names = converted_meshes()
    print(f"{len(paths)} converted mesh files under {CONVERTED}/meshes")
    missing = collections.Counter()
    present = collections.Counter()
    for row in conn.execute(
        "select distinct s.model_path from \"references\" r "
        "left join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? and s.model_path is not null",
        (box[0], box[2], box[1], box[3]),
    ):
        model = row[0]
        converted = model.replace("\\", "/").lower()
        if converted.endswith(".nif"):
            converted = converted[:-4] + ".glb"
        full = f"{CONVERTED}/meshes/{converted}"
        if full in paths or converted.rsplit("/", 1)[-1] in names:
            present[model] = 1
        else:
            missing[model] = 1
    total = sum(missing.values()) + sum(present.values())
    print(f"{total} distinct models in the box: {len(present)} have a converted glb, "
          f"{len(missing)} do not")
    for model in sorted(missing):
        print(f"  MISSING  {model}")


def buildings(conn, box):
    """The house shells and the mill in a box, the village's structure."""
    for row in conn.execute(
        "select r.id, s.editor_id, s.model_path, r.pos_x, r.pos_y, r.pos_z, r.rot_z, "
        "s.bounds_min_x, s.bounds_max_x, s.bounds_min_y, s.bounds_max_y from \"references\" r "
        "join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? "
        "and lower(s.model_path) like '%farmhouse%' "
        "and lower(s.model_path) not like '%walkway%' and lower(s.model_path) not like '%fence%' "
        "and lower(s.model_path) not like '%ivy%' and lower(s.model_path) not like '%wall%' "
        "and lower(s.model_path) not like '%lumbermill01w%' "
        "order by r.pos_y",
        (box[0], box[2], box[1], box[3]),
    ):
        print(
            f"  {hexid(row[0])} ({row[3]:8.0f}, {row[4]:8.0f}, {row[5]:8.0f}) rz {row[6]:.3f} "
            f"size {row[8] - row[7]:.0f} x {row[10] - row[9]:.0f}  {row[1]} {row[2]}"
        )


def roads(conn, box):
    """The road chunks in a box, ordered south to north: the village street."""
    for row in conn.execute(
        "select r.id, s.editor_id, s.model_path, r.pos_x, r.pos_y, r.pos_z, r.rot_z, r.scale "
        "from \"references\" r left join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? "
        "and (lower(coalesce(s.model_path,'')) like '%roadchunk%' "
        "or lower(coalesce(s.model_path,'')) like '%landscape\\paths%') "
        "order by r.pos_y",
        (box[0], box[2], box[1], box[3]),
    ):
        print(
            f"  {hexid(row[0])} ({row[3]:8.0f}, {row[4]:8.0f}, {row[5]:8.0f}) rz {row[6]:.3f} "
            f"s {row[7]:.2f} {row[1] or ''} {row[2]}"
        )


def extras(conn, box):
    """NPCs, snow candidates, trees and markers in a box: the gap counts."""
    print("-- NPC references (present in the data, nothing draws them yet)")
    for row in conn.execute(
        "select r.id, n.editor_id, n.full_name, r.pos_x, r.pos_y, r.pos_z from \"references\" r "
        "join npcs n on n.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? order by r.pos_y",
        (box[0], box[2], box[1], box[3]),
    ):
        print(f"  {hexid(row[0])} ({row[3]:8.0f}, {row[4]:8.0f}, {row[5]:8.0f}) {row[1]} {row[2]!r}")
    print("\n-- snow candidates (statics with a DNAM material_object)")
    for row in conn.execute(
        "select s.model_path, s.material_max_angle, count(*) from \"references\" r "
        "join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? and s.material_object is not null "
        "group by s.model_path order by count(*) desc limit 25",
        (box[0], box[2], box[1], box[3]),
    ):
        print(f"  {row[2]:6d}  angle {row[1]}  {row[0]}")
    print("\n-- trees and plants by model")
    for row in conn.execute(
        "select s.model_path, count(*) from \"references\" r join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? and ("
        "lower(s.model_path) like 'landscape\\trees%' or lower(s.model_path) like 'trees\\%') "
        "group by s.model_path order by count(*) desc limit 30",
        (box[0], box[2], box[1], box[3]),
    ):
        print(f"  {row[1]:6d}  {row[0]}")
    print("\n-- animated furniture (models whose name says animating)")
    for row in conn.execute(
        "select s.editor_id, s.model_path, count(*) from \"references\" r "
        "join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? and "
        "(lower(s.model_path) like '%animating%' or lower(s.model_path) like '%wheel%' "
        "or lower(s.editor_id) like '%sawmill%' or lower(s.editor_id) like '%mill%') "
        "group by s.model_path order by count(*) desc",
        (box[0], box[2], box[1], box[3]),
    ):
        print(f"  {row[2]:6d}  {row[0]}  {row[1]}")


def door_bases(conn, box):
    """Every load door in a box with its base record: what makes it auto-load."""
    for row in conn.execute(
        "select r.id, r.pos_x, r.pos_y, r.pos_z, r.scale, s.id, s.editor_id, s.model_path, "
        "d.destination_cell_id, d.destination_worldspace_id, d.pos_x, d.pos_y, d.pos_z, d.rot_z, "
        "d.destination_ref_id from \"references\" r "
        "join door_links d on d.ref_id = r.id left join statics s on s.id = r.base_form_id "
        "where r.pos_x between ? and ? and r.pos_y between ? and ? order by r.pos_y",
        (box[0], box[2], box[1], box[3]),
    ):
        print(
            f"  ref {hexid(row[0])} ({row[1]:.1f}, {row[2]:.1f}, {row[3]:.1f}) scale {row[4]:.2f} "
            f"base {hexid(row[5])} {row[6]!r} {row[7]}\n"
            f"      -> cell {cell_label(conn, row[8])} world {row[9]} ref {hexid(row[14])} "
            f"arrival ({row[10]:.2f}, {row[11]:.2f}, {row[12]:.2f}) rot_z {row[13]:.4f}"
        )


def cell_stats(conn, ids):
    """Size and contents of an interior: what a camera pose inside it has to work with."""
    for raw in ids:
        cell_id = int(raw, 16)
        row = conn.execute(
            "select count(*), min(pos_x), max(pos_x), min(pos_y), max(pos_y), min(pos_z), "
            "max(pos_z) from \"references\" where cell_id = ?",
            (cell_id,),
        ).fetchone()
        lights = conn.execute(
            'select count(*) from "references" r join lights l on l.id = r.base_form_id '
            "where r.cell_id = ?",
            (cell_id,),
        ).fetchone()[0]
        print(
            f"  {cell_label(conn, cell_id)}: {row[0]} refs, x {row[1]:.0f}..{row[2]:.0f} "
            f"({row[2] - row[1]:.0f} wide), y {row[3]:.0f}..{row[4]:.0f} ({row[4] - row[3]:.0f}), "
            f"z {row[5]:.0f}..{row[6]:.0f} ({row[6] - row[5]:.0f} tall), {lights} light refs"
        )


def spaces(conn, ids):
    """The space_lighting row of every space named, and its cell/ref count."""
    for raw in ids:
        space_id = int(raw, 16)
        row = conn.execute(
            "select is_interior, template_id, ambient, directional, fog, fog_near, fog_far, "
            "fog_power, fog_clip, sun_illuminance, climate_id, weather_id, has_sky, direction_fade, "
            "sky_upper, sky_lower, sun from space_lighting where space_id = ?",
            (space_id,),
        ).fetchone()
        cell = cell_row(conn, space_id)
        label = cell_label(conn, space_id) if cell else hexid(space_id)
        if row is None:
            print(f"  {label}: no space_lighting row")
            continue
        print(
            f"  {label}: interior {row[0]} template {hexid(row[1])} ambient {hexid(row[2])} "
            f"directional {hexid(row[3])} fog {hexid(row[4])} fog_near {row[5]} fog_far {row[6]} "
            f"power {row[7]} clip {row[8]} sun_illuminance {row[9]} climate {hexid(row[10])} "
            f"weather {hexid(row[11])} has_sky {row[12]} dir_fade {row[13]} "
            f"sky_upper {hexid(row[14])} sky_lower {hexid(row[15])} sun {hexid(row[16])}"
        )


def grid_count(conn, grids):
    """Reference, light, door and NPC counts per exterior grid, by position."""
    for grid_x, grid_y in grids:
        x0, y0 = grid_x * 4096.0, grid_y * 4096.0
        row = conn.execute(
            'select count(*) from "references" where pos_x >= ? and pos_x < ? '
            "and pos_y >= ? and pos_y < ?",
            (x0, x0 + 4096.0, y0, y0 + 4096.0),
        ).fetchone()
        lights = conn.execute(
            'select count(*) from "references" r join lights l on l.id = r.base_form_id '
            "where r.pos_x >= ? and r.pos_x < ? and r.pos_y >= ? and r.pos_y < ?",
            (x0, x0 + 4096.0, y0, y0 + 4096.0),
        ).fetchone()[0]
        doors = conn.execute(
            'select count(*) from "references" r join door_links d on d.ref_id = r.id '
            "where r.pos_x >= ? and r.pos_x < ? and r.pos_y >= ? and r.pos_y < ?",
            (x0, x0 + 4096.0, y0, y0 + 4096.0),
        ).fetchone()[0]
        npcs = conn.execute(
            'select count(*) from "references" r join npcs n on n.id = r.base_form_id '
            "where r.pos_x >= ? and r.pos_x < ? and r.pos_y >= ? and r.pos_y < ?",
            (x0, x0 + 4096.0, y0, y0 + 4096.0),
        ).fetchone()[0]
        invalid = conn.execute(
            'select count(*) from "references" r join statics s on s.id = r.base_form_id '
            "where r.pos_x >= ? and r.pos_x < ? and r.pos_y >= ? and r.pos_y < ? "
            "and s.bounds_valid = 0 and s.model_path <> ''",
            (x0, x0 + 4096.0, y0, y0 + 4096.0),
        ).fetchone()[0]
        kind = conn.execute(
            "select interior_name from cells where worldspace_id = 60 and grid_x = ? and grid_y = ?",
            (grid_x, grid_y),
        ).fetchone()
        print(
            f"  grid ({grid_x:3d}, {grid_y:3d}) {kind[0] if kind else '':<22} refs {row[0]:5d}  "
            f"lights {lights:3d}  doors {doors:2d}  npcs {npcs:3d}  bounds_valid=0 {invalid:3d}"
        )


def look(arguments):
    """Camera poses: six values per pose (eye xyz, target xyz), Creation yaw, Skyrim pitch."""
    values = [float(value) for value in arguments]
    for index in range(0, len(values) - 5, 6):
        x, y, z, tx, ty, tz = values[index : index + 6]
        dx, dy, dz = tx - x, ty - y, tz - z
        horizontal = math.hypot(dx, dy)
        yaw = math.degrees(math.atan2(dx, dy)) % 360.0
        pitch = -math.degrees(math.atan2(dz, horizontal)) if horizontal else 0.0
        print(
            f"  eye ({x:.0f}, {y:.0f}, {z:.0f}) -> ({tx:.0f}, {ty:.0f}, {tz:.0f}): "
            f"yaw {yaw:.1f} deg ({math.radians(yaw):+.4f} rad), pitch {pitch:+.1f} deg, "
            f"distance {math.sqrt(dx * dx + dy * dy + dz * dz):.0f}"
        )


def glb_info(models):
    """Nodes, meshes and animations of a converted glb (its JSON chunk)."""
    import json
    import struct

    for model in models:
        path = f"{CONVERTED}/meshes/{model.replace(chr(92), '/').lower()}"
        if not path.endswith(".glb"):
            path = path[:-4] + ".glb"
        try:
            with open(path, "rb") as handle:
                header = handle.read(12)
                magic, _version, _length = struct.unpack("<4sII", header)
                if magic != b"glTF":
                    print(f"  {path}: not a glb")
                    continue
                chunk_length, chunk_type = struct.unpack("<I4s", handle.read(8))
                document = json.loads(handle.read(chunk_length))
        except OSError as error:
            print(f"  {path}: {error}")
            continue
        animations = document.get("animations", [])
        print(
            f"  {model}: nodes {len(document.get('nodes', []))}, "
            f"meshes {len(document.get('meshes', []))}, "
            f"materials {len(document.get('materials', []))}, "
            f"animations {len(animations)}"
        )
        for animation in animations:
            print(
                f"      animation {animation.get('name')!r}: "
                f"{len(animation.get('channels', []))} channels, "
                f"{len(animation.get('samplers', []))} samplers"
            )
        for node in document.get("nodes", [])[:8]:
            if node.get("name"):
                print(f"      node {node['name']!r} mesh {node.get('mesh')}")


def snow(conn, box):
    """References whose base carries a material_object, split by base editor id."""
    for row in conn.execute(
        "select sum(case when s.material_object is null then 1 else 0 end), "
        "sum(case when s.material_object = 0 then 1 else 0 end), "
        "sum(case when s.material_object > 0 then 1 else 0 end), "
        "sum(case when s.material_max_angle is not null then 1 else 0 end) "
        'from "references" r join statics s on s.id = r.base_form_id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ?",
        (box[0], box[2], box[1], box[3]),
    ):
        print(
            f"-- material_object NULL {row[0]}, = 0 {row[1]}, > 0 (a real MATO) {row[2]}; "
            f"material_max_angle present {row[3]}"
        )
    print("-- references with a DNAM material_object, by base editor id")
    total = 0
    snow_named = 0
    for row in conn.execute(
        "select s.editor_id, s.material_object, s.material_max_angle, count(*) "
        'from "references" r join statics s on s.id = r.base_form_id '
        "where r.pos_x between ? and ? and r.pos_y between ? and ? "
        "and s.material_object is not null group by s.id order by count(*) desc",
        (box[0], box[2], box[1], box[3]),
    ):
        total += row[3]
        if "snow" in (row[0] or "").lower():
            snow_named += row[3]
        print(f"  {row[3]:5d}  mato {hexid(row[1])} angle {row[2]}  {row[0]}")
    print(f"  total {total} references, {snow_named} of them on a base whose editor id says snow")


def around(conn, x, y, radius):
    rows = conn.execute(
        "select r.id, r.cell_id, s.editor_id, s.model_path, r.pos_x, r.pos_y, r.pos_z, "
        "r.rot_z, r.scale from \"references\" r left join statics s on s.id = r.base_form_id "
        "where abs(r.pos_x - ?) < ? and abs(r.pos_y - ?) < ? "
        "order by (r.pos_x-?)*(r.pos_x-?)+(r.pos_y-?)*(r.pos_y-?)",
        (x, radius, y, radius, x, x, y, y),
    ).fetchall()
    print(f"{len(rows)} references within {radius:.0f} units of ({x:.0f}, {y:.0f})")
    for row in rows[:120]:
        print(
            f"  {hexid(row[0])} cell {hexid(row[1])} ({row[4]:8.0f}, {row[5]:8.0f}, "
            f"{row[6]:8.0f}) rz {row[7]:.3f} s {row[8]:.2f} {row[2] or ''} {row[3]}"
        )


def main(argv):
    conn = connect()
    if len(argv) < 2:
        print(__doc__)
        return 2
    command = argv[1]
    if command == "survey":
        survey(conn)
    elif command == "cell":
        cell_report(conn, int(argv[2], 16), "--refs" in argv)
    elif command == "doors":
        doors_report(conn, int(argv[2], 16))
    elif command == "around":
        around(conn, float(argv[2]), float(argv[3]), float(argv[4]))
    elif command == "box":
        box(conn, float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5]))
    elif command == "doorplaces":
        door_places(conn, int(argv[2]) if len(argv) > 2 else TAMRIEL)
    elif command == "gaps":
        gaps(conn, (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])))
    elif command == "buildings":
        buildings(conn, (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])))
    elif command == "roads":
        roads(conn, (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])))
    elif command == "extras":
        extras(conn, (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])))
    elif command == "assets":
        assets(conn, (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])))
    elif command == "notes":
        notes(conn, argv[2] if len(argv) > 2 else "lumbermill")
    elif command == "invalid":
        invalid_models(conn, (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])))
    elif command == "interiors":
        interiors_reachable(conn, (float(argv[2]), float(argv[3])), float(argv[4]))
    elif command == "snow":
        snow(conn, (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])))
    elif command == "glb":
        glb_info(argv[2:])
    elif command == "doorbases":
        door_bases(conn, (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])))
    elif command == "cellstats":
        cell_stats(conn, argv[2:])
    elif command == "spaces":
        spaces(conn, argv[2:])
    elif command == "gridcount":
        grid_count(
            conn,
            [
                (int(argv[index]), int(argv[index + 1]))
                for index in range(2, len(argv) - 1, 2)
            ],
        )
    elif command == "look":
        look(argv[2:])
    elif command == "river":
        river(
            argv[2],
            float(argv[3]),
            float(argv[4]),
            float(argv[5]),
            float(argv[6]),
            float(argv[7]),
            float(argv[8]),
        )
    elif command == "route":
        route(argv[2], argv[3:])
    elif command == "vhgt":
        vhgt(argv[2], argv[3:])
    elif command == "ground":
        points = [
            (float(argv[i]), float(argv[i + 1]))
            for i in range(3, len(argv) - 1, 2)
        ]
        ground(argv[2], points)
    elif command == "map":
        ascii_map(
            conn,
            (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])),
            float(argv[6]) if len(argv) > 6 else 512.0,
        )
    elif command == "modelsnear":
        for substring in argv[6:]:
            models_near(conn, substring, float(argv[2]), float(argv[3]), float(argv[4]))
    elif command == "keywords":
        keywords = argv[6:] if len(argv) > 6 else [
            "mill", "wheel", "bridge", "gate", "stone", "snow", "roof", "water", "tree",
            "grass", "sign", "well", "log", "crate", "lumber", "stump", "rock",
        ]
        keyword_census(
            conn, (float(argv[2]), float(argv[3]), float(argv[4]), float(argv[5])), keywords
        )
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
