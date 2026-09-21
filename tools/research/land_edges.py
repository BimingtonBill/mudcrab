#!/usr/bin/env python3
"""Compare the terrain fields of neighbouring cells at their shared edges.

A cell-boundary seam in the renderer can only come from a field that is discontinuous
across cells: VHGT (heights), VNML (normals) or VCLR (vertex colours). This prints the
worst difference per edge, so the source can be named.

Usage:
    python tools/research/land_edges.py <plugin> <world_hex> <grid_x> <grid_y>
"""

import struct
import sys

import esm_route  # noqa: E402
import land_vtxt  # noqa: E402

SIDE = 33


def decode_vhgt(bytes_):
    count = SIDE * SIDE
    if len(bytes_) < 4 + count:
        return None
    offset = struct.unpack_from("<f", bytes_, 0)[0] * 8.0
    deltas = bytes_[4 : 4 + count]
    heights = [0.0] * count
    row_origin = offset
    for row in range(SIDE):
        row_origin += struct.unpack_from("<b", deltas, row * SIDE)[0] * 8.0
        height = row_origin
        heights[row * SIDE] = height
        for column in range(1, SIDE):
            height += struct.unpack_from("<b", deltas, row * SIDE + column)[0] * 8.0
            heights[row * SIDE + column] = height
    return heights


def fields(payload):
    out = {}
    for tag, data in esm_route.subrecords(payload):
        if tag in (b"VHGT", b"VNML", b"VCLR", b"VHGT"):
            out[tag] = data
    return out


def main(argv):
    if len(argv) < 5:
        print(__doc__)
        return 2
    plugin = esm_route.Plugin(argv[1])
    world = int(argv[2], 16)
    grid_x, grid_y = int(argv[3]), int(argv[4])
    payloads = {}
    for dx, dy in ((0, 0), (1, 0), (0, 1), (-1, 0), (0, -1)):
        found = land_vtxt.land_records(plugin, world, grid_x + dx, grid_y + dy)
        if found:
            payloads[(dx, dy)] = fields(found[0])
    print(f"LAND payloads found for {sorted(payloads)}")
    centre = payloads.get((0, 0))
    if centre is None:
        print("no LAND for the centre cell")
        return 1
    heights = decode_vhgt(centre.get(b"VHGT", b""))
    for offset, name in (((1, 0), "east"), ((0, 1), "north"), ((-1, 0), "west"), ((0, -1), "south")):
        other = payloads.get(offset)
        if other is None:
            print(f"  {name}: neighbour LAND missing")
            continue
        other_heights = decode_vhgt(other.get(b"VHGT", b""))
        if heights and other_heights:
            if offset == (1, 0):
                mine = [heights[row * SIDE + SIDE - 1] for row in range(SIDE)]
                theirs = [other_heights[row * SIDE] for row in range(SIDE)]
            elif offset == (-1, 0):
                mine = [heights[row * SIDE] for row in range(SIDE)]
                theirs = [other_heights[row * SIDE + SIDE - 1] for row in range(SIDE)]
            elif offset == (0, 1):
                mine = heights[(SIDE - 1) * SIDE :]
                theirs = other_heights[:SIDE]
            else:
                mine = heights[:SIDE]
                theirs = other_heights[(SIDE - 1) * SIDE :]
            differences = [abs(a - b) for a, b in zip(mine, theirs)]
            print(
                f"  {name}: VHGT edge max |difference| {max(differences):.1f} units "
                f"(mean {sum(differences) / len(differences):.2f}), "
                f"{sum(1 for value in differences if value > 0.0)} of {len(differences)} differ"
            )
        mine_normals = centre.get(b"VNML", b"")
        their_normals = other.get(b"VNML", b"")
        if len(mine_normals) == SIDE * SIDE * 3 and len(their_normals) == SIDE * SIDE * 3:
            if offset == (1, 0):
                pairs = [
                    (mine_normals[(row * SIDE + SIDE - 1) * 3 + c], their_normals[row * SIDE * 3 + c])
                    for row in range(SIDE)
                    for c in range(3)
                ]
            elif offset == (-1, 0):
                pairs = [
                    (mine_normals[row * SIDE * 3 + c], their_normals[(row * SIDE + SIDE - 1) * 3 + c])
                    for row in range(SIDE)
                    for c in range(3)
                ]
            elif offset == (0, 1):
                pairs = [
                    (mine_normals[((SIDE - 1) * SIDE + column) * 3 + c], their_normals[column * 3 + c])
                    for column in range(SIDE)
                    for c in range(3)
                ]
            else:
                pairs = [
                    (mine_normals[column * 3 + c], their_normals[((SIDE - 1) * SIDE + column) * 3 + c])
                    for column in range(SIDE)
                    for c in range(3)
                ]
            differences = [abs((a if a < 128 else a - 256) - (b if b < 128 else b - 256)) for a, b in pairs]
            print(
                f"  {name}: VNML edge max |difference| {max(differences)} of 255 "
                f"(mean {sum(differences) / len(differences):.2f}), "
                f"{sum(1 for value in differences if value > 0)} of {len(differences)} components differ"
            )
        mine_colors = centre.get(b"VCLR", b"")
        their_colors = other.get(b"VCLR", b"")
        if len(mine_colors) == SIDE * SIDE * 3 and len(their_colors) == SIDE * SIDE * 3:
            if offset == (1, 0):
                pairs = [
                    (mine_colors[(row * SIDE + SIDE - 1) * 3 + c], their_colors[row * SIDE * 3 + c])
                    for row in range(SIDE)
                    for c in range(3)
                ]
            elif offset == (-1, 0):
                pairs = [
                    (mine_colors[row * SIDE * 3 + c], their_colors[(row * SIDE + SIDE - 1) * 3 + c])
                    for row in range(SIDE)
                    for c in range(3)
                ]
            elif offset == (0, 1):
                pairs = [
                    (mine_colors[((SIDE - 1) * SIDE + column) * 3 + c], their_colors[column * 3 + c])
                    for column in range(SIDE)
                    for c in range(3)
                ]
            else:
                pairs = [
                    (mine_colors[column * 3 + c], their_colors[((SIDE - 1) * SIDE + column) * 3 + c])
                    for column in range(SIDE)
                    for c in range(3)
                ]
            differences = [abs(a - b) for a, b in pairs]
            print(
                f"  {name}: VCLR edge max |difference| {max(differences)} of 255 "
                f"(mean {sum(differences) / len(differences):.2f}), "
                f"{sum(1 for value in differences if value > 0)} of {len(differences)} bytes differ"
            )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
