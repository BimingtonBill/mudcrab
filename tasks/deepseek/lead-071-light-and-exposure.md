# lead-071 - The light is wrong

You are a **lead**: you plan, write briefs, spawn coding workers with `tools/ds_impl.ps1`, review
their diffs, and report. You do not write production code yourself.

## Your territory, and nothing outside it

You own:

- `crates/engine/src/lights.rs`
- `crates/engine/src/world/lighting.rs`
- `crates/engine/src/snow.rs`
- `crates/engine/src/shaders/` **except `portal.wgsl`**
- `crates/engine/src/app.rs` - **only** the light spawns, the `GlobalAmbientLight` inserts,
  `update_atmosphere`, and the ambient/exposure constants around them

Two other leads are running: one owns `crates/converter/**`, one owns `portal.rs`,
`transition.rs`, `door_animation.rs`, `doors.rs`, `demo_tour.rs` and `portal.wgsl`. Do not touch
either set. Keep your `app.rs` hunks to the lighting blocks so the file stays mergeable.

## Hard constraints

- **At most ONE `ds_impl.ps1` coding worker at a time** (~11 GB per checkout, ~50 GB free).
  Discard each worktree once you have read its diff.
- **Nobody runs the engine.** Claude renders and measures; you reason from the frames that already
  exist and from the code.
- **A calibration must be checked on scenes it was not fitted to.** This project has been bitten
  before: `AGENTS.md` records a pink-daylight regression from a lighting change that looked right
  on its own samples. Every tuning task you write must name a holdout.

## The three problems, and what is known

**1. Shadows are far too dark.** Shadows landed today (`8bd7a01`) and work - Bevy's default
cascades were sized for a metre-scale world and a Skyrim cell is 4096 units, so the range was
fixed. But look at `local/reference/_shadows-before-after.png`: the shadowed foreground goes
nearly black. Real sunlight fills its shadows with sky light; this engine's ambient is a single
`GlobalAmbientLight` calibrated when nothing cast a shadow at all. The same defect shows on any
surface facing away from the sun - `local/reference/rw1/RW-05-mill-wheel-across-the-river.png` has
a mill wall that reads black. Work out what the fill should be and where it comes from: the
weather record's own ambient is already resolved per space in `space_lighting`
(`crates/engine/src/world/lighting.rs`, `docs/research/visual-gaps-spec.md`).

**2. Interiors are blown white.** `docs/research/riverwood-reference-poses.md` reports the
Riverwood Trader and Alvor's House rendering washed out against their UESP references, and names
the likely cause: the glossiness-to-roughness fix (`59f1f20`) changed the specular response of
**every** material in the game, and the interior ambient levels were fitted before it. The
calibration is fitted to data that no longer exists. Re-fit it. `local/reference/rw6/` is the
current render set, `local/reference/riverwood/` holds the UESP references, and
`tools/research/exposure_stats.py` measures a render against a shots file.

**3. What the per-space lighting is worth now.** `e795350` calibrated it on the schema-17 data.
Two converter changes have landed since and a third is coming (another lead is publishing Skyrim's
specular mask, which will dim specular highlights by up to ten times on some surfaces). Decide
whether to re-fit now or wait for that reconversion, and say which and why - waiting may be the
right answer, and saying so is a result.

## How to report

A brief per task in `tasks/deepseek/`, numbered from 073 upwards, named `impl-NNN-slug` or
`research-NNN-slug`. Record measurements in `docs/research/`, not only in your report. Your final
report says what landed, what it was fitted on, what holdout it passed, and what you left alone.

## What "done" means for each worker you spawn

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p engine
```

Read the whole diff of every patch before you accept it.
