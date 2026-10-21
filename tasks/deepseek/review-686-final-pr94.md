# Goal
Delta review of fix commits on open upstream PR #94 ("the engine answers --help and refuses unknown or malformed options", Mudcrab, Rust/Bevy). Decide whether the fixes answer the earlier review findings and whether they introduce new problems.

# Context
- Your working folder is the delta review folder. `delta.diff` is the change from the reviewed head 9a538bbd to the new head 6f480628 (`commits.txt`). `before/` and `after/` hold the old and new copy of every touched file: read code only from there.
- `reviews/review-1.md` and `reviews/review-2.md` are the two earlier reviews of the PR. Finding 6 (`--max-commit-ms 0`) needed no code change, only a test; finding 8 was a PR-text correction only. New in this delta: `--help`/`-h` anywhere wins over earlier errors, and a bare `--` is ignored.

# Scope
Read only. Change nothing.

# Done when
For every finding in review-1 and review-2: fixed, partly fixed, not fixed, or rightly skipped, with line evidence from `after/`. Plus any new problem the delta introduces (correctness, a weakened test, panics, misleading comments).

# Report
Verdict line: ready / small fixes / fix first. Then the per-finding table, then new problems as `file:line - issue - fix` (at most 6). Under 450 words.
