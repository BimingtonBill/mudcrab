# Loose distant-LOD metadata (`.lod`, `.lst`, `.btt`)

Phase 2, distant-LOD branch. Loose copies of the three LOD metadata files in the
Data folder now overlay the extracted-archive VFS, so a LOD mod (DynDOLOD and
friends) is read the way the game reads it, instead of the archived copy or
nothing.

## Approach

`overlay_loose_assets` (`crates/converter/src/pipeline.rs:1026`) filtered loose
files to `["dds", "nif", "btr", "bto", "pex"]`, so the metadata the inventory
reads was never copied out of the Data folder. The filter now also accepts
`["lod", "lst", "btt"]` (`LOD_METADATA_EXTENSIONS`, `pipeline.rs:1024`).

Those three are read back, never converted, so they take the Data-relative path
itself rather than a `canonical_asset_path`: `lodsettings/<worldspace>.lod` sits
outside every `AssetKind` folder, and `.lst`/`.btt` are neither meshes nor in any
kind's extension list (routing them through a kind would re-root them under
`textures/`/`meshes/` and rewrite the extension). The path is lowercased, which
keeps the loose copy on the same VFS path as the archived one, matching the
case-insensitive rule the archive extractor already uses for its own collisions.
`asset_path.rs` is unchanged: no new `AssetKind` was needed.

## Tests

`crates/converter/tests/fixture_lod.rs` grew a fixture builder
(`lod_fixture`/`convert_lod_fixture`) that can ship the same metadata in a BSA,
loose under Data, or both, and a `lod_snapshot` that reads back every `lod_*` row
plus each published billboard's glTF document.

* `pipeline_reads_loose_lod_metadata_like_the_archived_copy` — loose only, no
  BSA. Asserts the loose run's `LodInventoryReport` and database/billboard
  snapshot equal the archived run's.
* `pipeline_prefers_loose_lod_metadata_over_the_archived_copy` — the BSA ships
  one metadata set and the Data folder another (other grid origin, level range,
  billboard count and instance list). Asserts the loose grid origin `(-64, -64)`
  and levels `4,8,16`, one billboard type of `200 x 400`, two instances, and that
  only `generated.tree.0.glb` is published.

**Both fail on the old code — seen, not assumed.** With the metadata clause of
the loose-file filter removed (i.e. the previous behaviour), the final test file
gives:

```
test pipeline_prefers_loose_lod_metadata_over_the_archived_copy ... FAILED
  assertion `left == right` failed: the archived metadata won: LodInventoryReport { grids: 1, ...,
    tree_types: 2, tree_instances: 3, billboards: [".../generated.tree.0.glb", ".../generated.tree.1.glb"], ... }
    left: (1, 2, 3, 1)
   right: (1, 1, 2, 0)

test pipeline_reads_loose_lod_metadata_like_the_archived_copy ... FAILED
  assertion `left == right` failed: the loose metadata was not read: LodInventoryReport { grids: 0, ...,
    tree_types: 0, tree_instances: 0, billboards: [], ... }
    left: (1, 0, 0, 0)
   right: (1, 1, 2, 3)

test result: FAILED. 5 passed; 2 failed; 2 ignored
```

The other five tests in the file (including the archived-fixture inventory test)
passed in that run, so the two failures are the new behaviour and nothing else.

## Commands

| Command | Result |
| --- | --- |
| `cargo test -p converter --test fixture_lod` (fixed) | `ok. 7 passed; 0 failed; 2 ignored` |
| `cargo test -p converter --test fixture_lod` (metadata clause removed) | `FAILED. 5 passed; 2 failed; 2 ignored` |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p converter --all-targets -- -D warnings` | clean |
| `cargo test -p converter` | all binaries `ok` (`117 passed` lib, `7 passed` fixture_lod, `7 passed` fixture_round_trip, bins `ok`), 0 failed |

## Notes and limits

* Only the path is case-folded, not the file. On Windows (the target) a loose
  file whose case differs from the archive's entry resolves to the same file, so
  it overrides. On a case-sensitive filesystem the two would sit side by side and
  the inventory's sorted scan could pick either; not exercised here.
* Any loose `.lod`/`.lst`/`.btt` anywhere under Data is copied into the VFS, not
  only the ones under `lodsettings/` and `meshes/terrain/**/trees/`. Files at
  other paths are inert — the inventory only scans those two places — and
  filtering by path would be a second copy of the inventory's search rules.
* Two loose metadata files that differ only in case now fail the run with the
  existing "normalized path collision" error, the same as the archive extractor
  and `canonical_asset_path` already do for the other kinds.
* Nothing here touches the schema: no row layout or counter changed, so no
  re-conversion of an existing output is forced by this patch.
