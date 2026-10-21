# impl-044 - Engine: directional snow material on STAT references (DNAM/MATO)

Owned files: crates/engine/src/snow.rs,crates/engine/src/shaders/snow.wgsl,crates/engine/src/render.rs,crates/engine/src/streaming.rs,crates/engine/src/lib.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- `docs/research/visual-gaps-spec.md`, Gap 3 (snow; the Engine spec);
- impl-038's report, `local/agents/runs/impl-038-converter-lighting-weather-snow.1/report.md`,
  and its tables: `matos`, and the `statics.material_object` / `statics.material_max_angle`
  columns.

## Data facts (from impl-038, measured)

- MATO `0x25129`: `dir_proj` (0,0,-1), falloff scale/bias 0.35/0.4, normal dampener 0.4, single-pass
  colour (107,116,126), single_pass 1.
- `DweFacadeTowerRoof01SnowHeavy` has max_angle 120. The other three snow facades have 90.
- The snow statics share the plain NIF with their non-snow twins. The snow is only the projected
  material, which is why the roofs render bare bronze.

## Status of the data (read first)

The published assets at `C:/Modding/Mudcrab/SkyrimConverted` do **not** have the new tables yet. The
reconversion is waiting on a user decision about disk space, so the real `skyrim_world.db` has
no `space_lighting`, `matos` or `statics.material_*`. So:
- Implement, and prove correctness with unit tests on **fixture databases** that you build in
  the tests. Use impl-038's exact DDL, from `crates/converter/src/esm/exporter.rs`, and the real
  values listed in this brief.
- The engine must behave exactly as today when the tables are absent (probe for them). Check
  this at runtime: build release and run `--demo-tour`, which must pass 4/4 unchanged.
- **Skip the calibration against renders.** It needs the new data. Instead, leave in your report
  a ready-to-run calibration plan: the shots, the commands, the tuning/holdout split and the
  metric. The lead resumes you after the reconversion.

## What to build

- Read the two `statics` columns and the `matos` table through the database, with probes, as the
  existing optional columns are read (`world/database.rs`: follow how `radius_override` or the
  lights table is probed). You own `streaming.rs` for the reference-spawn side. If you must touch
  `world/database.rs`, it belongs to impl-043: describe the change in your report instead, and
  read the table from your own module with a separate query.
- `snow.rs` + `snow.wgsl`: an `ExtendedMaterial<StandardMaterial, SnowExtension>`, following
  the `TerrainMaterial`/`WaterMaterial` pattern in `render.rs`. For a reference whose static has
  a MATO, replace its meshes' materials with the snow-extended version, which blends the snow
  colour onto surfaces by how far their **world-space** normal faces against `dir_proj`:
  - `dir_proj` (0,0,-1) is "snow falls down", so up-facing surfaces get snow;
  - use the max angle, the falloff scale/bias and the normal dampener;
  - modulate by vertex-colour alpha if present.
  Document the exact formula. Skyrim's is undocumented, so fit it by eye against the references:
  `SR-place-Alftand_Ruined_Tower` and `SR-place-Alftand_02`/`_05`/`_06` show the snow caps.
  Decide whether `dir_proj` is in world or object space, and state it.
- Keep a static without a MATO exactly as it is. The snow material must survive the portal and
  the existing material handler (`SkyrimBlendHandler`/emissive): run after it, or compose with
  it.

## Check it (holdout)

Build release and render with the shots mode. Fit on `DweFacadeTowerRoof01SnowHeavy` (the Ruined
Tower shot). Then check, without re-tuning, the other snow facades (`DweFacadeTower1x3Snow`,
`Arch01Snow`, `Tower1x4Snow`) in `SR-place-Alftand_02`/`_05`/`_06`.

## Report

Per the implementation template, with before and after images and the formula.
