# impl-213 - A quick, automated portal benchmark in dense places, measured in GPU and CPU time

Owned files: crates/engine/src/portal_bench.rs (new),crates/engine/src/config.rs (the new flags only),crates/engine/src/app.rs (plugin registration only, bench runs only),crates/engine/src/lib.rs,tools/bench/portal_bench_doors.py (new),tools/bench/portal-bench.ps1 (new),docs/demo/README.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user (2026-09-25):
- The benchmark "should go to particularly dense spaces where there's likely to be a lot of stress on the portal system, so not Riverwood."
- It "should also be a quick automated process."
- Their machine forces V-Sync in the NVIDIA global settings, so frame times never drop below 16.67 ms. impl-194's `--tour-bench` read 16.67 ms in every state (`local/bench/*.csv`). We need numbers V-Sync cannot hide.

## What to build

### 1. `tools/bench/portal_bench_doors.py`: pick the stress doors from data

Read `C:/Modding/SkyrimConverted/skyrim_world.db` (read-only URI). Rank the load doors by how much the portal has to draw:
- the number of references within about 3000 units of the door in its own space;
- the number of references in the destination cell (for an interior) or within about 3000 units of the destination door (for an exterior);
- prefer doors in the big cities (Whiterun, Solitude, Windhelm, Markarth, Riften worldspaces and their interiors), and mix exterior-to-interior with interior-to-interior.

Write a fixed list of about 8 doors, spread over at least 3 places, to `tools/bench/portal_bench_doors.json`. Include each door's ref id, its space, the reason it was picked, and both counts. Check the file into the repository so every run uses the same doors. The script reproduces the list.

### 2. `--portal-bench <doors.json> --bench-out <file.csv>`: a teleporting bench mode

A new module `portal_bench.rs`. It does not use the demo tour and does not walk. For each door in the list:
1. Put the camera at a standoff in front of the door, in the door's space (reuse `transition::switch_space` and the demo tour's `stand_in_front_of_door` logic), facing it.
2. Wait until streaming settles (as `--shots` does).
3. Measure 3 s each of:
   - `closed`;
   - `open-in-view`: open the door, and wait until it is fully `Open`;
   - `open-behind`: turned 180 degrees;
   - `open-occluded`: if a wall-blocked pose is easy to find, for example turned 90 degrees behind the door frame; skip it otherwise.
4. Close the door and move on.

At the end, write the CSV and a summary, then exit. Use `WinitSettings::continuous()` and `AutoNoVsync`, as bench runs already do (app.rs); frame time is recorded anyway.

### 3. Numbers V-Sync cannot hide

For each state and each door, record the mean and p95 of:
- **GPU time**, from Bevy 0.19's render diagnostics (`RenderDiagnosticsPlugin`, per-pass GPU timings; impl-189 used them):
  - the total;
  - the main opaque pass;
  - the portal camera's passes, if they can be told apart (check how passes are labelled per camera; if they cannot be split, say so and record the total);
- **CPU time**: the main-world frame work, excluding the present wait;
- **frame time**, as before.

Register the diagnostics only in bench runs.

### 4. `tools/bench/portal-bench.ps1`: one command

It:
1. builds `--release --bin engine`;
2. runs `--portal-bench tools/bench/portal_bench_doors.json --bench-out local/bench/<date-time>.csv` (off screen, as tours are);
3. prints the summary;
4. compares it with the previous CSV in `local/bench/`, if there is one, per state, as a delta table.

The whole run should take about 5 minutes after the build. The README says how to run it and that it must run alone.

## Checks

- Unit tests: the door ranking (on a small fixture), the state sequencing, the CSV columns and aggregation.
- Run the script once end to end. Show:
  - the chosen doors;
  - the CSV;
  - that `open-in-view` costs more GPU time than `closed`, and `open-behind` returns near `closed`.
  The numbers can be rough; this run need not be alone.
- Under `local/`, read only `local/bench*`.

## Report

The door list and why each was picked; what each column measures; test names; the CSV and the script's output; the time the run took.
