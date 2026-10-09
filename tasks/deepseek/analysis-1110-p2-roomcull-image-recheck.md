# Goal
Re-run the #188 room-culling image check with impl-1104's fix (design-1100: an overlap edge is walked open when either room has no portal). Required result: 0 differing pixels on all 25 poses plus the 3 floor poses, and the culling power before and after. Track: Phase 2 (queue B5).

# Context
- Code: worktree D:/Mudcrab/worktrees/p2-roomcull-check-1104, commit 6a54fb0a. That is Portal's impl-1085 commit 16fad023 (room culling default on, `--room-culling=off` flag, reads portal-schema assets) plus the merged phase2/room-culling 8ee1f983 (the fix).
- The previous check, which is the method to repeat exactly: C:/Modding/Mudcrab/mudcrab/local/agents/runs/analysis-1092-portal-roomcull-image-check.2/report.md.
  - Scripts: C:/Modding/Mudcrab/mudcrab/local/impl/impl-1085-portal-room-culling-default/target/roomcull/*.py (run_mode2.py, compare_runs.py, floor_check.py, verify_pose.py). Copy them to your output dir and adjust only the engine path and output paths.
  - Poses: D:/Mudcrab/scratch/portal-roomcull-image/poses.json and poses_floor.json.
  - Previous results: D:/Mudcrab/scratch/portal-roomcull-image/summary.md.
- Assets: C:/Modding/Mudcrab/SkyrimConverted-portal-rooms.

# Steps
1. `df -h /d`; stop if D: has under 15 GB free. Build once:
   `BUILD_GATE_OWNER=p2 CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-roomcull-check-1104 python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo build --release --bin engine`
   It's a cold build, so the gate may make it wait; that is expected.
2. Before EVERY engine run, `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`. If any hold is listed, wait and recheck every few minutes. Never run the engine during a hold.
3. Run 3x off and 3x on over the 25 poses, plus the floor poses off and on, with the same rule as before: a pixel counts only when every on run differs from every off run by more than 8 on some channel. Output to D:/Mudcrab/scratch/p2-roomcull-1104/ (OWNER.txt: owner Phase 2 Dev, class A, keep-until date:2026-10-23, links no).
4. Culling power: from the engine logs or `culled_last_frame`, rooms not reached and meshes culled per pose, on vs before (the previous runs' logs in D:/Mudcrab/scratch/portal-roomcull-image/logs).
5. Look at the BleakFallsBarrow01-room1-c on/off pair yourself and say what it shows.

# Scope
No code changes. No git commands except read-only ones. Write only under D:/Mudcrab/scratch/p2-roomcull-1104/ and the build folder.

# Report
Status / Summary / Left / Next, then the per-pose table (as summary.md), culling before and after, and the BleakFallsBarrow pair.
