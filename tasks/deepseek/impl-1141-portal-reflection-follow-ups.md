# impl-1141-portal-reflection-follow-ups (track: Portal; queue B3a)

Base: portal (1ffb95ed or later). B3 is merged into portal (e1c4d77e: view policy c4eb07b6 + impl-1094 3f0d310e, b7bd1c42).
impl-1094 made every world producer widen its layers through `render::reflectable_layers` (render.rs ~408-430).
Three follow-ups from that merge message remain. Do them here.

## Problem and goal
1. **Portal spill light does not reach the reflection.** `crates/engine/src/portal_spill.rs` `SpillLight::layers()`
   (~line 205): `IntoSource` puts the light on layers `[0, 1]` only. The shadowless reflection route (water preset)
   draws only `UNSHADOWED_REFLECTION_LAYER` (4), and a light that lacks layer 4 does not light that view. A spot light
   that lights the room the player stands in must also carry layer 4. The rule for lights is in render.rs
   (`POINT_LIGHT_LAYERS`, ~450): a light names layer 4 directly, not through `reflectable_layers`.
   `IntoDestination` lights the doorway image on `DESTINATION_LAYER`. Leave it alone: destination content must not
   reach the main reflection (render.rs `reflectable_layers` says so).
   Spill is opt-in (`--portal-light-spill`, config.rs ~1429). The default path must not change.
2. **No test that LOD gets the reflection layer.** `crates/engine/src/streaming/lod.rs` ~855-861 stamps the LOD ring
   with `reflectable_world_layers()` and `Propagate(reflectable_world_layers())`. The table test
   `both_reflection_routes_draw_the_same_content` (render.rs ~2671) has rows for terrain, placed objects, world nodes,
   lights, sky dome, door layer, water (must not draw) and the destination (must not draw), but no LOD row.
   Add a test in `streaming/lod.rs`'s test module (or where that module's tests live) that spawns the LOD ring
   the way the code does and asserts: it intersects `UNSHADOWED_REFLECTION_VIEW_LAYERS` and
   `REFLECTION_VIEW_LAYERS`. If the ring is hard to spawn in a unit test, extract the layer choice into a small
   function both the code and the test call, and say so in the report.
3. **Document layer 4 as engine-reserved for mods.** Add the note on `UNSHADOWED_REFLECTION_LAYER` in render.rs (~400):
   the engine owns layer 4; mods must not put content or lights on it directly; a mod's own world content goes through
   `reflectable_layers` (or the world layer). Search `docs/` for a mod-facing layer or RenderLayers page
   (`rg -n -i "RenderLayers|render layer" docs`). If one exists, add the same note there. If none exists, say so in
   the report and do not create a new doc.

## Foundations (answer in the report)
- No content dropped; default path unchanged; spill stays opt-in; no scene or FormID knowledge.
- Data-driven and mod-safe: the reserved-layer note is the mod contract.
- Measured: this is a layer-membership change. The visual effect is checked later in a portal GPU run; not in this task.

## Scope (owned files)
crates/engine/src/portal_spill.rs, crates/engine/src/streaming/lod.rs, crates/engine/src/render.rs (doc and, if
needed, a test), and any docs page found under item 3. Never touch other files. Never stash, amend, rebase or push.
Commit only these paths, subject `[deepseek] impl-1141: ...` (portal branch commit style).

## Done when
Every acceptance command passes in the worktree (through the build gate, as set out in the brief's Accept list):
- `cargo fmt --all -- --check`
- `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`
- `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo nextest run -p engine --all-targets`

Set `BUILD_GATE_OWNER=portal` for each gated command. Check `python tools/disk_guard.py --drive C: --need 15` before
the first build; below 15 GB free, stop and report. Use the worktree's own `target/` only (never a shared target).

Tests to add or extend, all pass:
- portal_spill.rs: `SpillLight::IntoSource` layers intersect layer 4; `IntoDestination` does not.
- lod.rs: the LOD ring intersects both reflection view layer sets.

## Report
- The layer change in portal_spill.rs, with file:line.
- The LOD test: where it lives, what it asserts, and whether the ring was extracted to a helper.
- Where the layer 4 note went (render.rs, and any docs page). If no mod-facing docs exist, say so.
- Check results (fmt, clippy, nextest counts). Anything not covered.
