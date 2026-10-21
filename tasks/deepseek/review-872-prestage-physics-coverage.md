# Goal
Review a pull request before it is opened upstream on Mudcrab-Team/mudcrab (a Rust/Bevy engine that runs Skyrim assets converted ahead of time). Treat it exactly like a stranger's PR: the author is on our side, so be stricter, not kinder. Decide whether it is ready to open as a draft, and list what a maintainer would object to.

# Context
- PR title: feat(converter): convert sphere and cylinder collision shapes and more physical layers
- What it claims (the PR text a reviewer will read): `local/review/pr-phase1-physics-coverage/pr-text.md`
- Its diff against upstream main 869df14f: `local/review/pr-phase1-physics-coverage/full.diff` (commit list: `local/review/pr-phase1-physics-coverage/commits.txt`). No git is available; read the files.
- **Read code only from the review folder.** `local/review/pr-phase1-physics-coverage/base/` holds upstream main's version of every file the PR touches, and `local/review/pr-phase1-physics-coverage/head/` holds the PR's version. The checked-out source under `crates/` is a DIFFERENT branch (the portal fork, with its own streaming code), so never cite it as main's behaviour. If you need a file that isn't in the folder, say so instead of guessing from the checkout.
- Upstream conventions: CONTRIBUTING.md, `docs/contributing/` if present, CI runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and nextest.
- **Stacked PR.** It sits on PR #118 (rigid-body dynamics into the GLB collision data), which is open and not merged. Review THIS PR's own change: `local/review/pr-phase1-physics-coverage/this-pr-only.diff` (the head against #118's head 1540418a merged with main 869df14f). In this review, `local/review/pr-phase1-physics-coverage/base/` holds that #118+main version of each touched file, not plain main. `local/review/pr-phase1-physics-coverage/full.diff` is everything against main (includes #118); read #118's part only as context.
- Design choice already made: no change to the shared collision format or the engine. Spheres become `CollisionShape::Capsule` with a == b (the engine builds capsules with rapier's `Collider::capsule`); cylinders a 32-point `Hull`.
- Skyrim enum and block layouts (SkyrimLayer, hkResponseType, CollisionFilterFlags, bhkSphereShape, bhkCylinderShape, bhkMultiSphereShape, bhkRigidBodyCInfo2010) are in the reference niftools nif.xml at `D:/Mudcrab/worktrees/p1-scope/ref-byroredux/docs/legacy/nif.xml` (read-only).
- Evidence was produced by the author on a real install (you cannot rerun it); check the PR text's numbers are consistent with the code.
- Earlier review rounds on this branch already fixed: radius overflow, cylinder axis overflow, both copies of the No Collision flag / response, fixture-level tests. Look for what is still wrong, not for those.
- Overlap: Chef (TaylorTurnerIT) owns the engine physics half of #104.

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
