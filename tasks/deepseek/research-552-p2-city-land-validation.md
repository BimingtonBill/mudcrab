# Goal
Find why the land in the city worldspaces (Markarth especially, and Solitude) fails the engine's
strict LAND validation, which leaves holes in the ground, and say what the correct behaviour is.
Report the cause with evidence and the smallest fix. Do not implement it.

# Context
- Project: a Rust/Bevy reimplementation of Skyrim's renderer. The upstream Phase 2 code is
  `C:/Modding/wkj-phase2` (branch `phase2/integration-lod-shots`, read-only for you). Terrain
  loading and validation are in `crates/engine/src/streaming.rs`: search for
  `LAND failed strict validation`, `terrain edge`, and the seam weld (`weld`,
  `recompute_packed_normals`). The cell cache and LAND export are in the converter
  (`crates/converter/src/esm/`).
- The engine log shows lines like:
  `ERROR engine::streaming: LAND failed strict validation cell=0006D292 reason=terrain edge North differs from neighbor Exterior { worldspace_id: 93553, grid_x: -42, grid_y: -1 } by 1272 units`.
  Worldspace 93553 is MarkarthWorld. Differences are 500-1400 units, and one is 12,216 (cell
  000B2477 against -46,-2). The Phase 2 seam weld only fixes tiny one-point seams.
- What to establish, from the game's own data:
  - A city worldspace (MarkarthWorld, SolitudeWorld, WhiterunWorld...) is a child of Tamriel.
    Skyrim's WRLD record has "use parent land" style flags and a parent-use bitfield (PNAM), and
    child worldspaces may define LAND only for the city's own cells, relying on the parent's
    land or LOD beyond.
  - Do the failing edges sit between a city cell with its own LAND and a neighbour whose land
    belongs to the parent (Tamriel), or has no LAND at all? Or is the neighbour's LAND in the
    child worldspace simply different, because Bethesda never meant those edges to meet?
  - What does Skyrim do: does it compare or stitch those edges at all?
- Data:
  - `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db` and `C:/Modding/Mudcrab/SkyrimConverted-upstream15/skyrim_world.db`
    (tables `worldspaces`, `cells`, `land`; the `records` table holds raw records, including WRLD);
  - the game's `.esm` files are in `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/`
    (read-only).

# Method
1. List MarkarthWorld's cells with LAND, and the failing edge pairs (reproduce the comparison
   the validator makes).
2. Read MarkarthWorld's WRLD flags and parent fields, and say what they mean.
3. Classify each failing edge: a city cell against a parent-land cell, a missing neighbour,
   or a genuine mismatch inside the city.
4. Recommend the fix: skip validation on such edges, use parent land, or something else. Say
   what the engine should draw so there are no holes.

Write python scripts under `local/research-552/` and run them as `python <file>`. Inline
`python -c` is not allowed.

# Scope
Read everything. Write only under `local/research-552/`. No source changes, no builds.

# Report
- The cause in 3 sentences.
- The edge classification table.
- The fix: files, the change, and whether it is upstream-relevant (it is upstream's validator).
- Evidence paths.
