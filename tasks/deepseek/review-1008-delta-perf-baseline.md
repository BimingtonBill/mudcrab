# Goal
Delta review (round 1) of the fix commits on the local branch `phase2/perf-baseline` (Mudcrab, Rust/Bevy 0.19, upstream issue #184: engine performance baseline). Decide whether the fixes answer the round-0 review findings and whether they introduce new problems.

# Context
- Your working folder is the delta review folder. `delta.diff` is the change from the reviewed head a3a8465d to the new head (`commits.txt`). `before/` and `after/` hold the old and new copy of every touched file: read code only from there.
- `reviews/review-1.md` (DeepSeek) and `reviews/review-2.md` (Sol) are the round-0 reviews. `reviews/review-3.md` (engine sampling fixes) and `reviews/review-4.md` (script and baseline fixes) are the fixing workers' reports: what they say they fixed or skipped and why. `reviews/review-5.md`, if present, is the docs follow-up.
- Bevy 0.19 sources, if you need to check a claim: C:/Users/Brannock/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/ (bevy_diagnostic-0.19.*, bevy_render-0.19.*).

# Scope
Read only. Change nothing.

# Done when
For every finding in review-1 and review-2: fixed, partly fixed, not fixed, or rightly skipped, with line evidence from `after/`. Plus any new problem the delta introduces: wrong statistics (warm-up frames, double counting, a sample count that does not match what was averaged), a counter reported as 0 instead of null, a script path that reuses an old report or hides a failure, a comparison that accepts different machines or workloads, docs that disagree with the code, a weakened test.

# Report
Verdict line: ready / small fixes / fix first. Then the per-finding table, then new problems as `file:line - issue - fix` (at most 6). Under 500 words.
