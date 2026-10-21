#!/usr/bin/env python
"""Compares two cluster overlays for reproducibility.

    python -I scripts/cluster_repro_check.py A B [--overlay SELECTOR]

A and B are each an overlay folder (one holding `overlay.json`) or a converted base. For a base the
overlay is chosen as `tools/extension_hash.py --overlay` does: `--overlay SELECTOR` (label, settings
id, folder name or path), or the only overlay under `<base>/derived` when it is omitted.

Two derivations of the same base with the same settings, for example with `--jobs 1` and `--jobs 4`
(run in two copies of the base, since one base holds one overlay per settings id), must leave
identical owned content: `cluster-manifest.json`, `cluster-terrain-manifest.json` and every file under `clusters/`, the logical
content of every table in `clusters.db` (rows compared as sets, so row order and page layout of the
database file do not count), its indexes, and the overlay's recorded identity: settings id and
resolved settings, builder identity, base core identity, the recorded base failure count and the extension entries (hashes, versions,
ownership, stats). Not compared, because they legitimately differ between runs: label, creation time,
owner text, the base fingerprint and the host path of the terrain sources.

Exits 0 when everything matches, 1 with the differences listed, 2 on a usage error or an overlay that
cannot be found or read.
"""

import hashlib
import json
import sqlite3
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
import extension_hash as overlays  # noqa: E402

# The derived manifests an overlay can own beside `clusters/`: objects, and terrain when built.
MANIFESTS = ("cluster-manifest.json", "cluster-terrain-manifest.json")

# overlay.json keys that are compared; see the module description for what is left out.
IDENTITY_KEYS = (
    "settings_id", "build_settings", "builder_identity", "base_core_identity", "base_failure_count", "extensions"
)


def file_hashes(overlay):
    hashes = {}
    for name in MANIFESTS:
        manifest = overlay / name
        if manifest.is_file():
            hashes[name] = hashlib.sha256(manifest.read_bytes()).hexdigest()
    for path in sorted((overlay / "clusters").rglob("*")):
        if path.is_file():
            relative = path.relative_to(overlay).as_posix()
            hashes[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes


def table_hashes(overlay):
    """Row count and digest of every table of `clusters.db` (R-tree shadow tables included), plus its
    indexes. The overlay database holds only cluster objects, so no table list is needed."""
    database = sqlite3.connect(f"{(overlay / overlays.OVERLAY_DATABASE).resolve().as_uri()}?mode=ro", uri=True)
    try:
        names = sorted(
            row[0]
            for row in database.execute(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'"
            )
        )
        hashes = {}
        for table in names:
            quoted = '"' + table.replace('"', '""') + '"'
            rows = sorted(json.dumps(list(row), default=repr) for row in database.execute(f"SELECT * FROM {quoted}"))
            digest = hashlib.sha256()
            for row in rows:
                digest.update(row.encode())
                digest.update(b"\n")
            hashes[table] = f"{len(rows)} rows {digest.hexdigest()}"
        indexes = sorted(
            row[0]
            for row in database.execute(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'"
            )
        )
    finally:
        database.close()
    hashes["(indexes)"] = ",".join(indexes)
    return hashes


def identity(overlay):
    metadata = overlays.read_overlay_metadata(overlay)
    metadata.setdefault("base_failure_count", 0)  # absent in overlays written before the field
    return {key: json.dumps(metadata[key], sort_keys=True) for key in IDENTITY_KEYS}


def locate(path, selector):
    """The overlay folder `path` names: itself when it holds `overlay.json`, else the one chosen in a base."""
    path = Path(path)
    if (path / overlays.OVERLAY_METADATA).is_file():
        return path
    return overlays.resolve_overlay(path, selector)


def compare(label, left, right):
    problems = []
    for key in sorted(set(left) | set(right)):
        if left.get(key) != right.get(key):
            problems.append(f"{label} {key}: {left.get(key)} != {right.get(key)}")
    return problems


def compare_overlays(first, second):
    """Differences between two overlay folders; empty means reproducible. Returns (problems, files, tables)."""
    files = file_hashes(first)
    tables = table_hashes(first)
    problems = compare("file", files, file_hashes(second))
    problems += compare("table", tables, table_hashes(second))
    problems += compare("overlay.json", identity(first), identity(second))
    return problems, len(files), len(tables) - 1


def main(argv):
    selector = ""
    paths = []
    arguments = iter(argv)
    for argument in arguments:
        if argument == "--overlay":
            selector = next(arguments, None)
        elif argument.startswith("--overlay="):
            selector = argument.split("=", 1)[1]
        else:
            paths.append(argument)
    if selector is None or len(paths) != 2:
        print(__doc__)
        return 2
    try:
        first, second = (locate(path, selector) for path in paths)
        problems, files, tables = compare_overlays(first, second)
    except (overlays.OverlayError, sqlite3.DatabaseError, OSError) as error:
        print(f"cannot compare: {error}")
        return 2
    if problems:
        print(f"DIFFERENT: {len(problems)} differences")
        for problem in problems[:50]:
            print("  " + problem)
        return 1
    print(f"identical: {files} files and {tables} tables")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
