#!/usr/bin/env python3
"""Pre-flight check for disk space and the shared GPU before a big job.

Run it before anything that writes a lot (a build in a new folder, a
conversion, a worker worktree) or uses the GPU heavily (a timing run, a
capture). It reads only, except for the optional --ledger line.

    python tools/disk_guard.py --drive D: --need 20
    python tools/disk_guard.py --drive D: --need 20 --ledger "Phase 1 conversion into D:\\Mudcrab\\x"
    python tools/disk_guard.py --gpu

Exit codes: 0 go; 1 not enough space (free minus need would fall below the
floor); 2 the GPU lock is held by another session (with --gpu); 3 bad
arguments.

The floor, the ledger and the GPU lock are shared with the other projects on
this PC (Film VFX, DAO Xbox): D:\\VFXEnhance\\coordination\\RESOURCES.md.
Python 3.13, standard library only.
"""

from __future__ import annotations

import argparse
import os
import shutil
import sys
from datetime import datetime

COORDINATION = r"D:\VFXEnhance\coordination"
RESOURCES = os.path.join(COORDINATION, "RESOURCES.md")
GPU_LOCK = os.path.join(COORDINATION, "gpu.lock")
FLOOR_GB = 10.0  # never go below this on C: or D:
WARN_GB = 15.0  # below this, post in the ledger and message the others first
LEDGER_THRESHOLD_GB = 5.0  # jobs growing more than this get a ledger line
GB = 1024**3


def free_gb(drive: str) -> float:
    root = drive.rstrip("\\/") + "\\"
    return shutil.disk_usage(root).free / GB


def check_space(drive: str, need: float) -> tuple[bool, str]:
    free = free_gb(drive)
    after = free - need
    line = f"{drive} {free:.1f} GB free; after this job about {after:.1f} GB (floor {FLOOR_GB:.0f} GB)"
    if after < FLOOR_GB:
        return False, f"STOP: {line}. Free space first (owner's yes) or wait."
    if after < WARN_GB:
        return True, f"GO, but warn: {line}. Post a ledger line and message the other sessions first."
    return True, f"GO: {line}."


def read_gpu_lock() -> str | None:
    try:
        with open(GPU_LOCK, encoding="utf-8") as handle:
            return handle.read().strip()
    except FileNotFoundError:
        return None


def append_ledger(text: str, need: float) -> None:
    stamp = datetime.now().strftime("%Y-%m-%d %H:%M")
    with open(RESOURCES, "a", encoding="utf-8") as handle:
        handle.write(f"- {stamp}: Skyrim project: {text} (expected growth about {need:.0f} GB).\n")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--drive", help="drive the job writes to, e.g. D:")
    parser.add_argument("--need", type=float, default=0.0, help="GB the job will add")
    parser.add_argument("--ledger", help="also add this line to the shared ledger (jobs over 5 GB)")
    parser.add_argument("--gpu", action="store_true", help="check the shared GPU lock")
    args = parser.parse_args(argv)
    if not args.drive and not args.gpu:
        parser.print_usage()
        return 3

    code = 0
    if args.drive:
        ok, message = check_space(args.drive, args.need)
        print(message)
        if not ok:
            code = 1
        elif args.ledger:
            append_ledger(args.ledger, args.need)
            print(f"Ledger line added to {RESOURCES}.")
        elif args.need > LEDGER_THRESHOLD_GB:
            print(f"Note: over {LEDGER_THRESHOLD_GB:.0f} GB: add a ledger line (--ledger \"...\").")
    if args.gpu:
        lock = read_gpu_lock()
        if lock is None:
            print("GPU: free (no lock file).")
        else:
            print("GPU lock held:\n  " + lock.replace("\n", "\n  "))
            print(
                "Skyrim has priority: message the lock's owner session; it frees the GPU within 5 minutes."
            )
            code = code or 2
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
