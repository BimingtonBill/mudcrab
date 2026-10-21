# Goal
Track: Phase 2 (for upstream). Upstream's completion plan (docs/roadmap/02-completion-plan.md, the
M5.1 item near line 176) asks the streaming fixture to test fast traversal, teleport, exterior/interior
change and repeated rebasing. The fixture does all but the exterior/interior change. Add it.

# Context
- Your checkout is cut from upstream `main`. Upstream commit style (`test(engine): ...`,
  `feat(engine): ...`), no `[deepseek]` prefix, no project-internal references in code or commits.
- The fixture: `crates/engine/src/app.rs`, `StreamingFixture::create` / `populate` (~208-260) builds
  its own SQLite world DB and cell cache in a temp dir (no game data); the `--streaming-fixture`
  systems (~301-366) drive traversal, teleports and rebases, and `validate_streaming_fixture`
  (~331-380) checks the result. Lifecycle checks: `validate_streaming_lifecycle` in
  `crates/engine/src/streaming.rs` (~1780-1849).
- First find out how the engine on main enters and leaves an interior (a cell with an interior id,
  how streaming treats interior cells: `cell_within_unload_radius` keeps interiors, see
  `unload_radius_removes_distant_exteriors_but_keeps_interiors` in streaming.rs tests). If main has
  no runtime path that switches the active space to an interior, say so in the report and implement
  the smallest honest version: the fixture DB gains one interior cell, the fixture loads it by id
  (the same loader path the engine uses for an interior start), leaves it for an exterior grid and
  back, and asserts root counts, no duplicates or orphans, and the unload of the interior when it is
  left (or the documented reason it is kept). Do not invent a door/portal system; that is out of scope.
- The dummy-content crate's fixture plugin has an interior cell and a linked load-door pair
  (`crates/dummy-content`); reuse its record layout if the fixture DB needs interior rows, but do not
  change `crates/dummy-content`.

# Scope
Owned files: `crates/engine/src/app.rs` (the streaming fixture and its validation only),
`crates/engine/src/streaming.rs` (only if the loader needs a hook the fixture can call; say why),
`docs/handoff/impl-620-p2-fixture-interior-crossing.md`. You may not run the engine with a window
or GPU; tests only.

# Done when
- A test (or the fixture's own validation, exercised by a test) crosses exterior to interior and
  back, and fails if the interior root is duplicated, orphaned or left loaded against the rule.
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p engine --all-targets -- -D warnings`
  `cargo nextest run -p engine`
- One or two commits, upstream style.

# Report
How main enters an interior (file:line), what the fixture now does, the test names, and anything
the plan item asks for that this does not cover.
