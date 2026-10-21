# Goal
Fix how the #184 performance baseline samples GPU diagnostics (Phase 2 track), so the published numbers are honest: no warm-up frames, no repeated stale values, and no zeros for things never measured.

# Context
- Worktree D:/Mudcrab/worktrees/p2-perf, branch phase2/perf-baseline (head a3a8465d or later). Two reviews found these problems; read them: C:/Users/Brannock/.claude-deepseek/agents/runs/review-1004-deepseek.1/report.md and C:/Users/Brannock/.claude-deepseek/agents/runs/review-1004-sol.1/report.md (findings 3, 4, 5 of Sol's; the warm-up and reason-string findings of DeepSeek's).
- Code: crates/engine/src/profiling.rs (`sample_frame`, `gpu_passes`, `gpu_memory`, the `*_UNAVAILABLE` reason strings), crates/engine/src/metrics.rs (the report, where `sample_frame` is called before the warm-up gate), crates/engine/src/fingerprint.rs.
- Bevy 0.19 source: C:/Users/Brannock/.cargo/registry/src/*/bevy_diagnostic-0.19.*/ (`Diagnostic` keeps a history of `DiagnosticMeasurement { time, value }`; `value()` is only the latest) and bevy_render-0.19.*/src/diagnostic/ (how render diagnostics are written back, possibly several per path per frame from several cameras, and with readback latency; the mesh-allocator and render-asset plugins publish atomics in main-world PreUpdate from frame 1, zero before any extraction).

# Scope (owned files: only these)
crates/engine/src/profiling.rs, crates/engine/src/metrics.rs, crates/engine/src/fingerprint.rs. Do NOT edit docs/roadmap/02-profiling.md (another worker owns it now); put every doc sentence that must change in your report as exact old -> new text.

# Work (three items, one commit each)
1. **Every observation once.** For `render/*` GPU paths, take each NEW `DiagnosticMeasurement` exactly once (track the last measurement time seen per path), so several cameras' observations in one frame are all kept and a frame with no new readback adds nothing. If Bevy gives no way to tell cameras apart, say in a code comment (and in your report, for the docs) that a path's samples mix every camera that runs that pass. Tests: two measurements on one path in one frame -> two samples; an unchanged store -> no new sample; a pass that stops running -> no further samples.
2. **Measured window only.** The GPU-pass, pipeline-statistics and GPU-memory summaries cover only the measured window (after `benchmark_warmup_frames`), like `frame_ms_*`. Record each section's sample count in the report so readers see how many samples a number rests on. Test with a fed warm-up.
3. **Null until measured.** GPU-memory counters stay `null` with a reason until the render world has actually produced a value (e.g. the first non-default value after extraction, or a flag set by an extract/render-world system; pick what the Bevy source supports and explain it). A run with no renderer (`fingerprint.renderer` is null, e.g. headless) gets the reason "no renderer in this run", not "the backend does not support timestamp queries". Fix the reason strings DeepSeek flagged. Correct the glTF-loader sentence Sol flagged (profiling.rs ~604; give the docs ~99 change in your report: Bevy 0.19's default keeps meshes in both worlds unless the engine overrides it; check the engine and say which).

# Rules
- Never `git stash`. Path-only commits, no amend/rebase/push. Commit style `fix(engine): ...`, each ending `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`.
- Do not build or test before you see the line "TIMING RUN ENDED" in C:/Modding/Mudcrab/mudcrab/local/team/2026-10-06.md (a timing run holds the PC until about 17:05). Read and edit meanwhile.
- `df -h /d` before each build; below 10 GB stop and report. `export CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-pacing`.
- Another worker edits scripts/ in this same worktree at the same time: never touch scripts/, and `git add` only your owned files.

# Done when
fmt, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run -p engine --all-targets` pass; the new tests exist.

# Report
Status; commits; per item what and where (file:line); how cameras and readback latency are handled; test counts.
