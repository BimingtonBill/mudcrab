# Goal
Re-measured on a fresh conversion, the extra IO threads gave no load speed and caused longer frames; the model-arming limit alone was the bottleneck. Put the IO pool's automatic size back to what upstream main uses, keep the `--io-threads` flag as an override, and set the default models armed per frame to 32. Track: Phase 2 (upstream issue #117).

# Context
- Worktree `D:/Mudcrab/worktrees/p2-pacing`, branch `phase2/load-faster` (checked out; do not switch). Use the worktree's own `target/` for cargo.
- `crates/engine/src/app.rs`: `io_pool_threads(requested, available)` with `MAX_AUTO_IO_THREADS` (currently automatic = half the hardware threads, capped at 16). Upstream main's rule: `available.div_ceil(4).clamp(1, 4)` (a quarter of the hardware threads, at least 1, at most 4).
- `crates/engine/src/config.rs`: `io_threads` (0 = automatic), `max_model_spawns_per_frame` default 16 with a comment explaining the choice.
- `docs/roadmap/02-profiling.md`: section "Load speed defaults: IO threads and models per frame" with a table from older data.
- New measurements (Ryzen 7 5800X, 16 threads, release, fresh full conversion made with upstream main's converter on 2026-10-02, jump to cell 4,-21; "worst" = worst frame from the jump until the world is ready again):
  | IO threads, models a frame | world ready | ready after jump | worst frame while loading |
  | 4, 4 (upstream main, 4 runs) | 1.77-2.00 s | 2.36-2.80 s | 14.7-20.4 ms |
  | 4, 16 | 0.85-1.03 s | 1.10-1.14 s | 15.5-20.2 ms |
  | 4, 32 (5 runs) | 0.75-0.82 s | 0.82-1.01 s | 15.2-18.4 ms |
  | 4, no limit | 0.75-0.76 s | 0.92-0.94 s | 16.2-17.0 ms |
  | 8, 4 | 1.71-2.01 s | 2.42-2.73 s | 27.5-33.6 ms |
  | 8, 16 | 0.80-0.95 s | 0.93-1.18 s | 26.3-38.0 ms |
  Fly-through at 5000 units/s, stream radius 3: mean wait from load request to model ready 692-704 ms (4,4) -> 168 ms (4,32); worst frame 20.5 -> 22.2 ms; p95 10.2 -> 10.8 ms; peak memory 1.51 GiB both.

# What to change
1. `io_pool_threads`: automatic = `available.div_ceil(4).clamp(1, 4)` again (remove `MAX_AUTO_IO_THREADS`); a requested value is used as given. Update its doc comment (say why: measured, more threads did not load faster and lengthened frames) and its unit test.
2. `max_model_spawns_per_frame` default 32; rewrite the config comment: the cap protects against a burst when many models are already loaded (for example, cached assets) while 32 a frame does not slow loading; cite the measurement briefly.
3. Docs: rewrite the section (title "Load speed defaults: models per frame and IO threads") with the table above, the data used (fresh conversion, date, converter), and one paragraph on the IO finding. Remove claims about the half-the-threads rule everywhere (`rg -n "half the hardware|io_threads|MAX_AUTO_IO" crates docs`).
4. Keep the benchmark report's `io_threads` field.

# Scope
`crates/engine/src/app.rs`, `crates/engine/src/config.rs`, `docs/roadmap/02-profiling.md`. No new dependencies.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p engine` pass.
- One commit: `perf(engine): keep main's IO pool size and arm 32 models a frame`, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage files by name. No push, no amend. Never run engine.exe.

# Report
Commit id, files changed, test count. Under 150 words.
