# Goal
Delta review of fix commits on open upstream PR #98 ("feat(engine): --shots renders exact camera poses to PNG", Mudcrab, Rust/Bevy). Decide whether the fixes answer the earlier review findings and whether they introduce new problems.

# Context
- Your working folder is the delta review folder. `delta.diff` is the change from the reviewed head 91174fa0 to the new head 5b18ec81 (`commits.txt`). `before/` and `after/` hold the old and new copy of every touched file: read code only from there.
- `reviews/review-1.md` and `reviews/review-2.md` are the two previous delta reviews this delta answers. The delta also adds a temporary-file-then-rename write for each PNG (the maintainer suggested it). One fix was deliberately skipped: the image clone stays because Bevy's `Image::try_into_dynamic` consumes its receiver.

# Scope
Read only. Change nothing.

# Done when
For every finding in review-1 and review-2: fixed, partly fixed, not fixed, or rightly skipped, with line evidence from `after/`. Plus any new problem the delta introduces (correctness, a weakened test, panics, misleading comments).

# Report
Verdict line: ready / small fixes / fix first. Then the per-finding table, then new problems as `file:line - issue - fix` (at most 6). Under 450 words.
- The maintainer Chef raised two points that both earlier reviews confirmed: (1) a PNG that failed to save was reported as success; (2) --shots and --shots-out accepted no value. Say clearly whether each is now fixed, with line evidence.
