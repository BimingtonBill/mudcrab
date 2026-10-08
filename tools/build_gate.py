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

Defaults (override in <gate dir>/config.json): 3 slots (a cold build, one whose target folder has
no dependency artifacts yet, takes 2), at most 2 per session
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
DEFAULTS = {"slots": 3, "max_per_owner": 2, "min_free_ram_gb": 8.0, "min_commit_warm_gb": 7.0, "min_commit_cold_gb": 14.0, "preflight_commit_gb": 20.0,
            "warm_peak_gb": 4.0, "cold_peak_gb": 10.0, "min_free_disk_gb": 15.0, "max_wait_minutes": 120, "poll_seconds": 10}
GB = 1024**3
COLD_RLIBS = 150


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


def memory_status() -> MEMORYSTATUSEX:
    status = MEMORYSTATUSEX()
    status.dwLength = ctypes.sizeof(MEMORYSTATUSEX)
    ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status))
    return status


def free_ram_gb() -> float:
    return memory_status().ullAvailPhys / GB


def commit_headroom_gb() -> float:
    """Commit charge left before Windows refuses allocations (commit limit minus committed).

    2026-10-08: a conversion died at start with "memory allocation of 1048576 bytes failed" while free
    physical RAM looked fine: commit headroom was ~8.3 GB of 42.6 GB. Windows fails allocations at the commit
    limit, not when physical RAM runs out, so this is the number that predicts the crash.
    """
    return memory_status().ullAvailPageFile / GB


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
    """The session a run belongs to: BUILD_GATE_OWNER, else a track token in the label, else its first part."""
    aliases = {"phase1": "p1", "phase2": "p2", "phase3": "p3"}
    if os.environ.get("BUILD_GATE_OWNER"):
        owner = os.environ["BUILD_GATE_OWNER"].lower()
        return aliases.get(owner, owner)
    # Worker labels carry the track inside (impl-1900-p1-ktx2, review-650-portal-...), so look for a track
    # token anywhere; otherwise the first part (p2-water -> p2). 2026-10-08: every ds_impl coder counted as "impl".
    parts = [part.lower() for part in label.split("-")]
    for part in parts:
        part = aliases.get(part, part)
        if part in ("p1", "p2", "p3", "portal", "upstream"):
            return part
    return aliases.get(parts[0], parts[0])


def owner_slots(config: dict, owner: str) -> int:
    return sum(1 for path in slot_paths(config)
               if (info := read_json(path)) is not None and info.get("owner") == owner)


def build_profile_dir(command: list[str]) -> str:
    """The target sub-folder a cargo command builds into: release, quick, ..., else debug."""
    for index, arg in enumerate(command):
        if arg == "--release" or arg == "-r":
            return "release"
        if arg in ("--profile", "--cargo-profile") and index + 1 < len(command):
            return {"dev": "debug", "test": "debug", "bench": "release"}.get(command[index + 1], command[index + 1])
        if arg.startswith("--profile=") or arg.startswith("--cargo-profile="):
            name = arg.split("=", 1)[1]
            return {"dev": "debug", "test": "debug", "bench": "release"}.get(name, name)
    return "debug"


def is_cold_build(command: list[str]) -> bool:
    """True when a cargo command would compile the dependency tree from scratch.

    2026-10-08: three cold Bevy builds at once (coders run -NoSeed after the C: incident) pushed free RAM
    past the floor mid-build and rustc died (handle_alloc_error). The floor is only checked at start, so a
    cold build now takes two slots. Cold = the target folder for this profile has fewer than COLD_RLIBS
    dependency libraries or metadata files (a warm Mudcrab target has several hundred; clippy/check leave
    .rmeta only, so both count).
    """
    if not command or os.path.basename(command[0]).lower() not in ("cargo", "cargo.exe"):
        return False
    target = os.environ.get("CARGO_TARGET_DIR") or os.path.join(os.getcwd(), "target")
    deps = os.path.join(target, build_profile_dir(command), "deps")
    try:
        rlibs = sum(1 for name in os.listdir(deps) if name.endswith((".rlib", ".rmeta")))
    except OSError:
        return True
    return rlibs < COLD_RLIBS


def try_acquire(config: dict, label: str, command: list[str], weight: int = 1) -> list[str] | None:
    """Take `weight` free slots at once (all or none); a cold build takes two."""
    clear_stale_slots(config)
    owner = owner_of(label)
    if owner_slots(config, owner) + weight > max(int(config["max_per_owner"]), weight):
        return None
    taken: list[str] = []
    for path in slot_paths(config):
        if len(taken) == weight:
            break
        try:
            fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            continue
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            json.dump({"pid": os.getpid(), "label": label, "owner": owner, "command": " ".join(command)[:300],
                       "cwd": os.getcwd(), "since": datetime.now().isoformat(timespec="seconds"),
                       "cold": weight > 1}, handle)
        taken.append(path)
    if len(taken) == weight:
        return taken
    for path in taken:
        try:
            os.remove(path)
        except OSError:
            pass
    return None


def reserved_commit_gb(config: dict, exclude_pid: int | None = None) -> float:
    """Expected peak commit of the gated builds already running (one entry per build, not per slot).

    A build that has just started has committed little, so two starts a minute apart could both pass the
    floor and then collide at their peaks (2026-10-08). This counts the full peak for every running build,
    which over-reserves for builds already near their peak; that errs on the safe side.
    """
    total, seen = 0.0, set()
    for path in slot_paths(config):
        info = read_json(path)
        if info is None or info.get("pid") == exclude_pid or info.get("pid") in seen:
            continue
        seen.add(info.get("pid"))
        total += float(config["cold_peak_gb"] if info.get("cold") else config["warm_peak_gb"])
    return total


def blocking_reason(config: dict, weight: int = 1, exclude_pid: int | None = None) -> str | None:
    holds = active_holds()
    if holds:
        return "hold: " + "; ".join(f"{h['name']} ({h.get('owner', '?')}: {h.get('reason', '')})" for h in holds)
    ram = free_ram_gb()
    if ram < float(config["min_free_ram_gb"]):
        return f"free RAM {ram:.1f} GB < {config['min_free_ram_gb']} GB"
    # Tiered (2026-10-08): with the user's normal apps open this PC sits at ~9.5 GB headroom, so a single
    # 12 GB floor stalled every build. Warm builds peak far lower than cold ones.
    floor = float(config["min_commit_cold_gb"] if weight > 1 else config["min_commit_warm_gb"])
    commit, reserved = commit_headroom_gb(), reserved_commit_gb(config, exclude_pid)
    if commit - reserved < floor:
        held = f" minus {reserved:.0f} GB reserved for running builds" if reserved else ""
        return f"commit headroom {commit:.1f} GB{held} < {floor:.0f} GB ({'cold' if weight > 1 else 'warm'} floor)"
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
    weight = 2 if is_cold_build(args.command) else 1
    deadline = time.monotonic() + float(config["max_wait_minutes"]) * 60
    announced = None
    while True:
        reason = blocking_reason(config, weight)
        slot = None if reason else try_acquire(config, label, args.command, weight)
        if slot is None and reason is None:
            owner = owner_of(label)
            held = owner_slots(config, owner)
            if held + weight > max(int(config["max_per_owner"]), weight):
                reason = f"{owner} holds {held} of its {config['max_per_owner']} slots" + (
                    " (a cold build needs 2)" if weight > 1 else "")
            else:
                reason = (f"a cold build needs 2 free slots of {config['slots']}" if weight > 1
                          else f"all {config['slots']} build slots busy")
        if slot is not None:
            # Re-check after taking the slot: a hold set in between wins.
            late = blocking_reason(config, weight, exclude_pid=os.getpid())
            if late is None:
                break
            for path in slot:
                os.remove(path)
            reason = late
        if reason != announced:
            print(f"build_gate: waiting ({reason})", flush=True)
            announced = reason
        if time.monotonic() > deadline:
            print(f"build_gate: gave up after {config['max_wait_minutes']} min ({reason})", file=sys.stderr)
            return 4
        time.sleep(float(config["poll_seconds"]))
    print(f"build_gate: running in {', '.join(os.path.basename(path) for path in slot)}"
          f"{' (cold build: 2 slots)' if weight > 1 else ''}: {' '.join(args.command)}", flush=True)
    try:
        return subprocess.call(args.command)
    except KeyboardInterrupt:
        return 130
    finally:
        for path in slot:
            try:
                os.remove(path)
            except OSError:
                pass


def cmd_status(_args: argparse.Namespace) -> int:
    config = load_config()
    clear_stale_slots(config)
    print(f"gate: {GATE_DIR}")
    print(f"free RAM: {free_ram_gb():.1f} GB (minimum {config['min_free_ram_gb']} GB)")
    print(f"commit headroom: {commit_headroom_gb():.1f} GB, {reserved_commit_gb(config):.0f} GB reserved for running builds"
          f" (floors: warm {config['min_commit_warm_gb']}, cold {config['min_commit_cold_gb']},"
          f" conversion {config['preflight_commit_gb']} GB)")
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


def cmd_preflight(args: argparse.Namespace) -> int:
    """For jobs that don't run inside a gate slot (a conversion): check memory before starting.

    Exit 0 = go; 4 = not now (the reason is printed). A conversion needs more commit room than a build,
    so it needs `preflight_commit_gb` (20 GB) after the running builds' expected peaks, and it also refuses
    while gated cold builds are running.
    """
    config = load_config()
    clear_stale_slots(config)
    need_commit = float(config["preflight_commit_gb"]) if args.need_gb is None else args.need_gb
    commit, ram, reserved = commit_headroom_gb(), free_ram_gb(), reserved_commit_gb(config)
    cold = [info["label"] for path in slot_paths(config) if (info := read_json(path)) and info.get("cold")]
    problems = []
    if commit - reserved < need_commit:
        held = f" minus {reserved:.0f} GB reserved for running builds" if reserved else ""
        problems.append(f"commit headroom {commit:.1f} GB{held} < {need_commit:.0f} GB")
    if ram < float(config["min_free_ram_gb"]):
        problems.append(f"free RAM {ram:.1f} GB < {config['min_free_ram_gb']} GB")
    if cold:
        problems.append("cold builds running: " + ", ".join(sorted(set(cold))))
    if problems:
        print("build_gate: NOT NOW: " + "; ".join(problems))
        return 4
    print(f"build_gate: go (commit headroom {commit:.1f} GB, free RAM {ram:.1f} GB)")
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
    # A hold stops new runs only; a run that took its slot just before keeps going (2026-10-08: a cargo check
    # compiled into the first seconds of a timing window). List them, and with --drain wait until they end.
    config = load_config()
    deadline = time.monotonic() + args.drain_minutes * 60
    while True:
        clear_stale_slots(config)
        busy = [info for path in slot_paths(config) if (info := read_json(path)) is not None]
        if not busy:
            print("build_gate: no gated runs in progress")
            return 0
        for info in busy:
            print(f"build_gate: still running: {info['label']} ({info.get('owner', '?')}) pid {info['pid']}: "
                  f"{info['command'][:100]}")
        if not args.drain or time.monotonic() > deadline:
            if args.drain:
                print("build_gate: runs still busy after the drain wait; ask their owners to stop them", file=sys.stderr)
                return 4
            print("build_gate: tell their owners, or re-run with --drain to wait for them")
            return 0
        time.sleep(float(config["poll_seconds"]))


def main(argv: list[str] | None = None) -> int:
    os.makedirs(GATE_DIR, exist_ok=True)
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    run = sub.add_parser("run", help="run a heavy command once a build slot is free")
    run.add_argument("--label", help="who runs it (default: BUILD_GATE_LABEL or the folder name)")
    run.add_argument("command", nargs=argparse.REMAINDER)
    sub.add_parser("status", help="show slots, holds, free RAM and converters")
    pre = sub.add_parser("preflight", help="memory check before a job outside the gate (a conversion)")
    pre.add_argument("--need-gb", type=float, default=None, help="commit room needed (default: preflight_commit_gb, 20)")
    hold = sub.add_parser("hold", help="set or clear a machine-wide hold")
    hold.add_argument("action", choices=["set", "clear"])
    hold.add_argument("name")
    hold.add_argument("--owner")
    hold.add_argument("--reason")
    hold.add_argument("--until", help="END-by time HH:MM (shown; the owner still clears the hold)")
    hold.add_argument("--drain", action="store_true", help="after setting, wait until running gated jobs end")
    hold.add_argument("--drain-minutes", type=float, default=30.0, help="how long --drain waits (default 30)")
    args = parser.parse_args(argv)
    if args.cmd == "run" and args.command and args.command[0] == "--":
        args.command = args.command[1:]
    return {"run": cmd_run, "status": cmd_status, "hold": cmd_hold, "preflight": cmd_preflight}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
