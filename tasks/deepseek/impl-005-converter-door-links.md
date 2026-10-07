# impl-005 - Converter: XTEL door links, more renderable base types, schema v4

Owned files: crates/converter/src/esm/exporter.rs,crates/converter/src/esm/mod.rs,crates/shared/src/lib.rs
Acceptance: cargo test -p converter --lib,cargo test -p shared,cargo clippy -p converter -p shared --all-targets -- -D warnings,cargo fmt -p converter -p shared -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`, and the design
`docs/design/blackreach-demo.md` (the "Database schema version 4" section is your contract) first.

## Goal

Make the converter keep what a walkable Alftand -> Blackreach demo needs: which door leads where
(`XTEL`), and the models of doors, activators, flora, containers, trees and lights, so they render.

## Background

`docs/research/worldspace-transition-demo.md` (research t02, reviewed) established, with
`path:line`: `XTEL` is parsed nowhere (section 3); the bytes survive only in `references.data`
(`crates/converter/src/esm/exporter.rs:292`); the FormID remap only handles 4-byte subrecords
(`crates/converter/src/esm/mod.rs:145`); only `STAT`/`MSTT`/`FURN` fill `statics`
(`crates/converter/src/esm/exporter.rs:177`). Its section 4, steps 1-4, is the recipe. The route's
four doors and their `XTEL` targets are in its section 2.2 - use them as known-good values.

## What to build

1. **`door_links` table** exactly as the design's DDL, created with the other tables. Fill it while
   inserting `REFR` records that carry an `XTEL` subrecord of at least 28 bytes: destination ref
   (u32 LE at 0), arrival position (3 x f32 LE at 4), arrival rotation (3 x f32 LE at 16). A shorter
   `XTEL` is skipped with a warning, never a panic.
2. **Resolve destinations** in a post-pass inside the same export transaction: set
   `destination_cell_id` / `destination_worldspace_id` from the `references` row whose id is
   `destination_ref_id`. Unresolved stays NULL.
3. **Remap `XTEL`'s first four bytes** as a FormID for records from later plugins, leaving the 24
   float bytes untouched. Copy the pattern of the existing remap test in `crates/converter/src/esm/mod.rs`.
4. **More base types in `statics`:** `DOOR`, `ACTI`, `FLOR`, `CONT`, `TREE`, `LIGH` - same `MODL`
   and `OBND` handling as `STAT`/`MSTT`/`FURN`. A `LIGH` without `MODL` is simply not inserted (most
   lights are invisible).
5. **Schema version 4** in `crates/shared/src/lib.rs` and wherever the converter writes the version.
   Find every place the version 3 appears in the converter and shared crates, and say in your result
   which you changed. Do not edit the engine; the lead handles the engine side.

## Tests (write them first, see them fail)

- A synthetic plugin (the existing export tests show how they are built) with an interior cell, an
  exterior cell and two `REFR` doors pointing at each other: `door_links` has both rows with the
  exact floats, and the destinations resolve to the right cell/worldspace.
- The remap: a door in a second plugin whose `XTEL` points into the first gets the remapped id.
- A truncated `XTEL` (say 12 bytes) produces no row and no panic.
- A `DOOR` base with a `MODL` lands in `statics` with its model path; a `LIGH` without `MODL` does not.

## Real-data check (read-only, after your tests pass)

The real converted database from the old schema is at `C:/Modding/Mudcrab/SkyrimConverted/skyrim_world.db`
(read-only). You may **not** run the converter over the game install (it takes 20+ minutes and
writes outside your checkout). Instead, write a small test or script that parses `Skyrim.esm`'s four
route doors from the install at `C:/Games/Steam/steamapps/common/Skyrim Special Edition/Data/Skyrim.esm`
through your new code path if that is feasible within the crate's existing reader API; if not, say so
and rely on the synthetic tests. `tools/research/esm_route.py` (from t02) already finds them and is a
reference for the expected values.

## Crosstalk

You run alone on the converter; the engine workers (`impl-006-engine-door-streaming.1`,
`impl-007-player-controller.1`) consume your schema through the design doc, not through you. Do not
message them unless you must deviate from the design's DDL - then tell `impl-006-engine-door-streaming.1`
the exact column change and put it in your result.

## Done when

The acceptance commands pass in your checkout. Report per the implementation template; list every
schema-version site you changed in `critical_evidence`.
