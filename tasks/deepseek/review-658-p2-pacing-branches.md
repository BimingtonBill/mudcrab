# Goal
Review two stacked engine branches before they are offered upstream: `phase2/pacing-measure` (measurement only) and `phase2/load-faster` on top of it (new defaults). Find what used to work and now fails, wrong measurements, and unclear code or docs. Track: Phase 2 (upstream issue #117, engine pacing rework).

# Context
- Checkout `D:/Mudcrab/worktrees/p2-pacing` (branch `phase2/load-faster` checked out). Base is upstream main `461517bf`. Read the diffs with `git log --oneline 461517bf..HEAD` and `git diff 461517bf..HEAD -- <path>`, one commit at a time with `git show <id>`:
  - 1a7f6eb7, 1d3778a2: time to a loaded world, `--benchmark-jump`, fly lag, per-stage spans (crates/engine/src/pacing.rs, render_timing.rs, streaming.rs, app.rs, config.rs, metrics.rs).
  - 336f36f1: per-main-schedule timing and asset arrival counts (schedule_timing.rs).
  - d454c3d4: frame-time windows `frames_after_ready`, `jump_load_window`.
  - a75bb1b4 + 6d7d2ad7: IO pool = half the hardware threads (`--io-threads`), `max_model_spawns_per_frame` default 4 -> 16, `io_threads` in the benchmark report.
- Measured on a 16-thread Ryzen 7 5800X (release, jump to cell 4,-21, two repeats): world ready 2.26-2.46 s -> 1.12-1.30 s, ready after the jump 2.36-2.43 s -> 1.30-1.57 s, worst frame inside the jump loading window 14.7 ms -> 19-34 ms (one 34 ms frame in 4 runs), fly at 5000 u/s mean model wait 899 -> 207 ms; memory unchanged.
- Bevy 0.19.1 sources are under `C:/Users/Brannock/.cargo/registry/src/*/bevy_*-0.19.1/` (e.g. TaskPoolPlugin in bevy_app task_pool_plugin.rs) if you need to check how pools interact.

# Questions
1. Behaviour changes for players and tests: anything the new defaults break (tests, scripts in `scripts/`, acceptance thresholds, the streaming fixture, low-core machines: 2 and 4 hardware threads, very many cores, the 128 MiB IO stack times the thread count).
2. Measurement correctness: ready latch, jump window bounds, fly lag counts, schedule timing marker placement (does each span measure exactly its schedule?), asset arrival counting, p99 method.
3. Code quality against the surrounding code: naming, doc comments, dead code, anything in the wrong module, test gaps.
4. Docs (`docs/roadmap/02-profiling.md`): accurate and in step with the code?

# Scope
Read only. Do not edit or build. Do not run engine.exe.

# Report
Verdict first (ready / small fixes / fix first), then findings ranked by severity as `path:line - problem - suggested fix`, each with the evidence (quote the line). Under 900 words.
