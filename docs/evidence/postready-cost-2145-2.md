# Standing CPU follow-up, #2145.2

Owner: sol-2145. Branch: `phase2/cluster-postready-cost`.
Worktree: `D:/Mudcrab/worktrees/cluster-postready`.
Target and receipts: `C:/MudcrabBuilds/cluster-postready-target`.

## Scope

Fast-forwarded through `eaeddc022` to integration `0ef2bde23`, including #2147's
runtime-state distant stock. Both paired captures use that complete content.
No far-stock, readiness-bridge or streaming traversal implementation changes.
Geometry, settings, transparent ordering, authored materials, frozen ownership,
mutable invalidation, default-query disablers and recovery remain authoritative.
There is no terrain-radius override.

The lead's supplied I21 quiet timing was 103.1 FPS on main28 and 99.7 FPS on
cluster1. I21 predates the additional stock meshes in this comparison. The
smaller-world historical 8.7 ms result does not establish a regression. Tracy
runs below are diagnostic attribution, not quiet FPS acceptance.

## Engine-only candidate

The source extractor used wide Or change filters for identity, source facts,
hot facts and terrain visibility. Bevy 0.19 tests row ticks even when unchanged.
The candidate splits each disjunction into native single-input queries and
unions their results. This removes repeated branch/presence checks across broad
rows without changing dependencies or requiring producer-side mutation APIs.
Original disjunctions remain as independent test oracles. Duplicate dirty IDs
are harmless because the existing consumer deduplicates identities before
qualification, and identity/hot comparisons continue to suppress equal writes.

A second engine-only candidate admits each native single-input query through a
copied column-tick probe. It uses the existing 0.19 `Column::get_ticks_unchecked`
copy API and the exact system last/this tick window. Immutable World access
registers read-all access, excluding concurrent writers throughout extraction.
Sparse or unsupported storage conservatively enters the original query. The
probe includes disabled entities as a superset; the native filter remains the
actual domain authority. No raw tick/location references escape. Separate
counters report copied rows and admitted native queries.

This candidate still polls component populations. It cannot claim zero settled
scans. The lead explicitly prefers an engine-only alternative if it saves about
0.8 ms or more. The accepted paired attribution below determines that decision.
Per-extraction query counts report individual native query attempts;
process-wide settled counters record one execution of each query group.

The idle fly-camera writer also assigned its unchanged translation every frame.
The semantic regression failed with a changed tick of 75 versus 5. The writer
now compares the computed translation before writing. Tests include opposing
inputs cancelling movement and resumed movement when one key is released.

Terrain preservation coverage exercises real near-terrain outside stock source
roles, raw Visibility/InheritedVisibility writes, removal/restoration and a full
oracle extended to include terrain identities and eligibility. Review identified
an existing removal hole: Changed<T> cannot report a removed component. The
runtime regression failed at removal as expected (two other focused tests
passed). The candidate now invalidates terrain visibility when a journal/dirty
subject belongs to terrain; unchanged frames visit no additional population.
The corrected removal runtime proof is included in final engine validation below.

## Portable GPU fixture

The authored-depth layout fixture probes compiled Vulkan, DX12, Metal and GL
backends. It explicitly prints SKIPPED when no native backend or non-CPU adapter
is available. Available-device creation and layout/preparation failures remain
failures. Both native and forced non-bindless cases remain. Rust counts an early
return as a passing test, not a native ignored result. The probe and Bevy's
initializer select adapters independently. Actual Metal and adapter-less-host
execution remain unverified. The first check caught the wgpu 29 descriptor's
lack of Default; the corrected code uses new_without_display_handle().

## Complete-content before profile

Immutable executable: `r2-complete-baseline-0ef2bde23.exe`, SHA-256
`2ccab627837b4917694ab0d9e79e9a39f86e25ce64028c97678c3ea8f1dab0f0`.
Build passed in 26m19s. At the lead's instruction, both captures used the gate's
guard command under the lead-created `timing-2145-capture` hold. The hold was
cleared immediately after the retained capture. No compilation occurred in this
task during that window. The first 90-second warm-up was discarded; its guard
was disturbed by other Cargo processes. The retained 180-second capture's guard
was quiet: 5.4% other CPU and zero intruders. Profiler CPU counts as other CPU in
this guard. These remain diagnostic captures, not quiet FPS acceptance.

Selected tail: 30 seconds, 2,342 render frames, starting 122.394 seconds after
world-ready. Inclusive CPU observations per render frame, with enclosing
scheduler wrappers omitted:

| System | Before ms |
| --- | ---: |
| Submit pending command buffers | 3.6467 |
| Extract cluster sources | 1.6251 |
| Main transparent pass | 1.1997 |
| Transparent3d sorted batch preparation | 0.9131 |
| CPU visibility culling | 0.4609 |
| Directional-light mesh visibility | 0.3794 |
| Prepare preprocess bind groups | 0.3493 |
| Collect benchmark metrics | 0.2702 |
| GPU visibility culling | 0.2499 |
| Extract cameras | 0.2107 |

Source change queries alone cost 1.2388 ms/frame inside extraction. All four
query-attempt counters record 13,004 post-ready scans apiece; other settled-work
counters are zero. This measured cost justified investigating source mutation
observation. Transparent encoder finishing is 2.2834 ms and actual queue submit
1.1736 ms; finishing overlaps across workers and is contained in the submit
interval. These values must not be added together as serial system costs.

Render schedule: 10.3811 ms/frame. Approximate companion cycle rows report
10.3462 ms render-thread work, 2.1924 ms extraction, and 5.3853 ms main-thread
render wait. The GPU pass sum is 7.6718 ms/frame: opaque 2.3961, early prepass
2.1157, shadow cascade 3 0.6718, cascade 2 0.3805, and cluster cull 0.3718 ms.
The benchmark's whole-run GPU aggregate was also inspected; it mixes loading
with standing, so the exact post-ready Tracy window supplies the comparison.
CPU-path dominance is an inference from schedule/wait versus GPU observations;
driver calls contain waits and the GPU sum is not a matched whole-frame fence.
GPU has only approximately 0.66 ms pass-sum margin to 8.33 ms.

Content grader passes: 112,278 entities, 35,713 meshes, 29,116 validated transform
instances, 25 resident cells, 27,106 distant references, zero undrawn static or
runtime-refused geometry, zero failed cells/assets. Stock Bevy batch-set peak is
1,727; this does not count custom cluster groups. No draw-count observer was
enabled in this attribution capture. Grader performance fails at diagnostic
85.55 post-ready FPS; traversal is N/A. The engine exits 1 with 34 startup
commit-budget violations. No new speedup or global zero-work claim is established.


## Discarded first candidate

The split-query-only capture `r2-engine-only-after` is rejected as paired timing:
its guard verdict is disturbed (6.6% other CPU against the unchanged 6.0% limit).
Its .8923 ms source-query observation is only an attribution lead against the
1.2388 ms baseline, below the lead's approximately .8 ms desired saving. This
motivated the copied-tick candidate; it does not establish a speedup. The first
candidate's 90-second warm-up was quiet and discarded. The hold was cleared
immediately; no engine/recorder orphan remained.

The final Tracy build includes lead cherry-pick `8189a981e`, locally
`06c2722a0`, which limits the Tracy listener to loopback and avoids Windows
Security dialogs from all-interface listening at new binary paths.

## Accepted final paired profile

The final release Tracy executable is `r2-engine-only-final-06c2722a0.exe`,
SHA-256 `77dc9d7fde53e04af554a162c6a34c9ae5dc0c684179f0da34f6eda44d22d103`.
Its seven source/manifest hashes match the final verification and build snapshot.
The 56m33s build retains ordinary release optimization, thin LTO and one codegen
unit. The engine's embedded fingerprint says commit unknown/clean; external
source hashes, the retained source diff, HEAD and executable hash establish its
actual dirty-source identity.

Capture order followed the lead: #2151.1, this task, then #2153. After
`timing-2151` cleared, this task set its own bounded hold with drain. No build
ran inside its window. The first 90-second warm-up was discarded, guard quiet.
The retained 180-second capture was also quiet: 4.4% other CPU, peak 19.8%, zero
intruders. The hold was cleared immediately after capture; no owned engine or
recorder remained. Export/analysis ran through normal gate admission afterward.

The final capture reached ready at 34.8020 seconds and contains 13,767 post-ready
frames. Its selected last 30 seconds contain 2,705 render frames and begin
123.369 seconds after ready. Baseline and final use the same main28 package,
pose, resolution, renderer, graphics fingerprint and complete-content base.
These are one paired diagnostic observation, including profiler overhead.

| System / span | Before ms/render frame | After | Delta |
| --- | ---: | ---: | ---: |
| Source change queries, nested | 1.2388 | 0.8120 | -0.4268 |
| Extract cluster sources | 1.6251 | 1.1455 | -0.4796 |
| Fly-camera writer | 0.0036 | 0.0030 | -0.0006 |
| Submit pending command buffers | 3.6467 | 3.2691 | -0.3776 |
| Main transparent pass | 1.1997 | 1.0707 | -0.1290 |
| Transparent3d batch preparation | 0.9131 | 0.8437 | -0.0693 |
| CPU visibility culling | 0.4609 | 0.4160 | -0.0449 |
| Directional-light mesh visibility | 0.3794 | 0.3348 | -0.0446 |
| Prepare preprocess bind groups | 0.3493 | 0.3036 | -0.0457 |
| Collect benchmark metrics | 0.2702 | 0.2314 | -0.0388 |
| GPU visibility culling | 0.2499 | 0.2274 | -0.0226 |
| Extract cameras | 0.2107 | 0.1934 | -0.0173 |
| Render schedule | 10.3811 | 9.2633 | -1.1178 |
| GPU pass sum | 7.6718 | 7.8462 | +0.1744 |

The observed source-query reduction is 0.4268 ms, about 34.5%; it misses the
lead's approximately 0.8 ms engine-only preference threshold. Extraction falls
0.4796 ms. Unchanged CPU systems also improve broadly, so the whole render
schedule's 1.1178 ms reduction cannot be attributed to this fix. Root-cause
inference is supported by exact query admission counters and semantic tests;
the isolated timing benefit remains a single paired observation. The camera's
intended proof is its unchanged tick, not its sub-microsecond wall-time delta.
Terrain removal is a correctness fix; no removal event occurs in this standing
capture, so there is no separately measured removal-path speedup.

Approximate companion render-cycle rows cover at least 30 seconds, independently
of exact Tracy boundaries: render thread 10.3462 to 9.2347 ms, extraction 2.1924
to 1.6309 ms, main render wait 5.3853 to 4.8497 ms, wall cycle 12.6518 to
10.9639 ms. Transparent encoder finish is 2.2834 to 2.0366 ms; actual queue
submit is 1.1736 to 1.0683 ms. Finish work overlaps across workers inside the
submission interval; these are not additive serial costs. Recording/finishing
and sorted preparation were measured, not modified or proven to be active CPU
rather than synchronization throughout those driver calls.

All four native query-group counters are zero after ready. Near the tail, each
extraction still reports **1,300,211 copied tick rows**. Consequently,
`settled_work.after_ready` is
`{"cluster_sources/copied_tick_probe": 13767}` and `clean` is **false**.
This round does not achieve zero population polling. The remaining work scales
with stored components, and unrelated writes can reopen native scans. The
read-all World access is also a scheduling constraint; its net traversal cost
has not been established. The unlinked native prototype is not justified as a
ready solution by this sub-threshold result. A separate upstream-summary-tick
backport assessment remains the path toward zero row scans.

The supplied grader reports CONTENT PASS with identical counts and refusal
classes: 112,278 entities, 35,713 meshes, 29,116 validated transform instances,
25 resident cells and 27,106 distant references. Both undrawn counters are zero;
failed cells/assets, renderer validation failures and logged ERRORs are zero.
Stock Bevy batch-set peak stays **1,727**; cluster logs retain 16 camera groups,
10 shadow groups, and zero settled queue rebuilds/walks. The draw-count observer
is off, so no paired API draw-call count is established in this round.

The grader's diagnostic post-ready FPS is 85.55 before and 89.98 after; final
p99 is 14.87 ms. PERF and OVERALL FAIL against 120 minimum/140 goal; traversal
is N/A. This is not a quiet, profiler-free FPS comparison. Engine exit 1 is
retained: its only false boolean acceptance condition is commit-budget respect,
with 32 startup violations (baseline 34). The inherited attribution flags do
not enforce an FPS threshold. Existing startup SymInitialize and asset tangent/
enable-state warnings remain; no Vulkan validation error is logged.

CPU dominance remains an inference: final render schedule 9.2633 ms exceeds the
7.8462 ms GPU pass sum. GPU does not yet establish the active limit, but it has
only about 0.487 ms pass-sum margin to 8.333 ms and already exceeds the 7.143 ms
140 FPS budget by about 0.703 ms. Largest final GPU passes are opaque 2.4371 ms,
early prepass 2.2246, shadow cascade 3 0.7005, cascade 2 0.3814, cluster cull
0.3747 and cluster instance cull 0.3395. GPU is a likely next limit after further
CPU fixes; pass sums are not matched whole-frame fence measurements. No GPU
regression is established by one +0.1744 ms observation.

Receipts: `r2-comparison.json`, `r2-comparison.md`, and each retained run's
`postready-tail.json`, `postready-detail.json`, `benchmark.json`, guard verdict,
launch identity, runtime log and `capture.tracy`; final `verdict.txt` retains the
grader output. Source and build receipts are named in the verification section.

## Upstream capability and engine-only decision gate

Checked upstream on 2026-10-25. **In 0.20: yes (0.20.0-dev main), PR #25157
and PR #25946, both merged.** The first adds opt-in per-column summary ticks;
the second uses them to skip entire tables in ordinary dense Changed/Added
queries. Main's Cargo.toml currently identifies 0.20.0-dev. This is not a claim
that a released 0.20 is installed here.

Primary sources:
- https://github.com/bevyengine/bevy/pull/25157 (merged 2026-08-13)
- https://github.com/bevyengine/bevy/pull/25946 (merged 2026-09-29)
- https://github.com/bevyengine/bevy/blob/main/crates/bevy_ecs/src/query/filter.rs
- https://github.com/bevyengine/bevy/blob/main/Cargo.toml

The checked upstream main is `fd9e7609109758a2d9af401345f100be9442e9f6`.
GitHub API receipts and inspected source are retained under target-local
`r2-upstream-capability.json` and `upstream-*`. Upstream's summary ticks are
opt-in table-component metadata; sparse storage is explicitly rejected by
ComponentDescriptor. Automatic table skipping applies to dense queries. This is
therefore an existing capability and a backport starting point, not a drop-in
late-enable revision API for every third-party source component. No dependency
upgrade or upstream backport is part of this round.

Primary boundary source:
https://github.com/bevyengine/bevy/blob/main/crates/bevy_ecs/src/component/info.rs

Locked 0.19 has no column/archetype mutation summary. Its row tick getters
require reading every matching component; archetype generation records new
archetypes, not mutations. Lifecycle observers do not report arbitrary mutable
writes. A certified-only query set would lose discovery of newly qualifying
references; indexed iter_many still polls the complete ancestry index.

The first engine-only candidate splits each wide Or into single-input native
queries and unions changed identities, with identical observation allowances,
semantic comparisons and journal recovery. This removes per-row wide filter
branching without changing dependency code or writer contracts. It still polls
row ticks; it cannot claim zero settled population work. This round keeps a
bounded measured reduction, but its 0.4268 ms query saving does not meet the
lead's approximately 0.8 ms preference threshold. The native revision prototype
remains unlinked and unvalidated under its final policy. If native work is pursued,
backporting upstream summary ticks should be assessed before maintaining a
separate component-wide atomic increment design. Upstream explicitly minimizes
read-modify-write operations in the mutation hot path.

## Verification and remaining acceptance

Engine-only cargo check including tests passed after the descriptor correction.
The final copied-tick integration passed cargo check for engine tests and
27 focused nextest cases: 23 direct copied-tick/native-filter comparisons, the
quiet-admission work regression, split/native-disjunction differential proof,
real terrain visibility mutation/removal/restoration with full oracles, and
idle fly-camera change ticks. The prior source-query red showed four broad
query attempts versus zero expected; the helper's quiet-table stub separately
failed red. Original terrain removal failed before its journal correction.
Final Rust gates passed on 2026-10-26 with all seven source/manifest hashes
unchanged: fmt; engine and workspace all-target/all-feature Clippy with warnings
denied; engine all-target nextest 3,330 passed/13 skipped; workspace nextest
5,567 passed/63 skipped; workspace doctests 18 passed/15 ignored. The portable
binding fixture passed in both engine and workspace suites. Actual Metal and
adapter-less-host execution remain unverified. Receipts and source hashes:
`r2-final-verification.json` and `r2-final-proof-summary.json` in the target.

Two strict-Clippy test-fixture lint failures were corrected with a named
SystemParam. A shared-load LLVM out-of-memory compilation attempt did not run
tests; the successful unchanged-source retry used one Cargo job. Failed receipts
remain under `r2-final-initial-*`, `r2-final-tuple-*` and `r2-final-oom-*`.
A later waiting admission was stopped before Cargo to handle shared C: pressure;
owned cache files were compressed without deleting artifacts. The lead then
freed a finished unrelated cache. Resource floors and test selection were not
relaxed. The isolated native ECS prototype is not linked, vendored
or part of the engine's dependency graph. No native API is referenced by the
kept engine candidate. No push or PR is authorized.

## Isolated native prototype scope (not shipped)

Linking the native mutation-revision API would require a new vendored
`bevy_ecs-0.19.0` crate beside the renderer/PBR patches, with its own upstream
justification and PR. Against pristine registry 0.19, the isolated prototype
changes 17 Rust source files (16 modified, one new), +1,489/-41 lines, including
tests. Manifests, generated files and temporary artifacts are excluded. Normalized
per-file counts and hashes are retained at
`D:/Mudcrab/builds/cluster-postready-ecs-prototype/r2-review/delta-vs-pristine.json`.

A native revision can observe ordinary raw ECS mutations, which lifecycle
observers alone cannot cover. It adds mutation-hot-path atomic increments and
wrapper/storage provenance. That complexity is not justified if the engine-only
candidate reaches the lead's saving threshold. Upstream column summary ticks
provide a better starting point for a separate zero-row-scan backport than
maintaining this novel type-wide revision mechanism. Neither native design is
linked or measured in this engine comparison.

The agreed escape boundary is permanent invalidation for public raw tick/location
escapes, including escapes before tracking starts. Copied reads remain trackable.
The prototype implements that boundary but its final policy changes are uncompiled:
prior review fixes passed 19/19 track_location tests, then three permanent-escape
regressions failed as intended. Final focused green was cancelled before Cargo
admission after native integration was deferred. Full native suite, Clippy and
engine validation remain unproven. Its files and patch remain outside the
repository; these partial receipts do not count as final engine verification.

## Merge contract with #2151.1

The mutation probe is generic over `T: Component`; the macro generates a native
query and its matching probe from each declaration. There is no fixed query
count or special FlatSourcePrimitive/GlobalTransform gate. This round does not
change HotFactState, hot_facts, its pose comparison, or origin-rebase invalidation.
The existing hot_facts call remains semantically unchanged inside the new loop.

At this branch's complete-content base, SourceFactQueries declares exactly:

| Field | Changed component | Additional filter |
| --- | --- | --- |
| cell | CellRef | none |
| scene | WorldAssetRoot | none |
| original_scene | OriginalStockScene | none |
| flat_primitive | FlatSourcePrimitive | none |
| flat_pose | GlobalTransform | With<FlatSourcePrimitive> |
| handle | MeshHandle | none |
| mesh | Mesh3d | none |
| primitive | SourcePrimitive | none |
| primitive_mesh | SourcePrimitiveMesh | none |
| primitive_transform | SourcePrimitiveTransform | none |
| static_model | SourceStaticModel | none |
| node_pose | SourceNodePose | none |
| cell_key | StreamedCellKey | none |
| extras | bevy::gltf::GltfExtras | none |
| retired | crate::streaming::RetireHidden | none |

#2151.1 owns moving flat_primitive and flat_pose into the hot group and updating
the HotFacts proof. Preserve that move in both the declarative query lists and
the independent cfg(test) disjunction oracles. The probe requires no API change;
admission and row counters derive from the resulting lists automatically. The
merged branch must rerun differential/oracle tests and attribution because the
raw versus semantic invalidation split changes. Current receipts apply to the
15-fact/6-hot input layout above, not the future 13-fact/8-hot merge.
