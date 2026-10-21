# impl-008 - Terrain: weld mismatched cell edges instead of rejecting the cell; skip dry water planes

Owned files: crates/engine/src/streaming.rs
Acceptance: cargo test -p engine --lib streaming,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md` first. Your checkout
has its own `target/` - the first build takes several minutes; that is expected.

## Goal

Two terrain defects on real Skyrim data, both in `crates/engine/src/streaming.rs`:

1. **Holes where vanilla data has a tiny seam.** The strict terrain check rejects a whole cell when
   one edge height differs from the resident neighbour by more than 0.01 units
   (`crates/engine/src/streaming.rs:1817-1825`, "terrain edge does not match neighbor"), so the cell
   is not drawn and the player sees a void. Real `Skyrim.esm` has such seams: measured with
   `python tools/research/land_edges.py "<Data>/Skyrim.esm" 3c <x> <y>`, Tamriel cell (18,18)'s north
   edge differs from (18,19) at **1 of 33 points by 24 units**, and (18,20)'s east edge at 1 of 33
   points by 16 units. Both cells are rejected on the Alftand demo's starting spot (log:
   `LAND failed strict validation cell=00008F64 reason=terrain edge does not match neighbor Exterior { worldspace_id: 60, grid_x: 18, grid_y: 18 }`).
2. **Grid lines from water planes.** Every cell with a finite `water_height` gets a full 4096x4096
   water plane (`crates/engine/src/streaming.rs:474`), even where the terrain is entirely above the
   water, and the planes' edges show through the ground as a cell-aligned grid. Root cause and
   evidence: `docs/research/terrain-blending-seams.md` (research-004, reviewed), its "Defect B" and
   fix step (2).

## What to build

1. **Weld, do not reject.** When a cell's edge differs from a resident neighbour's shared edge, make
   the new cell's edge heights equal the neighbour's before its mesh is built (the resident cell is
   authoritative, so already-drawn geometry never moves). Recompute or carry the normals so the
   welded vertices shade consistently. Keep a metric counting welded points (add a field next to
   `terrain_seams_validated` in the same metrics struct only if it lives in `streaming.rs`; otherwise
   log it) and log one `warn!` per welded edge with the cell, the neighbour and the maximum delta.
   Keep rejecting genuinely broken input the check was also guarding: an edge of the wrong length, or
   non-finite heights. Decide a sanity bound above which you still reject (e.g. a delta over 512
   units means corrupt data, not a seam) and state it.
2. **No water plane where the terrain is dry.** Skip the water entity when every terrain height of
   the cell is above `water_height` (use the cell's own height samples). Leave partly submerged cells
   as they are - clipping the plane to the shoreline is out of scope.

## Tests (each must fail before your change)

- Two neighbouring synthetic cells whose shared edge differs at one point by 24 units: the second
  cell now spawns, its edge equals the first's exactly, the weld is counted/logged, and
  `validate_streaming_lifecycle` reports nothing.
- An edge of the wrong length (or a NaN height) is still rejected.
- A cell whose terrain lies entirely above its water height spawns no water entity; a cell with one
  sample below the water still does.
- The existing test `accepts_matching_neighbor_edges_and_rejects_cracks` encodes the old rule: update
  it to the new one and say how in your result.

## Scope

Only `crates/engine/src/streaming.rs`. Do not touch the terrain shader or the layer-weight packing -
that is a separate task.

## Crosstalk

You run alone; no sibling. Report per the implementation template.
