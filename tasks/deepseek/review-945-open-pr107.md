# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: docs(contributing): contributor workflow, PR guide, labels and AI policy (open PR #107)
- What it claims (the PR text a reviewer will read): `local/review/pr-107/pr-text.md`
- Its diff against upstream main a896b903: `local/review/pr-107/full.diff` (commit list: `local/review/pr-107/commits.txt`). No git is available; read the files.
- **Read code only from the review folder.** `local/review/pr-107/base` holds upstream main's version of every file the PR touches, and `local/review/pr-107/head` holds the PR's version. The checked-out source under `crates/` is a DIFFERENT branch (the portal fork, with its own streaming code), so never cite it as main's behaviour. If you need a file that isn't in the folder, say so instead of guessing from the checkout.
- Upstream conventions: CONTRIBUTING.md, `docs/contributing/` if present, CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and nextest.
- This is a documentation-only PR, already open, written as a PROPOSAL for the maintainers. Check especially: claims about GitHub settings (ruleset: squash or rebase only, `tests_pass` required, branch must be up to date, threads resolved, 0 approvals; no merge queue), commands that wouldn't work as written, rules that contradict CONTRIBUTING.md or each other, and anything that reads as a settled decision rather than a proposal. It closes issues #77, #78, #79 and #100. Team practice it should match: an assigned issue is a claim; a contributor without triage rights claims by commenting; draft PR = work in progress; PRs link issues with `Closes #N` / `Part of #N`; nobody merges their own PR (since 2026-10-01, team decision; check whether the docs say so).

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
