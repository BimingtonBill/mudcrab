# Goal
Explain why despawning one exterior cell costs about 0.47 ms on the door-crossing path, and rank the fixes that make the work itself cheaper. Track: Phase 2 (engine streaming). The user's decision 2026-10-08: make despawn cheaper first; hiding cells and despawning them over many frames is only a fallback, if the work cannot fit a frame even when made cheaper.

# Context
- Measured: on a door crossing into an interior, the old worldspace's exterior cells (434 on the worst row) unload in ONE frame. `main_schedule/Update` worst is 204.5 / 144.8 / 136.4 ms (runs 1/2/3), so about 0.47 ms per cell. The re-profile evidence:
  - local/research/portal-crossing-owner-reprofile-2026-10-08.md (§3, §4, §7.1)
  - local/research/portal-crossing-render-thread-2026-10-08.md (the render-thread side: extract_commands_and_assets)
  - raw runs in D:/Mudcrab/evidence/reprofile-portal-2026-10-08/scenes-1/runs/*/portal-door-tour/*/run-{1,2,3}/: cpu-spans.json, frame-series.csv, frame-times.csv, streaming.json and tour/
- Which path does it: `plan_cells` unloads a space the camera left IMMEDIATELY. `paces_its_unload` (crates/engine/src/streaming.rs ~1429) is false for those cells; the test `a_crossing_into_an_interior_unloads_the_space_it_left_in_one_frame` (~7535) pins it. It is NOT `despawn_cells`: `retire_backlog_overflows` is 0 in all three tour runs.
- Code: the checkout at C:/Modding/Mudcrab/mudcrab (branch portal). Start from `plan_cells` and the despawn helper it calls (`despawn_subtree` or equivalent) in crates/engine/src/streaming.rs. The portal layer code is crates/engine/src/portal.rs (observers, `adopt_own_layer_node`, the RenderLayers propagation from #649). Physics is Avian. The vendored Bevy is under vendor/ (or the cargo registry); cite the exact version.
- impl-285's render-cycles.csv, where present in the run folders: cycle n joins frame-series row n-1 (a known one-row offset).

# Scope
Read only. Change nothing. Run no cargo and no engine.

# Done when
You have traced, with file:line, what ONE cell's despawn touches, and estimated each part's share of the 0.47 ms:
1. **Entities per cell.** How many entities one exterior cell holds: the root, terrain quadrants, water, references, glTF scene children (mesh, material and node entities), lights, colliders. Count them from the spawn code, and from the evidence if any counter gives it (resident entities, scene instances).
2. **Hierarchy despawn.** `despawn()` / `despawn_recursive` on a big tree in Bevy 0.19: per-entity cost, the ChildOf/Children bookkeeping, and whether despawning many roots one at a time is slower than a batch.
3. **Observers and hooks fired per entity on despawn or removal:** the portal layer observers, `adopt_own_layer_node`, RenderLayers propagation, any `On<Remove>`/`OnReplace` hooks, the visibility and transform systems, Avian's own hooks. Which of them are O(entities), and which do O(n) work per call (a query scan inside an observer is O(n²) over a cell)?
4. **Avian teardown:** collider and rigid-body removal per entity (the BVH or broadphase update, contact graph cleanup), and whether it is batched.
5. **Asset handle drops:** dropping strong handles to meshes, materials and images, and the asset events that follow.
6. **Render world:** the extract of removals (`extract_commands_and_assets`, the render-thread burst in the second note), and per-entity removal from render-world tables.
7. **Anything else** O(n) per entity or per cell on that frame, including streaming's own bookkeeping (metrics, maps, timeline records).
Then rank the fixes by expected gain, with evidence for each:
- batch despawn;
- removing or batching per-entity observer work;
- cheaper collider teardown;
- fewer entities per cell from the converter (a converter-side change: say what it would cost and save);
- anything else you find.
Say which of them alone or together would bring a 434-cell unload under one frame's budget (6.25 ms at 160 fps). Say if no realistic combination does, since that is the trigger for the hide-then-pace fallback.

# Foundations check (answer explicitly)
For each fix: is it a root-cause fix or a shortcut? What would break for a mod (more references per cell, heavier models) or another worldspace? Is anything hard-coded?

# Report
Status / Summary / Left / Next, then the per-part cost table with evidence, the ranked fix list, and what you could not verify (state UNVERIFIED plainly; mark measured versus estimated). Also write the findings to local/research/engine-foundations/analysis-1074-despawn-cost.md.
