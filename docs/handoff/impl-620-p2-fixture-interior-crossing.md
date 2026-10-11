# Streaming fixture: exterior/interior crossing (M5.1)

Plan item: `docs/roadmap/02-completion-plan.md`, Milestone 5, implementation item 1 — the streaming
fixture must cover fast traversal, teleport, **exterior/interior change** and repeated rebasing. It
covered everything but the interior change.

## How this tree enters an interior today

There is no runtime path that switches the active space to an interior. Interiors exist in the
loader and in the unload rule, not in the planner:

- `crates/engine/src/world/database.rs:28` — `CellKey::Interior(u32)`; `load_cell` resolves an
  interior by cell id (`:262`) and reads its `references` by `cell_id`, with no spatial lookup
  (`:288`).
- `crates/engine/src/streaming.rs:1418` — `cell_translation` places an interior root at the origin;
  `spawn_cell` (`:444`) places its references in cell-local coordinates.
- `crates/engine/src/streaming.rs:434` — `cell_within_unload_radius` returns `true` for every
  interior, so a loaded interior is never dropped by the camera's radius.
- `crates/engine/src/streaming.rs:229` — `plan_cells` builds its wanted set from `stream_radius`
  around the camera and only ever inserts `CellKey::Exterior`. Nothing requests an interior. No
  door, teleport or `XTEL`-driven cell change exists in the engine on this tree.

So the fixture does the smallest honest thing the brief allows: it loads the interior **by id**
through the loader path the planner uses, then asserts the rule this tree actually has (interiors
are kept) rather than inventing a door crossing or an unload that no runtime path performs.

## What the fixture now does

- `StreamingFixtureDirectory::populate` (`crates/engine/src/app.rs:238`) writes one interior cell
  into the fixture database: `cells(id, worldspace_id, grid_x, grid_y, interior_name)` with
  `grid_x`/`grid_y` NULL and a name, matching the converter's shape (`esm/exporter.rs:317`), and one
  reference inside it with no `statics` row, so the fixture still needs no converted assets.
  Ids: `STREAMING_FIXTURE_INTERIOR_CELL_ID` `0x0001_0000` and its reference (`app.rs:211`).
- `cross_streaming_fixture_interior` (`app.rs:412`, `PreUpdate`, chained after
  `drive_streaming_fixture`) requests that interior at frame 8 through
  `StreamingWorld::request_cell` — the same submission `plan_cells` uses, extracted to
  `streaming.rs:79` so both share one definition. It then observes every later frame: how many
  `StreamedCellRoot` entities carry the interior's cell id, and whether the camera stands more than
  `unload_radius` cells from the grid the interior was loaded on (the camera's centre comes from
  `streaming_center`, now one definition for `plan_cells`, the lifecycle validator and the fixture).
- `validate_streaming_fixture` (`app.rs:463`) adds the crossing to its contract:
  `interior_crossing_valid` (`app.rs:344`) — requested, became resident, at least one observation
  from outside the unload radius, exactly one root on every such observation, and exactly one root
  and one reference at validation time. Duplicates and missing roots are the lifecycle validator's
  (`streaming.rs:1806`), which the run already requires to stay at zero.

The camera keeps crossing exteriors throughout: teleports at frames 6, 14 and 22 carry it 18–30
cells away from where the interior was loaded, and frame 30 returns it to the starting grid, which
restreams a full exterior ring before the fixture settles.

## Tests

- `app::tests::streaming_fixture_loads_its_interior_by_id_and_keeps_it_while_the_camera_crosses_exteriors`
  (`app.rs:1677`) — runs the fixture's own systems over its own database headless (no window, no
  GPU, no game data) and asserts the crossing observations, one interior root, one interior
  reference, zero duplicate/orphaned/missing roots, zero lifecycle invariant failures, and that the
  fixture's own validation accepted the run (`streaming_fixture_validated`). Headless frames would
  outrun the database worker by two orders of magnitude, so the loop pauses 1 ms per frame, which is
  the pacing a vsynced acceptance frame gives the worker.
- `app::tests::interior_crossing_fails_on_a_duplicated_orphaned_or_unloaded_interior` (`app.rs:1747`)
  — feeds `interior_crossing_valid` a second root, a second reference, no load, a load that
  disappeared, a root count below one on an observation while the camera was away, and no
  observation from outside the unload radius at all, and rejects each.

## Gates

`cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets -- -D warnings` and
`cargo nextest run -p engine` (54 tests) all pass. The fixture test is the slow one at ~21 s
(most of it outside the frame loop); every other test is under 0.2 s.

## Not covered

- The interior is never **unloaded**: leaving one is a `CellKey::Interior` removal that no runtime
  path performs yet, so the fixture asserts the current rule (kept while resident) instead of a
  removal. When a door/space change lands, the fixture's away-samples become the check that the
  interior root disappears exactly once on leave, and `interior_crossing_valid` is where the
  expectation changes.
- Nothing here exercises `XTEL` links, arrival positions or interior terrain: the fixture database
  carries no `LAND` and the interior has no door pair (per the brief, doors are out of scope).
