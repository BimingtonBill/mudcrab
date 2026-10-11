# Goal
Track: Phase 2 (for upstream). Stop the worst-frame spike when the streaming camera crosses into a
new cell: measure the cell despawns that happen then, and spread them over frames under a budget,
reviving a cell that becomes wanted again before its turn instead of despawning and reloading it.

# Context
- Your checkout is cut from upstream `main`. Upstream commit style (`perf(engine): ...`,
  `feat(engine): ...`, `test(engine): ...`), no `[deepseek]` prefix, no project-internal references in
  code or commits.
- Research (appended at the end of this brief; its line numbers are upstream main, which is your checkout).
  Findings: on a crossing, `plan_cells`' retain (`crates/engine/src/streaming.rs` ~231-242) despawns
  ~7 whole cell subtrees (~1,500 entities) in one frame, with no span around the work; frame worst is
  ~29.9 ms against p50 6.6 ms in the stress scenario; the commit side is already budgeted
  (`max_cell_commits_per_frame`, `config.rs`). Auto-flight reverses every 4 cells, so recently
  despawned cells get reloaded (504 requests for 64 crossings).
- `crates/engine/src/metrics.rs` keeps `frame_ms` per frame (~line 36, push ~119) but writes only
  percentiles.

# The work: three commits
1. Measurement only (no behaviour change): time the despawns as one batch per frame in a
   `streaming/cell_despawn` span; record a `max_despawns_per_frame` gauge and a
   `despawned_entities` counter (count the subtree entities if cheap, otherwise the roots, and say
   which); persist per-frame `(frame_index, frame_ms)` into `frame-metrics.json` alongside the
   existing percentiles, so spikes can be joined with timeline events. Keep the JSON additive (new
   fields only).
2. Paced unloads: `CellStatus::Retiring { root }`; the retain pass marks out-of-range cells
   `Retiring` instead of despawning (the map entry stays, so no double request); a new system in
   the streaming chain despawns at most `max_cell_unloads_per_frame` (config field + a
   `--max-unloads-per-frame` flag, default 2) and also despawns everything at once when the backlog
   exceeds two rings (say the bound you chose). A `Retiring` cell that becomes wanted again goes back
   to `Resident` with the same root (no new DB request). Update `validate_streaming_lifecycle` and
   any fixture bounds so `Retiring` roots within the lag bound are not counted as out-of-range.
   Add a retire-backlog high-water metric.
3. Tests for both: a crossing produces at most N despawns per frame and all complete within
   ceil(7/N) frames; a reversal before completion revives with no new `requests_submitted`;
   `streaming_invariant_failures == 0`; the backlog overflow path despawns all; the frame-metrics
   JSON has the per-frame series.
- Do not change `update_render_origin` (the rebase) in this task.

# Scope
Owned files: `crates/engine/src/streaming.rs`, `crates/engine/src/config.rs`,
`crates/engine/src/metrics.rs`, `crates/engine/src/profiling.rs` (only if the span/gauge API needs
it), `crates/engine/src/app.rs` (only the streaming-fixture bounds, if they must change), and
`docs/handoff/impl-619-p2-paced-cell-unloads.md`. Say in the report if you need anything else.
You may not run the engine (no GPU work); tests only.

# Done when
- These pass in your checkout:
  `cargo fmt --all -- --check`
  `cargo clippy -p engine --all-targets -- -D warnings`
  `cargo nextest run -p engine`
- Three commits, as above.

# Report
Per commit: files, functions, test names. The default budget and the overflow bound, with the
reason. Anything in the research you found wrong.

# Appendix: research-615 report

# Crossing frame, costs, and two designs (upstream `main`)

Line numbers below are `origin/main` for `app.rs`/`render.rs` (working tree differs by bundle merges; verified with `git diff origin/main --stat`). `streaming.rs`, `world/*`, `config.rs`, `metrics.rs`, `profiling.rs` are byte-identical to `origin/main`, so working-tree numbers apply. Bevy citations are to vendored `bevy_ecs-0.19.1`.

## Crossing frame (ordered steps)

Trigger: `fly_camera` moves the camera (`app.rs:1366-1418`; auto-flight bounces within ±4 cells, `AUTO_FLIGHT_HALF_SPAN = CELL_SIZE*4.0`, `app.rs:1333`, `bounded_auto_flight_direction` `app.rs:1346-1363`). A "crossing" is when `floor(global/CELL_SIZE)` changes (`streaming.rs:194-199`) — equivalently when `trunc(camera_local/CELL_SIZE) != 0` (`streaming.rs:1753-1756`). All six streaming systems run chained in `Update` (`streaming.rs:52-62`) on the ECS executor; `Commands` apply at auto-inserted `ApplyDeferred` sync points between them (`bevy_ecs-0.19.1/src/schedule/auto_insert_apply_deferred.rs:17-53`). The only off-executor work is the SQLite worker thread (`world/database.rs:150-153`) and asset IO (`IoTaskPool`, `app.rs:191-200`). Note: `fly_camera` (`app.rs:128`) has no ordering vs. the chain — a crossing is handled the same frame or the next.

On a crossing frame (steady 7×7 window, `--stream-radius 3` → `unload_radius 4`, `config.rs:109-113`):

1. `plan_cells` (`streaming.rs:179-258`) recomputes the center; the wanted set gains the leading column and drops the trailing one. 7 new `DatabaseRequest::Load` sends (main thread, channel send only) — `streaming.rs:210-230`; each gets a fresh generation (`212-213`) recorded in `StreamingWorld` (`67-71`).
2. `plan_cells` retain pass (`streaming.rs:231-242`): ~7 keys fall outside `cell_within_unload_radius` (`408-415`) and `commands.entity(root).despawn()` is queued for each — **recursive** despawn of each whole cell subtree (bevy doc: also despawns `Children` descendants, `bevy_ecs-0.19.1/src/world/entity_access/world_mut.rs:1796-1803`; `EntityCommands::despawn` `system/commands/mod.rs:1906-1918`). Main thread.
3. `ApplyDeferred` executes those despawns between `plan_cells` and `collect_cells` — ~1,500 entities removed (see Costs), per-cell unique terrain meshes/materials dropped, render-world entities removed. **No span covers this.** Main thread.
4. `collect_cells` (`streaming.rs:261-406`): at most `max_cell_commits_per_frame = 1` commit (`280`, `config.rs:48`). If a response is ready: terrain snapshot validation + edge registration (`320-346`), then `spawn_cell` (`347-359`, `418-584`): root + 4 terrain quadrants (mesh built at `449-451`, `build_terrain_quadrant_mesh` `1545-1643` — 289 verts/512 tris, 6 attributes), water plane (`479-526`), and ~54 reference entities with model loads (`528-579`). The `with_children` closure runs eagerly while queuing (`bevy_ecs-0.19.1/src/relationship/related_methods.rs:419-426`), so this work is inside the `cell_commit` span; entity materialization happens at the next `ApplyDeferred`. Main thread; DB query already ran on the worker thread (`database.rs:202-245`).
5. `track_asset_readiness` (`streaming.rs:643-802`): scans all pending instances, full transform/bounds/material validation for any scene that finished this frame (`688-765`). Main thread.
6. `track_surface_readiness` (`streaming.rs:804-890`): flips terrain/water `Visibility` once textures are ready (`826-827`, `863-864`). Main thread.
7. `update_render_origin` (`streaming.rs:1742-1778`) — runs on exactly the crossing frames: shifts `RenderOrigin` (`1760`), rewrites the camera translation (`1761-1762`), then **rewrites the `Transform` of every exterior cell root** (`1763-1769`, 56 roots). `Mut` writes dirty every root → all descendants re-propagate `GlobalTransform` and re-extract to the render world this frame. Main thread.
8. `validate_streaming_lifecycle` (`streaming.rs:1780-1849`): rebuilds root sets, checks duplicates/orphans/missing/out-of-range (`1804-1827`). Main thread.
9. Later frames: the 7 responses trickle in at 1 commit/frame (queue wait mean 38 ms, `db_queue_wait`), glTF scenes spawn and validate (`assets/model_ready` wall mean 293 ms — time-to-ready, not CPU).

Measured evidence of the burst (stress A/B `main-1`, 60 s, 8521 frames, 5800X/3090): 64 `origin_rebases`, 504 requests, 504 commits, 448 unloaded = exactly 7 unloads per rebase. Timeline frame 61 = 7×`unloaded` (grid_y=-4 column) + `origin_rebased` on the same frame (`stress-ab/runs/main-1/streaming.json:1107-1158`); frame 139 repeats. So despawn burst, window slide, and rebase all land on one frame.

**Correcting the brief's known-so-far:** the rebase does *not* unload/reload cells — it re-places roots in place (`streaming.rs:1763-1769`); this has been true since `update_render_origin` was introduced (`086407c`). What unloads and reloads is the sliding window (`streaming.rs:210-242`), which is phase-locked to the rebase because both key off the cell crossing. Additionally the bounce pattern (auto-flight reverses every 4 cells, `app.rs:1333/1346`) makes the window *re-load* recently despawned cells: 504 requests / 64 crossings ≈ 7.9 per crossing, more than the 7 fresh cells a single crossing needs.

## Costs

| Step | Cost | Measured/Estimated | Source |
|---|---|---|---|
| fly_camera | 0.001 ms mean, 0.0025 ms p99 | measured | `world/fly_camera` span, `cpu-spans.json` |
| plan_cells: 7 request sends + 7 despawn enqueues | 0.014 ms mean, 0.64 ms worst | measured | `streaming/plan_cells` span |
| **ApplyDeferred: despawn 7 subtrees** (~1,500 entities, 28 unique terrain meshes + materials, handle drops, render removals) | est. 2–6 ms | **estimated — no span** | `streaming.rs:238`; counts derived: 448/64 unloads, entity_count 12473 / 56 cells ≈ 223 entities/cell |
| collect_cells commit (terrain validation + spawn_cell incl. 4 mesh builds, 54 refs) | 2.8 ms mean, 7.7 ms worst (on ~6% of frames) | measured | `streaming/cell_commit`, `streaming/frame_commit`, `streaming/spawn_cell` (0.27 mean/2.3 worst), `streaming/terrain_mesh` (0.022×4) |
| ApplyDeferred: materialize spawned subtree (~1,000 entities) | est. 0.5–1.5 ms | estimated | spawn closure eager at `related_methods.rs:419-426`; apply inserts entities |
| track_asset_readiness scan | 0.014 ms mean, 0.94 ms worst | measured | `assets/readiness_scan` span |
| track_surface_readiness | <0.05 ms | estimated | `streaming.rs:804-890`, O(pending) |
| update_render_origin rewrites (camera + 56 roots) | 0.003 ms mean, 0.006 ms worst | measured | `streaming/render_origin_rebase` span |
| **Rebase downstream: GlobalTransform propagation + render re-extract of ~12k entities** | est. 1–4 ms | **estimated — no span** | `streaming.rs:1763-1769`; entity_count 12473 |
| validate_streaming_lifecycle | est. <0.1 ms | estimated | `streaming.rs:1780-1849` |
| DB query per new cell (worker thread) | 2.1 ms mean, 60 ms worst | measured, off-thread | `streaming/db_query` |
| Frame totals | p50 6.57 ms, p95 11.42, p99 14.40, worst 29.88 | measured | `frame-metrics.json` |
| Steady-state composition | 56 resident cells, 54 refs/cell, ~14.8 model instances/cell, 8.6 nodes/instance | measured (derived ratios) | `streaming.json` aggregate/counters |

Frame-band inference (derived, not measured): 504 commit frames = 5.9% of frames ≈ the p95 band; 64 crossing frames = 0.75% ≈ the p99/worst band. The earlier A/B "fix" cut commit cost 7.67→1.1 ms worst yet frame p95 stayed 11.4→12.9 ms and worst 29.9→29.7 ms (`stress-ab/runs/results.json`) — the spike is not the commit path, consistent with the despawn/rebase churn being unmeasured.

**Instrumentation that would pin this down** (none of it exists today): (a) `metrics.rs` already collects `frame_ms: Vec<f64>` (`metrics.rs:34-36`, push at `119`) and throws it away after percentiles (`155-162`, `238-242`) — persist `(frame_index, frame_ms)` and join with `TimelineEvent.frame` (`profiling.rs:269-271`, counter `90-95`); (b) a `streaming/cell_despawn` span around the actual despawn and a `max_despawns_per_frame` gauge; (c) spans for transform propagation and render extraction.

## Design 1 — spread despawns across frames under a budget

Files/functions:
- `config.rs`: add `max_cell_unloads_per_frame: usize` (default 1–2) beside `max_cell_commits_per_frame` (`config.rs:11,48`) and a `--max-unloads-per-frame` flag in `from_args` (`config.rs:84-206`).
- `streaming.rs`: new `CellStatus::Retiring { root: Entity }` (`169-173`). In `plan_cells`' retain (`231-242`), flip out-of-range cells to `Retiring` instead of despawning; keep the map entry so the request test at `211` cannot double-request. New system `despawn_retiring_cells` in the chain (`52-62`) despawns at most N per frame, timing each with a `streaming/cell_despawn` span and recording `max_despawns_per_frame`. **Revive rule:** if a `Retiring` cell becomes wanted again before its despawn runs, promote it back to `Resident` (same root) — kills unload/reload thrash on flight reversals.
- `streaming.rs` `validate_streaming_lifecycle` (`1811-1826`): restate the invariant as "no root beyond `unload_radius` + retire-lag bound" (count `Retiring` roots as allowed for ≤ N frames), else the existing `out_of_range_cell_roots` gate fails. Same for the fixture bound `maximum_resident` (`app.rs:335`).
- `streaming.rs` `StreamingMetrics` (`73-139`): add retire backlog high-water mark.

Invariants: *no cell drawn twice* — exactly one root per CellKey at all times (`Resident` or `Retiring` holds it; duplicate check `1804-1808` unchanged; revive reuses the root; stale DB responses still discarded by generation `307-316`). *No hole in view* — only cells outside `unload_radius = stream_radius + 1` (`config.rs:112`) ever retire, and cells draw out to `unload_radius` (`app.rs:1302-1305`), so at least one resident ring always buffers the view. *Commit budget* — despawns get their own budget; commits stay at `max_cell_commits_per_frame`/`max_commit_micros_per_frame` (`config.rs:48-49`, `commit_budget_exceeded` `streaming.rs:36-38`).

Risk: retire backlog under sustained fast flight — at 1 despawn/frame and ~7 cells per crossing the queue can outgrow the ring (entity/memory growth up to backlog × ~223 entities). Mitigate with a microsecond budget instead of a count, or an overflow rule (batch-despawn everything when backlog > 2 rings). Also `peak_resident_cells`/fixture bounds read higher until updated.

Test: unit test beside `unload_radius_removes_distant_exteriors_but_keeps_interiors` (`streaming.rs:1907`): simulate a crossing, assert ≤ N despawns per frame, all 7 complete within ceil(7/N) frames, `streaming_invariant_failures == 0`, and a reversal before completion revives (no new `requests_submitted`). Integration: extend `validate_streaming_fixture` (`app.rs:331-380`, driven by teleports at `app.rs:301-330`) with `max_despawns_per_frame <= budget`; then the stress A/B should show `frame_ms_worst`/p99 down (p95 likely unchanged — it is commit-bound).

## Design 2 — rebase the world origin without unloading and reloading cells

Baseline to preserve: the rebase already re-places roots in place (`streaming.rs:1763-1769`) and keeps every `GlobalTransform` invariant (test `repeated_rebasing_preserves_camera_and_cell_root_locality`, `1931-1969`). The remaining problems are (a) it runs on *every* crossing and dirties all 56 roots + ~12k descendants (estimated propagation/extract cost above), (b) the window slide on the same frame despawns/re-loads cells, and (c) flight reversals re-load cells despawned moments earlier.

Changes:
- **Tier A (ECS-side):** parent all cell roots under one `WorldRoot` entity (spawn site `streaming.rs:434-443`; create in `StreamingPlugin::build` or `setup_world`). A rebase becomes one transform write on `WorldRoot` plus the camera (`1761-1762`) — cells are re-placed, never despawned/re-spawned/re-validated for a rebase. `update_render_origin`'s root query (`1745`) shrinks to the single `WorldRoot`. Teach `validate_streaming_lifecycle`'s roots query (`1785`) to ignore `WorldRoot`.
- **Tier B (render-side, if the extract churn is confirmed by the new spans):** apply `RenderOrigin` at extraction/shader time (offset `GlobalTransform` in extract; `render.rs` water/terrain world-position paths) so a rebase changes no entity the render world re-extracts. This is what makes a rebase literally invisible downstream.
- **Rebase on threshold:** trigger at `trunc(local/CELL_SIZE) >= K` (K default 8–16) in `update_render_origin` (`1753-1758`) instead of every cell — crossing frames then carry only the paced ring slide; full-world churn happens 1/K as often.
- **Stop reload churn:** Design 1's revive rule, or an LRU of `CellPayload`+built meshes keyed by CellKey so `collect_cells` (`318-362`) can commit a returning cell without the DB and mesh rebuild.

Invariants: *no cell drawn twice* — a rebase must not respawn anything (assert `unloaded_cells` and `requests_submitted` do not move on a pure rebase frame). *No hole in view* — `GlobalTransform` of every entity is invariant under rebase (extend `streaming.rs:1931` to compare all GlobalTransforms before/after). *Commit budget* — a rebase routes zero cells through `collect_cells`.

Risk: Tier B touches `render.rs` extraction and world-space shader math (e.g. water reflection height, `render.rs:425`); far-from-origin precision regressions must keep passing the precision fix from `7151669` and the bounds tests in `streaming.rs`. Tier A is low risk but only removes main-world writes; propagation/extract churn remains (GlobalTransforms still move under `WorldRoot`).

Test: rebase N times; assert (a) zero despawns/requests on rebase frames, (b) all GlobalTransforms unchanged, (c) ≤2 changed transforms (WorldRoot + camera) via a change-detection probe; keep `streaming_fixture` green (`app.rs:331-380` — its `origin_rebases >= 6` bound may need adjusting under threshold rebasing); stress A/B shows extraction time and `frame_ms_worst` down on the 64 crossing frames.

## Recommendation

Do **Design 1 first**. It is contained in `streaming.rs`/`config.rs`, it attacks the unmeasured worst-frame burst that the previous A/B fix did not move (`results.json`: commit worst 7.7→1.1 ms, frame worst unchanged at ~29 ms), and it produces the missing measurement as a by-product. Design 2's Tier B is a renderer change whose benefit is still an estimate — do it only after the despawn/propagation spans have numbers.

**Smallest first commit (measurement only, no behavior change):** wrap the existing despawn at `streaming.rs:238` in a timed batch (`streaming/cell_despawn` span, `max_despawns_per_frame` gauge, `despawned_entities` counter), and persist per-frame `(frame_index, frame_ms)` from `metrics.rs:36/119` into `frame-metrics.json`. Second commit: `Retiring` status + budget + revive.

## Open questions

- "An origin rebase unloads and reloads cells" is false as causation at `origin/main` (verified in code and in `git log -S`: in-place since `086407c`); it is true as coincidence on crossing frames. If the earlier observation meant *render-world* churn (everything re-extracted on rebase frames), Tier B is the fix to prioritize.
- No per-frame time series exists in `stress-ab/runs/*` or `acceptance-main-60s/*` — crossing-frame cost attribution is inference from aggregates until the instrumentation above lands.
- `fly_camera` (`app.rs:128`) vs. the streaming chain has no ordering; pin it (`.after(fly_camera)` or move to `PreUpdate` like `drive_streaming_fixture`) so crossing latency is deterministic.
- The identity of the earlier A/B "fix" variant is not in the run metadata (`"commit": "unknown"`) — worth confirming before relying on its deltas.

