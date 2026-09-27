# impl-267 - Portal bench: states that look out of an interior

Owned files: crates/engine/src/portal_bench.rs,tools/bench/portal_bench_doors.py,tools/bench/test_portal_bench_doors.py,tools/bench/portal_bench_doors_out.json,tools/bench/portal-bench.ps1,docs/demo/README.md,docs/handoff/impl-267.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check,python -m unittest tools/bench/test_portal_bench_doors.py

Track: portal. Design already done and reviewed: `local/agents/runs/research-266-portal-bench-looking-out.1/report.md` sections 2-5. Follow it; where you deviate, say why in the report.

## Goal
Add two bench states that stand INSIDE an interior and look OUT through its load door at a sunlit exterior, so the destination sun's cost is measured (every current state looks into interiors):
- `closed-out`: inside, facing the closed door.
- `open-out-view`: the same pose with the door fully open; the portal draws the exterior.

## What to build (from the design)
1. `tools/bench/portal_bench_doors.py`: a hand-picked `LOOKING_OUT` table (interior-side refs 0x0001CBAF Sven's House, 0x000133FB Alvor's House, 0x00013419 Sleeping Giant Inn, 0x000918E6 Uthgerd's House), validated against the world DB when generating (kind must be `interior-to-exterior`, a return link must exist, else fail loudly), written to a second output `tools/bench/portal_bench_doors_out.json`. The main list's output must stay byte-identical. Generate the new JSON from `C:\Modding\SkyrimConverted\skyrim_world.db`, opened read-only.
2. `tools/bench/test_portal_bench_doors.py`: tests for the out table (writes interior-side doors; refuses an out-door that leads indoors; main list unchanged).
3. `crates/engine/src/portal_bench.rs`: `BenchDoor.kind` (`#[serde(default)]`); `BenchState::{ClosedOut, OpenOutView}` with names, poses and `ALL` so the summary prints them; for `interior-to-exterior` doors run only those two states, and keep the current chain unchanged for every other door. `door_pose` takes the standoff as a parameter: 160 units for the out states (320 clips Alvor's far wall), the current value elsewhere. A door whose `outward` is `None` is skipped with the existing SKIP log in the out states (no model-axis fallback). Add a test that the checked-in out file parses, beside the existing checked-in-file test.
4. `tools/bench/portal-bench.ps1`: add the two state names where the states are listed.
5. `docs/demo/README.md`, section "The portal bench": the out-run command, one short paragraph.

Not in scope: a sun-cascade variant flag (the old `--portal-sun-cascades` is not on this branch), any change to how other states measure.

## Checks
- The acceptance commands above, in your worktree; check your new tests appear in the run.
- One real out-run, quick build (not a timing run; numbers don't matter, only that it runs). You cannot run PowerShell: read the engine command line `tools/bench/portal-bench.ps1` builds and run `target/quick/engine.exe` in your worktree directly with the same bench flags and the out door file. Report each state's `portal_active` and the "view settled after" lines. Expected wall time about 30 s.

## Build and run rules
- Build: `cargo build --profile quick --bin engine` in your own worktree with its own target, `CARGO_BUILD_JOBS=6`; check `quick/engine.d` names your worktree.
- If the lead messages HOLD, finish the command in hand and start no cargo or engine command until told to resume.

## Commits
Do not commit; the lead integrates. Handoff note in `docs/handoff/impl-267.md`.

## Report
Files changed with line counts, test names, the out-run's per-state `portal_active` and settle lines, and any deviation from the design.
