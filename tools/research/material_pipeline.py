#!/usr/bin/env python3
"""Join a converted GLB material back to the NIF blocks it came from.

For every material in the GLB this prints what the converter published (`alphaMode`,
`alphaCutoff`, `baseColorFactor.a`, `doubleSided`, the `extras.openSkyrim` block numbers)
next to the source NIF's blocks: the shape's NiAlphaProperty (flags decoded: blend enable,
source/destination blend factor, test enable/threshold) and its shader property's flags.

That pairing is what shows which input produced a transparent-looking output.

Usage:
    python tools/research/material_pipeline.py <model substring or .glb> [more...]
    python tools/research/material_pipeline.py --dir <meshes subdirectory>
"""

import os
import struct
import sys

import glb_materials
import nif_alpha

VFS_MESHES = "C:/Modding/SkyrimConverted/vfs/meshes"

# Flags the converter reads, crates/converter/src/material.rs:14-21.
NAMED_FLAGS_1 = {
    3: "SLSF1_VERTEX_ALPHA",
    7: "SLSF1_ENVIRONMENT_MAPPING",
    19: "SLSF1_SCREENDOOR_ALPHA_FADE",
    22: "SLSF1_OWN_EMIT",
}
NAMED_FLAGS_2 = {
    4: "SLSF2_DOUBLE_SIDED",
    6: "SLSF2_GLOW_MAP",
    19: "SLSF2_PREMULTIPLIED_ALPHA",
}


def nif_path_for(glb_relative):
    candidate = os.path.join(VFS_MESHES, glb_relative)
    if candidate.endswith(".glb"):
        candidate = candidate[:-4] + ".nif"
    return candidate if os.path.isfile(candidate) else None


def decode_named(value, names):
    bits = [name for bit, name in sorted(names.items()) if value & (1 << bit)]
    return " ".join(bits) if bits else ""


def flag_line(label, value, names):
    named = decode_named(value, names)
    return f"{label}=0x{value:08X}" + (f" ({named})" if named else "")


def shader_summary(header, index):
    block = header["blocks"][index]
    data = block["data"]
    if block["type"] == "BSLightingShaderProperty":
        shader_type = struct.unpack_from("<I", data, 0)[0]
        flags_1, flags_2 = struct.unpack_from("<II", data, 16)
        floats = struct.unpack_from("<f", data, 61)[0] if len(data) >= 65 else float("nan")
        return (
            f"shader[{index}] BSLLightingShaderProperty type={shader_type} "
            f"{flag_line('flags1', flags_1, NAMED_FLAGS_1)} "
            f"{flag_line('flags2', flags_2, NAMED_FLAGS_2)} alpha@61={floats:.4f}"
        ).replace("BSLLighting", "BSLighting")
    if block["type"] == "BSEffectShaderProperty":
        flags_1, flags_2 = struct.unpack_from("<II", data, 12)
        return (
            f"shader[{index}] BSEffectShaderProperty "
            f"{flag_line('flags1', flags_1, NAMED_FLAGS_1)} "
            f"{flag_line('flags2', flags_2, NAMED_FLAGS_2)}"
        )
    return f"shader[{index}] {block['type']}"


def shape_alpha_text(header, shape_index, alpha_indices):
    """Which NiAlphaProperty the shape's block points at, or NULL when it has none."""
    block = header["blocks"][shape_index]
    hits = nif_alpha.shape_alpha_references(header, block, alpha_indices)
    # The shape's own fields start after NiAVObject, so a reference early in the block is
    # a coincidental small integer; keep the last hit, which is the alpha property field.
    if not hits:
        return "no NiAlphaProperty reference (NULL block)"
    offset, value = hits[-1]
    flags, threshold = nif_alpha.read_alpha_tail(header["blocks"][value]["data"])
    return f"alpha[{value}] @{offset} {nif_alpha.decode_alpha(flags, threshold)}"


def report_model(glb_relative):
    glb = os.path.join(glb_materials.DEFAULT_ROOT, glb_relative)
    document = glb_materials.read_json_chunk(glb)
    if document is None:
        print(f"== {glb_relative}: no JSON chunk")
        return
    nif = nif_path_for(glb_relative)
    header = None
    if nif:
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
    print(f"== {glb_relative}")
    if header is None:
        print(f"   (no source NIF parsed: {nif})")
    else:
        alpha_indices = {
            block["index"] for block in header["blocks"] if block["type"] == "NiAlphaProperty"
        }
    for material in document.get("materials", []):
        info = glb_materials.material_info(material)
        extras = info["extras"].get("openSkyrim", {})
        print(
            f"   {info['name']!r}: {info['alphaMode']} cutoff={info['alphaCutoff']} "
            f"base_alpha={info['alpha']:.4f} doubleSided={info['doubleSided']} "
            f"shapeBlock={extras.get('shapeBlock')} shaderBlock={extras.get('shaderBlock')}"
        )
        if header is None:
            continue
        shape_index = extras.get("shapeBlock")
        if isinstance(shape_index, int) and shape_index < len(header["blocks"]):
            print(f"      {shape_alpha_text(header, shape_index, alpha_indices)}")
        shader_index = extras.get("shaderBlock")
        if isinstance(shader_index, int) and shader_index < len(header["blocks"]):
            print(f"      {shader_summary(header, shader_index)}")


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    if argv[1] == "--dir":
        prefix = argv[2].replace("\\", "/").strip("/").lower()
        matches = [
            relative
            for relative, _, _ in glb_materials.models(glb_materials.DEFAULT_ROOT)
            if relative.startswith(prefix)
        ]
        for relative in sorted(matches):
            report_model(relative)
        return 0
    for needle in argv[1:]:
        if needle.lower().endswith(".glb") and os.path.isfile(needle):
            report_model(
                os.path.relpath(needle, glb_materials.DEFAULT_ROOT).replace("\\", "/")
            )
            continue
        found = False
        for relative, _, _ in glb_materials.models(glb_materials.DEFAULT_ROOT):
            if needle.lower() in relative.lower():
                report_model(relative)
                found = True
        if not found:
            print(f"no GLB matched {needle!r}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
