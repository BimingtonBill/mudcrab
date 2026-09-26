# Goal
Track: Phase 2 (for upstream). The water reflection camera draws what Skyrim's water reflects
(the land, and later distant LOD and the sky) instead of redrawing every placed object. Measured on
main at the rural cell: the reflection pass costs 1.72 ms of a 4.41 ms frame (219 fps with it,
366 without), because it redraws the whole scene, every pass and shadow included.

# Context
- Your checkout is cut from `main` (upstream). Upstream commit style (`perf(engine): ...`), no
  `[deepseek]` prefix, no project-internal references (no `local/`, task names, or "portal").
- What vanilla Skyrim SE reflects (cited research, summarised): its water reflection is built from
  the LOD world - LOD land, LOD objects, LOD trees - plus the sky, switched by the Skyrim.ini
  `[Water]` settings `bReflectLODLand`, `bReflectLODObjects`, `bReflectLODTrees`, `bReflectSky`
  (sources: DynDOLOD "How LOD Works" and "Terrain LOD and Water LOD" help pages; STEP's Skyrim.ini
  [Water] guide; modder reports that "the water only reflects distant LODs"). Full-detail placed
  objects, actors and grass are not in it; SE adds screen-space reflections for near things, which
  this engine does not have. Reflection size defaults to 512x512 (`iWaterReflectWidth/Height`).
- This engine (main) has no LOD and no sky yet, so today "what Skyrim reflects" is the terrain.
- Render layers today: layer 0 is the default (terrain, placed objects, lights), layer 1 is water
  (`RenderLayers::layer(1)` on water surfaces in `crates/engine/src/streaming.rs` and the fixtures
  in `crates/engine/src/app.rs`). The main/streaming cameras render `[0, 1]`; the water reflection
  camera (`setup_water_reflection` in `crates/engine/src/render.rs`) renders `[0]`, so it sees
  everything but water.
- Placed objects are reference entities spawned in `spawn_cell` (`crates/engine/src/streaming.rs`);
  their meshes arrive as descendants (glTF scene instances), so a `RenderLayers` on the reference
  entity alone does not reach the meshes. Bevy 0.19 ships `bevy::app::HierarchyPropagatePlugin<C>`
  with the `Propagate<C>` component (source:
  `C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/bevy_app-0.19.0/src/propagate.rs`),
  which copies a component to every descendant, including ones spawned later; it is not registered
  for `RenderLayers` by default. Read that file to use it correctly (schedule, ordering: the layers
  must be in place before `VisibilitySystems::CheckVisibility` in `PostUpdate`).
- Lights and cameras match on layers too: a light only lights views whose layers intersect its own.
  Keep the sun, ambient and point lights on layer 0 so both cameras are lit.

# Design (decided)
- One named layer constant per role in one place (e.g. `crates/engine/src/render.rs`):
  `WORLD_LAYER = 0` (terrain, lights: drawn by both cameras), `WATER_LAYER = 1` (existing),
  `PLACED_OBJECT_LAYER = 2` (full-detail placed objects: main view only). Replace the literal
  `RenderLayers::layer(1)` / `from_layers(&[0, 1])` uses with the constants.
- Placed-object references get `Propagate(RenderLayers::layer(PLACED_OBJECT_LAYER))`; register
  `HierarchyPropagatePlugin::<RenderLayers>` once (in the renderer plugin). The main and streaming
  cameras render `[WORLD, WATER, PLACED_OBJECT]`. The reflection camera renders `[WORLD]` only.
  Terrain stays on the world layer. Point lights spawned under a reference (if any exist on your
  base) must stay on the world layer: check that `Propagate` does not move a light child onto the
  object layer, and if it would, exclude it (or say what you found).
- Keep the reflection texture size as it is (1024x576): the measured cost is per object, not per
  pixel; say in the report if you think otherwise.
- Document the rule where the reflection camera is set up: what it reflects, why (vanilla reflects
  LOD land/objects/trees and sky, not full-detail objects), and that LOD and sky join the world
  layer when they land.

# Scope
Owned files: `crates/engine/src/render.rs`, `crates/engine/src/streaming.rs`,
`crates/engine/src/app.rs` (camera layer lists and fixture layers only), and
`docs/handoff/impl-589-p2-reflection-content.md`. If another file needs a change, say so in the
report instead of editing it.

# Done when
- Tests: the reflection camera's layers exclude the placed-object layer and include the world
  layer; a spawned reference (from a constructed `ReferenceRow` through `spawn_cell`, or an entity
  with the propagate component and a child spawned after it) ends up with its mesh descendants on
  the placed-object layer after an app update; terrain stays on the world layer; the main camera
  still renders all three.
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy --workspace --all-targets --target-dir C:/Modding/shared-check-phase2 -- -D warnings`
  `cargo test -p engine --target-dir C:/Modding/shared-check-phase2`
- Do not run the engine binary or any benchmark (the lead measures).

# Report
Diff summary per file; how propagation is scheduled and why it runs before visibility; what
happens to lights and to fixtures; test names and results; anything a reviewer should look at.
