# Goal
Review impl-1060 (re-scoped): it paces streaming-issued asset loads in flight, counted per issued load. Decide accept or fix first. Track: Phase 2.

# Context
- Worktree: C:/Modding/Mudcrab/mudcrab/local/impl/impl-1060-p2-load-pacing (detached, base portal bbfc2957). The change is uncommitted: review `git diff` for crates/engine/src/config.rs and crates/engine/src/streaming.rs. Ignore .deepseek-agents.json, which is harness config.
- The contract: C:/Modding/Mudcrab/mudcrab/tasks/deepseek/impl-1060-p2-load-pacing.md. Design review it answers: C:/Modding/Mudcrab/mudcrab/local/agents/runs/review-1062-p2-design-1060.glm.1/report.md.
- Known caveat: which work owns the PreUpdate worst (bevy_asset load-finalize, or the handle-drop cascade after a crossing's despawn) is unproven. impl-1077's spans settle it in the timing window. Don't judge the speed claim, only the code.
- The lead's checks: clippy -D warnings clean; the 6 pacer tests pass 5 runs in a row. The lead made three edits after the coder: registered `WorldAsset` in the uncapped test, and changed two frame-count waits to a 30 s wall-clock deadline. Check that those don't weaken anything.

# Scope
Read only; git read commands only; no cargo.

# Done when
You have checked, with file:line:
1. Cap 0 is EXACTLY today's behaviour: loads are issued in `spawn_cell`, nothing is queued, and there is no extra per-frame cost beyond a cheap early return.
2. Counting is per issued load. In-flight means issued and not finished or failed (`model_can_spawn`). The running count can't drift: check despawn mid-flight, failure, a cell re-request, a tier change (full or terrain), and a model handle shared by two references.
3. Readiness counts queued loads as not ready everywhere it matters: `pending_asset_instances`, the `--shots` settle, the prestream wait and `PortalPrewarm`, and door destination readiness (`destination_is_loaded`). A crossing can never proceed on a half-loaded interior.
4. The issue order is stable (cell request order, then sequence), so the prestream destination isn't starved by roaming cells. Does the destination behind a door get priority? If not, say whether it should.
5. No starvation: progress every frame while below the cap. A despawned cell drops its queue cleanly.
6. Which loads count (models; per-cell textures such as the water flow normal and terrain quadrant textures), and whether shared or cached handles are excluded correctly.
7. The tests would fail if the behaviour broke, and the lead's deadline change kept every assertion.

# Foundations check (answer explicitly)
What shortcut does this take, and what would break for a mod (very model-heavy cells) or another worldspace? Is anything hard-coded? Is it labelled plainly as pacing, with the default 0 until the A/B?

# Report
Status / Summary / Left / Next. Then the verdict (accept / fix first / reject), findings as `path:line - issue - fix` ordered by severity, and what you could not verify.
