# Distant object LOD: per-cell segments as glTF primitives

Phase 2, distant-LOD branch (step 4a, converter half). Cut from `phase2/lod-mesh-containers`
(step 1: `.btr`/`.bto` read as NIFs in a different container and converted to GLB). A `.bto`
shape's level-4 segment table — up to 16 per-cell triangle ranges — now survives into the
published GLB as one glTF primitive per non-empty segment, each tagged
`extras.openSkyrim.lodSegment`, so the (separate, later) engine half can hide the part of a
block that belongs to a loaded cell, the way Skyrim does.

## Approach

- New `crates/converter/src/lod.rs`, wired into `mesh.rs` as a private sibling module
  (`#[path = "lod.rs"] mod lod;`, so no `lib.rs` change was needed — `lib.rs` is not in this
  task's owned-file list). `lod::segments_from_table` reads a `BSSubIndexTriShape`'s segment
  table (already parsed by `vendor/project-wormhole-nif`) and returns its non-empty entries with
  cumulative triangle offsets, or `None` when there are fewer than 2 (every ordinary shape, and
  most `.bto` shapes). `lod::split_lod_segments` runs this per exported mesh and, where it
  returns `Some`, replaces the mesh's one glTF primitive with one per segment: each keeps the
  original attributes (vertex data is shared) and gets a new `indices` accessor/bufferView over
  the *same* index buffer at that segment's byte range, plus `extras.openSkyrim.lodSegment`. No
  vertex or index bytes move or duplicate.
- `mesh.rs`'s `rewrite_materials_and_texture_uris` now takes `&NifFile` and calls
  `lod::split_lod_segments` on the parsed glTF document before `publish_gltf_materials`, so
  material publication (which runs after) sees the final primitive count.
- `material.rs`'s `publish_gltf_materials` no longer assumes one primitive per mesh (see below).
- `crates/dummy-content/src/nif.rs` grew `object_lod_segmented`, which writes a `.bto` fixture
  with an explicit segment table instead of `object_lod`'s single all-triangles segment
  (`sub_index_tri_shape` and the new `sub_index_tri_shape_with_segments` share a writer).
- `docs/specs/converters/nif-to-gltf.md` gained "5. Distant object LOD segments (`.bto`)",
  documenting the segment table, the split contract and the `lodSegment` extras (the old "5.
  Rust Implementation" section is now "6", so the numbering stays sequential).

## Where the one-primitive assumption lived

`crates/converter/src/material.rs`, `publish_gltf_materials`: for each exported mesh it did
`ensure!(primitives.len() == 1, ...)` and then wrote the material (and, for an excluded shape,
the non-rendering material plus `extras.openSkyrim.{shapeBlock,materialExclusion}`) onto
`primitives[0]` only. Changed to `ensure!(!primitives.is_empty(), ...)` and a loop over every
primitive, applying the same material to each. For the excluded-shape branch, a split primitive
already carries `extras.openSkyrim.lodSegment` (set by `lod::split_lod_segments`, which runs
first); the branch now reads that value out before building the exclusion extras object and
merges it back in, instead of overwriting the primitive's `extras` wholesale.

Nothing in `mesh.rs`'s own primitive construction assumed a count — `model.to_glb()` (vendor
code, out of scope) always emits exactly one primitive per shape, and `lod.rs` is what turns
that into several. `accumulate_mesh` (mesh.rs, unmodified) already iterated `primitives` and
read each one's shared `POSITION` accessor bounds, so bounds needed no change: every split
primitive points at the same `POSITION` accessor, so the union is identical to the unsplit
shape's bounds.

## Why `lod.rs` is a new file

It does not exist on `phase2/lod-mesh-containers` (`git show phase2/lod-mesh-containers:crates/converter/src/lod.rs`
fails: "exists on disk, but not in 'phase2/lod-mesh-containers'") or anywhere earlier in that
branch's history. On that branch, `.bto`/`.btr` export has no dedicated module: `.btr`/`.bto`
files are routed through the same NIF-to-GLB path as any other mesh
(`pipeline.rs`'s `convert_kind` matches `["nif", "btr", "bto"] => ("glb", AssetKind::Mesh)`),
and the actual conversion — `NifFile` parsing, `model.to_glb()`, material publication, texture
URI rewriting — lives in `mesh.rs` (`MeshConverter::convert_nif_to_glb`,
`rewrite_materials_and_texture_uris`) and `material.rs` (`publish_gltf_materials`). Neither file
had any per-shape segment-table logic; the parser read `BSSubIndexTriShape::segments` but
nothing consumed it, so every shape (including a 16-segment `.bto` block) still exported as one
primitive. The brief lists `lod.rs` as an owned file for exactly this reason: it is new work,
not a rename or relocation, so I created it rather than growing `mesh.rs` further, matching how
the brief separates "where the one-primitive assumption lived" (material.rs) from "where `.bto`
shapes become glTF meshes" (mesh.rs, now also lod.rs).

## Tests

`crates/converter/tests/fixture_lod.rs`:

- `fixture_object_lod_segments_split_into_one_primitive_per_cell` (new) — proves item 1. A
  fixture shape (`segmented_lod_shape`, 6 unshared triangles) is written twice: once through
  `object_lod` (single segment, for the bounds comparison) and once through the new
  `object_lod_segmented` with a 16-entry table whose non-empty cells are index 0 (1 triangle),
  5 (2 triangles) and 15 (3 triangles). Asserts: exactly 3 primitives; each one's
  `extras.openSkyrim.lodSegment` is 0, 5, 15 in that order; each one's index accessor count is
  3x its segment's triangle count; every primitive shares the same `POSITION` accessor; the
  three primitives' index sets partition the shape's 18 vertex indices exactly (no overlap, no
  gap — proved with a `BTreeSet`, since the fixture's triangles share no vertices, so a vertex
  index identifies one triangle); and the segmented output's `MeshConverter::glb_bounds` equals
  the unsplit output's, bit for bit.
- `fixture_object_lod_converts_with_its_nested_hierarchy` (extended) — proves item 2 for the
  `.bto`/single-segment case. Added assertions that the shape still exports exactly 1 primitive
  and that primitive carries no `extras` at all (not just no `lodSegment`). The existing
  `fixture_terrain_lod_converts_with_vertex_colours_and_texture_uris` and
  `pipeline_publishes_terrain_and_object_lod_glbs` cover the non-`.bto`/`BSTriShape` case: those
  shapes never reach `NifBlock::BSSubIndexTriShape`, so `lod::lod_segments` returns `None`
  immediately and they were unaffected already; running unchanged is the regression proof.
- `mesh::lod::tests` (new, in `lod.rs` itself, unit-level): `empty_table_has_no_split` (0
  entries), `single_non_empty_segment_has_no_split` (1 entry, with and without trailing empty
  ones), `several_non_empty_segments_split_with_cumulative_offsets` (the 1/5/15 table from the
  fixture test, checked against hand-built `BSGeometrySegmentData` values directly, independent
  of any NIF/glTF plumbing) — together they prove item 3's "0 or 1 segment" boundary and the
  cumulative-offset arithmetic in isolation.
- Also checked against real shipped data (not part of the suite, run manually with
  `MUDCRAB_LOD_FIXTURE` against the ignored `real_lod_mesh_parses_and_converts` test, output
  discarded afterward): `tamriel.4.4.-4.bto` from `C:/Modding/SkyrimConverted-upstream15/` has
  two `BSSubIndexTriShape` shapes, `Obj` and `obj-LargeRef`. Both split correctly — `Obj` into 9
  primitives (`lodSegment` 1,2,3,5,9,12,13,14,15) and `obj-LargeRef` into 15 (every index but 4)
  — each set ascending and matching the table's own order, confirming the two shape groups are
  each split independently and that ordinary shipped data (not just the fixture) produces the
  expected multi-primitive output.
- `generated_nifs_never_panic_under_truncation_or_mutation` (unchanged) still fuzzes
  `MeshConverter::inspect_nif`, which never reaches `lod.rs` (parse-only), so it is not a
  regression check for this change; it was already passing and stayed so.

## Commands

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p converter --all-targets -- -D warnings` | clean (only a pre-existing `proc-macro-error2` future-incompat note, unrelated) |
| `cargo test -p converter --test fixture_lod` | `ok. 5 passed; 0 failed; 1 ignored` |
| `cargo test -p converter` | lib `ok. 104 passed; 0 failed; 7 ignored`; every `src/bin/*` unit suite `ok`; `fixture_lod` `ok. 5 passed; 1 ignored`; `fixture_round_trip` `ok. 7 passed; 1 ignored`; doc-tests `ok. 0 passed`; 0 failures anywhere |

## Notes and limits

- No converter schema bump, no engine changes, no new dependencies, as scoped.
- The `flag` byte of each segment entry is read but not interpreted (matching the NIF parser's
  own comment that it is "kept verbatim rather than interpreted" — every shipped entry has
  `flag == 0`); if a future block ever set it, this converter would still split on it exactly
  like a zero flag.
- `*-LargeRef` shapes are not special-cased by name; they go through the same split as any other
  segmented shape, confirmed on the real `obj-LargeRef` block above.
- The split only adds accessors/bufferViews (JSON) over the existing index buffer; it never
  touches the binary chunk, so file size grows by a small, fixed amount per segment (roughly two
  JSON objects), not by any multiple of the vertex/index data.
- `mesh.rs`'s own `rewrites_glb_material_chunk_without_corrupting_the_container` unit test built
  a hand-crafted glTF document with no real source NIF; it now needs a real (if unrelated)
  `NifFile` to pass to the changed function signature, so I added a small
  `nif_fixture_without_lod_shapes()` helper (parses a trivial `dummy_content::nif::static_shape`
  fixture) rather than hand-building a `NifHeader`. It never touches `lod.rs`'s logic (the block
  index it passes doesn't resolve to a `BSSubIndexTriShape`), so the test's original intent and
  assertions are unchanged.
