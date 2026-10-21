"""Task manager for the project: what on this PC belongs to Mudcrab work, and what it costs.

Terminal view (rich + psutil), refreshed every 2 s. Read-only: it never starts, stops or changes anything.
Shows machine CPU/RAM/disk, the build gate (slots, holds, floors), and every Mudcrab process grouped by
kind (engine, build, converter, tools, agents), with the worktree it runs in and its --run-label/--run-owner.

    python tools/project_monitor.py            # live view, Ctrl+C or q to quit
    python tools/project_monitor.py --once     # print one frame and exit
    python tools/project_monitor.py --json     # one frame as JSON (for other tools)

Needs: pip install psutil rich
"""

import argparse
import json
import os
import re
import subprocess
import sys
import threading
import time

import psutil

GATE = os.path.expanduser("~/.mudcrab/build-gate/build_gate.py")
PROJECT_MARKERS = ("mudcrab", "skyrimconverted")  # lower-case substrings of cwd / cmdline / exe

# (kind, exe-name regexes). First match wins; order matters.
KINDS = [
    ("engine", r"^(engine|mudcrab|mudcrab-engine)\.exe$"),
    ("converter", r"^(converter|mudcrab-converter|converter-gui)\.exe$"),
    ("profiler", r"^(tracy.*|renderdoc.*|qrenderdoc|pixwin|wpa|ncu.*)\.exe$"),
    ("build", r"^(cargo|rustc|rustdoc|cargo-nextest|nextest|link|lld-link|cl|ld|clang|cmake|ninja|build-script-build|build_script_build.*)\.exe$"),
    ("agents", r"^(claude|node|deepseek.*)\.exe$"),
    ("tools", r"^(python|pythonw|py|pwsh|powershell|git|rg|dust|ffmpeg|xedit|sseedit.*|ssedump.*|ktx.*|toktx.*)\.exe$"),
]
KIND_ORDER = ["engine", "converter", "profiler", "build", "tools", "agents"]
STATUS_COLOR = {"engine": "green", "converter": "magenta", "profiler": "yellow",
                "build": "cyan", "tools": "white", "agents": "blue"}


def classify(name: str) -> str | None:
    low = name.lower()
    for kind, pat in KINDS:
        if re.match(pat, low):
            return kind
    return None


def flag(cmd: list[str], name: str) -> str:
    for i, tok in enumerate(cmd):
        if tok == name and i + 1 < len(cmd):
            return cmd[i + 1]
        if tok.startswith(name + "="):
            return tok.split("=", 1)[1]
    return ""


def describe(cmd: list[str], kind: str) -> str:
    """Short human label: run label/owner if given, else the interesting part of the command."""
    label, owner = flag(cmd, "--run-label"), flag(cmd, "--run-owner")
    if label or owner:
        return f"{label} [{owner}]".strip()
    args = [c for c in cmd[1:] if c]
    if kind == "tools" and args:
        script = next((a for a in args if a.lower().endswith((".py", ".ps1"))), "")
        if script:
            return os.path.basename(script) + " " + " ".join(args[args.index(script) + 1:][:3])
    crate = flag(cmd, "--crate-name")
    if crate:
        return f"compiling {crate}"
    if kind == "build" and args:
        return " ".join(args[:4])
    return " ".join(args[:4])


SCRIPT_PURPOSE = {
    "build_gate.py": None,  # handled below
    "work_board_mcp.py": "work board server (feeds the board view)",
    "project_monitor.py": "this monitor",
    "hive.py": "hive: shared notes and thread presence",
    "t3_claim.py": "task queue: claim/add rows",
    "skyrim_rec.py": "Skyrim record dump lookup",
    "perf_capture.py": "profiler capture (Tracy/PIX/RenderDoc)",
    "tracy_report.py": "reads a Tracy capture into a report",
    "disk_report.py": "daily disk report",
    "disk_guard.py": "free-disk check before a big job",
    "team_board.py": "team work board read",
    "work_board.py": "work board build (branches, PRs)",
    "upstream_watch.py": "upstream issues/PRs watch",
    "ds_impl.ps1": "DeepSeek coder worker",
    "ds_status.ps1": "DeepSeek worker status",
    "run_ds_queue.ps1": "DeepSeek worker queue",
    "ds_check.py": "agent usage/pacing check",
    "ds_claude.py": "Claude plan usage record",
    "ds_audit.py": "audit tool",
    "ds_memory.py": "project memory map",
    "merge_pr.py": "PR merge helper",
    "next_task_number.py": "task number lookup",
    "spec_lint.py": "clean-room spec lint",
    "check_citations.py": "research note citation check",
}
CARGO_VERB = {
    "check": "type-check", "clippy": "lint (clippy)", "build": "build", "test": "run tests",
    "nextest": "run tests (nextest)", "run": "run", "fmt": "format check", "doc": "build docs",
    "bench": "benchmark", "install": "install",
}


def purpose(cmd: list[str], kind: str, name: str) -> str:
    """One plain-words line on why this process exists."""
    low = name.lower()
    args = [c for c in cmd[1:] if c]
    joined = " ".join(args)
    label, owner = flag(cmd, "--run-label"), flag(cmd, "--run-owner")
    if kind == "engine":
        what = label or flag(cmd, "--scene") or flag(cmd, "--route") or "game engine"
        return f"engine run: {what}" + (f" for {owner}" if owner else "")
    if kind == "converter":
        return "Skyrim data conversion"
    if kind == "profiler":
        return "profiler / GPU capture tool"
    if low == "rustc.exe":
        crate = flag(cmd, "--crate-name")
        return f"compiling Rust crate {crate}" if crate else "Rust compiler"
    if low in ("link.exe", "lld-link.exe", "ld.exe"):
        return "linking a binary"
    if low in ("cl.exe", "clang.exe"):
        return "compiling C/C++ (a dependency)"
    if low in ("build-script-build.exe",) or low.startswith("build_script_build"):
        return "crate build script"
    if low in ("cargo.exe", "cargo-nextest.exe", "nextest.exe"):
        verb = next((a for a in args if not a.startswith("-")), "")
        if low != "cargo.exe":
            verb = "nextest"
        if verb == "nextest" and "run" in args:
            verb = "nextest"
        pkg = flag(cmd, "-p") or flag(cmd, "--package")
        scope = f" of {pkg}" if pkg else (" (workspace)" if "--workspace" in args else "")
        return f"cargo: {CARGO_VERB.get(verb, verb or 'cargo')}{scope}"
    if kind == "agents":
        if low == "claude.exe":
            return "Claude Code session (one agent thread)"
        return "agent runtime (node)"
    if low in ("python.exe", "pythonw.exe", "py.exe"):
        script = next((os.path.basename(a) for a in args if a.lower().endswith(".py")), "")
        if script == "build_gate.py":
            sub = args[args.index(next(a for a in args if a.lower().endswith("build_gate.py"))) + 1:][:1]
            sub = sub[0] if sub else ""
            rest = joined.split("--", 1)[1].strip() if "--" in joined else ""
            return {"run": f"gate: queued/running {rest[:60]}",
                    "test": "build gate: test engine run",
                    "status": "build gate: status poll (this monitor)",
                    "hold": "build gate: set/clear hold",
                    "guard": "build gate: guarded timing run"}.get(sub, "build gate")
        if script in SCRIPT_PURPOSE:
            return SCRIPT_PURPOSE[script]
        if args[:1] == ["-"] or args[:1] == ["-c"]:
            return "inline Python snippet"
        return f"Python script {script}" if script else "Python"
    if low in ("pwsh.exe", "powershell.exe"):
        script = next((os.path.basename(a) for a in args if a.lower().endswith(".ps1")), "")
        return SCRIPT_PURPOSE.get(script, f"PowerShell script {script}" if script else "PowerShell shell")
    if low == "git.exe":
        return f"git {args[0]}" if args else "git"
    if low == "rg.exe":
        return "ripgrep search"
    if low == "dust.exe":
        return "disk sizing (dust)"
    if low == "ffmpeg.exe":
        return "video/image conversion"
    if low.startswith(("sseedit", "ssedump", "xedit")):
        return "xEdit record tool"
    if low.startswith(("ktx", "toktx")):
        return "texture compression (KTX)"
    return ""


def worktree(cwd: str) -> str:
    """Name of the worktree/checkout a process runs in, from its cwd."""
    c = cwd.replace("\\", "/").rstrip("/")
    if "/.cargo/registry/" in c:
        return "(dependency)"
    m = re.search(r"/worktrees/([^/]+)", c, re.I)
    if m:
        return m.group(1)
    m = re.search(r"/Mudcrab/(mudcrab|merge/[^/]+|builds/[^/]+)", c, re.I)
    if m:
        return m.group(1)
    return os.path.basename(c) if c else ""


def is_project(proc_info: dict, cmd: list[str], kind: str) -> bool:
    if kind in ("engine", "converter", "profiler"):
        return True
    hay = " ".join([proc_info.get("cwd") or "", " ".join(cmd), proc_info.get("exe") or ""]).lower()
    return any(m in hay for m in PROJECT_MARKERS)


def collect_processes(prev: dict) -> list[dict]:
    rows = []
    for p in psutil.process_iter(["pid", "name", "ppid", "create_time"]):
        try:
            name = p.info["name"] or ""
            kind = classify(name)
            if kind is None:
                continue
            if p.pid == os.getpid():
                continue
            cmd = p.cmdline()
            if any(c.endswith("build_gate.py") for c in cmd) and cmd[-1:] == ["status"]:
                continue  # this monitor's own gate poll
            try:
                cwd = p.cwd()
            except (psutil.AccessDenied, OSError):
                cwd = ""
            info = {"cwd": cwd, "exe": ""}
            if not is_project(info, cmd, kind):
                continue
            pid = p.pid
            cpu = p.cpu_percent(None) / max(psutil.cpu_count(), 1)
            prev[pid] = True
            mem = p.memory_info().rss
            rows.append({"pid": pid, "ppid": p.info["ppid"], "name": name, "kind": kind,
                         "cpu": cpu, "rss": mem,
                         "age": time.time() - p.info["create_time"],
                         "tree": worktree(cwd), "what": describe(cmd, kind),
                         "purpose": purpose(cmd, kind, name)})
        except (psutil.NoSuchProcess, psutil.AccessDenied, OSError):
            continue
    live = {r["pid"] for r in rows}
    for pid in [k for k in prev if k not in live and not psutil.pid_exists(k)]:
        prev.pop(pid, None)
    return rows


class GateReader(threading.Thread):
    """Runs `build_gate.py status` in the background (it samples disk for 2 s) and keeps the last text."""

    def __init__(self):
        super().__init__(daemon=True)
        self.text = "gate: not read yet"
        self.ok = False

    def run(self):
        while True:
            if os.path.exists(GATE):
                try:
                    out = subprocess.run([sys.executable, GATE, "status"], capture_output=True, text=True,
                                         encoding="utf-8", errors="replace", timeout=30)
                    self.text, self.ok = out.stdout.strip() or out.stderr.strip(), out.returncode == 0
                except Exception as err:  # noqa: BLE001 - display only
                    self.text, self.ok = f"gate status failed: {err}", False
            else:
                self.text, self.ok = f"no build gate at {GATE}", False
            time.sleep(3)


def parse_gate(text: str) -> dict:
    g = {"slots": [], "holds": [], "lines": []}
    in_holds = False
    for ln in text.splitlines():
        s = ln.strip()
        if re.match(r"slot-\d+\.json", s):
            g["slots"].append((s.split(".json")[0], s.endswith("free")))
        elif s.startswith("holds:"):
            in_holds = True
            rest = s[6:].strip()
            if rest and rest.lower() != "none":
                g["holds"].append(rest)
        elif in_holds and ln.startswith("  ") and not ln.startswith("    ") and s:
            g["holds"].append(s)
        elif s.startswith(("free RAM", "commit headroom", "disk activity", "free disk", "converter.exe")):
            g["lines"].append(s)
            in_holds = False
    return g


def fmt_bytes(n: float) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if n < 1024 or unit == "GB":
            return f"{n:.0f} {unit}" if unit in ("B", "KB") else f"{n:.1f} {unit}"
        n /= 1024
    return "?"


def fmt_age(sec: float) -> str:
    sec = int(sec)
    return f"{sec // 3600}h{sec % 3600 // 60:02d}m" if sec >= 3600 else f"{sec // 60}m{sec % 60:02d}s"


def frame(prev: dict, gate: GateReader) -> dict:
    rows = collect_processes(prev)
    vm = psutil.virtual_memory()
    disks = {}
    for d in ("C:\\", "D:\\"):
        try:
            u = psutil.disk_usage(d)
            disks[d[:2]] = {"free": u.free, "total": u.total}
        except OSError:
            pass
    return {"time": time.strftime("%H:%M:%S"), "cpu": psutil.cpu_percent(None),
            "ram_used": vm.total - vm.available, "ram_total": vm.total, "disks": disks,
            "procs": rows, "gate": parse_gate(gate.text), "gate_ok": gate.ok}


def render(f: dict, height: int | None = None):
    from rich.console import Group
    from rich.panel import Panel
    from rich.table import Table
    from rich.text import Text

    proj_cpu = sum(r["cpu"] for r in f["procs"])
    proj_rss = sum(r["rss"] for r in f["procs"])
    head = Table.grid(padding=(0, 3))
    head.add_row(
        Text(f"CPU {f['cpu']:.0f}%  (project {proj_cpu:.0f}%)", style="bold"),
        Text(f"RAM {fmt_bytes(f['ram_used'])} / {fmt_bytes(f['ram_total'])}  (project {fmt_bytes(proj_rss)})", style="bold"),
        Text("  ".join(f"{k} free {fmt_bytes(v['free'])}" for k, v in f["disks"].items()), style="bold"),
    )

    g = f["gate"]
    slots = Text("slots: ")
    for name, free in g["slots"]:
        slots.append(f"[{name.split('-')[-1]}: {'free' if free else 'BUSY'}] ", style="green" if free else "red")
    gate_lines = [slots] + [Text(ln, style="dim") for ln in g["lines"]]
    for h in g["holds"]:
        gate_lines.append(Text("HOLD " + h, style="bold yellow"))
    if not f["gate_ok"]:
        gate_lines.append(Text("gate status unavailable", style="red"))

    # Rows that fit: terminal height minus header panel (3), gate panel, table title+header (2), summary (1).
    max_rows = 200 if height is None else max(3, height - (3 + len(gate_lines) + 2 + 2 + 1))
    tbl = Table(expand=True, header_style="bold", title="project processes", box=None, pad_edge=False)
    # Fixed-width no-wrap columns so a long "what" can never squeeze the others to nothing.
    fixed = {"no_wrap": True, "overflow": "ellipsis"}
    for col, kw in (("kind", {"min_width": 9, "max_width": 9}),
                    ("pid", {"justify": "right", "min_width": 6, "max_width": 7}),
                    ("cpu%", {"justify": "right", "min_width": 5, "max_width": 6}),
                    ("ram", {"justify": "right", "min_width": 8, "max_width": 9}),
                    ("age", {"justify": "right", "min_width": 6, "max_width": 7}),
                    ("worktree", {"min_width": 10, "max_width": 16}),
                    ("purpose", {"ratio": 3, "min_width": 24}),
                    ("command", {"ratio": 2, "min_width": 14})):
        tbl.add_column(col, **fixed, **kw)
    order = {k: i for i, k in enumerate(KIND_ORDER)}
    for r in sorted(f["procs"], key=lambda r: (order.get(r["kind"], 9), -r["cpu"], -r["rss"]))[:max_rows]:
        tbl.add_row(Text(r["kind"], style=STATUS_COLOR.get(r["kind"], "")), str(r["pid"]), f"{r['cpu']:.1f}",
                    fmt_bytes(r["rss"]), fmt_age(r["age"]), r["tree"], r["purpose"], f"{r['name']} {r['what']}".strip())

    by_kind = {}
    for r in f["procs"]:
        k = by_kind.setdefault(r["kind"], [0, 0.0, 0])
        k[0] += 1
        k[1] += r["cpu"]
        k[2] += r["rss"]
    hidden = len(f["procs"]) - max_rows
    summary = (f"(+{hidden} more not shown)   " if hidden > 0 else "") + "   ".join(f"{k}: {v[0]}x {v[1]:.0f}% {fmt_bytes(v[2])}" for k, v in sorted(by_kind.items(), key=lambda kv: order.get(kv[0], 9)))
    return Group(Panel(head, title=f"Mudcrab project monitor  {f['time']}"),
                 Panel(Group(*gate_lines), title="build gate"),
                 tbl,
                 Text(summary or "no project processes running", style="dim"))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--once", action="store_true", help="print one frame and exit")
    ap.add_argument("--json", action="store_true", help="one frame as JSON and exit")
    ap.add_argument("--interval", type=float, default=2.0)
    a = ap.parse_args()

    prev: dict = {}
    psutil.cpu_percent(None)
    gate = GateReader()
    gate.start()
    collect_processes(prev)  # prime per-process cpu counters
    time.sleep(1.0 if (a.once or a.json) else 0.2)
    if a.once or a.json:
        for _ in range(30):  # wait for the first gate reading (its disk sample takes ~2 s)
            if gate.ok or not os.path.exists(GATE):
                break
            time.sleep(0.2)
        f = frame(prev, gate)
        if a.json:
            print(json.dumps(f, indent=1))
        else:
            from rich.console import Console
            Console(width=max(Console().width, 130)).print(render(f))
        return 0

    from rich.live import Live
    import msvcrt  # Windows only, like the rest of this tool's paths

    def quit_pressed() -> bool:
        while msvcrt.kbhit():
            if msvcrt.getwch().lower() == "q":
                return True
        return False

    try:
        with Live(render(frame(prev, gate)), refresh_per_second=2, screen=True) as live:
            while True:
                deadline = time.time() + a.interval
                while time.time() < deadline:
                    if quit_pressed():
                        return 0
                    time.sleep(0.1)
                live.update(render(frame(prev, gate), live.console.height))
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
