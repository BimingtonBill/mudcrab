# Goal
#188 room culling still hides geometry at two player-height poses in BleakFallsBarrow01, after impl-1104's fix. Find why, and propose the rule change. Read only. Track: Phase 2 (queue B5).

# Context
- The fix (commit 8ee1f983, phase2/room-culling): an overlap edge is walked open when either room of the pair has no portal (design: local/research/engine-foundations/design-1100-room-connection-rule.md; read §1, §4, §5).
- The re-check (D:/Mudcrab/scratch/p2-roomcull-1104/summary.md, diff/, runs/, logs/; report at D:/Mudcrab/worktrees/p2-roomcull-check-1104/local/agents/runs/analysis-1106-p2-roomcull-image-recheck.1/report.md):
  - 24 of 25 poses are 0 px;
  - BleakFallsBarrow01-room1-c is 621 px, max delta 12, box [909,433,973,448];
  - floor-mid is 334 px (max delta 17);
  - **floor-endA is 189,384 px and floor-endB 89,957 px, max delta 177, identical before and after the fix.**
  Poses: D:/Mudcrab/scratch/portal-roomcull-image/poses_floor.json (endA at z=700, endB similar; all in cell 225758 = 0x371DE).
- design-1100's offline walk mirror and cell report: C:/Modding/Mudcrab/mudcrab/local/tmp-1100/ (cell-report.txt, scripts). Reuse it, updated to the new rule.
- Code: D:/Mudcrab/worktrees/p2-roomcull-check-1104/crates/engine/src/room_culling.rs (camera room selection, `reached_rooms`, what happens when the camera is in several boxes or in none).
- Data: C:/Modding/Mudcrab/SkyrimConverted-portal-rooms/skyrim_world.db (read only).

# Questions
1. For endA and endB, which room box or boxes hold the camera, and which room the walk starts from. Is the camera in no box, or in overlapping boxes where the chosen start room is wrong? Which rooms own the hidden pixels (the same mask method as design-1100 §1)? Why does the fix not reach them?
2. For the 621 px at room1-c and the 334 px at floor-mid: is it real culling (a room still unreached) or noise (shadow, LOD or temporal effects at max delta 12-17)? Check the off-off noise of those poses, and which room owns the box.
3. Propose the rule change (start-room selection, a camera in several boxes, a camera outside all boxes), with evidence from the data across all interior cells: how often the camera-position case arises and what it would cost in culling. Same standard as design-1100: data-driven, never culls what the game draws, no tolerance tuning.

# Scope
Read only. No cargo, no engine runs. `python -I` for scripts. Write the note to local/research/engine-foundations/analysis-1112-roomcull-floor-residual.md and add an INDEX row.

# Done when
Each question is answered with evidence (file:line, SQL output, mask numbers), and one recommended rule change is given with the code location to change.

# Report
Status / Summary / Left / Next, then the answers and the recommendation.
