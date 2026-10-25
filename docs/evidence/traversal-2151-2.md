# Cluster traversal inventory and command application (#2151.2)

This round starts at `ec2596e8d`, the integration of #2151.1. All native captures
are diagnostic TEST runs with Tracy, not profiler-free performance acceptance.
The package, settings, route geometry and content remain those in the retained
I21 launch receipts. No content cap, reduced detail, owned stock stand-in or
additional traversal loading cover is permitted.

## Baseline and exact frame attribution

The baseline is `round1-after-fly` and `round1-after-fly-fast` under
`D:/Mudcrab/builds/cluster-traversal-target`. Their immutable binary and verified
source receipts are documented in `traversal-2151-1.md`.

| Reported post-ready frames, older admission boundary | Fly | Fly-fast |
| --- | ---: | ---: |
| All frames | 12,057 | 2,307 |
| Above 33 ms | 102 | 98 |
| Above 100 ms | 2 | 2 |
| Above 200 ms | 0 | 0 |
| Engine p99, ms | 31.9120 | 69.3352 |
| Worst, ms | 110.4957 | 110.3836 |
| Owned-not-drawable errors | 0 | 0 |

Both baselines pass CONTENT and finish with zero pending placements. PERF and
TRAVERSAL fail. Five fly frames above 33 ms occur between the ready latch and
route movement. They remain in the attribution ledger, but the cover-validity
check below excludes them from a visible-traversal improvement claim.

Every one of the 200 frames above 33 ms was joined to actual engine cycles and
Tracy clocks. The complete ledger is `round2-attribution/{fly,fly-fast}-frames.csv`;
the method, all component intersections and residuals are in `REPORT.txt` there.
The join uses actual `send_time` spans and both exported frame series, with no
estimated clock offset. Parallel and nested spans are measured as interval unions.

| Observational frame group | Fly count / frame-ms sum | Fly-fast count / frame-ms sum |
| --- | ---: | ---: |
| Origin crossing, main and render | 34 / 2579.9234 | 34 / 2465.0219 |
| Readiness aggregate tail | 29 / 1090.9539 | 31 / 1142.7341 |
| Selection and streaming tail | 22 / 811.8233 | 20 / 714.6888 |
| Scene spawn tail | 3 / 126.4500 | 3 / 133.4834 |
| Material queue tail | 2 / 109.6711 | 3 / 158.5840 |
| Distant admission, main and render | 3 / 114.7513 | 3 / 113.7147 |
| Catalog | 2 / 216.5598 | 2 / 212.2312 |
| Asset upload tail | 2 / 70.7724 | 0 |
| Cover swapchain tail | 5 / 179.9902 | 0 |
| Mixed submission | 0 | 2 / 69.3842 |

These exclusive labels describe observed groups, not a sufficient single cause
or estimated savings. The ledgers retain 776.568 ms of residual intervals across
all 200 frames, at most 5.436 ms in one frame. The largest individual unresolved
gap is 0.917 ms. Every frame is joined; not every microsecond has a named owner.

The 34 crossing command applications owned by `engine::streaming::far_stock::plan`
take 10.242617–18.224897 ms in fly (395.199678 ms total) and
10.047777–16.025648 ms in fast (383.166861 ms total). These are deferred command
spans, distinct from the planner body. The body takes 7.606183–15.739772 ms across
both runs and often overlaps the previous render interval. No baseline nested
timer isolates subtree validation inside the deferred closure; that narrower
attribution remains inference until the causal regression and after capture.

Source extraction at crossings takes 11.052–18.067 ms in fly and
10.693–15.289 ms in fast. Its change-query umbrella contains several operations;
these captures do not isolate hot queries. Readiness aggregation, selection,
cell commands, spawning and uploads also contribute separately. No overlapping
component totals are added together.

## Implementation and validation status

The counter-only release completed before behavior changes. Its unmeasured
gated fast TEST produced `round2-key-counter-fly-fast/key-evidence.json`. Exactly
two catalog events occurred after ready. Both changed only
`runtime_state_revision`, from `Some(1)` to `Some(2)` to `Some(3)`. GPU identity,
scene tick, permanent-refusal revision and layout scope stayed fixed. The two
matching pose revocations were references 544179 and 100665609. Read-only package
coverage identifies both as `not_eligible / state_change_writer_unavailable`,
with no mapped primitive contribution. The numeric IDs are evidence only;
implementation uses the producer's inverse reference index.

The old consumer therefore rescans the complete scene for semantic changes that
touch zero catalog batches. This establishes N29's invalidation cause. The sample
is not a timing comparison: it ran without an exclusive timing guard. Its exit 1
is the existing benchmark acceptance failure, not a panic; the benchmark and
complete route were written before normal application error reporting.

The native sample's source hash, counter-only patch and executable hash are
retained separately. Its old/new key log and counters agree: two runtime-key
changes and two catalog rebuilds, with no other after-ready catalog-key cause.

The gated red run compiled successfully and ran two tests. Both failed at their
intended assertions (`round2-red.log`, exit 100): GPU replacement called the
catalog builder twice instead of zero; real source adoption marked an unrelated
filled holder's visibility and propagated layers changed, instead of leaving
both unchanged. Earlier assertions verify retained owner, model handle, children,
collider and authoritative parent transfer before the holder-change assertion.

The streaming change separates empty-holder pruning from global role application.
Target-only replans and completed owner transfers prune only. Actual demand-role
changes retain full role refresh, and actual transfers retain their existing
authority validation and layer rebinding. Equal ordinary propagated layers and
visibility values are not reinserted. This removes unrelated hierarchy work at
its caller rather than bypassing validation inside the portal module.

No `portal.rs` hunk is included. New decisions and cleanup live in
`streaming/far_stock.rs`. Ordinary stock mode retains placed-object propagation
and hidden-role visibility. The prototype already calls portal role helpers;
this patch adds no portal API dependency. A clean cluster-stack build with that
separate feature module absent has not yet been compiled here.

The loader now publishes an immutable `InstanceInventory` after its final
instance reorder, using the same layout scope as `LazyBatches`. It contains all
instances, including those not yet GPU-ready, and inverse record/reference batch
membership. Refusal and runtime revision deltas rebuild only affected batches.
Per-world/domain contributions retain OR-invalidity across surviving objects;
removing the last contribution removes its stale requirement. An unknown
reference changes no batch membership and visits no catalog instances.

Inventory validation precedes aggregate replay. A tracked scene mutation checks
the ordered identity/batch snapshot; an invalid snapshot remains withdrawn until
valid inventory returns. Metadata-only scene writes validate without rebuilding
batch contributions. Settled frames compare retained scopes and revisions without
walking instances. GPU replacement refreshes aggregate proof without rebuilding
inventory. Publication retains existing package, layout, admission-generation and
frame checks, adds inventory allocation provenance, and clears replay after a
rejected publication so restoration of the same allocation can recover.

The shared rendering contract hunk is limited to `ClusterScene.inventory:
Option<Arc<InstanceInventory>>`, its fixture constructor and a test literal in
`render.rs`. `package.rs` allocates the shared layout scope and constructs the
index after final reorder, before scene publication. The other scene literals
add `inventory: None`. No GPU preparation/reuse or source-query hunk is changed;
`handoff.rs` contains only that fixture field addition.

Five index tests, nine per-batch regressions, GPU aggregate freshness, publication
rejection/recovery, and actual owner adoption cover the new contracts. The
same-world OR-survivor fixture uses the real converter/finalizer and invented
references. Independent loader, holder, readiness and measurement reviews found
no actionable correctness findings. Their static reviews do not substitute for
native or performance proof.

The execution plan is `traversal-2151-2-plan.md`. Frozen-source focused checks
passed 207 tests after observed reds. Full gates and native after results are
recorded below only after their completion.

## Cover validity and measurement admission

The lead's additional validity check found a separate measurement defect. The
existing `drive_route` guard waits until `StartupLoad.cover_pending()` is false,
so these `ec2596e8d` routes cannot begin while their startup cover remains. The
pacing tracker nevertheless latched its own post-ready window at dependency
admission, before the existing fade completed. The reported full post-ready
window therefore includes covered frames and is not a valid fully visible
measurement boundary.

| Existing run | Cover-minus-dependency-ready log interval | Route start, `Time<Real>` | Exact cover-minus-route gap |
| --- | ---: | ---: | --- |
| round1-after-fly | 257.098 ms | 31,195.8353 ms | Unknown: old cover log has no shared run clock |
| round1-after-fly-fast | 263.925 ms | 36,085.8058 ms | Unknown: old cover log has no shared run clock |
| round2-key-counter-fly-fast | 256.166 ms | 46,973.5037 ms | Unknown: old cover log has no shared run clock |

The first interval subtracts two UTC log timestamps. It is not a subtraction
between UTC, the startup `Instant`, the pacing `Instant` and Bevy `Time<Real>`.
Those clocks have no verified bridge in the old artifacts. The route's source
guard establishes ordering; it does not supply an exact numeric gap. Retained
clock provenance is `round2-cover-validity/evidence.json` in the target.

Route CSVs contain 97 fly and 98 fast frames above 33 ms, with two above 100 ms
each. The five additional fly frames in the full attribution ledger belong to
the covered tail. Removing that measurement error must not be credited to the
catalog or holder performance fixes.

A new scheduled regression compiled and failed first (`round2-cover-red.log`,
exit 100): the pacing tracker latched while the mandatory cluster cover was
visible. The fix keeps dependency admission and configured-pacing restoration
independent, then admits measurement only after cover removal. It does not hold
or restart a running route. Work-counter readiness also moves to visible release;
a non-cluster timeout without dependency admission does not mark the world ready.

New cover, pacing-window and route events all record unclamped
`Time<Real>.elapsed`. Each final capture must retain `visibility-admission.json`
and prove cover release precedes the recorded window's starting instant and
route start. The capture helper rejects a mixed-clock or covered-window result.

## Frozen-source checks

All 24 Rust files match `round2-source-hashes.json` through the final checks.
Every heavy command ran through the shared gate with owner `sol-2151`, one
Cargo build job and this worktree's independent target. The final receipts are
`round2-final-*.{log,exit}`; fmt and strict engine Clippy reuse the identical-source
`round2-initial-*` receipts explicitly recorded by the final campaign.

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| Strict engine all-target/all-feature Clippy | Pass |
| Focused regressions | 207 passed |
| Engine all-target nextest | 3,383 passed, 13 skipped |
| Strict workspace all-target/all-feature Clippy | Pass |
| Workspace nextest | 5,620 passed, 63 skipped |
| Workspace doctests | 18 passed, 15 ignored |
| Release engine with `--features tracy` | Pass |

The retained final executable is `round2-after-tracy.exe`, SHA-256
`ccffbea880dba907904f6daa113757accadeeb5e6e491494fec80a9d76081636`.
`round2-final-binary.json` binds it to the exact source hashes and to
`tracy-client/only-localhost`. This binary is a Tracy build; its profiler listener
is constrained to loopback. The older security prompt names the peer executable
`r2-engine-only-after-0ef2bde23.exe`, not this executable.

The dependency's existing `proc-macro-error2 v2.0.1` future-incompatibility notice
remains. No checks, settings or content were relaxed. The new inventory API and
measurement admission tests pass in both focused and full suites.

## Final TEST captures and attribution

One `timing-2151` hold was acquired at 07:55 local time with an honest 09:09
deadline. It drained the existing peer job, then ran one discarded warm-up,
scored fast and scored fly. The campaign's `finally` cleared the hold at about
08:10:37, before trace export or analysis; fresh gate status confirms no hold.
Both retained guards report `quiet`. These are single TEST/Tracy observations,
not profiler-free timing acceptance or a repeatability claim.

| Visible route CSV comparison | Fly before | Fly after | Fast before | Fast after |
| --- | ---: | ---: | ---: | ---: |
| Frames above 33 ms | 97 | 49 | 98 | 44 |
| Frames above 100 ms | 2 | 0 | 2 | 0 |
| p99, same grader interpolation, ms | 31.7193 | 26.7093 | 69.3546 | 56.0444 |
| Worst frame, ms | 110.4957 | 69.4343 | 110.3836 | 85.2595 |
| Owned-not-drawable errors | 0 | 0 | 0 | 0 |
| Pending placements at finish | 0 | 0 | 0 | 0 |

The visible comparison excludes the five old covered fly frames. The after
engine's complete admitted windows contain 12,110/2,333 frames, respectively,
with p99 26.7129/56.0544 ms and the same 49/44 band counts. No after hitch lies
before route movement. Both supplied graders return CONTENT PASS and TRAVERSAL
PASS; PERF and OVERALL still FAIL. Their stable-window labels do not establish
the user's absolute frame-time goal.

All four before/after endpoints retain 115,660 entities, 35,497 mesh entities and
136 terrain patches. Both after routes complete all 27 waypoints, 129,795 units
and 117 ground cells, with zero route frames without ground. Static and
runtime-refused undrawn counters are explicitly zero; failed streaming and
pending placements are zero. No settings or route speeds changed.

All **93/93** after frames above 33 ms have exact cycle/clock joins. Ledgers are
`round2-after-attribution/{fly,fly-fast}-frames.{csv,json}`; summaries,
`comparison.json`, narrow full-route exports and helper sources live beside
them. Maximum clock join errors are 0.000268 ms fly and 0.002765 ms fast.

| Exclusive observational group | Fly before count / frame-ms | Fly after count / frame-ms | Fast before count / frame-ms | Fast after count / frame-ms |
| --- | ---: | ---: | ---: | ---: |
| Origin crossing, main and render | 34 / 2579.9234 | 34 / 1944.0768 | 34 / 2465.0219 | 34 / 2020.4428 |
| Readiness aggregate tail | 29 / 1090.9539 | 10 / 382.2381 | 31 / 1142.7341 | 4 / 134.6363 |
| Selection and streaming tail | 22 / 811.8233 | 3 / 102.1918 | 20 / 714.6888 | 4 / 139.6933 |
| Scene spawn tail | 3 / 126.4500 | 0 | 3 / 133.4834 | 0 |
| Material queue tail | 2 / 109.6711 | 2 / 79.3499 | 3 / 158.5840 | 1 / 51.6484 |
| Distant admission | 3 / 114.7513 | 0 | 3 / 113.7147 | 0 |
| Catalog | 2 / 216.5598 | 0 | 2 / 212.2312 | 0 |
| Upload tail | 2 / 70.7724 | 0 | 0 | 1 / 34.0103 |
| Mixed submission | 0 | 0 | 2 / 69.3842 | 0 |

Counts classify frames; zero does not mean that spawning, uploads or other
necessary work stopped executing. Frame-duration sums are not exclusive system
costs. Each ledger retains overlapping component intersections and unnamed gaps.
Unnamed interval totals are 200.2301 ms fly and 184.2771 ms fast; maxima per frame
are 5.2902/5.7580 ms. The largest individual gap is 0.7481/0.8390 ms. Thus every
band frame is joined and classified, while sub-millisecond gaps remain unnamed.

N29's two baseline catalog calls take 74.5293/69.6863 ms fly and
68.8188/68.0471 ms fast. Their after equivalents take 0.003229/0.003420 ms and
0.003600/0.003970 ms. Both native after counters witness exactly one initial
index build, 555,032 initial indexed/validated/catalog instances and 13,420
initial rebuilt batches. After visible release, both runtime-key changes remain
observed, with **zero** rebuilt batches, catalog instance visits, index builds or
validation instance visits. This is measured zero work, not an absent counter.

All 34 crossing planner-command calls are retained in the narrow export. Full
call ranges fall from 10.2426–18.2249 to 0.2371–0.5172 ms fly, and from
10.0478–16.0256 to 0.2401–0.7175 ms fast. Their sums fall from 395.1997 to
11.6767 ms and 383.1669 to 13.0581 ms. Nested spans and counters show zero
global holder-role refreshes after ready. Empty-holder pruning still runs at
transfers/target changes (1,032,462/1,032,461 holder visits); it is not a new
settled-frame poll. The native process-wide settled-work report is not globally
clean: existing source queries, selection and other traversal work remain.

## Common-clock visibility proof

`visibility-admission.json` in each scored run proves ordering using only
`Time<Real>.elapsed`; benchmark route-start fields agree with the log.

| Run | Cover release, run ms | Window start, run ms | Route start, run ms | Cover minus route, ms | Cover minus window, ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Fly | 31794.3613 | 31819.7112 | 31832.8924 | -38.5311 | -25.3499 |
| Fast | 32601.5538 | 32629.3115 | 32641.6922 | -40.1384 | -27.7577 |

Both route motion and the full scored post-ready interval begin after cover
release. No running route waits for streaming or re-enters the startup cover.

Log review finds zero warnings/errors after cover release in both scored runs.
Each has 37 database-backpressure pending-request warnings before release, as do
both baselines; all requests subsequently drain. All four logs contain the same
nonfatal profiler bootstrap `SymInitialize` code 87 diagnostic. Startup enable
parent, missing UV/tangent and transient dependency warnings remain; the routes
finish without pending assets. Engine exit 1 retains failed performance
acceptance; it is not a panic or a gate/check failure.

## Remaining work and visual limit

The remaining 34 crossings per route cost 41.7208–69.4343 ms fly and
41.7644–85.2595 ms fast. Their critical-path intersections include source change
queries (9.3845–13.8284 ms), flat-stock synchronization (4.0392–6.2889 ms),
selection (5.3883–12.7907 ms), readiness aggregation (up to 11.1433 ms), and
smaller spawning, retirement and submission work. These components overlap;
their inclusive costs must not be summed into a fabricated exclusive total.

The concrete follow-up is to time individual hot/fact query families and
flat-stock comparison/command work first, then carry semantic producer deltas
separately from exact origin rebases. Preserve the existing overlap, finite-pose,
missed-rebase and full-oracle tests. For selection/readiness tails, retain
generation-scoped per-batch/per-consumer invalidation and update aggregate
contributions from changed producers rather than scanning every batch. Material
and upload tails need asset/task/batch identities joined to request, worker
preparation and publication before moving independent preparation off-thread;
do not spread their work with caps or postpone content. Source-query and GPU
reuse integration remains coordinated with #2145.3/#2153; no corresponding
renderer/query hunks are included here.

The final unmeasured gated TEST visual was checked once after startup release.
Fresh native window enumeration omitted the old security modal, but the engine
screenshot still shows **Windows Security for
`r2-engine-only-after-0ef2bde23.exe`** covering the center of the rendered world.
`round2-after-visual/{window-enumeration.json,visual-verdict.json,blocked-0.jpg}`
retains that obstruction. The owned visual engine was then stopped. This is not
clean full-frame or full-route pixel/pop-in proof. No further dismissal request,
retry loop, network allowance or firewall setting change was made.

Structural scene changes must continue publishing a rebuilt inventory. Tracked
same-count identity, order or batch mutations are rejected before replay;
current-main in-place edits retain the existing extraction pipeline latency.
Profiler-free repeated timing, the user's steady traversal target, and an
unobstructed visual route remain open. No push or PR was made.
