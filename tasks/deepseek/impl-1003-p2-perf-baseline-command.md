# Goal
Second slice of the engine performance baseline (upstream issue #184, Phase 2 track): scene counts and CPU time for streaming and render preparation in the benchmark report, and ONE command that runs the fixed scenes with repeats and writes a small JSON + Markdown baseline report for whatever branch is checked out.

# Context
- Worktree D:/Mudcrab/worktrees/p2-perf, branch phase2/perf-baseline: upstream main + PR #122 (pacing measurement tools) + slice 1 (commits from DeepSeek impl #1001: `RunFingerprint`, `gpu_passes`, `memory.gpu` in `BenchmarkReport`). Read slice 1's commits first (`git log --oneline -8`, `git show`).
- Design study: C:/Modding/Mudcrab/mudcrab/local/research/engine-foundations/research-999-p2-perf-baseline-design-report.md (its file:line refs come from another checkout; re-locate everything here).
- Existing harness: scripts/phase2-profile.ps1 (scenarios synthetic, rural, dense, water, stress, stability; medians of 5 metrics; regression vs profiling-baseline.json; campaign.json and summary.md). PR #122 added schedule timing (crates/engine/src/schedule_timing.rs: per main schedule) and pacing windows; profiling.rs records hand-made CPU spans (e.g. `streaming/*`).
- Issue #184 asks for: steady frames (standing still) and streaming (moving) reported separately; warm-up, several repeats with their spread; draw calls, triangles and entities; CPU time for streaming and render-prep systems; results compare only on the same machine and backend; real scenes run locally with one command and a small JSON report; CI keeps synthetic scenes.

# Scope (owned files: you may change only these)
crates/engine/src/metrics.rs, crates/engine/src/profiling.rs, crates/engine/src/schedule_timing.rs, scripts/phase2-profile.ps1, scripts/phase2-baseline.ps1 (new), docs/roadmap/02-profiling.md. No new crate dependencies.

# Work (three items)
1. **Counts in the report.** A `scene` section: entity count (EntityCountDiagnosticsPlugin), mesh entities, terrain patches, water surfaces, and per-pass `vertex_shader_invocations` / `clipper_primitives_out` from RenderDiagnosticsPlugin where the backend supports pipeline statistics (Vulkan), else `null` with an `unavailable` reason (DX12 has none). `draw_calls: null` with the reason "Bevy 0.19 exposes no draw-call count". Never zero-fill.
2. **CPU time in the report.** A `cpu` section: the main-schedule times #122 records (mean/p95/worst per schedule), the existing hand-recorded spans aggregated (mean/p95/worst/total per span name, at least every `streaming/*` span), and render-world prep if Bevy exposes it (check `RenderDiagnosticsPlugin`/`bevy_diagnostic` for render-app schedule timing; if there is nothing, say so in docs, do not invent).
3. **One baseline command.** `scripts/phase2-baseline.ps1 -Assets <dir> [-Scenarios ...] [-Repeats 3] [-Out <dir>]`: builds the engine once (`cargo build --release --bin engine`, skippable with `-NoBuild`), runs each scenario `-Repeats` times via the existing phase2-profile.ps1 machinery (reuse it, do not fork its logic; refactor shared parts into functions if needed), with a standing-still window and, where the scenario moves (stress / jump), the streaming window, then writes `baseline.json` (fingerprint once, per scenario: per-metric median and min-max spread across repeats, per-pass GPU medians, memory, counts, cpu) and `baseline.md` (one short table per scenario). Synthetic scenes need no `-Assets`. It refuses to compare two baselines whose fingerprint backend/adapter differ. Every engine window gets `--run-label "baseline <scenario> <n>/<repeats>"`. Print the expected duration before starting (scenarios x repeats x window).

# Rules
- Never use `git stash`. Path-only commits, no amend/rebase/push.
- Before every build run `df -h /d`; below 15 GB free, stop and report. Always `export CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-pacing`.
- Do NOT run timed scenarios on real data (a timing run needs a booked, quiet PC; the lead does that). You may run the synthetic scenario ONCE with `-Repeats 1` and a short window to prove the script end to end.
- Commits: one per item, upstream conventional style (`feat(engine): ...`, `feat(scripts): ...`), each ending with `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Update docs/roadmap/02-profiling.md for every field and the new command.

# Done when
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo nextest run -p engine --all-targets` pass (the load-sensitive `streaming_fixture_loads_its_interior_by_id...` may fail alone; rerun it alone and report). Unit tests for the new report sections fed with samples. The synthetic dry run produced baseline.json and baseline.md (paste both, shortened).

# Report
Status; commits; per item what and where (file:line); the baseline.json shape; the dry-run output; what is not possible on Bevy 0.19 and why.
