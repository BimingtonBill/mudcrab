"""Content hash of a converted data set's extensions (`mudcrab-ext-hash-v1`).

A data set's `conversion-manifest.json` lists the extensions the converter wrote beyond `main`'s
core schema:

    "extensions": [{"id": ..., "version": ..., "owns": {"tables": [...], "indexes": [...],
                    "files": [...], "gltf_extensions": [...]}, "sha256": ..., "built_from": ...,
                    "stats": {...}}]

This module is the reference implementation of the `sha256` value; the converter's Rust writer
must agree with it. Design: local/research/op6-one-data-set-design-2026-10-16.md, section 3.2.1.

    python tools/extension_hash.py <data set root>     check every listed extension, exit 1 on a mismatch
    python tools/extension_hash.py --assets <base> --overlay [<label|settings-id|folder|path>]
                                                       check one cluster overlay of a base (DS1)

Overlay mode (`<base>/derived/clusters-<settings-id>/`, see crates/shared/src/cluster_overlay.rs):
`overlay.json` carries the cluster `extensions`, `clusters.db` holds the cluster tables, and the
entries' `owns` paths are relative to the overlay folder. The base is never modified and its own
manifest is not checked here (run base mode for that). The overlay binds to its base by the base's core
build identity (`lod_build.build_identity`) and the base fingerprint, `sha256` of
`mudcrab-overlay-base-fingerprint-v1\n<core identity>\n<sha256 hex of conversion-manifest.json>\n`.
An empty selector takes the only overlay and fails when the base has none, several, or any unreadable one;
a named selector ignores unrelated unreadable overlay folders. Exit 0 clean, 1 problems, 2 usage.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sqlite3
import struct
import sys
from pathlib import Path

ALGORITHM = "mudcrab-ext-hash-v1"
DATABASE = "skyrim_world.db"
MANIFEST = "conversion-manifest.json"
GLB_MAGIC = 0x46546C67
GLB_JSON_CHUNK = 0x4E4F534A

# Cluster overlay constants; they mirror crates/shared/src/cluster_overlay.rs.
OVERLAY_KIND = "mudcrab.cluster_overlay"
OVERLAY_SCHEMA_VERSION = 1
DERIVED_DIR = "derived"
OVERLAY_DIR_PREFIX = "clusters-"
OVERLAY_METADATA = "overlay.json"
OVERLAY_DATABASE = "clusters.db"
FINGERPRINT_ALGORITHM = "mudcrab-overlay-base-fingerprint-v1"
CLUSTER_EXTENSION_IDS = ("mudcrab.clusters", "mudcrab.cluster_terrain")


def _hex_text(text: str) -> str:
    return text.encode("utf-8").hex()


def _real(value: float) -> str:
    return "f" + struct.pack(">d", value).hex()


def encode_sql(value) -> str:
    """One SQLite value as `n`, `i<dec>`, `f<bits>`, `t<utf8 hex>` or `b<hex>`."""
    if value is None:
        return "n"
    if isinstance(value, bool):  # sqlite3 never returns bool, but keep ints honest
        return f"i{int(value)}"
    if isinstance(value, int):
        return f"i{value}"
    if isinstance(value, float):
        return _real(value)
    if isinstance(value, str):
        return "t" + _hex_text(value)
    if isinstance(value, (bytes, bytearray, memoryview)):
        return "b" + bytes(value).hex()
    raise TypeError(f"unsupported SQLite value {value!r}")


def encode_json(value) -> str:
    """Canonical JSON: sorted keys, numbers as binary64 bits, strings as UTF-8 hex."""
    if value is None:
        return "null"
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, (int, float)):
        return _real(float(value))
    if isinstance(value, str):
        return "t" + _hex_text(value)
    if isinstance(value, list):
        return "[" + ",".join(encode_json(item) for item in value) + "]"
    if isinstance(value, dict):
        keys = sorted(value, key=lambda key: key.encode("utf-8"))
        return "{" + ",".join(f"t{_hex_text(key)}:{encode_json(value[key])}" for key in keys) + "}"
    raise TypeError(f"unsupported JSON value {value!r}")


def _open_read_only(path: Path) -> sqlite3.Connection:
    return sqlite3.connect(f"{path.resolve().as_uri()}?mode=ro", uri=True)


def _quote(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def table_lines(connection: sqlite3.Connection, table: str) -> list[str]:
    columns = connection.execute(f"PRAGMA table_info({_quote(table)})").fetchall()
    if not columns:
        raise LookupError(f"table {table} is missing")
    rows = sorted(
        ",".join(encode_sql(value) for value in row)
        for row in connection.execute(f"SELECT * FROM {_quote(table)}")
    )
    declared = ",".join(f"{column[1]}:{column[2]}" for column in columns)
    return [f"table {table}", *rows, f"columns {declared}"]


def index_line(connection: sqlite3.Connection, index: str) -> str:
    row = connection.execute(
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?", (index,)
    ).fetchone()
    if row is None:
        raise LookupError(f"index {index} is missing")
    return f"index {index} {row[0]}"


def owned_files(root: Path, entries: list[str]) -> list[str]:
    """Expand directory entries (ending in `/`) to every file below them, sorted bytewise.

    A directory that does not exist covers no files; an exact file entry that is missing is an error.
    """
    paths: set[str] = set()
    for entry in entries:
        if entry.endswith("/"):
            folder = root / entry
            if not folder.is_dir():
                continue  # a directory entry covers whatever is below it, which can be nothing
            paths.update(p.relative_to(root).as_posix() for p in folder.rglob("*") if p.is_file())
        else:
            if not (root / entry).is_file():
                raise LookupError(f"file {entry} is missing")
            paths.add(entry)
    return sorted(paths, key=lambda path: path.encode("utf-8"))


def glb_json(path: Path) -> dict:
    data = path.read_bytes()
    if path.suffix.lower() == ".gltf":
        return json.loads(data)
    magic, _version, _length = struct.unpack_from("<III", data, 0)
    if magic != GLB_MAGIC:
        raise ValueError(f"{path} is not a GLB")
    chunk_length, chunk_type = struct.unpack_from("<II", data, 12)
    if chunk_type != GLB_JSON_CHUNK:
        raise ValueError(f"{path} has no JSON chunk first")
    return json.loads(data[20 : 20 + chunk_length])


def gltf_extension_value(document: dict, name: str) -> str | None:
    """`<pointer>=<json>` pairs joined by `;`, document level first, then materials in order."""
    parts = []
    top = document.get("extensions", {})
    if name in top:
        parts.append(f"/extensions/{name}={encode_json(top[name])}")
    for index, material in enumerate(document.get("materials", [])):
        extension = material.get("extensions", {})
        if name in extension:
            parts.append(f"/materials/{index}/extensions/{name}={encode_json(extension[name])}")
    return ";".join(parts) if parts else None


def mesh_files(root: Path) -> list[Path]:
    meshes = [p for p in root.rglob("*") if p.suffix.lower() in (".glb", ".gltf") and p.is_file()]
    return sorted(meshes, key=lambda p: p.relative_to(root).as_posix().encode("utf-8"))


def extension_lines(
    root: Path, entry: dict, meshes: list[Path] | None = None, database: Path | None = None
) -> list[str]:
    """The hashed stream of one entry. `database` is the file holding its tables and indexes
    (default `<root>/skyrim_world.db`; an overlay passes its `clusters.db`)."""
    owns = entry.get("owns", {})
    lines = [ALGORITHM, str(entry["id"]), str(int(entry["version"]))]
    tables = sorted(owns.get("tables", []))
    indexes = sorted(owns.get("indexes", []))
    if tables or indexes:
        connection = _open_read_only(database if database is not None else root / DATABASE)
        try:
            for table in tables:
                lines.extend(table_lines(connection, table))
            for index in indexes:
                lines.append(index_line(connection, index))
        finally:
            connection.close()
    for path in owned_files(root, owns.get("files", [])):
        lines.append(f"file {path} {hashlib.sha256((root / path).read_bytes()).hexdigest()}")
    gltf_names = sorted(owns.get("gltf_extensions", []))
    if gltf_names:
        if meshes is None:
            meshes = mesh_files(root)
        documents = [(m.relative_to(root).as_posix(), glb_json(m)) for m in meshes]
        for name in gltf_names:
            for path, document in documents:
                if name not in document.get("extensionsUsed", []):
                    continue
                value = gltf_extension_value(document, name)
                if value is not None:
                    digest = hashlib.sha256(value.encode("utf-8")).hexdigest()
                    lines.append(f"gltf {name} {path} {digest}")
    return lines


def extension_hash(
    root: Path, entry: dict, meshes: list[Path] | None = None, database: Path | None = None
) -> str:
    digest = hashlib.sha256()
    for line in extension_lines(root, entry, meshes, database):
        digest.update(line.encode("utf-8"))
        digest.update(b"\n")
    return digest.hexdigest()


def load_extensions(root: Path) -> list[dict]:
    manifest = json.loads((root / MANIFEST).read_text(encoding="utf-8"))
    return list(manifest.get("extensions", []))


def ownership_problems(extensions: list[dict]) -> list[str]:
    """Two extensions may not own the same table, index, file entry or glTF extension name."""
    seen: dict[tuple[str, str], str] = {}
    problems = []
    ids = set()
    for entry in extensions:
        if entry["id"] in ids:
            problems.append(f"extension {entry['id']} is listed twice")
        ids.add(entry["id"])
        for kind, names in entry.get("owns", {}).items():
            for name in names:
                owner = seen.setdefault((kind, name), entry["id"])
                if owner != entry["id"]:
                    problems.append(f"{kind[:-1]} {name} is owned by {owner} and {entry['id']}")
    return problems


def check(root: Path) -> list[str]:
    """Problems with the data set's extension list; empty means every hash matches."""
    extensions = load_extensions(root)
    problems = ownership_problems(extensions)
    meshes = None
    if any(entry.get("owns", {}).get("gltf_extensions") for entry in extensions):
        meshes = mesh_files(root)
    for entry in extensions:
        try:
            actual = extension_hash(root, entry, meshes)
        except (LookupError, ValueError) as error:
            problems.append(f"{entry['id']}: {error}")
            continue
        if actual != entry.get("sha256"):
            problems.append(f"{entry['id']}: sha256 {entry.get('sha256')} in the manifest, {actual} on disk")
    return problems


class OverlayError(ValueError):
    """A cluster overlay cannot be found, read or bound to its base."""


def _regular_file(path: Path) -> None:
    if path.is_symlink() or not path.is_file():
        raise OverlayError(f"{path} is not a regular file")


def quote_argument(value) -> str:
    """One PowerShell single-quoted argument, as the Rust `quote_argument`: apostrophes and the
    typographic single quotes U+2018 to U+201B (which PowerShell reads as quotes) are doubled."""
    return "'" + "".join(
        character * (2 if character == "'" or 0x2018 <= ord(character) <= 0x201B else 1) for character in str(value)
    ) + "'"


def derive_command(base: Path, metadata: dict) -> str:
    """The command that rebuilds an overlay with its recorded settings, as the Rust
    `OverlayMetadata::derive_command`. It names no `--jobs`: scheduling is not a setting."""
    settings = metadata["build_settings"]
    command = (
        f"converter derive-clusters {quote_argument(command_path(base))} --triangle-threshold {settings['triangle_threshold']} "
        f"--label {quote_argument(metadata['label'])} --replace"
    )
    if settings["terrain"]:
        command += " --terrain"
        if metadata.get("terrain_sources"):
            command += f" --terrain-sources {quote_argument(metadata['terrain_sources'])}"
    if settings["include_trees"]:
        command += " --include-trees"
    return command


def command_path(base: Path) -> str:
    """A command-friendly Windows path, matching the Rust command formatter."""
    text = str(base)
    if text.startswith("\\\\?\\UNC\\"):
        return "\\\\" + text[8:]
    return text[4:] if text.startswith("\\\\?\\") else text


def rebuild_command(base: Path, metadata: dict | None = None) -> str:
    """The sentence a refusal ends with, as the Rust `rebuild_command` (no metadata) or
    `OverlayMetadata::rebuild_hint` (a terrain build adds what to do about its raw sources)."""
    if metadata is None:
        return f"rebuild it with converter derive-clusters {quote_argument(command_path(base))}"
    hint = f"rebuild it with {derive_command(base, metadata)}"
    if metadata["build_settings"]["terrain"] and metadata.get("terrain_sources"):
        hint += (
            f"; the terrain build read raw sources from {metadata['terrain_sources']}, which the overlay does not "
            "keep: restore or provide the matching raw DDS sources there, or pass their folder with --terrain-sources"
        )
    return hint


def base_core_identity(base: Path) -> str:
    """The base's nonempty `lod_build.build_identity`."""
    database = base / DATABASE
    _regular_file(database)
    connection = _open_read_only(database)
    try:
        has_table = connection.execute(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'lod_build'"
        ).fetchone()
        row = connection.execute("SELECT build_identity FROM lod_build LIMIT 1").fetchone() if has_table else None
    finally:
        connection.close()
    if row is None or not isinstance(row[0], str) or not row[0].strip():
        raise OverlayError(f"{base} has no core build identity (`lod_build`); it is not a converted dataset")
    return row[0]


def base_fingerprint(base: Path) -> str:
    """Core identity plus the hash of `conversion-manifest.json`, as the Rust `base_fingerprint`."""
    identity = base_core_identity(base)
    manifest = base / MANIFEST
    _regular_file(manifest)
    digest = hashlib.sha256()
    digest.update(FINGERPRINT_ALGORITHM.encode("utf-8") + b"\n")
    digest.update(identity.encode("utf-8") + b"\n")
    digest.update(hashlib.sha256(manifest.read_bytes()).hexdigest().encode("ascii") + b"\n")
    return digest.hexdigest()


def full_copy_problems(base: Path) -> list[str]:
    """The base carries cluster data itself (an old full-copy cluster dataset); no reader exists."""
    claims = []
    if (base / "cluster-manifest.json").exists():
        claims.append("cluster-manifest.json at its root")
    database = base / DATABASE
    if database.is_file():
        connection = _open_read_only(database)
        try:
            tables = connection.execute(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'cluster\\_%' ESCAPE '\\'"
            ).fetchall()
        finally:
            connection.close()
        if tables:
            claims.append("cluster tables in " + DATABASE)
    manifest = base / MANIFEST
    if manifest.is_file():
        listed = json.loads(manifest.read_text(encoding="utf-8")).get("extensions") or []
        if any(isinstance(item, dict) and item.get("id") in CLUSTER_EXTENSION_IDS for item in listed):
            claims.append("cluster extensions in " + MANIFEST)
    if not claims:
        return []
    return [
        f"{base} is an old full-copy cluster dataset ({', '.join(claims)}); there is no reader for it. "
        f"Convert the base again, then {rebuild_command(base)}"
    ]


def read_overlay_metadata(overlay: Path) -> dict:
    """`overlay.json` after the shape checks of the Rust `read_metadata`."""
    path = overlay / OVERLAY_METADATA
    _regular_file(path)
    try:
        metadata = json.loads(path.read_text(encoding="utf-8"))
    except ValueError as error:
        raise OverlayError(f"{path} is not a cluster overlay description: {error}") from error
    if not isinstance(metadata, dict):
        raise OverlayError(f"{path} is not a cluster overlay description")
    if metadata.get("kind") != OVERLAY_KIND:
        raise OverlayError(f"{path} has kind {metadata.get('kind')!r}, not {OVERLAY_KIND}")
    if metadata.get("schema_version") != OVERLAY_SCHEMA_VERSION:
        raise OverlayError(
            f"{path} has overlay schema version {metadata.get('schema_version')!r}, "
            f"but this tool reads version {OVERLAY_SCHEMA_VERSION}"
        )
    for name in ("label", "settings_id", "base_core_identity", "base_fingerprint", "builder_identity"):
        value = metadata.get(name)
        if not isinstance(value, str) or not value.strip():
            raise OverlayError(f"{path} has an empty or missing {name}")
    for name in ("created", "owner"):
        if not isinstance(metadata.get(name), str):
            raise OverlayError(f"{path} has no {name}")
    failures = metadata.get("base_failure_count", 0)  # optional; absent means none recorded
    if type(failures) is not int or failures < 0:
        raise OverlayError(f"{path} has an invalid base_failure_count")
    settings = metadata.get("build_settings")
    if not isinstance(settings, dict):
        raise OverlayError(f"{path} has no resolved build settings")
    expected = {"contract", "converter_version", "builder_identity", "triangle_threshold", "terrain", "include_trees"}
    if set(settings) != expected or settings["contract"] != "mudcrab.cluster-overlay.settings.v1":
        raise OverlayError(f"{path} has unsupported resolved build settings")
    if (type(settings["triangle_threshold"]) is not int or not 0 < settings["triangle_threshold"] <= 0xffffffff
            or type(settings["terrain"]) is not bool or type(settings["include_trees"]) is not bool
            or not isinstance(settings["converter_version"], str) or not settings["converter_version"].strip()):
        raise OverlayError(f"{path} has invalid resolved build settings")
    encoded = json.dumps(settings, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    if hashlib.sha256(encoded).hexdigest()[:16] != metadata["settings_id"]:
        raise OverlayError(f"{path} settings id differs from its resolved build settings")
    if settings.get("builder_identity") != metadata["builder_identity"]:
        raise OverlayError(f"{path} builder identity differs from its resolved build settings")
    builder = settings["builder_identity"]
    if len(builder) != 64 or any(character not in "0123456789abcdef" for character in builder):
        raise OverlayError(f"{path} has an invalid builder identity")
    sources = metadata.get("terrain_sources")
    if sources is not None and (not settings["terrain"] or not isinstance(sources, str) or not sources):
        raise OverlayError(f"{path} has invalid terrain sources")
    extensions = metadata.get("extensions")
    if not isinstance(extensions, list) or not all(
        isinstance(entry, dict) and "id" in entry and "version" in entry for entry in extensions
    ):
        raise OverlayError(f"{path} has no valid extensions list")
    problems = ownership_problems(extensions)
    if problems:
        raise OverlayError(f"{path} has conflicting extension ownership: {'; '.join(problems)}")
    return metadata


def scan_overlays(base: Path) -> tuple[list[tuple[Path, dict]], list[tuple[Path, OverlayError]]]:
    """Every `clusters-*` folder under `<base>/derived`, sorted by name: the readable ones with their
    description, and the unreadable ones with the reason."""
    derived = base / DERIVED_DIR
    if not derived.is_dir():
        return [], []
    folders = sorted(p for p in derived.iterdir() if p.name.startswith(OVERLAY_DIR_PREFIX) and p.is_dir())
    found, unreadable = [], []
    for folder in folders:
        try:
            found.append((folder, read_overlay_metadata(folder)))
        except OverlayError as error:
            unreadable.append((folder, OverlayError(f"cluster overlay {folder} is unreadable: {error}")))
    return found, unreadable


def list_overlays(base: Path) -> list[tuple[Path, dict]]:
    """Every overlay under `base`. An unreadable one is an error: it must not make another look like
    the only one."""
    found, unreadable = scan_overlays(base)
    if unreadable:
        raise unreadable[0][1]
    return found


def _describe(found: list[tuple[Path, dict]]) -> str:
    return ", ".join(f"{folder} (label {meta['label']}, settings id {meta['settings_id']})" for folder, meta in found)


def resolve_overlay(base: Path, selector: str) -> Path:
    """The overlay folder `selector` names, as the Rust `resolve_overlay` (empty = the only overlay;
    else a label, settings id or folder name under `derived/`; else a folder holding overlay.json
    relative to the working directory or to the base). It only finds the folder; binding is checked after.

    Only the bare selection needs every overlay readable. A named one ignores unrelated unreadable
    folders, but a folder name that selects an unreadable overlay reports why it is unreadable."""
    problems = full_copy_problems(base)
    if problems:
        raise OverlayError(problems[0])
    overlays, unreadable = scan_overlays(base)
    selector = selector.strip()
    derived = base / DERIVED_DIR
    if not selector:
        if unreadable:
            raise unreadable[0][1]
        if len(overlays) == 1:
            return overlays[0][0]
        if not overlays:
            raise OverlayError(f"no cluster overlay under {derived}; build one with converter derive-clusters {base}")
        raise OverlayError(
            f"{len(overlays)} cluster overlays under {derived}; choose one with "
            f"--overlay <label|settings-id|path>: {_describe(overlays)}"
        )
    matching = [
        (folder, meta) for folder, meta in overlays if selector in (meta["label"], meta["settings_id"], folder.name)
    ]
    if len(matching) == 1:
        return matching[0][0]
    if matching:
        raise OverlayError(
            f"--overlay {selector} matches {len(matching)} overlays under {derived}: {_describe(matching)}; "
            "use the settings id or the folder path"
        )
    for folder, error in unreadable:
        if selector in (folder.name, folder.name.removeprefix(OVERLAY_DIR_PREFIX)):
            raise error  # its folder name or settings id; a label of an unreadable one cannot be known
    for candidate in (Path(selector), base / selector):
        if (candidate / OVERLAY_METADATA).is_file():
            return candidate
    raise OverlayError(
        f"no cluster overlay {selector!r} for {base}; found: {_describe(overlays) if overlays else 'none'}"
    )


def _schema_objects(path: Path) -> set[tuple[str, str]]:
    connection = _open_read_only(path)
    try:
        return {
            (kind, name.lower())
            for kind, name in connection.execute(
                "SELECT type, name FROM sqlite_master WHERE type IN ('table', 'index')"
            )
        }
    finally:
        connection.close()


def check_overlay(base: Path, overlay: Path) -> list[str]:
    """Problems with one overlay of `base`; empty means it is bound to the base and every hash matches."""
    problems = full_copy_problems(base)
    if problems:
        return problems
    try:
        metadata = read_overlay_metadata(overlay)
        _regular_file(overlay / OVERLAY_DATABASE)
        identity = base_core_identity(base)
        fingerprint = base_fingerprint(base)
    except OverlayError as error:
        return [str(error)]
    foreign = f"this cluster overlay was built for another conversion of this base; {rebuild_command(base, metadata)}"
    if metadata["base_core_identity"] != identity:
        problems.append(f"{foreign} (recorded core identity {metadata['base_core_identity']}, base {identity})")
    if metadata["base_fingerprint"] != fingerprint:
        problems.append(f"{foreign} (recorded fingerprint {metadata['base_fingerprint']}, base {fingerprint})")
    extensions = metadata["extensions"]
    for entry in extensions:
        if entry.get("built_from") != identity:
            problems.append(
                f"{entry['id']}: built_from {entry.get('built_from')} is not the base core identity {identity}"
            )
    if problems:
        return problems  # A foreign overlay grants no authority to read its cluster data.
    ids = [entry["id"] for entry in extensions]
    problems.extend(f"extension {name} is listed twice" for name in sorted({i for i in ids if ids.count(i) > 1}))
    # A certified overlay table or index must never be answered by one of the same name in the base.
    base_objects = _schema_objects(base / DATABASE)
    database = overlay / OVERLAY_DATABASE
    meshes = None
    if any(entry.get("owns", {}).get("gltf_extensions") for entry in extensions):
        meshes = mesh_files(overlay)
    for entry in extensions:
        owns = entry.get("owns", {})
        for kind, names in (("table", owns.get("tables", [])), ("index", owns.get("indexes", []))):
            for name in names:
                if (kind, name.lower()) in base_objects:
                    problems.append(f"{entry['id']}: {kind} {name} exists in both the overlay and the base databases")
        try:
            actual = extension_hash(overlay, entry, meshes, database)
        except (LookupError, ValueError, sqlite3.DatabaseError) as error:
            problems.append(f"{entry['id']}: {error}")
            continue
        if actual != entry.get("sha256"):
            problems.append(f"{entry['id']}: sha256 {entry.get('sha256')} in overlay.json, {actual} on disk")
    return problems


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="extension_hash.py",
        description="Check the extension hashes of a converted data set, or of one cluster overlay of it.",
    )
    parser.add_argument("root", nargs="?", help="the data set root (the base in overlay mode)")
    parser.add_argument("--assets", help="the base data set root; same as the positional argument")
    parser.add_argument(
        "--overlay",
        nargs="?",
        const="",
        metavar="SELECTOR",
        help="check a cluster overlay of the base instead of the base: a label, settings id, folder name "
        "under derived/ or a folder path; no value takes the only overlay",
    )
    return parser


def main(argv: list[str]) -> int:
    parser = _parser()
    try:
        args = parser.parse_args(argv)
    except SystemExit as stop:
        return 2 if stop.code else 0
    if args.root and args.assets:
        print("give the data set root once, positionally or with --assets", file=sys.stderr)
        return 2
    given = args.assets or args.root
    if not given:
        parser.print_usage(sys.stderr)
        return 2
    root = Path(given)
    if args.overlay is not None:
        return main_overlay(root, args.overlay)
    extensions = load_extensions(root)
    problems = check(root)
    for entry in extensions:
        print(f"{entry['id']} v{entry['version']} {entry.get('sha256')}")
    for problem in problems:
        print(f"PROBLEM {problem}")
    print(f"{len(extensions)} extension(s), {len(problems)} problem(s)")
    return 1 if problems else 0


def main_overlay(base: Path, selector: str) -> int:
    try:
        overlay = resolve_overlay(base, selector)
    except OverlayError as error:
        print(f"PROBLEM {error}")
        print("0 extension(s), 1 problem(s)")
        return 1
    problems = check_overlay(base, overlay)
    try:
        extensions = read_overlay_metadata(overlay)["extensions"]
    except OverlayError:
        extensions = []  # already reported by check_overlay
    print(f"overlay {overlay}")
    for entry in extensions:
        print(f"{entry['id']} v{entry['version']} {entry.get('sha256')}")
    for problem in problems:
        print(f"PROBLEM {problem}")
    print(f"{len(extensions)} extension(s), {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
