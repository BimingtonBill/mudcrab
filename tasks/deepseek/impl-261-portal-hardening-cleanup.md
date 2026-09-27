# impl-261 - Door layer and streaming hardening, stale comments

Owned files: crates/engine/src/door_layer.rs,crates/engine/src/streaming.rs,crates/engine/src/app.rs,crates/engine/src/shaders/snow.wgsl

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user approved this batch on 2026-09-27. Sources: the audit findings in `local/agents/memory/findings.jsonl` (search each id below; each entry has the evidence).

## Goal
1. **DCP-20260927-01 (door_layer.rs, `manage_door_layer`):** the one-owner-per-doorway guard uses `drivers`, a snapshot of the previous frame's pairs taken before `std::mem::take`, and pairs spawned this frame are not in it. Make the guard see every pair that exists when it decides (the kept pairs so far plus pairs spawned this frame), on both the keep path and the spawn path, so no door can be the driver of one pair and the follower of another in the same frame. Add a test with two linked doors both in the player's space.
2. **ES-20260927-02 (streaming.rs):** the failed-Full rescue (`full_failed`, from ES-20260926-01) covers exterior cells only. Decide from the code whether an interior cell whose Full request fails while the camera is inside it can end up undrawn with no retry. If it can, extend the rescue or retry rule so it can't, without a retry loop. If it can't happen for interiors (they have no terrain tier), say why in a comment at the `CellKey::Exterior` check and in your report. Test whichever applies.
3. **ES-20260927-04 (app.rs ~265):** the comment "Nothing reads it yet" on the PortalGraph plugin is false (transition.rs ~514 and ~587 rank doors with `predicted_doors` over `PortalGraph`). Correct it.
4. **RM-20260926-01 (snow.wgsl ~68-72):** the comment names a function that no longer exists and says the vertex-alpha multiply is identity, which is not true since vertex-alpha-faithful. Correct the comment only; no shader behaviour change.

Not in scope: CC-20260927-01 (`RankedFrom` in transition.rs; another worker owns that file today).

## Checks
- `cargo test -p engine --lib` in your worktree; check that your new tests appear in the run.
- The short walked tour passes: `<exe> --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour <dir> --tour-doors 2`.

## Build and run rules
- Build: `cargo build --profile quick --bin engine` in your own worktree with its own target, `CARGO_BUILD_JOBS=6`; check `quick/engine.d` names your worktree.
- No timing runs. If the lead messages HOLD, finish the command in hand and start no cargo or engine command until told to resume.

## Commits
Do not commit; the lead integrates. Handoff note in `docs/handoff/impl-261.md`.

## Report
For each item: what you changed (file:line) or why nothing was needed, test names, the tour's last line.
