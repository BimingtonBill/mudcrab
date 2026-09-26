# Goal
Diagnose three Whiterun rendering bugs in this engine (`main` branch, a Rust/Bevy renderer of
converted Skyrim SE data) so fixes can be briefed. Read-only.

# Context
- Screenshots with pose/settings JSON and the user's notes:
  `C:/Modding/OpenSkyrimProject/local/captures/2026-09-26_11-44-01/` - `09.png` (Whiterun sewer
  streams missing: no water), `12.png` (Skyforge: a light-blue hole showing sky through the ground
  under the funeral pyre), `14.png` (water missing under the Dragonsreach bridge). `notes.md` has
  the user's words. These were taken on a fork build, but water and terrain code match `main` here.
- Converted database (read-only, sqlite): `C:/Modding/SkyrimConverted-upstream/skyrim_world.db`
  (tables: worldspaces, cells, references, statics, land, waters, ...). Schema:
  `docs/specs/converters/db-schema.md`. You may query it with python + sqlite3 in read-only mode
  (`sqlite3.connect('file:...?mode=ro', uri=True)`).
- Engine water: `crates/engine/src/streaming.rs` (per-cell water plane from the cell/worldspace
  water height) and `crates/engine/src/render.rs`. Whiterun's city worldspace is
  `WhiterunWorld` (form id 0x1A26F); it has a parent worldspace (Tamriel) and may inherit water
  and land from it.

# Questions
1. Water (09, 14): how does Skyrim place the water in the Whiterun sewer streams and under the
   Dragonsreach bridge - cell water height (XCLW) in WhiterunWorld cells, the worldspace default,
   inherited from the parent, or placed water objects (references to WATR/activator "water" meshes,
   e.g. `PlaceableWater`/`WaterPlane` statics)? Check the database for the cells at the poses in
   09.json / 14.json. Say which case it is and why this engine draws nothing.
2. The hole (12): at the pose in 12.json, is there LAND for that cell, is a quadrant hidden
   (LAND/VHGT flags, or cell "hide land" flags), or is the ground a placed mesh that failed to
   load? What should be there?
3. For each: the smallest change that fixes it, and whether it needs a reconversion.

# Scope
Read-only (python only for read-only database queries). Write nothing in the project.

# Report
Per bug: cause with evidence (query results, file:line), the fix, and confidence.
