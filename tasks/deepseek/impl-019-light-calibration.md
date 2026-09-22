# impl-019 - Calibrate interior and underground lighting against the reference screenshots

Owned files: crates/engine/src/lights.rs,crates/engine/src/app.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read these first:
- `tasks/deepseek/context.md` and `tasks/deepseek/templates/implementation.md`;
- `docs/design/reference-shots.md`, which covers the `--shots` renders and `tools/compare_shots.py`;
- `docs/research/reference-shot-poses.md`, which has the poses of the UESP references;
- `docs/design/lights-and-auto-doors.md`, section "Lights".

## Goal

The demo's Alftand interiors render **washed out to white** at eye height. For example, the
Alftand01 arrival at Creation (-947, 3958, 712) facing yaw 170 is a nearly white frame; see
`local/reference/smoke_shots/smoke-interior.png`. Make interiors and Blackreach read like the
game: dark spaces lit in pools by their `LIGH` references, with nothing clipping to white except
the light sources themselves. Tune against the reference screenshots.

## What is known

- `app.rs` `update_atmosphere` sets the ambient light, the sun and a **camera lantern**. The lantern
  is a warm point light at the camera: 3e9, range 2500, colour (1.0, 0.78, 0.52). It is on in
  every interior and underground space. It was a stopgap from before `LIGH` references were real
  lights, and the real game has no such light. The white frame is cream-coloured like the lantern,
  so the lantern is the first suspect. Show whether it is the cause, with a render with and
  without it, before you change anything else.
- `lights.rs` scales every `LIGH` light by `EXPOSURE_CALIBRATION = 50`, a value set by eye, and
  the camera applies no exposure or tonemapping pass beyond Bevy's defaults. See the module doc
  for the derivation. Whether 50 is right, or whether an exposure setting on the camera is the
  better knob, is for you to measure.
- The ambient brightnesses in `update_atmosphere` (interior 140, underground 110) were also set
  by eye.

## Method

0. **Setup.** Your checkout has no `local/reference/`. The references are in the lead's checkout at
   `C:/Modding/OpenSkyrimProject/local/reference/uesp/`, which you can read. Copy them into your
   checkout's `local/reference/uesp/` with a small script under `tools/research/` (the folder is
   git-ignored; never commit the images). The smoke-interior pose is in
   `C:/Modding/OpenSkyrimProject/local/reference/smoke_shots.json`.
   **The poses are rough:** another worker (research-020) is refining them now, and many current
   renders face a wall or the wrong way. So compare **per space**, not per shot. Pool the
   luminance statistics of all references of one space (Alftand01, Alftand02, AlftandWorld,
   Blackreach) and compare them with the pooled renders of that space. Also add a few poses of
   your own, at eye height looking along corridors and across caverns in each space (add them
   to a shots file of your own under `local/`), so the renders show typical views rather than
   walls.
1. Render the reference poses:
   ```
   target/release/engine.exe --assets C:/Modding/SkyrimConverted --shots tools/reference/uesp_shots_4x3.json --shots-out local/calib/<run>
   ```
   Then compare them with the references:
   ```
   python tools/compare_shots.py tools/reference/uesp_shots_4x3.json local/calib/<run> local/calib/<run>-cmp
   ```
   Look at the pairs (you can see images) and measure them. Write a small script under
   `tools/research/` that reports, per image, the mean luminance, the 5th and 95th percentile
   luminance, and the fraction of clipped pixels. Compare reference and render.
2. **Tuning set:** tune only on the interior and Blackreach shots whose `name` ends in an even
   digit, or is `SR-place-Blackreach`, `SR-place-Blackreach_02` or `SR-place-Blackreach_04`. The
   others are the lead's **holdout**, used to check your result afterwards. Do not look at their
   renders or tune on them.
3. Change as little as fixes it: the lantern (remove it, or make it faint), the ambient, the
   calibration factor, or a camera exposure setting. Every constant you change keeps a doc comment
   saying what it was measured against.
4. Rebuild with `cargo build --release -p engine` after each change, then render and measure again.
   Keep a table of runs.

## Constraints

- Exterior daylight must not change. Keep the sun and the exterior ambient as they are, and check
  one exterior reference render before and after.
- The pose file and the references are inputs; do not edit them. If a pose is clearly wrong, say
  so in the report and leave it out of your numbers.
- research-020 also runs the engine while you do. That is fine: only the images matter here,
  not timings. Run one engine at a time yourself, and do not measure FPS. `--shots` exits by
  itself.
- Run the engine from your checkout: `target/release/engine.exe`, rebuilt from your changes.

## Done when

- The Alftand01 arrival frame (the smoke-interior pose) is no longer washed out.
- On the tuning set, pooled per space, the render's median luminance is within about a factor of
  1.5 of the reference's, and clipped pixels stay under 2% except where the reference clips too.
- The acceptance commands pass, and any test that pinned an old constant is updated with its
  reason.

## Report

Per the implementation template. Include the run table (change, then per-image mean, p95 and
clip for render vs reference), the before and after of the smoke-interior frame, and the paths of
the comparison images you judged by.
