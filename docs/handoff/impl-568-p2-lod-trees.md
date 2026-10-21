# impl-568 — Phase 2 — distant LOD step 4b: tree billboards

Track: Phase 2. Branch `impl/568-lod-trees`, cut from `phase2/lod-engine-objects` (steps 3 and 4a).
Design: `local/research-557/DESIGN.md`, section 3 ("Tree billboards"), 4, 5, 6, 8 ("4b").

## What changed

- `crates/engine/src/world/database.rs`: a third `LodBlockKind::Trees` (never produced by
  `LodBlockKind::parse`, since `lod_block` carries no `trees` rows — its availability comes only
  from `lod_tree_instance`). `LodBlockTable::open` now also reads `lod_tree_type` (into a new
  `tree_types` map) and the distinct `(block_x, block_y)` pairs of `lod_tree_instance` (as level-4
  `Trees` keys), through the same dedicated connection, and treats either table being absent as
  "no trees" rather than an error (most fixtures, and every pre-existing database, have neither).
  `LodBlockPayload.mesh_path` is now `Option<String>` (`None` only for a trees payload) and gained
  `trees: Vec<LodTreeInstanceRow>` and `tree_unknown_type_instances: u64`. A new
  `load_lod_tree_block` reads one block's instances left-joined against `lod_tree_type`
  (`ORDER BY tree_index, pos_x, pos_z` for determinism), drops rows whose type is unknown and
  counts them, and errors (the same "table changed since availability was read" trap terrain/object
  blocks already have) only when the block has no rows at all. The worker dispatches to it for
  `LodBlockKind::Trees` requests.
- `crates/engine/src/config.rs`: `lod_trees_enabled` (default on, `--lod-trees off` alone turns it
  off), `lod_tree_distance` (default 75 000, `--lod-tree-distance <units>`), and
  `lod_tree_instances_per_frame` (default 512, no CLI flag — matches `lod_commits_per_frame` and
  `lod_requests_per_frame`, which also have none).
- `crates/engine/src/lod.rs`: the whole tree-billboard subsystem —
  - `LodBillboardCatalog` (tree_index → shared `Mesh`/`StandardMaterial` handles), filled once at
    Startup by `queue_tree_billboard_prototypes` (spawns each type's `Scene(0)` as a
    `TreeBillboardPrototype`) and `harvest_tree_billboard_prototype` (an observer on
    `WorldInstanceReady`, mirroring `mark_lod_instance_ready`'s pattern exactly: reads the first
    descendant's `Mesh3d`/`MeshMaterial3d<StandardMaterial>`, then despawns the prototype).
  - Trees are planned by the existing `plan_blocks` (the same nested-residency terrain uses; trees
    have one level, so nesting never applies but the covered-block pre-filter and hysteresis
    unload do), on their own single-entry band (`lod_tree_distance`), wired into `plan_lod_blocks`
    alongside the terrain and object plans.
  - `spawn_tree_block` spawns a hidden root (depth-lowered like a level-4 terrain block, *not* the
    tree band's own list — trees stand on the level-4 LOD surface regardless of how far the tree
    band itself reaches) carrying a `PendingTreeBlock` queue of its instance rows.
  - `spawn_tree_instances` drains that queue a few at a time (`lod_tree_instances_per_frame`, and
    the shared commit-budget time window), spawning one child entity per instance sharing the
    type's catalogue handles, `NotShadowCaster`, no `Name`. A block whose type has not resolved in
    the catalogue yet is left alone that frame rather than dropped. The root is shown, and
    `PendingTreeBlock` removed, only once every row has spawned.
  - Placement goes through the shared coordinate contract (`tree_instance_translation`: Creation
    position via `creation_to_runtime_vector`, minus the block's own south-west corner in the same
    basis, so the child's translation is small regardless of where the block sits in the world; yaw
    via `creation_euler_to_runtime_quaternion([0,0,rotation])`).
  - Hiding is per instance: each instance's absolute cell is computed once at spawn
    (`tree_instance_cell`) and stored on it; `update_lod_tree_visibility` hides/shows against the
    same `LoadedCells` the terrain clip and object segments use, but only walks the resident
    instances when the camera's cell has actually changed (`tree_cull_should_run`/`TreeCullState`),
    not every frame.
  - `track_lod_readiness`'s and `validate_lod_lifecycle`'s per-kind matches gained a `Trees` arm
    (the former is a documented no-op: a tree block never carries `PendingLodBlock`, it has no
    scene to wait on).
- `crates/engine/src/streaming.rs`: `StreamingMetrics.lod_tree_blocks_resident` (set in
  `validate_lod_lifecycle`, alongside the existing `lod_object_blocks_resident`); the placeholder
  `lod_tree_instances` counter from step 4a's scaffolding is now actually incremented.
- `crates/engine/src/app.rs`: **unchanged**. See "Left out" below.

## Tests

*Database (`world::database::tests`)*
- `loads_a_tree_block_with_every_instance_in_its_rectangle_ordered` — a synthetic block with one
  unknown-type instance among three known ones proves the `ORDER BY tree_index, pos_x, pos_z`
  ordering and the per-block scoping (a neighbouring block sees only its own row).
- `a_tree_instance_of_an_unknown_type_is_reported_and_does_not_fail_the_block` — the block still
  succeeds with its usable instances; the dropped one is counted, not fatal.
- `a_lod_request_for_a_missing_tree_block_reports_an_error_without_stopping_the_worker` (sync) and
  `a_lod_request_for_a_missing_tree_block_reports_an_error_through_the_worker` (through
  `WorldDatabase`'s async channel) — the same "table changed since availability was read" trap,
  extended to `Trees`; a missing tree block errors without starving the cell channel behind it.
- `a_lod_request_for_a_present_tree_block_answers_with_its_instances` — the async round trip for
  the success path.
- `a_tree_block_appears_in_the_table_only_when_it_has_instances` — `LodBlockTable::open` derives
  `Trees` keys strictly from `lod_tree_instance`, never from `lod_block`; also proves
  `tree_type_mesh_path`.
- `a_lod_block_table_with_no_tree_tables_reads_as_having_no_trees` — the base fixture (no
  `lod_tree_type`/`lod_tree_instance` tables at all, i.e. every pre-existing database) opens fine
  and reports zero tree keys/types.
- `loads_a_lod_block_row_with_its_bounds_and_mesh_path` extended to assert `payload.trees`/
  `tree_unknown_type_instances` are empty/zero for a terrain block.

*Config (`config::tests`)*
- `keeps_distant_lod_off_until_it_is_asked_for` extended: `lod_trees_enabled` defaults on,
  `lod_tree_distance` 75 000, `lod_tree_instances_per_frame` 512.
- `lod_trees_off_disables_only_the_tree_tier` — mirrors the existing objects test: `--lod-trees
  off` alone leaves the terrain/object tiers untouched; a bare `--lod-trees` doesn't swallow the
  next flag; `--lod` alone keeps trees on.
- `parses_lod_tree_distance` — parses, and an invalid/non-positive value keeps the default.

*Pure (`lod::tests`, no `App`)*
- `a_tree_block_is_keyed_to_its_level_four_block_on_the_worldspace_grid` — `plan_blocks` reused
  as-is for `LodBlockKind::Trees` selects the near block and excludes the far one on distance.
- `tree_band_off_unloads_every_resident_tree_block` — an empty band list (what `plan_lod_blocks`
  passes when `lod_trees_enabled` is false) unloads everything and wants nothing.
- `a_tree_instance_translation_is_relative_to_its_block_corner` — matches the shared contract's own
  basis directly at the Creation origin, and checks the corner subtraction for an off-origin block.
- `a_tree_cull_pass_does_not_run_while_the_camera_cell_is_unchanged` — the gate predicate in
  isolation (unchanged → false, changed → true, first pass with no prior cell → true).

*App (`lod::tests`, a bare `App::new()`, no full asset pipeline)*
- `a_tree_instance_inside_the_full_detail_square_is_hidden_and_one_outside_is_not` — runs the real
  `update_lod_tree_visibility` system: an instance in a loaded cell is hidden, one outside is not,
  and it shows again once the cell leaves (the cull-state reset that step needs is called out in
  the test as standing in for a camera move, which is what re-arms the gate for real — see the
  previous test).
- `spawn_tree_instances_respects_the_per_frame_cap_and_shows_the_root_once_complete` — runs the
  real `spawn_tree_instances` system against a hand-built `PendingTreeBlock` (5 rows) and
  `LodBillboardCatalog` (real `Mesh`/`StandardMaterial` handles, no glTF loading needed): a cap of
  2 spreads the block across three frames, the root stays `Visibility::Hidden` and keeps its
  `PendingTreeBlock` until the last frame, then shows and drops it.

`cargo test -p engine` (111 passed, 1 pre-existing `#[ignore]`d real-data test, 0 failed) and every
pre-existing LOD test (terrain, objects, `--lod-fixture`) is unchanged and still passes.

## Left out, and why

**No real-GLB round trip through `--lod-fixture`.** I first wrote an app.rs test that augmented an
existing `--lod-fixture` directory with a real tree GLB (reusing `write_fixture_glb`) and drove the
whole pipeline — planning, the async DB worker, `LodBlockTable`, the catalogue harvest observer,
and spawning — end to end. It hung forever with the prototype's asset load state stuck at
`(Loading, Loading, Loading)`. The cause: `lod_fixture_app`'s plugin list has no `ScenePlugin`, so
the `SpawnScene` schedule `bevy_world_serialization`'s `world_instance_spawner` system runs in (and
therefore `WorldInstanceReady` itself) never executes in this harness. This is **not new** — the
existing comment on `a_lod_block_that_cannot_be_read_increments_lod_asset_failures_once` already
says "a headless app never finishes an asset load (the loader's tasks stay `Loading`)" — but it is
the first time anything in this tier actually needed `WorldInstanceReady` to fire to prove itself
(terrain/object blocks reach `LodBlockStatus::Resident` the moment their DB row commits, independent
of whether their scene ever finishes spawning, so no existing test noticed). In real play
`DefaultPlugins` includes `ScenePlugin`, so `harvest_tree_billboard_prototype` (which is a
line-for-line mirror of the already-proven `mark_lod_instance_ready`) will fire correctly; verifying
that in this fixture harness would mean adding `ScenePlugin` to `lod_fixture_app` itself, which is
bigger than "tree rows" and risks retiming the four existing `--lod-fixture` tests that pass today
without it. I left `app.rs` untouched and reported this instead of fixing the harness under this
task's scope. **Follow-up worth doing** (not attempted here): add `bevy::scene::ScenePlugin` to
`lod_fixture_app` and confirm the existing terrain/object fixture tests still pass with scenes
actually finishing — that would also be the first real test of `track_lod_readiness`'s bounds/
material-rewrite code paths, which today likely also never run to completion in this harness.

**Everything else in the design landed as written.** No other deviations.

## Next

Nothing else queued from this task. The design's step 4c (real-data validation against
`SkyrimConverted-upstream15`, screenshots, `phase2-acceptance.ps1` wiring) is explicitly out of
scope here and is the lead's to run.
