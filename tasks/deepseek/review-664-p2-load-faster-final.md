# Goal
Final review of `phase2/load-faster` against `phase2/pacing-measure`: what used to work and now fails, wrong comments, docs out of step. Track: Phase 2 (upstream issue #117).

# Context
- Checkout `D:/Mudcrab/worktrees/p2-pacing` (branch `phase2/load-faster`, head 67a1e7e8). The load-faster commits are `git log --oneline d454c3d4..HEAD` (d454c3d4 is the top of the measurement branch). Read `git diff d454c3d4..HEAD` and each commit with `git show`.
- Net intent after the re-measurement: the IO pool keeps upstream main's automatic size (a quarter of the hardware threads, 1..4) with a new `--io-threads` override; models armed per frame default 4 -> 32; benchmark report carries `system.io_threads`; frame-time windows only in benchmark runs; review fixes from two earlier reviews (percentile wording, jump guards, count spans out of the ms table, docs).
- Measurements behind the defaults are in `docs/roadmap/02-profiling.md` (section on load speed defaults).

# Questions
1. Anything left over from the reverted "half the hardware threads" rule (comments, docs, tests, constants)?
2. Does any test, script in `scripts/`, or doc still assume 4 or 16 models a frame?
3. Bugs or unclear code in the diff; claims in comments or docs that the numbers in the docs do not support.

# Scope
Read only. Change nothing. Do not run engine.exe.

# Done when
Every commit in the range has been read, and each question has an answer.

# Report
Verdict first (ready / small fixes / fix first), then findings as `path:line - problem - fix`, with the quoted line as evidence. Under 500 words.
