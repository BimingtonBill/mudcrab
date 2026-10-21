"""research-028: the fixture shape the lead used, with the ring on and off.

The lead's command names no benchmark duration or frame count, so `collect_and_finish`
(crates/engine/src/metrics.rs) returns without ever asking the process to stop. This runs that
shape twice - the ring at radius 8 and the ring off (radius 2, the old engine) - so the hang can
be seen not to depend on the ring, then once in the Phase 2 acceptance shape with the ring named,
which is the only shape that finishes.

Both streams are captured: the engine buffers its log when stderr is a file and loses the tail
when it is killed, so a killed run's log shows only what had been flushed.

subprocess.run(timeout=...) kills only this script's own child. Nothing is killed by name.
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "target" / "release" / "engine.exe"
ASSETS = "C:/Modding/SkyrimConverted"
OUT = ROOT / "local" / "research-028"

METRICS = (
    "requests_submitted",
    "stale_responses",
    "unloaded_cells",
    "resident_cells",
    "peak_resident_cells",
    "loading_cells",
    "terrain_cells",
    "ring_cells_empty",
    "active_requests",
    "resident_roots",
    "streaming_invariant_failures",
    "origin_rebases",
    "commit_frames",
    "commit_budget_violations",
    "streaming_fixture_validated",
    "streaming_fixture_failures",
)


def run(name: str, args: list[str], timeout: float, report: Path) -> None:
    log = OUT / f"{name}.err"
    started = time.monotonic()
    with log.open("wb") as handle:
        try:
            completed = subprocess.run(
                [str(ENGINE), *args],
                stdout=handle,
                stderr=handle,
                timeout=timeout,
            )
            outcome = f"exit code {completed.returncode}"
        except subprocess.TimeoutExpired:
            outcome = f"still running when the script's {timeout:.0f} s timeout fired"
    seconds = time.monotonic() - started
    text = log.read_text(errors="replace") if log.stat().st_size else ""
    frames = text.count("frame_count")
    errors = [line for line in text.splitlines() if " ERROR " in line or "panicked" in line]
    print(f"--- {name}: {outcome} after {seconds:.1f} s")
    print(f"    frames logged: {frames}; ERROR lines: {len(errors)}")
    for line in errors[-2:]:
        print(f"    {line[:240]}")
    if report.is_file():
        payload = json.loads(report.read_text())
        streaming = payload["streaming"]
        print(
            f"    report: frames={payload['frames']} elapsed={payload['elapsed_seconds']:.1f} "
            f"fps={payload['average_fps']:.1f} passed={payload['passed']}"
        )
        for key in METRICS:
            print(f"    {key} = {streaming.get(key)}")
    else:
        print("    no benchmark report")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    shapes = {
        "nobench-r8": (
            ["--assets", ASSETS, "--streaming-fixture", "--terrain-radius", "8"],
            300.0,
            OUT / "nobench-r8.json",
        ),
        "nobench-r2": (
            ["--assets", ASSETS, "--streaming-fixture", "--terrain-radius", "2"],
            300.0,
            OUT / "nobench-r2.json",
        ),
        "campaign-r8": (
            [
                "--assets",
                ASSETS,
                "--streaming-fixture",
                "--max-commit-ms",
                "16.67",
                "--terrain-radius",
                "8",
                "--benchmark-duration",
                "20",
                "--benchmark-output",
                str(OUT / "campaign-r8.json"),
            ],
            300.0,
            OUT / "campaign-r8.json",
        ),
    }
    # No arguments runs every shape; names select one, so a single shape can be rerun.
    wanted = sys.argv[1:] or list(shapes)
    for name in wanted:
        args, timeout, report = shapes[name]
        run(name, args, timeout, report)


if __name__ == "__main__":
    main()
