# research-266 - A portal bench state that looks out of an interior

Track: portal. Read-only; change no files except your report.

## Goal
Design (do not implement) an extra state for the portal bench that stands inside an interior and looks OUT through an open load door at an exterior, so the cost of the destination's sun and its shadow cascades is measured. Today every bench state looks from outside into interiors, so the 2026-09-26 try of one sun cascade for the portal (`--portal-sun-cascades 1`) showed no gain.

## Context
- Bench driver: `tools/bench/portal-bench.ps1`; door list: `tools/bench/portal_bench_doors.json` and its generator `tools/bench/portal_bench_doors.py` (with `tools/bench/test_portal_bench_doors.py`). A water variant: `local/bench/portal_bench_doors_riften.json`. Latest results write-up: `local/bench/portal-bench-2026-09-26.md`.
- The engine side the bench drives: search `crates/engine/src` for `benchmark`, `portal_bench`, `PortalBench` and the state names the bench CSVs use (see a CSV in `local/bench/`, e.g. `am-default.csv`).
- Door links (which side of a door is interior): `crates/engine/src/doors.rs`, `crates/engine/src/transition.rs` (`door_map`, `DoorMap`).
- A run is about 133 s over 8 doors; every automated tool must stay as quick as possible.

## Done when
You can say exactly what to add: which states and how the camera is placed inside the interior facing the door (from existing door data: XTEL, doorway anchor), how the door list names interior-side doors (format change or a new file), which Riverwood/Whiterun doors to use (at least 3, with form IDs, chosen so the exterior view is outdoors in daylight), what extra wall time it adds, and which files change.

## Report
1. Current bench flow in 5-10 lines with file:line.
2. The proposed state(s), camera placement rule, and door-list change.
3. The chosen doors (form ID, cell, why).
4. Files to change, with the size of each change, and the added run time.
5. Risks (for example the interior's own lights or the portal cadence skewing the numbers).
