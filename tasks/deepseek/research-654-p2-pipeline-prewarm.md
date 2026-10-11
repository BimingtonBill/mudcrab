# Goal
Find how a Bevy 0.19 engine can build its render pipelines before they are first needed, so that loading a new area does not stall the render thread for 60-100 ms while pipelines compile. Read-only research with citations; the result decides the next change.

# Context
- Engine: `D:/Mudcrab/worktrees/p2-pacing` (Rust, Bevy 0.19.1 with default features, Windows, default wgpu backend DX12; Vulkan also used). Rendering code: `crates/engine/src/render.rs` (materials, terrain material extension, water, sky), `crates/engine/src/streaming.rs` (spawns converted glTF scenes and terrain), `crates/engine/src/render_timing.rs` (now samples `PipelineCache` per render frame).
- Measured tonight: on a load or a jump to a new cell, render-thread frames of 99 ms (21 pipelines finished that frame) and 61 ms (41 pipelines queued); 83 pipelines over a run. Mean render-thread time 9.4 ms on frames with pipeline activity vs 3.3 ms without.
- Bevy sources are in `C:/Users/Brannock/.cargo/registry/src/*/bevy_render-0.19.1/` (and bevy_pbr-0.19.1, bevy_core_pipeline, wgpu-*).

# Web questions
- Bevy 0.15-0.19: pipeline precompilation / warming. Is there `RenderPlugin { synchronous_pipeline_compilation }` or similar, and on which platforms is pipeline creation asynchronous (task pool) vs blocking on the render thread? Known issues or discussions about pipeline-compile hitches (e.g. bevy issue 10696) and recommended workarounds (warm-up scenes, pipeline cache to disk, `wgpu::PipelineCache`).
- wgpu on DX12/Vulkan: does wgpu 26+ (what Bevy 0.19 uses) support a persistent pipeline cache (`wgpu::PipelineCache`, `Features::PIPELINE_CACHE`) on Windows backends, and does Bevy expose it?

# Questions (answer from the Bevy source, with file:line)
1. In bevy_render 0.19.1, how does `PipelineCache` create render pipelines: synchronously on the render thread, or as tasks on a thread pool? Under which cfg/features? If async, why could the render thread still stall for 99 ms (shader module creation, `create_render_pipeline` on the device, waiting)?
2. What decides which pipelines exist: specialization keys (mesh vertex layout, material key, MSAA, HDR, shadow/prepass variants, view keys). For the engine's materials (StandardMaterial with the engine's alpha modes, the terrain ExtendedMaterial, water), which key components vary between models, so how many distinct pipelines a world can need.
3. Practical ways to build them ahead: (a) spawn one hidden/off-screen instance of each material+mesh-layout combination at startup and wait until the pipeline cache has no waiting pipelines; (b) queue specializations directly; (c) enable a wgpu pipeline cache to disk; (d) reduce variants (e.g. normalise vertex layouts or material keys at conversion). For each: what the engine would need to know (where to get the set of combinations: e.g. the converted assets' material list), cost, and risks.
4. Can a render frame be kept short while pipelines compile (e.g. drawing without the not-yet-ready pipeline, which Bevy does when a pipeline is not ready), and why the measured frames still took 60-100 ms.

# Scope
Read only: Bevy/wgpu sources, the engine source, the web. Change nothing.

# Report
Answers per question with file:line (sources) or links (web), then a recommended approach with the expected gain and the smallest first experiment to prove it (something measurable with the render_pipelines report fields). Under 1,200 words.
