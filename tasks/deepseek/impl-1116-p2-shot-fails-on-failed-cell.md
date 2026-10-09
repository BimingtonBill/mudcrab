# Goal
A `--shots` capture whose own cell FAILED to load must fail the run, not settle and photograph whatever is there. This is a C4 follow-up for #1055; both reviewers of review-1111 recommended it and Team Lead approved it. Track: Phase 2.

# Context
- Branch `phase2/terrain-drawn-bridge`, head 1216be35. It is already merged into portal; this is a new commit on top.
- crates/engine/src/shots.rs: `SettleCounts` (~378), `is_quiet` (~419), `settle_step` / `SettleStep` (~462-477), the settle in `run_shots` (~1167), `settle_counts` (~1416). Today `space_failed` is reported by `describe` but treated as terminal-quiet, so a shot of a failed cell settles and photographs.
- The acceptance path already fails on `failed_cells != 0`; the shot path should match that.
- Review findings: C:/Modding/Mudcrab/mudcrab/local/agents/runs/review-1111-p2-batch-terrain-roomcull-spans.glm.1/report.md and .muse.1/report.md (search "FAILED" / "space_failed").

# What to change
1. In the shot settle, the camera's own cell being failed is a terminal failure. `settle_step` returns a failure, and `run_shots` calls `run.fail(..)` with `counts.describe()`, writes no screenshot, and goes on to the next shot, the same shape as the timeout path. Other failed cells elsewhere in the window do not fail the shot: they are not what the shot photographs. Say if you think that is wrong.
2. Check that `--demo-tour` and `portal_bench`, which also call `settle_counts`, still behave sensibly. Change them only if they would now hang; say what they do.
3. Tests:
   - a shot whose own cell is failed fails the run, with no screenshot and the reason in the failure;
   - a failed neighbour cell does not fail the shot;
   - the existing timeout tests still pass.

# Scope
Owned files: crates/engine/src/shots.rs
- No git commands. No new dependencies.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast
- Set BUILD_GATE_OWNER=p2. A timing hold is active now: the gate waits, which is expected. Write the code meanwhile.
- `df -h /c` first; stop if C: has under 15 GB free.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
No silent fallback: a failed cell is a failure, not a photo. Nothing else is changed.

# Report
What changed (file:line), the test names, the commands and their Summary lines.
