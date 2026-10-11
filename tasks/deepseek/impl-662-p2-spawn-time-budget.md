# Goal
Replace the fixed "models a frame" limit with a per-frame time budget, so a frame never pays more than a set number of milliseconds for the models it hands to Bevy's scene spawner. A fast machine then arms many models a frame and a slow one few, and the worst frame while loading stays near the old 14.7 ms instead of 16-34 ms. Track: Phase 2 (upstream issue #117, engine pacing rework).

# Context
- Worktree `D:/Mudcrab/worktrees/p2-pacing`, branch `phase2/load-faster` (checked out; do not switch). Use the worktree's own `target/` for cargo.
- `crates/engine/src/streaming.rs`:
  - `arm_pending_models` (PreUpdate chain) hands loaded models (`PendingModel` with `Handle<WorldAsset>`, oldest `sequence` first) to the spawner by inserting `WorldAssetRoot`, at most `config.max_model_spawns_per_frame` a frame (default 16; 0 = all).
  - `begin_scene_spawn_batch` / `end_scene_spawn_batch` bracket Bevy's spawn pass in the `SpawnScene` schedule (`SceneSpawnBatch` resource, span `scene/spawn_batch`).
  - Bevy 0.19: `WorldAsset { pub world: World }` (bevy_world_serialization world_asset.rs:23); `world.entities().len()` gives how many entities a model spawns.
- Measured (traces, 16-thread PC): the cost of a burst of new models is paid mostly after the spawn pass, in `PostUpdate` (visibility, transform propagation, render preparation of new entities: 14-35 ms worst at 16-32 models a frame), plus the spawn pass itself (3-10 ms). So the time to learn is the main-world time from the start of `SpawnScene` to the end of `PostUpdate`, not only the spawn pass.

# What to build
1. `EngineConfig.max_spawn_ms_per_frame: f32`, flag `--max-spawn-ms-per-frame <ms>`, default 4.0; `0` turns the time budget off. Keep `max_model_spawns_per_frame` as a hard upper count on top (change its default to 0 = no count limit, since the time budget now does the limiting; keep the flag).
2. A `SpawnCostModel` resource: an estimate of milliseconds per spawned entity, learned online.
   - Measure each frame the span from the start of `SpawnScene` (the existing `begin_scene_spawn_batch` can record it) to the end of `PostUpdate` (a system in `Last`, ordered first, or at the end of `PostUpdate` after everything: pick the robust Bevy 0.19 way and explain it in a doc comment).
   - Keep a baseline: an exponential moving average (EMA) of that span on frames that spawned nothing.
   - On a frame that spawned N > 0 entities (sum of `world.entities().len()` of the models armed for that frame; record it when arming), sample = max(0, span - baseline) / N, and fold it into ms_per_entity with an EMA (alpha about 0.2). Start from a conservative prior (for example 0.01 ms per entity) until the first sample.
3. `arm_pending_models`: among ready models, oldest first, predict cost_i = entities_i * ms_per_entity; arm while the running total stays within `max_spawn_ms_per_frame`. Always arm at least one model a frame when any is ready (so a single huge model cannot stall the queue forever). Stop at the count cap if it is set. Failed loads (no asset) cost 0 and are armed as today.
4. Gauges in the profiler: `streaming/spawn_ms_per_entity`, `streaming/spawn_predicted_ms`, `streaming/spawn_baseline_ms`, and the entities armed this frame. Add the learned ms_per_entity and the budget to the benchmark report's streaming block if that is simple.
5. Unit tests: the arming decision as a pure function (given costs and budget: arms oldest first, stops at the budget, always at least one, respects the count cap, 0 budget = count cap only); the estimator (baseline and EMA updates, no update when N = 0, negative samples clamped).
6. Docs: replace the "16 models a frame" text in `docs/roadmap/02-profiling.md` and the config comment with the time budget; numbers will be filled in after measurement, write "to be measured" where needed.

# Scope
`crates/engine/src/streaming.rs`, `crates/engine/src/config.rs`, `crates/engine/src/metrics.rs` (only if adding the report fields), `docs/roadmap/02-profiling.md`. No new dependencies. Do not touch the IO pool code.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p engine` pass.
- One commit: `perf(engine): arm loaded models against a per-frame time budget`, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage files by name. No push, no amend. Never run engine.exe.

# Report
Commit id, where the span is measured (begin and end, which schedules), the estimator's formulas as coded, the new flag and defaults, test count. Under 300 words.
