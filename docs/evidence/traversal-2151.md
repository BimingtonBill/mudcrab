# Cluster traversal investigation (#2151)

Base: `eaeddc022` (`phase2/cluster-integration`, I21). Implementation branch:
`phase2/cluster-traversal`, checkout `D:/Mudcrab/worktrees/cluster-traversal`.
Build-gate owner: `sol-2151`. Isolated target and retained receipts:
`D:/Mudcrab/builds/cluster-traversal-target`. The requested C: disk check rejected
25 GiB of additional work; D: passed. Nothing is pushed.

Before after-measurements, the branch fast-forwarded to integration `0ef2bde23`
at the lead's request. This includes #2147 runtime-state distant drawing. Its
additional content is part of every final after run; comparison with I21 is not
a single-change performance experiment. No local edits are made to #2147's files.

## Acceptance and execution

1. Separate missing render dependencies from package/layout mismatch, active-world
   mismatch, and stale admission generations before changing behavior.
2. Reproduce the responsible lifecycle with a failing system test, fix its owner,
   and preserve explicit immediate-start routes and uninterrupted travel once started.
3. Run the original fly and fly-fast inputs through the TEST gate. Inspect native
   screenshots and all drawability errors. Run the supplied grader and retain the
   before/after hitch counts. TEST numbers are diagnostic, not quiet timing acceptance.
4. Run formatting, strict engine all-target/all-feature Clippy, and engine nextest.
   Review and commit with the requested co-author trailer.

No detail reduction, geometry ownership fallback, loading cover during traversal,
dependency upgrade, or rendering cap is permitted. Existing generation rejection
and GPU drawability checks must remain intact. Changes in `streaming/far_stock*.rs`
and render source-observation/stock command recording belong to other active tasks.

## Existing I21 evidence

Inputs are the run's exact `launch.json`, runtime log, frame CSV and benchmark JSON
under `D:/Mudcrab/review/drafts/PERF-INT/runs/`. Package:
`D:/Mudcrab/SkyrimConverted-cluster1/derived/clusters-b42ebe26e64f9912`.
The route has 27 waypoints, 129,795.15 units and 117 ground cells. Both I21 routes
finish with zero missing-ground frames.

| Diagnostic | I21 fly | I21 fly-fast |
| --- | ---: | ---: |
| Scored frames | 10,093 | 591 |
| Frames >33 ms | 212 | 201 |
| Frames >100 ms | 206 | 197 |
| Owned-not-drawable errors | 10,093 | 591 |
| End pending asset placements | 0 | 182 |

The supplied `D:/Mudcrab/review/drafts/DS-VERDICT/verdict.py` fails CONTENT,
PERF and TRAVERSAL for both runs. Its window-relative stability rule does not
waive its independent zero-hard-hitch rule. Quiet guard sidecars exist, although
their filenames differ from the grader's expected `guard.json`.

### Drawability hypothesis and separating evidence

The existing fly log's three periodic blocked reports show stable package/layout
scope (`layout: 1`), the same active exterior world (`60`), and unchanged last
accepted render frame (`104`). Admission generation rises from `806` at 60 seconds
to `4919` at 120 seconds and `9426` at 180 seconds. All three show
`snapshot_generation: None`, `counts: None`, and `cover_pending=true`.
The first two have zero pending cluster batches and zero ordinary pending assets,
surfaces, LOD requests and draw confirmations.

Code tracing identifies a feedback loop to reproduce: `drive_route` starts on
`PacingTracker::world_ready_reached`, while the startup cover still fades.
`drive_startup_load` observes the loading pose until that cover finishes. Route
movement changes that pose, invalidating admission before the previous render
generation can publish. The startup fade is reset and the cycle repeats.
`LazyBatches::layout_scope` changes on resource replacement, not ordinary batch
arrival. Missing-consumer log entries are absent from the supplied fly run.

The diagnostic-only error fields added before the new experiment include current
gate scope/generation/counts, evaluated generation/layout, coherence and active
world. A native screenshot must distinguish a still-raised black startup cover
from missing scene geometry; these are different failures.

### Hitch populations

Read-only analysis finds 34 broad source refreshes in each I21 run. Every frame
over 200 ms aligns with one of them. Fly's logged qualification boundaries total
6,350.2216 ms (174.6777–210.5541 ms each); fast totals 6,310.6453 ms
(176.2042–199.9433 ms each). Corresponding CSV frames total 9,493.4361 and
9,535.2998 ms. Alignment is an inference: CSV index is extraction frame minus
`frames_to_world_ready` minus two; the supplied CSV lacks explicit engine frame IDs.
`qualification_ms` includes earlier extraction work, so it is not exclusive
qualification-function time.

The other hard hitches number 172 (fly, 103.8939–139.7817 ms) and 163 (fast,
103.0427–136.7723 ms). Render-prepare p99 is 90.4544/95.8444 ms; render-thread
p99 is 103.0438/116.7524 ms. Graph/present maxima are only 10.5754/8.6500 ms.
Pipeline activity appears in 16/14 render frames, too few to explain all hitches.
Render preparation is implicated; the exact owner requires joined per-frame spans.
I21 contains no per-frame profiler trace. Earlier #2145 captures use different
inputs and cannot supply I21 system costs.

### Completed native baseline trace

`trace-before-fast-serial` completed the entire original fly-fast route using the
immutable I21 Tracy binary. This is a diagnostic TEST, not quiet timing acceptance.
There are 318 scored route frames and 320 post-ready frames. All 318 scored real
clock values match the corresponding render-cycle and frame-series records exactly;
CPU system events are checked inside their main/extract/render wrapper intervals.

The initial preliminary analysis followed an outdated source comment and joined
frame-series IDs to cycle IDs plus one. That phase attribution was wrong and is
superseded. In this capture, frame-series IDs equal cycle IDs. The corrected
analysis checks clock equality and refuses an assumed offset. Authoritative
receipts: `analysis2151-report.md`, `analysis2151-summary.json`, and
`analysis2151-correlations.csv` in the capture folder.

- All 34 slow source extraction calls coincide exactly with the 34 render-origin
  grid changes. They take 192.951–255.023 ms, totaling 7,311.317 ms, and each
  produces the following render-paced sample above 200 ms. Source change queries
  total 3,825.637 ms across the route; the disjoint refresh span totals 4,535.002 ms.
  Every slow extraction has complete journals, no recovery/full refresh, and
  scans 25,511 meshes. The origin-change path adds every indexed root to the
  changed set. This source-extraction work belongs to #2145's active follow-up.
- There are 177 slow readiness-observer calls, each 84.644–115.304 ms. They match
  all 177 preparation intervals above 50 ms. The observer totals 16,571.085 ms
  across the route, 91.887% of measured render preparation. Its route maximum is
  115.304 ms; the larger 141.133 ms whole-capture maximum includes earlier work.
  The 177 slow calls match the 177 `needed_catalog` rebuilds. Attributing their
  exact time to catalog construction remains inference because the baseline has
  no nested catalog timer or per-event key-change log. The catalog currently
  walks all 555,032 scene instances and looks up batch shadow policy per instance.
  Its key includes global prepared-material revision and GPU packing/generation,
  although those inputs do not change instance inventory.
- The 188 scored frames above 100 ms are exactly the union of the 34 source
  spikes and 177 observer spikes, with 23 overlapping samples: 154 additional
  hard hitches have a slow observer without a preceding slow source extraction.
- `far_stock::plan` has 37 slow calls at 35.737–51.020 ms, totaling 1,483.227 ms
  across all route calls. All 34 slow source extractions share slow-plan cycles.
  Command application is separate: 370.766 ms total, 11.420 ms maximum. There is
  no nested timer for the `desired` traversal, so the full planner cost cannot
  be assigned exclusively to that loop. I21 counts only once per world; the new
  per-crossing count walk in #2147 is not measured by this baseline. #2147.1 owns
  that count fix; the lead can route the remaining planner work with these numbers.
- Cluster GPU preparation totals only 2.187 ms during the route, maximum 0.014 ms;
  append-ready work totals 0.637 ms. Material bind-group preparation totals
  53.886 ms, maximum 1.519 ms. Mesh allocation totals 54.329 ms, maximum 5.624 ms.
  The large whole-capture upload maxima occur during startup.

The trace reproduces 318 owned-not-drawable errors and the stale admission
generation with `cover_pending=true`, layout 1 and render frame 104. Native
`before.png` contains a Windows Firewall dialog over a black client area and is
not a clean scene-visibility capture. The dialog is not automated or allowed;
counter proof continues while the user dismisses it. Final visual inspection
must use an unobstructed running engine.

### Fast route pending placements

The 182 pending placements at exit comprise 150 queued and 32 armed/spawned that
frame; 35 validations finish in that scan. All 203 cell requests have returned,
with zero active requests, loading cells, surface pending or asset failures.
`pending_asset_instances` counts unarmed plus pending placements, less completed
validations; it is not a unique-file or failure count. The route ends measurement
on arrival without a drain: measured 32.4447 seconds versus authored 32.4488 seconds.
An active backlog at termination is established. Permanently stuck individual
placements are not established by this final snapshot.

## Admission fix and current verification

The real `drive_route`/`StartupLoadPlugin` regression failed against the original
behavior: `world_ready_route_waits_for_startup_cover_to_finish_before_moving`
panics with "a ready latch must not start traversal during the startup fade".
Receipt: `route-red.log`, nextest exit 100, one test failed. The test also requires
actual travel after fade completion. A separate compatibility test preserves an
explicit immediate-start route.

The fix makes the initial `world_ready` route admission wait for the startup
cover to finish. A running route bypasses this initial check and never pauses
for streaming. Existing generation rejection, render proof and stock ownership
rules remain unchanged. The startup cover logs its completion once. Drawability
errors now include evaluated generation/layout, coherence, active world and
current cached gate diagnostics; the successful path adds no inventory scan.

First isolated release compilation failed with `rustc-LLVM ERROR: out of memory`
(exit 101) during concurrent heavy jobs. An attempted baseline Tracy capture
aborted before readiness with "memory allocation of 1398112 bytes failed"
(exit 3221226505). It is marked `INVALID.txt` and supplies no traversal evidence.
Native preflight subsequently rejected insufficient commit headroom. No gate
hold or admission check was bypassed. Own heavy jobs are being serialized.

The immutable baseline Tracy binary is copied from #2145's fresh I21 source
build, completed at 17:39 on 2026-10-25. At inspection/copy the source checkout
was `eaeddc022` with only `tests/nif_depth_bindings.rs` modified. Its SHA-256 is
`c85dfbcba3cb1ddacb0ad04628692e811e285e573da80b9f8888e57d03b30b76`.
Later edits in that other task do not describe the retained immutable binary.
The trace harness checks that the recorder's default Tracy port is unoccupied
before launching, to avoid attachment to another owner's client.

## Readiness inventory cache

The baseline's catalog key mixed instance-inventory scope with GPU packing and
global prepared-material changes. This needlessly repeated the 555,032-instance
walk as unrelated stock materials arrived. The regression
`prepared_material_changes_refresh_proof_without_rewalking_instances` failed
against that behavior (`catalog-red.log`, exit 100): the material-only change
invoked the inventory callback.

The catalog now keys package/GPU identity, scene change tick, refusal scope, lazy
layout scope and runtime reference-state revision. GPU generation/layout and
prepared-material revision still invalidate the readiness aggregate. Shadow
policy is evaluated per needed batch during aggregation, preserving material
changes without placing material state in the instance catalog. Unchanged
inputs retain the existing replay path. New counters identify material, GPU
generation and GPU layout invalidations, and nested Tracy spans distinguish
catalog construction from aggregation.

The earlier route-focused green run passed 74 tests. The final-source focused
run passed 75 tests, including the new material-inventory regression, route
admission, immediate-start compatibility and withdrawal/generation checks
(`final-focused.log`, exit zero). Final-source formatting and strict engine
all-target/all-feature Clippy passed (`final-fmt.exit` and `final-clippy.exit`,
both zero). Clippy retains the existing dependency future-incompatibility notice
for `proc-macro-error2 2.0.1`.

Independent read-only review of the full three-file diff against `0ef2bde23`
found no actionable correctness issue. It confirmed inventory dependencies,
material/GPU proof invalidation, conservative missing-material shadow policy,
withdrawal replay invalidation and initial-only route admission. Its limits are
native exterior proof and final green execution; the focused route fixture uses
an interior world and a prelatched tracker. Final native counter results are
below; clean visual acceptance is limited by the foreign security dialog. No
traversal stability, full-detail pixel coverage, quiet timing or 160 FPS
acceptance is claimed.

The final engine all-target nextest suite passed 3,306 tests with 13 skipped
(`final-nextest.log`, exit zero).

The final release build passed (`final-release.exit`, zero). Its immutable
`final-2151.exe` SHA-256 is
`2194df9699ffa8b89f5c88c946aae1430da428a058974f3bf4dbfca6acdab2c4`.
`final-build.json` records `0ef2bde23` plus the retained runtime patch with
SHA-256 `4659ace46cde9827702724170938b0429180addc131a907208a4c34b22438957`.
The runtime source still matched that patch after the build.

Repository workspace handoff checks also passed through the gate: strict
all-target/all-feature Clippy; nextest 5,543 passed, 63 skipped; doctests
18 passed, 15 ignored. All three `handoff-*.exit` receipts are zero. The first
native warm-up admission was refused under another owner's timing hold (exit 4)
and is explicitly marked `NOT_RUN.txt`; no engine launched from that attempt.

## Final native TEST results

The successful first launch, `after-warmup-2`, is discarded. Its ten-second
standing window ended before readiness and supplies no settled-performance
claim. `after-standing-1` ran for a 90-second measured window, reached readiness
at 38.404 seconds and finished the existing cover fade. Its 3,691 post-ready
frames have a maximum of 28.619 ms and zero frames above 33 ms. All after-ready
full-work counters are empty (`settled_work.clean=true`), including the catalog
and readiness aggregate. Pending assets and owned-not-drawable errors are zero.

The two scored flights use the original I21 route/settings/package with the
final normal release binary and `0ef2bde23` content. The additional profile files
are diagnostic. These are TEST runs, not quiet timing or an isolated performance
comparison. The supplied grader was run on each folder; exact commands, hashes,
frame CSVs, benchmark JSON, profiler exports and grader receipts are retained.

| Observation | I21 fly | TEST `after-fly-1` | I21 fast | TEST `after-flyfast-1` |
| --- | ---: | ---: | ---: | ---: |
| Scored route frames | 10,093 | 10,293 | 591 | 1,010 |
| Frames >33 ms | 212 | 88 | 201 | 130 |
| Frames >50 ms | 209 | 44 | 198 | 52 |
| Frames >100 ms | 206 | 39 | 197 | 40 |
| Frames >200 ms | 34 | 34 | 34 | 34 |
| Owned-not-drawable errors | 10,093 | 0 | 591 | 0 |
| End pending asset placements | 0 | 0 | 182 | 0 |
| Engine post-ready p99, ms | 110.491 | 31.925 | 317.839 | 444.902 |

Both final flights complete all 27 waypoints, 129,795.15 units and 117 ground
cells, with zero route frames missing ground. Their readiness times are 40.020
and 38.470 seconds. The cover finishes before route admission: route starts are
40.320 and 38.767 seconds. Neither route pauses after starting. Both logs contain
zero ERROR lines, zero failed cluster batches and zero streaming/asset failures.
The grader reports CONTENT PASS for both; PERF and TRAVERSAL still fail.

The fly endpoint contains 115,660 entities and 35,497 mesh entities versus I21's
102,589 and 32,474. `cluster/undrawn_static_geometry` and
`cluster/undrawn_runtime_refused` are both zero in each final flight. Stock
ownership policy is unchanged; no stock replacement path is added.

The engine returns exit 1 because `commit_budget_respected=false`, with startup
commit-budget violations retained in the benchmark. Relaxing that check is not
part of this fix. The generous FPS/memory thresholds are the exact I21 inputs,
not an acceptance verdict. The grader independently rejects remaining stalls.

### Cache and residual hitch evidence

After ready, fly/fast record 180/179 material-input changes, 199/196 aggregate
refreshes, but only **two instance-catalog rebuilds each**, versus 177 catalog
rebuilds in the before-fast Tracy capture. Neither final flight records a GPU
generation/layout input change after ready. Global material changes still refresh
proof, and settled standing performs neither inventory nor aggregate work.

Render preparation p99 is 9.352/18.004 ms in the final fly/fast TEST runs, versus
I21's 90.454/95.844 ms. The final preparation maxima remain 81.606/89.615 ms.
Those maxima are not exclusive catalog timers. Two inventory changes still
require full catalog construction; their exact triggering key fields and the
remaining non-origin hard frames need the new nested spans in a future capture.

Each final flight still logs 34 post-fade source qualification boundaries above
100 ms: fly 185.594–329.677 ms (7,793.096 ms total), fast 213.472–346.354 ms
(8,980.663 ms total). Each also retains 34 route frames above 200 ms. The count
match is inference for these normal builds; the before Tracy capture supplies
the exact source/origin correlation. As above, qualification boundaries include
earlier extraction work and must not be called exclusive qualification time.
The source-origin work belongs to #2145. The baseline far-stock planner totals
and the unmeasured new per-crossing counts/desired walk remain a handoff to
#2147.1/the lead. No local hunk touches their files.

Fast's final pending count is now zero, so the end backlog is not reproduced.
This does not identify/drain the original 182 placements individually: #2147 also
changes exact distant content admission. I21's progressing admissions and
validations establish that its final snapshot was an active backlog, not proof
of permanent starvation.

### Measurement and listener details

Route CSVs use the real clock. Ordinary standing CSVs include loading and use the
virtual clock, which clamps long frames at 250 ms. Their raw all-window hitch
counts must not be presented as post-ready counts. The analyzer now limits
cycle joins to routes. `frame-series.csv` stores `real_ms` as f32 while cycle and
benchmark clocks are f64; joins allow two f32 ULPs plus four-decimal CSV rounding,
and retain exact frame IDs. No assumed frame offset is used.

The final binary is **not a `--features tracy` build**. Its release fingerprint
(`engine-59bbaa9621e78ff2/bin-engine.json`) records `features: []`. The build command
is `cargo build --release -p engine`. Read-only socket enumeration of the running
final engine found no TCP listeners or UDP endpoints. The optional
`RemoteHttpPlugin` requires the `remote` feature and `--remote`, and binds
`127.0.0.1`; neither is enabled here. The earlier immutable before binary is a
Tracy build and must be distinguished from this final executable.

Final-run PNGs are marked obstructed: a Windows Security dialog names
the different executable `r2-engine-only-after-0ef2bde23.exe`. The window API
omits that dialog from enumeration, so the PNG itself is authoritative. No
firewall setting or Allow action is used. The listener receipt
`final-listener-proof.json` records final executable PID 42332 with no TCP
listeners or UDP endpoints. No live process for the named peer executable was
found during that inspection; its permission prompt remained visible.

`after-fly-visual-1` repeats the unchanged scored route solely for visual
inspection. `after-visual-hold-1` uses the same normal binary/package/route
without measurement/exit flags, so the window remains at the final pose for an
end-of-task capture. Its startup cover finishes normally too. These launches
do not replace the first-run hitch results above. The inspected peripheral
scene contains detailed terrain, rocks, trees and architecture, but the popup
obscures part of every saved view. Therefore these PNGs are **not clean visual
acceptance**, and cannot prove no pop-in throughout the route. The user asked
not to wait for the pending human Cancel; this visibility limitation remains
explicit in the handoff.

Quiet complete-content performance acceptance remains outstanding. In
particular, fast p99 is worse despite fewer hard frames. This branch does not
meet the user's no-stalls traversal bar by itself.
