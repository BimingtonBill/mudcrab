# Goal
Re-review two small follow-up fix deltas on open PRs of Mudcrab-Team/mudcrab (Rust/Bevy engine running converted Skyrim assets), made after an earlier delta review. Be strict. Decide per PR whether the follow-up is correct and breaks nothing.

# Context
No git is available; read the files. Read code only from the review folders; the checked-out `crates/` is a different branch.

PR #114 (native BC1/BC7 for uncompressed DDS). Follow-up delta: `local/review/pr-114/delta2.diff` (f3693f59 -> e21a1779); files after: `local/review/pr-114/head-final/`; upstream main's copy: `local/review/pr-114/base/`. It claims:
- Reverts the previous delta's refusal of "ALPHA_PIXELS with a zero alpha mask": that refusal sent such files to the generic decoder (image_dds), which has no mapping for them, so they failed where the packed path converted them. A zero mask has no alpha bits, so the layout reads it as opaque. ddsfile 0.5.2 only exposes `a_bit_mask` when ALPHA_PIXELS or ALPHA is set.
- Adds tests: the alpha-layout rules; the payload check before allocation (16384x16384 header, tiny payload, "truncated" error); the packed failure reason appears in a later X8R8G8B8 decode error ("packed path failed first").

PR #118 (rigid-body dynamics into GLB collision extras). Follow-up deltas: `local/review/pr-118/delta2.diff` (1d0d84da -> d70418d1); files after: `local/review/pr-118/head-final/`. It claims:
- The previous delta required a unique node name in the conversion path too; measured on all 1,745 vanilla meshes/clutter NIFs that dropped 12 correctly placed bodies (vanilla reuses names, e.g. a root and a child both "Potato"). The conversion path, whose GLB is built from the same NIF, now checks only the name at the predicted index in the written GLB; annotate (GLB built elsewhere) also requires uniqueness. Real-data conversion keeps 1,685 bodies, 727 dynamic (one "Table" body dropped, as before the delta).
- Both paths go through one helper `retain_bodies_for_written_glb(collision, glb_bytes, require_unique)`, tested with a GLB whose node order decides which body is kept.

# Scope
Read only. Change nothing.

# Done when
Both follow-ups are checked against their diff: correct, tested (tests fail if reverted), nothing that worked on upstream main now fails.

# Report
- One verdict line per PR: ready / small fixes / fix first.
- Findings, most severe first, as `file:line - issue - why it matters - suggested fix`, at most 5 per PR, verified only, each marked blocker / should fix / nit.
- Under 350 words.
