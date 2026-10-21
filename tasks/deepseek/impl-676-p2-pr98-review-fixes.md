# Goal
Fix the confirmed pre-open review findings on open PR #98 (`--shots` renders exact camera poses to PNG), including the maintainer Chef's two review points, which both reviewers confirmed. Track: Phase 2.

# Context
- Worktree `D:/Mudcrab/worktrees/p2-shots`, branch `phase2/reference-shots-v2` (checked out at the PR's current upstream head 904c4ed9; do not switch). Use the worktree's own `target/`. This PR is already open upstream (#98); you make LOCAL commits on top of its head. Never push. Never run engine.exe.
- Reviews to act on (read them): `C:/Users/Brannock/.claude-deepseek/agents/runs/review-669-prestage-open-98.1/report.md` and `C:/Users/Brannock/.claude-deepseek/agents/runs/review-669-mimo.1/report.md`. The PR text the reviewers read is `C:/Modding/Mudcrab/mudcrab/local/review/pr-open-98/pr-text.md` (do not edit it; list the PR-text corrections you recommend in your report instead).
- Verify every finding against the code before fixing; skip any that is wrong and say why.

# Fixes
1. Chef's point 1: a shot is marked done when `path.is_file()`, so a save that fails after creating the file (or a 0-byte / undecodable file) is logged `settled` and the run exits 0. Make the capture observe the save's result per shot (e.g. a custom observer around `save_to_disk`, or capture the `ScreenshotCaptured` image and write it yourself with `image` so the write's `Result` is known), fail the shot on any error, and require a non-empty decodable file before logging success. Test the failure path (for example a write into a directory that does not exist, or a mocked writer).
2. Chef's point 2: `--shots` with no value only warns and the run continues as a normal interactive run; `--shots-out` with no value is dropped silently. Both must stop the engine with a non-zero exit and a clear message before any window opens. Update the test that codified the leniency.
3. A shot that never settles is taken and marked `timed_out` but the run still exits 0, while the spec says the run exits non-zero when a shot was given up on: make the code match the spec (fail it) or the spec match the code; pick fail.
4. The settle rule ignores `failed_cells`, asset load failures and the validation counters main's readiness check uses: include them, and log them on the shot line.
5. Nits: `MAX_FRAME_PIXELS` is per side: rename (e.g. MAX_FRAME_SIDE_PIXELS); `run.current_shot()` clones every frame: return a reference; `Shot::validate` should also range-check hfov (0 < hfov < 180 degrees) and pitch (-90..=90).
6. The exit-code change for every run (`finished()`, commit 4f2eb1f5 "a failed engine run exits non-zero") is part of this PR by design: do not split it; report it for the PR text.

# Scope
The files the findings name, and their tests. No new dependencies.

# Done when
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` pass (the test `app::tests::streaming_fixture_loads_its_interior_by_id...` is load-sensitive on main; if only it fails, rerun it alone and report).
- Commits in upstream style (`fix(engine): ...`), trailer `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`, files staged by name. No push, no amend.

# Report
Commit ids; each finding: fixed / skipped (why); the PR-text corrections you recommend, as exact old -> new sentences; test count. Under 400 words.
