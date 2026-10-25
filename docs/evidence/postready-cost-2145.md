# Post-ready cluster cost investigation

Owner: sol-2145. Base: `1332e0d87`. Branch: `phase2/cluster-postready-cost`.
Worktree: `D:/Mudcrab/worktrees/cluster-postready`.
Isolated target and evidence: `C:/MudcrabBuilds/cluster-postready-target`.

## Final standing result and acceptance limits

The two source fixes preserve the authored render path: a resource-free NifDepth extension now retains StandardMaterial bindless grouping, and terrain proof queries native phase membership without copying unrelated stock populations. Both former semantic reds pass. No settings, geometry, sorted order, shader, ownership, or fallback policy changed.

The retained final TEST uses the same main28 dataset, lodfar0 pose and full launch settings as `baseline-tracy`. Its immutable binary SHA-256 is `c8a030cd0dbd6cb88750ee5460e879d953f726796854069b677d05efe2510d01`. Final readiness is 40.204 seconds; the selected final 30 seconds contain 2,044 render frames, beginning 119.667 seconds after readiness. Baseline selection contains 1,687 render frames. Both standing captures contain 99,220 entities and 32,692 meshes; these are not the older 81,380-mesh scene.

CPU render schedule averages **15.8262 to 11.9185 ms**, a 3.9077 ms reduction (24.7%). Recorded GPU pass sum is **7.4074 to 7.2965 ms**. Main-world render wait is **11.6240 to 6.2165 ms**. Diagnostic render-cycle wall time is **17.6427 to 14.4655 ms**. CPU render-path dominance remains an inference: inclusive wall spans include internal waits, nested/parallel spans overlap, and GPU pass sum is not a complete hardware frame-time measurement. These gated TEST captures establish attribution, not quiet FPS acceptance.

The ten largest baseline CPU systems, with their exact final counterparts:

| CPU system | Before inclusive ms/frame | Final inclusive ms/frame |
| --- | ---: | ---: |
| Submit command buffers | 6.0573 | 4.1116 |
| Transparent pass | 3.5078 | 1.4169 |
| Extract cluster sources | 1.2801 | 1.8346 |
| Prepare transparent batches | 0.8496 | 1.0173 |
| Finalize terrain clips | 0.7335 | 0.2375 |
| Terrain handoff filter | 0.6764 | 0.0912 |
| CPU visibility | 0.3995 | 0.4847 |
| Directional shadow visibility | 0.3250 | 0.3901 |
| Prepare preprocess bindings | 0.3089 | 0.3931 |
| Collect benchmark metrics | 0.2422 | 0.3059 |

Final top-ten ranking replaces terrain filtering with GPU visibility (0.2483 ms); final terrain clipping ranks tenth at 0.2375 ms. Transparent encoder finish is 4.5699 to 2.4737 ms, inside parallel submission. Actual queue submission is 1.3218 to 1.4304 ms. These are not added to enclosing submission.

The binding-only intermediate capture has render schedule 10.1250 ms. The final combined capture has broad CPU increases in unchanged source extraction, batching, visibility and metrics, despite removing terrain-copy work. This variability is unresolved; the final measurement cannot isolate the native lookup's total frame-time effect or establish a regression. The directly measured terrain finalization and filter reductions remain visible. Neither the intermediate nor final diagnostic meets the requested 8.3 ms budget.

Separate count-only captures retain the same assets/pose. In each final 1,000 frame samples, observed transparent stock calls fall **5,612 to 1,655 (70.5%)**: NifDepth 5,599 to 1,642, StandardMaterial 12 to 12, other 1 to 1, missing identity 0. All observed stock phase API calls fall 5,706 to 1,735. Fixed indexed argument slots are 5,613 versus 5,614; slots are commands, not geometry coverage. Custom unwrapped variants remain outside observer coverage. Final count mode has one extra diagnostic entity (99,221); pure standing attribution has 99,220. Whole-run stock indirect batch-set peaks are **5,685 to 1,713**, not exact settled samples and not cluster draw groups.

Both pure standing captures report `settled_work.after_ready = {}`, `clean = true`; final native binned/sorted full-scan counters are zero after readiness. Final `cluster/owned_not_drawable = 0`, exact-stock failed cells 0, global waiting surfaces 97. Zero full rebuild/copy counters does not mean literal zero per-frame work: source ECS change queries still cost 1.3869 ms inside 1.8346 ms extraction, and current-view rendering/terrain proof still runs. Remaining leading costs are stock command recording/finish/submission, source mutation observation, and sorted stock preparation. The next source investigation must preserve raw mutable ECS invalidation and ordering; it cannot replace those contracts with an unchecked settled flag.

Both final standing and count diagnostics exit 1 solely for the existing startup `commit_budget_respected = false` threshold. No readiness, asset, streaming or Vulkan validation failure was found in either standing run. Thresholds were not relaxed. Full fly-route acceptance is recorded separately below.

Final source verification, all through the gate, is complete: fmt check; engine and workspace all-target/all-feature Clippy with warnings denied; touched-crate nextest **3,293 passed, 13 skipped** (engine 3,270, bevy_render 23); workspace nextest **5,503 passed, 63 skipped**; workspace doctests **18 passed, 15 ignored**. `final-verification.json` records all seven zero exits, log hashes, seven unchanged Rust source hashes and the immutable executable hash. Independent final native-integration review found no actionable issue. Device-backed forced/non-forced binding tests passed on Vulkan; actual unsupported-hardware rendering remains untested. Visual corroboration uses the binding-only image pair and is not comprehensive semantic proof.

Evidence remains under the owned target: `final-comparison.json`, `final-tracy/capture.tracy`, both post-ready JSON tables, benchmark JSON, `final-counts/profile/draw-submission.json`, semantic red/green receipts and verification manifests. No root-level task note, push or PR is created.

## Required behavior

Preserve all static geometry at full detail, certified ownership, and real render readiness. No billboards, stand-ins, stock fallback for owned geometry, reduced settings, hidden loading, or terrain-radius override. After the scene settles, unchanged inputs must not cause readiness rebuilds or scans. Moving views and changed assets must invalidate the exact dependent proof.

## Investigation and implementation plan

- [x] Read predecessor evidence and create the requested isolated checkout and target with ownership records.
- [x] Build an optimized Tracy executable through the shared build gate. Capture the original lodfar0 pose through TEST after timing holds clear.
- [x] Verify readiness and select at least 30 seconds of post-ready trace events. Attribute CPU time by system and inspect GPU pass timings. CPU render-path dominance remains an inference; wall spans do not distinguish active work from internal waits. Profiler costs are attribution evidence, never accepted frame-time results.
- [x] Instrument the measured candidate's invalidation/work boundary before testing a change. Trace the responsible producer and all invalidators.
- [x] Add a failing regression at that boundary, observe semantic red, implement the source fix, and prove green without weakening readiness.
- [x] Repeat the same diagnostic capture, then exercise fly routes (results and limits below). Discard the first run after a build. Quiet timing and normal-release FPS acceptance remain separate from these TEST captures.
- [x] Run required engine and workspace checks and independently review the diff. A local commit includes the runtime evidence and requested trailer.

## Initial evidence

The user corrected the historical comparison: `91c23bbd` omitted far rocks and drew a smaller world. Approximately 25,500 distant references were added for completeness in #2133, and complete-world `dd15ef33` already flew at 29.9 fps. A regression is not established. The approximately 25 ms cost remains unexplained; the profile must distinguish content cost, CPU work, and GPU work.

The predecessor's final normal release diagnostic reached readiness at 42,822.4782 ms with 81,380 mesh entities. Its post-ready p50 was 24.7256 ms, p95 26.8767 ms, and p99 29.2202 ms. This is a diagnostic observation, not quiet timing acceptance.

That report mixes loading into its CPU/GPU aggregate tables: render-thread p50 18.990 ms, extraction p50 5.800 ms, queue p50 5.424 ms, graph/present p50 9.706 ms; GPU opaque p50 2.803 ms and early prepass p50 2.389 ms. They motivate render-thread investigation but do not establish the post-ready bottleneck. Its settled counters include 4,517 terrain finalization scans and several handover updates. The expensive catalog/source/selection rebuilds are absent after readiness.

The last 30.013 seconds of that run's `render-cycles.csv` contain 1,201 cycles, safely after the recorded first-ready frame. Mean wall cycle is 24.990 ms, main world 4.233 ms, extraction 5.832 ms, render thread 19.070 ms, queue 5.429 ms, preparation 3.120 ms, graph/present 9.803 ms, and graph submission 5.415 ms. Main-world and render-thread work overlap; these values must not be added as independent costs. The retained extraction is `predecessor-settled-render-tail.json` in this task's evidence folder. Fresh Tracy attribution remains required before choosing a fix.

Read-only source investigation found two finalization callers per frame, current per-view terrain complement proof, profile-only whole-phase terrain draw counts, and broad change-filtered source queries. These are candidates, not measured bottlenecks. Finalization polling predates the historical comparison. Existing retirement tests require exact mesh/material draw receipts or authoritative current unviewed observations for every replaced source patch; retaining hierarchy while waiting for a draw intentionally does not block world-ready.

## Review leads checked before the new capture

The lead supplied `D:/Mudcrab/review/drafts/DS-FRAMETIME/FRAMETIME.md` as unverified leads. Its claim of 5,678 cluster draw groups is incorrect: `sample_renderer_path` in `render.rs` reports the peak stock `IndirectParametersBuffers` batch-set count. The prior settled runtime log separately reports **16 camera cluster draw groups and 10 per shadow view**, 9 camera pipelines, 3 material bind groups, and all 10,111 batches bindless. Each retained 600-frame interval reports cluster batch and shadow queues at 0.01 ms/frame each, zero full walks, cull encoding at 0.03 ms/frame, and zero cull parameter uploads. These observations remain subject to fresh trace verification but reject the proposed cluster-group causal attribution.

The review's 36,443 prepared-but-undrawn count is a global swap gauge, not a terrain-batch classification. `record_swap_gauges` intentionally includes surfaces no view can see; their continued waiting does not establish a failed transfer or continued GPU preparation. The separate terrain-ready-but-undrawn gauge is also documented as an upper bound including unviewed patches. Neither number establishes how many meshes one retained chunk holds.

The suggested inventory dirty-flag patch is incomplete: it omits coverage/tier changes, role marker and mesh/visibility removals, and query disabler changes. Its broad `Changed` filter still checks matching rows. It has not been applied. Any replacement must preserve exact diagnostic membership while avoiding unrelated invalidation.

The original thread checkout contains an unrelated modified `.deepseek-agents.json`. The parent integration checkout contains untracked diagnostic outputs. Neither checkout is modified by this task.

## Run controls

All Cargo commands use `BUILD_GATE_OWNER=sol-2145`, the isolated target, and `build_gate.py run`. All engine launches use `build_gate.py test` via the repository capture wrapper or the evidence runner. Every launch is labeled `not-a-timing-run`. This task creates no timing holds and waits for existing holds to clear. The evidence runner copies predecessor assets, pose, renderer, cluster package, and terrain flags without a terrain-radius argument.

An independent Codex read-only investigation traces terrain finalization and recent source changes while the baseline build runs. Historical source changes are candidates, not evidence of a regression. No behavioral fix is chosen before the post-ready capture.

## Historical execution checkpoints

At 08:47 on 2026-10-25, the baseline release Tracy build remains active inside the gate. Cargo reported an unexpected sccache server shutdown and restarted local engine-library compilation; the seeded executable must not be used as the new baseline. `baseline-tracy-build.exit` must exist and contain zero before capture.

The independent `timing-int` hold, owned by `measure-int`, began at 08:43:49 and advertises 10:03 as its end. Its owner must explicitly clear it; elapsed advertised time is not authorization to run. No engine has been launched by this task.

A Codex capture child is active under request `sol-2145-postready-baseline-capture-1`. It waits for the build receipt and all holds to clear, runs the discarded warm-up and retained 180-second TEST capture through the prepared wrapper, and returns the verified post-ready 30-second system/GPU attribution. The parent retains implementation, regression tests, integration, and review. No Rust changes, test results, performance fix, or local commit is claimed at this checkpoint.

At 09:02, the baseline Tracy release build completed successfully in 35m36s, exit 0. Executable SHA-256: `0ba55c0e845bc193aa3e96cc89d7322dac071ba88fa6dbd31a0bef83022c9fba`. The build's execution session was collected; no Cargo, rustc, or engine process remained. The timing hold was still active. A baseline formatting check is queued inside the gate. Following the lead's explicit correction, this implementation turn remains active through all waits and only ends with the completed final report or a blocker requiring the lead.

## Fresh base attribution

The requested base already contains `684341e52`, which suppresses baked native LOD demand whenever a cluster package is selected (`streaming/lod.rs`). Unlike the predecessor binary, this source reaches ready with 32,692 mesh entities, zero native LOD patches, all 4,796 cells completed, and 25,525 distant references. The predecessor's 81,380 meshes and 4,517 finalization scans are not a paired baseline for this branch. No attempt is made to restore obsolete demand merely to reproduce that cost.

The retained `baseline-tracy` TEST reaches ready at 35.558 seconds. Its final 30 seconds include 1,687 render frames, starting 122.381 seconds after ready. Render schedule wall time averages 15.826 ms; the main-world render wait averages 11.624 ms. Recorded GPU pass sum averages 7.407 ms. CPU render-path dominance is an inference; overlapping/nested spans cannot be added, and wall time does not distinguish active CPU execution from internal waits.

| CPU system | Inclusive ms/render frame | Self ms/render frame |
| --- | ---: | ---: |
| Submit pending command buffers | 6.0573 | 0.0109 |
| Main transparent pass | 3.5078 | 0.0022 |
| Cluster extract sources | 1.2801 | 0.0112 |
| Transparent3d batch preparation | 0.8496 | 0.8496 |
| Finalize terrain clips | 0.7335 | 0.0029 |
| Terrain handoff filter | 0.6764 | 0.6764 |
| CPU visibility culling | 0.3995 | 0.1498 |
| Directional-light mesh visibility | 0.3250 | 0.0674 |
| Prepare preprocess bind groups | 0.3089 | 0.3089 |
| Collect benchmark metrics | 0.2422 | 0.2422 |

Transparent encoder finish averages 4.5699 ms, contained within submission's parallel-finishing interval. Actual queue submission averages 1.3218 ms. GPU opaque is 2.3948 ms, early prepass 2.0540 ms, shadow cascade 3 0.6021 ms, transparent 0.3414 ms, shadow cascade 2 0.3368 ms, cluster cull 0.3156 ms, and cluster instance cull 0.2959 ms. Complete tables, trace, benchmark, and selected-window outputs are retained in `C:/MudcrabBuilds/cluster-postready-target/baseline-tracy`.

Fresh settled work is clean: `settled_work.after_ready = {}`. Cluster camera groups are 16 and shadow groups 10, with zero settled queue rebuilds or walks. Global waiting surfaces remain 97, visible-undrawn zero. The diagnostic exits 1 solely for 31 startup commit-budget violations, not drawability failure. Startup Tracy `SymInitialize` code 87 remains; no Vulkan validation error was found.

The original shared Bevy draw-command observer, enabled only for `baseline-draw-counts`, measures 5,614 issued stock transparent fixed-indirect API calls and 5,615 indexed argument slots per settled frame. Its registry has additional unwrapped custom variants, so total transparent coverage is not claimed. Stock sorted batching already supports ordered multidraw across different meshes. A material-type partition is being added before selecting a behavioral change.

`NifDepthExtension` contributes only pipeline specialization data and no GPU resources, but its non-bindless derive prevents its StandardMaterial base from grouping bindless materials. A naive `#[bindless]` derive would introduce an empty index table and is unsuitable. If material counts confirm this population dominates, the responsible fix is an explicit resource-free `AsBindGroup` implementation preserving the base descriptor and device capability checks. Sorted item order, authored depth/alpha/decal/shadow state, shaders, geometry, and settings must remain unchanged.

The opt-in material partition compiles under `cargo check -p engine --all-targets --features tracy` (exit 0). Seven focused tests pass, including original draw-function registry substitution, asynchronous report deltas, successful-call type partition, and all 32 authored specialization states. Two acceptance tests fail on the original implementation as intended: combined slot limit is `None` instead of StandardMaterial's `Some(Auto)`, and the extension has no bindless descriptor. Red receipt: `nif-depth-red` (exit 100); instrumentation receipt: `material-partition-tests` (exit 0). A separate Codex read-only review found no actionable issue in the diagnostic partition or acceptance tests.

The material-count release Tracy build completed in 30m47s (exit 0); immutable executable `material-partition-tracy-before.exe` has SHA-256 `3727fbca7039a632f38240e9bdc719f3ed64b661b288d086299d8b03bd2fcbd5`. After a discarded 90-second warm-up, retained `partition-before-counts` reaches ready at 49.594 seconds. Its final 1,000 frame samples each issue 5,612 observed stock transparent calls: 5,599 NifDepth (99.768%), 12 StandardMaterial, one other, zero missing identities; 5,613 fixed indexed argument slots. Warm-up counts were 5,600/5,613 NifDepth/total, with the same other populations. This establishes material call ownership rather than raw surface population. Additional unwrapped custom draw variants remain explicitly outside coverage.

This count-only TEST overlapped the separately owned `cluster1-all-primitives` TEST. Available memory fell below 1 GiB; its frame times cannot support a performance comparison. The new source check correctly waited inside the gate for memory. All settled rebuild counters remain absent/zero (`clean = true`), with 32,692 meshes and no failed cells. Benchmark exit 1 is the startup commit-budget condition; streaming lifecycle and failure checks pass. The original uninstrumented-at-draw baseline remains the CPU/GPU attribution reference.

The behavioral candidate replaces only the extension's derived binding implementation with an explicit resource-free implementation. StandardMaterial retains its slot limit, descriptor, hardware support check, layout, uniforms and textures. The extension retains `Data = NifDepthState`, adds no bindings or index tables in either mode, and leaves its alpha routing, prepass exclusion, pipeline specialization and shaders unchanged. It permits existing ordered stock batching; it introduces no sorting or rendering algorithm.

To preserve disk headroom, gated `cargo clean -p engine --profile dev` reclaimed 3,185 disposable task-target artifacts / 12.1 GiB. Evidence and release binaries remain. No other owner's artifacts were cleaned.

The actual binding implementation received a fresh independent Codex review with no actionable source defect. A device-backed fixture now checks that the combined layout equals StandardMaterial in both forced/non-forced modes, that device support remains base-owned, and that the empty extension layout/bindings prepare on a physical Vulkan adapter. Its first compilation exposed an adapter-wrapper logging error; the log now prints name/backend fields. The all-target/all-feature engine Clippy retry passes with warnings denied. Device execution and green semantic tests remain pending at this checkpoint.

If command batching leaves the scene above budget, the existing baseline's nested spans identify narrower next candidates: source change queries consume 0.9394 ms/frame inside 1.2801 ms source extraction; terrain clipping constructs phase membership in 0.5893 ms/frame across five views inside its 0.7335 ms/frame total. Those costs remain despite zero expensive settled rebuild counters. They are not added to their enclosing systems, and no additional behavioral fix is selected before the candidate capture.

Green focused validation passes: 17 tests across the engine library and `nif_depth_bindings` integration binary, including both former reds, all 32 keys, existing authored render semantics, diagnostic behavior, and physical Vulkan layout/binding checks in both binding modes. `nif-depth-green.exit` is zero. Build time was 9m40s; test execution 1.337s.

At approximately 11:00, shared C: usage reduced free space to 11.18 GiB despite earlier cleanup. To preserve the required 10 GiB floor, the task stopped its verified compiler PID 51240. The interrupted candidate release build consequently exits 101; this is an intentional resource stop, not a compiler diagnosis. Its previous `release/engine.exe` must not be treated as a fixed binary. No owned Cargo/compiler/engine/recorder orphan remained after collection.

The task archives only its idle dev cache from `C:/MudcrabBuilds/cluster-postready-target/debug` to `D:/Mudcrab/builds/cluster-postready-dev-cache-archive/debug`. Source/destination absolute paths and ownership were verified before the native move; archive OWNER records identify sol-2145. D: had 48.3 GiB free, enough for the 18 GiB cache while preserving 10 GiB. This archive is not an active Cargo target. The actual target remains the requested C: folder. With C: recovering past 17 GiB during the move, the unchanged release-only build is retried as `candidate-tracy-build-retry`; no dev build starts during the archive.

The archive completed successfully at approximately 11:11. Observed free space is C: 31.77 GiB and D: 28.99 GiB. The archived cache had grown during green compilation beyond the earlier rounded 18 GiB inventory. The C: dev folder is absent; its replacement will be created by future gated verification. No lead action is required to clear this resource condition.

At 11:15:49 the shared sccache server dropped during the retry. Cargo reported its existing local-compilation fallback. Process inspection confirms exactly one task-target compiler, PID 32108, started at that time; no duplicate compiler was found. The active gated build continues. Future task-local Cargo helper launches clear only `RUSTC_WRAPPER`, avoiding another shared-daemon restart while retaining the same compiler, release profile, features and build gate. No shared server or configuration is changed.

The normal baseline diagnostic reused a verified same-HEAD parent binary, copied locally with SHA-256 `474ca1c2ad2f6bfa716b67e4dc78c7a7382873ce6d5ee2c734cd1df53731af66`. After a discarded warm-up, its 3,471 post-ready frames show p50 20.1271 ms, p95 31.1649 ms, p99 33.0057 ms. This disturbed TEST disagrees with profiler-tail cadence and is not quiet timing acceptance or a controlled speedup baseline.

The candidate Tracy release retry completed successfully (exit 0, 35m46s). Immutable executable candidate-tracy-bindless.exe has SHA-256 96fdea87a24c34c0e89c47513c0245b9ed1c3ca599d72f9a0ea72e517a03aaa7. The first 90-second scene run is discarded as warm-up; it reaches world-ready at 87.695 seconds under competing builds and retains clean settled rebuild counters. Its timing is not used. A separate material-count run follows. The normal build was cancelled while still waiting in the gate, with no compiler child, so retained Tracy attribution can run before another long release compilation. Workspace Clippy remains active; no timing hold was created.

Retained partition-after-counts reaches ready at 46.875 seconds and records 10,205 post-ready frames. In each of the final 1,000 samples, observed transparent API calls fall from 5,612 to 1,653 (70.5%); NifDepth falls from 5,599 to 1,640. StandardMaterial remains 12 calls and other materials one, with zero missing material identities. Fixed indexed argument slots are 5,614 versus 5,613; slots are commands, not geometry coverage. Both reports retain explicit incomplete coverage of custom variants. This run establishes the intended batching effect but overlaps workspace compilation and is not timing evidence. Comparison: target-local material-count-comparison.json.

## First source-fix attribution: authored-depth bindless composition

The retained `candidate-tracy` run started with all build slots free, no holds and no other engine/compiler process. It reaches ready at 36.089 seconds and retains 12,649 post-ready frames. The exact final 30-second window has 2,493 render frames and starts 121.422 seconds after ready. Recorded GPU pass sum is 7.297 ms versus 7.407 ms before. Render schedule wall time falls from 15.826 to 10.125 ms; the companion render-cycle tail reports main-thread render waiting of 6.035 ms versus 11.624 ms. CPU render-path dominance remains an inference, with profiler overhead and internal waits included.

| CPU system | Base inclusive ms/frame | Binding fix inclusive ms/frame |
| --- | ---: | ---: |
| Submit pending command buffers | 6.0573 | 3.1460 |
| Main transparent pass | 3.5078 | 1.0481 |
| Cluster extract sources | 1.2801 | 1.2432 |
| Transparent3d batch preparation | 0.8496 | 0.8387 |
| Finalize terrain clips | 0.7335 | 0.7608 |
| Terrain handoff filter | 0.6764 | 0.6645 |
| CPU visibility culling | 0.3995 | 0.3942 |
| Directional-light mesh visibility | 0.3250 | 0.3083 |
| Prepare preprocess bind groups | 0.3089 | 0.2677 |
| Collect benchmark metrics | 0.2422 | 0.2439 |

Transparent encoder finish falls from 4.5699 to 1.9612 ms; actual queue submission falls from 1.3218 to 1.0265 ms. These nested/parallel measurements are not added to submission. Terrain clip phase-membership copying remains 0.6128 ms inside finalization; the entire terrain handoff filter remains 0.6645 ms. Source change queries remain 0.9086 ms inside extraction. The first fix therefore removes the measured command fragmentation but does not reach the 8.3 ms frame budget.

The unchanged-binding engine all-target nextest run passes 3,269 tests with 13 skipped (`candidate-engine-nextest.exit = 0`). Further code changes require fresh checks.

The acceptance screenshot path captured a black loading frame on the baseline and is rejected as visual evidence. The two screenshot runs retain 99,220 entities each and identical one-time post-ready readiness invalidations caused by capture. Delayed capture is required before claiming visual acceptance. No screenshot comparison or timing acceptance is claimed from those runs.

The next source fix targets native phase membership. Terrain proof asks about a small set of stock terrain meshes but currently enumerates every stock phase into temporary sets twice per render frame. The native phase owner already has binned reverse keys and sorted hash storage. Exact lookups must preserve current view identity, current post-filter membership, every binned category, arbitrary sorted render carriers, batch followers, and all existing drawable/mesh/pass proof. A new counter records each remaining binned/sorted full membership scan, and a production-path work test must first fail on the copying implementation. No visibility or readiness cache is introduced.

## Resume and visual corroboration

The user cleared the gaming hold and resumed this task on 2026-10-25. The native-accessor child was interrupted; the parent reviewed its entire diff and owns completion and integration. The interrupted membership red attempt exited 1 without test execution, so it is not a semantic red. The parent reruns the copying implementation before changing the lookup boundary. Per the updated brief, this report lives under `docs/evidence`; no task note is kept at the repository root.

Delayed captures use the original pose, explicit original screenshot camera offset, and 10,000 warm-up frames solely to avoid photographing the startup overlay. The baseline uses the retained normal base executable; the binding-only candidate uses its retained Tracy executable. These are visual observations, not performance comparisons. Both show the same mountain, distant rocks, trees and city geometry on inspection. Excluding the changing FPS HUD, mean absolute RGB channel error is 0.1025 on a 0-255 scale, and 0.1114% of pixels differ by more than eight in any channel. Those small image differences do not prove all depth/material semantics; the semantic and device-backed tests provide separate evidence. Images and `visual-binding-comparison.json` remain in the task target.

The stock Bevy indirect batch-set metric is a whole-run peak, not a settled-frame sample and not a cluster group count. It falls from 5,685 in `baseline-tracy` to 1,717 in the binding-only `candidate-tracy`. Separate instrumented count runs report peaks of 5,684 and 1,711. These must not be presented as exact settled counts or mixed with the fixed cluster camera/shadow group counts.

## Native membership implementation and proof

The resumed copying implementation fails the intended work test: three forward lookups build three snapshots and visit 1,024 unrelated phase members, versus the required zero builds and visits. Receipt `phase-membership-red-resume.exit` is 100; the failure is an assertion after successful compilation, not a compiler failure.

The completed native binned accessor follows the existing reverse key to its current actual bin. It handles multidrawable, batchable, unbatchable and non-mesh entities, rejecting missing keys, absent members and stale caches. The sorted accessor probes the existing IndexMap by the main entity's hash, using Bevy 0.19's actual EntityHash contract and checking the main entity in the probe. It preserves custom and duplicate carriers, entity generations and empty-range batch followers. Neither API mutates membership or ordering, allocates an index, or walks unrelated phase population. Arbitrary public-bin edits that bypass Bevy's native cache are outside the binned owner's queue contract, as they already are for native removal.

Terrain complement filtering, late terrain clipping and profiling-only terrain draw counts now use these exact lookups. Filter and finalization obtain current membership independently; no pre-filter snapshot is retained. Existing mesh identity, material/pipeline preparation, required prepass, shadow/view domains, coverage and retry conditions remain unchanged. The existing removal/next-frame restoration fixture additionally compares exact lookup against the former enumeration oracle.

A fresh independent Codex review found no actionable defect in the APIs or integration. The first final Clippy attempt exposed a test import leaking into pre-existing native tests and one ambiguous test entity conversion. New native tests now have their own module, and the conversion names MainEntity explicitly. Formatting passes, and `final-workspace-clippy-retry.exit` is zero with all workspace targets/features and warnings denied. Seven Rust source hashes are retained in `final-source-hashes.json`; final tests and runtime captures use this fixed snapshot.

Final touched-crate green passes 3,293 tests: 3,270 engine tests and 23 native renderer tests, with 13 skipped. This includes the former work-test red, ten new native membership tests, current-frame removal/next-frame restoration, authored depth semantics and physical Vulkan binding composition. Receipt `final-touched-nextest.exit` is zero; the run takes 77.084 seconds after compilation. No failures appear in the full log.

The combined optimized Tracy build completes successfully in 22m42s. Immutable `final-tracy-2145.exe` has SHA-256 `c8a030cd0dbd6cb88750ee5460e879d953f726796854069b677d05efe2510d01`. Source hashes still match after both compilation and tests. Remaining workspace checks complete before retained runtime attribution; builds and profiling are not overlapped.

Workspace all-target/all-feature Clippy passes with warnings denied (workspace-clippy.exit = 0, 14m07s); the dependency future-incompatibility notice remains. After all task Cargo processes stopped, the 1.7 GiB new Clippy cache was moved to the owned D: check-cache folder and merged into the owned archived dev cache with no copy failures. The requested C: target/debug is now a native junction to that task-owned dev cache. CARGO_TARGET_DIR stays C:/MudcrabBuilds/cluster-postready-target. This reuses prior test artifacts and preserves the 10 GiB C: floor without deleting other owners' data. Both D: folders and their dev subfolders carry ownership records. The archive now backs active development artifacts and must not be deleted as idle cache.

## Final traversal evidence and unresolved acceptance

All three captures use unmodified route files with 27 waypoints, 129,795.15 units and 400 units eye height, start at world-ready and complete one full lap. Every run reads all 117 ground cells, with zero route frames missing ground, failed streaming cells, asset-load failures or material-validation failures. These are gated TEST diagnostics, not quiet timing acceptance. Fly-fast uses the same waypoints at its authored 4,000 units/s; no route is shortened.

| Capture | Route p50 ms | Route p99 ms | Route worst ms |
| --- | ---: | ---: | ---: |
| Base fly, 900 units/s | 18.4237 | 137.3175 | 703.0452 |
| Final fly, 900 units/s | 19.0853 | 210.9414 | 1085.9508 |
| Final fly-fast, 4,000 units/s | 193.4999 | 595.8662 | 598.8562 |

Both base and final full fly runs emit persistent `owned_not_drawable=1` after motion begins. The final fast run ends at the same value. This count includes a stale/missing-observation sentinel and is not a count of distinct missing objects; actual visual geometry loss is not established. Reproduction in the immutable unmodified base shows that this warning predates both fixes. Traversal drawability proof is not accepted. No owned stock replacement or fallback is enabled.

The full final route has worse p99 and worst time than the full base in these two TEST runs. Cause is unresolved; no traversal speedup, regression attribution or stability claim is made. Final fly records 5,397 post-ready frames and 133 commit-budget violations; base records 6,122 and final fast 112 post-ready frames, with 122 fast commit-budget violations. Every route exits 1; `commit_budget_respected` is the only false Boolean benchmark threshold. The separate route frame-budget pass bar fails; these permissive launch thresholds do not constitute acceptance.

Final moving-view source invalidation reaches **884.5613 ms qualification time** while refreshing **23,072 references**, scanning **22,979 roots** and **25,511 primitives**. The retained top-ten log records are `final-fly/source-invalidation-maxima.txt`. This is post-ready during the route, not loading. Base also performs broad moving-view invalidation. This unchanged source-observation/invalidation owner, plus its persistent proof withdrawal, is the next root-cause investigation. The current native membership fix removes whole-phase copying only; it cannot make these invalidations free.

The requested 120 to 140 FPS, 8.3 ms maximum budget and stable traversal remain **unachieved/unaccepted**. Literal zero per-frame checks is also unmet despite zero settled full-rebuild/copy counters. Both local improvements have complete source verification, but this is not an acceptance handoff for the wider performance goal. Recheck native EntityHash/IndexMap and queue lifecycle contracts on dependency upgrades.

All evidence is retained under the owned target in `route-comparison.json`, `baseline-fly`, `final-fly`, and `final-fly-fast`. No engine or recorder is intentionally left running after the captures.
