# Goal
Check whether eclepso's commits on upstream PR #92 (Mudcrab-Team/mudcrab, "fix(converter): validate terrain caches and repair invalid normals") resolve the three review points posted on 2026-09-30, and whether those commits introduce anything new that would block a merge.

# Context
The repository is a Rust/Bevy engine that runs Skyrim assets converted ahead of time; this PR is in the converter's terrain cell cache (`crates/converter/src/esm/cell_cache.rs`) plus a small engine comment change.

The diffs are already written to files (no git needed):
- `local/review/pr92/full.diff`: the whole PR against main.
- `local/review/pr92/74f4b4cd.diff`: commit "retain asset reuse and allow null overlay indices", pushed after the review.
- `local/review/pr92/delta-since-review.diff`: everything after the merge commit fda3d57a (commits 93caf1be "keep later duplicate terrain layer assignments", 876441d8 "return dropped terrain layers for caller diagnostics", 41baf9fd "mark welded border normals as untracked").
You may also read the checked-out source under `crates/` for context (it is main plus other work, not the PR head).

The three review points:
1. The two new "revision" keys changed the converter's configuration hash, which forces a full reconversion of every mesh and texture and ends the schema 12-15 upgrade path. Asked: could they be dropped?
2. The duplicate-layer check counted null (FormID 0) ATXT texture layers, which `normalize_texture_layers` removes and the engine never checks, so a LAND record the engine would render stopped the whole conversion. Asked: check non-null layers only.
3. Follow-up, not a blocker: the engine's `recompute_packed_normals` still uses the old border stencil, so welded seam normals differ slightly from the converter's.

# Scope
Read only. Change nothing.

# Done when
Each point has a verdict backed by diff lines, and every behaviour change in the commits after the review is named, with what used to happen and what happens now. In particular, check whether 93caf1be turns a former hard error (duplicate layer assignment) into a silent "later one wins", and whether that matches how the engine and Skyrim treat duplicates, and whether tests cover the new behaviour.

# Report
- Verdict line: merge-ready / small fixes / fix first.
- Points 1-3: Resolved / Partly / Not resolved, each with `file:line` evidence from the diffs.
- New findings, most severe first, as `file:line - issue - why it matters`, at most 5, only ones you verified in the diff.
- Under 350 words.
