#!/usr/bin/env python3
"""Turn Tracy zone statistics (tracy-csvexport output) into tables an agent can read and compare.

    python tools/perf/tracy_report.py summary <zones.csv> [--frames N] [--top 25] [--match TEXT]
    python tools/perf/tracy_report.py diff <before.csv> <after.csv> [--top 25] [--min-ms 0.01]
    python tools/perf/tracy_report.py gpu <gpu-events.csv> --out gpu-zones.csv

GPU pass zones (Bevy's render diagnostics, exported with `tracy-csvexport -g`) become a zone table with
`gpu`, so `summary` and `diff` work on them too; pass `--frames` taken from the CPU zones.csv.

tracy-csvexport writes one row per zone with the columns
name,src_file,src_line,total_ns,total_perc,counts,mean_ns,min_ns,max_ns,std_ns
(`-e` exports self time with the same columns; do not use `-u`, it lists every event). Bevy names a system's span after the system, so
`render thread`, `main_render_schedule` and the system names read straight off the table.

Numbers from a Tracy run carry the profiler's own overhead: compare two captures made the same way,
never quote them as frame times (docs/process/profiling-tools.md).
"""

from __future__ import annotations

import argparse
import math
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Zone:
    name: str
    src: str
    total_ns: float
    counts: int
    mean_ns: float
    max_ns: float
    loading: bool = False  # a one-off asset-loading zone (the name may be shortened, so the flag is set from the raw name)

    @property
    def total_ms(self) -> float:
        return self.total_ns / 1e6


def short_name(raw: str, limit: int = 100) -> str:
    """`system{name="a::b"}` -> `system a::b`; long generic names keep their tail."""
    name = raw
    if "{" in raw and raw.endswith("}"):
        kind, _, rest = raw.partition("{")
        value = rest[:-1]
        if value.startswith('name="') and value.endswith('"'):
            value = value[6:-1]
        name = f"{kind} {value}"
    name = name.replace("|", "/")  # keep the markdown table intact
    return name if len(name) <= limit else "..." + name[-(limit - 3):]


def load(path: Path) -> list[Zone]:
    """Read tracy-csvexport output. Zone names hold commas (Rust generics) and are not quoted, so
    each row is split from the right: eight numeric columns, then the source file, then the name."""
    zones: list[Zone] = []
    with path.open(encoding="utf-8") as handle:
        header = handle.readline().strip().split(",")
        if header[:3] != ["name", "src_file", "src_line"]:
            raise ValueError(f"{path}: not a tracy-csvexport zone table (header {header[:3]})")
        for line in handle:
            line = line.rstrip(chr(13) + chr(10))
            if not line:
                continue
            head, src_line, total_ns, _perc, counts, mean_ns, _min, max_ns, _std = line.rsplit(",", 8)
            name, _, src_file = head.rpartition(",")
            zones.append(
                Zone(
                    name=short_name(name),
                    src=f"{Path(src_file.replace(chr(92), '/')).name}:{src_line}",
                    total_ns=float(total_ns),
                    counts=int(float(counts)),
                    mean_ns=float(mean_ns),
                    max_ns=float(max_ns),
                    loading=name.startswith("asset loading"),
                )
            )
    return zones


def gpu_table(events_csv: Path) -> str:
    """Aggregate `tracy-csvexport -g` (one row per GPU zone event: name,src_file,time,duration_ns) into
    the zone-table layout of `load`, so GPU passes list, summarise and diff like CPU zones.
    Bevy's render diagnostics emit these zones (pass names such as `main_opaque_pass_3d`)."""
    stats: dict[str, list[float]] = {}
    with events_csv.open(encoding="utf-8") as handle:
        header = handle.readline().strip().split(",")
        if header[:2] != ["name", "src_file"] or "GPU execution time" not in header:
            raise ValueError(f"{events_csv}: not a tracy-csvexport -g table (header {header})")
        for line in handle:
            line = line.rstrip(chr(13) + chr(10))
            if not line:
                continue
            head, _start, duration = line.rsplit(",", 2)
            name = head.rpartition(",")[0]
            row = stats.setdefault(name, [0.0, 0, math.inf, 0.0])
            ns = float(duration)
            row[0] += ns
            row[1] += 1
            row[2] = min(row[2], ns)
            row[3] = max(row[3], ns)
    grand = sum(row[0] for row in stats.values()) or 1.0
    out = ["name,src_file,src_line,total_ns,total_perc,counts,mean_ns,min_ns,max_ns,std_ns"]
    for name, (total, count, low, high) in sorted(stats.items(), key=lambda kv: -kv[1][0]):
        out.append(f"{name},gpu,0,{total:.0f},{100 * total / grand:.2f},{count},{total / count:.0f},{low:.0f},{high:.0f},0")
    return chr(10).join(out) + chr(10)


FRAME_ZONE = "main_render_schedule"


def frames_in(zones: list[Zone]) -> int | None:
    """Rendered frames in the capture: how often the render graph's schedule ran."""
    counts = [z.counts for z in zones if z.name == FRAME_ZONE]
    return max(counts) if counts else None


def summary(
    zones: list[Zone],
    frames: int | None,
    top: int,
    match: str | None,
    keep_loading: bool = False,
) -> str:
    """Top zones by total time. One-off asset-loading zones are left out (a loading stall is not a
    per-frame cost; `keep_loading` shows them). Frames default to the rendered-frame count."""
    frames = frames or frames_in(zones)
    rows = [
        z
        for z in zones
        if (not match or match.lower() in z.name.lower())
        and (keep_loading or not z.loading)
    ]
    rows.sort(key=lambda z: z.total_ns, reverse=True)
    header = "| zone | source | total ms | calls | mean ms | max ms |"
    sep = "|---|---|---:|---:|---:|---:|"
    if frames:
        header = header[:-1].rstrip() + " | ms per frame |"
        sep += "---:|"
    lines = [header, sep]
    for z in rows[:top]:
        cells = [z.name, z.src, f"{z.total_ms:.2f}", str(z.counts), f"{z.mean_ns / 1e6:.3f}", f"{z.max_ns / 1e6:.3f}"]
        if frames:
            cells.append(f"{z.total_ms / frames:.3f}")
        lines.append("| " + " | ".join(cells) + " |")
    return "\n".join(lines)


def diff(before: list[Zone], after: list[Zone], top: int, min_ms: float) -> str:
    """Per-call mean change, so captures of different length compare."""
    old = {(z.name, z.src): z for z in before}
    new = {(z.name, z.src): z for z in after}
    rows = []
    for key in old.keys() | new.keys():
        a, b = old.get(key), new.get(key)
        a_mean = a.mean_ns / 1e6 if a else 0.0
        b_mean = b.mean_ns / 1e6 if b else 0.0
        delta = b_mean - a_mean
        if abs(delta) >= min_ms:
            rows.append((abs(delta), key[0], key[1], a_mean, b_mean, delta, a.counts if a else 0, b.counts if b else 0))
    rows.sort(reverse=True)
    lines = [
        "| zone | source | before mean ms | after mean ms | change ms | calls before | calls after |",
        "|---|---|---:|---:|---:|---:|---:|",
    ]
    for _, name, src, a_mean, b_mean, delta, a_n, b_n in rows[:top]:
        lines.append(f"| {name} | {src} | {a_mean:.3f} | {b_mean:.3f} | {delta:+.3f} | {a_n} | {b_n} |")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("summary")
    s.add_argument("csv", type=Path)
    s.add_argument("--frames", type=int, help="frames in the capture, adds a per-frame column")
    s.add_argument("--top", type=int, default=25)
    s.add_argument("--match", help="only zones whose name contains this text")
    s.add_argument("--keep-loading", action="store_true", help="also list one-off asset-loading zones")
    g = sub.add_parser("gpu", help="aggregate a tracy-csvexport -g file into a zone table (prints it, or --out)")
    g.add_argument("events", type=Path)
    g.add_argument("--out", type=Path)
    d = sub.add_parser("diff")
    d.add_argument("before", type=Path)
    d.add_argument("after", type=Path)
    d.add_argument("--top", type=int, default=25)
    d.add_argument("--min-ms", type=float, default=0.01)
    ns = parser.parse_args()
    if ns.cmd == "gpu":
        text = gpu_table(ns.events)
        if ns.out:
            ns.out.write_text(text, encoding="utf-8")
        else:
            print(text, end="")
    elif ns.cmd == "summary":
        print(summary(load(ns.csv), ns.frames, ns.top, ns.match, ns.keep_loading))
    else:
        print(diff(load(ns.before), load(ns.after), ns.top, ns.min_ms))
    return 0


if __name__ == "__main__":
    sys.exit(main())
