"""Summarise one conversion run: the report JSON and the published manifest.

Read-only. Counts and byte totals only; no game data is copied anywhere.

Usage: python tools/research/phase1_speed/summarize_s15.py
"""

from __future__ import annotations

import json
import os
import sys
from collections import Counter

REPORT = sys.argv[1] if len(sys.argv) > 1 else r"D:/Modding-overflow/s15-report.json"
TREE = (
    sys.argv[2] if len(sys.argv) > 2 else r"D:/Modding-overflow/SkyrimConverted-upstream-s15"
)


def kind(path: str) -> str:
    return os.path.splitext(path.lower())[1] or "<none>"


def human(n: float) -> str:
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if n < 1024 or unit == "TB":
            return f"{n:.1f} {unit}"
        n /= 1024
    return f"{n:.1f} TB"


def main() -> None:
    with open(REPORT, encoding="utf-8") as handle:
        report = json.load(handle)

    print(f"# report {REPORT}")
    for key in ("complete", "converted", "cache_hits", "skipped", "elapsed_ms"):
        print(f"{key}: {report.get(key)}")
    if report.get("elapsed_ms"):
        print(f"elapsed: {report['elapsed_ms'] / 1000 / 60:.1f} min")
    print(f"warnings: {len(report.get('warnings', []))}")
    print(f"inputs_by_kind: {report.get('inputs_by_kind')}")
    artifacts = report.get("artifacts", [])
    print(f"artifacts: {len(artifacts)}")
    for ext, count in Counter(kind(a) for a in artifacts).most_common():
        print(f"  artifact {ext:<10} {count}")

    manifest_path = os.path.join(TREE, "conversion-manifest.json")
    print(f"\n# manifest {manifest_path}")
    size = os.path.getsize(manifest_path)
    print(f"manifest bytes: {human(size)}")
    with open(manifest_path, encoding="utf-8") as handle:
        manifest = json.load(handle)
    entries = manifest.get("entries", {})
    print(f"schema_version: {manifest.get('schema_version')} complete: {manifest.get('complete')}")
    print(f"entries: {len(entries)}  failures: {len(manifest.get('failures', {}))}")
    bytes_by_ext: Counter[str] = Counter()
    count_by_ext: Counter[str] = Counter()
    for entry in entries.values():
        bytes_by_ext[kind(entry["output"])] += entry.get("output_size", 0)
        count_by_ext[kind(entry["output"])] += 1
    for ext, count in count_by_ext.most_common():
        print(f"  entry {ext:<10} {count:>7}  {human(bytes_by_ext[ext])}")
    archives = manifest.get("archives", {})
    ingested = sum(len(entry.get("files", [])) for entry in archives.values())
    ingested_bytes = sum(
        f.get("size", 0) for entry in archives.values() for f in entry.get("files", [])
    )
    print(f"archives: {len(archives)}  ingested files: {ingested}  {human(ingested_bytes)}")


if __name__ == "__main__":
    main()
