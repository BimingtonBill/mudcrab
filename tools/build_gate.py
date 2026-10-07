#!/usr/bin/env python3
"""Machine-wide gate for heavy builds: parallel code, staggered builds.

Learned on 2026-10-07: many coders in several sessions each started cargo at
once (26 cargo + 22 rustc processes); free RAM fell to 3.3 GB and a
conversion's extraction crawled at 4 MB/s. Per-session gate files and
messages were too slow and too easy to miss. This tool is one shared gate for
every session and worker on this PC:

    python tools/build_gate.py run -- cargo nextest run -p converter
        waits for a free build slot, enough free RAM and no hold, then runs the
        command and frees the slot when it ends (any exit, Ctrl+C included).
    python tools/build_gate.py status
        slots in use (who, what, since), holds, free RAM, converter processes.
    python tools/build_gate.py hold set conversion --owner "Portal Dev" --reason "portal-rooms extraction" [--until 17:15]
    python tools/build_gate.py hold clear conversion
        a hold stops NEW gated runs machine-wide (running ones finish). Use it
        for a conversion's extraction/convert phase and for timing windows.

Defaults (override in <gate dir>/config.json): 3 slots, at most 2 per session
(BUILD_GATE_OWNER, else the label's track prefix: p1, p2, p3, portal ...), 8 GB free RAM minimum,
15 GB free on the drive the build writes to (CARGO_TARGET_DIR, else the working folder),
wait up to 120 minutes for a slot (then exit 4 with the reason). The gate
lives in %USERPROFILE%\\.mudcrab\\build-gate, not AppData: the Claude desktop
app is an MSIX package, and Windows redirects its processes' AppData writes to
a private folder no other process sees. It survives folder moves and is shared
by every checkout, worktree and process.

Exit codes: the command's own exit code for `run`; 3 bad arguments; 4 gave up
waiting. Python 3.13, standard library only (ctypes for RAM and PID checks).
"""

from __future__ import annotations

import argparse
import ctypes
import json
import os
import shutil
import subprocess
import sys
import time
from ctypes import wintypes
from datetime import datetime, timedelta

# Not under AppData: the Claude desktop app is an MSIX package, which redirects AppData writes from its own
# processes to a private folder that every other process (consoles, workers, cargo) cannot see (2026-10-07).
GATE_DIR = os.environ.get("MUDCRAB_BUILD_GATE") or os.path.join(os.path.expanduser("~"), ".mudcrab", "build-gate")
DEFAULTS = {"slots": 3, "max_per_owner": 2, "min_free_ram_gb": 8.0, "min_free_disk_gb": 15.0, "max_wait_minutes": 120, "poll_seconds": 10}
GB = 1024**3


def load_config() -> dict:
    config = dict(DEFAULTS)
    path = os.path.join(GATE_DIR, "config.json")
    try:
        with open(path, encoding="utf-8") as handle:
            config.update(json.load(handle))
    except FileNotFoundError:
        pass
    return config


class MEMORYSTATUSEX(ctypes.Structure):
    _fields_ = [
        ("dwLength", wintypes.DWORD),
        ("dwMemoryLoad", wintypes.DWORD),
        ("ullTotalPhys", ctypes.c_ulonglong),
        ("ullAvailPhys", ctypes.c_ulonglong),
        ("ullTotalPageFile", ctypes.c_ulonglong),
        ("ullAvailPageFile", ctypes.c_ulonglong),
        ("ullTotalVirtual", ctypes.c_ulonglong),
        ("ullAvailVirtual", ctypes.c_ulonglong),
        ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
    ]


def free_ram_gb() -> float:
    status = MEMORYSTATUSEX()
    status.dwLength = ctypes.sizeof(MEMORYSTATUSEX)
    ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status))
    return status.ullAvailPhys / GB


def pid_alive(pid: int) -> bool:
    process_query_limited = 0x1000
    still_active = 259
    handle = ctypes.windll.kernel32.OpenProcess(process_query_limited, False, pid)
    if not handle:
        return False
    try:
        code = wintypes.DWORD()
        ok = ctypes.windll.kernel32.GetExitCodeProcess(handle, ctypes.byref(code))
        return bool(ok) and code.value == still_active
    finally:
        ctypes.windll.kernel32.CloseHandle(handle)


def converter_processes() -> list[str]:
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq converter.exe", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True).stdout
    return [line.split(",")[1].strip('"') for line in out.splitlines() if line.startswith('"converter.exe"')]


def read_json(path: str) -> dict | None:
    try:
        with open(path, encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, ValueError):
        return None


def slot_paths(config: dict) -> list[str]:
    return [os.path.join(GATE_DIR, f"slot-{index}.json") for index in range(int(config["slots"]))]


def clear_stale_slots(config: dict) -> None:
    for path in slot_paths(config):
        info = read_json(path)
        if info is not None and not pid_alive(int(info.get("pid", 0))):
            try:
                os.remove(path)
            except OSError:
                pass


def active_holds() -> list[dict]:
    holds = []
    for name in os.listdir(GATE_DIR):
        if not (name.startswith("hold-") and name.endswith(".json")):
            continue
        info = read_json(os.path.join(GATE_DIR, name))
        if info is None:
            continue
        until = info.get("until")
        if until and datetime.fromisoformat(until) < datetime.now():
            info["expired"] = True  # still shown, still blocks: a stale hold is cleared by its owner, not by time
        holds.append(info)
    return holds


def owner_of(label: str) -> str:
    """The session a run belongs to: BUILD_GATE_OWNER, else the label's track prefix (p2-water -> p2)."""
    owner = os.environ.get("BUILD_GATE_OWNER") or label.split("-")[0]
    return {"phase1": "p1", "phase2": "p2", "phase3": "p3"}.get(owner.lower(), owner.lower())


def owner_slots(config: dict, owner: str) -> int:
    return sum(1 for path in slot_paths(config)
               if (info := read_json(path)) is not None and info.get("owner") == owner)


def try_acquire(config: dict, label: str, command: list[str]) -> str | None:
    clear_stale_slots(config)
    owner = owner_of(label)
    if owner_slots(config, owner) >= int(config["max_per_owner"]):
        return None
    for path in slot_paths(config):
        try:
            fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            continue
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump({"pid": os.getpid(), "label": label, "owner": owner, "command": " ".join(command)[:300],
                       "cwd": os.getcwd(), "since": datetime.now().isoformat(timespec="seconds")}, handle)
        return path
    return None


def blocking_reason(config: dict) -> str | None:
    holds = active_holds()
    if holds:
        return "hold: " + "; ".join(f"{h['name']} ({h.get('owner', '?')}: {h.get('reason', '')})" for h in holds)
    ram = free_ram_gb()
    if ram < float(config["min_free_ram_gb"]):
        return f"free RAM {ram:.1f} GB < {config['min_free_ram_gb']} GB"
    # The drive the build writes to: CARGO_TARGET_DIR if set, else the working folder (2026-10-07: coder
    # worktrees filled C: to 0 GB and builds failed with os error 112).
    target = os.environ.get("CARGO_TARGET_DIR") or os.getcwd()
    free_disk = shutil.disk_usage(os.path.splitdrive(os.path.abspath(target))[0] + os.sep).free / GB
    if free_disk < float(config["min_free_disk_gb"]):
        return f"free disk on {os.path.splitdrive(os.path.abspath(target))[0]} {free_disk:.1f} GB < {config['min_free_disk_gb']} GB"
    return None


def cmd_run(args: argparse.Namespace) -> int:
    if not args.command:
        print("build_gate: nothing to run (use: run -- <command>)", file=sys.stderr)
        return 3
    config = load_config()
    label = args.label or os.environ.get("BUILD_GATE_LABEL") or os.path.basename(os.getcwd())
    deadline = time.monotonic() + float(config["max_wait_minutes"]) * 60
    announced = None
    while True:
        reason = blocking_reason(config)
        slot = None if reason else try_acquire(config, label, args.command)
        if slot is None and reason is None:
            owner = owner_of(label)
            reason = (f"{owner} already holds {config['max_per_owner']} slots (max per session)"
                      if owner_slots(config, owner) >= int(config["max_per_owner"])
                      else f"all {config['slots']} build slots busy")
        if slot is not None:
            # Re-check after taking the slot: a hold set in between wins.
            late = blocking_reason(config)
            if late is None:
                break
            os.remove(slot)
            reason = late
        if reason != announced:
            print(f"build_gate: waiting ({reason})", flush=True)
            announced = reason
        if time.monotonic() > deadline:
            print(f"build_gate: gave up after {config['max_wait_minutes']} min ({reason})", file=sys.stderr)
            return 4
        time.sleep(float(config["poll_seconds"]))
    print(f"build_gate: running in {os.path.basename(slot)}: {' '.join(args.command)}", flush=True)
    try:
        return subprocess.call(args.command)
    except KeyboardInterrupt:
        return 130
    finally:
        try:
            os.remove(slot)
        except OSError:
            pass


def cmd_status(_args: argparse.Namespace) -> int:
    config = load_config()
    clear_stale_slots(config)
    print(f"gate: {GATE_DIR}")
    print(f"free RAM: {free_ram_gb():.1f} GB (minimum {config['min_free_ram_gb']} GB)")
    for drive in ("C:\\", "D:\\"):
        print(f"free disk {drive}: {shutil.disk_usage(drive).free / GB:.1f} GB (minimum {config['min_free_disk_gb']} GB for builds there)")
    for path in slot_paths(config):
        info = read_json(path)
        print(f"{os.path.basename(path)}: " + ("free" if info is None else
              f"{info['label']} ({info.get('owner', '?')}) since {info['since']} pid {info['pid']}: {info['command'][:120]}"))
    holds = active_holds()
    print("holds: " + ("none" if not holds else ""))
    for hold in holds:
        note = " (past its END-by time: owner, clear or extend it)" if hold.get("expired") else ""
        print(f"  {hold['name']}: {hold.get('owner', '?')}, {hold.get('reason', '')}, until {hold.get('until') or '-'}{note}")
    pids = converter_processes()
    print(f"converter.exe running: {', '.join(pids) if pids else 'none'}")
    return 0


def cmd_hold(args: argparse.Namespace) -> int:
    path = os.path.join(GATE_DIR, f"hold-{args.name}.json")
    if args.action == "clear":
        try:
            os.remove(path)
            print(f"build_gate: hold '{args.name}' cleared")
        except FileNotFoundError:
            print(f"build_gate: no hold '{args.name}'")
        return 0
    if not args.owner or not args.reason:
        print("build_gate: hold set needs --owner and --reason", file=sys.stderr)
        return 3
    until = None
    if args.until:
        hour, minute = (int(part) for part in args.until.split(":"))
        until_dt = datetime.now().replace(hour=hour, minute=minute, second=0, microsecond=0)
        if until_dt < datetime.now():
            until_dt += timedelta(days=1)
        until = until_dt.isoformat(timespec="minutes")
    with open(path, "w", encoding="utf-8") as handle:
        json.dump({"name": args.name, "owner": args.owner, "reason": args.reason, "until": until,
                   "since": datetime.now().isoformat(timespec="seconds")}, handle)
    print(f"build_gate: hold '{args.name}' set by {args.owner}" + (f" until {until}" if until else ""))
    return 0


def main(argv: list[str] | None = None) -> int:
    os.makedirs(GATE_DIR, exist_ok=True)
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    run = sub.add_parser("run", help="run a heavy command once a build slot is free")
    run.add_argument("--label", help="who runs it (default: BUILD_GATE_LABEL or the folder name)")
    run.add_argument("command", nargs=argparse.REMAINDER)
    sub.add_parser("status", help="show slots, holds, free RAM and converters")
    hold = sub.add_parser("hold", help="set or clear a machine-wide hold")
    hold.add_argument("action", choices=["set", "clear"])
    hold.add_argument("name")
    hold.add_argument("--owner")
    hold.add_argument("--reason")
    hold.add_argument("--until", help="END-by time HH:MM (shown; the owner still clears the hold)")
    args = parser.parse_args(argv)
    if args.cmd == "run" and args.command and args.command[0] == "--":
        args.command = args.command[1:]
    return {"run": cmd_run, "status": cmd_status, "hold": cmd_hold}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
