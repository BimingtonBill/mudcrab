# Goal
Review commit 4e63dd9a on branch phase2/lod-terrain-tier (worktree D:/Mudcrab/worktrees/p2-lod; view with `git -C D:/Mudcrab/worktrees/p2-lod show 4e63dd9a`): the port of the distant terrain LOD streaming tier onto upstream main. Is it correct, and is it safe with LOD off?

# Context
- Plan and contract: C:/Modding/Mudcrab/mudcrab/local/research/lod-plan.md (piece 3; "Proposed DB contract"). The contract is that the lod_* tables are OPTIONAL: probed at open, and without them `--lod` warns and disables itself. The world DB schema stays 4, and DB 3 still loads.
- The old code it ports: branch phase2/lod-engine-terrain (tip ed09d925); the engine diff is `git diff 1293cfdc ed09d925 -- crates/engine`.
- Main moved underneath (plan table): the shared commit budget was re-added in streaming.rs, LodPlugin is registered after StreamingPlugin, RenderLayers are used everywhere, and the render-origin rebase must move LOD roots.
- Piece 4 (clip-mask material, lod_terrain.wgsl, normals) and DistanceFog were left out on purpose.

# Scope
Read only. Run only `git log`, `git diff`, `git show` and reading/searching tools. Do not build or edit.

# Done when
Each of these is checked, with file:line, or marked "not checked":
(1) With LOD off (the default), no behaviour change: the cell commit budget, the rebase and the streaming metrics are unchanged.
(2) The optional-table probe: a DB without lod_* tables opens, and `--lod` disables itself.
(3) Band selection and hysteresis, the fully-covered skip, unload and the rebase of LOD roots, compared with the old branch.
(4) The render layers of LOD entities (not drawn in the reflection view unless the old branch did; on the world layer like terrain).
(5) The SQL, compared with the agreed DDL.
(6) The tests actually test the behaviour.
The verdict is one of accept, fix first, reject.

# Report
Findings as `path:line - problem - why it matters`, most serious first; say whether each was checked in code or inferred. Under 500 words.
