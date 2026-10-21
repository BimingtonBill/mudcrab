# Cluster measurements and safe fixes, 2026-10-18

Task base: `212f86f2`, branch `opt/cluster-showcase-integration`, checkout
`D:/Mudcrab/merge/dev-integration`. Scope: reviewed plan A/B only. Distant stock
demand remains unchanged; SkyrimSE object LOD/tree billboard consumption is a separate task.

These are quick-profile **TEST** observations, explicitly requested without a timing hold.
They share the PC, include startup and loading, and are not release performance,
settled FPS, or demo acceptance. Comparisons from one open and one closed run are
inference about causality; directly logged phase durations/counters are observations
of those runs only. Far-terrain shadows are absent from both command lines.

## Reproduction and measurement semantics

The source of the shot set and engine arguments is
`C:/Modding/Mudcrab/builds/vg7-cycle34-fallback-runtime/{process,shots}.json` and
`docs/evidence/cluster-fallback-cycle34-2026-10-18.md`.
The open run retains all three shots. The closed run retains the exact first-shot
camera pose, resolution, FOV, worldspace and graphics, with `open_door: null`.
No deadline, reach, budget or graphics setting changes. Both use the certified
`D:/Mudcrab/SkyrimConverted-vg7-showcase-20261017-certified` dataset.

Owned local artifacts: `local/vg7/measurements-20261018/` (OWNER files).
Both A runs use the same immutable copied executable; its identity and full arguments
are retained alongside its log and requested profile-output directory (no bundle
was emitted by either A run). The build uses only
`CARGO_TARGET_DIR=C:/Modding/Mudcrab/builds/vg7-showcase-integration-target`.

Engine command (run-specific paths/label substituted):

```text
python C:/Users/Brannock/.mudcrab/build-gate/build_gate.py test --owner vg7-cluster --label measure-a-<open|closed> -- <immutable engine.exe> --assets D:/Mudcrab/SkyrimConverted-vg7-showcase-20261017-certified --cluster-package D:/Mudcrab/SkyrimConverted-vg7-showcase-20261017-certified --cluster-terrain --allow-incomplete-assets --graphics showcase --load-doors seamless --shots <shots.json> --shots-out <run dir> --show-window --profile-output <run profile dir> --run-label measure-a-<open|closed> --run-owner vg7-cluster
```

Certification phases are sequential wall durations; envelope subphases are nested
inside `envelope` and must not be added to it. Loader block/mesh microseconds are
nested within payloads/instances. Per-domain tangent busy time sums worker wall
intervals, including contention/waits, and is not single-thread CPU time.

CPU system logs are cumulative inclusive wall time every 120 invocations; subtract
successive totals/counts for an interval. Terrain identity/map conversion is inside
`extract_sources`, and cached terrain identity/clone is inside that segment. Neither
should be added to its parent. Other extraction work remains visible separately in
the existing aggregate extraction profile.

Far-stock logs are cumulative across demand generations. Worker latencies are
recorded when a response is received, including responses later held by commit
budget. `stop_no_budget_slot`/`stop_time_budget` count their actual break;
`stop_nothing_ready` counts natural exhaustion (including idle frames and frames
that consumed the final ready response). The three reasons partition collect calls;
queue/pending snapshots distinguish an idle frame from database waiting.
`committed_cells` counts successful cell commits, not discarded/stale/error replies.

## Step A results

Runtime image SHA256: `b54d6831457bb89cf7c2fbbf22370293262bd3cce9f72ea31d3be11aeaffd56b`.
The exact precommit source patch SHA256 is
`49e7a6f3415661d42943fd2b58496edc266af3df0130f4dfab5f42fc1716f760`;
`identity.json`, `measurement-source.patch`, process arguments, complete logs, shot
logs, full-log audits and the analysis script are retained in the owned local folder.
RTX 3090, Vulkan, driver 616.92; Ryzen 5800X, 31.9 GiB RAM. Gate holds: none.
Open GPU-before snapshot: 3,539 MiB used, 11% utilization; shared-PC TEST limits apply.

### Certification and main-thread loading

Sequential certification phases, **open / closed**, in milliseconds:

- Envelope: **16,556.832 / 14,583.465**.
- Source authority open: **2,598.317 / 2,516.554**.
- Mapping validation: **40,531.100 / 40,545.751**.
- Mapped-reference index: **3,002.155 / 3,694.943** (37,866 records, 35,732 references).
- Examined-reference index: **41,286.947 / 40,505.388** (269,761 references, 8,080 cells).
- Coverage/cell index: **251.102 / 248.364** (6,395 mapped cells).
- Certification total: **104,226.609 / 102,094.676**.

Nested envelope split: base tables/indexes **3,835.151 / 4,017.210 ms**;
base owned files **7,810.814 / 5,553.768 ms** (1,711 files, 2,939,229,249 bytes);
terrain tables **3,838.531 / 3,921.349 ms**; terrain manifest **3.292 / 3.520 ms**
(534,665 bytes); terrain cell-cache hash **531.277 / 523.459 ms** (488,986,104 bytes).
Inference: the source-validation/indexing path is the next certification target;
these phases do not separately attribute their SQL, model-catalog hashing and allocation work.
The first/second run difference is not a measured optimization.

Main-thread loader total **6,098.263 / 7,058.051 ms**:
rows/paths **4.660 / 5.516**; terrain load/validation **1,901.250 / 2,139.342**;
payloads/instances **4,186.984 / 4,907.983**; batching/order **4.857 / 4.643**.
Payload bytes read: **2,774,169,088** in each run; nested mesh read/check time
**4,097.159 / 4,780.486 ms**, block read/parse **62.007 / 100.851 ms**.
Both load 1,230 meshes, 38,126 instances, 1,485 batches and 340 blocks without skips.

Tangent preparation: **34,725.577 / 35,172.773 ms wall** on eight threads;
terrain occupied **269,912.956 / 274,977.173 worker-ms**, objects
**6,410.395 / 4,943.761 worker-ms**. Open terrain share: **97.68%** of summed
worker occupancy. This is the measured baseline for B1, not CPU instruction time.
All 340 blocks reach the GPU and all 260 terrain batches become ready.

### Extraction, readiness proof, occlusion and frame comparison

No profile bundle or PNG was written by either failing shots run, despite
`--profile-output`. Consequently there are no valid quantiles or GPU-pass timings.
The periodic logs are the retained measurements. For an inclusive system mean,
subtract `total_us` snapshots and divide by the `calls` difference. For frame cadence,
subtract timestamps of the once-per-frame extraction log and divide by that same
count difference. This is an **inference of mean frame cadence**, not a saved
`First`-to-`First` frame series or settled performance result.

The matched early windows begin 15 seconds after LOD first draws and end 45 seconds
after it; choose the first and last periodic observations entirely inside each window:

- Open source calls **2400 -> 3840**, **02:22:49.360517 -> 02:23:18.134498 UTC**:
  cadence **19.981931 ms/frame**; extraction **11.755001 ms/call**, comprising
  **5.846453 ms excluding terrain + 5.908549 ms terrain identity/map copy**.
  The nested cached `identities()` portion is **1.784243 ms**.
- Closed source calls **2280 -> 3600**, **02:27:38.183313 -> 02:28:05.605653 UTC**:
  cadence **20.774500 ms/frame**; extraction **12.473284 = 6.258906 + 6.214378 ms**;
  nested `identities()` **1.810504 ms**.
- The other two custom extraction systems are small: frame extraction
  **0.000430 / 0.000482 ms**, batch material extraction **0.000863 / 0.000879 ms**.
- In the corresponding LOD+15..45-second windows, main `swap/reports` costs
  **0.264862 / 0.276126 ms/call**, render `swap/evaluate` **0.074109 / 0.179678 ms/call**.
  Proof call ranges are **600 -> 1920** in both runs; they have separate system clocks,
  so their timestamp endpoints differ slightly from extraction's.

The closed shot has `open_door: null`, no doorway-open event, and no eligible
occlusion calls. **It is not a proven portal-free control:** warm-view proof still
runs during the early window. Source requires an active warm/open portal camera
for that request (`portal/readiness.rs`); prewarming is the source-backed explanation.
After its last 4,680-call proof snapshot, just before certification returns, hundreds
of extraction frames pass without another 120-call proof snapshot. Exact warm-camera
cessation was not instrumented. Background certification, prewarming and differing
near-source populations confound a causal portal-versus-LOD split. The measurements
show no 16 ms improvement merely from keeping this door closed; they do not assign
the shared cost to LOD or establish zero portal cost.

While far stock continues loading after initial cluster readiness, the first-shot
source windows are open **6840 -> 6960**, **02:24:59.867057 -> 02:25:13.815965 UTC**,
and closed **6600 -> 6720**, **02:29:51.320043 -> 02:30:06.325962 UTC**:

- Inferred cadence **116.240900 / 125.049325 ms/frame**; source extraction
  **89.493764 / 99.366538 ms/call**.
- Excluding terrain **44.292012 / 46.974727 ms**; terrain identity/copy
  **45.201753 / 52.391810 ms**; nested `identities()` **41.676318 / 48.808011 ms**.
- Open main proof **0.282070 ms**, render proof **0.053193 ms**, measured over proof
  calls **4920 -> 5040** within the first-shot readiness window.

These loading windows have growing stock populations and are not a settled A/B.
The extraction attribution is now measured; its old 28 ms aggregate must not be
assigned entirely to non-terrain source work.

Open doorway lifetime: **1,286 actual occlusion tests**, rounded mean
**1,038 us/test**, max **1,871 us**, **zero frames saved**. Last exact cumulative
snapshot: **5,040 eligible frames, 1,260 tests, 1,299,229 us total**, zero skipped;
that is **0.258 ms of test time per eligible frame**, including reused decisions.
The measured inclusive occlusion system means are **0.261440 ms/eligible frame**
in the early window and **0.314498 ms** after initial readiness. B4 therefore uses
the cheaper conservative main-view candidate list, retaining occlusion functionality.

### Far-stock throughput and stop reasons

Demand remains the reviewed 91-cell terrain reach. Initial desired/resident counts
are **198,126 / 3,159 open**, **198,034 / 1,864 closed**; these are current-head
observations, not the older cycle34 demand counts.

Matched first-shot snapshots near 61 seconds after far collection starts:

- Open **02:25:20.027650 UTC**, elapsed **61,066 ms**, **357 collect calls**,
  **356 responses/committed cells**, **15,118 references spawned**:
  **5.830 cells/s**, **247.568 refs/s**. Worker means: query **3.363809 ms**,
  queue wait **0.013219 ms**, total request **3.377649 ms**.
  Queue **6,242 -> 5,886**, pending **1**, failed **0**.
- Closed **02:30:06.012387 UTC**, elapsed **61,010 ms**, **378 collect calls**,
  **377 responses/committed cells**, **16,301 references spawned**:
  **6.179 cells/s**, **267.186 refs/s**. Worker means: query **2.863714 ms**,
  queue wait **0.012889 ms**, total request **2.877194 ms**.
  Queue **6,266 -> 5,889**, pending **1**, failed **0**.
- No-budget stops **0 / 0**; time-budget stops **0 / 0**; natural/no-more-ready
  exhaustion **357 / 378**. All logged snapshots satisfy `collect_calls = sum(stops)`.
  Natural exhaustion includes a frame that committed its final ready response;
  it does not mean the database starved those frames. Almost every collect call commits
  one cell and leaves one new query in flight.

The open run's later two far poses finish at a last periodic snapshot of **608 cells,
24,419 references in 122.571 s**, queue **5,634**, pending one, failed zero.
Inference: worker SQL/queue latency and budget starvation do not explain the observed
low throughput in these runs; growing frame cost and one-query-per-frame admission
are the mechanisms consistent with the counters. Demand/admission is unchanged here.

### Complete-log audit and acceptance limits

Read/audited all **1,028 open** and **887 closed** log lines, including all cumulative
series and readiness/pipeline arrays. CPU totals are monotonic; stop counts partition
collect calls. Both naturally exit **1**. Open's three and closed's one shot fail the
original 30-second readiness limit; no screenshots exist. First-shot queued far counts
are **5,863 in both**, pending one, zero failed far sources; pending assets are
**2,132 open / 2,151 closed**. The open later shots leave **5,740 / 5,617** queued.
There are no ERROR-level messages or reported cluster/pipeline failures.

Each run logs the existing 60-second startup pacing expiry, four ordinary-object
GLTF tangent warnings for missing UV attributes, and one async render-statistics
buffer download warning during teardown. The four object models are retained in
full logs (Helgen rubble, Civil War rubble, Whiterun scaffold); object behavior stays
unchanged. Source fallback qualification later requests one ordinary source; it is
not silently treated as a successful cluster template.

A checks: gated fmt passes; engine all-target/all-feature quick clippy passes;
engine all-target quick nextest **2,044 passed, eight existing skips**; normal quick
build passes. The first unrestricted parallel test build exhausted compiler memory;
serialized compilation (`CARGO_BUILD_JOBS=1`) passes without source/test waivers.
Logs: `local/vg7/measure-a-{fmt,clippy-fixed,nextest-serial,build}.log`.


## Step B validation

All safe fixes were applied after A's two TEST runs, one local commit each:

| Fix | Commit | Preserved contract and regression proof |
| --- | --- | --- |
| B1, unused terrain tangents | `700dc475` | Only exclusively Terrain mesh IDs skip MikkTSpace. Object, Fixture and shared IDs retain generation. Unused slots retain their diagnostic words and do not claim usable tangent proof. Three tests cover the colour-only material contract, cached slots and shared-domain exclusion. Filling slots remains inside the measured worker interval. |
| B2, far-stock bookkeeping | `d8d86be6` | Queued membership makes `cell_state` O(1); root and near-cell indexes replace global owner scans. Every lifecycle mutation maintains the indexes. Failed owners keep their first diagnostic instead of formatting it each frame. Four new tests plus lifecycle index-oracle checks cover parent/near-root identity, cross-world keys, revive/release and failure reset. Demand, submission and commit pacing stay unchanged. |
| B3, terrain diagnostics | `3c9bed2c` | `note_terrain_views` scans/allocations and its log run only when DEBUG is enabled. Required terrain readiness computation/publication remains outside that guard. This logging-only branch needs no duplicate implementation test. |
| B4, cheaper doorway test | `897c9f02` | A observed zero skipped frames for 1,286 tests, averaging 1,038 microseconds, so the condition for this fix is met. Retain exact ray tests but limit candidates to the previous main-view visible mesh list. Current visibility/layer/ancestry/material checks still apply; missing, omitted or stale candidates only leave the view drawn. Four actual-system tests cover a listed blocker, omitted blockers, missing/empty lists and stale/hidden entities. |

B4 retains the existing slab/ray algorithm unchanged. It avoids the all-world query
scan instead of removing useful occlusion in other scenes. Its candidate count is
logged and exposed as a profiler gauge. Independent reviewers approved the complete
kept source diff, including the final query alias and formatted tests, with the
foundations/mod/worldspace questions considered. No baked content IDs were added.

### Required checks

All cargo commands use the build gate and the existing session target, quick
profile, serialized compilation (`CARGO_BUILD_JOBS=1`). Each fix independently passes
`cargo fmt --all -- --check`, engine all-target/all-feature clippy with `-D warnings`,
and engine all-target nextest:

| Source change | Engine tests passed | Existing skips | Valid clippy / nextest logs under `local/vg7/` |
| --- | ---: | ---: | --- |
| B1 | 2,047 | 8 | `b1-clippy-fixed.log`, `b1-nextest.log` |
| B2 | 2,051 | 8 | `b2-clippy-fresh.log`, `b2-nextest.log` |
| B3 | 2,051 | 8 | `b3-clippy.log`, `b3-nextest.log` |
| B4 | 2,055 | 8 | `b4-clippy-fixed.log`, `b4-nextest.log` |

Final integration checks at source `897c9f02`:

- Workspace all-target/all-feature clippy, `-D warnings`: **pass**.
- Workspace nextest: **3,820 passed, 50 existing skips**.
- Workspace doctests: **pass, four existing ignored examples; no runnable examples**.
- Normal default-feature, non-Tracy quick engine build: **pass**.

Logs: `final-workspace-{clippy,nextest,doc}.log`, `final-build.log`.
The dependency future-incompatibility notice for `proc-macro-error2` remains; no
warning allowance was added. Initial draft compile/lint errors were corrected before
these valid checks. B2's preserved draft timestamp initially allowed a cached check;
source timestamps were refreshed and the explicitly named fresh clippy actually
checked engine. All new tests are present in their successful nextest logs.


### Final source TEST (all B fixes)

Source **`897c9f02b24635347559e921f94e3f3d0241d10d`**, normal quick image SHA256
**`2581f35003b216141ad08722afe82fd8b9ff64de9b722281a83184d6572fdb5b`**.
`local/vg7/measurements-20261018/post-fixes/` retains the immutable executable,
`identity.json`, exact gate/engine `command.json`, original three-shot `shots.json`,
GPU-before snapshot, complete log, shots log, exit and full-log audit. Same certified
dataset, Vulkan RTX 3090, Showcase, dimensions/poses and optional far shadows OFF;
no limits or settings changed. No timing hold or FPS acceptance claim.

Inputs agree with A open: **1,230 meshes, 38,126 instances, 1,485 batches, 340 blocks,
2,774,169,088 payload bytes, zero skips**; initial far demand **198,126**, already
resident **3,159**, queued cells **6,242**, pending **one**. All 340 blocks reach the
GPU and terrain readiness reaches **260/260**. The DEBUG per-view diagnostic emits
no rows at the normal log level; required readiness reports remain present.

- B1 skips generation for **260 exclusively terrain mesh IDs**. All-block preparation:
  **820.710 ms wall**, **308.559 terrain worker-ms**, **6,134.113 object worker-ms**,
  eight threads. A open was **34,725.577 wall**, **269,912.956 terrain**, **6,410.395
  object worker-ms**. These observations confirm the removed terrain work and retained
  object work; the difference is not an isolated end-to-end benchmark or FPS result.
- B4: **1,286 tests, rounded mean 190 microseconds, max 940 microseconds, zero skipped
  frames**, versus A's **1,038 mean / 1,871 max**, also 1,286 tests and zero skipped
  frames. Periodic exact totals at 5,040 eligible frames/1,260 tests are **235,137 us**
  versus A **1,299,229 us**, or **0.046654 versus 0.257784 ms per eligible frame**.
  Logged candidate-list sizes range **1,646–5,609**. The narrower input is measurably
  cheaper in these TEST observations; exact predicates and confirmation cadence stay.
- Certification **99,589.330 ms**, main-thread loader **5,432.127 ms**; phases and
  nested counters remain in the audit. No certification optimization was implemented,
  so these differences from A must not be attributed to B.
- Remaining extraction is substantial: source calls **7080 -> 7200**, UTC
  **03:31:47.940016 -> 03:32:01.600370**, average **89.406172 ms** = **43.325418
  excluding terrain + 46.080753 terrain identity/copy**; nested identities **42.370400
  ms**. The timestamp-derived cadence is an **inference**, **113.836283 ms/frame**,
  while loading; it is not a settled frame-time result or a causal comparison with A.
  Post-ready proof calls 4920 -> 5040 give `swap/reports` **0.292467 ms** and
  `swap/evaluate` **0.054294 ms**. Whole occlusion-system cost for its own calls
  **4920 -> 5040**, UTC **03:31:40.742577 -> 03:31:53.819306**, is **0.067730 ms**
  (including untested cadence frames and bookkeeping, not just actual ray tests).
  These proof/occlusion intervals differ from the extraction interval above.
- Last far snapshot at **101.828 s**: **600 cells / 24,128 references committed**, queue
  **5,642**, pending **one**, failed **zero**. No-budget/time stops remain **0/0**;
  natural-loop exhaustion **601/601 collect calls**. Query/wait/request totals are
  **815,873 / 6,149 / 822,357 microseconds** for 600 responses. No demand, admission,
  or commit-rate behavior was changed; these counts are not a B2 throughput claim.

Read/audited all **810 log lines**, including every numeric series/readiness array,
the footer and all shot failures. CPU cumulative totals are monotonic and all far
stop counts partition collect calls. Six warnings are the same startup pacing expiry,
four ordinary-object UV/tangent warnings and teardown buffer-map warning seen in A.
No ERROR-level message or failed cluster/LOD/far counter is reported. Per-view
pipeline failure columns are unavailable with DEBUG disabled; the expected failure footer
reports exit 1. The later ordinary-source qualification request is still explicit.

The process **exits 1 naturally**: all three original 30-second readiness windows
fail. Queued far cells at those failures are **5,907 / 5,766 / 5,638**; pending assets
**2,300 / 2,284 / 2,376**, with one far query pending and zero failed far sources.
No PNG or profile bundle is produced. One to three terrain materials were temporarily
unprepared during early upload and then resolved; no missing-pipeline or phase-drop
walk counts occurred. Earlier initial readiness changes the amount
of far work admitted before the shot deadline, so failure snapshots are not matched
population benchmarks. This run proves neither visual parity nor demo acceptance;
the large far backlog and extraction cost remain visible without a relaxed deadline.

## Recommended next work

Keep the separately agreed SkyrimSE object-LOD/tree-billboard representation task
separate from these behavior-preserving changes. It addresses the full-model distant
workload without lowering quality/reach or bypassing readiness. Certification's
mapping/examined-index phases and extraction's repeated identity/copy work also need
root-cause work at their own layers. After that work, repeat a settled control with
closed-door prewarming explicitly observed/finished before attributing portal versus
LOD cost. No launcher acceptance, merge, push, PR or external publication occurred.
Owned local raw evidence/drafts are retained for the coordinating thread; no owned
expired Class A folders or extra target/worktree were created in this task.
