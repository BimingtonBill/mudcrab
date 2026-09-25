# impl-573 — Phase 2 lights converter: `lights` table and `references.radius_override`

Task: publish Skyrim `LIGH` records as a `lights` table and each reference's
`XRDS` radius as `references.radius_override`, with the world database schema
bumped 3 → 4, so the runtime has the data to place point lights.

Ported from this project's `portal` branch, commit `5cd1bf4`
(`converter: lights table from LIGH records, XRDS radius override, schema v5`),
which is the same change against `portal`'s exporter. `main`'s exporter differs
(the dispatch arm is `STAT | MSTT | FURN`, no `DOOR/ACTI/FLOR/CONT/TREE`
widening, no door links), so the change was re-applied to `main`'s code rather
than copied. The `DOOR/ACTI/FLOR/CONT/TREE` widening, door links,
`space_lighting` and `matos` from that commit's neighbours are deliberately left
out.

## What changed

- `crates/converter/src/esm/exporter.rs`
  - `lights` DDL (`id`, `editor_id`, `radius`, `color_r/g/b`, `flags`,
    `falloff`, `fade`) and `radius_override REAL` on the `"references"` DDL.
  - `LIGH` arm in the record dispatch: `insert_light` always, and a `statics`
    row only when the record has a non-empty `MODL` (an invisible light has
    nothing to draw, and most `LIGH` records are invisible).
  - `LIGHT_DATA_MIN = 20` and `insert_light`: `DATA` is time (i32), radius
    (`u32` in Creation units widened to a float), colour (RGB + one unused
    byte), flags (`u32`), falloff (`f32`); `FNAM` is the optional fade. A
    missing or short `DATA` is warned about and skipped rather than guessed at.
  - `XRDS` read in `insert_reference` (a single little-endian `f32`, may be
    negative; a truncated one is ignored).
  - `"lights"` added to the required-table list in
    `creates_hybrid_spatial_schema`, and the schema stamp in `create_tables`
    raised to 4.
  - Ported unit tests: `writes_a_lights_row_from_the_light_data`,
    `gives_a_lights_row_to_a_light_with_no_model_and_none_to_a_light_without_data`,
    `drops_a_truncated_light_data_without_panicking`,
    `stores_a_reference_radius_override_from_xrds`, plus the opt-in
    `lights_of_the_real_plugin_decode_through_export` (the real-plugin test,
    `#[ignore]`d, reads `OPENSKYRIM_SKYRIM_DATA`). The portal version's
    hard-coded `C:/Games/...` fallback was dropped: no machine-specific paths.
- `crates/converter/tests/fixture_lights.rs` — impl-572's fixture extended with
  the export half: `generated_light_plugin_exports_a_lights_row_and_a_reference_radius_override`
  and `a_light_without_a_model_exports_a_lights_row_and_no_statics_row`, both
  running `create_tables` + `export_to_db` on the `dummy-content gen
  --with-lights` preset assembled from the same `layout` constants.
- `crates/shared/src/lib.rs` — `WORLD_DATABASE_SCHEMA_VERSION` 3 → 4.
  `CONVERTER_SCHEMA_VERSION` stays 14.
- `crates/engine/src/app.rs`, `crates/engine/src/world/database.rs` — the DB
  version literals the exporter/engine fixtures stamp: the
  `StreamingFixtureDirectory` stamp and the `world/database.rs` test fixture to
  4 (`WorldDatabase::open` validates against the constant, so a stale stamp
  breaks `--streaming-fixture`), and the two integration-report literals in
  `accepts_current_complete_runtime_assets` and
  `rejects_truncated_manifest_and_integration_report` now format
  `shared::WORLD_DATABASE_SCHEMA_VERSION` instead of a literal, so they cannot
  drift on the next bump. The deliberately stale literals in
  `rejects_stale_or_incomplete_runtime_assets` (manifest = converter schema 14,
  report) and `rejects_previous_database_schema` (version 2) are left alone.
- `docs/specs/converters/db-schema.md` — a `lights` section, the
  `radius_override` column on `references`, and the schema-version note.
- `docs/roadmap/02-core-engine.md` — the Compatibility line now says database
  schema 4.

## Evidence

- `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets --
  -D warnings` clean.
- `cargo test -p converter -p shared -p dummy-content`: all green (converter lib
  109 passed / 8 ignored; `fixture_lights` 4 passed).
- `cargo test -p engine --lib`: 42 passed.
- The opt-in real-plugin test was run against the local install
  (`OPENSKYRIM_SKYRIM_DATA=<Skyrim SE>/Data`, driven from
  `local/run_lights_ignored.py` because the shell refuses the `VAR=value cargo
  test` form): it passed, printing 435 `LIGH` records with a 48-byte `DATA`,
  10,810/12,148 light references with an `XRDS` in the plugin and 228/231 on
  the Alftand → Blackreach route — the same numbers `5cd1bf4` records, decoded
  through `main`'s parser.

## Open items

- `docs/roadmap/02-integration-and-acceptance.md:8` ("world database schema 3")
  and `docs/specs/converters/dummy-content.md:97` ("`skyrim_world.db` (schema
  3)") still say 3. Both are outside this task's owned files.
- No re-conversion is done here: schema 4 assets have to be produced before an
  engine run can open them, and the engine refuses old ones in the meantime
  (`validate_runtime_assets` / `WorldDatabase::open`), which is the check
  working.
- The engine side (a `LightsPlugin` that reads `lights` and `radius_override`)
  is not part of this task; the converter and the schema contract are.
