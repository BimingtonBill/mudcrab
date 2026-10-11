# Goal
Re-review two small fix deltas on open PRs of Mudcrab-Team/mudcrab (Rust/Bevy engine running converted Skyrim assets), made after the previous review round. Be strict. Decide per PR whether the fix is correct and breaks nothing.

# Context
No git is available; read the files. Read code only from the review folders; the checked-out `crates/` is a different branch.

PR #114 delta: `local/review/pr-114/delta3.diff` (e21a1779 -> e331049c). Files after: `local/review/pr-114/head-final/`. Upstream main: `local/review/pr-114/base/`. Claim: a 32-bit RGB DDS whose alpha is marked with DDPF_ALPHA (0x2) instead of ALPHA_PIXELS (0x1) was read as opaque on the packed path while main (image_dds) kept alpha; either flag with a non-zero mask now carries alpha, matching ddsfile 0.5.2's own A8R8G8B8 detection (`format/d3d.rs` ~315-336 in the ddsfile crate). Test adds a 0x42 case; the payload test now asserts the pre-allocation message "its mips need".

PR #118 delta: `local/review/pr-118/delta3.diff` (d70418d1 -> 827a9bd9). Files after: `local/review/pr-118/head-final/`. Claim: a node name used by several nodes was trusted at its predicted index in the conversion path; if the written GLB's node order differed from the NIF static scene's, a same-named node could sit at that index. Now a reused name is trusted only when the GLB's node names start with exactly the static-scene order (the order the index was predicted from); otherwise, and always for annotate (no source order), names must be unique. Real data: all 1,745 vanilla meshes/clutter NIFs still keep 1,685 bodies, 727 dynamic.

# Scope
Read only. Change nothing.

# Done when
Both deltas checked against their diffs: correct, tested (tests fail if reverted), nothing that worked on upstream main now fails.

# Report
- One verdict line per PR: ready / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 4 per PR, verified only, each marked blocker / should fix / nit.
- Under 300 words.
