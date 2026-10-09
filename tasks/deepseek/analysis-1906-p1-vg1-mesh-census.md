# Goal
VG1 mesh census of the converted game data. It fills the open unknowns of the virtualized-geometry plan (`local/research/fun/team-lead-queue-proposal-vg-2026-10-09.md`, section 1): which meshes a cluster path would cover, and how many triangles and draws they carry. Phase 1 track. Read only: no cargo, no engine, no writes outside your output folder.

# Scope
- Data: `C:/Modding/Mudcrab/SkyrimConverted-portal-rooms` (read only). `meshes/**/*.glb` are the converted models. `skyrim_world.db` (SQLite; open read-only with `?mode=ro` URI) has `references` (placed objects), `statics`, `cells`, `worldspaces` and `lod*` tables. Read the column names with `PRAGMA table_info` first.
- Output folder: `D:/Mudcrab/scratch/p1-vg1-census/` (create it, and put an OWNER.txt in it: owner Phase 1 Dev, purpose VG1 census, class A, keep-until date:2026-10-23, links no). Write the script(s) and results there. Python standard library only; GLB is JSON chunk plus BIN chunk, so read accessor counts from the JSON (no need to decode vertex data).
- Before writing anything: `rg -i "census|triangle|draw call|mesh count" C:/Modding/Mudcrab/mudcrab/local/research/INDEX.md` and reuse what exists.

# Questions to answer
1. Unique meshes: count of GLB files; total and distribution (median, p90, p99, max) of triangles per mesh (sum of index counts / 3 over all primitives; non-indexed primitives use POSITION count / 3).
2. Category shares, by mesh count AND by triangles: opaque, alpha-tested foliage or cutout (material alphaMode MASK), blended (alphaMode BLEND), skinned (a node with `skin`, or JOINTS_0 present), and any other you find (for example decals, effect, or water). Say exactly how each is detected and which GLB extras/material fields the converter writes (look at a few files first).
3. Placements: from `references`, how many placed instances per worldspace (Tamriel 0x3C and the five largest others) and for interiors; distinct base meshes placed; instances per mesh (median, p90, max); and triangles placed (instances x triangles of their mesh), split by category. Map a reference to its mesh through the tables the DB has; say the join you used.
4. Draws per category: primitives per mesh (each glTF primitive is one draw without batching) times placements, per category. Also per exterior cell: median, p90 and max draws and triangles for Tamriel cells.
5. Which meshes dominate: the 30 meshes with the most placed triangles, with category and placements.
6. Anything that blocks a cluster path: meshes over 1M triangles, non-indexed primitives, meshes with more than N primitives (say N), or meshes whose material set changes per placement.

# Done when
A report file `D:/Mudcrab/scratch/p1-vg1-census/census-2026-10-09.md` with tables for questions 1-6, the scripts beside it, and run time stated. Facts only from the data; say clearly where a number is an estimate or a detection rule could misclassify.

# Report (under 400 words)
Headline numbers (unique meshes, placed instances, placed triangles, draws, the category shares), the 5 biggest meshes, the blockers, the paths of the files you wrote, and the run time.
