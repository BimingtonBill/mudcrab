# Goal
Apply the confirmed pre-open review fixes to the pacing measurement branch. Track: Phase 2 (upstream issue #117). Its PR text says "measurement only, no behaviour change"; make that true.

# Context
- Worktree `D:/Mudcrab/worktrees/p2-measure`, branch `phase2/pacing-measure-v2` (checked out, current upstream main merged in; do not switch). Use the worktree's own `target/` (it starts cold; that is fine).
- Files: crates/engine/src/pacing.rs, schedule_timing.rs, render_timing.rs, streaming.rs, metrics.rs, profiling.rs, config.rs, app.rs; docs/roadmap/02-profiling.md.
- PR text: `C:/Modding/Mudcrab/mudcrab/local/pr-drafts/106-pr-117a-pacing-measure.md` (edit it too).

# Fixes (verify each against the code first; if one is wrong, skip it and say why)
1. Instrumentation runs in every session: ScheduleTimingPlugin (registered via AcceptanceMetricsPlugin), the asset arrival counts and the world-ready window scan. Gate all of it on a benchmark or profiling run (`EngineConfig::is_benchmark_run()` or the profiling flag, whichever the existing metrics use), so a normal play session runs none of it. Test that a non-benchmark app does not add the schedule markers / does not count.
2. World-ready never latches if any window cell ended in `CellStatus::Failed` (map edge, missing data). Count a failed cell as settled for the ready predicate, and report how many failed cells were in the window when it latched (a field next to the ready time). Test.
3. Frame times in the loading windows use virtual `Time`, which Bevy clamps (max delta 250 ms by default), so a 1 s hitch reads 250 ms. Use `Time<Real>` for the window frame times (check what the benchmark's own frame_ms_* use and match it; if those also use virtual time, use Real for the windows and say in the doc). Test if practical.
4. `format_version` jumps from 7 to 9: make it 8 (one bump for this PR), update docs.
5. The `streaming/terrain_mesh` span changes meaning (per cell now, real timing): say so in docs/roadmap/02-profiling.md and the PR text.
6. Schedule timings are only kept as summaries, so "frame-by-frame correlation" with asset bursts is not possible from the output: narrow the claim in docs and PR text (do not add per-frame export).
7. `--benchmark-jump x,y` as i32 can overflow at extremes in the position math: validate a sane grid range (for example -512..=512) when parsing, warn and drop otherwise. Test.
8. The pipeline-cache counters include compute pipelines: rename or document them as "pipelines (render and compute)".
9. Nits: no terrain at the jump target silently gives y = 0: log it once at warn (it may already; check). Leave the other nits.
10. PR text: "mostly tests and docs" is about a third: say "about a third tests and docs". "Documents every new field" misses `jump_target`: document it. Name the overlaps with #94 and #98 (both change flag parsing in config.rs).

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` pass.
- Commits in upstream style, e.g. `fix(engine): measure only in benchmark runs; settle failed cells; real frame times`, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage files by name. No push, no amend. Never run engine.exe.

# Report
Commit ids, each fix done or skipped with reason, test count. Under 300 words.
