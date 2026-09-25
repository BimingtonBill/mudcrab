# impl-180 - Sun shadows outside stop working after walking in and out of a house a few times

Owned files: crates/engine/src/demo_tour.rs,crates/engine/src/config.rs,crates/engine/src/atmosphere.rs,crates/engine/src/shadow_layers.rs

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user, in play (2026-09-25, on a build with impl-176..179): "I walked in and out of Sven's house a few times and the shadows stopped working outside." An earlier note said the same more vaguely ("sun shadows sometimes stop rendering correctly after transitions"). This task reproduces it, finds the cause, and fixes it.

## What is known

- research-175 (`local/agents/research-175.out`, section "Note 2"; read it) ruled out the suns being despawned, the shadow map size, cascades tied to the old camera, and stale bin entries, and ranked three candidates:
  (a) the portal camera's view and the main camera's view share the same directional shadow-map array layers (per-view base index restarts at 0 in `bevy_pbr-0.19.0/src/render/light.rs` ~1708/1763/1833-1846; the array is sized by the max over views ~1322-1354); a doorway is usually open right after a crossing (the far door opens at the arrival);
  (b) `update_atmosphere` (`atmosphere.rs` ~634-637) writes every `DirectionalLight`, including the portal sun, when `ActiveCell` changes, in an unordered frame (one-frame);
  (c) shadow-view churn when the portal camera's `is_active` flips.
  The user's report is persistent, not one frame, which fits (a) or something else.
- impl-177 (merged, `crates/engine/src/shadow_layers.rs`) copies each directional light's `RenderLayers` onto its shadow views in the render world (`try_insert` when different). Check whether that interacts: shadow view entities are retained across frames; after crossings, could a view keep a stale layer set, or a view entity be re-used for a different light/camera?
- `portal.rs` (the portal camera, `PortalDestinationSun`, `place_destination_sun` ~1251-1288) is owned by another task running now (impl-181). **Do not edit portal.rs.** If the fix belongs there, write the exact change in your report instead.

## What to build

1. **Reproduce.** Give the demo tour a way to repeat the start of its route: `--tour-repeat N` (config.rs, next to `--tour-doors`), which walks the first `--tour-doors` doors, then walks them again, N times in all (Riverwood's first two doors are into Sven's House and back out). Keep the short-tour verdict (`tour PASSED after K crossings (short tour)`). Each time the tour is back outside it already photographs `NN-arrived.png`; make sure there is an exterior screenshot with shadow-casting objects in view after every return (the arrival pose outside Sven's has the porch and trees). Run it with `--tour-doors 2 --tour-repeat 5` and compare the outside screenshots: find the repetition at which shadows disappear (count shadow pixels or compare against the first one; say how).
2. **Find the cause.** Instrument what you need (temporary logs are fine; remove them before committing): the directional lights and their `RenderLayers` and `shadows_enabled` per frame after each crossing, the render-world shadow views (`LightEntity::Directional`) with their layers and which view they belong to, the number of views, the portal camera's `is_active`. Test research-175's candidates in order; a candidate is confirmed only by a run that shows it.
3. **Fix** it in your owned files (or report the exact `portal.rs` change if it belongs there), and show the repeated tour keeping its shadows on every return.

## Checks

- Unit tests for the new tour option and for any pure logic you add.
- `cargo build --profile quick --bin engine`, then `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t180 --tour-doors 2 --tour-repeat 5` before and after the fix. Read only `local/t180*` under `local/`.

## Report

The repetition where shadows were lost before the fix (with image names); the cause, with the run evidence that confirms it; the fix; the after-run showing shadows on every return; the tours' last lines; anything left for `portal.rs`.
