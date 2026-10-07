# Goal
Find why Morthal's lake (the marsh water around Morthal, Hjaalmarch) is absent in both the
upstream Phase 2 engine and the demo engine, while other water (the Riverwood river) at least
draws a surface. Report the exact cause, with evidence, and the smallest fix. Do not implement it.

# Context
- Project: a Rust/Bevy reimplementation of Skyrim's renderer. Code bases:
  - `C:/Modding/wkj-phase2` (branch `phase2/integration-lod-shots`, read-only for you);
  - the demo engine, `C:/Modding/MudcrabProject` (branch `portal`).

  They share the converter (`crates/converter`, including the ESM exporter into the world
  database) and the engine's water drawing (search the engine crate for `water`, `WTHR`, `XCWT`,
  `water_height`, `WaterMaterial`).
- The symptom, from `docs/research/look-gaps-2026-09-24.md` item 5:
  - Morthal's lake is absent in both engines;
  - elsewhere water is drawn but white.

  The shot is `SR-place-Morthal` in `tools/reference/world_shots_4x3.json`. Renders are in
  `C:/Modding/Mudcrab/worktrees/p2-tools/shots-hand/{demo,up15}-world_shots_4x3/SR-place-Morthal.png`, and the
  screenshot is `local/reference/world/SR-place-Morthal.jpg`.
- How Skyrim does exterior water:
  - each exterior cell has a water height (`XCLW` in CELL, often "use the worldspace default"
    from WRLD's `DNAM`) and a water type (`XCWT`, else the worldspace's `NAM2`);
  - a cell has no water when its height is the special "no water" value, or its flags say so;
  - some water is placed separately: the marsh around Morthal may be water-plane statics or
    `ACTI` water references instead of cell water.

  Check which applies to Morthal's cells.
- Data: `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db` (tables `cells`, with `data`, `waters` and
  `references`; the `records` table has raw record data). The game's `.esm` files are in
  `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/` (read-only).

# Method
1. Find Morthal's exterior cells (around the `SR-place-Morthal` camera position, worldspace
   60, i.e. Tamriel; cell = floor(position / 4096)).
2. For those cells, show:
   - the raw water height and type from the records;
   - what the converter stored;
   - what the engine does with it. Why is no surface drawn?
3. Compare with a cell where water does draw (Riverwood's river, around the `RW-06` shot in
   `tools/reference/riverwood_shots.json`).
4. Name the step that loses Morthal's water, with file:line, and the smallest fix.

Write small python scripts under `local/research-551/` and run them as `python <file>`. Inline
`python -c` is not allowed.

# Scope
Read everything. Write only under `local/research-551/`. Change no source file. No builds.

# Report
- The cause in 3 sentences.
- A table: cell | raw XCLW / flags / XCWT | stored in the DB | what the engine does.
- The fix: files, the change, schema impact.
- Evidence file paths.
