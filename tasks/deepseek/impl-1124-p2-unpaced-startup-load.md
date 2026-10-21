# Goal
Load the world faster at startup: from the first frame until the first `world_ready` latch, run with streaming pacing off (no render-asset upload budget, model spawn cap 0 = unlimited, unload pacing off), then restore the configured values. Keep the screen black (or faded) until ready, the way a door crossing does, so the burst frames are never seen. Track: Phase 2 (portal-based code; asked by the user via Team Lead). Target: load time, not 160 fps.

# Context
- Prior art in this codebase: crates/engine/src/door_crossing.rs lifts `RenderAssetBytesPerFrame` to `None` during the black fade and restores it after (`set_upload_budget`, around lines 554-600 and its callers ~667/702/767). Reuse that mechanism; do not build a second one.
- Startup today has no such lift: pacing applies from frame 1. On portal-heavy, world_ready takes 342 frames / 5.1-7.2 s (u1-check window, 2026-10-09).
- The pacing knobs live in crates/engine/src/config.rs (`max_model_spawns_per_frame` default 4, 0 = all at once; `--max-unload-ms-per-frame` default 3.0) and crates/engine/src/pacing.rs / streaming.rs; the `world_ready` latch is in pacing.rs/metrics.rs (find it). `--shots` and demo-tour already run with no budget: nothing changes for them.
- Research: local/research/engine-foundations/analysis-1103-window-read-2026-10-09.md (cap 0 vs 8: cap 8 doubles frames_to_world_ready), local/research/pacing-prior-art.md, local/research/engine-foundations/analysis-1055-terrain-missing-cells.md (the upload budget defers GPU prepare).
- Bevy 0.19.0 (stay on it). Say whether Bevy 0.20 changes `RenderAssetBytesPerFrame` if you rely on it beyond what door_crossing already does (one line; no web search needed if unsure, say "unchecked").

# Foundations check (answer each in the report)
1. No shortcuts: the lift is restored exactly at the first world_ready, and the configured values are what run afterwards. Keeping the screen black until ready is a load screen, the same as a door crossing: say how it differs from "hiding loading" (it shortens the load, it does not hide a stall in play). Never skip the restore on any path (ready reached, a timeout, an exit).
2. Optimised: measured before/after with the project's own profiling (time_to_world_ready_ms, frames_to_world_ready) and peak commit.
3. Flexible: no worldspace, cell or route knowledge; works for any start position.
4. Moddable: a flag to turn it off (e.g. `--paced-startup`), default on; nothing else depends on it.
5. Aligned: frames after ready unchanged.

# Steps
1. Implement in its own small system/plugin piece next to the crossing's budget code. Latch: the first world_ready only; a later loss of ready must not re-lift.
2. Unit tests: lifted before ready, restored on the ready frame to the configured values (budget, spawn cap, unload ms), not re-lifted later, off with the flag.
3. Checks, all gated: `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py run -- cargo clippy -p engine --all-targets --all-features -- -D warnings`, `... run -- cargo nextest run -p engine --all-targets`, `cargo fmt --all -- --check`. Check `df -h /d` before each build; below 15 GB free, stop and report.
4. Measure (release build: `... run -- cargo build --release --bin engine`): portal-heavy acceptance, the same command the profiling scripts use (scripts/phase2-*.ps1; find the portal-heavy route), before (flag `--paced-startup`, i.e. old behaviour) and after, x3 interleaved (before, after, before, after, before, after), after one discarded warm-up run. Before every engine run, `python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py status`; if any hold is listed, wait and recheck every few minutes. Report per run: time_to_world_ready_ms, frames_to_world_ready, peak Windows commit during the startup burst (sample with a PowerShell `Get-Counter '\Memory\Committed Bytes'` loop at 250 ms, or the engine's own memory column if it has one), and p50/p95/worst frame ms over the frames after ready.

# Scope
Owned files: crates/engine/src/door_crossing.rs, crates/engine/src/pacing.rs, crates/engine/src/config.rs, crates/engine/src/app.rs, crates/engine/src/streaming.rs (only if the spawn cap or unload knob must be read/written there). No shared-crate, converter or Cargo changes. Commit on your branch in upstream's style (`feat(engine): ...`).

# Done when
The lift and restore work with tests, checks are clean, and the before/after table (x3 each) is in the report.

# Report
Status / Summary / Left / Next, the per-run table, the foundations answers, the commit, and any frame after ready that got worse.
