# impl-626-p2-paced-model-spawns — paced model spawns (Phase 2, for upstream)

Branch/worktree: `local/impl/impl-626-p2-paced-model-spawns`, base `cb917c9` (upstream main +
paced cell unloads + the render-asset upload throttle). Not committed: this note.

## What landed (three commits)

1. `456ad58` **perf(engine): measure the scene spawn batch a stream of models lands in** —
   `crates/engine/src/streaming.rs`, `crates/engine/src/profiling.rs`.
   `StreamingPlugin` adds two systems to `SpawnScene`, ordered `.before` / `.after`
   `SceneSpawnerSystems::WorldInstanceSpawn`, with an `Instant` in the new `SceneSpawnBatch`
   resource. They record the span `scene/spawn_batch` and gauge
   `scene/instances_spawned_this_frame`, `scene/instances_armed_this_frame` and (from
   `track_asset_readiness`) `assets/instances_completed_this_scan`. `StreamingMetrics` gained
   `instances_spawned_this_frame/max_…`, `instances_armed_this_frame/max_…`,
   `instances_completed_this_scan/max_…`; the summary reports the maxima next to the despawn lines.
   Test: `the_spawn_batch_counts_the_instances_bevy_instantiated_this_frame`.

2. `c59a28e` **perf(engine): arm streamed models a budget at a time** — `streaming.rs`, `config.rs`,
   `profiling.rs`. `spawn_cell` inserts `PendingModel { handle, sequence }` (sequence from
   `StreamingWorld::next_model_sequence`, assigned in spawn order) instead of `WorldAssetRoot`;
   `arm_pending_models` — new in the Update chain between `collect_cells` and
   `track_asset_readiness` — moves at most `max_model_spawns_per_frame`
   (`--max-model-spawns-per-frame`, 0 = unlimited) pending models to `WorldAssetRoot` per frame.
   The budget is spent on models whose scene is in `Assets<WorldAsset>` (the condition the spawner
   retries on) and skipped over ones still loading; ready models are armed oldest first. A failed
   load is armed too, so the readiness scan still records the failure. Metrics
   `arming_queue_depth` / `peak_arming_queue_depth` (peak = the backlog the pacer was handed),
   gauges `streaming/arming_queue_depth`, `streaming/peak_arming_queue_depth`, span
   `streaming/arm_models`, counter `streaming/models_armed`; summary line "Arming queue depth".

3. `b1907cf` **test(engine): the arming budget paces a cell without losing a model** — `streaming.rs`
   tests only: budget-per-frame and drain, FIFO (entities spawned newest-first), a loading model
   spends no slot and takes its turn later, a despawned cell leaves nothing queued, budget 0 arms
   everything, and paced models are spawned/validated exactly once. `model_app()` now runs
   `(despawn_cells, arm_pending_models, track_asset_readiness).chain()`; `settle_readiness` also
   waits for the arming queue to drain.

## The default, 4

A cell holds ~15 model references and cells commit one per frame, so 15 models is the largest
batch a frame is handed at once; the research measured 5–8 ms to instantiate 10–20 instances
(~0.4 ms each). 4 per frame keeps a spawn batch near 1.5 ms (~10 % of a 60 fps frame) and arms a
whole cell within four frames (~67 ms at 60 fps). Trade-off: during a burst (a teleport, several
cells committing in a row) the queue grows at up to 15 per frame and drains afterwards, so a big
jump's models pop in over a second or two. `--max-model-spawns-per-frame 0` is the old behaviour if
that is not wanted.

## The spawned-instance marker (source-checked)

`bevy_world_serialization-0.19.0` (the version in `Cargo.lock`):

- `WorldInstance` (`src/world_asset_spawner.rs:685`) is inserted **on the reference entity**, by
  `world_instance_spawner` (`:706`), when the reference *joins the spawner's queue* — whether or not
  the asset has landed. `Added<WorldInstance>` therefore counts *armed*, not spawned, on the
  pre-pacer code. That is why the spawn gauge is not built on it.
- The instantiation itself is `spawn_queued_instances` (`:440`) → `spawn_sync_internal` (`:336`) →
  `write_to_world_with`, and the instance root is attached to the reference by
  `set_instance_parent_sync` (`:501`, `world.entity_mut(parent).add_child(entity)`). So a reference
  that gained a child this frame is a model that was instantiated this frame:
  `scene/instances_spawned_this_frame` counts `(With<WorldInstance>, Changed<Children>)`, a direct
  world mutation the closing system sees without depending on when command queues flush.
- `Added<WorldInstance>` is kept as the separate `scene/instances_armed_this_frame` gauge; the test
  confirms both are visible to the after-`WorldInstanceSpawn` system on the spawn frame.
- `SceneSpawnerSystems` is `bevy::app::SceneSpawnerSystems` (`bevy_app-0.19.0/src/main_schedule.rs:204`).

## What the research got wrong / what is only read, not run

- Research §Instrumentation suggested `Added<WorldInstance>` as the spawned count; per the source it
  counts queue insertion, one frame early on the pre-pacer code. Fixed as above.
- The `~624` / `~2236` `WorldAssetRoot` readers in the brief are upstream-main line numbers. In this
  checkout `WorldAssetRoot` appears only in `spawn_cell` (now `PendingModel`), the `PendingAssetQuery`
  of `track_asset_readiness`, and the test fixtures; nothing else reads it, so nothing else had to
  learn about a pending-but-unarmed model. `validate_streaming_lifecycle` only tracks cell roots.
- Not run here (engine window/GPU is out of scope for this task): the stress profile numbers, the
  `--streaming-fixture` run, and the lifecycle validator on a real run. They are the acceptance step
  and should be re-measured before this is handed upstream.
- The `Changed<Children>` count can in principle count a diagnostic fallback child added to a
  reference (the failure path, `--diagnostic-asset-fallbacks`); it is one entity per failing
  reference and only on frames that record a failure as well.

## Next steps for the lead

- Re-run the stress scenario with the three commits and read `scene/spawn_batch` vs the old
  arrival-frame p99; tune `max_model_spawns_per_frame` from those numbers.
- If pop-in lag matters more than the spike, either raise the budget or add a backlog bound with an
  overflow escape (the `retire_backlog_bound` pattern) so a long teleport drains faster.
