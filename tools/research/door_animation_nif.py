#!/usr/bin/env python3
"""Read a NIF out of a Skyrim BSA and dump its animation blocks.

Written for research-027 (animated doors). It mirrors two implementations that are
known to work on this install:

* the BSA reader in `crates/converter/src/archive/bsa.rs` (header, folder records,
  file records, embedded names, zlib/LZ4 payloads);
* the NIF header walk in `crates/converter/src/mesh.rs` (`parse_skyrim_header`) and
  the block layouts in `vendor/project-wormhole-nif/src/nif_block.rs`.

Everything the vendored parser leaves as `Unhandled` - NiTransformController,
NiTransformInterpolator, NiTransformData - is parsed here by hand from the nif.xml
layout, so the numbers this prints are the file's own bytes and not a decoder's
guess. Where a hand layout does not consume a block exactly, the block is reported
as `mismatch` with the bytes it could not account for.

Usage:
    python tools/research/door_animation_nif.py list <archive> <substring>
    python tools/research/door_animation_nif.py dump <archive> <entry-name>
    python tools/research/door_animation_nif.py doors <archive> [<entry-name> ...]
"""

from __future__ import annotations

import json
import struct
import sys
import zlib
from pathlib import Path

HEADER_SIZE = 36
FILE_RECORD_SIZE = 16
ARCHIVE_COMPRESSED = 0x0004
ARCHIVE_EMBED_FILE_NAMES = 0x0100
FILE_COMPRESSION_TOGGLE = 0x40000000
FILE_SIZE_MASK = 0x3FFFFFFF

MESHES0 = Path("C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/Skyrim - Meshes0.bsa")
MESHES1 = Path("C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/Skyrim - Meshes1.bsa")

# The route's door models, as the demo and the brief name them.
ROUTE_MODELS = [
    "meshes/dungeons/dwemer/door/dwemersmalldoorload01.nif",
    "meshes/dungeons/dwemer/door/dwemerlargedoorload01.nif",
    "meshes/dungeons/dwemer/door/dwemersmalldoor01.nif",
    "meshes/dungeons/dwemer/door/dwemerlargedoor01.nif",
    "meshes/architecture/farmhouse/farmhouseanimdoor01.nif",
    "meshes/architecture/farmhouse/farmhousedoor01.nif",
    "meshes/clutter/doors/woodendoor01.nif",
]


def u32(b: bytes, o: int) -> int:
    return struct.unpack_from("<I", b, o)[0]


def f32(b: bytes, o: int) -> float:
    return struct.unpack_from("<f", b, o)[0]


def decompress(payload: bytes, version: int) -> bytes:
    expected = u32(payload, 0)
    body = payload[4:]
    if version >= 105 and body[:4] == b"\x04\x22\x4d\x18":
        try:
            import lz4.frame  # type: ignore
        except ImportError as exc:  # pragma: no cover - environment dependent
            raise SystemExit(
                "this archive stores LZ4 frames; install the lz4 package "
                f"(`python -m pip install lz4`) to read it: {exc}"
            )
        out = lz4.frame.decompress(body)
    elif version >= 105:
        try:
            out = zlib.decompress(body)
        except zlib.error:
            import lz4.block  # type: ignore

            out = lz4.block.decompress(body, uncompressed_size=expected)
    else:
        out = zlib.decompress(body)
    if len(out) != expected:
        raise SystemExit(f"decompressed size mismatch: expected {expected}, got {len(out)}")
    return out


class Bsa:
    def __init__(self, path: Path) -> None:
        self.path = path
        self.bytes = path.read_bytes()
        b = self.bytes
        if b[:4] != b"BSA\0":
            raise SystemExit(f"{path} is not a BSA")
        self.version = u32(b, 4)
        self.flags = u32(b, 12)
        folder_count = u32(b, 16)
        file_count = u32(b, 20)
        total_file_name_length = u32(b, 28)
        self.folder_record_size = 24 if self.version >= 105 else 16
        folder_table_end = HEADER_SIZE + folder_count * self.folder_record_size

        counts = [u32(b, HEADER_SIZE + i * self.folder_record_size + 8) for i in range(folder_count)]

        cursor = folder_table_end
        records = []
        for count in counts:
            name_len = b[cursor]
            cursor += 1
            folder = b[cursor : cursor + name_len].decode("latin1").rstrip("\0")
            cursor += name_len
            for _ in range(count):
                size_flags = u32(b, cursor + 8)
                offset = u32(b, cursor + 12)
                records.append((folder, size_flags, offset))
                cursor += FILE_RECORD_SIZE
        names_table = b[cursor : cursor + total_file_name_length]
        names = names_table.split(b"\0")
        self.entries: dict[str, tuple[int, int]] = {}
        for (folder, size_flags, offset), raw_name in zip(records, names[:file_count]):
            name = raw_name.decode("latin1")
            full = f"{folder}/{name}" if folder else name
            self.entries[full.lower().replace("\\", "/")] = (size_flags, offset)

    def read(self, name: str) -> bytes:
        size_flags, offset = self.entries[name.lower().replace("\\", "/")]
        stored = size_flags & FILE_SIZE_MASK
        payload = self.bytes[offset : offset + stored]
        if self.flags & ARCHIVE_EMBED_FILE_NAMES:
            embedded_len = payload[0]
            payload = payload[1 + embedded_len :]
        compressed = bool(self.flags & ARCHIVE_COMPRESSED) ^ bool(size_flags & FILE_COMPRESSION_TOGGLE)
        return decompress(payload, self.version) if compressed else payload

    def find(self, needle: str) -> list[str]:
        needle = needle.lower().replace("\\", "/")
        return sorted(name for name in self.entries if needle in name)


class Nif:
    """Header + block table of a Skyrim NIF, and the block bodies on demand."""

    def __init__(self, data: bytes) -> None:
        pos = data.index(b"\n") + 1
        self.version, endian, self.user_version, block_count, self.bethesda_version = struct.unpack_from(
            "<IBIII", data, pos
        )
        pos += 17
        if endian != 1:
            raise SystemExit("big-endian NIF")
        for _ in range(3):  # author, process script, export script
            length = data[pos]
            pos += 1 + length
        type_count = struct.unpack_from("<H", data, pos)[0]
        pos += 2
        self.block_types = []
        for _ in range(type_count):
            length = struct.unpack_from("<I", data, pos)[0]
            pos += 4
            self.block_types.append(data[pos : pos + length].decode("latin1").rstrip("\0"))
            pos += length
        self.type_index = list(struct.unpack_from(f"<{block_count}H", data, pos))
        pos += block_count * 2
        self.sizes = list(struct.unpack_from(f"<{block_count}I", data, pos))
        pos += block_count * 4
        string_count = u32(data, pos)
        pos += 8
        self.strings = []
        for _ in range(string_count):
            length = struct.unpack_from("<I", data, pos)[0]
            pos += 4
            self.strings.append(data[pos : pos + length].decode("latin1").rstrip("\0"))
            pos += length
        group_count = u32(data, pos)
        pos += 4 + group_count * 4

        self.bodies: list[bytes] = []
        for size in self.sizes:
            self.bodies.append(data[pos : pos + size])
            pos += size
        self.block_count = block_count

    def type_of(self, index: int) -> str:
        return self.block_types[self.type_index[index]]

    def string(self, index: int) -> str:
        if index == 0xFFFFFFFF or index >= len(self.strings):
            return f"<{index}>"
        return self.strings[index]

    def by_type(self, name: str) -> list[int]:
        return [i for i in range(self.block_count) if self.type_of(i) == name]

    def histogram(self) -> list[tuple[str, int]]:
        counts: dict[str, int] = {}
        for i in range(self.block_count):
            counts[self.type_of(i)] = counts.get(self.type_of(i), 0) + 1
        return sorted(counts.items(), key=lambda item: (-item[1], item[0]))


# ---------------------------------------------------------------------------------------
# The animation blocks, by hand (all three are `Unhandled` in the vendored parser).
# ---------------------------------------------------------------------------------------

KEY_FLOATS = {1: 1, 2: 1, 3: 1, 4: 3, 5: 1}  # linear/quadratic/tbc/xyz/const -> value floats
KEY_EXTRA = {1: 0, 2: 8, 3: 12, 4: 0, 5: 0}  # additional bytes per key (tangents, t/b/c)


def parse_quat_keys(body: bytes, pos: int, key_type: int) -> tuple[list[tuple[float, tuple[float, ...]]], int]:
    count = u32(body, pos)
    pos += 4
    keys = []
    for _ in range(count):
        time = f32(body, pos)
        pos += 4
        if key_type == 4:  # XYZ rotation: three euler angles, not a quaternion
            keys.append((time, (f32(body, pos), f32(body, pos + 4), f32(body, pos + 8))))
            pos += 12
        else:
            keys.append((time, struct.unpack_from("<4f", body, pos)))
            pos += 16 + KEY_EXTRA.get(key_type, 0.0)
    return keys, pos


def parse_key_group(body: bytes, pos: int) -> tuple[dict, int]:
    """nif.xml KeyGroup: Num Keys uint, Key Type uint, then the keys."""
    count, key_type = u32(body, pos), u32(body, pos + 4)
    pos += 8
    keys = []
    for _ in range(count):
        time = f32(body, pos)
        value = struct.unpack_from("<f", body, pos + 4)
        pos += 8 + KEY_EXTRA.get(key_type, 0)
        keys.append((time, value))
    return {"count": count, "type": key_type, "keys": keys}, pos


def parse_transform_data(body: bytes) -> dict:
    """NiTransformData, fitted against the Dwemer door files.

    ``(rotation type, number of rotation key sets)`` is written first; for
    ``XYZ_ROTATION_KEY`` (type 4, what every door checked here uses) the keys are then three
    :func:`parse_key_group` curves - one per euler axis, in X, Y, Z order - and nothing else
    carries keys: every block ends with one more two-word group header whose count is zero.
    """
    rotation_count = u32(body, 0)
    rotation_type = u32(body, 4)
    pos = 8
    rotation = []
    for _ in range(rotation_count * (3 if rotation_type == 4 else 1)):
        group, pos = parse_key_group(body, pos)
        rotation.append(group)
    rest = [u32(body, pos + offset) for offset in range(0, len(body) - pos, 4)]
    return {
        "rotation_type": rotation_type,
        "rotation_count": rotation_count,
        "rotation": rotation,
        "trailing": rest,
        "consumed": len(body),
    }


def parse_transform_interpolator(body: bytes) -> dict:
    """nif.xml NiTransformInterpolator: translation vec3, rotation quat, scale, data ref."""
    return {
        "translation": struct.unpack_from("<3f", body, 0),
        "rotation": struct.unpack_from("<4f", body, 12),
        "scale": f32(body, 28),
        "data": u32(body, 32),
        "consumed": 36,
    }


def parse_controller_sequence(body: bytes) -> dict:
    name_index = u32(body, 0)
    num_controlled = u32(body, 4)
    pos = 12  # name, num_controlled_blocks, array_grow_by
    controlled = []
    for _ in range(num_controlled):
        interpolator, controller, priority = struct.unpack_from("<IIB", body, pos)
        node_name, property_type, controller_type, controller_id, interpolator_id = struct.unpack_from(
            "<5I", body, pos + 9
        )
        controlled.append(
            {
                "interpolator": interpolator,
                "controller": controller,
                "priority": priority,
                "node_name": node_name,
                "property_type": property_type,
                "controller_type": controller_type,
                "controller_id": controller_id,
                "interpolator_id": interpolator_id,
            }
        )
        pos += 29
    # nif.xml: Weight float, Text Keys ref, Cycle Type uint, Frequency/Start/Stop float,
    # Manager ptr, Accum Root Name string index, Num Anim Note Arrays uint, then the array.
    weight = f32(body, pos)
    text_keys = u32(body, pos + 4)
    cycle_type = u32(body, pos + 8)
    frequency, start_time, stop_time = struct.unpack_from("<3f", body, pos + 12)
    manager, accum_root = struct.unpack_from("<2I", body, pos + 24)
    # The note array count is a ushort, not the uint nif.xml declares: the blocks are exactly
    # 102 bytes of fields + 2 bytes of count, which is how the vendored parser reads it too.
    anim_note_count = struct.unpack_from("<H", body, pos + 32)[0]
    notes_pos = pos + 34
    return {
        "name_index": name_index,
        "controlled": controlled,
        "weight": weight,
        "text_keys": text_keys,
        "cycle_type": cycle_type,
        "frequency": frequency,
        "start_time": start_time,
        "stop_time": stop_time,
        "manager": manager,
        "accum_root": accum_root,
        "anim_note_count": anim_note_count,
        "anim_notes": list(struct.unpack_from(f"<{anim_note_count}I", body, notes_pos))
        if anim_note_count
        else [],
        "consumed": notes_pos + anim_note_count * 4,
    }


KEY_TAIL = {1: 0, 2: 8, 3: 12, 4: 0, 5: 0}  # extra bytes per key for each KeyType


def try_layout(body: bytes, order: str, sizes: tuple[int, int, int]) -> tuple[int, list] | None:
    """Consume a NiTransformData body as rotation/translation/scale groups.

    Each group is a two-u32 header and a run of (float time + value floats) keys. `order`
    says whether those two u32 are (count, type) or (type, count); `sizes` is the value
    width in bytes of one key for each group.
    """
    pos = 0
    groups = []
    for size in sizes:
        if pos + 8 > len(body):
            return None
        first, second = u32(body, pos), u32(body, pos + 4)
        count, key_type = (first, second) if order == "count_type" else (second, first)
        pos += 8
        if key_type not in KEY_TAIL or count > 64:
            return None
        stride = 4 + size + KEY_TAIL[key_type]
        if pos + count * stride > len(body):
            return None
        keys = []
        for _ in range(count):
            keys.append((f32(body, pos), struct.unpack_from(f"<{size // 4}f", body, pos + 4)))
            pos += stride
        groups.append({"count": count, "type": key_type, "keys": keys})
    return (pos, groups) if pos == len(body) else None


def fit_layout(bodies: dict[int, bytes]) -> None:
    """Which (header order, key widths) consumes every block exactly."""
    print("\nlayout search over Rotation / Translation / Scale groups:")
    for order in ("count_type", "type_count"):
        for rotation in (12, 16, 4):
            for translation in (12, 4):
                for scale in (4,):
                    sizes = (rotation, translation, scale)
                    fits = {}
                    for index, body in bodies.items():
                        fits[index] = try_layout(body, order, sizes)
                    if all(result is not None for result in fits.values()):
                        print(f"  FITS order={order} key value floats: rot={rotation // 4} "
                              f"trans={translation // 4} scale={scale // 4}")
                        for index, result in fits.items():
                            assert result is not None
                            pos, groups = result
                            summary = " ".join(
                                f"{name}(n={g['count']},type={g['type']},"
                                f"{[tuple(round(v, 4) for v in key[1]) for key in g['keys']]})".replace(
                                    "(", "(", 1
                                )
                                for name, g in zip(("rot", "trans", "scale"), groups)
                            )
                            print(f"    data[{index}] {len(bodies[index])}B {summary}")


def hexdump(data: bytes, limit: int = 64) -> str:
    return " ".join(f"{b:02x}" for b in data[:limit]) + (" ..." if len(data) > limit else "")


def describe(nif: Nif, label: str) -> None:
    print(f"\n=== {label} ===")
    print(f"version {nif.version >> 24}.{(nif.version >> 16) & 0xff}.{(nif.version >> 8) & 0xff}.{nif.version & 0xff}"
          f" user {nif.user_version} bethesda {nif.bethesda_version} blocks {nif.block_count}")
    hist = nif.histogram()
    print("block types: " + ", ".join(f"{name}x{count}" for name, count in hist))
    nodes = [i for i in range(nif.block_count) if nif.type_of(i) in ("NiNode", "BSFadeNode")]
    print("node blocks: " + ", ".join(f"{i}:{nif.string(nif.bodies[i] and u32(nif.bodies[i], 0))}"
                                      for i in nodes))
    print("strings: " + ", ".join(f"{i}={name!r}" for i, name in enumerate(nif.strings)))

    managers = nif.by_type("NiControllerManager")
    sequences = nif.by_type("NiControllerSequence")
    palettes = nif.by_type("NiDefaultAVObjectPalette")
    multi = nif.by_type("NiMultiTargetTransformController")
    xforms = nif.by_type("NiTransformController")
    interps = nif.by_type("NiTransformInterpolator")
    data = nif.by_type("NiTransformData")
    print(f"animation blocks: managers={managers} sequences={sequences} palettes={palettes} "
          f"multi_target={multi} transform_controllers={xforms} "
          f"transform_interpolators={interps} transform_data={data}")

    for index in sequences:
        body = nif.bodies[index]
        try:
            sequence = parse_controller_sequence(body)
        except Exception as exc:  # noqa: BLE001 - a research dump, report and carry on
            print(f"  sequence {index}: parse failed: {exc}; bytes {hexdump(body)}")
            continue
        fits = "" if sequence["consumed"] == len(body) else f" MISMATCH consumed={sequence['consumed']}/{len(body)}"
        print(
            f"  sequence {index}: name={nif.string(sequence['name_index'])!r} "
            f"cycle={sequence['cycle_type']} range=[{sequence['start_time']:.4f},"
            f"{sequence['stop_time']:.4f}] weight={sequence['weight']:.3f} "
            f"text_keys=block{sequence['text_keys']} "
            f"manager={sequence['manager']} controlled={len(sequence['controlled'])}{fits}"
        )
        for block in sequence["controlled"]:
            node = nif.string(block["node_name"])
            controller_index = block["controller"]
            controller_type = (
                nif.type_of(controller_index) if controller_index < nif.block_count else "?"
            )
            interpolator_index = block["interpolator"]
            interpolator_type = (
                nif.type_of(interpolator_index) if interpolator_index < nif.block_count else "?"
            )
            print(
                f"    node={node!r} controller={controller_index}({controller_type}) "
                f"interpolator={interpolator_index}({interpolator_type}) "
                f"controller_id={nif.string(block['controller_id'])!r} "
                f"interpolator_id={nif.string(block['interpolator_id'])!r} "
                f"property={block['property_type']} priority={block['priority']}"
            )
            if interpolator_type == "NiTransformInterpolator" and interpolator_index < nif.block_count:
                interp_body = nif.bodies[interpolator_index]
                try:
                    interp = parse_transform_interpolator(interp_body)
                except Exception as exc:  # noqa: BLE001
                    print(f"      interpolator parse failed: {exc}")
                    continue
                print(
                    f"      rest T={interp['translation']} R={interp['rotation']} "
                    f"S={interp['scale']} data={interp['data']}"
                )
                data_index = interp["data"]
                if data_index < nif.block_count and nif.type_of(data_index) == "NiTransformData":
                    dbody = nif.bodies[data_index]
                    try:
                        keys = parse_transform_data(dbody)
                    except Exception as exc:  # noqa: BLE001
                        print(f"      transform data parse failed: {exc}; bytes {hexdump(dbody)}")
                        continue
                    print(
                        f"      transform data {data_index} size={len(dbody)} "
                        f"rotation_type={keys['rotation_type']} "
                        f"sets={keys['rotation_count']} trailing={keys['trailing']}"
                    )
                    for axis, group in zip("XYZ", keys["rotation"]):
                        print(
                            f"        {axis}: {group['count']} keys type={group['type']} "
                            + ", ".join(
                                f"t={time:.4f}->{value[0]:.5f}" for time, value in group["keys"][:6]
                            )
                        )

    for index in managers:
        print(f"  manager {index}: raw {hexdump(nif.bodies[index])}")
    for index in multi:
        print(f"  multi-target controller {index}: raw {hexdump(nif.bodies[index])}")
    for index in xforms:
        print(f"  transform controller {index}: raw {hexdump(nif.bodies[index])}")
    for index in palettes:
        print(f"  palette {index}: raw {hexdump(nif.bodies[index])}")


def dump_glb(path: Path) -> None:
    """What the converter's glTF has: node names, transforms, and whether it animates."""
    data = path.read_bytes()
    length = struct.unpack_from("<I", data, 8)[0]
    chunk_length, chunk_type = struct.unpack_from("<II", data, 12)
    assert chunk_type == 0x4E4F534A, "first chunk is not JSON"
    document = json.loads(data[20 : 20 + chunk_length])
    print(f"\n=== {path.name} ({length} bytes) ===")
    print(f"asset: {document.get('asset')}")
    print(f"animations: {document.get('animations', [])!r}")
    print(f"skins: {len(document.get('skins', []))} meshes: {len(document.get('meshes', []))}")
    for index, node in enumerate(document.get("nodes", [])):
        print(
            f"  node[{index}] name={node.get('name')!r} mesh={node.get('mesh')} "
            f"children={node.get('children')} T={node.get('translation')} "
            f"R={node.get('rotation')} S={node.get('scale')}"
        )


def census(root: Path, needle: str = "") -> None:
    """How many NIFs under `root` carry an Open/Close controller sequence, and how many of
    their transform-data blocks the fitted layout accounts for."""
    totals = {"files": 0, "animated": 0, "open_close": 0, "data_blocks": 0, "fitted": 0}
    names: dict[str, int] = {}
    # The words after the rotation curves: how long, and with what values. All-zero means the
    # remaining groups (translation, scale) carry no keys; anything else would be animation the
    # rotation-only reading is dropping.
    trailing: dict[int, set] = {}
    axis_motion: list[tuple[str, str, float]] = []
    durations: list[float] = []
    failures: list[str] = []
    for path in sorted(root.rglob("*.nif")):
        if needle and needle not in str(path).lower():
            continue
        totals["files"] += 1
        try:
            nif = Nif(path.read_bytes())
        except Exception:  # noqa: BLE001 - a census over 20k files: skip what will not parse
            continue
        sequences = nif.by_type("NiControllerSequence")
        if not sequences:
            continue
        totals["animated"] += 1
        labels = set()
        for index in sequences:
            try:
                sequence = parse_controller_sequence(nif.bodies[index])
            except Exception:  # noqa: BLE001
                continue
            label = nif.string(sequence["name_index"])
            labels.add(label)
            names[label] = names.get(label, 0) + 1
            if sequence["stop_time"] > 0.0:
                durations.append(sequence["stop_time"])
            for block in sequence["controlled"]:
                interpolator = block["interpolator"]
                if interpolator >= nif.block_count:
                    continue
                if nif.type_of(interpolator) != "NiTransformInterpolator":
                    continue
                data_index = u32(nif.bodies[interpolator], 32)
                if data_index >= nif.block_count:
                    continue
                if nif.type_of(data_index) != "NiTransformData":
                    continue
                totals["data_blocks"] += 1
                body = nif.bodies[data_index]
                try:
                    parsed = parse_transform_data(body)
                    consumed = 8 + sum(
                        8 + group["count"] * (8 + KEY_EXTRA.get(group["type"], 0))
                        for group in parsed["rotation"]
                    ) + 4 * len(parsed["trailing"])
                except Exception:  # noqa: BLE001
                    failures.append(f"{path.name}#{data_index}")
                    continue
                if consumed != len(body):
                    failures.append(f"{path.name}#{data_index}({consumed}/{len(body)})")
                    continue
                totals["fitted"] += 1
                trailing.setdefault(len(parsed["trailing"]), set()).add(
                    tuple(parsed["trailing"])
                )
                if parsed["rotation_type"] != 4:
                    continue
                node = nif.string(block["node_name"])
                for axis, group in zip("XYZ", parsed["rotation"]):
                    swing = max(key[1][0] for key in group["keys"]) - min(
                        key[1][0] for key in group["keys"]
                    )
                    if swing > 1.0e-3:
                        axis_motion.append((path.name, f"{node}.{axis}", swing))
        if "Open" in labels and "Close" in labels:
            totals["open_close"] += 1
    print(f"census of {root}{f' matching {needle!r}' if needle else ''}: {totals}")
    print("sequence names: " + ", ".join(
        f"{name!r}x{count}" for name, count in sorted(names.items(), key=lambda kv: -kv[1])[:20]
    ))
    if durations:
        durations.sort()
        print(f"sequence durations: min={durations[0]:.4f} median="
              f"{durations[len(durations) // 2]:.4f} max={durations[-1]:.4f} over {len(durations)}")
    for size, values in sorted(trailing.items()):
        print(f"  {size} trailing words ({size * 4} bytes) x{len(values)} distinct: "
              f"{sorted(values)[:4]}"
              + (" ..." if len(values) > 4 else ""))
    moving = sorted(axis_motion, key=lambda item: -item[2])
    print(f"animated axes (swing > 0.001 rad): {len(axis_motion)}; largest:")
    for name, axis, swing in moving[:25]:
        print(f"  {swing:7.4f} rad ({swing * 57.2958:7.2f} deg)  {name}  {axis}")
    if failures:
        print(f"{len(failures)} transform-data blocks did not fit; first 10: {failures[:10]}")


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 2
    if argv[1] == "census":
        census(Path(argv[2]), argv[3].lower() if len(argv) > 3 else "")
        return 0
    if argv[1] == "glb":
        for name in argv[2:]:
            dump_glb(Path(name))
        return 0
    if argv[1] == "debug":
        # Full bytes of every animation block, for fitting a layout by hand.
        for name in argv[2:]:
            nif = Nif(Path(name).read_bytes())
            print(f"\n=== {name} ===")
            fit_layout(
                {
                    index: nif.bodies[index]
                    for index in nif.by_type("NiTransformData")
                }
            )
            for type_name in (
                "NiTransformInterpolator",
                "NiTransformData",
                "NiControllerSequence",
                "NiControllerManager",
                "NiMultiTargetTransformController",
                "NiTextKeyExtraData",
                "NiDefaultAVObjectPalette",
            ):
                for index in nif.by_type(type_name):
                    body = nif.bodies[index]
                    print(f"  {type_name}[{index}] size={len(body)}")
                    for start in range(0, len(body), 32):
                        chunk = body[start : start + 32]
                        print(f"    {start:4d}: {hexdump(chunk, 32)}")
        return 0
    if len(argv) < 2:
        return 2
    # A loose .nif (the converter's extracted staging tree) reads straight off disk; a BSA
    # needs the archive index, which the rest of this file implements.
    if argv[1] == "nif":
        for name in argv[2:]:
            describe(Nif(Path(name).read_bytes()), name)
        return 0
    if len(argv) < 3:
        print(__doc__)
        return 2
    command, archive = argv[1], Path(argv[2])
    bsa = Bsa(archive)
    print(f"{archive.name}: version {bsa.version} flags {bsa.flags:#06x} entries {len(bsa.entries)}")
    if command == "list":
        for name in bsa.find(argv[3]):
            print(" ", name)
        return 0
    if command == "dump":
        describe(Nif(bsa.read(argv[3])), argv[3])
        return 0
    if command == "doors":
        names = [n for n in argv[3:] if n] or ROUTE_MODELS
        for name in names:
            key = name.lower()
            if key not in bsa.entries:
                print(f"\n=== {name} === NOT IN THIS ARCHIVE")
                continue
            describe(Nif(bsa.read(key)), key)
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
