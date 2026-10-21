# Goal
Find the best working tools and methods to capture GPU frames and rendering internals from The Elder Scrolls V: Skyrim Special Edition / Anniversary Edition (DirectX 11, 64-bit, 1.6.x), for studying how the game renders a frame.

# Context
- Hobby project re-implementing Skyrim's engine; we own a legitimate Skyrim SE install and want to study its output (draw calls, textures, meshes, shaders, render passes, depth, G-buffer-like targets, shadow maps, water, lighting) for comparison with our own renderer.
- Interested in: RenderDoc on Skyrim SE (does it launch? SKSE/anti-debug/overlay problems, version notes), NVIDIA Nsight Graphics, Microsoft PIX, AMD RGP/Radeon GPU Profiler, ReShade add-on API (frame/depth/render-target capture, "RenderDoc" style add-ons), apitrace, dx11 wrappers, DXVK-based capture, mesh/texture export from captures, replaying captures.
- Also known community tools (Community Shaders, ENB, ENB Helper) and any shader-dump or shader-source references for Skyrim SE.
- Not interested in cheating tools. Only tools that work on a normal single-player install.

# Done when
- For each tool: does it work with Skyrim SE/AE (which game version), exact setup steps or flags, known problems and workarounds, licence and download URL, and what you can extract. Two independent sources per claim where possible.

# Report
- Ranked shortlist of 5-8 methods with a one-line recommendation each, then details per tool with URLs, source count, confidence. Say plainly what you could not verify. Note where sources disagree.
