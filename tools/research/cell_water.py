#!/usr/bin/env python3
"""Compare each exterior cell's water plane height (CELL/XCLW) with its terrain heights.

crates/engine/src/streaming.rs:470-516 spawns a 4096x4096 water plane for every cell whose
XCLW is a real height, at that height, blended over the cell. If XCLW is close to the
terrain heights of the cell, the two surfaces are nearly coplanar and z-fight over the
whole cell.

Usage:
    python tools/research/cell_water.py <plugin> <world_hex> <grid_x> <grid_y> <radius>
"""

import struct
import sys

import esm_route  # noqa: E402
import land_edges  # noqa: E402
import land_vtxt  # noqa: E402

SIDE = 33


def water_heights(plugin, world_id, grids):
    """CELL/XCLW for each requested grid in the worldspace."""
    wanted = set(grids)
    out = {}
    state = {"grid": None, "world": None, "match": False}

    def visit(tag, form_id, flags, payload, cell_ctx, world_ctx):
        if flags & esm_route.FLAG_COMPRESSED:
            try:
                payload = esm_route.decoded(payload, flags)
            except Exception:
                return
        if tag == b"CELL":
            xclc = esm_route.field(payload, b"XCLC")
            grid = (
                (esm_route.i32(xclc, 0), esm_route.i32(xclc, 4))
                if xclc and len(xclc) >= 8
                else None
            )
            state["match"] = world_ctx == world_id and grid in wanted
            if not state["match"]:
                return
            xclw = esm_route.field(payload, b"XCLW")
            xcwt = esm_route.field(payload, b"XCWT")
            height = (
                struct.unpack_from("<f", xclw, 0)[0]
                if xclw and len(xclw) >= 4
                else None
            )
            water_type = esm_route.u32(xcwt, 0) if xcwt and len(xcwt) >= 4 else None
            out[grid] = (
                height,
                water_type,
                esm_route.cstring(esm_route.field(payload, b"EDID") or b""),
            )

    esm_route.walk(plugin.buf, esm_route.u32(plugin.buf, 4) + 24, len(plugin.buf), None, None, visit)
    return out


def main(argv):
    if len(argv) < 6:
        print(__doc__)
        return 2
    plugin = esm_route.Plugin(argv[1])
    world = int(argv[2], 16)
    grid_x, grid_y, radius = int(argv[3]), int(argv[4]), int(argv[5])
    grids = [
        (grid_x + dx, grid_y + dy)
        for dx in range(-radius, radius + 1)
        for dy in range(-radius, radius + 1)
    ]
    water = water_heights(plugin, world, grids)
    for grid in sorted(grids):
        payloads = land_vtxt.land_records(plugin, world, grid[0], grid[1])
        if not payloads:
            print(f"grid {grid}: no LAND")
            continue
        fields = land_edges.fields(payloads[0])
        heights = land_edges.decode_vhgt(fields.get(b"VHGT", b""))
        if heights is None:
            print(f"grid {grid}: no VHGT")
            continue
        entry = water.get(grid)
        lowest, highest = min(heights), max(heights)
        if entry is None or entry[0] is None or abs(entry[0]) > 1.0e7:
            print(
                f"grid {str(grid):>9}: no water plane; terrain {lowest:9.0f} .. {highest:9.0f}"
            )
            continue
        level = entry[0]
        below = sum(1 for value in heights if value < level)
        print(
            f"grid {str(grid):>9}: water {level:9.0f} ({'%08X' % (entry[1] or 0)}, {entry[2]!r}); "
            f"terrain {lowest:9.0f} .. {highest:9.0f}; "
            f"{below}/{len(heights)} samples below the water, "
            f"min |terrain-water| = {min(abs(value - level) for value in heights):.0f}, "
            f"water - min(terrain) = {level - lowest:.0f}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
