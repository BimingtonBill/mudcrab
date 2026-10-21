#!/usr/bin/env python3
"""Dump the LAND record of an exterior cell from a Bethesda plugin (read-only).

Answers: what does Skyrim's VTXT really contain (entry size, position encoding,
opacity values), and how many samples per quadrant/layer?

Usage:
    python tools/research/land_vtxt.py <plugin> <world_form_id_hex> <grid_x> <grid_y>

Example:
    python tools/research/land_vtxt.py "C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/Skyrim.esm" 3C 0 0

Nothing is written; the report goes to stdout. Reads .esm/.esp/.esl only.
"""

import collections
import struct
import sys

sys.path.insert(0, __file__.rsplit("\\", 1)[0])

import esm_route  # noqa: E402  (tools/research is not a package)


def subrecords(payload):
    return list(esm_route.subrecords(payload))


def land_records(plugin, world_id, grid_x, grid_y):
    """Yield the LAND payload(s) whose group cell has the requested XCLC grid."""
    found = []
    cells = {}
    state = {"cell": None, "world": None, "want": None}

    def visit(tag, form_id, flags, payload, cell_ctx, world_ctx):
        if flags & esm_route.FLAG_COMPRESSED:
            try:
                payload = esm_route.decoded(payload, flags)
            except Exception:
                return
        if tag == b"CELL":
            xclc = esm_route.field(payload, b"XCLC")
            grid = (esm_route.i32(xclc, 0), esm_route.i32(xclc, 4)) if xclc and len(xclc) >= 8 else None
            state["want"] = grid == (grid_x, grid_y) and world_ctx == world_id
            cells[form_id] = grid
        elif tag == b"LAND" and state["want"]:
            found.append(payload)

    esm_route.walk(plugin.buf, esm_route.u32(plugin.buf, 4) + 24, len(plugin.buf), None, None, visit)
    return found


def decode_btxt(data):
    return (
        "%08X" % esm_route.u32(data, 0),
        data[4] if len(data) > 4 else None,
        struct.unpack_from("<h", data, 6)[0] if len(data) >= 8 else None,
    )


def main(argv):
    if len(argv) < 5:
        print(__doc__)
        return 2
    plugin = esm_route.Plugin(argv[1])
    world_id = int(argv[2], 16)
    grid_x, grid_y = int(argv[3]), int(argv[4])
    records = land_records(plugin, world_id, grid_x, grid_y)
    print("LAND records for world %08X grid (%d,%d): %d" % (world_id, grid_x, grid_y, len(records)))
    for payload in records:
        print("record payload %d bytes" % len(payload))
        current = None
        quad_counts = collections.Counter()
        for tag, data in subrecords(payload):
            name = tag.decode("latin-1")
            if name in ("BTXT", "ATXT"):
                form, quadrant, layer = decode_btxt(data)
                current = (name, quadrant, layer)
                print("  %s len=%d form=%s quadrant=%d layer=%d" % (name, len(data), form, quadrant, layer))
            elif name == "VTXT":
                entries = len(data) // 8
                positions = []
                unknown = collections.Counter()
                opacities = []
                for index in range(entries):
                    entry = data[index * 8 : index * 8 + 8]
                    positions.append(struct.unpack_from("<H", entry, 0)[0])
                    unknown[struct.unpack_from("<H", entry, 2)[0]] += 1
                    opacities.append(struct.unpack_from("<f", entry, 4)[0])
                below289 = [p for p in positions if p < 289]
                print(
                    "  VTXT len=%d entries=%d after %s: pos min=%d max=%d distinct=%d "
                    "below289=%d mod17>0=%d mod32>16=%d unknown_words=%s"
                    % (
                        len(data),
                        entries,
                        current,
                        min(positions),
                        max(positions),
                        len(set(positions)),
                        len(below289),
                        sum(1 for p in positions if p % 17 != 0),
                        sum(1 for p in positions if p % 32 > 16),
                        dict(list(unknown.items())[:4]),
                    )
                )
                print(
                    "    positions sorted (first 60): %s" % sorted(positions)[:60]
                )
                print(
                    "    opacity min=%.4f max=%.4f distinct=%d zeros=%d ones=%d sample=%s"
                    % (
                        min(opacities),
                        max(opacities),
                        len(set(opacities)),
                        sum(1 for o in opacities if o == 0.0),
                        sum(1 for o in opacities if o >= 0.999),
                        [round(o, 3) for o in opacities[:20]],
                    )
                )
                if current:
                    quad_counts[current] += entries
            else:
                if name != "XXXX":
                    current = current
                print("  %s len=%d" % (name, len(data)))
                if name not in ("VTXT",):
                    current = None if name not in ("BTXT", "ATXT") else current
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
