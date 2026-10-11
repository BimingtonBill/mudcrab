# Goal
Round-2 fixes for the #184 performance baseline (Phase 2 track): answer the six small findings of the round-1 delta reviews in the baseline scripts and the profiling docs. No engine code changes.

# Context
- Worktree D:/Mudcrab/worktrees/p2-perf, branch phase2/perf-baseline. Files: scripts/phase2-baseline.ps1, scripts/phase2-profile.ps1 (dot-sourced with -AsLibrary), docs/roadmap/02-profiling.md.
- The findings, with line numbers: C:/Users/Brannock/.claude-deepseek/agents/runs/review-1008-deepseek.1/report.md ("New problems", three items) and C:/Users/Brannock/.claude-deepseek/agents/runs/review-1008-muse.1/report.md ("New problems": items 1, 3 and 4; item 2, the gauge sentence, is already fixed in commit 467bfe9b, skip it).
- The engine report (format_version 10) now has `measured_frames` in `gpu_passes`, `scene` and `memory.gpu` (see crates/engine/src/profiling.rs, read only). Readback latency: the first measured GPU samples can describe passes of the last warm-up frames (profiling.rs ~330).

# Scope (owned files: only these)
scripts/phase2-baseline.ps1, scripts/phase2-profile.ps1, docs/roadmap/02-profiling.md.

# Work (three items, one commit each)
1. **Script inputs and defaults.** `-Scenarios all` without `-Assets` throws with a clear message (and the docs say so), while the plain default with no `-Assets` still runs synthetic only. `Get-ScenarioInputs` records and the comparison checks the fly speed, the synthetic instance count and the assets path (or a stable identity of it such as the resolved full path), matching what the docs claim. An old or unreadable `baseline.json` in the output folder never aborts a campaign when `-CompareTo` was not given: check any baseline to compare against before the first window runs, and when none was asked for, skip comparison with one warning line.
2. **Carry `measured_frames`.** Each window in baseline.json keeps `measured_frames` of gpu_passes, scene and memory.gpu; the aggregate keeps their median (min-max); baseline.md prints them beside the GPU-pass and memory tables, and marks a pass whose sample count is below its window's measured_frames as "fewer samples than frames (GPU readback lag or a pass that did not run every frame)".
3. **Docs.** In docs/roadmap/02-profiling.md: one sentence under `gpu_passes` that the first measured samples may describe passes of the last warm-up frames (readback latency), and one that `count` below `measured_frames` means lag or a pass not run every frame; update the baseline-command section for items 1 and 2.

# Rules
- Never use `git stash` (the stash list is shared by all worktrees). Path-only commits, no amend, rebase or push. Commit style `fix(scripts): ...` / `docs(roadmap): ...`, each ending `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`.
- Do not run cargo. Do not run timed scenes on real data. Parse-check each script: `pwsh -NoProfile -Command "[System.Management.Automation.Language.Parser]::ParseFile('<path>', [ref]$null, [ref]$errors) | Out-Null; $errors"`.
- You may prove item 1 and 2 with ONE synthetic dry run using the existing engine binary: `pwsh -File scripts/phase2-baseline.ps1 -Scenarios synthetic -Repeats 2 -DurationSeconds 5 -NoBuild -Out D:/Mudcrab/scratch/p2-perf-dryrun2`, set `CARGO_TARGET_DIR=D:/Mudcrab/builds/p2-pacing` in that same command so the script finds the release engine, and delete that output folder afterwards.

# Done when
Both scripts parse; the dry run writes baseline.json with per-window and aggregate `measured_frames` and baseline.md shows them; `-Scenarios all` without `-Assets` throws; a junk baseline.json in the -Out folder does not abort a run without -CompareTo.

# Report
Status; commits; per finding what and where (file:line); dry-run output shortened.
