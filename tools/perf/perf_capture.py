#!/usr/bin/env python3
"""Profiling captures of the engine, under the run-class rules (docs/process/profiling-tools.md).

    python tools/perf/perf_capture.py check
        which profilers are installed (Tracy, PIX, RenderDoc), their versions, the GPU.

    python tools/perf/perf_capture.py run tracy --engine <engine built with --features tracy> \
        --owner "<thread>" --label "<what>" -- <engine args>
        Tracy: CPU spans for the main and render threads. Starts `tracy-capture`, then the engine,
        exports per-zone statistics with `tracy-csvexport` (inclusive and self time) and writes
        summary.md (top zones). Compare two captures with tools/perf/tracy_report.py diff.
        Build the engine with `cargo build --profile quick -p engine --features tracy`.

    python tools/perf/perf_capture.py run pix --engine <engine.exe> --owner "<thread>" --label "<what>" \
        [--pix-frames 1] -- <engine args>
        PIX GPU capture on the DX12 backend (the wrapper sets WGPU_BACKEND=dx12). pixtool can only
        capture a process it launched, a moment after launch, and cannot wait for a scene to settle:
        use it for synthetic and early-scene GPU timings; for a settled scene use the PIX GUI.
        Writes capture.wpix, events.csv (per-event GPU durations) and gpu-summary.md (top events).

A capture is a TEST run, never a TIMING run: the profiler slows the engine, so no fps or frame time
is read from it. The wrapper starts the engine through `build_gate.py test` (refused while any hold
is set, up to 3 at once), adds `--run-label` and `--run-owner` for the window title, writes the
capture under D:/Mudcrab/perf/<date>-<label>/ with an OWNER.txt, and leaves a meta.json (tool and
version, engine, git commit, arguments) beside it so an agent can find and explain the capture later.
The gate itself is not modified: this only calls its `test` command.
"""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import glob
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

GATE = Path("C:/Users/Brannock/.mudcrab/build-gate/build_gate.py")
PERF_ROOT = Path("D:/Mudcrab/perf")
REPO = Path(__file__).resolve().parents[2]

PIX_GLOB = "C:/Program Files/Microsoft PIX/*/pixtool.exe"
RENDERDOC = Path("C:/Program Files/RenderDoc/renderdoccmd.exe")
TRACY_DIRS = ["C:/Tools/tracy", "C:/Program Files/Tracy"]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import tracy_report  # noqa: E402


def newest(pattern: str) -> Path | None:
    found = sorted(glob.glob(pattern), key=lambda p: [int(n) for n in re.findall(r"\d+", p)])
    return Path(found[-1]) if found else None


def find_tracy(name: str) -> Path | None:
    """tracy-capture / tracy-csvexport / tracy-profiler: PATH, the usual folders, then winget packages."""
    found = shutil.which(name)
    if found:
        return Path(found)
    patterns = [f"{folder}/**/{name}.exe" for folder in TRACY_DIRS]
    patterns.append(os.path.expandvars(f"%LOCALAPPDATA%/Microsoft/WinGet/Packages/wolfpld.tracy*/**/{name}.exe"))
    for pattern in patterns:
        hits = glob.glob(pattern, recursive=True)
        if hits:
            return Path(hits[0])
    return None


def cmd_check(_: argparse.Namespace) -> int:
    pix = newest(PIX_GLOB)
    print(f"{'pix':15} {pix or 'NOT INSTALLED'}")
    print(f"{'renderdoc':15} {RENDERDOC if RENDERDOC.exists() else 'NOT INSTALLED'}")
    for name in ("tracy-profiler", "tracy-capture", "tracy-csvexport"):
        path = find_tracy(name)
        print(f"{name:15} {path or 'NOT INSTALLED (the engine bundles Tracy 0.14.0; use a 0.14.x release)'}")
    smi = subprocess.run(
        ["nvidia-smi", "--query-gpu=name,driver_version", "--format=csv,noheader"],
        capture_output=True, text=True,
    )
    print(f"{'gpu':15} {smi.stdout.strip() or 'nvidia-smi not found'}")
    print(f"{'output':15} {PERF_ROOT} (exists: {PERF_ROOT.exists()})")
    return 0


def git_head() -> str:
    out = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=REPO, capture_output=True, text=True)
    return out.stdout.strip()


def slug(text: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "-", text).strip("-")[:60] or "run"


def with_title_flags(engine_args: list[str], label: str, owner: str) -> list[str]:
    args = list(engine_args)
    if "--run-label" not in args:
        args += ["--run-label", label]
    if "--run-owner" not in args:
        args += ["--run-owner", owner]
    return args


def prepare_out(ns: argparse.Namespace) -> tuple[str, Path]:
    stamp = dt.datetime.now().strftime("%Y-%m-%d-%H%M%S")
    out = Path(ns.out) if ns.out else PERF_ROOT / f"{stamp}-{slug(ns.label)}"
    out.mkdir(parents=True, exist_ok=True)
    (out / "OWNER.txt").write_text(
        f"Owner: {ns.owner}\nCreated: {stamp}\nPurpose: {ns.tool} capture, {ns.label}\n"
        "Regenerable by rerunning tools/perf/perf_capture.py. Delete when the finding is written up.\n",
        encoding="utf-8",
    )
    return stamp, out


def gate_command(ns: argparse.Namespace, tail: list[str]) -> list[str]:
    return [sys.executable, str(GATE), "test", "--owner", ns.owner, "--label", f"{ns.tool} {ns.label}", "--"] + tail


def write_meta(out: Path, meta: dict) -> None:
    (out / "meta.json").write_text(json.dumps(meta, indent=2), encoding="utf-8")


def export_gpu_zones(export: Path, trace: Path, out: Path) -> Path | None:
    """GPU pass zones of a Tracy capture as gpu-zones.csv (zone-table layout); None when the capture has none."""
    raw = out / "gpu-events.csv"
    with raw.open("w", encoding="utf-8") as handle:
        subprocess.run([str(export), "-g", str(trace)], stdout=handle, check=False)
    try:
        table = tracy_report.gpu_table(raw)
    except ValueError:
        table = ""
    raw.unlink(missing_ok=True)  # per-event rows are large; the aggregate is what agents read
    if table.count("\n") <= 1:
        return None
    path = out / "gpu-zones.csv"
    path.write_text(table, encoding="utf-8")
    return path


SCENES = Path(__file__).resolve().parent / "scenes.json"
REPO = Path(__file__).resolve().parents[2]
BASELINES = Path("D:/Mudcrab/perf/baselines")


def scene_args(ns: argparse.Namespace) -> list[str]:
    """Engine arguments of a named scene (tools/perf/scenes.json) followed by any `--` arguments."""
    spec = json.loads(SCENES.read_text(encoding="utf-8"))
    scene = spec["scenes"].get(ns.scene)
    if scene is None:
        raise SystemExit(f"unknown scene {ns.scene!r}; scenes: {', '.join(spec['scenes'])}")
    if scene["assets"] and not ns.assets:
        raise SystemExit(f"scene {ns.scene} needs --assets <converted asset folder>")
    scratch = Path(ns.out or f"D:/Mudcrab/perf/{ns.label}") / "scratch"
    values = {"{assets}": ns.assets or "", "{repo}": REPO.as_posix(), "{scratch}": scratch.as_posix()}
    args = []
    for arg in scene["args"]:
        for key, value in values.items():
            arg = arg.replace(key, value)
        args.append(arg)
    if scene["assets"]:
        args += ["--assets", ns.assets]
    scratch.mkdir(parents=True, exist_ok=True)  # the engine's benchmark report goes here, not into the working directory
    args += ["--benchmark-output", (scratch / "benchmark.json").as_posix()]
    return args + ["--benchmark-duration", str(ns.seconds or scene["seconds"])] + spec["common"] + ns.engine_args


def cmd_baseline(ns: argparse.Namespace) -> int:
    """Keep a capture's zone tables as the baseline of a scene, or diff a capture against it."""
    folder = Path(ns.capture)
    target = BASELINES / ns.scene
    names = ("zones.csv", "zones-self.csv", "gpu-zones.csv", "meta.json")
    if ns.action == "save":
        if not (folder / "zones.csv").exists():
            print(f"{folder}: no zones.csv (is it a tracy capture folder?)", file=sys.stderr)
            return 2
        target.mkdir(parents=True, exist_ok=True)
        for name in names:
            if (folder / name).exists():
                shutil.copy2(folder / name, target / name)
        (target / "OWNER.txt").write_text(f"Scene baseline for perf_capture.py (kept to diff later captures against). Saved from {folder} by {os.environ.get('MUDCRAB_RUN_OWNER', 'unknown')}.\n", encoding="utf-8")
        print(f"baseline for {ns.scene} saved to {target}")
        return 0
    if not (target / "zones.csv").exists():
        print(f"no baseline for {ns.scene} in {target}; save one with: perf_capture.py baseline save {ns.scene} <capture folder>", file=sys.stderr)
        return 2
    if not (folder / "zones.csv").exists():
        print(f"{folder}: no zones.csv (is it a tracy capture folder?)", file=sys.stderr)
        return 2
    before_meta = json.loads((target / "meta.json").read_text(encoding="utf-8")) if (target / "meta.json").exists() else {}
    after_meta = json.loads((folder / "meta.json").read_text(encoding="utf-8")) if (folder / "meta.json").exists() else {}
    print(f"baseline {before_meta.get('git_head', '?')} ({before_meta.get('started', '?')}) against {after_meta.get('git_head', '?')} ({after_meta.get('started', '?')})")
    print("Same scene, same build profile and engine arguments only; both runs carry the profiler's overhead.\n")
    print("CPU zones, mean per call:")
    print(tracy_report.diff(tracy_report.load(target / "zones.csv"), tracy_report.load(folder / "zones.csv"), ns.top, ns.min_ms))
    if (target / "gpu-zones.csv").exists() and (folder / "gpu-zones.csv").exists():
        print("\nGPU passes, mean per pass:")
        print(tracy_report.diff(tracy_report.load(target / "gpu-zones.csv"), tracy_report.load(folder / "gpu-zones.csv"), ns.top, ns.min_ms))
    return 0


def cmd_tracy(ns: argparse.Namespace) -> int:
    engine = Path(ns.engine)
    if not engine.exists():
        print(f"engine not found: {engine}", file=sys.stderr)
        return 2
    capture, export = find_tracy("tracy-capture"), find_tracy("tracy-csvexport")
    if capture is None or export is None:
        print("tracy-capture / tracy-csvexport not installed (python tools/perf/perf_capture.py check)", file=sys.stderr)
        return 2
    engine_args = with_title_flags(scene_args(ns) if ns.scene else ns.engine_args, f"tracy {ns.label}", ns.owner)
    stamp, out = prepare_out(ns)
    trace = out / "capture.tracy"
    recorder = [str(capture), "-o", str(trace), "-f"]
    gated = gate_command(ns, [str(engine)] + engine_args)
    meta = {
        "tool": "tracy", "tool_path": str(capture), "engine": str(engine), "engine_args": engine_args,
        "git_head": git_head(), "owner": ns.owner, "label": ns.label, "started": stamp,
        "class": "TEST (never quote fps from a capture)", "recorder": recorder, "command": gated,
    }
    write_meta(out, meta)
    print("capture folder:", out)
    if ns.dry_run:
        print(" ".join(recorder))
        print(" ".join(gated))
        return 0
    # tracy-capture waits for the engine to connect and ends when the engine exits.
    recorder_proc = subprocess.Popen(recorder, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    status = subprocess.run(gated).returncode
    try:
        log, _ = recorder_proc.communicate(timeout=120)
    except subprocess.TimeoutExpired:
        recorder_proc.kill()
        log, _ = recorder_proc.communicate()
    (out / "tracy-capture.log").write_text(log or "", encoding="utf-8")
    if status == 4:
        print("gate refused (a hold is set or all TEST slots are busy); start it after the hold clears.", file=sys.stderr)
    if trace.exists() and trace.stat().st_size > 0:
        for flag, name in (([], "zones.csv"), (["-e"], "zones-self.csv")):
            with (out / name).open("w", encoding="utf-8") as handle:
                subprocess.run([str(export)] + flag + [str(trace)], stdout=handle, check=False)
        zones = tracy_report.load(out / "zones.csv")
        text = tracy_report.summary(zones, ns.frames, 30, None)
        gpu = export_gpu_zones(export, trace, out)
        if gpu:
            frames = ns.frames or tracy_report.frames_in(zones)
            text += "\n\nGPU passes (Bevy render diagnostics, timestamp queries):\n\n"
            text += tracy_report.summary(tracy_report.load(gpu), frames, 20, None)
        else:
            text += "\n\nNo GPU zones in the capture (GPU timestamps unavailable on this backend or adapter)."
        (out / "summary.md").write_text(text + "\n", encoding="utf-8")
        print(text)
    else:
        print("no capture.tracy written; was the engine built with --features tracy? See tracy-capture.log", file=sys.stderr)
    meta["exit_code"] = status
    meta["files"] = sorted(p.name for p in out.iterdir())
    write_meta(out, meta)
    return status


def pix_line(pixtool: str, engine: str, engine_args: list[str], frames: int, capture: Path) -> str:
    """The pixtool command line as one string. pixtool takes the engine's arguments from a single
    `--command-line="..."` option and cannot read it when a tool re-quotes the whole token, so this
    string is handed to the shell as it is."""
    inner = " ".join(f'\\"{a}\\"' if " " in a else a for a in engine_args)
    return (
        f'"{pixtool}" launch "{engine}" --command-line="{inner}" '
        f'take-capture --frames={frames} save-capture "{capture}"'
    )


def top_events(events_csv: Path, top: int = 25) -> str:
    """Largest GPU events of a PIX event list (columns: Queue ID, Parent, Name, Global ID, EOP to EOP ns, TOP to EOP ns)."""
    rows = []
    with events_csv.open(newline="", encoding="utf-8") as handle:
        reader = csv.reader(handle, skipinitialspace=True)
        next(reader, None)
        for row in reader:
            if len(row) < 6 or not row[4].strip():
                continue
            rows.append((float(row[4]), float(row[5] or 0), row[2].replace("<deprecated - use pix3.h instead> ", "pass: "), row[0]))
    rows.sort(reverse=True)
    total = sum(r[0] for r in rows) or 1.0
    lines = ["| event | queue id | EOP-to-EOP us | TOP-to-EOP us | share |", "|---|---:|---:|---:|---:|"]
    for eop, top_eop, name, qid in rows[:top]:
        lines.append(f"| {name} | {qid} | {eop / 1e3:.1f} | {top_eop / 1e3:.1f} | {100 * eop / total:.1f}% |")
    lines.append(f"\n{len(rows)} timed events, {total / 1e6:.3f} ms summed EOP-to-EOP (events overlap, so this is not frame time).")
    return "\n".join(lines)


def cmd_pix_inner(meta_path: str) -> int:
    """Run by the gate: launch the engine under pixtool, capture, then export the event list."""
    meta = json.loads(Path(meta_path).read_text(encoding="utf-8"))
    out = Path(meta_path).parent
    env = dict(os.environ, WGPU_BACKEND="dx12")  # pixtool captures D3D12 only; the engine otherwise picks Vulkan
    status = subprocess.run(meta["pix_line"], shell=True, env=env).returncode
    capture = out / "capture.wpix"
    if status == 0 and capture.exists():
        export = f'"{meta["tool_path"]}" open-capture "{capture}" save-event-list "{out / "events.csv"}" --counters="*Duration*"'
        status = subprocess.run(export, shell=True).returncode
        if status == 0:
            (out / "gpu-summary.md").write_text(top_events(out / "events.csv") + "\n", encoding="utf-8")
    return status


def cmd_pix(ns: argparse.Namespace) -> int:
    engine = Path(ns.engine)
    pixtool = newest(PIX_GLOB)
    if not engine.exists() or pixtool is None:
        print("engine not found, or PIX not installed (python tools/perf/perf_capture.py check)", file=sys.stderr)
        return 2
    engine_args = with_title_flags(scene_args(ns) if ns.scene else ns.engine_args, f"pix {ns.label}", ns.owner)
    stamp, out = prepare_out(ns)
    meta = {
        "tool": "pix", "tool_path": str(pixtool), "engine": str(engine), "engine_args": engine_args,
        "git_head": git_head(), "owner": ns.owner, "label": ns.label, "started": stamp,
        "class": "TEST (never quote fps from a capture)", "env": {"WGPU_BACKEND": "dx12"},
        "pix_line": pix_line(str(pixtool), str(engine), engine_args, ns.pix_frames, out / "capture.wpix"),
    }
    write_meta(out, meta)
    gated = gate_command(ns, [sys.executable, str(Path(__file__).resolve()), "_pix-inner", str(out / "meta.json")])
    print("capture folder:", out)
    if ns.dry_run:
        print(meta["pix_line"])
        print(" ".join(gated))
        return 0
    status = subprocess.run(gated).returncode
    if status == 4:
        print("gate refused (a hold is set or all TEST slots are busy); start it after the hold clears.", file=sys.stderr)
    summary = out / "gpu-summary.md"
    if summary.exists():
        print(summary.read_text(encoding="utf-8"))
    meta["exit_code"] = status
    meta["files"] = sorted(p.name for p in out.iterdir())
    write_meta(out, meta)
    return status


def cmd_renderdoc(ns: argparse.Namespace) -> int:
    """RenderDoc capture of one chosen frame of a settled scene. The engine (built with `--features
    renderdoc`) asks RenderDoc for the capture itself at main frame `--capture-frame`; the wrapper
    starts it under `renderdoccmd capture`, with the Vulkan backend RenderDoc is used with."""
    engine = Path(ns.engine)
    renderdoccmd = RENDERDOC
    if not engine.exists() or not renderdoccmd.exists():
        print("engine not found, or RenderDoc not installed (python tools/perf/perf_capture.py check)", file=sys.stderr)
        return 2
    engine_args = with_title_flags(scene_args(ns) if ns.scene else ns.engine_args, f"renderdoc {ns.label}", ns.owner)
    if "--render-backends" not in engine_args:
        engine_args += ["--render-backends", "vulkan"]
    stamp, out = prepare_out(ns)
    env = {"MUDCRAB_RENDERDOC_CAPTURE_FRAME": str(ns.capture_frame), "MUDCRAB_RENDERDOC_CAPTURE_FRAMES": str(ns.capture_frames)}
    recorder = [str(renderdoccmd), "capture", "-w", "-c", str(out / "capture"), str(engine)] + engine_args
    gated = gate_command(ns, recorder)
    meta = {
        "tool": "renderdoc", "tool_path": str(renderdoccmd), "engine": str(engine), "engine_args": engine_args,
        "git_head": git_head(), "owner": ns.owner, "label": ns.label, "started": stamp,
        "class": "TEST (never quote fps from a capture)", "env": env, "command": gated,
    }
    write_meta(out, meta)
    print("capture folder:", out)
    if ns.dry_run:
        print(" ".join(gated))
        return 0
    status = subprocess.run(gated, env=dict(os.environ, **env)).returncode
    if status == 4:
        print("gate refused (a hold is set or all TEST slots are busy); start it after the hold clears.", file=sys.stderr)
    captures = sorted(out.glob("*.rdc"))
    if captures:
        for capture in captures:
            print("capture:", capture)
        print(f'open one in RenderDoc: "{RENDERDOC.with_name("qrenderdoc.exe")}" <file>')
    else:
        print("no .rdc written; was the engine built with --features renderdoc, and does the run reach frame "
              f"{ns.capture_frame}? (a run shorter than that never asks for the capture)", file=sys.stderr)
    meta["exit_code"] = status
    meta["files"] = sorted(p.name for p in out.iterdir())
    write_meta(out, meta)
    return status


def cmd_run(ns: argparse.Namespace) -> int:
    if ns.tool == "renderdoc":
        return cmd_renderdoc(ns)
    return cmd_tracy(ns) if ns.tool == "tracy" else cmd_pix(ns)


def main() -> int:
    argv = sys.argv[1:]
    if argv and argv[0] == "_pix-inner":
        return cmd_pix_inner(argv[1])
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("check", help="list installed profilers").set_defaults(fn=cmd_check)
    run = sub.add_parser("run", help="capture with Tracy (CPU and GPU passes), PIX (GPU events) or RenderDoc (what is drawn)")
    run.add_argument("tool", choices=["tracy", "pix", "renderdoc"])
    run.add_argument("--engine", required=True)
    run.add_argument("--owner", required=True, help="thread title")
    run.add_argument("--label", required=True, help="what the run is")
    run.add_argument("--frames", type=int, help="tracy: frames in the capture (default: counted from the trace)")
    run.add_argument("--pix-frames", type=int, default=1, help="pix: frames to capture")
    run.add_argument("--capture-frame", type=int, default=600, help="renderdoc: main frame to capture (settle first)")
    run.add_argument("--capture-frames", type=int, default=1, help="renderdoc: frames in the capture")
    run.add_argument("--out", help="capture folder (default D:/Mudcrab/perf/<date>-<label>)")
    run.add_argument("--scene", help="named scene from tools/perf/scenes.json (its arguments go before any after `--`)")
    run.add_argument("--assets", help="converted asset folder, for scenes that load the world")
    run.add_argument("--seconds", type=int, help="scene window in seconds (default: the scene's)")
    run.add_argument("--dry-run", action="store_true", help="print the commands, run nothing")
    run.set_defaults(fn=cmd_run)
    base = sub.add_parser("baseline", help="save a tracy capture as a scene baseline, or diff a capture against it")
    base.add_argument("action", choices=["save", "compare"])
    base.add_argument("scene")
    base.add_argument("capture", help="capture folder (holds zones.csv)")
    base.add_argument("--top", type=int, default=25)
    base.add_argument("--min-ms", type=float, default=0.01)
    base.set_defaults(fn=cmd_baseline)
    engine_args: list[str] = []
    if "--" in argv:  # everything after `--` is the engine's own command line
        split = argv.index("--")
        argv, engine_args = argv[:split], argv[split + 1:]
    ns = parser.parse_args(argv)
    ns.engine_args = engine_args
    return ns.fn(ns)


if __name__ == "__main__":
    sys.exit(main())
