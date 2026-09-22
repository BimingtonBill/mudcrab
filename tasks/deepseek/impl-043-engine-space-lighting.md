# impl-043 - Engine: per-space lighting, fog and sky from `space_lighting`

Owned files: crates/engine/src/world/lighting.rs,crates/engine/src/world/mod.rs,crates/engine/src/world/database.rs,crates/engine/src/app.rs,crates/engine/src/portal.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- `docs/research/visual-gaps-spec.md`, Gap 2 (Engine spec);
- impl-038's result, the converter change that publishes the table: its report is in
  `local/agents/runs/impl-038-converter-lighting-weather-snow.1/report.md`, and the code is in
  `crates/converter/src/esm/exporter.rs` (the `space_lighting` DDL) and `lighting.rs`.

## Data facts from impl-038 (measured from Skyrim.esm; trust these over the spec)

- One `space_lighting` row per interior cell (`XCLL` resolved against its `LTMP` template, where
  a set inherit bit means "take it from the template") and one per worldspace (weather colours at
  the day index).
- Alftand01: ambient (40,82,87), directional (60,80,89), fog (153,210,238), near 1100 / far 9000,
  power 0.7.
- Alftand02: ambient (35,61,65), fog (162,208,228), near 1500 / far 12000.
- AlftandZCell: ambient (45,79,83).
- Tamriel: ambient (203,220,220), directional (177,155,150), sky (41,97,117) / (139,175,194) /
  (94,149,179).
- **Blackreach and AlftandWorld: ambient (10,11,12), but fog (0,169,183) and sky fog
  (14,156,156). Their teal is fog, not ambient**, so the backdrop colour (clear colour) must come
  from the sky fog, not the ambient.

## What to build

The engine spec from Gap 2, with these decisions:
- A `SpaceLightingCatalog`, preloaded at startup like `AssetCatalog` (`world/database.rs`). Probe
  for the table: a database without it keeps today's behaviour.
- `update_atmosphere` (`app.rs`) looks up the active space's row on every `ActiveCell` change and
  applies:
  - ambient: colour from the row. The brightness keeps today's calibrated magnitudes
    (`INTERIOR_AMBIENT_BRIGHTNESS` 800 / `CAVERN_AMBIENT_BRIGHTNESS` 650) as the scale, so that
    impl-019's calibration still holds on average. Use a per-camera `AmbientLight` or the global
    one, whichever the portal needs (see below).
  - fog: `DistanceFog` with the row's colour and near/far in interiors. This replaces
    `fog_off`, so interiors get real haze. Exteriors keep the terrain-ring fog distances from
    impl-021 (the "no cliff" guarantee: see the `exterior_fog` tests) and take only the colour
    from the weather.
  - clear colour: from `sky_fog`/`sky_upper` for exteriors, and from the fog colour for
    interiors. Set it **per camera** (`Camera.clear_color`), including the portal camera, so a
    doorway shows the destination's backdrop and not the source's.
  - sun colour and illuminance scale from the weather. Keep today's 12,000 as the day magnitude.
  - `has_sky` replaces the `UNDERGROUND_WORLDSPACES` constant.
- The portal camera (`portal.rs`) renders the destination with the **destination's** ambient,
  fog and backdrop. This is what makes a doorway look like the room behind it. If a global
  resource makes that impossible, explain the limit and do what is possible (per-camera clear
  colour and fog at least).

## Calibrate and check (a tuning task: holdout rules)

- Build release and render the reference shots (`docs/design/reference-shots.md`,
  `tools/reference/uesp_shots_4x3.json`). Copy the references first with
  `python tools/research/copy_reference_shots.py`. Measure with `exposure_stats.py`.
- Tune only on the shots whose names end in an even digit. The odd-digit shots are the lead's
  holdout: report their numbers once, at the end.
- Tamriel daylight: the median brightness within 5% of before. Blackreach's black backdrop
  fraction must collapse, and Alftand's interiors should show haze.

## Report

Per the implementation template. Include:
- before and after pairs for 4 spaces;
- the numbers on the tuning and holdout sets;
- whatever the portal could or could not get per camera.
