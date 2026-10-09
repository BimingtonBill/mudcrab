# Goal
Review one commit before it merges into portal: #188 room culling, "draw the whole cell from a room with no portal". Decide accept or fix first. Track: Phase 2 (queue B5).

# Context
- Commit 45eb7833 on `phase2/room-culling` (checkout C:/Modding/Mudcrab/mudcrab; `git show 45eb7833`). It sits on 8ee1f983, which review-1111 already accepted.
- Why it exists: local/agents/runs/analysis-1112-p2-roomcull-floor-residual.1/report.md (C:/Users/Brannock/.claude-deepseek/agents/runs/ if not there) and local/research/engine-foundations/analysis-1112-roomcull-floor-residual.md. The earlier rule: local/research/engine-foundations/design-1100-room-connection-rule.md §5.
- Real-data result with both commits: D:/Mudcrab/scratch/p2-roomcull-1104/summary.md. All 25 poses and all 3 floor poses are 0 differing pixels.

# Questions
1. The rule: `reached_rooms` returns `None` (draw everything) if any start room has no `PortalMarker` of the cell. Is it correct and minimal? Can it ever cull something the game draws? (It can only add.)
2. The tests: five fixtures got a parked portal (`park_portals`) so they still test their property, and two were renamed. Is any assertion weakened, or any property no longer tested? In particular, is the open-overlap rule of 8ee1f983 still pinned by a test where the walk actually runs?
3. Cost: how much culling does it give up? analysis-1112 estimates 9.5% of culled rooms. Is a portal-less start common in vanilla interiors? Say if the estimate looks wrong.
4. Foundations: what shortcut does it take, and what would break for a mod or another load order?

# Scope
Read only; git read commands only; no cargo.

# Done when
A verdict and findings (`path:line - issue - fix`), each with a severity.

# Report
Status / Summary / Left / Next, then the verdict and the findings.
