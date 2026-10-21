# impl-572 (Phase 2): a `LIGH` fixture and its placed reference

Handoff note for the worker task `impl-572-p2-lights-fixture`. Branch is cut from `main`
(`417a262`); nothing was merged. Upstream commit style, no `[deepseek]` prefix.

## What was added

`dummy-content` could already write the record types the converter's ESM parser consumes, plus an
interior cell with a reciprocal `DOOR`/`XTEL` pair. It can now write a light, behind a new
opt-in preset, so the point-lights work can be tested with no game installed.

- **New CLI preset** `dummy-content gen <out> --with-lights` (needs `esm`), which replaces
  `Skyrim.esm` with one exterior cell, one `LIGH` base record and the single `REFR` that places
  it. `--with-interior` and `--with-lights` each replace the plugin, so passing both is an error.
- **New library entry points** `esm::PRESET_LIGHT` and `esm::plugin_with_lights(spec, light)`,
  plus the `esm::Light` struct (the base record's fields, its optional `MODL`, the cell the
  reference stands in, and the reference's transform and `XRDS` radius override).

`LIGH` `DATA` is the 48-byte layout every `LIGH` record in `Skyrim.esm` carries and the converter
reads (`crates/converter/src/esm/exporter.rs` on `portal`, `insert_light`, ~:660-728): time (i32),
radius (u32), colour (RGB plus one unused byte), flags (u32), falloff exponent (f32), then FOV,
near clip, flicker period, flicker intensity amplitude, flicker movement amplitude, value (u32)
and weight (f32). The fixture writes the seven tail fields as zero; nothing reads them.
`FNAM` is the 4-byte fade. The reference's `XRDS` is the 4-byte `f32` the exporter stores in
`references.radius_override`.

### Preset values (`esm::PRESET_LIGHT`)

| Field | Value | Why |
| :--- | :--- | :--- |
| `EDID` | `GeneratedLight01` | the crate's generated-naming convention |
| `MODL` | `meshes/generated.nif` | the only mesh the fixture tree writes; the exporter reads no `LIGH` model |
| cell | grid (0, 0) | the square the crate's other fixtures place their static in |
| `DATA` time | `-1` | the value retail data writes for an always-on light (`DefaultTorch01NS_FastSaturate`, `000CB3B0`) |
| `DATA` radius | `512` | the widest common `LIGH` radius |
| `DATA` colour | `(216, 128, 39, 0)` | warm, and every byte distinct so an offset error is visible |
| `DATA` flags | `0` | clear: the portal engine skips a light that sets its off-by-default (`0x20`) or negative (`0x04`) bit, and a fixture that set either would be dark |
| `DATA` falloff | `1.0` | vanilla's value |
| `FNAM` fade | `1.0` | vanilla's value; the brightness knob of a converted light |
| reference position | `(1024, 2048, 128)` | inside the cell's 4096-unit square |
| `XRDS` | `1024.0` | deliberately twice the base radius, so a reader that took the override cannot be confused with one that took the base |

## Files changed

- `crates/dummy-content/src/esm.rs` — `Light`, `PRESET_LIGHT`, `plugin_with_lights`, the two
  record writers, validation, and unit tests.
- `crates/dummy-content/src/main.rs` — the `--with-lights` flag, `write_lights_plugin`, a shared
  `preset_spec()` for both presets, and tests.
- `crates/converter/tests/fixture_lights.rs` — new: reads the generated plugin back through
  `converter::esm::binary::parse_plugin_file`.
- `docs/specs/converters/dummy-content.md` — CLI, generated tree, library API, deviations and
  test pointers.
- This note. No converter source, no schema, no dependency change.

`src/layout.rs` was not touched.

## Tests

- `crates/dummy-content/src/esm.rs`
  - `a_light_is_appended_after_the_exterior_world` — the records above the world group are
    byte-identical to the exterior-only plugin, the `LIGH` record starts at or after the
    exterior-only plugin's end, and the counts are one `LIGH`, one `XRDS`, one `FNAM`.
  - `an_interior_and_a_light_in_one_cell_get_distinct_form_ids` — the private writer composes
    both extras and no two records share a FormID (the door reference takes offset 3 in the
    cell's FormID block, the light reference offset 4).
  - `rejects_invalid_lights` — empty/non-printable `EDID`, an escaping `MODL`, a cell outside
    the spec, and non-finite position, rotation, falloff, fade and radius override.
  - `output_is_deterministic` now also covers the light preset.
  - `exterior_only_output_is_unchanged` and `an_interior_is_appended_after_the_exterior_world`
    are untouched and still green, which is the "existing presets keep their bytes" guard.
- `crates/dummy-content/src/main.rs` — `parses_the_lights_preset_flag` (including that the two
  presets cannot be combined and that the flag needs `esm`) and `writes_the_lights_preset_plugin`.
- `crates/converter/tests/fixture_lights.rs`
  - `generated_light_plugin_holds_a_light_and_its_placed_reference` — one `LIGH` with
    `EDID`/`MODL`, `DATA` 48 bytes with `time -1`, `radius 512`, colour `(216,128,39,0)`,
    `flags 0`, `falloff 1.0` and a zero tail, `FNAM` 1.0; one reference in the exterior cell
    whose `NAME` is the `LIGH`'s FormID, whose `DATA` is `(1024, 2048, 128)` with no rotation,
    and whose `XRDS` is `1024.0` and not the base radius.
  - `generated_light_plugin_never_panics_under_truncation_or_mutation` — the prefix sweep and
    256 bit flips, the same shape as `fixture_doors.rs`.
- Falsified once: with `PRESET_LIGHT.radius_override` changed to `512.0`, the converter test
  failed at `fixture_lights.rs:192` with `left: 512.0, right: 1024.0`, so the `XRDS` assertion
  reads the bytes rather than a default. The preset was reverted immediately.

## Checks run (all in this worktree)

| Command | Result |
| :--- | :--- |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p dummy-content -p converter --all-targets -- -D warnings` | clean (only the unrelated `proc-macro-error2` future-incompat notice) |
| `cargo test -p dummy-content -p converter` | all green: 105 passed / 7 ignored (dummy-content lib, including the unchanged exterior hash test), 48 passed (converter lib), and every integration binary including `fixture_lights` 2 passed and `fixture_doors` 2 passed |

`cargo run -p dummy-content -- <out> --with-lights` itself was **denied** by this session's
permission layer (the command needs approval and no approval surface exists here), so the CLI was
exercised through `run_gen` in `writes_the_lights_preset_plugin`, which takes the same arguments
and writes the same file through the same writer. That is the one done-when item not verified
through the built binary.

## Notes for the lead

- Both presets write their reference into grid (0, 0). If a later task wants lights and interiors
  in one fixture, the writer already accepts both extras (and the FormID blocks do not collide);
  only the CLI refuses the combination, because each flag *replaces* `Skyrim.esm`.
- The `LIGH` `MODL` never reaches a converted world: the exporter's `lights` table is filled from
  `EDID`, `DATA` and `FNAM`. Same class of gap as the door `MODL`, and documented as such in the
  spec's deviations.
- `XRDS` is an `f32`, not a FormID, so the load-order remap leaves it alone on both branches
  (checked against `form_id_offset_in_subrecord` on `portal`); the value survives any load order,
  unlike `XTEL`'s destination id.
