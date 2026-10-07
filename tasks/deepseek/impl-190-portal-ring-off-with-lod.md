# impl-190-portal-ring-off-with-lod (track: Portal)

## Goal
Match vanilla Skyrim's view (the user's decision, 2026-10-07): when the current exterior worldspace
has terrain LOD data, the terrain-only ring is off by default (ring = `stream_radius`), and the LOD
draws everything beyond the full-detail grid. A worldspace without LOD data keeps the ring
(`DEFAULT_TERRAIN_RADIUS`, 8). An explicit `--terrain-radius N` always wins.

## Why (measured, local/reference/baseline-2026-10-07/RESULTS.md)
Same outdoor view (lod-far vs lod-far-ring-off): ring 8 = 1,156 terrain patches, render thread
14.2 ms, 64 fps; ring off with LOD = 100 patches, 7.2 ms, 121 fps. The LOD already covers that
distance; `update_terrain_lod_visibility` (crates/engine/src/streaming/lod.rs ~749) shows a LOD patch
wherever full-detail terrain is not ready, so turning the ring off leaves no gap.

## Context (portal branch, this checkout)
- `crates/engine/src/config.rs`: `DEFAULT_TERRAIN_RADIUS` (~181) with its measurement table;
  parsing at ~639 sets `terrain_radius_given`; ~865 turns the ring off for benchmark runs unless named.
- Readers of `config.terrain_radius`: streaming.rs ~1040, ~1143, ~4806 (wanted-cell window and unload
  limit), app.rs ~930 and `exterior_fog` ~2032-2070, atmosphere.rs ~69, ~712, ~1027, ~1642.
- LOD data lives in the world DB table `lod_chunks` (crates/engine/src/world/database.rs:
  `has_lod_table` ~726, `load_lod_chunks` ~638, rows keyed by worldspace). `LodHorizon`
  (streaming.rs ~946) is set only after the first async LOD answer, too late to decide the ring
  (the ring would start loading, then unload).
- The active exterior worldspace can change through a portal door (`ActiveSpace`, `lod_worldspace`
  in streaming/lod.rs ~262).

## Design (decided; do not change without reporting)
1. Config keeps whether the ring was named: store `terrain_radius_given: bool` (or make the field
   an explicit/auto choice); the benchmark rule at ~865 stays as it is.
2. `WorldDatabase` gains a synchronous, cached per-worldspace check `has_lod_for(worldspace_id) ->
   bool` (an `EXISTS` query on `lod_chunks` for that worldspace, false when the table is missing).
   Data-driven: no worldspace ids or paths in logic.
3. One resolver, e.g. `fn effective_terrain_radius(config, has_lod: bool) -> i32`: named ring wins;
   else `stream_radius` when the worldspace has LOD; else `config.terrain_radius`. Hold the result in
   a small resource (e.g. `TerrainRing { worldspace_id, radius }`) updated when the active exterior
   worldspace changes (and at startup before the first wanted-cell computation), and make every
   reader above use it instead of `config.terrain_radius`. Fog/far plane must follow the resolved
   ring (with LOD the fog already reaches the LOD horizon).
4. `--terrain-radius` help text (config.rs ~290) and the `DEFAULT_TERRAIN_RADIUS` doc say: default
   8 only for worldspaces without LOD data; off where LOD exists (vanilla behaviour).
Foundations check: no shortcut (view distance unchanged, LOD covers it); per-worldspace and
data-driven (mods/worldspaces without LOD keep the ring); one cached query per worldspace.

## Scope (owned files)
crates/engine/src/config.rs, crates/engine/src/streaming.rs, crates/engine/src/streaming/lod.rs,
crates/engine/src/world/database.rs, crates/engine/src/app.rs, crates/engine/src/atmosphere.rs.
Nothing else. Never git stash, amend, rebase or push. Commit only these paths, prefix `[deepseek]`.

## Done when
- Unit tests: resolver (named wins; LOD -> stream_radius; no LOD -> 8); `has_lod_for` on a fixture
  DB with rows for one worldspace and not another, and with no `lod_chunks` table; a worldspace switch
  re-resolves the ring.
- Existing tests pass or are updated only where they asserted the old always-8 default (say which).
- Checks, run through the build gate with BUILD_GATE_OWNER=portal:
  `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo fmt --all -- --check`
  `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
  `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`
  Check `df -h /d` before each build; below 15 GB, stop and report.

## Report
Diff summary per file; the resolver and where it is resolved; list of changed tests and why;
check results (last lines); anything in the design you could not follow.
