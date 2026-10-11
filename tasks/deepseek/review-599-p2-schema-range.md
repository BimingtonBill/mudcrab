# Goal
Review upstream PR #72 (branch phase2/schema-range, 5 commits on origin/main 7dae5b8/f3fd0e4) for correctness. Question: is it safe to merge as is, or does it need fixes first?

# Context
- Checkout: this folder, HEAD cd9ed90. Base: `git merge-base HEAD origin/main`. See the change with `git log --oneline origin/main..HEAD` and `git diff origin/main...HEAD`.
- Intent: after upstream's big merge (#65), the engine accepted only world database schema 4, so every pre-merge conversion (converter schema 15, database schema 3) was refused. The PR:
  1. makes the runtime accept world database schemas 3..=4 (`crates/engine/src/world/database.rs`: MIN/MAX consts, public `supports_runtime_database_schema`), and uses the same check in `crates/engine/src/bin/world-inspect.rs`;
  2. changes the asset-set rejection messages in `crates/engine/src/app.rs` to name the accepted range, and makes the incomplete-conversion message report the manifest's schema instead of the engine's;
  3. updates `docs/roadmap/02-integration-and-acceptance.md` and `docs/specs/converters/db-schema.md`;
  4. keeps point lights on `LIGHT_LAYERS` by giving the light child `RenderLayers::from_layers(LIGHT_LAYERS)` plus `PropagateOver<RenderLayers>` (`crates/engine/src/render.rs`, `crates/engine/src/streaming.rs`), because #48's `Propagate(PLACED_OBJECT_RENDER_LAYERS)` on the reference otherwise overwrites it.
- Already checked: engine clippy and nextest (229/229) pass; a real schema-3 conversion was refused before and loads and streams after.
- Main points to check: that every query the runtime runs against the world database really copes with schema 3 (no column or table added in schema 4 is used without a probe); that `PropagateOver` in Bevy 0.19 behaves as claimed (source in the cargo registry, crate `bevy_app`, file `propagate.rs`, under `C:/Users/Brannock/.cargo/registry/src/`); that the messages and tests say what they claim; and that the docs match the code.

# Scope
- Read only. Run only `git log`, `git diff`, `git show`, and reading/searching tools. Do not build or run tests. Do not edit anything.

# Done when
- Every claim above has been checked against the code or marked "not checked".
- The verdict is one of accept, fix first, reject.

# Report
- Findings as `path:line - problem - why it matters`, most serious first; say for each whether you checked it in code or inferred it.
- A list of the schema-4-only tables/columns and, for each, the runtime line that probes or guards it.
- Under 400 words.
