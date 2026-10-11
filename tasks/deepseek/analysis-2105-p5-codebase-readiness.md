# Goal
Map how ready the Mudcrab codebase is for each Phase 5 roadmap item (docs/roadmap/05-multiplatform-and-networking.md): what exists, what blocks it, and where the work would land. Read-only. Output feeds the Phase 5 plan.

# Context
- Workspace: crates/converter, crates/engine, crates/launcher, crates/shared, crates/dummy-content. Bevy 0.19.0, wgpu 29, mlua 0.10 (Luau), tokio. Branch in this checkout is `portal` (the user's demo branch; it carries main plus extras). Note when something exists only on portal-specific code (portal, room graph, door crossing) versus code that came from main.
- Phase 5 items: 5.1 RT GI/AO/shadows/reflections + upscaling; 5.2 Linux/macOS/Android/WASM; 5.3 co-op replication over WebSockets/UDP; 5.4 Luau LLM bindings + OpenXR PCVR.
- Project rules relevant here: stable IDs, per-cell change records, MP-ready scripting, optional features switchable off.

# Scope
Read source, Cargo.toml files, docs/, scripts/ and README. Do not edit anything, do not build. Cite file:line for every claim.

# Questions
1. Platform coupling (5.2): every Windows-only or platform-specific piece: cfg(windows)/cfg(target_os), registry access, hard-coded drive letters or path separators, DX12 default and where the backend is chosen, wgpu features requested (multi_draw_indirect, bindless, texture compression BC), native-only deps (tokio rt-multi-thread, std::fs, threads, mmap) that block wasm32, memory assumptions. Existing Linux scripts (scripts/run-riverwood.sh etc.) and whether CI builds anything but Windows.
2. Texture/asset formats (5.2): what the converter writes (BC7? UASTC? KTX2 + zstd?), whether the engine can load an alternative format, and where a per-platform asset variant would plug in.
3. Rendering hooks (5.1): custom render graph nodes, the main camera setup (HDR, MSAA vs TAA, Bevy anti-aliasing), lighting and shadow code, existing AO/SSR/bloom/TAA use, any ray-tracing or Solari references, render-resolution control. Where would an upscaler pass or Solari be inserted?
4. Networking readiness (5.3): stable object IDs (FormID mapping, any global ID type in crates/shared), change records or save/state code, events, anything deterministic, any existing net code or crates. How are runtime entities linked back to source records?
5. Scripting (5.4): what the Luau/mlua integration does today (if anything), its async model, sandboxing; any HTTP client in deps. Camera/input code a VR rig would replace.
6. For each item 5.1-5.4: a list of the files that would change and the likely owners (converter = Phase 1, engine = Phase 2/Portal, launcher = Phase 3), and the smallest first slice.

# Done when
Every question answered with file:line citations; a per-item table (exists / blockers / files / first slice); counts double-checked with Grep.

# Report
Your final answer is the report: the per-item table first, then answers with citations.
