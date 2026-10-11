# Goal
Make the #184 baseline scripts trustworthy (Phase 2 track): every repeat's report is fresh, the profile script behaves as before when run directly, and the aggregated baseline keeps what each run measured and why something is missing.

# Context
- Worktree D:/Mudcrab/worktrees/p2-perf, branch phase2/perf-baseline. Scripts: scripts/phase2-profile.ps1 (dot-sourceable with -AsLibrary) and scripts/phase2-baseline.ps1. Docs: docs/roadmap/02-profiling.md (another worker edits the engine-side parts of this file; you own only the "baseline command" and script sections; keep your edits to those sections).
- Two reviews found these problems; read them: C:/Users/Brannock/.claude-deepseek/agents/runs/review-1004-deepseek.1/report.md and C:/Users/Brannock/.claude-deepseek/agents/runs/review-1004-sol.1/report.md (Sol findings 1, 2, 6, 7, 8, 9, 10 and its "Documentation corrections"; DeepSeek's script and doc findings).

# Scope (owned files: only these)
scripts/phase2-profile.ps1, scripts/phase2-baseline.ps1, docs/roadmap/02-profiling.md (script/baseline sections only).

# Work (three items, one commit each)
1. **Fresh reports, same behaviour.** Each campaign writes to a new timestamped directory (never reuse `run-N` from an older campaign); a repeat counts only if a report was written by THIS invocation (check its run id / timestamp / format_version) and says the run completed; otherwise throw with the scenario, repeat and directory. Run the engine from the repository root again (Push-Location / try-finally) as the profile script did before. `-CompareTo <file>` that does not exist throws. The default with no `-Assets` runs `synthetic` only (or the docs example is fixed; prefer the fallback, like the profile script).
2. **Per-window provenance and union of keys.** Keep each window's fingerprint and its scenario inputs (worldspace, grid, stream radius, duration, warm-up) in baseline.json; the campaign level keeps only fields that are the same in every window, and the script throws if machine/build fields (backend, adapter, driver, resolution, build profile, schemas) differ between windows. Aggregate the UNION of keys across repeats (passes, spans, counters that appear only in some repeats), with per-key sample counts, and carry every section's `unavailable` reasons (gpu_passes, memory.gpu, scene, cpu).
3. **Honest comparison and Markdown.** A comparison against an earlier baseline is refused (or every row marked "not comparable") when any workload or machine field differs: resolution, build profile, driver, graphics settings, schemas, scenario inputs; a different COMMIT is expected and allowed. baseline.md shows the GPU-specific reason when GPU pass times are missing (not the CPU one), and a short memory table (CPU asset bytes in MiB, GPU counters as counts with their units). Fix the doc points: format_version 10 (not 9), `total` not `total_ms`, "median (min-max)" not "mean", and what different commits mean.

# Rules
- Never `git stash`. Path-only commits, no amend/rebase/push. Commit style `fix(scripts): ...` / `docs(roadmap): ...`, each ending `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`.
- Do NOT run the engine or any cargo command before you see the line "TIMING RUN ENDED" in C:/Modding/Mudcrab/mudcrab/local/team/2026-10-06.md (a timing run holds the PC until about 17:05). Never run timed scenes on real data. After it ended you may prove the scripts with ONE synthetic run: `pwsh -File scripts/phase2-baseline.ps1 -Scenarios synthetic -Repeats 2 -DurationSeconds 5 -NoBuild -Out D:/Mudcrab/scratch/p2-perf-dryrun` and delete that folder afterwards.
- PowerShell syntax check without running: `pwsh -NoProfile -Command "[System.Management.Automation.Language.Parser]::ParseFile('<path>', [ref]`$null, [ref]`$errors) | Out-Null; `$errors"`.
- Another worker edits crates/engine/src/* in this worktree at the same time: never touch it; `git add` only your owned files.

# Done when
Both scripts parse; the synthetic dry run (after the timing run) produced baseline.json/baseline.md with per-window provenance; a second dry run with `-CompareTo` the first is accepted, and a hand-edited copy with a different resolution is refused.

# Report
Status; commits; per item what and where (file:line); the new baseline.json shape (short example); dry-run output.
