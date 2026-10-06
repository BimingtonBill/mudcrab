# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: feat(engine): go through load doors (reads door links when the database has them)
- What it claims (the PR text a reviewer will read): `C:/Modding/MudcrabProject/local/pr-drafts/door-crossing.md`
- Its diff against upstream main origin/main 1b70b9b0 (head 1b92bf7e): `C:/Modding/MudcrabProject/local/review/pr-phase2-door-preload/full.diff` (commit list: `C:/Modding/MudcrabProject/local/review/pr-phase2-door-preload/commits.txt`). No git is available; read the files.
- **Read code only from the review folder.** `{BASE_DIR}` holds upstream main's version of every file the PR touches, and `{HEAD_DIR}` holds the PR's version. The checked-out source under `crates/` is a DIFFERENT branch (the portal fork, with its own streaming code), so never cite it as main's behaviour. If you need a file that isn't in the folder, say so instead of guessing from the checkout.
- Upstream conventions: CONTRIBUTING.md, `docs/contributing/` if present, CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and nextest.
- READ ONLY FROM `C:/Modding/MudcrabProject/local/review/pr-phase2-door-preload/base/` (main's copy of each changed file) and `C:/Modding/MudcrabProject/local/review/pr-phase2-door-preload/head/` (this PR's copy). The checkout you run in is the lead's door-preload worktree; do NOT read any other checkout: the portal branch has door code that main does not, and citing it as main's is wrong.
- Evidence: the PR's real-data run used a database copy with a `door_links` table filled from another branch's conversion (main's converter writes none yet); times from the crossing's own INFO lines; clip `C:/Modding/MudcrabProject/local/review/pr-phase2-door-preload/door-crossing-riverwood-svens.mp4` (frames during the black hold are not real time).
- Overlaps: #117 (planned shared "world ready" gate; streaming pacing), Phase 2's open physics work in `physics.rs` (a `TeleportPlayer` block is copied byte-identically into another open branch, phase2/console-coc), #110 (FormIDs in XTEL are plugin-local; the converter export is a later PR).
- The PR text still has placeholders {IN_MS} {OUT_MS} {MANUAL} {ACTUALLY_RUN} {EVIDENCE}: the measured values are 3326 ms in and 10324 ms out at the default upload budget; ignore the placeholders themselves.

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
