# impl-191 - Meshes that miss their first shadow queue get another chance (the shadows-after-exit race)

Owned files: crates/engine/src/shadow_layers.rs,crates/engine/src/render.rs (only to register a system, if needed)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. The user's capture `local/captures/2026-09-25_03-57-58/03.png` ("shadows still break when exiting an interior") was taken on a build WITH impl-185 (cc23747: a held-open door keeps its exterior destination loaded out to the unload radius). So the loss still happens.

## Known (impl-185, verified by the lead in the source)

Bevy 0.19 `queue_shadows` (`%USERPROFILE%/.cargo/registry/src/*/bevy_pbr-0.19.0/src/render/light.rs` ~2505-2634) walks an entity for a shadow view only when `DirtySpecializations::iter_to_queue` yields it (newly visible or re-specialized, `bevy_render-0.19.0/src/camera.rs` ~976-1010). Two misses are re-queued for the next frame (mesh instance not extracted, material not prepared). Three are a bare `continue`, and the entity is **never queued for that view again** until it is re-specialized or becomes newly visible:
- no specialized pipeline yet (~2559);
- render layers not intersecting the view's (~2582). This one matters here: impl-177 sets the shadow views' layers, and portal.rs moves cells between layer 0 and layer 2 at crossings;
- no mesh slab yet (~2611).

A burst of new or re-layered meshes, as at a crossing, makes these likely.

## What to build

A safety net that makes Bevy look at such entities again, cheaply:
1. Find the cheapest trigger that puts an entity back into `iter_to_queue` for the shadow views. Read the Bevy code; candidates include a `Changed<Mesh3d>`/`MeshMaterial3d` touch, the `EntitySpecializationTicks`, or the pending-queue resource itself in the render world.
2. Prefer a render-world fix in shadow_layers.rs. For example: each frame, for every shadow view, re-insert into its pending queue the visible entities that are in its `RenderVisibleEntities` but not in its shadow phase bins. Limit this to a bounded number of frames after an entity became visible, or rate-limit it, so the three drop paths retry like the other two.
3. In the report, say which mechanism you chose and why it is safe: no per-frame cost when nothing is missing, and no duplicate bin entries.

## Checks

- Unit tests for the pure selection logic.
- `cargo build --profile quick --bin engine`. Then reproduce with impl-185's `--tour-dwell`, **5 runs**: `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t191-<n> --tour-doors 2 --tour-dwell 2`.
  - Count the runs whose `02-arrived.png` has the roof-beam shadow across the boardwalk. For comparison, `local/captures/2026-09-25_03-57-58/03.png` is broken and `local/captures/2026-09-25_02-01-35/02.png` is correct.
  - Report the counts before (current portal) and after the fix.
  - Add an info log line saying how many entities the safety net re-queued per crossing.
- The short tour (`--tour-doors 2`, no dwell) still ends `tour PASSED after 2 crossings (short tour)`.
- Under `local/`, read only `local/t191*` and the two capture folders named here.

## Report

The mechanism; file:line of the Bevy paths it covers; the before/after counts; test names; the tour's last line.
