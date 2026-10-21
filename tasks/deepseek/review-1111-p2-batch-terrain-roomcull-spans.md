# Goal
A batch review of Phase 2 items before they go into portal. One verdict section per item: accept or fix first, with `path:line - issue - fix`. Two reviewers run this same brief independently. Track: Phase 2.

# Items (review each with `git show <commit>` or `git diff <a>..<b>`; the checkout is C:/Modding/Mudcrab/mudcrab)
1. **#1055 terrain holes, C1+C5: the render-to-main "drawn" bridge and its gauges.**
   - Commits 0beedbc0 + ae91dc5d on `phase2/terrain-drawn-bridge`, base portal 8b8ce8c0.
   - New crates/engine/src/swap_bridge.rs.
   - Design: local/research/engine-foundations/design-1091-terrain-holes-prepared-bridge.md §2.
2. **C3: the old root stays as a stand-in across a detail change.** Commit 69fcef54 (design §3, the "Detail-change hand-off" paragraph).
3. **C4: settles wait on visible, not-yet-drawn surfaces, and a settle timeout fails the run.** Commit 5c150770 (design §4). Note: the coder found a gap in the design. A surface no view can see is never confirmed drawn, so every count uses `pending_drawn`: SwapWaiting and ViewVisibility true.
   - The combined branch head is 1216be35. Clippy is clean and engine nextest passes 1183/1183 there.
4. **C6.1: count full exterior cells answered without terrain, and validate them.** Commit 2287a1f3 on `phase2/terrain-less-cell-counter` (design §6 item 1).
5. **#188 room culling: an overlap edge is walked open when either room of the pair has no portal.** Commit 8ee1f983 on `phase2/room-culling`. Design: local/research/engine-foundations/design-1100-room-connection-rule.md §5. A real-data image check is still running; judge the code and tests.
6. **impl-1105: crossing apply brackets and real unload gauge counts.** Commit 05f77219 on `phase2/crossing-spans-followup`, base portal f6953289.

# Questions for every item
- Does it do what its design says, and nothing more? Is each test able to fail on a regression?
- **Foundations:** what shortcut does this take, and what would break for a mod or another worldspace? Anything hidden, delayed or silently passed? Any magic number, and is it justified in a comment?
- Cost per frame: is it O(pending), with nothing walking all terrain or all entities every frame in an ordinary run?

# Questions for specific items
- **1:**
  - Is writing to `MainWorld` from ExtractSchedule safe as done (cached SystemState, entity_mut after the query borrow ends)?
  - Can `SwapSurface.images` drift from what the material binds?
  - Is the pipeline check right: `SpecializedMaterialPipelineCache` for a view whose `RenderVisibleEntities` holds the entity?
  - Bevy is 0.19.0 in Cargo.lock; check the APIs against bevy_render-0.19.0 / bevy_pbr-0.19.0 in C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/.
- **3:**
  - Can a stand-in leak: never despawned, or counted as a resident root by the lifecycle check or the settle?
  - Is MAX_SWAP_HOLD_FRAMES justified?
  - What happens when the new root despawns first?
- **4:**
  - Does `pending_drawn` read the right frame's ViewVisibility?
  - Can any settle now hang forever? For example, a visible surface whose image never arrives: does the timeout fail loudly?
  - Is MAX_SWAP_WAIT_FRAMES = 512 sound?
  - The coder flags that a shot whose own cell FAILED still settles and photographs. Should it fail the shot? Give a recommendation.
  - Did the edits to demo_tour.rs and portal_bench.rs change behaviour beyond the new argument?
- **4 vs C1:** C1's own comment says a patch the LOD tile still hides is Hidden, so it is not counted. Confirm that against lod.rs's visibility pass.
- **5:** Can the rule ever cull something the game would draw? Does it keep narrowing where both rooms have portals? Are the four adapted test fixtures still meaningful?
- **6:** Do the brackets sit at sync points Bevy already inserts, so that a profiled run's schedule equals an ordinary one? Is there zero cost when profiling is off?

# Scope
Read only; git read commands only. You may run gated cargo in a fresh throwaway worktree only if you need to prove a finding: `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo ...` with BUILD_GATE_OWNER=p2 and your own CARGO_TARGET_DIR, never a shared one. Prefer reading.

# Done when
Every item has a verdict and its findings, each with a severity.

# Report
Status / Summary / Left / Next, then one section per item: the verdict, then findings.
