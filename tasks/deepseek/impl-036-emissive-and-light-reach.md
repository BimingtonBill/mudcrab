# impl-036 - Glow (emissive exposure, HDR + bloom) and the over-bright big lights

Owned files: crates/engine/src/lights.rs,crates/engine/src/render.rs,crates/engine/src/app.rs
Acceptance: cargo test -p engine --lib,cargo clippy -p engine --all-targets -- -D warnings,cargo fmt -p engine -- --check

Read `tasks/deepseek/context.md`, `tasks/deepseek/templates/implementation.md`, and
`docs/research/visual-gaps-spec.md`: Gap 1 and Gap 4 (3) are this task.

## Goal

Two engine-only fixes from the spec:
1. **Gap 4 (3): big lights.** `intensity_for_radius` (`lights.rs`) scales intensity with the
   square of the radius, so Blackreach's `FalmerCityLight02NS` (radius 4334) floods the ceiling
   orange. Use `(radius * 0.5).min(INTENSITY_REFERENCE_RADIUS = 256.0)` as the reference
   distance. That is byte-identical for every radius ≤ 512, so the calibrated torches do not move,
   and the big light keeps its reach. Test that both properties hold.
2. **Gap 1: glow.** Emissive reaches the materials at glTF magnitude 1.2-3.6, against an ambient
   of 650-800 and lights at about 50x that, so it is invisible.
   - Add `EMISSIVE_EXPOSURE` beside `LIGHT_EXPOSURE` and apply it in the material handler
     (`render.rs`). Restructure the handler's early return so materials **without** an
     `OPEN_SKYRIM_material` blend pair are handled too: the mushroom caps are OPAQUE/MASK and are
     missed today.
   - Add `Hdr` + `Bloom::NATURAL` to the main streaming camera.
   - Check the portal camera: it has `Tonemapping::None` on purpose, so the doorway is
     tonemapped by the main camera. Make sure the doorway does not lose or double the bloom, and
     explain what you chose.

## Calibrate (a tuning task: holdout rules apply)

- Build release (`cargo build --release -p engine`), render with the shots mode
  (`docs/design/reference-shots.md`), and measure with `python tools/research/exposure_stats.py`.
- The references are in the lead's checkout at `C:/Modding/MudcrabProject/local/reference/uesp/`
  (read-only). Copy them into your checkout with `python tools/research/copy_reference_shots.py`
  (read it first). The shots files are `tools/reference/uesp_shots_4x3.json` and
  `uesp_shots_blackreach_04.json`. **`SR-place-Blackreach_04` has never been rendered: render it.**
- **Tune `EMISSIVE_EXPOSURE` only on** `SR-place-Blackreach`, `SR-place-Blackreach_02` and
  `SR-place-Blackreach_04`, plus poses of your own. Do not look at `SR-place-Blackreach_03`
  or `SR-interior-Alftand_12`/`_13`/`_14` until your value is fixed. They are the lead's holdout;
  report their numbers once, at the end.
- Tamriel daylight must not change: compare the nine `SR-place-Alftand*` renders before and
  after. Bloom may add a little glow, but the median brightness must stay within 5%.

## Report

Per the implementation template. Include:
- the before and after renders of the three tuning shots and of the holdout shots, each with its
  measured numbers;
- the chosen constants, with how each was measured.
