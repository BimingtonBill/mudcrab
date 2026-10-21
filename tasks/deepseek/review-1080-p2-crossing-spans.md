# Goal
Review impl-1077: profiler spans that will decide which work owns the door-crossing stalls. The question is whether each span measures exactly what its name says. A span that measures the wrong thing would send the next fix after the wrong owner. Decide accept or fix first. Track: Phase 2.

# Context
- Change: commit dac5f4f2 on branch `phase2/crossing-spans`, in the worktree C:/Modding/Mudcrab/mudcrab/local/impl/impl-1077-p2-crossing-spans. Review it with `git show dac5f4f2`. Files: crates/engine/src/profiling.rs and crates/engine/src/streaming.rs.
- Brief: C:/Modding/Mudcrab/mudcrab/tasks/deepseek/impl-1077-p2-crossing-spans.md. Why it matters: C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/analysis-1074-despawn-cost.md.
- Bevy 0.19 sources are in the cargo registry: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ (bevy_asset-0.19.0, bevy_ecs-0.19.x, bevy_render-0.19.x).

# Scope
Read only; git read commands only; no cargo.

# Done when
For each span, say what it really brackets, with file:line in our code and in Bevy:
1. `streaming/space_change_unload` (in `plan_cells`). Does it time only the queueing of `try_despawn` commands, plus the subtree count? It says so. Is the subtree count's cost inside the bracket, and does that inflate it? Is the root count right for each `CellStatus` arm, so a `replaced` root of a Loading or Failed cell is counted once?
2. `streaming/space_change_unload_apply`: an explicit `ApplyDeferred` chained after `plan_cells`, with `stamp` before it and `record` after it.
   - Does that ApplyDeferred apply ONLY plan_cells' commands, or every pending command buffer in Update up to that point (other systems' spawns and inserts)? Then the span would include unrelated work.
   - Does Bevy's automatic sync-point insertion add another ApplyDeferred that runs first and applies the despawns before the stamp, so the span measures nothing? Use `after_ignore_deferred`.
   - Does adding this sync point change when other systems see the despawns in a profiling run, against an ordinary run? Is that change a confound for the A/B?
3. `assets/load_finalize` around `handle_internal_asset_events`: are the begin and end markers truly adjacent to that one system, or can other PreUpdate systems run between them in parallel and land inside the bracket? With a multithreaded executor, a wall-clock bracket around one system can include concurrent systems' time, and the system itself may run on another thread. Say how far this can be trusted.
4. `assets/handle_drop` around `AssetTrackingSystems`: the same question. Does that set hold only `track_assets`, the handle-drop processing, for every asset type? Is the despawn's handle-drop cost really processed there and in that frame, or later, for example through the `drop` channel drained in `handle_internal_asset_events`? That matters most: if drops are processed inside handle_internal_asset_events, then `load_finalize` would include them and the split would be wrong.
5. `render/extract_commands_apply` around `RenderSystems::ExtractCommands`: is that set where the render world applies the extracted entity removals? Is the one-frame-late bridge read into the right frame series row (render-cycles.csv: cycle n joins frame-series row n-1)?
6. Double counting: does any span overlap another, or overlap an existing span such as `streaming/plan_cells`, in a way the readers should know about?
7. Zero cost when profiling is off: no systems added, no resources, and the `plan_cells` branch is a bool check.

# Foundations check (answer explicitly)
Measurement only, with no behaviour change in ordinary runs? Anything hard-coded?

# Report
Status / Summary / Left / Next. Then the verdict, then per span: what it measures, whether its name is accurate, and any fix (`path:line - issue - fix`). Then how to read them together in the window, for example whether load_finalize plus handle_drop should be compared with the PreUpdate span.
