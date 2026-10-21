# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: feat(converter): convert sphere and cylinder collision shapes and more physical layers
- What it claims (the PR text a reviewer will read): `local/review/pr-phase1-physics-coverage/pr-text.md`
- Its diff against upstream main 1ad25cdc: `local/review/pr-phase1-physics-coverage/full.diff` (commit list: `local/review/pr-phase1-physics-coverage/commits.txt`). No git is available; read the files.
- **Read code only from the review folder.** `local/review/pr-phase1-physics-coverage/base/` holds upstream main's version of every file the PR touches, and `local/review/pr-phase1-physics-coverage/head/` holds the PR's version. The checked-out source under `crates/` is a DIFFERENT branch (the portal fork, with its own streaming code), so never cite it as main's behaviour. If you need a file that isn't in the folder, say so instead of guessing from the checkout.
- Upstream conventions: CONTRIBUTING.md, `docs/contributing/` if present, CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and nextest.
- **Stacked PR.** This branch sits on draft PR #118 (`local/review/pr-phase1-physics-coverage/stacked-118.diff`, commits 60379b3d and 448a51c2: rigid-body dynamics extraction), which is NOT yet merged. Review THIS PR's own change: `local/review/pr-phase1-physics-coverage/this-pr-only.diff` (commits 1890f322 and 088050ca). Read #118's part only as context; findings in it belong to #118 and should be listed separately, briefly.
- Design choice already made: no change to `crates/shared` or the engine. Spheres become `CollisionShape::Capsule` with a == b; the engine builds capsules with rapier's `Collider::capsule(a, b, radius)` (parry handles a zero-length segment as a sphere). Cylinders become a 32-point `Hull`.
- The Skyrim enum definitions (SkyrimLayer, hkResponseType, CollisionFilterFlags, bhkSphereShape, bhkCylinderShape, bhkMultiSphereShape) are in the reference niftools nif.xml at `D:/Mudcrab/worktrees/p1-scope/ref-byroredux/docs/legacy/nif.xml` (read-only).
- Census evidence was run by the author on a real install (not reproducible here); judge whether the PR text's numbers are consistent with the diff.
- Known red CI: upstream main's clippy was red from a new Rust stable (fixed upstream by #120, Rust pinned to 1.98.1); main (with #120) is merged into this branch.
- Overlap: Chef (TaylorTurnerIT) owns the engine physics half of #104; no other open converter collision PR.

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
