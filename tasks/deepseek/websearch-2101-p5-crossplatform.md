# Goal
Find what it takes, in October 2026, to ship a Bevy 0.19 (wgpu 29) open-world engine on Linux (incl. Steam Deck), macOS (Apple Silicon, Metal), Android ARM64 and the browser (WebAssembly + WebGPU). Output feeds a Phase 5 plan for Mudcrab.

# Context
- Mudcrab: an open-source (MIT/Apache) Skyrim engine in Rust on Bevy 0.19.0 and wgpu 29. Windows today, DX12 default, Vulkan in timing runs. Target: stable 160 fps on desktop.
- Uses: Bevy PBR, GPU culling + multi-draw indirect (Bevy 0.19 default), KTX2 textures with BC7 and Basis UASTC, zstd, a tokio multi-thread runtime for asset IO, mlua (Luau, with JIT) for scripting, big converted asset folders (about 25-60 GB) read from local disk.
- Roadmap targets: Android ARM64 APK (POCO F5 / Snapdragon 7+ Gen 2 class), Linux and macOS native Vulkan/Metal (Steam Deck, Apple Silicon), WebAssembly WebGPU client.

# Scope
Web research only: Bevy and wgpu repos, release notes, migration guides, docs.rs, issues/PRs, Bevy examples (mobile, wasm), Khronos/Apple/Google docs, known shipped Bevy games on those platforms.

# Questions (answer each, with sources)
1. Per platform (Linux, Steam Deck/Proton vs native, macOS Metal, Android Vulkan, Web WebGPU): is Bevy 0.19/wgpu 29 supported, and which features used above are missing or degraded there? Check especially: multi_draw_indirect and indirect count, storage buffers in vertex shaders, bindless/binding arrays, compute, BC7 vs ASTC/ETC2 texture support, max texture/array limits, 32-bit index buffers.
2. Texture formats: BC7 is absent on most Android GPUs and on some WebGPU devices. What does Bevy 0.19 do with Basis UASTC KTX2 on each platform (transcode targets), and what do shipping engines do (ship ASTC, transcode at load, per-platform asset packs)?
3. Threads and IO: tokio multi-thread and std::fs on wasm32 (WebGPU), wasm threads status (SharedArrayBuffer, COOP/COEP), Bevy's task pools on web; Android asset access (APK assets vs external storage for tens of GB); mlua Luau with JIT on Android ARM64, iOS-style JIT bans (macOS fine?) and on wasm.
4. Build and packaging: cargo-apk / xbuild / cargo-mobile2 status for Bevy 0.19, Android NDK versions, macOS signing/notarisation basics, Linux packaging (Flatpak, AppImage, Steam runtime), wasm-bindgen/trunk sizes. CI runners for each (GitHub Actions macOS arm64, Android emulator GPU limits).
5. Performance reality: published numbers or reports for Bevy 3D scenes on Steam Deck, M-series Macs, Snapdragon 8 Gen 2 class phones, and Chrome WebGPU. What draw counts and memory budgets are realistic on mobile and web?
6. Which wgpu backend choices matter for one codebase (Vulkan everywhere except Metal on Apple, WebGPU on web, GL fallback?), and what Bevy 0.20 changes for any of this.

# Done when
Every question answered with at least one primary source; a per-platform table (supported / blockers / effort S-M-L / first step); claims you could not confirm marked UNVERIFIED.

# Report
Your final answer is the report: the table first, then the answers with sources inline, then a ranked list of the 5 first steps for Mudcrab.
