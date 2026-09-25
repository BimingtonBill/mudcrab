# impl-219 - Graphics settings: one resource, presets, CLI and a settings file, applied to every camera

Owned files: crates/engine/src/graphics_settings.rs (new),crates/engine/src/tonemapper.rs (fold into the new resource, keep `--tonemapper` and `M` working),crates/engine/src/config.rs (the flags only),crates/engine/src/app.rs (registration only; remove the hard-coded camera settings the resource now owns),crates/engine/src/portal.rs (only the portal camera's spawn components: have it take the same settings),crates/engine/src/render.rs (only the water-reflection camera's spawn components),crates/engine/src/field_notes.rs (record the active settings in NN.json),crates/engine/src/shots.rs (record the settings in the shots log),crates/engine/src/lib.rs,docs/demo/README.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal (the demo). The user's ask, relayed from the Fun Q&A session: add graphics settings that expose Bevy 0.19's rendering features. The survey is at `local/research/fun/rendering-ideas.md` and the plan at `local/research/fun/rendering-plan.md` (step 1); read both.

Today the demo uses almost none of them:
- `app.rs` ~1634 sets `Msaa::Off`, with no TAA, SMAA or FXAA;
- no SSAO;
- no AutoExposure;
- no contact shadows, SSR or volumetric fog.

## What to build (step 1: the resource and the basic knobs; a live in-game panel is impl-220)

1. **A `GraphicsSettings` resource,** applied whenever it changes to the main camera **and** the portal camera. The portal camera must match, or its pipelines differ from the main view's and the first-open hitch comes back (see impl-202's commit). The water-reflection camera gets the settings where they make sense (AA off, SSAO off).
2. **Knobs in this step:**
   - AA: Off / FXAA / SMAA / TAA. TAA needs motion vectors and jitter, and must not smear the portal quad: check it.
   - SSAO: off / low / medium / high, plus radius.
   - Exposure: a fixed EV100, or `AutoExposure` with min, max and speed.
   - Tonemapper: move impl-217's switcher in, keeping the flag and the `M` key.
   - Bloom: on/off and intensity.
   - Shadows: map size, cascade count and distance, and filter (Gaussian / Hardware2x2 / Temporal). Keep the portal sun's cascade count equal to the main sun's, or Bevy panics (impl-188).
   - Contact shadows, if Bevy 0.19 exposes them as a simple component. Otherwise leave them for impl-221.
   - Portal resolution scale.
3. **Presets:**
   - `current`: today's exact look. It is the default and must be pixel-identical to today, so check it.
   - `bevy`: every built-in on at a sensible quality.
   - `custom`: whatever the flags or the file set.
4. **Input:**
   - `--graphics <preset>`;
   - per-knob flags, e.g. `--aa smaa`, `--ssao medium`, `--exposure auto`;
   - `--graphics-file <path.toml|json>`, and a default file `local/graphics.toml` read when present.
5. **Record the active settings** in F12's `NN.json` and in the `--shots` log, so every capture says how it was rendered.

## Checks

- Unit tests: preset contents, flag and file parsing, and that `current` equals today's camera components exactly.
- `cargo build --profile quick --bin engine`. The short tour passes under `current` and under `bevy`.
- Render one Riverwood pose and Sven's doorway (`tools/reference/doorway_shots.json`, door open) under `current` and `bevy`. Make a labelled sheet at `local/t219/sheet.jpg`, and check the doorway image still lines up under TAA and SMAA.
- If `--portal-bench` is available (impl-213 merged), give a rough `bevy` vs `current` GPU-time line, not run alone.
- Under `local/`, read only `local/t219*` and `local/research/fun/`.

## Report

The resource and its knobs; the flags and file format; what `bevy` turns on; test names; the sheet; the tour's last lines; any knob Bevy 0.19 made hard (say why).
