# Goal
Apply the second reviewer's remaining fixes to the stacked pacing branches, in one commit on `phase2/load-faster`. Track: Phase 2 (upstream issue #117). Small, local changes; behaviour changes only in fix 3.

# Context
- Worktree `D:/Mudcrab/worktrees/p2-pacing`, branch `phase2/load-faster` (checked out; do not switch). Run cargo with the worktree's own `target/` (it is already built there).
- Files: `crates/engine/src/pacing.rs`, `crates/engine/src/schedule_timing.rs`, `crates/engine/src/profiling.rs`, `crates/engine/src/metrics.rs`, `crates/engine/src/config.rs`, `docs/roadmap/02-profiling.md`.
- Bevy fact (checked in bevy_asset 0.19.1 lib.rs:661-666): `Assets::<A>::asset_events` runs in `PostUpdate` (set `AssetEventSystems`), so counting `AssetEvent` messages in `Last` sees that frame's events. Keep the code; make the comment in schedule_timing.rs state this fact with the Bevy reference.

# Fixes
1. Percentile wording: `FrameWindowStats::p99_ms`, `distance_percentile` and their tests call the method "nearest rank", but the index is `ceil((n - 1) * p)` (the higher of the two ranks around (n-1)*p, same as `profiling::percentile`). Keep the formula (numbers stay comparable with the rest of the report); fix the wording in doc comments, test messages and docs. Make `distance_percentile` private if nothing outside pacing.rs uses it.
2. `docs/roadmap/02-profiling.md` load-speed table: set the 8 threads / 16 models row to these measured values: world ready 1.12-1.36 s, ready after jump 1.30-1.57 s, worst frame while loading after the jump 16-34 ms (one frame over 33 ms in seven runs). Also fix "ground height plus the usual start offset" for the jump position: only the start offset's height (Y) is applied (check `benchmark_jump_position` in app.rs and say exactly what it does).
3. `drop_jump_the_run_would_overwrite` (config.rs) drops `--benchmark-jump` for `--auto-fly-speed` and `--acceptance-screenshot`; also drop it (same warning style) for `--streaming-fixture`, whose driver moves the camera every frame. Add a warning (not an error) when `--benchmark-jump` is given without `--benchmark-frames`/`--benchmark-duration`, since no report is written then. Tests for both.
4. `assets_added/*` counts are recorded as spans, so profiling.rs's summary "Top CPU spans | Mean ms | P95 ms | Total ms" table can list a count as milliseconds. Leave them out of that table (they stay in cpu-spans.json), and say in the schedule_timing.rs doc that these spans are counts, not milliseconds.
5. Doc wording: `fly_lag` "Present only when the run flew" and similar "absent" claims on `Option` fields that serialise as `null`: say `null`. `FrameWindowStats` "from the same frame deltas as the report's `frame_ms_*` fields": say the windows include warm-up frames (the `frame_ms_*` fields do not), and that in a jump run `frames_after_ready` includes the jump window's frames.
6. Add one serialization test: a `BenchmarkReport` (or the smallest struct that carries `#[serde(flatten)] pacing: Option<PacingReport>`) with `pacing: None` serialises with `serde_json::to_value` without error and without pacing fields.

# Scope
Only the files named above. No new dependencies.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p engine` pass.
- One commit: `fix(engine): pacing review follow-ups: percentile wording, jump guards, count spans out of the ms table`, trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage files by name. No push, no amend. Never run engine.exe.

# Report
Commit id, one line per fix, files changed, test count. Under 200 words.
