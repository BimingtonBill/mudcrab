# impl-021 - Distant terrain: a terrain-only ring beyond the full-detail cells

Owned files: crates/engine/src/streaming.rs,crates/engine/src/config.rs,crates/engine/src/app.rs,crates/engine/src/world/cache.rs,crates/engine/src/world/database.rs,crates/engine/src/world/components.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md` first.

## Goal

The user wants to see distant terrain. Today the engine streams exterior cells to
`stream_radius` (default 2, a 5x5 grid about 20,000 units across). It draws nothing beyond, and the
camera's far plane sits just past that (`app.rs`, `far = CELL_SIZE * (stream_radius + 2) * 2`).
Skyrim loads the same 5x5 grid at full detail and shows everything further away as low-detail
terrain. Add that second tier: **terrain only** (no references, no lights, no scenes) out to a
much larger `terrain_radius`, with distance fog so the edge fades into the sky instead of
ending in a cliff.

## Design (decided; keep it)

- **Two rings, exterior worldspaces only.** A cell within `stream_radius` of the camera cell is
  loaded fully, exactly as today. A cell outside it but within `terrain_radius` loads only its
  terrain (the landscape mesh with its layers, as the full path builds it) and its water plane,
  if it has one. Interiors are unchanged.
- **Moving the camera** upgrades a terrain-only cell to full when it enters the inner ring, and
  downgrades a full cell when it leaves (despawn its references, keep or rebuild its terrain).
  There must never be two terrain meshes for one cell. Door pre-streaming (`PrestreamCells`) and
  the portal's destination isolation keep working: pre-streamed cells stay full cells.
- **Config:** `--terrain-radius <n>`, default 12. Choose the default by measurement: see below.
  `unload_radius` keeps its meaning for full cells. Terrain-only cells unload beyond
  `terrain_radius + 1`.
- **Far plane:** it covers the terrain ring, `CELL_SIZE * (terrain_radius + 2)` or similar. Keep
  the depth precision in mind; Bevy uses reverse-Z, so a long far plane is cheap.
- **Fog:** Bevy's `DistanceFog` on the streaming camera in exteriors. It is clear up to about the
  full-detail edge and reaches full density at the terrain-ring edge, in the clear colour: the sky
  outdoors, the dark underground colour in Blackreach. `update_atmosphere` (`app.rs`) already
  switches the clear colour per space, so it should drive the fog too. In interiors the fog is off,
  or so far away that it changes nothing. The portal camera must not get odd fog: check it.
- Where the loading pipeline (the database request queue, `cell_cache.rkyv` access, spawning)
  needs a terrain-only request kind, add one. Read how `plan_cells` / `wanted_cells`
  (`streaming.rs`) and the cell requests work before designing that.

## Tests (write them to fail before your change)

- Ring classification as a pure function: which cells are full, which are terrain-only and which
  are not wanted, for a camera cell and the two radii. Include a pre-streamed cell outside the
  inner ring, which stays full.
- An upgrade and a downgrade as the camera moves one cell, with no duplicate terrain.
- The fog / far-plane numbers for the default radii.

## Measure and look (you can build and run the engine)

- `cargo build --release -p engine`, then render views with the shots mode (see
  `docs/design/reference-shots.md`). Look out over the Pale from the Alftand ruins, e.g. Tamriel
  at (76000, 75000, -3000) with yaw 40, pitch 10, and at yaw 220. Use the reference shots file
  `tools/reference/uesp_shots_4x3.json` for the Alftand exteriors. You can see images: look at the
  renders, and check that distant mountains appear and fade into the sky.
- Frame rate: another program may be using the GPU, so FPS numbers are rough. Still compare
  terrain radius 2 (off), 8, 12 and 16 on the same view, using the engine's frame-time log. Pick
  the largest default that stays comfortably interactive, and report the numbers with that caveat.

## Scope

Only the owned files. Do not edit `portal.rs`, `transition.rs`, `lights.rs` or `player.rs`. If you
need a hook in one of them, describe it in the report. No crosstalk partner. Report per the
implementation template, with before and after screenshots of the far view.
