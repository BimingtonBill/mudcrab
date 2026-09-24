# Goal
Track: Phase 2 (for upstream). Distant LOD step 4a, converter half: keep the per-cell segments of
Skyrim's level-4 object LOD blocks (`.bto`) in the converted GLB, so the engine can hide the part
of a block that belongs to a loaded cell, as Skyrim does.

# Context
- Your checkout is cut from `phase2/lod-mesh-containers` (distant LOD step 1: the converter reads
  the LOD mesh containers and converts `.btr`/`.bto` to GLB). Use upstream's commit style
  (`feat(converter): ...`), with no `[deepseek]` prefix.
- How Skyrim hides object LOD (research: `C:/Modding/OpenSkyrimProject/local/research/lod-hiding-under-loaded-cells.md`,
  read its first 80 lines):
  - Every shape of a level-4 `.bto` block is a `BSSubIndexTriShape` whose triangles are split into
    up to 16 segments, one per cell of the 4x4 block. Segment index `i` = `4 * dx + dy`, where
    `dx`, `dy` (0..3) are the owner cell's offset from the block's south-west cell (x is the major
    axis). When a cell's full models load, Skyrim hides that cell's segment of every shape.
  - A segment entry is `u8 flag, u32 start index, u32 primitive count`. The start index points
    into the index buffer (start = previous start + 3 x previous count), and the count is in
    triangles. A table shorter than 16 means the trailing cells are empty. The flag is 0 in every
    shipped entry.
  - Shapes named `*-LargeRef` (e.g. `obj-LargeRef`, `objHD-LargeRef`) are a separate group with
    their own 16 segments. Keep the name; the engine treats them like the others for now.
  - Level-8 and level-16 blocks have one segment per shape, and nothing to split.
- The NIF parser already reads the table: `vendor/project-wormhole-nif/src/bs/sub_index_tri_shape.rs`
  (`BSGeometrySegmentData`, `segments`). The GLB export drops it today. Find where `.bto` shapes
  become glTF meshes (`crates/converter/src/lod.rs`, `mesh.rs`, and the material publication in
  `material.rs`, which currently asserts one primitive per mesh).
- The contract the engine half (a separate task, written against this contract) relies on:
  - For a `.bto` shape with 2 or more non-empty segments: one glTF **primitive** per non-empty
    segment, in the shape's glTF mesh. Each primitive has its own indices (that segment's
    triangles), may share the shape's vertex accessors, uses the shape's material, and carries
    **primitive extras** `{"openSkyrim": {"lodSegment": i}}` with `i` the segment's index in the
    table (0..15).
  - A shape with 0 or 1 segments, and every non-`.bto` model, is exported exactly as today (one
    primitive, no `lodSegment`).
  - Any mesh-count, bounds or material bookkeeping that assumed one primitive per shape must keep
    working: the block's recorded bounds (`lod_block.bounds_*`) must not change.
  - If you find you cannot put several primitives in one mesh without a large refactor, stop and
    report why, and propose the alternative (one glTF mesh and node per segment, each with the same
    extras) before implementing it.
- Converted data to check against (read-only): `C:/Modding/SkyrimConverted-upstream15/` (source NIFs
  extracted under `vfs/meshes/terrain/<worldspace>/objects/`). Write a small Python check of one
  real block's converted GLB in a temporary output folder if you can convert a single file;
  otherwise rely on tests.

# Scope
Owned files:
- `crates/converter/src/lod.rs`, `crates/converter/src/mesh.rs`, `crates/converter/src/material.rs`
  (only where the one-primitive assumption lives)
- `crates/converter/tests/fixture_lod.rs`
- `crates/dummy-content/src/nif.rs` (only if the fixture needs a segmented shape; it already writes
  a segment table)
- `docs/specs/converters/` (the LOD section: document the `lodSegment` extras)
- `docs/handoff/impl-558-p2-lod-objects.md`

No engine changes, no new dependencies. Do not bump the converter schema: the LOD series is
unreleased and the lead decides the bump.

# Done when
- Tests that fail on the old code and pass on yours:
  1. A level-4 `.bto` fixture shape with three non-empty segments converts to three primitives with
     `lodSegment` 0, 5 and 15 (or whatever indices the fixture uses), whose index counts are 3x each
     segment's triangle count, and together cover every triangle once.
  2. A shape with one segment, and an ordinary static model, convert exactly as before.
  3. The block's bounds are unchanged by the split.
- `cargo fmt --all -- --check`, `cargo clippy -p converter --all-targets -- -D warnings` and
  `cargo test -p converter` pass.

# Report
- Files changed and the approach in 5 lines.
- Where the one-primitive assumption lived and how you changed it.
- Each test's name and what it proves.
