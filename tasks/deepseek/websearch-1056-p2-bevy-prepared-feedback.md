# Goal
Find out whether Bevy (0.19, 0.20 release candidates, or main / open PRs) has a built-in way for the main world to know that a render asset (a Mesh, an Image, a Material's bind group) has been prepared on the GPU, so gameplay code can swap a placeholder only once the real asset is actually drawable.

# Context
Mudcrab (a Rust/Bevy 0.19 engine) streams terrain under `RenderAssetBytesPerFrame` (a per-frame upload byte budget), which defers `prepare_assets` by frames. Our LOD tiles hide when the CPU-side assets are loaded, which can be before the GPU prepare, leaving a hole. We need a render-world to main-world "prepared" signal. Look for: events or resources like RenderAssetPrepared / AssetPrepared, `RenderAssets<T>` readback, `MainWorld` access from render systems, `extract` in the reverse direction, `ExtractResource` feedback, the "prepared" set used by `bevy_render` for pipelines (`PipelineCache` readiness), mesh allocator readiness, any work on "asset readiness" or "render asset feedback", and how others solved it (bevy discussions, issues, community crates).

# Scope
Web search only. Change no files.

# Done when
For each candidate: what it is, which Bevy version has it (0.19 / 0.20-rc / main / open PR #N / not at all), a link, and whether it gives per-asset prepared state that main-world systems can read. End with "in 0.20: yes / PR #N / no" and the cleanest approach on 0.19 if there is no built-in.

# Report
Status / Summary / Left / Next, then the candidate list with links, then the recommendation. Cite every claim with a URL; say what you could not confirm.
