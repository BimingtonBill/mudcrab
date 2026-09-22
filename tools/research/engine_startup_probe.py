"""research-028 positive control: does the engine run at all in this session?

A synthetic benchmark-only run needs no fixture, no assets and no database: if it renders,
exits and writes a report, then anything the fixture runs do afterwards is the fixture's own
behaviour and not the environment. stdout and stderr go to the same file, because a fixture run
in this session logged nothing at all on stderr and the engine's own log is the only way to see
how far it got.

subprocess.run(timeout=...) kills only this script's own child.
"""

from __future__ import annotations

import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "target" / "release" / "engine.exe"
OUT = ROOT / "local" / "research-028"


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    log = OUT / "control.err"
    report = OUT / "control.json"
    started = time.monotonic()
    with log.open("wb") as handle:
        try:
            completed = subprocess.run(
                [
                    str(ENGINE),
                    "--benchmark-only",
                    "--synthetic-instances",
                    "20000",
                    "--benchmark-duration",
                    "3",
                    "--benchmark-output",
                    str(report),
                ],
                stdout=handle,
                stderr=handle,
                timeout=120.0,
            )
            outcome = f"exit code {completed.returncode}"
        except subprocess.TimeoutExpired:
            outcome = "still running when the script's 120 s timeout fired"
    print(f"control: {outcome} after {time.monotonic() - started:.1f} s")
    print(f"control: log {log.stat().st_size} bytes, report written: {report.is_file()}")
    if log.stat().st_size:
        lines = log.read_text(errors="replace").splitlines()
        print("control: first log lines:")
        for line in lines[:4]:
            print(f"    {line[:200]}")
        print(f"control: {len(lines)} log lines in total")


if __name__ == "__main__":
    main()
