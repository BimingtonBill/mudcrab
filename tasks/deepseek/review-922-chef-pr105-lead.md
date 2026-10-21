# Goal
Lead a review of upstream PR #105 by TaylorTurnerIT ("Chef") to Mudcrab-Team/mudcrab (Rust/Bevy engine for converted Skyrim data): "feat(lod): initial native terrain LOD and verified asset reuse" (+12,488 / -218 lines, 72 files). Question: what are the real problems a maintainer should raise before merge, and is it mergeable after fixes? Verdict: accept, fix first, or reject.

# Context
- Your working folder is a checkout of current main. git is NOT available: read `review-diffs/pr-105.diff` (full diff vs main), `pr-105.stat` (file list), `pr-105-description.md`.
- What it claims: compile 4/8/16-cell GLB terrain chunks with LAND diffuse baked into sRGB KTX2 atlases; SQLite/R-tree index; converter schema 16 -> 17, world database schema 4 -> 5; streaming with generation guards, retries, coarser-ready fallback; verified metadata-only reuse of converted assets; launcher shows LOD progress; fixtures, audits, capture scripts, specs. Part of umbrella issue #103 (native DynDOLOD-style LOD, not Skyrim's .btr/.bto).
- Interaction to check: an open PR #95 (`review-diffs/pr-95.diff`) moves the runtime schema minimums into `crates/shared` (MIN_RUNTIME_CONVERTER_SCHEMA_VERSION=15, MIN_RUNTIME_WORLD_DATABASE_SCHEMA_VERSION=3, supports_runtime_world_database_schema) and makes the launcher accept the engine's range. #105 conflicts with #95 in `crates/shared/src/lib.rs` and `crates/engine/src/world/database.rs`. Say how the two should be combined (which ranges after schema 17/5; does #105 keep older databases loadable?).
- CI is green.

# Scope
Read only; no builds. Split the review into independent parts and spawn read-only workers (each gets the diff path and its file subset): (1) converter LOD compiler + albedo + pipeline + metadata reuse; (2) engine streaming/lod.rs + database.rs + world-inspect; (3) shared (lod.rs, asset_lock.rs, lib.rs) + launcher + schema/compat with #95; (4) scripts, fixtures, docs, and whether the PR's size/scope should be split. Check workers' claims before merging their findings.

# Done when
Every part reviewed; findings merged and deduplicated; a verdict.

# Report
Verdict; top findings `path:line - problem - why` (max 10, most serious first, each with the worker run id that found it); the #95 combination advice; what is good; 3-5 polite questions for the author; a note on splitting.
