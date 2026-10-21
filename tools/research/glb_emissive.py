#!/usr/bin/env python3
"""Find converted models whose GLB materials have a non-zero `emissiveFactor`.

Usage:
    python tools/research/glb_emissive.py [--meshes <dir>] [--limit 40] [--warm]

A GLB's JSON chunk lists every material with its `emissiveFactor`, which the engine's
`streaming::material` turns into a Bevy `StandardMaterial::emissive` - a surface that is bright
whatever the lighting is.  This scans the converted meshes and prints each model with its emissive
colours, brightest relative luminance first, so a surface that glows the wrong colour in a render
can be traced to a material rather than guessed at.  `--warm` keeps only the ones whose emissive is
warmer than neutral (red above blue), which is what an orange ceiling would be.

Read-only.
"""

import argparse
import json
import os
import struct
import sys

DEFAULT_MESHES = "C:/Modding/SkyrimConverted/meshes"
LUMA = (0.2126, 0.7152, 0.0722)


def glb_json(path):
    """The JSON chunk of a .glb, without loading the binary chunk."""
    with open(path, "rb") as handle:
        header = handle.read(12)
        if len(header) < 12 or header[:4] != b"glTF":
            return None
        offset = 12
        while offset < len(header) + os.path.getsize(path):
            chunk = handle.read(8)
            if len(chunk) < 8:
                return None
            length, kind = struct.unpack("<II", chunk)
            data = handle.read(length)
            if kind == 0x4E4F534A:  # 'JSON'
                return json.loads(data.decode("utf-8"))
            offset += 8 + length
            if length == 0:
                return None
    return None


def emissive_of(document):
    """(name, [r, g, b]) for every material that emits something."""
    found = []
    for material in document.get("materials", []):
        factor = material.get("emissiveFactor")
        if factor and any(value > 0.0 for value in factor):
            found.append((material.get("name", "?"), [float(value) for value in factor]))
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--meshes", default=DEFAULT_MESHES)
    parser.add_argument("--limit", type=int, default=40)
    parser.add_argument("--warm", action="store_true", help="only red-dominant emissives")
    args = parser.parse_args()

    rows = []
    for root, _, files in os.walk(args.meshes):
        for name in files:
            if not name.lower().endswith(".glb"):
                continue
            path = os.path.join(root, name)
            try:
                document = glb_json(path)
            except (OSError, ValueError):
                continue
            if not document:
                continue
            for material, factor in emissive_of(document):
                luminance = sum(weight * value for weight, value in zip(LUMA, factor))
                if luminance <= 0.0:
                    continue
                if args.warm and factor[0] <= factor[2]:
                    continue
                rows.append((luminance, os.path.relpath(path, args.meshes), material, factor))

    rows.sort(reverse=True)
    print(f"{len(rows)} emissive materials" + (" (red-dominant)" if args.warm else ""))
    print(f"{'luminance':>9}  {'model':<64} {'material':<28} rgb")
    for luminance, path, material, factor in rows[: args.limit]:
        print(
            f"{luminance:>9.3f}  {path[:64]:<64} {material[:28]:<28} "
            f"({factor[0]:.2f}, {factor[1]:.2f}, {factor[2]:.2f})"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
