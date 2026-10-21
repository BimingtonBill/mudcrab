# Goal
Make the engine load cells about twice as fast by default: size the asset IO thread pool to the machine (half the hardware threads instead of a quarter capped at 4) and raise the default number of loaded models handed to the scene spawner per frame from 4 to 16. Add a `--io-threads <n>` flag. Track: Phase 2 (pacing rework, upstream issue #117).

# Context
- Worktree `D:/Mudcrab/worktrees/p2-pacing`, branch `phase2/load-faster` (cut from `phase2/pacing-measure`, which adds the measurements). Use `CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-pacing` for every cargo command.
- `crates/engine/src/app.rs`: `configure_io_task_pool()` (called at the top of `run`) builds Bevy's `IoTaskPool` with `std::thread::available_parallelism().div_ceil(4).clamp(1, 4)` threads and a 128 MiB stack (`io_task_pool_builder`, `IO_TASK_STACK_BYTES`; glTF loads nest). Bevy's own `TaskPoolPlugin` later sizes the async-compute and compute pools from its defaults; it skips the IO pool because it is already initialised.
- `crates/engine/src/config.rs`: `EngineConfig`, `max_model_spawns_per_frame` (default 4, `--max-model-spawns-per-frame`, 0 = no limit), its tests, and the help text for flags (find where `--max-model-spawns-per-frame` is described for `--help`; add `--io-threads` beside it in the same style).
- `crates/engine/src/streaming.rs` `arm_pending_models` doc comment describes the budget; update the default it mentions if any.
- Docs: `docs/roadmap/02-profiling.md` and any doc that states the 4-per-frame default or the IO pool size (`rg -n "max-model-spawns|IO pool|IoTaskPool|io task" docs crates README.md`).
- Measured on a 16-thread PC (Ryzen, RTX), release build, jump to cell 4,-21, two repeats each (record these in the docs where the defaults are explained, briefly):
  | IO threads, models per frame | world ready | ready after jump | worst frame while loading after the jump | fly at 5000 u/s: wait from load request to model ready (mean) |
  | 4, 4 (old) | 2.26-2.46 s | 2.36-2.43 s | 14.7 ms | 899 ms |
  | 8, 16 (new) | 1.12-1.30 s | 1.42 s | 18.9-22.3 ms, no frame over 33 ms | 207 ms |
  | 12, 16 | 1.07-1.13 s | 1.17-1.28 s | 27.3-34.7 ms | 206 ms |

# What to change
1. `EngineConfig.io_threads: usize`, default 0 meaning automatic; `--io-threads <n>` sets it (reject non-numbers the way the other numeric flags do). Automatic = `available_parallelism / 2`, at least 1 (16 -> 8, 8 -> 4, 4 -> 2, 2 -> 1); no upper cap. Put the rule in one small pure function with a unit test over those values.
2. `configure_io_task_pool` takes the config's value (it is called after the config is parsed; pass `config.io_threads` in). Log the chosen thread count once at info level.
3. Default `max_model_spawns_per_frame` 4 -> 16; update tests and docs.
4. Keep everything else as is (no change to the compute or async pools).

# Scope
Only `crates/engine/src/app.rs`, `crates/engine/src/config.rs`, `crates/engine/src/streaming.rs` (doc comment only), and docs under `docs/`. No new dependencies.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo nextest run -p engine` pass.
- One commit on `phase2/load-faster`: `perf(engine): size the asset IO pool to the machine and arm 16 models a frame`, body 2-4 lines with the numbers above, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage files by name. No push, no amend. Never run engine.exe.

# Report
Commit id, the automatic rule as coded, every file changed, test count. Under 200 words.
