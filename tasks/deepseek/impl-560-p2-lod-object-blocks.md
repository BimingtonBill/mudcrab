# Goal
Track: Phase 2 (for upstream). Distant LOD step 4a, engine half: stream Skyrim's far-away object
blocks (buildings, rocks, bridges baked into `.bto` LOD) beside the terrain blocks the engine
already draws with `--lod`. Hide the part of a level-4 block that belongs to a loaded cell, as
Skyrim does, and never draw a coarse object block over a finer one.

# Context
- Your checkout is cut from `phase2/lod-engine-terrain` (step 3). Use upstream's commit style
  (`feat(engine): ...`), with no `[deepseek]` prefix.
- Read first: `crates/engine/src/lod.rs` (the whole LOD tier: planner `plan_blocks`, spawn,
  readiness `track_lod_readiness`, the terrain clip `update_lod_clip`/`ClipMask`, validator,
  metrics) and the design `C:/Modding/MudcrabProject/local/research-557/DESIGN.md` sections 2,
  5, 6 and 8 ("4a"). Tonight's decisions override the design where they differ:
  1. **Object blocks hide per owner cell, not per pixel.** The terrain clip (`LodTerrainMaterial`,
     a fragment discard over cells whose full-detail terrain shows) must NOT be used for objects:
     19-47% of object-LOD triangles reach outside their owner cell, so a pixel clip cuts them
     wrongly. Research: `C:/Modding/MudcrabProject/local/research/lod-hiding-under-loaded-cells.md`.
  2. **The converter contract** (being implemented in parallel, task impl-558): in a level-4 object
     GLB, a shape with 2 or more segments is exported as one glTF primitive per segment, each with
     primitive extras `{"openSkyrim": {"lodSegment": i}}`, `i` in 0..15. Segment `i` belongs to
     the cell at offset `dx = i / 4`, `dy = i % 4` from the block's anchor (its south-west cell).
     Bevy puts primitive extras on the primitive's entity as `GltfExtras`
     (`has_explicit_material_exclusion` in `crates/engine/src/streaming.rs` reads the same
     component). A primitive without `lodSegment` belongs to the whole block. Today's converted
     data has no `lodSegment` yet, so every primitive is whole-block; the code must work both ways.
  3. **Hide a segment when its owner cell is loaded.** Use the same "loaded" signal as the terrain
     clip: the cell's full-detail terrain patches are visible (the set `update_lod_clip` builds from
     `TerrainPatch` + `ExteriorCellGrid`). Share that computation rather than duplicating it, e.g.
     make the covered-cell set a resource both systems read. Hidden = `Visibility::Hidden` on the
     primitive entity; shown again when the cell unloads.
  4. **Object blocks are selected exclusively, not nested.** Terrain keeps a coarse block resident
     under a finer one and lowers it (`depth_offset_for`). For objects that would draw every
     building twice. Select object blocks as a quadtree instead: start from the coarsest band's
     blocks in range, and split a block into its four children of the next finer level while it
     is within that finer level's band distance. The leaves are drawn. When a block is split, all
     four children are wanted, whatever their own distance. When they are merged, the parent
     comes back. Use the same rect distance and the unload-scale hysteresis as terrain, so a
     split/merge does not flap at the boundary. A child the table does not list is simply absent.
     Objects use the terrain bands (`lod_bands`); a level with no band is never drawn.
     Keep the switch seamless: keep the old blocks until their replacements are ready, if that
     fits the existing `Loading`/`Resident` states. If it doesn't, say so and draw a one-frame
     gap instead.
  5. **Material:** keep the GLB material as it is (culling, normal map, roughness); do not apply
     the terrain rewrite, the clip extension or `AlphaMode::Mask`. Mark primitives
     `NotShadowCaster`, like terrain. Hide the converter's non-rendering primitives (primitive
     extras `openSkyrim.materialExclusion`, e.g. a `WATER` node in terrain blocks) for both kinds,
     and count them.
  6. **Depth offset:** object blocks get the same per-level lowering as terrain blocks of that
     level (design §2): the LOD objects were generated standing on the lowered LOD land.
- `LodBlockKind::Objects` already exists in `crates/engine/src/world/database.rs`, and the
  `lod_block` table already lists `objects` rows; `plan_lod_blocks` plans `Terrain` only today.
  Objects are on whenever `--lod` is; `--lod-objects off` turns them off.
- Converted data for reference: `C:/Modding/Mudcrab/SkyrimConverted-upstream15/` (717 Tamriel object
  blocks at levels 4, 8 and 16 under `meshes/terrain/tamriel/objects/`). You need no GPU: the lead
  runs the real-data render.

# Scope
Owned files:
- `crates/engine/src/lod.rs`
- `crates/engine/src/config.rs` (the `--lod-objects` switch and its test)
- `crates/engine/src/world/database.rs` (only if the object rows need a query change)
- `crates/engine/src/streaming.rs` (only to share `has_explicit_material_exclusion` and add counters
  to `StreamingMetrics`)
- `crates/engine/src/metrics.rs` (only for new counters)
- `crates/engine/src/app.rs` (only for `--lod-fixture` object rows, if the fixture needs them)
- `docs/handoff/impl-560-p2-lod-object-blocks.md`

No converter changes, no shader changes, no schema bump, no new dependencies, no trees (step 4b).

# Done when
- Tests that fail on the old code and pass on yours:
  1. The object quadtree: for a camera cell and a table of level 4/8/16 object blocks, the plan
     draws each area exactly once (no block together with any of its ancestors), splits every
     block within the finer band's distance, and keeps the split through small camera moves
     (hysteresis).
  2. Segment ownership: `lodSegment` 0, 5 and 15 of a block anchored at (-8, 4) map to owner cells
     (-8, 4), (-7, 5) and (-5, 7).
  3. Visibility: with owner cell (-7, 5) loaded, only segment 5 is hidden, and it is shown again
     when the cell unloads. A primitive without `lodSegment` is never hidden by a cell.
  4. `--lod-objects off` parses and removes objects from the plan; `--lod` alone keeps them on.
  5. The exclusion check recognises `{"openSkyrim":{"materialExclusion":"..."}}` primitive extras.
- With `--lod` off nothing changes, and every existing LOD test still passes (terrain nesting
  unchanged).
- `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets -- -D warnings` and
  `cargo test -p engine` pass.

# Report
- Files changed and the approach in 6 lines.
- How the quadtree and its hysteresis work, in 3 lines.
- Each test's name and what it proves.
- Anything you left out and why.
