# Goal
Port the distant terrain LOD streaming tier (piece 3 of local/research/lod-plan.md) onto current upstream main, in this worktree (branch phase2/lod-terrain-tier, cut from origin/main a896b90). With `--lod`, the engine streams Skyrim's pre-built distant terrain blocks from the world database's optional `lod_*` tables around the camera, beyond the loaded cells. Everything is testable with the `--lod-fixture` harness, with no game data.

# Context
- Read first: C:/Modding/Mudcrab/mudcrab/local/research/lod-plan.md (the plan; sections "Old branches" 3, "What main changed underneath", "Proposed DB contract", "First engine piece"), and the design docs it cites if present.
- The old engine code is on local branch `phase2/lod-engine-terrain` (tip ed09d925), stacked on the converter branch `phase2/lod-database` (tip 1293cfdc). The engine-only diff is `git diff 1293cfdc ed09d925 -- crates/engine scripts docs` (crates/engine/src/lod.rs 1742 lines new, app.rs +909 incl. the fixture, config.rs, streaming.rs, world/database.rs, world/components.rs, metrics.rs, lib.rs, shaders/lod_terrain.wgsl). Read it with `git show ed09d925:crates/engine/src/lod.rs` etc.
- Contract (agreed by both tracks; do NOT change): the four tables lod_grid, lod_block (kind 'terrain'|'objects'), lod_tree_type, lod_tree_instance, exactly as in the plan's DDL. The world DB schema stays 4 (`shared::WORLD_DATABASE_SCHEMA_VERSION`), and the runtime keeps loading DB 3 and 4. The engine must treat the lod_* tables as OPTIONAL: probe at open like the waters columns (crates/engine/src/world/database.rs, the waters optional probe). Without them, `--lod` logs one warning and LOD stays off. Never raise the schema range.
- Main has moved (see the plan's table): the commit budget is inline in the streaming cell-commit loop (streaming.rs ~797-814, `commit_budget_exceeded` ~41). Add a shared per-frame commit window that cells and LOD blocks both draw from, without changing cell behaviour when LOD is off. Plugin order is renderer, sky, lights, streaming in app.rs (~165-206); register LodPlugin after StreamingPlugin. RenderLayers are used everywhere now (render.rs ~70-108): put LOD block entities on the world layer the same way terrain is, and check the reflection view rules. The DatabaseRequest worker (world/database.rs ~89) is where LOD requests go. The render-origin rebase must also move LOD block roots.

# Scope (piece 3 only)
- IN: LodPlugin; bands with the Medium/High/Ultra presets and `--lod-distances`; block planning with hysteresis and the fully-covered skip; the DB request/payload types and SQL; block spawn and placement with the depth offset; unload; rebase; LOD metrics counters; the flags `--lod [on|off]`, `--no-lod`, `--lod-distances`, `--lod-depth-offset`, `--lod-fixture`; the `--lod-fixture` synthetic world and its tests; objects/trees flags only if the planning code needs them as off-by-default switches.
- OUT (later pieces, do not port now): the clip-mask LodTerrainMaterial and shaders/lod_terrain.wgsl, and the model-space normals (piece 4). Use a plain StandardMaterial on the block meshes for now; the loaded cells hide underneath because of the depth offset. Also out: object quadtree and segments (6), trees (8, 9), acceptance-script changes (10).
- Parse options in main's current config.rs style (silently lenient; a separate open PR replaces the parser later). Keep the new options together so they move easily.
- Files you may change: crates/engine/src/{lod.rs (new), app.rs, config.rs, lib.rs, streaming.rs, metrics.rs, world/database.rs, world/components.rs}, plus a short doc section in docs/roadmap (the engine roadmap page) describing `--lod`. Nothing in crates/converter, crates/shared, crates/dummy-content, vendor, Cargo.toml or Cargo.lock.

# Done when
- These pass in this worktree (CARGO_TARGET_DIR is already set for you; do not change it): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p engine`.
- Tests cover: band selection and hysteresis, the fully-covered skip, unload, rebase of LOD roots, the optional-table probe (a DB without lod_* tables opens, and `--lod` disables itself with a warning), and a fixture end-to-end run like the old branch's `--lod-fixture` tests.
- With LOD off (the default), nothing changes for existing tests and streaming behaviour.
- Commit in upstream style when green, e.g. `feat(engine): stream distant terrain LOD blocks behind --lod`, ending with `Co-Authored-By: DeepSeek V4.1 <noreply@deepseek.com>`. Stage only files you changed.

# Report
- What you ported and what you changed from the old branch because main moved (with file:line).
- The commands you ran and what they printed (test counts).
- Anything left out or uncertain, and the cheapest check that would show the port is wrong.
