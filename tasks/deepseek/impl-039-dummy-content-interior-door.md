# impl-039 - dummy-content: generate an interior cell and a linked load door pair (XTEL), for upstream

Owned files: crates/dummy-content/src/esm.rs,crates/dummy-content/src/lib.rs,crates/dummy-content/src/main.rs,crates/converter/tests/fixture_doors.rs
Acceptance: cargo test -p dummy-content,cargo test -p converter --test fixture_doors,cargo clippy -p dummy-content -p converter --all-targets -- -D warnings,cargo fmt -p dummy-content -p converter -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- `docs/adr/0001-isolated-dummy-content-crate.md` and `docs/adr/0002-*.md`: upstream's rules
  for this crate;
- `crates/dummy-content/src/esm.rs` and its tests;
- `crates/converter/tests/fixture_round_trip.rs`: how the converter consumes the fixtures.

## Why

This work goes upstream. Upstream's `dummy-content` crate synthesizes an ESM with exterior cells
only. The fork's biggest features (load doors, interiors, portals, seamless crossings) cannot be
tested by an upstream maintainer, who has no Skyrim and no door, interior or `XTEL` in the
fixtures. research-029's PR audit calls this the single highest-value addition
(`docs/pr/upstream-pr-plan.md`).

## What to build

- Extend `esm.rs` (additively; keep every existing output byte-identical for existing specs) so a
  `Plugin` spec can also describe:
  - **interior cells**: `CELL` in the interior cell group (group type 2/3 blocks), with `EDID`,
    `FULL`, `DATA` flags (interior), and no `XCLC`;
  - **`DOOR` base records** (with `EDID`, `MODL` model path, and `FNAM` flags, including the
    auto-load bit `0x02`);
  - **door references**: `REFR` with `NAME` = door base, `DATA` position/rotation, and **`XTEL`**
    (destination door REFR FormID, position, rotation, flags; 32 bytes as in Skyrim SE). Doors
    come in pairs: an exterior door ↔ an interior door, each `XTEL` pointing at the other.
  Read the converter's ESM parser (`crates/converter/src/esm/`) for exactly what it expects:
  group types, the `XTEL` length, the `DOOR` fields and FormID remapping. The fixture must pass
  through the real parser.
- A preset in the CLI (`main.rs`), such as `--with-interior`, that emits one exterior cell with a
  door into one interior cell with the return door.
- `crates/converter/tests/fixture_doors.rs`: generate the preset, run the converter's ESM export
  into a temporary database, and assert:
  - `door_links` holds both links with the right destination cells and worldspace;
  - the interior cell exists;
  - the auto-load flag survives.
  Follow `fixture_round_trip.rs`'s pattern.
- Keep the crate workspace-independent (ADR-0001). No proprietary data. Everything is synthesized
  from the seed.

## Scope

Only the owned files. Do not change `converter/src/**`. If the parser needs something the fixture
cannot give it, say so in the report. No crosstalk partner. Report per the implementation
template, including the exact record layouts you emit.
