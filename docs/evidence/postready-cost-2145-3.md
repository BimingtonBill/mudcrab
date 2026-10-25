# Standing CPU round #2145.3

Owner `sol-2145`, branch `phase2/cluster-postready-cost`, worktree
`D:/Mudcrab/worktrees/cluster-postready`, target
`C:/MudcrabBuilds/cluster-postready-target`. Integration `551909a16` was
fast-forwarded before investigation. No push or native ECS dependency link.

## Question and measurement boundary

The prior accepted diagnostic measured render schedule 9.2633 ms, source
queries 0.8120 ms, Transparent3d preparation 0.8437 ms and pending command
submission 3.2691 ms. The target is below 8.33 ms with 7.5 ms preferred.
Those measurements precede integration's additional #2151 changes; a newly
built, frozen baseline is required for this round's paired attribution.

Historical notes read: `analysis-1009-baseline-cpu-breakdown.md`,
`portal-heavy-render-thread-2026-10-08.md`, and
`fun/o3-draw-count-by-cause-2026-10-14.md` under the shared local research root.
They distinguish frame scheduling, GPU pass times and native batch sets, but
their older worlds/settings cannot price this round's complete-content scene.
The newer #2145.2 trace and locked dependency source take precedence.

## Submission is not a geometry-upload measurement

`vendor/bevy_render-0.19.0/src/renderer/render_context.rs` finishes pending
encoders before queue submission. Prior transparent encoder finish is 2.0366
ms, inside parallel finishing; actual queue submission is 1.0683 ms. Do not add
those overlapping values to the enclosing 3.2691 ms system. Driver scopes can
contain waits; these observations do not prove uninterrupted active CPU work.

Native GPU preprocessing uploads CPU work-item lists, binned instance/index
tables, CPU metadata and batch-set records each frame. `RawBufferVec` uploads
each nonempty invocation. `UninitBufferVec::write_buffer`, despite its name,
reserves output storage rather than copying initialized payloads. Mesh uniforms,
visibility and indirect output are also produced on the GPU. Globals contain
time/frame count and must change even in a stationary scene. Sparse mesh input
already skips unchanged dirty pages.

Cluster source-order/batch-table bytes, material slots, cull parameters, terrain
clips and instance masks already have exact input comparisons. Prior settled
logs show zero cluster queue rebuilds, source-order construction and cull-param
uploads. Thus static resident asset uploads should settle, while all queue
traffic cannot correctly be assumed zero. Wgpu's existing `Queue::write_buffer`,
staging-write, texture-write and submit scopes separate API work; they do not
carry payload sizes. Draw counts and allocation counters are not upload-byte
measurements. No exact native byte count is inferred from their names.
The fresh baseline export contains no matching `Queue::` events. Its empty
`postready-queue.json` is missing runtime attribution, not evidence of zero
queue writes. The upload inventory above is source-derived; this build does
not establish per-owner native upload calls or payload bytes.

## Engine-owned candidates and proof

The copied-tick probe currently scans all tables containing its component, even
when the admitted query requires another table component. The bounded candidate
uses public native `WorldQuery::get_state` and `matches_component_set` only when
the exact query filter is dense. Sparse or missing state preserves the broader
probe. Default filters remain deliberately omitted from admission, giving a
superset; the existing native query decides the actual domain. Table IDs and
filter state are not cached across moves, and copied ticks use the exact system
window under the existing read-all World borrow. Only flat GlobalTransform
currently gains structural exclusion; this is not a zero-polling claim.

The gate is declaration-driven. At this base, `SourceFactQueries` contains
`cell`, `scene`, `original_scene`, `flat_primitive`, `flat_pose`, `handle`,
`mesh`, `primitive`, `primitive_mesh`, `primitive_transform`, `static_model`,
`node_pose`, `cell_key`, `extras`, and `retired`. Only `flat_pose` has the
additional `With<FlatSourcePrimitive>` predicate. Moving the flat primitive and
pose declarations into the hot-query group for #2151.1 preserves the same
generic admission mechanism. This round changes neither `HotFacts` pose
comparison nor origin-rebase invalidation and requires no new HotFacts API.

The NifDepth specialization key also contains caster eligibility, although
specialization reads only four raster fields. `NotShadowCaster` is inserted
from retained authored state and native shadow queuing checks that marker.
A separate public raster key can remove the unused specialization dimension
while preserving all authored fields, base-material keys, alpha/depth/decal
semantics and shadow inclusion. This changes the public `AsBindGroup::Data`
associated type; in-repository consumers must be checked. It does not reorder
transparent items or merge through intervening unequal state.

Design reviews approved both bounded mechanisms subject to executable proof.
Red fixtures were prepared outside the engine source while baseline compilation
was active. Dense scope tests count actual copied rows and compare native
queries; sparse/missing metadata stays conservative. Caster tests compare both
extension and composed keys while retaining opposing caster eligibility.

The gated copied-tick standalone proof uses the locked, already built Bevy ECS
library and exact staged helper source. The broad-scan red ran 29 tests:
25 passed and four expected failures, including 1,025 copied rows versus one
required. The filtered green passed all 29. The shim only aliases `bevy::ecs`
to the installed 0.19 library; it does not replace query behavior. This is
helper proof, not engine scheduler integration or a frame-time measurement.

The gated NifDepth standalone proof compiles the exact staged source against
already built locked Bevy/shared libraries. The caster-only key comparison
failed as intended before the change; all 11 NifDepth tests passed afterward.
An initial fixture assertion incorrectly required Debug on Bevy's packed
composed key, and the first direct linker invocation lacked Cargo's native
library search paths. Both setup failures are preserved separately; neither
counts as a semantic red. The corrected run uses the original Cargo build
outputs for native search paths. Engine-wide and device-backed checks remain
separate acceptance steps.

## Rejected shortcuts and limits

Retaining sorted preparation from unchanged mesh/camera flags is unsafe without
complete membership/order, mesh-slot, material-group, allocator/lightmap and
multi-view output-offset invalidation. Native phase buffer contents are cleared
and GPU-mutated each frame. Ordinary wgpu command buffers are single-use. No
recipe, render-bundle or command-buffer cache is introduced by this round.

Palette animation has an unconditional mutable-write path, but the prior trace
does not establish it as the dominant cost: palette asset preparation is about
0.0015 ms/render frame, and standard animation logs play zero materials in this
standing scene. Hidden/equal-animation suppression remains a separate lead,
not a claimed explanation for 3.27 ms of submission.

## Independent ECS assessment

See `ecs-backport-assessment.md`, committed as `d94c5a6af`. The exact two-PR
backport is preserved in owned scratch, not linked. It requires prerequisite
#25429 and macro/asset companions; external component opt-ins and safe tick
escape/rollback cases need separate decisions and proof. No ECS performance
result is claimed. Independent review reproduced the patch and pristine stats.

## Retained run and gate evidence

Baseline source identity is recorded in target-local
`r3-baseline-source-snapshot.json` across 845 Rust/shader/manifest files. The
capture driver records frozen binary/source identity separately from the live
launch worktree diff. All heavy work uses build-gate owner `sol-2145` and the
existing isolated target. Lead's revised capture order is #2151.1, #2153, then this task;
holds are bounded, exclusive and cleared in `finally`. Warm-up is discarded;
draw instrumentation uses a separately labeled capture.

Engine all-target nextest passed 3,359 tests, with 13 skipped. The first workspace
nextest attempt admitted at 04:53:12, before `timing-2151` appeared. It began
compiling converter; the parent stopped only its verified gate PID 4136 and
descendants to release the timing drain. No workspace tests had started.
Cancellation command identity and original nonzero exit/log are retained.
Remaining workspace checks resume after both predecessor windows and this
task's paired captures, without a separate repeat of the already successful
engine-only command. The required workspace suite still includes its engine
tests. This keeps another long compile out of the ordered capture sequence;
all required checks still precede the final source commit.

A second workspace attempt admitted at 06:31:31 in a gap between #2153's
per-capture holds. At the lead's urgent steer, the parent verified and stopped
its own gate PID 19744 and Cargo/rustc descendants, plus both automatic
check/export supervisors and the queued export gate. All slots were then free.
The command identities, log and nonzero exit remain in
`r3-second-workspace-cancelled-for-window.json` and
`r3-workspace-nextest-final.{log,exit}`. No tests had begun. Subsequent admission
requires the lead's explicit `2153 WINDOW COMPLETE` signal, not a hold-clear
observation. The eventual complete retry uses a new receipt stem.
The lead relayed `2153 WINDOW COMPLETE` at approximately 06:58; exports and
`r3-workspace-nextest-complete` resumed afterward. Later timing holds remain
authoritative for new gate admissions; already running jobs finish normally.

The third attempt was stopped at the lead's request at about 08:04 for
`timing-2151`. Gate PID 48416 and its complete verified child tree were recorded
before termination; `r3-third-workspace-cancelled-for-window.json` and
`r3-workspace-nextest-complete.{log,exit}` retain this cancellation. Tests had
not started. There were no Cargo file-lock diagnostics. The log's RAM waits
precede admission at 06:59:14; afterward the single-job build progressed through
engine, launcher, converter and integration-test targets. For example, compiler
38296 had consumed 1,046 CPU seconds at 07:36; distinct integration compilers
started at 07:50, 07:59 and 08:03. The idle Python/Cargo parents do not account
for completed compiler children. Completed artifacts remain cached. The next
retry waits for this specific hold to clear and uses two build jobs for the
remaining workspace targets; test settings and captured binaries are unchanged.
The watcher observed the hold clear at 08:10:41. Gate admission then waited on
other-job staggering and RAM (the log briefly reaches 0.3 GB free). After normal
admission, the cached remainder compiled in 1 minute 11 seconds. The complete
retry passed **5,596 tests, 63 skipped**, across 125 binaries, in **494.298 seconds
of test execution**. Five tests were slow; the final streaming interior fixture
passed at 280.321 seconds. It retains its existing bounded update loop and
headless pacing. No test was edited, excluded or retried after a failure.

## Paired capture facts

The ordered watcher observed both predecessor holds set and cleared, with
`timing-2151` clearing at 05:23:59 and `timing-2153-gpu` at 06:06:26 on
2026-10-26. A later #2151 diagnostic was drained before this task began.
Each binary received its own bounded hold, a discarded 90-second warm-up,
a retained 180-second attribution capture and a separate 90-second draw-count
capture. All six guard verdicts are **quiet**, all reached WorldReady, and this
task's hold was cleared immediately after the final capture. Another owner's
subsequent hold delays exports/checks without invalidating these completed runs.

The frozen baseline executable SHA-256 is
`67262fb4438c6a29d2d3cd27a07d0b4ec5d04f3b767b622b9625499ccf5463c4`;
candidate SHA-256 is
`571921c01fa8d4058921ed62c7f3bcbe54e2f21d99c070f1dc3a6a059b1b313f`.
Both use main28's explicitly selected `clusters-d4ca7334b3e2a58e`, Tamriel
position `(20500,-45500,6000)`, yaw zero, Vulkan RTX 3090, 1600 by 900.
Input manifests and database/cache metadata compare equal. Attribution captures
both contain 112,278 entities, 35,713 mesh entities, 100 terrain patches and ten
water surfaces; separate draw captures both contain 112,279 entities with the
same mesh/terrain/water counts. The one-entity cross-mode difference does not
change those content counts. No cross-mode timing comparison is used.

The final 30 seconds of the separate draw captures contain 2,773 baseline and
3,156 candidate frame samples, with no dropped samples. Observed transparent
API calls are constant within each window: **1,657 to 315**, including
**1,643 to 300 NifDepth calls**. StandardMaterial calls change from 13 to 14;
fixed indexed argument slots change from 5,631 to 5,633. These are command
argument capacities, not GPU-visible object counts. Whole-run native batch-set
peaks are **1,727 to 377** in attribution captures and **1,717 to 377** in
draw captures. Peaks include startup and are not settled per-frame counts.

The counter explicitly reports **incomplete coverage**: other registered draw
variants are unwrapped. Therefore these are observed instrumented command
counts, not an exhaustive stock/all-renderer count. The NifDepth path is
instrumented, but unobserved variants cannot be inferred to issue zero calls.
This limitation is retained rather than suppressing the coverage field.

Settled copied-row polling falls from **1,300,211 to 1,213,859 rows per
extraction** (86,352 fewer; approximately 6.64%). All four native query admission
counts remain zero. The copied probe itself still runs every extraction and
keeps `settled_work.clean=false`; no literal zero-polling result is claimed.

Both retained attribution runs have zero failed cells, asset-load failures and
renderer validation failures. Engine exit 1 is preserved: startup commit-budget
violations are 35 before and 37 after. Existing diagnostic FPS/p95 overrides
make these attribution runs unsuitable as ordinary performance acceptance.
The 6.25 ms project frame-budget bar also remains independently visible.
Guard other-process CPU means are 4.2% before and 4.0% after, with peaks 13.0%
and 13.3%. A single paired observation supports causal inference when backed by
the reduced work counts; it does not isolate every timing delta from drift.
Target-local `r3-run-facts.json` preserves the exact window/count facts.

Raw-log audit retains the same 48 tracing warnings in each attribution run:
initial source recovery, unresolved enable-parent records, missing glTF tangent
inputs and temporary distant-source database queue pressure during startup.
Each log also starts with Tracy's `SymInitialize` error 87; named span exports
remain available, but no native-symbol/call-stack proof is inferred. Neither log
contains a nonzero `owned_not_drawable` warning. These observations supplement
the grader and do not establish clean pixels or portal-heavy visual acceptance.

## Paired post-ready CPU/GPU attribution

The last complete 30-second Tracy windows contain 2,322 baseline and 3,041
candidate render schedules, both well after WorldReady. These are guarded
diagnostic observations with profiler overhead, not profiler-free FPS results.

| System or span | Before ms/frame | After ms/frame | Delta ms/frame |
| --- | ---: | ---: | ---: |
| Render schedule, inclusive | 10.7884 | 8.0623 | -2.7261 |
| Approximate companion render-thread cycle | 10.7519 | 8.0312 | -2.7207 |
| Submit pending command buffers, inclusive | 3.6716 | 2.7735 | -0.8981 |
| Transparent encoder finish, nested/parallel | 2.2063 | 1.2475 | -0.9587 |
| Actual queue-submit span, nested | 1.2629 | 0.9778 | -0.2850 |
| Main transparent pass CPU recording | 1.2736 | 0.2717 | -1.0019 |
| Transparent3d batch preparation | 0.9999 | 0.8484 | -0.1515 |
| Extract cluster sources | 1.3351 | 1.1056 | -0.2295 |
| Source change queries, nested in extraction | 0.9615 | 0.7674 | -0.1940 |
| GPU pass sum | 7.6422 | 7.6523 | +0.0101 |

Nested/parallel finish and submit spans must not be added to their enclosing
submission system. Source extraction belongs to extraction, not the render
schedule: its saving cannot be subtracted again from render-schedule time.
The older 9.2633 ms baseline is not substituted for the fresh 10.7884 ms run.

The work reduction establishes the raster-key mechanism: fewer instrumented
NifDepth commands and fewer batch sets accompany less transparent recording,
preparation and finishing. This pair does not price the two source changes
independently. Unchanged control systems also improve: CPU visibility culling
0.4759 to 0.4256 ms, directional visibility 0.3465 to 0.3264 ms, GPU visibility
culling 0.2534 to 0.2278 ms, and benchmark collection 0.2788 to 0.2451 ms.
Camera extraction changes 0.2853 to 0.1843 ms despite no camera-writer edit in
this round. Thus some broad CPU drift remains; the entire 2.7261 ms reduction,
and especially the 0.1940 ms query-zone reduction for 6.64% fewer copied rows,
cannot be assigned entirely to the patch. Causal timing claims remain inference.

The candidate diagnostic render schedule is 0.2677 ms below 8.33 ms, but misses
the preferred 7.5 ms by 0.5623 ms. This is not a normal 120/140 FPS acceptance.
The GPU pass sum is effectively unchanged and already exceeds 7.5 ms, leaving
about 0.68 ms under 8.33 ms. CPU rendering still measures slightly longer, so
these traces do not prove a fully GPU-bound frame. GPU work is a near-budget
constraint after CPU reductions; pass sums are not matched whole-frame GPU
wall times and cannot be added to CPU duration. Portal-heavy and profiler-free
acceptance remain separate work before any Plus merge.

The prescribed `DS-VERDICT/verdict.py` ran on all six folders; candidate folders
use their corresponding baseline as reference. Every result is **CONTENT PASS,
PERF FAIL, TRAVERSAL N/A, OVERALL FAIL**, exit 1. Retained attribution post-ready
FPS is 77.94 before and 101.66 after, with p99 16.86 and 13.24 ms. Both have
zero post-ready hitches over 33/50/100 ms. These remain Tracy-run observations,
not shipping FPS acceptance. Separate draw-counter runs report 92.46 and
103.93 FPS and must not replace the attribution pair.

The grader confirms equal 29,116 transform instances, 25 resident cells,
47,717 refused distant references (25,656 look, 22,061 runtime), and zero
undrawn-static/runtime-refused counters. It prints unknown dataset labels;
the independently compared manifest hashes and DB/cache metadata supply that
input identity check. Normalized fingerprints also compare equal. Retained
`r3-comparison.{json,md}`, per-run tail/detail/queue exports, guard receipts and
`grader.{log,exit}` preserve the observations and their limits.

## Final verification and handoff

All retained Rust checks ran through the shared build gate with owner
`sol-2145` and the same isolated target:

- `cargo fmt --all -- --check`: exit zero.
- Engine and workspace `cargo clippy --all-targets --all-features -- -D warnings`:
  exit zero; workspace execution took 7 minutes 56 seconds.
- `cargo nextest run -p engine --all-targets`: **3,359 passed, 13 skipped**,
  107.609 seconds of test execution.
- Final `cargo nextest run --workspace`: **5,596 passed, 63 skipped**,
  494.298 seconds, with the earlier timing-window cancellations preserved.
- `cargo test --workspace --doc`: **18 passed, 15 ignored**, zero failures.
- Focused authored-depth binding fixture: **one passed, zero skipped**. Its
  retained success stderr identifies **NVIDIA GeForce RTX 3090, Vulkan**, and
  `base bindless supported: true`. Both native and forced-non-bindless layout
  paths execute. Its small 18.49-second build completed during a cache-context
  inspection; no extra cancellation or rerun was needed.

The existing `proc-macro-error2 v2.0.1` future-incompatibility notice remains;
strict Clippy still exits zero. Frozen implementation review found no
actionable source defect. All **845 source/shader/manifest hashes** match the
verified build/check receipt byte for byte, including after the final tests.
An initial audit mistakenly normalized CRLF against raw-byte receipts; that
failed audit is preserved separately and the exact-byte audit has no mismatches.
`r3-postcheck-source-verification.json` retains all final exit codes, counts,
source identity and physical-adapter facts.

`AsBindGroup::Data` for `NifDepthExtension` changes publicly to
`NifDepthRasterKey`; consumers needing caster eligibility must read retained
`extension.state`. The full authored-state conversion remains available. No
matching in-repository consumer required a migration; the commit includes an
explicit breaking-change note.

Remaining limits: literal zero settled row polling, exact native upload bytes,
profiler-free 120/140 FPS acceptance, the preferred 7.5 ms CPU target, live Metal
execution and portal-heavy visual/GPU acceptance. The ECS backport stays
unlinked and uncompiled. No foreign-owner folder was removed; task-owned
artifacts remain active evidence, with no expired folder deleted. No push,
PR, integration merge, runtime cap or content/settings reduction was performed.
