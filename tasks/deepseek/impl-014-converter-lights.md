# impl-014 - Converter: a `lights` table from LIGH records (schema v5)

Owned files: crates/converter/src/esm/exporter.rs,crates/shared/src/lib.rs
Acceptance: cargo test -p converter --lib,cargo test -p shared,cargo clippy -p converter -p shared --all-targets -- -D warnings,cargo fmt -p converter -p shared -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md` and the design
`docs/design/lights-and-auto-doors.md` (its "Converter" section is your contract) first.

## Goal

Keep what the engine needs to turn Skyrim's `LIGH` references (torches, braziers, Dwemer lamps,
Blackreach's fungus lights) into real lights: each `LIGH` base record's radius, colour, flags and
falloff, in a new `lights` table exactly as the design's DDL.

## What to build

1. The `lights` table per the design, created with the others, one row per `LIGH` record whether or
   not it has a model (the existing `statics` rule for models stays as it is).
2. Parse `DATA`: verify the field layout against **real bytes** before trusting the design's summary -
   read a few `LIGH` records from `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/Skyrim.esm`
   through the crate's own reader in an `#[ignore]` test (as impl-005 did for `XTEL`; see its
   `#[ignore]` test in `exporter.rs`), and use the UESP page "Skyrim Mod:Mod File Format/LIGH" as the
   reference. Record in your result the layout you confirmed, including the record size.
   `FNAM` (fade) is optional.
3. Schema version 5: `WORLD_DATABASE_SCHEMA_VERSION` in `crates/shared/src/lib.rs` and the
   `schema_info` insert in `exporter.rs`. Do not edit the engine; the lead updates its fixtures.
4. While reading real data, count how many `REFR`s of `LIGH` bases carry an `XRDS` radius override on
   the demo route (Alftand01 `000152C3`, Alftand02 `00056C1B`, AlftandWorld `00069857`, Blackreach
   `0001EE62`) and say whether it is common - the design adds a column for it only if it is.

## Tests (fail before your change)

- A synthetic `LIGH` with a known `DATA` produces the exact row (radius, colour, flags, falloff, fade).
- A `LIGH` without `MODL` still gets a `lights` row and still gets no `statics` row.
- A truncated `DATA` produces no row and no panic.
- The `#[ignore]`d real-data test checks at least one torch-style light's values.

## Crosstalk

`impl-015-engine-lights.1` runs at the same time and consumes this table through the design's DDL.
If you must deviate from the DDL (a column name, a type, a unit), message `impl-015-engine-lights.1`
the exact change and put it in your result. Quote any message that changed your work.

## Done when

Acceptance commands pass. Report per the implementation template, including the confirmed `DATA`
layout and the `XRDS` count.
