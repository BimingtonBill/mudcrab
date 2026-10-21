#!/usr/bin/env python3
"""Count why each converted material ended up BLEND or MASK, over the whole mesh set.

The converter decides alpha mode in `alpha_contract` (crates/converter/src/material.rs:975):
an enabled alpha test is MASK, otherwise an enabled alpha blend is BLEND, otherwise shader
flags (SLSF1_VERTEX_ALPHA, SLSF1_SCREENDOOR_ALPHA_FADE, SLSF2_PREMULTIPLIED_ALPHA) force
BLEND, and `publish_material` (material.rs:425) turns a sub-1.0 shader alpha into BLEND.

For every converted GLB this reads the source NIF blocks named in `extras.openSkyrim` and
reports which of those inputs produced the mode. The result is the population each fix
recipe has to cover.

Usage:
    python tools/research/blend_cause_census.py [meshes_dir] [--limit N] [--list cause]
"""

import collections
import json
import os
import sys

import glb_materials
import nif_alpha

VFS_MESHES = "C:/Modding/SkyrimConverted/vfs/meshes"


def nif_path_for(glb_relative):
    candidate = os.path.join(VFS_MESHES, glb_relative)
    if candidate.endswith(".glb"):
        candidate = candidate[:-4] + ".nif"
    return candidate if os.path.isfile(candidate) else None


def shape_alpha(header, shape_index):
    """(alpha_block_index or None, flags or None) for one shape block."""
    blocks = header["blocks"]
    alpha_indices = {
        block["index"]: block for block in blocks if block["type"] == "NiAlphaProperty"
    }
    if not alpha_indices or shape_index >= len(blocks):
        return None, None
    hits = nif_alpha.shape_alpha_references(header, blocks[shape_index], set(alpha_indices))
    if not hits:
        return None, None
    _, value = hits[-1]
    flags, _threshold = nif_alpha.read_alpha_tail(alpha_indices[value]["data"])
    return value, flags


def classify(material, header):
    mode = material.get("alphaMode", "OPAQUE")
    if mode == "OPAQUE":
        return "opaque"
    extras = material.get("extras", {}).get("openSkyrim", {})
    shape_index = extras.get("shapeBlock")
    shader_index = extras.get("shaderBlock")
    flags = None
    if header is not None and isinstance(shape_index, int):
        _, flags = shape_alpha(header, shape_index)
    flags_1 = flags_2 = None
    if header is not None and isinstance(shader_index, int):
        block = header["blocks"][shader_index]
        data = block["data"]
        try:
            if block["type"] == "BSLightingShaderProperty":
                import struct

                flags_1, flags_2 = struct.unpack_from("<II", data, 16)
            elif block["type"] == "BSEffectShaderProperty":
                import struct

                flags_1, flags_2 = struct.unpack_from("<II", data, 12)
        except Exception:  # noqa: BLE001
            pass
    blended = flags is not None and flags & 0x0001 != 0
    tested = flags is not None and flags & 0x0200 != 0
    if mode == "MASK":
        if tested:
            return "mask_from_alpha_test"
        return "mask_other"
    # BLEND
    if tested:
        return "blend_with_test_enabled"
    if blended:
        return "blend_from_alpha_property"
    alpha = material.get("pbrMetallicRoughness", {}).get("baseColorFactor", [1, 1, 1, 1])[3]
    if alpha < 1.0:
        return "blend_from_shader_alpha"
    if flags_1 is not None and flags_1 & (1 << 3):
        return "blend_from_vertex_alpha_flag"
    if flags_1 is not None and flags_1 & (1 << 19):
        return "blend_from_screendoor_flag"
    if flags_2 is not None and flags_2 & (1 << 19):
        return "blend_from_premultiplied_flag"
    if flags is None:
        return "blend_without_alpha_property_or_flag"
    return "blend_unknown"


def main(argv):
    root = glb_materials.DEFAULT_ROOT
    limit = None
    for arg in argv[1:]:
        if os.path.isdir(arg):
            root = arg
        elif arg.startswith("--limit"):
            limit = int(arg.split("=")[1]) if "=" in arg else int(argv[argv.index(arg) + 1])
    counts = collections.Counter()
    families = collections.defaultdict(collections.Counter)
    examples = collections.defaultdict(list)
    material_total = 0
    model_total = 0
    for relative, _, document in glb_materials.models(root):
        materials = document.get("materials", [])
        if not materials:
            continue
        model_total += 1
        if limit is not None and model_total > limit:
            break
        nif = nif_path_for(relative)
        header = None
        if nif:
            try:
                with open(nif, "rb") as handle:
                    data = handle.read()
                for export_strings in (3, 4):
                    try:
                        candidate = nif_alpha.parse_header(data, export_strings)
                    except Exception:  # noqa: BLE001
                        continue
                    if candidate["valid"]:
                        header = candidate
                        break
            except OSError:
                header = None
        for material in materials:
            material_total += 1
            cause = classify(material, header)
            counts[cause] += 1
            folder = relative.split("/")[0]
            families[cause][folder] += 1
            if len(examples[cause]) < 12:
                examples[cause].append(relative)
    print(f"models with materials: {model_total}, materials: {material_total}")
    for cause, count in counts.most_common():
        print(f"  {count:7d}  {cause}")
        print(f"           top folders: {dict(families[cause].most_common(6))}")
        print(f"           e.g. {json.dumps(examples[cause][:6])}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
