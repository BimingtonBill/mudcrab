# I19 loading and readiness fix (#2143.1)

Base: `589dc62fc891661b5caeb4e779f5dcae86b3f7f3`. Branch: `phase2/cluster-no-stand-ins-fix`. Worktree: `D:/Mudcrab/worktrees/no-stand-ins-fix`. The original no-stand-ins worktree belongs to #2144 and remains untouched.

## Causes and changes

Sparse distant requests and publication inherited the default one-full-cell-per-frame count. Approximately 4,800 sparse cells therefore took hundreds of seconds at I19's frame rate, despite only about two seconds of accumulated database work after 227 seconds. Sparse publication also spent slots shared with terrain CPU batching and handover. The collector now admits asynchronous requests through existing worker backpressure and applicable admission/backlog controls, polls replies independently, and publishes nearest ready responses under the shared elapsed-time budget. Sparse work preserves full-cell slots. Each complete response is validated before publication, and its commands are applied before the clock admits another response. Failed responses spend shared work too. No demand, distance, settings, or numerical caps were changed.

Readiness rebuilt its needed-batch inventory by walking 510,129 instances on broad source changes. New owned terminal facts also invalidated refusal-only consumers and repeatedly projected ownership indexes. The renderer now retains the catalog under an exact inventory key, separately recomputes current consumer/source/compute admission, and skips unchanged aggregate work. Catalog invalidation includes GPU identity/generation/layout, scene change tick, prepared materials, fixed refusals, layout scope, and runtime state. Proof withdrawal requires a fresh aggregate. Incremental fixed-refusal indexes and a refusal-only revision avoid invalidation for newly learned owned facts. Camera, prepass, and shadow requirements remain authoritative, including missing-view sentinels.

The old stall diagnostic required 60 virtual seconds without any input change. Continuing loading reset that timer, so I19's silence did not establish a skipped startup system. An independent report now fires every 60 real seconds while admission remains blocked. It includes cached package/layout/generation/world counts, physics/certification state, and decomposed LOD work. Its diagnostic-only event also requests existing terrain blocker reporting. Admission, timeout recovery, pacing restoration, and cover release semantics are unchanged.

Six source counters partition existing changed entities by prior index membership and current liveness. They distinguish new loading activity from surviving roots and removed nodes without calling every unseen node a birth. Render timing marks now bracket the actual render system inside its set, preventing readiness work before rendering from appearing as graph/present time.

## Evidence collected before behavioral fixes

The supplied I19 run had one far-demand generation, a shrinking queue, increasing indexed reference groups, and zero scene modifications in all 2,523 invalidation reports. The instrumented capture also showed unseen live nodes and ancestry retirement with zero scene modifications. Ordinary partial-scene flattening removes ancestry, so those counters alone do not establish an erroneous replacement loop. I18 was also unready and had disturbed timing guards; it is not a successful-readiness baseline.

`diag-before-sparse-tracy` ran for 90 benchmark seconds as a gated diagnostic TEST. The new blocked warning appeared at 60.2025 real seconds with `stalled_seconds=0`, retaining cover. Tracy attributed 36.531 ms mean to readiness observation, 12.561 ms to far planning, and 8.468 ms to source-demand publication. Actual render-system mean was 3.979 ms, present self-time 0.454 ms, and readback submission 0.051 ms. These profiler attribution numbers are not timing-run performance measurements and do not justify a driver/present workaround.

Evidence is retained under `D:/Mudcrab/builds/no-stand-ins-fix-target`, including the diagnostic capture, source diff, binary hash, logs, test receipts, and integrated Rust source hashes. All Cargo uses the build gate with owner `sol-2143-1`; the isolated target was seeded from cluster-final-target with incremental directories excluded. Runtime diagnostics copy the original I19 assets, pose, Vulkan backend, package, and cluster flags. They are TEST runs, explicitly not timing runs.

## Regression proof

- Source counters first produced the expected missing-field compile red. Three source tests then passed while four periodic-report tests failed with zero captured reports.
- Expanded sparse red ran eight tests: one whole-response validation pass and seven expected scheduling failures. Tests cover native backpressure, exact placement, multiple publications preserving full-cell slots, polling under exhausted time, applied-command cost, nearest-ready priority/ties, slow-response bypass, atomic invalid-row rejection, and failure accounting.
- Inventory semantic red compiled and ran 17/23 tests before fail-fast: 12 passes, four expected catalog-reuse failures, and one expected refusal-revision failure. Six selected tests were not run in that receipt.
- Integrated `focused-green2` passed all 43 selected tests, with 3,172 unselected. A preceding fixture-only failure was corrected to respect a real authoritative row without valid bounds; production validation was unchanged.
- Strict engine Clippy passed all targets/features with warnings denied (`engine-clippy2`). Its first attempt found one redundant test-only explicit `drop` of a Bevy borrow guard; removal resolved the lint.

## Acceptance status

The final normal release reaches world-ready at lodfar0 with clusters enabled. Engine and workspace checks pass. The diagnostic benchmark still exits 1 on the pre-existing streaming commit-budget gate; readiness acceptance and benchmark threshold success are recorded separately below. This branch is local only; no push, PR, or folder deletion occurred.

## First post-fix diagnostic: remaining admission defects

`diag-after-fix-warmup` used the fresh normal release binary for 120 benchmark seconds. No timing hold was active; #2144's converter ran concurrently. It was the first run after building and is not a timing run. The run still failed admission, so it provides no settled frame-time window. Its loading-inclusive p50 was 28.0828 ms; this is not a settled performance claim.

All 4,796 sparse cells completed, publishing 25,525 distant references with zero failed queries. At five seconds after sparse demand began, 2,322 cells had already committed. Terrain's pending query/CPU/GPU counts reached zero. The independent blocked report then isolated eight pending models, two missing consumers, and eleven structurally refused batches repeated across seven consumers (77 failed/template batch-view pairs). All 91 created render pipelines became ready; the pending-pipeline count includes absent consumer proof, not thousands of compiling shaders.

Offline inspection of the exact certified torch source establishes the template defect: ordinals 3 and 4 are converter-accepted rigid solid geometry without billboard ancestors; refused flame siblings include billboard ancestry. Whole-template validation rejects these unrelated siblings and prevents terminal qualification of the valid certified body. New tests exercise occurrence-scoped proof while retaining all strict checks on the selected occurrence, ordinal ambiguity, mesh dependencies, and frozen ownership.

Bevy UI also creates a separate ExtractedView sharing its 3D camera's main entity. The current consumer query reads every extracted view and therefore can assign the UI subview a world and invent a shadow dependency for it. A production-query regression and changed-only identity diagnostics are being verified before changing that inventory rule. Genuine Camera3d and light views remain required even when their phases or GPU proof are absent.

The diagnostic-only `diag-missing-consumer-proof` run identified both false dependencies before the filter changed. Exactly one expected world camera was logged, with valid 1600×900 target and viewport. Both missing entries had the same retained main camera and subview index 1: one camera entry and one shadow entry. They had an extracted non-light view, no cluster GPU view, and no directional-light metadata. This matches Bevy UI's documented synthetic subview. The run used the already-built optimized test-profile binary, emitted Vulkan semaphore validation messages during startup, and was intentionally stopped after attribution. It is not GPU acceptance or frame-time evidence.

The observed new red was four tests: three expected semantic failures and one malformed-occurrence pass. Production now uses event-cached valid ordinal sets and Camera3d/light role queries. The independent scoped review found no actionable findings. Later checks and final release evidence appear below.

## Reviewed-tree checks

`engine-nextest` passed all 3,240 engine tests (13 skipped, one slow) in 88.397 seconds. This includes the four occurrence/UI regressions after their observed semantic red. `engine-clippy3` passed all targets/features with warnings denied. Those fourteen changed/new Rust files matched the retained `occurrence-source.json` snapshot. The later runtime producer fix and final checks supersede this intermediate snapshot.

## Remaining runtime-state granularity defect

The next fresh release diagnostic, `diag-final-warmup`, cleared model arming, template qualification, missing views, material/compute/pipeline readiness, all distant work, and all LOD queues. Admission still failed on 55 failed batch/view pairs: eleven batches across five genuine consumers. Its final `cluster/owned_not_drawable` gauge was 55. No settled frame window exists for this run; its loading-inclusive p50 is not a settled result.

The remaining failure comes from persistent reference-wide revocation. `observe_changed_reference_poses` treated any changed source descendant as movement of the entire placed reference. Nine logged revocations resolve to animated lamp/flame sources. Exact certified GLB inspection found no billboard or animation target in any accepted primitive's ancestry; movement belongs to refused sibling subtrees. One wholly refused fire source contributes no certified body.

The runtime producer now checks only the changed subtree against resolved frozen owned ordinals. A refused flame sibling or gameplay-only descendant does not move an owned body. An owned occurrence, retained owned-source marker, missing mesh identity, cycle, or broken child link remains conservative. Logical events, root motion/visibility, disabled roots, and actual owned-body/ancestor changes still revoke the whole reference persistently. No stock permission or render-gate behavior changes.

`runtime-scope-red` compiled and ran seven real-certificate regressions: four protection passes and three expected failures for refused billboard motion, refused animation, and a gameplay anchor below an owned mesh. Independent source review found no proven production defect and requested two coverage additions. The final tests also exercise untagged mesh/source-mesh markers and a real producer certificate replacement with different owned ordinals. `engine-nextest2` passed all 3,248 tests (13 skipped) in 99.599 seconds, including all eight descendant tests. Production strict engine Clippy passed in `engine-clippy4`; the later test-only additions also passed the final workspace strict check. Fifteen Rust files are frozen in `runtime-source.json`.

The optional `diag-runtime-scope-sanity` run used the optimized test-profile binary while workspace Clippy ran. It emitted the same startup Vulkan present/acquire validation errors seen in the earlier test-profile attribution run. The parent intentionally stopped its own process without world-ready proof or a benchmark report. It supplies no renderer-acceptance or frame-time evidence; final acceptance uses the normal release binary.

## Final verification and live admission

All Cargo ran through the build gate with owner `sol-2143-1` and the isolated target. Every retained Rust source matches `runtime-source.json`; `verification.json` records the receipts and diagnostic outcome.

- `engine-nextest2`: 3,248 passed, 13 skipped, 99.599 seconds.
- `workspace-clippy`: all targets/features, warnings denied, exit 0. This includes the final engine tests.
- `workspace-nextest`: 5,451 passed, 63 skipped, 498.506 seconds.
- `workspace-docs`: 18 passed, 15 ignored, exit 0.
- `final-fmt`: `cargo fmt --all -- --check`, exit 0.
- `normal-release3`: normal optimized release, exit 0. Binary SHA-256 `fc1e0471bdc8e5da9aa31c304607f9ffc819e4ca523b926850af9e1c48dc570e`.

Every engine run was announced and used the TEST gate with no hold active. The first post-build release run, `diag-runtime-fixed-warmup`, was discarded. Concurrent workspace tests disturbed startup: envelope certification took 47.475 seconds and the 60-second benchmark limit arrived before certification completed. Its exit 0 means permissive benchmark thresholds passed, not that the world was ready. No timings from it are used as settled results.

The subsequent 90-second `diag-runtime-fixed-final` ran after the builds and workspace tests ended, with the original I19 assets, world, pose, Vulkan backend, cluster and terrain flags. It is **not-a-timing-run**. It reached `world_ready_reached=true` at **42,822.4782 ms**. Startup independently logged readiness, final `cluster/owned_not_drawable` was **0**, all 4,796 sparse cells drained, and no later drawability-loss or renderer error appeared. Mesh entities remained **81,380**.

The engine's post-ready window contains 2,258 frames: p99 **29.2202 ms**, worst **40.5485 ms**, three over 33 ms, none over 50 ms. Matching the CSV tail against the engine's p99 and worst gives post-ready p50 **24.7256 ms**, p95 **26.8767 ms**, and mean **25.0089 ms**. These are diagnostic observations, not a controlled speedup or 160 fps acceptance claim.

After ready, the expensive needed catalog, source full/reference/template scans, and cluster selection readiness/pipeline scans ran zero times. There was one drawability aggregate refresh. The overall settled-work flag remains false: existing terrain finalization still records 4,517 scans and retains one undrawn terrain-batch handover, so this is not a claim that all engine work settled to zero.

The final benchmark reports `passed=false` and exits **1 solely because `commit_budget_respected=false`**, with 85 startup violations. All other threshold gates pass. Supplied I19 already reports the same false gate with 10 violations; the prior pre-runtime-fix diagnostic reports 83. Neither that gate nor its settings were weakened. Follow-up **OP39**, "Remove startup streaming commit-budget overruns", is queued in the shared local task board. Stationary lodfar0 readiness is proved; fly routes, portal transitions, and quiet timing acceptance were not exercised by this diagnostic.
