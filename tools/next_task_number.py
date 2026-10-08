"""Hand out the next free task number, machine-wide, without clashes.

Usage:
    python tools/next_task_number.py <track> [--label <slug>]

Tracks and ranges come from AGENTS.md; numbers already used anywhere in tasks/, local/agents/,
local/agents/runs/ or the claims ledger are skipped. A claim is written to
~/.mudcrab/task-numbers.csv under a lock, so two sessions asking at once get different numbers.
Prints the number.
"""

from __future__ import annotations

import argparse
import csv
import os
import re
import sys
import time
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = Path.home() / ".mudcrab" / "task-numbers.csv"
LOCK = LEDGER.with_suffix(".lock")

# Shared ranges (AGENTS.md). "lead" is Team Lead's own work; tracks keep their documented ranges.
RANGES: dict[str, list[range]] = {
    "lead": [range(1086, 1900)],
    "p2": [range(1086, 1900)],
    "portal": [range(1086, 1900)],
    "p1": [range(1900, 2000)],
    "p4": [range(2000, 2100)],
    "p5": [range(2100, 2200)],
    "p3": [range(900, 1000)],
}

NUMBER = re.compile(r"(?:^|[-_.])(\d{3,4})(?=[-_.])")


def used_numbers() -> set[int]:
    used: set[int] = set()
    for folder in (ROOT / "tasks", ROOT / "tasks" / "deepseek", ROOT / "tasks" / "claude",
                   ROOT / "local" / "agents", ROOT / "local" / "agents" / "runs"):
        if folder.is_dir():
            for name in os.listdir(folder):
                used.update(int(m) for m in NUMBER.findall("-" + name))
    if LEDGER.exists():
        with LEDGER.open(newline="", encoding="utf-8") as f:
            for row in csv.reader(f):
                if row and row[0].isdigit():
                    used.add(int(row[0]))
    return used


def locked(fn):
    LEDGER.parent.mkdir(parents=True, exist_ok=True)
    for _ in range(100):
        try:
            fd = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            break
        except FileExistsError:
            if time.time() - LOCK.stat().st_mtime > 30:  # stale lock
                LOCK.unlink(missing_ok=True)
            time.sleep(0.1)
    else:
        sys.exit("next_task_number: ledger lock busy")
    try:
        return fn()
    finally:
        os.close(fd)
        LOCK.unlink(missing_ok=True)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("track", choices=sorted(RANGES))
    ap.add_argument("--label", required=True, help="slug of the task the number is for")
    args = ap.parse_args()

    def claim() -> int:
        used = used_numbers()
        for r in RANGES[args.track]:
            for n in r:
                if n not in used:
                    with LEDGER.open("a", newline="", encoding="utf-8") as f:
                        csv.writer(f).writerow([n, args.track, args.label, datetime.now().isoformat(timespec="seconds")])
                    return n
        sys.exit(f"next_task_number: range for {args.track} is full")

    print(locked(claim))


if __name__ == "__main__":
    main()
