"""research-028: does the streaming fixture exit, and does the terrain ring stream?

Runs the release engine in the four shapes that matter and prints what each one did:

  lead      the lead's command verbatim: --streaming-fixture --terrain-radius 8, no benchmark
            flags, so nothing in the app ever asks it to stop
  ring8     the fixture as it must be run: --benchmark-duration, ring at radius 8
  ring2     the same with the ring off (terrain_radius = stream_radius), the old engine
  campaign  the Phase 2 acceptance shape: --streaming-fixture --max-commit-ms 16.67 and a
            duration, no --terrain-radius at all

Every run is started with subprocess.run(timeout=...), which kills only this script's own
child. Nothing is ever killed by name.
"""

from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "target" / "release" / "engine.exe"
ASSETS = "C:/Modding/SkyrimConverted"
OUT = ROOT / "local" / "research-028"

# The metrics that answer the question: what the fixture waits on, and what the ring did.
METRICS = (
    "frames",
    "elapsed_seconds",
    "average_fps",
    "passed",
    "requests_submitted",
    "responses_received",
    "stale_responses",
    "unloaded_cells",
    "resident_cells",
    "peak_resident_cells",
    "loading_cells",
    "peak_loading_cells",
    "terrain_cells",
    "peak_terrain_cells",
    "cells_upgraded",
    "cells_downgraded",
    "ring_cells_empty",
    "active_requests",
    "peak_active_requests",
    "resident_roots",
    "out_of_range_cell_roots",
    "streaming_invariant_failures",
    "origin_rebases",
    "commit_frames",
    "commit_budget_violations",
    "streaming_fixture_validated",
    "streaming_fixture_failures",
)


def run(name: str, args: list[str], timeout: float) -> dict[str, object]:
    log = OUT / f"{name}.err"
    started = time.monotonic()
    print(f"--- {name}: {timeout:.0f} s budget", flush=True)
    with log.open("wb") as handle:
        try:
            completed = subprocess.run(
                [str(ENGINE), *args],
                stdout=subprocess.DEVNULL,
                stderr=handle,
                timeout=timeout,
            )
            exit_code: int | None = completed.returncode
            timed_out = False
        except subprocess.TimeoutExpired:
            exit_code = None
            timed_out = True
    seconds = time.monotonic() - started
    return {"name": name, "exit_code": exit_code, "timed_out": timed_out, "seconds": seconds}


def summarise(result: dict[str, object], report_path: Path) -> None:
    name = result["name"]
    if result["timed_out"]:
        print(f"{name}: still running after {result['seconds']:.1f} s - killed by the script")
    else:
        print(f"{name}: exit code {result['exit_code']} after {result['seconds']:.1f} s")
    log = OUT / f"{name}.err"
    errors = []
    if log.is_file():
        for line in log.read_text(errors="replace").splitlines():
            if " ERROR " in line or "panicked" in line:
                errors.append(line.strip()[:300])
    print(f"{name}: {len(errors)} ERROR line(s) in the log")
    for line in errors[-3:]:
        print(f"    {line}")
    if not report_path.is_file():
        print(f"{name}: no benchmark report written")
        return
    report = json.loads(report_path.read_text())
    print(f"{name}: scenario={report.get('scenario')} passed={report.get('passed')}")
    streaming = report.get("streaming", {})
    for key in METRICS:
        print(f"    {key} = {streaming.get(key)}")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    runs = [
        (
            "lead",
            ["--assets", ASSETS, "--streaming-fixture", "--terrain-radius", "8"],
            60.0,
            OUT / "lead.json",
        ),
        (
            "ring8",
            [
                "--assets",
                ASSETS,
                "--streaming-fixture",
                "--terrain-radius",
                "8",
                "--benchmark-duration",
                "20",
                "--benchmark-output",
                str(OUT / "ring8.json"),
            ],
            300.0,
            OUT / "ring8.json",
        ),
        (
            "ring2",
            [
                "--assets",
                ASSETS,
                "--streaming-fixture",
                "--terrain-radius",
                "2",
                "--benchmark-duration",
                "20",
                "--benchmark-output",
                str(OUT / "ring2.json"),
            ],
            300.0,
            OUT / "ring2.json",
        ),
        (
            "campaign",
            [
                "--assets",
                ASSETS,
                "--streaming-fixture",
                "--max-commit-ms",
                "16.67",
                "--benchmark-duration",
                "20",
                "--benchmark-output",
                str(OUT / "campaign.json"),
                "--profile-output",
                str(OUT / "profile-campaign"),
                "--profile-scenario",
                "streaming",
            ],
            300.0,
            OUT / "campaign.json",
        ),
    ]
    for name, args, timeout, report in runs:
        summarise(run(name, args, timeout), report)


if __name__ == "__main__":
    main()
