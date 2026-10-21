# impl-053 - Calibrate the per-space lighting against the references (schema-17 data)

Owned files: crates/engine/src/app.rs,crates/engine/src/world/lighting.rs,crates/engine/src/portal.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`,
`docs/design/reference-shots.md`, and the module docs of
`crates/engine/src/world/lighting.rs` and `update_atmosphere` in `app.rs`, which is the code you
are calibrating.

## Where things stand

Per-space lighting now runs on real data: `C:/Modding/Mudcrab/SkyrimConverted` is converter schema 17 and
its `skyrim_world.db` carries `space_lighting` (810 rows), `matos` (42) and the `statics` snow
columns. Colours from the records are applied as **hue**, with the calibrated brightness kept.

The lead measured the whole reference set on that build (pooled medians, render against
reference):

| space | render / reference |
|---|---|
| Alftand01 | 1.8x too bright |
| AlftandZCell | 4.8x too bright |
| Alftand02 | 1.1x (good) |
| AlftandWorld | 3.5x too dark |
| Blackreach | 1.0x (good) |
| Tamriel | 0.69x, and the daylight hue has gone **pink** |

**The pink is the worst of it.** The exterior sun tint `(129,105,107)` is applied at full
saturation, so snow reads lilac: see `local/reference/cmp-s17/SR-place-Alftand_Ruined_Tower.jpg`
in the lead's checkout (read-only for you). Real daylight reads white to cool. Desaturate the sun
tint towards white, keeping a little warmth, or apply it at low strength; say which you chose and
why.

## Rules

- Tune on the shots whose names end in an **even** digit. The odd-digit shots are the lead's
  holdout. Report both sets.
- **Gate:** Tamriel's median comes back within 5% of the pre-space-lighting build (0.161, from
  `local/reference/render-lit` in the lead's checkout) and its hue is neutral to cool, not pink.
- Interiors: bring Alftand01 and AlftandZCell down, and AlftandWorld up, towards 1.0x without
  breaking Alftand02 or Blackreach, which are already right.
- Do not touch the emissive or light-radius constants in `lights.rs`/`render.rs`: another change
  owns them.
- Keep the fallback path (a database without the table) exactly as it is, with its tests.

## Commands

```
target/release/engine.exe --assets C:/Modding/Mudcrab/SkyrimConverted --shots tools/reference/uesp_shots_4x3.json --shots-out <dir>
python tools/research/exposure_stats.py --shots tools/reference/uesp_shots_4x3.json --render <dir>
python tools/compare_shots.py tools/reference/uesp_shots_4x3.json <dir> <cmp dir>
```

You can see images: look at the pairs, not only the numbers. Copy the references into your
checkout first with `python tools/research/copy_reference_shots.py`.

## Report

Per the implementation template: the constants you changed and what each was measured against,
the tuning and holdout tables, and before/after pairs for Tamriel, Alftand01, AlftandZCell and
AlftandWorld.
