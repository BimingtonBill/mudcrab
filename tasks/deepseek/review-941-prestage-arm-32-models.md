# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: perf(engine): arm 32 loaded models a frame instead of 4
- What it claims (the PR text a reviewer will read): `local/pr-drafts/105-pr-117b-load-faster.md`
- Its diff against upstream main 461517bf: `local/review/pr-arm-32-models/full.diff` (commit list: `local/review/pr-arm-32-models/commits.txt`). No git is available; read the files.
- You may read the checked-out source under `crates/` and `docs/` for surrounding code. That checkout is upstream main plus other work, not this PR's head.
- Upstream conventions: CONTRIBUTING.md, `docs/contributing/` if present, CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and nextest.
- Overlaps: our open PR #94 makes the engine refuse unknown flags (this PR adds `--io-threads`); a sibling PR (pacing-measure-v2) adds the measurement tools and must merge in either order; upstream issue #117 is the plan. The 4-per-frame limit being raised came from our own earlier PR #57.
- Known: upstream main's clippy is currently red from a new Rust stable (3 `bail!` lines in dummy-content); not this PR's job.

# Scope
Read only. Change nothing.

# Done when
You have checked, with diff line evidence:
1. **Correctness:** logic errors, edge cases, panics, integer or float overflow, off-by-one, wrong units.
2. **What used to work and now fails:** every behaviour change against main: defaults, flags, outputs, file formats, timing, a test that was weakened.
3. **Claims vs diff:** every number and statement in the PR text is backed by the diff or its evidence. Anything overstated or missing gets flagged (for example a claimed test that doesn't exist).
4. **Tests:** whether the new behaviour is covered, and what a reviewer would ask to see tested.
5. **Scope and size:** anything unrelated that should be split out; leftover debug code, local paths, TODOs.
6. **Overlap:** conflicts with other open work named in the context.

# Report
- Verdict line: ready to open / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 8, only ones you verified in the diff. Mark each **blocker**, **should fix** or **nit**.
- PR text problems (claims the diff doesn't support), as a separate short list.
- Under 450 words.
