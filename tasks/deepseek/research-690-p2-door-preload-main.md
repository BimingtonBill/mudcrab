# Goal
Track: Phase 2. Map what upstream main has TODAY for load doors and interior cells, as input to
issue #112 (pre-load a door's destination cell while the player approaches; fade on crossing).

# Context
Checkout: `C:/Modding/wkj-door-preload` (branch phase2/door-preload = origin/main 461517bf). Read only.
Engine: `crates/engine/src` (streaming.rs, app.rs, config.rs, physics.rs, world/database.rs, world/*,
any doors/interaction code). Converter output schema for door links: `crates/shared/**` and the
world DB tables `door_links`, `references`, `cells` (database.rs readers).

# Questions (answer each with file:line)
1. Can a player on main go through a load door at all? Is there any activation (E key), any door
   record read (`door_links`, XTEL arrival), any cell switch from exterior to interior or back?
   If not, what is the nearest thing (e.g. `--interior`/start-cell flags, how an interior is loaded)?
2. How main streams cells: the request/commit path (worker thread, `DatabaseRequest`, cell cache),
   whether a cell can be loaded without being drawn (hidden/visibility), and the per-frame pacing
   caps (model spawns, upload budget `--max-upload-mib-per-frame`, unload pacing).
3. Is there any fade, loading screen, or transition UI on main? Any camera/player teleport API?
4. The player on main (physics.rs `WorldPlayerPlugin`, capsule): how it is placed, and what a
   teleport to an interior arrival point would need. Do not propose edits to physics.rs.
5. Anything already in main or open upstream that overlaps: grep for "door", "prestream",
   "preload", "transition", "fade". Note issue #117 (faster streaming stages) touches the load path.

# Scope
Read only; change nothing. Do not read `local/`.

# Report
Numbered answers 1-5, each with file:line evidence, under 120 lines. End with a list "missing on main
for #112" (one line each).
