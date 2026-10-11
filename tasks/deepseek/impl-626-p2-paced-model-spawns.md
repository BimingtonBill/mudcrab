# Goal
Track: Phase 2 (for upstream). Spread the instantiation of newly loaded models over frames, so the
frames where streamed models arrive stop spiking on the main thread. Measure it first.

# Context
- Your checkout is the local branch `phase2/upload-throttle` (upstream main + paced cell unloads +
  the render-asset upload throttle). Upstream commit style (`perf(engine): ...`), no `[deepseek]`
  prefix, no project-internal references in code or commits.
- Research (read-only, appended below) found: `spawn_cell` (`crates/engine/src/streaming.rs`,
  ~555-578) inserts `WorldAssetRoot(asset_server.load(...))` on every reference with a model. When the
  glTF finishes loading, Bevy's scene spawner (`SceneSpawnerSystems::WorldInstanceSpawn`, in the
  SpawnScene schedule) instantiates EVERY ready scene in one unbudgeted batch; transform/visibility
  propagation and the engine's `track_asset_readiness` validation follow for all of them. On the
  stress scenario the main world peaks at 13-16 ms on those frames (p99 ~7 ms).
- Existing per-frame budgets to mirror: `max_cell_commits_per_frame` and `--max-unloads-per-frame`
  in `crates/engine/src/config.rs`, the `Retiring` pacer (`despawn_cells`) in streaming.rs.

# The work: three commits
1. Measurement only: a `scene/spawn_batch` span (two small systems in SpawnScene ordered before and
   after `SceneSpawnerSystems::WorldInstanceSpawn`, an `Instant` in a resource) and gauges
   `scene/instances_spawned_this_frame` (count of `Added<...>` scene instances; use whatever marker
   Bevy 0.19 puts on a spawned instance root, check the vendored bevy_scene source) and
   `assets/instances_completed_this_scan` (from `track_asset_readiness`). Show them in the profile
   summary next to the despawn lines.
2. Arming pacer: `spawn_cell` inserts a `PendingModel { handle }` (a new component) instead of
   `WorldAssetRoot`; a new system in the streaming chain moves at most
   `max_model_spawns_per_frame` pending models to `WorldAssetRoot` per frame, only once
   `Assets<...>` actually holds the loaded asset (a model still loading does not use a slot), oldest
   first (FIFO by spawn order). Config field + `--max-model-spawns-per-frame <n>` (0 = unlimited;
   choose a default from the numbers: ~15 models per cell, 1 cell commit per frame; say why).
   Gauge `streaming/arming_queue_depth` and a peak. Keep every invariant: each pending model becomes
   `WorldAssetRoot` exactly once, none is left pending forever while its cell stays resident, a
   model whose cell is unloaded or retired before arming simply disappears with its subtree (no
   dangling queue entry that panics), `PendingAssetProfile` and the readiness metrics still converge
   (`assets_ready` counts every instance once), the streaming fixture and lifecycle validator pass.
   Check every other place that reads `WorldAssetRoot` (e.g. ~624 and ~2236 in streaming.rs) still
   behaves for a pending-but-unarmed model.
3. Tests: at most N armed per frame; FIFO; a still-loading asset does not consume a slot; a pending
   model whose cell is despawned does not break the pacer; 0 = unlimited arms all; the queue drains.
- Do not change `track_asset_readiness`' completion logic beyond the gauge.

# Scope
Owned files: `crates/engine/src/streaming.rs`, `crates/engine/src/config.rs`,
`crates/engine/src/profiling.rs` (summary lines only), `crates/engine/src/app.rs` (only if a
fixture bound or system registration must change; say why), and
`docs/handoff/impl-626-p2-paced-model-spawns.md` (do not commit it). Tests only; no engine window/GPU.

# Done when
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p engine --all-targets -- -D warnings`
  `cargo nextest run -p engine`
- Three commits, as above.

# Report
Per commit: files, functions, tests. The default N and why. The spawned-instance marker you used
(vendored bevy_scene file:line). Anything in the research you found wrong.

# Appendix: research report (read-only worker; line numbers are upstream main)

<!-- research-625-p2-main-world-arrival.1 (research): Track: Phase 2. Find what the main world (Bevy's main schedule) does on the frames where streamed -->
Status: done
Verdict: answered
Summary: On an arrival frame the main world applies finished loads in PreUpdate, then Bevy's `spawn_queued_instances` (SpawnScene) instantiates **every** queued scene of every landed model in one unbudgeted batch via reflection `write_to_world_with`; PostUpdate propagates transforms/visibility for all new subtrees. Engine validation (`track_asset_readiness`, Update of the *next* frame, where `asset_ready` is emitted) also completes an unbounded number of instances per scan (measured ≤0.9 ms). Cell commits are already capped at 1/frame; the unbudgeted work is the spawn batch (best estimate 5–8 ms residual on 13–16 ms frames), validation completions, and load application. Best budget point: cap how many references get `WorldAssetRoot` inserted per frame (streaming.rs `spawn_cell` + a pacer); smallest first commit: cap completions per scan in `track_asset_readiness`.
Left: nothing in scope; no numbers for nodes-per-model (inferred ~8 entities/instance); SpawnScene itself has no span yet, so spawn-vs-validation split is inferred from residuals.
Next: instrument a span around `SceneSpawnerSystems::WorldInstanceSpawn` and a per-frame spawned-instance count (see Instrumentation) before landing the arming cap; treat `asset_ready` frames as possibly one frame behind the spawn spike.

## Arrival frame (ordered steps with file:line)

Engine paths are `origin/main`; Bevy paths are under `…/index.crates.io-1949cf8c6b5b557f/`. This Bevy's scene API is `WorldAssetRoot`/`WorldAsset` (the `SceneRoot` equivalent; `bevy_gltf-0.19.0/src/lib.rs:29` shows the same usage the engine has at `streaming.rs:566`). Main schedule order is First → PreUpdate → … → **Update → SpawnScene → PostUpdate → Last** (`bevy_app-0.19.1/src/main_schedule.rs:31-38`).

**Frame C — cell commit (budgeted: `max_cell_commits_per_frame`, one per frame):**
1. Update, `collect_cells` → `spawn_cell` (`crates/engine/src/streaming.rs:261`, `:418`): per reference with a model it inserts `WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0)…))` + `PendingAssetProfile` (`streaming.rs:555-578`). This is the engine's "scene handle insertion" — it happens at commit, while the model is still loading.
2. Same frame's SpawnScene: `world_instance_spawner` (`bevy_world_serialization-0.19.1/src/world_asset_spawner.rs:688`) sees `Changed<WorldAssetRoot>`, queues `spawn_as_child`; `world_instance_spawner_system` (`:565`) → `spawn_queued_instances` (`:440`) tries `spawn_sync_internal` (`:336`); while the asset is absent it gets `NonExistentWorldAsset` and is re-queued (`:492-494`).

**Frame A — the frame(s) the model lands** (all queued instances whose `WorldAsset` landed this frame):
1. **PreUpdate**: `handle_internal_asset_events` (`bevy_asset-0.19.1/src/lib.rs:426`) inserts the glTF's meshes/materials/images/`WorldAsset` into `Assets<T>` (decode already happened on IO threads; this is insert + event queueing). Unbudgeted: every load finished this frame is applied.
2. **Update**, streaming chain (`streaming.rs:52-63`, `.chain()`): `plan_cells`, `collect_cells` (optional commit, budgeted 1/frame) — `track_asset_readiness` (`streaming.rs:643`) validates instances whose *spawn* already happened (see step 4): `validate_spawned_transforms_and_bounds` (`:953`, full subtree walk via `accumulate_relative_bounds` `:1082`, per-node `validate_transform` + AABB) then `validate_spawned_asset` (`:1165`, per primitive: mesh/material lookup + `validate_standard_material` `:1218`, ≤7 image slots) — emits `asset_ready` (`:789`), **no per-frame cap on completions** — `track_surface_readiness`, `update_render_origin` (measured ~0), `validate_streaming_lifecycle` (`:1780`, O(cells+roots) every frame).
3. **SpawnScene** (`bevy_world_serialization-0.19.1/src/lib.rs:64-68`): `world_instance_spawner` + `world_instance_spawner_system` chained in `SceneSpawnerSystems::WorldInstanceSpawn`. `spawn_queued_instances` (`world_asset_spawner.rs:440-518`) instantiates **every** queued instance whose asset now exists — one `write_to_world_with` (`world_asset.rs:64`) per instance: spawn_empty per node, then per entity × per component a reflect-clone + `apply_or_insert_mapped` (archetype churn). **This loop has no budget — it instantiates all ready scenes in one frame.** Then `set_instance_parent_sync` (`:500`, ChildOf/Children per instance root) and `trigger_instance_ready_events` (`:522`, deferred `WorldInstanceReady`). The legacy `scene_spawner_system` runs after (`bevy_scene-0.19.0/src/lib.rs:1076-1081`, `SceneSpawn` after `WorldInstanceSpawn`) but is idle — the engine never uses `SceneRoot`.
4. Command flush (same frame, post-SpawnScene): `mark_world_instance_ready` observer (`streaming.rs:633`) sets `PendingAssetProfile.scene_spawned = true`.
5. **PostUpdate**: `Assets::<A>::asset_events` broadcast (`bevy_asset-0.19.1/src/lib.rs:661`); transform propagation `(mark_dirty_trees, propagate_parent_transforms, sync_simple_transforms)` (`bevy_transform-0.19.0/src/plugins.rs:37-46`) — dirty-tree, so cost ∝ new subtrees + moving cell roots only; visibility (`check_visibility`, bevy_camera) over the whole visible set; extraction handoff to the render world (its asset-prep side is what `RenderAssetBytesPerFrame` now throttles).

**Frame A+1 (or later)**: Update `track_asset_readiness` runs the validation passes for instances spawned in frame A (it needs `scene_spawned` plus `is_loaded_with_dependencies`, `streaming.rs:687`) — i.e. the measured `asset_ready` frames are **validation** frames, one frame behind the unbudgeted spawn batch. Material/mesh *component* setup happens inside step 3 (reflection insert of `Mesh3d`/`MeshMaterial3d` + required-component hooks); pipeline specialization is render-world. There is no `lights.rs`; render.rs per-frame systems (`animate_water_materials`, `update_water_reflection_camera`, `crates/engine/src/render.rs:333,382`) are O(water surfaces) and do no per-model work.

## Costs

| step | driver | limited? | estimate | measured/estimated |
|---|---|---|---|---|
| load apply (`handle_internal_asset_events`) | models finished that frame (meshes, image bytes) | no | ~0.1–0.5 ms per burst | estimated |
| cell commit (`spawn_cell`, `streaming.rs:418`) | 54 refs/cell + 4 terrain quadrant mesh builds | yes — 1 cell/frame | mean 2.8, worst 7.7 ms | measured |
| scene instance spawn (`spawn_queued_instances` → `write_to_world_with`) | instances × nodes × components (reflect clone+insert); ~8 entities/instance (184 entities/cell − 54 refs − scaffolding, ÷ ~15 instances) | **no** | 5–8 ms for a 10–20-instance batch (residual: 13–16 ms main_world − 0.9 scan − ≤7.7 commit) | estimated (residual) |
| parent sync + `WorldInstanceReady` observers | instance roots | no | µs each | estimated |
| transform propagation (PostUpdate) | new subtree nodes + cell-root motion | dirty-tree only | 0.1–0.5 ms on arrival frames | estimated |
| visibility check (PostUpdate) | all ~12k renderable entities every frame | no (always runs) | 0.2–0.5 ms | estimated |
| readiness scan `track_asset_readiness` (all validation passes) | ready instances × nodes (transform/matrix/AABB) × primitives × ≤7 image slots | **no** (unbounded completions/scan) | worst 0.9 ms whole scan | measured |
| `validate_streaming_lifecycle` | cells + roots | no | <0.2 ms at radius 3 | estimated |
| render.rs per-frame (water/reflection/metrics) | water surfaces | no | <0.1 ms | estimated |
| render-world extract/prep of new assets | new meshes/textures | now throttled (`--max-upload-mib-per-frame`) | worst fell 10.5–14.1 → 7.1–8.6 ms | measured |

## Budget options (ranked)

1. **Arming cap: insert `WorldAssetRoot` for at most N ready models per frame.** `spawn_cell` (`streaming.rs:555-578`) stops inserting `WorldAssetRoot` directly; it inserts `PendingModel { handle }`. A pacer system in `StreamingPlugin::build` (`streaming.rs:46`), in Update before SpawnScene, moves ≤N pending → `WorldAssetRoot` per frame when `Assets<WorldAsset>` contains the handle. This caps the spawn batch, the same-frame propagation/visibility delta, the next-frame validation burst, and per-frame new-entity extraction at once — every downstream cost scales with the batch. Invariants: every `PendingModel` becomes `WorldAssetRoot` exactly once (insert only when the component is absent; queue is append-only and FIFO-drained — no model missing forever); no double spawn (Bevy arms on `Changed<WorldAssetRoot>`, one insert = one instance); readiness metrics converge (`PendingAssetProfile` untouched, `track_asset_readiness` just skips unarmed references; `assets_ready` still increments once per instance; `validate_streaming_lifecycle` only tracks cell roots). Smallest first commit: the `PendingModel` swap in `spawn_cell` + one ~40-line pacer with N=2, pacer gauge in metrics.
2. **Validation completion cap in `track_asset_readiness`** (`streaming.rs:643`): complete at most N instances per scan, leave the rest pending (they keep `scene_spawned`, get finished next frame). Invariant: every pending instance is still examined each frame; `PendingAssetProfile` removed exactly on completion; metrics and `asset_ready` events still count every instance (only later). ~5 lines — the smallest first commit overall, but it only spreads the measured ≤0.9 ms scan; good as a first, cheap confirmation step.
3. **Budget the spawn drain itself** (≤N `spawn_sync` calls per frame from an engine system in SpawnScene, bypassing the component-driven queue). Equivalent pacing to (1) but must re-implement `WorldInstance`/parent bookkeeping; `world_assets_to_spawn` is `pub(crate)`, so capping `spawn_queued_instances` in place is not possible without a Bevy fork. Rank below (1) for invasiveness.
4. **Spread validation inside one instance** (node budget, partial summary in a component): only matters for very large single models. Invariant: failure at any partial point still hides the scene and records exactly once; empty-model detection (`:975-995`) runs before partial work.
5. **Distance/visibility-gate spawn or validation** (defer off-screen instances): weakest invariants — "no model missing forever" needs an aging/priority-by-distance guarantee; metrics converge only under revisits. Defer unless (1) is not enough.

Also flag (not streaming, but unbudgeted): the hot-reload path `update_spawned_instances` (`world_asset_spawner.rs:358`) despawns+respawns **every** instance of a modified asset in one frame ("a very heavy operation"). Keep asset watching off in stress runs; if watching ships, that path needs the same cap.

## Instrumentation

1. **Span `scene/spawn_batch`**: two tiny engine systems in SpawnScene ordered `.before(SceneSpawnerSystems::WorldInstanceSpawn)` / `.after(SceneSpawnerSystems::WorldInstanceSpawn)` (set is public via `bevy_app`), the first stashing an `Instant` in a resource, the second recording elapsed — this confirms or refutes the spawn batch as the 5–8 ms residual. Plus gauges `scene/instances_spawned_this_frame` (`Added<WorldInstance>` count) and `scene/entities_spawned_this_frame` (sum of instance subtree sizes via `WorldInstanceSpawner::iter_instance_entities`).
2. Gauge `assets/instances_completed_this_scan` from the existing `completed_this_scan` local (`streaming.rs:782` region).
3. Count `Added<WorldAsset>`, `Added<Mesh>`, `Added<Image>` per frame (one cheap PostUpdate system) to size the PreUpdate apply burst.
4. Join the tooling per-frame series on `scene/instances_spawned_this_frame` vs the `asset_ready` count: the spike should sit on the spawn frame A with a smaller validation echo on A+1 — this resolves the frame-lag attribution of today's summary.
5. Once a pacer exists: gauge `streaming/arming_queue_depth` (must fall to 0 outside bursts — the convergence check for "no model missing forever").

## Doubts

- The spawn-vs-validation split is inferred from residuals (readiness_scan measured ≤0.9 ms; SpawnScene never spanned). If option-2's cap alone removes the spike, the cost was validation bookkeeping not captured by the scan span — rerank accordingly.
- `track_asset_readiness` runs in Update, *before* SpawnScene, so validation is ≥1 frame after spawn; `asset_ready`-attributed frames in `crossing-ab/summary.txt` may lag the true spawn spike by one frame. The `World::commands().trigger` flush in the exclusive spawner system is assumed to run same frame (post-SpawnScene) — step-1 instrumentation settles it.
- ~8 entities/instance is arithmetic from the despawn-batch entity counts (184/cell), not a direct node count; per-model node/primitive counts would sharpen the spawn estimate.
- `mesh.compute_aabb()` (`bevy_camera-0.19.1/src/primitives.rs:22-25`) is O(1) only when `final_aabb` is cached on the mesh; if converted meshes lack cached AABBs, per-node validation is O(vertices) and would dominate the readiness scan — worth a one-off check of `final_aabb` presence per mesh.
- `is_loaded_with_dependencies` can push validation past the spawn frame if textures trickle in; an arming key of "asset present in `Assets<WorldAsset>`" (option 1) matches Bevy's spawn timing and avoids coupling pacing to textures.
- Legacy `scene_spawner_system` still runs each SpawnScene; assumed idle (engine uses `WorldAssetRoot` only), cost unmeasured but O(1) with empty queues.

