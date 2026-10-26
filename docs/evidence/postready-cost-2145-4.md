# Standing CPU round #2145.4

Status: Step 1 quiet measurements and required gates complete. Step 2 instrumentation remains staged only; its later campaign does not block the measured implementation commit.

## Identity and constraints

Base: `94d8722a0b116260e6359f1f31d25c192541a60a`, branch `phase2/cluster-postready-cost`, worktree `D:/Mudcrab/worktrees/cluster-postready`. All builds and runs use the isolated target `C:/MudcrabBuilds/cluster-postready-target` and gate owner `sol-2145`. No push, ECS dependency patch, quality reduction or content cap is authorized by this work.

The requested I22 profiler-free readings (122.7/120.9 FPS cluster1, 124.1 FPS main28) belong to integration. The earlier R3 8.0623 ms render schedule, 0.7674 ms source-query zone and 1,213,859 copied tick rows predate the integrated GPU reuse work. They are hypothesis inputs, not measurements of this base.

The R4 baseline Tracy release completed after a lead-requested memory cancellation and cached resume. The successful resumed build took 29m 48s. Its frozen executable SHA-256 is `c64c2faf2d931f29693d48a993730623af3c6312146715340a8e453856d64036`; `r4-baseline-build-snapshot.json` verifies all 894 source/manifest hashes and retains both build attempts. Cancellation receipts and the original log remain preserved. No candidate Rust edits preceded baseline freezing.

## Root cause and selected correction

The existing engine precheck enumerates every World table for each component, reconstructs structural filter state, copies ticks through an unsafe storage accessor and ignores native default disablers. Hot and Terrain independently repeat the two visibility probes. Native queries subsequently apply their authoritative policies.

Bevy 0.19 already exposes the responsible abstraction: `Query::contiguous_iter()` iterates native matched tables, and `ContiguousRef` provides a safe changed-tick slice and the native consumer window. A private `ComponentChangeQuery<T, F>` owns the unchanged native conjunctive `Changed<T>` query. Admission borrows a read-only `Ref<T>` lens from that same query, preserving its captured filter policy and consumer window. The macro retains each declaration's exact domain and additional structural predicates.

The first compile exposed an API constraint missed by static review: `Allow<T>` is archetypal but does not implement the `ArchetypeFilter` marker required by direct contiguous iteration. The revised bounded approach uses a safe read-only native query lens to erase the filter type while retaining its native matched storage, captured access/policy, density and window. A non-archetypal filter falls back before erasure. Lens conversion clones query metadata on each probe, so paired measurement must include that cost. No custom WorldQuery implementation, tautological filter expansion or ECS patch is introduced.

Runtime non-contiguous scopes admit the original query and copy zero ticks. This includes sparse components, sparse predicates and sparse default disablers. Dense scopes read fresh tick slices, retain the first-row native positive shortcut, then reduce the remainder's minimum wrapping age. For reader age `W <= MAX`, `W > min(A, MAX)` is equivalent to comparing `W` with the minimum age clamped at MAX. Safe tick rollback and wrap remain observable because no summary survives a call.

Hot visibility permits a superset of Terrain's disablers and adds no visibility scope. Its negative raw component probe therefore excludes Terrain changes; its positive probe conservatively admits Terrain's own native query. Only those two same-extraction decisions are shared. Filtered entities and semantic `hot_changed` remain separate. `visibility_probe_reuses` counts two only when Terrain actually consumes them. It is not an avoided-row estimate.

The change retains population polling. `tick_rows_checked` counts the ticks read, including a complete remainder on a late positive. There is no zero-polling or mutation-feed claim. Captured default policies retain Bevy 0.19's documented late-registration behavior; a new query captures new defaults, while an existing query retains its original defaults.

## Other investigated owners

Static ownership research rejects whole-frame submission skipping and an unchanged-camera early return from native transparent preparation. Submission includes ordered encoder finishing and queue submission; it is not a geometry-upload measurement. Encoder finishes already overlap one another. Queue-time asynchronous dispatch occurs during deferred application after the same camera's recording, so it cannot hide finishing behind that camera's already-recorded postprocessing.

Sorted preparation still owns current ordered membership, pipelines/draw IDs, instance indices, material groups, allocator/lightmap slabs, batch eligibility and frame-local output offsets. GPU-mutated indirect counters require resets. Existing shadow/cull reuse does not jointly certify these inputs. No new command or sorted-recipe cache is selected without a complete owner certificate and measured benefit.

The complete SourceIndex plus lifecycle subjects and full recovery could bound observation safely; certificate membership alone cannot. The retained earlier scene indexed 98,038 nodes among 112,278 entities. Random entity lookups are not evidence of cheaper work than native contiguous slices, so no index-scoped rewrite is selected here.

Bevy's default `wgpu=error` filter explains why global `RUST_LOG=info` did not expose native queue scopes. Separate count captures use `RUST_LOG=info,wgpu_core=info,wgpu_hal=info`; the primary pair preserves its original logging. These scopes count API calls and durations, not upload payload bytes. The profiling guide records this distinction.

## Proof and measurements pending

The first staged implementation had static review with no correctness findings, but failed compilation on the Allow marker and missing generic lifetime bounds. The real-macro baseline work reds were observed exactly: duplicate visibility reads are 4,096 against the required 2,048; disabled-table reads are 4,100 against the required 2. The other 30 original tests passed. The final single-query native module and extracted macro pass **21/21 tests**: 18 native differential cases and three macro cases. The compiler log is clean. Receipts: `r4-native-proof.json`, `r4-native-proof-{original,native}-{compile,tests}.log`; the failed direct approach is retained in `r4-native-direct-compile-failed.log`.

The old broad helper and its 29 implementation-specific tests are replaced by the native differential cases. The replacement covers raw/equal/query/bypassed writes, first/late positives and exact work counts, safe rollback, wrapping/expired ticks, sparse component/predicate/default-disabler fallback, sparse Allow, non-archetypal predicate fallback, captured late-disabler policies, fresh tables, migration, swap removal and independent consumer windows. Three production extraction regressions additionally exercise shared visibility accounting, terrain-only visibility/default-disabler removal/restoration against the full oracle, and disabled-table exclusion. Their production runtime results are recorded in the final verification section below.

Independent review of the complete final two-file diff found no actionable findings and verified the native safe-transmute access/density/window contract. Reviewed hashes remain `handoff.rs=874da1b47eac8c2f3c257f96471a2834d21e9a15756fd6ad358308f2cdbe4932`, `source_tick_probe.rs=ac50cd99152e15695594aa4c64e838619de5a3f805dabf2cbdac6bc85ac121f4`. Fmt and diff checks pass. Strict engine Clippy with all targets and features passed in 4m 07s (`r4-engine-clippy.log`, exit 0). This was the pre-build checkpoint; frozen releases and native measurement results are recorded below. Final workspace runtime verification follows.

At the lead's release-admission steer, verified unadmitted own gate PID 50552 was stopped before Cargo started. Its parent chain and status receipt are retained in `r4-after-build-cancelled-before-2153-min.json`; the unused build log and source snapshot are archived under `r4-after-before-2153-admission-*`. The bounded release controller then observed `timing-2153-gpu` set at 11:52:31, recorded the exact status/hold in `r4-2153-window-observed.json`, and queued the sequential Tracy/plain builds behind that window. This does not grant R4 capture admission.

The lead subsequently changed admission to whichever task has both frozen binaries first. Capture still requires the exact `2145.4 WINDOW GO`; binary readiness or another owner's hold clearance does not supply authorization. The bounded watcher was restarted at 12:10 with a fresh three-hour deadline. One bounded hold must cover all R4 captures and clear after verified descendant cleanup. Warm-ups are discarded. The campaign preserves the main28 package, Tamriel pose `(20500,-45500,6000)`, yaw 0, Vulkan, 1600×900, settings and content identity. It includes paired Tracy attribution, separate native count runs and one non-Tracy release standing TEST. This was the pre-capture checkpoint; the completed campaign is recorded below.

The lead's crossing steer adds paired, unchanged `scripts/routes/fly-fast.json` runs using the same frozen binaries and main28 inputs. Crossing-frame source-query duration must be joined to recorded origin-grid changes, separately from ordinary moving frames and standing tails. The complete route has 27 waypoints and 129,795 units at 4,000 units/second. Each route is bounded at 90 seconds and must complete. To retain one 25-minute hold, standing primary runs are 120 seconds, count runs 45 seconds, and the plain standing run 150 seconds. Three discarded 90-second warm-ups remain. Ten runs retain the original 930-second nominal engine budget; startup/save and cleanup allowances require at least 1,410 seconds remaining before launch.

Source inspection establishes that the existing `cluster::sources::change_queries` span includes journal reading/deduplication, identity and hot-fact semantic comparisons, missed-rebase cell checks, and the complete `SourceIndex::update`. It is therefore not a timer for tick polling alone. The index already deduplicates changed subjects and expands retained dependents incrementally. Every selected dirty subject still removes and rebuilds its ancestry and reference edges, then closes affected references over surviving roots and meshes. Crossing costs cannot be attributed to new entities or native query scanning without the joined work counters. A possible later ownership split would retain topology edges for semantic-only changes while rebuilding them for identity/lifecycle changes; that requires a separate completeness proof. Preparing immutable lineage before spawning may move preparation work, but cannot certify live entity identity, arbitrary parent edits, duplicate roots or consumer ticks in advance. Neither change is implemented or claimed here.

Independent campaign review exposed preflight and lifecycle gaps before any run. All three frozen receipts and binaries now validate before hold acquisition, bound to the reviewed two-file hashes, exact base, feature commands and campaign-specific GO. The original no-active-jobs acquisition had a 45-second subprocess bound and checked both build and TEST slots; the lead explicitly replaced that acquisition policy below. Native Windows job ownership contains each guard/engine/recorder subtree, including orphaned descendants. Two harmless native fixtures verified cleanup of active and root-exited three-process trees. Recorder failure or missing output rejects a capture. Unexpected engine exits reject the campaign; exit 1 is permitted only when the sole false acceptance threshold is the retained startup commit-budget condition. Analysis records all grader results and exits nonzero on a failing verdict. It requires the report/shutdown marker and omits the unjoined CSV tail companion rather than presenting it as an exact interval statistic.

Both candidate binaries subsequently froze: Tracy at 12:35:32 after 29m 48s, SHA-256 `c97b8cd642afb187f8e8d4fbea95e674a2c1d5a4be99489ee44afc2a5aaea901`; plain at 13:13:20 after 37m 15s, SHA-256 `9f351fb6df4a3517684c3d42854e002fa75522123a13c9fb1f24b49a7b4ef812`. The plain Cargo fingerprint records `features: []`. Readiness verification matched all 894 source hashes and all three frozen executables; only the two reviewed Rust files differ between baseline and candidate. The bounded watcher survived the provider-capacity interruption and was not restarted again.

The lead granted exact `2145.4 WINDOW GO` and explicitly changed acquisition to arm immediately with `--drain --drain-minutes 20`, with an honest expiry covering drain plus the 1,410-second campaign budget. This supersedes the earlier 25-minute total-hold limit for this campaign. The hold was set at 13:15:44 with expiry 14:00. Its gate subprocess is bounded at 1,245 seconds, including at most 30 seconds of disk-quiet waiting after the 20-minute build drain. Captures retain their unchanged budget and verify the remaining expiry before every run. No engine starts until the drain and remaining-budget checks pass. This entry records admission, not successful completion.

The lead also split completion into two steps: commit the current measured candidate after inclusive crossing attribution and green gates; then add identical four-part Tracy instrumentation to fresh baseline/candidate builds and capture that split in a later window. Step 2 must not block the Step 1 commit. Scratch staging adds only 16 Tracy-gated lines to each source variant. Independent review approved the journal, semantic, origin and index-update boundaries, with parent-minus-parts explicitly retaining structural/recovery/bookkeeping and instrumentation residual. Production source remains unchanged by that staging.

Fresh interval analysis must join extraction E_n to render R_n and compare R_n with Main_n+1. The extraction barrier is serial; overlapping worker, main and encoder durations must not be added. Prior evidence suggests render-side dominance, but it does not establish current ownership. Profiler-free post-ready frame times determine FPS acceptance; startup-inclusive averages and Tracy FPS do not.

## Step 1 quiet results

All ten guards reported quiet. Both fly-fast routes completed 27 waypoints and read 117 ground cells. All Tracy recorders exited 0 with nonempty captures. The three warm-ups are discarded. Native job accounting verified zero surviving owned processes after every run. The single hold finally cleared before 13:36:26, about 20m 42s after acquisition; no export or Cargo ran inside it. Scene/package/route input identities stayed unchanged. The retained standing pair and plain run each contain 112,278 entities, 35,713 meshes, 29,116 transform instances and 25 resident cells, with identical refusal counts and no undrawn static/runtime-refused geometry or streaming failures.

The primary comparison uses fully bounded CPU/GPU intervals in the final 30 seconds before the final `collect_and_finish` report. Both windows begin more than 64 seconds after estimated world readiness. Baseline contains 3,193 complete render schedules; candidate contains 3,379. Nested or parallel spans are not additive. Individual phase counts can differ by one at a window boundary; exact overlap joins and per-submission analysis retain their own validated pair counts.

| Diagnostic span | Before ms | After ms | Delta ms |
| --- | ---: | ---: | ---: |
| Render schedule | 7.7699 | 7.6583 | -0.1116 |
| Source extraction system | 1.0708 | 0.6692 | -0.4016 |
| Source change-query zone | 0.7430 | 0.4628 | -0.2802 |
| Pending command submission | 2.5512 | 2.5392 | -0.0120 |
| Transparent preparation | 0.8541 | 0.8414 | -0.0128 |
| Transparent recording | 0.2701 | 0.2669 | -0.0032 |
| Queue submission scopes | 0.9221 | 0.9188 | -0.0033 |
| GPU pass sum, not critical path | 6.0083 | 6.0453 | +0.0370 |

Literal quiet tick reads decrease from **1,213,859 to 996,853 per extraction**, a reduction of 217,006. Candidate records two consumed visibility-probe reuses. All four native-admission groups remain zero in the retained quiet samples. The remaining 996,853 reads and per-probe native metadata clones are real work. This does not establish zero settled population polling. Source extraction also improves outside the changed query zone; unmodified systems move slightly, so the entire frame or extraction delta is not attributed to this one correction from a single pair.

### Main/render overlap

The exact extraction/render/Main join accepted 3,191 baseline and 3,378 candidate pairs with no rejected ordering pairs. Extraction E_n precedes render R_n; R_n overlaps Main_n+1. The render thread remains the standing critical-path inference in these captures, with extraction as the serial barrier. Main-world work can still contend for shared CPU resources; this result does not make such work free.

| Joined interval | Before ms | After ms |
| --- | ---: | ---: |
| Handoff period | 9.3954 | 8.8773 |
| Main schedule | 4.3649 | 4.2610 |
| RenderApp update | 7.7993 | 7.6872 |
| Main/render intersection | 4.3649 | 4.2609 |
| Render tail when next handoff enters | 3.3026 | 3.2938 |
| Extraction schedule | 1.5357 | 1.1306 |

The 0.5182 ms handoff-period change is consistent with the combined render/update and serial-extraction changes. Almost all Main time overlaps rendering; adding Main and RenderApp durations would misstate frame latency.

### What remains submitted

Every retained pending submission contains **24 command buffers**. Candidate per-submission means divide the 2.5385 ms parent into **1.6184 ms finish/join**, **0.9185 ms queue submission**, and **0.0015 ms following bookkeeping**. The encoders already finish concurrently. Early prepass finishes last in 66.15% of candidate submissions, opaque in 25.80%, transparent in 7.72%, and late prepass in 0.33%. Therefore, transparent finish's 1.1495 ms per-frame inclusive mean is not an independent additive saving target. Its work overlaps the other finishes; early-prepass/opaque completion usually controls the join.

Recorded owners still encode prepass, opaque and transparent drawing, native preprocessing/bin unpacking, the remaining rendered shadow cascade, SSAO, bloom, TAA, tonemapping, upscaling and UI. Existing cluster/shadow reuse removes eligible output work without certifying all native phase inputs or the complete camera frame. No whole-frame or transparent-preparation skip was introduced.

Separate count tails observe **161 versus 157 `Queue::write_buffer` calls per render frame**, seven staging writes/allocations per frame, approximately two queue submissions per frame, and no observed texture-write scope. These are calls, not payload-byte measurements or proof of resident-geometry uploads. Their 10-second windows begin only about 10 seconds after readiness and still contain some rendering of cascades 0–2, unlike the primary late window where those cascades are reused. They are explicitly **early post-ready diagnostics**, not a settled zero-upload acceptance test. Owner attribution is separate research; a later long-tail count capture can resolve steady ownership. Neither the four-call difference nor count-run CPU times are attributed to the source-query correction.

### Exact crossing comparison

`r4_crossings.py` derives source-frame ownership from complete native root ordinals and shared frame IDs, validates every incoming/outgoing clock relationship, and uniquely matches the full route CSV. Crossings come only from consecutive recorded worldspace/origin-grid values. It includes the first unscored moving frame. Both captures contain 34 crossings and all 34 directed edges match by occurrence. This narrower source-frame proof does not certify the newer #2151 cover/admission protocol or render-paced hitch causality. Different sampled positions and live entity work can remain even on a matched edge.

| Source-frame population/metric | Before ms | After ms | Delta ms |
| --- | ---: | ---: | ---: |
| Crossing change-query zone, mean | 10.7278 | 9.9226 | -0.8052 |
| Crossing change-query zone, median | 10.7580 | 9.9897 | -0.7682 |
| Crossing change-query zone, maximum | 12.5570 | 11.4202 | -1.1368 |
| Crossing whole source extraction, mean | 13.0643 | 12.2009 | -0.8635 |
| Ordinary moving change-query zone, mean | 0.7998 | 0.5259 | -0.2739 |
| Ordinary moving source extraction, mean | 1.2715 | 0.8725 | -0.3990 |

Crossing query-zone ranges are 9.8567–12.5570 ms before and 8.7694–11.4202 ms after. Individual matched-edge deltas range from -2.5756 to +0.5295 ms. Route post-ready >33 ms counts are 41 versus 42, >50 ms counts 31 versus 29, and p99 54.3750 versus 50.6014 ms. This candidate does not remove crossing hitches. The remaining roughly 10 ms inclusive zone cannot yet be classified as pure native query cost or per-new-entity work. Step 2's four inner spans provide that missing split.

### Profiler-free standing TEST

The plain release was warmed and run at the same standing pose without Tracy. Its last 16,102 post-ready CSV samples give **127.1432 FPS**, mean **7.8651 ms**, p99 **9.5747 ms**, worst **16.1170 ms**, and zero frames above 33 ms. The grader reports CONTENT/PERF/OVERALL PASS, traversal N/A, and stable one-second windows. This clears the 120 FPS bar and misses the 140 FPS goal. There is no paired plain baseline in this campaign, so no normal-FPS improvement is claimed against I22.

Both diagnostic standing graders report CONTENT PASS and PERF FAIL at 106.87 and 112.29 FPS, respectively; those include Tracy overhead. The analysis command deliberately exits 1 after preserving all reports because those diagnostic performance verdicts fail; `r4-analysis-complete.json` records successful exports and grader exits `{baseline: 1, after: 1, normal: 0}`. This is not an exporter failure or a green runtime assertion. Every engine run retains exit 1 only for the known startup `commit_budget_respected=false` threshold; no other false acceptance threshold was permitted by the campaign. The 7.6583 ms diagnostic render schedule still misses the requested 7.0 ms target.

All exact data, per-edge ledgers, guard/process receipts and analysis limits remain under `C:/MudcrabBuilds/cluster-postready-target`, particularly `r4-comparison.json`, `r4-route-attribution/crossings-report.json`, the three frozen build receipts, and each immutable run directory. Required runtime gates subsequently passed; see final verification below.


## Merge surface and remaining proof

The gate is generated from declarations, with no hard-coded exception for flat facts. At this base, `SourceFactQueries` contains exactly `cell: CellRef`, `scene: WorldAssetRoot`, `original_scene: OriginalStockScene`, `handle: MeshHandle`, `mesh: Mesh3d`, `primitive: SourcePrimitive`, `primitive_mesh: SourcePrimitiveMesh`, `primitive_transform: SourcePrimitiveTransform`, `static_model: SourceStaticModel`, `node_pose: SourceNodePose`, `cell_key: StreamedCellKey`, `extras: bevy::gltf::GltfExtras`, and `retired: crate::streaming::RetireHidden`. Each gets its declared `SourceObservationDomain` plus conjunctive native `Changed<T>`. Flat primitive and flat-only global pose remain in `HotFactQueries`; no HotFacts comparison or origin-rebase semantics are edited. The only cross-group reuse depends on the explicitly documented visibility-domain superset, not this raw-field inventory.

The optional upload-system ownership export refused its 8-million streamed-row/line/byte bound before producing a certified report. The refusal and partial output remain retained under `r4-route-attribution`; no subsampling or raised limit was used. Twenty-six small synthetic checks establish helper behavior, not actual upload ownership or payload bytes. Independent helper review also found that rejected interval candidates can escape its comparison-work bound; the unused helper needs that correction before any later use. Neither issue blocks the measured two-file correction.


## Final Step 1 verification

Gated fmt, strict engine/workspace all-target/all-feature Clippy, workspace all-target nextest and workspace doctests passed. Nextest: Summary [ 493.844s] 5662 tests run: 5662 passed (9 slow), 65 skipped. Doctests: 18 passed, 15 ignored, zero failures. All three new production extraction regressions passed; 894 frozen source/manifest hashes match. Focused native proof passed 21/21 after exact work-count reds. Independent final source review found no actionable defect. Existing proc-macro-error2 future-incompatibility notice remains.

Exact receipts: `r4-step1-final-verification.json`, `r4-gates-green.json`, and the corresponding immutable logs under the isolated target. No full runtime test or performance result is inferred from source review alone.
