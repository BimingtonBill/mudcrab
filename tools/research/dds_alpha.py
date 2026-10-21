#!/usr/bin/env python3
"""Report whether a source DDS texture carries an alpha channel that a BLEND material uses.

A glTF material with `alphaMode: BLEND` and `baseColorFactor` alpha 1.0 is only visible as
transparent when the base colour texture's own alpha is below 1, so the source DDS decides
whether such a material looks solid or see-through.

Usage:
    python tools/research/dds_alpha.py <dds path or name substring> [...]
    python tools/research/dds_alpha.py --glb <model substring>     # textures of a converted model

Paths given as a substring are searched under C:/Modding/SkyrimConverted/vfs/textures.
"""

import os
import sys

from PIL import Image

VFS_TEXTURES = "C:/Modding/SkyrimConverted/vfs/textures"
MESH_ROOT = "C:/Modding/SkyrimConverted/meshes"


def find_dds(needle):
    if os.path.isfile(needle):
        return [needle]
    lowered = needle.lower().replace("\\", "/")
    matches = []
    for directory, _, files in os.walk(VFS_TEXTURES):
        for name in files:
            if not name.lower().endswith(".dds"):
                continue
            path = os.path.join(directory, name)
            if lowered in path.replace("\\", "/").lower():
                matches.append(path)
    return matches[:5]


def report(path):
    try:
        image = Image.open(path)
    except Exception as error:  # noqa: BLE001 - unreadable formats are a result
        print(f"  {path}: unreadable ({error})")
        return
    info = image.info
    mode = image.mode
    alpha_stats = ""
    if mode in ("RGBA", "LA", "PA"):
        alpha = image.getchannel("A")
        values = alpha.getdata()
        total = len(values)
        below = sum(1 for value in values if value < 250)
        alpha_stats = (
            f" alpha min={min(values)} max={max(values)} "
            f"mean={sum(values) / total:.1f} below250={below * 100.0 / total:.1f}%"
        )
    else:
        alpha_stats = " NO ALPHA CHANNEL"
    print(
        f"  {path}\n     mode={mode} size={image.size} "
        f"format={info.get('compression', '?')}{alpha_stats}"
    )


def glb_textures(substring):
    needle = substring.lower()
    for directory, _, files in os.walk(MESH_ROOT):
        for name in files:
            if not name.lower().endswith(".glb"):
                continue
            relative = os.path.relpath(os.path.join(directory, name), MESH_ROOT)
            if needle not in relative.replace("\\", "/").lower():
                continue
            sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
            import glb_materials  # noqa: PLC0415

            document = glb_materials.read_json_chunk(os.path.join(directory, name))
            if document is None:
                continue
            print(f"== {relative}")
            images = document.get("images", [])
            for index, material in enumerate(document.get("materials", [])):
                pbr = material.get("pbrMetallicRoughness", {})
                info = pbr.get("baseColorTexture")
                uri = "none"
                if info and info.get("index") is not None and info["index"] < len(images):
                    uri = images[info["index"]].get("uri", "?")
                print(
                    f"   material[{index}] {material.get('name')!r} "
                    f"{material.get('alphaMode', 'OPAQUE')} base={uri}"
                )
                converted = uri.replace("../../../", "")
                if converted.lower().startswith("textures/"):
                    converted = converted[len("textures/") :]
                source = os.path.join(
                    VFS_TEXTURES,
                    converted.replace(".opensky-srgb.ktx2", ".dds").replace(".ktx2", ".dds"),
                )
                if os.path.isfile(source):
                    report(source)


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    if argv[1] == "--glb":
        glb_textures(argv[2])
        return 0
    for needle in argv[1:]:
        matches = find_dds(needle)
        if not matches:
            print(f"  {needle}: no DDS matched")
        for path in matches:
            report(path)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
