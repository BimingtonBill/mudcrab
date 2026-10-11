# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: feat(engine): dynamic clutter bodies from authored rigid-body data
- What it claims (the PR text a reviewer will read): `pr-text.md`
- Its diff against upstream main 1ad25cdc: `full.diff` (commit list: `commits.txt`). No git is available; read the files.
- **Read code only from the review folder.** `base/` holds upstream main's version of every file the PR touches, and `head/` holds the PR's version. The checked-out source under `crates/` is a DIFFERENT branch (the portal fork, with its own streaming code), so never cite it as main's behaviour. If you need a file that isn't in the folder, say so instead of guessing from the checkout.
- Upstream conventions: CONTRIBUTING.md, `docs/contributing/` if present, CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and nextest.
- All paths are relative to your working folder (the review folder). The data contract agreed with the converter half is `p104-physics-body-contract.md`.
- This is a DRAFT; on-screen evidence is not done yet, so do not flag its absence. It depends on draft PR #118 (the converter half), and it deliberately carries #118's shared/collision.rs change, two streaming.rs hunks and two converter snapshots byte-identical so both merge in either order: do not flag that duplication, but do flag if it breaks anything on main alone.
- Overlaps: Portal Dev's planned door work adds a player-teleport event to physics.rs; the queued #117 pair changes model arming in streaming.rs.
- Known red CI on upstream main from a newer stable Rust (three `bail!` lines in dummy-content), being fixed separately.

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
