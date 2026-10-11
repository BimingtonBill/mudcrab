# Goal
First slice of the engine performance baseline (upstream issue #184, Phase 2 track): every benchmark report records a run fingerprint, per-render-pass GPU times, and GPU-side memory counters, so runs can be compared and frame time can be split by pass.

# Context
- Worktree (already made, branch phase2/perf-baseline): D:/Mudcrab/worktrees/p2-perf. It is upstream main plus PR #122 (the pacing measurement tools, head 2433ef4f). Build on #122's code; do not create a parallel measuring system.
- Design study to follow (read it first; its file:line references were taken from a DIFFERENT checkout, so re-locate everything in this worktree before relying on it, and its schema numbers 100_401 / 101_601 are NOT this branch's values; read the real constants here): C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/research-999-p2-perf-baseline-design-report.md
- Relevant code here: crates/engine/src/metrics.rs (`BenchmarkReport`, `AcceptanceMetricsPlugin`, the report `format_version`), crates/engine/src/profiling.rs (the profile bundle: metadata.json, gpu-passes, memory; `sample_frame` keeps only `render/` diagnostic paths), crates/engine/src/app.rs (`RenderDiagnosticsPlugin` is added there), crates/engine/src/graphics_settings.rs (`GraphicsSettings` is Serialize), crates/shared/src/lib.rs (world DB and cell cache schema constants), docs/roadmap/02-profiling.md (documents every report field).
- Bevy 0.19.0 / wgpu 29. Read Bevy sources in the cargo registry (`~/.cargo/registry/src/*/bevy_render-0.19.0/`, `bevy_pbr-0.19.0/` etc.) when you need an API.

# Scope (owned files: you may change only these)
crates/engine/src/fingerprint.rs (new), crates/engine/src/lib.rs, crates/engine/src/metrics.rs, crates/engine/src/profiling.rs, crates/engine/src/app.rs, docs/roadmap/02-profiling.md. No new crate dependencies (if one is truly needed, stop and report instead).

# Work (three items)
1. **Run fingerprint.** New `fingerprint.rs` with a `RunFingerprint` built once at startup in benchmark/profile runs: commit and dirty flag (reuse the existing `--profile-commit` / `--profile-dirty-worktree` values), graphics backend (Vulkan/DX12/...), adapter name, driver info, device type (from Bevy's `RenderAdapterInfo`), window resolution actually used, build profile (debug/release via cfg), world DB schema and cell cache version (from shared's constants and the opened world DB if available), and the EFFECTIVE `GraphicsSettings`. Put it in `BenchmarkReport` (new `fingerprint` field) and in the profile bundle's metadata.json (replace fields it duplicates there). Bump the report `format_version` by one from what this branch has.
2. **GPU time per render pass.** Bevy's `RenderDiagnosticsPlugin` already produces `render/<pass>/elapsed_gpu` diagnostics. Add to `BenchmarkReport` a `gpu_passes` section: per pass path, mean / p95 / worst / total ms over the measured frames, plus `timestamp_queries_supported` and, when timestamps are unavailable on this backend, an `unavailable` reason instead of zeros. Shadow cascades: find out from Bevy's source whether cascades get their own pass names (e.g. per light/cascade view labels); record whatever paths exist and state in the docs what a path corresponds to. Several cameras can share a pass path: keep the path as Bevy names it and say so in the docs.
3. **GPU memory counters.** Register Bevy's `MeshAllocatorDiagnosticPlugin` and `RenderAssetDiagnosticPlugin` for meshes, images and the engine's material type(s) ONLY in benchmark/profile runs (not in normal play), sample their diagnostics (they do not start with `render/`, so extend `sample_frame` deliberately), and add to the report a `memory.gpu` section: mesh allocator slab bytes / slab count / allocations, and live GPU asset counts per type. Label counts as counts, never as VRAM bytes. Also add CPU-side asset bytes if cheap (sum of `Mesh` attribute+index bytes and `Image` data lengths), sampled at the end of the run, not every frame.

# Rules
- Never use `git stash` (the stash list is shared by all worktrees). Path-only commits (`git add <paths>`), no amend/rebase/push.
- Before every build run `df -h /d`; below 15 GB free, stop and report. Always `export CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-pacing`.
- Normal play (no benchmark/profile flags) must add no new systems or plugins; test that.
- Commits: upstream conventional style, e.g. `feat(engine): record a run fingerprint in benchmark reports`, one commit per item, each ending with the line `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`.
- Docs: update docs/roadmap/02-profiling.md for every new field.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run -p engine --all-targets` pass (the test `streaming_fixture_loads_its_interior_by_id...` is known to be load-sensitive; if it alone fails, rerun it alone and report both).
- New unit tests: fingerprint serialisation; the gpu_passes statistics from a fed sequence of diagnostic samples (including the "timestamps unavailable" case); the memory section from fed samples; normal play installs none of the new plugins.

# Report
Status; commits (sha + subject); per item what was done and where (file:line); the exact JSON shape of the new sections (one example); what you could not do and why; test counts.
