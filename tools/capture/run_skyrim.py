#!/usr/bin/env python3
"""Launch Skyrim SE through Mod Organizer 2 under the machine-wide build gate.

Skyrim is a heavy job like a build: it takes the GPU, RAM and disk, and a frame-time run spoils
(and is spoiled by) anything else running. So it goes through the same gate as cargo:

    python tools/capture/run_skyrim.py play  [--label NAME]
        takes a gate slot (waits for free RAM and no hold), starts Skyrim, frees the slot when the
        game exits. For hand-played shots: framing, F8 poses, console work.

    python tools/capture/run_skyrim.py bench --until HH:MM [--label NAME] [--wait-minutes 30]
        a timing window: sets the hold 'timing-skyrim' with --drain (waits for running gated jobs and
        quiet disks), starts Skyrim, and clears the hold when the game exits or --until passes (the
        game is then stopped by its own PID). The gate refuses a second timing window while another
        hold is set, so this waits (up to --wait-minutes) for e.g. the cluster timing hold to clear.
        Use it for the route benchmark (F9 / [Auto] Mode=route in the plugin ini).

The gate itself (C:/Users/Brannock/.mudcrab/build-gate/build_gate.py) is not modified; this script only
calls its `run`, `hold` and `status` commands. Skyrim is stopped only by the PID this script saw start,
never by image name (other sessions may run an engine).
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timedelta

GATE = os.path.join(os.path.expanduser("~"), ".mudcrab", "build-gate", "build_gate.py")
MO2 = r"C:\Modding\MO2-capture\ModOrganizer.exe"
MO2_EXECUTABLE = "SKSE"  # the MO2 executable entry that starts skse64_loader.exe
GAME_EXE = "SkyrimSE.exe"
OWNER = "capture research"


def skyrim_pids() -> set[int]:
    out = subprocess.run(["tasklist", "/FI", f"IMAGENAME eq {GAME_EXE}", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True).stdout
    pids = set()
    for line in out.splitlines():
        parts = [p.strip('"') for p in line.split('","')]
        if len(parts) > 1 and parts[0].strip('"').lower() == GAME_EXE.lower():
            pids.add(int(parts[1]))
    return pids


def launch_and_wait(until: datetime | None, start_timeout: int = 180) -> int:
    before = skyrim_pids()
    try:
        subprocess.Popen([MO2, f"moshortcut://:{MO2_EXECUTABLE}"], cwd=os.path.dirname(MO2))
    except OSError as err:
        if getattr(err, "winerror", None) != 740:
            raise
        # MO2 is flagged "run as administrator": a plain start fails with 740. Ask Windows to elevate (UAC prompt).
        import ctypes
        shown = ctypes.windll.shell32.ShellExecuteW(None, "runas", MO2, f"moshortcut://:{MO2_EXECUTABLE}", os.path.dirname(MO2), 1)
        if shown <= 32:
            raise SystemExit(f"run_skyrim: elevated start of MO2 failed (ShellExecute code {shown})")
    print(f"run_skyrim: MO2 asked to start '{MO2_EXECUTABLE}'", flush=True)
    deadline = time.monotonic() + start_timeout
    mine: set[int] = set()
    while time.monotonic() < deadline and not mine:
        time.sleep(2)
        mine = skyrim_pids() - before
    if not mine:
        print(f"run_skyrim: {GAME_EXE} did not start within {start_timeout} s (MO2 prompt? check its window)", file=sys.stderr)
        return 5
    print(f"run_skyrim: {GAME_EXE} pid {sorted(mine)}", flush=True)
    while mine & skyrim_pids():
        if until and datetime.now() > until:
            for pid in mine:
                print(f"run_skyrim: END-by passed, stopping our pid {pid}", flush=True)
                subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True)
            break
        time.sleep(5)
    print("run_skyrim: game ended", flush=True)
    return 0


PLUGIN_CFG = os.path.join(os.path.dirname(MO2), "mods", "MudcrabCapture", "SKSE", "Plugins", "MudcrabCapture")
LOCK = os.path.join(os.path.dirname(MO2), "capture.lock")


def pid_alive(pid: int) -> bool:
    out = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/NH"], capture_output=True, text=True).stdout
    return str(pid) in out


def take_lock(label: str, wait_minutes: float) -> None:
    """One capture session at a time: the plugin config and the admin MO2 are shared by every run.
    A lock whose owner process is gone is taken over."""
    deadline = time.monotonic() + wait_minutes * 60
    while True:
        try:
            fd = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            os.write(fd, f"{os.getpid()} {label}\n".encode())
            os.close(fd)
            return
        except FileExistsError:
            try:
                owner = open(LOCK, encoding="utf-8").read().strip()
            except OSError:
                continue
            pid = int(owner.split()[0]) if owner.split()[:1] and owner.split()[0].isdigit() else 0
            if not pid or not pid_alive(pid):
                print(f"run_skyrim: stale capture lock '{owner}', taking over", flush=True)
                try:
                    os.remove(LOCK)
                except OSError:
                    pass
                continue
            if time.monotonic() > deadline:
                raise SystemExit(f"run_skyrim: capture lock held by '{owner}' for {wait_minutes} min; giving up")
            print(f"run_skyrim: capture lock held by '{owner}'; waiting 30 s", flush=True)
            time.sleep(30)


def parse_until(text: str) -> datetime:
    hour, minute = (int(p) for p in text.split(":"))
    until = datetime.now().replace(hour=hour, minute=minute, second=0, microsecond=0)
    return until + timedelta(days=1) if until < datetime.now() else until


def cmd_play(args: argparse.Namespace) -> int:
    # Re-run this script's 'launch' step inside the gate, so the slot is held for as long as the game runs.
    inner = [sys.executable, os.path.abspath(__file__), "_launch", "--label", args.label]
    if args.stage:
        inner += ["--stage", args.stage]
    return subprocess.call([sys.executable, GATE, "run", "--label", args.label, "--"] + inner)


def cmd_bench(args: argparse.Namespace) -> int:
    until = parse_until(args.until)
    name = "timing-skyrim"
    deadline = time.monotonic() + args.wait_minutes * 60
    while True:
        code = subprocess.call([sys.executable, GATE, "hold", "set", name, "--owner", OWNER,
                                "--reason", f"Skyrim route benchmark ({args.label})", "--until", args.until, "--drain"])
        if code == 0:
            break
        if code != 4 or time.monotonic() > deadline:
            print(f"run_skyrim: could not take the timing window (gate exit {code})", file=sys.stderr)
            return code or 4
        print("run_skyrim: another hold is set; waiting 60 s", flush=True)
        time.sleep(60)
    try:
        return launch_and_wait(until)
    finally:
        subprocess.call([sys.executable, GATE, "hold", "clear", name])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    play = sub.add_parser("play", help="hand-played session in a gate slot")
    play.add_argument("--label", default="skyrim-play")
    play.add_argument("--stage", help="folder of plugin config files (MudcrabCapture.ini, route.csv) copied in once the slot and the capture lock are ours")
    bench = sub.add_parser("bench", help="timing window with a hold")
    bench.add_argument("--until", required=True, help="END-by time HH:MM (the game is stopped then)")
    bench.add_argument("--label", default="route")
    bench.add_argument("--wait-minutes", type=float, default=30.0)
    launch = sub.add_parser("_launch")
    launch.add_argument("--label", default="skyrim-play")
    launch.add_argument("--stage")
    launch.add_argument("--lock-wait-minutes", type=float, default=60.0)
    args = parser.parse_args()
    if args.cmd == "_launch":
        take_lock(args.label, args.lock_wait_minutes)
        try:
            if args.stage:
                os.makedirs(PLUGIN_CFG, exist_ok=True)
                for name in os.listdir(args.stage):
                    shutil.copy(os.path.join(args.stage, name), os.path.join(PLUGIN_CFG, name))
            return launch_and_wait(None)
        finally:
            try:
                os.remove(LOCK)
            except OSError:
                pass
    return {"play": cmd_play, "bench": cmd_bench}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
