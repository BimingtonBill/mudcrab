# Goal
Track: portal. Port upstream main's paced cell unloads and paced model spawns onto the portal
branch's own cell streamer (`crates/engine/src/streaming.rs`), so streaming frames stop spiking when
a column of cells unloads or a cell's models arrive. Follow the research recipe appended below
(research-273), with the decisions fixed here.

# Context
- Your checkout is branch `portal-merge-main` (portal + current upstream main, merge 386044af, then
  0d426186). Commit prefix `[deepseek]`.
- Upstream main's streamer (the source to port from) is readable at
  `C:/Modding/OpenSkyrimProject/local/merge/streaming_main.rs`; what main changed against the common
  ancestor is `C:/Modding/OpenSkyrimProject/local/merge/streaming_base_to_main.diff`. Line numbers in
  the appendix that say `main:` refer to that file; `portal:` lines refer to your checkout's
  `streaming.rs` at 386044af (close to, but not exactly, your base).
- The config fields `max_cell_unloads_per_frame` and `max_model_spawns_per_frame` and every
  `StreamingMetrics` pacing field already exist (the metrics are zero today). `profiling.rs` already
  prints them.
- Decisions (fixed, do not reopen):
  1. `CellStatus::Retiring { root: Entity, detail: CellDetail }` (research D1).
  2. Space changes unload at once; only same-worldspace distance fall-out is paced (research D2):
     an interior that is neither active nor pre-streamed, and the exteriors of a worldspace the camera
     just left, are despawned immediately exactly as today.
  3. One despawn site for paced unloads: main's exclusive `despawn_cells` (research D3).
  4. A `Loading { replaced: Some(root), .. }` cell that falls out by distance keeps today's
     immediate despawn of `root` (not the research's derived-tier `Retiring`, which it calls
     fragile): simpler, and the case is rare.
  5. Keep research step F1-5: `request_cell` revives a `Retiring` entry it pins (otherwise a cell
     pinned between `plan_cells` and `despawn_cells` is despawned and re-requested).
  6. Port main's `SceneSpawnBatch` scaffolding too (research F2-1): without it the `instances_*`
     metrics stay zero. System order: plan_cells -> despawn_cells -> collect_cells ->
     arm_pending_models -> readiness.
  7. Do NOT bring main's terrain/static colliders, render-layer propagation, light rework, edge
     welding refactor or screenshot-anchored `streaming_center`; those are other ports.
- Invariants the reviewer (Phase 2 Dev, who wrote main's version) will check:
  a. A retiring cell is never re-requested while its paced despawn is in progress: the plan revives
     it (same tier) or replaces it through the existing `replaced` path (other tier). No flicker, no
     double spawn, the same root entity on revival.
  b. A pending model whose cell unloads or retires before arming disappears with its subtree: no
     dangling queue entry, no panic, never spawned into a gone cell.
  c. The pacing `StreamingMetrics` fields are written for real (`despawns_this_frame`,
     `max_despawns_per_frame`, `despawned_entities`, `retiring_cells`, `peak_retiring_cells`,
     `revived_cells`, `retire_backlog_overflows`, `instances_spawned_this_frame`,
     `max_instances_spawned_per_frame`, `arming_queue_depth`, `peak_arming_queue_depth`,
     `instances_armed_this_frame`, `max_instances_armed_per_frame`, `instances_completed_this_scan`,
     `max_instances_completed_per_scan`).
  d. `validate_streaming_lifecycle` counts no orphaned, missing or out-of-range roots while cells
     retire; `pinned` cells (`request_cell`) never retire; a full-detail cell that is still drawn
     while `Retiring` counts as resident for a door's destination check (`is_resident`).
  e. Portal's `mark_world_instance_ready` keeps its `record_authored_poses` call.
  f. Tests: also a retiring cell that is NOT pre-streamed (beside the research's
     `destination_is_resident` test), and a revived retiring cell keeps the same root entity.
- Before finishing, read how `crates/engine/src/portal.rs` (~line 751, cell-root bookkeeping) and
  `crates/engine/src/lights.rs` (~line 389, a cell changing role) react to a cell root, and say in
  the report whether a root that lingers while `Retiring` changes anything there (research risk 1).
  Do not edit those files; report it.

# Scope
Owned files: crates/engine/src/streaming.rs, crates/engine/src/profiling.rs, docs/handoff/impl-276-portal-port-pacing.md
Everything else is read-only. `profiling.rs` only if a summary line is missing. No engine window or GPU:
tests only. The handoff note is written but not committed.

# Work, in commits that each build green
1. Paced unloads (research Feature 1, steps 1-5), with main's unload-pacing tests ported (research
   section 4) and the portal-specific tests it lists under "Add".
2. Paced model spawns (research Feature 2, steps 1-5), with main's model-pacing tests ported.
3. Any fixes from running the full engine test suite.

Acceptance: cargo fmt --all -- --check, cargo clippy -p engine --all-targets -- -D warnings, cargo nextest run -p engine

# Done when
- The three acceptance commands pass in your checkout.
- Every existing portal test still passes; any portal test you had to change is listed with the reason.

# Report
Per commit: files, functions, tests added or changed. Answers on invariants a-e (where each is
enforced, file:line, and which test proves it). The portal.rs/lights.rs root question. Anything in
the research you found wrong.

# Appendix: research-273 report (read-only MiMo worker)

<!-- research-273-portal-port-pacing.1 (research): Produce a step-by-step porting recipe for two features of upstream main's -->
Status: done
Verdict: answered
Summary: Both features port cleanly; the one real design fork is how `Retiring` meets portal's tiers/doors — I propose `Retiring { root, detail }` and that *space-change* unloads (interiors left behind, worldspaces just left on a door crossing) stay immediate exactly as today, with only same-worldspace radial drift paced. Portal's config already defaults the budgets on (`max_cell_unloads_per_frame: 2`, `max_model_spawns_per_frame: 4`), so landing the port changes behavior immediately. Six numbered steps per feature keep the build green; main's 12 pacing tests port with small adapters.
Left: did not read portal.rs/lights.rs root bookkeeping or every portal test body (flagged in risks); no code changed (read-only).
Next: implementer should decide yes/no on the space-change rule before step F1-2 — it is what keeps `plan_cells_follows_the_active_cell_across_a_worldspace_change` green unchanged.

---

# Porting recipe: paced unloads + paced model spawns → portal's streamer

Sources: `local/merge/streaming_main.rs` (= **main**), `local/merge/streaming_base_to_main.diff` (= **diff**), target `crates/engine/src/streaming.rs` (= **portal**), config `crates/engine/src/config.rs`.
Notation: main:line / portal:line. Diff lines given only where a main line is hard to spot.

## Design decisions (ambiguous points, resolved)

- **D1 — `Retiring` carries the tier:** portal's `Resident{root, detail}` (portal:609) and `Loading{generation, detail, replaced}` (portal:604) make tier part of state; main's `Retiring { root }` (main:406) cannot restore a tier on revival. Portal gets `Retiring { root: Entity, detail: CellDetail }`.
- **D2 — space changes unload at once; only radial drift is paced.** A cell is paced only when it falls out by *distance*: `CellKey::Exterior` of `active.worldspace_id` while `active.interior.is_none()`. Everything else that leaves the keep-set (an interior neither active nor pre-streamed; exteriors of a worldspace being left — the door-crossing case) keeps portal's current immediate despawn (portal:751-765). This keeps the brief's constraints ("a door crossing despawns the worldspace just left", "an interior … must not stay drawn") and matches main's own warning that a worldspace change must unload at once (diff:361-365). Pacing then covers exactly main's use case (flying: a column retires per cell crossed).
- **D3 — one despawn site for paced unloads:** `despawn_cells` (exclusive) drains the retire backlog; immediate unloads stay in `plan_cells`' retain via `Commands`. `continuity.edges` for a paced cell is removed at despawn (main:565), not at retire-mark.

## Feature 1 — paced cell unloads

1. **Add the variant, arm every match (no behavior change).** `CellStatus::Retiring { root, detail }` after `Resident` (portal:595-619; shape main:400-408). Arms treating it as a drawn resident:
   - `plan_cells` held/root match (portal:668-689) → `Some(CellStatus::Retiring { root, detail }) => (Some(*detail), Some(*root))` — a retiring cell asked for again at another tier then flows into the existing `replaced` path (portal:697-738) untouched.
   - retain match (portal:757-765): `Retiring` handled like `Resident` for now.
   - `validate_streaming_lifecycle` resident set (portal:3196-3202) → include `Retiring { root, .. }` (main:3167).
   - test-local exhaustive matches (portal:4459-4461, 4716-4718, 4727-4734) gain arms. `matches!` sites (`is_resident`/`has_failed`/`retry_failed_interior`, portal:212-244, 1146-1151) compile unchanged.
2. **Rewrite the retain (portal:740-768) to main's shape (main:455-471) with D2.** Keep `pinned || cell_within_unload_radius(...)` as the keep test (portal:742-750). Keep-branch: revive `Retiring → Resident{root, detail}` and count `revived` (main:458-461). Drop-branch, classified by D2:
   - distance fall-out of `Resident{root, detail}` → `Retiring{root, detail}`, stay (main:464-467).
   - distance fall-out of `Loading{replaced: Some(r), detail: d}` → `Retiring{root: r, detail: other(d)}` (`other(Full)=Terrain`); exact because a request is only issued when `held != detail` (portal:690-692). Its in-flight answer is discarded as stale on arrival (portal:878-891 already does this for any non-`Loading` status).
   - `Loading{replaced: None}` and `Failed` → drop entry, stale response later (main:455-459).
   - anything in D2's immediate class → today's behavior: remove edges (portal:753), despawn root now (`try_despawn`, portal:758-763), drop entry.
   - Move `continuity.edges.remove` out of the paced path (see D3). Add `retiring_cells`/`peak_retiring_cells`/`revived_cells` accounting at portal:780-809 (main:490-500).
3. **Bring the despawn machinery** after `plan_cells` (insert near portal:810): `retire_backlog_bound` (main:515-520), `cell_order_key` (main:522-533), `despawn_cells` (main:538-629), `despawn_subtree` (main:631-648). Adapt: `CellStatus::Retiring { root }` patterns → `{ root, .. }` (main:555); metrics fields all already exist on portal (portal:460-474). Budget `0` = unbudgeted (main:545-548).
4. **Register `despawn_cells` in the Update chain** between `plan_cells` and `collect_cells` (portal:144-153; main's order main:71-78). It is `fn despawn_cells(world: &mut World)` — exclusive on purpose (main:541-543).
5. **Lifecycle + query APIs.** `validate_streaming_lifecycle`: the `Retiring` arm from step 1 is the whole change — retiring roots are accounted (not orphaned/missing, portal:3211-3212) and its out-of-range filter already matches only `Resident` status (portal:3235-3243), so a retiring root can never be out-of-range (main's entity-side filter main:3200 is unnecessary here). Then: `StreamingWorld::request_cell` (portal:256-288) revives a `Retiring` entry it pins before its `contains_key` return; `is_resident` (portal:212-220) also accepts `Retiring { detail: CellDetail::Full, .. }` so a door gate waiting on residency (transition.rs:1142 `destination_is_resident`) sees a still-drawn full destination as resident — the same frame's plan pass (chain runs `.after(DoorTransition)`, portal:158) revives it properly.
6. **Tests** (section 4), then update the doc comment on `CellStatus::Retiring` to portal's door-aware wording (main's comment at diff:361-365 is written for a tree with no runtime worldspace change; portal has one).

## Feature 2 — paced model spawns

1. **Plumbing first (green; nothing produces `PendingModel` yet).**
   - `next_model_sequence: u64` on `StreamingWorld` (portal:180-203; main:99).
   - `struct PendingModel { handle: Handle<WorldAsset>, sequence: u64 }` beside `PendingAssetProfile` (portal:1638; main:1422-1428).
   - `model_can_spawn` (main:1579-1591) and `arm_pending_models` (main:1529-1577) — the arming budget, `0` = arm all ready (main:1566-1570), oldest `sequence` first, still-loading models skip the budget (ready-list built at main:1543-1548).
   - Hook `arm_pending_models` between `collect_cells` and `track_asset_readiness` (portal:144-146; main:74-78).
   - `SceneSpawnBatch` + `begin_scene_spawn_batch`/`end_scene_spawn_batch` (main:1483-1527), `init_resource::<SceneSpawnBatch>()` (portal:128-137; main:65-66), and a `SpawnScene` block bracketing `SceneSpawnerSystems::WorldInstanceSpawn` (main:84-94, add after portal:159).
   - Readiness scan (portal:1755-1913): add `unarmed: Query<(), (With<PendingAssetProfile>, With<PendingModel>)>` and `metrics.pending_asset_instances = pending.iter().count() + unarmed.iter().count()` (main:1608, 1624) — otherwise unarmed models vanish from the pending set and readiness gates resolve early; write `instances_completed_this_scan`/`max_instances_completed_per_scan` + gauge at end of scan (portal:1905-1912; main:1831-1842). `mark_world_instance_ready` (portal:1740-1752) keeps its portal-only `record_authored_poses` — do not replace with main's shorter version (main:1593-1601).
2. **Producer: `spawn_cell`.** At the model insert (portal:1557-1572) replace `WorldAssetRoot(...)` with `PendingModel { handle: asset_server.load(...), sequence }`, taking and bumping the sequence (main:~1056-1072, the block at main:1023-1079). Keep portal's `MeshHandle`, light child, door/anchor and `DirectionalSnow` inserts (portal:1515-1556) — do **not** bring main's light block (diff:849-872), `Propagate(PLACED_OBJECT_RENDER_LAYERS)` (diff:844) or the `Result<Entity, String>`/terrain-collider rewrite of `spawn_cell` (diff:707-734, 897-905); those are other features.
3. **Thread the counter:** `spawn_cell(..., model_sequence: &mut u64, ...)` (main:841 signature, call main:747) → portal signature (portal:1323), call site (portal:998-1012), test helper `spawn_queued_references` (portal:5364-5383, pass a local `&mut 0u64`).
4. **Paced vs unbudgeted arming is config-only:** `max_model_spawns_per_frame` (config.rs:29, default 4 at config.rs:128). No further plumbing.
5. **Interaction with the readiness scan:** a failed load is still armed (main:1579-1588 arms on `LoadState::Failed` too) so failure reporting stays exactly where it is (portal:1774-1797 reads `WorldAssetRoot` through `PendingAssetQuery`, portal:1726-1738).
6. **Tests** (section 4).

## 3. Interactions and proposed rules

| Case | Rule | Where |
|---|---|---|
| Retiring cell wanted again at another tier | Treat as held at its `detail` with root (step F1-1); plan submits the other tier with `replaced: Some(root)` exactly as a resident would (portal:697-738), overwriting `Retiring` with `Loading`. Same tier → keep-branch revives (F1-2). | portal:668-738 |
| Retiring interior at a door crossing | Interiors never pace (D2): an interior neither active nor pre-streamed is immediate-unloaded in the retain — the constraint "must not stay drawn" is met in the same frame. | portal:741-768, 1291-1294 |
| Worldspace just left on a crossing | Immediate (D2): `(Exterior, Some(_)) => false` and `worldspace_id != active.worldspace_id` (portal:1294, 1304) are space reasons, not distance. Destination stays resident via `PrestreamCells` (portal:1288; transition.rs:395-408) and revival. | portal:741-768 |
| `Loading{replaced}` cell retires mid-load | Root retires as `Retiring{root, other(detail)}`; in-flight answer → `stale_discarded` (portal:884-886). Revival restores `Resident{root, other(detail)}` and the plan re-requests. | F1-2 |
| `pinned` | Never retires: keep-test checks `pinned` first (portal:742). `request_cell` revives a `Retiring` key it pins (F1-5). Fixture's "load by id and it stays" invariant (portal:199-202) preserved. | portal:742, 256-288 |
| `PrestreamCells` | Kept while requested (portal:1288); when the door goes away, falls out by D2 like any other cell (another worldspace → immediate; same worldspace distance → paced). | portal:1280-1309 |
| Tiers (`Terrain` ring roots) | `Retiring` carries `detail`; ring roots are `DistantTerrainRoot` (portal:1351-1353) and pace identically; `CellRootsQuery` counts both (portal:1719-1724). | F1-1 |
| `PendingModel` cell despawned before arming | No cleanup: the component dies with the subtree `despawn_subtree` removes (main:1378-1381 comment; test main:5280). Portal's `try_despawn`/`despawn` is already subtree-recursive. | F2-2 |
| Lifecycle validator | Retiring roots accounted in `resident_entities`; out-of-range filter matches `Resident` status only → invariant counts zero for both. | portal:3193-3246 |

## 4. Tests

**Bring from main (unload pacing):** `a_zero_unload_budget_unloads_the_whole_crossing_in_one_frame` (main:3983), `a_crossing_unloads_cells_within_the_budget_and_finishes_within_ceil_frames` (main:4009), `a_reversal_revives_retiring_cells_without_another_request` (main:4058), `retire_backlog_bound_is_one_window_at_the_unload_radius` (main:4095), `a_backlog_past_the_bound_unloads_every_retiring_cell_at_once` (main:4103). Helpers: `streaming_test_app` (main:3868), `spawn_camera`/`move_camera` (~main:3894-3914), `spawn_mock_cell` (main:3927), `spawn_steady_window` (main:3958), `cell_id_of`/`resident_root_count`/`streaming_metrics` (~main:3966-3980). Adapters: `spawn_mock_cell` stores `detail: CellDetail::Full`; `streaming_test_app` must also `init_resource` `ActiveCell` (exterior, `interior: None`) and `PrestreamCells` (portal's `plan_cells` takes them, portal:630-631) and chain `(plan_cells, despawn_cells, validate_streaming_lifecycle)` (main:3889).

**Bring from main (model pacing):** `the_spawn_batch_counts_the_instances_bevy_instantiated_this_frame` (main:5114), `the_arming_budget_spreads_a_cells_models_over_frames` (main:5166), `the_arming_budget_arms_the_oldest_reference_first` (main:5211), `a_model_still_loading_does_not_use_a_budget_slot` (main:5238), `a_pending_model_whose_cell_is_despawned_leaves_nothing_queued` (main:5280), `an_arming_budget_of_zero_arms_every_ready_model` (~main:5319), `every_paced_model_is_spawned_and_validated_exactly_once` (~main:5342). Helpers: `model_app` (main:4722 — drop `StaticCollisionCache`/collision registrations; keep `WorldSerializationPlugin`, type registrations, the `SpawnScene` block), `paced_config` (main:4823), `spawn_pending_reference` (main:4919), `armed_and_pending` (main:4927), main's drain-form `settle_readiness` (main:4943), `add_converted_model`/`empty_converted_scene`/`converted_scene_with_mesh`/`converted_scene_with_material` (~main:4768-4818), `spawn_test_cell` (~main:5051).

**Portal tests affected:**
- `plan_cells_follows_the_active_cell_across_a_worldspace_change` (portal:4108; runs `plan_cells` only, portal:4130, asserts the dropped interior is gone from `planned_keys`, portal:4155-4159) — **stays green unchanged only under D2**; this is the test that decides the design fork.
- Compile arms: `a_tier_change_is_never_a_hole_in_the_world` and helpers (portal:4701, 4716-4734), the status-format helper (portal:4459-4461).
- `unload_radius_drops_distant_exteriors_and_inactive_interiors` (portal:3603) — pure predicate, unchanged.
- `lifecycle_validator_detects_duplicate_and_orphaned_roots` (portal:5854) and `lifecycle_validator_accepts_a_cell_that_is_changing_tier` (portal:5896) — assertions hold; add one `Retiring` case to the latter's spirit (root accounted, not out-of-range).
- `spawn_queued_references` helper (portal:5351-5384) — signature only. Spawn-time assertions (lights portal:5427, doors portal:5138/5196/5263, snow marker portal:5549, position portal:5081) are unaffected; anything asserting *scene contents* through `spawn_cell` must add `arm_pending_models` to its schedule and wait (portal's `settle_readiness` portal:6827 is a fixed 8-frame loop; adopt main's drain form where arming is in play).
- Unaffected: the empty-model suite (portal:6840/6887/6930) and snow-swap tests (portal:5611/5692/5752) — they insert `WorldAssetRoot` by hand (portal:6810) or build entities directly, bypassing arming; interior-retry and full-failure tests (portal:4504/4580/4793/4879) unless they move the camera out of range.
- **Add (portal-specific):** crossing into an interior unloads the left-behind interior *and* the exterior window in one frame; a paced retire that later becomes a space change unloads at once; a revived retiring cell keeps its root entity (same entity id, no despawn+respawn); a full-detail destination still passes `destination_is_resident` while `Retiring`.

## 5. Risks / unsettled

- **Root-consuming systems:** `portal.rs` keeps per-frame cell-root bookkeeping ("a despawn and a respawn", portal.rs:751) and `lights.rs` reacts to "a cell changing role" (lights.rs:389). While a cell retires its root lingers several frames; I did not read those systems — verify neither keys "root present" to "cell resident" before merging.
- **Derived revival tier** (`other(detail)` for a retired `Loading{replaced}`) is exact today only because a request is issued solely when `held != detail` (portal:690-692); fragile if same-tier reloads are ever added. Alternative: make `Retiring` carry `Option<PendingLoad>` — rejected as overkill for the port.
- **Behavior change on landing:** portal's config defaults the budgets on (2 unloads, 4 spawns — config.rs:121, 128), unlike a "port inert, enable later" plan; every acceptance/shots frame time shifts. `0` restores the old behavior for both (main:545-548, config.rs:24-29).
- **Assumed identical Bevy/world-serialization surface** (`SceneSpawnerSystems`, `WorldInstance`, `WorldInstanceReady`) on both branches — supported by the portal←main merge (git log 386044af) but not independently diffed.
- **Not settled from code:** whether `demo_tour`/`--shots` waits (demo_tour.rs:1254) tolerate a destination judged resident via the new `is_resident` rule while still `Retiring`; and whether any test asserting `roots_of(...) == 0` after a camera move breaks under pacing (none found in the bodies I read; I did not read all test bodies).
- **Deliberately not brought** (other features in the same diff): screenshot-anchored `streaming_center` (diff:390-399), terrain colliders/static collision (main:906-1228), render-layer propagation, light rework, edge welding.

[ds-agent] run=research-273-portal-port-pacing.1 session=9e4cc324-cf4e-4ce3-b599-765379f8fd78 status=ok turns=40 tokens_in=110636 tokens_out=46670
