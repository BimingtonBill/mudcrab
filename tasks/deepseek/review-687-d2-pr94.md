# Goal
Delta review of fix commits on open upstream PR #94 ("the engine answers --help and refuses unknown or malformed options", Mudcrab, Rust/Bevy). Decide whether the fixes answer the earlier review findings and whether they introduce new problems.

# Context
- Your working folder is the delta review folder. `delta.diff` is the change from the reviewed head 6f480628 to the new head f589ebc7 (`commits.txt`). `before/` and `after/` hold the old and new copy of every touched file: read code only from there.
- `reviews/review-1.md` and `reviews/review-2.md` are the two previous delta reviews this delta answers. Their finding that `-h`/`--help` wins even in a value position is kept on purpose and documented in the PR text, not changed in code.

# Scope
Read only. Change nothing.

# Done when
For every finding in review-1 and review-2: fixed, partly fixed, not fixed, or rightly skipped, with line evidence from `after/`. Plus any new problem the delta introduces (correctness, a weakened test, panics, misleading comments).

# Report
Verdict line: ready / small fixes / fix first. Then the per-finding table, then new problems as `file:line - issue - fix` (at most 6). Under 450 words.
