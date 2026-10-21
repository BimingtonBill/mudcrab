# Goal
For the virtualized-geometry lane (queue row VG4), give a per-backend answer on what our pinned wgpu supports for GPU-driven drawing: `draw_indirect` / `draw_indexed_indirect`, `multi_draw_indirect`, and `multi_draw_indirect_count` (the count-buffer variant), on Vulkan, DX12 and Metal (and say briefly for GL and WebGPU). Read only. Track: Phase 2.

# Context
- Our lockfile pins wgpu, wgpu-core, wgpu-hal and wgpu-types at **29.0.4** (the queue row says 29.0.3; use 29.0.4 and say if anything differs from 29.0.3 in the changelog). Sources: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/wgpu-29.0.4, wgpu-hal-29.0.4, wgpu-types-29.0.4, wgpu-core-29.0.4. wgpu-30.0.1 is also there: note any relevant change in 30, in one line each.
- Read first, and do not repeat what is settled there (confirm or correct it):
  - local/agents/runs/websearch-1071-fun-vg-gpu-runtime.1/report.md (says Metal lacks indirect count);
  - local/research/fun/virtualized-geometry-plan.md;
  - local/research/engine-foundations/websearch-993-bevy-gpu-driven-report.md;
  - local/research/engine-foundations/analysis-1035-static-churn-indirect.md.
- The target machine is Windows with an RTX 3090 (DX12 default backend, Vulkan available).

# Questions (answer each with file:line in the wgpu sources)
1. Which `wgpu::Features` / `DownlevelFlags` gate each call (`INDIRECT_FIRST_INSTANCE`, `MULTI_DRAW_INDIRECT_COUNT`, any multi-draw flag; is plain multi_draw_indirect now core?), and where each backend in wgpu-hal sets them (vulkan/adapter.rs, dx12/adapter.rs, metal/adapter.rs, gles/adapter.rs).
2. Per backend, how the hal implements multi-draw: a native call (vkCmdDrawIndexedIndirect with drawCount, ExecuteIndirect) or a CPU loop of single draws. A loop matters for cost at our scale.
3. The count variant: native on which backends; any limits (max draw count, buffer alignment, `first_instance` rules).
4. Limits relevant to a GPU cull feeding indirect args: max indirect buffer size, storage-buffer limits for the args buffer, `max_storage_buffers_per_shader_stage`, and whether indirect args may be written by a compute pass in the same submission without extra barriers (wgpu tracks usage; say what it does).
5. What Bevy 0.19.1 already uses (bevy_render / bevy_pbr): does it call multi_draw_indirect_count on DX12 and Vulkan today, and what does it fall back to without the feature? Cite the code.
6. Known bugs or open issues in 29.x for these paths on DX12 or Vulkan, if the changelog or code comments mention any. Web lookups are allowed for this question only; cite URLs and mark them web-sourced.

# Scope
Read only. No cargo, no builds, no engine runs. Write the note to local/research/engine-foundations/research-1101-wgpu-indirect-support.md and add one row to local/research/INDEX.md in the existing format.

# Report
Status / Summary / Left / Next, then a per-backend table (call x backend: native / emulated / unsupported, with the gating feature), then what it means for the VG plan in 3-5 lines.
