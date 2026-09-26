# Converter output snapshots (`insta`)

`crates/converter/tests/snapshots.rs` pins what the converter produces, so a
change to its output shows up as a readable diff in review instead of as a wrong
render hours later. Four snapshots, all fed by the deterministic `dummy-content`
fixtures through the real code:

| Test | Covers | Snapshot |
| --- | --- | --- |
| `converted_static_model_gltf_matches_snapshot` | The published `meshes/generated.glb`'s glTF JSON chunk: node tree (root, mesh node, the `Creation-to-glTF basis` rotation), one primitive with its `POSITION`/`NORMAL`/`TEXCOORD_0` attributes, accessors with their `min`/`max` bounds, material with its `KHR_materials_specular` extension and `openSkyrim` extras, relative texture URIs | 184 lines |
| `world_database_tables_match_snapshot` | The exported `skyrim_world.db`: `worldspaces`, `cells` (exterior grid plus the interior with no worldspace), `references` (transforms, `is_exterior`, both door references), `statics`, `texture_sets`, `landscape_textures`, `land`, `formid_map`. Blob columns are reported as byte lengths | 228 lines |
| `conversion_manifest_matches_snapshot` | The published `conversion-manifest.json`: schema version, completeness, archive file lists, input counts, one entry per converted asset | 123 lines |
| `pipeline_report_matches_snapshot` | The `PipelineReport` the pipeline returns: counts, sorted artifacts, `inputs_by_kind`, and the whole integration gate | 54 lines |

The model snapshot runs the full pipeline rather than `MeshConverter` alone, so
it covers the material stage's texture-URI rewriting too. The world-database
snapshot drives `EsmParser::convert_plugins` directly on the `--with-interior`
preset plugin, which is cheaper and keeps the exporter the only thing under
test.

## Redactions

* `pipeline_report_matches_snapshot` - `.elapsed_ms` is wall-clock time.
  `.artifacts` uses `insta::sorted_redaction()`: the pipeline appends each
  artifact as its parallel conversion finishes, so the order differs between two
  runs of the same fixture (observed: two texture paths swapping).
* `conversion_manifest_matches_snapshot` - `.**.source_hash`, `.**.output_hash`
  and `.**.hash`. Content digests carry nothing a reviewer can read, and a
  texture's digest depends on the basis-universal encoder rather than on the
  converter. Entry keys, output paths and sizes stay in the snapshot.
* The other two need no redaction: nothing in the GLB JSON or the database rows
  is derived from a path, a clock or a pid.
* `assert_no_machine_paths` walks each value before it is snapshotted and fails
  the test if any string carries the fixture directory or looks like an absolute
  path, so a machine path cannot be accepted into a snapshot later.

Tables that were deliberately left out of the database snapshot: `plugins` and
`conversion_cache` (absolute plugin path, conversion timestamp), `records`
(raw subrecord blobs, one row per record), `scripts`, `npcs`, `waters`, `lod`.

## Accepting a change

`cargo-insta` is not installed here, so the first run of a new snapshot fails
and writes `<name>.snap.new` next to the test. Read it and rename it to
`<name>.snap` to accept it, or install `cargo-insta` and use `cargo insta review`.

## Determinism

Two runs of `cargo test -p converter --test snapshots` produced byte-identical
`.snap.new` files (sha256 of all four compared between runs). The `sorted_redaction`
on `.artifacts` is required for that; before it, the report snapshot differed
between runs.

## Notes for review

* `OPEN_SKYRIM_material` does not appear in the model snapshot. The material
  stage writes that extension only for a material with a non-diffuse/normal
  texture slot or an alpha flag, and `dummy-content`'s static shape has neither.
  The same gap is why there is no material-JSON snapshot for an effect/alpha
  material: the fixture has no such material, and adding one means touching
  `crates/dummy-content`.
* There is no lights snapshot either: the exporter has no lights table and the
  fixture plugin has no `LIGH` records.
* `cells.interior_name` comes out of the exporter as `"GeneratedInterior\u0000"`,
  NUL terminator included, while `statics.editor_id` and
  `texture_sets.editor_id` have it stripped. Recorded as-is; if the NUL is a
  bug, the snapshot is where it will show.
* `roughnessFactor` is `0.19999998807907104` - an `f32` widened to `f64`, stable
  across runs, left unrounded.
