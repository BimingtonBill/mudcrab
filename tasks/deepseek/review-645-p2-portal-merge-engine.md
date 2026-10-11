# Goal
Review the engine and shared side of the main-into-portal merge (Phase 2 review for Portal Dev, before a reconversion): find hunks of upstream main's engine behaviour that the merge lost or merged wrongly, beyond the list the merge commit already declares as "not ported yet".

# Context
- Worktree `C:/Modding/wkj-portal-merge`, branch `portal-merge-main`, merge commit 386044af. Its parents: portal (first parent) and origin/main f3fd0e4-or-later (second parent; check with `git log -1 --format=%P 386044af`). Read the merge commit message first: it lists every engine decision.
- Declared as deliberately NOT ported (do not report these as findings, only confirm the list is complete and that nothing compiles-but-silently-misbehaves because of them): main's paced cell unloads and paced model spawns, terrain normal maps, rapier terrain/static colliders and WorldPlayerPlugin, SkyPlugin, the placed-object render layer. Portal's streaming.rs, render.rs and terrain.wgsl are kept whole.
- Portal numbering: `shared::WORLD_DATABASE_SCHEMA_VERSION = 100_401`, engine accepts exactly that.
- Diff to review: `git -C C:/Modding/wkj-portal-merge diff origin/main -- crates/engine crates/shared`. Most of it is portal-only files (portal.rs, transition.rs, door_*.rs ...); skim those only where they meet main's code. Focus on the files both sides changed: `config.rs`, `app.rs`, `lib.rs`, `metrics.rs`, `lights.rs`, `world/database.rs`, `world/components.rs`, `streaming.rs` (main's version vs portal's), `render.rs`, `shared/src/lib.rs`, and main-only modules (`physics.rs`, `sky.rs` or similar, `remote.rs`, `prepass_vertex_alpha.rs`).

# Questions
1. **Lost main behaviour not on the declared list.** For each main feature in the shared files, say whether the merge keeps it: the strict CLI parser and `--help` (config.rs, exit code 2 for bad options), `--run-label` window titles, upload budget / throttle, `--lights` and the point-light layers, the reflection gate (water reflection only for water a camera draws), shadow cascades, sky dome/fog, water layers, the validate-once cell cache (`CELL_CACHE_VERSION`), read-only DB opens, schema-mismatch error text (which check failed + the reconvert command), the acceptance/benchmark exit codes (a failed run exits non-zero), streaming fixtures (`--streaming-fixture`, the fixture interior crossing). Cite file:line on both origin/main and the merge.
2. **Wrongly merged code.** Anything where a hunk from one side is present but wired wrong: a system registered twice or never, a run condition lost, a resource initialised in only one app path, a CLI option parsed but unused, a metrics field written by nobody that a gate or test reads as if real.
3. **Data contract.** Does `world/database.rs` read every column/table the merged converter writes and nothing it no longer writes? Check `ReferenceQuery` (main's `records`/`base_record_type` plus portal's door/light columns) against the converter's world-DB writer in the same worktree (`crates/converter/src/**`, search for the table names). Is `CELL_CACHE_VERSION` still right if either side changed the cell cache's layout?
4. **The new `StreamingWorld::request_cell` pinned set** (for main's fixture interior): can a pinned cell ever be unloaded, or never be unpinned and leak? Cite code.
5. **Tests dropped.** The message says main's legacy-schema tests were dropped. List every test in origin/main's crates/engine and crates/shared that no longer exists in the merge (names), and for each say whether dropping it is justified.

# Scope
Read only. Change nothing. Run only git read commands (`git diff`, `git show`, `git log`, `git grep`). Never build, test or run cargo in that worktree.

# Report
Findings first, each: severity (blocks reconversion / fix before showcase / minor), file:line in the merge, file:line on origin/main, what is wrong, suggested fix. Then a short "kept intact" list for question 1. Then answers to 3-5. Under 900 words.
