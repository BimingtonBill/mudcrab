# Goal
Re-review only the fix delta of a pull request before it is opened upstream on Mudcrab-Team/mudcrab (Rust/Bevy engine running Skyrim assets converted ahead of time). Treat it like a stranger's PR: be strict. Decide whether the delta correctly fixes the earlier findings without breaking anything.

# Context
- PR: "feat(converter): convert sphere and cylinder collision shapes and more physical layers" (part of issue #104; stacked on draft PR #118).
- The delta to review: `local/review/pr-phase1-physics-coverage/delta.diff` (commit 63757221 on top of 1ea98d19). Full PR diff for context: `local/review/pr-phase1-physics-coverage/full.diff`. PR text: `local/review/pr-phase1-physics-coverage/pr-text.md`. No git is available; read the files.
- **Read code only from the review folder.** `local/review/pr-phase1-physics-coverage/base/` holds upstream main's version of every touched file, `.../head/` the PR's new version. The checked-out source under `crates/` is a DIFFERENT branch; never cite it.
- Reference niftools nif.xml (read-only): `D:/Mudcrab/worktrees/p1-scope/ref-byroredux/docs/legacy/nif.xml` (hkResponseType ~line 1288, CollisionFilterFlags ~1641, bhkWorldObject 2765, bhkEntityCInfo 2797, bhkRigidBodyCInfo2010 ~2840, bhkCylinderShape 3070).
- Findings the delta claims to fix (from two earlier reviews):
  1. The No Collision / response check read only the outer copy (filter flags @5, response @28); bhkRigidBodyCInfo2010 holds a second filter (flags @37) and response (@48). Now both are checked.
  2. The check had only a helper-level test. The dummy-content body writer (`crates/dummy-content/src/nif.rs`, `BoxBody`) gained `collision_flags`, `collision_response` and `shape: BodyShape {Box, Sphere, Cylinder}`, and `crates/converter/tests/physics_bodies.rs` drives skips and sphere/cylinder bodies through a full NIF to GLB conversion.
  3. `crates/shared/src/collision.rs` `convex` doc said spheres are unsupported (comment-only change).
  4. The spec (`docs/specs/converters/nif-to-gltf.md` §6) said every skipped body keeps its shapes; it now says which skip classes drop them.
  5. A parry3d comment misdescribed its zero-length segment handling.
- The author ran: fmt, clippy -D warnings and nextest on converter, dummy-content and shared with Rust 1.98.1 (378 passed).

# Scope
Read only. Change nothing.

# Done when
You have checked, with diff line evidence: each fix is correct and complete (offsets against nif.xml, both copies, writer bytes match the decoder's offsets independently), the new tests would fail if the fix were wrong, nothing that used to work now fails (existing fixtures, other users of `BoxBody`), and the spec text matches the code.

# Report
- Verdict line: ready to open / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 6, only verified ones, each marked blocker / should fix / nit.
- Under 350 words.
