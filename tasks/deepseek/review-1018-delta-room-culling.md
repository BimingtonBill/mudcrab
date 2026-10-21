# Goal
Delta review (round 1) of the fix commits on the local branch `phase2/room-culling` (Mudcrab, Rust/Bevy 0.19, upstream issue #188 engine half: interior room-and-portal culling). Decide whether the fixes answer the round-0 review findings and whether they introduce new problems.

# Context
- Your working folder is the delta review folder. `delta.diff` is the change from the reviewed head 95ebd30a to the new head (`commits.txt`). `before/` and `after/` hold the old and new copy of every touched file: read code only from there.
- `reviews/review-1.md` (DeepSeek) and `reviews/review-2.md` (Muse) are the round-0 reviews. `reviews/review-3.md` summarises the fixer report.
- Bevy 0.19 sources, if you need to check a claim: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ (bevy_diagnostic-0.19.*, bevy_render-0.19.*).

# Scope
Read only. Change nothing.

# Done when
For every finding in review-1 and review-2: fixed, partly fixed, not fixed, or rightly skipped, with line evidence from `after/`. Plus any new problem the delta introduces: a test that does not exercise what it claims (check the negative-control claim by reading the schedule code), a stale owner cache (despawned or re-parented entities, entity reuse), a cache that grows without bound, a test-only helper reachable from production, a weakened test.

# Report
Verdict line: ready / small fixes / fix first. Then the per-finding table, then new problems as `file:line - issue - fix` (at most 6). Under 500 words.
