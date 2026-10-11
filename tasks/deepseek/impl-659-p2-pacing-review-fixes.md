# Goal
Apply six review fixes to the stacked pacing branches in one commit on `phase2/load-faster`. Track: Phase 2 (upstream issue #117).

# Context
- Worktree `D:/Mudcrab/worktrees/p2-pacing`, branch `phase2/load-faster` (checked out; do not switch). `CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-pacing` for every cargo command.
- Files: `crates/engine/src/pacing.rs` (PacingTracker, FrameWindowStats, track_world_ready), `crates/engine/src/app.rs` (`io_pool_threads`, `IO_TASK_STACK_BYTES`), `crates/engine/src/metrics.rs`, `crates/engine/src/config.rs` (EngineConfig, how a benchmark run is recognised: look for the benchmark-duration / benchmark fields), `docs/roadmap/02-profiling.md`.

# Fixes
1. `PacingTracker` pushes one f64 per frame into `frames_after_ready` forever, and `track_world_ready` runs in every run, including normal play. Record the two frame-time windows (`frames_after_ready`, the jump window) only in benchmark runs (pass a flag from the config, e.g. whether a benchmark duration/output is set; use the same test the benchmark report uses to decide whether to write). Keep the existing tests passing; add one test that a non-benchmark tracker keeps no frame times.
2. `io_pool_threads`: cap the automatic size at 16 (each IO thread reserves 128 MiB of stack address space; above 16 threads was never measured). A requested `--io-threads` value is not capped. Update the unit test (64 -> 16) and the doc comment, and the docs' "no cap" wording.
3. `pacing.rs` module header (lines ~6-9) and the doc on `world_ready_reached` still describe a one-frame rule; the code needs `READY_FRAMES_REQUIRED` (2) consecutive frames and times the first of them. Fix both texts.
4. Frame deltas: the window doc says "from the frame the jump is issued up to and including the frame the world is ready again". Since a frame's delta is the interval that ended at that frame, say so in the doc comment and in docs/roadmap/02-profiling.md in one sentence (no behaviour change).
5. `docs/roadmap/02-profiling.md` (load speed defaults table, ~line 135-139): the 8/16 row says "no frame over 33 ms"; the runs had one 34 ms frame in four runs. Say that instead.
6. `metrics.rs` (~line 80): the doc on `render_pipelines` says "Absent when the renderer did not run", but it serialises as `null`. Say `null`.

# Scope
Only the files named above. No new dependencies. No behaviour change except fixes 1 and 2.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p engine` pass.
- One commit: `fix(engine): keep frame windows to benchmark runs, cap the automatic IO pool at 16 and fix pacing docs`, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage files by name. No push, no amend. Never run engine.exe.

# Report
Commit id, how fix 1 decides a benchmark run (quote the condition), files changed, test count. Under 200 words.
