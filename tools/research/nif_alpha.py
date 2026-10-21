#!/usr/bin/env python3
"""Read a Skyrim NIF's NiAlphaProperty blocks and which shape uses each one.

The converter maps a NiAlphaProperty to glTF `alphaMode`/`alphaCutoff`: an enabled alpha
test becomes MASK with cutoff = threshold / 255, otherwise an enabled alpha blend becomes
BLEND, otherwise OPAQUE (crates/converter/src/material.rs:975). The blend factors stored in
the property (source/destination blend mode, mask 0x001E / 0x01E0) never reach the glTF, so
this script is the only place they can be read.

Usage:
    python tools/research/nif_alpha.py <nif...>            # one or more NIF paths
    python tools/research/nif_alpha.py --shapes <nif>      # also list shape blocks

The header layout follows vendor/project-wormhole-nif/src/nif_header.rs: after the version
quad and user/bethesda versions come four SizedString8 header strings, the block type table
(u16 count, each a SizedString32), a u16 block type index per block, a u32 block size per
block, then a string table and the group list. Block payloads follow in index order, so a
block's offset is the running sum of the sizes before it.
"""

import struct
import sys

ALPHA_SRC = {
    0: "ONE",
    1: "ZERO",
    2: "SRC_COLOR",
    3: "INV_SRC_COLOR",
    4: "DEST_COLOR",
    5: "INV_DEST_COLOR",
    6: "SRC_ALPHA",
    7: "INV_SRC_ALPHA",
    8: "DEST_ALPHA",
    9: "INV_DEST_ALPHA",
    10: "SRC_ALPHA_SATURATE",
}

TEST_FUNC = {
    0: "NEVER",
    1: "LESS",
    2: "EQUAL",
    3: "LESSEQUAL",
    4: "GREATER",
    5: "NOTEQUAL",
    6: "GREATEQUAL",
    7: "ALWAYS",
}

SHAPE_TYPES = ("BSTriShape", "BSDynamicTriShape", "BSLODTriShape", "NiTriShape", "NiTriStrips")


class Reader:
    def __init__(self, data):
        self.data = data
        self.offset = 0

    def u8(self):
        value = self.data[self.offset]
        self.offset += 1
        return value

    def u16(self):
        value = struct.unpack_from("<H", self.data, self.offset)[0]
        self.offset += 2
        return value

    def u32(self):
        value = struct.unpack_from("<I", self.data, self.offset)[0]
        self.offset += 4
        return value

    def string32(self):
        length = self.u32()
        text = self.data[self.offset : self.offset + length]
        self.offset += length
        return text.decode("latin-1").rstrip("\x00")

    def string8(self):
        length = self.u8()
        text = self.data[self.offset : self.offset + length]
        self.offset += length
        return text.decode("latin-1").rstrip("\x00")


def parse_header(data, export_strings=3):
    reader = Reader(data)
    end = data.find(b"\n")
    description = data[: end + 1].decode("latin-1") if end >= 0 else ""
    reader.offset = end + 1
    version = reader.u32()
    reader.u8()  # endianness
    user_version = reader.u32()
    block_count = reader.u32()
    bethesda_version = reader.u32()
    header_strings = [reader.string8() for _ in range(export_strings)]
    type_count = reader.u16()
    block_types = [reader.string32() for _ in range(type_count)]
    type_index = [reader.u16() for _ in range(block_count)]
    block_size = [reader.u32() for _ in range(block_count)]
    string_count = reader.u32()
    reader.u32()  # string max length
    strings = [reader.string32() for _ in range(string_count)]
    group_count = reader.u32()
    reader.offset += 4 * group_count
    offset = reader.offset
    blocks = []
    for index in range(block_count):
        size = block_size[index]
        blocks.append(
            {
                "index": index,
                "type": block_types[type_index[index]] if type_index[index] < len(block_types) else "?",
                "offset": offset,
                "size": size,
                "data": data[offset : offset + size],
            }
        )
        offset += size
    valid = (
        0 < type_count <= 64
        and all(
            name and len(name) <= 64 and all(32 <= ord(char) < 127 for char in name)
            for name in block_types
        )
        and (not blocks or blocks[-1]["offset"] + blocks[-1]["size"] <= len(data))
        and all(block["size"] >= 0 for block in blocks)
    )
    return {
        "description": description,
        "valid": valid,
        "export_strings": export_strings,
        "version": ".".join(str((version >> shift) & 0xFF) for shift in (24, 16, 8, 0)),
        "user_version": user_version,
        "bethesda_version": bethesda_version,
        "blocks": blocks,
        "strings": strings,
        "header_strings": header_strings,
    }


def decode_alpha(flags, threshold):
    blend = flags & 0x0001 != 0
    source = (flags >> 1) & 0x0F
    destination = (flags >> 5) & 0x0F
    test = flags & 0x0200 != 0
    test_func = (flags >> 10) & 0x07
    no_sorter = flags & 0x2000 != 0
    clone_unique = flags & 0x4000 != 0
    editor_threshold = flags & 0x8000 != 0
    return (
        f"flags=0x{flags:04X} blend={blend}"
        + (f" src={ALPHA_SRC.get(source, source)} dst={ALPHA_SRC.get(destination, destination)}" if blend else "")
        + f" test={test}"
        + (f" func={TEST_FUNC.get(test_func, test_func)}" if test else "")
        + f" threshold={threshold} ({threshold / 255:.3f})"
        + (" no_sorter" if no_sorter else "")
        + (" clone_unique" if clone_unique else "")
        + (" editor_threshold" if editor_threshold else "")
    )


def read_alpha_tail(data):
    return struct.unpack_from("<H", data, len(data) - 3)[0], data[-1]


def shape_alpha_references(header, block, alpha_indices):
    """Every u32 in the shape block that points at a NiAlphaProperty block."""
    hits = []
    data = block["data"]
    for offset in range(0, len(data) - 3, 1):
        value = struct.unpack_from("<I", data, offset)[0]
        if value in alpha_indices:
            hits.append((offset, value))
    return hits


def report(path, show_shapes=False):
    with open(path, "rb") as handle:
        data = handle.read()
    header = None
    error = None
    for export_strings in (3, 4):
        try:
            candidate = parse_header(data, export_strings)
        except Exception as candidate_error:  # noqa: BLE001 - a malformed file is a result
            error = candidate_error
            continue
        if candidate["valid"]:
            header = candidate
            break
        error = ValueError("block type table did not parse as text")
    if header is None:
        print(f"== {path}\n   header parse failed: {error}")
        return
    print(
        f"== {path}\n   {header['description']} version {header['version']} "
        f"user {header['user_version']} bethesda {header['bethesda_version']} "
        f"blocks {len(header['blocks'])}"
    )
    blocks = header["blocks"]
    alpha_indices = {block["index"] for block in blocks if block["type"] == "NiAlphaProperty"}
    for block in blocks:
        if block["type"] != "NiAlphaProperty":
            continue
        # NiAlphaProperty inherits NiObjectNET (name, extra data, controller) and then
        # appends flags (u16) and threshold (u8), so its own fields are the block tail.
        if len(block["data"]) < 3:
            print(f"   alpha[{block['index']}] truncated ({block['size']} bytes)")
            continue
        flags, threshold = read_alpha_tail(block["data"])
        print(f"   alpha[{block['index']}] {decode_alpha(flags, threshold)}")
    for block in blocks:
        if not block["type"].startswith(SHAPE_TYPES):
            continue
        hits = shape_alpha_references(header, block, alpha_indices)
        if not hits:
            continue
        name = ""
        # NiObjectNET name is the first u32 of the shape: an index into the string table.
        if len(block["data"]) >= 4:
            index = struct.unpack_from("<I", block["data"], 0)[0]
            if index < len(header["strings"]):
                name = header["strings"][index]
        print(
            f"   shape[{block['index']}] {block['type']} name={name!r} "
            f"alpha refs {[(offset, value) for offset, value in hits]}"
        )
        if show_shapes:
            for offset, value in hits:
                flags, threshold = read_alpha_tail(blocks[value]["data"])
                print(f"        -> alpha[{value}] {decode_alpha(flags, threshold)}")


def hexdump(path, count=128):
    with open(path, "rb") as handle:
        data = handle.read(count)
    for start in range(0, len(data), 16):
        row = data[start : start + 16]
        text = "".join(chr(byte) if 32 <= byte < 127 else "." for byte in row)
        print(f"   {start:04X}  {' '.join(f'{byte:02X}' for byte in row):<47}  {text}")


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    show_shapes = "--shapes" in argv
    if "--head" in argv:
        for path in [arg for arg in argv[1:] if not arg.startswith("--")]:
            print(f"== {path}")
            hexdump(path)
        return 0
    for path in [arg for arg in argv[1:] if not arg.startswith("--")]:
        report(path, show_shapes)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
