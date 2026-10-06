# Goal
Track: portal (internal experimental demo, local only, NEVER pushed). Finish merging the remaining open upstream
PRs into branch `experimental/2026-10-03` in worktree `D:/Modding-overflow/wt-experimental`, resolving conflicts
so the workspace builds. Merge resolution only: do not fix or change the PRs themselves.

# Context
- The branch is at origin/main + PRs #99 #119 #114 #118 #126 #125 #123 #124 #128 (merge commits named
  `Merge PR #N (<sha>) into experimental`). PR heads are local refs `refs/exp/pr<N>`.
- Merge, in this order, each with exactly:
  `git -c rerere.enabled=true merge --no-ff -m "Merge PR #N ($(git rev-parse --short refs/exp/prN)) into experimental" refs/exp/prN`
  #121, #122, #136, #137, #139, #141. Do NOT merge #105 (deep conflicts, skipped) or #127.
- Resolve every conflict by hand so BOTH PRs' intent survives: e.g. #121 changes `configure_io_task_pool()` to
  `configure_io_task_pool(config.io_threads)` while main's newer code added shots lines around the old call; both
  sides add tests or flags at the same spot (keep both, whole functions, valid braces). A helper exists for pure
  neighbouring additions: `python C:/Modding/MudcrabProject/tools/merge_keep_both.py <file>` (keeps ours then
  theirs verbatim); only use it after reading the hunk. After resolving: `git add <files>` and
  `git -c rerere.enabled=true commit --no-edit` (rerere records the resolution so the rebuild script replays it).
- If a PR's conflicts turn out deep (a real design clash, not neighbouring edits), `git merge --abort`, skip it,
  and report why.
- Build check after the last merge: `CARGO_TARGET_DIR=D:/Modding-overflow/wt-targets/experimental cargo check
  --workspace --all-targets -j 6`. Fix compile errors that come from merge resolution (e.g. duplicate
  registrations, a call site of a changed signature); report anything that is a PR-vs-PR incompatibility
  instead of redesigning it.

# Rules
Never `git push`; never `git stash` (shared across worktrees); never rebase/amend; commits only as merge
commits (and, if needed, one `[claude] experimental: fix merge fallout` commit with named paths); no engine
windows; no background hold/wait/sleep commands (foreground builds, timeout up to 600000 ms).

# Report
Per PR: merged / skipped (why); each conflict file with what you kept; any fallout fix (file:line, why); the
cargo check result; the final head.
