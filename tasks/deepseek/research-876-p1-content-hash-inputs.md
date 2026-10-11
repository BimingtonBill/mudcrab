# Goal
Gather the facts for a one-page design note on how derived per-cell render data could be keyed on content hashes of every input (records, meshes, materials, converter version) so only changed cells rebuild (upstream issue Mudcrab-Team/mudcrab#185), and how that interacts with two other contributors' open work. Phase 1 track. Research only; Claude writes the note.

# Context
- Read code ONLY from `D:/Mudcrab/worktrees/p1-main-ro` (upstream main 2c11897b).
- Read GitHub with `gh`, read-only commands only: `gh issue view <n> --repo Mudcrab-Team/mudcrab --comments`, `gh pr view <n> --repo Mudcrab-Team/mudcrab --comments`, and `gh pr diff <n> --repo Mudcrab-Team/mudcrab` when you need the code. Items: #185 (the proposal), #183 (principles), #156, #157, #158, #159, #160 (Chef's record schemas; some may be issues, some PRs: check both), #154 (MonoPT's MO2 profiles), #90 (saves).
- Never post, comment, create or edit anything on GitHub.

# Questions, each with evidence (file:line, or issue/PR number plus a quote under 15 words)
1. What the converter already hashes today: the ingestion cache (`crates/converter/src/cache.rs`), the `conversion_cache` table (file_hash per plugin), manifest hashes, converter schema version constants in `crates/shared/src/lib.rs`, per-output prune records. What is the key of each, and what is content-addressed versus path- or name-keyed?
2. How a cell's inputs are resolved today: from which records (CELL, REFR/ACHR, base records, their models and textures) and which plugin wins; is any per-cell or per-record provenance kept (which plugin supplied a record)?
3. Chef's #156-#160: what each proposes or changes about record storage, IDs or schemas, and what it would change about "a cell's inputs" or their identity.
4. MonoPT's #154: what an MO2 profile changes (load order, loose-file overrides, which archives), how the converter would see it, and whether one output folder would hold several profiles.
5. Saves (#90): what ID scheme is proposed for overrides that survive load-order changes.
6. Any existing per-cell derived data in the converter output (LOD, cell cache, navmesh, etc.) and how it is invalidated today.

# Scope
Read only; change nothing; no builds.

# Report
Under 700 words: numbered answers 1-6 with evidence, then a short list "facts that constrain a content-hash key" (what must be in the hash, what must not, where plugin names leak in today).
