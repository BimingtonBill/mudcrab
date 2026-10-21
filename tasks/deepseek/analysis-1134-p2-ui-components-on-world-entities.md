# Goal
Find out why world entities (LOD chunk meshes and their parents) carry bevy_ui's computed components, so that despawning them fires bevy_ui's propagation observers. Track: Phase 2 (A4, part 2). Analysis only: no code changes.

# Context
- analysis-1130 (local/research/engine-foundations/analysis-1130-a14-crossing-trace-2026-10-09.md): despawning 158,292 LOD entities on a door crossing fired, per entity, 2 observers `bevy_app::propagate::on_r_removed<...ComputedUiTargetCamera...>` and `<...ComputedUiRenderTargetInfo...>` (316,584 runs), beside our portal observer and a visibility one.
- Bevy 0.19.0 source is in ~/.cargo/registry/src/*/bevy_ui-0.19.0 and bevy_app-0.19.0 (propagate.rs). Read how `Propagate<C>`/`PropagateOver`/`on_r_removed` are registered and when they fire: is it on every despawned entity that has the component, or every entity with a `Propagate*` marker, or any entity in a hierarchy?
- Our spawning code: crates/engine/src/streaming/lod.rs (LOD chunk roots and children), crates/engine/src/streaming.rs, crates/engine/src/world/ (scene spawn), and the glTF scene path (models are scenes spawned from .glb).
- If you need a live check, the engine has `--remote` (Bevy Remote Protocol) only in a `--features remote` build; prefer reading code. Do not run cargo or the engine.

# Questions
1. Do LOD entities actually have ComputedUiTargetCamera / ComputedUiRenderTargetInfo, and which system inserts them (file:line in bevy_ui 0.19)? E.g. a propagate system that inserts the computed component on every descendant of a camera-targeted root, or on every entity with a Node, or on all entities.
2. If Bevy inserts them on every entity regardless, say so: then the lever is Bevy-side (an upstream issue) or disabling a bevy_ui feature/plugin we do not need for world entities. Which bevy_ui plugins does the engine add (crates/engine/src/app.rs; DefaultPlugins with or without UiPlugin), and does the engine's UI (overlays, the fps counter, the fade) need them?
3. Give the cheapest correct fix and its layer, with the foundations check: no shortcut, works for any mod content.
4. Bevy 0.20: does it change this (the propagate observers or UI computed components)? Check bevy's 0.20 changelog or source if available locally; else "unchecked".

# Report
Status / Summary / Left / Next, answers 1-4 with file:line evidence. Write the note to local/research/engine-foundations/analysis-1134-ui-components-on-world-entities-2026-10-09.md and add one row to local/research/INDEX.md.
