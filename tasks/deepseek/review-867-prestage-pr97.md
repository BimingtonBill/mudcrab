# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: #97 perf(converter): validate generated artifacts in parallel
- What it claims (the PR text a reviewer will read): `local/review/pr-97/pr-text.md`
- Its diff against upstream main a896b903: `local/review/pr-97/full.diff` (commit list: `local/review/pr-97/commits.txt`). No git is available; read the files.
- **Read code only from the review folder.** `local/review/pr-97/base/` holds upstream main's version of every file the PR touches, and `local/review/pr-97/head/` holds the PR's version. The checked-out source under `crates/` is a DIFFERENT branch (the portal fork, with its own streaming code), so never cite it as main's behaviour. If you need a file that isn't in the folder, say so instead of guessing from the checkout.
- Upstream conventions: CONTRIBUTING.md, `docs/contributing/` if present, CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and nextest.
- Upstream main has moved 2 commits past this PR's base a896b903 (#67: acceptance script BOM; #120: Rust pinned to 1.98.1 in rust-toolchain.toml and CI after a new stable broke clippy in dummy-content). Neither touches the converter source. CI runs on the merge with main.
- The PR is already open upstream; the author is "BimingtonBill" (us). Findings become a local fix commit on the PR head, so be concrete.
- Other open converter PRs by the same author: #93 (prune records across a resume; touches the resume/staging path), #114, #118. Check overlap with #93 if both touch the same files.

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
