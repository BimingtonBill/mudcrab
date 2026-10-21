# Goal
Delta review of fix commits on branch phase2/physics-bodies-v2, to open as an upstream draft PR "feat(engine): dynamic clutter bodies from authored rigid-body data" (Mudcrab, Rust/Bevy, bevy_rapier3d 0.36). Decide whether the fixes answer the earlier review findings and whether they introduce new problems.

# Context
- Your working folder is the delta review folder. `delta.diff` is the change from c9458e73 to the new head 22e1eeb9 (`commits.txt`). `before/` and `after/` hold the old and new copy of every touched file: read code only from there.
- `reviews/review-1.md` (DeepSeek) and `reviews/review-2.md` (MiMo) are the two pre-open reviews. The delta includes a merge of upstream main (#92: terrain cache validation and normal repair, in `crates/converter/**` and the terrain-normal code and tests of `streaming.rs`): ignore those parts, they are not this branch's work. Deliberately skipped by the fixer: retrying a body refused over the live cap (it is documented instead), an angular speed cap (this Rapier has no global one; the per-body clamp covers it), zeroing Velocity on spawn (the clamp needs the component), and simulating more than one dynamic body per model (counted and logged instead).

# Scope
Read only. Change nothing.

# Done when
For every finding in review-1 and review-2: fixed, partly fixed, not fixed, or rightly skipped, with line evidence from `after/`. Plus any new problem the delta introduces (correctness, a weakened test, panics, misleading comments).

# Report
Verdict line: ready / small fixes / fix first. Then the per-finding table, then new problems as `file:line - issue - fix` (at most 6). Under 450 words.
