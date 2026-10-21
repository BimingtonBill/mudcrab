# Goal
Apply confirmed pre-open review fixes to open PR #93 "fix(converter): keep prune records across a resumed conversion" (Phase 1 track), as uncommitted edits in the worktree.

# Context
- Worktree `D:/Mudcrab/worktrees/p1-pr93`, branch phase1/resume-keeps-prune-records, head a3aa6e05. Only file: `crates/converter/src/pipeline.rs`. See the PR's change with `git diff a896b903 HEAD -- crates/converter/src/pipeline.rs`.
- Fixes to make:
  1. `staged_matches_previous_entry` (around line 1735, called around line 639) scans every `manifest.entries` entry per pruned candidate (O(pruned x entries), ~240k entries on a real pack). Build one `output -> key` (or output -> entry) map once before the loop that calls it, and look entries up by key. Keep behaviour identical.
  2. Missing test: the case the old `changed_meshes_do_not_keep_stale_prune_records` covered before this PR rewrote it: the published copy EXISTS but differs in bytes, and the staged GLB matches the hash recorded in the previous manifest entry, so the prune record must be KEPT. Add one pipeline test asserting that, written in the style of the neighbouring resume tests. Also add a test that resumes twice in a row (stop, resume, stop again, resume) and asserts the prune record survives, if it can reuse the existing stop mechanism of the neighbouring tests; skip it and say why if not.
  3. Update the comment near line 1007-1011 ("reused only when the journal says ...") to also name the `previous_entries` fallback near line 1015.
- Do not change anything else. No new dependencies.

# Scope
Owned files: crates/converter/src/pipeline.rs
- No git commands except `git diff`, `git status`, `git log`.
- Always pass `--target-dir D:/Mudcrab/builds/p1-pr93` to cargo and use toolchain `+1.98.1`.

# Done when
Acceptance: cargo +1.98.1 fmt --all -- --check, cargo +1.98.1 clippy -p converter --all-targets --target-dir D:/Mudcrab/builds/p1-pr93 -- -D warnings, cargo +1.98.1 nextest run -p converter --target-dir D:/Mudcrab/builds/p1-pr93
- All pass. The new keep-case test fails if `staged_matches_previous_entry` is replaced by `false` (check it by reasoning or a temporary edit you revert).

# Report
- What changed, per fix; commands run and results; anything skipped and why.
