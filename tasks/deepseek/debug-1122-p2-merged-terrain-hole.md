# Goal
In the B1 terrain-merge image diff, the merged path is MISSING distant ground in one view (blue void and a translucent water patch, where today's path draws the river and banks). Find out whether that is a capture-timing hole (a shot taken before the far chunks drew) or a real merged-terrain bug, and if real, why. Track: Phase 2 (queue B1). Team Lead's scope: keep it small.

# Context
- Evidence: D:/Mudcrab/evidence/p2-terrain-diff-2026-10-08/.
  - RESULTS.md (read the correction note at the top: chunk files are 4x4 cells, so all of Tamriel is covered);
  - key-images/whiterun-fourcorner.today-vs-merged.side.png (the hole) and key-images/riverwood-bridge.today-vs-merged.side.png (a distant snow slope that differs);
  - views.json and the latest view definitions (whiterun-fourcorner: position 28672, -16384, -4112, yaw 0, pitch 45).
  - The merged side's own run-to-run noise on that view was 13.9%.
- Scripts from the earlier runs: C:/Modding/Mudcrab/worktrees/p2-terrain/p2diff/ (diff_shots.py and others). Reuse them.
- Data:
  - merged: D:/Mudcrab/SkyrimConverted-p2main-v24-terrain;
  - today: C:/Modding/Mudcrab/SkyrimConverted-p2main-v24.
- Your worktree: D:/Mudcrab/worktrees/p2-terrain-c4check, branch p2/terrain-c4check, which equals phase2/terrain-merge. You may make ONE throwaway change for this debug only, never to be merged: raise `SETTLE_QUIET_FRAMES` in crates/engine/src/shots.rs from 10 to 300 (about 5 s of quiet frames) so every shot waits much longer. Commit nothing.
- Engine code for the merged path: crates/engine/src/terrain_merge/ (mod.rs: chunk store, `TerrainMerge::open`), streaming.rs `choose_merged_terrain`; shared crates/shared/src/terrain_mesh.rs (`terrain_chunk_for_cell`, 4x4 cells per chunk).

# Steps
1. Build: `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --profile quick --bin engine` with BUILD_GATE_OWNER=p2 and CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-terrain-c4check. `df -h /d` first; stop if D: has under 15 GB free.
2. Before EVERY engine run, `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`. If any hold is listed, wait and recheck every few minutes. Never run the engine during a hold.
3. **Step 1: reshoot.**
   - Shoot whiterun-fourcorner and riverwood-bridge on the merged side 3 times, and on the today side twice.
   - Output to D:/Mudcrab/evidence/p2-terrain-diff-2026-10-08/debug-1122/ (create it).
   - Diff merged against merged (the noise), and merged against today.
   - **If the hole is gone in all 3 merged shots and they match each other, it was capture timing. Say so, show the images, and stop.**
4. **Step 2, only if the hole is still there:**
   - list the exterior cells (worldspace 60) in that view that show no merged ground;
   - for each, check whether its chunk file exists and holds that cell (`terrain_chunk_for_cell`), and whether the engine took the merged path or a fallback for it (log lines, or a short read of the code path);
   - check whether the ring/LOD handoff hides a cell before its merged mesh draws.
   Say which it is, with file:line evidence.

# Scope
The one throwaway constant change only; no other code changes; no git commits. Write only under the debug-1122 folder and p2diff/.

# Done when
Step 1's answer (timing or not) is given with images and numbers, and if not timing, Step 2's cause with evidence.

# Report
Status / Summary / Left / Next, the per-shot table, the image paths, and the cause.

# Leads from Phase 1 Dev (data owner), for Step 2 only
- The merged DATA has no hole for whiterun-fourcorner: every cell in x -9..23, y -8..20 (957 cells) has LAND and a non-empty chunk file (52-336 KB); the conversion wrote 52,181 cells, 0 skipped. Script: C:/Modding/Mudcrab/mudcrab/local/research/terrain-merge/view_cone_check.py. So look at the ENGINE side.
- Lead 1, LOD handoff: does the merged path hide or skip LOD terrain for an area it claims, while loading merged chunks only near the camera? Then the distant ring has neither and the clear colour shows. Check what draws beyond the merged load radius.
- Lead 2, water: 78 cells in that box have their own water height (XCLW), e.g. (2,0) -5400, (2,1) -8050, (-4,-1) -7049, (-9..-5, 0..3) -7700. Merged chunks hold terrain only. If today's path spawns water per cell (water_height) and the merged path skips it, rivers vanish. Compare one river cell's entities with the merged path off and on.
