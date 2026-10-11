# Goal
Delta check of round-2 fixes on three of our open upstream draft PRs (Mudcrab, Rust/Bevy 0.19). Each fix answers one CodeRabbit finding. Decide per PR whether the fix answers the finding correctly and introduces no new problem.

# Context
Your folder holds pr121/, pr124/ and pr125/. Each one has `delta.diff`, `commits.txt`, and `before/` and `after/` copies of the touched files. Read code only from there.

1. **#121.** Finding: when an IO task pool already exists, `engine::run` must check its thread count via `IoTaskPool::try_get()`. If it differs from the requested count, return an actionable error; if it matches, keep success.
   - Fix: `check_existing_io_pool`. An explicit `--io-threads` mismatch is an error; the automatic size (0) keeps the host pool with a warning.
2. **#124.** Finding: `coc`'s editor-id lookup used an unordered LIMIT 1, so a shared name could pick an interior or another worldspace's cell before the valid exterior in the active worldspace.
   - Fix: ORDER BY prefers a gridded cell in the streaming worldspace, then that worldspace, then the lowest id.
3. **#125.** Finding: `shape_hull_points` sampled only 6 directions per capsule end, so the convex hull under-covers the capsule.
   - Fix: 26 directions per end (the 13 cube-neighbour directions, plus and minus), with a coverage test.

# What to check
- Is each fix correct and complete for its finding?
- Edge cases:
  - #121: a race with another initialiser, and whether the error message is accurate;
  - #124: NULL grids, and several gridded matches;
  - #125: whether the direction vectors are normalised, whether points are scaled by radius, and whether the test asserts what it claims.
- No test was weakened, and no behaviour changed beyond the finding.

# Scope
Read only. Change nothing.

# Report
- Per PR: a verdict (ready / small fixes / fix first), then findings as `file:line - issue - fix`.
- Under 350 words.
