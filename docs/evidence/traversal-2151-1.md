# Cluster traversal follow-up (#2151.1)

This round starts at integration `8ae4e5733`, fast-forwarded into
`phase2/cluster-traversal` before measurement. It includes #2151 admission/readiness,
#2147 runtime-state content, #2147.1 inventory caching, and Tracy's loopback-only
feature. All captures are diagnostic TEST runs, not quiet timing acceptance.

Before retained checks and after measurement, the lead required integration
`551909a16` (#2145.2 source-query admission and its standing CPU changes). That
head was fast-forwarded into this branch. The source-query declarations and their
native-filter oracles were reconciled with the moved flat hot facts, preserving
the new tick-probe admission path. After results therefore include #2145.2;
overall frame-time improvements cannot be attributed exclusively to this patch.

## Reproduction and ownership

- Checkout: `D:/Mudcrab/worktrees/cluster-traversal`.
- Build target and retained evidence: `D:/Mudcrab/builds/cluster-traversal-target`.
- Gate owner: `sol-2151`. Heavy Cargo commands use the build gate.
- Route inputs come from the original I21 fly launch manifest, with the current
  executable and checkout-local fly/fly-fast route paths. Package remains
  `D:/Mudcrab/SkyrimConverted-cluster1/derived/clusters-b42ebe26e64f9912`.
- `round2151.py` retains commands, binary SHA-256, source patches, engine profiles,
  logs, verdicts and guard results. It acquires `timing-2151` with drain, runs the
  guard as its owner, and clears the hold in `finally`. The first full fast route
  after each build is discarded as warm-up. Captures use release plus `tracy`;
  integration enables `tracy-client/only-localhost`.
- `trace2151.py` joins route clocks to engine cycles, then uses Tracy root-span
  containment and known system schedule ownership. It does not estimate a clock
  offset. Ambiguous worker spans are retained separately, never assigned by guess.
  System events below 0.03 ms are omitted from the compact export; complete source
  traces and engine phase timings remain available.

The join was checked against the older serial baseline: all 188 hard route frames
are the union of 34 preceding source-extraction spikes and 177 readiness-observer
spikes, with 23 overlaps and zero unexplained frames. This is a parser validation,
not the merged-head baseline for this round.

## Investigation

### Merged-head baseline

The completed campaign is `round1-before-{warmup-fly-fast,fly-fast,fly}` under the
retained target. The warm-up is discarded. Both scored guards are quiet, both
routes complete, and both receive CONTENT PASS. Neither has an owned-not-drawable
error or pending assets at termination. The campaign exited 0 and finally cleared
its hold; gate status then showed the next owner's `timing-2145-capture` only.

| Route | Scored frames | >33 ms | >100 ms | >200 ms | Post-ready engine p99 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Fly | 11,263 | 82 | 36 | 34 | 31.03 ms |
| Fly-fast | 1,602 | 94 | 35 | 34 | 298.04 ms |

The engine's complete post-ready window includes 17 additional frames in fly and
16 in fast before route scoring. Its >33 ms counts are 82 and 97 respectively;
>100/>200 ms counts are identical to the route table. The extra fast cover frames
therefore do not conceal another hard hitch. `analyze.py` validates the full-window
>33/>50 counts against the engine summary and retains its hard cycle identities.

Every hard route frame has an exact cycle attribution, with zero dropped streaming
samples and zero unexplained hard frames. Each route has 34 origin changes and
exactly 34 source calls above 100 ms on those changes; no source spike occurs
without an origin change. Each call causes the following render-paced hard frame.
Source calls take 185.425–226.603 ms in fly (6,686.185 ms total), and
184.587–226.862 ms in fast (6,743.156 ms total). The fast route's same-crossing
exclusive desired construction takes 43.331–57.189 ms, totaling 1,583.814 ms over
those 34 crossings. Its run-lifetime desired-candidate counter is 10,104,549.

The remaining hard frames are independently timed catalog rebuilds, not another
origin or asset-upload inference:

| Route frame / engine cycle | Frame | Observer | Catalog | Aggregate |
| --- | ---: | ---: | ---: | ---: |
| Fast 612 / 2181 | 102.547 ms | 74.481 ms | 68.447 ms | 6.019 ms |
| Fly 4233 / 5822 | 104.511 ms | 78.423 ms | 73.402 ms | 5.006 ms |
| Fly 6184 / 7773 | 113.975 ms | 74.884 ms | 68.775 ms | 6.095 ms |

`readiness_bridge::observe` rebuilds that catalog by walking every scene instance
and resolving world/batch membership. The timer proves the expensive operation;
this capture does not distinguish which `NeededCatalogKey` input invalidated it.
If hard catalog frames remain after the traversal fixes, the concrete follow-up
is to record the changed key fields, maintain per-batch inventory contributions at
their scene/runtime/refusal change boundary, and publish a generation-matched
catalog. No generation or GPU-readiness check may be weakened to avoid the walk.

These are TEST plus Tracy measurements. Guard quietness excludes competing builds;
it does not remove profiler overhead or establish performance acceptance. The
Tracy preamble reports `SymInitialize` error 87; instrumented zones, clocks and all
cycle joins remain present. Engine exit 1 is retained alongside grader failures.
The fast listener snapshot records TCP `127.0.0.1:8086` and its loopback capture
connection, plus an ephemeral wildcard UDP endpoint. It does not claim every
socket is loopback-bound. Locked `tracy-client-sys 0.29.0` selects loopback broadcast
address `127.255.255.255` under `TRACY_ONLY_LOCALHOST`; its UDP sender opens a
datagram socket without an explicit local bind. This is consistent with the
observed wildcard UDP source endpoint; no packet capture was taken. Native window
enumeration found no firewall dialog.

The planner's existing inventory cache does not cache desired membership. Every
changed `LodDemand`, including its center, reaches
`placed_cells_in_window(world, center, None)` in cluster-owned mode. An exclusive
`far_stock::desired` span and `cluster/stock_desired_candidates` counter were added
before changing this behavior. The planner also drops pending exact requests when
the camera demand changes even if the exact target map remains identical.

Source extraction explicitly adds every indexed root when `RenderOrigin` changes.
Its cached hot facts additionally compare cell transforms literally, although
qualification only requires their exact grid-relative pose at the current origin.
Flat stock has another equivalent-change path: ordinary synchronization moves both
the expected global affine and the actual draw affine while preserving model-space
proof. Raw change flags currently repeat qualification for those writes.

The required invariant is proof equivalence, not an origin-change exemption. An
exact rebase may reuse proof; a missed cell rebase, changed cell translation,
rotation or scale, changed model matrix, or mismatched flat draw must still revoke
the affected proof. Source output origins and current view calculations must keep
advancing even when qualification is reused.

Regression coverage uses the real producer certificate and GLTF loader for source
proof, including a forced full-qualification oracle. Planner regressions use the
runtime enable-state fixture and assert exact desired identities, candidate visits,
and pending-response survival across empty-cell crossings.

The red receipts are `round1-origin-red.log/.exit` and
`round1-planner-flat-red.log/.exit` in the retained target. Both exact-rebase tests
observe qualification refreshes increasing from 1 to 2. Three empty planner
crossings visit 27 additional fixture candidates instead of zero. The pending
response test reports `Disconnected(..)` after the crossing. The cell-edit
regression already passes on the baseline, protecting its existing refusal and
recovery behavior. In total, the selected baseline runs contain four expected
failures and one passing protection test.

## Proof-preserving implementation

The demand cache separates unbounded static membership from a bounded source
contribution. Package identity, placing world, ownership mode, effective enable
revision and package-matched permanent-refusal revision invalidate the unbounded
projection. Camera center, radius, role and transient source feedback affect only
the bounded contribution. Crossing updates mutate the union in place, preserving
unbounded overlaps without cloning or comparing the world-sized map. Ordinary
Complete arrivals enroll in the bounded contribution at their existing admission
boundary. An unchanged effective target map retains request generation, receivers
and ready responses while updating center-dependent queue order and holder roles.
Actual target changes retain the existing generation and publication checks.

Source hot facts retain the qualification predicate for exterior cells rather than
the render-relative translation. Correct rebases remain equivalent; missed rebases
change the predicate and invalidate indexed cell dependents. On origin changes,
checking cells catches unchanged transforms without marking every source root.
Flat metadata retains a finite model matrix and the exact expected/actual affine
agreement predicate. Model edits, missing data, real draw motion, ancestry,
visibility and material changes remain invalidating inputs. Extraction still
publishes the current origin and view state on every call.

Baseline capture and exclusive-span attribution preceded implementation. Both
implementation diffs received independent static review with no concrete findings;
a fresh source review after merging `551909a16` also found none. Retained checks
and native after evidence are recorded below.

The traversal changes add no settled candidate walk or source requalification.
This is not a claim of zero total extraction work: merged #2145.2 still polls
copied component ticks to safely admit native change queries, as its own counters
and comments explicitly report.

Independent design review found that choosing one pose role by priority is
insufficient: a primitive may also carry a cell key, and a cell may also carry an
unrelated FormId. The revised snapshot keeps cell validity independent from
literal-root and authored-descendant pose facts. Added oracle regressions cover
both overlaps and a root carrying a cell key. Consecutive exact rebases are checked
without an intervening oracle, including the advancing output origin. Flat tests
also cover metadata-only affine mismatch, repeated nonfinite model/affine writes,
and recovery. All expanded tests passed in the retained engine nextest run.

## Verification

On merged `551909a16` plus the traversal patch, gated `cargo fmt --all -- --check`
and strict `cargo clippy -p engine --all-targets --all-features -- -D warnings`
passed. Gated `cargo nextest run -p engine --all-targets` passed **3,365 tests,
13 skipped**. This includes the original red regressions, all nine source-origin
tests, overlap/enable invalidation and held-ready response cases, #2145.2's copied
tick tests, and its extended per-family native query oracle. Receipts are
`round1-{fmt,clippy-engine,nextest-engine}.log/.exit` under the retained target.

Strict gated workspace Clippy (`--workspace --all-targets --all-features -- -D
warnings`) also passed. The existing `proc-macro-error2 v2.0.1` future
incompatibility notice remains; it is not a Clippy warning in the workspace source.


### Interior fixture investigation during workspace verification

The first workspace nextest attempt ran 5,029 of 5,602 selected tests: 5,028
passed, one failed, and 573 were cancelled/unrun (63 skipped). The failure was
`transition::tests::an_interior_destination_is_one_cell_and_the_gate_reads_it`:
the original negative assertion found interior 99 already resident. An unchanged
focused workspace invocation failed at the same assertion; the earlier engine
suite had passed it. Both failed receipts remain retained. The first attempt is
not reported as a passing workspace check.

At the lead's request, the original assertion was compared before retaining a
fixture change. The original traversal-head unit binary was frozen before any
transition edit. Tracked Rust sources were restored to integration `551909a16`,
then a base workspace-unit binary was built through the gate. Compiler, feature,
profile, dependency, rustflag and target fingerprint inputs match. Both immutable
executables and SHA-256 manifests remain under the retained target.

Thirty adjacent alternating base/head pairs used native nextest, shared Cargo and
binary metadata, the same original executable path and working directory, and no
retries. Base failed **0/30**, traversal head failed **0/30**. Mean sampled system
CPU use was **27.84% / 28.45%**. Direct invocation and native nextest at copied
paths also each observed 0/30 on both variants. These observations do not
reproduce the original failure or establish a pre-existing flake rate. No such
statistical innocence claim is made. Full paired results, per-invocation load,
exit codes and logs are in `round1-fixture-comparison-original-path-nextest/`.

The causal code path is separate from this round's runtime changes. The fixture
installs Minimal, Transform, Asset, Streaming and Transition plugins. It installs
neither ClusterPackagePlugin (which installs FarStockPlugin) nor
ClusterRenderPlugin (which installs source extraction into RenderApp). The new
planner/pending-request and source-origin paths consequently do not execute.
Ordinary streaming can call unchanged far-stock bridge helpers; release/adopt/
revive return without FarStock, while queue_despawn still performs an ordinary
despawn. This distinction does not activate the changed planner.

The original fixture began 200 units from its door, inside the 800-unit prestream
radius, then waited for that door to spawn before asserting the destination had
not loaded. The existing crossing-prefetch test uses the same near pose and
explicitly expects early residency. The retained five-line test-only correction
starts at the existing far fixture pose (1,000 units away), retains the negative
residency assertion, then moves to the original near pose before waiting for the
positive result. It makes the initial condition explicit without altering
production loading policy or weakening either assertion. An independent Codex
review verified this path and the complete small diff without concrete findings;
it retained the statistical proof limit above. Queue row **N28** records the
bounded correction. Final-source verification follows below.


### Final-source gates after the fixture correction

The original five traversal source hashes remain byte-identical to their reviewed
snapshot. `round1-source-hashes.json` additionally records the test-only
`transition.rs` correction. The final sequential checker is
`check2151_final.py`; it uses the same isolated target, one Cargo build job, and
`BUILD_GATE_OWNER=sol-2151`. Every command below ran through the build gate:

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy -p engine --all-targets --all-features -- -D warnings` | Pass |
| `cargo nextest run -p engine --all-targets` | 3,365 passed, 13 skipped; 74.359 s execution |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Pass |
| `cargo nextest run --workspace` | 5,602 passed, 63 skipped; 567.072 s execution |
| `cargo test --workspace --doc` | 18 passed, 15 ignored, zero failures |

The corrected interior test passes in both final suites (0.291 s engine,
1.269 s workspace). Final receipts are
`round1-final-{fmt,clippy-engine,nextest-engine,clippy-workspace,nextest-workspace,doctest-workspace}.log/.exit`.
The earlier failed workspace attempt and original focused failure remain separate;
no test was excluded or retried automatically. Five slow workspace tests are
reported honestly, including a 319.120 s headless streaming fixture. These suite
execution times are not native performance evidence.

The final release build with `--features tracy` passed in 34m42s. The retained
executable is `round1-after-tracy.exe`, frozen at 04:48:55 Adelaide on
2026-10-26, SHA-256
`8e95265a194bfc72ac18caa10d205133ee4c0d509fce4e8a48e9ff6503a80d07`.
Its Tracy dependency enables `only-localhost`. The build reported an interrupted
sccache server and completed compilation locally with the same flags; the final
build and checker exits are zero. `round1-final-binary.json` binds the binary to
the six final Rust source hashes. The hashes were independently rechecked before
capture. No Rust source changes followed the final gates.

## After captures and remaining traversal work

The single after campaign drained the admitted jobs under `timing-2151` after the
lead explicitly swapped the capture order. Its hold began at 04:54, was valid until
06:30, and allowed a 75-minute drain. After the drain, the campaign ran one
discarded full fast warm-up, scored fast, then scored fly. It exited zero and
finally-cleared its hold by 05:24; independent gate status confirmed no
`timing-2151` remained. No second capture campaign or overlapping hold was started.

Both scored runs have quiet guards, complete all 27 waypoints and 117 ground
cells, and receive **CONTENT PASS** from the supplied `verdict.py`. Both report
zero owned-not-drawable errors, zero pending asset instances, zero undrawn static
geometry, and zero undrawn runtime-refused references. No geometry setting,
distance, detail level, loading behavior, or ownership fallback was relaxed.

The following counts cover the engine's **complete post-ready window**, including
the cover tail. They are TEST/Tracy observations, not performance acceptance.

| Route | >33 ms before / after | >100 ms before / after | >200 ms before / after | Engine p99 before / after | Worst before / after |
| --- | ---: | ---: | ---: | ---: | ---: |
| Fly | 82 / 102 | 36 / 2 | 34 / 0 | 31.0283 / 31.9120 ms | 333.3373 / 110.4957 ms |
| Fly-fast | 97 / 98 | 35 / 2 | 34 / 0 | 298.0357 / 69.3352 ms | 337.2758 / 110.3836 ms |

The grader's route CSV window has >33 ms counts of 82 / 97 for fly and 94 / 98
for fast; its >100 and >200 counts match the full-window table. After runs contain
12,046 / 2,287 route frames respectively. Full-window hard cycle identities equal
the exactly joined route hard cycles, so no additional hard frame is omitted
between world-ready and route admission. `round1-summary.json` retains the
cross-checked machine-readable comparison.

Both routes still fail **PERF** and **TRAVERSAL**. Fly retains three one-second
windows above twice its run median p99; fast's relative-window test passes but
still has two hard hitches. Fly's p99 and >33 ms count did not improve in this
single comparison. The stable-p99/no-stall bar remains open. After includes
`551909a16`/#2145.2, while before is `8ae4e5733`; aggregate changes cannot isolate
this patch from that integration change.

The mechanism counters and spans isolate the eliminated work:

| Route / operation on 34 origin changes | Before range / total | After range / total |
| --- | ---: | ---: |
| Fly source extraction | 185.425–226.603 / 6,686.185 ms | 11.052–18.067 / 476.109 ms |
| Fast source extraction | 184.587–226.862 / 6,743.156 ms | 10.693–15.289 / 452.496 ms |
| Fly exclusive desired construction | 44.432–49.931 / 1,598.702 ms | 2.935–6.505 / 134.446 ms |
| Fast exclusive desired construction | 43.331–57.189 / 1,583.814 ms | 2.744–5.752 / 130.938 ms |

Each route still crosses 34 origins. Origin-associated source calls above 100 ms
change from 34 to zero; no source spike above 100 ms remains anywhere on either
route. Initial source work before world-ready remains visible in the full trace.
The run-lifetime desired-candidate total changes from **10,104,549 to 573,941** in
each route. These are actual visits, including initial projection and bounded
work, not a counter of loaded assets. Red/green regressions separately establish
zero added unbounded candidate visits on empty crossings and retained pending and
held-ready responses when targets do not change.

### Every remaining hard frame

The Tracy joins have zero dropped streaming samples and zero unclassified hard
frames. All four hard frames are render-preparation catalog rebuilds, with
exclusive nested timers:

| Route / CSV frame / cycle | Frame | Observer | Catalog | Aggregate |
| --- | ---: | ---: | ---: | ---: |
| Fast / 886 / 3200 | 101.8476 ms | 75.178913 ms | 68.818788 ms | 6.342015 ms |
| Fast / 1286 / 3600 | 110.3836 ms | 74.421330 ms | 68.047085 ms | 6.360095 ms |
| Fly / 4480 / 6052 | 106.0641 ms | 78.597572 ms | 74.529263 ms | 4.050469 ms |
| Fly / 6503 / 8075 | 110.4957 ms | 76.986499 ms | 69.686312 ms | 7.283537 ms |

Their cause is the synchronous world/batch inventory construction over
`scene.instances` in `readiness_bridge::observe`. It dominates the readiness
observer on each exact hard cycle. The frame also contains ordinary main-thread
and render work; the table does not add overlapping spans as exclusive frame
time or blame all latency on one timer. No hard frame is attributed to an asset
upload, stock spawn burst, or desired reconstruction by inference.

The catalog key-field trigger is not instrumented, so the invalidating field is
still unknown. Queue row **N29** records the concrete follow-up: first count which
of GPU identity, scene tick, refusal revision, layout scope and runtime-state
revision changed. Maintain per-world/per-batch inventory contributions at the
scene/refusal/runtime producers, then publish a package/layout/generation-matched
catalog instead of rescanning every instance on the render thread. Preserve
material, GPU, pipeline, view and ownership dependencies. Prove real invalidation,
withdraw/recover and zero settled catalog walks, then repeat both routes. This
removes repeated work rather than spreading it with a budget or delaying content.

### Streaming and log limits

Both after routes end with zero pending placements and no streaming failure. The
37 exact-request queue-full warnings in each run all precede world-ready; none
occurs during traversal. Baselines have the same 37 warnings. This does not prove
every possible long-running request can never stall, but the retained workloads
drain. The earlier I21 fast termination backlog remains an observed backlog,
without proof that an individual placement was permanently stuck.

All four scored logs contain the same one native `SymInitialize FAILED` code-87
message; joined named Tracy spans remain present. All also contain two unresolved
enable-parent warnings and six missing-UV tangent warnings. Fast after additionally
has two startup unknown-asset-dependency warnings; final pending work is zero.
Neither after log contains a Vulkan validation error or a post-ready source full
recovery. These log findings are retained rather than described as an error-free
run. They do not establish complete dataset or save/script parity.

### Native visual proof and scope

Fresh native window enumeration preceded each attempted capture. It omitted an
old peer Windows Security dialog, while the actual screenshot showed the prompt
for `r2-engine-only-after-0ef2bde23.exe`. The authorized Cancel click was refused by
Computer Use because the point belongs to `PickerHost.exe`, not the engine window.
`list_apps` reports PickerHost running but exposes no targetable window. The lead
and user were asked to dismiss it manually; no network permission or firewall
setting was changed.

A separate, unmeasured fast visual TEST ran through `build_gate.py test` after the
three-run campaign had finally-cleared its hold. It acquired no hold and obeyed
the gate. Its screenshot still showed the same peer prompt. The retained evidence
is `round1-after-visual-fly-fast/blocked-visual-01.png`, with fresh window
enumeration, launch/environment and executable hash. The visible terrain outside
the dialog is drawn, but the covered center prevents a clean visual completeness
claim. This is an explicit remaining proof gap, not a passing screenshot. No
screenshot or input action was taken in either scored after run.

This round touches the far-stock planner and the source-origin/hot-fact portions
of `handoff.rs`, including the declarations and test oracles required by merged
#2145.2. It does not take that task's terrain, camera or render command recording
hunks. The only other behavioral-file hunk is the bounded, test-only interior
fixture correction. All Rust gates apply to the exact retained source snapshot.
