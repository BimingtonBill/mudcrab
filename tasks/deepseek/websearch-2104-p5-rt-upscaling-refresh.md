# Goal
Update two-week-old research on ray tracing and upscaling for Bevy/wgpu: what changed between 2026-09-26 and today (October 2026)? Output feeds Phase 5 item 5.1 of Mudcrab's plan.

# Context
- Mudcrab: open-source Skyrim engine in Rust on Bevy 0.19.0, wgpu 29, RTX 3090, DX12 default, Vulkan in timing runs. The frame is render-thread-CPU-bound today, so upscaling buys looks, not speed.
- Findings as of 2026-09-26/10-07 (only report what changed or was wrong):
  - dlss_wgpu 6.0.0 is Vulkan-only; Bevy 0.19 pins 4.0.0; frame generation not exposed.
  - FSR4 is RDNA4-only; wgpu-ffx 0.1.0 (FSR 3 port) targets wgpu 29. XeSS and DirectSR unreachable from wgpu.
  - Bevy has no dynamic resolution scaling ("not currently planned").
  - bevy_solari is Bevy's only path tracer (8-15 ms GPU), effectively NVIDIA-only via DLSS-RR, no transparency or skinned meshes; point lights merged on main (PR #25913, milestone 0.21). Bevy 0.20 was at rc.2 on 2026-10-07.
  - wgpu ray-query / acceleration-structure support status on DX12, Vulkan, Metal.

# Scope
Web research only: Bevy and wgpu repos, releases, PRs, docs.rs, crate changelogs (dlss_wgpu, wgpu-ffx, bevy_solari), vendor SDK pages.

# Questions
1. Has Bevy 0.20 shipped? What in it touches ray tracing, Solari, DLSS, upscaling, TAA, dynamic resolution? Its wgpu version.
2. New versions of dlss_wgpu, wgpu-ffx or any other wgpu upscaler crate (FSR, XeSS, TAAU, SGSR/Snapdragon for mobile); which Bevy/wgpu versions they need; DX12 support.
3. wgpu ray tracing: ray query and acceleration structures per backend (DX12, Vulkan, Metal), hardware ray tracing on AMD/Intel through wgpu, any non-NVIDIA Solari denoiser.
4. Cheaper non-RT options Bevy now has or is adding for 5.1's goals (SSGI, VBAO/SSAO, contact shadows, SSR, probe GI).

# Done when
Each question answered with primary sources; a "changed since 2026-09-26" list at the top; UNVERIFIED on anything not confirmed.

# Report
Your final answer is the report: the change list first, then answers with sources inline.
