# Goal
Finish the B1 terrain-merge image diff (#186) that impl-1086 started and could not finish. Deliver the per-view table, the explanations and RESULTS.md, so the user can look at the merged terrain and Phase 1 can start its default-texture rebuild. Measurement and evidence only; no engine code changes. Track: Phase 2.

# Context
- The original brief, which is the contract for method and report: C:/Modding/Mudcrab/mudcrab/tasks/deepseek/impl-1086-p2-terrain-merge-image-diff.md. Read it first.
- What the first run already did (its partial log): C:/Users/Brannock/.claude-deepseek/agents/runs/impl-1086-p2-terrain-merge-image-diff.1/report.md.
  - The engine is built at D:/Mudcrab/builds/p2-terrain-target/quick/engine.exe, from the worktree C:/Modding/Mudcrab/worktrees/p2-terrain on branch phase2/terrain-merge.
  - Shot runs: D:/Mudcrab/evidence/p2-terrain-diff-2026-10-08/today-1..7 and merged-1..6.
  - Partial noise and cross diffs: comparison/ (cross, noise-today, noise-merged, pairs*.json).
  - Its scripts are in C:/Modding/Mudcrab/worktrees/p2-terrain/p2diff/ (diff_shots.py, cross_diff.py, pick_pairs3.py, reposition*.py, views and cells helpers). Reuse them.
- What it found:
  - the corner view had a pitch sign error, which it fixed (positive pitch looks down);
  - run-to-run noise was sky, clouds, LOD and waterfall strips. Riverwood-bridge on the merged side differs up to 4.6% between runs, all in sky and clouds;
  - views 3 (Whiterun tundra) and 5 (four-cell corner) landed on the city wall, rooftops, rocks or water, so they miss the brief's intent.
- The known capture problem (#1055: a shot can be taken before terrain draws) is not fixed on this branch. Keep the "two matching shots per side" rule. For the noise test, mask the sky: count only pixels below the horizon, or the bottom 60% of the frame for downward views. Say which.

# Steps
1. Keep views 1 (riverwood-bridge), 2 (riverwood-high) and 4 (mountain blend) if their pairs pass. Use the existing shots wherever a clean pair exists; do not reshoot what is already good.
2. Re-pick views 3 and 5 from data, not by trial:
   - view 3: an exterior cell near Whiterun (Tamriel 60) whose LAND has at least 3 texture layers blending, not under the city or the walls;
   - view 5: a four-cell corner over open land (no water, no references above 200 units in view if you can check).
   Before the comparison runs, shoot each candidate once on the today side, and accept it only if ground fills at least 70% of the frame.
3. Shoot the new views twice per side, then diff as in the original brief:
   - noise per side, sky masked;
   - today vs merged;
   - heat maps and side-by-side PNGs.
4. Before every engine run, `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`. If any hold is listed, wait and recheck every few minutes. Never run the engine during a hold.
5. Write RESULTS.md and copy the key images to D:/Mudcrab/evidence/p2-terrain-diff-2026-10-08/.

# Scope
No engine code changes and no git commands. Write only in the evidence folder and p2diff/.

# Done when
All 5 views have a clean pair per side or a stated reason why not, a today-vs-merged number, and an explanation in words. RESULTS.md is written.

# Report
Status / Summary / Left / Next, then the per-view table (noise today, noise merged, today vs merged, verdict), the explanations, the PNG paths, and any suspected engine or converter bug with evidence.
