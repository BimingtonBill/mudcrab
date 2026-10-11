# Goal
Add the converter function that builds one merged terrain mesh per exterior cell (`shared::terrain_mesh::TerrainCellMesh`) from a LAND record, with tests. Phase 1 track, #186 (terrain merge), converter half. This task builds the in-memory mesh only; writing files, texture arrays and pipeline wiring are later tasks.

# Context
- Worktree: `C:/Modding/Mudcrab/worktrees/p1-terrain-merge`, branch `phase1/terrain-merge` (off origin/main f40ca1df). Work and commit only there.
- Build folder: `CARGO_TARGET_DIR=D:/Mudcrab/builds/p1-target` (Phase 1's own). Before building, run `df -h /d`; below 15 GB, stop and report.
- The agreed format (read first): `C:/Modding/Mudcrab/mudcrab/local/research/terrain-merge/format-proposal.md`, including the "v2: agreed changes" section. The data types and layout helpers are already written in `crates/shared/src/terrain_mesh.rs` (uncommitted; the lead owns that file; do not change it, report if it needs a change).
- Today's path, for reuse and parity: `crates/converter/src/esm/cell_cache.rs`. Reuse its `decode_vhgt`, `decode_normals` and `extract_texture_layers` (make them `pub(crate)` if needed). Do NOT use `normalize_texture_layers`: it trims quadrants to 6 layers, and the new mesh keeps up to 8 slots. Engine reference for today's mesh: `crates/engine/src/streaming.rs:2815-2913` (`build_terrain_quadrant_mesh`) and `streaming::quadrant_layers` (`streaming.rs:2691-2741`, layer order: base first, then overlays by ATXT layer index, then texture FormID).
- Record layout per xEdit 4.1.5f defs: LAND `Core/wbDefinitionsTES5.pas:10417-10437`; BTXT/ATXT/VTXT `Core/wbDefinitionsCommon.pas:1371-1400` (BTXT/ATXT: LTEX FormID, quadrant u8, unused u8, layer s16; VTXT entries: position u16 (row*17+column in the quadrant), 2 unused bytes, opacity f32); quadrants 0 bottom left, 1 bottom right, 2 top left, 3 top right (`wbDefinitionsCommon.pas:582-588`).

# What to build
New file `crates/converter/src/esm/terrain_mesh.rs` (add `pub mod terrain_mesh;` in `esm/mod.rs`):
1. `pub fn build_cell_mesh(cell_id: u32, record: &RawRecord, known_ltex: &HashSet<u32>, key: [u8; 32]) -> Result<(TerrainCellMesh, CellMeshReport)>` (v3 of the format: slots hold LTEX FormIDs in load-order numbering, not table ids; `slot_stride` is 8, or 16 when a quadrant needs more than 8; more than 16 is an error naming the cell):
   - heights from VHGT (as `decode_vhgt` gives them, 33x33), copied into the 4 unwelded quadrants via `shared::terrain_mesh::vertex_index` and `grid_position`; same for VNML normal bytes and VCLR colour bytes (VCLR empty stays empty).
   - per quadrant, the slot list: slot 0 = the BTXT's LTEX FormID (or `DEFAULT_LAND_TEXTURE_FORM_ID` when the quadrant has no BTXT or its LTEX is not in `known_ltex`), then every ATXT layer with any opacity > 0, in today's order (layer index, then texture FormID). An ATXT whose LTEX is not in `known_ltex` is skipped and counted. `quadrant_slots` is `4 * slot_stride` long, quadrant by quadrant; weights are `slot_stride * 1156`.
   - weights: for each vertex of a quadrant and each slot 1..count, the VTXT opacity at that vertex, clamped to 0..=1 and quantised to u8 with `(opacity * 255.0).round()`. Slot 0 weight is 0 (implied). Unused slots stay 0.
   - the finished mesh must pass `TerrainCellMesh::validate` (build the ids map from `known_ltex` plus 0 for the test).
2. `pub struct CellMeshReport` with: quadrants carrying more than 6 slots (the deliberate change from today's cap), quadrants using the default texture, skipped ATXT layers (unknown LTEX), and whether the cell needed stride 16.

# Tests (in the same file)
- A synthetic LAND (build `RawRecord` subrecords by hand, as other converter tests do): heights land at the right vertices, including the duplicated inner seams (a vertex at grid (16, y) has the same height in quadrants 0 and 1).
- Slot order and weights: a quadrant with a base and 3 ATXT layers given out of layer order comes out in layer order, with the right weights at chosen vertices.
- 7 alpha layers plus the base fit stride 8; 8 alpha layers plus the base give stride 16; 16 alpha layers plus the base is an error naming the cell.
- A quadrant without BTXT gets FormID 0 (default) and is reported.
- Parity with today: for one synthetic cell, the heights, normals and colours of quadrant q in the new mesh equal what `build_terrain_quadrant_mesh` reads for that quadrant (compare against `CachedLand` data from `cell_cache`'s decoders; do not depend on the engine crate).

# Checks
`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo nextest run -p converter -p shared --all-targets`. All in the build folder above.

# Rules
- Never git stash, never rebase or amend, commit only the files you changed by name (`git add <paths>`), commit message in upstream style: `feat(converter): build a merged terrain mesh per cell (#186)` plus the line `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. Do not commit `crates/shared/src/terrain_mesh.rs` or `crates/shared/src/lib.rs` (the lead commits those first; if they are still uncommitted when you finish, leave them and say so).
- Foreground commands only; no background waits or sleeps.
- No pushes, no PRs.

# Report (under 300 words)
Files changed, the commit hash, test names and results, the checks' output lines, anything in the shared types that needs changing, and any parity doubt.
