#!/usr/bin/env python3
"""Named hardware gate for production stable cluster compaction (including ignored tests)."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

HARDWARE_TESTS = {
    "stable_compaction_matches_cpu_repeatedly",
    "stable_order_ignores_instances_appended_after_planning",
    "stable_lod_selection_follows_projected_error_rules",
    "stable_visible_set_matches_frozen_atomic_cull",
    "stable_views_share_instances_with_independent_scratch",
}


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", default="quick")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    gate = Path.home() / ".mudcrab/build-gate/build_gate.py"
    if not os.environ.get("CARGO_TARGET_DIR"):
        parser.error("set CARGO_TARGET_DIR to this session's owned target folder")
    env = dict(os.environ, CARGO_BUILD_JOBS="2", PYTHONUTF8="1")
    build = subprocess.run(
        [sys.executable, str(gate), "run", "--label", "stable-cluster-gpu-build", "--",
         "cargo", "test", "-p", "engine", "--test", "cluster_stable_compaction",
         "--profile", args.profile, "--no-run", "--message-format=json"],
        cwd=root, env=env, stdout=subprocess.PIPE, text=True, encoding="utf-8",
    )
    executables = set()
    for line in build.stdout.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            print(line, flush=True)
            continue
        if event.get("reason") == "compiler-message":
            print(event["message"].get("rendered", ""), end="", flush=True)
        if (event.get("reason") == "compiler-artifact"
                and event.get("target", {}).get("name") == "cluster_stable_compaction"
                and event.get("executable")):
            executables.add(event["executable"])
    if build.returncode:
        return build.returncode
    if len(executables) != 1:
        sys.exit(f"expected one compiled GPU test executable, got {executables}")
    executable = executables.pop()
    print(f"hardware gate executable: {executable}", flush=True)
    listed = subprocess.run(
        [executable, "--list", "--ignored"], cwd=root, env=env,
        capture_output=True, text=True, encoding="utf-8", check=True,
    )
    names = {line.removesuffix(": test") for line in listed.stdout.splitlines()}
    if missing := HARDWARE_TESTS - names:
        sys.exit(f"required ignored hardware proofs are missing: {sorted(missing)}")
    # The test rejects software adapters and validation/readback failures. Serialize
    # fixtures on one GPU; CPU nextest deliberately leaves these ignored.
    return subprocess.call(
        [sys.executable, str(gate), "test", "--label", "stable-cluster-gpu-proof", "--",
         executable, "--include-ignored", "--nocapture", "--test-threads=1"],
        cwd=root, env=env,
    )


if __name__ == "__main__":
    raise SystemExit(main())
