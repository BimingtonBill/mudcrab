# Goal
#188 room culling hides 4-13% of the frame in one real Skyrim cave hall. Find the correct rule for how rooms connect when no portal joins them, and write a design the coder can build. Not a bigger magic tolerance. Design first in this job: read code and data, change no code. Track: Phase 2 (queue row B5).

# Context
- Branch `phase2/room-culling` (head 4bce5914), worktree D:/Mudcrab/worktrees/p2-roomcull. Engine: crates/engine/src/room_culling.rs. Read the module doc (lines 1-120): today the graph has three edge kinds:
  - portals (`PortalMarker` pairs), which narrow the region;
  - `XLRM` links (`room_links`), which are open;
  - implicit overlap edges (`overlap_links`), added between rooms whose oriented boxes overlap or lie within `OVERLAP_TOLERANCE` = 8 units, which act as a thick portal.
  Converter: crates/converter/src/esm/rooms.rs (room_links from XLRM, ~414).
- The failure (Portal Dev's image check, 25 poses on portal-rooms data, only one fails):
  - pose BleakFallsBarrow01-room1-c, cell 225758;
  - room 234060 is a 6,976-unit hall with no portal and one room_links edge;
  - the neighbour whose geometry vanishes lies 512 units past its box, beyond the 8-unit tolerance, so the walk stops;
  - 127,216 px differ (8.7%), deterministic.
  - Evidence: D:/Mudcrab/scratch/portal-roomcull-image/ (summary.md, poses.json, diff/BleakFallsBarrow01-room1-c-marked.png, runs/).
- Data: C:/Modding/Mudcrab/SkyrimConverted-portal-rooms/skyrim_world.db (read only, sqlite3 or python -I). Tables include rooms, room portals, room_links and room membership; check the exact names in the converter.
- Prior research: local/research/interior-room-culling-design.md (research-584), local/research/engine-foundations/websearch-1011-bevy-020-portal-culling.md, local/research/reference/skyrim-re-digest.md (clean-room rules: read and cite, never copy code).
- xEdit 4.1.5f is installed at C:/Tools/SSEEdit. Use the dumper only, never the GUI: `SSEDump64.exe "-d:<Data path>\" ...`, output to D:/Mudcrab/scratch/ssedit-dumps. For record definitions, cite `Core/wbDefinitionsTES5.pas` at tag `xedit-4.1.5f` (web lookup allowed for that file only).

# Questions (answer with evidence: file:line, SQL output, record dumps)
1. **The failing hall.** List its room markers and their boxes, portals, XLRM links (both directions) and membership counts. Which room does the vanished geometry belong to? Where is the camera? Is the vanished geometry in a room at all, in no room, or in a room the walk cannot reach? Draw the graph for that cell (rooms, edges, distances between boxes).
2. **What the game does.** From the record definitions and the RE digest: what XLRM ("linked rooms") means in the Creation Engine; whether a room with no portals is treated specially; what happens to geometry outside every room box, or in a room the camera's room cannot reach; and whether the game uses the occlusion planes or the room bounds for this. Separate what is cited from what is inferred.
3. **How common it is.** Across all interior cells in the DB: how many room pairs lie closer than, say, 64 / 256 / 1024 units with no portal and no link? How many rooms have no portal at all? Which cells look like the failing one? Give counts.
4. **Candidate rules, compared on that data:**
   - (a) XLRM transitive or treated differently;
   - (b) a gap-closing rule derived from data (for example, the gap between two boxes is joined when membership geometry spans it);
   - (c) treating a room with no portals as open to every room it links or overlaps;
   - (d) anything the game evidence supports.
   For each, say how many cells change and whether it would still cull the cells that pass today (the 24 passing poses).
5. **The recommendation.** One rule, the code change in room_culling.rs (and converter, if it needs a new precomputed field), and the tests:
   - the failing cell's shape as a fixture;
   - the existing passing cases unchanged;
   - an image re-check of the 25 poses with Portal Dev's harness, the test/room-culling-image branch at D:/Mudcrab/worktrees/p2-roomcull-image (say the command, don't run it).

# Scope
Read only; no cargo, no engine runs. Write the design to local/research/engine-foundations/design-1100-room-connection-rule.md and add a row to local/research/INDEX.md.

# Foundations check (answer explicitly)
The rule comes from data, not a tolerance tuned to one cave. It holds for any mod's interiors and load order, and it never culls something the game would draw. What shortcut would a quick fix take here (raise the tolerance, exempt the cell), and why is it rejected?

# Report
Status / Summary / Left / Next, then the design path and a 5-line summary.
