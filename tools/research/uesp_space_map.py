#!/usr/bin/env python3
"""Draw a top-down map of a game space so a reference screenshot can be matched to the world.

Usage:
    python tools/research/uesp_space_map.py <space> <out.png> [options]

    <space>          cell:<hex form id>   an interior cell, e.g. cell:0x152C3 (Alftand01)
                     world:<decimal id>  a worldspace, e.g. world:126562 (Blackreach)

Options:
    --filter a,b,c   only draw references whose model path or editor id contains one of
                     these substrings (case-insensitive).  Repeatable.
    --center x,y     centre the map here (default: the centre of the reference extents)
    --radius r       half-width of the map in Creation units (default: fit the references)
    --size px        output image size in pixels (default 1600)
    --zrange a,b     only draw references whose pos_z is inside this range
    --terrain        draw the cell terrain heightmaps as a background
    --no-labels      dots only
    --list           print every drawn reference as a table (default: on)
    --quiet          do not print the table

North is +Y and up in the image (Skyrim's map orientation).  Grid lines every 1024 units,
heavier lines every 4096 (a cell) and every 16384.
"""

import sqlite3
import sys

from PIL import Image, ImageDraw, ImageFont

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"
CELL = 4096.0


def connect():
    return sqlite3.connect(f"file:{DB}?mode=ro", uri=True)


def parse_space(spec):
    kind, value = spec.split(":", 1)
    if kind == "cell":
        return ("cell_id", int(value, 16))
    if kind == "world":
        return ("worldspace_id", int(value))
    raise SystemExit(f"unknown space {spec!r}; use cell:<hex> or world:<id>")


def load_references(conn, column, key, box=None, zrange=None):
    query = (
        "select r.id, r.cell_id, r.base_form_id, s.model_path, s.editor_id,"
        " r.pos_x, r.pos_y, r.pos_z, r.rot_z, r.scale"
        ' from "references" r left join statics s on s.id = r.base_form_id'
        f" where r.{column} = ?"
    )
    rows = []
    for row in conn.execute(query, (key,)):
        if box is not None:
            x, y, radius = box
            if abs(row[5] - x) > radius or abs(row[6] - y) > radius:
                continue
        if zrange is not None and not (zrange[0] <= row[7] <= zrange[1]):
            continue
        rows.append(
            {
                "id": row[0],
                "cell_id": row[1],
                "base": row[2],
                "model": row[3] or "",
                "editor": (row[4] or "").strip("\x00"),
                "x": row[5],
                "y": row[6],
                "z": row[7],
                "rot_z": row[8],
                "scale": row[9],
            }
        )
    return rows


def decode_heightmap(blob):
    """Absolute heights of a 33x33 LAND grid, or None.

    The database stores the raw `VHGT` subrecord: a little-endian float offset, 1089 signed
    byte deltas (8 units each, accumulated across a row and from row to row) and three padding
    bytes.  Same decoding as `crates/converter/src/esm/cell_cache.rs` `decode_vhgt`.
    """
    import numpy as np

    if blob is None or len(blob) < 4 + 1089:
        return None
    offset = np.frombuffer(blob[:4], dtype="<f4")[0] * 8.0
    deltas = np.frombuffer(blob[4 : 4 + 1089], dtype="i1").astype("f4")
    heights = np.zeros(1089, dtype="f4")
    row_origin = float(offset)
    for row in range(33):
        row_origin += deltas[row * 33] * 8.0
        height = row_origin
        heights[row * 33] = height
        for column in range(1, 33):
            height += deltas[row * 33 + column] * 8.0
            heights[row * 33 + column] = height
    return heights.reshape(33, 33)


def load_terrain(conn, column, key):
    """(world_x, world_y, world_z) samples for every LAND cell of a space."""
    import numpy as np

    if column == "worldspace_id":
        query = (
            "select l.cell_id, l.heightmap, c.grid_x, c.grid_y from land l"
            " join cells c on c.id = l.cell_id where c.worldspace_id = ?"
        )
    else:
        query = (
            "select l.cell_id, l.heightmap, c.grid_x, c.grid_y from land l"
            " join cells c on c.id = l.cell_id where l.cell_id = ?"
        )
    points = []
    for cell_id, blob, grid_x, grid_y in conn.execute(query, (key,)):
        heights = decode_heightmap(blob)
        if heights is None:
            continue
        if grid_x is None or grid_y is None:
            continue
        # LAND vertex (row, column): +x east, +y north.  A cell's west/south edge is
        # grid * 4096 and its 33 vertices step 128 units from there.
        index = np.arange(33) * 128.0
        xs = grid_x * CELL + index
        ys = grid_y * CELL + index
        grid_heights = np.asarray(heights, dtype="f4")
        mesh_x, mesh_y = np.meshgrid(xs, ys, indexing="xy")
        points.append(
            np.stack([mesh_x.ravel(), mesh_y.ravel(), grid_heights.ravel()], axis=1)
        )
    if not points:
        return None
    return np.concatenate(points, axis=0)


def font(size):
    for name in ("segoeui.ttf", "arial.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    space = parse_space(argv[1])
    out = argv[2]
    filters, center, radius, size, zrange, terrain = [], None, None, 1600, None, False
    labels, quiet = True, False
    max_per_model, show_height = 0, False
    index = 3
    while index < len(argv):
        option = argv[index]
        if option == "--filter":
            filters.append(argv[index + 1].lower())
            index += 2
        elif option == "--center":
            center = tuple(float(v) for v in argv[index + 1].split(","))
            index += 2
        elif option == "--radius":
            radius = float(argv[index + 1])
            index += 2
        elif option == "--size":
            size = int(argv[index + 1])
            index += 2
        elif option == "--zrange":
            zrange = tuple(float(v) for v in argv[index + 1].split(","))
            index += 2
        elif option == "--terrain":
            terrain = True
            index += 1
        elif option == "--max-per-model":
            max_per_model = int(argv[index + 1])
            index += 2
        elif option == "--with-height":
            show_height = True
            index += 1
        elif option == "--no-labels":
            labels = False
            index += 1
        elif option == "--quiet":
            quiet = True
            index += 1
        else:
            raise SystemExit(f"unknown option {option!r}")

    conn = connect()
    box = None
    if center is not None:
        span = radius if radius is not None else 4096.0
        box = (center[0], center[1], span)
    rows = load_references(conn, space[0], space[1], box, zrange)
    if not rows:
        raise SystemExit("no references in that space / box")

    if center is None:
        xs = [row["x"] for row in rows]
        ys = [row["y"] for row in rows]
        center = ((min(xs) + max(xs)) / 2.0, (min(ys) + max(ys)) / 2.0)
    if radius is None:
        radius = (
            max(max(abs(row["x"] - center[0]), abs(row["y"] - center[1])) for row in rows)
            * 1.05
            + 256.0
        )

    image = Image.new("RGB", (size, size), (18, 20, 26))
    draw = ImageDraw.Draw(image)
    scale = size / (2.0 * radius)

    def to_pixel(x, y):
        return (size / 2.0 + (x - center[0]) * scale, size / 2.0 - (y - center[1]) * scale)

    if terrain:
        points = load_terrain(conn, space[0], space[1])
        if points is not None and len(points):
            import numpy as np

            ground = points[
                (np.abs(points[:, 0] - center[0]) <= radius)
                & (np.abs(points[:, 1] - center[1]) <= radius)
            ]
            if len(ground):
                low, high = float(ground[:, 2].min()), float(ground[:, 2].max())
                span = max(high - low, 1.0)
                dot = max(128.0 * scale / 2.0, 1.0)
                for x, y, z in ground:
                    px, py = to_pixel(x, y)
                    shade = int(40 + 190 * (z - low) / span)
                    draw.rectangle(
                        [px - dot, py - dot, px + dot, py + dot],
                        fill=(shade // 3, shade // 2, shade),
                    )
                print(f"  terrain: {len(ground)} vertices, z {low:.0f}..{high:.0f}")

    # Grid.
    step = 1024 if radius < 30000 else 4096
    start = int((center[0] - radius) // step) * step
    while start <= center[0] + radius:
        x0 = to_pixel(start, center[1] - radius)[0]
        heavy = start % (step * 4) == 0
        colour = (70, 78, 92) if heavy else (38, 42, 52)
        draw.line([(x0, 0), (x0, size)], fill=colour, width=2 if heavy else 1)
        if heavy:
            draw.text((x0 + 3, 3), f"{int(start)}", fill=(130, 140, 160), font=font(14))
        start += step
    start = int((center[1] - radius) // step) * step
    while start <= center[1] + radius:
        y0 = to_pixel(center[0] - radius, start)[1]
        heavy = start % (step * 4) == 0
        colour = (70, 78, 92) if heavy else (38, 42, 52)
        draw.line([(0, y0), (size, y0)], fill=colour, width=2 if heavy else 1)
        if heavy:
            draw.text((3, y0 + 3), f"{int(start)}", fill=(130, 140, 160), font=font(14))
        start += step

    drawn = []
    palette = [
        (255, 96, 96),
        (96, 220, 255),
        (255, 210, 80),
        (150, 255, 120),
        (255, 140, 220),
        (170, 170, 255),
        (255, 170, 90),
        (120, 255, 220),
    ]
    per_model = {}
    for row in rows:
        text = (row["editor"] + " " + row["model"]).lower()
        if filters and not any(needle in text for needle in filters):
            continue
        key = row["model"]
        per_model[key] = per_model.get(key, 0) + 1
        if max_per_model and per_model[key] > max_per_model:
            continue
        colour = palette[len(drawn) % len(palette)] if filters else (220, 220, 220)
        px, py = to_pixel(row["x"], row["y"])
        if not (0 <= px < size and 0 <= py < size):
            continue
        draw.ellipse([px - 4, py - 4, px + 4, py + 4], fill=colour, outline=(0, 0, 0))
        if labels:
            name = (row["editor"] or row["model"].rsplit("/", 1)[-1]) + (
                f" {row['z']:.0f}" if show_height else ""
            )
            left, top, right, bottom = draw.textbbox((px + 7, py - 8), name, font=font(14))
            draw.rectangle([left - 1, top - 1, right + 1, bottom + 1], fill=(0, 0, 0))
            draw.text((px + 7, py - 8), name, fill=colour, font=font(14))
        row["_colour"] = colour
        drawn.append(row)

    if "/" in out:
        import os

        os.makedirs(out.rsplit("/", 1)[0], exist_ok=True)
    image.save(out)
    print(
        f"  wrote {out}: {image.width}x{image.height}, centre ({center[0]:.0f},{center[1]:.0f}),"
        f" radius {radius:.0f}, {scale:.4f} px/unit, {len(drawn)} references drawn"
    )
    if not quiet:
        drawn.sort(key=lambda row: (row["cell_id"], row["editor"], row["model"]))
        for row in drawn:
            print(
                f"    {row['x']:9.0f} {row['y']:9.0f} {row['z']:8.0f}  "
                f"{row['editor'][:34]:34.34} {row['model']}"
            )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
