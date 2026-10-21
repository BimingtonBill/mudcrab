#!/usr/bin/env python3
"""Runs the cluster stage benchmark of ADR-0013 ("Prototype proof") and writes the report.

Each run is one `cluster-stage` process on its own work folder, started in sequence so the
machine is quiet for each. A run holds no copy of the world database (it reads the converted
output's), so a work folder is its payloads and a small tables database; folders are deleted once
the next run of the chain no longer needs them (use --keep-work to keep them). Modes compared, all with the stand-in baked object chunks on so that
`cluster` mode builds the baked remainder too:

  baked   : --lod-mode baked  --bench-baked    (everything goes through the baked stand-in)
  cluster : --lod-mode cluster --bench-baked   (eligible primitives get clusters, the rest baked)
  cluster-only : --lod-mode cluster            (cluster stage alone, for its own cost)

Each mode runs cold (no earlier output), warm (the cold run's output as the previous output) and
after one plugin changed (the warm run's input with one plugin's references moved).

Usage:
  python scripts/cluster_bench.py --exe PATH/cluster-stage.exe --converted DIR --work-root DIR
         --out report.md [--plugin Update.esm] [--threshold 1024] [--jobs N]
         [--terrain-lod-seconds S] [--package-seconds S]

`--terrain-lod-seconds` and `--package-seconds` come from a full conversion of the same data
(the report's `lod_elapsed_ms` and `elapsed_ms`); they are not measured here and the report says
so when they are absent. Run it inside a build-gate timing hold.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path


def run(exe, converted, work, previous, mode, bench_baked, threshold, jobs, plugin):
    command = [
        str(exe),
        "--converted", str(converted),
        "--work", str(work),
        "--lod-mode", mode,
        "--cluster-triangle-threshold", str(threshold),
        "--jobs", str(jobs),
    ]
    if bench_baked:
        command.append("--bench-baked")
    if previous:
        command += ["--previous", str(previous)]
    if plugin:
        command += ["--simulate-plugin-change", plugin]
    started = time.perf_counter()
    result = subprocess.run(command, capture_output=True, text=True)
    wall = time.perf_counter() - started
    if result.returncode != 0:
        sys.exit(f"{' '.join(command)} failed:\n{result.stderr}")
    report = json.loads(result.stdout)
    report["process_wall_ms"] = wall * 1000.0
    # The stage writes its tables into a database of its own, so its size is the growth.
    tables = work / "skyrim_world.db"
    report["database_growth_bytes"] = tables.stat().st_size if tables.exists() else 0
    return report


def median(values):
    ordered = sorted(values)
    middle = len(ordered) // 2
    return ordered[middle] if len(ordered) % 2 else (ordered[middle - 1] + ordered[middle]) / 2


def reduce_runs(reports):
    """One report whose timings, wall time and peak memory are medians over the repeats; counts and
    bytes come from the first (they do not vary). `total_spread_ms` keeps the min and max."""
    merged = json.loads(json.dumps(reports[0]))
    for key in merged["timings"]:
        merged["timings"][key] = median([r["timings"][key] for r in reports])
    merged["process_wall_ms"] = median([r["process_wall_ms"] for r in reports])
    peaks = [r["peak_working_set_bytes"] for r in reports if r["peak_working_set_bytes"] is not None]
    merged["peak_working_set_bytes"] = median(peaks) if peaks else None
    totals = [r["timings"]["total_ms"] for r in reports]
    merged["total_spread_ms"] = [min(totals), max(totals)]
    merged["repeats"] = len(reports)
    return merged


def mib(value):
    return "n/a" if value is None else f"{value / 1048576:.0f} MiB"


def seconds(ms):
    return f"{ms / 1000:.1f} s"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", required=True, type=Path)
    parser.add_argument("--converted", required=True, type=Path)
    parser.add_argument("--work-root", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--plugin", default="Update.esm")
    parser.add_argument("--threshold", type=int, default=1024)
    parser.add_argument("--jobs", type=int, default=os.cpu_count() or 1)
    parser.add_argument("--terrain-lod-seconds", type=float)
    parser.add_argument("--package-seconds", type=float)
    parser.add_argument("--keep-work", action="store_true", help="keep every run's work folder")
    parser.add_argument("--repeats", type=int, default=3, help="whole-matrix repeats; medians are reported")
    args = parser.parse_args()
    args.work_root.mkdir(parents=True, exist_ok=True)

    plans = [
        ("baked", "baked", True),
        ("cluster", "cluster", True),
        ("cluster-only", "cluster", False),
    ]
    repeats = {label: {"cold": [], "warm": [], "plugin": []} for label, _, _ in plans}
    for repeat in range(args.repeats):
        for label, mode, bench in plans:
            cold_dir = args.work_root / f"r{repeat}-{label}-cold"
            warm_dir = args.work_root / f"r{repeat}-{label}-warm"
            plugin_dir = args.work_root / f"r{repeat}-{label}-plugin"
            print(f"repeat {repeat + 1}/{args.repeats} {label}: cold", flush=True)
            cold = run(args.exe, args.converted, cold_dir, None, mode, bench, args.threshold, args.jobs, None)
            print(f"repeat {repeat + 1}/{args.repeats} {label}: warm", flush=True)
            warm = run(args.exe, args.converted, warm_dir, cold_dir, mode, bench, args.threshold, args.jobs, None)
            if not args.keep_work:
                shutil.rmtree(cold_dir, ignore_errors=True)
            print(f"repeat {repeat + 1}/{args.repeats} {label}: one plugin changed ({args.plugin})", flush=True)
            changed = run(args.exe, args.converted, plugin_dir, warm_dir, mode, bench, args.threshold, args.jobs, args.plugin)
            repeats[label]["cold"].append(cold)
            repeats[label]["warm"].append(warm)
            repeats[label]["plugin"].append(changed)
            if not args.keep_work:
                shutil.rmtree(warm_dir, ignore_errors=True)
                shutil.rmtree(plugin_dir, ignore_errors=True)
    results = {
        label: {name: reduce_runs(reports) for name, reports in runs.items()}
        for label, runs in repeats.items()
    }

    lines = []
    add = lines.append
    add("# Cluster LOD against baked LOD: converter stage measurements")
    add("")
    add(f"Input: `{args.converted}`; threshold {args.threshold} triangles; {args.jobs} jobs; "
        f"plugin changed in the third run of each mode: {args.plugin}; medians of {args.repeats} "
        "repeats (stage total shows min to max). \"Cold\" means no previous cluster or baked output; "
        "the operating system's file cache is warm after the first repeat, so read times here are "
        "optimistic against a first conversion on a freshly booted machine.")
    add("")
    add("The baked side is a stand-in (plain LOD chains merged per 16x16-cell block); no baked "
        "object chunk builder exists on `main` (#106 is unmerged). Stage times below are the "
        "stage alone; the model pass runs load, classify, build and chain in parallel, so the "
        "split of that pass is attributed by summed CPU time (an estimate).")
    add("")
    if args.terrain_lod_seconds is not None:
        add(f"Rest of LOD (terrain chunk compiler, same in both modes): {args.terrain_lod_seconds:.0f} s, "
            "from the full conversion of this data.")
    else:
        add("Rest of LOD (terrain chunk compiler): not measured here.")
    if args.package_seconds is not None:
        add(f"Whole-package conversion without clusters: {args.package_seconds:.0f} s.")
    else:
        add("Whole-package conversion time: not measured here; add a full run for the headline number.")
    add("")
    add("## Conversion time per stage")
    add("")
    add("| mode | run | probe | classify | model load (attr.) | baked build (attr.) | cluster build (attr.) | tables | stage total (min to max) | process wall | peak memory |")
    add("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for label, runs in results.items():
        for run_name in ("cold", "warm", "plugin"):
            r = runs[run_name]
            t = r["timings"]
            add(f"| {label} | {run_name} | {seconds(t['probe_ms'])} | {seconds(t['classify_ms'])} | "
                f"{seconds(t['attributed_shared_ms'] - t['classify_ms'])} | {seconds(t['attributed_baked_ms'])} | "
                f"{seconds(t['attributed_cluster_ms'] - t['tables_ms'])} | {seconds(t['tables_ms'])} | "
                f"{seconds(t['total_ms'])} ({r['total_spread_ms'][0] / 1000:.1f} to {r['total_spread_ms'][1] / 1000:.1f}) | "
                f"{seconds(r['process_wall_ms'])} | {mib(r['peak_working_set_bytes'])} |")
    add("")
    add("## Bytes on disk and counts (cold runs)")
    add("")
    add("| mode | cluster payloads | instance blocks | baked chunks | database growth | primitives clustered | clusters | baked primitives | baked output triangles |")
    add("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for label, runs in results.items():
        r = runs["cold"]
        b, c = r["bytes"], r["counts"]
        add(f"| {label} | {mib(b['mesh_payloads'])} | {mib(b['instance_payloads'])} | {mib(b['baked_chunks'])} | "
            f"{mib(r['database_growth_bytes'])} | {c['primitives_clustered']} | {c['clusters']} | "
            f"{c['baked_primitives']} | {c['baked_output_triangles']} |")
    add("")
    add("## What the cluster path did not take (primitive rejects, cold cluster run)")
    add("")
    add("| reason | primitives |")
    add("|---|---:|")
    for reason, count in sorted(results["cluster"]["cold"]["primitive_rejects"].items(), key=lambda kv: -kv[1]):
        add(f"| {reason} | {count} |")
    add("")
    add("## Reading the numbers")
    add("")
    baked_cold = results["baked"]["cold"]["timings"]["total_ms"]
    cluster_cold = results["cluster"]["cold"]["timings"]["total_ms"]
    verdict = "faster" if cluster_cold < baked_cold else "slower"
    add(f"- Cold: `cluster` mode took {seconds(cluster_cold)} against {seconds(baked_cold)} for `baked`: "
        f"{verdict} by {abs(cluster_cold - baked_cold) / 1000:.1f} s.")
    for run_name in ("warm", "plugin"):
        b = results["baked"][run_name]["timings"]["total_ms"]
        c = results["cluster"][run_name]["timings"]["total_ms"]
        add(f"- {run_name}: `cluster` {seconds(c)} against `baked` {seconds(b)}.")
    add("")
    args.out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    (args.out.with_suffix(".json")).write_text(json.dumps(results, indent=2), encoding="utf-8")
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
