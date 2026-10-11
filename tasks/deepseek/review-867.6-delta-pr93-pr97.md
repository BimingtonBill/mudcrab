# Goal
Re-review the fix deltas of two open PRs on Mudcrab-Team/mudcrab (Rust/Bevy engine running converted Skyrim assets), before the fixes are pushed. Be strict, like a stranger's PR. Decide per PR whether the delta fixes the earlier findings and breaks nothing.

# Context
No git is available; read the files. Read code only from the review folders; the checked-out `crates/` is a different branch.

PR #93 "fix(converter): keep prune records across a resumed conversion". Delta: `local/review/pr-93/delta.diff` (a3aa6e05 -> 305de20a). File after: `local/review/pr-93/head-new/crates/converter/src/pipeline.rs`; before: `local/review/pr-93/head/...`; upstream main: `local/review/pr-93/base/...`. It claims:
- `staged_matches_previous_entry` used an O(entries) scan per pruned mesh; now an output -> keys map built once. Behaviour identical.
- New tests: the published copy replaced by other bytes while the staged mesh matches the previous manifest's recorded hash keeps the prune record; two stop-and-resume cycles in a row keep it.
- A reuse comment now names the previous-manifest fallback.

PR #97 "perf(converter): validate generated artifacts in parallel". Delta: `local/review/pr-97/delta.diff` (64ad53af -> ea57ecf0). File after: `local/review/pr-97/head-new/crates/converter/src/pipeline.rs`; before: `local/review/pr-97/head/...`; main: `local/review/pr-97/base/...`. It claims:
- Deterministic error: all failures are collected and the lowest artifact index is returned, whatever the thread count (a failing run now checks all remaining files before reporting; main stopped at the first).
- The pool takes `jobs` like the conversion stage (`num_threads(cpu_jobs)`, 0 = rayon default) instead of `jobs.max(1)`.
- GLB test fixtures use escapes instead of raw control bytes (same bytes intended), and the timing test samples in u64.
- New test: two bad meshes, the first in list order is reported on 1, 4 and 16 threads.
Proposed PR body changes: `local/review/pr-93/pr-text-fixes.md`, `local/review/pr-97/pr-text-fixes.md`; check they match the code.

# Scope
Read only. Change nothing.

# Done when
Each fix is checked against its diff: correct, complete, tested (tests fail if reverted), nothing that worked on main now fails. For #97, check the fixture escapes reproduce what the tests need and that collecting failures is safe with rayon (no panic poisoning a lock, memory bounded).

# Report
- One verdict line per PR: ready / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 5 per PR, verified only, each marked blocker / should fix / nit.
- Under 400 words.
