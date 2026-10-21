#!/usr/bin/env python3
"""Scan converted GLBs for material settings that decide the render pass they land in.

A GLB's JSON chunk carries every material the converter published: `alphaMode`,
`alphaCutoff`, `doubleSided`, `pbrMetallicRoughness.baseColorFactor` (whose alpha is the
NIF shader's alpha) and the `extras.openSkyrim` block with the source NIF block numbers.

Usage:
    python tools/research/glb_materials.py summary [meshes_dir]
    python tools/research/glb_materials.py list <mode> [meshes_dir]     # OPAQUE/MASK/BLEND
    python tools/research/glb_materials.py model <substring> [meshes_dir]
    python tools/research/glb_materials.py hist [meshes_dir]            # alpha value histogram

`meshes_dir` defaults to C:/Modding/SkyrimConverted/meshes.
"""

import collections
import json
import os
import struct
import sys

DEFAULT_ROOT = "C:/Modding/SkyrimConverted/meshes"


def read_json_chunk(path):
    with open(path, "rb") as handle:
        header = handle.read(12)
        if len(header) < 12 or header[:4] != b"glTF":
            return None
        chunk = handle.read(8)
        length, kind = struct.unpack("<II", chunk)
        if kind != 0x4E4F534A:  # 'JSON'
            return None
        return json.loads(handle.read(length))


def material_info(material):
    pbr = material.get("pbrMetallicRoughness", {})
    factor = pbr.get("baseColorFactor", [1.0, 1.0, 1.0, 1.0])
    return {
        "name": material.get("name"),
        "alphaMode": material.get("alphaMode", "OPAQUE"),
        "alphaCutoff": material.get("alphaCutoff"),
        "doubleSided": bool(material.get("doubleSided", False)),
        "alpha": factor[3] if len(factor) > 3 else 1.0,
        "baseColor": factor[:3],
        "extras": material.get("extras", {}),
        "extensions": material.get("extensions", {}),
        "hasBaseColorTexture": "baseColorTexture" in pbr,
    }


def models(root):
    """Yield (model_path_relative, [material_info, ...], gltf_json)."""
    for directory, _, files in os.walk(root):
        for name in sorted(files):
            if not name.lower().endswith(".glb"):
                continue
            path = os.path.join(directory, name)
            document = read_json_chunk(path)
            if document is None:
                continue
            relative = os.path.relpath(path, root).replace("\\", "/")
            yield relative, [material_info(m) for m in document.get("materials", [])], document


def summary(root):
    total_models = 0
    models_with_material = 0
    modes = collections.Counter()
    alpha_lt_one_blend = 0
    per_model_modes = collections.Counter()
    block_numbers = collections.Counter()
    for _, materials, _ in models(root):
        total_models += 1
        if not materials:
            continue
        models_with_material += 1
        model_modes = {m["alphaMode"] for m in materials}
        for mode in model_modes:
            per_model_modes[mode] += 1
        for material in materials:
            modes[material["alphaMode"]] += 1
            if material["alphaMode"] == "BLEND":
                if material["alpha"] < 1.0:
                    alpha_lt_one_blend += 1
                if material["name"]:
                    block_numbers[material["name"]] += 1
    print(f"glb files:              {total_models}")
    print(f"  with materials:       {models_with_material}")
    print(f"materials by alphaMode: {dict(modes)}")
    print(f"  BLEND with base alpha < 1: {alpha_lt_one_blend}")
    print(f"models containing each mode: {dict(per_model_modes)}")


def list_mode(root, mode):
    rows = []
    for relative, materials, _ in models(root):
        hits = [m for m in materials if m["alphaMode"] == mode.upper()]
        if hits:
            rows.append((relative, hits))
    rows.sort()
    print(f"{len(rows)} models with a {mode.upper()} material:")
    for relative, hits in rows:
        detail = ", ".join(
            f"{m['name']}@a={m['alpha']:.3f}"
            + (f" cutoff={m['alphaCutoff']}" if m["alphaCutoff"] is not None else "")
            + (" ds" if m["doubleSided"] else "")
            for m in hits[:4]
        )
        print(f"  {relative}: {len(hits)}  {detail}")


def show_model(root, substring):
    needle = substring.lower()
    for relative, materials, document in models(root):
        if needle not in relative.lower():
            continue
        print(f"== {relative}")
        meshes = document.get("meshes", [])
        print(f"   meshes: {len(meshes)}  nodes: {len(document.get('nodes', []))}")
        for index, material in enumerate(materials):
            extras = json.dumps(material["extras"])
            extensions = json.dumps(material["extensions"])
            print(
                f"   material[{index}] {material['name']!r} {material['alphaMode']} "
                f"alpha={material['alpha']:.4f} cutoff={material['alphaCutoff']} "
                f"doubleSided={material['doubleSided']} tex={material['hasBaseColorTexture']}"
            )
            print(f"      extras={extras}")
            if extensions != "{}":
                print(f"      extensions={extensions}")


def detail(root, substring):
    """Textures and vertex attributes of matching models (what a transparent pass can use)."""
    needle = substring.lower()
    for relative, _, document in models(root):
        if needle not in relative.lower():
            continue
        print(f"== {relative}")
        images = document.get("images", [])
        textures = document.get("textures", [])
        for index, material in enumerate(document.get("materials", [])):
            pbr = material.get("pbrMetallicRoughness", {})
            slots = []
            for label, key in (
                ("baseColor", "baseColorTexture"),
                ("normal", "normalTexture"),
                ("emissive", "emissiveTexture"),
            ):
                if key in pbr or key in material:
                    info = pbr.get(key) or material.get(key)
                    texture_index = info.get("index")
                    uri = "?"
                    if texture_index is not None and texture_index < len(textures):
                        source = textures[texture_index].get("source")
                        if source is not None and source < len(images):
                            uri = images[source].get("uri", "?")
                    slots.append(f"{label}={uri}")
            print(f"   material[{index}] {material.get('name')!r}: " + " ".join(slots))
        for mesh in document.get("meshes", []):
            for primitive in mesh.get("primitives", []):
                attributes = ",".join(sorted(primitive.get("attributes", {})))
                print(f"   mesh {mesh.get('name')!r} attributes {attributes}")


def raw(root, substring, limit=3):
    needle = substring.lower()
    for relative, _, document in models(root):
        if needle not in relative.lower():
            continue
        print(f"== {relative}")
        print(json.dumps(document.get("materials", [])[:limit], indent=2)[:6000])
        print("images:", json.dumps(document.get("images", [])[:6]))
        print("textures:", json.dumps(document.get("textures", [])[:6]))
        print("extensionsUsed:", document.get("extensionsUsed"))


def untextured(root, list_models=0):
    """Materials with no base colour texture: they render as a flat factor colour."""
    counts = collections.Counter()
    families_by_kind = collections.defaultdict(collections.Counter)
    examples = collections.defaultdict(list)
    for relative, _, document in models(root):
        for material in document.get("materials", []):
            info = material_info(material)
            kind = (
                f"{info['alphaMode']} {'textured' if info['hasBaseColorTexture'] else 'untextured'}"
                f" alpha={info['alpha']:.2f}"
            )
            counts[kind] += 1
            if not info["hasBaseColorTexture"]:
                folder = relative.split("/")[0]
                families_by_kind[kind][folder] += 1
                if len(examples[kind]) < list_models:
                    examples[kind].append(f"{relative} {info['name']!r}")
    for kind, count in counts.most_common():
        print(f"  {count:7d}  {kind}")
        if families_by_kind[kind]:
            print(f"           folders: {dict(families_by_kind[kind].most_common(6))}")
        for example in examples[kind]:
            print(f"           {example}")


def families(root, needle="", limit=60):
    """Where the BLEND materials live: counts per model and per directory of origin."""
    directories = collections.Counter()
    models_blend = []
    for relative, materials, _ in models(root):
        if needle and needle.lower() not in relative.lower():
            continue
        blends = [m for m in materials if m["alphaMode"] == "BLEND"]
        if not blends:
            continue
        models_blend.append((relative, blends))
        directories[os.path.dirname(relative)] += 1
    print(f"{len(models_blend)} models with a BLEND material (filter {needle!r})")
    print("directories, most first:")
    for directory, count in directories.most_common(40):
        print(f"  {count:5d}  {directory}")
    print("models, path order (first {limit})\n".replace("\n", ""))
    for relative, blends in models_blend[:limit]:
        detail = ", ".join(
            f"{m['name']}@a={m['alpha']:.3f}:{m['alphaMode']}"
            + (f"={m['alphaCutoff']}" if m["alphaCutoff"] is not None else "")
            for m in blends[:3]
        )
        print(f"  {relative}  ({len(blends)})  {detail}")


def histogram(root):
    values = collections.Counter()
    for _, materials, _ in models(root):
        for material in materials:
            values[round(material["alpha"], 3)] += 1
    for value, count in sorted(values.items(), key=lambda item: -item[1])[:30]:
        print(f"  alpha {value:6.3f}: {count}")


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    command = argv[1]
    logical = [arg for arg in argv[2:] if not os.path.isdir(arg)]
    directories = [arg for arg in argv[2:] if os.path.isdir(arg)]
    root = directories[0] if directories else DEFAULT_ROOT
    if command == "summary":
        summary(root)
    elif command == "list":
        list_mode(root, logical[0])
    elif command == "model":
        show_model(root, logical[0])
    elif command == "hist":
        histogram(root)
    elif command == "raw":
        raw(root, logical[0], int(logical[1]) if len(logical) > 1 else 3)
    elif command == "untextured":
        untextured(root, int(logical[0]) if logical else 0)
    elif command == "detail":
        detail(root, logical[0])
    elif command == "families":
        families(
            root,
            logical[0] if logical else "",
            int(logical[1]) if len(logical) > 1 else 60,
        )
    else:
        print(__doc__)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
