# Goal
Find which part of the main world causes 20-230 ms main-thread frames while many models finish loading at once, by timing each Bevy main schedule per frame. Measurement only (pacing rework step 1, upstream #117).

# Context
- Worktree `D:/Mudcrab/worktrees/p2-pacing`, branch `phase2/pacing-measure` (commits 1a7f6eb7, 1d3778a2 add pacing measurements). It has ONE uncommitted experiment edit in `crates/engine/src/app.rs` (`configure_io_task_pool` reads `MUDCRAB_EXP_IO_THREADS`): leave it uncommitted and untouched, and do not include it in your commit (`git add` your files by name, and for app.rs use `git add -p` is not available, so do not edit app.rs at all).
- CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-pacing.
- Existing timing: `crates/engine/src/render_timing.rs` records `render_world/main_world` as one phase (the whole main-world update); `crates/engine/src/profiling.rs` holds spans (`ProfilingState::record_elapsed`, summaries in `cpu-spans.json` with mean/p95/p99/worst). Tonight with 12 asset threads and no model spawn limit, `main_world` reached 32-227 ms worst, while every streaming span of ours stays under 1 ms. Suspects: Bevy's asset event handling inserting many loaded assets into `Assets<T>` at once, scene spawning (`SpawnScene`), transform propagation (`PostUpdate`), our readiness scan.

# What to add
In a new module `crates/engine/src/schedule_timing.rs` (registered from the same place `render_timing`'s plugin is, without touching app.rs: e.g. add it inside `RenderTimingPlugin::build` or `AcceptanceMetricsPlugin`, whichever already runs in benchmark runs), time each main-app schedule per frame: `First`, `PreUpdate`, `StateTransition`, `RunFixedMainLoop`, `Update`, `SpawnScene`, `PostUpdate`, `Last`. Use a system at the very start and the very end of each schedule (Bevy 0.19: find the right way, e.g. `.before`/`.after` on all systems via a dedicated `SystemSet` configured first/last, or `MainScheduleOrder` + a timing schedule between each; choose the robust one and explain it). Record spans `main_schedule/<name>` in `ProfilingState` so they land in `cpu-spans.json` with worst values. Also record, per frame, how many assets of type `Mesh`, `Image`, `StandardMaterial` and `WorldAsset` were added that frame (from `AssetEvent::Added` messages) as gauges or spans' companions, so a spike frame can be matched with an asset burst.

# Scope
Only the new module plus a registration line in an existing plugin file (not app.rs), and `lib.rs` for the module. No behaviour change. No new dependencies.

# Done when
- A unit or headless app test proves a schedule span is recorded.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p engine` pass (with the uncommitted app.rs edit present; it compiles).
- ONE commit `feat(engine): time each main schedule and count asset arrivals per frame`, files by name, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. No push, no amend, never run engine.exe.

# Report
Commit id, how the timing hooks into each schedule, the span names, test count. Under 250 words.
