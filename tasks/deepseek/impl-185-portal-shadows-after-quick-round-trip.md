# impl-185 - Sun shadows outside are lost after ONE quick trip into Sven's House and back

Owned files: crates/engine/src/portal.rs,crates/engine/src/transition.rs,crates/engine/src/shadow_layers.rs,crates/engine/src/streaming.rs,crates/engine/src/atmosphere.rs,crates/engine/src/lights.rs,crates/engine/src/config.rs,crates/engine/src/demo_tour.rs (only the repeat/dwell code of the tour: another task is removing the objective line from that file at the same time, so do not touch `spawn_demo_objective`, `update_demo_objective`, `DemoObjective` or the routes' `objective` strings)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal.

## The evidence (new: the user captured it with F12)

`local/captures/2026-09-25_02-01-35/` (read it: `notes.md`, `01..03.png`, `01..03.json`, `shots.json`):
- `02.png` (note "shadows working"): outside Sven's House, on the porch boardwalk, ~318 units from Sven's door `0001CBB0`, which is **Open** (so the portal camera is active, showing the interior). The roof shadow lies across the boardwalk and the path has dappled shadows.
- Then **one** round trip: into Sven's at 02:04:33, **out again 2 seconds later** at 02:04:35.
- `03.png` (note "went in and went out of sven's house, shadows not working properly"): the same spot, the door still Open at 305 units. No roof shadow on the boardwalk, no shadows on the path.
- The main-world state in `02.json` and `03.json` is **identical**: the same two directional lights (the main sun on layer [0], illuminance 19967, shadows on, the same cascade bounds; the portal sun on layer [2]), the same three cameras (main [0,1] order 0, portal [2] order -2, another [0] order -1), all active. So whatever broke is not in those components: look at the meshes' `RenderLayers`/visibility, the shadow-caster visibility lists, and the render world.

impl-180 (`local/agents/impl-180.out`, `tasks/deepseek/impl-180-portal-shadows-after-crossings.md`) walked 5 scripted round trips (`--tour-doors 2 --tour-repeat 5`) and saw no loss. The differences from the user's run are the thing to test: the user was inside for **2 s** (the tour stays much longer and takes survey shots), and came straight back out through the same door. research-175 (`local/agents/research-175.out`, note 2) ruled some causes out on paper. impl-177's `shadow_layers.rs` copies each directional light's `RenderLayers` onto its shadow views in the render world.

## What to do

1. **Reproduce.** Add a tour option `--tour-dwell <seconds>` (config.rs next to `--tour-repeat`; demo_tour.rs where a crossing's stage ends): after a crossing, wait only this long before walking to the next door, skipping the survey shots. Then run `target/quick/engine.exe --assets C:\Modding\SkyrimConverted --demo riverwood --walk --demo-tour local/t185 --tour-doors 2 --tour-dwell 2` and make sure an exterior shot is saved after the return from where the user stood (the poses are in the capture's `shots.json`: `02`/`03`; `--shots` renders a pose, but the bug needs the round trip first, so photograph from the tour itself, e.g. by walking the tour's camera to the `03` pose after the return, or compare the tour's own `arrived` shots before and after). Show before/after images where the shadows go. If 2 s does not reproduce it, try 0.5 s and 5 s and say what the threshold is.
2. **Find the cause** with a run that shows it (temporary logs are fine; remove them before committing). Candidates to check, most likely first:
   - when the player goes inside, the exterior becomes the portal destination: are the exterior's meshes moved to layer 2 (`DESTINATION_LAYER`) and back to layer 0 on the return? If the return comes before some step has finished (streaming, a despawn/respawn, a pending layer change), are some meshes left on the wrong layer, or re-layered without Bevy noticing that their shadow-caster visibility changed?
   - Bevy 0.19's directional-light shadow casters are chosen per cascade in the main world (`check_dir_light_mesh_visibility`, by the light's `RenderLayers` against the meshes'), then queued per shadow view in the render world (`queue_shadows`, bevy_pbr `render/light.rs`), with specialization caches keyed by view and entity. Does a mesh whose layers changed drop out of the main sun's shadow list and never come back? Check the counts of shadow casters per cascade before and after the trip.
   - impl-177's shadow-view layers (`shadow_layers.rs`): after the trip, do the main sun's shadow views still carry layer 0?
3. **Fix** the real cause, add a unit test where the logic allows one, and show the dwell-2 run keeping its shadows.

## Checks

- `cargo build --profile quick --bin engine`; the repro run above before and after the fix (images); the short tour `--demo riverwood --walk --demo-tour local/t185-short --tour-doors 2` still ends `tour PASSED after 2 crossings (short tour)`. Read only `local/t185*`, `local/captures/2026-09-25_02-01-35/` and the `local/agents/` files named above under `local/`.

## Report

The reproduction (option, threshold, image names); the cause with the run evidence; the fix and its files; tests; the before/after images; the tour's last line.
