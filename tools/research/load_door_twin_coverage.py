#!/usr/bin/env python3
"""How many of the game's load doors have a non-load twin whose clips could be borrowed.

impl-052 wants every load door to swing like a real door: a load door's own `Open` clip barely
moves (the demo route's `DweDoorLarge01Load` turns its leaves 5-9 degrees), so when it does not
clear the doorway the engine looks for the door's non-load twin model and plays *its* clips
instead. This script derives the path rule from the data rather than from one example, and reports
the coverage.

Commands:

    models                 the load-door models in the converted database, the twin each derives,
                           and the coverage summary
    nodes <load> <twin>    the node name paths of two converted GLBs, and which paths they share

Read-only: the database is opened with `mode=ro` and nothing is written.
"""

from __future__ import annotations

import json
import math
import re
import sqlite3
import struct
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import door_animation_nif as nif_tool  # noqa: E402 - the verified NIF header decoder

DATABASE = "C:/Modding/SkyrimConverted/skyrim_world.db"
CONVERTED = Path("C:/Modding/SkyrimConverted")

# The marker that makes a door model a load door: its filename carries "load" right where the
# non-load twin has nothing. Everything else about the two paths is the same file name, so the
# rule is "delete the first `load` token from the file stem".
LOAD_TOKEN = re.compile(r"load", re.IGNORECASE)


def twin_path(model_path: str) -> str | None:
    """The non-load twin of a door model path, or None when the name carries no `load` marker."""
    normalized = model_path.replace("\\", "/")
    directory, _, filename = normalized.rpartition("/")
    stem, dot, extension = filename.rpartition(".")
    if not dot:
        stem, extension = filename, ""
    match = LOAD_TOKEN.search(stem)
    if match is None:
        return None
    twin_stem = stem[: match.start()] + stem[match.end() :]
    twin_name = f"{twin_stem}.{extension}" if extension else twin_stem
    return f"{directory}/{twin_name}" if directory else twin_name


def load_door_models(connection: sqlite3.Connection) -> Counter:
    """Every model a load door reference uses, counted by reference.

    A load door is a reference with a `door_links` row: that row is its `XTEL`, and a door without
    one never moves the player anywhere.
    """
    return Counter(
        {
            model: count
            for model, count in connection.execute(
                """
                SELECT s.model_path, COUNT(*)
                FROM "references" r
                JOIN statics s ON s.id = r.base_form_id
                JOIN door_links l ON l.ref_id = r.id
                WHERE s.model_path IS NOT NULL
                GROUP BY s.model_path
                ORDER BY COUNT(*) DESC
                """
            )
        }
    )


def key(model_path: str) -> str:
    """A model path as a lookup key: one separator, one case. The database stores backslashes and
    the converter lowercases the file name, so every comparison goes through this."""
    return model_path.replace("\\", "/").lower()


def static_models(connection: sqlite3.Connection) -> set[str]:
    return {
        key(row[0])
        for row in connection.execute("SELECT DISTINCT model_path FROM statics WHERE model_path IS NOT NULL")
    }


def converted_exists(model_path: str) -> bool:
    """Whether the model is on disk as a converted GLB, the way the engine loads it."""
    normalized = key(model_path).removeprefix("meshes/")
    return (CONVERTED / "meshes" / normalized).with_suffix(".glb").exists()


def source_exists(model_path: str) -> bool:
    """Whether the model is in the converter's view of the game's own files."""
    normalized = key(model_path).removeprefix("meshes/")
    return (CONVERTED / "vfs" / "meshes" / normalized).exists()


def glb_document(path: Path) -> dict:
    """The JSON chunk of a GLB."""
    with path.open("rb") as handle:
        header = handle.read(12)
        magic, _version, _length = struct.unpack("<III", header)
        assert magic == 0x46546C67, f"{path} is not a GLB"
        chunk_length, chunk_type = struct.unpack("<II", handle.read(8))
        assert chunk_type == 0x4E4F534A, f"{path} has no JSON chunk first"
        return json.loads(handle.read(chunk_length))


def node_paths(document: dict) -> dict[str, list[str]]:
    """Every node's name path from its scene root, keyed by the path joined with `/`."""
    nodes = document.get("nodes", [])
    paths: dict[str, list[str]] = {}

    def walk(index: int, path: list[str]) -> None:
        name = nodes[index].get("name", f"#{index}")
        here = path + [name]
        paths["/".join(here)] = here
        for child in nodes[index].get("children", []):
            walk(child, here)

    for scene in document.get("scenes", []):
        for root in scene.get("nodes", []):
            walk(root, [])
    return paths


def models_command() -> int:
    connection = sqlite3.connect(f"file:{DATABASE}?mode=ro", uri=True)
    doors = load_door_models(connection)
    statics = static_models(connection)
    connection.close()

    print(f"load door references: {sum(doors.values())} over {len(doors)} distinct models")
    patterns: Counter = Counter()
    covered = uncovered = no_marker = 0
    covered_refs = uncovered_refs = no_marker_refs = 0
    rows = []
    for model, count in sorted(doors.items(), key=lambda item: -item[1]):
        twin = twin_path(model)
        twin_key = key(twin) if twin is not None else None
        if twin is None:
            verdict = "no load marker"
            no_marker += 1
            no_marker_refs += count
        elif twin_key in statics and converted_exists(twin):
            verdict = "twin"
            covered += 1
            covered_refs += count
        elif twin_key in statics and source_exists(twin):
            verdict = "twin not converted"
            uncovered += 1
            uncovered_refs += count
        elif source_exists(twin):
            verdict = "twin in the game, no static row"
            uncovered += 1
            uncovered_refs += count
        else:
            verdict = "nothing at that path"
            uncovered += 1
            uncovered_refs += count
        rows.append((count, model, twin, verdict))
        if twin is not None:
            normalized = model.replace("\\", "/")
            stem = normalized.rpartition("/")[2].rpartition(".")[0]
            match = LOAD_TOKEN.search(stem)
            if match is not None:
                patterns[stem[match.start() :]] += count

    print("\npattern after the load marker (by reference count):")
    for pattern, count in patterns.most_common():
        print(f"  {count:6d}  ...{pattern}")

    print("\ncoverage:")
    print(f"  {covered:4d} models / {covered_refs:6d} references have a converted twin at the "
          f"derived path")
    print(f"  {uncovered:4d} models / {uncovered_refs:6d} references have no converted twin")
    print(f"  {no_marker:4d} models / {no_marker_refs:6d} references have no load marker at all")

    print("\nmodels with a load marker but no converted twin (top 25):")
    for count, model, twin, verdict in rows:
        if verdict.startswith("nothing") or verdict == "twin (no glb)":
            print(f"  {count:6d}  {model}  ->  {twin}  [{verdict}]")
    print("\nall load-door models (top 40 by reference count):")
    for count, model, twin, verdict in rows[:40]:
        print(f"  {count:6d}  {model}  ->  {twin}  [{verdict}]")
    return 0


def nodes_command(load_path: str, twin_path_arg: str) -> int:
    load = node_paths(glb_document(Path(load_path)))
    twin = node_paths(glb_document(Path(twin_path_arg)))
    print(f"{load_path}: {len(load)} nodes")
    for path in load:
        print(f"  {path}")
    print(f"\n{twin_path_arg}: {len(twin)} nodes")
    for path in twin:
        print(f"  {path}")
    print("\nsame name path in both:", sorted(set(load) & set(twin)))
    print(
        "same last name in both:",
        sorted({p.rpartition('/')[2] for p in load} & {p.rpartition('/')[2] for p in twin}),
    )
    return 0


def nif_nodes(path: Path) -> list[str]:
    """Every node block's name in a NIF."""
    nif = nif_tool.Nif(path.read_bytes())
    return [
        nif.string(nif_tool.u32(nif.bodies[index], 0))
        for index in range(nif.block_count)
        if nif.type_of(index) in ("NiNode", "BSFadeNode")
    ]


def nif_open_targets(path: Path) -> list[str]:
    """The nodes the NIF's `Open` controller sequence animates, by name."""
    nif = nif_tool.Nif(path.read_bytes())
    targets = []
    for index in nif.by_type("NiControllerSequence"):
        sequence = nif_tool.parse_controller_sequence(nif.bodies[index])
        name = nif.string(sequence["name_index"])
        if name.strip().lower() != "open":
            continue
        targets.extend(nif.string(block["node_name"]) for block in sequence["controlled"])
    return targets


def nif_open_swing(path: Path) -> float:
    """The widest angle the NIF's `Open` sequence turns any node by, in degrees."""
    nif = nif_tool.Nif(path.read_bytes())
    widest = 0.0
    for index in nif.by_type("NiControllerSequence"):
        sequence = nif_tool.parse_controller_sequence(nif.bodies[index])
        if nif.string(sequence["name_index"]).strip().lower() != "open":
            continue
        for block in sequence["controlled"]:
            interpolator = block["interpolator"]
            if interpolator >= nif.block_count:
                continue
            if nif.type_of(interpolator) != "NiTransformInterpolator":
                continue
            data = nif_tool.parse_transform_interpolator(nif.bodies[interpolator])["data"]
            if data >= nif.block_count or nif.type_of(data) != "NiTransformData":
                continue
            keys = nif_tool.parse_transform_data(nif.bodies[data])
            if keys["rotation_type"] != 4:
                continue
            for group in keys["rotation"]:
                if not group["keys"]:
                    continue
                values = [key[1][0] for key in group["keys"]]
                widest = max(widest, abs(max(values) - min(values)))
    return widest * 57.2958


def pairs_command() -> int:
    """For every load door with a converted non-load twin: do the twin's `Open` clips name nodes
    that exist in the load door's own model? That is the check the engine makes before borrowing."""
    connection = sqlite3.connect(f"file:{DATABASE}?mode=ro", uri=True)
    doors = load_door_models(connection)
    statics = static_models(connection)
    connection.close()

    source = CONVERTED / "vfs" / "meshes"
    pass_models = pass_refs = fail_models = fail_refs = 0
    table = []
    for model, count in sorted(doors.items(), key=lambda item: -item[1]):
        twin = twin_path(model)
        if twin is None or key(twin) not in statics:
            continue
        load_nif = source / key(model).removeprefix("meshes/")
        twin_nif = source / key(twin).removeprefix("meshes/")
        if not (load_nif.exists() and twin_nif.exists()):
            continue
        load_nodes = nif_nodes(load_nif)
        targets = nif_open_targets(twin_nif)
        missing = [target for target in targets if target not in load_nodes]
        ok = bool(targets) and not missing
        load_root = load_nodes[0] if load_nodes else "?"
        twin_root = nif_nodes(twin_nif)[0] if nif_nodes(twin_nif) else "?"
        expected_root = twin_root_name(load_root)
        load_swing = nif_open_swing(load_nif)
        twin_swing = nif_open_swing(twin_nif)
        if ok:
            pass_models += 1
            pass_refs += count
        else:
            fail_models += 1
            fail_refs += count
        table.append(
            (count, model, load_nodes, targets, missing, load_root, twin_root, expected_root,
             load_swing, twin_swing)
        )

    print(f"pairs checked (a load model with a converted twin): {len(table)}")
    print(f"  {pass_models:3d} models / {pass_refs:5d} references: the twin's Open clip names only "
          f"nodes the load model has, so the door borrows its swing")
    print(f"  {fail_models:3d} models / {fail_refs:5d} references: at least one name is missing, so "
          f"the door keeps its own clip and that is the clip the engine scales (path 2)")
    print("\nroot node names: does the twin's root equal the load root with the marker removed?")
    roots_ok = sum(1 for row in table if row[7] == row[6])
    print(f"  {roots_ok}/{len(table)} pairs agree")
    for count, model, load_nodes, targets, missing, load_root, twin_root, expected, _l, _t in table:
        if expected != twin_root:
            print(f"  MISMATCH {load_root!r} -> expected {expected!r}, twin is {twin_root!r}")
    print("\nswings: the door's own Open clip against its twin's, where the names fit")
    borrowed_refs = 0
    for count, model, _nodes, _targets, missing, _lr, _tr, _er, load_swing, twin_swing in table:
        if missing or not _targets:
            continue
        borrow = load_swing < 45.0
        borrowed_refs += count if borrow else 0
        print(f"  {count:4d} refs  own {load_swing:6.1f} deg, twin {twin_swing:6.1f} deg  "
              f"{'-> the door would borrow' if borrow else '-> own clip already opens it'}")
    print(f"  references that would borrow a swing: {borrowed_refs}")
    print("\ntable:")
    for (
        count, model, load_nodes, targets, missing, load_root, twin_root, expected, load_swing,
        twin_swing,
    ) in table:
        verdict = "borrow" if not missing and targets and load_swing < 45.0 else "keep own clip"
        print(f"  {count:4d} refs {model}\n"
              f"        load nodes {load_nodes}  own Open swing {load_swing:.1f} deg\n"
              f"        twin Open targets {targets}  swing {twin_swing:.1f} deg  "
              f"missing {missing}  -> {verdict}")
    return 0


def swings_command() -> int:
    """What every load door's own `Open` clip turns its widest leaf by, and what the engine does
    with it: borrow a twin's swing, scale its own up, or leave it alone.

    The engine's three cases, in the order it tries them:
      1. its own swing is 45 degrees or more - already a doorway, nothing is done;
      2. its own swing is under 45 and its non-load twin's clips move nodes it has - borrowed;
      3. otherwise its own swing is scaled by min(90 / swing, 20) - the cap means a door whose own
         swing is under 2.25 degrees still comes out short of 45.
    A model with no `Open` sequence at all - or one that turns nothing - has no clip to scale and
    stays the static door it is today.
    """
    connection = sqlite3.connect(f"file:{DATABASE}?mode=ro", uri=True)
    doors = load_door_models(connection)
    statics = static_models(connection)
    connection.close()

    source = CONVERTED / "vfs" / "meshes"
    groups: dict[str, list] = {}
    for model, count in sorted(doors.items(), key=lambda item: -item[1]):
        load_nif = source / key(model).removeprefix("meshes/")
        if not load_nif.exists():
            groups.setdefault("no model on disk", []).append((count, model, None, None))
            continue
        swing = nif_open_swing(load_nif)
        factor = None
        if swing >= 45.0:
            group = "own clip already opens the doorway"
        elif swing <= 0.0:
            group = "no rotation to swing: as today"
        else:
            factor = min(90.0 / swing, 20.0)
            twin = twin_path(model)
            twin_nif = source / key(twin).removeprefix("meshes/") if twin else None
            fits = False
            if twin_nif is not None and twin_nif.exists() and key(twin) in statics:
                load_nodes = nif_nodes(load_nif)
                targets = nif_open_targets(twin_nif)
                fits = bool(targets) and all(target in load_nodes for target in targets)
            if fits:
                group = "borrows its twin's swing (path 1)"
            else:
                group = (
                    "scales its own swing (path 2), capped"
                    if factor >= 20.0
                    else "scales its own swing (path 2)"
                )
        groups.setdefault(group, []).append((count, model, swing, factor))

    order = [
        "borrows its twin's swing (path 1)",
        "scales its own swing (path 2)",
        "scales its own swing (path 2), capped",
        "own clip already opens the doorway",
        "no rotation to swing: as today",
        "no model on disk",
    ]
    total_models = sum(len(rows) for rows in groups.values())
    total_refs = sum(row[0] for rows in groups.values() for row in rows)
    print(f"load-door models: {total_models}, references: {total_refs}")
    for group in order:
        rows = groups.get(group, [])
        refs = sum(row[0] for row in rows)
        print(f"  {refs:5d} refs / {len(rows):3d} models  {group}")
    for group, rows in groups.items():
        if group not in order:
            refs = sum(row[0] for row in rows)
            print(f"  {refs:5d} refs / {len(rows):3d} models  {group}")

    print("\nwhere the scaling happens, by own swing (the widest leaf):")
    scaled = [
        row
        for group, rows in groups.items()
        if group.startswith("scales")
        for row in rows
    ]
    for count, model, swing, factor in sorted(scaled, key=lambda row: row[2])[:12]:
        print(f"  {count:4d} refs  own {swing:5.1f} deg  factor {factor:5.2f} "
              f"-> {swing * factor:5.1f} deg  {model}")

    print("\nmodels whose own `Open` clip turns nothing (the top 20 by reference count):")
    for count, model, swing, factor in sorted(
        groups.get("no rotation to swing: as today", []), key=lambda row: -row[0]
    )[:20]:
        print(f"  {count:4d} refs  {model}")

    print("\nthe demo route's own door:")
    for count, model, swing, factor in [
        row for group, rows in groups.items() for row in rows
        if "DwemerLargeDoorLoad01" in row[1]
    ]:
        print(f"  {count} references, own swing {swing:.2f} deg, factor {factor:.2f}, "
              f"opens to {swing * factor:.1f} deg")
    return 0


def twin_root_name(root: str) -> str:
    """The load door's own root node name with the marker removed: the twin's root node name."""
    match = LOAD_TOKEN.search(root)
    if match is None:
        return root
    return root[: match.start()] + root[match.end() :]


def clip_command(paths: list[str]) -> int:
    """What a converted model's baked `Open`/`Close` clips turn their nodes by.

    The engine measures a door's swing as the angle between a leaf's first and last pose
    (`swing_degrees`), which is what decides whether the doorway opens and what a scaled clip is
    scaled by. This reads the same numbers straight out of the GLB: the animation samplers' rotation
    outputs, and the node each channel targets by name.
    """
    for name in paths:
        path = Path(name)
        document = glb_document(path)
        binary = glb_binary(path)
        nodes = document.get("nodes", [])
        print(f"\n{name}")
        for animation in document.get("animations", []):
            swings = []
            for channel in animation["channels"]:
                if channel["target"]["path"] != "rotation":
                    continue
                node = nodes[channel["target"]["node"]].get("name", "?")
                sampler = animation["samplers"][channel["sampler"]]
                values = accessor_floats(document, binary, sampler["output"])
                times = accessor_floats(document, binary, sampler["input"])
                degrees = quat_angle(values[:4], values[-4:])
                swings.append((degrees, node, times[0], times[-1]))
            if not swings:
                print(f"  {animation.get('name', '?')}: no rotation channels")
                continue
            widest = max(swings, key=lambda row: row[0])
            print(f"  {animation.get('name', '?')}: {len(swings)} rotation channels, "
                  f"widest {widest[0]:.2f} deg on {widest[1]!r} "
                  f"(t {widest[2]:.4f}..{widest[3]:.4f})")
            for degrees, node, first, last in swings:
                print(f"      {node!r}: {degrees:.2f} deg, t {first:.4f}..{last:.4f}")
    return 0


def glb_binary(path: Path) -> bytes:
    """The BIN chunk of a GLB - where the animation samplers' values live."""
    with path.open("rb") as handle:
        handle.seek(12)
        length, chunk_type = struct.unpack("<II", handle.read(8))
        assert chunk_type == 0x4E4F534A, f"{path} has no JSON chunk first"
        handle.read(length)
        length, chunk_type = struct.unpack("<II", handle.read(8))
        assert chunk_type == 0x004E4942, f"{path} has no BIN chunk second"
        return handle.read(length)


def accessor_floats(document: dict, binary: bytes, accessor: int) -> list[float]:
    """The floats of a FLOAT accessor, read from the GLB's binary chunk."""
    accessor = document["accessors"][accessor]
    assert accessor["componentType"] == 5126, "only float accessors are read here"
    width = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4}[accessor["type"]]
    view = document["bufferViews"][accessor["bufferView"]]
    start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
    stride = view.get("byteStride") or width * 4
    out = []
    for index in range(accessor["count"]):
        out.extend(struct.unpack_from(f"<{width}f", binary, start + index * stride))
    return out


def quat_angle(a: list[float], b: list[float]) -> float:
    """The rotation between two glTF quaternions (x, y, z, w), in degrees."""
    dot = min(1.0, abs(sum(x * y for x, y in zip(a, b))))
    return 2.0 * math.degrees(math.acos(dot))


def main(argv: list[str]) -> int:
    if not argv or argv[0] == "models":
        return models_command()
    if argv[0] == "pairs":
        return pairs_command()
    if argv[0] == "swings":
        return swings_command()
    if argv[0] == "clip":
        return clip_command(argv[1:])
    if argv[0] == "nodes" and len(argv) == 3:
        return nodes_command(argv[1], argv[2])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
