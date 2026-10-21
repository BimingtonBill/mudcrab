"""Summarize conversion-manifest.json + integration-report.json for the Phase 1 gap triage.

Prints JSON: schema, complete, inputs_by_kind, failure count and reason histogram,
entry count by output extension, and the integration report's coverage counters.
Read-only; never touches the converted tree.
"""
from __future__ import annotations

import json
import sys
from collections import Counter
from pathlib import Path


def main() -> int:
    root = Path(sys.argv[1])
    manifest = json.loads((root / "conversion-manifest.json").read_text(encoding="utf-8"))
    report_path = root / "integration-report.json"
    report = json.loads(report_path.read_text(encoding="utf-8")) if report_path.is_file() else None

    entries = manifest.get("entries", {}) or {}
    failures = manifest.get("failures", {}) or {}
    archives = manifest.get("archives", {}) or {}

    ext_counts: Counter[str] = Counter()
    for entry in entries.values():
        out = (entry.get("output") or "").lower()
        ext = Path(out).suffix or "<none>"
        ext_counts[ext] += 1

    reason_counts: Counter[str] = Counter()
    for key, message in failures.items():
        # Bucket by the message's leading clause; converter messages start with a kind.
        head = message.split(":")[0].strip().lower()[:80]
        reason_counts[head] += 1

    out = {
        "manifest_schema_version": manifest.get("schema_version"),
        "manifest_complete": manifest.get("complete"),
        "inputs_by_kind": manifest.get("inputs_by_kind"),
        "entry_count": len(entries),
        "entry_output_extensions": dict(sorted(ext_counts.items())),
        "failure_count": len(failures),
        "failure_reason_buckets": dict(sorted(reason_counts.items(), key=lambda kv: -kv[1])),
        "failure_samples": dict(list(sorted(failures.items()))[:30]),
        "archive_count": len(archives),
        "integration_report": report,
    }
    print(json.dumps(out, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
