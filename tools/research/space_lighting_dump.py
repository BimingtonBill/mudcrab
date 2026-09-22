#!/usr/bin/env python3
"""Step 0 for impl-038: read the real XCLL / LGTM / CLMT / WTHR / DNAM / MATO bytes.

The record layouts the visual-gaps spec (docs/research/visual-gaps-spec.md, gap 2 and
gap 3) uses come from web documentation and had never been checked against our data.
This script prints the bytes and the decoded fields so the parsers can be written
against the data instead of the documentation.

Usage:
    python tools/research/space_lighting_dump.py [plugin ...]

Defaults to Skyrim.esm in the Steam install. It also consults the converted database
read-only to name the cells and worldspaces (the DB's record ids match Skyrim.esm's
for master index 0). Nothing is written anywhere.

The group/record/subrecord walk mirrors crates/converter/src/esm/binary.rs so the
numbers agree with the converter.

Never point this at a whole .ba2/.bsa; it reads .esm/.esp/.esl only.
"""

import collections
import mmap
import sqlite3
import struct
import sys
import zlib

FLAG_COMPRESSED = 0x00040000
GRUP = b"GRUP"

DB = "C:/Modding/SkyrimConverted/skyrim_world.db"
DEFAULT_PLUGIN = (
    "C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/Skyrim.esm"
)

# The spaces the brief names. Alftand01/Alftand02/AlftandZCell are interiors,
# Tamriel/AlftandWorld/Blackreach are worldspaces.
CELLS = {
    0x152C3: "Alftand01",
    0x56C1B: "Alftand02",
    0x69858: "AlftandZCell",
    0x2D4E0: "Blackreach cell (0x2D4E0)",
    0x69869: "AlftandWorld cell (0x69869)",
}
WORLDS = {0x3C: "Tamriel", 0x69857: "AlftandWorld", 0x1EE62: "Blackreach"}

# The snow roof the brief names, plus the holdout roofs the spec lists.
STATS = {
    "DweFacadeTowerRoof01SnowHeavy": None,
    "DweFacadeTower1x3Snow": None,
    "DweFacadeTowerArch01Snow": None,
    "DweFacadeTower1x4Snow": None,
}


def u32(buf, pos):
    return struct.unpack_from("<I", buf, pos)[0]


def i32(buf, pos):
    return struct.unpack_from("<i", buf, pos)[0]


def f32(buf, pos):
    return struct.unpack_from("<f", buf, pos)[0]


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
    if not flags & FLAG_COMPRESSED or len(payload) < 4:
        return payload
    expected = u32(payload, 0)
    out = zlib.decompress(payload[4:])
    if len(out) != expected:
        raise ValueError("decompressed size mismatch")
    return out


def walk(buf, start, end, visit, depth=0):
    """Walk groups and records, calling visit(tag, form_id, flags, payload)."""
    pos = start
    while pos + 24 <= end:
        tag = buf[pos : pos + 4]
        size = u32(buf, pos + 4)
        if tag == GRUP:
            if size < 24 or pos + size > end:
                return
            walk(buf, pos + 24, pos + size, visit, depth + 1)
            pos += size
            continue
        flags = u32(buf, pos + 8)
        form_id = u32(buf, pos + 12)
        if pos + 24 + size > end:
            return
        visit(tag, form_id, flags, buf[pos + 24 : pos + 24 + size])
        pos += 24 + size


def rgba(data):
    if data is None or len(data) < 4:
        return None
    r, g, b, a = data[0], data[1], data[2], data[3]
    return (r, g, b, a)


def luminance(color):
    """Rec.709 luma of an RGB triple, 0..255."""
    if color is None:
        return None
    r, g, b = color[:3]
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


INHERIT_NAMES = ["inherit bit %d" % bit for bit in range(11)]

# bit -> the XCLL fields it governs. The spec's list is 11 flags; the rotation
# flags drive both rotation fields and the light-fade flag both distances.
INHERIT_BITS = {
    0: ["ambient"],
    1: ["directional"],
    2: ["fog_near_color"],
    3: ["fog_near"],
    4: ["fog_far"],
    5: ["direction_xy", "direction_z"],
    6: ["direction_fade"],
    7: ["fog_clip"],
    8: ["fog_power"],
    9: ["fog_max"],
    10: ["light_fade_begin", "light_fade_end"],
}
# Not covered by any inherit flag. SSE publishes them in a separate DALC
# subrecord; the fields are the six directional-ambient tints, the specular
# colour and the specular power, in both the cell's XCLL and the template's DATA.
UNTILED_FIELDS = ["tints", "specular", "specular_power"]


def decode_xcll(data):
    """Decode the 92-byte XCLL. Returns None if the record is a shorter variant."""
    if data is None or len(data) < 92:
        return None
    out = {
        "length": len(data),
        "ambient": rgba(data[0x00:0x04]),
        "directional": rgba(data[0x04:0x08]),
        "fog_near_color": rgba(data[0x08:0x0C]),
        "fog_near": f32(data, 0x0C),
        "fog_far": f32(data, 0x10),
        "direction_xy": i32(data, 0x14),
        "direction_z": i32(data, 0x18),
        "direction_fade": f32(data, 0x1C),
        "fog_clip": f32(data, 0x20),
        "fog_power": f32(data, 0x24),
        "fog_far_color": rgba(data[0x48:0x4C]),
        "fog_max": f32(data, 0x4C),
        "light_fade_begin": f32(data, 0x50),
        "light_fade_end": f32(data, 0x54),
        "inherit": u32(data, 0x58),
    }
    out["tints"] = [rgba(data[0x28 + 4 * i : 0x2C + 4 * i]) for i in range(6)]
    out["specular"] = rgba(data[0x40:0x44])
    out["specular_power"] = f32(data, 0x44)
    return out


def show_xcll(label, data):
    decoded_xcll = decode_xcll(data)
    print(f"  {label}: {(len(data) if data is not None else 'absent')} bytes")
    if data is None:
        return None
    if decoded_xcll is None:
        print(f"    raw ({len(data)}B, shorter than 92): {bytes(data).hex(' ')}")
        return None
    print(f"    ambient={decoded_xcll['ambient']} (L2 {luminance(decoded_xcll['ambient']):.1f})")
    print(
        f"    directional={decoded_xcll['directional']} (L2 "
        f"{luminance(decoded_xcll['directional']):.1f})"
    )
    print(
        f"    fog_near_color={decoded_xcll['fog_near_color']} fog_far_color="
        f"{decoded_xcll['fog_far_color']}"
    )
    print(
        f"    fog_near={decoded_xcll['fog_near']:.3f} fog_far={decoded_xcll['fog_far']:.3f} "
        f"fog_clip={decoded_xcll['fog_clip']:.3f} fog_power={decoded_xcll['fog_power']:.3f} "
        f"fog_max={decoded_xcll['fog_max']:.3f}"
    )
    print(
        f"    direction_xy={decoded_xcll['direction_xy']} direction_z="
        f"{decoded_xcll['direction_z']} direction_fade={decoded_xcll['direction_fade']:.3f}"
    )
    print(
        f"    light_fade_begin={decoded_xcll['light_fade_begin']:.3f} "
        f"light_fade_end={decoded_xcll['light_fade_end']:.3f}"
    )
    print(f"    specular={decoded_xcll['specular']} power={decoded_xcll['specular_power']:.3f}")
    print(f"    tints={decoded_xcll['tints']}")
    inherit = decoded_xcll["inherit"]
    bits = [name for bit, name in enumerate(INHERIT_NAMES) if inherit & (1 << bit)]
    print(f"    inherit=0x{inherit:08X} bits set: {', '.join(bits) if bits else '(none)'}")
    return decoded_xcll


def resolve(cell_xcll, template_xcll, inherit_means_set, tints_from_template=False):
    """Resolve a field the two ways: bit set means 'inherited from the template',
    or bit set means 'this record overrides'."""
    if cell_xcll is None:
        return template_xcll
    if template_xcll is None:
        return cell_xcll
    flags = cell_xcll["inherit"]
    out = {}
    for bit, names in INHERIT_BITS.items():
        taken_from_template = bool(flags & (1 << bit)) == inherit_means_set
        for name in names:
            out[name] = template_xcll[name] if taken_from_template else cell_xcll[name]
    for name in UNTILED_FIELDS:
        out[name] = template_xcll[name] if tints_from_template else cell_xcll[name]
    return out


def template_payload(template):
    """The LGTM's 92-byte lighting payload lives in its DATA subrecord, not in an
    XCLL one - verified on the bytes below."""
    if template is None:
        return None
    return template["data"] if template.get("data") is not None else template.get("xcll")


def with_dalc(decoded_xcll, dalc):
    """SSE moved the six directional-ambient tints, the specular colour and the
    specular power into a separate 32-byte DALC subrecord. When a record has one
    it is authoritative; the same bytes in XCLL's 0x28..0x48 are the older
    in-record copy."""
    if decoded_xcll is None or dalc is None or len(dalc) < 32:
        return decoded_xcll
    colors = dalc_colors(dalc)
    out = dict(decoded_xcll)
    out["tints"] = colors[:6]
    out["specular"] = colors[6]
    out["specular_power"] = f32(dalc, 28)
    out["tints_from"] = "DALC"
    return out


def dalc_colors(data):
    """DALC is 32 bytes: eight little-endian RGBA colours."""
    if data is None or len(data) < 32:
        return None
    return [rgba(data[4 * i : 4 * i + 4]) for i in range(len(data) // 4)]


def inherit_statistics(cells, ltgm):
    """Bulk evidence for the polarity.

    For every interior cell with both an XCLL and an LTMP, count per field: does
    the cell's own value equal the template's, and is the corresponding inherit
    bit set. Under "bit set = inherited from the template" (A) the cell's own
    value is irrelevant for that field, so a *zero* own value with the bit set is
    harmless; under "bit set = this cell overrides" (B) that same cell renders
    black. The count of such cells is the arithmetic: shipped data cannot be full
    of black rooms, so the reading that does not require them is the right one.
    """
    fields = [
        ("ambient", 0),
        ("directional", 1),
        ("fog_near_color", 2),
        ("fog_near", 3),
        ("fog_far", 4),
    ]
    counts = {
        name: {
            "set_equal": 0, "set_different": 0, "clear_equal": 0, "clear_different": 0,
            "set_own_zero": 0, "set_template_zero": 0, "clear_own_zero": 0,
            "clear_template_zero": 0,
        }
        for name, _ in fields
    }
    pairs = 0
    black_ambient = {"A": 0, "B": 0}
    equal_ambient = {"A": 0, "B": 0}
    examples = []
    templates_with_dalc = 0
    cells_with_dalc = 0
    for cell in cells.values():
        template = ltgm.get(cell["ltmp"]) if cell["ltmp"] else None
        if template is not None and template["dalc"] is not None:
            templates_with_dalc += 1
        if cell["dalc"] is not None:
            cells_with_dalc += 1
        own = with_dalc(decode_xcll(cell["xcll"]), cell["dalc"])
        template_xcll = with_dalc(decode_xcll(template_payload(template)), template["dalc"]) if template else None
        if own is None or template_xcll is None:
            continue
        pairs += 1
        for name, bit in fields:
            flag = bool(own["inherit"] & (1 << bit))
            equal = own[name] == template_xcll[name]
            zero = (0, 0, 0, 0) if isinstance(own[name], tuple) else 0.0
            bucket = "set" if flag else "clear"
            counts[name][f"{bucket}_equal" if equal else f"{bucket}_different"] += 1
            if zero == own[name]:
                counts[name][f"{bucket}_own_zero"] += 1
            if zero == template_xcll[name]:
                counts[name][f"{bucket}_template_zero"] += 1
        for key, inherit_means_set in (("A", True), ("B", False)):
            resolved = resolve(own, template_xcll, inherit_means_set)
            if resolved["ambient"] == (0, 0, 0, 0):
                black_ambient[key] += 1
            if resolved["ambient"] == own["ambient"]:
                equal_ambient[key] += 1
        if own["ambient"] == (0, 0, 0, 0) and len(examples) < 12:
            examples.append(
                (
                    cell["edid"],
                    cell["ltmp"],
                    template["edid"],
                    hex(own["inherit"]),
                    own["ambient"],
                    template_xcll["ambient"],
                )
            )
    print(f"\n== inherit polarity: cells with both an XCLL and an LGTM ({pairs} pairs)")
    print(f"  cells carrying a DALC: {cells_with_dalc}, templates with one: {templates_with_dalc}")
    for name, _ in fields:
        row = counts[name]
        print(
            f"  {name:15s} bit set: equal {row['set_equal']:4d} different {row['set_different']:4d} "
            f"| set & own zero {row['set_own_zero']:4d} | set & template zero {row['set_template_zero']:4d}"
        )
        print(
            f"  {'':15s} bit clear: equal {row['clear_equal']:4d} different {row['clear_different']:4d} "
            f"| clear & own zero {row['clear_own_zero']:4d} | clear & template zero "
            f"{row['clear_template_zero']:4d}"
        )
    print(
        "\n  reading A (bit set = inherited from the template) leaves "
        f"{black_ambient['A']} cells with a black ambient; reading B leaves {black_ambient['B']}"
    )
    print(
        "  cells whose resolved ambient is their own XCLL value: "
        f"A {equal_ambient['A']}, B {equal_ambient['B']}"
    )
    print("  cells whose own ambient is black (edid, ltmp, template, inherit, own, template):")
    for example in examples:
        print(f"    {example}")


def dump_db():
    conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
    print("== database: the spaces the brief names")
    print("-- cells")
    for form_id, name in CELLS.items():
        row = conn.execute(
            "select id, interior_name, worldspace_id, grid_x, grid_y, flags "
            "from cells where id = ?",
            (form_id,),
        ).fetchone()
        print(f"  0x{form_id:08X} {name}: {row}")
    print("-- cells of AlftandWorld (0x69857)")
    for row in conn.execute(
        "select id, interior_name, grid_x, grid_y from cells where worldspace_id = 0x69857"
    ):
        print(f"  {row}")
    print("-- worldspaces")
    for row in conn.execute(
        "select id, editor_id, parent_world, flags from worldspaces "
        "where id in (0x3C, 0x69857, 0x1EE62)"
    ):
        print(f"  {row}")
    print("-- cells whose interior_name mentions Alftand or Blackreach")
    for row in conn.execute(
        "select id, interior_name, worldspace_id, grid_x, grid_y from cells "
        "where interior_name like '%Alftand%' or interior_name like '%Blackreach%' "
        "order by id"
    ):
        print(f"  {row}")
    conn.close()


def main():
    paths = sys.argv[1:] or [DEFAULT_PLUGIN]
    try:
        dump_db()
    except sqlite3.Error as error:
        print(f"(database not readable: {error})")

    cells = {}
    worlds = {}
    ltgm = {}
    clmt = {}
    wthr = {}
    mato = {}
    imad = {}
    stats = {}
    target_stats = {}
    xcll_lengths = collections.Counter()
    xcll_lengths_interior = collections.Counter()
    xcll_lengths_exterior = collections.Counter()
    ltmp_ids = set()

    for path in paths:
        handle = open(path, "rb")
        buf = mmap.mmap(handle.fileno(), 0, access=mmap.ACCESS_READ)
        print(f"\n== plugin {path}")

        def visit(tag, form_id, flags, payload):
            if flags & FLAG_COMPRESSED:
                try:
                    payload = decoded(payload, flags)
                except (ValueError, zlib.error):
                    return
            if tag == b"CELL":
                xcll = field(payload, b"XCLL")
                data = field(payload, b"DATA")
                interior = bool(data and len(data) >= 1 and data[0] & 0x01)
                ltgm_id = field(payload, b"LTMP")
                record = {
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "interior": interior,
                    "xcll": xcll,
                    "ltmp": u32(ltgm_id, 0) if ltgm_id and len(ltgm_id) >= 4 else None,
                    "xccl": field(payload, b"XCCM"),
                    "xcwt": field(payload, b"XCWT"),
                    "dalc": field(payload, b"DALC"),
                    "tags": [(t.decode("latin-1"), len(d)) for t, d in subrecords(payload)],
                    "extra": {
                        name: field(payload, name.encode())
                        for name in ("XCIM", "XCCM", "XCWT", "XCMO", "XCAS", "XLCN",
                                     "XEZN", "XCLW", "XWEM", "XNAM", "LNAM")
                    },
                }
                cells[form_id] = record
                if record["ltmp"] is not None:
                    ltmp_ids.add(record["ltmp"])
                if xcll is not None:
                    xcll_lengths[len(xcll)] += 1
                    (xcll_lengths_interior if interior else xcll_lengths_exterior)[
                        len(xcll)
                    ] += 1
            elif tag == b"LGTM":
                ltgm[form_id] = {
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "data": field(payload, b"DATA"),
                    "xcll": field(payload, b"XCLL"),
                    "dalc": field(payload, b"DALC"),
                    "tags": [(t.decode("latin-1"), len(d)) for t, d in subrecords(payload)],
                }
            elif tag == b"IMAD":
                imad[form_id] = {
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "tags": [(t.decode("latin-1"), len(d)) for t, d in subrecords(payload)],
                    "data": field(payload, b"DATA"),
                    "tints": [bytes(d) for d in fields(payload, b"NAM3")],
                }
            elif tag == b"WRLD":
                cnam = field(payload, b"CNAM")
                worlds[form_id] = {
                    "tags": [(t.decode("latin-1"), len(d)) for t, d in subrecords(payload)],
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "cnam": u32(cnam, 0) if cnam and len(cnam) >= 4 else None,
                    "nam0": field(payload, b"NAM0"),
                    "tnam": field(payload, b"TNAM"),
                    "flags": flags,
                }
            elif tag == b"CLMT":
                wlst = fields(payload, b"WLST")
                clmt[form_id] = {
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "wlst": [bytes(entry) for entry in wlst],
                    "fnam": field(payload, b"FNAM"),
                    "tnam": field(payload, b"TNAM"),
                    "model": cstring(field(payload, b"MODL") or b""),
                }
            elif tag == b"WTHR":
                tags = [(t.decode("latin-1"), len(d)) for t, d in subrecords(payload)]
                wthr[form_id] = {
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "tags": tags,
                    "nam0": field(payload, b"NAM0"),
                    "fnam": field(payload, b"FNAM"),
                    "data": field(payload, b"DATA"),
                    "tnam": fields(payload, b"TNAM"),
                }
            elif tag == b"MATO":
                mato[form_id] = {
                    "edid": cstring(field(payload, b"EDID") or b""),
                    "data": field(payload, b"DATA"),
                }
            elif tag in (b"STAT", b"MSTT", b"FURN", b"ACTI", b"FLOR", b"CONT", b"TREE"):
                edid = cstring(field(payload, b"EDID") or b"")
                dnam = field(payload, b"DNAM")
                stats[form_id] = {
                    "type": tag.decode("ascii"),
                    "edid": edid,
                    "model": cstring(field(payload, b"MODL") or b""),
                    "dnam": dnam,
                }
                if edid in STATS:
                    target_stats[form_id] = stats[form_id]

        walk(buf, u32(buf, 4) + 24, len(buf), visit)
        buf.close()
        handle.close()

    print("\n== XCLL length histogram over every CELL in the plugin(s)")
    print(f"  all: {dict(sorted(xcll_lengths.items()))}")
    print(f"  interior: {dict(sorted(xcll_lengths_interior.items()))}")
    print(f"  exterior: {dict(sorted(xcll_lengths_exterior.items()))}")
    print(f"  cells: {len(cells)}, with an LTMP: {sum(1 for c in cells.values() if c['ltmp'])}")

    print("\n== the named cells")
    for form_id, name in CELLS.items():
        cell = cells.get(form_id)
        print(f"\n-- 0x{form_id:08X} {name}: {cell['edid'] if cell else 'NOT IN PLUGIN'}")
        if not cell:
            continue
        print(f"  subrecords: {cell['tags']}")
        if cell["xcll"] is not None:
            print(f"  XCLL hex: {bytes(cell['xcll']).hex(' ')}")
        cell_xcll = with_dalc(show_xcll("cell XCLL", cell["xcll"]), cell["dalc"])
        print(f"  CELL DALC: {dalc_colors(cell['dalc'])}")
        for name in ("XCIM", "XCCM", "XCWT", "XCMO", "XCAS", "XLCN", "XEZN", "XCLW"):
            value = cell["extra"].get(name)
            if value is not None and len(value) >= 4:
                print(f"  {name} -> 0x{u32(value, 0):08X}")
        xcim = cell["extra"].get("XCIM")
        if xcim is not None and len(xcim) >= 4:
            modifier = imad.get(u32(xcim, 0))
            if modifier is None:
                print("    (XCIM target not in plugin)")
            else:
                print(f"    IMAD {modifier['edid']!r} subrecords: {modifier['tags']}")
                data = modifier["data"]
                print(f"    IMAD DATA ({len(data) if data else 0}B): "
                      f"{bytes(data).hex(' ') if data else None}")
                if data:
                    print(
                        "    IMAD DATA floats: "
                        + " ".join(f"{f32(data, 4 * i):.4f}" for i in range(len(data) // 4))
                    )
                for tint in modifier["tints"]:
                    print(f"    IMAD NAM3: {bytes(tint).hex(' ')}")
        template_id = cell["ltmp"]
        print(f"  LTMP -> 0x{template_id:08X}" if template_id else "  LTMP: none")
        template = ltgm.get(template_id) if template_id else None
        if template is not None:
            print(f"  LGTM {template['edid']!r} subrecords: {template['tags']}")
            print(
                f"  LGTM DATA ({len(template['data']) if template['data'] is not None else 0}B): "
                f"{bytes(template['data']).hex(' ') if template['data'] is not None else None}"
            )
            template_xcll = with_dalc(
                decode_xcll(template_payload(template)), template["dalc"]
            )
            print(f"  LGTM DALC: {dalc_colors(template['dalc'])}")
            print("\n  polarity arithmetic:")
            for inherit_means_set, label in (
                (True, "A: bit set => value comes from the TEMPLATE"),
                (False, "B: bit set => this cell OVERRIDES (value comes from the cell)"),
            ):
                resolved = resolve(cell_xcll, template_xcll, inherit_means_set)
                print(
                    f"    {label}: ambient={resolved['ambient']} "
                    f"L2={luminance(resolved['ambient']):.1f} "
                    f"direction={resolved['directional']} "
                    f"L2={luminance(resolved['directional']):.1f} "
                    f"fog_near={resolved['fog_near']:.1f} "
                    f"fog_far={resolved['fog_far']:.1f} "
                    f"tints_mean_L2={sum(luminance(t) for t in resolved['tints']) / 6:.1f}"
                )

    print("\n== LGTM records reached from the named cells")
    for form_id, name in CELLS.items():
        template_id = cells.get(form_id, {}).get("ltmp")
        if template_id and template_id in ltgm:
            template = ltgm[template_id]
            print(f"\n-- LGTM 0x{template_id:08X} {template['edid']!r} (for {name})")
            print(f"  subrecords: {template['tags']}")
            show_xcll("LGTM DATA (the 92-byte payload)", template["data"])
            print(f"  DALC: {dalc_colors(template['dalc'])}")
    print(f"\n  (LGTM records total: {len(ltgm)}, LTMP ids referenced by cells: {len(ltmp_ids)})")
    inherit_statistics(cells, ltgm)

    print("\n== WRLD climates")
    for form_id, name in WORLDS.items():
        world = worlds.get(form_id)
        print(f"-- 0x{form_id:08X} {name}: {world['edid'] if world else 'NOT IN PLUGIN'}")
        if not world:
            continue
        climate_id = world["cnam"]
        print(f"  CNAM climate -> 0x{climate_id:08X}" if climate_id else "  CNAM: none")
        climate = clmt.get(climate_id) if climate_id else None
        if climate is None:
            print("  (climate record not found)")
            continue
        print(f"  CLMT {climate['edid']!r} WLST entries: {len(climate['wlst'])}")
        for entry in climate["wlst"]:
            weather_id = u32(entry, 0) if len(entry) >= 4 else None
            chance = u32(entry, 4) if len(entry) >= 8 else None
            glob = u32(entry, 8) if len(entry) >= 12 else None
            weather = wthr.get(weather_id)
            print(
                f"    WLST {len(entry)}B: weather=0x{weather_id:08X} "
                f"chance={chance} glob=0x{glob:08X} "
                f"{weather['edid'] if weather else '(weather not in plugin)'}"
            )
        print(
            f"  CLMT FNAM ({len(climate['fnam']) if climate['fnam'] else 0}B)="
            f"{cstring(climate['fnam']) if climate['fnam'] else None} "
            f"TNAM={bytes(climate['tnam']).hex(' ') if climate['tnam'] else None}"
        )

    print("\n== WTHR records (the first of each climate)")
    seen = set()
    for form_id, name in WORLDS.items():
        world = worlds.get(form_id)
        if not world or not world["cnam"]:
            continue
        climate = clmt.get(world["cnam"])
        if not climate or not climate["wlst"]:
            continue
        weather_id = u32(climate["wlst"][0], 0)
        if weather_id in seen:
            continue
        seen.add(weather_id)
        weather = wthr.get(weather_id)
        if weather is None:
            print(f"-- WTHR 0x{weather_id:08X} not in plugin")
            continue
        print(f"\n-- WTHR 0x{weather_id:08X} {weather['edid']!r} (first WLST of {name})")
        print(f"  subrecords: {weather['tags']}")
        nam0 = weather["nam0"]
        print(f"  NAM0: {len(nam0) if nam0 else 0} bytes")
        if nam0:
            for group in range(len(nam0) // 16):
                chunk = nam0[group * 16 : group * 16 + 16]
                print(
                    f"    group {group:2d} (time index {group % 4}): "
                    f"sky_upper={rgba(chunk[0:4])} fog_near={rgba(chunk[4:8])} "
                    f"[2]={rgba(chunk[8:12])} [3]={rgba(chunk[12:16])}"
                )
        fnam = weather["fnam"]
        print(f"  FNAM: {len(fnam) if fnam else 0} bytes")
        if fnam and len(fnam) >= 32:
            names = [
                "day_near", "day_far", "day_power", "day_max",
                "night_near", "night_far", "night_power", "night_max",
            ]
            print(
                "    " + " ".join(
                    f"{name}={f32(fnam, 4 * i):.3f}" for i, name in enumerate(names)
                )
            )
        data = weather["data"]
        print(f"  DATA: {len(data) if data else 0} bytes {bytes(data).hex(' ') if data else ''}")

    print("\n== WRLD record flags and subrecords (a candidate for has_sky)")
    for form_id, name in WORLDS.items():
        world = worlds.get(form_id)
        if not world:
            continue
        print(
            f"  0x{form_id:08X} {name}: flags=0x{world['flags']:08X} cnam={world['cnam'] and hex(world['cnam'])} "
            f"tags={world['tags']}"
        )
    flagged = [
        (form_id, world["edid"], hex(world["flags"]), world["cnam"])
        for form_id, world in sorted(worlds.items())
        if world["flags"]
    ]
    print(f"  worldspaces with any record flag: {len(flagged)} of {len(worlds)}")
    for form_id, edid, flags, cnam in flagged:
        print(f"    0x{form_id:08X} {edid!r} flags={flags} cnam={cnam and hex(cnam)} tags={worlds[form_id]['tags'][:8]}")

    print("\n== WTHR DATA for a few named weathers (classification byte)")
    for form_id, weather in sorted(wthr.items()):
        edid = weather["edid"]
        if not edid.startswith("Skyrim"):
            continue
        data = weather["data"]
        if data is None:
            continue
        print(f"  0x{form_id:08X} {edid:28s} ({len(data)}B) {bytes(data).hex(' ')}")

    print("\n== LGTM shapes: is there ever a parent template?")
    shapes = collections.Counter(
        tuple(name for name, _ in template["tags"]) for template in ltgm.values()
    )
    for shape, count in shapes.most_common():
        print(f"  {count:4d} x {shape}")
    data_lengths = collections.Counter(
        len(template["data"]) if template["data"] is not None else None
        for template in ltgm.values()
    )
    print(f"  DATA lengths: {dict(data_lengths)}")
    trailing = collections.Counter(
        u32(template["data"], 88)
        for template in ltgm.values()
        if template["data"] is not None and len(template["data"]) >= 92
    )
    print(f"  last u32 of an LGTM DATA (the inherit-flag slot): {dict(trailing)}")
    dalc_lengths = collections.Counter(
        len(template["dalc"]) if template["dalc"] is not None else None
        for template in ltgm.values()
    )
    print(f"  DALC lengths: {dict(dalc_lengths)}")
    missing = [form_id for form_id, cell in cells.items() if cell["ltmp"] and cell["ltmp"] not in ltgm]
    print(f"  cells whose LTMP is not a record in the plugin: {len(missing)}")
    for form_id, template in sorted(ltgm.items()):
        data = template["data"]
        if data is not None and len(data) != 92:
            print(
                f"  short LGTM 0x{form_id:08X} {template['edid']!r}: DATA {len(data)}B "
                f"{bytes(data).hex(' ')} DALC {len(template['dalc']) if template['dalc'] else 0}B "
                f"{bytes(template['dalc']).hex(' ') if template['dalc'] else ''}"
            )
    for form_id, cell in sorted(cells.items()):
        payload = cell["xcll"]
        if payload is not None and len(payload) != 92:
            print(
                f"  short CELL 0x{form_id:08X} {cell['edid']!r}: XCLL {len(payload)}B "
                f"{bytes(payload).hex(' ')}"
            )

    print("\n== snow roof STATs")
    print(f"  (STAT/MSTT/FURN/ACTI/FLOR/CONT/TREE records: {len(stats)})")
    for form_id, record in sorted(target_stats.items()):
        print(f"\n-- {record['type']} 0x{form_id:08X} {record['edid']!r} model={record['model']!r}")
        dnam = record["dnam"]
        print(f"  DNAM ({len(dnam) if dnam is not None else 0}B): {bytes(dnam).hex(' ') if dnam else None}")
        if dnam and len(dnam) >= 8:
            max_angle = f32(dnam, 0)
            mato_id = u32(dnam, 4)
            print(f"  max_angle={max_angle:.4f} ({max_angle * 57.29578:.2f} deg) MATO=0x{mato_id:08X}")
            material = mato.get(mato_id)
            if material is None:
                print("  (MATO record not found)")
            else:
                md = material["data"]
                print(f"  MATO {material['edid']!r} DATA ({len(md) if md else 0}B): {bytes(md).hex(' ') if md else None}")
                if md and len(md) >= 32:
                    print(
                        "    floats: "
                        + " ".join(f"{f32(md, 4 * i):.5f}" for i in range(len(md) // 4))
                    )
    roofs = [r for r in stats.values() if "Snow" in r["edid"] and r["edid"].startswith("DweFacade")]
    print(f"\n  other DweFacade*Snow* STATs: {sorted(r['edid'] for r in roofs)}")
    print(f"  MATO records in plugin: {len(mato)}")


if __name__ == "__main__":
    main()
