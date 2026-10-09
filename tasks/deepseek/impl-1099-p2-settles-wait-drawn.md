# Goal
Slice C4 of the approved #1055 design (terrain holes): every settle waits on the same "drawn" signal, and a settle that times out is a hard failure, never a silent screenshot. Track: Phase 2, branch `phase2/terrain-drawn-bridge` (you start from its C1+C5 commit).

# Context
- Design (the contract): local/research/engine-foundations/design-1091-terrain-holes-prepared-bridge.md §4 (the settle), §7 (tests). The user approved it, including: **a settle timeout becomes a hard failure**.
- C1+C5 are already on the branch: crates/engine/src/swap_bridge.rs has `SwapWaiting`, `SwapSurface`, `SwapPrepared`, `SwapDrawn`, the `swap_bridge` extract system, `complete_drawn_swaps`, the `swap_readiness` decision function and the gauges. Read its module doc and tests first. Only terrain quadrant patches (`TerrainPatch`) carry `SwapWaiting` today.
- **One correction to the design, which you must build:** a surface no camera can see is never confirmed drawn (the bridge needs the entity in a view's `RenderVisibleEntities`). So a settle that waits on "all SwapWaiting" would hang on every patch behind the camera. The settle counts must count only surfaces that are **visible in the main world and not yet drawn**. Use Bevy's `ViewVisibility` (main world, computed in PostUpdate by `check_visibility`; bevy_camera in the 0.19 registry). Count `SwapWaiting` entities whose `ViewVisibility::get()` is true. Off-view surfaces are not a hole and do not hold a settle. Say in the report which frame's visibility the count reads, and whether that can miss a surface for one frame.
- **A finding to cover (Portal Dev, 2026-10-08):** `--shots` can time out while `cell_resident=false` and still write a blank PNG with no failure. Find that path in shots.rs and make it fail the run, as the design's timeout rule says.

# What to build
1. `SettleCounts` (shots.rs ~377-404): add `pending_drawn` (visible, waiting, not drawn), plus `pending_lod_queries` and `pending_lod_chunks` from `StreamingMetrics` (streaming.rs ~708-724). Add the failure-side LOD counters as the design says. `is_quiet` requires them at zero, and `describe` prints them. `settle_counts` (~1343-1359) fills them.
2. **Timeout = failure.**
   - In `--shots` (~1101-1162, `SETTLE_TIMEOUT_SECONDS` ~71, ~1131-1142), a shot that does not settle calls `run.fail(..)` with `counts.describe()` in the reason, shaped like the door timeout (~2003-2020). No screenshot is written for it.
   - Also the cell_resident=false path above.
   - The run exits non-zero.
3. **Acceptance** (app.rs `screenshot_assets_ready` ~2540-2610): add `pending_drawn == 0`.
4. **World-ready** (pacing.rs `WorldReadyInputs::is_ready` ~49-61, `track_world_ready` ~420-471): add `pending_drawn == 0` and the LOD pending counts.
5. **Door crossing:** `PendingUnder` (streaming.rs ~2907-2931) counts `SwapWaiting` under a root. Only visible ones count, by the same rule: a landing interior's surfaces are mostly in view once the camera is there. Say what you chose and why.
6. **Run-level invariant** (streaming.rs `validate_streaming_lifecycle`): for each resident full-detail exterior root in the stream window, a `TerrainPatch` that is visible and has been `SwapWaiting` for longer than a bound counts as a violation. Use a named, documented bound in frames or seconds, chosen from the bridge's 2-frame latency plus the upload budget's worst case; say how you derived it. Counted terrain-less cells (`TerrainLess`, from the sibling branch phase2/terrain-less-cell-counter, not on yours) are out of scope: skip that part, and note it for the merge.
7. **Tests** (design §7):
   - `a_settle_timeout_fails_the_shot`;
   - the blank-PNG path fails;
   - `world_ready_waits_for_drawn`;
   - `settle_counts` ignores an off-view waiting surface and counts a visible one;
   - `PendingUnder` counts a visible waiting surface;
   - the invariant flags a visible patch stuck past the bound and not one that is within it.
   Each test must fail if its line is removed.

# Scope
Owned files: crates/engine/src/shots.rs, crates/engine/src/pacing.rs, crates/engine/src/app.rs, crates/engine/src/door_crossing.rs, crates/engine/src/swap_bridge.rs (a small pub(crate) helper or query alias only), crates/engine/src/streaming.rs (`PendingUnder`, `validate_streaming_lifecycle` and tests only).
- Another coder works on streaming.rs `collect_cells` and the hand-over in swap_bridge.rs at the same time: do not touch `collect_cells`, `despawn_cells` or `complete_drawn_swaps`.
- No lod.rs. No new dependencies. No git commands.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast
- Set BUILD_GATE_OWNER=p2. Run `df -h /c` first; stop if C: has under 15 GB free.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
No frame-count settle, no silent timeout, no raised budget, nothing forced to prepare. Cost per frame scales with waiting surfaces only. Works for any worldspace.

# Report
What you changed (file:line), the test names, the commands and their Summary lines, the visibility-frame answer, the invariant bound and how you derived it.
