#!/usr/bin/env python3
"""Read-only plugin probe for the worldspace-transition demo research (task t02).

Derives, from a real Bethesda plugin, the data the engine and the converter need for a
seamless interior <-> exterior worldspace transition: worldspaces, cells, load doors and
their XTEL destinations.

Usage:
    python tools/research/esm_route.py <plugin> [substring ...]

The substrings select the records of interest (matched case-insensitively against EDID and
FULL). With no substring the whole WRLD list and the door graph are still reported.

Nothing is written anywhere; the report goes to stdout. The group/record/subrecord layout
mirrors crates/converter/src/esm/binary.rs so the numbers agree with the converter.

Never point this at a whole .ba2/.bsa; it reads .esm/.esp/.esl only.
"""

import collections
import mmap
import struct
import sys
import zlib

FLAG_COMPRESSED = 0x00040000
GRUP = b"GRUP"

# Group types that carry a cell context (crates/converter/src/esm/binary.rs:140-147).
CELL_GROUP_TYPES = (6, 8, 9, 10)
WORLDSPACE_GROUP_TYPE = 1


def u32(buf, pos):
    return struct.unpack_from("<I", buf, pos)[0]


def i32(buf, pos):
    return struct.unpack_from("<i", buf, pos)[0]


def f32s(buf, pos, count):
    return struct.unpack_from("<%df" % count, buf, pos)


def cstring(buf):
    return bytes(buf).split(b"\0")[0].decode("latin-1")


def subrecords(payload):
    """Yield (tag, data) for one record payload (crates/converter/src/esm/extractors.rs)."""
    pos = 0
    size = len(payload)
    while pos + 6 <= size:
        tag = payload[pos : pos + 4]
        length = int.from_bytes(payload[pos + 4 : pos + 6], "little")
        pos += 6
        if length > size - pos:
            return
        yield tag, payload[pos : pos + length]
        pos += length


def field(payload, tag):
    for candidate, data in subrecords(payload):
        if candidate == tag:
            return data
    return None


def fields(payload, tag):
    return [data for candidate, data in subrecords(payload) if candidate == tag]


def decoded(payload, flags):
    """Apply the compressed-record convention (crates/converter/src/esm/binary.rs:165-196)."""
    if not flags & FLAG_COMPRESSED or len(payload) < 4:
        return payload
    expected = u32(payload, 0)
    out = zlib.decompress(payload[4:])
    if len(out) != expected:
        raise ValueError("decompressed size mismatch")
    return out


def walk(buf, start, end, cell_ctx, world_ctx, visit, depth=0):
    pos = start
    while pos + 24 <= end:
        tag = buf[pos : pos + 4]
        size = u32(buf, pos + 4)
        if tag == GRUP:
            if size < 24 or pos + size > end:
                return
            label = u32(buf, pos + 8)
            group_type = i32(buf, pos + 12)
            next_cell = label if group_type in CELL_GROUP_TYPES else cell_ctx
            next_world = label if group_type == WORLDSPACE_GROUP_TYPE else world_ctx
            walk(buf, pos + 24, pos + size, next_cell, next_world, visit, depth + 1)
            pos += size
            continue
        flags = u32(buf, pos + 8)
        form_id = u32(buf, pos + 12)
        if pos + 24 + size > end:
            return
        visit(tag, form_id, flags, buf[pos + 24 : pos + 24 + size], cell_ctx, world_ctx)
        pos += 24 + size


class Plugin:
    def __init__(self, path):
        self.path = path
        self.handle = open(path, "rb")
        self.buf = mmap.mmap(self.handle.fileno(), 0, access=mmap.ACCESS_READ)

    def masters(self):
        size = u32(self.buf, 4)
        names = []
        for tag, data in subrecords(self.buf[24 : 24 + size]):
            if tag == b"MAST":
                names.append(cstring(data))
        return names

    def index(self):
        """Pass 1: worldspaces, cells, base objects. Returns a dict of indexes."""
        index = {
            "worlds": {},
            "cells": {},
            "doors": {},
            "models": {},
            "land_cells": set(),
        }
        systems = {b"STAT", b"MSTT", b"FURN", b"ACTI", b"FLOR", b"DOOR", b"CONT", b"TREE", b"IDLM"}

        def visit(tag, form_id, flags, payload, cell_ctx, world_ctx):
            if flags & FLAG_COMPRESSED:
                try:
                    payload = decoded(payload, flags)
                except (ValueError, zlib.error):
                    return
            if tag == b"WRLD":
                index["worlds"][form_id] = {
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "full": cstring(field(payload, b"FULL") or b""),
                    "parent": (u32(field(payload, b"WNAM"), 0) if field(payload, b"WNAM") else None),
                    "flags": flags,
                }
            elif tag == b"CELL":
                xclc = field(payload, b"XCLC")
                data = field(payload, b"DATA")
                index["cells"][form_id] = {
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "full": cstring(field(payload, b"FULL") or b""),
                    "grid": ((i32(xclc, 0), i32(xclc, 4)) if xclc and len(xclc) >= 8 else None),
                    "interior": bool(data and data[0] & 0x01),
                    "world": world_ctx,
                    "flags": flags,
                }
            elif tag in systems:
                index["models"][form_id] = {
                    "type": tag.decode("ascii"),
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "full": cstring(field(payload, b"FULL") or b""),
                    "model": (cstring(field(payload, b"MODL")).lower() if field(payload, b"MODL") else None),
                }
                if tag == b"DOOR":
                    index["doors"][form_id] = index["models"][form_id]
            elif tag == b"LAND":
                if cell_ctx is not None:
                    index["land_cells"].add(cell_ctx)

        walk(self.buf, u32(self.buf, 4) + 24, len(self.buf), None, None, visit)
        return index

    def references(self, index, focus_cells, focus_worlds, needles):
        """Pass 2: reference counts, focused cell contents, and every door reference."""
        counts = collections.Counter()
        world_counts = collections.Counter()
        unique = collections.defaultdict(set)
        door_refs = []
        target_refs = collections.defaultdict(list)
        meshes = collections.defaultdict(set)
        models = index["models"]

        def visit(tag, form_id, flags, payload, cell_ctx, world_ctx):
            if tag not in (b"REFR", b"ACHR", b"ACRE"):
                return
            if flags & FLAG_COMPRESSED:
                try:
                    payload = decoded(payload, flags)
                except (ValueError, zlib.error):
                    return
            base = field(payload, b"NAME")
            base_id = u32(base, 0) if base and len(base) >= 4 else 0
            if cell_ctx is not None:
                counts[cell_ctx] += 1
                if cell_ctx in focus_cells:
                    unique[cell_ctx].add(base_id)
            if world_ctx is not None:
                world_counts[world_ctx] += 1
                if world_ctx in focus_worlds and base_id in models:
                    model = models[base_id]["model"]
                    if model:
                        meshes[world_ctx].add(model)
            data = field(payload, b"DATA")
            pos = f32s(data, 0, 3) if data and len(data) >= 12 else None
            rot = f32s(data, 12, 3) if data and len(data) >= 24 else None
            xtel = field(payload, b"XTEL")
            if base_id in index["doors"]:
                door_refs.append(
                    {
                        "ref": form_id,
                        "door": base_id,
                        "cell": cell_ctx,
                        "world": world_ctx,
                        "pos": pos,
                        "rot": rot,
                        "xtel": describe_xtel(xtel),
                    }
                )
            if cell_ctx in focus_cells:
                target_refs[cell_ctx].append((form_id, base_id, pos, describe_xtel(xtel)))

        walk(self.buf, u32(self.buf, 4) + 24, len(self.buf), None, None, visit)
        return counts, world_counts, unique, door_refs, target_refs, meshes


def describe_xtel(data):
    """XTEL: destination door FormID, position, rotation (TES5 'Teleport' subrecord)."""
    if not data or len(data) < 16:
        return None
    out = {"door": u32(data, 0), "pos": f32s(data, 4, 3)}
    if len(data) >= 28:
        out["rot"] = f32s(data, 16, 3)
    return out


def fmt_form(value):
    return "%08X" % (value or 0)


def match(text, needles):
    lowered = text.lower()
    return any(needle in lowered for needle in needles)


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    plugin = Plugin(argv[1])
    needles = [value.lower() for value in argv[2:]]
    index = plugin.index()

    print("PLUGIN %s" % plugin.path)
    print("MASTERS %s" % ", ".join(plugin.masters()))

    worlds = index["worlds"]
    cells = index["cells"]
    doors = index["doors"]
    models = index["models"]

    print("\n== WRLD (%d) ==" % len(worlds))
    for form_id in sorted(worlds, key=lambda value: worlds[value]["edid"]):
        info = worlds[form_id]
        print(
            "  %s %-32s full=%-24s parent=%s"
            % (fmt_form(form_id), info["edid"], info["full"], fmt_form(info["parent"]))
        )

    if needles:
        print("\n== CELLS matching %s ==" % needles)
        focus_cells = set()
        named_cells = set()
        for form_id in sorted(cells, key=lambda value: (cells[value]["edid"] or "", value)):
            info = cells[form_id]
            if not (match(info["edid"], needles) or match(info["full"], needles)):
                continue
            focus_cells.add(form_id)
            named_cells.add(form_id)
            print(
                "  cell %s edid=%-28s full=%-30s interior=%s grid=%s world=%s flags=0x%X"
                % (
                    fmt_form(form_id),
                    info["edid"],
                    info["full"],
                    info["interior"],
                    info["grid"],
                    fmt_form(info["world"]),
                    info["flags"],
                )
            )
        focus_worlds = {
            form_id
            for form_id, info in worlds.items()
            if match(info["edid"], needles) or match(info["full"], needles)
        }
        if focus_worlds:
            print("\n  worldspaces matched: %s" % ", ".join(sorted(fmt_form(f) for f in focus_worlds)))
            for world_id in sorted(focus_worlds):
                owned = [f for f, info in cells.items() if info["world"] == world_id]
                land = [f for f in owned if f in index["land_cells"]]
                print(
                    "  %s %s: %d cells, %d with LAND"
                    % (fmt_form(world_id), worlds[world_id]["edid"], len(owned), len(land))
                )
                focus_cells.update(owned)
    else:
        focus_cells = set()
        focus_worlds = set()
        named_cells = set()

    counts, world_counts, unique, door_refs, target_refs, world_meshes = plugin.references(
        index, focus_cells, focus_worlds, needles
    )

    by_ref = {entry["ref"]: entry for entry in door_refs}

    def label(cell_id, world_id):
        if cell_id is None:
            return "<no cell>"
        info = cells.get(cell_id)
        if info is None:
            return fmt_form(cell_id)
        world = info["world"]
        world_name = worlds[world]["edid"] if world in worlds else "interior"
        return "%s %s [%s%s]" % (
            fmt_form(cell_id),
            info["edid"] or info["full"] or "?",
            world_name,
            " grid=%s" % (info["grid"],) if info["grid"] else "",
        )

    def route_line(entry):
        destination = entry["xtel"]
        target = " -> (no XTEL)"
        if destination and destination["door"] in by_ref:
            other = by_ref[destination["door"]]
            target = " -> ref %s %s @ %s pos=%s rot=%s" % (
                fmt_form(destination["door"]),
                doors[other["door"]]["edid"],
                label(other["cell"], other["world"]),
                destination["pos"],
                destination.get("rot"),
            )
        elif destination:
            target = " -> ref %s (base %s, outside this plugin)" % (
                fmt_form(destination["door"]),
                fmt_form(by_ref.get(destination["door"], {}).get("door", 0)),
            )
        return "  ref %s %-38s @ %-52s pos=%s%s" % (
            fmt_form(entry["ref"]),
            doors[entry["door"]]["edid"],
            label(entry["cell"], entry["world"]),
            entry["pos"],
            target,
        )

    destinations = {
        entry["xtel"]["door"]
        for entry in door_refs
        if entry["xtel"] and entry["cell"] in focus_cells
    }
    relevant = [
        entry
        for entry in door_refs
        if entry["cell"] in focus_cells or entry["ref"] in destinations
    ]
    print(
        "\n== LOAD DOORS ON OR INTO THE SELECTED CELLS (%d of %d door references) =="
        % (len(relevant), len(door_refs))
    )
    for entry in sorted(relevant, key=lambda item: (item["cell"] or 0, item["ref"])):
        print(route_line(entry))
    print(
        "\n== ALL DOOR REFERENCES: %d; with XTEL: %d; unique door base objects: %d =="
        % (
            len(door_refs),
            sum(1 for entry in door_refs if entry["xtel"]),
            len({entry["door"] for entry in door_refs}),
        )
    )

    if focus_cells:
        print("\n== FOCUS CELL CONTENTS ==")
        refs_by_id = {entry["ref"]: entry for entry in door_refs}
        for cell_id in sorted(
            named_cells, key=lambda value: (cells[value]["edid"] or "", value)
        ):
            info = cells[cell_id]
            total = counts.get(cell_id, 0)
            print(
                "  cell %s %-30s interior=%-5s refs=%-5d unique_bases=%-4d land=%s"
                % (
                    fmt_form(cell_id),
                    info["edid"] or info["full"],
                    info["interior"],
                    total,
                    len(unique.get(cell_id, ())),
                    cell_id in index["land_cells"],
                )
            )
        grouped = collections.defaultdict(lambda: [0, 0, 0])
        for cell_id in focus_cells - named_cells:
            world_id = cells[cell_id]["world"]
            grouped[world_id][0] += 1
            grouped[world_id][1] += counts.get(cell_id, 0)
            grouped[world_id][2] += len(unique.get(cell_id, ()))
        for world_id, (cells_count, refs_count, unique_count) in sorted(
            grouped.items(), key=lambda item: -(item[1][1])
        ):
            print(
                "  + %d unnamed cells of %s: %d references, %d (cell,base) pairs"
                % (
                    cells_count,
                    fmt_form(world_id) if world_id is not None else "<none>",
                    refs_count,
                    unique_count,
                )
            )
        meshes = set()
        types = collections.Counter()
        for cell_id in focus_cells:
            for _ref, base_id, _pos, _xtel in target_refs.get(cell_id, ()):
                entry = models.get(base_id)
                if entry is None:
                    continue
                types[entry["type"]] += 1
                if entry["model"]:
                    meshes.add(entry["model"])
                if base_id in refs_by_id:
                    pass
        print(
            "\n  base-object types over the focus cells: %s"
            % ", ".join("%s=%d" % (key, value) for key, value in sorted(types.items()))
        )
        print("  distinct mesh paths (MODL) reachable from the focus cells: %d" % len(meshes))
        missing = sorted(
            {entry["type"] for entry in models.values() if entry["type"] in ("STAT", "MSTT", "FURN")}
        )
        print("  base record types carried by the schema: %s" % ", ".join(missing))

    if focus_worlds:
        print("\n== FOCUS WORLDSPACE REFERENCE TOTALS ==")
        for world_id in sorted(focus_worlds):
            print(
                "  %s %s: %d references, %d distinct mesh paths (MODL)"
                % (
                    fmt_form(world_id),
                    worlds[world_id]["edid"],
                    world_counts.get(world_id, 0),
                    len(world_meshes.get(world_id, ())),
                )
            )
            for mesh in sorted(world_meshes.get(world_id, ()))[:0]:
                print("    %s" % mesh)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
