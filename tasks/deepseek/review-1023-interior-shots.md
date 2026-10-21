# Goal
Review a local branch for Mudcrab (Rust/Bevy 0.19): `phase2/interior-shots`, which makes `--shots` render poses inside interior cells. It is stacked on open PR #128 (door crossing and ActiveSpace, head 386267dc). Decide whether it is correct, small and safe, and find what used to work and now fails.

# Context
- Your working folder is the review folder: `delta.diff` (386267dc to b9cec47b, see `commits.txt`), and `before/` and `after/` copies of every touched file. Read code only from there.
- What the coder says:
  - **Factored out of door_crossing.rs, with no behaviour change:**
    - `switch_active_space(world, space, origin, camera_space)`, the unload/ActiveSpace/render-origin/CameraSpace part of `switch_space`;
    - `cell_progress(...) -> CellProgress`, the crossing's inline landing-readiness check. The crossing now calls both.
  - **Interior shots:**
    - An interior shot queues one command that switches the space and sets the camera translation. Interior roots sit at (0,0,0), with a zero render origin.
    - Readiness: the cell is resident with pending == 0 and assets_waiting == 0 under its root (`SettleCounts.interior_cell_pending`), plus the existing 10-quiet-frame rule and 30 s timeout.
    - A failed or unknown interior is skipped and the run exits non-zero.
    - A first interior shot sets ActiveSpace at plugin build, so no exterior is ever planned.
    - shots.log gains `space=`.
  - **streaming.rs** gained only `#[cfg(test)]` helpers and a test that unloads while a load is in flight.
  - **Real run:** BleakFallsBarrow01 rendered correctly twice, with an exterior shot between them.

# What to check
1. **The door crossing is unchanged.** Compare `before/after door_crossing.rs` line by line: the same order of operations, the same readiness rule, the same fade-in timing.
2. **Exterior-only shots files behave exactly as before:** the same poses, settle rules, log fields (except the added `space=`), and exit codes.
3. **Interior shots:**
   - the space switch happens before the planner runs that frame;
   - no exterior cell is planned while an interior is active;
   - the switch back to exterior restores the render origin;
   - no stale camera pose;
   - readiness cannot pass before the interior has been requested.
4. **Failure paths:** unknown or failed interior cells, and timeouts. No hangs. Is the non-zero exit right?
5. **Docs** (shots.rs module doc, docs/specs/engine/reference-shots.md) match the code.
6. **Tests:** they exercise the real systems, and no test was weakened.

# Scope
Read only. Change nothing.

# Report
- A verdict line: ready / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - fix`. At most 8.
- What you verified as correct.
- What you could not check.
- Under 600 words.
