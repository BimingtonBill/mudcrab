# Goal
Track: Phase 2 (for upstream). The converter publishes Skyrim light records as a `lights` table and
each reference's radius override (`XRDS`) as `references.radius_override`, with the world database
schema bumped 3 → 4, so the runtime has the data to place point lights.

# Context
- Your checkout is cut from `phase2/lights-fixture` (impl-572's branch, which adds a `LIGH` record
  and a lit reference to the dummy-content fixture). Upstream commit style
  (`feat(converter)!: ...`), no `[deepseek]` prefix.
- The implementation exists on this project's `portal` branch, commit `5cd1bf4`
  (`git show 5cd1bf4 -- crates/converter/src/esm/exporter.rs`). Port its converter part:
  the `lights` table DDL, the `radius_override` column in the `"references"` DDL, the `LIGH` arm of
  the record dispatch (always `insert_light`; a `statics` row only when the record has a `MODL`),
  `insert_light` (`LIGHT_DATA_MIN = 20`, DATA/FNAM layout documented there), the `XRDS` read in
  `insert_reference`, `"lights"` in the required-table list, and its unit tests.
  Leave out everything else that commit's neighbours touch on portal (the `DOOR/ACTI/FLOR/CONT/TREE`
  widening, door links, `space_lighting`, `matos`).
- `main`'s exporter differs from portal's; re-apply the change to `main`'s code, don't copy the file.
- Schema: `WORLD_DATABASE_SCHEMA_VERSION` in `crates/shared/src/lib.rs` 3 → 4 (portal's comments
  say "schema 5"; write 4). Update the test literals that hard-code the DB version
  (`grep -rn '"schema_version":3\|schema_info VALUES(3' crates/`), the Compatibility line in
  `docs/roadmap/02-core-engine.md` (database schema 4), and `docs/specs/converters/db-schema.md`
  (a `lights` section and the new `references` column). `CONVERTER_SCHEMA_VERSION` stays 14.

# Scope
Owned files: `crates/converter/src/esm/exporter.rs`, `crates/converter/tests/fixture_lights.rs`
(extend impl-572's test), `crates/shared/src/lib.rs` (the one constant), the test literals the grep
finds, `docs/specs/converters/db-schema.md`, `docs/roadmap/02-core-engine.md`, and your handoff note
`docs/handoff/impl-573-p2-lights-converter.md`. No engine source changes beyond the schema-literal
tests. No new dependencies. No machine-specific paths.

# Done when
- The fixture test runs `esm::exporter::create_tables` + the export on the impl-572 preset and asserts:
  the `lights` row's radius, colour, flags, falloff and fade; the lit reference's `radius_override`;
  `NULL` for a reference without `XRDS`; no `statics` row for a model-less `LIGH`;
  `schema_info.version == 4`.
- The ported unit tests pass; the real-plugin test stays `#[ignore]`d behind `MUDCRAB_SKYRIM_DATA`.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test -p converter -p shared -p dummy-content` pass, and `cargo test -p engine --lib` still
  passes with the bumped literal.

# Report
Per Done-when item: holds or not, and the test name. List every file changed and why.
