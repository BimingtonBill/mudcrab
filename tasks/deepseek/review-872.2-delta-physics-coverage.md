# Goal
Re-review only the fix delta of a pull request before it is opened as a draft on Mudcrab-Team/mudcrab (Rust/Bevy engine running converted Skyrim assets). Be strict, like a stranger's PR. Decide whether the delta fixes the earlier findings and breaks nothing. This is the only delta round before the draft opens, so report only real problems.

# Context
- PR: "feat(converter): convert sphere and cylinder collision shapes and more physical layers" (part of issue #104, stacked on PR #118).
- Delta: `local/review/pr-phase1-physics-coverage/delta.diff` (b15d153d -> 765b5440). Files after: `local/review/pr-phase1-physics-coverage/head-new/`; before: `.../head/`. PR text (updated): `.../pr-text.md`. No git is available; read the files. Read code only from the review folder; the checked-out `crates/` is a different branch.
- Findings it claims to fix (from the previous DeepSeek + GPT review):
  1. Sphere and multi-sphere radii took the scale from the transformed X axis only, so a non-uniform or sheared wrapper transform wrote a sphere of the wrong size. Now `uniform_scale` refuses such transforms (skip with a reason); a rotated uniform scale still works. Tests cover stretch, shear and rotate+scale.
  2. Multi-sphere was only tested from hand-built shape bytes. The dummy-content NIF writer gained `BodyShape::MultiSphere` and a test converts an 8-sphere body to a GLB.
  3. PR text arithmetic: denominators reconciled (1,685 GLB bodies at #118; 78 layer + 8 sphere = 86 added; 10 remaining = root-node bodies), response 0 explained, layer-0 caveat in Still open.
- Reference nif.xml (read-only): `D:/Mudcrab/worktrees/p1-scope/ref-byroredux/docs/legacy/nif.xml` (bhkMultiSphereShape ~line 3124).

# Scope
Read only. Change nothing.

# Done when
Each fix checked against the diff: correct, tested (tests fail if reverted), nothing that worked before now fails; the PR text matches the code.

# Report
- Verdict line: ready / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 5, verified only, each marked blocker / should fix / nit.
- Under 300 words.
