# analysis-1106-portal-render-set-systems (track: Portal; B2). Read-only.

## Question
In the portal-heavy steady state (window 2026-10-09 04:34, local/review/portal-1093-window-2026-10-09/), the root
RenderGraph schedule's Render set (lap render/graph/views) takes 5.7 ms per frame, of which the three camera runs
(Core3d schedules run by camera_driver) take only 1.2 ms; 4.5 ms (render/graph/views_remainder) is outside every
camera run. What runs there?

## Work
1. List every system in RenderGraphSystems::Render of the RenderGraph schedule in Bevy 0.19.0 (sources:
   ~/.cargo/registry/src/index.crates.io-*/bevy_render-0.19.0, bevy_core_pipeline-0.19.0, bevy_pbr-0.19.0,
   bevy_anti_alias-0.19.0, bevy_post_process-0.19.0, any other bevy_* 0.19.0 crate in the lockfile
   C:/Modding/Mudcrab/mudcrab/Cargo.lock) and in our engine (C:/Modding/Mudcrab/mudcrab/crates/engine/src: grep
   RenderGraph, RenderGraphSystems, add_systems(RenderGraph). For each: file:line, what it does, whether it runs
   per frame, per view or once, and its ordering constraints.
2. camera_driver (bevy_core_pipeline src/schedule.rs ~123-206): what it does per view besides world.run_schedule
   (CurrentView insert/remove, RenderContext creation, command encoder creation and finish, command buffer
   collection, schedule lookups), and anything that could cost milliseconds per view (e.g. command encoder
   finish, ViewTarget work, a per-view world scan). Cite lines.
3. Which RootNonCameraView or auxiliary views exist in a portal run (shadow views were 0 in the window), and
   whether our water-reflection camera and the portal camera each add a full camera_driver run.
4. Rank the likely owners of the 4.5 ms with reasons, and say exactly where a lap would split them (system
   names to order against).
Under 700 words, file:line for every claim. No web sources needed.
