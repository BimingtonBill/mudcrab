# Goal
Slice C3 (first part) of the approved #1055 design (terrain holes): when a cell changes detail on the same grid (the terrain ring upgrading from `CellDetail::Terrain` to `Full`, or downgrading back), the old root stays drawn as a stand-in until the new root's terrain is confirmed drawn. Today the old root is despawned in the same frame, which leaves a hole for the whole texture-load window. Track: Phase 2, branch `phase2/terrain-drawn-bridge` (you start from its C1+C5 commit).

# Context
- Design (the contract): local/research/engine-foundations/design-1091-terrain-holes-prepared-bridge.md §3, the "Detail-change hand-off" paragraph, plus §7 test `a_detail_change_keeps_the_old_root`.
- C1+C5 are already on the branch: crates/engine/src/swap_bridge.rs has `SwapWaiting`, `SwapSurface` (with `stand_in: Option<Entity>`, always None so far), `SwapPrepared { stand_in, since }`, `SwapDrawn`, and `complete_drawn_swaps` (removes `SwapWaiting` when `SwapDrawn` appears). Read its module doc and tests first.
- streaming.rs `collect_cells`: the `replaced` root (~1953-1958; search `replaced`) is despawned when the new detail commits. `cells_upgraded` and `cells_downgraded` count these (~651-652).
- **The visibility rule:** a surface no view can see is never confirmed drawn. If the new root's patches are all off-view, the hand-over must still finish. In that case nothing on screen needs covering, so off-view should count as done once the surface is `SwapPrepared` (its assets are on the GPU). Use the main world's `ViewVisibility` to tell them apart. A stand-in is released when every one of the new root's waiting surfaces is either drawn, or prepared and not visible.

# What to build
1. In `collect_cells`, do not despawn the `replaced` root. Keep it, record it as the new root's stand-in (on the new root, or on its patches' `SwapSurface.stand_in`; choose one and say why), and keep it drawn.
2. A hand-over in swap_bridge.rs: when the release condition above holds for the new root, despawn the stand-in root with the same despawn path `collect_cells` used (keep its counters and pacing behaviour).
3. **A bound, loud and never silent:** if the condition does not hold within `MAX_SWAP_HOLD_FRAMES`, a named constant (the design suggests about 120; say how you chose it), despawn the stand-in anyway and count `swap_hold_timeouts` in `StreamingMetrics` and the profiler.
4. **Edge cases (one sentence each in the report):**
   - the new root despawns (camera moved away) while holding a stand-in;
   - the stand-in's cell is requested again;
   - the new root's cell has no terrain, so it has no waiting surfaces and releases at once;
   - a terrain-only ring cell replaced by full detail and the reverse.
   The stand-in must never be counted as a resident root in the lifecycle check or the `--shots` settle. Say how you ensure that.
5. **Tests:**
   - `a_detail_change_keeps_the_old_root`: the old root stays while the new root is waiting;
   - the old root goes the frame after the new patches get `SwapDrawn` inserted by hand;
   - off-view prepared releases;
   - the hold timeout despawns and counts;
   - the new root despawning also releases the stand-in.

# Scope
Owned files: crates/engine/src/streaming.rs (`collect_cells`, `StreamingMetrics`, the despawn path, and tests), crates/engine/src/swap_bridge.rs (the hand-over), crates/engine/src/profiling.rs (a counter column only if siblings have one).
- Another coder works on shots.rs, pacing.rs, app.rs, door_crossing.rs and streaming.rs `PendingUnder` and `validate_streaming_lifecycle` at the same time. Do not touch those.
- No lod.rs. The unload-side hold, where full detail stays until its LOD tile is drawn, needs LOD patches to carry the marker. That is slice C2 (lod.rs), so it is NOT in this job.
- No new dependencies. No git commands.

# Done when
Acceptance: python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets -- -D warnings, python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets --no-fail-fast
- Set BUILD_GATE_OWNER=p2. Run `df -h /c` first; stop if C: has under 15 GB free.
- `cargo fmt --all -- --check` is clean.

# Foundations check (answer in the report)
Nothing is hidden or delayed: the old detail covers until the new one draws, and the bound is loud. Cost is per held root. Works for any worldspace, with or without LOD.

# Report
What you changed (file:line), the test names, the commands and their Summary lines, the edge cases, and what only a GPU run can confirm.
