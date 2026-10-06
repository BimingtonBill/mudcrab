# impl-154 - PortalPlugin refactor step 6: the atmosphere and the terrain ring get their own files

Owned files: crates/engine/src/app.rs,crates/engine/src/atmosphere.rs,crates/engine/src/terrain_ring.rs,crates/engine/src/lib.rs,crates/engine/src/portal.rs,crates/engine/src/lights.rs,crates/engine/src/render.rs,crates/engine/src/snow.rs,crates/engine/src/streaming.rs,docs/design/portal-plugin.md

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. Approved by the user with the refactor (2026-09-23). **Do not start before the lead
says the Phase 2 conversion is over** - it builds.

## Read first

`docs/design/portal-plugin.md`: the "Decisions" row for **Step 6**, and "The order of work" item 6.
Steps 1-5 are merged (impl-153, `cdefe05`).

## Why

`git diff main portal -- crates/engine/src/app.rs` is still ~1,860 added lines, nearly all of them the
fork's lighting and the terrain ring, sitting inside upstream's own file. Upstream edits `app.rs`, so
every merge from `main` risks a conflict there. A **new** file never conflicts. And these are the
shapes the code will have when it goes upstream (the lighting proposal waits on upstream issue #24;
the ring is a Phase 2 candidate), so matching them now makes that merge a no-op.

## What to move - pure moves, one commit per file

1. **`crates/engine/src/atmosphere.rs`, with an `AtmospherePlugin`**: sky, fog, ambient and
   per-space lighting - today `app.rs` from `atmosphere_applies` through `update_atmosphere`
   (`SKY_COLOR`, the ambient constants and `AmbientBases`, `DAY_SUN_*`, `SKY_FILL`,
   `space_atmosphere`, `sky_fill_brightness`, `fallback_atmosphere`, `backdrop_of`, `sky_sun`,
   `ambient_light`, `atmosphere_fog`, `update_atmosphere` and their helpers) and the tests that test
   them. `run()` then adds it with one `app.add_plugins(...)` under exactly today's condition
   (`atmosphere_applies(...)`), inserting the same `ClearColor`.
2. **`crates/engine/src/terrain_ring.rs`**: the distant terrain ring's app-side code -
   `camera_far_plane`, `exterior_fog`, `fog_off` and their tests. The ring *planner* inside
   `streaming.rs` stays where it is (it is the streamer's; moving it would put a hole in
   `plan_cells`). `--terrain-radius` stays in `config.rs` where it is.

**Stays in `app.rs`:** the sun shadow cascades (`CREATION_UNITS_PER_METRE`, `SUN_SHADOW_*`,
`sun_shadow_cascades`) - their upstream PR is a small edit in `app.rs`, so leaving them makes a clean
merge; the material fixture; `converter_schema_version()`; `setup_world`; `fly_camera`; the
acceptance capture.

Rules:

- **No behaviour change.** Same constants, same values, same systems, same order, same run
  conditions. Only `pub(crate)`/`use` lines may change so the moved items stay reachable
  (`portal.rs` reads `space_atmosphere`, `ambient_light`, `atmosphere_fog`; `lights.rs` reads
  `INTERIOR_AMBIENT_*`; check `render.rs` and `snow.rs` too). Do not copy a function to avoid an
  import (decision H9).
- Nothing moved goes inside `PortalPlugin`: atmosphere and the ring are Phase-2-shaped.
- `crates/engine/src/main.rs` stays byte-identical to `main`.

## Checks

After each commit: `cargo test -p engine --lib`, clippy, fmt. At the end, a release build and both
tours (you may build and run the engine; the assets are read-only; create `local/` first):

```
mkdir local
cargo build --release -p engine
target/release/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t154-riverwood
target/release/engine.exe --assets C:\Modding\SkyrimConverted --demo alftand --walk --demo-tour local/t154-alftand
```

Both must end `tour PASSED` (8 and 4 crossings), no `panicked`. Lighting must be unchanged: compare
the mean luma of `local/t154-riverwood/NN-arrived.png` for every stage with the same files in the
lead's pre-refactor run, `C:\Modding\MudcrabProject\local\demo\t103-riverwood\` (same lighting
code; the tour poses are deterministic, but streaming timing is not, so expect small differences and
report them all rather than a verdict).

**If `cargo` is refused in your session** (impl-153's worker was), stop after the first commit and
report that, rather than writing the rest blind.

Report `git diff --stat main -- crates/engine/src/app.rs` before and after, and update the Status
line of `docs/design/portal-plugin.md`.
