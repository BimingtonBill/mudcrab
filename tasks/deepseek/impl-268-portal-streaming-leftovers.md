# impl-268 - Streaming audit leftovers: retried failures, one grid offset, the Full commit budget

Owned files: crates/engine/src/streaming.rs,crates/engine/src/demo_tour.rs,crates/engine/src/pose_capture.rs,crates/engine/src/app.rs,docs/handoff/impl-268.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Open audit findings; search each id in `local/agents/memory/findings.jsonl` for the full evidence. Line numbers are approximate; find the code by name.

## Goal
1. **ES-20260928-01 (streaming.rs `has_failed`, ~170):** its contract says a failed cell "will not be repeated", but `retry_failed_interior` (~991-1006) re-requests a failed interior the camera stands in up to `INTERIOR_FAILED_RETRIES` times, and `has_failed` reads true in the gaps between a failure and its retry. Callers treat it as final (`settle_counts` in shots.rs ~1333; `destination_is_loaded` in transition.rs ~1145). Make `has_failed` false while a retry is still owed for that cell, true once retries are spent; keep the contract comment true. Test both halves (a retried interior is not "failed" until its retries run out; an exterior failure is unchanged). Do not edit shots.rs or transition.rs.
2. **ES-20260926-03 (streaming.rs, the grid-to-render offset):** the same formula is written in `cell_translation` (~2379), `reposition_cell_roots` (~2835), `plan_cells` (~477) and inverted in `render_position` (~2402), and repeated in `demo_tour.rs` (~879), `pose_capture.rs` (~168) and `app.rs` (~528). Make one function the owner (or `cell_translation`/`render_position` as the pair) and route every copy in your owned files through it. Leave `portal.rs` and `portal_bench.rs` alone but list their copies in your report. Add a test that pins the pair as inverses at a non-zero origin. No behaviour change.
3. **ES-20260926-02 (streaming.rs `over_budget`, ~709):** the Full arm checks only the commit count, while the Terrain arm also checks the elapsed-microsecond budget, so one Full commit can start after the budget is already spent. Decide from the code: if a Full commit can safely be deferred to the next frame when the budget is already spent before it starts (never starving: the first commit of a frame still runs), make it so and test it. If deferring is wrong (say why), keep the behaviour, correct the comment at ~704-708 to state the rule exactly, and add a test that pins it.

Not in scope: shots.rs, transition.rs, portal.rs, portal_bench.rs (another worker owns it today), world/*.

## Checks
- `cargo test -p engine --lib` in your worktree; check your new tests appear in the run (a shared target can serve a stale binary).
- Short walked tour passes: `target/quick/engine.exe --assets C:\Modding\Mudcrab\SkyrimConverted --demo riverwood --walk --demo-tour <dir under your worktree> --tour-doors 2`. Report its last line.

## Build and run rules
- Build: `cargo build --profile quick --bin engine` in your own worktree with its own target, `CARGO_BUILD_JOBS=6`; check `quick/engine.d` names your worktree.
- No timing runs. If the lead messages HOLD, finish the command in hand and start no cargo or engine command until told to resume.

## Commits
Do not commit; the lead integrates. Handoff note in `docs/handoff/impl-268.md`.

## Report
For each item: what changed (file:line) or why not, test names, the tour's last line, and the list of offset copies left in portal.rs and portal_bench.rs.
