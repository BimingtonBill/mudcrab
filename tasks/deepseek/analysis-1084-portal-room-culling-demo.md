# analysis-1084-portal-room-culling-demo (track: Portal; queue item B5). Read-only.

Repo C:/Modding/Mudcrab/mudcrab, branch portal (head 2c7a0ea0). Converted data C:/Modding/Mudcrab/SkyrimConverted-portal-rooms
(skyrim_world.db holds room tables: 4,246 rooms, 3,097 portals per Phase 1). Room culling is opt-in: `--room-culling`
(crates/engine/src/config.rs ~191/308/943, default false). Nothing in the demo seems to pass it.
Background: local/research/interior-room-culling-design.md, local/research/engine-foundations/websearch-1011-bevy-020-portal-culling.md;
grep docs/handoff/LOG.md and local/team/*.md for "room cull" and "18/20" (an image test where 18 of 20 poses matched).

Answer with file:line for every claim:
1. The runtime path: which systems read which DB tables, how culling is applied (VisibleEntities edits? which cameras: main,
   water reflection, portal camera?), what happens with the portal camera looking through a doorway into a culled room, and in
   exteriors. Any log line, diagnostic, profiling span or counter that proves at runtime that the tables loaded and N
   entities were culled. If none exists, say exactly where one line of info! logging would go.
2. Every launcher of the demo/tour/perf runs (scripts/*.ps1, tools/, config files, EngineConfig defaults, the portal tour,
   the re-profile scripts) and whether any passes --room-culling. Exact command line for a quick interactive check in
   Bleak Falls (an interior with rooms), using the quick profile binary target/quick/engine.exe if that's how others run it.
3. Known correctness issues: the 18/20 poses note (what failed in the 2 poses and why), missing objects at room edges or
   through open portals, rooms in mods/other worldspaces, missing tables in an older DB (does the engine fall back safely?).
4. Recommendation on how the demo should get it, judged by the AGENTS.md Foundations check: (a) pass the flag in the
   demo/tour/perf configs, or (b) auto-enable when the DB has the tables, with an off switch. Name what would have to change
   for each (files, lines) and the risks.
Report under 600 words, findings first.
