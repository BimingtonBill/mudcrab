# Goal
Write the plan for rebuilding distant LOD (level of detail: the low-detail terrain, objects and trees Skyrim draws beyond the loaded cells) on current upstream main, from the old local branches. The result decides the pieces, their order, who builds each, and the world-database contract.

# Context
- Repo: this folder. Upstream main is `origin/main` (a896b90 or later). Old branches (local refs, cut around 2026-09-24 from an older main, far behind now):
  - phase2/lod-mesh-containers (99599440): converter, keeps object-LOD per-cell segments as glTF primitives.
  - phase2/lod-database (1293cfdc, stacks on the above): converter exporter, records distant terrain/object/tree LOD blocks in the world DB (schema bump).
  - phase2/lod-engine-terrain (ed09d925), phase2/lod-engine-objects (d12eb2f4), phase2/lod-engine-trees (e8506a09): engine, streams the blocks behind `--lod`.
  - phase2/tree-billboard-lighting (fd292df4): engine, shades distant tree billboards.
  - See each with `git log --oneline $(git merge-base origin/main B)..B` and `git diff $(git merge-base origin/main B) B --stat`, then read the diffs.
- Research already done (read these first): docs/research/tree-lod-layout.md, and any docs/research or docs/specs file about LOD (`git grep -il lod origin/main -- docs` and on the branches). Upstream issue #96 is about LOD (not readable here; ignore).
- Ownership: crates/converter and crates/dummy-content belong to Phase 1 Dev (they rebuild the converter half); crates/engine is Phase 2's; crates/shared is agreed by both. The world database schema is `shared::WORLD_DATABASE_SCHEMA_VERSION` (4 on main). The runtime also loads database schema 3 (`MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION`, on an open PR), so new tables must be optional to the engine or the minimum must be raised on purpose.
- Main changed a lot since the old branches (PR #65 staging merge: lights, sky, water, terrain textures, streaming pacing, render layers; #27 new base types; #51/#52 converter output layout and hard links; #72 schema range). Find what the old branches touched that main has since rewritten.

# Scope
- Read only. Run only `git log`, `git diff`, `git show`, `git grep`, `git merge-base`, `git rev-list`, `git branch`, and reading/searching tools. Change nothing. Do not build.
- You may split the reading among sub-workers (for example converter half and engine half).

# Done when
- Every old branch is summarised: what it adds (files, tables, flags, shaders), its tests, and what on main now conflicts or makes it obsolete (with file:line on origin/main).
- A proposed world-database contract for LOD: table(s) and columns, which converter stage writes them, how the engine finds them, and whether the engine treats them as optional (so DB 3/4 still load) — with a recommendation.
- An ordered list of pieces, each small enough for one PR, with owner (Phase 1 converter / Phase 2 engine), dependencies, rough size (S/M/L), and what evidence each needs (a test, a real-data count, a `--shots` before/after of distant terrain).
- The first engine piece named, with what it needs from the converter half (or a way to start it without the converter half, e.g. from data main already exports, if any).

# Report
Markdown, headed "# Distant LOD rebuild plan", with sections: Summary; Old branches (one subsection each); What main changed underneath; Proposed DB contract; Pieces in order (a table); First engine piece; Open questions. Cite file:line (origin/main or branch:path) for every claim about code. No word limit.
