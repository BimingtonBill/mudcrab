# Goal
Find the state, in October 2026, of (a) OpenXR PC VR on Bevy/wgpu and (b) local-LLM NPC dialogue from a Rust game engine. Output feeds Phase 5 item 5.4 of Mudcrab's plan.

# Context
- Mudcrab: open-source (MIT/Apache) Skyrim engine in Rust on Bevy 0.19.0, wgpu 29, Windows first (DX12 default, Vulkan available). Desktop goal: stable 160 fps; VR would need two eyes at 90+ Hz.
- Scripting is Luau via mlua 0.10. Roadmap 5.4: "Optional Luau bindings to local LLMs (e.g. Ollama) for dynamic, unscripted NPC dialogue. Native OpenXR stereo rendering and motion controller bindings for PCVR." Optional features must be switchable off and never become something the core depends on.

# Scope
Web research only: GitHub repos, docs.rs, Khronos docs, Bevy issues/PRs, Ollama/llama.cpp docs, posts about LLM NPC mods (e.g. Mantella, Herika for Skyrim) for design lessons only (licences may differ; no code copying).

# Questions (answer each, with sources)
1. OpenXR on Bevy: bevy_mod_openxr / bevy_oxr / any successor. Latest version, Bevy version supported, licence, backends (Vulkan only? D3D12?), multiview/single-pass stereo support in wgpu 29, controller input, known performance limits. What does Bevy upstream plan?
2. Stereo cost: what a renderer must change for VR (two views, foveated rendering, reprojection, MSAA vs TAA in VR, shadow sharing between eyes). Typical frame budgets.
3. Local LLM from Rust: options (Ollama HTTP API, llama.cpp bindings like llama-cpp-2, candle, mistral.rs), latency for a 7-8B model on an RTX 3090 while the GPU also renders a game, VRAM contention, streaming tokens.
4. Design lessons from Skyrim LLM mods (Mantella, Herika, CHIM): what context they feed (NPC bio, location, quest state), speech/TTS pipeline, latency tricks, failure modes (lore breaks, quest-breaking promises), and how they keep scripted quests safe.
5. A safe Luau API shape: async calls from Luau into an LLM without blocking the frame; sandboxing; making it optional.

# Done when
Each question answered with primary sources; a short "ready / blocked / effort S-M-L" verdict for VR and for LLM NPCs; UNVERIFIED on anything not confirmed.

# Report
Your final answer is the report: verdicts first, then answers with sources inline.
