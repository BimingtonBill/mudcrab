# impl-038 - Converter: per-space lighting (XCLL/LTMP/LGTM, WRLD climate/weather) and STAT directional snow (DNAM/MATO)

Owned files: crates/converter/src/esm/mod.rs,crates/converter/src/esm/exporter.rs,crates/converter/src/esm/lighting.rs,crates/converter/src/esm/directional_material.rs
Acceptance: cargo test -p converter --lib,cargo clippy -p converter --all-targets -- -D warnings,cargo fmt -p converter -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- `docs/research/visual-gaps-spec.md`: Gap 2 and the snow half of Gap 3 are this task. Its
  "Spec" sections define the tables.

## Step 0: settle the unknowns from real bytes (mandatory, before writing parsers)

The spec's record layouts come from web documentation and were **never checked against our data**.
The raw records are in `C:/Modding/SkyrimConverted/skyrim_world.db` (read-only, `records.data` /
`cells.data`) and in the game's `Skyrim.esm` (read-only). Write a script under `tools/research/`
and run it with `python tools/research/<script>.py`. It must print:
- the `XCLL` subrecord lengths across all cells, and for Alftand01 (86723), Alftand02 (355355),
  AlftandZCell (432216) and the Alftand cavern: the decoded fields and their `LTMP` template;
- each template's `LGTM` fields;
- **the `LGTM` inherit-flag polarity**, settled by arithmetic. The spec says AlftandZCell renders
  4.5x too bright and needs a template several times darker than Alftand02's. Which reading
  produces that?
- the `WRLD` `CNAM` climate of Tamriel (60), Blackreach (0x1EE62) and the Alftand cavern
  (0x69857), each climate's `WLST`, and one real `WTHR`'s subrecord sizes (`NAM0`, `FNAM`, `DATA`);
- `STAT` `DNAM` for `DweFacadeTowerRoof01SnowHeavy` and its `MATO` `DATA` bytes.

Report all of this. If the bytes contradict the spec, the bytes win.

## What to build

- A parser module for `XCLL`, `LGTM`, `CLMT` and the `WTHR` fields we use
  (`crates/converter/src/esm/lighting.rs`), and one for `DNAM`/`MATO`
  (`crates/converter/src/esm/directional_material.rs`).
- Add `LTMP`, `WRLD CNAM` and `STAT DNAM` (the `MATO` FormID) to `is_form_id_subrecord`
  (`esm/mod.rs`), so FormIDs remap for plugins.
- In `export_to_db` (`exporter.rs`), new tables:
  - **`space_lighting`**, one resolved row per interior cell and per worldspace, in the spec's
    column list;
  - **`matos`**;
  - two nullable columns on **`statics`**: `material_object` and `material_max_angle`.

  Resolve the `LTMP` → `LGTM` inheritance and the `WRLD` → `CLMT` → first `WLST` `WTHR` chain
  deterministically at conversion time. Keep every existing table and column exactly as it is.
- **Do not bump `WORLD_DATABASE_SCHEMA_VERSION` or `CONVERTER_SCHEMA_VERSION`**
  (`crates/shared/`, `cache.rs`). The lead does the bump. Make the new tables additive, so an
  engine that ignores them keeps working.

## Tests (write them to fail before your change)

- Parsers on hand-built byte slices and on slices copied from the real records you dumped. Put
  the bytes in the test as literals, since they are a handful of bytes. Test the lengths the data
  actually has.
- An `export_to_db` fixture that resolves `LTMP` → `LGTM` (both inherit cases) and
  `WRLD` → `CLMT` → `WTHR`.
- `DNAM`/`MATO` on the roof's real bytes.

## Report

Per the implementation template. Include:
- the Step 0 dump, especially the polarity verdict and its arithmetic;
- the table DDL;
- the resolved rows for the six spaces;
- anything the engine task needs to know.
