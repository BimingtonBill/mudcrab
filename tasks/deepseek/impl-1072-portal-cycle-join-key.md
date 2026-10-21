# impl-1072-portal-cycle-join-key (track: Portal)

## Goal
Fix the documented join key between `render-cycles.csv` and `frame-series.csv` (impl-285, merged into portal as
903342c9 + d2be20ee) and pin it with a test. Review: local/agents/runs/review-1065-portal-merge-285.muse.1/report.md
(findings 1, 2 and the missing test in 4). Read it first.

## Context (crates/engine/src)
- profiling.rs ~531-539 says cycle row n joins frame-series row n. render_timing.rs module doc (~line 52) says the same.
- Muse traced: render_timing.rs ~217-218 and ~529 count cycles from 1 (cycle starts 0, incremented at the end of
  `start_main`); profiling.rs ~423, ~476-491 number frame rows from `self.frame`, which starts at 0 and increments in
  end-of-frame `sample_frame`. So cycle n would join frame row n-1. Both `real_ms` columns read the same `First`
  `Time<Real>` delta (render_timing.rs ~620-622, profiling.rs ~259-261).

## Work (2-3 items, small)
1. Verify the relation in code (and with a test that drives a few frames through `begin_frame` /
   `extend_render_cycles` or the real systems). If cycle n == frame row n-1: correct both doc comments (state the
   relation and that equal `real_ms` confirms the join). Prefer fixing the docs over renumbering; renumbering only if
   a one-line change makes cycle == frame and nothing else reads the numbers (say which you chose and why).
2. Document in the render_timing.rs module doc that the render-thread phases are a lower-bound decomposition of
   `render_thread` (inter-mark gaps unattributed; swapchain wait sits inside prepare) - Muse finding 2. Doc only.
3. Add the missing test that a late phase write to an already-handed-over cycle cannot produce a duplicate row
   (`cycle_rows_flushed_through` guard, render_timing.rs ~418-434).

## Scope (owned files)
crates/engine/src/profiling.rs, crates/engine/src/render_timing.rs. Nothing else. Never git stash, amend, rebase or
push. Commit only these paths, subject prefix `[deepseek] impl-1072:`.

## Done when
Through the build gate (BUILD_GATE_OWNER=portal); check `df -h /c` before each build, below 15 GB stop and report:
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo fmt --all -- --check`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
`python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
About 40 steps.

## Report
The verified relation (with the code lines that prove it), the doc lines changed, the new tests' names, check results.
