#!/usr/bin/env python3
"""Inventory every NiAlphaProperty in the converted NIF set by its flag word.

The converter reads only three things from the flag word - blend enable (0x0001), test
enable (0x0200) and the threshold byte (crates/converter/src/material.rs:975) - and drops
the source/destination blend factors (0x001E / 0x01E0). This counts how often each
combination occurs, so the size of the dropped information is known.

Usage:
    python tools/research/alpha_blend_census.py [nif_root]
"""

import collections
import os
import struct
import sys

import nif_alpha
import nif_alpha as parser

DEFAULT_ROOT = "C:/Modding/SkyrimConverted/vfs/meshes"


def scan(path, combos, thresholds, property_count):
    with open(path, "rb") as handle:
        data = handle.read(1 << 20)
    header = None
    for export_strings in (3, 4):
        try:
            candidate = nif_alpha.parse_header(data, export_strings)
        except Exception:  # noqa: BLE001
            continue
        if candidate["valid"]:
            header = candidate
            break
    if header is None:
        return 0
    count = 0
    for block in header["blocks"]:
        if block["type"] != "NiAlphaProperty" or len(block["data"]) < 3:
            continue
        flags, threshold = nif_alpha.read_alpha_tail(block["data"])
        blend = flags & 0x0001 != 0
        source = (flags >> 1) & 0x0F
        destination = (flags >> 5) & 0x0F
        test = flags & 0x0200 != 0
        if not blend and not test:
            combos[("off", "off", 0, 0)] += 1
        if blend:
            combos[("blend", f"src={nif_alpha.ALPHA_SRC.get(source, source)}",
                    f"dst={nif_alpha.ALPHA_SRC.get(destination, destination)}",
                    0)] += 1
        if test:
            thresholds[threshold] += 1
        count += 1
    property_count[0] += count
    return count


def main(argv):
    root = argv[1] if len(argv) > 1 and os.path.isdir(argv[1]) else DEFAULT_ROOT
    combos = collections.Counter()
    thresholds = collections.Counter()
    files = 0
    property_count = [0]
    for directory, _, names in os.walk(root):
        for name in names:
            if not name.lower().endswith(".nif"):
                continue
            try:
                scan(os.path.join(directory, name), combos, thresholds, property_count)
                files += 1
            except (OSError, struct.error, ValueError):
                continue
    print(f"NIFs scanned: {files}, NiAlphaProperty blocks: {property_count[0]}")
    print("flag combinations:")
    for key, count in combos.most_common(20):
        print(f"  {count:7d}  {key}")
    print("alpha-test thresholds (top 15):")
    for value, count in thresholds.most_common(15):
        print(f"  {count:7d}  {value} ({value / 255:.3f})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
