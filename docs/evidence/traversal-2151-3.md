# I22 traversal cost and frame-band investigation (#2151.3)

This round starts at integration `94d8722a0` in
`D:/Mudcrab/worktrees/cluster-traversal`, branch `phase2/cluster-traversal`.
The isolated target and retained evidence are
`D:/Mudcrab/builds/cluster-traversal-target`; gate owner is `sol-2151`.
No push is authorized. The acceptance target is the user's 120 FPS traversal
target, rather than the scorer's older 160 FPS target.

**Final result: the traversal bar remains open.** Profiler-free fly records
105.3064 FPS, p99 21.4631 ms and 41 frames above 33 ms. Fast records 92.3401
FPS and 21 frames above 50 ms. All code gates pass. All 84 final diagnostic
Tracy hitches have exact frame joins and explicit root plans; coarse plain
ownership and uncovered intervals remain honestly limited below.

## Profiler-free integration baseline

The supplied I22 runs retain the same authored routes, content and rendering
settings. The independent audit checked all 36 input hashes. Its complete
frame ledgers and input receipts are in `round3-i22-audit` under the target.

| I22 run | Mean of frames at or below 33 ms | Frames >33 ms | Frames >50 ms |
| --- | ---: | ---: | ---: |
| Standing 1 | 8.1508 ms | 0 | 0 |
| Standing 2 | 8.2732 ms | 0 | 0 |
| Fly 1 | 9.2004 ms | 39 | 27 |
| Fly 2 | 9.1984 ms | 36 | 30 |
| Fast | 10.2285 ms | 38 | 30 |

The fly spikes add only 0.1132 ms to the overall mean. The ordinary frame band
therefore needs investigation independently of crossings. Standing and moving
routes change the view and visible workload; their difference alone does not
isolate the causal cost of movement. Fly 2 also has an unstable one-second
window at 25.1208–26.1237 seconds. All I22 windows start after startup cover
release, retain complete content, and have zero frames above 100 ms.

I22 contains no exact nested per-frame spans. Labels from the previous N30
round cannot establish the causes of individual I22 spikes.

## Counter-only diagnostic baseline

The baseline executable is `round3-before-tracy.exe`, SHA-256
`2677ca3d9f1139d23689f41727284f558214911d736ad7e06391e2751c6dfcd7`.
Strict engine all-target/all-feature Clippy and the release Tracy build passed
before freezing it. Source hashes, binary receipt and exact patch are retained
as `round3-baseline-source-hashes.json`, `round3-baseline-binary.json`, and
`round3-baseline.patch`. No behavior fix preceded this baseline.

The instrumentation separates selection material evaluation, readiness,
indexing, base construction/update, mask copying, permanent-refusal filtering,
layer filtering and upload. Readiness counts aggregate batch visits and its
source/layout inputs. Flat synchronization counts admission reasons, members,
ordered pose computations and actual global/proof writes.

Accepted diagnostic captures are:

- `round3-before-retry3-tracy-standing`: 4.1% foreign CPU, quiet guard.
- `round3-before-retry3-tracy-fly-fast`: 4.1% foreign CPU, quiet guard.
- `round3-before-retry3-tracy-fly`: 5.5% foreign CPU, quiet guard.

All have matching I22 content references, successful nonempty recorder output,
and common `Time<Real>` cover/window/route admission proof. Both moving routes
completed. The standing run is 120 seconds, versus I22's 240 seconds; it is a
shorter diagnostic comparison. Tracy runs do not establish normal FPS.

The campaign finally cleared its hold before export. The exact
`BASELINE COMPLETE` relay was sent to the lead. A bounded observer recorded
`timing-2153-gpu` SET at 11:52:21 Adelaide before any fix Cargo job could start.
Exports yield when another owner's timing hold is requested and resume through
the normal gate after clearance.

### Rejected evidence and guard ownership

The original discarded fast warm-up completed its route but reported 11.4%
foreign CPU. A first retry expired while draining, without captures. A second
retry's standing guard reported 6.5%; that run remains rejected.

The required Tracy recorder initially ran outside the guarded process tree.
The guard consequently counted its work as foreign CPU. The corrected wrapper
owns both engine and recorder beneath the guarded command. Per-run
`tracy-owned-processes.json` records all three process IDs, exits and trace
size. The foreign CPU threshold remains 6%, and foreign app/build detection is
unchanged. No rejected guard was modified or retrospectively accepted.

### Clock and schedule ownership

The join requires complete native cycle/frame series, unique duration-sequence
alignment, exact admitted counts and percentile/threshold agreement, and
native post-ready/log boundary agreement. Every admitted native delta must fit
between the recorded neighboring `send_time` span endpoints.

Profiled frame numbering advances in `First`; ordinary numbering advances in
the `Last` sampler. The plain CSV/log offset formula is therefore retained
only for plain runs. Profiled standing still requires complete ordered
CSV/native-tail equality and all native clock checks. Its first admitted CSV
sample is 3,249, one earlier than the inapplicable plain-clock formula.

`send_time` is scheduled in RenderRecovery and can execute on a worker.
Ownership is established by unique, nonoverlapping RenderApp root containment
and one send per cycle, rather than equality with the enclosing root's thread.
Known named stages are assigned using actual schedule registration before
thread containment. Unresolved spans remain explicit; unresolved named stages
reject the report instead of being represented as zero work. An independent
bounded source review found no actionable defect in these corrections.

The accepted standing join contains 9,272 admitted frames and zero frames above
33 ms. Its mean is 9.6265 ms and p99 is 11.4345 ms. These are diagnostic timings,
not profiler-free acceptance. The full output is under `round3-attribution`.

## Baseline attribution

The first complete analysis exits zero in `round3-attribution-5.exit`.
Every admitted frame passes its exact native/send-time bounds. Fly has 12,851
admitted frames, including 12,621 ordinary moving frames; fast has 2,580,
including 2,348 ordinary moving frames. The ordinary population excludes
crossings and their render-fed neighbors, every >33 ms frame, and two cycles on
either side. All exclusions remain in the population ledgers.

### Ordinary moving frame cost

These are means over exact native cycles. Nested and parallel rows are
**nonadditive**, and the camera/content workload differs between standing and
moving. They locate work; they do not establish isolated causal effects or
profiler-free FPS gains.

| Stage | Standing | Fly | Fast |
| --- | ---: | ---: | ---: |
| Main world | 4.0231 ms | 4.2806 ms | 4.4239 ms |
| Extract | 1.5889 ms | 1.6948 ms | 1.7585 ms |
| Render thread | 7.9453 ms | 9.1627 ms | 9.4284 ms |
| Render prepare | 2.6053 ms | 3.0117 ms | 3.3704 ms |
| Render queue | 0.4347 ms | 0.5918 ms | 0.7508 ms |
| Graph and present | 4.2213 ms | 4.8536 ms | 4.5968 ms |
| Graph views | 1.1252 ms | 1.6714 ms | 1.6441 ms |
| Graph submit | 2.7693 ms | 2.8430 ms | 2.6238 ms |
| Source change queries | 0.7565 ms | 0.8059 ms | 0.7983 ms |
| Flat synchronization | 0.0305 ms | 0.0298 ms | 0.0293 ms |
| Cluster selection | 0.0117 ms | 0.0474 ms | 0.1845 ms |
| Cluster readiness aggregate | no admitted calls | 0.0327 ms | 0.1683 ms |

The fly render-thread difference is 1.2174 ms. Its prepare difference is
0.4064 ms and graph/present difference is 0.6323 ms; source query difference is
only 0.0494 ms, with no extra ordinary flat-sync cost. A crossing-only fix
cannot be credited with eliminating this steady gap. Native worker stage
ownership was subsequently verified from source registration; the refinement
below separates native LOD evaluation from mesh collection and other work.

### Every baseline frame above 33 ms

All **99/99** frames join exactly. Complete intervals, spans, critical
intersections and unresolved observations remain in the two
`*-threshold-frames.json` ledgers. `*-classified-frames.csv` labels each frame.
Crossing flags take precedence; other labels identify the largest recorded
stage among selection, aggregate, native material and native asset work.
These are descriptive primary labels, not an exclusive causal partition.

| Primary label | Fly count | Fly frame sum | Fast count | Fast frame sum |
| --- | ---: | ---: | ---: | ---: |
| Crossing, multiple stages | 34 | 2,088.4020 ms | 34 | 2,041.6467 ms |
| Readiness aggregate | 9 | 344.8044 ms | 11 | 382.0888 ms |
| Selection and batch readiness | 3 | 107.0187 ms | 3 | 106.4749 ms |
| Native material specialization/queue | 2 | 79.0011 ms | 2 | 95.9707 ms |
| Native asset extraction/preparation | 1 | 38.6334 ms | 0 | 0 ms |
| Total | 49 | 2,657.8596 ms | 50 | 2,626.1811 ms |

Frame sums include all work in those frames; they are not milliseconds
exclusively caused by the primary label. Every overlapping stage is retained.

Crossing critical intersections are likewise nonadditive:

| Crossing stage | Fly range | Fast range |
| --- | ---: | ---: |
| Source change queries | 9.0803–13.3521 ms | 9.3217–12.3845 ms |
| Flat synchronization | 4.0997–8.4158 ms | 3.8360–4.6605 ms |
| Selection total | 5.6195–13.6741 ms | 5.4851–12.5630 ms |
| Full selection-base construction | 3.4452–5.4239 ms | 3.4071–5.0731 ms |
| Readiness aggregate, when invoked | 3.4053–14.0317 ms | 3.6357–11.4712 ms |

Permanent-refusal filtering has **zero** admitted invocations. Its proposed
index optimization and draft regression were discarded before implementation;
the cancelled queued RED attempt is retained in target-local receipts. This
round does not claim a measured cost or saving for that path.

The retained red candidates address actual mechanisms: owned grid movement
advances an ineffective suppression epoch and forces full bases; fresh equal
readiness vectors allocate identities that invalidate consumers; aggregation
re-reads identical per-batch facts once per applicable consumer. Required
source/material/view/role checks remain authoritative. The baseline flat
counters show pose-only rebases, so policy-only flat optimization is excluded.

### Native worker attribution refinement

Source registration assigns the GPU mesh collector, asset preparation, phase
batching/buffer writes and native LOD evaluation to Render. Previous-transform
maintenance and material-specialization candidate collection run in Main.
The mesh extractor is a distinct ExtractSchedule producer. Its collector's
name does not imply GPU execution time. The independent audit retains the
exact registration citations and full producer/consumer mechanism.

The expanded report is `round3-attribution-7`, exit zero. Ordinary GPU mesh
collector means are only 0.0139/0.0265/0.0358 ms standing/fly/fast, despite
large collector work in crossing frames. Native LOD evaluation instead adds
0.2878 ms fly and 0.4560 ms fast. Those ordinary system observations have a
30-microsecond export floor; absent tiny invocations are censored, not proof of
literal zero work. Named cluster scopes retain no duration floor.

With these source-verified mappings, residual critical time across all
threshold frames totals 314.5653 ms fly and 302.2802 ms fast. Largest individual
uncovered intervals are 1.8775 and 1.5063 ms. All intervals and ambiguous spans
remain explicit. Frame labels identify measured mechanisms, not exhaustive
exclusive causation.

Native LOD already publishes `Inputs.roles`, the exact union of near, native
and object maps; its filter uses that inventory. Evaluation nevertheless reads
up to five source-map entries to decide membership and role for each visible
mesh. The retained design reuses that producer inventory and preserves every
queue, material, prepass and drawability predicate. It does not skip near draw
proof when native far geometry is absent.

## Regression and diagnostic proof

The initial three regressions compiled and failed before behavior changes:
owned-window revision 28 versus 27; two batch-fact preparations versus one;
and fresh equal readiness losing its allocation identity. In `round3-red-3`,
all three pass. Strict engine all-target/all-feature Clippy also passes.
Independent review found no actionable finding in those three fixes.

Readiness vectors are still freshly evaluated on every cache miss. Equal bits
retain identity only within the cache's GPU-owned view and matching generation,
layout, batch count, shadow and prepass roles. Replacement currently creates
fresh GPU views. A future transfer of view caches between GPU identities must
reset them or make GPU identity explicit; that is an integration boundary, not
permission to reuse arbitrary old proof.

The profiler-free benchmark already retains a scored native budget ledger.
Its new export writes `frames.budget.csv` only after timing has finished, beside
the unchanged virtual-clock `frames.csv`. It enables no profile-output mode or
new per-frame collection. Both filesystem regressions first failed because
the sidecar was absent. The new output preserves its independent order/count,
real duration, existing coarse owner and optional main-thread wait.

Plain analysis must find a unique complete duration-sequence match before
joining those rows. Tiny checks reject mismatched and ambiguous sequences.
The sidecar supplies the existing `FrameBudget` rule, not nested Tracy stages
or a GPU execution timestamp. `Unknown` remains unknown; no crossing label is
inferred from frame duration. Missing broader post-ready threshold frames
reject the plain proof instead of disappearing from its ledger.

## Retained implementation scope

Four changes remain after baseline attribution and observed REDs:

1. `streaming/native_lod.rs::FarNativePolicy::update` retains every current
   window, including absence, but advances the suppression revision only when
   ownership changes or an unowned window changes. Owned crossings therefore
   preserve the effective policy. Ownership removal uses the latest window.
2. `cluster/handoff.rs::retain_equal_readiness` compares freshly evaluated
   readiness with the previous vector. Equal bits retain their Arc within the
   same GPU-owned view, generation, layout, batch count and draw roles. Every
   miss still evaluates proof and publishes the new cache key.
3. `cluster/readiness_bridge.rs::Producer::observe_with` prepares immutable
   batch facts once per observation. Consumer world, layer/domain, readiness
   and surviving-group invalidity remain independent. No cross-frame fact
   cache was added.
4. `streaming/native_lod.rs::draw_role` reads the existing coherent merged
   role inventory once per visible entity. Block-before-native-before-near
   precedence, queue membership, StockDrawable, prepass and statistics remain
   unchanged. The observed RED records 2–5 source-map reads per entity versus
   the required one merged lookup; the independent priority oracle covers
   every overlap and an unrelated entity.

`streaming/flat_stock.rs` contains counter/span instrumentation only. Its
ordered transform fold remains unchanged. There is no portal, source-query
gate, GPU reuse, loader/package or ClusterScene contract hunk this round.
The metrics change exports already-retained samples at shutdown only.

Final-source fmt and strict engine all-target/all-feature Clippy passed.
The final focused suite passed all 58 selected tests, including policy,
equal-readiness, merged-role, batch-fact and filesystem regressions. Broader
checks, both release builds and final native measurements are separate gates;
this focused result does not establish traversal acceptance.

### Frozen-source verification

`round3_checks.py` verifies all 255 frozen engine Rust/Cargo file hashes before
and after every job. The retained `round3-final-*.log/.exit` receipts record:

| Gate | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| Engine strict all-target/all-feature Clippy | PASS |
| Focused nextest | 58 passed |
| Engine all-target nextest | 3,424 passed, 15 skipped |
| Workspace strict all-target/all-feature Clippy | PASS |
| Workspace nextest | 5,681 passed, 65 skipped |
| Workspace doctests | 18 passed, 15 ignored |

Every listed gate exited zero. The known dependency future-compatibility notice
remains; no warning denial was relaxed. The workspace interior-streaming
fixture passed in 306.831 seconds under shared verification load.

Final Tracy release build exited zero after 28m29s. Its frozen executable
`round3-after-tracy.exe` has SHA-256
`5b4d439f64872c1fad7a1b1f240c3b5051261e7cb33b782d82f528ab9a15e8ff`;
`round3-final-tracy-binary.json` records features, command and source receipt.
The cache server exited during compilation and Cargo compiled locally; this
changes build latency, not the requirement for a discarded runtime warm-up.

Final plain release build exited zero after 27m27s. Its frozen executable
`round3-after-plain.exe` has SHA-256
`02e0b28666daee749cd00d9a41a25b76512acc8a2d5cf4d5d8e954c79b3fce2d`.
`round3-final-plain-binary.json` records the feature-free engine build command
and the same source receipt. The campaign supplies no `--profile-output` to
this executable. All checks and both builds completed before capture arming.

### Rejected final warm-up

The first final reservation armed at approximately 15:51 and finally cleared
at 15:52:49. It ran only `round3-after-tracy-warmup-fly-fast`; no scored run
started. The unchanged guard rejected 7.5% mean foreign CPU against its 6%
limit, with a 23.7% peak and no recognized build intruders. The initial ambient
snapshot was only 1.8%, so that snapshot did not establish sustained quiet.
Busy-process peaks do not prove which app caused the mean disturbance.

The rejected warm-up, successful recorder and its 108,794,054-byte trace remain
retained; none supplies accepted timings. Both binary/source receipts remain
unchanged. The bounded retry uses the unchanged capture implementation with a
new prefix. An extra preflight was removed: `guard` correctly refused to run
without a hold, and an additional contiguous-minute probe duplicated its own
quiet admission. Those unarmed diagnostics are retained separately. Every
actual run retains the original quiet admission, full-run guard and threshold.
Each retry reservation remains at most 25 minutes and finally-clears.

## Root-layer plans for residual mechanisms

The final ledgers must determine which of these measured baseline mechanisms
remain. A stage label establishes recorded work in the frame, not its exclusive
causal share. The plans preserve complete content and immediate invalidation:

| Mechanism | Next bounded change and proof |
| --- | --- |
| Crossing source observation | Coordinate with #2145.4's native query work. Separate admission probes, semantic comparisons and dependency reconstruction before selecting a producer delta. Keep arbitrary in-place ECS writes, missed rebases, structural recovery and full-oracle equivalence authoritative. |
| Crossing flat pose propagation | Reuse producer-owned validated pose inputs while preserving the exact ordered transform fold. First prove bounded chain work across positive/negative rebases with shear, nonuniform scale and simultaneous authored edits. Do not substitute a differently associated matrix product or accumulate displacement drift. |
| Remaining readiness aggregate | Project exact templates, fallback membership, shadow policy and consumer readiness dependencies rather than broad source/material epochs. Update affected batch/world/domain contributions, retain independent invalid-survivor OR semantics, and reject stale certificate/layout/admission snapshots. Test fallback-only transitions and absent-resource recovery. |
| Remaining selection reconstruction | Separate actual selection dependencies from broad source notifications; retain readiness proof and exact native suppression policy. Validate one-reference/template/refusal changes against the full oracle and prove unchanged unrelated world/batch membership. |
| Native material and asset tails | Join affected material/mesh/task IDs, allocator events, queue membership and upload bytes to the recorded frame before pre-staging work off-thread. Publish only generation-matched prepared results; retain raw mutable material access, address relocation, removal/reinsertion and pipeline/queued-proof withdrawal checks. |
| Ordinary moving Render prepare/queue/graph | Compare matching per-view work and upload/submission IDs. The baseline CPU phase means overlap, and GPU pass means lack frame joins. Retain dynamic camera/culling correctness; coordinate GPU work with #2153.1. No frozen view, dispatch throttle or asset cap is an admissible fix. |
| Uncovered intervals or coarse plain ownership | Retain every interval and frame explicitly. Add a coarse enclosing phase/task/submission timestamp only where the existing ledger cannot distinguish causes. A plain FrameBudget owner is not a nested-stage or matched GPU attribution. |

No residual plan authorizes reduced settings, delayed content, loading cover
during movement or replacement of owned geometry with stock stand-ins.

## Accepted final campaign and profiler-free result

Accepted prefix: `round3-after-retry2-{tracy,plain}-{standing,fly-fast,fly}`.
One reservation covered all six scored captures, approximately 16:15–16:32
Adelaide, with hard deadline 16:40. It finally-cleared, campaign exit was zero,
and exact `FINAL COMPLETE` was relayed immediately. No build or export ran
inside the hold. Trace export and validation ran through the gate afterward.

The preceding retry's discarded Tracy warm-up also failed its unchanged guard:
6.1% foreign CPU, no recognized build intruders. Its hold finally-cleared at
16:10. The accepted campaign reused that completed, matching-binary run only
as discarded warm-up, using the already established warm-up reuse checks.
Its timings remain rejected. Plain received its own discarded warm-up.
Neither rejected guard was modified. The capture helper's sole subsequent
change generalizes existing warm-up reuse to the selected binary kind.

| Accepted run | Foreign CPU mean | CONTENT/reference | Route |
| --- | ---: | --- | --- |
| Tracy standing | 4.1% | PASS | N/A |
| Tracy fly | 3.8% | PASS | complete |
| Tracy fast | 3.5% | PASS | complete |
| Plain standing | 3.9% | PASS | N/A |
| Plain fly | 3.7% | PASS | complete |
| Plain fast | 4.7% | PASS | complete |

`round3-final-audit.json` and `round3-final-input-sha256.csv` retain the six
accepted inputs and all 36 essential file hashes. Graphics, renderer, resolution,
database schemas and cell-cache fingerprints match the corresponding I22
reference exactly, excluding commit/dirty-worktree metadata. Both binary
hashes match their frozen receipts. Standing is configured for 120 seconds
versus I22's 240 seconds; the table uses each actual post-ready tail. Routes
use the complete measured route CSV. One final run per workload establishes an
observation, not repeatability or an isolated causal FPS effect.

| Profiler-free workload | I22 FPS | Final FPS | I22 p99 | Final p99 | I22 >33 ms | Final >33 ms | I22 >50 ms | Final >50 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Standing | 122.69 / 120.87 | 118.47 | 9.3800 / 9.7406 | 10.0996 | 0 / 0 | 0 | 0 / 0 | 0 |
| Fly | 107.37 / 107.39 | 105.31 | 22.9011 / 21.5560 | 21.4631 | 39 / 36 | 41 | 27 / 30 | 19 |
| Fast | 92.39 | 92.34 | 51.8427 | 45.1165 | 38 | 38 | 30 | 21 |

All admitted final windows have zero frames above 100 ms. Final one-second
window checks report zero violations: fly's worst p99 is 42.46 ms, **1.98×**
the median window p99; fast is 57.06 ms, 1.31×. Fly has little margin to the
2× boundary. The scorer's `TRAVERSAL: PASS` does not satisfy the user's
additional zero-hitch and absolute p99/FPS requirements. CONTENT passes;
PERF and OVERALL remain failed. Native exits remain 1 because the existing
whole-run startup commit-budget predicate is false, the only false engine
threshold boolean in all six runs. No predicate was relaxed.

The steady band remains the main FPS problem. Plain means at or below 33 ms
are standing **8.4413**, fly **9.3920**, fast **10.3263 ms**. Fly's steady
penalty relative to this standing run is 0.9507 ms; rare hitches add another
0.1041 ms to its full mean. I22's corresponding steady fly penalty was roughly
0.93–1.05 ms. This round does not establish a profiler-free FPS improvement.

### Cover and content validity

All values below share `Bevy Time<Real>.elapsed`. Negative gaps mean the cover
released before the admitted event. No moving frame receives hidden cover.

| Run | Cover minus route | Cover minus window |
| --- | ---: | ---: |
| Tracy standing | N/A | -18.3584 ms |
| Tracy fly | -35.2479 ms | -24.0113 ms |
| Tracy fast | -33.0227 ms | -21.5951 ms |
| Plain standing | N/A | -17.8726 ms |
| Plain fly | -29.4852 ms | -19.7677 ms |
| Plain fast | -33.6097 ms | -22.0573 ms |

Both modes retain 35,497 route mesh entities and 136 terrain patches; standing
retains 35,713 and 100. Plain route entity count is 115,656, standing 112,274;
Tracy has four additional diagnostic entities. All six have zero timestamped
WARN/ERROR entries after cover release. The same nonfatal, untimestamped
`SymInitialize` code 87 occurs at line 1 of Tracy logs, before cover release;
it is retained rather than misclassified as a late runtime error. Endpoint and
scorer content checks do not prove complete pixels throughout a route. The
pre-existing `--allow-incomplete-assets` launch setting is unchanged from I22.

## Measured work changes and remaining cost

These are one paired diagnostic observation, so wall-time effects are
**inference**. Deterministic tests establish the invalidation/work contracts.

| Diagnostic work | Before | After |
| --- | ---: | ---: |
| Fly crossing full-base construction | 34 recorded critical calls, 3.4452–5.4239 ms | no admitted calls |
| Fast crossing full-base construction | 34 recorded critical calls, 3.4071–5.0731 ms | no admitted calls |
| Fly crossing selection total | 5.6195–13.6741 ms | 0.0091–6.8029 ms |
| Fast crossing selection total | 5.4851–12.5630 ms | 0.0102–7.5564 ms |
| Fly crossing aggregate, when invoked | 3.4053–14.0317 ms | 2.6946–4.3754 ms |
| Fast crossing aggregate, when invoked | 3.6357–11.4712 ms | 2.7651–4.7804 ms |
| Fly native LOD ordinary mean | 0.2878 ms | 0.2825 ms |
| Fast native LOD ordinary mean | 0.4560 ms | 0.4645 ms |

Role consolidation therefore has **no established wall-time benefit** in this
pair. It reuses the authoritative merged inventory and proves one lookup per
visible entity; it does not remove draw proof. Do not credit it with eliminating
native LOD's remaining roughly 0.28/0.46 ms ordinary cost.

All 915 fly / 905 fast readiness misses still freshly evaluate their vectors:
12,279,300 / 12,145,100 batch evaluations, unchanged from baseline. Each final
vector retained equal identity, preventing pointer-only downstream invalidation.
Aggregation still performs 199 / 198 non-replays. Final aggregate visits are
6,794,855 / 6,760,710 and actual unique fact preparations 1,358,971 / 1,352,142.
The visit counter precedes domain rejection, so it is not a general formula for
avoided callbacks. The observed ratio here is five visits per preparation.
Broad material/source invalidation and full fresh readiness evaluation remain.

| Ordinary native CPU phase, nonadditive | Before standing / fly / fast | After standing / fly / fast |
| --- | --- | --- |
| Render thread | 7.9453 / 9.1627 / 9.4284 ms | 7.8642 / 8.7658 / 9.3937 ms |
| Prepare | 2.6053 / 3.0117 / 3.3704 ms | 2.5547 / 2.9068 / 3.3045 ms |
| Queue | 0.4347 / 0.5918 / 0.7508 ms | 0.4234 / 0.5680 / 0.7694 ms |
| Graph/present | 4.2213 / 4.8536 / 4.5968 ms | 4.2073 / 4.6171 / 4.6104 ms |
| Source queries | 0.7565 / 0.8059 / 0.7983 ms | 0.7351 / 0.7610 / 0.7754 ms |
| Flat sync | 0.0305 / 0.0298 / 0.0293 ms | 0.0302 / 0.0288 / 0.0293 ms |

The final ordinary fly render gap is still 0.9016 ms; fast is 1.5295 ms.
Ordinary source/flat work does not explain most of that movement penalty.
Render prepare, per-view graph work, queue/proof work and waits need exact
consumer/task/submission dependencies. Native phase means overlap; CPU graph
wall time is not matched GPU execution. No moving-view or GPU work was frozen.

## Every remaining threshold frame and root plan

Exact final joins cover **42/42 fly and 42/42 fast** Tracy frames above 33 ms,
including broader post-ready admission. All admitted durations satisfy their
recorded send-time bounds. Standing has zero hard frames. Native admitted
counts are 9,718 standing, 13,463 fly and 2,631 fast. The corresponding ordinary
moving cohorts contain 13,239 and 2,409 frames under the same exclusion rule.

The refined primary label ranks the largest single recorded scope among the
listed mechanisms, with crossing flags taking precedence. It now includes
scene instantiation and mesh collection and is applied to both sides. Counts
describe primary observations, not exclusive causes or summed stage costs.

| Primary observed mechanism | Before fly / fast | After fly / fast |
| --- | ---: | ---: |
| Crossing, multiple stages | 34 / 34 | 34 / 34 |
| Readiness aggregate | 9 / 10 | 0 / 0 |
| Selection/readiness | 3 / 3 | 4 / 4 |
| Native mesh preparation | 0 / 2 | 0 / 1 |
| World scene instantiation | 0 / 0 | 1 / 1 |
| Native material specialization/queue | 2 / 1 | 2 / 1 |
| Native asset extraction/preparation | 1 / 0 | 1 / 1 |
| Total | 49 / 50 | 42 / 42 |

Aggregate work remains in many frames even though it is no longer the primary
label. All 68 crossing hitches remain. Final source-query critical intersections
are 9.6671–15.9284 ms fly and 9.2599–13.3081 ms fast; flat sync is 3.9386–5.5076
and 3.9327–5.4169 ms. These overlap other pipeline work and must not be added.
Outside crossings, scene instantiation reaches 7.370/7.701 ms, mesh collection
5.603 ms, material specialization 6.987/6.699 ms, and image asset extraction
6.082/7.138 ms in the indicated frames. These are recorded system scopes,
not established inner algorithms; each receives the bounded plan below.

`round3-attribution/round3-after-retry2-tracy-frame-plans.csv` gives every final
cycle, duration, observed mechanisms, root plan, exact interval-ledger pointer
and uncovered-gap plan. All 84 rows are present and unique. The before plan
ledger similarly contains all 99 rows. The existing full JSON/CSV interval
ledgers retain every overlapping span and unresolved observation.

Uncovered critical time totals 270.3246 ms fly / 274.1711 ms fast across the
threshold frames. Largest individual gaps are 1.6294 / 1.8209 ms. Multiple gaps
can occur in one frame; this is not proof that every frame has less than 2 ms
unexplained. Add phase/task/submission IDs for those intervals before assigning
them to CPU scheduling or GPU execution. Generic sub-30-microsecond spans are
censored; named cluster stages retain no duration floor.

Plain sidecars pass unique complete duration-sequence joins, including broader
post-ready threshold-count checks. Their 79-row plan ledger retains all
41 fly / 38 fast hard frames. Existing coarse owners are 34 Unknown + 7
RenderThread fly and 34 Unknown + 4 RenderThread fast. **No Tracy cycle,
crossing or nested stage is transferred to those plain rows.** Unknown is a
real evidence limit. The concrete next proof is to terminal-export already
sampled FrameSides plus native cycle IDs and join the missing phases before
claiming inner attribution. RenderThread also includes submit/present waits.

N30 remains open for exact source/flat propagation, affected-batch proof,
scene/asset pre-staging and native per-view preparation. The per-frame ledgers
attach those plans individually. This round takes no cap, throttle, reduced
setting, frozen view, hidden loading or new stock stand-in shortcut.

## Final visual and integration limits

One bounded unmeasured plain TEST visual ran after timing clearance. Two
post-cover scene captures are retained in `round3-after-visual`; the route
passes foliage and rock at those poses. No Windows Security dialog is visible
in those window captures. **No Cancel/dismissal action was performed.**
Window capture does not prove desktop modal absence or full-route pixel/pop-in
correctness. The owned visual process was stopped; its overlay FPS is not
performance evidence. No user confirmation was requested again.

Current GPU replacement at this base creates fresh ViewGpu caches. Any later
GPU reuse merge that transfers them must reset readiness or include GPU identity
explicitly. Integration with #2145.4/#2153.1 and an unobstructed portal-heavy
GPU check remain separate merge proof. Global settled-work cleanliness remains
false because source probes and phase scans persist; omitted counter keys are
not promoted to new measured zeroes.

No push, PR, Plus merge or firewall change occurred. All owned evidence remains
within review retention; no expired owned folder was deleted. The local commit
must retain the required `Co-Authored-By: GPT Sol <noreply@openai.com>` trailer.
