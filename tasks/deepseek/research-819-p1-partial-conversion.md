# Goal
Track: Phase 1. Design a partial conversion so a new contributor can get a playable area quickly:
convert only what one worldspace area (for example Riverwood and Whiterun, or a list of cells, or
an interior) needs, instead of the whole game. Read-only research; produce a design and a step plan
a coder can follow.

# Context
- Read `tasks/deepseek/context.md` first.
- The converter is `crates/converter` (upstream main; this checkout's copy is a fork but the
  converter pipeline, `esm/` exporter, `integration.rs` asset closure and `cache.rs` manifest are
  close to upstream). The engine is `crates/engine` (streaming reads `skyrim_world.db` and the cell
  cache and opens GLB/KTX2 by path; find what it requires to exist and what it tolerates missing).
- Measured costs of a full run: textures ~4.5 h of a ~5 h fresh conversion; extraction ~14 min on a
  reconversion; meshes ~4 min.

# Questions
1. What does the engine need to show one area: which DB tables/rows, cell cache entries, GLBs,
   textures (via each GLB's texture references), scripts? What does it do when an asset referenced
   by a loaded cell is missing (error, placeholder, skip)? Cite `path:line`.
2. How to compute the closure for a set of cells: references in those cells (and persistent refs,
   LOD, doors to interiors), their base records' models, each model's textures (the GLB/NIF texture
   paths), skeletons and dependencies. Can this be computed before converting (from the ESM and the
   NIFs) so only those meshes and textures are converted?
3. The database and cell cache: keep them whole (cheap, minutes) or filter? What breaks if they list
   cells whose assets were not converted?
4. Interaction with the manifest and a later full conversion: a partial output must be marked as
   such (not `complete`), and a later full run must reuse everything already converted.
5. The CLI: `--only-cells`, `--only-worldspace <edid> --grid x,y,radius`, `--only-interior <edid>`?
   Propose one, with examples for Riverwood/Whiterun (find their worldspace EDIDs and grid
   coordinates in the code, docs or tests; do not read game data).
6. Estimated time for a Riverwood+Whiterun partial run and its output size.
7. A step plan in 2-4 coder tasks with owned files and tests.

# Scope and rules
Read-only. Write only `local/research/phase1-partial-conversion.md` (under 1300 words). No cargo
builds, no engine runs, no game data. Do not commit. Then a summary under 300 words as your final
message.
